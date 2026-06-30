# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.1] - 2026-06-30

### Changed

- Bumped the internal `jiff` dependency from `0.2.28` to `0.2.31`.
- Refreshed `Cargo.lock` to match the current dependency graph.
- Updated GitHub Actions workflow dependencies to the current major versions of `actions/checkout` and `actions/cache`.
- Switched the CI MSRV job to the stable toolchain while keeping the job in place as a compatibility smoke check.

### Fixed

- Corrected the package author metadata in `Cargo.toml`.
- Updated README and crate-level documentation snippets to consistently reference `snowflake-me = "2.0.1"`.

## [2.0.0] - 2026-06-16

### Changed

- **Time dependency switched from `chrono` to `jiff`** (`jiff 0.2.28`).
  - `current_millis()` (std path) now uses `jiff::Timestamp::now().as_millisecond()` instead of `chrono`'s nanosecond division.
  - The `std` feature now enables `dep:jiff` + `jiff/std` (previously `chrono`).
  - MSRV is unchanged: `jiff 0.2.28` targets Rust 1.70, still within this crate's 1.75 MSRV.
  - `jiff` is now an internal dependency only — no datetime type appears in the public API.

### Removed

- Internal `to_snowflake_time()` helper removed (the start time is now stored as raw milliseconds).
- Misnamed `MICROS_PER_MILLI` constant removed; replaced by the correctly-named `NANOS_PER_MILLI` (used only by `DecomposedSnowflake::nanos_time()`).

### Breaking Changes

- **`Builder::start_time` is unified to `start_time(i64)`** — milliseconds since the Unix epoch — for both `std` and `no_std`. Previously the `std` build accepted a `chrono::DateTime<chrono::Utc>`. Pass a raw millisecond timestamp instead (e.g. `1_640_995_200_000` for 2022-01-01 UTC).
- **`Error::StartTimeAheadOfCurrentTime` payload changed** from `chrono::DateTime<Utc>` to `i64` (the offending start time in milliseconds).

### Migration

Replace any `start_time` call that passed a `chrono::DateTime<chrono::Utc>` with its millisecond value:

```rust
// Before (v1.x, std)
// .start_time(chrono::Utc::now())

// After (v2.0)
.start_time(/* millis since epoch, e.g. from your clock or a fixed epoch */)
```

See `docs/chrono-to-jiff-migration.md` for the full feasibility analysis and rationale.

## [1.0.0] - 2026-06-09

### Added

- **API stabilization**: public API frozen for v1.x stability guarantee.
- **`missing_docs` lint**: all public items now have documentation.
- **Crate-level documentation**: architecture overview with bit layout diagram, performance characteristics, feature flags table, thread safety notes, `no_std` usage guide.
- **Property-based tests** (`tests/proptest.rs`): monotonicity, decompose roundtrip, uniqueness under contention.
- **Fuzz tests** (`fuzz/`): targets for `fuzz_decompose`, `fuzz_parse_id`, `fuzz_encodings`.
- **Loom concurrency tests** (`tests/loom_test.rs`): concurrent `next_id` and `next_ids` verification.
- **Example code** (`examples/`): `basic.rs`, `builder.rs`, `concurrent.rs`, `decompose.rs`, `serde_example.rs`.
- **CI/CD improvements**:
  - Matrix testing across Linux/macOS/Windows and stable/beta/nightly.
  - MSRV verification (Rust 1.75).
  - WASM build check (`wasm32-unknown-unknown`).
  - Documentation check with `-D warnings`.
  - Code coverage workflow with `cargo-llvm-cov`.

### Changed

- `SharedSnowflake` visibility changed from `pub` to `pub(crate)`.
- Added `#[must_use]` to `Snowflake::builder()`, `Snowflake::decompose()`, `DecomposedSnowflake::decompose()`.
- **README overhaul**: removed outdated v0.5.0 rename notice, added `no_std` usage guide, batch generation example, migration guide from v0.6.x, complete feature flags table (`std`, `use-strong-cas`).

## [0.9.0] - 2026-06-09

### Added

- **Expanded benchmark suite** (`benches/bench.rs`): 7 Criterion benchmarks covering single-thread ID generation, concurrent generation, builder finalization, decompose, encoding methods, and CAS strategy comparison.
- **`use-strong-cas` feature flag**: switches CAS from `compare_exchange_weak` to `compare_exchange` for environments where spurious CAS failures are undesirable.
- **`next_ids(count)` batch API**: generates multiple unique IDs in a single call, amortizing overhead.
- **`no_std` support**: the crate now works in `no_std` + `alloc` environments.
  - `std` feature (default-enabled) gates `chrono` and `thiserror/std`.
  - Time source abstraction in `src/time.rs`: `current_millis()` uses `chrono` for std, `AtomicI64` for no_std.
  - `set_time_source(millis)` public API for no_std environments to provide the current time.
  - All internal imports use `core::` and `alloc::` for no_std compatibility.
