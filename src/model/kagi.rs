//! Kagi chart transformation.
//!
//! Kagi charts display price movements as a series of connected vertical
//! lines whose thickness changes based on trend direction:
//!
//! - **Thick (Yin)** -- uptrend; price has broken above the previous
//!   significant high.
//! - **Thin (Yang)** -- downtrend; price has broken below the previous
//!   significant low.
//!
//! A new line segment is created whenever price reverses by at least the
//! configured [`reversal_amount`](KagiConfig::reversal_amount). Time is
//! ignored -- only price movement matters.
//!
//! Use [`to_kagi_lines`] to transform a slice of [`Bar`]s.

use crate::model::Bar;
use chrono::{DateTime, Utc};

/// A single vertical segment in a Kagi chart.
#[derive(Debug, Clone)]
pub struct KagiLine {
    /// Timestamp when this segment was created (from the source bar).
    pub ts: DateTime<Utc>,
    /// Price at the beginning of this segment.
    pub start_price: f64,
    /// Price at the end of this segment.
    pub end_price: f64,
    /// Line thickness indicating trend state.
    pub thickness: KagiThickness,
}

/// Kagi line thickness, encoding the current trend direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KagiThickness {
    /// Thin line -- downtrend (Yang).
    Thin,
    /// Thick line -- uptrend (Yin).
    Thick,
}

impl KagiLine {
    /// Convert to a Bar for rendering compatibility
    pub fn to_bar(&self) -> Bar {
        Bar {
            time: self.ts,
            open: self.start_price,
            high: self.start_price.max(self.end_price),
            low: self.start_price.min(self.end_price),
            close: self.end_price,
            volume: 0.0, // Kagi doesn't use volume
        }
    }

    /// Check if this line is moving up
    pub fn is_up(&self) -> bool {
        self.end_price > self.start_price
    }
}

/// Configuration for the Kagi chart transformation.
///
/// The reversal amount can be set directly, derived from ATR, or
/// computed as a percentage of price.
#[derive(Debug, Clone)]
pub struct KagiConfig {
    /// Reversal amount - price movement needed to create a new line
    pub reversal_amount: f64,
}

impl Default for KagiConfig {
    fn default() -> Self {
        Self {
            reversal_amount: 1.0,
        }
    }
}

impl KagiConfig {
    /// Create a Kagi config with an explicit reversal amount in price units.
    pub fn new(reversal_amount: f64) -> Self {
        Self { reversal_amount }
    }

    /// Calculate reversal amount from ATR (Avg True Range)
    pub fn from_atr(bars: &[Bar], period: usize, multiplier: f64) -> Self {
        let atr = calculate_atr(bars, period);
        Self {
            reversal_amount: atr * multiplier,
        }
    }

    /// Calculate reversal amount as percentage of price
    pub fn from_percentage(base_price: f64, percentage: f64) -> Self {
        Self {
            reversal_amount: base_price * (percentage / 100.0),
        }
    }
}

