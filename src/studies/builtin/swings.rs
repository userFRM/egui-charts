//! Alternating swing pivots shared by the pattern studies.
//!
//! A pivot is confirmed `right` bars after its bar, as for
//! [`PivotPointsHighLow`](super::PivotPointsHighLow). Confirmed pivots alternate between highs and
//! lows: of two in a row of one kind the more extreme replaces the other, and a pivot never changes
//! otherwise. A bar with a non-finite high or low takes no part, so positions count the usable
//! bars only.

use crate::model::Bar;
use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// A confirmed swing pivot.
#[derive(Clone, Copy)]
pub(super) struct Swing {
    /// Index of the pivot's bar among the usable bars.
    pub position: usize,
    pub time: DateTime<Utc>,
    pub price: f64,
    pub high: bool,
}

#[derive(Clone)]
struct Entry {
    position: usize,
    time: DateTime<Utc>,
    high: f64,
    low: f64,
}

/// The bars around the next pivot and the latest confirmed swings.
#[derive(Clone, Default)]
pub(super) struct Swings {
    window: VecDeque<Entry>,
    /// Number of usable bars so far, the position of the next one.
    pub position: usize,
    /// The latest swings, oldest first.
    pub pivots: VecDeque<Swing>,
}

impl Swings {
    /// Add the next usable bar, keeping `capacity` swings, and return whether the swings changed:
    /// one was added or the newest was replaced by a more extreme one of its kind.
    pub fn push(&mut self, bar: &Bar, left: usize, right: usize, capacity: usize) -> bool {
        self.window.push_back(Entry {
            position: self.position,
            time: bar.time,
            high: bar.high,
            low: bar.low,
        });
        self.position += 1;
        if self.window.len() > left.saturating_add(right).saturating_add(1) {
            self.window.pop_front();
        }
        let Some(at) = self
            .window
            .len()
            .checked_sub(right.saturating_add(1))
            .filter(|&at| at >= left)
        else {
            return false;
        };
        let candidate = self.window[at].clone();
        // The kind that alternates with the newest swing comes first, lows if there is none.
        let order = if self.pivots.back().is_some_and(|last| !last.high) {
            [true, false]
        } else {
            [false, true]
        };
        let mut changed = false;
        for high in order {
            let value = |entry: &Entry| if high { entry.high } else { -entry.low };
            let pivot = value(&candidate);
            let before = self.window.range(at - left..at).all(|v| value(v) <= pivot);
            let after = self.window.range(at + 1..).all(|v| value(v) < pivot);
            if !(before && after) {
                continue;
            }
            let swing = Swing {
                position: candidate.position,
                time: candidate.time,
                price: if high { candidate.high } else { candidate.low },
                high,
            };
            match self.pivots.back_mut() {
                Some(last) if last.high == high => {
                    if (high && swing.price > last.price) || (!high && swing.price < last.price) {
                        *last = swing;
                        changed = true;
                    }
                }
                _ => {
                    self.pivots.push_back(swing);
                    if self.pivots.len() > capacity {
                        self.pivots.pop_front();
                    }
                    changed = true;
                }
            }
        }
        changed
    }
}
