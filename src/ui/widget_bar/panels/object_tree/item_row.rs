//! Item row renderer for the Object Tree
//!
//! Renders individual items (drawings, indicators) in the tree.

use egui::{Rect, Sense, Stroke, Ui, vec2};

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons as embedded_icons};
use crate::styles::typography;
use crate::theme::ThemeExt;

use super::actions::ObjectTreeAction;
use super::config::ObjectTreeConfig;
use super::context_menu::render_item_menu;
use super::state::ObjectTreeState;
use super::types::{SourceItem, SourceType};
use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;

/// Render a single item row in the object tree
pub fn render_item_row(
    ui: &mut Ui,
    item: &mut SourceItem,
    config: &ObjectTreeConfig,
    state: &mut ObjectTreeState,
    indent_level: usize,
) -> ObjectTreeAction {
    let id = item.id;

    // Calculate indentation
    let indent = indent_level as f32 * config.indent_width;

    // Allocate row
    let available_width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        vec2(available_width, config.row_height),
        Sense::click_and_drag(),
    );

    // Handle interactions
    let mut action = handle_interactions(ui, &response, id, config, state);

    // Render if visible
    if ui.is_rect_visible(rect) {
        render_row_content(ui, rect, item, config, state, indent, &mut action);
    }

    if config.enable_context_menu {
        response.context_menu(|ui| {
            let menu_action = render_item_menu(ui, item, state);
            if !menu_action.is_none() {
                action = menu_action;
            }
        });
    }

    response.on_hover_text(item.full_display_name());

    // Handle expand/collapse for items with properties
    if item.source_type == SourceType::Drawing && state.is_item_expanded(id) {
        ui.indent(format!("item_props_{id}"), |ui| {
            if let Some(props) = &item.properties {
                // Show expandable properties
                render_properties(ui, props, id, &mut action);
            }
        });
    }

    action
}

/// Handle row interactions (clicks, drag, context menu)
fn handle_interactions(
    ui: &mut Ui,
    response: &egui::Response,
    id: usize,
    config: &ObjectTreeConfig,
    state: &mut ObjectTreeState,
) -> ObjectTreeAction {
    let mut action = ObjectTreeAction::None;

    // Click handling
    if response.clicked() {
        let modifiers = ui.input(|i| i.modifiers);

        if modifiers.ctrl && config.enable_multi_select {
            action = ObjectTreeAction::AddToSelection(id);
        } else if modifiers.shift && config.enable_multi_select {
            action = ObjectTreeAction::RangeSelect(id);
        } else {
            action = ObjectTreeAction::Select(id);
        }
    }

    // Double-click to zoom
    if response.double_clicked() {
        action = ObjectTreeAction::ZoomTo(id);
    }

    // Drag start
    if config.enable_drag_drop
        && response.drag_started()
        && let Some(pos) = response.interact_pointer_pos()
    {
        state.start_drag(id, pos);
    }

    // Hover tracking
    if response.hovered() {
        state.hovered_id = Some(id);
    }

    action
}

