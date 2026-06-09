// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::builder::Builder;
use crate::error::*;
use base64::Engine;
use base64::engine::general_purpose;
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
    pub fn next_id(&self) -> Result<u64, Error> {
        let sequence_mask = (1u64 << self.0.bit_len_sequence) - 1;
        let time_shift = self.0.bit_len_sequence;
        let time_max = (1u64 << self.0.bit_len_time) - 1;

        loop {
            let current_state = self.0.state.load(Ordering::Relaxed);
            let last_time = current_state >> time_shift;

            let elapsed_time = current_elapsed_time(self.0.start_time) as u64;

            let (next_time, next_sequence) = if elapsed_time == last_time {
                // In the same millisecond, the serial number is incremented
                let sequence = (current_state & sequence_mask) + 1;
                if sequence > sequence_mask {
                    // The serial number has run out, busy waiting until the next millisecond
                    til_next_millis(self.0.start_time + last_time as i64);
                    continue; // Restart the loop to get a new timestamp
                }
                (last_time, sequence)
            } else {
                // new milliseconds, the serial number resets to 0
                (elapsed_time, 0)
            };

            if next_time > time_max {
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
                return Ok(id);
            }
            // CAS failure means that another thread has modified its state and the loop will be retried
        }
    }

    /// Decompose a Snowflake ID into its constituent parts using the generator's configuration.
    pub fn decompose(&self, id: u64) -> DecomposedSnowflake {
        DecomposedSnowflake::decompose(
            id,
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
pub struct DecomposedSnowflake {
    /// Original `u64` ID.
    pub id: u64,
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
            id,
            time: id >> time_shift,
            data_center_id: (id >> data_center_id_shift) & data_center_id_mask,
            machine_id: (id >> machine_id_shift) & machine_id_mask,
            sequence: (id >> sequence_shift) & sequence_mask,
        }
    }

    /// Returns the elapsed time component as nanoseconds.
    #[must_use]
    pub fn nanos_time(&self) -> i64 {
        (self.time as i64) * MICROS_PER_MILLI
    }

    /// Returns the ID as a signed `i64`.
    #[must_use]
    pub fn int64(&self) -> i64 {
        self.id as i64
    }

    /// Returns the decimal string representation of the ID.
    #[must_use]
    pub fn string(&self) -> String {
        self.id.to_string()
    }

    /// Returns the binary string representation of the ID.
    #[must_use]
    pub fn base2(&self) -> String {
        format!("{:b}", self.id)
    }

    /// Returns the base32 encoded string using a custom alphabet.
    #[must_use]
    pub fn base32(&self) -> String {
        const ENCODE_BASE32_MAP: &str = "ybndrfg8ejkmcpqxot1uwisza345h769";
        let mut id = self.id;
        if id < 32 {
            return ENCODE_BASE32_MAP
                .chars()
                .nth(id as usize)
                .unwrap()
                .to_string();
        }

        let mut b = Vec::new();
        while id >= 32 {
            b.push(ENCODE_BASE32_MAP.chars().nth((id % 32) as usize).unwrap());
            id /= 32;
        }
        b.push(ENCODE_BASE32_MAP.chars().nth(id as usize).unwrap());

        b.reverse();
        b.into_iter().collect()
    }

    /// Returns the lowercase hexadecimal string representation of the ID.
    #[must_use]
    pub fn hex(&self) -> String {
        format!("{:x}", self.id)
    }

    /// Returns the base36 encoded string (digits + lowercase letters).
    #[must_use]
    pub fn base36(&self) -> String {
        const CHARSET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let mut id = self.id;
        if id == 0 {
            return "0".to_string();
        }
        let mut buf = Vec::new();
        while id > 0 {
            buf.push(CHARSET[(id % 36) as usize]);
            id /= 36;
        }
        buf.reverse();
        String::from_utf8(buf).expect("base36 charset is valid UTF-8")
    }

    /// Returns the base58 encoded string.
    #[must_use]
    pub fn base58(&self) -> String {
        const ENCODE_BASE58_MAP: &str =
            "123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";
        let mut id = self.id;
        if id < 58 {
            return ENCODE_BASE58_MAP
                .chars()
                .nth(id as usize)
                .unwrap()
                .to_string();
        }

        let mut b = Vec::new();
        while id >= 58 {
            b.push(ENCODE_BASE58_MAP.chars().nth((id % 58) as usize).unwrap());
            id /= 58;
        }
        b.push(ENCODE_BASE58_MAP.chars().nth(id as usize).unwrap());

        b.reverse();
        b.into_iter().collect()
    }

    /// Returns the base64 encoded string of the raw 8-byte ID.
    #[must_use]
    pub fn base64(&self) -> String {
        general_purpose::STANDARD.encode(self.id.to_be_bytes())
    }

    /// Returns the decimal string representation as bytes.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.id.to_string().into_bytes()
    }

    /// Returns the raw 8-byte big-endian representation of the ID.
    #[must_use]
    pub fn int_bytes(&self) -> [u8; 8] {
        self.id.to_be_bytes()
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
