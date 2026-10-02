#![allow(missing_docs)]
#![cfg(not(loom))]

use snowflake_me::{ClockDriftStrategy, Error, Snowflake};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

fn generator(clock: Arc<AtomicI64>, strategy: ClockDriftStrategy) -> Snowflake {
    Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(1_000)
        .clock_source(move || clock.load(Ordering::SeqCst))
        .clock_drift_strategy(strategy)
        .finalize()
        .unwrap()
}

#[test]
fn last_timestamp_survives_rollback_past_epoch_for_single_and_batch() {
    let clock = Arc::new(AtomicI64::new(1_010));
    let sf = generator(clock.clone(), ClockDriftStrategy::LastTimestamp);
    let first = sf.next_id().unwrap();
    clock.store(999, Ordering::SeqCst);
    let second = sf.next_id().unwrap();
    let batch = sf.next_ids(2).unwrap();
    assert_eq!(sf.decompose(second).time, 10);
    assert_eq!(sf.decompose(batch[1]).sequence, 3);
    assert!(first < second && second < batch[0] && batch[0] < batch[1]);
}

#[test]
fn error_strategy_reports_signed_timestamp_before_epoch() {
    let clock = Arc::new(AtomicI64::new(1_010));
    let sf = generator(clock.clone(), ClockDriftStrategy::Error);
    sf.next_id().unwrap();
    clock.store(i64::MIN, Ordering::SeqCst);
    assert!(matches!(
        sf.next_id(),
        Err(Error::BeforeEpoch {
            timestamp: i64::MIN,
            start_time: 1_000
        })
    ));
}

#[test]
fn last_timestamp_before_first_id_cannot_reuse_an_uninitialized_timestamp() {
    let clock = Arc::new(AtomicI64::new(1_000));
    let sf = generator(clock.clone(), ClockDriftStrategy::LastTimestamp);
    clock.store(999, Ordering::SeqCst);
    assert!(matches!(sf.next_id(), Err(Error::BeforeEpoch { .. })));
}

#[test]
fn signed_extreme_elapsed_time_returns_limit_error_without_overflow() {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(i64::MIN)
        .clock_source(|| i64::MAX)
        .finalize()
        .unwrap();
    assert!(matches!(sf.next_id(), Err(Error::OverTimeLimit)));
    assert!(matches!(sf.next_ids(2), Err(Error::OverTimeLimit)));
}

#[test]
fn zero_sequence_layout_can_issue_first_id_at_exact_epoch() {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(0)
        .clock_source(|| 0)
        .bit_len_time(53)
        .bit_len_sequence(0)
        .finalize()
        .unwrap();
    assert_eq!(sf.next_id().unwrap().as_u64(), 0);
}

fn exhausted_then_rollback(strategy: ClockDriftStrategy, batch: bool) -> Error {
    let stage = Arc::new(AtomicUsize::new(0));
    let observed = stage.clone();
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(0)
        .bit_len_time(52)
        .bit_len_sequence(1)
        .clock_source(move || match observed.load(Ordering::SeqCst) {
            0 => 100,
            _ => match observed.fetch_add(1, Ordering::SeqCst) {
                1 => 100,
                2..=10 => 0,
                _ => panic!("clock rollback was not detected inside the wait loop"),
            },
        })
        .clock_drift_strategy(strategy)
        .max_clock_drift_ms(10)
        .finalize()
        .unwrap();
    sf.next_ids(2).unwrap();
    stage.store(1, Ordering::SeqCst);
    if batch {
        sf.next_ids(2).unwrap_err()
    } else {
        sf.next_id().unwrap_err()
    }
}

#[test]
fn sequence_wait_rechecks_drift_limit_for_single_and_batch() {
    for batch in [false, true] {
        assert!(matches!(
            exhausted_then_rollback(ClockDriftStrategy::Wait, batch),
            Error::ClockDriftExceeded {
                drift_ms: 100,
                max_ms: 10
            }
        ));
    }
}

#[test]
fn sequence_wait_rechecks_error_strategy_for_single_and_batch() {
    for batch in [false, true] {
        assert!(matches!(
            exhausted_then_rollback(ClockDriftStrategy::Error, batch),
            Error::ClockDrift {
                last_time: 100,
                current_time: 0
            }
        ));
    }
}

#[test]
fn wait_strategy_measures_rollback_past_epoch_without_unsigned_wrap() {
    let clock = Arc::new(AtomicI64::new(1_010));
    let read_clock = clock.clone();
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(1_000)
        .clock_source(move || read_clock.load(Ordering::SeqCst))
        .max_clock_drift_ms(10)
        .finalize()
        .unwrap();
    sf.next_id().unwrap();
    clock.store(999, Ordering::SeqCst);
    assert!(matches!(
        sf.next_id(),
        Err(Error::ClockDriftExceeded {
            drift_ms: 11,
            max_ms: 10
        })
    ));
}

