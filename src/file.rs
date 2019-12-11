use littlefs2_sys as lfs;

use crate::{
    LittleFs,
    mount_state,
    storage,
    error::{
        Error,
        Result,
    },
};

use generic_array::{
    ArrayLength,
    GenericArray,
    typenum::marker_traits::Unsigned as _,
};

use core::{
    // marker::PhantomData,
    cmp,
    mem,
    slice,
};

pub struct FileAllocation<Storage>
where
    Storage: storage::Storage,
    <Storage as storage::Storage>::CACHE_SIZE: ArrayLength<u8>,
{
    cache: GenericArray<u8, Storage::CACHE_SIZE>,
    state: lfs::lfs_file_t,
    config: lfs::lfs_file_config,
}

pub struct File<'alloc, Storage>
where
    Storage: storage::Storage,
    Storage: 'alloc,
    <Storage as storage::Storage>::CACHE_SIZE: ArrayLength<u8>,
{
    alloc: &'alloc mut FileAllocation<Storage>,
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

impl<'alloc, Storage> File<'alloc, Storage>
where
    Storage: storage::Storage,
    Storage: 'alloc,
    <Storage as storage::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as storage::Storage>::FILENAME_MAX: ArrayLength<u8>,
    <Storage as storage::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    pub fn allocate() -> FileAllocation<Storage> {
        // TODO: more checks
        let cache_size: u32 = <Storage as storage::Storage>::CACHE_SIZE::to_u32();
        debug_assert!(cache_size > 0);

        let config = lfs::lfs_file_config {
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

    // in `std::fs::File`:
    // pub fn open<P: AsRef<Path>>(path: P) -> Result<File>
    // Attempts to open a file in read-only mode.
    // pub fn open<P: >(path: P, cache: ...
    pub fn open(
        path: &str,
        alloc: &'alloc mut FileAllocation<Storage>,
        littlefs: &mut LittleFs<'alloc, Storage, mount_state::Mounted>,
        storage: &mut Storage,
    ) ->
        Result<Self>
    {
        alloc.config.buffer = alloc.cache.as_mut_slice() as *mut _ as *mut cty::c_void;

        let mut file = File {
            alloc,
        };

        // let mut cstr_path = [0u8; Storage::FILENAME_MAX];
        let mut cstr_path: GenericArray<u8, Storage::FILENAME_MAX> = Default::default();
        let name_max = <Storage as storage::Storage>::FILENAME_MAX::to_usize();
        let len = cmp::min(name_max - 1, path.len());
        cstr_path[..len].copy_from_slice(&path.as_bytes()[..len]);

        let return_code = unsafe { lfs::lfs_file_opencfg(
                &mut littlefs.alloc.state,
                &mut file.alloc.state,
                cstr_path.as_ptr() as *const cty::c_char,
                (FileOpenFlags::RDONLY).bits() as i32,
                &file.alloc.config,
        ) };

        Error::empty_from(return_code)?;

        Ok(file)
    }

    pub fn create(
        path: &str,
        alloc: &'alloc mut FileAllocation<Storage>,
        littlefs: &mut LittleFs<'alloc, Storage, mount_state::Mounted>,
        storage: &mut Storage,
    ) ->
        Result<Self>
    {
        alloc.config.buffer = alloc.cache.as_mut_slice() as *mut _ as *mut cty::c_void;

        let mut file = File {
            alloc,
        };

        let mut cstr_path: GenericArray<u8, Storage::FILENAME_MAX> = Default::default();
        let name_max = <Storage as storage::Storage>::FILENAME_MAX::to_usize();
        let len = cmp::min(name_max - 1, path.len());
        cstr_path[..len].copy_from_slice(&path.as_bytes()[..len]);

        let return_code = unsafe { lfs::lfs_file_opencfg(
                &mut littlefs.alloc.state,
                &mut file.alloc.state,
                cstr_path.as_ptr() as *const cty::c_char,
                (FileOpenFlags::WRONLY | FileOpenFlags::TRUNC | FileOpenFlags::CREAT).bits() as i32,
                &file.alloc.config,
        ) };

        Error::empty_from(return_code)?;

        Ok(file)
    }

    // // Opens a file in write-only mode.
    // // This function will create a file if it does not exist, and will truncate it if it does
    // pub fn create(
    //     path:
    //     alloc: &'alloc mut FileAllocation<Storage>,
    //     storage: &mut Storage,
    // ) ->
    //     Self
    // {
    // }
}
