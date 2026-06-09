# Snowflake-Me

English | [简体中文](README_CN.md)

[![Crates.io](https://img.shields.io/crates/v/snowflake-me.svg)](https://crates.io/crates/snowflake-me)
[![Docs.rs](https://docs.rs/snowflake_me/badge.svg)](https://docs.rs/snowflake_me)
[![Build](https://github.com/houseme/snowflake-rs/workflows/Build/badge.svg)](https://github.com/houseme/snowflake-rs/actions?query=workflow%3ABuild)
[![License](https://img.shields.io/crates/l/snowflake-me)](LICENSE-APACHE)

A high-performance, highly concurrent, distributed Snowflake ID generator in Rust.

This implementation is **lock-free**, designed for maximum throughput and minimum latency on multi-core CPUs.

## Highlights

- **Lock-Free Concurrency**: Uses `AtomicU64` and CAS (Compare-And-Swap) operations to manage internal state, completely eliminating the overhead of `Mutex` contention and context switching.
- **High Performance**: Cache-line aligned state (`#[repr(align(64))]`) prevents false sharing between threads. CAS loop defaults to `compare_exchange_weak` for better throughput on ARM and other architectures.
- **Highly Customizable**: The `Builder` pattern allows you to flexibly configure:
    - `start_time`: The epoch timestamp to shorten the time component of the generated ID.
    - `machine_id` and `data_center_id`: Identifiers for your machines and data centers.
    - Bit lengths for each component (`time`, `sequence`, `machine_id`, `data_center_id`).
    - Clock drift strategy and maximum allowed drift.
- **Batch Generation**: Generate multiple unique IDs in a single call with `next_ids(count)`, amortizing overhead across the batch.
- **Smart IP Fallback**: With the `ip-fallback` feature enabled, if `machine_id` or `data_center_id` are not provided, the system will automatically use the machine's local IP address.
    - **Supports both IPv4 and IPv6**: It prioritizes private IPv4 addresses and falls back to private IPv6 addresses if none are found.
    - **Conflict-Free**: To ensure uniqueness, `machine_id` and `data_center_id` are derived from **distinct parts** of the IP address.
- **`no_std` Support**: Works in `no_std` + `alloc` environments with a user-provided time source.
- **Thread-Safe**: `Snowflake` instances can be safely cloned and shared across threads. Cloning is a lightweight operation (just an `Arc` reference count increment).

## Snowflake ID Structure

The generated ID is a 64-bit unsigned integer (`u64`) with the following default structure:

```text
┌─────────────────────────────────────────────────────────────────┐
│ 0 │ 41 bits: time  │ 5 bits: dc │ 5 bits: machine │ 12 bits: seq │
└─────────────────────────────────────────────────────────────────┘
```

- **Sign Bit (1 bit)**: Always 0 to ensure the ID is positive.
- **Timestamp (41 bits)**: Milliseconds elapsed since your configured `start_time`. 41 bits can represent about 69 years.
- **Data Center ID (5 bits)**: Allows for up to 32 data centers.
- **Machine ID (5 bits)**: Allows for up to 32 machines per data center.
- **Sequence (12 bits)**: The number of IDs that can be generated per millisecond on a single machine. 12 bits allow for 4096 IDs per millisecond.

**Note**: The bit lengths of all components are customizable via the `Builder`, but their sum must be 63.

## Quick Start

### 1. Add Dependency

Add this library to your `Cargo.toml`:

```toml
[dependencies]
snowflake-me = "1.0"
```

To enable the IP address fallback feature:

```toml
[dependencies]
snowflake-me = { version = "1.0", features = ["ip-fallback"] }
```

To enable all optional features at once:

```toml
[dependencies]
snowflake-me = { version = "1.0", features = ["full"] }
```

### Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | Yes | Standard library support (time via `chrono`). Disable for `no_std` environments. |
| `ip-fallback` | No | Auto-detect `machine_id` and `data_center_id` from local network interfaces (IPv4/IPv6). Requires `std`. |
| `serde` | No | Serde `Serialize`/`Deserialize` for `SnowflakeId` (u64) and `SnowflakeIdString` (string). |
| `tracing` | No | Structured logging via `tracing` at key points (ID generation, clock drift, etc.). |
| `metrics` | No | Counters and gauges via `metrics` crate for observability. |
| `use-strong-cas` | No | Use `compare_exchange` instead of `compare_exchange_weak`. Slightly slower but eliminates spurious CAS failures. |
| `full` | No | Enables all optional features at once. |

### 2. Basic Usage

```rust
# #[cfg(feature = "std")] {
use snowflake_me::Snowflake;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a generator with explicit machine and data center IDs.
    // Alternatively, enable the `ip-fallback` feature and use `Snowflake::new()`
    // to auto-detect IDs from the local network interface.
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    // Generate a unique ID
    let id = sf.next_id()?;
    println!("Generated Snowflake ID: {id}");

    Ok(())
}
# }
```

### 3. Batch Generation

Generate multiple unique IDs in a single call:

```rust
# #[cfg(feature = "std")] {
use snowflake_me::Snowflake;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    let ids = sf.next_ids(100)?;
    println!("Generated {} IDs", ids.len());

    Ok(())
}
# }
```

### 4. Multi-threaded Usage

`Snowflake` instances can be efficiently cloned and shared between threads.

```rust
# #[cfg(feature = "std")] {
use snowflake_me::Snowflake;
use std::thread;
use std::sync::Arc;
use std::collections::HashSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Manually configure machine_id and data_center_id using the Builder.
    // This is the recommended approach for production environments.
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(10))
        .data_center_id(&|| Ok(5))
        .finalize()?;

    let sf_arc = Arc::new(sf);
    let mut handles = vec![];

    for _ in 0..10 {
        let sf_clone = Arc::clone(&sf_arc);
        let handle = thread::spawn(move || {
            let mut ids = Vec::new();
            for _ in 0..10000 {
                ids.push(sf_clone.next_id().unwrap());
            }
            ids
        });
        handles.push(handle);
    }

    let mut all_ids = HashSet::new();
    for handle in handles {
        let ids = handle.join().unwrap();
        for id in ids {
            assert!(all_ids.insert(id), "Found duplicate ID: {id}");
        }
    }

    println!("Successfully generated {} unique IDs across 10 threads.", all_ids.len());
    Ok(())
}
# }
```

### 5. Decomposing an ID

You can decompose a Snowflake ID back into its components for debugging or analysis.

```rust
# #[cfg(feature = "std")] {
use snowflake_me::Snowflake;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(15))
        .data_center_id(&|| Ok(7))
        .finalize()?;

    let id = sf.next_id()?;
    let decomposed = sf.decompose(id);

    println!("ID: {}", decomposed.id);
    println!("Time: {}", decomposed.time);
    println!("Data Center ID: {}", decomposed.data_center_id);
    println!("Machine ID: {}", decomposed.machine_id);
    println!("Sequence: {}", decomposed.sequence);

    assert_eq!(decomposed.machine_id, 15);
    assert_eq!(decomposed.data_center_id, 7);

    Ok(())
}
# }
```

### 6. Clock Drift Protection

If the system clock moves backward (e.g., due to NTP adjustments), the generator handles it based on the configured strategy. By default, it busy-waits until the clock catches up.

```rust
# #[cfg(feature = "std")] {
use snowflake_me::{Snowflake, ClockDriftStrategy};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .clock_drift_strategy(ClockDriftStrategy::Wait)
        .max_clock_drift_ms(5000)  // fail if drift exceeds 5 seconds
        .finalize()?;

    let id = sf.next_id()?;
    println!("Generated ID: {id}");

    Ok(())
}
# }
```

Available strategies:
- **`ClockDriftStrategy::Wait`** (default) — Busy-wait until the clock catches up. Optionally set `max_clock_drift_ms` to fail if the drift is too large.
- **`ClockDriftStrategy::Error`** — Return `Error::ClockDrift` immediately on backward drift.
- **`ClockDriftStrategy::LastTimestamp`** — Reuse the last known timestamp. IDs remain unique but timestamps become approximate.

### 7. `no_std` Usage

In `no_std` environments, disable default features and provide a time source:

```toml
[dependencies]
snowflake-me = { version = "1.0", default-features = false }
```

```rust,ignore
use snowflake_me::{Snowflake, set_time_source};

// Call periodically (e.g., from a timer interrupt or RTC read)
set_time_source(get_current_millis());

let sf = Snowflake::builder()
    .start_time(1_640_995_200_000) // 2022-01-01 UTC in milliseconds
    .machine_id(&|| Ok(1))
    .data_center_id(&|| Ok(1))
    .finalize()
    .unwrap();

let id = sf.next_id().unwrap();
```

## Migration from v0.6.x

If you are upgrading from v0.6.x, note the following breaking changes:

- `next_id()` now returns `Result<SnowflakeId, Error>` instead of `Result<u64, Error>`. Use `id.as_u64()` to get the raw `u64` value.
- `DecomposedSnowflake::id` field type changed from `u64` to `SnowflakeId`.
- `Error::NoPrivateIPv4` renamed to `Error::NoPrivateIP` (also attempts IPv6 fallback).
- `Error::MutexPoisoned` removed (the generator is now lock-free).
- New optional features: `serde`, `tracing`, `metrics`, `use-strong-cas`.

## Contributing

Issues and Pull Requests are welcome.

## License

This project is dual-licensed under the [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE) licenses.
