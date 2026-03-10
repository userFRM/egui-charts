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

impl FromStr for DrawingToolType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            // Cursors
            "crosscursor" | "cross cursor" | "crosshair" | "cursor" => Ok(Self::CrossCursor),
            "dotcursor" | "dot cursor" | "dot" => Ok(Self::DotCursor),
            "arrowcursor" | "arrow cursor" | "pointer" => Ok(Self::ArrowCursor),
            "eraser" | "erase" => Ok(Self::Eraser),

            // Lines
            "trendline" | "trend line" | "trend" => Ok(Self::TrendLine),
            "ray" => Ok(Self::Ray),
            "infoline" | "info line" | "info" => Ok(Self::InfoLine),
            "extendedline" | "extended line" | "extended" => Ok(Self::ExtendedLine),
            "horizontalline" | "horizontal line" | "horizontal" | "hline" => {
                Ok(Self::HorizontalLine)
            }
            "horizontalray" | "horizontal ray" | "hray" => Ok(Self::HorizontalRay),
            "verticalline" | "vertical line" | "vertical" | "vline" => Ok(Self::VerticalLine),
            "crossline" | "cross line" | "cross" => Ok(Self::CrossLine),
            "trendangle" | "trend angle" | "angle" => Ok(Self::TrendAngle),

            // Channels
            "parallelchannel" | "parallel channel" | "channel" => Ok(Self::ParallelChannel),
            "regressiontrend" | "regression trend" | "regression" | "linear regression" => {
                Ok(Self::RegressionTrend)
            }
            "flattopbottom" | "flat top bottom" | "flat top/bottom" | "flat channel" => {
                Ok(Self::FlatTopBottom)
            }
            "disjointchannel" | "disjoint channel" => Ok(Self::DisjointChannel),

            // Pitchforks
            "pitchfork" | "andrews pitchfork" | "andrews" => Ok(Self::Pitchfork),
            "schiffpitchfork" | "schiff pitchfork" | "schiff" => Ok(Self::SchiffPitchfork),
            "modifiedschiffpitchfork" | "modified schiff pitchfork" | "modified schiff" => {
                Ok(Self::ModifiedSchiffPitchfork)
            }
            "insidepitchfork" | "inside pitchfork" | "inside" => Ok(Self::InsidePitchfork),

            // Fibonacci
            "fibonacci" | "fib" | "fibonacci retracement" | "fib retracement" => {
                Ok(Self::FibonacciRetracement)
            }
            "fibonacciextension" | "fibonacci extension" | "fib extension" | "fib ext" => {
                Ok(Self::FibonacciExtension)
            }
            "fibonaccichannel" | "fibonacci channel" | "fib channel" => Ok(Self::FibonacciChannel),
            "fibonaccitimezones" | "fibonacci time zones" | "fib time" | "fib time zones" => {
                Ok(Self::FibonacciTimeZones)
            }
            "fibonaccispeedfan" | "fibonacci speed fan" | "fib speed fan" | "speed fan" => {
                Ok(Self::FibonacciSpeedFan)
            }
            "trendbasedfibtime" | "trend based fib time" | "fib trend time" => {
                Ok(Self::TrendBasedFibTime)
            }
            "fibonaccicircles" | "fibonacci circles" | "fib circles" => Ok(Self::FibonacciCircles),
            "fibonaccispiral" | "fibonacci spiral" | "fib spiral" | "spiral" => {
                Ok(Self::FibonacciSpiral)
            }
            "fibonaccispeedresistancearcs" | "fibonacci speed resistance arcs" | "fib arcs" => {
                Ok(Self::FibonacciSpeedResistanceArcs)
            }
            "fibonacciwedge" | "fibonacci wedge" | "fib wedge" => Ok(Self::FibonacciWedge),
            "pitchfan" | "pitch fan" => Ok(Self::Pitchfan),

            // Gann
            "gannbox" | "gann box" => Ok(Self::GannBox),
            "gannsquare" | "gann square" => Ok(Self::GannSquare),
            "gannfixed" | "gann fixed" | "gann square fixed" => Ok(Self::GannFixed),
            "gannfan" | "gann fan" | "gann" => Ok(Self::GannFan),

            // Patterns
            "xabcdpattern" | "xabcd pattern" | "xabcd" => Ok(Self::XABCDPattern),
            "cypherpattern" | "cypher pattern" | "cypher" => Ok(Self::CypherPattern),
            "headandshoulders" | "head and shoulders" | "h&s" | "head shoulders" => {
                Ok(Self::HeadAndShoulders)
            }
            "abcdpattern" | "abcd pattern" | "abcd" => Ok(Self::ABCDPattern),
            "trianglepattern" | "triangle pattern" => Ok(Self::TrianglePattern),
            "threedrivespattern" | "three drives pattern" | "three drives" => {
                Ok(Self::ThreeDrivesPattern)
            }

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
            "cycliclines" | "cyclic lines" | "cyclic" => Ok(Self::CyclicLines),
            "timecycles" | "time cycles" => Ok(Self::TimeCycles),
            "sineline" | "sine line" | "sine" => Ok(Self::SineLine),

            // Projection
            "longposition" | "long position" | "long" => Ok(Self::LongPos),
            "shortposition" | "short position" | "short" => Ok(Self::ShortPos),
            "forecast" => Ok(Self::Forecast),
            "barspattern" | "bars pattern" => Ok(Self::BarsPattern),
            "ghostfeed" | "ghost feed" => Ok(Self::GhostFeed),
            "projectiontool" | "projection tool" | "projection" => Ok(Self::ProjectionTool),

            // Volume Tools
            "anchoredvwap" | "anchored vwap" | "vwap" => Ok(Self::AnchoredVWAP),
            "fixedrangevolumeprofile" | "fixed range volume profile" | "volume profile" => {
                Ok(Self::FixedRangeVolumeProfile)
            }
            "anchoredvolumeprofile" | "anchored volume profile" => Ok(Self::AnchoredVolumeProfile),

            // Measurers
            "measure" | "ruler" => Ok(Self::Measure),
            "pricerange" | "price range" => Ok(Self::PriceRange),
            "daterange" | "date range" | "time range" => Ok(Self::DateRange),
            "dateandpricerange" | "date and price range" => Ok(Self::DateAndPriceRange),

            // Brushes
            "brush" | "freehand" => Ok(Self::Brush),
            "highlighter" | "highlight" => Ok(Self::Highlighter),
            "paintbrush" | "paint brush" | "paint" => Ok(Self::Paintbrush),

            // Arrows
            "arrowmarker" | "arrow marker" => Ok(Self::ArrowMarker),
            "arrow" => Ok(Self::Arrow),
            "arrowmarkup" | "arrow mark up" | "arrow up" => Ok(Self::ArrowMarkUp),
            "arrowmarkdown" | "arrow mark down" | "arrow down" => Ok(Self::ArrowMarkDown),

            // Shapes
            "rect" | "rectangle" => Ok(Self::Rect),
            "rotatedrect" | "rotated rect" | "rotated rectangle" => Ok(Self::RotatedRect),
            "path" | "multipath" => Ok(Self::Path),
            "circle" => Ok(Self::Circle),
            "ellipse" | "oval" => Ok(Self::Ellipse),
            "polyline" | "poly line" | "multiline" => Ok(Self::Polyline),
            "triangle" => Ok(Self::Triangle),
            "arc" => Ok(Self::Arc),
            "curve" | "bezierquadro" | "bezier quadro" => Ok(Self::Curve),
            "doublecurve" | "double curve" | "beziercubic" | "bezier cubic" => {
                Ok(Self::DoubleCurve)
            }

            // Annotations
            "textlabel" | "text label" | "text" | "label" => Ok(Self::TextLabel),
            "anchoredtext" | "anchored text" => Ok(Self::AnchoredText),
            "note" | "textnote" | "text note" => Ok(Self::Note),
            "pricenote" | "price note" => Ok(Self::PriceNote),
            "anchorednote" | "anchored note" | "pin" => Ok(Self::AnchoredNote),
            "table" => Ok(Self::Table),
            "callout" => Ok(Self::Callout),
            "comment" => Ok(Self::Comment),
            "pricelabel" | "price label" => Ok(Self::PriceLabel),
            "signpost" => Ok(Self::Signpost),
            "flagnote" | "flag note" | "flag" | "flagmark" | "flag mark" => Ok(Self::FlagNote),

            // Content
            "image" | "img" => Ok(Self::Image),
            "tweet" | "twitter" => Ok(Self::Tweet),
            "idea" => Ok(Self::Idea),

            // Icons
            "fonticon" | "font icon" | "icon" | "emoji" => Ok(Self::FontIcon),

            _ => Err(format!("Invalid drawing tool type: {s}")),
        }
    }
}

