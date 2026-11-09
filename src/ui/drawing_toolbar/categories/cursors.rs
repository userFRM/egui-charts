//! Cursors category for left toolbar.
//!
//! Contains cursor/pointer tools:
//! - Cross (crosshair cursor)
//! - Dot (simple dot cursor)
//! - Arrow (pointer arrow)
//! - Eraser

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use crate::localization as l10n;
use egui::Ui;

use super::{DrawingToolbarAction, ToolCategory};
use crate::theme::ThemeExt;
use crate::ui::drawing_toolbar::toolbar::CursorStateData;

pub use crate::config::CursorType;

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
        &icons::CROSSHAIR
    }

    fn curr_tool_icon(&self, _sel: Option<DrawingToolType>) -> &'static Icon {
        // Cursors don't have drawing tools, always show Cross
        &icons::CROSSHAIR
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
        _sel_tool: Option<DrawingToolType>,
        _favorites: &mut Vec<DrawingToolType>,
    ) -> DrawingToolbarAction {
        let id = ui.make_persistent_id("cursor_favorites");
        let mut favorites = ui
            .data_mut(|data| data.remove_temp::<Vec<CursorType>>(id))
            .unwrap_or_default();
        let action = self.render_with_favorites(ui, &mut favorites);
        ui.data_mut(|data| data.insert_temp(id, favorites));
        action
    }
}

impl CursorsCategory {
    /// Show the cursor menu with host-owned favourites. Keep and serialize `favorites` with
    /// user preferences; [`ToolCategory::render_submenu`] retains them only for the UI session.
    pub fn render_with_favorites(
        &self,
        ui: &mut Ui,
        favorites: &mut Vec<CursorType>,
    ) -> DrawingToolbarAction {
        // The toolbar stores its cursor state for the frame before it shows this menu.
        let Some(state) = ui
            .ctx()
            .data(|d| d.get_temp::<CursorStateData>(CursorStateData::id()))
        else {
            return DrawingToolbarAction::None;
        };
        let values_label = "Values tooltip on long press";

        let theme = ui.active_theme();
        let font = egui::FontId::proportional(16.0);
        let width = ui
            .painter()
            .layout_no_wrap(
                l10n::text(ui.ctx(), values_label),
                egui::FontId::proportional(14.0),
                theme.ui.text,
            )
            .size()
            .x
            + 64.0;
        ui.set_min_width(width);
        let mut action = DrawingToolbarAction::None;
        for cursor in CursorType::all() {
            if *cursor == CursorType::Eraser {
                ui.separator();
            }
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(width, 40.0), egui::Sense::click());
            let active = state.is_active(*cursor);
            let favorite = favorites.contains(cursor);
            let translated_label = l10n::text(ui.ctx(), cursor.name());
            let star_rect = egui::Rect::from_center_size(
                rect.right_center() - egui::vec2(20.0, 0.0),
                egui::vec2(28.0, rect.height()),
            );
            let star = ui.interact(
                star_rect,
                response.id.with("favorite"),
                egui::Sense::click(),
            );
            let label = l10n::format_text(ui.ctx(), "Favourite {0}", &[("0", &translated_label)]);
            star.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::Button, star.enabled(), favorite, &label)
            });
            let hovered = ui.rect_contains_pointer(rect);
            let foreground = if active {
                theme.ui.panel_bg
            } else {
                theme.ui.text
            };
            if active || hovered {
                ui.painter().rect_filled(
                    rect,
                    6,
                    if active {
                        theme.ui.text
                    } else {
                        theme.ui.btn_bg_hover
                    },
                );
            }
            cursor
                .icon()
                .as_image_tinted(egui::Vec2::splat(24.0), foreground)
                .paint_at(
                    ui,
                    egui::Rect::from_center_size(
                        egui::pos2(rect.left() + 20.0, rect.center().y),
                        egui::Vec2::splat(24.0),
                    ),
                );
            ui.painter().text(
                egui::pos2(rect.left() + 40.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                &translated_label,
                font.clone(),
                foreground,
            );
            if favorite || hovered || response.has_focus() || star.has_focus() {
                let icon = if favorite {
                    &icons::STAR_FILLED
                } else {
                    &icons::STAR_EMPTY
                };
                let color = if active {
                    foreground
                } else if favorite {
                    theme.ui.warning
                } else {
                    crate::ui_kit::section::menu_heading_color(&theme)
                };
                icon.as_image_tinted(egui::Vec2::splat(16.0), color)
                    .paint_at(
                        ui,
                        egui::Rect::from_center_size(star_rect.center(), egui::Vec2::splat(16.0)),
                    );
            }
            crate::styles::focus::draw_focus_ring(ui, &star, theme.radius.sm);
            if star.has_focus() {
                star.scroll_to_me(None);
            }
            if star.clicked() {
                if favorite {
                    favorites.retain(|value| value != cursor);
                } else {
                    favorites.push(*cursor);
                }
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::RadioButton,
                    response.enabled(),
                    active,
                    &translated_label,
                )
            });
            if response.has_focus() {
                ui.painter().rect_stroke(
                    rect.shrink(2.0),
                    6,
                    egui::Stroke::new(
                        2.0,
                        if active {
                            foreground
                        } else {
                            crate::styles::focus::focus_ring_color(ui)
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
                response.scroll_to_me(None);
            }
            if response.clicked() {
                action = match cursor {
                    CursorType::Eraser => DrawingToolbarAction::ToggleEraserMode,
                    _ => DrawingToolbarAction::SetCursorType(*cursor),
                };
            }
        }
        ui.separator();
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, 40.0), egui::Sense::click());
        ui.painter().text(
            rect.left_center() + egui::vec2(8.0, 0.0),
            egui::Align2::LEFT_CENTER,
            l10n::text(ui.ctx(), values_label),
            egui::FontId::proportional(14.0),
            theme.ui.text,
        );
        let switch = egui::Rect::from_center_size(
            rect.right_center() - egui::vec2(24.0, 0.0),
            egui::vec2(38.0, 20.0),
        );
        ui.painter().rect_filled(
            switch,
            10,
            if state.values_tooltip {
                theme.ui.text
            } else {
                theme.ui.border_control
            },
        );
        let knob_x = if state.values_tooltip {
            switch.right() - 10.0
        } else {
            switch.left() + 10.0
        };
        ui.painter().circle_filled(
            egui::pos2(knob_x, switch.center().y),
            7.0,
            theme.ui.panel_bg,
        );
        let translated_label = l10n::text(ui.ctx(), values_label);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Switch,
                response.enabled(),
                state.values_tooltip,
                &translated_label,
            )
        });
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.shrink(2.0),
                6,
                egui::Stroke::new(2.0, crate::styles::focus::focus_ring_color(ui)),
                egui::StrokeKind::Inside,
            );
            response.scroll_to_me(None);
        }
        if response.clicked() {
            action = DrawingToolbarAction::ToggleValuesTooltip;
        }
        action
    }
}
