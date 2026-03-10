//! Floating Selection Toolbar Implementation
//!
//! Floating toolbar for object customization.
//!
//! Note: This toolbar uses text-emoji icon buttons rather than the SVG-based
//! [`crate::ui_kit::buttons::IconButton`]. When dedicated SVG icons are added
//! for these actions (settings, duplicate, lock, visibility, delete), the
//! `render_icon_button` helper should migrate to ui_kit's `IconButton`.

use crate::ext::HasDesignTokens;
use crate::ext::UiExt;
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// Action that can be triggered from the floating toolbar
#[derive(Clone, Debug, PartialEq)]
pub enum FloatingToolbarAction {
    /// Change a color slot (index, new color)
    ChangeColor(usize, Color32),
    /// Open settings dialog
    OpenSettings,
    /// Duplicate the selected object
    Duplicate,
    /// Delete the selected object
    Delete,
    /// Toggle lock state
    ToggleLock,
    /// Toggle visibility
    ToggleVisibility,
    /// Close the toolbar
    Close,
}

/// Configuration for color slots shown in toolbar
#[derive(Clone, Debug)]
pub struct ColorSlot {
    pub label: &'static str,
    pub color: Color32,
}

/// Floating selection toolbar state
#[derive(Clone, Debug)]
pub struct FloatingSelectionToolbar {
    /// Whether toolbar is visible
    visible: bool,
    /// Current position
    position: Pos2,
    /// Drag offset for dragging
    drag_offset: Option<Vec2>,
    /// Color slots to display (mutable for color picker)
    color_slots: Vec<ColorSlot>,
    /// Whether object is locked
    is_locked: bool,
    /// Whether object is visible
    is_visible: bool,
    /// Last rendered rect (for hit testing)
    last_rect: Rect,
}

impl Default for FloatingSelectionToolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl FloatingSelectionToolbar {
    /// Create a new hidden floating selection toolbar at the default position
    pub fn new() -> Self {
        Self {
            visible: false,
            position: Pos2::new(
                DESIGN_TOKENS.sizing.floating_toolbar.default_x,
                DESIGN_TOKENS.sizing.floating_toolbar.default_y,
            ),
            drag_offset: None,
            color_slots: Vec::new(),
            is_locked: false,
            is_visible: true,
            last_rect: Rect::NOTHING,
        }
    }

    /// Show the toolbar at a specific position
    pub fn show_at(&mut self, position: Pos2) {
        self.visible = true;
        self.position = position;
    }

    /// Hide the toolbar
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if toolbar is visible
    #[allow(clippy::misnamed_getters)]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Check if a point is inside the toolbar area (for hit testing)
    pub fn contains_point(&self, point: Pos2) -> bool {
        self.visible && self.last_rect.contains(point)
    }

    /// Set the color slots to display
    pub fn set_colors(&mut self, slots: Vec<ColorSlot>) {
        self.color_slots = slots;
    }

    /// Get mutable reference to color slots (for updating from outside)
    pub fn color_slots_mut(&mut self) -> &mut Vec<ColorSlot> {
        &mut self.color_slots
    }

    /// Set locked state
    pub fn set_locked(&mut self, locked: bool) {
        self.is_locked = locked;
    }

    /// Set visibility state
    pub fn set_object_visible(&mut self, visible: bool) {
        self.is_visible = visible;
    }

