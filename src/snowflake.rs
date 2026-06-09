// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::builder::Builder;
use crate::clock::ClockDriftStrategy;
use crate::error::*;
use crate::id::SnowflakeId;
use chrono::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Shared state and configuration for the [`Snowflake`] generator.
///
/// Designed to be lock-free for high concurrency performance.
pub struct SharedSnowflake {
    /// Atomic state packing `elapsed_time` (high bits) and `sequence` (low bits).
    pub(crate) state: AtomicU64,
    /// Start timestamp in milliseconds since Unix epoch.
    pub(crate) start_time: i64,
    /// Data center ID.
    pub(crate) data_center_id: u16,
    /// Machine ID.
    pub(crate) machine_id: u16,
    /// Bit length of the timestamp section.
    pub(crate) bit_len_time: u8,
    /// Bit length of the sequence number section.
    pub(crate) bit_len_sequence: u8,
    /// Bit length of the data center ID section.
    pub(crate) bit_len_data_center_id: u8,
    /// Bit length of the machine ID section.
    pub(crate) bit_len_machine_id: u8,
    /// Strategy for handling backward clock drift.
    pub(crate) clock_drift_strategy: ClockDriftStrategy,
    /// Maximum allowed clock drift in milliseconds (for `Wait` strategy).
    pub(crate) max_clock_drift_ms: Option<i64>,
}

/// A high-performance, distributed, unique ID generator.
///
/// Instances can be safely cloned and shared across threads (cloning is a cheap `Arc` increment).
pub struct Snowflake(pub(crate) Arc<SharedSnowflake>);

impl Snowflake {
    /// Create a new `Snowflake` generator with default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if `machine_id` or `data_center_id` cannot be resolved
    /// (e.g., when the `ip-fallback` feature is disabled and no IDs are provided).
    pub fn new() -> Result<Self, Error> {
        Builder::new().finalize()
    }

    /// Create a new [`Builder`] to configure a `Snowflake` generator.
    pub fn builder<'a>() -> Builder<'a> {
        Builder::new()
    }

    pub(crate) fn new_inner(shared: Arc<SharedSnowflake>) -> Self {
        Self(shared)
    }

