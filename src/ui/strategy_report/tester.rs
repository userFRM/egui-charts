//! Strategy selection, execution and optimisation on host-supplied history.

use std::{
    cell::Cell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

use chrono::{DateTime, NaiveDate, Utc};
use egui::Ui;

use super::{
    StrategyReportAction, StrategyReportPanel,
    optimisation::{self, Range},
};
use crate::{
    data::StudyDataRequest,
    localization as l10n,
    model::{Bar, BarClock, Signal, StrategyReport, Timeframe},
    strategies::{Backtest, BacktestSettings, OrderSize, Strategy, StrategyFactory},
    studies::{InputValue, StudyInput},
    theme::ThemeExt,
    ui_kit::tab_bar::{TabBar, TabLabel},
};

/// History needed for one run. Answer with [`StrategyTester::set_history`],
/// [`StrategyTester::set_history_with_rates`] when it names exchange rates,
/// [`StrategyTester::set_history_with_calculation`] when it names calculation bars, or
/// [`StrategyTester::history_failed`]; the library never fetches it itself.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StrategyHistoryRequest {
    /// Identifies this request within the tester, including otherwise identical reruns.
    pub id: u64,
    /// Instrument to request.
    pub symbol: String,
    /// Decision interval.
    pub timeframe: Timeframe,
    /// Requested beginning. Include earlier bars when needed to warm the strategy.
    pub start: DateTime<Utc>,
    /// Requested end, or `None` for the newest bars. Bars closing after it are not used.
    pub end: Option<DateTime<Utc>>,
    /// Finer fill interval, or `None` for chart-bar fills.
    pub lower_timeframe: Option<Timeframe>,
    /// The interval the strategy calculates on besides the chart's, or `None`. Supply the
    /// instrument's own bars from the one containing `start`, or earlier for warm-up, through
    /// [`StrategyTester::set_history_with_calculation`].
    pub calculation_timeframe: Option<Timeframe>,
    /// Exchange rates from the instrument's currency into the account currency, when they
    /// differ. Answer with [`StrategyTester::set_history_with_rates`]; without them the run
    /// reports them unavailable.
    pub exchange_rate: Option<StudyDataRequest>,
}

impl StrategyHistoryRequest {
    /// The request `id` for `symbol`'s `timeframe` bars from `start`, filled on chart bars in
    /// the instrument's currency. Assign [`lower_timeframe`](Self::lower_timeframe) for finer
    /// fills, [`end`](Self::end) to bound the run and [`exchange_rate`](Self::exchange_rate)
    /// for another account currency, and [`calculation_timeframe`](Self::calculation_timeframe)
    /// for the bars a strategy calculates on.
    ///
    /// ```
    /// use egui_charts::model::Timeframe;
    /// use egui_charts::ui::strategy_report::StrategyHistoryRequest;
    ///
    /// let start = "2026-01-05T00:00:00Z".parse()?;
    /// let mut request = StrategyHistoryRequest::new(1, "AAPL", Timeframe::Day1, start);
    /// request.end = Some("2026-02-02T00:00:00Z".parse()?);
    /// request.lower_timeframe = Some(Timeframe::Hour1);
    /// assert_eq!(request.exchange_rate, None);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(
        id: u64,
        symbol: impl Into<String>,
        timeframe: Timeframe,
        start: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            symbol: symbol.into(),
            timeframe,
            start,
            end: None,
            lower_timeframe: None,
            calculation_timeframe: None,
            exchange_rate: None,
        }
    }
}

/// Host intents from the tester. Replace the previous run's markers when a run completes or continues.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StrategyTesterAction {
    /// Fetch chart history and, when requested, finer fill bars, calculation bars and exchange
    /// rates for this run.
    RequestHistory(StrategyHistoryRequest),
    /// Replace markers for the given scope with these entries and exits, including an empty list.
    Signals {
        /// Instrument of the completed run.
        symbol: String,
        /// Decision interval of the completed run.
        timeframe: Timeframe,
        /// Labelled fills from [`StrategyReport::signals`].
        signals: Vec<Signal>,
    },
    /// Navigation or export intent from the report view.
    Report(StrategyReportAction),
    /// Start a live run with these inputs and settings on the chart's symbol and interval,
    /// continue it with [`Backtest::update`], and open the alert dialog on its Strategy fills,
    /// listing the run in [`AlertDialog::strategy_runs`](crate::ui::dialogs::AlertDialog::strategy_runs).
    /// The host supplies instrument currency, exchange rates and calculation bars when needed.
    AddAlert {
        /// Strategy name in the tester's factory.
        strategy: String,
        /// Inputs by id, as `Strategy::set_input` takes them.
        inputs: Vec<(String, InputValue)>,
        /// Account and execution settings of the form.
        settings: BacktestSettings,
        /// Instrument of the tester's chart.
        symbol: String,
        /// Decision interval of the tester's chart.
        timeframe: Timeframe,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Tab {
    Inputs,
    Properties,
    Optimisation,
    #[default]
    Report,
}

impl TabLabel for Tab {
    fn tab_label(&self) -> &str {
        match self {
            Self::Inputs => "Inputs",
            Self::Properties => "Backtest settings",
            Self::Optimisation => "Optimisation",
            Self::Report => "Report",
        }
    }
}

#[derive(Debug)]
struct Trial {
    inputs: Vec<StudyInput>,
    label: String,
    profit: f64,
    drawdown: Option<f64>,
    trades: usize,
}

enum JobOutput {
    Report(Box<StrategyReport>, Box<Continuation>),
    Trials(Vec<Trial>, Vec<(&'static str, usize)>),
}

type Job = Pin<Box<dyn Future<Output = Result<JobOutput, &'static str>>>>;

/// The completed single run's bars, retained to compare against later chart frames.
struct Continuation {
    /// Retained execution state and the strategy that produced it.
    engine: Backtest,
    strategy: Box<dyn Strategy>,
    /// Finer fill history and its requested interval, if used.
    lower: Vec<Bar>,
    lower_timeframe: Option<Timeframe>,
    /// Host history first, then chart bars, oldest first.
    bars: Vec<Bar>,
    /// Leading bars supplied through `set_history`.
    host: usize,
    /// Chart bars at or before the last host bar when the run completed.
    chart: Vec<Bar>,
}

impl Continuation {
    /// Chart bars after host history, and whether the captured chart bars within that history
    /// are unchanged. Chart bars that close later within it are host bars the run already used.
    fn tail<'a>(&self, bars: &'a [Bar]) -> (&'a [Bar], bool) {
        let split = self.host.checked_sub(1).map(|last| self.bars[last].time);
        let cut = bars.partition_point(|bar| split.is_some_and(|time| bar.time <= time));
        let kept = bars[..cut]
            .get(..self.chart.len())
            .is_some_and(|captured| crate::strategies::same_bars(captured, &self.chart));
        (&bars[cut..], kept)
    }
}

impl std::fmt::Debug for Continuation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Continuation")
            .field("bars", &self.bars.len())
            .field("host", &self.host)
            .field("chart", &self.chart.len())
            .field("lower", &self.lower.len())
            .field("lower_timeframe", &self.lower_timeframe)
            .finish_non_exhaustive()
    }
}

