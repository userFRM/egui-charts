//! Price scale engine -- maps price values to Y-axis pixel coordinates.
//!
//! Supports four scaling modes:
//!
//! | Mode            | Axis labels             | Use case                         |
//! |-----------------|-------------------------|----------------------------------|
//! | Normal          | Absolute prices         | Default for most instruments     |
//! | Logarithmic     | Log-spaced prices       | Long-term trends, large ranges   |
//! | Percentage      | `+5.2%`, `-3.1%`        | Comparing relative performance   |
//! | IndexedTo100    | Rebased to 100          | Portfolio benchmarking           |

use std::fmt;
use std::str::FromStr;

use super::{PriceMark, PriceMarkGenerator, PriceMarkGeneratorConfig};

/// Scaling mode for the price (Y) axis.
///
/// Determines how raw price values are transformed before being mapped to
/// pixel coordinates.  The default is [`Normal`](PriceScaleMode::Normal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PriceScaleMode {
    /// Prices on a linear axis: equal price differences take equal heights.
    #[default]
    Normal,
    /// Prices on a logarithmic axis: equal price ratios take equal heights.
    Logarithmic,
    /// Change from the first visible value, in percent; that value reads 0%.
    Percentage,
    /// Like [`Percentage`](PriceScaleMode::Percentage), shifted so the first visible value reads 100.
    IndexedTo100,
}

impl fmt::Display for PriceScaleMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PriceScaleMode::Normal => write!(f, "Normal"),
            PriceScaleMode::Logarithmic => write!(f, "Logarithmic"),
            PriceScaleMode::Percentage => write!(f, "Percentage"),
            PriceScaleMode::IndexedTo100 => write!(f, "Indexed to 100"),
        }
    }
}

impl FromStr for PriceScaleMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "normal" => Ok(PriceScaleMode::Normal),
            "logarithmic" | "log" => Ok(PriceScaleMode::Logarithmic),
            "percentage" | "percent" | "%" => Ok(PriceScaleMode::Percentage),
            "indexedto100" | "indexed to 100" | "indexed" => Ok(PriceScaleMode::IndexedTo100),
            _ => Err(format!("Invalid price scale mode: {s}")),
        }
    }
}

/// A min/max price range, used throughout the scale engine.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PriceRange {
    /// Minimum price in the range.
    pub min: f64,
    /// Maximum price in the range.
    pub max: f64,
}

impl PriceRange {
    /// Creates a new price range.
    pub fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }

    /// Returns the size of the range (`max - min`), clamped to a minimum of `1e-12`.
    pub fn length(&self) -> f64 {
        (self.max - self.min).max(1e-12)
    }

    /// Returns `true` if `price` is within `[min, max]` inclusive.
    pub fn contains(&self, price: f64) -> bool {
        price >= self.min && price <= self.max
    }
}

/// Space auto-scaling leaves above and below the data, as fractions of the pane height.
///
/// The defaults keep the data in the middle 80% of the pane. Margins that together
/// leave no room for the data are ignored.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PriceScaleMargins {
    /// Space above the highest value, as a fraction of the pane height (0.0 to 1.0)
    pub top: f64,
    /// Space below the lowest value, as a fraction of the pane height (0.0 to 1.0)
    pub bottom: f64,
}

impl Default for PriceScaleMargins {
    fn default() -> Self {
        Self {
            top: 0.1,
            bottom: 0.1,
        }
    }
}

/// Base span that auto-scale margins are computed from, for data covering `[min, max]`.
///
/// Flat data (no movement beyond float noise) gets 1% of the price level, or 1.0 at
/// zero, so other prices such as drawing anchors stay within a sane pixel distance.
pub(crate) fn auto_scale_span(min: f64, max: f64) -> f64 {
    let range = max - min;
    let level = min.abs().max(max.abs());
    if range > level * 1e-9 {
        range
    } else if level > 0.0 {
        level * 0.01
    } else {
        1.0
    }
}

/// Identifies one of a pane's price axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PriceScaleId {
    /// Left price scale
    Left,
    /// Right price scale (holds the main series)
    #[default]
    Right,
}

impl PriceScaleId {
    /// Returns the opposite scale
    pub fn opposite(&self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

impl fmt::Display for PriceScaleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Left => write!(f, "left"),
            Self::Right => write!(f, "right"),
        }
    }
}

