//! Source, bands, fills and base line of the anchored VWAP drawing.

use crate::drawings::validation::MAX_STROKE_WIDTH;
use crate::model::{LineStyle, PriceSource};
use serde::{Deserialize, Deserializer, Serialize};

/// Colours of the three bands: green, olive and teal.
const BAND_COLORS: [[u8; 4]; 3] = [[76, 175, 80, 255], [128, 128, 0, 255], [0, 137, 123, 255]];

/// A line above and a line below an anchored VWAP, a multiple of the
/// [band unit](VwapBandMode) away from it, each styled by its own [`VwapBandLine`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "VwapBandFields")]
pub struct VwapBand {
    /// Whether the band is drawn.
    pub enabled: bool,
    multiplier: f64,
    /// The line above the VWAP. On the first band its colour is also the fill's, unless the
    /// fill has its own.
    pub upper: VwapBandLine,
    /// The line below the VWAP.
    pub lower: VwapBandLine,
}

/// Stored settings of a band, checked by [`VwapBand::new`] on load.
#[derive(Deserialize)]
struct VwapBandFields {
    enabled: bool,
    multiplier: f64,
    /// The colour of both lines of a band saved before each line had its own style; bands
    /// saved before they had colours were all green.
    #[serde(default = "first_band_color")]
    color: [u8; 4],
    upper: Option<VwapBandLine>,
    lower: Option<VwapBandLine>,
}

fn first_band_color() -> [u8; 4] {
    BAND_COLORS[0]
}

impl TryFrom<VwapBandFields> for VwapBand {
    type Error = &'static str;

    fn try_from(fields: VwapBandFields) -> Result<Self, Self::Error> {
        let mut band = Self::new(fields.enabled, fields.multiplier)
            .ok_or("anchored VWAP band multiplier out of bounds")?;
        let saved = VwapBandLine::new(fields.color);
        band.upper = fields.upper.unwrap_or(saved);
        band.lower = fields.lower.unwrap_or(saved);
        Ok(band)
    }
}

impl VwapBand {
    /// A band of green lines `multiplier` band units either side of the VWAP (above 0, at
    /// most 10); `None` when out of bounds.
    pub fn new(enabled: bool, multiplier: f64) -> Option<Self> {
        let line = VwapBandLine::new(first_band_color());
        (multiplier > 0.0 && multiplier <= 10.0).then_some(Self {
            enabled,
            multiplier,
            upper: line,
            lower: line,
        })
    }

    /// Distance of the band's lines from the VWAP, in band units.
    pub fn multiplier(&self) -> f64 {
        self.multiplier
    }
}

/// How one line of an anchored VWAP band is drawn: whether it shows, its colour, pattern,
/// width, plot and opacity, and whether its latest value is marked.
///
/// ```
/// use egui_charts::drawings::{VwapBand, VwapBandLine, VwapConfig, VwapPlotType};
/// use egui_charts::model::LineStyle;
///
/// let mut line = VwapBandLine::new([200, 0, 0, 255])
///     .with_width(2.0)
///     .and_then(|line| line.with_opacity(0.5))
///     .unwrap();
/// line.line_style = LineStyle::Dashed;
/// line.plot_type = VwapPlotType::StepLine;
/// line.track_price = true;
/// let mut band = VwapBand::new(true, 1.5).unwrap();
/// band.lower = line;
/// let mut config = VwapConfig::default();
/// config.executions = true;
/// config.bands[1] = band;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "VwapBandLineFields")]
pub struct VwapBandLine {
    /// Whether the line is drawn while its band is enabled.
    pub visible: bool,
    /// Colour, whose alpha the opacity scales.
    pub color: [u8; 4],
    /// Pattern of the line and step line plots.
    pub line_style: LineStyle,
    width: f32,
    /// How the line is drawn.
    pub plot_type: VwapPlotType,
    /// Whether a dotted line across the pane and a price axis tag mark the line's latest value.
    pub track_price: bool,
    opacity: f32,
}

