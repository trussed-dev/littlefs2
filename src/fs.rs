/*! Filesystem manipulation operations.

Start with the documentation for [`File`](struct.File.html) and [`Filesystem`](struct.Filesystem.html).

Then peek at [`ReadDirWith`](struct.ReadDirWith.html)'s example.
*/
use core::{cmp, marker::PhantomData, mem, slice};

use crate::{
    driver,
    io::{self, Error, Result},
    path::{Filename, Path},
};

// use aligned::{A4, Aligned};

use bitflags::bitflags;
use littlefs2_sys as ll;

use generic_array::{typenum::marker_traits::Unsigned as _, GenericArray};

/// The three global buffers used by LittleFS
// #[derive(Debug)]
struct Buffers<Storage: driver::Storage> {
    read: GenericArray<u8, Storage::CACHE_SIZE>,
    write: GenericArray<u8, Storage::CACHE_SIZE>,
    // must be 4-byte aligned, hence the `u32`s
    lookahead: GenericArray<u32, Storage::LOOKAHEADWORDS_SIZE>,
}

/// The state of a `Filesystem`. Pre-allocate with `Filesystem::allocate`.
// #[derive(Debug)]
pub struct FilesystemAllocation<Storage: driver::Storage> {
    buffers: Buffers<Storage>,
    pub(crate) state: ll::lfs_t,
    pub(crate) config: ll::lfs_config,
}
// unsafe impl<Storage: driver::Storage> Send for FilesystemAllocation<Storage> {}

/** One of the main API entry points, manipulates files by [`Path`](../path/struct.Path.html)
without opening the corresponding [`File`](struct.File.html).

Use the constructors
[`Filesystem::format`](struct.Filesystem.html#method.format) or
[`Filesystem::mount`](struct.Filesystem.html#method.mount) to obtain an instance,
given a [`FilesystemAllocation`](struct.FilesystemAllocation.html) from
[`Filesystem::allocate`](struct.File.html#method.allocate).

If a filesystem is not formatted or corrupt, `mount`ing will fail with
[`Error::Corruption`](../io/enum.Error.html#variant.Corruption).

To actually read and write files, use the methods of [`File`](struct.File.html).

*/
// #[derive(Debug)]
pub struct Filesystem<'alloc, Storage: driver::Storage> {
    pub(crate) alloc: &'alloc mut FilesystemAllocation<Storage>,
}

pub struct FilesystemWith<'alloc, 'storage, Storage: driver::Storage> {
    pub(crate) alloc: &'alloc mut FilesystemAllocation<Storage>,
    // TODO: remove
    #[allow(dead_code)]
    pub(crate) storage: &'storage mut Storage,
}

impl<'alloc, 'storage, Storage> FilesystemWith<'alloc, 'storage, Storage>
where
    Storage: driver::Storage,
    Storage: 'alloc,
{
    pub fn mount(
        alloc: &'alloc mut FilesystemAllocation<Storage>,
        storage: &'storage mut Storage,
    ) -> Result<FilesystemWith<'alloc, 'storage, Storage>> {
        let fs = Filesystem::placement_new(alloc, storage);
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_mount(&mut fs.alloc.state, &fs.alloc.config) };
        Error::result_from(return_code).map(move |_| FilesystemWith {
            alloc: fs.alloc,
            storage,
        })
    }

    pub fn total_blocks(&self) -> usize {
        Storage::BLOCK_COUNT
    }

    /// Total number of bytes in the filesystem
    pub fn total_space(&self) -> usize {
        Storage::BLOCK_COUNT * Storage::BLOCK_SIZE
    }

    pub fn available_blocks(&mut self) -> Result<usize> {
        // self.alloc.config.context = self.storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_fs_size(&mut self.alloc.state) };
        Error::usize_result_from(return_code).map(|blocks| self.total_blocks() - blocks)
    }

    pub fn available_space(&mut self) -> Result<usize> {
        self.available_blocks()
            .map(|blocks| blocks * Storage::BLOCK_SIZE)
    }

    /// Creates a new, empty directory at the provided path.
    pub fn create_dir(&mut self, path: impl Into<Path<Storage>>) -> Result<()> {
        let return_code = unsafe {
            ll::lfs_mkdir(
                &mut self.alloc.state,
                &path.into() as *const _ as *const cty::c_char,
            )
        };
        Error::result_from(return_code)
    }

    /// Remove a file or directory.
    pub fn remove(&mut self, path: impl Into<Path<Storage>>) -> Result<()> {
        let return_code = unsafe {
            ll::lfs_remove(
                &mut self.alloc.state,
                &path.into() as *const _ as *const cty::c_char,
            )
        };
        Error::result_from(return_code)
    }

    /// Rename or move a file or directory.
    pub fn rename(
        &mut self,
        from: impl Into<Path<Storage>>,
        to: impl Into<Path<Storage>>,
    ) -> Result<()> {
        let return_code = unsafe {
            ll::lfs_rename(
                &mut self.alloc.state,
                &from.into() as *const _ as *const cty::c_char,
                &to.into() as *const _ as *const cty::c_char,
            )
        };
        Error::result_from(return_code)
    }

    /// Given a path, query the filesystem to get information about a file or directory.
    ///
    /// To read user attributes, use
    /// [`Filesystem::attribute`](struct.Filesystem.html#method.attribute)
    pub fn metadata(&mut self, path: impl Into<Path<Storage>>) -> Result<Metadata>
// Result<Metadata<Storage>>
    {
        let mut info: ll::lfs_info = unsafe { mem::MaybeUninit::zeroed().assume_init() };
        let return_code = unsafe {
            ll::lfs_stat(
                &mut self.alloc.state,
                &path.into() as *const _ as *const cty::c_char,
                &mut info,
            )
        };

        Error::result_from(return_code).map(|_| info.into())
    }
}

