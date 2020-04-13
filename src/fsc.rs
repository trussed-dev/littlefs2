//! Experimental Filesystem version using closures.

use core::{cell::RefCell, cmp, mem, slice};

use bitflags::bitflags;
use generic_array::typenum::marker_traits::Unsigned;
use littlefs2_sys as ll;

// so far, don't need `heapless-bytes`.
pub type Bytes<SIZE> = generic_array::GenericArray<u8, SIZE>;

use crate::{
    io::{
        self,
        Error,
        Result,
        SeekFrom,
    },
    path::{
        Filename,
        Path,
    },
    driver,
};

struct Cache<Storage: driver::Storage> {
    read: Bytes<Storage::CACHE_SIZE>,
    write: Bytes<Storage::CACHE_SIZE>,
    // lookahead: aligned::Aligned<aligned::A4, Bytes<Storage::LOOKAHEAD_SIZE>>,
    lookahead: generic_array::GenericArray<u32, Storage::LOOKAHEADWORDS_SIZE>,
}

impl<S: driver::Storage> Cache<S> {
    pub fn new() -> Self {
        Self {
            read: Default::default(),
            write: Default::default(),
            // lookahead: aligned::Aligned(Default::default()),
            lookahead: Default::default(),
        }
    }
}

impl<S: driver::Storage> Default for Cache<S> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Allocation<Storage: driver::Storage> {
    cache: Cache<Storage>,
    config: ll::lfs_config,
    state: ll::lfs_t,
}

// pub fn check_storage_requirements(

impl<Storage: driver::Storage> Allocation<Storage> {

    pub fn new() -> Allocation<Storage> {
        let read_size: u32 = Storage::READ_SIZE as _;
        let write_size: u32 = Storage::WRITE_SIZE as _;
        let block_size: u32 = Storage::BLOCK_SIZE as _;
        let cache_size: u32 = <Storage as driver::Storage>::CACHE_SIZE::U32;
        let lookahead_size: u32 =
            32 * <Storage as driver::Storage>::LOOKAHEADWORDS_SIZE::U32;
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

        let cache = Cache::new();

        let filename_max_plus_one: u32 =
            <Storage as driver::Storage>::FILENAME_MAX_PLUS_ONE::to_u32();
        debug_assert!(filename_max_plus_one > 1);
        debug_assert!(filename_max_plus_one <= 1_022+1);
        // limitation of ll-bindings
        debug_assert!(filename_max_plus_one == 255+1);
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
            read: Some(<Filesystem<'_, Storage>>::lfs_config_read),
            prog: Some(<Filesystem<'_, Storage>>::lfs_config_prog),
            erase: Some(<Filesystem<'_, Storage>>::lfs_config_erase),
            sync: Some(<Filesystem<'_, Storage>>::lfs_config_sync),
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

        Self {
            cache,
            state: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            config,
        }
    }

}

// pub struct Filesystem<'alloc, 'storage, Storage: driver::Storage> {
//     pub(crate) alloc: &'alloc mut Allocation<Storage>,
//     pub(crate) storage: &'storage mut Storage,
// }

// one lifetime is simpler than two... hopefully should be enough
// also consider "erasing" the lifetime completely
pub struct Filesystem<'a, Storage: driver::Storage> {
    alloc: RefCell<&'a mut Allocation<Storage>>,
    storage: &'a mut Storage,
}

/// Regular file vs directory
#[derive(Clone,Copy,Debug,Eq,Hash,PartialEq)]
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

/// File type (regular vs directory) and size of a file.
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct Metadata {
    file_type: FileType,
    size: usize,
}

impl Metadata
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
}

impl From<ll::lfs_info> for Metadata
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
        }
    }
}

