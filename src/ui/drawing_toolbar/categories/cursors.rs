//! Cursors category for left toolbar.
//!
//! Contains cursor/pointer tools:
//! - Cross (crosshair cursor)
//! - Dot (simple dot cursor)
//! - Arrow (pointer arrow)
//! - Eraser

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use crate::styles::typography;
use egui::{Color32, Pos2, Rect, Ui, Vec2};

use super::{DrawingToolbarAction, ToolCategory};
use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;

/// Cursor tool types (not drawing tools, but cursor modes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorType {
    Cross,
    Dot,
    Arrow,
    Eraser,
}

impl CursorType {
    pub fn all() -> &'static [CursorType] {
        &[
            CursorType::Cross,
            CursorType::Dot,
            CursorType::Arrow,
            CursorType::Eraser,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            CursorType::Cross => "Cross",
            CursorType::Dot => "Dot",
            CursorType::Arrow => "Arrow",
            CursorType::Eraser => "Eraser",
        }
    }

    pub fn icon(&self) -> &'static Icon {
        match self {
            CursorType::Cross => &icons::ERASER,
            CursorType::Dot => &icons::DOT,
            CursorType::Arrow => &icons::ARROW,
            CursorType::Eraser => &icons::ERASER,
        }
    }

    pub fn shortcut(&self) -> Option<&'static str> {
        match self {
            CursorType::Cross => None,
            CursorType::Dot => None,
            CursorType::Arrow => None,
            CursorType::Eraser => None,
        }
    }
}

/// Cursors category implementation
pub struct CursorsCategory;

