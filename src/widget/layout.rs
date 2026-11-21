//! Tiled and tabbed charts with independent state and timestamp-based synchronization.

use super::keyboard::ShortcutRequest;
use super::{Chart, ShortcutMap, VisibleTimeRange};
use crate::config::KeyboardAction;
use crate::localization as l10n;
use crate::model::Timeframe;
use crate::theme::ThemeExt;
use egui::{Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::sync::{Arc, Mutex};

/// Narrowest span from the plot's left edge to the chart's right edge that shows the field
/// selectors in the header. The title controls need 8 points of status line inset, 116 of their
/// own and 228 from the chart's right edge to the end of the first row; a narrower chart leaves
/// the selectors to its corner menu and shows its group markers alone.
const LINK_CONTROLS_MIN_WIDTH: f32 = 352.0;
/// Distance of the selectors from the chart's right edge, which ends the status line's first row.
pub(super) const LINK_CONTROLS_INSET: f32 = 220.0;
/// Most charts a layout serves: the slots of the largest preset, and of tabs.
pub(super) const MAX_CHARTS: usize = 16;

/// The keys a layout of more than one chart takes: Alt+Enter in grids, Tab in every preset.
const LAYOUT_KEYS: &[KeyboardAction] = &[
    KeyboardAction::MaximizeChart,
    KeyboardAction::CycleChartFocus,
    KeyboardAction::CycleChartFocusBack,
];

const LINK_MARKERS: [(&str, egui::Color32); 5] = [
    ("🔶", egui::Color32::from_rgb(240, 150, 0)),
    ("🍀", egui::Color32::from_rgb(110, 160, 70)),
    ("💧", egui::Color32::from_rgb(70, 170, 225)),
    ("❤️", egui::Color32::from_rgb(225, 65, 80)),
    ("💩", egui::Color32::from_rgb(160, 105, 90)),
];

/// Paints `chart`'s background over `rect`, the container a chart with `skip_background` set
/// leaves its background to.
fn slot_background(ui: &Ui, rect: Rect, chart: &Chart) {
    let config = chart.config();
    super::frame::paint_background(
        ui.painter(),
        rect,
        rect,
        config.background_style,
        config.background_color,
    );
}

// These markers must also work in hosts without emoji fonts.
fn paint_link_marker(ui: &Ui, rect: Rect, marker: &str) {
    let Some(index) = LINK_MARKERS.iter().position(|(name, _)| *name == marker) else {
        return;
    };
    let color = LINK_MARKERS[index].1;
    let point = |x, y| rect.min + Vec2::new(x * rect.width(), y * rect.height());
    let painter = ui.painter();
    let polygon = |points: &[(f32, f32)]| {
        painter.add(egui::Shape::convex_polygon(
            points.iter().map(|&(x, y)| point(x, y)).collect(),
            color,
            Stroke::NONE,
        ));
    };
    match index {
        0 => polygon(&[(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)]),
        1 => {
            for (x, y) in [(0.3, 0.3), (0.7, 0.3), (0.3, 0.7), (0.7, 0.7)] {
                painter.circle_filled(point(x, y), rect.width() * 0.25, color);
            }
            painter.line_segment([point(0.5, 0.5), point(0.75, 1.0)], Stroke::new(2.0, color));
        }
        2 => polygon(&[
            (0.5, 0.0),
            (0.88, 0.6),
            (0.9, 0.78),
            (0.76, 0.95),
            (0.5, 1.0),
            (0.24, 0.95),
            (0.1, 0.78),
            (0.12, 0.6),
        ]),
        3 => {
            for x in [0.28, 0.72] {
                painter.circle_filled(point(x, 0.3), rect.width() * 0.28, color);
            }
            polygon(&[
                (0.0, 0.35),
                (1.0, 0.35),
                (0.9, 0.65),
                (0.5, 1.0),
                (0.1, 0.65),
            ]);
        }
        _ => {
            for (inset, y) in [(0.3, 0.05), (0.15, 0.3), (0.0, 0.55)] {
                painter.rect_filled(
                    Rect::from_min_max(point(inset, y), point(1.0 - inset, y + 0.4)),
                    rect.width() * 0.2,
                    color,
                );
            }
            for x in [0.35, 0.65] {
                painter.circle_filled(point(x, 0.62), rect.width() * 0.1, egui::Color32::WHITE);
                painter.circle_filled(point(x, 0.64), rect.width() * 0.05, egui::Color32::BLACK);
            }
            painter.line_segment(
                [point(0.35, 0.83), point(0.65, 0.83)],
                Stroke::new(1.0, egui::Color32::BLACK),
            );
        }
    }
}

#[cfg(feature = "studies")]
mod copy;
#[cfg(feature = "studies")]
pub use copy::{LayoutCopy, LayoutCopyIssue, LayoutCopyIssueKind};
#[cfg(feature = "drawings")]
mod drawings;
#[cfg(feature = "drawings")]
use drawings::DrawingLinks;
#[cfg(feature = "drawings")]
pub(super) use drawings::UnshownDeletion;
#[cfg(feature = "drawings")]
pub use drawings::{SymbolDrawing, SymbolDrawings};

/// Multi-chart arrangement. Changing presets retains the host's hidden charts. Row arrangements
/// fill from the top, column arrangements from the left. Independently divided halves fill the
/// left or top half first, following its rows or columns before filling the other half.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[non_exhaustive]
pub enum ChartLayoutMode {
    /// One chart.
    #[default]
    Single,
    /// Two columns.
    TwoHorizontal,
    /// Two rows.
    TwoVertical,
    /// Three charts, with the last spanning the bottom row.
    Three,
    /// Three columns.
    ThreeHorizontal,
    /// Three rows.
    ThreeVertical,
    /// One chart in the left half, two stacked in the right half.
    ThreeLargeLeft,
    /// Two charts stacked in the left half, one in the right half.
    ThreeLargeRight,
    /// One chart in the top half, two side by side in the bottom half; [`Three`](Self::Three) is
    /// the reverse.
    ThreeLargeTop,
    /// Two columns and two rows.
    Four,
    /// Four columns.
    FourHorizontal,
    /// Four rows.
    FourVertical,
    /// One chart in the left half, three stacked in the right half.
    FourLargeLeft,
    /// Three charts stacked in the left half, one in the right half.
    FourLargeRight,
    /// One chart in the top half, three side by side in the bottom half.
    FourLargeTop,
    /// Three charts side by side in the top half, one in the bottom half.
    FourLargeBottom,
    /// Two columns in the left half, two rows in the right half.
    FourSplitLeft,
    /// Two rows in the left half, two columns in the right half.
    FourSplitRight,
    /// Two columns in the top half, two rows in the bottom half.
    FourSplitBottom,
    /// Five charts in three columns.
    Five,
    /// Five columns.
    FiveHorizontal,
    /// Five rows.
    FiveVertical,
    /// One chart in the left half, four stacked in the right half.
    FiveLargeLeft,
    /// Four charts stacked in the left half, one in the right half.
    FiveLargeRight,
    /// One chart in the top half, four side by side in the bottom half.
    FiveLargeTop,
    /// Four charts side by side in the top half, one in the bottom half.
    FiveLargeBottom,
    /// Two columns in the left half, three rows in the right half.
    FiveSplitLeft,
    /// Three rows in the left half, two columns in the right half.
    FiveSplitRight,
    /// Two charts in the top half, three in the bottom half.
    FiveTwoTop,
    /// Three columns and two rows.
    Six,
    /// Two columns and three rows.
    SixTwoColumns,
    /// Six columns.
    SixHorizontal,
    /// Six rows.
    SixVertical,
    /// One chart in the left half, five stacked in the right half.
    SixLargeLeft,
    /// Five charts stacked in the left half, one in the right half.
    SixLargeRight,
    /// One chart in the top half, five side by side in the bottom half.
    SixLargeTop,
    /// Five charts side by side in the top half, one in the bottom half.
    SixLargeBottom,
    /// Two charts in the top half, four in the bottom half.
    SixTwoTop,
    /// Four charts in the top half, two in the bottom half.
    SixFourTop,
    /// Seven charts in four columns.
    Seven,
    /// Seven columns.
    SevenHorizontal,
    /// One chart in the left half, six stacked in the right half.
    SevenLargeLeft,
    /// Four columns and two rows.
    Eight,
    /// Eight rows.
    EightVertical,
    /// Two columns and four rows.
    EightTwoColumns,
    /// Eight columns.
    EightHorizontal,
    /// Three columns and three rows.
    Nine,
    /// Nine rows.
    NineVertical,
    /// Nine columns.
    NineHorizontal,
    /// Five charts in the top half, four in the bottom half.
    NineFiveTop,
    /// Ten charts in four columns.
    Ten,
    /// Ten columns.
    TenHorizontal,
    /// Ten rows.
    TenVertical,
    /// Five columns and two rows.
    TenFiveColumns,
    /// Eleven charts in four columns.
    Eleven,
    /// Four columns and three rows.
    Twelve,
    /// Twelve columns.
    TwelveHorizontal,
    /// Three columns and four rows.
    TwelveThreeColumns,
    /// Six columns and two rows.
    TwelveSixColumns,
    /// Thirteen charts in four columns.
    Thirteen,
    /// Fourteen charts in four columns.
    Fourteen,
    /// Seven columns and two rows.
    FourteenSevenColumns,
    /// Fifteen charts in four columns.
    Fifteen,
    /// Four columns and four rows.
    Sixteen,
    /// Eight columns and two rows.
    SixteenEightColumns,
    /// One tab per supplied chart, up to sixteen.
    Tabs,
}

impl ChartLayoutMode {
    /// All selectable presets, by chart count.
    pub fn all() -> &'static [Self] {
        &[
            Self::Single,
            Self::TwoHorizontal,
            Self::TwoVertical,
            Self::Three,
            Self::ThreeHorizontal,
            Self::ThreeVertical,
            Self::ThreeLargeLeft,
            Self::ThreeLargeRight,
            Self::ThreeLargeTop,
            Self::Four,
            Self::FourHorizontal,
            Self::FourVertical,
            Self::FourLargeLeft,
            Self::FourLargeRight,
            Self::FourLargeTop,
            Self::FourLargeBottom,
            Self::FourSplitLeft,
            Self::FourSplitRight,
            Self::FourSplitBottom,
            Self::Five,
            Self::FiveHorizontal,
            Self::FiveVertical,
            Self::FiveLargeLeft,
            Self::FiveLargeRight,
            Self::FiveLargeTop,
            Self::FiveLargeBottom,
            Self::FiveSplitLeft,
            Self::FiveSplitRight,
            Self::FiveTwoTop,
            Self::Six,
            Self::SixTwoColumns,
            Self::SixHorizontal,
            Self::SixVertical,
            Self::SixLargeLeft,
            Self::SixLargeRight,
            Self::SixLargeTop,
            Self::SixLargeBottom,
            Self::SixTwoTop,
            Self::SixFourTop,
            Self::Seven,
            Self::SevenHorizontal,
            Self::SevenLargeLeft,
            Self::Eight,
            Self::EightVertical,
            Self::EightTwoColumns,
            Self::EightHorizontal,
            Self::Nine,
            Self::NineVertical,
            Self::NineHorizontal,
            Self::NineFiveTop,
            Self::Ten,
            Self::TenHorizontal,
            Self::TenVertical,
            Self::TenFiveColumns,
            Self::Eleven,
            Self::Twelve,
            Self::TwelveHorizontal,
            Self::TwelveThreeColumns,
            Self::TwelveSixColumns,
            Self::Thirteen,
            Self::Fourteen,
            Self::FourteenSevenColumns,
            Self::Fifteen,
            Self::Sixteen,
            Self::SixteenEightColumns,
            Self::Tabs,
        ]
    }

    /// Human-readable preset name, distinct for every preset.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::TwoHorizontal => "2 Horizontal",
            Self::TwoVertical => "2 Vertical",
            Self::Three => "2 Top, 1 Bottom",
            Self::ThreeHorizontal => "3 Horizontal",
            Self::ThreeVertical => "3 Vertical",
            Self::ThreeLargeLeft => "1 Left, 2 Right",
            Self::ThreeLargeRight => "2 Left, 1 Right",
            Self::ThreeLargeTop => "1 Top, 2 Bottom",
            Self::Four => "2x2 Grid",
            Self::FourHorizontal => "4 Horizontal",
            Self::FourVertical => "4 Vertical",
            Self::FourLargeLeft => "1 Left, 3 Right",
            Self::FourLargeRight => "3 Left, 1 Right",
            Self::FourLargeTop => "1 Top, 3 Bottom",
            Self::FourLargeBottom => "3 Top, 1 Bottom",
            Self::FourSplitLeft => "2 Columns Left, 2 Rows Right",
            Self::FourSplitRight => "2 Rows Left, 2 Columns Right",
            Self::FourSplitBottom => "2 Columns Top, 2 Rows Bottom",
            Self::Five => "3 Top, 2 Bottom",
            Self::FiveHorizontal => "5 Horizontal",
            Self::FiveVertical => "5 Vertical",
            Self::FiveLargeLeft => "1 Left, 4 Right",
            Self::FiveLargeRight => "4 Left, 1 Right",
            Self::FiveLargeTop => "1 Top, 4 Bottom",
            Self::FiveLargeBottom => "4 Top, 1 Bottom",
            Self::FiveSplitLeft => "2 Columns Left, 3 Rows Right",
            Self::FiveSplitRight => "3 Rows Left, 2 Columns Right",
            Self::FiveTwoTop => "2 Top, 3 Bottom",
            Self::Six => "3x2 Grid",
            Self::SixTwoColumns => "2x3 Grid",
            Self::SixHorizontal => "6 Horizontal",
            Self::SixVertical => "6 Vertical",
            Self::SixLargeLeft => "1 Left, 5 Right",
            Self::SixLargeRight => "5 Left, 1 Right",
            Self::SixLargeTop => "1 Top, 5 Bottom",
            Self::SixLargeBottom => "5 Top, 1 Bottom",
            Self::SixTwoTop => "2 Top, 4 Bottom",
            Self::SixFourTop => "4 Top, 2 Bottom",
            Self::Seven => "7 Charts",
            Self::SevenHorizontal => "7 Horizontal",
            Self::SevenLargeLeft => "1 Left, 6 Right",
            Self::Eight => "4x2 Grid",
            Self::EightVertical => "8 Vertical",
            Self::EightTwoColumns => "2x4 Grid",
            Self::EightHorizontal => "8 Horizontal",
            Self::Nine => "3x3 Grid",
            Self::NineVertical => "9 Vertical",
            Self::NineHorizontal => "9 Horizontal",
            Self::NineFiveTop => "5 Top, 4 Bottom",
            Self::Ten => "10 Charts",
            Self::TenHorizontal => "10 Horizontal",
            Self::TenVertical => "10 Vertical",
            Self::TenFiveColumns => "5x2 Grid",
            Self::Eleven => "11 Charts",
            Self::Twelve => "4x3 Grid",
            Self::TwelveHorizontal => "12 Horizontal",
            Self::TwelveThreeColumns => "3x4 Grid",
            Self::TwelveSixColumns => "6x2 Grid",
            Self::Thirteen => "13 Charts",
            Self::Fourteen => "14 Charts",
            Self::FourteenSevenColumns => "7x2 Grid",
            Self::Fifteen => "15 Charts",
            Self::Sixteen => "4x4 Grid",
            Self::SixteenEightColumns => "8x2 Grid",
            Self::Tabs => "Tabs",
        }
    }

    /// Number of charts in a preset; tabs can show up to sixteen supplied charts.
    pub fn chart_count(&self) -> usize {
        if *self == Self::Tabs {
            return MAX_CHARTS;
        }
        self.tracks().chart_count()
    }

    /// Maximum columns and rows, counting subdivisions within each half. A row or column with
    /// fewer cells than another stretches them to fill it.
    pub fn grid_size(&self) -> (usize, usize) {
        self.tracks().grid_size()
    }

    /// How the preset divides its rectangle. Tabs show one chart at a time in one cell.
    fn tracks(&self) -> Tracks {
        use Tracks::{Columns, Halves, Rows};
        match self {
            Self::Single | Self::Tabs => Rows(&[1]),
            Self::TwoHorizontal => Rows(&[2]),
            Self::TwoVertical => Rows(&[1, 1]),
            Self::Three => Rows(&[2, 1]),
            Self::ThreeHorizontal => Rows(&[3]),
            Self::ThreeVertical => Rows(&[1, 1, 1]),
            Self::ThreeLargeLeft => Columns(&[1, 2]),
            Self::ThreeLargeRight => Columns(&[2, 1]),
            Self::ThreeLargeTop => Rows(&[1, 2]),
            Self::Four => Rows(&[2, 2]),
            Self::FourHorizontal => Rows(&[4]),
            Self::FourVertical => Rows(&[1, 1, 1, 1]),
            Self::FourLargeLeft => Columns(&[1, 3]),
            Self::FourLargeRight => Columns(&[3, 1]),
            Self::FourLargeTop => Rows(&[1, 3]),
            Self::FourLargeBottom => Rows(&[3, 1]),
            Self::FourSplitLeft => Halves {
                by_rows: false,
                halves: [&Rows(&[2]), &Columns(&[2])],
            },
            Self::FourSplitRight => Halves {
                by_rows: false,
                halves: [&Columns(&[2]), &Rows(&[2])],
            },
            Self::FourSplitBottom => Halves {
                by_rows: true,
                halves: [&Rows(&[2]), &Columns(&[2])],
            },
            Self::Five => Rows(&[3, 2]),
            Self::FiveHorizontal => Rows(&[5]),
            Self::FiveVertical => Rows(&[1, 1, 1, 1, 1]),
            Self::FiveLargeLeft => Columns(&[1, 4]),
            Self::FiveLargeRight => Columns(&[4, 1]),
            Self::FiveLargeTop => Rows(&[1, 4]),
            Self::FiveLargeBottom => Rows(&[4, 1]),
            Self::FiveSplitLeft => Halves {
                by_rows: false,
                halves: [&Rows(&[2]), &Columns(&[3])],
            },
            Self::FiveSplitRight => Halves {
                by_rows: false,
                halves: [&Columns(&[3]), &Rows(&[2])],
            },
            Self::FiveTwoTop => Rows(&[2, 3]),
            Self::Six => Rows(&[3, 3]),
            Self::SixTwoColumns => Rows(&[2, 2, 2]),
            Self::SixHorizontal => Rows(&[6]),
            Self::SixVertical => Rows(&[1, 1, 1, 1, 1, 1]),
            Self::SixLargeLeft => Columns(&[1, 5]),
            Self::SixLargeRight => Columns(&[5, 1]),
            Self::SixLargeTop => Rows(&[1, 5]),
            Self::SixLargeBottom => Rows(&[5, 1]),
            Self::SixTwoTop => Rows(&[2, 4]),
            Self::SixFourTop => Rows(&[4, 2]),
            Self::Seven => Rows(&[4, 3]),
            Self::SevenHorizontal => Rows(&[7]),
            Self::SevenLargeLeft => Columns(&[1, 6]),
            Self::Eight => Rows(&[4, 4]),
            Self::EightVertical => Rows(&[1, 1, 1, 1, 1, 1, 1, 1]),
            Self::EightTwoColumns => Rows(&[2, 2, 2, 2]),
            Self::EightHorizontal => Rows(&[8]),
            Self::Nine => Rows(&[3, 3, 3]),
            Self::NineVertical => Rows(&[1, 1, 1, 1, 1, 1, 1, 1, 1]),
            Self::NineHorizontal => Rows(&[9]),
            Self::NineFiveTop => Rows(&[5, 4]),
            Self::Ten => Rows(&[4, 4, 2]),
            Self::TenHorizontal => Rows(&[10]),
            Self::TenVertical => Rows(&[1, 1, 1, 1, 1, 1, 1, 1, 1, 1]),
            Self::TenFiveColumns => Rows(&[5, 5]),
            Self::Eleven => Rows(&[4, 4, 3]),
            Self::Twelve => Rows(&[4, 4, 4]),
            Self::TwelveHorizontal => Rows(&[12]),
            Self::TwelveThreeColumns => Rows(&[3, 3, 3, 3]),
            Self::TwelveSixColumns => Rows(&[6, 6]),
            Self::Thirteen => Rows(&[4, 4, 4, 1]),
            Self::Fourteen => Rows(&[4, 4, 4, 2]),
            Self::FourteenSevenColumns => Rows(&[7, 7]),
            Self::Fifteen => Rows(&[4, 4, 4, 3]),
            Self::Sixteen => Rows(&[4, 4, 4, 4]),
            Self::SixteenEightColumns => Rows(&[8, 8]),
        }
    }
}

impl std::fmt::Display for ChartLayoutMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// The cells of a preset: rows from the top or columns from the left, each holding the given
/// number of equal cells. A half can divide across the other axis independently. Charts fill one
/// row, column or half after the other.
enum Tracks {
    Rows(&'static [usize]),
    Columns(&'static [usize]),
    /// Two halves, stacked when `by_rows` and side by side otherwise, each divided on its own.
    Halves {
        by_rows: bool,
        halves: [&'static Tracks; 2],
    },
}

/// Relative sizes of one preset's splits, in depth-first order. Each split contains one positive
/// weight per child, from the top or left. Obtain the shape with [`ChartLayout::sizes`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ChartLayoutSizing {
    /// The arrangement these weights belong to.
    pub mode: ChartLayoutMode,
    /// Weights of each split; their sum must be finite and need not be one.
    pub weights: Vec<Vec<f32>>,
}

impl ChartLayoutSizing {
    /// The `weights` of the splits of `mode`, in the shape [`ChartLayout::sizes`] gives them.
    pub fn new(mode: ChartLayoutMode, weights: Vec<Vec<f32>>) -> Self {
        Self { mode, weights }
    }
}

#[derive(Default)]
struct SizingState {
    sizes: Vec<ChartLayoutSizing>,
    pending: Vec<SizeChange>,
    drag: Option<DividerDrag>,
    /// Held from the press on a divider until the release, so that an undo a host dispatches
    /// before `show` cannot run in the frame egui starts the drag.
    pressed: Option<Arc<()>>,
    selected: Option<(usize, usize)>,
}

impl Clone for SizingState {
    fn clone(&self) -> Self {
        Self {
            sizes: self.sizes.clone(),
            pending: self.pending.clone(),
            ..Self::default()
        }
    }
}

/// A split size step. Undo and redo queue it on the chart that holds it, and the layout that
/// shows that chart next applies it to its own sizes.
#[derive(Clone)]
pub(super) struct SizeChange {
    other: Vec<(ChartLayoutMode, Option<ChartLayoutSizing>)>,
}

impl SizingState {
    fn record(&mut self, before: Vec<ChartLayoutSizing>) {
        let after = &self.sizes;
        let mut other = Vec::new();
        for mode in before.iter().chain(after).map(|size| size.mode) {
            let old = before.iter().find(|size| size.mode == mode);
            let new = after.iter().find(|size| size.mode == mode);
            if old != new && !other.iter().any(|(previous, _)| *previous == mode) {
                other.push((mode, old.cloned()));
            }
        }
        if !other.is_empty() {
            self.pending.push(SizeChange { other });
        }
    }
}

impl SizeChange {
    fn swap(&mut self, sizes: &mut Vec<ChartLayoutSizing>) {
        for (mode, other) in &mut self.other {
            let current = sizes
                .iter()
                .position(|size| size.mode == *mode)
                .map(|at| sizes.remove(at));
            if let Some(previous) = other.take() {
                sizes.push(previous);
            }
            *other = current;
        }
    }
}

/// The recursive splits of a preset. Leaves hold one chart, including when a later slot is absent.
struct Split {
    rows: bool,
    children: Vec<Split>,
}

#[derive(Clone)]
struct Divider {
    group: usize,
    at: usize,
    rows: bool,
    rect: Rect,
    lengths: Vec<f32>,
    minimum: Vec<f32>,
}

impl Divider {
    fn coordinate(&self) -> f32 {
        self.rect.center()[usize::from(self.rows)]
    }

    fn limits(&self) -> (f32, f32) {
        (
            (self.minimum[self.at] - self.lengths[self.at]).min(0.0),
            (self.lengths[self.at + 1] - self.minimum[self.at + 1]).max(0.0),
        )
    }
}

struct DividerDrag {
    history_guard: Arc<()>,
    mode: ChartLayoutMode,
    count: usize,
    start: egui::Pos2,
    before: Vec<ChartLayoutSizing>,
    dividers: Vec<Divider>,
    weights: Vec<Vec<f32>>,
}

impl Split {
    fn leaf() -> Self {
        Self {
            rows: false,
            children: Vec::new(),
        }
    }

    fn weights(&self, weights: &mut Vec<Vec<f32>>) {
        if self.children.is_empty() {
            return;
        }
        weights.push(vec![1.0 / self.children.len() as f32; self.children.len()]);
        for child in &self.children {
            child.weights(weights);
        }
    }

    fn chart_count(&self) -> usize {
        if self.children.is_empty() {
            1
        } else {
            self.children.iter().map(Self::chart_count).sum()
        }
    }

    fn minimum(&self, count: usize) -> Vec2 {
        if count == 0 {
            return Vec2::ZERO;
        }
        if self.children.is_empty() {
            return Vec2::new(80.0, 60.0);
        }
        let axis = usize::from(self.rows);
        let mut minimum = Vec2::ZERO;
        let mut left = count;
        for child in &self.children {
            let used = left.min(child.chart_count());
            left -= used;
            if used > 0 {
                let size = child.minimum(used);
                if minimum[axis] > 0.0 && self.rows {
                    minimum[axis] += 2.0;
                }
                minimum[axis] += size[axis];
                minimum[1 - axis] = minimum[1 - axis].max(size[1 - axis]);
            }
        }
        minimum
    }

    fn rects(
        &self,
        rect: Rect,
        count: usize,
        weights: &[Vec<f32>],
        group: &mut usize,
        output: &mut (Vec<Rect>, Vec<Divider>),
        scale: Vec2,
    ) {
        if self.children.is_empty() {
            if count > 0 {
                output.0.push(rect);
            }
            return;
        }
        let current = *group;
        *group += 1;
        let mut left = count;
        let counts: Vec<_> = self
            .children
            .iter()
            .map(|child| {
                let used = left.min(child.chart_count());
                left -= used;
                used
            })
            .collect();
        let used = counts.iter().take_while(|&&count| count > 0).count();
        let axis = usize::from(self.rows);
        let gap = if self.rows {
            2.0_f32.min(rect.height() / used.max(1) as f32)
        } else {
            0.0
        };
        let available = (rect.size()[axis] - gap * used.saturating_sub(1) as f32).max(0.0);
        let mut minimum: Vec<_> = self
            .children
            .iter()
            .zip(&counts)
            .take(used)
            .map(|(child, &count)| child.minimum(count)[axis] * scale[axis])
            .collect();
        let required: f32 = minimum.iter().sum();
        if required > available {
            for size in &mut minimum {
                *size *= available / required;
            }
        }
        let mut lengths = vec![0.0; used];
        let mut remaining = available;
        let mut open: Vec<_> = (0..used).collect();
        // Freeze children below their minimum, then divide the remainder among the others.
        while !open.is_empty() {
            let sum: f32 = open.iter().map(|&i| weights[current][i]).sum();
            let frozen: Vec<_> = open
                .iter()
                .copied()
                .filter(|&i| remaining * (weights[current][i] / sum) < minimum[i])
                .collect();
            if frozen.is_empty() {
                for &i in &open {
                    lengths[i] = remaining * (weights[current][i] / sum);
                }
                break;
            }
            for i in frozen {
                lengths[i] = minimum[i];
                remaining = (remaining - minimum[i]).max(0.0);
                open.retain(|&j| j != i);
            }
        }
        let mut position = rect.min[axis];
        for (i, (child, &count)) in self.children.iter().zip(&counts).enumerate() {
            let mut bounds = rect;
            bounds.min[axis] = position;
            bounds.max[axis] = if i + 1 == used {
                rect.max[axis]
            } else {
                (position + lengths.get(i).copied().unwrap_or(0.0)).min(rect.max[axis])
            };
            child.rects(bounds, count, weights, group, output, scale);
            if i + 1 < used {
                let mut border = rect;
                border.min[axis] = bounds.max[axis] + gap / 2.0;
                border.max[axis] = border.min[axis];
                output.1.push(Divider {
                    group: current,
                    at: i,
                    rows: self.rows,
                    rect: border,
                    lengths: lengths.clone(),
                    minimum: minimum.clone(),
                });
            }
            position = bounds.max[axis] + gap;
        }
    }
}

impl Tracks {
    /// Charts the arrangement holds.
    fn chart_count(&self) -> usize {
        match self {
            Self::Rows(cells) | Self::Columns(cells) => cells.iter().sum(),
            Self::Halves { halves, .. } => halves.iter().map(|half| half.chart_count()).sum(),
        }
    }

    /// Maximum columns and rows, with a half's subdivisions added across the split.
    fn grid_size(&self) -> (usize, usize) {
        match self {
            Self::Rows(cells) => (cells.iter().copied().max().unwrap_or(1), cells.len()),
            Self::Columns(cells) => (cells.len(), cells.iter().copied().max().unwrap_or(1)),
            Self::Halves { by_rows, halves } => {
                let (a, b) = (halves[0].grid_size(), halves[1].grid_size());
                if *by_rows {
                    (a.0.max(b.0), a.1 + b.1)
                } else {
                    (a.0 + b.0, a.1.max(b.1))
                }
            }
        }
    }

    fn split(&self) -> Split {
        match self {
            Self::Rows(tracks) | Self::Columns(tracks) => {
                let rows = matches!(self, Self::Rows(_));
                Split {
                    rows,
                    children: tracks
                        .iter()
                        .map(|&count| Split {
                            rows: !rows,
                            children: (0..count).map(|_| Split::leaf()).collect(),
                        })
                        .collect(),
                }
            }
            Self::Halves { by_rows, halves } => Split {
                rows: *by_rows,
                children: halves.iter().map(|half| half.split()).collect(),
            },
        }
    }
}

/// Membership for one linked chart field.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum ChartLink {
    /// Follow the layout-wide switch and legacy group. No individual marker is shown.
    #[default]
    Layout,
    /// Exclude this field from synchronization, including the layout-wide switch.
    Unlinked,
    /// Link this field to charts carrying the same identifier, independently of the switch.
    /// A chart that joins the group takes its symbol or interval, as [`ChartLayout`] describes.
    Group(String),
}

/// Independent symbol and interval memberships, suitable for host serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct ChartLinkGroups {
    /// Symbol membership.
    pub symbol: ChartLink,
    /// Interval membership.
    pub interval: ChartLink,
}

