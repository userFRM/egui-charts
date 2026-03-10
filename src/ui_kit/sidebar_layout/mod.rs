//! Sidebar + content two-column layout using `egui_extras::StripBuilder`.
//!
//! Provides a proportional two-column layout with a fixed-width sidebar,
//! a 1px vertical separator, and a remainder-width content area.
//!
//! # Example
//! ```ignore
//! SidebarLayout::new(160.0)
//!     .show(ui,
//!         |ui| { /* sidebar navigation */ },
//!         |ui| { /* scrollable content */ },
//!     );
//! ```

use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect, Sense, Ui, Vec2};
use egui_extras::{Size, StripBuilder};

/// Two-column layout: fixed-width sidebar + flexible content area.
///
/// Uses `egui_extras::StripBuilder` for clean proportional column sizing.
/// Renders a vertical separator between the columns automatically.
pub struct SidebarLayout {
    sidebar_width: f32,
}

impl SidebarLayout {
    pub fn new(sidebar_width: f32) -> Self {
        Self { sidebar_width }
    }

    /// Show the two-column layout.
    ///
    /// The `sidebar` closure renders the left navigation.
    /// The `content` closure renders the right content area.
    pub fn show(self, ui: &mut Ui, sidebar: impl FnOnce(&mut Ui), content: impl FnOnce(&mut Ui)) {
        StripBuilder::new(ui)
            .size(Size::exact(self.sidebar_width))
            .size(Size::exact(1.0)) // separator
            .size(Size::remainder())
            .horizontal(|mut strip| {
                // Sidebar column
                strip.cell(|ui| {
                    sidebar(ui);
                });

                // Vertical separator
                strip.cell(|ui| {
                    let rect = ui.available_rect_before_wrap();
                    ui.painter().rect_filled(
                        rect,
                        0.0,
                        ui.style().visuals.widgets.noninteractive.bg_stroke.color,
                    );
                });

                // Content column
                strip.cell(|ui| {
                    content(ui);
                });
            });
    }
}

/// A sidebar tab item with icon, label, and selection state.
///
/// Used inside `SidebarLayout`'s sidebar column. Renders a full-width
/// row with optional icon and label, with painted background for
/// active/hover states.
///
/// # Example
/// ```ignore
/// for tab in Tab::all() {
///     if SidebarTab::new(tab.label(), tab == active)
///         .icon(tab.icon())
///         .show(ui)
///     {
///         active = tab;
///     }
/// }
/// ```
pub struct SidebarTab<'a> {
    label: &'a str,
    icon: Option<&'a crate::icons::Icon>,
    is_active: bool,
}

impl<'a> SidebarTab<'a> {
    pub fn new(label: &'a str, is_active: bool) -> Self {
        Self {
            label,
            icon: None,
            is_active,
        }
    }

    pub fn icon(mut self, icon: &'a crate::icons::Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Show the sidebar tab. Returns `true` if clicked.
    pub fn show(self, ui: &mut Ui) -> bool {
        let width = ui.available_width();
        let height = DESIGN_TOKENS.sizing.button_lg;
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

        let is_hovered = response.hovered();

        // Background
        if self.is_active {
            ui.painter().rect_filled(
                rect,
                0.0,
                ui.style().visuals.selection.bg_fill.gamma_multiply(0.3),
            );
        } else if is_hovered {
            ui.painter()
                .rect_filled(rect, 0.0, ui.style().visuals.widgets.hovered.bg_fill);
        }

        let text_color = if self.is_active {
            ui.style().visuals.selection.bg_fill
        } else {
            ui.style().visuals.text_color()
        };

        // Icon
        let mut text_x = rect.min.x + DESIGN_TOKENS.spacing.xl;
        if let Some(icon) = self.icon {
            let icon_size = DESIGN_TOKENS.sizing.icon_md;
            let icon_rect = Rect::from_min_size(
                Pos2::new(text_x, rect.center().y - icon_size / 2.0),
                Vec2::splat(icon_size),
            );
            icon.as_image_tinted(icon_rect.size(), text_color)
                .paint_at(ui, icon_rect);
            text_x = icon_rect.max.x + DESIGN_TOKENS.spacing.sm;
        }

        // Label
        ui.painter().text(
            Pos2::new(text_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            self.label,
            egui::FontId::proportional(typography::MD),
            text_color,
        );

        response.clicked()
    }
}
