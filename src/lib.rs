// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A high-performance, highly concurrent, distributed Snowflake ID generator.
//!
//! This implementation is **lock-free**, designed for maximum throughput and minimum latency on multi-core CPUs.
//!
//! ## Highlights
//!
//! - **Lock-Free Concurrency**: Uses `AtomicU64` and CAS operations to manage internal state, eliminating `Mutex` lock overhead.
//! - **High Performance**: The lock-free design makes ID generation extremely fast, performing exceptionally well under high concurrency.
//! - **Highly Customizable**: The `Builder` pattern allows you to flexibly configure the start time, machine ID, data center ID, and the bit lengths of each component.
//! - **Smart IP Fallback**: With the `ip-fallback` feature enabled, if `machine_id` or `data_center_id` are not provided, the system automatically derives them from local network interfaces.
//!     - **Supports both IPv4 and IPv6**: It prioritizes private IPv4 addresses and falls back to private IPv6 addresses.
//!     - **Conflict-Free**: To ensure uniqueness, `machine_id` and `data_center_id` are derived from distinct parts of the IP address.
//! - **`no_std` Support**: Works in `no_std` + `alloc` environments with a user-provided time source.
//!
//! ## Architecture
//!
//! A Snowflake ID is a 64-bit integer composed of four sections:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ 0 │ 41 bits: time  │ 5 bits: dc │ 5 bits: machine │ 12 bits: seq │
//! └─────────────────────────────────────────────────────────────────┘
//!   ↑                                                                  │
//!   sign bit (always 0)                                                 │
//! ```
//!
//! - **Time** (41 bits): Milliseconds since the configured start time. Default epoch is 2022-01-01.
//! - **Data Center ID** (5 bits): Identifies the data center (0–31).
//! - **Machine ID** (5 bits): Identifies the machine within the data center (0–31).
//! - **Sequence** (12 bits): Per-millisecond counter (0–4095).
//!
//! The bit lengths are fully configurable via [`Builder`], as long as they sum to 63.
//!
//! ## Performance
//!
//! The generator uses a lock-free CAS (Compare-And-Swap) loop with cache-line aligned state
//! (`#[repr(align(64))]`) to prevent false sharing between threads. Under contention, it
//! defaults to `compare_exchange_weak` which allows spurious failures for better throughput
//! on ARM and other architectures. Enable the `use-strong-cas` feature to use
//! `compare_exchange` instead.
//!
//! ## Thread Safety
//!
//! [`Snowflake`] is safe to `Clone` and share across threads. Cloning is a cheap `Arc`
//! increment — all clones share the same internal state. The [`next_id`](Snowflake::next_id)
//! method is lock-free and safe to call concurrently from any number of threads.
//!
//! ## Feature Flags
//!
//! | Feature | Dependencies | Default | Description |
//! |---------|-------------|---------|-------------|
//! | `std` | chrono, thiserror/std | Yes | Standard library support (time via chrono) |
//! | `ip-fallback` | std, pnet_datalink | No | Auto-derive IDs from local IP address |
//! | `serde` | serde | No | Serde serialization for `SnowflakeId` and `DecomposedSnowflake` |
//! | `tracing` | tracing | No | Structured logging at key points |
//! | `metrics` | metrics | No | Runtime counters and gauges |
//! | `use-strong-cas` | — | No | Use `compare_exchange` instead of `compare_exchange_weak` |
//! | `full` | all of the above | No | Enable all optional features |
//!
//! ## Quick Start
//!
//! ### 1. Add Dependency
//!
//! Add this to your `Cargo.toml`. To use automatic IP-based configuration, enable the `ip-fallback` feature.
//!
//! ```toml
//! [dependencies]
//! snowflake-me = { version = "1.0", features = ["ip-fallback"] }
//! ```
//!
//! ### 2. Basic Usage
//!
//! ```rust
//! use snowflake_me::Snowflake;
//!
//! // Create a generator with the default configuration.
//! // Note: This requires the `ip-fallback` feature to auto-detect machine and data center IDs.
//! # #[cfg(feature = "ip-fallback")]
//! # {
//! let sf = Snowflake::new().unwrap();
//! let next_id = sf.next_id().unwrap();
//! println!("Generated ID: {}", next_id);
//! # }
//! ```
//!
//! ## Recommended Usage for Production
//!
//! For production environments, it is highly recommended to use the `Builder` to manually configure `machine_id` and `data_center_id` for maximum reliability.
//!
//! ```rust
//! use snowflake_me::Snowflake;
//! use std::thread;
//! use std::sync::Arc;
//! use std::collections::HashSet;
//!
//! // Manually configure IDs for reliability.
//! let sf = Snowflake::builder()
//!     .machine_id(&|| Ok(10))
//!     .data_center_id(&|| Ok(5))
//!     .finalize()
//!     .unwrap();
//!
//! let sf_arc = Arc::new(sf);
//! let mut handles = vec![];
//!
//! for _ in 0..10 {
//!     let sf_clone = Arc::clone(&sf_arc);
//!     let handle = thread::spawn(move || {
//!         let mut ids = Vec::new();
//!         for _ in 0..1000 {
//!             ids.push(sf_clone.next_id().unwrap());
//!         }
//!         ids
//!     });
//!     handles.push(handle);
//! }
//!
//! let mut all_ids = HashSet::new();
//! for handle in handles {
//!     let ids = handle.join().unwrap();
//!     for id in ids {
//!         // Verify that all IDs are unique
//!         assert!(all_ids.insert(id), "Found duplicate ID: {}", id);
//!     }
//! }
//!
//! println!("Successfully generated {} unique IDs.", all_ids.len());
//! ```
//!
//! ## Decomposing an ID
//!
//! You can decompose a Snowflake ID back into its original components.
//!
//! ```rust
//! use snowflake_me::{Snowflake, DecomposedSnowflake};
//!
//! // Use the same configuration that was used for generation.
//! let sf = Snowflake::builder()
//!     .machine_id(&|| Ok(15))
//!     .data_center_id(&|| Ok(7))
//!     .finalize()
//!     .unwrap();
//!
//! let id = sf.next_id().unwrap();
//!
//! // Decompose the ID using the generator's configuration.
//! let decomposed = sf.decompose(id);
//!
//! println!("ID: {}", decomposed.id);
//! println!("Time: {}", decomposed.time);
//! println!("Data Center ID: {}", decomposed.data_center_id);
//! println!("Machine ID: {}", decomposed.machine_id);
//! println!("Sequence: {}", decomposed.sequence);
//!
//! assert_eq!(decomposed.machine_id, 15);
//! assert_eq!(decomposed.data_center_id, 7);
//! ```
//!
//! ## Clock Drift Protection
//!
//! If the system clock moves backward (e.g., due to NTP adjustments), the generator
//! can handle it using one of three strategies:
//!
//! ```rust
//! use snowflake_me::{Snowflake, ClockDriftStrategy};
//!
//! let sf = Snowflake::builder()
//!     .machine_id(&|| Ok(1))
//!     .data_center_id(&|| Ok(1))
//!     .clock_drift_strategy(ClockDriftStrategy::Wait)
//!     .max_clock_drift_ms(5000)  // fail if drift > 5 seconds
//!     .finalize()
//!     .unwrap();
//! ```
//!
//! See [`ClockDriftStrategy`] for details on each strategy.
//!
//! ## `no_std` Usage
//!
//! In `no_std` environments, disable default features and provide a time source:
//!
//! ```rust,ignore
//! // In your timer interrupt or main loop:
//! snowflake_me::set_time_source(current_millis);
//!
//! // Then create the generator (start_time must be a raw i64 in milliseconds):
//! let sf = Snowflake::builder()
//!     .start_time(1_640_995_200_000) // 2022-01-01 UTC
//!     .machine_id(&|| Ok(1))
//!     .data_center_id(&|| Ok(1))
//!     .finalize()
//!     .unwrap();
//! ```
//!
//! [Twitter's Snowflake]: https://blog.twitter.com/2010/announcing-snowflake

#![doc(html_root_url = "https://docs.rs/snowflake-me/*")]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

mod builder;
/// Clock drift handling strategies for backward clock detection.
pub mod clock;
mod error;
/// The [`SnowflakeId`] newtype with encoding methods and trait implementations.
pub mod id;
mod snowflake;
pub(crate) mod time;

#[cfg(test)]
mod tests;

pub use builder::Builder;
pub use clock::ClockDriftStrategy;
pub use error::Error;
pub use id::SnowflakeId;
#[cfg(feature = "serde")]
pub use id::SnowflakeIdString;
pub use snowflake::{DecomposedSnowflake, Snowflake};
#[cfg(not(feature = "std"))]
pub use time::set_time_source;
