#![no_std]

/*!

[littlefs](https://github.com/ARMmbed/littlefs) is a filesystem for microcontrollers
written in C, that claims to be *fail-safe*:
- power-loss resilience, by virtue of copy-on-write guarantees
- bounded RAM/ROM, with stack-allocated buffers

Since [version 2](https://github.com/ARMmbed/littlefs/releases/tag/v2.0.0), it has
some nifty features such as:
- dynamic wear-leveling, including detection of bad Flash blocks
- custom user attributes
- inline files, avoiding block waste

For more background, see its [design notes](https://github.com/ARMmbed/littlefs/blob/master/DESIGN.md)
and the [specification](https://github.com/ARMmbed/littlefs/blob/master/SPEC.md) of its format.

### What is this?

This library, [`littlefs2`](https://lib.rs/littlefs2), offers an idiomatic Rust API for littlefs.

It follows the design of [`std::fs`](https://doc.rust-lang.org/std/fs/index.html) as much as reasonable,
and builds on the bindings [`littlefs2-sys`](https://lib.rs/littlefs2-sys).

Some complications arise due to the lack of const generics in Rust, we work around these
with the [`generic-array`](https://lib.rs/generic-array) library, and long for the day when
constants associated to traits will be treated as constants by the compiler.

⯈ [**The best place to start reading the API docs is here**](fs/index.html). ⯇

### Usage

To use this library, implement `littlefs2::driver::Storage`.
The macro `ram_storage!` generates examples of this.

Roughly speaking, the [`Storage`](driver/trait.Storage.html) trait defines a block device in
terms of actual and `typenum` constants, and an implementation supplies methods to read, erase and write.

The filesystem and each open file need memory for state and caching, this has to be allocated
beforehand and passed to constructors.

### Design notes

All operations on the filesystem require passing a `&mut Storage`, which guarantees by Rust's
borrow checker that only one thread can manipulate the filesystem.
This design choice (as opposed to consuming the Storage, which would be less verbose) was made to
enable use of the underlying flash peripheral outside of the filesystem (the `Storage` can be
dropped and reconstructed).  For instance, one could setup an additional filesystem,
or handle some flash data manually.

As an experiment, we implemented [`ReadDirWith`](fs/struct.ReadDirWith.html). It converts a
[`ReadDir`](fs/struct.ReadDir.html) (which needs mutable references, and so is "not quite an iterator"
over the files of a directory), into a true iterator, by temporarily binding the mutable references.

Currying with lifetime gymnastics!

In the future, we may extend this approach to other operations, thus adding a secondary API layer.

<https://play.rust-lang.org/?edition=2018&gist=c86abf99fc87551cfe3136e398a45d19>

Separately, keeping track of the allocations is a chore, we hope that
[`Pin`](https://doc.rust-lang.org/core/pin/index.html) magic will help fix this.

### Example

```
# use littlefs2::fs::{Filesystem, File, OpenOptions, SeekFrom};
# use littlefs2::io::prelude::*;
#
# use littlefs2::{consts, ram_storage, driver, io::Result};
#
#
// example storage backend
ram_storage!(tiny);
let mut ram = Ram::default();
let mut storage = RamStorage::new(&mut ram);

// must format before first mount
Filesystem::format(&mut storage).unwrap();
// must allocate state statically before use
let mut alloc = Filesystem::allocate();
let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

// must allocate state statically before use
let mut alloc = File::allocate();
// may use common `OpenOptions`
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
```
*/

/// Low-level bindings
use littlefs2_sys as ll;

/// Re-export of `typenum::consts`.
pub use generic_array::typenum::consts;

/// cf. Macros documentation
#[macro_use]
pub mod macros;

pub mod driver;

pub mod fs;
#[cfg(feature = "closures")]
pub mod fsc;
pub mod io;
pub mod path;

/// get information about the C backend
pub fn version() -> Version {
    Version {
        format: (ll::LFS_DISK_VERSION_MAJOR, ll::LFS_DISK_VERSION_MINOR),
        backend: (ll::LFS_VERSION_MAJOR, ll::LFS_VERSION_MINOR),
    }
}

/// Information about the C backend
#[derive(Clone,Copy,Debug)]
pub struct Version {
	/// On-disk format (currently: 2.0)
    pub format: (u32, u32),
	/// Backend release (currently: 2.1)
    pub backend: (u32, u32),
}

#[cfg(test)]
mod tests;
