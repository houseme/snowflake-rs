// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[cfg(feature = "std")]
use chrono::{DateTime, Utc};
use thiserror::Error;

extern crate alloc;
use alloc::boxed::Box;
use alloc::string::String;

/// Convenience type alias for usage within Snowflake.
pub(crate) type BoxDynError = Box<dyn core::error::Error + 'static + Send + Sync>;

/// The error type for this crate.
#[derive(Error, Debug)]
pub enum Error {
    /// The configured start time is in the future relative to the system clock.
    ///
    /// Only raised when the `std` feature is enabled and a `DateTime<Utc>` start time
    /// is provided to [`Builder::start_time`](crate::Builder::start_time).
    #[cfg(feature = "std")]
    #[error("start_time `{0}` is ahead of current time")]
    StartTimeAheadOfCurrentTime(DateTime<Utc>),

    /// The user-provided `machine_id` closure returned an error.
    #[error("machine_id returned an error: {0}")]
    MachineIdFailed(#[source] BoxDynError),

    /// The user-provided `data_center_id` closure returned an error.
    #[error("data_center_id returned an error: {0}")]
    DataCenterIdFailed(#[source] BoxDynError),

    /// The `check_machine_id` validation function returned `false`.
    #[error("check_machine_id returned false")]
    CheckMachineIdFailed,

    /// The `check_data_center_id` validation function returned `false`.
    #[error("check_data_center_id returned false")]
    CheckDataCenterIdFailed,

    /// The elapsed time exceeds the maximum value representable by the configured time bit length.
    ///
    /// This occurs when the generator has been running for longer than the timestamp
    /// field can represent (e.g., ~69 years with the default 41-bit time field).
    #[error("over the time limit")]
    OverTimeLimit,

    /// No private IPv4 or IPv6 address was found on any network interface.
    ///
    /// Only raised when the `ip-fallback` feature is enabled and no `machine_id` or
    /// `data_center_id` is explicitly provided.
    #[error("could not find any private IPv4 or IPv6 address")]
    NoPrivateIP,

    /// Failed to parse a string as a [`SnowflakeId`](crate::SnowflakeId).
    #[error("failed to parse SnowflakeId: {0}")]
    ParseIdFailed(String),

    /// The system clock moved backward (clock drift detected).
    ///
    /// Raised when the [`ClockDriftStrategy`](crate::ClockDriftStrategy) is `Error`
    /// and the current time is earlier than the last recorded timestamp.
    #[error("clock drifted backward: last_time={last_time}, current_time={current_time}")]
    ClockDrift {
        /// The last recorded timestamp in the state.
        last_time: u64,
        /// The current (earlier) timestamp from the system clock.
        current_time: u64,
    },

    /// Clock drift exceeded the configured maximum allowed drift.
    ///
    /// Raised when the [`ClockDriftStrategy`](crate::ClockDriftStrategy) is `Wait`
    /// and the drift exceeds [`Builder::max_clock_drift_ms`](crate::Builder::max_clock_drift_ms).
    #[error("clock drift {drift_ms}ms exceeded maximum allowed {max_ms}ms")]
    ClockDriftExceeded {
        /// The actual drift in milliseconds.
        drift_ms: u64,
        /// The configured maximum allowed drift in milliseconds.
        max_ms: i64,
    },

    /// The sum of all bit lengths does not equal 63.
    ///
    /// The four configurable sections (time, sequence, data center ID, machine ID)
    /// must sum to exactly 63 bits to fit within a `u64` with the sign bit unset.
    #[error(
        "invalid bit length configuration: time({0}) + sequence({1}) + data_center({2}) + machine({3}) must be 63"
    )]
    InvalidBitLength(u8, u8, u8, u8),
}
