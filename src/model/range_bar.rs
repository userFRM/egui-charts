//! Range-bar chart transformation.
//!
//! Range bars have a fixed price range (high minus low) and variable time
//! duration. A new bar starts only once the previous bar's range is filled.
//! This normalises volatility, giving each bar equal "importance" in price
//! terms.
//!
//! - [`to_range_bars_from_ticks`] -- build from tick data (exact).
//! - [`to_range_bars_from_ohlc`] -- approximate from OHLCV bars.

use super::bar::Bar;
use chrono::{DateTime, Utc};

/// Configuration for range-bar construction.
///
/// The `range_size` sets the fixed price range per bar. It can also be
/// derived from ATR via [`with_atr`](Self::with_atr).
#[derive(Debug, Clone)]
pub struct RangeBarConfig {
    /// Fixed range size in price units
    pub range_size: f64,
    /// Whether to use ATR for range size
    pub use_atr: bool,
    /// ATR period if using ATR-based ranges
    pub atr_period: usize,
    /// ATR multiplier
    pub atr_multiplier: f64,
}

impl Default for RangeBarConfig {
    fn default() -> Self {
        Self {
            range_size: 10.0,
            use_atr: false,
            atr_period: 14,
            atr_multiplier: 1.0,
        }
    }
}

impl RangeBarConfig {
    /// Create a range-bar config with an explicit range size in price units.
    pub fn new(range_size: f64) -> Self {
        Self {
            range_size,
            ..Default::default()
        }
    }

    /// Derive the range size from ATR instead of a fixed value.
    pub fn with_atr(mut self, period: usize, multiplier: f64) -> Self {
        self.use_atr = true;
        self.atr_period = period;
        self.atr_multiplier = multiplier;
        self
    }
}

/// A single range bar with fixed price range and variable time span.
#[derive(Debug, Clone)]
pub struct RangeBar {
    /// Open price
    pub open: f64,
    /// High price
    pub high: f64,
    /// Low price
    pub low: f64,
    /// Close price
    pub close: f64,
    /// Total volume
    pub volume: f64,
    /// Ts when bar started
    pub start_time: DateTime<Utc>,
    /// Ts when bar completed
    pub end_time: DateTime<Utc>,
    /// Range size used for this bar
    pub range_size: f64,
    /// Number of ticks in this bar
    pub tick_cnt: usize,
}

impl RangeBar {
    /// Check if this is a bullish (up) bar
    pub fn is_bullish(&self) -> bool {
        self.close >= self.open
    }

    /// Check if this is a bearish (down) bar
    pub fn is_bearish(&self) -> bool {
        self.close < self.open
    }

    /// Get the range of this bar
    pub fn range(&self) -> f64 {
        self.high - self.low
    }

    /// Get the body size (absolute)
    pub fn body(&self) -> f64 {
        (self.close - self.open).abs()
    }

    /// Convert to standard Bar format
    pub fn to_bar(&self) -> Bar {
        Bar {
            time: self.end_time,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
        }
    }
}

/// Minimal tick record for building range bars from raw trade data.
#[derive(Debug, Clone)]
pub struct TickData {
    /// Trade price.
    pub price: f64,
    /// Trade size/volume.
    pub volume: f64,
    /// Trade timestamp.
    pub ts: DateTime<Utc>,
}

/// Build range bars from raw tick data (exact construction).
pub fn to_range_bars_from_ticks(ticks: &[TickData], config: &RangeBarConfig) -> Vec<RangeBar> {
    if ticks.is_empty() {
        return Vec::new();
    }

    let range_size = config.range_size;
    let mut bars: Vec<RangeBar> = Vec::new();

    let first_tick = &ticks[0];
    let mut curr_bar = RangeBar {
        open: first_tick.price,
        high: first_tick.price,
        low: first_tick.price,
        close: first_tick.price,
        volume: first_tick.volume,
        start_time: first_tick.ts,
        end_time: first_tick.ts,
        range_size,
        tick_cnt: 1,
    };

    for tick in ticks.iter().skip(1) {
        // Update current bar
        curr_bar.high = curr_bar.high.max(tick.price);
        curr_bar.low = curr_bar.low.min(tick.price);
        curr_bar.close = tick.price;
        curr_bar.volume += tick.volume;
        curr_bar.end_time = tick.ts;
        curr_bar.tick_cnt += 1;

        // Check if range is exceeded
        if curr_bar.range() >= range_size {
            // Determine close price based on direction
            if tick.price >= curr_bar.open {
                // Bar is moving up
                curr_bar.close = curr_bar.low + range_size;
                curr_bar.high = curr_bar.close;
            } else {
                // Bar is moving down
                curr_bar.close = curr_bar.high - range_size;
                curr_bar.low = curr_bar.close;
            }

            bars.push(curr_bar.clone());

            // Start new bar from where the last one ended
            curr_bar = RangeBar {
                open: curr_bar.close,
                high: tick.price.max(curr_bar.close),
                low: tick.price.min(curr_bar.close),
                close: tick.price,
                volume: 0.0,
                start_time: tick.ts,
                end_time: tick.ts,
                range_size,
                tick_cnt: 0,
            };
        }
    }

    // Add the incomplete bar if it has any ticks
    if curr_bar.tick_cnt > 0 {
        bars.push(curr_bar);
    }

    bars
}

