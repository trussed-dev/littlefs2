use bitflags::bitflags;
use littlefs2_sys as ll;

use crate::{
    error::{
        Error,
        Result,
    },
    Filesystem,
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

    pub fn open<'alloc, Storage>(
        &self,
        path: &str,
        alloc: &'alloc mut FileAllocation<Storage>,
        fs: &mut Filesystem<'alloc, Storage, mount_state::Mounted>,
        storage: &mut Storage,
    ) ->
        Result<File<'alloc, Storage>>
    where
        Storage: traits::Storage,
        <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
        <Storage as traits::Storage>::FILENAME_MAX: ArrayLength<u8>,
        <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
    {
        debug_assert!(fs.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        alloc.config.buffer = alloc.cache.as_mut_slice() as *mut _ as *mut cty::c_void;

        let file = File { alloc };

        let mut padded_path: GenericArray<u8, Storage::FILENAME_MAX> = Default::default();
        let name_max = <Storage as traits::Storage>::FILENAME_MAX::to_usize();
        let len = cmp::min(name_max - 1, path.len());
        padded_path[..len].copy_from_slice(&path.as_bytes()[..len]);

        let return_code = unsafe { ll::lfs_file_opencfg(
                &mut fs.alloc.state,
                &mut file.alloc.state,
                padded_path.as_ptr() as *const cty::c_char,
                self.0.bits() as i32,
                &file.alloc.config,
        ) };

        Error::empty_from(return_code)?;

        Ok(file)
    }

}

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
        fs: &mut Filesystem<'alloc, Storage, mount_state::Mounted>,
        storage: &mut Storage,
    ) ->
        Result<Self>
    {
        OpenOptions::new()
            .read(true)
            .open(path, alloc, fs, storage)
    }

    pub fn create(
        path: &str,
        alloc: &'alloc mut FileAllocation<Storage>,
        fs: &mut Filesystem<'alloc, Storage, mount_state::Mounted>,
        storage: &mut Storage,
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
        fs: &mut Filesystem<'alloc, Storage, mount_state::Mounted>,
        storage: &mut Storage,
    ) ->
        Result<()>
    {
        debug_assert!(fs.alloc.config.context == storage as *mut _ as *mut cty::c_void);
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
        fs: &mut Filesystem<'alloc, Storage, mount_state::Mounted>,
        storage: &mut Storage,
    ) ->
        Result<()>
    {
        debug_assert!(fs.alloc.config.context == storage as *mut _ as *mut cty::c_void);
        let return_code = unsafe { ll::lfs_file_sync(
            &mut fs.alloc.state,
            &mut self.alloc.state,
        ) };
        Error::empty_from(return_code)?;
        Ok(())
    }

    pub fn len(
        &mut self,
        fs: &mut Filesystem<'alloc, Storage, mount_state::Mounted>,
    ) ->
        Result<usize>
    {
        let return_code = unsafe { ll::lfs_file_size(
            &mut fs.alloc.state, &mut self.alloc.state
        ) };
        Error::usize_from(return_code)
    }

}

impl<'alloc, Storage> traits::Read<'alloc, Storage> for File<'alloc, Storage>
where
    Storage: traits::Storage,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    fn read(
        &mut self,
        fs: &mut Filesystem<'alloc, Storage, mount_state::Mounted>,
        _storage: &mut Storage,
        buf: &mut [u8],
    ) ->
        Result<usize>
    {
        let return_code = unsafe { ll::lfs_file_read(
            &mut fs.alloc.state,
            &mut self.alloc.state,
            buf.as_mut_ptr() as *mut cty::c_void,
            buf.len() as u32,
        ) };
        Error::usize_from(return_code)
    }
}

impl<'alloc, Storage> traits::Write<'alloc, Storage> for File<'alloc, Storage>
where
    Storage: traits::Storage,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    fn write(
        &mut self,
        fs: &mut Filesystem<'alloc, Storage, mount_state::Mounted>,
        _storage: &mut Storage,
        buf: &[u8],
    ) ->
        Result<usize>
    {
        let return_code = unsafe { ll::lfs_file_write(
            &mut fs.alloc.state,
            &mut self.alloc.state,
            buf.as_ptr() as *const cty::c_void,
            buf.len() as u32,
        ) };
        Error::usize_from(return_code)
    }
}

impl<'alloc, Storage> traits::Seek<'alloc, Storage> for File<'alloc, Storage>
where
    Storage: traits::Storage,
    <Storage as traits::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as traits::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    fn seek(
        &mut self,
        fs: &mut Filesystem<'alloc, Storage, mount_state::Mounted>,
        _storage: &mut Storage,
        pos: SeekFrom,
    ) ->
        Result<usize>
    {
        let return_code = unsafe { ll::lfs_file_seek(
            &mut fs.alloc.state,
            &mut self.alloc.state,
            pos.off(),
            pos.whence(),
        ) };
        Error::usize_from(return_code)
    }
}