impl<Storage: driver::Storage> Filesystem<'_, Storage> {

    pub fn allocate() -> Allocation<Storage> {
        Allocation::new()
    }

    pub fn format(storage: &mut Storage) -> Result<()> {

        let alloc = &mut Allocation::new();
        let fs = Filesystem::new(alloc, storage);
        let mut alloc = fs.alloc.borrow_mut();
        let return_code = unsafe { ll::lfs_format(&mut alloc.state, &alloc.config) };
        Error::result_from(return_code)
    }

    // TODO: check if this is equivalent to `is_formatted`.
    pub fn is_mountable(storage: &mut Storage) -> bool {
        let alloc = &mut Allocation::new();
        match Filesystem::mount(alloc, storage) {
            Ok(_) => true,
            _ => false,
        }
    }

    // Can BorrowMut be implemented "unsafely" instead?
    // This is intended to be a second option, besides `into_inner`, to
    // get access to the Flash peripheral in Storage.
    pub unsafe fn borrow_storage_mut(&mut self) -> &mut Storage {
        self.storage
    }

    /// This API avoids the need for using `Allocation`.
    pub fn mount_and_then<R>(
        storage: &mut Storage,
        f: impl FnOnce(&Filesystem<'_, Storage>) -> Result<R>,
    ) -> Result<R> {

        let mut alloc = Allocation::new();
        let fs = Filesystem::mount(&mut alloc, storage)?;
        f(&fs)
    }

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
    pub fn available_blocks(&self) -> Result<usize> {
        let return_code = unsafe { ll::lfs_fs_size( &mut self.alloc.borrow_mut().state) };
        Error::usize_result_from(return_code).map(|blocks| self.total_blocks() - blocks)
    }

    /// Available number of unused bytes in the filesystem
    ///
    /// This is a lower bound, more may be available. First, more blocks may be available as
    /// explained in [`available_blocks`](struct.Filesystem.html#method.available_blocks).
    /// Second, files may be inlined.
    pub fn available_space(&self) -> Result<usize> {
        self.available_blocks().map(|blocks| blocks * Storage::BLOCK_SIZE)
    }

    /// Remove a file or directory.
    pub fn remove(&self, path: impl Into<Path<Storage>>) -> Result<()> {
        let return_code = unsafe { ll::lfs_remove(
            &mut self.alloc.borrow_mut().state,
            &path.into()[..] as *const _ as *const cty::c_char,
        ) };
        Error::result_from(return_code)
    }

    /// Remove a file or directory.
    pub fn remove_dir(&self, path: impl Into<Path<Storage>>) -> Result<()> {
        self.remove(path)
    }

    /// Rename or move a file or directory.
    pub fn rename(
        &self,
        from: impl Into<Path<Storage>>,
        to: impl Into<Path<Storage>>,
    ) -> Result<()> {
        let return_code = unsafe { ll::lfs_rename(
            &mut self.alloc.borrow_mut().state,
            &from.into()[..] as *const _ as *const cty::c_char,
            &to.into()[..] as *const _ as *const cty::c_char,
        ) };
        Error::result_from(return_code)
    }

    /// Given a path, query the filesystem to get information about a file or directory.
    ///
    /// To read user attributes, use
    /// [`Filesystem::attribute`](struct.Filesystem.html#method.attribute)
    pub fn metadata(&self, path: impl Into<Path<Storage>>) -> Result<Metadata> {

        // do *not* not call assume_init here and pass into the unsafe block.
        // strange things happen ;)

        // TODO: Check we don't have UB here *too*.
        // I think it's fine, as we immediately copy out the data
        // to our own structure.
        let mut info: ll::lfs_info = unsafe { mem::MaybeUninit::zeroed().assume_init() };
        let return_code = unsafe {
            ll::lfs_stat(
                &mut self.alloc.borrow_mut().state,
                &path.into()[..] as *const _ as *const cty::c_char,
                &mut info,
            )
        };

        Error::result_from(return_code).map(|_| info.into())
    }

    /// Read attribute.
    pub fn attribute(
        &self,
        path: impl Into<Path<Storage>>,
        id: u8,
    ) ->
        Result<Option<Attribute<Storage>>>
    {
        let mut attribute = Attribute::new(id);
        let attr_max = <Storage as driver::Storage>::ATTRBYTES_MAX::to_u32();

        let return_code = unsafe { ll::lfs_getattr(
            &mut self.alloc.borrow_mut().state,
            &path.into()[..] as *const _ as *const cty::c_char,
            id,
            &mut attribute.data as *mut _ as *mut cty::c_void,
            attr_max,
        ) };

        if return_code >= 0 {
            attribute.size = cmp::min(attr_max, return_code as u32) as usize;
            return Ok(Some(attribute));
        }
        if return_code == ll::lfs_error_LFS_ERR_NOATTR {
            return Ok(None)
        }

        Error::result_from(return_code)?;
        // TODO: get rid of this
        unreachable!();
    }

    /// Remove attribute.
    pub fn remove_attribute(
        &self,
        path: impl Into<Path<Storage>>,
        id: u8,
    ) -> Result<()> {
        let return_code = unsafe { ll::lfs_removeattr(
            &mut self.alloc.borrow_mut().state,
            &path.into()[..] as *const _ as *const cty::c_char,
            id,
        ) };
        Error::result_from(return_code)
    }

    /// Set attribute.
    pub fn set_attribute(
        &self,
        path: impl Into<Path<Storage>>,
        attribute: &Attribute<Storage>
    ) ->
        Result<()>
    {
        let return_code = unsafe { ll::lfs_setattr(
            &mut self.alloc.borrow_mut().state,
            &path.into()[..] as *const _ as *const cty::c_char,
            attribute.id,
            &attribute.data as *const _ as *const cty::c_void,
            attribute.size as u32,
        ) };

        Error::result_from(return_code)
    }


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
    extern "C" fn lfs_config_erase(
        c: *const ll::lfs_config,
        block: ll::lfs_block_t,
    ) -> cty::c_int {
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

#[derive(Clone,Debug,Eq,PartialEq)]
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
    data: Bytes<S::ATTRBYTES_MAX>,
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

/// The state of a `File`. Pre-allocate with `File::allocate`.
pub struct FileAllocation<S: driver::Storage>
{
    cache: Bytes<S::CACHE_SIZE>,
    state: ll::lfs_file_t,
    config: ll::lfs_file_config,
}

impl<S: driver::Storage> FileAllocation<S> {
    pub fn new() -> Self {
        let cache_size: u32 = <S as driver::Storage>::CACHE_SIZE::to_u32();
        debug_assert!(cache_size > 0);
        unsafe { mem::MaybeUninit::zeroed().assume_init() }
    }
}

pub struct File<'a, 'b, S: driver::Storage>
{
    alloc: RefCell<&'b mut FileAllocation<S>>,
    fs: &'b Filesystem<'a, S>,
}

impl<'a, 'b, Storage: driver::Storage> File<'a, 'b, Storage>
{
    pub fn allocate() -> FileAllocation<Storage> {
        FileAllocation::new()
    }

    pub unsafe fn open(
        fs: &'b Filesystem<'a, Storage>,
        alloc: &'b mut FileAllocation<Storage>,
        path:  impl Into<Path<Storage>>,
    ) ->
        Result<Self>
    {
        OpenOptions::new()
            .read(true)
            .open(fs, alloc, path)
    }

    pub fn open_and_then<R>(
        fs: &Filesystem<'a, Storage>,
        path: impl Into<Path<Storage>>,
        f: impl FnOnce(&File<'_, '_, Storage>) -> Result<R>,
    ) ->
        Result<R>
    {
        OpenOptions::new()
            .read(true)
            .open_and_then(fs, path, f)
    }

    pub unsafe fn create(
        fs: &'b Filesystem<'a, Storage>,
        alloc: &'b mut FileAllocation<Storage>,
        path:  impl Into<Path<Storage>>,
    ) ->
        Result<Self>
    {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(fs, alloc, path)
    }

    pub fn create_and_then<R>(
        fs: &Filesystem<'a, Storage>,
        path: impl Into<Path<Storage>>,
        f: impl FnOnce(&File<'_, '_, Storage>) -> Result<R>,
    ) ->
        Result<R>
    {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open_and_then(fs, path, f)
    }

    // Safety-hatch to experiment with missing parts of API
    pub unsafe fn borrow_filesystem<'c>(&'c mut self) -> &'c Filesystem<'a, Storage> {
        &self.fs
    }

    /// Sync the file and drop it from the internal linked list.
    /// Not doing this is UB, which is why we have all the closure-based APIs.
    ///
    /// TODO: check if this can be closed >1 times, if so make it safe
    ///
    /// Update: It seems like there's an assertion on a flag called `LFS_F_OPENED`:
    /// https://github.com/ARMmbed/littlefs/blob/4c9146ea539f72749d6cc3ea076372a81b12cb11/lfs.c#L2549
    /// https://github.com/ARMmbed/littlefs/blob/4c9146ea539f72749d6cc3ea076372a81b12cb11/lfs.c#L2566
    ///
    /// - On second call, shouldn't find ourselves in the "mlist of mdirs"
    /// - Since we don't have dynamically allocated buffers, at least we don't hit the double-free.
    /// - Not sure what happens in `lfs_file_sync`, but it should be easy to just error on
    ///   not LFS_F_OPENED...
    pub unsafe fn close(self) -> Result<()>
    {
        let return_code = ll::lfs_file_close(
            &mut self.fs.alloc.borrow_mut().state,
            &mut self.alloc.borrow_mut().state,
        );
        Error::result_from(return_code)
    }

    /// Synchronize file contents to storage.
    pub fn sync(&self) -> Result<()> {
        let return_code = unsafe { ll::lfs_file_sync(
            &mut self.fs.alloc.borrow_mut().state,
            &mut self.alloc.borrow_mut().state,
        ) };
        Error::result_from(return_code)
    }

    /// Size of the file in bytes.
    pub fn len(&self) -> Result<usize> {
        let return_code = unsafe { ll::lfs_file_size(
            &mut self.fs.alloc.borrow_mut().state,
            &mut self.alloc.borrow_mut().state
        ) };
        Error::usize_result_from(return_code)
    }

    /// Truncates or extends the underlying file, updating the size of this file to become size.
    ///
    /// If the size is less than the current file's size, then the file will be shrunk. If it is
    /// greater than the current file's size, then the file will be extended to size and have all
    /// of the intermediate data filled in with 0s.
    pub fn set_len(&self, size: usize) -> Result<()> {
        let return_code = unsafe { ll::lfs_file_truncate(
            &mut self.fs.alloc.borrow_mut().state,
            &mut self.alloc.borrow_mut().state,
            size as u32,
        ) };
        Error::result_from(return_code)
    }

}


/// Options and flags which can be used to configure how a file is opened.
///
/// This builder exposes the ability to configure how a File is opened and what operations
/// are permitted on the open file. The File::open and File::create methods are aliases
/// for commonly used options using this builder.
///
/// Consider `File::with_options()` to avoid having to `use` OpenOptions.
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct OpenOptions (FileOpenFlags);

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenOptions {

    /// Open the file with the options previously specified, keeping references.
    ///
    /// unsafe since UB can arise if files are not closed (see below).
    ///
    /// The alternative method `open_and_then` is suggested.
    ///
    /// Note that:
    /// - files *must* be closed before going out of scope (they are stored in a linked list),
    ///   closing removes them from there
    /// - since littlefs is supposed to be *fail-safe*, we can't just close files in
    ///   Drop and panic if something went wrong.
    pub unsafe fn open<'a, 'b, S: driver::Storage>(
        &self,
        fs: &'b Filesystem<'a, S>,
        alloc: &'b mut FileAllocation<S>,
        path: impl Into<Path<S>>,
    ) ->
        Result<File<'a, 'b, S>>
    {
        alloc.config.buffer = &mut alloc.cache as *mut _ as *mut cty::c_void;
        let path = path.into();

        let return_code = ll::lfs_file_opencfg(
                &mut fs.alloc.borrow_mut().state,
                &mut alloc.state,
                &path[..] as *const _  as *const cty::c_char,
                self.0.bits() as i32,
                &alloc.config,
        );

        let file = File {
            alloc: RefCell::new(alloc),
            fs,
        };

        Error::result_from(return_code).map(|_| file)
    }

    /// (Hopefully) safe abstraction around `open`.
    pub fn open_and_then<'a, R, S: driver::Storage>(
        &self,
        fs: &Filesystem<'a, S>,
        path: impl Into<Path<S>>,
        f: impl FnOnce(&File<'a, '_, S>) -> Result<R>,
    )
        -> Result<R>
    {
        let mut alloc = FileAllocation::new(); // lifetime 'c
        let mut file = unsafe { self.open(fs, &mut alloc, path)? };
        // Q: what is the actually correct behaviour?
        // E.g. if res is Ok but closing gives an error.
        // Or if closing fails because something is broken and
        // we'd already know that from an Err res.
        let res = f(&mut file);
        unsafe { file.close()? };
        res
    }

    pub fn new() -> Self {
        OpenOptions(FileOpenFlags::empty())
    }

    pub fn read(&mut self, read: bool) -> &mut Self {
        if read {
            self.0.insert(FileOpenFlags::READ)
        } else {
            self.0.remove(FileOpenFlags::READ)
        }; self
    }

    pub fn write(&mut self, write: bool) -> &mut Self {
        if write {
            self.0.insert(FileOpenFlags::WRITE)
        } else {
            self.0.remove(FileOpenFlags::WRITE)
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
            self.0.insert(FileOpenFlags::CREATE)
        } else {
            self.0.remove(FileOpenFlags::CREATE)
        }; self
    }

    pub fn create_new(&mut self, create_new: bool) -> &mut Self {
        if create_new {
            self.0.insert(FileOpenFlags::EXCL);
            self.0.insert(FileOpenFlags::CREATE);
        } else {
            self.0.remove(FileOpenFlags::EXCL);
            self.0.remove(FileOpenFlags::CREATE);
        }; self
    }

    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        if truncate {
            self.0.insert(FileOpenFlags::TRUNCATE)
        } else {
            self.0.remove(FileOpenFlags::TRUNCATE)
        }; self
    }

    pub fn with_options() -> OpenOptions {
        OpenOptions::new()
    }

}

