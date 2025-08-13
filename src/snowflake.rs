use crate::builder::Builder;
use crate::error::*;
use base64::Engine;
use base64::engine::general_purpose;
use chrono::prelude::*;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

/// Internals of Snowflake.
#[derive(Debug)]
pub(crate) struct Internals {
    pub(crate) elapsed_time: i64,
    pub(crate) sequence: u16,
}

/// SharedSnowflake is shared between Snowflake instances.
pub(crate) struct SharedSnowflake {
    pub(crate) start_time: i64,
    pub(crate) data_center_id: u16,
    pub(crate) machine_id: u16,
    pub(crate) internals: Mutex<Internals>,
    pub(crate) bit_len_time: u8,
    pub(crate) bit_len_sequence: u8,
    pub(crate) bit_len_data_center_id: u8,
    pub(crate) bit_len_machine_id: u8,
}

/// Snowflake is a distributed unique ID generator.
pub struct Snowflake(pub(crate) Arc<SharedSnowflake>);

impl Snowflake {
    /// Create a new Snowflake with the default configuration.
    pub fn new() -> Result<Self, Error> {
        Builder::new().finalize()
    }

    /// Create a new [`Builder`] to construct a Snowflake.
    pub fn builder<'a>() -> Builder<'a> {
        Builder::new()
    }

    /// Create a new Snowflake with the given SharedSnowflake.
    pub(crate) fn new_inner(shared: Arc<SharedSnowflake>) -> Self {
        Self(shared)
    }

    /// Generate the next unique id.
    pub fn next_id(&self) -> Result<u64, Error> {
        let mut internals = self.0.internals.lock().map_err(|_| Error::MutexPoisoned)?;
        let sequence_mask = (1 << self.0.bit_len_sequence) - 1;

        let current = current_elapsed_time(self.0.start_time);
        if internals.elapsed_time < current {
            internals.elapsed_time = current;
            internals.sequence = 0;
        } else {
            internals.sequence = (internals.sequence + 1) & sequence_mask;
            if internals.sequence == 0 {
                internals.elapsed_time += 1;
                let overtime = internals.elapsed_time - current;
                thread::sleep(sleep_time(overtime));
            }
        }

        if internals.elapsed_time >= (1 << self.0.bit_len_time) {
            return Err(Error::OverTimeLimit);
        }

        let time_shift =
            self.0.bit_len_sequence + self.0.bit_len_data_center_id + self.0.bit_len_machine_id;
        let sequence_shift = self.0.bit_len_data_center_id + self.0.bit_len_machine_id;
        let data_center_shift = self.0.bit_len_machine_id;

        Ok(((internals.elapsed_time as u64) << time_shift)
            | ((internals.sequence as u64) << sequence_shift)
            | ((self.0.data_center_id as u64) << data_center_shift)
            | (self.0.machine_id as u64))
    }
}

impl Clone for Snowflake {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

const SNOWFLAKE_TIME_UNIT: i64 = 1_000_000; // nanoseconds, i.e. 1 msec

pub(crate) fn to_snowflake_time(time: DateTime<Utc>) -> i64 {
    time.timestamp_nanos_opt().unwrap_or(0) / SNOWFLAKE_TIME_UNIT
}

fn current_elapsed_time(start_time: i64) -> i64 {
    to_snowflake_time(Utc::now()) - start_time
}

fn sleep_time(overtime: i64) -> Duration {
    let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    Duration::from_millis(overtime as u64)
        - Duration::from_nanos((now_ns % SNOWFLAKE_TIME_UNIT) as u64)
}

/// DecomposedSnowflake is the parts of a Snowflake ID.
/// DecomposedSnowflake contains the components of a Snowflake ID.
///
/// It is created by calling the 'DecomposedSnowflake::d ecompose' method and providing the bit length configuration used when generating the ID.
pub struct DecomposedSnowflake {
    /// Original u64 format ID
    pub id: u64,
    /// Timestamped portion from epoch (in milliseconds)
    pub time: u64,
    /// Serial number section
    pub sequence: u64,
    /// Data Center ID section
    pub data_center_id: u64,
    /// Machine ID section
    pub machine_id: u64,
}

impl DecomposedSnowflake {
    /// Break a Snowflake ID up into its parts based on the provided bit lengths.
    pub fn decompose(
        id: u64,
        bit_len_time: u8,
        bit_len_sequence: u8,
        bit_len_data_center_id: u8,
        bit_len_machine_id: u8,
    ) -> Self {
        assert_eq!(
            bit_len_time as u32
                + bit_len_sequence as u32
                + bit_len_data_center_id as u32
                + bit_len_machine_id as u32,
            63,
            "Total bit length must be 63"
        );
        let machine_id_mask = (1 << bit_len_machine_id) - 1;
        let data_center_id_mask = ((1 << bit_len_data_center_id) - 1) << bit_len_machine_id;
        let sequence_mask =
            ((1 << bit_len_sequence) - 1) << (bit_len_data_center_id + bit_len_machine_id);

        let time_shift = bit_len_sequence + bit_len_data_center_id + bit_len_machine_id;
        let sequence_shift = bit_len_data_center_id + bit_len_machine_id;
        let data_center_shift = bit_len_machine_id;

        Self {
            id,
            time: id >> time_shift,
            sequence: (id & sequence_mask) >> sequence_shift,
            data_center_id: (id & data_center_id_mask) >> data_center_shift,
            machine_id: id & machine_id_mask,
        }
    }

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

/// bit length of time
pub(crate) const BIT_LEN_TIME: u64 = 39;
/// bit length of sequence number
pub(crate) const BIT_LEN_SEQUENCE: u64 = 8;
/// bit length of the data center id
const BIT_LEN_DATA_CENTER_ID: u64 = 8;
/// bit length of machine id
const BIT_LEN_MACHINE_ID: u64 = 63 - BIT_LEN_TIME - BIT_LEN_SEQUENCE - BIT_LEN_DATA_CENTER_ID;

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
        time: id >> (BIT_LEN_SEQUENCE + BIT_LEN_MACHINE_ID + BIT_LEN_DATA_CENTER_ID),
        sequence: (id & DECOMPOSE_MASK_SEQUENCE) >> (BIT_LEN_MACHINE_ID + BIT_LEN_DATA_CENTER_ID),
        data_center_id: (id & MASK_DATA_CENTER_ID) >> BIT_LEN_MACHINE_ID,
        machine_id: id & MASK_MACHINE_ID,
    }
}
