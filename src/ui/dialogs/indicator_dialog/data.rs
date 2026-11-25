//! Study browser entries and saved favorites.
//!
//! Use [`IndicatorPreferences`] to retain favorite factory names and
//! [`IndicatorInfo`] to describe studies in a picker.

use crate::studies::IndicatorFactory;

/// Host-owned indicator browser settings. Save these alongside the user's other settings
/// after [`IndicatorDialogAction::ToggleFavorite`](super::IndicatorDialogAction::ToggleFavorite),
/// and restore them with [`IndicatorDialog::set_preferences`](super::IndicatorDialog::set_preferences).
/// Unknown study names are retained so temporarily unavailable host studies keep their stars.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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

    /// Further names a built-in study is found by in the browser: conventional full and short
    /// names its description does not carry, other common spellings, the author's name before
    /// the study's, and the same names with the qualifier last, as study lists often order them.
    pub(super) fn search_names(&self) -> &'static [&'static str] {
        match self.id.as_str() {
            "%B" => &["Bollinger Bands %B"],
            "52W High/Low" => &["52 Week High/Low"],
            "A/D" => &["Accumulation Distribution", "ADL"],
            "ADX Breakout" => &["Rob Booker - ADX Breakout"],
            "ASI" => &["Accumulative Swing Index"],
            "Analyst price forecast" => &["Analyst price targets"],
            "Auto Fibonacci Extension" => &["Auto Fib Extension"],
            "Auto Fibonacci Retracement" => &["Auto Fib Retracement"],
            "Auto Trend Line" => &["Auto Trendlines"],
            "Average Daily Range" => &["ADR"],
            "BBW" => &["Bollinger BandWidth"],
            "Booker Reversal" => &["Rob Booker Reversal", "Rob Booker - Reversal"],
            "C2C Vol" => &["Volatility Close-to-Close"],
            "Connors RSI" => &["CRSI"],
            "DEMA" => &["Double EMA"],
            "Donchian" => &["DC"],
            "EMA Cross" | "MA Cross" => &["Double Moving Average", "Moving Average Double"],
            "Elliott Wave" => &["Elliott Waves"],
            "Force Index" => &["Elder's Force Index", "EFI"],
            "GK Vol" => &["Volatility O-H-L-C"],
            "Hamming MA" => &["Hamming Moving Average", "Moving Average Hamming"],
            "Higher Interval Candles" => &["Multi-Time Period Charts"],
            "Intraday Pivot Points" => &["Rob Booker Intraday Pivot Points"],
            "KAMA" => &[
                "Kaufman's Adaptive Moving Average",
                "Moving Average Adaptive",
            ],
            "Klinger" => &["Klinger Oscillator"],
            "Knoxville Divergence" => &["Rob Booker - Knoxville Divergence"],
            "LinReg" => &["Linear Regression Curve"],
            "Lunar Phases" => &["Moon Phases"],
            "MA Ribbon" => &["Moving Average Ribbon"],
            "MACD" => &["Moving Average Convergence Divergence"],
            "Missed Pivot Levels" => &["Rob Booker Missed Pivot Points"],
            "Multi MA" => &["Moving Average Multiple"],
            "OHLC4" => &["Average Price"],
            "PVT" => &["Volume Price Trend", "VPT"],
            "Price Momentum Oscillator" => &["PMO"],
            "RCI" => &["RankCorr"],
            "SMA/EMA Cross" => &[
                "MA with EMA Cross",
                "Double Moving Average",
                "Moving Average Double",
            ],
            "StochRSI" => &["Stoch RSI"],
            "TEMA" => &["Triple EMA"],
            "Time Price Opportunity" => &["TPO", "Session Time Price Opportunity"],
            "Triple MA" => &["Triple Moving Average", "Moving Average Triple"],
            "VWMA" => &["Volume-Weighted Moving Average"],
            "Vol Profile Anchored" => &["Anchored Volume Profile"],
            "Vol Profile Auto Anchored" => &["Auto Anchored Volume Profile"],
            "Vol Profile FR" => &["Fixed Range Volume Profile"],
            "Vol Profile Periodic" => &["Periodic Volume Profile"],
            "Vol Profile Session" => &["Session Volume Profile"],
            "Vol Profile Session HD" => &["Session Volume Profile HD"],
            "Vol Profile VR" => &[
                "Visible Range Volume Profile",
                "Volume Profile Visible Range",
            ],
            "Zero-Trend Vol" => &["Volatility Zero Trend Close-to-Close"],
            "Ziv Ghost Pivots" => &[
                "Rob Booker Ziv Ghost Pivots",
                "Rob Booker - Ziv Ghost Pivots",
            ],
            _ => &[],
        }
    }
}
