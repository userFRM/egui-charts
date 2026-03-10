//! Core types for the Object Tree panel
//!
//! Defines the data structures representing chart objects:
//! - `SourceItem`: An object in the tree (drawing or indicator)
//! - `SourceType`: Classification of source items
//! - `DataWindowInfo`: OHLCV data at cursor position
//! - `DrawingProperties`: Expandable properties for drawings

use crate::drawings::DrawingToolType;
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Type of source in the tree
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceType {
    /// Main data source (chart symbol and timeframe)
    DataSource,
    /// Technical indicator (SMA, RSI, MACD, etc.)
    Indicator,
    /// Drawing tool (trend line, fibonacci, shapes, etc.)
    Drawing,
    /// Template (saved drawing configuration)
    Template,
}

impl SourceType {
    /// Get display name for the source type
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::DataSource => "Data Source",
            Self::Indicator => "Indicators",
            Self::Drawing => "Drawings",
            Self::Template => "Templates",
        }
    }

    /// Get icon for the source type group header
    pub fn icon(&self) -> &'static crate::icons::Icon {
        use crate::icons::icons as embedded_icons;
        match self {
            Self::DataSource => &embedded_icons::EMOJI_ICON,
            Self::Indicator => &embedded_icons::EMOJI_ICON,
            Self::Drawing => &embedded_icons::EMOJI_ICON,
            Self::Template => &embedded_icons::EMOJI_ICON,
        }
    }
}

/// Line style for drawings
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl LineStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Dashed => "Dashed",
            Self::Dotted => "Dotted",
        }
    }
}

/// Expandable properties for drawings
///
/// These are shown when a drawing item is expanded in the tree.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawingProperties {
    /// Primary color
    pub color: Color32,
    /// Line width (1-5 pixels)
    pub line_width: f32,
    /// Line style
    pub line_style: LineStyle,
    /// Transparency (0-100%)
    pub transparency: u8,
    /// Fill color (for shapes)
    pub fill_color: Option<Color32>,
    /// Whether fill is enabled
    pub fill_enabled: bool,
    /// Extend line to left
    pub extend_left: bool,
    /// Extend line to right
    pub extend_right: bool,
    /// Show price labels
    pub show_price_labels: bool,
    /// Show coordinates
    pub show_coordinates: bool,
}

impl Default for DrawingProperties {
    fn default() -> Self {
        Self {
            color: DESIGN_TOKENS.semantic.extended.accent,
            line_width: 1.0,
            line_style: LineStyle::Solid,
            transparency: 0,
            fill_color: None,
            fill_enabled: false,
            extend_left: false,
            extend_right: false,
            show_price_labels: false,
            show_coordinates: false,
        }
    }
}

/// A source item in the object tree (indicator or drawing)
///
/// Represents a single row in the Object Tree with all its properties
/// and state for UI rendering.
#[derive(Clone, Debug)]
pub struct SourceItem {
    /// Unique identifier
    pub id: usize,
    /// Source type (indicator, drawing, template)
    pub source_type: SourceType,
    /// For drawings: the tool type
    pub tool_type: Option<DrawingToolType>,
    /// Display name (e.g., "Trend Line", "SMA(20)")
    pub name: String,
    /// User-defined label (overrides name if set)
    pub label: Option<String>,
    /// Primary color for display
    pub color: Color32,
    /// Is visible on chart
    pub visible: bool,
    /// Is locked (drawings only - prevents modification)
    pub locked: bool,
    /// Is selected in the tree
    pub selected: bool,
    /// Is expanded (shows properties)
    pub expanded: bool,
    /// Z-index for rendering order
    pub z_index: i32,
    /// Drawing properties (for drawings only)
    pub properties: Option<DrawingProperties>,
    /// Indicator parameters string (for indicators, e.g., "20, close")
    pub parameters: Option<String>,
}

impl SourceItem {
    /// Create a new drawing source
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier
    /// * `tool_type` - Type of drawing tool
    /// * `color` - Primary color
    pub fn drawing(id: usize, tool_type: DrawingToolType, color: Color32) -> Self {
        Self {
            id,
            source_type: SourceType::Drawing,
            tool_type: Some(tool_type),
            name: tool_type.as_str().to_string(),
            label: None,
            color,
            visible: true,
            locked: false,
            selected: false,
            expanded: false,
            z_index: id as i32,
            properties: Some(DrawingProperties {
                color,
                ..Default::default()
            }),
            parameters: None,
        }
    }

    /// Create a new indicator source
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier
    /// * `name` - Indicator name (e.g., "SMA")
    /// * `color` - Line color
    pub fn indicator(id: usize, name: impl Into<String>, color: Color32) -> Self {
        Self {
            id,
            source_type: SourceType::Indicator,
            tool_type: None,
            name: name.into(),
            label: None,
            color,
            visible: true,
            locked: false,
            selected: false,
            expanded: false,
            z_index: id as i32,
            properties: None,
            parameters: None,
        }
    }

