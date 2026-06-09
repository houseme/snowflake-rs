# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] - 2026-06-09

### Fixed

- Fixed `base36()` method which was incorrectly returning lowercase hex instead of actual base36 encoding.
- Fixed incorrect `SNOWFLAKE_TIME_UNIT` comment (was labeled "Nanosecond", now correctly documented as microseconds-per-millisecond).
- Fixed doc links using quotes instead of backticks in `Builder`.
- Fixed duplicate benchmark function names in `benches/bench.rs`.
- Fixed README doctests to work without `ip-fallback` feature.

### Changed

- Removed unused `Error::MutexPoisoned` variant (leftover from old Mutex-based design).
- **BREAKING**: Renamed `Error::NoPrivateIPv4` to `Error::NoPrivateIP` since the code also attempts IPv6 fallback.
- Added `#[must_use]` to all `Builder` and `DecomposedSnowflake` methods.
- Added `Display` trait implementation for `DecomposedSnowflake`.
- Added `hex()` method to `DecomposedSnowflake` for lowercase hex encoding.
- Added `elapsed_millis()` method to replace ambiguous `time()` method.
- Improved documentation across all modules with consistent formatting and accurate descriptions.
- Elided unnecessary lifetime in `Default for Builder<'_>`.
- Added `std::hint::spin_loop()` in `til_next_millis` for better CPU behavior during busy-wait.
- Benchmarks now use explicit `machine_id`/`data_center_id` instead of relying on `ip-fallback`.

## [0.5.0] - 2025-09-19

### Added

- Correct the version in `Cargo.toml` to `0.5.0`.
- Update `README.md` and `README_CN.md` to reflect the new version `0.5.0`.
- Change the crate name from `snowflake_me` to `snowflake-me`.
- Update all examples in `README.md` and `README_CN.md` to use the new crate name `snowflake-me`.
- Update the documentation links in `README.md` and `README_CN.md` to point to `snowflake-me`.

## [0.4.1] - 2025-09-18

### Fixed

- modify README.md crate name from `snowflake-me` to `snowflake_me`

## [0.4.0] - 2025-08-13

### Added

- **Configurable Bit Lengths**: The `Builder` now supports setting custom bit lengths for `time`, `sequence`,
  `data_center_id`, and `machine_id`. This allows tailoring the ID structure to specific application requirements.
- Added `Error::InvalidBitLength` to handle incorrect bit length configurations.
- Added `Error::DataCenterIdFailed` for more specific error reporting on builder finalization.

### Changed

- **BREAKING**: The free-standing `decompose()` function has been removed. ID parsing is now performed by the associated
  function `DecomposedSnowflake::decompose()`, which requires the bit length configuration used to generate the ID.
- **BREAKING**: All ID format conversion methods (e.g., `base58`, `base64`, `string`) are now methods on the
  `DecomposedSnowflake` struct, rather than being part of a separate module.
- The `msb` field was removed from `DecomposedSnowflake` as it was redundant (always 0).
- The internal time unit (`SNOWFLAKE_TIME_UNIT`) is now consistently 1 millisecond.

### Fixed

- Updated all tests to align with the new configurable bit length and decomposition APIs.

## [0.3.1] - 2024-11-16

- Released version 0.3.1 with `ip-fallback` feature including `pnet_datalink` dependency.

## [0.3.0] - 2024-10-10

### Added

- Implemented error handling for missing `machine_id` and `data_center_id` when the `ip-fallback` feature is disabled.
    - The `Builder`'s `finalize` method now returns an error if `machine_id` or
      `data_center_id` are not provided and the `ip-fallback` feature is not enabled.
    - This change ensures that users receive clear error messages when necessary identifiers are missing without the
      fallback mechanism.

### Changed

- Updated the version in `Cargo.toml` to `0.3.0`.

### CI/CD

- Modified GitHub Actions configuration to support running on branches matching `feature/**`.
    - This update enhances the development workflow by allowing CI runs on feature branches.

## [0.2.0] - 2024-09-04

### Added

- Split machine ID into 8 bits for machine ID and 8 bits for data center ID.
- Added `data_center_id` field to `SharedSnowflake` and `Builder`.
- Added `data_center_id` to `DecomposedSnowflake`.

### Changed

- Modified `next_id` method to use new `machine_id` and `data_center_id`.
- Updated `decompose` function to handle new `data_center_id`.

### Removed

- Removed support for 16-bit machine ID.

## [Unreleased]

## [0.1.5]

This is the initial version.

[0.6.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.6.0
[0.5.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.5.0
[0.4.1]: https://github.com/houseme/snowflake-rs/releases/tag/v0.4.1
[0.4.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.4.0
[0.3.1]: https://github.com/houseme/snowflake-rs/releases/tag/v0.3.1
[0.3.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.3.0
[0.2.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.2.0
[0.1.5]: https://github.com/houseme/snowflake-rs/releases/tag/v0.1.5

