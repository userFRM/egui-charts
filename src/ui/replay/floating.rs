//! Floating Replay Control
//!
//! A toolbar for controlling bar replay, docked in a strip the host reserves below its chart or
//! floating over a rect. Displays the start control, play/pause, step controls, and speed selector.

use super::start::ReplayStartAction;
use crate::icons::icons;
use crate::localization as l10n;
use crate::model::ReplayScope;
use crate::services::ReplayAccountSettings;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{Button, ButtonSize, ButtonVariant, IconButton};
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::form::FormGrid;
use crate::ui_kit::list_item::ListItem;
use egui::{
    Align2, Context, CursorIcon, FontId, Id, Key, Modifiers, Rect, RichText, Ui, Vec2, pos2,
};

/// Replay speed presets for playback control
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReplaySpeed {
    /// Play at 0.5 times the original speed.
    X0_5,
    /// Play at 1 times the original speed.
    X1,
    /// Play at 2 times the original speed.
    X2,
    /// Play at 3 times the original speed.
    X3,
    /// Play at 5 times the original speed.
    X5,
    /// Play at 10 times the original speed.
    X10,
}

impl ReplaySpeed {
    /// Get the multiplier value
    pub fn multiplier(&self) -> f32 {
        match self {
            Self::X0_5 => 0.5,
            Self::X1 => 1.0,
            Self::X2 => 2.0,
            Self::X3 => 3.0,
            Self::X5 => 5.0,
            Self::X10 => 10.0,
        }
    }

    /// Get display label
    pub fn label(&self) -> &'static str {
        match self {
            Self::X0_5 => "0.5x",
            Self::X1 => "1x",
            Self::X2 => "2x",
            Self::X3 => "3x",
            Self::X5 => "5x",
            Self::X10 => "10x",
        }
    }

    /// All speeds in order (slowest to fastest)
    pub fn all() -> &'static [Self] {
        &[
            Self::X0_5,
            Self::X1,
            Self::X2,
            Self::X3,
            Self::X5,
            Self::X10,
        ]
    }
}

/// Current replay state
#[derive(Debug, Clone, Default)]
pub struct ReplayControlState {
    /// Is replay currently playing
    pub is_playing: bool,
    /// Current speed
    pub speed: f32,
    /// Current symbol being replayed
    pub symbol: String,
    /// Progress percentage (0.0 - 100.0)
    pub progress_pct: f32,
    /// Ticks processed
    pub ticks_processed: u64,
    /// Current time being replayed
    pub current_time: String,
}

/// Actions emitted by the replay control
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ReplayControlAction {
    /// No action
    None,
    /// Play/resume replay
    Play,
    /// Pause replay
    Pause,
    /// Stop and close replay
    Stop,
    /// Step forward one bar
    StepForward,
    /// Step backward one bar
    StepBackward,
    /// Go to first bar
    GoToStart,
    /// Go to previous bar
    PreviousBar,
    /// Go to next bar
    NextBar,
    /// Go to current/end bar
    GoToEnd,
    /// Change replay speed
    SetSpeed(f32),
    /// Change the source interval used to advance the replay cursor.
    SetUpdateInterval(crate::model::Timeframe),
    /// Let the host choose the chart interval, or the largest common compatible interval of
    /// the replayed charts, and follow subsequent chart interval changes. Confirm the choice
    /// with [`FloatingReplayControl::set_automatic_update_interval`].
    SetAutomaticUpdateInterval,
    /// Close the control (but don't stop replay)
    Close,
    /// Start, or restart, the replay from this choice of the start control.
    Start(ReplayStartAction),
    /// Cover these charts with the replay; the host passes the scope to
    /// [`ChartLayout::set_replay_time`](crate::widget::ChartLayout::set_replay_time) and back to
    /// [`FloatingReplayControl::set_scope`].
    SetScope(ReplayScope),
}

/// Floating replay control widget
#[derive(Debug)]
pub struct FloatingReplayControl {
    /// Widget ID for state persistence
    id: Id,
    /// Current state (public for external access)
    pub state: ReplayControlState,
    /// Area the bar docks in, set by the host each frame
    dock_rect: Option<Rect>,
    /// `dock_rect` is a strip reserved for the bar rather than a rect the bar floats over
    docked: bool,
    /// The host waits for a click on a bar, so the start control is lit and the transport is off
    selecting: bool,
    /// The start control offers the host's saved session
    can_resume: bool,
    /// Charts the replay covers, offered in the start menu when set
    scope: Option<ReplayScope>,
    /// Whether control is visible
    visible: bool,
    /// User explicitly dismissed the control (prevents auto-show until replay restarts)
    user_dismissed: bool,
    /// Replay is on its last bar, where Next and Go to end do nothing
    at_end: bool,
    update_intervals: Vec<crate::model::Timeframe>,
    update_interval: Option<crate::model::Timeframe>,
    automatic_update_interval: bool,
    /// The replay account the start menu offers
    account: ReplayAccountSettings,
    /// The text of the start menu's currency field, which may be blank while the user edits it
    currency: String,
}

