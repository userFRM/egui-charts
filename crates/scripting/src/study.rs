//! A compiled script as a [`CustomIndicator`]: its declarations become the study's inputs,
//! plots, fills and data requests, and its calculation resumes from the last bars away from the
//! UI thread.

use crate::runtime::data::{self, History, Patch};
use crate::runtime::drawings::{self, Drawings};
use crate::runtime::program::DrawingCalls;
use crate::runtime::worker::{Poll, Runner};
use crate::runtime::{Outcome, Request, RunState};
use crate::{Script, ScriptError};
use ecolor::Color32;
use egui_charts::model::Bar;
use egui_charts::studies::{
    Calculation, CustomIndicator, IndicatorMarker, IndicatorValues, InputValue, Resume, ResumeRun,
    StudyData, StudyFill, StudyInput, StudyLabel, StudySegment, StudyZone, palette,
};
use std::time::Duration;

/// The colour of a plot that names none.
const PLOT_COLOR: Color32 = palette::BLUE;

/// The inputs `script` declares, at their defaults.
pub(crate) fn inputs(script: &Script) -> Vec<StudyInput> {
    let spec = &script.compiled.spec;
    spec.inputs
        .iter()
        .map(|input| {
            StudyInput::new(
                input.id.clone(),
                input.label.clone(),
                input.default.clone(),
                input.kind.clone(),
            )
        })
        .collect()
}

/// The study of `script` with its default inputs.
pub(crate) fn build(script: &Script) -> CustomIndicator {
    let spec = &script.compiled.spec;
    let inputs = self::inputs(script);
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
    if !spec.histories.is_empty() {
        let spec = spec.clone();
        study = study
            .with_data_requests(move |inputs| {
                let mut requests = Vec::new();
                for request in data::requests(&spec, inputs) {
                    if !requests.contains(&request) {
                        requests.push(request);
                    }
                }
                requests
            })
            .with_wait_for_close();
    }
    study
}

/// Calculate `script` on `bars` with its default inputs, natively on a thread whose stack fits
/// the deepest scripts, which the caller's may not.
pub(crate) fn check(script: &Script, bars: &[Bar]) -> Result<(), ScriptError> {
    #[cfg(not(target_arch = "wasm32"))]
    let result = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("script check".into())
            .stack_size(crate::runtime::worker::STACK)
            .spawn_scoped(scope, || calculate(script, bars))
            .ok()
            .and_then(|thread| thread.join().ok())
            .unwrap_or_else(|| crate::runtime::lost().result.map(|_| ()))
    });
    #[cfg(target_arch = "wasm32")]
    let result = calculate(script, bars);
    result
}

