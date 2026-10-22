//! Services a host supplies to the terminal, each through a small trait.
//!
//! Like [`Datafeed`](crate::data::Datafeed), every service is held as an `Arc<dyn Trait>` and
//! called on the UI thread. Its methods return at once and answer later through a
//! [`Reply`](crate::data::Reply), from any thread, runtime or browser task.

mod search;
mod storage;

pub use search::{SymbolQuery, SymbolSearch};
pub use storage::{Storage, StoreKind};