/// Configuration options for a [`PriceScale`].
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PriceScaleOptions {
    /// Price scale mode
    pub mode: PriceScaleMode,

    /// Invert the price scale (uptrends become downtrends)
    pub invert_scale: bool,

    /// Margins (top and bottom)
    pub scale_margins: PriceScaleMargins,

    /// Spacing and number of the tick marks, which place the axis labels and grid lines
    pub tick_marks: PriceMarkGeneratorConfig,
}

/// How one series' prices become scale units.
///
/// Percentage and indexed modes are relative to `base`, the series' first
/// visible close; normal and logarithmic modes ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct PriceTransform {
    /// Scale mode
    pub mode: PriceScaleMode,
    /// Reference price for the percentage and indexed modes
    pub base: f64,
}

impl PriceTransform {
    /// Relative modes without a finite, nonzero reference show ordinary prices instead.
    pub(crate) fn new(mode: PriceScaleMode, base: f64) -> Self {
        match mode {
            PriceScaleMode::Percentage | PriceScaleMode::IndexedTo100 => {
                if base.is_finite() && base != 0.0 {
                    Self { mode, base }
                } else {
                    Self::default()
                }
            }
            _ => Self { mode, base: 0.0 },
        }
    }

    /// Converts a price to scale units.
    ///
    /// Logarithmic maps non-positive prices to negative infinity, which
    /// [`PriceMap`] places far below the pane.
    pub fn to_scale(&self, price: f64) -> f64 {
        match self.mode {
            PriceScaleMode::Normal => price,
            PriceScaleMode::Logarithmic if price > 0.0 => price.ln(),
            PriceScaleMode::Logarithmic => f64::NEG_INFINITY,
            PriceScaleMode::Percentage => (price - self.base) / self.base.abs() * 100.0,
            PriceScaleMode::IndexedTo100 => 100.0 + (price - self.base) / self.base.abs() * 100.0,
        }
    }

    /// Converts scale units back to a price.
    pub fn to_price(&self, value: f64) -> f64 {
        match self.mode {
            PriceScaleMode::Normal => value,
            PriceScaleMode::Logarithmic => value.exp(),
            PriceScaleMode::Percentage => self.base + value / 100.0 * self.base.abs(),
            PriceScaleMode::IndexedTo100 => self.base + (value - 100.0) / 100.0 * self.base.abs(),
        }
    }

    /// The value an axis label shows for a scale value: the price in the normal and logarithmic
    /// modes, the percentage or index value otherwise.
    pub(crate) fn axis_value(&self, value: f64) -> f64 {
        if self.mode == PriceScaleMode::Logarithmic {
            self.to_price(value)
        } else {
            value
        }
    }

    /// Formats a scale value for the axis: prices, signed percentages or index values.
    ///
    /// A host `formatter` receives the price in the normal and logarithmic modes and
    /// the percentage or index value otherwise, and replaces `decimals`.
    pub(crate) fn label(
        &self,
        value: f64,
        decimals: usize,
        formatter: Option<&dyn super::PriceFormatter>,
    ) -> String {
        use super::{DefaultPriceFormatter, PriceFormatter};
        let value = self.axis_value(value);
        match (formatter, self.mode) {
            (Some(formatter), _) => formatter.format(value),
            (None, PriceScaleMode::Percentage) => format!("{value:+.decimals$}%"),
            (None, _) => DefaultPriceFormatter::with_precision(decimals).format(value),
        }
    }
}

/// One price axis: options and the visible range in scale units, fitted to the
/// data until the user scales it by hand.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PriceScale {
    options: PriceScaleOptions,
    /// Visible range in scale units
    range: PriceRange,
    /// The transform `range` is in, from the last `rebase`
    transform: PriceTransform,
    /// The user zoomed or set the range, so autoscale leaves it alone until reset.
    manual: bool,
    /// Larger magnitude of the data at the last fit, in scale units
    level: f64,
    price_precision: Option<u8>,
    bar_ratio: Option<f64>,
}

impl PriceScale {
    /// Creates an auto-scaled price scale.
    pub fn new(options: PriceScaleOptions) -> Self {
        Self {
            options,
            range: PriceRange::new(0.0, 100.0),
            transform: PriceTransform::default(),
            manual: false,
            level: 0.0,
            price_precision: None,
            bar_ratio: None,
        }
    }

    /// Sets the decimals used by price ticks and tags. `None` derives them from the tick step.
    pub fn set_price_precision(&mut self, decimals: Option<u8>) {
        self.price_precision = decimals;
    }

