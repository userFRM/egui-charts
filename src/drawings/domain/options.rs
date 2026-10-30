//! Drawing options and configuration types.
//!
//! Contains styling enums ([`LineStyle`](crate::model::LineStyle), [`ArrowStyle`], [`FontWeight`]),
//! interaction configuration ([`DrawingOptions`], [`HandlePos`]),
//! per-timeframe visibility ([`TimeframeVisibility`]), Fibonacci level
//! configuration ([`FibonacciConfig`], [`FibonacciLevel`]), and the settings of
//! the volume profile, regression trend and position drawings
//! ([`VolumeProfileConfig`](crate::data::VolumeProfileConfig), [`RegressionTrendConfig`], [`PositionSize`], [`PositionSizing`]).

use crate::model::TimeframeUnit;
use serde::{Deserialize, Serialize};
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
    /// Default drawing color (RGBA)
    pub default_color: [u8; 4],
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
            default_color: [242, 54, 69, 255],    // Default red
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
    /// on for retracements and extensions, off for channels.
    #[serde(default)]
    pub show_prices: Option<bool>,
    /// Show the level value in labels. `None` preserves legacy labels (off for wedges).
    #[serde(default)]
    pub show_levels: Option<bool>,
    /// Format level values as percentages rather than ratios.
    #[serde(default = "percent_levels")]
    pub levels_as_percent: bool,
    /// Place labels at the left end of level lines instead of the right.
    #[serde(default)]
    pub labels_on_left: bool,
    /// Measure retracement and extension levels in logarithmic price when all anchors are positive.
    #[serde(default)]
    pub logarithmic: bool,
}

fn percent_levels() -> bool {
    true
}

impl Default for FibonacciConfig {
    /// Default Fibonacci retracement levels
    fn default() -> Self {
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
            show_background: false,
            background_opacity: 30,
            extend_left: false,
            extend_right: false,
            reverse: false,
            logarithmic: false,
            show_prices: None,
            show_levels: None,
            levels_as_percent: true,
            labels_on_left: false,
        }
    }
}

impl FibonacciConfig {
    /// Extension levels (beyond 100%)
    pub fn extension_default() -> Self {
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
            show_background: false,
            background_opacity: 30,
            extend_left: false,
            extend_right: true,
            reverse: false,
            logarithmic: false,
            show_prices: None,
            show_levels: None,
            levels_as_percent: true,
            labels_on_left: false,
        }
    }

    /// Initial levels for a tool. Existing drawings without a stored config keep their
    /// original renderer defaults; new tools store this table when constructed.
    pub fn for_tool(tool: super::DrawingToolType) -> Self {
        use super::DrawingToolType as T;
        let grey = [128, 128, 128, 255];
        let blue = [41, 98, 255, 255];
        let red = [242, 54, 69, 255];
        let orange = [255, 152, 0, 255];
        let green = [76, 175, 80, 255];
        let teal = [8, 153, 129, 255];
        let cyan = [0, 188, 212, 255];
        let pink = [233, 30, 99, 255];
        let purple = [156, 39, 176, 255];
        let table = match tool {
            T::FibonacciTimeZones => [0.0, 1.0, 2.0, 3.0, 5.0, 8.0, 13.0, 21.0, 34.0, 55.0, 89.0]
                .map(|value| (value, if value == 0.0 { grey } else { blue }))
                .to_vec(),
            T::TrendBasedFibTime => vec![
                (0.0, grey),
                (0.382, red),
                (0.5, [129, 199, 132, 255]),
                (0.618, green),
                (1.0, teal),
                (1.382, cyan),
                (1.618, grey),
                (2.0, blue),
                (2.382, pink),
                (2.618, purple),
                (3.0, [103, 58, 183, 255]),
            ],
            T::FibonacciSpeedFan => vec![
                (0.0, grey),
                (0.25, orange),
                (0.382, cyan),
                (0.5, green),
                (0.618, teal),
                (0.75, blue),
                (1.0, grey),
            ],
            T::FibonacciCircles | T::FibonacciSpeedResistanceArcs => vec![
                (0.236, red),
                (0.382, orange),
                (0.5, teal),
                (0.618, green),
                (0.786, cyan),
                (1.0, grey),
                (1.618, blue),
                (2.618, pink),
                (3.618, blue),
                (4.236, pink),
                (4.618, red),
            ],
            T::FibonacciExtension => return Self::extension_default(),
            _ => return Self::default(),
        };
        Self {
            levels: table
                .into_iter()
                .map(|(value, color)| {
                    let mut level = FibonacciLevel::new(value, "", color);
                    level.visible = tool != T::TrendBasedFibTime || value != 0.5;
                    level
                })
                .collect(),
            show_background: tool != T::FibonacciTimeZones,
            background_opacity: 51,
            levels_as_percent: tool != T::FibonacciTimeZones,
            ..Self::default()
        }
    }

    /// Label content; a tool without a price passes `None`.
    pub(crate) fn label_text(&self, level: &FibonacciLevel, price: Option<f64>) -> String {
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
            (Some(value), Some(price)) => format!("{value} ({price:.2})"),
            (Some(value), None) => value,
            (None, Some(price)) => format!("{price:.2}"),
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
/// in the account currency per price point. Defaults retain unit sizing without rounding.
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
