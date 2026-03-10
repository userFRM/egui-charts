//! Type definitions for the indicator dialog.
//!
//! Defines indicator tabs, categories, and the built-in indicator type catalog
//! with metadata (name, description, ID, overlay status).

/// Indicator tabs in the dialog sidebar
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorTab {
    /// Technical indicators (SMA, RSI, MACD, etc.)
    Indicators,
    /// Trading strategies
    Strategies,
    /// Indicator profiles (saved combinations)
    Profiles,
    /// Chart patterns (head-and-shoulders, triangles, etc.)
    Patterns,
}

impl IndicatorTab {
    /// Return all available tabs
    pub fn all() -> &'static [IndicatorTab] {
        &[
            IndicatorTab::Indicators,
            IndicatorTab::Strategies,
            IndicatorTab::Profiles,
            IndicatorTab::Patterns,
        ]
    }

    /// Get the display name for this tab
    pub fn name(&self) -> &'static str {
        match self {
            IndicatorTab::Indicators => "Indicators",
            IndicatorTab::Strategies => "Strategies",
            IndicatorTab::Profiles => "Profiles",
            IndicatorTab::Patterns => "Patterns",
        }
    }
}

/// Indicator categories for filtering the indicator list
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorCategory {
    /// User-favorited indicators
    Favorites,
    /// User-created custom scripts
    MyScripts,
    /// Built-in platform indicators
    BuiltIns,
    /// Community-contributed indicators
    Community,
    /// Premium/paid indicators
    Premium,
}

impl IndicatorCategory {
    /// Return all available categories
    pub fn all() -> &'static [IndicatorCategory] {
        &[
            IndicatorCategory::Favorites,
            IndicatorCategory::MyScripts,
            IndicatorCategory::BuiltIns,
            IndicatorCategory::Community,
            IndicatorCategory::Premium,
        ]
    }

    /// Get the display name for this category
    pub fn name(&self) -> &'static str {
        match self {
            IndicatorCategory::Favorites => "Favorites",
            IndicatorCategory::MyScripts => "My scripts",
            IndicatorCategory::BuiltIns => "Built-ins",
            IndicatorCategory::Community => "Community",
            IndicatorCategory::Premium => "Premium",
        }
    }

    /// Get the sidebar icon for this category
    pub fn icon(&self) -> &'static crate::icons::Icon {
        use crate::icons::icons as embedded_icons;
        match self {
            IndicatorCategory::Favorites => &embedded_icons::EMOJI_ICON,
            IndicatorCategory::MyScripts => &embedded_icons::EMOJI_ICON,
            IndicatorCategory::BuiltIns => &embedded_icons::EMOJI_ICON,
            IndicatorCategory::Community => &embedded_icons::EMOJI_ICON,
            IndicatorCategory::Premium => &embedded_icons::EMOJI_ICON,
        }
    }
}

/// Simple indicator type enum for quick selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorType {
    SMA,
    EMA,
    WMA,
    VWMA,
    BollingerBands,
    RSI,
    MACD,
    Stochastic,
    CCI,
    ATR,
    ADX,
    OBV,
    MFI,
    VWAP,
    IchimokuCloud,
    SuperTrend,
    ParabolicSAR,
    KeltnerChannels,
    DonchianChannels,
    WilliamsR,
    ROC,
    Momentum,
    Aroon,
    ChaikinMoneyFlow,
}

impl IndicatorType {
    /// Return all available indicator types
    pub fn all() -> Vec<Self> {
        vec![
            Self::SMA,
            Self::EMA,
            Self::WMA,
            Self::VWMA,
            Self::BollingerBands,
            Self::RSI,
            Self::MACD,
            Self::Stochastic,
            Self::CCI,
            Self::ATR,
            Self::ADX,
            Self::OBV,
            Self::MFI,
            Self::VWAP,
            Self::IchimokuCloud,
            Self::SuperTrend,
            Self::ParabolicSAR,
            Self::KeltnerChannels,
            Self::DonchianChannels,
            Self::WilliamsR,
            Self::ROC,
            Self::Momentum,
            Self::Aroon,
            Self::ChaikinMoneyFlow,
        ]
    }

    /// Get the human-readable display name
    pub fn name(&self) -> &str {
        match self {
            Self::SMA => "Simple Moving Avg",
            Self::EMA => "Exponential Moving Avg",
            Self::WMA => "Weighted Moving Avg",
            Self::VWMA => "Volume Weighted Moving Avg",
            Self::BollingerBands => "Bollinger Bands",
            Self::RSI => "Relative Strength Index",
            Self::MACD => "MACD",
            Self::Stochastic => "Stochastic",
            Self::CCI => "Commodity Channel Index",
            Self::ATR => "Avg True Range",
            Self::ADX => "Avg Directional Index",
            Self::OBV => "On Balance Volume",
            Self::MFI => "Money Flow Index",
            Self::VWAP => "Volume Weighted Avg Price",
            Self::IchimokuCloud => "Ichimoku Cloud",
            Self::SuperTrend => "Supertrend",
            Self::ParabolicSAR => "Parabolic SAR",
            Self::KeltnerChannels => "Keltner Channels",
            Self::DonchianChannels => "Donchian Channels",
            Self::WilliamsR => "Williams %R",
            Self::ROC => "Rate of Change",
            Self::Momentum => "Momentum",
            Self::Aroon => "Aroon",
            Self::ChaikinMoneyFlow => "Chaikin Money Flow",
        }
    }

