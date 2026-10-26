//! Optional analyst estimates, requested independently of bar history.

use super::Chart;
use crate::chart::coords::ChartMapping;
use crate::data::PriceTargets;
use crate::data::{FeedError, inbox::Inbox};
use crate::localization as l10n;
use crate::scales::{DefaultPriceFormatter, PriceFormatter, PriceScaleMode};
use crate::theme::ThemeExt;
use egui::{FontId, Pos2, Sense, Stroke, Vec2};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct TargetState {
    inbox: Arc<Inbox>,
    symbol: Option<String>,
    values: Option<PriceTargets>,
    error: Option<FeedError>,
}

impl Chart {
    /// Show or hide the host's analyst estimates. Hidden by default; hiding retains the answer.
    /// Leave future whitespace in the visible logical range to see the supplied horizon date.
    pub fn set_price_targets_visible(&mut self, visible: bool) {
        self.show_price_targets = visible;
    }

    /// Whether analyst estimates are enabled in chart settings.
    pub fn price_targets_visible(&self) -> bool {
        self.show_price_targets
    }

    /// The accepted targets for the current symbol, once answered by the feed.
    pub fn price_targets(&self) -> Option<&PriceTargets> {
        self.targets.values.as_ref()
    }

    /// Ask again on the next frame, for example after estimates change at the host.
    pub fn reload_price_targets(&mut self) {
        self.targets = TargetState::default();
    }

    /// Take a request or validation error. Unsupported feeds silently leave the chart unchanged.
    pub fn take_price_targets_error(&mut self) -> Option<FeedError> {
        self.targets.error.take()
    }

    pub(super) fn pump_targets(&mut self, ctx: &egui::Context) {
        if !self.show_price_targets {
            return;
        }
        let Some(link) = &self.feed_link else {
            return;
        };
        if self.targets.symbol.as_deref() != Some(&self.symbol) {
            self.targets = TargetState {
                symbol: Some(self.symbol.clone()),
                ..Default::default()
            };
            let repaint = ctx.clone();
            let viewport = ctx.viewport_id();
            self.targets
                .inbox
                .set_waker(move || repaint.request_repaint_of(viewport));
            link.feed
                .price_targets(&self.symbol, self.targets.inbox.reply(0));
        }
        for (_, answer) in self.targets.inbox.drain().answers {
            match *answer
                .downcast::<Result<PriceTargets, FeedError>>()
                .expect("price targets")
            {
                Ok(targets) if valid(&targets) => {
                    self.targets.values = Some(targets);
                    self.targets.error = None;
                }
                Ok(_) => {
                    self.targets.error = Some(FeedError::InvalidData(
                        "Invalid analyst price targets".into(),
                    ))
                }
                Err(FeedError::NotSupported) => {}
                Err(error) => self.targets.error = Some(error),
            }
        }
    }

    fn visible_targets(&self) -> Option<&PriceTargets> {
        (self.show_price_targets
            && self.main_series_visible
            && !self.chart_type.is_time_independent())
        .then_some(())?;
        let targets = self.price_targets()?;
        let last = crate::model::last_with_value(self.state.shown())?;
        let horizon = targets.horizon.and_hms_opt(0, 0, 0)?.and_utc();
        let range = self.visible_time_range()?;
        (horizon > last.time && range.to >= last.time && range.from <= horizon).then_some(targets)
    }

    pub(super) fn target_prices(&self) -> Vec<f64> {
        self.visible_targets()
            .map(|t| vec![t.low, t.high])
            .unwrap_or_default()
    }

