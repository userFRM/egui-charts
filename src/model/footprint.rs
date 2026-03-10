//! Footprint (order-flow) chart data model.
//!
//! Footprint charts decompose each candlestick into a grid of price levels,
//! showing bid and ask volume at every tick. They reveal the battle between
//! buyers and sellers that is invisible on a standard OHLCV chart.
//!
//! Key concepts exposed here:
//!
//! - **[`FootprintCell`]** -- bid/ask volume at a single price level.
//! - **[`FootprintBar`]** -- one time period containing many cells, plus
//!   delta, POC (Point of Control), and value-area calculations.
//! - **[`FootprintConfig`]** -- rendering options (tick size, imbalance
//!   thresholds, display mode, color mode, etc.).
//! - **[`DiagonalImbalance`]** -- cross-level imbalance detection result.

use chrono::{DateTime, Utc};

/// Volume data at a single price level within a [`FootprintBar`].
///
/// Each cell records how much volume was traded at the bid (sell
/// aggressor) versus the ask (buy aggressor). The [`delta`](Self::delta)
/// and [`imbalance_ratio`](Self::imbalance_ratio) methods quantify
/// buying/selling pressure at this level.
#[derive(Debug, Clone)]
pub struct FootprintCell {
    /// Price level
    pub price: f64,
    /// Volume traded at bid (sell aggressor)
    pub bid_volume: f64,
    /// Volume traded at ask (buy aggressor)
    pub ask_volume: f64,
}

impl FootprintCell {
    /// Create an empty cell at the given price level.
    pub fn new(price: f64) -> Self {
        Self {
            price,
            bid_volume: 0.0,
            ask_volume: 0.0,
        }
    }

    /// Delta = ask_volume - bid_volume (positive = buying pressure)
    pub fn delta(&self) -> f64 {
        self.ask_volume - self.bid_volume
    }

    /// Total volume at this price level
    pub fn total_volume(&self) -> f64 {
        self.bid_volume + self.ask_volume
    }

    /// Imbalance ratio (0 to 1, 0.5 = balanced)
    pub fn imbalance_ratio(&self) -> f64 {
        let total = self.total_volume();
        if total > 0.0 {
            self.ask_volume / total
        } else {
            0.5
        }
    }

    /// Whether this cell shows a stacked imbalance (significant buyer/seller)
    pub fn has_imbalance(&self, threshold: f64) -> bool {
        let ratio = self.imbalance_ratio();
        ratio > threshold || ratio < (1.0 - threshold)
    }
}

/// A complete footprint bar -- one candlestick enriched with per-level volume.
///
/// Like a standard OHLCV bar but with an additional [`cells`](Self::cells)
/// vector showing bid/ask volume at every price tick. Also tracks the
/// Point of Control (POC), bar delta, and cumulative delta.
#[derive(Debug, Clone)]
pub struct FootprintBar {
    /// Bar ts
    pub ts: DateTime<Utc>,
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
    /// Volume at each price level (sorted by price descending)
    pub cells: Vec<FootprintCell>,
    /// Point of Control (price with highest volume)
    pub poc_price: Option<f64>,
    /// Total delta for the bar
    pub delta: f64,
    /// Cumulative delta (running total)
    pub cumulative_delta: f64,
}

impl FootprintBar {
    /// Create a new empty footprint bar starting at `ts`.
    pub fn new(ts: DateTime<Utc>, _tick_size: f64) -> Self {
        Self {
            ts,
            open: 0.0,
            high: f64::NEG_INFINITY,
            low: f64::INFINITY,
            close: 0.0,
            volume: 0.0,
            cells: Vec::new(),
            poc_price: None,
            delta: 0.0,
            cumulative_delta: 0.0,
        }
    }

