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
use crate::sync::{Arc, AtomicU64, Ordering};
use crate::time;

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Shared state and configuration for the [`Snowflake`] generator.
///
/// Designed to be lock-free for high concurrency performance.
/// Aligned to 64 bytes to reduce sharing with adjacent allocations on targets
/// with 64-byte cache lines; threads still contend on the shared atomic state.
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
    /// Optional per-generator clock, shared by clones.
    pub(crate) clock_source: Option<alloc::sync::Arc<dyn Fn() -> i64 + Send + Sync>>,
    /// Maximum elapsed duration after a generation call first needs to wait.
    #[cfg(feature = "std")]
    pub(crate) wait_timeout: Option<core::time::Duration>,
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

    /// Atomically update the packed state, retaining a racing writer's state
    /// on failure so the retry does not need another atomic load.
    fn cas(&self, current: u64, new: u64) -> Result<u64, u64> {
        if cfg!(feature = "use-strong-cas") {
            self.0
                .state
                .compare_exchange(current, new, Ordering::AcqRel, Ordering::Relaxed)
        } else {
            self.0
                .state
                .compare_exchange_weak(current, new, Ordering::AcqRel, Ordering::Relaxed)
        }
    }

    /// Generate the next unique ID.
    ///
    /// State updates use lock-free CAS operations. The call can wait for the
    /// clock when sequences are exhausted or the drift strategy requires it.
    /// Clones share the same state and clock.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OverTimeLimit`] if the elapsed timestamp does not fit
    /// its configured field. Backward clocks return [`Error::ClockDrift`],
    /// [`Error::BeforeEpoch`], or [`Error::ClockDriftExceeded`] according to the
    /// configured strategy. With `std`, a configured wait timeout returns
    /// `Error::WaitTimeout` if the clock does not recover within the budget.
    pub fn next_id(&self) -> Result<SnowflakeId, Error> {
        let reservation = self.reserve(1, &mut WaitBudget::default())?;
        Ok(SnowflakeId::new(
            self.id_prefix(reservation.time) | reservation.first_sequence,
        ))
    }

    /// Generate multiple unique IDs in a single call.
    ///
    /// Reserves contiguous sequences with one CAS per chunk. Returned IDs are
    /// monotonically increasing, but another thread can reserve between chunks.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::next_id`]. Previously reserved IDs are
    /// consumed and discarded if a later chunk fails.
    pub fn next_ids(&self, count: usize) -> Result<Vec<SnowflakeId>, Error> {
        let mut ids = Vec::with_capacity(count);
        self.append_ids(count, &mut ids)?;
        Ok(ids)
    }

    fn append_ids(&self, count: usize, output: &mut Vec<SnowflakeId>) -> Result<(), Error> {
        // Reserve before claiming sequences, so allocation cannot fail between
        // claiming a chunk and appending its IDs.
        output.reserve(count);
        let mut remaining = count;
        let mut wait = WaitBudget::default();
        while remaining != 0 {
            let reservation = self.reserve(remaining, &mut wait)?;
            let prefix = self.id_prefix(reservation.time);
            for sequence in
                reservation.first_sequence..reservation.first_sequence + reservation.count
            {
                output.push(SnowflakeId::new(prefix | sequence));
            }
            remaining -= reservation.count as usize;
        }
        Ok(())
    }

    fn id_prefix(&self, time: u64) -> u64 {
        let data_center_shift = self.0.bit_len_machine_id + self.0.bit_len_sequence;
        (time << (self.0.bit_len_data_center_id + data_center_shift))
            | (u64::from(self.0.data_center_id) << data_center_shift)
            | (u64::from(self.0.machine_id) << self.0.bit_len_sequence)
    }

    /// Shared reservation path keeps clock checks and observability identical
    /// for single IDs, allocating batches, and reusable buffers.
    fn reserve(&self, count: usize, wait: &mut WaitBudget) -> Result<Reservation, Error> {
        let sequence_mask = (1u64 << self.0.bit_len_sequence) - 1;
        let time_max = (1u64 << self.0.bit_len_time) - 1;
        let mut current_state = self.0.state.load(Ordering::Relaxed);
        let mut observed_drift = false;
        let mut observed_exhaustion = false;

        loop {
            wait.check_timeout(&self.0)?;
            // No valid packed state uses bit 63. The sentinel lets the first
            // reservation use timestamp zero and sequence zero, even when the
            // sequence field has zero bits.
            let initialized = current_state != u64::MAX;
            let last_time = if initialized {
                current_state >> self.0.bit_len_sequence
            } else {
                0
            };
            // Once the last representable sequence is consumed, no clock
            // recovery can make another ID valid, including after rollback.
            if initialized
                && last_time == time_max
                && current_state & sequence_mask == sequence_mask
            {
                #[cfg(feature = "metrics")]
                metrics::counter!("snowflake_sequence_exhaustion_total").increment(1);
                return Err(Error::OverTimeLimit);
            }
            let timestamp = self
                .0
                .clock_source
                .as_ref()
                .map_or_else(time::current_millis, |clock| clock());
            let elapsed = i128::from(timestamp) - i128::from(self.0.start_time);
            if elapsed > i128::from(time_max) || last_time > time_max {
                return Err(Error::OverTimeLimit);
            }

            let next_time = if elapsed < i128::from(last_time) {
                if !observed_drift {
                    #[cfg(feature = "metrics")]
                    metrics::counter!("snowflake_clock_drift_events_total").increment(1);
                    #[cfg(feature = "tracing")]
                    tracing::warn!(last_time, timestamp, strategy = ?self.0.clock_drift_strategy, "clock drift detected");
                    observed_drift = true;
                }
                match self.0.clock_drift_strategy {
                    ClockDriftStrategy::Wait => {
                        if let Some(max_ms) = self.0.max_clock_drift_ms {
                            let drift = i128::from(last_time) - elapsed;
                            if drift > i128::from(max_ms) {
                                return Err(Error::ClockDriftExceeded {
                                    drift_ms: drift.min(i128::from(u64::MAX)) as u64,
                                    max_ms,
                                });
                            }
                        }
                        wait.pause(&self.0)?;
                        current_state = self.0.state.load(Ordering::Relaxed);
                        continue;
                    }
                    ClockDriftStrategy::Error => {
                        if elapsed < 0 {
                            return Err(Error::BeforeEpoch {
                                timestamp,
                                start_time: self.0.start_time,
                            });
                        }
                        return Err(Error::ClockDrift {
                            last_time,
                            current_time: elapsed as u64,
                        });
                    }
                    ClockDriftStrategy::LastTimestamp => {
                        if !initialized {
                            return Err(Error::BeforeEpoch {
                                timestamp,
                                start_time: self.0.start_time,
                            });
                        }
                        last_time
                    }
                }
            } else {
                elapsed as u64
            };

            let first_sequence = if initialized && next_time == last_time {
                (current_state & sequence_mask) + 1
            } else {
                0
            };
            if first_sequence > sequence_mask {
                if !observed_exhaustion {
                    #[cfg(feature = "metrics")]
                    metrics::counter!("snowflake_sequence_exhaustion_total").increment(1);
                    #[cfg(feature = "tracing")]
                    tracing::debug!("sequence exhausted, waiting for next millisecond");
                    observed_exhaustion = true;
                }
                wait.pause(&self.0)?;
                current_state = self.0.state.load(Ordering::Relaxed);
                continue;
            }
            let reserved = (count as u64).min(sequence_mask - first_sequence + 1);
            let final_sequence = first_sequence + reserved - 1;
            let new_state = (next_time << self.0.bit_len_sequence) | final_sequence;
            match self.cas(current_state, new_state) {
                Ok(_) => {
                    #[cfg(feature = "metrics")]
                    {
                        metrics::counter!("snowflake_ids_generated_total").increment(reserved);
                        let utilization = if sequence_mask == 0 {
                            1.0
                        } else {
                            final_sequence as f64 / sequence_mask as f64
                        };
                        metrics::gauge!("snowflake_sequence_utilization").set(utilization);
                    }
                    #[cfg(feature = "tracing")]
                    tracing::trace!(
                        time = next_time,
                        count = reserved,
                        "snowflake ids generated"
                    );
                    return Ok(Reservation {
                        time: next_time,
                        first_sequence,
                        count: reserved,
                    });
                }
                Err(actual) => current_state = actual,
            }
        }
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
    /// Returns [`Error::ParseIdFailed`] if any component exceeds its configured
    /// bit width.
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
            return Err(Error::ParseIdFailed(format!(
                "component out of range for compose: time={time}, data_center_id={data_center_id}, machine_id={machine_id}, sequence={sequence}"
            )));
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

struct Reservation {
    time: u64,
    first_sequence: u64,
    count: u64,
}

#[derive(Default)]
struct WaitBudget {
    spins: u8,
    #[cfg(feature = "std")]
    started: Option<std::time::Instant>,
}

impl WaitBudget {
    fn check_timeout(&self, _shared: &SharedSnowflake) -> Result<(), Error> {
        #[cfg(feature = "std")]
        if let (Some(started), Some(timeout)) = (self.started, _shared.wait_timeout)
            && started.elapsed() >= timeout
        {
            return Err(Error::WaitTimeout);
        }
        Ok(())
    }

    fn pause(&mut self, shared: &SharedSnowflake) -> Result<(), Error> {
        #[cfg(feature = "std")]
        if shared.wait_timeout.is_some() && self.started.is_none() {
            self.started = Some(std::time::Instant::now());
        }
        self.check_timeout(shared)?;
        if self.spins < 64 {
            self.spins += 1;
            core::hint::spin_loop();
        } else {
            #[cfg(loom)]
            loom::thread::yield_now();
            #[cfg(all(feature = "std", not(loom)))]
            std::thread::yield_now();
            #[cfg(all(not(feature = "std"), not(loom)))]
            core::hint::spin_loop();
        }
        Ok(())
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
