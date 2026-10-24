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
        chart.set_data(SeriesId::MAIN, source.state().data().bars.clone());
        chart.set_event_visibility(source.event_visibility().clone());
        chart.set_mark_source(Some(Arc::new(super::marks::DemoMarks(
            chart.bars(SeriesId::MAIN).to_vec(),
        ))));
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
        self.last_chart_click = None;
        self.crosshair = None;
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
            LayoutAction::SelectGrid(mode) => {
                self.grid_edits += 1;
                self.ensure_chart_slots(mode);
            }
            LayoutAction::CreateNew => self.new_layout(None),
            LayoutAction::SaveAs(name) => {
                self.new_layout(Some(name));
                self.save_layout();
            }
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
                        Ok(layout) => {
                            self.layout_tabs[self.layout_tab].name = new.clone();
                            self.top_toolbar.layout_menu.rename(&new, &layout);
                        }
                        Err(error) => self.toasts.error(error),
                    }
                }
            }
            LayoutAction::Save => self.save_layout(),
            LayoutAction::DownloadImage => {
                self.export_generation += 1;
                self.export_dialog.open();
            }
            LayoutAction::None => {}
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

    /// Saves the open layout under its tab's name.
    fn save_layout(&mut self) {
        let name = self.layout_tabs[self.layout_tab].name.clone();
        match self.capture_layout() {
            Ok(layout) => self.top_toolbar.layout_menu.save(&name, &layout),
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
        self.layout_tabs[self.layout_tab].name = name;
        self.ensure_chart_slots(layout.mode);
        while self.extra_charts.len() + 1 < layout.charts.len() {
            let copy = self.copy_chart(&self.chart, &self.live, &self.comparisons);
            self.extra_charts.push(copy);
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
            if let Some(Err(error)) = restored {
                self.toasts
                    .error(format!("Could not open chart {}: {error}", slot + 1));
            }
        }
        self.refresh_active_chart();
        self.settings_state =
            egui_charts::ui::top_toolbar::components::settings_menu::data::ChartSettingsState::from_chart(&self.chart);
    }

    /// The settings the demo keeps between sessions.
    pub(super) fn user_settings(&self) -> UserSettings {
        let menu = &self.top_toolbar.layout_menu;
        UserSettings {
            theme: Some(if self.theme.dark { "dark" } else { "light" }.into()),
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
        if settings
            .theme
            .is_some_and(|theme| (theme == "dark") != self.theme.dark)
        {
            self.toggle_theme(ctx);
        }
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
        if let Some((name, layout)) = self.top_toolbar.layout_menu.poll(ctx) {
            self.restore_layout(name, layout);
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
        app.chart.set_main_series_visible(false);
        app.chart
            .set_event_visibility(egui_charts::model::EventVisibility {
                show_earnings: true,
                show_economic_events: true,
                ..Default::default()
            });
        app.handle_comparison(super::super::CompareAction::AddSymbol("AAPL".into()));
        app.layout.sync.sync_drawings = true;
        app.ensure_chart_slots(ChartLayoutMode::Four);
        for copy in &mut app.extra_charts {
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
        let copied = app.copy_chart(&app.chart, &app.live, &app.comparisons);
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
