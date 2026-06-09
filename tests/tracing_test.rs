// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(missing_docs)]
#![cfg(feature = "tracing")]

use snowflake_me::Snowflake;
use tracing_subscriber::FmtSubscriber;

fn init_subscriber() -> tracing::subscriber::DefaultGuard {
    let subscriber = FmtSubscriber::builder()
        .with_target(false)
        .with_test_writer()
        .finish();
    tracing::subscriber::set_default(subscriber)
}

#[test]
fn test_tracing_next_id() {
    let _guard = init_subscriber();

    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()
        .unwrap();

    // Should not panic with tracing initialized
    let id = sf.next_id().unwrap();
    assert!(id.as_u64() > 0);
}

#[test]
fn test_tracing_builder_finalize() {
    let _guard = init_subscriber();

    // Should not panic during initialization with tracing
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(10))
        .data_center_id(&|| Ok(5))
        .finalize()
        .unwrap();

    let id = sf.next_id().unwrap();
    assert!(id.as_u64() > 0);
}

#[test]
fn test_tracing_multiple_ids() {
    let _guard = init_subscriber();

    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()
        .unwrap();

    // Generate multiple IDs to exercise all tracing paths
    for _ in 0..10 {
        let id = sf.next_id().unwrap();
        assert!(id.as_u64() > 0);
    }
}
