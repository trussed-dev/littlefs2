#![no_std]

use core::{
    // marker::PhantomData,
    mem,
    slice,
};

use generic_array::{
    ArrayLength,
    GenericArray,
    typenum::marker_traits::Unsigned,
};

use littlefs2_sys as lfs;

pub mod error;
pub use error::{
    Error,
    Result,
};

pub mod storage;
pub use storage::Storage as StorageTrait;

// pub fn littlefs_software_major_version() -> u32 {
//     lfs::LFS_VERSION_MAJOR
// }

// pub fn littlefs_software_minor_version() -> u32 {
//     lfs::LFS_VERSION_MINOR
// }

/// The three global buffers used by LittleFS
// #[derive(Debug)]
pub struct Buffers<Storage>
where
    Storage: storage::Storage,
    <Storage as storage::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as storage::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    read: GenericArray<u8, Storage::CACHE_SIZE>,
    write: GenericArray<u8, Storage::CACHE_SIZE>,
    // must be 4-byte aligned, hence the `u32`s
    lookahead: GenericArray<u32, Storage::LOOKAHEADWORDS_SIZE>,
}

pub mod mount_state {
    pub trait MountState {}
    pub struct Mounted;
    impl MountState for Mounted {}
    pub struct NotMounted;
    impl MountState for NotMounted {}

}

// #[derive(Debug)]
pub struct LittleFsAllocation<Storage>
where
    Storage: storage::Storage,
    <Storage as storage::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as storage::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    buffers: Buffers<Storage>,
    state: lfs::lfs_t,
    config: lfs::lfs_config,
}

// #[derive(Debug)]
pub struct LittleFs<'alloc, Storage, MountState = mount_state::NotMounted>
where
    Storage: storage::Storage,
    <Storage as storage::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as storage::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
    MountState: mount_state::MountState,
{
    alloc: &'alloc mut LittleFsAllocation<Storage>,
    mount_state: MountState,
}


