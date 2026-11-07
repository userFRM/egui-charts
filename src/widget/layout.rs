//! Tiled and tabbed charts with independent state and timestamp-based synchronization.

use super::keyboard::ShortcutRequest;
use super::{Chart, VisibleTimeRange};
use crate::config::KeyboardAction;
use crate::localization as l10n;
use crate::model::Timeframe;
use crate::theme::ThemeExt;
use egui::{Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::sync::{Arc, Mutex};

/// Narrowest chart that shows the field selectors in its header. The title controls need 8 points
/// of status line inset, 116 of their own and 228 from the chart's right edge to the end of the
/// first row; a narrower chart leaves the selectors to its corner menu.
const LINK_CONTROLS_MIN_WIDTH: f32 = 352.0;
/// Distance of the selectors from the chart's right edge, which ends the status line's first row.
pub(super) const LINK_CONTROLS_INSET: f32 = 220.0;

const LINK_MARKERS: [(&str, egui::Color32); 5] = [
    ("🔶", egui::Color32::from_rgb(240, 150, 0)),
    ("🍀", egui::Color32::from_rgb(110, 160, 70)),
    ("💧", egui::Color32::from_rgb(70, 170, 225)),
    ("❤️", egui::Color32::from_rgb(225, 65, 80)),
    ("💩", egui::Color32::from_rgb(160, 105, 90)),
];

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
            return 16;
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
pub struct ChartLayoutSizing {
    /// The arrangement these weights belong to.
    pub mode: ChartLayoutMode,
    /// Weights of each split; their sum must be finite and need not be one.
    pub weights: Vec<Vec<f32>>,
}

#[derive(Default)]
struct SizingState {
    sizes: Arc<Mutex<Vec<ChartLayoutSizing>>>,
    pending: Vec<SizeChange>,
    drag: Option<DividerDrag>,
    selected: Option<(usize, usize)>,
}

impl Clone for SizingState {
    fn clone(&self) -> Self {
        Self {
            sizes: Arc::new(Mutex::new(self.sizes.lock().unwrap().clone())),
            ..Self::default()
        }
    }
}

pub(super) struct SizeChange {
    sizes: Arc<Mutex<Vec<ChartLayoutSizing>>>,
    other: Vec<(ChartLayoutMode, Option<ChartLayoutSizing>)>,
}

impl SizingState {
    fn record(&mut self, before: Vec<ChartLayoutSizing>) {
        let after = self.sizes.lock().unwrap();
        let mut other = Vec::new();
        for mode in before.iter().chain(after.iter()).map(|size| size.mode) {
            let old = before.iter().find(|size| size.mode == mode);
            let new = after.iter().find(|size| size.mode == mode);
            if old != new && !other.iter().any(|(previous, _)| *previous == mode) {
                other.push((mode, old.cloned()));
            }
        }
        if !other.is_empty() {
            self.pending.push(SizeChange {
                sizes: Arc::clone(&self.sizes),
                other,
            });
        }
    }
}

impl SizeChange {
    pub(super) fn swap(&mut self) {
        let mut sizes = self.sizes.lock().unwrap();
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
pub enum ChartLink {
    /// Follow the layout-wide switch and legacy group. No individual marker is shown.
    #[default]
    Layout,
    /// Exclude this field from synchronization, including the layout-wide switch.
    Unlinked,
    /// Link this field to charts carrying the same identifier, independently of the switch.
    Group(String),
}

/// Independent symbol and interval memberships, suitable for host serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChartLinkGroups {
    /// Symbol membership.
    pub symbol: ChartLink,
    /// Interval membership.
    pub interval: ChartLink,
}

#[derive(Default)]
struct LinkState {
    groups: Arc<Mutex<[ChartLinkGroups; 16]>>,
    pending: Vec<LinkChange>,
}

impl Clone for LinkState {
    fn clone(&self) -> Self {
        Self {
            groups: Arc::new(Mutex::new(self.groups.lock().unwrap().clone())),
            pending: Vec::new(),
        }
    }
}

pub(super) struct LinkChange {
    groups: Arc<Mutex<[ChartLinkGroups; 16]>>,
    index: usize,
    symbol: Option<ChartLink>,
    interval: Option<ChartLink>,
}

impl LinkChange {
    pub(super) fn swap(&mut self) {
        let mut groups = self.groups.lock().unwrap();
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
#[derive(Default)]
struct LayoutEdits {
    steps: Arc<Mutex<Vec<(u64, bool)>>>,
}

impl Clone for LayoutEdits {
    /// A copy serves other charts, whose steps do not concern this layout.
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl LayoutEdits {
    #[cfg(any(feature = "studies", feature = "drawings"))]
    fn start(&mut self) -> LayoutEditLink {
        // Process-wide, so that a clone of a layout, which starts a new queue, never reuses an
        // id that a chart's history may already hold from the original.
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
        self.steps.lock().unwrap().push((self.id, redo));
    }
}

/// Independent synchronization switches within each layout group.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSyncOptions {
    /// Synchronize scrolling and zooming using UTC plot edges.
    pub sync_time_axis: bool,
    /// Synchronize the hovered timestamp.
    pub sync_crosshair: bool,
    /// Synchronize completed main-pane drawings between charts of the same symbol.
    /// Study-owned drawings stay local because study ids and scales belong to each chart.
    /// Charts that keep their drawings in the same storage are left to it: they read each other's
    /// stored drawings whether this is on or off, so a drawing reaches them once a chart history
    /// edit writes it. Only drawings that are not saveable still sync between them here.
    /// A top-level folder's `FolderSync` overrides this for its drawings.
    pub sync_drawings: bool,
    /// Synchronize symbol changes for fields following the layout setting.
    /// Explicit individual groups work independently. A host without a feed supplies bars.
    pub sync_symbol: bool,
    /// Synchronize interval changes for fields following the layout setting.
    /// Explicit individual groups work independently. A host without a feed supplies bars.
    pub sync_timeframe: bool,
    /// Apply host date-range selections to the group, independently of scrolling and zooming.
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
/// charts through host storage as needed. Synchronization includes supplied slots hidden by a
/// grid preset. The container does not acquire data or persist it. Charts that keep their
/// drawings in the same storage (`Chart::set_storage`) read the drawings of their symbol again
/// after another of them wrote them, in any group.
///
/// ```
/// # fn example(ui: &mut egui::Ui) {
/// use egui_charts::widget::{Chart, ChartLayout, ChartLayoutMode};
/// let mut a = Chart::new(egui_charts::model::BarData::new());
/// let mut b = Chart::new(egui_charts::model::BarData::new());
/// let mut layout = ChartLayout::default();
/// layout.mode = ChartLayoutMode::TwoHorizontal;
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
    /// The chart slot Alt+Enter shows alone in place of the preset.
    maximized: Option<usize>,
    focus_active: bool,
    /// The slot whose corner menu is open. It stays shown when a click selects another chart, so
    /// a name being edited in it can still be committed.
    corner_menu: Option<usize>,
    groups: [Option<String>; 16],
    links: LinkState,
    detached: [bool; 16],
    previous: [Option<(String, Option<Timeframe>)>; 16],
    previous_charts: [u64; 16],
    previous_ranges: [Option<VisibleTimeRange>; 16],
    previous_bars: [BarsKey; 16],
    pending_ranges: [Option<PendingRange>; 16],
    #[cfg(feature = "drawings")]
    drawings: DrawingLinks,
    previous_preset: Option<(ChartLayoutMode, usize)>,
    sizing: SizingState,
    edits: LayoutEdits,
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
        self.sizing.sizes.lock().unwrap().clone()
    }

    /// Current relative sizes of a preset's splits, including its default weights when unedited.
    /// Splits are visited depth first: the outer rows or columns, then each child's subdivisions.
    pub fn sizes(&self, mode: ChartLayoutMode) -> ChartLayoutSizing {
        self.sizing
            .sizes
            .lock()
            .unwrap()
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
            *self.sizing.sizes.lock().unwrap() = updated;
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
            self.sizing.sizes.lock().unwrap().clear();
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

    fn step_layout_edits(&mut self, charts: &mut [&mut Chart]) {
        loop {
            let step = self.edits.steps.lock().unwrap().pop();
            let Some((id, redo)) = step else {
                return;
            };
            for chart in charts.iter_mut() {
                chart.step_layout_edit(id, redo);
            }
        }
    }

    /// Active chart slot, clamped to supplied charts by [`Self::show`].
    pub fn active(&self) -> usize {
        self.active
    }

    /// Selects a chart slot and focuses its canvas when shown; out-of-range indices are ignored.
    pub fn set_active(&mut self, index: usize) {
        if index < 16 {
            self.active = index;
            self.focus_active = true;
        }
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
    pub fn saved_link_groups(&self) -> [ChartLinkGroups; 16] {
        self.links.groups.lock().unwrap().clone()
    }

    /// Memberships of a slot; invalid indices return `None`.
    pub fn link_groups(&self, index: usize) -> Option<ChartLinkGroups> {
        self.links.groups.lock().unwrap().get(index).cloned()
    }

    /// Sets independent field memberships. Empty identifiers are treated as unlinked.
    /// Unless `skip_history` is true, records one undo step on the active chart at the next
    /// [`Self::show`]. Undo restores only the fields changed by this call.
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
        let mut current = self.links.groups.lock().unwrap();
        let Some(slot) = current.get_mut(index) else {
            return;
        };
        if !skip_history && *slot != groups {
            self.links.pending.push(LinkChange {
                groups: Arc::clone(&self.links.groups),
                index,
                symbol: (slot.symbol != groups.symbol).then(|| slot.symbol.clone()),
                interval: (slot.interval != groups.interval).then(|| slot.interval.clone()),
            });
        }
        *slot = groups;
    }

    fn record_links(&mut self, chart: &mut Chart) {
        for change in self.links.pending.drain(..) {
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

    fn linked(&self, a: usize, b: usize, interval: bool) -> bool {
        let groups = self.links.groups.lock().unwrap();
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

    /// Fits an explicit date selection to the active chart, or its synchronization group
    /// when `sync_date_range` is enabled. Call after the charts have rendered at their sizes.
    pub fn apply_date_range(&self, charts: &mut [&mut Chart], range: super::VisibleTimeRange) {
        let Some(active) = charts.get_mut(self.active) else {
            return;
        };
        let source = active
            .fit_time_range(range, f64::INFINITY)
            .then(|| active.sync_source())
            .flatten();
        for (index, chart) in charts.iter_mut().take(16).enumerate() {
            if index != self.active
                && self.sync.sync_date_range
                && self.same_group(index, self.active)
            {
                chart.apply_synced_time_range_from(range, source);
            }
        }
    }

    fn same_group(&self, a: usize, b: usize) -> bool {
        self.groups[a] == self.groups[b]
    }

    /// Shows the preset and synchronizes its charts. Click or scroll a panel to activate it.
    /// Its header and corner menu offer independent symbol and interval groups; the corner menu
    /// also offers detachment and the synchronization group. The active border is painted
    /// inside the chart; stacked charts have a two-point gap and side-by-side charts meet at their
    /// edge.
    /// Dragging a border resizes its neighbours and Shift also moves the borders aligned with it; a
    /// touch must tap a border before moving it. Double-clicking a border restores the proportions.
    /// Extra charts remain owned by the host and are not painted by a grid preset.
    pub fn show(&mut self, ui: &mut Ui, charts: &mut [&mut Chart]) -> Response {
        let total = charts.len().min(16);
        let charts = &mut charts[..total];
        let count = total.min(self.mode.chart_count());
        if count == 0 {
            return ui.allocate_response(Vec2::ZERO, Sense::hover());
        }
        self.active = self.active.min(count - 1);
        if self.maximized.is_some_and(|index| index >= count) {
            self.maximized = None;
        }
        if let Some(index) = self.maximized {
            self.active = index;
        }
        // A chart supplied or restored in a slot starts a new baseline: nothing changed in it.
        for (index, chart) in charts.iter().enumerate() {
            if self.previous_charts[index] != chart.layout_identity {
                self.previous_charts[index] = chart.layout_identity;
                self.previous[index] = None;
                self.previous_ranges[index] = None;
                self.pending_ranges[index] = None;
            }
        }
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
        self.record_sizes(charts[self.active]);
        self.record_links(charts[self.active]);
        self.step_layout_edits(charts);
        let (mut rects, mut dividers) = self.geometry(rect, count);
        if self.move_divider(ui) {
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
        }
        let mut pointer_down = ui.input(|i| i.pointer.any_down());
        let before_range = charts[self.active].visible_time_range();
        let before_sync: Vec<_> = charts
            .iter()
            .map(|c| (c.synced_crosshair_bar_idx, c.get_time_scale_state()))
            .collect();
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
                            let response = chart.show(ui);
                            if focus {
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
            ui.painter()
                .rect_filled(rect, 0.0, charts[index].config().background_color);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .id_salt(("chart_slot", index)),
                |ui| {
                    ui.shrink_clip_rect(rect);
                    let link_controls = total > 1 && rect.width() >= LINK_CONTROLS_MIN_WIDTH;
                    charts[index].layout_link_controls = link_controls;
                    let response = charts[index].show(ui);
                    charts[index].layout_link_controls = false;
                    charts[index].layout_inactive = false;
                    if focus {
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
                    }
                    if self.corner_menu == Some(index)
                        || index == self.active
                            && (ui.rect_contains_pointer(rect)
                                || egui::Popup::is_any_open(ui.ctx())
                                || total > 1 && rect.width() < LINK_CONTROLS_MIN_WIDTH)
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
        self.record_sizes(charts[self.active]);
        self.record_links(charts[self.active]);
        self.focus_active = false;
        // Alt+Enter toggles the focused tile in a grid; a lone chart leaves it to the host.
        if self.mode != ChartLayoutMode::Tabs && count > 1 {
            for (index, chart) in charts.iter_mut().take(count).enumerate() {
                let requests = &mut chart.keyboard.requests;
                let before = requests.len();
                requests.retain(|request| {
                    *request != ShortcutRequest::Action(KeyboardAction::MaximizeChart)
                });
                // A discarded pass replays its input, so only the last one toggles.
                if requests.len() != before && !ui.ctx().will_discard() {
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
        // Apply gestures from the source after it has handled this frame's input.
        let sync_range = charts[self.active].visible_time_range() != before_range
            || self.previous_preset != Some((self.mode, count));
        self.sync_view(charts, total, sync_range);
        self.previous_preset = Some((self.mode, count));
        if charts
            .iter()
            .zip(before_sync)
            .any(|(chart, (crosshair, (spacing, offset)))| {
                chart.synced_crosshair_bar_idx != crosshair
                    || (chart.get_time_scale_state().0 - spacing).abs() > 0.001
                    || (chart.get_time_scale_state().1 - offset).abs() > 0.001
            })
        {
            ui.ctx().request_repaint();
        }
        #[cfg(feature = "drawings")]
        if (self.sync.sync_drawings
            || charts.iter().take(total).any(|chart| {
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .any(|d| d.folder_sync_mode().is_some())
            }))
            && self.drawings.sync(
                charts,
                total,
                &self.groups,
                pointer_down,
                self.sync.sync_drawings,
            )
        {
            ui.ctx().request_repaint();
        }
        #[cfg(feature = "drawings")]
        if Chart::share_stored_drawings(charts) {
            ui.ctx().request_repaint();
        }
        for (index, chart) in charts.iter().enumerate() {
            // A change the active chart made while rendering, such as keyboard undo, stays
            // unrecorded, so the next frame sends it like one made between frames.
            if index != self.active {
                self.record_state(index, chart);
            } else if !self.state_recorded(index, chart) {
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

    fn sync_state(&mut self, charts: &mut [&mut Chart], count: usize) {
        let source = &charts[self.active];
        let Some(previous) = &self.previous[self.active] else {
            return;
        };
        let symbol_changed = previous.0 != source.symbol();
        let timeframe_changed = previous.1 != source.timeframe();
        if !symbol_changed && !timeframe_changed {
            return;
        }
        let symbol = source.symbol().to_owned();
        let timeframe = source.timeframe();
        for (index, chart) in charts.iter_mut().take(count).enumerate() {
            if index == self.active {
                continue;
            }
            if self.linked(index, self.active, false) && symbol_changed {
                chart.set_symbol(&symbol);
            }
            if self.linked(index, self.active, true)
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
            chart.set_synced_crosshair(None, None);
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
        let count = count.min(16).min(self.mode.chart_count());
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
        if self.mode == ChartLayoutMode::Tabs || self.maximized.is_some() {
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
                *self.sizing.sizes.lock().unwrap() = before;
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
            *self.sizing.sizes.lock().unwrap() = before;
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
        if let Some(slot) = self.maximized {
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
        assert_eq!(layout.visible_chart_rects(bounds, 4), default);
        assert_eq!(
            layout.sizes(ChartLayoutMode::TwoVertical),
            hidden,
            "undoing one preset must retain unrelated host size updates"
        );
        assert!(charts[0].redo());
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
        assert_eq!(layout.link_groups(0).unwrap(), ChartLinkGroups::default());
        assert!(charts[0].redo());
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
        let before = refs[1].get_time_scale_state();
        layout.apply_date_range(&mut refs, range);
        assert_eq!(refs[1].get_time_scale_state(), before);
        layout.sync.sync_date_range = true;
        let isolated = refs[2].get_time_scale_state();
        layout.apply_date_range(&mut refs, range);
        assert_ne!(refs[1].get_time_scale_state(), before);
        assert_eq!(refs[2].get_time_scale_state(), isolated);

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
            groups.symbol = Group("🔶".into());
            assert_eq!(linked.link_groups(0), Some(groups.clone()));
            assert!(charts[0].redo());
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

    /// Alt+Enter shows the focused tile alone and a second press brings the preset back with the
    /// tile's slot, viewport and synchronization group unchanged.
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
        let mut frame = |layout: &mut ChartLayout, charts: &mut [Chart; 4], pressed: bool| {
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
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                },
            )
            .drop_without_applying_deltas();
        };
        for _ in 0..3 {
            frame(&mut layout, &mut charts, false);
        }
        let tile = charts[2].get_rendered_price_rect();
        let viewport = charts[2].get_time_scale_state();
        frame(&mut layout, &mut charts, true);
        // The chart's widget id follows its rect, so focus moves over to it on the next frames.
        frame(&mut layout, &mut charts, false);
        frame(&mut layout, &mut charts, false);
        assert_eq!((layout.maximized, layout.active()), (Some(2), 2));
        assert!(charts[2].get_rendered_price_rect().width() > tile.width() * 1.8);
        assert_eq!(layout.sync_group(2), Some("B"));
        assert!(charts[2].take_shortcut_requests().is_empty());
        frame(&mut layout, &mut charts, true);
        frame(&mut layout, &mut charts, false);
        frame(&mut layout, &mut charts, false);
        assert_eq!((layout.maximized, layout.active()), (None, 2));
        assert_eq!(charts[2].get_rendered_price_rect(), tile);
        assert_eq!(charts[2].get_time_scale_state(), viewport);
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
    /// crosshair, drawing, date range and replay links, and leaves its field memberships alone.
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
            let symbol = || layout.link_groups(slot).unwrap().symbol;
            if committed {
                assert_eq!(symbol(), ChartLink::Group("Desk".into()));
                let active = layout.active();
                assert!(charts[active].undo());
                assert_eq!(symbol(), ChartLink::Layout, "one name, one undo step");
                assert!(charts[active].redo());
                assert_eq!(symbol(), ChartLink::Group("Desk".into()));
            } else {
                assert_eq!(symbol(), ChartLink::Layout);
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
    /// which starts 8 points right of the slot's left edge, only from 8 + 116 + 228 = 352 points.
    /// Narrower slots leave the selectors to the corner menu; the slot of exactly 352 keeps them.
    #[cfg(feature = "icons")]
    #[test]
    fn title_controls_fit_in_narrow_linked_slots() {
        for (target, shows_selectors) in [
            (320.0_f32, false),
            (336.0, false),
            (351.0, false),
            (352.0, true),
        ] {
            let ctx = egui::Context::default();
            let mut layout = ChartLayout {
                mode: ChartLayoutMode::TwoHorizontal,
                ..Default::default()
            };
            let mut charts = [chart(60), chart(60)];
            let mut size = vec2(2.0 * target, 600.0);
            let slot = layout.rects(Rect::from_min_size(Pos2::ZERO, size), 2)[0];
            size.x += 2.0 * (target - slot.width());
            let slot = layout.rects(Rect::from_min_size(Pos2::ZERO, size), 2)[0];
            assert_eq!(slot.width(), target, "{size:?}");
            let bg = ctx.active_theme().chart.bg;
            let mut controls = None;
            let mut selectors = false;
            for frame in 0..4 {
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        time: Some(frame as f64 * 0.1),
                        events: vec![Event::PointerMoved(pos2(
                            slot.left() + 40.0,
                            slot.top() + 14.0,
                        ))],
                        ..Default::default()
                    },
                    |ui| {
                        layout.show(ui, &mut charts.iter_mut().collect::<Vec<_>>());
                    },
                );
                for shape in &output.shapes {
                    match &shape.shape {
                        egui::Shape::Rect(rect)
                            if rect.fill == bg
                                && rect.rect.size() == vec2(116.0, 20.0)
                                && slot.intersects(rect.rect) =>
                        {
                            controls = Some(rect.rect);
                        }
                        egui::Shape::Text(text) if text.galley.text().starts_with("S ") => {
                            selectors |= slot.contains(text.pos);
                        }
                        _ => {}
                    }
                }
                output.drop_without_applying_deltas();
            }
            let controls = controls.expect("the hovered title shows its controls");
            let flag = Rect::from_min_size(controls.min, Vec2::splat(20.0));
            assert!(
                flag.left() >= slot.left() + 8.0 && slot.contains_rect(flag),
                "slot {target}: flag {flag:?} leaves the status line"
            );
            assert_eq!(selectors, shows_selectors, "slot {target}");
            if shows_selectors {
                assert!(
                    controls.right() <= slot.right() - LINK_CONTROLS_INSET,
                    "slot {target}: controls {controls:?} reach the selectors"
                );
            }
        }
    }
}
