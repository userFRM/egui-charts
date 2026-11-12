//! Marks where the close of a series rose or fell between updates.

use super::{Chart, SeriesId};
use crate::data::merge::Origin;
use crate::model::{Bar, BarClock, ChangeMarkers};
use chrono::{DateTime, Utc};
use egui::{Painter, Pos2};
use std::cell::RefCell;

/// The change markers of one bar series and the marks of its recent updates.
pub(super) struct ChangeMarks {
    options: ChangeMarkers,
    marks: RefCell<Vec<Mark>>,
}

/// The change of one bar.
struct Mark {
    slot: DateTime<Utc>,
    time: DateTime<Utc>,
    close: f64,
    up: bool,
    at: Option<f64>,
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
        self.retain_bars(bars, clock);
        let marks = self.marks.get_mut();
        for (slot, old) in before {
            let Some((time, close)) = close_at(bars, clock, slot) else {
                continue;
            };
            if close == old || !old.is_finite() || !close.is_finite() {
                continue;
            }
            marks.retain(|mark| mark.slot != slot);
            marks.push(Mark {
                slot,
                time,
                close,
                up: close > old,
                at: None,
            });
        }
    }

    /// Keeps marks on their stored bars under the current merge clock.
    pub(super) fn retain_bars(&mut self, bars: &[Bar], clock: &BarClock) {
        self.marks.get_mut().retain_mut(|mark| {
            let slot = clock.bar_open(mark.time);
            let Some((time, close)) = close_at(bars, clock, slot) else {
                return false;
            };
            if !close.is_finite() {
                return false;
            }
            mark.slot = slot;
            mark.time = time;
            true
        });
    }

    /// Drops the marks, for bars that were replaced.
    pub(super) fn clear(&mut self) {
        self.marks.get_mut().clear();
    }

    /// Starts the fade of marks recorded since the last frame at `now` and drops the marks whose
    /// duration has passed, whether or not the series is painted.
    pub(super) fn expire(&self, now: f64) {
        let duration = self.options.duration.as_secs_f64();
        self.marks
            .borrow_mut()
            .retain_mut(|mark| now - *mark.at.get_or_insert(now) < duration);
    }

    /// Paints fading marks placed by `position`, removes expired marks and requests another
    /// frame while a fade is visible.
    pub(super) fn paint(
        &self,
        painter: &Painter,
        position: impl Fn(DateTime<Utc>, f64) -> Option<Pos2>,
    ) {
        let now = painter.ctx().input(|input| input.time);
        self.expire(now);
        let duration = self.options.duration.as_secs_f64();
        for mark in self.marks.borrow().iter() {
            let elapsed = now - mark.at.unwrap_or(now);
            if let Some(pos) = position(mark.time, mark.close)
                && painter.clip_rect().expand(12.0).contains(pos)
            {
                let color = if mark.up {
                    self.options.up_color
                } else {
                    self.options.down_color
                };
                let opacity = 1.0 - elapsed / duration;
                crate::chart::rendering::render_change_mark(
                    painter,
                    pos,
                    mark.up,
                    color.gamma_multiply(opacity as f32),
                );
                painter.ctx().request_repaint();
            }
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
    /// Live change marker settings of a bar series, or `None` when disabled or absent.
    pub fn change_markers(&self, series: SeriesId) -> Option<ChangeMarkers> {
        if let Some(item) = self.price_series.iter().find(|item| item.id == series) {
            item.change_marks.as_ref().map(|marks| marks.options)
        } else if series == SeriesId::MAIN && self.main_series_present {
            self.change_marks.as_ref().map(|marks| marks.options)
        } else {
            None
        }
    }

    /// Marks, for `options.duration` after each live update, where the close of `series` rose
    /// or fell against the close of the bar the update replaced: a dot at the new close with a
    /// chevron above it for a rise and below it for a fall. Each mark fades linearly to
    /// transparent over that duration, and another changed close of the same bar restarts its
    /// fade. Fades run on egui's frame clock from the first frame after the change, whether or
    /// not the series is drawn, so a series shown again draws only the marks still within their
    /// duration. Updates that leave the close unchanged, new bars, whitespace fills and history
    /// mark nothing. `None` removes the markers.
    ///
    /// Bar series accept change markers; unknown ids and study ids are ignored with a warning.
    pub fn set_change_markers(&mut self, series: SeriesId, options: Option<ChangeMarkers>) {
        let marks = options.map(|options| ChangeMarks {
            options,
            marks: RefCell::default(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::test_frames::chart;
    use egui::epaint::ColorMode;
    use egui::{Color32, Shape};
    use std::time::Duration;

    #[test]
    fn live_bars_stamped_inside_their_slot_restart_one_mark() {
        use crate::data::{Datafeed, HistoryPage, HistoryRequest, Reply, Sink, Subscription};
        use crate::model::Timeframe;
        use crate::widget::ChartBuilder;
        use chrono::TimeDelta;
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct Feed(Mutex<Option<Sink>>);
        impl Datafeed for Feed {
            fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
                reply.send(Ok(HistoryPage::new(Vec::new(), true)));
            }
            fn subscribe(&self, subscription: Subscription) {
                *self.0.lock().unwrap() = Some(subscription.sink);
            }
        }

        for main in [true, false] {
            let feed = Arc::new(Feed::default());
            let mut chart = ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol("A")
                .with_timeframe(Timeframe::Min1)
                .build();
            let ctx = egui::Context::default();
            for _ in 0..4 {
                chart.pump(&ctx);
            }
            let start = DateTime::UNIX_EPOCH + TimeDelta::hours(10);
            let bars: Vec<_> = (0..10)
                .map(|i| {
                    Bar::new(
                        start + TimeDelta::minutes(i),
                        100.0,
                        101.0,
                        99.0,
                        100.0,
                        1.0,
                    )
                })
                .collect();
            let slot = bars[9].time;
            let id = if main {
                SeriesId::MAIN
            } else {
                chart.add_series(Default::default())
            };
            chart.set_data(id, bars);
            chart.set_change_markers(id, Some(ChangeMarkers::default()));
            for (paused, seconds, close, up) in [
                (false, 20, 101.0, true),
                (false, 40, 99.0, false),
                (false, 50, 99.0, false),
                (true, 50, 100.0, true),
            ] {
                if paused {
                    chart.set_feed_paused(true);
                    assert!(chart.pop_bars(id, 0).is_empty());
                    assert_eq!(marks(&chart, main).marks.borrow().len(), 1);
                }
                let time = slot + TimeDelta::seconds(seconds);
                let bar = Bar::new(time, 100.0, 101.0, 99.0, close, 1.0);
                if main && !paused {
                    feed.0.lock().unwrap().as_ref().unwrap().push_bar(bar);
                    chart.pump(&ctx);
                } else {
                    chart.update(id, bar);
                }
                assert_eq!(chart.bars(id).len(), 10);
                assert_eq!(chart.bars(id)[9].time, time);
                let change_marks = marks(&chart, main);
                assert_eq!(
                    change_marks.marks.borrow().len(),
                    1,
                    "main {main}, second {seconds}, paused {paused}"
                );
                let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    change_marks.paint(ui.painter(), |marked_time, marked_close| {
                        assert_eq!(marked_time, time);
                        assert_eq!(marked_close, close);
                        assert_eq!(
                            crate::model::index_at(chart.bars(id), marked_time),
                            Some(9.0)
                        );
                        Some(Pos2::new(50.0, 50.0))
                    });
                });
                output.drop_without_applying_deltas();
                assert_eq!(change_marks.marks.borrow()[0].up, up);
            }
        }
    }

    #[test]
    fn filling_a_whitespace_bar_marks_no_change() {
        for main in [true, false] {
            let mut chart = chart(&[100.0], &[110.0]);
            let id = if main {
                SeriesId::MAIN
            } else {
                let id = chart.add_series(Default::default());
                chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
                id
            };
            chart.set_change_markers(id, Some(ChangeMarkers::default()));
            let time = chart.bars(id).last().unwrap().time;
            for marked in [false, true] {
                if marked {
                    chart.update(id, Bar::new(time, 100.0, 112.0, 100.0, 108.0, 1.0));
                    assert_eq!(marks(&chart, main).marks.borrow().len(), 1);
                }
                chart.update(id, Bar::whitespace(time));
                assert!(marks(&chart, main).marks.borrow().is_empty());
                chart.update(id, Bar::new(time, 100.0, 112.0, 100.0, 105.0, 1.0));
                assert!(marks(&chart, main).marks.borrow().is_empty(), "main {main}");
            }
        }
    }

    #[test]
    fn change_marks_leave_with_their_bars() {
        for main in [true, false] {
            for (replay, restore, evict, limit) in [
                (false, false, false, false),
                (true, false, false, false),
                (true, true, false, false),
                (false, false, true, false),
                (false, false, true, true),
            ] {
                if main && restore {
                    continue;
                }
                let mut chart = chart(&[100.0], &[110.0]);
                let id = if main {
                    SeriesId::MAIN
                } else {
                    let id = chart.add_series(Default::default());
                    chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
                    id
                };
                // Host bars have exact keys even with a displayed interval. Two retained bars
                // can share a replay slot without sharing their marks.
                chart.set_timeframe(crate::model::Timeframe::Min1);
                let mut bars = chart.bars(id).to_vec();
                bars[97].time = bars[98].time;
                bars[98].time += chrono::TimeDelta::seconds(20);
                chart.set_data(id, bars);
                chart.set_change_markers(id, Some(ChangeMarkers::default()));
                let previous = chart.bars(id)[98].time;
                let last = chart.bars(id)[99].time;
                for time in [previous, last] {
                    chart.update(id, Bar::new(time, 100.0, 110.0, 100.0, 108.0, 1.0));
                }
                assert_eq!(marks(&chart, main).marks.borrow().len(), 2);
                if replay {
                    if restore {
                        assert!(chart.user_action(|chart| chart.remove_series(id)));
                    }
                    chart.set_replay_time(Some(last));
                    if restore {
                        assert!(chart.undo());
                    }
                } else if evict {
                    // The oldest bars leave while the marks of the retained ones stay.
                    if limit {
                        // A bar past the limit keeps the newest 100 - 100 / 8 bars.
                        chart.set_bar_limit(id, std::num::NonZeroUsize::new(100));
                        let next = last + chrono::TimeDelta::days(1);
                        chart.update(id, Bar::new(next, 100.0, 110.0, 100.0, 110.0, 1.0));
                        assert_eq!(chart.bars(id).len(), 88);
                    } else {
                        assert_eq!(chart.remove_oldest_bars(id, 10).len(), 10);
                    }
                    assert_eq!(marks(&chart, main).marks.borrow().len(), 2);
                    let newer = 1 + usize::from(limit);
                    assert_eq!(chart.pop_bars(id, newer).len(), newer);
                } else {
                    assert!(chart.pop_bars(id, 0).is_empty());
                    assert_eq!(marks(&chart, main).marks.borrow().len(), 2);
                    assert_eq!(chart.pop_bars(id, 1).len(), 1);
                }
                let kept = usize::from(!replay || !main);
                assert_eq!(
                    marks(&chart, main).marks.borrow().len(),
                    kept,
                    "main {main}, replay {replay}, restored {restore}, evicted {evict}, limit {limit}"
                );
                assert!(
                    marks(&chart, main)
                        .marks
                        .borrow()
                        .iter()
                        .all(|mark| mark.time == previous)
                );
                if replay {
                    chart.set_replay_time(None);
                } else {
                    chart.update(id, Bar::new(last, 100.0, 112.0, 100.0, 112.0, 1.0));
                }
                assert_eq!(
                    marks(&chart, main).marks.borrow().len(),
                    kept,
                    "returning bar starts without a mark"
                );
                let left = match (evict, limit) {
                    (false, _) => 100,
                    (true, false) => 90,
                    (true, true) => 87,
                };
                assert_eq!(chart.pop_bars(id, usize::MAX).len(), left);
                assert!(marks(&chart, main).marks.borrow().is_empty());
            }
        }
    }

    fn marks(chart: &Chart, main: bool) -> &ChangeMarks {
        if main {
            chart.change_marks.as_ref().unwrap()
        } else {
            chart.price_series[0].change_marks.as_ref().unwrap()
        }
    }

    /// Both directions keep their colour while opacity falls linearly: halfway through
    /// 10 seconds every premultiplied channel is halved. A changed close restarts that
    /// lifetime; at its end the mark is removed and idle frames request no further repaint.
    #[test]
    fn change_marks_fade_on_the_frame_clock() {
        let up = Color32::from_rgba_premultiplied(20, 160, 40, 200);
        let down = Color32::from_rgba_premultiplied(120, 20, 40, 160);
        for main in [true, false] {
            let mut chart = chart(&[100.0], &[110.0]);
            let id = if main {
                SeriesId::MAIN
            } else {
                let id = chart.add_series(crate::widget::SeriesOptions::default());
                chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
                id
            };
            chart.set_change_markers(
                id,
                Some(ChangeMarkers {
                    up_color: up,
                    down_color: down,
                    duration: Duration::from_secs(10),
                }),
            );
            let time = chart.bars(id).last().unwrap().time;
            let ctx = egui::Context::default();
            for (start, close, color, half) in [
                (
                    0.5,
                    108.0,
                    down,
                    Color32::from_rgba_premultiplied(60, 10, 20, 80),
                ),
                (
                    6.0,
                    109.0,
                    up,
                    Color32::from_rgba_premultiplied(10, 80, 20, 100),
                ),
            ] {
                chart.update(id, Bar::new(time, 100.0, 110.0, 100.0, close, 1.0));
                assert_eq!(marks(&chart, main).marks.borrow().len(), 1);
                for (age, expected) in [(0, color), (5, half), (10, Color32::TRANSPARENT)] {
                    if age == 10 && close == 108.0 {
                        // Keep the half-faded mark for the next update to replace.
                        continue;
                    }
                    if age == 5 {
                        chart.update(id, Bar::new(time, 100.0, 110.0, 100.0, close, 1.0));
                    }
                    // egui can carry a repaint request from the preceding frame.
                    for _ in 0..3 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                time: Some(start + f64::from(age)),
                                ..Default::default()
                            },
                            |ui| {
                                marks(&chart, main)
                                    .paint(ui.painter(), |_, _| Some(Pos2::new(50.0, 50.0)));
                            },
                        );
                        let painted: Vec<_> = std::mem::take(&mut output.shapes)
                            .into_iter()
                            .filter_map(|shape| match shape.shape {
                                Shape::Circle(dot) => Some(dot.fill),
                                Shape::Path(path) => match path.stroke.color {
                                    ColorMode::Solid(color) => Some(color),
                                    _ => None,
                                },
                                _ => None,
                            })
                            .collect();
                        let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
                        output.drop_without_applying_deltas();
                        if age < 10 {
                            assert_eq!(painted, vec![expected; 2], "main {main}, age {age}");
                            assert_eq!(delay, Duration::ZERO, "animate the visible fade");
                        } else {
                            assert!(painted.is_empty());
                        }
                    }
                }
            }
            assert!(marks(&chart, main).marks.borrow().is_empty());
            let output = ctx.run_ui(
                egui::RawInput {
                    time: Some(17.0),
                    ..Default::default()
                },
                |ui| {
                    marks(&chart, main).paint(ui.painter(), |_, _| Some(Pos2::new(50.0, 50.0)));
                },
            );
            let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
            output.drop_without_applying_deltas();
            assert_eq!(
                delay,
                Duration::MAX,
                "expired markers leave no repaint work"
            );
            // A mark recorded while nothing is drawn for the series ages on the same clock.
            // The series is hidden, or for the main series the chart type switches to Renko.
            for renko in [false, true].into_iter().filter(|&renko| main || !renko) {
                let ctx = egui::Context::default();
                let frame = |chart: &mut Chart, time: f64| {
                    crate::widget::test_frames::shapes(&ctx, chart, time, Vec::new());
                };
                frame(&mut chart, 0.0);
                if renko {
                    chart.set_chart_type(crate::model::ChartType::Renko);
                } else {
                    assert!(chart.set_series_visible(id, false));
                }
                let close = if renko { 106.0 } else { 105.0 };
                chart.update(id, Bar::new(time, 100.0, 110.0, 100.0, close, 1.0));
                assert_eq!(marks(&chart, main).marks.borrow().len(), 1);
                frame(&mut chart, 1.0);
                frame(&mut chart, 60.0);
                if renko {
                    chart.set_chart_type(crate::model::ChartType::Candles);
                } else {
                    assert!(chart.set_series_visible(id, true));
                }
                frame(&mut chart, 61.0);
                assert!(
                    marks(&chart, main).marks.borrow().is_empty(),
                    "main {main}, renko {renko}"
                );
            }
        }
    }
}
