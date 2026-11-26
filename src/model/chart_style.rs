//! Independent styles retained when switching chart types.

use crate::model::{LineStyle, PriceSource};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;
use serde::{Deserialize, Serialize};
use std::cell::Cell;

/// How a series line connects its points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
/// A saved style reads each field it lacks as it was drawn before the fills faded: flat at 12%
/// opacity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default = "BaselineStyle::saved_flat")]
#[non_exhaustive]
pub struct BaselineStyle {
    /// Line above the base in price units, also on an inverted scale.
    pub above: LinePlotStyle,
    /// Line below the base in price units.
    pub below: LinePlotStyle,
    /// Point marker radius in logical points. Non-positive or non-finite radii hide markers,
    /// the default. Each marker takes its point's line colour on its side of the base.
    pub marker_radius: f32,
    /// Fill at the pane's high-price edge, including opacity, also on an inverted scale.
    /// Default: the rising colour at 28% opacity.
    pub above_fill: Color32,
    /// Fill at the pane's low-price edge, including opacity.
    /// Default: the falling colour at 28% opacity.
    pub below_fill: Color32,
    /// Fill at the base on its higher-price side. `None` uses `above_fill` for a flat fill, as
    /// documents saved before the faded default read. The gradient interpolates in screen space
    /// between the pane edge and the base. Default: the rising colour at 5% opacity.
    pub above_fill_at_base: Option<Color32>,
    /// Fill at the base on its lower-price side. `None` uses `below_fill` for a flat fill, as
    /// documents saved before the faded default read. Default: the falling colour at 5% opacity.
    pub below_fill_at_base: Option<Color32>,
}

impl Default for BaselineStyle {
    fn default() -> Self {
        let up = DESIGN_TOKENS.semantic.extended.bullish;
        let down = DESIGN_TOKENS.semantic.extended.bearish;
        // Each side fades from 28% opacity at the pane edge, round(0.28 * 255) = 71, to 5% at the
        // base, round(0.05 * 255) = 13.
        let fill = HlcAreaStyle::fill_tint;
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
            above_fill: fill(up, 71),
            below_fill: fill(down, 71),
            above_fill_at_base: Some(fill(up, 13)),
            below_fill_at_base: Some(fill(down, 13)),
        }
    }
}

impl BaselineStyle {
    /// How a Baseline chart drew before its fills faded: both flat at 12% opacity,
    /// round(0.12 * 255) = 30.
    fn saved_flat() -> Self {
        let style = Self::default();
        Self {
            above_fill: HlcAreaStyle::fill_tint(style.above.color, 30),
            below_fill: HlcAreaStyle::fill_tint(style.below.color, 30),
            above_fill_at_base: None,
            below_fill_at_base: None,
            ..style
        }
    }
}

/// The HLC area chart's high, low and close lines and the fills between them. Lines join their
/// points with straight segments like the fills, whatever their `line_type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
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
#[non_exhaustive]
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

/// The Columns chart's height and colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ColumnsStyle {
    /// The price each column reaches. Its colour still compares closes.
    pub price_source: PriceSource,
    /// Colour of a column whose close is at or above the close before it, including opacity.
    /// [`Color32::PLACEHOLDER`] draws it in the chart's rising candle body colour, as documents
    /// saved before this colour read.
    #[serde(default = "candle_body_color")]
    pub up_color: Color32,
    /// Colour of a column whose close is below the close before it, including opacity.
    /// [`Color32::PLACEHOLDER`] draws it in the chart's falling candle body colour, as documents
    /// saved before this colour read.
    #[serde(default = "candle_body_color")]
    pub down_color: Color32,
}

impl Default for ColumnsStyle {
    fn default() -> Self {
        // The rising and falling colours at 50% opacity: round(0.5 * 255) = 128.
        Self {
            price_source: PriceSource::default(),
            up_color: HlcAreaStyle::fill_tint(DESIGN_TOKENS.semantic.extended.bullish, 128),
            down_color: HlcAreaStyle::fill_tint(DESIGN_TOKENS.semantic.extended.bearish, 128),
        }
    }
}