impl ToolCategory for CursorsCategory {
    fn name(&self) -> &'static str {
        "Cursors"
    }

    fn tooltip(&self) -> &'static str {
        "Cursors"
    }

    fn icon(&self) -> &'static Icon {
        &icons::ERASER
    }

    fn curr_tool_icon(&self, _sel: Option<DrawingToolType>) -> &'static Icon {
        // Cursors don't have drawing tools, always show Cross
        &icons::ERASER
    }

    fn all_tools(&self) -> Vec<DrawingToolType> {
        // Cursors category has no drawing tools
        vec![]
    }

    fn sections(&self) -> Vec<(&'static str, Vec<DrawingToolType>)> {
        // Cursors don't use drawing tools
        vec![]
    }

    fn render_submenu(
        &self,
        ui: &mut Ui,
        anchor_rect: Rect,
        _sel_tool: Option<DrawingToolType>,
        _favorites: &[DrawingToolType],
    ) -> DrawingToolbarAction {
        let mut action = DrawingToolbarAction::None;

        // Create submenu with cursor items
        let submenu_pos = Pos2::new(anchor_rect.right() + 4.0, anchor_rect.min.y);

        egui::Area::new(egui::Id::new("cursors_submenu"))
            .fixed_pos(submenu_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(ui.style().visuals.window_fill)
                    .stroke(egui::Stroke::new(
                        DESIGN_TOKENS.stroke.hairline,
                        ui.style().visuals.widgets.noninteractive.bg_stroke.color,
                    ))
                    .corner_radius(DESIGN_TOKENS.rounding.md)
                    .shadow(egui::Shadow {
                        spread: 2,
                        blur: 8,
                        offset: [2, 2],
                        color: egui::Color32::from_black_alpha(40),
                    })
                    .inner_margin(egui::Margin::same(DESIGN_TOKENS.spacing.sm as i8))
                    .show(ui, |ui| {
                        ui.set_min_width(DESIGN_TOKENS.sizing.dialog.submenu_width);

                        // Get cursor state from context (set by toolbar)
                        let cursor_state = ui.ctx().data(|d| {
                            d.get_temp::<crate::ui::drawing_toolbar::toolbar::CursorStateData>(
                                egui::Id::new("cursor_state"),
                            )
                        });

                        // Render cursor items
                        for cursor in CursorType::all() {
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(
                                    DESIGN_TOKENS.sizing.dialog.submenu_width,
                                    DESIGN_TOKENS.sizing.dialog.submenu_item_height,
                                ),
                                egui::Sense::click(),
                            );

                            // Check if this cursor is currently active
                            let is_active = cursor_state.is_some_and(|state| match cursor {
                                CursorType::Eraser => state.eraser_mode,
                                _ => {
                                    !state.eraser_mode
                                        && state.current_cursor == *cursor
                                }
                            });

                            // Active or hover background
                            if is_active {
                                // Active state - highlighted background
                                let accent = DESIGN_TOKENS.semantic.extended.accent;
                                let accent_with_alpha = Color32::from_rgba_unmultiplied(
                                    accent.r(), accent.g(), accent.b(), 40
                                );
                                ui.painter()
                                    .rect_filled(rect, 2.0, accent_with_alpha);
                            } else if response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    2.0,
                                    ui.style().visuals.widgets.hovered.bg_fill,
                                );
                            }

                            // Checkmark for active cursor
                            if is_active {
                                let check_color = DESIGN_TOKENS.semantic.extended.accent;
                                let check_pos = Pos2::new(rect.min.x + 5.0, rect.center().y);
                                // Draw a simple checkmark
                                let p1 = Pos2::new(check_pos.x - 2.0, check_pos.y);
                                let p2 = Pos2::new(check_pos.x, check_pos.y + 3.0);
                                let p3 = Pos2::new(check_pos.x + 4.0, check_pos.y - 3.0);
                                ui.painter()
                                    .line_segment([p1, p2], egui::Stroke::new(DESIGN_TOKENS.stroke.medium, check_color));
                                ui.painter()
                                    .line_segment([p2, p3], egui::Stroke::new(DESIGN_TOKENS.stroke.medium, check_color));
                            }

                            // Icon - offset to make room for checkmark
                            let icon_size = DESIGN_TOKENS.sizing.icon_md;
                            let icon_x_offset = if is_active {
                                DESIGN_TOKENS.spacing.lg + DESIGN_TOKENS.spacing.md
                            } else {
                                DESIGN_TOKENS.spacing.lg
                            };
                            let icon_rect = Rect::from_min_size(
                                Pos2::new(
                                    rect.min.x + icon_x_offset,
                                    rect.center().y - icon_size / 2.0,
                                ),
                                Vec2::splat(icon_size),
                            );
                            crate::ui::drawing_toolbar::components::svg_helpers::render_svg_at_rect_themed(
                                ui,
                                cursor.icon(),
                                icon_rect,
                                response.hovered(),
                                is_active,
                            );

                            // Name - offset when active
                            let name_x_offset = if is_active {
                                DESIGN_TOKENS.sizing.drawing_toolbar_ext.name_offset_active
                            } else {
                                DESIGN_TOKENS.sizing.drawing_toolbar_ext.name_offset_inactive
                            };
                            let text_color = if is_active {
                                DESIGN_TOKENS.semantic.extended.accent
                            } else {
                                ui.style().visuals.text_color()
                            };
                            ui.painter().text(
                                Pos2::new(rect.min.x + name_x_offset, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                cursor.name(),
                                egui::FontId::proportional(typography::SM_MD),
                                text_color,
                            );

                            // Shortcut (if any)
                            if let Some(shortcut) = cursor.shortcut() {
                                ui.painter().text(
                                    Pos2::new(rect.right() - 8.0, rect.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    shortcut,
                                    egui::FontId::proportional(typography::XS),
                                    ui.style().visuals.widgets.noninteractive.fg_stroke.color,
                                );
                            }

                            if response.clicked() {
                                action = match cursor {
                                    CursorType::Cross => {
                                        DrawingToolbarAction::SetCursorType(CursorType::Cross)
                                    }
                                    CursorType::Dot => {
                                        DrawingToolbarAction::SetCursorType(CursorType::Dot)
                                    }
                                    CursorType::Arrow => {
                                        DrawingToolbarAction::SetCursorType(CursorType::Arrow)
                                    }
                                    CursorType::Eraser => DrawingToolbarAction::ToggleEraserMode,
                                };
                            }
                        }

                        // Separator
                        ui.space_sm();
                        let sep_rect = ui.allocate_space(Vec2::new(DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_width_lg, DESIGN_TOKENS.spacing.hairline)).1;
                        ui.painter().rect_filled(
                            sep_rect,
                            0.0,
                            ui.style().visuals.widgets.noninteractive.bg_stroke.color,
                        );
                        ui.space_sm();

                        // "Values tooltip on long press" toggle
                        let (toggle_rect, toggle_res) = ui.allocate_exact_size(
                            Vec2::new(
                                DESIGN_TOKENS.sizing.dialog.submenu_width,
                                DESIGN_TOKENS.sizing.dialog.submenu_item_height,
                            ),
                            egui::Sense::click(),
                        );

                        if toggle_res.hovered() {
                            ui.painter().rect_filled(
                                toggle_rect,
                                2.0,
                                ui.style().visuals.widgets.hovered.bg_fill,
                            );
                        }

                        ui.painter().text(
                            Pos2::new(toggle_rect.min.x + 8.0, toggle_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            "Values tooltip on long press",
                            egui::FontId::proportional(typography::SM),
                            ui.style().visuals.text_color(),
                        );

                        if toggle_res.clicked() {
                            action = DrawingToolbarAction::ToggleValuesTooltip;
                        }
                    });
            });

        action
    }
}
