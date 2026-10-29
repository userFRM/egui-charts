//! Canvas tab of the chart settings dialog.

use super::super::SettingsDialog;
use super::{styled_dropdown, styled_text_input};
use crate::localization as l10n;
use crate::model::LineStyle;
use crate::theme::Theme;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::ColorPicker;
use crate::widget::settings::{BackgroundType, ButtonVisibility, GridLinesMode, WatermarkMode};
use egui::Ui;

impl SettingsDialog {
    pub(in super::super) fn draw_canvas_tab(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal(|ui| {
            ui.add_space(0.0);
            ui.vertical(|ui| {
                // CHART BASIC STYLES
                self.draw_section_header(ui, "CHART BASIC STYLES", theme);

                // Background
                self.draw_setting_row_with_controls(ui, "Background", theme, |ui, dialog| {
                    // Type dropdown
                    styled_dropdown(
                        ui,
                        "bg_type",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        dialog.settings.chart_basic_styles.background_type.name(),
                        |ui| {
                            for bt in BackgroundType::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.chart_basic_styles.background_type,
                                    *bt,
                                    l10n::text(ui.ctx(), bt.name()),
                                );
                            }
                        },
                    );

                    ui.add_space(ui.active_theme().spacing.lg);

                    // Color swatch(es)
                    match dialog.settings.chart_basic_styles.background_type {
                        BackgroundType::Solid => {
                            if let Some(color) = ColorPicker::new(
                                &mut dialog.color_picker_states.background,
                                "bg_color",
                            )
                            .show(ui)
                            .inner
                            {
                                dialog.settings.chart_basic_styles.background_color = color;
                            }
                        }
                        _ => {
                            if let Some(color) = ColorPicker::new(
                                &mut dialog.color_picker_states.background_gradient_top,
                                "bg_grad_top",
                            )
                            .show(ui)
                            .inner
                            {
                                dialog.settings.chart_basic_styles.background_gradient_top = color;
                            }
                            if let Some(color) = ColorPicker::new(
                                &mut dialog.color_picker_states.background_gradient_bottom,
                                "bg_grad_bot",
                            )
                            .show(ui)
                            .inner
                            {
                                dialog
                                    .settings
                                    .chart_basic_styles
                                    .background_gradient_bottom = color;
                            }
                        }
                    }
                });

                // Grid lines
                self.draw_setting_row_with_controls(ui, "Grid lines", theme, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "grid_mode",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        dialog.settings.grid_lines.mode.name(),
                        |ui| {
                            for mode in GridLinesMode::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.grid_lines.mode,
                                    *mode,
                                    l10n::text(ui.ctx(), mode.name()),
                                );
                            }
                        },
                    );

                    // Color picker(s) based on mode
                    if matches!(
                        dialog.settings.grid_lines.mode,
                        GridLinesMode::Horizontal | GridLinesMode::Both
                    ) {
                        ui.add_space(ui.active_theme().spacing.lg);
                        if let Some(color) =
                            ColorPicker::new(&mut dialog.color_picker_states.grid_h, "grid_h")
                                .show(ui)
                                .inner
                        {
                            dialog.settings.grid_lines.horizontal_color = color;
                        }
                    }
                    if matches!(
                        dialog.settings.grid_lines.mode,
                        GridLinesMode::Vertical | GridLinesMode::Both
                    ) && let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.grid_v, "grid_v")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.grid_lines.vertical_color = color;
                    }
                });

                // Crosshair
                self.draw_setting_row_with_controls(ui, "Crosshair", theme, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.crosshair, "crosshair")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.crosshair.color = color;
                    }

                    ui.add_space(ui.active_theme().spacing.lg);

                    // Line style with dashes preview
                    styled_dropdown(
                        ui,
                        "crosshair_style",
                        DESIGN_TOKENS.sizing.settings_dialog.small_dropdown_width,
                        theme.ui.text,
                        line_style_preview(dialog.settings.crosshair.line_style),
                        |ui| {
                            for ls in LineStyle::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.crosshair.line_style,
                                    *ls,
                                    l10n::text(ui.ctx(), line_style_preview(*ls)),
                                );
                            }
                        },
                    );

                    ui.add_space(ui.active_theme().spacing.lg);

                    // Line width slider
                    let mut lw = dialog.settings.crosshair.line_width;
                    ui.add(
                        egui::DragValue::new(&mut lw)
                            .range(1.0..=4.0)
                            .speed(0.5)
                            .suffix(l10n::text(ui.ctx(), " px")),
                    );
                    dialog.settings.crosshair.line_width = lw;
                });

                ui.add_space(ui.active_theme().spacing.xxl);

                // WATERMARK
                self.draw_section_header(ui, "WATERMARK", theme);

                self.draw_setting_row_with_controls(ui, "Mode", theme, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "watermark_mode",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        dialog.settings.watermark.mode.name(),
                        |ui| {
                            for wm in &[
                                WatermarkMode::ReplayMode,
                                WatermarkMode::Symbol,
                                WatermarkMode::SymbolAndDescription,
                            ] {
                                ui.selectable_value(
                                    &mut dialog.settings.watermark.mode,
                                    *wm,
                                    l10n::text(ui.ctx(), wm.name()),
                                );
                            }
                        },
                    );

                    ui.add_space(ui.active_theme().spacing.lg);

                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.watermark, "watermark")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.watermark.color = color;
                    }
                });

                ui.add_space(ui.active_theme().spacing.xxl);

                // SCALES APPEARANCE
                self.draw_section_header(ui, "SCALES APPEARANCE", theme);

                self.draw_setting_row_with_controls(ui, "Text", theme, |ui, dialog| {
                    if let Some(color) =
                        ColorPicker::new(&mut dialog.color_picker_states.scales_text, "scales_text")
                            .show(ui)
                            .inner
                    {
                        dialog.settings.scales_appearance.text_color = color;
                    }
                });

                self.draw_setting_row_with_controls(ui, "Lines", theme, |ui, dialog| {
                    if let Some(color) = ColorPicker::new(
                        &mut dialog.color_picker_states.scales_lines,
                        "scales_lines",
                    )
                    .show(ui)
                    .inner
                    {
                        dialog.settings.scales_appearance.lines_color = color;
                    }
                });

                self.draw_setting_row_with_controls(ui, "Font size", theme, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "scales_font_size",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        &dialog.settings.scales_appearance.font_size.to_string(),
                        |ui| {
                            for size in [10, 11, 12, 14, 16, 20] {
                                ui.selectable_value(
                                    &mut dialog.settings.scales_appearance.font_size,
                                    size,
                                    size.to_string(),
                                );
                            }
                        },
                    );
                });

                ui.add_space(ui.active_theme().spacing.xxl);

                // MARGINS (Pixel input fields)
                self.draw_section_header(ui, "MARGINS", theme);

                self.draw_setting_row_with_controls(ui, "Top margin (%)", theme, |ui, dialog| {
                    styled_text_input(
                        ui,
                        &mut dialog.margin_top_str,
                        DESIGN_TOKENS.sizing.settings_dialog.input_width_margin,
                    );
                    if let Ok(v) = dialog.margin_top_str.parse::<f32>() {
                        dialog.settings.margins.top_percent = v;
                    }
                });

                self.draw_setting_row_with_controls(
                    ui,
                    "Bottom margin (%)",
                    theme,
                    |ui, dialog| {
                        styled_text_input(
                            ui,
                            &mut dialog.margin_bottom_str,
                            DESIGN_TOKENS.sizing.settings_dialog.input_width_margin,
                        );
                        if let Ok(v) = dialog.margin_bottom_str.parse::<f32>() {
                            dialog.settings.margins.bottom_percent = v;
                        }
                    },
                );

                self.draw_setting_row_with_controls(
                    ui,
                    "Right margin (bars)",
                    theme,
                    |ui, dialog| {
                        styled_text_input(
                            ui,
                            &mut dialog.margin_right_str,
                            DESIGN_TOKENS.sizing.settings_dialog.input_width_margin,
                        );
                        if let Ok(v) = dialog.margin_right_str.parse::<u32>() {
                            dialog.settings.margins.right_bars = v;
                        }
                    },
                );

                ui.add_space(ui.active_theme().spacing.xxl);

                // NAVIGATION BUTTONS
                self.draw_section_header(ui, "NAVIGATION BUTTONS", theme);

                self.draw_setting_row_with_controls(ui, "Visibility", theme, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "nav_visibility",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        dialog.settings.btns.navigation.name(),
                        |ui| {
                            for vis in ButtonVisibility::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.btns.navigation,
                                    *vis,
                                    l10n::text(ui.ctx(), vis.name()),
                                );
                            }
                        },
                    );
                });

                self.draw_setting_row_with_controls(ui, "Pane buttons", theme, |ui, dialog| {
                    styled_dropdown(
                        ui,
                        "pane_visibility",
                        DESIGN_TOKENS.sizing.settings_dialog.dropdown_width,
                        theme.ui.text,
                        dialog.settings.btns.pane.name(),
                        |ui| {
                            for vis in ButtonVisibility::all() {
                                ui.selectable_value(
                                    &mut dialog.settings.btns.pane,
                                    *vis,
                                    l10n::text(ui.ctx(), vis.name()),
                                );
                            }
                        },
                    );
                });
            });
        });
    }
}

fn line_style_preview(ls: LineStyle) -> &'static str {
    match ls {
        LineStyle::Solid => "Solid",
        LineStyle::Dashed => "\u{2013} \u{2013} \u{2013}",
        LineStyle::Dotted => "· · · ·",
        LineStyle::LargeDashed => "\u{2014}  \u{2014}  \u{2014}",
        LineStyle::SparseDotted => "·   ·   ·",
    }
}
