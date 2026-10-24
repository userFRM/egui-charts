//! Session-owned layouts and independent feeds for each chart slot.

use super::{DemoApp, LiveFeed};
use egui_charts::model::Timeframe;
use egui_charts::ui::top_toolbar::components::layout_menu::LayoutAction;
use egui_charts::widget::{Chart, ChartLayout, ChartLayoutMode, SeriesId};
use std::sync::Arc;

pub(super) struct DemoChart {
    pub chart: Chart,
    pub live: Arc<LiveFeed>,
    pub comparisons: Vec<(String, SeriesId, Arc<LiveFeed>)>,
}

pub(super) struct LayoutTab {
    pub name: String,
    workspace: Option<Workspace>,
    summary: String,
    change: f64,
}

struct Workspace {
    chart: Chart,
    live: Arc<LiveFeed>,
    comparisons: Vec<(String, SeriesId, Arc<LiveFeed>)>,
    extra: Vec<DemoChart>,
    layout: ChartLayout,
    slot: usize,
}

impl LayoutTab {
    pub fn unnamed() -> Self {
        Self {
            name: "Unnamed".into(),
            workspace: None,
            summary: String::new(),
            change: 0.0,
        }
    }
}

impl DemoApp {
    pub(super) fn toggle_theme(&mut self, ctx: &egui::Context) {
        self.theme = if self.theme.dark {
            egui_charts::theme::Theme::light()
        } else {
            egui_charts::theme::Theme::dark()
        };
        let saved = self
            .layout_tabs
            .iter_mut()
            .filter_map(|tab| tab.workspace.as_mut());
        for chart in std::iter::once(&mut self.chart)
            .chain(self.extra_charts.iter_mut().map(|c| &mut c.chart))
            .chain(saved.flat_map(|workspace| {
                std::iter::once(&mut workspace.chart)
                    .chain(workspace.extra.iter_mut().map(|c| &mut c.chart))
            }))
        {
            chart.set_theme(ctx, &self.theme);
        }
    }

    pub(super) fn ensure_chart_slots(&mut self, mode: ChartLayoutMode) {
        let count = if mode == ChartLayoutMode::Tabs {
            (self.extra_charts.len() + 1).max(2)
        } else {
            mode.chart_count()
        };
        while self.extra_charts.len() + 1 < count {
            let mut copy = self.copy_chart(&self.chart, &self.live, &self.comparisons);
            // New slots receive drawings through synchronization, which records their links.
            copy.chart.drawings_mut().drawings.clear();
            self.extra_charts.push(copy);
        }
        self.layout.mode = mode;
        if self.layout_slot >= count {
            self.activate_chart_slot(0);
        }
        self.top_toolbar.chart_grid_selector.set_layout(mode);
    }

    fn copy_chart(
        &self,
        source: &Chart,
        feed: &LiveFeed,
        comparisons: &[(String, SeriesId, Arc<LiveFeed>)],
    ) -> DemoChart {
        let live = Arc::new(feed.fork());
        live.set_symbol(source.symbol());
        let (session, adjusted) = feed.view(source.extended_hours());
        live.set_view(session, adjusted);
        let mut chart = egui_charts::ChartBuilder::new()
            .with_feed(live.clone())
            .with_symbol(source.symbol())
            .with_timeframe(source.timeframe().unwrap_or(Timeframe::Min1))
            .with_theme(self.theme.clone())
            .with_config(source.config().clone())
            .with_chart_options(source.chart_options().clone())
            .with_chart_type(source.chart_type())
            .with_visible_candles(0)
            .build();
        chart.set_main_series_visible(source.is_main_series_visible());
        chart.set_data(SeriesId::MAIN, source.bars(SeriesId::MAIN).to_vec());
        chart.set_instrument(Some(live.instrument()));
        chart.set_extended_hours(source.extended_hours());
        chart.set_chart_type_params(source.chart_type_params().clone());
        *chart.price_scale_mut() = source.state().price_scale().clone();
        let (spacing, offset) = source.get_time_scale_state();
        chart.apply_synced_time_scale(spacing, offset);
        let mut copied_comparisons = Vec::new();
        for (id, indicator) in source.indicators() {
            let copy = if let Some((symbol, _, feed)) =
                comparisons.iter().find(|(_, current, _)| *current == id)
            {
                let feed = Arc::new(feed.fork());
                let copy = chart.add_indicator(Box::new(super::comparison::Comparison::new(
                    symbol.clone(),
                    feed.clone(),
                    indicator.colors()[0],
                )));
                if let Some(study) = chart.indicator_mut(copy) {
                    study
                        .apply_settings(&[], indicator.settings())
                        .expect("existing comparison style");
                }
                chart.set_indicator_visible(copy, indicator.is_visible());
                copied_comparisons.push((symbol.clone(), copy, feed));
                copy
            } else {
                chart.add_indicator(indicator.clone_box())
            };
            if let Some(inputs) = source.indicator_inputs(id) {
                chart.set_indicator_inputs(copy, inputs);
            }
        }
        chart.drawings_mut().drawings = source.drawings().drawings.clone();
        for drawing in &mut chart.drawings_mut().drawings {
            drawing.stored_id = None;
            drawing.created_at = None;
        }
        DemoChart {
            chart,
            live,
            comparisons: copied_comparisons,
        }
    }

