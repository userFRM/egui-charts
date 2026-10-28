//! Layouts: chart slots, layouts and the layout tabs.
//!
//! A layout is the arrangement of the chart slots with their charts. The terminal shows one
//! layout at a time; the others wait in tabs and are swapped in when selected. Saving, opening,
//! renaming and deleting go through the layout menu, which keeps layouts in the host's
//! [`Storage`](crate::services::Storage).

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use crate::config::KeyboardAction;
use crate::localization as l10n;
use crate::model::Timeframe;
use crate::services::UserSettings;
use crate::ui::Control;
use crate::ui::export::{ExportConfig, ExportFormat, ExportResponse};
use crate::ui::floating_replay_control::ReplayControlAction;
use crate::ui::top_toolbar::{LayoutAction, TopToolbarAction};
use crate::ui_kit::modal::Modal;
use crate::widget::{
    Chart, ChartBuilder, ChartLayout, ChartLayoutMode, SavedLayout, SeriesId, ShortcutRequest,
};

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
        let mut errors = vec![self.top_toolbar.layout_menu.take_error()];
        errors.extend(self.charts.iter_mut().map(Chart::take_storage_error));
        for error in errors.into_iter().flatten() {
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
            Action::TopToolbar(TopToolbarAction::Save) => self.save_layout(),
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
            }
            Direction::Load => {
                menu.autosave = settings.layout_autosave;
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
                self.layouts.grid_edits += 1;
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
            LayoutAction::CreateNew => self.new_layout(None),
            LayoutAction::SaveAs(name) => match self.capture_layout() {
                Ok(layout) => {
                    self.new_layout(Some(name));
                    let name = self.layouts.tabs[self.layouts.tab].name.clone();
                    self.restore_layout(name, layout);
                    self.save_layout();
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
            let mut copy = self.copy_slot(self.active);
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

    /// A chart that shows what the chart in slot `from` shows: its snapshot, and the properties
    /// the snapshot leaves out. The caller wires it into its slot.
    fn copy_slot(&mut self, from: usize) -> Chart {
        let source = &self.charts[from];
        let mut chart = ChartBuilder::new()
            .with_feed(self.feed.clone())
            .with_symbol(source.symbol())
            .with_timeframe(source.timeframe().unwrap_or(Timeframe::Day1))
            .with_drawing_registry(source.drawings().registry.clone())
            .with_visible_candles(0)
            .build();
        chart.set_price_targets_visible(source.price_targets_visible());
        chart.set_event_visibility(source.event_visibility().clone());
        let copied = source
            .snapshot()
            .map_err(|error| error.to_string())
            .and_then(|snapshot| chart.restore(&snapshot));
        if let Err(error) = copied {
            self.toasts
                .error(format!("Could not copy the chart: {error}"));
        }
        chart
    }

    fn layout_stop_replay(&mut self) {
        if self.replay_active {
            self.perform(Action::Replay(ReplayControlAction::Stop));
        }
    }

    fn show_layout_tabs(&mut self, ui: &mut egui::Ui) {
        if self.layouts.tabs.len() <= 1 {
            return;
        }
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
        if self.top_toolbar.layout_menu.autosave {
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
        let grid = &mut self.top_toolbar.chart_grid_selector;
        grid.set_layout(self.layout.mode);
        grid.sync = self.layout.sync.clone();
    }

    /// Opens a tab with a copy of the active chart, or with a copy of the whole layout under
    /// `name`.
    fn new_layout(&mut self, name: Option<String>) {
        let duplicate = name.is_some();
        let (charts, layout, active) = if duplicate {
            let charts = (0..self.charts.len())
                .map(|slot| self.copy_slot(slot))
                .collect();
            (charts, self.layout.clone(), self.active)
        } else {
            (vec![self.copy_slot(self.active)], ChartLayout::default(), 0)
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

    /// The layout on show: its arrangement and the charts it shows, in slot order.
    fn capture_layout(&self) -> Result<SavedLayout, String> {
        let mode = self.layout.mode;
        let count = if mode == ChartLayoutMode::Tabs {
            self.charts.len()
        } else {
            mode.chart_count().min(self.charts.len())
        };
        let charts = self.charts[..count]
            .iter()
            .map(Chart::snapshot)
            .collect::<Result<_, _>>()
            .map_err(|error| format!("Could not save the layout: {error}"))?;
        Ok(SavedLayout { mode, charts })
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
            || (layout.mode != ChartLayoutMode::Tabs
                && layout.charts.len() != layout.mode.chart_count())
        {
            self.toasts
                .error("Could not open layout: invalid chart count");
            return;
        }
        for snapshot in &layout.charts {
            let mut check = Chart::new(crate::model::BarData::new());
            if let Err(error) = check.restore(snapshot) {
                self.toasts.error(format!("Could not open layout: {error}"));
                return;
            }
        }
        self.layout_stop_replay();
        self.layouts.tabs[self.layouts.tab].name = name;
        if self.active >= layout.charts.len() {
            self.activate_slot(0);
        }
        self.charts.truncate(layout.charts.len());
        self.set_grid(layout.mode);
        while self.charts.len() < layout.charts.len() {
            let copy = self.copy_slot(self.active);
            self.charts.push(copy);
            self.wire_slot(self.charts.len() - 1);
        }
        for (slot, snapshot) in layout.charts.iter().enumerate() {
            match self.charts[slot].restore(snapshot) {
                Err(error) => self
                    .toasts
                    .error(format!("Could not open chart {}: {error}", slot + 1)),
                Ok(skipped) if !skipped.is_empty() => self
                    .toasts
                    .warning(format!("Skipped {} unreadable drawings", skipped.len())),
                Ok(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{MemoryStorage, SIZE, TestFeed, click, frame, key};
    use super::*;
    use crate::config::CrosshairStyle;
    use crate::model::ChartType;
    use crate::studies::{InputValue, RSI, SMA};
    use crate::widget::PaneId;
    use egui::{Key, Modifiers};
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
        // One layout needs no strip.
        assert!(frame(&mut terminal, SIZE, vec![]).text_at("+").is_none());
        for (slot, width) in [(1, 0.6), (3, 0.9)] {
            terminal.activate_slot(slot);
            terminal.chart_mut().config_mut().candle_width = width;
        }
        terminal.activate_slot(0);
        terminal
            .chart_mut()
            .user_action(|chart| chart.config_mut().show_grid = false);
        layout(&mut terminal, LayoutAction::CreateNew);
        // The new layout is one slot with a copy of the active chart; the strip lists both.
        let names: Vec<_> = terminal.layouts.tabs.iter().map(|tab| &*tab.name).collect();
        assert_eq!(names, ["Unnamed", "Unnamed 2"]);
        assert_eq!(terminal.layouts.tab, 1);
        assert_eq!(terminal.layout.mode, ChartLayoutMode::Single);
        assert_eq!(terminal.charts.len(), 1);
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
        assert!(
            terminal
                .run(Action::TopToolbar(TopToolbarAction::Save))
                .is_none()
        );
        settle(&mut terminal);
        assert_eq!(terminal.top_toolbar.layout_menu.layouts, ["Unnamed"]);
        assert_eq!(terminal.layouts.tabs.len(), 1);
        // Opening the current layout again returns to it as saved.
        select(&mut terminal, ChartType::Area);
        layout(&mut terminal, LayoutAction::Open("Unnamed".into()));
        saved(&terminal);
        // A new terminal on the same storage opens the layout that was open last.
        drop(terminal);
        let mut terminal = new_terminal(&storage);
        settle(&mut terminal);
        saved(&terminal);
        layout(
            &mut terminal,
            LayoutAction::Rename("Unnamed".into(), "Renamed".into()),
        );
        assert_eq!(terminal.top_toolbar.layout_menu.layouts, ["Renamed"]);
        assert_eq!(terminal.layouts.tabs[0].name, "Renamed");
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
    }
}