    pub(super) fn paint_targets(
        &self,
        ui: &egui::Ui,
        coords: &ChartMapping,
        label_top: f32,
        line_slot: egui::layers::ShapeIdx,
    ) {
        let Some(targets) = self.visible_targets() else {
            return;
        };
        let Some((index, last)) = self
            .state
            .shown()
            .iter()
            .enumerate()
            .rev()
            .find(|(_, bar)| !bar.is_whitespace())
        else {
            return;
        };
        let time = targets
            .horizon
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc();
        if time <= last.time {
            return;
        }
        let Some(horizon) = self.time_to_index(time) else {
            return;
        };
        let origin = Pos2::new(coords.idx_to_x(index), coords.price.price_to_y(last.close));
        let end_x = coords.idx_to_x(0) + horizon as f32 * coords.bar_spacing;
        let theme = ui.active_theme();
        let painter = ui.painter().with_clip_rect(coords.rect);
        let mut levels = [
            ("High", targets.high),
            ("Average", targets.average),
            ("Low", targets.low),
        ];
        levels.sort_by(|a, b| {
            coords
                .price
                .price_to_y(a.1)
                .total_cmp(&coords.price.price_to_y(b.1))
        });
        let mut label_bottom = label_top.max(coords.rect.top());
        let mut lines = Vec::new();
        let formatter = self.price_label_formatter(PriceScaleMode::Normal);
        for (position, (name, price)) in levels.into_iter().enumerate() {
            let end = Pos2::new(end_x, coords.price.price_to_y(price));
            if !end.is_finite() || !origin.is_finite() {
                continue;
            }
            let color = if price > last.close {
                theme.chart.candle_up
            } else if price < last.close {
                theme.chart.candle_down
            } else {
                theme.ui.text_secondary
            };
            lines.push(egui::Shape::line_segment(
                [origin, end],
                Stroke::new(1.5, color),
            ));
            let decimals = self
                .instrument()
                .and_then(|instrument| instrument.price_decimals)
                .map(usize::from)
                .or_else(|| coords.price.price_precision())
                .unwrap_or(2);
            let price_text = formatter.as_ref().map_or_else(
                || DefaultPriceFormatter::with_precision(decimals).format(price),
                |formatter| formatter.format(price),
            );
            let change = percent_change(last.close, price)
                .map(|change| format!(" ({change:+.2}%)"))
                .unwrap_or_default();
            let text = format!("{name} {price_text}{change}");
            let galley = painter.layout_no_wrap(
                l10n::text(ui.ctx(), text),
                FontId::proportional(12.0),
                color,
            );
            let size = galley.size();
            let top = (end.y - size.y / 2.0)
                .min(coords.rect.bottom() - (3 - position) as f32 * (size.y + 4.0))
                .max(label_bottom + 2.0);
            let label = egui::Rect::from_min_size(
                Pos2::new(
                    (end.x + 8.0)
                        .min(coords.rect.right() - size.x - 4.0)
                        .max(coords.rect.left() + 4.0),
                    top,
                ),
                size,
            );
            label_bottom = label.bottom();
            if (label.center().y - end.y).abs() > size.y / 2.0 {
                lines.push(egui::Shape::line_segment(
                    [end, label.left_center()],
                    Stroke::new(1.0, color),
                ));
            }
            painter.rect_filled(label.expand(2.0), 2.0, theme.chart.bg);
            painter.galley(label.min, galley, color);
            let mut hit = label.expand(4.0);
            if coords.rect.contains(end) {
                lines.push(egui::Shape::circle_filled(end, 3.0, color));
                hit = hit.union(egui::Rect::from_center_size(end, Vec2::splat(12.0)));
            }
            ui.interact(
                hit.intersect(coords.rect),
                ui.scope_id().with(("price_target", name)),
                Sense::hover(),
            )
            .on_hover_text(l10n::format_text(
                ui.ctx(),
                "{name}: {price_text}{change}\n{0} analysts\nPublished: {1}\nHorizon: {2}",
                &[
                    ("name", name.to_string()),
                    ("price_text", price_text.to_string()),
                    ("change", change.to_string()),
                    ("0", format!("{}", targets.analysts)),
                    ("1", format!("{}", targets.published)),
                    ("2", format!("{}", targets.horizon)),
                ],
            ));
        }
        painter.set(line_slot, egui::Shape::Vec(lines));
    }
}

fn valid(targets: &PriceTargets) -> bool {
    [targets.high, targets.average, targets.low]
        .into_iter()
        .all(f64::is_finite)
        && targets.low <= targets.average
        && targets.average <= targets.high
        && targets.analysts > 0
        && targets.horizon > targets.published
}

