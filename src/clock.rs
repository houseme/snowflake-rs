// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/// Strategy for handling backward clock drift (e.g., due to NTP adjustments).
///
/// When the system clock moves backward, the generator must decide how to
/// maintain ID uniqueness. Each strategy offers a different trade-off
/// between monotonicity guarantees and availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClockDriftStrategy {
    /// Busy-wait until the clock catches up. Guarantees strict monotonicity.
    ///
    /// This is the default strategy. If `max_clock_drift_ms` is configured and
    /// the drift exceeds that limit, [`Error::ClockDriftExceeded`] is returned
    /// instead of waiting indefinitely.
    #[default]
    Wait,
    /// Return [`Error::ClockDrift`] immediately on backward clock drift.
    ///
    /// Use this when the caller prefers to handle clock issues explicitly
    /// rather than blocking.
    Error,
    /// Reuse the last known timestamp when the clock moves backward.
    ///
    /// IDs remain globally unique (the sequence number still advances),
    /// but the time-to-ID mapping becomes approximate. Useful when
    /// availability is more important than exact timestamp accuracy.
    LastTimestamp,
}
