//! Drawing options and configuration types.
//!
//! Contains styling enums ([`LineStyle`](crate::model::LineStyle), [`ArrowStyle`], [`FontWeight`]),
//! interaction configuration ([`DrawingOptions`], [`HandlePos`]),
//! per-timeframe visibility ([`TimeframeVisibility`]), Fibonacci level
//! configuration ([`FibonacciConfig`], [`FibonacciLevel`]), and the settings of
//! the volume profile, regression trend and position drawings
//! ([`VolumeProfileConfig`](crate::data::VolumeProfileConfig), [`RegressionTrendConfig`],
//! [`PositionSize`], [`PositionSizing`], [`PositionReadout`]).

use crate::model::{Timeframe, TimeframeUnit};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::{BTreeMap, HashSet};
use std::ops::RangeInclusive;

/// Identifies a selection handle position on a drawing.
///
/// Different tool types expose different handle layouts. Two-point tools use
/// `Start` and `End`; rectangles use four corner and four edge handles;
/// multi-point tools use indexed `Point(i)` handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Indexed anchor point for multi-point drawings (e.g., `Point(2)` is the
    /// third point in an XABCD pattern).
    Point(usize),
}

/// Arrow/arrowhead style for line drawings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
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

/// The intervals a drawing shows on.
///
/// While [`show_on_all`](Self::show_on_all) is set, a drawing shows on every interval. Otherwise it
/// shows on the intervals named in [`visible_timeframes`](Self::visible_timeframes) and on those
/// whose unit has a range in [`ranges`](Self::ranges) holding their value. An interval's unit and
/// value are those of its label, as [`Timeframe::as_str`](crate::model::Timeframe::as_str) writes
/// it: `90min` is 90 minutes, and a 24 hour interval, labelled `1D`, is 1 day.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TimeframeVisibility {
    /// Show on every interval.
    pub show_on_all: bool,
    /// Interval labels the drawing shows on, matched exactly.
    #[serde(serialize_with = "serialize_sorted")]
    pub visible_timeframes: HashSet<String>,
    /// Values of each unit the drawing shows on, both ends included, such as 1 to 4 hours for
    /// 1h, 2h, 3h and 4h. A range whose start is after its end holds no value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ranges: BTreeMap<TimeframeUnit, RangeInclusive<u64>>,
}

/// Writes the set sorted, so saving the same drawings gives the same file.
fn serialize_sorted<S: serde::Serializer>(
    set: &HashSet<String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(set.iter().collect::<std::collections::BTreeSet<_>>())
}

/// The value and unit of an interval label written by `Timeframe::as_str`, such as `90min`.
fn interval_parts(label: &str) -> Option<(u64, TimeframeUnit)> {
    let split = label.find(|c: char| !c.is_ascii_digit())?;
    let unit = match &label[split..] {
        "t" => TimeframeUnit::Tick,
        "v" => TimeframeUnit::Volume,
        "r" => TimeframeUnit::Range,
        "delta" => TimeframeUnit::Delta,
        "ms" => TimeframeUnit::Millisecond,
        "s" => TimeframeUnit::Second,
        "min" => TimeframeUnit::Minute,
        "h" => TimeframeUnit::Hour,
        "D" => TimeframeUnit::Day,
        "W" => TimeframeUnit::Week,
        "M" => TimeframeUnit::Month,
        _ => return None,
    };
    Some((label[..split].parse().ok()?, unit))
}

impl TimeframeVisibility {
    /// Create visibility for all timeframes (default)
    pub fn all() -> Self {
        Self {
            show_on_all: true,
            visible_timeframes: HashSet::new(),
            ranges: BTreeMap::new(),
        }
    }

    /// Create visibility for specific timeframes only
    pub fn specific(timeframes: &[&str]) -> Self {
        Self {
            show_on_all: false,
            visible_timeframes: timeframes.iter().map(|s| (*s).to_string()).collect(),
            ranges: BTreeMap::new(),
        }
    }

    /// Show only on `current`, matched by its canonical interval label.
    pub fn current_interval(current: Timeframe) -> Self {
        Self::specific(&[&current.as_str()])
    }

