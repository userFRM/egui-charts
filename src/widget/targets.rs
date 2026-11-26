//! Optional analyst estimates, requested independently of bar history.

use super::Chart;
use crate::chart::coords::ChartMapping;
use crate::data::PriceTargets;
use crate::data::{FeedError, inbox::Inbox};
use crate::localization as l10n;
use crate::scales::{DefaultPriceFormatter, PriceFormatter, PriceScaleMode};
#[cfg(feature = "studies")]
use crate::studies::Indicator;
use crate::theme::ThemeExt;
use egui::{Color32, FontId, Pos2, Sense, Stroke, Vec2};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct TargetState {
    inbox: Arc<Inbox>,
    symbol: Option<String>,
    /// The conversion `symbol` was asked in.
    conversion: Option<String>,
    values: Option<PriceTargets>,
    error: Option<FeedError>,
}

impl Chart {
    /// Show or hide the host's analyst estimates. Hidden by default; hiding retains the answer.
    /// Leave future whitespace in the visible logical range to see the supplied horizon date.
    /// A replay draws them only once its time has passed their publication date in UTC.
    /// They are drawn as dotted lines from the close on their publication date, in the colours of
    /// [`ChartConfig::price_target_positive_color`](crate::config::ChartConfig::price_target_positive_color)
    /// and [`price_target_negative_color`](crate::config::ChartConfig::price_target_negative_color).
    ///
    /// With the `studies` feature, the targets are the Analyst price forecast study's, which the
    /// study catalogue offers: showing them adds that study unless the chart holds one, and
    /// hiding them removes every such study. Inside a [user action](Self::user_action), showing
    /// them at the [study cap](Self::set_study_limit) leaves the setting off unless a forecast
    /// study is already on the chart, as the volume setting does. Host calls outside user
    /// actions, restores and undo/redo stay uncapped. Adding the study turns this setting on
    /// and removing the last one turns it off. The chart draws the targets while the study is
    /// shown in the main series' pane, so hiding the study, or every study, hides them and keeps
    /// this setting on.
    ///
    /// ```rust
    /// use egui::Color32;
    /// use egui_charts::config::ChartConfig;
    /// use egui_charts::widget::ChartBuilder;
    ///
    /// let mut config = ChartConfig::default();
    /// // Blue lines and labels above the publication close, with a fill at 20% opacity.
    /// config.price_target_positive_color = Color32::from_rgba_unmultiplied(0, 0, 255, 51);
    /// let mut chart = ChartBuilder::new().with_config(config).build();
    /// chart.set_price_targets_visible(true);
    /// ```
    pub fn set_price_targets_visible(&mut self, visible: bool) {
        self.show_price_targets = visible;
        #[cfg(feature = "studies")]
        {
            let held: Vec<_> = self
                .indicators()
                .filter(|(_, study)| super::studies::is_price_target_study(*study))
                .map(|(id, _)| id)
                .collect();
            if visible && held.is_empty() {
                if self.history.recording() && self.study_limit_reached() {
                    self.show_price_targets = false;
                    return;
                }
                self.add_indicator(Box::new(crate::studies::AnalystPriceForecast::default()));
            } else if !visible {
                for id in held {
                    self.remove_series(id);
                }
            }
        }
    }

    /// Whether analyst estimates are enabled in chart settings; with the `studies` feature,
    /// whether the chart holds an Analyst price forecast study, shown or hidden.
    pub fn price_targets_visible(&self) -> bool {
        self.show_price_targets
    }

    /// The accepted targets for the current symbol and conversion, once answered by the feed.
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
        if self.targets.symbol.as_deref() != Some(&self.symbol)
            || self.targets.conversion != self.conversion
        {
            self.targets = TargetState {
                symbol: Some(self.symbol.clone()),
                conversion: self.conversion.clone(),
                ..Default::default()
            };
            let repaint = ctx.clone();
            let viewport = ctx.viewport_id();
            self.targets
                .inbox
                .set_waker(move || repaint.request_repaint_of(viewport));
            let (symbol, reply) = (&self.symbol, self.targets.inbox.reply(0));
            match self.conversion.as_deref() {
                Some(conversion) => link.feed.price_targets_converted(symbol, conversion, reply),
                None => link.feed.price_targets(symbol, reply),
            }
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
        // A replay knows the estimates once its exclusive cutoff has passed their publication day.
        if self
            .replay_time()
            .is_some_and(|cutoff| targets.published >= cutoff.date_naive())
        {
            return None;
        }
        let last = crate::model::last_with_value(self.state.shown())?;
        let horizon = targets.horizon.and_hms_opt(0, 0, 0)?.and_utc();
        let range = self.visible_time_range()?;
        (horizon > last.time && range.to >= last.time && range.from <= horizon).then_some(targets)
    }

    /// The prices of the levels [`paint_targets`](Self::paint_targets) draws, for fitting.
    pub(super) fn target_prices(&self) -> Vec<f64> {
        let (Some(targets), Some(sides)) = (self.visible_targets(), self.target_sides()) else {
            return Vec::new();
        };
        let Some((_, reference)) = publication_bar(self.state.shown(), targets.published) else {
            return Vec::new();
        };
        let shown = sides.map(|(_, shown)| shown);
        [targets.high, targets.average, targets.low]
            .into_iter()
            .filter(|price| level_shown(shown, reference.close, *price))
            .collect()
    }

    /// Each side's colour choice and whether it is drawn, positive then negative, or `None` when
    /// no Analyst price forecast study is shown in the main series' pane.
    fn target_sides(&self) -> Option<[(Color32, bool); 2]> {
        #[cfg(feature = "studies")]
        {
            let main = self.series_pane(super::SeriesId::MAIN);
            let (_, study) = self.indicators().find(|(id, study)| {
                super::studies::is_price_target_study(*study)
                    && self.shows_study(*study)
                    && self.series_pane(*id) == main
            })?;
            Some([0, 1].map(|side| {
                let plot = study.plot_style(side);
                (plot.color, plot.visible)
            }))
        }
        #[cfg(not(feature = "studies"))]
        {
            Some([
                (self.config.price_target_positive_color, true),
                (self.config.price_target_negative_color, true),
            ])
        }
    }

    /// Gives the Analyst price forecast studies the configured colours, resolved in the chart's
    /// theme, where those changed since the studies last had them. A colour that only a study
    /// changed, as its settings preview does, stays until the configuration changes.
    #[cfg(feature = "studies")]
    pub(super) fn sync_price_target_studies(&mut self) {
        let Some(theme) = &self.inherited_theme else {
            return;
        };
        let themed = [theme.chart.candle_up, theme.chart.candle_down];
        let config = [
            self.config.price_target_positive_color,
            self.config.price_target_negative_color,
        ];
        let shown = [0, 1].map(|side| {
            crate::config::ChartConfig::price_target_study_color(config[side], themed[side])
        });
        if self.price_target_colors == Some(shown) {
            return;
        }
        let memo = self.price_target_colors.replace(shown);
        let mut written = false;
        for study in self.studies.indicators_mut() {
            if !super::studies::is_price_target_study(study.as_ref()) {
                continue;
            }
            for side in 0..2 {
                if memo.is_none_or(|memo| memo[side] != shown[side]) {
                    study.set_color(side, shown[side]);
                    written = true;
                }
            }
        }
        if written {
            self.readout_revision = crate::model::next_revision();
        }
    }

