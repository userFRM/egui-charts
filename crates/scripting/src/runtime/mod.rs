//! The series runtime: the bar loop, the columns and helpers a run reads and writes, the state
//! kept between bars and runs, and the limits of 1 bar, a full recompute and memory. No type of
//! a front end appears here; a front end implements [`Program`] and reaches the run being
//! calculated through [`with`].

pub(crate) mod kernels;
pub(crate) mod limits;
pub(crate) mod program;
pub(crate) mod series;
pub(crate) mod state;
pub(crate) mod worker;

use crate::ScriptError;
use egui_charts::model::Bar;
use egui_charts::studies::InputValue;
use kernels::Kernel;
use limits::{Clock, Limits, Stop};
use program::{ScriptSpec, Span};
use series::Columns;
use state::Snapshot;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use web_time::Instant;

/// A compiled per-bar program, run with the [`Run`] it reads installed for [`with`].
pub(crate) trait Program: Send + Sync + 'static {
    /// The front end's state of one study: the values that carry from one bar to the next.
    type Slots: Clone + Send + Sync;

    /// Fresh slots for `inputs`, the states holding their initial values.
    fn slots(&self, inputs: &[InputValue], fed: bool) -> Self::Slots;

    /// Run the program on [`Run::bar`]; it stores plots in [`Run::row`].
    fn bar(&self, slots: &mut Self::Slots) -> Result<(), BarError>;
}

/// Why a bar failed and where; [`Run::stop`] says whether it was a limit.
#[derive(Clone, Debug)]
pub(crate) struct BarError {
    pub message: String,
    pub span: Span,
}

/// A streaming helper call site: its position and its occurrence in the bar, so a call inside a
/// function called from two places keeps one state per place.
pub(crate) type Site = (Span, u32);

/// A streaming helper's state at one call site.
#[derive(Clone, Debug)]
pub(crate) struct CallSite {
    pub kernel: Kernel,
    pub length: i64,
    /// The last bar the site was fed.
    pub last: usize,
}

/// What the per-bar functions of a front end read and write while a run is installed.
#[derive(Clone, Debug)]
pub(crate) struct Run {
    pub spec: Arc<ScriptSpec>,
    pub columns: Columns,
    /// The bar being calculated.
    pub bar: usize,
    /// The plot values of the current bar, missing until a plot stores one.
    pub row: Vec<f64>,
    /// The bytes each value kept between bars holds after the last bar, with where it is
    /// declared; the front end sets them after each bar.
    pub kept: Vec<(usize, Span)>,
    pub sites: HashMap<Site, CallSite>,
    occurrences: HashMap<Span, u32>,
    pub clock: Clock,
    /// The operations run so far.
    operations: u64,
    /// Whether the values of the running scope are due to be counted, once per clock read.
    due: bool,
    /// The bytes the values of the scope at each call level held at the last count there on this
    /// bar, and those of them outside the states, zeros for a level without one, the running level
    /// last. A caller's scope is out of reach while it waits, so the last count at its level stands
    /// for it, without what the caller bound after that count.
    levels: Vec<(usize, usize)>,
    /// The bytes the values of the running scope outside the states held when their count
    /// stopped the run, for its report.
    running: usize,
    /// The limit that stopped the run, set by the checks.
    pub stop: Option<Stop>,
    cancel: Option<Arc<AtomicBool>>,
    /// Where each `series` declaration is, for memory reports.
    series_spans: Arc<[Span]>,
    /// The call sites counted against the memory limit so far.
    sites_counted: usize,
}

impl Run {
    /// The occurrence of the helper call at `span` on this bar, counting from 0.
    pub fn occurrence(&mut self, span: Span) -> u32 {
        let count = self.occurrences.entry(span).or_insert(0);
        *count += 1;
        *count - 1
    }

