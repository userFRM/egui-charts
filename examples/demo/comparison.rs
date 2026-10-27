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
}

impl Spread {
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
        }
    }

    pub(super) fn copy(&self, chart: &mut Chart, source: &Chart) -> Option<Self> {
        Some(Self {
            expression: self.expression.clone(),
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
    width: f32,
    scale: SeriesScale,
    columns_base: Option<f64>,
    #[serde(with = "SavedStyle")]
    style: egui_charts::model::SeriesStyle,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "egui_charts::model::SeriesStyle")]
struct SavedStyle {
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
    let timeframe = chart.timeframe().unwrap_or(Timeframe::Hour1);
    let bars = feed.snapshot(timeframe, chart.bar_clock(), chart.extended_hours());
    chart.set_data(id, bars);
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

pub(super) fn refresh_spreads(chart: &mut Chart, spreads: &mut Vec<Spread>, paused: bool) {
    spreads.retain(|spread| chart.series().any(|(id, _)| id == spread.id));
    let timeframe = chart.timeframe().unwrap_or(Timeframe::Hour1);
    for spread in spreads {
        let histories: Vec<_> = spread
            .feeds
            .iter()
            .map(|feed| {
                feed.set_paused(paused);
                feed.advance(chrono::Utc::now());
                feed.snapshot(timeframe, chart.bar_clock(), chart.extended_hours())
            })
            .collect();
        let symbols = spread.expression.symbols();
        let history = |symbol: &str| {
            let at = symbols.iter().position(|current| *current == symbol)?;
            Some(&histories[at][..])
        };
        if let Ok(bars) = spread.expression.bars(history) {
            chart.set_data(spread.id, bars);
        }
    }
}

impl DemoApp {
    pub(super) fn sync_comparisons(&mut self) {
        self.comparisons
            .retain(|(_, id, _)| self.chart.series().any(|(current, _)| current == *id));
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
        let id = self.chart.add_series(SeriesOptions {
            name: symbol.to_owned(),
            chart_type: ChartType::Candles,
            color: self.theme.ui.info,
            up_color: self.theme.ui.info,
            down_color: self.theme.ui.warning,
            ..Default::default()
        });
        let pane = self.chart.add_pane();
        self.chart.move_series_to_pane(id, pane);
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
                        .comparisons
                        .iter()
                        .any(|(current, _, _)| current == symbol)
            })
            .map(str::to_owned)
        else {
            return;
        };
        let feed = Arc::new(LiveFeed::new(&symbol, chrono::Utc::now()));
        let id = self.chart.add_series(SeriesOptions {
            name: symbol.clone(),
            chart_type: ChartType::Candles,
            color: self.theme.ui.info,
            up_color: self.theme.ui.info,
            down_color: self.theme.ui.warning,
            scale: SeriesScale::Left,
            ..Default::default()
        });
        refresh(&mut self.chart, id, &feed);
        let pane = self.chart.add_pane();
        self.chart.move_series_to_pane(id, pane);
        // Half the price pane's default share of the height.
        self.chart.set_pane_stretch_factor(pane, 1.5);
        self.chart.move_pane(pane, 0);
        self.comparisons.push((symbol, id, feed));
        self.grid_edits = self.grid_edits.wrapping_add(1);
        self.sync_comparisons();
    }

    pub(super) fn handle_comparison(&mut self, action: CompareAction) {
        match action {
            CompareAction::AddSymbol(symbol, placement) => {
                if self
                    .comparisons
                    .iter()
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
                let color = colors[self.comparisons.len() % colors.len()];
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
                let id = match scale {
                    Some(scale) => self.chart.add_series(SeriesOptions {
                        name: symbol.clone(),
                        color,
                        scale,
                        ..Default::default()
                    }),
                    None => self.chart.add_comparison(symbol.clone(), color),
                };
                if placement == ComparePlacement::NewPane {
                    let pane = self.chart.add_pane();
                    self.chart.move_series_to_pane(id, pane);
                }
                refresh(&mut self.chart, id, &feed);
                self.comparisons.push((symbol, id, feed));
                self.grid_edits = self.grid_edits.wrapping_add(1);
            }
            CompareAction::RemoveSymbol(symbol) => {
                if let Some((_, id, _)) = self
                    .comparisons
                    .iter()
                    .find(|(current, _, _)| *current == symbol)
                {
                    self.chart.remove_series(*id);
                    self.grid_edits = self.grid_edits.wrapping_add(1);
                }
            }
            CompareAction::ToggleVisibility(symbol) => {
                if let Some((_, id, _)) = self
                    .comparisons
                    .iter()
                    .find(|(current, _, _)| *current == symbol)
                {
                    let visible = self.chart.is_series_visible(*id);
                    self.chart.set_series_visible(*id, !visible);
                    self.grid_edits = self.grid_edits.wrapping_add(1);
                }
            }
            CompareAction::None | CompareAction::Close => {}
            _ => {}
        }
        self.sync_comparisons();
    }
}