    pub(super) fn activate_chart_slot(&mut self, slot: usize) {
        if slot == self.layout_slot {
            return;
        }
        self.stop_replay();
        if self.footprint.take().is_some() {
            self.restore_feed_after_footprint();
        }
        // `chart` always holds the active slot so every existing toolbar and dialog acts on it.
        let other = if slot < self.layout_slot {
            slot
        } else {
            slot - 1
        };
        if let Some(target) = self.extra_charts.get_mut(other) {
            std::mem::swap(&mut self.chart, &mut target.chart);
            std::mem::swap(&mut self.live, &mut target.live);
            std::mem::swap(&mut self.comparisons, &mut target.comparisons);
            // Restore stable slot order after moving the former active chart into the vector.
            let old_position = if self.layout_slot < slot {
                self.layout_slot
            } else {
                self.layout_slot - 1
            };
            let moved = self.extra_charts.remove(other);
            self.extra_charts.insert(old_position, moved);
            self.layout_slot = slot;
            self.layout.set_active(slot);
            self.refresh_active_chart();
        }
    }

    fn refresh_active_chart(&mut self) {
        self.state.symbol = self.chart.symbol().to_owned();
        self.state.timeframe = self.chart.timeframe().unwrap_or(Timeframe::Min1);
        self.state.chart_type = self.chart.chart_type();
        self.bars = self.chart.bars(SeriesId::MAIN).to_vec();
        self.timeframe_toolbar.state.timeframe = self.state.timeframe;
        for control in [
            egui_charts::ui::Control::Session,
            egui_charts::ui::Control::DividendAdjustment,
        ] {
            self.timeframe_toolbar
                .set_control_visible(control, self.state.symbol == "SAMPLE");
        }
        let (session, adjusted) = self.live.view(self.chart.extended_hours());
        self.timeframe_toolbar.set_session(session);
        self.timeframe_toolbar.state.adjust_for_dividends = adjusted;
        use egui_charts::ui::top_toolbar::components::timeframe_selector::{
            Timeframe as Interval, TimeframeUnit as Unit,
        };
        let tf = self.state.timeframe;
        let seconds = tf.duration_ms() / 1000;
        let (value, unit) = match tf {
            Timeframe::Ticks(n) => (n.get(), Unit::Tick),
            Timeframe::Month1 => (1, Unit::Month),
            Timeframe::Week1 => (1, Unit::Week),
            _ if seconds >= 86400 && seconds % 86400 == 0 => (seconds as u64 / 86400, Unit::Day),
            _ if seconds >= 3600 && seconds % 3600 == 0 => (seconds as u64 / 3600, Unit::Hour),
            _ if seconds >= 60 && seconds % 60 == 0 => (seconds as u64 / 60, Unit::Minute),
            _ => (seconds.max(1) as u64, Unit::Second),
        };
        self.top_toolbar.timeframe_selector.current =
            Interval::new(value.min(u32::MAX as u64) as u32, unit);
        self.top_toolbar.chart_type_selector.current = self.state.chart_type;
    }

