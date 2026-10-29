//! Range-bar chart transformation.
//!
//! Build bars with a configured price range rather than a fixed time interval.
//! Use [`to_range_bars_from_ticks`] with executions, or
//! [`to_range_bars_from_ohlc`] for an approximation from existing bars. The newest
//! output bar can be incomplete.

use super::PriceBuilder;
use super::Tick;
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
            colors: None,
            payload: None,
        }
    }
}

/// What a chart's range bars are built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeBarSource {
    /// The trades the feed supplied, in order, with their timestamps and volume.
    Executions,
    /// The open, high, low and close of the chart's bars: an approximation, used while the chart
    /// holds no trades.
    OhlcApproximation,
}

/// Build range bars from trades (exact construction). A trade's size that would make its bar's
/// volume overflow is left out.
///
/// # Safety Limits
/// - At most the newest 5,000 range bars
pub fn to_range_bars_from_ticks(ticks: &[Tick], config: &RangeBarConfig) -> Vec<RangeBar> {
    let mut builder = RangeBarBuilder::new(config.range_size);
    for tick in ticks {
        builder.push_tick(tick);
    }
    builder.elements()
}

/// Convert OHLC bars to range bars (approximation). A bar's volume that would make its range bar's
/// volume overflow is left out.
///
/// Returns no bars when the range size is not a positive finite number.
///
/// # Safety Limits
/// - When the bars move by more than two million range sizes, the range bars are built from the
///   latest bar after which they move at most that much
/// - At most the newest 5,000 range bars
pub fn to_range_bars_from_ohlc(data: &[Bar], config: &RangeBarConfig) -> Vec<RangeBar> {
    // The range size, from ATR when the config asks for it
    let range_size = if config.use_atr && data.len() >= config.atr_period {
        calculate_atr(data, config.atr_period) * config.atr_multiplier
    } else {
        config.range_size
    };
    if data.is_empty() || !(range_size > 0.0 && range_size.is_finite()) {
        return Vec::new();
    }
    let mut builder = RangeBarBuilder::new(range_size);
    for bar in data[work_start(data, range_size)..]
        .iter()
        .filter(|bar| !bar.is_whitespace())
    {
        builder.push(bar);
    }
    builder.elements()
}

pub(crate) const MAX_BARS: usize = 5_000;

/// Builds range bars from OHLC bars one at a time; `range_size` must be a positive finite
/// number. The range bar in progress is kept apart from the closed bars and added by `elements`.
#[derive(Clone)]
pub(crate) struct RangeBarBuilder {
    range_size: f64,
    /// The range bar in progress; `None` until the first bar.
    curr_bar: Option<RangeBar>,
    bars: Vec<RangeBar>,
}

impl RangeBarBuilder {
    pub(crate) fn new(range_size: f64) -> Self {
        Self {
            range_size,
            curr_bar: None,
            bars: Vec::new(),
        }
    }

    /// Adds one trade, after the bars or trades already added. A trade that crosses several
    /// ranges closes one bar per range; a range of 0 or less closes none. A trade closes at most
    /// 100 bars, which bounds the work for one bad print; raise it if gaps that large must chart
    /// in full. The bar that follows a close holds the trade only when it lies beyond the close.
    pub(crate) fn push_tick(&mut self, tick: &Tick) {
        let range_size = self.range_size;
        let Some(mut curr_bar) = self.curr_bar.take() else {
            self.curr_bar = Some(RangeBar {
                open: tick.price,
                high: tick.price,
                low: tick.price,
                close: tick.price,
                volume: add_volume(0.0, tick.size),
                start_time: tick.ts,
                end_time: tick.ts,
                range_size,
                tick_cnt: 1,
            });
            return;
        };
        curr_bar.high = curr_bar.high.max(tick.price);
        curr_bar.low = curr_bar.low.min(tick.price);
        curr_bar.close = tick.price;
        curr_bar.volume = add_volume(curr_bar.volume, tick.size);
        curr_bar.end_time = tick.ts;
        curr_bar.tick_cnt += 1;

        const MAX_CLOSES: usize = 100;
        let mut closes = 0;
        while curr_bar.range() >= range_size && range_size > 0.0 && closes < MAX_CLOSES {
            closes += 1;
            if tick.price >= curr_bar.open {
                curr_bar.close = curr_bar.low + range_size;
                curr_bar.high = curr_bar.close;
            } else {
                curr_bar.close = curr_bar.high - range_size;
                curr_bar.low = curr_bar.close;
            }
            self.bars.push(curr_bar.clone());
            if self.bars.len() == 2 * MAX_BARS {
                self.bars.drain(..MAX_BARS);
            }

            // Start the next bar where the last one ended
            curr_bar = RangeBar {
                open: curr_bar.close,
                high: tick.price.max(curr_bar.close),
                low: tick.price.min(curr_bar.close),
                close: tick.price,
                volume: 0.0,
                start_time: tick.ts,
                end_time: tick.ts,
                range_size,
                tick_cnt: usize::from(tick.price != curr_bar.close),
            };
        }
        self.curr_bar = Some(curr_bar);
    }
}

