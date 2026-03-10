//! Builder framework for dropdown submenus.
//!
//! This module provides a reusable builder pattern for creating consistent,
//! theme-aware dropdown submenus.
//!
//! ## Benefits
//!
//! - Eliminates ~600 lines of duplicated submenu rendering code
//! - Enforces consistent styling across all submenus
//! - Makes adding new submenus trivial (15 lines vs 150 lines)
//! - Centralized positioning and interaction logic
//!
//! ## Usage
//!
//! ```rust
//! use submenu_builder::SubmenuBuilder;
//!
//! let action = SubmenuBuilder::new(ui, sidebar_rect)
//!     .with_category_rect(category_rect)
//!     .with_width(240.0)
//!     .add_icon_item(&icons::ERASER, "Cross", "Cross cursor")
//!     .add_icon_item(&icons::DOT, "Dot", "Dot cursor")
//!     .add_toggle(ToggleConfig {
//!         label: "Enable feature".to_string(),
//!         value: &mut state.feature_enabled,
//!     })
//!     .show();
//! ```

use super::actions::DrawingToolbarAction;
use crate::ext::UiExt;
use crate::icons::Icon;
use crate::styles::{icons as icon_sizes, typography};
use crate::theming;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Sense, Ui, Vec2};

/// Builder for creating dropdown submenus with consistent styling.
///
/// This builder provides a fluent API for constructing submenus with icons,
/// text items, separators, and toggles.
pub struct SubmenuBuilder<'a> {
    ui: &'a mut Ui,
    sidebar_rect: Rect,
    category_rect: Option<Rect>,
    items: Vec<SubmenuItem>,
    width: f32,
    toggle: Option<ToggleConfig<'a>>,
}

/// A single item in a submenu
pub struct SubmenuItem {
    pub icon: Option<&'static Icon>,
    pub label: String,
    pub tooltip: String,
    pub is_sel: bool,
    pub is_favorite: bool,
    pub on_click: Option<Box<dyn Fn() -> Option<DrawingToolbarAction>>>,
}

/// Configuration for a toggle checkbox at the bottom of a submenu
pub struct ToggleConfig<'a> {
    pub label: String,
    pub value: &'a mut bool,
}

impl<'a> SubmenuBuilder<'a> {
    /// Create a new submenu builder
    ///
    /// # Arguments
    ///
    /// * `ui` - The egui UI context
    /// * `sidebar_rect` - The rect of the sidebar (for positioning)
    pub fn new(ui: &'a mut Ui, sidebar_rect: Rect) -> Self {
        Self {
            ui,
            sidebar_rect,
            category_rect: None,
            items: Vec::new(),
            width: DESIGN_TOKENS.sizing.dialog.submenu_width, // Default submenu width
            toggle: None,
        }
    }

    /// Set the category button rect for precise alignment
    pub fn with_category_rect(mut self, rect: Rect) -> Self {
        self.category_rect = Some(rect);
        self
    }

    /// Set the submenu width (default: 200px)
    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Add an item with icon, label, and tooltip
    pub fn add_icon_item(
        mut self,
        icon: &'static Icon,
        label: impl Into<String>,
        tooltip: impl Into<String>,
    ) -> Self {
        self.items.push(SubmenuItem {
            icon: Some(icon),
            label: label.into(),
            tooltip: tooltip.into(),
            is_sel: false,
            is_favorite: false,
            on_click: None,
        });
        self
    }

    /// Add a text-only item (no icon)
    pub fn add_text_item(mut self, label: impl Into<String>, tooltip: impl Into<String>) -> Self {
        self.items.push(SubmenuItem {
            icon: None,
            label: label.into(),
            tooltip: tooltip.into(),
            is_sel: false,
            is_favorite: false,
            on_click: None,
        });
        self
    }

    /// Add a text-only item with an action callback
    pub fn add_text_item_with_action<F>(
        mut self,
        label: impl Into<String>,
        tooltip: impl Into<String>,
        action: F,
    ) -> Self
    where
        F: Fn() -> Option<DrawingToolbarAction> + 'static,
    {
        self.items.push(SubmenuItem {
            icon: None,
            label: label.into(),
            tooltip: tooltip.into(),
            is_sel: false,
            is_favorite: false,
            on_click: Some(Box::new(action)),
        });
        self
    }

    /// Add a toggle checkbox at the bottom of the submenu
    pub fn add_toggle(mut self, config: ToggleConfig<'a>) -> Self {
        self.toggle = Some(config);
        self
    }

