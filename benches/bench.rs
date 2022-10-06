use bencher::{benchmark_group, benchmark_main, Bencher};
use snowflake_me::{decompose, Snowflake};

fn bench_new(b: &mut Bencher) {
    b.iter(|| Snowflake::new());
}

fn bench_next_id(b: &mut Bencher) {
    let mut sf = Snowflake::new().expect("Could not create Snowflake");
    b.iter(|| sf.next_id());
}

fn bench_decompose(b: &mut Bencher) {
    let mut sf = Snowflake::new().expect("Could not create Snowflake");
    let next_id = sf.next_id().expect("Could not get next id");

    b.iter(|| decompose(next_id));
}

benchmark_group!(snowflake_perf, bench_new, bench_next_id, bench_decompose);

benchmark_main!(snowflake_perf);
