//! Experimental Filesystem version using closures.

use core::ffi::{c_int, c_void};
use core::ptr::addr_of;
use core::ptr::addr_of_mut;
use core::{
    cell::{RefCell, UnsafeCell},
    mem, slice,
};
use generic_array::typenum::marker_traits::Unsigned;
use littlefs2_sys as ll;

// so far, don't need `heapless-bytes`.
pub type Bytes<SIZE> = generic_array::GenericArray<u8, SIZE>;

pub use littlefs2_core::{Attribute, DirEntry, FileOpenFlags, FileType, Metadata};

use crate::{
    driver,
    io::{self, Error, OpenSeekFrom, Result},
    path::{Path, PathBuf},
    DISK_VERSION,
};

fn error_code_from<T>(result: Result<T>) -> ll::lfs_error {
    result
        .map(|_| ll::lfs_error_LFS_ERR_OK)
        .unwrap_or_else(From::from)
}

fn result_from<T>(return_value: T, error_code: ll::lfs_error) -> Result<T> {
    if let Some(error) = Error::new(error_code) {
        Err(error)
    } else {
        Ok(return_value)
    }
}

pub fn u32_result(return_value: i32) -> Result<u32> {
    u32::try_from(return_value).map_err(|_| {
        let error_code = c_int::try_from(return_value).unwrap_or(c_int::MIN);
        Error::new(error_code).unwrap()
    })
}

struct Cache<Storage: driver::Storage> {
    read: UnsafeCell<Bytes<Storage::CACHE_SIZE>>,
    write: UnsafeCell<Bytes<Storage::CACHE_SIZE>>,
    // lookahead: aligned::Aligned<aligned::A4, Bytes<Storage::LOOKAHEAD_SIZE>>,
    lookahead: UnsafeCell<generic_array::GenericArray<u64, Storage::LOOKAHEAD_SIZE>>,
}