    /// Show on `current` and longer time intervals. Calendar months compare as 30 days,
    /// as in [`Timeframe::duration_ms`]. Trade intervals compare only within their own unit.
    pub fn higher_intervals(current: Timeframe) -> Self {
        Self::relative_intervals(current, true)
    }

    /// Show on `current` and shorter time intervals, using the same units and calendar
    /// approximation as [`Self::higher_intervals`].
    pub fn lower_intervals(current: Timeframe) -> Self {
        Self::relative_intervals(current, false)
    }

    fn relative_intervals(current: Timeframe, higher: bool) -> Self {
        use TimeframeUnit as U;
        let (value, unit) = interval_parts(&current.as_str()).expect("canonical interval label");
        let mut visibility = Self::default();
        if current.is_trade_based() {
            visibility.set_range(
                unit,
                Some(if higher { value..=u64::MAX } else { 1..=value }),
            );
            return visibility;
        }
        // Milliseconds in u128 keep the largest custom seconds and converted bounds exact.
        let units = [
            (U::Millisecond, 1_u128),
            (U::Second, 1_000),
            (U::Minute, 60_000),
            (U::Hour, 3_600_000),
            (U::Day, 86_400_000),
            (U::Week, 604_800_000),
            (U::Month, 2_592_000_000),
        ];
        let duration = u128::from(value) * units.iter().find(|(u, _)| *u == unit).unwrap().1;
        for (unit, scale) in units {
            let bound = if higher {
                duration.div_ceil(scale)
            } else {
                duration / scale
            };
            // Subsecond intervals are presets; custom time intervals use whole seconds.
            let max = if unit == U::Millisecond {
                999
            } else {
                u64::MAX
            };
            if higher && bound > u128::from(max) || !higher && bound == 0 {
                continue;
            }
            let bound = bound.min(u128::from(max)) as u64;
            visibility.set_range(unit, Some(if higher { bound..=max } else { 1..=bound }));
        }
        visibility
    }

    /// Check if visible on the interval labelled `timeframe`
    #[inline]
    pub fn is_visible_on(&self, timeframe: &str) -> bool {
        self.show_on_all
            || self.visible_timeframes.contains(timeframe)
            || (!self.ranges.is_empty()
                && interval_parts(timeframe).is_some_and(|(value, unit)| {
                    self.ranges
                        .get(&unit)
                        .is_some_and(|range| range.contains(&value))
                }))
    }

    /// The values of `unit` the drawing shows on: its range, widened to cover the labels of that
    /// unit in [`visible_timeframes`](Self::visible_timeframes). `None` when neither names the unit.
    pub fn range(&self, unit: TimeframeUnit) -> Option<RangeInclusive<u64>> {
        self.visible_timeframes
            .iter()
            .filter_map(|label| interval_parts(label))
            .filter(|&(_, of)| of == unit)
            .map(|(value, _)| value..=value)
            .chain(self.ranges.get(&unit).cloned())
            .reduce(|a, b| *a.start().min(b.start())..=*a.end().max(b.end()))
    }

    /// Show the drawing on `range` of `unit`, or on no interval of that unit for `None`, in place
    /// of that unit's labels in [`visible_timeframes`](Self::visible_timeframes). A range turns
    /// [`show_on_all`](Self::show_on_all) off.
    pub fn set_range(&mut self, unit: TimeframeUnit, range: Option<RangeInclusive<u64>>) {
        self.visible_timeframes
            .retain(|label| interval_parts(label).is_none_or(|(_, of)| of != unit));
        if let Some(range) = range {
            self.show_on_all = false;
            self.ranges.insert(unit, range);
        } else {
            self.ranges.remove(&unit);
        }
    }

    /// Add a timeframe to visibility
    pub fn add_timeframe(&mut self, timeframe: &str) {
        self.show_on_all = false;
        self.visible_timeframes.insert(timeframe.to_string());
    }

    /// Remove a timeframe from visibility
    pub fn remove_timeframe(&mut self, timeframe: &str) {
        self.visible_timeframes.remove(timeframe);
        if self.visible_timeframes.is_empty() && self.ranges.is_empty() {
            self.show_on_all = true;
        }
    }
}

