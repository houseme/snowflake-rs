use crate::Snowflake;
use crate::error::{BoxDynError, Error};
use crate::snowflake::{Internals, SharedSnowflake, to_snowflake_time};

use chrono::prelude::*;
use std::sync::{Arc, Mutex};

#[cfg(feature = "ip-fallback")]
use std::net::{IpAddr, Ipv4Addr};

/// A builder to build a [`Snowflake`] generator.
///
/// [`Snowflake`]: struct.Snowflake.html
pub struct Builder<'a> {
    start_time: Option<DateTime<Utc>>,
    machine_id: Option<&'a dyn Fn() -> Result<u16, BoxDynError>>,
    data_center_id: Option<&'a dyn Fn() -> Result<u16, BoxDynError>>,
    check_machine_id: Option<&'a dyn Fn(u16) -> bool>,
    check_data_center_id: Option<&'a dyn Fn(u16) -> bool>,
    bit_len_time: u8,
    bit_len_sequence: u8,
    bit_len_data_center_id: u8,
    bit_len_machine_id: u8,
}

impl<'a> Default for Builder<'a> {
    fn default() -> Self {
        Builder::new()
    }
}

impl<'a> Builder<'a> {
    /// Construct a new builder to call methods on for the [`Snowflake`] construction.
    ///
    /// [`Snowflake`]: struct.Snowflake.html
    pub fn new() -> Self {
        #[cfg(not(feature = "ip-fallback"))]
        {
            Self {
                start_time: None,
                machine_id: Some(&|| Ok(0)),
                data_center_id: Some(&|| Ok(0)),
                check_machine_id: None,
                check_data_center_id: None,
                bit_len_time: 41,
                bit_len_sequence: 12,
                bit_len_data_center_id: 5,
                bit_len_machine_id: 5,
            }
        }

        #[cfg(feature = "ip-fallback")]
        {
            Self {
                start_time: None,
                machine_id: None,
                data_center_id: None,
                check_machine_id: None,
                check_data_center_id: None,
                bit_len_time: 41,
                bit_len_sequence: 12,
                bit_len_data_center_id: 5,
                bit_len_machine_id: 5,
            }
        }
    }

    /// Sets the start time.
    /// If the time is ahead of the current time, finalize will fail.
    pub fn start_time(mut self, start_time: DateTime<Utc>) -> Self {
        self.start_time = Some(start_time);
        self
    }

    /// Sets the machine id.
    /// If the fn returns an error, finalize will fail.
    pub fn machine_id(mut self, machine_id: &'a dyn Fn() -> Result<u16, BoxDynError>) -> Self {
        self.machine_id = Some(machine_id);
        self
    }

    /// Sets the data center id.
    /// If the fn returns an error, finalize will fail.
    pub fn data_center_id(
        mut self,
        data_center_id: &'a dyn Fn() -> Result<u16, BoxDynError>,
    ) -> Self {
        self.data_center_id = Some(data_center_id);
        self
    }

    /// Set a function to check the machine id.
    /// If the fn returns false, finalize will fail.
    pub fn check_machine_id(mut self, check_machine_id: &'a dyn Fn(u16) -> bool) -> Self {
        self.check_machine_id = Some(check_machine_id);
        self
    }

    /// Set a function to check the data center id.
    /// If the fn returns false, finalize will fail.
    pub fn check_data_center_id(mut self, check_data_center_id: &'a dyn Fn(u16) -> bool) -> Self {
        self.check_data_center_id = Some(check_data_center_id);
        self
    }

    /// Sets the bit length for the time part.
    pub fn bit_len_time(mut self, bit_len_time: u8) -> Self {
        self.bit_len_time = bit_len_time;
        self
    }

    /// Sets the bit length for the sequence part.
    pub fn bit_len_sequence(mut self, bit_len_sequence: u8) -> Self {
        self.bit_len_sequence = bit_len_sequence;
        self
    }

    /// Sets the bit length for the data center ID part.
    pub fn bit_len_data_center_id(mut self, bit_len_data_center_id: u8) -> Self {
        self.bit_len_data_center_id = bit_len_data_center_id;
        self
    }

