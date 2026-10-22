//! Services the host application supplies to the terminal, each through a small trait.

mod storage;

pub use storage::{Storage, StoreKind};
