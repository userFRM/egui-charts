//! Saved levels, fan lines, arcs and background of the Gann tools.

use super::DrawingToolType;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::f32::consts::SQRT_2;

/// Units per side of a Gann square and of a fixed Gann square.
pub(crate) const GANN_SQUARE_UNITS: f32 = 5.0;
/// An arc of this radius in units, or more, lies wholly outside a Gann square.
pub(crate) const GANN_MAX_ARC: f32 = GANN_SQUARE_UNITS * SQRT_2;

const GREY: [u8; 4] = [128, 128, 128, 255];
const ORANGE: [u8; 4] = [255, 152, 0, 255];
const CYAN: [u8; 4] = [0, 188, 212, 255];
const GREEN: [u8; 4] = [76, 175, 80, 255];
const TEAL: [u8; 4] = [8, 153, 129, 255];
const BLUE: [u8; 4] = [41, 98, 255, 255];
const RED: [u8; 4] = [242, 54, 69, 255];
const PINK: [u8; 4] = [233, 30, 99, 255];
const PURPLE: [u8; 4] = [156, 39, 176, 255];

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

/// One line of a Gann tool: a price or time level, a fan line or an arc. The [`GannConfig`]
/// field that holds it says how its value is read.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "SavedGannLevel")]
pub struct GannLevel {
    /// Where the line lies, in the unit of the field that holds it.
    pub value: f32,
    /// Whether the line, its label and the band it closes are drawn.
    pub visible: bool,
    /// Line, label and band colour as RGBA.
    pub color: [u8; 4],
}

impl GannLevel {
    /// A visible level.
    pub fn new(value: f32, color: [u8; 4]) -> Self {
        Self {
            value,
            visible: true,
            color,
        }
    }
}

/// A saved level: a full entry, or a bare value as drawings saved before levels had a
/// visibility and colour hold it.
#[derive(Deserialize)]
#[serde(untagged)]
enum SavedGannLevel {
    Level {
        value: f32,
        visible: bool,
        color: [u8; 4],
    },
    Value(f32),
}

impl From<SavedGannLevel> for GannLevel {
    fn from(saved: SavedGannLevel) -> Self {
        match saved {
            SavedGannLevel::Level {
                value,
                visible,
                color,
            } => Self {
                value,
                visible,
                color,
            },
            // The drawing colour is not known here, so older values take the neutral level colour.
            SavedGannLevel::Value(value) => Self::new(value, GREY),
        }
    }
}

/// Levels, fan lines and arcs of the Gann tools, and the background between them.
///
/// A field missing from a saved object takes the value of [`GannConfig::for_tool`] for the tool
/// of the [`Drawing`](crate::drawings::Drawing) that holds it, or of [`GannConfig::default`]
/// when the object is read on its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GannConfig {
    /// Price levels counted from the first anchor: for a Gann box a fraction of its height from
    /// 0 to 1, for a Gann square or fixed square units from 0 to 5. Others are not drawn.
    #[serde(default = "left_out_price_levels")]
    pub price_levels: Vec<GannLevel>,
    /// Time levels counted from the first anchor, read as `price_levels` are.
    #[serde(default = "left_out_time_levels")]
    pub time_levels: Vec<GannLevel>,
    /// Whether a Gann square draws its price and time levels.
    pub show_grid: bool,
    /// Whether a Gann square draws its fan lines.
    pub show_fan: bool,
    /// Fan lines of a Gann square, and the lines of a Gann fan: price units per time unit, where
    /// 1 is the 1x1 line. Values must be positive and finite.
    #[serde(default = "left_out_fan_ratios")]
    pub fan_ratios: Vec<GannLevel>,
    /// The corners the fan lines of a Gann square start from.
    pub fan_direction: GannFanDirection,
    /// Whether a Gann square draws its arcs.
    pub show_arcs: bool,
    /// Arcs of a Gann square around its first anchor, as radii in units. Values must be positive
    /// and finite; the part outside the square is not drawn.
    pub arc_ratios: Vec<GannLevel>,
    /// Whether bands are tinted: between the levels of a box, the arcs of a square and the
    /// lines of a fan, each in the colour of the band's outer line.
    pub show_background: bool,
    /// Band opacity, from 0 to 255.
    pub background_opacity: u8,
    /// Whether a Gann box draws rays from its corners to the level points on its far edges.
    pub show_angles: bool,
}

thread_local! {
    /// The fields whose default differs between the Gann tools that the last [`GannConfig`] read
    /// on this thread left out: its price levels, time levels and fan lines.
    static LEFT_OUT: Cell<[bool; 3]> = const { Cell::new([false; 3]) };
}

/// Notes that the configuration being read left out `field`, and gives its [`Default`] value.
fn left_out<T>(field: usize, value: fn(GannConfig) -> T) -> T {
    LEFT_OUT.with(|left_out| {
        let mut fields = left_out.get();
        fields[field] = true;
        left_out.set(fields);
    });
    value(GannConfig::default())
}

fn left_out_price_levels() -> Vec<GannLevel> {
    left_out(0, |config| config.price_levels)
}

fn left_out_time_levels() -> Vec<GannLevel> {
    left_out(1, |config| config.time_levels)
}

fn left_out_fan_ratios() -> Vec<GannLevel> {
    left_out(2, |config| config.fan_ratios)
}

