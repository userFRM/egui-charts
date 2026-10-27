//! Open layouts as tabs with independent feeds for each chart slot, saved to and opened from
//! the demo's storage, and the user settings kept beside them.

use super::{DemoApp, LiveFeed};
use egui_charts::model::Timeframe;
use egui_charts::services::UserSettings;
use egui_charts::ui::top_toolbar::components::layout_menu::LayoutAction;
use egui_charts::widget::{Chart, ChartLayout, ChartLayoutMode, SavedLayout, SeriesId};
use std::sync::Arc;

pub(super) struct DemoChart {
    pub chart: Chart,
    pub live: Arc<LiveFeed>,
    pub comparisons: Vec<(String, SeriesId, Arc<LiveFeed>)>,
    pub spreads: Vec<super::comparison::Spread>,
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
    spreads: Vec<super::comparison::Spread>,
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
        self.theme_choice = Some(!self.theme.dark);
        self.update_theme(ctx);
    }

    pub(super) fn update_theme(&mut self, ctx: &egui::Context) {
        let dark = self
            .theme_choice
            .or_else(|| ctx.system_theme().map(|theme| theme == egui::Theme::Dark))
            .unwrap_or(self.theme.dark);
        if self.theme.dark == dark {
            return;
        }
        self.theme = if dark {
            egui_charts::theme::Theme::dark()
        } else {
            egui_charts::theme::Theme::light()
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
            let mut copy =
                self.copy_chart(&self.chart, &self.live, &self.comparisons, &self.spreads);
            // New slots receive drawings through synchronization, which records their links.
            copy.chart.drawings_mut().drawings.clear();
            self.extra_charts.push(copy);
        }
        self.layout.mode = mode;
        if self.layout_slot >= count {
            self.activate_chart_slot(0);
        }
        self.top_toolbar.chart_grid_selector.set_layout(mode);
        self.refresh_strategy_signals();
    }

    fn copy_chart(
        &self,
        source: &Chart,
        feed: &LiveFeed,
        comparisons: &[(String, SeriesId, Arc<LiveFeed>)],
        spreads: &[super::comparison::Spread],
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
            .with_drawing_registry(source.drawings().registry.clone())
            .with_visible_candles(0)
            .build();
        chart.set_main_series_visible(source.is_main_series_visible());
        chart.set_price_targets_visible(source.price_targets_visible());
        if self.api_samples {
            chart.set_signal_source(Some(live.clone()));
        }
        chart.set_data(SeriesId::MAIN, source.state().data().bars.clone());
        chart.set_event_visibility(source.event_visibility().clone());
        chart.set_mark_source(Some(Arc::new(super::marks::DemoMarks {
            bars: chart.bars(SeriesId::MAIN).to_vec(),
            samples: self.api_samples,
            notifications: source
                .event_visibility()
                .show_news_notifications
                .then(|| self.notification_host.clone()),
        })));
        if let Some(scale) = source.series_scale(SeriesId::MAIN) {
            chart.set_series_scale(SeriesId::MAIN, scale);
        } else {
            chart.remove_series(SeriesId::MAIN);
        }
        chart.set_instrument(Some(live.instrument()));
        chart.set_extended_hours(source.extended_hours());
        chart.set_chart_type_params(source.chart_type_params().clone());
        *chart.price_scale_mut() = source.state().price_scale().clone();
        let (spacing, offset) = source.get_time_scale_state();
        chart.apply_synced_time_scale(spacing, offset);
        let copied_comparisons = super::comparison::copy(&mut chart, source, comparisons);
        let copied_spreads = spreads
            .iter()
            .filter_map(|spread| spread.copy(&mut chart, source))
            .collect();
        let mut study_ids = std::collections::HashMap::new();
        for (id, indicator) in source.indicators() {
            let copy = chart.add_indicator(indicator.clone_box());
            study_ids.insert(id.0, copy.0);
            if let Some(inputs) = source.indicator_inputs(id) {
                chart.set_indicator_inputs(copy, inputs);
            }
        }
        chart.drawings_mut().drawings = source.drawings().drawings.clone();
        chart.drawings_mut().drawings.retain_mut(|drawing| {
            if let Some(pane) = drawing.pane {
                let Some(&copied) = study_ids.get(&pane) else {
                    return false;
                };
                drawing.pane = Some(copied);
            }
            drawing.stored_id = None;
            drawing.created_at = None;
            true
        });
        chart.set_storage(self.storage.clone());
        DemoChart {
            chart,
            live,
            comparisons: copied_comparisons,
            spreads: copied_spreads,
        }
    }

    pub(super) fn activate_chart_slot(&mut self, slot: usize) {
        if slot == self.layout_slot {
            return;
        }
        self.stop_replay();
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
            std::mem::swap(&mut self.spreads, &mut target.spreads);
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
            self.chart.set_storage(self.storage.clone());
            self.refresh_active_chart();
        }
    }

    fn refresh_active_chart(&mut self) {
        self.refresh_strategy_signals();
        self.last_chart_click = None;
        self.crosshair = None;
        self.pointer_readout = None;
        self.time_and_sales.clear();
        self.watchlist.set_selected(self.chart.symbol());
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
        self.refresh_interval_control();
        self.top_toolbar.chart_type_selector.current = self.state.chart_type;
    }

    pub(super) fn refresh_interval_control(&mut self) {
        self.top_toolbar.timeframe_selector.current = self.state.timeframe;
    }

    pub(super) fn show_layout_tabs(&mut self, ui: &mut egui::Ui) {
        if self.layout_tabs.len() <= 1 {
            return;
        }
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
        if self.top_toolbar.layout_menu.autosave {
            self.save_layout();
        }
        self.stop_replay();
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
            spreads: std::mem::replace(&mut self.spreads, next.spreads),
            extra: std::mem::replace(&mut self.extra_charts, next.extra),
            layout: std::mem::replace(&mut self.layout, next.layout),
            slot: std::mem::replace(&mut self.layout_slot, next.slot),
        });
        self.layout_tab = index;
        self.refresh_layout_menu();
        // Other tabs may have edited the same symbol since this tab was last shown.
        self.chart.set_storage(self.storage.clone());
        for chart in &mut self.extra_charts {
            chart.chart.set_storage(self.storage.clone());
        }
        self.refresh_active_chart();
        self.top_toolbar
            .chart_grid_selector
            .set_layout(self.layout.mode);
        self.top_toolbar.chart_grid_selector.sync = self.layout.sync.clone();
    }

    fn new_layout(&mut self, name: Option<String>) {
        let copy = self.copy_chart(&self.chart, &self.live, &self.comparisons, &self.spreads);
        let duplicate = name.is_some();
        let layout = if duplicate {
            self.layout.clone()
        } else {
            ChartLayout::default()
        };
        let extra = if duplicate {
            self.extra_charts
                .iter()
                .map(|c| self.copy_chart(&c.chart, &c.live, &c.comparisons, &c.spreads))
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
                spreads: copy.spreads,
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

    /// The saved layouts, for the Open layout shortcut.
    pub(super) fn show_open_layout(&mut self, ctx: &egui::Context) {
        if !self.open_layout {
            return;
        }
        let mut open = true;
        let mut chosen = None;
        egui_charts::ui_kit::modal::Modal::new("open_layout", "Open layout")
            .width(320.0)
            .height(400.0)
            .show(ctx, &mut open, |ui| {
                if self.top_toolbar.layout_menu.layouts.is_empty() {
                    ui.label("No saved layouts");
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for name in &self.top_toolbar.layout_menu.layouts {
                        if ui
                            .add_sized(
                                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                                egui::Button::new(name).frame(false).truncate(),
                            )
                            .on_hover_text(name)
                            .clicked()
                        {
                            chosen = Some(name.clone());
                        }
                    }
                });
            });
        self.open_layout = open && chosen.is_none();
        if let Some(name) = chosen {
            self.handle_layout_action(LayoutAction::Open(name));
        }
    }

    pub(super) fn handle_layout_action(&mut self, action: LayoutAction) {
        match action {
            LayoutAction::SelectGrid(mode) => {
                self.grid_edits += 1;
                self.ensure_chart_slots(mode);
            }
            LayoutAction::CreateNew => self.new_layout(None),
            LayoutAction::SaveAs(name) => match self
                .capture_layout()
                .and_then(|layout| Ok((layout, self.capture_comparisons()?)))
            {
                Ok((layout, comparisons)) => {
                    self.new_layout(Some(name));
                    let name = self.layout_tabs[self.layout_tab].name.clone();
                    if let Err(error) = self.storage.save_comparisons(&name, &comparisons) {
                        self.toasts.error(error);
                        return;
                    }
                    self.restore_layout(name, layout);
                    self.save_layout();
                }
                Err(error) => self.toasts.error(error),
            },
            LayoutAction::Open(name) => {
                match self.layout_tabs.iter().position(|t| t.name == name) {
                    Some(i) if i != self.layout_tab => self.select_layout(i),
                    // The current layout opens again as it was saved.
                    _ => self.top_toolbar.layout_menu.open(&name),
                }
            }
            LayoutAction::Delete(name) => self.top_toolbar.layout_menu.delete(&name),
            LayoutAction::Rename(_, new) => {
                if !new.trim().is_empty() && !self.layout_tabs.iter().any(|t| t.name == new) {
                    match self.capture_layout() {
                        Ok(layout) => match self.save_comparisons(&new) {
                            Ok(()) => self.top_toolbar.layout_menu.rename(&new, &layout),
                            Err(error) => self.toasts.error(error),
                        },
                        Err(error) => self.toasts.error(error),
                    }
                }
            }
            LayoutAction::Save => self.save_layout(),
            LayoutAction::DownloadImage => {
                self.screenshot_requested = None;
                self.export_dialog.open();
            }
            LayoutAction::None => {}
            _ => {}
        }
    }

    pub(super) fn refresh_layout_menu(&mut self) {
        self.top_toolbar.layout_menu.curr_layout =
            Some(self.layout_tabs[self.layout_tab].name.clone());
    }

    /// The chart in `slot` of the open layout, counted from the first.
    fn chart_in_slot(&mut self, slot: usize) -> Option<&mut Chart> {
        if slot == self.layout_slot {
            return Some(&mut self.chart);
        }
        let index = if slot < self.layout_slot {
            slot
        } else {
            slot - 1
        };
        self.extra_charts.get_mut(index).map(|c| &mut c.chart)
    }

    /// The open layout: its arrangement and the charts it shows, in slot order.
    pub(super) fn capture_layout(&mut self) -> Result<SavedLayout, String> {
        let mode = self.layout.mode;
        let count = if mode == ChartLayoutMode::Tabs {
            self.extra_charts.len() + 1
        } else {
            mode.chart_count().min(self.extra_charts.len() + 1)
        };
        let charts = (0..count)
            .filter_map(|slot| self.chart_in_slot(slot).map(|chart| chart.snapshot()))
            .collect::<Result<_, _>>()
            .map_err(|error| format!("Could not save the layout: {error}"))?;
        Ok(SavedLayout { mode, charts })
    }

    fn capture_comparisons(&self) -> Result<Vec<Vec<super::comparison::SavedComparison>>, String> {
        let active = super::comparison::capture(&self.chart, &self.comparisons, &self.spreads)?;
        let mut slots = self
            .extra_charts
            .iter()
            .map(|slot| super::comparison::capture(&slot.chart, &slot.comparisons, &slot.spreads))
            .collect::<Result<Vec<_>, _>>()?;
        slots.insert(self.layout_slot, active);
        if self.layout.mode != ChartLayoutMode::Tabs {
            slots.truncate(self.layout.mode.chart_count());
        }
        Ok(slots)
    }

    fn save_comparisons(&self, name: &str) -> Result<(), String> {
        self.storage
            .save_comparisons(name, &self.capture_comparisons()?)
    }

    /// Saves the open layout under its tab's name.
    fn save_layout(&mut self) {
        let name = self.layout_tabs[self.layout_tab].name.clone();
        match self.capture_layout() {
            Ok(layout) => match self.save_comparisons(&name) {
                Ok(()) => self.top_toolbar.layout_menu.save(&name, &layout),
                Err(error) => self.toasts.error(error),
            },
            Err(error) => self.toasts.error(error),
        }
    }

    /// Shows a stored layout in the current tab, which takes its name.
    fn restore_layout(&mut self, name: String, layout: SavedLayout) {
        if let Some(index) = self.layout_tabs.iter().position(|t| t.name == name)
            && index != self.layout_tab
        {
            self.select_layout(index);
            return;
        }
        if layout.charts.is_empty()
            || (layout.mode != ChartLayoutMode::Tabs
                && layout.charts.len() != layout.mode.chart_count())
        {
            self.toasts
                .error("Could not open layout: invalid chart count");
            return;
        }
        let mut comparisons = match self.storage.comparisons(&name) {
            Ok(saved) if saved.is_empty() => (0..layout.charts.len()).map(|_| Vec::new()).collect(),
            Ok(saved) if saved.len() == layout.charts.len() => saved,
            Ok(_) => {
                self.toasts
                    .error("Could not open comparisons: invalid chart count");
                return;
            }
            Err(error) => {
                self.toasts.error(error);
                return;
            }
        };
        for (saved, snapshot) in comparisons.iter().zip(&layout.charts) {
            if let Err(error) = super::comparison::validate(saved, snapshot.panes.len()) {
                self.toasts.error(error);
                return;
            }
        }
        for snapshot in &layout.charts {
            let mut check = Chart::new(egui_charts::model::BarData::new());
            if let Err(error) = check.restore(snapshot) {
                self.toasts.error(format!("Could not open layout: {error}"));
                return;
            }
        }
        self.layout_tabs[self.layout_tab].name = name;
        if self.layout_slot >= layout.charts.len() {
            self.activate_chart_slot(0);
        }
        self.extra_charts.truncate(layout.charts.len() - 1);
        self.ensure_chart_slots(layout.mode);
        while self.extra_charts.len() + 1 < layout.charts.len() {
            let copy = self.copy_chart(&self.chart, &self.live, &self.comparisons, &self.spreads);
            self.extra_charts.push(copy);
        }
        super::comparison::clear(&mut self.chart, &mut self.comparisons, &mut self.spreads);
        for target in &mut self.extra_charts {
            super::comparison::clear(
                &mut target.chart,
                &mut target.comparisons,
                &mut target.spreads,
            );
        }
        for (slot, snapshot) in layout.charts.iter().enumerate() {
            if slot == self.layout_slot {
                // The demo's feed and toolbars follow the active chart's symbol and interval.
                self.switch_symbol(&snapshot.symbol);
                if let Some(interval) = snapshot.interval {
                    self.switch_timeframe(interval);
                }
            }
            let restored = self
                .chart_in_slot(slot)
                .map(|chart| chart.restore(snapshot));
            match restored {
                Some(Err(error)) => self
                    .toasts
                    .error(format!("Could not open chart {}: {error}", slot + 1)),
                Some(Ok(skipped)) if !skipped.is_empty() => self
                    .toasts
                    .warning(format!("Skipped {} unreadable drawings", skipped.len())),
                _ => {}
            }
        }
        for (slot, saved) in comparisons.drain(..).enumerate() {
            if slot == self.layout_slot {
                super::comparison::restore(
                    &mut self.chart,
                    &mut self.comparisons,
                    &mut self.spreads,
                    saved,
                );
            } else {
                let index = if slot < self.layout_slot {
                    slot
                } else {
                    slot - 1
                };
                let target = &mut self.extra_charts[index];
                target.live.set_symbol(target.chart.symbol());
                target.chart.set_data(
                    SeriesId::MAIN,
                    target.live.snapshot(
                        target.chart.timeframe().unwrap_or(Timeframe::Min1),
                        target.chart.bar_clock(),
                        target.chart.extended_hours(),
                    ),
                );
                super::comparison::restore(
                    &mut target.chart,
                    &mut target.comparisons,
                    &mut target.spreads,
                    saved,
                );
            }
        }
        self.refresh_active_chart();
        self.settings_state = egui_charts::widget::settings::ChartSettingsState {
            alerts: self.settings_state.alerts.clone(),
            trading: self.trading.display.clone(),
            adjust_for_dividends: self.timeframe_toolbar.state.adjust_for_dividends,
            ..egui_charts::widget::settings::ChartSettingsState::from_chart(&self.chart)
        };
    }

    /// The settings the demo keeps between sessions.
    pub(super) fn user_settings(&self) -> UserSettings {
        let menu = &self.top_toolbar.layout_menu;
        UserSettings {
            theme: self
                .theme_choice
                .map(|dark| if dark { "dark" } else { "light" }.into()),
            symbol: Some(self.state.symbol.clone()),
            interval: Some(self.state.timeframe),
            drawing_favorites: self.drawing_toolbar.state.favorites.clone(),
            layout: menu
                .curr_layout
                .clone()
                .filter(|name| menu.layouts.contains(name)),
            layout_autosave: menu.autosave,
        }
    }

    /// Applies stored settings, opening the layout that was open last.
    pub(super) fn apply_user_settings(&mut self, ctx: &egui::Context, settings: UserSettings) {
        self.theme_choice = match settings.theme.as_deref() {
            Some("dark") => Some(true),
            Some("light") => Some(false),
            _ => None,
        };
        self.update_theme(ctx);
        // Switching stops a replay the demo restored from its own storage.
        if let Some(symbol) = settings
            .symbol
            .filter(|symbol| *symbol != self.state.symbol)
        {
            self.switch_symbol(&symbol);
        }
        if let Some(interval) = settings
            .interval
            .filter(|interval| *interval != self.state.timeframe)
        {
            self.switch_timeframe(interval);
        }
        self.drawing_toolbar.state.favorites = settings.drawing_favorites;
        self.top_toolbar.layout_menu.autosave = settings.layout_autosave;
        if let Some(layout) = settings.layout {
            self.top_toolbar.layout_menu.open(&layout);
        }
    }

    /// Takes the answers of storage, restoring settings and layouts, reports failures and saves
    /// the open layout when autosave asks for it.
    pub(super) fn poll_storage(&mut self, ctx: &egui::Context) {
        let current = self.user_settings();
        if let Some(settings) = self.settings.poll(ctx, &current) {
            self.apply_user_settings(ctx, settings);
        }
        let previous = self.top_toolbar.layout_menu.curr_layout.clone();
        if let Some((name, layout)) = self.top_toolbar.layout_menu.poll(ctx) {
            self.restore_layout(name, layout);
        } else if self.top_toolbar.layout_menu.curr_layout != previous
            && let Some(name) = &self.top_toolbar.layout_menu.curr_layout
        {
            self.layout_tabs[self.layout_tab].name = name.clone();
        }
        let mut errors = vec![
            self.settings.take_error(),
            self.top_toolbar.layout_menu.take_error(),
            self.chart.take_storage_error(),
        ];
        errors.extend(
            self.extra_charts
                .iter_mut()
                .map(|c| c.chart.take_storage_error()),
        );
        for error in errors.into_iter().flatten() {
            self.toasts.error(error);
        }
        let edits = std::iter::once(&self.chart)
            .chain(self.extra_charts.iter().map(|c| &c.chart))
            .map(Chart::edit_count)
            .fold(self.grid_edits, u64::wrapping_add);
        if self.top_toolbar.layout_menu.autosave_due(ctx, edits) {
            self.save_layout();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_demo_keeps_hidden_rail_panels_reachable_without_a_single_layout_strip() {
        for width in [393.0, 599.0, 600.0, 1440.0] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
            app.live.set_paused(true);
            assert!(
                app.widget_bar.widgets.is_empty(),
                "sample panels start hidden"
            );
            app.execute_search_command(&ctx, "samples");
            assert_eq!(app.widget_bar.widgets.len(), 2);
            app.widget_bar.active_widget = Some(2);
            app.execute_search_command(&ctx, "samples");
            assert!(app.widget_bar.widgets.is_empty());
            assert_eq!(app.widget_bar.active_widget, None);
            app.execute_search_command(&ctx, "samples");

            let frame = |app: &mut DemoApp| {
                let mut remaining = egui::Rect::NOTHING;
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 852.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        let before = ui.available_rect_before_wrap();
                        app.show_layout_tabs(ui);
                        assert_eq!(
                            ui.available_rect_before_wrap().top(),
                            before.top(),
                            "one layout needs no strip"
                        );
                        app.show_widget_bar(ui, app.theme.ui.panel_bg);
                        remaining = ui.available_rect_before_wrap();
                        assert_eq!(
                            before.right() - remaining.right(),
                            if width < 600.0 { 0.0 } else { 49.0 }
                        );
                    },
                )
                .drop_without_applying_deltas();
                remaining
            };
            frame(&mut app);
            frame(&mut app);
            if width < 600.0 {
                let menu_frame = |app: &mut DemoApp, events| {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            events,
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 852.0),
                            )),
                            ..Default::default()
                        },
                        |ui| app.show_top_toolbar(ui, app.theme.ui.panel_bg),
                    );
                    let labels = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) => Some((
                                text.galley.text().to_owned(),
                                egui::Rect::from_min_size(text.pos, text.galley.size()),
                            )),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    output.drop_without_applying_deltas();
                    labels
                };
                let click = |app: &mut DemoApp, pos| {
                    menu_frame(app, vec![egui::Event::PointerMoved(pos)]);
                    menu_frame(
                        app,
                        vec![egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Default::default(),
                        }],
                    );
                    menu_frame(
                        app,
                        vec![egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: Default::default(),
                        }],
                    );
                    menu_frame(app, vec![])
                };
                menu_frame(&mut app, vec![]);
                for (label, panel) in [
                    ("Object Tree", Some(super::super::SidePanel::ObjectTree)),
                    ("Alerts", Some(super::super::SidePanel::Alerts)),
                    ("Data Window", Some(super::super::SidePanel::DataWindow)),
                    ("Watchlist", Some(super::super::SidePanel::Watchlist)),
                    (
                        "Time and sales",
                        Some(super::super::SidePanel::TimeAndSales),
                    ),
                    ("Session notes", None),
                    ("Keyboard shortcuts", None),
                ] {
                    let labels = click(&mut app, egui::pos2(22.0, 20.0));
                    let rect = labels
                        .iter()
                        .find(|(text, _)| text == label)
                        .unwrap_or_else(|| panic!("missing menu entry: {label}"))
                        .1;
                    click(&mut app, rect.center());
                    if let Some(panel) = panel {
                        assert!(app.open_panel == Some(panel));
                        assert_eq!(app.widget_bar.active_widget, None);
                    } else if label == "Session notes" {
                        assert!(app.open_panel.is_none());
                        assert_eq!(app.widget_bar.active_widget, Some(2));
                    } else {
                        assert!(app.keyboard_shortcuts.is_open);
                    }
                }
            }
            app.new_layout(None);
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let top = ui.available_rect_before_wrap().top();
                app.show_layout_tabs(ui);
                assert_eq!(ui.available_rect_before_wrap().top() - top, 26.0);
            })
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn keyboard_cycles_charts_and_layout_switches_keep_history_closed() {
        use super::super::TopToolbarAction;
        use eframe::App;
        use egui::{Event, Key, Modifiers};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.ensure_chart_slots(ChartLayoutMode::TwoHorizontal);
        app.extra_charts[0].live.set_paused(true);
        app.layout.sync.sync_time_axis = false;
        app.layout.set_active(0);
        app.open_panel = Some(super::super::SidePanel::TimeAndSales);
        let frame = |app: &mut DemoApp, events| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            );
            let has_old_trade = output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == "987.65")
            });
            output.drop_without_applying_deltas();
            assert!(
                !has_old_trade,
                "switching charts must clear the previous tape"
            );
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![]);
        let key = |key, modifiers| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        for (modifiers, expected) in [(Modifiers::NONE, 1), (Modifiers::SHIFT, 0)] {
            app.time_and_sales.push(egui_charts::model::Tick::new(
                chrono::Utc::now(),
                987.65,
                1.0,
                None,
            ));
            frame(
                &mut app,
                vec![Event::ModifiersChanged(modifiers), key(Key::Tab, modifiers)],
            );
            assert_eq!(app.layout_slot, expected);
            frame(&mut app, vec![]);
            let before = app.chart.get_time_scale_state();
            let other = app.extra_charts[0].chart.get_time_scale_state();
            frame(&mut app, vec![key(Key::ArrowRight, Modifiers::NONE)]);
            assert_ne!(app.chart.get_time_scale_state(), before);
            assert_eq!(app.extra_charts[0].chart.get_time_scale_state(), other);
        }
        app.chart
            .user_action(|chart| chart.config_mut().show_grid = false);
        app.handle_top_toolbar(
            &ctx,
            TopToolbarAction::Layout(LayoutAction::SaveAs("Copy".into())),
        );
        app.chart
            .user_action(|chart| chart.config_mut().show_grid = true);
        assert!(
            app.chart.undo(),
            "switching layouts must close the old chart's action"
        );
        app.handle_top_toolbar(
            &ctx,
            TopToolbarAction::Layout(LayoutAction::Open("Unnamed".into())),
        );
        assert!(app.chart.undo());
        assert!(app.chart.config().show_grid);
    }

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
        app.chart.drawings_mut().registry.set_availability(
            egui_charts::drawings::DrawingToolType::TrendLine,
            egui_charts::drawings::ToolAvailability {
                visible: false,
                enabled: false,
            },
        );
        app.chart.set_main_series_visible(false);
        app.chart
            .set_event_visibility(egui_charts::model::EventVisibility {
                show_earnings: true,
                show_economic_events: true,
                ..Default::default()
            });
        app.handle_comparison(super::super::CompareAction::AddSymbol(
            "AAPL".into(),
            Default::default(),
        ));
        app.add_spread("AAPL-MSFT");
        app.layout.sync.sync_drawings = true;
        app.ensure_chart_slots(ChartLayoutMode::Four);
        for copy in &mut app.extra_charts {
            let spread = copy
                .chart
                .series()
                .find(|(_, series)| series.name == "AAPL-MSFT")
                .expect("chart slots retain spread series")
                .0;
            assert!(!copy.chart.bars(spread).is_empty());
            assert!(
                !copy
                    .chart
                    .drawings()
                    .registry
                    .availability(egui_charts::drawings::DrawingToolType::TrendLine)
                    .enabled
            );
            assert_eq!(copy.chart.event_visibility(), app.chart.event_visibility());
            copy.chart.set_visible_bars(200);
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                copy.chart.show(ui);
            });
            let has_event = output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                    if matches!(text.galley.text(), "D" | "S" | "E" | "Ec" | "N"))
            });
            output.drop_without_applying_deltas();
            assert!(has_event, "copied charts retain their event source");
        }
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

        let removed =
            app.chart
                .add_indicator(Box::new(egui_charts::studies::CustomIndicator::new(
                    "Removed",
                    |_| Vec::new(),
                )));
        app.chart.remove_series(removed);
        let pane = app.chart.add_indicator(Box::new(
            egui_charts::studies::CustomIndicator::new("Copied pane", |_| Vec::new())
                .with_overlay(false),
        ));
        let mut drawing = app.chart.drawings().drawings[0].clone();
        drawing.id = 8;
        drawing.pane = Some(pane.0);
        app.chart.drawings_mut().drawings.push(drawing);
        let copied = app.copy_chart(&app.chart, &app.live, &app.comparisons, &app.spreads);
        let copied_pane = copied
            .chart
            .indicators()
            .find(|(_, study)| study.name() == "Copied pane")
            .unwrap()
            .0;
        assert_eq!(
            copied
                .chart
                .drawings()
                .drawings
                .iter()
                .find(|d| d.id == 8)
                .unwrap()
                .pane,
            Some(copied_pane.0),
            "layout copies remap pane ownership after series ids change"
        );
    }
}