impl<S: driver::Storage> io::ReadClosure for File<'_, '_, S>
{
    fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let return_code = unsafe { ll::lfs_file_read(
            &mut self.fs.alloc.borrow_mut().state,
            &mut self.alloc.borrow_mut().state,
            buf.as_mut_ptr() as *mut cty::c_void,
            buf.len() as u32,
        ) };
        Error::usize_result_from(return_code)
    }
}

impl<S: driver::Storage> io::SeekClosure for File<'_, '_, S>
{
    fn seek(&self, pos: SeekFrom) -> Result<usize> {
        let return_code = unsafe { ll::lfs_file_seek(
            &mut self.fs.alloc.borrow_mut().state,
            &mut self.alloc.borrow_mut().state,
            pos.off(),
            pos.whence(),
        ) };
        Error::usize_result_from(return_code)
    }
}

impl<S: driver::Storage> io::WriteClosure for File<'_, '_, S>
{
    fn write(&self, buf: &[u8]) -> Result<usize> {
        let return_code = unsafe { ll::lfs_file_write(
            &mut self.fs.alloc.borrow_mut().state,
            &mut self.alloc.borrow_mut().state,
            buf.as_ptr() as *const cty::c_void,
            buf.len() as u32,
        ) };
        Error::usize_result_from(return_code)
    }