    /// Generate the next unique ID.
    ///
    /// This method is lock-free and thread-safe, using CAS operations for high concurrency.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OverTimeLimit`] if the timestamp exceeds the maximum value
    /// representable by the configured time bit length.
    ///
    /// Returns [`Error::ClockDrift`] when the clock drift strategy is [`ClockDriftStrategy::Error`]
    /// and backward clock drift is detected.
    ///
    /// Returns [`Error::ClockDriftExceeded`] when the clock drift strategy is [`ClockDriftStrategy::Wait`]
    /// and the drift exceeds `max_clock_drift_ms`.
    pub fn next_id(&self) -> Result<SnowflakeId, Error> {
        let sequence_mask = (1u64 << self.0.bit_len_sequence) - 1;
        let time_shift = self.0.bit_len_sequence;
        let time_max = (1u64 << self.0.bit_len_time) - 1;

        #[cfg(feature = "tracing")]
        tracing::trace!("generating next snowflake id");

        loop {
            let current_state = self.0.state.load(Ordering::Relaxed);
            let last_time = current_state >> time_shift;

            let elapsed_time = current_elapsed_time(self.0.start_time) as u64;

            // Clock drift detection: elapsed_time < last_time means clock went backward
            if elapsed_time < last_time {
                #[cfg(feature = "tracing")]
                tracing::warn!(
                    last_time,
                    current_time = elapsed_time,
                    strategy = ?self.0.clock_drift_strategy,
                    "clock drift detected"
                );
                match self.0.clock_drift_strategy {
                    ClockDriftStrategy::Wait => {
                        if let Some(max_drift) = self.0.max_clock_drift_ms {
                            let drift = last_time - elapsed_time;
                            if drift > max_drift as u64 {
                                return Err(Error::ClockDriftExceeded {
                                    drift_ms: drift,
                                    max_ms: max_drift,
                                });
                            }
                        }
                        til_next_millis(self.0.start_time + last_time as i64);
                        continue;
                    }
                    ClockDriftStrategy::Error => {
                        return Err(Error::ClockDrift {
                            last_time,
                            current_time: elapsed_time,
                        });
                    }
                    ClockDriftStrategy::LastTimestamp => {
                        let sequence = (current_state & sequence_mask) + 1;
                        if sequence > sequence_mask {
                            til_next_millis(self.0.start_time + last_time as i64);
                            continue;
                        }
                        let new_state = (last_time << time_shift) | sequence;
                        if self
                            .0
                            .state
                            .compare_exchange_weak(
                                current_state,
                                new_state,
                                Ordering::AcqRel,
                                Ordering::Relaxed,
                            )
                            .is_ok()
                        {
                            let id = (last_time
                                << (self.0.bit_len_data_center_id
                                    + self.0.bit_len_machine_id
                                    + self.0.bit_len_sequence))
                                | (u64::from(self.0.data_center_id)
                                    << (self.0.bit_len_machine_id + self.0.bit_len_sequence))
                                | (u64::from(self.0.machine_id) << self.0.bit_len_sequence)
                                | sequence;
                            return Ok(SnowflakeId::new(id));
                        }
                        continue;
                    }
                }
            }

            let (next_time, next_sequence) = if elapsed_time == last_time {
                // In the same millisecond, the serial number is incremented
                let sequence = (current_state & sequence_mask) + 1;
                if sequence > sequence_mask {
                    // The serial number has run out, busy waiting until the next millisecond
                    #[cfg(feature = "tracing")]
                    tracing::debug!("sequence exhausted, waiting for next millisecond");
                    til_next_millis(self.0.start_time + last_time as i64);
                    continue; // Restart the loop to get a new timestamp
                }
                (last_time, sequence)
            } else {
                // new milliseconds, the serial number resets to 0
                (elapsed_time, 0)
            };

            if next_time > time_max {
                #[cfg(feature = "tracing")]
                tracing::error!(time = next_time, max = time_max, "time limit exceeded");
                return Err(Error::OverTimeLimit);
            }

            // Pack the new time and serial number into a new state
            let new_state = (next_time << time_shift) | next_sequence;

            // Use CAS (Compare-And-Swap) to update status atomically
            // 'compare_exchange_weak' performs better at high concurrency because it allows for spurious failures,
            // It is safe to use in cycles.
            if self
                .0
                .state
                .compare_exchange_weak(
                    current_state,
                    new_state,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                let id = (next_time
                    << (self.0.bit_len_data_center_id
                        + self.0.bit_len_machine_id
                        + self.0.bit_len_sequence))
                    | (u64::from(self.0.data_center_id)
                        << (self.0.bit_len_machine_id + self.0.bit_len_sequence))
                    | (u64::from(self.0.machine_id) << self.0.bit_len_sequence)
                    | next_sequence;
                #[cfg(feature = "tracing")]
                tracing::trace!(time = next_time, sequence = next_sequence, "snowflake id generated");
                return Ok(SnowflakeId::new(id));
            }
            // CAS failure means that another thread has modified its state and the loop will be retried
        }
    }

    /// Decompose a Snowflake ID into its constituent parts using the generator's configuration.
    pub fn decompose(&self, id: SnowflakeId) -> DecomposedSnowflake {
        DecomposedSnowflake::decompose(
            id.as_u64(),
            self.0.bit_len_time,
            self.0.bit_len_sequence,
            self.0.bit_len_data_center_id,
            self.0.bit_len_machine_id,
        )
    }
}