    /// Add a trade to the footprint bar
    pub fn add_trade(&mut self, price: f64, size: f64, is_buyer_aggressor: bool, tick_size: f64) {
        // Round price to tick size
        let rounded_price = (price / tick_size).round() * tick_size;

        // Update OHLCV
        if self.volume == 0.0 {
            self.open = rounded_price;
        }
        self.high = self.high.max(rounded_price);
        self.low = self.low.min(rounded_price);
        self.close = rounded_price;
        self.volume += size;

        // Update delta
        if is_buyer_aggressor {
            self.delta += size;
        } else {
            self.delta -= size;
        }

        // Find or create cell for this price
        if let Some(cell) = self
            .cells
            .iter_mut()
            .find(|c| (c.price - rounded_price).abs() < tick_size * 0.5)
        {
            if is_buyer_aggressor {
                cell.ask_volume += size;
            } else {
                cell.bid_volume += size;
            }
        } else {
            let mut cell = FootprintCell::new(rounded_price);
            if is_buyer_aggressor {
                cell.ask_volume = size;
            } else {
                cell.bid_volume = size;
            }
            self.cells.push(cell);
        }

        // Sort cells by price descending
        self.cells.sort_by(|a, b| {
            b.price
                .partial_cmp(&a.price)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Update POC
        self.update_poc();
    }

    /// Find the Point of Control (price with highest volume)
    fn update_poc(&mut self) {
        self.poc_price = self
            .cells
            .iter()
            .max_by(|a, b| {
                a.total_volume()
                    .partial_cmp(&b.total_volume())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|c| c.price);
    }

    /// Get the value area -- the price range containing `percentage` of total volume.
    ///
    /// Returns `Some((low, high))` or `None` if no cells exist. Pass `0.70`
    /// for the standard 70% value area.
    pub fn val_area(&self, percentage: f64) -> Option<(f64, f64)> {
        if self.cells.is_empty() {
            return None;
        }

        let total_volume: f64 = self.cells.iter().map(|c| c.total_volume()).sum();
        let target_volume = total_volume * percentage;

        // Sort by volume descending and accumulate until we hit target
        let mut cells_by_volume: Vec<_> = self.cells.clone();
        cells_by_volume.sort_by(|a, b| {
            b.total_volume()
                .partial_cmp(&a.total_volume())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut accumulated = 0.0;
        let mut prices = Vec::new();

        for cell in cells_by_volume {
            accumulated += cell.total_volume();
            prices.push(cell.price);
            if accumulated >= target_volume {
                break;
            }
        }

        if prices.is_empty() {
            return None;
        }

        let low = prices.iter().cloned().fold(f64::INFINITY, f64::min);
        let high = prices.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        Some((low, high))
    }

    /// Get all bid imbalances (cells where bid volume >> ask volume)
    pub fn bid_imbalances(&self, threshold: f64) -> Vec<&FootprintCell> {
        self.cells
            .iter()
            .filter(|c| c.imbalance_ratio() < (1.0 - threshold))
            .collect()
    }

    /// Get all ask imbalances (cells where ask volume >> bid volume)
    pub fn ask_imbalances(&self, threshold: f64) -> Vec<&FootprintCell> {
        self.cells
            .iter()
            .filter(|c| c.imbalance_ratio() > threshold)
            .collect()
    }

    /// Detect stacked imbalances (consecutive price levels with the same
    /// side dominating beyond `threshold`).
    ///
    /// Returns `(start_price, end_price, is_ask_side)` tuples for each
    /// stack of at least `min_cnt` consecutive imbalanced levels.
    pub fn stacked_imbalances(&self, threshold: f64, min_cnt: usize) -> Vec<(f64, f64, bool)> {
        let mut result = Vec::new();
        let mut curr_stack: Vec<&FootprintCell> = Vec::new();
        let mut curr_is_ask = None;

        for cell in &self.cells {
            let ratio = cell.imbalance_ratio();

            if ratio > threshold {
                // Ask imbalance
                if curr_is_ask == Some(true) {
                    curr_stack.push(cell);
                } else {
                    if curr_stack.len() >= min_cnt
                        && let (Some(first), Some(last)) = (curr_stack.first(), curr_stack.last())
                    {
                        result.push((first.price, last.price, curr_is_ask.unwrap_or(false)));
                    }
                    curr_stack = vec![cell];
                    curr_is_ask = Some(true);
                }
            } else if ratio < (1.0 - threshold) {
                // Bid imbalance
                if curr_is_ask == Some(false) {
                    curr_stack.push(cell);
                } else {
                    if curr_stack.len() >= min_cnt
                        && let (Some(first), Some(last)) = (curr_stack.first(), curr_stack.last())
                    {
                        result.push((first.price, last.price, curr_is_ask.unwrap_or(false)));
                    }
                    curr_stack = vec![cell];
                    curr_is_ask = Some(false);
                }
            } else {
                if curr_stack.len() >= min_cnt
                    && let (Some(first), Some(last)) = (curr_stack.first(), curr_stack.last())
                {
                    result.push((first.price, last.price, curr_is_ask.unwrap_or(false)));
                }
                curr_stack.clear();
                curr_is_ask = None;
            }
        }

        // Check final stack
        if curr_stack.len() >= min_cnt
            && let (Some(first), Some(last)) = (curr_stack.first(), curr_stack.last())
        {
            result.push((first.price, last.price, curr_is_ask.unwrap_or(false)));
        }

        result
    }

    /// Detect diagonal imbalances (comparing bid\[price\] vs ask\[price + tick\])
    ///
    /// Diagonal imbalances occur when the bid volume at one price level is
    /// significantly higher/lower than the ask volume at the adjacent price level.
    /// This indicates aggressive buying/selling pressure across price levels.
    ///
    /// # Arguments
    /// * `threshold` - Ratio threshold (e.g., 3.0 for 300%)
    /// * `tick_size` - Price tick size for determining adjacent levels
    ///
    /// # Returns
    /// Vec of DiagonalImbalance indicating where imbalances were detected
    pub fn diagonal_imbalances(&self, threshold: f64, tick_size: f64) -> Vec<DiagonalImbalance> {
        let mut result = Vec::new();

        // Cells are sorted by price descending, so we iterate from high to low
        for i in 0..self.cells.len().saturating_sub(1) {
            let upper_cell = &self.cells[i];
            let lower_cell = &self.cells[i + 1];

            // Check if cells are adjacent (within one tick)
            let price_diff = (upper_cell.price - lower_cell.price).abs();
            if (price_diff - tick_size).abs() > tick_size * 0.1 {
                continue; // Not adjacent levels
            }

            // Check for buying imbalance: ask at upper level >> bid at lower level
            // This shows aggressive buyers lifting offers
            if lower_cell.bid_volume > 0.0 {
                let ratio = upper_cell.ask_volume / lower_cell.bid_volume;
                if ratio >= threshold {
                    result.push(DiagonalImbalance {
                        price: upper_cell.price,
                        volume: upper_cell.ask_volume,
                        is_buying: true,
                        ratio,
                    });
                }
            }

            // Check for selling imbalance: bid at lower level >> ask at upper level
            // This shows aggressive sellers hitting bids
            if upper_cell.ask_volume > 0.0 {
                let ratio = lower_cell.bid_volume / upper_cell.ask_volume;
                if ratio >= threshold {
                    result.push(DiagonalImbalance {
                        price: lower_cell.price,
                        volume: lower_cell.bid_volume,
                        is_buying: false,
                        ratio,
                    });
                }
            }
        }

        result
    }

    /// Get cells within the value area
    pub fn cells_in_value_area(&self) -> Vec<&FootprintCell> {
        if let Some((va_low, va_high)) = self.val_area(0.70) {
            self.cells
                .iter()
                .filter(|c| c.price >= va_low && c.price <= va_high)
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Check if a price level is the POC
    pub fn is_poc(&self, price: f64, tick_size: f64) -> bool {
        self.poc_price
            .map(|poc| (poc - price).abs() < tick_size * 0.5)
            .unwrap_or(false)
    }
}

/// Configuration for footprint chart rendering.
///
/// Controls tick aggregation, display mode, color mode, imbalance
/// thresholds, and which overlays (POC, value area, delta histogram)
/// are shown.
#[derive(Debug, Clone)]
pub struct FootprintConfig {
    /// Tick size for price aggregation
    pub tick_size: f64,
    /// Show delta values
    pub show_delta: bool,
    /// Show POC line
    pub show_poc: bool,
    /// Show value area
    pub show_value_area: bool,
    /// Value area percentage (default 70%)
    pub val_area_pct: f64,
    /// Imbalance threshold (default 300% = 3:1 ratio)
    pub imbalance_threshold: f64,
    /// Highlight stacked imbalances
    pub show_stacked_imbalances: bool,
    /// Min bars for stacked imbalance
    pub stacked_min_cnt: usize,
    /// Color mode
    pub color_mode: FootprintColorMode,
    /// Display mode (bid/ask split, volume only, delta only, etc.)
    pub display_mode: FootprintDisplayMode,
    /// How to group ticks into price levels
    pub tick_grouping: TickGrouping,
    /// Minimum cell height in pixels (responsive)
    pub min_cell_height: f32,
    /// Diagonal imbalance threshold (default 3.0 = 300%)
    pub diagonal_imbalance_threshold: f64,
    /// Show cumulative delta line overlay
    pub show_cumulative_delta_line: bool,
    /// Show delta histogram sub-pane
    pub show_delta_histogram: bool,
}

/// Color-mapping strategy for footprint cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FootprintColorMode {
    /// Color by delta (green = buyer, red = seller)
    Delta,
    /// Color by volume intensity
    Volume,
    /// Color by imbalance
    Imbalance,
}

/// What numbers to show inside each footprint cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FootprintDisplayMode {
    /// Left: bid volume, Right: ask volume
    #[default]
    BidAskSplit,
    /// Total volume per level only
    VolumeOnly,
    /// Delta (ask - bid) per level only
    DeltaOnly,
    /// Delta plus total volume
    DeltaPlusVolume,
}

/// Strategy for grouping ticks into price levels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TickGrouping {
    /// Automatic based on chart zoom level
    #[default]
    Automatic,
    /// Manual ticks per level
    Manual { ticks_per_level: usize },
    /// Fixed price interval (e.g., $0.50)
    FixedInterval { interval: f64 },
    /// Fixed number of levels per bar
    BarBased { levels_per_bar: usize },
}

/// Result of diagonal imbalance detection across adjacent price levels.
///
/// A diagonal imbalance compares bid volume at one level against ask
/// volume at the adjacent level. Large ratios signal aggressive
/// directional activity.
#[derive(Debug, Clone)]
pub struct DiagonalImbalance {
    /// Price level where imbalance was detected
    pub price: f64,
    /// The volume that caused the imbalance
    pub volume: f64,
    /// True if buying imbalance (ask dominance), false if selling (bid dominance)
    pub is_buying: bool,
    /// The actual ratio (e.g., 3.5 for 350%)
    pub ratio: f64,
}

impl Default for FootprintConfig {
    fn default() -> Self {
        Self {
            tick_size: 1.0,
            show_delta: true,
            show_poc: true,
            show_value_area: true,
            val_area_pct: 0.70,
            imbalance_threshold: 0.75, // 3:1 ratio
            show_stacked_imbalances: true,
            stacked_min_cnt: 3,
            color_mode: FootprintColorMode::Delta,
            display_mode: FootprintDisplayMode::default(),
            tick_grouping: TickGrouping::default(),
            min_cell_height: 14.0, // Desktop default, responsive sizing will override
            diagonal_imbalance_threshold: 3.0, // 300%
            show_cumulative_delta_line: false,
            show_delta_histogram: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_bar() -> FootprintBar {
        let mut bar = FootprintBar::new(Utc::now(), 1.0);

        // Add some trades
        bar.add_trade(100.0, 10.0, true, 1.0); // Buy at 100
        bar.add_trade(100.0, 5.0, false, 1.0); // Sell at 100
        bar.add_trade(101.0, 15.0, true, 1.0); // Buy at 101
        bar.add_trade(99.0, 8.0, false, 1.0); // Sell at 99

        bar
    }

    #[test]
    fn test_footprint_cell() {
        let mut cell = FootprintCell::new(100.0);
        cell.bid_volume = 5.0;
        cell.ask_volume = 15.0;

        assert_eq!(cell.delta(), 10.0);
        assert_eq!(cell.total_volume(), 20.0);
        assert!((cell.imbalance_ratio() - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_footprint_bar_trades() {
        let bar = create_test_bar();

        assert_eq!(bar.open, 100.0);
        assert_eq!(bar.high, 101.0);
        assert_eq!(bar.low, 99.0);
        assert_eq!(bar.close, 99.0);
        assert_eq!(bar.volume, 38.0);
    }

    #[test]
    fn test_footprint_bar_delta() {
        let bar = create_test_bar();

        // Delta = (10 + 15) - (5 + 8) = 25 - 13 = 12
        assert_eq!(bar.delta, 12.0);
    }

    #[test]
    fn test_footprint_poc() {
        let bar = create_test_bar();

        // 100: 15 total, 101: 15 total, 99: 8 total
        // POC should be 100 or 101 (both have 15)
        assert!(bar.poc_price.is_some());
    }

    #[test]
    fn test_footprint_value_area() {
        let bar = create_test_bar();

        let va = bar.val_area(0.70);
        assert!(va.is_some());

        let (low, high) = va.unwrap();
        assert!(low <= high);
    }

    #[test]
    fn test_footprint_imbalances() {
        let mut bar = FootprintBar::new(Utc::now(), 1.0);

        // Create an ask imbalance at 100 (10:1 ratio)
        bar.add_trade(100.0, 10.0, true, 1.0);
        bar.add_trade(100.0, 1.0, false, 1.0);

        let ask_imbalances = bar.ask_imbalances(0.75);
        assert!(!ask_imbalances.is_empty());
    }
}
