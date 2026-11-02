//! Host analytics, scoped to the chart's symbol and interval.

use super::Chart;
use crate::chart::coords::ChartMapping;
use crate::data::inbox::Inbox;
use crate::localization as l10n;
use crate::model::{Direction, Signal, SignalShape, Timeframe};
use crate::services::{Listener, SignalEvent, Signals};
use crate::theme::ThemeExt;
use egui::{FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::sync::Arc;

pub(super) struct SignalSource {
    source: Arc<dyn Signals>,
    inbox: Arc<Inbox>,
    listener: Option<Listener>,
    scope: Option<(String, Timeframe)>,
    signals: Vec<Signal>,
}

impl SignalSource {
    fn pump(&mut self, ctx: &egui::Context, scope: &(String, Timeframe)) {
        if self.scope.as_ref() != Some(scope)
            || self.listener.as_ref().is_none_or(|l| !l.is_active())
        {
            self.listener = None;
            self.signals.clear();
            self.inbox = Arc::default();
            let repaint = ctx.clone();
            let viewport = ctx.viewport_id();
            self.inbox
                .set_waker(move || repaint.request_repaint_of(viewport));
            let (listener, events) = Listener::new(&self.inbox, 0);
            self.listener = Some(listener);
            self.scope = Some(scope.clone());
            self.source.subscribe(&scope.0, scope.1, events);
        }
        if self.listener.as_ref().is_some_and(|l| !l.is_active()) {
            self.signals.clear();
            ctx.request_repaint();
            return;
        }
        for (_, event) in self.inbox.drain().answers {
            match *event.downcast::<SignalEvent>().expect("signal event") {
                SignalEvent::Upsert(signal) => {
                    if let Some(index) = self.signals.iter().position(|old| old.id == signal.id) {
                        self.signals.remove(index);
                    }
                    if valid(&signal.shape) {
                        self.signals.push(signal);
                    }
                }
                SignalEvent::Remove(id) => self.signals.retain(|signal| signal.id != id),
                SignalEvent::Clear => self.signals.clear(),
            }
        }
    }
}

impl Chart {
    /// Connects host analytics. Replacing or removing the service clears its signals and stream.
    pub fn set_signal_source(&mut self, source: Option<Arc<dyn Signals>>) {
        self.set_signal_sources(source.into_iter().collect());
    }

    /// Connects several analytics services, each with its own stream and signals, so that one
    /// source's `Clear` or lagged stream leaves the others' signals in place. Replacing the
    /// sources clears their signals and streams.
    pub fn set_signal_sources(&mut self, sources: Vec<Arc<dyn Signals>>) {
        self.signal_sources = sources
            .into_iter()
            .map(|source| SignalSource {
                source,
                inbox: Arc::default(),
                listener: None,
                scope: None,
                signals: Vec::new(),
            })
            .collect();
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
        for source in &mut self.signal_sources {
            source.listener = None;
            source.scope = None;
            source.signals.clear();
            source.inbox = Arc::default();
        }
    }

    pub(super) fn pump_signals(&mut self, ctx: &egui::Context) {
        let scope = (self.symbol.clone(), self.timeframe.unwrap_or_default());
        for source in &mut self.signal_sources {
            source.pump(ctx, &scope);
        }
    }

    pub(super) fn paint_signals(&self, ui: &mut egui::Ui, coords: &ChartMapping) {
        if self
            .signal_sources
            .iter()
            .all(|source| source.signals.is_empty())
        {
            return;
        }
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
        let mut occupied = Vec::new();
        let half_width = (coords.bar_spacing * self.config.candle_width)
            .max(self.config.wick_width)
            .max(1.0)
            / 2.0;
        if self.main_series_visible {
            let end = coords
                .x_to_idx(coords.rect.right())
                .saturating_add(1)
                .min(bars.len());
            let mut reserve = |left, right, bar: &crate::model::Bar| {
                if !bar.is_whitespace() {
                    occupied.push(Rect::from_two_pos(
                        Pos2::new(left, coords.price.price_to_y(bar.high)),
                        Pos2::new(right, coords.price.price_to_y(bar.low)),
                    ));
                }
            };
            let factor = self.merge_groups.factor();
            if factor > 1 {
                for index in self.merge_groups.covering(coords.start_idx..end) {
                    let first = index * factor;
                    let last = (first + factor).min(bars.len()) - 1;
                    let left = coords.idx_to_x(first);
                    let right = coords.idx_to_x(last);
                    let center = (left + right) / 2.0;
                    let half_group = ((right - left).abs() / 2.0 + half_width)
                        .max(coords.bar_spacing * factor as f32 * self.config.candle_width / 2.0);
                    reserve(
                        center - half_group,
                        center + half_group,
                        &self.merge_groups.groups[index].bar,
                    );
                }
            } else {
                for (index, bar) in bars.iter().enumerate().take(end).skip(coords.start_idx) {
                    let x = coords.idx_to_x(index);
                    reserve(x - half_width, x + half_width, bar);
                }
            }
        }
        let price_obstacles = occupied.len();
        let mut visible = Vec::new();
        // Zones stay behind their markers, including hit targets after a zone update.
        let signals = [false, true].into_iter().flat_map(|markers| {
            self.signal_sources
                .iter()
                .enumerate()
                .flat_map(move |(index, source)| {
                    source
                        .signals
                        .iter()
                        .filter(move |signal| {
                            matches!(signal.shape, SignalShape::Marker { .. }) == markers
                        })
                        .map(move |signal| (index, source, signal))
                })
        });
        for (index, source, signal) in signals {
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
                    let px = coords.idx_to_x(0) + index as f32 * coords.bar_spacing;
                    let color = match direction {
                        Direction::Up => theme.chart.candle_up,
                        Direction::Down => theme.chart.candle_down,
                        Direction::Neutral => neutral,
                    };
                    let value = if let Some(price) = price {
                        price
                    } else {
                        if index < 0.0 {
                            continue;
                        }
                        let Some(bar) = bars.get(index as usize) else {
                            continue;
                        };
                        if direction == Direction::Up {
                            bar.low
                        } else {
                            bar.high
                        }
                    };
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
                        painter.circle(point, 4.0, color, Stroke::new(1.0, theme.chart.bg));
                    } else {
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                point + Vec2::new(0.0, -5.0 * sign),
                                point + Vec2::new(-5.0, 4.0 * sign),
                                point + Vec2::new(5.0, 4.0 * sign),
                            ],
                            color,
                            Stroke::new(1.0, theme.chart.bg),
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
                        Pos2::new(left + 6.0, y - 11.0),
                        neutral,
                    )
                }
                SignalShape::PriceZone {
                    from,
                    to,
                    price,
                    direction,
                } => {
                    let Some(left) = x(from) else {
                        continue;
                    };
                    let right = match to {
                        Some(time) => {
                            let Some(right) = x(time) else {
                                continue;
                            };
                            right
                        }
                        None => coords.rect.right(),
                    };
                    let y = coords.price.price_to_y(price);
                    if right < coords.rect.left()
                        || left > coords.rect.right()
                        || !coords.rect.y_range().contains(y)
                    {
                        continue;
                    }
                    let color = match direction {
                        Direction::Up => theme.chart.candle_up,
                        Direction::Down => theme.chart.candle_down,
                        Direction::Neutral => neutral,
                    };
                    let left = left.max(coords.rect.left());
                    let right = right.max(left + 1.0).min(coords.rect.right());
                    painter.line_segment(
                        [Pos2::new(left, y), Pos2::new(right, y)],
                        Stroke::new(2.0, color),
                    );
                    (
                        Rect::from_min_max(Pos2::new(left, y - 4.0), Pos2::new(right, y + 4.0)),
                        Pos2::new(left + 6.0, y - 11.0),
                        color,
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
                    (rect, rect.left_top() + Vec2::new(6.0, 13.0), neutral)
                }
            };
            match signal.shape {
                SignalShape::Marker { .. } => occupied.push(hit.shrink(2.0)),
                SignalShape::Level { .. } | SignalShape::PriceZone { .. } => {
                    occupied.push(Rect::from_center_size(
                        hit.center(),
                        Vec2::new(hit.width(), 2.0),
                    ));
                }
                SignalShape::Zone { .. } => {
                    for [a, b] in [
                        [hit.left_top(), hit.right_top()],
                        [hit.right_top(), hit.right_bottom()],
                        [hit.right_bottom(), hit.left_bottom()],
                        [hit.left_bottom(), hit.left_top()],
                    ] {
                        occupied.push(Rect::from_two_pos(a, b).expand(0.5));
                    }
                }
            }
            visible.push((index, source, signal, hit, label, color, None));
        }
        // Recent marker captions take space before older captions and zone names.
        for markers in [true, false] {
            for (_, _, signal, hit, anchor, color, caption) in visible.iter_mut().rev() {
                if matches!(signal.shape, SignalShape::Marker { .. }) != markers {
                    continue;
                }
                let Some(text) = &signal.label else { continue };
                let galley = painter.layout_no_wrap(
                    l10n::text(ui.ctx(), text),
                    FontId::proportional(11.0),
                    *color,
                );
                let pos = *anchor - Vec2::new(0.0, galley.size().y / 2.0);
                let bounds = galley.mesh_bounds.translate(pos.to_vec2());
                let beside_body = if markers
                    && occupied[..price_obstacles]
                        .iter()
                        .any(|obstacle| obstacle.intersects(bounds.expand(2.0)))
                {
                    (half_width - 5.0).max(0.0)
                } else {
                    0.0
                };
                for offset in [0.0, beside_body] {
                    let pos = pos + Vec2::new(offset, 0.0);
                    let bounds = bounds.translate(Vec2::new(offset, 0.0));
                    let padded = bounds.expand(2.0);
                    if coords.rect.contains_rect(padded)
                        && !occupied.iter().any(|obstacle| obstacle.intersects(padded))
                    {
                        occupied.push(padded);
                        *hit = hit.union(bounds);
                        *caption = Some((pos, galley));
                        break;
                    }
                }
            }
        }
        let pointer_layer = ui
            .input(|input| input.pointer.hover_pos())
            .and_then(|point| ui.ctx().layer_id_at(point));
        for (index, source, signal, hit, anchor, color, caption) in &visible {
            if let Some((pos, galley)) = caption {
                painter.galley(*pos, galley.clone(), *color);
            }
            let response = ui.interact(
                hit.intersect(coords.rect),
                ui.scope_id().with(("host_signal", *index, &signal.id.0)),
                Sense::click(),
            );
            if response.clicked() {
                source.source.clicked(&signal.id);
            }
            // Wheel input normally dismisses tooltips; retain the active list under the pointer.
            let inside_tooltip = response.is_tooltip_open()
                && pointer_layer
                    == Some(egui::LayerId::new(
                        egui::Order::Tooltip,
                        egui::Tooltip::next_tooltip_id(ui.ctx(), response.id),
                    ));
            if response.enabled()
                && (inside_tooltip || egui::Tooltip::should_show_tooltip(&response, true))
            {
                let marker = matches!(signal.shape, SignalShape::Marker { .. });
                let compact =
                    Rect::from_center_size(*anchor - Vec2::new(9.0, 0.0), Vec2::splat(14.0));
                let details: Vec<_> = visible
                    .iter()
                    .filter(|(_, _, other, other_hit, ..)| {
                        if marker {
                            other_hit.intersects(compact)
                        } else {
                            std::ptr::eq(*other, *signal)
                        }
                    })
                    .flat_map(|(_, _, other, ..)| {
                        other
                            .label
                            .iter()
                            .chain(other.tooltip.iter())
                            .map(|text| l10n::text(ui.ctx(), text))
                    })
                    .collect();
                if !details.is_empty() {
                    egui::Tooltip::for_widget(&response).show(|ui| {
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(ui, |ui| {
                                for text in &details {
                                    ui.label(text);
                                }
                            });
                    });
                }
            }
        }
    }

    pub(super) fn show_signal_menu(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let categories: std::collections::BTreeSet<_> = self
            .signal_sources
            .iter()
            .flat_map(|source| &source.signals)
            .map(|signal| signal.category.clone())
            .collect();
        if !categories.is_empty() {
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(Rect::from_min_size(
                    rect.right_top() + Vec2::new(-85.0, 8.0),
                    Vec2::new(80.0, 24.0),
                )),
                |ui| {
                    ui.menu_button(l10n::text(ui.ctx(), "Signals"), |ui| {
                        for category in categories {
                            let mut visible = !self.hidden_signal_categories.contains(&category);
                            if ui
                                .checkbox(&mut visible, l10n::text(ui.ctx(), &category))
                                .changed()
                            {
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
        SignalShape::PriceZone {
            from, to, price, ..
        } => price.is_finite() && to.is_none_or(|to| to >= from),
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
    fn dense_captions_leave_prices_and_signals_clear_without_losing_hover_details() {
        for size in [Vec2::new(800.0, 600.0), Vec2::new(1440.0, 900.0)] {
            for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
                for visible in [390, 12] {
                    let ctx = egui::Context::default();
                    let mut chart = test_frames::chart(&[40.0], &[60.0]);
                    chart.set_symbol("A");
                    chart.set_timeframe(Timeframe::Day1);
                    let first = chart.state.shown()[0].clone();
                    let bars: Vec<_> = (0..390)
                        .map(|i| {
                            let mut bar = first.clone();
                            bar.time += chrono::TimeDelta::days(i);
                            bar.open = 45.0;
                            bar.close = 55.0;
                            bar
                        })
                        .collect();
                    chart.set_data(crate::widget::SeriesId::MAIN, bars.clone());
                    let hosts = [Arc::new(Host::default()), Arc::new(Host::default())];
                    chart.set_signal_sources(hosts.iter().map(|host| host.clone() as _).collect());
                    chart.pump_signals(&ctx);
                    for (i, bar) in bars.iter().enumerate() {
                        for (host, name, price) in [
                            (&hosts[0], "First", 82.0),
                            (&hosts[1], "Second", 82.0),
                            (&hosts[0], "Body", 50.0),
                            (&hosts[0], "Border", 72.0),
                        ] {
                            host.streams.lock().unwrap()[0]
                                .2
                                .push(SignalEvent::Upsert(Signal {
                                    id: SignalId(format!("{name}-{i}")),
                                    shape: SignalShape::Marker {
                                        time: bar.time,
                                        price: Some(price),
                                        direction: Direction::Neutral,
                                    },
                                    category: "Executions".into(),
                                    label: Some(format!("{name} {i}")),
                                    tooltip: (name != "Second")
                                        .then(|| format!("{name} details {i}: 500 lots")),
                                }));
                        }
                    }
                    for (label, price) in [("1", 96.0), ("2", 97.0)] {
                        hosts[0].streams.lock().unwrap()[0]
                            .2
                            .push(SignalEvent::Upsert(Signal {
                                id: SignalId(label.into()),
                                shape: SignalShape::Marker {
                                    time: bars[380].time,
                                    price: Some(price),
                                    direction: Direction::Neutral,
                                },
                                category: "Executions".into(),
                                label: Some(label.into()),
                                tooltip: None,
                            }));
                    }
                    hosts[0].streams.lock().unwrap()[0]
                        .2
                        .push(SignalEvent::Upsert(Signal {
                            id: SignalId("short-body-caption".into()),
                            shape: SignalShape::Marker {
                                time: bars[382].time,
                                price: Some(50.0),
                                direction: Direction::Neutral,
                            },
                            category: "Executions".into(),
                            label: Some("5".into()),
                            tooltip: None,
                        }));
                    // Insert the zone last: its borders must still reserve space before captions.
                    hosts[0].streams.lock().unwrap()[0]
                        .2
                        .push(SignalEvent::Upsert(Signal {
                            id: SignalId("zone".into()),
                            shape: SignalShape::Zone {
                                from: bars[0].time,
                                to: None,
                                low: 72.0,
                                high: 90.0,
                            },
                            category: "Zones".into(),
                            label: Some("Zone caption".into()),
                            tooltip: Some("Zone details".into()),
                        }));
                    chart.pump_signals(&ctx);
                    let rect = Rect::from_min_size(Pos2::ZERO, size);
                    let coords = ChartMapping::new(
                        rect,
                        size.x / visible as f32,
                        390 - visible,
                        389,
                        0.0,
                        crate::scales::PriceMap::new(
                            crate::scales::PriceTransform::default(),
                            crate::scales::PriceRange::new(0.0, 100.0),
                            0.0,
                            size.y,
                            false,
                        ),
                    );
                    let candle_width = chart.config.candle_width;
                    let mut frame = |time, events| {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(rect),
                                time: Some(time),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                chart.set_theme(ui.ctx(), &theme);
                                chart.paint_signals(ui, &coords);
                            },
                        );
                        let shapes = std::mem::take(&mut output.shapes);
                        output.drop_without_applying_deltas();
                        shapes
                    };
                    let shapes = frame(0.0, vec![]);
                    let captions: Vec<_> = shapes
                        .iter()
                        .filter_map(|clipped| match &clipped.shape {
                            egui::Shape::Text(text) => {
                                Some((text.galley.text(), text.visual_bounding_rect()))
                            }
                            _ => None,
                        })
                        .collect();
                    assert!(captions.iter().any(|(text, _)| *text == "2"));
                    assert!(
                        !captions.iter().any(|(text, _)| *text == "1"),
                        "the recent caption has priority"
                    );
                    assert!(
                        captions.len() < visible * 4,
                        "dense captions are suppressed"
                    );
                    assert_eq!(
                        captions.iter().any(|(text, _)| *text == "5"),
                        visible == 12,
                        "zooming in restores a caption beside the candle body"
                    );
                    let markers = shapes
                        .iter()
                        .filter(|clipped| matches!(clipped.shape, egui::Shape::Circle(_)))
                        .count();
                    assert_eq!(markers, visible * 4 + 3, "every compact marker remains");
                    for (i, (name, bounds)) in captions.iter().enumerate() {
                        assert!(rect.contains_rect(*bounds), "clipped {name}");
                        assert!(!name.starts_with("Border"), "obstructed {name}");
                        for (_, other) in &captions[..i] {
                            assert!(
                                !bounds.expand(2.0).intersects(*other),
                                "overlapping captions"
                            );
                        }
                        // Prices 40..60 occupy y = 0.4h..0.6h; the zone edges are at 0.1h and 0.28h.
                        for index in 390 - visible..390 {
                            let x = coords.idx_to_x(index);
                            let half_body = size.x / visible as f32 * candle_width / 2.0;
                            assert!(!bounds.intersects(Rect::from_min_max(
                                Pos2::new(x - half_body, size.y * 0.4),
                                Pos2::new(x + half_body, size.y * 0.6)
                            )));
                        }
                        for y in [size.y * 0.1, size.y * 0.28] {
                            assert!(!(bounds.top() <= y && bounds.bottom() >= y));
                        }
                    }
                    let point = Pos2::new(coords.idx_to_x(388), size.y * 0.18);
                    frame(0.1, vec![egui::Event::PointerMoved(point)]);
                    frame(1.0, vec![]);
                    let hovered = frame(1.1, vec![]);
                    let details = hovered
                        .iter()
                        .filter_map(|clipped| match &clipped.shape {
                            egui::Shape::Text(text) => Some(text.galley.text()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    assert!(details.contains("First details 388: 500 lots"));
                    if size.x == 800.0 && visible == 390 {
                        let inside = hovered
                            .iter()
                            .find_map(|clipped| match &clipped.shape {
                                egui::Shape::Text(text) if text.galley.text() == "Zone details" => {
                                    Some(text.visual_bounding_rect().center())
                                }
                                _ => None,
                            })
                            .expect("tooltip entry");
                        frame(1.2, vec![egui::Event::PointerMoved(inside)]);
                        frame(
                            1.3,
                            vec![egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: Vec2::new(0.0, -1000.0),
                                phase: egui::TouchPhase::Move,
                                source: egui::MouseWheelSource::Unknown,
                                modifiers: Default::default(),
                            }],
                        );
                        let scrolled = frame(1.4, vec![]);
                        assert!(
                            scrolled.iter().any(|clipped| matches!(&clipped.shape,
                            egui::Shape::Text(text) if text.galley.text() == "Second 388")),
                            "moving into the tooltip keeps its events available to scroll: inside={inside:?}, texts={:?}",
                            scrolled
                                .iter()
                                .filter_map(|clipped| match &clipped.shape {
                                    egui::Shape::Text(text) => Some(text.galley.text()),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                        );
                    } else {
                        assert!(
                            details.contains("Second 388"),
                            "a caption without a tooltip remains discoverable"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn host_signals_update_render_and_clear_with_the_subscription() {
        let ctx = egui::Context::default();
        let mut chart = test_frames::chart(&[95.0], &[105.0]);
        chart.config_mut().show_volume = false;
        chart.set_main_series_visible(false);
        chart.price_scale_mut().set_range(94.0, 106.0);
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
        let future = signal(
            SignalShape::Marker {
                time: chart.state.shown().last().unwrap().time + chrono::TimeDelta::days(5),
                price: Some(100.0),
                direction: Direction::Neutral,
            },
            "Future",
        );
        chart.set_visible_logical_range(crate::model::LogicalRange::new(40.0, 120.0));
        stream.push(SignalEvent::Upsert(future.clone()));
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.61, vec![]);
        assert!(
            (label_at(&shapes, "Future")
                .expect("priced marker in future whitespace")
                .x
                - chart.index_to_x(104.0).unwrap()
                - 9.0)
                .abs()
                < 0.01
        );
        stream.push(SignalEvent::Remove(future.id));
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
        assert_eq!(chart.signal_sources[0].signals.len(), 2);
        stream.push(SignalEvent::Clear);
        test_frames::frame(&ctx, &mut chart, 0.9, vec![]);
        assert!(chart.signal_sources[0].signals.is_empty());
        chart.set_timeframe(Timeframe::Hour1);
        assert!(!stream.is_active());
        test_frames::frame(&ctx, &mut chart, 1.0, vec![]);
        let (_, interval, new) = host.streams.lock().unwrap().pop().unwrap();
        assert_eq!(interval, Timeframe::Hour1);
        stream.push(SignalEvent::Upsert(marker.clone()));
        new.push(SignalEvent::Upsert(marker.clone()));
        test_frames::frame(&ctx, &mut chart, 1.1, vec![]);
        assert_eq!(chart.signal_sources[0].signals.len(), 1);
        chart.set_symbol("B");
        assert!(!new.is_active());
        test_frames::frame(&ctx, &mut chart, 1.2, vec![]);
        let (symbol, _, stream) = host.streams.lock().unwrap().pop().unwrap();
        assert_eq!(symbol, "B");
        assert!(chart.signal_sources[0].signals.is_empty());
        for _ in 0..10_001 {
            stream.push(SignalEvent::Upsert(marker.clone()));
        }
        test_frames::frame(&ctx, &mut chart, 1.3, vec![]);
        assert!(!stream.is_active());
        assert!(chart.signal_sources[0].signals.is_empty());
        assert_eq!(
            host.streams.lock().unwrap().len(),
            1,
            "lag starts a new stream"
        );
        chart.set_signal_source(None);
        assert!(chart.signal_sources.is_empty());
    }

    #[test]
    fn a_clear_from_one_source_leaves_the_signals_of_the_others() {
        let ctx = egui::Context::default();
        let mut chart = test_frames::chart(&[95.0], &[105.0]);
        chart.config_mut().show_volume = false;
        chart.set_main_series_visible(false);
        chart.price_scale_mut().set_range(94.0, 106.0);
        chart.set_timeframe(Timeframe::Day1);
        chart.set_symbol("A");
        let hosts = [Arc::new(Host::default()), Arc::new(Host::default())];
        chart.set_signal_sources(hosts.iter().map(|host| host.clone() as _).collect());
        test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        // Both sources use the same identity: it belongs to each subscription.
        for (i, (host, label)) in hosts.iter().zip(["From A", "From B"]).enumerate() {
            host.streams.lock().unwrap()[0]
                .2
                .push(SignalEvent::Upsert(Signal {
                    id: SignalId("level".into()),
                    shape: SignalShape::Level {
                        price: 100.0 + i as f64 * 2.0,
                        from: None,
                    },
                    category: "Levels".into(),
                    label: Some(label.into()),
                    tooltip: None,
                }));
        }
        let labels = |chart: &mut Chart, time| {
            test_frames::shapes(&ctx, chart, time, vec![])
                .into_iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Text(text) if text.galley.text().starts_with("From") => {
                        Some(text.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(labels(&mut chart, 0.1), ["From A", "From B"]);
        hosts[0].streams.lock().unwrap()[0]
            .2
            .push(SignalEvent::Clear);
        assert_eq!(labels(&mut chart, 0.2), ["From B"]);
    }
    #[test]
    fn one_price_zones_render_with_their_side_even_with_no_duration() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (direction, color) in [
                (Direction::Up, theme.chart.candle_up),
                (Direction::Down, theme.chart.candle_down),
            ] {
                let ctx = egui::Context::default();
                let mut chart = test_frames::chart(&[95.0], &[105.0]);
                chart.set_theme_override(Some(theme.clone()));
                chart.set_symbol("A");
                let host = Arc::new(Host::default());
                chart.set_signal_source(Some(host.clone()));
                test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
                let stream = host.streams.lock().unwrap()[0].2.clone();
                let time = chart.state.shown()[70].time;
                let zone = Signal {
                    id: SignalId("price-zone".into()),
                    shape: SignalShape::PriceZone {
                        from: time,
                        to: Some(time),
                        price: 100.0,
                        direction,
                    },
                    category: "Depth".into(),
                    label: Some("Held level".into()),
                    tooltip: Some("Absorbed 10".into()),
                };
                stream.push(SignalEvent::Upsert(zone.clone()));
                let shapes = test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
                let x = chart.index_to_x(70.0).unwrap();
                let y = chart.last_price_map.price_to_y(100.0);
                let zone_visible = |shapes: &[egui::Shape]| {
                    shapes.iter().any(|shape| matches!(shape,
                    egui::Shape::LineSegment { points, stroke }
                    if (points[0].x - x).abs() < 0.01 && (points[0].y - y).abs() < 0.01
                    && (points[1].x - points[0].x - 1.0).abs() < 0.01 && stroke.color == color && stroke.width == 2.0))
                };
                assert!(zone_visible(&shapes));
                stream.push(SignalEvent::Upsert(Signal {
                    id: SignalId("wall".into()),
                    shape: SignalShape::Marker {
                        time,
                        price: Some(100.0),
                        direction,
                    },
                    category: "Depth".into(),
                    label: None,
                    tooltip: Some("Wall executions".into()),
                }));
                // A later zone update must leave the coincident marker available on hover.
                stream.push(SignalEvent::Upsert(zone));
                test_frames::frame(
                    &ctx,
                    &mut chart,
                    0.2,
                    vec![egui::Event::PointerMoved(Pos2::new(x, y))],
                );
                test_frames::frame(&ctx, &mut chart, 1.0, vec![]);
                let hovered = test_frames::shapes(&ctx, &mut chart, 1.1, vec![]);
                assert!(hovered.iter().any(|shape| matches!(shape, egui::Shape::Path(marker)
                    if marker.points.len() == 3 && marker.fill == color
                    && marker.stroke == egui::epaint::PathStroke::from(Stroke::new(1.0, theme.chart.bg)))));
                assert!(
                    hovered
                        .iter()
                        .any(|shape| matches!(shape, egui::Shape::Text(text)
                    if text.galley.text() == "Wall executions"))
                );
                chart.set_signal_category_visible("Depth", false);
                let hidden = test_frames::shapes(&ctx, &mut chart, 1.2, vec![]);
                assert!(!zone_visible(&hidden));
                assert!(!valid(&SignalShape::PriceZone {
                    from: time,
                    to: Some(time - chrono::TimeDelta::seconds(1)),
                    price: 100.0,
                    direction
                }));
                assert!(!valid(&SignalShape::PriceZone {
                    from: time,
                    to: None,
                    price: f64::NAN,
                    direction
                }));
            }
        }
    }
}