    /// The check of an operation, callbacks' included: every
    /// [`OPERATIONS_PER_CHECK`](limits::OPERATIONS_PER_CHECK) operations against the time
    /// limits and whether the study was dropped meanwhile.
    pub fn operation(&mut self) -> bool {
        self.operations += 1;
        if self.stop.is_none() && self.operations.is_multiple_of(limits::OPERATIONS_PER_CHECK) {
            self.stop = self
                .clock
                .check()
                .or_else(|| self.cancelled().then_some(Stop::Cancelled));
            self.due = self.stop.is_none();
        }
        self.stop.is_some()
    }

    /// Whether the front end is to count the values of the scope running at call level `level`
    /// now, which it does once after each clock read. The counts of deeper levels, whose calls
    /// have returned, are dropped.
    pub fn count_due(&mut self, level: usize) -> bool {
        self.levels.truncate(level + 1);
        std::mem::take(&mut self.due)
    }

    /// The count of the values of the scope running at call level `level`, `bytes` in all,
    /// `outside` of them outside the states: a stop at the memory limit when the study holds
    /// more with them and the last counts of its callers' scopes in place of the values kept
    /// after the last bar, which the snapshot's copy may still hold.
    pub fn count(&mut self, level: usize, bytes: usize, outside: usize) -> bool {
        self.levels.resize(level, (0, 0));
        self.levels.push((bytes, outside));
        let bytes = self.levels.iter().map(|(bytes, _)| bytes).sum();
        if self.stop.is_none() && self.holding(bytes) > self.clock.limits.memory {
            self.stop = Some(Stop::Memory);
            self.running = self.levels.iter().map(|(_, outside)| outside).sum();
        }
        self.stop.is_some()
    }

    /// The bar loop's check against the full-recompute limit and for a dropped study, keeping
    /// a stop already reached.
    fn check_bars(&mut self) -> Option<Stop> {
        self.stop = self.stop.or_else(|| {
            self.clock
                .bars()
                .or_else(|| self.cancelled().then_some(Stop::Cancelled))
        });
        self.stop
    }

    /// Prepare bar `bar`: no plot values yet, no helper reached, a missing value in every
    /// declared series and the bar's clock not started.
    fn start_bar(&mut self, bar: usize) {
        self.bar = bar;
        self.row.fill(f64::NAN);
        self.occurrences.clear();
        self.levels.clear();
        self.clock.next_bar();
        for column in &mut self.columns.declared {
            column.push(f64::NAN);
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
    }

    /// Feed streaming helper `name` at call site `site` with `x` (and `b` for the crossings)
    /// on this bar, feeding missing values for the bars it was not reached on. A new site
    /// counts against the memory limit.
    pub fn step(
        &mut self,
        site: Site,
        name: &str,
        length: i64,
        x: f64,
        b: f64,
    ) -> Result<f64, String> {
        if length < 0 {
            return Err(format!("{name} length {length} is negative"));
        }
        let bar = self.bar;
        let entry = match self.sites.entry(site) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let kernel = Kernel::new(name, usize::try_from(length).unwrap_or(usize::MAX));
                entry.insert(CallSite {
                    kernel,
                    length,
                    last: bar,
                })
            }
        };
        if entry.length != length {
            return Err(format!(
                "{name} length changed from {} to {length}; a call keeps its length on every bar",
                entry.length
            ));
        }
        for _ in entry.last + 1..bar {
            entry.kernel.step(f64::NAN, f64::NAN);
        }
        entry.last = bar;
        let value = entry.kernel.step(x, b);
        if self.sites.len() > self.sites_counted {
            self.sites_counted = self.sites.len();
            if self.bytes() > self.clock.limits.memory {
                self.stop = Some(Stop::Memory);
                return Err(String::new());
            }
        }
        Ok(value)
    }
}

thread_local! {
    static RUN: RefCell<Option<Run>> = const { RefCell::new(None) };
}

