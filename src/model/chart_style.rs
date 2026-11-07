//! Independent styles retained when switching chart types.

use crate::model::{LineStyle, PriceSource};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;
use serde::{Deserialize, Serialize};
use std::cell::Cell;

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
    /// Point marker radius in logical points. Non-positive or non-finite radii hide markers,
    /// the default. Each marker takes its point's line colour.
    pub marker_radius: f32,
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
            marker_radius: 0.0,
            top_fill: Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 115),
            bottom_fill: Color32::TRANSPARENT,
            relative_gradient: true,
            invert_filled_area: false,
        }
    }
}

/// The Baseline chart's independent colours and vertical fills on either side of its base.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BaselineStyle {
    /// Line above the base in price units, also on an inverted scale.
    pub above: LinePlotStyle,
    /// Line below the base in price units.
    pub below: LinePlotStyle,
    /// Point marker radius in logical points. Non-positive or non-finite radii hide markers,
    /// the default. Each marker takes its point's line colour on its side of the base.
    pub marker_radius: f32,
    /// Fill at the pane's high-price edge, including opacity, also on an inverted scale.
    pub above_fill: Color32,
    /// Fill at the pane's low-price edge, including opacity.
    pub below_fill: Color32,
    /// Fill at the base on its higher-price side. `None` uses `above_fill` for a flat fill.
    /// The gradient interpolates in screen space between the pane edge and the base.
    pub above_fill_at_base: Option<Color32>,
    /// Fill at the base on its lower-price side. `None` uses `below_fill` for a flat fill.
    pub below_fill_at_base: Option<Color32>,
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
            marker_radius: 0.0,
            above_fill: Color32::from_rgba_unmultiplied(up.r(), up.g(), up.b(), 30),
            below_fill: Color32::from_rgba_unmultiplied(down.r(), down.g(), down.b(), 30),
            above_fill_at_base: None,
            below_fill_at_base: None,
        }
    }
}

/// The HLC area chart's high, low and close lines and the fills between them. Lines join their
/// points with straight segments like the fills, whatever their `line_type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HlcAreaStyle {
    /// Draw the line through the highs.
    pub high_line_visible: bool,
    /// Line through the highs.
    pub high_line: LinePlotStyle,
    /// Draw the line through the lows.
    pub low_line_visible: bool,
    /// Line through the lows.
    pub low_line: LinePlotStyle,
    /// Line through the closes. A non-positive or non-finite width hides it.
    pub close_line: LinePlotStyle,
    /// Draw the fill between the high and close lines.
    pub high_to_close_fill_visible: bool,
    /// Fill between the high and close lines, including opacity.
    pub high_to_close_fill: Color32,
    /// Draw the fill between the close and low lines.
    pub close_to_low_fill_visible: bool,
    /// Fill between the close and low lines, including opacity.
    pub close_to_low_fill: Color32,
}

impl HlcAreaStyle {
    /// The default style with its high line and upper fill in `up` and its low line and lower
    /// fill in `down`.
    pub(crate) fn with_colors(up: Color32, down: Color32) -> Self {
        let line = |color| LinePlotStyle {
            color,
            ..Default::default()
        };
        // Fills default to 20% opacity.
        let fill = |color| Self::fill_tint(color, 51);
        Self {
            high_line_visible: true,
            high_line: line(up),
            low_line_visible: true,
            low_line: line(down),
            close_line: line(Color32::from_rgb(134, 137, 147)),
            high_to_close_fill_visible: true,
            high_to_close_fill: fill(up),
            close_to_low_fill_visible: true,
            close_to_low_fill: fill(down),
        }
    }

    /// `color` at the fill opacity `alpha`.
    pub(crate) fn fill_tint(color: Color32, alpha: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
    }

