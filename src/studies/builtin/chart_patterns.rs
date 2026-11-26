//! Automatic chart patterns found in confirmed swing pivots.
//!
//! [`ChartPatterns`] detects continuation formations (flags, pennants, rectangles and
//! triangles), reversal formations (cup and handle, double and triple tops and bottoms,
//! head and shoulders), or every formation together, including wedges, in a bar history, and
//! returns each one's anchors, boundary segments, direction and confirmation time through
//! [`ChartPatterns::formations`]. Developing formations are told apart from confirmed ones, and
//! every confirmed formation is marked on the bar that confirms it. The factory also offers
//! each formation under its own name.

use super::clamp_param;
use super::swings::{Swing, Swings};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, InputKind, InputValue, StudyInput,
};
use egui::Color32;
use std::collections::VecDeque;

/// A flag's end width stays within this fraction of its start width, and a pennant, triangle
/// or other converging pair of lines ends at most this fraction narrower.
const SLACK: f64 = 0.25;
/// Pivots kept: enough for the longest formation, eight touches on each of two boundaries.
const MAX_PIVOTS: usize = 16;
/// Developing formations kept; the oldest is dropped first.
const MAX_PENDING: usize = 8;
/// Confirmed formations kept; the oldest is dropped first.
const MAX_CONFIRMED: usize = 64;
/// A cup spans at least this many bars, whatever the minimum length.
const MIN_CUP: usize = 20;

/// The formation a detector looks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormationKind {
    /// A rise, then a falling channel; confirms above it.
    BullFlag,
    /// A fall, then a rising channel; confirms below it.
    BearFlag,
    /// A rise, then converging lines; confirms above them.
    BullPennant,
    /// A fall, then converging lines; confirms below them.
    BearPennant,
    /// Flat highs over flat lows; confirms in the direction of the break.
    Rectangle,
    /// Flat highs over rising lows; confirms above.
    AscendingTriangle,
    /// Falling highs over flat lows; confirms below.
    DescendingTriangle,
    /// Falling highs over rising lows; confirms in the direction of the break.
    SymmetricalTriangle,
    /// A rounded fall and recovery to the same level, a small pullback, then a break above the rim.
    CupAndHandle,
    /// The mirror image: a rounded rise and return, a small bounce, then a break below the rim.
    InvertedCupAndHandle,
    /// Two tops at the same level; confirms below the valley between them.
    DoubleTop,
    /// Two bottoms at the same level; confirms above the peak between them.
    DoubleBottom,
    /// Three tops at the same level; confirms below the lower valley.
    TripleTop,
    /// Three bottoms at the same level; confirms above the higher peak.
    TripleBottom,
    /// A head above two shoulders; confirms below the neckline through the valleys.
    HeadAndShoulders,
    /// A head below two shoulders; confirms above the neckline through the peaks.
    InverseHeadAndShoulders,
    /// Rising highs over more steeply rising lows; confirms below.
    RisingWedge,
    /// Falling lows under more steeply falling highs; confirms above.
    FallingWedge,
}

/// Every formation, as [`ChartPatterns::all`] lists them: the continuation family, the reversal
/// family, then the wedges.
static ALL_KINDS: [FormationKind; 18] = [
    FormationKind::BullFlag,
    FormationKind::BearFlag,
    FormationKind::BullPennant,
    FormationKind::BearPennant,
    FormationKind::Rectangle,
    FormationKind::AscendingTriangle,
    FormationKind::DescendingTriangle,
    FormationKind::SymmetricalTriangle,
    FormationKind::CupAndHandle,
    FormationKind::InvertedCupAndHandle,
    FormationKind::DoubleTop,
    FormationKind::DoubleBottom,
    FormationKind::TripleTop,
    FormationKind::TripleBottom,
    FormationKind::HeadAndShoulders,
    FormationKind::InverseHeadAndShoulders,
    FormationKind::RisingWedge,
    FormationKind::FallingWedge,
];

impl FormationKind {
    fn label(self) -> &'static str {
        match self {
            Self::BullFlag => "Bull Flag",
            Self::BearFlag => "Bear Flag",
            Self::BullPennant => "Bull Pennant",
            Self::BearPennant => "Bear Pennant",
            Self::Rectangle => "Rectangle",
            Self::AscendingTriangle => "Ascending Triangle",
            Self::DescendingTriangle => "Descending Triangle",
            Self::SymmetricalTriangle => "Symmetrical Triangle",
            Self::CupAndHandle => "Cup and Handle",
            Self::InvertedCupAndHandle => "Inverted Cup and Handle",
            Self::DoubleTop => "Double Top",
            Self::DoubleBottom => "Double Bottom",
            Self::TripleTop => "Triple Top",
            Self::TripleBottom => "Triple Bottom",
            Self::HeadAndShoulders => "Head and Shoulders",
            Self::InverseHeadAndShoulders => "Inverse Head and Shoulders",
            Self::RisingWedge => "Rising Wedge",
            Self::FallingWedge => "Falling Wedge",
        }
    }

    fn abbreviation(self) -> &'static str {
        match self {
            Self::BullFlag => "BF",
            Self::BearFlag => "SF",
            Self::BullPennant => "BP",
            Self::BearPennant => "SP",
            Self::Rectangle => "REC",
            Self::AscendingTriangle => "ATR",
            Self::DescendingTriangle => "DTR",
            Self::SymmetricalTriangle => "STR",
            Self::CupAndHandle => "CH",
            Self::InvertedCupAndHandle => "ICH",
            Self::DoubleTop => "DT",
            Self::DoubleBottom => "DB",
            Self::TripleTop => "TT",
            Self::TripleBottom => "TB",
            Self::HeadAndShoulders => "HS",
            Self::InverseHeadAndShoulders => "IHS",
            Self::RisingWedge => "RW",
            Self::FallingWedge => "FW",
        }
    }

    /// The direction the formation resolves in, `None` for a two-sided one.
    fn expected(self) -> Option<PatternDirection> {
        match self {
            Self::BullFlag
            | Self::BullPennant
            | Self::AscendingTriangle
            | Self::CupAndHandle
            | Self::DoubleBottom
            | Self::TripleBottom
            | Self::InverseHeadAndShoulders
            | Self::FallingWedge => Some(PatternDirection::Bullish),
            Self::BearFlag
            | Self::BearPennant
            | Self::DescendingTriangle
            | Self::InvertedCupAndHandle
            | Self::DoubleTop
            | Self::TripleTop
            | Self::HeadAndShoulders
            | Self::RisingWedge => Some(PatternDirection::Bearish),
            Self::Rectangle | Self::SymmetricalTriangle => None,
        }
    }
}

/// Which formations a [`ChartPatterns`] study looks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternFamily {
    /// Flags, pennants, rectangles and triangles.
    Continuation,
    /// Cup and handle, double and triple tops and bottoms, head and shoulders.
    Reversal,
}

impl PatternFamily {
    /// Every family.
    pub(crate) const ALL: [Self; 2] = [Self::Continuation, Self::Reversal];

    pub(crate) fn kinds(self) -> &'static [FormationKind] {
        match self {
            Self::Continuation => &[
                FormationKind::BullFlag,
                FormationKind::BearFlag,
                FormationKind::BullPennant,
                FormationKind::BearPennant,
                FormationKind::Rectangle,
                FormationKind::AscendingTriangle,
                FormationKind::DescendingTriangle,
                FormationKind::SymmetricalTriangle,
            ],
            Self::Reversal => &[
                FormationKind::CupAndHandle,
                FormationKind::InvertedCupAndHandle,
                FormationKind::DoubleTop,
                FormationKind::DoubleBottom,
                FormationKind::TripleTop,
                FormationKind::TripleBottom,
                FormationKind::HeadAndShoulders,
                FormationKind::InverseHeadAndShoulders,
            ],
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Continuation => "Continuation Chart Patterns",
            Self::Reversal => "Reversal Chart Patterns",
        }
    }
}

/// The formations a study looks for: a family, every formation, or one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Family(PatternFamily),
    All,
    One(FormationKind),
}

impl Scope {
    fn kinds(self) -> &'static [FormationKind] {
        match self {
            Self::Family(family) => family.kinds(),
            Self::All => &ALL_KINDS,
            Self::One(kind) => {
                let at = ALL_KINDS.iter().position(|&k| k == kind).unwrap();
                &ALL_KINDS[at..=at]
            }
        }
    }

    /// Whether the scope holds a formation of `family`; wedges count as continuation.
    fn covers(self, family: PatternFamily) -> bool {
        let reversal = |kind: &FormationKind| PatternFamily::Reversal.kinds().contains(kind);
        self.kinds()
            .iter()
            .any(|kind| reversal(kind) == (family == PatternFamily::Reversal))
    }
}

/// The side a confirmed formation points to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternDirection {
    /// A break above the formation.
    Bullish,
    /// A break below the formation.
    Bearish,
}

/// Whether a formation has broken out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormationStatus {
    /// All its pivots are known but no close has broken its boundary yet.
    Developing,
    /// A close broke its boundary, on bar [`Formation::confirmed_at`].
    Confirmed,
}

