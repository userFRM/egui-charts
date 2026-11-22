//! A compiled script as a [`CustomIndicator`]: its declarations become the study's inputs,
//! plots and fills, and its calculation resumes from the last bars away from the UI thread.

use crate::runtime::worker::{Poll, Runner};
use crate::runtime::{Outcome, Request, RunState};
use crate::{Script, ScriptError};
use ecolor::Color32;
use egui_charts::model::Bar;
use egui_charts::studies::{
    Calculation, CustomIndicator, IndicatorValues, InputValue, Resume, ResumeRun, StudyFill,
    StudyInput, palette,
};
use std::time::Duration;

/// The colour of a plot that names none.
const PLOT_COLOR: Color32 = palette::BLUE;

/// The study of `script` with its default inputs.
pub(crate) fn build(script: &Script) -> CustomIndicator {
    let spec = &script.compiled.spec;
    let inputs = spec
        .inputs
        .iter()
        .map(|input| {
            StudyInput::new(
                input.id.clone(),
                input.label.clone(),
                input.default.clone(),
                input.kind.clone(),
            )
        })
        .collect();
    let mut study = CustomIndicator::new(script.compiled.name.clone(), |_| Vec::new())
        .with_inputs(inputs, |_, _, _, _| Vec::new())
        .expect("inputs are checked when the script compiles")
        .with_overlay(spec.display.overlay)
        .with_line_count(spec.plots.len())
        .with_line_names(spec.plots.iter().map(|plot| plot.title.clone()).collect())
        .with_colors(
            spec.plots
                .iter()
                .map(|plot| plot.color.unwrap_or(PLOT_COLOR))
                .collect(),
        )
        .with_plot_types(spec.plots.iter().map(|plot| plot.style).collect())
        .with_offsets(spec.plots.iter().map(|plot| plot.offset).collect())
        .with_fills(
            spec.fills
                .iter()
                .map(|fill| StudyFill {
                    first: fill.first,
                    second: fill.second,
                    // A fill that names no colour takes the plot colour at a tenth of its opacity.
                    color: fill.color.unwrap_or(PLOT_COLOR.gamma_multiply(0.1)),
                    inverted_color: fill.inverted,
                })
                .collect(),
        )
        .with_resume(ScriptState::new(script));
    if let Some(format) = spec.display.format {
        study = study.with_value_format(format);
    }
    if let Some(scale) = spec.display.scale {
        study = study.with_scale(scale);
    }
    study
}

/// Calculate `script` on `bars` with its default inputs on this thread.
pub(crate) fn check(script: &Script, bars: &[Bar]) -> Result<(), ScriptError> {
    let compiled = &script.compiled;
    let mut state = RunState::new(
        compiled.clone(),
        compiled.spec.clone(),
        compiled.series_spans.clone(),
        script.limits,
        None,
    );
    state.begin(Request {
        from: 0,
        bars: bars.to_vec(),
        fed: None,
        inputs: compiled
            .spec
            .inputs
            .iter()
            .map(|input| input.default.clone())
            .collect(),
        reset: true,
    });
    let outcome = state.advance(None).expect("a run without a slice ends");
    match (outcome.result, outcome.error) {
        (Err(error), _) | (Ok(_), Some(error)) => Err(error),
        (Ok(_), None) => Ok(()),
    }
}

/// The calculation of one script study. Its run state lives with its [`Runner`]; this side
/// keeps the bars it sent, to find what changed since.
struct ScriptState {
    script: Script,
    runner: Option<Runner>,
    /// The bars and fed column as last sent to the runner, and the inputs.
    bars: Vec<Bar>,
    fed: Option<Vec<f64>>,
    inputs: Option<Vec<InputValue>>,
    /// The first bar changed since the bars of the latest run request.
    changed: Option<usize>,
    /// Whether a run is in progress.
    running: bool,
    /// Whether the next run starts from bar 0 whatever changed: after a stop, which keeps no
    /// state, and for a new study.
    reset: bool,
    /// The result of the latest run that ended.
    last: Option<Calculation>,
}

impl ScriptState {
    fn new(script: &Script) -> Self {
        Self {
            script: script.clone(),
            runner: None,
            bars: Vec::new(),
            fed: None,
            inputs: None,
            changed: None,
            running: false,
            reset: true,
            last: None,
        }
    }

