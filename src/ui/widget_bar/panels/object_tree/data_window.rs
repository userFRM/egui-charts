//! Data Window component
//!
//! Displays real-time OHLCV data at the cursor position.
//! Collapsible section at the top of the Object Tree panel.

use egui::{RichText, Ui};

use crate::ext::HasDesignTokens;

use super::types::DataWindowInfo;
use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;

/// Render the Data Window section
///
/// Shows OHLCV data at cursor position in a compact grid layout.
pub fn show_data_window(ui: &mut Ui, data: &DataWindowInfo, expanded: bool) -> bool {
    let mut is_expanded = expanded;

    // Header with collapse button
    ui.horizontal(|ui| {
        // Collapse/expand button
        let icon = if is_expanded { "v" } else { ">" };
        if ui.small_button(icon).clicked() {
            is_expanded = !is_expanded;
        }

        // Title
        ui.strong_label("Data Window");

        // Symbol badge
        ui.right_aligned(|ui| {
            if !data.symbol.is_empty() {
                ui.label(
                    RichText::new(&data.symbol)
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            }
        });
    });

    // Content (if expanded)
    if is_expanded {
        ui.indent("data_window_content", |ui| {
            ui.space_xs();

            // Timestamp and timeframe row
            ui.horizontal(|ui| {
                if !data.timestamp.is_empty() {
                    ui.hint_label(&data.timestamp);
                }
                if !data.timeframe.is_empty() {
                    ui.hint_label(&data.timeframe);
                }
            });

            ui.space_xs();

            // OHLCV grid
            show_ohlcv_grid(ui, data);

            ui.space_sm();
        });
    }

    is_expanded
}

/// Render the OHLCV grid
fn show_ohlcv_grid(ui: &mut Ui, data: &DataWindowInfo) {
    let change_color = if data.is_bullish() {
        ui.bullish_color()
    } else {
        ui.bearish_color()
    };

    egui::Grid::new("ohlcv_grid")
        .num_columns(4)
        .spacing([DESIGN_TOKENS.spacing.md, DESIGN_TOKENS.spacing.xs])
        .min_col_width(50.0)
        .show(ui, |ui| {
            // Row 1: Open, High
            ui.hint_label("O");
            ui.label(format_price(data.open));
            ui.hint_label("H");
            ui.label(format_price(data.high));
            ui.end_row();

            // Row 2: Low, Close
            ui.hint_label("L");
            ui.label(format_price(data.low));
            ui.hint_label("C");
            ui.label(RichText::new(format_price(data.close)).color(change_color));
            ui.end_row();

            // Row 3: Volume, Change
            ui.hint_label("Vol");
            ui.label(format_volume(data.volume));
            ui.hint_label("Chg");
            ui.label(
                RichText::new(format!(
                    "{:+.2} ({:+.2}%)",
                    data.change, data.change_percent
                ))
                .color(change_color)
                .small(),
            );
            ui.end_row();
        });
}

/// Format price with appropriate precision
fn format_price(price: f64) -> String {
    if price == 0.0 {
        return "—".to_string();
    }

    if price >= 10000.0 {
        format!("{:.0}", price)
    } else if price >= 1000.0 {
        format!("{:.1}", price)
    } else if price >= 10.0 {
        format!("{:.2}", price)
    } else if price >= 1.0 {
        format!("{:.4}", price)
    } else if price >= 0.001 {
        format!("{:.6}", price)
    } else {
        format!("{:.8}", price)
    }
}

/// Format volume with K/M/B suffixes
fn format_volume(volume: f64) -> String {
    if volume == 0.0 {
        return "—".to_string();
    }

    if volume >= 1_000_000_000.0 {
        format!("{:.2}B", volume / 1_000_000_000.0)
    } else if volume >= 1_000_000.0 {
        format!("{:.2}M", volume / 1_000_000.0)
    } else if volume >= 1_000.0 {
        format!("{:.2}K", volume / 1_000.0)
    } else {
        format!("{:.0}", volume)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_price() {
        assert_eq!(format_price(50000.0), "50000");
        assert_eq!(format_price(1234.5), "1234.5");
        assert_eq!(format_price(99.99), "99.99");
        assert_eq!(format_price(1.2345), "1.2345");
        assert_eq!(format_price(0.001234), "0.001234");
    }

    #[test]
    fn test_format_volume() {
        assert_eq!(format_volume(1_500_000_000.0), "1.50B");
        assert_eq!(format_volume(2_500_000.0), "2.50M");
        assert_eq!(format_volume(3_500.0), "3.50K");
        assert_eq!(format_volume(500.0), "500");
    }
}
