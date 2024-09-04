use crate::builder::Builder;
use crate::error::*;
use base64::engine::general_purpose;
use base64::Engine;
use chrono::prelude::*;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

/// bit length of time
pub(crate) const BIT_LEN_TIME: u64 = 39;
/// bit length of sequence number
pub(crate) const BIT_LEN_SEQUENCE: u64 = 8;
/// bit length of the data center id
pub(crate) const BIT_LEN_DATA_CENTER_ID: u64 = 8;
/// bit length of machine id
pub(crate) const BIT_LEN_MACHINE_ID: u64 =
    63 - BIT_LEN_TIME - BIT_LEN_SEQUENCE - BIT_LEN_DATA_CENTER_ID;
/// mask for sequence number
pub(crate) const GENERATE_MASK_SEQUENCE: u16 = (1 << BIT_LEN_SEQUENCE) - 1;

/// Internals of Snowflake.
/// This struct is not exposed to the public.
#[derive(Debug)]
pub(crate) struct Internals {
    pub(crate) elapsed_time: i64,
    pub(crate) sequence: u16,
}

/// SharedSnowflake is shared between Snowflake instances.
/// This struct is not exposed to the public.
pub(crate) struct SharedSnowflake {
    pub(crate) start_time: i64,
    pub(crate) data_center_id: u8,
    pub(crate) machine_id: u8,
    pub(crate) internals: Mutex<Internals>,
}

/// Snowflake is a distributed unique ID generator.
/// It is thread-safe and can be cloned to be used in multiple threads.
pub struct Snowflake(pub(crate) Arc<SharedSnowflake>);

impl Snowflake {
    /// Create a new Snowflake with the default configuration.
    /// For custom configuration see [`builder`].
    ///
    /// [`builder`]: struct.Snowflake.html#method.builder
    pub fn new() -> Result<Self, Error> {
        Builder::new().finalize()
    }

    /// Create a new [`Builder`] to construct a Snowflake.
    ///
    /// [`Builder`]: struct.Builder.html
    pub fn builder<'a>() -> Builder<'a> {
        Builder::new()
    }

    /// Create a new Snowflake with the given SharedSnowflake.
    /// This is used for testing.
    pub(crate) fn new_inner(shared: Arc<SharedSnowflake>) -> Self {
        Self(shared)
    }

    /// Generate the next unique id.
    /// After the Snowflake time overflows, next_id returns an error.
    pub fn next_id(&self) -> Result<u64, Error> {
        let mut internals = self.0.internals.lock().map_err(|_| Error::MutexPoisoned)?;

        let current = current_elapsed_time(self.0.start_time);
        if internals.elapsed_time < current {
            internals.elapsed_time = current;
            internals.sequence = 0;
        } else {
            // self.elapsed_time >= current
            internals.sequence = (internals.sequence + 1) & GENERATE_MASK_SEQUENCE;
            if internals.sequence == 0 {
                internals.elapsed_time += 1;
                let overtime = internals.elapsed_time - current;
                thread::sleep(sleep_time(overtime));
            }
        }

        if internals.elapsed_time >= 1 << BIT_LEN_TIME {
            return Err(Error::OverTimeLimit);
        }

        Ok((internals.elapsed_time as u64)
            << (BIT_LEN_SEQUENCE + BIT_LEN_MACHINE_ID + BIT_LEN_DATA_CENTER_ID)
            | (internals.sequence as u64) << (BIT_LEN_MACHINE_ID + BIT_LEN_DATA_CENTER_ID)
            | (self.0.data_center_id as u64) << BIT_LEN_MACHINE_ID
            | (self.0.machine_id as u64))
    }
}

