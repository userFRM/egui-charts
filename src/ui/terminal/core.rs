//! What every group relies on: the core actions, theme, settings and the shared helpers.

use super::{Action, ExportContents, ExportFile, Terminal};
use crate::Chart;
use crate::data::FeedStatus;
use crate::model::{ChartType, Timeframe};
use crate::services::{Notification, UserSettings};
use crate::theme::Theme;
use crate::ui::Control;
use crate::ui::app_state::ChartAppState;
use crate::ui::floating_replay_control::ReplayControlAction;
use crate::ui::top_toolbar::TopToolbarAction;
use crate::widget::SeriesId;

/// Whether settings are being read into the terminal or written from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    /// Apply what storage returned.
    Load,
    /// Write the current state into the settings.
    Save,
}

/// The side panels beside the chart.
#[expect(dead_code, reason = "the side panels open these")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SidePanel {
    ObjectTree,
    Alerts,
    DataWindow,
    Watchlist,
    Highlights,
    Screener,
    Hotlists,
    MarketHeatmap,
    Calendar,
    News,
    Notifications,
    Chats,
    Products,
}

/// The active chart, as the toolbars read it.
struct ActiveChart<'a> {
    chart: &'a Chart,
    timeframe: Timeframe,
    replay_active: bool,
}

impl ChartAppState for ActiveChart<'_> {
    fn active_symbol(&self) -> &str {
        self.chart.symbol()
    }
    fn active_timeframe(&self) -> &Timeframe {
        &self.timeframe
    }
    fn chart_type(&self) -> ChartType {
        self.chart.chart_type()
    }
    fn is_connected(&self) -> bool {
        // A feed without a live subscription, such as static history, has no status.
        self.chart
            .feed_state(SeriesId::MAIN)
            .is_none_or(|feed| matches!(feed.status, None | Some(FeedStatus::Live)))
    }
    fn is_replay_active(&self) -> bool {
        self.replay_active
    }
}

impl Terminal {
    pub(super) fn core_perform(&mut self, action: Action) -> Option<Action> {
        let Action::TopToolbar(top) = action else {
            return Some(action);
        };
        match top {
            TopToolbarAction::ChartStyleSelected(chart_type) => {
                self.user_action(|terminal| terminal.chart_mut().set_chart_type(chart_type));
            }
            // Undo and redo are not edits themselves.
            TopToolbarAction::Undo => {
                self.chart_mut().undo();
            }
            TopToolbarAction::Redo => {
                self.chart_mut().redo();
            }
            TopToolbarAction::ThemeToggle => {
                self.theme_choice = Some(!self.theme.dark);
                self.update_theme(&self.ctx());
            }
            TopToolbarAction::ToggleFullscreen => {
                let ctx = self.ctx();
                let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
            }
            top => return Some(Action::TopToolbar(top)),
        }
        None
    }

    /// The context of the frames. Actions are performed only once it is known.
    pub(super) fn ctx(&self) -> egui::Context {
        self.ctx
            .clone()
            .expect("actions are performed from the first frame on")
    }

    /// Queues `action` as if a control had emitted it this frame.
    pub(super) fn emit(&mut self, action: Action) {
        self.emitted.push(action);
    }

    /// Shows `control` in the terminal's own components unless the host hid it, or hides it.
    /// Groups apply the same rule to their components through [`Self::shown`].
    pub(super) fn set_control_visible(&mut self, control: Control, visible: bool) {
        let visible = visible && self.shown(control);
        self.top_toolbar.set_control_visible(control, visible);
        self.menus.set_control_visible(control, visible);
    }

    /// Whether the host left `control` visible.
    pub(super) fn shown(&self, control: Control) -> bool {
        !self.hidden.contains(&control)
    }

    /// Follows the user's theme choice, or the system appearance without one, on every slot.
    pub(super) fn update_theme(&mut self, ctx: &egui::Context) {
        let dark = self
            .theme_choice
            .or_else(|| ctx.system_theme().map(|theme| theme == egui::Theme::Dark))
            .unwrap_or(self.theme.dark);
        if self.theme.dark == dark {
            return;
        }
        self.theme = if dark { Theme::dark() } else { Theme::light() };
        for chart in &mut self.charts {
            chart.set_theme(ctx, &self.theme);
        }
    }

    /// Records the edits `edit` makes to the active chart as one undoable step.
    pub(super) fn user_action<R>(&mut self, edit: impl FnOnce(&mut Self) -> R) -> R {
        let slot = self.active;
        self.charts[slot].begin_user_action();
        let result = edit(self);
        self.charts[slot].end_user_action();
        result
    }

    /// Shows `symbol` on the active chart, stopping a replay, which the new symbol invalidates.
    pub(super) fn switch_symbol(&mut self, symbol: &str) {
        if self.replay_active {
            self.perform(Action::Replay(ReplayControlAction::Stop));
        }
        self.chart_mut().set_symbol(symbol);
    }

    /// Makes chart slot `index` the active one, which the toolbars and menus act on.
    pub(super) fn activate_slot(&mut self, index: usize) {
        if index < self.charts.len() {
            self.active = index;
            self.layout.set_active(index);
        }
    }

