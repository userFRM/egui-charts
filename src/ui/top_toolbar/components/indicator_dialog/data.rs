//! Study browser entries, saved favorites and editable parameters.
//!
//! Use [`IndicatorPreferences`] to retain favorite factory names and
//! [`IndicatorInfo`] to describe studies in a picker.

use crate::studies::IndicatorFactory;

/// Host-owned indicator browser settings. Save these alongside the user's other settings
/// after [`IndicatorDialogAction::ToggleFavorite`](super::IndicatorDialogAction::ToggleFavorite),
/// and restore them with [`IndicatorDialog::set_preferences`](super::IndicatorDialog::set_preferences).
/// Unknown study names are retained so temporarily unavailable host studies keep their stars.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IndicatorPreferences {
    /// Favourite factory names, sorted for stable saved documents.
    #[serde(default)]
    pub favorites: std::collections::BTreeSet<String>,
}

/// Indicator information
#[derive(Debug, Clone)]
pub struct IndicatorInfo {
    /// The name the dialog's [`IndicatorFactory`] creates the study under
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description displayed to the user.
    pub desc: String,
    /// Category label used to group the entry in the browser.
    pub category: String,
    /// Whether the study draws on the price pane by default.
    pub is_overlay: bool,
    /// Display flag for a premium entry; access control belongs to the host.
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

    /// Mark this entry as premium for display. Access control belongs to the host.
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
    /// Shared lookback length in bars for the selected study.
    pub period: usize,
    // Keltner Channels
    /// ATR lookback length in bars for Keltner Channels.
    pub keltner_atr: usize,
    /// ATR multiplier for the Keltner Channel bands.
    pub keltner_multiplier: f64,
    // Bollinger Bands
    /// Standard deviation multiplier for Bollinger Bands.
    pub bb_std_dev: f64,
    // MACD
    /// Fast EMA length in bars.
    pub macd_fast: usize,
    /// Slow EMA length in bars.
    pub macd_slow: usize,
    /// MACD signal EMA length in bars.
    pub macd_signal: usize,
    // Stochastic
    /// Stochastic %K lookback length in bars.
    pub stoch_k: usize,
    /// Smoothing length in bars for stochastic %K.
    pub stoch_smooth: usize,
    /// Smoothing length in bars for stochastic %D.
    pub stoch_d: usize,
    // SuperTrend
    /// ATR multiplier for SuperTrend bands.
    pub supertrend_multiplier: f64,
    // Ichimoku
    /// Ichimoku conversion line lookback in bars.
    pub ichimoku_tenkan: usize,
    /// Ichimoku base line lookback in bars.
    pub ichimoku_kijun: usize,
    /// Ichimoku leading span B lookback in bars.
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
