// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use chrono::{DateTime, Utc};
use std::error::Error as StdError;
use thiserror::Error;

/// Convenience type alias for usage within Snowflake.
pub(crate) type BoxDynError = Box<dyn StdError + 'static + Send + Sync>;

/// The error type for this crate.
#[derive(Error, Debug)]
pub enum Error {
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
    #[error("could not find any private ipv4 address")]
    NoPrivateIPv4,
    #[error("mutex is poisoned (i.e. a panic happened while it was locked)")]
    MutexPoisoned,
    #[error(
        "invalid bit length configuration: time({0}) + sequence({1}) + data_center({2}) + machine({3}) must be 63"
    )]
    InvalidBitLength(u8, u8, u8, u8),
}
