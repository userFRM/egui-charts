//! Host analytics, scoped to the chart's symbol and interval.

use super::Chart;
use crate::chart::coords::ChartMapping;
use crate::data::inbox::Inbox;
use crate::model::{Direction, Signal, SignalShape, Timeframe};
use crate::services::{Listener, SignalEvent, Signals};
use crate::theme::ThemeExt;
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::sync::Arc;

pub(super) struct SignalSource {
    source: Arc<dyn Signals>,
    inbox: Arc<Inbox>,
    listener: Option<Listener>,
    scope: Option<(String, Timeframe)>,
    signals: Vec<Signal>,
}

impl Chart {
    /// Connects host analytics. Replacing or removing the service clears its signals and stream.
    pub fn set_signal_source(&mut self, source: Option<Arc<dyn Signals>>) {
        self.signal_source = source.map(|source| SignalSource {
            source,
            inbox: Arc::default(),
            listener: None,
            scope: None,
            signals: Vec::new(),
        });
    }

    /// Hides or restores a category without discarding signals or requesting them again.
    pub fn set_signal_category_visible(&mut self, category: &str, visible: bool) {
        if visible {
            self.hidden_signal_categories.remove(category);
        } else {
            self.hidden_signal_categories.insert(category.into());
        }
    }

    pub(super) fn clear_source_signals(&mut self) {
        if let Some(source) = &mut self.signal_source {
            source.listener = None;
            source.scope = None;
            source.signals.clear();
            source.inbox = Arc::default();
        }
    }

    pub(super) fn pump_signals(&mut self, ctx: &egui::Context) {
        let scope = (self.symbol.clone(), self.timeframe.unwrap_or_default());
        let Some(source) = &mut self.signal_source else {
            return;
        };
        if source.scope.as_ref() != Some(&scope)
            || source.listener.as_ref().is_none_or(|l| !l.is_active())
        {
            source.listener = None;
            source.signals.clear();
            source.inbox = Arc::default();
            let repaint = ctx.clone();
            let viewport = ctx.viewport_id();
            source
                .inbox
                .set_waker(move || repaint.request_repaint_of(viewport));
            let (listener, events) = Listener::new(&source.inbox, 0);
            source.listener = Some(listener);
            source.scope = Some(scope.clone());
            source.source.subscribe(&scope.0, scope.1, events);
        }
        if source.listener.as_ref().is_some_and(|l| !l.is_active()) {
            source.signals.clear();
            ctx.request_repaint();
            return;
        }
        for (_, event) in source.inbox.drain().answers {
            match *event.downcast::<SignalEvent>().expect("signal event") {
                SignalEvent::Upsert(signal) => {
                    if let Some(index) = source.signals.iter().position(|old| old.id == signal.id) {
                        source.signals.remove(index);
                    }
                    if valid(&signal.shape) {
                        source.signals.push(signal);
                    }
                }
                SignalEvent::Remove(id) => source.signals.retain(|signal| signal.id != id),
                SignalEvent::Clear => source.signals.clear(),
            }
        }
    }