impl<'alloc, Storage> Filesystem<'alloc, Storage>
where
    Storage: driver::Storage,
    Storage: 'alloc,
{
    #[allow(clippy::all)] // yes should simplify this
    pub fn allocate() -> FilesystemAllocation<Storage> {
        let read_size: u32 = Storage::READ_SIZE as _;
        let write_size: u32 = Storage::WRITE_SIZE as _;
        let block_size: u32 = Storage::BLOCK_SIZE as _;
        let cache_size: u32 = <Storage as driver::Storage>::CACHE_SIZE::to_u32();
        let lookahead_size: u32 = 32 * <Storage as driver::Storage>::LOOKAHEADWORDS_SIZE::to_u32();
        let block_cycles: i32 = Storage::BLOCK_CYCLES as _;
        let block_count: u32 = Storage::BLOCK_COUNT as _;

        debug_assert!(block_cycles >= -1);
        debug_assert!(block_cycles != 0);
        debug_assert!(block_count > 0);

        debug_assert!(read_size > 0);
        debug_assert!(write_size > 0);
        // https://github.com/ARMmbed/littlefs/issues/264
        // Technically, 104 is enough.
        debug_assert!(block_size >= 128);
        debug_assert!(cache_size > 0);
        debug_assert!(lookahead_size > 0);

        // cache must be multiple of read
        debug_assert!(read_size <= cache_size);
        debug_assert!(cache_size % read_size == 0);

        // cache must be multiple of write
        debug_assert!(write_size <= cache_size);
        debug_assert!(cache_size % write_size == 0);

        // block must be multiple of cache
        debug_assert!(cache_size <= block_size);
        debug_assert!(block_size % cache_size == 0);

        let buffers = Buffers {
            read: Default::default(),
            write: Default::default(),
            lookahead: Default::default(),
        };

        let filename_max_plus_one: u32 =
            <Storage as driver::Storage>::FILENAME_MAX_PLUS_ONE::to_u32();
        debug_assert!(filename_max_plus_one > 1);
        debug_assert!(filename_max_plus_one <= 1_022 + 1);
        // limitation of ll-bindings
        debug_assert!(filename_max_plus_one == 255 + 1);
        let path_max_plus_one: u32 = <Storage as driver::Storage>::PATH_MAX_PLUS_ONE::to_u32();
        // TODO: any upper limit?
        debug_assert!(path_max_plus_one >= filename_max_plus_one);
        let file_max = Storage::FILEBYTES_MAX as u32;
        assert!(file_max > 0);
        assert!(file_max <= 2_147_483_647);
        // limitation of ll-bindings
        assert!(file_max == 2_147_483_647);
        let attr_max: u32 = <Storage as driver::Storage>::ATTRBYTES_MAX::to_u32();
        assert!(attr_max > 0);
        assert!(attr_max <= 1_022);
        // limitation of ll-bindings
        assert!(attr_max == 1_022);

        let config = ll::lfs_config {
            context: core::ptr::null_mut(),
            read: Some(<Filesystem<'alloc, Storage>>::lfs_config_read),
            prog: Some(<Filesystem<'alloc, Storage>>::lfs_config_prog),
            erase: Some(<Filesystem<'alloc, Storage>>::lfs_config_erase),
            sync: Some(<Filesystem<'alloc, Storage>>::lfs_config_sync),
            // read: None,
            // prog: None,
            // erase: None,
            // sync: None,
            read_size,
            prog_size: write_size,
            block_size,
            block_count,
            block_cycles,
            cache_size,
            lookahead_size,

            read_buffer: core::ptr::null_mut(),
            prog_buffer: core::ptr::null_mut(),
            lookahead_buffer: core::ptr::null_mut(),

            name_max: filename_max_plus_one.wrapping_sub(1),
            file_max,
            attr_max: attr_max,
        };

        let alloc = FilesystemAllocation {
            buffers,
            state: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            config,
        };

        alloc
    }

    pub fn with<'storage>(
        self,
        storage: &'storage mut Storage,
    ) -> FilesystemWith<'alloc, 'storage, Storage> {
        FilesystemWith {
            alloc: self.alloc,
            storage,
        }
    }

    // TODO: make this an internal method,
    // expose just `mount` and `format`.
    fn placement_new(
        alloc: &'alloc mut FilesystemAllocation<Storage>,
        storage: &mut Storage,
    ) -> Filesystem<'alloc, Storage> {
        alloc.config.context = storage as *mut _ as *mut cty::c_void;

        alloc.config.read_buffer = alloc.buffers.read.as_mut_slice() as *mut _ as *mut cty::c_void;
        alloc.config.prog_buffer = alloc.buffers.write.as_mut_slice() as *mut _ as *mut cty::c_void;
        alloc.config.lookahead_buffer =
            alloc.buffers.lookahead.as_mut_slice() as *mut _ as *mut cty::c_void;

        // alloc.config.read =
        //     Some(<Filesystem<'alloc, Storage>::lfs_config_read);

        // alloc.state.lfs_config = alloc.config;

        Filesystem { alloc }
    }

    pub fn mount(
        alloc: &'alloc mut FilesystemAllocation<Storage>,
        storage: &mut Storage,
    ) -> Result<Filesystem<'alloc, Storage>> {
        let fs = Filesystem::placement_new(alloc, storage);
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_mount(&mut fs.alloc.state, &fs.alloc.config) };
        Error::result_from(return_code).map(move |_| Filesystem { alloc: fs.alloc })
    }

    pub fn format(
        // alloc: &'alloc mut FilesystemAllocation<Storage>,
        storage: &mut Storage,
    ) -> Result<()> {
        let alloc = &mut Self::allocate();
        let fs = Filesystem::placement_new(alloc, storage);
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_format(&mut fs.alloc.state, &fs.alloc.config) };
        Error::result_from(return_code)
    }
}

