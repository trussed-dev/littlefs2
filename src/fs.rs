use core::{
    cmp,
    mem,
    slice,
};

use crate::{
    error::{
        Error,
        Result,
        MountError,
        MountResult,
    },
    mount_state,
    traits,
};

use littlefs2_sys as ll;

use generic_array::{
    ArrayLength,
    GenericArray,
    typenum::marker_traits::Unsigned as _,
};

/// The three global buffers used by LittleFS
// #[derive(Debug)]
pub struct Buffers<Storage>
where
    Storage: traits::Storage,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    read: GenericArray<u8, Storage::CACHE_SIZE>,
    write: GenericArray<u8, Storage::CACHE_SIZE>,
    // must be 4-byte aligned, hence the `u32`s
    lookahead: GenericArray<u32, Storage::LOOKAHEADWORDS_SIZE>,
}

// #[derive(Debug)]
pub struct LittleFsAllocation<Storage>
where
    Storage: traits::Storage,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    buffers: Buffers<Storage>,
    pub(crate) state: ll::lfs_t,
    pub(crate) config: ll::lfs_config,
}

// #[derive(Debug)]
pub struct LittleFs<'alloc, Storage, MountState = mount_state::NotMounted>
where
    Storage: traits::Storage,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
    MountState: mount_state::MountState,
{
    pub(crate) alloc: &'alloc mut LittleFsAllocation<Storage>,
    #[allow(dead_code)]
    mount_state: MountState,
}


impl<'alloc, Storage> LittleFs<'alloc, Storage>
where
    Storage: traits::Storage,
    Storage: 'alloc,
    <Storage as traits::Storage>::BLOCK_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
    <Storage as traits::Storage>::FILENAME_MAX: ArrayLength<u8>,
{
    pub fn allocate() -> LittleFsAllocation<Storage> {
        let read_size: u32 = Storage::READ_SIZE as _;
        let write_size: u32 = Storage::WRITE_SIZE as _;
        let block_size: u32 = <Storage as traits::Storage>::BLOCK_SIZE::to_u32();
        let cache_size: u32 = <Storage as traits::Storage>::CACHE_SIZE::to_u32();
        let lookahead_size: u32 =
            32 * <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE::to_u32();
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

        let name_max: u32 = <Storage as traits::Storage>::FILENAME_MAX::to_u32();

        let config = ll::lfs_config {
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

            name_max,
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

    // TODO: make this an internal method,
    // expose just `mount` and `format`.
    fn placement_new(
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

    pub fn mount(
        alloc: &'alloc mut LittleFsAllocation<Storage>,
        storage: &mut Storage,
    ) -> MountResult<'alloc, Storage> {

        let fs = LittleFs::placement_new(alloc, storage);
        debug_assert!(fs.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        let return_code = unsafe { ll::lfs_mount(&mut fs.alloc.state, &fs.alloc.config) };
        match Error::empty_from(return_code) {
            Ok(_) => {
                let mounted = LittleFs {
                    alloc: fs.alloc,
                    mount_state: mount_state::Mounted,
                };
                MountResult::Ok(mounted)
            },
            Err(error) => {
                MountResult::Err(MountError(fs, error))
            }
        }
    }

    pub fn format(
        alloc: &'alloc mut LittleFsAllocation<Storage>,
        storage: &mut Storage,
    ) ->
        Result<()>
    {
        let fs = LittleFs::placement_new(alloc, storage);
        debug_assert!(fs.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        let return_code = unsafe { ll::lfs_format(&mut fs.alloc.state, &fs.alloc.config) };
        Error::empty_from(return_code)?;
        Ok(())
    }
}

impl<'alloc, Storage> LittleFs<'alloc, Storage, mount_state::Mounted>
where
    Storage: traits::Storage,
    Storage: 'alloc,
    // MountState: mount_state::MountState,
    <Storage as traits::Storage>::BLOCK_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
    <Storage as traits::Storage>::FILENAME_MAX: ArrayLength<u8>,
{
    pub fn unmount(self, storage: &mut Storage) -> Result<LittleFs<'alloc, Storage, mount_state::NotMounted>> {
        debug_assert!(self.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        let return_code = unsafe { ll::lfs_unmount(&mut self.alloc.state) };
        Error::empty_from(return_code)?;
        Ok(
            LittleFs {
                alloc: self.alloc,
                mount_state: mount_state::NotMounted,
            }
        )
    }

    /// Remove a file or directory.
    pub fn remove(&mut self, path: &str, storage: &mut Storage) -> Result<()> {
        debug_assert!(self.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        let mut cstr_path: GenericArray<u8, Storage::FILENAME_MAX> = Default::default();
        let name_max = <Storage as traits::Storage>::FILENAME_MAX::to_usize();
        let len = cmp::min(name_max - 1, path.len());
        cstr_path[..len].copy_from_slice(&path.as_bytes()[..len]);

        let return_code = unsafe { ll::lfs_remove(
            &mut self.alloc.state,
            &cstr_path as *const _ as *const cty::c_char,
        ) };
        Error::empty_from(return_code)?;
        Ok(())
    }

    /// Rename or move a file or directory.
    pub fn rename(&mut self, old_path: &str, new_path: &str, storage: &mut Storage) -> Result<()> {
        debug_assert!(self.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        let name_max = <Storage as traits::Storage>::FILENAME_MAX::to_usize();

        let mut old_cstr_path: GenericArray<u8, Storage::FILENAME_MAX> = Default::default();
        let len = cmp::min(name_max - 1, old_path.len());
        old_cstr_path[..len].copy_from_slice(&old_path.as_bytes()[..len]);

        let mut new_cstr_path: GenericArray<u8, Storage::FILENAME_MAX> = Default::default();
        let len = cmp::min(name_max - 1, new_path.len());
        new_cstr_path[..len].copy_from_slice(&new_path.as_bytes()[..len]);

        let return_code = unsafe { ll::lfs_rename(
            &mut self.alloc.state,
            &old_cstr_path as *const _ as *const cty::c_char,
            &new_cstr_path as *const _ as *const cty::c_char,
        ) };
        Error::empty_from(return_code)?;
        Ok(())
    }

}

impl<'alloc, Storage, MountState> LittleFs<'alloc, Storage, MountState>
where
    Storage: traits::Storage,
    Storage: 'alloc,
    MountState: mount_state::MountState,
    <Storage as traits::Storage>::BLOCK_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
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
        let block_size = <Storage as traits::Storage>::BLOCK_SIZE::to_u32();
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
        // let littlefs: &mut LittleFs<Storage> = unsafe { mem::transmute((*c).context) };
        let storage: &mut Storage = unsafe { mem::transmute((*c).context) };
        let off = block as usize * <Storage as traits::Storage>::BLOCK_SIZE::to_usize();

        // TODO
        storage.erase(off, <Storage as traits::Storage>::BLOCK_SIZE::to_usize()).unwrap();
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