    fn flush(&self) -> Result<()> { Ok(()) }
}

#[derive(Clone,Debug,PartialEq)]
pub struct DirEntry<S: driver::Storage> {
    file_name: Filename<S>,
    metadata: Metadata,
    #[cfg(feature = "dir-entry-path")]
    path: Path<S>,
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

    /// Returns the full path to the file that this entry represents.
    ///
    /// The full path is created by joining the original path to read_dir with the filename of this entry.
    #[cfg(feature = "dir-entry-path")]
    pub fn path(&self) -> Path<S> {
        self.path.clone()
    }

}

pub struct ReadDirAllocation {
    state: ll::lfs_dir_t,
}

impl ReadDirAllocation {
    pub fn new() -> Self {
        unsafe { mem::MaybeUninit::zeroed().assume_init() }
    }
}

pub struct ReadDir<'a, 'b, S: driver::Storage>
{
    alloc: RefCell<&'b mut ReadDirAllocation>,
    fs: &'b Filesystem<'a, S>,
    #[cfg(feature = "dir-entry-path")]
    path: Path<S>,
}

impl<'a, 'b, S: driver::Storage> Iterator for ReadDir<'a, 'b, S>
{
    type Item = Result<DirEntry<S>>;

    // remove this allowance again, once path overflow is properly handled
    #[allow(unreachable_code)]
    fn next(&mut self) -> Option<Self::Item> {
        let mut info: ll::lfs_info = unsafe {
            mem::MaybeUninit::zeroed().assume_init()
        };

        let return_code = unsafe {
            ll::lfs_dir_read(
                &mut self.fs.alloc.borrow_mut().state,
                &mut self.alloc.borrow_mut().state,
                &mut info,
            )
        };

        if return_code > 0 {
            // well here we have it: nasty C strings!
            // actually... nasty C arrays with static lengths! o.O
            let transmuted = & unsafe { mem::transmute::<[cty::c_char; 256], [u8; 256]>(info.name) };
            let file_name = Filename::new(transmuted);

            let metadata = info.into();

            #[cfg(feature = "dir-entry-path")]
            // TODO: error handling...
            let path = self.path.try_join(&file_name).unwrap();

            let dir_entry = DirEntry {
                file_name,
                metadata,
                #[cfg(feature = "dir-entry-path")]
                path,
            };
            return Some(Ok(dir_entry));
        }

        if return_code == 0 {
            return None
        }

        Some(Err(Error::result_from(return_code).unwrap_err()))
    }
}

