// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(missing_docs)]
#![cfg(feature = "metrics")]

use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use snowflake_me::Snowflake;
use std::sync::Arc;
use std::sync::OnceLock;

static SNAPSHOTTER: OnceLock<Snapshotter> = OnceLock::new();

fn get_snapshotter() -> &'static Snapshotter {
    SNAPSHOTTER.get_or_init(|| {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        metrics::set_global_recorder(recorder).ok();
        snapshotter
    })
}

fn counter_value(name: &str) -> Option<u64> {
    let snapshot = get_snapshotter().snapshot().into_vec();
    for (ck, _, _, value) in snapshot {
        if ck.key().name() == name
            && let DebugValue::Counter(val) = value
        {
            return Some(val);
        }
    }
    None
}

fn gauge_value(name: &str) -> Option<f64> {
    let snapshot = get_snapshotter().snapshot().into_vec();
    for (ck, _, _, value) in snapshot {
        if ck.key().name() == name
            && let DebugValue::Gauge(val) = value
        {
            return Some(val.into_inner());
        }
    }
    None
}

#[test]
fn test_metrics_full() {
    // Single-thread counter test — compare deltas to avoid interference from other tests
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()
        .unwrap();

    let before_single = counter_value("snowflake_ids_generated_total").unwrap_or(0);
    let n = 10;
    for _ in 0..n {
        sf.next_id().unwrap();
    }
    let after_single = counter_value("snowflake_ids_generated_total")
        .expect("snowflake_ids_generated_total not found");
    assert_eq!(
        after_single - before_single,
        n,
        "counter mismatch after single-thread generation"
    );

    // Gauge test
    sf.next_id().unwrap();
    let val = gauge_value("snowflake_sequence_utilization")
        .expect("snowflake_sequence_utilization not found");
    assert!(
        (0.0..=1.0).contains(&val),
        "gauge value {val} out of range [0.0, 1.0]"
    );

    // Multi-thread counter test — compare deltas
    let sf_mt = Arc::new(
        Snowflake::builder()
            .machine_id(&|| Ok(2))
            .data_center_id(&|| Ok(2))
            .finalize()
            .unwrap(),
    );

    let before_mt = counter_value("snowflake_ids_generated_total")
        .expect("snowflake_ids_generated_total not found");
    let num_threads = 4;
    let ids_per_thread = 50;
    let mut handles = vec![];

    for _ in 0..num_threads {
        let sf_clone = Arc::clone(&sf_mt);
        handles.push(std::thread::spawn(move || {
            for _ in 0..ids_per_thread {
                sf_clone.next_id().unwrap();
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let after_mt = counter_value("snowflake_ids_generated_total")
        .expect("snowflake_ids_generated_total not found");
    let mt_generated = (num_threads * ids_per_thread) as u64;
    assert!(
        after_mt - before_mt >= mt_generated,
        "counter mismatch after multi-thread generation: delta={}, expected>={}",
        after_mt - before_mt,
        mt_generated
    );
}
