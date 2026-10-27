//! Floating Selection Toolbar Implementation
//!
//! Floating toolbar for object customization.

use crate::icons::icons;
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{ButtonSize, IconButton};
use crate::ui_kit::ext::UiExt;
use egui::{Color32, Pos2, Rect, Sense, Ui, Vec2};

/// Action that can be triggered from the floating toolbar
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FloatingToolbarAction {
    /// Change a color slot (index, new color)
    ChangeColor(usize, Color32),
    /// Open settings dialog
    OpenSettings,
    /// Add an alert on the selected object
    AddAlert,
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
    /// Text displayed by the component.
    pub label: &'static str,
    /// Color.
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
    /// Whether to offer an alert on the object
    alert_available: bool,
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
            alert_available: false,
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

    /// Show the Add alert button, for an object an alert can follow, such as a drawing with
    /// [`alert_levels`](crate::drawings::Drawing::alert_levels). Hidden by default.
    pub fn set_alert_available(&mut self, available: bool) {
        self.alert_available = available;
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
                let toolbar_rect = Rect::from_min_size(self.position, self.toolbar_size(ui));
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

            if ui.ctx().input(|i| i.pointer.any_down())
                && let Some(offset) = self.drag_offset
            {
                self.position = pointer_pos - offset;
            }

            if ui.ctx().input(|i| i.pointer.any_released()) {
                self.drag_offset = None;
            }
        }

        // Clamp position to viewport bounds. `min` then `max` (not `f32::clamp`, which panics
        // when min > max) places the toolbar at the viewport's min edge on an axis it does not fit.
        let screen_rect = ui.ctx().input(|i| i.viewport_rect());
        let toolbar_size = self.toolbar_size(ui);
        self.position.x = self
            .position
            .x
            .min(screen_rect.max.x - toolbar_size.x)
            .max(screen_rect.min.x);
        self.position.y = self
            .position
            .y
            .min(screen_rect.max.y - toolbar_size.y)
            .max(screen_rect.min.y);

        // Clone data we need inside closures
        let color_slots_len = self.color_slots.len();
        let is_locked = self.is_locked;
        let is_visible = self.is_visible;
        let alert_available = self.alert_available;

        // We need to track color changes
        let mut color_changes: Vec<(usize, Color32)> = Vec::new();

        // Take ownership of colors temporarily for mutation
        let mut temp_colors: Vec<Color32> = self.color_slots.iter().map(|s| s.color).collect();

        let area_response = egui::Area::new(egui::Id::unique("floating_selection_toolbar"))
            .fixed_pos(self.position)
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .corner_radius(ui.active_theme().radius.md)
                    .inner_margin(egui::Margin::symmetric(
                        ui.active_theme().spacing.md as i8,
                        ui.active_theme().spacing.sm as i8,
                    ))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = ui.active_theme().spacing.sm;

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
                                        .on_hover_text(l10n::text(
                                            ui.ctx(),
                                            "Click to change color",
                                        ))
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
                            if IconButton::new(&icons::SETTINGS)
                                .size(ButtonSize::SM)
                                .tooltip("Settings")
                                .show(ui)
                                .clicked()
                            {
                                inner_action = Some(FloatingToolbarAction::OpenSettings);
                            }

                            if alert_available
                                && IconButton::new(&icons::ALERTS)
                                    .size(ButtonSize::SM)
                                    .tooltip("Add alert")
                                    .show(ui)
                                    .clicked()
                            {
                                inner_action = Some(FloatingToolbarAction::AddAlert);
                            }

                            // Duplicate button
                            if IconButton::new(&icons::DUPLICATE)
                                .size(ButtonSize::SM)
                                .tooltip("Duplicate")
                                .show(ui)
                                .clicked()
                            {
                                inner_action = Some(FloatingToolbarAction::Duplicate);
                            }

                            Self::render_separator(ui);

                            // Lock button
                            let lock_icon = if is_locked {
                                &icons::LOCK
                            } else {
                                &icons::UNLOCK
                            };
                            let lock_tooltip = if is_locked { "Unlock" } else { "Lock" };
                            if IconButton::new(lock_icon)
                                .size(ButtonSize::SM)
                                .tooltip(lock_tooltip)
                                .show(ui)
                                .clicked()
                            {
                                inner_action = Some(FloatingToolbarAction::ToggleLock);
                            }

                            // Visibility button
                            let vis_icon = if is_visible {
                                &icons::EYE
                            } else {
                                &icons::EYE_HIDE
                            };
                            let vis_tooltip = if is_visible { "Hide" } else { "Show" };
                            if IconButton::new(vis_icon)
                                .size(ButtonSize::SM)
                                .tooltip(vis_tooltip)
                                .show(ui)
                                .clicked()
                            {
                                inner_action = Some(FloatingToolbarAction::ToggleVisibility);
                            }

                            Self::render_separator(ui);

                            // Delete button
                            if IconButton::new(&icons::TRASH)
                                .size(ButtonSize::SM)
                                .tooltip("Delete")
                                .show(ui)
                                .clicked()
                            {
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

    fn toolbar_size(&self, ui: &Ui) -> Vec2 {
        // Approximate size based on content
        let color_count = self.color_slots.len();
        let button_size = DESIGN_TOKENS.sizing.floating_toolbar.button_size;
        let color_size = DESIGN_TOKENS.sizing.floating_toolbar.color_size;
        let sep_count = 4;
        let sep_width = DESIGN_TOKENS.sizing.floating_toolbar.separator_width;
        let drag_handle = DESIGN_TOKENS.sizing.floating_toolbar.drag_handle_height;
        let padding = ui.active_theme().spacing.xl;
        // Settings, duplicate, lock, visibility, delete and the optional alert.
        let icon_buttons = 5 + usize::from(self.alert_available);

        Vec2::new(
            drag_handle
                + (color_count as f32 * (color_size + ui.active_theme().spacing.sm))
                + (icon_buttons as f32 * (button_size + ui.active_theme().spacing.sm))
                + (sep_count as f32 * sep_width)
                + padding,
            DESIGN_TOKENS.sizing.floating_toolbar.height,
        )
    }

    fn render_drag_handle(ui: &mut Ui) {
        let size = Vec2::new(
            ui.active_theme().spacing.lg + ui.active_theme().spacing.md,
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
            Vec2::new(DESIGN_TOKENS.stroke.hairline, height),
            Sense::hover(),
        );
        let painter = ui.painter();
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        painter.line_segment(
            [
                Pos2::new(rect.center().x, rect.min.y),
                Pos2::new(rect.center().x, rect.max.y),
            ],
            stroke,
        );
        ui.space_xs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keyboard navigation announces each action, including the current lock and visibility state
    /// and the alert when it is available.
    #[test]
    fn action_buttons_have_accessible_names() {
        for locked in [false, true] {
            let ctx = egui::Context::default();
            let mut toolbar = FloatingSelectionToolbar::new();
            toolbar.show_at(Pos2::new(100.0, 100.0));
            toolbar.set_locked(locked);
            toolbar.set_object_visible(!locked);
            toolbar.set_alert_available(locked);
            let mut frame = |tab| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        events: if tab {
                            vec![egui::Event::Key {
                                key: egui::Key::Tab,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            }]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    },
                    |ui| {
                        toolbar.show(ui);
                    },
                )
            };
            frame(false).drop_without_applying_deltas();
            frame(false).drop_without_applying_deltas();
            let alert = locked.then_some("Add alert");
            for label in ["Settings"].into_iter().chain(alert).chain([
                "Duplicate",
                if locked { "Unlock" } else { "Lock" },
                if locked { "Show" } else { "Hide" },
                "Delete",
            ]) {
                let mut output = frame(true);
                let events = std::mem::take(&mut output.platform_output.events);
                output.drop_without_applying_deltas();
                assert!(
                    events
                        .iter()
                        .any(|event| { event.widget_info().label.as_deref() == Some(label) }),
                    "missing accessible name: {label}"
                );
            }
        }
    }

    /// Floating surfaces and group rules follow each preset's popup and separator visuals.
    #[test]
    fn surface_and_separators_follow_the_popup_theme() {
        for (_, theme) in crate::theme::presets() {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let style = ctx.global_style();
            let mut toolbar = FloatingSelectionToolbar::new();
            toolbar.show_at(Pos2::new(100.0, 100.0));
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            };
            ctx.run_ui(raw.clone(), |ui| {
                toolbar.show(ui);
            })
            .drop_without_applying_deltas();
            let mut output = ctx.run_ui(raw, |ui| {
                toolbar.show(ui);
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let shapes: Vec<_> = shapes
                .into_iter()
                .flat_map(|shape| match shape.shape {
                    egui::Shape::Vec(shapes) => shapes,
                    shape => vec![shape],
                })
                .collect();
            assert!(shapes.iter().any(|shape| matches!(shape,
                egui::Shape::Rect(rect) if rect.fill == style.visuals.window_fill()
                    && rect.stroke == style.visuals.window_stroke
            )));
            let shadow = style.visuals.popup_shadow;
            if shadow != egui::epaint::Shadow::NONE {
                assert!(shapes.iter().any(|shape| matches!(shape,
                    egui::Shape::Rect(rect) if rect.fill == shadow.color
                        && rect.blur_width == f32::from(shadow.blur)
                )));
            }
            let separators = shapes.iter().filter(|shape| matches!(shape,
                egui::Shape::LineSegment { points, stroke }
                    if points[0].x == points[1].x
                        && (points[1].y - points[0].y) == DESIGN_TOKENS.sizing.floating_toolbar.separator_height
                        && *stroke == style.visuals.widgets.noninteractive.bg_stroke
            )).count();
            assert_eq!(separators, 3);
        }
    }

    /// A viewport smaller than the toolbar (204 x 32 px without colour slots) must not panic and
    /// pins the toolbar to the viewport's min edge on each axis that does not fit.
    #[test]
    fn show_survives_viewport_smaller_than_toolbar() {
        for (w, h, expected) in [
            (100.0, 100.0, Pos2::new(0.0, 10.0)),
            (800.0, 20.0, Pos2::new(10.0, 0.0)),
            (100.0, 20.0, Pos2::ZERO),
        ] {
            let ctx = egui::Context::default();
            let mut toolbar = FloatingSelectionToolbar::new();
            toolbar.show_at(Pos2::new(10.0, 10.0));
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
                ..Default::default()
            };
            // egui asserts that a frame's texture delta is consumed before the next frame.
            ctx.run_ui(raw.clone(), |_| {}).textures_delta.clear();
            ctx.run_ui(raw, |ui| {
                toolbar.show(ui);
            })
            .textures_delta
            .clear();
            assert_eq!(toolbar.position, expected, "viewport {w}x{h}");
        }
    }
}
