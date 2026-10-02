// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Encoding wire-format, input validation, and round-trip regression tests.

use proptest::prelude::*;
use snowflake_me::SnowflakeId;

#[test]
fn encodings_preserve_known_wire_values() {
    for (raw, base32, base36, base58, base64) in [
        (0, "y", "0", "1", "AAAAAAAAAAA="),
        (1, "b", "1", "2", "AAAAAAAAAAE="),
        (31, "9", "v", "x", "AAAAAAAAAB8="),
        (32, "by", "w", "y", "AAAAAAAAACA="),
        (35, "bd", "z", "B", "AAAAAAAAACM="),
        (36, "br", "10", "C", "AAAAAAAAACQ="),
        (57, "b3", "1l", "Z", "AAAAAAAAADk="),
        (58, "b4", "1m", "21", "AAAAAAAAADo="),
        (123_456_789, "dizuei", "21i3v9", "bUKpk", "AAAAAAdbzRU="),
        (
            1 << 63,
            "eyyyyyyyyyyyy",
            "1y2p0ij32e8e8",
            "npL6MjP8Qfd",
            "gAAAAAAAAAA=",
        ),
        (
            u64::MAX,
            "x999999999999",
            "3w5e11264sgsf",
            "JPwcyDCgEup",
            "//////////8=",
        ),
    ] {
        let id = SnowflakeId::new(raw);
        assert_eq!(id.base32(), base32, "base32 for {raw}");
        assert_eq!(id.base36(), base36, "base36 for {raw}");
        assert_eq!(id.base58(), base58, "base58 for {raw}");
        assert_eq!(id.base64(), base64, "base64 for {raw}");
        assert_eq!(SnowflakeId::from_base32(base32).unwrap(), id);
        assert_eq!(SnowflakeId::from_base58(base58).unwrap(), id);
        assert_eq!(SnowflakeId::from_base64(base64).unwrap(), id);
    }
}

#[test]
fn positional_decoders_reject_empty_invalid_and_overflowing_inputs() {
    for input in [
        "",
        "0",
        "B",
        "🙂",
        "by\n",
        "oyyyyyyyyyyyy",
        "99999999999999",
    ] {
        assert!(
            SnowflakeId::from_base32(input).is_err(),
            "unexpectedly accepted base32 {input:?}"
        );
    }
    for input in [
        "",
        "0",
        "O",
        "I",
        "l",
        "🙂",
        "21 ",
        "JPwcyDCgEuq",
        "ZZZZZZZZZZZZ",
    ] {
        assert!(
            SnowflakeId::from_base58(input).is_err(),
            "unexpectedly accepted base58 {input:?}"
        );
    }
}

#[test]
fn positional_decoders_preserve_leading_zero_compatibility() {
    assert_eq!(SnowflakeId::from_base32("yyyy").unwrap().as_u64(), 0);
    assert_eq!(SnowflakeId::from_base58("1111").unwrap().as_u64(), 0);
    assert_eq!(SnowflakeId::from_base32("yyyby").unwrap().as_u64(), 32);
    assert_eq!(SnowflakeId::from_base58("11121").unwrap().as_u64(), 58);
    assert_eq!(
        SnowflakeId::from_base32("yyyyx999999999999")
            .unwrap()
            .as_u64(),
        u64::MAX
    );
    assert_eq!(
        SnowflakeId::from_base58("111JPwcyDCgEup").unwrap().as_u64(),
        u64::MAX
    );
}

#[test]
fn base64_decoder_requires_exactly_eight_bytes_with_canonical_padding() {
    for input in [
        "",               // empty input
        "AAAA",           // three bytes
        "AAAAAAAAAA==",   // seven bytes
        "AAAAAAAAAAAA",   // nine bytes
        "AAAAAAAAAAA",    // missing padding
        "AAAAAAAAAAA==",  // extra padding
        "AAAAAAAAAAA=\n", // trailing whitespace
        "AAAAAAAAAAB=",   // nonzero padding bits
        "__________8=",   // URL-safe alphabet
        "AAAAAAAAAA@=",   // invalid character
        "AAAAAAAA=AA=",   // interior padding
    ] {
        assert!(
            SnowflakeId::from_base64(input).is_err(),
            "unexpectedly accepted base64 {input:?}"
        );
    }
    assert!(SnowflakeId::from_base64(&"A".repeat(16_384)).is_err());
}

proptest! {
    #[test]
    fn encodings_roundtrip_entire_u64_range(raw in any::<u64>()) {
        let id = SnowflakeId::new(raw);
        prop_assert_eq!(SnowflakeId::from_base32(&id.base32()).unwrap(), id);
        prop_assert_eq!(SnowflakeId::from_base58(&id.base58()).unwrap(), id);
        prop_assert_eq!(SnowflakeId::from_base64(&id.base64()).unwrap(), id);
        prop_assert_eq!(u64::from_str_radix(&id.base36(), 36).unwrap(), raw);
    }
}

#[cfg(feature = "serde")]
mod serde_tests {
    use serde::Deserialize;
    use serde::de::value::{BorrowedStrDeserializer, BytesDeserializer, Error, StringDeserializer};
    use snowflake_me::{SnowflakeId, SnowflakeIdString};

    #[test]
    fn string_wrapper_preserves_decimal_hex_and_escaped_json_inputs() {
        let expected = SnowflakeIdString(SnowflakeId::new(123));
        for json in [
            r#""123""#,
            r#""0x7b""#,
            r#""0X7B""#,
            r#""\u0031\u0032\u0033""#,
        ] {
            assert_eq!(
                serde_json::from_str::<SnowflakeIdString>(json).unwrap(),
                expected
            );
            assert_eq!(
                serde_json::from_reader::<_, SnowflakeIdString>(json.as_bytes()).unwrap(),
                expected
            );
        }
        for raw in [0, (1 << 53) + 1, u64::MAX] {
            let id = SnowflakeIdString(SnowflakeId::new(raw));
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{raw}\""));
            assert_eq!(
                serde_json::from_str::<SnowflakeIdString>(&json).unwrap(),
                id
            );
        }
    }

    #[test]
    fn string_wrapper_accepts_borrowed_owned_and_utf8_byte_visitors() {
        let expected = SnowflakeIdString(SnowflakeId::new(123));
        let borrowed = BorrowedStrDeserializer::<Error>::new("123");
        assert_eq!(SnowflakeIdString::deserialize(borrowed).unwrap(), expected);
        let owned = StringDeserializer::<Error>::new("123".to_string());
        assert_eq!(SnowflakeIdString::deserialize(owned).unwrap(), expected);
        let bytes = BytesDeserializer::<Error>::new(b"123");
        assert_eq!(SnowflakeIdString::deserialize(bytes).unwrap(), expected);
        let invalid = BytesDeserializer::<Error>::new(b"\xff");
        assert!(SnowflakeIdString::deserialize(invalid).is_err());
    }

    #[test]
    fn string_wrapper_rejects_non_strings_negative_and_overflowing_values() {
        for json in [
            "123",
            "null",
            "true",
            r#""""#,
            r#""-1""#,
            r#""18446744073709551616""#,
        ] {
            assert!(
                serde_json::from_str::<SnowflakeIdString>(json).is_err(),
                "{json}"
            );
        }
    }
}
