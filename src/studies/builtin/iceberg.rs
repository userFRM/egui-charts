//! # Iceberg Detector
//!
//! Flags bars that close at a price level the recent bars kept trading at with
//! heavy volume. Bars carry no order book or trade data, so this is a proxy for
//! repeated activity at one price, estimated from bars; it cannot see hidden
//! orders.

use crate::model::Bar;
use crate::model::bar::{bar_level_keys, level_grid, level_key};
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Iceberg Detector, estimated from bars.
///
/// Each bar's volume is spread evenly over the price levels between its low and
/// high. A bar is flagged when the level of its close was touched by at least
/// `min_touches` of the last `period` bars and holds at least `volume_threshold`
/// times the series' average bar volume from them. The value is a score from 0
/// to 1 on flagged bars and 0 on other bars, plotted in its own pane.
///
/// Levels lie on the configured tick size (see
/// [`with_tick_size`](Self::with_tick_size)), widened to a whole number of
/// ticks when the series spans more than 1,000 ticks so it covers at most
/// about 1,000 levels; detection prices are multiples of that grid. Because the
/// grid and the average volume come from the whole series, a bar that extends
/// the series can change earlier values. Bars with a non-finite price or an
/// inverted range are not tracked.
#[derive(Clone)]
pub struct IcebergDetector {
    /// Number of bars a level's touches and volume are counted over
    period: usize,
    /// Instrument tick size; zero when not configured
    tick_size: f64,
    /// Minimum volume threshold (as multiple of average)
    volume_threshold: f64,
    /// Minimum touches at price level to flag
    min_touches: usize,
    /// Calculated values
    values: Vec<IndicatorValue>,
    /// Color for buy iceberg markers
    buy_color: Color32,
    /// Color for sell iceberg markers
    sell_color: Color32,
    /// Visibility
    visible: bool,
    /// Detected icebergs
    detections: Vec<IcebergDetection>,
}

/// A bar flagged by [`IcebergDetector`]
#[derive(Debug, Clone)]
pub struct IcebergDetection {
    /// Bar index where detected
    pub bar_idx: usize,
    /// Price level
    pub price: f64,
    /// Volume at this level over the window
    pub volume: f64,
    /// Number of bars in the window that touched the level
    pub touches: usize,
    /// Whether the bar that began the level's current run of touches closed at or
    /// above its open; that bar may have left the window
    pub is_buy_side: bool,
    /// Score from 0 to 1: touches and volume relative to twice their thresholds
    pub confidence: f64,
}

impl IcebergDetector {
    /// Create a new iceberg detector
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            tick_size: 0.0,
            volume_threshold: 2.0,
            min_touches: 3,
            values: Vec::new(),
            buy_color: DESIGN_TOKENS.semantic.chart.bullish,
            sell_color: DESIGN_TOKENS.semantic.chart.bearish,
            visible: true,
            detections: Vec::new(),
        }
    }

    /// Set the instrument's tick size, which levels are keyed on. With zero
    /// (the default), a negative or a non-finite value, levels are the series'
    /// price span divided into 1,000 levels instead.
    pub fn with_tick_size(mut self, tick_size: f64) -> Self {
        self.tick_size = tick_size;
        self
    }

    /// Set volume threshold
    pub fn with_volume_threshold(mut self, threshold: f64) -> Self {
        self.volume_threshold = threshold;
        self
    }

    /// Set minimum touches
    pub fn with_min_touches(mut self, touches: usize) -> Self {
        self.min_touches = touches;
        self
    }

    /// Set colors
    pub fn with_colors(mut self, buy: Color32, sell: Color32) -> Self {
        self.buy_color = buy;
        self.sell_color = sell;
        self
    }

    /// Get detected icebergs
    pub fn detections(&self) -> &[IcebergDetection] {
        &self.detections
    }
}

/// Construct with the conventional default parameters.
impl Default for IcebergDetector {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for IcebergDetector {
    fn name(&self) -> &str {
        "Iceberg"
    }

