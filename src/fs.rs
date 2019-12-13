/*! Filesystem manipulation operations.
*/
use core::{
    marker::PhantomData,
    mem,
    slice,
};

use crate::{
    io::{
        self,
        Error,
        Result,
        MountError,
        MountResult,
    },
    path::{
        Filename,
        Path,
    },
    driver,
};

use bitflags::bitflags;
use littlefs2_sys as ll;

use generic_array::{
    ArrayLength,
    GenericArray,
    typenum::marker_traits::Unsigned as _,
};

/// Typestates to distinguish mounted from not mounted filesystems
pub mod mount_state {
    pub trait MountState {}
    pub struct Mounted;
    impl MountState for Mounted {}
    pub struct NotMounted;
    impl MountState for NotMounted {}

}

/// The three global buffers used by LittleFS
// #[derive(Debug)]
pub struct Buffers<Storage>
where
    Storage: driver::Storage,
    <Storage as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    read: GenericArray<u8, Storage::CACHE_SIZE>,
    write: GenericArray<u8, Storage::CACHE_SIZE>,
    // must be 4-byte aligned, hence the `u32`s
    lookahead: GenericArray<u32, Storage::LOOKAHEADWORDS_SIZE>,
}

// #[derive(Debug)]
pub struct FilesystemAllocation<Storage>
where
    Storage: driver::Storage,
    <Storage as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    buffers: Buffers<Storage>,
    pub(crate) state: ll::lfs_t,
    pub(crate) config: ll::lfs_config,
}

// #[derive(Debug)]
pub struct Filesystem<'alloc, Storage, MountState = mount_state::NotMounted>
where
    Storage: driver::Storage,
    <Storage as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
    MountState: mount_state::MountState,
{
    pub(crate) alloc: &'alloc mut FilesystemAllocation<Storage>,
    _mount_state: MountState,
}