/// A detected formation, with bar indices into the calculated bars.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Formation {
    /// The formation's shape.
    pub kind: FormationKind,
    /// Developing or confirmed.
    pub status: FormationStatus,
    /// The confirmed direction; for a developing formation the direction its kind resolves in,
    /// `None` for a rectangle or symmetrical triangle.
    pub direction: Option<PatternDirection>,
    /// The pivots that form it, as bar index and price, oldest first.
    pub anchors: Vec<(usize, f64)>,
    /// Boundary segments as bar index and price at each end. A confirmed formation's end at
    /// its confirmation bar, a developing one's at the last bar.
    pub boundaries: Vec<[(usize, f64); 2]>,
    /// The bar whose close confirmed the formation.
    pub confirmed_at: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct Config {
    strength: usize,
    min_length: usize,
    max_length: usize,
    tolerance: f64,
    touches: usize,
    pole: f64,
    depth: f64,
    handle: f64,
}

/// A straight line in position and price.
#[derive(Clone, Copy, Debug)]
struct Line {
    seq: f64,
    price: f64,
    slope: f64,
}

impl Line {
    fn through(a: &Swing, b: &Swing) -> Self {
        Self {
            seq: a.position as f64,
            price: a.price,
            slope: (b.price - a.price) / (b.position as f64 - a.position as f64),
        }
    }

    fn at(&self, seq: usize) -> f64 {
        self.price + self.slope * (seq as f64 - self.seq)
    }
}

#[derive(Clone, Copy, Debug)]
enum Outcome {
    Confirm(PatternDirection),
    Fail,
}

/// A formation whose pivots are known, waiting for a close beyond a boundary.
#[derive(Clone, Debug)]
struct Candidate {
    kind: FormationKind,
    anchors: Vec<Swing>,
    /// Index of the anchor the drawn boundaries start at.
    from: usize,
    upper: Line,
    lower: Line,
    on_up: Outcome,
    on_down: Outcome,
    /// Whether the upper and lower line are reported as boundaries.
    shown: [bool; 2],
}

impl Candidate {
    fn resolve(&self, close: f64, seq: usize, max_length: usize) -> Option<Outcome> {
        if seq - self.anchors[0].position > max_length {
            Some(Outcome::Fail)
        } else if close > self.upper.at(seq) {
            Some(self.on_up)
        } else if close < self.lower.at(seq) {
            Some(self.on_down)
        } else {
            None
        }
    }

    fn formation(
        &self,
        direction: Option<PatternDirection>,
        confirmed: bool,
        end: (usize, usize),
    ) -> Formation {
        let start = self.anchors[self.from];
        Formation {
            kind: self.kind,
            status: if confirmed {
                FormationStatus::Confirmed
            } else {
                FormationStatus::Developing
            },
            direction,
            anchors: self.anchors.iter().map(|p| (p.index, p.price)).collect(),
            boundaries: [self.upper, self.lower]
                .iter()
                .zip(self.shown)
                .filter(|&(_, shown)| shown)
                .map(|(line, _)| {
                    [
                        (start.index, line.at(start.position)),
                        (end.1, line.at(end.0)),
                    ]
                })
                .collect(),
            confirmed_at: confirmed.then_some(end.1),
        }
    }
}

/// `a` and `b` differ by at most `tolerance` percent of their mean.
fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance / 100.0 * (a.abs() + b.abs()) / 2.0
}

fn last_pivots(pivots: &VecDeque<Swing>, count: usize) -> Option<Vec<Swing>> {
    (pivots.len() >= count).then(|| pivots.iter().skip(pivots.len() - count).copied().collect())
}

/// A flag or pennant: a pole from the first pivot to the second, then lines through the
/// second and fourth pivots and through the third and fifth.
fn flag(config: &Config, pivots: &VecDeque<Swing>, bull: bool, pennant: bool) -> Option<Candidate> {
    let w = last_pivots(pivots, 5)?;
    if w[0].high == bull || (w[1].price > w[0].price) != bull {
        return None;
    }
    let pole = (w[1].price - w[0].price).abs() / w[0].price.abs() * 100.0;
    let length = w[4].position - w[1].position;
    if pole < config.pole
        || length < config.min_length
        || w[4].position - w[0].position > config.max_length
    {
        return None;
    }
    let (a, b) = (Line::through(&w[1], &w[3]), Line::through(&w[2], &w[4]));
    let (upper, lower) = if bull { (a, b) } else { (b, a) };
    let width = |seq| upper.at(seq) - lower.at(seq);
    let (start, end) = (width(w[1].position), width(w[4].position));
    let middle = (w[0].price + w[1].price) / 2.0;
    if start <= 0.0 || end <= 0.0 || w[2..].iter().step_by(2).any(|p| (p.price > middle) != bull) {
        return None;
    }
    let shaped = if pennant {
        upper.slope < 0.0 && lower.slope > 0.0 && end <= start * (1.0 - SLACK)
    } else {
        let falling = |line: Line| (line.slope < 0.0) == bull && line.slope != 0.0;
        falling(upper) && falling(lower) && (end / start - 1.0).abs() <= SLACK
    };
    shaped.then_some(Candidate {
        kind: match (bull, pennant) {
            (true, false) => FormationKind::BullFlag,
            (false, false) => FormationKind::BearFlag,
            (true, true) => FormationKind::BullPennant,
            (false, true) => FormationKind::BearPennant,
        },
        anchors: w,
        from: 1,
        upper,
        lower,
        shown: [true; 2],
        on_up: if bull {
            Outcome::Confirm(PatternDirection::Bullish)
        } else {
            Outcome::Fail
        },
        on_down: if bull {
            Outcome::Fail
        } else {
            Outcome::Confirm(PatternDirection::Bearish)
        },
    })
}

/// A rectangle, triangle or wedge through the last `2 * touches` pivots.
fn channel(config: &Config, pivots: &VecDeque<Swing>) -> Option<Candidate> {
    let w = last_pivots(pivots, 2 * config.touches)?;
    let side =
        |high: bool| -> Vec<Swing> { w.iter().filter(|p| p.high == high).copied().collect() };
    let (highs, lows) = (side(true), side(false));
    let upper = Line::through(&highs[0], highs.last()?);
    let lower = Line::through(&lows[0], lows.last()?);
    let on_line = |pivots: &[Swing], line: Line| {
        pivots
            .iter()
            .all(|p| near(p.price, line.at(p.position), config.tolerance))
    };
    let (first, last) = (w[0].position, w[w.len() - 1].position);
    if !on_line(&highs, upper)
        || !on_line(&lows, lower)
        || !(config.min_length..=config.max_length).contains(&(last - first))
    {
        return None;
    }
    let (start, end) = (
        upper.at(first) - lower.at(first),
        upper.at(last) - lower.at(last),
    );
    if start <= 0.0 || end <= 0.0 {
        return None;
    }
    let flat = |line: Line| {
        (line.at(last) - line.at(first)).abs()
            <= config.tolerance / 100.0 * line.at(first).abs().max(line.at(last).abs())
    };
    let kind = match (flat(upper), flat(lower)) {
        (true, true) if end >= 2.0 * config.tolerance / 100.0 * lower.at(last).abs() => {
            FormationKind::Rectangle
        }
        (true, false) if lower.slope > 0.0 => FormationKind::AscendingTriangle,
        (false, true) if upper.slope < 0.0 => FormationKind::DescendingTriangle,
        (false, false) if upper.slope < 0.0 && lower.slope > 0.0 => {
            FormationKind::SymmetricalTriangle
        }
        (false, false) if upper.slope > 0.0 && lower.slope > upper.slope => {
            FormationKind::RisingWedge
        }
        (false, false) if lower.slope < 0.0 && upper.slope < lower.slope => {
            FormationKind::FallingWedge
        }
        _ => return None,
    };
    let up = Outcome::Confirm(PatternDirection::Bullish);
    let down = Outcome::Confirm(PatternDirection::Bearish);
    Some(Candidate {
        kind,
        anchors: w,
        from: 0,
        upper,
        lower,
        shown: [true; 2],
        on_up: if matches!(
            kind,
            FormationKind::DescendingTriangle | FormationKind::RisingWedge
        ) {
            Outcome::Fail
        } else {
            up
        },
        on_down: if matches!(
            kind,
            FormationKind::AscendingTriangle | FormationKind::FallingWedge
        ) {
            Outcome::Fail
        } else {
            down
        },
    })
}

/// A horizontal line.
fn level(price: f64) -> Line {
    Line {
        seq: 0.0,
        price,
        slope: 0.0,
    }
}