#[test]
fn wait_strategy_reuses_remaining_sequences_when_clock_catches_up() {
    let stage = Arc::new(AtomicUsize::new(0));
    let read_stage = stage.clone();
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(0)
        .clock_source(move || {
            if read_stage.load(Ordering::SeqCst) == 1 {
                read_stage.store(2, Ordering::SeqCst);
                99
            } else {
                100
            }
        })
        .finalize()
        .unwrap();
    sf.next_id().unwrap();
    stage.store(1, Ordering::SeqCst);
    assert_eq!(sf.decompose(sf.next_id().unwrap()).sequence, 1);
}

#[test]
fn exhausted_final_timestamp_returns_time_limit_without_waiting() {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(0)
        .clock_source(|| i64::MAX)
        .bit_len_time(63)
        .bit_len_sequence(0)
        .bit_len_data_center_id(0)
        .bit_len_machine_id(0)
        .finalize()
        .unwrap();
    sf.next_id().unwrap();
    assert!(matches!(sf.next_id(), Err(Error::OverTimeLimit)));
}

#[cfg(feature = "std")]
#[test]
fn timeout_bounds_stalled_clocks_for_every_strategy() {
    for strategy in [
        ClockDriftStrategy::Wait,
        ClockDriftStrategy::Error,
        ClockDriftStrategy::LastTimestamp,
    ] {
        let sf = Snowflake::builder()
            .machine_id(&|| Ok(0))
            .data_center_id(&|| Ok(0))
            .start_time(0)
            .clock_source(|| 100)
            .bit_len_time(53)
            .bit_len_sequence(0)
            .clock_drift_strategy(strategy)
            .wait_timeout(std::time::Duration::ZERO)
            .finalize()
            .unwrap();
        sf.next_id().unwrap();
        assert!(matches!(sf.next_id(), Err(Error::WaitTimeout)));
        assert!(matches!(sf.next_ids(2), Err(Error::WaitTimeout)));
    }
}

#[test]
fn exhausted_final_timestamp_cannot_wait_forever_after_rollback() {
    for strategy in [
        ClockDriftStrategy::Wait,
        ClockDriftStrategy::Error,
        ClockDriftStrategy::LastTimestamp,
    ] {
        let clock = Arc::new(AtomicI64::new(i64::MAX));
        let read_clock = clock.clone();
        let sf = Snowflake::builder()
            .machine_id(&|| Ok(0))
            .data_center_id(&|| Ok(0))
            .start_time(0)
            .clock_source(move || read_clock.load(Ordering::SeqCst))
            .clock_drift_strategy(strategy)
            .bit_len_time(63)
            .bit_len_sequence(0)
            .bit_len_data_center_id(0)
            .bit_len_machine_id(0)
            .finalize()
            .unwrap();
        sf.next_id().unwrap();
        clock.store(-1, Ordering::SeqCst);
        assert!(matches!(sf.next_id(), Err(Error::OverTimeLimit)));
    }
}

#[cfg(feature = "std")]
#[test]
fn positive_wait_timeout_expires_while_the_clock_stays_stalled() {
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    let waiting_since = Arc::new(OnceLock::<Instant>::new());
    let clock_waiting_since = waiting_since.clone();
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(0)
        .clock_source(move || {
            // Release an incorrectly unbounded wait, so a regression produces
            // a failed assertion instead of hanging the test binary.
            if clock_waiting_since
                .get()
                .is_some_and(|started| started.elapsed() >= Duration::from_secs(1))
            {
                101
            } else {
                100
            }
        })
        .bit_len_time(53)
        .bit_len_sequence(0)
        .wait_timeout(Duration::from_millis(5))
        .finalize()
        .unwrap();
    sf.next_id().unwrap();
    waiting_since.set(Instant::now()).unwrap();
    assert!(matches!(sf.next_id(), Err(Error::WaitTimeout)));
}

#[cfg(feature = "std")]
#[test]
fn batch_wait_timeout_budget_is_shared_after_a_chunk_completes() {
    use std::time::Duration;

    let stage = Arc::new(AtomicUsize::new(0));
    let clock_stage = stage.clone();
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(0)
        .clock_source(move || match clock_stage.load(Ordering::SeqCst) {
            0 => 100,
            1 => {
                // The first batch read finds an exhausted sequence and starts
                // the call's wait budget.
                clock_stage.store(2, Ordering::SeqCst);
                100
            }
            2 => {
                // Timeouts are cooperative: the generator cannot interrupt a
                // user callback. This chunk may complete after the deadline;
                // the next reservation must retain the expired wait budget.
                std::thread::sleep(Duration::from_millis(75));
                clock_stage.store(3, Ordering::SeqCst);
                101
            }
            _ => 102,
        })
        .bit_len_time(53)
        .bit_len_sequence(0)
        .wait_timeout(Duration::from_millis(50))
        .finalize()
        .unwrap();
    sf.next_id().unwrap();
    stage.store(1, Ordering::SeqCst);
    let mut output = Vec::new();
    assert!(matches!(
        sf.next_ids_into(2, &mut output),
        Err(Error::WaitTimeout)
    ));
    // A long scheduler pause or a spurious weak-CAS failure can make the call
    // time out before publishing this chunk. It must never issue both chunks:
    // resetting the budget per reservation would incorrectly allow that.
    assert!(output.len() <= 1);
    if let Some(id) = output.first() {
        assert_eq!(sf.decompose(*id).time, 101);
    }
}