impl<'alloc, Storage> LittleFs<'alloc, Storage>
where
    Storage: storage::Storage,
    Storage: 'alloc,
    // MountState: mount_state::MountState,
    <Storage as storage::Storage>::BLOCK_SIZE: ArrayLength<u8>,
    <Storage as storage::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as storage::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    pub fn allocate() -> LittleFsAllocation<Storage> {
        let read_size: u32 = Storage::READ_SIZE as _;
        let write_size: u32 = Storage::WRITE_SIZE as _;
        let block_size: u32 = <Storage as storage::Storage>::BLOCK_SIZE::to_u32();
        let cache_size: u32 = <Storage as storage::Storage>::CACHE_SIZE::to_u32();
        let lookahead_size: u32 =
            32 * <Storage as storage::Storage>::LOOKAHEADWORDS_SIZE::to_u32();
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

        let config = lfs::lfs_config {
            context: core::ptr::null_mut(),
            read: Some(<LittleFs<'alloc, Storage, mount_state::Mounted>>::lfs_config_read),
            prog: Some(<LittleFs<'alloc, Storage, mount_state::Mounted>>::lfs_config_prog),
            erase: Some(<LittleFs<'alloc, Storage, mount_state::Mounted>>::lfs_config_erase),
            sync: Some(<LittleFs<'alloc, Storage, mount_state::Mounted>>::lfs_config_sync),
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

            name_max: Storage::FILENAME_MAX as u32,
            file_max: Storage::FILEBYTES_MAX as u32,
            attr_max: Storage::ATTRBYTES_MAX as u32,
        };

        let alloc = LittleFsAllocation {
            buffers,
            state: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            config,
        };

        alloc
    }

    pub fn new_at(
        alloc: &'alloc mut LittleFsAllocation<Storage>,
        storage: &mut Storage
    ) ->
        LittleFs<'alloc, Storage, mount_state::NotMounted>
    {
        alloc.config.context = storage as *mut _ as *mut cty::c_void;

        alloc.config.read_buffer =
            alloc.buffers.read.as_mut_slice() as *mut _ as *mut cty::c_void;
        alloc.config.prog_buffer =
            alloc.buffers.write.as_mut_slice() as *mut _ as *mut cty::c_void;
        alloc.config.lookahead_buffer =
            alloc.buffers.lookahead.as_mut_slice() as *mut _ as *mut cty::c_void;

        // alloc.config.read =
        //     Some(<LittleFs<'alloc, Storage, mount_state::Mounted>>::lfs_config_read);

        // alloc.state.lfs_config = alloc.config;

        let littlefs = LittleFs {
            alloc,
            mount_state: mount_state::NotMounted,
        };

        littlefs

    }

    pub fn mount(mut self, storage: &mut Storage) ->
        core::result::Result<
            LittleFs<'alloc, Storage, mount_state::Mounted>,
            (LittleFs<'alloc, Storage, mount_state::NotMounted>, Error)
        >
    {
        debug_assert!(self.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        let return_code = unsafe { lfs::lfs_mount(&mut self.alloc.state, &self.alloc.config) };
        match Error::empty_from(return_code) {
            Ok(_) => {
                let mounted = LittleFs {
                    alloc: self.alloc,
                    mount_state: mount_state::Mounted,
                };
                Ok(mounted)
            },
            Err(error) => {
                Err((self, error))
            }
        }
    }

    pub fn format(&mut self, storage: &mut Storage) -> Result<()> {
        debug_assert!(self.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        let return_code = unsafe { lfs::lfs_format(&mut self.alloc.state, &self.alloc.config) };
        Error::empty_from(return_code)?;
        Ok(())
    }
}

impl<'alloc, Storage, MountState> LittleFs<'alloc, Storage, MountState>
where
    Storage: storage::Storage,
    Storage: 'alloc,
    MountState: mount_state::MountState,
    <Storage as storage::Storage>::BLOCK_SIZE: ArrayLength<u8>,
    <Storage as storage::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as storage::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    /// C callback interface used by LittleFS to read data with the lower level system below the
    /// filesystem.
    extern "C" fn lfs_config_read(
        c: *const lfs::lfs_config,
        block: lfs::lfs_block_t,
        off: lfs::lfs_off_t,
        buffer: *mut cty::c_void,
        size: lfs::lfs_size_t,
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
        c: *const lfs::lfs_config,
        block: lfs::lfs_block_t,
        off: lfs::lfs_off_t,
        buffer: *const cty::c_void,
        size: lfs::lfs_size_t,
    ) -> cty::c_int {
        // println!("in lfs_config_prog");
        let storage: &mut Storage = unsafe { mem::transmute((*c).context) };
        debug_assert!(!c.is_null());
        // let block_size = unsafe { c.read().block_size };
        let block_size = <Storage as storage::Storage>::BLOCK_SIZE::to_u32();
        let off = (block * block_size + off) as usize;
        let buf: &[u8] = unsafe { slice::from_raw_parts(buffer as *const u8, size as usize) };

        // TODO
        storage.write(off, buf).unwrap();
        0
    }

    /// C callback interface used by LittleFS to erase data with the lower level system below the
    /// filesystem.
    extern "C" fn lfs_config_erase(
        c: *const lfs::lfs_config,
        block: lfs::lfs_block_t,
    ) -> cty::c_int {
        // println!("in lfs_config_erase");
        // let littlefs: &mut LittleFs<Storage> = unsafe { mem::transmute((*c).context) };
        let storage: &mut Storage = unsafe { mem::transmute((*c).context) };
        let off = block as usize * <Storage as storage::Storage>::BLOCK_SIZE::to_usize();

        // TODO
        storage.erase(off, <Storage as storage::Storage>::BLOCK_SIZE::to_usize()).unwrap();
        0
    }

    /// C callback interface used by LittleFS to sync data with the lower level interface below the
    /// filesystem. Note that this function currently does nothing.
    extern "C" fn lfs_config_sync(_c: *const lfs::lfs_config) -> i32 {
        // println!("in lfs_config_sync");
        // Do nothing; we presume that data is synchronized.
        0
    }
}

#[cfg(test)]
mod tests;
