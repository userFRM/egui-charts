//! Drawing options and configuration types.
//!
//! Contains styling enums ([`LineStyle`](crate::model::LineStyle), [`ArrowStyle`], [`FontWeight`]),
//! handle positions ([`HandlePos`]),
//! per-timeframe visibility ([`TimeframeVisibility`]), Fibonacci level
//! configuration ([`FibonacciConfig`], [`FibonacciLevel`]), and the settings of
//! the volume profile, regression trend and position drawings
//! ([`VolumeProfileConfig`](crate::data::VolumeProfileConfig), [`RegressionTrendConfig`],
//! [`PositionSize`], [`PositionSizing`], [`PositionReadout`]), and the marker settings of pins
//! and signposts ([`AnnotationMarker`]).

use serde::{Deserialize, Serialize};
use std::cell::Cell;

/// Identifies a selection handle position on a drawing.
///
/// Different tool types expose different handle layouts. Two-point tools use
/// `Start` and `End`; rectangles use four corner and four edge handles;
/// multi-point tools use indexed `Point(i)` handles; icons, emoji and stickers add `Rotate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HandlePos {
    /// First anchor point of the drawing.
    Start,
    /// Last anchor point of the drawing.
    End,
    /// Whole-drawing move, also used internally for body drags.
    Middle,
    /// Top-left corner (used by rectangle and similar axis-aligned shapes).
    TopLeft,
    /// Top-right corner.
    TopRight,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom-right corner.
    BottomRight,
    /// Top edge midpoint.
    Top,
    /// Bottom edge midpoint.
    Bottom,
    /// Left edge midpoint.
    Left,
    /// Right edge midpoint.
    Right,
    /// Rotation handle beyond the top edge of an icon, emoji or sticker.
    Rotate,
    /// Indexed anchor point for multi-point drawings (e.g., `Point(2)` is the
    /// third point in an XABCD pattern).
    Point(usize),
}

/// Arrow/arrowhead style for line drawings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ArrowStyle {
    /// No arrowhead
    None,
    /// Standard filled arrow (default)
    #[default]
    Arrow,
    /// Open/outline arrow
    OpenArrow,
    /// Circle marker
    Circle,
    /// Square marker
    Square,
    /// Diamond marker
    Diamond,
}

impl ArrowStyle {
    /// Check if this style requires any rendering
    #[inline]
    pub fn is_visible(&self) -> bool {
        !matches!(self, ArrowStyle::None)
    }

    /// Get the marker size multiplier relative to stroke width
    #[inline]
    pub fn size_multiplier(&self) -> f32 {
        match self {
            ArrowStyle::None => 0.0,
            ArrowStyle::Arrow | ArrowStyle::OpenArrow => 4.0,
            ArrowStyle::Circle => 3.0,
            ArrowStyle::Square => 3.0,
            ArrowStyle::Diamond => 3.5,
        }
    }

    /// All available arrow styles
    pub fn all() -> &'static [ArrowStyle] {
        &[
            ArrowStyle::None,
            ArrowStyle::Arrow,
            ArrowStyle::OpenArrow,
            ArrowStyle::Circle,
            ArrowStyle::Square,
            ArrowStyle::Diamond,
        ]
    }
}

pub use crate::config::FontWeight;
pub use crate::model::TimeframeVisibility;

/// Configuration for a single Fibonacci level
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FibonacciLevel {
    /// The ratio value (e.g., 0.236, 0.382, 0.618)
    pub value: f32,
    /// Legacy custom percentage label. Empty derives the label from `value`.
    #[serde(default)]
    pub label: String,
    /// Custom retracement or extension text beside this level, independent of its numeric label.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// Line width in points; absent inherits the drawing's level width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    /// Line pattern; absent inherits the drawing's level pattern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_style: Option<crate::model::LineStyle>,
    /// Level color as RGBA
    pub color: [u8; 4],
    /// Whether this level is visible
    pub visible: bool,
    /// Legacy price visibility, used when the drawing leaves `show_prices` unset.
    pub show_price: bool,
}

impl FibonacciLevel {
    /// Creates a new Fibonacci level with the given ratio, display label, and color.
    ///
    /// The level is visible by default with price display enabled.
    pub fn new(value: f32, label: &str, color: [u8; 4]) -> Self {
        Self {
            value,
            label: label.to_string(),
            text: String::new(),
            width: None,
            line_style: None,
            color,
            visible: true,
            show_price: true,
        }
    }

    /// Label derived from the current ratio. Saved nonnumeric custom labels remain in percentage mode.
    pub fn label(&self, as_percent: bool) -> String {
        let numeric = self
            .label
            .strip_suffix('%')
            .is_some_and(|v| v.parse::<f32>().is_ok());
        if as_percent && !self.label.is_empty() && !numeric {
            return self.label.clone();
        }
        let text = if as_percent {
            format!("{:.4}", f64::from(self.value) * 100.0)
        } else {
            format!("{:.6}", self.value)
        };
        let text = text.trim_end_matches('0').trim_end_matches('.');
        if as_percent {
            format!("{text}%")
        } else {
            text.to_owned()
        }
    }
}

/// Configuration for all Fibonacci levels in a drawing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FibonacciConfig {
    /// Individual level configurations
    pub levels: Vec<FibonacciLevel>,
    /// Width of level lines without an assigned width, in points.
    #[serde(default = "fib_level_width")]
    pub level_width: f32,
    /// Pattern of level lines without an assigned pattern.
    #[serde(default)]
    pub level_style: crate::model::LineStyle,
    /// Show background fill between levels
    pub show_background: bool,
    /// Background opacity (0-255)
    pub background_opacity: u8,
    /// Extend lines to the left
    pub extend_left: bool,
    /// Extend lines to the right
    pub extend_right: bool,
    /// Reverse the direction (flip vertically)
    pub reverse: bool,
    /// Show prices in level labels. `None` retains the tool's legacy choice:
    /// inherited from each level.
    #[serde(default)]
    pub show_prices: Option<bool>,
    /// Show the level value in labels; enabled when unset.
    #[serde(default)]
    pub show_levels: Option<bool>,
    /// Format level values as percentages rather than ratios.
    #[serde(default = "percent_levels")]
    pub levels_as_percent: bool,
    /// Place labels at the left end of level lines instead of the right.
    #[serde(default)]
    pub labels_on_left: bool,
    /// Font size of level labels and custom text, in points (1 through 256).
    #[serde(default = "fib_font_size")]
    pub label_size: f32,
    /// Retracement, extension and channel labels sit outside the Left/Right endpoint or midway at Center.
    /// Time projections place labels left of, on, or right of each vertical level.
    /// Absent follows `labels_on_left` for earlier drawings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_position: Option<LineTextHorizontalAlign>,
    /// Edge of a retracement, extension or channel label resting on its level; middle places it on the line.
    #[serde(default = "fib_vertical_align")]
    pub label_vertical: LineTextVerticalAlign,
    /// Time projection labels at the top, middle or bottom of the pane; defaults to bottom.
    #[serde(default = "fib_time_label_vertical")]
    pub time_label_vertical: LineTextVerticalAlign,
    /// Whether custom retracement or extension text is visible, independently of prices and ratios.
    #[serde(default = "percent_levels")]
    pub show_text: bool,
    /// Custom text sits outside retracement endpoints and inside extension endpoints;
    /// Center places it midway along the level.
    #[serde(default)]
    pub text_position: LineTextHorizontalAlign,
    /// Edge of the custom retracement or extension text resting on its level.
    #[serde(default = "fib_vertical_align")]
    pub text_vertical: LineTextVerticalAlign,
    /// Show the construction line between anchors, using the drawing's line style.
    #[serde(default = "percent_levels")]
    pub trend_visible: bool,
    /// Complete resistance arcs into circles, including their shaded bands.
    #[serde(default)]
    pub close_arcs: bool,
    /// Grow a spiral counterclockwise in screen space instead of clockwise.
    #[serde(default)]
    pub spiral_counterclockwise: bool,
    /// Measure retracement and extension levels in logarithmic price when all anchors are positive.
    #[serde(default)]
    pub logarithmic: bool,
    /// Time levels of a speed resistance fan, read along the time axis as `levels` are along the
    /// price axis. Absent on fans saved before the two families were separate, which share
    /// `levels` and draw as they did then, whatever the grid, label side and Reverse settings;
    /// see [`FibonacciConfig::fan_time_levels`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_levels: Option<Vec<FibonacciLevel>>,
    /// Whether a speed resistance fan draws its grid: a line through each visible level inside
    /// the anchors' box.
    #[serde(default = "percent_levels")]
    pub grid_visible: bool,
    /// Colour of the speed resistance fan grid as RGBA.
    #[serde(default = "fib_grid_color")]
    pub grid_color: [u8; 4],
    /// Width of the speed resistance fan grid lines, in points.
    #[serde(default = "fib_grid_width")]
    pub grid_width: f32,
    /// Line pattern of the speed resistance fan grid.
    #[serde(default)]
    pub grid_style: crate::model::LineStyle,
    /// Speed resistance fan price level ratios on the left of the anchors' box.
    #[serde(default = "percent_levels")]
    pub left_labels: bool,
    /// Speed resistance fan price level ratios on the right of the anchors' box.
    #[serde(default = "percent_levels")]
    pub right_labels: bool,
    /// Speed resistance fan time level ratios above the anchors' box.
    #[serde(default = "percent_levels")]
    pub top_labels: bool,
    /// Speed resistance fan time level ratios below the anchors' box.
    #[serde(default = "percent_levels")]
    pub bottom_labels: bool,
}