/// Call `f` with the run installed on this thread, `None` outside a bar loop.
pub(crate) fn with<R>(f: impl FnOnce(&mut Run) -> R) -> Option<R> {
    RUN.with(|run| run.try_borrow_mut().ok()?.as_mut().map(f))
}

/// Installs a run on this thread and takes it back on every exit path, a panic included.
struct Installed<'a>(&'a mut Run);

impl<'a> Installed<'a> {
    fn new(run: &'a mut Run) -> Self {
        RUN.with(|slot| *slot.borrow_mut() = Some(std::mem::replace(run, Run::empty())));
        Self(run)
    }
}

impl Drop for Installed<'_> {
    fn drop(&mut self) {
        if let Some(run) = RUN.with(|slot| slot.borrow_mut().take()) {
            *self.0 = run;
        }
    }
}

/// The bars of a run request, from the first bar that changed since the previous request.
#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub from: usize,
    pub bars: Vec<Bar>,
    pub fed: Option<Vec<f64>>,
    pub inputs: Vec<InputValue>,
    /// Start over from bar 0 whatever `from` says.
    pub reset: bool,
}

/// The end of a run.
#[derive(Clone, Debug)]
pub(crate) struct Outcome {
    /// The first bar of `rows`.
    pub start: usize,
    /// The plot values of the bars from `start`, one row of every plot per bar.
    pub rows: Vec<f64>,
    /// How long the run took, or the limit that stopped it, whose rows are empty.
    pub result: Result<Duration, ScriptError>,
    /// The first runtime error, whose bar and later bars are missing.
    pub error: Option<ScriptError>,
}

/// A run in progress.
#[derive(Debug)]
struct Flight {
    start: usize,
    next: usize,
    rows: Vec<f64>,
    error: Option<ScriptError>,
    /// The time spent running it, summed over its slices.
    took: Duration,
}

/// One study's calculation: its run, the front end's slots and what carries between runs.
pub(crate) struct RunState<P: Program> {
    program: Arc<P>,
    run: Run,
    slots: P::Slots,
    inputs: Vec<InputValue>,
    /// The state before the last bar calculated.
    snapshot: Option<Snapshot<P::Slots>>,
    /// The bar of the latest runtime error, after which no bar runs.
    failed: Option<usize>,
    /// The bars calculated.
    done: usize,
    flight: Option<Flight>,
    /// The fraction of the run's bars done, as `f32` bits, for a reader on another thread.
    progress: Arc<AtomicU32>,
}

impl<P: Program> RunState<P> {
    pub fn new(
        program: Arc<P>,
        spec: Arc<ScriptSpec>,
        series_spans: Arc<[Span]>,
        limits: Limits,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Self {
        let run = Run {
            columns: Columns {
                declared: vec![Vec::new(); spec.series.len()],
                ..Columns::default()
            },
            row: vec![f64::NAN; spec.plots.len()],
            kept: Vec::new(),
            spec,
            bar: 0,
            sites: HashMap::new(),
            occurrences: HashMap::new(),
            clock: Clock::start(limits),
            operations: 0,
            due: false,
            levels: Vec::new(),
            running: 0,
            stop: None,
            cancel,
            series_spans,
            sites_counted: 0,
        };
        Self {
            slots: program.slots(&[], false),
            program,
            run,
            inputs: Vec::new(),
            snapshot: None,
            failed: None,
            done: 0,
            flight: None,
            progress: Arc::new(AtomicU32::new(0)),
        }
    }

    /// The fraction of the current run's bars done, readable from any thread.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn progress(&self) -> Arc<AtomicU32> {
        self.progress.clone()
    }

