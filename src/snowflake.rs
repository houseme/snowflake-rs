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
use crate::time;
use core::sync::atomic::{AtomicU64, Ordering};

extern crate alloc;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// Shared state and configuration for the [`Snowflake`] generator.
///
/// Designed to be lock-free for high concurrency performance.
/// Cache-line aligned to prevent false sharing between threads.
#[repr(align(64))]
pub(crate) struct SharedSnowflake {
    // Hot path — written by every next_id() call
    /// Atomic state packing `elapsed_time` (high bits) and `sequence` (low bits).
    pub(crate) state: AtomicU64,

    // Cold path — read-only after init
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
    #[must_use]
    pub fn builder<'a>() -> Builder<'a> {
        Builder::new()
    }

    pub(crate) fn new_inner(shared: Arc<SharedSnowflake>) -> Self {
        Self(shared)
    }

    /// Atomically update the packed state via compare-and-swap.
    ///
    /// Uses `compare_exchange_weak` by default — it permits spurious failures,
    /// which are safe inside a retry loop and yield better throughput under
    /// contention (especially on ARM). Enable the `use-strong-cas` feature to
    /// use the stronger `compare_exchange` instead.
    fn cas(&self, current: u64, new: u64) -> bool {
        if cfg!(feature = "use-strong-cas") {
            self.0
                .state
                .compare_exchange(current, new, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        } else {
            self.0
                .state
                .compare_exchange_weak(current, new, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        }
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
                #[cfg(feature = "metrics")]
                metrics::counter!("snowflake_clock_drift_events_total").increment(1);
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
                        let cas_ok = self.cas(current_state, new_state);
                        if cas_ok {
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
                    #[cfg(feature = "metrics")]
                    metrics::counter!("snowflake_sequence_exhaustion_total").increment(1);
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
            // compare_exchange_weak performs better at high concurrency because it allows spurious failures,
            // which is safe in retry loops. compare_exchange is stronger but slightly slower.
            let cas_ok = self.cas(current_state, new_state);
            if cas_ok {
                let id = (next_time
                    << (self.0.bit_len_data_center_id
                        + self.0.bit_len_machine_id
                        + self.0.bit_len_sequence))
                    | (u64::from(self.0.data_center_id)
                        << (self.0.bit_len_machine_id + self.0.bit_len_sequence))
                    | (u64::from(self.0.machine_id) << self.0.bit_len_sequence)
                    | next_sequence;
                #[cfg(feature = "metrics")]
                {
                    metrics::counter!("snowflake_ids_generated_total").increment(1);
                    metrics::gauge!("snowflake_sequence_utilization")
                        .set(next_sequence as f64 / sequence_mask as f64);
                }
                #[cfg(feature = "tracing")]
                tracing::trace!(
                    time = next_time,
                    sequence = next_sequence,
                    "snowflake id generated"
                );
                return Ok(SnowflakeId::new(id));
            }
            // CAS failure means that another thread has modified its state and the loop will be retried
        }
    }

    /// Generate multiple unique IDs in a single call.
    ///
    /// Allocates contiguous sequence numbers in batches within the CAS loop,
    /// so generating `n` IDs typically needs far fewer atomic operations than
    /// calling [`next_id`](Self::next_id) `n` times. Returned IDs are in
    /// monotonically increasing order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OverTimeLimit`] if the timestamp exceeds the maximum
    /// value representable by the configured time bit length, or a clock drift
    /// error ([`Error::ClockDrift`] / [`Error::ClockDriftExceeded`]) depending
    /// on the configured [`ClockDriftStrategy`].
    pub fn next_ids(&self, count: usize) -> Result<Vec<SnowflakeId>, Error> {
        if count == 0 {
            return Ok(Vec::new());
        }

        let sequence_mask = (1u64 << self.0.bit_len_sequence) - 1;
        let time_shift = self.0.bit_len_sequence;
        let time_max = (1u64 << self.0.bit_len_time) - 1;

        // Pre-compute the bit offsets and fixed components used to assemble each ID.
        let machine_shift = self.0.bit_len_sequence;
        let data_center_shift = self.0.bit_len_machine_id + machine_shift;
        let time_total_shift = self.0.bit_len_data_center_id + data_center_shift;
        let data_center_bits = u64::from(self.0.data_center_id) << data_center_shift;
        let machine_bits = u64::from(self.0.machine_id) << machine_shift;
        let assemble = |time: u64, seq: u64| {
            SnowflakeId::new((time << time_total_shift) | data_center_bits | machine_bits | seq)
        };

        let mut ids: Vec<SnowflakeId> = Vec::with_capacity(count);

        while ids.len() < count {
            let remaining = count - ids.len();
            let current_state = self.0.state.load(Ordering::Relaxed);
            let last_time = current_state >> time_shift;
            let current_seq = current_state & sequence_mask;
            let elapsed_time = current_elapsed_time(self.0.start_time) as u64;

            // Clock drift detection: elapsed_time < last_time means the clock went backward.
            if elapsed_time < last_time {
                #[cfg(feature = "metrics")]
                metrics::counter!("snowflake_clock_drift_events_total").increment(1);
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
                        // Reuse last_time; reserve a contiguous run of sequence numbers.
                        let next_seq = current_seq + 1;
                        if next_seq > sequence_mask {
                            til_next_millis(self.0.start_time + last_time as i64);
                            continue;
                        }
                        let avail = sequence_mask - next_seq + 1;
                        let reserved = (remaining as u64).min(avail);
                        let new_state = (last_time << time_shift) | (next_seq + reserved - 1);
                        if self.cas(current_state, new_state) {
                            for seq in next_seq..next_seq + reserved {
                                ids.push(assemble(last_time, seq));
                            }
                        }
                        continue;
                    }
                }
            }

            let (next_time, next_seq, avail) = if elapsed_time == last_time {
                let next_seq = current_seq + 1;
                if next_seq > sequence_mask {
                    // Sequence exhausted within this millisecond — wait for the next one.
                    #[cfg(feature = "metrics")]
                    metrics::counter!("snowflake_sequence_exhaustion_total").increment(1);
                    #[cfg(feature = "tracing")]
                    tracing::debug!("sequence exhausted, waiting for next millisecond");
                    til_next_millis(self.0.start_time + last_time as i64);
                    continue;
                }
                (last_time, next_seq, sequence_mask - next_seq + 1)
            } else {
                // New millisecond — sequence resets to 0.
                (elapsed_time, 0, sequence_mask + 1)
            };

            if next_time > time_max {
                #[cfg(feature = "tracing")]
                tracing::error!(time = next_time, max = time_max, "time limit exceeded");
                return Err(Error::OverTimeLimit);
            }

            let reserved = (remaining as u64).min(avail);
            let new_state = (next_time << time_shift) | (next_seq + reserved - 1);
            if self.cas(current_state, new_state) {
                for seq in next_seq..next_seq + reserved {
                    ids.push(assemble(next_time, seq));
                }
                #[cfg(feature = "metrics")]
                {
                    metrics::counter!("snowflake_ids_generated_total").increment(reserved);
                    metrics::gauge!("snowflake_sequence_utilization")
                        .set((next_seq + reserved - 1) as f64 / sequence_mask as f64);
                }
                #[cfg(feature = "tracing")]
                tracing::trace!(
                    time = next_time,
                    count = reserved,
                    "snowflake ids generated"
                );
            }
            // CAS failure means another thread raced us; retry the loop.
        }

        Ok(ids)
    }