    /// Record `run`'s changes since the bars sent last: the first bar that differs, trusting
    /// `run.data[..run.from]` unless `from` is 0, and whether the inputs differ.
    fn record(&mut self, run: &ResumeRun<'_>) -> bool {
        let inputs: Vec<_> = run.inputs.iter().map(|input| input.value.clone()).collect();
        let inputs_changed = self.inputs.as_ref() != Some(&inputs);
        let bars = first_change(&self.bars, run.data, run.from, same_bar);
        let fed = match (&self.fed, run.source) {
            (None, None) => None,
            (Some(seen), Some(source)) => {
                first_change(seen, source, run.from, |a, b| a.to_bits() == b.to_bits())
            }
            _ => Some(0),
        };
        let mut first = match (bars, fed) {
            (Some(bars), Some(fed)) => Some(bars.min(fed)),
            (bars, fed) => bars.or(fed),
        };
        if inputs_changed {
            first = Some(0);
            self.inputs = Some(inputs);
        }
        if let Some(first) = first {
            self.bars.truncate(first);
            self.bars.extend_from_slice(&run.data[first..]);
            self.fed = run.source.map(|source| {
                let mut fed = self.fed.take().unwrap_or_default();
                fed.truncate(first.min(fed.len()));
                fed.extend_from_slice(&source[first.min(source.len())..]);
                fed
            });
            self.changed = Some(self.changed.map_or(first, |changed| changed.min(first)));
        }
        inputs_changed
    }

    /// Apply a run's outcome to `values`.
    fn install(&mut self, outcome: Outcome, values: &mut IndicatorValues) -> Calculation {
        self.running = false;
        let mut last_error = self
            .script
            .last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let calculation = match outcome.result {
            Ok(took) => {
                values.truncate(outcome.start);
                let width = self.script.compiled.spec.plots.len();
                for row in outcome.rows.chunks(width) {
                    values.push_lines(row);
                }
                match outcome.error {
                    Some(error) => *last_error = Some(error),
                    None if outcome.start == 0 => *last_error = None,
                    None => {}
                }
                Calculation::Done(took)
            }
            Err(error) => {
                // The run keeps nothing, so the next one starts from bar 0.
                self.reset = true;
                self.changed = None;
                let mut message = error.message.clone();
                if error.line > 0 {
                    message += &format!(" at line {}", error.line);
                } else if let Some(bar) = error.bar {
                    message += &format!(" on bar {bar}");
                }
                *last_error = Some(error);
                Calculation::Stopped(message)
            }
        };
        self.last = Some(calculation.clone());
        calculation
    }
}

impl Clone for ScriptState {
    /// A clone keeps the script and starts its own calculation from bar 0 on its first call;
    /// the study it belongs to keeps drawing the values cloned with it until then.
    fn clone(&self) -> Self {
        Self::new(&self.script)
    }
}

impl Resume for ScriptState {
    fn resume(&mut self, run: ResumeRun<'_>, values: &mut IndicatorValues) -> Calculation {
        let inputs_changed = self.record(&run);
        let runner = self.runner.get_or_insert_with(|| {
            let compiled = &self.script.compiled;
            Runner::new(
                compiled.clone(),
                compiled.spec.clone(),
                compiled.series_spans.clone(),
                self.script.limits,
            )
        });
        if self.running {
            match runner.poll() {
                Poll::Running(done) => return Calculation::Running(done),
                Poll::Done(outcome) => {
                    let calculation = self.install(outcome, values);
                    if self.changed.is_none() || matches!(calculation, Calculation::Stopped(_)) {
                        return calculation;
                    }
                }
            }
        }
        // A stopped study runs again only when its inputs change or its bars change from the
        // first one on, not on every tick.
        if let Some(Calculation::Stopped(message)) = &self.last
            && !inputs_changed
            && self.changed != Some(0)
        {
            return Calculation::Stopped(message.clone());
        }
        let Some(changed) = self.changed.take() else {
            return self
                .last
                .clone()
                .unwrap_or(Calculation::Done(Duration::ZERO));
        };
        let from = if self.reset { 0 } else { changed };
        let request = Request {
            from,
            bars: self.bars[from..].to_vec(),
            fed: self
                .fed
                .as_ref()
                .map(|fed| fed[from.min(fed.len())..].to_vec()),
            inputs: self.inputs.clone().unwrap_or_default(),
            reset: self.reset,
        };
        self.reset = false;
        self.running = true;
        let runner = self.runner.as_mut().expect("the runner was started above");
        match runner.send(request) {
            Poll::Running(done) => Calculation::Running(done),
            Poll::Done(outcome) => self.install(outcome, values),
        }
    }
}

/// The first bar from which `seen` and `data` differ, given that `data[..from]` is unchanged
/// unless `from` is 0, or `None` when they are the same.
fn first_change<T>(
    seen: &[T],
    data: &[T],
    from: usize,
    same: impl Fn(&T, &T) -> bool,
) -> Option<usize> {
    let common = seen.len().min(data.len());
    let start = if from == 0 { 0 } else { from.min(common) };
    (start..common)
        .find(|&at| !same(&seen[at], &data[at]))
        .or_else(|| (seen.len() != data.len()).then_some(common))
}

/// Whether two bars hold the same time and prices, missing ones included.
fn same_bar(a: &Bar, b: &Bar) -> bool {
    a.time == b.time
        && [
            (a.open, b.open),
            (a.high, b.high),
            (a.low, b.low),
            (a.close, b.close),
            (a.volume, b.volume),
        ]
        .iter()
        .all(|(a, b)| a.to_bits() == b.to_bits())
}
