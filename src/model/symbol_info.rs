//! Extended symbol information
//!
//! Comprehensive symbol metadata with 50+ fields.
//! Provides comprehensive metadata for financial instruments including
//! session info, pricing parameters, supported resolutions, and display options.

use serde::{Deserialize, Serialize};

/// Symbol type categories for instrument classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SymbolType {
    #[default]
    Stock,
    Index,
    Forex,
    Futures,
    Crypto,
    Cfd,
    Bond,
    Fund,
    Option,
    Commodity,
    Economic,
    Dr,
    Right,
    Warrant,
    Expression,
    Spread,
}

impl std::fmt::Display for SymbolType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stock => write!(f, "stock"),
            Self::Index => write!(f, "index"),
            Self::Forex => write!(f, "forex"),
            Self::Futures => write!(f, "futures"),
            Self::Crypto => write!(f, "crypto"),
            Self::Cfd => write!(f, "cfd"),
            Self::Bond => write!(f, "bond"),
            Self::Fund => write!(f, "fund"),
            Self::Option => write!(f, "option"),
            Self::Commodity => write!(f, "commodity"),
            Self::Economic => write!(f, "economic"),
            Self::Dr => write!(f, "dr"),
            Self::Right => write!(f, "right"),
            Self::Warrant => write!(f, "warrant"),
            Self::Expression => write!(f, "expression"),
            Self::Spread => write!(f, "spread"),
        }
    }
}

/// Data status indicating how the symbol's data is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum DataStatus {
    /// Real-time streaming data
    #[default]
    Streaming,
    /// End-of-day updates only
    EndOfDay,
    /// Pulsed/periodic updates
    Pulsed,
    /// Delayed data (typically 15-20 minutes)
    Delayed,
}

impl std::fmt::Display for DataStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Streaming => write!(f, "streaming"),
            Self::EndOfDay => write!(f, "endofday"),
            Self::Pulsed => write!(f, "pulsed"),
            Self::Delayed => write!(f, "delayed"),
        }
    }
}

/// Symbol format for price display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SymbolFormat {
    /// Standard price format (e.g. 150.25)
    #[default]
    Price,
    /// Volume format
    Volume,
}

impl std::fmt::Display for SymbolFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Price => write!(f, "price"),
            Self::Volume => write!(f, "volume"),
        }
    }
}

/// Subsession information for extended/pre-market trading hours.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsessionInfo {
    /// Unique identifier for the subsession (e.g. "regular", "premarket")
    pub id: String,
    /// Human-readable description
    pub description: String,
    /// Session string (e.g. "0930-1600")
    pub session: String,
}

