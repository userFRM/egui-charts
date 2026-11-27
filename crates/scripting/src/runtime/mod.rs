//! The series runtime: the bar loop, the columns and helpers a run reads and writes, the studies
//! and other histories a script reads, the state kept between bars and runs, and the limits of 1
//! bar, a full recompute and memory. No type of a front end appears here; a front end implements
//! [`Program`] and reaches the run being calculated through [`with`].

pub(crate) mod bindings;
pub(crate) mod data;
pub(crate) mod drawings;
pub(crate) mod kernels;
pub(crate) mod limits;
pub(crate) mod orders;
pub(crate) mod profile;
pub(crate) mod program;
pub(crate) mod series;
pub(crate) mod state;
pub(crate) mod worker;

use crate::ScriptError;
use bindings::Instance;
use data::Patch;
use drawings::Drawn;
use egui_charts::model::Bar;
use egui_charts::studies::InputValue;
use kernels::Kernel;
use limits::{Clock, Limits, Stop};
use orders::{Decision, OrderCall, Readings};
use profile::{Lines, Profile};
use program::{ScriptSpec, Span};
use series::Columns;
use state::Snapshot;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
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

/// The last count on this bar of the values in a scope, the top level's or a function call's. A
/// call written `f!()` runs in its caller's scope, so it adds to its caller's count.
#[derive(Clone, Copy, Debug)]
struct ScopeCount {
    /// The call level of the call the scope belongs to, the top level's at 0.
    level: usize,
    /// The address of the scope, which stays the same while its call runs.
    scope: usize,
    /// The variables counted together, before the others: at the top level at the start of a
    /// bar the declarations and the states, and otherwise none.
    base: usize,
    /// Where the bytes of the variables after `base` start in [`Run::slots`].
    first: usize,
    /// The bytes of the variables and of a method call's receiver, and those of them outside the
    /// states.
    bytes: usize,
    outside: usize,
}

/// What the per-bar functions of a front end read and write while a run is installed.
#[derive(Debug)]
pub(crate) struct Run {
    pub spec: Arc<ScriptSpec>,
    pub columns: Columns,
    /// The bar being calculated.
    pub bar: usize,
    /// The plot values of the current bar, its alert conditions (1 for true, 0 for false), then
    /// the colours of the plots coloured by an expression ([`drawings::encode`]), each missing
    /// until the script stores one.
    pub row: Vec<f64>,
    /// The drawings of the bar being calculated, in call order.
    pub drawn: Vec<Drawn>,
    /// The bytes of the drawings made up to and including each bar calculated, the bar being
    /// calculated last.
    drawn_bytes: Vec<usize>,
    /// The bytes each value kept between bars holds after the last bar, with where it is
    /// declared; the front end sets them after each bar, and a restored snapshot sets those of
    /// its bar.
    pub kept: Vec<(usize, Span)>,
    pub sites: HashMap<Site, CallSite>,
    occurrences: HashMap<Span, u32>,
    /// The studies called so far, whose lines are in the columns. One reads only those before it.
    instances: Vec<Instance>,
    pub clock: Clock,
    /// The operations run so far; while the run is installed, its thread holds the count
    /// ([`operation`]).
    operations: u64,
    /// Whether the values of the running scope are due to be counted whole, once per clock read.
    due: bool,
    /// The last count on this bar of each scope in reach, the running one last. A caller's scope
    /// is out of reach while it waits, so its last count stands for it, without what it assigned
    /// to its variables after that count.
    scopes: Vec<ScopeCount>,
    /// The bytes of each variable the scopes counted one by one, scope by scope in scope order,
    /// and whether it is outside the states.
    slots: Vec<(usize, bool)>,
    /// The call level, scope address and number of variables of the last variable read on this
    /// bar, whose count is up to date until a clock read or a change of one of them.
    read: (usize, usize, usize),
    /// The bytes the values of the running scope outside the states held when their count
    /// stopped the run, for its report.
    running: usize,
    /// The limit that stopped the run, set by the checks.
    pub stop: Option<Stop>,
    cancel: Option<Arc<AtomicBool>>,
    /// Where each `series` and `bars` declaration is, for memory reports.
    series_spans: Arc<[Span]>,
    history_spans: Arc<[Span]>,
    /// The call sites counted against the memory limit so far.
    sites_counted: usize,
    /// The calls and time of each line, when the run profiles.
    profile: Option<Lines>,
    /// The strategy decision being run, `None` in a study.
    pub decision: Option<Decision>,
    /// The readings each bar of a strategy was last decided with, which a bar run again inside a
    /// later decision is decided with again, counted against the memory limit; empty in a study.
    decided: Vec<Readings>,
}