impl<'alloc, Storage> Filesystem<'alloc, Storage>
where
    Storage: driver::Storage,
    Storage: 'alloc,
{
    // According to documentation, does nothing besides releasing resources.
    // We have drop for that!
    // Further, having it might mislead into thinking that unmounting synchronizes
    // all open buffers or something like that
    // pub fn unmount(self, storage: &mut Storage)
    //     -> Result<()>
    // {
    //     self.alloc.config.context = storage as *mut _ as *mut cty::c_void;
    //     let return_code = unsafe { ll::lfs_unmount(&mut self.alloc.state) };
    //     Error::result_from(return_code)
    // }

    /// Total number of blocks in the filesystem
    pub fn total_blocks(&self) -> usize {
        Storage::BLOCK_COUNT
    }

    /// Total number of bytes in the filesystem
    pub fn total_space(&self) -> usize {
        Storage::BLOCK_COUNT * Storage::BLOCK_SIZE
    }

    /// Available number of unused blocks in the filesystem
    ///
    /// Upstream littlefs documentation notes (on its "current size" function):
    /// "Result is best effort.  If files share COW structures, the returned size may be larger
    /// than the filesystem actually is."
    ///
    /// So it would seem that there are *at least* the number of blocks returned
    /// by this method available, at any given time.
    pub fn available_blocks(&mut self, storage: &mut Storage) -> Result<usize> {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_fs_size(&mut self.alloc.state) };
        Error::usize_result_from(return_code).map(|blocks| self.total_blocks() - blocks)
    }

    /// Available number of unused bytes in the filesystem
    ///
    /// This is a lower bound, more may be available. First, more blocks may be available as
    /// explained in [`available_blocks`](struct.Filesystem.html#method.available_blocks).
    /// Second, files may be inlined.
    pub fn available_space(&mut self, storage: &mut Storage) -> Result<usize> {
        self.available_blocks(storage)
            .map(|blocks| blocks * Storage::BLOCK_SIZE)
    }

    /// Creates a new, empty directory at the provided path.
    pub fn create_dir(
        &mut self,
        path: impl Into<Path<Storage>>,
        storage: &mut Storage,
    ) -> Result<()> {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe {
            ll::lfs_mkdir(
                &mut self.alloc.state,
                &path.into() as *const _ as *const cty::c_char,
            )
        };
        Error::result_from(return_code)
    }

    /// Remove a file or directory.
    pub fn remove(&mut self, path: impl Into<Path<Storage>>, storage: &mut Storage) -> Result<()> {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe {
            ll::lfs_remove(
                &mut self.alloc.state,
                &path.into() as *const _ as *const cty::c_char,
            )
        };
        Error::result_from(return_code)
    }

    /// Rename or move a file or directory.
    pub fn rename(
        &mut self,
        from: impl Into<Path<Storage>>,
        to: impl Into<Path<Storage>>,
        storage: &mut Storage,
    ) -> Result<()> {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe {
            ll::lfs_rename(
                &mut self.alloc.state,
                &from.into() as *const _ as *const cty::c_char,
                &to.into() as *const _ as *const cty::c_char,
            )
        };
        Error::result_from(return_code)
    }

    /// Given a path, query the filesystem to get information about a file or directory.
    ///
    /// To read user attributes, use
    /// [`Filesystem::attribute`](struct.Filesystem.html#method.attribute)
    pub fn metadata(
        &mut self,
        path: impl Into<Path<Storage>>,
        storage: &mut Storage,
    ) -> Result<Metadata>
