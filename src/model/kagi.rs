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

use crate::model::{Bar, PriceBuilder};
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
/// Returns no lines when the reversal amount is not a positive finite number.
///
/// # Safety Limits
/// - At most the newest 10,000 lines
pub fn to_kagi_lines(bars: &[Bar], config: &KagiConfig) -> Vec<KagiLine> {
    let reversal = config.reversal_amount;
    if bars.is_empty() || !(reversal > 0.0 && reversal.is_finite()) {
        return Vec::new();
    }

    let mut builder = KagiBuilder::new(reversal);
    for bar in bars {
        builder.push(bar);
    }
    builder.elements()
}

const MAX_LINES: usize = 10_000;

/// The line in progress and the trend state.
#[derive(Clone)]
struct KagiState {
    curr_price: f64,
    line_start_price: f64,
    line_direction_up: bool,
    last_ts: DateTime<Utc>,
    /// Significant highs and lows decide when the thickness changes.
    significant_high: f64,
    significant_low: f64,
    is_thick: bool,
}

impl KagiState {
    fn line(&self) -> KagiLine {
        KagiLine {
            ts: self.last_ts,
            start_price: self.line_start_price,
            end_price: self.curr_price,
            thickness: if self.is_thick {
                KagiThickness::Thick
            } else {
                KagiThickness::Thin
            },
        }
    }
}

/// Builds Kagi lines bar by bar; `reversal` must be a positive finite number. The line in
/// progress is kept apart from the closed lines and added by `elements`.
#[derive(Clone)]
pub(crate) struct KagiBuilder {
    reversal: f64,
    state: Option<KagiState>,
    lines: Vec<KagiLine>,
}

impl KagiBuilder {
    pub(crate) fn new(reversal: f64) -> Self {
        Self {
            reversal,
            state: None,
            lines: Vec::new(),
        }
    }
}

impl PriceBuilder for KagiBuilder {
    type Element = KagiLine;

    fn push(&mut self, bar: &Bar) {
        let reversal = self.reversal;
        let Some(st) = self.state.as_mut() else {
            self.state = Some(KagiState {
                curr_price: bar.close,
                line_start_price: bar.close,
                line_direction_up: true,
                last_ts: bar.time,
                significant_high: bar.close,
                significant_low: bar.close,
                is_thick: false,
            });
            return;
        };
        st.last_ts = bar.time;
        let price = bar.close;

        // Check for continuation or reversal
        if st.line_direction_up {
            // Currently moving up
            if price > st.curr_price {
                // Continue up
                st.curr_price = price;
                if price > st.significant_high {
                    st.significant_high = price;
                }
            } else if price < (st.curr_price - reversal) {
                // Reversal down: save the current line and start a new one going down
                self.lines.push(st.line());
                st.line_start_price = st.curr_price;
                st.curr_price = price;
                st.line_direction_up = false;

                // Check thickness change
                if price < st.significant_low {
                    st.is_thick = false; // Switch to thin (downtrend)
                    st.significant_low = price;
                }
            }
        } else {
            // Currently moving down
            if price < st.curr_price {
                // Continue down
                st.curr_price = price;
                if price < st.significant_low {
                    st.significant_low = price;
                }
            } else if price > (st.curr_price + reversal) {
                // Reversal up: save the current line and start a new one going up
                self.lines.push(st.line());
                st.line_start_price = st.curr_price;
                st.curr_price = price;
                st.line_direction_up = true;

                // Check thickness change
                if price > st.significant_high {
                    st.is_thick = true; // Switch to thick (uptrend)
                    st.significant_high = price;
                }
            }
        }

        // Bound memory by dropping the oldest lines; the state never reads them.
        if self.lines.len() == 2 * MAX_LINES {
            self.lines.drain(..MAX_LINES);
        }
    }

    fn elements(&self) -> Vec<KagiLine> {
        let mut lines = self.lines.clone();
        // The line in progress, unless it has not moved
        if let Some(st) = self
            .state
            .as_ref()
            .filter(|st| st.line_start_price != st.curr_price)
        {
            lines.push(st.line());
        }
        lines.drain(..lines.len().saturating_sub(MAX_LINES));
        lines
    }
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
