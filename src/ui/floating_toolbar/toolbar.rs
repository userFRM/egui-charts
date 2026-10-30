//! Floating Selection Toolbar Implementation
//!
//! Floating toolbar for object customization.

use crate::icons::icons;
use crate::localization as l10n;
use crate::model::LineStyle;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::context_menu::drawing::{DrawingContextMenu, DrawingContextMenuAction};
use crate::ui_kit::buttons::{ButtonSize, IconButton};
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::{ColorPicker, ColorPickerState};
use crate::widget::Chart;
use egui::{Color32, Pos2, Rect, Sense, Ui, Vec2};

/// Action that can be triggered from the floating toolbar
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FloatingToolbarAction {
    /// Change a color slot (index, new color)
    ChangeColor(usize, Color32),
    /// Change the stroke width in pixels.
    ChangeWidth(f32),
    /// Change the stroke pattern.
    ChangeStyle(LineStyle),
    /// An entry from the drawing context menu.
    DrawingMenu(DrawingContextMenuAction),
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
    /// Color slots to display (mutable for color picker)
    color_slots: Vec<ColorSlot>,
    pickers: Vec<ColorPickerState>,
    stroke: Option<(f32, LineStyle)>,
    selected: Option<(String, usize)>,
    colour_drag: bool,
    menu: DrawingContextMenu,
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
            color_slots: Vec::new(),
            pickers: Vec::new(),
            stroke: None,
            selected: None,
            colour_drag: false,
            menu: DrawingContextMenu::new(),
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
        self.selected = None;
        self.pickers.clear();
        self.menu.close();
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

    /// Follow the selected drawing, applying quick style edits as undoable chart actions.
    /// Returns menu actions for the host's dialogs, clipboard and drawing commands.
    /// Call each frame, including after deselection, to finish a continuous colour edit.
    pub fn show_and_apply(
        &mut self,
        ui: &mut Ui,
        chart: &mut Chart,
    ) -> Option<DrawingContextMenuAction> {
        if self.colour_drag && !ui.input(|i| i.pointer.any_down()) {
            chart.user_action(|chart| chart.drawings_mut().end_drag_handle());
            self.colour_drag = false;
        }
        let drawing = chart.drawings().sel_drawing().and_then(|id| {
            chart
                .drawings()
                .drawings
                .iter()
                .find(|d| d.id == id && d.visible && d.completed)
        });
        let Some(drawing) = drawing else {
            self.hide();
            return None;
        };
        let id = drawing.id;
        let selected = (chart.symbol().to_owned(), id);
        if self.selected.as_ref() != Some(&selected) {
            self.hide();
            let anchor = drawing
                .points
                .first()
                .copied()
                .unwrap_or(chart.get_rendered_price_rect().center());
            self.show_at(anchor + Vec2::new(12.0, -52.0));
            self.selected = Some(selected);
        }
        self.set_locked(drawing.locked);
        self.set_object_visible(drawing.visible);
        self.set_colors(vec![ColorSlot {
            label: if drawing.tool_type.supports_text() {
                "Text color"
            } else {
                "Line color"
            },
            color: drawing.color32(),
        }]);
        if drawing.tool_type.supports_fill() {
            self.color_slots.push(ColorSlot {
                label: "Fill color",
                color: drawing.fill_color32().unwrap_or(Color32::TRANSPARENT),
            });
        }
        self.stroke = Some((drawing.stroke_width, drawing.line_style));
        // Refresh context without reopening the menu or resetting its dismiss state.
        if !self.menu.is_open() {
            self.menu.open_on(chart, self.position, id);
            self.menu.close();
        }
        let action = self.show_with_shortcuts(ui, chart.shortcuts())?;
        match action {
            FloatingToolbarAction::ChangeColor(slot, color) => {
                let down = ui.input(|i| i.pointer.any_down());
                chart.user_action(|chart| {
                    let edit = |drawing: &mut crate::drawings::Drawing| {
                        if slot == 0 {
                            drawing.color = color.to_srgba_unmultiplied();
                        } else {
                            drawing.fill_color = Some(color.to_srgba_unmultiplied());
                        }
                    };
                    if down {
                        chart.drawings_mut().drag_edit_drawing(id, edit);
                    } else {
                        chart.drawings_mut().edit_drawing(id, edit);
                    }
                });
                self.colour_drag = down;
            }
            FloatingToolbarAction::ChangeWidth(width) => chart.user_action(|chart| {
                chart
                    .drawings_mut()
                    .edit_drawing(id, |d| d.stroke_width = width);
            }),
            FloatingToolbarAction::ChangeStyle(style) => chart.user_action(|chart| {
                chart
                    .drawings_mut()
                    .edit_drawing(id, |d| d.line_style = style);
            }),
            FloatingToolbarAction::OpenSettings => return Some(DrawingContextMenuAction::Edit),
            FloatingToolbarAction::Duplicate => return Some(DrawingContextMenuAction::Clone),
            FloatingToolbarAction::ToggleLock => return Some(DrawingContextMenuAction::Lock),
            FloatingToolbarAction::ToggleVisibility => return Some(DrawingContextMenuAction::Hide),
            FloatingToolbarAction::Delete => return Some(DrawingContextMenuAction::Delete),
            FloatingToolbarAction::AddAlert => {
                return self
                    .menu
                    .alert
                    .clone()
                    .map(|alert| DrawingContextMenuAction::AddAlert(Box::new(alert)));
            }
            FloatingToolbarAction::DrawingMenu(action) => return Some(action),
            _ => {}
        }
        ui.ctx().request_repaint();
        None
    }

    /// Render the toolbar.
    pub fn show(&mut self, ui: &mut Ui) -> Option<FloatingToolbarAction> {
        self.show_with_shortcuts(ui, &crate::widget::ShortcutMap::default())
    }

    fn show_with_shortcuts(
        &mut self,
        ui: &mut Ui,
        shortcuts: &crate::widget::ShortcutMap,
    ) -> Option<FloatingToolbarAction> {
        if !self.visible {
            return None;
        }
        let bounds = ui.ctx().content_rect();
        let size = if self.last_rect.is_finite() {
            self.last_rect.size()
        } else {
            Vec2::new(240.0, 40.0)
        };
        self.position.x = self
            .position
            .x
            .min(bounds.right() - size.x)
            .max(bounds.left());
        self.position.y = self
            .position
            .y
            .min(bounds.bottom() - size.y)
            .max(bounds.top());
        self.pickers
            .resize_with(self.color_slots.len(), ColorPickerState::default);
        let mut action = None;
        let area = egui::Area::new(egui::Id::unique("floating_selection_toolbar"))
            .fixed_pos(self.position)
            .default_size(size)
            .movable(false)
            .constrain_to(bounds)
            .order(egui::Order::Foreground)
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
                            let drag = Self::render_drag_handle(ui);
                            if drag.dragged() {
                                self.position += drag.drag_delta();
                            }
                            Self::render_separator(ui);
                            ui.add_enabled_ui(!self.is_locked, |ui| {
                                ui.horizontal(|ui| {
                                    for (idx, slot) in self.color_slots.iter_mut().enumerate() {
                                        let picker = &mut self.pickers[idx];
                                        picker.sel_color = slot.color;
                                        let custom = picker.custom_mode;
                                        let picked =
                                            ColorPicker::new(picker, ("selection_color", idx))
                                                .show(ui);
                                        picked
                                            .response
                                            .on_hover_text(l10n::text(ui.ctx(), slot.label));
                                        if let Some(mut color) = picked.inner {
                                            if slot.label == "Fill color"
                                                && !custom
                                                && !picker.custom_mode
                                                && slot.color.a() > 0
                                            {
                                                let [r, g, b, _] = color.to_srgba_unmultiplied();
                                                color = Color32::from_rgba_unmultiplied(
                                                    r,
                                                    g,
                                                    b,
                                                    slot.color.a(),
                                                );
                                            }
                                            if color != slot.color {
                                                slot.color = color;
                                                action = Some(FloatingToolbarAction::ChangeColor(
                                                    idx, color,
                                                ));
                                            }
                                        }
                                    }
                                    if let Some((width, style)) = &mut self.stroke {
                                        if ui.combo_select_width(
                                            "selection_width",
                                            width,
                                            [1.0, 2.0, 3.0, 4.0],
                                            |v| format!("{v} px"),
                                            48.0,
                                        ) {
                                            action =
                                                Some(FloatingToolbarAction::ChangeWidth(*width));
                                        }
                                        let ctx = ui.ctx().clone();
                                        if ui.combo_select_width(
                                            "selection_style",
                                            style,
                                            LineStyle::all().iter().copied(),
                                            |v| l10n::text(&ctx, v.name()),
                                            72.0,
                                        ) {
                                            action =
                                                Some(FloatingToolbarAction::ChangeStyle(*style));
                                        }
                                    }
                                });
                            });
                            Self::render_separator(ui);
                            if IconButton::new(&icons::SETTINGS)
                                .size(ButtonSize::SM)
                                .tooltip("Settings")
                                .show(ui)
                                .clicked()
                            {
                                action = Some(FloatingToolbarAction::OpenSettings);
                            }
                            if self.alert_available
                                && IconButton::new(&icons::ALERTS)
                                    .size(ButtonSize::SM)
                                    .tooltip("Add alert")
                                    .show(ui)
                                    .clicked()
                            {
                                action = Some(FloatingToolbarAction::AddAlert);
                            }
                            if IconButton::new(if self.is_locked {
                                &icons::LOCK
                            } else {
                                &icons::UNLOCK
                            })
                            .size(ButtonSize::SM)
                            .tooltip(if self.is_locked { "Unlock" } else { "Lock" })
                            .show(ui)
                            .clicked()
                            {
                                action = Some(FloatingToolbarAction::ToggleLock);
                            }
                            if ui
                                .add_enabled_ui(!self.is_locked, |ui| {
                                    IconButton::new(&icons::TRASH)
                                        .size(ButtonSize::SM)
                                        .tooltip("Delete")
                                        .show(ui)
                                })
                                .inner
                                .clicked()
                            {
                                action = Some(FloatingToolbarAction::Delete);
                            }
                            Self::render_separator(ui);
                            let more = IconButton::new(&icons::UI_MORE_HORIZONTAL)
                                .size(ButtonSize::SM)
                                .tooltip("More")
                                .show(ui);
                            if more.clicked() {
                                self.menu.set_context(
                                    self.menu.drawing_type.clone(),
                                    self.is_locked,
                                    self.is_visible,
                                );
                                self.menu.open(
                                    more.rect.left_bottom(),
                                    self.menu.drawing_id.unwrap_or_default(),
                                );
                            }
                        });
                    });
            });
        self.last_rect = area.response.rect;
        // Keep the next frame's anchor in step with egui's measured, constrained surface.
        if !ui.input(|i| i.pointer.any_down()) {
            self.position = self.last_rect.min;
        }
        self.menu
            .show_with_shortcuts(ui.ctx(), &ui.active_theme(), shortcuts);
        let menu_action = self.menu.take_action();
        if menu_action != DrawingContextMenuAction::None {
            action = Some(match menu_action {
                DrawingContextMenuAction::Edit => FloatingToolbarAction::OpenSettings,
                DrawingContextMenuAction::Clone => FloatingToolbarAction::Duplicate,
                DrawingContextMenuAction::Lock => FloatingToolbarAction::ToggleLock,
                DrawingContextMenuAction::Hide => FloatingToolbarAction::ToggleVisibility,
                DrawingContextMenuAction::Delete => FloatingToolbarAction::Delete,
                action => FloatingToolbarAction::DrawingMenu(action),
            });
        }
        action
    }

    fn render_drag_handle(ui: &mut Ui) -> egui::Response {
        let size = Vec2::new(
            ui.active_theme().spacing.lg + ui.active_theme().spacing.md,
            DESIGN_TOKENS.sizing.floating_toolbar.drag_handle_height,
        );
        let (rect, response) = ui.allocate_exact_size(size, Sense::DRAG);

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
        response.on_hover_cursor(egui::CursorIcon::Grab)
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
            for label in ["Settings"]
                .into_iter()
                .chain(alert)
                .chain([if locked { "Unlock" } else { "Lock" }])
                .chain((!locked).then_some("Delete"))
                .chain(["More"])
            {
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