impl<'a, 'b, S: driver::Storage> ReadDir<'a, 'b, S> {

    // Safety-hatch to experiment with missing parts of API
    pub unsafe fn borrow_filesystem<'c>(&'c mut self) -> &'c Filesystem<'a, S> {
        &self.fs
    }
}

impl<S: driver::Storage> ReadDir<'_, '_, S> {
    // Again, not sure if this can be called twice
    // Update: This one seems to be safe to call multiple times,
    // it just goes through the "mlist" and removes itself.
    //
    // Although I guess if the compiler reuses the ReadDirAllocation, and we still
    // have an (unsafely genereated) ReadDir with that handle; on the other hand
    // as long as ReadDir is not Copy.
    pub /* unsafe */ fn close(self) -> Result<()>
    {
        let return_code = unsafe { ll::lfs_dir_close(
            &mut self.fs.alloc.borrow_mut().state,
            &mut self.alloc.borrow_mut().state,
        ) };
        Error::result_from(return_code)
    }
}


impl<'a, Storage: driver::Storage> Filesystem<'a, Storage> {

    pub fn read_dir_and_then<R>(
        &self,
        path: impl Into<Path<Storage>>,
        // *not* &ReadDir, as Iterator takes &mut
        f: impl FnOnce(&mut ReadDir<'_, '_, Storage>) -> Result<R>,
    ) -> Result<R>
    {
        let mut alloc = ReadDirAllocation::new();
        let mut read_dir = unsafe { self.read_dir(&mut alloc, path)? };
        let res = f(&mut read_dir);
        // unsafe { read_dir.close()? };
        read_dir.close()?;
        res
    }

