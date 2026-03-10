//! Desktop Toolbar Components
//!
//! Simplified toolbar abstractions for desktop-only applications.
//!
//! # Key Components
//! - [`ResponsiveToolbar`]: Direct toolbar layout (no mobile scroll)
//! - [`ButtonGroup`]: Groups toolbar buttons with optional separators
//! - [`ToolbarLayout`]: Left/right split layout helper

mod button_group;

pub use button_group::{ButtonGroup, ToolbarLayout};

use std::hash::Hash;

use egui::{Id, Ui};

use crate::styles::responsive::LayoutContext;

/// Toolbar orientation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    /// Horizontal toolbar (most common)
    #[default]
    Horizontal,
    /// Vertical toolbar (e.g., drawing toolbar)
    Vertical,
}

/// Desktop toolbar component (simplified from ResponsiveToolbar).
///
/// Desktop-only: No scrolling or mobile adaptations.
///
/// # Example
/// ```ignore
/// ResponsiveToolbar::horizontal("my_toolbar")
///     .height(48.0)
///     .show(ui, |ui, ctx| {
///         // Render toolbar contents
///     });
/// ```
pub struct ResponsiveToolbar {
    /// Unique toolbar ID — reserved for egui state persistence (scroll pos, collapse)
    #[allow(dead_code)]
    id: Id,
    orientation: Orientation,
    height: Option<f32>,
    /// Force scrollable — reserved for mobile/tablet overflow support
    #[allow(dead_code)]
    force_scrollable: bool,
}

impl ResponsiveToolbar {
    /// Create a new horizontal toolbar with the given ID
    pub fn horizontal(id: impl Hash) -> Self {
        Self {
            id: Id::new(id),
            orientation: Orientation::Horizontal,
            height: None,
            force_scrollable: false,
        }
    }

    /// Create a new vertical toolbar with the given ID
    pub fn vertical(id: impl Hash) -> Self {
        Self {
            id: Id::new(id),
            orientation: Orientation::Vertical,
            height: None,
            force_scrollable: false,
        }
    }

    /// Set the fixed height for the toolbar
    #[must_use]
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }

    /// Legacy method - kept for API compatibility, but ignored on desktop
    #[must_use]
    pub fn force_scrollable(self, _force: bool) -> Self {
        // Desktop-only: scrolling not needed
        self
    }

    /// Show the toolbar (desktop-only: direct layout, no scroll)
    ///
    /// The callback receives:
    /// - `ui`: The UI context to render into
    /// - `ctx`: The layout context
    ///
    /// Returns the value returned by the callback.
    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui, &LayoutContext) -> R) -> R {
        let layout_ctx = LayoutContext::from_egui(ui.ctx());

        match self.orientation {
            Orientation::Horizontal => self.show_horizontal(ui, &layout_ctx, content),
            Orientation::Vertical => self.show_vertical(ui, &layout_ctx, content),
        }
    }

    fn show_horizontal<R>(
        self,
        ui: &mut Ui,
        layout_ctx: &LayoutContext,
        content: impl FnOnce(&mut Ui, &LayoutContext) -> R,
    ) -> R {
        // Get the FULL panel rect
        let full_rect = ui.max_rect();
        let bg_color = ui.style().visuals.panel_fill;

        // Paint background over the full area
        ui.painter().rect_filled(full_rect, 0.0, bg_color);

        // Allocate the entire space to prevent gaps
        let _ = ui.allocate_rect(full_rect, egui::Sense::hover());

        // Create a child UI with centered layout
        let mut child_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(full_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );

        // Desktop: render content directly (no scroll)
        content(&mut child_ui, layout_ctx)
    }

    fn show_vertical<R>(
        self,
        ui: &mut Ui,
        layout_ctx: &LayoutContext,
        content: impl FnOnce(&mut Ui, &LayoutContext) -> R,
    ) -> R {
        // Desktop: normal vertical layout (no scroll)
        ui.vertical(|ui| content(ui, layout_ctx)).inner
    }
}