/// Stored settings of a band line, checked by [`VwapBandLine::with_width`] and
/// [`VwapBandLine::with_opacity`] on load.
#[derive(Deserialize)]
struct VwapBandLineFields {
    visible: bool,
    color: [u8; 4],
    line_style: LineStyle,
    width: f32,
    plot_type: VwapPlotType,
    track_price: bool,
    opacity: f32,
}

impl TryFrom<VwapBandLineFields> for VwapBandLine {
    type Error = &'static str;

    fn try_from(fields: VwapBandLineFields) -> Result<Self, Self::Error> {
        let line = Self {
            visible: fields.visible,
            line_style: fields.line_style,
            plot_type: fields.plot_type,
            track_price: fields.track_price,
            ..Self::new(fields.color)
        };
        line.with_width(fields.width)
            .ok_or("anchored VWAP band line width out of bounds")?
            .with_opacity(fields.opacity)
            .ok_or("anchored VWAP band line opacity out of bounds")
    }
}

impl VwapBandLine {
    /// A shown, solid, opaque line of 1 px in `color`, without a latest-value mark.
    pub fn new(color: [u8; 4]) -> Self {
        Self {
            visible: true,
            color,
            line_style: LineStyle::Solid,
            width: 1.0,
            plot_type: VwapPlotType::Line,
            track_price: false,
            opacity: 1.0,
        }
    }

    /// The line at `width` px, above 0 and at most 100; `None` when out of bounds.
    pub fn with_width(self, width: f32) -> Option<Self> {
        (width > 0.0 && width <= MAX_STROKE_WIDTH).then_some(Self { width, ..self })
    }

    /// The line at `opacity`, from 0 (clear) through 1 (opaque); `None` when out of bounds.
    pub fn with_opacity(self, opacity: f32) -> Option<Self> {
        (0.0..=1.0)
            .contains(&opacity)
            .then_some(Self { opacity, ..self })
    }

    /// Width in px of the line, and the size of its circles, crosses and histogram bars.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Opacity from 0 (clear) through 1 (opaque), which scales the colour's own alpha.
    pub fn opacity(&self) -> f32 {
        self.opacity
    }
}

/// What a band multiplier counts: the unit of an anchored VWAP band's distance from the VWAP.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum VwapBandMode {
    /// Volume-weighted standard deviations of the source price.
    #[default]
    Deviation,
    /// Percent of the VWAP.
    Percentage,
}

/// How the line of an anchored VWAP drawing or of one of its bands is drawn, at that line's
/// colour and width.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum VwapPlotType {
    /// Connected values.
    #[default]
    Line,
    /// Connected values, restarting after a bar without a value.
    LineWithBreaks,
    /// Horizontal segments followed by a vertical change at the next bar.
    StepLine,
    /// Step segments, restarting after a bar without a value.
    StepLineWithBreaks,
    /// Step segments with a diamond at the first value and each change of value.
    StepLineWithDiamonds,
    /// Bars of the line's width from zero to each value.
    Histogram,
    /// Columns from zero to each value, at 80% of the bar pitch.
    Columns,
    /// Filled area between the values and zero.
    Area,
    /// Filled area and its line, restarting after a bar without a value.
    AreaWithBreaks,
    /// A circle at each value.
    Circles,
    /// A plus at each value.
    Crosses,
}

impl VwapPlotType {
    /// Every plot type, in the order the Inputs tab lists them.
    pub const ALL: [Self; 11] = [
        Self::Line,
        Self::LineWithBreaks,
        Self::StepLine,
        Self::StepLineWithBreaks,
        Self::StepLineWithDiamonds,
        Self::Histogram,
        Self::Columns,
        Self::Area,
        Self::AreaWithBreaks,
        Self::Circles,
        Self::Crosses,
    ];

    /// User-facing name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::LineWithBreaks => "Line with breaks",
            Self::StepLine => "Step line",
            Self::StepLineWithBreaks => "Step line with breaks",
            Self::StepLineWithDiamonds => "Step line with diamonds",
            Self::Histogram => "Histogram",
            Self::Columns => "Columns",
            Self::Area => "Area",
            Self::AreaWithBreaks => "Area with breaks",
            Self::Circles => "Circles",
            Self::Crosses => "Crosses",
        }
    }

    pub(crate) fn has_breaks(self) -> bool {
        matches!(
            self,
            Self::LineWithBreaks | Self::StepLineWithBreaks | Self::AreaWithBreaks
        )
    }
}

