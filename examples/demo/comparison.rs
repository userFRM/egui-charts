//! Host-owned feeds attached as native comparison series, and a second instrument drawn as
//! candles in a pane of its own.

use super::{DemoApp, DemoSymbol, LiveFeed};
use egui_charts::model::{ChartType, SeriesScale, Timeframe};
use egui_charts::services::SymbolExpression;
use egui_charts::ui::top_toolbar::{CompareAction, ComparePlacement, CompareSymbol};
use egui_charts::widget::{Chart, PaneId, SeriesId, SeriesOptions};
use std::sync::Arc;

pub(super) struct Spread {
    expression: SymbolExpression,
    id: SeriesId,
    feeds: Vec<Arc<LiveFeed>>,
    snapshot: Option<Vec<super::live::ComparisonSnapshot>>,
    replay_time: Option<chrono::DateTime<chrono::Utc>>,
}

impl Spread {
    pub(super) fn loading(&self, chart: &Chart) -> bool {
        chart.series_order(self.id).is_some() && self.feeds.iter().any(|feed| feed.loading())
    }

    fn new(expression: SymbolExpression, id: SeriesId) -> Self {
        let now = chrono::Utc::now();
        let feeds = expression
            .symbols()
            .into_iter()
            .map(|symbol| Arc::new(LiveFeed::new(symbol, now)))
            .collect();
        Self {
            expression,
            id,
            feeds,
            snapshot: None,
            replay_time: None,
        }
    }

    pub(super) fn copy(&self, chart: &mut Chart, source: &Chart) -> Option<Self> {
        Some(Self {
            expression: self.expression.clone(),
            snapshot: None,
            replay_time: None,
            id: copy_series(chart, source, self.id)?,
            feeds: self
                .feeds
                .iter()
                .map(|feed| Arc::new(feed.fork()))
                .collect(),
        })
    }
}

/// Host series metadata kept beside the library's chart snapshots.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SavedComparison {
    symbol: String,
    #[serde(with = "SavedOptions")]
    options: SeriesOptions,
    visible: bool,
    pane: usize,
    scale: egui_charts::scales::PriceScale,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "SeriesOptions")]
struct SavedOptions {
    name: String,
    chart_type: ChartType,
    color: egui::Color32,
    up_color: egui::Color32,
    down_color: egui::Color32,
    #[serde(default)]
    candle_border_width: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    baseline_style: Option<egui_charts::model::BaselineStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line_style: Option<egui_charts::model::LinePlotStyle>,
    #[serde(default)]
    bars_style: egui_charts::model::BarsStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    area_style: Option<egui_charts::model::AreaStyle>,
    #[serde(default)]
    last_price_animation: egui_charts::model::LastPriceAnimation,
    width: f32,
    #[serde(default)]
    marker_radius: Option<f32>,
    scale: SeriesScale,
    columns_base: Option<f64>,
    #[serde(with = "SavedStyle")]
    style: egui_charts::model::SeriesStyle,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "egui_charts::model::SeriesStyle")]
struct SavedStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display_format: Option<egui_charts::model::SeriesDisplayFormat>,
    last_value_visible: bool,
    price_line_visible: bool,
    #[serde(with = "SavedPriceLineSource")]
    price_line_source: egui_charts::model::PriceLineSource,
    price_line_color: Option<egui::Color32>,
    price_line_width: f32,
    price_line_style: egui_charts::model::LineStyle,
    base_line_visible: bool,
    #[serde(skip)]
    autoscale: Option<egui_charts::model::AutoscaleProvider>,
    hit_tolerance: f32,
    #[serde(default)]
    alerts: egui_charts::model::SeriesAlerts,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "egui_charts::model::PriceLineSource")]
enum SavedPriceLineSource {
    LastBar,
    LastVisible,
}