fn fib_grid_color() -> [u8; 4] {
    [21, 56, 153, 204]
}

fn fib_grid_width() -> f32 {
    1.0
}

fn fib_font_size() -> f32 {
    12.0
}

fn fib_vertical_align() -> LineTextVerticalAlign {
    LineTextVerticalAlign::Middle
}

fn fib_time_label_vertical() -> LineTextVerticalAlign {
    LineTextVerticalAlign::Bottom
}

fn fib_level_width() -> f32 {
    2.0
}

fn percent_levels() -> bool {
    true
}

impl Default for FibonacciConfig {
    /// The levels a new Fibonacci retracement starts with.
    fn default() -> Self {
        Self::for_tool(super::DrawingToolType::FibonacciRetracement)
    }
}

impl FibonacciConfig {
    /// The levels a new trend-based Fibonacci extension starts with.
    pub fn extension_default() -> Self {
        Self::for_tool(super::DrawingToolType::FibonacciExtension)
    }

    /// The presentation of retracements and channels saved without a config: seven levels with
    /// the background off.
    pub(crate) fn legacy() -> Self {
        Self {
            levels: vec![
                FibonacciLevel::new(0.0, "0%", [120, 123, 134, 255]),
                FibonacciLevel::new(0.236, "23.6%", [242, 54, 69, 255]),
                FibonacciLevel::new(0.382, "38.2%", [255, 152, 0, 255]),
                FibonacciLevel::new(0.5, "50%", [76, 175, 80, 255]),
                FibonacciLevel::new(0.618, "61.8%", [33, 150, 243, 255]),
                FibonacciLevel::new(0.786, "78.6%", [156, 39, 176, 255]),
                FibonacciLevel::new(1.0, "100%", [120, 123, 134, 255]),
            ],
            level_width: fib_level_width(),
            level_style: crate::model::LineStyle::Solid,
            show_background: false,
            background_opacity: 30,
            extend_left: false,
            extend_right: false,
            reverse: false,
            logarithmic: false,
            show_prices: None,
            show_levels: None,
            levels_as_percent: false,
            labels_on_left: true,
            label_size: fib_font_size(),
            label_position: None,
            label_vertical: fib_vertical_align(),
            time_label_vertical: fib_time_label_vertical(),
            show_text: true,
            text_position: LineTextHorizontalAlign::Center,
            text_vertical: fib_vertical_align(),
            trend_visible: true,
            time_levels: None,
            grid_visible: true,
            grid_color: fib_grid_color(),
            grid_width: fib_grid_width(),
            grid_style: crate::model::LineStyle::Solid,
            left_labels: true,
            right_labels: true,
            top_labels: true,
            bottom_labels: true,
            close_arcs: false,
            spiral_counterclockwise: false,
        }
    }

    /// The levels of extensions saved without a config.
    pub(crate) fn legacy_extension() -> Self {
        Self {
            levels: vec![
                FibonacciLevel::new(0.0, "0%", [120, 123, 134, 255]),
                FibonacciLevel::new(0.618, "61.8%", [33, 150, 243, 255]),
                FibonacciLevel::new(1.0, "100%", [120, 123, 134, 255]),
                FibonacciLevel::new(1.272, "127.2%", [242, 54, 69, 255]),
                FibonacciLevel::new(1.618, "161.8%", [255, 152, 0, 255]),
                FibonacciLevel::new(2.0, "200%", [76, 175, 80, 255]),
                FibonacciLevel::new(2.618, "261.8%", [156, 39, 176, 255]),
            ],
            ..Self::legacy()
        }
    }

    /// Initial levels for a tool as ratio, colour and visibility, with the background on at 20%
    /// except on time zones. Hidden levels are listed so they can be switched on without typing.
    /// Drawings saved without a config keep their old levels; new tools store this table when
    /// constructed.
    pub fn for_tool(tool: super::DrawingToolType) -> Self {
        use super::DrawingToolType as T;
        use super::drawing::{
            BLUE, CYAN, DEEP_PURPLE, GREEN, GREY, ORANGE, PINK, PURPLE, RED, TEAL,
        };
        let levels: Vec<(f32, [u8; 4], bool)> = match tool {
            T::FibonacciSpiral => Vec::new(),
            T::FibonacciTimeZones => [0.0, 1.0, 2.0, 3.0, 5.0, 8.0, 13.0, 21.0, 34.0, 55.0, 89.0]
                .map(|value| (value, if value == 0.0 { GREY } else { BLUE }, true))
                .to_vec(),
            T::TrendBasedFibTime => vec![
                (0.0, GREY, true),
                (0.382, RED, true),
                (0.5, [129, 199, 132, 255], false),
                (0.618, GREEN, true),
                (1.0, TEAL, true),
                (1.382, CYAN, true),
                (1.618, GREY, true),
                (2.0, BLUE, true),
                (2.382, PINK, true),
                (2.618, PURPLE, true),
                (3.0, DEEP_PURPLE, true),
            ],
            T::FibonacciSpeedFan => vec![
                (0.0, GREY, true),
                (0.25, ORANGE, true),
                (0.382, CYAN, true),
                (0.5, GREEN, true),
                (0.618, TEAL, true),
                (0.75, BLUE, true),
                (1.0, GREY, true),
            ],
            T::FibonacciCircles | T::FibonacciSpeedResistanceArcs => vec![
                (0.236, RED, true),
                (0.382, ORANGE, true),
                (0.5, TEAL, true),
                (0.618, GREEN, true),
                (0.786, CYAN, true),
                (1.0, GREY, true),
                (1.618, BLUE, true),
                (2.618, PINK, true),
                (3.618, BLUE, true),
                (4.236, PINK, true),
                (4.618, RED, true),
            ],
            T::FibonacciWedge => vec![
                (0.236, RED, true),
                (0.382, ORANGE, true),
                (0.5, GREEN, true),
                (0.618, TEAL, true),
                (0.786, CYAN, true),
                (1.0, GREY, true),
                (1.618, BLUE, false),
                (2.618, RED, false),
                (3.618, DEEP_PURPLE, false),
                (4.236, PINK, false),
                (4.618, PINK, false),
            ],
            T::FibonacciRetracement | T::FibonacciExtension | T::FibonacciChannel => vec![
                (0.0, GREY, true),
                (0.236, RED, true),
                (0.382, ORANGE, true),
                (0.5, GREEN, true),
                (0.618, TEAL, true),
                (0.786, CYAN, true),
                (1.0, GREY, true),
                (1.618, BLUE, true),
                (2.618, RED, true),
                (3.618, PURPLE, true),
                (4.236, PINK, true),
                (1.272, ORANGE, false),
                (1.414, RED, false),
                (2.272, ORANGE, false),
                (2.414, GREEN, false),
                (2.0, TEAL, false),
                (3.0, CYAN, false),
                (3.272, GREY, false),
                (3.414, BLUE, false),
                (4.0, RED, false),
                (4.272, PURPLE, false),
                (4.414, PINK, false),
                (4.618, ORANGE, false),
                (4.764, TEAL, false),
            ],
            _ => return Self::default(),
        };
        let levels: Vec<_> = levels
            .into_iter()
            .map(|(value, color, visible)| FibonacciLevel {
                visible,
                ..FibonacciLevel::new(value, "", color)
            })
            .collect();
        Self {
            time_levels: (tool == T::FibonacciSpeedFan).then(|| levels.clone()),
            levels,
            show_background: !matches!(tool, T::FibonacciTimeZones | T::FibonacciSpiral),
            background_opacity: 51,
            show_levels: (tool == T::FibonacciWedge).then_some(true),
            labels_on_left: !matches!(tool, T::FibonacciTimeZones | T::TrendBasedFibTime),
            ..Self::legacy()
        }
    }

    /// Whether ratios, line widths and typography are finite and within drawing limits.
    pub fn is_valid(&self) -> bool {
        let width = |value: f32| value > 0.0 && value <= super::super::validation::MAX_STROKE_WIDTH;
        width(self.level_width)
            && width(self.grid_width)
            && (1.0..=256.0).contains(&self.label_size)
            && self
                .levels
                .iter()
                .chain(self.time_levels.iter().flatten())
                .all(|level| level.value.is_finite() && level.width.is_none_or(width))
    }

    /// The time levels of a speed resistance fan: its own, or `levels` on fans saved before it
    /// had them.
    pub fn fan_time_levels(&self) -> &[FibonacciLevel] {
        self.time_levels.as_deref().unwrap_or(&self.levels)
    }

