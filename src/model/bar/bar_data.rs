//! Bar Data Container
//!
//! A collection of bars with utility methods for aggregation and analysis.

use super::bar::Bar;

/// Maximum bars to keep in memory per chart
/// Prevents unbounded memory growth for streaming data
pub const MAX_BARS: usize = 10_000;

/// Maximum bars visible at once (prevent CPU overload)
pub const MAX_VISIBLE_BARS: usize = 2_000;

/// Container for multiple bars with utility methods
///
/// This container provides common operations over bar collections.
///
/// # Example
///
/// ```
/// use egui_charts::model::BarData;
///
/// let data = BarData::new();
/// assert!(data.is_empty());
/// ```
#[derive(Debug, Clone, Default)]
pub struct BarData {
    pub bars: Vec<Bar>,
}

impl BarData {
    /// Creates a new empty BarData
    pub fn new() -> Self {
        Self { bars: Vec::new() }
    }

    /// Creates BarData with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bars: Vec::with_capacity(capacity),
        }
    }

    /// Creates BarData from a vector of bars
    pub fn from_bars(bars: Vec<Bar>) -> Self {
        Self { bars }
    }

    /// Adds a bar to the dataset
    pub fn push(&mut self, bar: Bar) {
        self.bars.push(bar);
    }

    /// Returns the number of bars
    pub fn len(&self) -> usize {
        self.bars.len()
    }

    /// Returns true if there are no bars
    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    /// Clear all bars
    pub fn clear(&mut self) {
        self.bars.clear();
    }

    /// Trim old bars when exceeding MAX_BARS limit
    ///
    /// This prevents unbounded memory growth when streaming data.
    /// Removes oldest bars first.
    pub fn trim_to_limit(&mut self) {
        if self.bars.len() > MAX_BARS {
            let excess = self.bars.len() - MAX_BARS;
            self.bars.drain(0..excess);
            log::debug!("Trimmed {} old bars, {} remaining", excess, self.bars.len());
        }
    }

    /// Push a bar and automatically trim if limit exceeded
    pub fn push_with_limit(&mut self, bar: Bar) {
        self.bars.push(bar);
        self.trim_to_limit();
    }

    /// Get the first bar
    pub fn first(&self) -> Option<&Bar> {
        self.bars.first()
    }

    /// Get the last bar
    pub fn last(&self) -> Option<&Bar> {
        self.bars.last()
    }

    /// Get a bar by index
    pub fn get(&self, index: usize) -> Option<&Bar> {
        self.bars.get(index)
    }

    /// Get a mutable reference to a bar by index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Bar> {
        self.bars.get_mut(index)
    }

    /// Iterate over bars
    pub fn iter(&self) -> impl Iterator<Item = &Bar> {
        self.bars.iter()
    }

    // ============= Aggregation Methods =============

    /// Returns the min price (low) across all bars
    pub fn min_price(&self) -> Option<f64> {
        self.bars
            .iter()
            .map(|c| c.low)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Returns the max price (high) across all bars
    pub fn max_price(&self) -> Option<f64> {
        self.bars
            .iter()
            .map(|c| c.high)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Returns the price range (max_high - min_low)
    pub fn price_range(&self) -> Option<(f64, f64)> {
        let min = self.min_price()?;
        let max = self.max_price()?;
        Some((min, max))
    }

    /// Returns the max volume across all bars
    pub fn max_volume(&self) -> Option<f64> {
        self.bars
            .iter()
            .map(|c| c.volume)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Returns the total volume across all bars
    pub fn total_volume(&self) -> f64 {
        self.bars.iter().map(|c| c.volume).sum()
    }

    /// Returns the average volume across all bars
    pub fn avg_volume(&self) -> Option<f64> {
        if self.bars.is_empty() {
            return None;
        }
        Some(self.total_volume() / self.bars.len() as f64)
    }

    // ============= Transformation Methods =============

    /// Converts regular OHLC bars to Heikin-Ashi bars
    ///
    /// Heikin-Ashi is a Japanese candlestick technique that filters out market noise
    /// and highlights trends more clearly than traditional candlesticks.
    ///
    /// # Formula
    /// - HA Close = (Open + High + Low + Close) / 4
    /// - HA Open = (Previous HA Open + Previous HA Close) / 2
    /// - HA High = Max(High, HA Open, HA Close)
    /// - HA Low = Min(Low, HA Open, HA Close)
    pub fn to_heikin_ashi(&self) -> Self {
        if self.bars.is_empty() {
            return Self::new();
        }

        let mut ha_bars = Vec::with_capacity(self.bars.len());

        // First bar: use modified OHLC values
        let first = &self.bars[0];
        let mut prev_ha_open = (first.open + first.close) / 2.0;
        let mut prev_ha_close = (first.open + first.high + first.low + first.close) / 4.0;

        ha_bars.push(Bar::new(
            first.time,
            prev_ha_open,
            first.high,
            first.low,
            prev_ha_close,
            first.volume,
        ));

        // Process remaining bars
        for bar in self.bars.iter().skip(1) {
            let ha_close = (bar.open + bar.high + bar.low + bar.close) / 4.0;
            let ha_open = (prev_ha_open + prev_ha_close) / 2.0;
            let ha_high = bar.high.max(ha_open).max(ha_close);
            let ha_low = bar.low.min(ha_open).min(ha_close);

            ha_bars.push(Bar::new(
                bar.time, ha_open, ha_high, ha_low, ha_close, bar.volume,
            ));

            prev_ha_open = ha_open;
            prev_ha_close = ha_close;
        }

        Self::from_bars(ha_bars)
    }

    /// Returns a clone of the bar data (for regular candlesticks)
    ///
    /// This method exists for API consistency when switching between chart types.
    pub fn to_regular(&self) -> Self {
        self.clone()
    }

    /// Get a slice of bars within an index range
    pub fn slice(&self, start: usize, end: usize) -> &[Bar] {
        let end = end.min(self.bars.len());
        let start = start.min(end);
        &self.bars[start..end]
    }
}

impl IntoIterator for BarData {
    type Item = Bar;
    type IntoIter = std::vec::IntoIter<Bar>;

    fn into_iter(self) -> Self::IntoIter {
        self.bars.into_iter()
    }
}

impl<'a> IntoIterator for &'a BarData {
    type Item = &'a Bar;
    type IntoIter = std::slice::Iter<'a, Bar>;

    fn into_iter(self) -> Self::IntoIter {
        self.bars.iter()
    }
}

impl FromIterator<Bar> for BarData {
    fn from_iter<I: IntoIterator<Item = Bar>>(iter: I) -> Self {
        Self {
            bars: iter.into_iter().collect(),
        }
    }
}

impl std::ops::Index<usize> for BarData {
    type Output = Bar;

    fn index(&self, index: usize) -> &Self::Output {
        &self.bars[index]
    }
}

impl std::ops::IndexMut<usize> for BarData {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.bars[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_bars() -> BarData {
        let now = Utc::now();
        BarData::from_bars(vec![
            Bar::new(now, 100.0, 110.0, 95.0, 105.0, 1000.0),
            Bar::new(now, 105.0, 115.0, 100.0, 110.0, 1500.0),
            Bar::new(now, 110.0, 120.0, 105.0, 115.0, 1200.0),
        ])
    }

    #[test]
    fn test_aggregations() {
        let data = create_test_bars();

        assert_eq!(data.min_price(), Some(95.0));
        assert_eq!(data.max_price(), Some(120.0));
        assert_eq!(data.max_volume(), Some(1500.0));
        assert_eq!(data.total_volume(), 3700.0);
    }

    #[test]
    fn test_heikin_ashi() {
        let data = create_test_bars();
        let ha = data.to_heikin_ashi();

        assert_eq!(ha.len(), data.len());
        // HA bars should have smoothed values
        assert!(ha.bars[1].open != data.bars[1].open);
    }

    #[test]
    fn test_iteration() {
        let data = create_test_bars();
        let count = data.iter().count();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_indexing() {
        let data = create_test_bars();
        assert_eq!(data[0].open, 100.0);
        assert_eq!(data[2].close, 115.0);
    }
}
