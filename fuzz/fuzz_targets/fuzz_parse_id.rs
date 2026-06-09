#![no_main]
use libfuzzer_sys::fuzz_target;
use snowflake_me::SnowflakeId;
use std::str::FromStr;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = SnowflakeId::from_str(s);
    }
});
