# Snowflake-Me

English | [Chinese](README_CN.md)

[![Crates.io](https://img.shields.io/crates/v/snowflake_me.svg)](https://crates.io/crates/snowflake_me)
[![Docs.rs](https://docs.rs/snowflake_me/badge.svg)](https://docs.rs/snowflake_me)
[![Build](https://github.com/houseme/snowflake-rs/workflows/Build/badge.svg)](https://github.com/houseme/snowflake-rs/actions?query=workflow%3ABuild)
[![License](https://img.shields.io/crates/l/snowflake_me)](LICENSE-APACHE)
[![Miri](https://github.com/houseme/snowflake-rs/actions/workflows/miri.yml/badge.svg)](https://github.com/houseme/snowflake-rs/actions/workflows/miri.yml)
[![Semver](https://github.com/houseme/snowflake-rs/actions/workflows/semver.yml/badge.svg)](https://github.com/houseme/snowflake-rs/actions/workflows/semver.yml)

A high-performance, highly concurrent, distributed Snowflake ID generator in Rust.

ID allocation uses a **lock-free CAS loop**. Calls can wait when the sequence is exhausted or the clock moves backward; an optional `std` wait timeout bounds this waiting.

## Highlights

- **Lock-Free Concurrency**: Uses `AtomicU64` and CAS (Compare-And-Swap) operations to manage internal state, without a mutex in the allocation path.
- **High Performance**: Cache-line aligned state (`#[repr(align(64))]`) reduces adjacent-state sharing. Threads still contend on the same atomic state. The CAS loop defaults to `compare_exchange_weak`; compare both CAS modes on your target hardware.
- **Highly Customizable**: The `Builder` pattern allows you to flexibly configure:
    - `start_time`: The epoch timestamp to shorten the time component of the generated ID.
    - `machine_id` and `data_center_id`: Identifiers for your machines and data centers.
    - Bit lengths for each component (`time`, `sequence`, `machine_id`, `data_center_id`).
    - Clock drift strategy and maximum allowed drift.
- **Batch Generation**: Generate multiple unique IDs in a single call with `next_ids(count)`, amortizing overhead across the batch.
- **Smart IP Fallback**: With the `ip-fallback` feature enabled, if `machine_id` or `data_center_id` are not provided, the system will automatically use the machine's local IP address.
    - **Supports both IPv4 and IPv6**: It prioritizes private IPv4 addresses and falls back to private IPv6 addresses if none are found.
    - **Deployment constraint**: IDs are derived from distinct address components and truncated to the configured widths. Different addresses can produce the same node IDs; IP fallback does not coordinate node allocation.
- **`no_std` Support**: Works in `no_std` + `alloc` environments with a user-provided time source.
- **Thread-Safe**: `Snowflake` instances can be safely cloned and shared across threads. Cloning is a lightweight operation (just an `Arc` reference count increment).

## Snowflake ID Structure

The generated ID is a 64-bit unsigned integer (`u64`) with the following default structure:

```text
┌─────────────────────────────────────────────────────────────────┐
│ 0 │ 41 bits: time  │ 5 bits: dc │ 5 bits: machine │ 12 bits: seq │
└─────────────────────────────────────────────────────────────────┘
```

- **Sign Bit (1 bit)**: Always 0, so generated IDs fit in a non-negative `i64`. An ID can be zero at the configured epoch with zero node IDs and sequence.
- **Timestamp (41 bits)**: Milliseconds elapsed since your configured `start_time`. 41 bits can represent about 69 years.
- **Data Center ID (5 bits)**: Allows for up to 32 data centers.
- **Machine ID (5 bits)**: Allows for up to 32 machines per data center.
- **Sequence (12 bits)**: The number of IDs that can be generated per millisecond on a single machine. 12 bits allow for 4096 IDs per millisecond.

**Note**: The bit lengths must sum to 63. Machine and data center sections each support 0–16 bits; a zero-width section requires a zero ID. Invalid layouts are rejected before constructing the generator.

## Uniqueness and capacity

Share one generator, or its clones, for each `(data_center_id, machine_id)` pair. Separately constructed generators do not share state: two `Snowflake::default()` instances both use `(0, 0)` and can issue duplicate IDs. Production deployments must allocate distinct node pairs to independent generators using the same epoch and bit layout.

State is in memory. When reusing a node pair after a restart, ensure its clock is later than the previous generator's last issued timestamp, or use external coordination that prevents overlapping use and timestamp reuse. IP fallback is only a configuration convenience: with default widths, `192.168.1.1` and `192.168.33.33` both map to `(1, 1)`.

The default 12-bit sequence permits 4096 IDs per millisecond, approximately 4.096 million IDs per second per node pair under a normally advancing clock. Batching reduces atomic operations and allocation costs but does not increase that capacity. More sequence bits reduce the timestamp lifetime or node namespace; changing layouts requires a migration plan. See [benchmark methodology](docs/BENCHMARKS.md).

## Quick Start

### 1. Add Dependency

The library supports Rust 1.88 and newer. Development tests and benchmarks use the current stable toolchain. Source on `main` may include unreleased APIs; see [CHANGELOG.md](CHANGELOG.md).

Add this library to your `Cargo.toml`:

```toml
[dependencies]
snowflake_me = "2.1.1"
```

To enable the IP address fallback feature:

```toml
[dependencies]
snowflake_me = { version = "2.1.1", features = ["ip-fallback"] }
```

To enable all optional features at once:

```toml
[dependencies]
snowflake_me = { version = "2.1.1", features = ["full"] }
```

### Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | Yes | Standard library support (time via `jiff`). Disable for `no_std` environments. |
| `ip-fallback` | No | Auto-detect `machine_id` and `data_center_id` from local network interfaces (IPv4/IPv6). Requires `std`. |
| `serde` | No | Serde `Serialize`/`Deserialize` for `SnowflakeId` (u64) and `SnowflakeIdString` (string). |
| `tracing` | No | Structured logging at key points. Requires `std`. |
| `metrics` | No | Counters and gauges for observability. Requires `std`. |
| `use-strong-cas` | No | Use `compare_exchange` instead of `compare_exchange_weak`. Eliminates spurious CAS failures; performance depends on hardware and workload. |
| `full` | No | Enables `std`, `serde`, `tracing`, `metrics`, and `ip-fallback`; retains weak CAS. |

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

For a reusable allocation, append into an existing vector. On error, successfully appended IDs remain valid and reserved; the length increase tells you how many were generated:

```rust
use snowflake_me::Snowflake;
let sf = Snowflake::builder()
    .start_time(0)
    .clock_source(|| 1_000)
    .machine_id(&|| Ok(1))
    .data_center_id(&|| Ok(1))
    .finalize()?;
let mut ids = Vec::with_capacity(100);
sf.next_ids_into(100, &mut ids)?;
ids.clear(); // Reuse the allocation for the next batch.
sf.next_ids_into(100, &mut ids)?;
# Ok::<(), snowflake_me::Error>(())
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

If the system clock moves backward (e.g., due to NTP adjustments), the generator handles it based on the configured strategy. By default, it waits until the clock catches up, using a short spin followed by scheduler yields in `std` builds. A stopped clock can block indefinitely unless a wait timeout is configured.

```rust
# #[cfg(feature = "std")] {
use snowflake_me::{Snowflake, ClockDriftStrategy};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .clock_drift_strategy(ClockDriftStrategy::Wait)
        .max_clock_drift_ms(5000)  // fail if drift exceeds 5 seconds
        .wait_timeout(std::time::Duration::from_millis(100))
        .finalize()?;

    let id = sf.next_id()?;
    println!("Generated ID: {id}");

    Ok(())
}
# }
```

Available strategies:
- **`ClockDriftStrategy::Wait`** (default) — Wait until the clock catches up. `max_clock_drift_ms` must be non-negative and is checked throughout waiting, including waits caused by sequence exhaustion.
- **`ClockDriftStrategy::Error`** — Return `Error::ClockDrift` immediately on backward drift.
- **`ClockDriftStrategy::LastTimestamp`** — Reuse the last known timestamp while sequence numbers remain available. Exhaustion still waits for clock advancement, so this strategy does not provide unlimited availability during a rollback.

`wait_timeout(Duration)` is available with `std` and bounds the waiting period within one generation call using a monotonic clock. It is separate from the allowed wall-clock rollback magnitude. Calls can return `Error::WaitTimeout`; timestamps before the epoch are reported as `Error::BeforeEpoch` when they cannot be handled by the configured strategy.

For deterministic tests or a device clock, use `Builder::clock_source(F)` with a thread-safe closure returning Unix milliseconds. Clones share that clock; each independently built generator can use its own source. The default remains the system clock with `std`, or `set_time_source` without it.

### 7. `no_std` Usage

In `no_std` environments, disable default features and provide a time source. An allocator and native 64-bit atomics are required; CI checks `aarch64-unknown-none`, including the optional `serde` feature. The supplied clock must advance independently while a generation call waits (for example in an interrupt); updating it only after the call returns cannot release a sequence-exhaustion wait:

```toml
[dependencies]
snowflake_me = { version = "2.1.1", default-features = false }
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