impl PriceBuilder for RangeBarBuilder {
    type Element = RangeBar;

    fn push(&mut self, bar: &Bar) {
        let range_size = self.range_size;
        let Some(mut curr_bar) = self.curr_bar.take() else {
            self.curr_bar = Some(RangeBar {
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
                volume: add_volume(0.0, bar.volume),
                start_time: bar.time,
                end_time: bar.time,
                range_size,
                tick_cnt: 1,
            });
            return;
        };

        // Simulate processing this bar's range
        let prices = [bar.open, bar.high, bar.low, bar.close];

        for &price in &prices {
            curr_bar.high = curr_bar.high.max(price);
            curr_bar.low = curr_bar.low.min(price);

            while curr_bar.range() >= range_size {
                // Complete the bar
                if curr_bar.high - curr_bar.open >= range_size {
                    // A size below the precision of the price moves nothing
                    if curr_bar.low + range_size == curr_bar.low {
                        break;
                    }
                    // Closed up
                    curr_bar.close = curr_bar.low + range_size;
                    curr_bar.high = curr_bar.close;
                } else {
                    if curr_bar.high - range_size == curr_bar.high {
                        break;
                    }
                    // Closed down
                    curr_bar.close = curr_bar.high - range_size;
                    curr_bar.low = curr_bar.close;
                }

                curr_bar.end_time = bar.time;
                self.bars.push(curr_bar.clone());

                // Bound memory by dropping the oldest bars; `curr_bar` never reads them.
                if self.bars.len() == 2 * MAX_BARS {
                    self.bars.drain(..MAX_BARS);
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
        self.curr_bar = Some(curr_bar);
    }

    fn elements(&self) -> Vec<RangeBar> {
        let mut bars = self.bars.clone();
        // The incomplete bar, if it has any ticks
        bars.extend(self.curr_bar.clone().filter(|b| b.tick_cnt > 0));
        bars.drain(..bars.len().saturating_sub(MAX_BARS));
        bars
    }
}

/// `volume` plus `added`, or `volume` alone when the sum is not finite: two valid volumes can add
/// up to infinity.
fn add_volume(volume: f64, added: f64) -> f64 {
    let sum = volume + added;
    if sum.is_finite() { sum } else { volume }
}

/// Index of the bar to build from so that the range bars move at most `MAX_WORK` range sizes.
/// Bar `i` costs its path through open, high, low and close in sizes plus 4; the returned bar
/// only seeds the state, and its own range is split at the next bar's first price.
pub(crate) fn work_start(bars: &[Bar], size: f64) -> usize {
    let mut total = 0.0;
    let mut priced = bars
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, bar)| !bar.is_whitespace());
    let Some((mut i, mut b)) = priced.next() else {
        return 0;
    };
    for (previous_i, prev) in priced {
        total += (((b.open - prev.close).abs()
            + (b.high - b.open).abs()
            + (b.high - b.low)
            + (b.close - b.low).abs())
            / size)
            .max(0.0)
            + 4.0;
        if total + (prev.high - prev.low) / size > super::MAX_WORK {
            return i;
        }
        (i, b) = (previous_i, prev);
    }
    0
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
            let ticks: Vec<Tick> = prices
                .iter()
                .enumerate()
                .map(|(i, &price)| {
                    Tick::new(
                        DateTime::from_timestamp(i as i64, 0).unwrap(),
                        price,
                        1.0,
                        None,
                    )
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
        let first = DateTime::UNIX_EPOCH;
        let next = first + chrono::Duration::minutes(1);
        let data = [
            Bar::new(first, 100.0, 100.0, 100.0, 100.0, 10.0),
            Bar::new(next, 112.0, 112.0, 112.0, 112.0, 20.0),
        ];
        // A 12-point advance closes two five-point ranges and leaves a two-point tail.
        let bars = to_range_bars_from_ohlc(&data, &RangeBarConfig::new(5.0));
        let actual: Vec<_> = bars
            .iter()
            .map(|b| {
                (
                    b.open,
                    b.high,
                    b.low,
                    b.close,
                    b.volume,
                    b.start_time,
                    b.end_time,
                )
            })
            .collect();
        assert_eq!(
            actual,
            [
                (100.0, 105.0, 100.0, 105.0, 10.0, first, next),
                (105.0, 110.0, 105.0, 110.0, 0.0, next, next),
                (110.0, 112.0, 110.0, 112.0, 20.0, next, next),
            ]
        );
    }

    /// Guards the volume scale: an infinite first volume, or two valid volumes near `f64::MAX`,
    /// must not give a range bar an infinite volume, from ticks or from bars.
    #[test]
    fn a_volume_that_would_overflow_is_left_out() {
        let ts = Utc::now();
        let tick = |volume| Tick::new(ts, 100.0, volume, None);
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
