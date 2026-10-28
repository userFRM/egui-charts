//! Alternating swing pivots shared by the pattern studies.
//!
//! A pivot is confirmed `right` bars after its bar, as for
//! [`PivotPointsHighLow`](super::PivotPointsHighLow): its high (low) is greater (lower) than those
//! of the `left` bars before it and not lower (higher) than those of the `right` bars after it, so
//! the first of equal extremes is the pivot. Confirmed pivots alternate between highs and lows: of
//! two in a row of one kind the more extreme replaces the other, and a pivot never changes
//! otherwise. A bar a study cannot use is passed to [`Swings::skip`] and takes no part, so
//! positions count the usable bars only.

use crate::model::Bar;
use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// A confirmed swing pivot.
#[derive(Clone, Copy)]
pub(super) struct Swing {
    /// Position among the usable bars.
    pub position: usize,
    /// Index in the calculated bars.
    pub index: usize,
    pub time: DateTime<Utc>,
    pub price: f64,
    pub high: bool,
}

#[derive(Clone)]
struct Entry {
    position: usize,
    index: usize,
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
    /// Number of bars passed to [`Swings::skip`].
    skipped: usize,
    /// The latest swings, oldest first.
    pub pivots: VecDeque<Swing>,
}

impl Swings {
    /// Pass a bar that takes no part, so that indexes still count it.
    pub fn skip(&mut self) {
        self.skipped += 1;
    }

    /// Add the next usable bar, keeping `capacity` swings, and return whether the swings changed:
    /// one was added or the newest was replaced by a more extreme one of its kind.
    pub fn push(&mut self, bar: &Bar, left: usize, right: usize, capacity: usize) -> bool {
        self.window.push_back(Entry {
            position: self.position,
            index: self.position + self.skipped,
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
        let beats = |high: bool| {
            let value = |entry: &Entry| if high { entry.high } else { -entry.low };
            let pivot = value(&candidate);
            self.window.range(at - left..at).all(|v| pivot > value(v))
                && self.window.range(at + 1..).all(|v| pivot >= value(v))
        };
        let high = match (beats(true), beats(false)) {
            (true, false) => true,
            (false, true) => false,
            _ => return false,
        };
        let swing = Swing {
            position: candidate.position,
            index: candidate.index,
            time: candidate.time,
            price: if high { candidate.high } else { candidate.low },
            high,
        };
        match self.pivots.back_mut() {
            Some(last) if last.high == high => {
                if swing.price != last.price && (swing.price > last.price) == high {
                    *last = swing;
                } else {
                    return false;
                }
            }
            _ => {
                self.pivots.push_back(swing);
                if self.pivots.len() > capacity {
                    self.pivots.pop_front();
                }
            }
        }
        true
    }
}