    /// Locks scale units per bar at the current bar spacing and pane height.
    /// Invalid dimensions leave the scale unlocked. Unlocking keeps the current manual range.
    pub fn set_bar_ratio_locked(&mut self, locked: bool, bar_spacing: f32, height: f32) {
        self.bar_ratio = (locked
            && self.range.max > self.range.min
            && (self.range.max - self.range.min).is_finite()
            && bar_spacing.is_finite()
            && bar_spacing > 0.0
            && height.is_finite()
            && height > 0.0)
            .then(|| (self.range.max - self.range.min) * f64::from(bar_spacing / height));
        if self.bar_ratio.is_some() {
            self.manual = true;
        }
    }

    /// Locked scale units per bar, or `None` when unlocked.
    pub fn bar_ratio(&self) -> Option<f64> {
        self.bar_ratio
    }

    /// Set scale units per bar; `None` unlocks. Invalid ratios leave the scale unchanged.
    /// The next `update_bar_ratio` applies it about the range centre.
    pub fn set_bar_ratio(&mut self, ratio: Option<f64>) {
        if ratio.is_some_and(|r| !r.is_finite() || r <= 0.0) {
            return;
        }
        self.bar_ratio = ratio;
        if ratio.is_some() {
            self.manual = true;
        }
    }

    /// Whether horizontal zoom and pane resizing preserve scale units per bar.
    pub fn bar_ratio_locked(&self) -> bool {
        self.bar_ratio.is_some()
    }

    /// Preserves the locked ratio about the range centre after horizontal zoom or pane resize.
    pub fn update_bar_ratio(&mut self, bar_spacing: f32, height: f32) {
        if let Some(ratio) = self.bar_ratio
            && bar_spacing.is_finite()
            && bar_spacing > 0.0
            && height.is_finite()
            && height > 0.0
        {
            let span = ratio * f64::from(height / bar_spacing);
            let centre = self.range.min / 2.0 + self.range.max / 2.0;
            if span.is_finite() && span > 0.0 {
                self.range = PriceRange::new(centre - span / 2.0, centre + span / 2.0);
            }
        }
    }

    /// Returns the options.
    pub fn options(&self) -> &PriceScaleOptions {
        &self.options
    }

    /// Returns the options for changing the mode, inversion or margins.
    ///
    /// The range is in scale units, so after changing the mode call
    /// [`reset`](Self::reset) to fit the data in the new units.
    pub fn options_mut(&mut self) -> &mut PriceScaleOptions {
        &mut self.options
    }

    /// Returns the visible range in scale units.
    pub fn range(&self) -> PriceRange {
        self.range
    }

    /// Returns the transform the range is in.
    pub(crate) fn transform(&self) -> PriceTransform {
        self.transform
    }

    /// Puts the range in the units of `transform`, this frame's transform.
    ///
    /// A range scaled by hand keeps showing the same prices: the relative modes
    /// measure from a base that moves as the chart scrolls.
    pub(crate) fn rebase(&mut self, transform: PriceTransform) {
        let old = std::mem::replace(&mut self.transform, transform);
        if self.manual && old != transform {
            let rebased = |value| transform.to_scale(old.to_price(value));
            self.range = PriceRange::new(rebased(self.range.min), rebased(self.range.max));
        }
    }

    /// Returns `true` when the user has scaled by hand.
    pub fn is_manual(&self) -> bool {
        self.manual
    }

    /// Fits `min..max` (scale units) plus margins unless the user scaled by hand.
    ///
    /// Non-finite bounds (no drawable data) keep the current range.
    pub fn autoscale(&mut self, min: f64, max: f64) {
        if !min.is_finite() || !max.is_finite() {
            return;
        }
        self.level = min.abs().max(max.abs());
        if self.manual {
            return;
        }
        let span = match self.transform.mode {
            // Log units measure ratios: flat data gets 0.01, which is 1% of the price.
            PriceScaleMode::Logarithmic if max - min <= 1e-9 => 0.01,
            _ => auto_scale_span(min, max),
        };
        let PriceScaleMargins { top, bottom } = self.options.scale_margins;
        let (top, bottom) = (top.clamp(0.0, 1.0), bottom.clamp(0.0, 1.0));
        let (top, bottom) = if top + bottom < 1.0 {
            (top, bottom)
        } else {
            (0.0, 0.0)
        };
        // The data fills the rest of the pane, so the whole range is that share's inverse.
        let total = span / (1.0 - top - bottom);
        self.range = PriceRange::new(min - total * bottom, max + total * top);
    }

