//! Display, parsing, and identification methods for [`DrawingToolType`].
//!
//! Implements [`Display`](std::fmt::Display) and [`FromStr`](std::str::FromStr)
//! for human-readable names and flexible case-insensitive parsing, plus methods
//! for data-name identifiers (for protocol/storage compatibility) and SVG icon
//! paths (for toolbar rendering).

use std::fmt;
use std::str::FromStr;

use super::DrawingToolType;

impl fmt::Display for DrawingToolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// The error [`DrawingToolType`]'s [`FromStr`] returns for a name that is not a
/// drawing tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseToolTypeError {
    /// The name that did not parse, as given.
    pub input: String,
}

impl fmt::Display for ParseToolTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown drawing tool {:?}", self.input)
    }
}

impl std::error::Error for ParseToolTypeError {}

impl FromStr for DrawingToolType {
    type Err = ParseToolTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let name = s.to_lowercase();
        if let Some(tool) = Self::all()
            .iter()
            .find(|tool| tool.as_str().eq_ignore_ascii_case(&name))
        {
            return Ok(*tool);
        }
        // Names other than the display names.
        match name.as_str() {
            // Cursors
            "crosscursor" | "crosshair" | "cursor" => Ok(Self::CrossCursor),
            "dotcursor" | "dot" => Ok(Self::DotCursor),
            "arrowcursor" | "pointer" => Ok(Self::ArrowCursor),
            "erase" => Ok(Self::Eraser),

            // Lines
            "trendline" | "trend" => Ok(Self::TrendLine),
            "infoline" | "info" => Ok(Self::InfoLine),
            "extendedline" | "extended" => Ok(Self::ExtendedLine),
            "horizontalline" | "horizontal" | "hline" => Ok(Self::HorizontalLine),
            "horizontalray" | "hray" => Ok(Self::HorizontalRay),
            "verticalline" | "vertical" | "vline" => Ok(Self::VerticalLine),
            "crossline" | "cross" => Ok(Self::CrossLine),
            "trendangle" | "angle" => Ok(Self::TrendAngle),

            // Channels
            "parallelchannel" | "channel" => Ok(Self::ParallelChannel),
            "regressiontrend" | "regression" | "linear regression" => Ok(Self::RegressionTrend),
            "flattopbottom" | "flat top bottom" | "flat channel" => Ok(Self::FlatTopBottom),
            "disjointchannel" => Ok(Self::DisjointChannel),

            // Pitchforks
            "andrews pitchfork" | "andrews" => Ok(Self::Pitchfork),
            "schiffpitchfork" | "schiff" => Ok(Self::SchiffPitchfork),
            "modifiedschiffpitchfork" | "modified schiff" => Ok(Self::ModifiedSchiffPitchfork),
            "insidepitchfork" | "inside" => Ok(Self::InsidePitchfork),

            // Fibonacci
            "fibonacci" | "fib" | "fibonacci retracement" => Ok(Self::FibonacciRetracement),
            "fibonacciextension" | "fibonacci extension" | "fib extension" | "fib ext" => {
                Ok(Self::FibonacciExtension)
            }
            "fibonaccichannel" | "fibonacci channel" => Ok(Self::FibonacciChannel),
            "fibonaccitimezones" | "fibonacci time zones" | "fib time" | "fib time zones" => {
                Ok(Self::FibonacciTimeZones)
            }
            "fibonaccispeedfan" | "fibonacci speed fan" | "fib speed fan" | "speed fan" => {
                Ok(Self::FibonacciSpeedFan)
            }
            "trendbasedfibtime" | "trend based fib time" | "fib trend time" => {
                Ok(Self::TrendBasedFibTime)
            }
            "fibonaccicircles" | "fibonacci circles" => Ok(Self::FibonacciCircles),
            "fibonaccispiral" | "fibonacci spiral" | "spiral" => Ok(Self::FibonacciSpiral),
            "fibonaccispeedresistancearcs" | "fibonacci speed resistance arcs" | "fib arcs" => {
                Ok(Self::FibonacciSpeedResistanceArcs)
            }
            "fibonacciwedge" | "fibonacci wedge" => Ok(Self::FibonacciWedge),
            "pitch fan" => Ok(Self::Pitchfan),

            // Gann
            "gannbox" => Ok(Self::GannBox),
            "gannsquare" => Ok(Self::GannSquare),
            "gannfixed" | "gann fixed" => Ok(Self::GannFixed),
            "gannfan" | "gann" => Ok(Self::GannFan),

            // Patterns
            "xabcdpattern" | "xabcd" => Ok(Self::XABCDPattern),
            "cypherpattern" | "cypher" => Ok(Self::CypherPattern),
            "headandshoulders" | "h&s" | "head shoulders" => Ok(Self::HeadAndShoulders),
            "abcdpattern" | "abcd" => Ok(Self::ABCDPattern),
            "trianglepattern" => Ok(Self::TrianglePattern),
            "threedrivespattern" | "three drives" => Ok(Self::ThreeDrivesPattern),

            // Elliott Wave
            "elliottimpulse" | "elliott impulse" | "impulse wave" | "impulse" => {
                Ok(Self::ElliottImpulse)
            }
            "elliottcorrection" | "elliott correction" | "correction wave" | "abc" => {
                Ok(Self::ElliottCorrection)
            }
            "elliotttriangle" | "elliott triangle" | "triangle wave" => Ok(Self::ElliottTriangle),
            "elliottdoublecombo" | "elliott double combo" | "wxy" => Ok(Self::ElliottDoubleCombo),
            "elliotttriplecombo" | "elliott triple combo" | "wxyxz" => Ok(Self::ElliottTripleCombo),

            // Cycles
            "cycliclines" | "cyclic" => Ok(Self::CyclicLines),
            "timecycles" => Ok(Self::TimeCycles),
            "sineline" | "sine" => Ok(Self::SineLine),

            // Projection
            "longposition" | "long" => Ok(Self::LongPos),
            "shortposition" | "short" => Ok(Self::ShortPos),
            "barspattern" => Ok(Self::BarsPattern),
            "ghostfeed" => Ok(Self::GhostFeed),
            "projectiontool" | "projection tool" => Ok(Self::ProjectionTool),

            // Volume Tools
            "anchoredvwap" | "vwap" => Ok(Self::AnchoredVWAP),
            "fixedrangevolumeprofile" | "volume profile" => Ok(Self::FixedRangeVolumeProfile),
            "anchoredvolumeprofile" => Ok(Self::AnchoredVolumeProfile),

            // Measurers
            "ruler" => Ok(Self::Measure),
            "pricerange" => Ok(Self::PriceRange),
            "daterange" | "time range" => Ok(Self::DateRange),
            "dateandpricerange" => Ok(Self::DateAndPriceRange),

            // Brushes
            "freehand" | "paintbrush" | "paint brush" | "paint" => Ok(Self::Brush),
            "highlight" => Ok(Self::Highlighter),

            // Arrows
            "arrowmarker" => Ok(Self::ArrowMarker),
            "arrowmarkup" | "arrow up" => Ok(Self::ArrowMarkUp),
            "arrowmarkdown" | "arrow down" => Ok(Self::ArrowMarkDown),
            "arrowmarkleft" | "arrow left" => Ok(Self::ArrowMarkLeft),
            "arrowmarkright" | "arrow right" => Ok(Self::ArrowMarkRight),

            // Shapes
            "rect" => Ok(Self::Rect),
            "rotatedrect" | "rotated rect" => Ok(Self::RotatedRect),
            "multipath" => Ok(Self::Path),
            "oval" => Ok(Self::Ellipse),
            "poly line" | "multiline" => Ok(Self::Polyline),
            "bezierquadro" | "bezier quadro" => Ok(Self::Curve),
            "doublecurve" | "beziercubic" | "bezier cubic" => Ok(Self::DoubleCurve),

            // Annotations
            "textlabel" | "text label" | "label" => Ok(Self::TextLabel),
            "anchoredtext" => Ok(Self::AnchoredText),
            "textnote" | "text note" => Ok(Self::Note),
            "pricenote" => Ok(Self::PriceNote),
            "anchorednote" | "anchored note" => Ok(Self::AnchoredNote),
            "pricelabel" => Ok(Self::PriceLabel),
            "flagnote" | "flag note" | "flag" | "flagmark" => Ok(Self::FlagNote),

            // Content
            "img" => Ok(Self::Image),
            "twitter" => Ok(Self::Tweet),

            // Icons
            "fonticon" | "font icon" | "emoji" => Ok(Self::FontIcon),

            _ => Err(ParseToolTypeError {
                input: s.to_string(),
            }),
        }
    }
}

