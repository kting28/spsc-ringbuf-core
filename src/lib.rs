// Include std only in cargo test
#![cfg_attr(not(test), no_std)]
pub mod ringbuf_ref;
pub mod shared_singleton;
pub mod ringbuf;
pub mod shared_pool;