    /// Shows `min..max` and switches to manual.
    ///
    /// The bounds are in the units of the transform the last fit returned
    /// ([`ChartState::fit_price_scale`](crate::studies::ChartState::fit_price_scale)).
    pub fn set_range(&mut self, min: f64, max: f64) {
        if let Some(ratio) = &mut self.bar_ratio {
            let span = self.range.max - self.range.min;
            if span > 0.0 && span.is_finite() && (max - min).is_finite() && max > min {
                *ratio *= (max - min) / span;
            } else {
                self.bar_ratio = None;
            }
        }
        self.range = PriceRange::new(min, max);
        self.manual = true;
    }

    /// Scales the range by `factor` (above 1 zooms out) around the scale value
    /// `anchor`, which keeps its position on screen, and switches to manual.
    /// `anchor` is in the units of the transform the last fit returned.
    ///
    /// One call changes the span by at most 20 times either way. Zooming in stops at a
    /// millionth of the price level: 1e-6 log units on a logarithmic scale, 1e-4 percentage
    /// or index points in the relative modes, and for prices 1e-6 of the larger magnitude of
    /// the data at the last fit or of the range. A result that is not finite leaves the range
    /// unchanged.
    pub fn zoom(&mut self, factor: f64, anchor: f64) {
        let PriceRange { min, max } = self.range;
        let span = auto_scale_span(min, max);
        let floor = self.min_span();
        // Never widen a range that is already narrower than the floor.
        let new_span = (span * factor)
            .clamp(span * 0.05, span * 20.0)
            .max(floor.min(span));
        let t = ((anchor - min) / span).clamp(0.0, 1.0);
        let new_min = anchor - t * new_span;
        let new_max = new_min + new_span;
        if (new_max - new_min).is_finite() {
            self.set_range(new_min, new_max);
        }
    }

    /// The narrowest span zooming reaches, in scale units: a millionth of the price level,
    /// far above f64 rounding (~1e-16) and at or below common tick sizes. The level is the
    /// price on a logarithmic scale and the base in the relative modes, which do not shrink
    /// with a view that crosses 0; for prices it is the larger magnitude of the data at the last
    /// fit and of the range (1e-6 when both are 0).
    pub(crate) fn min_span(&self) -> f64 {
        match self.transform.mode {
            PriceScaleMode::Logarithmic => 1e-6,
            PriceScaleMode::Percentage | PriceScaleMode::IndexedTo100 => 1e-4,
            PriceScaleMode::Normal => {
                let level = self
                    .level
                    .max(self.range.min.abs())
                    .max(self.range.max.abs());
                if level > 0.0 { level * 1e-6 } else { 1e-6 }
            }
        }
    }

    /// Returns to auto-scaling (double click on the axis).
    pub fn reset(&mut self) {
        self.manual = false;
        self.bar_ratio = None;
    }

    /// Mapping for one series for one frame, given the pane's vertical extent in points.
    pub fn map(&self, transform: PriceTransform, top: f32, bottom: f32) -> PriceMap {
        PriceMap::new(
            transform,
            self.range,
            top,
            bottom,
            self.options.invert_scale,
        )
        .with_price_precision(self.price_precision)
        .with_tick_marks(self.options.tick_marks)
    }
}

/// Per-frame price-to-pixel mapping.
///
/// Unclamped: a price outside the visible range maps outside the pane, and the
/// painter's clip rect clips it.
#[derive(Debug, Clone, Copy)]
pub struct PriceMap {
    transform: PriceTransform,
    range: PriceRange,
    top: f32,
    bottom: f32,
    inverted: bool,
    price_precision: Option<u8>,
    tick_marks: PriceMarkGeneratorConfig,
}

impl PriceMap {
    /// Maps `range` (scale units) onto `top..bottom` (points), the low end at the
    /// bottom unless `inverted`.
    pub fn new(
        transform: PriceTransform,
        range: PriceRange,
        top: f32,
        bottom: f32,
        inverted: bool,
    ) -> Self {
        Self {
            transform,
            range,
            top,
            bottom,
            inverted,
            price_precision: None,
            tick_marks: PriceMarkGeneratorConfig::default(),
        }
    }

    /// Uses the same scale, orientation and formatting in a different vertical plot area.
    pub fn with_bounds(mut self, top: f32, bottom: f32) -> Self {
        self.top = top;
        self.bottom = bottom;
        self
    }

