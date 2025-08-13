use crate::Snowflake;
use crate::error::{BoxDynError, Error};
use crate::snowflake::{SharedSnowflake, to_snowflake_time};
use chrono::prelude::*;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[cfg(feature = "ip-fallback")]
use std::net::{IpAddr, Ipv4Addr};

/// A builder for building the ['Snowflake'] generator.
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
    /// Construct a new builder for the build of ['Snowflake'].
    ///
    /// [`Snowflake`]: struct.Snowflake.html
    pub fn new() -> Self {
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

    /// Set the start time.
    /// If the time is set later than the current time, 'finalize' will fail.
    pub fn start_time(mut self, start_time: DateTime<Utc>) -> Self {
        self.start_time = Some(start_time);
        self
    }

    /// Set the machine ID.
    ///If the provided closure returns an error, 'finalize' will fail。
    pub fn machine_id(mut self, machine_id: &'a dyn Fn() -> Result<u16, BoxDynError>) -> Self {
        self.machine_id = Some(machine_id);
        self
    }

    /// Set up the data center ID.
    /// If the provided closure returns an error, 'finalize' will fail.
    pub fn data_center_id(
        mut self,
        data_center_id: &'a dyn Fn() -> Result<u16, BoxDynError>,
    ) -> Self {
        self.data_center_id = Some(data_center_id);
        self
    }

    /// Set up a function to check the machine ID.
    /// If the function returns 'false', 'finalize' will fail.
    pub fn check_machine_id(mut self, check_machine_id: &'a dyn Fn(u16) -> bool) -> Self {
        self.check_machine_id = Some(check_machine_id);
        self
    }

    /// Set up a function to check the data center ID.
    /// If the function returns 'false', 'finalize' will fail.
    pub fn check_data_center_id(mut self, check_data_center_id: &'a dyn Fn(u16) -> bool) -> Self {
        self.check_data_center_id = Some(check_data_center_id);
        self
    }

    /// Set the bit length of the timestamp section。
    pub fn bit_len_time(mut self, bit_len_time: u8) -> Self {
        self.bit_len_time = bit_len_time;
        self
    }

    /// Sets the bit length of the serial number section。
    pub fn bit_len_sequence(mut self, bit_len_sequence: u8) -> Self {
        self.bit_len_sequence = bit_len_sequence;
        self
    }

    /// Set the bit length for the Data Center ID section.
    pub fn bit_len_data_center_id(mut self, bit_len_data_center_id: u8) -> Self {
        self.bit_len_data_center_id = bit_len_data_center_id;
        self
    }

    /// Set the bit length of the machine ID section.
    pub fn bit_len_machine_id(mut self, bit_len_machine_id: u8) -> Self {
        self.bit_len_machine_id = bit_len_machine_id;
        self
    }

    /// Finish building and create a Snowflake instance.
    /// This method will return an error if any of the configured functions return an error or if validation fails.
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
            // Default start time
            to_snowflake_time(Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap())
        };

        #[cfg(feature = "ip-fallback")]
        let ip_derived_ids = get_ids_from_ip();

        let machine_id_mask = (1 << self.bit_len_machine_id) - 1;
        let machine_id = if let Some(machine_id_fn) = self.machine_id {
            machine_id_fn().map_err(Error::MachineIdFailed)?
        } else {
            #[cfg(feature = "ip-fallback")]
            {
                if let Some((_, machine_id)) = ip_derived_ids {
                    machine_id & machine_id_mask
                } else {
                    // For compatibility, leave the NoPrivateIPv4 error on hold for now
                    return Err(Error::NoPrivateIPv4);
                }
            }
            #[cfg(not(feature = "ip-fallback"))]
            {
                return Err(Error::MachineIdFailed(
                    "Machine ID not provided and `ip-fallback` feature is disabled".into(),
                ));
            }
        };

        if machine_id > machine_id_mask {
            return Err(Error::MachineIdFailed(
                format!(
                    "Machine ID {} is greater than the max allowed value {}",
                    machine_id, machine_id_mask
                )
                .into(),
            ));
        }

        if let Some(check_machine_id) = self.check_machine_id
            && !check_machine_id(machine_id)
        {
            return Err(Error::CheckMachineIdFailed);
        }

        let data_center_id_mask = (1 << self.bit_len_data_center_id) - 1;
        let data_center_id = if let Some(data_center_id_fn) = self.data_center_id {
            data_center_id_fn().map_err(Error::DataCenterIdFailed)?
        } else {
            #[cfg(feature = "ip-fallback")]
            {
                if let Some((data_center_id, _)) = ip_derived_ids {
                    data_center_id & data_center_id_mask
                } else {
                    return Err(Error::NoPrivateIPv4);
                }
            }
            #[cfg(not(feature = "ip-fallback"))]
            {
                return Err(Error::DataCenterIdFailed(
                    "Data Center ID not provided and `ip-fallback` feature is disabled".into(),
                ));
            }
        };

        if data_center_id > data_center_id_mask {
            return Err(Error::DataCenterIdFailed(
                format!(
                    "Data Center ID {} is greater than the max allowed value {}",
                    data_center_id, data_center_id_mask
                )
                .into(),
            ));
        }

        if let Some(check_data_center_id) = self.check_data_center_id
            && !check_data_center_id(data_center_id)
        {
            return Err(Error::CheckDataCenterIdFailed);
        }

        let shared = Arc::new(SharedSnowflake {
            state: AtomicU64::new(0),
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

/// Get the data center ID and machine ID from the private IP address (v4 or v6).
/// Returns a tuple (data_center_id, machine_id).
#[cfg(feature = "ip-fallback")]
fn get_ids_from_ip() -> Option<(u16, u16)> {
    if let Some(ipv4) = private_ipv4() {
        let octets = ipv4.octets();
        // IPv4: Use bytes 3 and 4
        let data_center_id = u16::from(octets[2]);
        let machine_id = u16::from(octets[3]);
        return Some((data_center_id, machine_id));
    }

    if let Some(ipv6) = private_ipv6() {
        let segments = ipv6.segments();
        //IPv6: Use the last two 16-bit segments
        let data_center_id = segments[6];
        let machine_id = segments[7];
        return Some((data_center_id, machine_id));
    }

    None
}

#[cfg(feature = "ip-fallback")]
fn private_ipv4() -> Option<Ipv4Addr> {
    pnet_datalink::interfaces()
        .iter()
        .filter(|iface| iface.is_up() && !iface.is_loopback() && !iface.ips.is_empty())
        .flat_map(|iface| iface.ips.iter())
        .find_map(|network| match network.ip() {
            IpAddr::V4(ipv4) if is_private_ipv4(&ipv4) => Some(ipv4),
            _ => None,
        })
}

#[cfg(feature = "ip-fallback")]
fn is_private_ipv4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    matches!(octets[0], 10)
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}

#[cfg(feature = "ip-fallback")]
fn private_ipv6() -> Option<std::net::Ipv6Addr> {
    pnet_datalink::interfaces()
        .iter()
        .filter(|iface| iface.is_up() && !iface.is_loopback() && !iface.ips.is_empty())
        .flat_map(|iface| iface.ips.iter())
        .find_map(|network| match network.ip() {
            IpAddr::V6(ipv6) if is_private_ipv6(&ipv6) => Some(ipv6),
            _ => None,
        })
}

#[cfg(feature = "ip-fallback")]
fn is_private_ipv6(ip: &std::net::Ipv6Addr) -> bool {
    // fc00::/7 (Unique Local Address)
    // fe80::/10 (Link-Local Address)
    (ip.segments()[0] & 0xfe00) == 0xfc00 || (ip.segments()[0] & 0xffc0) == 0xfe80
}