impl<S: driver::Storage> Cache<S> {
    pub fn new() -> Self {
        Self {
            read: Default::default(),
            write: Default::default(),
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

impl<Storage: driver::Storage> Default for Allocation<Storage> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default, Clone, Debug)]
#[non_exhaustive]
pub struct Config {
    pub mount_flags: MountFlags,
}

bitflags::bitflags! {
    #[derive(Default, Clone, Copy,Debug)]
    pub struct MountFlags: u32 {
        const DISABLE_BLOCK_COUNT_CHECK = ll::lfs_fs_flags_LFS_CFG_DISABLE_BLOCK_COUNT_CHECK as _;
    }
}

impl<Storage: driver::Storage> Allocation<Storage> {
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }
    pub fn with_config(config: Config) -> Allocation<Storage> {
        let read_size: u32 = Storage::READ_SIZE as _;
        let write_size: u32 = Storage::WRITE_SIZE as _;
        let block_size: u32 = Storage::BLOCK_SIZE as _;
        let cache_size: u32 = <Storage as driver::Storage>::CACHE_SIZE::U32;
        let lookahead_size: u32 = 8 * <Storage as driver::Storage>::LOOKAHEAD_SIZE::U32;
        let block_cycles: i32 = Storage::BLOCK_CYCLES as _;
        let block_count: u32 = Storage::BLOCK_COUNT as _;

        debug_assert!(block_cycles >= -1);
        debug_assert!(block_cycles != 0);
        debug_assert!(block_count > 0);

        debug_assert!(read_size > 0);
        debug_assert!(write_size > 0);
        // https://github.com/littlefs-project/littlefs/issues/264
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

        let filename_max_plus_one: u32 = crate::consts::FILENAME_MAX_PLUS_ONE;
        debug_assert!(filename_max_plus_one > 1);
        debug_assert!(filename_max_plus_one <= 1_022 + 1);
        // limitation of ll-bindings
        debug_assert!(filename_max_plus_one == 255 + 1);
        let path_max_plus_one: u32 = crate::consts::PATH_MAX_PLUS_ONE as _;
        // TODO: any upper limit?
        debug_assert!(path_max_plus_one >= filename_max_plus_one);
        let file_max = crate::consts::FILEBYTES_MAX;
        assert!(file_max > 0);
        assert!(file_max <= 2_147_483_647);
        // limitation of ll-bindings
        assert!(file_max == 2_147_483_647);
        let attr_max: u32 = crate::consts::ATTRBYTES_MAX;
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
            attr_max,
            compact_thresh: 0,
            metadata_max: 0,
            inline_max: 0,
            disk_version: DISK_VERSION.into(),
            flags: config.mount_flags.bits(),
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

fn metadata(info: ll::lfs_info) -> Metadata {
    let file_type = match info.type_ as ll::lfs_type {
        ll::lfs_type_LFS_TYPE_DIR => FileType::Dir,
        ll::lfs_type_LFS_TYPE_REG => FileType::File,
        _ => {
            unreachable!();
        }
    };

    Metadata::new(file_type, info.size as usize)
}

struct RemoveDirAllProgress {
    files_removed: usize,
    skipped_any: bool,
}

impl<Storage: driver::Storage> Filesystem<'_, Storage> {
    pub fn allocate() -> Allocation<Storage> {
        Allocation::new()
    }

    pub fn format(storage: &mut Storage) -> Result<()> {
        Self::format_with_config(storage, Config::default())
    }

    pub fn format_with_config(storage: &mut Storage, config: Config) -> Result<()> {
        let alloc = &mut Allocation::with_config(config);
        let fs = Filesystem::new(alloc, storage);
        let mut alloc = fs.alloc.borrow_mut();
        let return_code = unsafe { ll::lfs_format(&mut alloc.state, &alloc.config) };
        result_from((), return_code)
    }

    // TODO: check if this is equivalent to `is_formatted`.
    pub fn is_mountable(storage: &mut Storage) -> bool {
        Self::is_mountable_with_config(storage, Config::default())
    }

    // TODO: check if this is equivalent to `is_formatted`.
    pub fn is_mountable_with_config(storage: &mut Storage, config: Config) -> bool {
        let alloc = &mut Allocation::with_config(config);
        Filesystem::mount(alloc, storage).is_ok()
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
        Self::mount_and_then_with_config(storage, Config::default(), f)
    }

    /// This API avoids the need for using `Allocation`.
    pub fn mount_and_then_with_config<R>(
        storage: &mut Storage,
        config: Config,
        f: impl FnOnce(&Filesystem<'_, Storage>) -> Result<R>,
    ) -> Result<R> {
        let mut alloc = Allocation::with_config(config);
        let fs = Filesystem::mount(&mut alloc, storage)?;
        f(&fs)
    }

    pub fn shrink(&self, block_count: usize) -> Result<()> {
        let mut alloc = self.alloc.borrow_mut();
        let return_code = unsafe { ll::lfs_fs_shrink(&mut alloc.state, block_count as _) };
        drop(alloc);
        result_from((), return_code)
    }

    pub fn grow(&self, block_count: usize) -> Result<()> {
        let mut alloc = self.alloc.borrow_mut();
        let return_code = unsafe { ll::lfs_fs_grow(&mut alloc.state, block_count as _) };
        drop(alloc);
        result_from((), return_code)
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
        let return_code = unsafe { ll::lfs_fs_size(&mut self.alloc.borrow_mut().state) };
        u32_result(return_code)
            .map(|blocks| usize::try_from(blocks).unwrap_or(usize::MAX))
            .map(|blocks| self.total_blocks().saturating_sub(blocks))
    }

    /// Available number of unused bytes in the filesystem
    ///
    /// This is a lower bound, more may be available. First, more blocks may be available as
    /// explained in [`available_blocks`](struct.Filesystem.html#method.available_blocks).
    /// Second, files may be inlined.
    pub fn available_space(&self) -> Result<usize> {
        self.available_blocks()
            .map(|blocks| blocks * Storage::BLOCK_SIZE)
    }

    /// Remove a file or directory.
    pub fn remove(&self, path: &Path) -> Result<()> {
        let return_code =
            unsafe { ll::lfs_remove(&mut self.alloc.borrow_mut().state, path.as_ptr()) };
        result_from((), return_code)
    }

    /// Remove a file or directory.
    pub fn remove_dir(&self, path: &Path) -> Result<()> {
        self.remove(path)
    }

    /// TODO: This method fails if some `println!` calls are removed.
    /// Whyy?
    pub fn remove_dir_all(&self, path: &Path) -> Result<()> {
        self.remove_dir_all_where(path, &|_| true).map(|_| ())
    }

    /// Returns number of deleted files + whether the directory was fully deleted or not
    fn remove_dir_all_where_inner<P>(
        &self,
        path: &Path,
        predicate: &P,
    ) -> Result<RemoveDirAllProgress>
    where
        P: Fn(&DirEntry) -> bool,
    {
        use crate::path;

        if !self.exists(path) {
            debug_now!("no such directory {}, early return", path);
            return Ok(RemoveDirAllProgress {
                files_removed: 0,
                skipped_any: false,
            });
        }
        let mut skipped_any = false;
        let mut files_removed = 0;
        debug_now!("starting to remove_dir_all_where in {}", path);
        self.read_dir_and_then(path, |read_dir| {
            // skip "." and ".."
            for entry in read_dir.skip(2) {
                let entry = entry?;

                if entry.file_type().is_file() {
                    if predicate(&entry) {
                        debug_now!("removing file {}", &entry.path());
                        self.remove(entry.path())?;
                        debug_now!("...done");
                        files_removed += 1;
                    } else {
                        debug_now!("skipping file {}", &entry.path());
                        skipped_any = true;
                    }
                }
                if entry.file_type().is_dir() {
                    debug_now!("recursing into directory {}", &entry.path());
                    let progress = self.remove_dir_all_where_inner(entry.path(), predicate)?;
                    files_removed += progress.files_removed;
                    skipped_any |= progress.skipped_any;
                    debug_now!("...back");
                }
            }
            Ok(())
        })?;
        if !skipped_any && path != path!("") && path != path!("/") {
            debug_now!("removing directory {} too", &path);
            self.remove_dir(path)?;
            debug_now!("..worked");
        }
        Ok(RemoveDirAllProgress {
            files_removed,
            skipped_any,
        })
    }

    pub fn remove_dir_all_where<P>(&self, path: &Path, predicate: &P) -> Result<usize>
    where
        P: Fn(&DirEntry) -> bool,
    {
        self.remove_dir_all_where_inner(path, predicate)
            .map(|progress| progress.files_removed)
    }

    /// Rename or move a file or directory.
    pub fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let return_code = unsafe {
            ll::lfs_rename(
                &mut self.alloc.borrow_mut().state,
                from.as_ptr(),
                to.as_ptr(),
            )
        };
        result_from((), return_code)
    }

    /// Check whether a file or directory exists at a path.
    ///
    /// This is equivalent to calling [`Filesystem::metadata`][] and checking for an `Ok` return
    /// value.
    pub fn exists(&self, path: &Path) -> bool {
        self.metadata(path).is_ok()
    }

    /// Given a path, query the filesystem to get information about a file or directory.
    ///
    /// To read user attributes, use
    /// [`Filesystem::attribute`](struct.Filesystem.html#method.attribute)
    pub fn metadata(&self, path: &Path) -> Result<Metadata> {
        // do *not* not call assume_init here and pass into the unsafe block.
        // strange things happen ;)

        // TODO: Check we don't have UB here *too*.
        // I think it's fine, as we immediately copy out the data
        // to our own structure.
        let mut info: ll::lfs_info = unsafe { mem::MaybeUninit::zeroed().assume_init() };
        let return_code =
            unsafe { ll::lfs_stat(&mut self.alloc.borrow_mut().state, path.as_ptr(), &mut info) };

        result_from((), return_code).map(|_| metadata(info))
    }

    pub fn create_file_and_then<R>(
        &self,
        path: &Path,
        f: impl FnOnce(&File<'_, '_, Storage>) -> Result<R>,
    ) -> Result<R> {
        File::create_and_then(self, path, f)
    }

    pub fn open_file_and_then<R>(
        &self,
        path: &Path,
        f: impl FnOnce(&File<'_, '_, Storage>) -> Result<R>,
    ) -> Result<R> {
        File::open_and_then(self, path, f)
    }

    pub fn with_options() -> OpenOptions {
        OpenOptions::new()
    }

    pub fn open_file_with_options_and_then<R>(
        &self,
        o: impl FnOnce(&mut OpenOptions) -> &OpenOptions,
        path: &Path,
        f: impl FnOnce(&File<'_, '_, Storage>) -> Result<R>,
    ) -> Result<R> {
        let mut options = OpenOptions::new();
        o(&mut options).open_and_then(self, path, f)
    }

    /// Read attribute.
    pub fn attribute<'a>(
        &self,
        path: &Path,
        id: u8,
        buffer: &'a mut [u8],
    ) -> Result<Option<Attribute<'a>>> {
        let n = u32::try_from(buffer.len()).unwrap_or(u32::MAX);