    /// Sets the price precision for this frame.
    pub fn with_price_precision(mut self, decimals: Option<u8>) -> Self {
        self.price_precision = decimals;
        self
    }

    /// Sets how [`marks`](Self::marks) spaces the tick marks.
    pub fn with_tick_marks(mut self, config: PriceMarkGeneratorConfig) -> Self {
        self.tick_marks = config;
        self
    }

    /// Tick marks for the visible range, spaced by the scale's
    /// [`tick_marks`](PriceScaleOptions::tick_marks) settings.
    pub fn marks(&self) -> Vec<PriceMark> {
        PriceMarkGenerator::with_config(self.tick_marks).generate_marks(self)
    }

    /// Supplied price decimals, or `None` to derive them from the tick step.
    /// Percentage and indexed axes keep their own precision.
    pub fn price_precision(&self) -> Option<usize> {
        match self.transform.mode {
            PriceScaleMode::Normal | PriceScaleMode::Logarithmic => {
                self.price_precision.map(usize::from)
            }
            _ => None,
        }
    }

    /// Returns the price transform.
    pub fn transform(&self) -> PriceTransform {
        self.transform
    }

    /// Returns the visible range in scale units.
    pub fn range(&self) -> PriceRange {
        self.range
    }

    /// Returns the pane's vertical extent `(top, bottom)` in points.
    pub(crate) fn extent(&self) -> (f32, f32) {
        (self.top, self.bottom)
    }

    /// Converts a price to a y coordinate.
    #[inline]
    pub fn price_to_y(&self, price: f64) -> f32 {
        self.scale_to_y(self.transform.to_scale(price))
    }

    /// Converts a y coordinate to a price.
    #[inline]
    pub fn y_to_price(&self, y: f32) -> f64 {
        self.transform.to_price(self.y_to_scale(y))
    }

    /// Converts scale units to a y coordinate.
    ///
    /// A flat range has no vertical extent to map onto, so every value lands on
    /// the middle of the pane.
    #[inline]
    pub(crate) fn scale_to_y(&self, value: f64) -> f32 {
        let span = self.range.max - self.range.min;
        if span == 0.0 {
            return (self.top + self.bottom) / 2.0;
        }
        // Bounded so that a non-positive price on a log scale stays a finite coordinate off the pane.
        let ratio = ((value - self.range.min) / span).clamp(-1e4, 1e4) as f32;
        let height = self.bottom - self.top;
        if self.inverted {
            self.top + ratio * height
        } else {
            self.bottom - ratio * height
        }
    }