    /// Apply just the fields edited against `original`, keeping the rest of this drawing's
    /// settings. Adding or removing a level replaces the ordered table intentionally.
    #[cfg(feature = "ui")]
    pub(crate) fn apply_edits(&mut self, original: &Self, edited: &Self) {
        macro_rules! fields {
            ($target:expr, $original:expr, $edited:expr, $($field:ident),+ $(,)?) => {
                $(if $original.$field != $edited.$field {
                    $target.$field.clone_from(&$edited.$field);
                })+
            };
        }
        fields!(
            self,
            original,
            edited,
            level_width,
            level_style,
            show_background,
            background_opacity,
            extend_left,
            extend_right,
            reverse,
            show_prices,
            show_levels,
            levels_as_percent,
            labels_on_left,
            logarithmic,
            label_size,
            label_position,
            label_vertical,
            time_label_vertical,
            show_text,
            text_position,
            text_vertical,
            trend_visible,
            grid_visible,
            grid_color,
            grid_width,
            grid_style,
            left_labels,
            right_labels,
            top_labels,
            bottom_labels,
            close_arcs,
            spiral_counterclockwise
        );
        // A fan still sharing its price levels keeps its time levels apart from this edit.
        if self.time_levels.is_none() && edited.time_levels.is_some() && original != edited {
            self.time_levels = Some(self.levels.clone());
        }
        let apply = |target: &mut Vec<FibonacciLevel>,
                     original: &[FibonacciLevel],
                     edited: &[FibonacciLevel]| {
            if original.len() == edited.len() {
                for ((target, original), edited) in target.iter_mut().zip(original).zip(edited) {
                    fields!(
                        target, original, edited, value, label, color, visible, show_price, text,
                        width, line_style
                    );
                }
            } else {
                *target = edited.to_vec();
            }
        };
        if let Some(target) = &mut self.time_levels {
            apply(target, original.fan_time_levels(), edited.fan_time_levels());
        }
        apply(&mut self.levels, &original.levels, &edited.levels);
    }

    /// Label content; a tool without a price passes `None`.
    pub(crate) fn label_text(&self, level: &FibonacciLevel, price: Option<&str>) -> String {
        if self.show_prices.is_none()
            && self.show_levels.is_none()
            && !level.show_price
            && level.label.is_empty()
        {
            return String::new();
        }
        let value = self
            .show_levels
            .unwrap_or(true)
            .then(|| level.label(self.levels_as_percent));
        let price = price.filter(|_| self.show_prices.unwrap_or(level.show_price));
        match (value, price) {
            (Some(value), Some(price)) => format!("{value} ({price})"),
            (Some(value), None) => value,
            (None, Some(price)) => price.to_owned(),
            (None, None) => String::new(),
        }
    }

    /// Get visible levels only
    pub fn visible_levels(&self) -> impl Iterator<Item = &FibonacciLevel> {
        self.levels.iter().filter(|l| l.visible)
    }

    /// Position of retracement level `value` between the first anchor `start`
    /// and the second anchor `end`, in price units.
    ///
    /// Level 0 sits on the second anchor and level 1 on the first, so levels
    /// measure back from the end of the move; `reverse` swaps the two anchors.
    pub(crate) fn retracement_position(&self, start: f64, end: f64, value: f64) -> f64 {
        let (from, to) = if self.reverse {
            (start, end)
        } else {
            (end, start)
        };
        if self.logarithmic && from > 0.0 && to > 0.0 {
            (from.ln() + (to.ln() - from.ln()) * value).exp()
        } else {
            from + (to - from) * value
        }
    }

    /// Price of each visible level, in config order, between the anchor prices `start` and `end`:
    /// a retracement's as [`Self::retracement_position`] places them, an extension's projected
    /// from `origin` by `end - start`, or by the log swing when all three prices are positive.
    pub(crate) fn level_prices(
        &self,
        start: f64,
        end: f64,
        extension: bool,
        origin: f64,
    ) -> impl Iterator<Item = f64> + '_ {
        self.visible_levels().map(move |level| {
            let value = f64::from(level.value);
            if extension {
                let value = if self.reverse { -value } else { value };
                if self.logarithmic && start > 0.0 && end > 0.0 && origin > 0.0 {
                    (origin.ln() + (end.ln() - start.ln()) * value).exp()
                } else {
                    origin + (end - start) * value
                }
            } else {
                self.retracement_position(start, end, value)
            }
        })
    }
}

/// Settings of the regression trend drawing.
///
/// ```
/// use egui_charts::drawings::{RegressionBandLine, RegressionTrendConfig};
/// use egui_charts::model::{LineStyle, PriceSource};
///
/// let mut config = RegressionTrendConfig::default()
///     .with_deviations(1.5, 2.5)
///     .unwrap();
/// config.use_upper = false;
/// config.source = PriceSource::HL2;
/// config.line_style = LineStyle::Solid;
/// config.show_base_line = true;
/// config.upper_line = RegressionBandLine::new([242, 54, 69, 255])
///     .with_width(Some(2.0))
///     .unwrap();
/// config.lower_line.visible = false;
/// config.show_correlation = false;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RegressionTrendFields")]
pub struct RegressionTrendConfig {
    upper: f64,
    lower: f64,
    /// Whether the upper bound is the upper deviation away from the fitted line; otherwise it
    /// passes through the high furthest above the line.
    pub use_upper: bool,
    /// Whether the lower bound is the lower deviation away from the fitted line; otherwise it
    /// passes through the low furthest below the line.
    pub use_lower: bool,
    /// Bar price the line is fitted to.
    pub source: crate::model::PriceSource,
    /// Pattern of the fitted line.
    pub line_style: crate::model::LineStyle,
    /// Whether the fitted line is drawn, in the drawing's colour and width and in
    /// [`Self::line_style`]. The drawing's text is placed along it either way.
    pub show_base_line: bool,
    /// The line through the upper bound.
    pub upper_line: RegressionBandLine,
    /// The line through the lower bound.
    pub lower_line: RegressionBandLine,
    /// Whether the Pearson correlation of the fitted prices is written below the start of the
    /// lower line.
    pub show_correlation: bool,
    precision: Option<u8>,
}

/// Stored settings of a regression trend, checked by [`RegressionTrendConfig::with_deviations`]
/// and [`RegressionTrendConfig::with_precision`] on load. Settings saved before the bounds were
/// set apart hold one `deviations` for both.
#[derive(Deserialize)]
struct RegressionTrendFields {
    deviations: Option<f64>,
    upper: Option<f64>,
    lower: Option<f64>,
    use_upper: Option<bool>,
    use_lower: Option<bool>,
    source: Option<crate::model::PriceSource>,
    line_style: Option<crate::model::LineStyle>,
    show_base_line: Option<bool>,
    upper_line: Option<RegressionBandLine>,
    lower_line: Option<RegressionBandLine>,
    show_correlation: Option<bool>,
    precision: Option<u8>,
}

impl TryFrom<RegressionTrendFields> for RegressionTrendConfig {
    type Error = &'static str;

    fn try_from(fields: RegressionTrendFields) -> Result<Self, Self::Error> {
        let default = Self::default();
        let both = fields.deviations.unwrap_or(default.upper);
        let config = Self {
            use_upper: fields.use_upper.unwrap_or(default.use_upper),
            use_lower: fields.use_lower.unwrap_or(default.use_lower),
            source: fields.source.unwrap_or(default.source),
            line_style: fields.line_style.unwrap_or(default.line_style),
            show_base_line: fields.show_base_line.unwrap_or(default.show_base_line),
            upper_line: fields.upper_line.unwrap_or(default.upper_line),
            lower_line: fields.lower_line.unwrap_or(default.lower_line),
            show_correlation: fields.show_correlation.unwrap_or(default.show_correlation),
            ..default
        };
        config
            .with_deviations(fields.upper.unwrap_or(both), fields.lower.unwrap_or(both))
            .and_then(|config| config.with_precision(fields.precision))
            .ok_or("regression trend settings out of bounds")
    }
}

impl RegressionTrendConfig {
    /// Bands `deviations` standard deviations either side of the fitted line
    /// (above 0, at most 10), with the other settings at their defaults; `None` when out of
    /// bounds.
    pub fn new(deviations: f64) -> Option<Self> {
        Self::default().with_deviations(deviations, deviations)
    }

    /// These settings with the upper bound `upper` and the lower bound `lower` standard
    /// deviations from the fitted line, each above 0 and at most 10; `None` when out of bounds.
    pub fn with_deviations(self, upper: f64, lower: f64) -> Option<Self> {
        let valid = |d: f64| d > 0.0 && d <= 10.0;
        (valid(upper) && valid(lower)).then_some(Self {
            upper,
            lower,
            ..self
        })
    }

    /// Distance of the upper bound above the fitted line, in standard deviations, while
    /// [`Self::use_upper`] is set.
    pub fn upper_deviation(&self) -> f64 {
        self.upper
    }

