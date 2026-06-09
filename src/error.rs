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
    #[cfg(feature = "std")]
    #[error("start_time `{0}` is ahead of current time")]
    StartTimeAheadOfCurrentTime(DateTime<Utc>),
    #[error("machine_id returned an error: {0}")]
    MachineIdFailed(#[source] BoxDynError),
    #[error("data_center_id returned an error: {0}")]
    DataCenterIdFailed(#[source] BoxDynError),
    #[error("check_machine_id returned false")]
    CheckMachineIdFailed,
    #[error("check_data_center_id returned false")]
    CheckDataCenterIdFailed,
    #[error("over the time limit")]
    OverTimeLimit,
    #[error("could not find any private IPv4 or IPv6 address")]
    NoPrivateIP,
    #[error("failed to parse SnowflakeId: {0}")]
    ParseIdFailed(String),
    #[error("clock drifted backward: last_time={last_time}, current_time={current_time}")]
    ClockDrift {
        /// The last recorded timestamp in the state.
        last_time: u64,
        /// The current (earlier) timestamp from the system clock.
        current_time: u64,
    },
    #[error("clock drift {drift_ms}ms exceeded maximum allowed {max_ms}ms")]
    ClockDriftExceeded {
        /// The actual drift in milliseconds.
        drift_ms: u64,
        /// The configured maximum allowed drift in milliseconds.
        max_ms: i64,
    },
    #[error(
        "invalid bit length configuration: time({0}) + sequence({1}) + data_center({2}) + machine({3}) must be 63"
    )]
    InvalidBitLength(u8, u8, u8, u8),
}
