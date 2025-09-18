# Snowflake-Me

English | [简体中文](README_CN.md)

[![Crates.io](https://img.shields.io/crates/v/snowflake_me.svg)](https://crates.io/crates/snowflake_me)
[![Docs.rs](https://docs.rs/snowflake_me/badge.svg)](https://docs.rs/snowflake_me)
[![Build](https://github.com/houseme/snowflake-rs/workflows/Build/badge.svg)](https://github.com/houseme/snowflake-rs/actions?query=workflow%3ABuild)
[![License](https://img.shields.io/crates/l/snowflake-me)](LICENSE-APACHE)

A high-performance, highly concurrent, distributed Snowflake ID generator in Rust.

This implementation is **lock-free**, designed for maximum throughput and minimum latency on multi-core CPUs.

## Highlights

- **Lock-Free Concurrency**: Uses `AtomicU64` and CAS (Compare-And-Swap) operations to manage internal state, completely
  eliminating the overhead of `Mutex` contention and context switching.
- **High Performance**: The lock-free design makes ID generation extremely fast, performing exceptionally well under
  high concurrency.
- **Highly Customizable**: The `Builder` pattern allows you to flexibly configure:
    - `start_time`: The epoch timestamp to shorten the time component of the generated ID.
    - `machine_id` and `data_center_id`: Identifiers for your machines and data centers.
    - Bit lengths for each component (`time`, `sequence`, `machine_id`, `data_center_id`).
- **Smart IP Fallback**: With the `ip-fallback` feature enabled, if `machine_id` or `data_center_id` are not provided,
  the system will automatically use the machine's local IP address.
    - **Supports both IPv4 and IPv6**: It prioritizes private IPv4 addresses and falls back to private IPv6 addresses if
      none are found.
    - **Conflict-Free**: To ensure uniqueness, `machine_id` and `data_center_id` are derived from **distinct parts** of
      the IP address:
        - **IPv4**: `data_center_id` from the 3rd octet, `machine_id` from the 4th octet.
        - **IPv6**: `data_center_id` from the 7th segment, `machine_id` from the 8th (last) segment.
- **Thread-Safe**: `Snowflake` instances can be safely cloned and shared across threads. Cloning is a lightweight
  operation (just an `Arc` reference count increment).

## Snowflake ID Structure

The generated ID is a 64-bit unsigned integer (`u64`) with the following default structure:

```text
+-------------------------------------------------------------------------------------------------+
| 1 Bit (Unused, Sign Bit) | 41 Bits (Timestamp, ms) | 5 Bits (Data Center ID) | 5 Bits (Machine ID) | 12 Bits (Sequence) |
+-------------------------------------------------------------------------------------------------+
```

- **Sign Bit (1 bit)**: Always 0 to ensure the ID is positive.
- **Timestamp (41 bits)**: Milliseconds elapsed since your configured `start_time`. 41 bits can represent about 69
  years.
- **Data Center ID (5 bits)**: Allows for up to 32 data centers.
- **Machine ID (5 bits)**: Allows for up to 32 machines per data center.
- **Sequence (12 bits)**: The number of IDs that can be generated per millisecond on a single machine. 12 bits allow for
  4096 IDs per millisecond.

**Note**: The bit lengths of all components are customizable via the `Builder`, but their sum must be 63.

## Quick Start

### 1. Add Dependency

Add this library to your `Cargo.toml`:

```toml
[dependencies]
snowflake_me = "0.4.0" # Please use the latest version
```

To enable the IP address fallback feature, enable the `ip-fallback` feature:

```toml
[dependencies]
snowflake_me = { version = "0.4.0", features = ["ip-fallback"] }
```

### 2. Basic Usage

```rust
use snowflake_me::Snowflake;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a generator with the default configuration.
    // Note: This requires the `ip-fallback` feature to auto-detect machine and data center IDs.
    let sf = Snowflake::new()?;

    // Generate a unique ID
    let id = sf.next_id()?;
    println!("Generated Snowflake ID: {}", id);

    Ok(())
}
```

### 3. Multi-threaded Usage

`Snowflake` instances can be efficiently cloned and shared between threads.

```rust
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
            // Verify that all IDs are unique
            assert!(all_ids.insert(id), "Found duplicate ID: {}", id);
        }
    }

    println!("Successfully generated {} unique IDs across 10 threads.", all_ids.len());
    Ok(())
}
```

### 4. Decomposing an ID

You can decompose a Snowflake ID back into its components for debugging or analysis.

```rust
use snowflake_me::{Snowflake, DecomposedSnowflake};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Ensure you use the same bit length configuration as when the ID was generated.
    let bit_len_time = 41;
    let bit_len_sequence = 12;
    let bit_len_data_center_id = 5;
    let bit_len_machine_id = 5;

    let sf = Snowflake::builder()
        .bit_len_time(bit_len_time)
        .bit_len_sequence(bit_len_sequence)
        .bit_len_data_center_id(bit_len_data_center_id)
        .bit_len_machine_id(bit_len_machine_id)
        .machine_id(&|| Ok(15))
        .data_center_id(&|| Ok(7))
        .finalize()?;

    let id = sf.next_id()?;
    let decomposed = DecomposedSnowflake::decompose(
        id,
        bit_len_time,
        bit_len_sequence,
        bit_len_data_center_id,
        bit_len_machine_id,
    );

    println!("ID: {}", decomposed.id);
    println!("Time: {}", decomposed.time);
    println!("Data Center ID: {}", decomposed.data_center_id);
    println!("Machine ID: {}", decomposed.machine_id);
    println!("Sequence: {}", decomposed.sequence);

    assert_eq!(decomposed.machine_id, 15);
    assert_eq!(decomposed.data_center_id, 7);

    Ok(())
}
```

## Contributing

Issues and Pull Requests are welcome.

## License

This project is dual-licensed under the [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE) licenses.