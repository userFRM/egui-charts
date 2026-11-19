//! Alternating swing pivots shared by the pattern studies.
//!
//! A pivot is confirmed `right` bars after its bar, as for
//! [`PivotPointsHighLow`](super::PivotPointsHighLow): its high (low) is greater (lower) than those
//! of the `left` bars before it and not lower (higher) than those of the `right` bars after it, so
//! the first of equal extremes is the pivot. Confirmed pivots alternate between highs and lows: of
//! two in a row of one kind the more extreme replaces the other, and a pivot never changes
//! otherwise. A bar a study cannot use is passed to [`Swings::skip`] and takes no part, so
//! positions count the usable bars only. [`Swings::push_move`] sizes the swings by price instead:
//! a pivot is confirmed by the first later bar that moves back from it far enough.

use crate::model::Bar;
use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// A confirmed swing pivot.
#[derive(Clone, Copy, Debug)]
pub(super) struct Swing {
    /// Position among the usable bars.
    pub position: usize,
    /// Index in the calculated bars.
    pub index: usize,
    pub time: DateTime<Utc>,
    pub price: f64,
    pub high: bool,
}

#[derive(Clone, Debug)]
struct Entry {
    position: usize,
    index: usize,
    time: DateTime<Utc>,
    high: f64,
    low: f64,
}

/// The bars around the next pivot and the latest confirmed swings.
#[derive(Clone, Debug, Default)]
pub(super) struct Swings {
    window: VecDeque<Entry>,
    /// Number of usable bars so far, the position of the next one.
    pub position: usize,
    /// Number of bars passed to [`Swings::skip`].
    skipped: usize,
    /// The latest swings, oldest first.
    pub pivots: VecDeque<Swing>,
    /// The extremes [`Swings::push_move`] measures moves back from.
    leg: Leg,
}

/// The extremes a price-sized swing is measured from.
#[derive(Clone, Copy, Debug, Default)]
enum Leg {
    /// No bar yet.
    #[default]
    Start,
    /// No pivot yet: the highest high and the lowest low so far, the first of equal ones.
    Range { top: Swing, bottom: Swing },
    /// The extreme of the leg since the last pivot: a high in an up leg, a low in a down leg.
    Trend(Swing),
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

    /// Add the next usable bar to swings sized by price, keeping `capacity` swings, and return
    /// whether a pivot was added. The extreme of the current leg is confirmed as a pivot by the
    /// first later bar that moves back from it by more than nothing and by at least
    /// `threshold(price of the extreme)`; a bar that extends the leg is never a reversal. Before
    /// the first pivot the highest high and the lowest low so far are measured; when one bar
    /// confirms both, which happens only when both lie on one bar, the low is the first pivot.
    pub fn push_move(
        &mut self,
        bar: &Bar,
        threshold: impl Fn(f64) -> f64,
        capacity: usize,
    ) -> bool {
        let position = self.position;
        let swing = |price: f64, high: bool| Swing {
            position,
            index: position + self.skipped,
            time: bar.time,
            price,
            high,
        };
        let (high, low) = (swing(bar.high, true), swing(bar.low, false));
        self.position += 1;
        let back = |from: &Swing, price: f64| {
            let moved = if from.high {
                from.price - price
            } else {
                price - from.price
            };
            from.position < position && moved > 0.0 && moved >= threshold(from.price)
        };
        let (pivot, leg) = match self.leg {
            Leg::Start => (
                None,
                Leg::Range {
                    top: high,
                    bottom: low,
                },
            ),
            Leg::Range {
                mut top,
                mut bottom,
            } => {
                if high.price > top.price {
                    top = high;
                }
                if low.price < bottom.price {
                    bottom = low;
                }
                // Both can move back far enough only when they lie on one bar: the bar that set
                // the later of them already measured the move back from the earlier one.
                if back(&bottom, bar.high) {
                    (Some(bottom), Leg::Trend(high))
                } else if back(&top, bar.low) {
                    (Some(top), Leg::Trend(low))
                } else {
                    (None, Leg::Range { top, bottom })
                }
            }
            Leg::Trend(extreme) if extreme.high && bar.high > extreme.price => {
                (None, Leg::Trend(high))
            }
            Leg::Trend(extreme) if !extreme.high && bar.low < extreme.price => {
                (None, Leg::Trend(low))
            }
            Leg::Trend(extreme) if extreme.high && back(&extreme, bar.low) => {
                (Some(extreme), Leg::Trend(low))
            }
            Leg::Trend(extreme) if !extreme.high && back(&extreme, bar.high) => {
                (Some(extreme), Leg::Trend(high))
            }
            leg @ Leg::Trend(_) => (None, leg),
        };
        self.leg = leg;
        let Some(pivot) = pivot else {
            return false;
        };
        self.pivots.push_back(pivot);
        if self.pivots.len() > capacity {
            self.pivots.pop_front();
        }
        true
    }
}
