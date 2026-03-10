//! Trend Lines category for left toolbar.
//!
//! Contains three sub-sections:
//! - LINES: Trend Line, Ray, Info Line, Extended Line, Trend Angle, etc.
//! - CHANNELS: Parallel Channel, Regression Trend, Flat Top/Bottom, Disjoint Channel
//! - PITCHFORKS: Pitchfork, Schiff Pitchfork, Modified Schiff, Inside Pitchfork

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};
use crate::styles::typography;
use egui::{Color32, Pos2, Rect, Ui, Vec2};

use super::{DrawingToolbarAction, ToolCategory};
use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::drawing_toolbar::icons as toolbar_icons;

/// Trend Lines category implementation
pub struct TrendLinesCategory;

impl ToolCategory for TrendLinesCategory {
    fn name(&self) -> &'static str {
        "Lines"
    }

    fn tooltip(&self) -> &'static str {
        "Trend line tools"
    }

    fn icon(&self) -> &'static Icon {
        &icons::TREND_LINE
    }

    fn curr_tool_icon(&self, selected: Option<DrawingToolType>) -> &'static Icon {
        if let Some(tool) = selected
            && self.contains(tool)
        {
            return toolbar_icons::get_icon(tool);
        }
        &icons::TREND_LINE
    }

    fn all_tools(&self) -> Vec<DrawingToolType> {
        self.sections()
            .into_iter()
            .flat_map(|(_, tools)| tools)
            .collect()
    }

    fn sections(&self) -> Vec<(&'static str, Vec<DrawingToolType>)> {
        vec![
            (
                "LINES",
                vec![
                    DrawingToolType::TrendLine,
                    DrawingToolType::Ray,
                    DrawingToolType::InfoLine,
                    DrawingToolType::ExtendedLine,
                    DrawingToolType::TrendAngle,
                    DrawingToolType::HorizontalLine,
                    DrawingToolType::HorizontalRay,
                    DrawingToolType::VerticalLine,
                    DrawingToolType::CrossLine,
                ],
            ),
            (
                "CHANNELS",
                vec![
                    DrawingToolType::ParallelChannel,
                    DrawingToolType::RegressionTrend,
                    DrawingToolType::FlatTopBottom,
                    DrawingToolType::DisjointChannel,
                ],
            ),
            (
                "PITCHFORKS",
                vec![
                    DrawingToolType::Pitchfork,
                    DrawingToolType::SchiffPitchfork,
                    DrawingToolType::ModifiedSchiffPitchfork,
                    DrawingToolType::InsidePitchfork,
                ],
            ),
        ]
    }

    fn render_submenu(
        &self,
        ui: &mut Ui,
        anchor_rect: Rect,
        sel_tool: Option<DrawingToolType>,
        favorites: &[DrawingToolType],
    ) -> DrawingToolbarAction {
        render_tool_submenu(
            ui,
            anchor_rect,
            self.sections(),
            sel_tool,
            favorites,
            "trend_lines_submenu",
        )
    }
}

/// Keyboard shortcuts for trend line tools
pub fn get_shortcut(tool: DrawingToolType) -> Option<&'static str> {
    match tool {
        DrawingToolType::TrendLine => Some("Alt T"),
        DrawingToolType::HorizontalLine => Some("Alt H"),
        DrawingToolType::HorizontalRay => Some("Alt J"),
        DrawingToolType::VerticalLine => Some("Alt V"),
        DrawingToolType::CrossLine => Some("Alt C"),
        _ => None,
    }
}

