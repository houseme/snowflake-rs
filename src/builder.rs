use crate::error::{BoxDynError, Error};
use crate::snowflake::{to_snowflake_time, Internals, SharedSnowflake, BIT_LEN_SEQUENCE};
use crate::Snowflake;

use chrono::prelude::*;
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex},
};

/// A builder to build a [`Snowflake`] generator.
///
/// [`Snowflake`]: struct.Snowflake.html
pub struct Builder<'a> {
    start_time: Option<DateTime<Utc>>,
    machine_id: Option<&'a dyn Fn() -> Result<u8, BoxDynError>>,
    data_center_id: Option<&'a dyn Fn() -> Result<u8, BoxDynError>>,
    check_machine_id: Option<&'a dyn Fn(u8) -> bool>,
    check_data_center_id: Option<&'a dyn Fn(u8) -> bool>,
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
        Self {
            start_time: None,
            machine_id: None,
            data_center_id: None,
            check_machine_id: None,
            check_data_center_id: None,
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
    pub fn machine_id(mut self, machine_id: &'a dyn Fn() -> Result<u8, BoxDynError>) -> Self {
        self.machine_id = Some(machine_id);
        self
    }

    /// Sets the data center id.
    /// If the fn returns an error, finalize will fail.
    pub fn data_center_id(
        mut self,
        data_center_id: &'a dyn Fn() -> Result<u8, BoxDynError>,
    ) -> Self {
        self.data_center_id = Some(data_center_id);
        self
    }

    /// Set a function to check the machine id.
    /// If the fn returns false, finalize will fail.
    pub fn check_machine_id(mut self, check_machine_id: &'a dyn Fn(u8) -> bool) -> Self {
        self.check_machine_id = Some(check_machine_id);
        self
    }

    /// Set a function to check the data center id.
    /// If the fn returns false, finalize will fail.
    pub fn check_data_center_id(mut self, check_data_center_id: &'a dyn Fn(u8) -> bool) -> Self {
        self.check_data_center_id = Some(check_data_center_id);
        self
    }

    /// Finalize the builder to create a Snowflake.
    /// If any of the functions return an error, finalize will fail.
    pub fn finalize(self) -> Result<Snowflake, Error> {
        let sequence = 1 << (BIT_LEN_SEQUENCE - 1);
        let start_time = if let Some(start_time) = self.start_time {
            if start_time > Utc::now() {
                return Err(Error::StartTimeAheadOfCurrentTime(start_time));
            }

            to_snowflake_time(start_time)
        } else {
            to_snowflake_time(Utc.with_ymd_and_hms(2014, 9, 1, 0, 0, 0).unwrap())
        };

        let machine_id = if let Some(machine_id) = self.machine_id {
            match machine_id() {
                Ok(machine_id) => machine_id,
                Err(e) => return Err(Error::MachineIdFailed(e)),
            }
        } else {
            lower_8_bit_private_ip()?
        };

        if let Some(check_machine_id) = self.check_machine_id {
            if !check_machine_id(machine_id) {
                return Err(Error::CheckMachineIdFailed);
            }
        }

        let data_center_id = if let Some(data_center_id) = self.data_center_id {
            match data_center_id() {
                Ok(data_center_id) => data_center_id,
                Err(e) => return Err(Error::MachineIdFailed(e)),
            }
        } else {
            lower_8_bit_private_ip()?
        };

        if let Some(check_data_center_id) = self.check_data_center_id {
            if !check_data_center_id(data_center_id) {
                return Err(Error::CheckMachineIdFailed);
            }
        }

        let shared = Arc::new(SharedSnowflake {
            internals: Mutex::new(Internals {
                sequence,
                elapsed_time: 0,
            }),
            start_time,
            machine_id,
            data_center_id,
        });
        Ok(Snowflake::new_inner(shared))
    }
}

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

fn is_private_ipv4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 10
        || octets[0] == 172 && (octets[1] >= 16 && octets[1] < 32)
        || octets[0] == 192 && octets[1] == 168
}

#[allow(dead_code)]
pub(crate) fn lower_16_bit_private_ip() -> Result<u16, Error> {
    match private_ipv4() {
        Some(ip) => {
            let octets = ip.octets();
            Ok(((octets[2] as u16) << 8) + (octets[3] as u16))
        }
        None => Err(Error::NoPrivateIPv4),
    }
}

pub(crate) fn lower_8_bit_private_ip() -> Result<u8, Error> {
    match private_ipv4() {
        Some(ip) => {
            let octets = ip.octets();
            Ok(octets[3])
        }
        None => Err(Error::NoPrivateIPv4),
    }
}
