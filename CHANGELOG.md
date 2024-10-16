# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1] - 2024-11-16

- Released version 0.3.1 with `ip-fallback` feature including `pnet_datalink` dependency.

## [0.3.0] - 2024-10-10

### Added

- Implemented error handling for missing `machine_id` and `data_center_id` when the `ip-fallback` feature is disabled.
    - The `Builder`'s `finalize` method now returns an error if `machine_id` or
      `data_center_id` are not provided and the `ip-fallback` feature is not enabled.
    - This change ensures that users receive clear error messages when necessary identifiers are missing without the fallback mechanism.

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

[0.1.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.1.5

