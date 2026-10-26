//! Revision identifiers shared by source and derived bar histories.

use std::sync::atomic::{AtomicU64, Ordering};

/// Source of revision values, so that no two chart states or
/// histories ever share one.
static REVISIONS: AtomicU64 = AtomicU64::new(0);

pub(crate) fn next_revision() -> u64 {
    REVISIONS.fetch_add(1, Ordering::Relaxed)
}
