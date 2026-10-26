//! Point & Figure (P&F) chart transformation.
//!
//! P&F charts remove time from the horizontal axis. Instead, each column
//! represents a sequence of rising Xs or falling Os. A new column starts
//! only when price reverses by at least `reversal_boxes * box_size`.
//!
//! Use [`to_pnf_columns`] to transform OHLCV bars into P&F columns.

use super::PriceBuilder;
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
    /// Inclusive number of boxes, saturated at `usize::MAX` when the count is larger.
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

    /// Get all box prices in ascending price order.
    ///
    /// # Errors
    /// Returns an error for non-finite endpoints, an invalid box size, more than two million
    /// boxes, or an allocation failure. The column endpoints remain available without expansion.
    pub fn boxes(&self) -> Result<Vec<f64>, String> {
        if !(self.start_price.is_finite()
            && self.end_price.is_finite()
            && self.box_size > 0.0
            && self.box_size.is_finite())
        {
            return Err(
                "Box prices require finite endpoints and a positive finite box size".into(),
            );
        }
        let count = box_count(self.start_price, self.end_price, self.box_size);
        const MAX_BOXES: usize = 2_000_000;
        if count > MAX_BOXES {
            return Err("Column exceeds the two-million-box expansion limit".into());
        }
        let mut prices = Vec::new();
        prices
            .try_reserve_exact(count)
            .map_err(|error| error.to_string())?;
        let low = self.low();
        prices.extend((0..count).map(|i| low + i as f64 * self.box_size));
        Ok(prices)
    }

    /// Check if column contains a specific price level
    pub fn contains(&self, price: f64) -> bool {
        price >= self.low() && price <= self.high()
    }
}

/// Convert bar data to Point and Figure columns
///
/// Returns no columns when the box size is not a positive finite number or the reversal is zero
/// boxes.
///
/// # Safety Limits
/// - At most the newest 5,000 columns
pub fn to_pnf_columns(data: &[Bar], config: &PointFigureConfig) -> Vec<PnfColumn> {
    // The box size, from ATR when the config asks for it
    let box_size = if config.use_atr && data.len() >= config.atr_period {
        calculate_atr(data, config.atr_period)
    } else {
        config.box_size
    };
    if data.is_empty() || !(box_size > 0.0 && box_size.is_finite()) || config.reversal_boxes == 0 {
        return Vec::new();
    }

    let mut builder = PnfBuilder::new(box_size, config);
    for bar in data {
        builder.push(bar);
    }
    builder.elements()
}

pub(crate) const MAX_COLUMNS: usize = 5_000;

/// Builds Point and Figure columns bar by bar; `box_size` must be a positive finite number and
/// the reversal at least one box. The column in progress is kept apart from the closed columns
/// and added by `elements`.
#[derive(Clone)]
pub(crate) struct PnfBuilder {
    box_size: f64,
    reversal_amount: f64,
    use_close: bool,
    /// The column in progress; `None` until the first bar.
    curr_column: Option<PnfColumn>,
    columns: Vec<PnfColumn>,
}

impl PnfBuilder {
    pub(crate) fn new(box_size: f64, config: &PointFigureConfig) -> Self {
        Self {
            box_size,
            reversal_amount: box_size * config.reversal_boxes as f64,
            use_close: config.use_close,
            curr_column: None,
            columns: Vec::new(),
        }
    }
}

impl PriceBuilder for PnfBuilder {
    type Element = PnfColumn;

