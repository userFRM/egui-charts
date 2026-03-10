//! Study (indicator) related enums
//!
//! Study plot types, input types, display targets, and plot styles
//! for indicator/study configuration.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Study plot type — defines how a study output is rendered
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum StudyPlotType {
    /// Standard line plot
    #[default]
    Line,
    /// Histogram bars
    Histogram,
    /// Arrow markers (up/down)
    Arrow,
    /// Column (vertical bar from zero)
    Column,
    /// Area (filled below line)
    Area,
    /// Circles / dots
    Circles,
    /// Line with breaks (NaN gaps)
    LineWithBreaks,
    /// Area with breaks
    AreaWithBreaks,
    /// Step line (horizontal then vertical)
    StepLine,
    /// Step line with diamonds at corners
    StepLineDiamond,
    /// Cross markers
    Cross,
    /// OHLC bars (for multi-output studies)
    OhlcBars,
    /// Candlestick (for multi-output studies)
    OhlcCandles,
    /// Color-coded line
    ColorLine,
    /// Color-coded histogram
    ColorHistogram,
    /// Color-coded column
    ColorColumn,
    /// Color-coded area
    ColorArea,
    /// Shape markers (various shapes)
    Shapes,
}

impl StudyPlotType {
    pub fn all() -> &'static [Self] {
        &[
            Self::Line,
            Self::Histogram,
            Self::Arrow,
            Self::Column,
            Self::Area,
            Self::Circles,
            Self::LineWithBreaks,
            Self::AreaWithBreaks,
            Self::StepLine,
            Self::StepLineDiamond,
            Self::Cross,
            Self::OhlcBars,
            Self::OhlcCandles,
            Self::ColorLine,
            Self::ColorHistogram,
            Self::ColorColumn,
            Self::ColorArea,
            Self::Shapes,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Histogram => "histogram",
            Self::Arrow => "arrow",
            Self::Column => "column",
            Self::Area => "area",
            Self::Circles => "circles",
            Self::LineWithBreaks => "line_with_breaks",
            Self::AreaWithBreaks => "area_with_breaks",
            Self::StepLine => "step_line",
            Self::StepLineDiamond => "step_line_diamond",
            Self::Cross => "cross",
            Self::OhlcBars => "ohlc_bars",
            Self::OhlcCandles => "ohlc_candles",
            Self::ColorLine => "color_line",
            Self::ColorHistogram => "color_histogram",
            Self::ColorColumn => "color_column",
            Self::ColorArea => "color_area",
            Self::Shapes => "shapes",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::Histogram => "Histogram",
            Self::Arrow => "Arrow",
            Self::Column => "Column",
            Self::Area => "Area",
            Self::Circles => "Circles",
            Self::LineWithBreaks => "Line with Breaks",
            Self::AreaWithBreaks => "Area with Breaks",
            Self::StepLine => "Step Line",
            Self::StepLineDiamond => "Step Line Diamond",
            Self::Cross => "Cross",
            Self::OhlcBars => "OHLC Bars",
            Self::OhlcCandles => "OHLC Candles",
            Self::ColorLine => "Color Line",
            Self::ColorHistogram => "Color Histogram",
            Self::ColorColumn => "Color Column",
            Self::ColorArea => "Color Area",
            Self::Shapes => "Shapes",
        }
    }
}

impl fmt::Display for StudyPlotType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Bitflags-style display target for study plots
///
/// Controls which pane(s) a study plot renders in.
/// Values can be OR'd together: `PriceScale | SeparatePane` = 3
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StudyDisplayTarget {
    /// Do not display
    None,
    /// Display on the main price scale (value = 1)
    #[default]
    PriceScale,
    /// Display in a separate pane below the chart (value = 2)
    SeparatePane,
    /// Display on the status line only (value = 4)
    StatusLine,
    /// Display in data window only (value = 8)
    DataWindow,
    /// Display everywhere (value = 15)
    All,
}

impl StudyDisplayTarget {
    pub fn all() -> &'static [Self] {
        &[
            Self::None,
            Self::PriceScale,
            Self::SeparatePane,
            Self::StatusLine,
            Self::DataWindow,
            Self::All,
        ]
    }

    /// Numeric value for display target bitflags
    pub fn value(&self) -> u8 {
        match self {
            Self::None => 0,
            Self::PriceScale => 1,
            Self::SeparatePane => 2,
            Self::StatusLine => 4,
            Self::DataWindow => 8,
            Self::All => 15,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PriceScale => "price_scale",
            Self::SeparatePane => "separate_pane",
            Self::StatusLine => "status_line",
            Self::DataWindow => "data_window",
            Self::All => "all",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::PriceScale => "Price Scale",
            Self::SeparatePane => "Separate Pane",
            Self::StatusLine => "Status Line",
            Self::DataWindow => "Data Window",
            Self::All => "All",
        }
    }
}

impl fmt::Display for StudyDisplayTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Study input parameter type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StudyInputType {
    /// Integer input
    #[default]
    Integer,
    /// Floating point input
    Float,
    /// Boolean toggle
    Bool,
    /// Text / string input
    Text,
    /// Color picker
    Color,
    /// Symbol search input
    Symbol,
    /// Resolution / timeframe selector
    Resolution,
    /// Session selector
    Session,
    /// Source selector (close, open, high, low, etc.)
    Source,
    /// Line style selector
    LineStyle,
    /// Line width selector
    LineWidth,
    /// Price source (another study's output)
    PriceSource,
    /// Custom input defined by the study
    Custom,
}