    /// Create a new indicator with parameters
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier
    /// * `name` - Indicator name (e.g., "SMA")
    /// * `params` - Parameters string (e.g., "20, close")
    /// * `color` - Line color
    pub fn indicator_with_params(
        id: usize,
        name: impl Into<String>,
        params: impl Into<String>,
        color: Color32,
    ) -> Self {
        let mut item = Self::indicator(id, name, color);
        item.parameters = Some(params.into());
        item
    }

    /// Create a new template source
    pub fn template(id: usize, name: impl Into<String>) -> Self {
        Self {
            id,
            source_type: SourceType::Template,
            tool_type: None,
            name: name.into(),
            label: None,
            color: Color32::GRAY,
            visible: true,
            locked: false,
            selected: false,
            expanded: false,
            z_index: id as i32,
            properties: None,
            parameters: None,
        }
    }

    /// Create a new data source (main chart symbol)
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier
    /// * `name` - Data source name (e.g., "BTCUSDT · 1H")
    pub fn data_source(id: usize, name: impl Into<String>) -> Self {
        Self {
            id,
            source_type: SourceType::DataSource,
            tool_type: None,
            name: name.into(),
            label: None,
            color: DESIGN_TOKENS.semantic.extended.text_secondary, // Use theme constant for muted text
            visible: true,
            locked: true, // Data source is always locked
            selected: false,
            expanded: true, // Data source is expanded by default
            z_index: 0,     // Data source is always at z-index 0
            properties: None,
            parameters: None,
        }
    }

    /// Get display name (label if set, otherwise name)
    pub fn display_name(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }

    /// Get full display name including parameters
    pub fn full_display_name(&self) -> String {
        if let Some(params) = &self.parameters {
            format!("{} ({})", self.display_name(), params)
        } else {
            self.display_name().to_string()
        }
    }

    /// Set custom label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set z-index
    pub fn with_z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }

    /// Check if this is a drawing
    pub fn is_drawing(&self) -> bool {
        self.source_type == SourceType::Drawing
    }

    /// Check if this is an indicator
    pub fn is_indicator(&self) -> bool {
        self.source_type == SourceType::Indicator
    }
}

/// Data Window information (OHLCV at cursor position)
///
/// Displays real-time price data for the bar under the cursor.
#[derive(Clone, Debug, Default)]
pub struct DataWindowInfo {
    /// Symbol name (e.g., "XAUUSD")
    pub symbol: String,
    /// Timestamp at cursor position
    pub timestamp: String,
    /// Timeframe (e.g., "1m", "1H", "1D")
    pub timeframe: String,
    /// Open price
    pub open: f64,
    /// High price
    pub high: f64,
    /// Low price
    pub low: f64,
    /// Close price
    pub close: f64,
    /// Volume
    pub volume: f64,
    /// Change from previous close (absolute)
    pub change: f64,
    /// Change from previous close (percentage)
    pub change_percent: f64,
    /// Previous close price
    pub prev_close: f64,
}

impl DataWindowInfo {
    /// Create new data window info
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            ..Default::default()
        }
    }

    /// Set OHLCV data
    pub fn with_ohlcv(mut self, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Self {
        self.open = open;
        self.high = high;
        self.low = low;
        self.close = close;
        self.volume = volume;
        self
    }

    /// Set timestamp
    pub fn with_timestamp(mut self, ts: impl Into<String>) -> Self {
        self.timestamp = ts.into();
        self
    }

    /// Set timeframe
    pub fn with_timeframe(mut self, tf: impl Into<String>) -> Self {
        self.timeframe = tf.into();
        self
    }

    /// Calculate and set change from previous close
    pub fn with_prev_close(mut self, prev: f64) -> Self {
        self.prev_close = prev;
        self.change = self.close - prev;
        self.change_percent = if prev != 0.0 {
            (self.change / prev) * 100.0
        } else {
            0.0
        };
        self
    }

    /// Check if price is bullish (close >= open or change >= 0)
    pub fn is_bullish(&self) -> bool {
        self.change >= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_item_drawing() {
        let item = SourceItem::drawing(1, DrawingToolType::TrendLine, Color32::BLUE);
        assert!(item.is_drawing());
        assert!(!item.is_indicator());
        assert_eq!(item.tool_type, Some(DrawingToolType::TrendLine));
        assert!(item.properties.is_some());
    }

    #[test]
    fn test_source_item_indicator() {
        let item = SourceItem::indicator_with_params(2, "SMA", "20, close", Color32::YELLOW);
        assert!(item.is_indicator());
        assert!(!item.is_drawing());
        assert_eq!(item.parameters.as_deref(), Some("20, close"));
        assert_eq!(item.full_display_name(), "SMA (20, close)");
    }

    #[test]
    fn test_data_window_info() {
        let info = DataWindowInfo::new("XAUUSD")
            .with_ohlcv(2000.0, 2010.0, 1990.0, 2005.0, 100000.0)
            .with_prev_close(1995.0);

        assert!(info.is_bullish());
        assert_eq!(info.change, 10.0);
        assert!((info.change_percent - 0.501).abs() < 0.01);
    }
}
