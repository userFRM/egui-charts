//! A compiled script as a [`Strategy`]: its inputs become the strategy's, and each decision runs
//! the script on the bar that just closed and places the orders it called.

use crate::runtime::Request;
use crate::runtime::limits::{Clock, SLICE, Stop};
use crate::runtime::orders::{OrderCall, Readings};
use crate::runtime::program::Span;
use crate::runtime::worker::Runner;
use crate::study::{first_change, same_bar};
use crate::{Script, ScriptError};
use egui_charts::model::{Bar, OrderType};
use egui_charts::strategies::{Strategy, StrategyContext};
use egui_charts::studies::{InputValue, StudyInput};
use std::time::Duration;
use web_time::Instant;

/// A script as a [`Strategy`] for the backtester, from [`Script::strategy`].
///
/// On each decision the script runs on the bar that just closed, with the bars before it as its
/// history, and its order calls are placed on the context in the order it made them. Every
/// decision on a bar, such as one after a fill, runs the whole script on that bar again from the
/// state before it, and the last one sets the states the next bar starts from. A runtime error or
/// a time or memory limit ends the strategy's orders from that bar until the next run: the
/// backtester cancels its working orders, so none fills later, and withholds the run's report
/// with the error as its reason, through [`Strategy::failure`], and [`Script::last_error`]
/// reports it at its line and column. Natively the script runs on a thread
/// of its own, as a study's does; on the web on the caller's.
pub struct ScriptStrategy {
    script: Script,
    inputs: Vec<StudyInput>,
    /// Started by the first decision; natively a worker thread.
    runner: Option<Runner>,
    /// The bars as the last decision sent them.
    bars: Vec<Bar>,
    /// Whether the next decision starts over from bar 0.
    reset: bool,
    /// The error or stop that ended this run's orders.
    failure: Option<ScriptError>,
    /// The calculation time of this run's decisions, against `Limits::full`.
    spent: Duration,
    /// The time of the decisions since `should_yield` last answered `true`.
    since_yield: Duration,
}

impl std::fmt::Debug for ScriptStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptStrategy")
            .field("name", &self.script.name())
            .finish_non_exhaustive()
    }
}

impl ScriptStrategy {
    pub(crate) fn new(script: &Script) -> Self {
        Self {
            script: script.clone(),
            inputs: crate::study::inputs(script),
            runner: None,
            bars: Vec::new(),
            reset: true,
            failure: None,
            spent: Duration::ZERO,
            since_yield: Duration::ZERO,
        }
    }

    /// End this run's orders with `error`, which the script reports as its last.
    fn fail(&mut self, error: ScriptError) {
        let last_error = self.script.last_error.lock();
        *last_error.unwrap_or_else(|e| e.into_inner()) = Some(error.clone());
        self.failure = Some(error);
    }
}

impl Strategy for ScriptStrategy {
    fn study_inputs(&self) -> Vec<StudyInput> {
        self.inputs.clone()
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        let input = (self.inputs.iter_mut())
            .find(|input| input.id == id)
            .ok_or("unknown strategy input")?;
        if !input.accepts(&value) {
            return Err("strategy input is outside its accepted values");
        }
        input.value = value;
        self.reset = true;
        Ok(())
    }

    fn validate_inputs(&self) -> Result<(), &'static str> {
        if self.script.compiled.spec.histories.is_empty() {
            Ok(())
        } else {
            Err("a strategy script cannot read other histories")
        }
    }

    fn prepare(&mut self, _bars: &[Bar]) {
        self.reset = true;
        self.bars.clear();
        self.failure = None;
        self.spent = Duration::ZERO;
        self.since_yield = Duration::ZERO;
        let last_error = self.script.last_error.lock();
        *last_error.unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Does nothing: each decision finds the bars that changed since the last one, and
    /// preparing again would reset the states the script keeps.
    fn prepare_update(&mut self, _bars: &[Bar]) {}

    fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
        let bars = context.bars();
        if bars.is_empty() || self.failure.is_some() {
            return;
        }
        let last = bars.len() - 1;
        let limits = self.script.limits;
        if self.spent >= limits.full {
            let message = Clock::start(limits).message(Stop::Full);
            return self.fail(ScriptError::at(message, Span::default(), Some(last)));
        }
        // Within a run the backtester appends bars or changes the last, so the last two bars
        // sent tell what changed. An unchanged history reruns the last bar.
        let from = if self.reset {
            0
        } else {
            let start = self.bars.len().saturating_sub(2);
            first_change(&self.bars, bars, start, same_bar)
                .unwrap_or(bars.len())
                .min(last)
        };
        let compiled = &self.script.compiled;
        let runner = self.runner.get_or_insert_with(|| {
            Runner::new(
                "script strategy",
                compiled.clone(),
                compiled.spec.clone(),
                compiled.series_spans.clone(),
                compiled.history_spans.clone(),
                limits,
            )
        });
        let request = Request {
            from,
            bars: bars[from..].to_vec(),
            fed: None,
            histories: Vec::new(),
            inputs: self
                .inputs
                .iter()
                .map(|input| input.value.clone())
                .collect(),
            reset: self.reset,
            profile: false,
            decision: Some(Readings {
                position: context.position(),
                equity: context.equity(),
                price: context.price(),
                after_fill: context.is_after_fill(),
            }),
        };
        let started = Instant::now();
        let outcome = runner.decide(request);
        self.since_yield += started.elapsed();
        self.reset = false;
        self.bars.truncate(from);
        self.bars.extend_from_slice(&bars[from..]);
        match outcome.result {
            Ok(took) => self.spent += took,
            Err(stop) => return self.fail(stop),
        }
        if let Some(error) = outcome.error {
            return self.fail(error);
        }
        for call in outcome.orders {
            match call {
                OrderCall::Entry {
                    id,
                    side,
                    qty,
                    limit,
                    stop,
                } => {
                    let order = match (limit, stop) {
                        (None, None) => OrderType::Market,
                        (Some(limit), None) => OrderType::Limit(limit),
                        (None, Some(stop)) => OrderType::Stop(stop),
                        (Some(limit), Some(stop)) => OrderType::StopLimit { stop, limit },
                    };
                    context.entry(id, side, qty, order);
                }
                OrderCall::Exit {
                    id,
                    qty,
                    profit,
                    loss,
                } => context.exit(id, qty, profit, loss),
                OrderCall::Relative {
                    id,
                    side,
                    profit,
                    loss,
                } => context.exit_relative(id, side, profit, loss),
                OrderCall::Cancel { id } => context.cancel(&id),
            }
        }
    }

    fn should_yield(&mut self) -> bool {
        let due = self.since_yield >= SLICE;
        if due {
            self.since_yield = Duration::ZERO;
        }
        due
    }

    fn failure(&self) -> Option<String> {
        self.failure.as_ref().map(ToString::to_string)
    }
}
