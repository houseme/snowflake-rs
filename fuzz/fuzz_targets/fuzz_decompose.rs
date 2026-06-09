#![no_main]
use libfuzzer_sys::fuzz_target;
use snowflake_me::DecomposedSnowflake;

fuzz_target!(|data: &[u8]| {
    if data.len() >= 8 {
        let id = u64::from_be_bytes(data[..8].try_into().unwrap());
        let id = id & ((1u64 << 63) - 1); // ensure 63 bits
        let _ = DecomposedSnowflake::decompose(id, 41, 12, 5, 5);
    }
});