/// Drawing interaction options
#[derive(Debug, Clone)]
pub struct DrawingOptions {
    /// Enable snap to OHLC prices
    pub snap_to_price: bool,
    /// Enable snap to candle open/close times
    pub snap_to_time: bool,
    /// Snap distance in pixels
    pub snap_distance: f32,
    /// Enable magnet mode (snap to nearby points)
    pub magnet_mode: bool,
    /// Magnet distance in pixels
    pub magnet_distance: f32,
    /// Show selection handles
    pub show_handles: bool,
    /// Handle size in pixels
    pub handle_size: f32,
    /// Handle color (RGBA)
    pub handle_color: [u8; 4],
    /// Selected handle color (RGBA)
    pub handle_sel_color: [u8; 4],
}

impl Default for DrawingOptions {
    fn default() -> Self {
        Self {
            snap_to_price: true,
            snap_to_time: true,
            snap_distance: 10.0,
            magnet_mode: false,
            magnet_distance: 15.0,
            show_handles: true,
            handle_size: 6.0,
            handle_color: [255, 255, 255, 200],
            handle_sel_color: [242, 54, 69, 255], // Default red
        }
    }
}

/// Configuration for a single Fibonacci level
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Measure retracement and extension levels in logarithmic price when all anchors are positive.
    #[serde(default)]
    pub logarithmic: bool,
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
        Self {
            levels: levels
                .into_iter()
                .map(|(value, color, visible)| FibonacciLevel {
                    visible,
                    ..FibonacciLevel::new(value, "", color)
                })
                .collect(),
            show_background: tool != T::FibonacciTimeZones,
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
            && (1.0..=256.0).contains(&self.label_size)
            && self
                .levels
                .iter()
                .all(|level| level.value.is_finite() && level.width.is_none_or(width))
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
            trend_visible
        );
        if original.levels.len() == edited.levels.len() {
            for ((target, original), edited) in self
                .levels
                .iter_mut()
                .zip(&original.levels)
                .zip(&edited.levels)
            {
                fields!(
                    target, original, edited, value, label, color, visible, show_price, text,
                    width, line_style
                );
            }
        } else {
            self.levels.clone_from(&edited.levels);
        }
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RegressionTrendFields")]
pub struct RegressionTrendConfig {
    deviations: f64,
}

/// Stored settings of a regression trend, checked by [`RegressionTrendConfig::new`] on load.
#[derive(Deserialize)]
struct RegressionTrendFields {
    deviations: f64,
}

impl TryFrom<RegressionTrendFields> for RegressionTrendConfig {
    type Error = &'static str;

    fn try_from(fields: RegressionTrendFields) -> Result<Self, Self::Error> {
        Self::new(fields.deviations).ok_or("regression trend settings out of bounds")
    }
}

impl RegressionTrendConfig {
    /// Bands `deviations` standard deviations either side of the fitted line
    /// (above 0, at most 10); `None` when out of bounds.
    pub fn new(deviations: f64) -> Option<Self> {
        (deviations > 0.0 && deviations <= 10.0).then_some(Self { deviations })
    }

    /// Distance of the bands from the fitted line, in standard deviations.
    pub fn deviations(&self) -> f64 {
        self.deviations
    }
}

impl Default for RegressionTrendConfig {
    /// Bands two standard deviations away.
    fn default() -> Self {
        Self { deviations: 2.0 }
    }
}

