//! Renko chart transformation.
//!
//! Renko charts filter out time entirely, plotting uniform-sized bricks
//! that represent a fixed price movement. A brick in the direction of the
//! trend is drawn when the close moves one [`brick_size`](RenkoConfig::brick_size)
//! beyond the close of the previous brick. A reversal needs a two-brick move:
//! the close must move one brick beyond the open of the previous brick, and
//! the reversal brick starts at that open.
//!
//! Use [`to_renko_bricks`] to transform OHLCV bars into Renko bricks.

use super::PriceBasedSource;
use crate::model::{Bar, PriceBuilder};
use chrono::{DateTime, Utc};

/// A single Renko brick representing a fixed-size price movement.
#[derive(Debug, Clone)]
pub struct RenkoBrick {
    /// Timestamp of the source bar that completed this brick.
    pub ts: DateTime<Utc>,
    /// Opening price (bottom of an Up brick, top of a Down brick).
    pub open: f64,
    /// Closing price (top of an Up brick, bottom of a Down brick).
    pub close: f64,
    /// Highest source price of this brick, including an upper wick on a down brick.
    pub high: f64,
    /// Lowest source price of this brick, including a lower wick on an up brick.
    pub low: f64,
    /// Direction of this brick.
    pub direction: RenkoDirection,
}

/// Direction of a Renko brick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenkoDirection {
    /// Bullish brick (price moved up by one brick size).
    Up,
    /// Bearish brick (price moved down by one brick size).
    Down,
}

impl RenkoBrick {
    /// Convert to a Bar for rendering compatibility
    pub fn to_bar(&self) -> Bar {
        Bar {
            time: self.ts,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: 0.0, // Renko doesn't use volume
            colors: None,
            payload: None,
        }
    }
}

/// Configuration for the Renko chart transformation.
///
/// The brick size can be set directly or derived from ATR via
/// [`from_atr`](Self::from_atr).
#[derive(Debug, Clone)]
pub struct RenkoConfig {
    /// Size of each brick in price units
    pub brick_size: f64,
    /// Price input; high/low gives continuation priority when a bar crosses both thresholds.
    pub source: PriceBasedSource,
}

impl Default for RenkoConfig {
    fn default() -> Self {
        Self {
            brick_size: 1.0,
            source: PriceBasedSource::Close,
        }
    }
}

impl RenkoConfig {
    /// Create a Renko config with an explicit brick size in price units.
    pub fn new(brick_size: f64) -> Self {
        Self {
            brick_size,
            source: PriceBasedSource::Close,
        }
    }

    /// Calculate brick size from ATR (Avg True Range)
    pub fn from_atr(bars: &[Bar], period: usize, multiplier: f64) -> Self {
        Self {
            brick_size: super::BoxSizeMethod::Atr { period, multiplier }
                .resolve(0.0, bars)
                .unwrap_or(f64::NAN),
            source: PriceBasedSource::Close,
        }
    }
}

/// Transform bars into Renko bricks
///
/// The first brick is drawn once a close moves one brick either way from the
/// first close rounded down to a multiple of the brick size. After that, a
/// brick continues the trend when a close reaches one brick beyond the last
/// brick's close, and reverses it when a close reaches one brick beyond the
/// last brick's open, so a reversal needs a two-brick move.
///
/// Returns no bricks when the brick size is not a positive finite number.
///
/// # Safety Limits
/// - When the bars move by more than two million brick sizes, the bricks are built from the latest
///   bar after which they move at most that much
/// - At most the newest 10,000 bricks
pub fn to_renko_bricks(bars: &[Bar], config: &RenkoConfig) -> Vec<RenkoBrick> {
    let brick_size = config.brick_size;
    if bars.is_empty() || !(brick_size > 0.0 && brick_size.is_finite()) {
        return Vec::new();
    }
    let mut builder = RenkoBuilder::new(brick_size, config.source);
    for bar in bars[work_start(bars, brick_size, config.source)..]
        .iter()
        .filter(|bar| !bar.is_whitespace())
    {
        builder.push(bar);
    }
    builder.elements()
}

pub(crate) const MAX_BRICKS: usize = 10_000;

/// Builds Renko bricks bar by bar; `size` must be a positive finite number.
#[derive(Clone)]
pub(crate) struct RenkoBuilder {
    size: f64,
    source: PriceBasedSource,
    extremes: Option<(f64, f64)>,
    /// Open and close of the last brick, or the start price before the first brick; `None`
    /// until the first bar.
    last: Option<(f64, f64)>,
    bricks: Vec<RenkoBrick>,
}

impl RenkoBuilder {
    pub(crate) fn new(size: f64, source: PriceBasedSource) -> Self {
        Self {
            size,
            source,
            extremes: None,
            last: None,
            bricks: Vec::new(),
        }
    }
}

impl PriceBuilder for RenkoBuilder {
    type Element = RenkoBrick;

