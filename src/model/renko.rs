//! Renko chart transformation.
//!
//! Renko charts filter out time entirely, plotting uniform-sized bricks
//! that represent a fixed price movement. A new brick is drawn only when
//! price moves by at least [`brick_size`](RenkoConfig::brick_size) from
//! the close of the previous brick. This makes trends and reversals very
//! easy to spot.
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
    let mut curr_price = (bars[0].close / brick_size).floor() * brick_size;

    for bar in bars.iter() {
        let price = bar.close;

        // Calculate how many bricks we can form
        let price_diff = price - curr_price;
        let num_bricks = (price_diff.abs() / brick_size).floor() as i32;

        if num_bricks > 0 {
            let direction = if price_diff > 0.0 {
                RenkoDirection::Up
            } else {
                RenkoDirection::Down
            };

            // Create bricks for this price movement (with safety limit)
            let bricks_to_create = (num_bricks as usize).min(MAX_BRICKS - bricks.len());
            for _ in 0..bricks_to_create {
                let brick_open = curr_price;
                let brick_close = if direction == RenkoDirection::Up {
                    curr_price + brick_size
                } else {
                    curr_price - brick_size
                };

                bricks.push(RenkoBrick {
                    ts: bar.time,
                    open: brick_open,
                    close: brick_close,
                    direction,
                });

                curr_price = brick_close;

                // Safety check
                if bricks.len() >= MAX_BRICKS {
                    return bricks;
                }
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
    fn test_renko_brick_creation() {
        let bars = create_test_bars();
        let config = RenkoConfig::new(2.0);
        let bricks = to_renko_bricks(&bars, &config);

        assert!(!bricks.is_empty());
        // With brick size of 2.0, price moving from 101 to 106 should create bricks
        assert!(bricks.len() >= 2);
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