        let return_code = unsafe {
            ll::lfs_getattr(
                &mut self.alloc.borrow_mut().state,
                path.as_ptr(),
                id,
                buffer as *mut _ as *mut c_void,
                n,
            )
        };

        u32_result(return_code)
            .map(|n| {
                let total_size = usize::try_from(n).unwrap_or(usize::MAX);
                Some(Attribute::new(buffer, total_size))
            })
            .or_else(|err| {
                if err == Error::NO_ATTRIBUTE {
                    Ok(None)
                } else {
                    Err(err)
                }
            })
    }

    /// Remove attribute.
    pub fn remove_attribute(&self, path: &Path, id: u8) -> Result<()> {
        let return_code =
            unsafe { ll::lfs_removeattr(&mut self.alloc.borrow_mut().state, path.as_ptr(), id) };
        result_from((), return_code)
    }

    /// Set attribute.
    pub fn set_attribute(&self, path: &Path, id: u8, data: &[u8]) -> Result<()> {
        let return_code = unsafe {
            ll::lfs_setattr(
                &mut self.alloc.borrow_mut().state,
                path.as_ptr(),
                id,
                data as *const _ as *const c_void,
                u32::try_from(data.len()).unwrap_or(u32::MAX),
            )
        };

        result_from((), return_code)
    }

    /// C callback interface used by LittleFS to read data with the lower level system below the
    /// filesystem.
    extern "C" fn lfs_config_read(
        c: *const ll::lfs_config,
        block: ll::lfs_block_t,
        off: ll::lfs_off_t,
        buffer: *mut c_void,
        size: ll::lfs_size_t,
    ) -> c_int {
        // println!("in lfs_config_read for {} bytes", size);
        let storage = unsafe { &mut *((*c).context as *mut Storage) };
        debug_assert!(!c.is_null());
        let block_size = unsafe { c.read().block_size };
        let off = (block * block_size + off) as usize;
        let buf: &mut [u8] = unsafe { slice::from_raw_parts_mut(buffer as *mut u8, size as usize) };

        error_code_from(storage.read(off, buf))
    }

    /// C callback interface used by LittleFS to program data with the lower level system below the
    /// filesystem.
    extern "C" fn lfs_config_prog(
        c: *const ll::lfs_config,
        block: ll::lfs_block_t,
        off: ll::lfs_off_t,
        buffer: *const c_void,
        size: ll::lfs_size_t,
    ) -> c_int {
        // println!("in lfs_config_prog");
        let storage = unsafe { &mut *((*c).context as *mut Storage) };
        debug_assert!(!c.is_null());
        // let block_size = unsafe { c.read().block_size };
        let block_size = Storage::BLOCK_SIZE as u32;
        let off = (block * block_size + off) as usize;
        let buf: &[u8] = unsafe { slice::from_raw_parts(buffer as *const u8, size as usize) };

        error_code_from(storage.write(off, buf))
    }

    /// C callback interface used by LittleFS to erase data with the lower level system below the
    /// filesystem.
    extern "C" fn lfs_config_erase(c: *const ll::lfs_config, block: ll::lfs_block_t) -> c_int {
        // println!("in lfs_config_erase");
        let storage = unsafe { &mut *((*c).context as *mut Storage) };
        let off = block as usize * Storage::BLOCK_SIZE;

        error_code_from(storage.erase(off, Storage::BLOCK_SIZE))
    }

    /// C callback interface used by LittleFS to sync data with the lower level interface below the
    /// filesystem. Note that this function currently does nothing.
    extern "C" fn lfs_config_sync(_c: *const ll::lfs_config) -> c_int {
        // println!("in lfs_config_sync");
        // Do nothing; we presume that data is synchronized.
        0
    }
}

/// The state of a `File`. Pre-allocate with `File::allocate`.
pub struct FileAllocation<S: driver::Storage> {
    cache: UnsafeCell<Bytes<S::CACHE_SIZE>>,
    state: ll::lfs_file_t,
    config: ll::lfs_file_config,
}

