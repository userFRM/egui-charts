//! Host-owned feeds attached as native comparison series, and a second instrument drawn as
//! candles in a pane of its own.

use super::{DemoApp, DemoSymbol, LiveFeed};
use egui_charts::model::{ChartType, SeriesScale, Timeframe};
use egui_charts::ui::top_toolbar::{CompareAction, CompareSymbol};
use egui_charts::widget::{Chart, PaneId, SeriesId, SeriesOptions};
use std::sync::Arc;

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
) -> Result<Vec<SavedComparison>, String> {
    let panes = chart.panes();
    comparisons
        .iter()
        .filter_map(|(symbol, id, _)| {
            let (_, options) = chart.series().find(|(current, _)| current == id)?;
            Some(if options.style.autoscale.is_some() {
                Err("Could not save a comparison with a custom autoscale callback".into())
            } else {
                Ok(SavedComparison {
                    symbol: symbol.clone(),
                    options: options.clone(),
                    visible: chart.is_series_visible(*id),
                    pane: chart
                        .series_pane(*id)
                        .and_then(|pane| panes.iter().position(|p| *p == pane))
                        .unwrap_or(0),
                    scale: chart
                        .series_price_scale(*id)
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
            || !DemoSymbol::catalog()
                .iter()
                .any(|symbol| symbol.symbol == entry.symbol)
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
    saved: Vec<SavedComparison>,
) {
    let panes = chart.panes();
    for entry in saved {
        let feed = Arc::new(LiveFeed::new(&entry.symbol, chrono::Utc::now()));
        let id = chart.add_series(entry.options);
        if let Some(pane) = panes.get(entry.pane) {
            chart.move_series_to_pane(id, *pane);
        }
        refresh(chart, id, &feed);
        chart.set_series_visible(id, entry.visible);
        *chart
            .series_price_scale_mut(id)
            .expect("restored comparison") = entry.scale;
        comparisons.push((entry.symbol, id, feed));
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
        let Some((_, options)) = source.series().find(|(current, _)| *current == *id) else {
            continue;
        };
        let feed = Arc::new(feed.fork());
        let next = chart.add_series(options.clone());
        if let Some(pane) = source.series_pane(*id).filter(|pane| *pane != PaneId::MAIN) {
            let target = chart.add_pane();
            chart.move_series_to_pane(next, target);
            if let Some(factor) = source.pane_stretch_factor(pane) {
                chart.set_pane_stretch_factor(target, factor);
            }
            if let Some(at) = source.panes().iter().position(|item| *item == pane) {
                chart.move_pane(target, at);
            }
        }
        chart.set_data(next, source.bars(*id).to_vec());
        chart.set_series_visible(next, source.is_series_visible(*id));
        *chart.series_price_scale_mut(next).expect("copied series") = source
            .series_price_scale(*id)
            .expect("source series")
            .clone();
        copied.push((symbol.clone(), next, feed));
    }
    copied
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
            CompareAction::AddSymbol(symbol) => {
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
                let id = self.chart.add_comparison(symbol.clone(), color);
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