/// A fill of an anchored VWAP drawing: whether it is painted, its colour and opacity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "VwapFillFields")]
pub struct VwapFill {
    /// Whether the fill is painted.
    pub visible: bool,
    /// Colour; `None` follows the colour of the first band's upper line in the band fill and the
    /// line colour in the background.
    pub color: Option<[u8; 4]>,
    opacity: f32,
}

/// Stored settings of a fill, checked by [`VwapFill::new`] on load.
#[derive(Deserialize)]
struct VwapFillFields {
    visible: bool,
    color: Option<[u8; 4]>,
    opacity: f32,
}

impl TryFrom<VwapFillFields> for VwapFill {
    type Error = &'static str;

    fn try_from(fields: VwapFillFields) -> Result<Self, Self::Error> {
        Self::new(fields.visible, fields.color, fields.opacity)
            .ok_or("anchored VWAP fill opacity out of bounds")
    }
}

impl VwapFill {
    /// A fill at `opacity`, from 0 (clear) through 1 (opaque); `None` when out of bounds.
    pub fn new(visible: bool, color: Option<[u8; 4]>, opacity: f32) -> Option<Self> {
        (0.0..=1.0).contains(&opacity).then_some(Self {
            visible,
            color,
            opacity,
        })
    }

    /// The band fill of a drawing saved before the fill had its own colour: on, in the first
    /// band's colour at 5%.
    fn following_band() -> Self {
        Self {
            visible: true,
            color: None,
            opacity: 0.05,
        }
    }

    /// Opacity from 0 (clear) through 1 (opaque), which scales the colour's own alpha.
    pub fn opacity(&self) -> f32 {
        self.opacity
    }
}

/// The settings of an anchored VWAP drawing; a drawing without them uses the default, and a
/// saved one restores missing settings from it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct VwapConfig {
    /// The bands, each drawn when enabled.
    pub bands: [VwapBand; 3],
    /// Bar price the VWAP and its deviation average on bars that do not count executions.
    pub source: PriceSource,
    /// Whether each bar whose executions the chart holds in full counts their volume at the
    /// middle of each row's ticks. Other bars count their volume at [`source`](Self::source).
    /// While a visible drawing counts executions, the chart holds them on time-based intervals
    /// and on every interval of its footprint chart types. Alerts evaluated without the chart
    /// count every bar at the source. Off by default and for drawings saved without it.
    pub executions: bool,
    /// Unit of the band multipliers.
    pub band_mode: VwapBandMode,
    /// Whether the VWAP line is drawn; the bands and fills keep their own switches.
    pub show_line: bool,
    /// How the VWAP line is drawn.
    pub plot_type: VwapPlotType,
    /// Whether a dotted line across the pane and a price axis tag mark the latest VWAP.
    pub track_price: bool,
    /// Decimal places of the latest VWAP's price axis tag, from 0 through 8; `None` follows
    /// the price scale.
    #[serde(deserialize_with = "precision")]
    pub precision: Option<u8>,
    /// Fill between the first band's lines, drawn while that band is enabled. A drawing saved
    /// without one fills in the colour of the first band's upper line, as it was drawn.
    #[serde(default = "VwapFill::following_band")]
    pub band_fill: VwapFill,
    /// Wash over the pane's height from the anchor bar to the last bar, under the bands.
    pub background: VwapFill,
}

fn precision<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u8>, D::Error> {
    let places = Option::<u8>::deserialize(deserializer)?;
    if places.is_some_and(|p| p > 8) {
        return Err(serde::de::Error::custom(
            "anchored VWAP precision above 8 decimals",
        ));
    }
    Ok(places)
}

impl VwapConfig {
    /// Whether the precision is at most 8 decimals.
    pub fn is_valid(&self) -> bool {
        self.precision.is_none_or(|places| places <= 8)
    }
}