impl Default for FloatingReplayControl {
    fn default() -> Self {
        Self::new()
    }
}

impl FloatingReplayControl {
    /// Height of the strip to reserve for [`Self::set_dock_strip`].
    pub const STRIP_HEIGHT: f32 = 48.0;

    /// Create a new floating replay control
    pub fn new() -> Self {
        let account = ReplayAccountSettings::default();
        Self {
            id: Id::unique("floating_replay_control"),
            state: ReplayControlState {
                speed: 1.0,
                ..Default::default()
            },
            dock_rect: None,
            docked: false,
            selecting: false,
            can_resume: false,
            scope: None,
            visible: false,
            user_dismissed: false,
            at_end: false,
            update_intervals: Vec::new(),
            update_interval: None,
            automatic_update_interval: false,
            currency: account.currency.clone(),
            account,
        }
    }

    /// Float the bar over `rect`.
    ///
    /// The bar opens centred at the bottom of this rect and can be dragged only within it, so it
    /// covers what is drawn there; to keep a chart's time axis clear, reserve a strip for
    /// [`Self::set_dock_strip`] instead. Call it before [`Self::show`] in each frame; without it
    /// the bar floats over the window's content rect.
    pub fn set_dock_rect(&mut self, rect: Rect) {
        self.dock_rect = Some(rect);
        self.docked = false;
    }

    /// Dock the bar in `strip`, a band [`Self::STRIP_HEIGHT`] high that the host reserves between
    /// its chart and bottom bar while [`Self::is_visible`].
    ///
    /// The bar is centred in the strip and drawn flat on the host's panel. Call it before
    /// [`Self::show`] in each frame.
    pub fn set_dock_strip(&mut self, strip: Rect) {
        self.dock_rect = Some(strip);
        self.docked = true;
    }

    /// Say whether the host waits for a click on a bar after [`ReplayStartAction::SelectBar`].
    ///
    /// Meanwhile the start control is lit and the transport and its shortcuts are off.
    pub fn set_selecting(&mut self, selecting: bool) {
        self.selecting = selecting;
    }

    /// Say whether the start control offers [`ReplayStartAction::Resume`].
    pub fn set_can_resume(&mut self, can_resume: bool) {
        self.can_resume = can_resume;
    }

    /// Say which charts the replay covers, offered as This chart and All charts in the start
    /// menu, which emits [`ReplayControlAction::SetScope`]. `None`, the default, hides the
    /// choice, for a host with one chart.
    pub fn set_scope(&mut self, scope: Option<ReplayScope>) {
        self.scope = scope;
    }

    /// Supplies source intervals the host can serve. An empty list hides the selector.
    /// Selecting one emits an intent; the host reloads that source and retains the cursor time.
    /// Offer `Timeframe::Ticks` of one for trade-by-trade updates when the host holds the trades
    /// (see [`Chart::set_replay_source`](crate::Chart::set_replay_source)).
    pub fn set_update_intervals(
        &mut self,
        current: crate::model::Timeframe,
        available: Vec<crate::model::Timeframe>,
    ) {
        self.update_interval = Some(current);
        self.update_intervals = available;
    }

    /// Shows Automatic as the selected update interval. The host supplies its resolved interval
    /// through [`Self::set_update_intervals`] and follows chart interval changes while this is
    /// on. Manual selection emits [`ReplayControlAction::SetUpdateInterval`]; confirm it with
    /// `false`, the default. Every nonempty interval list offers Automatic and emits
    /// [`ReplayControlAction::SetAutomaticUpdateInterval`]; hosts must handle that action to
    /// resolve the interval before confirming it here.
    pub fn set_automatic_update_interval(&mut self, automatic: bool) {
        self.automatic_update_interval = automatic;
    }

    /// Whether the host confirmed Automatic as the current update interval.
    pub fn uses_automatic_update_interval(&self) -> bool {
        self.automatic_update_interval
    }

    /// The replay account the start menu offers: read it when the user picks a start, and save
    /// it with the host's settings.
    pub fn replay_account(&self) -> &ReplayAccountSettings {
        &self.account
    }

    /// Sets the replay account the start menu offers, as saved by the host. Returns `false`, and
    /// keeps the current one, when `settings` are not [valid](ReplayAccountSettings::is_valid).
    pub fn set_replay_account(&mut self, settings: ReplayAccountSettings) -> bool {
        if !settings.is_valid() {
            return false;
        }
        self.currency = settings.currency.clone();
        self.account = settings;
        true
    }

