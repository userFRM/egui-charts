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

    pub(crate) fn move_pointer(&mut self, pos: Option<egui::Pos2>) {
        if let Some(pos) = pos {
            if self
                .trail
                .last()
                .is_none_or(|(last, _)| last.distance(pos) > 1.0)
            {
                if self.trail.len() == 64 {
                    self.trail.remove(0);
                }
                self.trail.push((pos, 0.0));
            }
        } else {
            self.trail.clear();
        }
    }

    /// Reset all cursor mode states.
    pub fn clear_all(&mut self) {
        self.eraser.clear();
        self.trail.clear();
    }
}
