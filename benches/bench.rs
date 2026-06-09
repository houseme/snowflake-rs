// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use snowflake_me::Snowflake;
use std::sync::Arc;

fn bench_new(c: &mut Criterion) {
    c.bench_function("snowflake_new", |b| b.iter(Snowflake::new));
}

fn bench_next_id_single(c: &mut Criterion) {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()
        .unwrap();

    let mut group = c.benchmark_group("next_id");
    group.throughput(Throughput::Elements(1));
    group.bench_function("single", |b| b.iter(|| sf.next_id()));
    group.finish();
}

fn bench_next_id_concurrent(c: &mut Criterion) {
    let mut group = c.benchmark_group("next_id/concurrent");
    for num_threads in [2, 4, 8, 16] {
        group.throughput(Throughput::Elements(num_threads * 1000));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_threads),
            &num_threads,
            |b, &num_threads| {
                let sf = Arc::new(
                    Snowflake::builder()
                        .machine_id(&|| Ok(1))
                        .data_center_id(&|| Ok(1))
                        .finalize()
                        .unwrap(),
                );
                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|_| {
                            let sf = Arc::clone(&sf);
                            std::thread::spawn(move || {
                                for _ in 0..1000 {
                                    let _ = sf.next_id();
                                }
                            })
                        })
                        .collect();
                    for h in handles {
                        h.join().unwrap();
                    }
                });
            },
        );
    }
    group.finish();
}

fn bench_decompose(c: &mut Criterion) {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()
        .unwrap();
    let id = sf.next_id().unwrap();

    c.bench_function("decompose", |b| b.iter(|| sf.decompose(id)));
}

fn bench_encodings(c: &mut Criterion) {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()
        .unwrap();
    let id = sf.next_id().unwrap();

    let mut group = c.benchmark_group("encoding");
    group.bench_function("hex", |b| b.iter(|| id.hex()));
    group.bench_function("base2", |b| b.iter(|| id.base2()));
    group.bench_function("base32", |b| b.iter(|| id.base32()));
    group.bench_function("base36", |b| b.iter(|| id.base36()));
    group.bench_function("base58", |b| b.iter(|| id.base58()));
    group.bench_function("base64", |b| b.iter(|| id.base64()));
    group.finish();
}

fn bench_builder_finalize(c: &mut Criterion) {
    c.bench_function("builder/finalize", |b| {
        b.iter(|| {
            Snowflake::builder()
                .machine_id(&|| Ok(1))
                .data_center_id(&|| Ok(1))
                .finalize()
        })
    });
}

criterion_group!(
    snowflake_perf,
    bench_new,
    bench_next_id_single,
    bench_next_id_concurrent,
    bench_decompose,
    bench_encodings,
    bench_builder_finalize
);
criterion_main!(snowflake_perf);
