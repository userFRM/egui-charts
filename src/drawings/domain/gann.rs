//! Saved price and time divisions for Gann grids, and the fan and arcs of Gann squares.

use serde::{Deserialize, Serialize};

/// The corners the fan lines of a Gann square start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GannFanDirection {
    /// From the first anchor towards the opposite corner.
    #[default]
    Forward,
    /// From the opposite corner back towards the first anchor.
    Reverse,
    /// From both corners.
    Both,
}

/// Divisions of a Gann box or square, and the grid, fan and arcs of a Gann square.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GannConfig {
    /// Price fractions measured from the first anchor; values outside 0..=1 are ignored.
    pub price_levels: Vec<f32>,
    /// Time fractions measured from the first anchor; values outside 0..=1 are ignored.
    pub time_levels: Vec<f32>,
    /// Whether a Gann square draws its grid of price and time divisions.
    pub show_grid: bool,
    /// Whether a Gann square draws its fan lines.
    pub show_fan: bool,
    /// Price units per time unit of each fan line of a Gann square, where 1 is the diagonal.
    /// Must be positive and finite.
    pub fan_ratios: Vec<f32>,
    /// The corners the fan lines start from.
    pub fan_direction: GannFanDirection,
    /// Whether a Gann square draws its arcs.
    pub show_arcs: bool,
    /// Radius of each arc of a Gann square around its first anchor, as a fraction of its side.
    /// Must be positive and finite; the part outside the square is not drawn.
    pub arc_ratios: Vec<f32>,
}

impl Default for GannConfig {
    fn default() -> Self {
        let levels = (0..=8).map(|i| i as f32 / 8.0).collect::<Vec<_>>();
        Self {
            price_levels: levels.clone(),
            time_levels: levels,
            show_grid: true,
            show_fan: true,
            fan_ratios: vec![0.125, 0.25, 1.0 / 3.0, 0.5, 1.0, 2.0, 3.0, 4.0, 8.0],
            fan_direction: GannFanDirection::default(),
            show_arcs: true,
            arc_ratios: vec![0.25, 0.5, 0.75, 1.0],
        }
    }
}
