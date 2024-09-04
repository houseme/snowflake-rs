# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