/// Comprehensive symbol metadata for chart configuration.
///
/// Contains all fields necessary for proper chart configuration,
/// price formatting, session handling, and data resolution support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibrarySymbolInfo {
    // ── Identity ──────────────────────────────────────────────
    /// Full symbol name (e.g. "NASDAQ:AAPL")
    pub name: String,
    /// Short ticker (e.g. "AAPL"), if different from name
    pub ticker: Option<String>,
    /// Human-readable description (e.g. "Apple Inc.")
    pub description: String,

    // ── Exchange ──────────────────────────────────────────────
    /// Exchange where the symbol is traded (e.g. "NASDAQ")
    pub exchange: String,
    /// Exchange where the symbol is officially listed
    pub listed_exchange: String,
    /// Instrument type classification
    pub symbol_type: SymbolType,

    // ── Session ──────────────────────────────────────────────
    /// IANA timezone (e.g. "America/New_York")
    pub timezone: String,
    /// Session definition (e.g. "0930-1600:23456")
    pub session: String,
    /// Holiday dates when the exchange is closed
    pub session_holidays: Option<String>,
    /// Session corrections for special trading days
    pub corrections: Option<String>,
    /// Extended session definitions (pre-market, after-hours, etc.)
    pub subsessions: Vec<SubsessionInfo>,

    // ── Price Format ─────────────────────────────────────────
    /// Price scale denominator (e.g. 100 means 2 decimal places)
    pub pricescale: i64,
    /// Minimum price movement numerator
    pub minmov: i64,
    /// Secondary minimum movement (for complex tick sizes)
    pub minmove2: Option<i64>,
    /// Whether prices are displayed as fractions (e.g. bonds)
    pub fractional: bool,
    /// Variable tick size rules (e.g. for futures with different tick sizes at different prices)
    pub variable_tick_size: Option<String>,

    // ── Data Capabilities ────────────────────────────────────
    /// Symbol supports intraday resolutions
    pub has_intraday: bool,
    /// Symbol supports seconds-based resolutions
    pub has_seconds: bool,
    /// Symbol supports tick data
    pub has_ticks: bool,
    /// Symbol supports daily resolution
    pub has_daily: bool,
    /// Symbol supports weekly and monthly resolutions
    pub has_weekly_and_monthly: bool,
    /// Symbol has no volume data
    pub has_no_volume: bool,
    /// Symbol may have bars with no trades
    pub has_empty_bars: bool,
    /// Number of decimal places for volume display
    pub volume_precision: Option<u8>,

    // ── Resolution Support ───────────────────────────────────
    /// List of supported resolution strings (e.g. ["1", "5", "15", "60", "D", "W"])
    pub supported_resolutions: Vec<String>,
    /// Intraday multipliers (e.g. ["1", "5", "15", "30", "60"])
    pub intraday_multipliers: Vec<String>,
    /// Seconds-based multipliers (e.g. ["1", "5", "10"])
    pub seconds_multipliers: Option<Vec<String>>,

    // ── Data Status ──────────────────────────────────────────
    /// How data is delivered for this symbol
    pub data_status: DataStatus,

    // ── Expiration (Futures/Options) ─────────────────────────
    /// Whether the contract has expired
    pub expired: bool,
    /// Expiration date in YYYYMMDD format
    pub expiration_date: Option<String>,

    // ── Currency ─────────────────────────────────────────────
    /// Currency code for the symbol's price (e.g. "USD")
    pub currency_code: Option<String>,
    /// Original currency before any conversion
    pub original_currency_code: Option<String>,

    // ── Units ────────────────────────────────────────────────
    /// Unit identifier for the symbol's value
    pub unit_id: Option<String>,
    /// Original unit before any conversion
    pub original_unit_id: Option<String>,

    // ── Branding ─────────────────────────────────────────────
    /// Logo image URLs for the symbol
    pub logo_urls: Option<Vec<String>>,
    /// Exchange logo URL
    pub exchange_logo: Option<String>,

    // ── Identifiers ──────────────────────────────────────────
    /// ISIN code (International Securities Identification Number)
    pub isin: Option<String>,

    // ── Display ──────────────────────────────────────────────
    /// Visible plots configuration (e.g. "ohlcv", "c")
    pub visible_plots_set: Option<String>,
    /// Price display format
    pub format: SymbolFormat,

    // ── Additional Fields for 100% TV Compatibility ───────────
    /// Array of base symbol names for spreads and composite symbols
    pub base_name: Option<Vec<String>>,

    /// Whether to build seconds timeframe from tick data
    pub build_seconds_from_ticks: Option<bool>,

    /// Library custom fields metadata (key-value pairs)
    pub library_custom_fields: Option<std::collections::HashMap<String, String>>,

    /// Current subsession ID for extended trading hours
    pub subsession_id: Option<String>,

    /// Session display string for UI (e.g., "Regular", "Pre-market")
    pub session_display: Option<String>,

    /// Unit conversion types available for this symbol
    pub unit_conversion_types: Option<Vec<String>>,

    /// Delay in seconds for delayed data feeds
    pub delay: Option<i64>,

    /// Extended description (longer than description field)
    pub long_description: Option<String>,

    /// Selected price source ID
    pub price_source_id: Option<String>,

    /// Daily resolution multipliers
    pub daily_multipliers: Option<Vec<String>>,

    /// Weekly resolution multipliers  
    pub weekly_multipliers: Option<Vec<String>>,

    /// Monthly resolution multipliers
    pub monthly_multipliers: Option<Vec<String>>,
}

