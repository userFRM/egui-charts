//! Marks where the close of a series rose or fell between updates.

use super::{Chart, SeriesId};
use crate::data::merge::Origin;
use crate::model::{Bar, BarClock, ChangeMarkers};
use chrono::{DateTime, Utc};
use egui::{Painter, Pos2};
use std::cell::RefCell;
use web_time::Instant;

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
        let marks = self.marks.get_mut();
        marks.retain(|mark| now.duration_since(mark.at) < self.options.duration);
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
                at: now,
            });
        }
    }

    /// Drops the marks, for bars that were replaced.
    pub(super) fn clear(&mut self) {
        self.marks.get_mut().clear();
    }

    /// Paints fading marks placed by `position`, removes expired marks and requests another
    /// frame while a fade is visible.
    pub(super) fn paint(
        &self,
        painter: &Painter,
        position: impl Fn(DateTime<Utc>, f64) -> Option<Pos2>,
    ) {
        let now = Instant::now();
        self.marks.borrow_mut().retain(|mark| {
            let elapsed = now.duration_since(mark.at);
            if elapsed >= self.options.duration {
                return false;
            }
            if let Some(pos) = position(mark.time, mark.close)
                && painter.clip_rect().expand(12.0).contains(pos)
            {
                let color = if mark.up {
                    self.options.up_color
                } else {
                    self.options.down_color
                };
                let opacity = 1.0 - elapsed.as_secs_f64() / self.options.duration.as_secs_f64();
                crate::chart::rendering::render_change_mark(
                    painter,
                    pos,
                    mark.up,
                    color.gamma_multiply(opacity as f32),
                );
                painter.ctx().request_repaint();
            }
            true
        });
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
    /// fade. Updates that leave the close unchanged, new bars, whitespace fills and history mark
    /// nothing. `None`
    /// removes the markers.
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
            for (seconds, close) in [(20, 101.0), (40, 99.0)] {
                let time = slot + TimeDelta::seconds(seconds);
                let bar = Bar::new(time, 100.0, 101.0, 99.0, close, 1.0);
                if main {
                    feed.0.lock().unwrap().as_ref().unwrap().push_bar(bar);
                    chart.pump(&ctx);
                } else {
                    chart.update(id, bar);
                }
                assert_eq!(chart.bars(id).len(), 10);
                assert_eq!(chart.bars(id)[9].time, time);
                let marks = marks(&chart, main);
                assert_eq!(
                    marks.marks.borrow().len(),
                    1,
                    "main {main}, second {seconds}"
                );
                let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    marks.paint(ui.painter(), |marked_time, marked_close| {
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
                assert_eq!(marks.marks.borrow()[0].up, seconds == 20);
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
            chart.update(id, Bar::whitespace(time));
            assert!(marks(&chart, main).marks.borrow().is_empty());
            chart.update(id, Bar::new(time, 100.0, 112.0, 100.0, 105.0, 1.0));
            assert!(marks(&chart, main).marks.borrow().is_empty(), "main {main}");
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
    /// 1,000 seconds every premultiplied channel is halved. A changed close restarts that
    /// lifetime; at its end the mark is removed and idle frames request no further repaint.
    #[test]
    fn change_markers_fade_restart_and_expire_on_each_series() {
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
                    duration: Duration::from_secs(1_000),
                }),
            );
            let time = chart.bars(id).last().unwrap().time;
            let ctx = egui::Context::default();
            for (close, color, half) in [
                (
                    108.0,
                    down,
                    Color32::from_rgba_premultiplied(60, 10, 20, 80),
                ),
                (109.0, up, Color32::from_rgba_premultiplied(10, 80, 20, 100)),
            ] {
                chart.update(id, Bar::new(time, 100.0, 110.0, 100.0, close, 1.0));
                assert_eq!(marks(&chart, main).marks.borrow().len(), 1);
                assert!(
                    marks(&chart, main).marks.borrow()[0].at.elapsed() < Duration::from_secs(1)
                );
                for (age, expected) in [(0, color), (500, half), (1_000, Color32::TRANSPARENT)] {
                    if age == 1_000 && close == 108.0 {
                        // Keep the half-faded mark for the next update to replace.
                        continue;
                    }
                    for mark in marks(&chart, main).marks.borrow_mut().iter_mut() {
                        mark.at = Instant::now() - Duration::from_secs(age);
                    }
                    // egui can carry a repaint request from the preceding frame.
                    for _ in 0..3 {
                        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                            marks(&chart, main)
                                .paint(ui.painter(), |_, _| Some(Pos2::new(50.0, 50.0)));
                        });
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
                        if age < 1_000 {
                            assert_eq!(painted, vec![expected; 2], "main {main}, age {age}");
                            assert_eq!(delay, Duration::ZERO, "animate the visible fade");
                        } else {
                            assert!(painted.is_empty());
                        }
                    }
                }
            }
            assert!(marks(&chart, main).marks.borrow().is_empty());
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                marks(&chart, main).paint(ui.painter(), |_, _| Some(Pos2::new(50.0, 50.0)));
            });
            let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
            output.drop_without_applying_deltas();
            assert_eq!(
                delay,
                Duration::MAX,
                "expired markers leave no repaint work"
            );
        }
    }
}
