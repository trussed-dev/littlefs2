#![no_std]

/*!

[`littlefs`](https://github.com/ARMmbed/littlefs) is a filesystem for microcontrollers
written in C, that claims to be "fail-safe":
- power-loss resilience, by virtue of copy-on-write guarantees
- dynamic wear-leveling, including detection of bad Flash blocks
- bounded RAM/ROM, with stack-allocated buffers

For more background, see its [design](https://github.com/ARMmbed/littlefs/blob/master/DESIGN.md)
and the [specification](https://github.com/ARMmbed/littlefs/blob/master/SPEC.md) of its format.

This library, `littlefs2`, offers an idiomatic Rust API around `littlefs`, following the design
of `std::fs` as much as reasonable.
It builds on the low-level library [`littlefs2-sys`](https://lib.rs/littlefs2-sys).

Some complications arise due to the lack of const generics in Rust, we work around these
with the [`generic-array`](https://lib.rs/generic-array) library, and long for the day when
constants associated to driver will be treated as constants by the compiler.

## Usage

This library requires an implementation of `littlefs2::driver::Storage`.

Roughly speaking, such an implementation defines a block device in terms of actual and
`typenum` constants, and supplies methods to read, erase and write.
The macro `ram_storage!` generates examples of this.

Beyond that, the filesystem and all files need memory for state, this has to be allocated
beforehand and passed to constructors.

Generally speaking, all operations on the filesystem require passing `&mut Storage`,
whereas all operations with files require passing both a `&mut Filesystem` and its
`&mut Storage` backend. This design choice was made to enable multiple filesystems
at different locations.


```
use littlefs2::fs::{Filesystem, File, OpenOptions, SeekFrom};
use littlefs2::prelude::*;
#
# use littlefs2::{consts, ram_storage, driver, io::Result};
#
# ram_storage!(
#     name=RamStorage,
#     backend=Ram,
#     trait=driver::Storage,
#     erase_value=0xff,
#     read_size=32,
#     write_size=32,
#     cache_size_ty=consts::U32,
#     block_size_ty=consts::U256,
#     block_size=256,
#     block_count=512,
#     lookaheadwords_size_ty=consts::U1,
#     filename_max_plus_one_ty=consts::U256,
#     path_max_plus_one_ty=consts::U256,
#     result=Result,
# );

// example storage backend
let mut ram = Ram::default();
let mut storage = RamStorage::new(&mut ram);

// must allocate state statically before use, must format before first mount
let mut alloc = Filesystem::allocate();
Filesystem::format(&mut alloc, &mut storage).unwrap();
let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

// must allocated state statically before use, may use common `OpenOptions`.
let mut alloc = File::allocate();
let mut file = OpenOptions::new()
	.read(true)
	.write(true)
	.create(true)
	.open("example.txt", &mut alloc, &mut fs, &mut storage)
	.unwrap();

// may read/write/seek as usual
file.write(&mut fs, &mut storage, b"Why is black smoke coming out?!").unwrap();
file.seek(&mut fs, &mut storage, SeekFrom::End(-24)).unwrap();
let mut buf = [0u8; 11];
assert_eq!(file.read(&mut fs, &mut storage, &mut buf).unwrap(), 11);
assert_eq!(&buf, b"black smoke");

// optionally unmount filesystem after use
fs.unmount(&mut storage).unwrap();

```

## Limitations

Directories and file attributes are not exposed yet.

*/

/// Low-level bindings
use littlefs2_sys as ll;

/// Re-export of `typenum::consts`.
pub use generic_array::typenum::consts;

pub mod prelude;

/// cf. Macros section below
#[macro_use]
pub mod macros;

pub mod fs;

/// Traits and types for core I/O functionality.
pub mod io;

pub mod path;

/// The `Storage`, `Read`, `Write` and `Seek` driver.
pub mod driver;

/// get information about the C backend
pub fn version() -> Version {
    Version {
        format: (ll::LFS_DISK_VERSION_MAJOR, ll::LFS_DISK_VERSION_MINOR),
        backend: (ll::LFS_VERSION_MAJOR, ll::LFS_VERSION_MINOR),
    }
}

/// Information about the C backend
#[derive(Copy,Clone,Debug)]
pub struct Version {
	/// On-disk format (currently: 2.0)
    pub format: (u32, u32),
	/// Backend release (currently: 2.1)
    pub backend: (u32, u32),
}

#[cfg(test)]
mod tests;
