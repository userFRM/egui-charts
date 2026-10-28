//! Automatic chart patterns found in confirmed swing pivots.
//!
//! [`ChartPatterns`] detects continuation formations (flags, pennants, rectangles and
//! triangles) or reversal formations (cup and handle, double and triple tops and bottoms,
//! head and shoulders) in a bar history, and returns each one's anchors, boundary segments,
//! direction and confirmation time through [`ChartPatterns::formations`]. Developing formations are
//! told apart from confirmed ones, and every confirmed formation is marked on the bar that
//! confirms it.
//!
//! # Rules
//!
//! A swing pivot is a bar whose high (low) is greater (lower) than those of the `strength`
//! bars before it and at least (at most) those of the `strength` bars after it. It is known
//! `strength` bars later, and consecutive pivots of one kind keep the more extreme. A
//! formation is built from the last pivots, so it exists only once its last pivot is known.
//!
//! A line through two pivots is projected forward. A formation is confirmed on the first bar
//! from the one that completes it onward whose close lies beyond its breakout boundary, and
//! is dropped if the close first crosses the opposite guard or the formation is older than
//! `max_length` bars. A bar with a non-finite price is skipped as if absent, and lengths
//! count the other bars. Every rule reads bars up to the confirmation bar only, so results
//! on a prefix of the history equal those of the full history up to that bar.
//!
//! | Formation | Pivots | Rule |
//! |---|---|---|
//! | Bull flag | low, high, low, high, low | pole from the first to the second rises at least `pole` percent; the highs and the lows after the pole fall along nearly parallel lines (the end width within a quarter of the start width); both lows stay above the pole's middle |
//! | Bear flag | mirror | the pole falls and the flag rises |
//! | Bull pennant | as the bull flag | the highs fall, the lows rise and the end width is at most three quarters of the start width |
//! | Bear pennant | as the bear flag | the same converging lines |
//! | Rectangle | `2 * touches` alternating pivots | each boundary through its first and last pivot is flat, all its pivots within `tolerance` percent of it, and the height at least twice the tolerance |
//! | Ascending triangle | as the rectangle | flat upper boundary, rising lower boundary |
//! | Descending triangle | as the rectangle | falling upper boundary, flat lower boundary |
//! | Symmetrical triangle | as the rectangle | falling upper and rising lower boundary |
//!
//! | Cup and handle | high, low, high, low | the rims differ by at most `tolerance` percent of their mean; the cup is at least `depth` percent deep below the mean rim, lasts between `min_length` and `max_length` bars and has its low in the middle half of that time; the handle pulls back from the right rim by at most `handle` percent of the cup depth over at most half the cup's duration; confirms bullish above the higher rim, fails below the cup's low |
//! | Inverted cup and handle | mirror | confirms bearish below the lower rim |
//! | Double top | high, low, high | the tops differ by at most `tolerance` percent; at least `min_length` bars apart; the valley at least `depth` percent below them; the neckline is the valley's low; confirms bearish below it, fails above the higher top by more than `tolerance` percent |
//! | Double bottom | mirror | the neckline is the valley's high; confirms bullish above it |
//! | Triple top | high, low, high, low, high | as the double top for all three tops and both valleys; the neckline is the lower valley |
//! | Triple bottom | mirror | the neckline is the higher valley |
//! | Head and shoulders | high, low, high, low, high | the shoulders differ by at most `tolerance` percent, the head is at least `depth` percent above each, each valley lies below its shoulder, the shoulders are between `min_length` and `max_length` bars apart; the neckline runs through the valleys and may slope; confirms bearish below it, fails above the head |
//! | Inverse head and shoulders | mirror | confirms bullish above the neckline |
//!
//! A boundary is flat when it moves at most `tolerance` percent between the first and last
//! pivot. A flag or pennant's length is counted from its pole's top and the length of the
//! others from their first pivot, and lies between `min_length` and `max_length`. A bull flag, bull
//! pennant and ascending triangle confirm bullish above their upper boundary; a bear flag,
//! bear pennant and descending triangle confirm bearish below their lower boundary. A
//! rectangle and a symmetrical triangle confirm in the direction of the break. A pennant's
//! last four pivots also form a symmetrical triangle, which is then not reported, and a triple
//! top or bottom follows the double top or bottom of its first two tops. Each pivot
//! belongs to at most one formation of a kind. At most 16 pivots, 8 developing and 64
//! confirmed formations are kept.
//!
//! # Outputs
//!
//! Line 0 marks a bullish confirmation at the low of its bar, line 1 a bearish one at the
//! high, each labelled with the abbreviations of the formations it confirms. Boundaries are
//! also generated as drawings, dashed while developing; a reversal formation draws only its
//! neckline or breakout level.