// Result<Metadata<Storage>>
    {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        // do *not* not call assume_init here and pass into the unsafe block.
        // strange things happen ;)
        let mut info: ll::lfs_info = unsafe { mem::MaybeUninit::zeroed().assume_init() };
        let return_code = unsafe {
            ll::lfs_stat(
                &mut self.alloc.state,
                &path.into() as *const _ as *const cty::c_char,
                &mut info,
            )
        };

        Error::result_from(return_code).map(|_| info.into())
    }

    /// Returns a pseudo-iterator over the entries within a directory.
    pub fn read_dir(
        &mut self,
        path: impl Into<Path<Storage>>,
        storage: &mut Storage,
    ) -> Result<ReadDir<Storage>> {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;

        let mut read_dir = ReadDir {
            state: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            _storage: PhantomData,
        };

        let return_code = unsafe {
            ll::lfs_dir_open(
                &mut self.alloc.state,
                &mut read_dir.state,
                &path.into() as *const _ as *const cty::c_char,
            )
        };

        Error::result_from(return_code).map(|_| read_dir)
    }

    // /// List existing attribute ids
    // pub unsafe fn attributes<P: Into<Path<Storage>>>(
    //     &mut self,
    //     path: P,
    //     storage: &mut Storage,
    // ) ->
    //     Result<[bool; 256]>
    // {
    //     let mut attributes = [false; 256];
    //     let path = path.into();
    //     for (id, attribute_id) in attributes.iter_mut().enumerate() {
    //         *attribute_id = self.attribute(path.clone(), id as u8, storage)?.is_some();
    //     }
    //     Ok(attributes)
    // }

    /// Read attribute.
    pub fn attribute(
        &mut self,
        path: impl Into<Path<Storage>>,
        id: u8,
        storage: &mut Storage,
    ) -> Result<Option<Attribute<Storage>>> {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let mut attribute = Attribute::new(id);
        let attr_max = <Storage as driver::Storage>::ATTRBYTES_MAX::to_u32();

        let return_code = unsafe {
            ll::lfs_getattr(
                &mut self.alloc.state,
                &path.into() as *const _ as *const cty::c_char,
                id,
                &mut attribute.data as *mut _ as *mut cty::c_void,
                attr_max,
            )
        };

        if return_code >= 0 {
            attribute.size = cmp::min(attr_max, return_code as u32) as usize;
            return Ok(Some(attribute));
        }
        if return_code == ll::lfs_error_LFS_ERR_NOATTR {
            return Ok(None);
        }

        Error::result_from(return_code)?;
        // TODO: get rid of this
        unreachable!();
    }

    /// Remove attribute.
    pub fn remove_attribute(
        &mut self,
        path: impl Into<Path<Storage>>,
        id: u8,
        storage: &mut Storage,
    ) -> Result<()> {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;

        let return_code = unsafe {
            ll::lfs_removeattr(
                &mut self.alloc.state,
                &path.into() as *const _ as *const cty::c_char,
                id,
            )
        };
        Error::result_from(return_code)
    }

    /// Set attribute.
    pub fn set_attribute(
        &mut self,
        path: impl Into<Path<Storage>>,
        attribute: &Attribute<Storage>,
        storage: &mut Storage,
    ) -> Result<()> {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;

        let return_code = unsafe {
            ll::lfs_setattr(
                &mut self.alloc.state,
                &path.into() as *const _ as *const cty::c_char,
                attribute.id,
                &attribute.data as *const _ as *const cty::c_void,
                attribute.size as u32,
            )
        };

        Error::result_from(return_code)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Custom user attribute that can be set on files and directories.
///
/// Consists of an numerical identifier between 0 and 255, and arbitrary
/// binary data up to size `ATTRBYTES_MAX`.
///
/// Use [`Filesystem::attribute`](struct.Filesystem.html#method.attribute),
/// [`Filesystem::set_attribute`](struct.Filesystem.html#method.set_attribute), and
/// [`Filesystem::clear_attribute`](struct.Filesystem.html#method.clear_attribute).
pub struct Attribute<S: driver::Storage> {
    id: u8,
    data: GenericArray<u8, S::ATTRBYTES_MAX>,
    size: usize,
}

impl<S: driver::Storage> Attribute<S> {
    pub fn new(id: u8) -> Self {
        Attribute {
            id,
            data: Default::default(),
            size: 0,
        }
    }

    pub fn id(&self) -> u8 {
        self.id
    }

    pub fn data(&self) -> &[u8] {
        let attr_max = <S as driver::Storage>::ATTRBYTES_MAX::to_usize();
        let len = cmp::min(attr_max, self.size);
        &self.data[..len]
    }

    pub fn set_data(&mut self, data: &[u8]) {
        let attr_max = <S as driver::Storage>::ATTRBYTES_MAX::to_usize();
        let len = cmp::min(attr_max, data.len());
        self.data[..len].copy_from_slice(&data[..len]);
        self.size = len;
        for entry in self.data[len..].iter_mut() {
            *entry = 0;
        }
    }
}

/// Item of the directory iterator on [`ReadDirWith`](struct.ReadDirWith.html) and directory
/// pseudo-iterator on [`ReadDir`](struct.ReadDir.html).
#[derive(Clone, Debug, PartialEq)]
pub struct DirEntry<S: driver::Storage> {
    file_name: Filename<S>,
    metadata: Metadata,
}

impl<S: driver::Storage> DirEntry<S> {
    // // Returns the full path to the file that this entry represents.
    // pub fn path(&self) -> Path {}

    // Returns the metadata for the file that this entry points at.
    pub fn metadata(&self) -> Metadata {
        self.metadata.clone()
    }

    // Returns the file type for the file that this entry points at.
    pub fn file_type(&self) -> FileType {
        self.metadata.file_type
    }

    // Returns the bare file name of this directory entry without any other leading path component.
    pub fn file_name(&self) -> Filename<S> {
        self.file_name.clone()
    }
}

/** [`Iterator`](https://doc.rust-lang.org/core/iter/trait.Iterator.html) over files in a directory, returned by [`ReadDir::with`](struct.ReadDir.html#method.with).

Call [`Filesystem::read_dir`](struct.Filesystem.html#method.read_dir) and then
[`ReadDir::with`](struct.ReadDir.html#method.with) to get one.

## Example
```
# use littlefs2::fs::{Filesystem, File};
# use littlefs2::io::prelude::*;
#
# use littlefs2::{consts, ram_storage, driver, io::Result};
#
// setup
ram_storage!(tiny);
let mut ram = Ram::default();
let mut storage = RamStorage::new(&mut ram);
Filesystem::format(&mut storage).ok();
let mut alloc = Filesystem::allocate();
let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

// create
let mut alloc = File::allocate();
let mut file = File::create("abc", &mut alloc, &mut fs, &mut storage).unwrap();
file.set_len(&mut fs, &mut storage, 37);
file.sync(&mut fs, &mut storage);

let mut alloc = File::allocate();
let mut file = File::create("xyz", &mut alloc, &mut fs, &mut storage).unwrap();
file.set_len(&mut fs, &mut storage, 42);
file.sync(&mut fs, &mut storage);

// count
let mut read_dir = fs.read_dir("/", &mut storage).unwrap();
let it = read_dir.with(&mut fs, &mut storage);
assert_eq!(it.count(), 4); // two directories (`.` and `..`) and two regular files

// confirm
let it = fs.read_dir("/", &mut storage).unwrap().with(&mut fs, &mut storage);
assert_eq!(
    it
        .map(|entry| entry.unwrap().metadata().is_dir() as usize)
        .fold(0, |sum, l| sum + l),
    2 // two directories, indeed!
);

// inspect
let it = fs.read_dir("/", &mut storage).unwrap().with(&mut fs, &mut storage);
assert_eq!(
    it
        .map(|entry| entry.unwrap().metadata().len())
        .fold(0, |sum, l| sum + l),
    42 + 37
);

```
*/
pub struct ReadDirWith<'alloc, 'fs, 'storage, S>
where
    S: driver::Storage,
{
    read_dir: ReadDir<S>,
    fs: &'fs mut Filesystem<'alloc, S>,
    storage: &'storage mut S,
}

impl<'alloc, 'fs, 'storage, S> Iterator for ReadDirWith<'alloc, 'fs, 'storage, S>
where
    S: driver::Storage,
{
    type Item = Result<DirEntry<S>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.read_dir.next(self.fs, self.storage)
    }
}

