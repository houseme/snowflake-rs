// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use base64::Engine;
use base64::engine::general_purpose;
use core::fmt;
use core::ops::Deref;
use core::str::FromStr;

use crate::error::Error;

const BASE32_ALPHABET: &[u8; 32] = b"ybndrfg8ejkmcpqxot1uwisza345h769";
const BASE36_ALPHABET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const BASE58_ALPHABET: &[u8; 58] = b"123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";
const BASE32_LOOKUP: [u8; 256] = decode_lookup(BASE32_ALPHABET);
const BASE58_LOOKUP: [u8; 256] = decode_lookup(BASE58_ALPHABET);

const fn decode_lookup(alphabet: &[u8]) -> [u8; 256] {
    let mut lookup = [u8::MAX; 256];
    let mut index = 0;
    while index < alphabet.len() {
        lookup[alphabet[index] as usize] = index as u8;
        index += 1;
    }
    lookup
}

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
        self.encode_positional(BASE32_ALPHABET)
    }

    /// Returns the base36 encoded string (digits + lowercase letters).
    #[must_use]
    pub fn base36(&self) -> String {
        self.encode_positional(BASE36_ALPHABET)
    }

    /// Returns the base58 encoded string.
    #[must_use]
    pub fn base58(&self) -> String {
        self.encode_positional(BASE58_ALPHABET)
    }

    fn encode_positional<const N: usize>(&self, alphabet: &[u8; N]) -> String {
        // Thirteen digits suffice for every u64 in each of the supported bases (32, 36, 58).
        let mut buffer = [0u8; 13];
        let mut start = buffer.len();
        let mut id = self.0;
        loop {
            start -= 1;
            buffer[start] = alphabet[(id % N as u64) as usize];
            id /= N as u64;
            if id == 0 {
                break;
            }
        }

        // The fixed alphabets are ASCII, so this borrows the stack buffer before one allocation.
        String::from_utf8_lossy(&buffer[start..]).into_owned()
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
        // Eight bytes always occupy twelve characters in padded standard base64.
        if s.len() != 12 {
            return Err(Error::ParseIdFailed(
                "base64 ID must contain exactly 12 characters".into(),
            ));
        }
        let mut bytes = [0u8; 8];
        let decoded_len = general_purpose::STANDARD
            .decode_slice(s, &mut bytes)
            .map_err(|e| Error::ParseIdFailed(e.to_string()))?;
        if decoded_len != bytes.len() {
            return Err(Error::ParseIdFailed(format!(
                "base64 input decoded to {decoded_len} bytes, expected 8"
            )));
        }
        Ok(Self(u64::from_be_bytes(bytes)))
    }

    /// Decode a [`base32`](Self::base32)-encoded string back into a `SnowflakeId`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ParseIdFailed`] if the input is empty, contains characters outside
    /// the base32 alphabet or the value overflows `u64`.
    pub fn from_base32(s: &str) -> Result<Self, Error> {
        Self::from_positional(s, &BASE32_LOOKUP, 32)
    }

    /// Decode a [`base58`](Self::base58)-encoded string back into a `SnowflakeId`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ParseIdFailed`] if the input is empty, contains characters outside
    /// the base58 alphabet or the value overflows `u64`.
    pub fn from_base58(s: &str) -> Result<Self, Error> {
        Self::from_positional(s, &BASE58_LOOKUP, 58)
    }

    /// Shared positional-notation decoder for the custom-alphabet encodings.
    fn from_positional(s: &str, lookup: &[u8; 256], base: u64) -> Result<Self, Error> {
        if s.is_empty() {
            return Err(Error::ParseIdFailed("encoded ID must not be empty".into()));
        }
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
        serializer.collect_str(&self.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SnowflakeIdString {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IdStringVisitor;

        impl serde::de::Visitor<'_> for IdStringVisitor {
            type Value = SnowflakeIdString;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a decimal or hexadecimal Snowflake ID string")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                value.parse().map(SnowflakeIdString).map_err(E::custom)
            }

            fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
                let value = core::str::from_utf8(value)
                    .map_err(|_| E::invalid_value(serde::de::Unexpected::Bytes(value), &self))?;
                self.visit_str(value)
            }
        }

        deserializer.deserialize_str(IdStringVisitor)
    }
}

#[cfg(feature = "serde")]
impl fmt::Display for SnowflakeIdString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
