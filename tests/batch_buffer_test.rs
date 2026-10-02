#![allow(missing_docs)]
#![cfg(not(loom))]

use snowflake_me::{ClockDriftStrategy, Error, Snowflake, SnowflakeId};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn batch_buffer_reuses_capacity_and_appends_without_clearing() {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(0)
        .clock_source(|| 100)
        .finalize()
        .unwrap();
    let mut ids = Vec::with_capacity(20);
    let original_pointer = ids.as_ptr();
    ids.push(SnowflakeId::new(42));
    sf.next_ids_into(10, &mut ids).unwrap();
    assert_eq!(ids[0].as_u64(), 42);
    assert_eq!(ids.len(), 11);
    assert_eq!(ids.as_ptr(), original_pointer);
    assert!(ids[1..].windows(2).all(|pair| pair[0] < pair[1]));
    let previous_last = *ids.last().unwrap();
    ids.clear();
    sf.next_ids_into(10, &mut ids).unwrap();
    assert_eq!(ids.as_ptr(), original_pointer);
    assert!(ids[0] > previous_last);
}

#[test]
fn partial_batch_remains_in_buffer_and_is_never_reissued() {
    let reads = Arc::new(AtomicUsize::new(0));
    let clock_reads = reads.clone();
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(0)
        .clock_source(move || match clock_reads.fetch_add(1, Ordering::SeqCst) {
            0 => 100,
            _ => 99,
        })
        .clock_drift_strategy(ClockDriftStrategy::Error)
        .bit_len_time(52)
        .bit_len_sequence(1)
        .finalize()
        .unwrap();
    reads.store(0, Ordering::SeqCst);
    let mut output = vec![SnowflakeId::new(42)];
    assert!(matches!(
        sf.next_ids_into(3, &mut output),
        Err(Error::ClockDrift { .. })
    ));
    assert_eq!(output.len(), 3);
    assert_eq!(output[0].as_u64(), 42);
    assert_eq!(sf.decompose(output[1]).sequence, 0);
    assert_eq!(sf.decompose(output[2]).sequence, 1);
    reads.store(0, Ordering::SeqCst);
    assert!(matches!(sf.next_id(), Err(Error::ClockDrift { .. })));
    assert_eq!(output.len(), 3);
}

#[test]
fn empty_batch_does_not_read_clock_or_change_output() {
    let enabled = Arc::new(AtomicUsize::new(0));
    let read_enabled = enabled.clone();
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(0)
        .clock_source(move || {
            assert_eq!(
                read_enabled.load(Ordering::SeqCst),
                0,
                "empty batch must not read time"
            );
            100
        })
        .finalize()
        .unwrap();
    enabled.store(1, Ordering::SeqCst);
    let mut ids = vec![SnowflakeId::new(42)];
    sf.next_ids_into(0, &mut ids).unwrap();
    assert_eq!(ids, vec![SnowflakeId::new(42)]);
}

#[test]
fn clones_share_reservations_across_single_and_buffer_calls() {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(0))
        .data_center_id(&|| Ok(0))
        .start_time(0)
        .clock_source(|| 100)
        .finalize()
        .unwrap();
    let clone = sf.clone();
    let first = sf.next_id().unwrap();
    let mut batch = Vec::new();
    clone.next_ids_into(10, &mut batch).unwrap();
    let last = sf.next_id().unwrap();
    let sequences: Vec<_> = std::iter::once(first)
        .chain(batch)
        .chain(std::iter::once(last))
        .map(|id| sf.decompose(id).sequence)
        .collect();
    assert_eq!(sequences, (0..12).collect::<Vec<_>>());
}