/// Common submenu renderer for tool categories
pub fn render_tool_submenu(
    ui: &mut Ui,
    anchor_rect: Rect,
    sections: Vec<(&'static str, Vec<DrawingToolType>)>,
    sel_tool: Option<DrawingToolType>,
    favorites: &[DrawingToolType],
    id: &str,
) -> DrawingToolbarAction {
    let mut action = DrawingToolbarAction::None;

    let submenu_pos = Pos2::new(anchor_rect.right() + 4.0, anchor_rect.min.y);

    egui::Area::new(egui::Id::new(id))
        .fixed_pos(submenu_pos)
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(ui.style().visuals.window_fill)
                .stroke(egui::Stroke::new(
                    1.0,
                    ui.style().visuals.widgets.noninteractive.bg_stroke.color,
                ))
                .corner_radius(DESIGN_TOKENS.rounding.md)
                .shadow(egui::Shadow {
                    spread: 2,
                    blur: 8,
                    offset: [2, 2],
                    color: Color32::from_black_alpha(40),
                })
                .inner_margin(egui::Margin::same(DESIGN_TOKENS.spacing.sm as i8))
                .show(ui, |ui| {
                    let submenu_width = DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_width_xl;
                    ui.set_min_width(submenu_width);

                    for (section_idx, (section_name, tools)) in sections.iter().enumerate() {
                        // Section header
                        if section_idx > 0 {
                            // Separator before section (except first)
                            ui.space_sm();
                            let sep_rect = ui.allocate_space(Vec2::new(submenu_width, DESIGN_TOKENS.spacing.hairline)).1;
                            ui.painter().rect_filled(
                                sep_rect,
                                0.0,
                                ui.style().visuals.widgets.noninteractive.bg_stroke.color,
                            );
                            ui.space_sm();
                        }

                        // Section title
                        ui.painter().text(
                            Pos2::new(
                                ui.available_rect_before_wrap().min.x + DESIGN_TOKENS.spacing.lg,
                                ui.available_rect_before_wrap().min.y + DESIGN_TOKENS.spacing.lg,
                            ),
                            egui::Align2::LEFT_CENTER,
                            *section_name,
                            egui::FontId::proportional(typography::XS),
                            ui.style().visuals.widgets.noninteractive.fg_stroke.color,
                        );
                        ui.space_xxl();

                        // Tools in section
                        for tool in tools {
                            let is_sel = sel_tool == Some(*tool);
                            let is_favorite = favorites.contains(tool);

                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(submenu_width, DESIGN_TOKENS.sizing.dialog.submenu_item_height),
                                egui::Sense::click(),
                            );

                            // Background
                            if is_sel {
                                ui.painter().rect_filled(rect, DESIGN_TOKENS.rounding.xs, DESIGN_TOKENS.semantic.extended.accent);
                            } else if response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    DESIGN_TOKENS.rounding.xs,
                                    ui.style().visuals.widgets.hovered.bg_fill,
                                );
                            }

                            // Icon
                            let icon_size = DESIGN_TOKENS.sizing.icon_md;
                            let icon_rect = Rect::from_min_size(
                                Pos2::new(rect.min.x + DESIGN_TOKENS.spacing.lg, rect.center().y - icon_size / 2.0),
                                Vec2::splat(icon_size),
                            );
                            crate::ui::drawing_toolbar::components::svg_helpers::render_svg_at_rect_themed(
                                ui,
                                toolbar_icons::get_icon(*tool),
                                icon_rect,
                                response.hovered(),
                                is_sel,
                            );

                            // Name
                            let text_color = if is_sel {
                                DESIGN_TOKENS.semantic.ui.text_light
                            } else {
                                ui.style().visuals.text_color()
                            };
                            ui.painter().text(
                                Pos2::new(rect.min.x + DESIGN_TOKENS.sizing.drawing_toolbar_ext.name_offset_inactive, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                tool.as_str(),
                                egui::FontId::proportional(typography::SM_MD),
                                text_color,
                            );

                            // Shortcut
                            if let Some(shortcut) = get_shortcut(*tool) {
                                let shortcut_color = if is_sel {
                                    DESIGN_TOKENS.semantic.ui.text_light.gamma_multiply(0.7)
                                } else {
                                    ui.style().visuals.widgets.noninteractive.fg_stroke.color
                                };
                                ui.painter().text(
                                    Pos2::new(rect.right() - DESIGN_TOKENS.spacing.section_lg, rect.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    shortcut,
                                    egui::FontId::proportional(typography::XS),
                                    shortcut_color,
                                );
                            }

                            // Favorite star (on hover)
                            if response.hovered() || is_favorite {
                                let star_rect = Rect::from_min_size(
                                    Pos2::new(rect.right() - DESIGN_TOKENS.spacing.xxxl, rect.center().y - DESIGN_TOKENS.spacing.lg),
                                    Vec2::splat(DESIGN_TOKENS.sizing.icon_sm),
                                );
                                let star_color = if is_favorite {
                                    DESIGN_TOKENS.semantic.extended.favorite_gold
                                } else {
                                    ui.style().visuals.widgets.noninteractive.fg_stroke.color
                                };
                                ui.painter().text(
                                    star_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    if is_favorite { "*" } else { "o" },
                                    egui::FontId::proportional(typography::SM_MD),
                                    star_color,
                                );
                            }

                            if response.clicked() {
                                action = DrawingToolbarAction::SelectTool(*tool);
                            }
                        }
                    }
                });
        });

    action
}
