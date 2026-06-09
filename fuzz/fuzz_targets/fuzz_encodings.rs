#![no_main]
use libfuzzer_sys::fuzz_target;
use snowflake_me::DecomposedSnowflake;

fuzz_target!(|data: &[u8]| {
    if data.len() >= 8 {
        let id = u64::from_be_bytes(data[..8].try_into().unwrap());
        let id = id & ((1u64 << 63) - 1);
        let d = DecomposedSnowflake::decompose(id, 41, 12, 5, 5);
        let _ = d.hex();
        let _ = d.base32();
        let _ = d.base36();
        let _ = d.base58();
        let _ = d.base64();
        let _ = d.string();
        let _ = d.bytes();
        let _ = d.int_bytes();
    }
});
