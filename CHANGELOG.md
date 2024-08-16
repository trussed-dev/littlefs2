# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## Unreleased

### Added
- Added object-safe traits `DynFile`, `DynFilesystem` and `DynStorage` for
  accessing `Storage`, `Filesystem` and `File` implementations for any storage.
- Added `Filesystem::mount_or_else` function ([#57][])
- Marked `Path::is_empty`, `Path::from_bytes_with_nul`, `Path::from_cstr`, `Path::from_cstr_unchecked`, `Path::as_str_ref_with_trailing_nul`, `Path::as_str`, and `PathBuf::new` as `const`.
- Made `fs::FileOpenFlags` public and added `From<fs::FileOpenFlags>` for `fs::OpenOptions`.

### Fixed

- Fixed macro hygiene for `path!`.
- Fixed build error that would occur on Windows systems.
- Fixed compilation without default features.
- Added path iteration utilities ([#47][])

### Changed

- Enforced const evaluation for `path!`.
- Removed `cstr_core` and `cty` dependencies.
- Updated `littlefs2-sys` dependency to 0.2.0.
- Replace all panicking `Path`/`PathBuf` conversions with fallible alternatives:
  - Return a `Result` from `Path::from_str_with_nul`.
  - Replace the `From<_>` implementations for `Path` and `PathBuf` with `TryFrom<_>`, except for `From<&Path> for PathBuf`.
- Refactor error types:
  - Change `Error` enum to a struct with associated constants.
  - Remove `Error::Success` and enforce negative values for `Error`.
- Replace `Path::exists` with `Filesystem::exists`
- Replace `DynFilesystem::open_file_with_options_and_then{,unit}` with `DynFilesystem::open_file_with_flags_and_then{,unit}` using `FileOpenFlags` instead of `OpenOptionsCallback`
- Refactor attributes API:
  - Change the `set_attribute` function in `DynFilesystem` and `Filesystem` to accept an ID and a slice instead of an `Attribute`.
  - Add a buffer argument to the `attribute` function in `DynFilesystem` and `Filesystem` and return a slice of that buffer containing the read data.
  - Change the `Attribute` struct to store a slice with the read data and the total size of the attribute on the filesystem.

### Removed

- Removed `Path::from_bytes_with_nul_unchecked`.  Use `CStr::from_bytes_with_nul_unchecked` and `Path::from_cstr_unchecked` instead.
- Removed `From<littlefs2::path::Error> for littlefs2::io::Error`.
- Removed `object_safe::OpenOptionsCallback`.
- Removed `consts::ATTRBYTES_MAX_TYPE`.
- Removed `dir-entry-path` feature (now always enabled).

[#47]: https://github.com/trussed-dev/littlefs2/pull/47
[#57]: https://github.com/trussed-dev/littlefs2/pull/57

## [v0.4.0] - 2023-02-07

This release fixes an overflow of the lookahead buffer.  Users are advised to
upgrade from previous releases to avoid filesystem corruption.

### Added
- Added `Path::from_str_with_nul` and `path!` to create `Path` instances from
  `str`.
- Added `Eq` and `PartialEq` implementations for `Path`.

### Changed
- Made `Path::from_bytes_with_nul_unchecked` `const`.
- [breaking] Replaced `LOOKAHEADWORDS_SIZE` (measured in multiples of four
  bytes) with `LOOKAHEAD_SIZE` (measured in multiples of eight bytes) in
  `driver::Storage` so that all possible values are valid.  (See the lookahead
  size fix below for context.)
- [breaking] Require `&mut self` in `driver::Storage::read` for compatibility
  with `embedded_storage`.

### Fixed
- Fixed the lookahead size reported to `littlefs2-sys`.  Previously, the
  reported size was too large by the factor of 8, potentially leading to a
  buffer overflow causing filesystem corruption.  Fixing this means that
  `Storage::LOOKAHEADWORD_SIZE` values that are not a multiple of 2 can now
  lead to an error.  Fixes [#16].
- Propagate errors in littlefs callbacks instead of panicking.

[#16]: https://github.com/trussed-dev/littlefs2/issues/16

## [v0.3.2] - 2021-09-16

### Added
- Added the `c-stubs` feature for improved `no-std` compatiblity.

## [v0.3.1] - 2021-06-10

### Changed
- Removed the `PATH_MAX_PLUS_ONE` type from the `driver::Storage` trait.

## [v0.3.0] - 2021-06-10

*Yanked.*

### Changed
- Removed the `FILEBYTES_MAX`, `ATTRBYTES_MAX` and `FILENAME_MAX_PLUS_ONE`
  types from the `driver::Storage` trait as they determined by the backend.
- Updated to use resolver v2 to fix a dependency issue.

## [v0.2.2] - 2021-03-20

### Changed
- Added `remove_dir_all_when`, allowing to filter "rm -rf <path>"

## [v0.2.1] - 2021-02-26

### Changed
- PathBuf::from errors on embedded nuls, and prevents ending
  with nuls
- get rid of ufmt (oversight in 0.2 release)
- get rid of dead code (oversight in 0.2 release)

## [v0.2.0] - 2021-02-02

### Changed

- [breaking-change] The version of the `generic-array` dependency has been
  bumped to v0.14.2 (now that `heapless` v0.6.0` is out).

## [v0.1.1] - 2021-02-11

### Fixed

- `std`-triggering regression

[Unreleased]: https://github.com/trussed-dev/littlefs2/compare/0.4.0...HEAD
[0.4.0]: https://github.com/trussed-dev/littlefs2/releases/tag/0.4.0
[0.3.2]: https://github.com/trussed-dev/littlefs2/releases/tag/0.3.2
[0.3.1]: https://github.com/trussed-dev/littlefs2/releases/tag/0.3.1
[0.3.0]: https://github.com/trussed-dev/littlefs2/releases/tag/0.3.0
[0.2.2]: https://github.com/trussed-dev/littlefs2/releases/tag/0.2.2
[0.2.1]: https://github.com/trussed-dev/littlefs2/releases/tag/0.2.1
[0.2.0]: https://github.com/trussed-dev/littlefs2/releases/tag/0.2.0
[0.1.1]: https://github.com/trussed-dev/littlefs2/releases/tag/0.1.0
