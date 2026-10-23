//! Saved price and time divisions for Gann grids.

use serde::{Deserialize, Serialize};

/// Fractional divisions along the two oriented axes of a Gann box or square.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GannConfig {
    /// Price fractions measured from the first anchor; values outside 0..=1 are ignored.
    pub price_levels: Vec<f32>,
    /// Time fractions measured from the first anchor; values outside 0..=1 are ignored.
    pub time_levels: Vec<f32>,
}

impl Default for GannConfig {
    fn default() -> Self {
        let levels = (0..=8).map(|i| i as f32 / 8.0).collect::<Vec<_>>();
        Self {
            price_levels: levels.clone(),
            time_levels: levels,
        }
    }
}