/// A double or triple top or bottom through the last three or five pivots.
fn multiple(
    config: &Config,
    pivots: &VecDeque<Swing>,
    top: bool,
    triple: bool,
) -> Option<Candidate> {
    let w = last_pivots(pivots, if triple { 5 } else { 3 })?;
    if w[0].high != top {
        return None;
    }
    let peaks: Vec<f64> = w.iter().step_by(2).map(|p| p.price).collect();
    let valleys: Vec<f64> = w.iter().skip(1).step_by(2).map(|p| p.price).collect();
    let (high, low) = (
        peaks.iter().copied().fold(f64::MIN, f64::max),
        peaks.iter().copied().fold(f64::MAX, f64::min),
    );
    let mean = peaks.iter().sum::<f64>() / peaks.len() as f64;
    let apart = w
        .windows(3)
        .all(|p| p[2].position - p[0].position >= config.min_length);
    // A valley this far below (above) a top (bottom).
    let deep = |valley: f64| (mean - valley) * if top { 1.0 } else { -1.0 } / mean.abs() * 100.0;
    if !near(high, low, config.tolerance)
        || !apart
        || w[w.len() - 1].position - w[0].position > config.max_length
        || valleys.iter().any(|&v| deep(v) < config.depth)
    {
        return None;
    }
    let tolerance = config.tolerance / 100.0;
    let (neckline, kind) = match (top, triple) {
        (true, false) => (
            valleys.iter().copied().fold(f64::MAX, f64::min),
            FormationKind::DoubleTop,
        ),
        (true, true) => (
            valleys.iter().copied().fold(f64::MAX, f64::min),
            FormationKind::TripleTop,
        ),
        (false, false) => (
            valleys.iter().copied().fold(f64::MIN, f64::max),
            FormationKind::DoubleBottom,
        ),
        (false, true) => (
            valleys.iter().copied().fold(f64::MIN, f64::max),
            FormationKind::TripleBottom,
        ),
    };
    let (up, down) = (
        Outcome::Confirm(PatternDirection::Bullish),
        Outcome::Confirm(PatternDirection::Bearish),
    );
    let (upper, lower, on_up, on_down, shown) = if top {
        (
            level(high * (1.0 + tolerance)),
            level(neckline),
            Outcome::Fail,
            down,
            [false, true],
        )
    } else {
        (
            level(neckline),
            level(low * (1.0 - tolerance)),
            up,
            Outcome::Fail,
            [true, false],
        )
    };
    Some(Candidate {
        kind,
        anchors: w,
        from: 0,
        upper,
        lower,
        on_up,
        on_down,
        shown,
    })
}

/// A head and shoulders (or its inverse) through the last five pivots.
fn head_and_shoulders(config: &Config, pivots: &VecDeque<Swing>, top: bool) -> Option<Candidate> {
    let w = last_pivots(pivots, 5)?;
    if w[0].high != top {
        return None;
    }
    let side = if top { 1.0 } else { -1.0 };
    let (left, first, head, second, right) = (w[0], w[1], w[2], w[3], w[4]);
    let above = |shoulder: &Swing| {
        side * (head.price - shoulder.price) / shoulder.price.abs() * 100.0 >= config.depth
    };
    if !near(left.price, right.price, config.tolerance)
        || !above(&left)
        || !above(&right)
        || side * first.price >= side * left.price
        || side * second.price >= side * right.price
        || !(config.min_length..=config.max_length).contains(&(right.position - left.position))
    {
        return None;
    }
    let neckline = Line::through(&first, &second);
    let guard = level(head.price);
    let (kind, upper, lower, on_up, on_down, shown) = if top {
        (
            FormationKind::HeadAndShoulders,
            guard,
            neckline,
            Outcome::Fail,
            Outcome::Confirm(PatternDirection::Bearish),
            [false, true],
        )
    } else {
        (
            FormationKind::InverseHeadAndShoulders,
            neckline,
            guard,
            Outcome::Confirm(PatternDirection::Bullish),
            Outcome::Fail,
            [true, false],
        )
    };
    Some(Candidate {
        kind,
        anchors: w,
        from: 1,
        upper,
        lower,
        on_up,
        on_down,
        shown,
    })
}

/// A cup and handle (or its inverse) through the last four pivots: the rims, the cup's
/// extreme between them and the handle's. `bars` holds the high and low of the latest bars, the
/// newest at position `end - 1`.
fn cup(
    config: &Config,
    pivots: &VecDeque<Swing>,
    bars: &VecDeque<[f64; 2]>,
    end: usize,
    bull: bool,
) -> Option<Candidate> {
    let w = last_pivots(pivots, 4)?;
    if w[0].high != bull {
        return None;
    }
    let side = if bull { 1.0 } else { -1.0 };
    let (left, base, right, handle) = (w[0], w[1], w[2], w[3]);
    let mean = (left.price + right.price) / 2.0;
    let depth = side * (mean - base.price);
    let duration = right.position - left.position;
    let pullback = side * (right.price - handle.price);
    if !near(left.price, right.price, config.tolerance)
        || depth <= 0.0
        || depth / mean.abs() * 100.0 < config.depth
        || !(config.min_length.max(MIN_CUP)..=config.max_length).contains(&duration)
        || handle.position - left.position > config.max_length
        || 4 * (base.position - left.position) < duration
        || 4 * (right.position - base.position) < duration
        || pullback <= 0.0
        || pullback > config.handle / 100.0 * depth
        || handle.position - right.position > duration
    {
        return None;
    }
    // A rounded base: at least half the bars between the rims lie in the third of the depth
    // nearest the cup's extreme, where a V spends a third of them.
    let from = (left.position + 1).checked_sub(end - bars.len())?;
    let near_base = bars
        .range(from..from + duration - 1)
        .filter(|&&[high, low]| side * (if bull { low } else { high } - base.price) <= depth / 3.0)
        .count();
    if 2 * near_base < duration - 1 {
        return None;
    }
    let (kind, upper, lower, on_up, on_down, shown) = if bull {
        (
            FormationKind::CupAndHandle,
            level(left.price.max(right.price)),
            level(base.price),
            Outcome::Confirm(PatternDirection::Bullish),
            Outcome::Fail,
            [true, false],
        )
    } else {
        (
            FormationKind::InvertedCupAndHandle,
            level(base.price),
            level(left.price.min(right.price)),
            Outcome::Fail,
            Outcome::Confirm(PatternDirection::Bearish),
            [false, true],
        )
    };
    Some(Candidate {
        kind,
        anchors: w,
        from: 0,
        upper,
        lower,
        on_up,
        on_down,
        shown,
    })
}

/// The detectors' state after a bar: the bars of the latest pivot test, the pivots,
/// the formations waiting for a break and the confirmed ones.
#[derive(Clone, Debug, Default)]
struct State {
    /// Bars seen, finite or not.
    index: usize,
    /// Index of the last finite bar.
    last: usize,
    swings: Swings,
    /// High and low of the latest usable bars, oldest first: as many as a cup and its handle
    /// span when the handle becomes known.
    extremes: VecDeque<[f64; 2]>,
    pending: Vec<Candidate>,
    confirmed: Vec<Formation>,
    /// Per kind, the last pivot of the latest formation found.
    used: [Option<usize>; 18],
}

impl State {
    fn advance(
        &mut self,
        config: &Config,
        scope: Scope,
        enabled: &[bool; 18],
        bar: &Bar,
    ) -> [u32; 2] {
        // Every family keeps its share of developing and confirmed formations.
        let share = if scope == Scope::All { 2 } else { 1 };
        let index = self.index;
        self.index += 1;
        let mut marks = [0; 2];
        if ![bar.open, bar.high, bar.low, bar.close]
            .iter()
            .all(|v| v.is_finite())
        {
            self.swings.skip();
            return marks;
        }
        let seq = self.swings.position;
        self.last = index;
        self.extremes.push_back([bar.high, bar.low]);
        if self.extremes.len()
            > config
                .max_length
                .saturating_add(config.strength)
                .saturating_add(1)
        {
            self.extremes.pop_front();
        }
        if self
            .swings
            .push(bar, config.strength, config.strength, MAX_PIVOTS)
        {
            self.detect(config, scope, enabled, share * MAX_PENDING);
        }
        let mut i = 0;
        while i < self.pending.len() {
            match self.pending[i].resolve(bar.close, seq, config.max_length) {
                None => i += 1,
                Some(Outcome::Fail) => {
                    self.pending.remove(i);
                }
                Some(Outcome::Confirm(direction)) => {
                    let candidate = self.pending.remove(i);
                    self.confirmed
                        .push(candidate.formation(Some(direction), true, (seq, index)));
                    if self.confirmed.len() > share * MAX_CONFIRMED {
                        self.confirmed.remove(0);
                    }
                    let slot = scope
                        .kinds()
                        .iter()
                        .position(|&k| k == candidate.kind)
                        .unwrap();
                    marks[direction as usize] |= 1 << slot;
                }
            }
        }
        marks
    }