impl<'alloc, Storage> Filesystem<'alloc, Storage>
where
    Storage: driver::Storage,
    Storage: 'alloc,
    <Storage as driver::Storage>::BLOCK_SIZE: ArrayLength<u8>,
    <Storage as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
    <Storage as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
    <Storage as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>,
{
    pub fn allocate() -> FilesystemAllocation<Storage> {
        let read_size: u32 = Storage::READ_SIZE as _;
        let write_size: u32 = Storage::WRITE_SIZE as _;
        let block_size: u32 = <Storage as driver::Storage>::BLOCK_SIZE::to_u32();
        let cache_size: u32 = <Storage as driver::Storage>::CACHE_SIZE::to_u32();
        let lookahead_size: u32 =
            32 * <Storage as driver::Storage>::LOOKAHEADWORDS_SIZE::to_u32();
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

        let filename_max: u32 = <Storage as driver::Storage>::FILENAME_MAX_PLUS_ONE::to_u32();
        debug_assert!(filename_max > 0);
        let path_max: u32 = <Storage as driver::Storage>::PATH_MAX_PLUS_ONE::to_u32();
        debug_assert!(path_max >= filename_max);
        let file_max = Storage::FILEBYTES_MAX as u32;
        assert!(file_max > 0);
        assert!(file_max <= 2_147_483_647);
        let attr_max = Storage::ATTRBYTES_MAX as u32;
        assert!(attr_max > 0);
        assert!(attr_max <= 1_022);

        let config = ll::lfs_config {
            context: core::ptr::null_mut(),
            read: Some(<Filesystem<'alloc, Storage, mount_state::Mounted>>::lfs_config_read),
            prog: Some(<Filesystem<'alloc, Storage, mount_state::Mounted>>::lfs_config_prog),
            erase: Some(<Filesystem<'alloc, Storage, mount_state::Mounted>>::lfs_config_erase),
            sync: Some(<Filesystem<'alloc, Storage, mount_state::Mounted>>::lfs_config_sync),
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

            name_max: filename_max,
            file_max,
            attr_max,
        };

        let alloc = FilesystemAllocation {
            buffers,
            state: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            config,
        };

        alloc
    }

    // TODO: make this an internal method,
    // expose just `mount` and `format`.
    fn placement_new(
        alloc: &'alloc mut FilesystemAllocation<Storage>,
        storage: &mut Storage
    ) ->
        Filesystem<'alloc, Storage, mount_state::NotMounted>
    {
        alloc.config.context = storage as *mut _ as *mut cty::c_void;

        alloc.config.read_buffer =
            alloc.buffers.read.as_mut_slice() as *mut _ as *mut cty::c_void;
        alloc.config.prog_buffer =
            alloc.buffers.write.as_mut_slice() as *mut _ as *mut cty::c_void;
        alloc.config.lookahead_buffer =
            alloc.buffers.lookahead.as_mut_slice() as *mut _ as *mut cty::c_void;

        // alloc.config.read =
        //     Some(<Filesystem<'alloc, Storage, mount_state::Mounted>>::lfs_config_read);

        // alloc.state.lfs_config = alloc.config;

        let littlefs = Filesystem {
            alloc,
            _mount_state: mount_state::NotMounted,
        };

        littlefs

    }

    pub fn mount(
        alloc: &'alloc mut FilesystemAllocation<Storage>,
        storage: &mut Storage,
    ) -> MountResult<'alloc, Storage> {

        let fs = Filesystem::placement_new(alloc, storage);
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_mount(&mut fs.alloc.state, &fs.alloc.config) };
        match Error::empty_from(return_code) {
            Ok(_) => {
                let mounted = Filesystem {
                    alloc: fs.alloc,
                    _mount_state: mount_state::Mounted,
                };
                MountResult::Ok(mounted)
            },
            Err(error) => {
                MountResult::Err(MountError(fs, error))
            }
        }
    }

    pub fn format(
        alloc: &'alloc mut FilesystemAllocation<Storage>,
        storage: &mut Storage,
    ) ->
        Result<()>
    {
        let fs = Filesystem::placement_new(alloc, storage);
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_format(&mut fs.alloc.state, &fs.alloc.config) };
        Error::empty_from(return_code)?;
        Ok(())
    }
}

impl<'alloc, Storage> Filesystem<'alloc, Storage, mount_state::Mounted>
where
    Storage: driver::Storage,
    Storage: 'alloc,
    // MountState: mount_state::MountState,
    <Storage as driver::Storage>::BLOCK_SIZE: ArrayLength<u8>,
    <Storage as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
    <Storage as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
    <Storage as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>,
{
    pub fn unmount(self, storage: &mut Storage)
        -> Result<Filesystem<'alloc, Storage, mount_state::NotMounted>>
    {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_unmount(&mut self.alloc.state) };
        Error::empty_from(return_code)?;
        Ok(
            Filesystem {
                alloc: self.alloc,
                _mount_state: mount_state::NotMounted,
            }
        )
    }

    /// Creates a new, empty directory at the provided path.
    pub fn create_dir<P: Into<Path<Storage>>>(&mut self, path: P, storage: &mut Storage) -> Result<()> {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_mkdir(
            &mut self.alloc.state,
            &path.into() as *const _ as *const cty::c_char,
        ) };
        Error::empty_from(return_code)?;
        Ok(())
    }

    /// Remove a file or directory.
    pub fn remove<P: Into<Path<Storage>>>(&mut self, path: P, storage: &mut Storage) -> Result<()> {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_remove(
            &mut self.alloc.state,
            &path.into() as *const _ as *const cty::c_char,
        ) };
        Error::empty_from(return_code)?;
        Ok(())
    }

    /// Rename or move a file or directory.
    pub fn rename<P, Q>(
        &mut self,
        from: P, to: Q,
        storage: &mut Storage,
    ) -> Result<()> where
        P: Into<Path<Storage>>,
        Q: Into<Path<Storage>>,
    {
        self.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_rename(
            &mut self.alloc.state,
            &from.into() as *const _ as *const cty::c_char,
            &to.into() as *const _ as *const cty::c_char,
        ) };
        Error::empty_from(return_code)?;
        Ok(())
    }

    /// Given a path, query the file system to get information about a file or directory.
    pub fn metadata<P: Into<Path<Storage>>>(
        &mut self,
        path: P,
        storage: &mut Storage,
    ) ->
        Result<Metadata>
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

        Error::empty_from(return_code)?;
        let metadata = info.into();
        Ok(metadata)
    }

	/// Returns an iterator over the entries within a directory.
	pub fn read_dir<P: Into<Path<Storage>>>(
        &mut self,
        path: P,
        storage: &mut Storage,
    ) ->
        Result<ReadDir<Storage>>
    {
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

        Error::empty_from(return_code).map(|_| read_dir)
    }
}