impl ColumnsStyle {
    /// How a Columns chart drew before it had colours of its own: in the candle body colours.
    fn saved_without_colors() -> Self {
        Self {
            up_color: Color32::PLACEHOLDER,
            down_color: Color32::PLACEHOLDER,
            ..Self::default()
        }
    }
}

/// The Candles chart's bodies, wicks and colouring. Borders are drawn while the chart's candle
/// border width is above zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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

impl VolumeCandlesStyle {
    /// Stands for the style of a document saved before it existed: volume candles drew without a
    /// border then, and transparent border colours tell it from a saved style, which offers them
    /// with borders on. Applying the document's settings replaces it through
    /// [`Self::drawn_before_style`], and `Chart::set_chart_type_params` takes the defaults for it,
    /// so no chart keeps it.
    fn saved_without_style() -> Self {
        Self {
            border_visible: false,
            border_up: Color32::TRANSPARENT,
            border_down: Color32::TRANSPARENT,
            ..Self::default()
        }
    }

    /// Whether this is the style of a document saved before it existed.
    #[cfg(feature = "widget")]
    pub(crate) fn is_saved_without_style(&self) -> bool {
        *self == Self::saved_without_style()
    }

    /// How volume candles drew before they had this style: body and both wicks in the rising or
    /// falling body colour, by the close against the open, without a border.
    #[cfg(feature = "widget")]
    pub(crate) fn drawn_before_style(up: Color32, down: Color32) -> Self {
        Self {
            border_visible: false,
            body_up: up,
            body_down: down,
            border_up: up,
            border_down: down,
            wick_up: up,
            wick_down: down,
            ..Self::default()
        }
    }
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
#[non_exhaustive]
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

/// The Kagi chart's strokes. Widths are in logical points; non-positive or non-finite widths
/// hide the lines of that thickness.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct KagiStyle {
    /// Width of the thick lines of an uptrend.
    pub thick_width: f32,
    /// Width of the thin lines of a downtrend.
    pub thin_width: f32,
    /// Colour of the thick lines. [`Color32::PLACEHOLDER`] draws them in the chart's rising
    /// candle body colour, as documents saved before this colour read.
    #[serde(default = "candle_body_color")]
    pub up_color: Color32,
    /// Colour of the thin lines. [`Color32::PLACEHOLDER`] draws them in the chart's falling
    /// candle body colour, as documents saved before this colour read.
    #[serde(default = "candle_body_color")]
    pub down_color: Color32,
}

impl Default for KagiStyle {
    fn default() -> Self {
        Self {
            thick_width: 2.5,
            thin_width: 1.5,
            up_color: DESIGN_TOKENS.semantic.extended.bullish,
            down_color: DESIGN_TOKENS.semantic.extended.bearish,
        }
    }
}

impl KagiStyle {
    /// How a Kagi chart drew before it had colours of its own: in the candle body colours.
    fn saved_without_colors() -> Self {
        Self {
            up_color: Color32::PLACEHOLDER,
            down_color: Color32::PLACEHOLDER,
            ..Self::default()
        }
    }
}

/// What a missing colour of a style that once drew in the candle body colours reads.
fn candle_body_color() -> Color32 {
    Color32::PLACEHOLDER
}

/// `color`, or `candle` for [`Color32::PLACEHOLDER`]: the candle body colour that a style saved
/// before it had colours of its own draws in.
#[cfg(feature = "widget")]
pub(crate) fn or_candle(color: Color32, candle: Color32) -> Color32 {
    if color == Color32::PLACEHOLDER {
        candle
    } else {
        color
    }
}

/// The Point and Figure chart's colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct PointFigureStyle {
    /// Colour of the X columns. [`Color32::PLACEHOLDER`] draws them in the chart's rising candle
    /// body colour, as documents saved before this colour read.
    pub up_color: Color32,
    /// Colour of the O columns. [`Color32::PLACEHOLDER`] draws them in the chart's falling
    /// candle body colour, as documents saved before this colour read.
    pub down_color: Color32,
}

