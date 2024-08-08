#![allow(non_camel_case_types)]

/// Re-export of `typenum::consts`.
pub use generic_array::typenum::consts::*;

pub use littlefs2_core::{ATTRBYTES_MAX, PATH_MAX, PATH_MAX_PLUS_ONE};

pub const FILENAME_MAX_PLUS_ONE: u32 = 255 + 1;
pub const FILEBYTES_MAX: u32 = crate::ll::LFS_FILE_MAX as _;
pub const LOOKAHEADWORDS_SIZE: u32 = 16;