impl Default for LibrarySymbolInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            ticker: None,
            description: String::new(),
            exchange: String::new(),
            listed_exchange: String::new(),
            symbol_type: SymbolType::default(),
            timezone: "Etc/UTC".to_owned(),
            session: "24x7".to_owned(),
            session_holidays: None,
            corrections: None,
            subsessions: Vec::new(),
            pricescale: 100,
            minmov: 1,
            minmove2: None,
            fractional: false,
            variable_tick_size: None,
            has_intraday: true,
            has_seconds: false,
            has_ticks: false,
            has_daily: true,
            has_weekly_and_monthly: true,
            has_no_volume: false,
            has_empty_bars: false,
            volume_precision: None,
            supported_resolutions: vec![
                "1".to_owned(),
                "5".to_owned(),
                "15".to_owned(),
                "30".to_owned(),
                "60".to_owned(),
                "D".to_owned(),
                "W".to_owned(),
                "M".to_owned(),
            ],
            intraday_multipliers: vec![
                "1".to_owned(),
                "5".to_owned(),
                "15".to_owned(),
                "30".to_owned(),
                "60".to_owned(),
            ],
            seconds_multipliers: None,
            data_status: DataStatus::default(),
            expired: false,
            expiration_date: None,
            currency_code: None,
            original_currency_code: None,
            unit_id: None,
            original_unit_id: None,
            logo_urls: None,
            exchange_logo: None,
            isin: None,
            visible_plots_set: None,
            format: SymbolFormat::default(),

            // Additional fields for 100% TV compatibility
            base_name: None,
            build_seconds_from_ticks: None,
            library_custom_fields: None,
            subsession_id: None,
            session_display: None,
            unit_conversion_types: None,
            delay: None,
            long_description: None,
            price_source_id: None,
            daily_multipliers: None,
            weekly_multipliers: None,
            monthly_multipliers: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_type_display() {
        assert_eq!(SymbolType::Stock.to_string(), "stock");
        assert_eq!(SymbolType::Crypto.to_string(), "crypto");
        assert_eq!(SymbolType::Futures.to_string(), "futures");
        assert_eq!(SymbolType::Forex.to_string(), "forex");
    }

    #[test]
    fn test_data_status_display() {
        assert_eq!(DataStatus::Streaming.to_string(), "streaming");
        assert_eq!(DataStatus::EndOfDay.to_string(), "endofday");
        assert_eq!(DataStatus::Delayed.to_string(), "delayed");
    }

    #[test]
    fn test_symbol_format_display() {
        assert_eq!(SymbolFormat::Price.to_string(), "price");
        assert_eq!(SymbolFormat::Volume.to_string(), "volume");
    }

    #[test]
    fn test_default_library_symbol_info() {
        let info = LibrarySymbolInfo::default();
        assert_eq!(info.pricescale, 100);
        assert_eq!(info.minmov, 1);
        assert!(info.has_intraday);
        assert!(info.has_daily);
        assert!(!info.has_no_volume);
        assert!(!info.expired);
        assert_eq!(info.timezone, "Etc/UTC");
        assert_eq!(info.session, "24x7");
        assert_eq!(info.data_status, DataStatus::Streaming);
        assert_eq!(info.format, SymbolFormat::Price);
        assert_eq!(info.supported_resolutions.len(), 8);
    }

    #[test]
    fn test_symbol_type_default() {
        assert_eq!(SymbolType::default(), SymbolType::Stock);
    }

    #[test]
    fn test_library_symbol_info_serialize() {
        let info = LibrarySymbolInfo {
            name: "NASDAQ:AAPL".to_owned(),
            ticker: Some("AAPL".to_owned()),
            description: "Apple Inc.".to_owned(),
            exchange: "NASDAQ".to_owned(),
            listed_exchange: "NASDAQ".to_owned(),
            symbol_type: SymbolType::Stock,
            currency_code: Some("USD".to_owned()),
            ..Default::default()
        };
        let json = serde_json::to_string(&info).expect("serialize");
        assert!(json.contains("NASDAQ:AAPL"));
        assert!(json.contains("Apple Inc."));
    }
}
