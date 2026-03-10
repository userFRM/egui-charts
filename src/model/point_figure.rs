//! Point & Figure (P&F) chart transformation.
//!
//! P&F charts remove time from the horizontal axis. Instead, each column
//! represents a sequence of rising Xs or falling Os. A new column starts
//! only when price reverses by at least `reversal_boxes * box_size`.
//!
//! Use [`to_pnf_columns`] to transform OHLCV bars into P&F columns.

use super::bar::Bar;
use chrono::{DateTime, Utc};

/// Configuration for Point & Figure chart transformation.
///
/// The two key parameters are `box_size` (price units per box) and
/// `reversal_boxes` (how many boxes of counter-movement trigger a new
/// column). The traditional default is a 1-box size with 3-box reversal.
#[derive(Debug, Clone)]
pub struct PointFigureConfig {
    /// Box size in price units
    pub box_size: f64,
    /// Number of boxes required for reversal
    pub reversal_boxes: usize,
    /// Whether to use ATR for box size
    pub use_atr: bool,
    /// ATR period if using ATR-based boxes
    pub atr_period: usize,
    /// Traditional (high/low) or close-only method
    pub use_close: bool,
}

impl Default for PointFigureConfig {
    fn default() -> Self {
        Self {
            box_size: 1.0,
            reversal_boxes: 3,
            use_atr: false,
            atr_period: 14,
            use_close: false,
        }
    }
}

impl PointFigureConfig {
    /// Create a P&F config with explicit box size and reversal count.
    pub fn new(box_size: f64, reversal_boxes: usize) -> Self {
        Self {
            box_size,
            reversal_boxes,
            ..Default::default()
        }
    }

    /// Use ATR-derived box size instead of a fixed value.
    pub fn with_atr(mut self, period: usize) -> Self {
        self.use_atr = true;
        self.atr_period = period;
        self
    }

    /// Use close-only method instead of high/low.
    pub fn with_close_only(mut self) -> Self {
        self.use_close = true;
        self
    }
}

/// Direction of a P&F column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnDirection {
    /// Rising column (Xs)
    Up,
    /// Falling column (Os)
    Down,
}

/// A single column of Xs (up) or Os (down) in a P&F chart.
#[derive(Debug, Clone)]
pub struct PnfColumn {
    /// Starting price (bottom for Up, top for Down)
    pub start_price: f64,
    /// Ending price (top for Up, bottom for Down)
    pub end_price: f64,
    /// Direction of column
    pub direction: ColumnDirection,
    /// Ts when column started
    pub start_time: DateTime<Utc>,
    /// Ts when column ended (or current time if active)
    pub end_time: DateTime<Utc>,
    /// Box size used for this column
    pub box_size: f64,
    /// Number of boxes in this column
    pub box_cnt: usize,
}

impl PnfColumn {
    /// Get the low price of the column
    pub fn low(&self) -> f64 {
        self.start_price.min(self.end_price)
    }

    /// Get the high price of the column
    pub fn high(&self) -> f64 {
        self.start_price.max(self.end_price)
    }

    /// Get all box prices in this column
    pub fn boxes(&self) -> Vec<f64> {
        let mut prices = Vec::new();
        let low = self.low();
        let count = ((self.high() - low) / self.box_size).round() as usize;

        for i in 0..=count {
            prices.push(low + (i as f64 * self.box_size));
        }

        prices
    }

    /// Check if column contains a specific price level
    pub fn contains(&self, price: f64) -> bool {
        price >= self.low() && price <= self.high()
    }
}

