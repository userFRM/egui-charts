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

use crate::model::Bar;
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
            high: self.open.max(self.close),
            low: self.open.min(self.close),
            close: self.close,
            volume: 0.0, // Renko doesn't use volume
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
}

impl Default for RenkoConfig {
    fn default() -> Self {
        Self { brick_size: 1.0 }
    }
}

impl RenkoConfig {
    /// Create a Renko config with an explicit brick size in price units.
    pub fn new(brick_size: f64) -> Self {
        Self { brick_size }
    }

    /// Calculate brick size from ATR (Avg True Range)
    pub fn from_atr(bars: &[Bar], period: usize, multiplier: f64) -> Self {
        let atr = calculate_atr(bars, period);
        Self {
            brick_size: atr * multiplier,
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
/// # Safety Limits
/// - Maximum 10,000 bricks to prevent memory issues
/// - Auto-adjusts brick size if too small relative to price range
pub fn to_renko_bricks(bars: &[Bar], config: &RenkoConfig) -> Vec<RenkoBrick> {
    if bars.is_empty() {
        return Vec::new();
    }

    // Calculate price range to validate brick size
    let (min_price, max_price) = bars.iter().fold((f64::MAX, f64::MIN), |(min, max), bar| {
        (min.min(bar.low), max.max(bar.high))
    });
    let price_range = max_price - min_price;

    // Auto-adjust brick size if it would create too many bricks
    // Aim for roughly 50-200 bricks maximum for visible data
    let min_brick_size = price_range / 200.0;
    let brick_size = config.brick_size.max(min_brick_size).max(0.0001); // Prevent zero/negative

    let mut bricks = Vec::new();
    const MAX_BRICKS: usize = 10_000;

    // Start with the first bar's close price, aligned to brick boundary
    let start = (bars[0].close / brick_size).floor() * brick_size;
    // Open and close of the last brick, or the start price before the first brick.
    let mut last = (start, start);

    for bar in bars.iter() {
        let price = bar.close;
        loop {
            let (open, close) = last;
            let (low, high) = (open.min(close), open.max(close));
            // Before the first brick both ends are the start, so either direction needs one brick.
            let (brick_open, brick_close) = if price >= high + brick_size {
                (high, high + brick_size)
            } else if price <= low - brick_size {
                (low, low - brick_size)
            } else {
                break;
            };
            // A box below the precision of the price moves nothing.
            if brick_close == brick_open {
                break;
            }
            let up = brick_close > brick_open;

            bricks.push(RenkoBrick {
                ts: bar.time,
                open: brick_open,
                close: brick_close,
                direction: if up {
                    RenkoDirection::Up
                } else {
                    RenkoDirection::Down
                },
            });
            last = (brick_open, brick_close);

            // Safety check
            if bricks.len() >= MAX_BRICKS {
                return bricks;
            }
        }
    }

    bricks
}

/// Calculate Avg True Range for automatic brick sizing
fn calculate_atr(bars: &[Bar], period: usize) -> f64 {
    if bars.len() < period {
        // Fallback to simple range
        let sum: f64 = bars.iter().map(|b| b.high - b.low).sum();
        return sum / bars.len() as f64;
    }

    let mut true_ranges = Vec::new();

    for i in 1..bars.len() {
        let high_low = bars[i].high - bars[i].low;
        let high_close_prev = (bars[i].high - bars[i - 1].close).abs();
        let low_close_prev = (bars[i].low - bars[i - 1].close).abs();

        let true_range = high_low.max(high_close_prev).max(low_close_prev);
        true_ranges.push(true_range);
    }

    // Simple moving avg of true ranges
    let sum: f64 = true_ranges
        .iter()
        .skip(true_ranges.len().saturating_sub(period))
        .sum();
    sum / period.min(true_ranges.len()) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        vec![
            Bar {
                time: start,
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 101.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(1),
                open: 101.0,
                high: 105.0,
                low: 100.0,
                close: 104.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(2),
                open: 104.0,
                high: 107.0,
                low: 103.0,
                close: 106.0,
                volume: 1000.0,
            },
        ]
    }

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
            direction: RenkoDirection::Up,
        };

        let bar = brick.to_bar();
        assert_eq!(bar.open, 100.0);
        assert_eq!(bar.close, 102.0);
        assert_eq!(bar.high, 102.0);
        assert_eq!(bar.low, 100.0);
    }

    #[test]
    fn test_atr_calculation() {
        let bars = create_test_bars();
        let atr = calculate_atr(&bars, 2);
        assert!(atr > 0.0);
    }
}
