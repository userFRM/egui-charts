//! The assembled terminal: toolbars, side panels, bottom dock, dialogs, menus and one or more
//! charts on the host's feed and services.
//!
//! [`Terminal`] performs every action its controls emit unless the host intercepts it, and
//! returns what it does not perform. The host supplies data and services through the traits of
//! [`crate::data`] and [`crate::services`], and the outputs only it can perform (files, sounds)
//! as callbacks. One terminal per viewport: its dialogs use fixed egui ids.
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use egui_charts::data::{Datafeed, HistoryPage, HistoryRequest, Reply};
//! use egui_charts::ui::terminal::Terminal;
//! # struct MyFeed;
//! # impl Datafeed for MyFeed {
//! #     fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
//! #         reply.send(Ok(HistoryPage::default()));
//! #     }
//! # }
//!
//! let mut terminal = Terminal::new(Arc::new(MyFeed)).with_symbol("AAPL");
//! # egui::__run_test_ui(|ui| {
//! for action in terminal.show(ui) {
//!     // Intercepted actions, host entries and actions the terminal has no default for.
//!     let _ = action;
//! }
//! # });
//! ```

mod action;
mod alerts;
mod commands;
mod compare;
mod core;
mod drawings;
mod layout;
mod layouts;
mod panels;
mod replay;
mod services;
mod studies;
#[cfg(test)]
mod test_support;
mod time;
mod trading;

pub use action::Action;
pub use services::{ExportContents, ExportFile};

use crate::data::Datafeed;
use crate::model::{StrategyReport, Timeframe};
use crate::services::{
    Alerts, Broker, Chats, Marks, Notification, Notifications, Quotes, SettingsStore, Signals,
    Storage, SymbolSearch,
};
use crate::theme::Theme;
use crate::ui::Control;
use crate::ui::context_menu::ChartMenus;
use crate::ui::export::ExportFormat;
use crate::ui::top_toolbar::{TopToolbar, TopToolbarConfig};
use crate::ui::trading::BrokerTile;
use crate::ui_kit::notifications::Toasts;
use crate::widget::{ChartEvent, ChartLayout};
use crate::{Chart, ChartBuilder};
use std::collections::HashSet;
use std::sync::Arc;

type Intercept = Box<dyn FnMut(&Action) -> bool>;

/// The assembled terminal: toolbars, side panels, bottom dock, dialogs, menus and one or more
/// charts on the host's feed and services. It performs every action its controls emit unless the
/// host intercepts it.
///
/// Every chart slot loads from the same [`Datafeed`]. Each service is handed to the components
/// that consume it; a control whose service or output is absent is hidden. The controls of
/// features the terminal does not perform yet stay hidden and the services passed for them are
/// kept unused.
pub struct Terminal {
    charts: Vec<Chart>,
    layout: ChartLayout,
    active: usize,
    feed: Arc<dyn Datafeed>,
    services: services::Services,
    intercept: Option<Intercept>,
    /// Actions run before the first frame, which wait for the `egui::Context`.
    pending: Vec<Action>,
    /// Actions the controls emitted this frame, performed once they are drawn.
    emitted: Vec<Action>,
    /// Actions for the host: intercepted ones and those no part of the terminal performs.
    returned: Vec<Action>,
    ctx: Option<egui::Context>,
    theme: Theme,
    /// The user's explicit theme, dark or light; `None` follows the system appearance.
    theme_choice: Option<bool>,
    toasts: Toasts,
    top_toolbar: TopToolbar,
    menus: ChartMenus,
    /// Controls the host hid with [`Terminal::hide`].
    hidden: HashSet<Control>,
    settings: Option<SettingsStore>,
    /// Entries for the notifications panel, taken by the panel when it is shown.
    #[allow(dead_code, reason = "the notifications panel takes the entries")]
    notices: Vec<Notification>,
    /// What each chart slot reported this frame, by slot.
    chart_events: Vec<(usize, ChartEvent)>,
    /// The side panel on show.
    #[allow(dead_code, reason = "the side panels read the open panel")]
    panel: Option<core::SidePanel>,
    /// Whether a replay is running, so a symbol change stops it.
    replay_active: bool,
    strategy_report: Option<StrategyReport>,
    drawings: drawings::State,
    studies: studies::State,
    time: time::State,
    #[expect(dead_code, reason = "the group keeps its state here")]
    replay: replay::State,
    #[expect(dead_code, reason = "the group keeps its state here")]
    alerts: alerts::State,
    #[expect(dead_code, reason = "the group keeps its state here")]
    panels: panels::State,
    #[expect(dead_code, reason = "the group keeps its state here")]
    trading: trading::State,
    #[expect(dead_code, reason = "the group keeps its state here")]
    layouts: layouts::State,
    #[expect(dead_code, reason = "the group keeps its state here")]
    compare: compare::State,
    #[expect(dead_code, reason = "the group keeps its state here")]
    commands: commands::State,
}

