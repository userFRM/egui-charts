//! Icon mapping functions for drawing tools and categories.
//!
//! Pure mapping functions that translate DrawingToolType to SVG icons.
//! No state, no dependencies - just mappings.
//! Full parity - 97 tools.

use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons};

/// Get icon for a category
pub fn category_icon(category: &str) -> &'static Icon {
    match category {
        "Cursors" => &icons::ERASER,
        "Lines" => &icons::TREND_LINE,
        "Fibonacci" => &icons::FIB_RETRACEMENT,
        "Patterns" => &icons::XABCD_PATTERN,
        "Projection" => &icons::LONG_POSITION,
        "Brushes/Shapes" => &icons::BRUSH,
        "Text/Annotations" => &icons::TEXT,
        "Icons/Emojis" => &icons::EMOJI_ICON,
        // Legacy support
        "Channels" => &icons::PARALLEL_CHANNEL,
        "Gann" => &icons::GANN_FAN,
        "Shapes" => &icons::RECT,
        "Brushes" => &icons::BRUSH,
        "Arrows" => &icons::ARROW_MARKER,
        "Pitchforks" => &icons::PITCHFORK,
        "Elliott" => &icons::ELLIOTT_IMPULSE,
        "Elliott Wave" => &icons::ELLIOTT_IMPULSE,
        "Cycles" => &icons::TIME_CYCLES,
        "Volume" => &icons::ANCHORED_VOLUME_PROFILE,
        "Measurers" => &icons::MEASURE,
        "Annotations" => &icons::TEXT,
        "Content" => &icons::TEXT,
        _ => &icons::TREND_LINE,
    }
}

