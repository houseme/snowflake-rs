#![allow(missing_docs)]
#![cfg(all(feature = "metrics", not(loom)))]

use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use snowflake_me::{ClockDriftStrategy, Snowflake};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

fn counters(snapshotter: &Snapshotter) -> std::collections::BTreeMap<String, u64> {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter_map(|(key, _, _, value)| match value {
            DebugValue::Counter(value) => Some((key.key().name().to_owned(), value)),
            _ => None,
        })
        .collect()
}

#[test]
fn last_timestamp_records_single_and_batch_successes() {
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    metrics::with_local_recorder(&recorder, || {
        let clock = Arc::new(AtomicI64::new(100));
        let clock_read = clock.clone();
        let sf = Snowflake::builder()
            .machine_id(&|| Ok(0))
            .data_center_id(&|| Ok(0))
            .start_time(0)
            .clock_source(move || clock_read.load(Ordering::SeqCst))
            .clock_drift_strategy(ClockDriftStrategy::LastTimestamp)
            .finalize()
            .unwrap();
        sf.next_id().unwrap();
        clock.store(99, Ordering::SeqCst);
        sf.next_id().unwrap();
        sf.next_ids(2).unwrap();
    });
    // A snapshot drains counters, so inspect every counter from the same snapshot.
    let values = counters(&snapshotter);
    assert_eq!(values["snowflake_ids_generated_total"], 4);
    assert_eq!(values["snowflake_clock_drift_events_total"], 2);
}

#[test]
fn last_timestamp_records_exhaustion_before_clock_recovers() {
    for batch in [false, true] {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        metrics::with_local_recorder(&recorder, || {
            let stage = Arc::new(AtomicUsize::new(0));
            let read_stage = stage.clone();
            let sf = Snowflake::builder()
                .machine_id(&|| Ok(0))
                .data_center_id(&|| Ok(0))
                .start_time(0)
                .bit_len_time(52)
                .bit_len_sequence(1)
                .clock_source(move || match read_stage.load(Ordering::SeqCst) {
                    0 => 100,
                    1 => {
                        read_stage.store(2, Ordering::SeqCst);
                        99
                    }
                    _ => 101,
                })
                .clock_drift_strategy(ClockDriftStrategy::LastTimestamp)
                .finalize()
                .unwrap();
            sf.next_ids(2).unwrap();
            stage.store(1, Ordering::SeqCst);
            if batch {
                sf.next_ids(1).unwrap();
            } else {
                sf.next_id().unwrap();
            }
        });
        let values = counters(&snapshotter);
        assert_eq!(values["snowflake_ids_generated_total"], 3);
        assert_eq!(values["snowflake_sequence_exhaustion_total"], 1);
    }
}

#[test]
fn zero_sequence_width_has_finite_full_utilization() {
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    metrics::with_local_recorder(&recorder, || {
        let sf = Snowflake::builder()
            .machine_id(&|| Ok(0))
            .data_center_id(&|| Ok(0))
            .start_time(0)
            .clock_source(|| 100)
            .bit_len_time(53)
            .bit_len_sequence(0)
            .finalize()
            .unwrap();
        sf.next_id().unwrap();
    });
    let utilization = snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .find_map(|(key, _, _, value)| {
            if key.key().name() == "snowflake_sequence_utilization"
                && let DebugValue::Gauge(value) = value
            {
                Some(value.into_inner())
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(utilization, 1.0);
}