    /// Takes into the configuration a colour that an Analyst price forecast study's settings
    /// gave it since the studies last had the configured ones, and gives it to the other such
    /// studies, so that the edit is recorded with the user action that ends.
    #[cfg(feature = "studies")]
    pub(super) fn pull_price_target_colors(&mut self) {
        let (Some(mut memo), Some(theme)) = (self.price_target_colors, &self.inherited_theme)
        else {
            return;
        };
        let themed = [theme.chart.candle_up, theme.chart.candle_down];
        let mut written = false;
        for side in 0..2 {
            let Some(edited) = self
                .indicators()
                .filter(|(_, study)| super::studies::is_price_target_study(*study))
                .map(|(_, study)| study.plot_style(side).color)
                .find(|color| *color != memo[side])
            else {
                continue;
            };
            let choice = crate::config::ChartConfig::price_target_choice(edited, themed[side]);
            if side == 0 {
                self.config.price_target_positive_color = choice;
            } else {
                self.config.price_target_negative_color = choice;
            }
            memo[side] = edited;
            for study in self.studies.indicators_mut() {
                if super::studies::is_price_target_study(study.as_ref())
                    && study.plot_style(side).color != edited
                {
                    study.set_color(side, edited);
                    written = true;
                }
            }
        }
        if written {
            self.readout_revision = crate::model::next_revision();
        }
        self.price_target_colors = Some(memo);
    }

    /// Turns the targets setting on when the study added last is an Analyst price forecast, and
    /// gives that study the colours the others have where it holds the catalogue's defaults, so
    /// that a study saved with its own colours keeps them.
    #[cfg(feature = "studies")]
    pub(super) fn adopt_price_target_study(&mut self) {
        let Some(study) = self.studies.indicators_mut().last_mut() else {
            return;
        };
        if !super::studies::is_price_target_study(study.as_ref()) {
            return;
        }
        self.show_price_targets = true;
        let Some(memo) = self.price_target_colors else {
            return;
        };
        let defaults = crate::studies::AnalystPriceForecast::default().colors();
        for (side, default) in defaults.into_iter().enumerate() {
            if study.plot_style(side).color == default {
                study.set_color(side, memo[side]);
            }
        }
    }

