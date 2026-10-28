//! Floating strip of favourite drawing tools.

use super::super::{components::render_svg_at_rect_themed, icons};
use super::DrawingToolbar;
use crate::drawings::DrawingToolType;
use crate::icons::icons as embedded_icons;
use crate::localization as l10n;
use crate::styles::icons as icon_sizes;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Id, Pos2, Rect, Sense, Ui, Vec2};

impl DrawingToolbar {
    /// Render the floating strip of favourite drawing tools.
    ///
    /// [`Self::show`] and [`Self::show_with_action`] already draw it beside the toolbar and return
    /// a tool picked on it; call this only to place the strip elsewhere, before them, as a
    /// second call in the same pass draws nothing.
    ///
    /// The strip lists [`ToolbarState::favorites`], so starring or unstarring a tool changes it
    /// at once, and it is hidden while there are none. `position` places it until it has been
    /// moved; afterwards [`ToolbarState::floating_pos`] holds its position. Its grip drags it, kept
    /// on screen, and its close button clears [`ToolbarState::floating_visible`], as the
    /// favourites button of the toolbar does.
    pub fn show_floating(&mut self, ui: &mut Ui, position: Pos2) -> Option<DrawingToolType> {
        let pass = ui.ctx().cumulative_pass_nr();
        if self.strip_pass == Some(pass) {
            return None;
        }
        let tools: Vec<_> = self
            .state
            .favorites
            .iter()
            .copied()
            .filter(|tool| self.registry.availability(*tool).visible)
            .collect();
        if !self.state.floating_visible || tools.is_empty() {
            return None;
        }
        self.strip_pass = Some(pass);

        let mut new_selection = None;
        let mut moved = Vec2::ZERO;
        let area = egui::Area::new(Id::NULL.with("floating_drawing_toolbar"))
            .enabled(ui.is_enabled())
            .current_pos(self.state.floating_pos.unwrap_or(position))
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .stroke(ui.style().visuals.window_stroke)
                    .corner_radius(ui.active_theme().radius.md)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            moved = Self::strip_grip(ui);
                            for tool in tools {
                                let is_sel = self.state.sel_tool == Some(tool);
                                let enabled = self.registry.availability(tool).enabled;
                                let (rect, response) = ui.allocate_exact_size(
                                    Vec2::splat(DESIGN_TOKENS.sizing.button_md),
                                    Sense::click(),
                                );
                                let translated_label = l10n::text(ui.ctx(), tool.as_str());
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::Role::Button,
                                        enabled,
                                        is_sel,
                                        &translated_label,
                                    )
                                });
                                crate::styles::focus::draw_focus_ring(
                                    ui,
                                    &response,
                                    ui.active_theme().radius.sm,
                                );

                                let bg = if is_sel {
                                    ui.active_theme().ui.accent
                                } else if response.hovered() {
                                    ui.visuals().widgets.hovered.bg_fill
                                } else {
                                    Color32::TRANSPARENT
                                };
                                ui.painter()
                                    .rect_filled(rect, ui.active_theme().radius.sm, bg);

                                let icon_rect = Rect::from_center_size(
                                    rect.center(),
                                    Vec2::splat(icon_sizes::MD),
                                );
                                render_svg_at_rect_themed(
                                    ui,
                                    icons::get_icon(tool),
                                    icon_rect,
                                    response.hovered(),
                                    is_sel,
                                );

                                if response.on_hover_text(translated_label).clicked() && enabled {
                                    new_selection = Some(tool);
                                }
                            }

                            ui.separator();

                            if crate::ui_kit::buttons::Button::icon(&embedded_icons::CLOSE)
                                .ghost()
                                .tooltip("Hide favorite tools")
                                .show(ui)
                                .clicked()
                            {
                                self.state.floating_visible = false;
                            }
                        });
                    });
            });
        self.state.floating_pos = Some(area.response.rect.min + moved);
        if moved != Vec2::ZERO {
            ui.ctx().request_repaint();
        }

        if let Some(tool) = new_selection {
            self.select_tool(tool);
        }

        new_selection
    }

    /// The dotted grip at the strip's leading edge. Returns how far the user dragged it.
    fn strip_grip(ui: &mut Ui) -> Vec2 {
        let size = Vec2::new(icon_sizes::SM, DESIGN_TOKENS.sizing.button_md);
        let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
        let label = l10n::text(ui.ctx(), "Move favorite tools");
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        let response = response.on_hover_cursor(if ui.input(|i| i.pointer.any_down()) {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
        let color = if response.hovered() || response.dragged() {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        for col in [-2.0, 2.0] {
            for row in [-5.0, 0.0, 5.0] {
                ui.painter()
                    .circle_filled(rect.center() + Vec2::new(col, row), 1.0, color);
            }
        }
        response.drag_delta()
    }
}
