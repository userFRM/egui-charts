//! Shared painting algorithms for chart and drawing geometry, and symbol marks.

pub(crate) mod dashed_line;
#[cfg(feature = "widget")]
pub(crate) mod span_cache;
pub(crate) mod symbol_mark;
pub(crate) mod volume_profile;