/// Render the row content (background, icons, text, buttons)
fn render_row_content(
    ui: &mut Ui,
    rect: Rect,
    item: &SourceItem,
    config: &ObjectTreeConfig,
    state: &mut ObjectTreeState,
    indent: f32,
    action: &mut ObjectTreeAction,
) {
    let is_selected = state.is_selected(item.id);
    let is_hovered = state.hovered_id == Some(item.id);
    let is_dragging = state.dragging_id == Some(item.id);

    let center_y = rect.center().y;

    // First pass: paint background and static elements
    {
        let painter = ui.painter();
        let accent = ui.active_theme().ui.accent;

        // Background
        if is_selected {
            painter.rect_filled(rect, 0.0, ui.visuals().selection.bg_fill);
        } else if is_hovered && !is_dragging {
            painter.rect_filled(rect, 0.0, ui.style().visuals.widgets.hovered.bg_fill);
        }

        // Drag indicator
        if is_dragging {
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(DESIGN_TOKENS.stroke.hairline, accent),
                egui::StrokeKind::Inside,
            );
        }

        // Drop indicator
        if state.drop_target == Some(item.id) {
            let drop_line = Rect::from_min_size(rect.left_top(), vec2(rect.width(), 2.0));
            painter.rect_filled(drop_line, 0.0, accent);
        }

        // Calculate x position starting after indent
        let mut x = rect.left() + indent + DESIGN_TOKENS.spacing.sm;

        // Expand/collapse chevron (for items with properties)
        if config.show_expand_chevron && item.source_type == SourceType::Drawing {
            let chevron = if state.is_item_expanded(item.id) {
                "v"
            } else {
                ">"
            };
            painter.text(
                egui::pos2(x + 6.0, center_y),
                egui::Align2::LEFT_CENTER,
                chevron,
                egui::FontId::proportional(typography::XS),
                DESIGN_TOKENS.semantic.extended.text_muted,
            );
            x += 16.0;
        }

        // Skip interactive elements space
        x += 20.0; // visibility
        if item.source_type == SourceType::Drawing && config.show_lock_indicator {
            x += 20.0; // lock
        }

        // Color indicator
        if config.show_color_indicators {
            let color_rect = Rect::from_center_size(
                egui::pos2(x + config.color_indicator_size / 2.0, center_y),
                vec2(config.color_indicator_size, config.color_indicator_size),
            );
            painter.rect_filled(color_rect, 2.0, item.color);
            painter.rect_stroke(
                color_rect,
                2.0,
                Stroke::new(
                    DESIGN_TOKENS.stroke.hairline,
                    DESIGN_TOKENS.semantic.extended.gray,
                ),
                egui::StrokeKind::Inside,
            );
            x += config.color_indicator_size + DESIGN_TOKENS.spacing.sm;
        }

        // Tool icon abbreviation (for drawings)
        if let Some(tool_type) = item.tool_type {
            let abbrev = get_tool_abbreviation(tool_type);
            painter.text(
                egui::pos2(x + 10.0, center_y),
                egui::Align2::LEFT_CENTER,
                abbrev,
                egui::FontId::proportional(typography::TINY),
                DESIGN_TOKENS.semantic.extended.gray,
            );
            x += 24.0;
        }

        // Name
        let name_color = if is_selected {
            ui.visuals().selection.stroke.color
        } else if item.visible {
            ui.style().visuals.text_color()
        } else {
            DESIGN_TOKENS.semantic.extended.gray
        };
        let mut job = egui::text::LayoutJob::simple(
            item.full_display_name(),
            egui::FontId::proportional(typography::SM_MD),
            name_color,
            (rect.right() - DESIGN_TOKENS.spacing.sm - 40.0 - x).max(0.0),
        );
        job.wrap.max_rows = 1;
        let galley = painter.layout_job(job);
        let pos = egui::pos2(x, center_y - galley.size().y / 2.0);
        painter.galley(pos, galley, name_color);
    } // painter dropped

    // Second pass: interactive elements
    let mut x = rect.left() + indent + DESIGN_TOKENS.spacing.sm;

    // Expand/collapse button
    if config.show_expand_chevron && item.source_type == SourceType::Drawing {
        let chevron_rect = Rect::from_center_size(egui::pos2(x + 6.0, center_y), vec2(14.0, 14.0));
        let chevron_response = ui.put(chevron_rect, egui::Label::new("").sense(Sense::click()));
        if chevron_response.clicked() {
            state.toggle_item(item.id);
        }
        x += 16.0;
    }

    // Visibility toggle
    let vis_rect = Rect::from_center_size(egui::pos2(x + 8.0, center_y), vec2(16.0, 16.0));
    let vis_icon: &Icon = if item.visible {
        &embedded_icons::EYE
    } else {
        &embedded_icons::EYE_HIDE
    };
    let vis_response = ui.allocate_rect(vis_rect, Sense::click());
    if ui.is_rect_visible(vis_rect) {
        let icon_color = if vis_response.hovered() {
            ui.visuals().widgets.hovered.fg_stroke.color
        } else {
            ui.visuals().widgets.noninteractive.fg_stroke.color
        };
        vis_icon
            .as_image_tinted(vec2(14.0, 14.0), icon_color)
            .paint_at(ui, vis_rect);
    }
    if vis_response.clicked() {
        *action = ObjectTreeAction::ToggleVisibility(item.id);
    }
    vis_response.on_hover_text(if item.visible { "Hide" } else { "Show" });
    x += 20.0;

    // Lock toggle (drawings only)
    if item.source_type == SourceType::Drawing && config.show_lock_indicator {
        let lock_rect = Rect::from_center_size(egui::pos2(x + 8.0, center_y), vec2(16.0, 16.0));
        let lock_icon: &Icon = if item.locked {
            &embedded_icons::LOCK
        } else {
            &embedded_icons::UNLOCK
        };
        let lock_response = ui.allocate_rect(lock_rect, Sense::click());
        if ui.is_rect_visible(lock_rect) {
            let icon_color = if lock_response.hovered() {
                ui.visuals().widgets.hovered.fg_stroke.color
            } else {
                ui.visuals().widgets.noninteractive.fg_stroke.color
            };
            lock_icon
                .as_image_tinted(vec2(14.0, 14.0), icon_color)
                .paint_at(ui, lock_rect);
        }
        if lock_response.clicked() {
            *action = ObjectTreeAction::ToggleLock(item.id);
        }
        lock_response.on_hover_text(if item.locked { "Unlock" } else { "Lock" });
    }

    // Right-side buttons
    let mut right_x = rect.right() - DESIGN_TOKENS.spacing.sm;

    // Delete button
    let del_rect = Rect::from_center_size(egui::pos2(right_x - 8.0, center_y), vec2(16.0, 16.0));
    let del_response = ui.put(del_rect, egui::Label::new("X").sense(Sense::click()));
    if del_response.clicked() {
        *action = if item.source_type == SourceType::Indicator {
            ObjectTreeAction::RemoveIndicator(item.id)
        } else {
            ObjectTreeAction::Delete(item.id)
        };
    }
    del_response.on_hover_text("Remove");
    right_x -= 20.0;

    // Settings/Properties button
    let settings_rect =
        Rect::from_center_size(egui::pos2(right_x - 8.0, center_y), vec2(16.0, 16.0));
    let settings_response = ui.put(settings_rect, egui::Label::new("S").sense(Sense::click()));
    if settings_response.clicked() {
        *action = if item.source_type == SourceType::Indicator {
            ObjectTreeAction::OpenIndicatorSettings(item.id)
        } else {
            ObjectTreeAction::OpenProperties(item.id)
        };
    }
    settings_response.on_hover_text("Settings");
}

