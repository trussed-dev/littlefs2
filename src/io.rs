//! Traits and types for core I/O functionality.

pub mod prelude;

use littlefs2_sys as ll;
use ufmt::derive::uDebug;

/// The `Read` trait allows for reading bytes from a file.
pub trait Read {
    /// Read at most buf.len() bytes.
    /// Upon success, return how many bytes were read.
    fn read(&self, buf: &mut [u8]) -> Result<usize>;

    fn read_exact(&self, buf: &mut [u8]) -> Result<()> {
        // Same assumption as for `read_to_end`.
        let len = self.read(buf)?;
        if len == buf.len() {
            Ok(())
        } else {
            // TODO: Decide whether to add an equivalent of `ErrorKind::UnexpectedEof`
            Err(Error::Io)
        }
    }

}

/** The `Write` trait allows for writing bytes to a file.

By analogy with `std::io::Write`, we also define a `flush()`
method. In the current implementation, writes are final and
flush has no effect.
*/
pub trait Write {
    /// Write at most data.len() bytes.
    /// The file will not necessarily be updated unless
    /// flush is called as there is a cache.
    /// Upon success, return how many bytes were written.
    fn write(&self, data: &[u8]) -> Result<usize>;

    /// Write out all pending writes to storage.
    fn flush(&self) -> Result<()>;

    fn write_all(&self, mut buf: &[u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => {
                    // failed to write whole buffer
                    return Err(Error::Io)
                }
                Ok(n) => buf = &buf[n..],
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/** Enumeration of possible methods to seek within an I/O object.

Use the [`Seek`](../io/trait.Seek.html) trait.
*/
#[derive(Clone,Copy,Debug,Eq,PartialEq,uDebug)]
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

/** The `Seek` trait provides a cursor which can be moved within a file.

It is possible to seek relative to either end or the current offset.
*/
pub trait Seek {
    /// Seek to an offset in bytes.
    /// If successful, returns the new position from start of file.
    fn seek(&self, pos: SeekFrom) -> Result<usize>;
}

pub type Result<T> = core::result::Result<T, Error>;

/// Definition of errors that might be returned by filesystem functionality.
#[derive(Clone,Copy,Debug,PartialEq,uDebug)]
pub enum Error {
    /// Input / output error occurred.
    Io,
    /// File or filesystem was corrupt.
    Corruption,
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
    /// No attribute or data available
    NoAttribute,
    /// Filename too long
    FilenameTooLong,
    /// Unknown error occurred, integer code specified.
    Unknown(i32),
}

pub fn result_from(error_code: ll::lfs_error) -> Result<u32> {
    match error_code {
        n if n >= 0 => Ok(n as u32),
        // negative codes
        ll::lfs_error_LFS_ERR_IO => Err(Error::Io),
        ll::lfs_error_LFS_ERR_CORRUPT => Err(Error::Corruption),
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
        ll::lfs_error_LFS_ERR_NOATTR => Err(Error::NoAttribute),
        ll::lfs_error_LFS_ERR_NAMETOOLONG => Err(Error::FilenameTooLong),
        // positive codes should always indicate success
        _ => Err(Error::Unknown(error_code)),
    }
}