    fn push(&mut self, bar: &Bar) {
        let size = self.size;
        let Some(mut last) = self.last else {
            let start = (bar.close / size).floor() * size;
            self.last = Some((start, start));
            self.extremes = Some((bar.close, bar.close));
            return;
        };
        let (low, high) = match self.source {
            PriceBasedSource::Close => (bar.close, bar.close),
            PriceBasedSource::HighLow => (bar.low, bar.high),
        };
        let (prior_low, prior_high) = self.extremes.unwrap_or((bar.close, bar.close));
        let (mut wick_low, mut wick_high) = (prior_low.min(low), prior_high.max(high));
        let (open, close) = last;
        let price = match self.source {
            PriceBasedSource::Close => bar.close,
            PriceBasedSource::HighLow if close >= open => {
                if high >= close + size {
                    high
                } else {
                    low
                }
            }
            PriceBasedSource::HighLow => {
                if low <= close - size {
                    low
                } else {
                    high
                }
            }
        };
        loop {
            let (open, close) = last;
            let (low, high) = (open.min(close), open.max(close));
            // Before the first brick both ends are the start, so either direction needs one brick.
            let (brick_open, brick_close) = if price >= high + size {
                (high, high + size)
            } else if price <= low - size {
                (low, low - size)
            } else {
                break;
            };
            // A box below the precision of the price moves nothing.
            if brick_close == brick_open {
                break;
            }
            let up = brick_close > brick_open;

            self.bricks.push(RenkoBrick {
                ts: bar.time,
                open: brick_open,
                close: brick_close,
                high: if up {
                    brick_close
                } else {
                    wick_high.max(brick_open)
                },
                low: if up {
                    wick_low.min(brick_open)
                } else {
                    brick_close
                },
                direction: if up {
                    RenkoDirection::Up
                } else {
                    RenkoDirection::Down
                },
            });
            last = (brick_open, brick_close);
            wick_low = brick_close;
            wick_high = brick_close;

            // Bound memory by dropping the oldest bricks; `last` never reads them.
            if self.bricks.len() == 2 * MAX_BRICKS {
                self.bricks.drain(..MAX_BRICKS);
            }
        }
        self.last = Some(last);
        // Keep the unfinished move beyond the last brick for the next wick.
        self.extremes = Some((wick_low.min(price), wick_high.max(price)));
    }

    fn elements(&self) -> Vec<RenkoBrick> {
        self.bricks[self.bricks.len().saturating_sub(MAX_BRICKS)..].to_vec()
    }
}

/// Index of the bar to build from so that the bricks move at most `MAX_WORK` brick sizes. Bar
/// `i` costs its close-to-close move in sizes plus 2; the returned bar only seeds the state.
pub(crate) fn work_start(bars: &[Bar], size: f64, source: PriceBasedSource) -> usize {
    let mut total = 0.0;
    let mut priced = bars
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, bar)| !bar.is_whitespace());
    let Some((mut i, mut bar)) = priced.next() else {
        return 0;
    };
    for (previous_i, previous) in priced {
        let movement = match source {
            PriceBasedSource::Close => (bar.close - previous.close).abs(),
            PriceBasedSource::HighLow => bar.high.max(previous.high) - bar.low.min(previous.low),
        };
        total += (movement / size).max(0.0) + 2.0;
        if total > super::MAX_WORK {
            return i;
        }
        (i, bar) = (previous_i, previous);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    #[test]
    fn bricks_reverse_only_after_a_two_box_move() {
        let cases = [
            (
                vec![100.0, 103.0, 102.0],
                vec![(100.0, 101.0), (101.0, 102.0), (102.0, 103.0)],
            ),
            (
                vec![100.0, 103.0, 101.0],
                vec![
                    (100.0, 101.0),
                    (101.0, 102.0),
                    (102.0, 103.0),
                    (102.0, 101.0),
                ],
            ),
            (
                vec![100.0, 103.0, 101.0, 102.0, 103.0],
                vec![
                    (100.0, 101.0),
                    (101.0, 102.0),
                    (102.0, 103.0),
                    (102.0, 101.0),
                    (102.0, 103.0),
                ],
            ),
            (vec![100.0, 98.0], vec![(100.0, 99.0), (99.0, 98.0)]),
            (vec![100.0, 100.9], vec![]),
            // At 2^53 one box rounds away above the price but not below it.
            (vec![9_007_199_254_740_992.0; 2], vec![]),
        ];
        let start = Utc::now();
        for (closes, expected) in &cases {
            let bars: Vec<Bar> = closes
                .iter()
                .enumerate()
                .map(|(i, &c)| Bar {
                    time: start + Duration::minutes(i as i64),
                    open: c,
                    high: c,
                    low: c,
                    close: c,
                    volume: 0.0,
                    colors: None,
                    payload: None,
                })
                .collect();
            let got: Vec<(f64, f64)> = to_renko_bricks(&bars, &RenkoConfig::new(1.0))
                .iter()
                .map(|b| (b.open, b.close))
                .collect();
            assert_eq!(got, *expected, "closes {closes:?}");
        }
    }

    #[test]
    fn test_renko_brick_to_bar() {
        let brick = RenkoBrick {
            ts: Utc::now(),
            open: 100.0,
            close: 102.0,
            high: 102.0,
            low: 99.5,
            direction: RenkoDirection::Up,
        };

        let bar = brick.to_bar();
        assert_eq!(bar.open, 100.0);
        assert_eq!(bar.close, 102.0);
        assert_eq!(bar.high, 102.0);
        assert_eq!(bar.low, 99.5);
    }
}