impl<S: driver::Storage> Default for FileAllocation<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: driver::Storage> FileAllocation<S> {
    pub fn new() -> Self {
        let cache_size: u32 = <S as driver::Storage>::CACHE_SIZE::to_u32();
        debug_assert!(cache_size > 0);
        unsafe { mem::MaybeUninit::zeroed().assume_init() }
    }
}

pub struct File<'a, 'b, S: driver::Storage> {
    // We must store a raw pointer here since the FFI retains a copy of a pointer
    // to the field alloc.state, so we cannot assert unique mutable access.
    alloc: RefCell<*mut FileAllocation<S>>,
    fs: &'b Filesystem<'a, S>,
}

impl<'a, 'b, Storage: driver::Storage> File<'a, 'b, Storage> {
    pub fn allocate() -> FileAllocation<Storage> {
        FileAllocation::new()
    }

    /// Returns a new OpenOptions object.
    ///
    /// This function returns a new OpenOptions object that you can use to open or create a file
    /// with specific options if open() or create() are not appropriate.
    ///
    /// It is equivalent to OpenOptions::new() but allows you to write more readable code.
    /// This also avoids the need to import OpenOptions`.
    pub fn with_options() -> OpenOptions {
        OpenOptions::new()
    }

    pub unsafe fn open(
        fs: &'b Filesystem<'a, Storage>,
        alloc: &'b mut FileAllocation<Storage>,
        path: &Path,
    ) -> Result<Self> {
        OpenOptions::new().read(true).open(fs, alloc, path)
    }

    pub fn open_and_then<R>(
        fs: &Filesystem<'a, Storage>,
        path: &Path,
        f: impl FnOnce(&File<'_, '_, Storage>) -> Result<R>,
    ) -> Result<R> {
        OpenOptions::new().read(true).open_and_then(fs, path, f)
    }

    pub unsafe fn create(
        fs: &'b Filesystem<'a, Storage>,
        alloc: &'b mut FileAllocation<Storage>,
        path: &Path,
    ) -> Result<Self> {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(fs, alloc, path)
    }

    pub fn create_and_then<R>(
        fs: &Filesystem<'a, Storage>,
        path: &Path,
        f: impl FnOnce(&File<'_, '_, Storage>) -> Result<R>,
    ) -> Result<R> {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open_and_then(fs, path, f)
    }

    // Safety-hatch to experiment with missing parts of API
    pub unsafe fn borrow_filesystem<'c>(&'c mut self) -> &'c Filesystem<'a, Storage> {
        self.fs
    }

    /// Sync the file and drop it from the internal linked list.
    /// Not doing this is UB, which is why we have all the closure-based APIs.
    ///
    /// This must not be called twice.
    pub unsafe fn close(self) -> Result<()> {
        let return_code = ll::lfs_file_close(
            &mut self.fs.alloc.borrow_mut().state,
            // We need to use addr_of_mut! here instead of & mut since
            // the FFI stores a copy of a pointer to the field state,
            // so we cannot assert unique mutable access.
            addr_of_mut!((*(*self.alloc.borrow_mut())).state),
        );
        result_from((), return_code)
    }

    /// Synchronize file contents to storage.
    pub fn sync(&self) -> Result<()> {
        let return_code = unsafe {
            // We need to use addr_of_mut! here instead of & mut since
            // the FFI stores a copy of a pointer to the field state,
            // so we cannot assert unique mutable access.
            ll::lfs_file_sync(
                &mut self.fs.alloc.borrow_mut().state,
                addr_of_mut!((*(*self.alloc.borrow_mut())).state),
            )
        };
        result_from((), return_code)
    }

    /// Size of the file in bytes.
    pub fn len(&self) -> Result<usize> {
        let return_code = unsafe {
            // We need to use addr_of_mut! here instead of & mut since
            // the FFI stores a copy of a pointer to the field state,
            // so we cannot assert unique mutable access.
            ll::lfs_file_size(
                &mut self.fs.alloc.borrow_mut().state,
                addr_of_mut!((*(*self.alloc.borrow_mut())).state),
            )
        };
        u32_result(return_code).map(|n| n as usize)
    }

    pub fn is_empty(&self) -> Result<bool> {
        self.len().map(|l| l == 0)
    }

    /// Truncates or extends the underlying file, updating the size of this file to become size.
    ///
    /// If the size is less than the current file's size, then the file will be shrunk. If it is
    /// greater than the current file's size, then the file will be extended to size and have all
    /// of the intermediate data filled in with 0s.
    pub fn set_len(&self, size: usize) -> Result<()> {
        let return_code = unsafe {
            // We need to use addr_of_mut! here instead of & mut since
            // the FFI stores a copy of a pointer to the field state,
            // so we cannot assert unique mutable access.
            ll::lfs_file_truncate(
                &mut self.fs.alloc.borrow_mut().state,
                addr_of_mut!((*(*self.alloc.borrow_mut())).state),
                size as u32,
            )
        };
        result_from((), return_code)
    }

    // This belongs in `io::Read` but really don't want that to have a generic parameter
    pub fn read_to_end<const N: usize>(&self, buf: &mut heapless::Vec<u8, N>) -> Result<usize> {
        // My understanding of
        // https://github.com/littlefs-project/littlefs/blob/4c9146ea539f72749d6cc3ea076372a81b12cb11/lfs.c#L2816
        // is that littlefs keeps reading until either the buffer is full, or the file is exhausted

        let had = buf.len();
        // no panic by construction
        buf.resize_default(buf.capacity()).unwrap();
        // use io::Read;
        let read = self.read(&mut buf[had..])?;
        // no panic by construction
        buf.resize_default(had + read).unwrap();
        Ok(read)
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        <Self as io::Read>::read(self, buf)
    }

    pub fn seek(&self, pos: io::SeekFrom) -> Result<usize> {
        <Self as io::Seek>::seek(self, pos)
    }

    pub fn write(&self, buf: &[u8]) -> Result<usize> {
        <Self as io::Write>::write(self, buf)
    }
}