impl Terminal {
    /// Controls whose feature the terminal does not perform yet. They stay hidden.
    const DEFERRED: [Control; 21] = [
        Control::Alerts,
        Control::Trading,
        Control::Replay,
        Control::Layouts,
        Control::Templates,
        Control::Compare,
        Control::TimeZone,
        Control::Session,
        Control::DividendAdjustment,
        Control::ObjectTree,
        Control::QuickSearch,
        Control::Snapshot,
        Control::ExportData,
        Control::Indicators,
        Control::Settings,
        Control::SymbolSearch,
        Control::Interval,
        Control::Help,
        Control::CopyPrice,
        Control::ResetView,
        Control::Publish,
    ];

    /// A terminal whose charts load from `feed`, on the daily interval.
    pub fn new(feed: Arc<dyn Datafeed>) -> Self {
        let mut chart = ChartBuilder::new()
            .with_feed(feed.clone())
            .with_timeframe(Timeframe::Day1)
            .build();
        chart.config_mut().skip_background = true;
        let mut terminal = Self {
            charts: vec![chart],
            layout: ChartLayout::default(),
            active: 0,
            feed,
            services: services::Services::default(),
            intercept: None,
            pending: Vec::new(),
            emitted: Vec::new(),
            returned: Vec::new(),
            ctx: None,
            theme: Theme::light(),
            theme_choice: None,
            toasts: Toasts::new(),
            top_toolbar: TopToolbar::new()
                .with_config(TopToolbarConfig::default().with_height(38.0)),
            menus: ChartMenus::default(),
            hidden: HashSet::new(),
            settings: None,
            notices: Vec::new(),
            chart_events: Vec::new(),
            panel: None,
            replay_active: false,
            strategy_report: None,
            drawings: Default::default(),
            studies: Default::default(),
            time: Default::default(),
            replay: Default::default(),
            alerts: Default::default(),
            panels: Default::default(),
            trading: Default::default(),
            layouts: Default::default(),
            compare: Default::default(),
            commands: Default::default(),
        };
        for control in Self::DEFERRED {
            terminal.set_control_visible(control, false);
        }
        terminal
    }

