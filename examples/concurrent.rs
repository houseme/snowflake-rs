#![allow(missing_docs)]

use snowflake_me::Snowflake;
use std::collections::HashSet;
use std::sync::Arc;
use std::thread;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Arc::new(
        Snowflake::builder()
            .machine_id(&|| Ok(1))
            .data_center_id(&|| Ok(1))
            .finalize()?,
    );

    let num_threads = 8;
    let ids_per_thread = 10_000;
    let mut handles = Vec::with_capacity(num_threads);

    for _ in 0..num_threads {
        let sf = Arc::clone(&sf);
        handles.push(thread::spawn(move || {
            let mut ids = Vec::with_capacity(ids_per_thread);
            for _ in 0..ids_per_thread {
                ids.push(sf.next_id().unwrap());
            }
            ids
        }));
    }

    let mut all_ids = HashSet::new();
    for handle in handles {
        for id in handle.join().unwrap() {
            assert!(all_ids.insert(id), "Duplicate ID: {id}");
        }
    }

    println!(
        "Generated {} unique IDs across {} threads",
        all_ids.len(),
        num_threads
    );
    Ok(())
}