/// Options and flags which can be used to configure how a file is opened.
///
/// This builder exposes the ability to configure how a File is opened and what operations
/// are permitted on the open file. The File::open and File::create methods are aliases
/// for commonly used options using this builder.
///
/// Consider `File::with_options()` to avoid having to `use` OpenOptions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenOptions(FileOpenFlags);

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
        alloc: &mut FileAllocation<S>,
        path: &Path,
    ) -> Result<File<'a, 'b, S>> {
        alloc.config.buffer = alloc.cache.get() as *mut _;
        // We need to use addr_of_mut! here instead of & mut since
        // the FFI stores a copy of a pointer to the field state,
        // so we cannot assert unique mutable access.
        let return_code = ll::lfs_file_opencfg(
            &mut fs.alloc.borrow_mut().state,
            addr_of_mut!(alloc.state),
            path.as_ptr(),
            self.0.bits(),
            addr_of!(alloc.config),
        );

        let file = File {
            alloc: RefCell::new(alloc),
            fs,
        };

        result_from(file, return_code)
    }

    /// (Hopefully) safe abstraction around `open`.
    pub fn open_and_then<'a, R, S: driver::Storage>(
        &self,
        fs: &Filesystem<'a, S>,
        path: &Path,
        f: impl FnOnce(&File<'a, '_, S>) -> Result<R>,
    ) -> Result<R> {
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
}

impl From<FileOpenFlags> for OpenOptions {
    fn from(flags: FileOpenFlags) -> Self {
        Self(flags)
    }
}

impl<S: driver::Storage> io::Read for File<'_, '_, S> {
    fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let return_code = unsafe {
            // We need to use addr_of_mut! here instead of & mut since
            // the FFI stores a copy of a pointer to the field state,
            // so we cannot assert unique mutable access.
            ll::lfs_file_read(
                &mut self.fs.alloc.borrow_mut().state,
                addr_of_mut!((*(*self.alloc.borrow_mut())).state),
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as u32,
            )
        };
        u32_result(return_code).map(|n| n as usize)
    }
}

impl<S: driver::Storage> io::Seek for File<'_, '_, S> {
    fn seek(&self, pos: io::SeekFrom) -> Result<usize> {
        let return_code = unsafe {
            // We need to use addr_of_mut! here instead of & mut since
            // the FFI stores a copy of a pointer to the field state,
            // so we cannot assert unique mutable access.
            ll::lfs_file_seek(
                &mut self.fs.alloc.borrow_mut().state,
                addr_of_mut!((*(*self.alloc.borrow_mut())).state),
                pos.off(),
                pos.whence(),
            )
        };
        u32_result(return_code).map(|n| n as usize)
    }
}

impl<S: driver::Storage> io::Write for File<'_, '_, S> {
    fn write(&self, buf: &[u8]) -> Result<usize> {
        let return_code = unsafe {
            // We need to use addr_of_mut! here instead of & mut since
            // the FFI stores a copy of a pointer to the field state,
            // so we cannot assert unique mutable access.
            ll::lfs_file_write(
                &mut self.fs.alloc.borrow_mut().state,
                addr_of_mut!((*(*self.alloc.borrow_mut())).state),
                buf.as_ptr() as *const c_void,
                buf.len() as u32,
            )
        };
        u32_result(return_code).map(|n| n as usize)
    }

    fn flush(&self) -> Result<()> {
        Ok(())
    }
}

pub struct ReadDirAllocation {
    state: ll::lfs_dir_t,
}

impl Default for ReadDirAllocation {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadDirAllocation {
    pub fn new() -> Self {
        unsafe { mem::MaybeUninit::zeroed().assume_init() }
    }
}

pub struct ReadDir<'a, 'b, S: driver::Storage> {
    // We must store a raw pointer here since the FFI retains a copy of a pointer
    // to the field alloc.state, so we cannot assert unique mutable access.
    alloc: RefCell<*mut ReadDirAllocation>,
    fs: &'b Filesystem<'a, S>,
    path: &'b Path,
}

impl<S: driver::Storage> Iterator for ReadDir<'_, '_, S> {
    type Item = Result<DirEntry>;

    // remove this allowance again, once path overflow is properly handled
    #[allow(unreachable_code)]
    fn next(&mut self) -> Option<Self::Item> {
        let mut info: ll::lfs_info = unsafe { mem::MaybeUninit::zeroed().assume_init() };
        // We need to use addr_of_mut! here instead of & mut since
        // the FFI stores a copy of a pointer to the field state,
        // so we cannot assert unique mutable access.
        let return_code = unsafe {
            ll::lfs_dir_read(
                &mut self.fs.alloc.borrow_mut().state,
                addr_of_mut!((*(*self.alloc.borrow_mut())).state),
                &mut info,
            )
        };

        if return_code > 0 {
            let file_name = unsafe { PathBuf::from_buffer_unchecked(info.name) };
            let metadata = metadata(info);

            let path = self.path.join(&file_name);

            let dir_entry = DirEntry::new(file_name, metadata, path);
            return Some(Ok(dir_entry));
        }

        if return_code == 0 {
            return None;
        }

        Some(Err(result_from((), return_code).unwrap_err()))
    }
}

impl<'a, S: driver::Storage> ReadDir<'a, '_, S> {
    // Safety-hatch to experiment with missing parts of API
    pub unsafe fn borrow_filesystem<'b>(&'b mut self) -> &'b Filesystem<'a, S> {
        self.fs
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
    pub fn close(self) -> Result<()> {
        let return_code = unsafe {
            // We need to use addr_of_mut! here instead of & mut since
            // the FFI stores a copy of a pointer to the field state,
            // so we cannot assert unique mutable access.
            ll::lfs_dir_close(
                &mut self.fs.alloc.borrow_mut().state,
                addr_of_mut!((*(*self.alloc.borrow_mut())).state),
            )
        };
        result_from((), return_code)
    }
}

