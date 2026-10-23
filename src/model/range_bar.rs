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

/// Build range bars from raw tick data (exact construction). A tick's volume that would make its
/// bar's volume overflow is left out.
///
/// # Safety Limits
/// - At most the newest 5,000 range bars
pub fn to_range_bars_from_ticks(ticks: &[TickData], config: &RangeBarConfig) -> Vec<RangeBar> {
    if ticks.is_empty() {
        return Vec::new();
    }

    const MAX_BARS: usize = 5_000;
    let range_size = config.range_size;
    let mut bars: Vec<RangeBar> = Vec::new();

    let first_tick = &ticks[0];
    let mut curr_bar = RangeBar {
        open: first_tick.price,
        high: first_tick.price,
        low: first_tick.price,
        close: first_tick.price,
        volume: add_volume(0.0, first_tick.volume),
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
        curr_bar.volume = add_volume(curr_bar.volume, tick.volume);
        curr_bar.end_time = tick.ts;
        curr_bar.tick_cnt += 1;

        // A tick that crosses several ranges closes one bar per range; a range of 0 or less closes none.
        // A tick closes at most 100 bars, which bounds the work for one bad print; raise it if gaps
        // that large must chart in full.
        const MAX_CLOSES: usize = 100;
        let mut closes = 0;
        while curr_bar.range() >= range_size && range_size > 0.0 && closes < MAX_CLOSES {
            closes += 1;
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
            if bars.len() == 2 * MAX_BARS {
                bars.drain(..MAX_BARS);
            }

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
                tick_cnt: 1,
            };
        }
    }

    // The incomplete bar holds the last trade, unless that trade only closed the previous bar exactly.
    if bars.is_empty() || curr_bar.tick_cnt > 1 || curr_bar.range() > 0.0 {
        bars.push(curr_bar);
    }
    bars.drain(..bars.len().saturating_sub(MAX_BARS));
    bars
}

/// Convert OHLC bars to range bars (approximation). A bar's volume that would make its range bar's
/// volume overflow is left out.
///
/// # Safety Limits
/// - At most the newest 5,000 range bars
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
        volume: add_volume(0.0, first.volume),
        start_time: first.time,
        end_time: first.time,
        range_size,
        tick_cnt: 1,
    };

    for bar in data.iter().skip(1) {
        // Simulate processing this bar's range
        let prices = [bar.open, bar.high, bar.low, bar.close];

        for &price in &prices {
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

                // Bound memory by dropping the oldest bars; `curr_bar` never reads them.
                if bars.len() == 2 * MAX_BARS {
                    bars.drain(..MAX_BARS);
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
        curr_bar.volume = add_volume(curr_bar.volume, bar.volume);
        curr_bar.end_time = bar.time;
        curr_bar.tick_cnt += 1;
    }

    // Add incomplete bar
    if curr_bar.tick_cnt > 0 {
        bars.push(curr_bar);
    }

    bars.drain(..bars.len().saturating_sub(MAX_BARS));
    bars
}

/// `volume` plus `added`, or `volume` alone when the sum is not finite: two valid volumes can add
/// up to infinity.
fn add_volume(volume: f64, added: f64) -> f64 {
    let sum = volume + added;
    if sum.is_finite() { sum } else { volume }
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
    fn range_bars_from_ticks_follow_every_tick() {
        // (range, tick prices, (open, high, low, close) of each bar)
        type Ohlc = (f64, f64, f64, f64);
        let cases: [(f64, &[f64], &[Ohlc]); 4] = [
            (
                10.0,
                &[100.0, 150.0],
                &[
                    (100.0, 110.0, 100.0, 110.0),
                    (110.0, 120.0, 110.0, 120.0),
                    (120.0, 130.0, 120.0, 130.0),
                    (130.0, 140.0, 130.0, 140.0),
                    (140.0, 150.0, 140.0, 150.0),
                ],
            ),
            (
                10.0,
                &[100.0, 105.0, 112.0],
                &[(100.0, 110.0, 100.0, 110.0), (110.0, 112.0, 110.0, 112.0)],
            ),
            // A last tick exactly on a boundary adds no zero-range bar with the previous bar's time.
            (10.0, &[100.0, 110.0], &[(100.0, 110.0, 100.0, 110.0)]),
            // A range of 0 closes nothing and still terminates.
            (0.0, &[100.0, 0.0], &[(100.0, 100.0, 0.0, 0.0)]),
        ];
        for (range, prices, expected) in cases {
            let ticks: Vec<TickData> = prices
                .iter()
                .enumerate()
                .map(|(i, &price)| TickData {
                    price,
                    volume: 1.0,
                    ts: DateTime::from_timestamp(i as i64, 0).unwrap(),
                })
                .collect();
            let bars: Vec<_> = to_range_bars_from_ticks(&ticks, &RangeBarConfig::new(range))
                .iter()
                .map(|b| (b.open, b.high, b.low, b.close))
                .collect();
            assert_eq!(bars, expected, "{prices:?} range {range}");
        }
    }

    #[test]
    fn test_range_bars_from_ohlc() {
        let data = create_test_bars();
        let config = RangeBarConfig::new(5.0);
        let bars = to_range_bars_from_ohlc(&data, &config);

        assert!(!bars.is_empty());
    }

    /// Guards the volume scale: an infinite first volume, or two valid volumes near `f64::MAX`,
    /// must not give a range bar an infinite volume, from ticks or from bars.
    #[test]
    fn a_volume_that_would_overflow_is_left_out() {
        let ts = Utc::now();
        let tick = |volume| TickData {
            price: 100.0,
            volume,
            ts,
        };
        let bar = |volume| Bar::new(ts, 100.0, 100.0, 100.0, 100.0, volume);
        let volumes = [f64::INFINITY, f64::MAX, f64::MAX];
        let config = RangeBarConfig::new(5.0);
        let from_ticks = to_range_bars_from_ticks(&volumes.map(tick), &config);
        let from_bars = to_range_bars_from_ohlc(&volumes.map(bar), &config);
        let volumes = |bars: Vec<RangeBar>| Vec::from_iter(bars.iter().map(|bar| bar.volume));
        assert_eq!(volumes(from_ticks), [f64::MAX]);
        assert_eq!(volumes(from_bars), [f64::MAX]);
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