	/// Returns a pseudo-iterator over the entries within a directory.
    ///
    /// This is unsafe since it can induce UB just like File::open.
	pub unsafe fn read_dir<'b>(
        &'b self,
        alloc: &'b mut ReadDirAllocation,
        path: impl Into<Path<Storage>>,
    ) ->
        Result<ReadDir<'a, 'b, Storage>>
    {
        let path = path.into();

        let return_code = ll::lfs_dir_open(
            &mut self.alloc.borrow_mut().state,
            &mut alloc.state,
            &path[..] as *const _ as *const cty::c_char,
        );

        let read_dir = ReadDir {
            alloc: RefCell::new(alloc),
            fs: self,
            #[cfg(feature = "dir-entry-path")]
            path,
        };

        Error::result_from(return_code).map(|_| read_dir)
    }

}


impl<'a, Storage: driver::Storage> Filesystem<'a, Storage> {

    pub fn mount(
        alloc: &'a mut Allocation<Storage>,
        storage: &'a mut Storage,
    ) -> Result<Self> {

        let fs = Self::new(alloc, storage);
        let mut alloc = fs.alloc.borrow_mut();
        let return_code = unsafe { ll::lfs_mount(&mut alloc.state, &alloc.config) };
        drop(alloc);
        Error::result_from(return_code).map(move |_| { fs } )
    }

    // Not public, user should use `mount`, possibly after `format`
    fn new(alloc: &'a mut Allocation<Storage>, storage: &'a mut Storage) -> Self {

        alloc.config.context = storage as *mut _ as *mut cty::c_void;

        alloc.config.read_buffer = &mut alloc.cache.read as *mut _ as *mut cty::c_void;
        alloc.config.prog_buffer = &mut alloc.cache.write as *mut _ as *mut cty::c_void;
        alloc.config.lookahead_buffer = &mut alloc.cache.lookahead as *mut _ as *mut cty::c_void;

        Filesystem { alloc: RefCell::new(alloc), storage }
    }

    /// Deconstruct `Filesystem`, intention is to allow access to
    /// the underlying Flash peripheral in driver::Storage etc.
    ///
    /// See also `borrow_storage_mut`.
    pub fn into_inner(self) -> (&'a mut Allocation<Storage>, &'a mut Storage) {
        (self.alloc.into_inner(), self.storage)
    }