    /// Show the control (clears user_dismissed flag)
    pub fn show_control(&mut self) {
        self.visible = true;
        self.user_dismissed = false;
    }

    /// Hide the control
    pub fn hide_control(&mut self) {
        self.visible = false;
    }

    /// Hide control via user action (sets user_dismissed flag to prevent auto-show)
    pub fn dismiss(&mut self) {
        self.visible = false;
        self.user_dismissed = true;
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Check if user explicitly dismissed (prevents auto-show)
    pub fn is_user_dismissed(&self) -> bool {
        self.user_dismissed
    }

    /// Update the state
    pub fn update_state(&mut self, state: ReplayControlState) {
        self.at_end = state.progress_pct >= 100.0;
        self.state = state;
        // Auto-show when replay is active
        if self.state.is_playing || self.state.ticks_processed > 0 {
            self.visible = true;
        }
    }

    /// Set playing state
    pub fn set_playing(&mut self, playing: bool) {
        self.state.is_playing = playing;
    }

    /// Sync replay control state from external values.
    ///
    /// Drives visibility and playback display from central replay state.
    pub fn sync_from(
        &mut self,
        active: bool,
        playing: bool,
        speed: f32,
        current_bar: usize,
        total_bars: usize,
    ) {
        self.state.is_playing = playing;
        self.state.speed = speed;

        // `current_bar` is zero-based, so the last bar is 100%.
        self.state.progress_pct =
            current_bar as f32 / total_bars.saturating_sub(1).max(1) as f32 * 100.0;

        self.state.ticks_processed = current_bar as u64;
        // A one-bar replay is at its start and its end.
        self.at_end = current_bar + 1 >= total_bars;

        if active {
            if !self.user_dismissed {
                self.visible = true;
            }
        } else {
            self.user_dismissed = false;
        }
    }

    /// Set speed
    pub fn set_speed(&mut self, speed: f32) {
        self.state.speed = speed;
    }

    /// Set symbol
    pub fn set_symbol(&mut self, symbol: &str) {
        self.state.symbol = symbol.to_string();
    }

    /// Show the floating control and return any action
    pub fn show(&mut self, ctx: &Context) -> ReplayControlAction {
        if !self.visible {
            return ReplayControlAction::None;
        }

        let mut action = self.shortcut(ctx);

        let dock = self
            .dock_rect
            .unwrap_or_else(|| ctx.content_rect())
            .intersect(ctx.content_rect());
        let inset = ctx.active_theme().spacing.lg.max(8.0);
        let area = egui::Area::new(self.id).order(egui::Order::Foreground);
        let area = if self.docked {
            area.movable(false)
                .pivot(Align2::CENTER_CENTER)
                .current_pos(dock.center())
                .constrain_to(dock)
        } else {
            area.movable(true)
                .pivot(Align2::CENTER_BOTTOM)
                .default_pos(pos2(dock.center().x, dock.bottom() - inset))
                .constrain_to(dock.shrink(inset))
        };
        area.show(ctx, |ui| {
            ui.set_max_width((dock.width() - 2.0 * inset).max(0.0));
            // Keep the shadow outside the inset while constraining the toolbar itself.
            ui.set_clip_rect(dock);
            let clicked = self.render_control(ui);
            if clicked != ReplayControlAction::None {
                action = clicked;
            }
        });

        action
    }

    /// The action of a shortcut pressed this frame, which takes its key.
    ///
    /// Shortcuts act only while no text field has focus, no popup is open and no control
    /// outside the background layer has focus, so keys typed into a field, meant for an open
    /// popup or pressing a focused button in the bar or a dialog never reach the replay. A
    /// focused chart in a panel keeps them. Each key needs exactly its modifiers: Space, and keys
    /// with Alt or another extra modifier held, are left to the chart.
    pub fn shortcut(&self, ctx: &Context) -> ReplayControlAction {
        let focus_off_background = ctx
            .memory(|m| m.focused())
            .and_then(|id| ctx.read_response(id))
            .is_some_and(|r| r.layer_id.order != egui::Order::Background);
        if !self.visible
            || self.selecting
            || crate::widget::keyboard::shortcuts_suppressed(ctx)
            || ctx.text_edit_focused()
            || focus_off_background
            || egui::Popup::is_any_open(ctx)
        {
            return ReplayControlAction::None;
        }
        let play_or_pause = if self.state.is_playing {
            ReplayControlAction::Pause
        } else {
            ReplayControlAction::Play
        };
        [
            (Modifiers::SHIFT, Key::ArrowDown, play_or_pause),
            (Modifiers::NONE, Key::Home, ReplayControlAction::GoToStart),
            (Modifiers::NONE, Key::End, ReplayControlAction::GoToEnd),
            (
                Modifiers::SHIFT,
                Key::ArrowLeft,
                ReplayControlAction::StepBackward,
            ),
            (
                Modifiers::SHIFT,
                Key::ArrowRight,
                ReplayControlAction::StepForward,
            ),
            (
                Modifiers::NONE,
                Key::ArrowLeft,
                ReplayControlAction::PreviousBar,
            ),
            (
                Modifiers::NONE,
                Key::ArrowRight,
                ReplayControlAction::NextBar,
            ),
        ]
        .into_iter()
        // Exactly these modifiers, as the chart matches its keys, so Alt+Shift+arrows stay its jumps.
        .find(|(modifiers, key, _)| {
            ctx.input_mut(|input| {
                let before = input.events.len();
                input.events.retain(|event| {
                    !matches!(event, egui::Event::Key { key: pressed, pressed: true, modifiers: held, .. }
                        if pressed == key && held.matches_exact(*modifiers))
                });
                input.events.len() != before
            })
        })
        .map_or(ReplayControlAction::None, |(_, _, action)| action)
    }

    /// Render the control contents
    /// Layout: grip | start control | start, previous, play or pause, next, end | speed | symbol,
    /// progress | exit, hide. A docked bar has no grip.
    fn render_control(&mut self, ui: &mut Ui) -> ReplayControlAction {
        let mut action = ReplayControlAction::None;

        // Floating surface colours follow the active theme.
        let theme = ui.active_theme();
        let visuals = ui.visuals();
        let row = ButtonSize::MD.pixels();

        let frame = if self.docked {
            egui::Frame::new()
        } else {
            egui::Frame::new()
                .fill(visuals.window_fill)
                .stroke(visuals.window_stroke)
                .corner_radius(visuals.window_corner_radius)
                .shadow(visuals.popup_shadow)
        };
        frame
            .inner_margin(egui::Margin::symmetric(
                theme.spacing.sm as i8,
                (theme.spacing.xs - visuals.window_stroke.width).max(0.0) as i8,
            ))
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .auto_shrink([true, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // The grip and first separator are laid out before any button sets the height.
                            ui.set_min_height(row);
                            ui.spacing_mut().item_spacing.x = theme.spacing.sm;
                            ui.spacing_mut().interact_size.y = row;

                            if !self.docked {
                                // The whole frame drags the bar; the grip says so.
                                ui.add(icons::UI_DRAG_HANDLE.as_image_tinted(
                                    Vec2::splat(DESIGN_TOKENS.sizing.icon_14),
                                    theme.ui.text_muted,
                                ))
                                .on_hover_cursor(CursorIcon::Grab)
                                .on_hover_text(l10n::text(ui.ctx(), "Move"));

                                ui.separator();
                            }

                            if let Some(chosen) = self.show_start_control(ui) {
                                action = chosen;
                            }

                            ui.separator();

                            let at_start = self.selecting || self.state.progress_pct <= 0.0;
                            let at_end = self.selecting || self.at_end;
                            if IconButton::new(&icons::MEDIA_SKIP_BACK)
                                .tooltip("Go to start (Home)")
                                .enabled(!at_start)
                                .show(ui)
                                .clicked()
                            {
                                action = ReplayControlAction::GoToStart;
                            }

                            if IconButton::new(&icons::UI_ARROW_BACK)
                                .tooltip("Previous bar (Left arrow)")
                                .enabled(!at_start)
                                .show(ui)
                                .clicked()
                            {
                                action = ReplayControlAction::PreviousBar;
                            }

                            let (play_icon, play_tooltip, play_action) = if self.state.is_playing {
                                (
                                    &icons::MEDIA_PAUSE,
                                    "Pause (Shift+Down arrow)",
                                    ReplayControlAction::Pause,
                                )
                            } else {
                                (
                                    &icons::MEDIA_PLAY,
                                    "Play (Shift+Down arrow)",
                                    ReplayControlAction::Play,
                                )
                            };
                            if IconButton::new(play_icon)
                                .tooltip(play_tooltip)
                                .enabled(!self.selecting)
                                .show(ui)
                                .clicked()
                            {
                                action = play_action;
                            }

                            if IconButton::new(&icons::UI_ARROW_FORWARD)
                                .tooltip("Next bar (Right arrow)")
                                .enabled(!at_end)
                                .show(ui)
                                .clicked()
                            {
                                action = ReplayControlAction::NextBar;
                            }

                            if IconButton::new(&icons::MEDIA_SKIP_FORWARD)
                                .tooltip("Go to end (End)")
                                .enabled(!at_end)
                                .show(ui)
                                .clicked()
                            {
                                action = ReplayControlAction::GoToEnd;
                            }

                            ui.separator();

                            // As wide as the widest preset label or the current speed, so the bar
                            // keeps its width for every preset speed.
                            let font = FontId::proportional(theme.text.label);
                            let text_width = |ui: &Ui, text: &str| {
                                ui.painter()
                                    .layout_no_wrap(
                                        l10n::text(ui.ctx(), text),
                                        font.clone(),
                                        theme.ui.text,
                                    )
                                    .size()
                                    .x
                            };
                            // The speed as the replay plays it, which may be none of the presets.
                            let current = super::ReplaySpeed::from(self.state.speed).label();
                            let spacing = ui.spacing();
                            let combo_width = text_width(ui, ReplaySpeed::X0_5.label())
                                .max(text_width(ui, &current))
                                + spacing.icon_spacing
                                + spacing.icon_width
                                + 2.0 * spacing.button_padding.x;
                            let mut speed = self.state.speed;
                            let speed_response = ui.scope(|ui| {
                                ui.combo_select_width(
                                    "replay_speed_selector",
                                    &mut speed,
                                    ReplaySpeed::all().iter().map(ReplaySpeed::multiplier),
                                    |&speed| super::ReplaySpeed::from(speed).label(),
                                    combo_width,
                                )
                            });
                            speed_response
                                .response
                                .on_hover_text(l10n::text(ui.ctx(), "Playback speed"));
                            if speed_response.inner {
                                action = ReplayControlAction::SetSpeed(speed);
                            }

                            if !self.update_intervals.is_empty()
                                && let Some(current) = self.update_interval
                            {
                                let mut interval =
                                    (!self.automatic_update_interval).then_some(current);
                                let changed = ui
                                    .add_enabled_ui(!self.selecting, |ui| {
                                        let ctx = ui.ctx().clone();
                                        ui.combo_select_width(
                                            "replay_update_interval",
                                            &mut interval,
                                            std::iter::once(None).chain(
                                                self.update_intervals.iter().copied().map(Some),
                                            ),
                                            |interval| match interval {
                                                None => l10n::text(&ctx, "Automatic"),
                                                Some(interval) => l10n::format_text(
                                                    &ctx,
                                                    "{count} updates",
                                                    &[("count", interval.to_string())],
                                                ),
                                            },
                                            110.0,
                                        )
                                    })
                                    .inner;
                                if changed {
                                    action = interval.map_or(
                                        ReplayControlAction::SetAutomaticUpdateInterval,
                                        ReplayControlAction::SetUpdateInterval,
                                    );
                                }
                            }

                            ui.separator();

                            if !self.state.symbol.is_empty() {
                                ui.label(
                                    RichText::new(l10n::text(ui.ctx(), &self.state.symbol))
                                        .font(font.clone())
                                        .color(theme.ui.text),
                                );
                            }

                            // A slot as wide as "100.0%", so the bar keeps its width as progress changes.
                            ui.add_sized(
                                [text_width(ui, "100.0%"), row],
                                egui::Label::new(
                                    RichText::new(format!("{:.1}%", self.state.progress_pct))
                                        .font(font.clone())
                                        .color(theme.ui.text_secondary),
                                ),
                            );

                            ui.separator();

                            if Button::new("Exit replay")
                                .secondary()
                                .text_color(theme.ui.error)
                                .size(ButtonSize::MD)
                                .show(ui)
                                .clicked()
                            {
                                action = ReplayControlAction::Stop;
                            }

                            if IconButton::new(&icons::CLOSE)
                                .size(ButtonSize::MD)
                                .tooltip("Hide replay controls")
                                .show(ui)
                                .clicked()
                            {
                                action = ReplayControlAction::Close;
                            }
                        });
                        if let Some(response) = ui
                            .ctx()
                            .memory(|m| m.focused())
                            .and_then(|id| ui.ctx().read_response(id))
                            && response.layer_id == ui.layer_id()
                        {
                            ui.scroll_to_rect(response.rect, None);
                        }
                    });
            });

        action
    }

    /// The start control: a button to select a bar, every start, the charts covered and the
    /// replay account's terms.
    fn show_start_control(&mut self, ui: &mut Ui) -> Option<ReplayControlAction> {
        let mut chosen = None;
        if Button::icon_and_text(&icons::REPLAY_SELECT_BAR, "Select bar")
            .variant(if self.selecting {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Ghost
            })
            .tooltip("Click a bar on the chart to start replay there (Esc cancels)")
            .show(ui)
            .clicked()
        {
            chosen = Some(ReplayControlAction::Start(ReplayStartAction::SelectBar));
        }
        let menu = IconButton::new(&icons::CHEVRON_DOWN)
            .tooltip("Replay start")
            .show(ui);
        egui::Popup::menu(&menu)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(DESIGN_TOKENS.sizing.dialog.submenu_width);
                let theme = ui.active_theme();
                ui.label(
                    RichText::new(l10n::text(ui.ctx(), "Replay start").to_uppercase())
                        .size(theme.text.small)
                        .color(crate::ui_kit::section::menu_heading_color(&theme)),
                );
                let resume = self.can_resume.then_some((
                    &icons::REPLAY,
                    "Resume saved session",
                    ReplayStartAction::Resume,
                ));
                for (icon, label, choice) in [
                    (
                        &icons::REPLAY_SELECT_BAR,
                        "Select bar",
                        ReplayStartAction::SelectBar,
                    ),
                    (
                        &icons::WIDGET_BAR_CALENDAR,
                        "Select date…",
                        ReplayStartAction::SelectDate,
                    ),
                    (
                        &icons::FLAG_MARK,
                        "First available bar",
                        ReplayStartAction::First,
                    ),
                    (
                        &icons::REPLAY_RANDOM,
                        "Random bar",
                        ReplayStartAction::Random,
                    ),
                ]
                .into_iter()
                .chain(resume)
                {
                    if ListItem::new().show_with_icon(ui, label, icon).clicked() {
                        chosen = Some(ReplayControlAction::Start(choice));
                        ui.close();
                    }
                }
                ui.separator();
                ui.label(
                    RichText::new(l10n::text(ui.ctx(), "Replay account").to_uppercase())
                        .size(theme.text.small)
                        .color(crate::ui_kit::section::menu_heading_color(&theme)),
                );
                FormGrid::new("replay_account").show(ui, |ui| {
                    ui.label(l10n::text(ui.ctx(), "Initial capital"));
                    ui.add(
                        egui::DragValue::new(&mut self.account.initial_capital)
                            .range(1.0..=1.0e12)
                            .speed(100.0)
                            .max_decimals(2),
                    );
                    ui.end_row();
                    ui.label(l10n::text(ui.ctx(), "Currency"));
                    let currency = egui::TextEdit::singleline(&mut self.currency)
                        .char_limit(8)
                        .desired_width(64.0);
                    // A blank field keeps the last currency, so the settings stay valid.
                    if ui.add(currency).changed() && !self.currency.trim().is_empty() {
                        self.account.currency = self.currency.trim().to_uppercase();
                    }
                    ui.end_row();
                    ui.label(l10n::text(ui.ctx(), "Commission"));
                    let mut percent = self.account.commission * 100.0;
                    let commission = egui::DragValue::new(&mut percent)
                        .range(0.0..=100.0)
                        .speed(0.01)
                        .max_decimals(4)
                        .suffix("%");
                    // Written back only when edited, so an untouched rate never drifts.
                    if ui.add(commission).changed() {
                        self.account.commission = percent / 100.0;
                    }
                    ui.end_row();
                });
                let Some(current) = self.scope else {
                    return;
                };
                ui.separator();
                ui.label(
                    RichText::new(l10n::text(ui.ctx(), "Charts").to_uppercase())
                        .size(theme.text.small)
                        .color(crate::ui_kit::section::menu_heading_color(&theme)),
                );
                for (icon, label, scope) in [
                    (&icons::LAYOUT_SINGLE, "This chart", ReplayScope::ThisChart),
                    (&icons::LAYOUT_GRID, "All charts", ReplayScope::AllCharts),
                ] {
                    let item = ListItem::new()
                        .selected(scope == current)
                        .show_with_icon(ui, label, icon);
                    // One of two choices, so assistive technology hears which one is on.
                    ui.ctx().accesskit_node_builder(item.id, |node| {
                        node.set_role(egui::Role::MenuItemRadio);
                        node.set_toggled((scope == current).into());
                    });
                    // The menu stays open, so a start can follow.
                    if item.clicked() && scope != current {
                        chosen = Some(ReplayControlAction::SetScope(scope));
                    }
                }
            });
        chosen
    }
}