/// Calculate `script` on `bars` with its default inputs on this thread.
fn calculate(script: &Script, bars: &[Bar]) -> Result<(), ScriptError> {
    let compiled = &script.compiled;
    let mut state = RunState::new(
        compiled.clone(),
        compiled.spec.clone(),
        compiled.series_spans.clone(),
        compiled.history_spans.clone(),
        script.limits,
        None,
    );
    state.begin(Request {
        from: 0,
        bars: bars.to_vec(),
        fed: None,
        histories: Vec::new(),
        inputs: compiled
            .spec
            .inputs
            .iter()
            .map(|input| input.default.clone())
            .collect(),
        reset: true,
        profile: false,
        decision: None,
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
    /// The bars and fed column as last sent to the runner, the histories of its declarations and
    /// the inputs.
    bars: Vec<Bar>,
    fed: Option<Vec<f64>>,
    histories: Vec<Supplied>,
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
    /// The alert conditions of the bars calculated, one row of every condition per bar: NaN where
    /// no call stored one, else 1 or 0.
    conditions: Vec<f64>,
    /// The colours of the bars calculated, one row of every plot coloured by an expression per
    /// bar, as [`drawings::encode`] writes them.
    colors: Vec<f64>,
    /// The drawings of the bars calculated.
    drawings: Drawings,
}

/// A history as the chart supplied it last.
#[derive(Default)]
struct Supplied {
    history: Option<History>,
    /// The first of its bars changed since the latest run request.
    changed: Option<usize>,
}

impl Supplied {
    /// Take `new` in place of the history supplied before, and return the first bar of `chart`
    /// that may read it otherwise, if one may.
    fn replace(&mut self, new: Option<&StudyData<'_>>, chart: &[Bar]) -> Option<usize> {
        let old = self.history.as_ref();
        let changed = match (old, new) {
            (None, None) => return None,
            (Some(old), Some(new))
                if old.clock == new.clock
                    && old.timeframe == new.timeframe
                    && old.chart == new.chart =>
            {
                first_change(&old.bars, new.bars, 0, same_bar)?
            }
            _ => 0,
        };
        let data = old.map(History::data);
        let affected = data::first_affected(data.as_ref(), new, changed, chart);
        let bars = self.history.take().map_or_else(Vec::new, |old| old.bars);
        self.history = new.map(|new| {
            let mut bars = bars;
            bars.truncate(changed);
            bars.extend_from_slice(&new.bars[changed..]);
            History {
                bars,
                clock: new.clock.clone(),
                timeframe: new.timeframe,
                chart: new.chart.clone(),
            }
        });
        self.changed = Some(self.changed.map_or(changed, |first| first.min(changed)));
        Some(affected)
    }

    /// The change since the latest run request, the whole history with `reset`.
    fn patch(&mut self, reset: bool) -> Option<Patch> {
        let changed = self.changed.take();
        let from = if reset { 0 } else { changed? };
        Some(Patch {
            from,
            history: self.history.as_ref().map(|history| History {
                bars: history.bars[from..].to_vec(),
                clock: history.clock.clone(),
                timeframe: history.timeframe,
                chart: history.chart.clone(),
            }),
        })
    }
}

impl ScriptState {
    fn new(script: &Script) -> Self {
        let histories = script.compiled.spec.histories.len();
        Self {
            script: script.clone(),
            runner: None,
            bars: Vec::new(),
            fed: None,
            histories: std::iter::repeat_with(Supplied::default)
                .take(histories)
                .collect(),
            inputs: None,
            changed: None,
            running: false,
            reset: true,
            last: None,
            conditions: Vec::new(),
            colors: Vec::new(),
            drawings: Drawings::default(),
        }
    }

    /// Record `run`'s changes since the bars sent last: the first bar that differs, trusting
    /// `run.data[..run.from]` unless `from` is 0, and whether the inputs differ. A history changes
    /// with a `from` of 0, but for its end, which a replay moves; any other call may leave it out.
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
        let all = run.from == 0 || self.inputs.is_none();
        let requests = data::requests(&self.script.compiled.spec, run.inputs);
        for (request, supplied) in requests.iter().zip(&mut self.histories) {
            // The chart supplies the available histories of all its studies.
            let new = run.histories.iter().find(|data| data.request == *request);
            let seen = supplied.history.as_ref().map(|seen| seen.bars.len());
            if all || new.is_some_and(|new| seen != Some(new.bars.len())) {
                let affected = supplied.replace(new, run.data);
                first = first.into_iter().chain(affected).min();
            }
        }
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
                let spec = &self.script.compiled.spec;
                let (plots, alerts) = (spec.plots.len(), spec.alerts.len());
                self.conditions.truncate(outcome.start * alerts);
                self.colors.truncate(outcome.start * spec.coloured());
                let width = spec.width();
                // A script without a plot, as a strategy may be, still keeps a row per bar.
                for bar in 0..outcome.bars {
                    let row = &outcome.rows[bar * width..][..width];
                    values.push_lines(&row[..plots]);
                    self.conditions
                        .extend_from_slice(&row[plots..plots + alerts]);
                    self.colors.extend_from_slice(&row[plots + alerts..]);
                }
                self.drawings.replace(outcome.start, outcome.drawn);
                match outcome.error {
                    Some(error) => *last_error = Some(error),
                    // A study built on no bars, as a layout check builds one, measures nothing:
                    // it leaves the error and the profile of the script's studies as they are.
                    None if outcome.start == 0 && !self.bars.is_empty() => {
                        *last_error = None;
                        if outcome.profile.is_some() {
                            let profile = self.script.profile.lock();
                            *profile.unwrap_or_else(|e| e.into_inner()) = outcome.profile;
                        }
                    }
                    None => {}
                }
                Calculation::Done(took)
            }
            Err(error) => {
                // The run keeps nothing, so the next one starts from bar 0.
                self.reset = true;
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
    /// the study it belongs to keeps drawing the values cloned with it until then, and answers
    /// its alert conditions, colours and drawings from them.
    fn clone(&self) -> Self {
        Self {
            conditions: self.conditions.clone(),
            colors: self.colors.clone(),
            drawings: self.drawings.clone(),
            ..Self::new(&self.script)
        }
    }
}

impl Resume for ScriptState {
    fn resume(&mut self, run: ResumeRun<'_>, values: &mut IndicatorValues) -> Calculation {
        let inputs_changed = self.record(&run);
        let runner = self.runner.get_or_insert_with(|| {
            let compiled = &self.script.compiled;
            Runner::new(
                "script study",
                compiled.clone(),
                compiled.spec.clone(),
                compiled.series_spans.clone(),
                compiled.history_spans.clone(),
                self.script.limits,
            )
        });
        if self.running {
            match runner.poll() {
                Poll::Running(done) => return Calculation::Running(done),
                Poll::Done(outcome) => {
                    let calculation = self.install(*outcome, values);
                    if self.changed.is_none() {
                        return calculation;
                    }
                }
            }
        }
        // A stopped study runs again only when its inputs change or its bars change from the
        // first one on, not on every tick. Such a change recorded while the stopped run was in
        // flight starts its run in this call, since the chart stops asking once a study stops.
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
            histories: (self.histories.iter_mut())
                .map(|supplied| supplied.patch(self.reset))
                .collect(),
            inputs: self.inputs.clone().unwrap_or_default(),
            reset: self.reset,
            profile: self.script.profiling,
            decision: None,
        };
        self.reset = false;
        self.running = true;
        let runner = self.runner.as_mut().expect("the runner was started above");
        match runner.send(request) {
            Poll::Running(done) => Calculation::Running(done),
            Poll::Done(outcome) => self.install(*outcome, values),
        }
    }

    fn alert_conditions(&self) -> Vec<String> {
        self.script.compiled.spec.alerts.clone()
    }

    fn alert_condition(&self, condition: usize, bar: usize) -> Option<bool> {
        let alerts = self.script.compiled.spec.alerts.len();
        if condition >= alerts {
            return None;
        }
        let value = *self
            .conditions
            .get(bar.checked_mul(alerts)?.checked_add(condition)?)?;
        (!value.is_nan()).then_some(value != 0.0)
    }

    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        let spec = &self.script.compiled.spec;
        let (_, column) = spec.plots.get(line)?.dynamic?;
        let at = bar.checked_mul(spec.coloured())?.checked_add(column)?;
        drawings::decode(*self.colors.get(at)?)
    }

    fn labels(&self) -> &[StudyLabel] {
        self.drawings.labels()
    }

    fn price_segments(&self) -> &[StudySegment] {
        self.drawings.segments()
    }

    fn zones(&self) -> &[StudyZone] {
        self.drawings.zones()
    }

    fn markers(&self) -> &[IndicatorMarker] {
        self.drawings.markers()
    }

    fn draws_only_plots(&self) -> bool {
        self.script.compiled.spec.calls == DrawingCalls::default()
    }

    fn draws_on_other_bars(&self) -> bool {
        let calls = self.script.compiled.spec.calls;
        !calls.labels && !calls.segments
    }
}

/// The first bar from which `seen` and `data` differ, given that `data[..from]` is unchanged
/// unless `from` is 0, or `None` when they are the same.
pub(crate) fn first_change<T>(
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
pub(crate) fn same_bar(a: &Bar, b: &Bar) -> bool {
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