/// Convert bar data to Point and Figure columns
///
/// # Safety Limits
/// - Maximum 5,000 columns to prevent memory issues
/// - Auto-adjusts box size if too small relative to price range
pub fn to_pnf_columns(data: &[Bar], config: &PointFigureConfig) -> Vec<PnfColumn> {
    if data.is_empty() {
        return Vec::new();
    }

    // Calculate price range to validate box size
    let (min_price, max_price) = data.iter().fold((f64::MAX, f64::MIN), |(min, max), bar| {
        (min.min(bar.low), max.max(bar.high))
    });
    let price_range = max_price - min_price;

    // Calculate box size (potentially using ATR)
    let base_box_size = if config.use_atr && data.len() >= config.atr_period {
        calculate_atr(data, config.atr_period)
    } else {
        config.box_size
    };

    // Auto-adjust box size if it would create too many boxes
    let min_box_size = price_range / 500.0;
    let box_size = base_box_size.max(min_box_size).max(0.0001);

    let reversal_amount = box_size * config.reversal_boxes as f64;
    let mut columns: Vec<PnfColumn> = Vec::new();
    const MAX_COLUMNS: usize = 5_000;

    // Initialize with first bar
    let first = &data[0];
    let initial_price = if config.use_close {
        first.close
    } else {
        first.high
    };

    let mut curr_column = PnfColumn {
        start_price: snap_to_box(initial_price, box_size),
        end_price: snap_to_box(initial_price, box_size),
        direction: ColumnDirection::Up,
        start_time: first.time,
        end_time: first.time,
        box_size,
        box_cnt: 1,
    };

    for bar in data.iter().skip(1) {
        let (high_price, low_price) = if config.use_close {
            (bar.close, bar.close)
        } else {
            (bar.high, bar.low)
        };

        match curr_column.direction {
            ColumnDirection::Up => {
                // Check for continuation (new highs)
                let new_high = snap_to_box(high_price, box_size);
                if new_high > curr_column.end_price {
                    curr_column.end_price = new_high;
                    curr_column.end_time = bar.time;
                    curr_column.box_cnt = ((curr_column.end_price - curr_column.start_price)
                        / box_size)
                        .round() as usize
                        + 1;
                }

                // Check for reversal
                let new_low = snap_to_box(low_price, box_size);
                if curr_column.end_price - new_low >= reversal_amount {
                    // Complete current column and start new one
                    columns.push(curr_column.clone());

                    // Safety check
                    if columns.len() >= MAX_COLUMNS {
                        return columns;
                    }

                    curr_column = PnfColumn {
                        start_price: curr_column.end_price - box_size,
                        end_price: new_low,
                        direction: ColumnDirection::Down,
                        start_time: bar.time,
                        end_time: bar.time,
                        box_size,
                        box_cnt: ((curr_column.end_price - box_size - new_low) / box_size).round()
                            as usize
                            + 1,
                    };
                }
            }
            ColumnDirection::Down => {
                // Check for continuation (new lows)
                let new_low = snap_to_box(low_price, box_size);
                if new_low < curr_column.end_price {
                    curr_column.end_price = new_low;
                    curr_column.end_time = bar.time;
                    curr_column.box_cnt = ((curr_column.start_price - curr_column.end_price)
                        / box_size)
                        .round() as usize
                        + 1;
                }

                // Check for reversal
                let new_high = snap_to_box(high_price, box_size);
                if new_high - curr_column.end_price >= reversal_amount {
                    // Complete current column and start new one
                    columns.push(curr_column.clone());

                    // Safety check
                    if columns.len() >= MAX_COLUMNS {
                        return columns;
                    }

                    curr_column = PnfColumn {
                        start_price: curr_column.end_price + box_size,
                        end_price: new_high,
                        direction: ColumnDirection::Up,
                        start_time: bar.time,
                        end_time: bar.time,
                        box_size,
                        box_cnt: ((new_high - curr_column.end_price - box_size) / box_size).round()
                            as usize
                            + 1,
                    };
                }
            }
        }
    }

    // Add the final column
    columns.push(curr_column);

    columns
}

/// Snap price to nearest box boundary
fn snap_to_box(price: f64, box_size: f64) -> f64 {
    (price / box_size).floor() * box_size
}

/// Calculate ATR for box size
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

    // Simple moving avg of TR
    let sum: f64 = tr_values.iter().rev().take(period).sum();
    sum / period as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_bars() -> Vec<Bar> {
        let now = Utc::now();
        vec![
            Bar {
                time: now,
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 101.0,
                volume: 1000.0,
            },
            Bar {
                time: now,
                open: 101.0,
                high: 105.0,
                low: 100.0,
                close: 104.0,
                volume: 1000.0,
            },
            Bar {
                time: now,
                open: 104.0,
                high: 106.0,
                low: 103.0,
                close: 105.0,
                volume: 1000.0,
            },
            Bar {
                time: now,
                open: 105.0,
                high: 106.0,
                low: 98.0,
                close: 99.0,
                volume: 1000.0,
            },
        ]
    }

    #[test]
    fn test_config_creation() {
        let config = PointFigureConfig::new(1.0, 3);
        assert!((config.box_size - 1.0).abs() < 0.01);
        assert_eq!(config.reversal_boxes, 3);
    }

    #[test]
    fn test_snap_to_box() {
        assert!((snap_to_box(10.5, 1.0) - 10.0).abs() < 0.01);
        assert!((snap_to_box(10.9, 1.0) - 10.0).abs() < 0.01);
        assert!((snap_to_box(11.0, 1.0) - 11.0).abs() < 0.01);
    }

    #[test]
    fn test_pnf_conversion() {
        let bars = create_test_bars();
        let config = PointFigureConfig::new(1.0, 3);
        let columns = to_pnf_columns(&bars, &config);

        assert!(!columns.is_empty());
    }

    #[test]
    fn test_column_boxes() {
        let column = PnfColumn {
            start_price: 100.0,
            end_price: 105.0,
            direction: ColumnDirection::Up,
            start_time: Utc::now(),
            end_time: Utc::now(),
            box_size: 1.0,
            box_cnt: 6,
        };

        let boxes = column.boxes();
        assert_eq!(boxes.len(), 6); // 100, 101, 102, 103, 104, 105
    }
}
