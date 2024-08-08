use core::cmp;

use bitflags::bitflags;

use crate::path::{Path, PathBuf};

pub type Bytes<SIZE> = generic_array::GenericArray<u8, SIZE>;

bitflags! {
    /// Definition of file open flags which can be mixed and matched as appropriate. These definitions
    /// are reminiscent of the ones defined by POSIX.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct FileOpenFlags: i32 {
        /// Open file in read only mode.
        const READ = 0x1;
        /// Open file in write only mode.
        const WRITE = 0x2;
        /// Open file for reading and writing.
        const READWRITE = Self::READ.bits() | Self::WRITE.bits();
        /// Create the file if it does not exist.
        const CREATE = 0x0100;
        /// Fail if creating a file that already exists.
        /// TODO: Good name for this
        const EXCL = 0x0200;
        /// Truncate the file if it already exists.
        const TRUNCATE = 0x0400;
        /// Open the file in append only mode.
        const APPEND = 0x0800;
    }
}

/// Regular file vs directory
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FileType {
    File,
    Dir,
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        *self == FileType::Dir
    }

    pub fn is_file(&self) -> bool {
        *self == FileType::File
    }
}

/// File type (regular vs directory) and size of a file.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Metadata {
    file_type: FileType,
    size: usize,
}

impl Metadata {
    pub fn new(file_type: FileType, size: usize) -> Self {
        Self { file_type, size }
    }

    pub fn file_type(&self) -> FileType {
        self.file_type
    }

    pub fn is_dir(&self) -> bool {
        self.file_type().is_dir()
    }

    pub fn is_file(&self) -> bool {
        self.file_type().is_file()
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Custom user attribute that can be set on files and directories.
///
/// Consists of an numerical identifier between 0 and 255, and arbitrary
/// binary data up to size `ATTRBYTES_MAX`.
///
/// Use [`Filesystem::attribute`](struct.Filesystem.html#method.attribute),
/// [`Filesystem::set_attribute`](struct.Filesystem.html#method.set_attribute), and
/// [`Filesystem::clear_attribute`](struct.Filesystem.html#method.clear_attribute).
pub struct Attribute {
    id: u8,
    pub data: Bytes<crate::consts::ATTRBYTES_MAX_TYPE>,
    pub size: usize,
}

impl Attribute {
    pub fn new(id: u8) -> Self {
        Attribute {
            id,
            data: Default::default(),
            size: 0,
        }
    }

    pub fn id(&self) -> u8 {
        self.id
    }

    pub fn data(&self) -> &[u8] {
        let attr_max = crate::consts::ATTRBYTES_MAX as _;
        let len = cmp::min(attr_max, self.size);
        &self.data[..len]
    }

    pub fn set_data(&mut self, data: &[u8]) -> &mut Self {
        let attr_max = crate::consts::ATTRBYTES_MAX as _;
        let len = cmp::min(attr_max, data.len());
        self.data[..len].copy_from_slice(&data[..len]);
        self.size = len;
        for entry in self.data[len..].iter_mut() {
            *entry = 0;
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DirEntry {
    file_name: PathBuf,
    metadata: Metadata,
    #[cfg(feature = "dir-entry-path")]
    path: PathBuf,
}

impl DirEntry {
    pub fn new(
        file_name: PathBuf,
        metadata: Metadata,
        #[cfg(feature = "dir-entry-path")] path: PathBuf,
    ) -> Self {
        Self {
            file_name,
            metadata,
            #[cfg(feature = "dir-entry-path")]
            path,
        }
    }

    // Returns the metadata for the file that this entry points at.
    pub fn metadata(&self) -> Metadata {
        self.metadata.clone()
    }

    // Returns the file type for the file that this entry points at.
    pub fn file_type(&self) -> FileType {
        self.metadata.file_type
    }

    // Returns the bare file name of this directory entry without any other leading path component.
    pub fn file_name(&self) -> &Path {
        &self.file_name
    }

    /// Returns the full path to the file that this entry represents.
    ///
    /// The full path is created by joining the original path to read_dir with the filename of this entry.
    #[cfg(feature = "dir-entry-path")]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(feature = "dir-entry-path")]
    #[doc(hidden)]
    // This is used in `crypto-service` to "namespace" paths
    // by mutating a DirEntry in-place.
    pub unsafe fn path_buf_mut(&mut self) -> &mut PathBuf {
        &mut self.path
    }
}