/// Instrument economics used by long and short position drawings.
/// Quantities are lots; one lot contains `lot_size` units, each worth `point_value`
/// per price point in the account currency, or in the instrument currency when
/// [`PositionReadout::account_currency`] requests conversion. Defaults retain unit sizing
/// without rounding.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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
    pub fn account_sizing(
        &self,
        mut sizing: PositionSizing,
        instrument_currency: Option<&str>,
    ) -> Option<PositionSizing> {
        if !self.is_valid() {
            return None;
        }
        if let Some(currency) = &self.account_currency
            && !instrument_currency.is_some_and(|base| base.eq_ignore_ascii_case(currency))
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

/// Placement of a line tool's measurement text relative to its anchors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
pub enum LineTextOrientation {
    /// Along the line, reading from bottom to top.
    #[default]
    Vertical,
    /// Upright, across the line.
    Horizontal,
}

/// Independent readouts of a trend line, ray, extended line, info line or trend angle, and the
/// placement of the text of those tools and of a horizontal line or ray or a vertical line,
/// which have no readouts. Readouts start hidden except on an info line, see [`Self::for_tool`];
/// saves from before these settings read the same way.
/// Price changes follow anchor order; lengths and elapsed time are unsigned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
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
    /// Show the price of each anchor beside it, whether or not the line is selected.
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
    /// vertical line runs through every pane, other tools keep their readouts hidden and their text
    /// above the line.
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

/// How a long or short position drawing sizes its quantity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
    use crate::model::Timeframe;

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
        for deviations in [0.0, -1.0, 10.5, f64::INFINITY, f64::NAN] {
            assert!(RegressionTrendConfig::new(deviations).is_none());
        }
    }

    /// A drawing shows on intervals whose unit and value, read from the label, fall in a range,
    /// and on labels saved before ranges, matched exactly until their unit's range is set.
    #[test]
    fn test_timeframe_visibility() {
        let mut vis = TimeframeVisibility::all();
        assert!(vis.is_visible_on("1h"));
        assert!(vis.is_visible_on("1D"));

        vis.add_timeframe("1h");
        assert!(vis.is_visible_on("1h"));
        assert!(!vis.is_visible_on("1D"));

        let n = |v| std::num::NonZeroU64::new(v).unwrap();
        let mut vis = TimeframeVisibility::all();
        vis.set_range(TimeframeUnit::Hour, Some(1..=4));
        vis.set_range(TimeframeUnit::Minute, Some(1..=59));
        vis.set_range(TimeframeUnit::Second, Some(1..=600));
        vis.set_range(TimeframeUnit::Tick, Some(100..=1000));
        for (timeframe, shown) in [
            (Timeframe::Hour1, true),                // "1h", the start
            (Timeframe::Custom(n(2 * 3600)), true),  // "2h", not one of the nine former names
            (Timeframe::Hour4, true),                // "4h", the end
            (Timeframe::Custom(n(5 * 3600)), false), // "5h"
            (Timeframe::Day1, false),                // "1D"
            (Timeframe::Min30, true),                // "30min"
            (Timeframe::Custom(n(90 * 60)), false),  // "90min", 90 > 59
            (Timeframe::Month1, false),              // "1M" is a month, not a minute
            (Timeframe::Sec30, true),                // "30s"
            (Timeframe::Ms500, false),               // "500ms" is not 500 seconds
            (Timeframe::Ticks(n(99)), false),
            (Timeframe::Ticks(n(100)), true),
            (Timeframe::Ticks(n(1000)), true),
            (Timeframe::Ticks(n(1001)), false),
            (Timeframe::Range(n(100)), false), // "100r", unit not set
            (Timeframe::Delta(n(100)), false), // "100delta"
        ] {
            assert_eq!(vis.is_visible_on(&timeframe.as_str()), shown, "{timeframe}");
        }
        vis.show_on_all = true;
        assert!(vis.is_visible_on("1D"));

        let mut saved = TimeframeVisibility::specific(&["1h", "4h", "1D"]);
        assert_eq!(saved.range(TimeframeUnit::Hour), Some(1..=4));
        assert!(!saved.is_visible_on("2h"));
        saved.set_range(TimeframeUnit::Hour, Some(2..=3));
        assert_eq!(saved.range(TimeframeUnit::Hour), Some(2..=3));
        assert!(
            !saved.is_visible_on("1h") && saved.is_visible_on("2h") && saved.is_visible_on("1D")
        );
        saved.remove_timeframe("1D");
        assert!(!saved.is_visible_on("1D") && saved.is_visible_on("3h"));
        saved.set_range(TimeframeUnit::Hour, None);
        assert_eq!(saved.range(TimeframeUnit::Hour), None);

        // One hour = 3,600 seconds; 90 minutes = 5,400 seconds. Both bounds include equality.
        for (current, probes) in [
            (
                Timeframe::Hour1,
                vec![
                    ("30s", false, true),
                    ("59min", false, true),
                    ("1h", true, true),
                    ("90min", true, false),
                    ("1D", true, false),
                    ("1W", true, false),
                    ("1M", true, false),
                    ("100t", false, false),
                ],
            ),
            (
                Timeframe::Custom(n(5400)),
                vec![
                    ("1h", false, true),
                    ("89min", false, true),
                    ("90min", true, true),
                    ("91min", true, false),
                    ("2h", true, false),
                ],
            ),
            (
                Timeframe::Ms250,
                vec![
                    ("100ms", false, true),
                    ("250ms", true, true),
                    ("500ms", true, false),
                    ("1s", true, false),
                ],
            ),
            (
                Timeframe::Week1,
                vec![
                    ("6D", false, true),
                    ("7D", true, true),
                    ("1W", true, true),
                    ("1M", true, false),
                ],
            ),
            (
                Timeframe::Month1,
                vec![
                    ("4W", false, true),
                    ("30D", true, true),
                    ("1M", true, true),
                    ("5W", true, false),
                ],
            ),
            (
                Timeframe::Ticks(n(100)),
                vec![
                    ("99t", false, true),
                    ("100t", true, true),
                    ("101t", true, false),
                    ("100v", false, false),
                    ("1D", false, false),
                ],
            ),
            (
                Timeframe::Volume(n(100)),
                vec![
                    ("99v", false, true),
                    ("100v", true, true),
                    ("101v", true, false),
                    ("100t", false, false),
                ],
            ),
            (
                Timeframe::Range(n(100)),
                vec![
                    ("99r", false, true),
                    ("100r", true, true),
                    ("101r", true, false),
                    ("100delta", false, false),
                ],
            ),
            (
                Timeframe::Delta(n(100)),
                vec![
                    ("99delta", false, true),
                    ("100delta", true, true),
                    ("101delta", true, false),
                    ("100r", false, false),
                ],
            ),
        ] {
            let above = TimeframeVisibility::higher_intervals(current);
            let below = TimeframeVisibility::lower_intervals(current);
            let only = TimeframeVisibility::current_interval(current);
            for (label, higher, lower) in probes {
                assert_eq!(
                    above.is_visible_on(label),
                    higher,
                    "above {current}: {label}"
                );
                assert_eq!(
                    below.is_visible_on(label),
                    lower,
                    "below {current}: {label}"
                );
                assert_eq!(
                    only.is_visible_on(label),
                    label == current.as_str(),
                    "only {current}: {label}"
                );
            }
        }
        assert_eq!(
            TimeframeVisibility::higher_intervals(Timeframe::Hour1)
                .range(TimeframeUnit::Millisecond),
            None
        );
        // Values above i64::MAX milliseconds must not collapse to equal saturated durations.
        let largest = TimeframeVisibility::higher_intervals(Timeframe::Custom(n(u64::MAX)));
        assert!(largest.is_visible_on(&format!("{}s", u64::MAX)));
        assert!(!largest.is_visible_on(&format!("{}s", u64::MAX - 1)));
        assert!(
            TimeframeVisibility::lower_intervals(Timeframe::Custom(n(u64::MAX)))
                .is_visible_on(&format!("{}s", u64::MAX - 1))
        );

        let legacy = r#"{"show_on_all":false,"visible_timeframes":["4h"]}"#;
        let loaded: TimeframeVisibility = serde_json::from_str(legacy).unwrap();
        assert_eq!(loaded, TimeframeVisibility::specific(&["4h"]));
        assert_eq!(serde_json::to_string(&loaded).unwrap(), legacy);
        let mut hours = TimeframeVisibility::default();
        hours.set_range(TimeframeUnit::Hour, Some(1..=4));
        let json = serde_json::to_string(&hours).unwrap();
        assert_eq!(
            json,
            r#"{"show_on_all":false,"visible_timeframes":[],"ranges":{"Hour":{"start":1,"end":4}}}"#
        );
        assert_eq!(
            serde_json::from_str::<TimeframeVisibility>(&json).unwrap(),
            hours
        );
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
}