    /// Start a run for `request`, resuming from the state before the last bar when only the
    /// last bar changed and from the last state when bars were appended.
    pub fn begin(&mut self, request: Request) {
        let Request {
            from,
            bars,
            fed,
            inputs,
            reset,
        } = request;
        let columns = &mut self.run.columns;
        columns.bars.truncate(from);
        columns.bars.extend(bars);
        let fed_changed = fed.is_some() != columns.fed.is_some();
        columns.fed = fed.map(|suffix| {
            let mut column = columns.fed.take().unwrap_or_default();
            column.truncate(from);
            column.extend(suffix);
            column
        });
        let start = if reset || fed_changed || inputs != self.inputs || from + 1 < self.done {
            0
        } else if from + 1 == self.done {
            match self.snapshot.take() {
                Some(snapshot) => {
                    self.slots = snapshot.slots;
                    self.run.sites = snapshot.sites;
                    self.failed = snapshot.failed;
                    self.done - 1
                }
                None => 0,
            }
        } else {
            self.done
        };
        if start == 0 {
            self.slots = self.program.slots(&inputs, self.run.columns.fed.is_some());
            self.run.kept.clear();
            self.run.sites.clear();
            self.snapshot = None;
            self.failed = None;
        }
        self.inputs = inputs;
        self.run.columns.truncate(start);
        self.run.sites_counted = self.run.sites.len();
        self.run.clock = Clock::start(self.run.clock.limits);
        self.run.stop = None;
        self.run.running = 0;
        self.progress.store(0f32.to_bits(), Ordering::Relaxed);
        self.flight = Some(Flight {
            start,
            next: start,
            rows: Vec::new(),
            error: None,
            took: Duration::ZERO,
        });
        if self.run.bytes() > self.run.clock.limits.memory {
            self.run.stop = Some(Stop::Memory);
        }
    }

    /// Run bars of the current run until it ends, or until `until` once a bar has run, and
    /// return its outcome when it ends.
    pub fn advance(&mut self, until: Option<Instant>) -> Option<Outcome> {
        let mut flight = self.flight.take()?;
        let began = Instant::now();
        let len = self.run.columns.bars.len();
        let width = self.run.spec.plots.len();
        let fraction =
            |next: usize, start: usize| ((next - start) as f32 / (len - start) as f32).to_bits();
        let mut stop = self.run.stop.map(|limit| (limit, Span::default(), None));
        if stop.is_none() {
            let _installed = Installed::new(&mut self.run);
            let first = flight.next;
            while flight.next < len {
                let i = flight.next;
                if i > first && until.is_some_and(|until| Instant::now() >= until) {
                    break;
                }
                if i > flight.start && (i - flight.start).is_multiple_of(limits::BARS_PER_CHECK) {
                    self.progress
                        .store(fraction(i, flight.start), Ordering::Relaxed);
                    if let Some(limit) = with(Run::check_bars).flatten() {
                        stop = Some((limit, Span::default(), Some(i)));
                        break;
                    }
                }
                if i + 1 == len {
                    self.snapshot = Some(Snapshot {
                        slots: self.slots.clone(),
                        sites: with(|run| run.sites.clone()).unwrap_or_default(),
                        failed: self.failed,
                    });
                }
                with(|run| run.start_bar(i));
                let result = match self.failed {
                    Some(_) => Ok(()),
                    None => self.program.bar(&mut self.slots),
                };
                // A script can catch a stop raised in a callback, so a stop ends the run
                // whatever the bar returned. The values the bar keeps count from now on.
                let limit = with(|run| {
                    if run.stop.is_none() && run.bytes() > run.clock.limits.memory {
                        run.stop = Some(Stop::Memory);
                    }
                    run.stop
                });
                if let Some(limit) = limit.flatten() {
                    let span = result.err().map_or(Span::default(), |error| error.span);
                    stop = Some((limit, span, Some(i)));
                    break;
                }
                match result {
                    Ok(()) => {
                        with(|run| flight.rows.extend_from_slice(&run.row));
                    }
                    Err(error) => {
                        self.failed = Some(i);
                        flight.error = Some(ScriptError::at(error.message, error.span, Some(i)));
                        flight.rows.extend(std::iter::repeat_n(f64::NAN, width));
                    }
                }
                flight.next = i + 1;
            }
        }
        flight.took += began.elapsed();
        if let Some((limit, span, bar)) = stop {
            let span = if limit == Stop::Memory {
                self.run.largest(span)
            } else {
                span
            };
            // A stop keeps nothing of the run, so the next one starts over.
            self.done = 0;
            self.snapshot = None;
            return Some(Outcome {
                start: flight.start,
                rows: Vec::new(),
                result: Err(ScriptError::at(self.run.clock.message(limit), span, bar)),
                error: None,
            });
        }
        if flight.next < len {
            self.progress
                .store(fraction(flight.next, flight.start), Ordering::Relaxed);
            self.flight = Some(flight);
            return None;
        }
        self.done = len;
        Some(Outcome {
            start: flight.start,
            rows: flight.rows,
            result: Ok(flight.took),
            error: flight.error,
        })
    }