/// Convert OHLC bars to range bars (approximation)
///
/// # Safety Limits
/// - Maximum 5,000 range bars to prevent memory issues
/// - Auto-adjusts range size if too small relative to price range
pub fn to_range_bars_from_ohlc(data: &[Bar], config: &RangeBarConfig) -> Vec<RangeBar> {
    if data.is_empty() {
        return Vec::new();
    }

    // Calculate price range to validate range size
    let (min_price, max_price) = data.iter().fold((f64::MAX, f64::MIN), |(min, max), bar| {
        (min.min(bar.low), max.max(bar.high))
    });
    let price_range = max_price - min_price;

    // Calculate range size (potentially using ATR)
    let base_range_size = if config.use_atr && data.len() >= config.atr_period {
        calculate_atr(data, config.atr_period) * config.atr_multiplier
    } else {
        config.range_size
    };

    // Auto-adjust range size if it would create too many bars
    let min_range_size = price_range / 200.0;
    let range_size = base_range_size.max(min_range_size).max(0.0001);

    let mut bars: Vec<RangeBar> = Vec::new();
    const MAX_BARS: usize = 5_000;

    let first = &data[0];
    let mut curr_bar = RangeBar {
        open: first.open,
        high: first.high,
        low: first.low,
        close: first.close,
        volume: first.volume,
        start_time: first.time,
        end_time: first.time,
        range_size,
        tick_cnt: 1,
    };

    for bar in data.iter().skip(1) {
        // Safety check
        if bars.len() >= MAX_BARS {
            break;
        }

        // Simulate processing this bar's range
        let prices = [bar.open, bar.high, bar.low, bar.close];

        for &price in &prices {
            // Safety check inside inner loop
            if bars.len() >= MAX_BARS {
                break;
            }

            curr_bar.high = curr_bar.high.max(price);
            curr_bar.low = curr_bar.low.min(price);

            // Check if range is exceeded (with iteration limit)
            let mut iterations = 0;
            const MAX_ITERATIONS: usize = 100;

            while curr_bar.range() >= range_size && iterations < MAX_ITERATIONS {
                iterations += 1;

                // Complete the bar
                if curr_bar.high - curr_bar.open >= range_size {
                    // Closed up
                    curr_bar.close = curr_bar.low + range_size;
                    curr_bar.high = curr_bar.close;
                } else {
                    // Closed down
                    curr_bar.close = curr_bar.high - range_size;
                    curr_bar.low = curr_bar.close;
                }

                curr_bar.end_time = bar.time;
                bars.push(curr_bar.clone());

                // Safety check
                if bars.len() >= MAX_BARS {
                    return bars;
                }

                // Start new bar
                curr_bar = RangeBar {
                    open: curr_bar.close,
                    high: price.max(curr_bar.close),
                    low: price.min(curr_bar.close),
                    close: price,
                    volume: 0.0,
                    start_time: bar.time,
                    end_time: bar.time,
                    range_size,
                    tick_cnt: 0,
                };
            }
        }

        curr_bar.close = bar.close;
        curr_bar.volume += bar.volume;
        curr_bar.end_time = bar.time;
        curr_bar.tick_cnt += 1;
    }

    // Add incomplete bar
    if curr_bar.tick_cnt > 0 && bars.len() < MAX_BARS {
        bars.push(curr_bar);
    }

    bars
}

/// Calculate ATR for range size
fn calculate_atr(data: &[Bar], period: usize) -> f64 {
    if data.len() < 2 {
        return 1.0;
    }

    let mut tr_values = Vec::new();

    for i in 1..data.len() {
        let high = data[i].high;
        let low = data[i].low;
        let prev_close = data[i - 1].close;

        let tr = (high - low)
            .max((high - prev_close).abs())
            .max((low - prev_close).abs());
        tr_values.push(tr);
    }

    if tr_values.len() < period {
        return tr_values.iter().sum::<f64>() / tr_values.len() as f64;
    }

    let sum: f64 = tr_values.iter().rev().take(period).sum();
    sum / period as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_ticks() -> Vec<TickData> {
        let now = Utc::now();
        let prices = [
            100.0, 101.0, 102.0, 103.0, 105.0, 108.0, 110.0, 107.0, 104.0, 102.0,
        ];

        prices
            .iter()
            .map(|&price| TickData {
                price,
                volume: 100.0,
                ts: now,
            })
            .collect()
    }

    fn create_test_bars() -> Vec<Bar> {
        let now = Utc::now();
        (0..20)
            .map(|i| {
                let base = 100.0 + (i as f64 % 10.0);
                Bar {
                    time: now,
                    open: base,
                    high: base + 3.0,
                    low: base - 2.0,
                    close: base + 1.0,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_config_creation() {
        let config = RangeBarConfig::new(5.0);
        assert!((config.range_size - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_range_bars_from_ticks() {
        let ticks = create_test_ticks();
        let config = RangeBarConfig::new(5.0);
        let bars = to_range_bars_from_ticks(&ticks, &config);

        assert!(!bars.is_empty());

        // Each completed bar should have range approximately equal to config
        for bar in &bars[..bars.len().saturating_sub(1)] {
            // Skip last (incomplete) bar
            assert!(bar.range() <= config.range_size + 0.01);
        }
    }

    #[test]
    fn test_range_bars_from_ohlc() {
        let data = create_test_bars();
        let config = RangeBarConfig::new(5.0);
        let bars = to_range_bars_from_ohlc(&data, &config);

        assert!(!bars.is_empty());
    }

    #[test]
    fn test_range_bar_properties() {
        let bar = RangeBar {
            open: 100.0,
            high: 105.0,
            low: 100.0,
            close: 105.0,
            volume: 1000.0,
            start_time: Utc::now(),
            end_time: Utc::now(),
            range_size: 5.0,
            tick_cnt: 10,
        };

        assert!(bar.is_bullish());
        assert!(!bar.is_bearish());
        assert!((bar.range() - 5.0).abs() < 0.01);
        assert!((bar.body() - 5.0).abs() < 0.01);
    }
}
