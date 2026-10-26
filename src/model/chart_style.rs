//! Independent styles retained when switching chart types.

use crate::model::LineStyle;
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;
use serde::{Deserialize, Serialize};

/// How a series line connects its points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LineType {
    /// Straight segments between points.
    #[default]
    Simple,
    /// A smooth curve through every point, a Catmull-Rom spline drawn as cubic Bezier segments.
    Curved,
}

/// When the Line, Step line, Line with markers, Area and Baseline charts pulse a circle at the
/// last price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LastPriceAnimation {
    /// No animation.
    #[default]
    Disabled,
    /// Pulse continuously.
    Continuous,
    /// Pulse once each time the last bar's time or price changes.
    OnDataUpdate,
}

/// A series line, with width in logical points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LinePlotStyle {
    /// Line colour.
    pub color: Color32,
    /// Width in logical points. Non-positive or non-finite widths hide the line.
    pub width: f32,
    /// Solid, dashed or dotted.
    pub style: LineStyle,
    /// Straight or curved. The Step line chart ignores it; the Baseline chart follows its
    /// `above` line on both sides.
    pub line_type: LineType,
}

impl Default for LinePlotStyle {
    fn default() -> Self {
        Self {
            color: DESIGN_TOKENS.semantic.brand.accent,
            width: DESIGN_TOKENS.stroke.thick,
            style: LineStyle::Solid,
            line_type: LineType::Simple,
        }
    }
}

/// The Area chart's line and vertical fill stops.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AreaStyle {
    /// Line drawn along the edge of the fill.
    pub line: LinePlotStyle,
    /// Fill at the top of the gradient, including opacity.
    pub top_fill: Color32,
    /// Fill at the bottom of the gradient, including opacity.
    pub bottom_fill: Color32,
    /// Spread the gradient between the line's visible extreme and the pane edge the fill
    /// reaches, the default, instead of over the whole price pane.
    pub relative_gradient: bool,
    /// Fill from the line up to the pane top instead of down to the pane bottom.
    pub invert_filled_area: bool,
}

impl Default for AreaStyle {
    fn default() -> Self {
        let color = DESIGN_TOKENS.semantic.brand.accent;
        Self {
            line: LinePlotStyle::default(),
            top_fill: Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 115),
            bottom_fill: Color32::TRANSPARENT,
            relative_gradient: true,
            invert_filled_area: false,
        }
    }
}

/// The Baseline chart's independent colours on either side of its base.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BaselineStyle {
    /// Line above the base in price units, also on an inverted scale.
    pub above: LinePlotStyle,
    /// Line below the base in price units.
    pub below: LinePlotStyle,
    /// Fill above the base, including opacity.
    pub above_fill: Color32,
    /// Fill below the base, including opacity.
    pub below_fill: Color32,
}

impl Default for BaselineStyle {
    fn default() -> Self {
        let up = DESIGN_TOKENS.semantic.extended.bullish;
        let down = DESIGN_TOKENS.semantic.extended.bearish;
        Self {
            above: LinePlotStyle {
                color: up,
                ..Default::default()
            },
            below: LinePlotStyle {
                color: down,
                ..Default::default()
            },
            above_fill: Color32::from_rgba_unmultiplied(up.r(), up.g(), up.b(), 30),
            below_fill: Color32::from_rgba_unmultiplied(down.r(), down.g(), down.b(), 30),
        }
    }
}

/// The Bars chart's OHLC ticks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarsStyle {
    /// Draw the open tick on the left of each bar.
    pub open_visible: bool,
    /// Draw bars with the chart's wick width at every zoom level instead of a stroke that
    /// widens with the bar spacing.
    pub thin_bars: bool,
}

impl Default for BarsStyle {
    fn default() -> Self {
        Self {
            open_visible: true,
            thin_bars: true,
        }
    }
}

/// Styles of the bars, line, step, marker, area and baseline chart types.
/// Defaults use the design-token palette and remain independent of candle colours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChartTypeStyles {
    /// Line chart.
    pub line: LinePlotStyle,
    /// Step line chart.
    pub step_line: LinePlotStyle,
    /// Line with markers chart.
    pub line_with_markers: LinePlotStyle,
    /// Marker radius in logical points. Non-positive or non-finite radii hide markers.
    pub marker_radius: f32,
    /// Area chart.
    pub area: AreaStyle,
    /// Baseline chart.
    pub baseline: BaselineStyle,
    /// Bars chart.
    pub bars: BarsStyle,
    /// Radius in logical points of the marker drawn on the line at the crosshair's bar in the
    /// Line, Step line, Line with markers, Area and Baseline charts. Non-positive or non-finite
    /// radii hide it, the default.
    pub crosshair_marker_radius: f32,
    /// Pulse at the last price of the line-based charts.
    pub last_price_animation: LastPriceAnimation,
}

impl Default for ChartTypeStyles {
    fn default() -> Self {
        Self {
            line: LinePlotStyle::default(),
            step_line: LinePlotStyle::default(),
            line_with_markers: LinePlotStyle::default(),
            marker_radius: 3.0,
            area: AreaStyle::default(),
            baseline: BaselineStyle::default(),
            bars: BarsStyle::default(),
            crosshair_marker_radius: 0.0,
            last_price_animation: LastPriceAnimation::Disabled,
        }
    }
}
