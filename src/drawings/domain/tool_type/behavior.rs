//! Behavioral queries for [`DrawingToolType`].
//!
//! Provides methods that describe *how* each tool type behaves: whether it is
//! a cursor, what interaction mode it uses, and how many points it requires.

use super::{DrawingInteractionMode, DrawingToolType};

impl DrawingToolType {
    /// Returns `true` if this tool is a cursor/utility tool rather than a
    /// drawing tool.
    ///
    /// Cursor tools (`CrossCursor`, `DotCursor`, `ArrowCursor`, `Eraser`)
    /// do not create [`Drawing`](crate::drawings::Drawing) objects. They modify
    /// the cursor appearance or provide utility functions (e.g., erasing).
    #[inline]
    pub fn is_cursor(&self) -> bool {
        matches!(
            self,
            Self::CrossCursor | Self::DotCursor | Self::ArrowCursor | Self::Eraser
        )
    }

    /// Returns the [`DrawingInteractionMode`] that governs how mouse input is
    /// captured when creating a drawing of this type.
    ///
    /// See [`DrawingInteractionMode`] for details on each mode.
    pub fn interaction_mode(&self) -> DrawingInteractionMode {
        match self {
            // Cursors - not drawing tools
            Self::CrossCursor | Self::DotCursor | Self::ArrowCursor | Self::Eraser => {
                DrawingInteractionMode::NotDrawing
            }

            // Single-click tools
            Self::HorizontalLine
            | Self::VerticalLine
            | Self::CrossLine
            | Self::HorizontalRay
            | Self::TextLabel
            | Self::AnchoredText
            | Self::Note
            | Self::AnchoredNote
            | Self::PriceLabel
            | Self::FlagNote
            | Self::ArrowMarker
            | Self::ArrowMarkUp
            | Self::ArrowMarkDown
            | Self::AnchoredVWAP
            | Self::PriceNote
            | Self::Table
            | Self::Comment
            | Self::Signpost
            | Self::FontIcon
            | Self::Image
            | Self::Tweet
            | Self::Idea => DrawingInteractionMode::SingleClick,

            // Drag-to-draw tools
            Self::Rect
            | Self::RotatedRect
            | Self::Circle
            | Self::Ellipse
            | Self::Arc
            | Self::Measure
            | Self::PriceRange
            | Self::DateRange
            | Self::DateAndPriceRange
            | Self::GannBox
            | Self::GannSquare
            | Self::GannFixed
            | Self::LongPos
            | Self::ShortPos
            | Self::Forecast
            | Self::FixedRangeVolumeProfile
            | Self::AnchoredVolumeProfile
            | Self::FibonacciRetracement
            | Self::FibonacciExtension
            | Self::FibonacciSpeedResistanceArcs
            | Self::FibonacciTimeZones
            | Self::FibonacciCircles
            | Self::FibonacciSpeedFan
            | Self::FibonacciSpiral
            | Self::FibonacciWedge
            | Self::TrendBasedFibTime
            | Self::Pitchfan
            | Self::GannFan
            | Self::RegressionTrend
            | Self::CyclicLines
            | Self::TimeCycles
            | Self::SineLine
            | Self::Curve
            | Self::DoubleCurve
            | Self::Callout
            | Self::Arrow
            | Self::ProjectionTool => DrawingInteractionMode::DragToDraw,

            // Continuous drawing tools
            Self::Brush | Self::Highlighter | Self::Paintbrush => {
                DrawingInteractionMode::ContinuousDraw
            }

            // Multi-point tools
            Self::Polyline
            | Self::Path
            | Self::Triangle
            | Self::ParallelChannel
            | Self::FlatTopBottom
            | Self::DisjointChannel
            | Self::FibonacciChannel
            | Self::Pitchfork
            | Self::SchiffPitchfork
            | Self::ModifiedSchiffPitchfork
            | Self::InsidePitchfork
            | Self::XABCDPattern
            | Self::CypherPattern
            | Self::HeadAndShoulders
            | Self::ABCDPattern
            | Self::TrianglePattern
            | Self::ThreeDrivesPattern
            | Self::ElliottImpulse
            | Self::ElliottCorrection
            | Self::ElliottTriangle
            | Self::ElliottDoubleCombo
            | Self::ElliottTripleCombo
            | Self::BarsPattern
            | Self::GhostFeed => DrawingInteractionMode::MultiPoint,

            // Click-click tools
            Self::TrendLine
            | Self::Ray
            | Self::InfoLine
            | Self::ExtendedLine
            | Self::TrendAngle => DrawingInteractionMode::ClickClick,
        }
    }

    /// Returns the number of [`ChartPoint`](crate::drawings::ChartPoint)s
    /// required to complete a drawing of this type, or `None` for tools with
    /// no fixed point count (`Polyline`, `Brush`, cursors).
    ///
    /// | Mode | Typical count |
    /// |---|---|
    /// | `SingleClick` | 1 |
    /// | `ClickClick` / `DragToDraw` | 2 |
    /// | `MultiPoint` | 3--7 depending on tool |
    /// | `ContinuousDraw` | `None` (unlimited) |
    /// | `NotDrawing` | `None` |
    pub fn required_points(&self) -> Option<usize> {
        match self.interaction_mode() {
            DrawingInteractionMode::SingleClick => Some(1),
            DrawingInteractionMode::ClickClick | DrawingInteractionMode::DragToDraw => Some(2),
            DrawingInteractionMode::MultiPoint => {
                // Multi-point tools have varying requirements
                match self {
                    Self::Triangle
                    | Self::ParallelChannel
                    | Self::Pitchfork
                    | Self::SchiffPitchfork
                    | Self::ModifiedSchiffPitchfork
                    | Self::InsidePitchfork => Some(3),
                    Self::ABCDPattern | Self::ElliottCorrection => Some(4),
                    Self::XABCDPattern | Self::CypherPattern | Self::ElliottTriangle => Some(5),
                    Self::ElliottImpulse | Self::ThreeDrivesPattern => Some(6),
                    Self::HeadAndShoulders | Self::TrianglePattern | Self::ElliottTripleCombo => {
                        Some(7)
                    }
                    Self::ElliottDoubleCombo => Some(5),
                    // Polyline, Path have no fixed limit
                    _ => None,
                }
            }
            DrawingInteractionMode::ContinuousDraw => None,
            DrawingInteractionMode::NotDrawing => None,
        }
    }
}
