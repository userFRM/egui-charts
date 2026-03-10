//! Floating Replay Control
//!
//! A floating toolbar for controlling tick replay.
//! Displays play/pause, step controls, and speed selector.

use crate::icons::icons;
use crate::styles::icons as icon_sizes;
use crate::tokens::DESIGN_TOKENS;
use egui::{Context, Id, Pos2, RichText, Ui, Vec2};

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
    /// Position offset (for dragging)
    position: Option<Pos2>,
    /// Whether control is visible
    visible: bool,
    /// User explicitly dismissed the control (prevents auto-show until replay restarts)
    user_dismissed: bool,
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
            id: Id::new("floating_replay_control"),
            state: ReplayControlState::default(),
            position: None,
            visible: false,
            user_dismissed: false,
        }
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

        if total_bars > 0 {
            self.state.progress_pct = (current_bar as f32 / total_bars as f32) * 100.0;
        } else {
            self.state.progress_pct = 0.0;
        }

        self.state.ticks_processed = current_bar as u64;

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

        let mut action = ReplayControlAction::None;

        // Handle keyboard shortcuts
        ctx.input(|i| {
            // Escape - close control
            if i.key_pressed(egui::Key::Escape) {
                action = ReplayControlAction::Close;
            }
            // Space - play/pause
            else if i.key_pressed(egui::Key::Space) {
                action = if self.state.is_playing {
                    ReplayControlAction::Pause
                } else {
                    ReplayControlAction::Play
                };
            }
            // Home - go to start
            else if i.key_pressed(egui::Key::Home) {
                action = ReplayControlAction::GoToStart;
            }
            // End - go to end
            else if i.key_pressed(egui::Key::End) {
                action = ReplayControlAction::GoToEnd;
            }
            // Shift+Left - step backward
            else if i.modifiers.shift && i.key_pressed(egui::Key::ArrowLeft) {
                action = ReplayControlAction::StepBackward;
            }
            // Shift+Right - step forward
            else if i.modifiers.shift && i.key_pressed(egui::Key::ArrowRight) {
                action = ReplayControlAction::StepForward;
            }
            // Left Arrow - previous bar
            else if i.key_pressed(egui::Key::ArrowLeft) {
                action = ReplayControlAction::PreviousBar;
            }
            // Right Arrow - next bar
            else if i.key_pressed(egui::Key::ArrowRight) {
                action = ReplayControlAction::NextBar;
            }
        });

        if action != ReplayControlAction::None {
            return action;
        }

        // Calculate default position (centered horizontally, near bottom)
        let screen = ctx
            .input(|i| i.viewport().inner_rect)
            .unwrap_or(egui::Rect::ZERO);
        let control_width = 280.0;
        let control_height = 40.0;
        let default_pos = Pos2::new(
            (screen.width() - control_width) / 2.0,
            screen.height() - control_height - 80.0, // Above bottom toolbar
        );

        // Use stored position or default
        let pos = self.position.unwrap_or(default_pos);

        // Create floating window
        let response = egui::Area::new(self.id)
            .order(egui::Order::Foreground)
            .movable(true)
            .default_pos(pos)
            .show(ctx, |ui| {
                action = self.render_control(ui);
            });

        // Update stored position from drag
        if let Some(state) = ctx.memory(|m| m.area_rect(self.id)) {
            self.position = Some(state.min);
        }

        // Handle drag response
        if response.response.dragged() {
            self.position = Some(response.response.rect.min);
        }

        action
    }

    /// Render the control contents
    /// Layout: [⟵|] [|◀] [◀◀] [▶/⏸] [▶▶] [▶|] [|⟶] | Speed | [Exit]
    fn render_control(&mut self, ui: &mut Ui) -> ReplayControlAction {
        let mut action = ReplayControlAction::None;

        // Background frame - dark semi-transparent (uses chart tooltip styling)
        let bg_color = DESIGN_TOKENS.semantic.extended.chart_tooltip_bg;
        let border_color = DESIGN_TOKENS.semantic.chart.crosshair_label_bg;
        let btn_size = Vec2::splat(DESIGN_TOKENS.sizing.floating_toolbar.button_size);
        let icon_size = icon_sizes::COMPACT;

        egui::Frame::new()
            .fill(bg_color)
            .stroke(egui::Stroke::new(1.0, border_color))
            .corner_radius(DESIGN_TOKENS.rounding.md)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;

                    // Drag handle (grip icon)
                    ui.add(icons::UI_DRAG_HANDLE.as_image_tinted(
                        Vec2::splat(icon_size),
                        DESIGN_TOKENS.semantic.extended.text_muted,
                    ));

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // [⟵|] Go to first bar (Home) - using skip_back icon
                    let go_start = ui.add(
                        icons::MEDIA_SKIP_BACK
                            .as_button(Vec2::splat(icon_size))
                            .min_size(btn_size),
                    );
                    if go_start.on_hover_text("Go to start (Home)").clicked() {
                        action = ReplayControlAction::GoToStart;
                    }

                    // [|◀] Previous bar (Left Arrow) - using chevron left
                    let prev_bar = ui.add(
                        icons::UI_ARROW_BACK
                            .as_button(Vec2::splat(icon_size))
                            .min_size(btn_size),
                    );
                    if prev_bar.on_hover_text("Previous bar (←)").clicked() {
                        action = ReplayControlAction::PreviousBar;
                    }

                    // [▶/⏸] Play/Pause (Space)
                    let play_icon = if self.state.is_playing {
                        &icons::MEDIA_PAUSE
                    } else {
                        &icons::MEDIA_PLAY
                    };
                    let play_tooltip = if self.state.is_playing {
                        "Pause (Space)"
                    } else {
                        "Play (Space)"
                    };
                    let play_btn = ui.add(
                        egui::Button::image(play_icon.as_image_tinted(
                            Vec2::splat(icon_sizes::SM),
                            DESIGN_TOKENS.semantic.extended.bullish,
                        ))
                        .min_size(btn_size),
                    );
                    if play_btn.on_hover_text(play_tooltip).clicked() {
                        action = if self.state.is_playing {
                            ReplayControlAction::Pause
                        } else {
                            ReplayControlAction::Play
                        };
                    }

                    // [▶|] Next bar (Right Arrow) - using chevron right
                    let next_bar = ui.add(
                        icons::ARROW
                            .as_button(Vec2::splat(icon_size))
                            .min_size(btn_size),
                    );
                    if next_bar.on_hover_text("Next bar (→)").clicked() {
                        action = ReplayControlAction::NextBar;
                    }

                    // [|⟶] Go to end (End) - using skip_forward icon
                    let go_end = ui.add(
                        icons::MEDIA_SKIP_FORWARD
                            .as_button(Vec2::splat(icon_size))
                            .min_size(btn_size),
                    );
                    if go_end.on_hover_text("Go to end (End)").clicked() {
                        action = ReplayControlAction::GoToEnd;
                    }

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // Speed selector
                    let current_speed = ReplaySpeed::from_multiplier(self.state.speed);
                    egui::ComboBox::from_id_salt("replay_speed_selector")
                        .selected_text(
                            RichText::new(current_speed.label())
                                .size(12.0)
                                .color(DESIGN_TOKENS.semantic.chart.crosshair_label_text),
                        )
                        .width(55.0)
                        .show_ui(ui, |ui| {
                            for speed in ReplaySpeed::all() {
                                let is_selected = *speed == current_speed;
                                let response = ui.selectable_label(is_selected, speed.label());
                                if response.clicked() && !is_selected {
                                    action = ReplayControlAction::SetSpeed(speed.multiplier());
                                }
                            }
                        });

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // Symbol and progress info
                    if !self.state.symbol.is_empty() {
                        ui.label(
                            RichText::new(&self.state.symbol)
                                .size(11.0)
                                .color(DESIGN_TOKENS.semantic.chart.crosshair_label_text)
                                .strong(),
                        );
                    }

                    // Progress percentage
                    if self.state.progress_pct > 0.0 {
                        ui.label(
                            RichText::new(format!("{:.1}%", self.state.progress_pct))
                                .size(10.0)
                                .color(DESIGN_TOKENS.semantic.extended.text_muted),
                        );
                    }

                    ui.add_space(8.0);

                    // Exit Replay button (red text)
                    let exit_btn = ui.add(
                        egui::Button::new(
                            RichText::new("Exit Replay")
                                .size(11.0)
                                .color(DESIGN_TOKENS.semantic.extended.bearish),
                        )
                        .min_size(Vec2::new(70.0, btn_size.y)),
                    );
                    if exit_btn.clicked() {
                        action = ReplayControlAction::Stop;
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
