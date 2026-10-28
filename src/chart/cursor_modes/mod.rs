//! Cursor Modes Module
//!
//! Cursor modes for chart interaction.
//! Each cursor type provides different visual feedback and behavior.
//!
//! ## Cursor Types
//! - **Cross**: Full crosshair with vertical + horizontal lines
//! - **Dot**: Simple dot at cursor position
//! - **Arrow**: Standard pointer, no crosshair overlay
//! - **Eraser**: Click to delete drawings

pub mod eraser;

use crate::config::CrosshairStyle;

pub use eraser::EraserMode;

/// Aggregate state for all cursor modes.
///
/// Holds the state for each cursor mode variant (eraser).
/// The chart widget owns a single `CursorModeState` and delegates input events
/// and rendering to the individual mode states.
#[derive(Clone, Debug, Default)]
pub struct CursorModeState {
    /// Eraser mode: click on drawings to delete them.
    pub eraser: EraserMode,
    trail: Vec<(egui::Pos2, f32)>,
    last_pointer: Option<egui::Pos2>,
    trail_style: Option<CrosshairStyle>,
}

impl CursorModeState {
    /// Create a new state with all cursor modes inactive.
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance all cursor mode animations by `dt` seconds (call once per frame).
    pub fn update(&mut self, dt: f32) {
        for (_, age) in &mut self.trail {
            *age += dt.max(0.0);
        }
        self.trail.retain(|(_, age)| *age < 0.5);
    }

    /// Render all active cursor mode visual effects.
    pub fn render(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        use crate::theme::ThemeExt;
        let painter = ui.painter().with_clip_rect(rect);
        for pair in self.trail.windows(2) {
            painter.line_segment(
                [pair[0].0, pair[1].0],
                egui::Stroke::new(
                    6.0,
                    ui.active_theme()
                        .ui
                        .accent
                        .gamma_multiply((1.0 - pair[0].1 / 0.5) * 0.65),
                ),
            );
        }
        for (pos, age) in &self.trail {
            painter.circle_filled(
                *pos,
                5.0,
                ui.active_theme()
                    .ui
                    .accent
                    .gamma_multiply((1.0 - age / 0.5) * 0.65),
            );
        }
        if !self.trail.is_empty() {
            ui.ctx().request_repaint();
        }
    }

    /// Extend the trail of `style` (`None` when no trail cursor is in force) for this frame's
    /// pointer. Demonstration follows the pointer; Magic follows it only while the primary button
    /// is held and lets the trail fade after. Changing the style drops the old trail.
    pub(crate) fn follow(
        &mut self,
        style: Option<CrosshairStyle>,
        pointer: Option<egui::Pos2>,
        pressed: bool,
    ) {
        if self.trail_style != style {
            self.trail_style = style;
            self.move_pointer(None);
        }
        match style {
            Some(CrosshairStyle::Magic) if !pressed => self.last_pointer = None,
            Some(_) => self.move_pointer(pointer),
            None => {}
        }
    }

    pub(crate) fn move_pointer(&mut self, pos: Option<egui::Pos2>) {
        if let Some(pos) = pos {
            if self
                .last_pointer
                .is_none_or(|last| last.distance(pos) > 1.0)
            {
                if self.trail.len() == 64 {
                    self.trail.remove(0);
                }
                self.trail.push((pos, 0.0));
                self.last_pointer = Some(pos);
            }
        } else {
            self.trail.clear();
            self.last_pointer = None;
        }
    }

    /// Reset all cursor mode states.
    pub fn clear_all(&mut self) {
        self.eraser.clear();
        self.trail.clear();
        self.last_pointer = None;
        self.trail_style = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stationary pointer must not restart a trail after its last point expires.
    #[test]
    fn demonstration_trail_expires_without_restarting() {
        let mut modes = CursorModeState::new();
        for (dt, x, expected) in [
            (0.0, 10.0, 1),
            (0.1, 20.0, 2),
            (0.4, 20.0, 1),
            (0.2, 20.0, 0),
            (0.1, 20.0, 0),
            (0.1, 30.0, 1),
        ] {
            modes.update(dt);
            modes.move_pointer(Some(egui::pos2(x, 10.0)));
            assert_eq!(modes.trail.len(), expected, "at {x} after {dt}");
        }
        modes.clear_all();
        assert!(modes.trail.is_empty());
    }

    /// Magic marks only while the button is held, Demonstration marks always, and changing the
    /// style or leaving the trail cursors drops the trail the other left.
    #[test]
    fn trails_follow_their_cursor_and_clear_on_switching() {
        use CrosshairStyle::{Demonstration, Magic};
        let mut modes = CursorModeState::new();
        for (style, x, pressed, expected) in [
            (Some(Magic), 10.0, false, 0),
            (Some(Magic), 20.0, true, 1),
            (Some(Magic), 30.0, true, 2),
            (Some(Magic), 40.0, false, 2),
            (Some(Demonstration), 50.0, false, 1),
            (Some(Magic), 60.0, false, 0),
            (Some(Magic), 70.0, true, 1),
            (None, 80.0, true, 0),
        ] {
            modes.follow(style, Some(egui::pos2(x, 10.0)), pressed);
            assert_eq!(modes.trail.len(), expected, "{style:?} at {x}");
        }
    }
}
