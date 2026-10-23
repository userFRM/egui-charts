//! Data Window component
//!
//! Displays real-time OHLCV data at the cursor position.
//! Collapsible section at the top of the Object Tree panel.

use egui::{RichText, Ui};

use crate::icons::icons;
use crate::theme::ThemeExt;
use crate::ui_kit::IconButton;

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
        let icon = if is_expanded {
            &icons::CHEVRON_DOWN
        } else {
            &icons::CHEVRON_RIGHT
        };
        if IconButton::new(icon)
            .tooltip("Toggle Data Window")
            .show(ui)
            .clicked()
        {
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

            egui::Grid::new("bar_time").num_columns(2).show(ui, |ui| {
                if !data.timestamp.is_empty() {
                    ui.hint_label("Time");
                    ui.label(&data.timestamp);
                    ui.end_row();
                }
                if !data.timeframe.is_empty() {
                    ui.hint_label("Interval");
                    ui.label(&data.timeframe);
                    ui.end_row();
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
        ui.active_theme().chart.candle_up
    } else {
        ui.active_theme().chart.candle_down
    };

    egui::Grid::new("ohlcv_grid")
        .num_columns(2)
        .spacing([DESIGN_TOKENS.spacing.md, DESIGN_TOKENS.spacing.xs])
        .min_col_width(50.0)
        .show(ui, |ui| {
            ui.hint_label("O");
            ui.label(format_price(data.open, data.price_decimals));
            ui.end_row();
            ui.hint_label("H");
            ui.label(format_price(data.high, data.price_decimals));
            ui.end_row();

            ui.hint_label("L");
            ui.label(format_price(data.low, data.price_decimals));
            ui.end_row();
            ui.hint_label("C");
            ui.label(
                RichText::new(format_price(data.close, data.price_decimals)).color(change_color),
            );
            ui.end_row();

            ui.hint_label("Vol");
            ui.label(format_volume(data.volume));
            ui.end_row();
            ui.hint_label("Chg");
            ui.label(
                RichText::new(format!(
                    "{:+.*} ({:+.2}%)",
                    usize::from(data.price_decimals.unwrap_or(2)),
                    data.change,
                    data.change_percent
                ))
                .color(change_color)
                .small(),
            );
            ui.end_row();
        });
}

/// Format price with decimals chosen by its magnitude, so a negative price
/// keeps the decimals of the positive price of the same size
fn format_price(price: f64, decimals: Option<u8>) -> String {
    if !price.is_finite() {
        return "\u{2014}".to_owned();
    }
    if let Some(decimals) = decimals {
        return format!("{:.*}", usize::from(decimals), price);
    }
    if price == 0.0 {
        return "\u{2014}".to_string();
    }

    let magnitude = price.abs();
    if magnitude >= 10000.0 {
        format!("{price:.0}")
    } else if magnitude >= 1000.0 {
        format!("{price:.1}")
    } else if magnitude >= 10.0 {
        format!("{price:.2}")
    } else if magnitude >= 1.0 {
        format!("{price:.4}")
    } else if magnitude >= 0.001 {
        format!("{price:.6}")
    } else {
        format!("{price:.8}")
    }
}

/// Format volume with K/M/B suffixes
fn format_volume(volume: f64) -> String {
    if volume == 0.0 {
        return "\u{2014}".to_string();
    }

    if volume >= 1_000_000_000.0 {
        format!("{:.2}B", volume / 1_000_000_000.0)
    } else if volume >= 1_000_000.0 {
        format!("{:.2}M", volume / 1_000_000.0)
    } else if volume >= 1_000.0 {
        format!("{:.2}K", volume / 1_000.0)
    } else {
        format!("{volume:.0}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_price() {
        assert_eq!(format_price(50000.0, None), "50000");
        assert_eq!(format_price(1234.5, None), "1234.5");
        assert_eq!(format_price(99.99, None), "99.99");
        assert_eq!(format_price(1.2345, None), "1.2345");
        assert_eq!(format_price(0.001234, None), "0.001234");
        assert_eq!(format_price(-37.63, None), "-37.63");
        for (price, decimals, expected) in [
            (50000.25, 2, "50000.25"),
            (-37.625, 3, "-37.625"),
            (0.0, 2, "0.00"),
            (1.2345, 4, "1.2345"),
        ] {
            assert_eq!(format_price(price, Some(decimals)), expected);
        }
    }

    #[test]
    fn test_format_volume() {
        assert_eq!(format_volume(1_500_000_000.0), "1.50B");
        assert_eq!(format_volume(2_500_000.0), "2.50M");
        assert_eq!(format_volume(3_500.0), "3.50K");
        assert_eq!(format_volume(500.0), "500");
    }
}
