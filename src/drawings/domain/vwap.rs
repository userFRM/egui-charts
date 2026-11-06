//! Bands of the anchored VWAP drawing.

use serde::{Deserialize, Serialize};

/// Colours of the three bands: green, olive and teal.
const BAND_COLORS: [[u8; 4]; 3] = [[76, 175, 80, 255], [128, 128, 0, 255], [0, 137, 123, 255]];

/// A line above and a line below an anchored VWAP, a multiple of the volume-weighted standard
/// deviation of the typical price away from it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "VwapBandFields")]
pub struct VwapBand {
    /// Whether the band is drawn.
    pub enabled: bool,
    multiplier: f64,
    /// Colour of the band's lines and, on the first band, of its fill at 5%.
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
    /// A green band `multiplier` standard deviations either side of the VWAP (above 0, at most
    /// 10); `None` when out of bounds.
    pub fn new(enabled: bool, multiplier: f64) -> Option<Self> {
        (multiplier > 0.0 && multiplier <= 10.0).then_some(Self {
            enabled,
            multiplier,
            color: first_band_color(),
        })
    }

    /// Distance of the band's lines from the VWAP, in standard deviations.
    pub fn multiplier(&self) -> f64 {
        self.multiplier
    }
}

/// The bands of an anchored VWAP drawing; a drawing without one uses the default.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct VwapConfig {
    /// The bands, each drawn when enabled; the first one is also filled.
    pub bands: [VwapBand; 3],
}

impl Default for VwapConfig {
    /// The green band one standard deviation away is on; the olive and teal bands two and three
    /// away are off.
    fn default() -> Self {
        let band = |index: usize, enabled| VwapBand {
            enabled,
            multiplier: (index + 1) as f64,
            color: BAND_COLORS[index],
        };
        Self {
            bands: [band(0, true), band(1, false), band(2, false)],
        }
    }
}
