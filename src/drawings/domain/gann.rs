//! Saved levels, fan lines, arcs and background of the Gann tools.

use super::DrawingToolType;
use crate::model::LineStyle;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::f32::consts::SQRT_2;

/// Units per side of a Gann square and of a fixed Gann square.
pub(crate) const GANN_SQUARE_UNITS: f32 = 5.0;
/// An arc of this radius in units, or more, lies wholly outside a Gann square.
pub(crate) const GANN_MAX_ARC: f32 = GANN_SQUARE_UNITS * SQRT_2;

const GREY: [u8; 4] = [128, 128, 128, 255];
const LAVENDER: [u8; 4] = [179, 157, 219, 255];
const VIOLET: [u8; 4] = [149, 117, 205, 255];
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
#[non_exhaustive]
pub struct GannLevel {
    /// Where the line lies, in the unit of the field that holds it.
    pub value: f32,
    /// Whether the line, its label and the band it closes are drawn.
    pub visible: bool,
    /// Line, label and band colour as RGBA, unless [`GannConfig::one_color`] is set.
    pub color: [u8; 4],
    /// Stroke width in points. `None` takes the drawing's width, as drawings saved before levels
    /// had a width do.
    pub width: Option<f32>,
    /// Pattern of a Gann fan line. `None` takes the drawing's style, as older saves do.
    /// Other Gann tools keep the drawing's style for their levels, fan lines and arcs.
    pub line_style: Option<LineStyle>,
}

impl GannLevel {
    /// A visible level.
    pub fn new(value: f32, color: [u8; 4]) -> Self {
        Self {
            value,
            visible: true,
            color,
            width: None,
            line_style: None,
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
        #[serde(default)]
        width: Option<f32>,
        #[serde(default)]
        line_style: Option<LineStyle>,
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
                width,
                line_style,
            } => Self {
                value,
                visible,
                color,
                width,
                line_style,
            },
            // The drawing colour is not known here, so older values take the neutral level colour.
            SavedGannLevel::Value(value) => Self::new(value, GREY),
        }
    }
}

/// Levels, fan lines and arcs of the Gann tools, and the background between them.
///
/// Fields missing from a saved drawing take its former defaults: squares keep ten arcs and
/// grey hidden fan lines. New squares store the lists from [`GannConfig::for_tool`]. An object
/// read on its own takes [`GannConfig::default`].
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
    /// Whether labels are drawn: a box's level values on the enabled sides, a fan's ratios
    /// at the pane edge, or a square's price range, bar count and price per bar beside it.
    /// On by default except for the fixed square, including saves without this field.
    #[serde(default = "left_out_show_labels")]
    pub show_labels: bool,
    /// Whether a Gann box, Gann square or fixed Gann square is mirrored inside its anchors: its
    /// levels, fan lines and arcs are counted from the corner opposite the first anchor, and
    /// [`fan_direction`](Self::fan_direction) is read from there. A Gann fan follows its anchors.
    pub reverse: bool,
    /// A colour as RGBA that every line, band and label of the tool takes instead of the level
    /// colours, which are kept for when it is cleared. Labels drawn on a box, the fan's ratios
    /// and a square's label, take it as the box's fill under white or black text,
    /// whichever reads on it. `None` draws the level colours.
    pub one_color: Option<[u8; 4]>,
    /// Whether a Gann box labels its price levels on its left side.
    pub labels_left: bool,
    /// Whether a Gann box labels its price levels on its right side.
    pub labels_right: bool,
    /// Whether a Gann box labels its time levels above it.
    pub labels_top: bool,
    /// Whether a Gann box labels its time levels below it.
    pub labels_bottom: bool,
    /// Whether a Gann box tints bands between time levels. `None` follows
    /// [`show_background`](Self::show_background), preserving older saves. Price bands
    /// always follow `show_background`.
    pub show_time_background: Option<bool>,
    /// Time band opacity from 0 to 255. `None` follows
    /// [`background_opacity`](Self::background_opacity).
    pub time_background_opacity: Option<u8>,
    /// Label size in points for a Gann square or fixed square, from 1 to 256.
    pub label_size: f32,
    /// Whether a square's label is bold.
    pub label_bold: bool,
    /// Whether a square's label is italic.
    pub label_italic: bool,
}

thread_local! {
    /// The fields whose default differs between the Gann tools that the last [`GannConfig`] read
    /// on this thread left out: its price levels, time levels, fan lines and labels switch.
    static LEFT_OUT: Cell<[bool; 4]> = const { Cell::new([false; 4]) };
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

fn left_out_show_labels() -> bool {
    left_out(3, |config| config.show_labels)
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
            show_labels: true,
            reverse: false,
            one_color: None,
            labels_left: true,
            labels_right: false,
            labels_top: false,
            labels_bottom: true,
            show_time_background: None,
            time_background_opacity: None,
            label_size: 10.0,
            label_bold: false,
            label_italic: false,
        }
    }
}