pub struct DirEntry<S>
where
    S: driver::Storage,
    <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
{
    file_name: Filename<S>,
    metadata: Metadata,
}

impl<S> DirEntry<S>
where
    S: driver::Storage,
    <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
{
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
    <S as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
    <S as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    pub fn next<'alloc>(
        &mut self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
    ) ->
        Option<Result<DirEntry<S>>>
    {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;

        let mut info: ll::lfs_info = unsafe { mem::MaybeUninit::zeroed().assume_init() };

        let return_code = unsafe {
            ll::lfs_dir_read(
                &mut fs.alloc.state,
                &mut self.state,
                &mut info,
            )
        };

        if return_code > 0 {
            // well here we have it: nasty C strings!
            // actually... nasty C arrays with static lengths! o.O
            let file_name = Filename::new(& unsafe { mem::transmute::<[i8; 256], [u8; 256]>(info.name) } );
            // let buf: &mut [u8] = unsafe { slice::from_raw_parts_mut(buffer as *mut u8, size as usize) };

            let metadata = info.into();
            let dir_entry = DirEntry { file_name, metadata };
            return Some(Ok(dir_entry));
        }

        if return_code == 0 {
            return None
        }

        Some(Err(Error::empty_from(return_code).unwrap_err()))

    }
}


#[derive(Clone,Debug)]
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
            _ => { unreachable!(); }
        };

        Metadata {
            file_type,
            size: info.size as usize,
            // name: Filename::from_c_char_array(info.name.as_ptr()),
        }
    }
}