impl DrawingToolType {
    /// Returns a human-readable display name for the tool (e.g., `"Trend Line"`,
    /// `"Fib Retracement"`).
    ///
    /// This is the string shown in the toolbar UI and in the [`Display`](std::fmt::Display)
    /// implementation.
    pub fn as_str(&self) -> &str {
        match self {
            // Cursors
            Self::CrossCursor => "Cross",
            Self::DotCursor => "Dot",
            Self::ArrowCursor => "Arrow",
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
            Self::Paintbrush => "Paintbrush",

            // Arrows
            Self::ArrowMarker => "Arrow Marker",
            Self::Arrow => "Arrow",
            Self::ArrowMarkUp => "Arrow Mark Up",
            Self::ArrowMarkDown => "Arrow Mark Down",

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

    /// Returns the TradingView-compatible data-name identifier for the tool.
    ///
    /// These identifiers (e.g., `"LineToolTrendLine"`, `"LineToolFibRetracement"`)
    /// match the naming convention used by TradingView's charting library and are
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
            Self::Paintbrush => "LineToolPaintbrush",
            Self::ArrowMarker => "LineToolArrowMarker",
            Self::Arrow => "LineToolArrow",
            Self::ArrowMarkUp => "LineToolArrowMarkUp",
            Self::ArrowMarkDown => "LineToolArrowMarkDown",
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
