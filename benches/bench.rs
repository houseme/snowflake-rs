use criterion::{Criterion, criterion_group, criterion_main};
use snowflake_me::Snowflake;

fn bench_new(c: &mut Criterion) {
    c.bench_function("bench_unique_id_threaded", |b| {
        b.iter(Snowflake::new);
    });
}

fn bench_next_id(c: &mut Criterion) {
    let sf = Snowflake::new().expect("Could not create Snowflake");
    c.bench_function("bench_unique_id_threaded", |b| {
        b.iter(|| sf.next_id());
    });
}

criterion_group!(snowflake_perf, bench_new, bench_next_id);
criterion_main!(snowflake_perf);
