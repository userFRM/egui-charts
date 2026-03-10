//! Data structures for indicator information and configuration

use super::types::IndicatorType;

/// Indicator information
#[derive(Debug, Clone)]
pub struct IndicatorInfo {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub category: String,
    pub is_overlay: bool,
    pub is_premium: bool,
    pub indicator_type: Option<IndicatorType>,
}

impl IndicatorInfo {
    /// Create a new indicator info entry with the given ID, name, and description
    pub fn new(id: impl Into<String>, name: impl Into<String>, desc: impl Into<String>) -> Self {
        let id_str: String = id.into();
        Self {
            indicator_type: IndicatorType::from_id(&id_str),
            id: id_str,
            name: name.into(),
            desc: desc.into(),
            category: "Technical Analysis".to_string(),
            is_overlay: false,
            is_premium: false,
        }
    }

    /// Mark this indicator as an overlay (renders on the price chart)
    pub fn overlay(mut self) -> Self {
        self.is_overlay = true;
        self
    }

    /// Mark this indicator as premium (requires subscription)
    pub fn premium(mut self) -> Self {
        self.is_premium = true;
        self
    }

    /// Get default built-in indicators
    pub fn builtin_indicators() -> Vec<IndicatorInfo> {
        IndicatorType::all()
            .into_iter()
            .map(|t| {
                let mut info = IndicatorInfo::new(t.id(), t.name(), t.desc());
                info.indicator_type = Some(t);
                if t.is_overlay() {
                    info = info.overlay();
                }
                info
            })
            .collect()
    }
}

/// Configured indicator with type and params
#[derive(Debug, Clone)]
pub struct ConfiguredIndicator {
    /// The indicator type
    pub indicator_type: IndicatorType,
    /// Custom params (if any)
    pub params: IndicatorParams,
}

/// Indicator params for configuration
#[derive(Debug, Clone, PartialEq)]
pub struct IndicatorParams {
    // Moving avgs
    pub period: usize,
    // Bollinger Bands
    pub bb_std_dev: f64,
    // MACD
    pub macd_fast: usize,
    pub macd_slow: usize,
    pub macd_signal: usize,
    // Stochastic
    pub stoch_k: usize,
    pub stoch_d: usize,
    // SuperTrend
    pub supertrend_multiplier: f64,
    // Ichimoku
    pub ichimoku_tenkan: usize,
    pub ichimoku_kijun: usize,
    pub ichimoku_senkou: usize,
}

impl Default for IndicatorParams {
    fn default() -> Self {
        Self {
            period: 14,
            bb_std_dev: 2.0,
            macd_fast: 12,
            macd_slow: 26,
            macd_signal: 9,
            stoch_k: 14,
            stoch_d: 3,
            supertrend_multiplier: 3.0,
            ichimoku_tenkan: 9,
            ichimoku_kijun: 26,
            ichimoku_senkou: 52,
        }
    }
}