impl<'a, Storage: driver::Storage> Filesystem<'a, Storage> {
    pub fn read_dir_and_then<R>(
        &self,
        path: &Path,
        // *not* &ReadDir, as Iterator takes &mut
        f: impl FnOnce(&mut ReadDir<'_, '_, Storage>) -> Result<R>,
    ) -> Result<R> {
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
        path: &'b Path,
    ) -> Result<ReadDir<'a, 'b, Storage>> {
        // ll::lfs_dir_open stores a copy of the pointer to alloc.state, so
        // we must use addr_of_mut! here, since &mut alloc.state asserts unique
        // mutable access, and we need shared mutable access.
        let return_code = ll::lfs_dir_open(
            &mut self.alloc.borrow_mut().state,
            addr_of_mut!(alloc.state),
            path.as_ptr(),
        );

        let read_dir = ReadDir {
            alloc: RefCell::new(alloc),
            fs: self,
            path,
        };

        result_from(read_dir, return_code)
    }
}

impl<'a, Storage: driver::Storage> Filesystem<'a, Storage> {
    pub fn mount(alloc: &'a mut Allocation<Storage>, storage: &'a mut Storage) -> Result<Self> {
        let fs = Self::new(alloc, storage);
        fs.raw_mount()?;
        Ok(fs)
    }

    fn set_alloc_config(alloc: &mut Allocation<Storage>, storage: &mut Storage) {
        alloc.config.context = storage as *mut _ as *mut c_void;
        alloc.config.read_buffer = alloc.cache.read.get() as *mut c_void;
        alloc.config.prog_buffer = alloc.cache.write.get() as *mut c_void;
        alloc.config.lookahead_buffer = alloc.cache.lookahead.get() as *mut c_void;
    }

    /// Mount the filesystem or, if that fails, call `f` with the mount error and the storage and then try again.
    pub fn mount_or_else<F>(
        alloc: &'a mut Allocation<Storage>,
        storage: &'a mut Storage,
        f: F,
    ) -> Result<Self>
    where
        F: FnOnce(Error, &mut Storage, &mut Allocation<Storage>) -> Result<()>,
    {
        let mut fs = Self::new(alloc, storage);
        if let Err(err) = fs.raw_mount() {
            let alloc = fs.alloc.get_mut();
            f(err, fs.storage, alloc)?;
            Self::set_alloc_config(alloc, fs.storage);
            fs.raw_mount()?;
        }
        Ok(fs)
    }

    fn raw_mount(&self) -> Result<()> {
        let mut alloc = self.alloc.borrow_mut();
        let return_code = unsafe { ll::lfs_mount(&mut alloc.state, &alloc.config) };
        drop(alloc);
        result_from((), return_code)
    }

    // Not public, user should use `mount`, possibly after `format`
    fn new(alloc: &'a mut Allocation<Storage>, storage: &'a mut Storage) -> Self {
        Self::set_alloc_config(alloc, storage);
        Filesystem {
            alloc: RefCell::new(alloc),
            storage,
        }
    }

    /// Deconstruct `Filesystem`, intention is to allow access to
    /// the underlying Flash peripheral in driver::Storage etc.
    ///
    /// See also `borrow_storage_mut`.
    pub fn into_inner(self) -> (&'a mut Allocation<Storage>, &'a mut Storage) {
        (self.alloc.into_inner(), self.storage)
    }

    /// Creates a new, empty directory at the provided path.
    pub fn create_dir(&self, path: &Path) -> Result<()> {
        #[cfg(test)]
        println!("creating {:?}", path);
        let return_code =
            unsafe { ll::lfs_mkdir(&mut self.alloc.borrow_mut().state, path.as_ptr()) };
        result_from((), return_code)
    }

    /// Recursively create a directory and all of its parent components if they are missing.
    pub fn create_dir_all(&self, path: &Path) -> Result<()> {
        // Placeholder implementation!
        // - Path should gain a few methods
        // - Maybe should pull in `heapless-bytes` (and merge upstream into `heapless`)
        // - All kinds of sanity checks and possible logic errors possible...

        let path_slice = path.as_ref().as_bytes();
        for i in 0..path_slice.len() {
            if path_slice[i] == b'/' {
                let dir = PathBuf::try_from(&path_slice[..i]).map_err(|_| Error::IO)?;
                #[cfg(test)]
                println!("generated PathBuf dir {:?} using i = {}", &dir, i);
                if let Err(error) = self.create_dir(&dir) {
                    if error != Error::ENTRY_ALREADY_EXISTED {
                        return Err(error);
                    }
                }
            }
        }
        if let Err(error) = self.create_dir(path) {
            if error != Error::ENTRY_ALREADY_EXISTED {
                return Err(error);
            }
        }
        Ok(())

        // if path.as_ref() == "" {
        //     return Ok(());
        // }

        // match self.create_dir(path) {
        //     Ok(()) => return Ok(()),
        //     Err(_) if path.is_dir() => return Ok(()),
        //     Err(e) => return Err(e),
        // }

        // match path.parent() {
        //     Some(p) => self.create_dir(p)?,
        //     None => panic!("unexpected"),
        // }

        // match self.create_dir(path) {
        //     Ok(()) => return Ok(()),
        //     Err(e) => return Err(e),
        // }
    }

