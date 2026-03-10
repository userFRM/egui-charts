//! Symbol tab - chart type, price source, and line/candle style settings.

use crate::chart::series::SeriesSettings;
use crate::ext::UiExt;
use crate::model::PriceSource;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::FormGrid;
use egui::{Color32, Ui};

/// Symbol tab renderer - shows chart type and style settings
pub struct SymbolTab;

impl SymbolTab {
    /// Show the symbol tab content
    pub fn show(ui: &mut Ui, settings: &mut SeriesSettings) {
        ui.space_md();

        // Line style section
        ui.label("Line");
        ui.space_sm();

        FormGrid::new("symbol_line_grid").show(ui, |ui| {
            // Price source
            ui.label("Price source");
            Self::price_source_dropdown(ui, &mut settings.price_source);
            ui.end_row();

            // Line style
            ui.label("Line");
            Self::line_color_picker(ui, &mut settings.bullish_color);
            ui.end_row();
        });

        ui.space_xl();
        ui.separator();
        ui.space_xl();

        // Candle colors section (shown for candlestick chart types)
        ui.label("Candle colors");
        ui.space_sm();

        FormGrid::new("symbol_candle_grid").show(ui, |ui| {
            Self::candle_colors_section(ui, settings);
        });

        ui.space_xl();
        ui.separator();
        ui.space_xl();

        // Visibility section
        ui.label("Visibility");
        ui.space_sm();

        FormGrid::new("symbol_visibility_grid").show(ui, |ui| {
            Self::visibility_section(ui, settings);
        });
    }

    fn visibility_section(ui: &mut Ui, settings: &mut SeriesSettings) {
        // Show on all timeframes
        ui.label("Show on all timeframes");
        ui.checkbox(&mut settings.visible_all_timeframes, "");
        ui.end_row();

        // Individual timeframe toggles (only when not showing on all)
        if !settings.visible_all_timeframes {
            ui.label("Visible timeframes");
            ui.vertical(|ui| {
                let timeframes = ["1m", "5m", "15m", "1h", "4h", "1D", "1W", "1M"];
                for tf in timeframes {
                    let tf_str = tf.to_string();
                    let mut is_visible = settings.visible_timeframes.contains(&tf_str);
                    if ui.checkbox(&mut is_visible, tf).changed() {
                        if is_visible {
                            if !settings.visible_timeframes.contains(&tf_str) {
                                settings.visible_timeframes.push(tf_str);
                            }
                        } else {
                            settings.visible_timeframes.retain(|t| t != tf);
                        }
                    }
                }
            });
            ui.end_row();
        }
    }

    fn price_source_dropdown(ui: &mut Ui, source: &mut PriceSource) {
        ui.combo_select("price_source", source, PriceSource::all(), |v| {
            v.label().to_string()
        });
    }

    fn line_color_picker(ui: &mut Ui, color: &mut Color32) {
        ui.horizontal(|ui| {
            let mut color_arr = color.to_array();
            if ui
                .color_edit_button_srgba_unmultiplied(&mut color_arr)
                .changed()
            {
                *color = Color32::from_rgba_unmultiplied(
                    color_arr[0],
                    color_arr[1],
                    color_arr[2],
                    color_arr[3],
                );
            }
        });
    }

    fn candle_colors_section(ui: &mut Ui, settings: &mut SeriesSettings) {
        // Bullish body color
        ui.label("Body (up)");
        Self::color_picker(ui, &mut settings.bullish_color);
        ui.end_row();

        // Bearish body color
        ui.label("Body (down)");
        Self::color_picker(ui, &mut settings.bearish_color);
        ui.end_row();

        // Border colors
        ui.label("Border (up)");
        Self::optional_color_picker(
            ui,
            &mut settings.bullish_border_color,
            settings.bullish_color,
        );
        ui.end_row();

        ui.label("Border (down)");
        Self::optional_color_picker(
            ui,
            &mut settings.bearish_border_color,
            settings.bearish_color,
        );
        ui.end_row();

        // Wick colors
        ui.label("Wick (up)");
        Self::optional_color_picker(ui, &mut settings.bullish_wick_color, settings.bullish_color);
        ui.end_row();

        ui.label("Wick (down)");
        Self::optional_color_picker(ui, &mut settings.bearish_wick_color, settings.bearish_color);
        ui.end_row();
    }

    fn color_picker(ui: &mut Ui, color: &mut Color32) {
        let mut color_arr = color.to_array();
        if ui
            .color_edit_button_srgba_unmultiplied(&mut color_arr)
            .changed()
        {
            *color = Color32::from_rgba_unmultiplied(
                color_arr[0],
                color_arr[1],
                color_arr[2],
                color_arr[3],
            );
        }
    }

    fn optional_color_picker(ui: &mut Ui, color: &mut Option<Color32>, default: Color32) {
        ui.horizontal(|ui| {
            let mut use_custom = color.is_some();
            if ui.checkbox(&mut use_custom, "").changed() {
                *color = if use_custom { Some(default) } else { None };
            }

            if let Some(c) = color {
                let mut color_arr = c.to_array();
                if ui
                    .color_edit_button_srgba_unmultiplied(&mut color_arr)
                    .changed()
                {
                    *c = Color32::from_rgba_unmultiplied(
                        color_arr[0],
                        color_arr[1],
                        color_arr[2],
                        color_arr[3],
                    );
                }
            } else {
                let preview = egui::Button::new("")
                    .fill(default)
                    .min_size(egui::Vec2::splat(
                        DESIGN_TOKENS.sizing.settings_dialog.checkbox_size,
                    ));
                ui.add_enabled(false, preview)
                    .on_hover_text("Using body color");
            }
        });
    }
}
