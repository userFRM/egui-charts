//! Drawing tool identifiers and creation behavior.
//!
//! Use [`DrawingToolType`] to select a tool and inspect its category, interaction
//! mode and required points when composing a host picker.

mod behavior;
mod categories;
mod display;
mod interaction_mode;

use serde::{Deserialize, Serialize};

pub use display::ParseToolTypeError;
pub use interaction_mode::DrawingInteractionMode;

/// Enumeration of all drawing tool types available in the charting engine.
///
/// Identifies a drawing's rendering behavior, interaction mode, required point
/// count and toolbar placement. Use [`categories`](Self::categories) and
/// [`by_category`](Self::by_category) to build a categorized picker.
///
/// # Interaction modes
///
/// [`interaction_mode`](Self::interaction_mode) describes how a tool captures
/// points; [`required_points`](Self::required_points) gives its completion count,
/// or `None` for tools without a fixed count.
///
/// # Parsing
///
/// `DrawingToolType` implements [`FromStr`](std::str::FromStr) with flexible
/// case-insensitive matching, including common abbreviations:
///
/// ```
/// # use egui_charts::config::DrawingToolType;
/// let tool: DrawingToolType = "fib".parse().unwrap();
/// assert_eq!(tool, DrawingToolType::FibonacciRetracement);
/// ```
///
/// Serde writes and reads the variant name (`"FibonacciRetracement"`), which
/// stays stable when display names change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DrawingToolType {
    // ============ CURSORS (4 tools) ============
    /// Crosshair cursor overlay -- not a drawing, provides precision targeting.
    CrossCursor,
    /// Dot cursor overlay -- minimal cursor for clean chart views.
    DotCursor,
    /// Standard arrow/pointer cursor.
    ArrowCursor,
    /// Eraser tool -- click on drawings to delete them.
    Eraser,

    // ============ LINES (9 tools) ============
    /// A line segment between two points (click-click interaction).
    TrendLine,
    /// A ray starting at a point and extending infinitely in one direction.
    Ray,
    /// An info line showing distance, angle, and bar count between two points.
    InfoLine,
    /// A line that extends infinitely in both directions through two points.
    ExtendedLine,
    /// A horizontal line at a specific price level, extending across the chart.
    HorizontalLine,
    /// A horizontal ray extending from a point to the right edge of the chart.
    HorizontalRay,
    /// A vertical line at a specific bar/time, extending top to bottom.
    VerticalLine,
    /// Combined horizontal + vertical crosshair lines at a single point.
    CrossLine,
    /// A trend line that displays the angle of the line in degrees.
    TrendAngle,

    // ============ CHANNELS (4 tools) ============
    /// Two parallel lines defining a price channel (3-point: two on baseline, one for width).
    ParallelChannel,
    /// Linear regression channel with standard deviation bands.
    RegressionTrend,
    /// A trend line through the first two anchors and a horizontal line at the third anchor's price.
    FlatTopBottom,
    /// Two parallel trend lines: the first two anchors set the first line, and the third anchor sets
    /// where the second line starts, at any time.
    DisjointChannel,

    // ============ PITCHFORKS (4 tools) ============
    /// Andrews' Pitchfork -- median line with parallel support/resistance (3-point).
    Pitchfork,
    /// Schiff Pitchfork variant with adjusted median line start point.
    SchiffPitchfork,
    /// Modified Schiff Pitchfork with further median line adjustment.
    ModifiedSchiffPitchfork,
    /// Inside Pitchfork -- median line drawn inside the price action.
    InsidePitchfork,

    // ============ FIBONACCI (11 tools) ============
    /// Fibonacci retracement levels between two price points (drag-to-draw).
    FibonacciRetracement,
    /// Trend-based Fibonacci extension projecting levels beyond the measured move.
    FibonacciExtension,
    /// Fibonacci channel with parallel lines at Fibonacci ratios.
    FibonacciChannel,
    /// Vertical time zones at Fibonacci number intervals from a start point.
    FibonacciTimeZones,
    /// Fibonacci speed/resistance fan with diagonal lines at Fibonacci angles.
    FibonacciSpeedFan,
    /// Trend-based Fibonacci time projections using price swing timing.
    TrendBasedFibTime,
    /// Concentric circles at Fibonacci ratio radii.
    FibonacciCircles,
    /// Golden spiral (logarithmic spiral based on the golden ratio).
    FibonacciSpiral,
    /// Arcs at Fibonacci speed/resistance levels.
    FibonacciSpeedResistanceArcs,
    /// Fibonacci wedge -- converging lines at Fibonacci angles.
    FibonacciWedge,
    /// Pitch fan -- fan lines at specific angle ratios from a pivot point.
    Pitchfan,

    // ============ GANN (4 tools) ============
    /// Gann Box -- a grid of price/time squares using Gann ratios.
    GannBox,
    /// Gann Square -- W.D. Gann's square of nine overlay.
    GannSquare,
    /// Gann Square Fixed -- fixed-size Gann grid.
    GannFixed,
    /// Gann Fan -- fan lines at standard Gann angles (1x1, 1x2, 2x1, etc.).
    GannFan,

    // ============ PATTERNS (6 tools) ============
    /// XABCD harmonic pattern (5-point: Gartley, Butterfly, Bat, Crab, etc.).
    XABCDPattern,
    /// Cypher harmonic pattern (5-point variant with specific ratio rules).
    CypherPattern,
    /// Head and Shoulders reversal pattern (7-point).
    HeadAndShoulders,
    /// ABCD harmonic pattern (4-point geometric price pattern).
    ABCDPattern,
    /// Triangle chart pattern (ascending, descending, or symmetrical; 7-point).
    TrianglePattern,
    /// Three Drives harmonic pattern (6-point repeating impulse pattern).
    ThreeDrivesPattern,

    // ============ ELLIOTT WAVES (5 tools) ============
    /// Elliott impulse wave labels (1-2-3-4-5, 6 points).
    ElliottImpulse,
    /// Elliott correction wave labels (A-B-C, 4 points).
    ElliottCorrection,
    /// Elliott triangle wave labels (A-B-C-D-E, 5 points).
    ElliottTriangle,
    /// Elliott double combination wave labels (W-X-Y, 5 points).
    ElliottDoubleCombo,
    /// Elliott triple combination wave labels (W-X-Y-X-Z, 7 points).
    ElliottTripleCombo,

    // ============ CYCLES (3 tools) ============
    /// Evenly-spaced vertical lines highlighting cyclic time intervals.
    CyclicLines,
    /// Circular time cycle arcs projected from a start point.
    TimeCycles,
    /// Sine wave curve between two points, for cyclical analysis.
    SineLine,

    // ============ PROJECTION (6 tools) ============
    /// Long position risk/reward tool showing entry, stop loss, and take profit.
    LongPos,
    /// Short position risk/reward tool showing entry, stop loss, and take profit.
    ShortPos,
    /// Forecast/prediction zone projected from two points.
    Forecast,
    /// Bars pattern -- copy and project a price pattern forward in time.
    BarsPattern,
    /// Ghost feed -- translucent historical price overlay for comparison.
    GhostFeed,
    /// Generic projection tool for price/time forecasting.
    ProjectionTool,

    // ============ VOLUME-BASED (3 tools) ============
    /// Anchored VWAP (Volume-Weighted Average Price) from a user-selected bar.
    AnchoredVWAP,
    /// Fixed-range volume profile histogram between two time points.
    FixedRangeVolumeProfile,
    /// Anchored volume profile starting from a user-selected bar.
    AnchoredVolumeProfile,

    // ============ MEASURERS (4 tools) ============
    /// Ruler/measure tool showing distance, bars, and percentage between two points.
    Measure,
    /// Price range highlight (vertical span at a price zone).
    PriceRange,
    /// Date range highlight (horizontal span across a time period).
    DateRange,
    /// Combined date-and-price range rectangle.
    DateAndPriceRange,

    // ============ BRUSHES (2 tools) ============
    /// Freehand drawing brush (continuous-draw interaction). Drawings saved with the former
    /// `Paintbrush` tool, which drew the same, load as brushes.
    #[serde(alias = "Paintbrush")]
    Brush,
    /// Semi-transparent highlighter brush for marking chart regions.
    Highlighter,

    // ============ ARROWS (6 tools) ============
    /// Two-point directional arrow marker.
    ArrowMarker,
    /// Two-point arrow with customizable head styles.
    Arrow,
    /// Upward arrow marker (single-click, bullish signal).
    ArrowMarkUp,
    /// Downward arrow marker (single-click, bearish signal).
    ArrowMarkDown,

    /// Leftward arrow mark placed with one click at its tip.
    ArrowMarkLeft,
    /// Rightward arrow mark placed with one click at its tip.
    ArrowMarkRight,

    // ============ SHAPES (11 tools) ============
    /// Axis-aligned rectangle defined by two corner points.
    Rect,
    /// Rectangle that can be rotated to any angle.
    RotatedRect,
    /// Multi-segment path ending in an arrowhead.
    Path,
    /// Circle defined by center and edge point.
    Circle,
    /// Ellipse defined by bounding rectangle corners.
    Ellipse,
    /// Open polyline with unlimited points (double-click to finish).
    Polyline,
    /// Triangle defined by three corner points.
    Triangle,
    /// Circular sector: center, radius endpoint, then end ray, swept clockwise on screen.
    Sector,
    /// Circular arc between two points with adjustable curvature.
    Arc,
    /// Quadratic Bezier curve (3 control points: start, control, end).
    Curve,
    /// Cubic Bezier curve (4 control points).
    DoubleCurve,

    // ============ ANNOTATIONS (11 tools) ============
    /// Simple text label placed on the chart.
    TextLabel,
    /// Text label anchored to screen position (doesn't move with pan).
    AnchoredText,
    /// Sticky note annotation with background.
    Note,
    /// Price note -- text annotation anchored to a specific price level.
    PriceNote,
    /// Anchored/pinned note that stays at a fixed screen position.
    AnchoredNote,
    /// Data table overlay on the chart.
    Table,
    /// Callout with arrow pointing to a chart location.
    Callout,
    /// Comment bubble annotation.
    Comment,
    /// Price label badge shown on the price axis.
    PriceLabel,
    /// Signpost marker with directional indicator.
    Signpost,
    /// Flag/mark note at a specific chart location.
    FlagNote,

    // ============ CONTENT (3 tools - placeholders) ============
    /// Image embed identifier; built-in rendering draws a frame without loading an image.
    Image,
    /// Social post embed identifier; built-in rendering draws a card without fetching content.
    Tweet,
    /// Idea embed identifier; built-in rendering draws a badge without fetching content.
    Idea,

    // ============ ICONS (2 tools) ============
    /// Bundled vector icon or Unicode emoji placed at a single point.
    FontIcon,
    /// Bundled illustrated sticker placed with one click, sized by `Drawing::font_size`.
    Sticker,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_detection() {
        assert!(DrawingToolType::CrossCursor.is_cursor());
        assert!(DrawingToolType::Eraser.is_cursor());
        assert!(!DrawingToolType::TrendLine.is_cursor());
        assert!(!DrawingToolType::FibonacciRetracement.is_cursor());
    }

    #[test]
    fn test_from_str() {
        assert_eq!(
            "trendline".parse::<DrawingToolType>().unwrap(),
            DrawingToolType::TrendLine
        );
        assert_eq!(
            "fib".parse::<DrawingToolType>().unwrap(),
            DrawingToolType::FibonacciRetracement
        );
        let unparsed: Vec<_> = DrawingToolType::all()
            .iter()
            .filter(|tool| {
                let name = tool.as_str();
                name.parse() != Ok(**tool) || name.to_uppercase().parse() != Ok(**tool)
            })
            .map(|tool| tool.as_str())
            .collect();
        assert!(unparsed.is_empty(), "{unparsed:?}");
    }

    #[test]
    fn test_interaction_modes() {
        assert_eq!(
            DrawingToolType::CrossCursor.interaction_mode(),
            DrawingInteractionMode::NotDrawing
        );
        assert_eq!(
            DrawingToolType::HorizontalLine.interaction_mode(),
            DrawingInteractionMode::SingleClick
        );
        assert_eq!(
            DrawingToolType::TrendLine.interaction_mode(),
            DrawingInteractionMode::ClickClick
        );
        assert_eq!(
            DrawingToolType::Rect.interaction_mode(),
            DrawingInteractionMode::DragToDraw
        );
        assert_eq!(
            DrawingToolType::Brush.interaction_mode(),
            DrawingInteractionMode::ContinuousDraw
        );
        assert_eq!(
            DrawingToolType::Polyline.interaction_mode(),
            DrawingInteractionMode::MultiPoint
        );
    }
}