impl Default for PointFigureStyle {
    fn default() -> Self {
        Self {
            up_color: DESIGN_TOKENS.semantic.extended.bullish,
            down_color: DESIGN_TOKENS.semantic.extended.bearish,
        }
    }
}

impl PointFigureStyle {
    /// How a Point and Figure chart drew before it had this style: in the candle body colours.
    fn saved_without_style() -> Self {
        Self {
            up_color: Color32::PLACEHOLDER,
            down_color: Color32::PLACEHOLDER,
        }
    }
}

/// The Line Break chart's line borders. A colour left `None` draws the border in its line's
/// colour at full opacity, as a chart that has not set it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct LineBreakStyle {
    /// Outline each line.
    pub border_visible: bool,
    /// Border of rising lines.
    pub border_up: Option<Color32>,
    /// Border of falling lines.
    pub border_down: Option<Color32>,
}

impl Default for LineBreakStyle {
    fn default() -> Self {
        Self {
            border_visible: true,
            border_up: None,
            border_down: None,
        }
    }
}

/// The Renko chart's brick borders and wicks. Colours left `None` draw as a chart that has not
/// set them: the border in its brick's colour at full opacity and the wick in the brick's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct RenkoStyle {
    /// Outline each brick.
    pub border_visible: bool,
    /// Border of rising bricks.
    pub border_up: Option<Color32>,
    /// Border of falling bricks.
    pub border_down: Option<Color32>,
    /// Wick of rising bricks, drawn while
    /// [`ChartTypeParams::renko_wicks`](super::ChartTypeParams::renko_wicks) is on.
    pub wick_up: Option<Color32>,
    /// Wick of falling bricks, drawn while
    /// [`ChartTypeParams::renko_wicks`](super::ChartTypeParams::renko_wicks) is on.
    pub wick_down: Option<Color32>,
}

impl Default for RenkoStyle {
    fn default() -> Self {
        Self {
            border_visible: true,
            border_up: None,
            border_down: None,
            wick_up: None,
            wick_down: None,
        }
    }
}

/// The Range chart's bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct RangeStyle {
    /// Draw each bar as a stroke from its low to its high with open and close ticks, at the
    /// chart's wick width at every zoom level as thin OHLC bars are, instead of a filled block.
    pub thin_bars: bool,
    /// Draw a bar that closed without a trade in its range, as one of the bars a single trade
    /// across a price gap closes, as an outline in its direction's colour without a fill. These
    /// bars stay elements of the chart: studies, the legend and the data window read them as
    /// they read the bars that traded.
    pub phantom_bars: bool,
    /// Colour of the bar in progress, the newest bar until its range is complete, while its close
    /// is above its open. [`Color32::PLACEHOLDER`] draws it in the chart's rising candle body
    /// colour, as completed bars and documents saved before this colour are drawn.
    #[serde(default = "candle_body_color")]
    pub forming_up: Color32,
    /// Colour of the bar in progress while its close is at or below its open.
    /// [`Color32::PLACEHOLDER`] draws it in the chart's falling candle body colour, as completed
    /// bars and documents saved before this colour are drawn.
    #[serde(default = "candle_body_color")]
    pub forming_down: Color32,
}

impl Default for RangeStyle {
    fn default() -> Self {
        Self {
            thin_bars: false,
            phantom_bars: false,
            // Light green and light red, apart from the completed bars on dark and light charts.
            forming_up: Color32::from_rgb(169, 220, 195),
            forming_down: Color32::from_rgb(245, 166, 174),
        }
    }
}

impl RangeStyle {
    /// How a Range chart drew before it had forming bar colours: in the candle body colours.
    fn saved_without_style() -> Self {
        Self {
            forming_up: Color32::PLACEHOLDER,
            forming_down: Color32::PLACEHOLDER,
            ..Self::default()
        }
    }
}