/// Render expandable properties for a drawing
fn render_properties(
    ui: &mut Ui,
    props: &super::types::DrawingProperties,
    id: usize,
    action: &mut ObjectTreeAction,
) {
    ui.space_xs();

    egui::Grid::new(format!("props_grid_{id}"))
        .num_columns(2)
        .spacing([DESIGN_TOKENS.spacing.sm, DESIGN_TOKENS.spacing.xs])
        .show(ui, |ui| {
            // Color
            ui.hint_label("Color");
            let mut color = props.color;
            if ui.color_edit_button_srgba(&mut color).changed() {
                *action = ObjectTreeAction::ChangeColor(id, color);
            }
            ui.end_row();

            // Line width
            ui.hint_label("Width");
            ui.label(format!("{}px", props.line_width));
            ui.end_row();

            // Line style
            ui.hint_label("Style");
            ui.label(props.line_style.as_str());
            ui.end_row();

            // Transparency
            if props.transparency > 0 {
                ui.hint_label("Opacity");
                ui.label(format!("{}%", 100 - props.transparency));
                ui.end_row();
            }
        });

    ui.space_xs();
}

/// Get abbreviated tool type name for display
fn get_tool_abbreviation(tool_type: DrawingToolType) -> &'static str {
    match tool_type {
        DrawingToolType::TrendLine => "TL",
        DrawingToolType::HorizontalLine => "HL",
        DrawingToolType::VerticalLine => "VL",
        DrawingToolType::Ray => "RY",
        DrawingToolType::ExtendedLine => "EL",
        DrawingToolType::FibonacciRetracement => "FR",
        DrawingToolType::FibonacciExtension => "FE",
        DrawingToolType::FibonacciChannel => "FC",
        DrawingToolType::GannBox => "GB",
        DrawingToolType::GannFan => "GF",
        DrawingToolType::Rect => "RT",
        DrawingToolType::Circle => "CR",
        DrawingToolType::Ellipse => "EL",
        DrawingToolType::Triangle => "TR",
        DrawingToolType::Brush => "BR",
        DrawingToolType::Highlighter => "HI",
        DrawingToolType::TextLabel => "TX",
        DrawingToolType::Note => "NT",
        DrawingToolType::Arrow => "AR",
        DrawingToolType::ArrowMarker => "AM",
        DrawingToolType::Pitchfork => "PF",
        DrawingToolType::XABCDPattern => "XB",
        DrawingToolType::ElliottImpulse => "EI",
        DrawingToolType::Measure => "MS",
        _ => "••",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Color32;

    #[test]
    fn selected_names_use_selection_colours_and_end_before_the_buttons() {
        for (_, theme) in crate::theme::presets() {
            for width in [220.0, 280.0] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 200.0))),
                    ..Default::default()
                };
                let mut row = Rect::NOTHING;
                let mut selection = egui::style::Selection::default();
                let mut output = ctx.run_ui(input, |ui| {
                    selection = ui.visuals().selection;
                    let mut item =
                        SourceItem::drawing(1, DrawingToolType::ElliottTripleCombo, Color32::BLUE);
                    let mut state = ObjectTreeState::new();
                    state.select(1);
                    row = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 24.0));
                    render_item_row(ui, &mut item, &ObjectTreeConfig::default(), &mut state, 0);
                });
                output.textures_delta.clear();
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(rect) if rect.rect == row && rect.fill == selection.bg_fill)
                }));
                let name = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text)
                            if text.galley.text().starts_with("Elliott Triple") =>
                        {
                            Some(text)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(name.fallback_color, selection.stroke.color);
                assert!(name.galley.elided);
                assert!(
                    name.pos.x + name.galley.rect.right()
                        <= row.right() - DESIGN_TOKENS.spacing.sm - 40.0
                );
                output.drop_without_applying_deltas();
            }
        }
    }
}