    /// Distance of the lower bound below the fitted line, in standard deviations, while
    /// [`Self::use_lower`] is set.
    pub fn lower_deviation(&self) -> f64 {
        self.lower
    }

    /// These settings with `places` decimals, at most 8, on the correlation label; `None`
    /// shows two. Returns `None` above 8.
    pub fn with_precision(self, places: Option<u8>) -> Option<Self> {
        places.is_none_or(|p| p <= 8).then_some(Self {
            precision: places,
            ..self
        })
    }

    /// Decimals of the correlation label; `None` shows two.
    pub fn precision(&self) -> Option<u8> {
        self.precision
    }

    /// The distances of the upper and lower bounds in standard deviations, `None` for a side
    /// bounded by its extreme price.
    pub(crate) fn band_deviations(&self) -> [Option<f64>; 2] {
        [
            self.use_upper.then_some(self.upper),
            self.use_lower.then_some(self.lower),
        ]
    }
}

impl RegressionTrendConfig {
    /// Settings for a new drawing: the defaults with 2 px accent blue bands at 30% opacity.
    pub fn drawing_default() -> Self {
        let band = RegressionBandLine::new([41, 98, 255, 77])
            .with_width(Some(2.0))
            .expect("2 px is a valid width");
        Self {
            upper_line: band,
            lower_line: band,
            ..Self::default()
        }
    }
}

impl Default for RegressionTrendConfig {
    /// A dashed line fitted to the closes in the drawing's colour and width, solid bands two
    /// standard deviations away in the accent blue at the drawing's width, and a correlation label
    /// of two decimals.
    fn default() -> Self {
        Self {
            upper: 2.0,
            lower: 2.0,
            use_upper: true,
            use_lower: true,
            source: crate::model::PriceSource::Close,
            line_style: crate::model::LineStyle::Dashed,
            show_base_line: true,
            upper_line: RegressionBandLine::new(super::drawing::BLUE),
            lower_line: RegressionBandLine::new(super::drawing::BLUE),
            show_correlation: true,
            precision: None,
        }
    }
}

/// One band line of a regression trend: whether it is drawn, and its colour, pattern and width.
///
/// ```
/// use egui_charts::drawings::RegressionBandLine;
/// use egui_charts::model::LineStyle;
///
/// let mut line = RegressionBandLine::new([242, 54, 69, 128])
///     .with_width(Some(3.0))
///     .unwrap();
/// line.line_style = LineStyle::Dotted;
/// line.visible = false;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RegressionBandLineFields")]
pub struct RegressionBandLine {
    /// Whether the line is drawn. A hidden line still bounds the fill.
    pub visible: bool,
    /// Colour, whose alpha is the line's opacity.
    pub color: [u8; 4],
    /// Pattern of the line.
    pub line_style: crate::model::LineStyle,
    width: Option<f32>,
}

/// Stored settings of a band line, checked by [`RegressionBandLine::with_width`] on load.
#[derive(Deserialize)]
struct RegressionBandLineFields {
    visible: bool,
    color: [u8; 4],
    line_style: crate::model::LineStyle,
    width: Option<f32>,
}

impl TryFrom<RegressionBandLineFields> for RegressionBandLine {
    type Error = &'static str;

    fn try_from(fields: RegressionBandLineFields) -> Result<Self, Self::Error> {
        Self {
            visible: fields.visible,
            line_style: fields.line_style,
            ..Self::new(fields.color)
        }
        .with_width(fields.width)
        .ok_or("regression trend line width out of bounds")
    }
}

impl RegressionBandLine {
    /// A shown, solid line in `color`, as wide as the drawing's line.
    pub fn new(color: [u8; 4]) -> Self {
        Self {
            visible: true,
            color,
            line_style: crate::model::LineStyle::Solid,
            width: None,
        }
    }

    /// The line at `width` px, above 0 and at most 100, or as wide as the drawing's line for
    /// `None`. Returns `None` when out of bounds.
    pub fn with_width(self, width: Option<f32>) -> Option<Self> {
        width
            .is_none_or(|w| w > 0.0 && w <= crate::drawings::validation::MAX_STROKE_WIDTH)
            .then_some(Self { width, ..self })
    }

    /// Width in px, or `None` while the line is as wide as the drawing's line.
    pub fn width(&self) -> Option<f32> {
        self.width
    }
}

/// Instrument economics used by long and short position drawings.
/// Quantities are lots; one lot contains `lot_size` units, each worth `point_value`
/// per price point in the account currency, or in the instrument currency when
/// [`PositionReadout::account_currency`] requests conversion. Defaults retain unit sizing
/// without rounding.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct PositionSizing {
    /// Account-currency value of one price point per underlying unit.
    pub point_value: f64,
    /// Underlying units per displayed lot.
    pub lot_size: f64,
    /// Permitted lot increment; zero leaves the quantity unrounded.
    pub quantity_step: f64,
    /// Notional value divided by required capital; positive and finite.
    pub leverage: f64,
}

impl Default for PositionSizing {
    fn default() -> Self {
        Self {
            point_value: 1.0,
            lot_size: 1.0,
            quantity_step: 0.0,
            leverage: 1.0,
        }
    }
}

impl PositionSizing {
    /// Whether the economics are finite and positive, with a nonnegative quantity step.
    pub fn is_valid(&self) -> bool {
        [
            self.point_value,
            self.lot_size,
            self.leverage,
            self.point_value * self.lot_size,
        ]
        .into_iter()
        .all(|v| v.is_finite() && v > 0.0)
            && self.quantity_step.is_finite()
            && self.quantity_step >= 0.0
    }

    /// Required capital: absolute entry notional divided by leverage, in account currency.
    /// Returns `None` for invalid settings or a nonpositive or nonfinite quantity.
    pub fn required_capital(&self, entry: f64, quantity: f64) -> Option<f64> {
        if !self.is_valid() || !quantity.is_finite() || quantity <= 0.0 {
            return None;
        }
        let capital = entry.abs() * self.point_value * self.lot_size * quantity / self.leverage;
        capital.is_finite().then_some(capital)
    }
}

/// Statistics and account conversion for long and short position drawings.
/// New drawings show full statistics only while selected. Currency conversion is supplied
/// by the host: the library never downloads exchange rates.
///
/// ```
/// use egui_charts::drawings::PositionReadout;
///
/// let mut readout = PositionReadout::default();
/// readout.account_currency = Some("USD".into());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct PositionReadout {
    /// Keep statistics visible when the drawing is not selected.
    pub persistent: bool,
    /// Omit captions from the level figures and position result, quantity and reward/risk ratio.
    pub condensed: bool,
    /// Account currency code; `None` follows the instrument currency.
    pub account_currency: Option<String>,
    /// Account-currency units per unit of the instrument currency. Required when the account
    /// currency differs from the instrument's; `None` leaves that conversion unavailable.
    pub conversion_rate: Option<f64>,
    /// Decimal places for quantity rounding and display, from 0 through 8. `None` retains
    /// the instrument's quantity step and automatic display precision. Rounding never adds risk.
    pub quantity_decimals: Option<u8>,
}

impl PositionReadout {
    /// Whether the currency code, optional rate and quantity precision can be used safely.
    pub fn is_valid(&self) -> bool {
        self.quantity_decimals.is_none_or(|places| places <= 8)
            && self
                .conversion_rate
                .is_none_or(|rate| rate.is_finite() && rate > 0.0)
            && self.account_currency.as_ref().is_none_or(|code| {
                !code.is_empty()
                    && code.len() <= 16
                    && code.bytes().all(|c| c.is_ascii_alphanumeric())
            })
    }

    /// Resolve account economics from a host-supplied rate. Matching currencies need no rate;
    /// a missing rate for a different currency returns `None`, rather than a mislabeled result.
    /// A code that mixes cases, such as `GBp` for pence, matches only itself; other codes match
    /// regardless of case.
    pub fn account_sizing(
        &self,
        mut sizing: PositionSizing,
        instrument_currency: Option<&str>,
    ) -> Option<PositionSizing> {
        if !self.is_valid() {
            return None;
        }
        if let Some(currency) = &self.account_currency
            && !instrument_currency.is_some_and(|base| crate::model::same_currency(base, currency))
        {
            sizing.point_value *= self.conversion_rate?;
        }
        sizing.is_valid().then_some(sizing)
    }

