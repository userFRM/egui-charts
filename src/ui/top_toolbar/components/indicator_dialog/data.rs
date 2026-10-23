//! Data structures for indicator information and configuration

use crate::studies::IndicatorFactory;

/// Indicator information
#[derive(Debug, Clone)]
pub struct IndicatorInfo {
    /// The name the dialog's [`IndicatorFactory`] creates the study under
    pub id: String,
    pub name: String,
    pub desc: String,
    pub category: String,
    pub is_overlay: bool,
    pub is_premium: bool,
}

impl IndicatorInfo {
    /// Create a new indicator info entry with the given ID, name, and description
    pub fn new(id: impl Into<String>, name: impl Into<String>, desc: impl Into<String>) -> Self {
        Self {
            id: id.into(),
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

    /// Every study `factory` creates, sorted by display name, with its description, category
    /// and overlay flag
    pub(super) fn from_factory(factory: &IndicatorFactory) -> Vec<IndicatorInfo> {
        let mut infos: Vec<IndicatorInfo> = factory
            .list()
            .into_iter()
            .filter_map(|id| {
                let study = factory.create(&id)?;
                // A description reads "Full name - what it measures", as `Indicator::desc`
                // shows; one without a full name keeps the registered name.
                let (name, desc) = study
                    .desc()
                    .split_once(" - ")
                    .unwrap_or((id.as_str(), study.desc()));
                let mut info = IndicatorInfo::new(id.clone(), name, desc);
                info.category = factory.category(&id).unwrap_or_default().to_string();
                info.is_overlay = study.is_overlay();
                Some(info)
            })
            .collect();
        infos.sort_by_key(|info| info.name.to_lowercase());
        infos
    }
}

/// Indicator params for configuration
///
/// Selecting a study in the dialog sets these to the parameters the factory creates it with.
#[derive(Debug, Clone, PartialEq)]
pub struct IndicatorParams {
    // Moving avgs
    pub period: usize,
    // Keltner Channels
    pub keltner_atr: usize,
    pub keltner_multiplier: f64,
    // Bollinger Bands
    pub bb_std_dev: f64,
    // MACD
    pub macd_fast: usize,
    pub macd_slow: usize,
    pub macd_signal: usize,
    // Stochastic
    pub stoch_k: usize,
    pub stoch_smooth: usize,
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
            keltner_atr: 10,
            keltner_multiplier: 2.0,
            bb_std_dev: 2.0,
            macd_fast: 12,
            macd_slow: 26,
            macd_signal: 9,
            stoch_k: 14,
            stoch_smooth: 3,
            stoch_d: 3,
            supertrend_multiplier: 3.0,
            ichimoku_tenkan: 9,
            ichimoku_kijun: 26,
            ichimoku_senkou: 52,
        }
    }
}
