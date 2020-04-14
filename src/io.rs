//! Traits and types for core I/O functionality.

pub mod prelude;

use littlefs2_sys as ll;
use ufmt::derive::uDebug;

use crate::{
    fs::{
        Filesystem,
    },
    driver::Storage,
};

/// The `Read` trait allows for reading bytes from a file.
pub trait Read<'alloc, S: Storage> {
    /// Read at most buf.len() bytes.
    /// Upon success, return how many bytes were read.
    fn read(
        &mut self,
        fs: &mut Filesystem<'alloc, S>,
        storage: &mut S,
        buf: &mut [u8],
    ) -> Result<usize>;

    fn read_exact(
        &mut self,
        fs: &mut Filesystem<'alloc, S>,
        storage: &mut S,
        mut buf: &mut [u8],
    ) -> Result<()>
    {
        while !buf.is_empty() {
            match self.read(fs, storage, buf) {
                Ok(0) => break,
                Ok(n) => { let tmp = buf; buf = &mut tmp[n..]; },
                Err(e) => return Err(e),
            }
        }

        if !buf.is_empty() {
            // TODO: better error
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
}

/// The `ReadWith` trait allows for reading bytes from a file.
pub trait ReadWith {
    /// Read at most buf.len() bytes.
    /// Upon success, return how many bytes were read.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => { let tmp = buf; buf = &mut tmp[n..]; },
                Err(e) => return Err(e),
            }
        }

        if !buf.is_empty() {
            // TODO: better error
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
}

/// The `ReadClosure` trait allows for reading bytes from a file.
pub trait ReadClosure<N: heapless::ArrayLength<u8>> {
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

    fn read_to_end(&self, buf: &mut heapless::Vec<u8, N>) -> Result<usize> {
        // My understanding of
        // https://github.com/ARMmbed/littlefs/blob/4c9146ea539f72749d6cc3ea076372a81b12cb11/lfs.c#L2816
        // is that littlefs keeps reading until either the buffer is full, or the file is exhausted

        let had = buf.len();
        // no panic by construction
        buf.resize_default(buf.capacity()).unwrap();
        let read = self.read(&mut buf[had..])?;
        // no panic by construction
        buf.resize_default(had + read).unwrap();
        Ok(read)
    }

}

/** The `Write` trait allows for writing bytes to a file.

By analogy with `std::io::Write`, we also define a `flush()`
method. In the current implementation, writes are final and
flush has no effect.
*/
pub trait Write<'alloc, S: Storage> {
    /// Write at most data.len() bytes.
    /// The file will not necessarily be updated unless
    /// flush is called as there is a cache.
    /// Upon success, return how many bytes were written.
    fn write(
        &mut self,
        fs: &mut Filesystem<'alloc, S>,
        storage: &mut S,
        data: &[u8],
    ) -> Result<usize>;

    /// Write out all pending writes to storage.
    fn flush(
        &mut self,
        fs: &mut Filesystem<'alloc, S>,
        storage: &mut S,
    ) -> Result<()>;

}

pub trait WriteWith {
    /// Write at most data.len() bytes.
    /// The file will not necessarily be updated unless
    /// flush is called as there is a cache.
    /// Upon success, return how many bytes were written.
    fn write(&mut self, data: &[u8]) -> Result<usize>;

    /// Write out all pending writes to storage.
    fn flush(&mut self) -> Result<()>;

    fn write_all(&mut self, mut buf: &[u8]) -> Result<()> {
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

pub trait WriteClosure {
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

/** The `Seek` trait provides a cursor which can be moved within a file.

It is possible to seek relative to either end or the current offset.
*/
pub trait Seek<'alloc, S: Storage>
{
    /// Seek to an offset in bytes.
    /// If successful, returns the new position from start of file.
    fn seek(
        &mut self,
        fs: &mut Filesystem<'alloc, S>,
        storage: &mut S,
        pos: SeekFrom,
    ) -> Result<usize>;
}

pub trait SeekClosure {
    /// Seek to an offset in bytes.
    /// If successful, returns the new position from start of file.
    fn seek(&self, pos: SeekFrom) -> Result<usize>;
}

pub trait SeekWith {
    /// Seek to an offset in bytes.
    /// If successful, returns the new position from start of file.
    fn seek(&mut self, pos: SeekFrom) -> Result<usize>;
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

// TODO: Should this return an enum ErrorCode { Result<()>, usize } ?
impl Error {
    pub(crate) fn result_from(error_code: ll::lfs_error) -> Result<()> {
        match error_code {
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
            ll::lfs_error_LFS_ERR_OK => Ok(()),
            // positive codes should always indicate success
            _ => Err(Error::Unknown(error_code)),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn usize_result_from(error_code: ll::lfs_error) -> Result<usize> {
        let result = Error::result_from(error_code);
        match result {
            Ok(()) => Ok(0),
            Err(Error::Unknown(value)) => Ok(value as usize),
            Err(error) => Err(error),
        }
    }
}