    /// Quantity in lots after conversion and downward rounding. Decimal precision and a
    /// lot increment both apply: with quarter-lots and one decimal, valid lots are halves.
    /// An account-percent size also fits the account's available leveraged capital.
    pub fn quantity(
        &self,
        size: PositionSize,
        entry: f64,
        stop: f64,
        sizing: PositionSizing,
        instrument_currency: Option<&str>,
    ) -> Option<f64> {
        let mut sizing = if matches!(size, PositionSize::Quantity(_)) {
            (self.is_valid() && sizing.is_valid()).then_some(sizing)?
        } else {
            self.account_sizing(sizing, instrument_currency)?
        };
        if let Some(places) = self.quantity_decimals {
            let decimal_step = 10f64.powi(-i32::from(places));
            if sizing.quantity_step == 0.0 {
                sizing.quantity_step = decimal_step;
            } else {
                // A decimal lot step is m / 10^d. Its common multiple with 10^-p is
                // 10^-p * m / gcd(m, 10^(d-p)); remove only factors two and five.
                let text = sizing.quantity_step.to_string();
                let digits = text
                    .split_once('.')
                    .map_or(0, |(_, fraction)| fraction.len());
                if digits > usize::from(places) {
                    let mut units: u64 =
                        text.replace('.', "").trim_start_matches('0').parse().ok()?;
                    for factor in [2, 5] {
                        for _ in 0..digits - usize::from(places) {
                            if !units.is_multiple_of(factor) {
                                break;
                            }
                            units /= factor;
                        }
                    }
                    sizing.quantity_step = decimal_step * units as f64;
                }
            }
        }
        size.quantity_with_sizing(entry, stop, &sizing)
    }

    /// Statistics in files written before these settings were always visible.
    pub(crate) fn legacy() -> Self {
        Self {
            persistent: true,
            ..Self::default()
        }
    }
}

/// Lines, labels, zone fill and border of long and short position drawings. The target and
/// stop colours, whose opacity is that of their zones, are
/// [`Drawing::position_target_color`](crate::drawings::Drawing::position_target_color) and
/// [`Drawing::position_stop_color`](crate::drawings::Drawing::position_stop_color).
///
/// ```
/// use egui_charts::drawings::PositionStyle;
///
/// let mut style = PositionStyle::default();
/// style.line_width = 2.0;
/// assert!(style.is_valid());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct PositionStyle {
    /// Colour of the entry line, its price tag and the trade's path; `None` draws the lines in a
    /// translucent shade of the theme's secondary text colour and the tag in that colour.
    pub line_color: Option<[u8; 4]>,
    /// Width of the entry line, the trade's path and the zone border, in points.
    pub line_width: f32,
    /// Colour of the label text; `None` is white on a plate and the level or result colour
    /// without one, or the theme's secondary text on a label without a result.
    pub text_color: Option<[u8; 4]>,
    /// Size of the label text, in points.
    pub font_size: f32,
    /// Whether the labels sit on filled plates.
    pub label_background: bool,
    /// Plate colour of every label; `None` takes the target, stop or result colour.
    pub label_color: Option<[u8; 4]>,
    /// Whether the target and stop zones are filled.
    pub fill: bool,
    /// Whether the target and stop zones are outlined in [`Self::border_color`].
    pub border: bool,
    /// Colour of the zone outline.
    pub border_color: [u8; 4],
}

impl Default for PositionStyle {
    fn default() -> Self {
        Self {
            line_color: None,
            line_width: 1.0,
            text_color: None,
            font_size: 12.0,
            label_background: true,
            label_color: None,
            fill: true,
            border: false,
            border_color: [102, 123, 139, 255],
        }
    }
}

impl PositionStyle {
    /// Whether the line width lies in 1 to 100 points and the font size in 1 to 256 points.
    pub fn is_valid(&self) -> bool {
        (1.0..=100.0).contains(&self.line_width) && (1.0..=256.0).contains(&self.font_size)
    }
}

/// Colour, size and emphasis of text a drawing styles apart from its own: the text typed on a
/// price note, apart from the price beside it, and the prices beside a channel's lines.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NoteTextStyle {
    /// Text colour as RGBA.
    pub color: [u8; 4],
    /// Size of the text, in points.
    pub font_size: f32,
    /// Whether the text is bold.
    pub bold: bool,
    /// Whether the text is slanted.
    pub italic: bool,
}

impl NoteTextStyle {
    /// Text of `color` at `font_size` points, neither bold nor italic.
    ///
    /// ```
    /// use egui_charts::drawings::NoteTextStyle;
    ///
    /// let mut style = NoteTextStyle::new([255, 255, 255, 255], 14.0);
    /// style.bold = true;
    /// assert!(style.is_valid());
    /// ```
    pub fn new(color: [u8; 4], font_size: f32) -> Self {
        Self {
            color,
            font_size,
            bold: false,
            italic: false,
        }
    }

    /// Whether the font size lies in 1 to 256 points.
    pub fn is_valid(&self) -> bool {
        (1.0..=256.0).contains(&self.font_size)
    }
}

/// Colour, width and pattern of a parallel channel's middle line. The default draws it as
/// channels saved before these settings do: dashed, 1 point wide, in the line colour.
///
/// ```
/// use egui_charts::drawings::{Drawing, DrawingToolType, MiddleLineStyle};
/// use egui_charts::model::LineStyle;
///
/// let mut channel = Drawing::new(1, DrawingToolType::ParallelChannel);
/// let mut middle = MiddleLineStyle::default();
/// middle.color = Some([255, 152, 0, 255]);
/// middle.width = 2.0;
/// middle.line_style = LineStyle::Dotted;
/// channel.middle_line = middle;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct MiddleLineStyle {
    /// Line colour as RGBA; `None` takes the drawing's line colour.
    pub color: Option<[u8; 4]>,
    /// Stroke width in points, above 0 and at most 100.
    pub width: f32,
    /// Stroke pattern.
    pub line_style: crate::model::LineStyle,
}

impl Default for MiddleLineStyle {
    fn default() -> Self {
        Self {
            color: None,
            width: crate::styles::stroke::HAIRLINE,
            line_style: crate::model::LineStyle::Dashed,
        }
    }
}

/// What a pin and a signpost show at their marker: a pin's text popup, and the glyph a signpost
/// can carry on a coloured disc below its text. The text switch also shows and hides the text
/// of a horizontal line, a horizontal ray, an arrow mark and the parallel, flat top/bottom and
/// disjoint channels. Other tools ignore these settings. A new drawing shows its text and
/// leaves a signpost's glyph off.
///
/// ```
/// use egui_charts::drawings::{AnnotationMarker, Drawing, DrawingToolType};
///
/// let mut marker = AnnotationMarker::default();
/// marker.glyph = "🚀".into();
/// marker.show_glyph = true;
/// assert!(marker.is_valid());
///
/// let mut level = Drawing::new(1, DrawingToolType::HorizontalLine);
/// level.text = Some("Weekly support".into());
/// level.marker.show_text = false;
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct AnnotationMarker {
    /// Whether a pin shows its text in a popup while it is selected or under the pointer, and
    /// whether a horizontal line, a horizontal ray, an arrow mark or a parallel, flat
    /// top/bottom or disjoint channel draws its text. Switched off, the text is kept.
    pub show_text: bool,
    /// Glyph on a signpost's disc: an emoji, or the id of a bundled icon or sticker.
    pub glyph: String,
    /// Whether a signpost carries the disc and its glyph.
    pub show_glyph: bool,
    /// Colour of a signpost's disc; `None` takes the drawing colour.
    pub glyph_color: Option<[u8; 4]>,
}

impl Default for AnnotationMarker {
    fn default() -> Self {
        Self {
            show_text: true,
            glyph: "🙂".into(),
            show_glyph: false,
            glyph_color: None,
        }
    }
}

impl AnnotationMarker {
    /// Whether the glyph is named, in at most 32 bytes.
    pub fn is_valid(&self) -> bool {
        (1..=32).contains(&self.glyph.len())
    }
}

/// Placement of a line tool's measurement text relative to its anchors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LineMeasurementPosition {
    /// Before the left anchor.
    Left,
    /// Above the middle of the anchors.
    Center,
    /// After the right anchor.
    #[default]
    Right,
    /// After the right anchor if the text fits in the pane there, else before the left anchor if
    /// it fits there, else above the middle of the anchors.
    Auto,
}

/// Which part of a line's text sits at which point of the line, reading left to right. A vertical
/// line reads from bottom to top, so its left end is at the bottom, whichever way its text is turned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LineTextHorizontalAlign {
    /// The text starts at the line's left end.
    Left,
    /// The text is centred on the line's middle.
    #[default]
    Center,
    /// The text ends at the line's right end.
    Right,
}

/// Which edge of a line's text rests on the line. A vertical line takes its text's top to face
/// left, so [`Self::Top`] reads right of the line and [`Self::Bottom`] left of it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LineTextVerticalAlign {
    /// The text's top edge, so that it reads below the line.
    Top,
    /// The text's middle, so that it reads on the line.
    Middle,
    /// The text's bottom edge, so that it reads above the line.
    #[default]
    Bottom,
}

/// How a vertical line's text is turned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LineTextOrientation {
    /// Along the line, reading from bottom to top.
    #[default]
    Vertical,
    /// Upright, across the line.
    Horizontal,
}