/// Styles of the candle, hollow candle, volume candle, Heikin-Ashi, bar, column, high-low, line,
/// step, marker, area, HLC area, baseline, Renko, Kagi, Line Break, Point and Figure and Range
/// chart types.
/// Defaults use the design-token palette and remain independent of candle colours.
///
/// ```
/// use egui_charts::model::{
///     CandlesStyle, ChartTypeStyles, ColumnsStyle, HeikinStyle, HighLowStyle, HlcAreaStyle,
///     HollowStyle, KagiStyle, LineBreakStyle, PointFigureStyle, PriceSource, RangeStyle,
///     RenkoStyle, VolumeCandlesStyle,
/// };
///
/// let mut styles = ChartTypeStyles::default();
/// let mut candles = CandlesStyle::default();
/// candles.wick_visible = false;
/// styles.candles = candles;
/// let mut heikin = HeikinStyle::default();
/// heikin.color_by_previous_close = true;
/// styles.heikin = heikin;
/// let mut hollow = HollowStyle::default();
/// hollow.wick_visible = false;
/// styles.hollow = hollow;
/// let mut volume_candles = VolumeCandlesStyle::default();
/// volume_candles.border_visible = false;
/// styles.volume_candles = volume_candles;
/// let mut high_low = HighLowStyle::default();
/// high_low.labels_visible = false;
/// styles.high_low = high_low;
/// let mut hlc_area = HlcAreaStyle::default();
/// hlc_area.high_line_visible = false;
/// styles.hlc_area = hlc_area;
/// let mut kagi = KagiStyle::default();
/// kagi.thick_width = 3.0;
/// kagi.up_color = egui::Color32::BLUE;
/// styles.kagi = kagi;
/// let mut point_figure = PointFigureStyle::default();
/// point_figure.up_color = egui::Color32::BLUE;
/// styles.point_figure = point_figure;
/// let mut range = RangeStyle::default();
/// range.thin_bars = true;
/// range.forming_up = egui::Color32::BLUE;
/// styles.range = range;
/// let mut renko = RenkoStyle::default();
/// renko.border_visible = false;
/// styles.renko = renko;
/// let mut line_break = LineBreakStyle::default();
/// line_break.border_visible = false;
/// styles.line_break = line_break;
/// let mut columns = ColumnsStyle::default();
/// columns.up_color = egui::Color32::BLUE;
/// styles.columns = columns;
/// styles.step_line_price_source = Some(PriceSource::High);
/// styles.line_gradient = true;
/// styles.line_gradient_end = egui::Color32::from_rgba_premultiplied(0, 0, 120, 120);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ChartTypeStyles {
    /// Line chart.
    pub line: LinePlotStyle,
    /// Draw the Line chart's line in a gradient instead of in `line`'s colour: the leftmost
    /// segment in view takes `line_gradient_start`, the rightmost `line_gradient_end`, and the
    /// segments between them blend from one to the other by position. A bar's own colour still
    /// colours the segment it starts. Off by default and in documents saved without it.
    pub line_gradient: bool,
    /// The Line chart's gradient colour at its leftmost segment in view, including opacity, kept
    /// while the line is drawn in one colour.
    pub line_gradient_start: Color32,
    /// The Line chart's gradient colour at its rightmost segment in view, including opacity,
    /// kept while the line is drawn in one colour.
    pub line_gradient_end: Color32,
    /// Step line chart.
    pub step_line: LinePlotStyle,
    /// Line with markers chart.
    pub line_with_markers: LinePlotStyle,
    /// Marker radius in logical points. Non-positive or non-finite radii hide markers.
    pub marker_radius: f32,
    /// Area chart.
    pub area: AreaStyle,
    /// Baseline chart. Documents saved before its fills faded draw them flat, as they were
    /// drawn.
    #[serde(default = "BaselineStyle::saved_flat")]
    pub baseline: BaselineStyle,
    /// Price the Line chart draws. `None`, as in documents saved before each line-based chart
    /// type had a source of its own, draws `ChartConfig::price_source`.
    pub line_price_source: Option<PriceSource>,
    /// Price the Step line chart draws; `None` draws `ChartConfig::price_source`.
    pub step_line_price_source: Option<PriceSource>,
    /// Price the Line with markers chart draws; `None` draws `ChartConfig::price_source`.
    pub line_with_markers_price_source: Option<PriceSource>,
    /// Price the Area chart draws; `None` draws `ChartConfig::price_source`.
    pub area_price_source: Option<PriceSource>,
    /// Price the Baseline chart draws on both sides of its base; `None` draws
    /// `ChartConfig::price_source`.
    pub baseline_price_source: Option<PriceSource>,
    /// HLC area chart. Documents saved with its former `fill_opacity` and `line_width` restore as
    /// they were drawn.
    #[serde(deserialize_with = "read_hlc_area")]
    pub hlc_area: HlcAreaStyle,
    /// Candles chart.
    pub candles: CandlesStyle,
    /// Hollow candles chart.
    pub hollow: HollowStyle,
    /// Volume candles chart. Documents saved before this style restore as they were drawn, in
    /// their saved body colours without a border.
    #[serde(default = "VolumeCandlesStyle::saved_without_style")]
    pub volume_candles: VolumeCandlesStyle,
    /// Heikin-Ashi chart. Documents saved before this style restore with their original close
    /// alone on the scale, as they were drawn.
    #[serde(default = "HeikinStyle::saved_without_style")]
    pub heikin: HeikinStyle,
    /// Bars chart.
    pub bars: BarsStyle,
    /// Columns chart. Documents saved before its colours draw in the candle body colours, as
    /// they were drawn.
    #[serde(default = "ColumnsStyle::saved_without_colors")]
    pub columns: ColumnsStyle,
    /// High-low chart. Documents saved before this style restore without price labels, as they
    /// were drawn.
    #[serde(default = "HighLowStyle::saved_without_style")]
    pub high_low: HighLowStyle,
    /// Renko chart.
    pub renko: RenkoStyle,
    /// Line Break chart.
    pub line_break: LineBreakStyle,
    /// Kagi chart. Documents saved before its colours draw in the candle body colours, as they
    /// were drawn.
    #[serde(default = "KagiStyle::saved_without_colors")]
    pub kagi: KagiStyle,
    /// Point and Figure chart. Documents saved before this style draw in the candle body
    /// colours, as they were drawn.
    #[serde(default = "PointFigureStyle::saved_without_style")]
    pub point_figure: PointFigureStyle,
    /// Range chart. Documents saved before its forming bar colours draw the bar in progress in
    /// the candle body colours, as they were drawn.
    #[serde(default = "RangeStyle::saved_without_style")]
    pub range: RangeStyle,
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
            line_gradient: false,
            line_gradient_start: DESIGN_TOKENS.semantic.brand.accent,
            line_gradient_end: DESIGN_TOKENS.semantic.extended.purple,
            step_line: LinePlotStyle::default(),
            line_with_markers: LinePlotStyle::default(),
            marker_radius: 3.0,
            area: AreaStyle::default(),
            baseline: BaselineStyle::default(),
            line_price_source: None,
            step_line_price_source: None,
            line_with_markers_price_source: None,
            area_price_source: None,
            baseline_price_source: None,
            hlc_area: HlcAreaStyle::default(),
            candles: CandlesStyle::default(),
            hollow: HollowStyle::default(),
            volume_candles: VolumeCandlesStyle::default(),
            heikin: HeikinStyle::default(),
            bars: BarsStyle::default(),
            columns: ColumnsStyle::default(),
            high_low: HighLowStyle::default(),
            renko: RenkoStyle::default(),
            line_break: LineBreakStyle::default(),
            kagi: KagiStyle::default(),
            point_figure: PointFigureStyle::default(),
            range: RangeStyle::default(),
            crosshair_marker_radius: 0.0,
            crosshair_marker_fill: None,
            crosshair_marker_border_color: None,
            crosshair_marker_border_width: DESIGN_TOKENS.stroke.thick,
            last_price_animation: LastPriceAnimation::Disabled,
        }
    }
}