    /// Opens `panel` beside the chart.
    pub(super) fn open_panel(&mut self, panel: SidePanel) {
        self.panel = Some(panel);
    }

    /// Hands `contents` to the host's file export as `name`. False without one.
    #[expect(dead_code, reason = "the export dialog and the panels save files")]
    pub(super) fn save_file(&mut self, name: String, contents: ExportContents) -> bool {
        let Some(save) = &mut self.services.save else {
            return false;
        };
        save(ExportFile {
            name,
            path: None,
            contents,
        });
        true
    }

    /// Plays the host sound `name` at `volume`, from zero to one, if the host plays sounds.
    pub(super) fn play_sound(&mut self, name: &str, volume: f32) {
        if let Some(play) = &mut self.services.play {
            play(name, volume);
        }
    }

    /// Shows `message` briefly.
    #[expect(dead_code, reason = "the groups report their results")]
    pub(super) fn toast(&mut self, message: impl Into<String>) {
        self.toasts.info(message);
    }

    /// Shows `title` briefly and logs it in the notifications panel, about `symbol` if any.
    #[expect(dead_code, reason = "alerts and trading notify the user")]
    pub(super) fn notify(&mut self, title: impl Into<String>, symbol: Option<&str>) {
        let title = title.into();
        let mut entry = Notification::new(chrono::Utc::now(), title.clone());
        entry.symbol = symbol.map(str::to_owned);
        self.notices.push(entry);
        self.toasts.info(title);
    }

    /// Whether a dialog that takes keys is open, so the chart leaves the keys to it.
    pub(super) fn keyboard_dialog_open(&self) -> bool {
        self.drawings_modal()
            || self.studies_modal()
            || self.time_modal()
            || self.replay_modal()
            || self.alerts_modal()
            || self.panels_modal()
            || self.trading_modal()
            || self.layouts_modal()
            || self.compare_modal()
            || self.commands_modal()
    }

    /// Shows the top toolbar in line with the active chart and queues what it emitted.
    pub(super) fn show_top_toolbar(&mut self, ui: &mut egui::Ui) {
        let chart = &self.charts[self.active];
        let state = ActiveChart {
            chart,
            timeframe: chart.timeframe().unwrap_or_default(),
            replay_active: self.replay_active,
        };
        let toolbar = &mut self.top_toolbar;
        toolbar.sync_from_app_state(&state);
        toolbar.timeframe_selector.current = state.timeframe;
        toolbar.chart_type_selector.current = chart.chart_type();
        toolbar.state.can_undo = chart.can_undo();
        toolbar.state.can_redo = chart.can_redo();
        let action = toolbar.show_with_state(ui, &state);
        if action != TopToolbarAction::None {
            self.emit(Action::TopToolbar(action));
        }
    }

    /// The settings as they are now, for storage.
    pub(super) fn user_settings(&mut self) -> UserSettings {
        let chart = self.chart();
        let mut settings = UserSettings {
            theme: self
                .theme_choice
                .map(|dark| if dark { "dark" } else { "light" }.into()),
            symbol: Some(chart.symbol().to_owned()).filter(|symbol| !symbol.is_empty()),
            interval: chart.timeframe(),
            ..UserSettings::default()
        };
        self.group_settings(&mut settings, Direction::Save);
        settings
    }

    /// Applies settings read from storage.
    fn apply_user_settings(&mut self, mut settings: UserSettings) {
        self.theme_choice = match settings.theme.as_deref() {
            Some("dark") => Some(true),
            Some("light") => Some(false),
            _ => None,
        };
        self.update_theme(&self.ctx());
        if let Some(symbol) = settings
            .symbol
            .clone()
            .filter(|symbol| symbol != self.chart().symbol())
        {
            self.switch_symbol(&symbol);
        }
        if let Some(interval) = settings
            .interval
            .filter(|interval| Some(*interval) != self.chart().timeframe())
        {
            self.chart_mut().set_timeframe(interval);
        }
        self.group_settings(&mut settings, Direction::Load);
    }

    fn group_settings(&mut self, settings: &mut UserSettings, direction: Direction) {
        self.drawings_settings(settings, direction);
        self.studies_settings(settings, direction);
        self.time_settings(settings, direction);
        self.replay_settings(settings, direction);
        self.alerts_settings(settings, direction);
        self.panels_settings(settings, direction);
        self.trading_settings(settings, direction);
        self.layouts_settings(settings, direction);
        self.compare_settings(settings, direction);
        self.commands_settings(settings, direction);
    }

    /// Applies the settings once storage returns them, writes them once they change and reports
    /// a failed read or write.
    pub(super) fn poll_settings(&mut self, ctx: &egui::Context) {
        if self.settings.is_none() {
            return;
        }
        let current = self.user_settings();
        let Some(store) = &mut self.settings else {
            return;
        };
        let loaded = store.poll(ctx, &current);
        if let Some(error) = store.take_error() {
            self.toasts.error(error);
        }
        if let Some(settings) = loaded {
            self.apply_user_settings(settings);
        }
    }
}