- **Cache-line alignment**: `SharedSnowflake` is now `#[repr(align(64))]` with hot/cold field ordering to prevent false sharing between threads.

### Changed

- **Feature flag refactor**: `chrono` is now an optional dependency gated behind the `std` feature.
- `thiserror` and `base64` use `default-features = false` for no_std compatibility.
- `Builder` fields reordered: hot-path fields grouped separately from cold configuration.

## [0.8.0] - 2026-06-09

### Added

- **Clock drift protection**: `ClockDriftStrategy` enum with three strategies:
  - `Wait` (default) — busy-wait until clock catches up, with optional `max_clock_drift_ms` limit.
  - `Error` — return `Error::ClockDrift` immediately on backward drift.
  - `LastTimestamp` — reuse last known timestamp, IDs remain unique but timestamps become approximate.
- **Builder methods**: `clock_drift_strategy()` and `max_clock_drift_ms()` for configuring drift behavior.
- **Error variants**: `Error::ClockDrift { last_time, current_time }` and `Error::ClockDriftExceeded { drift_ms, max_ms }`.
- **Tracing integration** (optional `tracing` feature): structured logging at key points — ID generation, clock drift, sequence exhaustion, time overflow, generator initialization.
- **Metrics integration** (optional `metrics` feature): counters and gauges — `snowflake_ids_generated_total`, `snowflake_clock_drift_events_total`, `snowflake_sequence_exhaustion_total`, `snowflake_sequence_utilization`.
- **`full` feature flag**: enables all optional features (`serde`, `tracing`, `metrics`, `ip-fallback`) at once.
- Integration tests for tracing (`tests/tracing_test.rs`) and metrics (`tests/metrics_test.rs`).

### Changed

- `SharedSnowflake` now includes `clock_drift_strategy` and `max_clock_drift_ms` fields.

## [0.7.0] - 2026-06-09

### Added

- **`SnowflakeId` newtype**: Introduced `SnowflakeId(u64)` wrapper with rich encoding methods (`hex`, `base2`, `base32`, `base36`, `base58`, `base64`).
- **Standard trait implementations**: `Display`, `From<u64>`, `Into<u64>`, `AsRef<u64>`, `Deref<Target=u64>`, `Ord`, `PartialOrd`, `FromStr`, `Hash`, `Eq`.
- **`FromStr` hex parsing**: Supports `0x`/`0X` prefix for hexadecimal input.
- **`TryFrom` implementations**: `TryFrom<String>`, `TryFrom<&str>`, `TryFrom<i64>`.
- **Serde support** (optional): `serde` feature flag for `SnowflakeId` (u64 format), `SnowflakeIdString` (string format), and `DecomposedSnowflake`.
- **`SnowflakeIdString`**: Wrapper type that serializes `SnowflakeId` as a decimal string (for JavaScript-safe JSON).
- `DecomposedSnowflake::to_id()` method and `From<&DecomposedSnowflake> for SnowflakeId`.

### Changed

- **BREAKING**: `Snowflake::next_id()` now returns `Result<SnowflakeId, Error>` instead of `Result<u64, Error>`.
- **BREAKING**: `Snowflake::decompose()` now accepts `SnowflakeId` instead of `u64`.
- **BREAKING**: `DecomposedSnowflake::id` field type changed from `u64` to `SnowflakeId`.
- Encoding methods on `DecomposedSnowflake` now delegate to `SnowflakeId`.
- `Error::ParseIdFailed(String)` added for ID parsing failures.

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

[2.0.1]: https://github.com/houseme/snowflake-rs/releases/tag/v2.0.1
[2.0.0]: https://github.com/houseme/snowflake-rs/releases/tag/v2.0.0
[1.0.0]: https://github.com/houseme/snowflake-rs/releases/tag/v1.0.0
[0.8.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.8.0
[0.7.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.7.0
[0.6.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.6.0
[0.5.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.5.0
[0.4.1]: https://github.com/houseme/snowflake-rs/releases/tag/v0.4.1
[0.4.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.4.0
[0.3.1]: https://github.com/houseme/snowflake-rs/releases/tag/v0.3.1
[0.3.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.3.0
[0.2.0]: https://github.com/houseme/snowflake-rs/releases/tag/v0.2.0
[0.1.5]: https://github.com/houseme/snowflake-rs/releases/tag/v0.1.5