    /// Sets the symbol shown at start. Settings read from [`Storage`] take precedence.
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.chart_mut().set_symbol(symbol);
        self
    }

    /// Sets the interval shown at start, which defaults to one day. Settings read from
    /// [`Storage`] take precedence.
    pub fn with_timeframe(mut self, timeframe: Timeframe) -> Self {
        self.chart_mut().set_timeframe(timeframe);
        self
    }

    /// Searches symbols through `service`.
    pub fn with_symbol_search(mut self, service: Arc<dyn SymbolSearch>) -> Self {
        self.services.symbol_search = Some(service);
        self
    }

    /// Keeps drawings, settings, layouts and templates in `service`.
    pub fn with_storage(mut self, service: Arc<dyn Storage>) -> Self {
        self.settings = Some(SettingsStore::new(service.clone()));
        self.services.storage = Some(service);
        self
    }

    /// Keeps and evaluates alerts through `service`.
    pub fn with_alerts(mut self, service: Arc<dyn Alerts>) -> Self {
        self.services.alerts = Some(service);
        self
    }

    /// Offers `service` in the broker picker as `tile`. Call once per broker.
    pub fn with_broker(mut self, tile: BrokerTile, service: Arc<dyn Broker>) -> Self {
        self.services.brokers.push((tile, service));
        self
    }

    /// Draws the signals of `service` on every chart. Call once per source: each keeps its own
    /// stream, so one source's `Clear` leaves the others' signals.
    pub fn with_signals(mut self, service: Arc<dyn Signals>) -> Self {
        self.services.signals.push(service);
        self
    }

    /// Draws the marks of `service` on every chart.
    pub fn with_marks(mut self, service: Arc<dyn Marks>) -> Self {
        self.services.marks = Some(service);
        self
    }

    /// Fills the watchlist with the quotes of `service`.
    pub fn with_quotes(mut self, service: Arc<dyn Quotes>) -> Self {
        self.services.quotes = Some(service);
        self
    }

    /// Fills the notifications panel from `service`.
    pub fn with_notifications(mut self, service: Arc<dyn Notifications>) -> Self {
        self.services.notifications = Some(service);
        self
    }

    /// Fills the chats panel from `service`.
    pub fn with_chats(mut self, service: Arc<dyn Chats>) -> Self {
        self.services.chats = Some(service);
        self
    }

    /// Offers `formats` in the export dialog and hands each finished export to `save`. Without
    /// it, exports go to the clipboard only.
    pub fn with_file_export(
        mut self,
        formats: &'static [ExportFormat],
        save: impl FnMut(ExportFile) + 'static,
    ) -> Self {
        self.services.export_formats = formats;
        self.services.save = Some(Box::new(save));
        self
    }

    /// Offers the sounds `names` and plays one by calling `play` with its name and a volume from
    /// zero to one.
    pub fn with_sounds(
        mut self,
        names: Vec<String>,
        play: impl FnMut(&str, f32) + 'static,
    ) -> Self {
        self.services.sounds = names;
        self.services.play = Some(Box::new(play));
        self
    }

    /// Returns the actions for which `intercept` is true from [`show`](Self::show) instead of
    /// performing them. [`run`](Self::run) performs an intercepted action.
    pub fn with_intercept(mut self, intercept: impl FnMut(&Action) -> bool + 'static) -> Self {
        self.intercept = Some(Box::new(intercept));
        self
    }

    /// Hides `control` in every component of the terminal.
    pub fn hide(mut self, control: Control) -> Self {
        self.hidden.insert(control);
        self.set_control_visible(control, false);
        self
    }

    /// Performs `action` whatever the intercept says, and returns it when nothing in the terminal
    /// performs it. Before the first [`show`](Self::show) the action waits for the first frame,
    /// and what it leaves is returned from that frame's `show`.
    pub fn run(&mut self, action: Action) -> Option<Action> {
        let Some(ctx) = self.ctx.clone() else {
            self.pending.push(action);
            return None;
        };
        ctx.request_repaint();
        self.perform(action)
    }

    /// The active chart slot.
    pub fn chart(&self) -> &Chart {
        &self.charts[self.active]
    }

    /// The active chart slot, to customise its options, studies and drawings.
    pub fn chart_mut(&mut self) -> &mut Chart {
        &mut self.charts[self.active]
    }

    /// The chart context menus, to add host entries.
    pub fn menus_mut(&mut self) -> &mut ChartMenus {
        &mut self.menus
    }

    /// The top toolbar, to add host entries and a call to action.
    pub fn top_toolbar_mut(&mut self) -> &mut TopToolbar {
        &mut self.top_toolbar
    }

    /// Shows `report` in the strategy report panel, or closes it with `None`.
    pub fn set_strategy_report(&mut self, report: Option<StrategyReport>) {
        self.strategy_report = report;
    }

    /// Passes `action` along the groups until one performs it, and returns what is left.
    /// A group that takes an action only in its own state comes before the group that performs
    /// the default. Host entries are always left for the host.
    pub(super) fn perform(&mut self, action: Action) -> Option<Action> {
        use crate::ui::context_menu::ChartContextAction;
        use crate::ui::top_toolbar::TopToolbarAction;
        if matches!(
            action,
            Action::TopToolbar(TopToolbarAction::Host(_))
                | Action::ChartMenu(ChartContextAction::Host { .. })
        ) {
            return Some(action);
        }
        let action = self.core_perform(action)?;
        let action = self.drawings_perform(action)?;
        let action = self.panels_perform(action)?;
        let action = self.compare_perform(action)?;
        let action = self.studies_perform(action)?;
        let action = self.time_perform(action)?;
        let action = self.replay_perform(action)?;
        let action = self.alerts_perform(action)?;
        let action = self.trading_perform(action)?;
        let action = self.layouts_perform(action)?;
        self.commands_perform(action)
    }
}

