//! Toolbar groupings for drawing tools.
//!
//! [`CATEGORY_ORDER`] and [`get_category_sections`] define the toolbar's grouped
//! menus. [`DrawingToolType::categories`] and [`DrawingToolType::by_category`]
//! provide finer groups for command lists. They group the same tools differently,
//! and [`DrawingToolType::Measure`] is a standalone toolbar button outside the
//! toolbar groups.

use crate::drawings::DrawingToolType;

/// Category display order in the toolbar
pub const CATEGORY_ORDER: &[&str] = &[
    "Cursors",
    "Lines",
    "Fibonacci",
    "Patterns",
    "Projection",
    "Brushes/Shapes",
    "Text/Annotations",
    "Icons/Emojis",
];

/// Get the category that a tool belongs to
///
/// Returns the main category name, not the sub-section.
/// [`DrawingToolType::Measure`] is a standalone toolbar button and returns `None`.
pub fn get_tool_category(tool: DrawingToolType) -> Option<&'static str> {
    for &category in CATEGORY_ORDER {
        let sections = get_category_sections(category);
        for (_, tools) in sections {
            if tools.contains(&tool) {
                return Some(category);
            }
        }
    }
    None
}

/// Get sections and tools for a category
///
/// Returns a vector of (section_name, tools) tuples for the given category,
/// or an empty vector for an unknown name. Names are case-sensitive.
pub fn get_category_sections(category: &str) -> Vec<(&'static str, Vec<DrawingToolType>)> {
    match category {
        "Cursors" => vec![(
            "CURSORS",
            vec![
                DrawingToolType::CrossCursor,
                DrawingToolType::DotCursor,
                DrawingToolType::ArrowCursor,
                DrawingToolType::Eraser,
            ],
        )],
        "Lines" => vec![
            (
                "LINES",
                vec![
                    DrawingToolType::TrendLine,
                    DrawingToolType::Ray,
                    DrawingToolType::InfoLine,
                    DrawingToolType::ExtendedLine,
                    DrawingToolType::TrendAngle,
                    DrawingToolType::HorizontalLine,
                    DrawingToolType::HorizontalRay,
                    DrawingToolType::VerticalLine,
                    DrawingToolType::CrossLine,
                ],
            ),
            (
                "CHANNELS",
                vec![
                    DrawingToolType::ParallelChannel,
                    DrawingToolType::RegressionTrend,
                    DrawingToolType::FlatTopBottom,
                    DrawingToolType::DisjointChannel,
                ],
            ),
            (
                "PITCHFORKS",
                vec![
                    DrawingToolType::Pitchfork,
                    DrawingToolType::SchiffPitchfork,
                    DrawingToolType::ModifiedSchiffPitchfork,
                    DrawingToolType::InsidePitchfork,
                ],
            ),
        ],
        "Fibonacci" => vec![
            (
                "FIBONACCI",
                vec![
                    DrawingToolType::FibonacciRetracement,
                    DrawingToolType::FibonacciExtension,
                    DrawingToolType::FibonacciChannel,
                    DrawingToolType::FibonacciTimeZones,
                    DrawingToolType::FibonacciSpeedFan,
                    DrawingToolType::TrendBasedFibTime,
                    DrawingToolType::FibonacciCircles,
                    DrawingToolType::FibonacciSpiral,
                    DrawingToolType::FibonacciSpeedResistanceArcs,
                    DrawingToolType::FibonacciWedge,
                    DrawingToolType::Pitchfan,
                ],
            ),
            (
                "GANN",
                vec![
                    DrawingToolType::GannBox,
                    DrawingToolType::GannFixed,
                    DrawingToolType::GannSquare,
                    DrawingToolType::GannFan,
                ],
            ),
        ],
        "Patterns" => vec![
            (
                "PATTERNS",
                vec![
                    DrawingToolType::XABCDPattern,
                    DrawingToolType::CypherPattern,
                    DrawingToolType::HeadAndShoulders,
                    DrawingToolType::ABCDPattern,
                    DrawingToolType::TrianglePattern,
                    DrawingToolType::ThreeDrivesPattern,
                ],
            ),
            (
                "ELLIOTT WAVES",
                vec![
                    DrawingToolType::ElliottImpulse,
                    DrawingToolType::ElliottCorrection,
                    DrawingToolType::ElliottTriangle,
                    DrawingToolType::ElliottDoubleCombo,
                    DrawingToolType::ElliottTripleCombo,
                ],
            ),
            (
                "CYCLES",
                vec![
                    DrawingToolType::CyclicLines,
                    DrawingToolType::TimeCycles,
                    DrawingToolType::SineLine,
                ],
            ),
        ],
        "Projection" => vec![
            (
                "PROJECTION",
                vec![
                    DrawingToolType::LongPos,
                    DrawingToolType::ShortPos,
                    DrawingToolType::Forecast,
                    DrawingToolType::BarsPattern,
                    DrawingToolType::GhostFeed,
                    DrawingToolType::ProjectionTool,
                ],
            ),
            (
                "VOLUME-BASED",
                vec![
                    DrawingToolType::AnchoredVWAP,
                    DrawingToolType::FixedRangeVolumeProfile,
                    DrawingToolType::AnchoredVolumeProfile,
                ],
            ),
            (
                "MEASURER",
                vec![
                    DrawingToolType::PriceRange,
                    DrawingToolType::DateRange,
                    DrawingToolType::DateAndPriceRange,
                ],
            ),
        ],
        "Brushes/Shapes" => vec![
            (
                "BRUSHES",
                vec![DrawingToolType::Brush, DrawingToolType::Highlighter],
            ),
            (
                "ARROWS",
                vec![
                    DrawingToolType::ArrowMarker,
                    DrawingToolType::Arrow,
                    DrawingToolType::ArrowMarkUp,
                    DrawingToolType::ArrowMarkDown,
                    DrawingToolType::ArrowMarkLeft,
                    DrawingToolType::ArrowMarkRight,
                ],
            ),
            (
                "SHAPES",
                vec![
                    DrawingToolType::Rect,
                    DrawingToolType::RotatedRect,
                    DrawingToolType::Path,
                    DrawingToolType::Circle,
                    DrawingToolType::Ellipse,
                    DrawingToolType::Polyline,
                    DrawingToolType::Triangle,
                    DrawingToolType::Arc,
                    DrawingToolType::Sector,
                    DrawingToolType::Curve,
                    DrawingToolType::DoubleCurve,
                ],
            ),
        ],
        "Text/Annotations" => vec![
            (
                "TEXT & NOTES",
                vec![
                    DrawingToolType::TextLabel,
                    DrawingToolType::AnchoredText,
                    DrawingToolType::Note,
                    DrawingToolType::PriceNote,
                    DrawingToolType::AnchoredNote,
                    DrawingToolType::Table,
                    DrawingToolType::Callout,
                    DrawingToolType::Comment,
                    DrawingToolType::PriceLabel,
                    DrawingToolType::Signpost,
                    DrawingToolType::FlagNote,
                ],
            ),
            (
                "CONTENT",
                vec![
                    DrawingToolType::Image,
                    DrawingToolType::Tweet,
                    DrawingToolType::Idea,
                ],
            ),
        ],
        "Icons/Emojis" => vec![(
            "ICONS",
            vec![DrawingToolType::FontIcon, DrawingToolType::Sticker],
        )],
        _ => vec![],
    }
}

/// Get the default tool for a category
///
/// Returns `None` for an unknown name. Names are case-sensitive.
pub fn get_category_default_tool(category: &str) -> Option<DrawingToolType> {
    Some(match category {
        "Cursors" => DrawingToolType::CrossCursor,
        "Lines" => DrawingToolType::TrendLine,
        "Fibonacci" => DrawingToolType::FibonacciRetracement,
        "Patterns" => DrawingToolType::XABCDPattern,
        "Projection" => DrawingToolType::LongPos,
        "Brushes/Shapes" => DrawingToolType::Brush,
        "Text/Annotations" => DrawingToolType::TextLabel,
        "Icons/Emojis" => DrawingToolType::FontIcon,
        _ => return None,
    })
}

/// Get all tools in a flat list
pub fn get_all_tools() -> Vec<DrawingToolType> {
    DrawingToolType::all().to_vec()
}

/// Count of all tools
pub fn tool_cnt() -> usize {
    DrawingToolType::all().len()
}