impl<'alloc, Storage, MountState> Filesystem<'alloc, Storage, MountState>
where
    Storage: driver::Storage,
    Storage: 'alloc,
    MountState: mount_state::MountState,
    <Storage as driver::Storage>::BLOCK_SIZE: ArrayLength<u8>,
    <Storage as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
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
        let storage: &mut Storage = unsafe { mem::transmute((*c).context) };
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
        let storage: &mut Storage = unsafe { mem::transmute((*c).context) };
        debug_assert!(!c.is_null());
        // let block_size = unsafe { c.read().block_size };
        let block_size = <Storage as driver::Storage>::BLOCK_SIZE::to_u32();
        let off = (block * block_size + off) as usize;
        let buf: &[u8] = unsafe { slice::from_raw_parts(buffer as *const u8, size as usize) };

        // TODO
        storage.write(off, buf).unwrap();
        0
    }

    /// C callback interface used by LittleFS to erase data with the lower level system below the
    /// filesystem.
    extern "C" fn lfs_config_erase(
        c: *const ll::lfs_config,
        block: ll::lfs_block_t,
    ) -> cty::c_int {
        // println!("in lfs_config_erase");
        // let littlefs: &mut Filesystem<Storage> = unsafe { mem::transmute((*c).context) };
        let storage: &mut Storage = unsafe { mem::transmute((*c).context) };
        let off = block as usize * <Storage as driver::Storage>::BLOCK_SIZE::to_usize();

        // TODO
        storage.erase(off, <Storage as driver::Storage>::BLOCK_SIZE::to_usize()).unwrap();
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

Starting with an empty set of flags, add options and finally
call `open`. This avoids fiddling with the actual [`FileOpenFlags`](struct.FileOpenFlags.html).
*/
pub struct OpenOptions (FileOpenFlags);

impl OpenOptions {
    pub fn new() -> Self {
        OpenOptions(FileOpenFlags::empty())
    }
    pub fn read(&mut self, read: bool) -> &mut Self {
        if read {
            self.0.insert(FileOpenFlags::RDONLY)
        } else {
            self.0.remove(FileOpenFlags::RDONLY)
        }; self
    }
    pub fn write(&mut self, write: bool) -> &mut Self {
        if write {
            self.0.insert(FileOpenFlags::WRONLY)
        } else {
            self.0.remove(FileOpenFlags::WRONLY)
        }; self
    }
    pub fn append(&mut self, append: bool) -> &mut Self {
        if append {
            self.0.insert(FileOpenFlags::APPEND)
        } else {
            self.0.remove(FileOpenFlags::APPEND)
        }; self
    }
    pub fn create(&mut self, create: bool) -> &mut Self {
        if create {
            self.0.insert(FileOpenFlags::CREAT)
        } else {
            self.0.remove(FileOpenFlags::CREAT)
        }; self
    }
    pub fn create_new(&mut self, create_new: bool) -> &mut Self {
        if create_new {
            self.0.insert(FileOpenFlags::EXCL);
            self.0.insert(FileOpenFlags::CREAT);
        } else {
            self.0.remove(FileOpenFlags::EXCL);
            self.0.remove(FileOpenFlags::CREAT);
        }; self
    }

    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        if truncate {
            self.0.insert(FileOpenFlags::TRUNC)
        } else {
            self.0.remove(FileOpenFlags::TRUNC)
        }; self
    }

    pub fn open<'alloc, S, P: Into<Path<S>>>(
        &self,
        path: P,
        alloc: &'alloc mut FileAllocation<S>,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
    ) ->
        Result<File<'alloc, S>>
    where
        S: driver::Storage,
        <S as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
        <S as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>,
        <S as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
    {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        alloc.config.buffer = alloc.cache.as_mut_slice() as *mut _ as *mut cty::c_void;

        let file = File { alloc };

        let return_code = unsafe { ll::lfs_file_opencfg(
                &mut fs.alloc.state,
                &mut file.alloc.state,
                &path.into() as *const _  as *const cty::c_char,
                self.0.bits() as i32,
                &file.alloc.config,
        ) };

        Error::empty_from(return_code)?;
        Ok(file)
    }
}

/** Enumeration of possible methods to seek within an I/O object.

Use the the [`Seek`](../io/trait.Seek.html) trait.
*/
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
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


// /// The state of a `Dir`. Must be pre-allocated via `File::allocate()`.
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

/// The state of a `File`. Must be pre-allocated via `File::allocate()`.
pub struct FileAllocation<S>
where
    S: driver::Storage,
    <S as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
{
    cache: GenericArray<u8, S::CACHE_SIZE>,
    state: ll::lfs_file_t,
    config: ll::lfs_file_config,
}

/** The main abstraction. Use this to read and write binary data to the file system.

Given a [`FileAllocation`](struct.FileAllocation.html), use the shortcuts `File::open` or `File::create` to
open existing, or create new files. Generally, [`OpenOptions`](struct.OpenOptions.html) exposes all the
available options how to open files.

*/
pub struct File<'alloc, S>
where
    S: driver::Storage,
    S: 'alloc,
    <S as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
{
    alloc: &'alloc mut FileAllocation<S>,
}

bitflags! {
    /// Definition of file open flags which can be mixed and matched as appropriate. These definitions
    /// are reminiscent of the ones defined by POSIX.
    pub struct FileOpenFlags: u32 {
        /// Open file in read only mode.
        const RDONLY = 0x1;
        /// Open file in write only mode.
        const WRONLY = 0x2;
        /// Open file for reading and writing.
        const RDWR = Self::RDONLY.bits | Self::WRONLY.bits;
        /// Create the file if it does not exist.
        const CREAT = 0x0100;
        /// Fail if creating a file that already exists.
        const EXCL = 0x0200;
        /// Truncate the file if it already exists.
        const TRUNC = 0x0400;
        /// Open the file in append only mode.
        const APPEND = 0x0800;
    }
}