/// Independent readouts of a trend line, ray, extended line, info line, trend angle or arrow,
/// and the placement of the text of those tools and of a horizontal line or ray or a vertical
/// line, which have no readouts. Readouts start hidden except on an info line, see
/// [`Self::for_tool`]; saves from before these settings read the same way.
/// Price changes follow anchor order; lengths and elapsed time are unsigned.
///
/// ```
/// use egui_charts::drawings::LineMeasurements;
///
/// let mut measurements = LineMeasurements::default();
/// measurements.bar_count = true;
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct LineMeasurements {
    /// Keep enabled readouts visible without selecting the line.
    pub persistent: bool,
    /// Price difference in the chart's price format.
    pub price_change: bool,
    /// Price difference as a percentage of the first anchor's price.
    pub percent_change: bool,
    /// Price difference in instrument pips, or ticks when pips are not defined.
    pub pip_change: bool,
    /// Number of bars between the anchors.
    pub bar_count: bool,
    /// Time between the anchors, using their timestamps.
    pub elapsed_time: bool,
    /// Screen length divided by the horizontal spacing of one bar, in periods.
    pub length: bool,
    /// Inclination on the displayed scale, in degrees.
    pub inclination: bool,
    /// Show a movable marker halfway along the displayed line segment.
    pub midpoint: bool,
    /// Show the price of each anchor beside it, whether or not the line is selected. On a flat
    /// top/bottom or disjoint channel, the price at each end of both lines, styled by
    /// [`Drawing::anchor_price_style`](crate::drawings::Drawing::anchor_price_style).
    pub price_labels: bool,
    /// Where to place the readouts; right of the anchors by default.
    pub position: LineMeasurementPosition,
    /// Where the line's custom text sits along the line.
    pub text_horizontal_align: LineTextHorizontalAlign,
    /// Which edge of the line's custom text rests on the line.
    pub text_vertical_align: LineTextVerticalAlign,
    /// How a vertical line's custom text is turned. Other lines write theirs along the line.
    pub text_orientation: LineTextOrientation,
    /// Whether a vertical line runs through the chart's other panes at its anchor's time, as well
    /// as its own. Off keeps it, and always its text, in its own pane; other lines ignore it.
    pub across_panes: bool,
}

thread_local! {
    /// Whether the last drawing read on this thread left out its line measurements.
    static LINE_MEASUREMENTS_LEFT_OUT: Cell<bool> = const { Cell::new(false) };
}

impl LineMeasurements {
    /// The settings a new drawing of `tool` starts with: an info line keeps every readout visible
    /// in the middle of the line, a horizontal or vertical line writes its text on the line and a
    /// vertical line runs through every pane, a parallel channel, flat top/bottom or disjoint
    /// channel starts its text at the left end of its first line, and other tools keep their
    /// readouts hidden and their text above the middle of the line.
    pub fn for_tool(tool: super::DrawingToolType) -> Self {
        if tool == super::DrawingToolType::VerticalLine {
            return Self {
                text_vertical_align: LineTextVerticalAlign::Middle,
                across_panes: true,
                ..Self::default()
            };
        }
        if tool == super::DrawingToolType::HorizontalLine {
            return Self {
                text_vertical_align: LineTextVerticalAlign::Middle,
                ..Self::default()
            };
        }
        if matches!(
            tool,
            super::DrawingToolType::ParallelChannel
                | super::DrawingToolType::FlatTopBottom
                | super::DrawingToolType::DisjointChannel
        ) {
            return Self {
                text_horizontal_align: LineTextHorizontalAlign::Left,
                ..Self::default()
            };
        }
        if tool != super::DrawingToolType::InfoLine {
            return Self::default();
        }
        Self {
            persistent: true,
            price_change: true,
            percent_change: true,
            pip_change: true,
            bar_count: true,
            elapsed_time: true,
            length: true,
            inclination: true,
            position: LineMeasurementPosition::Center,
            ..Self::default()
        }
    }

    /// Notes that the drawing being read has no line measurements, and gives the [`Default`] ones.
    pub(crate) fn left_out() -> Self {
        LINE_MEASUREMENTS_LEFT_OUT.set(true);
        Self::default()
    }

    /// Clears the note of [`Self::left_out`], before a drawing is read.
    pub(crate) fn forget_left_out() {
        LINE_MEASUREMENTS_LEFT_OUT.set(false);
    }

    /// Takes [`Self::for_tool`] of an info line whose settings the drawing just read left out, so
    /// one saved before they existed keeps the readouts it always showed; other tools keep the
    /// [`Default`] ones, which draw their text where it always was.
    pub(crate) fn fill_left_out(&mut self, tool: super::DrawingToolType) {
        if LINE_MEASUREMENTS_LEFT_OUT.replace(false) && tool == super::DrawingToolType::InfoLine {
            *self = Self::for_tool(tool);
        }
    }
}

thread_local! {
    /// Whether the last drawing read on this thread left out its range settings.
    static RANGE_SETTINGS_LEFT_OUT: Cell<bool> = const { Cell::new(false) };
}

/// Readouts, readout card, box border and vertical reach of a date range and a date and price
/// range; other tools ignore these settings. Missing saved fields keep the readouts on a card
/// in the theme's tooltip colours and the box edges in the drawing's line colour, width and
/// pattern. New drawings start from [`Self::for_tool`].
///
/// ```
/// use egui_charts::drawings::RangeSettings;
///
/// let mut settings = RangeSettings::default();
/// settings.volume = false;
/// settings.text_color = Some([242, 54, 69, 255]);
/// settings.extend_top = true;
/// assert!(settings.is_valid());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct RangeSettings {
    /// Price change from the first anchor to the second. A date range shows no price readouts.
    pub price_change: bool,
    /// Price change as a percentage of the first anchor's price.
    pub percent_change: bool,
    /// Price change in instrument pips, or ticks when pips are not defined.
    pub pip_change: bool,
    /// Number of bars between the anchors.
    pub bar_count: bool,
    /// Time between the anchors, on a time axis.
    pub elapsed_time: bool,
    /// Volume traded over the bars from one anchor to the other.
    pub volume: bool,
    /// Colour of the readouts, whose alpha is their opacity; `None` takes the theme's axis text
    /// colour.
    pub text_color: Option<[u8; 4]>,
    /// Size of the readouts, in points.
    pub font_size: f32,
    /// Whether the readouts sit on a card.
    pub background: bool,
    /// Colour of the card, whose alpha is its opacity; `None` takes the theme's tooltip colour.
    pub background_color: Option<[u8; 4]>,
    /// Colour of the card's shadow, whose alpha is its opacity. Missing saved fields are
    /// transparent; new range cards start with black at 20% opacity.
    pub shadow_color: [u8; 4],
    /// Whether the box edges are drawn: at the measured times and, on a date and price range,
    /// at the measured prices.
    pub border: bool,
    /// Colour of the box edges; `None` takes the drawing colour.
    pub border_color: Option<[u8; 4]>,
    /// Width of the box edges, in points; `None` takes the drawing's line width.
    pub border_width: Option<f32>,
    /// Pattern of the box edges; `None` takes the drawing's line style.
    pub border_style: Option<crate::model::LineStyle>,
    /// Whether the time edges of a date range or a date and price range run up to the top of
    /// the pane.
    pub extend_top: bool,
    /// Whether the time edges of a date range or a date and price range run down to the bottom
    /// of the pane.
    pub extend_bottom: bool,
}

impl Default for RangeSettings {
    fn default() -> Self {
        Self {
            price_change: true,
            percent_change: true,
            pip_change: true,
            bar_count: true,
            elapsed_time: true,
            volume: true,
            text_color: None,
            font_size: 12.0,
            background: true,
            background_color: None,
            shadow_color: [0; 4],
            border: true,
            border_color: None,
            border_width: None,
            border_style: None,
            extend_top: false,
            extend_bottom: false,
        }
    }
}

impl RangeSettings {
    /// Settings for a new range, with a black card shadow at 20% opacity. A date and price
    /// range starts without box edges; enabling them draws a 1 px border in the line colour.
    pub fn for_tool(tool: super::DrawingToolType) -> Self {
        let mut settings = Self::default();
        if matches!(
            tool,
            super::DrawingToolType::DateRange | super::DrawingToolType::DateAndPriceRange
        ) {
            settings.shadow_color = [0, 0, 0, 51];
        }
        if tool == super::DrawingToolType::DateAndPriceRange {
            settings.border = false;
            settings.border_width = Some(1.0);
        }
        settings
    }

    /// Whether the font size lies in 1 to 256 points and a set border width above 0 and at
    /// most 100 points.
    pub fn is_valid(&self) -> bool {
        crate::styles::typography::FONT_SIZES.contains(&self.font_size)
            && self.border_width.is_none_or(|width| {
                width > 0.0 && width <= crate::drawings::validation::MAX_STROKE_WIDTH
            })
    }

    /// Notes that the drawing being read has no range settings, and gives the [`Default`] ones.
    pub(crate) fn left_out() -> Self {
        RANGE_SETTINGS_LEFT_OUT.set(true);
        Self::default()
    }

    /// Clears the note of [`Self::left_out`], before a drawing is read.
    pub(crate) fn forget_left_out() {
        RANGE_SETTINGS_LEFT_OUT.set(false);
    }