    /// Converts a y coordinate to scale units.
    ///
    /// A zero-height pane has no inverse, so the low end of the range is returned.
    #[inline]
    pub(crate) fn y_to_scale(&self, y: f32) -> f64 {
        let height = self.bottom - self.top;
        if height.abs() < f32::EPSILON {
            return self.range.min;
        }
        let ratio = if self.inverted {
            (y - self.top) / height
        } else {
            (self.bottom - y) / height
        };
        self.range.min + ratio as f64 * (self.range.max - self.range.min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-computed positions on a pane from y = 0 (top) to y = 100 (bottom).
    #[test]
    fn price_map_places_prices_by_mode() {
        use PriceScaleMode::*;
        let ln = f64::ln;
        // (mode, base, scale range, inverted, price, expected y)
        let cases = [
            (Normal, 0.0, (100.0, 200.0), false, 150.0, 50.0),
            (Normal, 0.0, (100.0, 200.0), false, 120.0, 80.0),
            (Normal, 0.0, (1e-18, 6e-18), false, 2e-18, 80.0),
            // ln 100 is the mean of ln 10 and ln 1,000.
            (Logarithmic, 0.0, (ln(10.0), ln(1000.0)), false, 100.0, 50.0),
            // 55 is +10% of 50, the top of -10%..+10%.
            (Percentage, 50.0, (-10.0, 10.0), false, 55.0, 0.0),
            (Percentage, 50.0, (-10.0, 10.0), false, 50.0, 50.0),
            // 45 is 90 when 50 is 100.
            (IndexedTo100, 50.0, (90.0, 110.0), false, 45.0, 100.0),
            // A negative base keeps the price order: -40 is 20% above -50 and reads 120.
            (IndexedTo100, -50.0, (80.0, 120.0), false, -40.0, 0.0),
            (IndexedTo100, -50.0, (80.0, 120.0), false, -60.0, 100.0),
            (Normal, 0.0, (100.0, 200.0), true, 120.0, 20.0),
            // Outside the range: above the pane, not on its edge.
            (Normal, 0.0, (100.0, 200.0), false, 250.0, -50.0),
            (Normal, 0.0, (100.0, 200.0), true, 50.0, -50.0),
        ];
        for (mode, base, (min, max), inverted, price, y) in cases {
            let transform = PriceTransform { mode, base };
            let map = PriceMap::new(transform, PriceRange::new(min, max), 0.0, 100.0, inverted);
            let got = map.price_to_y(price);
            assert!(
                (got - y).abs() < 1e-3,
                "{mode:?} {price}: y {got}, expected {y}"
            );
            let back = map.y_to_price(got);
            assert!(
                (back - price).abs() < 1e-3 * price.abs(),
                "{mode:?} {price}: back {back}"
            );
        }
    }

    /// Margins are shares of the pane height: data from 100 to 170 with 20% above and 10% below
    /// fills 70% of the pane, so the range is 100 wide and runs from 90 to 190. Margins that
    /// leave no room for the data are ignored.
    #[test]
    fn autoscale_margins_are_shares_of_the_pane() {
        for (top, bottom, expected) in [
            (0.2, 0.1, (90.0, 190.0)),
            (0.0, 0.0, (100.0, 170.0)),
            (0.6, 0.4, (100.0, 170.0)),
            (f64::NAN, 0.1, (100.0, 170.0)),
        ] {
            let mut scale = PriceScale::new(PriceScaleOptions {
                scale_margins: PriceScaleMargins { top, bottom },
                ..Default::default()
            });
            scale.autoscale(100.0, 170.0);
            let PriceRange { min, max } = scale.range();
            assert!(
                (min - expected.0).abs() < 1e-9 && (max - expected.1).abs() < 1e-9,
                "{top} {bottom}: {min}..{max}"
            );
        }
    }

    /// Manual zoom must narrow ordinary intraday ranges, stop at a span tied to the
    /// price level, and never overflow to infinite or NaN bounds when zooming out.
    #[test]
    fn zoom_stays_within_sane_span() {
        let scale_with = |lo, hi| {
            let mut s = PriceScale::new(PriceScaleOptions::default());
            s.set_range(lo, hi);
            s
        };
        // EURUSD, BTC and a series crossing zero, each zoomed in at its anchor.
        for (lo, hi, anchor) in [
            (1.0845, 1.0855, 1.085),
            (99_500.0, 100_000.0, 99_750.0),
            (-0.5, 0.5, 0.0),
        ] {
            let mut s = scale_with(lo, hi);
            s.zoom((-0.1f64).exp(), anchor);
            assert!(
                s.range().max - s.range().min < hi - lo,
                "({lo}, {hi}) -> {:?}",
                s.range()
            );
        }

        // Zoomed all the way in, the span is a millionth of the price level: of the visible data
        // (at most 110, or 0.5 across zero), of the base (100) in percentage and index points,
        // and of the price (1e-6 log units) on a logarithmic scale, also around ln 1 = 0.
        use PriceScaleMode::*;
        let ln = f64::ln;
        // (mode, data in scale units, anchor, expected span)
        for (mode, (lo, hi), anchor, floor) in [
            (Normal, (90.0, 110.0), 100.0, 1.1e-4),
            (Normal, (-0.5, 0.5), 0.0, 5e-7),
            (Percentage, (-5.0, 5.0), 0.0, 1e-4),
            (IndexedTo100, (95.0, 105.0), 100.0, 1e-4),
            (Logarithmic, (ln(0.95), ln(1.05)), 0.0, 1e-6),
        ] {
            let mut s = PriceScale::new(PriceScaleOptions::default());
            s.rebase(PriceTransform { mode, base: 50.0 });
            s.autoscale(lo, hi);
            for _ in 0..200 {
                s.zoom((-1.0f64).exp(), anchor);
            }
            let span = s.range().max - s.range().min;
            assert!(
                (span - floor).abs() < floor * 1e-6,
                "{mode:?} ({lo}, {hi}): {:?}",
                s.range()
            );

            for _ in 0..1000 {
                s.zoom(1.0f64.exp(), anchor);
            }
            let r = s.range();
            assert!(
                r.min.is_finite() && r.max.is_finite() && r.min < r.max,
                "{mode:?}: {r:?}"
            );
        }
    }
}