use super::clamp_param;
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

/// The formation a detector looks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

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
            | Self::InverseHeadAndShoulders => Some(PatternDirection::Bullish),
            Self::BearFlag
            | Self::BearPennant
            | Self::DescendingTriangle
            | Self::InvertedCupAndHandle
            | Self::DoubleTop
            | Self::TripleTop
            | Self::HeadAndShoulders => Some(PatternDirection::Bearish),
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

    fn kinds(self) -> &'static [FormationKind] {
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
pub enum FormationStatus {
    /// All its pivots are known but no close has broken its boundary yet.
    Developing,
    /// A close broke its boundary, on bar [`Formation::confirmed_at`].
    Confirmed,
}

/// A detected formation, with bar indices into the calculated bars.
#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Copy)]
struct Pivot {
    /// Position among the finite bars.
    seq: usize,
    /// Index in the calculated bars.
    bar: usize,
    price: f64,
    high: bool,
}

/// A straight line in position and price.
#[derive(Clone, Copy)]
struct Line {
    seq: f64,
    price: f64,
    slope: f64,
}

impl Line {
    fn through(a: &Pivot, b: &Pivot) -> Self {
        Self {
            seq: a.seq as f64,
            price: a.price,
            slope: (b.price - a.price) / (b.seq as f64 - a.seq as f64),
        }
    }

    fn at(&self, seq: usize) -> f64 {
        self.price + self.slope * (seq as f64 - self.seq)
    }
}

#[derive(Clone, Copy)]
enum Outcome {
    Confirm(PatternDirection),
    Fail,
}

/// A formation whose pivots are known, waiting for a close beyond a boundary.
#[derive(Clone)]
struct Candidate {
    kind: FormationKind,
    anchors: Vec<Pivot>,
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
        if seq - self.anchors[0].seq > max_length {
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
            anchors: self.anchors.iter().map(|p| (p.bar, p.price)).collect(),
            boundaries: [self.upper, self.lower]
                .iter()
                .zip(self.shown)
                .filter(|&(_, shown)| shown)
                .map(|(line, _)| [(start.bar, line.at(start.seq)), (end.1, line.at(end.0))])
                .collect(),
            confirmed_at: confirmed.then_some(end.1),
        }
    }
}

/// `a` and `b` differ by at most `tolerance` percent of their mean.
fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance / 100.0 * (a.abs() + b.abs()) / 2.0
}

fn last_pivots(pivots: &VecDeque<Pivot>, count: usize) -> Option<Vec<Pivot>> {
    (pivots.len() >= count).then(|| pivots.iter().skip(pivots.len() - count).copied().collect())
}

