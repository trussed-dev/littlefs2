use littlefs2_sys as lfs;
use generic_array::ArrayLength;
use crate::{
    error::Result,
    file::SeekFrom,
    Filesystem,
    mount_state,
};

/// Users of this library provide a "storage driver" by implementing this trait.
///
/// The `write` method is assumed to be synchronized to storage immediately.
/// littlefs provides more flexibility - if required, this could also be exposed.
///
/// The `*_SIZE` types must be `generic_array::typenume::consts` such as `U256`.
///
/// Why? Currently, associated constants can not be used (as constants...) to define
/// arrays. This "will be fixed" as part of const generics.
/// Once that's done, we can get rid of the `generic-array`, and replace the
/// `*_SIZE` types with `usize`s.
pub trait Storage {

    // /// Error type for user-provided read/write/erase methods
    // type Error = usize;

    /// Minimum size of block read in bytes.
    const READ_SIZE: usize;
    /// Minimum size of block write in bytes.
    const WRITE_SIZE: usize;

    /// Size of an erasable block in bytes, as unsigned typenum.
    /// Must be a multiple of READ_SIZE and WRITE_SIZE.
    /// At least 128.
    type BLOCK_SIZE;
    /// Number of erasable blocks.
    /// Hence storage capacity is BLOCK_COUNT * BLOCK_SIZE
    const BLOCK_COUNT: usize;

    /// Suggested values are 100-1000, higher is more performant and less wear-leveled.
    /// Default of -1 disables wear-leveling.
    const BLOCK_CYCLES: isize = -1;

    /// littlefs uses a read cache, a write cache, and one cache per per file.
    /// Must be a multiple of READ_SIZE and WRITE_SIZE.
    /// Must be a factor of BLOCK_SIZE.
    type CACHE_SIZE;

    /// littlefs itself has a LOOKAHEAD_SIZE, which must be a multiple of 8,
    /// as it stores data in a bitmap. It also asks for 4-byte aligned buffers.
    /// Hence, we further restrict LOOKAHEAD_SIZE to be a multiple of 32.
    /// Our LOOKAHEADWORDS_SIZE is this multiple.
    type LOOKAHEADWORDS_SIZE;


    /// Maximum length of a filename. Stored in superblock.
    /// Defaults to 255. At most 1_022.
    // const FILENAME_MAX: usize = lfs::LFS_NAME_MAX as _;
    type FILENAME_MAX;

    /// Maximum size of file. Stored in superblock.
    /// Defaults to 2_147_483_647. At most 2_147_483_647.
    const FILEBYTES_MAX: usize = lfs::LFS_FILE_MAX as _;
    /// Maximum size of custom attributes.
    /// Defaults to 1_022. At most 1_022.
    const ATTRBYTES_MAX: usize = lfs::LFS_ATTR_MAX as _;

    /// Read data from the storage device.
    /// Called with bufs of length a multiple of READ_SIZE.
    fn read(&self, off: usize, buf: &mut [u8]) -> Result<usize>;
    /// Write data to the storage device.
    /// Called with bufs of length a multiple of WRITE_SIZE.
    fn write(&mut self, off: usize, data: &[u8]) -> Result<usize>;
    /// Erase data from the storage device.
    /// Called with bufs of length a multiple of BLOCK_SIZE.
    fn erase(&mut self, off: usize, len: usize) -> Result<usize>;
    // /// Synchronize writes to the storage device.
    // fn sync(&mut self) -> Result<usize>;
}

pub trait Read<'alloc, S>
where
    S: Storage,
    <S as Storage>::CACHE_SIZE: ArrayLength<u8>,
    <S as Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    fn read(
        &mut self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
        buf: &mut [u8],
    ) -> Result<usize>;
}

pub trait Write<'alloc, S>
where
    S: Storage,
    <S as Storage>::CACHE_SIZE: ArrayLength<u8>,
    <S as Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    fn write(
        &mut self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
        buf: &[u8],
    ) -> Result<usize>;
}

pub trait Seek<'alloc, S>
where
    S: Storage,
    <S as Storage>::CACHE_SIZE: ArrayLength<u8>,
    <S as Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    fn seek(
        &mut self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
        pos: SeekFrom,
    ) -> Result<usize>;
}