/// Pseudo-Iterator over files in a directory, returned by
/// [`Filesystem::read_dir`](struct.Filesystem.html#method.read_dir).
///
/// Call [`with`](struct.ReadDir.html#method.with) to obtain a real iterator
/// [`ReadDirWith`](struct.ReadDirWith.html), or write your own de-sugared for-loops.
pub struct ReadDir<S>
where
    S: driver::Storage,
{
    state: ll::lfs_dir_t,
    _storage: PhantomData<S>,
}

impl<S> ReadDir<S>
where
    S: driver::Storage,
{
    /// Temporarily bind `fs` and `storage`, to get a real
    /// [`Iterator`](https://doc.rust-lang.org/core/iter/trait.Iterator.html)
    pub fn with<'alloc, 'fs, 'read_dir, 'storage>(
        self,
        fs: &'fs mut Filesystem<'alloc, S>,
        storage: &'storage mut S,
    ) -> ReadDirWith<'alloc, 'fs, 'storage, S>
where {
        ReadDirWith {
            read_dir: self,
            fs,
            storage,
        }
    }

    /// Returns files and directories, starting with `.` and `..`.
    pub fn next<'alloc>(
        &mut self,
        fs: &mut Filesystem<'alloc, S>,
        storage: &mut S,
    ) -> Option<Result<DirEntry<S>>> {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;

        let mut info: ll::lfs_info = unsafe { mem::MaybeUninit::zeroed().assume_init() };

        let return_code =
            unsafe { ll::lfs_dir_read(&mut fs.alloc.state, &mut self.state, &mut info) };

        if return_code > 0 {
            // well here we have it: nasty C strings!
            // actually... nasty C arrays with static lengths! o.O
            let file_name = Filename::new(&unsafe {
                mem::transmute::<[cty::c_char; 256], [u8; 256]>(info.name)
            });
            // let buf: &mut [u8] = unsafe { slice::from_raw_parts_mut(buffer as *mut u8, size as usize) };

            let metadata = info.into();
            let dir_entry = DirEntry {
                file_name,
                metadata,
            };
            return Some(Ok(dir_entry));
        }

        if return_code == 0 {
            return None;
        }

        Some(Err(Error::result_from(return_code).unwrap_err()))
    }
}

/// File type (regular vs directory) and size of a file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata
// pub struct Metadata<S>
// where
//     S: driver::Storage,
//     <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
{
    file_type: FileType,
    size: usize,
    // This belongs in `path::Path`, really!
    // name: Filename<S>,
}

impl From<ll::lfs_info> for Metadata
// impl<S> From<ll::lfs_info> for Metadata<S>
// where
//     S: driver::Storage,
//     <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
{
    fn from(info: ll::lfs_info) -> Self {
        let file_type = match info.type_ as u32 {
            ll::lfs_type_LFS_TYPE_DIR => FileType::Dir,
            ll::lfs_type_LFS_TYPE_REG => FileType::File,
            _ => {
                unreachable!();
            }
        };

        Metadata {
            file_type,
            size: info.size as usize,
            // name: Filename::from_c_char_array(info.name.as_ptr()),
        }
    }
}

impl<'alloc, Storage> Filesystem<'alloc, Storage>
where
    Storage: driver::Storage,
    Storage: 'alloc,
{
    /// C callback interface used by LittleFS to read data with the lower level system below the
    /// filesystem.
    extern "C" fn lfs_config_read(
        c: *const ll::lfs_config,
        block: ll::lfs_block_t,
        off: ll::lfs_off_t,
        buffer: *mut cty::c_void,
        size: ll::lfs_size_t,
    ) -> cty::c_int {
        // println!("in lfs_config_read for {} bytes", size);
        let storage = unsafe { &mut *((*c).context as *mut Storage) };
        debug_assert!(!c.is_null());
        let block_size = unsafe { c.read().block_size };
        let off = (block * block_size + off) as usize;
        let buf: &mut [u8] = unsafe { slice::from_raw_parts_mut(buffer as *mut u8, size as usize) };

        // TODO
        storage.read(off, buf).unwrap();
        0
    }

    /// C callback interface used by LittleFS to program data with the lower level system below the
    /// filesystem.
    extern "C" fn lfs_config_prog(
        c: *const ll::lfs_config,
        block: ll::lfs_block_t,
        off: ll::lfs_off_t,
        buffer: *const cty::c_void,
        size: ll::lfs_size_t,
    ) -> cty::c_int {
        // println!("in lfs_config_prog");
        let storage = unsafe { &mut *((*c).context as *mut Storage) };
        debug_assert!(!c.is_null());
        // let block_size = unsafe { c.read().block_size };
        let block_size = Storage::BLOCK_SIZE as u32;
        let off = (block * block_size + off) as usize;
        let buf: &[u8] = unsafe { slice::from_raw_parts(buffer as *const u8, size as usize) };

        // TODO
        storage.write(off, buf).unwrap();
        0
    }

    /// C callback interface used by LittleFS to erase data with the lower level system below the
    /// filesystem.
    extern "C" fn lfs_config_erase(c: *const ll::lfs_config, block: ll::lfs_block_t) -> cty::c_int {
        // println!("in lfs_config_erase");
        let storage = unsafe { &mut *((*c).context as *mut Storage) };
        let off = block as usize * Storage::BLOCK_SIZE as usize;

        // TODO
        storage.erase(off, Storage::BLOCK_SIZE as usize).unwrap();
        0
    }

    /// C callback interface used by LittleFS to sync data with the lower level interface below the
    /// filesystem. Note that this function currently does nothing.
    extern "C" fn lfs_config_sync(_c: *const ll::lfs_config) -> i32 {
        // println!("in lfs_config_sync");
        // Do nothing; we presume that data is synchronized.
        0
    }
}

