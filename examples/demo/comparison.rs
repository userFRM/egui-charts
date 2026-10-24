//! Host-owned feeds attached as native comparison series, and a second instrument drawn as
//! candles in its own pane or the main one.

use super::{DemoApp, DemoSymbol, LiveFeed};
use egui_charts::model::{BarClock, ChartType, SeriesScale, Timeframe};
use egui_charts::ui::top_toolbar::{CompareAction, CompareSymbol};
use egui_charts::widget::{Chart, SeriesId, SeriesOptions};
use std::sync::Arc;

pub(super) fn refresh(chart: &mut Chart, id: SeriesId, feed: &LiveFeed) {
    let timeframe = chart.timeframe().unwrap_or(Timeframe::Hour1);
    let bars = feed.snapshot(timeframe, BarClock::new(timeframe), chart.extended_hours());
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
        if source.series_pane(*id) == Some(*id) {
            chart.move_series_to_pane(next, next);
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

    /// Shows a second instrument as candles on its own scale, in a pane of its own or in the
    /// main pane, adding it on first use.
    pub(super) fn place_second_instrument(&mut self, own_pane: bool) {
        let existing = self.comparisons.iter().map(|(_, id, _)| *id).find(|id| {
            self.chart.series().any(|(current, options)| {
                current == *id && options.chart_type == ChartType::Candles
            })
        });
        let id = match existing {
            Some(id) => id,
            None => {
                let Some(spec) = DemoSymbol::catalog().iter().find(|spec| {
                    spec.symbol != self.chart.symbol()
                        && !self
                            .comparisons
                            .iter()
                            .any(|(symbol, _, _)| symbol == spec.symbol)
                }) else {
                    return;
                };
                let feed = Arc::new(LiveFeed::new(spec.symbol, chrono::Utc::now()));
                let id = self.chart.add_series(SeriesOptions {
                    name: spec.symbol.into(),
                    chart_type: ChartType::Candles,
                    color: self.theme.ui.info,
                    up_color: self.theme.ui.info,
                    down_color: self.theme.ui.warning,
                    scale: SeriesScale::Left,
                    ..Default::default()
                });
                refresh(&mut self.chart, id, &feed);
                self.comparisons.push((spec.symbol.into(), id, feed));
                id
            }
        };
        self.chart
            .move_series_to_pane(id, if own_pane { id } else { SeriesId::MAIN });
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
            }
            CompareAction::RemoveSymbol(symbol) => {
                if let Some((_, id, _)) = self
                    .comparisons
                    .iter()
                    .find(|(current, _, _)| *current == symbol)
                {
                    self.chart.remove_series(*id);
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
                }
            }
            CompareAction::None | CompareAction::Close => {}
        }
        self.sync_comparisons();
    }
}
