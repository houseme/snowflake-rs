# Benchmarking ID generation

Run benchmarks with the default `std` feature on an otherwise idle host. Record
the commit, Rust version, target, CPU, feature set, benchmark filter, and power
settings with each result. `cargo bench` uses the optimized bench profile.

```sh
cargo bench --bench bench
cargo bench --bench bench -- 'next_id/'
cargo bench --bench bench -- 'next_ids/high_capacity/'
cargo bench --bench bench -- 'concurrent/high_capacity/'
cargo bench --bench bench -- 'encoding|decoding'
```

The complete suite includes many cases and can take several minutes. For a smoke
run, append `--test` after `--`; smoke runs verify execution, not performance.

## What the groups measure

| Group | Work measured |
| --- | --- |
| `builder/finalize` | Successful construction with explicit node IDs, including allocation and destruction. It does not benchmark the error returned by `Snowflake::new()` when IP fallback is disabled. |
| `next_id/default_capacity/single` | Sustained generation with the default 41/12/5/5 layout and production clock. Sequence exhaustion and clock waiting are included. |
| `next_id/high_capacity/single` | One call at a time with a 31/22/5/5 layout and a recent custom epoch, using the same production clock. The much higher sequence capacity helps expose clock, state update, and API costs. |
| `concurrent/{default_capacity,high_capacity}/{1,2,4,8,16}` | Shared-state generation using persistent worker threads. Each Criterion iteration represents 1,000 IDs per worker. |
| `next_ids/{default_capacity,high_capacity}/batch_alloc/*` | Batch reservation and allocation of a result vector. |
| `next_ids/{default_capacity,high_capacity}/singles_alloc/*` | The same number of individual calls collected into a new result vector. |
| `next_ids/{default_capacity,high_capacity}/batch_reuse/*` | Batch reservation into a preallocated vector cleared before each call. |
| `next_ids/{default_capacity,high_capacity}/singles_reuse/*` | Individual calls collected into an equally preallocated and cleared vector. |
| `encoding/*`, `decoding/*` | A cycle of eight values from zero through `u64::MAX`, including short and long encodings. Decoder inputs are prepared before timing. |

All fallible operations must succeed. Generation errors fail the benchmark rather
than inflating throughput. Outputs are consumed with `black_box`. Both allocating
batch and individual-call cases include result allocation and destruction; both
reusing cases exclude initial allocation. The `next_ids_into` API appends, so the
benchmark clears its vector explicitly before each iteration.

The default 12-bit sequence contains 4,096 values per millisecond. With a normally
advancing millisecond clock, one shared generator therefore has a sustained
capacity of roughly **4,096,000 IDs/s**, independent of how many threads call it.
A result near this rate can hide improvements in CPU cost. The high-capacity
layout has 4,194,304 values per millisecond, retaining the real clock read and CAS
path while moving the sequence ceiling well beyond usual CPU throughput. Its
31-bit timestamp expires about 24.9 days after the epoch selected at benchmark
setup; it is a measurement configuration, not a recommendation for deployed IDs.
Neither configuration freezes the clock or advances synthetic time on each call.

Concurrent workers are created and synchronized before timing. One command and
one result per worker are exchanged for each Criterion sample; workers perform
all iterations for that sample before replying. Thread creation and joining are
excluded, while dispatch, wakeup, scheduling, checksum accumulation, and result
collection remain included and are amortized across the sample. The one-worker
case exposes some harness overhead, but it should not be subtracted from other
results. The returned duration spans dispatch through completion of every worker.

Criterion's time estimate is an average over measured work. Even the single-call
case is **not a p95 or p99 latency measurement**. Investigate tail latency with a
separate timestamped workload and a histogram that includes clock waits. Compare
contention under the same thread count and workload; a throughput change alone
does not identify the cause. Use a CPU profiler and allocation measurements when
making claims about reduced CPU time or memory traffic.

The benchmark executable uses `std`. It can compile against the library with
`--no-default-features`, in which case it supplies a host system-clock callback;
those runs are not directly comparable to the production-clock `std` groups.

## Comparing CAS and observability features

Use the same benchmark names and host settings for each feature variant. Save
separate baselines so Criterion does not silently compare incompatible cases.

```sh
cargo bench --bench bench -- 'concurrent/high_capacity/' --save-baseline weak
cargo bench --bench bench --features use-strong-cas -- 'concurrent/high_capacity/' --baseline weak
cargo bench --bench bench -- 'next_id/high_capacity/' --save-baseline plain
cargo bench --bench bench --features metrics -- 'next_id/high_capacity/' --baseline plain
cargo bench --bench bench --features tracing -- 'next_id/high_capacity/' --baseline plain
```

The metrics and tracing commands above measure compiled instrumentation **without
an installed recorder or subscriber**. They do not measure an active telemetry
backend. Measure the deployed backend separately with its real labels, filtering,
formatting, and export behavior. Avoid `--all-features` for an attribution run:
it changes multiple factors at once, including the CAS strategy and IP fallback.
The constructor group supplies both node IDs, so enabling `ip-fallback` does not
enumerate interfaces. Benchmark `Snowflake::new()` separately when automatic
interface discovery is part of the startup cost you want to measure.

When comparing revisions, use this same harness on both revisions (or restrict
the harness to APIs present in both) and alternate baseline/candidate order to
check for host drift. Earlier measurements that created threads inside every
iteration, discarded single-call results, used one constant encoding value, or
returned unchecked errors are not comparable to this suite. Never convert a
short local benchmark into a universal speedup or production latency claim.