    /// Creates a new, empty directory at the provided path.
    pub fn create_dir(&self, path: impl Into<Path<Storage>>) -> Result<()> {

        let return_code = unsafe { ll::lfs_mkdir(
            &mut self.alloc.borrow_mut().state,
            &path.into()[..] as *const _ as *const cty::c_char,
        ) };
        Error::result_from(return_code)
    }

    /// Recursively create a directory and all of its parent components if they are missing.
    pub fn create_dir_all(&self, path: impl Into<Path<Storage>>) -> Result<()> {
        // Placeholder implementation!
        // - Path should gain a few methods
        // - Maybe should pull in `heapless-bytes` (and merge upstream into `heapless`)
        // - All kinds of sanity checks and possible logic errors possible...
        let path = path.into();

        for i in 0..path.0.len() {
            if path.0[i] == b'/' {
                let dir = &path.0[..i];
                match self.create_dir(dir) {
                    Ok(_) => {}
                    Err(io::Error::EntryAlreadyExisted) => {}
                    error => { panic!("{:?}", &error); }
                }
            }
        }
        match self.create_dir(&path.0[..]) {
            Ok(_) => {}
            Err(io::Error::EntryAlreadyExisted) => {}
            error => { panic!("{:?}", &error); }
        }
        Ok(())
    }

    /// Read the entire contents of a file into a bytes vector.
    pub fn read<N: generic_array::ArrayLength<u8>>(
        &self,
        path: impl Into<Path<Storage>>,
    )
        -> Result<heapless::Vec<u8, N>>
    {
        let mut contents: heapless::Vec::<u8, N> = Default::default();
        contents.resize_default(contents.capacity()).unwrap();
        let len = File::open_and_then(self, path, |file| {
            use io::ReadClosure;
            // let len = file.read_to_end(&mut contents)?;
            let len = file.read(&mut contents)?;
            Ok(len)
        })?;
        contents.resize_default(len).unwrap();
        Ok(contents)
    }

    /// Write a slice as the entire contents of a file.
    ///
    /// This function will create a file if it does not exist,
    /// and will entirely replace its contents if it does.
    pub fn write(
        &self,
        path: impl Into<Path<Storage>>,
        contents: &[u8],
    ) -> Result<()>
    {
        File::create_and_then(self, path, |file| {
            use io::WriteClosure;
            file.write_all(contents)
        })?;
        Ok(())
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use generic_array::typenum::consts;
    use driver::Storage as LfsStorage;
    use io::Result as LfsResult;
    const_ram_storage!(TestStorage, 4096);

    #[test]
    fn todo() {
        let mut test_storage = TestStorage::new();
        // let jackson5 = [b"A", b"B", b"C", 1, 2, 3];
        let jackson5 = b"ABC 123";
        let jackson5 = &jackson5[..];

        Filesystem::format(&mut test_storage).unwrap();
        Filesystem::mount_and_then(&mut test_storage, |fs| {

            println!("blocks going in: {}", fs.available_blocks()?);
            fs.create_dir_all("/tmp/test")?;
            // let weird_filename = b"/tmp/test/a.t\x7fxt";
            // fs.write(&weird_filename[..], jackson5)?;
            fs.write(b"/tmp/test/a.t\x7fxt".as_ref(), jackson5)?;
            fs.write("/tmp/test/b.txt", jackson5)?;
            fs.write("/tmp/test/c.txt", jackson5)?;
            println!("blocks after 3 files of size 3: {}", fs.available_blocks()?);

            // Not only does this need "unsafe", but also the compiler catches
            // the double-call of `file.close` (here, and in the closure teardown).
            //
            // File::create_and_then(&mut fs, "/tmp/zzz", |file| {
            //     unsafe { file.close() }
            // }).unwrap();

            fs.read_dir_and_then("/", |read_dir| {
                for entry in read_dir {
                    let entry: DirEntry<_> = entry?;
                    println!("hello {:?}", entry.file_name());
                    #[cfg(feature = "dir-entry-path")]
                    println!("--> path = {:?}", entry.path());
                }
                Ok(())
            })?;

            fs.read_dir_and_then("/tmp", |read_dir| {
                for entry in read_dir {
                    println!("entry: {:?}", entry?.file_name());
                }
                Ok(())
            })?;

            fs.read_dir_and_then("/tmp/test", |read_dir| {
                for entry in read_dir {
                    let entry = entry?;
                    println!("entry: {:?}", entry.file_name());
                    #[cfg(feature = "dir-entry-path")] {
                        println!("path: {:?}", entry.path());

                        let mut attribute = Attribute::new(37);
                        if entry.file_type().is_dir() {
                            attribute.set_data(b"directory alarm");
                        } else {
                            attribute.set_data(b"ceci n'est pas une pipe");
                            // not 100% sure this is allowed, but if seems to work :)
                            fs.write(entry.path(), b"Alles neu macht n\xc3\xa4chstens der Mai")?;
                        }
                        fs.set_attribute(entry.path(), &attribute)?;
                    }
                }
                Ok(())
            })?;

            #[cfg(feature = "dir-entry-path")]
            fs.read_dir_and_then("/tmp/test", |read_dir| {
                for (i, entry) in read_dir.enumerate() {
                    let entry = entry?;
                    println!("\nfile {:?}", entry.file_name());

                    if entry.file_type().is_file() {
                        let content: heapless::Vec::<u8, heapless::consts::U256> = fs.read(entry.path())?;
                        println!("content:\n{:?}", core::str::from_utf8(&content).unwrap());
                        // println!("and now the removal");
                        // fs.remove(entry.path())?;
                    }

                    let attribute = fs.attribute(entry.path(), 37)?.unwrap();
                    println!("attribute 37: {:?}", core::str::from_utf8(attribute.data()).unwrap());

                    // TODO: There is a problem removing the file with special name.
                    // Not sure if I'm not understanding how Rust "strings" work, or whether
                    // littlefs has a problem with filenames of this type.
                    // if entry.file_type().is_file() {
                    if entry.file_type().is_file() && i >= 2 + 1 {
                        fs.remove(entry.path())?;
                    }

                    // // this one fails:
                    // // - our iterator reaches it (at the end, after `c.txt`)
                    // // - reading it fails with `NoSuchEntry`
                    // if i == 3 {
                    //     fs.write("/tmp/test/out-of-nowhere.txt", &[])?;
                    // }

                }
                Ok(())
            })?;

            Ok(())
        }).unwrap();

        let mut alloc = Allocation::new();
        let fs = Filesystem::mount(&mut alloc, &mut test_storage).unwrap();
        fs.write("/z.txt", &jackson5).unwrap();
    }