pub(super) fn capture(
    chart: &Chart,
    comparisons: &[(String, SeriesId, Arc<LiveFeed>)],
    spreads: &[Spread],
) -> Result<Vec<SavedComparison>, String> {
    let panes = chart.panes();
    comparisons
        .iter()
        .map(|(symbol, id, _)| (symbol.clone(), *id))
        .chain(
            spreads
                .iter()
                .map(|spread| (spread.expression.to_string(), spread.id)),
        )
        .filter_map(|(symbol, id)| {
            let (_, options) = chart.series().find(|(current, _)| *current == id)?;
            Some(if options.style.autoscale.is_some() {
                Err("Could not save a comparison with a custom autoscale callback".into())
            } else {
                Ok(SavedComparison {
                    symbol,
                    options: options.clone(),
                    visible: chart.is_series_visible(id),
                    pane: chart
                        .series_pane(id)
                        .and_then(|pane| panes.iter().position(|p| *p == pane))
                        .unwrap_or(0),
                    scale: chart
                        .series_price_scale(id)
                        .expect("comparison scale")
                        .clone(),
                })
            })
        })
        .collect()
}

pub(super) fn validate(saved: &[SavedComparison], panes: usize) -> Result<(), String> {
    if saved.iter().any(|entry| {
        entry.pane >= panes
            || !SymbolExpression::parse(&entry.symbol).is_ok_and(|expression| {
                expression.symbols().iter().all(|component| {
                    DemoSymbol::catalog()
                        .iter()
                        .any(|symbol| symbol.symbol == *component)
                })
            })
            || !entry.options.width.is_finite()
            || entry.options.width <= 0.0
            || entry
                .options
                .columns_base
                .is_some_and(|base| !base.is_finite())
            || !entry.scale.range().min.is_finite()
            || !entry.scale.range().max.is_finite()
            || entry.scale.range().min >= entry.scale.range().max
    }) {
        return Err("Could not open comparisons: invalid series metadata".into());
    }
    Ok(())
}

pub(super) fn restore(
    chart: &mut Chart,
    comparisons: &mut Vec<(String, SeriesId, Arc<LiveFeed>)>,
    spreads: &mut Vec<Spread>,
    saved: Vec<SavedComparison>,
) {
    let panes = chart.panes();
    for entry in saved {
        let id = chart.add_series(entry.options);
        if let Some(pane) = panes.get(entry.pane) {
            chart.move_series_to_pane(id, *pane);
        }
        if DemoSymbol::catalog()
            .iter()
            .any(|symbol| symbol.symbol == entry.symbol)
        {
            let feed = Arc::new(LiveFeed::new(&entry.symbol, chrono::Utc::now()));
            refresh(chart, id, &feed);
            comparisons.push((entry.symbol, id, feed));
        } else {
            let expression = SymbolExpression::parse(&entry.symbol).expect("validated comparison");
            spreads.push(Spread::new(expression, id));
        }
        chart.set_series_visible(id, entry.visible);
        *chart
            .series_price_scale_mut(id)
            .expect("restored comparison") = entry.scale;
    }
    refresh_spreads(chart, spreads, false);
}

pub(super) fn clear(
    chart: &mut Chart,
    comparisons: &mut Vec<(String, SeriesId, Arc<LiveFeed>)>,
    spreads: &mut Vec<Spread>,
) {
    for id in comparisons
        .drain(..)
        .map(|(_, id, _)| id)
        .chain(spreads.drain(..).map(|spread| spread.id))
    {
        chart.remove_series(id);
    }
}

pub(super) fn refresh(chart: &mut Chart, id: SeriesId, feed: &LiveFeed) {
    if chart.series_order(id).is_none() {
        return;
    }
    feed.refresh_comparison(chart, id);
}

pub(super) fn copy(
    chart: &mut Chart,
    source: &Chart,
    comparisons: &[(String, SeriesId, Arc<LiveFeed>)],
) -> Vec<(String, SeriesId, Arc<LiveFeed>)> {
    let mut copied = Vec::new();
    for (symbol, id, feed) in comparisons {
        let Some(next) = copy_series(chart, source, *id) else {
            continue;
        };
        let feed = Arc::new(feed.fork());
        copied.push((symbol.clone(), next, feed));
    }
    copied
}

fn copy_series(chart: &mut Chart, source: &Chart, id: SeriesId) -> Option<SeriesId> {
    let (_, options) = source.series().find(|(current, _)| *current == id)?;
    let next = chart.add_series(options.clone());
    if let Some(pane) = source.series_pane(id).filter(|pane| *pane != PaneId::MAIN) {
        let target = chart.add_pane();
        chart.move_series_to_pane(next, target);
        if let Some(factor) = source.pane_stretch_factor(pane) {
            chart.set_pane_stretch_factor(target, factor);
        }
        if let Some(at) = source.panes().iter().position(|item| *item == pane) {
            chart.move_pane(target, at);
        }
    }
    chart.set_data(next, source.bars(id).to_vec());
    chart.set_series_visible(next, source.is_series_visible(id));
    *chart.series_price_scale_mut(next).expect("copied series") = source
        .series_price_scale(id)
        .expect("source series")
        .clone();
    Some(next)
}