impl<'alloc, S> File<'alloc, S>
where
    S: driver::Storage,
    S: 'alloc,
    <S as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <S as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>,
    <S as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
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

        let alloc = FileAllocation {
            cache: Default::default(),
            state: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            config,
        };
        alloc
    }

    pub fn open<P: Into<Path<S>>>(
        path: P,
        alloc: &'alloc mut FileAllocation<S>,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
    ) ->
        Result<Self>
    {
        OpenOptions::new()
            .read(true)
            .open(path, alloc, fs, storage)
    }

    pub fn create<P: Into<Path<S>>>(
        path: P,
        alloc: &'alloc mut FileAllocation<S>,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
    ) ->
        Result<Self>
    {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path, alloc, fs, storage)
    }

    /// Sync the file and drop it.
    /// NB: `std::fs` does not have this, just drops at end of scope.
    pub fn close(
        self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
    ) ->
        Result<()>
    {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_file_close(
            &mut fs.alloc.state,
            &mut self.alloc.state,
        ) };
        Error::empty_from(return_code)?;
        Ok(())
    }

    /// Synchronize file contents to storage.
    pub fn sync(
        &mut self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
    ) ->
        Result<()>
    {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_file_sync(
            &mut fs.alloc.state,
            &mut self.alloc.state,
        ) };
        Error::empty_from(return_code)?;
        Ok(())
    }

    pub fn len(
        &mut self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
    ) ->
        Result<usize>
    {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_file_size(
            &mut fs.alloc.state, &mut self.alloc.state
        ) };
        Error::usize_from(return_code)
    }

    /// Truncates or extends the underlying file, updating the size of this file to become size.
    ///
    /// If the size is less than the current file's size, then the file will be shrunk. If it is
    /// greater than the current file's size, then the file will be extended to size and have all
    /// of the intermediate data filled in with 0s.
    pub fn set_len(
        &mut self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
        size: usize,
    ) ->
        Result<()>
    {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_file_truncate(
            &mut fs.alloc.state,
            &mut self.alloc.state,
            size as u32,
        ) };
        Error::empty_from(return_code)?;
        Ok(())
    }

}

#[derive(Clone,Copy,Debug,Eq,Hash,PartialEq)]
pub enum FileType {
    File,
    Dir,
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        *self == FileType::Dir
    }

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

    // pub fn name(&self) -> Path<S> {
    //     self.name.clone()
    // }
}

impl<'alloc, S> io::Read<'alloc, S> for File<'alloc, S>
where
    S: driver::Storage,
    <S as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <S as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    fn read(
        &mut self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
        buf: &mut [u8],
    ) ->
        Result<usize>
    {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_file_read(
            &mut fs.alloc.state,
            &mut self.alloc.state,
            buf.as_mut_ptr() as *mut cty::c_void,
            buf.len() as u32,
        ) };
        Error::usize_from(return_code)
    }
}

impl<'alloc, S> io::Write<'alloc, S> for File<'alloc, S>
where
    S: driver::Storage,
    <S as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <S as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    fn write(
        &mut self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
        buf: &[u8],
    ) ->
        Result<usize>
    {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_file_write(
            &mut fs.alloc.state,
            &mut self.alloc.state,
            buf.as_ptr() as *const cty::c_void,
            buf.len() as u32,
        ) };
        Error::usize_from(return_code)
    }

    fn flush(
        &mut self,
        _fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        _storage: &mut S,
    ) ->
        Result<()>
    {
        Ok(())
    }
}

impl<'alloc, S> io::Seek<'alloc, S> for File<'alloc, S>
where
    S: driver::Storage,
    <S as driver::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <S as driver::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    fn seek(
        &mut self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
        pos: SeekFrom,
    ) ->
        Result<usize>
    {
        fs.alloc.config.context = storage as *mut _ as *mut cty::c_void;
        let return_code = unsafe { ll::lfs_file_seek(
            &mut fs.alloc.state,
            &mut self.alloc.state,
            pos.off(),
            pos.whence(),
        ) };
        Error::usize_from(return_code)
    }
}
