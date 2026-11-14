//! Layouts: chart slots, layouts and the layout tabs.
//!
//! A layout is the arrangement of the chart slots with their charts. The terminal shows one
//! layout at a time; the others wait in tabs and are swapped in when selected. Saving, opening,
//! renaming and deleting go through the layout menu, which keeps layouts in the host's
//! [`Storage`](egui_charts::services::Storage).

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use egui_charts::config::KeyboardAction;
use egui_charts::drawings::{DrawingTemplate, DrawingToolType};
use egui_charts::localization as l10n;
use egui_charts::services::UserSettings;
use egui_charts::ui::Control;
use egui_charts::ui::export::{ExportConfig, ExportFormat, ExportResponse};
use egui_charts::ui::floating_replay_control::ReplayControlAction;
use egui_charts::ui::top_toolbar::{LayoutAction, TopToolbarAction};
use egui_charts::ui_kit::modal::Modal;
use egui_charts::widget::{
    Chart, ChartBuilder, ChartLayout, ChartLayoutMode, SavedLayout, SeriesId, ShortcutRequest,
};
use std::collections::HashMap;

/// An open layout. The one on show lives in the terminal; the others keep their slots here.
struct Tab {
    name: String,
    workspace: Option<Workspace>,
    summary: String,
    change: f64,
}

/// The chart slots of a layout that is not on show.
struct Workspace {
    charts: Vec<Chart>,
    layout: ChartLayout,
    active: usize,
}