pub(super) fn refresh_spreads(chart: &mut Chart, spreads: &mut [Spread], paused: bool) {
    let timeframe = chart.timeframe().unwrap_or(Timeframe::Hour1);
    let clock = chart.bar_clock();
    let extended_hours = chart.extended_hours();
    // Candle replay windows supplied series; market replay leaves host series to the host.
    let replay_time = chart.replay_driver().and_then(|_| chart.replay_time());
    let session = if !clock.has_session() {
        egui_charts::ui::timeframe_toolbar::SessionType::Full24x7
    } else if extended_hours {
        egui_charts::ui::timeframe_toolbar::SessionType::ETH
    } else {
        egui_charts::ui::timeframe_toolbar::SessionType::RTH
    };
    for spread in spreads {
        if chart.series_order(spread.id).is_none() {
            continue;
        }
        for feed in &spread.feeds {
            feed.set_view(session, chart.conversion() == Some("adjusted"));
            feed.set_paused(paused);
            feed.advance(chrono::Utc::now());
        }
        if spread.loading(chart) {
            continue;
        }
        let key: Vec<_> = spread
            .feeds
            .iter()
            .map(|feed| feed.comparison_snapshot(chart, spread.id))
            .collect();
        if spread.snapshot.as_ref() == Some(&key) && spread.replay_time == replay_time {
            continue;
        }
        let histories: Vec<_> = spread
            .feeds
            .iter()
            .map(|feed| {
                let mut bars =
                    feed.snapshot_before(timeframe, clock.clone(), extended_hours, replay_time);
                if let Some(time) = replay_time {
                    let end = bars.partition_point(|bar| bar.time < clock.bar_open(time));
                    bars.truncate(end);
                }
                bars
            })
            .collect();
        let symbols = spread.expression.symbols();
        let history = |symbol: &str| {
            let at = symbols.iter().position(|current| *current == symbol)?;
            Some(&histories[at][..])
        };
        if let Ok(bars) = spread.expression.bars(history) {
            let loaded = chart.bars(spread.id);
            let tail = loaded.len().saturating_sub(1);
            if !loaded.is_empty()
                && (replay_time.is_some() || chart.replay_time().is_none())
                && bars.len() >= loaded.len()
                && bars[..tail] == loaded[..tail]
            {
                let start = if bars[tail] == loaded[tail] {
                    tail + 1
                } else {
                    tail
                };
                for bar in bars.into_iter().skip(start) {
                    chart.update(spread.id, bar);
                }
            } else {
                chart.set_data(spread.id, bars);
            }
            spread.replay_time = replay_time;
            spread.snapshot = Some(
                spread
                    .feeds
                    .iter()
                    .map(|feed| feed.comparison_snapshot(chart, spread.id))
                    .collect(),
            );
        }
    }
}

impl DemoApp {
    /// The comparisons on the chart, without those an undoable edit removed.
    fn live_comparisons(&self) -> impl Iterator<Item = &(String, SeriesId, Arc<LiveFeed>)> {
        self.comparisons
            .iter()
            .filter(|(_, id, _)| self.chart.series_order(*id).is_some())
    }

    pub(super) fn sync_comparisons(&mut self) {
        // shortcut: removed entries stay until history is empty, including those past its
        // 100-step cap; prune per step if retaining their feeds becomes costly.
        if !self.chart.can_undo() && !self.chart.can_redo() {
            let chart = &self.chart;
            self.comparisons
                .retain(|(_, id, _)| chart.series_order(*id).is_some());
            self.spreads
                .retain(|spread| chart.series_order(spread.id).is_some());
        }
        for (_, id, feed) in &self.comparisons {
            refresh(&mut self.chart, *id, feed);
        }
        self.compare_dialog.compared_symbols = self
            .comparisons
            .iter()
            .filter_map(|(symbol, id, _)| {
                let (_, series) = self.chart.series().find(|(current, _)| current == id)?;
                let spec = DemoSymbol::find(symbol);
                let mut entry =
                    CompareSymbol::new(symbol, spec.company_name, spec.exchange, series.color);
                entry.visible = self.chart.is_series_visible(*id);
                Some(entry)
            })
            .collect();
        refresh_spreads(
            &mut self.chart,
            &mut self.spreads,
            self.live.paused() || self.replay.is_some(),
        );
    }

