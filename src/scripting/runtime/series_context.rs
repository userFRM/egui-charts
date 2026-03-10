//! Series context for Pine Script runtime
//!
//! Manages bar data and series caching for builtin functions.

use crate::model::Bar;
use std::collections::HashMap;

/// Context passed to builtin functions for series calculations
#[derive(Clone)]
pub struct SeriesContext {
    /// All bar data
    pub bars: Vec<Bar>,
    /// Current bar index
    pub curr_bar: usize,
    /// Cached series data (e.g., "close" -> all close prices)
    pub series_cache: HashMap<String, Vec<f64>>,
}

impl SeriesContext {
    pub fn new(bars: &[Bar]) -> Self {
        let mut series_cache = HashMap::new();

        // Pre-compute built-in series
        series_cache.insert("open".to_string(), bars.iter().map(|b| b.open).collect());
        series_cache.insert("high".to_string(), bars.iter().map(|b| b.high).collect());
        series_cache.insert("low".to_string(), bars.iter().map(|b| b.low).collect());
        series_cache.insert("close".to_string(), bars.iter().map(|b| b.close).collect());
        series_cache.insert(
            "volume".to_string(),
            bars.iter().map(|b| b.volume).collect(),
        );

        // HL2, HLC3, OHLC4 for Pine Script compatibility
        series_cache.insert(
            "hl2".to_string(),
            bars.iter().map(|b| (b.high + b.low) / 2.0).collect(),
        );
        series_cache.insert(
            "hlc3".to_string(),
            bars.iter()
                .map(|b| (b.high + b.low + b.close) / 3.0)
                .collect(),
        );
        series_cache.insert(
            "ohlc4".to_string(),
            bars.iter()
                .map(|b| (b.open + b.high + b.low + b.close) / 4.0)
                .collect(),
        );

        Self {
            bars: bars.to_vec(),
            curr_bar: 0,
            series_cache,
        }
    }

    /// Get value from series at current bar with optional offset
    pub fn get_series_val(&self, series_name: &str, offset: usize) -> Option<f64> {
        let series = self.series_cache.get(series_name)?;
        if offset <= self.curr_bar {
            series.get(self.curr_bar - offset).copied()
        } else {
            None
        }
    }

    /// Get entire series
    pub fn get_series(&self, name: &str) -> Option<&Vec<f64>> {
        self.series_cache.get(name)
    }

    /// Store computed series
    pub fn store_series(&mut self, name: String, values: Vec<f64>) {
        self.series_cache.insert(name, values);
    }
}