#[derive(Clone, Default)]
struct LinkState {
    groups: [ChartLinkGroups; MAX_CHARTS],
    pending: Vec<LinkChange>,
    /// The steps recorded on chart histories, renumbered by `swap_slots`. A clone shares the
    /// entries recorded before it, so a replacement layout keeps them in step.
    recorded: Vec<std::sync::Weak<Mutex<LinkChange>>>,
}

/// A link membership step, applied like a [`SizeChange`] by the layout that shows the chart.
#[derive(Clone)]
pub(super) struct LinkChange {
    index: usize,
    symbol: Option<ChartLink>,
    interval: Option<ChartLink>,
}

impl LinkChange {
    fn swap(&mut self, groups: &mut [ChartLinkGroups; MAX_CHARTS]) {
        if let Some(symbol) = &mut self.symbol {
            std::mem::swap(symbol, &mut groups[self.index].symbol);
        }
        if let Some(interval) = &mut self.interval {
            std::mem::swap(interval, &mut groups[self.index].interval);
        }
    }
}

/// Undo and redo steps of cross-chart edits, which every chart of the layout takes at the next
/// [`ChartLayout::show`].
#[derive(Clone, Default)]
struct LayoutEdits {
    steps: Arc<Mutex<Vec<(u64, bool)>>>,
}

impl LayoutEdits {
    #[cfg(any(feature = "studies", feature = "drawings"))]
    fn start(&mut self) -> LayoutEditLink {
        // Process-wide, so that copies made through different layout values never reuse an id
        // that a chart's history may already hold.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        LayoutEditLink {
            steps: Arc::clone(&self.steps),
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// Marks a chart's history action as its part of a cross-chart edit.
#[derive(Clone)]
pub(super) struct LayoutEditLink {
    steps: Arc<Mutex<Vec<(u64, bool)>>>,
    pub(super) id: u64,
}

impl LayoutEditLink {
    /// Asks the other charts of the edit to take the same step.
    pub(super) fn request(&self, redo: bool) {
        let mut steps = self.steps.lock().unwrap();
        // The other charts follow the last step asked for this edit.
        match steps.iter_mut().find(|(id, _)| *id == self.id) {
            Some(step) => step.1 = redo,
            None => steps.push((self.id, redo)),
        }
    }
}

/// Independent synchronization switches within each layout group.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ChartSyncOptions {
    /// Synchronize scrolling and zooming using UTC plot edges. Turning this on fits the other
    /// charts of the active chart's group to its visible span at the next [`ChartLayout::show`].
    pub sync_time_axis: bool,
    /// Synchronize the hovered timestamp.
    pub sync_crosshair: bool,
    /// Synchronize completed main-pane drawings between charts of the same symbol.
    /// A chart that comes to a symbol receives the drawings mirrored there. Turning this on links
    /// equal drawings of the charts of a symbol and mirrors the others both ways, without replaying
    /// what was done while it was off.
    /// Study-owned drawings stay local because study ids and scales belong to each chart.
    /// Charts that keep their drawings in the same storage are left to it: they read each other's
    /// stored drawings whether this is on or off, so a drawing reaches them once a chart history
    /// edit writes it. Drawings that are not saveable still sync between them here, and so do
    /// drawings in a top-level folder kept in the layout, which storage does not hold.
    /// A top-level folder's `FolderSync` overrides this for its drawings: one kept on the chart
    /// reaches no other chart, one kept in the layout reaches the charts of this layout only.
    pub sync_drawings: bool,
    /// Synchronize symbol changes for fields following the layout setting. Turning this on
    /// aligns each group at the next [`ChartLayout::show`]: those fields take the active chart's
    /// symbol, or in a group without it the lowest such slot's, and keep their undo history.
    /// Explicit individual groups work independently. A host without a feed supplies bars.
    pub sync_symbol: bool,
    /// Synchronize interval changes for fields following the layout setting. Turning this on
    /// aligns each group at the next [`ChartLayout::show`]: those fields take the active chart's
    /// interval, or in a group without it the lowest such slot's.
    /// Explicit individual groups work independently. A host without a feed supplies bars.
    pub sync_timeframe: bool,
    /// Pass chosen date ranges, as given to [`ChartLayout::apply_date_range`], on to the group.
    /// Later scrolling and zooming stay on each chart. A group that synchronizes scrolling and
    /// zooming receives chosen ranges whether this is on or off.
    pub sync_date_range: bool,
}

impl Default for ChartSyncOptions {
    fn default() -> Self {
        Self {
            sync_time_axis: true,
            sync_crosshair: true,
            sync_drawings: false,
            sync_symbol: false,
            sync_timeframe: false,
            sync_date_range: false,
        }
    }
}

/// Container state for up to sixteen host-owned charts. Supply charts in stable slot order.
///
/// Each chart keeps its own feed, settings, scales, studies and drawings. Legacy view groups
/// isolate scrolling, crosshairs and drawings; unnamed view groups synchronize together.
/// Symbol and interval memberships are independent through [`ChartLinkGroups`]. Save these and the
/// charts through host storage as needed. A symbol or interval changed in any chart reaches the
/// charts linked to it, whichever chart is active; when several of them changed it in one frame,
/// the active chart's change wins, then the lowest slot's. A chart that joins a group through
/// its link controls or [`Self::set_link_groups`] with history takes the group's symbol or
/// interval: the active chart's when it is a member, otherwise the lowest other slot's. Charts
/// supplied or restored into linked slots, and memberships set without history, keep their own
/// until the next change. A switch of [`ChartSyncOptions`] turned on aligns the charts following
/// it at the next [`Self::show`], unless that show receives a chart supplied or restored into a
/// slot. Synchronization includes supplied slots hidden by a grid preset. The container does not
/// acquire data or persist it. Charts that keep their drawings in the same storage
/// (`Chart::set_storage`) read the drawings of their symbol again after another of them wrote
/// them, in any group.
/// A clone follows pending cross-chart undo and redo steps when it shows the same charts.
/// Clones showing other charts leave those steps for their owners. Each clone keeps its own
/// split sizes and link memberships; undo and redo of a change to them apply to the layout that
/// shows the chart holding the step next, at that [`Self::show`], replacing what the host set
/// meanwhile for the same preset or link field.
///
/// ```
/// # fn example(ui: &mut egui::Ui) {
/// use egui_charts::widget::{Chart, ChartLayout, ChartLayoutMode};
/// let mut a = Chart::new(egui_charts::model::BarData::new());
/// let mut b = Chart::new(egui_charts::model::BarData::new());
/// let mut layout = ChartLayout::default();
/// layout.mode = ChartLayoutMode::TwoHorizontal;
/// layout.sync.sync_crosshair = true;
/// layout.show(ui, &mut [&mut a, &mut b]);
/// # }
/// ```
#[derive(Clone, Default)]
pub struct ChartLayout {
    /// The selected grid or tab preset.
    pub mode: ChartLayoutMode,
    /// Synchronization controls, applied within each group.
    pub sync: ChartSyncOptions,
    /// Offer editable group identifiers instead of the default emoji choices.
    pub text_sync_groups: bool,
    active: usize,
    /// The chart slot shown alone in place of the preset.
    maximized: Option<usize>,
    focus_active: bool,
    /// A keyboard or maximized-chart selection fits the source's range at the new plot width.
    focus_range: Option<(usize, usize)>,
    /// The slot whose corner menu is open. It stays shown when a click selects another chart, so
    /// a name being edited in it can still be committed.
    corner_menu: Option<usize>,
    groups: [Option<String>; MAX_CHARTS],
    links: LinkState,
    detached: [bool; MAX_CHARTS],
    previous: [Option<(String, Option<Timeframe>)>; MAX_CHARTS],
    previous_charts: [u64; MAX_CHARTS],
    previous_ranges: [Option<VisibleTimeRange>; MAX_CHARTS],
    previous_bars: [BarsKey; MAX_CHARTS],
    pending_ranges: [Option<PendingRange>; MAX_CHARTS],
    #[cfg(feature = "drawings")]
    drawings: DrawingLinks,
    previous_preset: Option<(ChartLayoutMode, usize)>,
    /// The switches at the last [`Self::show`], to tell which of them were turned on since.
    previous_sync: ChartSyncOptions,
    sizing: SizingState,
    edits: LayoutEdits,
    /// The bindings [`Self::show`] gives every chart, if set.
    shortcuts: Option<ShortcutMap>,
}

impl std::fmt::Debug for ChartLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChartLayout")
            .field("mode", &self.mode)
            .field("sync", &self.sync)
            .field("text_sync_groups", &self.text_sync_groups)
            .field("active", &self.active)
            .field("maximized", &self.maximized)
            .finish_non_exhaustive()
    }
}

/// Identifies a chart's bars: it changes when they are replaced, not when they stream.
type BarsKey = (u64, Option<chrono::DateTime<chrono::Utc>>);

fn bars_key(chart: &Chart) -> BarsKey {
    (
        chart.state.revision(),
        chart.state.shown().first().map(|bar| bar.time),
    )
}

/// A group name field that returns an edited name once, when it loses focus or on Enter, so
/// nothing joins a group while typing. Escape discards the edit.
fn group_name_edit(ui: &mut Ui, current: &str) -> Option<String> {
    let id = ui.scope_id().with("group_name");
    let mut name = ui
        .data(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| current.to_owned());
    let response = ui.add(
        egui::TextEdit::singleline(&mut name)
            .id(id)
            .desired_width(140.0),
    );
    if response.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, name));
        return None;
    }
    // An idle field has no outline of its own, and its fill can match the menu's.
    ui.painter().rect_stroke(
        response.rect,
        ui.visuals().widgets.inactive.corner_radius,
        Stroke::new(1.0, ui.active_theme().ui.border_control),
        StrokeKind::Inside,
    );
    ui.data_mut(|data| data.remove_temp::<String>(id));
    (response.lost_focus() && name != current && !ui.input(|i| i.key_pressed(egui::Key::Escape)))
        .then_some(name)
}

/// A range a chart fits once bars of its new interval replace those it held at the switch.
#[derive(Clone, Copy)]
struct PendingRange {
    range: VisibleTimeRange,
    /// The [`Chart::sync_source`] of the chart it came from, `None` for the chart's own range.
    source: Option<(Timeframe, f64)>,
    bars: BarsKey,
}

impl ChartLayout {
    /// Sizes retained for every edited preset, suitable for host serialization.
    pub fn saved_sizes(&self) -> Vec<ChartLayoutSizing> {
        self.sizing.sizes.clone()
    }

    /// Current relative sizes of a preset's splits, including its default weights when unedited.
    /// Splits are visited depth first: the outer rows or columns, then each child's subdivisions.
    pub fn sizes(&self, mode: ChartLayoutMode) -> ChartLayoutSizing {
        self.sizing
            .sizes
            .iter()
            .find(|sizes| sizes.mode == mode)
            .cloned()
            .unwrap_or_else(|| {
                let mut weights = Vec::new();
                mode.tracks().split().weights(&mut weights);
                ChartLayoutSizing { mode, weights }
            })
    }

