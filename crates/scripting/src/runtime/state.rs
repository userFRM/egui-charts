//! The state a study keeps from before its last bar, so a change of that bar resumes there.

use super::{CallSite, Site};
use std::collections::HashMap;

/// Everything that carries from one bar to the next, as it was before the last bar: the front
/// end's slots, every helper call site and the bar of a runtime error. The declared series need
/// no copy, since their earlier values do not change; they are truncated instead.
#[derive(Clone, Debug)]
pub(crate) struct Snapshot<S> {
    pub slots: S,
    pub sites: HashMap<Site, CallSite>,
    pub failed: Option<usize>,
}
