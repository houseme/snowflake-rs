# Snowflake-rs

A simple to use rust package to generate or parse Twitter snowflake IDs, generate time sortable 64-bit unique ids for
distributed systems (inspired from twitter snowflake)

[![Build](https://github.com/houseme/snowflake-rs/workflows/Build/badge.svg)](https://github.com/houseme/snowflake-rs/actions?query=workflow%3ABuild)
[![crates.io](https://img.shields.io/crates/v/snowflake_me.svg)](https://crates.io/crates/snowflake_me)
[![docs.rs](https://docs.rs/snowflake_me/badge.svg)](https://docs.rs/snowflake_me/)
[![License](https://img.shields.io/crates/l/snowflake_me)](LICENSE-APACHE)

A distributed unique ID generator inspired by [Twitter's Snowflake](https://blog.twitter.com/2010/announcing-snowflake).

This is a Rust implementation of the original [houseme/snowflake](https://github.com/houseme/snowflake), which is
written in Go.

A Snowflake ID is composed of

- 39 bits for time in units of 10 msec
- 8 bits for a sequence number
- 8 bits for a data center id
- 8 bits for a machine id

## Install

Add the following to your `Cargo.toml`:

```toml
[dependencies]
snowflake_me = "0.4"
```

## Quickstart

```rust
use snowflake_me::Snowflake;

let sf = Snowflake::new().unwrap();
let next_id = sf.next_id().unwrap();
println!("{}", next_id);
```

### Customize the start time

```rust
use snowflake_me::Snowflake;
use chrono::prelude::*;

let sf = Snowflake::builder().start_time(Utc::now()).finalize().unwrap();
let next_id = sf.next_id().unwrap();
println!("{}", next_id);
```

### Customize the machine ID

```rust 
use snowflake_me::Snowflake;

let sf = Snowflake::builder().machine_id( & | | Ok(42)).finalize().unwrap();
let next_id = sf.next_id().unwrap();
println!("{}", next_id);
``` 

### Customize the datacenter ID

```rust
use snowflake_me::Snowflake;

let sf = Snowflake::builder().data_center_id( & | | Ok(42)).finalize().unwrap();
let next_id = sf.next_id().unwrap();
println!("{}", next_id);
```

### Resolve ID

```rust
use snowflake_me::{decompose, Snowflake};

let sf = Snowflake::new().unwrap();
let next_id = sf.next_id().unwrap();

let parts = decompose(next_id);
println!("timestamp: {}, machine_id: {}, sequence: {}, data_center_id:{}", parts.time, parts.machine_id, parts.sequence, parts.data_center_id);
```

## Benchmarks

Run them yourself with `cargo bench`.

#### 1, Benchmarks were run on a MacBook Pro (16-inch, 2019) with a 2,4GHz i9 and 64 GB memory.

```benchmark
test bench_decompose ... bench:         651 ns/iter (+/- 251)
test bench_new       ... bench:     795,722 ns/iter (+/- 371,556)
test bench_next_id   ... bench:      36,652 ns/iter (+/- 1,105)
```

#### 2, Benchmarks were run on a MacBook Pro (15-inch, 2017) with a 2,8GHz i7 and 16 GB memory.

```benchmark
test bench_decompose ... bench:       1,066 ns/iter (+/- 132)
test bench_new       ... bench:     738,129 ns/iter (+/- 318,192)
test bench_next_id   ... bench:      37,390 ns/iter (+/- 499)
```

## License

Licensed under either of

* Apache License, Version 2.0, [LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0
* MIT license [LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as
defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.