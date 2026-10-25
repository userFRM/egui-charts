//! Host-owned feeds attached as native comparison series, and a second instrument drawn as
//! candles in a pane of its own.

use super::{DemoApp, DemoSymbol, LiveFeed};
use egui_charts::model::{BarClock, ChartType, SeriesScale, Timeframe};
use egui_charts::ui::top_toolbar::{CompareAction, CompareSymbol};
use egui_charts::widget::{Chart, PaneId, SeriesId, SeriesOptions};
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
