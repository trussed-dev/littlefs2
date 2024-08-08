#![no_std]

//! Core types for the [`littlefs2`][] crate.
//!
//! See the documentation for [`littlefs2`][] for more information.
//!
//! [`littlefs2`]: https://docs.rs/littlefs2

mod consts;
mod fs;
mod io;
mod object_safe;
mod path;

pub use consts::{ATTRBYTES_MAX, ATTRBYTES_MAX_TYPE, PATH_MAX, PATH_MAX_PLUS_ONE};
pub use fs::{Attribute, DirEntry, FileOpenFlags, FileType, Metadata};
pub use io::{Error, OpenSeekFrom, Read, Result, Seek, SeekFrom, Write};
pub use object_safe::{DirEntriesCallback, DynFile, DynFilesystem, FileCallback, Predicate};
pub use path::{Ancestors, Iter, Path, PathBuf, PathError};

/// Creates a path from a string without a trailing null.
///
/// Panics and causes a compiler error if the string contains null bytes or non-ascii characters.
///
/// # Examples
///
/// ```
/// use littlefs2_core::{path, Path};
///
/// const HOME: &Path = path!("/home");
/// let root = path!("/");
/// ```
///
/// Illegal values:
///
/// ```compile_fail
/// # use littlefs2_core::{path, Path};
/// const WITH_NULL: &Path = path!("/h\0me");  // does not compile
/// ```
///
/// ```compile_fail
/// # use littlefs2_core::{path, Path};
/// const WITH_UTF8: &Path = path!("/höme");  // does not compile
/// ```
///
/// The macro enforces const evaluation so that compilation fails for illegal values even if the
/// macro is not used in a const context:
///
/// ```compile_fail
/// # use littlefs2_core::path;
/// let path = path!("te\0st");  // does not compile
/// ```
#[macro_export]
macro_rules! path {
    ($path:literal) => {{
        const _PATH: &$crate::Path =
            match $crate::Path::from_str_with_nul(::core::concat!($path, "\0")) {
                Ok(path) => path,
                Err(_) => panic!("invalid littlefs2 path"),
            };
        _PATH
    }};
}