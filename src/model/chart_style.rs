//! Independent styles retained when switching chart types.

use super::LineStyle;
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;
use serde::{Deserialize, Serialize};

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
}

impl Default for LinePlotStyle {
    fn default() -> Self {
        Self {
            color: DESIGN_TOKENS.semantic.brand.accent,
            width: DESIGN_TOKENS.stroke.thick,
            style: LineStyle::Solid,
        }
    }
}

/// The Area chart's line and vertical fill stops.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AreaStyle {
    /// Line drawn above the fill.
    pub line: LinePlotStyle,
    /// Fill at the highest visible point, including opacity.
    pub top_fill: Color32,
    /// Fill at the plot bottom, including opacity.
    pub bottom_fill: Color32,
}

impl Default for AreaStyle {
    fn default() -> Self {
        let color = DESIGN_TOKENS.semantic.brand.accent;
        Self {
            line: LinePlotStyle::default(),
            top_fill: Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 115),
            bottom_fill: Color32::TRANSPARENT,
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
        }
    }
}
