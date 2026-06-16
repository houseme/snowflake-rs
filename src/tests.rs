// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[cfg(feature = "std")]
use crate::{ClockDriftStrategy, snowflake::to_snowflake_time};
use crate::{SnowflakeId, error::*, snowflake::Snowflake};
#[cfg(feature = "std")]
use chrono::prelude::*;
#[cfg(feature = "std")]
use std::time::Duration;
#[cfg(feature = "std")]
use std::{
    collections::HashSet,
    sync::{Mutex, atomic::Ordering},
};
use std::{sync::Arc, thread, time::Instant};
#[cfg(feature = "std")]
use thiserror::Error;

#[cfg(feature = "std")]
#[test]
fn test_next_id() -> Result<(), BoxDynError> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;
    assert!(sf.next_id().is_ok());
    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn test_once() -> Result<(), BoxDynError> {
    let start_instant = Instant::now();
    let now = Utc::now();
    let expected_machine_id = 10u64;
    let expected_data_center_id = 5u64;

    let sf = Snowflake::builder()
        .start_time(now)
        .machine_id(&|| Ok(expected_machine_id as u16))
        .data_center_id(&|| Ok(expected_data_center_id as u16))
        .finalize()?;

    let sleep_duration_ms = 500;
    thread::sleep(Duration::from_millis(sleep_duration_ms));

    let id = sf.next_id()?;
    let elapsed_ms = start_instant.elapsed().as_millis() as i64;
    let parts = sf.decompose(id);

    let actual_time = parts.time;
    // parts.time is the milliseconds elapsed since start_time, captured right
    // before the sleep above. We compare it against an independent monotonic
    // measurement rather than the raw sleep duration: thread::sleep only
    // guarantees sleeping *at least* the requested duration, and the test
    // runner / OS scheduler (notably on macOS) can add noticeable overhead.
    let diff = actual_time as i64 - elapsed_ms;
    assert!(
        diff.abs() <= 100,
        "unexpected time difference: actual={}ms, elapsed={}ms",
        actual_time,
        elapsed_ms
    );

    assert_eq!(
        parts.machine_id, expected_machine_id,
        "Unexpected machine id"
    );
    assert_eq!(
        parts.data_center_id, expected_data_center_id,
        "Unexpected data center id"
    );

    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn test_run_for_1s() -> Result<(), BoxDynError> {
    let now = Utc::now();
    let start_time = to_snowflake_time(now);
    let expected_machine_id = 15u64;

    let sf = Snowflake::builder()
        .start_time(now)
        .machine_id(&|| Ok(expected_machine_id as u16))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    let mut last_id = SnowflakeId::new(0);
    let mut max_sequence: u64 = 0;

    let initial = to_snowflake_time(Utc::now());
    let mut current = initial;
    while current - initial < 1000 {
        current = to_snowflake_time(Utc::now());

        let id = sf.next_id()?;
        let parts = sf.decompose(id);

        assert!(
            id > last_id,
            "duplicated id (id: {}, last_id: {})",
            id,
            last_id
        );
        last_id = id;

        let actual_time = parts.time;
        let expected_time_range = current - start_time;
        assert!(
            (actual_time as i64 - expected_time_range).abs() <= 50,
            "unexpected time difference: actual={}, expected_range={}",
            actual_time,
            expected_time_range
        );

        if max_sequence < parts.sequence {
            max_sequence = parts.sequence;
        }

        assert_eq!(
            parts.machine_id, expected_machine_id,
            "unexpected machine id: {}",
            parts.machine_id
        );
    }

    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn test_threads_uniqueness() -> Result<(), BoxDynError> {
    let sf = Arc::new(
        Snowflake::builder()
            .machine_id(&|| Ok(1))
            .data_center_id(&|| Ok(2))
            .finalize()?,
    );
    let ids = Arc::new(Mutex::new(HashSet::new()));
    let mut children = Vec::new();
    let num_threads = 10;
    let ids_per_thread = 10_000;

    for _ in 0..num_threads {
        let thread_sf = Arc::clone(&sf);
        let thread_ids = Arc::clone(&ids);
        children.push(thread::spawn(move || {
            let mut local_ids = Vec::with_capacity(ids_per_thread);
            for _ in 0..ids_per_thread {
                local_ids.push(thread_sf.next_id().unwrap());
            }
            let mut ids_lock = thread_ids.lock().unwrap();
            for id in local_ids {
                assert!(ids_lock.insert(id), "Duplicate ID detected: {id}");
            }
        }));
    }

    for child in children {
        child.join().expect("Child thread panicked");
    }

    let final_count = ids.lock().unwrap().len();
    assert_eq!(final_count, num_threads * ids_per_thread);
    println!(
        "Successfully verified {} unique IDs across {} threads.",
        final_count, num_threads
    );

    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn test_generate_10_ids() -> Result<(), BoxDynError> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(30))
        .data_center_id(&|| Ok(1))
        .finalize()?;
    let mut ids = HashSet::new();
    for _ in 0..10 {
        let id = sf.next_id()?;
        assert!(ids.insert(id), "duplicated id: {id}");
    }
    Ok(())
}

#[cfg(feature = "std")]
#[derive(Error, Debug)]
pub enum TestError {
    #[error("some error")]
    SomeError,
}

#[cfg(feature = "std")]
#[test]
fn test_builder_errors() {
    let start_time = Utc::now() + chrono::Duration::seconds(1);
    assert!(matches!(
        Snowflake::builder().start_time(start_time).finalize(),
        Err(Error::StartTimeAheadOfCurrentTime(_))
    ));

    assert!(matches!(
        Snowflake::builder()
            .machine_id(&|| Err(Box::new(TestError::SomeError)))
            .finalize(),
        Err(Error::MachineIdFailed(_))
    ));

    assert!(matches!(
        Snowflake::builder()
            .machine_id(&|| Ok(1))
            .check_machine_id(&|_| false)
            .finalize(),
        Err(Error::CheckMachineIdFailed)
    ));
}

#[test]
fn test_error_send_sync() {
    // This test ensures the Error type is Send + Sync
    let err = Error::CheckMachineIdFailed;
    thread::spawn(move || {
        let _ = err;
    })
    .join()
    .unwrap();
}

#[cfg(feature = "std")]
#[test]
fn test_over_time_limit() -> Result<(), BoxDynError> {
    let bit_len_time = 30;
    let sf = Snowflake::builder()
        .bit_len_time(bit_len_time)
        .bit_len_sequence(10)
        .bit_len_data_center_id(10)
        .bit_len_machine_id(13)
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    // Manually set the state to be over the time limit
    let time_max = 1u64 << bit_len_time;
    let time_shift = sf.0.bit_len_sequence;
    let state_over_limit = time_max << time_shift;
    sf.0.state.store(state_over_limit, Ordering::Relaxed);

    assert!(matches!(sf.next_id(), Err(Error::OverTimeLimit)));
    Ok(())
}

// --- SnowflakeId trait tests ---

#[test]
fn test_snowflake_id_display() {
    let id = SnowflakeId::new(12345);
    assert_eq!(id.to_string(), "12345");
}

#[test]
fn test_snowflake_id_from_u64() {
    let id: SnowflakeId = 12345u64.into();
    assert_eq!(id.as_u64(), 12345);
    let raw: u64 = id.into();
    assert_eq!(raw, 12345);
}

#[test]
fn test_snowflake_id_from_str() {
    let id: SnowflakeId = "12345".parse().unwrap();
    assert_eq!(id.as_u64(), 12345);

    let err = "not_a_number".parse::<SnowflakeId>();
    assert!(err.is_err());
}

#[test]
fn test_snowflake_id_ord() {
    let id1 = SnowflakeId::new(100);
    let id2 = SnowflakeId::new(200);
    assert!(id1 < id2);
    assert!(id2 > id1);
    assert_eq!(id1, SnowflakeId::new(100));
}

#[test]
fn test_snowflake_id_deref() {
    let id = SnowflakeId::new(42);
    assert_eq!(*id, 42u64);
    // Can use u64 methods via Deref
    assert_eq!(id.leading_zeros(), 58);
}

#[test]
fn test_snowflake_id_encodings() {
    let id = SnowflakeId::new(255);
    assert_eq!(id.hex(), "ff");
    assert_eq!(id.base2(), "11111111");
    assert_eq!(id.string(), "255");
    assert_eq!(id.int64(), 255);
    assert_eq!(id.int_bytes(), [0, 0, 0, 0, 0, 0, 0, 255]);
}

#[test]
fn test_snowflake_id_partial_eq_u64() {
    let id = SnowflakeId::new(100);
    assert_eq!(id, 100u64);
    assert_ne!(id, 200u64);
}

#[test]
fn test_snowflake_id_from_str_hex() {
    let id1: SnowflakeId = "12345".parse().unwrap();
    let id2: SnowflakeId = "0x3039".parse().unwrap();
    assert_eq!(id1, id2);
    assert_eq!(id1.as_u64(), 12345);

    let id3: SnowflakeId = "0X3039".parse().unwrap();
    assert_eq!(id1, id3);
}

#[test]
fn test_snowflake_id_try_from_string() {
    let id = SnowflakeId::try_from("12345".to_string()).unwrap();
    assert_eq!(id.as_u64(), 12345);
}

#[test]
fn test_snowflake_id_try_from_str() {
    let id = SnowflakeId::try_from("0x3039").unwrap();
    assert_eq!(id.as_u64(), 12345);
}

#[test]
fn test_snowflake_id_try_from_i64() {
    let id = SnowflakeId::try_from(12345i64).unwrap();
    assert_eq!(id.as_u64(), 12345);

    assert!(SnowflakeId::try_from(-1i64).is_err());
}

// --- Serde tests ---

#[cfg(feature = "serde")]
#[test]
fn test_serde_snowflake_id_roundtrip() {
    let id = SnowflakeId::new(1_234_567_890_123_456_789);
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "1234567890123456789");
    let back: SnowflakeId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

#[cfg(feature = "serde")]
#[test]
fn test_serde_snowflake_id_string_roundtrip() {
    use crate::SnowflakeIdString;
    let id = SnowflakeIdString(SnowflakeId::new(1_234_567_890_123_456_789));
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"1234567890123456789\"");
    let back: SnowflakeIdString = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

#[cfg(feature = "serde")]
#[test]
fn test_serde_decomposed_snowflake() {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()
        .unwrap();
    let id = sf.next_id().unwrap();
    let decomposed = sf.decompose(id);
    let json = serde_json::to_string(&decomposed).unwrap();
    assert!(json.contains("\"id\""));
    assert!(json.contains("\"time\""));
    assert!(json.contains("\"sequence\""));
    assert!(json.contains("\"data_center_id\""));
    assert!(json.contains("\"machine_id\""));
}

// --- Clock drift tests ---

#[cfg(feature = "std")]
#[test]
fn test_clock_drift_error_strategy() -> Result<(), BoxDynError> {
    let sf = Snowflake::builder()
        .clock_drift_strategy(ClockDriftStrategy::Error)
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    // Generate one ID to establish a baseline time
    let _id = sf.next_id()?;

    // Read current elapsed time and set state to a time far ahead of it
    let time_shift = sf.0.bit_len_sequence;
    let current_elapsed = to_snowflake_time(Utc::now()) - sf.0.start_time;
    let future_time = (current_elapsed as u64) + 100_000; // 100 seconds in the future
    sf.0.state
        .store(future_time << time_shift, Ordering::Relaxed);

    // Now next_id should detect clock drift and return Error::ClockDrift
    let result = sf.next_id();
    assert!(matches!(result, Err(Error::ClockDrift { .. })));

    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn test_clock_drift_last_timestamp_strategy() -> Result<(), BoxDynError> {
    let sf = Snowflake::builder()
        .clock_drift_strategy(ClockDriftStrategy::LastTimestamp)
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    // Generate one ID to establish a baseline
    let _id = sf.next_id()?;

    // Read current elapsed time and set state to a time far ahead of it
    let time_shift = sf.0.bit_len_sequence;
    let current_elapsed = to_snowflake_time(Utc::now()) - sf.0.start_time;
    let future_time = (current_elapsed as u64) + 100_000;
    sf.0.state
        .store(future_time << time_shift, Ordering::Relaxed);

    // With LastTimestamp strategy, should still generate IDs using the old timestamp
    let id1 = sf.next_id()?;
    let id2 = sf.next_id()?;
    assert!(id2 > id1, "IDs should be monotonically increasing");

    // Decompose and verify the time matches the "last known" timestamp
    let parts = sf.decompose(id1);
    assert_eq!(parts.time, future_time);

    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn test_clock_drift_exceeded() -> Result<(), BoxDynError> {
    let sf = Snowflake::builder()
        .clock_drift_strategy(ClockDriftStrategy::Wait)
        .max_clock_drift_ms(50)
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    // Generate one ID to establish a baseline
    let _id = sf.next_id()?;

    // Read current elapsed time and set state to a time far ahead (drift > 50ms)
    let time_shift = sf.0.bit_len_sequence;
    let current_elapsed = to_snowflake_time(Utc::now()) - sf.0.start_time;
    let future_time = (current_elapsed as u64) + 100_000; // 100 seconds >> 50ms
    sf.0.state
        .store(future_time << time_shift, Ordering::Relaxed);

    // Should return ClockDriftExceeded since drift (100s) >> max (50ms)
    let result = sf.next_id();
    assert!(matches!(
        result,
        Err(Error::ClockDriftExceeded {
            drift_ms: _,
            max_ms: 50
        })
    ));

    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn test_clock_drift_wait_strategy_normal() -> Result<(), BoxDynError> {
    // Default Wait strategy should work normally when there's no clock drift
    let sf = Snowflake::builder()
        .clock_drift_strategy(ClockDriftStrategy::Wait)
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    let mut last_id = SnowflakeId::new(0);
    for _ in 0..100 {
        let id = sf.next_id()?;
        assert!(id > last_id, "IDs should be monotonically increasing");
        last_id = id;
    }

    Ok(())
}

// --- Combined feature test ---

#[cfg(feature = "std")]
#[test]
fn test_full_features() {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()
        .unwrap();

    let id = sf.next_id().unwrap();
    assert!(id.as_u64() > 0);

    let decomposed = sf.decompose(id);
    assert_eq!(decomposed.machine_id, 1);
    assert_eq!(decomposed.data_center_id, 1);

    // Verify serde roundtrip when serde feature is enabled
    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string(&id).unwrap();
        let back: SnowflakeId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}

// --- Performance optimization tests ---

#[cfg(feature = "std")]
#[test]
fn test_next_ids_uniqueness() -> Result<(), BoxDynError> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    let ids = sf.next_ids(10_000)?;
    assert_eq!(ids.len(), 10_000);

    let set: HashSet<_> = ids.iter().collect();
    assert_eq!(set.len(), 10_000, "duplicate IDs found in batch");

    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn test_next_ids_empty() -> Result<(), BoxDynError> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    let ids = sf.next_ids(0)?;
    assert!(ids.is_empty());

    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn test_next_ids_concurrent() -> Result<(), BoxDynError> {
    let sf = Arc::new(
        Snowflake::builder()
            .machine_id(&|| Ok(1))
            .data_center_id(&|| Ok(1))
            .finalize()?,
    );
    let ids = Arc::new(Mutex::new(HashSet::new()));
    let mut handles = vec![];

    for _ in 0..10 {
        let sf_clone = Arc::clone(&sf);
        let ids_clone = Arc::clone(&ids);
        handles.push(thread::spawn(move || {
            let batch = sf_clone.next_ids(1000).unwrap();
            let mut lock = ids_clone.lock().unwrap();
            for id in batch {
                assert!(lock.insert(id), "duplicate ID detected: {id}");
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let final_count = ids.lock().unwrap().len();
    assert_eq!(final_count, 10_000);

    Ok(())
}

#[test]
fn test_cache_line_alignment() {
    use crate::snowflake::SharedSnowflake;
    use std::mem::align_of;
    assert!(
        align_of::<SharedSnowflake>() >= 64,
        "SharedSnowflake alignment {} is less than 64",
        align_of::<SharedSnowflake>()
    );
}

// --- Performance Benchmarks ---
// These tests are ignored by default. Run with `cargo test -- --ignored`.

#[test]
#[ignore = "benchmark, run with `cargo test -- --ignored`"]
fn bench_single_thread_performance() -> Result<(), BoxDynError> {
    let sf = Snowflake::new()?;
    let iterations = 1_000_000;

    let start = Instant::now();
    for _ in 0..iterations {
        // Using black_box would be better with a real bench harness,
        // but for a simple test, this is okay.
        let _ = sf.next_id()?;
    }
    let duration = start.elapsed();
    let rate = iterations as f64 / duration.as_secs_f64();

    println!("\n--- Single-Thread Benchmark ---");
    println!(
        "Generated {} IDs in {:?}. Rate: {:.2} IDs/sec",
        iterations, duration, rate
    );
    println!("-----------------------------\n");

    Ok(())
}

#[test]
#[ignore = "benchmark, run with `cargo test -- --ignored`"]
fn bench_multi_thread_throughput() -> Result<(), BoxDynError> {
    let sf = Arc::new(Snowflake::new()?);
    let num_threads = num_cpus::get().max(2); // Use available cores, at least 2
    let ids_per_thread = 1_000_000 / num_threads;
    let total_ids = num_threads * ids_per_thread;

    let start = Instant::now();
    let mut handles = vec![];

    for _ in 0..num_threads {
        let sf_clone = Arc::clone(&sf);
        handles.push(thread::spawn(move || {
            for _ in 0..ids_per_thread {
                let _ = sf_clone.next_id().unwrap();
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let rate = total_ids as f64 / duration.as_secs_f64();

    println!("\n--- Multi-Thread Benchmark ---");
    println!("Threads: {}", num_threads);
    println!(
        "Generated {} IDs in {:?}. Throughput: {:.2} IDs/sec",
        total_ids, duration, rate
    );
    println!("----------------------------\n");

    Ok(())
}

// --- no_std verification ---
//
// The crate must compile with `--no-default-features` (verified by CI).
// `cargo test` itself requires std, so these tests exercise the shared code paths
// that work in both std and no_std modes.

#[cfg(feature = "std")]
#[test]
fn test_time_source_abstraction() {
    // Verify that the time module's current_millis works (std path)
    let t1 = crate::time::current_millis();
    std::thread::sleep(Duration::from_millis(10));
    let t2 = crate::time::current_millis();
    assert!(t2 >= t1, "time should not go backward");
}

#[test]
fn test_snowflake_id_core_traits() {
    // Verify core trait implementations that must work without std
    let id = SnowflakeId::new(12345);
    assert_eq!(id.as_u64(), 12345);
    assert_eq!(format!("{id}"), "12345");
    assert_eq!(id.int64(), 12345i64);

    let id2 = SnowflakeId::new(12345);
    assert_eq!(id, id2);
    assert_eq!(id, 12345u64);

    let id3 = SnowflakeId::new(99999);
    assert!(id < id3);
}
