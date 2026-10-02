// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(missing_docs)]

use criterion::{
    BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
    measurement::WallTime,
};
use snowflake_me::{Error, Snowflake, SnowflakeId};
use std::hint::black_box;
use std::sync::{Arc, Barrier, mpsc};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const IDS_PER_WORKER: u64 = 1_000;
const ENCODING_VALUES: [u64; 8] = [
    0,
    31,
    58,
    u16::MAX as u64,
    u32::MAX as u64,
    0x0123_4567_89ab_cdef,
    i64::MAX as u64,
    u64::MAX,
];

fn system_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("benchmark clock must be after the Unix epoch")
        .as_millis()
        .try_into()
        .expect("benchmark clock must fit in i64 milliseconds")
}

fn generator(high_capacity: bool) -> Snowflake {
    let builder = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1));
    let builder = if high_capacity {
        // Keep the production clock path, but move the sequence ceiling out of
        // reach of normal CPU throughput. This layout lasts about 24 days.
        builder
            .start_time(system_millis() - 1_000)
            .bit_len_time(31)
            .bit_len_sequence(22)
    } else {
        builder
    };
    // Host-side no_std builds need an advancing clock as well. Production std
    // benchmarks above use the library's default clock, without a callback.
    #[cfg(not(feature = "std"))]
    let builder = builder.clock_source(system_millis);
    builder.finalize().expect("valid benchmark configuration")
}

fn bench_new(c: &mut Criterion) {
    // Snowflake::new() requires IP discovery and can simply return an error
    // without ip-fallback. Always measure successful explicit construction.
    c.bench_function("builder/finalize", |b| {
        b.iter(|| black_box(generator(false)))
    });
}

fn bench_next_id(c: &mut Criterion) {
    for (name, high_capacity) in [("default_capacity", false), ("high_capacity", true)] {
        let sf = generator(high_capacity);
        let mut group = c.benchmark_group(format!("next_id/{name}"));
        group.throughput(Throughput::Elements(1));
        group.bench_function("single", |b| {
            b.iter(|| black_box(sf.next_id().expect("ID generation must succeed")))
        });
        group.finish();
    }
}

fn bench_concurrent(c: &mut Criterion) {
    for (name, high_capacity) in [("default_capacity", false), ("high_capacity", true)] {
        let mut group = c.benchmark_group(format!("concurrent/{name}"));
        for num_threads in [1_usize, 2, 4, 8, 16] {
            group.throughput(Throughput::Elements(num_threads as u64 * IDS_PER_WORKER));
            group.bench_with_input(
                BenchmarkId::from_parameter(num_threads),
                &num_threads,
                |b, &num_threads| {
                    let sf = generator(high_capacity);
                    let ready = Arc::new(Barrier::new(num_threads + 1));
                    let (results_tx, results_rx) = mpsc::channel();
                    std::thread::scope(|scope| {
                        let mut commands = Vec::with_capacity(num_threads);
                        for _ in 0..num_threads {
                            let (command_tx, command_rx) = mpsc::channel::<u64>();
                            commands.push(command_tx);
                            let sf = sf.clone();
                            let ready = Arc::clone(&ready);
                            let results_tx = results_tx.clone();
                            scope.spawn(move || {
                                ready.wait();
                                while let Ok(iterations) = command_rx.recv() {
                                    let result = (|| -> Result<u64, Error> {
                                        let mut checksum = 0;
                                        for _ in 0..iterations {
                                            for _ in 0..IDS_PER_WORKER {
                                                checksum ^= black_box(sf.next_id()?).as_u64();
                                            }
                                        }
                                        Ok(checksum)
                                    })();
                                    if results_tx.send(result).is_err() {
                                        break;
                                    }
                                }
                            });
                        }
                        drop(results_tx);
                        ready.wait();
                        // Thread creation and teardown are outside measurement.
                        // Each sample includes one dispatch and completion per
                        // worker; work scales with Criterion's iteration count.
                        b.iter_custom(|iterations| {
                            let start = Instant::now();
                            for command in &commands {
                                command.send(iterations).expect("worker must be running");
                            }
                            for _ in 0..num_threads {
                                black_box(
                                    results_rx
                                        .recv()
                                        .expect("worker must return a result")
                                        .expect("ID generation must succeed"),
                                );
                            }
                            start.elapsed()
                        });
                        // Disconnect workers before the scoped join, including
                        // when unwinding from a failed sample.
                        drop(commands);
                    });
                },
            );
        }
        group.finish();
    }
}