    /// The style drawing what a saved entry of the former format drew: both fills at
    /// `fill_opacity` in `up` and `down`, the close line at `line_width` in `up`, and no high or
    /// low line.
    pub(crate) fn former(fill_opacity: f32, line_width: f32, up: Color32, down: Color32) -> Self {
        // `as u8` saturates and maps NaN to 0.
        let alpha = (fill_opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        Self {
            high_line_visible: false,
            low_line_visible: false,
            close_line: LinePlotStyle {
                color: up,
                width: line_width,
                ..Default::default()
            },
            high_to_close_fill: Self::fill_tint(up, alpha),
            close_to_low_fill: Self::fill_tint(down, alpha),
            ..Self::with_colors(up, down)
        }
    }

    /// Forgets the entry of the former format noted on this thread.
    #[cfg(feature = "widget")]
    pub(crate) fn forget_former() {
        FORMER_HLC_AREA.set(None);
    }

    /// Draws this style, the last one read on this thread, in the candle colours of the document
    /// holding it if it was saved in the former format, which took them from the chart.
    #[cfg(feature = "widget")]
    pub(crate) fn fill_former(&mut self, up: Color32, down: Color32) {
        if let Some((fill_opacity, line_width)) = FORMER_HLC_AREA.take() {
            *self = Self::former(fill_opacity, line_width, up, down);
        }
    }
}

thread_local! {
    /// The `fill_opacity` and `line_width` of the entry of the former format last read on this
    /// thread.
    static FORMER_HLC_AREA: Cell<Option<(f32, f32)>> = const { Cell::new(None) };
}

/// Reads a saved HLC area entry. One of the former format notes its fields for the document
/// holding it and reads in the token colours, the colours of a style read without its settings.
fn read_hlc_area<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<HlcAreaStyle, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged, deny_unknown_fields)]
    enum Saved {
        // The former serializer wrote exactly these fields, and a current entry has neither.
        Former { fill_opacity: f32, line_width: f32 },
        Current(HlcAreaStyle),
    }
    Ok(match Saved::deserialize(deserializer)? {
        Saved::Former {
            fill_opacity,
            line_width,
        } => {
            FORMER_HLC_AREA.set(Some((fill_opacity, line_width)));
            HlcAreaStyle::former(
                fill_opacity,
                line_width,
                DESIGN_TOKENS.semantic.extended.bullish,
                DESIGN_TOKENS.semantic.extended.bearish,
            )
        }
        Saved::Current(style) => style,
    })
}

impl Default for HlcAreaStyle {
    fn default() -> Self {
        Self::with_colors(
            DESIGN_TOKENS.semantic.extended.bullish,
            DESIGN_TOKENS.semantic.extended.bearish,
        )
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
    /// Colour each bar by its close against the previous close instead of its open: rising when
    /// the close is at or above the last close before it.
    pub color_by_previous_close: bool,
}

impl Default for BarsStyle {
    fn default() -> Self {
        Self {
            open_visible: true,
            thin_bars: true,
            color_by_previous_close: false,
        }
    }
}

/// The Columns chart's height.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColumnsStyle {
    /// The price each column reaches. Its colour still compares closes.
    pub price_source: PriceSource,
}

/// The Candles chart's bodies, wicks and colouring. Borders are drawn while the chart's candle
/// border width is above zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CandlesStyle {
    /// Fill each candle's body between its open and close.
    pub body_visible: bool,
    /// Draw each candle's wick from its high to its low.
    pub wick_visible: bool,
    /// Colour each candle by its close against the previous close instead of its open: rising
    /// when the close is at or above the last close before it.
    pub color_by_previous_close: bool,
}

impl Default for CandlesStyle {
    fn default() -> Self {
        Self {
            body_visible: true,
            wick_visible: true,
            color_by_previous_close: false,
        }
    }
}

/// Independent candle components and scale prices for the Heikin-Ashi chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeikinStyle {
    /// Fill the transformed open-to-close body.
    pub body_visible: bool,
    /// Outline the transformed body inside its bounds, using the chart's positive border
    /// width or a one-point stroke by default.
    pub border_visible: bool,
    /// Draw the transformed high-to-low wick.
    pub wick_visible: bool,
    /// Compare transformed closes when choosing colours instead of open and close.
    pub color_by_previous_close: bool,
    /// Rising body colour.
    pub body_up: Color32,
    /// Falling body colour.
    pub body_down: Color32,
    /// Rising border colour.
    pub border_up: Color32,
    /// Falling border colour.
    pub border_down: Color32,
    /// Rising wick colour.
    pub wick_up: Color32,
    /// Falling wick colour.
    pub wick_down: Color32,
    /// Show only the original close on the price scale. By default both original and
    /// transformed closes are labelled.
    pub real_price_only: bool,
}

impl Default for HeikinStyle {
    fn default() -> Self {
        let up = DESIGN_TOKENS.semantic.extended.bullish;
        let down = DESIGN_TOKENS.semantic.extended.bearish;
        Self {
            body_visible: true,
            border_visible: true,
            wick_visible: true,
            color_by_previous_close: false,
            body_up: up,
            body_down: down,
            border_up: up,
            border_down: down,
            wick_up: up,
            wick_down: down,
            real_price_only: false,
        }
    }
}

impl HeikinStyle {
    /// How a Heikin-Ashi chart drew before it had this style: only the original close was tagged.
    fn saved_without_style() -> Self {
        Self {
            real_price_only: true,
            ..Self::default()
        }
    }
}