    /// Updates only the supplied presets. Each weight must be finite and positive, and the shape
    /// must match [`Self::sizes`]. Invalid input leaves every preset unchanged.
    /// Weights are stored as fractions of their split total.
    /// Unless `skip_history` is true, each call enters the active chart's history on the next
    /// [`Self::show`]; the chart's regular undo, redo and toolbar controls then restore it.
    pub fn set_sizes(
        &mut self,
        sizes: &[ChartLayoutSizing],
        skip_history: bool,
    ) -> Result<(), &'static str> {
        for (index, size) in sizes.iter().enumerate() {
            let shape = self.sizes(size.mode);
            if sizes[..index]
                .iter()
                .any(|previous| previous.mode == size.mode)
                || size.weights.len() != shape.weights.len()
                || size
                    .weights
                    .iter()
                    .zip(&shape.weights)
                    .any(|(weights, expected)| {
                        weights.len() != expected.len()
                            || weights
                                .iter()
                                .any(|weight| !weight.is_finite() || *weight <= 0.0)
                            || !weights.iter().sum::<f32>().is_finite()
                            || weights
                                .iter()
                                .any(|weight| *weight / weights.iter().sum::<f32>() <= 0.0)
                    })
            {
                return Err("invalid chart split weights");
            }
        }
        let before = self.saved_sizes();
        let mut updated = before.clone();
        for size in sizes {
            if *size == self.sizes(size.mode) {
                continue;
            }
            let mut size = size.clone();
            for weights in &mut size.weights {
                let sum: f64 = weights.iter().map(|&weight| f64::from(weight)).sum();
                for weight in weights {
                    *weight = (f64::from(*weight) / sum) as f32;
                }
            }
            if size == self.sizes(size.mode) {
                continue;
            }
            if let Some(previous) = updated
                .iter_mut()
                .find(|previous| previous.mode == size.mode)
            {
                *previous = size;
            } else {
                updated.push(size);
            }
        }
        if updated != before {
            self.sizing.sizes = updated;
            if !skip_history {
                self.sizing.record(before);
            }
        }
        Ok(())
    }

    /// Restores every preset's initial proportions. Undo is recorded on the next [`Self::show`]
    /// unless `skip_history` is true. Hidden presets are reset too.
    pub fn reset_sizes(&mut self, skip_history: bool) {
        let before = self.saved_sizes();
        if !before.is_empty() {
            self.sizing.sizes.clear();
            if !skip_history {
                self.sizing.record(before);
            }
        }
    }

    fn record_sizes(&mut self, chart: &mut Chart) {
        for change in self.sizing.pending.drain(..) {
            chart.history.record_sizes(change);
        }
    }

    /// Applies the split size and link membership steps the charts undid or redid since they
    /// were last shown, whichever layout recorded them, in the order they were taken. Returns
    /// whether any was applied.
    fn apply_history_steps(&mut self, charts: &mut [&mut Chart]) -> bool {
        let mut sizes: Vec<_> = charts
            .iter_mut()
            .flat_map(|chart| std::mem::take(&mut chart.history.size_steps))
            .collect();
        let mut links: Vec<_> = charts
            .iter_mut()
            .flat_map(|chart| std::mem::take(&mut chart.history.link_steps))
            .collect();
        sizes.sort_by_key(|(order, _)| *order);
        links.sort_by_key(|(order, _)| *order);
        for (_, change) in &sizes {
            change.lock().unwrap().swap(&mut self.sizing.sizes);
        }
        for (_, change) in &links {
            change.lock().unwrap().swap(&mut self.links.groups);
        }
        !sizes.is_empty() || !links.is_empty()
    }

    fn step_layout_edits(&mut self, charts: &mut [&mut Chart]) -> bool {
        let mut stepped = [false; MAX_CHARTS];
        loop {
            let step = {
                let mut steps = self.edits.steps.lock().unwrap();
                let at = steps.iter().rposition(|(id, _)| {
                    let mut belongs = false;
                    for chart in charts
                        .iter()
                        .filter(|chart| chart.history.holds_layout_edit(*id))
                    {
                        if chart.history_blocked() {
                            return false;
                        }
                        belongs = true;
                    }
                    belongs
                });
                at.map(|at| steps.remove(at))
            };
            let Some((id, redo)) = step else {
                break;
            };
            for (index, chart) in charts.iter_mut().enumerate() {
                stepped[index] |= chart.history.holds_layout_edit(id);
                chart.step_layout_edit(id, redo);
            }
        }
        let any = stepped.contains(&true);
        // A symbol or interval an edit's step restored reaches the linked charts, as after an
        // undo on the active chart; the active chart's goes first, then the lowest slot.
        let count = charts.len();
        for index in std::iter::once(self.active).chain(0..count) {
            if std::mem::take(&mut stepped[index]) {
                self.send_state(charts, count, index);
            }
        }
        any
    }

    /// Active chart slot, clamped to supplied charts by [`Self::show`].
    pub fn active(&self) -> usize {
        self.active
    }

    /// Selects a chart slot and focuses its canvas when shown; out-of-range indices are ignored.
    /// While a chart is maximized, the next [`Self::show`] shows this slot maximized in its place
    /// unless it is detached. A slot past the preset is clamped to its last supplied chart.
    pub fn set_active(&mut self, index: usize) {
        if index < MAX_CHARTS {
            if self.maximized.is_some() && index != self.active {
                self.focus_range = Some((self.active, index));
            }
            self.active = index;
            self.focus_active = true;
        }
    }

    /// The chart slot shown alone in place of the preset, by Alt+Enter in a grid or
    /// [`Self::set_maximized`]; `None` while the preset shows.
    pub fn maximized(&self) -> Option<usize> {
        self.maximized
    }

    /// Shows chart slot `index` alone in place of the preset and makes it the active chart, or
    /// restores the preset with `None`. Out-of-range indices are ignored. A slot the preset does
    /// not show, and the tab preset, restore the preset at the next [`Self::show`].
    pub fn set_maximized(&mut self, index: Option<usize>) {
        match index {
            Some(index) if index < MAX_CHARTS => {
                self.set_active(index);
                self.maximized = Some(index);
            }
            Some(_) => {}
            None => self.maximized = None,
        }
    }

    /// Moves the sync memberships, detached state and drawing links of two slots with their
    /// charts, for a host that exchanges those charts. Undo and redo of a membership change then
    /// apply to the slot its chart moved to. Invalid indices are ignored.
    pub fn swap_slots(&mut self, a: usize, b: usize) {
        if a.max(b) >= self.groups.len() {
            return;
        }
        self.groups.swap(a, b);
        self.links.groups.swap(a, b);
        let moved = |index: &mut usize| {
            if *index == a {
                *index = b;
            } else if *index == b {
                *index = a;
            }
        };
        if let Some((from, to)) = &mut self.focus_range {
            moved(from);
            moved(to);
        }
        for change in &mut self.links.pending {
            moved(&mut change.index);
        }
        for step in self
            .links
            .recorded
            .iter()
            .filter_map(std::sync::Weak::upgrade)
        {
            moved(&mut step.lock().unwrap().index);
        }
        self.detached.swap(a, b);
        #[cfg(feature = "drawings")]
        self.drawings.swap_slots(a, b);
    }

    /// Assigns the legacy group used by view/drawing links and layout-wide field switches.
    /// `Some` keeps both memberships: a field in an individual group or unlinked stays so, and a
    /// field following the layout setting follows it within the new group.
    /// `None` restores the unnamed view group and unlinks both symbol and interval.
    /// Prefer [`Self::set_link_groups`] for independent field memberships.
    pub fn set_sync_group(&mut self, index: usize, group: Option<String>) {
        let group = group.filter(|name| !name.trim().is_empty());
        if let Some(slot) = self.groups.get_mut(index) {
            let unnamed = group.is_none();
            *slot = group;
            if unnamed {
                self.set_link_groups(
                    index,
                    ChartLinkGroups {
                        symbol: ChartLink::Unlinked,
                        interval: ChartLink::Unlinked,
                    },
                    true,
                );
            }
        }
    }

    /// The group assigned to a slot, or `None` for the unnamed group or an invalid slot.
    pub fn sync_group(&self, index: usize) -> Option<&str> {
        self.groups.get(index).and_then(|g| g.as_deref())
    }

    /// Memberships of all slots, including hidden charts, for host storage.
    pub fn saved_link_groups(&self) -> [ChartLinkGroups; MAX_CHARTS] {
        self.links.groups.clone()
    }

    /// Memberships of a slot; invalid indices return `None`.
    pub fn link_groups(&self, index: usize) -> Option<ChartLinkGroups> {
        self.links.groups.get(index).cloned()
    }

    /// Sets independent field memberships. Empty identifiers are treated as unlinked.
    /// Unless `skip_history` is true, records one undo step on the active chart at the next
    /// [`Self::show`], where the chart in slot `index` also takes the symbol and interval of the
    /// groups it joined. Undo restores only the fields changed by this call. Restore saved
    /// memberships with `skip_history` true, which keeps each chart's own symbol and interval.
    pub fn set_link_groups(
        &mut self,
        index: usize,
        mut groups: ChartLinkGroups,
        skip_history: bool,
    ) {
        for field in [&mut groups.symbol, &mut groups.interval] {
            if matches!(field, ChartLink::Group(name) if name.trim().is_empty()) {
                *field = ChartLink::Unlinked;
            }
        }
        let Some(slot) = self.links.groups.get_mut(index) else {
            return;
        };
        if !skip_history && *slot != groups {
            self.links.pending.push(LinkChange {
                index,
                symbol: (slot.symbol != groups.symbol).then(|| slot.symbol.clone()),
                interval: (slot.interval != groups.interval).then(|| slot.interval.clone()),
            });
        }
        *slot = groups;
    }

    fn record_links(&mut self, chart: &mut Chart) {
        let links = &mut self.links;
        if !links.pending.is_empty() {
            // Steps no history holds any more cannot be undone and need no renumbering.
            links.recorded.retain(|step| step.strong_count() > 0);
        }
        for change in links.pending.drain(..) {
            let change = Arc::new(Mutex::new(change));
            links.recorded.push(Arc::downgrade(&change));
            chart.history.record_links(change);
        }
    }

    /// Shows separate symbol and interval selectors for a chart slot. Returns whether edited.
    /// The built-in chart corner menu uses these same controls. A host can place them elsewhere;
    /// [`Self::show`] transfers their edits to the active chart's undo history.
    pub fn show_link_groups(&mut self, ui: &mut Ui, index: usize) -> bool {
        self.link_controls(ui, index, false)
    }

    fn link_controls(&mut self, ui: &mut Ui, index: usize, compact: bool) -> bool {
        let Some(mut groups) = self.link_groups(index) else {
            return false;
        };
        let before = groups.clone();
        ui.push_id(("chart_links", index), |ui| {
            for (label, short, field, enabled) in [
                (
                    "Symbol group",
                    "S",
                    &mut groups.symbol,
                    self.sync.sync_symbol,
                ),
                (
                    "Interval group",
                    "I",
                    &mut groups.interval,
                    self.sync.sync_timeframe,
                ),
            ] {
                let marker = match field {
                    ChartLink::Group(name) => name.clone(),
                    ChartLink::Layout | ChartLink::Unlinked => "○".to_owned(),
                };
                let label = l10n::text(ui.ctx(), label);
                let painted = LINK_MARKERS.iter().any(|(emoji, _)| *emoji == marker);
                let caption = if painted {
                    format!("     {}", if compact { short } else { &label })
                } else if compact {
                    format!("{short} {}", marker.chars().take(2).collect::<String>())
                } else {
                    format!("{label}  {marker}")
                };
                let menu = ui.menu_button(caption, |ui| {
                    ui.label(&label);
                    let unlink = ui
                        .selectable_label(
                            *field == ChartLink::Unlinked
                                || *field == ChartLink::Layout && !enabled,
                            l10n::text(ui.ctx(), "Unlinked"),
                        )
                        .clicked();
                    if unlink {
                        *field = ChartLink::Unlinked;
                        ui.close();
                    }
                    if !self.text_sync_groups {
                        ui.horizontal(|ui| {
                            for (emoji, _) in LINK_MARKERS {
                                let selected =
                                    matches!(field, ChartLink::Group(name) if name == emoji);
                                let button = ui.add(
                                    egui::Button::selectable(selected, "")
                                        .min_size(Vec2::splat(28.0)),
                                );
                                button.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::Role::MenuItemCheckBox,
                                        button.enabled(),
                                        selected,
                                        emoji,
                                    )
                                });
                                paint_link_marker(
                                    ui,
                                    Rect::from_center_size(button.rect.center(), Vec2::splat(16.0)),
                                    emoji,
                                );
                                if button.clicked() {
                                    *field = if selected {
                                        ChartLink::Unlinked
                                    } else {
                                        ChartLink::Group(emoji.into())
                                    };
                                    ui.close();
                                }
                            }
                        });
                    }
                    if self.text_sync_groups {
                        ui.separator();
                        let name = match field {
                            ChartLink::Group(name) => name.as_str(),
                            _ => "",
                        };
                        // The click on "Unlinked" also ends the edit; the click wins.
                        if let Some(name) = group_name_edit(ui, name)
                            && !unlink
                        {
                            *field = if name.trim().is_empty() {
                                ChartLink::Unlinked
                            } else {
                                ChartLink::Group(name)
                            };
                        }
                    }
                    ui.separator();
                    if ui
                        .selectable_label(
                            *field == ChartLink::Layout && enabled,
                            l10n::text(ui.ctx(), "Follow layout setting"),
                        )
                        .clicked()
                    {
                        *field = ChartLink::Layout;
                        ui.close();
                    }
                });
                if painted {
                    paint_link_marker(
                        ui,
                        Rect::from_center_size(
                            egui::pos2(
                                menu.response.rect.left() + 14.0,
                                menu.response.rect.center().y,
                            ),
                            Vec2::splat(14.0),
                        ),
                        &marker,
                    );
                }
                menu.response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        menu.response.enabled(),
                        format!("{label} {marker}"),
                    )
                });
                menu.response
                    .on_hover_text(if !painted && matches!(field, ChartLink::Group(_)) {
                        format!("{label} {marker}")
                    } else {
                        label
                    });
            }
        });
        let changed = groups != before;
        if changed {
            self.set_link_groups(index, groups, false);
        }
        changed
    }

    /// Paints the symbol and interval group markers of a slot too narrow for the selectors,
    /// without captions, in a row that ends at `end`. A field outside a group paints none.
    fn paint_link_markers(&self, ui: &Ui, index: usize, end: egui::Pos2) {
        let groups = &self.links.groups[index];
        for (field, offset) in [(&groups.symbol, 27.0), (&groups.interval, 9.0)] {
            let ChartLink::Group(name) = field else {
                continue;
            };
            let rect = Rect::from_center_size(end - Vec2::new(offset, 0.0), Vec2::splat(14.0));
            if LINK_MARKERS.iter().any(|(emoji, _)| emoji == name) {
                paint_link_marker(ui, rect, name);
            } else {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    name.chars().take(2).collect::<String>(),
                    egui::FontId::proportional(11.0),
                    ui.visuals().text_color(),
                );
            }
        }
    }

    fn linked(&self, a: usize, b: usize, interval: bool) -> bool {
        let groups = &self.links.groups;
        let field = |index: usize| {
            if interval {
                &groups[index].interval
            } else {
                &groups[index].symbol
            }
        };
        match (field(a), field(b)) {
            (ChartLink::Group(a), ChartLink::Group(b)) => a == b,
            (ChartLink::Layout, ChartLink::Layout) => {
                self.same_group(a, b)
                    && if interval {
                        self.sync.sync_timeframe
                    } else {
                        self.sync.sync_symbol
                    }
            }
            _ => false,
        }
    }

    /// Detaches a chart into an immediate viewport. Backends without native multi-window
    /// support embed the viewport. Closing it returns the chart to its grid or tab.
    pub fn set_detached(&mut self, index: usize, detached: bool) {
        if let Some(slot) = self.detached.get_mut(index) {
            *slot = detached;
        }
    }

    /// Whether a chart is detached.
    pub fn is_detached(&self, index: usize) -> bool {
        self.detached.get(index).copied().unwrap_or(false)
    }

    /// Gives `map` to every chart this layout shows, at each [`Self::show`] before the charts
    /// dispatch keys, so a chart supplied later takes it at its first show. While a map is set it
    /// replaces bindings changed through [`Chart::shortcuts_mut`]. `None`, the default, leaves
    /// each chart the bindings it has.
    pub fn set_shortcuts(&mut self, map: Option<ShortcutMap>) {
        self.shortcuts = map;
    }

    /// The map [`Self::set_shortcuts`] gives every chart, if any.
    pub fn shortcuts(&self) -> Option<&ShortcutMap> {
        self.shortcuts.as_ref()
    }

    /// Fits an explicit date selection to the active chart. With `sync_time_axis` on, the other
    /// charts of its synchronization group then show the active chart's new span, as after a
    /// scroll or zoom; otherwise they fit the selection itself when `sync_date_range` is on, and
    /// keep their view with both off. Each chart it fits ends the scrolls and animations pending
    /// on it, as [`Chart::set_visible_time_range`] does. Call after the charts have rendered at
    /// their sizes.
    pub fn apply_date_range(&self, charts: &mut [&mut Chart], range: super::VisibleTimeRange) {
        let Some(active) = charts.get_mut(self.active) else {
            return;
        };
        let fitted = active.set_visible_time_range(range);
        let source = fitted.then(|| active.sync_source()).flatten();
        let span = active
            .visible_time_range()
            .filter(|_| fitted && self.sync.sync_time_axis);
        let Some(range) = span.or(self.sync.sync_date_range.then_some(range)) else {
            return;
        };
        for (index, chart) in charts.iter_mut().take(MAX_CHARTS).enumerate() {
            if index != self.active
                && self.same_group(index, self.active)
                && chart.apply_synced_time_range_from(range, source)
            {
                chart.cancel_view_motion();
            }
        }
    }

    /// Goes to `time` on `date`, or to the start of `date`, on the active chart, as
    /// [`Chart::go_to_local_time`] does. With `sync_time_axis` on, the other charts of its
    /// synchronization group follow its new span, as after a scroll; otherwise each goes to
    /// the same local date and time in its own zone when `sync_date_range` is on, and keeps its
    /// view with both off.
    pub fn go_to_local_time(
        &self,
        charts: &mut [&mut Chart],
        date: chrono::NaiveDate,
        time: Option<chrono::NaiveTime>,
    ) {
        if self.active >= charts.len().min(MAX_CHARTS) {
            return;
        }
        let group = !self.sync.sync_time_axis && self.sync.sync_date_range;
        for (index, chart) in charts.iter_mut().take(MAX_CHARTS).enumerate() {
            if index == self.active || (group && self.same_group(index, self.active)) {
                chart.go_to_local_time(date, time);
            }
        }
    }

    /// Shows the days from `start` to `end`, both included, on the active chart's calendar:
    /// from `start_time` on the first day, or its start, to `end_time` on the last day, or its
    /// end. The charts of its group receive the range as from [`Self::apply_date_range`].
    /// A range that does not end after its start changes nothing.
    pub fn apply_local_date_range(
        &self,
        charts: &mut [&mut Chart],
        start: chrono::NaiveDate,
        start_time: Option<chrono::NaiveTime>,
        end: chrono::NaiveDate,
        end_time: Option<chrono::NaiveTime>,
    ) {
        let Some(active) = charts.get(self.active) else {
            return;
        };
        let from = active.local_date_instant(start, start_time);
        let to = match end_time {
            Some(time) => active.local_date_instant(end, Some(time)),
            // The last second before the next day starts, as the hosts counted days.
            None => end
                .succ_opt()
                .and_then(|next| active.local_date_instant(next, None))
                .map(|next| next - chrono::TimeDelta::seconds(1)),
        };
        if let Some((from, to)) = from.zip(to) {
            self.apply_date_range(charts, super::VisibleTimeRange::new(from, to));
        }
    }

    fn same_group(&self, a: usize, b: usize) -> bool {
        self.groups[a] == self.groups[b]
    }

    /// Steps among supplied slots, skipping detached charts and wrapping at the ends.
    fn cycled_slot(&self, from: usize, forward: bool, count: usize) -> usize {
        (1..count)
            .map(|step| {
                if forward {
                    (from + step) % count
                } else {
                    (from + count - step) % count
                }
            })
            .find(|slot| !self.detached[*slot])
            .unwrap_or(from)
    }

    /// Activates the chart that the queued Tab and Shift+Tab requests of the first `count` charts
    /// move to, handing it any maximized place; reports whether the active chart moved.
    fn cycle_charts(&mut self, ui: &Ui, charts: &mut [&mut Chart], count: usize) -> bool {
        if ui.ctx().will_discard() {
            return false;
        }
        let mut moved = false;
        for (index, chart) in charts.iter_mut().take(count).enumerate() {
            let mut steps = Vec::new();
            chart.keyboard.requests.retain(|request| match request {
                ShortcutRequest::Action(KeyboardAction::CycleChartFocus) => {
                    steps.push(true);
                    false
                }
                ShortcutRequest::Action(KeyboardAction::CycleChartFocusBack) => {
                    steps.push(false);
                    false
                }
                _ => true,
            });
            if steps.is_empty() {
                continue;
            }
            let target = steps.into_iter().fold(index, |slot, forward| {
                self.cycled_slot(slot, forward, count)
            });
            if self.maximized.is_some() && !self.detached[target] {
                self.maximized = Some(target);
            }
            self.set_active(target);
            if target != index {
                self.focus_range = Some((index, target));
            }
            ui.ctx().request_repaint();
            moved = true;
        }
        moved
    }

    /// Shows the preset and synchronizes its charts. Click or scroll a panel to activate it;
    /// chart shortcuts and typed entry go to the active chart, while a focused text field or
    /// dialog keeps its keys.
    /// With more than one chart, Tab and Shift+Tab activate and focus the next and previous chart
    /// in slot order, skipping detached ones and wrapping at the ends; while a chart is maximized,
    /// that chart is shown maximized instead. Alt+Enter in a grid shows the active chart alone and
    /// restores the preset. These keys stay with the layout even when the host drains
    /// [`Chart::take_shortcut_requests`] first; with one chart they reach the host.
    /// Its header and corner menu offer independent symbol and interval groups; the corner menu
    /// also offers detachment and the synchronization group. The active border is painted
    /// inside the chart; stacked charts have a two-point gap and side-by-side charts meet at their
    /// edge.
    /// Dragging a border resizes its neighbours and Shift also moves the borders aligned with it; a
    /// touch must tap a border before moving it. Double-clicking a border restores the proportions.
    /// Extra charts remain owned by the host and are not painted by a grid preset.
    pub fn show(&mut self, ui: &mut Ui, charts: &mut [&mut Chart]) -> Response {
        self.show_with(ui, charts, |ui, _, chart| chart.show(ui))
    }

    /// Draws the layout with a host-selected renderer for each visible chart, including
    /// detached viewports. The callback runs every frame and must not block; supply cached
    /// data to calls such as [`Chart::show_footprint`]. Return the chart's response so focus,
    /// pointer activation and link controls behave as they do with [`Self::show`].
    pub fn show_with(
        &mut self,
        ui: &mut Ui,
        charts: &mut [&mut Chart],
        mut render: impl FnMut(&mut Ui, usize, &mut Chart) -> Response,
    ) -> Response {
        let total = charts.len().min(MAX_CHARTS);
        let charts = &mut charts[..total];
        let count = total.min(self.mode.chart_count());
        if count == 0 {
            return ui.allocate_response(Vec2::ZERO, Sense::hover());
        }
        // Keep the layout's keys queued when a host drains requests before `show`.
        let pass = ui.ctx().cumulative_pass_nr();
        let keys = (count > 1).then_some(if self.mode == ChartLayoutMode::Tabs {
            &LAYOUT_KEYS[1..]
        } else {
            LAYOUT_KEYS
        });
        for (index, chart) in charts.iter_mut().enumerate() {
            chart.keyboard.layout_keys = keys.filter(|_| index < count).map(|keys| (pass, keys));
        }
        self.active = self.active.min(count - 1);
        if let Some((_, target)) = &mut self.focus_range {
            *target = (*target).min(count - 1);
        }
        // Tabs already show one chart at a time and use their strip to select the others.
        if self.mode == ChartLayoutMode::Tabs || self.maximized.is_some_and(|index| index >= count)
        {
            self.maximized = None;
        }
        // A Tab the host dispatched before `show` moves before the charts are drawn, as a host's
        // own `set_active` would, so the chart it reaches takes the focus in this pass and egui
        // does not also move the focus on from the chart that had it.
        if count > 1 && self.cycle_charts(ui, charts, count) {
            ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
        }
        // Host activation replaces the maximized chart; a detached chart keeps its window.
        if self.maximized.is_some() && !self.detached[self.active] {
            self.maximized = Some(self.active);
        }
        // A chart supplied or restored in a slot starts a new baseline: nothing changed in it.
        let known = self.previous_charts;
        let mut supplied = false;
        for (index, chart) in charts.iter().enumerate() {
            if self.previous_charts[index] != chart.layout_identity {
                supplied = true;
                // A chart new to the layout brings drawings new to it, except those of a restored
                // chart that it held before, while one that `swap_slots` moved keeps its links.
                #[cfg(feature = "drawings")]
                if !known.contains(&chart.layout_identity) {
                    self.drawings.forget_slot(index, chart);
                }
                self.previous_charts[index] = chart.layout_identity;
                self.previous[index] = None;
                self.previous_ranges[index] = None;
                self.pending_ranges[index] = None;
            }
        }
        if self.focus_range.is_some_and(|(from, to)| {
            from >= count
                || from == to
                || to != self.active
                || !self.sync.sync_time_axis
                || !self.same_group(from, to)
                || !known.contains(&charts[from].layout_identity)
                || !known.contains(&charts[to].layout_identity)
        }) {
            self.focus_range = None;
        }
        if let Some(map) = &self.shortcuts {
            for chart in charts.iter_mut() {
                if chart.shortcuts() != map {
                    *chart.shortcuts_mut() = map.clone();
                }
            }
        }
        // A switch turned on aligns the charts that follow it, except in a show that brings a
        // supplied or restored chart: like the charts, the switches are then a new baseline.
        let turned_on = |now: bool, before: bool| now && !before && !supplied;
        let aligned = (
            turned_on(self.sync.sync_symbol, self.previous_sync.sync_symbol),
            turned_on(self.sync.sync_timeframe, self.previous_sync.sync_timeframe),
            turned_on(self.sync.sync_time_axis, self.previous_sync.sync_time_axis),
        );
        self.previous_sync = self.sync.clone();
        // Before this frame's input selects a chart, so a change is sent from the chart it is in.
        self.sync_state(charts, total);
        let mut tab = None;
        if self.mode == ChartLayoutMode::Tabs {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, chart) in charts.iter().take(count).enumerate() {
                        if ui
                            .selectable_label(index == self.active, chart.symbol())
                            .clicked()
                        {
                            tab = Some(index);
                        }
                    }
                });
            });
        }
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        if self.previous_preset != Some((self.mode, count)) {
            self.sizing.selected = None;
        }
        if self
            .sizing
            .drag
            .as_ref()
            .is_some_and(|drag| drag.mode != self.mode || drag.count != count)
        {
            self.finish_resize();
        }
        let (mut rects, mut dividers) = self.geometry(rect, count);
        if self.move_divider(ui) {
            (rects, dividers) = self.geometry(rect, count);
        }
        // After a released drag queues its step, so an undo in this frame reaches it.
        self.record_sizes(charts[self.active]);
        self.join_groups(charts);
        self.align_followers(charts, aligned.0, aligned.1);
        self.record_links(charts[self.active]);
        // Divider hit regions stay above the charts, but their input is known before painting.
        let starting_resize = self.sizing.drag.is_none()
            && dividers.iter().any(|divider| {
                let key = (divider.group, divider.at);
                ui.ctx()
                    .is_being_dragged(ui.scope_id().with(("chart_divider", self.mode, key)))
                    && (self.sizing.selected == Some(key) || !ui.input(|input| input.any_touches()))
            });
        let resize_start = starting_resize.then(|| Arc::new(()));
        // A release in this frame ends the press, so an undo in it reaches the drag it ends.
        if !ui.input(|input| input.pointer.primary_down()) {
            self.sizing.pressed = None;
        }
        let resizing = self
            .sizing
            .drag
            .as_ref()
            .map(|drag| &drag.history_guard)
            .or(resize_start.as_ref())
            .or(self.sizing.pressed.as_ref());
        for chart in charts.iter_mut() {
            chart.history.resizing = resizing.map(Arc::downgrade).unwrap_or_default();
        }
        // After the layout edits, whose undo can take size steps with it.
        let stepped = self.step_layout_edits(charts);
        if self.apply_history_steps(charts) || stepped {
            (rects, dividers) = self.geometry(rect, count);
        }
        let over_divider = self.sizing.drag.is_some()
            || ui
                .input(|input| input.pointer.interact_pos())
                .is_some_and(|pos| {
                    dividers
                        .iter()
                        .any(|divider| divider.rect.expand(4.0).contains(pos))
                });
        ui.painter()
            .rect_filled(rect, 0.0, ui.active_theme().ui.border);
        // Select before drawing so keyboard and toolbar state follow pointer input this frame.
        let wheel = super::pan_zoom::raw_wheel_delta(ui) != Vec2::ZERO;
        if self.mode != ChartLayoutMode::Tabs
            && !over_divider
            && let Some(index) = rects.iter().position(|r| {
                ui.rect_contains_pointer(*r) && ui.input(|i| i.pointer.any_pressed() || wheel)
            })
        {
            self.active = index;
        }
        for (index, chart) in charts.iter().enumerate() {
            if !self.sync.sync_time_axis || total == 1 {
                self.pending_ranges[index] = None;
            } else if self.previous[index]
                .as_ref()
                .is_some_and(|p| p.1 != chart.timeframe())
            {
                let own = self.previous_ranges[index].map(|range| PendingRange {
                    range,
                    source: None,
                    bars: self.previous_bars[index],
                });
                self.pending_ranges[index] = self.pending_ranges[index].or(own);
            }
            self.record_state(index, chart);
        }
        for chart in charts.iter_mut() {
            chart.last_hover_bar_idx = None;
            chart.last_hover_whitespace = None;
        }
        let mut pointer_down = ui.input(|i| i.pointer.any_down());
        let before_range = charts[self.active].visible_time_range();
        let before_sync: Vec<_> = charts
            .iter()
            .map(|c| {
                (
                    c.synced_crosshair_bar_idx,
                    c.get_time_scale_state(),
                    c.visible_time_range(),
                )
            })
            .collect();
        let mut rendered = [false; MAX_CHARTS];
        for index in 0..count {
            if self.mode == ChartLayoutMode::Tabs && index != self.active && !self.detached[index] {
                continue;
            }
            if self.maximized.is_some_and(|slot| slot != index) && !self.detached[index] {
                continue;
            }
            charts[index].layout_inactive =
                count > 1 && index != self.active && !self.detached[index];
            let focus = self.focus_active && index == self.active;
            if self.detached[index] {
                let id = egui::ViewportId::from_hash_of((ui.scope_id(), "detached_chart", index));
                let chart = &mut charts[index];
                let mut close = false;
                let mut activate = false;
                ui.ctx().show_viewport_immediate(
                    id,
                    egui::ViewportBuilder::default()
                        .with_title(chart.symbol())
                        .with_inner_size([900.0, 600.0]),
                    |ui, _| {
                        pointer_down |= ui.input(|i| i.pointer.any_down());
                        close |= ui.input(|i| i.viewport().close_requested());
                        if ui
                            .button(l10n::text(ui.ctx(), "Return to layout"))
                            .clicked()
                        {
                            close = true;
                        }
                        activate |= ui
                            .horizontal(|ui| self.link_controls(ui, index, false))
                            .inner;
                        egui::CentralPanel::default().show(ui, |ui| {
                            slot_background(ui, ui.max_rect(), chart);
                            let response = render(ui, index, chart);
                            rendered[index] = true;
                            if focus && !response.has_focus() {
                                response.request_focus();
                            }
                            activate |= response.is_pointer_button_down_on()
                                || response.clicked()
                                || (response.hovered()
                                    && super::pan_zoom::raw_wheel_delta(ui) != Vec2::ZERO);
                        });
                    },
                );
                self.restore_time_range(index, chart);
                self.detached[index] = !close;
                if activate {
                    self.active = index;
                }
                if (self.mode != ChartLayoutMode::Tabs || index == self.active)
                    && self.maximized.is_none_or(|slot| slot == index)
                {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rects[index]), |ui| {
                        if ui
                            .button(l10n::text(ui.ctx(), "Return chart to layout"))
                            .clicked()
                        {
                            self.detached[index] = false;
                        }
                    });
                }
                continue;
            }
            let rect = rects[index];
            slot_background(ui, rect, charts[index]);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .id_salt(("chart_slot", index)),
                |ui| {
                    ui.shrink_clip_rect(rect);
                    // The status line starts at the plot, after any left price axis.
                    let plot = charts[index].last_rendered_price_rect;
                    let plot_left = if plot.is_positive() {
                        plot.left()
                    } else {
                        rect.left()
                    };
                    let link_controls =
                        total > 1 && rect.right() - plot_left >= LINK_CONTROLS_MIN_WIDTH;
                    charts[index].layout_link_controls = link_controls;
                    let response = render(ui, index, charts[index]);
                    rendered[index] = true;
                    charts[index].layout_link_controls = false;
                    charts[index].layout_inactive = false;
                    // Reselecting a focused canvas must keep its arrow-key filter.
                    if focus && !response.has_focus() {
                        response.request_focus();
                    }
                    if link_controls {
                        ui.scope_builder(
                            egui::UiBuilder::new().max_rect(Rect::from_min_size(
                                egui::pos2(rect.right() - LINK_CONTROLS_INSET, rect.top() + 2.0),
                                Vec2::new(110.0, 22.0),
                            )),
                            |ui| {
                                ui.horizontal(|ui| {
                                    self.link_controls(ui, index, true);
                                });
                            },
                        );
                    } else if total > 1 {
                        // The markers alone end the first row, past the status line, which stops
                        // 48 points short of the plot's right edge and 100 short of the slot's,
                        // and before the corner menu button.
                        let plot = charts[index].last_rendered_price_rect;
                        let right = if plot.is_positive() {
                            plot.right()
                        } else {
                            rect.right()
                        };
                        self.paint_link_markers(
                            ui,
                            index,
                            egui::pos2(right.min(rect.right() - 24.0), rect.top() + 13.0),
                        );
                    }
                    if self.corner_menu == Some(index)
                        || index == self.active
                            && (ui.rect_contains_pointer(rect)
                                || egui::Popup::is_any_open(ui.ctx())
                                || total > 1 && !link_controls)
                            && charts[index]
                                .pane_layout
                                .maximized
                                .is_none_or(|pane| pane == super::PaneId::MAIN)
                    {
                        let button = ui.put(
                            Rect::from_min_size(
                                rect.min
                                    + Vec2::new(
                                        (rect.width()
                                            - if rect.width() < 600.0 { 22.0 } else { 100.0 })
                                        .max(0.0),
                                        2.0,
                                    ),
                                Vec2::splat(20.0),
                            ),
                            egui::Button::new("…").frame(false),
                        );
                        let menu = egui::Popup::menu(&button)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| {
                                if ui.button(l10n::text(ui.ctx(), "Detach chart")).clicked() {
                                    self.detached[index] = true;
                                    ui.close();
                                }
                                self.show_link_groups(ui, index);
                                ui.label(l10n::text(ui.ctx(), "Sync group"));
                                let group = self.groups[index].as_deref().unwrap_or_default();
                                if let Some(name) = group_name_edit(ui, group) {
                                    if name.trim().is_empty() {
                                        // `set_sync_group(None)` would unlink both fields.
                                        self.groups[index] = None;
                                    } else {
                                        self.set_sync_group(index, Some(name));
                                    }
                                }
                            });
                        // Another slot's closed menu must not end this one's.
                        if menu.is_some() || self.corner_menu == Some(index) {
                            self.corner_menu = menu.map(|_| index);
                        }
                    }
                },
            );
            self.restore_time_range(index, charts[index]);
            if count > 1 && self.maximized.is_none() && index == self.active {
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(2.0, ui.active_theme().ui.accent),
                    StrokeKind::Inside,
                );
            }
        }
        if self.show_dividers(ui, &dividers, count)
            && let Some(id) = charts[self.active].keyboard.chart_id
        {
            ui.memory_mut(|memory| memory.request_focus(id));
        }
        for chart in charts.iter_mut() {
            chart.history.resizing = self
                .sizing
                .drag
                .as_ref()
                .map(|drag| &drag.history_guard)
                .or(self.sizing.pressed.as_ref())
                .map(Arc::downgrade)
                .unwrap_or_default();
        }
        if self.apply_history_steps(charts) {
            ui.ctx().request_repaint();
        }
        self.record_sizes(charts[self.active]);
        self.join_groups(charts);
        self.record_links(charts[self.active]);
        self.focus_active = false;
        // Keys go to the active chart, also when the wheel activated it and left the focus on
        // another chart. After the charts are shown, so a Tab the host turned into a new active
        // chart cannot move the focus on from it, and before a host dispatches the next frame.
        if !self.detached[self.active]
            && let Some(id) = charts[self.active].keyboard.chart_id
            && ui.memory(|memory| {
                memory.focused().is_some_and(|focused| {
                    focused != id
                        && charts
                            .iter()
                            .any(|chart| chart.keyboard.chart_id == Some(focused))
                })
            })
        {
            ui.memory_mut(|memory| memory.request_focus(id));
        }
        // Alt+Enter toggles the focused tile in a grid; a lone chart leaves it to the host.
        // A discarded pass does not repeat key events; leave its requests for the next pass.
        if self.mode != ChartLayoutMode::Tabs && count > 1 && !ui.ctx().will_discard() {
            for (index, chart) in charts.iter_mut().take(count).enumerate() {
                let requests = &mut chart.keyboard.requests;
                let before = requests.len();
                requests.retain(|request| {
                    *request != ShortcutRequest::Action(KeyboardAction::MaximizeChart)
                });
                if requests.len() != before {
                    self.maximized = if self.maximized == Some(index) {
                        None
                    } else {
                        Some(index)
                    };
                    self.set_active(index);
                    ui.ctx().request_repaint();
                }
            }
        }
        // Tab from a chart that dispatched while drawn; a lone chart leaves it to the host.
        if count > 1 {
            self.cycle_charts(ui, charts, count);
        }
        // A hidden follower was fitted at its old tile width. Fit the previous active chart's
        // UTC span only after the newly selected chart has its final width; otherwise selecting
        // it would enlarge the synchronized range. A late Tab waits until the next show.
        let mut fitted_focus = false;
        if let Some((from, to)) = self.focus_range {
            if to != self.active || !self.sync.sync_time_axis || !self.same_group(from, to) {
                self.focus_range = None;
            } else if rendered[to] {
                let changed_by_host = self.previous_ranges[to]
                    .zip(before_sync[to].2)
                    .is_some_and(|(previous, before)| previous != before);
                if !changed_by_host {
                    let range = self.pending_ranges[from]
                        .map(|pending| pending.range)
                        .or_else(|| charts[from].visible_time_range());
                    if let Some(range) = range {
                        let source = charts[from].sync_source();
                        match &mut self.pending_ranges[to] {
                            Some(pending) => {
                                pending.range = range;
                                pending.source = source;
                            }
                            None => {
                                fitted_focus =
                                    charts[to].apply_synced_time_range_from(range, source);
                            }
                        }
                    }
                }
                self.focus_range = None;
            }
        }
        // Apply gestures from the source after it has handled this frame's input.
        let sync_range = self.focus_range.is_none()
            && (fitted_focus
                || charts[self.active].visible_time_range() != before_range
                || self.previous_preset != Some((self.mode, count))
                || aligned.2);
        self.sync_view(charts, total, sync_range);
        self.previous_preset = Some((self.mode, count));
        if charts
            .iter()
            .zip(before_sync)
            .any(|(chart, (crosshair, (spacing, offset), _))| {
                chart.synced_crosshair_bar_idx != crosshair
                    || (chart.get_time_scale_state().0 - spacing).abs() > 0.001
                    || (chart.get_time_scale_state().1 - offset).abs() > 0.001
            })
        {
            ui.ctx().request_repaint();
        }
        #[cfg(feature = "drawings")]
        {
            let tracking = self.sync.sync_drawings
                || charts.iter().take(total).any(|chart| {
                    chart
                        .drawings()
                        .drawings
                        .iter()
                        .any(|d| d.folder_sync_mode().is_some())
                });
            if !tracking {
                // Turning the switch on is the first time for the drawings it mirrors, while
                // those that their folder mirrors stay linked on the charts away from them.
                self.drawings.forget_switched(charts, total);
            } else if self.drawings.sync(
                charts,
                total,
                &self.groups,
                pointer_down,
                self.sync.sync_drawings,
            ) {
                ui.ctx().request_repaint();
            }
        }
        #[cfg(feature = "drawings")]
        if Chart::share_stored_drawings(charts) {
            ui.ctx().request_repaint();
        }
        for (index, chart) in charts.iter().enumerate() {
            // A change a chart made while rendering, such as keyboard undo, stays unrecorded, so
            // the next frame sends it like one made between frames.
            if !self.state_recorded(index, chart) {
                ui.ctx().request_repaint();
            }
            self.previous_bars[index] = bars_key(chart);
            if self.pending_ranges[index].is_none()
                && let Some(range) = chart.visible_time_range()
            {
                self.previous_ranges[index] = Some(range);
            }
        }
        // The chart shown when a tab is clicked handles that click, so a group name typed in
        // its menu commits as the field loses focus. The clicked tab shows from the next frame.
        if let Some(index) = tab {
            self.active = index;
            ui.ctx().request_repaint();
        }
        response
    }

    fn restore_time_range(&mut self, index: usize, chart: &mut Chart) {
        // Reloads can leave a chart empty for several frames, and a host without a feed replaces
        // the old bars a frame or more after the switch; keep UTC edges until new bars arrive.
        if let Some(pending) = self.pending_ranges[index]
            && chart.visible_time_range().is_some()
            && bars_key(chart) != pending.bars
        {
            // A range the new bars do not cover keeps the view, but must not block the group.
            chart.apply_synced_time_range_from(pending.range, pending.source);
            self.pending_ranges[index] = None;
        }
    }

    fn state_recorded(&self, index: usize, chart: &Chart) -> bool {
        self.previous[index]
            .as_ref()
            .is_some_and(|p| p.0 == chart.symbol() && p.1 == chart.timeframe())
    }

    fn record_state(&mut self, index: usize, chart: &Chart) {
        if !self.state_recorded(index, chart) {
            self.previous[index] = Some((chart.symbol().to_owned(), chart.timeframe()));
        }
    }

    /// Sends the changes made in any chart since the last frame. The active chart's goes first,
    /// then the lowest slot's, so it wins over a change another member made in the same frame.
    fn sync_state(&mut self, charts: &mut [&mut Chart], count: usize) {
        let active = self.active;
        for from in std::iter::once(active).chain((0..count).filter(|&index| index != active)) {
            self.send_state(charts, count, from);
        }
    }

    /// Gives each chart that joined a symbol or interval group through a recorded membership
    /// change the symbol or interval of the group: the active chart's when it is a member,
    /// otherwise that of the lowest other slot in it.
    fn join_groups(&self, charts: &mut [&mut Chart]) {
        let count = charts.len();
        for change in &self.links.pending {
            let index = change.index;
            for (interval, joined) in [(false, &change.symbol), (true, &change.interval)] {
                if joined.is_none() || index >= count {
                    continue;
                }
                let Some(from) = std::iter::once(self.active)
                    .chain(0..count)
                    .find(|&from| from != index && self.linked(index, from, interval))
                else {
                    continue;
                };
                if interval {
                    if let Some(timeframe) = charts[from].timeframe() {
                        charts[index].set_timeframe(timeframe);
                    }
                } else {
                    let symbol = charts[from].symbol().to_owned();
                    charts[index].change_symbol(symbol, true);
                }
            }
        }
    }

    /// Gives the charts whose symbol or interval follows the layout setting the value of their
    /// view group, after its switch was turned on: the active chart's in its group, otherwise
    /// that of the lowest slot following the setting. Each keeps its undo history.
    fn align_followers(&self, charts: &mut [&mut Chart], symbol: bool, interval: bool) {
        let count = charts.len();
        for (on, interval) in [(symbol, false), (interval, true)] {
            if !on {
                continue;
            }
            let mut done = [false; MAX_CHARTS];
            for from in std::iter::once(self.active).chain(0..count) {
                if std::mem::replace(&mut done[from], true) {
                    continue;
                }
                for index in 0..count {
                    let groups = &self.links.groups[index];
                    let field = if interval {
                        &groups.interval
                    } else {
                        &groups.symbol
                    };
                    if done[index]
                        || *field != ChartLink::Layout
                        || !self.linked(index, from, interval)
                    {
                        continue;
                    }
                    done[index] = true;
                    if interval {
                        if let Some(timeframe) = charts[from].timeframe() {
                            charts[index].set_timeframe(timeframe);
                        }
                    } else {
                        let symbol = charts[from].symbol().to_owned();
                        charts[index].change_symbol(symbol, true);
                    }
                }
            }
        }
    }

    /// Sends the symbol and interval of chart `from`, where they differ from its recorded state,
    /// to the charts linked to it.
    fn send_state(&mut self, charts: &mut [&mut Chart], count: usize, from: usize) {
        let source = &charts[from];
        let Some(previous) = &self.previous[from] else {
            return;
        };
        let symbol_changed = previous.0 != source.symbol();
        let timeframe_changed = previous.1 != source.timeframe();
        if !symbol_changed && !timeframe_changed {
            return;
        }
        let symbol = source.symbol().to_owned();
        let timeframe = source.timeframe();
        // A linked chart keeps its history as the source did; a host change outside a user
        // action, which cleared the source's, clears it too.
        let keep_history = source.can_undo() || source.can_redo();
        for (index, chart) in charts.iter_mut().take(count).enumerate() {
            if index == from {
                continue;
            }
            if self.linked(index, from, false) && symbol_changed {
                chart.change_symbol(symbol.clone(), keep_history);
            }
            if self.linked(index, from, true)
                && timeframe_changed
                && let Some(tf) = timeframe
            {
                chart.set_timeframe(tf);
            }
        }
    }

    fn sync_view(&mut self, charts: &mut [&mut Chart], count: usize, sync_range: bool) {
        let hovered = charts
            .iter()
            .take(count)
            .position(|c| c.hover_time().is_some());
        let period = hovered.and_then(|source| charts[source].hover_period());
        // The source's main-pane price, or `None` over volume and indicator panes.
        let price = hovered.and_then(|source| charts[source].last_crosshair?.1);
        let range = charts[self.active].visible_time_range();
        let source = charts[self.active].sync_source();
        for (index, chart) in charts.iter_mut().take(count).enumerate() {
            chart.restore_host_crosshair();
            if self.sync.sync_crosshair
                && let Some(source) = hovered
                && index != source
                && self.same_group(index, source)
            {
                chart.set_synced_crosshair(period, price);
            }
            if index != self.active
                && self.same_group(index, self.active)
                && self.sync.sync_time_axis
                && self.pending_ranges[self.active].is_none()
                && sync_range
                && let Some(range) = range
            {
                // A chart awaiting its new interval's bars fits the latest range once they arrive.
                match &mut self.pending_ranges[index] {
                    Some(pending) => {
                        pending.range = range;
                        pending.source = source;
                    }
                    None => {
                        chart.apply_synced_time_range_from(range, source);
                    }
                }
            }
        }
    }

    /// The slot and rectangle of each visible chart inside `rect`, for an image export.
    /// Uses the on-screen arrangement, omitting detached and hidden slots and tab controls.
    /// `count` is the number of charts the host owns, including the active one.
    pub fn visible_chart_rects(&self, rect: Rect, count: usize) -> Vec<(usize, Rect)> {
        let count = count.min(MAX_CHARTS).min(self.mode.chart_count());
        self.rects(rect, count)
            .into_iter()
            .enumerate()
            .filter(|(slot, rect)| {
                !self.is_detached(*slot)
                    && rect.is_positive()
                    && (self.mode != ChartLayoutMode::Tabs
                        || *slot == self.active.min(count.saturating_sub(1)))
            })
            .collect()
    }

    fn geometry(&self, rect: Rect, count: usize) -> (Vec<Rect>, Vec<Divider>) {
        if self.mode == ChartLayoutMode::Tabs || self.maximized.is_some_and(|slot| slot < count) {
            return (self.rects(rect, count), Vec::new());
        }
        let split = self.mode.tracks().split();
        let minimum = split.minimum(count);
        let scale = Vec2::new(
            (rect.width() / minimum.x.max(1.0)).min(1.0),
            (rect.height() / minimum.y.max(1.0)).min(1.0),
        );
        let mut output = (Vec::new(), Vec::new());
        split.rects(
            rect,
            count,
            &self.sizes(self.mode).weights,
            &mut 0,
            &mut output,
            scale,
        );
        output
    }

    fn finish_resize(&mut self) {
        if let Some(drag) = self.sizing.drag.take() {
            let before = self.before_resize(&drag);
            if drag.weights == self.sizes(drag.mode).weights {
                self.sizing.sizes = before;
            } else {
                self.sizing.record(before);
            }
        }
    }

    fn before_resize(&self, drag: &DividerDrag) -> Vec<ChartLayoutSizing> {
        let mut before = self.saved_sizes();
        before.retain(|size| size.mode != drag.mode);
        before.extend(
            drag.before
                .iter()
                .filter(|size| size.mode == drag.mode)
                .cloned(),
        );
        before
    }

    fn move_divider(&mut self, ui: &Ui) -> bool {
        let Some(drag) = self.sizing.drag.as_ref() else {
            return false;
        };
        let Some(pos) = ui.input(|input| input.pointer.interact_pos()) else {
            self.finish_resize();
            return false;
        };
        let axis = usize::from(drag.dividers[0].rows);
        let mut delta = pos[axis] - drag.start[axis];
        for divider in &drag.dividers {
            let (min, max) = divider.limits();
            delta = delta.clamp(min, max);
        }
        let mut weights = drag.weights.clone();
        for divider in &drag.dividers {
            let (at, next) = (divider.at, divider.at + 1);
            let group = &mut weights[divider.group];
            // Charts sharing the split by weight have the most weight per point: a chart held at
            // its minimum is longer than its weight asks. Pricing both sides at that rate keeps
            // their sum when both share the split, and lets a held chart follow the pointer.
            let per_point = divider
                .lengths
                .iter()
                .zip(group.iter())
                .map(|(length, weight)| weight / length)
                .filter(|rate| rate.is_finite())
                .fold(0.0, f32::max);
            if per_point > 0.0 {
                group[at] = ((divider.lengths[at] + delta) * per_point).max(f32::MIN_POSITIVE);
                group[next] = ((divider.lengths[next] - delta) * per_point).max(f32::MIN_POSITIVE);
            }
        }
        if delta == 0.0 {
            let before = self.before_resize(drag);
            self.sizing.sizes = before;
        } else {
            let sizes = ChartLayoutSizing {
                mode: drag.mode,
                weights,
            };
            let _ = self.set_sizes(&[sizes], true);
        }
        if !ui.input(|input| input.pointer.primary_down()) {
            self.finish_resize();
        }
        ui.ctx().request_repaint();
        true
    }

    fn show_dividers(&mut self, ui: &mut Ui, dividers: &[Divider], count: usize) -> bool {
        let mut interacted = false;
        let mut pressed = false;
        let apply_input = !ui.ctx().will_discard();
        for divider in dividers {
            let key = (divider.group, divider.at);
            let response = ui
                .interact(
                    divider.rect.expand(4.0).intersect(ui.clip_rect()),
                    ui.scope_id().with(("chart_divider", self.mode, key)),
                    Sense::click_and_drag(),
                )
                .on_hover_and_drag_cursor(if divider.rows {
                    egui::CursorIcon::ResizeVertical
                } else {
                    egui::CursorIcon::ResizeHorizontal
                });
            let name = l10n::text(ui.ctx(), "Resize chart");
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Splitter, response.enabled(), &name)
            });
            interacted |= apply_input && (response.clicked() || response.drag_stopped());
            pressed |= response.is_pointer_button_down_on()
                && (self.sizing.selected == Some(key) || !ui.input(|input| input.any_touches()));
            if apply_input && response.double_clicked() {
                self.finish_resize();
                self.reset_sizes(false);
                ui.ctx().request_repaint();
            } else if apply_input
                && response.drag_started()
                && (self.sizing.selected == Some(key) || !ui.input(|input| input.any_touches()))
                && let Some(start) = ui.input(|input| input.pointer.press_origin())
            {
                let shift = ui.input(|input| input.modifiers.shift);
                self.sizing.drag = Some(DividerDrag {
                    history_guard: Arc::new(()),
                    mode: self.mode,
                    count,
                    start,
                    before: self.saved_sizes(),
                    weights: self.sizes(self.mode).weights,
                    dividers: dividers
                        .iter()
                        .filter(|other| {
                            (other.group == divider.group && other.at == divider.at)
                                || (shift
                                    && other.rows == divider.rows
                                    && (other.coordinate() - divider.coordinate()).abs() < 0.5)
                        })
                        .cloned()
                        .collect(),
                });
                self.sizing.selected = Some(key);
            } else if apply_input && response.clicked() {
                self.sizing.selected = Some(key);
            }
            if response.hovered() || response.dragged() || self.sizing.selected == Some(key) {
                let axis = usize::from(divider.rows);
                let mut line = divider.rect;
                line.min[axis] -= 1.0;
                line.max[axis] += 1.0;
                ui.painter()
                    .rect_filled(line, 0.0, ui.active_theme().ui.accent);
            }
        }
        if pressed && ui.input(|input| input.pointer.primary_down()) {
            self.sizing.pressed.get_or_insert_default();
        } else {
            self.sizing.pressed = None;
        }
        if ui.input(|input| input.pointer.any_pressed())
            && !dividers
                .iter()
                .any(|divider| ui.rect_contains_pointer(divider.rect.expand(4.0)))
        {
            self.sizing.selected = None;
        }
        interacted
    }

    fn rects(&self, rect: Rect, count: usize) -> Vec<Rect> {
        if self.mode == ChartLayoutMode::Tabs {
            return vec![rect; count];
        }
        if let Some(slot) = self.maximized.filter(|slot| *slot < count) {
            return (0..count)
                .map(|index| if index == slot { rect } else { Rect::NOTHING })
                .collect();
        }
        self.geometry(rect, count).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, BarData};
    use chrono::{DateTime, Duration};
    use egui::{Event, PointerButton, Pos2, RawInput, pos2, vec2};

    fn chart(step: i64) -> Chart {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let mut chart = Chart::new(BarData::from_bars(
            (0..100)
                .map(|i| {
                    Bar::new(
                        start + Duration::seconds(i * step),
                        10.0,
                        11.0,
                        9.0,
                        10.0,
                        1.0,
                    )
                })
                .collect(),
        ));
        chart.set_symbol("SAMPLE");
        chart.set_timeframe(Timeframe::Min1);
        chart
    }

    /// A chart that leaves its background to the layout gets its gradient over its slot.
    #[test]
    fn layout_paints_a_gradient_background() {
        use crate::config::BackgroundStyle;
        use egui::Color32;
        let (top, bottom) = (Color32::from_rgb(200, 0, 0), Color32::from_rgb(0, 0, 200));
        let mut chart = chart(60);
        chart.config_mut().skip_background = true;
        chart.config_mut().background_style = BackgroundStyle::VerticalGradient {
            top_color: top,
            bottom_color: bottom,
        };
        let size = vec2(800.0, 600.0);
        let output = egui::Context::default().run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| {
                ChartLayout::default().show(ui, &mut [&mut chart]);
            },
        );
        let slot = ChartLayout::default().rects(Rect::from_min_size(Pos2::ZERO, size), 1)[0];
        let meshes: Vec<Vec<_>> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) => Some(
                    mesh.vertices
                        .iter()
                        .map(|vertex| (vertex.pos, vertex.color))
                        .collect(),
                ),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        // The slot's fill is painted before the chart, so it is the first mesh.
        assert_eq!(
            meshes.first(),
            Some(&vec![
                (slot.left_top(), top),
                (slot.right_top(), top),
                (slot.right_bottom(), bottom),
                (slot.left_bottom(), bottom),
            ])
        );
    }

    /// Split size and link membership steps follow the layout value that shows their chart: a host
    /// that replaces its layout with a clone undoes and redoes them on the clone, including the
    /// changes still waiting for a show, and the replaced layout keeps its own. Steps undone or
    /// redone before a show apply in the order taken, and a restored chart drops its own.
    #[test]
    fn size_undo_after_layout_clone() {
        let show = |layout: &mut ChartLayout, charts: &mut [Chart; 3]| {
            egui::Context::default()
                .run_ui(RawInput::default(), |ui| {
                    layout.show(ui, &mut charts.each_mut());
                })
                .drop_without_applying_deltas();
        };
        let mode = ChartLayoutMode::ThreeHorizontal;
        let mut charts = [chart(60), chart(60), chart(60)];
        let mut layout = ChartLayout {
            mode,
            ..Default::default()
        };
        let thirds = layout.sizes(mode);
        let mut wider = thirds.clone();
        // One row of three columns; the first column doubled is 2/3 : 1/3 : 1/3 out of 4/3.
        wider.weights[1][0] *= 2.0;
        layout.set_sizes(&[wider], false).unwrap();
        let wider = ChartLayoutSizing {
            mode,
            weights: vec![vec![1.0], vec![0.5, 0.25, 0.25]],
        };
        assert_eq!(layout.sizes(mode), wider);
        show(&mut layout, &mut charts);
        // Not shown yet when the host replaces the layout.
        let linked = ChartLinkGroups {
            symbol: ChartLink::Group("A".into()),
            ..Default::default()
        };
        layout.set_link_groups(1, linked.clone(), false);
        layout.reset_sizes(false);
        let clone = layout.clone();
        let mut original = std::mem::replace(&mut layout, clone);
        let mut others = [chart(60), chart(60), chart(60)];
        show(&mut original, &mut others);
        show(&mut layout, &mut charts);
        let default_groups = ChartLinkGroups::default();
        // Undo the link, the reset and the first size change, then redo them.
        for (redo, sizes, groups) in [
            (false, &thirds, &default_groups),
            (false, &wider, &default_groups),
            (false, &thirds, &default_groups),
            (true, &wider, &default_groups),
            (true, &thirds, &default_groups),
            (true, &thirds, &linked),
        ] {
            assert!(if redo {
                charts[0].redo()
            } else {
                charts[0].undo()
            });
            show(&mut layout, &mut charts);
            show(&mut original, &mut others);
            assert_eq!(layout.sizes(mode), *sizes, "redo {redo}");
            assert_eq!(layout.link_groups(1).as_ref(), Some(groups), "redo {redo}");
            assert_eq!(
                original.sizes(mode),
                thirds,
                "the replaced layout keeps its sizes"
            );
            assert_eq!(original.link_groups(1), Some(linked.clone()));
        }
        assert!(!charts[0].redo());
        // A restore replaces the chart's history, with the steps its last undo queued.
        #[cfg(feature = "ui")]
        {
            assert!(charts[0].undo());
            let snapshot = charts[2].snapshot().unwrap();
            charts[0].restore(&snapshot).unwrap();
            show(&mut layout, &mut charts);
            assert_eq!(layout.link_groups(1), Some(linked.clone()));
        }
        // Two charts' steps of one preset, undone and redone before a show in the reverse order
        // of their recording, apply in the order taken.
        layout
            .set_sizes(std::slice::from_ref(&wider), false)
            .unwrap();
        show(&mut layout, &mut charts);
        layout.set_active(1);
        let middle = ChartLayoutSizing {
            mode,
            weights: vec![vec![1.0], vec![0.25, 0.5, 0.25]],
        };
        layout
            .set_sizes(std::slice::from_ref(&middle), false)
            .unwrap();
        show(&mut layout, &mut charts);
        assert!(charts[1].undo() && charts[0].undo());
        show(&mut layout, &mut charts);
        assert_eq!(layout.sizes(mode), thirds);
        assert!(charts[0].redo() && charts[1].redo());
        show(&mut layout, &mut charts);
        assert_eq!(layout.sizes(mode), middle);
    }

    /// The renderer receives the original slot of every visible chart in each layout path.
    #[test]
    fn host_renderer_receives_visible_slots_in_each_layout() {
        for (mode, maximized, detached, expected) in [
            (ChartLayoutMode::TwoHorizontal, None, false, vec![0, 1]),
            (ChartLayoutMode::Tabs, None, false, vec![1]),
            (ChartLayoutMode::TwoHorizontal, Some(1), false, vec![1]),
            (ChartLayoutMode::Tabs, None, true, vec![0, 1]),
        ] {
            let ctx = egui::Context::default();
            let mut charts = [chart(60), chart(60)];
            charts[0].set_symbol("FIRST");
            charts[1].set_symbol("SECOND");
            let mut layout = ChartLayout {
                mode,
                active: 1,
                maximized,
                ..Default::default()
            };
            layout.set_detached(0, detached);
            for _ in 0..3 {
                let mut visited = Vec::new();
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                        ..Default::default()
                    },
                    |ui| {
                        visited.clear();
                        layout.show_with(
                            ui,
                            &mut charts.iter_mut().collect::<Vec<_>>(),
                            |ui, index, chart| {
                                assert_eq!(chart.symbol(), ["FIRST", "SECOND"][index]);
                                assert_eq!(chart.layout_inactive, index != 1 && !detached);
                                visited.push(index);
                                chart.show(ui)
                            },
                        );
                    },
                );
                output.textures_delta.clear();
                assert_eq!(
                    visited, expected,
                    "{mode:?}, maximized={maximized:?}, detached={detached}"
                );
            }
        }
    }

    /// A border drag affects its neighbouring charts; Shift also moves aligned borders in other
    /// tracks. Double-click restores the preset, including at phone widths.
    #[test]
    fn dividers_resize_neighbours_link_aligned_borders_and_reset() {
        for width in [320.0, 1440.0] {
            for (shift, touch) in [(false, false), (true, false), (false, true)] {
                let bounds = Rect::from_min_size(pos2(10.0, 10.0), vec2(width, 720.0));
                let ctx = egui::Context::default();
                let mut layout = ChartLayout {
                    mode: ChartLayoutMode::Four,
                    ..Default::default()
                };
                layout.sync.sync_time_axis = false;
                let mut charts = [chart(60), chart(60), chart(60), chart(60)];
                let mut time = 0.0;
                let mut frame =
                    |layout: &mut ChartLayout, charts: &mut [Chart], events, modifiers| {
                        time += 0.1;
                        ctx.run_ui(
                            RawInput {
                                screen_rect: Some(bounds.expand(10.0)),
                                time: Some(time),
                                events: std::iter::once(Event::ModifiersChanged(modifiers))
                                    .chain(events)
                                    .collect(),
                                ..Default::default()
                            },
                            |ui| {
                                ui.scope_builder(egui::UiBuilder::new().max_rect(bounds), |ui| {
                                    layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                                });
                            },
                        )
                        .drop_without_applying_deltas();
                    };
                let modifiers = if shift {
                    egui::Modifiers::SHIFT
                } else {
                    egui::Modifiers::NONE
                };
                for _ in 0..2 {
                    frame(&mut layout, &mut charts, vec![], modifiers);
                }
                let before = layout.rects(bounds, 4);
                let scales: Vec<_> = charts.iter().map(Chart::get_time_scale_state).collect();
                let start = pos2(before[0].right(), before[0].center().y);
                let end = start + vec2(40.0, 0.0);
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                };
                let contact = |phase, pos| {
                    touch.then_some(Event::Touch {
                        device_id: egui::TouchDeviceId(1),
                        id: egui::TouchId(1),
                        phase,
                        pos,
                        force: None,
                    })
                };
                let press = |pos| {
                    let touch = contact(egui::TouchPhase::Start, pos);
                    std::iter::once(button(pos, true)).chain(touch).collect()
                };
                let drag_to = |pos| {
                    let touch = contact(egui::TouchPhase::Move, pos);
                    std::iter::once(Event::PointerMoved(pos))
                        .chain(touch)
                        .collect()
                };
                let release = |pos| {
                    let touch = contact(egui::TouchPhase::End, pos);
                    std::iter::once(button(pos, false)).chain(touch).collect()
                };
                frame(
                    &mut layout,
                    &mut charts,
                    vec![Event::PointerMoved(start)],
                    modifiers,
                );
                if touch {
                    frame(&mut layout, &mut charts, press(start), modifiers);
                    for fraction in [0.1, 0.5, 1.0] {
                        frame(
                            &mut layout,
                            &mut charts,
                            drag_to(start.lerp(end, fraction)),
                            modifiers,
                        );
                    }
                    frame(&mut layout, &mut charts, release(end), modifiers);
                    assert_eq!(
                        layout.rects(bounds, 4),
                        before,
                        "a touch moves only a border it selected"
                    );
                    frame(
                        &mut layout,
                        &mut charts,
                        vec![Event::PointerMoved(start)],
                        modifiers,
                    );
                    for _ in 0..6 {
                        frame(&mut layout, &mut charts, vec![], modifiers);
                    }
                    frame(&mut layout, &mut charts, press(start), modifiers);
                    frame(&mut layout, &mut charts, release(start), modifiers);
                    for _ in 0..6 {
                        frame(&mut layout, &mut charts, vec![], modifiers);
                    }
                }
                frame(&mut layout, &mut charts, press(start), modifiers);
                for fraction in [0.1, 0.5, 1.0] {
                    frame(
                        &mut layout,
                        &mut charts,
                        drag_to(start.lerp(end, fraction)),
                        modifiers,
                    );
                }
                let mut other = layout.sizes(ChartLayoutMode::TwoVertical);
                other.weights[0] = vec![3.0, 1.0];
                layout
                    .set_sizes(std::slice::from_ref(&other), true)
                    .unwrap();
                let other = layout.sizes(other.mode);
                for pos in [start, end] {
                    frame(&mut layout, &mut charts, drag_to(pos), modifiers);
                }
                frame(&mut layout, &mut charts, release(end), modifiers);
                frame(&mut layout, &mut charts, vec![], modifiers);
                let resized = layout.rects(bounds, 4);
                // Widths are W/2 + 40 and W/2 - 40; heights and the other row stay fixed.
                assert!(
                    (resized[0].width() - width / 2.0 - 40.0).abs() < 0.01,
                    "{width}, shift {shift}"
                );
                assert!((resized[1].width() - width / 2.0 + 40.0).abs() < 0.01);
                assert!(
                    (resized[2].width() - width / 2.0 - if shift { 40.0 } else { 0.0 }).abs()
                        < 0.01
                );
                assert_eq!(resized[0].height(), before[0].height());
                assert_eq!(layout.active(), 0);
                assert_eq!(
                    charts
                        .iter()
                        .map(Chart::get_time_scale_state)
                        .collect::<Vec<_>>(),
                    scales,
                    "a divider drag must not pan either chart"
                );
                let command = egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                };
                frame(
                    &mut layout,
                    &mut charts,
                    vec![Event::Key {
                        key: egui::Key::Z,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: command,
                    }],
                    command,
                );
                assert_eq!(
                    layout.rects(bounds, 4),
                    before,
                    "keyboard undo after a divider drag"
                );
                assert_eq!(
                    layout.sizes(other.mode),
                    other,
                    "a host update during dragging survives undo"
                );
                assert!(charts[0].redo());
                layout.apply_history_steps(&mut charts.each_mut());
                assert_eq!(layout.rects(bounds, 4), resized);
                // Wait out the drag's click before the two reset clicks.
                for _ in 0..6 {
                    frame(&mut layout, &mut charts, vec![], modifiers);
                }
                for _ in 0..2 {
                    frame(&mut layout, &mut charts, press(end), modifiers);
                    frame(&mut layout, &mut charts, release(end), modifiers);
                }
                frame(&mut layout, &mut charts, vec![], modifiers);
                assert_eq!(layout.rects(bounds, 4), before);
                assert!(charts[0].undo(), "reset must be one undoable action");
                layout.apply_history_steps(&mut charts.each_mut());
                assert_eq!(layout.rects(bounds, 4), resized);
            }
        }
    }

    /// Programmatic weights validate atomically, survive serialization and cloning, respect minimum
    /// sizes, and participate in the active chart's ordinary history only when requested.
    #[test]
    fn split_sizes_validate_restore_and_share_chart_history() {
        let bounds = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 722.0));
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::FourSplitLeft,
            ..Default::default()
        };
        let default = layout.visible_chart_rects(bounds, 4);
        let mut same = layout.sizes(layout.mode);
        for weights in &mut same.weights {
            for weight in weights {
                *weight *= 100.0;
            }
        }
        layout.set_sizes(&[same], false).unwrap();
        assert!(
            layout.saved_sizes().is_empty(),
            "equivalent ratios leave defaults unchanged"
        );
        let mut sizes = layout.sizes(layout.mode);
        // Outer halves: 3:1. The left half's columns: 2:1. Right rows stay equal.
        sizes.weights[0] = vec![3.0, 1.0];
        sizes.weights[2] = vec![2.0, 1.0];
        layout.set_sizes(&[sizes], false).unwrap();
        let resized = layout.visible_chart_rects(bounds, 4);
        for (slot, x, y, width, height) in [
            (0, 0.0, 0.0, 600.0, 722.0),
            (1, 600.0, 0.0, 300.0, 722.0),
            (2, 900.0, 0.0, 300.0, 360.0),
            (3, 900.0, 362.0, 300.0, 360.0),
        ] {
            let expected = Rect::from_min_size(pos2(x, y), vec2(width, height));
            assert!((resized[slot].1.min - expected.min).length() < 0.01);
            assert!((resized[slot].1.size() - expected.size()).length() < 0.01);
        }
        let saved = serde_json::to_string(&layout.saved_sizes()).unwrap();
        let mut copy = layout.clone();
        copy.reset_sizes(true);
        assert_eq!(
            layout.visible_chart_rects(bounds, 4),
            resized,
            "layout copies own their sizes"
        );
        copy.set_sizes(
            &serde_json::from_str::<Vec<ChartLayoutSizing>>(&saved).unwrap(),
            true,
        )
        .unwrap();
        assert_eq!(copy.visible_chart_rects(bounds, 4), resized);
        for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let mut other = layout.sizes(ChartLayoutMode::TwoVertical);
            other.weights[0] = vec![3.0, 1.0];
            let mut wrong = layout.sizes(layout.mode);
            wrong.weights[0][0] = invalid;
            let saved = layout.saved_sizes();
            assert!(layout.set_sizes(&[other, wrong], false).is_err());
            assert_eq!(
                layout.saved_sizes(),
                saved,
                "invalid batches leave all presets unchanged"
            );
            assert_eq!(layout.visible_chart_rects(bounds, 4), resized);
        }
        let mut wrong = layout.sizes(layout.mode);
        wrong.weights.pop();
        assert!(layout.set_sizes(&[wrong], false).is_err());
        let ctx = egui::Context::default();
        let mut charts = [chart(60), chart(60), chart(60), chart(60)];
        ctx.run_ui(RawInput::default(), |ui| {
            layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
        })
        .drop_without_applying_deltas();
        let mut hidden = layout.sizes(ChartLayoutMode::TwoVertical);
        hidden.weights[0] = vec![3.0, 1.0];
        layout.set_sizes(&[hidden], true).unwrap();
        let hidden = layout.sizes(ChartLayoutMode::TwoVertical);
        charts[0].user_action(|chart| chart.config_mut().show_grid = false);
        assert!(charts[0].undo());
        assert_eq!(layout.visible_chart_rects(bounds, 4), resized);
        assert!(charts[0].undo());
        layout.apply_history_steps(&mut charts.each_mut());
        assert_eq!(layout.visible_chart_rects(bounds, 4), default);
        assert_eq!(
            layout.sizes(ChartLayoutMode::TwoVertical),
            hidden,
            "undoing one preset must retain unrelated host size updates"
        );
        assert!(charts[0].redo());
        layout.apply_history_steps(&mut charts.each_mut());
        assert_eq!(layout.visible_chart_rects(bounds, 4), resized);
        layout.reset_sizes(true);
        assert_eq!(layout.visible_chart_rects(bounds, 4), default);
        let edits = charts[0].edit_count();
        ctx.run_ui(RawInput::default(), |ui| {
            layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
        })
        .drop_without_applying_deltas();
        assert_eq!(
            charts[0].edit_count(),
            edits,
            "the undo opt-out records no edit"
        );
        // A second drag must not consume the first size step, or a waiting redo.
        let bounds = Rect::from_min_size(pos2(10.0, 10.0), vec2(1440.0, 720.0));
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::ThreeHorizontal,
            ..Default::default()
        };
        let mut charts = [chart(60), chart(60), chart(60)];
        let mut time = 0.0;
        let mut frame = |layout: &mut ChartLayout, charts: &mut [Chart], events| {
            time += 0.1;
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(bounds.expand(10.0)),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(bounds), |ui| {
                        layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    });
                },
            )
            .drop_without_applying_deltas();
        };
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let assert_widths = |layout: &ChartLayout, expected: [f32; 3]| {
            for (rect, width) in layout.rects(bounds, 3).iter().zip(expected) {
                assert!(
                    (rect.width() - width).abs() < 0.01,
                    "{rect:?}, width {width}"
                );
            }
        };
        for _ in 0..2 {
            frame(&mut layout, &mut charts, vec![]);
        }
        assert_widths(&layout, [480.0, 480.0, 480.0]);
        let start = pos2(bounds.left() + 480.0, bounds.center().y);
        let first = start + vec2(120.0, 0.0);
        let second = first + vec2(60.0, 0.0);
        frame(
            &mut layout,
            &mut charts,
            vec![Event::PointerMoved(start), button(start, true)],
        );
        for pos in [start + vec2(4.0, 0.0), first] {
            frame(&mut layout, &mut charts, vec![Event::PointerMoved(pos)]);
        }
        frame(&mut layout, &mut charts, vec![button(first, false)]);
        assert_widths(&layout, [600.0, 360.0, 480.0]);
        charts[0].user_action(|chart| chart.config_mut().show_grid = false);
        assert!(charts[0].undo());
        let edits = charts[0].edit_count();
        let command = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let undo = || Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: command,
        };
        frame(&mut layout, &mut charts, vec![button(first, true)]);
        for pos in [first + vec2(12.0, 0.0), second] {
            frame(
                &mut layout,
                &mut charts,
                vec![
                    Event::PointerMoved(pos),
                    Event::ModifiersChanged(command),
                    undo(),
                ],
            );
            assert_eq!(
                charts[0].edit_count(),
                edits,
                "drag start preserves history"
            );
        }
        for key in [egui::Key::Z, egui::Key::Y] {
            frame(
                &mut layout,
                &mut charts,
                vec![
                    Event::ModifiersChanged(command),
                    Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: command,
                    },
                ],
            );
            assert_eq!(
                charts[0].edit_count(),
                edits,
                "{key:?} during a divider drag"
            );
            assert_widths(&layout, [660.0, 300.0, 480.0]);
            assert!(charts[0].can_undo() && charts[0].can_redo());
            for chart in &mut charts {
                assert!(
                    !chart.undo() && !chart.redo(),
                    "history is blocked on every chart"
                );
            }
        }
        // Ctrl+Z in the release frame undoes the drag it ends: 660 back to 600.
        frame(
            &mut layout,
            &mut charts,
            vec![
                button(second, false),
                Event::ModifiersChanged(command),
                undo(),
            ],
        );
        assert_eq!(
            charts[0].edit_count(),
            edits + 2,
            "one step on release, undone"
        );
        assert_widths(&layout, [600.0, 360.0, 480.0]);
        assert!(charts[0].undo());
        layout.apply_history_steps(&mut charts.each_mut());
        assert_widths(&layout, [480.0, 480.0, 480.0]);
        assert!(!charts[0].undo());
        for expected in [[600.0, 360.0, 480.0], [660.0, 300.0, 480.0]] {
            assert!(charts[0].redo());
            layout.apply_history_steps(&mut charts.each_mut());
            assert_widths(&layout, expected);
        }
        assert!(!charts[0].redo());
        frame(&mut layout, &mut charts, vec![button(second, true)]);
        frame(
            &mut layout,
            &mut charts,
            vec![Event::PointerMoved(second + vec2(8.0, 0.0))],
        );
        frame(&mut layout, &mut charts, vec![]);
        assert!(!charts[0].undo());
        drop(layout);
        assert!(
            charts[0].undo(),
            "dropping a dragged layout releases history"
        );

        for &mode in ChartLayoutMode::all() {
            let mut layout = ChartLayout {
                mode,
                ..Default::default()
            };
            let mut size = layout.sizes(mode);
            for weights in &mut size.weights {
                weights[0] = 1e6;
            }
            layout.set_sizes(&[size], true).unwrap();
            for width in [32.0, 320.0, 1440.0] {
                let rect = Rect::from_min_size(Pos2::ZERO, vec2(width, 720.0));
                for count in 1..=mode.chart_count() {
                    for (slot, cell) in layout.visible_chart_rects(rect, count) {
                        assert!(
                            cell.is_finite() && cell.is_positive(),
                            "{mode:?}, {width}, {count}"
                        );
                        assert!(rect.expand(0.01).contains_rect(cell), "{mode:?}, {cell:?}");
                        if mode == ChartLayoutMode::TwoHorizontal && count == 2 && slot == 1 {
                            assert!((cell.width() - 80.0_f32.min(width / 2.0)).abs() < 0.01);
                        }
                        if mode == ChartLayoutMode::TwoVertical && count == 2 && slot == 1 {
                            assert!((cell.height() - 60.0).abs() < 0.01);
                        }
                    }
                }
            }
        }
    }

    /// A host dispatches the active chart's shortcuts before it shows the layout, as the demo
    /// does, and Ctrl+Z comes in the frame a divider drag starts. The undo is ignored as it is
    /// when `show` dispatches, so the drag neither starts from undone sizes nor drops a step.
    #[test]
    fn undo_dispatched_before_the_layout_at_drag_start_keeps_size_steps() {
        let bounds = Rect::from_min_size(pos2(10.0, 10.0), vec2(1440.0, 720.0));
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::ThreeHorizontal,
            ..Default::default()
        };
        let mut charts = [chart(60), chart(60), chart(60)];
        let mut time = 0.0;
        let mut frame = |layout: &mut ChartLayout, charts: &mut [Chart], events| {
            time += 0.1;
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(bounds.expand(10.0)),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    charts[layout.active()].dispatch_shortcuts(ui.ctx());
                    ui.scope_builder(egui::UiBuilder::new().max_rect(bounds), |ui| {
                        layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    });
                },
            )
            .drop_without_applying_deltas();
        };
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let command = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let widths = |layout: &ChartLayout| -> Vec<f32> {
            layout
                .rects(bounds, 3)
                .iter()
                .map(|rect| rect.width().round())
                .collect()
        };
        for _ in 0..2 {
            frame(&mut layout, &mut charts, vec![]);
        }
        let start = pos2(bounds.left() + 480.0, bounds.center().y);
        let first = start + vec2(120.0, 0.0);
        let second = first + vec2(60.0, 0.0);
        frame(
            &mut layout,
            &mut charts,
            vec![Event::PointerMoved(start), button(start, true)],
        );
        for pos in [start + vec2(4.0, 0.0), first] {
            frame(&mut layout, &mut charts, vec![Event::PointerMoved(pos)]);
        }
        frame(&mut layout, &mut charts, vec![button(first, false)]);
        assert_eq!(widths(&layout), [600.0, 360.0, 480.0]);
        let edits = charts[0].edit_count();
        frame(&mut layout, &mut charts, vec![button(first, true)]);
        // The pointer passes egui's click distance, so the drag starts in this frame.
        frame(
            &mut layout,
            &mut charts,
            vec![
                Event::PointerMoved(first + vec2(12.0, 0.0)),
                Event::ModifiersChanged(command),
                Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: command,
                },
            ],
        );
        let at_start = (charts[0].edit_count() - edits, widths(&layout));
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(second)]);
        frame(&mut layout, &mut charts, vec![button(second, false)]);
        let released = widths(&layout);
        let mut undone = Vec::new();
        while charts[0].undo() {
            layout.apply_history_steps(&mut charts.each_mut());
            undone.push(widths(&layout));
        }
        assert_eq!(
            (at_start, released, undone),
            (
                (0, vec![600.0, 360.0, 480.0]),
                vec![660.0, 300.0, 480.0],
                vec![vec![600.0, 360.0, 480.0], vec![480.0, 480.0, 480.0]],
            )
        );
    }

    /// Only the active and hovered charts expose their scale settings in a layout.
    #[cfg(feature = "icons")]
    #[test]
    fn scale_settings_follow_layout_activity_and_hover() {
        for (mode, maximized, hovered, expected) in [
            (ChartLayoutMode::Single, None, false, 1),
            (ChartLayoutMode::Four, None, false, 1),
            (ChartLayoutMode::Four, None, true, 2),
            (ChartLayoutMode::Tabs, None, false, 1),
            (ChartLayoutMode::Four, Some(2), false, 1),
        ] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut charts = [chart(60), chart(60), chart(60), chart(60)];
            let mut layout = ChartLayout {
                mode,
                maximized,
                ..Default::default()
            };
            for _ in 0..2 {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
                        events: if hovered {
                            vec![Event::PointerMoved(pos2(900.0, 200.0))]
                        } else {
                            vec![Event::PointerGone]
                        },
                        ..Default::default()
                    },
                    |ui| {
                        layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    },
                );
                output.textures_delta.clear();
                let gears = output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .filter(|(_, node)| node.label() == Some("Scale settings"))
                    .count();
                assert_eq!(
                    gears, expected,
                    "{mode:?}, maximized={maximized:?}, hovered={hovered}"
                );
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn default_syncs_time_axis_and_crosshair_but_not_drawings() {
        let options = ChartSyncOptions::default();
        assert!(options.sync_time_axis);
        assert!(options.sync_crosshair);
        assert!(!options.sync_drawings);
    }

    #[test]
    fn presets_preserve_chart_state_and_route_input_to_visible_slots() {
        let rect = Rect::from_min_size(pos2(54.0, 70.0), vec2(1386.0, 716.0));
        // Saved presets restore their geometry and stable slot order at narrow and wide sizes.
        let regular = [
            ("SixTwoColumns", 2, 3),
            ("SevenHorizontal", 7, 1),
            ("EightVertical", 1, 8),
            ("EightTwoColumns", 2, 4),
            ("EightHorizontal", 8, 1),
            ("NineVertical", 1, 9),
            ("NineHorizontal", 9, 1),
            ("TenHorizontal", 10, 1),
            ("TenVertical", 1, 10),
            ("TenFiveColumns", 5, 2),
            ("TwelveHorizontal", 12, 1),
            ("TwelveThreeColumns", 3, 4),
            ("TwelveSixColumns", 6, 2),
            ("FourteenSevenColumns", 7, 2),
            ("SixteenEightColumns", 8, 2),
        ]
        .into_iter()
        .map(|(name, columns, rows)| {
            let width = 1200.0 / columns as f32;
            let height = 724.0 / rows as f32 - 2.0;
            (
                name,
                (columns, rows),
                (0..columns * rows)
                    .map(|slot| {
                        [
                            (slot % columns) as f32 * width,
                            (slot / columns) as f32 * (height + 2.0),
                            width,
                            height,
                        ]
                    })
                    .collect::<Vec<_>>(),
            )
        });
        // Half-height is 360, with a two-point gutter. Splitting a half again gives 179.
        let divided = [
            (
                "FourSplitLeft",
                (3, 2),
                vec![
                    [0.0, 0.0, 300.0, 722.0],
                    [300.0, 0.0, 300.0, 722.0],
                    [600.0, 0.0, 600.0, 360.0],
                    [600.0, 362.0, 600.0, 360.0],
                ],
            ),
            (
                "FourSplitRight",
                (3, 2),
                vec![
                    [0.0, 0.0, 600.0, 360.0],
                    [0.0, 362.0, 600.0, 360.0],
                    [600.0, 0.0, 300.0, 722.0],
                    [900.0, 0.0, 300.0, 722.0],
                ],
            ),
            (
                "FourSplitBottom",
                (2, 3),
                vec![
                    [0.0, 0.0, 600.0, 360.0],
                    [600.0, 0.0, 600.0, 360.0],
                    [0.0, 362.0, 1200.0, 179.0],
                    [0.0, 543.0, 1200.0, 179.0],
                ],
            ),
            (
                "FiveSplitLeft",
                (3, 3),
                vec![
                    [0.0, 0.0, 300.0, 722.0],
                    [300.0, 0.0, 300.0, 722.0],
                    [600.0, 0.0, 600.0, 718.0 / 3.0],
                    [600.0, 724.0 / 3.0, 600.0, 718.0 / 3.0],
                    [600.0, 1448.0 / 3.0, 600.0, 718.0 / 3.0],
                ],
            ),
            (
                "FiveSplitRight",
                (3, 3),
                vec![
                    [0.0, 0.0, 600.0, 718.0 / 3.0],
                    [0.0, 724.0 / 3.0, 600.0, 718.0 / 3.0],
                    [0.0, 1448.0 / 3.0, 600.0, 718.0 / 3.0],
                    [600.0, 0.0, 300.0, 722.0],
                    [900.0, 0.0, 300.0, 722.0],
                ],
            ),
            (
                "FiveTwoTop",
                (3, 2),
                vec![
                    [0.0, 0.0, 600.0, 360.0],
                    [600.0, 0.0, 600.0, 360.0],
                    [0.0, 362.0, 400.0, 360.0],
                    [400.0, 362.0, 400.0, 360.0],
                    [800.0, 362.0, 400.0, 360.0],
                ],
            ),
            (
                "SixTwoTop",
                (4, 2),
                vec![
                    [0.0, 0.0, 600.0, 360.0],
                    [600.0, 0.0, 600.0, 360.0],
                    [0.0, 362.0, 300.0, 360.0],
                    [300.0, 362.0, 300.0, 360.0],
                    [600.0, 362.0, 300.0, 360.0],
                    [900.0, 362.0, 300.0, 360.0],
                ],
            ),
            (
                "SixFourTop",
                (4, 2),
                vec![
                    [0.0, 0.0, 300.0, 360.0],
                    [300.0, 0.0, 300.0, 360.0],
                    [600.0, 0.0, 300.0, 360.0],
                    [900.0, 0.0, 300.0, 360.0],
                    [0.0, 362.0, 600.0, 360.0],
                    [600.0, 362.0, 600.0, 360.0],
                ],
            ),
            (
                "SevenLargeLeft",
                (2, 6),
                vec![
                    [0.0, 0.0, 600.0, 722.0],
                    [600.0, 0.0, 600.0, 712.0 / 6.0],
                    [600.0, 724.0 / 6.0, 600.0, 712.0 / 6.0],
                    [600.0, 1448.0 / 6.0, 600.0, 712.0 / 6.0],
                    [600.0, 2172.0 / 6.0, 600.0, 712.0 / 6.0],
                    [600.0, 2896.0 / 6.0, 600.0, 712.0 / 6.0],
                    [600.0, 3620.0 / 6.0, 600.0, 712.0 / 6.0],
                ],
            ),
            (
                "NineFiveTop",
                (5, 2),
                vec![
                    [0.0, 0.0, 240.0, 360.0],
                    [240.0, 0.0, 240.0, 360.0],
                    [480.0, 0.0, 240.0, 360.0],
                    [720.0, 0.0, 240.0, 360.0],
                    [960.0, 0.0, 240.0, 360.0],
                    [0.0, 362.0, 300.0, 360.0],
                    [300.0, 362.0, 300.0, 360.0],
                    [600.0, 362.0, 300.0, 360.0],
                    [900.0, 362.0, 300.0, 360.0],
                ],
            ),
        ];
        for (name, grid, expected) in regular.chain(divided) {
            let saved = format!("\"{name}\"");
            let mode: ChartLayoutMode = serde_json::from_str(&saved).expect(name);
            assert_eq!(serde_json::to_string(&mode).unwrap(), saved);
            assert!(ChartLayoutMode::all().contains(&mode));
            assert_eq!(mode.chart_count(), expected.len(), "{name}");
            assert_eq!(mode.grid_size(), grid, "{name}");
            for width in [320.0, 1440.0] {
                let bounds = Rect::from_min_size(pos2(54.0, 70.0), vec2(width, 722.0));
                let mut layout = ChartLayout {
                    mode,
                    ..Default::default()
                };
                let cells = layout.visible_chart_rects(bounds, 16);
                assert_eq!(cells.len(), expected.len());
                let ctx = egui::Context::default();
                let mut charts: Vec<_> = (0..16)
                    .map(|slot| {
                        let mut chart = chart(60);
                        chart.set_symbol(format!("SLOT_{slot}"));
                        chart.user_action(|chart| {
                            chart.set_chart_type(crate::model::ChartType::Line)
                        });
                        chart
                    })
                    .collect();
                let frame = |layout: &mut ChartLayout, charts: &mut [Chart], events| {
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(bounds.expand(100.0)),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            ui.scope_builder(egui::UiBuilder::new().max_rect(bounds), |ui| {
                                layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                            });
                        },
                    );
                    output.drop_without_applying_deltas();
                };
                frame(&mut layout, &mut charts, vec![]);
                layout.mode = ChartLayoutMode::Single;
                frame(&mut layout, &mut charts, vec![]);
                layout.mode = mode;
                frame(&mut layout, &mut charts, vec![]);
                for (slot, chart) in charts.iter_mut().enumerate() {
                    assert_eq!(chart.symbol(), format!("SLOT_{slot}"));
                    assert!(chart.undo(), "{name}, slot {slot} keeps its history");
                    assert_eq!(chart.chart_type(), crate::model::ChartType::Candles);
                    assert!(chart.redo());
                    assert_eq!(chart.chart_type(), crate::model::ChartType::Line);
                }
                #[cfg(feature = "ui")]
                {
                    let saved = super::super::SavedLayout {
                        mode,
                        charts: charts
                            .iter()
                            .map(|chart| chart.snapshot().unwrap())
                            .collect(),
                        sizing: None,
                        link_groups: None,
                    };
                    let restored =
                        super::super::SavedLayout::from_json(&saved.to_json().unwrap()).unwrap();
                    layout.mode = restored.mode;
                    assert_eq!(
                        layout.visible_chart_rects(bounds, restored.charts.len()),
                        cells
                    );
                    for (slot, snapshot) in restored.charts.iter().enumerate() {
                        let mut chart = chart(60);
                        chart.restore(snapshot).unwrap();
                        assert_eq!(chart.symbol(), format!("SLOT_{slot}"));
                    }
                }
                for (slot, cell) in cells {
                    let [x, y, w, h] = expected[slot];
                    let expected = Rect::from_min_size(
                        bounds.min + vec2(x * width / 1200.0, y),
                        vec2(w * width / 1200.0, h),
                    );
                    assert!((cell.min - expected.min).length() < 0.001, "{name}, {slot}");
                    assert!(
                        (cell.size() - expected.size()).length() < 0.001,
                        "{name}, {slot}"
                    );
                    assert!(cell.is_positive() && bounds.contains(cell.center()));
                    frame(
                        &mut layout,
                        &mut charts,
                        vec![
                            Event::PointerMoved(cell.center()),
                            Event::PointerButton {
                                pos: cell.center(),
                                button: PointerButton::Primary,
                                pressed: true,
                                modifiers: Default::default(),
                            },
                        ],
                    );
                    assert_eq!(layout.active(), slot, "{name}, width {width}");
                    frame(
                        &mut layout,
                        &mut charts,
                        vec![Event::PointerButton {
                            pos: cell.center(),
                            button: PointerButton::Primary,
                            pressed: false,
                            modifiers: Default::default(),
                        }],
                    );
                }
            }
        }
        let mut labels = std::collections::HashSet::new();
        for &mode in ChartLayoutMode::all() {
            assert!(labels.insert(mode.label()), "{mode:?}");
            let layout = ChartLayout {
                mode,
                ..Default::default()
            };
            for count in 0..=mode.chart_count() {
                let rects = layout.rects(rect, count);
                assert_eq!(rects.len(), count);
                if count == 0 {
                    continue;
                }
                if mode == ChartLayoutMode::Tabs {
                    continue;
                }
                let area: f32 = rects.iter().map(Rect::area).sum();
                // Each cell above the bottom edge leaves a two-point gap below it.
                let gaps: f32 = rects
                    .iter()
                    .filter(|cell| cell.bottom() < rect.bottom() - 1.0)
                    .map(|cell| 2.0 * cell.width())
                    .sum();
                assert!((area + gaps - rect.area()).abs() < 1.0, "{mode:?}");
                for (index, cell) in rects.iter().enumerate() {
                    assert!(rect.contains_rect(*cell));
                    assert!(
                        rects[..index]
                            .iter()
                            .all(|other| other.intersect(*cell).area() == 0.0)
                    );
                }
                if mode == ChartLayoutMode::Four && count == mode.chart_count() {
                    assert_eq!(rects[0], Rect::from_min_size(rect.min, vec2(693.0, 357.0)));
                    assert_eq!(rects[2].top(), 429.0);
                    assert_eq!(rects[1].left(), 747.0);
                }
                if mode == ChartLayoutMode::ThreeLargeLeft && count == mode.chart_count() {
                    assert_eq!(
                        rects,
                        [
                            Rect::from_min_size(rect.min, vec2(693.0, 716.0)),
                            Rect::from_min_size(pos2(747.0, 70.0), vec2(693.0, 357.0)),
                            Rect::from_min_size(pos2(747.0, 429.0), vec2(693.0, 357.0)),
                        ]
                    );
                }
            }
        }
        // Fewer charts than a preset holds fill its first rows, which share the full height.
        let sixteen = ChartLayout {
            mode: ChartLayoutMode::Sixteen,
            ..Default::default()
        };
        assert_eq!(
            sixteen.rects(rect, 6)[5],
            Rect::from_min_size(pos2(747.0, 429.0), vec2(693.0, 357.0))
        );
        let ctx = egui::Context::default();
        for width in [80.0, 240.0] {
            let clip = Rect::from_min_size(Pos2::ZERO, vec2(width, 600.0));
            let output = ctx.run_ui(RawInput::default(), |ui| {
                ui.set_clip_rect(clip);
                ChartLayout::default().show(ui, &mut [&mut chart(60)]);
            });
            let clipped = output.shapes.iter().all(|shape| {
                !matches!(shape.shape, egui::Shape::Text(_)) || clip.contains_rect(shape.clip_rect)
            });
            output.drop_without_applying_deltas();
            assert!(clipped, "a chart must respect its host's clip");
        }
        let mut charts = [chart(60), chart(120)];
        for chart in &mut charts {
            chart.set_symbol("LONG_SYMBOL_".repeat(12));
        }
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Tabs,
            ..Default::default()
        };
        layout.set_active(1);
        let frame = |layout: &mut ChartLayout, charts: &mut [Chart; 2], events| {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(640.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    assert!(
                        ui.min_rect().right() <= 640.0,
                        "long chart tabs must scroll inside the container"
                    );
                },
            );
            let mut group: std::collections::HashMap<_, _> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.text().to_owned(),
                        text.pos + text.galley.size() / 2.0,
                    )),
                    _ => None,
                })
                .collect();
            let text_rects: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.text(),
                        text.galley
                            .rect
                            .translate(text.pos.to_vec2())
                            .intersect(shape.clip_rect),
                        shape.clip_rect,
                    )),
                    _ => None,
                })
                .collect();
            for (_, selector, _) in text_rects.iter().filter(|(text, ..)| *text == "S ○") {
                // The hover data window is a separate overlay; only the chart's own clip holds titles.
                for (_, title, _) in text_rects.iter().filter(|(text, _, clip)| {
                    text.starts_with("LONG_SYMBOL_") && clip.left() == 0.0
                }) {
                    assert!(
                        !selector.intersects(*title),
                        "link controls must not cover the status title"
                    );
                }
            }
            // The menu fades in, so find the teardrop (the only eight-point marker) by shape, not colour.
            for shape in &output.shapes {
                if matches!(&shape.shape, egui::Shape::Path(path) if path.points.len() == 8) {
                    group.insert("💧".into(), shape.shape.visual_bounding_rect().center());
                }
            }
            output.drop_without_applying_deltas();
            group
        };
        frame(&mut layout, &mut charts, vec![]);
        let point = charts[1].get_rendered_price_rect().center();
        frame(
            &mut layout,
            &mut charts,
            vec![
                Event::PointerMoved(point),
                Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        assert_eq!(
            layout.active(),
            1,
            "clicking the tab's plot must not activate slot zero"
        );
        frame(
            &mut layout,
            &mut charts,
            vec![Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        layout.mode = ChartLayoutMode::TwoHorizontal;
        frame(&mut layout, &mut charts, vec![]);
        let point = charts[0].get_rendered_price_rect().center();
        frame(
            &mut layout,
            &mut charts,
            vec![
                Event::PointerMoved(point),
                Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        assert_eq!(layout.active(), 0);
        frame(
            &mut layout,
            &mut charts,
            vec![Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        // A group editor can overlap the next panel without activating that panel.
        let click = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let corner = pos2(308.0, 12.0);
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(corner)]);
        frame(&mut layout, &mut charts, vec![click(corner, true)]);
        frame(&mut layout, &mut charts, vec![click(corner, false)]);
        frame(&mut layout, &mut charts, vec![]);
        let input = frame(&mut layout, &mut charts, vec![])["Symbol group  ○"];
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(input)]);
        frame(&mut layout, &mut charts, vec![click(input, true)]);
        frame(&mut layout, &mut charts, vec![click(input, false)]);
        let emoji = frame(&mut layout, &mut charts, vec![])["💧"];
        assert!(emoji.x > 320.0, "the submenu overlaps the next chart");
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(emoji)]);
        frame(&mut layout, &mut charts, vec![click(emoji, true)]);
        frame(&mut layout, &mut charts, vec![click(emoji, false)]);
        assert_eq!(
            layout.link_groups(0).unwrap(),
            ChartLinkGroups {
                symbol: ChartLink::Group("💧".into()),
                interval: ChartLink::Layout,
            }
        );
        assert!(charts[0].undo());
        layout.apply_history_steps(&mut charts.each_mut());
        assert_eq!(layout.link_groups(0).unwrap(), ChartLinkGroups::default());
        assert!(charts[0].redo());
        layout.apply_history_steps(&mut charts.each_mut());
        assert_eq!(
            layout.link_groups(0).unwrap().symbol,
            ChartLink::Group("💧".into())
        );
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(corner)]);
        frame(&mut layout, &mut charts, vec![click(corner, true)]);
        frame(&mut layout, &mut charts, vec![click(corner, false)]);
        let input = frame(&mut layout, &mut charts, vec![])["     Symbol group"];
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(input)]);
        frame(&mut layout, &mut charts, vec![click(input, true)]);
        frame(&mut layout, &mut charts, vec![click(input, false)]);
        let emoji = frame(&mut layout, &mut charts, vec![])["💧"];
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(emoji)]);
        frame(&mut layout, &mut charts, vec![click(emoji, true)]);
        frame(&mut layout, &mut charts, vec![click(emoji, false)]);
        assert_eq!(
            layout.link_groups(0).unwrap(),
            ChartLinkGroups {
                symbol: ChartLink::Unlinked,
                interval: ChartLink::Layout,
            },
            "clicking the selected marker unlinks only its field"
        );
        assert_eq!(layout.active(), 0);
        let escape = Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        frame(&mut layout, &mut charts, vec![escape]);
        layout.set_sync_group(0, None);
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(point)]);
        frame(&mut layout, &mut charts, vec![click(point, true)]);
        frame(&mut layout, &mut charts, vec![click(point, false)]);
        layout.sync.sync_time_axis = false;
        for chart in &mut charts {
            chart.apply_synced_time_scale(3.0, 0.0);
        }
        #[cfg(feature = "drawings")]
        {
            // Delete applies only to the chart that received the pointer focus.
            for chart in &mut charts {
                let mut drawing = crate::drawings::Drawing::new(
                    0,
                    crate::drawings::DrawingToolType::HorizontalLine,
                );
                drawing.completed = true;
                drawing
                    .chart_points
                    .push(crate::drawings::ChartPoint::new(10.0, 10.0));
                chart.drawings_mut().add_synced_drawing(drawing).unwrap();
                let id = chart.drawings().drawings[0].id;
                chart.drawings_mut().select(id);
            }
            frame(
                &mut layout,
                &mut charts,
                vec![Event::Key {
                    key: egui::Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
            assert!(charts[0].drawings().drawings.is_empty());
            assert_eq!(charts[1].drawings().drawings.len(), 1);
            let id = charts[1].drawings().drawings[0].id;
            charts[1].drawings_mut().delete_drawing(id);
        }

        // Hidden chart tabs must not remain crosshair sources.
        charts[0].last_hover_bar_idx = Some(10.0);
        layout.mode = ChartLayoutMode::Tabs;
        layout.set_active(1);
        frame(&mut layout, &mut charts, vec![Event::PointerGone]);
        assert_eq!(charts[0].hover_time(), None);
        assert_eq!(charts[1].synced_crosshair_bar_idx, None);

        #[cfg(feature = "drawings")]
        {
            // Preset changes retain links, including edits and deletions while a slot is hidden.
            layout.mode = ChartLayoutMode::TwoHorizontal;
            layout.sync.sync_drawings = true;
            let mut drawing =
                crate::drawings::Drawing::new(0, crate::drawings::DrawingToolType::HorizontalLine);
            drawing.completed = true;
            drawing
                .chart_points
                .push(crate::drawings::ChartPoint::new(10.0, 10.0));
            charts[0]
                .drawings_mut()
                .add_synced_drawing(drawing)
                .unwrap();
            frame(&mut layout, &mut charts, vec![]);
            assert_eq!(charts[1].drawings().drawings.len(), 1);
            layout.mode = ChartLayoutMode::Single;
            frame(&mut layout, &mut charts, vec![]);
            layout.mode = ChartLayoutMode::TwoHorizontal;
            frame(&mut layout, &mut charts, vec![]);
            assert_eq!(charts[0].drawings().drawings.len(), 1);
            assert_eq!(charts[1].drawings().drawings.len(), 1);
            layout.mode = ChartLayoutMode::Single;
            let id = charts[0].drawings().drawings[0].id;
            charts[0].drawings_mut().delete_drawing(id);
            frame(&mut layout, &mut charts, vec![]);
            layout.mode = ChartLayoutMode::TwoHorizontal;
            frame(&mut layout, &mut charts, vec![]);
            assert!(charts[1].drawings().drawings.is_empty());
        }

        layout.set_detached(1, true);
        frame(&mut layout, &mut charts, vec![]);
        assert_eq!(charts[1].bars(super::super::SeriesId::MAIN).len(), 100);
        layout.set_detached(1, false);
        frame(&mut layout, &mut charts, vec![]);
        assert!(!layout.is_detached(1));
    }

    #[test]
    fn layout_keeps_a_crosshair_position_set_from_code() {
        use super::super::{SeriesId, SeriesOptions};

        for sync_crosshair in [false, true] {
            for setter in [0, 1, 2] {
                let ctx = egui::Context::default();
                let mut layout = ChartLayout {
                    mode: ChartLayoutMode::TwoHorizontal,
                    ..Default::default()
                };
                layout.sync.sync_crosshair = sync_crosshair;
                let mut charts = [chart(60), chart(60)];
                let series = charts[1].add_series(SeriesOptions::default());
                let bars = charts[1].bars(SeriesId::MAIN).to_vec();
                charts[1].set_data(series, bars);
                let mut time = 0.0;
                let mut frame = |layout: &mut ChartLayout, charts: &mut [Chart], events| {
                    time += 0.1;
                    ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                            time: Some(time),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                        },
                    )
                    .textures_delta
                    .clear();
                };
                frame(&mut layout, &mut charts, vec![]);
                let selected = charts[1].bars(SeriesId::MAIN)[50].time;
                match setter {
                    0 => charts[1].set_crosshair_position(Some((selected, 10.5))),
                    1 => charts[1].set_synced_crosshair_time(Some(selected)),
                    _ => charts[1].set_series_crosshair_position(series, Some((selected, 10.5))),
                }
                let price = (setter != 1).then_some(10.5);
                let target = (setter == 2).then_some(series);
                for _ in 0..2 {
                    frame(&mut layout, &mut charts, vec![Event::PointerGone]);
                    assert_eq!(charts[1].synced_crosshair_bar_idx, Some(50.0));
                    assert_eq!(charts[1].last_crosshair, Some((selected, price)));
                    assert_eq!(charts[1].synced_crosshair_series, target);
                }

                let pointer = pos2(
                    charts[0].index_to_x(80.0).unwrap(),
                    charts[0].last_rendered_price_rect.center().y,
                );
                for _ in 0..2 {
                    frame(&mut layout, &mut charts, vec![Event::PointerMoved(pointer)]);
                }
                let hovered = charts[0].last_crosshair.unwrap();
                assert_ne!(hovered.0, selected);
                assert_eq!(
                    charts[1].last_crosshair,
                    Some(if sync_crosshair {
                        hovered
                    } else {
                        (selected, price)
                    })
                );
                for _ in 0..2 {
                    frame(&mut layout, &mut charts, vec![Event::PointerGone]);
                }
                assert_eq!(charts[1].last_crosshair, Some((selected, price)));
                assert_eq!(charts[1].synced_crosshair_series, target);

                // An explicit local index replaces the saved time-based request.
                charts[1].set_synced_crosshair_bar_idx(Some(20.0));
                for _ in 0..2 {
                    frame(&mut layout, &mut charts, vec![]);
                }
                assert_eq!(charts[1].synced_crosshair_bar_idx, None);
                charts[1].set_crosshair_position(Some((selected, 10.5)));
                charts[1].set_crosshair_position(None);
                frame(&mut layout, &mut charts, vec![]);
                assert_eq!(charts[1].last_crosshair, None);
            }
        }
    }

    /// A chart reads its status line at a crosshair it receives, from a linked chart or the host,
    /// and at its latest bar once that crosshair is gone.
    #[test]
    fn status_line_reads_a_received_crosshair() {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        for linked in [true, false] {
            let ctx = egui::Context::default();
            let mut layout = ChartLayout {
                mode: ChartLayoutMode::TwoHorizontal,
                ..Default::default()
            };
            // Bar `i` of the second chart opens at 700 + i.
            let second = Chart::new(BarData::from_bars(
                (0..100)
                    .map(|i| {
                        let open = 700.0 + i as f64;
                        Bar::new(
                            start + Duration::minutes(i),
                            open,
                            open + 3.0,
                            open - 2.0,
                            open - 1.0,
                            1.0,
                        )
                    })
                    .collect(),
            ));
            let mut charts = [chart(60), second];
            let mut time = 0.0;
            let mut opens = |layout: &mut ChartLayout, charts: &mut [Chart], mut events| {
                let mut texts = Vec::new();
                for _ in 0..2 {
                    time += 0.1;
                    texts = frame_texts(&ctx, layout, charts, vec2(1440.0, 900.0), time, events);
                    events = vec![];
                }
                texts
                    .into_iter()
                    .map(|(text, ..)| text)
                    .filter(|text| text.starts_with("O 7"))
                    .collect::<Vec<_>>()
            };
            opens(&mut layout, &mut charts, vec![]);
            let events = if linked {
                vec![Event::PointerMoved(pos2(
                    charts[0].index_to_x(50.0).unwrap(),
                    charts[0].last_rendered_price_rect.center().y,
                ))]
            } else {
                charts[1].set_synced_crosshair_time(Some(start + Duration::minutes(50)));
                vec![]
            };
            assert_eq!(opens(&mut layout, &mut charts, events), ["O 750.00"]);
            #[cfg(feature = "ui")]
            assert_eq!(
                crate::ui::widget_bar::panels::object_tree::DataWindowInfo::from_chart(&charts[1])
                    .close,
                749.0
            );
            if !linked {
                charts[1].set_synced_crosshair_time(None);
            }
            assert_eq!(
                opens(&mut layout, &mut charts, vec![Event::PointerGone]),
                ["O 799.00"],
                "linked {linked}"
            );
        }
    }

    /// Linked charts mark a time the pointer reaches after the last bar at the slot holding it:
    /// eight one-minute slots after bar 99, opened 22:13:20 on Tuesday 14 November 2023, open
    /// at 00:00:20 the next day. One-minute bars from 22:13:00 hold it in their slot 107, opened
    /// at midnight, and twenty five-minute bars from 22:13:00 in their slot 21, opened at 23:58.
    #[test]
    fn linked_crosshair_marks_a_time_after_the_last_bar() {
        let start = DateTime::from_timestamp(1_699_999_980, 0).unwrap();
        let whole_minutes = |count: i64, minutes: i64, timeframe| {
            let mut chart = Chart::new(BarData::from_bars(
                (0..count)
                    .map(|i| {
                        let time = start + Duration::minutes(minutes * i);
                        Bar::new(time, 10.0, 11.0, 9.0, 10.0, 1.0)
                    })
                    .collect(),
            ));
            chart.set_symbol("SAMPLE");
            chart.set_timeframe(timeframe);
            chart
        };
        let (one, five) = (
            whole_minutes(100, 1, Timeframe::Min1),
            whole_minutes(20, 5, Timeframe::Min5),
        );
        for (receiver, slot, label) in [
            (one, 107.0, "Wed 15 Nov '23 00:00"),
            (five, 21.0, "Tue 14 Nov '23 23:58"),
        ] {
            let ctx = egui::Context::default();
            let mut layout = ChartLayout {
                mode: ChartLayoutMode::TwoHorizontal,
                ..Default::default()
            };
            let mut charts = [chart(60), receiver];
            let size = vec2(1440.0, 900.0);
            frame_texts(&ctx, &mut layout, &mut charts, size, 0.0, vec![]);
            // Twenty empty slots after the last bar of each chart, at a spacing whose slots do not
            // fall on whole screen points.
            for chart in &mut charts {
                chart.apply_synced_time_scale(6.7, 20.0);
            }
            frame_texts(&ctx, &mut layout, &mut charts, size, 0.1, vec![]);
            let pointer = pos2(
                charts[0].index_to_x(107.0).unwrap(),
                charts[0].last_rendered_price_rect.center().y,
            );
            let marked = |texts: &[(String, Rect, Pos2)], chart: &Chart| {
                let x = chart.index_to_x(slot).unwrap();
                let bottom = chart.last_rendered_price_rect.bottom();
                texts.iter().any(|(text, _, centre)| {
                    text == label && (centre.x - x).abs() < 2.0 && (centre.y - bottom).abs() < 30.0
                })
            };
            let mut texts = Vec::new();
            for time in [0.2, 0.3] {
                let events = vec![Event::PointerMoved(pointer)];
                texts = frame_texts(&ctx, &mut layout, &mut charts, size, time, events);
            }
            assert!(marked(&texts, &charts[1]), "{label}");
            for time in [0.4, 0.5] {
                let events = vec![Event::PointerGone];
                texts = frame_texts(&ctx, &mut layout, &mut charts, size, time, events);
            }
            assert!(
                !marked(&texts, &charts[1]),
                "{label} after the pointer left"
            );
        }
    }

    /// A time sent without a price, from a pointer over a study pane, draws the vertical guide
    /// and the time label alone, with no price label in the normal or magnet crosshair mode.
    #[cfg(feature = "studies")]
    #[test]
    fn time_only_crosshair_draws_no_price_guide() {
        for mode in [
            crate::config::CrosshairMode::Normal,
            crate::config::CrosshairMode::Magnet,
        ] {
            let ctx = egui::Context::default();
            let mut layout = ChartLayout {
                mode: ChartLayoutMode::TwoHorizontal,
                ..Default::default()
            };
            let mut charts = [chart(60), chart(60)];
            for chart in &mut charts {
                chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            }
            charts[1].set_crosshair_mode(mode);
            let size = vec2(1440.0, 900.0);
            frame_texts(&ctx, &mut layout, &mut charts, size, 0.0, vec![]);
            // The texts painted on the receiver's price axis, beside its price pane.
            let axis = |texts: &[(String, Rect, Pos2)], chart: &Chart| -> Vec<String> {
                let plot = chart.last_rendered_price_rect;
                texts
                    .iter()
                    .filter(|(_, _, centre)| {
                        centre.x > plot.right() && plot.y_range().contains(centre.y)
                    })
                    .map(|(text, ..)| text.clone())
                    .collect()
            };
            let idle = frame_texts(&ctx, &mut layout, &mut charts, size, 0.1, vec![]);
            let pane = charts[0].get_rendered_indicator_panes()[0].panel_rect;
            let pointer = pos2(charts[0].index_to_x(40.0).unwrap(), pane.center().y);
            let mut texts = Vec::new();
            for time in [0.2, 0.3] {
                let events = vec![Event::PointerMoved(pointer)];
                texts = frame_texts(&ctx, &mut layout, &mut charts, size, time, events);
            }
            assert_eq!(charts[1].synced_crosshair_bar_idx, Some(40.0), "{mode:?}");
            assert_eq!(charts[1].last_crosshair.map(|(_, price)| price), Some(None));
            assert_eq!(
                axis(&texts, &charts[1]),
                axis(&idle, &charts[1]),
                "{mode:?}"
            );
        }
    }

    /// A range chosen after scrolls were requested, as after Go to date, ends them on the active
    /// chart and on the chart that takes its span, so the next frame still shows the range.
    #[test]
    fn chosen_date_range_ends_pending_scrolls() {
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        let mut charts = [chart(60), chart(60)];
        let frame = |layout: &mut ChartLayout, charts: &mut [Chart; 2]| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                    ..Default::default()
                },
                |ui| {
                    layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                },
            )
            .drop_without_applying_deltas();
        };
        frame(&mut layout, &mut charts);
        let shown = charts[0].state.shown();
        let range = super::super::VisibleTimeRange::new(shown[10].time, shown[30].time);
        for chart in &mut charts {
            chart.scroll_to_time(chart.state.shown()[80].time);
        }
        layout.apply_date_range(&mut charts.iter_mut().collect::<Vec<_>>(), range);
        frame(&mut layout, &mut charts);
        for chart in &charts {
            let actual = chart.visible_time_range().unwrap();
            for error in [actual.from - range.from, actual.to - range.to] {
                assert!(error.num_milliseconds().abs() < 1, "{actual:?}");
            }
        }
    }

    #[test]
    fn sync_switches_and_groups_isolate_changes() {
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Four,
            ..Default::default()
        };
        let mut charts = [chart(60), chart(120), chart(60), chart(60)];
        let mut refs: Vec<_> = charts.iter_mut().collect();
        layout.set_sync_group(2, Some("B".into()));
        layout.previous[0] = Some(("SAMPLE".into(), Some(Timeframe::Min1)));
        refs[0].set_symbol("OTHER");
        refs[0].set_timeframe(Timeframe::Hour1);
        layout.sync_state(&mut refs, 4);
        assert_eq!(refs[1].symbol(), "SAMPLE");
        layout.sync.sync_symbol = true;
        layout.sync.sync_timeframe = true;
        layout.sync_state(&mut refs, 4);
        assert_eq!(
            (refs[1].symbol(), refs[1].timeframe()),
            ("OTHER", Some(Timeframe::Hour1))
        );
        assert_eq!(
            (refs[2].symbol(), refs[2].timeframe()),
            ("SAMPLE", Some(Timeframe::Min1))
        );
        // The hovered chart emits crosshairs without becoming the selected chart.
        refs[1].last_hover_bar_idx = Some(2.0);
        let time = refs[1].hover_time().unwrap();
        refs[1].last_crosshair = Some((time, Some(10.5)));
        layout.sync_view(&mut refs, 4, false);
        assert_eq!(refs[0].synced_crosshair_bar_idx, Some(4.0));
        assert_eq!(refs[0].synced_crosshair_price, Some(10.5));
        assert_eq!(refs[2].synced_crosshair_bar_idx, None);
        refs[1].last_hover_bar_idx = None;
        layout.sync_view(&mut refs, 4, false);
        assert_eq!(refs[0].synced_crosshair_bar_idx, None);
        assert_eq!(refs[0].synced_crosshair_price, None);
        refs[1].last_hover_bar_idx = Some(2.0);
        layout.sync.sync_crosshair = false;
        layout.sync_view(&mut refs, 4, false);
        assert_eq!(refs[0].synced_crosshair_bar_idx, None);
        let range = super::super::VisibleTimeRange {
            from: refs[0].state.shown()[10].time,
            to: refs[0].state.shown()[30].time,
        };
        // Shared scrolling and zooming passes a chosen range on; without it only "Date range"
        // does, and other groups keep their view either way.
        let before = refs[1].get_time_scale_state();
        let isolated = refs[2].get_time_scale_state();
        // New York is EST (UTC-5) on 14 November 2023. A fixed offset also exercises
        // local ranges in builds without the named-zone database.
        refs[0].chart_options.time_scale.timezone = crate::scales::TimezoneMode::Timezone(
            crate::model::TimezoneId::fixed_offset(-5 * 3600).unwrap(),
        );
        let day = chrono::NaiveDate::from_ymd_opt(2023, 11, 14).unwrap();
        let time = |hour, minute| chrono::NaiveTime::from_hms_opt(hour, minute, 0);
        let utc = |hour, minute| {
            DateTime::from_naive_utc_and_offset(
                day.and_hms_opt(hour, minute, 0).unwrap(),
                chrono::Utc,
            )
        };
        for (local, sync_time_axis, sync_date_range, moves) in [
            (false, true, false, true),
            (false, false, false, false),
            (false, false, true, true),
            (true, true, false, true),
            (true, false, false, false),
            (true, false, true, true),
        ] {
            layout.sync.sync_time_axis = sync_time_axis;
            layout.sync.sync_date_range = sync_date_range;
            refs[1].apply_synced_time_scale(before.0, before.1);
            if local {
                layout.apply_local_date_range(&mut refs, day, time(17, 30), day, time(18, 0));
                let span = refs[0].visible_time_range().unwrap();
                assert!(
                    (span.from - utc(22, 30)).abs() <= Duration::seconds(60),
                    "{span:?}"
                );
                assert!(
                    (span.to - utc(23, 0)).abs() <= Duration::seconds(60),
                    "{span:?}"
                );
            } else {
                layout.apply_date_range(&mut refs, range);
            }
            assert_eq!(
                refs[1].get_time_scale_state() != before,
                moves,
                "local={local} {sync_time_axis} {sync_date_range}"
            );
            assert_eq!(refs[2].get_time_scale_state(), isolated);
        }
        for (sync_time_axis, sync_date_range, follower) in [
            (true, true, None),
            (false, false, None),
            (false, true, Some(utc(17, 30))),
        ] {
            layout.sync.sync_time_axis = sync_time_axis;
            layout.sync.sync_date_range = sync_date_range;
            for chart in refs.iter_mut() {
                chart.pending_scroll_time = None;
            }
            layout.go_to_local_time(&mut refs, day, time(17, 30));
            assert_eq!(refs[0].pending_scroll_time, Some(utc(22, 30)));
            assert_eq!(refs[1].pending_scroll_time, follower);
            assert_eq!(refs[2].pending_scroll_time, None);
        }
        // Bad host ranges, including equal instants, leave every chart's view alone.
        for end in [time(17, 0), time(17, 30)] {
            let before: Vec<_> = refs
                .iter()
                .map(|chart| chart.get_time_scale_state())
                .collect();
            layout.apply_local_date_range(&mut refs, day, time(17, 30), day, end);
            assert_eq!(
                refs.iter()
                    .map(|chart| chart.get_time_scale_state())
                    .collect::<Vec<_>>(),
                before
            );
        }
        // An absent active slot cannot navigate its group.
        for chart in refs.iter_mut() {
            chart.pending_scroll_time = None;
        }
        layout.active = 5;
        layout.go_to_local_time(&mut refs, day, time(17, 30));
        assert!(refs.iter().all(|chart| chart.pending_scroll_time.is_none()));
        layout.active = 0;
        layout.sync.sync_time_axis = true;

        // Clearing the legacy picker must exclude a chart, rather than joining unnamed charts.
        {
            let mut cleared = ChartLayout::default();
            cleared.sync.sync_symbol = true;
            cleared.sync.sync_timeframe = true;
            cleared.set_sync_group(1, None);
            cleared.previous[0] = Some(("SAMPLE".into(), Some(Timeframe::Min1)));
            let mut charts = [chart(60), chart(60)];
            charts[0].set_symbol("OTHER");
            charts[0].set_timeframe(Timeframe::Hour1);
            cleared.sync_state(&mut charts.iter_mut().collect::<Vec<_>>(), 2);
            assert_eq!(charts[1].symbol(), "SAMPLE", "cleared symbol stays local");
            assert_eq!(
                charts[1].timeframe(),
                Some(Timeframe::Min1),
                "cleared interval stays local"
            );
        }

        // Explicit field memberships work with either layout switch setting. Matching an
        // interval marker to a symbol marker must never couple the two fields.
        for enabled in [false, true] {
            use ChartLink::{Group, Unlinked};
            let mut linked = ChartLayout::default();
            linked.sync.sync_symbol = enabled;
            linked.sync.sync_timeframe = enabled;
            let memberships = [
                (Group("🔶".into()), Group("💧".into()), true, true),
                (Group("🔶".into()), Group("🍀".into()), true, false),
                (Group("🍀".into()), Group("💧".into()), false, true),
                (Group("💧".into()), Group("🔶".into()), false, false),
                (Unlinked, Unlinked, false, false),
                (Group("".into()), Group("  ".into()), false, false),
                (ChartLink::Layout, ChartLink::Layout, false, false),
                (Unlinked, Group("💧".into()), false, true),
                (Group("🔶".into()), Unlinked, true, false),
            ];
            let mut charts: Vec<_> = memberships.iter().map(|_| chart(60)).collect();
            for (slot, (symbol, interval, ..)) in memberships.iter().enumerate() {
                linked.set_link_groups(
                    slot,
                    ChartLinkGroups {
                        symbol: symbol.clone(),
                        interval: interval.clone(),
                    },
                    true,
                );
            }
            linked.previous[0] = Some(("SAMPLE".into(), Some(Timeframe::Min1)));
            charts[0].set_symbol("OTHER");
            charts[0].set_timeframe(Timeframe::Hour1);
            linked.sync_state(
                &mut charts.iter_mut().collect::<Vec<_>>(),
                memberships.len(),
            );
            for (chart, (_, _, symbol, interval)) in charts.iter().zip(&memberships) {
                assert_eq!(chart.symbol(), if *symbol { "OTHER" } else { "SAMPLE" });
                assert_eq!(
                    chart.timeframe(),
                    Some(if *interval {
                        Timeframe::Hour1
                    } else {
                        Timeframe::Min1
                    })
                );
            }
            // An unlinked source cannot send updates either, even with both global switches on.
            linked.active = 4;
            linked.previous[4] = Some(("SAMPLE".into(), Some(Timeframe::Min1)));
            charts[4].set_symbol("LOCAL");
            charts[4].set_timeframe(Timeframe::Day1);
            linked.sync_state(
                &mut charts.iter_mut().collect::<Vec<_>>(),
                memberships.len(),
            );
            assert_eq!(charts[0].symbol(), "OTHER");
            assert_eq!(charts[0].timeframe(), Some(Timeframe::Hour1));

            let mut groups = linked.link_groups(0).unwrap();
            groups.symbol = Unlinked;
            linked.set_link_groups(0, groups.clone(), false);
            linked.record_links(&mut charts[0]);
            groups.interval = Group("Host".into());
            linked.set_link_groups(0, groups.clone(), true);
            assert!(charts[0].undo());
            linked.apply_history_steps(&mut [&mut charts[0]]);
            groups.symbol = Group("🔶".into());
            assert_eq!(linked.link_groups(0), Some(groups.clone()));
            assert!(charts[0].redo());
            linked.apply_history_steps(&mut [&mut charts[0]]);
            groups.symbol = Unlinked;
            assert_eq!(linked.link_groups(0), Some(groups.clone()));
            let copy = linked.clone();
            assert!(charts[0].undo());
            assert_eq!(copy.link_groups(0), Some(groups));
            let saved = serde_json::to_string(&copy.saved_link_groups()).unwrap();
            let restored: [ChartLinkGroups; 16] = serde_json::from_str(&saved).unwrap();
            assert_eq!(restored, copy.saved_link_groups());
        }

        // A minute chart at the spacing floor must retain its UTC view after an hourly reload.
        let ctx = egui::Context::default();
        for mode in [ChartLayoutMode::Four, ChartLayoutMode::Tabs] {
            let mut layout = ChartLayout {
                mode,
                ..Default::default()
            };
            let mut charts = [chart(60), chart(60), chart(60), chart(60)];
            let frame = |layout: &mut ChartLayout, charts: &mut [Chart; 4]| {
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                        ..Default::default()
                    },
                    |ui| {
                        layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    },
                )
                .drop_without_applying_deltas();
            };
            frame(&mut layout, &mut charts);
            for active in [0, 1] {
                layout.set_active(active);
                frame(&mut layout, &mut charts);
                charts[active].apply_synced_time_scale(0.5, 0.0);
                frame(&mut layout, &mut charts);
                let remembered = charts[active].visible_time_range().unwrap();
                charts[active].set_timeframe(Timeframe::Hour1);
                charts[active].set_data(super::super::SeriesId::MAIN, vec![]);
                frame(&mut layout, &mut charts);
                let others: Vec<_> = charts.iter().map(Chart::get_time_scale_state).collect();
                frame(&mut layout, &mut charts);
                for (index, chart) in charts.iter().enumerate() {
                    if index != active {
                        assert_eq!(chart.get_time_scale_state(), others[index]);
                    }
                }
                charts[active].set_data(
                    super::super::SeriesId::MAIN,
                    chart(3600).state.shown().to_vec(),
                );
                frame(&mut layout, &mut charts);
                // A 60x longer interval needs 60x the spacing for the same UTC span.
                assert!((charts[active].get_time_scale_state().0 - 30.0).abs() < 0.01);
                let actual = charts[active].visible_time_range().unwrap();
                assert!((actual.from - remembered.from).num_milliseconds().abs() < 1);
                assert!((actual.to - remembered.to).num_milliseconds().abs() < 1);
            }
            layout.set_active(0);
            charts[0].set_visible_bars(65);
            frame(&mut layout, &mut charts);
            for chart in &charts {
                let scale = chart.state.time_scale();
                let range = scale.visible_logical_range();
                assert!(scale.bar_spacing().is_finite() && scale.bar_spacing() >= 0.5);
                assert!(range.left.ceil().max(0.0) <= range.right.floor().min(99.0));
            }
            // Bars that miss the remembered range keep the view and must not block the group.
            layout.set_active(2);
            frame(&mut layout, &mut charts);
            charts[2].set_timeframe(Timeframe::Day1);
            charts[2].set_data(super::super::SeriesId::MAIN, vec![]);
            frame(&mut layout, &mut charts);
            let mut later = chart(86_400).state.shown().to_vec();
            later
                .iter_mut()
                .for_each(|bar| bar.time += Duration::days(3650));
            charts[2].set_data(super::super::SeriesId::MAIN, later);
            frame(&mut layout, &mut charts);
            assert!(layout.pending_ranges.iter().all(Option::is_none));

            // A chart still loading a synced interval adopts the group's latest range, so its
            // arrival does not pull the group back to the range from before the switch, also
            // past 50 px per bar and when its old bars are replaced, not emptied, a frame later.
            for emptied in [true, false] {
                let mut layout = ChartLayout {
                    mode,
                    ..Default::default()
                };
                layout.sync.sync_timeframe = true;
                let mut charts = [chart(60), chart(60), chart(60), chart(60)];
                frame(&mut layout, &mut charts);
                charts[0].set_timeframe(Timeframe::Hour1);
                for chart in charts.iter_mut().filter(|_| emptied) {
                    chart.set_data(super::super::SeriesId::MAIN, vec![]);
                }
                frame(&mut layout, &mut charts);
                let hourly = chart(3600).state.shown().to_vec();
                charts[0].set_data(super::super::SeriesId::MAIN, hourly.clone());
                frame(&mut layout, &mut charts);
                charts[0].set_visible_bars(8);
                frame(&mut layout, &mut charts);
                assert!(charts[0].get_time_scale_state().0 > 50.0);
                let latest = charts[0].visible_time_range().unwrap();
                charts[1].set_data(super::super::SeriesId::MAIN, hourly);
                layout.set_active(1);
                frame(&mut layout, &mut charts);
                for chart in &charts[..2] {
                    let actual = chart.visible_time_range().unwrap();
                    assert!((actual.from - latest.from).num_milliseconds().abs() < 1);
                    assert!((actual.to - latest.to).num_milliseconds().abs() < 1);
                }
            }

            // Charts of one interval zoom together past the cross-interval spacing cap.
            let mut layout = ChartLayout {
                mode,
                ..Default::default()
            };
            let mut charts = [chart(60), chart(60), chart(60), chart(60)];
            frame(&mut layout, &mut charts);
            charts[0].set_visible_bars(8);
            frame(&mut layout, &mut charts);
            assert!(charts[0].get_time_scale_state().0 > 72.0);
            let source = charts[0].visible_time_range().unwrap();
            for chart in &charts[1..] {
                let actual = chart.visible_time_range().unwrap();
                assert!((actual.from - source.from).num_milliseconds().abs() < 1);
                assert!((actual.to - source.to).num_milliseconds().abs() < 1);
            }
        }
    }

    /// Six charts that differ in symbol, interval and span, all switches off: slot 0 is active,
    /// slot 1 follows the layout setting beside it, slots 2 and 5 share explicit symbol and
    /// interval groups, slots 3 and 4 are in view group "B". Turning one switch on aligns its
    /// field at the next show: within the active chart's view group from the active chart,
    /// within "B" from its lowest slot for symbol and interval, while the explicit groups keep
    /// theirs and scrolling stays with the active chart's group. The follower keeps its undo
    /// history. Turning the switch off changes nothing, and a show that receives a supplied
    /// chart takes a switch turned on as found.
    #[test]
    fn turning_a_sync_switch_on_aligns_the_charts_that_follow_it() {
        type Switch = fn(&mut ChartSyncOptions) -> &mut bool;
        type Row = (&'static str, Switch, [usize; 6]);
        let rows: [Row; 3] = [
            ("symbol", |sync| &mut sync.sync_symbol, [0, 0, 2, 3, 3, 5]),
            (
                "interval",
                |sync| &mut sync.sync_timeframe,
                [0, 0, 2, 3, 3, 5],
            ),
            ("span", |sync| &mut sync.sync_time_axis, [0, 0, 0, 3, 4, 0]),
        ];
        let symbols = ["AAA", "BBB", "CCC", "DDD", "EEE", "FFF"];
        let intervals = [
            Timeframe::Min1,
            Timeframe::Min5,
            Timeframe::Min15,
            Timeframe::Hour1,
            Timeframe::Day1,
            Timeframe::Week1,
        ];
        let ctx = egui::Context::default();
        let show = |layout: &mut ChartLayout, charts: &mut [Chart; 6]| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
                    ..Default::default()
                },
                |ui| {
                    layout.show(ui, &mut charts.each_mut());
                },
            )
            .drop_without_applying_deltas();
        };
        let span = |chart: &Chart| {
            let range = chart.visible_time_range().unwrap();
            (range.from.timestamp_millis(), range.to.timestamp_millis())
        };
        type State = (String, Option<Timeframe>, (i64, i64));
        let state = |chart: &Chart| -> State {
            (chart.symbol().to_owned(), chart.timeframe(), span(chart))
        };
        // A synchronized fit lands within a microsecond of the source's edges, which whole
        // milliseconds can round apart by one.
        let same = |a: &State, b: &State, name: &str| match name {
            "symbol" => a.0 == b.0,
            "interval" => a.1 == b.1,
            _ => (a.2.0 - b.2.0).abs() <= 1 && (a.2.1 - b.2.1).abs() <= 1,
        };
        for (row, switch, sources) in rows {
            let mut layout = ChartLayout {
                mode: ChartLayoutMode::Six,
                ..Default::default()
            };
            layout.sync.sync_time_axis = false;
            layout.sync.sync_crosshair = false;
            for slot in [2, 5] {
                layout.set_link_groups(
                    slot,
                    ChartLinkGroups {
                        symbol: ChartLink::Group("🔶".into()),
                        interval: ChartLink::Group("💧".into()),
                    },
                    true,
                );
            }
            layout.set_sync_group(3, Some("B".into()));
            layout.set_sync_group(4, Some("B".into()));
            let mut charts = [
                chart(60),
                chart(60),
                chart(60),
                chart(60),
                chart(60),
                chart(60),
            ];
            for (slot, chart) in charts.iter_mut().enumerate() {
                chart.set_symbol(symbols[slot]);
                chart.set_timeframe(intervals[slot]);
                chart.set_visible_bars(30 + 10 * slot);
            }
            show(&mut layout, &mut charts);
            show(&mut layout, &mut charts);
            charts[1].user_action(|chart| chart.set_chart_type(crate::model::ChartType::Line));
            let before: Vec<_> = charts.iter().map(state).collect();
            *switch(&mut layout.sync) = true;
            show(&mut layout, &mut charts);
            let after: Vec<_> = charts.iter().map(state).collect();
            for (slot, source) in sources.into_iter().enumerate() {
                // An interval set on bars it does not match maps other times to the whitespace
                // past the last bar, so the interval row leaves the span to the span row.
                for name in ["symbol", "interval", "span"]
                    .into_iter()
                    .filter(|name| row != "interval" || *name != "span")
                {
                    let expected = if name == row {
                        &before[source]
                    } else {
                        &before[slot]
                    };
                    assert!(
                        same(&after[slot], expected, name),
                        "{row} on: {name} of slot {slot}: {:?}, expected {expected:?}",
                        after[slot]
                    );
                }
            }
            assert!(
                charts[1].can_undo(),
                "{row}: the follower keeps its history"
            );
            *switch(&mut layout.sync) = false;
            show(&mut layout, &mut charts);
            for (slot, chart) in charts.iter().enumerate() {
                let off = state(chart);
                for name in ["symbol", "interval", "span"] {
                    assert!(
                        same(&off, &after[slot], name),
                        "{row} off: {name} of slot {slot}"
                    );
                }
            }
            // Supplied in the show that sees the switch on, slot 1 keeps its own like a restore.
            charts[1] = chart(60);
            charts[1].set_symbol("ZZZ");
            charts[1].set_timeframe(Timeframe::Month1);
            charts[1].set_visible_bars(90);
            *switch(&mut layout.sync) = true;
            show(&mut layout, &mut charts);
            let supplied = state(&charts[1]);
            assert!(
                !same(&supplied, &after[0], row),
                "{row}: supplied chart {supplied:?}"
            );
        }
    }

    /// Alt+Enter shows the focused tile alone and a second press brings the preset back with the
    /// tile's slot, viewport and synchronization group unchanged. A clamped reselection keeps
    /// live tracking enabled.
    #[test]
    fn alt_enter_maximizes_and_restores_the_active_tile() {
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Four,
            ..Default::default()
        };
        let mut charts = [chart(60), chart(120), chart(60), chart(60)];
        layout.set_sync_group(2, Some("B".into()));
        layout.set_active(2);
        let mut time = 0.0;
        let mut frame =
            |layout: &mut ChartLayout, charts: &mut [Chart; 4], pressed: bool, host_first: bool| {
                let mut requests = Vec::new();
                time += 0.1;
                let events = if pressed {
                    vec![Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::ALT,
                    }]
                } else {
                    vec![]
                };
                let mut discard = pressed && host_first;
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        if discard {
                            ui.ctx().request_discard("layout shortcut sizing pass");
                            discard = false;
                        }
                        if host_first {
                            charts[2].dispatch_shortcuts(ui.ctx());
                            requests.extend(charts[2].take_shortcut_requests());
                        }
                        layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    },
                )
                .drop_without_applying_deltas();
                requests
            };
        for _ in 0..3 {
            frame(&mut layout, &mut charts, false, false);
        }
        let tiles = charts
            .each_ref()
            .map(|chart| chart.get_rendered_price_rect());
        let tile = tiles[2];
        let viewport = charts[2].get_time_scale_state();
        frame(&mut layout, &mut charts, true, false);
        // The chart's widget id follows its rect, so focus moves over to it on the next frames.
        frame(&mut layout, &mut charts, false, false);
        frame(&mut layout, &mut charts, false, false);
        assert_eq!((layout.maximized(), layout.active()), (Some(2), 2));
        assert!(charts[2].get_rendered_price_rect().width() > tile.width() * 1.8);
        assert_eq!(layout.sync_group(2), Some("B"));
        assert!(charts[2].take_shortcut_requests().is_empty());
        frame(&mut layout, &mut charts, true, false);
        frame(&mut layout, &mut charts, false, false);
        frame(&mut layout, &mut charts, false, false);
        assert_eq!((layout.maximized(), layout.active()), (None, 2));
        assert_eq!(charts[2].get_rendered_price_rect(), tile);
        assert_eq!(charts[2].get_time_scale_state(), viewport);
        for maximized in [Some(2), None] {
            let requests = frame(&mut layout, &mut charts, true, true);
            frame(&mut layout, &mut charts, false, true);
            frame(&mut layout, &mut charts, false, true);
            assert_eq!((layout.maximized(), layout.active()), (maximized, 2));
            assert!(
                requests.is_empty(),
                "layout keys reached the host: {requests:?}"
            );
        }
        layout.set_maximized(Some(2));
        frame(&mut layout, &mut charts, false, false);
        charts[2].enable_tracking_mode();
        layout.mode = ChartLayoutMode::ThreeHorizontal;
        frame(&mut layout, &mut charts, false, false);
        assert!(charts[2].is_tracking_mode_active());
        layout.set_active(5);
        frame(&mut layout, &mut charts, false, false);
        assert_eq!((layout.maximized(), layout.active()), (Some(2), 2));
        assert!(
            charts[2].is_tracking_mode_active(),
            "clamped reselection disabled live tracking"
        );
        layout.mode = ChartLayoutMode::Four;
        let other_group_spacing = charts[1].get_time_scale_state().0;
        layout.set_active(1);
        frame(&mut layout, &mut charts, false, false);
        frame(&mut layout, &mut charts, false, false);
        assert_eq!((layout.maximized(), layout.active()), (Some(1), 1));
        assert_eq!(charts[1].get_time_scale_state().0, other_group_spacing);
        assert!(charts[1].get_rendered_price_rect().width() > tiles[1].width() * 1.8);
        let range = charts[1].visible_time_range().unwrap();
        layout.set_active(5);
        frame(&mut layout, &mut charts, false, false);
        frame(&mut layout, &mut charts, false, false);
        assert_eq!((layout.maximized(), layout.active()), (Some(3), 3));
        let selected = charts[3].visible_time_range().unwrap();
        assert!(
            (selected.from - range.from).num_milliseconds().abs() <= 1
                && (selected.to - range.to).num_milliseconds().abs() <= 1,
            "clamped activation changed the UTC span from {range:?} to {selected:?}"
        );
        layout.mode = ChartLayoutMode::Tabs;
        frame(&mut layout, &mut charts, false, false);
        assert_eq!(layout.maximized(), None);
        layout.mode = ChartLayoutMode::Four;
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let grid = layout.visible_chart_rects(screen, 4);
        assert_eq!(grid.len(), 4);
        layout.set_maximized(Some(3));
        assert_eq!(layout.visible_chart_rects(screen, 4), vec![(3, screen)]);
        assert_eq!((layout.maximized(), layout.active()), (Some(3), 3));
        layout.set_maximized(Some(16));
        assert_eq!((layout.maximized(), layout.active()), (Some(3), 3));
        layout.set_maximized(None);
        assert_eq!(layout.visible_chart_rects(screen, 4), grid);
        assert_eq!((layout.maximized(), layout.active()), (None, 3));
        layout.set_maximized(Some(5));
        assert_eq!(layout.visible_chart_rects(screen, 4), grid);
        frame(&mut layout, &mut charts, false, false);
        assert_eq!((layout.maximized(), layout.active()), (None, 3));
    }

    /// Tab follows slot order, wraps, skips detached slots and transfers both maximization and
    /// canvas focus without changing the synchronized UTC span. A host draining first cannot
    /// take layout keys; a lone chart still yields them.
    #[test]
    fn tab_and_shift_tab_move_between_the_shown_charts() {
        type Row = (
            &'static str,
            ChartLayoutMode,
            usize,
            &'static [usize],
            Option<usize>,
            usize,
            bool,
            bool,
            (usize, Option<usize>),
        );
        use ChartLayoutMode::{Four, Single, Tabs};
        let rows: [Row; 14] = [
            ("next", Four, 4, &[], None, 1, true, false, (2, None)),
            (
                "wrap forward",
                Four,
                4,
                &[],
                None,
                3,
                true,
                false,
                (0, None),
            ),
            ("wrap back", Four, 4, &[], None, 0, false, false, (3, None)),
            (
                "skip detached",
                Four,
                4,
                &[2],
                None,
                1,
                true,
                false,
                (3, None),
            ),
            (
                "skip detached back",
                Four,
                4,
                &[2],
                None,
                3,
                false,
                false,
                (1, None),
            ),
            ("tab preset", Tabs, 4, &[], None, 3, true, false, (0, None)),
            (
                "maximized",
                Four,
                4,
                &[],
                Some(2),
                2,
                true,
                false,
                (3, Some(3)),
            ),
            (
                "maximized back",
                Four,
                4,
                &[],
                Some(0),
                0,
                false,
                false,
                (3, Some(3)),
            ),
            (
                "host drains first",
                Four,
                4,
                &[],
                None,
                1,
                true,
                true,
                (2, None),
            ),
            (
                "tabs host drains first",
                Tabs,
                4,
                &[],
                None,
                3,
                true,
                true,
                (0, None),
            ),
            (
                "others detached",
                Four,
                4,
                &[0, 1, 3],
                None,
                2,
                true,
                false,
                (2, None),
            ),
            (
                "shrink preset with queued Tab",
                Four,
                8,
                &[],
                Some(5),
                0,
                true,
                true,
                (1, None),
            ),
            ("one chart", Single, 2, &[], None, 0, true, false, (0, None)),
            (
                "one supplied chart",
                Four,
                1,
                &[],
                None,
                0,
                true,
                true,
                (0, None),
            ),
        ];
        for (
            discard_input,
            (row, mode, supplied, detached, maximized, start, forward, host_first, expected),
        ) in [false, true]
            .into_iter()
            .flat_map(|discard| rows.map(|row| (discard, row)))
        {
            let ctx = egui::Context::default();
            let mut layout = ChartLayout {
                mode: if maximized.is_some_and(|slot| slot >= mode.chart_count()) {
                    ChartLayoutMode::Eight
                } else {
                    mode
                },
                ..Default::default()
            };
            let mut charts: Vec<_> = (0..supplied).map(|_| chart(60)).collect();
            for slot in detached {
                layout.set_detached(*slot, true);
            }
            layout.set_active(start);
            let modifiers = if forward {
                egui::Modifiers::NONE
            } else {
                egui::Modifiers::SHIFT
            };
            let mut requests = Vec::new();
            let mut time = 0.0;
            let mut discarded = false;
            let mut frame = |layout: &mut ChartLayout, charts: &mut Vec<Chart>, pressed| {
                time += 0.1;
                let source = layout.active();
                let mut range = charts[source].visible_time_range();
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                        time: Some(time),
                        events: if pressed {
                            vec![
                                Event::ModifiersChanged(modifiers),
                                Event::Key {
                                    key: egui::Key::Tab,
                                    physical_key: None,
                                    pressed: true,
                                    repeat: false,
                                    modifiers,
                                },
                            ]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    },
                    |ui| {
                        if pressed && discard_input && !discarded {
                            ui.ctx().request_discard("layout shortcut sizing pass");
                            discarded = true;
                        }
                        if host_first {
                            let chart = &mut charts[layout.active()];
                            chart.dispatch_shortcuts(ui.ctx());
                            requests.extend(chart.take_shortcut_requests());
                        }
                        layout.show_with(
                            ui,
                            &mut charts.iter_mut().collect::<Vec<_>>(),
                            |ui, index, chart| {
                                let response = chart.show(ui);
                                if index == source {
                                    range = chart.visible_time_range();
                                }
                                response
                            },
                        );
                    },
                )
                .drop_without_applying_deltas();
                range
            };
            for _ in 0..3 {
                frame(&mut layout, &mut charts, false);
            }
            if let Some(slot) = maximized {
                layout.set_maximized(Some(slot));
                for _ in 0..3 {
                    frame(&mut layout, &mut charts, false);
                }
            }
            if layout.mode != mode {
                // Hosts keep the active chart when shrinking a preset by moving it to slot zero.
                let from = layout.active();
                charts.swap(from, start);
                layout.swap_slots(from, start);
                layout.set_active(start);
                layout.mode = mode;
            }
            let range = frame(&mut layout, &mut charts, true).unwrap();
            for _ in 0..2 {
                frame(&mut layout, &mut charts, false);
            }
            assert_eq!(
                (layout.active(), layout.maximized()),
                expected,
                "{row}, discard={discard_input}"
            );
            let count = supplied.min(mode.chart_count());
            if count > 1 {
                let selected = charts[layout.active()].visible_time_range().unwrap();
                assert!(
                    (selected.from - range.from).num_milliseconds().abs() <= 1
                        && (selected.to - range.to).num_milliseconds().abs() <= 1,
                    "{row}, discard={discard_input}: UTC span changed from {range:?} to {selected:?}"
                );
                let focused = ctx.memory(|memory| memory.focused()).and_then(|id| {
                    charts
                        .iter()
                        .position(|chart| chart.keyboard.chart_id == Some(id))
                });
                assert_eq!(
                    focused,
                    Some(expected.0),
                    "{row}, discard={discard_input}: canvas focus"
                );
            }
            for chart in &mut charts {
                requests.extend(chart.take_shortcut_requests());
            }
            if count > 1 {
                assert!(
                    requests.is_empty(),
                    "{row}: layout keys reached the host: {requests:?}"
                );
            } else {
                assert_eq!(
                    requests,
                    [ShortcutRequest::Action(KeyboardAction::CycleChartFocus)],
                    "{row}"
                );
            }
        }
    }

    /// A border drag changes only the charts beside it and leaves the dragged border under the
    /// pointer. Hand values: weights 1:4:4 give 160, 640 and 640 points of a 1440-point row. At 540
    /// points the first chart is held at its 80-point minimum and the others share 460 as 230 each.
    /// Widened again, a drag between the last two leaves the first its 160 points and divides the
    /// other 1280 by 250:210. A drag of the held chart's own border lets it follow the pointer, so
    /// all three then divide the row by their dragged lengths, 100:210:230.
    #[test]
    fn border_drag_keeps_the_other_charts_of_its_row_after_a_resize() {
        let narrow = Rect::from_min_size(pos2(10.0, 10.0), vec2(540.0, 720.0));
        let wide = Rect::from_min_size(pos2(10.0, 10.0), vec2(1440.0, 720.0));
        let last_two = ([80.0, 250.0, 210.0], [160.0, 695.652, 584.348]);
        let held_chart = ([100.0, 210.0, 230.0], [266.667, 560.0, 613.333]);
        for (mode, border, shift, (dragged, widened)) in [
            (ChartLayoutMode::ThreeHorizontal, 310.0, false, last_two),
            (ChartLayoutMode::ThreeHorizontal, 80.0, false, held_chart),
            (ChartLayoutMode::Six, 310.0, true, last_two),
            (ChartLayoutMode::Six, 80.0, true, held_chart),
        ] {
            let rows = usize::from(mode == ChartLayoutMode::Six) + 1;
            let mut layout = ChartLayout {
                mode,
                ..Default::default()
            };
            layout.sync.sync_time_axis = false;
            let mut sizes = layout.sizes(mode);
            for row in &mut sizes.weights[1..] {
                *row = vec![1.0, 4.0, 4.0];
            }
            layout.set_sizes(&[sizes], true).unwrap();
            let assert_widths = |layout: &ChartLayout, rect: Rect, expected: [f32; 3]| {
                let cells = layout.visible_chart_rects(rect, 3 * rows);
                let widths: Vec<_> = cells.iter().map(|(_, cell)| cell.width()).collect();
                assert_eq!(widths.len(), 3 * rows, "{mode:?}");
                for (width, expected) in widths.iter().zip(expected.iter().cycle()) {
                    assert!(
                        (width - expected).abs() < 0.01,
                        "{mode:?} border {border}: {widths:?}, expected {expected:?} in each row"
                    );
                }
            };
            let ctx = egui::Context::default();
            let mut charts: Vec<_> = (0..3 * rows).map(|_| chart(60)).collect();
            let mut time = 0.0;
            let mut frame = |layout: &mut ChartLayout, charts: &mut [Chart], events: Vec<Event>| {
                time += 0.1;
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(narrow.expand(10.0)),
                        time: Some(time),
                        events: std::iter::once(Event::ModifiersChanged(if shift {
                            egui::Modifiers::SHIFT
                        } else {
                            egui::Modifiers::NONE
                        }))
                        .chain(events)
                        .collect(),
                        ..Default::default()
                    },
                    |ui| {
                        ui.scope_builder(egui::UiBuilder::new().max_rect(narrow), |ui| {
                            layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                        });
                    },
                )
                .drop_without_applying_deltas();
            };
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            let start = pos2(narrow.left() + border, narrow.top() + 100.0);
            let end = start + vec2(20.0, 0.0);
            for _ in 0..2 {
                frame(&mut layout, &mut charts, vec![]);
            }
            frame(&mut layout, &mut charts, vec![Event::PointerMoved(start)]);
            frame(&mut layout, &mut charts, vec![button(start, true)]);
            for fraction in [0.1, 0.5, 1.0, 1.0] {
                frame(
                    &mut layout,
                    &mut charts,
                    vec![Event::PointerMoved(start.lerp(end, fraction))],
                );
            }
            frame(&mut layout, &mut charts, vec![button(end, false)]);
            frame(&mut layout, &mut charts, vec![]);
            assert_widths(&layout, narrow, dragged);
            assert_widths(&layout, wide, widened);
        }
    }

    /// Placing the charts of one preset, which the layout does on every frame, costs the same
    /// however many other presets have stored sizes.
    #[cfg(feature = "studies")]
    #[test]
    fn chart_placement_does_not_copy_other_presets_sizes() {
        use super::super::studies_data::tests::allocations;
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 720.0));
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Four,
            ..Default::default()
        };
        let cells = layout.visible_chart_rects(rect, 4);
        let (unedited, same) = allocations(|| layout.visible_chart_rects(rect, 4));
        assert_eq!(same, cells);
        let others: Vec<_> = ChartLayoutMode::all()
            .iter()
            .filter(|&&mode| mode != layout.mode)
            .filter_map(|&mode| {
                let mut sizes = layout.sizes(mode);
                sizes.weights.iter_mut().find(|split| split.len() > 1)?[0] *= 3.0;
                Some(sizes)
            })
            .collect();
        layout.set_sizes(&others, true).unwrap();
        assert_eq!(layout.saved_sizes().len(), others.len());
        let (edited, same) = allocations(|| layout.visible_chart_rects(rect, 4));
        assert_eq!(same, cells);
        assert_eq!(edited, unedited, "with {} other presets", others.len());
    }

    /// Naming a synchronization group decides which charts a field following the layout setting
    /// links to; it never changes a membership, so restoring works in either order.
    #[test]
    fn set_sync_group_keeps_individual_field_groups() {
        for memberships in [
            ChartLinkGroups {
                symbol: ChartLink::Group("🔶".into()),
                interval: ChartLink::Group("💧".into()),
            },
            ChartLinkGroups {
                symbol: ChartLink::Unlinked,
                interval: ChartLink::Layout,
            },
        ] {
            let mut layout = ChartLayout::default();
            layout.set_link_groups(0, memberships.clone(), true);
            layout.set_sync_group(0, Some("Desk".into()));
            assert_eq!(layout.sync_group(0), Some("Desk"));
            assert_eq!(layout.link_groups(0), Some(memberships));
        }
    }

    /// One frame and each text shape it painted: text, painted rect inside its clip, centre.
    fn frame_texts(
        ctx: &egui::Context,
        layout: &mut ChartLayout,
        charts: &mut [Chart],
        size: Vec2,
        time: f64,
        events: Vec<Event>,
    ) -> Vec<(String, Rect, Pos2)> {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| {
                layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
            },
        );
        let mut texts = Vec::new();
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape {
                texts.push((
                    text.galley.text().to_owned(),
                    text.galley
                        .rect
                        .translate(text.pos.to_vec2())
                        .intersect(shape.clip_rect),
                    text.pos + text.galley.size() / 2.0,
                ));
            }
        }
        output.drop_without_applying_deltas();
        texts
    }

    fn pointer_button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    fn key(key: egui::Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }
    }

    fn text_center(texts: &[(String, Rect, Pos2)], label: &str) -> Pos2 {
        texts
            .iter()
            .find(|(text, ..)| text == label)
            .map(|(.., center)| *center)
            .unwrap_or_else(|| {
                panic!(
                    "no {label:?} among {:?}",
                    texts.iter().map(|t| &t.0).collect::<Vec<_>>()
                )
            })
    }

    /// The chart menu names the chart's synchronization group, which decides its scrolling,
    /// crosshair, drawing and date range links, and leaves its field memberships alone.
    #[test]
    fn chart_menu_still_assigns_the_synchronization_group() {
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        let memberships = ChartLinkGroups {
            symbol: ChartLink::Group("🔶".into()),
            interval: ChartLink::Group("💧".into()),
        };
        layout.set_link_groups(0, memberships.clone(), true);
        let mut charts = [chart(60), chart(60)];
        let mut time = 0.0;
        let mut frame = |layout: &mut ChartLayout, charts: &mut [Chart; 2], events| {
            time += 0.1;
            frame_texts(&ctx, layout, charts, vec2(640.0, 900.0), time, events)
        };
        frame(&mut layout, &mut charts, vec![]);
        let corner = pos2(308.0, 12.0);
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(corner)]);
        frame(&mut layout, &mut charts, vec![pointer_button(corner, true)]);
        frame(
            &mut layout,
            &mut charts,
            vec![pointer_button(corner, false)],
        );
        frame(&mut layout, &mut charts, vec![]);
        let texts = frame(&mut layout, &mut charts, vec![]);
        let input = text_center(&texts, "Sync group") + vec2(0.0, 22.0);
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(input)]);
        frame(&mut layout, &mut charts, vec![pointer_button(input, true)]);
        frame(&mut layout, &mut charts, vec![pointer_button(input, false)]);
        frame(&mut layout, &mut charts, vec![Event::Text("Desk".into())]);
        assert_eq!(layout.sync_group(0), None, "typing joins no group");
        frame(&mut layout, &mut charts, vec![key(egui::Key::Enter)]);
        assert_eq!(layout.sync_group(0), Some("Desk"));
        assert_eq!(layout.link_groups(0).as_ref(), Some(&memberships));
        frame(&mut layout, &mut charts, vec![pointer_button(input, true)]);
        frame(&mut layout, &mut charts, vec![pointer_button(input, false)]);
        for _ in 0.."Desk".len() {
            frame(&mut layout, &mut charts, vec![key(egui::Key::Backspace)]);
        }
        frame(&mut layout, &mut charts, vec![key(egui::Key::Enter)]);
        assert_eq!(layout.sync_group(0), None, "clearing returns it to unnamed");
        assert_eq!(layout.link_groups(0), Some(memberships));
    }

    /// Naming a group by typing is one membership edit, however it ends: Enter and a click on
    /// another chart, which selects it, commit it as one undo step, Escape discards it. The menu
    /// of the second chart is edited too, so the click selects a chart before it.
    #[test]
    fn typing_a_group_name_is_one_undo_step() {
        let first = pos2(100.0, 500.0);
        let second = pos2(500.0, 500.0);
        for (slot, commit, committed) in [
            (0, vec![key(egui::Key::Enter)], true),
            (
                0,
                vec![pointer_button(second, true), pointer_button(second, false)],
                true,
            ),
            (
                1,
                vec![pointer_button(first, true), pointer_button(first, false)],
                true,
            ),
            (0, vec![key(egui::Key::Escape)], false),
        ] {
            let ctx = egui::Context::default();
            let mut layout = ChartLayout {
                mode: ChartLayoutMode::TwoHorizontal,
                text_sync_groups: true,
                ..Default::default()
            };
            let mut charts = [chart(60), chart(60)];
            let mut time = 0.0;
            let mut frame = |layout: &mut ChartLayout, charts: &mut [Chart; 2], events| {
                time += 0.1;
                frame_texts(&ctx, layout, charts, vec2(640.0, 900.0), time, events)
            };
            frame(&mut layout, &mut charts, vec![]);
            if slot == 1 {
                frame(&mut layout, &mut charts, vec![pointer_button(second, true)]);
                frame(
                    &mut layout,
                    &mut charts,
                    vec![pointer_button(second, false)],
                );
            }
            let corner = pos2(308.0 + 320.0 * slot as f32, 12.0);
            frame(&mut layout, &mut charts, vec![Event::PointerMoved(corner)]);
            frame(&mut layout, &mut charts, vec![pointer_button(corner, true)]);
            frame(
                &mut layout,
                &mut charts,
                vec![pointer_button(corner, false)],
            );
            frame(&mut layout, &mut charts, vec![]);
            let texts = frame(&mut layout, &mut charts, vec![]);
            let picker = text_center(&texts, "Symbol group  ○");
            frame(&mut layout, &mut charts, vec![Event::PointerMoved(picker)]);
            frame(&mut layout, &mut charts, vec![pointer_button(picker, true)]);
            frame(
                &mut layout,
                &mut charts,
                vec![pointer_button(picker, false)],
            );
            frame(&mut layout, &mut charts, vec![]);
            let texts = frame(&mut layout, &mut charts, vec![]);
            let unlinked = text_center(&texts, "Unlinked");
            let follow = text_center(&texts, "Follow layout setting");
            // The name field sits between the two rows, each one separator away.
            let field = pos2(unlinked.x, (unlinked.y + follow.y) / 2.0);
            frame(&mut layout, &mut charts, vec![Event::PointerMoved(field)]);
            frame(&mut layout, &mut charts, vec![pointer_button(field, true)]);
            frame(&mut layout, &mut charts, vec![pointer_button(field, false)]);
            for letter in ["D", "e", "s", "k"] {
                frame(&mut layout, &mut charts, vec![Event::Text(letter.into())]);
                assert_eq!(
                    layout.link_groups(slot).unwrap().symbol,
                    ChartLink::Layout,
                    "typing joins no group"
                );
            }
            frame(&mut layout, &mut charts, commit);
            frame(&mut layout, &mut charts, vec![]);
            frame(&mut layout, &mut charts, vec![]);
            let symbol = |layout: &ChartLayout| layout.link_groups(slot).unwrap().symbol;
            if committed {
                assert_eq!(symbol(&layout), ChartLink::Group("Desk".into()));
                let active = layout.active();
                assert!(charts[active].undo());
                layout.apply_history_steps(&mut charts.each_mut());
                assert_eq!(
                    symbol(&layout),
                    ChartLink::Layout,
                    "one name, one undo step"
                );
                assert!(charts[active].redo());
                layout.apply_history_steps(&mut charts.each_mut());
                assert_eq!(symbol(&layout), ChartLink::Group("Desk".into()));
            } else {
                assert_eq!(symbol(&layout), ChartLink::Layout);
                assert!(!charts[slot].can_undo(), "a discarded name records nothing");
            }
        }
    }

    /// Of a tab layout only the shown chart draws its menu. A click on another tab must still
    /// commit a group name typed there, as a click on another chart of a grid does.
    #[test]
    fn clicking_another_tab_commits_a_typed_group_name() {
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Tabs,
            ..Default::default()
        };
        let mut charts = [chart(60), chart(60)];
        charts[1].set_symbol("OTHER");
        let mut time = 0.0;
        let mut frame = |layout: &mut ChartLayout, charts: &mut [Chart; 2], events| {
            time += 0.1;
            frame_texts(&ctx, layout, charts, vec2(640.0, 900.0), time, events)
        };
        frame(&mut layout, &mut charts, vec![]);
        let texts = frame(
            &mut layout,
            &mut charts,
            vec![Event::PointerMoved(pos2(300.0, 400.0))],
        );
        let corner = text_center(&texts, "…");
        let tab = text_center(&texts, "OTHER");
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(corner)]);
        frame(&mut layout, &mut charts, vec![pointer_button(corner, true)]);
        frame(
            &mut layout,
            &mut charts,
            vec![pointer_button(corner, false)],
        );
        frame(&mut layout, &mut charts, vec![]);
        let texts = frame(&mut layout, &mut charts, vec![]);
        let input = text_center(&texts, "Sync group") + vec2(0.0, 22.0);
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(input)]);
        frame(&mut layout, &mut charts, vec![pointer_button(input, true)]);
        frame(&mut layout, &mut charts, vec![pointer_button(input, false)]);
        frame(&mut layout, &mut charts, vec![Event::Text("Desk".into())]);
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(tab)]);
        frame(&mut layout, &mut charts, vec![pointer_button(tab, true)]);
        frame(&mut layout, &mut charts, vec![pointer_button(tab, false)]);
        frame(&mut layout, &mut charts, vec![]);
        assert_eq!(layout.active(), 1);
        assert_eq!(layout.sync_group(0), Some("Desk"));
    }

    /// Ctrl+Z reaches the active chart while the layout renders it, as in a host that leaves
    /// shortcut dispatch to `show`. The undone symbol or interval must reach its linked chart
    /// as the edit did.
    #[test]
    fn keyboard_undo_of_a_linked_change_reaches_the_group() {
        let command = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let state = |chart: &Chart| (chart.symbol().to_owned(), chart.timeframe());
        let original = ("SAMPLE".to_owned(), Some(Timeframe::Min1));
        let mut received = Vec::new();
        for interval in [false, true] {
            let ctx = egui::Context::default();
            let mut layout = ChartLayout {
                mode: ChartLayoutMode::TwoHorizontal,
                ..Default::default()
            };
            let link = ChartLinkGroups {
                symbol: ChartLink::Group("🔶".into()),
                interval: ChartLink::Group("💧".into()),
            };
            layout.set_link_groups(0, link.clone(), true);
            layout.set_link_groups(1, link, true);
            let mut charts = [chart(60), chart(60)];
            let mut time = 0.0;
            let mut frame = |layout: &mut ChartLayout, charts: &mut [Chart; 2], events| {
                time += 0.1;
                frame_texts(&ctx, layout, charts, vec2(1440.0, 900.0), time, events);
            };
            frame(&mut layout, &mut charts, vec![]);
            // A toolbar edit between frames reaches the linked chart.
            charts[0].user_action(|chart| {
                if interval {
                    chart.set_timeframe(Timeframe::Hour1);
                } else {
                    chart.set_symbol("OTHER");
                }
            });
            frame(&mut layout, &mut charts, vec![]);
            let edited = if interval {
                ("SAMPLE".to_owned(), Some(Timeframe::Hour1))
            } else {
                ("OTHER".to_owned(), Some(Timeframe::Min1))
            };
            assert_eq!(
                state(&charts[1]),
                edited,
                "the edit reaches the linked chart"
            );
            frame(
                &mut layout,
                &mut charts,
                vec![
                    Event::ModifiersChanged(command),
                    Event::Key {
                        key: egui::Key::Z,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: command,
                    },
                ],
            );
            assert_eq!(
                state(&charts[0]),
                original,
                "keyboard undo restores the active chart"
            );
            frame(
                &mut layout,
                &mut charts,
                vec![Event::ModifiersChanged(egui::Modifiers::NONE)],
            );
            frame(&mut layout, &mut charts, vec![]);
            received.push(state(&charts[1]));
        }
        assert_eq!(
            received,
            [original.clone(), original],
            "the undone symbol, then the undone interval, reaches the linked chart"
        );
    }

    /// Keys reach the active chart however it became active: a letter typed after a wheel turn
    /// over the second chart while a click left the first focused, and the keys after Escape,
    /// which leaves no chart focused.
    #[test]
    fn chart_keys_reach_the_active_chart() {
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        let mut charts = [chart(60), chart(60)];
        let mut time = 0.0;
        let mut frame = |layout: &mut ChartLayout, charts: &mut [Chart; 2], events| {
            time += 0.1;
            frame_texts(&ctx, layout, charts, vec2(1440.0, 900.0), time, events);
        };
        let pressed = |key, modifiers| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let command = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        frame(&mut layout, &mut charts, vec![]);
        for chart in &mut charts {
            chart.user_action(|chart| chart.set_chart_type(crate::model::ChartType::Line));
        }
        let (left, right) = (pos2(300.0, 400.0), pos2(1000.0, 400.0));
        frame(&mut layout, &mut charts, vec![Event::PointerMoved(left)]);
        frame(&mut layout, &mut charts, vec![pointer_button(left, true)]);
        frame(&mut layout, &mut charts, vec![pointer_button(left, false)]);
        frame(
            &mut layout,
            &mut charts,
            vec![
                Event::PointerMoved(right),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, -40.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                    source: egui::MouseWheelSource::Wheel,
                },
            ],
        );
        assert_eq!(layout.active(), 1);
        frame(&mut layout, &mut charts, vec![Event::Text("B".into())]);
        frame(&mut layout, &mut charts, vec![key(egui::Key::Escape)]);
        frame(
            &mut layout,
            &mut charts,
            vec![
                Event::ModifiersChanged(command),
                pressed(egui::Key::Z, command),
            ],
        );
        frame(
            &mut layout,
            &mut charts,
            vec![
                Event::ModifiersChanged(egui::Modifiers::NONE),
                pressed(egui::Key::L, egui::Modifiers::ALT),
            ],
        );
        frame(&mut layout, &mut charts, vec![Event::Text("A".into())]);
        let state = |chart: &mut Chart| {
            (
                chart.chart_type(),
                chart.state.price_scale().options().mode,
                chart.take_shortcut_requests(),
            )
        };
        use crate::scales::PriceScaleMode;
        let line = crate::model::ChartType::Line;
        assert_eq!(
            charts.each_mut().map(state),
            [
                (line, PriceScaleMode::Normal, vec![]),
                (
                    crate::model::ChartType::Candles,
                    PriceScaleMode::Logarithmic,
                    vec![
                        ShortcutRequest::SymbolSearch("B".into()),
                        ShortcutRequest::SymbolSearch("A".into())
                    ]
                ),
            ]
        );
    }

    /// The map set on the layout reaches every chart it shows and decides what a key does in the
    /// active one, also in a chart first supplied after the map was set.
    #[test]
    fn a_layout_map_drives_every_chart_and_charts_supplied_later() {
        let alt_o = egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::O);
        let mut map = ShortcutMap::default();
        for (key, action) in &mut map.bindings {
            if *action == KeyboardAction::OpenLayout {
                *key = alt_o;
            }
        }
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        layout.set_shortcuts(Some(map.clone()));
        let mut charts = vec![chart(60), chart(60)];
        let mut time = 0.0;
        let mut frame = |layout: &mut ChartLayout, charts: &mut Vec<Chart>, events| {
            time += 0.1;
            frame_texts(&ctx, layout, charts, vec2(1440.0, 900.0), time, events);
        };
        let press_alt_o = Event::Key {
            key: egui::Key::O,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        };
        let open = || vec![ShortcutRequest::Action(KeyboardAction::OpenLayout)];
        frame(&mut layout, &mut charts, vec![]);
        assert!(charts.iter().all(|chart| *chart.shortcuts() == map));
        for index in 0..2 {
            layout.set_active(index);
            frame(&mut layout, &mut charts, vec![]);
            frame(&mut layout, &mut charts, vec![press_alt_o.clone()]);
            let requests: Vec<_> = charts
                .iter_mut()
                .map(Chart::take_shortcut_requests)
                .collect();
            let mut expected = vec![vec![], vec![]];
            expected[index] = open();
            assert_eq!(requests, expected, "active chart {index}");
        }
        layout.mode = ChartLayoutMode::ThreeHorizontal;
        charts.push(chart(60));
        layout.set_active(2);
        frame(&mut layout, &mut charts, vec![]);
        assert_eq!(*charts[2].shortcuts(), map);
        frame(&mut layout, &mut charts, vec![press_alt_o]);
        assert_eq!(charts[2].take_shortcut_requests(), open());
        frame(&mut layout, &mut charts, vec![key(egui::Key::Period)]);
        assert!(charts[2].take_shortcut_requests().is_empty());
    }

    /// A host exchanges two charts with `swap_slots`, as the demo does when a smaller preset
    /// would hide the active chart, between a group change of the moved chart and its undo. The
    /// undo and the redo reach the chart in its new slot and leave the chart now in its old slot
    /// alone, whether the change was recorded before the exchange or at the next frame.
    #[test]
    fn link_undo_follows_a_chart_moved_by_swap_slots() {
        let show = |layout: &mut ChartLayout, charts: &mut [Chart; 4]| {
            egui::Context::default()
                .run_ui(RawInput::default(), |ui| {
                    layout.show(ui, &mut charts.each_mut());
                })
                .drop_without_applying_deltas();
        };
        let group = |name: &str| ChartLinkGroups {
            symbol: ChartLink::Group(name.to_owned()),
            ..Default::default()
        };
        for recorded in [true, false] {
            let mut charts = [chart(60), chart(60), chart(60), chart(60)];
            let mut layout = ChartLayout {
                mode: ChartLayoutMode::Four,
                ..Default::default()
            };
            layout.set_link_groups(0, group("B"), true);
            layout.set_active(3);
            show(&mut layout, &mut charts);
            layout.set_link_groups(3, group("A"), false);
            if recorded {
                show(&mut layout, &mut charts);
            }
            charts.swap(0, 3);
            layout.swap_slots(0, 3);
            layout.set_active(0);
            if !recorded {
                show(&mut layout, &mut charts);
            }
            let mut seen = Vec::new();
            for undo in [true, false] {
                assert!(if undo {
                    charts[0].undo()
                } else {
                    charts[0].redo()
                });
                show(&mut layout, &mut charts);
                seen.push((layout.link_groups(0), layout.link_groups(3)));
            }
            assert_eq!(
                seen,
                [
                    (Some(ChartLinkGroups::default()), Some(group("B"))),
                    (Some(group("A")), Some(group("B"))),
                ],
                "recorded before the exchange: {recorded}"
            );
        }
    }

    /// Four charts, the second and the fourth in one symbol group. The fourth keeps its own
    /// undo step through the symbol the active second sends it; a host change outside a user
    /// action clears the history of both, as it clears the source's.
    #[test]
    fn a_linked_symbol_change_keeps_the_history_of_the_chart_it_reaches() {
        let show = |layout: &mut ChartLayout, charts: &mut [Chart; 4]| {
            egui::Context::default()
                .run_ui(RawInput::default(), |ui| {
                    layout.show(ui, &mut charts.each_mut());
                })
                .drop_without_applying_deltas();
        };
        let mut charts = [chart(60), chart(60), chart(60), chart(60)];
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Four,
            ..Default::default()
        };
        let group = ChartLinkGroups {
            symbol: ChartLink::Group("🔶".into()),
            ..Default::default()
        };
        layout.set_link_groups(1, group.clone(), true);
        layout.set_link_groups(3, group, true);
        layout.set_active(1);
        show(&mut layout, &mut charts);
        charts[3].user_action(|chart| chart.set_chart_type(crate::model::ChartType::Line));
        charts[1].user_action(|chart| chart.set_symbol("OTHER"));
        show(&mut layout, &mut charts);
        assert_eq!(charts[3].symbol(), "OTHER");
        assert!(charts[3].undo(), "the linked chart keeps its step");
        show(&mut layout, &mut charts);
        assert_eq!(
            (
                charts[1].symbol(),
                charts[3].symbol(),
                charts[3].chart_type()
            ),
            ("OTHER", "OTHER", crate::model::ChartType::Candles)
        );
        charts[1].set_symbol("HOST");
        show(&mut layout, &mut charts);
        assert_eq!(charts[3].symbol(), "HOST");
        assert!(!charts[3].can_undo() && !charts[3].can_redo());
    }

    /// The third and fourth of four charts share a symbol and an interval group while the first
    /// is active. A symbol the host sets on the third and an interval it sets on the fourth reach
    /// the other; the second, joining both groups, then takes them.
    #[test]
    fn joining_a_group_adopts_its_symbol_and_interval() {
        let show = |layout: &mut ChartLayout, charts: &mut [Chart; 4]| {
            egui::Context::default()
                .run_ui(RawInput::default(), |ui| {
                    layout.show(ui, &mut charts.each_mut());
                })
                .drop_without_applying_deltas();
        };
        let mut charts = [chart(60), chart(60), chart(60), chart(60)];
        for (slot, chart) in charts.iter_mut().enumerate() {
            chart.set_symbol(format!("SLOT_{slot}"));
        }
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Four,
            ..Default::default()
        };
        let link = ChartLinkGroups {
            symbol: ChartLink::Group("🔶".into()),
            interval: ChartLink::Group("💧".into()),
        };
        layout.set_link_groups(2, link.clone(), true);
        layout.set_link_groups(3, link.clone(), true);
        show(&mut layout, &mut charts);
        charts[2].set_symbol("HOST");
        charts[3].set_timeframe(Timeframe::Hour1);
        show(&mut layout, &mut charts);
        let state = |chart: &Chart| (chart.symbol().to_owned(), chart.timeframe());
        let group = ("HOST".to_owned(), Some(Timeframe::Hour1));
        assert_eq!(
            [state(&charts[2]), state(&charts[3])],
            [group.clone(), group.clone()],
            "a change in a member that is not active"
        );
        layout.set_link_groups(1, link, false);
        show(&mut layout, &mut charts);
        assert_eq!(state(&charts[1]), group, "the joining chart");
        assert_eq!(
            state(&charts[0]),
            ("SLOT_0".to_owned(), Some(Timeframe::Min1))
        );
    }

    /// A host opens a saved layout by supplying its charts in the same slots or restoring
    /// snapshots into them. Nobody changed a symbol or interval, so each opened chart keeps its
    /// own, also in slots that share a symbol and an interval group; the next change is sent.
    #[test]
    fn charts_opened_into_linked_slots_keep_their_symbols() {
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        let link = ChartLinkGroups {
            symbol: ChartLink::Group("🔶".into()),
            interval: ChartLink::Group("💧".into()),
        };
        layout.set_link_groups(0, link.clone(), true);
        layout.set_link_groups(1, link, true);
        let mut charts = vec![chart(60), chart(60)];
        let mut time = 0.0;
        let mut frames = |layout: &mut ChartLayout, charts: &mut Vec<Chart>| {
            for _ in 0..2 {
                time += 0.1;
                frame_texts(&ctx, layout, charts, vec2(1440.0, 900.0), time, vec![]);
            }
        };
        let state = |charts: &[Chart]| {
            charts
                .iter()
                .map(|chart| (chart.symbol().to_owned(), chart.timeframe()))
                .collect::<Vec<_>>()
        };
        let opened = [
            ("AAPL".to_owned(), Some(Timeframe::Hour1)),
            ("MSFT".to_owned(), Some(Timeframe::Day1)),
        ];
        frames(&mut layout, &mut charts);
        // Assigned in place, so a new chart can sit at the address of the one it replaces.
        charts[0] = chart(3600);
        charts[0].set_symbol("AAPL");
        charts[0].set_timeframe(Timeframe::Hour1);
        charts[1] = chart(86_400);
        charts[1].set_symbol("MSFT");
        charts[1].set_timeframe(Timeframe::Day1);
        frames(&mut layout, &mut charts);
        assert_eq!(state(&charts), opened, "supplied charts");

        #[cfg(feature = "ui")]
        let saved = charts[0].snapshot().unwrap();
        charts[0].set_symbol("TSLA");
        frames(&mut layout, &mut charts);
        assert_eq!(charts[1].symbol(), "TSLA", "the next change is sent");
        #[cfg(feature = "ui")]
        {
            charts[1].set_symbol("MSFT");
            frames(&mut layout, &mut charts);
            charts[0].restore(&saved).unwrap();
            frames(&mut layout, &mut charts);
            assert_eq!(state(&charts), opened, "a restored chart");
        }
    }

    /// Four charts on a 1536 x 864 screen are 768 points wide. Beside the 70 point price axis
    /// the status line keeps 768 - 70 - 48 = 650 points, over the 600 that show open, high, low
    /// and close. The selectors cover x 548..658 of the first row only, so the values still fit,
    /// and nothing of the status line may paint beneath the selectors: no text, and no outline of
    /// a hovered title or study row that starts on the first row and continues on the next.
    #[test]
    fn link_selectors_keep_open_high_low_in_the_status_line() {
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Four,
            ..Default::default()
        };
        let mut charts = [chart(60), chart(60), chart(60), chart(60)];
        let size = vec2(1536.0, 864.0);
        let mut texts = Vec::new();
        for frame in 0..3 {
            texts = frame_texts(
                &ctx,
                &mut layout,
                &mut charts,
                size,
                frame as f64 * 0.1,
                vec![],
            );
        }
        let slot = layout.rects(Rect::from_min_size(Pos2::ZERO, size), 4)[0];
        let price = charts[0].get_rendered_price_rect();
        assert!(price.width() - 48.0 >= 600.0, "{slot:?} {price:?}");
        let own: Vec<_> = texts
            .iter()
            .filter(|(_, rect, _)| rect.is_positive() && slot.contains_rect(*rect))
            .collect();
        let selectors = Rect::from_min_max(
            pos2(slot.right() - 220.0, slot.top() + 2.0),
            pos2(slot.right() - 110.0, slot.top() + 24.0),
        );
        for (text, rect, _) in &own {
            if !text.starts_with("S ") && !text.starts_with("I ") {
                assert!(
                    !rect.intersect(selectors).is_positive(),
                    "{text:?} paints beneath the selectors"
                );
            }
        }
        let values: Vec<_> = own
            .iter()
            .map(|(text, ..)| text.as_str())
            .filter(|text| {
                ["O ", "H ", "L ", "C "]
                    .iter()
                    .any(|field| text.starts_with(field))
            })
            .collect();
        let fields: Vec<_> = values.iter().map(|text| &text[..2]).collect();
        assert_eq!(
            fields,
            ["O ", "H ", "L ", "C "],
            "status line values {values:?}"
        );
        // A long title, then a study row with nothing before it, continues on the next row, and
        // its outline spans both rows when hovered.
        let accent = ctx.active_theme().ui.accent;
        let mut time = 0.2;
        let mut hover = |layout: &mut ChartLayout, charts: &mut [Chart; 4], row: &str| {
            let mut outlines = Vec::new();
            for _ in 0..3 {
                time += 0.1;
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        time: Some(time),
                        events: vec![Event::PointerMoved(pos2(100.0, 14.0))],
                        ..Default::default()
                    },
                    |ui| {
                        layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    },
                );
                outlines = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect)
                            if rect.stroke.width == 1.0 && rect.stroke.color == accent =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .collect();
                output.drop_without_applying_deltas();
            }
            assert!(!outlines.is_empty(), "hovering the {row} outlines it");
            for outline in outlines {
                assert!(
                    !outline.intersects(selectors),
                    "the {row} outline {outline:?} reaches beneath the selectors"
                );
            }
        };
        #[cfg(feature = "icons")]
        {
            charts[0].set_symbol("LONG_SYMBOL_".repeat(12));
            hover(&mut layout, &mut charts, "title");
        }
        let config = charts[0].config_mut();
        config.show_symbol_labels = false;
        config.show_ohlc_info = false;
        config.show_ohlc_change = false;
        config.show_ohlc_change_percent = false;
        config.show_legend_volume = false;
        charts[0].add_indicator(Box::new(
            crate::studies::CustomIndicator::new("LONG_STUDY_".repeat(4), |_| Vec::new())
                .with_line_count(3)
                .with_line_names(vec![
                    "Ask levels".into(),
                    "Bid levels".into(),
                    "Mid levels".into(),
                ]),
        ));
        hover(&mut layout, &mut charts, "study row");
    }

    /// In a linked slot the title controls end where the first row before the selectors ends, 228
    /// points left of the slot's right edge, so their flag button stays inside the status line,
    /// which starts 8 points right of the plot's left edge, only from 8 + 116 + 228 = 352 points
    /// of plot left to slot right. Narrower slots leave the selectors to the corner menu, the
    /// slot of exactly 352 keeps them, and a left price axis takes its width from those 352.
    /// Narrower slots show their group markers instead, and the hovered title shows only the
    /// buttons that fit before the legend's end, which is 126 points left of the slot's right
    /// edge with a 70 point right axis: none at 120, the flag at 160, flag and menu at 200, and
    /// all but the remove button with a left axis at 300.
    #[cfg(feature = "icons")]
    #[test]
    fn title_controls_fit_in_narrow_linked_slots() {
        for (target, left_axis, shows_selectors, buttons) in [
            (120.0_f32, false, false, 0),
            (145.0, false, false, 0),
            (160.0, false, false, 1),
            (200.0, false, false, 2),
            (320.0, false, false, 5),
            (336.0, false, false, 5),
            (351.0, false, false, 5),
            (352.0, false, true, 5),
            (300.0, true, false, 4),
            (352.0, true, false, 5),
            (411.0, true, false, 5),
            (430.0, true, false, 5),
            (480.0, true, true, 5),
        ] {
            let ctx = egui::Context::default();
            let mut layout = ChartLayout {
                mode: ChartLayoutMode::TwoHorizontal,
                ..Default::default()
            };
            let marker_colors = [LINK_MARKERS[0].1, LINK_MARKERS[2].1];
            layout.set_link_groups(
                0,
                ChartLinkGroups {
                    symbol: ChartLink::Group(LINK_MARKERS[0].0.into()),
                    interval: ChartLink::Group(LINK_MARKERS[2].0.into()),
                },
                true,
            );
            let mut charts = [chart(60), chart(60)];
            for chart in &mut charts {
                chart.config_mut().show_left_axis = left_axis;
                chart.config_mut().show_right_axis = !left_axis;
            }
            let mut size = vec2(2.0 * target, 600.0);
            let slot = layout.rects(Rect::from_min_size(Pos2::ZERO, size), 2)[0];
            size.x += 2.0 * (target - slot.width());
            let slot = layout.rects(Rect::from_min_size(Pos2::ZERO, size), 2)[0];
            assert_eq!(slot.width(), target, "{size:?}");
            let (mut icons, mut markers, mut load_errors) = (Vec::new(), Vec::new(), 0);
            let mut selectors = false;
            for frame in 0..5 {
                let plot = charts[0].last_rendered_price_rect;
                let plot_left = if plot.is_positive() {
                    plot.left()
                } else {
                    slot.left()
                };
                // On the symbol mark, inside the title at every width.
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        time: Some(frame as f64 * 0.1),
                        events: vec![Event::PointerMoved(pos2(
                            plot_left + 12.0,
                            slot.top() + 14.0,
                        ))],
                        ..Default::default()
                    },
                    |ui| {
                        layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    },
                );
                (icons, markers, load_errors) = (Vec::new(), Vec::new(), 0);
                selectors = false;
                for shape in &output.shapes {
                    match &shape.shape {
                        egui::Shape::Rect(rect)
                            if rect.brush.as_ref().is_some_and(|brush| {
                                brush.fill_texture_id != egui::TextureId::default()
                            }) && slot.intersects(rect.rect)
                                && rect.rect.bottom() < slot.center().y =>
                        {
                            icons.push(rect.rect);
                        }
                        egui::Shape::Path(path)
                            if marker_colors.contains(&path.fill)
                                && slot.intersects(path.visual_bounding_rect()) =>
                        {
                            markers.push(path.visual_bounding_rect());
                        }
                        // egui's mark for an image it cannot load at the size it is given.
                        egui::Shape::Text(text) if text.galley.text().starts_with('\u{26a0}') => {
                            load_errors += 1;
                        }
                        // The compact symbol selector's caption beside its painted marker.
                        egui::Shape::Text(text) if text.galley.text().ends_with(" S") => {
                            selectors |= slot.contains(text.pos);
                        }
                        _ => {}
                    }
                }
                output.drop_without_applying_deltas();
            }
            let plot = charts[0].last_rendered_price_rect;
            assert_eq!(
                load_errors, 0,
                "slot {target}, left axis {left_axis}: a squeezed button"
            );
            assert_eq!(
                icons.len(),
                buttons,
                "slot {target}, left axis {left_axis}: buttons {icons:?}"
            );
            for icon in &icons {
                assert!(
                    icon.width() >= 14.0
                        && icon.left() >= plot.left() + 8.0
                        && slot.contains_rect(*icon),
                    "slot {target}, left axis {left_axis}: button {icon:?} leaves the status line"
                );
                for marker in &markers {
                    assert!(
                        icon.right() < marker.left(),
                        "slot {target}, left axis {left_axis}: button {icon:?} reaches marker {marker:?}"
                    );
                }
            }
            assert_eq!(
                selectors, shows_selectors,
                "slot {target}, left axis {left_axis}"
            );
            assert_eq!(markers.len(), 2, "slot {target}, left axis {left_axis}");
            if shows_selectors {
                for icon in &icons {
                    assert!(
                        icon.right() <= slot.right() - LINK_CONTROLS_INSET,
                        "slot {target}: button {icon:?} reaches the selectors"
                    );
                }
            }
        }
    }

    /// Four charts in two by two 320 point slots, too narrow for the selectors. Each grouped
    /// chart, active or not, paints the markers of its groups past its status line and axis
    /// labels, and an ungrouped chart paints none.
    #[test]
    fn narrow_slots_show_their_group_markers() {
        let ctx = egui::Context::default();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Four,
            ..Default::default()
        };
        let symbol = ChartLink::Group("🔶".into());
        let interval = ChartLink::Group("💧".into());
        layout.set_link_groups(
            0,
            ChartLinkGroups {
                symbol: symbol.clone(),
                interval: interval.clone(),
            },
            true,
        );
        layout.set_link_groups(
            1,
            ChartLinkGroups {
                interval,
                ..Default::default()
            },
            true,
        );
        layout.set_link_groups(
            2,
            ChartLinkGroups {
                symbol,
                interval: ChartLink::Unlinked,
            },
            true,
        );
        let mut charts = [chart(60), chart(60), chart(60), chart(60)];
        let size = vec2(640.0, 600.0);
        let slots = layout.rects(Rect::from_min_size(Pos2::ZERO, size), 4);
        let colors = [LINK_MARKERS[0].1, LINK_MARKERS[2].1];
        let mut markers = vec![Vec::new(); 4];
        for frame in 0..3 {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    time: Some(frame as f64 * 0.1),
                    ..Default::default()
                },
                |ui| {
                    layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                },
            );
            markers = vec![Vec::new(); 4];
            let mut texts = Vec::new();
            for shape in &output.shapes {
                match &shape.shape {
                    egui::Shape::Path(path) if colors.contains(&path.fill) => {
                        let rect = path.visual_bounding_rect();
                        let slot = slots.iter().position(|slot| slot.contains_rect(rect));
                        markers[slot.expect("a marker inside a slot")].push((path.fill, rect));
                    }
                    egui::Shape::Text(text) => texts.push(
                        text.galley
                            .rect
                            .translate(text.pos.to_vec2())
                            .intersect(shape.clip_rect),
                    ),
                    _ => {}
                }
            }
            output.drop_without_applying_deltas();
            for (marker, text) in markers.iter().flatten().flat_map(|marker| {
                texts
                    .iter()
                    .filter(|text| text.is_positive())
                    .map(move |text| (marker, text))
            }) {
                assert!(
                    !marker.1.intersects(*text),
                    "frame {frame}: marker {marker:?} covers text at {text:?}"
                );
            }
        }
        let fills: Vec<Vec<_>> = markers
            .iter()
            .map(|slot| slot.iter().map(|(fill, _)| *fill).collect())
            .collect();
        assert_eq!(
            fills,
            [
                vec![colors[0], colors[1]],
                vec![colors[1]],
                vec![colors[0]],
                vec![]
            ]
        );
    }
}