    /// Get the abbreviated name (e.g. "SMA", "RSI", "BB")
    pub fn short_name(&self) -> &str {
        match self {
            Self::SMA => "SMA",
            Self::EMA => "EMA",
            Self::WMA => "WMA",
            Self::VWMA => "VWMA",
            Self::BollingerBands => "BB",
            Self::RSI => "RSI",
            Self::MACD => "MACD",
            Self::Stochastic => "STOCH",
            Self::CCI => "CCI",
            Self::ATR => "ATR",
            Self::ADX => "ADX",
            Self::OBV => "OBV",
            Self::MFI => "MFI",
            Self::VWAP => "VWAP",
            Self::IchimokuCloud => "ICHI",
            Self::SuperTrend => "ST",
            Self::ParabolicSAR => "PSAR",
            Self::KeltnerChannels => "KC",
            Self::DonchianChannels => "DC",
            Self::WilliamsR => "W%R",
            Self::ROC => "ROC",
            Self::Momentum => "MOM",
            Self::Aroon => "AROON",
            Self::ChaikinMoneyFlow => "CMF",
        }
    }

    /// Get a brief description of the indicator
    pub fn desc(&self) -> &str {
        match self {
            Self::SMA => "Avg price over N periods",
            Self::EMA => "Weighted avg giving more importance to recent prices",
            Self::WMA => "Weighted avg with linear weights",
            Self::VWMA => "Moving avg weighted by volume",
            Self::BollingerBands => "Volatility indicator with upper and lower bands",
            Self::RSI => "Momentum oscillator (0-100)",
            Self::MACD => "Trend-following momentum indicator",
            Self::Stochastic => "Momentum indicator comparing closing price to price range",
            Self::CCI => "Measures price deviation from mean",
            Self::ATR => "Avg true range - volatility indicator",
            Self::ADX => "Trend strength indicator",
            Self::OBV => "Volume-based momentum indicator",
            Self::MFI => "Volume-weighted RSI",
            Self::VWAP => "Volume weighted avg price",
            Self::IchimokuCloud => "Multi-component trend indicator",
            Self::SuperTrend => "Trend following indicator",
            Self::ParabolicSAR => "Stop and reverse indicator",
            Self::KeltnerChannels => "Volatility-based envelope indicator",
            Self::DonchianChannels => "Highest high and lowest low channels",
            Self::WilliamsR => "Momentum indicator showing overbought/oversold",
            Self::ROC => "Rate of price change",
            Self::Momentum => "Price momentum indicator",
            Self::Aroon => "Identifies trend direction and strength",
            Self::ChaikinMoneyFlow => "Measures money flow over a period",
        }
    }

    /// Return true if this indicator renders as an overlay on the price chart
    pub fn is_overlay(&self) -> bool {
        matches!(
            self,
            Self::SMA
                | Self::EMA
                | Self::WMA
                | Self::VWMA
                | Self::BollingerBands
                | Self::VWAP
                | Self::IchimokuCloud
                | Self::SuperTrend
                | Self::ParabolicSAR
                | Self::KeltnerChannels
                | Self::DonchianChannels
        )
    }

    /// Get the unique string identifier used for serialization and lookup
    pub fn id(&self) -> &str {
        match self {
            Self::SMA => "sma",
            Self::EMA => "ema",
            Self::WMA => "wma",
            Self::VWMA => "vwma",
            Self::BollingerBands => "bb",
            Self::RSI => "rsi",
            Self::MACD => "macd",
            Self::Stochastic => "stoch",
            Self::CCI => "cci",
            Self::ATR => "atr",
            Self::ADX => "adx",
            Self::OBV => "obv",
            Self::MFI => "mfi",
            Self::VWAP => "vwap",
            Self::IchimokuCloud => "ichimoku",
            Self::SuperTrend => "supertrend",
            Self::ParabolicSAR => "psar",
            Self::KeltnerChannels => "keltner",
            Self::DonchianChannels => "donchian",
            Self::WilliamsR => "williams",
            Self::ROC => "roc",
            Self::Momentum => "momentum",
            Self::Aroon => "aroon",
            Self::ChaikinMoneyFlow => "chaikin",
        }
    }

    /// Look up an indicator type by its string ID, returning None if unrecognized
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "sma" => Some(Self::SMA),
            "ema" => Some(Self::EMA),
            "wma" => Some(Self::WMA),
            "vwma" => Some(Self::VWMA),
            "bb" => Some(Self::BollingerBands),
            "rsi" => Some(Self::RSI),
            "macd" => Some(Self::MACD),
            "stoch" => Some(Self::Stochastic),
            "cci" => Some(Self::CCI),
            "atr" => Some(Self::ATR),
            "adx" => Some(Self::ADX),
            "obv" => Some(Self::OBV),
            "mfi" => Some(Self::MFI),
            "vwap" => Some(Self::VWAP),
            "ichimoku" => Some(Self::IchimokuCloud),
            "supertrend" => Some(Self::SuperTrend),
            "psar" => Some(Self::ParabolicSAR),
            "keltner" => Some(Self::KeltnerChannels),
            "donchian" => Some(Self::DonchianChannels),
            "williams" => Some(Self::WilliamsR),
            "roc" => Some(Self::ROC),
            "momentum" => Some(Self::Momentum),
            "aroon" => Some(Self::Aroon),
            "chaikin" => Some(Self::ChaikinMoneyFlow),
            _ => None,
        }
    }
}
