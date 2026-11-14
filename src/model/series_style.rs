//! How a bar series shows its last value and base line, fits its scale and takes clicks.

use super::Bar;
use crate::model::LineStyle;
use egui::Color32;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// The units a series writes, independently of its price scale's transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SeriesFormat {
    /// Fixed decimal prices.
    #[default]
    Price,
    /// Quantities abbreviated with K, M and B suffixes.
    Volume,
    /// Raw percentage units: 12.5 is written as 12.50%, without a base calculation.
    Percentage,
}

/// Saved numeric display settings for a bar series. Values and scale modes stay unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeriesDisplayFormat {
    /// The units and suffix to display.
    pub kind: SeriesFormat,
    /// Decimal places, after abbreviation for volume.
    pub precision: u8,
    /// Smallest movement in raw units. Labels round to its nearest multiple, with halfway
    /// values rounded away from zero; raw-price ticks use its multiples. Non-positive or
    /// non-finite values are ignored, leaving precision to determine rounding and tick spacing.
    pub min_move: f64,
}

impl Default for SeriesDisplayFormat {
    fn default() -> Self {
        Self {
            kind: SeriesFormat::Price,
            precision: 2,
            min_move: 0.01,
        }
    }
}

/// The bar whose value a series' price line and last value label show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PriceLineSource {
    /// The newest bar, also when it is scrolled out of view.
    #[default]
    LastBar,
    /// The newest bar in view.
    LastVisible,
}

/// Extra space at the screen edges of an automatically fitted price scale, in logical pixels.
///
/// Each edge takes the largest request among visible contributors on that pane and scale.
/// These margins add to the scale's fractional margins, including on inverted scales.
/// Negative and non-finite requests are ignored. Manual ranges ignore these margins;
/// a pane too short to leave room for prices keeps its ordinary fit.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AutoscaleMargins {
    /// Extra space at the top of the plot.
    pub top: f32,
    /// Extra space at the bottom of the plot.
    pub bottom: f32,
}

impl AutoscaleMargins {
    #[cfg(feature = "widget")]
    pub(crate) fn merge(&mut self, other: Self) {
        if other.top.is_finite() {
            self.top = self.top.max(other.top);
        }
        if other.bottom.is_finite() {
            self.bottom = self.bottom.max(other.bottom);
        }
    }
}

/// Prices and extra screen space contributed to one automatically fitted price scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoscaleInfo {
    /// The lowest and highest raw prices to include.
    pub price_range: (f64, f64),
    /// Extra screen space above and below the fitted prices. Defaults to zero.
    pub margins: AutoscaleMargins,
}

impl From<(f64, f64)> for AutoscaleInfo {
    fn from(price_range: (f64, f64)) -> Self {
        Self {
            price_range,
            margins: AutoscaleMargins::default(),
        }
    }
}

/// The prices and optional pixel margins a series contributes when fitting visible bars.
///
/// The function receives the series' visible bars and the range the chart would fit, in prices,
/// or `None` when they have no finite values. It returns the range to fit instead, as
/// `(min, max)` in prices, or `None` to leave the series out of the fit. Use
/// [`with_margins`](Self::with_margins) to also reserve screen space. A manual price range
/// ignores the contribution.
#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct AutoscaleProvider(
    Arc<dyn Fn(&[Bar], Option<(f64, f64)>) -> Option<AutoscaleInfo> + Send + Sync>,
);

impl AutoscaleProvider {
    /// Wraps `provide`, called with the visible bars and the default range on every fit.
    pub fn new(
        provide: impl Fn(&[Bar], Option<(f64, f64)>) -> Option<(f64, f64)> + Send + Sync + 'static,
    ) -> Self {
        Self::with_margins(move |bars, default| provide(bars, default).map(Into::into))
    }

    /// Wraps a provider that returns prices together with fixed logical-pixel margins.
    /// The inputs and `None` behavior are the same as [`new`](Self::new).
    pub fn with_margins(
        provide: impl Fn(&[Bar], Option<(f64, f64)>) -> Option<AutoscaleInfo> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(provide))
    }

    /// The contribution for `bars`, given the chart's `default` price range.
    pub(crate) fn info(&self, bars: &[Bar], default: Option<(f64, f64)>) -> Option<AutoscaleInfo> {
        (self.0)(bars, default)
    }
}

impl std::fmt::Debug for AutoscaleProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AutoscaleProvider(..)")
    }
}