    /// Render the toolbar
    pub fn show(&mut self, ui: &mut Ui) -> Option<FloatingToolbarAction> {
        if !self.visible {
            return None;
        }

        // Handle dragging
        if let Some(pointer_pos) = ui.ctx().input(|i| i.pointer.interact_pos()) {
            if ui.ctx().input(|i| i.pointer.any_pressed()) && self.drag_offset.is_none() {
                // Check if clicking on drag handle area (first 24px)
                let toolbar_rect = Rect::from_min_size(self.position, self.toolbar_size());
                let drag_handle_rect = Rect::from_min_size(
                    toolbar_rect.min,
                    Vec2::new(
                        DESIGN_TOKENS.sizing.floating_toolbar.drag_handle_width,
                        toolbar_rect.height(),
                    ),
                );
                if drag_handle_rect.contains(pointer_pos) {
                    self.drag_offset = Some(pointer_pos - self.position);
                }
            }

            if ui.ctx().input(|i| i.pointer.any_down()) {
                if let Some(offset) = self.drag_offset {
                    self.position = pointer_pos - offset;
                }
            }

            if ui.ctx().input(|i| i.pointer.any_released()) {
                self.drag_offset = None;
            }
        }

        // Clamp position to viewport bounds
        let screen_rect = ui.ctx().input(|i| i.viewport_rect());
        let toolbar_size = self.toolbar_size();
        self.position.x = self
            .position
            .x
            .clamp(screen_rect.min.x, screen_rect.max.x - toolbar_size.x);
        self.position.y = self
            .position
            .y
            .clamp(screen_rect.min.y, screen_rect.max.y - toolbar_size.y);

        // Clone data we need inside closures
        let color_slots_len = self.color_slots.len();
        let is_locked = self.is_locked;
        let is_visible = self.is_visible;

        // We need to track color changes
        let mut color_changes: Vec<(usize, Color32)> = Vec::new();

        // Take ownership of colors temporarily for mutation
        let mut temp_colors: Vec<Color32> = self.color_slots.iter().map(|s| s.color).collect();

        let area_response = egui::Area::new(egui::Id::new("floating_selection_toolbar"))
            .fixed_pos(self.position)
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ui.ctx(), |ui| {
                let toolbar_bg = ui.panel_fill();
                let border_color = ui.style().visuals.widgets.noninteractive.bg_stroke.color;

                egui::Frame::new()
                    .fill(toolbar_bg)
                    .stroke(Stroke::new(DESIGN_TOKENS.stroke.hairline, border_color))
                    .corner_radius(DESIGN_TOKENS.rounding.md)
                    .inner_margin(egui::Margin::symmetric(
                        DESIGN_TOKENS.spacing.md as i8,
                        DESIGN_TOKENS.spacing.sm as i8,
                    ))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 2],
                        blur: 8,
                        spread: 0,
                        color: Color32::from_black_alpha(40),
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = DESIGN_TOKENS.spacing.sm;

                            let mut inner_action: Option<FloatingToolbarAction> = None;

                            // Drag handle (6-dot pattern)
                            Self::render_drag_handle(ui);

                            ui.space_xs();

                            // Vertical separator
                            Self::render_separator(ui);

                            // Color pickers using egui's native color_edit
                            for idx in 0..color_slots_len {
                                if idx < temp_colors.len() {
                                    let mut color = temp_colors[idx];
                                    let original_color = color;

                                    // Use egui's color picker button
                                    if ui
                                        .color_edit_button_srgba(&mut color)
                                        .on_hover_text("Click to change color")
                                        .changed()
                                    {
                                        color_changes.push((idx, color));
                                    }

                                    if color != original_color {
                                        temp_colors[idx] = color;
                                    }
                                }
                            }

                            if !temp_colors.is_empty() {
                                Self::render_separator(ui);
                            }

                            // Settings button (gear icon)
                            if Self::render_icon_button(ui, "⚙", "Settings").clicked() {
                                inner_action = Some(FloatingToolbarAction::OpenSettings);
                            }

                            // Duplicate button
                            if Self::render_icon_button(ui, "⧉", "Duplicate").clicked() {
                                inner_action = Some(FloatingToolbarAction::Duplicate);
                            }

                            Self::render_separator(ui);

                            // Lock button
                            let lock_icon = if is_locked { "🔒" } else { "🔓" };
                            let lock_tooltip = if is_locked { "Unlock" } else { "Lock" };
                            if Self::render_icon_button(ui, lock_icon, lock_tooltip).clicked() {
                                inner_action = Some(FloatingToolbarAction::ToggleLock);
                            }

                            // Visibility button
                            let vis_icon = if is_visible { "👁" } else { "◌" };
                            let vis_tooltip = if is_visible { "Hide" } else { "Show" };
                            if Self::render_icon_button(ui, vis_icon, vis_tooltip).clicked() {
                                inner_action = Some(FloatingToolbarAction::ToggleVisibility);
                            }

                            Self::render_separator(ui);

                            // Delete button (red on hover)
                            if Self::render_delete_button(ui).clicked() {
                                inner_action = Some(FloatingToolbarAction::Delete);
                            }

                            inner_action
                        })
                        .inner
                    })
                    .inner
            });

        // Store the toolbar rect for hit testing
        self.last_rect = area_response.response.rect;

        // Apply color changes back to slots
        for (idx, color) in &color_changes {
            if *idx < self.color_slots.len() {
                self.color_slots[*idx].color = *color;
            }
        }

        // Return the first color change action if any
        if let Some((idx, color)) = color_changes.first() {
            return Some(FloatingToolbarAction::ChangeColor(*idx, *color));
        }

        area_response.inner
    }

    fn toolbar_size(&self) -> Vec2 {
        // Approximate size based on content
        let color_count = self.color_slots.len();
        let button_size = DESIGN_TOKENS.sizing.floating_toolbar.button_size;
        let color_size = DESIGN_TOKENS.sizing.floating_toolbar.color_size;
        let sep_count = 4;
        let sep_width = DESIGN_TOKENS.sizing.floating_toolbar.separator_width;
        let drag_handle = DESIGN_TOKENS.sizing.floating_toolbar.drag_handle_height;
        let padding = DESIGN_TOKENS.spacing.xl;
        let icon_buttons = 5; // settings, duplicate, lock, visibility, delete

        Vec2::new(
            drag_handle
                + (color_count as f32 * (color_size + DESIGN_TOKENS.spacing.sm))
                + (icon_buttons as f32 * (button_size + DESIGN_TOKENS.spacing.sm))
                + (sep_count as f32 * sep_width)
                + padding,
            DESIGN_TOKENS.sizing.floating_toolbar.height,
        )
    }

    fn render_drag_handle(ui: &mut Ui) {
        let size = Vec2::new(
            DESIGN_TOKENS.spacing.lg + DESIGN_TOKENS.spacing.md,
            DESIGN_TOKENS.sizing.floating_toolbar.drag_handle_height,
        );
        let (rect, _response) = ui.allocate_exact_size(size, Sense::hover());

        let painter = ui.painter();
        let dot_color = ui.style().visuals.text_color().gamma_multiply(0.4);
        let dot_size = DESIGN_TOKENS.sizing.floating_toolbar.dot_size;
        let dot_spacing = DESIGN_TOKENS.sizing.floating_toolbar.dot_spacing;

        // Draw 6-dot drag handle pattern (2 columns, 3 rows)
        let start_x = rect.center().x - dot_spacing / 2.0;
        let start_y = rect.center().y - dot_spacing;

        for col in 0..2 {
            for row in 0..3 {
                let x = start_x + col as f32 * dot_spacing;
                let y = start_y + row as f32 * dot_spacing;
                painter.circle_filled(Pos2::new(x, y), dot_size, dot_color);
            }
        }
    }

    fn render_separator(ui: &mut Ui) {
        let height = DESIGN_TOKENS.sizing.floating_toolbar.separator_height;
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(DESIGN_TOKENS.spacing.hairline, height),
            Sense::hover(),
        );
        let painter = ui.painter();
        let color = ui.style().visuals.widgets.noninteractive.bg_stroke.color;
        painter.line_segment(
            [
                Pos2::new(rect.center().x, rect.min.y),
                Pos2::new(rect.center().x, rect.max.y),
            ],
            Stroke::new(DESIGN_TOKENS.stroke.hairline, color.gamma_multiply(0.5)),
        );
        ui.space_xs();
    }

    fn render_icon_button(ui: &mut Ui, icon: &str, tooltip: &str) -> Response {
        let size = Vec2::splat(DESIGN_TOKENS.sizing.icon_btn);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());

        let painter = ui.painter();
        let is_hovered = response.hovered();

        // Background with hover effect
        if is_hovered {
            painter.rect_filled(
                rect,
                DESIGN_TOKENS.rounding.sm,
                ui.style().visuals.widgets.hovered.bg_fill,
            );
        }

        // Icon text
        let text_color = if is_hovered {
            ui.style().visuals.strong_text_color()
        } else {
            ui.style().visuals.text_color().gamma_multiply(0.8)
        };

        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(typography::MD),
            text_color,
        );

        response.on_hover_text(tooltip)
    }

    fn render_delete_button(ui: &mut Ui) -> Response {
        let size = Vec2::splat(DESIGN_TOKENS.sizing.icon_btn);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());

        let painter = ui.painter();
        let is_hovered = response.hovered();

        // Red background on hover
        if is_hovered {
            painter.rect_filled(
                rect,
                DESIGN_TOKENS.rounding.sm,
                DESIGN_TOKENS.semantic.status.error.gamma_multiply(0.3),
            );
        }

        // Icon - red when hovered
        let text_color = if is_hovered {
            DESIGN_TOKENS.semantic.status.error
        } else {
            ui.style().visuals.text_color().gamma_multiply(0.8)
        };

        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "🗑",
            egui::FontId::proportional(typography::MD),
            text_color,
        );

        response.on_hover_text("Delete")
    }
}
