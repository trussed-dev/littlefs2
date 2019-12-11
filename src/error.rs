use generic_array::{
    ArrayLength,
};
use littlefs2_sys as lfs;
use crate::storage;

pub type Result<T> = core::result::Result<T, Error>;

/// Definition of errors that might be returned by filesystem functionality.
#[derive(Copy,Clone,Debug)]
pub enum Error {
    /// Input / output error occurred.
    Io,
    /// File was corrupt.
    CorruptFile,
    /// No entry found with that name.
    NoSuchEntry,
    /// File or directory already exists.
    EntryAlreadyExisted,
    /// Path name is not a directory.
    PathNotDir,
    /// Path specification is to a directory.
    PathIsDir,
    /// Directory was not empty.
    DirNotEmpty,
    /// Bad file descriptor.
    BadFileDescriptor,
    /// File is too big.
    FileTooBig,
    /// Incorrect value specified to function.
    Invalid,
    /// No space left available for operation.
    NoSpace,
    /// No memory available for completing request.
    NoMemory,
    /// Unknown error occurred, integer code specified.
    Unknown(i32),
}

// NB: core::convert::From does not work here due to coherence rules
// #[derive(Debug)]
pub struct MountError<'alloc, Storage>
where
    Storage: storage::Storage,
    <Storage as storage::Storage>::CACHE_SIZE: ArrayLength<u8>,
    <Storage as storage::Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    not_mounted: super::LittleFs<'alloc, Storage, super::mount_state::NotMounted>,
    error: Error,
}

impl Error {
    pub(crate) fn empty_from(error_code: lfs::lfs_error) -> Result<()> {
        match error_code {
            // negative codes
            lfs::lfs_error_LFS_ERR_IO => Err(Error::Io),
            lfs::lfs_error_LFS_ERR_CORRUPT => Err(Error::CorruptFile),
            lfs::lfs_error_LFS_ERR_NOENT => Err(Error::NoSuchEntry),
            lfs::lfs_error_LFS_ERR_EXIST => Err(Error::EntryAlreadyExisted),
            lfs::lfs_error_LFS_ERR_NOTDIR => Err(Error::PathNotDir),
            lfs::lfs_error_LFS_ERR_ISDIR => Err(Error::PathIsDir),
            lfs::lfs_error_LFS_ERR_NOTEMPTY => Err(Error::DirNotEmpty),
            lfs::lfs_error_LFS_ERR_BADF => Err(Error::BadFileDescriptor),
            lfs::lfs_error_LFS_ERR_FBIG => Err(Error::FileTooBig),
            lfs::lfs_error_LFS_ERR_INVAL => Err(Error::Invalid),
            lfs::lfs_error_LFS_ERR_NOSPC => Err(Error::NoSpace),
            lfs::lfs_error_LFS_ERR_NOMEM => Err(Error::NoMemory),
            lfs::lfs_error_LFS_ERR_OK => Ok(()),
            // positive codes, the suer should see these only in usize results
            _ => Err(Error::Unknown(error_code)),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn usize_from(error_code: lfs::lfs_error) -> Result<usize> {
        let result = Error::empty_from(error_code);
        match result {
            Ok(()) => Ok(0),
            Err(Error::Unknown(value)) => Ok(value as usize),
            Err(error) => Err(error),
        }
    }
}