impl Run {
    /// A [`Run::read`] that matches no read.
    const NO_READ: (usize, usize, usize) = (usize::MAX, 0, 0);

    /// Count a runtime operation at `span` in the profile, charging it the time since the last
    /// one, when the run profiles.
    pub fn mark(&mut self, span: Span) {
        if let Some(lines) = &mut self.profile {
            lines.mark(span);
        }
    }

    /// The occurrence of the helper call at `span` on this bar, counting from 0.
    pub fn occurrence(&mut self, span: Span) -> u32 {
        let count = self.occurrences.entry(span).or_insert(0);
        *count += 1;
        *count - 1
    }

    /// The check at operation `done`, callbacks' included: every
    /// [`OPERATIONS_PER_CHECK`](limits::OPERATIONS_PER_CHECK) operations against the time
    /// limits and whether the study was dropped meanwhile.
    fn operation(&mut self, done: u64) -> bool {
        self.operations = done;
        if self.stop.is_none() && done.is_multiple_of(limits::OPERATIONS_PER_CHECK) {
            self.due = !self.check_time();
        }
        self.schedule();
        self.stop.is_some()
    }

    /// Publish the operation count and the next check of this run to its thread.
    fn schedule(&self) {
        let every = limits::OPERATIONS_PER_CHECK;
        let next = match self.stop {
            Some(_) => 0,
            None => (self.operations / every + 1) * every,
        };
        OPERATIONS.set((self.operations, next));
    }

    /// Stop the run at `limit`; every later operation of the bar ends it.
    fn halt(&mut self, limit: Stop) {
        self.stop = Some(limit);
        // The thread's count stands, as the run's own is stale while it is installed.
        OPERATIONS.set((OPERATIONS.get().0, 0));
    }

    /// The check inside a bar against the time limits and for a dropped study, keeping a stop
    /// already reached.
    fn check_time(&mut self) -> bool {
        if self.stop.is_none() {
            let limit = self.clock.check();
            if let Some(limit) = limit.or_else(|| self.cancelled().then_some(Stop::Cancelled)) {
                self.halt(limit);
            }
        }
        self.stop.is_some()
    }

    /// Start the counts of a bar with the top level's scope, at address `scope`, holding its
    /// `declared` declarations and states: the states count at the bytes they kept after the last
    /// bar and the declarations at nothing, as between bars.
    pub fn count_bar(&mut self, scope: usize, declared: usize) {
        let bytes = self.kept.iter().map(|(bytes, _)| bytes).sum();
        self.scopes.clear();
        self.slots.clear();
        self.read = Self::NO_READ;
        self.scopes.push(ScopeCount {
            level: 0,
            scope,
            base: declared,
            first: 0,
            bytes,
            outside: 0,
        });
    }