/// A flag or pennant: a pole from the first pivot to the second, then lines through the
/// second and fourth pivots and through the third and fifth.
fn flag(config: &Config, pivots: &VecDeque<Pivot>, bull: bool, pennant: bool) -> Option<Candidate> {
    let w = last_pivots(pivots, 5)?;
    if w[0].high == bull || (w[1].price > w[0].price) != bull {
        return None;
    }
    let pole = (w[1].price - w[0].price).abs() / w[0].price.abs() * 100.0;
    let length = w[4].seq - w[1].seq;
    if pole < config.pole || length < config.min_length || w[4].seq - w[0].seq > config.max_length {
        return None;
    }
    let (a, b) = (Line::through(&w[1], &w[3]), Line::through(&w[2], &w[4]));
    let (upper, lower) = if bull { (a, b) } else { (b, a) };
    let width = |seq| upper.at(seq) - lower.at(seq);
    let (start, end) = (width(w[1].seq), width(w[4].seq));
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

/// A rectangle or triangle through the last `2 * touches` pivots.
fn channel(config: &Config, pivots: &VecDeque<Pivot>) -> Option<Candidate> {
    let w = last_pivots(pivots, 2 * config.touches)?;
    let side =
        |high: bool| -> Vec<Pivot> { w.iter().filter(|p| p.high == high).copied().collect() };
    let (highs, lows) = (side(true), side(false));
    let upper = Line::through(&highs[0], highs.last()?);
    let lower = Line::through(&lows[0], lows.last()?);
    let on_line = |pivots: &[Pivot], line: Line| {
        pivots
            .iter()
            .all(|p| near(p.price, line.at(p.seq), config.tolerance))
    };
    let (first, last) = (w[0].seq, w[w.len() - 1].seq);
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
        on_up: if kind == FormationKind::DescendingTriangle {
            Outcome::Fail
        } else {
            up
        },
        on_down: if kind == FormationKind::AscendingTriangle {
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
    pivots: &VecDeque<Pivot>,
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
        .all(|p| p[2].seq - p[0].seq >= config.min_length);
    // A valley this far below (above) a top (bottom).
    let deep = |valley: f64| (mean - valley) * if top { 1.0 } else { -1.0 } / mean.abs() * 100.0;
    if !near(high, low, config.tolerance)
        || !apart
        || w[w.len() - 1].seq - w[0].seq > config.max_length
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
fn head_and_shoulders(config: &Config, pivots: &VecDeque<Pivot>, top: bool) -> Option<Candidate> {
    let w = last_pivots(pivots, 5)?;
    if w[0].high != top {
        return None;
    }
    let side = if top { 1.0 } else { -1.0 };
    let (left, first, head, second, right) = (w[0], w[1], w[2], w[3], w[4]);
    let above = |shoulder: &Pivot| {
        side * (head.price - shoulder.price) / shoulder.price.abs() * 100.0 >= config.depth
    };
    if !near(left.price, right.price, config.tolerance)
        || !above(&left)
        || !above(&right)
        || side * first.price >= side * left.price
        || side * second.price >= side * right.price
        || !(config.min_length..=config.max_length).contains(&(right.seq - left.seq))
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
/// extreme between them and the handle's.
fn cup(config: &Config, pivots: &VecDeque<Pivot>, bull: bool) -> Option<Candidate> {
    let w = last_pivots(pivots, 4)?;
    if w[0].high != bull {
        return None;
    }
    let side = if bull { 1.0 } else { -1.0 };
    let (left, base, right, handle) = (w[0], w[1], w[2], w[3]);
    let mean = (left.price + right.price) / 2.0;
    let depth = side * (mean - base.price);
    let duration = right.seq - left.seq;
    let pullback = side * (right.price - handle.price);
    if !near(left.price, right.price, config.tolerance)
        || depth <= 0.0
        || depth / mean.abs() * 100.0 < config.depth
        || !(config.min_length..=config.max_length).contains(&duration)
        || handle.seq - left.seq > config.max_length
        || 4 * (base.seq - left.seq) < duration
        || 4 * (right.seq - base.seq) < duration
        || pullback <= 0.0
        || pullback > config.handle / 100.0 * depth
        || 2 * (handle.seq - right.seq) > duration
    {
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
#[derive(Clone, Default)]
struct State {
    /// Bars seen, finite or not.
    index: usize,
    /// Finite bars seen.
    seq: usize,
    /// Index of the last finite bar.
    last: usize,
    recent: VecDeque<(usize, f64, f64)>,
    pivots: VecDeque<Pivot>,
    pending: Vec<Candidate>,
    confirmed: Vec<Formation>,
    /// Per kind, the last pivot of the latest formation found.
    used: [Option<usize>; 8],
}

impl State {
    fn advance(
        &mut self,
        config: &Config,
        family: PatternFamily,
        enabled: &[bool; 8],
        bar: &Bar,
    ) -> [u8; 2] {
        let index = self.index;
        self.index += 1;
        let mut marks = [0; 2];
        if ![bar.open, bar.high, bar.low, bar.close]
            .iter()
            .all(|v| v.is_finite())
        {
            return marks;
        }
        let seq = self.seq;
        self.seq += 1;
        self.last = index;
        let width = 2 * config.strength + 1;
        self.recent.push_back((index, bar.high, bar.low));
        if self.recent.len() > width {
            self.recent.pop_front();
        }
        if self.recent.len() == width && self.find_pivot(config, seq) {
            self.detect(config, family, enabled);
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
                    if self.confirmed.len() > MAX_CONFIRMED {
                        self.confirmed.remove(0);
                    }
                    let slot = family
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

    /// Adds the pivot at the middle of the recent bars, if there is one. Returns whether the
    /// pivots changed.
    fn find_pivot(&mut self, config: &Config, seq: usize) -> bool {
        let n = config.strength;
        let (bar, high, low) = self.recent[n];
        let others = |keep: fn(f64, f64) -> bool, pick: fn(&(usize, f64, f64)) -> f64, value| {
            self.recent
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != n)
                .all(|(j, b)| keep(value, pick(b)) || (j > n && value == pick(b)))
        };
        let is_high = others(|v, o| v > o, |b| b.1, high);
        let is_low = others(|v, o| v < o, |b| b.2, low);
        let pivot = match (is_high, is_low) {
            (true, false) => Pivot {
                seq: seq - n,
                bar,
                price: high,
                high: true,
            },
            (false, true) => Pivot {
                seq: seq - n,
                bar,
                price: low,
                high: false,
            },
            _ => return false,
        };
        match self.pivots.back_mut() {
            Some(last) if last.high == pivot.high => {
                if (pivot.price > last.price) == pivot.high && pivot.price != last.price {
                    *last = pivot;
                } else {
                    return false;
                }
            }
            _ => {
                self.pivots.push_back(pivot);
                if self.pivots.len() > MAX_PIVOTS {
                    self.pivots.pop_front();
                }
            }
        }
        true
    }

    fn detect(&mut self, config: &Config, family: PatternFamily, enabled: &[bool; 8]) {
        let pivots = &self.pivots;
        let found: Vec<Option<Candidate>> = match family {
            PatternFamily::Continuation => vec![
                flag(config, pivots, true, false),
                flag(config, pivots, false, false),
                flag(config, pivots, true, true),
                flag(config, pivots, false, true),
                channel(config, pivots),
            ],
            PatternFamily::Reversal => vec![
                cup(config, pivots, true),
                cup(config, pivots, false),
                multiple(config, pivots, true, false),
                multiple(config, pivots, false, false),
                multiple(config, pivots, true, true),
                multiple(config, pivots, false, true),
                head_and_shoulders(config, pivots, true),
                head_and_shoulders(config, pivots, false),
            ],
        };
        // A pennant's last four pivots are also a symmetrical triangle; only the pennant is kept.
        let pennant = found.iter().flatten().any(|c| {
            matches!(
                c.kind,
                FormationKind::BullPennant | FormationKind::BearPennant
            ) && enabled[family.kinds().iter().position(|&k| k == c.kind).unwrap()]
        });
        for candidate in found.into_iter().flatten() {
            if pennant
                && candidate.kind == FormationKind::SymmetricalTriangle
                && candidate.anchors.len() == 4
            {
                continue;
            }
            let slot = family
                .kinds()
                .iter()
                .position(|&k| k == candidate.kind)
                .unwrap();
            let (first, last) = (
                candidate.anchors[0].seq,
                candidate.anchors.last().unwrap().seq,
            );
            if !enabled[slot] || self.used[slot].is_some_and(|used| first <= used) {
                continue;
            }
            self.used[slot] = Some(last);
            self.pending.push(candidate);
            if self.pending.len() > MAX_PENDING {
                self.pending.remove(0);
            }
        }
    }
}

/// Automatic chart patterns: the formations of a [`PatternFamily`], each with a toggle,
/// detected from swing pivots and marked on the bar that confirms them.
///
/// The rules are in the [module documentation](self). Read the formations, with their
/// anchors, boundaries, direction and confirmation bar, from
/// [`formations`](Self::formations).
#[derive(Clone)]
pub struct ChartPatterns {
    family: PatternFamily,
    config: Config,
    enabled: [bool; 8],
    /// One bit per kind of the family confirmed bullish and bearish on each bar.
    found: Vec<[u8; 2]>,
    values: IndicatorValues,
    stepper: Stepper<State>,
    #[cfg(feature = "drawings")]
    drawings: Vec<crate::drawings::Drawing>,
    colors: [Color32; 3],
    visible: bool,
}

impl ChartPatterns {
    /// Detect every formation of `family` with its conventional parameters.
    pub fn new(family: PatternFamily) -> Self {
        Self {
            family,
            config: match family {
                PatternFamily::Continuation => Config {
                    strength: 3,
                    min_length: 8,
                    max_length: 120,
                    tolerance: 1.0,
                    touches: 2,
                    pole: 5.0,
                    depth: 3.0,
                    handle: 50.0,
                },
                PatternFamily::Reversal => Config {
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
            enabled: [true; 8],
            found: Vec::new(),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            #[cfg(feature = "drawings")]
            drawings: Vec::new(),
            colors: [palette::BLUE, palette::RED, palette::GRAY],
            visible: true,
        }
    }

    /// Detect only the formations whose label, such as `"Bull Flag"`, `keep` accepts.
    pub fn with_patterns(mut self, keep: impl Fn(&str) -> bool) -> Self {
        for (on, kind) in self.enabled.iter_mut().zip(self.family.kinds()) {
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
        let end = (state.seq.saturating_sub(1), state.last);
        let mut formations = state.confirmed.clone();
        formations.extend(
            state
                .pending
                .iter()
                .map(|c| c.formation(c.kind.expected(), false, end)),
        );
        formations
    }
}

impl Indicator for ChartPatterns {
    fn inputs(&self) -> Vec<StudyInput> {
        let defaults = Self::new(self.family);
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
        inputs.extend(match self.family {
            PatternFamily::Continuation => vec![
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
            ],
            PatternFamily::Reversal => vec![
                number("depth", "Depth %", c.depth, d.depth),
                number("handle", "Handle pullback %", c.handle, d.handle),
            ],
        });
        inputs.extend(self.family.kinds().iter().enumerate().map(|(k, kind)| {
            StudyInput::new(
                format!("patterns[{k}]"),
                kind.label(),
                InputValue::Toggle(self.enabled[k]),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(true))
        }));
        inputs
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        let schema = self
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .ok_or("unknown study input")?;
        if !schema.accepts(&value) {
            return Err("study input is outside its accepted values");
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
                    .ok_or("unknown study input")?;
                self.enabled[slot] = on;
            }
            _ => return Err("unknown study input"),
        }
        self.calculate(&[]);
        Ok(())
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line.min(1)],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: if line == 0 {
                crate::studies::PlotType::TriangleUp
            } else {
                crate::studies::PlotType::TriangleDown
            },
            visible: true,
        }
    }

    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        let bits = self.found.get(bar)?.get(line).copied()?;
        let text: Vec<&str> = self
            .family
            .kinds()
            .iter()
            .enumerate()
            .filter(|(k, _)| bits >> k & 1 == 1)
            .map(|(_, kind)| kind.abbreviation())
            .collect();
        (!text.is_empty()).then(|| text.join("\n"))
    }

    fn name(&self) -> &str {
        self.family.name()
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
        let (config, family, enabled) = (self.config, self.family, self.enabled);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let marks = state.advance(&config, family, &enabled, bar);
                self.found.push(marks);
                if marks == [0, 0] {
                    return IndicatorValue::None;
                }
                IndicatorValue::Multiple(vec![
                    if marks[0] != 0 { bar.low } else { f64::NAN },
                    if marks[1] != 0 { bar.high } else { f64::NAN },
                ])
            });
        #[cfg(feature = "drawings")]
        {
            self.drawings = self
                .formations()
                .iter()
                .flat_map(|formation| {
                    let confirmed = formation.status == FormationStatus::Confirmed;
                    let color = match formation.direction {
                        Some(PatternDirection::Bullish) if confirmed => self.colors[0],
                        Some(PatternDirection::Bearish) if confirmed => self.colors[1],
                        _ => self.colors[2],
                    };
                    formation.boundaries.iter().map(move |segment| {
                        let mut drawing = crate::drawings::Drawing::new(
                            0,
                            crate::drawings::DrawingToolType::TrendLine,
                        );
                        drawing.completed = true;
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
                        drawing
                    })
                })
                .collect();
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
        vec!["Bullish".into(), "Bearish".into()]
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
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

    /// Each fixture confirms its formation on the hand-computed bar, or, as a near miss that
    /// breaks one rule, finds nothing: the flag's lows widen the channel, the pennant's lines
    /// stay too far apart, the rectangle's lows differ by more than the tolerance, and the
    /// triangles lack the flat or the converging side. Mirrors give the bearish kinds.
    #[test]
    fn continuation_fixtures_confirm_where_the_rules_say() {
        use FormationKind::*;
        use PatternDirection::*;
        let flat = |a: &[f64]| a.to_vec();
        #[rustfmt::skip]
        let cases: [Case; 14] = [
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
        for (label, anchors, kind, expected) in cases {
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
    const CUP: [f64; 6] = [110.0, 120.0, 100.0, 119.0, 114.0, 130.0];

    /// Each reversal fixture confirms on the hand-computed bar, often later than the first
    /// close beyond the neckline because the last pivot is known only three bars on, or, as a
    /// near miss breaking one rule, does not: unequal tops, a shallow valley, a third top
    /// that is too low, a head too close to its shoulders, shoulders of unequal height, and a
    /// handle that gives back more than half the cup. Mirrors give the other direction.
    #[test]
    fn reversal_fixtures_confirm_where_the_rules_say() {
        use FormationKind::*;
        use PatternDirection::*;
        let flat = |a: &[f64]| a.to_vec();
        #[rustfmt::skip]
        let cases: [Case; 20] = [
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
            ("cup and handle", flat(&CUP), CupAndHandle, Some((Bullish, 23))),
            ("inverted cup and handle", mirror(&CUP), InvertedCupAndHandle, Some((Bearish, 23))),
            ("handle too deep", vec![110.0, 120.0, 100.0, 119.0, 105.0, 130.0], CupAndHandle, None),
            ("unequal rims", vec![110.0, 120.0, 100.0, 112.0, 108.0, 130.0], CupAndHandle, None),
            ("cup without a break", vec![110.0, 120.0, 100.0, 119.0, 114.0, 116.0], CupAndHandle, None),
            ("double top that recovers", vec![100.0, 120.0, 105.0, 119.0, 125.0], DoubleTop, None),
            ("head and shoulders that recovers", vec![100.0, 110.0, 100.0, 120.0, 101.0, 111.0, 125.0], HeadAndShoulders, None),
            ("double top with no break", vec![100.0, 120.0, 105.0, 119.0, 112.0], DoubleTop, None),
            ("cup with a break back down", vec![110.0, 120.0, 100.0, 119.0, 114.0, 95.0], CupAndHandle, None),
        ];
        for (label, anchors, kind, expected) in cases {
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
        let relaxed = found(
            &mut ChartPatterns::new(PatternFamily::Reversal).with_geometry(3, 12..=200, 3.0, 2),
            &path(&DOUBLE_TOP),
        );
        assert!(relaxed.iter().all(|f| f.kind != DoubleTop));
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