    /// Takes `font_size`, the drawing's own, for the readouts when the drawing just read left
    /// these settings out: its card was drawn at that size.
    pub(crate) fn fill_left_out(&mut self, font_size: f32) {
        if RANGE_SETTINGS_LEFT_OUT.replace(false) {
            self.font_size = font_size;
        }
    }
}

/// How a long or short position drawing sizes its quantity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PositionSize {
    /// A fixed quantity, interpreted as lots by [`Self::quantity_with_sizing`].
    Quantity(f64),
    /// The quantity that loses this amount if the stop is hit.
    RiskAmount(f64),
    /// The quantity that loses `percent` of `account_size` if the stop is hit.
    RiskPercent {
        /// Account size, in the same currency as risk amounts and the configured point value.
        account_size: f64,
        /// Share of the account risked, in percent (above 0, at most 100).
        percent: f64,
    },
}

impl PositionSize {
    /// Size in lots using the instrument's point value and lot size. Risk-based sizes and
    /// fixed lot quantities round down to `quantity_step`, so rounding cannot increase risk.
    /// Account-percent sizing also caps quantity to the account's available leveraged capital.
    /// Returns `None` if no positive permitted quantity fits, or inputs are invalid.
    pub fn quantity_with_sizing(
        &self,
        entry: f64,
        stop: f64,
        sizing: &PositionSizing,
    ) -> Option<f64> {
        if !sizing.is_valid() {
            return None;
        }
        let mut quantity = self.quantity(entry, stop)?;
        if !matches!(self, Self::Quantity(_)) {
            quantity /= sizing.point_value * sizing.lot_size;
        }
        if let Self::RiskPercent { account_size, .. } = *self {
            let capital_per_lot = sizing.required_capital(entry, 1.0)?;
            if capital_per_lot > 0.0 {
                quantity = quantity.min(account_size / capital_per_lot);
            }
        }
        if sizing.quantity_step > 0.0 {
            let steps = quantity / sizing.quantity_step;
            // Decimal steps can put an exact multiple just below its integer quotient.
            let nearest = steps.round();
            let steps = if (steps - nearest).abs() <= 4.0 * f64::EPSILON * steps.abs() {
                nearest
            } else {
                steps.floor()
            };
            quantity = steps * sizing.quantity_step;
        }
        (quantity.is_finite() && quantity > 0.0).then_some(quantity)
    }

    /// Unit quantity at `entry` with its stop at `stop`, without instrument conversion.
    /// Use [`Self::quantity_with_sizing`] for lot sizing and quantity-step rounding.
    ///
    /// `None` when a size from the risk has its stop at the entry, or a
    /// setting is out of bounds (not positive and finite, or a percent above 100).
    pub fn quantity(&self, entry: f64, stop: f64) -> Option<f64> {
        let positive = |value: f64| value.is_finite() && value > 0.0;
        let risk = match *self {
            Self::Quantity(quantity) => return positive(quantity).then_some(quantity),
            Self::RiskAmount(amount) => amount,
            Self::RiskPercent {
                account_size,
                percent,
            } if positive(account_size) && positive(percent) && percent <= 100.0 => {
                account_size * (percent / 100.0)
            }
            Self::RiskPercent { .. } => return None,
        };
        let quantity = risk / (entry - stop).abs();
        positive(quantity).then_some(quantity)
    }
}