/** Builder approach to opening files.

Start with an empty set of flags by calling the constructor `new`, add options, and finally
call `open`. This avoids fiddling with the actual [`FileOpenFlags`](struct.FileOpenFlags.html).
*/
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenOptions(FileOpenFlags);

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenOptions {
    pub fn new() -> Self {
        OpenOptions(FileOpenFlags::empty())
    }
    pub fn read(&mut self, read: bool) -> &mut Self {
        if read {
            self.0.insert(FileOpenFlags::READ)
        } else {
            self.0.remove(FileOpenFlags::READ)
        };
        self
    }
    pub fn write(&mut self, write: bool) -> &mut Self {
        if write {
            self.0.insert(FileOpenFlags::WRITE)
        } else {
            self.0.remove(FileOpenFlags::WRITE)
        };
        self
    }
    pub fn append(&mut self, append: bool) -> &mut Self {
        if append {
            self.0.insert(FileOpenFlags::APPEND)
        } else {
            self.0.remove(FileOpenFlags::APPEND)
        };
        self
    }
    pub fn create(&mut self, create: bool) -> &mut Self {
        if create {
            self.0.insert(FileOpenFlags::CREATE)
        } else {
            self.0.remove(FileOpenFlags::CREATE)
        };
        self
    }
    pub fn create_new(&mut self, create_new: bool) -> &mut Self {
        if create_new {
            self.0.insert(FileOpenFlags::EXCL);
            self.0.insert(FileOpenFlags::CREATE);
        } else {
            self.0.remove(FileOpenFlags::EXCL);
            self.0.remove(FileOpenFlags::CREATE);
        };
        self
    }

    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        if truncate {
            self.0.insert(FileOpenFlags::TRUNCATE)
        } else {
            self.0.remove(FileOpenFlags::TRUNCATE)
        };
        self
    }

    /// Open the file with the options previously specified.
    pub fn open<'falloc, 'fsalloc, S>(
        &self,
        path: impl Into<Path<S>>,
        // attributes: Option<&mut [Attribute<S>]>,
        alloc: &'falloc mut FileAllocation<S>,
        fs: &mut Filesystem<'fsalloc, S>,
        storage: &mut S,
    ) -> Result<File<'falloc, S>>
    where
        S: driver::Storage,
    {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        alloc.config.buffer = alloc.cache.as_mut_slice() as *mut _ as *mut cty::c_void;

        let file = File { alloc };

        let return_code = unsafe {
            ll::lfs_file_opencfg(
                &mut fs.alloc.state,
                &mut file.alloc.state,
                &path.into() as *const _ as *const cty::c_char,
                self.0.bits() as i32,
                &file.alloc.config,
            )
        };

        Error::result_from(return_code)?;
        Ok(file)
    }

    /// Open the file with the options previously specified, keeping references.
    pub fn open_with<'falloc, 'fs, 'fsalloc, 'storage, S>(
        &self,
        path: impl Into<Path<S>>,
        // attributes: Option<&mut [Attribute<S>]>,
        alloc: &'falloc mut FileAllocation<S>,
        fs_with: &'fs mut FilesystemWith<'fsalloc, 'storage, S>,
    ) -> Result<FileWith<'falloc, 'fs, 'fsalloc, 'storage, S>>
    where
        S: driver::Storage,
    {
        // fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        alloc.config.buffer = alloc.cache.as_mut_slice() as *mut _ as *mut cty::c_void;

        let return_code = unsafe {
            ll::lfs_file_opencfg(
                &mut fs_with.alloc.state,
                &mut alloc.state,
                &path.into() as *const _ as *const cty::c_char,
                self.0.bits() as i32,
                &alloc.config,
            )
        };

        let file_with = FileWith { alloc, fs_with };

        Error::result_from(return_code).map(|_| file_with)
    }
}

/** Enumeration of possible methods to seek within an I/O object.

Use the [`Seek`](../io/trait.Seek.html) trait.
*/
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeekFrom {
    Start(u32),
    End(i32),
    Current(i32),
}

impl SeekFrom {
    pub(crate) fn off(self) -> i32 {
        match self {
            SeekFrom::Start(u) => u as i32,
            SeekFrom::End(i) => i,
            SeekFrom::Current(i) => i,
        }
    }

    pub(crate) fn whence(self) -> i32 {
        match self {
            SeekFrom::Start(_) => 0,
            SeekFrom::End(_) => 2,
            SeekFrom::Current(_) => 1,
        }
    }
}

// /// The state of a `Dir`. Pre-allocate with `File::allocate()`.
// pub struct DirAllocation
// {
//     state: ll::lfs_dir_t,
// }

// pub struct Dir<'alloc, S>
// where
//     S: driver::Storage,
//     S: 'alloc,
// {
//     alloc: &'alloc mut DirAllocation,
// }

/// The state of a `File`. Pre-allocate with `File::allocate`.
pub struct FileAllocation<S>
where
    S: driver::Storage,
{
    cache: GenericArray<u8, S::CACHE_SIZE>,
    state: ll::lfs_file_t,
    config: ll::lfs_file_config,
}