    /// Read the entire contents of a file into a bytes vector.
    pub fn read<const N: usize>(&self, path: &Path) -> Result<heapless::Vec<u8, N>> {
        let mut contents: heapless::Vec<u8, N> = Default::default();
        File::open_and_then(self, path, |file| {
            // use io::Read;
            let len = file.read_to_end(&mut contents)?;
            Ok(len)
        })?;
        Ok(contents)
    }

    /// Read a chunk of a file into a bytes vector
    /// Returns the data and the size of the file
    pub fn read_chunk<const N: usize>(
        &self,
        path: &Path,
        pos: OpenSeekFrom,
    ) -> Result<(heapless::Vec<u8, N>, usize)> {
        let mut contents: heapless::Vec<u8, N> = Default::default();
        contents.resize_default(contents.capacity()).unwrap();
        let file_len = File::open_and_then(self, path, |file| {
            file.seek(pos.into())?;
            let read_n = file.read(&mut contents)?;
            contents.truncate(read_n);
            file.len()
        })?;
        Ok((contents, file_len))
    }

    /// Write a slice as the entire contents of a file.
    ///
    /// This function will create a file if it does not exist,
    /// and will entirely replace its contents if it does.
    pub fn write(&self, path: &Path, contents: &[u8]) -> Result<()> {
        #[cfg(test)]
        println!("writing {:?}", path);
        File::create_and_then(self, path, |file| {
            use io::Write;
            file.write_all(contents)
        })?;
        Ok(())
    }