    /// Which variables of the scope at address `scope`, running at call level `level` and
    /// holding `len` variables, the front end is to count now, from the one returned on, whether
    /// a clock read was due and whether the scope is the top level's; `None` for none. It counts
    /// all of them after a clock read and at the first read of a call with a scope of its own,
    /// and otherwise those the scope did not hold at its last count, its caller's for a call
    /// written `f!()`. The counts of the calls that have returned are dropped, and so are those
    /// of the variables the scope no longer holds.
    #[inline]
    pub fn count_due(
        &mut self,
        level: usize,
        scope: usize,
        len: usize,
    ) -> Option<(usize, bool, bool)> {
        // Most reads come in the scope of the last one with nothing bound or released since, which
        // leaves its count as it is.
        if !self.due && self.read == (level, scope, len) {
            return None;
        }
        self.read = (level, scope, len);
        self.count_scope(level, scope, len)
    }

    /// [`count_due`](Self::count_due) for a read whose scope may have changed since the last.
    fn count_scope(
        &mut self,
        level: usize,
        scope: usize,
        len: usize,
    ) -> Option<(usize, bool, bool)> {
        // The calls deeper than this one have returned, and so has one at this level in a scope
        // that is not this one.
        while let Some(last) = self.scopes.last()
            && (last.level > level || (last.level == level && last.scope != scope))
        {
            self.slots.truncate(last.first);
            self.scopes.pop();
        }
        let clock = std::mem::take(&mut self.due);
        let Some(last) = self.scopes.last_mut().filter(|last| last.scope == scope) else {
            // The first read of a call that does not share its caller's scope.
            self.scopes.push(ScopeCount {
                level,
                scope,
                base: 0,
                first: self.slots.len(),
                bytes: 0,
                outside: 0,
            });
            return Some((0, clock, level == 0));
        };
        let top = last.level == 0;
        if clock {
            self.slots.truncate(last.first);
            *last = ScopeCount {
                base: 0,
                bytes: 0,
                outside: 0,
                ..*last
            };
            return Some((0, true, top));
        }
        let counted = last.base + self.slots.len() - last.first;
        if len < counted {
            // The places of the variables a block released, which later variables take.
            let released = last.first + len.max(last.base) - last.base;
            for (bytes, outside) in self.slots.drain(released..) {
                last.bytes -= bytes;
                if outside {
                    last.outside -= bytes;
                }
            }
        }
        (len > counted).then_some((counted, false, top))
    }

