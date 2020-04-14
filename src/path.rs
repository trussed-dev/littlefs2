//! Path manipulation

use core::convert::AsRef;
// use core::marker::PhantomData;
use core::{cmp, fmt, mem};
#[cfg(test)]
use core::slice;

use generic_array::{
    ArrayLength,
    typenum::marker_traits::Unsigned as _,
};

// TODO: use `heapless-bytes` instead?
use heapless::Vec;

use crate::{
    driver,
};

// TODO: add a `CString` type to heapless?
// - meaning a "byte" string (not UTF-8)
// - allocate "one more" than necessary
// - keep invariants (data till first null, then only nulls)
// - allow transforming in both directions

// GENERALLY:
// - littlefs has a notion of "max filename"
// - our "max path" only comes from being alloc-free
// - std::path distinguishes between Path and PathBuf (our Path is really their PathBuf)
// - for filenames, std::path uses OsString
//
// At minimum get rid of copy-paste implementation of Filename/Path

// pub trait CStringType {}
// pub struct PathType {}
// impl CStringType for PathType {}
// pub struct FilenameType {}
// impl CStringType for FilenameType {}

// pub struct CString<T: CStringType, N: ArrayLength<u8>> (Vec<u8, N>, PhantomData<T>);

// pub type Filename2<S> = CString<FilenameType, <S as driver::Storage>::FILENAME_MAX_PLUS_ONE>;

// impl<S> core::ops::Deref for Filename2<S>
// where
//     S: driver::Storage,
//     <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
// {
//     type Target = [u8];
//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

/// PREVIOUSLY...

pub struct Filename<S> (Vec<u8, S::FILENAME_MAX_PLUS_ONE>)
where
    S: driver::Storage,
    <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
;

impl<S> core::ops::Deref for Filename<S>
where
    S: driver::Storage,
    <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
{
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> core::ops::Deref for Path<S>
where
    S: driver::Storage,
    <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
{
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// to compare filename
impl<S> cmp::PartialEq for Filename<S>
where
    S: driver::Storage,
    <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
{
    fn eq(&self, other: &Self) -> bool {
        // let shrunk_self = self.clone().shrunk_to_first_null();
        // let shrunk_other = other.clone().shrunk_to_first_null();
        // shrunk_self.0 == shrunk_other.0
        self.0 == other.0
    }
}

// to make `DirEntry` Clone
impl<S> Clone for Filename<S>
where
    S: driver::Storage,
    <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
{
    fn clone(&self) -> Self {
        // the `Clone` impl skips unused bytes
        Filename(self.0.clone()).shrunk_to_first_null()
    }
}

// to make `Metadata` Debug
impl<S> fmt::Debug for Filename<S>
where
    S: driver::Storage,
    <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // let len = self.0.iter().position(|b| *b == 0).unwrap_or(self.0.len());
        // core::str::from_utf8(&self.0[..len]).unwrap().fmt(f)

        use core::ascii::escape_default;
        f.write_str("b'")?;
        for byte in &self.0 {
            for ch in escape_default(*byte) {
                // Debug::fmt(unsafe { core::str::from_utf8_unchecked(&[ch]) }, f)?;
                f.write_str(unsafe { core::str::from_utf8_unchecked(&[ch]) })?;
                // f.write(&ch);
            }
        }
        f.write_str("'")?;
        Ok(())
    }
}

impl<S> Filename<S>
where
    S: driver::Storage,
    <S as driver::Storage>::FILENAME_MAX_PLUS_ONE: ArrayLength<u8>
{
    /// Silently truncates to maximum configured path length
    // pub fn new<F: AsRef<[u8]> + ?Sized>(f: &F) -> Self {
    pub fn new(f: &[u8]) -> Self {
        let mut filename = Filename(Default::default());
        filename.resize_to_capacity();

        let name_max = <S as driver::Storage>::FILENAME_MAX_PLUS_ONE::USIZE;
        let len = cmp::min(name_max - 1, f.len());

        filename.0[..len].copy_from_slice(&f[..len]);

        filename.shrink_to_first_null();
        filename
    }

    // pub fn as_bytes(&self) -> &[u8] {
    //     &self.0
    // }

    pub fn shrink_to_first_null(&mut self) -> &mut Self {
        self.resize_to_capacity();
        let len = self.0.iter().position(|b| *b == 0).unwrap_or(self.0.len());
        self.0.resize_default(len).unwrap();
        // now clear potential "junk"
        self.resize_to_capacity();
        self.0.resize_default(len).unwrap();
        self
    }

    pub fn shrunk_to_first_null(self) -> Self {
        let mut self_ = self;
        self_.shrink_to_first_null();
        self_
    }


    pub fn resize_to_capacity(&mut self) -> &mut Self {
        self.0.resize_default(self.0.capacity()).unwrap();
        self
    }

    // LFS_NAME_MAX + 1 = 256 is hardcoded in littlefs2-sys
    // - can't actually change it via driver::Storage trait
    // - need to fix in future..
    pub fn from_littlefs_file_name_c_string(name: &[cty::c_char; 256]) -> Self {
        let effective_length = name.iter().position(|b| *b == 0).unwrap_or(255);
        let usable_length = cmp::min(
            effective_length,
            <S as driver::Storage>::FILENAME_MAX_PLUS_ONE::to_usize() - 1,
        );

        // // explicit version
        // let mut filename = Self::new(&[]);
        // for i in 0..usable_length {
        //     filename.0.push(name[i] as u8).unwrap();
        // }
        // filename

        Self::new(unsafe { mem::transmute::<&[cty::c_char], &[u8]>(&name[..usable_length]) })
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn print_raw(&self) {
        let underlying_array = unsafe {
            let ptr = &self[..] as *const _ as *const u8;
            slice::from_raw_parts(ptr, 255)
        };
        println!("-> raw path {:?}", &underlying_array);
    }
}

/// A slice of a specification of the location of a [`File`](../fs/struct.File.html).
///
/// This module is rather incomplete, compared to `std::path`.
pub struct Path<S> (pub(crate) Vec<u8, S::PATH_MAX_PLUS_ONE>)
where
    S: driver::Storage,
    <S as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>,
;

// to make `Metadata` Clone
impl<S> Clone for Path<S>
where
    S: driver::Storage,
    <S as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>,
{
    fn clone(&self) -> Self {
        // the `Clone` impl skips unused bytes
        Path(self.0.clone()).shrunk_to_first_null()
    }
}

impl<S> PartialEq for Path<S>
where
    S: driver::Storage,
    <S as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>,
{
    fn eq(&self, other: &Self) -> bool {
        // let shrunk_self = self.clone().shrunk_to_first_null();
        // let shrunk_other = other.clone().shrunk_to_first_null();
        // shrunk_self.0 == shrunk_other.0
        self.0 == other.0
    }
}

// to make `Metadata` Debug
impl<S> fmt::Debug for Path<S>
where
    S: driver::Storage,
    <S as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // let len = self.0.iter().position(|b| *b == 0).unwrap_or(self.0.len());
        // core::str::from_utf8(&self.0[..len]).unwrap().fmt(f)

        use core::ascii::escape_default;
        f.write_str("b\"")?;
        for byte in &self.0 {
            for ch in escape_default(*byte) {
                f.write_str(unsafe { core::str::from_utf8_unchecked(&[ch]) })?;
            }
        }
        f.write_str("\"")?;
        Ok(())
    }
}