    /// Write a slice as a chunk of a file.
    ///
    /// This function will not create a file if it does not exist,
    /// it will fail if the file is not already large enough with regard to the `pos` parameter
    pub fn write_chunk(&self, path: &Path, contents: &[u8], pos: OpenSeekFrom) -> Result<()> {
        #[cfg(test)]
        println!("writing {:?}", path);
        OpenOptions::new()
            .read(true)
            .write(true)
            .truncate(false)
            .open_and_then(self, path, |file| {
                use io::Write;
                file.seek(pos.into())?;
                file.write_all(contents)
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path;
    use core::convert::TryInto;
    const_ram_storage!(TestStorage, 4096);

    #[test]
    fn disk_version() {
        let mut test_storage = TestStorage::new();
        Filesystem::format(&mut test_storage).unwrap();
        Filesystem::mount_and_then(&mut test_storage, |fs| {
            let mut fs_info = ll::lfs_fsinfo {
                disk_version: 0,
                block_size: 0,
                block_count: 0,
                name_max: 0,
                file_max: 0,
                attr_max: 0,
            };
            let return_code =
                unsafe { ll::lfs_fs_stat(&mut fs.alloc.borrow_mut().state, &mut fs_info) };
            result_from((), return_code).unwrap();
            assert_eq!(fs_info.disk_version, DISK_VERSION.into());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn todo() {
        let mut test_storage = TestStorage::new();
        // let jackson5 = [b"A", b"B", b"C", 1, 2, 3];
        let jackson5 = b"ABC 123";
        let jackson5 = &jackson5[..];

        Filesystem::format(&mut test_storage).unwrap();
        Filesystem::mount_and_then(&mut test_storage, |fs| {
            println!("blocks going in: {}", fs.available_blocks()?);
            fs.create_dir_all(b"/tmp/test\0".try_into().unwrap())?;
            println!("dir done");
            // let weird_filename = b"/tmp/test/a.t\x7fxt";
            // fs.write(&weird_filename[..], jackson5)?;
            fs.write(b"/tmp/test/a.txt\0".try_into().unwrap(), jackson5)?;
            println!("a.txt");
            fs.write(b"/tmp/test/b.txt\0".try_into().unwrap(), jackson5)?;
            fs.write(b"/tmp/test/c.txt\0".try_into().unwrap(), jackson5)?;
            println!("blocks after 3 files of size 3: {}", fs.available_blocks()?);

            // Not only does this need "unsafe", but also the compiler catches
            // the double-call of `file.close` (here, and in the closure teardown).
            //
            // File::create_and_then(&mut fs, "/tmp/zzz", |file| {
            //     unsafe { file.close() }
            // }).unwrap();

            fs.read_dir_and_then(b"/\0".try_into().unwrap(), |read_dir| {
                for entry in read_dir {
                    let entry = entry?;
                    println!("{:?} --> path = {:?}", entry.file_name(), entry.path());
                }
                Ok(())
            })?;

            fs.read_dir_and_then(b"/tmp\0".try_into().unwrap(), |read_dir| {
                for entry in read_dir {
                    println!("entry: {:?}", entry?.file_name());
                }
                Ok(())
            })?;

            fs.read_dir_and_then(b"/tmp/test\0".try_into().unwrap(), |read_dir| {
                for entry in read_dir {
                    let entry = entry?;
                    println!("entry: {:?}", entry.file_name());
                    println!("path: {:?}", entry.path());

                    let attribute: &[u8] = if entry.file_type().is_dir() {
                        b"directory alarm"
                    } else {
                        // not 100% sure this is allowed, but if seems to work :)
                        fs.write(entry.path(), b"Alles neu macht n\xc3\xa4chstens der Mai")?;
                        b"ceci n'est pas une pipe"
                    };
                    fs.set_attribute(entry.path(), 37, attribute)?;
                }
                Ok(())
            })?;

            fs.read_dir_and_then(b"/tmp/test\0".try_into().unwrap(), |read_dir| {
                for (i, entry) in read_dir.enumerate() {
                    let entry = entry?;
                    println!("\nfile {}: {:?}", i, entry.file_name());

                    if entry.file_type().is_file() {
                        let content: heapless::Vec<u8, 256> = fs.read(entry.path())?;
                        println!("content:\n{:?}", core::str::from_utf8(&content).unwrap());
                        // println!("and now the removal");
                        // fs.remove(entry.path())?;
                    }

                    let mut buffer = [0; Attribute::MAX_SIZE as _];
                    if let Some(attribute) = fs.attribute(entry.path(), 37, &mut buffer)? {
                        println!(
                            "attribute 37: {:?}",
                            core::str::from_utf8(attribute.data()).unwrap()
                        );
                    }

                    // deleting (self) file while iterating!
                    if entry.file_type().is_file() {
                        println!("removing {:?}", entry.path());
                        fs.remove(entry.path())?;
                    }

                    // // WE CANNOT REMOVE THE NEXT FILE
                    // // can we `remove` the "next" file?
                    // if entry.file_name() == "b.txt"{
                    //     println!("deleting c.txt");
                    //     fs.remove(&PathBuf::from(b"/tmp/test/c.txt\0"))?;
                    // }

                    // adding file while iterating!
                    if i == 1 {
                        println!("writing new file");
                        fs.write(b"/tmp/test/out-of-nowhere.txt\0".try_into().unwrap(), &[])?;
                    }
                }
                Ok(())
            })?;

            println!("\nDELETION SPREE\n");
            // behaves veeryweirldy
            // (...)
            // entry = DirEntry { file_name: "test", metadata: Metadata { file_type: Dir, size: 0 }, path: "/tmp\u{0}/test" }
            // (...)
            // fs.remove_dir_all(&PathBuf::from(b"/tmp\0"))?;
            // fs.remove_dir_all(&PathBuf::from(b"/tmp"))?;
            fs.remove_dir_all(path!("/tmp"))?;

            Ok(())
        })
        .unwrap();

        let mut alloc = Allocation::new();
        let fs = Filesystem::mount(&mut alloc, &mut test_storage).unwrap();
        // fs.write(b"/z.txt\0".try_into().unwrap(), &jackson5).unwrap();
        fs.write(path!("z.txt"), jackson5).unwrap();
    }

    #[test]
    fn remove_dir_all() {
        let mut test_storage = TestStorage::new();
        let jackson5 = b"ABC 123";
        let jackson5 = &jackson5[..];

        Filesystem::format(&mut test_storage).unwrap();
        Filesystem::mount_and_then(&mut test_storage, |fs| {
            fs.create_dir_all(b"/tmp/test\0".try_into().unwrap())?;
            fs.write(b"/tmp/test/a.txt\0".try_into().unwrap(), jackson5)?;
            fs.write(b"/tmp/test/b.txt\0".try_into().unwrap(), jackson5)?;
            fs.write(b"/tmp/test/c.txt\0".try_into().unwrap(), jackson5)?;

            println!("\nDELETION SPREE\n");
            fs.remove_dir_all(b"/tmp\0".try_into().unwrap())?;

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn path() {
        let _path: &Path = b"a.txt\0".try_into().unwrap();
    }

    #[test]
    fn open_file_with_options_and_then() {
        let mut test_storage = TestStorage::new();
        Filesystem::format(&mut test_storage).unwrap();
        Filesystem::mount_and_then(&mut test_storage, |fs| {
            let filename = b"append.to.me\0".try_into().unwrap();
            fs.write(filename, b"first part")?;

            fs.open_file_with_options_and_then(
                |options| options.write(true).create(false).truncate(false),
                filename,
                |file| {
                    // this is a bit of a pitfall :)
                    file.seek(io::SeekFrom::End(0))?;
                    file.write(b" - ")?;
                    file.write(b"second part")?;

                    Ok(())
                },
            )?;

            let content: heapless::Vec<_, 256> = fs.read(filename)?;
            assert_eq!(content, b"first part - second part");
            // println!("content: {:?}", core::str::from_utf8(&content).unwrap());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn nested() {
        let mut test_storage = TestStorage::new();

        Filesystem::format(&mut test_storage).unwrap();
        Filesystem::mount_and_then(&mut test_storage, |fs| {
            fs.write(b"a.txt\0".try_into().unwrap(), &[])?;
            fs.write(b"b.txt\0".try_into().unwrap(), &[])?;
            fs.write(b"c.txt\0".try_into().unwrap(), &[])?;

            fs.read_dir_and_then(b".\0".try_into().unwrap(), |read_dir| {
                for entry in read_dir {
                    let entry = entry?;
                    println!("{:?}", entry.file_name());

                    // The `&mut ReadDir` is not actually available here
                    // Do we want a way to borrow_filesystem for DirEntry?
                    // One usecase is to read data from the files iterated over.
                    //
                    if entry.metadata().is_file() {
                        fs.write(entry.file_name(), b"wowee zowie")?;
                    }
                }
                Ok(())
            })?;

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn issue_3_original_report() {
        let mut test_storage = TestStorage::new();

        Filesystem::format(&mut test_storage).unwrap();
        Filesystem::mount_and_then(&mut test_storage, |fs| {
            fs.write(b"a.txt\0".try_into().unwrap(), &[])?;
            fs.write(b"b.txt\0".try_into().unwrap(), &[])?;
            fs.write(b"c.txt\0".try_into().unwrap(), &[])?;

            // works fine
            fs.read_dir_and_then(b".\0".try_into().unwrap(), |read_dir| {
                for entry in read_dir {
                    let entry = entry?;
                    println!("{:?}", entry.file_type());
                }
                Ok(())
            })?;

            let mut a1 = File::allocate();
            let f1 = unsafe { File::create(fs, &mut a1, b"a.txt\0".try_into().unwrap())? };
            f1.write(b"some text")?;

            let mut a2 = File::allocate();
            let f2 = unsafe { File::create(fs, &mut a2, b"b.txt\0".try_into().unwrap())? };
            f2.write(b"more text")?;

            unsafe { f1.close()? }; // program hangs here
            unsafe { f2.close()? }; // this statement is never reached

            Ok(())
        })
        .unwrap();
    }
}