    fn desc(&self) -> &str {
        "Iceberg Detector - Repeated heavy price levels, estimated from bars"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();
        self.detections.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate average volume for threshold
        let avg_volume: f64 = data.iter().map(|b| b.volume).sum::<f64>() / data.len() as f64;
        let volume_threshold = avg_volume * self.volume_threshold;

        let tick_size = level_grid(data, self.tick_size);
        // Quantize a tracked bar (see the type docs) to its low, high and close keys,
        // with the volume it spreads on each level between its low and high.
        let keys = |bar: &Bar| {
            let (low, high) = bar_level_keys(bar, tick_size)?;
            let share = bar.volume / (high.abs_diff(low) + 1) as f64;
            let close = bar.close.is_finite();
            close.then(|| (low, high, level_key(bar.close, tick_size), share))
        };

        // Track price levels and their touches. Tracked keys lie on the grid between
        // the series' lowest and highest key, so the levels fit a dense vector; a
        // level with no touches is absent.
        let (first, last) = data
            .iter()
            .filter_map(keys)
            .fold((i64::MAX, i64::MIN), |(lo, hi), (low, high, close, _)| {
                (lo.min(low).min(close), hi.max(high).max(close))
            });
        let slot = |key: i64| (key >= first).then(|| usize::try_from(key.abs_diff(first)).ok())?;
        let mut price_levels: Vec<(usize, f64, usize, bool)> =
            vec![(0, 0.0, 0, false); slot(last).map_or(0, |i| i + 1)];

        for (idx, bar) in data.iter().enumerate() {
            // Drop the bar leaving the window, so levels count the last `period` bars
            if idx >= self.period
                && let Some((low_key, high_key, _, share)) = keys(&data[idx - self.period])
            {
                for key in low_key..=high_key {
                    if let Some(entry) = slot(key).and_then(|i| price_levels.get_mut(i)) {
                        entry.0 = entry.0.saturating_sub(1);
                        entry.1 -= share;
                    }
                }
            }

            let mut value = IndicatorValue::None;
            if let Some((low_key, high_key, close_key, share)) = keys(bar) {
                let is_bullish = bar.close >= bar.open;

                for key in low_key..=high_key {
                    let Some(entry) = slot(key).and_then(|i| price_levels.get_mut(i)) else {
                        continue;
                    };
                    if entry.0 == 0 {
                        *entry = (0, 0.0, idx, is_bullish);
                    }
                    entry.0 += 1; // touches
                    entry.1 += share;
                }

                value = IndicatorValue::Single(0.0);
                if let Some((touches, vol, _, is_buy)) =
                    slot(close_key).and_then(|i| price_levels.get(i))
                    && *touches > 0
                    && *touches >= self.min_touches
                    && *vol >= volume_threshold
                {
                    let price = close_key as f64 * tick_size;
                    let confidence = (*touches as f64 / (self.min_touches as f64 * 2.0)).min(1.0)
                        * (*vol / (volume_threshold * 2.0)).min(1.0);

                    self.detections.push(IcebergDetection {
                        bar_idx: idx,
                        price,
                        volume: *vol,
                        touches: *touches,
                        is_buy_side: *is_buy,
                        confidence,
                    });
                    value = IndicatorValue::Single(confidence);
                }
            }
            self.values.push(value);
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.buy_color, self.sell_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.buy_color = colors[0];
        }
        if colors.len() > 1 {
            self.sell_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        false
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        vec!["Iceberg".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    /// A level's touches and volume both cover the last `period` bars only.
    #[test]
    fn level_volume_leaves_the_window_with_its_touches() {
        let bar = Bar {
            time: Utc::now(),
            open: 100.0,
            high: 100.0,
            low: 100.0,
            close: 100.0,
            volume: 10.0,
        };
        let mut detector = IcebergDetector::new(2)
            .with_min_touches(1)
            .with_volume_threshold(1.0);
        detector.calculate(&vec![bar; 5]);
        let seen: Vec<_> = detector
            .detections()
            .iter()
            .map(|d| (d.bar_idx, d.touches, d.volume))
            .collect();
        // Two bars of 10 at one level once the window is full
        let expected = [
            (0, 1, 10.0),
            (1, 2, 20.0),
            (2, 2, 20.0),
            (3, 2, 20.0),
            (4, 2, 20.0),
        ];
        assert_eq!(seen, expected);
    }
}
