<h1 align="center">littlefs2</h1>
<div align="center">
 <strong>
   Rust API for littlefs2
 </strong>
</div>

<br />

<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/littlefs2">
    <img src="https://img.shields.io/crates/v/littlefs2.svg?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/littlefs2">
    <img src="https://img.shields.io/crates/d/littlefs2.svg?style=flat-square"
      alt="Download" />
  </a>
  <!-- API docs -->
  <a href="https://docs.rs/littlefs2">
    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square"
      alt="main branch API docs" />
  </a>
</div>

## What is this?

[![Build Status][build-image]][build-link]

Idiomatic Rust API for the [littlefs][littlefs] microcontroller filesystem.

The `2` refers to the on-disk format, which includes support for inline files and file attributes.

The low-level bindings are provided by the [littlefs2-sys][littlefs2-sys] library.

We try to follow `std::fs` as much as reasonable.

Upstream release: [v2.1.4][upstream-release]

[build-image]: https://builds.sr.ht/~nickray/littlefs2.svg
[build-link]: https://builds.sr.ht/~nickray/littlefs2
[littlefs]: https://github.com/ARMmbed/littlefs
[littlefs2-sys]: https://lib.rs/littlefs2-sys
[upstream-release]: https://github.com/ARMmbed/littlefs/releases/tag/v2.1.4

#### License

<sup>littlefs is licensed under [BSD-3-Clause](https://github.com/ARMmbed/littlefs/blob/master/LICENSE.md).</sup>
<sup>This API for littlefs is licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.</sup>
<sup>Previous bindings exist in the [rust-littlefs](https://github.com/brandonedens/rust-littlefs) repository, also dual-licensed under Apache-2.0 and MIT.</sup>
<br>
<sub>Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.</sub>