impl ChartTypeStyles {
    /// The price `chart_type` draws: a Line, Step line, Line with markers, Area or Baseline
    /// chart its own source, or `shared` while that is `None`; any other chart type `shared`.
    #[cfg(feature = "widget")]
    pub(crate) fn line_source(
        &self,
        chart_type: super::ChartType,
        shared: PriceSource,
    ) -> PriceSource {
        use super::ChartType as Kind;
        match chart_type {
            Kind::Line => self.line_price_source,
            Kind::StepLine => self.step_line_price_source,
            Kind::LineWithMarkers => self.line_with_markers_price_source,
            Kind::Area => self.area_price_source,
            Kind::Baseline => self.baseline_price_source,
            _ => None,
        }
        .unwrap_or(shared)
    }

    /// Makes the Line, Step line, Line with markers, Area or Baseline `chart_type` draw `source`,
    /// `shared` being what it draws without a source of its own. A type that already draws
    /// `source` is left as it is, and so is any other chart type.
    #[cfg(feature = "widget")]
    pub(crate) fn set_line_source(
        &mut self,
        chart_type: super::ChartType,
        source: PriceSource,
        shared: PriceSource,
    ) {
        use super::ChartType as Kind;
        if self.line_source(chart_type, shared) == source {
            return;
        }
        let own = match chart_type {
            Kind::Line => &mut self.line_price_source,
            Kind::StepLine => &mut self.step_line_price_source,
            Kind::LineWithMarkers => &mut self.line_with_markers_price_source,
            Kind::Area => &mut self.area_price_source,
            Kind::Baseline => &mut self.baseline_price_source,
            _ => return,
        };
        *own = Some(source);
    }

