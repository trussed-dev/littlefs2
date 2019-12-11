use bitflags::bitflags;
use littlefs2_sys as ll;

use crate::{
    error::{
        Error,
        Result,
    },
    LittleFs,
    mount_state,
    traits,
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
    // slice,
};

pub struct FileAllocation<Storage>
where
    Storage: traits::Storage,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
{
    cache: GenericArray<u8, Storage::CACHE_SIZE>,
    state: ll::lfs_file_t,
    config: ll::lfs_file_config,
}

pub struct File<'alloc, Storage>
where
    Storage: traits::Storage,
    Storage: 'alloc,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
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
    Storage: traits::Storage,
    Storage: 'alloc,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::FILENAME_MAX: ArrayLength<u8>,
    <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    pub fn allocate() -> FileAllocation<Storage> {
        // TODO: more checks
        let cache_size: u32 = <Storage as traits::Storage>::CACHE_SIZE::to_u32();
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
        debug_assert!(littlefs.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        alloc.config.buffer = alloc.cache.as_mut_slice() as *mut _ as *mut cty::c_void;

        let file = File {
            alloc,
        };

        // let mut cstr_path = [0u8; Storage::FILENAME_MAX];
        let mut cstr_path: GenericArray<u8, Storage::FILENAME_MAX> = Default::default();
        let name_max = <Storage as traits::Storage>::FILENAME_MAX::to_usize();
        let len = cmp::min(name_max - 1, path.len());
        cstr_path[..len].copy_from_slice(&path.as_bytes()[..len]);

        let return_code = unsafe { ll::lfs_file_opencfg(
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
        debug_assert!(littlefs.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        alloc.config.buffer = alloc.cache.as_mut_slice() as *mut _ as *mut cty::c_void;

        let file = File {
            alloc,
        };

        let mut cstr_path: GenericArray<u8, Storage::FILENAME_MAX> = Default::default();
        let name_max = <Storage as traits::Storage>::FILENAME_MAX::to_usize();
        let len = cmp::min(name_max - 1, path.len());
        cstr_path[..len].copy_from_slice(&path.as_bytes()[..len]);

        let return_code = unsafe { ll::lfs_file_opencfg(
                &mut littlefs.alloc.state,
                &mut file.alloc.state,
                cstr_path.as_ptr() as *const cty::c_char,
                (FileOpenFlags::WRONLY | FileOpenFlags::TRUNC | FileOpenFlags::CREAT).bits() as i32,
                &file.alloc.config,
        ) };

        Error::empty_from(return_code)?;
        Ok(file)
    }

    /// Sync the file and drop it.
    /// NB: `std::fs` does not have this, just drops at end of scope.
    pub fn close(
        self,
        littlefs: &mut LittleFs<'alloc, Storage, mount_state::Mounted>,
        storage: &mut Storage,
    ) ->
        Result<()>
    {
        debug_assert!(littlefs.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        let return_code = unsafe { ll::lfs_file_close(
            &mut littlefs.alloc.state,
            &mut self.alloc.state,
        ) };
        Error::empty_from(return_code)?;
        Ok(())
    }

    /// Synchronize file contents to storage.
    pub fn sync(
        &mut self,
        littlefs: &mut LittleFs<'alloc, Storage, mount_state::Mounted>,
        storage: &mut Storage,
    ) ->
        Result<()>
    {
        debug_assert!(littlefs.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        let return_code = unsafe { ll::lfs_file_sync(
            &mut littlefs.alloc.state,
            &mut self.alloc.state,
        ) };
        Error::empty_from(return_code)?;
        Ok(())
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