/// Get icon for a specific tool
///
/// Maps all 97 tools to appropriate icons.
pub fn get_icon(tool: DrawingToolType) -> &'static Icon {
    match tool {
        // ============ CURSORS (4 tools) ============
        DrawingToolType::CrossCursor => &icons::ERASER,
        DrawingToolType::DotCursor => &icons::DOT,
        DrawingToolType::ArrowCursor => &icons::ARROW,
        DrawingToolType::Eraser => &icons::ERASER,

        // ============ LINES (9 tools) ============
        DrawingToolType::TrendLine => &icons::TREND_LINE,
        DrawingToolType::Ray => &icons::RAY,
        DrawingToolType::InfoLine => &icons::INFO_LINE,
        DrawingToolType::ExtendedLine => &icons::EXTENDED,
        DrawingToolType::TrendAngle => &icons::TREND_ANGLE,
        DrawingToolType::HorizontalLine => &icons::HORIZONTAL_LINE,
        DrawingToolType::HorizontalRay => &icons::HORIZONTAL_RAY,
        DrawingToolType::VerticalLine => &icons::VERTICAL_LINE,
        DrawingToolType::CrossLine => &icons::CROSS_LINE,

        // ============ CHANNELS (4 tools) ============
        DrawingToolType::ParallelChannel => &icons::PARALLEL_CHANNEL,
        DrawingToolType::RegressionTrend => &icons::REGRESSION_TREND,
        DrawingToolType::FlatTopBottom => &icons::FLAT_BOTTOM,
        DrawingToolType::DisjointChannel => &icons::DISJOINT_ANGLE,

        // ============ PITCHFORKS (4 tools) ============
        DrawingToolType::Pitchfork => &icons::PITCHFORK,
        DrawingToolType::SchiffPitchfork => &icons::SCHIFF_PITCHFORK,
        DrawingToolType::ModifiedSchiffPitchfork => &icons::SCHIFF_PITCHFORK2,
        DrawingToolType::InsidePitchfork => &icons::INSIDE_PITCHFORK,

        // ============ FIBONACCI (11 tools) ============
        DrawingToolType::FibonacciRetracement => &icons::FIB_RETRACEMENT,
        DrawingToolType::FibonacciExtension => &icons::FIB_EXTENSION,
        DrawingToolType::FibonacciChannel => &icons::FIB_CHANNEL,
        DrawingToolType::FibonacciTimeZones => &icons::FIB_TIME_ZONE,
        DrawingToolType::FibonacciSpeedFan => &icons::FIB_SPEED_RESISTANCE_FAN,
        DrawingToolType::TrendBasedFibTime => &icons::TREND_BASED_FIB_TIME,
        DrawingToolType::FibonacciCircles => &icons::FIB_CIRCLES,
        DrawingToolType::FibonacciSpiral => &icons::FIB_SPIRAL,
        DrawingToolType::FibonacciSpeedResistanceArcs => &icons::FIB_SPEED_RESISTANCE_ARCS,
        DrawingToolType::FibonacciWedge => &icons::FIB_WEDGE,
        DrawingToolType::Pitchfan => &icons::PITCHFAN,

        // ============ GANN (4 tools) ============
        DrawingToolType::GannBox => &icons::GANN_SQUARE,
        DrawingToolType::GannSquare => &icons::GANN_COMPLEX,
        DrawingToolType::GannFixed => &icons::GANN_FIXED,
        DrawingToolType::GannFan => &icons::GANN_FAN,

        // ============ PATTERNS (6 tools) ============
        DrawingToolType::XABCDPattern => &icons::XABCD_PATTERN,
        DrawingToolType::CypherPattern => &icons::CYPHER_PATTERN,
        DrawingToolType::HeadAndShoulders => &icons::HEAD_AND_SHOULDERS,
        DrawingToolType::ABCDPattern => &icons::ABCD,
        DrawingToolType::TrianglePattern => &icons::TRIANGLE_PATTERN,
        DrawingToolType::ThreeDrivesPattern => &icons::THREE_DRIVERS,

        // ============ ELLIOTT WAVES (5 tools) ============
        DrawingToolType::ElliottImpulse => &icons::ELLIOTT_IMPULSE,
        DrawingToolType::ElliottCorrection => &icons::ELLIOTT_CORRECTION,
        DrawingToolType::ElliottTriangle => &icons::ELLIOTT_TRIANGLE,
        DrawingToolType::ElliottDoubleCombo => &icons::ELLIOTT_DOUBLE_COMBO,
        DrawingToolType::ElliottTripleCombo => &icons::ELLIOTT_TRIPLE_COMBO,

        // ============ CYCLES (3 tools) ============
        DrawingToolType::CyclicLines => &icons::CIRCLE_LINES,
        DrawingToolType::TimeCycles => &icons::TIME_CYCLES,
        DrawingToolType::SineLine => &icons::SINE_LINE,

        // ============ PROJECTION (6 tools) ============
        DrawingToolType::LongPos => &icons::LONG_POSITION,
        DrawingToolType::ShortPos => &icons::SHORT_POSITION,
        DrawingToolType::Forecast => &icons::PREDICTION,
        DrawingToolType::BarsPattern => &icons::BARS_PATTERN,
        DrawingToolType::GhostFeed => &icons::GHOST_FEED,
        DrawingToolType::ProjectionTool => &icons::PROJECTION,

        // ============ VOLUME (3 tools) ============
        DrawingToolType::AnchoredVWAP => &icons::ANCHORED_VWAP,
        DrawingToolType::FixedRangeVolumeProfile => &icons::FIXED_RANGE_VOLUME_PROFILE,
        DrawingToolType::AnchoredVolumeProfile => &icons::ANCHORED_VOLUME_PROFILE,

        // ============ MEASURERS (4 tools) ============
        DrawingToolType::Measure => &icons::MEASURE,
        DrawingToolType::PriceRange => &icons::MEASURE,
        DrawingToolType::DateRange => &icons::DATE_RANGE,
        DrawingToolType::DateAndPriceRange => &icons::DATE_AND_PRICE_RANGE,

        // ============ BRUSHES (3 tools) ============
        DrawingToolType::Brush => &icons::BRUSH,
        DrawingToolType::Highlighter => &icons::HIGHLIGHTER,
        DrawingToolType::Paintbrush => &icons::PAINTBRUSH,

        // ============ ARROWS (4 tools) ============
        DrawingToolType::ArrowMarker => &icons::ARROW_MARKER,
        DrawingToolType::Arrow => &icons::ARROW_MARKER,
        DrawingToolType::ArrowMarkUp => &icons::ARROW_MARK_UP,
        DrawingToolType::ArrowMarkDown => &icons::ARROW_MARK_DOWN,

        // ============ SHAPES (10 tools) ============
        DrawingToolType::Rect => &icons::RECT,
        DrawingToolType::RotatedRect => &icons::ROTATED_RECT,
        DrawingToolType::Path => &icons::PATH,
        DrawingToolType::Circle => &icons::CIRCLE,
        DrawingToolType::Ellipse => &icons::ELLIPSE,
        DrawingToolType::Polyline => &icons::POLYLINE,
        DrawingToolType::Triangle => &icons::TRIANGLE,
        DrawingToolType::Arc => &icons::ARC,
        DrawingToolType::Curve => &icons::BEZIER_QUADRO,
        DrawingToolType::DoubleCurve => &icons::BEZIER_CUBIC,

        // ============ ANNOTATIONS (11 tools) ============
        DrawingToolType::TextLabel => &icons::TEXT,
        DrawingToolType::AnchoredText => &icons::ANCHORED_TEXT,
        DrawingToolType::Note => &icons::TEXT,
        DrawingToolType::PriceNote => &icons::TEXT, // Uses Note as fallback
        DrawingToolType::AnchoredNote => &icons::TEXT, // Uses Note as fallback
        DrawingToolType::Table => &icons::TEXT,     // Uses Note as fallback
        DrawingToolType::Callout => &icons::TEXT,   // Uses Note as fallback
        DrawingToolType::Comment => &icons::TEXT,   // Uses Note as fallback
        DrawingToolType::PriceLabel => &icons::TEXT, // Uses Text as fallback
        DrawingToolType::Signpost => &icons::TEXT,  // Uses Note as fallback
        DrawingToolType::FlagNote => &icons::TEXT,

        // ============ CONTENT (3 tools) ============
        DrawingToolType::Image => &icons::TEXT, // Uses Note as fallback
        DrawingToolType::Tweet => &icons::TEXT, // Uses Note as fallback
        DrawingToolType::Idea => &icons::IDEAS,

        // ============ ICONS (1 tool) ============
        DrawingToolType::FontIcon => &icons::EMOJI_ICON,
    }
}

/// Get the currently selected tool's icon for a category button
///
/// If no tool is selected, returns the default icon for that category.
pub fn get_sel_tool_icon_for_category(
    category: &str,
    sel_tool: Option<DrawingToolType>,
) -> &'static Icon {
    if let Some(tool) = sel_tool {
        // Check if the selected tool belongs to this category
        if tool_belongs_to_category(tool, category) {
            return get_icon(tool);
        }
    }
    // Return default category icon
    category_icon(category)
}

/// Check if a tool belongs to a given category
fn tool_belongs_to_category(tool: DrawingToolType, category: &str) -> bool {
    use crate::ui::drawing_toolbar::data::get_category_sections;

    let sections = get_category_sections(category);
    for (_, tools) in sections {
        if tools.contains(&tool) {
            return true;
        }
    }
    false
}