impl<S> Path<S>
where
    S: driver::Storage,
    <S as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>
{
    /// Silently truncates to maximum configured path length
    pub fn new<P: AsRef<[u8]> + ?Sized>(p: &P) -> Self {

        let mut path = Path(Default::default());
        path.resize_to_capacity();

        let path_max = <S as driver::Storage>::PATH_MAX_PLUS_ONE::USIZE;
        let len = cmp::min(path_max - 1, p.as_ref().len());

        path.0[..len].copy_from_slice(&p.as_ref()[..len]);

        path.shrink_to_first_null();
        path
    }

    pub fn is_absolute(&self) -> bool {
        self.has_root()
    }

    pub fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    pub fn has_root(&self) -> bool {
        self.0.len() > 0 && self.0[0] == b'/'
    }

    pub fn shrink_to_first_null(&mut self) -> &mut Self {
        self.resize_to_capacity();
        let len = self.0.iter().position(|b| *b == 0).unwrap_or(self.0.len());
        self.0.resize_default(len).unwrap();
        // now clear potential "junk"
        self.resize_to_capacity();
        self.0.resize_default(len).unwrap();
        self
    }

    pub fn shrunk_to_first_null(self) -> Self {
        let mut self_ = self;
        self_.shrink_to_first_null();
        self_
    }

    pub fn resize_to_capacity(&mut self) -> &mut Self {
        self.0.resize_default(self.0.capacity()).unwrap();
        self
    }

    #[cfg(test)]
    #[allow(dead_code)]
    fn print_raw(&self) {
        let underlying_array = unsafe {
            let ptr = &self[..] as *const _ as *const u8;
            slice::from_raw_parts(ptr, 255)
        };
        println!("-> raw path {:?}", &underlying_array);
    }

    // what to do about possible "array-too-small" errors?
    // what does littlefs actually do?
    // one option would be:
    //
    // enum Path {
    //   NotTruncated(RawPath),
    //   Truncated(RawPath),
    // }
    //
    // impl Deref<RawPath> for Path { ... }
    //
    // that is, never fail, but tag if truncation was necessary
    // this way, no need to do error handling for the rare cases,
    // but can still detect them

    // pub fn join<P: AsRef<Path>>(&self, path: P) -> Path {
    // }

    pub fn try_join(&self, path: impl Into<Path<S>>) -> core::result::Result<Path<S>, ()> {
        let mut joined = self.clone();
        // yolo
        if joined.0.len() > 0 {
            if joined.0[joined.0.len() - 1] != b'/' {
                joined.0.extend_from_slice(b"/")?;
            }
        }
        joined.0.extend_from_slice(&path.into().0).map(|_| joined)
    }
}

impl<S> From<&str> for Path<S>
where
    S: driver::Storage,
    <S as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>
{
    fn from(p: &str) -> Path<S> {
        Path::new(p.as_bytes())
    }
}

impl<S> From<&[u8]> for Path<S>
where
    S: driver::Storage,
    <S as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>
{
    fn from(p: &[u8]) -> Path<S> {
        Path::new(p)
    }
}

impl<S> From<&Filename<S>> for Path<S>
where
    S: driver::Storage,
    <S as driver::Storage>::PATH_MAX_PLUS_ONE: ArrayLength<u8>
{
    fn from(p: &Filename<S>) -> Path<S> {
        Path::new(&p[..])
    }
}

