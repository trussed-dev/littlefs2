//! Object-safe traits for [`File`][], [`Filesystem`][] and [`Storage`][].

use generic_array::typenum::Unsigned as _;
use heapless::Vec;

use crate::{
    driver::Storage,
    fs::{Attribute, DirEntry, File, Filesystem, Metadata, OpenOptions},
    io::{Error, OpenSeekFrom, Read, Result, Seek, Write},
    path::Path,
};

// Make sure that the traits actually are object safe.
const _: Option<&dyn DynFile> = None;
const _: Option<&dyn DynFilesystem> = None;
const _: Option<&dyn DynStorage> = None;

pub type DirEntriesCallback<'a, R = ()> =
    &'a mut dyn FnMut(&mut dyn Iterator<Item = Result<DirEntry>>) -> Result<R>;
pub type FileCallback<'a, R = ()> = &'a mut dyn FnMut(&dyn DynFile) -> Result<R>;
pub type FilesystemCallback<'a, R = ()> = &'a mut dyn FnMut(&dyn DynFilesystem) -> Result<R>;
pub type OpenOptionsCallback<'a> = &'a dyn Fn(&mut OpenOptions) -> &OpenOptions;
pub type Predicate<'a> = &'a dyn Fn(&DirEntry) -> bool;

/// Object-safe trait for [`File`][].
///
/// The methods for opening files cannot be implemented in this trait.  Use these methods instead:
/// - [`DynFilesystem::create_file_and_then`](trait.DynFilesystem.html#method.create_file_and_then)
/// - [`DynFilesystem::open_file_and_then`](trait.DynFilesystem.html#method.open_file_and_then)
/// - [`DynFilesystem::open_file_with_options_and_then`](trait.DynFilesystem.html#method.open_file_with_options_and_then)
///
/// All other methods are mirrored directly.  See the documentation for [`File`][] for more information.
pub trait DynFile: Read + Seek + Write {
    fn sync(&self) -> Result<()>;
    fn len(&self) -> Result<usize>;
    fn is_empty(&self) -> Result<bool>;
    fn set_len(&self, size: usize) -> Result<()>;
}

impl<S: Storage> DynFile for File<'_, '_, S> {
    fn sync(&self) -> Result<()> {
        File::sync(self)
    }

    fn len(&self) -> Result<usize> {
        File::len(self)
    }

    fn is_empty(&self) -> Result<bool> {
        File::is_empty(self)
    }

    fn set_len(&self, size: usize) -> Result<()> {
        File::set_len(self, size)
    }
}

impl dyn DynFile + '_ {
    pub fn read_to_end<const N: usize>(&self, buf: &mut Vec<u8, N>) -> Result<usize> {
        let had = buf.len();
        buf.resize_default(buf.capacity()).unwrap();
        let read = self.read(&mut buf[had..])?;
        buf.truncate(had + read);
        Ok(read)
    }
}

