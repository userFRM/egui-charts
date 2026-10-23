//! Floating Replay Control
//!
//! A floating toolbar for controlling bar replay.
//! Displays play/pause, step controls, and speed selector.

use crate::ext::UiExt;
use crate::icons::icons;
use crate::styles::icons as icon_sizes;
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::{Button, ButtonSize, IconButton};
use egui::{
    Align2, Context, CursorIcon, FontId, Id, Key, Modifiers, Rect, RichText, Ui, Vec2, pos2,
};

/// Replay speed presets for playback control
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReplaySpeed {
    X0_5,
    X1,
    X2,
    X3,
    X5,
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
    /// Close the control (but don't stop replay)
    Close,
}

/// Floating replay control widget
pub struct FloatingReplayControl {
    /// Widget ID for state persistence
    id: Id,
    /// Current state (public for external access)
    pub state: ReplayControlState,
    /// Area the bar docks in, set by the host each frame
    dock_rect: Option<Rect>,
    /// Whether control is visible
    visible: bool,
    /// User explicitly dismissed the control (prevents auto-show until replay restarts)
    user_dismissed: bool,
    /// Replay is on its last bar, where Next and Go to end do nothing
    at_end: bool,
}

impl Default for FloatingReplayControl {
    fn default() -> Self {
        Self::new()
    }
}

impl FloatingReplayControl {
    /// Create a new floating replay control
    pub fn new() -> Self {
        Self {
            id: Id::unique("floating_replay_control"),
            state: ReplayControlState::default(),
            dock_rect: None,
            visible: false,
            user_dismissed: false,
            at_end: false,
        }
    }

    /// Dock the bar in `rect`, usually the chart's rect.
    ///
    /// The bar opens centred at the bottom of this rect and can be dragged only within it. Call
    /// it before [`Self::show`] in each frame; without it the bar docks in the window's content
    /// rect.
    pub fn set_dock_rect(&mut self, rect: Rect) {
        self.dock_rect = Some(rect);
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

        let dock = self.dock_rect.unwrap_or_else(|| ctx.content_rect());
        let inset = ctx.active_theme().spacing.lg;
        egui::Area::new(self.id)
            .order(egui::Order::Foreground)
            .movable(true)
            .pivot(Align2::CENTER_BOTTOM)
            .default_pos(pos2(dock.center().x, dock.bottom() - inset))
            // Not inset: an area clips to its constraint, which would cut off the shadow.
            .constrain_to(dock)
            .show(ctx, |ui| {
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
    fn shortcut(&self, ctx: &Context) -> ReplayControlAction {
        let focus_off_background = ctx
            .memory(|m| m.focused())
            .and_then(|id| ctx.read_response(id))
            .is_some_and(|r| r.layer_id.order != egui::Order::Background);
        if ctx.text_edit_focused() || focus_off_background || egui::Popup::is_any_open(ctx) {
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
    /// Layout: grip | start, previous, play or pause, next, end | speed | symbol, progress | exit, hide
    fn render_control(&mut self, ui: &mut Ui) -> ReplayControlAction {
        let mut action = ReplayControlAction::None;

        // Floating surface colours follow the active theme.
        let theme = ui.active_theme();
        let visuals = ui.visuals();
        let row = ButtonSize::MD.pixels();

        egui::Frame::new()
            .fill(visuals.window_fill)
            .stroke(visuals.window_stroke)
            .corner_radius(visuals.window_corner_radius)
            .shadow(visuals.popup_shadow)
            .inner_margin(egui::Margin::symmetric(
                theme.spacing.sm as i8,
                theme.spacing.xs as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // The grip and first separator are laid out before any button sets the height.
                    ui.set_min_height(row);
                    ui.spacing_mut().item_spacing.x = theme.spacing.sm;
                    ui.spacing_mut().interact_size.y = row;

                    // The whole frame drags the bar; the grip says so.
                    ui.add(
                        icons::UI_DRAG_HANDLE
                            .as_image_tinted(Vec2::splat(icon_sizes::COMPACT), theme.ui.text_muted),
                    )
                    .on_hover_cursor(CursorIcon::Grab)
                    .on_hover_text("Move");

                    ui.separator();

                    let at_start = self.state.progress_pct <= 0.0;
                    let at_end = self.at_end;
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
                            .layout_no_wrap(text.to_owned(), font.clone(), theme.ui.text)
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
                    speed_response.response.on_hover_text("Playback speed");
                    if speed_response.inner {
                        action = ReplayControlAction::SetSpeed(speed.multiplier());
                    }

                    ui.separator();

                    if !self.state.symbol.is_empty() {
                        ui.label(
                            RichText::new(&self.state.symbol)
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

                    let exit = ui
                        .scope(|ui| {
                            let widgets = &mut ui.visuals_mut().widgets;
                            for visuals in [
                                &mut widgets.inactive,
                                &mut widgets.hovered,
                                &mut widgets.active,
                            ] {
                                visuals.fg_stroke.color = theme.ui.error;
                            }
                            Button::new("Exit replay")
                                .secondary()
                                .size(ButtonSize::MD)
                                .show(ui)
                        })
                        .inner;
                    if exit.clicked() {
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
            });

        action
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
                ReplayControlAction::GoToStart,
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