    fn detect(&mut self, config: &Config, scope: Scope, enabled: &[bool; 18], limit: usize) {
        let pivots = &self.swings.pivots;
        let mut found: Vec<Option<Candidate>> = Vec::new();
        if scope.covers(PatternFamily::Continuation) {
            found.extend([
                flag(config, pivots, true, false),
                flag(config, pivots, false, false),
                flag(config, pivots, true, true),
                flag(config, pivots, false, true),
                channel(config, pivots),
            ]);
        }
        if scope.covers(PatternFamily::Reversal) {
            found.extend([
                cup(config, pivots, &self.extremes, self.swings.position, true),
                cup(config, pivots, &self.extremes, self.swings.position, false),
                multiple(config, pivots, true, false),
                multiple(config, pivots, false, false),
                multiple(config, pivots, true, true),
                multiple(config, pivots, false, true),
                head_and_shoulders(config, pivots, true),
                head_and_shoulders(config, pivots, false),
            ]);
        }
        let slot_of = |kind| scope.kinds().iter().position(|&k| k == kind);
        // A pennant's last four pivots are also a symmetrical triangle, and a narrowing flag's
        // a wedge; only the flag or pennant is kept.
        let pole = found.iter().flatten().any(|c| {
            matches!(
                c.kind,
                FormationKind::BullFlag
                    | FormationKind::BearFlag
                    | FormationKind::BullPennant
                    | FormationKind::BearPennant
            ) && slot_of(c.kind).is_some_and(|slot| enabled[slot])
        });
        for candidate in found.into_iter().flatten() {
            if pole
                && matches!(
                    candidate.kind,
                    FormationKind::SymmetricalTriangle
                        | FormationKind::RisingWedge
                        | FormationKind::FallingWedge
                )
                && candidate.anchors.len() == 4
            {
                continue;
            }
            // Wedges and the other family's formations are not this study's.
            let Some(slot) = slot_of(candidate.kind) else {
                continue;
            };
            let (first, last) = (
                candidate.anchors[0].position,
                candidate.anchors.last().unwrap().position,
            );
            if !enabled[slot] || self.used[slot].is_some_and(|used| first <= used) {
                continue;
            }
            self.used[slot] = Some(last);
            self.pending.push(candidate);
            if self.pending.len() > limit {
                self.pending.remove(0);
            }
        }
    }
}

/// Automatic chart patterns: a [`PatternFamily`], every formation with [`all`](Self::all),
/// or one formation registered under its label, with a toggle per kind for multiple formations,
/// detected from swing pivots and marked on the bar that confirms them. Read the formations,
/// with their anchors, boundaries, direction and confirmation bar, from
/// [`formations`](Self::formations).
///
/// # Rules
///
/// A swing pivot is a bar whose high (low) is greater (lower) than those of the `strength`
/// bars before it and at least (at most) those of the `strength` bars after it. It is known
/// `strength` bars later, and consecutive pivots of one kind keep the more extreme. A
/// formation is built from the last pivots, so it exists only once its last pivot is known.
///
/// A line through two pivots is projected forward. A formation is confirmed on the first bar
/// from the one that completes it onward whose close lies beyond its breakout boundary, and
/// is dropped if the close first crosses the opposite guard or the formation is older than
/// `max_length` bars. A bar with a non-finite price is skipped as if absent, and lengths
/// count the other bars. Every rule reads bars up to the confirmation bar only, so results
/// on a prefix of the history equal those of the full history up to that bar.
///
/// | Formation | Pivots | Rule |
/// |---|---|---|
/// | Bull flag | low, high, low, high, low | pole from the first to the second rises at least `pole` percent; the highs and the lows after the pole fall along nearly parallel lines (the end width within a quarter of the start width); both lows stay above the pole's middle |
/// | Bear flag | mirror | the pole falls and the flag rises |
/// | Bull pennant | as the bull flag | the highs fall, the lows rise and the end width is at most three quarters of the start width |
/// | Bear pennant | as the bear flag | the same converging lines |
/// | Rectangle | `2 * touches` alternating pivots | each boundary through its first and last pivot is flat, all its pivots within `tolerance` percent of it, and the height at least twice the tolerance |
/// | Ascending triangle | as the rectangle | flat upper boundary, rising lower boundary |
/// | Descending triangle | as the rectangle | falling upper boundary, flat lower boundary |
/// | Symmetrical triangle | as the rectangle | falling upper and rising lower boundary |
/// | Rising wedge | as the rectangle | neither boundary flat, both rise and the lower more steeply; confirms bearish below the lower boundary, fails above the upper |
/// | Falling wedge | as the rectangle | neither boundary flat, both fall and the upper more steeply; confirms bullish above the upper boundary, fails below the lower |
///
/// | Cup and handle | high, low, high, low | the rims differ by at most `tolerance` percent of their mean; the cup is at least `depth` percent deep below the mean rim, lasts between `min_length`, but at least 20, and `max_length` bars and has its low in the middle half of that time; its base is rounded, with at least half the bars between the rims low within a third of the depth above the cup's low; the handle pulls back from the right rim by at most `handle` percent of the cup depth over at most the cup's duration; confirms bullish above the higher rim, fails below the cup's low |
/// | Inverted cup and handle | mirror | confirms bearish below the lower rim |
/// | Double top | high, low, high | the tops differ by at most `tolerance` percent; at least `min_length` bars apart; the valley at least `depth` percent below them; the neckline is the valley's low; confirms bearish below it, fails above the higher top by more than `tolerance` percent |
/// | Double bottom | mirror | the neckline is the valley's high; confirms bullish above it |
/// | Triple top | high, low, high, low, high | as the double top for all three tops and both valleys; the neckline is the lower valley |
/// | Triple bottom | mirror | the neckline is the higher valley |
/// | Head and shoulders | high, low, high, low, high | the shoulders differ by at most `tolerance` percent, the head is at least `depth` percent above each, each valley lies below its shoulder, the shoulders are between `min_length` and `max_length` bars apart; the neckline runs through the valleys and may slope; confirms bearish below it, fails above the head |
/// | Inverse head and shoulders | mirror | confirms bullish above the neckline |
///
/// A boundary is flat when it moves at most `tolerance` percent between the first and last
/// pivot. A flag or pennant spans at least `min_length` bars from its pole's top and at most
/// `max_length` bars from the pole's start; the others span between `min_length` and
/// `max_length` bars from their first pivot. A bull flag, bull pennant and ascending triangle
/// confirm bullish above their upper boundary; a bear flag, bear pennant and descending
/// triangle confirm bearish below their lower boundary. A
/// rectangle and a symmetrical triangle confirm in the direction of the break. A pennant's
/// last four pivots also form a symmetrical triangle, and a narrowing flag's a wedge; neither is
/// then reported. A triple top or bottom follows the double top or bottom of its first two tops.
/// Each pivot belongs to at most one formation of a kind. At most 16 pivots, 8 developing and 64
/// confirmed formations are kept, twice as many formations for every formation at once.
/// Wedges here follow these shared pivot, length and tolerance rules; the standalone
/// [`RisingWedge`](crate::studies::RisingWedge) and [`FallingWedge`](crate::studies::FallingWedge)
/// studies use their own touch and convergence rules and can find different wedges.
///
/// # Outputs
///
/// Line 0 marks a bullish confirmation at the low of its bar, line 1 a bearish one at the
/// high, each labelled with the abbreviations of the formations it confirms. Line 2,
/// Developing, holds no values. Boundaries are also generated as drawings, each in the colour,
/// width and pattern of its plot and hidden with it: Bullish or Bearish once its formation is
/// confirmed, Developing, grey and dashed by default, until then. A reversal formation draws
/// only its neckline or breakout level. Hovering a mark or a boundary shows the formation's
/// name, such as "Bear Flag".
#[derive(Clone, Debug)]
pub struct ChartPatterns {
    scope: Scope,
    config: Config,
    enabled: [bool; 18],
    /// One bit per kind of the scope confirmed bullish and bearish on each bar.
    found: Vec<[u32; 2]>,
    values: IndicatorValues,
    stepper: Stepper<State>,
    #[cfg(feature = "drawings")]
    drawings: Vec<crate::drawings::Drawing>,
    /// The plot that styles each of `drawings`: 0 Bullish, 1 Bearish, 2 Developing.
    #[cfg(feature = "drawings")]
    drawing_plots: Vec<usize>,
    colors: [Color32; 3],
    visible: bool,
}

impl ChartPatterns {
    /// Detect every formation of `family` with its conventional parameters.
    pub fn new(family: PatternFamily) -> Self {
        Self::scoped(Scope::Family(family))
    }

    /// Detect every formation of both families and the rising and falling wedges, with one
    /// set of inputs and a toggle per formation; registered as All Chart Patterns.
    pub fn all() -> Self {
        Self::scoped(Scope::All)
    }

    /// Detect only `kind`, with the inputs and defaults of its family, under the name of the
    /// formation. Not for a wedge, whose name the Rising Wedge and Falling Wedge studies hold.
    pub(crate) fn only(kind: FormationKind) -> Self {
        Self::scoped(Scope::One(kind))
    }