/// Object-safe trait for [`Filesystem`][].
///
/// It contains these additional methods from [`Path`][]:
/// - [`DynFilesystem::exists`][]
///
/// The following methods are implemented in [`DynStorage`][] instead:
/// - [`DynStorage::format`][]
/// - [`DynStorage::is_mountable`][]
/// - [`DynStorage::mount_and_then`](trait.DynStorage.html#method.mount_and_then)
///
/// The following methods cannot support generic return types in the callbacks:
/// - [`DynFilesystem::create_file_and_then_unit`][]
/// - [`DynFilesystem::open_file_and_then_unit`][]
/// - [`DynFilesystem::open_file_with_options_and_then_unit`][]
/// - [`DynFilesystem::read_dir_and_then_unit`][]
///
/// Use these helper functions instead:
/// - [`DynFilesystem::create_file_and_then`](#method.create_file_and_then)
/// - [`DynFilesystem::open_file_and_then`](#method.open_file_and_then)
/// - [`DynFilesystem::open_file_with_options_and_then`](#method.open_file_with_options_and_then)
/// - [`DynFilesystem::read_dir_and_then`](#method.read_dir_and_then)
///
/// All other methods are mirrored directly.  See the documentation for [`Filesystem`][] for more information.
pub trait DynFilesystem {
    fn total_blocks(&self) -> usize;
    fn total_space(&self) -> usize;
    fn available_blocks(&self) -> Result<usize>;
    fn available_space(&self) -> Result<usize>;
    fn remove(&self, path: &Path) -> Result<()>;
    fn remove_dir(&self, path: &Path) -> Result<()>;
    #[cfg(feature = "dir-entry-path")]
    fn remove_dir_all(&self, path: &Path) -> Result<()>;
    #[cfg(feature = "dir-entry-path")]
    fn remove_dir_all_where(&self, path: &Path, predicate: Predicate<'_>) -> Result<usize>;
    fn rename(&self, from: &Path, to: &Path) -> Result<()>;
    fn metadata(&self, path: &Path) -> Result<Metadata>;
    fn create_file_and_then_unit(&self, path: &Path, f: FileCallback<'_>) -> Result<()>;
    fn open_file_and_then_unit(&self, path: &Path, f: FileCallback<'_>) -> Result<()>;
    fn open_file_with_options_and_then_unit(
        &self,
        o: OpenOptionsCallback<'_>,
        path: &Path,
        f: FileCallback<'_>,
    ) -> Result<()>;
    fn attribute(&self, path: &Path, id: u8) -> Result<Option<Attribute>>;
    fn remove_attribute(&self, path: &Path, id: u8) -> Result<()>;
    fn set_attribute(&self, path: &Path, attribute: &Attribute) -> Result<()>;
    fn read_dir_and_then_unit(&self, path: &Path, f: DirEntriesCallback<'_>) -> Result<()>;
    fn create_dir(&self, path: &Path) -> Result<()>;
    fn create_dir_all(&self, path: &Path) -> Result<()>;
    fn write(&self, path: &Path, contents: &[u8]) -> Result<()>;
    fn write_chunk(&self, path: &Path, contents: &[u8], pos: OpenSeekFrom) -> Result<()>;
    fn exists(&self, path: &Path) -> bool;
}

impl<S: Storage> DynFilesystem for Filesystem<'_, S> {
    fn total_blocks(&self) -> usize {
        Filesystem::total_blocks(self)
    }

    fn total_space(&self) -> usize {
        Filesystem::total_space(self)
    }

    fn available_blocks(&self) -> Result<usize> {
        Filesystem::available_blocks(self)
    }

    fn available_space(&self) -> Result<usize> {
        Filesystem::available_space(self)
    }

    fn remove(&self, path: &Path) -> Result<()> {
        Filesystem::remove(self, path)
    }

    fn remove_dir(&self, path: &Path) -> Result<()> {
        Filesystem::remove_dir(self, path)
    }

    #[cfg(feature = "dir-entry-path")]
    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        Filesystem::remove_dir_all(self, path)
    }

    #[cfg(feature = "dir-entry-path")]
    fn remove_dir_all_where(&self, path: &Path, predicate: Predicate<'_>) -> Result<usize> {
        Filesystem::remove_dir_all_where(self, path, &|entry| predicate(entry))
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        Filesystem::rename(self, from, to)
    }

    fn metadata(&self, path: &Path) -> Result<Metadata> {
        Filesystem::metadata(self, path)
    }

    fn create_file_and_then_unit(&self, path: &Path, f: FileCallback<'_>) -> Result<()> {
        Filesystem::create_file_and_then(self, path, |file| f(file))
    }

    fn open_file_and_then_unit(&self, path: &Path, f: FileCallback<'_>) -> Result<()> {
        Filesystem::open_file_and_then(self, path, |file| f(file))
    }

    fn open_file_with_options_and_then_unit(
        &self,
        o: OpenOptionsCallback<'_>,
        path: &Path,
        f: FileCallback<'_>,
    ) -> Result<()> {
        Filesystem::open_file_with_options_and_then(self, o, path, |file| f(file))
    }

    fn attribute(&self, path: &Path, id: u8) -> Result<Option<Attribute>> {
        Filesystem::attribute(self, path, id)
    }

    fn remove_attribute(&self, path: &Path, id: u8) -> Result<()> {
        Filesystem::remove_attribute(self, path, id)
    }

    fn set_attribute(&self, path: &Path, attribute: &Attribute) -> Result<()> {
        Filesystem::set_attribute(self, path, attribute)
    }

    fn read_dir_and_then_unit(&self, path: &Path, f: DirEntriesCallback<'_>) -> Result<()> {
        Filesystem::read_dir_and_then(self, path, |entries| f(entries))
    }

    fn create_dir(&self, path: &Path) -> Result<()> {
        Filesystem::create_dir(self, path)
    }

    fn create_dir_all(&self, path: &Path) -> Result<()> {
        Filesystem::create_dir_all(self, path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<()> {
        Filesystem::write(self, path, contents)
    }

    fn write_chunk(&self, path: &Path, contents: &[u8], pos: OpenSeekFrom) -> Result<()> {
        Filesystem::write_chunk(self, path, contents, pos)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists(self)
    }
}

impl dyn DynFilesystem + '_ {
    pub fn read<const N: usize>(&self, path: &Path) -> Result<Vec<u8, N>> {
        let mut contents = Vec::new();
        self.open_file_and_then(path, &mut |file| {
            file.read_to_end(&mut contents)?;
            Ok(())
        })?;
        Ok(contents)
    }

    pub fn read_chunk<const N: usize>(
        &self,
        path: &Path,
        pos: OpenSeekFrom,
    ) -> Result<(Vec<u8, N>, usize)> {
        let mut contents = Vec::new();
        let file_len = self.open_file_and_then(path, &mut |file| {
            file.seek(pos.into())?;
            let read_n = file.read(&mut contents)?;
            contents.truncate(read_n);
            file.len()
        })?;
        Ok((contents, file_len))
    }

    pub fn create_file_and_then<R>(&self, path: &Path, f: FileCallback<'_, R>) -> Result<R> {
        let mut result = Err(Error::Io);
        self.create_file_and_then_unit(path, &mut |file| {
            result = Ok(f(file)?);
            Ok(())
        })?;
        result
    }

    pub fn open_file_and_then<R>(&self, path: &Path, f: FileCallback<'_, R>) -> Result<R> {
        let mut result = Err(Error::Io);
        self.open_file_and_then_unit(path, &mut |file| {
            result = Ok(f(file)?);
            Ok(())
        })?;
        result
    }

    pub fn open_file_with_options_and_then<R>(
        &self,
        o: OpenOptionsCallback<'_>,
        path: &Path,
        f: FileCallback<'_, R>,
    ) -> Result<R> {
        let mut result = Err(Error::Io);
        self.open_file_with_options_and_then_unit(o, path, &mut |file| {
            result = Ok(f(file)?);
            Ok(())
        })?;
        result
    }

    pub fn read_dir_and_then<R>(&self, path: &Path, f: DirEntriesCallback<'_, R>) -> Result<R> {
        let mut result = Err(Error::Io);
        self.read_dir_and_then_unit(path, &mut |entries| {
            result = Ok(f(entries)?);
            Ok(())
        })?;
        result
    }
}

/// Object-safe trait for [`Storage`][].
///
/// It contains these additional methods from [`Filesystem`][]:
/// - [`DynStorage::format`][]
/// - [`DynStorage::is_mountable`][]
/// - [`DynStorage::mount_and_then`](#method.mount_and_then)
///
/// The following methods cannot support generic return types in the callbacks:
/// - [`DynStorage::mount_and_then_unit`][]
///
/// Use these helper functions instead:
/// - [`DynStorage::mount_and_then`](#method.mount_and_then)
///
/// The `read`, `write` and `erase` methods are mirrored directly.  The associated constants and
/// types are transformed into methods.  See the documentation for [`Storage`][] for more
/// information.
pub trait DynStorage {
    fn read_size(&self) -> usize;
    fn write_size(&self) -> usize;
    fn block_size(&self) -> usize;
    fn block_count(&self) -> usize;
    fn block_cycles(&self) -> isize;
    fn cache_size(&self) -> usize;
    fn lookahead_size(&self) -> usize;
    fn read(&mut self, off: usize, buf: &mut [u8]) -> Result<usize>;
    fn write(&mut self, off: usize, data: &[u8]) -> Result<usize>;
    fn erase(&mut self, off: usize, len: usize) -> Result<usize>;
    fn format(&mut self) -> Result<()>;
    fn is_mountable(&mut self) -> bool;
    fn mount_and_then_unit(&mut self, f: FilesystemCallback<'_>) -> Result<()>;
}

impl<S: Storage> DynStorage for S {
    fn read_size(&self) -> usize {
        Self::READ_SIZE
    }

    fn write_size(&self) -> usize {
        Self::WRITE_SIZE
    }

    fn block_size(&self) -> usize {
        Self::BLOCK_SIZE
    }

    fn block_count(&self) -> usize {
        Self::BLOCK_COUNT
    }

    fn block_cycles(&self) -> isize {
        Self::BLOCK_CYCLES
    }

    fn cache_size(&self) -> usize {
        S::CACHE_SIZE::to_usize()
    }

    fn lookahead_size(&self) -> usize {
        S::LOOKAHEAD_SIZE::to_usize()
    }

    fn read(&mut self, off: usize, buf: &mut [u8]) -> Result<usize> {
        Storage::read(self, off, buf)
    }

    fn write(&mut self, off: usize, data: &[u8]) -> Result<usize> {
        Storage::write(self, off, data)
    }

    fn erase(&mut self, off: usize, len: usize) -> Result<usize> {
        Storage::erase(self, off, len)
    }

    fn format(&mut self) -> Result<()> {
        Filesystem::format(self)
    }

    fn is_mountable(&mut self) -> bool {
        Filesystem::is_mountable(self)
    }

    fn mount_and_then_unit(&mut self, f: FilesystemCallback<'_>) -> Result<()> {
        Filesystem::mount_and_then(self, |fs| f(fs))
    }
}

impl dyn DynStorage + '_ {
    pub fn mount_and_then<R>(&mut self, f: FilesystemCallback<'_, R>) -> Result<R> {
        let mut result = Err(Error::Io);
        self.mount_and_then_unit(&mut |fs| {
            result = Ok(f(fs)?);
            Ok(())
        })?;
        result
    }
}
