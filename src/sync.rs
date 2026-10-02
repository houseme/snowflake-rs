// Copyright 2022 houseme
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Keep model tests on the same reservation implementation as production.
#[cfg(not(loom))]
pub(crate) use alloc::sync::Arc;
#[cfg(not(loom))]
pub(crate) use core::sync::atomic::{AtomicU64, Ordering};
#[cfg(loom)]
pub(crate) use loom::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