    fn scoped(scope: Scope) -> Self {
        let family = match scope {
            Scope::Family(family) => Some(family),
            Scope::All => None,
            Scope::One(kind) if PatternFamily::Reversal.kinds().contains(&kind) => {
                Some(PatternFamily::Reversal)
            }
            Scope::One(_) => Some(PatternFamily::Continuation),
        };
        Self {
            scope,
            config: match family {
                None => Config {
                    strength: 3,
                    min_length: 8,
                    max_length: 200,
                    tolerance: 1.0,
                    touches: 2,
                    pole: 5.0,
                    depth: 3.0,
                    handle: 50.0,
                },
                Some(PatternFamily::Continuation) => Config {
                    strength: 3,
                    min_length: 8,
                    max_length: 120,
                    tolerance: 1.0,
                    touches: 2,
                    pole: 5.0,
                    depth: 3.0,
                    handle: 50.0,
                },
                Some(PatternFamily::Reversal) => Config {
                    strength: 3,
                    min_length: 10,
                    max_length: 200,
                    tolerance: 3.0,
                    touches: 2,
                    pole: 5.0,
                    depth: 3.0,
                    handle: 50.0,
                },
            },
            enabled: [true; 18],
            found: Vec::new(),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            #[cfg(feature = "drawings")]
            drawings: Vec::new(),
            #[cfg(feature = "drawings")]
            drawing_plots: Vec::new(),
            colors: [palette::BLUE, palette::RED, palette::GRAY],
            visible: true,
        }
    }

    /// Detect only the formations whose label, such as `"Bull Flag"`, `keep` accepts.
    pub fn with_patterns(mut self, keep: impl Fn(&str) -> bool) -> Self {
        for (on, kind) in self.enabled.iter_mut().zip(self.scope.kinds()) {
            *on = keep(kind.label());
        }
        self
    }

    /// Set the bars on each side of a pivot (at least 1), the shortest and longest formation in
    /// bars, the price tolerance in percent and the pivots per boundary (2 to 8). Bounds are
    /// clamped, NaN to the bottom.
    pub fn with_geometry(
        mut self,
        strength: usize,
        length: std::ops::RangeInclusive<usize>,
        tolerance: f64,
        touches: usize,
    ) -> Self {
        self.config.strength = strength.max(1);
        self.config.min_length = *length.start();
        self.config.max_length = (*length.end()).max(*length.start());
        self.config.tolerance = clamp_param(tolerance, 0.0, 100.0);
        self.config.touches = touches.clamp(2, MAX_PIVOTS / 2);
        self
    }

    /// The smallest rise or fall of a flag or pennant pole, in percent; NaN counts as zero.
    pub fn with_pole(mut self, percent: f64) -> Self {
        self.config.pole = clamp_param(percent, 0.0, f64::MAX);
        self
    }

    /// The smallest depth of a cup, of a valley between tops and of a head above its shoulders,
    /// in percent, and the largest pullback of a cup's handle, in percent of the cup's depth,
    /// each clamped into 0 to 100, NaN to 0.
    pub fn with_depth(mut self, depth: f64, handle: f64) -> Self {
        self.config.depth = clamp_param(depth, 0.0, 100.0);
        self.config.handle = clamp_param(handle, 0.0, 100.0);
        self
    }

    /// The confirmed formations, oldest first, then the developing ones, from the last
    /// calculation. Developing formations are re-evaluated on every bar.
    pub fn formations(&self) -> Vec<Formation> {
        let state = self.stepper.state();
        let end = (state.swings.position.saturating_sub(1), state.last);
        let mut formations = state.confirmed.clone();
        formations.extend(
            state
                .pending
                .iter()
                .map(|c| c.formation(c.kind.expected(), false, end)),
        );
        formations
    }

    /// The formations one line marks at a source bar, in the scope's order.
    fn marked(&self, line: usize, bar: usize) -> impl Iterator<Item = FormationKind> + '_ {
        let bits = self
            .found
            .get(bar)
            .and_then(|marks| marks.get(line))
            .copied()
            .unwrap_or(0);
        self.scope
            .kinds()
            .iter()
            .enumerate()
            .filter(move |(k, _)| bits >> k & 1 == 1)
            .map(|(_, &kind)| kind)
    }
}

