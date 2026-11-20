//! Floating strip of favourite drawing tools.

use super::super::categories::cursors::CursorType;
use super::super::{components::render_svg_at_rect_themed, icons};
use super::{DrawingToolbar, DrawingToolbarAction};
use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons as embedded_icons};
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Id, Pos2, Rect, Sense, Ui, Vec2};

impl DrawingToolbar {
    /// Render the floating strip of favourite drawing tools and return a drawing pick.
    /// Use [`Self::show_floating_with_action`] to include cursor modes and receive their host
    /// actions. Registered tools appear only with [`Self::show_floating_with_action`].
    ///
    /// [`Self::show`] and [`Self::show_with_action`] already draw it beside the toolbar and return
    /// a tool picked on it; call this only to place the strip elsewhere, before them, as a
    /// second call in the same pass draws nothing.
    ///
    /// The strip lists [`ToolbarState::favorites`][favorites], so starring or unstarring changes it
    /// at once, and it is hidden while no visible drawing tools are starred. `position` places it
    /// until it has been moved; afterwards [`ToolbarState::floating_pos`][floating_pos] holds its position. Its grip drags it, kept
    /// on screen, and its close button clears [`ToolbarState::floating_visible`][floating_visible], as the
    /// favourites button of the toolbar does.
    ///
    /// A click on a tool the host disabled returns `None` here;
    /// [`Self::show_floating_with_action`] reports it as
    /// [`DrawingToolbarAction::ToolUnavailable`].
    ///
    /// [favorites]: super::super::state::ToolbarState::favorites
    /// [floating_pos]: super::super::state::ToolbarState::floating_pos
    /// [floating_visible]: super::super::state::ToolbarState::floating_visible
    pub fn show_floating(&mut self, ui: &mut Ui, position: Pos2) -> Option<DrawingToolType> {
        match self.draw_floating(ui, position, false) {
            Some(DrawingToolbarAction::SelectTool(tool)) => Some(tool),
            _ => None,
        }
    }

    /// Render the favourite tools and cursors, applying the pick and returning its host action.
    /// Cursor favourites alone keep this strip visible.
    /// Use this instead of [`Self::show_floating`] when placing the strip separately and handling
    /// cursor actions. The regular toolbar already calls it.
    pub fn show_floating_with_action(
        &mut self,
        ui: &mut Ui,
        position: Pos2,
    ) -> Option<DrawingToolbarAction> {
        self.draw_floating(ui, position, true)
    }

    /// Draw the strip; `cursors` adds the starred cursor modes, which only the action API reports.
    fn draw_floating(
        &mut self,
        ui: &mut Ui,
        position: Pos2,
        cursors: bool,
    ) -> Option<DrawingToolbarAction> {
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
        let custom: Vec<_> = self
            .state
            .custom_favorites
            .iter()
            .filter_map(|id| {
                let tool = self.registry.get(id)?;
                (cursors && self.registry.custom_availability(id).visible)
                    .then_some((id.clone(), tool))
            })
            .collect();
        if !self.state.floating_visible
            || (tools.is_empty()
                && (!cursors || (self.state.cursor_favorites.is_empty() && custom.is_empty())))
        {
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
                                let is_sel = self.state.sel_builtin() == Some(tool);
                                let enabled = self.registry.availability(tool).enabled;
                                if Self::strip_tool(
                                    ui,
                                    icons::get_icon(tool),
                                    tool.as_str(),
                                    is_sel,
                                    enabled,
                                )
                                .clicked()
                                {
                                    new_selection = Some(if enabled {
                                        DrawingToolbarAction::SelectTool(tool)
                                    } else {
                                        DrawingToolbarAction::ToolUnavailable(tool)
                                    });
                                }
                            }

                            for (id, tool) in &custom {
                                let enabled = self.registry.custom_availability(id).enabled;
                                let selected =
                                    self.state.sel_custom_tool.as_deref() == Some(id.as_str());
                                let icon = tool
                                    .icon
                                    .as_ref()
                                    .unwrap_or(icons::get_icon(DrawingToolType::Polyline));
                                if Self::strip_tool(
                                    ui,
                                    icon,
                                    self.registry.custom_name(id),
                                    selected,
                                    enabled,
                                )
                                .clicked()
                                {
                                    new_selection = Some(if enabled {
                                        DrawingToolbarAction::SelectCustomTool(id.clone())
                                    } else {
                                        DrawingToolbarAction::CustomToolUnavailable(id.clone())
                                    });
                                }
                            }

                            if cursors {
                                for cursor in &self.state.cursor_favorites {
                                    let selected = self.state.sel_tool.is_none()
                                        && !self.state.zoom_mode_active
                                        && if *cursor == CursorType::Eraser {
                                            self.state.eraser_mode_enabled
                                        } else {
                                            !self.state.eraser_mode_enabled
                                                && self.state.current_cursor_type == *cursor
                                        };
                                    let response =
                                        crate::ui_kit::buttons::IconButton::new(cursor.icon())
                                            .selected(selected)
                                            .tooltip(cursor.name())
                                            .show(ui);
                                    if response.clicked() {
                                        new_selection = Some(if *cursor == CursorType::Eraser {
                                            DrawingToolbarAction::ToggleEraserMode
                                        } else {
                                            DrawingToolbarAction::SetCursorType(*cursor)
                                        });
                                    }
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

        self.apply_pick(new_selection?)
    }

    fn strip_tool(
        ui: &mut Ui,
        icon: &Icon,
        label: &str,
        is_sel: bool,
        enabled: bool,
    ) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::splat(DESIGN_TOKENS.sizing.button_md), Sense::click());
        let translated_label = l10n::text(ui.ctx(), label);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, enabled, is_sel, &translated_label)
        });
        crate::styles::focus::draw_focus_ring(ui, &response, ui.active_theme().radius.sm);
        // A disabled tool is drawn as egui draws a disabled widget but
        // still answers clicks, so the host can say why.
        let opacity = ui.opacity();
        if !enabled {
            ui.multiply_opacity(ui.visuals().disabled_alpha());
        }

        let bg = if is_sel {
            ui.active_theme().ui.accent
        } else if response.hovered() {
            ui.visuals().widgets.hovered.bg_fill
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, ui.active_theme().radius.sm, bg);

        let icon_rect =
            Rect::from_center_size(rect.center(), Vec2::splat(DESIGN_TOKENS.sizing.icon_md));
        render_svg_at_rect_themed(ui, icon, icon_rect, response.hovered(), is_sel);
        ui.set_opacity(opacity);

        response.on_hover_text(translated_label)
    }

    /// The dotted grip at the strip's leading edge. Returns how far the user dragged it.
    fn strip_grip(ui: &mut Ui) -> Vec2 {
        let size = Vec2::new(DESIGN_TOKENS.sizing.icon_sm, DESIGN_TOKENS.sizing.button_md);
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
