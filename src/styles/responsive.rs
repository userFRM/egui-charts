//! Simplified layout context for desktop-only UI.
//!
//! Provides a minimal [`LayoutContext`] that captures screen dimensions and
//! DPI scaling. In this desktop-only build, touch targets default to pointer
//! (24px) mode.

use egui::Vec2;

/// Simplified layout context for desktop-only applications
#[derive(Debug, Clone)]
pub struct LayoutContext {
    /// Screen size in logical pixels
    pub screen_size: Vec2,
    /// Display scale factor (pixels per point)
    pub scale_factor: f32,
}

impl LayoutContext {
    /// Create LayoutContext from egui Context
    pub fn from_egui(ctx: &egui::Context) -> Self {
        let screen = ctx.input(|i| i.viewport().outer_rect.unwrap_or(egui::Rect::EVERYTHING));
        Self {
            screen_size: screen.size(),
            scale_factor: ctx.pixels_per_point(),
        }
    }

    /// Physical width in pixels
    pub fn physical_width(&self) -> f32 {
        self.screen_size.x
    }

    /// Physical height in pixels
    pub fn physical_height(&self) -> f32 {
        self.screen_size.y
    }

    /// Minimum touch target size (always pointer/desktop value)
    pub fn min_touch_target(&self) -> f32 {
        24.0
    }
}