/// Two providers are equal when they share one function.
impl PartialEq for AutoscaleProvider {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// How the alert levels of one bar series are drawn.
///
/// Alerts are not bound to a series, so the host passes the alerts it resolves to a series to
/// `chart::render_alert_levels` (requires `widget`) with these `labels` and `color`,
/// and draws none while `visible` is unset.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SeriesAlerts {
    /// Draw the series' alert levels.
    pub visible: bool,
    /// Label each level with its alert.
    pub labels: bool,
    /// Colour of the levels and labels.
    pub color: Color32,
}

impl Default for SeriesAlerts {
    fn default() -> Self {
        Self {
            visible: true,
            labels: true,
            color: Color32::from_rgb(255, 152, 0),
        }
    }
}

/// A bar series' last value label, price line, base line, autoscale, click tolerance and alert
/// levels.
///
/// The chart-wide `ChartConfig::show_symbol_last_val` and `show_price_line` switches hide every
/// series' labels and price lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeriesStyle {
    /// Raw-value display for ticks, tags, legends and readouts. `None` retains instrument or
    /// scale formatting. Explicit series formatters take precedence, followed by the chart's
    /// price formatter, then this declaration. Percentage and indexed scale modes keep their
    /// transformed labels. A shared axis takes precision and minimum movement from its first
    /// visible series; each series keeps its own tag and readout format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_format: Option<SeriesDisplayFormat>,
    /// Label the last value on the price axis.
    pub last_value_visible: bool,
    /// Draw a horizontal line at the last value.
    pub price_line_visible: bool,
    /// The bar whose value the price line and the last value label show.
    pub price_line_source: PriceLineSource,
    /// Price line colour; `None` takes the series colour, which for the main series follows the
    /// direction of the bar.
    pub price_line_color: Option<Color32>,
    /// Price line width in logical points.
    pub price_line_width: f32,
    /// Price line style.
    pub price_line_style: LineStyle,
    /// Draw a line at the series' base value, 0% in percentage mode and 100 in indexed mode.
    /// Other scale modes have no base value. Defaults to true.
    pub base_line_visible: bool,
    /// Base line colour. Defaults to `#B2B5BE`.
    pub base_line_color: Color32,
    /// Base line width in logical points, default 1. Non-positive or non-finite widths hide it.
    pub base_line_width: f32,
    /// Base line pattern. Defaults to [`LineStyle::Solid`].
    pub base_line_style: LineStyle,
    /// Replaces the prices and pixel margins the series contributes to its scale's fit.
    /// A function is not saved; hosts set it again after restoring a chart.
    #[serde(skip)]
    pub autoscale: Option<AutoscaleProvider>,
    /// Horizontal distance in logical points from a bar's centre within which a click on its
    /// high to low range selects the main series. Host series do not take clicks. A column
    /// series also selects a click this far outside its painted column in any direction.
    pub hit_tolerance: f32,
    /// Visibility and style of the series' alert levels.
    pub alerts: SeriesAlerts,
}

impl Default for SeriesStyle {
    fn default() -> Self {
        Self {
            display_format: None,
            last_value_visible: true,
            price_line_visible: true,
            price_line_source: PriceLineSource::LastBar,
            price_line_color: None,
            price_line_width: 1.0,
            price_line_style: LineStyle::Dashed,
            base_line_visible: true,
            base_line_color: Color32::from_rgb(178, 181, 190),
            base_line_width: 1.0,
            base_line_style: LineStyle::Solid,
            autoscale: None,
            hit_tolerance: 5.0,
            alerts: SeriesAlerts::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saved strokes round-trip, and layouts without the new fields get the default stroke
    /// while keeping their explicit visibility choice.
    #[test]
    fn base_line_settings_round_trip_and_default_older_styles() {
        let mut saved = serde_json::to_value(SeriesStyle::default()).unwrap();
        saved["base_line_color"] = serde_json::json!([12, 34, 56, 255]);
        saved["base_line_width"] = serde_json::json!(3.0);
        saved["base_line_style"] = serde_json::json!("LargeDashed");
        saved["base_line_visible"] = serde_json::json!(false);
        let style: SeriesStyle = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(serde_json::to_value(style).unwrap(), saved);

        for visible in [false, true] {
            saved["base_line_visible"] = serde_json::json!(visible);
            for field in ["base_line_color", "base_line_width", "base_line_style"] {
                saved.as_object_mut().unwrap().remove(field);
            }
            let style: SeriesStyle = serde_json::from_value(saved.clone()).unwrap();
            let actual = serde_json::to_value(style).unwrap();
            assert_eq!(
                actual["base_line_color"],
                serde_json::json!([178, 181, 190, 255])
            );
            assert_eq!(actual["base_line_width"], 1.0);
            assert_eq!(actual["base_line_style"], "Solid");
            assert_eq!(actual["base_line_visible"], visible);
        }
        assert!(SeriesStyle::default().base_line_visible);
    }
}
