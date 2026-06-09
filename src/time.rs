// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Time source abstraction supporting both `std` and `no_std` environments.

#[cfg(feature = "std")]
use chrono::prelude::*;

/// Microseconds per millisecond, used to convert nanoseconds to milliseconds.
#[cfg(feature = "std")]
const MICROS_PER_MILLI: i64 = 1_000_000;

/// Get current time in milliseconds since Unix epoch (std version).
#[cfg(feature = "std")]
pub(crate) fn current_millis() -> i64 {
    Utc::now().timestamp_nanos_opt().unwrap_or(0) / MICROS_PER_MILLI
}

// --- no_std time source ---

#[cfg(not(feature = "std"))]
use core::sync::atomic::{AtomicI64, Ordering};

/// Global time source for no_std environments.
///
/// Must be set via [`set_time_source`] before generating IDs.
#[cfg(not(feature = "std"))]
static NO_STD_TIME_SOURCE: AtomicI64 = AtomicI64::new(0);

/// Get current time in milliseconds (no_std version).
///
/// Returns the value last set via [`set_time_source`].
#[cfg(not(feature = "std"))]
pub(crate) fn current_millis() -> i64 {
    NO_STD_TIME_SOURCE.load(Ordering::Relaxed)
}

/// Set the time source for no_std environments.
///
/// Must be called periodically (e.g., from a timer interrupt or RTC read)
/// with the current time in milliseconds since Unix epoch.
///
/// # Example
///
/// ```rust
/// // In a timer interrupt handler or main loop:
/// snowflake_me::set_time_source(get_rtc_millis());
/// ```
#[cfg(not(feature = "std"))]
pub fn set_time_source(millis: i64) {
    NO_STD_TIME_SOURCE.store(millis, Ordering::Relaxed);
}
