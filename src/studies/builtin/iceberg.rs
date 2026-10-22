//! # Iceberg Detector
//!
//! Flags bars that close at a price level the recent bars kept trading at with
//! heavy volume. Bars carry no order book or trade data, so this is a proxy for
//! repeated activity at one price, estimated from bars; it cannot see hidden
//! orders.

use super::{clamp_param, window_volume};
use crate::model::Bar;
use crate::model::bar::{GrowingGrid, bar_level_keys, level_key};
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

/// Iceberg Detector, estimated from bars.
///
/// Each bar's volume is spread evenly over the price levels between its low and
/// high. A bar is flagged when the level of its close was touched by at least
/// `min_touches` of the last `period` bars and holds at least `volume_threshold`
/// times their average bar volume from them. The value is a score from 0 to 1
/// on flagged bars and 0 on other bars, plotted in its own pane.
///
/// Levels lie on the configured tick size (see
/// [`with_tick_size`](Self::with_tick_size)), widened to the tick times a
/// power of two once the bars up to the current one span more than 1,000
/// ticks, so they cover at most about 1,000 levels; detection prices are
/// multiples of that grid. Each value uses only its bar and the bars before
/// it, so appending bars never changes earlier values. Bars with a non-finite
/// price or an inverted range are not tracked.
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
    /// Whether the bar that began the level's current run of touches, or, after
    /// the grid widens, the oldest bar in the window on the level, closed at or
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

    /// Set the instrument's tick size, which levels are keyed on, clamped into
    /// 0 to `f64::MAX`, NaN to 0. With zero (the default), levels divide the
    /// price span of the bars so far into about 1,000 levels instead, rounded
    /// to a power of two.
    pub fn with_tick_size(mut self, tick_size: f64) -> Self {
        self.tick_size = clamp_param(tick_size, 0.0, f64::MAX);
        self
    }

    /// Set the volume threshold, as a multiple of the average bar volume,
    /// clamped into 0 to `f64::MAX`, NaN to 0
    pub fn with_volume_threshold(mut self, threshold: f64) -> Self {
        self.volume_threshold = clamp_param(threshold, 0.0, f64::MAX);
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

        let mut grid = GrowingGrid::new(self.tick_size);
        // Per touched level: touches, volume, and whether the bar that began the
        // current run of touches closed at or above its open
        let mut levels: HashMap<i64, (usize, f64, bool)> = HashMap::default();
        let mut volume = 0.0;
        for (idx, bar) in data.iter().enumerate() {
            volume = window_volume(data, idx, self.period, volume);
            let regrid = grid.push(bar);
            let grid = grid.grid();
            // Quantize a tracked bar (see the type docs) to its low, high and close keys,
            // with the volume it spreads on each level between its low and high.
            let keys = |bar: &Bar| {
                let (low, high) = bar_level_keys(bar, grid)?;
                let share = bar.volume / (high.abs_diff(low) + 1) as f64;
                let close = bar.close.is_finite();
                close.then(|| (low, high, level_key(bar.close, grid), share))
            };
            let add = |levels: &mut HashMap<i64, (usize, f64, bool)>, bar: &Bar| {
                if let Some((low_key, high_key, _, share)) = keys(bar) {
                    for key in low_key..=high_key {
                        let level = levels.entry(key).or_insert((0, 0.0, bar.close >= bar.open));
                        level.0 += 1;
                        level.1 += share;
                    }
                }
            };

            // The levels hold the `period` bars before this one, keyed again when the grid changes
            if regrid {
                levels.clear();
                for bar in &data[idx.saturating_sub(self.period)..idx] {
                    add(&mut levels, bar);
                }
            }
            if let Some(leaving) = idx.checked_sub(self.period)
                && let Some((low_key, high_key, _, share)) = keys(&data[leaving])
            {
                for key in low_key..=high_key {
                    if let Entry::Occupied(mut entry) = levels.entry(key) {
                        let level = entry.get_mut();
                        level.0 = level.0.saturating_sub(1);
                        level.1 -= share;
                        if level.0 == 0 {
                            entry.remove();
                        }
                    }
                }
            }
            add(&mut levels, bar);

            let Some((_, _, close_key, _)) = keys(bar) else {
                self.values.push(IndicatorValue::None);
                continue;
            };
            let avg_volume = volume / (idx + 1).min(self.period) as f64;
            let volume_threshold = avg_volume * self.volume_threshold;

            let mut value = IndicatorValue::Single(0.0);
            if let Some(&(touches, vol, is_buy)) = levels.get(&close_key)
                && touches >= self.min_touches
                && vol >= volume_threshold
            {
                let price = close_key as f64 * grid;
                let confidence = (touches as f64 / (self.min_touches as f64 * 2.0)).min(1.0)
                    * (vol / (volume_threshold * 2.0)).min(1.0);

                self.detections.push(IcebergDetection {
                    bar_idx: idx,
                    price,
                    volume: vol,
                    touches,
                    is_buy_side: is_buy,
                    confidence,
                });
                value = IndicatorValue::Single(confidence);
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
