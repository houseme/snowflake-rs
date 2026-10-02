#![allow(missing_docs)]
#![cfg(loom)]

use loom::sync::Arc;
use loom::sync::atomic::{AtomicUsize, Ordering};
use snowflake_me::{Snowflake, SnowflakeId};

fn generator(clock: impl Fn() -> i64 + Send + Sync + 'static) -> Snowflake {
    Snowflake::builder()
        .start_time(0)
        .clock_source(clock)
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .bit_len_time(59)
        .bit_len_sequence(2)
        .bit_len_machine_id(1)
        .bit_len_data_center_id(1)
        .finalize()
        .unwrap()
}

fn assert_unique(mut ids: Vec<SnowflakeId>, expected: usize) {
    assert_eq!(ids.len(), expected);
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), expected, "overlapping sequence reservations");
}

#[test]
fn concurrent_single_ids_share_atomic_state() {
    loom::model(|| {
        let sf = generator(|| 1);
        let cloned = sf.clone();
        let first = loom::thread::spawn(move || cloned.next_id().unwrap());
        let second = loom::thread::spawn(move || sf.next_id().unwrap());
        assert_unique(vec![first.join().unwrap(), second.join().unwrap()], 2);
    });
}

#[test]
fn concurrent_batches_reserve_disjoint_ranges() {
    loom::model(|| {
        let sf = generator(|| 1);
        let cloned = sf.clone();
        let first = loom::thread::spawn(move || cloned.next_ids(2).unwrap());
        let second = loom::thread::spawn(move || sf.next_ids(2).unwrap());
        let mut ids = first.join().unwrap();
        ids.extend(second.join().unwrap());
        assert_unique(ids, 4);
    });
}

#[test]
fn mixed_single_and_batch_fill_sequence_capacity() {
    loom::model(|| {
        let sf = generator(|| 1);
        let cloned = sf.clone();
        let first = loom::thread::spawn(move || cloned.next_id().unwrap());
        let second = loom::thread::spawn(move || sf.next_ids(3).unwrap());
        let mut ids = second.join().unwrap();
        ids.push(first.join().unwrap());
        assert_unique(ids, 4);
    });
}

#[test]
fn batches_cross_clock_boundaries_without_reusing_sequences() {
    loom::model(|| {
        // Advancing by reads keeps every schedule finite without a wall clock.
        // Instrument the clock too, so stale reads and competing reservations
        // are interleaved by Loom together with the generator's actual state.
        let reads = Arc::new(AtomicUsize::new(0));
        let sf = generator(move || 1 + (reads.fetch_add(1, Ordering::Relaxed) / 4) as i64);
        let cloned = sf.clone();
        let first = loom::thread::spawn(move || cloned.next_ids(3).unwrap());
        let second = loom::thread::spawn(move || sf.next_ids(3).unwrap());
        let mut ids = first.join().unwrap();
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
        let second_ids = second.join().unwrap();
        assert!(second_ids.windows(2).all(|pair| pair[0] < pair[1]));
        ids.extend(second_ids);
        assert_unique(ids, 6);
    });
}