/** One of the main API entry points, used to read and write binary non-attribute data to the filesystem.

Use the constructors
[`File::open`](struct.File.html#method.open) or
[`File::create`](struct.File.html#method.create)
to read from existing or write to new files.

More generally, [`OpenOptions`](struct.OpenOptions.html) exposes all the
available options to open files, such as read/write non-truncating access.

Each file has [`Metadata`](struct.Metadata.html) such as its [`FileType`](struct.FileType.html).
Further, each file (including directories) can have [`Attribute`](struct.Attribute.html)s attached.

To manipulate [`Path`](../path/struct.Path.html)s without opening the associated file, use the
methods of [`Filesystem`](struct.Filesystem.html).

**WARNING**: Whereas `std::fs` files are synched automatically on drop, here this is not possible
due to the required references. Therefore, **make sure you call `File::sync` after file changes**.

*/
pub struct File<'falloc, S>
where
    S: driver::Storage,
{
    alloc: &'falloc mut FileAllocation<S>,
}

pub struct FileWith<'falloc, 'fs, 'fsalloc, 'storage, S>
where
    S: driver::Storage,
{
    alloc: &'falloc mut FileAllocation<S>,
    fs_with: &'fs mut FilesystemWith<'fsalloc, 'storage, S>,
}

impl<'falloc, 'fs, 'fsalloc, 'storage, S> FileWith<'falloc, 'fs, 'fsalloc, 'storage, S>
where
    S: driver::Storage,
{
    pub fn open(
        path: impl Into<Path<S>>,
        alloc: &'falloc mut FileAllocation<S>,
        fs_with: &'fs mut FilesystemWith<'fsalloc, 'storage, S>,
    ) -> Result<Self> {
        OpenOptions::new()
            .read(true)
            .open_with(path, alloc, fs_with)
    }

    pub fn create(
        path: impl Into<Path<S>>,
        alloc: &'falloc mut FileAllocation<S>,
        fs_with: &'fs mut FilesystemWith<'fsalloc, 'storage, S>,
    ) -> Result<Self> {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open_with(path, alloc, fs_with)
    }

    pub fn borrow_filesystem<'a>(&'a mut self) -> &'a mut FilesystemWith<'fsalloc, 'storage, S> {
        &mut self.fs_with
    }

    /// Sync the file and drop it.
    /// NB: `std::fs` does not have this, just drops at end of scope.
    pub fn close(self) -> Result<()> {
        // fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code =
            unsafe { ll::lfs_file_close(&mut self.fs_with.alloc.state, &mut self.alloc.state) };
        Error::result_from(return_code)
    }

    /// Synchronize file contents to storage.
    pub fn sync(&mut self) -> Result<()> {
        // assert!(self.fs_with.alloc.config.context == self.fs_with.storage as *mut _ as *mut cty::c_void);
        // fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code =
            unsafe { ll::lfs_file_sync(&mut self.fs_with.alloc.state, &mut self.alloc.state) };
        Error::result_from(return_code)
    }

    /// Size of the file in bytes.
    pub fn len(&mut self) -> Result<usize> {
        // fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code =
            unsafe { ll::lfs_file_size(&mut self.fs_with.alloc.state, &mut self.alloc.state) };
        Error::usize_result_from(return_code)
    }

    /// Truncates or extends the underlying file, updating the size of this file to become size.
    ///
    /// If the size is less than the current file's size, then the file will be shrunk. If it is
    /// greater than the current file's size, then the file will be extended to size and have all
    /// of the intermediate data filled in with 0s.
    pub fn set_len(&mut self, size: usize) -> Result<()> {
        // fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe {
            ll::lfs_file_truncate(
                &mut self.fs_with.alloc.state,
                &mut self.alloc.state,
                size as u32,
            )
        };
        Error::result_from(return_code)
    }
}

bitflags! {
    /// Definition of file open flags which can be mixed and matched as appropriate. These definitions
    /// are reminiscent of the ones defined by POSIX.
    struct FileOpenFlags: u32 {
        /// Open file in read only mode.
        const READ = 0x1;
        /// Open file in write only mode.
        const WRITE = 0x2;
        /// Open file for reading and writing.
        const READWRITE = Self::READ.bits | Self::WRITE.bits;
        /// Create the file if it does not exist.
        const CREATE = 0x0100;
        /// Fail if creating a file that already exists.
        /// TODO: Good name for this
        const EXCL = 0x0200;
        /// Truncate the file if it already exists.
        const TRUNCATE = 0x0400;
        /// Open the file in append only mode.
        const APPEND = 0x0800;
    }
}

impl<'falloc, S> File<'falloc, S>
where
    S: driver::Storage,
{
    pub fn allocate() -> FileAllocation<S> {
        // TODO: more checks
        let cache_size: u32 = <S as driver::Storage>::CACHE_SIZE::to_u32();
        debug_assert!(cache_size > 0);

        let config = ll::lfs_file_config {
            buffer: core::ptr::null_mut(),
            attrs: core::ptr::null_mut(),
            attr_count: 0,
        };

        FileAllocation {
            cache: Default::default(),
            state: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            config,
        }
    }

    pub fn open<'fsalloc: 'falloc>(
        path: impl Into<Path<S>>,
        alloc: &'falloc mut FileAllocation<S>,
        fs: &mut Filesystem<'fsalloc, S>,
        storage: &mut S,
    ) -> Result<Self> {
        OpenOptions::new().read(true).open(path, alloc, fs, storage)
    }

    pub fn create<'fsalloc: 'falloc>(
        path: impl Into<Path<S>>,
        alloc: &'falloc mut FileAllocation<S>,
        fs: &mut Filesystem<'fsalloc, S>,
        storage: &mut S,
    ) -> Result<Self> {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path, alloc, fs, storage)
    }

    /// Sync the file and drop it.
    /// NB: `std::fs` does not have this, just drops at end of scope.
    pub fn close<'fsalloc: 'falloc>(
        self,
        fs: &mut Filesystem<'fsalloc, S>,
        storage: &mut S,
    ) -> Result<()> {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_file_close(&mut fs.alloc.state, &mut self.alloc.state) };
        Error::result_from(return_code)?;
        Ok(())
    }

    /// Synchronize file contents to storage.
    pub fn sync<'fsalloc: 'falloc>(
        &mut self,
        fs: &mut Filesystem<'fsalloc, S>,
        storage: &mut S,
    ) -> Result<()> {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_file_sync(&mut fs.alloc.state, &mut self.alloc.state) };
        Error::result_from(return_code)?;
        Ok(())
    }

    /// Size of the file in bytes.
    pub fn len<'fsalloc: 'falloc>(
        &mut self,
        fs: &mut Filesystem<'fsalloc, S>,
        storage: &mut S,
    ) -> Result<usize> {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_file_size(&mut fs.alloc.state, &mut self.alloc.state) };
        Error::usize_result_from(return_code)
    }

    /// Truncates or extends the underlying file, updating the size of this file to become size.
    ///
    /// If the size is less than the current file's size, then the file will be shrunk. If it is
    /// greater than the current file's size, then the file will be extended to size and have all
    /// of the intermediate data filled in with 0s.
    pub fn set_len<'fsalloc: 'falloc>(
        &mut self,
        fs: &mut Filesystem<'fsalloc, S>,
        storage: &mut S,
        size: usize,
    ) -> Result<()> {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe {
            ll::lfs_file_truncate(&mut fs.alloc.state, &mut self.alloc.state, size as u32)
        };
        Error::result_from(return_code)?;
        Ok(())
    }
}