/// Independent candle components for the hollow candles chart.
///
/// A bar is coloured by its close against the previous close and is hollow, an outline without
/// fill, when its close is above its open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HollowStyle {
    /// Fill the body of a candle that is not hollow.
    pub body_visible: bool,
    /// Outline the body, which is all a hollow candle shows of it.
    pub border_visible: bool,
    /// Draw the high-to-low wick.
    pub wick_visible: bool,
    /// Rising body colour.
    pub body_up: Color32,
    /// Falling body colour.
    pub body_down: Color32,
    /// Rising border colour.
    pub border_up: Color32,
    /// Falling border colour.
    pub border_down: Color32,
    /// Rising wick colour.
    pub wick_up: Color32,
    /// Falling wick colour.
    pub wick_down: Color32,
}

impl Default for HollowStyle {
    fn default() -> Self {
        let up = DESIGN_TOKENS.semantic.extended.bullish;
        let down = DESIGN_TOKENS.semantic.extended.bearish;
        Self {
            body_visible: true,
            border_visible: true,
            wick_visible: true,
            body_up: up,
            body_down: down,
            border_up: up,
            border_down: down,
            wick_up: up,
            wick_down: down,
        }
    }
}

/// Independent candle components for the volume candles chart.
///
/// A bar is rising when its close is above its open, or with `color_by_previous_close` when its
/// close is not below the previous close.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VolumeCandlesStyle {
    /// Fill the body.
    pub body_visible: bool,
    /// Outline the body.
    pub border_visible: bool,
    /// Draw the high-to-low wick.
    pub wick_visible: bool,
    /// Compare a bar's close with the previous close instead of its open.
    pub color_by_previous_close: bool,
    /// Rising body colour.
    pub body_up: Color32,
    /// Falling body colour.
    pub body_down: Color32,
    /// Rising border colour.
    pub border_up: Color32,
    /// Falling border colour.
    pub border_down: Color32,
    /// Rising wick colour.
    pub wick_up: Color32,
    /// Falling wick colour.
    pub wick_down: Color32,
}

impl Default for VolumeCandlesStyle {
    fn default() -> Self {
        let up = DESIGN_TOKENS.semantic.extended.bullish;
        let down = DESIGN_TOKENS.semantic.extended.bearish;
        Self {
            body_visible: true,
            border_visible: true,
            wick_visible: true,
            color_by_previous_close: false,
            body_up: up,
            body_down: down,
            border_up: up,
            border_down: down,
            wick_up: up,
            wick_down: down,
        }
    }
}

/// The High-low chart's single bar colour and its price labels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HighLowStyle {
    /// Colour of every bar, whatever its direction.
    pub bar_color: Color32,
    /// Print each bar's high beyond its high end and its low beyond its low end, once the bar
    /// spacing is wide enough to fit every visible label.
    pub labels_visible: bool,
    /// Price label colour. `Color32::PLACEHOLDER`, the default, prints the labels in the price
    /// scale's text colour.
    pub label_color: Color32,
    /// Price label size in logical points, drawn at 1 to 256 points. `Chart::snapshot` and
    /// `Chart::restore` refuse a positive size outside that range. Non-positive or non-finite
    /// sizes hide the labels.
    pub label_font_size: f32,
}

impl Default for HighLowStyle {
    fn default() -> Self {
        Self {
            bar_color: DESIGN_TOKENS.semantic.brand.accent,
            labels_visible: true,
            label_color: Color32::PLACEHOLDER,
            label_font_size: 7.0,
        }
    }
}

impl HighLowStyle {
    /// How a high-low chart drew before it had this style: without price labels.
    fn saved_without_style() -> Self {
        Self {
            labels_visible: false,
            ..Self::default()
        }
    }
}

/// The Kagi chart's stroke widths in logical points. Non-positive or non-finite widths hide
/// the lines of that thickness.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KagiStyle {
    /// Width of the thick lines of an uptrend.
    pub thick_width: f32,
    /// Width of the thin lines of a downtrend.
    pub thin_width: f32,
}

impl Default for KagiStyle {
    fn default() -> Self {
        Self {
            thick_width: 2.5,
            thin_width: 1.5,
        }
    }
}