impl Default for VwapConfig {
    /// The VWAP of the typical price as a line, with the green band one standard deviation away
    /// filled green at 5%; the olive and teal bands two and three away, the green background at
    /// 5% and the latest-value line are off.
    fn default() -> Self {
        let band = |index: usize, enabled| VwapBand {
            enabled,
            multiplier: (index + 1) as f64,
            upper: VwapBandLine::new(BAND_COLORS[index]),
            lower: VwapBandLine::new(BAND_COLORS[index]),
        };
        Self {
            bands: [band(0, true), band(1, false), band(2, false)],
            source: PriceSource::HLC3,
            executions: false,
            band_mode: VwapBandMode::Deviation,
            show_line: true,
            plot_type: VwapPlotType::Line,
            track_price: false,
            precision: None,
            band_fill: VwapFill {
                color: Some(BAND_COLORS[0]),
                ..VwapFill::following_band()
            },
            background: VwapFill {
                visible: false,
                color: Some(BAND_COLORS[0]),
                opacity: 0.05,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drawing saved with bands only restores every later setting at its default, except its
    /// band fill, which keeps following the first band's colour, so it paints as before, and
    /// each band's colour styles both its lines; out-of-range settings are refused.
    #[test]
    fn saved_settings_restore_and_are_checked() {
        let saved = r#"{"bands":[{"enabled":true,"multiplier":1.0,"color":[76,175,80,255]},{"enabled":false,"multiplier":2.0,"color":[128,128,0,255]},{"enabled":false,"multiplier":3.0,"color":[0,137,123,255]}]}"#;
        let config: VwapConfig = serde_json::from_str(saved).unwrap();
        assert!(!config.executions);
        let mut legacy = VwapConfig::default();
        legacy.band_fill.color = None;
        assert_eq!(config, legacy);
        // Full settings saved before executions were an option retain every style and source.
        let saved = r#"{"bands":[{"enabled":true,"multiplier":1.0,"upper":{"visible":true,"color":[76,175,80,255],"line_style":"Solid","width":1.0,"plot_type":"Line","track_price":false,"opacity":1.0},"lower":{"visible":true,"color":[76,175,80,255],"line_style":"Solid","width":1.0,"plot_type":"Line","track_price":false,"opacity":1.0}},{"enabled":false,"multiplier":2.0,"upper":{"visible":true,"color":[128,128,0,255],"line_style":"Solid","width":1.0,"plot_type":"Line","track_price":false,"opacity":1.0},"lower":{"visible":true,"color":[128,128,0,255],"line_style":"Solid","width":1.0,"plot_type":"Line","track_price":false,"opacity":1.0}},{"enabled":false,"multiplier":3.0,"upper":{"visible":true,"color":[0,137,123,255],"line_style":"Solid","width":1.0,"plot_type":"Line","track_price":false,"opacity":1.0},"lower":{"visible":true,"color":[0,137,123,255],"line_style":"Solid","width":1.0,"plot_type":"Line","track_price":false,"opacity":1.0}}],"source":"HLC3","band_mode":"Deviation","show_line":true,"plot_type":"Line","track_price":false,"precision":null,"band_fill":{"visible":true,"color":[76,175,80,255],"opacity":0.05},"background":{"visible":false,"color":[76,175,80,255],"opacity":0.05}}"#;
        let config: VwapConfig = serde_json::from_str(saved).unwrap();
        assert!(!config.executions);
        assert_eq!(config, VwapConfig::default());
        for bad in [
            r#"{"precision":9}"#,
            r#"{"band_fill":{"visible":true,"color":null,"opacity":1.5}}"#,
            r#"{"background":{"visible":true,"color":null,"opacity":-0.1}}"#,
        ] {
            assert!(serde_json::from_str::<VwapConfig>(bad).is_err(), "{bad}");
        }
        for (field, value) in [("width", 0.0), ("width", 101.0), ("opacity", 1.5)] {
            let mut saved = serde_json::to_value(VwapConfig::default()).unwrap();
            saved["bands"][1]["lower"][field] = value.into();
            assert!(
                serde_json::from_value::<VwapConfig>(saved).is_err(),
                "{field} {value}"
            );
        }
    }
}
