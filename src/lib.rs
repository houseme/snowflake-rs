//! A distributed unique ID generator inspired by [Twitter's Snowflake].
//!
//! This is a Rust implementation of the original [houseme/snowflake], which is written in Go.
//!
//! ## Quickstart
//!
//! Add the following to your `Cargo.toml`:
//! ```toml
//! [dependencies]
//! snowflake = "0.1"
//! ```
//!
//! Use the library like this:
//!
//! ```
//! use snowflake::Snowflake;
//!
//! let mut sf = Snowflake::new().unwrap();
//! let next_id = sf.next_id().unwrap();
//! println!("{}", next_id);
//! ```
//!
//! ## Concurrent use
//!
//! Snowflake is threadSafe. `clone` it before moving to another thread:
//! ```
//! use snowflake::Snowflake;
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
#![doc(html_root_url = "https://docs.rs/sonyflake/*")]

mod builder;
mod error;
mod snowflake;
#[cfg(test)]
mod tests;

pub use crate::snowflake::*;
pub use builder::*;
pub use error::*;
