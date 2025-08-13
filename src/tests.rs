use crate::DecomposedSnowflake;
use crate::{
    error::*,
    snowflake::{Snowflake, to_snowflake_time},
};
use chrono::prelude::*;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

use thiserror::Error;

#[test]
fn test_next_id() -> Result<(), BoxDynError> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;
    assert!(sf.next_id().is_ok());
    Ok(())
}

// Default bit length constant for testing
const DEFAULT_BIT_LEN_TIME: u8 = 41;
const DEFAULT_BIT_LEN_SEQUENCE: u8 = 12;
const DEFAULT_BIT_LEN_DATA_CENTER_ID: u8 = 5;
const DEFAULT_BIT_LEN_MACHINE_ID: u8 = 5;

#[test]
fn test_once() -> Result<(), BoxDynError> {
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
    let parts = DecomposedSnowflake::decompose(
        id,
        DEFAULT_BIT_LEN_TIME,
        DEFAULT_BIT_LEN_SEQUENCE,
        DEFAULT_BIT_LEN_DATA_CENTER_ID,
        DEFAULT_BIT_LEN_MACHINE_ID,
    );

    let actual_time = parts.time;
    // 允许时间上的微小误差
    if actual_time < sleep_duration_ms || actual_time > sleep_duration_ms + 50 {
        panic!(
            "Unexpected time {}, expected around {}",
            actual_time, sleep_duration_ms
        )
    }

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

    let mut last_id: u64 = 0;
    let mut max_sequence: u64 = 0;

    let initial = to_snowflake_time(Utc::now());
    let mut current = initial;
    while current - initial < 1000 {
        // 运行 1 秒
        let id = sf.next_id()?;
        let parts = DecomposedSnowflake::decompose(
            id,
            DEFAULT_BIT_LEN_TIME,
            DEFAULT_BIT_LEN_SEQUENCE,
            DEFAULT_BIT_LEN_DATA_CENTER_ID,
            DEFAULT_BIT_LEN_MACHINE_ID,
        );

        assert!(
            id > last_id,
            "duplicated id (id: {}, last_id: {})",
            id,
            last_id
        );
        last_id = id;

        current = to_snowflake_time(Utc::now());

        let actual_time = parts.time as i64;
        let overtime = start_time + actual_time - current;
        assert!(overtime.abs() <= 1, "unexpected overtime: {}", overtime); // 允许 1ms 的抖动

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
                assert!(ids_lock.insert(id), "Duplicate ID detected: {}", id);
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

#[test]
fn test_generate_10_ids() -> Result<(), BoxDynError> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(30))
        .data_center_id(&|| Ok(1))
        .finalize()?;
    let mut ids = HashSet::new();
    for _ in 0..10 {
        let id = sf.next_id()?;
        assert!(ids.insert(id), "duplicated id: {}", id);
    }
    Ok(())
}

#[derive(Error, Debug)]
pub enum TestError {
    #[error("some error")]
    SomeError,
}

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

// --- Performance Benchmarks ---
// These tests are ignored by default. Run with `cargo test -- --ignored`.

#[test]
#[ignore]
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
#[ignore]
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