impl Indicator for ChartPatterns {
    fn inputs(&self) -> Vec<StudyInput> {
        let defaults = Self::scoped(self.scope);
        let length = |id: &str, label: &str, value, default| {
            StudyInput::new(
                id,
                label,
                InputValue::Length(value),
                InputKind::Length(1, usize::MAX),
            )
            .with_default(InputValue::Length(default))
        };
        let number = |id: &str, label: &str, value, default| {
            StudyInput::new(
                id,
                label,
                InputValue::Number(value),
                InputKind::Number(0.0, 100.0),
            )
            .with_default(InputValue::Number(default))
        };
        let (c, d) = (&self.config, &defaults.config);
        let mut inputs = vec![
            length("strength", "Swing strength", c.strength, d.strength),
            length("min_length", "Minimum length", c.min_length, d.min_length),
            length("max_length", "Maximum length", c.max_length, d.max_length),
            number("tolerance", "Tolerance %", c.tolerance, d.tolerance),
        ];
        if self.scope.covers(PatternFamily::Continuation) {
            inputs.extend([
                StudyInput::new(
                    "touches",
                    "Touches per boundary",
                    InputValue::Length(c.touches),
                    InputKind::Length(2, MAX_PIVOTS / 2),
                )
                .with_default(InputValue::Length(d.touches)),
                StudyInput::new(
                    "pole",
                    "Pole %",
                    InputValue::Number(c.pole),
                    InputKind::Number(0.0, f64::MAX),
                )
                .with_default(InputValue::Number(d.pole)),
            ]);
        }
        if self.scope.covers(PatternFamily::Reversal) {
            inputs.extend([
                number("depth", "Depth %", c.depth, d.depth),
                number("handle", "Handle pullback %", c.handle, d.handle),
            ]);
        }
        // A study of one formation has no toggle to switch it off.
        let kinds = self.scope.kinds();
        if kinds.len() > 1 {
            inputs.extend(kinds.iter().enumerate().map(|(k, kind)| {
                StudyInput::new(
                    format!("patterns[{k}]"),
                    kind.label(),
                    InputValue::Toggle(self.enabled[k]),
                    InputKind::Toggle,
                )
                .with_default(InputValue::Toggle(true))
            }));
        }
        inputs
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), crate::studies::StudyError> {
        let schema = self
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .ok_or_else(|| crate::studies::StudyError::UnknownInput(id.into()))?;
        if !schema.accepts(&value) {
            return Err(crate::studies::StudyError::invalid_input(id));
        }
        match (id, value) {
            ("strength", InputValue::Length(v)) => self.config.strength = v,
            ("min_length", InputValue::Length(v)) => self.config.min_length = v,
            ("max_length", InputValue::Length(v)) => self.config.max_length = v,
            ("touches", InputValue::Length(v)) => self.config.touches = v,
            ("tolerance", InputValue::Number(v)) => self.config.tolerance = v,
            ("pole", InputValue::Number(v)) => self.config.pole = v,
            ("depth", InputValue::Number(v)) => self.config.depth = v,
            ("handle", InputValue::Number(v)) => self.config.handle = v,
            (_, InputValue::Toggle(on)) => {
                let slot = id
                    .strip_prefix("patterns[")
                    .and_then(|rest| rest.strip_suffix(']'))
                    .and_then(|k| k.parse::<usize>().ok())
                    .ok_or_else(|| crate::studies::StudyError::UnknownInput(id.into()))?;
                self.enabled[slot] = on;
            }
            _ => return Err(crate::studies::StudyError::UnknownInput(id.into())),
        }
        self.calculate(&[]);
        Ok(())
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line.min(2)],
            width: 1.0,
            pattern: if line >= 2 {
                crate::model::LineStyle::Dashed
            } else {
                crate::model::LineStyle::Solid
            },
            plot_type: match line {
                0 => crate::studies::PlotType::TriangleUp,
                1 => crate::studies::PlotType::TriangleDown,
                _ => crate::studies::PlotType::Line,
            },
            visible: true,
        }
    }

    fn plot_shape_side(&self, line: usize) -> crate::studies::ShapeSide {
        // A bullish confirmation marks its bar's low, a bearish one its high.
        match line {
            0 => crate::studies::ShapeSide::Below,
            1 => crate::studies::ShapeSide::Above,
            _ => crate::studies::ShapeSide::Centred,
        }
    }

    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        let text: Vec<&str> = self
            .marked(line, bar)
            .map(FormationKind::abbreviation)
            .collect();
        (!text.is_empty()).then(|| text.join("\n"))
    }

    fn plot_tooltip(&self, line: usize, bar: usize) -> Option<String> {
        let names: Vec<&str> = self.marked(line, bar).map(FormationKind::label).collect();
        (!names.is_empty()).then(|| names.join("\n"))
    }

    fn name(&self) -> &str {
        match self.scope {
            Scope::Family(family) => family.name(),
            Scope::All => "All Chart Patterns",
            Scope::One(kind) => kind.label(),
        }
    }

    fn desc(&self) -> &str {
        "Chart patterns found in swing pivots, confirmed by a close beyond their boundary"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let len = self.values.len();
        let start = if len > 0 && from >= len - 1 && data.len() >= len {
            from.min(len)
        } else {
            0
        };
        self.found.truncate(start);
        let (config, scope, enabled) = (self.config, self.scope, self.enabled);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let marks = state.advance(&config, scope, &enabled, bar);
                self.found.push(marks);
                if marks == [0, 0] {
                    return IndicatorValue::None;
                }
                IndicatorValue::Multiple(vec![
                    if marks[0] != 0 { bar.low } else { f64::NAN },
                    if marks[1] != 0 { bar.high } else { f64::NAN },
                    f64::NAN,
                ])
            });
        #[cfg(feature = "drawings")]
        {
            (self.drawings, self.drawing_plots) = self
                .formations()
                .iter()
                .flat_map(|formation| {
                    let confirmed = formation.status == FormationStatus::Confirmed;
                    let plot = match formation.direction {
                        Some(PatternDirection::Bullish) if confirmed => 0,
                        Some(PatternDirection::Bearish) if confirmed => 1,
                        _ => 2,
                    };
                    let color = self.colors[plot];
                    let name = formation.kind.label();
                    formation.boundaries.iter().map(move |segment| {
                        let mut drawing = crate::drawings::Drawing::new(
                            0,
                            crate::drawings::DrawingToolType::TrendLine,
                        );
                        drawing.completed = true;
                        drawing.stroke_width = 1.0;
                        drawing.label = Some(name.to_owned());
                        drawing.color = color.to_srgba_unmultiplied();
                        if !confirmed {
                            drawing.line_style = crate::model::LineStyle::Dashed;
                        }
                        drawing.chart_points = segment
                            .iter()
                            .map(|&(bar, price)| {
                                crate::drawings::ChartPoint::new(bar as f32, price)
                            })
                            .collect();
                        (drawing, plot)
                    })
                })
                .unzip();
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    #[cfg(feature = "drawings")]
    fn drawings(&self) -> &[crate::drawings::Drawing] {
        &self.drawings
    }

    #[cfg(feature = "drawings")]
    fn drawings_use_plot_style(&self) -> bool {
        false
    }

    #[cfg(feature = "drawings")]
    fn drawing_plot(&self, index: usize) -> Option<usize> {
        self.drawing_plots.get(index).copied()
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (slot, color) in self.colors.iter_mut().zip(colors) {
            *slot = color;
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_names(&self) -> Vec<String> {
        vec!["Bullish".into(), "Bearish".into(), "Developing".into()]
    }

    /// Developing styles boundaries only and has no value to read.
    fn legend_lines(&self) -> Vec<usize> {
        vec![0, 1]
    }

    // Developing only styles the boundaries of formations not yet confirmed.
    fn plot_holds_values(&self, line: usize) -> bool {
        line < 2
    }

    // Developing holds no values, and a side holds values only while a formation that confirms
    // that way is on: a one-sided formation fails on a break the other way.
    fn alert_outputs(&self) -> Vec<(usize, String)> {
        [PatternDirection::Bullish, PatternDirection::Bearish]
            .into_iter()
            .zip(self.line_names())
            .enumerate()
            .filter(|(_, (side, _))| {
                self.scope
                    .kinds()
                    .iter()
                    .zip(self.enabled)
                    .any(|(kind, on)| {
                        on && kind.expected().is_none_or(|expected| expected == *side)
                    })
            })
            .map(|(output, (_, name))| (output, name))
            .collect()
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    // Its drawings sit on source bars, which another symbol or interval does not place.
    fn draws_on_other_bars(&self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// Flat bars along straight legs of 5 bars between `anchors`, so every anchor is a pivot at
    /// a multiple of 5 (the first bar excepted) and is known 3 bars later.
    fn path(anchors: &[f64]) -> Vec<Bar> {
        let mut closes = vec![anchors[0]];
        for pair in anchors.windows(2) {
            closes.extend((1..=5).map(|i| pair[0] + (pair[1] - pair[0]) * f64::from(i) / 5.0));
        }
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                Bar::new(
                    Utc.timestamp_opt(i as i64 * 60, 0).unwrap(),
                    c,
                    c,
                    c,
                    c,
                    1.0,
                )
            })
            .collect()
    }

    /// A label, anchors, the formation looked for and its expected direction and confirmation bar.
    type Case = (
        &'static str,
        Vec<f64>,
        FormationKind,
        Option<(PatternDirection, usize)>,
    );

    fn mirror(anchors: &[f64]) -> Vec<f64> {
        anchors.iter().map(|a| 260.0 - a).collect()
    }

    fn found(study: &mut ChartPatterns, bars: &[Bar]) -> Vec<Formation> {
        study.calculate(bars);
        study.formations()
    }

    const BULL_FLAG: [f64; 7] = [105.0, 100.0, 130.0, 122.0, 127.0, 119.0, 135.0];
    const BULL_PENNANT: [f64; 7] = [105.0, 100.0, 130.0, 118.0, 125.0, 121.0, 135.0];
    const RECTANGLE: [f64; 6] = [105.0, 110.0, 100.0, 110.5, 100.5, 115.0];
    const ASCENDING: [f64; 6] = [105.0, 110.0, 100.0, 110.5, 103.0, 116.0];
    const SYMMETRICAL: [f64; 6] = [106.0, 112.0, 100.0, 109.0, 103.0, 114.0];

    fn continuation_cases() -> Vec<Case> {
        use FormationKind::*;
        use PatternDirection::*;
        let flat = |a: &[f64]| a.to_vec();
        #[rustfmt::skip]
        let cases = vec![
            ("bull flag", flat(&BULL_FLAG), BullFlag, Some((Bullish, 28))),
            ("bear flag", mirror(&BULL_FLAG), BearFlag, Some((Bearish, 28))),
            ("flag with widening lows", vec![105.0, 100.0, 130.0, 122.0, 127.0, 116.0, 135.0], BullFlag, None),
            ("flag with a small pole", vec![105.0, 100.0, 104.0, 102.8, 103.5, 102.3, 108.0], BullFlag, None),
            ("bull pennant", flat(&BULL_PENNANT), BullPennant, Some((Bullish, 28))),
            ("bear pennant", mirror(&BULL_PENNANT), BearPennant, Some((Bearish, 28))),
            ("pennant too wide", vec![105.0, 100.0, 130.0, 118.0, 129.0, 118.9, 135.0], BullPennant, None),
            ("rectangle up", flat(&RECTANGLE), Rectangle, Some((Bullish, 24))),
            ("rectangle down", mirror(&RECTANGLE), Rectangle, Some((Bearish, 24))),
            ("rectangle with unequal lows", vec![105.0, 110.0, 100.0, 110.5, 98.5, 115.0], Rectangle, None),
            ("ascending triangle", flat(&ASCENDING), AscendingTriangle, Some((Bullish, 24))),
            ("descending triangle", mirror(&ASCENDING), DescendingTriangle, Some((Bearish, 24))),
            ("rising channel", vec![105.0, 110.0, 100.0, 113.0, 103.0, 116.0], AscendingTriangle, None),
            ("symmetrical triangle", flat(&SYMMETRICAL), SymmetricalTriangle, Some((Bullish, 23))),
        ];
        cases
    }

    /// Each fixture confirms its formation on the hand-computed bar, or, as a near miss that
    /// breaks one rule, finds nothing: the flag's lows widen the channel, the pennant's lines
    /// stay too far apart, the rectangle's lows differ by more than the tolerance, and the
    /// triangles lack the flat or the converging side. Mirrors give the bearish kinds.
    #[test]
    fn continuation_fixtures_confirm_where_the_rules_say() {
        use FormationKind::*;
        use PatternDirection::*;
        for (label, anchors, kind, expected) in continuation_cases() {
            let formations = found(
                &mut ChartPatterns::new(PatternFamily::Continuation),
                &path(&anchors),
            );
            let got: Vec<_> = formations
                .iter()
                .filter(|f| f.kind == kind && f.status == FormationStatus::Confirmed)
                .map(|f| (f.direction.unwrap(), f.confirmed_at.unwrap()))
                .collect();
            assert_eq!(got, expected.into_iter().collect::<Vec<_>>(), "{label}");
        }

        // The small-pole flag is found once its 4 percent pole is enough.
        let small = [105.0, 100.0, 104.0, 102.8, 103.5, 102.3, 108.0];
        let formations = found(
            &mut ChartPatterns::new(PatternFamily::Continuation).with_pole(3.0),
            &path(&small),
        );
        assert_eq!(
            formations.iter().map(|f| f.kind).collect::<Vec<_>>(),
            [BullFlag]
        );

        // A formation's toggle is an input: with the bull flag off, none is found.
        let mut off = ChartPatterns::new(PatternFamily::Continuation);
        off.set_input("patterns[0]", InputValue::Toggle(false))
            .unwrap();
        let formations = found(&mut off, &path(&BULL_FLAG));
        assert!(formations.iter().all(|f| f.kind != BullFlag));

        // A swing strength at the input's upper bound finds no pivot instead of overflowing.
        let mut wide = ChartPatterns::new(PatternFamily::Continuation);
        wide.set_input("strength", InputValue::Length(usize::MAX))
            .unwrap();
        assert!(found(&mut wide, &path(&BULL_FLAG)).is_empty());

        // A symmetrical triangle whose last low is followed by a bounce and a break down
        // confirms bearish, on the first close under its lower line.
        let down = [106.0, 112.0, 100.0, 109.0, 103.0, 105.5, 95.0];
        let formations = found(
            &mut ChartPatterns::new(PatternFamily::Continuation),
            &path(&down),
        );
        assert_eq!(
            formations
                .iter()
                .filter(|f| f.kind == SymmetricalTriangle)
                .map(|f| (f.direction, f.confirmed_at))
                .collect::<Vec<_>>(),
            [(Some(Bearish), Some(26))]
        );
    }

    /// The bull flag: pole 100 to 130, falling highs 130 and 127 and lows 122 and 119 on lines
    /// of slope -0.3 (so 6.5 apart throughout), the close passing the upper line
    /// first on bar 27 but the last pivot only becoming known on bar 28, where it confirms.
    /// A flag still inside its lines is developing, drawn dashed, with a boundary end on the
    /// last bar.
    #[test]
    fn a_confirmed_flag_reports_its_anchors_boundaries_and_confirmation_bar() {
        let mut study = ChartPatterns::new(PatternFamily::Continuation);
        let bars = path(&BULL_FLAG);
        let formations = found(&mut study, &bars);
        assert_eq!(
            formations,
            [Formation {
                kind: FormationKind::BullFlag,
                status: FormationStatus::Confirmed,
                direction: Some(PatternDirection::Bullish),
                anchors: vec![
                    (5, 100.0),
                    (10, 130.0),
                    (15, 122.0),
                    (20, 127.0),
                    (25, 119.0)
                ],
                boundaries: vec![[(10, 130.0), (28, 124.6)], [(10, 123.5), (28, 118.1)]],
                confirmed_at: Some(28),
            }]
        );
        assert_eq!(study.plot_text(0, 28).as_deref(), Some("BF"));
        assert_eq!(study.values().value(0, 28), Some(bars[28].low));
        assert_eq!(study.plot_text(0, 27), None);

        // Cut after the pivot is known and before any break: the same flag is developing.
        let inside = found(&mut study, &bars[..27]);
        assert_eq!(inside, []);
        let mut held = BULL_FLAG;
        held[6] = 122.0;
        let developing = found(&mut study, &path(&held)[..31]);
        assert_eq!(developing.len(), 1);
        assert_eq!(developing[0].status, FormationStatus::Developing);
        assert_eq!(developing[0].direction, Some(PatternDirection::Bullish));
        assert_eq!(developing[0].confirmed_at, None);
        assert_eq!(developing[0].boundaries[0][1].0, 30);
    }

    const DOUBLE_TOP: [f64; 5] = [100.0, 120.0, 105.0, 119.0, 60.0];
    const TRIPLE_TOP: [f64; 7] = [100.0, 120.0, 105.0, 119.5, 105.5, 119.0, 60.0];
    const HEAD_AND_SHOULDERS: [f64; 7] = [100.0, 110.0, 100.0, 120.0, 101.0, 111.0, 80.0];
    const CUP: [f64; 8] = [110.0, 120.0, 104.0, 100.0, 104.0, 119.0, 114.0, 130.0];
    const V_CUP: [f64; 8] = [110.0, 120.0, 110.0, 100.0, 109.5, 119.0, 114.0, 130.0];
    const NARROW_CUP: [f64; 7] = [110.0, 120.0, 101.0, 100.0, 119.0, 114.0, 130.0];

    fn reversal_cases() -> Vec<Case> {
        use FormationKind::*;
        use PatternDirection::*;
        let flat = |a: &[f64]| a.to_vec();
        #[rustfmt::skip]
        let cases = vec![
            ("double top", flat(&DOUBLE_TOP), DoubleTop, Some((Bearish, 18))),
            ("double bottom", mirror(&DOUBLE_TOP), DoubleBottom, Some((Bullish, 18))),
            ("double top with unequal tops", vec![100.0, 120.0, 105.0, 115.0, 60.0], DoubleTop, None),
            ("double top with a shallow valley", vec![100.0, 120.0, 118.0, 119.5, 60.0], DoubleTop, None),
            ("triple top", flat(&TRIPLE_TOP), TripleTop, Some((Bearish, 28))),
            ("triple bottom", mirror(&TRIPLE_TOP), TripleBottom, Some((Bullish, 28))),
            ("triple top with a low third top", vec![100.0, 120.0, 105.0, 119.5, 105.5, 112.0, 60.0], TripleTop, None),
            ("head and shoulders", flat(&HEAD_AND_SHOULDERS), HeadAndShoulders, Some((Bearish, 28))),
            ("inverse head and shoulders", mirror(&HEAD_AND_SHOULDERS), InverseHeadAndShoulders, Some((Bullish, 28))),
            ("head too low", vec![100.0, 110.0, 100.0, 112.0, 101.0, 111.0, 80.0], HeadAndShoulders, None),
            ("unequal shoulders", vec![100.0, 110.0, 100.0, 125.0, 101.0, 116.0, 80.0], HeadAndShoulders, None),
            ("cup and handle", flat(&CUP), CupAndHandle, Some((Bullish, 33))),
            ("inverted cup and handle", mirror(&CUP), InvertedCupAndHandle, Some((Bearish, 33))),
            ("cup with a handle longer than half the cup", vec![110.0, 120.0, 104.0, 100.0, 104.0, 119.0, 117.0, 115.5, 114.0, 130.0], CupAndHandle, Some((Bullish, 43))),
            ("V-shaped cup", V_CUP.to_vec(), CupAndHandle, None),
            ("V-shaped inverted cup", mirror(&V_CUP), InvertedCupAndHandle, None),
            ("cup of 15 bars", NARROW_CUP.to_vec(), CupAndHandle, None),
            ("inverted cup of 15 bars", mirror(&NARROW_CUP), InvertedCupAndHandle, None),
            ("handle too deep", vec![110.0, 120.0, 104.0, 100.0, 104.0, 119.0, 105.0, 130.0], CupAndHandle, None),
            ("unequal rims", vec![110.0, 120.0, 104.0, 100.0, 104.0, 112.0, 108.0, 130.0], CupAndHandle, None),
            ("cup without a break", vec![110.0, 120.0, 104.0, 100.0, 104.0, 119.0, 114.0, 116.0], CupAndHandle, None),
            ("double top that recovers", vec![100.0, 120.0, 105.0, 119.0, 125.0], DoubleTop, None),
            ("head and shoulders that recovers", vec![100.0, 110.0, 100.0, 120.0, 101.0, 111.0, 125.0], HeadAndShoulders, None),
            ("double top with no break", vec![100.0, 120.0, 105.0, 119.0, 112.0], DoubleTop, None),
            ("cup with a break back down", vec![110.0, 120.0, 104.0, 100.0, 104.0, 119.0, 114.0, 116.0, 95.0, 130.0], CupAndHandle, None),
        ];
        cases
    }

    /// Each reversal fixture confirms on the hand-computed bar, often later than the first
    /// close beyond the neckline because the last pivot is known only three bars on, or, as a
    /// near miss breaking one rule, does not: unequal tops, a shallow valley, a third top
    /// that is too low, a head too close to its shoulders, shoulders of unequal height, a
    /// handle that gives back more than half the cup, a V-shaped cup and a cup of 15 bars; a
    /// handle longer than half the cup still confirms. Mirrors give the other direction.
    #[test]
    fn reversal_fixtures_confirm_where_the_rules_say() {
        use FormationKind::*;
        use PatternDirection::*;
        for (label, anchors, kind, expected) in reversal_cases() {
            let formations = found(
                &mut ChartPatterns::new(PatternFamily::Reversal),
                &path(&anchors),
            );
            let got: Vec<_> = formations
                .iter()
                .filter(|f| f.kind == kind && f.status == FormationStatus::Confirmed)
                .map(|f| (f.direction.unwrap(), f.confirmed_at.unwrap()))
                .collect();
            assert_eq!(got, expected.into_iter().collect::<Vec<_>>(), "{label}");
        }

        // The double top without a break is developing, with only its neckline as boundary,
        // ending on the last bar.
        let formations = found(
            &mut ChartPatterns::new(PatternFamily::Reversal),
            &path(&[100.0, 120.0, 105.0, 119.0, 112.0]),
        );
        let double: Vec<_> = formations.iter().filter(|f| f.kind == DoubleTop).collect();
        assert_eq!(double.len(), 1);
        assert_eq!(double[0].status, FormationStatus::Developing);
        assert_eq!(double[0].direction, Some(Bearish));
        assert_eq!(double[0].anchors, [(5, 120.0), (10, 105.0), (15, 119.0)]);
        assert_eq!(double[0].boundaries, [[(5, 105.0), (20, 105.0)]]);

        // A head and shoulders with a sloping neckline through (10, 100) and (20, 101) breaks
        // it where the line is, 101.7 on bar 27, but is known only on bar 28.
        let formations = found(
            &mut ChartPatterns::new(PatternFamily::Reversal),
            &path(&HEAD_AND_SHOULDERS),
        );
        let shoulders = formations
            .iter()
            .find(|f| f.kind == HeadAndShoulders)
            .unwrap();
        assert_eq!(shoulders.boundaries.len(), 1);
        let [(from, start), (to, end)] = shoulders.boundaries[0];
        assert_eq!((from, to), (10, 28));
        assert!((start - 100.0).abs() < 1e-9 && (end - 101.8).abs() < 1e-9);

        // With a minimum separation longer than the tops have, the same double top is not one.
        let apart = found(
            &mut ChartPatterns::new(PatternFamily::Reversal).with_geometry(3, 12..=200, 3.0, 2),
            &path(&DOUBLE_TOP),
        );
        assert!(apart.iter().all(|f| f.kind != DoubleTop));
    }

    /// All scopes preserve family confirmations, independent toggles and template identity;
    /// the wedge's slopes 0.4 and 0.6 put its lower break on bar 26, before the double top.
    #[test]
    fn all_chart_patterns_match_the_families_and_keep_their_toggles() {
        use crate::studies::{ConfiguredStudy, IndicatorFactory, StudyTemplate};
        use FormationKind::*;
        use PatternDirection::*;

        let confirmed = |study: &mut ChartPatterns, bars: &[Bar]| -> Vec<Formation> {
            found(study, bars)
                .into_iter()
                .filter(|f| f.status == FormationStatus::Confirmed)
                .collect()
        };
        for (family, cases) in [
            (PatternFamily::Continuation, continuation_cases()),
            (PatternFamily::Reversal, reversal_cases()),
        ] {
            for (label, anchors, _, _) in cases {
                let mut one_family = ChartPatterns::new(family);
                let mut all = ChartPatterns::all();
                for input in one_family.inputs() {
                    if !input.id.starts_with("patterns[") {
                        all.set_input(&input.id, input.value).unwrap();
                    }
                }
                let bars = path(&anchors);
                let together: Vec<_> = confirmed(&mut all, &bars)
                    .into_iter()
                    .filter(|f| family.kinds().contains(&f.kind))
                    .collect();
                assert_eq!(together, confirmed(&mut one_family, &bars), "{label}");
            }
        }

        const RISING_WEDGE: [f64; 7] = [100.0, 110.0, 102.0, 114.0, 108.0, 113.0, 90.0];
        let bars = path(&RISING_WEDGE);
        let mut all = ChartPatterns::all();
        let formations = confirmed(&mut all, &bars);
        let signature = |formations: &[Formation]| -> Vec<_> {
            formations
                .iter()
                .map(|f| (f.kind, f.direction.unwrap(), f.confirmed_at.unwrap()))
                .collect()
        };
        // The later upper line moves 1.5 across bars 10..25, beyond the 1% flatness bound.
        assert_eq!(
            signature(&formations),
            [
                (RisingWedge, Bearish, 26),
                (SymmetricalTriangle, Bearish, 28),
                (DoubleTop, Bearish, 28)
            ]
        );
        assert_eq!(
            formations[0].anchors,
            [(5, 110.0), (10, 102.0), (15, 114.0), (20, 108.0)]
        );
        let expected = [[(5, 110.0), (26, 118.4)], [(5, 99.0), (26, 111.6)]];
        assert_eq!(formations[0].boundaries.len(), expected.len());
        for (actual, expected) in formations[0].boundaries.iter().zip(expected) {
            for (&(bar, price), (expected_bar, expected_price)) in actual.iter().zip(expected) {
                assert_eq!(bar, expected_bar);
                assert!((price - expected_price).abs() < 1e-9);
            }
        }
        assert_eq!(all.plot_text(1, 26).as_deref(), Some("RW"));
        assert_eq!(all.plot_tooltip(1, 26).as_deref(), Some("Rising Wedge"));
        assert_eq!(all.values().value(1, 26), Some(bars[26].high));
        assert_eq!(all.plot_text(1, 28).as_deref(), Some("STR\nDT"));
        assert_eq!(
            signature(&confirmed(
                &mut ChartPatterns::all(),
                &path(&mirror(&RISING_WEDGE))
            )),
            [
                (FallingWedge, Bullish, 26),
                (SymmetricalTriangle, Bullish, 28),
                (DoubleBottom, Bullish, 28)
            ]
        );
        // A parallel rising channel breaking down is no rising wedge.
        assert!(
            confirmed(
                &mut ChartPatterns::all(),
                &path(&[105.0, 110.0, 100.0, 113.0, 103.0, 106.0, 90.0])
            )
            .iter()
            .all(|f| f.kind != RisingWedge)
        );
        // A narrowing flag's four pivots also make a falling wedge, reported only without the flag.
        let narrowing = path(&[105.0, 100.0, 130.0, 122.0, 127.0, 120.0, 135.0]);
        let kinds = |study: &mut ChartPatterns| -> Vec<_> {
            confirmed(study, &narrowing)
                .iter()
                .map(|f| f.kind)
                .collect()
        };
        assert_eq!(kinds(&mut ChartPatterns::all()), [BullFlag]);
        assert_eq!(
            kinds(&mut ChartPatterns::all().with_patterns(|label| label != "Bull Flag")),
            [FallingWedge]
        );
        for family in PatternFamily::ALL {
            assert!(
                confirmed(&mut ChartPatterns::new(family), &bars)
                    .iter()
                    .all(|f| !matches!(f.kind, RisingWedge | FallingWedge))
            );
        }
        let toggles: Vec<_> = all
            .inputs()
            .into_iter()
            .filter(|input| input.id.starts_with("patterns["))
            .collect();
        assert_eq!(toggles.len(), 18);
        for input in toggles {
            assert_eq!(input.value, InputValue::Toggle(true));
            let mut off = ChartPatterns::all();
            off.set_input(&input.id, InputValue::Toggle(false)).unwrap();
            let expected: Vec<_> = formations
                .iter()
                .filter(|f| f.kind.label() != input.label)
                .cloned()
                .collect();
            assert_eq!(confirmed(&mut off, &bars), expected, "{}", input.label);
        }

        all.set_input("patterns[16]", InputValue::Toggle(false))
            .unwrap();
        all.set_input("tolerance", InputValue::Number(2.0)).unwrap();
        let all = ConfiguredStudy::new(Box::new(all));
        let mut restored = StudyTemplate::from_study(&all).create().unwrap();
        assert_eq!(restored.name(), "All Chart Patterns");
        assert_eq!(
            StudyTemplate::from_study(restored.as_ref()).inputs,
            StudyTemplate::from_study(&all).inputs
        );
        restored.calculate(&bars);
        assert_eq!(restored.plot_text(1, 26), None);
        assert_eq!(restored.plot_text(1, 28).as_deref(), Some("DT"));

        let factory = IndicatorFactory::new();
        for (name, extra) in [
            ("Double Top", ["depth", "handle"]),
            ("Bull Flag", ["touches", "pole"]),
        ] {
            let single = factory.create(name).unwrap();
            assert_eq!(
                single
                    .inputs()
                    .iter()
                    .map(|input| input.id.as_str())
                    .collect::<Vec<_>>(),
                [
                    "strength",
                    "min_length",
                    "max_length",
                    "tolerance",
                    extra[0],
                    extra[1]
                ]
            );
            let single = ConfiguredStudy::new(single);
            let mut restored = StudyTemplate::from_study(&single).create().unwrap();
            assert_eq!(restored.name(), name);
            assert_eq!(
                StudyTemplate::from_study(restored.as_ref()).inputs,
                StudyTemplate::from_study(&single).inputs
            );
            let (anchors, line, bar, text): (&[f64], _, _, _) = if name == "Double Top" {
                (&DOUBLE_TOP, 1, 18, "DT")
            } else {
                (&BULL_FLAG, 0, 28, "BF")
            };
            restored.calculate(&path(anchors));
            assert_eq!(restored.plot_text(line, bar).as_deref(), Some(text));
        }
    }

    /// Replaying any prefix of the bars finds the confirmed formations the full history
    /// confirmed by then and no others, and updating bar by bar equals a fresh calculation.
    #[test]
    fn prefixes_and_incremental_updates_agree_with_the_full_history() {
        use PatternFamily::{Continuation, Reversal};
        for (family, anchors) in [
            (Continuation, BULL_FLAG.to_vec()),
            (Continuation, RECTANGLE.to_vec()),
            (Continuation, mirror(&ASCENDING)),
            (Reversal, TRIPLE_TOP.to_vec()),
            (Reversal, HEAD_AND_SHOULDERS.to_vec()),
            (Reversal, mirror(&CUP)),
        ] {
            let bars = path(&anchors);
            let full = found(&mut ChartPatterns::new(family), &bars);
            assert!(!full.is_empty());
            let mut live = ChartPatterns::new(family);
            for k in 1..=bars.len() {
                let confirmed = |formations: Vec<Formation>| -> Vec<Formation> {
                    formations
                        .into_iter()
                        .filter(|f| f.status == FormationStatus::Confirmed)
                        .collect()
                };
                let replay = confirmed(found(&mut ChartPatterns::new(family), &bars[..k]));
                let expected: Vec<_> = confirmed(full.clone())
                    .into_iter()
                    .filter(|f| f.confirmed_at.unwrap() < k)
                    .collect();
                assert_eq!(replay, expected, "prefix {k}");
                live.update(&bars[..k], k - 1);
                let fresh = found(&mut ChartPatterns::new(family), &bars[..k]);
                assert_eq!(live.formations(), fresh, "update at {k}");
            }
        }
    }
}