impl Tab {
    fn unnamed() -> Self {
        Self {
            name: "Unnamed".into(),
            workspace: None,
            summary: String::new(),
            change: 0.0,
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    tabs: Vec<Tab>,
    /// The tab on show.
    tab: usize,
    /// Whether the open-layout chooser is open.
    chooser: bool,
    /// Grid changes, counted with the chart edits so autosave sees them.
    grid_edits: u64,
    /// The slot that was active at the last poll, to notice a change of slot.
    seen_active: usize,
}

impl Terminal {
    pub(super) fn layouts_init(&mut self) {
        self.layouts.tabs.push(Tab::unnamed());
        if let Some(storage) = self.services.storage.clone() {
            self.top_toolbar.layout_menu.set_storage(storage);
            self.set_control_visible(Control::Layouts, true);
        }
    }

    pub(super) fn layouts_poll(&mut self, ctx: &egui::Context) {
        // A replay belongs to the chart it was started on.
        if self.layouts.seen_active != self.active {
            self.layouts.seen_active = self.active;
            self.layout_stop_replay();
        }
        let previous = self.top_toolbar.layout_menu.curr_layout.clone();
        if let Some((name, layout)) = self.top_toolbar.layout_menu.poll(ctx) {
            self.restore_layout(name, layout);
        } else if self.top_toolbar.layout_menu.curr_layout != previous
            && let Some(name) = &self.top_toolbar.layout_menu.curr_layout
        {
            self.layouts.tabs[self.layouts.tab].name = name.clone();
        }
        self.refresh_layout_menu();
        if let Some(error) = self.top_toolbar.layout_menu.take_error() {
            self.toasts.error(error);
        }
        let edits = self
            .charts
            .iter()
            .map(Chart::edit_count)
            .fold(self.layouts.grid_edits, u64::wrapping_add);
        if self.top_toolbar.layout_menu.autosave_due(ctx, edits) {
            self.save_layout();
        }
    }

    pub(super) fn layouts_show(&mut self, ui: &mut egui::Ui, region: Region) {
        match region {
            Region::Top => self.show_layout_tabs(ui),
            // The toolbar has been drawn: its grid selector holds this frame's sync options.
            Region::Left => self.layout.sync = self.top_toolbar.chart_grid_selector.sync.clone(),
            Region::Windows => self.show_open_layout(ui.ctx()),
            _ => {}
        }
    }

    pub(super) fn layouts_perform(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::TopToolbar(TopToolbarAction::Save)
            | Action::Shortcut(ShortcutRequest::Action(KeyboardAction::SaveLayout)) => {
                if self.services.storage.is_none() {
                    return Some(action);
                }
                self.save_layout();
            }
            Action::TopToolbar(TopToolbarAction::Layout(action)) | Action::Layout(action) => {
                return self.perform_layout_action(action);
            }
            Action::Shortcut(ShortcutRequest::Action(
                request @ (KeyboardAction::CycleChartFocus | KeyboardAction::CycleChartFocusBack),
            )) => {
                let count = self.layout.mode.chart_count().min(self.charts.len());
                let delta = if request == KeyboardAction::CycleChartFocus {
                    1
                } else {
                    count - 1
                };
                self.activate_slot((self.active + delta) % count);
            }
            action => return Some(action),
        }
        None
    }

    pub(super) fn layouts_settings(&mut self, settings: &mut UserSettings, direction: Direction) {
        let menu = &mut self.top_toolbar.layout_menu;
        match direction {
            Direction::Save => {
                settings.layout = menu
                    .curr_layout
                    .clone()
                    .filter(|name| menu.layouts.contains(name));
                settings.layout_autosave = menu.autosave;
                settings.layout_favorites = menu.favorites.clone();
            }
            Direction::Load => {
                menu.autosave = settings.layout_autosave;
                menu.favorites = settings.layout_favorites.clone();
                if let Some(layout) = &settings.layout {
                    menu.open(layout);
                }
            }
        }
    }

    pub(super) fn layouts_modal(&self) -> bool {
        self.layouts.chooser
    }

    fn perform_layout_action(&mut self, action: LayoutAction) -> Option<Action> {
        match action {
            LayoutAction::SelectGrid(mode) => {
                self.layouts.grid_edits = self.layouts.grid_edits.wrapping_add(1);
                self.set_grid(mode);
            }
            LayoutAction::DownloadData => {
                return self.perform(Action::Export(ExportResponse::Export {
                    config: ExportConfig {
                        format: ExportFormat::Csv,
                        ..ExportConfig::default()
                    },
                    save_path: None,
                }));
            }
            LayoutAction::DownloadImage => {
                return self.perform(Action::Export(ExportResponse::Export {
                    config: ExportConfig::default(),
                    save_path: None,
                }));
            }
            LayoutAction::OpenLayout => self.layouts.chooser = true,
            LayoutAction::CreateNew => {
                self.new_layout(None);
            }
            LayoutAction::SaveAs(name) => match self.capture_layout() {
                Ok(layout) => {
                    if self.new_layout(Some(name)) {
                        let name = self.layouts.tabs[self.layouts.tab].name.clone();
                        self.top_toolbar.layout_menu.save(&name, &layout);
                    }
                }
                Err(error) => self.toasts.error(error),
            },
            LayoutAction::Open(name) => {
                match self.layouts.tabs.iter().position(|tab| tab.name == name) {
                    Some(index) if index != self.layouts.tab => self.select_layout(index),
                    // The current layout opens again as it was saved.
                    _ => self.top_toolbar.layout_menu.open(&name),
                }
            }
            LayoutAction::Delete(name) => self.top_toolbar.layout_menu.delete(&name),
            LayoutAction::Rename(_, new)
                if !new.trim().is_empty()
                    && !self.layouts.tabs.iter().any(|tab| tab.name == new) =>
            {
                match self.capture_layout() {
                    Ok(layout) => self.top_toolbar.layout_menu.rename(&new, &layout),
                    Err(error) => self.toasts.error(error),
                }
            }
            LayoutAction::Save => self.save_layout(),
            _ => {}
        }
        None
    }

    /// Gives the layout the slots `mode` shows, copying the active chart into new ones. Slots
    /// beyond a smaller mode are kept, hidden, until a larger one shows them again.
    fn set_grid(&mut self, mode: ChartLayoutMode) {
        let count = if mode == ChartLayoutMode::Tabs {
            self.charts.len().max(2)
        } else {
            mode.chart_count()
        };
        while self.charts.len() < count {
            let Some(mut copy) = self.copy_slot(self.active) else {
                return;
            };
            // Drawings reach new slots through synchronization, which records their links.
            copy.drawings_mut().drawings.clear();
            self.charts.push(copy);
            self.wire_slot(self.charts.len() - 1);
        }
        self.layout.mode = mode;
        if self.active >= count {
            self.activate_slot(0);
        }
        self.top_toolbar.chart_grid_selector.set_layout(mode);
    }

    /// The drawing defaults every chart agrees on, with the active chart winning where
    /// charts disagree. Settings hold one set, so a pick made on one chart is the
    /// terminal's default everywhere.
    pub(super) fn merged_drawing_defaults(&self) -> HashMap<DrawingToolType, DrawingTemplate> {
        let mut merged: HashMap<DrawingToolType, DrawingTemplate> = HashMap::new();
        let take = |merged: &mut HashMap<DrawingToolType, DrawingTemplate>, chart: &Chart| {
            for (tool, template) in &chart.drawings().tool_defaults {
                merged.entry(*tool).or_insert_with(|| template.clone());
            }
        };
        for chart in &self.charts {
            take(&mut merged, chart);
        }
        for tab in &self.layouts.tabs {
            if let Some(workspace) = &tab.workspace {
                for chart in &workspace.charts {
                    take(&mut merged, chart);
                }
            }
        }
        // A pick just made on the active chart wins over a stale default parked elsewhere.
        if let Some(chart) = self.charts.get(self.active) {
            for (tool, template) in &chart.drawings().tool_defaults {
                merged.insert(*tool, template.clone());
            }
        }
        merged
    }

    /// Gives every chart, on show and parked in a layout tab, the same drawing defaults.
    pub(super) fn apply_drawing_defaults(
        &mut self,
        defaults: &HashMap<DrawingToolType, DrawingTemplate>,
    ) {
        for chart in &mut self.charts {
            chart.drawings_mut().tool_defaults = defaults.clone();
        }
        for tab in &mut self.layouts.tabs {
            if let Some(workspace) = &mut tab.workspace {
                for chart in &mut workspace.charts {
                    chart.drawings_mut().tool_defaults = defaults.clone();
                }
            }
        }
    }

    /// Carries a default written on one chart to every other chart.
    pub(super) fn sync_drawing_defaults(&mut self) {
        let merged = self.merged_drawing_defaults();
        self.apply_drawing_defaults(&merged);
    }

    /// A chart that shows what the chart in slot `from` shows: its snapshot, and the properties
    /// the snapshot leaves out. The caller wires it into its slot.
    fn copy_slot(&mut self, from: usize) -> Option<Chart> {
        let source = &self.charts[from];
        let mut chart = ChartBuilder::new()
            .with_feed(self.feed.clone())
            .with_symbol(source.symbol())
            .with_timeframe(source.timeframe().unwrap_or_default())
            .with_theme(self.theme.clone())
            .with_drawing_registry(source.drawings().registry.clone())
            .with_visible_candles(0)
            .build();
        chart.set_price_targets_visible(source.price_targets_visible());
        chart.set_event_visibility(source.event_visibility().clone());
        let copied = source
            .snapshot_with(self.studies.indicators.factory())
            .map_err(|error| error.to_string())
            .and_then(|snapshot| {
                chart
                    .restore_with(&snapshot, self.studies.indicators.factory())
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = copied {
            self.toasts
                .error(format!("Could not copy the chart: {error}"));
            return None;
        }
        chart.set_data(SeriesId::MAIN, source.bars(SeriesId::MAIN).to_vec());
        chart.set_conversion(source.conversion().map(str::to_owned));
        chart.set_study_limit(source.study_limit());
        // A snapshot carries no drawing defaults, so a new slot starts from the
        // terminal's current defaults, not the built-in ones.
        chart.drawings_mut().tool_defaults = self.merged_drawing_defaults();
        Some(chart)
    }

    fn layout_stop_replay(&mut self) {
        if self.replay_active {
            self.perform(Action::Replay(ReplayControlAction::Stop));
        }
    }

    fn show_layout_tabs(&mut self, ui: &mut egui::Ui) {
        let chart = self.chart();
        let bars = chart.bars(SeriesId::MAIN);
        let last = bars.last().map_or(0.0, |bar| bar.close);
        let previous = bars
            .get(bars.len().saturating_sub(2))
            .map_or(last, |bar| bar.close);
        let summary = format!("{}  {last:.2}", chart.symbol());
        let change = if previous == 0.0 {
            0.0
        } else {
            (last / previous - 1.0) * 100.0
        };
        let (up, down) = (chart.config().bullish_color, chart.config().bearish_color);
        let tab = &mut self.layouts.tabs[self.layouts.tab];
        tab.summary = summary;
        tab.change = change;
        if self.layouts.tabs.len() <= 1 {
            return;
        }
        let ctx = ui.ctx().clone();
        let (close_hint, new_hint) = (
            l10n::text(&ctx, "Close layout"),
            l10n::text(&ctx, "New layout"),
        );
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
                        for (index, tab) in self.layouts.tabs.iter().enumerate() {
                            egui::Frame::new()
                                .fill(if index == self.layouts.tab {
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
                                        let color = if tab.change >= 0.0 { up } else { down };
                                        ui.colored_label(color, format!("{:+.2}%", tab.change));
                                        if ui.small_button("×").on_hover_text(&close_hint).clicked()
                                        {
                                            close = Some(index);
                                        }
                                    });
                                });
                        }
                        new = ui.small_button("+").on_hover_text(&new_hint).clicked();
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

    /// Shows the layout in tab `index`, parking the one on show in its tab.
    fn select_layout(&mut self, index: usize) {
        if index == self.layouts.tab {
            return;
        }
        // Autosave keeps stored layouts current; it does not store the ones never saved.
        let menu = &self.top_toolbar.layout_menu;
        if menu.autosave
            && menu
                .layouts
                .contains(&self.layouts.tabs[self.layouts.tab].name)
        {
            self.save_layout();
        }
        self.layout_stop_replay();
        let Some(next) = self
            .layouts
            .tabs
            .get_mut(index)
            .and_then(|tab| tab.workspace.take())
        else {
            return;
        };
        self.layouts.tabs[self.layouts.tab].workspace = Some(Workspace {
            charts: std::mem::replace(&mut self.charts, next.charts),
            layout: std::mem::replace(&mut self.layout, next.layout),
            active: std::mem::replace(&mut self.active, next.active),
        });
        self.layouts.tab = index;
        self.layout.set_active(self.active);
        self.refresh_layout_menu();
        // The theme may have changed and other tabs may have edited the same symbol since this
        // layout was last on show.
        for slot in 0..self.charts.len() {
            self.wire_slot(slot);
        }
        // A default picked while another tab was on show reaches the tab just shown.
        self.sync_drawing_defaults();
        let grid = &mut self.top_toolbar.chart_grid_selector;
        grid.set_layout(self.layout.mode);
        grid.sync = self.layout.sync.clone();
    }

    /// Opens a tab with a copy of the active chart, or with a copy of the whole layout under
    /// `name`.
    fn new_layout(&mut self, name: Option<String>) -> bool {
        let duplicate = name.is_some();
        let (charts, layout, active) = if duplicate {
            let Some(charts) = (0..self.charts.len())
                .map(|slot| self.copy_slot(slot))
                .collect::<Option<Vec<_>>>()
            else {
                return false;
            };
            (charts, self.layout.clone(), self.active)
        } else {
            let Some(chart) = self.copy_slot(self.active) else {
                return false;
            };
            (vec![chart], ChartLayout::default(), 0)
        };
        let mut name = name.unwrap_or_else(|| "Unnamed".into());
        let base = name.clone();
        let mut suffix = 2;
        while self.layouts.tabs.iter().any(|tab| tab.name == name) {
            name = format!("{base} {suffix}");
            suffix += 1;
        }
        self.layouts.tabs.push(Tab {
            name,
            summary: self.chart().symbol().to_owned(),
            change: 0.0,
            workspace: Some(Workspace {
                charts,
                layout,
                active,
            }),
        });
        self.select_layout(self.layouts.tabs.len() - 1);
        true
    }

    fn close_layout(&mut self, index: usize) {
        if self.layouts.tabs.len() == 1 {
            return;
        }
        if index == self.layouts.tab {
            self.select_layout(if index == 0 { 1 } else { index - 1 });
        }
        self.layouts.tabs.remove(index);
        if self.layouts.tab > index {
            self.layouts.tab -= 1;
        }
    }

    /// The saved layouts, for the Open layout shortcut.
    fn show_open_layout(&mut self, ctx: &egui::Context) {
        if !self.layouts.chooser {
            return;
        }
        let mut open = true;
        let mut chosen = None;
        let none = l10n::text(ctx, "No saved layouts");
        Modal::new("open_layout", l10n::text(ctx, "Open layout"))
            .width(320.0)
            .height(400.0)
            .show(ctx, &mut open, |ui| {
                if self.top_toolbar.layout_menu.layouts.is_empty() {
                    ui.label(none);
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
        self.layouts.chooser = open && chosen.is_none();
        if let Some(name) = chosen {
            self.perform_layout_action(LayoutAction::Open(name));
        }
    }

    fn refresh_layout_menu(&mut self) {
        self.top_toolbar.layout_menu.curr_layout =
            Some(self.layouts.tabs[self.layouts.tab].name.clone());
    }

    /// The layout on show: its arrangement, the charts it shows, in slot order, and the chart
    /// sizes of the arrangement when its borders were moved.
    fn capture_layout(&self) -> Result<SavedLayout, String> {
        let mode = self.layout.mode;
        let count = if mode == ChartLayoutMode::Tabs {
            self.charts.len()
        } else {
            mode.chart_count().min(self.charts.len())
        };
        let charts = self.charts[..count]
            .iter()
            .map(|chart| chart.snapshot_with(self.studies.indicators.factory()))
            .collect::<Result<_, _>>()
            .map_err(|error| format!("Could not save the layout: {error}"))?;
        let sizing = self
            .layout
            .saved_sizes()
            .into_iter()
            .find(|sizes| sizes.mode == mode);
        Ok(SavedLayout {
            mode,
            charts,
            sizing,
        })
    }

    /// Saves the layout on show under its tab's name.
    fn save_layout(&mut self) {
        let name = self.layouts.tabs[self.layouts.tab].name.clone();
        match self.capture_layout() {
            Ok(layout) => self.top_toolbar.layout_menu.save(&name, &layout),
            Err(error) => self.toasts.error(error),
        }
    }

    /// Shows a stored layout in the current tab, which takes its name.
    fn restore_layout(&mut self, name: String, layout: SavedLayout) {
        if let Some(index) = self.layouts.tabs.iter().position(|tab| tab.name == name)
            && index != self.layouts.tab
        {
            self.select_layout(index);
            return;
        }
        if layout.charts.is_empty()
            || layout.charts.len() > ChartLayoutMode::Tabs.chart_count()
            || (layout.mode != ChartLayoutMode::Tabs
                && layout.charts.len() != layout.mode.chart_count())
        {
            self.toasts
                .error("Could not open layout: invalid chart count");
            return;
        }
        let source = self.chart();
        let defaults = self.merged_drawing_defaults();
        let mut charts = Vec::with_capacity(layout.charts.len());
        let mut skipped_drawings = 0;
        for snapshot in &layout.charts {
            let mut chart = ChartBuilder::new()
                .with_feed(self.feed.clone())
                .with_symbol(&snapshot.symbol)
                .with_timeframe(snapshot.interval.unwrap_or_default())
                .with_theme(self.theme.clone())
                .with_drawing_registry(source.drawings().registry.clone())
                .with_visible_candles(0)
                .build();
            match chart.restore_with(snapshot, self.studies.indicators.factory()) {
                Err(error) => {
                    self.toasts.error(format!("Could not open layout: {error}"));
                    return;
                }
                Ok(skipped) => skipped_drawings += skipped.len(),
            }
            chart.set_study_limit(source.study_limit());
            chart.set_price_targets_visible(source.price_targets_visible());
            chart.set_event_visibility(source.event_visibility().clone());
            chart.set_conversion(source.conversion().map(str::to_owned));
            // A saved layout carries no drawing defaults, so a reopened layout
            // starts from the terminal's current defaults.
            chart.drawings_mut().tool_defaults = defaults.clone();
            charts.push(chart);
        }
        if skipped_drawings > 0 {
            self.toasts
                .warning(format!("Skipped {skipped_drawings} unreadable drawings"));
        }
        self.layout_stop_replay();
        self.layouts.tabs[self.layouts.tab].name = name;
        self.charts = charts;
        // A copy drops a divider drag in progress, which would overwrite the restored sizes.
        self.layout = self.layout.clone();
        self.layout.reset_sizes(true);
        let sized = match &layout.sizing {
            Some(sizing) if sizing.mode != layout.mode => Err("sizes of another arrangement"),
            Some(sizing) => self.layout.set_sizes(std::slice::from_ref(sizing), true),
            None => Ok(()),
        };
        if let Err(error) = sized {
            self.toasts
                .error(format!("Could not open chart sizes: {error}"));
        }
        self.layout.mode = layout.mode;
        if self.active >= self.charts.len() {
            self.active = 0;
        }
        self.layout.set_active(self.active);
        self.top_toolbar.chart_grid_selector.set_layout(layout.mode);
        for slot in 0..self.charts.len() {
            self.wire_slot(slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MemoryStorage, SIZE, TestFeed, click, frame, key};
    use egui::{Key, Modifiers};
    use egui_charts::config::CrosshairStyle;
    use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType, ToolAvailability};
    use egui_charts::model::ChartType;
    use egui_charts::services::StoreKind;
    use egui_charts::studies::{CustomIndicator, IndicatorFactory, InputValue, RSI, SMA};
    use egui_charts::widget::{ChartLayoutSizing, PaneId};
    use std::sync::Arc;

    fn new_terminal(storage: &Arc<MemoryStorage>) -> Terminal {
        Terminal::new(Arc::new(TestFeed))
            .with_symbol("AAPL")
            .with_storage(storage.clone())
    }

    /// Runs frames until the answers of storage to the layout menu and the settings arrive.
    fn settle(terminal: &mut Terminal) {
        for _ in 0..4 {
            frame(terminal, SIZE, vec![]);
        }
    }

    fn layout(terminal: &mut Terminal, action: LayoutAction) {
        assert!(terminal.run(Action::Layout(action)).is_none());
        settle(terminal);
    }

    #[test]
    fn grid_selector_grows_slots_and_tab_cycles_the_visible_ones() {
        let storage = Arc::new(MemoryStorage::default());
        let mut terminal = new_terminal(&storage);
        settle(&mut terminal);
        // The slots a grid shows, and the slots that exist after it: shrinking keeps them.
        for (mode, shown, existing) in [
            (ChartLayoutMode::Three, 3, 3),
            (ChartLayoutMode::Single, 1, 3),
            (ChartLayoutMode::Sixteen, 16, 16),
            (ChartLayoutMode::Tabs, 16, 16),
        ] {
            layout(&mut terminal, LayoutAction::SelectGrid(mode));
            assert_eq!(terminal.layout.mode.chart_count(), shown);
            assert_eq!(terminal.charts.len(), existing);
            assert!(terminal.charts.iter().all(|chart| chart.symbol() == "AAPL"));
        }
        layout(
            &mut terminal,
            LayoutAction::SelectGrid(ChartLayoutMode::Three),
        );
        terminal.layout.sync.sync_time_axis = false;
        terminal.top_toolbar.chart_grid_selector.sync = terminal.layout.sync.clone();
        terminal.activate_slot(0);
        // The canvas takes the keyboard focus in the frame after it is activated.
        frame(&mut terminal, SIZE, vec![]);
        let tab = |terminal: &mut Terminal, modifiers| {
            let events = vec![
                egui::Event::ModifiersChanged(modifiers),
                key(Key::Tab, modifiers),
            ];
            frame(terminal, SIZE, events);
            frame(terminal, SIZE, vec![]);
            terminal.active
        };
        let forward: Vec<_> = (0..4)
            .map(|_| tab(&mut terminal, Modifiers::NONE))
            .collect();
        assert_eq!(forward, [1, 2, 0, 1]);
        let back: Vec<_> = (0..2)
            .map(|_| tab(&mut terminal, Modifiers::SHIFT))
            .collect();
        assert_eq!(back, [0, 2]);
        let before = terminal.chart().get_time_scale_state();
        let other = terminal.charts[0].get_time_scale_state();
        frame(
            &mut terminal,
            SIZE,
            vec![key(Key::ArrowRight, Modifiers::NONE)],
        );
        assert_ne!(terminal.chart().get_time_scale_state(), before);
        assert_eq!(terminal.charts[0].get_time_scale_state(), other);
        // Only the slots the grid shows take focus.
        layout(
            &mut terminal,
            LayoutAction::SelectGrid(ChartLayoutMode::Single),
        );
        assert_eq!(terminal.active, 0);
        assert_eq!(tab(&mut terminal, Modifiers::NONE), 0);
    }

    #[test]
    fn layout_tabs_keep_their_own_slots_history_and_theme() {
        let storage = Arc::new(MemoryStorage::default());
        let mut terminal = new_terminal(&storage);
        settle(&mut terminal);
        layout(
            &mut terminal,
            LayoutAction::SelectGrid(ChartLayoutMode::Four),
        );
        // One layout needs no strip, including on compact screens.
        let check_strip = |terminal: &mut Terminal, height| {
            let ctx = terminal.ctx();
            for width in [393.0, 599.0, 600.0, 1440.0] {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        let top = ui.available_rect_before_wrap().top();
                        terminal.show_layout_tabs(ui);
                        assert_eq!(ui.available_rect_before_wrap().top() - top, height);
                    },
                )
                .drop_without_applying_deltas();
            }
        };
        check_strip(&mut terminal, 0.0);
        for (slot, width) in [(1, 0.6), (3, 0.9)] {
            terminal.activate_slot(slot);
            terminal.chart_mut().config_mut().candle_width = width;
        }
        terminal.activate_slot(0);
        terminal
            .chart_mut()
            .user_action(|chart| chart.config_mut().show_grid = false);
        terminal.top_toolbar.layout_menu.autosave = true;
        layout(&mut terminal, LayoutAction::CreateNew);
        // The new layout is one slot with a copy of the active chart; the strip lists both.
        let names: Vec<_> = terminal.layouts.tabs.iter().map(|tab| &*tab.name).collect();
        assert_eq!(names, ["Unnamed", "Unnamed 2"]);
        assert!(
            storage.get(StoreKind::Layouts, "Unnamed").is_none(),
            "autosave does not store a layout that was never saved"
        );
        assert!(
            terminal
                .layouts
                .tabs
                .iter()
                .all(|tab| !tab.summary.is_empty()),
            "the first layout has a label when the strip appears"
        );
        assert_eq!(terminal.layouts.tab, 1);
        assert_eq!(terminal.layout.mode, ChartLayoutMode::Single);
        assert_eq!(terminal.charts.len(), 1);
        assert!(!terminal.chart().config().show_grid);
        check_strip(&mut terminal, 26.0);
        terminal
            .chart_mut()
            .user_action(|chart| chart.config_mut().show_grid = true);
        assert!(
            terminal.chart_mut().undo(),
            "the copied chart has no unfinished action"
        );
        assert!(!terminal.chart().config().show_grid);
        terminal.chart_mut().set_symbol("MSFT");
        // Back in the first layout the slots, their settings and their undo history are as left.
        layout(&mut terminal, LayoutAction::Open("Unnamed".into()));
        assert_eq!(terminal.layout.mode, ChartLayoutMode::Four);
        let widths: Vec<_> = terminal
            .charts
            .iter()
            .map(|chart| chart.config().candle_width)
            .collect();
        assert_eq!(widths[1], 0.6);
        assert_eq!(widths[3], 0.9);
        assert_eq!(terminal.chart().symbol(), "AAPL");
        assert!(terminal.chart_mut().undo(), "the edit stays undoable");
        assert!(terminal.chart().config().show_grid);
        // A layout that was parked follows the theme change made meanwhile.
        assert!(
            terminal
                .run(Action::TopToolbar(TopToolbarAction::ThemeToggle))
                .is_none()
        );
        // Checked before the next frame, which would restyle the chart anyway.
        let open = Action::Layout(LayoutAction::Open("Unnamed 2".into()));
        assert!(terminal.run(open).is_none());
        assert_eq!(terminal.chart().symbol(), "MSFT");
        assert_eq!(
            terminal.chart().config().background_color,
            terminal.theme.chart.bg
        );
        // The first close button belongs to the other layout: the one on show stays.
        let close = frame(&mut terminal, SIZE, vec![])
            .text_at("×")
            .expect("close button");
        click(&mut terminal, SIZE, close);
        assert_eq!(terminal.layouts.tabs.len(), 1);
        assert_eq!(terminal.layouts.tabs[0].name, "Unnamed 2");
        assert_eq!(terminal.layouts.tab, 0);
        assert_eq!(terminal.chart().symbol(), "MSFT");
    }

    #[test]
    fn chart_slots_and_layout_tabs_keep_independent_state() {
        let mut factory = IndicatorFactory::new();
        factory.register("Host pane", || {
            Box::new(CustomIndicator::new("Host pane", |_| Vec::new()).with_overlay(false))
        });
        let mut terminal = Terminal::new(Arc::new(TestFeed))
            .with_symbol("AAPL")
            .with_indicator_factory(factory);
        settle(&mut terminal);
        terminal.chart_mut().set_study_limit(Some(3));
        terminal.chart_mut().set_main_series_visible(false);
        terminal
            .chart_mut()
            .set_event_visibility(egui_charts::model::EventVisibility {
                show_earnings: true,
                show_economic_events: true,
                ..Default::default()
            });
        terminal
            .chart_mut()
            .drawings_mut()
            .registry
            .set_availability(
                DrawingToolType::TrendLine,
                ToolAvailability {
                    visible: false,
                    enabled: false,
                },
            );
        let mut drawing = Drawing::new(7, DrawingToolType::HorizontalLine);
        drawing.completed = true;
        drawing.chart_points.push(ChartPoint::new(10.0, 42.0));
        terminal
            .chart_mut()
            .drawings_mut()
            .add_synced_drawing(drawing)
            .unwrap();
        terminal.layout.sync.sync_drawings = true;
        terminal.top_toolbar.chart_grid_selector.sync = terminal.layout.sync.clone();
        layout(
            &mut terminal,
            LayoutAction::SelectGrid(ChartLayoutMode::Four),
        );
        for chart in &terminal.charts {
            assert_eq!(chart.study_limit(), Some(3));
            assert!(!chart.is_main_series_visible());
            assert!(chart.event_visibility().show_earnings);
            assert!(
                !chart
                    .drawings()
                    .registry
                    .availability(DrawingToolType::TrendLine)
                    .enabled
            );
            assert_eq!(chart.drawings().drawings.len(), 1);
        }
        for slot in [1, 3, 0, 2] {
            terminal.activate_slot(slot);
            terminal.chart_mut().config_mut().candle_width = 0.5 + slot as f32 * 0.1;
        }
        for slot in [3, 1, 2, 0] {
            terminal.activate_slot(slot);
            assert_eq!(
                terminal.chart().config().candle_width,
                0.5 + slot as f32 * 0.1
            );
        }
        let removed = terminal.chart_mut().add_indicator(Box::new(SMA::new(7)));
        terminal.chart_mut().remove_series(removed);
        let study = terminal
            .studies
            .indicators
            .factory()
            .create("Host pane")
            .unwrap();
        let id = terminal.chart_mut().add_indicator(study);
        let mut drawing = terminal.chart().drawings().drawings[0].clone();
        drawing.id = 8;
        drawing.pane = Some(id.0);
        terminal
            .chart_mut()
            .drawings_mut()
            .add_synced_drawing(drawing)
            .unwrap();
        terminal.chart_mut().set_conversion(Some("adjusted".into()));
        let before = terminal.chart().get_time_scale_state();
        assert!(terminal.new_layout(Some("Copy".into())));
        assert_eq!(terminal.chart().conversion(), Some("adjusted"));
        assert_eq!(terminal.layout.mode, ChartLayoutMode::Four);
        assert_eq!(terminal.chart().get_time_scale_state(), before);
        assert_eq!(terminal.chart().study_limit(), Some(3));
        let copied = terminal
            .chart()
            .indicators()
            .find(|(_, study)| study.name() == "Host pane")
            .unwrap()
            .0;
        assert_eq!(
            terminal
                .chart()
                .drawings()
                .drawings
                .iter()
                .find(|drawing| drawing.pane.is_some())
                .unwrap()
                .pane,
            Some(copied.0)
        );
        terminal.chart_mut().set_symbol("copy-only");
        terminal.select_layout(0);
        assert_eq!(terminal.chart().symbol(), "AAPL");
        terminal.select_layout(1);
        assert_eq!(terminal.chart().symbol(), "copy-only");
        terminal.close_layout(1);
        assert_eq!(terminal.chart().symbol(), "AAPL");
        terminal.set_grid(ChartLayoutMode::Single);
        terminal.set_grid(ChartLayoutMode::Four);
        terminal.activate_slot(3);
        assert_eq!(terminal.chart().config().candle_width, 0.8);
    }

    #[test]
    fn saved_layouts_open_again_and_after_a_restart() {
        let storage = Arc::new(MemoryStorage::default());
        let mut terminal = new_terminal(&storage);
        settle(&mut terminal);
        layout(
            &mut terminal,
            LayoutAction::SelectGrid(ChartLayoutMode::TwoHorizontal),
        );
        let select = |terminal: &mut Terminal, chart_type| {
            let action = Action::TopToolbar(TopToolbarAction::ChartStyleSelected(chart_type));
            assert!(terminal.run(action).is_none());
        };
        select(&mut terminal, ChartType::Line);
        terminal
            .chart_mut()
            .set_crosshair_style(CrosshairStyle::Dot);
        terminal.user_action(|terminal| {
            terminal.chart_mut().add_indicator(Box::new(SMA::new(7)));
        });
        let rsi = terminal.chart_mut().add_indicator(Box::new(RSI::new(14)));
        let pane = terminal.chart().series_pane(rsi).unwrap();
        terminal.chart_mut().move_pane(pane, 0);
        terminal.chart_mut().set_pane_stretch_factor(pane, 1.5);
        settle(&mut terminal);
        let saved = |terminal: &Terminal| {
            assert_eq!(terminal.charts.len(), 2);
            let chart = terminal.chart();
            assert_eq!(chart.chart_type(), ChartType::Line);
            assert_eq!(chart.chart_options().crosshair.style, CrosshairStyle::Dot);
            assert!(chart.indicators().any(|(_, study)| {
                study.inputs().first().map(|input| &input.value) == Some(&InputValue::Length(7))
            }));
            let rsi = chart
                .indicators()
                .find_map(|(id, study)| study.name().starts_with("RSI").then_some(id))
                .expect("RSI study");
            let pane = chart.series_pane(rsi).unwrap();
            assert_ne!(pane, PaneId::MAIN);
            assert_eq!(
                chart.panes()[0],
                pane,
                "the study pane stays above the price pane"
            );
            assert_eq!(chart.pane_stretch_factor(pane), Some(1.5));
        };
        frame(&mut terminal, SIZE, vec![key(Key::S, Modifiers::COMMAND)]);
        settle(&mut terminal);
        assert_eq!(terminal.top_toolbar.layout_menu.layouts, ["Unnamed"]);
        assert_eq!(terminal.layouts.tabs.len(), 1);
        // Opening the current layout again returns to it as saved.
        select(&mut terminal, ChartType::Area);
        layout(&mut terminal, LayoutAction::Open("Unnamed".into()));
        saved(&terminal);
        terminal.top_toolbar.layout_menu.favorites = vec!["Unnamed".into()];
        terminal.top_toolbar.layout_menu.autosave = true;
        terminal
            .chart_mut()
            .user_action(|chart| chart.config_mut().show_grid = false);
        for _ in 0..25 {
            frame(&mut terminal, SIZE, vec![]);
        }
        let document = storage.get(StoreKind::Layouts, "Unnamed").unwrap();
        let autosaved = SavedLayout::from_json(&document).unwrap();
        assert!(!autosaved.charts[0].config.as_ref().unwrap().show_grid);
        // A new terminal on the same storage opens the layout that was open last.
        drop(terminal);
        let mut terminal = new_terminal(&storage);
        settle(&mut terminal);
        saved(&terminal);
        assert!(terminal.top_toolbar.layout_menu.autosave);
        assert_eq!(terminal.top_toolbar.layout_menu.favorites, ["Unnamed"]);
        layout(
            &mut terminal,
            LayoutAction::Rename("Unnamed".into(), "Renamed".into()),
        );
        assert_eq!(terminal.top_toolbar.layout_menu.layouts, ["Renamed"]);
        assert_eq!(terminal.layouts.tabs[0].name, "Renamed");
        assert_eq!(terminal.top_toolbar.layout_menu.favorites, ["Renamed"]);
        saved(&terminal);
        layout(&mut terminal, LayoutAction::SaveAs("Copy".into()));
        assert_eq!(
            terminal.top_toolbar.layout_menu.layouts,
            ["Copy", "Renamed"]
        );
        assert_eq!(terminal.layouts.tabs.len(), 2);
        saved(&terminal);
        // Opening restored the chart as one undoable edit after it.
        select(&mut terminal, ChartType::Area);
        assert!(terminal.chart().can_undo());
        terminal.run(Action::TopToolbar(TopToolbarAction::Undo));
        assert_eq!(terminal.chart().chart_type(), ChartType::Line);
        layout(&mut terminal, LayoutAction::Delete("Renamed".into()));
        assert_eq!(terminal.top_toolbar.layout_menu.layouts, ["Copy"]);
        assert!(terminal.top_toolbar.layout_menu.favorites.is_empty());

        // The chooser stays within small screens with a long catalogue and names.
        terminal.top_toolbar.layout_menu.layouts = (0..40)
            .map(|i| format!("Saved layout {i}: {}", "a long name ".repeat(10)))
            .collect();
        for theme in [
            egui_charts::theme::Theme::light(),
            egui_charts::theme::Theme::dark(),
        ] {
            let ctx = terminal.ctx();
            egui_charts::theme::apply_to_egui(&ctx, &theme);
            for size in [egui::vec2(320.0, 360.0), SIZE] {
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                terminal.layouts.chooser = true;
                for _ in 0..3 {
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ui| terminal.show_open_layout(ui.ctx()),
                    )
                    .drop_without_applying_deltas();
                }
                let bounds = ctx
                    .memory(|memory| memory.area_rect(egui::Id::unique("open_layout")))
                    .unwrap();
                assert!(
                    screen.contains_rect(bounds),
                    "Open layout exceeds {size:?}: {bounds:?}"
                );
            }
        }
    }

    /// A saved layout opens at the chart sizes it was saved with, whatever borders were moved
    /// in the tab since, in the same session and after a restart.
    #[test]
    fn opening_a_saved_layout_again_restores_its_chart_sizes() {
        let mode = ChartLayoutMode::TwoHorizontal;
        let resize = |terminal: &mut Terminal, columns: [f32; 2]| {
            let sizes = ChartLayoutSizing {
                mode,
                weights: vec![vec![1.0], columns.to_vec()],
            };
            terminal.layout.set_sizes(&[sizes], false).unwrap();
            settle(terminal);
        };
        // The column weights at the save, `None` for the preset's two equal charts, and the
        // fractions of the width the layout opens at.
        for (saved, opened) in [(None, [0.5, 0.5]), (Some([1.0, 3.0]), [0.25, 0.75])] {
            let storage = Arc::new(MemoryStorage::default());
            let mut terminal = new_terminal(&storage);
            settle(&mut terminal);
            layout(&mut terminal, LayoutAction::SelectGrid(mode));
            if let Some(columns) = saved {
                resize(&mut terminal, columns);
            }
            layout(&mut terminal, LayoutAction::Save);
            let document: serde_json::Value =
                serde_json::from_str(&storage.get(StoreKind::Layouts, "Unnamed").unwrap()).unwrap();
            assert_eq!(
                document.get("sizing").is_some(),
                saved.is_some(),
                "preset proportions are stored as they were before layouts kept sizes"
            );
            let opened = [vec![1.0], opened.to_vec()];
            resize(&mut terminal, [3.0, 1.0]);
            assert_eq!(terminal.layout.sizes(mode).weights[1], [0.75, 0.25]);
            layout(&mut terminal, LayoutAction::Open("Unnamed".into()));
            assert_eq!(terminal.layout.sizes(mode).weights, opened, "{saved:?}");
            drop(terminal);
            let mut terminal = new_terminal(&storage);
            settle(&mut terminal);
            resize(&mut terminal, [3.0, 1.0]);
            layout(&mut terminal, LayoutAction::Open("Unnamed".into()));
            assert_eq!(terminal.layout.mode, mode);
            assert_eq!(terminal.charts.len(), 2);
            assert_eq!(
                terminal.layout.sizes(mode).weights,
                opened,
                "{saved:?}, after a restart"
            );
        }
    }
}