/// Styles of the candle, hollow candle, volume candle, Heikin-Ashi, bar, column, high-low, line,
/// step, marker, area, HLC area, baseline and Kagi chart types.
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
    /// HLC area chart. Documents saved with its former `fill_opacity` and `line_width` restore as
    /// they were drawn.
    #[serde(deserialize_with = "read_hlc_area")]
    pub hlc_area: HlcAreaStyle,
    /// Candles chart.
    pub candles: CandlesStyle,
    /// Hollow candles chart.
    pub hollow: HollowStyle,
    /// Volume candles chart.
    pub volume_candles: VolumeCandlesStyle,
    /// Heikin-Ashi chart. Documents saved before this style restore with their original close
    /// alone on the scale, as they were drawn.
    #[serde(default = "HeikinStyle::saved_without_style")]
    pub heikin: HeikinStyle,
    /// Bars chart.
    pub bars: BarsStyle,
    /// Columns chart.
    pub columns: ColumnsStyle,
    /// High-low chart. Documents saved before this style restore without price labels, as they
    /// were drawn.
    #[serde(default = "HighLowStyle::saved_without_style")]
    pub high_low: HighLowStyle,
    /// Kagi chart.
    pub kagi: KagiStyle,
    /// Radius in logical points of the marker drawn on the line at the crosshair's bar in the
    /// Line, Step line, Line with markers, Area and Baseline charts. Non-positive or non-finite
    /// radii hide it, the default.
    pub crosshair_marker_radius: f32,
    /// Crosshair marker fill, including opacity. `None` uses the hovered point's line colour.
    pub crosshair_marker_fill: Option<Color32>,
    /// Crosshair marker border colour. `None` uses the chart background.
    pub crosshair_marker_border_color: Option<Color32>,
    /// Crosshair marker border width in logical points. Non-positive or non-finite widths
    /// hide the border; the default is the design-token thick stroke.
    pub crosshair_marker_border_width: f32,
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
            hlc_area: HlcAreaStyle::default(),
            candles: CandlesStyle::default(),
            hollow: HollowStyle::default(),
            volume_candles: VolumeCandlesStyle::default(),
            heikin: HeikinStyle::default(),
            bars: BarsStyle::default(),
            columns: ColumnsStyle::default(),
            high_low: HighLowStyle::default(),
            kagi: KagiStyle::default(),
            crosshair_marker_radius: 0.0,
            crosshair_marker_fill: None,
            crosshair_marker_border_color: None,
            crosshair_marker_border_width: DESIGN_TOKENS.stroke.thick,
            last_price_animation: LastPriceAnimation::Disabled,
        }
    }
}

impl ChartTypeStyles {
    /// The styles of a document saved before chart types had styles of their own.
    pub(super) fn saved_without_styles() -> Self {
        Self {
            heikin: HeikinStyle::saved_without_style(),
            high_low: HighLowStyle::saved_without_style(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BarColors;

    #[test]
    fn baseline_saved_colours_keep_legacy_flat_fills_and_gradient_endpoints() {
        for gradient in [false, true] {
            let mut saved = serde_json::json!({
                "above_fill": [0, 200, 0, 200], "below_fill": [200, 0, 0, 200]
            });
            let mut point = serde_json::json!({
                "top_fill": [0, 0, 160, 160], "bottom_fill": [160, 160, 0, 160]
            });
            if gradient {
                saved["above_fill_at_base"] = serde_json::json!([0, 40, 0, 40]);
                saved["below_fill_at_base"] = serde_json::json!([40, 0, 0, 40]);
                point["top_fill_at_base"] = serde_json::json!([0, 0, 0, 0]);
                point["bottom_fill_at_base"] = serde_json::json!([0, 0, 0, 0]);
            }
            let style: BaselineStyle = serde_json::from_value(saved.clone()).unwrap();
            let own: BarColors = serde_json::from_value(point.clone()).unwrap();
            let restored = serde_json::to_value(&style).unwrap();
            let restored_point = serde_json::to_value(&own).unwrap();
            for (original, restored) in [(&saved, &restored), (&point, &restored_point)] {
                for (key, color) in original.as_object().unwrap() {
                    assert_eq!(&restored[key], color, "{key}");
                }
            }
            for key in ["above_fill_at_base", "below_fill_at_base"] {
                assert_eq!(restored[key], saved[key]);
            }
            for key in ["top_fill_at_base", "bottom_fill_at_base"] {
                assert_eq!(restored_point[key], point[key]);
            }
        }
    }

    /// The former serializer wrote only `fill_opacity` and `line_width`, so an HLC area entry
    /// holding current fields beside them was saved since and keeps their values.
    #[test]
    fn hlc_area_entry_with_current_fields_keeps_them_beside_former_ones() {
        let mut saved = ChartTypeStyles::default();
        saved.hlc_area.close_line.width = 3.5;
        let mut json = serde_json::to_value(&saved).unwrap();
        json["hlc_area"]["fill_opacity"] = serde_json::json!(0.5);
        json["hlc_area"]["line_width"] = serde_json::json!(4.0);
        let read: ChartTypeStyles = serde_json::from_value(json).unwrap();
        assert_eq!(read.hlc_area, saved.hlc_area);
    }
}