    pub(super) fn paint_targets(
        &self,
        ui: &egui::Ui,
        coords: &ChartMapping,
        label_top: f32,
        line_slot: egui::layers::ShapeIdx,
    ) {
        let (Some(targets), Some(sides)) = (self.visible_targets(), self.target_sides()) else {
            return;
        };
        let Some(last) = crate::model::last_with_value(self.state.shown()) else {
            return;
        };
        let Some((origin_index, reference)) =
            publication_bar(self.state.shown(), targets.published)
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
        let origin = Pos2::new(
            coords.idx_to_x(origin_index),
            coords.price.price_to_y(reference.close),
        );
        let end_x = coords.index_to_x(horizon);
        let theme = ui.active_theme();
        let themed = [theme.chart.candle_up, theme.chart.candle_down];
        let [(up_line, up_fill), (down_line, down_fill)] = [0, 1].map(|side| {
            let choice = sides[side].0;
            // A study holds the theme's own fill for the theme's colour, whose line it keeps.
            #[cfg(feature = "studies")]
            let choice = crate::config::ChartConfig::price_target_choice(choice, themed[side]);
            crate::config::ChartConfig::price_target_colors(choice, themed[side])
        });
        let shown = sides.map(|(_, shown)| shown);
        let (dash, gap) = crate::model::LineStyle::Dotted
            .dash_pattern()
            .expect("dotted lines have a dash pattern");
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
        // The area between the projections on each side of the publication close.
        let y = |price| coords.price.price_to_y(price);
        for (fill, top, bottom, drawn) in [
            (
                up_fill,
                targets.high,
                targets.low.max(reference.close),
                shown[0] && targets.high > reference.close,
            ),
            (
                down_fill,
                targets.high.min(reference.close),
                targets.low,
                shown[1] && targets.low < reference.close,
            ),
        ] {
            let points = vec![
                origin,
                Pos2::new(end_x, y(top)),
                Pos2::new(end_x, y(bottom)),
            ];
            if drawn && fill.a() > 0 && points.iter().all(|point| point.is_finite()) {
                lines.push(egui::Shape::convex_polygon(points, fill, Stroke::NONE));
            }
        }
        let formatter = self.series_label_formatter(super::SeriesId::MAIN, PriceScaleMode::Normal);
        for (position, (name, price)) in levels.into_iter().enumerate() {
            let end = Pos2::new(end_x, coords.price.price_to_y(price));
            if !end.is_finite()
                || !origin.is_finite()
                || !level_shown(shown, reference.close, price)
            {
                continue;
            }
            let color = if price > reference.close {
                up_line
            } else if price < reference.close {
                down_line
            } else {
                theme.ui.text_secondary
            };
            lines.extend(crate::painting::dashed_line::dashed_polyline_shapes(
                &[origin, end],
                Stroke::new(1.5, color),
                dash,
                gap,
                coords.rect,
            ));
            let decimals = self
                .readout_price_decimals()
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
            let shown = l10n::text(ui.ctx(), name);
            let galley = painter.layout_no_wrap(
                format!("{shown} {price_text}{change}"),
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
                    ("name", shown),
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

/// Whether the level at `price` is drawn when each side, positive then negative, is `shown`: a
/// level above the publication close `reference` with the positive side, one below it with the
/// negative side, and one equal to it with either.
fn level_shown(shown: [bool; 2], reference: f64, price: f64) -> bool {
    if price > reference {
        shown[0]
    } else if price < reference {
        shown[1]
    } else {
        shown[0] || shown[1]
    }
}

fn percent_change(close: f64, target: f64) -> Option<f64> {
    if close == 0.0 {
        return None;
    }
    let change = (target - close) / close * 100.0;
    change.is_finite().then_some(change)
}

/// The close of the last bar opening before the publication day's end in UTC, as replay
/// cutoffs count it, or the first bar with a value when the history starts later.
fn publication_bar(
    bars: &[crate::model::Bar],
    published: chrono::NaiveDate,
) -> Option<(usize, &crate::model::Bar)> {
    let end = published.succ_opt()?.and_hms_opt(0, 0, 0)?.and_utc();
    let before = bars.partition_point(|bar| bar.time < end);
    bars[..before]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, bar)| !bar.is_whitespace())
        .or_else(|| {
            bars.iter()
                .enumerate()
                .find(|(_, bar)| !bar.is_whitespace())
        })
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
            reply.send(Ok(HistoryPage::new(vec![], true)));
        }
        fn price_targets(&self, symbol: &str, reply: Reply<PriceTargets>) {
            self.0.lock().unwrap().push((symbol.into(), reply));
        }
        fn price_targets_converted(
            &self,
            symbol: &str,
            conversion: &str,
            reply: Reply<PriceTargets>,
        ) {
            let asked = format!("{symbol} in {conversion}");
            self.0.lock().unwrap().push((asked, reply));
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

    /// The chart's Analyst price forecast studies.
    #[cfg(feature = "studies")]
    fn forecast(chart: &crate::widget::Chart) -> Vec<SeriesId> {
        chart
            .indicators()
            .filter(|(_, study)| study.name() == crate::studies::AnalystPriceForecast::NAME)
            .map(|(id, _)| id)
            .collect()
    }

    #[cfg(feature = "studies")]
    #[test]
    fn targets_setting_respects_the_study_cap_in_user_actions() {
        use crate::studies::{AnalystPriceForecast, SMA};
        use crate::widget::settings::ChartSettingsState;

        for (cap, with_forecast, shown, count) in [
            (Some(2), false, false, 2),
            (Some(3), false, true, 3),
            (None, false, true, 3),
            (Some(2), true, true, 2),
        ] {
            let ctx = egui::Context::default();
            let mut chart = test_frames::chart(&[100.0], &[200.0]);
            chart.config_mut().show_volume = false;
            chart.add_indicator(Box::new(SMA::new(2)));
            if with_forecast {
                chart.add_indicator(Box::new(AnalystPriceForecast::default()));
            } else {
                chart.add_indicator(Box::new(SMA::new(5)));
            }
            chart.set_study_limit(cap);
            test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
            chart.user_action(|chart| {
                let mut settings = ChartSettingsState::from_chart(chart);
                settings.show_price_targets = true;
                settings.apply_to_chart(chart).unwrap();
            });
            assert_eq!(
                (
                    chart.price_targets_visible(),
                    chart.indicators().count(),
                    forecast(&chart).len(),
                    chart.can_undo(),
                ),
                (shown, count, usize::from(shown), shown && !with_forecast),
                "cap {cap:?}, forecast study {with_forecast}"
            );

            if shown && !with_forecast {
                chart.set_study_limit(Some(2));
                assert!(chart.undo());
                assert!(!chart.price_targets_visible());
                assert_eq!(chart.indicators().count(), 2);
                assert!(chart.redo());
                assert!(chart.price_targets_visible());
                assert_eq!(chart.indicators().count(), 3);
            } else if !shown {
                // Host calls and saved layouts still restore a study past the cap.
                chart.set_price_targets_visible(true);
                assert!(chart.price_targets_visible());
                assert_eq!(chart.indicators().count(), 3);
                assert!(!chart.can_undo());
                #[cfg(feature = "ui")]
                {
                    let saved = chart.snapshot().unwrap();
                    chart.set_price_targets_visible(false);
                    let mut legacy = chart.snapshot().unwrap();
                    legacy.settings.show_price_targets = true;
                    for snapshot in [saved, legacy] {
                        chart.set_price_targets_visible(false);
                        chart.user_action(|chart| chart.restore(&snapshot).unwrap());
                        assert!(chart.price_targets_visible());
                        assert_eq!(chart.indicators().count(), 3);
                        assert_eq!(forecast(&chart).len(), 1);
                        assert!(!chart.can_undo());
                    }
                }
            }
        }
    }

    /// Publication colours and dotted projections use the publication close; percentages keep
    /// using the latest close. Here bar 79 (2024-02-01) closes at 130 and the last at 100.
    #[test]
    fn targets_are_drawn_from_their_publication_close_in_their_colours() {
        use crate::theme::Theme;
        use egui::Color32;

        let ctx = egui::Context::default();
        let feed = Arc::new(Feed::default());
        let mut highs = [100.0; 100];
        highs[79] = 130.0;
        let mut chart = crate::widget::ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("A")
            .build();
        chart.set_data(
            SeriesId::MAIN,
            test_frames::chart(&[95.0], &highs)
                .state
                .data()
                .bars
                .clone(),
        );
        chart.set_timeframe(crate::model::Timeframe::Day1);
        chart.set_feed_paused(true);
        chart.set_price_targets_visible(true);
        chart.pump_targets(&ctx);
        feed.0.lock().unwrap().pop().unwrap().1.send(Ok(targets()));
        chart.pump_targets(&ctx);
        test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        let horizon = chart
            .time_to_index(targets().horizon.and_hms_opt(0, 0, 0).unwrap().and_utc())
            .unwrap() as f32;
        let label = |shapes: &[egui::Shape], prefix: &str| {
            shapes
                .iter()
                .find_map(|shape| match shape {
                    egui::Shape::Text(text) if text.galley.text().starts_with(prefix) => {
                        Some(text.galley.job.sections[0].format.color)
                    }
                    _ => None,
                })
                .expect(prefix)
        };
        let dashes = |shapes: &[egui::Shape], color| {
            shapes
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::LineSegment { points, stroke }
                        if stroke.width == 1.5 && stroke.color == color =>
                    {
                        Some(*points)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let fills = |shapes: &[egui::Shape], x: f32| {
            shapes
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Path(path)
                        if path.closed
                            && path.points.len() == 3
                            && (path.points[0].x - x).abs() < 0.5 =>
                    {
                        Some(path.fill)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        for (time, theme, fill) in [
            (
                0.1,
                Theme::dark(),
                Color32::from_rgba_premultiplied(48, 11, 14, 51),
            ),
            (
                0.2,
                Theme::high_contrast(),
                Color32::from_rgba_premultiplied(51, 19, 19, 51),
            ),
        ] {
            chart.set_theme(&ctx, &theme);
            chart.set_visible_logical_range(crate::model::LogicalRange::new(0.0, horizon + 200.0));
            let shapes = test_frames::shapes(&ctx, &mut chart, time, vec![]);
            let x = chart.index_to_x(79.0).unwrap();
            for text in [
                "High 120.00 (+20.00%)",
                "Average 110.00 (+10.00%)",
                "Low 80.00 (-20.00%)",
            ] {
                assert_eq!(label(&shapes, text), theme.chart.candle_down, "{text}");
            }
            let dots = dashes(&shapes, theme.chart.candle_down);
            assert_eq!(dots.iter().filter(|p| (p[0].x - x).abs() < 0.5).count(), 3);
            assert!(
                dots.iter()
                    .all(|p| p[0].x >= x - 0.5 && p[0].distance(p[1]) <= 2.001)
            );
            assert_eq!(fills(&shapes, x), [fill]);
        }
        let theme = Theme::dark();
        chart.set_theme(&ctx, &theme);
        chart.set_price_targets_visible(false);
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.3, vec![]);
        assert!(dashes(&shapes, theme.chart.candle_down).is_empty());
        assert!(fills(&shapes, chart.index_to_x(79.0).unwrap()).is_empty());
        chart.set_price_targets_visible(true);
        chart.reload_price_targets();
        chart.pump_targets(&ctx);
        feed.0
            .lock()
            .unwrap()
            .pop()
            .unwrap()
            .1
            .send(Ok(PriceTargets {
                high: 150.0,
                average: 140.0,
                ..targets()
            }));
        chart.pump_targets(&ctx);
        let mut settings = crate::widget::settings::ChartSettingsState::from_chart(&chart);
        settings.price_target_positive_color = Some(Color32::from_rgba_unmultiplied(0, 0, 255, 51));
        settings.apply_to_chart(&mut chart).unwrap();
        chart.set_visible_logical_range(crate::model::LogicalRange::new(0.0, horizon + 200.0));
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.4, vec![]);
        let x = chart.index_to_x(79.0).unwrap();
        for text in ["High 150.00 (+50.00%)", "Average 140.00 (+40.00%)"] {
            assert_eq!(label(&shapes, text), Color32::BLUE);
        }
        assert_eq!(
            label(&shapes, "Low 80.00 (-20.00%)"),
            theme.chart.candle_down
        );
        assert_eq!(
            dashes(&shapes, Color32::BLUE)
                .iter()
                .filter(|p| (p[0].x - x).abs() < 0.5)
                .count(),
            2
        );
        assert_eq!(
            fills(&shapes, x),
            [
                Color32::from_rgba_premultiplied(0, 0, 51, 51),
                Color32::from_rgba_premultiplied(48, 11, 14, 51),
            ]
        );
        #[cfg(feature = "ui")]
        {
            let saved = serde_json::to_value(chart.snapshot().unwrap()).unwrap();
            let mut old = saved.clone();
            for group in ["config", "settings"] {
                for key in ["price_target_positive_color", "price_target_negative_color"] {
                    old[group].as_object_mut().unwrap().remove(key).expect(key);
                }
            }
            // Serialized on origin/main at 92bfa0a0f, with the optional config, options and
            // scale omitted: targets enabled and coloured blue, with no forecast study.
            let before_study: serde_json::Value = serde_json::from_str(
                r##"{"main_scale":"Right","symbol":"A","interval":"Day1","extended_hours":false,"overnight_hours":false,"chart_type":"Candles","chart_type_params":{"footprint_modes":["Ladder"],"footprint_profile":false,"footprint_color_by":"VolumeSteps","footprint_intensity":"Bar","footprint_colors":[null,null,null,null],"footprint_imbalance_colors":[null,null],"footprint_row_ticks":0,"footprint_row_atr":false,"footprint_row_atr_length":14,"footprint_auto_row_height":16.0,"footprint_min_text_height":12.0,"footprint_imbalance":true,"footprint_poc":true,"footprint_value_area":0.7,"footprint_imbalance_ratio":3.0,"footprint_imbalance_min_volume":0.0,"footprint_diagonal":true,"footprint_stacked_imbalances":false,"footprint_stack":3,"footprint_naked_poc":false,"footprint_delta_poc":false,"footprint_unfinished_auctions":false,"footprint_cluster_share":null,"footprint_single_print_max":null,"footprint_delta_percent":null,"footprint_ratio":null,"footprint_extend_bars":null,"footprint_extreme_ratio":false,"footprint_pullback":false,"footprint_abbreviate":true,"footprint_show_zero":true,"footprint_font_size":[10.0,10.0],"footprint_info_box":true,"footprint_statistics":false,"footprint_statistics_rows":["Volume","Delta","DeltaPercent"],"footprint_session_cumulative_delta":false,"profile":{"session":"Chart","rows":"Feed","mode":"BidAsk","developing_poc":false,"developing_value_area":false,"extend_poc":false,"extend_vah":false,"extend_val":false,"nodes":null,"value_area":0.7,"value_area_guides":false,"poc":true,"poc_color":null,"value_area_high":true,"value_area_high_color":null,"value_area_low":true,"value_area_low_color":null,"up_volume_color":null,"down_volume_color":null,"value_area_up_color":null,"value_area_down_color":null},"styles":{"line":{"color":[41,98,255,255],"width":2.0,"style":"Solid","line_type":"Simple"},"line_gradient":false,"line_gradient_start":[41,98,255,255],"line_gradient_end":[156,39,176,255],"step_line":{"color":[41,98,255,255],"width":2.0,"style":"Solid","line_type":"Simple"},"line_with_markers":{"color":[41,98,255,255],"width":2.0,"style":"Solid","line_type":"Simple"},"marker_radius":3.0,"area":{"line":{"color":[41,98,255,255],"width":2.0,"style":"Solid","line_type":"Simple"},"marker_radius":0.0,"top_fill":[18,44,115,115],"bottom_fill":[0,0,0,0],"relative_gradient":true,"invert_filled_area":false},"baseline":{"above":{"color":[8,153,129,255],"width":2.0,"style":"Solid","line_type":"Simple"},"below":{"color":[242,54,69,255],"width":2.0,"style":"Solid","line_type":"Simple"},"marker_radius":0.0,"above_fill":[1,18,15,30],"below_fill":[28,6,8,30],"above_fill_at_base":null,"below_fill_at_base":null},"line_price_source":null,"step_line_price_source":null,"line_with_markers_price_source":null,"area_price_source":null,"baseline_price_source":null,"hlc_area":{"high_line_visible":true,"high_line":{"color":[8,153,129,255],"width":2.0,"style":"Solid","line_type":"Simple"},"low_line_visible":true,"low_line":{"color":[242,54,69,255],"width":2.0,"style":"Solid","line_type":"Simple"},"close_line":{"color":[134,137,147,255],"width":2.0,"style":"Solid","line_type":"Simple"},"high_to_close_fill_visible":true,"high_to_close_fill":[2,31,26,51],"close_to_low_fill_visible":true,"close_to_low_fill":[48,11,14,51]},"candles":{"body_visible":true,"wick_visible":true,"color_by_previous_close":false},"hollow":{"body_visible":true,"border_visible":true,"wick_visible":true,"body_up":[8,153,129,255],"body_down":[242,54,69,255],"border_up":[8,153,129,255],"border_down":[242,54,69,255],"wick_up":[8,153,129,255],"wick_down":[242,54,69,255]},"volume_candles":{"body_visible":true,"border_visible":true,"wick_visible":true,"color_by_previous_close":false,"body_up":[8,153,129,255],"body_down":[242,54,69,255],"border_up":[8,153,129,255],"border_down":[242,54,69,255],"wick_up":[8,153,129,255],"wick_down":[242,54,69,255]},"heikin":{"body_visible":true,"border_visible":true,"wick_visible":true,"color_by_previous_close":false,"body_up":[8,153,129,255],"body_down":[242,54,69,255],"border_up":[8,153,129,255],"border_down":[242,54,69,255],"wick_up":[8,153,129,255],"wick_down":[242,54,69,255],"real_price_only":false},"bars":{"open_visible":true,"thin_bars":true,"color_by_previous_close":false},"columns":{"price_source":"Close"},"high_low":{"bar_color":[41,98,255,255],"labels_visible":true,"label_color":[64,254,0,128],"label_font_size":7.0},"renko":{"border_visible":true,"border_up":null,"border_down":null,"wick_up":null,"wick_down":null},"line_break":{"border_visible":true,"border_up":null,"border_down":null},"kagi":{"thick_width":2.5,"thin_width":1.5,"up_color":[8,153,129,255],"down_color":[242,54,69,255]},"point_figure":{"up_color":[8,153,129,255],"down_color":[242,54,69,255]},"range":{"thin_bars":false,"phantom_bars":false},"crosshair_marker_radius":0.0,"crosshair_marker_fill":null,"crosshair_marker_border_color":null,"crosshair_marker_border_width":2.0,"last_price_animation":"Disabled"},"renko_size_method":"Fixed","kagi_size_method":"Fixed","range_size_method":"Fixed","pnf_size_method":"Fixed","renko_source":"Close","kagi_source":"Close","line_break_source":"Close","pnf_source":"HighLow","renko_wicks":false,"renko_brick_size":1.0,"kagi_reversal":1.0,"range_size":1.0,"pnf_box_size":1.0,"pnf_reversal":3,"line_break_count":3,"pnf_one_step_back":false,"renko_real_price":true,"kagi_real_price":true,"line_break_real_price":true,"pnf_real_price":true,"baseline":{"PercentOfPane":50.0},"columns_base":null,"tpo_period_minutes":30,"tpo_initial_balance_minutes":60,"tpo_poor_highs_lows":false,"tpo_poor_high":false,"tpo_poor_low":false,"tpo_extend_poor":false,"tpo_initial_balance_extensions":false,"tpo_row_ticks":0,"tpo_value_area":70.0,"tpo_split_sessions":false,"tpo_single_prints":false,"tpo_midpoint":false,"tpo_letters":true,"tpo_statistics":false,"tpo_colored_blocks":true,"tpo_block_colors":[[214,56,100,255],[90,197,97,255],[84,185,209,255],[93,36,245,255]],"tpo_block_color":[189,189,189,255],"tpo_outside_value_area_opacity":45,"tpo_profile_period":"Session","tpo_split_periods":false,"tpo_initial_balance":true,"tpo_poc":true,"tpo_value_area_levels":true,"tpo_open_price":true,"tpo_close_price":false,"tpo_volume_profile":false,"tpo_composites":[],"tpo_splits":[]},"main_series_visible":true,"main_series_alerts":{"visible":true,"labels":true,"color":[255,152,0,255]},"settings":{"chart_type":"Candles","chart_type_params":{"footprint_modes":["Ladder"],"footprint_profile":false,"footprint_color_by":"VolumeSteps","footprint_intensity":"Bar","footprint_colors":[null,null,null,null],"footprint_imbalance_colors":[null,null],"footprint_row_ticks":0,"footprint_row_atr":false,"footprint_row_atr_length":14,"footprint_auto_row_height":16.0,"footprint_min_text_height":12.0,"footprint_imbalance":true,"footprint_poc":true,"footprint_value_area":0.7,"footprint_imbalance_ratio":3.0,"footprint_imbalance_min_volume":0.0,"footprint_diagonal":true,"footprint_stacked_imbalances":false,"footprint_stack":3,"footprint_naked_poc":false,"footprint_delta_poc":false,"footprint_unfinished_auctions":false,"footprint_cluster_share":null,"footprint_single_print_max":null,"footprint_delta_percent":null,"footprint_ratio":null,"footprint_extend_bars":null,"footprint_extreme_ratio":false,"footprint_pullback":false,"footprint_abbreviate":true,"footprint_show_zero":true,"footprint_font_size":[10.0,10.0],"footprint_info_box":true,"footprint_statistics":false,"footprint_statistics_rows":["Volume","Delta","DeltaPercent"],"footprint_session_cumulative_delta":false,"profile":{"session":"Chart","rows":"Feed","mode":"BidAsk","developing_poc":false,"developing_value_area":false,"extend_poc":false,"extend_vah":false,"extend_val":false,"nodes":null,"value_area":0.7,"value_area_guides":false,"poc":true,"poc_color":null,"value_area_high":true,"value_area_high_color":null,"value_area_low":true,"value_area_low_color":null,"up_volume_color":null,"down_volume_color":null,"value_area_up_color":null,"value_area_down_color":null},"styles":{"line":{"color":[41,98,255,255],"width":2.0,"style":"Solid","line_type":"Simple"},"line_gradient":false,"line_gradient_start":[41,98,255,255],"line_gradient_end":[156,39,176,255],"step_line":{"color":[41,98,255,255],"width":2.0,"style":"Solid","line_type":"Simple"},"line_with_markers":{"color":[41,98,255,255],"width":2.0,"style":"Solid","line_type":"Simple"},"marker_radius":3.0,"area":{"line":{"color":[41,98,255,255],"width":2.0,"style":"Solid","line_type":"Simple"},"marker_radius":0.0,"top_fill":[18,44,115,115],"bottom_fill":[0,0,0,0],"relative_gradient":true,"invert_filled_area":false},"baseline":{"above":{"color":[8,153,129,255],"width":2.0,"style":"Solid","line_type":"Simple"},"below":{"color":[242,54,69,255],"width":2.0,"style":"Solid","line_type":"Simple"},"marker_radius":0.0,"above_fill":[1,18,15,30],"below_fill":[28,6,8,30],"above_fill_at_base":null,"below_fill_at_base":null},"line_price_source":null,"step_line_price_source":null,"line_with_markers_price_source":null,"area_price_source":null,"baseline_price_source":null,"hlc_area":{"high_line_visible":true,"high_line":{"color":[8,153,129,255],"width":2.0,"style":"Solid","line_type":"Simple"},"low_line_visible":true,"low_line":{"color":[242,54,69,255],"width":2.0,"style":"Solid","line_type":"Simple"},"close_line":{"color":[134,137,147,255],"width":2.0,"style":"Solid","line_type":"Simple"},"high_to_close_fill_visible":true,"high_to_close_fill":[2,31,26,51],"close_to_low_fill_visible":true,"close_to_low_fill":[48,11,14,51]},"candles":{"body_visible":true,"wick_visible":true,"color_by_previous_close":false},"hollow":{"body_visible":true,"border_visible":true,"wick_visible":true,"body_up":[8,153,129,255],"body_down":[242,54,69,255],"border_up":[8,153,129,255],"border_down":[242,54,69,255],"wick_up":[8,153,129,255],"wick_down":[242,54,69,255]},"volume_candles":{"body_visible":true,"border_visible":true,"wick_visible":true,"color_by_previous_close":false,"body_up":[8,153,129,255],"body_down":[242,54,69,255],"border_up":[8,153,129,255],"border_down":[242,54,69,255],"wick_up":[8,153,129,255],"wick_down":[242,54,69,255]},"heikin":{"body_visible":true,"border_visible":true,"wick_visible":true,"color_by_previous_close":false,"body_up":[8,153,129,255],"body_down":[242,54,69,255],"border_up":[8,153,129,255],"border_down":[242,54,69,255],"wick_up":[8,153,129,255],"wick_down":[242,54,69,255],"real_price_only":false},"bars":{"open_visible":true,"thin_bars":true,"color_by_previous_close":false},"columns":{"price_source":"Close"},"high_low":{"bar_color":[41,98,255,255],"labels_visible":true,"label_color":[64,254,0,128],"label_font_size":7.0},"renko":{"border_visible":true,"border_up":null,"border_down":null,"wick_up":null,"wick_down":null},"line_break":{"border_visible":true,"border_up":null,"border_down":null},"kagi":{"thick_width":2.5,"thin_width":1.5,"up_color":[8,153,129,255],"down_color":[242,54,69,255]},"point_figure":{"up_color":[8,153,129,255],"down_color":[242,54,69,255]},"range":{"thin_bars":false,"phantom_bars":false},"crosshair_marker_radius":0.0,"crosshair_marker_fill":null,"crosshair_marker_border_color":null,"crosshair_marker_border_width":2.0,"last_price_animation":"Disabled"},"renko_size_method":"Fixed","kagi_size_method":"Fixed","range_size_method":"Fixed","pnf_size_method":"Fixed","renko_source":"Close","kagi_source":"Close","line_break_source":"Close","pnf_source":"HighLow","renko_wicks":false,"renko_brick_size":1.0,"kagi_reversal":1.0,"range_size":1.0,"pnf_box_size":1.0,"pnf_reversal":3,"line_break_count":3,"pnf_one_step_back":false,"renko_real_price":true,"kagi_real_price":true,"line_break_real_price":true,"pnf_real_price":true,"baseline":{"PercentOfPane":50.0},"columns_base":null,"tpo_period_minutes":30,"tpo_initial_balance_minutes":60,"tpo_poor_highs_lows":false,"tpo_poor_high":false,"tpo_poor_low":false,"tpo_extend_poor":false,"tpo_initial_balance_extensions":false,"tpo_row_ticks":0,"tpo_value_area":70.0,"tpo_split_sessions":false,"tpo_single_prints":false,"tpo_midpoint":false,"tpo_letters":true,"tpo_statistics":false,"tpo_colored_blocks":true,"tpo_block_colors":[[214,56,100,255],[90,197,97,255],[84,185,209,255],[93,36,245,255]],"tpo_block_color":[189,189,189,255],"tpo_outside_value_area_opacity":45,"tpo_profile_period":"Session","tpo_split_periods":false,"tpo_initial_balance":true,"tpo_poc":true,"tpo_value_area_levels":true,"tpo_open_price":true,"tpo_close_price":false,"tpo_volume_profile":false,"tpo_composites":[],"tpo_splits":[]},"price_source":"Close","candle_colors":{"color_by_previous_close":false,"show_body":true,"show_border":false,"show_wick":true,"body_up":[8,153,129,255],"body_down":[242,54,69,255],"border_up":[8,153,129,255],"border_down":[242,54,69,255],"wick_up":[8,153,129,255],"wick_down":[242,54,69,255]},"precision":"Default","timezone":"UTC","show_price_targets":true,"price_target_positive_color":[0,0,51,51],"price_target_negative_color":[64,254,0,128],"adjust_for_dividends":false,"back_adjust_futures":false,"settlement_as_close":true,"order_flow":{"show_heatmap":false,"heatmap":{"brightness":1.0,"lower_cutoff":0.0,"full_intensity_size":null},"heatmap_retention":null,"show_order_book":false,"order_book":{"filter":{"min_size":0.0,"min_age":null,"max_age":null,"exclude_participants":[]},"profile":"Total","quantity_unit":null,"tiers":null,"tier_colors":null},"show_volume_bubbles":false,"volume_bubbles":{"min_diameter":4.0,"max_diameter":40.0,"opacity":128,"buy_color":[8,153,129,255],"sell_color":[242,54,69,255],"neutral_color":[150,150,150,255],"color_by":"Side","scaling_mode":"Area","clamp_std_devs":null,"show_border":false,"border_color":[100,100,100,100],"min_size_filter":null,"grouping":"Ungrouped","shaded":true}},"status_line":{"show_logo":true,"show_symbol":true,"title_as_description":true,"show_market_status":true,"show_quote_boxes":true,"quote_panel":false,"show_indicator_titles":true,"show_indicator_inputs":true,"show_indicator_values":true,"show_ohlc":true,"show_change":true,"show_change_percent":true,"show_volume":true,"show_last_day_change":false,"indicator_background_opacity":60},"scales_and_lines":{"axis_layout":{"side":null,"hide_unused":true},"show_currency":true,"show_unit":true,"currency_unit_on_hover":false,"show_scale_mode_buttons":true,"show_plus_button":true,"allow_label_overlap":false,"lock_price_to_bar_ratio":false,"price_to_bar_ratio":1.0,"symbol_price_color":null,"price_line_width":1.0,"price_line_style":"Dashed","show_last_price_change":false,"show_study_values":true,"study_value_color":null,"show_symbol_name":false,"show_study_names":false,"show_extended_session_price":true,"extended_session_price_color":[251,142,1,255],"auto_scale":true,"log_scale":false,"percentage_scale":false,"indexed_to_100":false,"invert_scale":false,"price_scale_left":false,"show_last_price_label":true,"show_prev_close_label":false,"show_price_line":true,"price_line_source":"LastBar","thousands_sep":"Comma","date_format":"Default","use_24_hour":true,"weekday_names":"Default","show_countdown":true,"show_bid_ask":false,"show_bid_ask_lines":null,"show_high_low":false,"show_high_low_lines":null,"show_prev_close":false,"prev_close_line":{"color":null,"width":1.0,"style":"Dotted"}},"chart_basic_styles":{"background_type":"Solid","background_color":[0,0,0,255],"background_gradient_top":[0,0,0,255],"background_gradient_bottom":[0,0,0,255]},"grid_lines":{"mode":"Both","horizontal_color":[34,34,34,255],"vertical_color":[34,34,34,255]},"crosshair":{"color":[64,254,0,128],"line_width":1.0,"line_style":"Dashed"},"watermark":{"mode":"ReplayMode","color":[34,34,34,255]},"scales_appearance":{"text_color":[255,255,255,255],"font_size":12,"font_family":"Proportional","lines_color":[0,0,0,0]},"btns":{"navigation":"Hidden","pane":"VisibleOnMouseOver"},"margins":{"top_percent":10.0,"bottom_percent":10.0,"right_bars":2.5},"trading":{"instant_orders":false,"compact_ticket":false,"bid_ask_labels":false,"play_execution_sound":false,"execution_sound":"","execution_sound_volume":1.0,"notifications":"All","show_positions":true,"position_pnl":true,"position_pnl_unit":"Money","reverse_button":true,"extend_lines":true,"label_alignment":"Right","include_in_image":false,"show_orders":true,"bracket_pnl":true,"bracket_pnl_unit":"Money","show_executions":true,"show_execution_labels":true},"alerts":{"play_sound":false,"sound":"","sound_volume":1.0,"show_alerts":true,"show_alert_labels":true,"alert_color":[255,152,0,255],"show_active_only":true,"notification_seconds":20,"close_toasts":true,"desktop_notifications":false},"events":{"show_dividends":true,"show_splits":true,"show_earnings":false,"show_earnings_breaks":false,"show_economic_events":false,"show_news":true,"show_news_notifications":false,"show_ideas":true},"show_session_breaks":false},"config":null,"palette":{"bullish_color":[8,153,129,255],"bearish_color":[242,54,69,255],"bullish_border_color":null,"bearish_border_color":null,"bullish_wick_color":null,"bearish_wick_color":null,"price_source":"Close","conflation_threshold":1.0,"background_color":[0,0,0,255],"background_style":"Solid","grid_color":[34,34,34,255],"vertical_grid_color":null,"grid_line_style":"Dotted","vertical_grid_line_style":null,"text_color":[255,255,255,255],"axis_font_size":null,"axis_font_family":"Proportional","thousands_sep":",","candle_width":0.6,"wick_width":1.0,"candle_border_width":0.0,"show_grid":true,"show_horizontal_grid":true,"show_vertical_grid":true,"show_volume":true,"volume_height_fraction":0.12,"volume_baseline_inset":12.0,"padding":0.0,"show_time_labels":true,"show_ohlc_info":true,"title_as_description":true,"show_quote_boxes":true,"quote_panel":false,"show_ohlc_change":true,"show_ohlc_change_percent":true,"show_legend_volume":true,"show_indicator_inputs":true,"show_legend_badge":true,"show_market_status":true,"show_last_day_change":false,"indicator_backdrop_opacity":0.6,"show_currency":true,"show_unit":true,"currency_unit_on_hover":false,"show_scale_mode_buttons":true,"show_plus_button":true,"show_extended_session_price":true,"extended_session_price_color":[251,142,1,255],"show_right_axis":true,"show_left_axis":false,"right_axis_width":70.0,"left_axis_width":70.0,"price_axis":{"allow_label_overlap":false,"show_study_values":true,"study_value_color":null,"show_symbol_name":false,"show_study_names":false,"border_visible":false,"border_color":null,"ticks_visible":false,"edge_ticks_visible":false,"hide_clipped_labels":true},"right_axis_scale_mode":"Normal","left_axis_scale_mode":"Normal","show_symbol_labels":true,"show_symbol_last_val":true,"show_symbol_last_change":false,"show_price_line":true,"show_prev_close_line":false,"prev_close_line":{"color":null,"width":1.0,"style":"Dotted"},"show_symbol_prev_close":false,"show_indicator_labels":true,"show_indicator_last_val":true,"lock_price_to_bar_ratio":false,"scale_price_chart_only":false,"show_bid_ask":false,"show_bid_ask_lines":null,"bid_line":{"color":null,"width":1.0,"style":"Dotted"},"ask_line":{"color":null,"width":1.0,"style":"Dotted"},"show_high_low":false,"show_high_low_lines":null,"high_line":{"color":null,"width":1.0,"style":"Dotted"},"low_line":{"color":null,"width":1.0,"style":"Dotted"},"show_average_close_line":false,"show_average_close_label":false,"average_close_line":{"color":null,"width":1.0,"style":"Dotted"},"show_countdown":true,"show_realtime_btn":true,"realtime_button_pos":"BottomRight","realtime_button_text":null,"realtime_button_size":[28.0,28.0],"realtime_button_color":[64,254,0,128],"realtime_button_hover_color":[64,254,0,128],"realtime_button_text_color":[64,254,0,128],"navigation_buttons":"Never","pane_buttons":"OnHover","show_session_breaks":false,"session_break_color":[73,133,231,255],"session_break_style":"Dashed","session_break_width":1.0,"session_config":null,"show_extended_hours":true,"extended_hours_color":[64,254,0,128],"price_target_positive_color":[64,254,0,128],"price_target_negative_color":[64,254,0,128],"premarket_color":null,"postmarket_color":null,"shade_premarket":true,"shade_postmarket":true,"overnight_hours_color":[64,254,0,128],"show_watermark":false,"watermark_text":null,"watermark_description":false,"watermark_color":[34,34,34,255],"watermark_font_size":48.0,"watermark_pos":"Center","skip_background":false},"options":null,"price_scale":null,"maximized":null,"studies":[],"series":[],"main_annotations":{"markers":[],"options":{"auto_scale":true,"z_order":"Normal"},"price_lines":[]},"panes":[{"main":true,"stretch":3.0,"collapsed":null,"preserve":false,"members":[0],"scale":null,"scales":[],"axes":{"side":null,"hide_unused":true}}],"bar_spacing":6.0,"right_offset":2.5,"drawings":{"drawings":[],"metadata":{"app_version":null,"created_at":1795162714678,"modified_at":1795162714678},"saved_by":5668528550023745189,"symbol":"A","timeframe":"1D","version":10},"saved_by":5668528550023745189}"##,
            )
            .unwrap();
            for (time, document, color, fill) in [
                (
                    0.5,
                    old,
                    theme.chart.candle_up,
                    Color32::from_rgba_premultiplied(2, 31, 26, 51),
                ),
                (
                    0.6,
                    before_study,
                    Color32::BLUE,
                    Color32::from_rgba_premultiplied(0, 0, 51, 51),
                ),
                (
                    0.7,
                    saved,
                    Color32::BLUE,
                    Color32::from_rgba_premultiplied(0, 0, 51, 51),
                ),
            ] {
                chart
                    .restore(&serde_json::from_value(document).unwrap())
                    .unwrap();
                assert_eq!(forecast(&chart).len(), 1, "{time}");
                test_frames::frame(&ctx, &mut chart, time, vec![]);
                chart.set_visible_logical_range(crate::model::LogicalRange::new(
                    0.0,
                    horizon + 200.0,
                ));
                let shapes = test_frames::shapes(&ctx, &mut chart, time + 0.1, vec![]);
                assert_eq!(label(&shapes, "High 150"), color);
                assert_eq!(fills(&shapes, chart.index_to_x(79.0).unwrap())[0], fill);
            }
        }
        // A fully transparent choice holds no RGB: only its fill disappears. The other side's
        // chosen hue and opacity remain independent, including through theme changes.
        chart.config_mut().price_target_positive_color = Color32::TRANSPARENT;
        chart.config_mut().price_target_negative_color =
            Color32::from_rgba_unmultiplied(255, 255, 0, 51);
        chart.set_theme(&ctx, &Theme::high_contrast());
        chart.set_visible_logical_range(crate::model::LogicalRange::new(0.0, horizon + 200.0));
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.9, vec![]);
        assert_eq!(
            label(&shapes, "High 150"),
            Theme::high_contrast().chart.candle_up
        );
        assert_eq!(label(&shapes, "Low 80"), Color32::YELLOW);
        assert_eq!(
            fills(&shapes, chart.index_to_x(79.0).unwrap()),
            [Color32::from_rgba_premultiplied(51, 51, 0, 51)]
        );
    }

    /// The catalogue's Analyst price forecast study draws the targets, its legend row hides and
    /// removes them and its settings recolour them, and the chart setting adds and removes it.
    /// Bar 79 (2024-02-01) closes at 130 and the last bar at 100; the targets are 150, 140 and 80.
    #[cfg(feature = "studies")]
    #[test]
    fn forecast_study_and_targets_setting_stay_in_step() {
        use crate::model::LogicalRange;
        use crate::studies::{AnalystPriceForecast, IndicatorFactory};
        use crate::theme::Theme;
        use crate::widget::settings::ChartSettingsState;
        use egui::Color32;

        let ctx = egui::Context::default();
        let feed = Arc::new(Feed::default());
        let mut highs = [100.0; 100];
        highs[79] = 130.0;
        let mut chart = crate::widget::ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("A")
            .build();
        chart.set_data(
            SeriesId::MAIN,
            test_frames::chart(&[95.0], &highs)
                .state
                .data()
                .bars
                .clone(),
        );
        chart.set_timeframe(crate::model::Timeframe::Day1);
        chart.set_feed_paused(true);
        let theme = Theme::dark();
        chart.set_theme(&ctx, &theme);
        // A colour chosen while the targets are off is the colour of the study added later.
        let yellow = Color32::from_rgba_unmultiplied(255, 255, 0, 51);
        chart.config_mut().price_target_positive_color = yellow;
        test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        let study = IndicatorFactory::new()
            .create(AnalystPriceForecast::NAME)
            .unwrap();
        let id = chart
            .user_action(|chart| chart.try_add_indicator(study))
            .unwrap();
        assert!(chart.price_targets_visible());
        assert_eq!(forecast(&chart), [id]);
        assert_eq!(chart.config().price_target_positive_color, yellow);
        chart.pump_targets(&ctx);
        feed.0
            .lock()
            .unwrap()
            .pop()
            .unwrap()
            .1
            .send(Ok(PriceTargets {
                high: 150.0,
                average: 140.0,
                ..targets()
            }));
        chart.pump_targets(&ctx);
        test_frames::frame(&ctx, &mut chart, 0.1, vec![]);
        let horizon = chart
            .time_to_index(targets().horizon.and_hms_opt(0, 0, 0).unwrap().and_utc())
            .unwrap() as f32;
        let mut time = 0.1;
        let mut shapes = |chart: &mut crate::widget::Chart| {
            time += 0.1;
            chart.set_visible_logical_range(LogicalRange::new(0.0, horizon + 200.0));
            test_frames::shapes(&ctx, chart, time, vec![])
        };
        let label = |shapes: &[egui::Shape], prefix: &str| {
            shapes.iter().find_map(|shape| match shape {
                egui::Shape::Text(text) if text.galley.text().starts_with(prefix) => {
                    Some(text.galley.job.sections[0].format.color)
                }
                _ => None,
            })
        };
        let fills = |chart: &crate::widget::Chart, shapes: &[egui::Shape]| {
            let x = chart.index_to_x(79.0).unwrap();
            shapes
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Path(path)
                        if path.closed
                            && path.points.len() == 3
                            && (path.points[0].x - x).abs() < 0.5 =>
                    {
                        Some(path.fill)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let down = theme.chart.candle_down;
        let down_fill = Color32::from_rgba_premultiplied(48, 11, 14, 51);
        let painted = shapes(&mut chart);
        assert!(label(&painted, AnalystPriceForecast::NAME).is_some());
        assert_eq!(label(&painted, "∅"), None, "no calculated forecast values");
        assert_eq!(
            label(&painted, "High 150.00 (+50.00%)"),
            Some(Color32::YELLOW)
        );
        assert_eq!(
            label(&painted, "Average 140.00 (+40.00%)"),
            Some(Color32::YELLOW)
        );
        assert_eq!(label(&painted, "Low 80.00 (-20.00%)"), Some(down));
        assert_eq!(
            fills(&chart, &painted),
            [Color32::from_rgba_premultiplied(51, 51, 0, 51), down_fill]
        );
        assert!(chart.get_chart_mapping().price.range().max >= 150.0);

        // The legend's eye hides the targets and keeps the setting.
        chart.user_action(|chart| chart.set_indicator_visible(id, false));
        assert_eq!(label(&shapes(&mut chart), "High 150"), None);
        assert!(chart.price_targets_visible());
        chart.user_action(|chart| chart.set_indicator_visible(id, true));

        // Hiding the Positive output hides the targets above the publication close and stops
        // fitting them: 80 to 130 with the default margins of 10% tops out near 135.
        let show_positive = |chart: &mut crate::widget::Chart, visible: bool| {
            chart.user_action(|chart| {
                let study = chart.indicator_mut(id).unwrap();
                let mut settings = study.settings();
                settings.plots[0].visible = visible;
                study.apply_settings(&[], settings).unwrap();
            })
        };
        show_positive(&mut chart, false);
        let painted = shapes(&mut chart);
        assert_eq!(label(&painted, "High 150"), None);
        assert_eq!(label(&painted, "Average 140"), None);
        assert_eq!(label(&painted, "Low 80.00 (-20.00%)"), Some(down));
        assert_eq!(fills(&chart, &painted), [down_fill]);
        assert!(chart.get_chart_mapping().price.range().max < 150.0);
        show_positive(&mut chart, true);

        // The study's settings recolour the targets and the chart setting, as one undo step.
        let blue = Color32::from_rgba_unmultiplied(0, 0, 255, 51);
        chart.user_action(|chart| chart.indicator_mut(id).unwrap().set_color(1, blue));
        assert_eq!(label(&shapes(&mut chart), "Low 80"), Some(Color32::BLUE));
        assert_eq!(chart.config().price_target_negative_color, blue);
        assert_eq!(
            ChartSettingsState::from_chart(&chart).price_target_negative_color,
            Some(blue)
        );
        assert!(chart.undo());
        assert_eq!(label(&shapes(&mut chart), "Low 80"), Some(down));
        assert_eq!(
            chart.config().price_target_negative_color,
            Color32::PLACEHOLDER
        );
        // A picker's rounding of the theme's fill keeps following the theme.
        chart.user_action(|chart| {
            let near = Color32::from_rgba_premultiplied(49, 11, 14, 51);
            chart.indicator_mut(id).unwrap().set_color(1, near)
        });
        assert_eq!(
            chart.config().price_target_negative_color,
            Color32::PLACEHOLDER
        );

        // The chart setting recolours the study.
        let magenta = Color32::from_rgba_unmultiplied(255, 0, 255, 51);
        chart.config_mut().price_target_positive_color = magenta;
        let painted = shapes(&mut chart);
        let positive = chart
            .indicators()
            .find(|(study, _)| *study == id)
            .unwrap()
            .1;
        assert_eq!(positive.plot_style(0).color, magenta);
        assert_eq!(
            label(&painted, "High 150"),
            Some(Color32::from_rgb(255, 0, 255))
        );

        // The legend's Remove turns the setting off, and undo brings both back.
        chart.user_action(|chart| chart.remove_series(id));
        assert!(!chart.price_targets_visible());
        assert_eq!(label(&shapes(&mut chart), "High 150"), None);
        assert!(chart.undo());
        assert_eq!(forecast(&chart), [id]);
        assert!(chart.price_targets_visible());
        assert!(label(&shapes(&mut chart), "High 150").is_some());
        assert!(chart.redo());
        assert!(!chart.price_targets_visible());
        assert!(forecast(&chart).is_empty());

        // The setting adds one study and removes every one.
        chart.set_price_targets_visible(true);
        chart.set_price_targets_visible(true);
        assert_eq!(forecast(&chart).len(), 1);
        let mut settings = ChartSettingsState::from_chart(&chart);
        settings.show_price_targets = false;
        settings.apply_to_chart(&mut chart).unwrap();
        assert!(forecast(&chart).is_empty());
        assert!(!chart.price_targets_visible());
    }

    /// Midnight of the next day is excluded, missing bars are skipped, and short or closed-market
    /// histories retain the first or last close they actually hold.
    #[test]
    fn publication_bar_is_the_last_bar_opening_on_the_publication_day() {
        use crate::model::Bar;
        use chrono::{NaiveDate, TimeDelta, TimeZone, Utc};
        let start = Utc.with_ymd_and_hms(2024, 1, 30, 0, 0, 0).unwrap();
        for (date, whitespace, expected) in [
            ((2024, 2, 1), None, Some(2)),
            ((2024, 2, 1), Some(2), Some(1)),
            ((2024, 1, 1), None, Some(0)),
            ((2024, 3, 1), None, Some(4)),
        ] {
            let bars: Vec<_> = (0..5)
                .map(|i| {
                    let time = start + TimeDelta::days(i);
                    if Some(i) == whitespace {
                        Bar::whitespace(time)
                    } else {
                        Bar::new(time, 100.0, 105.0, 95.0, 100.0 + i as f64, 1.0)
                    }
                })
                .collect();
            let published = NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap();
            assert_eq!(
                publication_bar(&bars, published).map(|(index, _)| index),
                expected,
                "{date:?}, whitespace {whitespace:?}"
            );
        }
    }

    #[test]
    fn target_labels_use_the_last_close_and_clear_on_symbol_changes() {
        use chrono::{TimeZone, Utc};
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
        // A bar replay draws and fits the estimates, published on 2024-02-01, only once its
        // cutoff has passed that day.
        let at = |day, hour| Utc.with_ymd_and_hms(2024, 2, day, hour, 0, 0).unwrap();
        for (time, cutoff, drawn) in [(0.1003, at(2, 0), true), (0.1006, at(1, 12), false)] {
            chart.set_replay_time(Some(cutoff));
            chart.set_visible_logical_range(crate::model::LogicalRange::new(0.0, horizon + 200.0));
            let shapes = test_frames::shapes(&ctx, &mut chart, time, vec![]);
            let label = shapes.iter().any(
                |s| matches!(s, egui::Shape::Text(t) if t.galley.text() == "High 120.00 (+20.00%)"),
            );
            let fitted = !chart.target_prices().is_empty();
            assert_eq!((label, fitted), (drawn, drawn), "replay to {cutoff}");
        }
        chart.set_replay_time(None);
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
        // A precision chosen in the chart settings takes precedence over the instrument's.
        chart.set_instrument(Some(
            crate::model::Instrument::new("B").with_price_decimals(3),
        ));
        chart.price_scale_mut().set_price_precision(Some(1));
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.1105, vec![]);
        assert!(
            shapes.iter().any(
                |s| matches!(s, egui::Shape::Text(t) if t.galley.text() == "High 120.0 (+20.00%)")
            ),
            "target labels follow the chosen precision"
        );
        chart.price_scale_mut().set_price_precision(None);
        chart.set_instrument(None);
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

    /// A market replay draws only the targets its driver answers, which are none, and leaving
    /// it asks the live feed again.
    #[test]
    fn market_replay_draws_no_live_feed_targets() {
        use crate::data::{MarketEvent, ReplayDriver, ReplayEvent};
        use crate::model::{Instrument, Tick};
        use chrono::{TimeZone, Utc};
        let ctx = egui::Context::default();
        let feed = Arc::new(Feed::default());
        let mut chart = crate::widget::ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("A")
            .build();
        chart.set_timeframe(crate::model::Timeframe::Day1);
        chart.set_price_targets_visible(true);
        chart.pump_targets(&ctx);
        let (_, reply) = feed.0.lock().unwrap().pop().unwrap();
        reply.send(Ok(targets()));
        chart.pump_targets(&ctx);
        assert_eq!(chart.price_targets(), Some(&targets()), "live targets held");
        // Two sessions recorded at 100 in March 2024, after the estimates were published, so
        // that only the replay's own feed decides what is drawn.
        let days = [
            Utc.with_ymd_and_hms(2024, 3, 4, 14, 30, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 3, 5, 14, 30, 0).unwrap(),
        ];
        let events = days
            .into_iter()
            .enumerate()
            .map(|(id, time)| {
                let fill = Tick::new(time, 100.0, 1.0, None).with_id(id as u64);
                ReplayEvent::new("A", time, id as u64, MarketEvent::Trade(fill))
            })
            .collect();
        let instruments = vec![Instrument::new("A").with_tick_size(0.01)];
        let driver = Arc::new(ReplayDriver::new(events, instruments).unwrap());
        driver.set_speed(0.0).unwrap();
        driver.seek(days[1]);
        chart.set_market_replay(Some(driver));
        for time in [0.0, 0.1, 0.2, 0.3] {
            test_frames::frame(&ctx, &mut chart, time, vec![]);
        }
        assert!(!chart.bars(SeriesId::MAIN).is_empty(), "replay bars");
        let horizon = chart
            .time_to_index(targets().horizon.and_hms_opt(0, 0, 0).unwrap().and_utc())
            .unwrap() as f32;
        chart.set_visible_logical_range(crate::model::LogicalRange::new(0.0, horizon + 200.0));
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.4, vec![]);
        assert!(
            !shapes.iter().any(
                |s| matches!(s, egui::Shape::Text(t) if t.galley.text().starts_with("High 120"))
            ),
            "live feed targets over the replay"
        );
        assert!(
            chart.target_prices().is_empty(),
            "live targets fit the scale"
        );
        assert_eq!(chart.price_targets(), None, "the driver answers none");
        chart.set_market_replay(None);
        chart.pump_targets(&ctx);
        let (asked, reply) = feed.0.lock().unwrap().pop().expect("live feed asked again");
        assert_eq!(asked, "A");
        reply.send(Ok(targets()));
        chart.pump_targets(&ctx);
        assert_eq!(
            chart.price_targets(),
            Some(&targets()),
            "live targets after the replay"
        );
    }

    /// A changed conversion drops the targets and the late reply of the previous choice, as it
    /// drops the bars, and asks the feed in the chosen currency.
    #[test]
    fn conversion_drops_targets_of_the_previous_currency() {
        let ctx = egui::Context::default();
        let feed = Arc::new(Feed::default());
        let mut chart = crate::widget::ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("A")
            .build();
        chart.set_price_targets_visible(true);
        chart.pump_targets(&ctx);
        let (_, late) = feed.0.lock().unwrap().pop().unwrap();
        chart.set_conversion(Some("EUR".into()));
        late.send(Ok(targets()));
        chart.pump_targets(&ctx);
        assert_eq!(
            chart.price_targets(),
            None,
            "a reply to the previous choice"
        );
        let (asked, reply) = feed.0.lock().unwrap().pop().unwrap();
        assert_eq!(asked, "A in EUR");
        reply.send(Ok(targets()));
        chart.pump_targets(&ctx);
        assert_eq!(chart.price_targets(), Some(&targets()), "answered in EUR");
        chart.set_conversion(None);
        assert_eq!(
            chart.price_targets(),
            None,
            "EUR targets in the own currency"
        );
        chart.pump_targets(&ctx);
        assert_eq!(feed.0.lock().unwrap().pop().unwrap().0, "A");
    }
}