fn percent_change(close: f64, target: f64) -> Option<f64> {
    if close == 0.0 {
        return None;
    }
    let change = (target - close) / close * 100.0;
    change.is_finite().then_some(change)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Datafeed, HistoryPage, HistoryRequest, Reply};
    use crate::widget::SeriesId;
    use crate::widget::test_frames;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Feed(Mutex<Vec<(String, Reply<PriceTargets>)>>);
    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage {
                bars: vec![],
                end_of_history: true,
            }));
        }
        fn price_targets(&self, symbol: &str, reply: Reply<PriceTargets>) {
            self.0.lock().unwrap().push((symbol.into(), reply));
        }
    }
    fn targets() -> PriceTargets {
        let published = chrono::NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        PriceTargets {
            high: 120.0,
            average: 110.0,
            low: 80.0,
            analysts: 12,
            published,
            horizon: published + chrono::TimeDelta::days(365),
        }
    }

    #[test]
    fn target_labels_use_the_last_close_and_clear_on_symbol_changes() {
        let ctx = egui::Context::default();
        let feed = Arc::new(Feed::default());
        let mut chart = crate::widget::ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("A")
            .build();
        chart.set_data(
            SeriesId::MAIN,
            test_frames::chart(&[95.0], &[100.0])
                .state
                .data()
                .bars
                .clone(),
        );
        chart.set_timeframe(crate::model::Timeframe::Day1);
        chart.set_feed_paused(true);
        chart.set_price_targets_visible(true);
        chart.pump_targets(&ctx);
        let (_, old) = feed.0.lock().unwrap().pop().unwrap();
        chart.set_symbol("B");
        old.send(Ok(targets()));
        chart.pump_targets(&ctx);
        assert!(chart.price_targets().is_none(), "stale symbol reply");
        let (symbol, reply) = feed.0.lock().unwrap().pop().unwrap();
        assert_eq!(symbol, "B");
        reply.send(Ok(targets()));
        chart.pump_targets(&ctx);
        assert_eq!(chart.price_targets(), Some(&targets()));
        test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        let horizon = chart
            .time_to_index(targets().horizon.and_hms_opt(0, 0, 0).unwrap().and_utc())
            .unwrap() as f32;
        chart.set_visible_logical_range(crate::model::LogicalRange {
            left: 0.0,
            right: horizon + 200.0,
        });
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
        for label in [
            "High 120.00 (+20.00%)",
            "Average 110.00 (+10.00%)",
            "Low 80.00 (-20.00%)",
        ] {
            assert!(
                shapes
                    .iter()
                    .any(|s| matches!(s, egui::Shape::Text(t) if t.galley.text() == label)),
                "missing {label}"
            );
        }
        chart.set_visible_logical_range(crate::model::LogicalRange::new(0.0, 40.0));
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.101, vec![]);
        assert!(
            !shapes.iter().any(
                |s| matches!(s, egui::Shape::Text(t) if t.galley.text().starts_with("High 120"))
            ),
            "projections must stay to the right of the last bar when viewing older history"
        );
        assert!(
            chart.target_prices().is_empty(),
            "offscreen targets do not widen the price range"
        );
        chart.set_visible_logical_range(crate::model::LogicalRange::new(60.0, 130.0));
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.102, vec![]);
        let label = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Text(t) if t.galley.text().starts_with("High 120") => {
                    Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                }
                _ => None,
            })
            .unwrap();
        ctx.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let mut hover_shapes = Vec::new();
        for time in [0.103, 0.104, 0.105] {
            hover_shapes = test_frames::shapes(
                &ctx,
                &mut chart,
                time,
                vec![egui::Event::PointerMoved(label.center())],
            );
        }
        assert!(
            hover_shapes.iter().any(
                |s| matches!(s, egui::Shape::Text(t) if t.galley.text().contains("12 analysts"))
            ),
            "a visible label retains its tooltip when the horizon is offscreen"
        );
        chart.set_visible_logical_range(crate::model::LogicalRange::new(0.0, horizon + 200.0));
        for custom in [false, true] {
            chart.set_price_formatter(custom.then(|| {
                Arc::new(crate::scales::CustomPriceFormatter::new(|price| {
                    format!("host {price:.2}")
                })) as Arc<dyn PriceFormatter>
            }));
            for mode in [
                crate::scales::PriceScaleMode::Normal,
                crate::scales::PriceScaleMode::Percentage,
                crate::scales::PriceScaleMode::IndexedTo100,
            ] {
                chart.price_scale_mut().options_mut().mode = mode;
                let shapes = test_frames::shapes(&ctx, &mut chart, 0.11, vec![]);
                let label = if custom {
                    "High host 120.00 (+20.00%)"
                } else {
                    "High 120.00 (+20.00%)"
                };
                assert!(
                    shapes
                        .iter()
                        .any(|s| matches!(s, egui::Shape::Text(t) if t.galley.text() == label)),
                    "target prices keep the host format and instrument units on {mode:?} scales"
                );
            }
        }
        chart.set_price_formatter(None);
        chart.price_scale_mut().options_mut().mode = crate::scales::PriceScaleMode::Normal;
        chart.state.time_scale_mut().set_min_bar_spacing(0.01);
        chart.set_size(Some(Vec2::new(260.0, 600.0)));
        chart.set_quote(Some(crate::model::Level1Quote {
            ts: chart.state.shown().last().unwrap().time,
            symbol: "B".into(),
            bid: 99.0,
            bid_size: 1.0,
            ask: 101.0,
            ask_size: 1.0,
        }));
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            chart.set_theme(&ctx, &theme);
            test_frames::frame(&ctx, &mut chart, 0.111, vec![egui::Event::PointerGone]);
            chart.set_visible_logical_range(crate::model::LogicalRange::new(0.0, horizon + 200.0));
            let shapes = test_frames::shapes(&ctx, &mut chart, 0.112, vec![]);
            let quote_bottom = shapes
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Text(text) if ["SELL", "BUY"].contains(&text.galley.text()) => {
                        Some(text.pos.y + text.galley.size().y)
                    }
                    _ => None,
                })
                .reduce(f32::max)
                .unwrap();
            let labels: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Text(text)
                        if ["High 120", "Average 110", "Low 80"]
                            .iter()
                            .any(|prefix| text.galley.text().starts_with(prefix)) =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(labels.len(), 3);
            assert!(
                labels.iter().all(|text| text.pos.y > quote_bottom),
                "target labels stay below quote controls in a narrow chart"
            );
        }
        chart.set_size(None);
        chart.set_quote(None);
        test_frames::frame(&ctx, &mut chart, 0.113, vec![]);
        chart.reload_price_targets();
        chart.pump_targets(&ctx);
        let mut equal = targets();
        equal.high = 100.0;
        equal.average = 100.0;
        equal.low = 100.0;
        feed.0.lock().unwrap().pop().unwrap().1.send(Ok(equal));
        chart.pump_targets(&ctx);
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.12, vec![]);
        let labels: Vec<_> = shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Text(t)
                    if ["High 100.00", "Average 100.00", "Low 100.00"]
                        .iter()
                        .any(|prefix| t.galley.text().starts_with(prefix)) =>
                {
                    Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(labels.len(), 3);
        for (i, label) in labels.iter().enumerate() {
            for other in &labels[i + 1..] {
                assert!(
                    !label.intersects(*other),
                    "equal estimates must retain three readable labels"
                );
            }
        }
        chart.user_action(|chart| chart.set_price_targets_visible(false));
        assert!(chart.undo());
        assert!(chart.price_targets_visible());
        assert!(chart.redo());
        assert!(!chart.price_targets_visible());
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.2, vec![]);
        assert!(
            !shapes.iter().any(|s| matches!(s, egui::Shape::Text(t) if
                ["High 100.00", "Average 100.00", "Low 100.00"].iter().any(|prefix| t.galley.text().starts_with(prefix))))
        );
        assert!(
            chart.price_targets().is_some(),
            "hiding retains the response"
        );
        chart.set_symbol("unsupported");
        chart.set_price_targets_visible(true);
        chart.pump_targets(&ctx);
        feed.0
            .lock()
            .unwrap()
            .pop()
            .unwrap()
            .1
            .send(Err(FeedError::NotSupported));
        chart.pump_targets(&ctx);
        assert!(chart.price_targets().is_none());
        assert!(chart.take_price_targets_error().is_none());
        for (close, target, expected) in [
            (100.0, 120.0, Some(20.0)),
            (100.0, 80.0, Some(-20.0)),
            (-100.0, -80.0, Some(-20.0)),
            (0.0, 10.0, None),
        ] {
            assert_eq!(percent_change(close, target), expected);
        }
    }
}
