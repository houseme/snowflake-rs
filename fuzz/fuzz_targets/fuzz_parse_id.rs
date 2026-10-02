#![no_main]
use libfuzzer_sys::fuzz_target;
use snowflake_me::SnowflakeId;
use std::str::FromStr;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = SnowflakeId::from_str(s);
        let _ = SnowflakeId::from_base32(s);
        let _ = SnowflakeId::from_base58(s);
        let _ = SnowflakeId::from_base64(s);
    }
});
