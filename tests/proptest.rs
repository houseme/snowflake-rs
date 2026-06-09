#![allow(missing_docs)]

use proptest::prelude::*;
use snowflake_me::{DecomposedSnowflake, Snowflake, SnowflakeId};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread;

proptest! {
    #[test]
    fn id_is_monotonic_single_thread(ids in 1..10000usize) {
        let sf = Snowflake::builder()
            .machine_id(&|| Ok(1))
            .data_center_id(&|| Ok(1))
            .finalize()
            .unwrap();
        let mut last = SnowflakeId::new(0);
        for _ in 0..ids {
            let id = sf.next_id().unwrap();
            prop_assert!(id > last);
            last = id;
        }
    }

    #[test]
    fn decompose_roundtrip(id in 0u64..(1u64 << 63)) {
        let decomposed = DecomposedSnowflake::decompose(id, 41, 12, 5, 5);
        let reconstructed = (decomposed.time << 22)
            | (decomposed.data_center_id << 17)
            | (decomposed.machine_id << 12)
            | decomposed.sequence;
        prop_assert_eq!(id, reconstructed);
    }

    #[test]
    fn id_uniqueness_under_contention(
        num_threads in 2..8usize,
        ids_per_thread in 50..500usize,
    ) {
        let sf = Arc::new(
            Snowflake::builder()
                .machine_id(&|| Ok(1))
                .data_center_id(&|| Ok(1))
                .finalize()
                .unwrap(),
        );
        let ids = Arc::new(Mutex::new(HashSet::new()));
        let mut handles = vec![];

        for _ in 0..num_threads {
            let sf = Arc::clone(&sf);
            let ids = Arc::clone(&ids);
            handles.push(thread::spawn(move || {
                let mut local = Vec::with_capacity(ids_per_thread);
                for _ in 0..ids_per_thread {
                    local.push(sf.next_id().unwrap());
                }
                let mut lock = ids.lock().unwrap();
                for id in local {
                    assert!(lock.insert(id), "duplicate id: {id}");
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }
        let final_count = ids.lock().unwrap().len();
        prop_assert_eq!(final_count, num_threads * ids_per_thread);
    }
}