/// Resume the retained engine cooperatively, returning it with its updated report.
fn continue_job(
    mut follow: Box<Continuation>,
    bars: Vec<Bar>,
    lower: Vec<Bar>,
    completed: Rc<Cell<usize>>,
) -> Job {
    Box::pin(async move {
        let Continuation {
            engine, strategy, ..
        } = &mut *follow;
        engine
            .update_closed_async(strategy.as_mut(), &bars, &lower)
            .await;
        let report = engine
            .report()
            .cloned()
            .expect("an update leaves a live report");
        follow.bars = bars;
        follow.lower = lower;
        completed.set(1);
        Ok(JobOutput::Report(Box::new(report), follow))
    })
}

/// Counts one trial skipped for `reason`, keeping reasons in first-seen order.
fn skip(skipped: &mut Vec<(&'static str, usize)>, reason: &'static str) {
    match skipped.iter_mut().find(|(seen, _)| *seen == reason) {
        Some((_, count)) => *count += 1,
        None => skipped.push((reason, 1)),
    }
}

/// A factory-backed strategy tester, available with `ui` and `strategies`.
///
/// Call [`Self::show`] with completed bars of the chart's current symbol and interval. Run and
/// optimisation use a snapshot of those bars and yield between execution batches on native and
/// web builds. Host strategy preparation and callbacks must be bounded, as documented on
/// [`Backtest::run_async`]. Edits cancel pending work and retain the previous report as stale.
/// History replies are tied to a request id and discarded after an edit or scope change.
///
/// A completed Run retains its engine and strategy. More closed chart bars continue its report,
/// returning [`StrategyTesterAction::Signals`] with updated fills; the result equals a fresh Run
/// over that history. A corrected processed bar rebuilds it. Shorter history, or a changed or
/// missing chart bar at or before the last host-supplied bar, marks it stale and ends
/// continuation; chart bars that close later within host history are already part of the run.
/// Runs using finer fills, exchange rates or calculation bars request those data for new bars
/// and stay stale until answered through [`Self::set_history`], [`Self::set_history_with_rates`]
/// or [`Self::set_history_with_calculation`]. Only the reply's finer bars, rates and calculation
/// bars are used. Optimisation results do not continue. Form edits, a scope
/// change, Run and [`Self::set_report`] end continuation; optimisation ranges and requests leave
/// it running. Continued reports keep the report tab, comparison toggle and selected entry when
/// it remains, as [`Self::selected_trade`] reports.
///
/// An optimisation skips combinations the strategy refuses as a whole, and runs that lack exchange
/// rates, and shows how many, with the reason. Pass the chart instrument's currency to
/// [`Self::set_instrument_currency`] and its calendar to [`Self::set_bar_clock`]. Give each
/// tester its own egui id scope.
pub struct StrategyTester {
    factory: Arc<StrategyFactory>,
    name: String,
    inputs: Vec<StudyInput>,
    settings: BacktestSettings,
    instrument_currency: Option<String>,
    bar_clock: Option<BarClock>,
    undo_settings: Vec<BacktestSettings>,
    redo_settings: Vec<BacktestSettings>,
    settings_edit: Option<BacktestSettings>,
    start: String,
    end: String,
    lower: bool,
    lower_timeframe: Timeframe,
    tab: Tab,
    report: StrategyReportPanel,
    stale: bool,
    error: Option<String>,
    scope: Option<(String, Timeframe)>,
    revision: u64,
    pending: Option<(StrategyHistoryRequest, bool)>,
    job: Option<Job>,
    follow: Option<Box<Continuation>>,
    /// Owns the continuation until the job completes.
    follow_job: Option<Job>,
    /// Host data requested for a captured continuation history.
    follow_request: Option<(StrategyHistoryRequest, Box<Continuation>, Vec<Bar>)>,
    completed: Rc<Cell<usize>>,
    total: usize,
    ranges: Vec<Range>,
    trials: Vec<Trial>,
    skipped: Vec<(&'static str, usize)>,
    sort: usize,
    ascending: bool,
}

impl std::fmt::Debug for StrategyTester {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StrategyTester")
            .field("factory", &self.factory)
            .field("name", &self.name)
            .field("inputs", &self.inputs)
            .field("settings", &self.settings)
            .field("instrument_currency", &self.instrument_currency)
            .field("bar_clock", &self.bar_clock)
            .field("undo_settings", &self.undo_settings)
            .field("redo_settings", &self.redo_settings)
            .field("settings_edit", &self.settings_edit)
            .field("start", &self.start)
            .field("end", &self.end)
            .field("lower", &self.lower)
            .field("lower_timeframe", &self.lower_timeframe)
            .field("tab", &self.tab)
            .field("report", &self.report)
            .field("stale", &self.stale)
            .field("error", &self.error)
            .field("scope", &self.scope)
            .field("revision", &self.revision)
            .field("pending", &self.pending)
            .field("follow", &self.follow)
            .field(
                "follow_request",
                &self.follow_request.as_ref().map(|(request, ..)| request),
            )
            .field("completed", &self.completed)
            .field("total", &self.total)
            .field("ranges", &self.ranges)
            .field("trials", &self.trials)
            .field("skipped", &self.skipped)
            .field("sort", &self.sort)
            .field("ascending", &self.ascending)
            .finish_non_exhaustive()
    }
}

impl Default for StrategyTester {
    fn default() -> Self {
        Self::new(StrategyFactory::new())
    }
}

impl StrategyTester {
    /// Use the built-in catalogue and any constructors the host registered in `factory`.
    pub fn new(factory: StrategyFactory) -> Self {
        let name = factory.list().first().map_or("", |name| *name).to_owned();
        let strategy = factory.create(&name);
        let inputs = strategy
            .as_ref()
            .map_or_else(Vec::new, |strategy| strategy.study_inputs());
        let settings = BacktestSettings {
            order_size: strategy
                .as_deref()
                .map_or(BacktestSettings::default().order_size, strategy_order_size),
            ..Default::default()
        };
        let ranges = inputs.iter().filter_map(Range::new).collect();
        Self {
            factory: Arc::new(factory),
            name,
            inputs,
            ranges,
            settings,
            instrument_currency: None,
            bar_clock: None,
            undo_settings: Vec::new(),
            redo_settings: Vec::new(),
            settings_edit: None,
            start: String::new(),
            end: String::new(),
            lower: false,
            lower_timeframe: Timeframe::Sec1,
            tab: Tab::Report,
            report: StrategyReportPanel::default(),
            stale: true,
            error: None,
            scope: None,
            revision: 0,
            pending: None,
            job: None,
            follow: None,
            follow_job: None,
            follow_request: None,
            completed: Rc::new(Cell::new(0)),
            total: 0,
            trials: Vec::new(),
            skipped: Vec::new(),
            sort: 1,
            ascending: false,
        }
    }

    /// Current strategy name in the factory.
    pub fn strategy(&self) -> &str {
        &self.name
    }

    /// Select a strategy and restore its declared defaults: its inputs and its
    /// [order size](Strategy::default_order_size), or the default size when none is declared
    /// or the declaration is invalid. The size changes as one settings step, which
    /// [`Self::undo_settings`] reverts; other settings are kept. Unknown names leave the form
    /// intact. A host restoring saved settings selects the strategy first.
    pub fn select_strategy(&mut self, name: &str) -> Result<(), &'static str> {
        let strategy = self.factory.create(name).ok_or("Unknown strategy")?;
        let order_size = strategy_order_size(strategy.as_ref());
        if self.settings.order_size != order_size {
            self.undo_settings.push(self.settings.clone());
            self.redo_settings.clear();
            self.settings.order_size = order_size;
        }
        self.name = name.into();
        self.inputs = strategy.study_inputs();
        self.ranges = self.inputs.iter().filter_map(Range::new).collect();
        self.changed();
        Ok(())
    }

    /// Edit an input through the strategy's own validation. Inputs checked together, such as
    /// zone levels, are checked when a run starts, so they can be edited in any order.
    pub fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        let mut strategy = self.configured_strategy()?;
        strategy.set_input(id, value)?;
        self.inputs = strategy.study_inputs();
        self.changed();
        Ok(())
    }

    /// Current account and execution assumptions.
    pub fn settings(&self) -> &BacktestSettings {
        &self.settings
    }

    /// Replace assumptions, cancelling pending work and marking the report stale.
    pub fn set_settings(&mut self, settings: BacktestSettings) -> Result<(), &'static str> {
        super::properties::validate(&settings)?;
        if self.settings != settings {
            self.undo_settings.push(self.settings.clone());
            self.redo_settings.clear();
            self.start = settings
                .start_time
                .map_or_else(String::new, |time| time.to_rfc3339());
            self.end = settings
                .end_time
                .map_or_else(String::new, |time| time.to_rfc3339());
            self.settings = settings;
            self.changed();
        }
        Ok(())
    }

    /// Set the currency of the chart instrument's prices, such as
    /// [`Instrument::currency`](crate::model::Instrument::currency). Runs report in it when
    /// the settings name no currency, and request exchange rates when they name another. A
    /// change makes the report stale.
    pub fn set_instrument_currency(&mut self, currency: Option<String>) {
        if self.instrument_currency != currency {
            self.instrument_currency = currency;
            self.changed();
        }
    }

    /// Set the instrument's calendar, including its zone and exchange session, as used to build
    /// the supplied history. Runs place other calculation intervals on this calendar, as
    /// [`Backtest::bar_clock`] describes. A change makes the report stale.
    pub fn set_bar_clock(&mut self, clock: BarClock) {
        if self.bar_clock.as_ref() != Some(&clock) {
            self.bar_clock = Some(clock);
            self.changed();
        }
    }

    /// Restore the previous account and execution settings, retaining the report as stale.
    pub fn undo_settings(&mut self) -> bool {
        let Some(settings) = self.undo_settings.pop() else {
            return false;
        };
        self.redo_settings
            .push(std::mem::replace(&mut self.settings, settings));
        self.restore_settings();
        true
    }

    /// Reapply settings undone through [`Self::undo_settings`].
    pub fn redo_settings(&mut self) -> bool {
        let Some(settings) = self.redo_settings.pop() else {
            return false;
        };
        self.undo_settings
            .push(std::mem::replace(&mut self.settings, settings));
        self.restore_settings();
        true
    }

    fn restore_settings(&mut self) {
        self.end = self
            .settings
            .end_time
            .map_or_else(String::new, |time| time.to_rfc3339());
        self.start = self
            .settings
            .start_time
            .map_or_else(String::new, |time| time.to_rfc3339());
        self.changed();
    }

    /// Whether the report no longer describes the current form, chart scope or chart bars.
    pub fn is_stale(&self) -> bool {
        self.stale
    }

    /// The last completed or host-supplied report.
    pub fn report(&self) -> Option<&StrategyReport> {
        self.report.report()
    }

    /// Index into the report's trades of the trade selected in its list. Continued runs keep it
    /// on the same entry, so a host highlighting that trade reads it again after
    /// [`StrategyTesterAction::Signals`].
    pub fn selected_trade(&self) -> Option<usize> {
        self.report.selected
    }

    /// Replace or clear a host report, cancelling pending work. A host report is not a run of the
    /// current form, so it stays stale until Run completes.
    pub fn set_report(&mut self, report: Option<StrategyReport>, bars: &[Bar]) {
        self.changed();
        self.tab = Tab::Report;
        self.report.set_report(report, bars);
    }

    /// Show only the report, without the tester form, while a host-managed run supplies updates.
    pub fn show_report(&mut self, ui: &mut Ui) -> StrategyReportAction {
        self.report.show(ui)
    }

    /// Supply both histories for the exact pending request. A superseded reply is rejected.
    /// Empty finer history is refused rather than silently using chart-bar fills.
    /// A request naming exchange rates then reports them unavailable. A continuation uses only
    /// `lower` from the reply and the chart bars received by [`Self::show`]. A request naming a
    /// calculation interval needs [`Self::set_history_with_calculation`].
    pub fn set_history(
        &mut self,
        request: &StrategyHistoryRequest,
        bars: Vec<Bar>,
        lower: Vec<Bar>,
    ) -> Result<(), &'static str> {
        self.set_history_with_rates(request, bars, lower, Vec::new())
    }

    /// Supply both histories and the exchange rates the request names, as [`Self::set_history`]
    /// does. Rates are ignored when the request names none; empty rates for a request that names
    /// them report the rates unavailable. Continuation replies replace finer bars and rates from
    /// the requested start onward, retaining earlier history and using the captured chart bars.
    /// A request naming a calculation interval needs [`Self::set_history_with_calculation`].
    pub fn set_history_with_rates(
        &mut self,
        request: &StrategyHistoryRequest,
        bars: Vec<Bar>,
        lower: Vec<Bar>,
        rates: Vec<Bar>,
    ) -> Result<(), &'static str> {
        self.set_history_with_calculation(request, bars, lower, rates, Vec::new())
    }

    /// Supply the histories, rates and calculation bars named by the pending request. As with
    /// [`Self::set_history_with_rates`], superseded replies are rejected. Empty requested
    /// calculation history is refused; calculation bars are ignored when none were requested.
    /// A continuation replaces held calculation bars from the reply's first bar onward, so the
    /// reply starts with the bar containing the requested `start`.
    pub fn set_history_with_calculation(
        &mut self,
        request: &StrategyHistoryRequest,
        bars: Vec<Bar>,
        lower: Vec<Bar>,
        rates: Vec<Bar>,
        calculation: Vec<Bar>,
    ) -> Result<(), &'static str> {
        if self
            .follow_request
            .as_ref()
            .is_some_and(|(pending, ..)| pending == request)
        {
            if bars.is_empty()
                || (request.lower_timeframe.is_some() && lower.is_empty())
                || (request.calculation_timeframe.is_some() && calculation.is_empty())
            {
                return Err("Requested history is empty");
            }
            let (_, mut follow, bars) = self.follow_request.take().expect("a matching request");
            follow.lower.retain(|bar| bar.time < request.start);
            follow
                .lower
                .extend(lower.into_iter().filter(|bar| bar.time >= request.start));
            if request.exchange_rate.is_some() {
                follow
                    .engine
                    .exchange_rates
                    .retain(|bar| bar.time < request.start);
                follow
                    .engine
                    .exchange_rates
                    .extend(rates.into_iter().filter(|bar| bar.time >= request.start));
            }
            if request.calculation_timeframe.is_some() {
                // The first reply bar can update a forming calculation bar already held.
                let first = calculation[0].time;
                let held = &mut follow.engine.calculation_bars;
                held.retain(|bar| bar.time < first);
                held.extend(calculation);
            }
            let lower = std::mem::take(&mut follow.lower);
            self.follow_job = Some(continue_job(follow, bars, lower, Rc::new(Cell::new(0))));
            return Ok(());
        }
        let Some((pending, optimise)) = &self.pending else {
            return Err("History request is no longer current");
        };
        if pending != request {
            return Err("History request is no longer current");
        }
        if bars.is_empty()
            || (request.lower_timeframe.is_some() && lower.is_empty())
            || (request.calculation_timeframe.is_some() && calculation.is_empty())
        {
            return Err("Requested history is empty");
        }
        let optimise = *optimise;
        self.pending = None;
        let mut engine = self.engine(&request.symbol, request.timeframe);
        engine.exchange_rates = rates;
        if request.calculation_timeframe.is_some() {
            engine.calculation_bars = calculation;
        }
        self.start_job(engine, bars, lower, optimise, Some(request.lower_timeframe))
    }

    /// Show the host's failure for the exact pending request. Returns false for a late failure.
    pub fn history_failed(
        &mut self,
        request: &StrategyHistoryRequest,
        message: impl Into<String>,
    ) -> bool {
        if self
            .follow_request
            .as_ref()
            .is_some_and(|(pending, ..)| pending == request)
        {
            self.follow_request = None;
            self.stale = true;
            self.error = Some(message.into());
            return true;
        }
        if self
            .pending
            .as_ref()
            .is_none_or(|(pending, _)| pending != request)
        {
            return false;
        }
        self.pending = None;
        self.error = Some(message.into());
        true
    }

    /// An edit or scope change stops following and retains the report as stale.
    fn changed(&mut self) {
        self.stale = true;
        self.follow = None;
        self.follow_job = None;
        self.follow_request = None;
        self.cancel();
    }

    /// Cancel pending runs and optimisations, keeping the report and what it follows.
    fn cancel(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.pending = None;
        self.job = None;
        self.trials.clear();
        self.skipped.clear();
        self.error = None;
    }

    fn configured_strategy(&self) -> Result<Box<dyn Strategy>, &'static str> {
        let mut strategy = self.factory.create(&self.name).ok_or("Unknown strategy")?;
        for input in &self.inputs {
            strategy.set_input(&input.id, input.value.clone())?;
        }
        Ok(strategy)
    }

    /// A single run needs the form's inputs valid together on the chart's `timeframe`; an
    /// optimisation checks each combination instead and skips those refused.
    fn check_run(&self, timeframe: Timeframe, optimise: bool) -> Result<(), &'static str> {
        let mut strategy = self.configured_strategy()?;
        if optimise {
            Ok(())
        } else {
            strategy.set_timeframe(timeframe);
            strategy.validate_inputs()
        }
    }

    /// The engine of a run on `symbol`'s `timeframe` bars with the form's settings.
    fn engine(&self, symbol: &str, timeframe: Timeframe) -> Backtest {
        let mut engine =
            Backtest::new(&self.name, symbol, timeframe, self.settings.initial_capital);
        engine.settings = self.settings.clone();
        engine.instrument_currency = self.instrument_currency.clone();
        engine.bar_clock = self.bar_clock.clone();
        engine
    }

    fn start_job(
        &mut self,
        engine: Backtest,
        bars: Vec<Bar>,
        lower: Vec<Bar>,
        optimise: bool,
        request: Option<Option<Timeframe>>,
    ) -> Result<(), &'static str> {
        let timeframe = engine.timeframe;
        super::properties::validate(&self.settings)?;
        self.check_run(timeframe, optimise)?;
        if !optimise {
            if !bars.windows(2).all(|pair| pair[0].time < pair[1].time) {
                return Err("Bar times must be strictly increasing");
            }
            let follow = Box::new(Continuation {
                engine,
                strategy: self.configured_strategy()?,
                host: if request.is_some() { bars.len() } else { 0 },
                bars: Vec::new(),
                chart: Vec::new(),
                lower: Vec::new(),
                lower_timeframe: request.flatten(),
            });
            self.total = 1;
            self.completed = Rc::new(Cell::new(0));
            self.error = None;
            self.trials.clear();
            self.skipped.clear();
            self.job = Some(continue_job(follow, bars, lower, self.completed.clone()));
            return Ok(());
        }
        let (count, axes) = optimisation::grid(&self.ranges)?;
        let factory = self.factory.clone();
        let name = self.name.clone();
        let inputs = self.inputs.clone();
        self.total = count;
        self.completed = Rc::new(Cell::new(0));
        let completed = self.completed.clone();
        self.error = None;
        self.trials.clear();
        self.skipped.clear();
        self.job = Some(Box::pin(async move {
            let mut trials = Vec::new();
            let mut skipped = Vec::new();
            for ordinal in 0..count {
                let mut inputs = inputs.clone();
                let mut index = ordinal;
                for (id, values) in &axes {
                    let input = inputs
                        .iter_mut()
                        .find(|input| input.id == *id)
                        .ok_or("Unknown optimisation input")?;
                    input.value = values[index % values.len()].clone();
                    index /= values.len();
                }
                let mut strategy = factory.create(&name).ok_or("Unknown strategy")?;
                for input in &inputs {
                    strategy.set_input(&input.id, input.value.clone())?;
                }
                strategy.set_timeframe(timeframe);
                if let Err(reason) = strategy.validate_inputs() {
                    skip(&mut skipped, reason);
                    completed.set(ordinal + 1);
                    continue;
                }
                let label = strategy
                    .inputs()
                    .iter()
                    .map(|(name, value)| format!("{name}: {value}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let report = engine.run_async(strategy.as_mut(), &bars, &lower).await;
                completed.set(ordinal + 1);
                if report.unavailable.is_some() {
                    skip(&mut skipped, "Exchange rates unavailable");
                    continue;
                }
                let performance = report.performance();
                trials.push(Trial {
                    inputs,
                    label,
                    profit: performance.all.net_profit,
                    drawdown: performance.max_drawdown,
                    trades: performance.all.closed_trades,
                });
            }
            Ok(JobOutput::Trials(trials, skipped))
        }));
        Ok(())
    }

    fn request_run(
        &mut self,
        symbol: &str,
        timeframe: Timeframe,
        bars: &[Bar],
        optimise: bool,
    ) -> Result<Option<StrategyTesterAction>, &'static str> {
        if bars.is_empty() && self.settings.start_time.is_none() {
            return Err("Supply completed chart bars or a start date before running");
        }
        if self.lower
            && (timeframe.is_trade_based()
                || self.lower_timeframe.is_trade_based()
                || self.lower_timeframe.duration() >= timeframe.duration())
        {
            return Err("Choose a fill interval shorter than the chart interval");
        }
        super::properties::validate(&self.settings)?;
        self.check_run(timeframe, optimise)?;
        if optimise {
            optimisation::grid(&self.ranges)?;
        }
        if optimise {
            self.cancel();
        } else {
            self.changed();
        }
        let calculation = {
            let mut strategy = self.configured_strategy()?;
            strategy.set_timeframe(timeframe);
            strategy.calculation_timeframe()
        };
        let exchange_rate = self.engine(symbol, timeframe).exchange_rate_request();
        if self.settings.start_time.is_some()
            || self.lower
            || exchange_rate.is_some()
            || calculation.is_some()
        {
            let mut request = StrategyHistoryRequest::new(
                self.revision,
                symbol,
                timeframe,
                self.settings
                    .start_time
                    .or_else(|| bars.first().map(|bar| bar.time))
                    .ok_or("Supply a start date")?,
            );
            request.end = self.settings.end_time;
            request.lower_timeframe = self.lower.then_some(self.lower_timeframe);
            request.calculation_timeframe = calculation;
            request.exchange_rate = exchange_rate;
            self.pending = Some((request.clone(), optimise));
            Ok(Some(StrategyTesterAction::RequestHistory(request)))
        } else {
            self.start_job(
                self.engine(symbol, timeframe),
                bars.to_vec(),
                Vec::new(),
                optimise,
                None,
            )?;
            Ok(None)
        }
    }

    fn add_alert(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<StrategyTesterAction, &'static str> {
        super::properties::validate(&self.settings)?;
        self.check_run(timeframe, false)?;
        Ok(StrategyTesterAction::AddAlert {
            strategy: self.name.clone(),
            inputs: self
                .inputs
                .iter()
                .map(|input| (input.id.clone(), input.value.clone()))
                .collect(),
            settings: self.settings.clone(),
            symbol: symbol.into(),
            timeframe,
        })
    }

    /// Draw the form, advance pending execution by one batch and return all host intents.
    /// Bars must belong to `symbol` and `timeframe`; do not include the live, unfinished bar.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        symbol: &str,
        timeframe: Timeframe,
        bars: &[Bar],
    ) -> Vec<StrategyTesterAction> {
        let mut actions = Vec::new();
        if self
            .scope
            .as_ref()
            .is_none_or(|scope| scope.0 != symbol || scope.1 != timeframe)
        {
            self.changed();
            self.scope = Some((symbol.into(), timeframe));
        }
        let mut run = false;
        let mut optimise = false;
        ui.horizontal_wrapped(|ui| {
            ui.strong(l10n::text(ui.ctx(), "Strategy tester"));
            let mut name = self.name.clone();
            egui::ComboBox::from_id_salt("strategy")
                .selected_text(&name)
                .show_ui(ui, |ui| {
                    for choice in self.factory.list() {
                        ui.selectable_value(&mut name, choice.to_owned(), choice);
                    }
                })
                .response
                .accessible_name(l10n::text(ui.ctx(), "Strategy"));
            if name != self.name
                && let Err(error) = self.select_strategy(&name)
            {
                self.error = Some(error.into());
            }
            run = ui
                .add_enabled(
                    self.job.is_none() && self.pending.is_none(),
                    egui::Button::new(l10n::text(ui.ctx(), "Run")),
                )
                .clicked();
            if ui.button(l10n::text(ui.ctx(), "Add alert")).clicked() {
                match self.add_alert(symbol, timeframe) {
                    Ok(action) => actions.push(action),
                    Err(error) => self.error = Some(error.into()),
                }
            }
            if self.stale && self.report().is_some() {
                ui.colored_label(
                    ui.active_theme().ui.warning,
                    l10n::text(ui.ctx(), "Result is stale. Run again to update."),
                );
            }
            if self.pending.is_some() || self.follow_request.is_some() {
                ui.label(l10n::text(ui.ctx(), "Waiting for host history"));
            }
            if self.job.is_some() || self.pending.is_some() {
                ui.label(l10n::format_text(
                    ui.ctx(),
                    "{done} / {total} trials",
                    &[
                        ("done", self.completed.get().to_string()),
                        ("total", self.total.to_string()),
                    ],
                ));
                if ui.button(l10n::text(ui.ctx(), "Cancel run")).clicked() {
                    self.changed();
                }
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(ui.active_theme().ui.error, l10n::text(ui.ctx(), error));
        }
        egui::ScrollArea::horizontal()
            .id_salt("tester_tabs")
            .show(ui, |ui| {
                TabBar::new(
                    &[Tab::Inputs, Tab::Properties, Tab::Optimisation, Tab::Report],
                    &mut self.tab,
                )
                .show(ui);
            });
        let mut edited = false;
        let mut previous_settings = None;
        match self.tab {
            Tab::Report => {
                let action = self.show_report(ui);
                if action != StrategyReportAction::None {
                    actions.push(StrategyTesterAction::Report(action));
                }
            }
            _ => {
                egui::ScrollArea::both()
                    .id_salt("tester_form")
                    .show(ui, |ui| match self.tab {
                        Tab::Inputs => {
                            let shown: Vec<_> = self
                                .inputs
                                .iter()
                                .map(|input| input.is_shown(&self.inputs))
                                .collect();
                            let zones = crate::ui::timeframe_toolbar::default_time_zones();
                            crate::ui_kit::FormGrid::new("strategy_inputs").show(ui, |ui| {
                                for (input, _) in self
                                    .inputs
                                    .iter_mut()
                                    .zip(shown)
                                    .filter(|(_, shown)| *shown)
                                {
                                    edited |= super::inputs::show_input(ui, input, &[], &zones);
                                }
                            });
                            if ui.button(l10n::text(ui.ctx(), "Reset inputs")).clicked() {
                                for input in &mut self.inputs {
                                    input.value = input.default.clone();
                                }
                                edited = true;
                            }
                        }
                        Tab::Properties => {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        !self.undo_settings.is_empty(),
                                        egui::Button::new(l10n::text(ui.ctx(), "Undo settings")),
                                    )
                                    .clicked()
                                {
                                    self.undo_settings();
                                }
                                if ui
                                    .add_enabled(
                                        !self.redo_settings.is_empty(),
                                        egui::Button::new(l10n::text(ui.ctx(), "Redo settings")),
                                    )
                                    .clicked()
                                {
                                    self.redo_settings();
                                }
                            });
                            let before = self.settings.clone();
                            let changed = super::properties::show(
                                ui,
                                &mut self.settings,
                                &mut self.start,
                                &mut self.end,
                                &mut self.lower,
                                &mut self.lower_timeframe,
                            );
                            if changed {
                                edited = true;
                                previous_settings = Some(before);
                            }
                        }
                        Tab::Optimisation => {
                            if optimisation::show(ui, &mut self.ranges) {
                                self.cancel();
                            }
                            match optimisation::grid(&self.ranges) {
                                Ok((count, _)) => {
                                    ui.label(l10n::format_text(
                                        ui.ctx(),
                                        "{count} trials",
                                        &[("count", count.to_string())],
                                    ));
                                    optimise = ui
                                        .add_enabled(
                                            self.job.is_none() && self.pending.is_none(),
                                            egui::Button::new(l10n::text(
                                                ui.ctx(),
                                                "Run optimisation",
                                            )),
                                        )
                                        .clicked();
                                }
                                Err(error) => {
                                    ui.label(l10n::text(ui.ctx(), error));
                                }
                            }
                            for (reason, count) in &self.skipped {
                                ui.colored_label(
                                    ui.active_theme().ui.warning,
                                    l10n::format_text(
                                        ui.ctx(),
                                        "{count} skipped: {reason}",
                                        &[
                                            ("count", count.to_string()),
                                            ("reason", l10n::text(ui.ctx(), reason)),
                                        ],
                                    ),
                                );
                            }
                            edited |= self.show_trials(ui);
                        }
                        Tab::Report => {}
                    });
            }
        }
        if edited {
            self.changed();
        }
        let parse = |text: &str| -> Result<Option<DateTime<Utc>>, &'static str> {
            if text.trim().is_empty() {
                return Ok(None);
            }
            DateTime::parse_from_rfc3339(text)
                .map(|time| Some(time.with_timezone(&Utc)))
                .or_else(|_| {
                    NaiveDate::parse_from_str(text, "%Y-%m-%d")
                        .map(|date| Some(date.and_hms_opt(0, 0, 0).unwrap().and_utc()))
                })
                .map_err(|_| "Enter a UTC date (YYYY-MM-DD) or an RFC 3339 timestamp")
        };
        match parse(&self.start).and_then(|start| parse(&self.end).map(|end| (start, end))) {
            Ok((start, end)) => {
                self.settings.start_time = start;
                self.settings.end_time = end;
                if run || optimise {
                    match self.request_run(symbol, timeframe, bars, optimise) {
                        Ok(Some(action)) => actions.push(action),
                        Ok(None) => {}
                        Err(error) => self.error = Some(error.into()),
                    }
                }
            }
            Err(error) => {
                self.error = Some(error.into());
            }
        }
        if let Some(before) = previous_settings {
            self.settings_edit.get_or_insert(before);
        }
        // A drag changes settings on every frame but is one undo step.
        if !ui.input(|input| input.pointer.any_down())
            && let Some(before) = self.settings_edit.take()
            && self.settings != before
        {
            self.undo_settings.push(before);
            self.redo_settings.clear();
        }
        let stale = self.stale;
        if let Some(action) = self.follow_chart(bars) {
            actions.push(action);
        }
        if self.stale != stale {
            ui.ctx().request_repaint();
        }
        if let Some(job) = &mut self.job {
            match job.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                Poll::Pending => ui.ctx().request_repaint(),
                Poll::Ready(result) => {
                    self.job = None;
                    match result {
                        Ok(JobOutput::Report(report, mut follow)) => {
                            let report = *report;
                            actions.push(StrategyTesterAction::Signals {
                                symbol: report.symbol.clone(),
                                timeframe: report.timeframe,
                                signals: report.signals(),
                            });
                            let from = follow.bars.partition_point(|bar| {
                                self.settings
                                    .start_time
                                    .is_some_and(|start| bar.time < start)
                            });
                            let to = self
                                .settings
                                .bars_before_end(report.timeframe, &follow.bars)
                                .max(from);
                            self.report.set_report(Some(report), &follow.bars[from..to]);
                            if follow.host > 0 {
                                follow.chart = bars[..bars.partition_point(|bar| {
                                    bar.time <= follow.bars[follow.host - 1].time
                                })]
                                    .to_vec();
                            }
                            self.follow = Some(follow);
                            self.stale = false;
                            self.tab = Tab::Report;
                        }
                        Ok(JobOutput::Trials(trials, skipped)) => {
                            self.trials = trials;
                            self.skipped = skipped;
                            self.sort_trials();
                        }
                        Err(error) => self.error = Some(error.into()),
                    }
                    ui.ctx().request_repaint();
                }
            }
        }
        if let Some(job) = &mut self.follow_job {
            match job.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                Poll::Pending => ui.ctx().request_repaint(),
                Poll::Ready(result) => {
                    self.follow_job = None;
                    match result {
                        Ok(JobOutput::Report(report, follow)) => {
                            let report = *report;
                            actions.push(StrategyTesterAction::Signals {
                                symbol: report.symbol.clone(),
                                timeframe: report.timeframe,
                                signals: report.signals(),
                            });
                            let from = follow.bars.partition_point(|bar| {
                                self.settings
                                    .start_time
                                    .is_some_and(|start| bar.time < start)
                            });
                            let to = self
                                .settings
                                .bars_before_end(report.timeframe, &follow.bars)
                                .max(from);
                            self.report.continue_report(report, &follow.bars[from..to]);
                            self.stale = false;
                            self.follow = Some(follow);
                        }
                        Err(error) => {
                            self.error = Some(error.into());
                            self.stale = true;
                        }
                        _ => {}
                    }
                    ui.ctx().request_repaint();
                }
            }
        }
        actions
    }

    /// Follow the chart only while a completed single run is idle.
    fn follow_chart(&mut self, bars: &[Bar]) -> Option<StrategyTesterAction> {
        let follow = self.follow.as_ref()?;
        let (tail, kept) = follow.tail(bars);
        let previous = &follow.bars[follow.host..];
        if !kept || tail.len() < previous.len() {
            self.stale = true;
            self.follow = None;
            return None;
        }
        if crate::strategies::same_bars(tail, previous) {
            return None;
        }
        if !tail.windows(2).all(|pair| pair[0].time < pair[1].time) {
            self.stale = true;
            self.follow = None;
            self.error = Some("Bar times must be strictly increasing".into());
            return None;
        }
        let first = tail
            .iter()
            .zip(previous)
            .take_while(|(a, b)| {
                crate::strategies::same_bars(std::slice::from_ref(a), std::slice::from_ref(b))
            })
            .count();
        let mut bars = follow.bars[..follow.host].to_vec();
        bars.extend_from_slice(tail);
        let exchange_rate = follow.engine.exchange_rate_request();
        let calculation = follow.strategy.calculation_timeframe();
        // Bars past a historical end need no finer fills, exchange rates or calculation bars.
        let needs_history =
            (follow.lower_timeframe.is_some() || exchange_rate.is_some() || calculation.is_some())
                && follow.host + first
                    < self
                        .settings
                        .bars_before_end(follow.engine.timeframe, &bars);
        if needs_history {
            self.revision = self.revision.wrapping_add(1);
            let mut request = StrategyHistoryRequest::new(
                self.revision,
                &follow.engine.symbol,
                follow.engine.timeframe,
                tail[first].time,
            );
            request.end = self.settings.end_time;
            request.lower_timeframe = follow.lower_timeframe;
            request.calculation_timeframe = calculation;
            request.exchange_rate = exchange_rate;
            self.stale = true;
            let follow = self.follow.take().expect("an idle continuation");
            self.follow_request = Some((request.clone(), follow, bars));
            Some(StrategyTesterAction::RequestHistory(request))
        } else {
            let mut follow = self.follow.take().expect("an idle continuation");
            let lower = std::mem::take(&mut follow.lower);
            self.follow_job = Some(continue_job(follow, bars, lower, Rc::new(Cell::new(0))));
            None
        }
    }

    fn sort_trials(&mut self) {
        self.trials.sort_by(|a, b| {
            let order = match self.sort {
                1 => a.profit.total_cmp(&b.profit),
                2 => a
                    .drawdown
                    .partial_cmp(&b.drawdown)
                    .unwrap_or(std::cmp::Ordering::Equal),
                3 => a.trades.cmp(&b.trades),
                _ => a.label.cmp(&b.label),
            };
            if self.ascending {
                order
            } else {
                order.reverse()
            }
        });
    }

    fn show_trials(&mut self, ui: &mut Ui) -> bool {
        let mut sort = None;
        let mut apply = None;
        use egui_extras::{Column, TableBuilder};
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .column(Column::initial(260.0))
            .columns(Column::initial(100.0), 3)
            .column(Column::remainder().at_least(100.0))
            .header(28.0, |mut header| {
                for (index, label) in ["Inputs", "Net profit", "Drawdown", "Closed trades"]
                    .iter()
                    .enumerate()
                {
                    header.col(|ui| {
                        if ui.button(l10n::text(ui.ctx(), *label)).clicked() {
                            sort = Some(index);
                        }
                    });
                }
                header.col(|_| {});
            })
            .body(|body| {
                body.rows(28.0, self.trials.len(), |mut row| {
                    let index = row.index();
                    let trial = &self.trials[index];
                    row.col(|ui| {
                        ui.label(&trial.label);
                    });
                    row.col(|ui| {
                        ui.label(super::number(Some(trial.profit)));
                    });
                    row.col(|ui| {
                        ui.label(super::number(trial.drawdown));
                    });
                    row.col(|ui| {
                        ui.label(trial.trades.to_string());
                    });
                    row.col(|ui| {
                        if ui.button(l10n::text(ui.ctx(), "Apply inputs")).clicked() {
                            apply = Some(index);
                        }
                    });
                });
            });
        if let Some(index) = sort {
            if self.sort == index {
                self.ascending = !self.ascending;
            } else {
                self.sort = index;
                self.ascending = false;
            }
            self.sort_trials();
        }
        if let Some(index) = apply {
            self.inputs = self.trials[index].inputs.clone();
            self.tab = Tab::Inputs;
            return true;
        }
        false
    }
}

/// The order size `strategy` declares, or the default one when it declares none or one the
/// settings refuse.
fn strategy_order_size(strategy: &dyn Strategy) -> OrderSize {
    let default = BacktestSettings::default();
    strategy
        .default_order_size()
        .filter(|&order_size| {
            super::properties::validate(&BacktestSettings {
                order_size,
                ..default.clone()
            })
            .is_ok()
        })
        .unwrap_or(default.order_size)
}

#[cfg(test)]
#[path = "tester_tests.rs"]
mod tests;