/// Show a simple replay control (function style)
pub fn show_floating_replay_control(
    ctx: &Context,
    control: &mut FloatingReplayControl,
) -> ReplayControlAction {
    control.show(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers, RawInput};

    fn key(key: Key) -> Event {
        key_with(key, Modifiers::NONE)
    }

    fn key_with(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// Runs one frame: the bar, then a text field, as the demo draws dialogs after the bar.
    fn frame(
        ctx: &Context,
        bar: &mut FloatingReplayControl,
        events: Vec<Event>,
        focus_field: bool,
    ) -> ReplayControlAction {
        let mut action = ReplayControlAction::None;
        let input = RawInput {
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            action = bar.show(ui.ctx());
            let field = ui.text_edit_singleline(&mut String::new());
            if focus_field {
                field.request_focus();
            }
        })
        .drop_without_applying_deltas();
        action
    }

    #[test]
    fn replay_surface_fits_the_dock_and_keeps_the_exit_label_readable() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut bar = FloatingReplayControl::new();
            bar.sync_from(true, false, 1.0, 150, 300);
            bar.set_symbol("SAMPLE");
            bar.set_update_intervals(
                crate::model::Timeframe::Hour1,
                vec![
                    crate::model::Timeframe::Hour1,
                    crate::model::Timeframe::Min1,
                ],
            );
            for width in [900.0, 320.0, 600.0] {
                let dock = Rect::from_min_max(pos2(0.0, 80.0), pos2(width, 560.0));
                bar.set_dock_rect(dock);
                for pass in 0..8 {
                    let mut neutral = egui::Color32::TRANSPARENT;
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                egui::Pos2::ZERO,
                                Vec2::new(width, 600.0),
                            )),
                            events: if width == 320.0 && pass == 4 {
                                vec![
                                    Event::PointerMoved(pos2(width / 2.0, dock.bottom() - 24.0)),
                                    Event::MouseWheel {
                                        unit: egui::MouseWheelUnit::Point,
                                        delta: egui::vec2(-1000.0, 0.0),
                                        phase: egui::TouchPhase::Move,
                                        source: egui::MouseWheelSource::Wheel,
                                        modifiers: Modifiers::NONE,
                                    },
                                ]
                            } else {
                                vec![]
                            },
                            time: Some(ctx.input(|i| i.time) + 1.0),
                            ..Default::default()
                        },
                        |ui| {
                            neutral = crate::ui_kit::buttons::ButtonVariant::Secondary
                                .bg_color(ui, false, false);
                            bar.show(ui.ctx());
                        },
                    );
                    output.textures_delta.clear();
                    if pass == 7 {
                        let rect = ctx.memory(|m| m.area_rect(bar.id)).unwrap();
                        assert!(dock.shrink(8.0).contains_rect(rect), "{width}: {rect:?}");
                        assert_eq!(rect.height(), 32.0);
                        let text = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                egui::Shape::Text(text) if text.galley.text() == "Exit replay" => {
                                    Some(text)
                                }
                                _ => None,
                            })
                            .unwrap();
                        assert_eq!(text.fallback_color, theme.ui.error);
                        assert!(rect.contains(text.pos + text.galley.size() / 2.0));
                        let center = text.pos + text.galley.size() / 2.0;
                        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                                egui::Shape::Rect(rect) if rect.rect.contains(center) && rect.rect.height() == ButtonSize::MD.pixels() && rect.fill == neutral)));
                    }
                    output.drop_without_applying_deltas();
                }
            }
        }
    }

    /// Every start other than a click on a bar stays one menu away on the bar, in the strip,
    /// and so does the choice of charts when the host offers it.
    #[test]
    fn start_control_offers_every_start_and_selection_takes_the_transport_keys() {
        use ReplayControlAction::{SetScope, Start};
        use ReplayScope::{AllCharts, ThisChart};
        use egui_kittest::{Harness, kittest::Queryable};
        for (can_resume, scope, label, expected) in [
            (
                false,
                None,
                "Select bar",
                Start(ReplayStartAction::SelectBar),
            ),
            (
                false,
                None,
                "Select date…",
                Start(ReplayStartAction::SelectDate),
            ),
            (
                false,
                None,
                "First available bar",
                Start(ReplayStartAction::First),
            ),
            (false, None, "Random bar", Start(ReplayStartAction::Random)),
            (
                true,
                None,
                "Resume saved session",
                Start(ReplayStartAction::Resume),
            ),
            (false, Some(ThisChart), "All charts", SetScope(AllCharts)),
            (false, Some(AllCharts), "This chart", SetScope(ThisChart)),
        ] {
            let mut bar = FloatingReplayControl::new();
            bar.show_control();
            bar.set_can_resume(can_resume);
            bar.set_scope(scope);
            let strip = Rect::from_min_size(pos2(0.0, 500.0), Vec2::new(900.0, 48.0));
            bar.set_dock_strip(strip);
            let mut harness = Harness::builder()
                .allow_missing_glyphs()
                .with_accessibility_check(false)
                .with_size(Vec2::new(900.0, 600.0))
                .build_ui_state(
                    |ui, (bar, action): &mut (FloatingReplayControl, ReplayControlAction)| {
                        let shown = bar.show(ui.ctx());
                        if shown != ReplayControlAction::None {
                            *action = shown;
                        }
                    },
                    (bar, ReplayControlAction::None),
                );
            harness.run();
            let rect = harness.ctx.memory(|m| m.area_rect(harness.state().0.id));
            assert!(strip.contains_rect(rect.unwrap()), "{rect:?} in {strip:?}");
            assert!(harness.query_by_label("Move").is_none());
            harness.get_by_label("Replay start").click();
            harness.run();
            assert_eq!(
                harness.query_by_label("Resume saved session").is_some(),
                can_resume
            );
            assert_eq!(
                harness.query_by_label("This chart").is_some(),
                scope.is_some()
            );
            let role = match expected {
                SetScope(_) => egui::Role::MenuItemRadio,
                _ => egui::Role::MenuItem,
            };
            harness.get_by_role_and_label(role, label).click();
            harness.run();
            assert_eq!(harness.state().1, expected);
        }

        // Waiting for the click leaves the transport off and its keys to the chart.
        let ctx = Context::default();
        let mut bar = FloatingReplayControl::new();
        bar.show_control();
        bar.set_selecting(true);
        for _ in 0..3 {
            frame(&ctx, &mut bar, vec![], false);
        }
        assert_eq!(
            frame(
                &ctx,
                &mut bar,
                vec![key_with(Key::ArrowDown, Modifiers::SHIFT)],
                false
            ),
            ReplayControlAction::None
        );
    }

    /// `current_bar` is zero-based: the first of 300 bars is 0% and the last 100%.
    #[test]
    fn progress_spans_first_to_last_bar() {
        let mut bar = FloatingReplayControl::new();
        for (current, expected) in [(0, 0.0), (299, 100.0)] {
            bar.sync_from(true, false, 1.0, current, 300);
            assert_eq!(bar.state.progress_pct, expected, "bar {current} of 300");
        }
    }

    /// A speed the bar does not offer reads as the replay plays it, not as the nearest preset.
    #[test]
    fn speed_selector_shows_the_replay_speed() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let shown: Vec<String> = [0.25_f32, 2.0, 100.0]
            .iter()
            .map(|&speed| {
                let mut bar = FloatingReplayControl::new();
                bar.show_control();
                bar.sync_from(true, false, speed, 0, 10);
                let mut harness = Harness::builder()
                    .allow_missing_glyphs()
                    .with_accessibility_check(false)
                    .with_size(Vec2::new(900.0, 600.0))
                    .build_ui_state(
                        |ui, bar: &mut FloatingReplayControl| {
                            bar.show(ui.ctx());
                        },
                        bar,
                    );
                harness.run();
                harness
                    .get_by_role(egui::Role::ComboBox)
                    .accesskit_node()
                    .value()
                    .map(|value| value.to_string())
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(shown, ["0.25x", "2x", "100x"]);
    }

    /// The key a shortcut reads goes to the replay only when nothing else should take it.
    #[test]
    fn shortcuts_leave_keys_to_focused_widgets() {
        let shift_down = key_with(Key::ArrowDown, Modifiers::SHIFT);
        // (keys before the checked frame, focus the text field, checked key, expected action)
        let cases = [
            (
                vec![],
                false,
                shift_down.clone(),
                ReplayControlAction::Pause,
            ),
            (vec![], false, key(Key::Escape), ReplayControlAction::None),
            (vec![], true, shift_down, ReplayControlAction::None),
            // Tab focuses the bar's first button, which Space then presses.
            (
                vec![key(Key::Tab)],
                false,
                key(Key::Space),
                ReplayControlAction::Start(ReplayStartAction::SelectBar),
            ),
            // Space and Shift+Space flip the watchlist, Alt+Shift+Right jumps the chart.
            (vec![], false, key(Key::Space), ReplayControlAction::None),
            (
                vec![],
                false,
                key_with(Key::Space, Modifiers::SHIFT),
                ReplayControlAction::None,
            ),
            (
                vec![],
                false,
                key_with(Key::ArrowRight, Modifiers::SHIFT | Modifiers::ALT),
                ReplayControlAction::None,
            ),
            (
                vec![],
                false,
                key_with(Key::ArrowRight, Modifiers::SHIFT),
                ReplayControlAction::StepForward,
            ),
        ];
        for (before, focus_field, pressed, expected) in cases {
            let ctx = Context::default();
            let mut bar = FloatingReplayControl::new();
            bar.sync_from(true, true, 1.0, 150, 300);
            for _ in 0..2 {
                frame(&ctx, &mut bar, vec![], focus_field);
            }
            frame(&ctx, &mut bar, before, focus_field);
            frame(&ctx, &mut bar, vec![], focus_field);
            let action = frame(&ctx, &mut bar, vec![pressed.clone()], focus_field);
            assert_eq!(
                action, expected,
                "{pressed:?}, text field focused: {focus_field}"
            );
        }
    }
}