impl Drop for Terminal {
    /// Writes changed settings, as an application does when it exits.
    fn drop(&mut self) {
        if self.settings.is_some() {
            let current = self.user_settings();
            if let Some(settings) = &mut self.settings {
                settings.flush(&current);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{self, MemoryStorage, SIZE, TestFeed, click, frame, key};
    use super::*;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::ui::top_toolbar::TopToolbarAction;
    use crate::widget::ShortcutRequest;
    use egui::{Key, Modifiers, vec2};

    #[test]
    fn intercepted_actions_are_returned_not_performed() {
        let is_undo =
            |action: &Action| matches!(action, Action::TopToolbar(TopToolbarAction::Undo));
        // Whether Undo is intercepted, whether it is run before the first frame instead of
        // clicked, and the drawings left after the click or the first frame.
        for (intercept, queued, drawings) in [(true, false, 1), (false, false, 0), (true, true, 0)]
        {
            let mut terminal = Terminal::new(Arc::new(TestFeed)).with_symbol("AAPL");
            if intercept {
                terminal = terminal.with_intercept(is_undo);
            }
            let mut line = Drawing::new(0, DrawingToolType::HorizontalLine);
            line.chart_points = vec![ChartPoint::new(0.0, 180.0)];
            terminal
                .chart_mut()
                .user_action(|chart| chart.drawings_mut().add_drawing(line).unwrap());
            if queued {
                assert!(
                    terminal
                        .run(Action::TopToolbar(TopToolbarAction::Undo))
                        .is_none()
                );
            }
            let first = frame(&mut terminal, SIZE, vec![]);
            let returned = if queued {
                first.actions
            } else {
                click(
                    &mut terminal,
                    SIZE,
                    first.text_at("Undo").expect("undo button"),
                )
            };
            assert_eq!(terminal.chart().drawings().drawings.len(), drawings);
            if drawings == 1 {
                let [undo] = <[Action; 1]>::try_from(returned).expect("one action");
                assert!(is_undo(&undo));
                assert!(terminal.run(undo).is_none());
                assert!(terminal.chart().drawings().drawings.is_empty());
            } else {
                assert!(returned.is_empty());
            }
        }
        // Host entries and actions no group performs go back to the host, run or emitted.
        let mut terminal = Terminal::new(Arc::new(TestFeed)).with_symbol("AAPL");
        frame(&mut terminal, SIZE, vec![]);
        for action in [
            Action::TopToolbar(TopToolbarAction::Host("x".into())),
            Action::Shortcut(ShortcutRequest::SymbolSearch("A".into())),
        ] {
            assert!(terminal.run(action).is_some());
        }
        let help = frame(&mut terminal, SIZE, vec![key(Key::F1, Modifiers::NONE)]).actions;
        assert!(matches!(help.as_slice(), [Action::Shortcut(_)]));
    }

    #[test]
    fn theme_selection_survives_restart_and_initializes_the_chart() {
        let size = vec2(393.0, 852.0);
        let system = |terminal: &mut Terminal, theme| {
            test_support::frame_with(terminal, size, vec![], |input| {
                input.system_theme = Some(theme);
            })
        };
        for initial in [None, Some(egui::Theme::Light), Some(egui::Theme::Dark)] {
            let storage = Arc::new(MemoryStorage::default());
            for explicit in [false, true, true] {
                let mut terminal = Terminal::new(Arc::new(TestFeed))
                    .with_symbol("AAPL")
                    .with_storage(storage.clone());
                test_support::frame_with(&mut terminal, size, vec![], |input| {
                    input.system_theme = initial;
                });
                if !explicit {
                    assert_eq!(terminal.theme.dark, initial == Some(egui::Theme::Dark));
                }
                if explicit {
                    terminal.run(Action::TopToolbar(TopToolbarAction::ThemeToggle));
                }
                let choice = explicit.then_some(terminal.theme.dark);
                terminal.charts.push(ChartBuilder::new().build());
                terminal.wire_slot(1);
                for appearance in [egui::Theme::Dark, egui::Theme::Light, egui::Theme::Dark] {
                    system(&mut terminal, appearance);
                    let expected = if choice.unwrap_or(appearance == egui::Theme::Dark) {
                        Theme::dark()
                    } else {
                        Theme::light()
                    };
                    assert_eq!(terminal.theme, expected);
                    for chart in &terminal.charts {
                        assert_eq!(chart.config().background_color, expected.chart.bg);
                    }
                    let ctx = terminal.ctx.clone().unwrap();
                    assert_eq!(crate::theme::ThemeExt::active_theme(&ctx), expected);
                }
                drop(terminal);
                let mut restored = Terminal::new(Arc::new(TestFeed))
                    .with_symbol("AAPL")
                    .with_storage(storage.clone());
                system(&mut restored, egui::Theme::Light);
                assert_eq!(restored.theme.dark, choice.unwrap_or(false));
                assert_eq!(
                    restored.user_settings().theme,
                    choice.map(|dark| if dark { "dark" } else { "light" }.into())
                );
                assert_eq!(
                    restored.chart().config().background_color,
                    restored.theme.chart.bg
                );
                assert_eq!(
                    restored.chart().config().bearish_color,
                    restored.theme.chart.candle_down
                );
            }
        }
    }
}