impl GannConfig {
    /// Settings a new Gann drawing starts with: a box labels all four sides, and a square or
    /// fixed square uses a 12 point regular, upright label. The fixed square's label stays off.
    /// Saved drawings without settings keep their former defaults instead.
    pub fn drawing_default(tool: DrawingToolType) -> Self {
        let mut config = Self::for_tool(tool);
        match tool {
            DrawingToolType::GannBox => {
                config.labels_right = true;
                config.labels_top = true;
            }
            DrawingToolType::GannSquare | DrawingToolType::GannFixed => config.label_size = 12.0,
            _ => {}
        }
        config
    }

    /// The levels, fan lines and arcs a new Gann box, square, fixed square or fan starts from.
    /// Start a configuration from the drawing's own tool here rather than from
    /// [`GannConfig::default`], which is the Gann box's. A Gann square or fixed square adds an
    /// arc at 1.5 units and colours each fan line.
    pub fn for_tool(tool: DrawingToolType) -> Self {
        let mut config = Self::saved_default(tool);
        if matches!(
            tool,
            DrawingToolType::GannSquare | DrawingToolType::GannFixed
        ) {
            config.fan_ratios = levels(&[
                (8.0, LAVENDER),
                (5.0, RED),
                (4.0, GREY),
                (3.0, ORANGE),
                (2.0, CYAN),
                (1.0, GREEN),
                (0.5, TEAL),
                (1.0 / 3.0, TEAL),
                (0.25, BLUE),
                (0.2, VIOLET),
                (0.125, LAVENDER),
            ]);
            for line in &mut config.fan_ratios {
                line.visible = matches!(line.value, 2.0 | 1.0 | 0.5);
            }
            // 1.5 units, in the unit-1 colour, between the 1x1 arc (√2) and the 2x0 arc.
            config.arc_ratios.insert(2, GannLevel::new(1.5, ORANGE));
        }
        config
    }

    /// Defaults for saved drawings and templates: boxes keep left and bottom labels, squares
    /// keep 10 point labels, ten arcs and grey hidden fan lines, and fixed labels stay off.
    pub(crate) fn saved_default(tool: DrawingToolType) -> Self {
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
                    show_labels: tool != DrawingToolType::GannFixed,
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
        LEFT_OUT.with(|left_out| left_out.set([false; 4]));
    }

    /// Takes missing fields from the saved defaults of `tool`, preserving its former lists.
    pub(crate) fn fill_left_out(&mut self, tool: DrawingToolType) {
        let [price_levels, time_levels, fan_ratios, show_labels] =
            LEFT_OUT.with(|l| l.replace([false; 4]));
        if !(price_levels || time_levels || fan_ratios || show_labels) {
            return;
        }
        let defaults = Self::saved_default(tool);
        if price_levels {
            self.price_levels = defaults.price_levels;
        }
        if time_levels {
            self.time_levels = defaults.time_levels;
        }
        if fan_ratios {
            self.fan_ratios = defaults.fan_ratios;
        }
        if show_labels {
            self.show_labels = defaults.show_labels;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The square and fixed square's coloured fan lines, with three on, and eleven arcs at n,
    /// √(n² + 1) and 1.5 units. Radii by hand: √2, √5, √10, √17, √26. The fan keeps nine lines.
    #[test]
    fn new_tools_start_from_their_defaults() {
        let square = GannConfig::for_tool(DrawingToolType::GannSquare);
        let fans: Vec<_> = square
            .fan_ratios
            .iter()
            .map(|l| (l.value, l.color, l.visible))
            .collect();
        assert_eq!(
            fans,
            [
                (8.0, [179, 157, 219, 255], false),
                (5.0, [242, 54, 69, 255], false),
                (4.0, [128, 128, 128, 255], false),
                (3.0, [255, 152, 0, 255], false),
                (2.0, [0, 188, 212, 255], true),
                (1.0, [76, 175, 80, 255], true),
                (0.5, [8, 153, 129, 255], true),
                (1.0 / 3.0, [8, 153, 129, 255], false),
                (0.25, [41, 98, 255, 255], false),
                (0.2, [149, 117, 205, 255], false),
                (0.125, [179, 157, 219, 255], false),
            ]
        );
        let radii = [
            1.0, SQRT_2, 1.5, 2.0, 2.236068, 3.0, 3.1622777, 4.0, 4.1231055, 5.0, 5.0990195,
        ];
        let colours = [
            [255, 152, 0, 255],
            [255, 152, 0, 255],
            [255, 152, 0, 255],
            [0, 188, 212, 255],
            [0, 188, 212, 255],
            [76, 175, 80, 255],
            [76, 175, 80, 255],
            [8, 153, 129, 255],
            [8, 153, 129, 255],
            [41, 98, 255, 255],
            [41, 98, 255, 255],
        ];
        assert_eq!(square.arc_ratios.len(), radii.len());
        for ((arc, radius), colour) in square.arc_ratios.iter().zip(radii).zip(colours) {
            assert!((arc.value - radius).abs() < 1e-5, "{arc:?} != {radius}");
            assert_eq!(arc.color, colour);
            assert!(arc.visible);
        }
        assert_eq!(GannConfig::for_tool(DrawingToolType::GannFixed), square);
        let fan = GannConfig::for_tool(DrawingToolType::GannFan);
        assert_eq!(fan.fan_ratios.iter().filter(|l| l.visible).count(), 9);
    }
}