    pub(super) fn paint_signals(&self, ui: &mut egui::Ui, coords: &ChartMapping) {
        let Some(source) = &self.signal_source else {
            return;
        };
        let theme = ui.active_theme();
        let painter = ui.painter().with_clip_rect(coords.rect);
        let bars = self.state.shown();
        let x = |time| {
            crate::chart::rendering::candles::price_based::element_index_at(
                &self.state,
                self.chart_type,
                &self.chart_type_params,
                self.older_bars_drawn,
                time,
            )
            .map(|index| index as f64)
            .or_else(|| self.time_to_index(time))
            .map(|index| coords.idx_to_x(0) + index as f32 * coords.bar_spacing)
        };
        for signal in &source.signals {
            if self.hidden_signal_categories.contains(&signal.category) {
                continue;
            }
            let neutral = theme.ui.accent;
            let (hit, label, color) = match signal.shape {
                SignalShape::Marker {
                    time,
                    price,
                    direction,
                } => {
                    let Some(index) =
                        crate::chart::rendering::candles::price_based::element_index_at(
                            &self.state,
                            self.chart_type,
                            &self.chart_type_params,
                            self.older_bars_drawn,
                            time,
                        )
                        .map(|index| index as f64)
                        .or_else(|| self.time_to_index(time).map(f64::floor))
                    else {
                        continue;
                    };
                    if index < 0.0 {
                        continue;
                    }
                    let Some(bar) = bars.get(index as usize) else {
                        continue;
                    };
                    let px = coords.idx_to_x(index as usize);
                    let color = match direction {
                        Direction::Up => theme.chart.candle_up,
                        Direction::Down => theme.chart.candle_down,
                        Direction::Neutral => neutral,
                    };
                    let value = price.unwrap_or(if direction == Direction::Up {
                        bar.low
                    } else {
                        bar.high
                    });
                    if !value.is_finite() {
                        continue;
                    }
                    let y = coords.price.price_to_y(value)
                        + if price.is_some() {
                            0.0
                        } else if direction == Direction::Up {
                            12.0
                        } else {
                            -12.0
                        };
                    let point = Pos2::new(px, y);
                    if !coords.rect.expand(8.0).contains(point) {
                        continue;
                    }
                    let sign = if direction == Direction::Up {
                        1.0
                    } else {
                        -1.0
                    };
                    if direction == Direction::Neutral {
                        painter.circle_filled(point, 4.0, color);
                    } else {
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                point + Vec2::new(0.0, -5.0 * sign),
                                point + Vec2::new(-5.0, 4.0 * sign),
                                point + Vec2::new(5.0, 4.0 * sign),
                            ],
                            color,
                            Stroke::NONE,
                        ));
                    }
                    (
                        Rect::from_center_size(point, Vec2::splat(14.0)),
                        point + Vec2::new(9.0, 0.0),
                        color,
                    )
                }
                SignalShape::Level { price, from } => {
                    let left = match from {
                        Some(time) => {
                            let Some(x) = x(time) else {
                                continue;
                            };
                            x.max(coords.rect.left())
                        }
                        None => coords.rect.left(),
                    };
                    let y = coords.price.price_to_y(price);
                    if left > coords.rect.right() || !coords.rect.y_range().contains(y) {
                        continue;
                    }
                    painter.line_segment(
                        [Pos2::new(left, y), Pos2::new(coords.rect.right(), y)],
                        Stroke::new(1.0, neutral),
                    );
                    (
                        Rect::from_min_max(
                            Pos2::new(left, y - 4.0),
                            Pos2::new(coords.rect.right(), y + 4.0),
                        ),
                        Pos2::new(left + 6.0, y - 9.0),
                        neutral,
                    )
                }
                SignalShape::Zone {
                    from,
                    to,
                    low,
                    high,
                } => {
                    let Some(left) = x(from) else {
                        continue;
                    };
                    let right = match to {
                        Some(time) => {
                            let Some(x) = x(time) else {
                                continue;
                            };
                            x
                        }
                        None => coords.rect.right(),
                    };
                    let rect = Rect::from_two_pos(
                        Pos2::new(left, coords.price.price_to_y(high)),
                        Pos2::new(right, coords.price.price_to_y(low)),
                    )
                    .intersect(coords.rect);
                    if !rect.is_positive() {
                        continue;
                    }
                    painter.rect_filled(rect, 0.0, neutral.linear_multiply(0.12));
                    painter.rect_stroke(
                        rect,
                        0.0,
                        Stroke::new(1.0, neutral),
                        egui::StrokeKind::Inside,
                    );
                    (rect, rect.left_top() + Vec2::splat(6.0), neutral)
                }
            };
            let mut hit = hit;
            if let Some(label_text) = &signal.label {
                let label_rect = painter.text(
                    label,
                    Align2::LEFT_CENTER,
                    label_text,
                    FontId::proportional(11.0),
                    color,
                );
                hit = hit.union(label_rect);
            }
            let response = ui.interact(
                hit.intersect(coords.rect),
                ui.scope_id().with(("host_signal", &signal.id.0)),
                Sense::click(),
            );
            if response.clicked() {
                source.source.clicked(&signal.id);
            }
            if let Some(tooltip) = &signal.tooltip {
                response.on_hover_text(tooltip);
            }
        }
    }

    pub(super) fn show_signal_menu(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let Some(source) = &self.signal_source else {
            return;
        };
        let categories: std::collections::BTreeSet<_> = source
            .signals
            .iter()
            .map(|signal| signal.category.clone())
            .collect();
        if !categories.is_empty() {
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(Rect::from_min_size(
                    rect.right_top() + Vec2::new(-85.0, 8.0),
                    Vec2::new(80.0, 24.0),
                )),
                |ui| {
                    ui.menu_button("Signals", |ui| {
                        for category in categories {
                            let mut visible = !self.hidden_signal_categories.contains(&category);
                            if ui.checkbox(&mut visible, &category).changed() {
                                self.set_signal_category_visible(&category, visible);
                            }
                        }
                    });
                },
            );
        }
    }
}