fn bench_next_ids(c: &mut Criterion) {
    for (name, high_capacity) in [("default_capacity", false), ("high_capacity", true)] {
        let mut group = c.benchmark_group(format!("next_ids/{name}"));
        for count in [100_usize, 1_000, 10_000] {
            group.throughput(Throughput::Elements(count as u64));
            group.bench_with_input(
                BenchmarkId::new("batch_alloc", count),
                &count,
                |b, &count| {
                    let sf = generator(high_capacity);
                    b.iter(|| {
                        black_box(sf.next_ids(count).expect("batch generation must succeed"))
                    });
                },
            );
            group.bench_with_input(
                BenchmarkId::new("singles_alloc", count),
                &count,
                |b, &count| {
                    let sf = generator(high_capacity);
                    b.iter(|| {
                        let mut ids = Vec::with_capacity(count);
                        for _ in 0..count {
                            ids.push(sf.next_id().expect("ID generation must succeed"));
                        }
                        black_box(ids)
                    });
                },
            );
            group.bench_with_input(
                BenchmarkId::new("batch_reuse", count),
                &count,
                |b, &count| {
                    let sf = generator(high_capacity);
                    let mut ids = Vec::with_capacity(count);
                    b.iter(|| {
                        ids.clear();
                        sf.next_ids_into(count, &mut ids)
                            .expect("batch generation must succeed");
                        black_box(ids.as_slice());
                    });
                },
            );
            group.bench_with_input(
                BenchmarkId::new("singles_reuse", count),
                &count,
                |b, &count| {
                    let sf = generator(high_capacity);
                    let mut ids = Vec::with_capacity(count);
                    b.iter(|| {
                        ids.clear();
                        for _ in 0..count {
                            ids.push(sf.next_id().expect("ID generation must succeed"));
                        }
                        black_box(ids.as_slice());
                    });
                },
            );
        }
        group.finish();
    }
}

fn bench_decompose(c: &mut Criterion) {
    let sf = generator(false);
    let id = sf.next_id().expect("ID generation must succeed");
    c.bench_function("decompose", |b| {
        b.iter(|| black_box(sf.decompose(black_box(id))))
    });
}

fn bench_encoder(
    group: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    encode: impl Fn(SnowflakeId) -> String,
) {
    group.bench_function(name, |b| {
        let mut cursor = 0;
        b.iter(|| {
            let id = SnowflakeId::from(black_box(ENCODING_VALUES[cursor]));
            cursor = (cursor + 1) % ENCODING_VALUES.len();
            black_box(encode(id))
        });
    });
}

fn bench_decoder(
    group: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    encode: impl Fn(SnowflakeId) -> String,
    decode: impl Fn(&str) -> Result<SnowflakeId, Error>,
) {
    let inputs: Vec<_> = ENCODING_VALUES
        .iter()
        .map(|&value| encode(SnowflakeId::from(value)))
        .collect();
    for (input, expected) in inputs.iter().zip(ENCODING_VALUES) {
        assert_eq!(
            decode(input).expect("encoding must round trip").as_u64(),
            expected
        );
    }
    group.bench_function(name, |b| {
        let mut cursor = 0;
        b.iter(|| {
            let input = black_box(inputs[cursor].as_str());
            cursor = (cursor + 1) % inputs.len();
            black_box(decode(input).expect("decoding must succeed"))
        });
    });
}

fn bench_encodings(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoding");
    group.throughput(Throughput::Elements(1));
    bench_encoder(&mut group, "hex", |id| id.hex());
    bench_encoder(&mut group, "base2", |id| id.base2());
    bench_encoder(&mut group, "base32", |id| id.base32());
    bench_encoder(&mut group, "base36", |id| id.base36());
    bench_encoder(&mut group, "base58", |id| id.base58());
    bench_encoder(&mut group, "base64", |id| id.base64());
    group.finish();

    let mut group = c.benchmark_group("decoding");
    group.throughput(Throughput::Elements(1));
    bench_decoder(
        &mut group,
        "base32",
        |id| id.base32(),
        SnowflakeId::from_base32,
    );
    bench_decoder(
        &mut group,
        "base58",
        |id| id.base58(),
        SnowflakeId::from_base58,
    );
    bench_decoder(
        &mut group,
        "base64",
        |id| id.base64(),
        SnowflakeId::from_base64,
    );
    group.finish();
}

criterion_group!(
    snowflake_perf,
    bench_new,
    bench_next_id,
    bench_concurrent,
    bench_next_ids,
    bench_decompose,
    bench_encodings
);
criterion_main!(snowflake_perf);
