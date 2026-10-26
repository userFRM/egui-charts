//! Data Window component
//!
//! Displays real-time OHLCV data at the cursor position.
//! Collapsible section at the top of the Object Tree panel.

use egui::{RichText, Ui};

use crate::icons::icons;
use crate::scales::{PriceFormatter, VolumeFormatter};
use crate::theme::ThemeExt;
use crate::ui_kit::IconButton;

use super::types::DataWindowInfo;
use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;

impl DataWindowInfo {
    /// Populates the readout from the hovered rendered bar, falling back to the latest bar with a
    /// value.
    /// Uses the chart's time formatter, instrument precision and visible study plots.
    /// Call after [`crate::widget::Chart::show`] so hover and study values describe the current frame.
    pub fn from_chart(chart: &crate::widget::Chart) -> Self {
        let bars = chart.state().shown();
        let index = chart
            .get_hover_bar_idx()
            .filter(|index| index.is_finite() && index.round() >= 0.0)
            .map(|index| index.round() as usize)
            .filter(|index| *index < bars.len())
            .unwrap_or_else(|| {
                bars.iter()
                    .rposition(|bar| !bar.is_whitespace())
                    .unwrap_or(bars.len().saturating_sub(1))
            });
        let mut info = Self::new(chart.symbol())
            .with_timeframe(chart.timeframe().map(|tf| tf.as_str()).unwrap_or_default());
        if let Some(instrument) = chart.instrument() {
            info = info.with_instrument(instrument);
        }
        if let Some(bar) = bars.get(index) {
            info = info
                .with_ohlcv(bar.open, bar.high, bar.low, bar.close, bar.volume)
                .with_timestamp(chart.format_time(bar.time))
                .with_prev_close(
                    crate::model::last_with_value(&bars[..index]).map_or(bar.open, |bar| bar.close),
                );
            info.studies = chart.study_values_at(index);
        }
        info
    }
}

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
            if !data.studies.is_empty() {
                ui.space_sm();
                egui::Grid::new("study_values")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for study in &data.studies {
                            ui.hint_label(study.label());
                            ui.label(RichText::new(study.formatted_value()).color(study.color));
                            ui.end_row();
                        }
                    });
            }

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
            ui.label(data.volume_precision.map_or_else(
                || format_volume(data.volume),
                |decimals| {
                    VolumeFormatter {
                        precision: usize::from(decimals),
                    }
                    .format(data.volume)
                },
            ));
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
pub(crate) fn format_price(price: f64, decimals: Option<u8>) -> String {
    if !price.is_finite() {
        return "\u{2014}".to_owned();
    }
    let decimals = decimals.unwrap_or_else(|| price_decimals(price));
    format!("{:.*}", usize::from(decimals), price)
}

/// Decimals of a price label without instrument precision, chosen by the price's magnitude
pub(crate) fn price_decimals(price: f64) -> u8 {
    match price.abs() {
        m if m >= 10000.0 => 0,
        m if m >= 1000.0 => 1,
        m if m >= 10.0 => 2,
        m if m >= 1.0 => 4,
        m if m >= 0.001 => 6,
        _ => 8,
    }
}

/// Format volume with K/M/B suffixes
fn format_volume(volume: f64) -> String {
    if volume == 0.0 {
        return "\u{2014}".to_string();
    }

    crate::studies::StudyFormat::Volume.format(volume, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_price() {
        assert_eq!(format_price(0.0, None), "0.00000000");
        assert_eq!(format_price(f64::NAN, Some(2)), "\u{2014}");
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
    fn instrument_precision_formats_prices_change_and_volume() {
        let mut instrument = crate::model::Instrument::new("FX").with_volume_decimals(3);
        instrument.tick_size = Some(0.00001);
        let data = DataWindowInfo::new("FX")
            .with_instrument(&instrument)
            .with_ohlcv(0.0, 1.23456, 0.0, 1.23456, 1234.0)
            .with_prev_close(1.23455);
        assert_eq!(data.price_decimals, Some(5));
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show_ohlcv_grid(ui, &data);
        });
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect();
        for expected in ["0.00000", "1.23456", "+0.00001 (+0.00%)", "1.234K"] {
            assert!(texts.contains(&expected), "{expected}: {texts:?}");
        }
        output.drop_without_applying_deltas();
    }

    #[test]
    fn test_format_volume() {
        use crate::studies::StudyFormat::*;
        for (format, value, precision, expected) in [
            (Volume, 1_500_000_000.0, None, "1.50B"),
            (Volume, 2_500_000.0, None, "2.50M"),
            (Volume, 3_500.0, None, "3.50K"),
            (Volume, 500.0, None, "500"),
            (Volume, -12_345_678.0, None, "-12.35M"),
            (Volume, 0.0, Some(2), "0.00"),
            (Volume, 1_234.0, Some(3), "1.234K"),
            (Price, 1.23456, Some(5), "1.23456"),
            (Price, -37.625, Some(3), "-37.625"),
            (Percent, 5.125, None, "5.12%"),
            (Percent, -2.5, Some(1), "-2.5%"),
            (Fixed(4), 0.0004, None, "0.0004"),
            (Fixed(4), 1.25, Some(1), "1.2"),
            (Fixed(usize::MAX), 1.0, None, "1.000000000000"),
            (Volume, f64::NAN, None, "-"),
            (Percent, f64::INFINITY, None, "-"),
        ] {
            assert_eq!(format.format(value, precision), expected, "{format:?}");
            if format == Volume && value > 0.0 && precision.is_none() {
                assert_eq!(format_volume(value), expected);
            }
        }
    }
}