    /// Sets the bit length for the machine ID part.
    pub fn bit_len_machine_id(mut self, bit_len_machine_id: u8) -> Self {
        self.bit_len_machine_id = bit_len_machine_id;
        self
    }

    /// Finalize the builder to create a Snowflake.
    /// If any of the functions return an error, finalize will fail.
    pub fn finalize(self) -> Result<Snowflake, Error> {
        if self.bit_len_time
            + self.bit_len_sequence
            + self.bit_len_data_center_id
            + self.bit_len_machine_id
            != 63
        {
            return Err(Error::InvalidBitLength(
                self.bit_len_time,
                self.bit_len_sequence,
                self.bit_len_data_center_id,
                self.bit_len_machine_id,
            ));
        }

        let start_time = if let Some(start_time) = self.start_time {
            if start_time > Utc::now() {
                return Err(Error::StartTimeAheadOfCurrentTime(start_time));
            }
            to_snowflake_time(start_time)
        } else {
            to_snowflake_time(Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap())
        };

        let machine_id = if let Some(machine_id_fn) = self.machine_id {
            machine_id_fn().map_err(Error::MachineIdFailed)?
        } else {
            #[cfg(feature = "ip-fallback")]
            {
                lower_16_bit_private_ip()?
            }
            #[cfg(not(feature = "ip-fallback"))]
            {
                return Err(Error::MachineIdFailed(
                    "Machine ID not provided and IP fallback feature is disabled".into(),
                ));
            }
        };

        if let Some(check_machine_id) = self.check_machine_id
            && !check_machine_id(machine_id)
        {
            return Err(Error::CheckMachineIdFailed);
        }

        let data_center_id = if let Some(data_center_id_fn) = self.data_center_id {
            data_center_id_fn().map_err(Error::DataCenterIdFailed)?
        } else {
            #[cfg(feature = "ip-fallback")]
            {
                lower_8_bit_private_ip()?.into() // Default to 0 if not provided and ip-fallback is enabled
            }
            #[cfg(not(feature = "ip-fallback"))]
            {
                return Err(Error::DataCenterIdFailed(
                    "Data Center ID not provided and IP fallback feature is disabled".into(),
                ));
            }
        };

        if let Some(check_data_center_id) = self.check_data_center_id
            && !check_data_center_id(data_center_id)
        {
            return Err(Error::CheckDataCenterIdFailed);
        }

        let shared = Arc::new(SharedSnowflake {
            internals: Mutex::new(Internals {
                sequence: 0,
                elapsed_time: 0,
            }),
            start_time,
            machine_id,
            data_center_id,
            bit_len_time: self.bit_len_time,
            bit_len_sequence: self.bit_len_sequence,
            bit_len_data_center_id: self.bit_len_data_center_id,
            bit_len_machine_id: self.bit_len_machine_id,
        });
        Ok(Snowflake::new_inner(shared))
    }
}

#[cfg(feature = "ip-fallback")]
fn private_ipv4() -> Option<Ipv4Addr> {
    pnet_datalink::interfaces()
        .iter()
        .filter(|interface| interface.is_up() && !interface.is_loopback())
        .flat_map(|interface| interface.ips.iter())
        .filter_map(|network| match network.ip() {
            IpAddr::V4(ipv4) => Some(ipv4),
            IpAddr::V6(_) => None,
        })
        .find(is_private_ipv4)
}

#[cfg(feature = "ip-fallback")]
fn is_private_ipv4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    matches!(octets[0], 10)
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}

#[cfg(feature = "ip-fallback")]
pub(crate) fn lower_16_bit_private_ip() -> Result<u16, Error> {
    private_ipv4()
        .map(|ip| {
            let octets = ip.octets();
            ((octets[2] as u16) << 8) + (octets[3] as u16)
        })
        .ok_or(Error::NoPrivateIPv4)
}

#[cfg(feature = "ip-fallback")]
#[allow(dead_code)]
pub(crate) fn lower_8_bit_private_ip() -> Result<u8, Error> {
    match private_ipv4() {
        Some(ip) => {
            let octets = ip.octets();
            Ok(octets[3])
        }
        None => Err(Error::NoPrivateIPv4),
    }
}