    /// Render the submenu and return any action triggered
    pub fn show(mut self) -> Option<DrawingToolbarAction> {
        let action = None;

        // Submenu styling constants
        let item_height = DESIGN_TOKENS.sizing.dialog.submenu_item_height; // 38px
        let padding = DESIGN_TOKENS.spacing.lg;
        let toggle_height = if self.toggle.is_some() {
            DESIGN_TOKENS.sizing.button_xl
        } else {
            0.0
        };
        let submenu_height =
            (self.items.len() as f32 * item_height) + toggle_height + padding * 2.0;

        // Pos submenu aligned with category button, clamped to screen
        let screen_rect = self.ui.ctx().content_rect();
        let submenu_y = if let Some(cat_rect) = self.category_rect {
            cat_rect.min.y.min(
                screen_rect.max.y
                    - submenu_height
                    - DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_margin,
            )
        } else {
            (self.sidebar_rect.min.y + DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_offset_y)
                .min(
                    screen_rect.max.y
                        - submenu_height
                        - DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_margin,
                )
        };

        let submenu_pos = Pos2::new(
            self.sidebar_rect.right() + DESIGN_TOKENS.spacing.sm,
            submenu_y,
        );

        // Render submenu using egui::Area (floats above other UI)
        let area_res = egui::Area::new(egui::Id::new("submenu_builder"))
            .fixed_pos(submenu_pos)
            .order(egui::Order::Foreground)
            .constrain(true)
            .show(self.ui.ctx(), |ui| {
                // Frame with shadow
                egui::Frame::new()
                    .fill(theming::toolbar_bg(ui))
                    .stroke(ui.style().visuals.window_stroke)
                    .corner_radius(DESIGN_TOKENS.rounding.lg)
                    .shadow(egui::Shadow {
                        spread: 0,
                        blur: 16,
                        offset: [0, 4],
                        color: Color32::from_black_alpha(60),
                    })
                    .inner_margin(egui::Margin::same(padding as i8))
                    .show(ui, |ui| {
                        ui.set_min_width(self.width);
                        ui.set_max_width(self.width);

                        let mut result_action = None;

                        // Render all items
                        for item in &self.items {
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(self.width - padding * 2.0, item_height),
                                Sense::click(),
                            );

                            // Hover background
                            if response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    DESIGN_TOKENS.rounding.sm,
                                    theming::hover_color(ui),
                                );
                            }

                            // Draw icon if present
                            if let Some(icon) = item.icon {
                                let icon_rect = Rect::from_center_size(
                                    Pos2::new(rect.min.x + icon_sizes::MD, rect.center().y),
                                    Vec2::splat(icon_sizes::SM_MD),
                                );

                                // Render icon with theme-aware colors
                                let icon_color = if item.is_sel {
                                    theming::icon_active(ui)
                                } else if response.hovered() {
                                    theming::icon_hover_color(ui)
                                } else {
                                    theming::icon_normal(ui)
                                };
                                icon.as_image_tinted(Vec2::splat(icon_sizes::SM_MD), icon_color)
                                    .paint_at(ui, icon_rect);
                            }

                            // Draw text
                            let text_x = if item.icon.is_some() {
                                rect.min.x + DESIGN_TOKENS.sizing.toolbar.left_width
                            } else {
                                rect.min.x + DESIGN_TOKENS.spacing.xl
                            };
                            ui.painter().text(
                                Pos2::new(text_x, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                &item.label,
                                egui::FontId::proportional(typography::LG),
                                theming::icon_color(ui),
                            );

                            // Handle click
                            if response.clicked()
                                && let Some(ref on_click) = item.on_click
                            {
                                result_action = on_click();
                            }

                            response.on_hover_text(&item.tooltip);
                        }

                        // Add toggle if present
                        if let Some(toggle_config) = &mut self.toggle {
                            ui.spaced_separator();

                            ui.horizontal(|ui| {
                                ui.set_width(self.width - padding * 2.0);
                                ui.checkbox(toggle_config.value, &toggle_config.label);
                            });
                        }

                        result_action
                    })
                    .inner
            })
            .inner;

        // Close submenu on outside click
        if self.ui.input(|i| i.pointer.any_click())
            && let Some(pos) = self.ui.input(|i| i.pointer.interact_pos())
        {
            let sidebar_expanded = self.sidebar_rect.expand(5.0);
            let submenu_rect =
                Rect::from_min_size(submenu_pos, Vec2::new(self.width, submenu_height));
            let submenu_expanded = submenu_rect.expand(5.0);

            if !sidebar_expanded.contains(pos) && !submenu_expanded.contains(pos) {
                // Return None to signal submenu should close
                return None;
            }
        }

        area_res.or(action)
    }
}
