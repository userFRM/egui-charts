//! Where the rows of a history response are, and which layout a response most likely has.

use serde_json::Value;

use super::profile::{BarFields, Rows};

/// The rows of `response`, one JSON value per bar, in the order the response lists them.
/// Whatever the layout, a row is a value the paths of [`BarFields`] step into:
///
/// - `Objects`, and `Arrays` without a header: the elements of the list at `rows.path`, as
///   they are.
/// - `Arrays` with a header: each element paired with the header names into an object. A short
///   row leaves its last fields absent; values beyond the header are dropped.
/// - `Columns`: row `i` is an object that holds element `i` of every list in the object at
///   `rows.path`. Members that are not lists are skipped. The longest list gives the number of
///   rows, and a shorter one leaves its field absent in the rows it does not reach.
///
/// A path that leads nowhere or to `null` is a page without rows, because services omit the
/// list when a window holds no data. A value of another type than the layout needs is an error
/// that says what was found, such as `expected a list at data.candles, found an object`.
pub(crate) fn rows(_response: &Value, _rows: &Rows) -> Result<Vec<Value>, String> {
    unimplemented!("part A")
}

/// One way to read a response, which the dialog offers for confirmation.
pub(crate) struct Proposal {
    /// Where the rows are and how they are laid out.
    pub(crate) rows: Rows,
    /// The fields recognised by name. A field that was not recognised has an empty path, for
    /// the user to click.
    pub(crate) bar: BarFields,
    /// How many rows the response holds when it is read this way.
    pub(crate) count: usize,
}

/// The ways `response` can be read, the most likely first. Empty when no list of rows is found
/// within four levels of the root.
pub(crate) fn propose(_response: &Value) -> Vec<Proposal> {
    unimplemented!("part A")
}