impl DrawingToolType {
    /// Returns a human-readable display name for the tool (e.g., `"Trend Line"`,
    /// `"Fib Retracement"`).
    ///
    /// This is the string shown in the toolbar UI and in the [`Display`](std::fmt::Display)
    /// implementation. Each tool has its own name, and [`FromStr`] parses it back
    /// to the same tool, ignoring case.
    pub fn as_str(&self) -> &str {
        match self {
            // Cursors
            Self::CrossCursor => "Cross Cursor",
            Self::DotCursor => "Dot Cursor",
            Self::ArrowCursor => "Arrow Cursor",
            Self::Eraser => "Eraser",

            // Lines
            Self::TrendLine => "Trend Line",
            Self::Ray => "Ray",
            Self::InfoLine => "Info Line",
            Self::ExtendedLine => "Extended Line",
            Self::HorizontalLine => "Horizontal Line",
            Self::HorizontalRay => "Horizontal Ray",
            Self::VerticalLine => "Vertical Line",
            Self::CrossLine => "Cross Line",
            Self::TrendAngle => "Trend Angle",

            // Channels
            Self::ParallelChannel => "Parallel Channel",
            Self::RegressionTrend => "Regression Trend",
            Self::FlatTopBottom => "Flat Top/Bottom",
            Self::DisjointChannel => "Disjoint Channel",

            // Pitchforks
            Self::Pitchfork => "Pitchfork",
            Self::SchiffPitchfork => "Schiff Pitchfork",
            Self::ModifiedSchiffPitchfork => "Modified Schiff Pitchfork",
            Self::InsidePitchfork => "Inside Pitchfork",

            // Fibonacci
            Self::FibonacciRetracement => "Fib Retracement",
            Self::FibonacciExtension => "Trend-Based Fib Extension",
            Self::FibonacciChannel => "Fib Channel",
            Self::FibonacciTimeZones => "Fib Time Zone",
            Self::FibonacciSpeedFan => "Fib Speed Resistance Fan",
            Self::TrendBasedFibTime => "Trend-Based Fib Time",
            Self::FibonacciCircles => "Fib Circles",
            Self::FibonacciSpiral => "Fib Spiral",
            Self::FibonacciSpeedResistanceArcs => "Fib Speed Resistance Arcs",
            Self::FibonacciWedge => "Fib Wedge",
            Self::Pitchfan => "Pitchfan",

            // Gann
            Self::GannBox => "Gann Box",
            Self::GannSquare => "Gann Square",
            Self::GannFixed => "Gann Square Fixed",
            Self::GannFan => "Gann Fan",

            // Patterns
            Self::XABCDPattern => "XABCD Pattern",
            Self::CypherPattern => "Cypher Pattern",
            Self::HeadAndShoulders => "Head and Shoulders",
            Self::ABCDPattern => "ABCD Pattern",
            Self::TrianglePattern => "Triangle Pattern",
            Self::ThreeDrivesPattern => "Three Drives Pattern",

            // Elliott Wave
            Self::ElliottImpulse => "Elliott Impulse Wave (12345)",
            Self::ElliottCorrection => "Elliott Correction Wave (ABC)",
            Self::ElliottTriangle => "Elliott Triangle Wave (ABCDE)",
            Self::ElliottDoubleCombo => "Elliott Double Combo Wave (WXY)",
            Self::ElliottTripleCombo => "Elliott Triple Combo Wave (WXYXZ)",

            // Cycles
            Self::CyclicLines => "Cyclic Lines",
            Self::TimeCycles => "Time Cycles",
            Self::SineLine => "Sine Line",

            // Projection
            Self::LongPos => "Long Position",
            Self::ShortPos => "Short Position",
            Self::Forecast => "Forecast",
            Self::BarsPattern => "Bars Pattern",
            Self::GhostFeed => "Ghost Feed",
            Self::ProjectionTool => "Projection",

            // Volume
            Self::AnchoredVWAP => "Anchored VWAP",
            Self::FixedRangeVolumeProfile => "Fixed Range Volume Profile",
            Self::AnchoredVolumeProfile => "Anchored Volume Profile",

            // Measurers
            Self::Measure => "Measure",
            Self::PriceRange => "Price Range",
            Self::DateRange => "Date Range",
            Self::DateAndPriceRange => "Date and Price Range",

            // Brushes
            Self::Brush => "Brush",
            Self::Highlighter => "Highlighter",

            // Arrows
            Self::ArrowMarker => "Arrow Marker",
            Self::Arrow => "Arrow",
            Self::ArrowMarkUp => "Arrow Mark Up",
            Self::ArrowMarkDown => "Arrow Mark Down",
            Self::ArrowMarkLeft => "Arrow Mark Left",
            Self::ArrowMarkRight => "Arrow Mark Right",
            Self::Sector => "Sector",
            Self::Sticker => "Sticker",

            // Shapes
            Self::Rect => "Rectangle",
            Self::RotatedRect => "Rotated Rectangle",
            Self::Path => "Path",
            Self::Circle => "Circle",
            Self::Ellipse => "Ellipse",
            Self::Polyline => "Polyline",
            Self::Triangle => "Triangle",
            Self::Arc => "Arc",
            Self::Curve => "Curve",
            Self::DoubleCurve => "Double Curve",

            // Annotations
            Self::TextLabel => "Text",
            Self::AnchoredText => "Anchored Text",
            Self::Note => "Note",
            Self::PriceNote => "Price Note",
            Self::AnchoredNote => "Pin",
            Self::Table => "Table",
            Self::Callout => "Callout",
            Self::Comment => "Comment",
            Self::PriceLabel => "Price Label",
            Self::Signpost => "Signpost",
            Self::FlagNote => "Flag Mark",

            // Content
            Self::Image => "Image",
            Self::Tweet => "Tweet",
            Self::Idea => "Idea",

            // Icons
            Self::FontIcon => "Icon",
        }
    }

