//! Floating Replay Control
//!
//! A toolbar for controlling bar replay, docked in a strip the host reserves below its chart or
//! floating over a rect. Displays the start control, play/pause, step controls, and speed selector.

use super::start::ReplayStartAction;
use crate::icons::icons;
use crate::localization as l10n;
use crate::styles::icons as icon_sizes;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{Button, ButtonSize, ButtonVariant, IconButton};
use crate::ui_kit::ext::UiExt;
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

    /// From multiplier (find closest)
    pub fn from_multiplier(mult: f32) -> Self {
        Self::all()
            .iter()
            .min_by(|a, b| {
                let diff_a = (a.multiplier() - mult).abs();
                let diff_b = (b.multiplier() - mult).abs();
                diff_a
                    .partial_cmp(&diff_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
            .unwrap_or(Self::X1)
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
    /// Close the control (but don't stop replay)
    Close,
    /// Start, or restart, the replay from this choice of the start control.
    Start(ReplayStartAction),
}

/// Floating replay control widget
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
    /// Whether control is visible
    visible: bool,
    /// User explicitly dismissed the control (prevents auto-show until replay restarts)
    user_dismissed: bool,
    /// Replay is on its last bar, where Next and Go to end do nothing
    at_end: bool,
    update_intervals: Vec<crate::model::Timeframe>,
    update_interval: Option<crate::model::Timeframe>,
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
            visible: false,
            user_dismissed: false,
            at_end: false,
            update_intervals: Vec::new(),
            update_interval: None,
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
    /// focused chart in a panel keeps them.
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
        // Shift+arrow first: a pattern without Shift also matches the key with Shift held.
        [
            (Modifiers::NONE, Key::Space, play_or_pause),
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
        .find(|(modifiers, key, _)| ctx.input_mut(|i| i.consume_key(*modifiers, *key)))
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
                                    Vec2::splat(icon_sizes::COMPACT),
                                    theme.ui.text_muted,
                                ))
                                .on_hover_cursor(CursorIcon::Grab)
                                .on_hover_text(l10n::text(ui.ctx(), "Move"));

                                ui.separator();
                            }

                            if let Some(start) = self.show_start_control(ui) {
                                action = ReplayControlAction::Start(start);
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
                                    "Pause (Space)",
                                    ReplayControlAction::Pause,
                                )
                            } else {
                                (
                                    &icons::MEDIA_PLAY,
                                    "Play (Space)",
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

                            // As wide as the widest label, so the bar keeps its width for every speed.
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
                            let spacing = ui.spacing();
                            let combo_width = text_width(ui, ReplaySpeed::X0_5.label())
                                + spacing.icon_spacing
                                + spacing.icon_width
                                + 2.0 * spacing.button_padding.x;
                            let mut speed = ReplaySpeed::from_multiplier(self.state.speed);
                            let speed_response = ui.scope(|ui| {
                                ui.combo_select_width(
                                    "replay_speed_selector",
                                    &mut speed,
                                    ReplaySpeed::all().iter().copied(),
                                    |speed| speed.label().to_owned(),
                                    combo_width,
                                )
                            });
                            speed_response
                                .response
                                .on_hover_text(l10n::text(ui.ctx(), "Playback speed"));
                            if speed_response.inner {
                                action = ReplayControlAction::SetSpeed(speed.multiplier());
                            }

                            if !self.update_intervals.is_empty()
                                && let Some(mut interval) = self.update_interval
                                && ui
                                    .add_enabled_ui(!self.selecting, |ui| {
                                        ui.combo_select_width(
                                            "replay_update_interval",
                                            &mut interval,
                                            self.update_intervals.iter().copied(),
                                            |interval| format!("{} updates", interval),
                                            110.0,
                                        )
                                    })
                                    .inner
                            {
                                action = ReplayControlAction::SetUpdateInterval(interval);
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

    /// The start control: a button that selects a bar, and a menu of every start.
    fn show_start_control(&self, ui: &mut Ui) -> Option<ReplayStartAction> {
        let mut start = None;
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
            start = Some(ReplayStartAction::SelectBar);
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
                        start = Some(choice);
                        ui.close();
                    }
                }
            });
        start
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
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
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

    /// Every start other than a click on a bar stays one menu away on the bar, in the strip.
    #[test]
    fn start_control_offers_every_start_and_selection_takes_the_transport_keys() {
        use egui_kittest::{Harness, kittest::Queryable};
        for (can_resume, label, start) in [
            (false, "Select bar", ReplayStartAction::SelectBar),
            (false, "Select date…", ReplayStartAction::SelectDate),
            (false, "First available bar", ReplayStartAction::First),
            (false, "Random bar", ReplayStartAction::Random),
            (true, "Resume saved session", ReplayStartAction::Resume),
        ] {
            let mut bar = FloatingReplayControl::new();
            bar.show_control();
            bar.set_can_resume(can_resume);
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
            harness
                .get_by_role_and_label(egui::Role::MenuItem, label)
                .click();
            harness.run();
            assert_eq!(harness.state().1, ReplayControlAction::Start(start));
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
            frame(&ctx, &mut bar, vec![key(Key::Space)], false),
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

    /// The key a shortcut reads goes to the replay only when nothing else should take it.
    #[test]
    fn shortcuts_leave_keys_to_focused_widgets() {
        // (keys before the checked frame, focus the text field, checked key, expected action)
        let cases = [
            (vec![], false, Key::Space, ReplayControlAction::Pause),
            (vec![], false, Key::Escape, ReplayControlAction::None),
            (vec![], true, Key::Space, ReplayControlAction::None),
            // Tab focuses the bar's first button, which Space then presses.
            (
                vec![key(Key::Tab)],
                false,
                Key::Space,
                ReplayControlAction::Start(ReplayStartAction::SelectBar),
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
            let action = frame(&ctx, &mut bar, vec![key(pressed)], focus_field);
            assert_eq!(
                action, expected,
                "{pressed:?}, text field focused: {focus_field}"
            );
        }
    }
}
