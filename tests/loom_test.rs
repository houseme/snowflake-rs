#![allow(missing_docs)]

#[cfg(loom)]
mod loom_tests {
    use loom::sync::Arc;
    use snowflake_me::Snowflake;

    #[test]
    fn loom_concurrent_next_id() {
        loom::model(|| {
            let sf = Arc::new(
                Snowflake::builder()
                    .machine_id(&|| Ok(1))
                    .data_center_id(&|| Ok(1))
                    .finalize()
                    .unwrap(),
            );

            let sf1 = Arc::clone(&sf);
            let sf2 = Arc::clone(&sf);

            let t1 = loom::thread::spawn(move || sf1.next_id().unwrap());
            let t2 = loom::thread::spawn(move || sf2.next_id().unwrap());

            let id1 = t1.join().unwrap();
            let id2 = t2.join().unwrap();
            assert_ne!(id1, id2);
        });
    }

    #[test]
    fn loom_concurrent_next_ids() {
        loom::model(|| {
            let sf = Arc::new(
                Snowflake::builder()
                    .machine_id(&|| Ok(1))
                    .data_center_id(&|| Ok(1))
                    .finalize()
                    .unwrap(),
            );

            let sf1 = Arc::clone(&sf);
            let sf2 = Arc::clone(&sf);

            let t1 = loom::thread::spawn(move || sf1.next_ids(10).unwrap());
            let t2 = loom::thread::spawn(move || sf2.next_ids(10).unwrap());

            let ids1 = t1.join().unwrap();
            let ids2 = t2.join().unwrap();

            let mut all = std::collections::HashSet::new();
            for id in ids1.iter().chain(ids2.iter()) {
                assert!(all.insert(*id), "duplicate: {id:?}");
            }
        });
    }
}
