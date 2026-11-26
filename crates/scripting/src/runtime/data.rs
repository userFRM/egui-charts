//! The other histories a script reads with `bars`: what its study requests for them, and which of
//! their bars each bar of the chart reads.

use super::program::{ScriptSpec, TextRef};
use egui_charts::model::{Bar, BarClock, Timeframe};
use egui_charts::studies::{InputValue, StudyChart, StudyData, StudyDataRequest, StudyInput};

/// A history as the chart supplied it.
#[derive(Clone, Debug)]
pub(crate) struct History {
    pub bars: Vec<Bar>,
    pub clock: BarClock,
    pub timeframe: Timeframe,
    /// The chart bars it is read on.
    pub chart: Option<StudyChart>,
}

impl History {
    /// The history as the chart's studies read it, for [`StudyData::closed_at`].
    pub fn data(&self) -> StudyData<'_> {
        StudyData {
            request: StudyDataRequest {
                symbol: String::new(),
                timeframe: None,
                metric: None,
            },
            bars: &self.bars,
            clock: self.clock.clone(),
            instrument: None,
            timeframe: self.timeframe,
            chart: self.chart.clone(),
        }
    }
}

/// A change of a history since the previous request: its bars from `from`, or `None` when the
/// chart no longer supplies it.
#[derive(Clone, Debug)]
pub(crate) struct Patch {
    pub from: usize,
    pub history: Option<History>,
}

impl Patch {
    /// Make the change to `history`.
    pub fn apply(self, history: &mut Option<History>) {
        let seen = history.take();
        *history = self.history.map(|mut changed| {
            let mut bars = seen.map_or_else(Vec::new, |seen| seen.bars);
            bars.truncate(self.from);
            bars.append(&mut changed.bars);
            changed.bars = bars;
            changed
        });
    }
}

/// What the study requests for each history of `spec` under `inputs`.
pub(crate) fn requests(spec: &ScriptSpec, inputs: &[StudyInput]) -> Vec<StudyDataRequest> {
    let input = |at: usize| inputs.get(at).map(|input| &input.value);
    spec.histories
        .iter()
        .map(|history| StudyDataRequest {
            symbol: match &history.symbol {
                TextRef::Literal(symbol) => symbol.clone(),
                TextRef::Input(at) => match input(*at) {
                    Some(InputValue::Symbol(symbol)) => symbol.clone(),
                    _ => String::new(),
                },
            },
            timeframe: match &history.interval {
                TextRef::Literal(text) => interval(text).flatten(),
                TextRef::Input(at) => match input(*at) {
                    Some(InputValue::Timeframe(timeframe)) => *timeframe,
                    _ => None,
                },
            },
            metric: None,
        })
        .collect()
}

/// The interval `text` names, `Some(None)` for the chart's when it is empty, or `None` when it
/// names none.
pub(crate) fn interval(text: &str) -> Option<Option<Timeframe>> {
    if text.is_empty() {
        Some(None)
    } else {
        text.parse().ok().map(Some)
    }
}

/// The first bar of `chart` that may read a history otherwise once `new` replaces `old`, which
/// differ from their bar `changed` on; the last bar, which reads the newest bars, at the latest.
pub(crate) fn first_affected(
    old: Option<&StudyData<'_>>,
    new: Option<&StudyData<'_>>,
    changed: usize,
    chart: &[Bar],
) -> usize {
    let first = |history: Option<&StudyData<'_>>, index: usize| {
        history.map_or(chart.len(), |history| {
            chart.partition_point(|bar| {
                history
                    .closed_at(bar.time, false)
                    .is_none_or(|at| at < index)
            })
        })
    };
    // The bar before the change is the same in both, but one without a scheduled close closes
    // once a later bar opens.
    let before = changed.saturating_sub(1);
    let (closes, closed) = (first(new, before), first(old, before));
    let affected = if closes == closed {
        first(new, changed).min(first(old, changed))
    } else {
        closes.min(closed)
    };
    affected.min(chart.len().saturating_sub(1))
}