    /// Charts a spread picked in symbol search, such as `AAPL-MSFT`, as candles in a pane of its
    /// own, from feeds of its symbols.
    pub(super) fn add_spread(&mut self, symbol: &str) {
        let Ok(expression) = SymbolExpression::parse(symbol) else {
            return;
        };
        let options = SeriesOptions {
            name: symbol.to_owned(),
            chart_type: ChartType::Candles,
            color: self.theme.ui.info,
            up_color: self.theme.ui.info,
            down_color: self.theme.ui.warning,
            ..Default::default()
        };
        let id = self.chart.user_action(|chart| {
            let id = chart.add_series(options);
            let pane = chart.add_pane();
            chart.move_series_to_pane(id, pane);
            id
        });
        self.spreads.push(Spread::new(expression, id));
        self.grid_edits = self.grid_edits.wrapping_add(1);
        self.sync_comparisons();
    }

    /// Feeds another symbol as candles in a pane of its own, above the price pane, on the left
    /// scale once moved to the price pane.
    pub(super) fn add_series_pane(&mut self) {
        let Some(symbol) = DemoSymbol::catalog()
            .iter()
            .map(|spec| spec.symbol)
            .find(|symbol| {
                *symbol != self.chart.symbol()
                    && !self
                        .live_comparisons()
                        .any(|(current, _, _)| current == symbol)
            })
            .map(str::to_owned)
        else {
            self.notify("All demo symbols are already shown. Remove one before adding a pane.");
            return;
        };
        let feed = Arc::new(LiveFeed::new(&symbol, chrono::Utc::now()));
        let options = SeriesOptions {
            name: symbol.clone(),
            chart_type: ChartType::Candles,
            color: self.theme.ui.info,
            up_color: self.theme.ui.info,
            down_color: self.theme.ui.warning,
            scale: SeriesScale::Left,
            ..Default::default()
        };
        let id = self.chart.user_action(|chart| {
            let id = chart.add_series(options);
            refresh(chart, id, &feed);
            let pane = chart.add_pane();
            chart.move_series_to_pane(id, pane);
            // Half the price pane's default share of the height.
            chart.set_pane_stretch_factor(pane, 1.5);
            chart.move_pane(pane, 0);
            id
        });
        self.comparisons.push((symbol, id, feed));
        self.grid_edits = self.grid_edits.wrapping_add(1);
        self.sync_comparisons();
    }

