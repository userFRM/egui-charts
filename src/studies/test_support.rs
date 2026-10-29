//! Bar fixtures shared by the built-in study tests.

use crate::model::Bar;
use chrono::Utc;

/// A flat bar: open, high, low and close all equal `close`.
pub(super) fn make_bar(close: f64) -> Bar {
    Bar {
        time: Utc::now(),
        open: close,
        high: close,
        low: close,
        close,
        volume: 1000.0,
        colors: None,
        payload: None,
    }
}

/// A bar opening at the middle of its range.
pub(super) fn make_range_bar(high: f64, low: f64, close: f64) -> Bar {
    Bar {
        time: Utc::now(),
        open: (high + low) / 2.0,
        high,
        low,
        close,
        volume: 1000.0,
        colors: None,
        payload: None,
    }
}
