//! Item row renderer for the Object Tree
//!
//! Renders individual items (drawings, indicators) in the tree.

use crate::localization as l10n;
use egui::{Color32, Rect, Sense, Stroke, Ui, vec2};

use crate::icons::{Icon, icons as embedded_icons};
use crate::theme::ThemeExt;

use super::actions::ObjectTreeAction;
use super::config::ObjectTreeConfig;
use super::context_menu::render_item_menu;
use super::state::ObjectTreeState;
use super::types::{SourceItem, SourceType};
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::ext::UiExt;

/// Render a single item row in the object tree
pub fn render_item_row(
    ui: &mut Ui,
    item: &mut SourceItem,
    config: &ObjectTreeConfig,
    state: &mut ObjectTreeState,
    indent_level: usize,
) -> ObjectTreeAction {
    let id = item.id;
    state.visible_ids.push(id);

    // Calculate indentation
    let indent_width = if config.indent_width == ObjectTreeConfig::default().indent_width {
        ui.active_theme().spacing.xl
    } else {
        config.indent_width
    };
    let indent = indent_level as f32 * indent_width;

    // Allocate row
    let available_width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        vec2(available_width, config.row_height),
        Sense::click_and_drag(),
    );

    let translated_label = l10n::text(ui.ctx(), item.full_display_name());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::TreeItem, response.enabled(), &translated_label)
    });
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_selected(state.is_selected(id)));

    // Handle interactions
    let mut action = handle_interactions(ui, &response, id, config, state);

    // Render if visible
    if ui.is_rect_visible(rect) {
        render_row_content(ui, &response, item, config, state, indent, &mut action);
    }
    crate::styles::focus::draw_focus_ring(ui, &response, 0.0);

    if config.enable_context_menu {
        response.context_menu(|ui| {
            let menu_action = render_item_menu(ui, item, state);
            if !menu_action.is_none() {
                action = menu_action;
            }
        });
    }

    response.on_hover_text(l10n::text(ui.ctx(), item.full_display_name()));

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
    response: &egui::Response,
    item: &SourceItem,
    config: &ObjectTreeConfig,
    state: &mut ObjectTreeState,
    indent: f32,
    action: &mut ObjectTreeAction,
) {
    let rect = response.rect;
    let is_selected = state.is_selected(item.id);
    let is_hovered = response.hovered();
    let is_dragging = state.dragging_id == Some(item.id);

    let center_y = rect.center().y;
    let icon_color = if is_selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().widgets.noninteractive.fg_stroke.color
    };

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
        let mut x = rect.left() + indent + ui.active_theme().spacing.sm;

        // Expand/collapse chevron (for items with properties)
        if config.show_expand_chevron && item.source_type == SourceType::Drawing {
            let chevron = if state.is_item_expanded(item.id) {
                &embedded_icons::CHEVRON_DOWN
            } else {
                &embedded_icons::CHEVRON_RIGHT
            };
            chevron
                .as_image_tinted(vec2(16.0, 16.0), icon_color)
                .paint_at(
                    ui,
                    Rect::from_center_size(egui::pos2(x + 6.0, center_y), vec2(16.0, 16.0)),
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
            painter.rect_filled(
                color_rect,
                ui.active_theme().radius.xs,
                if item.color == Color32::PLACEHOLDER {
                    ui.active_theme().ui.text_secondary
                } else {
                    item.color
                },
            );
            painter.rect_stroke(
                color_rect,
                ui.active_theme().radius.xs,
                Stroke::new(
                    DESIGN_TOKENS.stroke.hairline,
                    ui.active_theme().ui.text_muted,
                ),
                egui::StrokeKind::Inside,
            );
            x += config.color_indicator_size + ui.active_theme().spacing.sm;
        }

        if let Some(tool_type) = item.tool_type {
            crate::ui::drawing_toolbar::icons::get_icon(tool_type)
                .as_image_tinted(vec2(16.0, 16.0), icon_color)
                .paint_at(
                    ui,
                    Rect::from_center_size(egui::pos2(x + 10.0, center_y), vec2(16.0, 16.0)),
                );
            x += 24.0;
        }

        // Name
        let name_color = if is_selected {
            ui.visuals().selection.stroke.color
        } else if item.visible {
            ui.style().visuals.text_color()
        } else {
            ui.active_theme().ui.text_muted
        };
        let mut job = egui::text::LayoutJob::simple(
            l10n::text(ui.ctx(), item.full_display_name()),
            egui::FontId::proportional(ui.active_theme().text.label),
            name_color,
            (rect.right() - ui.active_theme().spacing.sm - 40.0 - x).max(0.0),
        );
        job.wrap.max_rows = 1;
        let galley = painter.layout_job(job);
        let pos = egui::pos2(x, center_y - galley.size().y / 2.0);
        painter.galley(pos, galley, name_color);
    } // painter dropped

    // Second pass: interactive elements
    let mut x = rect.left() + indent + ui.active_theme().spacing.sm;

    // Expand/collapse button
    if config.show_expand_chevron && item.source_type == SourceType::Drawing {
        let chevron_rect = Rect::from_center_size(egui::pos2(x + 6.0, center_y), vec2(14.0, 14.0));
        let chevron_response = ui.allocate_rect(chevron_rect, Sense::click());
        let translated_label = l10n::text(ui.ctx(), "Drawing properties");
        chevron_response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                chevron_response.enabled(),
                &translated_label,
            )
        });
        crate::styles::focus::draw_focus_ring(ui, &chevron_response, 0.0);
        ui.ctx()
            .accesskit_node_builder(chevron_response.id, |node| {
                node.set_expanded(state.is_item_expanded(item.id))
            });
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
    let translated_label = l10n::text(ui.ctx(), if item.visible { "Hide" } else { "Show" });
    vis_response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::Button,
            vis_response.enabled(),
            item.visible,
            &translated_label,
        )
    });
    crate::styles::focus::draw_focus_ring(ui, &vis_response, 0.0);
    if ui.is_rect_visible(vis_rect) {
        let icon_color = if is_selected {
            ui.visuals().selection.stroke.color
        } else if vis_response.hovered() {
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
    vis_response.on_hover_text(l10n::text(
        ui.ctx(),
        if item.visible { "Hide" } else { "Show" },
    ));
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
        let translated_label = l10n::text(ui.ctx(), if item.locked { "Unlock" } else { "Lock" });
        lock_response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Button,
                lock_response.enabled(),
                item.locked,
                &translated_label,
            )
        });
        crate::styles::focus::draw_focus_ring(ui, &lock_response, 0.0);
        if ui.is_rect_visible(lock_rect) {
            let icon_color = if is_selected {
                ui.visuals().selection.stroke.color
            } else if lock_response.hovered() {
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
        lock_response.on_hover_text(l10n::text(
            ui.ctx(),
            if item.locked { "Unlock" } else { "Lock" },
        ));
    }

    // Right-side buttons, registered in visual order for keyboard navigation.
    let right_x = rect.right() - ui.active_theme().spacing.sm;
    let button_color = if is_selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().text_color()
    };

    // Settings/Properties button
    let settings_rect =
        Rect::from_center_size(egui::pos2(right_x - 28.0, center_y), vec2(16.0, 16.0));
    let settings_response = ui.allocate_rect(settings_rect, Sense::click());
    let translated_label = l10n::text(ui.ctx(), "Settings");
    settings_response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            settings_response.enabled(),
            &translated_label,
        )
    });
    embedded_icons::SETTINGS
        .as_image_tinted(vec2(16.0, 16.0), button_color)
        .paint_at(ui, settings_rect);
    crate::styles::focus::draw_focus_ring(ui, &settings_response, 0.0);
    if settings_response.clicked() {
        *action = if item.source_type == SourceType::Indicator {
            ObjectTreeAction::OpenIndicatorSettings(item.id)
        } else {
            ObjectTreeAction::OpenProperties(item.id)
        };
    }
    settings_response.on_hover_text(l10n::text(ui.ctx(), "Settings"));

    // Delete button
    if item.source_type != SourceType::DataSource {
        let del_rect =
            Rect::from_center_size(egui::pos2(right_x - 8.0, center_y), vec2(16.0, 16.0));
        let del_response = ui.allocate_rect(del_rect, Sense::click());
        let translated_label = l10n::text(ui.ctx(), "Remove");
        del_response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                del_response.enabled(),
                &translated_label,
            )
        });
        embedded_icons::TRASH
            .as_image_tinted(vec2(16.0, 16.0), button_color)
            .paint_at(ui, del_rect);
        crate::styles::focus::draw_focus_ring(ui, &del_response, 0.0);
        if del_response.clicked() {
            *action = if item.source_type == SourceType::Indicator {
                ObjectTreeAction::RemoveIndicator(item.id)
            } else {
                ObjectTreeAction::Delete(item.id)
            };
        }
        del_response.on_hover_text(l10n::text(ui.ctx(), "Remove"));
    }
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
        .spacing([ui.active_theme().spacing.sm, ui.active_theme().spacing.xs])
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
            ui.label(l10n::format_text(
                ui.ctx(),
                "{0}px",
                &[("0", format!("{}", props.line_width))],
            ));
            ui.end_row();

            // Line style
            ui.hint_label("Style");
            ui.label(l10n::text(ui.ctx(), props.line_style.name()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingToolType;
    use egui::Color32;

    #[test]
    fn selected_rows_use_the_theme_and_end_names_before_the_buttons() {
        let mut custom = crate::theme::Theme::light();
        custom.radius.xs = 6.0;
        for (_, theme) in crate::theme::presets()
            .into_iter()
            .chain([("custom", custom)])
        {
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
                let swatches: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) if rect.rect.size() == vec2(10.0, 10.0) => {
                            Some(rect)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(swatches.len(), 2);
                assert!(swatches.iter().all(|rect| rect.corner_radius == egui::CornerRadius::from(theme.radius.xs)));
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
                        <= row.right() - ctx.active_theme().spacing.sm - 40.0
                );
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn tab_visits_the_row_and_its_actions_in_visual_order_with_visible_focus() {
        for (_, theme) in crate::theme::presets() {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut item = SourceItem::drawing(1, DrawingToolType::TrendLine, Color32::RED);
            let mut state = ObjectTreeState::default();
            let mut frame = |events| {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(400.0, 200.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        render_item_row(ui, &mut item, &ObjectTreeConfig::default(), &mut state, 0);
                    },
                );
                let rings: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) if rect.stroke.width == 2.0 => Some(rect.rect),
                        _ => None,
                    })
                    .collect();
                output.drop_without_applying_deltas();
                let focused = ctx
                    .memory(|m| m.focused())
                    .and_then(|id| ctx.read_response(id));
                (rings, focused)
            };
            assert!(frame(vec![]).0.is_empty());
            let mut right = 0.0;
            for index in 0..6 {
                let (rings, focused) = frame(vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
                let focused = focused.unwrap();
                assert_eq!(rings, vec![focused.rect.expand(2.0)]);
                if index > 1 {
                    assert!(focused.rect.left() >= right, "Tab moved backwards");
                }
                right = focused.rect.right();
            }
        }
    }
}
