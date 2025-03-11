//! The `Storage`, `Read`, `Write` and `Seek` driver.
#![allow(non_camel_case_types)]

use crate::io::Error;

mod private {
    pub struct NotEnoughCapacity;
    pub trait Sealed {
        /// Returns a buffer of bytes initialized and valid. If [`set_len`]() was called previously successfully,
        /// its last call defines the minimum number of valid bytes
        fn as_ptr(&self) -> *const u8;
        /// Returns a buffer of bytes initialized and valid. If [`set_len`]() was called previously successfully,
        /// its last call defines the minimum number of valid bytes
        fn as_mut_ptr(&mut self) -> *mut u8;

        /// Current lenght, set by the last call to [`set_len`](Buffer::set_len)
        fn current_len(&self) -> usize;

        /// Atempts to set the length of the buffer to `len`
        ///
        /// If succeeded, the buffer obtained through the pointer operation **must** be of at least `len` bytes
        fn set_len(&mut self, len: usize) -> Result<(), NotEnoughCapacity>;

        // We could use a `Default` trait bound but it's not implemented  for all array sizes
        fn empty() -> Self;
    }
}

pub(crate) use private::Sealed;

/// Safety: implemented only by `[u8; N]` and `Vec<u8>` if the alloc feature is enabled
pub unsafe trait Buffer: private::Sealed {}

impl<const N: usize> private::Sealed for [u8; N] {
    fn as_ptr(&self) -> *const u8 {
        <[u8]>::as_ptr(self)
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        <[u8]>::as_mut_ptr(self)
    }

    fn current_len(&self) -> usize {
        N
    }

    fn set_len(&mut self, len: usize) -> Result<(), private::NotEnoughCapacity> {
        if len > N {
            Err(private::NotEnoughCapacity)
        } else {
            Ok(())
        }
    }

    fn empty() -> Self {
        [0; N]
    }
}

unsafe impl<const N: usize> Buffer for [u8; N] {}

#[cfg(feature = "alloc")]
impl private::Sealed for alloc::vec::Vec<u8> {
    fn as_ptr(&self) -> *const u8 {
        <[u8]>::as_ptr(self)
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        <[u8]>::as_mut_ptr(self)
    }

    fn current_len(&self) -> usize {
        self.len()
    }

    fn set_len(&mut self, len: usize) -> Result<(), private::NotEnoughCapacity> {
        self.resize(len, 0);
        Ok(())
    }

    fn empty() -> Self {
        Self::new()
    }
}

#[cfg(feature = "alloc")]
unsafe impl Buffer for alloc::vec::Vec<u8> {}

/// Users of this library provide a "storage driver" by implementing this trait.
///
/// The `write` method is assumed to be synchronized to storage immediately.
/// littlefs provides more flexibility - if required, this could also be exposed.
/// Do note that due to caches, files still must be synched. And unfortunately,
/// this can't be automatically done in `drop`, since it needs mut refs to both
/// filesystem and storage.
pub trait Storage {
    // /// Error type for user-provided read/write/erase methods
    // type Error = usize;

    /// Minimum size of block read in bytes. Not in superblock
    fn read_size(&self) -> usize;

    /// Minimum size of block write in bytes. Not in superblock
    fn write_size(&self) -> usize;

    /// Size of an erasable block in bytes, as unsigned typenum.
    /// Must be a multiple of both `READ_SIZE` and `WRITE_SIZE`.
    /// [At least 128](https://github.com/littlefs-project/littlefs/issues/264#issuecomment-519963153). Stored in superblock.
    fn block_size(&self) -> usize;

    /// Number of erasable blocks.
    /// Hence storage capacity is `BLOCK_COUNT * BLOCK_SIZE`
    fn block_count(&self) -> usize;

    /// Suggested values are 100-1000, higher is more performant but
    /// less wear-leveled.  Default of -1 disables wear-leveling.
    /// Value zero is invalid, must be positive or -1.
    fn block_cycles(&self) -> isize {
        -1
    }

    /// littlefs uses a read cache, a write cache, and one cache per per file.
    type CACHE_BUFFER: Buffer;

    /// Must be a multiple of `read_size` and `write_size`.
    /// Must be a factor of `block_size`.
    fn cache_size(&self) -> usize;

    /// Lookahead buffer used by littlefs
    type LOOKAHEAD_BUFFER: Buffer;
    /// Size of the lookahead buffer used by littlefs, measured in multiples of 8 bytes.
    fn lookahead_size(&self) -> usize;

    ///// Maximum length of a filename plus one. Stored in superblock.
    ///// Should default to 255+1, but associated type defaults don't exist currently.
    ///// At most 1_022+1.
    /////
    ///// TODO: We can't actually change this - need to pass on as compile flag
    ///// to the C backend.
    //type FILENAME_MAX_PLUS_ONE: ArrayLength<u8>;

    // /// Maximum length of a path plus one. Necessary to convert Rust string slices
    // /// to C strings, which requires an allocation for the terminating
    // /// zero-byte. If in doubt, set to `FILENAME_MAX_PLUS_ONE`.
    // /// Must be larger than `FILENAME_MAX_PLUS_ONE`.
    // type PATH_MAX_PLUS_ONE: ArrayLength<u8>;

    ///// Maximum size of file. Stored in superblock.
    ///// Defaults to 2_147_483_647 (or u31, to avoid sign issues in the C code).
    ///// At most 2_147_483_647.
    /////
    ///// TODO: We can't actually change this - need to pass on as compile flag
    ///// to the C backend.
    //const FILEBYTES_MAX: usize = ll::LFS_FILE_MAX as _;

    ///// Maximum size of custom attributes.
    ///// Should default to 1_022, but associated type defaults don't exists currently.
    ///// At most 1_022.
    /////
    ///// TODO: We can't actually change this - need to pass on as compile flag
    ///// to the C backend.
    //type ATTRBYTES_MAX: ArrayLength<u8>;

    /// Read data from the storage device.
    /// Guaranteed to be called only with bufs of length a multiple of READ_SIZE.
    fn read(&mut self, off: usize, buf: &mut [u8]) -> Result<usize, Error>;
    /// Write data to the storage device.
    /// Guaranteed to be called only with bufs of length a multiple of WRITE_SIZE.
    fn write(&mut self, off: usize, data: &[u8]) -> Result<usize, Error>;
    /// Erase data from the storage device.
    /// Guaranteed to be called only with bufs of length a multiple of BLOCK_SIZE.
    fn erase(&mut self, off: usize, len: usize) -> Result<usize, Error>;
    // /// Synchronize writes to the storage device.
    // fn sync(&mut self) -> Result<usize>;
}
