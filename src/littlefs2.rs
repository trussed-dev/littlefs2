#![no_std]

use littlefs2_sys as ll;

#[macro_use]
pub mod ramstorage;

/// The error types.
pub mod error;
pub use error::{
    Error,
    Result,
};

/// The `File` abstraction.
pub mod file;
pub use file::{
    FileAllocation,
    File,
};

/// The `LittleFs` abstraction.
pub mod fs;
pub use fs::{
    LittleFsAllocation,
    LittleFs,
};

/// The `Storage`, `Read`, `Write` and `Seek` traits.
pub mod traits;
pub use traits::{
    Storage,
};

#[derive(Copy,Clone,Debug)]
pub struct Version {
    format: (u32, u32),
    backend: (u32, u32),
}

pub fn version() -> Version {
    Version {
        format: (ll::LFS_DISK_VERSION_MAJOR, ll::LFS_DISK_VERSION_MINOR),
        backend: (ll::LFS_VERSION_MAJOR, ll::LFS_VERSION_MINOR),
    }
}

pub mod mount_state {
    pub trait MountState {}
    pub struct Mounted;
    impl MountState for Mounted {}
    pub struct NotMounted;
    impl MountState for NotMounted {}

}

#[cfg(test)]
mod tests;
