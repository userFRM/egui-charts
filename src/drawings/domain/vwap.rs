//! Bands of the anchored VWAP drawing.

use serde::{Deserialize, Serialize};

/// A line above and a line below an anchored VWAP, a multiple of the volume-weighted standard
/// deviation of the typical price away from it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "VwapBandFields")]
pub struct VwapBand {
    /// Whether the band is drawn.
    pub enabled: bool,
    multiplier: f64,
    /// Line colour as RGBA. The first band of a [`VwapConfig`] also fills the area between its
    /// lines with this colour at 5% of its opacity.
    pub color: [u8; 4],
}

/// Stored settings of a band, checked by [`VwapBand::new`] on load.
#[derive(Deserialize)]
struct VwapBandFields {
    enabled: bool,
    multiplier: f64,
    color: [u8; 4],
}

impl TryFrom<VwapBandFields> for VwapBand {
    type Error = &'static str;

    fn try_from(fields: VwapBandFields) -> Result<Self, Self::Error> {
        Self::new(fields.enabled, fields.multiplier, fields.color)
            .ok_or("anchored VWAP band multiplier out of bounds")
    }
}

impl VwapBand {
    /// A band `multiplier` standard deviations either side of the VWAP (above 0, at most 10);
    /// `None` when out of bounds.
    pub fn new(enabled: bool, multiplier: f64, color: [u8; 4]) -> Option<Self> {
        (multiplier > 0.0 && multiplier <= 10.0).then_some(Self {
            enabled,
            multiplier,
            color,
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
    /// The band one standard deviation away is on, in green; the bands two and three away are
    /// off, in olive and teal, the colours of the VWAP study's bands.
    fn default() -> Self {
        let band = |enabled, multiplier, color| VwapBand {
            enabled,
            multiplier,
            color,
        };
        Self {
            bands: [
                band(true, 1.0, [76, 175, 80, 255]),
                band(false, 2.0, [128, 128, 0, 255]),
                band(false, 3.0, [0, 137, 123, 255]),
            ],
        }
    }
}