    /// Returns the data-name identifier for the tool.
    ///
    /// These identifiers (e.g., `"LineToolTrendLine"`, `"LineToolFibRetracement"`)
    /// follow the common `LineTool*` naming convention for drawing tools and are
    /// used for serialization/protocol compatibility.
    pub fn data_name(&self) -> &str {
        match self {
            Self::CrossCursor => "cursor",
            Self::DotCursor => "dot",
            Self::ArrowCursor => "arrow",
            Self::Eraser => "eraser",
            Self::TrendLine => "LineToolTrendLine",
            Self::Ray => "LineToolRay",
            Self::InfoLine => "LineToolInfoLine",
            Self::ExtendedLine => "LineToolExtended",
            Self::HorizontalLine => "LineToolHorzLine",
            Self::HorizontalRay => "LineToolHorzRay",
            Self::VerticalLine => "LineToolVertLine",
            Self::CrossLine => "LineToolCrossLine",
            Self::TrendAngle => "LineToolTrendAngle",
            Self::ParallelChannel => "LineToolParallelChannel",
            Self::RegressionTrend => "LineToolRegressionTrend",
            Self::FlatTopBottom => "LineToolFlatBottom",
            Self::DisjointChannel => "LineToolDisjointAngle",
            Self::Pitchfork => "LineToolPitchfork",
            Self::SchiffPitchfork => "LineToolSchiffPitchfork2",
            Self::ModifiedSchiffPitchfork => "LineToolSchiffPitchfork",
            Self::InsidePitchfork => "LineToolInsidePitchfork",
            Self::FibonacciRetracement => "LineToolFibRetracement",
            Self::FibonacciExtension => "LineToolTrendBasedFibExtension",
            Self::FibonacciChannel => "LineToolFibChannel",
            Self::FibonacciTimeZones => "LineToolFibTimeZone",
            Self::FibonacciSpeedFan => "LineToolFibSpeedResistanceFan",
            Self::TrendBasedFibTime => "LineToolTrendBasedFibTime",
            Self::FibonacciCircles => "LineToolFibCircles",
            Self::FibonacciSpiral => "LineToolFibSpiral",
            Self::FibonacciSpeedResistanceArcs => "LineToolFibSpeedResistanceArcs",
            Self::FibonacciWedge => "LineToolFibWedge",
            Self::Pitchfan => "LineToolPitchfan",
            Self::GannBox => "LineToolGannSquare",
            Self::GannSquare => "LineToolGannComplex",
            Self::GannFixed => "LineToolGannFixed",
            Self::GannFan => "LineToolGannFan",
            Self::XABCDPattern => "LineTool5PointsPattern",
            Self::CypherPattern => "LineToolCypherPattern",
            Self::HeadAndShoulders => "LineToolHeadAndShoulders",
            Self::ABCDPattern => "LineToolABCD",
            Self::TrianglePattern => "LineToolTrianglePattern",
            Self::ThreeDrivesPattern => "LineToolThreeDrivers",
            Self::ElliottImpulse => "LineToolElliottImpulse",
            Self::ElliottCorrection => "LineToolElliottCorrection",
            Self::ElliottTriangle => "LineToolElliottTriangle",
            Self::ElliottDoubleCombo => "LineToolElliottDoubleCombo",
            Self::ElliottTripleCombo => "LineToolElliottTripleCombo",
            Self::CyclicLines => "LineToolCircleLines",
            Self::TimeCycles => "LineToolTimeCycles",
            Self::SineLine => "LineToolSineLine",
            Self::LongPos => "LineToolRiskRewardLong",
            Self::ShortPos => "LineToolRiskRewardShort",
            Self::Forecast => "LineToolPrediction",
            Self::BarsPattern => "LineToolBarsPattern",
            Self::GhostFeed => "LineToolGhostFeed",
            Self::ProjectionTool => "LineToolProjection",
            Self::AnchoredVWAP => "LineToolAnchoredVWAP",
            Self::FixedRangeVolumeProfile => "LineToolFixedRangeVolumeProfile",
            Self::AnchoredVolumeProfile => "LineToolAnchoredVolumeProfile",
            Self::Measure => "measure",
            Self::PriceRange => "LineToolPriceRange",
            Self::DateRange => "LineToolDateRange",
            Self::DateAndPriceRange => "LineToolDateAndPriceRange",
            Self::Brush => "LineToolBrush",
            Self::Highlighter => "LineToolHighlighter",
            Self::ArrowMarker => "LineToolArrowMarker",
            Self::Arrow => "LineToolArrow",
            Self::ArrowMarkUp => "LineToolArrowMarkUp",
            Self::ArrowMarkDown => "LineToolArrowMarkDown",
            Self::ArrowMarkLeft => "LineToolArrowMarkLeft",
            Self::ArrowMarkRight => "LineToolArrowMarkRight",
            Self::Sector => "LineToolSector",
            Self::Sticker => "LineToolSticker",
            Self::Rect => "LineToolRect",
            Self::RotatedRect => "LineToolRotatedRect",
            Self::Path => "LineToolPath",
            Self::Circle => "LineToolCircle",
            Self::Ellipse => "LineToolEllipse",
            Self::Polyline => "LineToolPolyline",
            Self::Triangle => "LineToolTriangle",
            Self::Arc => "LineToolArc",
            Self::Curve => "LineToolBezierQuadro",
            Self::DoubleCurve => "LineToolBezierCubic",
            Self::TextLabel => "LineToolText",
            Self::AnchoredText => "LineToolTextAbsolute",
            Self::Note => "LineToolTextNote",
            Self::PriceNote => "LineToolPriceNote",
            Self::AnchoredNote => "LineToolNote",
            Self::Table => "LineToolTable",
            Self::Callout => "LineToolCallout",
            Self::Comment => "LineToolComment",
            Self::PriceLabel => "LineToolPriceLabel",
            Self::Signpost => "LineToolSignpost",
            Self::FlagNote => "LineToolFlagMark",
            Self::Image => "LineToolImage",
            Self::Tweet => "LineToolTweet",
            Self::Idea => "LineToolIdea",
            Self::FontIcon => "LineToolFontIcon",
        }
    }
}
