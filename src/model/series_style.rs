//! How a bar series shows its last value and base line, fits its scale and takes clicks.

use super::Bar;
use crate::model::LineStyle;
use egui::Color32;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// The bar whose value a series' price line and last value label show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PriceLineSource {
    /// The newest bar, also when it is scrolled out of view.
    #[default]
    LastBar,
    /// The newest bar in view.
    LastVisible,
}

/// The range a series contributes when its price scale fits the visible bars.
///
/// The function receives the series' visible bars and the range the chart would fit, in prices,
/// or `None` when they have no finite values. It returns the range to fit instead, as
/// `(min, max)` in prices, or `None` to leave the series out of the fit. A manual price range
/// ignores it.
#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct AutoscaleProvider(
    Arc<dyn Fn(&[Bar], Option<(f64, f64)>) -> Option<(f64, f64)> + Send + Sync>,
);

impl AutoscaleProvider {
    /// Wraps `provide`, called with the visible bars and the default range on every fit.
    pub fn new(
        provide: impl Fn(&[Bar], Option<(f64, f64)>) -> Option<(f64, f64)> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(provide))
    }

    /// The range to fit for `bars`, given the chart's `default` range.
    pub(crate) fn range(&self, bars: &[Bar], default: Option<(f64, f64)>) -> Option<(f64, f64)> {
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
/// [`render_alert_levels`](crate::chart::render_alert_levels) with these `labels` and `color`,
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
pub struct SeriesStyle {
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
    /// Other scale modes have no base value.
    pub base_line_visible: bool,
    /// Replaces the range the series contributes to its scale's fit. A function is not saved;
    /// hosts set it again after restoring a chart.
    #[serde(skip)]
    pub autoscale: Option<AutoscaleProvider>,
    /// Horizontal distance in logical points from a bar's centre within which a click on its
    /// high to low range selects the main series. Host series do not take clicks.
    pub hit_tolerance: f32,
    /// Visibility and style of the series' alert levels.
    pub alerts: SeriesAlerts,
}

impl Default for SeriesStyle {
    fn default() -> Self {
        Self {
            last_value_visible: true,
            price_line_visible: true,
            price_line_source: PriceLineSource::LastBar,
            price_line_color: None,
            price_line_width: 1.0,
            price_line_style: LineStyle::Dotted,
            base_line_visible: false,
            autoscale: None,
            hit_tolerance: 5.0,
            alerts: SeriesAlerts::default(),
        }
    }
}