fn levels(entries: &[(f32, [u8; 4])]) -> Vec<GannLevel> {
    entries
        .iter()
        .map(|&(value, color)| GannLevel::new(value, color))
        .collect()
}

impl Default for GannConfig {
    /// The Gann box's configuration, as [`GannConfig::for_tool`] gives it. A Gann square, fixed
    /// square or fan reads its levels or fan lines differently and starts from `for_tool` of its
    /// own tool.
    fn default() -> Self {
        let box_levels = levels(&[
            (0.0, GREY),
            (0.25, ORANGE),
            (0.382, CYAN),
            (0.5, GREEN),
            (0.618, TEAL),
            (0.75, BLUE),
            (1.0, GREY),
        ]);
        let mut fan_ratios = levels(&[
            (8.0, GREY),
            (5.0, GREY),
            (4.0, GREY),
            (3.0, GREY),
            (2.0, CYAN),
            (1.0, GREEN),
            (0.5, TEAL),
            (1.0 / 3.0, GREY),
            (0.25, GREY),
            (0.2, GREY),
            (0.125, GREY),
        ]);
        for line in &mut fan_ratios {
            line.visible = matches!(line.value, 2.0 | 1.0 | 0.5);
        }
        Self {
            price_levels: box_levels.clone(),
            time_levels: box_levels,
            show_grid: true,
            show_fan: true,
            fan_ratios,
            fan_direction: GannFanDirection::default(),
            show_arcs: true,
            arc_ratios: [ORANGE, CYAN, GREEN, TEAL, BLUE]
                .into_iter()
                .zip(1..=5)
                .flat_map(|(color, n)| {
                    let n = n as f32;
                    [
                        GannLevel::new(n, color),
                        GannLevel::new((n * n + 1.0).sqrt(), color),
                    ]
                })
                .collect(),
            show_background: true,
            background_opacity: 51,
            show_angles: false,
        }
    }
}

impl GannConfig {
    /// The levels, fan lines and arcs a new Gann box, square, fixed square or fan starts from,
    /// which a drawing without [`Drawing::gann`](crate::drawings::Drawing::gann) draws. Start a
    /// configuration from the drawing's own tool here rather than from
    /// [`GannConfig::default`], which is the Gann box's.
    pub fn for_tool(tool: DrawingToolType) -> Self {
        match tool {
            DrawingToolType::GannSquare | DrawingToolType::GannFixed => {
                let units = levels(&[
                    (0.0, GREY),
                    (1.0, ORANGE),
                    (2.0, CYAN),
                    (3.0, GREEN),
                    (4.0, TEAL),
                    (5.0, GREY),
                ]);
                Self {
                    price_levels: units.clone(),
                    time_levels: units,
                    ..Self::default()
                }
            }
            DrawingToolType::GannFan => Self {
                fan_ratios: levels(&[
                    (8.0, RED),
                    (4.0, PINK),
                    (3.0, PURPLE),
                    (2.0, BLUE),
                    (1.0, CYAN),
                    (0.5, TEAL),
                    (1.0 / 3.0, GREEN),
                    (0.25, TEAL),
                    (0.125, ORANGE),
                ]),
                ..Self::default()
            },
            _ => Self::default(),
        }
    }

    /// Forgets the fields an earlier configuration read on this thread left out, before a
    /// drawing that may hold one is read.
    pub(crate) fn forget_left_out() {
        LEFT_OUT.with(|left_out| left_out.set([false; 3]));
    }

    /// Takes the fields that this configuration, the last one read on this thread, left out from
    /// [`GannConfig::for_tool`] of `tool` instead of [`GannConfig::default`].
    pub(crate) fn fill_left_out(&mut self, tool: DrawingToolType) {
        let [price_levels, time_levels, fan_ratios] = LEFT_OUT.with(|l| l.replace([false; 3]));
        if !(price_levels || time_levels || fan_ratios) {
            return;
        }
        let defaults = Self::for_tool(tool);
        if price_levels {
            self.price_levels = defaults.price_levels;
        }
        if time_levels {
            self.time_levels = defaults.time_levels;
        }
        if fan_ratios {
            self.fan_ratios = defaults.fan_ratios;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaults a new tool draws: the square's three fan lines on, its arcs at n and the
    /// square root of n² + 1 units, and the fan's nine lines. Radii by hand: √2, √5, √10, √17, √26.
    #[test]
    fn new_tools_start_from_their_defaults() {
        let square = GannConfig::for_tool(DrawingToolType::GannSquare);
        let visible: Vec<f32> = square
            .fan_ratios
            .iter()
            .filter(|l| l.visible)
            .map(|l| l.value)
            .collect();
        assert_eq!(visible, [2.0, 1.0, 0.5]);
        assert_eq!(square.fan_ratios.len(), 11);
        let radii = [
            1.0, SQRT_2, 2.0, 2.236068, 3.0, 3.1622777, 4.0, 4.1231055, 5.0, 5.0990195,
        ];
        assert_eq!(square.arc_ratios.len(), radii.len());
        for (arc, radius) in square.arc_ratios.iter().zip(radii) {
            assert!((arc.value - radius).abs() < 1e-5, "{arc:?} != {radius}");
        }
        let fan = GannConfig::for_tool(DrawingToolType::GannFan);
        assert_eq!(fan.fan_ratios.iter().filter(|l| l.visible).count(), 9);
    }
}
