use criterion::{Criterion, criterion_group, criterion_main};
use snowflake_me::{Snowflake, decompose};

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

fn bench_decompose(c: &mut Criterion) {
    let sf = Snowflake::new().expect("Could not create Snowflake");
    let next_id = sf.next_id().expect("Could not get next id");
    c.bench_function("bench_unique_id_threaded", |b| {
        b.iter(|| decompose(next_id));
    });
}

criterion_group!(snowflake_perf, bench_new, bench_next_id, bench_decompose);
criterion_main!(snowflake_perf);