    pub(super) fn handle_comparison(&mut self, action: CompareAction) {
        match action {
            CompareAction::AddSymbol(symbol, placement) => {
                if self
                    .live_comparisons()
                    .any(|(current, _, _)| *current == symbol)
                    || !DemoSymbol::catalog()
                        .iter()
                        .any(|spec| spec.symbol == symbol)
                {
                    return;
                }
                let feed = Arc::new(LiveFeed::new(&symbol, chrono::Utc::now()));
                let colors = [
                    self.theme.ui.info,
                    self.theme.ui.warning,
                    self.theme.ui.success,
                ];
                let color = colors[self.live_comparisons().count() % colors.len()];
                let scale = match placement {
                    // The lowest additional scale id no series uses yet.
                    ComparePlacement::NewPriceScale => {
                        (1..).map(SeriesScale::Separate).find(|scale| {
                            self.chart.series_scale(SeriesId::MAIN) != Some(*scale)
                                && !self
                                    .chart
                                    .series()
                                    .any(|(_, series)| series.scale == *scale)
                        })
                    }
                    ComparePlacement::NewPane => Some(SeriesScale::Right),
                    _ => None,
                };
                let id = self.chart.user_action(|chart| {
                    let id = match scale {
                        Some(scale) => chart.add_series(SeriesOptions {
                            name: symbol.clone(),
                            color,
                            scale,
                            ..Default::default()
                        }),
                        None => chart.add_comparison(symbol.clone(), color),
                    };
                    if placement == ComparePlacement::NewPane {
                        let pane = chart.add_pane();
                        chart.move_series_to_pane(id, pane);
                    }
                    refresh(chart, id, &feed);
                    id
                });
                self.comparisons.push((symbol, id, feed));
                self.grid_edits = self.grid_edits.wrapping_add(1);
            }
            CompareAction::RemoveSymbol(symbol) => {
                let id = self
                    .live_comparisons()
                    .find(|(current, _, _)| *current == symbol)
                    .map(|(_, id, _)| *id);
                if let Some(id) = id {
                    self.chart.user_action(|chart| chart.remove_series(id));
                    self.grid_edits = self.grid_edits.wrapping_add(1);
                }
            }
            CompareAction::ToggleVisibility(symbol) => {
                let id = self
                    .live_comparisons()
                    .find(|(current, _, _)| *current == symbol)
                    .map(|(_, id, _)| *id);
                if let Some(id) = id {
                    let visible = self.chart.is_series_visible(id);
                    self.chart
                        .user_action(|chart| chart.set_series_visible(id, !visible));
                    self.grid_edits = self.grid_edits.wrapping_add(1);
                }
            }
            CompareAction::None | CompareAction::Close => {}
            _ => {}
        }
        self.sync_comparisons();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pointer frames keep loaded comparisons intact; trades and view changes still refresh
    /// their bars, and a history restore that cleared the bars loads them again.
    #[test]
    fn unchanged_comparisons_do_not_reload_history() {
        use chrono::{DateTime, TimeDelta};
        use egui_charts::model::BarData;
        use egui_charts::widget::ChartEvent;

        let now = DateTime::parse_from_rfc3339("2026-10-23T14:30:00Z")
            .unwrap()
            .to_utc();
        for count in [1, 2, 5] {
            let mut chart = Chart::new(BarData::new());
            chart.set_timeframe(Timeframe::Min1);
            let feeds: Vec<_> = (0..count)
                .map(|_| {
                    let id = chart.add_comparison("EURUSD", egui::Color32::BLUE);
                    let feed = LiveFeed::new("EURUSD", now);
                    super::super::tests::finish_feed(&feed);
                    refresh(&mut chart, id, &feed);
                    assert!(!chart.bars(id).is_empty());
                    (id, feed)
                })
                .collect();
            chart.drain_events().for_each(drop);
            for (id, feed) in &feeds {
                feed.advance(now + TimeDelta::nanoseconds(1));
                refresh(&mut chart, *id, feed);
            }
            assert!(
                !chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::DataLoaded { .. })),
                "{count} comparisons must not reload on an unchanged frame"
            );
            for (id, feed) in &feeds {
                feed.advance(now + TimeDelta::seconds(2));
                refresh(&mut chart, *id, feed);
                assert_eq!(
                    chart.bars(*id),
                    feed.snapshot(Timeframe::Min1, chart.bar_clock(), false)
                );
            }
            chart.set_timeframe(Timeframe::Min5);
            for (id, feed) in &feeds {
                refresh(&mut chart, *id, feed);
                assert_eq!(
                    chart.bars(*id),
                    feed.snapshot(Timeframe::Min5, chart.bar_clock(), false)
                );
                chart.set_data(*id, Vec::new());
                refresh(&mut chart, *id, feed);
                assert!(!chart.bars(*id).is_empty());
            }
            chart.drain_events().for_each(drop);
            chart.set_extended_hours(true);
            for (id, feed) in &feeds {
                refresh(&mut chart, *id, feed);
                assert_eq!(
                    chart.bars(*id),
                    feed.snapshot(Timeframe::Min5, chart.bar_clock(), true)
                );
            }
            assert_eq!(
                chart
                    .drain_events()
                    .filter(|event| matches!(event, ChartEvent::DataLoaded { .. }))
                    .count(),
                count,
                "a session change reloads each comparison"
            );
            chart.set_replay_time(Some(DateTime::UNIX_EPOCH));
            for (id, feed) in &feeds {
                refresh(&mut chart, *id, feed);
                assert!(chart.bars(*id).is_empty());
            }
            chart.drain_events().for_each(drop);
            for (id, feed) in &feeds {
                refresh(&mut chart, *id, feed);
            }
            assert!(
                !chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::DataLoaded { .. })),
                "an empty replay window must not reload retained history"
            );
        }
    }

    /// Paused spreads leave loaded history intact, but trades, view changes and restoration
    /// still update it. Replay moves window the retained bars without rebuilding the expression.
    #[test]
    fn unchanged_spreads_do_not_reload_history() {
        use egui_charts::data::{MarketEvent, ReplayDriver, ReplayEvent};
        use egui_charts::model::{BarData, Instrument, Tick};
        use egui_charts::widget::ChartEvent;

        let mut chart = Chart::new(BarData::new());
        chart.set_timeframe(Timeframe::Min1);
        let id = chart.add_comparison("AAPL-MSFT", egui::Color32::BLUE);
        let now = "2026-10-23T14:30:00Z".parse().unwrap();
        let mut spreads = vec![Spread {
            expression: SymbolExpression::parse("AAPL-MSFT").unwrap(),
            id,
            feeds: ["AAPL", "MSFT"]
                .into_iter()
                .map(|symbol| Arc::new(LiveFeed::new(symbol, now)))
                .collect(),
            snapshot: None,
            replay_time: None,
        }];
        for feed in &spreads[0].feeds {
            super::super::tests::finish_feed(feed);
        }
        refresh_spreads(&mut chart, &mut spreads, true);
        assert!(!chart.bars(id).is_empty());
        let instrument = Instrument::new("SAMPLE")
            .with_tick_size(0.01)
            .with_time_zone(crate::sampledata::us_equity_zone())
            .with_session(crate::sampledata::us_equity_session());
        for sessions in [false, true] {
            chart.set_instrument(sessions.then(|| instrument.clone()));
            for timeframe in [Timeframe::Min1, Timeframe::Min5] {
                chart.set_timeframe(timeframe);
                for extended in [false, true] {
                    chart.set_extended_hours(extended);
                    refresh_spreads(&mut chart, &mut spreads, true);
                    let histories: Vec<_> = spreads[0]
                        .feeds
                        .iter()
                        .map(|feed| feed.snapshot(timeframe, chart.bar_clock(), extended))
                        .collect();
                    let expected = spreads[0]
                        .expression
                        .bars(|symbol| Some(&histories[usize::from(symbol == "MSFT")][..]))
                        .unwrap();
                    assert_eq!(chart.bars(id), expected);
                    if sessions {
                        assert!(chart.bars(id).iter().all(|bar| {
                            crate::sampledata::us_equity_hours().is_open(bar.time, extended)
                        }));
                    }

                    chart.drain_events().for_each(drop);
                    for _ in 0..3 {
                        refresh_spreads(&mut chart, &mut spreads, true);
                    }
                    assert!(
                        !chart
                            .drain_events()
                            .any(|event| matches!(event, ChartEvent::DataLoaded { .. })),
                        "unchanged spreads must not reload history"
                    );
                    for feed in &spreads[0].feeds {
                        feed.set_paused(false);
                        feed.advance(feed.now() + chrono::TimeDelta::seconds(2));
                    }
                    refresh_spreads(&mut chart, &mut spreads, true);
                    let last = chart.bars(id).last().unwrap();
                    let closes: Vec<_> = spreads[0]
                        .feeds
                        .iter()
                        .map(|feed| {
                            feed.snapshot(timeframe, chart.bar_clock(), extended)
                                .last()
                                .unwrap()
                                .close
                        })
                        .collect();
                    assert!((last.close - (closes[0] - closes[1])).abs() < 1e-9);
                    chart.set_data(id, Vec::new());
                    refresh_spreads(&mut chart, &mut spreads, true);
                    assert!(
                        !chart.bars(id).is_empty(),
                        "restore must reload cleared bars"
                    );
                }
            }
        }
        for time in [chrono::DateTime::UNIX_EPOCH, spreads[0].feeds[0].now()] {
            chart.set_replay_time(Some(time));
            chart.drain_events().for_each(drop);
            refresh_spreads(&mut chart, &mut spreads, true);
            chart.drain_events().for_each(drop);
            refresh_spreads(&mut chart, &mut spreads, true);
            assert!(
                !chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::DataLoaded { .. })),
                "a replay window must not repeatedly rebuild its source"
            );
        }
        chart.set_replay_time(None);
        refresh_spreads(&mut chart, &mut spreads, true);
        let history = chart.bars(id).to_vec();
        let events = [now - chrono::TimeDelta::minutes(30), now]
            .into_iter()
            .enumerate()
            .map(|(sequence, time)| {
                ReplayEvent::new(
                    "SAMPLE",
                    time,
                    sequence as u64,
                    MarketEvent::Trade(Tick::new(time, 100.0, 1.0, None)),
                )
            })
            .collect();
        let driver = Arc::new(ReplayDriver::new(events, vec![instrument.clone()]).unwrap());
        chart.set_market_replay(Some(driver.clone()));
        chart.set_instrument(Some(instrument));
        for time in [
            now - chrono::TimeDelta::minutes(20),
            now - chrono::TimeDelta::minutes(10),
            now - chrono::TimeDelta::minutes(30),
        ] {
            driver.seek(time);
            refresh_spreads(&mut chart, &mut spreads, true);
            let expected: Vec<_> = history
                .iter()
                .filter(|bar| {
                    bar.time + chrono::TimeDelta::minutes(5)
                        <= time + chrono::TimeDelta::nanoseconds(1)
                })
                .cloned()
                .collect();
            assert_eq!(
                chart.bars(id),
                expected,
                "market replay must hide future spread bars"
            );
            chart.drain_events().for_each(drop);
            refresh_spreads(&mut chart, &mut spreads, true);
            assert!(
                !chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::DataLoaded { .. })),
                "a paused market replay must keep its spread history"
            );
        }
        let trade = Timeframe::Ticks(std::num::NonZeroU64::MIN);
        let first: Vec<_> = spreads[0]
            .feeds
            .iter()
            .map(|feed| {
                feed.snapshot(trade, egui_charts::model::BarClock::new(trade), true)[0].clone()
            })
            .collect();
        assert_eq!(first[0].time, first[1].time);
        chart.set_timeframe(Timeframe::Ticks(std::num::NonZeroU64::new(1000).unwrap()));
        driver.seek(first[0].time + chrono::TimeDelta::nanoseconds(1));
        refresh_spreads(&mut chart, &mut spreads, true);
        assert_eq!(chart.bars(id).len(), 1);
        let bar = &chart.bars(id)[0];
        let expected = first[0].close - first[1].close;
        assert!(
            [bar.open, bar.high, bar.low, bar.close]
                .iter()
                .all(|value| (*value - expected).abs() < 1e-9),
            "a count bar cannot use executions after the cursor"
        );
        chart.set_timeframe(Timeframe::Min5);
        chart.set_market_replay(None);
        refresh_spreads(&mut chart, &mut spreads, true);
        assert_eq!(
            chart.bars(id),
            history,
            "exiting market replay restores the spread"
        );
    }

    fn listed(app: &DemoApp) -> Vec<&str> {
        app.compare_dialog
            .compared_symbols
            .iter()
            .map(|entry| entry.symbol.as_str())
            .collect()
    }

    /// Undo restores a removed comparison's feed and dialog entry; the removed symbol can
    /// also be compared anew, in its first palette colour, without its parked entry intercepting
    /// later dialog edits.
    #[test]
    fn undo_brings_back_a_removed_comparison_with_its_feed() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx));
        app.handle_comparison(CompareAction::AddSymbol("AAPL".into(), Default::default()));
        super::super::tests::finish_feed(&app.comparisons[0].2);
        let id = app.comparisons[0].1;
        let colour = |app: &DemoApp, id| {
            app.chart
                .series()
                .find(|(current, _)| *current == id)
                .unwrap()
                .1
                .color
        };
        let first = colour(&app, id);
        app.handle_comparison(CompareAction::RemoveSymbol("AAPL".into()));
        assert!(listed(&app).is_empty());

        assert!(app.chart.undo());
        app.chart.set_data(id, Vec::new());
        app.sync_comparisons();
        assert_eq!(listed(&app), ["AAPL"]);
        assert!(!app.chart.bars(id).is_empty());

        assert!(app.chart.redo());
        app.handle_comparison(CompareAction::AddSymbol("AAPL".into(), Default::default()));
        assert_eq!(listed(&app), ["AAPL"]);
        let again = app.comparisons.last().unwrap().1;
        assert_ne!(again, id);
        assert_eq!(colour(&app, again), first);
        app.handle_comparison(CompareAction::ToggleVisibility("AAPL".into()));
        assert!(!app.chart.is_series_visible(again));
        app.handle_comparison(CompareAction::RemoveSymbol("AAPL".into()));
        assert!(listed(&app).is_empty());
    }
}