/// Returns a new `Snowflake` referencing the same state as `self`.
/// This is used for concurrent use.
impl Clone for Snowflake {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Convert a `DateTime<Utc>` to a Snowflake time.
const SNOWFLAKE_TIME_UNIT: i64 = 10_000_000; // nanoseconds, i.e. 10 msec

/// Convert a `DateTime<Utc>` to a Snowflake time.
/// The time is the number of 10 msec since the start time.
/// The start time is 2014-09-01T00:00:00Z.
pub(crate) fn to_snowflake_time(time: DateTime<Utc>) -> i64 {
    time.timestamp_nanos_opt().unwrap() / SNOWFLAKE_TIME_UNIT
}
/// Returns the current elapsed time in 10 msec units.
fn current_elapsed_time(start_time: i64) -> i64 {
    to_snowflake_time(Utc::now()) - start_time
}
/// Returns the sleep time in nanoseconds.
fn sleep_time(overtime: i64) -> Duration {
    Duration::from_millis(overtime as u64 * 10)
        - Duration::from_nanos(
            (Utc::now().timestamp_nanos_opt().unwrap() % SNOWFLAKE_TIME_UNIT) as u64,
        )
}
/// DecomposedSnowflake is the parts of a Snowflake ID.
pub struct DecomposedSnowflake {
    pub id: u64,
    pub msb: u64,
    pub time: u64,
    pub sequence: u64,
    pub data_center_id: u64,
    pub machine_id: u64,
}

impl DecomposedSnowflake {
    /// Returns the timestamp in nanoseconds without an epoch.
    pub fn nanos_time(&self) -> i64 {
        (self.time as i64) * SNOWFLAKE_TIME_UNIT
    }

    /// Returns the timestamp in milliseconds since the epoch.
    pub fn int64(&self) -> i64 {
        self.id as i64
    }

    /// Returns the string representation of the Snowflake ID.
    pub fn string(&self) -> String {
        self.id.to_string()
    }

    /// Returns the base2 encoded string.
    pub fn base2(&self) -> String {
        format!("{:b}", self.id)
    }

    /// Returns the base32 encoded string.
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

    /// Returns the base36 encoded string.
    pub fn base36(&self) -> String {
        format!("{:x}", self.id)
    }

    /// Returns the base58 encoded string.
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

    /// Returns the base64 encoded string.
    pub fn base64(&self) -> String {
        general_purpose::STANDARD.encode(self.id.to_be_bytes())
    }
    /// Returns the bytes of the Snowflake ID.
    pub fn bytes(&self) -> Vec<u8> {
        self.id.to_string().into_bytes()
    }
    /// Returns the bytes of the Snowflake ID.
    pub fn int_bytes(&self) -> [u8; 8] {
        self.id.to_be_bytes()
    }
    /// Returns the timestamp in seconds since the epoch.
    pub fn time(&self) -> i64 {
        self.time as i64
    }
}

/// The mask to decompose Snowflake ID.
const DECOMPOSE_MASK_SEQUENCE: u64 =
    ((1 << BIT_LEN_SEQUENCE) - 1) << (BIT_LEN_DATA_CENTER_ID + BIT_LEN_MACHINE_ID);
/// The mask for data center ID.
const MASK_DATA_CENTER_ID: u64 = ((1 << BIT_LEN_DATA_CENTER_ID) - 1) << BIT_LEN_MACHINE_ID;
/// The mask for machine ID.
const MASK_MACHINE_ID: u64 = (1 << BIT_LEN_MACHINE_ID) - 1;

/// Break a Snowflake ID up into its parts.
pub fn decompose(id: u64) -> DecomposedSnowflake {
    DecomposedSnowflake {
        id,
        msb: id >> 63,
        time: id >> (BIT_LEN_SEQUENCE + BIT_LEN_MACHINE_ID + BIT_LEN_DATA_CENTER_ID),
        sequence: (id & DECOMPOSE_MASK_SEQUENCE) >> (BIT_LEN_MACHINE_ID + BIT_LEN_DATA_CENTER_ID),
        data_center_id: (id & MASK_DATA_CENTER_ID) >> BIT_LEN_MACHINE_ID,
        machine_id: id & MASK_MACHINE_ID,
    }
}
