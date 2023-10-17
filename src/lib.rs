// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A distributed unique ID generator inspired by [Twitter's Snowflake].
//!
//! This is a Rust implementation of the original [houseme/snowflake-rs], which is written in Go.
//!
//! ## Quickstart
//!
//! Add the following to your `Cargo.toml`:
//! ```toml
//! [dependencies]
//! snowflake_me = "0.1"
//! ```
//!
//! Use the library like this:
//!
//! ```
//! use snowflake_me::Snowflake;
//!
//! let sf = Snowflake::new().unwrap();
//! let next_id = sf.next_id().unwrap();
//! println!("{}", next_id);
//! ```
//!
//! ## Concurrent use
//!
//! Snowflake is threadSafe. `clone` it before moving to another thread:
//! ```
//! use snowflake_me::Snowflake;
//! use std::thread;
//!
//! let sf = Snowflake::new().unwrap();
//!
//! let mut children = Vec::new();
//! for _ in 0..10 {
//!     let mut thread_sf = sf.clone();
//!     children.push(thread::spawn(move || {
//!         println!("{}", thread_sf.next_id().unwrap());
//!     }));
//! }
//!
//! for child in children {
//!     child.join().unwrap();
//! }
//! ```
//!
//! [houseme/snowflake]: https://github.com/houseme/snowflake
//! [Twitter's Snowflake]: https://blog.twitter.com/2010/announcing-snowflake
#![doc(html_root_url = "https://docs.rs/snowflake_me/*")]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
mod builder;
mod error;
mod snowflake;
#[cfg(test)]
mod tests;

pub use crate::snowflake::*;
pub use builder::*;
pub use error::*;
