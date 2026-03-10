//! Price source selection for chart series.
//!
//! Defines which price value to use for line-based chart calculations.

/// Price source for series calculations
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PriceSource {
    /// Opening price
    Open,
    /// Highest price
    High,
    /// Lowest price
    Low,
    /// Closing price (most common)
    #[default]
    Close,
    /// (High + Low) / 2
    HL2,
    /// (High + Low + Close) / 3
    HLC3,
    /// (Open + High + Low + Close) / 4
    OHLC4,
}

impl PriceSource {
    /// Display label for the price source
    pub fn label(&self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::High => "High",
            Self::Low => "Low",
            Self::Close => "Close",
            Self::HL2 => "HL/2",
            Self::HLC3 => "HLC/3",
            Self::OHLC4 => "OHLC/4",
        }
    }

    /// All available price sources
    pub fn all() -> [Self; 7] {
        [
            Self::Open,
            Self::High,
            Self::Low,
            Self::Close,
            Self::HL2,
            Self::HLC3,
            Self::OHLC4,
        ]
    }

    /// Compute the price value from OHLC data
    pub fn compute(&self, open: f64, high: f64, low: f64, close: f64) -> f64 {
        match self {
            Self::Open => open,
            Self::High => high,
            Self::Low => low,
            Self::Close => close,
            Self::HL2 => (high + low) / 2.0,
            Self::HLC3 => (high + low + close) / 3.0,
            Self::OHLC4 => (open + high + low + close) / 4.0,
        }
    }
}