impl Default for PositionSize {
    /// One unit, so the profit and loss read per unit.
    fn default() -> Self {
        Self::Quantity(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::VolumeProfileConfig;

    #[test]
    fn new_fibonacci_tools_start_from_their_level_tables() {
        use crate::drawings::DrawingToolType as T;
        // Length, label side and (position, ratio, colour, shown) of sample rows.
        let spares = [
            (0, 0.0, [0x80, 0x80, 0x80], true),
            (10, 4.236, [0xE9, 0x1E, 0x63], true),
            (11, 1.272, [0xFF, 0x98, 0x00], false),
            (15, 2.0, [0x08, 0x99, 0x81], false),
            (23, 4.764, [0x08, 0x99, 0x81], false),
        ];
        let wedge = [
            (0, 0.236, [0xF2, 0x36, 0x45], true),
            (8, 3.618, [0x67, 0x3A, 0xB7], false),
            (10, 4.618, [0xE9, 0x1E, 0x63], false),
        ];
        let zones = [
            (0, 0.0, [0x80, 0x80, 0x80], true),
            (10, 89.0, [0x29, 0x62, 0xFF], true),
        ];
        let time = [
            (2, 0.5, [0x81, 0xC7, 0x84], false),
            (10, 3.0, [0x67, 0x3A, 0xB7], true),
        ];
        for (name, config, len, left, rows) in [
            ("default", FibonacciConfig::default(), 24, true, &spares[..]),
            (
                "extension",
                FibonacciConfig::extension_default(),
                24,
                true,
                &spares,
            ),
            (
                "channel",
                FibonacciConfig::for_tool(T::FibonacciChannel),
                24,
                true,
                &spares,
            ),
            (
                "wedge",
                FibonacciConfig::for_tool(T::FibonacciWedge),
                11,
                true,
                &wedge,
            ),
            (
                "zones",
                FibonacciConfig::for_tool(T::FibonacciTimeZones),
                11,
                false,
                &zones,
            ),
            (
                "time",
                FibonacciConfig::for_tool(T::TrendBasedFibTime),
                11,
                false,
                &time,
            ),
        ] {
            assert_eq!(config.levels.len(), len, "{name}");
            assert_eq!(config.labels_on_left, left, "{name}");
            for &(index, value, [r, g, b], visible) in rows {
                let level = &config.levels[index];
                assert_eq!(
                    (level.value, level.color, level.visible),
                    (value, [r, g, b, 255], visible),
                    "{name}, level {index}"
                );
            }
        }
    }

    /// Reference: quantity = risked amount / |entry - stop| (reference.py);
    /// none when a size from the risk has its stop at the entry, or a setting
    /// is out of bounds.
    #[test]
    fn position_size_quantity() {
        let percent = |account_size, percent| PositionSize::RiskPercent {
            account_size,
            percent,
        };
        for (size, entry, stop, want) in [
            (PositionSize::Quantity(3.0), 50.0, 48.0, Some(3.0)),
            (PositionSize::RiskAmount(100.0), 50.0, 48.0, Some(50.0)),
            (percent(10_000.0, 1.0), 50.0, 52.5, Some(40.0)),
            (percent(2_000.0, 2.5), 1.1, 1.0975, Some(19999.99999999865)),
            (PositionSize::RiskAmount(100.0), 50.0, 50.0, None),
            (PositionSize::RiskAmount(-100.0), 50.0, 48.0, None),
            (PositionSize::Quantity(f64::NAN), 50.0, 48.0, None),
            (percent(10_000.0, 150.0), 50.0, 48.0, None),
            (percent(-10_000.0, -1.0), 50.0, 48.0, None),
            (percent(1e308, 100.0), 1.0, 0.0, Some(1e308)),
        ] {
            let got = size.quantity(entry, stop);
            assert!(
                got.zip(want)
                    .map_or(got == want, |(got, want)| (got - want).abs() < 1e-6),
                "{size:?}: got {got:?}, want {want:?}"
            );
        }

        // Hand calculations in lots, independent of the renderer's label formatting.
        let sizing = PositionSizing {
            point_value: 50.0,
            lot_size: 10.0,
            quantity_step: 0.01,
            leverage: 5.0,
        };
        for (size, stop, expected) in [
            (PositionSize::RiskAmount(230.0), 97.5, Some(0.18)),
            (PositionSize::RiskAmount(230.0), 102.5, Some(0.18)),
            (
                PositionSize::RiskPercent {
                    account_size: 23000.0,
                    percent: 1.0,
                },
                97.5,
                Some(0.18),
            ),
            (PositionSize::Quantity(0.189), 97.5, Some(0.18)),
            (PositionSize::Quantity(0.29), 97.5, Some(0.29)),
            (PositionSize::Quantity(0.009), 97.5, None),
            (PositionSize::RiskAmount(230.0), 100.0, None),
        ] {
            let quantity = size.quantity_with_sizing(100.0, stop, &sizing);
            assert_eq!(quantity, expected, "{size:?}, stop {stop}");
            let unleveraged = PositionSizing {
                leverage: 1.0,
                ..sizing
            };
            assert_eq!(
                quantity,
                size.quantity_with_sizing(100.0, stop, &unleveraged)
            );
        }
        // Ten quarter-point stop ticks at 50 per point, ten units per lot and 0.8
        // account units per quote unit cost 1000 per lot: 230 risk permits 0.23 lots.
        let mut readout = PositionReadout {
            account_currency: Some("EUR".into()),
            conversion_rate: Some(0.8),
            quantity_decimals: Some(2),
            ..Default::default()
        };
        for (places, step, expected) in [
            (Some(2), 0.01, Some(0.23)),
            (Some(1), 0.01, Some(0.2)),
            (Some(0), 0.01, None),
            (None, 0.01, Some(0.23)),
            (Some(1), 0.25, Some(0.5)),
        ] {
            readout.quantity_decimals = places;
            let sizing = PositionSizing {
                quantity_step: step,
                ..sizing
            };
            let size = if step == 0.25 {
                PositionSize::Quantity(0.99)
            } else {
                PositionSize::RiskAmount(230.0)
            };
            let got = readout.quantity(size, 100.0, 97.5, sizing, Some("USD"));
            assert!(
                got.zip(expected)
                    .map_or(got == expected, |(a, b)| (a - b).abs() < 1e-12),
                "{places:?}, step {step}: {got:?}"
            );
        }
        // One lot needs 100 * 50 * 0.8 * 10 / 5 = 8000 account units.
        // An account of 1000 permits 0.125 lots, or 0.12 at the 0.01 increment.
        readout.quantity_decimals = Some(2);
        for (leverage, expected) in [(1.0, 0.02), (5.0, 0.12), (10.0, 0.25), (100.0, 1.0)] {
            assert_eq!(
                readout.quantity(
                    PositionSize::RiskPercent {
                        account_size: 1000.0,
                        percent: 100.0
                    },
                    100.0,
                    97.5,
                    PositionSizing { leverage, ..sizing },
                    Some("USD")
                ),
                Some(expected)
            );
        }
        readout.conversion_rate = None;
        assert!(readout.account_sizing(sizing, Some("USD")).is_none());
        assert_eq!(readout.account_sizing(sizing, Some("EUR")), Some(sizing));
        readout.quantity_decimals = Some(9);
        assert!(!readout.is_valid());
        assert_eq!(sizing.required_capital(100.0, 0.18), Some(1800.0));
        assert_eq!(
            PositionSize::Quantity(2.13).quantity_with_sizing(
                100.0,
                97.5,
                &PositionSizing {
                    quantity_step: 0.25,
                    ..sizing
                }
            ),
            Some(2.0)
        );
        for sizing in [
            PositionSizing {
                point_value: 0.0,
                ..sizing
            },
            PositionSizing {
                lot_size: -1.0,
                ..sizing
            },
            PositionSizing {
                point_value: 1e-200,
                lot_size: 1e-200,
                ..sizing
            },
            PositionSizing {
                quantity_step: -0.01,
                ..sizing
            },
            PositionSizing {
                leverage: f64::NAN,
                ..sizing
            },
        ] {
            assert!(
                PositionSize::default()
                    .quantity_with_sizing(100.0, 97.5, &sizing)
                    .is_none()
            );
            assert!(sizing.required_capital(100.0, 0.18).is_none());
        }
    }

    #[test]
    fn drawing_settings_reject_out_of_bounds_values() {
        assert!(VolumeProfileConfig::new(1, 1.0).is_some());
        assert!(VolumeProfileConfig::new(1000, 0.01).is_some());
        for (rows, value_area) in [(0, 0.7), (1001, 0.7), (24, 0.0), (24, 1.01), (24, f64::NAN)] {
            assert!(VolumeProfileConfig::new(rows, value_area).is_none());
        }
        for height in [0.0, -1.0, f64::INFINITY, f64::NAN] {
            assert!(
                VolumeProfileConfig::default()
                    .with_row_size(Some(height))
                    .is_none()
            );
        }
        assert!(RegressionTrendConfig::new(10.0).is_some());
        let regression = RegressionTrendConfig::default();
        for deviations in [0.0, -1.0, 10.5, f64::INFINITY, f64::NAN] {
            assert!(RegressionTrendConfig::new(deviations).is_none());
            assert!(regression.with_deviations(1.0, deviations).is_none());
            assert!(regression.with_deviations(deviations, 1.0).is_none());
        }
        assert!(regression.with_precision(Some(8)).is_some());
        assert!(regression.with_precision(Some(9)).is_none());
        for saved in [
            r#"{"upper":2.0,"lower":0.0}"#,
            r#"{"precision":9}"#,
            r#"{"upper_line":{"visible":true,"color":[1,2,3,255],"line_style":"Solid","width":0.0}}"#,
        ] {
            assert!(serde_json::from_str::<RegressionTrendConfig>(saved).is_err());
        }
        // Settings saved before the lines were styled draw them as they did.
        for saved in [
            r#"{"deviations":2.0}"#,
            r#"{"upper":2.0,"lower":2.0,"use_upper":true,"use_lower":true,"source":"Close","line_style":"Dashed","precision":null}"#,
        ] {
            let saved = serde_json::from_str::<RegressionTrendConfig>(saved).unwrap();
            let band = RegressionBandLine::new([41, 98, 255, 255]);
            assert!(saved.show_base_line && saved.show_correlation);
            assert_eq!([saved.upper_line, saved.lower_line], [band, band]);
        }
    }

    /// Anchors 100 then 200: level v sits at 200 - 100 * v, or at 100 + 100 * v when reversed.
    #[test]
    fn fibonacci_retracement_position() {
        for (reverse, value, want) in [
            (false, 0.0, 200.0),
            (false, 0.236, 176.4),
            (false, 0.618, 138.2),
            (false, 1.0, 100.0),
            (true, 0.236, 123.6),
            (true, 0.786, 178.6),
        ] {
            let config = FibonacciConfig {
                reverse,
                ..Default::default()
            };
            let got = config.retracement_position(100.0, 200.0, value);
            assert!(
                (got - want).abs() < 1e-9,
                "reverse {reverse}, level {value}: got {got}, want {want}"
            );
        }
        // With 0 at 10 and 1 at 1000, log10(price) = 1 + 2*r.
        // Rounded independently: 10^1.472, 10^2, 10^2.236.
        for (logarithmic, low, ratio, want) in [
            (true, 10.0, 0.236, 29.64831389524342),
            (true, 10.0, 0.5, 100.0),
            (true, 10.0, 0.618, 172.1868574986006),
            (false, 10.0, 0.236, 243.64),
            (false, 10.0, 0.5, 505.0),
            (false, 10.0, 0.618, 621.82),
            (true, 0.0, 0.5, 500.0),
            (true, -10.0, 0.5, 495.0),
        ] {
            for reverse in [false, true] {
                let config = FibonacciConfig {
                    logarithmic,
                    reverse,
                    ..Default::default()
                };
                let (start, end) = if reverse {
                    (low, 1000.0)
                } else {
                    (1000.0, low)
                };
                let got = config.retracement_position(start, end, ratio);
                assert!(
                    (got - want).abs() < 1e-9,
                    "log {logarithmic}, reverse {reverse}, ratio {ratio}: {got} != {want}"
                );
            }
        }
    }

    /// A code that mixes cases names a minor unit (`GBp` pence, `ZAr` South African cents) and
    /// matches only itself; codes in a single case match regardless of case. The instrument's
    /// point is worth one unit of its currency, times the rate when the codes differ.
    #[test]
    fn minor_unit_instrument_converts_into_the_account_currency() {
        let sizing = PositionSizing::default();
        for (instrument, account, rate, point_value) in [
            // One pence is 1 x 0.01 = 0.01 pounds.
            ("GBp", "GBP", Some(0.01), Some(0.01)),
            ("GBp", "gbp", Some(0.01), Some(0.01)),
            ("ZAr", "ZAR", Some(0.01), Some(0.01)),
            // One pound is 1 x 100 = 100 pence.
            ("GBP", "GBp", Some(100.0), Some(100.0)),
            // Pence without a rate have no pound value, rather than a mislabeled one.
            ("GBp", "GBP", None, None),
            // The same currency takes no rate: the point stays 1.
            ("GBp", "GBp", Some(0.01), Some(1.0)),
            ("gbp", "GBP", Some(0.01), Some(1.0)),
            ("GBP", "GBP", None, Some(1.0)),
            // Different letters are different currencies in any case.
            ("GBX", "GBP", Some(0.01), Some(0.01)),
        ] {
            let readout = PositionReadout {
                account_currency: Some(account.into()),
                conversion_rate: rate,
                ..Default::default()
            };
            assert_eq!(
                readout
                    .account_sizing(sizing, Some(instrument))
                    .map(|sizing| sizing.point_value),
                point_value,
                "{instrument} in {account}"
            );
        }
        // Risking 10 pounds with the stop 10 pence away: 10 / (10 x 0.01) = 100 units. Quoted in
        // pounds, the stop is 10 pounds away: 10 / (10 x 1) = 1 unit.
        let readout = PositionReadout {
            account_currency: Some("GBP".into()),
            conversion_rate: Some(0.01),
            ..Default::default()
        };
        for (instrument, expected) in [("GBp", 100.0), ("gbp", 1.0)] {
            let quantity = readout.quantity(
                PositionSize::RiskAmount(10.0),
                250.0,
                240.0,
                sizing,
                Some(instrument),
            );
            assert!(
                quantity.is_some_and(|quantity| (quantity - expected).abs() < 1e-9),
                "{instrument}: {quantity:?}"
            );
        }
    }
}
