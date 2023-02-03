# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

### Added
- Added `Path::from_str_with_nul` and `path!` to create `Path` instances from
  `str`.

### Changed
- Made `Path::from_bytes_with_nul_unchecked` `const`.
- Replaced `LOOKAHEADWORDS_SIZE` (measured in multiples of four bytes) with
  `LOOKAHEAD_SIZE` (measured in multiples of eight bytes) in `driver::Storage`
  so that all possible values are valid.  (See the lookahead size fix below for
  context.)

### Fixed
- Fixed the lookahead size reported to `littlefs2-sys`.  Previously, the
  reported size was too large by the factor of 8, potentially leading to a
  buffer overflow causing filesystem corruption.  Fixing this means that
  `Storage::LOOKAHEADWORD_SIZE` values that are not a multiple of 2 can now
  lead to an error.  Fixes [#16].

[#16]: https://github.com/trussed-dev/littlefs2/issues/16

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