fn valid(shape: &SignalShape) -> bool {
    match *shape {
        SignalShape::Marker { price, .. } => price.is_none_or(f64::is_finite),
        SignalShape::Level { price, .. } => price.is_finite(),
        SignalShape::Zone {
            from,
            to,
            low,
            high,
        } => low.is_finite() && high.is_finite() && low <= high && to.is_none_or(|to| to >= from),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SignalId;
    use crate::services::Events;
    use crate::widget::test_frames;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Host {
        streams: Mutex<Vec<(String, Timeframe, Events<SignalEvent>)>>,
        clicks: Mutex<Vec<SignalId>>,
    }
    impl Signals for Host {
        fn subscribe(&self, symbol: &str, timeframe: Timeframe, events: Events<SignalEvent>) {
            self.streams
                .lock()
                .unwrap()
                .push((symbol.into(), timeframe, events));
        }
        fn clicked(&self, id: &SignalId) {
            self.clicks.lock().unwrap().push(id.clone());
        }
    }

    #[test]
    fn host_signals_update_render_and_clear_with_the_subscription() {
        let ctx = egui::Context::default();
        let mut chart = test_frames::chart(&[95.0], &[105.0]);
        chart.config_mut().show_volume = false;
        chart.set_timeframe(Timeframe::Day1);
        chart.set_symbol("A");
        let host = Arc::new(Host::default());
        chart.set_signal_source(Some(host.clone()));
        test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        let (symbol, timeframe, stream) = host.streams.lock().unwrap().pop().unwrap();
        assert_eq!((symbol.as_str(), timeframe), ("A", Timeframe::Day1));
        let time = chart.state.shown()[70].time;
        let signal = |shape, name: &str| Signal {
            id: SignalId(name.into()),
            shape,
            category: name.into(),
            label: Some(name.into()),
            tooltip: Some("Host details".into()),
        };
        let marker = signal(
            SignalShape::Marker {
                time,
                price: Some(100.0),
                direction: Direction::Up,
            },
            "Marker",
        );
        for item in [
            marker.clone(),
            signal(
                SignalShape::Level {
                    price: 104.0,
                    from: None,
                },
                "Level",
            ),
            signal(
                SignalShape::Zone {
                    from: time,
                    to: None,
                    low: 96.0,
                    high: 98.0,
                },
                "Zone",
            ),
        ] {
            stream.push(SignalEvent::Upsert(item));
        }
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
        let label_at = |shapes: &[egui::Shape], label: &str| {
            shapes.iter().find_map(|shape| match shape {
                egui::Shape::Text(text) if text.galley.text() == label => Some(text.pos),
                _ => None,
            })
        };
        for label in ["Marker", "Level", "Zone"] {
            assert!(label_at(&shapes, label).is_some(), "missing {label}");
        }
        let expected_x = chart.index_to_x(70.0).unwrap();
        assert!((label_at(&shapes, "Marker").unwrap().x - expected_x - 9.0).abs() < 0.01);
        let point = Pos2::new(expected_x, chart.last_price_map.price_to_y(100.0));
        test_frames::frame(
            &ctx,
            &mut chart,
            0.2,
            vec![egui::Event::PointerMoved(point)],
        );
        for pressed in [true, false] {
            test_frames::frame(
                &ctx,
                &mut chart,
                0.3 + f64::from(!pressed) * 0.1,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
            );
        }
        assert_eq!(
            host.clicks.lock().unwrap().as_slice(),
            std::slice::from_ref(&marker.id)
        );
        chart.set_signal_category_visible("Marker", false);
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.5, vec![]);
        assert!(label_at(&shapes, "Marker").is_none());
        chart.set_signal_category_visible("Marker", true);
        let mut replacement = marker.clone();
        replacement.label = Some("Updated".into());
        stream.push(SignalEvent::Upsert(replacement));
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.6, vec![]);
        assert!(label_at(&shapes, "Marker").is_none());
        assert!(label_at(&shapes, "Updated").is_some());
        let mut bars = chart.state.shown().to_vec();
        let first = bars[0].clone();
        for days in 1..=10 {
            let mut older = first.clone();
            older.time -= chrono::TimeDelta::days(days);
            bars.push(older);
        }
        chart.set_data(crate::widget::SeriesId::MAIN, bars);
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.7, vec![]);
        assert!(
            (label_at(&shapes, "Updated").unwrap().x - chart.index_to_x(80.0).unwrap() - 9.0).abs()
                < 0.01
        );
        stream.push(SignalEvent::Remove(marker.id.clone()));
        test_frames::frame(&ctx, &mut chart, 0.8, vec![]);
        assert_eq!(chart.signal_source.as_ref().unwrap().signals.len(), 2);
        stream.push(SignalEvent::Clear);
        test_frames::frame(&ctx, &mut chart, 0.9, vec![]);
        assert!(chart.signal_source.as_ref().unwrap().signals.is_empty());
        chart.set_timeframe(Timeframe::Hour1);
        assert!(!stream.is_active());
        test_frames::frame(&ctx, &mut chart, 1.0, vec![]);
        let (_, interval, new) = host.streams.lock().unwrap().pop().unwrap();
        assert_eq!(interval, Timeframe::Hour1);
        stream.push(SignalEvent::Upsert(marker.clone()));
        new.push(SignalEvent::Upsert(marker.clone()));
        test_frames::frame(&ctx, &mut chart, 1.1, vec![]);
        assert_eq!(chart.signal_source.as_ref().unwrap().signals.len(), 1);
        chart.set_symbol("B");
        assert!(!new.is_active());
        test_frames::frame(&ctx, &mut chart, 1.2, vec![]);
        let (symbol, _, stream) = host.streams.lock().unwrap().pop().unwrap();
        assert_eq!(symbol, "B");
        assert!(chart.signal_source.as_ref().unwrap().signals.is_empty());
        for _ in 0..10_001 {
            stream.push(SignalEvent::Upsert(marker.clone()));
        }
        test_frames::frame(&ctx, &mut chart, 1.3, vec![]);
        assert!(!stream.is_active());
        assert!(chart.signal_source.as_ref().unwrap().signals.is_empty());
        assert_eq!(
            host.streams.lock().unwrap().len(),
            1,
            "lag starts a new stream"
        );
        chart.set_signal_source(None);
        assert!(chart.signal_source.is_none());
    }
}