    #[test]
    fn path() {
        let _path: Path<TestStorage> = b"a.txt"[..].into();
    }

    #[test]
    fn nested() {
        let mut test_storage = TestStorage::new();

        Filesystem::format(&mut test_storage).unwrap();
        Filesystem::mount_and_then(&mut test_storage, |fs| {

            fs.write("a\x7f.txt", &[])?;
            fs.write("b.txt", &[])?;
            fs.write("c.txt", &[])?;

            fs.read_dir_and_then(".", |read_dir| {
                for entry in read_dir {
                    let entry = entry?;
                    println!("{:?}", entry.file_name());

                    // The `&mut ReadDir` is not actually available here
                    // Do we want a way to borrow_filesystem for DirEntry?
                    // One usecase is to read data from the files iterated over.
                    //
                    if entry.metadata.is_file() {
                        fs.write(
                            &entry.file_name()[..],
                            b"wowee zowie"
                        )?;
                    }
                }
                Ok(())
            })?;

            Ok(())
        }).unwrap();
    }


    #[test]
    fn issue_3_original_report() {
        let mut test_storage = TestStorage::new();

        Filesystem::format(&mut test_storage).unwrap();
        Filesystem::mount_and_then(&mut test_storage, |fs| {

            fs.write("a.txt", &[])?;
            fs.write("b.txt", &[])?;
            fs.write("c.txt", &[])?;

            // works fine
            fs.read_dir_and_then(".", |read_dir| {
                for entry in read_dir {
                    let entry = entry?;
                    println!("{:?}", entry.file_type());
                }
                Ok(())
            })?;


            use io::WriteClosure;

            let mut a1 = File::allocate();
            let f1 = unsafe { File::open(&fs, &mut a1, "a.txt")? };
            f1.write(b"some text")?;

            let mut a2 = File::allocate();
            let f2 = unsafe { File::open(&fs, &mut a2, "b.txt")? };
            f2.write(b"more text")?;

            unsafe { f1.close()? }; // program hangs here
            unsafe { f2.close()? }; // this statement is never reached

            Ok(())
        }).unwrap();
    }
}