    /// Add to the count of the scope [`count_due`](Self::count_due) last named the bytes of the
    /// variables it asked for, the last first as the engine lists them, with whether each is
    /// outside the states, and `this`, those of a method call's receiver: a stop at the memory
    /// limit when the study holds more with them and the last counts of its callers' scopes in
    /// place of the values kept after the last bar, which the snapshot's copy may still hold.
    /// Unless `clock`, a count that adds nothing leaves the check as it was.
    pub fn count(
        &mut self,
        variables: impl Iterator<Item = (usize, bool)>,
        this: usize,
        clock: bool,
    ) -> bool {
        let first = self.slots.len();
        let (mut added, mut outside) = (this, this);
        for (bytes, out) in variables {
            self.slots.push((bytes, out));
            added += bytes;
            if out {
                outside += bytes;
            }
        }
        self.slots[first..].reverse();
        if let Some(last) = self.scopes.last_mut() {
            last.bytes += added;
            last.outside += outside;
        }
        if self.stop.is_none() && (clock || added > 0) {
            let values = self.scopes.iter().map(|counted| counted.bytes).sum();
            if self.holding(values) > self.clock.limits.memory {
                self.halt(Stop::Memory);
                self.running = self.scopes.iter().map(|counted| counted.outside).sum();
            }
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

    /// Prepare bar `bar`: no plot values yet, no helper reached, no order placed, a missing value
    /// in every declared series, the bar it reads of every other history and the bar's clock not
    /// started.
    fn start_bar(&mut self, bar: usize) {
        self.bar = bar;
        self.row.fill(f64::NAN);
        self.drawn.clear();
        let made = self.drawn_bytes.last().copied().unwrap_or(0);
        self.drawn_bytes.push(made);
        if let Some(decision) = &mut self.decision {
            decision.calls.clear();
            if let Some(readings) = self.decided.get(bar) {
                decision.readings = *readings;
            }
        }
        self.occurrences.clear();
        self.scopes.clear();
        self.slots.clear();
        self.read = Self::NO_READ;
        self.clock.next_bar();
        for column in &mut self.columns.declared {
            column.push(f64::NAN);
        }
        let time = self.columns.bars[bar].time;
        let last = bar + 1 == self.columns.bars.len();
        for (history, aligned) in self.columns.histories.iter().zip(&mut self.columns.aligned) {
            let at = history
                .as_ref()
                .and_then(|history| history.data().closed_at(time, last));
            aligned.push(at.unwrap_or(usize::MAX));
        }
    }

    /// Keep `drawn`, made by the bar being calculated, and whether the study then holds more than
    /// the memory limit, which stops the run.
    pub fn draw(&mut self, drawn: Drawn) -> bool {
        if let Some(made) = self.drawn_bytes.last_mut() {
            *made += drawn.bytes();
        }
        self.drawn.push(drawn);
        if self.stop.is_none() && self.bytes() > self.clock.limits.memory {
            self.halt(Stop::Memory);
        }
        self.stop.is_some()
    }

    /// Drop the drawings of the bar being calculated, which failed.
    fn forget_drawn(&mut self) {
        self.drawn.clear();
        let before = self
            .bar
            .checked_sub(1)
            .and_then(|bar| self.drawn_bytes.get(bar));
        let before = before.copied().unwrap_or(0);
        if let Some(made) = self.drawn_bytes.last_mut() {
            *made = before;
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
                self.halt(Stop::Memory);
                return Err(String::new());
            }
        }
        Ok(value)
    }
}

thread_local! {
    static RUN: RefCell<Option<Run>> = const { RefCell::new(None) };
    /// The operations of the run installed on this thread, and the operation at which it is next
    /// checked: the next multiple of [`OPERATIONS_PER_CHECK`](limits::OPERATIONS_PER_CHECK), or 0
    /// once it has stopped, so that every later operation reaches it.
    static OPERATIONS: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

/// Call `f` with the run installed on this thread, `None` outside a bar loop.
pub(crate) fn with<R>(f: impl FnOnce(&mut Run) -> R) -> Option<R> {
    RUN.with(|run| run.try_borrow_mut().ok()?.as_mut().map(f))
}

/// The check of an operation of the run installed on this thread, callbacks' included: whether
/// the run has stopped. Most operations only count; the run itself is reached at a check and
/// after a stop.
pub(crate) fn operation() -> bool {
    let (done, next) = OPERATIONS.get();
    let done = done + 1;
    OPERATIONS.set((done, next));
    done >= next && with(|run| run.operation(done)).unwrap_or(false)
}


/// Installs a run on this thread and takes it back on every exit path, a panic included.
struct Installed<'a>(&'a mut Run);

impl<'a> Installed<'a> {
    fn new(run: &'a mut Run) -> Self {
        run.schedule();
        RUN.with(|slot| *slot.borrow_mut() = Some(std::mem::replace(run, Run::empty())));
        Self(run)
    }
}

impl Drop for Installed<'_> {
    fn drop(&mut self) {
        if let Some(mut run) = RUN.with(|slot| slot.borrow_mut().take()) {
            run.operations = OPERATIONS.get().0;
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
    /// The changes of the other histories in declaration order, `None` for an unchanged one.
    pub histories: Vec<Option<Patch>>,
    pub inputs: Vec<InputValue>,
    /// Start over from bar 0 whatever `from` says.
    pub reset: bool,
    /// Record the calls and time of each line.
    pub profile: bool,
    /// The readings of a strategy decision, `None` for a study.
    pub decision: Option<Readings>,
}

/// The end of a run.
#[derive(Clone, Debug)]
pub(crate) struct Outcome {
    /// The first bar of `rows`.
    pub start: usize,
    /// The values of the bars from `start`, one row of every plot, then of every alert condition,
    /// then of the colour of every plot coloured by an expression, per bar.
    pub rows: Vec<f64>,
    /// The drawings of the bars from `start`, with the bar that made each.
    pub drawn: Vec<(usize, Drawn)>,
    /// The bars the rows cover, from `start`.
    pub bars: usize,
    /// How long the run took, or the limit that stopped it, whose rows are empty.
    pub result: Result<Duration, ScriptError>,
    /// The first runtime error, whose bar and later bars are missing.
    pub error: Option<ScriptError>,
    /// The calls and time of each line, when the request asked for them and the run ended.
    pub profile: Option<Profile>,
    /// The order calls of the last bar, in call order, when it ran without an error; empty for a
    /// study and after a stop.
    pub orders: Vec<OrderCall>,
}

/// A run in progress.
#[derive(Debug)]
struct Flight {
    start: usize,
    /// The studies called in earlier runs yet to bring up to date, which come before any bar.
    updating: Range<usize>,
    next: usize,
    rows: Vec<f64>,
    drawn: Vec<(usize, Drawn)>,
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
        history_spans: Arc<[Span]>,
        limits: Limits,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Self {
        let run = Run {
            columns: Columns {
                declared: vec![Vec::new(); spec.series.len()],
                histories: vec![None; spec.histories.len()],
                aligned: vec![Vec::new(); spec.histories.len()],
                ..Columns::default()
            },
            row: vec![f64::NAN; spec.width()],
            drawn: Vec::new(),
            drawn_bytes: Vec::new(),
            kept: Vec::new(),
            spec,
            bar: 0,
            sites: HashMap::new(),
            occurrences: HashMap::new(),
            instances: Vec::new(),
            clock: Clock::start(limits),
            operations: 0,
            due: false,
            scopes: Vec::new(),
            slots: Vec::new(),
            read: Run::NO_READ,
            running: 0,
            stop: None,
            cancel,
            series_spans,
            history_spans,
            sites_counted: 0,
            profile: None,
            decision: None,
            decided: Vec::new(),
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
            histories,
            inputs,
            reset,
            profile,
            decision,
        } = request;
        self.run.profile = profile.then(Lines::new);
        self.run.decision = decision.map(|readings| Decision {
            readings,
            calls: Vec::new(),
        });
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
        for (patch, history) in histories.into_iter().zip(&mut columns.histories) {
            if let Some(patch) = patch {
                patch.apply(history);
            }
        }
        let len = columns.bars.len();
        // The bars from `from` are decided with this decision's readings, the earlier ones with
        // those they were last decided with, also when they run again.
        if let Some(readings) = decision {
            self.run.decided.truncate(from);
            self.run.decided.resize(len, readings);
        }
        // A bar reads the other histories differently while it is the last, so another number of
        // bars changes the one that was or becomes the last.
        let from = if columns.histories.is_empty() || len == self.done {
            from
        } else {
            from.min(self.done.min(len).saturating_sub(1))
        };
        let start = if reset || fed_changed || inputs != self.inputs || from + 1 < self.done {
            0
        } else if from + 1 == self.done {
            match self.snapshot.take() {
                Some(snapshot) => {
                    self.slots = snapshot.slots;
                    self.run.sites = snapshot.sites;
                    self.run.kept = snapshot.kept;
                    self.run.instances.truncate(snapshot.studies);
                    self.failed = snapshot.failed;
                    self.done - 1
                }
                None => 0,
            }
        } else {
            self.done
        };
        self.inputs = inputs;
        self.start_at(start);
        self.run.clock = Clock::start(self.run.clock.limits);
        self.run.stop = None;
        self.run.running = 0;
        self.progress.store(0f32.to_bits(), Ordering::Relaxed);
        self.flight = Some(Flight {
            start,
            updating: 0..self.run.instances.len(),
            next: start,
            rows: Vec::new(),
            drawn: Vec::new(),
            error: None,
            took: Duration::ZERO,
        });
        if self.run.bytes() > self.run.clock.limits.memory {
            self.run.stop = Some(Stop::Memory);
        }
    }

    /// Prepare the run to start at bar `start`, at bar 0 with nothing kept of an earlier run.
    fn start_at(&mut self, start: usize) {
        let run = &mut self.run;
        if start == 0 {
            self.slots = self.program.slots(&self.inputs, run.columns.fed.is_some());
            run.kept.clear();
            run.sites.clear();
            run.instances.clear();
            self.snapshot = None;
            self.failed = None;
        }
        run.columns.truncate(start, run.instances.len());
        run.drawn_bytes.truncate(start);
        run.sites_counted = run.sites.len();
    }

    /// Run bars of the current run until it ends, or until `until` once a bar has run or a study
    /// it called before is up to date, and return its outcome when it ends.
    pub fn advance(&mut self, until: Option<Instant>) -> Option<Outcome> {
        let mut flight = self.flight.take()?;
        let began = Instant::now();
        let len = self.run.columns.bars.len();
        let width = self.run.spec.width();
        let fraction =
            |next: usize, start: usize| ((next - start) as f32 / (len - start) as f32).to_bits();
        let mut stop = self.run.stop.map(|limit| (limit, Span::default(), None));
        while stop.is_none()
            && let Some(at) = flight.updating.next()
        {
            let span = self.run.instances[at].span;
            if self.run.update_study(at, flight.start) {
                // Earlier bars read what the study has changed since.
                self.start_at(0);
                (flight.start, flight.next, flight.updating) = (0, 0, 0..0);
            }
            stop = self.run.check_bars().map(|limit| (limit, span, None));
            if stop.is_none() && until.is_some_and(|until| Instant::now() >= until) {
                flight.took += began.elapsed();
                self.flight = Some(flight);
                return None;
            }
        }
        if stop.is_none() {
            if let Some(lines) = &mut self.run.profile {
                lines.resume();
            }
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
                    let (sites, studies, kept) =
                        with(|run| (run.sites.clone(), run.instances.len(), run.kept.clone()))
                            .unwrap_or_default();
                    self.snapshot = Some(Snapshot {
                        slots: self.slots.clone(),
                        sites,
                        studies,
                        kept,
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
                        with(|run| {
                            flight.rows.extend_from_slice(&run.row);
                            flight
                                .drawn
                                .extend(run.drawn.drain(..).map(|drawn| (i, drawn)));
                        });
                    }
                    Err(error) => {
                        // A failed bar's drawings are dropped with it, as its orders are.
                        with(Run::forget_drawn);
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
                drawn: Vec::new(),
                bars: 0,
                result: Err(ScriptError::at(self.run.clock.message(limit), span, bar)),
                error: None,
                profile: None,
                orders: Vec::new(),
            });
        }
        if flight.next < len {
            self.progress
                .store(fraction(flight.next, flight.start), Ordering::Relaxed);
            self.flight = Some(flight);
            return None;
        }
        self.done = len;
        // A failed bar's calls are dropped with it, and no bar after it runs.
        let orders = match (self.failed, &mut self.run.decision) {
            (None, Some(decision)) => std::mem::take(&mut decision.calls),
            _ => Vec::new(),
        };
        Some(Outcome {
            start: flight.start,
            rows: flight.rows,
            drawn: flight.drawn,
            bars: len - flight.start,
            result: Ok(flight.took),
            error: flight.error,
            profile: self.run.profile.as_ref().map(Lines::profile),
            orders,
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
        drawn: Vec::new(),
        bars: 0,
        result: Err(ScriptError::at(
            "the calculation ended unexpectedly".to_owned(),
            Span::default(),
            None,
        )),
        error: None,
        profile: None,
        orders: Vec::new(),
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
            drawn: Vec::new(),
            drawn_bytes: Vec::new(),
            kept: Vec::new(),
            sites: HashMap::new(),
            occurrences: HashMap::new(),
            instances: Vec::new(),
            clock: Clock::start(Limits::default()),
            operations: 0,
            due: false,
            scopes: Vec::new(),
            slots: Vec::new(),
            read: Self::NO_READ,
            running: 0,
            stop: None,
            cancel: None,
            series_spans: Arc::from(Vec::new()),
            history_spans: Arc::from(Vec::new()),
            sites_counted: 0,
            profile: None,
            decision: None,
            decided: Vec::new(),
        }
    }

    /// The bytes the study holds once every bar of the run is calculated: the bars, the fed
    /// column, the declared series, the plot, alert condition and colour columns, the drawings,
    /// the studies called, the other histories with the bars read of them, the helpers and the
    /// values kept between bars, the last two again for the snapshot's copy, and the readings
    /// each bar of a strategy was decided with.
    fn bytes(&self) -> usize {
        self.holding(self.kept.iter().map(|(bytes, _)| bytes).sum())
    }

    /// [`bytes`](Self::bytes) with `values` bytes in the values a bar runs on in place of the
    /// kept ones, which the snapshot's copy may still hold.
    fn holding(&self, values: usize) -> usize {
        let len = self.columns.bars.len();
        let columns =
            usize::from(self.columns.fed.is_some()) + self.spec.series.len() + self.spec.width();
        let helpers: usize = self
            .sites
            .values()
            .map(|site| std::mem::size_of::<CallSite>() + site.kernel.bytes(len))
            .sum();
        let studies: usize = self.instances.iter().map(|study| study.bytes(len)).sum();
        let histories: usize = self.history_bytes().sum();
        let kept: usize = self.kept.iter().map(|(bytes, _)| bytes).sum();
        len * (std::mem::size_of::<Bar>() + columns * 8)
            + self.drawings()
            + studies
            + histories
            + 2 * helpers
            + kept
            + values
            + self.decided.len() * std::mem::size_of::<Readings>()
    }

    /// The bytes of the drawings made so far.
    fn drawings(&self) -> usize {
        self.drawn_bytes.last().copied().unwrap_or(0)
    }

    /// The bytes each other history holds, the bars read of it included.
    fn history_bytes(&self) -> impl Iterator<Item = usize> {
        let aligned = self.columns.bars.len() * std::mem::size_of::<usize>();
        self.columns.histories.iter().map(move |history| {
            let bars = history.as_ref().map_or(0, |history| history.bars.len());
            bars * std::mem::size_of::<Bar>() + aligned
        })
    }

    /// Where the declaration or call site holding the most memory is, the first of equals, or
    /// `running`, where the bar was when the count of its running values or a drawing stopped
    /// it, if those held more.
    fn largest(&self, running: Span) -> Span {
        let len = self.columns.bars.len();
        let series = self.series_spans.first().map(|span| (len * 8, *span));
        let helpers = self
            .sites
            .iter()
            .map(|((span, _), site)| (site.kernel.bytes(len), *span));
        let studies = self
            .instances
            .iter()
            .map(|study| (study.bytes(len), study.span));
        let histories = self.history_bytes().zip(self.history_spans.iter().copied());
        let values = (self.running > 0).then_some((self.running, running));
        let drawings = (self.drawings() > 0).then_some((self.drawings(), running));
        series
            .into_iter()
            .chain(helpers)
            .chain(studies)
            .chain(histories)
            .chain(self.kept.iter().copied())
            .chain(values)
            .chain(drawings)
            .reduce(|largest, next| if next.0 > largest.0 { next } else { largest })
            .map_or(Span::default(), |(_, span)| span)
    }
}