    fn push(&mut self, bar: &Bar) {
        let (box_size, reversal_amount) = (self.box_size, self.reversal_amount);
        let Some(mut curr_column) = self.curr_column.take() else {
            // The first bar starts an up column of one box
            let initial_price = if self.use_close { bar.close } else { bar.high };
            self.curr_column = Some(PnfColumn {
                start_price: snap_to_box(initial_price, box_size),
                end_price: snap_to_box(initial_price, box_size),
                direction: ColumnDirection::Up,
                start_time: bar.time,
                end_time: bar.time,
                box_size,
                box_cnt: 1,
            });
            return;
        };
        let (high_price, low_price) = if self.use_close {
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
                    curr_column.box_cnt =
                        box_count(curr_column.start_price, curr_column.end_price, box_size);
                }

                // Check for reversal
                let new_low = snap_to_box(low_price, box_size);
                if curr_column.end_price - new_low >= reversal_amount {
                    // Complete current column and start new one
                    let reversed = PnfColumn {
                        start_price: curr_column.end_price - box_size,
                        end_price: new_low,
                        direction: ColumnDirection::Down,
                        start_time: bar.time,
                        end_time: bar.time,
                        box_size,
                        box_cnt: box_count(curr_column.end_price - box_size, new_low, box_size),
                    };
                    self.columns
                        .push(std::mem::replace(&mut curr_column, reversed));
                }
            }
            ColumnDirection::Down => {
                // Check for continuation (new lows)
                let new_low = snap_to_box(low_price, box_size);
                if new_low < curr_column.end_price {
                    curr_column.end_price = new_low;
                    curr_column.end_time = bar.time;
                    curr_column.box_cnt =
                        box_count(curr_column.start_price, curr_column.end_price, box_size);
                }

                // Check for reversal
                let new_high = snap_to_box(high_price, box_size);
                if new_high - curr_column.end_price >= reversal_amount {
                    // Complete current column and start new one
                    let reversed = PnfColumn {
                        start_price: curr_column.end_price + box_size,
                        end_price: new_high,
                        direction: ColumnDirection::Up,
                        start_time: bar.time,
                        end_time: bar.time,
                        box_size,
                        box_cnt: box_count(curr_column.end_price + box_size, new_high, box_size),
                    };
                    self.columns
                        .push(std::mem::replace(&mut curr_column, reversed));
                }
            }
        }
        self.curr_column = Some(curr_column);

        // Bound memory by dropping the oldest columns; the builder never reads them.
        if self.columns.len() == 2 * MAX_COLUMNS {
            self.columns.drain(..MAX_COLUMNS);
        }
    }

    fn elements(&self) -> Vec<PnfColumn> {
        let mut columns = self.columns.clone();
        columns.extend(self.curr_column.clone());
        columns.drain(..columns.len().saturating_sub(MAX_COLUMNS));
        columns
    }
}

// Check the floating count before converting: the inclusive endpoint can exceed `usize` too.
fn box_count(start: f64, end: f64, size: f64) -> usize {
    let steps = ((end - start).abs() / size).round();
    if steps >= usize::MAX as f64 {
        usize::MAX
    } else {
        steps as usize + 1
    }
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
                colors: None,
                payload: None,
            },
            Bar {
                time: now,
                open: 101.0,
                high: 105.0,
                low: 100.0,
                close: 104.0,
                volume: 1000.0,
                colors: None,
                payload: None,
            },
            Bar {
                time: now,
                open: 104.0,
                high: 106.0,
                low: 103.0,
                close: 105.0,
                volume: 1000.0,
                colors: None,
                payload: None,
            },
            Bar {
                time: now,
                open: 105.0,
                high: 106.0,
                low: 98.0,
                close: 99.0,
                volume: 1000.0,
                colors: None,
                payload: None,
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
    fn column_counts_saturate_without_losing_price_endpoints() {
        // Inclusive counts: 100..101 has two boxes; 100..150 at 1e-8 has 5,000,000,001.
        for (prices, size, expected) in [
            (vec![100.0, 101.0], 1.0, 2),
            (
                vec![100.0, 150.0],
                1e-8,
                5_000_000_001_u64.min(usize::MAX as u64) as usize,
            ),
            (vec![100.0, 1e20], 1.0, usize::MAX),
            (vec![100.0, -1e20], 1.0, usize::MAX),
            (vec![100.0, 97.0, -1e20], 1.0, usize::MAX),
            (vec![100.0, 97.0, 1e20], 1.0, usize::MAX),
        ] {
            let data: Vec<_> = prices
                .iter()
                .enumerate()
                .map(|(i, &p)| {
                    Bar::new(
                        DateTime::from_timestamp(i as i64, 0).unwrap(),
                        p,
                        p,
                        p,
                        p,
                        0.0,
                    )
                })
                .collect();
            let columns = to_pnf_columns(&data, &PointFigureConfig::new(size, 3));
            let last = columns.last().unwrap();
            assert_eq!(last.box_cnt, expected, "{prices:?} size {size}");
            assert_eq!(last.end_price, *prices.last().unwrap());
        }
    }

    #[test]
    fn test_column_boxes() {
        for (start, end, size, expected) in [
            (
                100.0,
                105.0,
                1.0,
                Some(vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0]),
            ),
            (
                105.0,
                100.0,
                1.0,
                Some(vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0]),
            ),
            (100.0, 100.0, 1.0, Some(vec![100.0])),
            (100.0, 1e12, 1.0, None),
            (100.0, 1e20, 1.0, None),
            (100.0, 150.0, 1e-8, None),
            (100.0, 105.0, 0.0, None),
            (100.0, 105.0, f64::NAN, None),
            (100.0, f64::INFINITY, 1.0, None),
            (f64::NAN, 105.0, 1.0, None),
        ] {
            let column = PnfColumn {
                start_price: start,
                end_price: end,
                direction: ColumnDirection::Up,
                start_time: Utc::now(),
                end_time: Utc::now(),
                box_size: size,
                box_cnt: 1,
            };
            assert_eq!(column.boxes().ok(), expected, "{start}..{end} size {size}");
        }
    }
}
