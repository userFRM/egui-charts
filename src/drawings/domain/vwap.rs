//! Source, bands, fills and base line of the anchored VWAP drawing.

use crate::model::PriceSource;
use serde::{Deserialize, Deserializer, Serialize};

/// Colours of the three bands: green, olive and teal.
const BAND_COLORS: [[u8; 4]; 3] = [[76, 175, 80, 255], [128, 128, 0, 255], [0, 137, 123, 255]];

/// A line above and a line below an anchored VWAP, a multiple of the
/// [band unit](VwapBandMode) away from it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "VwapBandFields")]
pub struct VwapBand {
    /// Whether the band is drawn.
    pub enabled: bool,
    multiplier: f64,
    /// Colour of the band's lines and, on the first band, of its fill unless the fill has its own.
    pub color: [u8; 4],
}

/// Stored settings of a band, checked by [`VwapBand::new`] on load.
#[derive(Deserialize)]
struct VwapBandFields {
    enabled: bool,
    multiplier: f64,
    /// Bands saved before they had colours were all green.
    #[serde(default = "first_band_color")]
    color: [u8; 4],
}

fn first_band_color() -> [u8; 4] {
    BAND_COLORS[0]
}

impl TryFrom<VwapBandFields> for VwapBand {
    type Error = &'static str;

    fn try_from(fields: VwapBandFields) -> Result<Self, Self::Error> {
        let mut band = Self::new(fields.enabled, fields.multiplier)
            .ok_or("anchored VWAP band multiplier out of bounds")?;
        band.color = fields.color;
        Ok(band)
    }
}

impl VwapBand {
    /// A green band `multiplier` band units either side of the VWAP (above 0, at most 10);
    /// `None` when out of bounds.
    pub fn new(enabled: bool, multiplier: f64) -> Option<Self> {
        (multiplier > 0.0 && multiplier <= 10.0).then_some(Self {
            enabled,
            multiplier,
            color: first_band_color(),
        })
    }

    /// Distance of the band's lines from the VWAP, in band units.
    pub fn multiplier(&self) -> f64 {
        self.multiplier
    }
}

/// What a band multiplier counts: the unit of an anchored VWAP band's distance from the VWAP.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VwapBandMode {
    /// Volume-weighted standard deviations of the source price.
    #[default]
    Deviation,
    /// Percent of the VWAP.
    Percentage,
}

/// How the line of an anchored VWAP drawing is drawn, at the drawing's colour and width.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum VwapPlotType {
    /// Connected values.
    #[default]
    Line,
    /// Horizontal segments followed by a vertical change at the next bar.
    StepLine,
    /// Bars of the line's width from zero to each value.
    Histogram,
    /// Columns from zero to each value, at 80% of the bar pitch.
    Columns,
    /// Filled area between the values and zero.
    Area,
    /// A circle at each value.
    Circles,
    /// A plus at each value.
    Crosses,
}

impl VwapPlotType {
    /// Every plot type, in the order the Inputs tab lists them.
    pub const ALL: [Self; 7] = [
        Self::Line,
        Self::StepLine,
        Self::Histogram,
        Self::Columns,
        Self::Area,
        Self::Circles,
        Self::Crosses,
    ];

    /// User-facing name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::StepLine => "Step line",
            Self::Histogram => "Histogram",
            Self::Columns => "Columns",
            Self::Area => "Area",
            Self::Circles => "Circles",
            Self::Crosses => "Crosses",
        }
    }
}

/// A fill of an anchored VWAP drawing: whether it is painted, its colour and opacity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "VwapFillFields")]
pub struct VwapFill {
    /// Whether the fill is painted.
    pub visible: bool,
    /// Colour; `None` follows the first band's colour in the band fill and the line colour in
    /// the background.
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
    /// Bar price the VWAP and its deviation average.
    pub source: PriceSource,
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
    /// without one fills in the first band's colour, as it was drawn.
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
            color: BAND_COLORS[index],
        };
        Self {
            bands: [band(0, true), band(1, false), band(2, false)],
            source: PriceSource::HLC3,
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
    /// band fill, which keeps following the first band's colour, so it paints as before;
    /// out-of-range settings are refused.
    #[test]
    fn saved_settings_restore_and_are_checked() {
        let saved = r#"{"bands":[{"enabled":true,"multiplier":1.0,"color":[76,175,80,255]},{"enabled":false,"multiplier":2.0,"color":[128,128,0,255]},{"enabled":false,"multiplier":3.0,"color":[0,137,123,255]}]}"#;
        let config: VwapConfig = serde_json::from_str(saved).unwrap();
        let mut legacy = VwapConfig::default();
        legacy.band_fill.color = None;
        assert_eq!(config, legacy);
        for bad in [
            r#"{"precision":9}"#,
            r#"{"band_fill":{"visible":true,"color":null,"opacity":1.5}}"#,
            r#"{"background":{"visible":true,"color":null,"opacity":-0.1}}"#,
        ] {
            assert!(serde_json::from_str::<VwapConfig>(bad).is_err(), "{bad}");
        }
    }
}