impl StudyInputType {
    pub fn all() -> &'static [Self] {
        &[
            Self::Integer,
            Self::Float,
            Self::Bool,
            Self::Text,
            Self::Color,
            Self::Symbol,
            Self::Resolution,
            Self::Session,
            Self::Source,
            Self::LineStyle,
            Self::LineWidth,
            Self::PriceSource,
            Self::Custom,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::Text => "text",
            Self::Color => "color",
            Self::Symbol => "symbol",
            Self::Resolution => "resolution",
            Self::Session => "session",
            Self::Source => "source",
            Self::LineStyle => "line_style",
            Self::LineWidth => "line_width",
            Self::PriceSource => "price_source",
            Self::Custom => "custom",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Integer => "Integer",
            Self::Float => "Float",
            Self::Bool => "Boolean",
            Self::Text => "Text",
            Self::Color => "Color",
            Self::Symbol => "Symbol",
            Self::Resolution => "Resolution",
            Self::Session => "Session",
            Self::Source => "Source",
            Self::LineStyle => "Line Style",
            Self::LineWidth => "Line Width",
            Self::PriceSource => "Price Source",
            Self::Custom => "Custom",
        }
    }
}

impl fmt::Display for StudyInputType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Target price scale for a study
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StudyTargetPriceScale {
    /// Left price scale
    Left,
    /// Right price scale
    #[default]
    Right,
    /// No price scale (overlay)
    NoScale,
}

impl StudyTargetPriceScale {
    pub fn all() -> &'static [Self] {
        &[Self::Left, Self::Right, Self::NoScale]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::NoScale => "no_scale",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::NoScale => "No Scale",
        }
    }
}

impl fmt::Display for StudyTargetPriceScale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Line study plot style (for studies that render as line tools)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineStudyPlotStyle {
    /// Solid line
    #[default]
    Line,
    /// Histogram bars
    Histogram,
    /// Cross markers
    Cross,
    /// Area fill
    Area,
    /// Column bars
    Columns,
    /// Circles / dots
    Circles,
    /// Arrow markers
    Arrows,
    /// Step line
    StepLine,
    /// Bars (OHLC)
    Bars,
    /// Candlestick
    Candles,
    /// Hollow candles
    HollowCandles,
}

impl LineStudyPlotStyle {
    pub fn all() -> &'static [Self] {
        &[
            Self::Line,
            Self::Histogram,
            Self::Cross,
            Self::Area,
            Self::Columns,
            Self::Circles,
            Self::Arrows,
            Self::StepLine,
            Self::Bars,
            Self::Candles,
            Self::HollowCandles,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Histogram => "histogram",
            Self::Cross => "cross",
            Self::Area => "area",
            Self::Columns => "columns",
            Self::Circles => "circles",
            Self::Arrows => "arrows",
            Self::StepLine => "step_line",
            Self::Bars => "bars",
            Self::Candles => "candles",
            Self::HollowCandles => "hollow_candles",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::Histogram => "Histogram",
            Self::Cross => "Cross",
            Self::Area => "Area",
            Self::Columns => "Columns",
            Self::Circles => "Circles",
            Self::Arrows => "Arrows",
            Self::StepLine => "Step Line",
            Self::Bars => "Bars",
            Self::Candles => "Candles",
            Self::HollowCandles => "Hollow Candles",
        }
    }
}

impl fmt::Display for LineStudyPlotStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// OHLC study plot style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OhlcStudyPlotStyle {
    /// Traditional OHLC bars
    #[default]
    OhlcBars,
    /// Japanese candlesticks
    Candles,
}

impl OhlcStudyPlotStyle {
    pub fn all() -> &'static [Self] {
        &[Self::OhlcBars, Self::Candles]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::OhlcBars => "ohlc_bars",
            Self::Candles => "candles",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::OhlcBars => "OHLC Bars",
            Self::Candles => "Candles",
        }
    }
}

impl fmt::Display for OhlcStudyPlotStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Filled area type (for study areas between two plots)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FilledAreaType {
    /// Fill between two plot lines
    #[default]
    AbsolutePrice,
    /// Fill between a plot and a horizontal level
    HorizontalLine,
}

impl FilledAreaType {
    pub fn all() -> &'static [Self] {
        &[Self::AbsolutePrice, Self::HorizontalLine]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::AbsolutePrice => "absolute_price",
            Self::HorizontalLine => "horizontal_line",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::AbsolutePrice => "Between Plots",
            Self::HorizontalLine => "To Level",
        }
    }
}

impl fmt::Display for FilledAreaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_study_plot_type_count() {
        // 18 variants (not counting default duplicate)
        assert!(StudyPlotType::all().len() >= 18);
    }

    #[test]
    fn test_display_target_values() {
        assert_eq!(StudyDisplayTarget::None.value(), 0);
        assert_eq!(StudyDisplayTarget::PriceScale.value(), 1);
        assert_eq!(StudyDisplayTarget::All.value(), 15);
    }

    #[test]
    fn test_study_input_type_count() {
        assert_eq!(StudyInputType::all().len(), 13);
    }
}