    pub(super) fn show_layout_tabs(&mut self, ui: &mut egui::Ui) {
        let bars = self.chart.bars(SeriesId::MAIN);
        let last = bars.last().map_or(0.0, |b| b.close);
        let previous = bars
            .get(bars.len().saturating_sub(2))
            .map_or(last, |b| b.close);
        let tab = &mut self.layout_tabs[self.layout_tab];
        tab.summary = format!("{}  {last:.2}", self.chart.symbol());
        tab.change = if previous == 0.0 {
            0.0
        } else {
            (last / previous - 1.0) * 100.0
        };
        let mut select = None;
        let mut close = None;
        let mut new = false;
        egui::Panel::top("layout_tabs")
            .exact_size(26.0)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(self.theme.ui.border))
            .show(ui, |ui| {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (index, tab) in self.layout_tabs.iter().enumerate() {
                            egui::Frame::new()
                                .fill(if index == self.layout_tab {
                                    self.theme.ui.panel_bg
                                } else {
                                    self.theme.ui.border
                                })
                                .inner_margin(egui::Margin::symmetric(6, 0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add(egui::Button::new(&tab.summary).frame(false))
                                            .on_hover_text(&tab.name)
                                            .clicked()
                                        {
                                            select = Some(index);
                                        }
                                        let color = if tab.change >= 0.0 {
                                            self.chart.config().bullish_color
                                        } else {
                                            self.chart.config().bearish_color
                                        };
                                        ui.colored_label(color, format!("{:+.2}%", tab.change));
                                        if ui
                                            .small_button("×")
                                            .on_hover_text("Close layout")
                                            .clicked()
                                        {
                                            close = Some(index);
                                        }
                                    });
                                });
                        }
                        new = ui.small_button("+").on_hover_text("New layout").clicked();
                    });
                });
            });
        if let Some(index) = select {
            self.select_layout(index);
        }
        if let Some(index) = close {
            self.close_layout(index);
        }
        if new {
            self.new_layout(None);
        }
    }

    fn select_layout(&mut self, index: usize) {
        if index == self.layout_tab {
            return;
        }
        self.stop_replay();
        if self.footprint.take().is_some() {
            self.restore_feed_after_footprint();
        }
        let Some(next) = self
            .layout_tabs
            .get_mut(index)
            .and_then(|t| t.workspace.take())
        else {
            return;
        };
        self.layout_tabs[self.layout_tab].workspace = Some(Workspace {
            chart: std::mem::replace(&mut self.chart, next.chart),
            live: std::mem::replace(&mut self.live, next.live),
            comparisons: std::mem::replace(&mut self.comparisons, next.comparisons),
            extra: std::mem::replace(&mut self.extra_charts, next.extra),
            layout: std::mem::replace(&mut self.layout, next.layout),
            slot: std::mem::replace(&mut self.layout_slot, next.slot),
        });
        self.layout_tab = index;
        self.refresh_active_chart();
        self.top_toolbar
            .chart_grid_selector
            .set_layout(self.layout.mode);
        self.top_toolbar.chart_grid_selector.sync = self.layout.sync.clone();
    }

    fn new_layout(&mut self, name: Option<String>) {
        let copy = self.copy_chart(&self.chart, &self.live, &self.comparisons);
        let duplicate = name.is_some();
        let layout = if duplicate {
            self.layout.clone()
        } else {
            ChartLayout::default()
        };
        let extra = if duplicate {
            self.extra_charts
                .iter()
                .map(|c| self.copy_chart(&c.chart, &c.live, &c.comparisons))
                .collect()
        } else {
            Vec::new()
        };
        let mut name = name.unwrap_or_else(|| "Unnamed".into());
        let base = name.clone();
        let mut suffix = 2;
        while self.layout_tabs.iter().any(|t| t.name == name) {
            name = format!("{base} {suffix}");
            suffix += 1;
        }
        self.layout_tabs.push(LayoutTab {
            name,
            summary: self.state.symbol.clone(),
            change: 0.0,
            workspace: Some(Workspace {
                chart: copy.chart,
                live: copy.live,
                comparisons: copy.comparisons,
                extra,
                layout,
                slot: if duplicate { self.layout_slot } else { 0 },
            }),
        });
        self.select_layout(self.layout_tabs.len() - 1);
    }

    fn close_layout(&mut self, index: usize) {
        if self.layout_tabs.len() == 1 {
            return;
        }
        if index == self.layout_tab {
            self.select_layout(if index == 0 { 1 } else { index - 1 });
        }
        self.layout_tabs.remove(index);
        if self.layout_tab > index {
            self.layout_tab -= 1;
        }
    }

    pub(super) fn handle_layout_action(&mut self, action: LayoutAction) {
        match action {
            LayoutAction::SelectGrid(mode) => self.ensure_chart_slots(mode),
            LayoutAction::CreateNew => self.new_layout(None),
            LayoutAction::SaveAs(name) => self.new_layout(Some(name)),
            LayoutAction::Open(name) => {
                if let Some(i) = self.layout_tabs.iter().position(|t| t.name == name) {
                    self.select_layout(i);
                }
            }
            LayoutAction::Delete(name) => {
                if let Some(i) = self.layout_tabs.iter().position(|t| t.name == name) {
                    self.close_layout(i);
                }
            }
            LayoutAction::Rename(old, new) => {
                if !new.trim().is_empty()
                    && !self.layout_tabs.iter().any(|t| t.name == new)
                    && let Some(tab) = self.layout_tabs.iter_mut().find(|t| t.name == old)
                {
                    tab.name = new;
                }
            }
            LayoutAction::Save => {
                self.notify("Layout kept for this session; persistent storage is host-owned")
            }
            LayoutAction::DownloadImage => self.export_dialog.open(),
            LayoutAction::None => {}
        }
    }

    pub(super) fn refresh_layout_menu(&mut self) {
        self.top_toolbar.layout_menu.curr_layout =
            Some(self.layout_tabs[self.layout_tab].name.clone());
        self.top_toolbar.layout_menu.layouts =
            self.layout_tabs.iter().map(|t| t.name.clone()).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_slots_and_layout_tabs_keep_independent_state() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let mut drawing = egui_charts::drawings::Drawing::new(
            7,
            egui_charts::drawings::DrawingToolType::HorizontalLine,
        );
        drawing.completed = true;
        drawing
            .chart_points
            .push(egui_charts::drawings::ChartPoint::new(10.0, 42.0));
        app.chart.drawings_mut().drawings.push(drawing);
        app.chart.set_main_series_visible(false);
        app.handle_comparison(super::super::CompareAction::AddSymbol("AAPL".into()));
        app.layout.sync.sync_drawings = true;
        app.ensure_chart_slots(ChartLayoutMode::Four);
        let frame = |app: &mut DemoApp| {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                app.show_chart(ui, app.theme.chart.bg)
            })
            .drop_without_applying_deltas();
            assert!(!app.chart.is_main_series_visible());
            assert!(
                app.extra_charts
                    .iter()
                    .all(|c| !c.chart.is_main_series_visible())
            );
            assert_eq!(app.chart.drawings().drawings.len(), 1);
            assert!(
                app.extra_charts
                    .iter()
                    .all(|c| c.chart.drawings().drawings.len() == 1)
            );
        };
        frame(&mut app);
        for slot in [1, 3, 0, 2] {
            app.activate_chart_slot(slot);
            app.chart.config_mut().candle_width = 0.5 + slot as f32 * 0.1;
            app.chart.config_mut().background_color = egui::Color32::from_gray(10 + slot as u8);
        }
        for slot in [3, 1, 2, 0] {
            app.activate_chart_slot(slot);
            assert_eq!(app.chart.config().candle_width, 0.5 + slot as f32 * 0.1);
            assert_eq!(app.state.symbol, app.chart.symbol());
            assert_eq!(
                app.chart.config().background_color,
                egui::Color32::from_gray(10 + slot as u8)
            );
        }
        app.handle_comparison(super::super::CompareAction::RemoveSymbol("AAPL".into()));
        app.activate_chart_slot(1);
        app.sync_comparisons();
        assert_eq!(app.compare_dialog.compared_symbols.len(), 1);
        app.activate_chart_slot(0);
        app.sync_comparisons();
        assert!(app.compare_dialog.compared_symbols.is_empty());
        let before = app.chart.get_time_scale_state();
        app.new_layout(Some("Saved".into()));
        assert_eq!(app.layout.mode, ChartLayoutMode::Four);
        assert_eq!(app.chart.get_time_scale_state(), before);
        frame(&mut app);
        app.chart.set_symbol("copy-only");
        app.select_layout(0);
        assert_eq!(app.chart.symbol(), "SAMPLE");
        assert_eq!(app.chart.config().candle_width, 0.5);
        app.select_layout(1);
        assert_eq!(app.chart.symbol(), "copy-only");
        app.toggle_theme(&ctx);
        let background = app.theme.chart.bg;
        app.select_layout(0);
        assert_eq!(app.chart.config().background_color, background);
        assert_eq!(app.chart.config().bullish_color, app.theme.chart.candle_up);
        assert_eq!(
            app.chart.config().bearish_wick_color,
            Some(app.theme.chart.candle_down)
        );
        assert_eq!(egui_charts::theme::ThemeExt::active_theme(&ctx), app.theme);
        app.select_layout(1);
        app.close_layout(1);
        assert_eq!(app.chart.symbol(), "SAMPLE");
        assert_eq!(app.chart.config().candle_width, 0.5);
        app.ensure_chart_slots(ChartLayoutMode::Single);
        app.ensure_chart_slots(ChartLayoutMode::Four);
        app.activate_chart_slot(3);
        assert_eq!(app.chart.config().candle_width, 0.8);
    }
}
