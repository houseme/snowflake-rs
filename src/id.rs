// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use base64::Engine;
use base64::engine::general_purpose;
use core::fmt;
use core::ops::Deref;
use core::str::FromStr;

use crate::error::Error;

/// A typed Snowflake ID wrapping a `u64`.
///
/// This newtype provides encoding methods and standard trait implementations
/// for ergonomic use of Snowflake IDs throughout your application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnowflakeId(u64);

impl SnowflakeId {
    /// Create a new `SnowflakeId` from a raw `u64` value.
    #[must_use]
    pub fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the underlying `u64` value.
    #[must_use]
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Returns the lowercase hexadecimal string representation.
    #[must_use]
    pub fn hex(&self) -> String {
        format!("{:x}", self.0)
    }

    /// Returns the binary string representation.
    #[must_use]
    pub fn base2(&self) -> String {
        format!("{:b}", self.0)
    }

    /// Returns the base32 encoded string using a custom alphabet.
    #[must_use]
    pub fn base32(&self) -> String {
        const ENCODE_BASE32_MAP: &str = "ybndrfg8ejkmcpqxot1uwisza345h769";
        let mut id = self.0;
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

    /// Returns the base36 encoded string (digits + lowercase letters).
    #[must_use]
    pub fn base36(&self) -> String {
        const CHARSET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let mut id = self.0;
        if id == 0 {
            return "0".to_string();
        }
        let mut buf = Vec::new();
        while id > 0 {
            buf.push(CHARSET[(id % 36) as usize]);
            id /= 36;
        }
        buf.reverse();
        String::from_utf8(buf).expect("base36 charset is valid UTF-8")
    }

    /// Returns the base58 encoded string.
    #[must_use]
    pub fn base58(&self) -> String {
        const ENCODE_BASE58_MAP: &str =
            "123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";
        let mut id = self.0;
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

    /// Returns the base64 encoded string of the raw 8-byte ID.
    #[must_use]
    pub fn base64(&self) -> String {
        general_purpose::STANDARD.encode(self.0.to_be_bytes())
    }

    /// Decode a [`base64`](Self::base64)-encoded string back into a `SnowflakeId`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ParseIdFailed`] if the input is not valid standard base64
    /// or does not decode to exactly 8 bytes.
    pub fn from_base64(s: &str) -> Result<Self, Error> {
        let bytes = general_purpose::STANDARD
            .decode(s)
            .map_err(|e| Error::ParseIdFailed(e.to_string()))?;
        if bytes.len() != 8 {
            return Err(Error::ParseIdFailed(format!(
                "base64 input decoded to {} bytes, expected 8",
                bytes.len()
            )));
        }
        let raw = u64::from_be_bytes(bytes.try_into().expect("length checked to be 8"));
        Ok(Self(raw))
    }

    /// Decode a [`base32`](Self::base32)-encoded string back into a `SnowflakeId`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ParseIdFailed`] if the input contains characters outside
    /// the base32 alphabet or the value overflows `u64`.
    pub fn from_base32(s: &str) -> Result<Self, Error> {
        const MAP: &[u8; 32] = b"ybndrfg8ejkmcpqxot1uwisza345h769";
        Self::from_positional(s, MAP)
    }

    /// Decode a [`base58`](Self::base58)-encoded string back into a `SnowflakeId`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ParseIdFailed`] if the input contains characters outside
    /// the base58 alphabet or the value overflows `u64`.
    pub fn from_base58(s: &str) -> Result<Self, Error> {
        const MAP: &[u8; 58] = b"123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";
        Self::from_positional(s, MAP)
    }

    /// Shared positional-notation decoder for the custom-alphabet encodings.
    fn from_positional<const N: usize>(s: &str, map: &[u8; N]) -> Result<Self, Error> {
        let mut lookup = [255u8; 256];
        for (i, &byte) in map.iter().enumerate() {
            lookup[byte as usize] = i as u8;
        }
        let base = N as u64;
        let mut result = 0u64;
        for byte in s.bytes() {
            let value = lookup[byte as usize];
            if value == 255 {
                return Err(Error::ParseIdFailed(format!(
                    "invalid character in encoded ID: {:?}",
                    char::from(byte)
                )));
            }
            result = result
                .checked_mul(base)
                .and_then(|r| r.checked_add(u64::from(value)))
                .ok_or_else(|| Error::ParseIdFailed("encoded value overflowed u64".into()))?;
        }
        Ok(Self(result))
    }

    /// Returns the decimal string representation.
    #[must_use]
    pub fn string(&self) -> String {
        self.0.to_string()
    }

    /// Returns the decimal string representation as bytes.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.0.to_string().into_bytes()
    }

    /// Returns the raw 8-byte big-endian representation.
    #[must_use]
    pub fn int_bytes(&self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    /// Returns the ID as a signed `i64`.
    #[must_use]
    pub fn int64(&self) -> i64 {
        self.0 as i64
    }
}

// --- Standard trait implementations ---

impl fmt::Display for SnowflakeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for SnowflakeId {
    fn from(raw: u64) -> Self {
        Self(raw)
    }
}

impl From<SnowflakeId> for u64 {
    fn from(id: SnowflakeId) -> u64 {
        id.0
    }
}

impl AsRef<u64> for SnowflakeId {
    fn as_ref(&self) -> &u64 {
        &self.0
    }
}

impl Deref for SnowflakeId {
    type Target = u64;

    fn deref(&self) -> &u64 {
        &self.0
    }
}

impl Ord for SnowflakeId {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for SnowflakeId {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialOrd<u64> for SnowflakeId {
    fn partial_cmp(&self, other: &u64) -> Option<core::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialEq<u64> for SnowflakeId {
    fn eq(&self, other: &u64) -> bool {
        self.0 == *other
    }
}

impl FromStr for SnowflakeId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16)
                .map(SnowflakeId)
                .map_err(|e| Error::ParseIdFailed(e.to_string()))
        } else {
            s.parse::<u64>()
                .map(SnowflakeId)
                .map_err(|e| Error::ParseIdFailed(e.to_string()))
        }
    }
}

impl TryFrom<String> for SnowflakeId {
    type Error = Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<&str> for SnowflakeId {
    type Error = Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<i64> for SnowflakeId {
    type Error = Error;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if value < 0 {
            Err(Error::ParseIdFailed(format!("negative value: {value}")))
        } else {
            Ok(SnowflakeId(value as u64))
        }
    }
}

// --- Serde support ---

#[cfg(feature = "serde")]
impl serde::Serialize for SnowflakeId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SnowflakeId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = u64::deserialize(deserializer)?;
        Ok(SnowflakeId(raw))
    }
}

/// Wrapper that serializes [`SnowflakeId`] as a decimal string.
///
/// Useful for JSON where `u64` may lose precision in JavaScript.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnowflakeIdString(pub SnowflakeId);

#[cfg(feature = "serde")]
impl serde::Serialize for SnowflakeIdString {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SnowflakeIdString {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let id: SnowflakeId = s.parse().map_err(serde::de::Error::custom)?;
        Ok(SnowflakeIdString(id))
    }
}

#[cfg(feature = "serde")]
impl fmt::Display for SnowflakeIdString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