/// Transform bars into Kagi lines
///
/// # Safety Limits
/// - Maximum 10,000 lines to prevent memory issues
/// - Auto-adjusts reversal amount if too small relative to price range
pub fn to_kagi_lines(bars: &[Bar], config: &KagiConfig) -> Vec<KagiLine> {
    if bars.is_empty() {
        return Vec::new();
    }

    // Calculate price range to validate reversal amount
    let (min_price, max_price) = bars.iter().fold((f64::MAX, f64::MIN), |(min, max), bar| {
        (min.min(bar.low), max.max(bar.high))
    });
    let price_range = max_price - min_price;

    // Auto-adjust reversal amount if it would create too many lines
    let min_reversal = price_range / 500.0;
    let reversal = config.reversal_amount.max(min_reversal).max(0.0001);

    let mut lines = Vec::new();
    const MAX_LINES: usize = 10_000;

    // Track current line state
    let mut curr_price = bars[0].close;
    let mut line_start_price = bars[0].close;
    let mut line_direction_up = true;
    let mut last_ts = bars[0].time;

    // Track significant highs and lows for thickness changes
    let mut significant_high = curr_price;
    let mut significant_low = curr_price;
    let mut is_thick = curr_price > line_start_price;

    for bar in bars.iter().skip(1) {
        last_ts = bar.time;
        let price = bar.close;

        // Check for continuation or reversal
        if line_direction_up {
            // Currently moving up
            if price > curr_price {
                // Continue up
                curr_price = price;
                if price > significant_high {
                    significant_high = price;
                }
            } else if price < (curr_price - reversal) {
                // Reversal down
                // Save current line
                lines.push(KagiLine {
                    ts: last_ts,
                    start_price: line_start_price,
                    end_price: curr_price,
                    thickness: if is_thick {
                        KagiThickness::Thick
                    } else {
                        KagiThickness::Thin
                    },
                });

                // Safety check
                if lines.len() >= MAX_LINES {
                    return lines;
                }

                // Start new line going down
                line_start_price = curr_price;
                curr_price = price;
                line_direction_up = false;

                // Check thickness change
                if price < significant_low {
                    is_thick = false; // Switch to thin (downtrend)
                    significant_low = price;
                }
            }
        } else {
            // Currently moving down
            if price < curr_price {
                // Continue down
                curr_price = price;
                if price < significant_low {
                    significant_low = price;
                }
            } else if price > (curr_price + reversal) {
                // Reversal up
                // Save current line
                lines.push(KagiLine {
                    ts: last_ts,
                    start_price: line_start_price,
                    end_price: curr_price,
                    thickness: if is_thick {
                        KagiThickness::Thick
                    } else {
                        KagiThickness::Thin
                    },
                });

                // Safety check
                if lines.len() >= MAX_LINES {
                    return lines;
                }

                // Start new line going up
                line_start_price = curr_price;
                curr_price = price;
                line_direction_up = true;

                // Check thickness change
                if price > significant_high {
                    is_thick = true; // Switch to thick (uptrend)
                    significant_high = price;
                }
            }
        }
    }

    // Add final line
    if line_start_price != curr_price {
        lines.push(KagiLine {
            ts: last_ts,
            start_price: line_start_price,
            end_price: curr_price,
            thickness: if is_thick {
                KagiThickness::Thick
            } else {
                KagiThickness::Thin
            },
        });
    }

    lines
}

/// Calculate Avg True Range for automatic reversal sizing
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
                close: 100.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(1),
                open: 100.0,
                high: 105.0,
                low: 99.0,
                close: 105.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(2),
                open: 105.0,
                high: 106.0,
                low: 101.0,
                close: 102.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(3),
                open: 102.0,
                high: 108.0,
                low: 101.0,
                close: 107.0,
                volume: 1000.0,
            },
        ]
    }

    #[test]
    fn test_kagi_line_creation() {
        let bars = create_test_bars();
        let config = KagiConfig::new(2.0);
        let lines = to_kagi_lines(&bars, &config);

        assert!(!lines.is_empty());
    }

    #[test]
    fn test_kagi_line_to_bar() {
        let line = KagiLine {
            ts: Utc::now(),
            start_price: 100.0,
            end_price: 105.0,
            thickness: KagiThickness::Thick,
        };

        let bar = line.to_bar();
        assert_eq!(bar.open, 100.0);
        assert_eq!(bar.close, 105.0);
        assert_eq!(bar.high, 105.0);
        assert_eq!(bar.low, 100.0);
    }

    #[test]
    fn test_kagi_line_direction() {
        let up_line = KagiLine {
            ts: Utc::now(),
            start_price: 100.0,
            end_price: 105.0,
            thickness: KagiThickness::Thick,
        };
        assert!(up_line.is_up());

        let down_line = KagiLine {
            ts: Utc::now(),
            start_price: 105.0,
            end_price: 100.0,
            thickness: KagiThickness::Thin,
        };
        assert!(!down_line.is_up());
    }
}