impl Clone for Snowflake {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Microseconds per millisecond, used to convert nanoseconds to milliseconds.
const MICROS_PER_MILLI: i64 = 1_000_000;

pub(crate) fn to_snowflake_time(time: DateTime<Utc>) -> i64 {
    time.timestamp_nanos_opt().unwrap_or(0) / MICROS_PER_MILLI
}

fn current_elapsed_time(start_time: i64) -> i64 {
    to_snowflake_time(Utc::now()) - start_time
}

fn til_next_millis(last_timestamp: i64) {
    let mut now = to_snowflake_time(Utc::now());
    while now <= last_timestamp {
        std::hint::spin_loop();
        now = to_snowflake_time(Utc::now());
    }
}

/// All components of a decomposed Snowflake ID.
///
/// Created by calling [`Snowflake::decompose`] or [`DecomposedSnowflake::decompose`].
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DecomposedSnowflake {
    /// The Snowflake ID.
    pub id: SnowflakeId,
    /// Elapsed milliseconds since the configured start time.
    pub time: u64,
    /// Sequence number within the same millisecond.
    pub sequence: u64,
    /// Data center ID.
    pub data_center_id: u64,
    /// Machine ID.
    pub machine_id: u64,
}

impl DecomposedSnowflake {
    /// Decompose a Snowflake ID into its components based on the provided bit lengths.
    ///
    /// # Panics
    ///
    /// Panics if the total bit length does not equal 63.
    pub fn decompose(
        id: u64,
        bit_len_time: u8,
        bit_len_sequence: u8,
        bit_len_data_center_id: u8,
        bit_len_machine_id: u8,
    ) -> Self {
        let total_bits = bit_len_time as u32
            + bit_len_sequence as u32
            + bit_len_data_center_id as u32
            + bit_len_machine_id as u32;
        assert_eq!(total_bits, 63, "Total bit length must be 63");

        // 修正位移计算
        let sequence_shift = 0;
        let machine_id_shift = sequence_shift + bit_len_sequence;
        let data_center_id_shift = machine_id_shift + bit_len_machine_id;
        let time_shift = data_center_id_shift + bit_len_data_center_id;

        let sequence_mask = (1u64 << bit_len_sequence) - 1;
        let machine_id_mask = (1u64 << bit_len_machine_id) - 1;
        let data_center_id_mask = (1u64 << bit_len_data_center_id) - 1;

        Self {
            id: SnowflakeId::new(id),
            time: id >> time_shift,
            data_center_id: (id >> data_center_id_shift) & data_center_id_mask,
            machine_id: (id >> machine_id_shift) & machine_id_mask,
            sequence: (id >> sequence_shift) & sequence_mask,
        }
    }

    /// Returns the underlying `SnowflakeId`.
    #[must_use]
    pub fn to_id(&self) -> SnowflakeId {
        self.id
    }

    /// Returns the elapsed time component as nanoseconds.
    #[must_use]
    pub fn nanos_time(&self) -> i64 {
        (self.time as i64) * MICROS_PER_MILLI
    }

    /// Returns the ID as a signed `i64`.
    #[must_use]
    pub fn int64(&self) -> i64 {
        self.id.int64()
    }

    /// Returns the decimal string representation of the ID.
    #[must_use]
    pub fn string(&self) -> String {
        self.id.string()
    }

    /// Returns the binary string representation of the ID.
    #[must_use]
    pub fn base2(&self) -> String {
        self.id.base2()
    }

    /// Returns the base32 encoded string using a custom alphabet.
    #[must_use]
    pub fn base32(&self) -> String {
        self.id.base32()
    }

    /// Returns the lowercase hexadecimal string representation of the ID.
    #[must_use]
    pub fn hex(&self) -> String {
        self.id.hex()
    }

    /// Returns the base36 encoded string (digits + lowercase letters).
    #[must_use]
    pub fn base36(&self) -> String {
        self.id.base36()
    }

    /// Returns the base58 encoded string.
    #[must_use]
    pub fn base58(&self) -> String {
        self.id.base58()
    }

    /// Returns the base64 encoded string of the raw 8-byte ID.
    #[must_use]
    pub fn base64(&self) -> String {
        self.id.base64()
    }

    /// Returns the decimal string representation as bytes.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.id.bytes()
    }

    /// Returns the raw 8-byte big-endian representation of the ID.
    #[must_use]
    pub fn int_bytes(&self) -> [u8; 8] {
        self.id.int_bytes()
    }

    /// Returns the elapsed time in milliseconds since the configured start time.
    #[must_use]
    pub fn elapsed_millis(&self) -> u64 {
        self.time
    }
}

impl std::fmt::Display for DecomposedSnowflake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "id={}, time={}, data_center={}, machine={}, seq={}",
            self.id, self.time, self.data_center_id, self.machine_id, self.sequence
        )
    }
}

impl From<&DecomposedSnowflake> for SnowflakeId {
    fn from(decomposed: &DecomposedSnowflake) -> Self {
        decomposed.id
    }
}