    /// The current run's progress, as a fraction of its bars.
    #[cfg(target_arch = "wasm32")]
    pub fn fraction(&self) -> f32 {
        f32::from_bits(self.progress.load(Ordering::Relaxed))
    }
}

/// The outcome of a run whose worker ended without answering, after a panic in it.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn lost() -> Outcome {
    Outcome {
        start: 0,
        rows: Vec::new(),
        result: Err(ScriptError::at(
            "the calculation ended unexpectedly".to_owned(),
            Span::default(),
            None,
        )),
        error: None,
    }
}

impl Run {
    /// A run holding nothing, left in place while the real one is installed.
    fn empty() -> Self {
        Self {
            spec: Arc::default(),
            columns: Columns::default(),
            bar: 0,
            row: Vec::new(),
            kept: Vec::new(),
            sites: HashMap::new(),
            occurrences: HashMap::new(),
            clock: Clock::start(Limits::default()),
            operations: 0,
            due: false,
            levels: Vec::new(),
            running: 0,
            stop: None,
            cancel: None,
            series_spans: Arc::from(Vec::new()),
            sites_counted: 0,
        }
    }

    /// The bytes the study holds once every bar of the run is calculated: the bars, the fed
    /// column, the declared series, the plot columns, the helpers and the values kept between
    /// bars, the last two again for the snapshot's copy.
    fn bytes(&self) -> usize {
        self.holding(self.kept.iter().map(|(bytes, _)| bytes).sum())
    }

    /// [`bytes`](Self::bytes) with `values` bytes in the values a bar runs on in place of the
    /// kept ones, which the snapshot's copy may still hold.
    fn holding(&self, values: usize) -> usize {
        let len = self.columns.bars.len();
        let columns = usize::from(self.columns.fed.is_some())
            + self.spec.series.len()
            + self.spec.plots.len();
        let helpers: usize = self
            .sites
            .values()
            .map(|site| std::mem::size_of::<CallSite>() + site.kernel.bytes(len))
            .sum();
        let kept: usize = self.kept.iter().map(|(bytes, _)| bytes).sum();
        len * (std::mem::size_of::<Bar>() + columns * 8) + 2 * helpers + kept + values
    }

    /// Where the declaration or call site holding the most memory is, the first of equals, or
    /// `running`, where the bar was when the count of its running values stopped it, if those
    /// held more.
    fn largest(&self, running: Span) -> Span {
        let len = self.columns.bars.len();
        let series = self.series_spans.first().map(|span| (len * 8, *span));
        let helpers = self
            .sites
            .iter()
            .map(|((span, _), site)| (site.kernel.bytes(len), *span));
        let values = (self.running > 0).then_some((self.running, running));
        series
            .into_iter()
            .chain(helpers)
            .chain(self.kept.iter().copied())
            .chain(values)
            .reduce(|largest, next| if next.0 > largest.0 { next } else { largest })
            .map_or(Span::default(), |(_, span)| span)
    }
}