/// Regular file vs directory
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FileType {
    File,
    Dir,
}

impl FileType {
    #[allow(clippy::all)] // following `std::fs`
    pub fn is_dir(&self) -> bool {
        *self == FileType::Dir
    }

    #[allow(clippy::all)] // following `std::fs`
    pub fn is_file(&self) -> bool {
        *self == FileType::File
    }
}

impl Metadata
// impl<S> Metadata<S>
// where
//     S: driver::Storage,
//     <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
{
    pub fn file_type(&self) -> FileType {
        self.file_type
    }

    pub fn is_dir(&self) -> bool {
        self.file_type().is_dir()
    }

    pub fn is_file(&self) -> bool {
        self.file_type().is_file()
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    // pub fn name(&self) -> Path<S> {
    //     self.name.clone()
    // }
}

impl<'falloc, 'fsalloc, S> io::Read<'fsalloc, S> for File<'falloc, S>
where
    S: driver::Storage,
    'fsalloc: 'falloc,
{
    fn read(
        &mut self,
        fs: &mut Filesystem<'fsalloc, S>,
        storage: &mut S,
        buf: &mut [u8],
    ) -> Result<usize> {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe {
            ll::lfs_file_read(
                &mut fs.alloc.state,
                &mut self.alloc.state,
                buf.as_mut_ptr() as *mut cty::c_void,
                buf.len() as u32,
            )
        };
        Error::usize_result_from(return_code)
    }
}

impl<'falloc, 'fs, 'fsalloc, 'storage, S> io::ReadWith
    for FileWith<'falloc, 'fs, 'fsalloc, 'storage, S>
where
    S: driver::Storage,
{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let return_code = unsafe {
            ll::lfs_file_read(
                &mut self.fs_with.alloc.state,
                &mut self.alloc.state,
                buf.as_mut_ptr() as *mut cty::c_void,
                buf.len() as u32,
            )
        };
        Error::usize_result_from(return_code)
    }
}

impl<'falloc, 'fs, 'fsalloc, 'storage, S> io::WriteWith
    for FileWith<'falloc, 'fs, 'fsalloc, 'storage, S>
where
    S: driver::Storage,
    'fsalloc: 'falloc,
{
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let return_code = unsafe {
            ll::lfs_file_write(
                &mut self.fs_with.alloc.state,
                &mut self.alloc.state,
                buf.as_ptr() as *const cty::c_void,
                buf.len() as u32,
            )
        };
        Error::usize_result_from(return_code)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl<'falloc, 'fsalloc, S> io::Write<'fsalloc, S> for File<'falloc, S>
where
    S: driver::Storage,
    'fsalloc: 'falloc,
{
    fn write(
        &mut self,
        fs: &mut Filesystem<'fsalloc, S>,
        storage: &mut S,
        buf: &[u8],
    ) -> Result<usize> {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe {
            ll::lfs_file_write(
                &mut fs.alloc.state,
                &mut self.alloc.state,
                buf.as_ptr() as *const cty::c_void,
                buf.len() as u32,
            )
        };
        Error::usize_result_from(return_code)
    }

    fn flush(&mut self, _fs: &mut Filesystem<'fsalloc, S>, _storage: &mut S) -> Result<()> {
        Ok(())
    }
}

impl<'falloc, 'fs, 'fsalloc, 'storage, S> io::SeekWith
    for FileWith<'falloc, 'fs, 'fsalloc, 'storage, S>
where
    S: driver::Storage,
    'fsalloc: 'falloc,
{
    fn seek(&mut self, pos: SeekFrom) -> Result<usize> {
        let return_code = unsafe {
            ll::lfs_file_seek(
                &mut self.fs_with.alloc.state,
                &mut self.alloc.state,
                pos.off(),
                pos.whence(),
            )
        };
        Error::usize_result_from(return_code)
    }
}

impl<'falloc, 'fsalloc, S> io::Seek<'fsalloc, S> for File<'falloc, S>
where
    S: driver::Storage,
    'fsalloc: 'falloc,
{
    fn seek(
        &mut self,
        fs: &mut Filesystem<'fsalloc, S>,
        storage: &mut S,
        pos: SeekFrom,
    ) -> Result<usize> {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe {
            ll::lfs_file_seek(
                &mut fs.alloc.state,
                &mut self.alloc.state,
                pos.off(),
                pos.whence(),
            )
        };
        Error::usize_result_from(return_code)
    }
}