    /// Decompose a Snowflake ID into its constituent parts using the generator's configuration.
    #[must_use]
    pub fn decompose(&self, id: SnowflakeId) -> DecomposedSnowflake {
        DecomposedSnowflake::decompose(
            id.as_u64(),
            self.0.bit_len_time,
            self.0.bit_len_sequence,
            self.0.bit_len_data_center_id,
            self.0.bit_len_machine_id,
        )
    }

    /// Compose a Snowflake ID from its components — the inverse of [`decompose`](Self::decompose).
    ///
    /// Uses this generator's bit-length configuration. Each component must fit
    /// within its allotted bit width.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ComponentOutOfRange`] if any component exceeds its
    /// configured bit width.
    pub fn compose(
        &self,
        time: u64,
        data_center_id: u64,
        machine_id: u64,
        sequence: u64,
    ) -> Result<SnowflakeId, Error> {
        let bit_len_sequence = self.0.bit_len_sequence;
        let bit_len_machine_id = self.0.bit_len_machine_id;
        let bit_len_data_center_id = self.0.bit_len_data_center_id;
        let bit_len_time = self.0.bit_len_time;

        let sequence_max = (1u64 << bit_len_sequence) - 1;
        let machine_max = (1u64 << bit_len_machine_id) - 1;
        let data_center_max = (1u64 << bit_len_data_center_id) - 1;
        let time_max = (1u64 << bit_len_time) - 1;

        if time > time_max
            || data_center_id > data_center_max
            || machine_id > machine_max
            || sequence > sequence_max
        {
            return Err(Error::ComponentOutOfRange {
                time,
                data_center_id,
                machine_id,
                sequence,
            });
        }

        let machine_shift = bit_len_sequence;
        let data_center_shift = bit_len_machine_id + machine_shift;
        let time_shift = bit_len_data_center_id + data_center_shift;

        let id = (time << time_shift)
            | (data_center_id << data_center_shift)
            | (machine_id << machine_shift)
            | sequence;
        Ok(SnowflakeId::new(id))
    }
}

impl Default for Snowflake {
    fn default() -> Self {
        // Zero machine / data center IDs avoid any IP-fallback dependency, so
        // finalization cannot fail. Production deployments should still set
        // explicit IDs via the builder for uniqueness across hosts.
        Builder::new()
            .machine_id(&|| Ok(0))
            .data_center_id(&|| Ok(0))
            .finalize()
            .expect("default Snowflake config (zero ids) cannot fail")
    }
}

impl Clone for Snowflake {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Nanoseconds per millisecond, used to convert elapsed milliseconds to nanoseconds.
const NANOS_PER_MILLI: i64 = 1_000_000;

fn current_elapsed_time(start_time: i64) -> i64 {
    time::current_millis() - start_time
}

fn til_next_millis(last_timestamp: i64) {
    let mut now = time::current_millis();
    while now <= last_timestamp {
        core::hint::spin_loop();
        now = time::current_millis();
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
    #[must_use]
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

        // Compute the bit-shift offset for each ID section.
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
        (self.time as i64) * NANOS_PER_MILLI
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

    /// Returns the absolute Unix-millisecond timestamp of this ID.
    ///
    /// This is `start_time + elapsed_millis`, i.e. the wall-clock time at which
    /// the ID was generated, given the generator's configured `start_time`.
    #[must_use]
    pub fn absolute_millis(&self, start_time: i64) -> i64 {
        start_time + (self.time as i64)
    }
}

impl core::fmt::Display for DecomposedSnowflake {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
