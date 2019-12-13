use littlefs2_sys as ll;

use generic_array::ArrayLength;

use crate::{
    fs::{
        Filesystem,
        mount_state,
        SeekFrom,
    },
    traits::Storage,
};

/// The `Read` trait allows for reading bytes from a file.
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

/** The `Write` trait allows for writing bytes to a file.

By analogy with `std::io::Write`, we also define a `flush()`
method. In the current implementation, writes are final and
flush has no effect.
*/
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

    fn flush(
        &mut self,
        fs: &mut Filesystem<'alloc, S, mount_state::Mounted>,
        storage: &mut S,
    ) -> Result<()>;

}

/** The `Seek` trait provides a cursor which can be moved within a file.

It is possible to seek relative to either end or the current offset.
*/
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

pub type Result<T> = core::result::Result<T, Error>;

/// Definition of errors that might be returned by filesystem functionality.
#[derive(Copy,Clone,Debug,PartialEq)]
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
pub struct MountError<'alloc, S> (
    pub Filesystem<'alloc, S, mount_state::NotMounted>,
    pub Error,
)
where
    S: Storage,
    <S as Storage>::CACHE_SIZE: ArrayLength<u8>,
    <S as Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
;

/// This gets its own implementation of `.unwrap()`, `.is_ok()`,
/// `.is_err()` etc., as normal unwrap on a Result would need the
/// error value to be `fmt::Debug`.
pub enum MountResult<'alloc, S>
where
    S: Storage,
    <S as Storage>::CACHE_SIZE: ArrayLength<u8>,
    <S as Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    Ok(Filesystem<'alloc, S, mount_state::Mounted>),
    Err(MountError<'alloc, S>),
}

impl<'alloc, S> MountResult<'alloc, S>
where
    S: Storage,
    <S as Storage>::CACHE_SIZE: ArrayLength<u8>,
    <S as Storage>::LOOKAHEADWORDS_SIZE: ArrayLength<u32>,
{
    pub fn unwrap(self) -> Filesystem<'alloc, S, mount_state::Mounted> {
        match self {
            MountResult::Ok(fs) => fs,
            MountResult::Err(error) => Err(error.1).unwrap(),
        }
    }

    pub fn is_ok(&self) -> bool {
        match self {
            MountResult::Ok(_) => true,
            MountResult::Err(_) => false,
        }
    }

    // pub fn contains<U>(&self, x: &U) -> bool
    // where U: PartialEq<MountError<'alloc, S>>
    // {
    //     match self {
    //         MountResult::Ok(value) => value == x,
    //         _ => false,
    //     }
    // }

    pub fn ok(self) -> Option<Filesystem<'alloc, S, mount_state::Mounted>> {
        match self {
            MountResult::Ok(fs) => Some(fs),
            MountResult::Err(_) => None,
        }
    }

    pub fn is_err(&self) -> bool {
        !self.is_ok()
    }

    pub fn err(self) -> Option<MountError<'alloc, S>> {
        match self {
            MountResult::Ok(_) => None,
            MountResult::Err(error) => Some(error),
        }
    }

    pub fn contains_err<F>(&self, f: &F) -> bool
    where F: PartialEq<Error>
    {
        match self {
            MountResult::Err(MountError(_, error)) => f == error,
            _ => false,
        }
    }

}

impl Error {
    pub(crate) fn empty_from(error_code: ll::lfs_error) -> Result<()> {
        match error_code {
            // negative codes
            ll::lfs_error_LFS_ERR_IO => Err(Error::Io),
            ll::lfs_error_LFS_ERR_CORRUPT => Err(Error::CorruptFile),
            ll::lfs_error_LFS_ERR_NOENT => Err(Error::NoSuchEntry),
            ll::lfs_error_LFS_ERR_EXIST => Err(Error::EntryAlreadyExisted),
            ll::lfs_error_LFS_ERR_NOTDIR => Err(Error::PathNotDir),
            ll::lfs_error_LFS_ERR_ISDIR => Err(Error::PathIsDir),
            ll::lfs_error_LFS_ERR_NOTEMPTY => Err(Error::DirNotEmpty),
            ll::lfs_error_LFS_ERR_BADF => Err(Error::BadFileDescriptor),
            ll::lfs_error_LFS_ERR_FBIG => Err(Error::FileTooBig),
            ll::lfs_error_LFS_ERR_INVAL => Err(Error::Invalid),
            ll::lfs_error_LFS_ERR_NOSPC => Err(Error::NoSpace),
            ll::lfs_error_LFS_ERR_NOMEM => Err(Error::NoMemory),
            ll::lfs_error_LFS_ERR_OK => Ok(()),
            // positive codes, the suer should see these only in usize results
            _ => Err(Error::Unknown(error_code)),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn usize_from(error_code: ll::lfs_error) -> Result<usize> {
        let result = Error::empty_from(error_code);
        match result {
            Ok(()) => Ok(0),
            Err(Error::Unknown(value)) => Ok(value as usize),
            Err(error) => Err(error),
        }
    }
}

