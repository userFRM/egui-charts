//! Marks where the close of a series rose or fell between updates.

use super::{Chart, SeriesId};
use crate::data::merge::Origin;
use crate::model::{Bar, BarClock, ChangeMarkers};
use chrono::{DateTime, Utc};
use egui::{Painter, Pos2};
use web_time::Instant;

/// The change markers of one bar series and the marks of its recent updates.
pub(super) struct ChangeMarks {
    options: ChangeMarkers,
    marks: Vec<Mark>,
}

/// The change of one bar.
struct Mark {
    time: DateTime<Utc>,
    close: f64,
    up: bool,
    at: Instant,
}

impl ChangeMarks {
    /// The slot and close of each stored bar that a live update in `incoming` merges into.
    pub(super) fn before(
        bars: &[Bar],
        clock: &BarClock,
        incoming: &[(Bar, Origin)],
    ) -> Vec<(DateTime<Utc>, f64)> {
        incoming
            .iter()
            .filter(|(_, origin)| !matches!(origin, Origin::History))
            .filter_map(|(bar, _)| {
                let slot = clock.bar_open(bar.time);
                close_at(bars, clock, slot).map(|(_, close)| (slot, close))
            })
            .collect()
    }

    /// Marks each slot of `before` whose close in `bars` differs from the close it had.
    pub(super) fn record(
        &mut self,
        bars: &[Bar],
        clock: &BarClock,
        before: Vec<(DateTime<Utc>, f64)>,
    ) {
        let now = Instant::now();
        self.marks
            .retain(|mark| now.duration_since(mark.at) < self.options.duration);
        for (slot, old) in before {
            let Some((time, close)) = close_at(bars, clock, slot) else {
                continue;
            };
            if close == old || !close.is_finite() {
                continue;
            }
            self.marks.retain(|mark| mark.time != time);
            self.marks.push(Mark {
                time,
                close,
                up: close > old,
                at: now,
            });
        }
    }

    /// Drops the marks, for bars that were replaced.
    pub(super) fn clear(&mut self) {
        self.marks.clear();
    }

    /// Paints the marks still shown, placed by `position` from a bar time and a price, and asks
    /// for a repaint when the next one expires.
    pub(super) fn paint(
        &self,
        painter: &Painter,
        position: impl Fn(DateTime<Utc>, f64) -> Option<Pos2>,
    ) {
        let now = Instant::now();
        let mut next = None::<std::time::Duration>;
        for mark in &self.marks {
            let Some(left) = self
                .options
                .duration
                .checked_sub(now.duration_since(mark.at))
            else {
                continue;
            };
            next = Some(next.map_or(left, |next| next.min(left)));
            if let Some(pos) = position(mark.time, mark.close) {
                let color = if mark.up {
                    self.options.up_color
                } else {
                    self.options.down_color
                };
                crate::chart::renderers::render_change_mark(painter, pos, mark.up, color);
            }
        }
        if let Some(next) = next {
            painter.ctx().request_repaint_after(next);
        }
    }
}

/// The time and close of the bar of `bars` in `slot`.
fn close_at(bars: &[Bar], clock: &BarClock, slot: DateTime<Utc>) -> Option<(DateTime<Utc>, f64)> {
    let at = bars.partition_point(|bar| clock.bar_open(bar.time) < slot);
    bars.get(at)
        .filter(|bar| clock.bar_open(bar.time) == slot)
        .map(|bar| (bar.time, bar.close))
}

impl Chart {
    /// Marks, for `options.duration` after each live update, where the close of `series` rose
    /// or fell against the close of the bar the update replaced: a dot at the new close with a
    /// chevron above it for a rise and below it for a fall. Updates that leave the close
    /// unchanged, new bars and history mark nothing. `None` removes the markers.
    ///
    /// Bar series accept change markers; unknown ids and study ids are ignored with a warning.
    pub fn set_change_markers(&mut self, series: SeriesId, options: Option<ChangeMarkers>) {
        let marks = options.map(|options| ChangeMarks {
            options,
            marks: Vec::new(),
        });
        if let Some(item) = self.price_series.iter_mut().find(|item| item.id == series) {
            item.change_marks = marks;
        } else if self.main_series_present
            && super::series::holds_bars(series, "set_change_markers")
        {
            self.change_marks = marks;
        }
    }
}