    /// The styles of a document saved before chart types had styles of their own.
    pub(super) fn saved_without_styles() -> Self {
        Self {
            volume_candles: VolumeCandlesStyle::saved_without_style(),
            heikin: HeikinStyle::saved_without_style(),
            baseline: BaselineStyle::saved_flat(),
            columns: ColumnsStyle::saved_without_colors(),
            high_low: HighLowStyle::saved_without_style(),
            kagi: KagiStyle::saved_without_colors(),
            point_figure: PointFigureStyle::saved_without_style(),
            range: RangeStyle::saved_without_style(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BarColors;

    #[test]
    fn saved_documents_keep_flat_baseline_fills_and_candle_coloured_columns() {
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

        // A Columns entry, styles and parameters saved before the column colours.
        let empty = || serde_json::json!({});
        let entry: ColumnsStyle =
            serde_json::from_value(serde_json::json!({"price_source": "Close"})).unwrap();
        let styles: ChartTypeStyles = serde_json::from_value(empty()).unwrap();
        let params: crate::model::ChartTypeParams = serde_json::from_value(empty()).unwrap();
        for (saved, columns) in [
            ("entry", entry),
            ("styles", styles.columns),
            ("parameters", params.styles.columns),
        ] {
            assert_eq!(
                [columns.up_color, columns.down_color],
                [Color32::PLACEHOLDER; 2],
                "{saved}"
            );
        }

        // A Baseline entry, styles and parameters saved without the fills draw them as they
        // were drawn: flat, rgb(8, 153, 129) and rgb(242, 54, 69) at alpha 30, premultiplied.
        let entry: BaselineStyle = serde_json::from_value(empty()).unwrap();
        for (saved, baseline) in [
            ("entry", entry),
            ("styles", styles.baseline),
            ("parameters", params.styles.baseline),
        ] {
            let baseline = serde_json::to_value(&baseline).unwrap();
            assert_eq!(
                baseline["above_fill"],
                serde_json::json!([1, 18, 15, 30]),
                "{saved}"
            );
            assert_eq!(
                baseline["below_fill"],
                serde_json::json!([28, 6, 8, 30]),
                "{saved}"
            );
            for key in ["above_fill_at_base", "below_fill_at_base"] {
                assert!(baseline[key].is_null(), "{saved} {key}");
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
