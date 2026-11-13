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
    localization as l10n,
    model::{Bar, Signal, StrategyReport, Timeframe},
    strategies::{Backtest, BacktestSettings, Strategy, StrategyFactory},
    studies::{InputValue, StudyInput},
    theme::ThemeExt,
    ui_kit::tab_bar::{TabBar, TabLabel},
};

/// History needed for one run. Answer with [`StrategyTester::set_history`] or
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
    /// Finer fill interval, or `None` for chart-bar fills.
    pub lower_timeframe: Option<Timeframe>,
}

/// Host intents from the tester. Replace the previous run's markers when a run completes.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StrategyTesterAction {
    /// Fetch chart history and, when requested, finer fill bars for this run.
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
}

#[derive(Clone, Copy, Default, PartialEq)]
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

struct Trial {
    inputs: Vec<StudyInput>,
    label: String,
    profit: f64,
    drawdown: Option<f64>,
    trades: usize,
}

enum JobOutput {
    Report(StrategyReport, Vec<Bar>),
    Trials(Vec<Trial>),
}

type Job = Pin<Box<dyn Future<Output = Result<JobOutput, &'static str>>>>;

/// A factory-backed strategy tester, available with `ui` and `strategies`.
///
/// Call [`Self::show`] with completed bars of the chart's current symbol and interval. Run and
/// optimisation use a snapshot of those bars and yield between execution batches on native and
/// web builds. Host strategy preparation and callbacks must be bounded, as documented on
/// [`Backtest::run_async`]. Edits cancel pending work and retain the previous report as stale.
/// History replies are tied to a request id and discarded after an edit or scope change.
/// Give each tester its own egui id scope.
pub struct StrategyTester {
    factory: Arc<StrategyFactory>,
    name: String,
    inputs: Vec<StudyInput>,
    settings: BacktestSettings,
    undo_settings: Vec<BacktestSettings>,
    redo_settings: Vec<BacktestSettings>,
    settings_edit: Option<BacktestSettings>,
    start: String,
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
    completed: Rc<Cell<usize>>,
    total: usize,
    ranges: Vec<Range>,
    trials: Vec<Trial>,
    sort: usize,
    ascending: bool,
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
        let inputs = factory
            .create(&name)
            .map_or_else(Vec::new, |strategy| strategy.study_inputs());
        let ranges = inputs.iter().filter_map(Range::new).collect();
        Self {
            factory: Arc::new(factory),
            name,
            inputs,
            ranges,
            settings: BacktestSettings::default(),
            undo_settings: Vec::new(),
            redo_settings: Vec::new(),
            settings_edit: None,
            start: String::new(),
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
            completed: Rc::new(Cell::new(0)),
            total: 0,
            trials: Vec::new(),
            sort: 1,
            ascending: false,
        }
    }

    /// Current strategy name in the factory.
    pub fn strategy(&self) -> &str {
        &self.name
    }

    /// Select a strategy and restore its declared defaults. Unknown names leave the form intact.
    pub fn select_strategy(&mut self, name: &str) -> Result<(), &'static str> {
        let strategy = self.factory.create(name).ok_or("Unknown strategy")?;
        self.name = name.into();
        self.inputs = strategy.study_inputs();
        self.ranges = self.inputs.iter().filter_map(Range::new).collect();
        self.changed();
        Ok(())
    }

    /// Edit an input through the strategy's own validation.
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
            self.settings = settings;
            self.changed();
        }
        Ok(())
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
        self.start = self
            .settings
            .start_time
            .map_or_else(String::new, |time| time.to_rfc3339());
        self.changed();
    }

    /// Whether the report no longer describes the current form or chart scope.
    pub fn is_stale(&self) -> bool {
        self.stale
    }

    /// The last completed or host-supplied report.
    pub fn report(&self) -> Option<&StrategyReport> {
        self.report.report()
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
    pub fn set_history(
        &mut self,
        request: &StrategyHistoryRequest,
        bars: Vec<Bar>,
        lower: Vec<Bar>,
    ) -> Result<(), &'static str> {
        let Some((pending, optimise)) = &self.pending else {
            return Err("History request is no longer current");
        };
        if pending != request {
            return Err("History request is no longer current");
        }
        if bars.is_empty() || (request.lower_timeframe.is_some() && lower.is_empty()) {
            return Err("Requested history is empty");
        }
        let optimise = *optimise;
        self.pending = None;
        self.start_job(&request.symbol, request.timeframe, bars, lower, optimise)
    }

    /// Show the host's failure for the exact pending request. Returns false for a late failure.
    pub fn history_failed(
        &mut self,
        request: &StrategyHistoryRequest,
        message: impl Into<String>,
    ) -> bool {
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

    fn changed(&mut self) {
        self.stale = true;
        self.revision = self.revision.wrapping_add(1);
        self.pending = None;
        self.job = None;
        self.trials.clear();
        self.error = None;
    }

    fn configured_strategy(&self) -> Result<Box<dyn Strategy>, &'static str> {
        let mut strategy = self.factory.create(&self.name).ok_or("Unknown strategy")?;
        for input in &self.inputs {
            strategy.set_input(&input.id, input.value.clone())?;
        }
        Ok(strategy)
    }

    fn start_job(
        &mut self,
        symbol: &str,
        timeframe: Timeframe,
        bars: Vec<Bar>,
        lower: Vec<Bar>,
        optimise: bool,
    ) -> Result<(), &'static str> {
        super::properties::validate(&self.settings)?;
        self.configured_strategy()?;
        let (count, axes) = if optimise {
            optimisation::grid(&self.ranges)?
        } else {
            (1, Vec::new())
        };
        let factory = self.factory.clone();
        let name = self.name.clone();
        let inputs = self.inputs.clone();
        let mut engine = Backtest::new(&name, symbol, timeframe, self.settings.initial_capital);
        engine.settings = self.settings.clone();
        self.total = count;
        self.completed = Rc::new(Cell::new(0));
        let completed = self.completed.clone();
        self.error = None;
        self.trials.clear();
        self.job = Some(Box::pin(async move {
            let mut trials = Vec::new();
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
                let label = strategy
                    .inputs()
                    .iter()
                    .map(|(name, value)| format!("{name}: {value}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let report = engine.run_async(strategy.as_mut(), &bars, &lower).await;
                completed.set(ordinal + 1);
                if !optimise {
                    return Ok(JobOutput::Report(report, bars));
                }
                let performance = report.performance();
                trials.push(Trial {
                    inputs,
                    label,
                    profit: performance.net_profit,
                    drawdown: performance.max_drawdown,
                    trades: performance.closed_trades,
                });
            }
            Ok(JobOutput::Trials(trials))
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
        self.configured_strategy()?;
        if optimise {
            optimisation::grid(&self.ranges)?;
        }
        let stale = self.stale;
        self.changed();
        if optimise {
            self.stale = stale;
        }
        if self.settings.start_time.is_some() || self.lower {
            let request = StrategyHistoryRequest {
                id: self.revision,
                symbol: symbol.into(),
                timeframe,
                start: self
                    .settings
                    .start_time
                    .or_else(|| bars.first().map(|bar| bar.time))
                    .ok_or("Supply a start date")?,
                lower_timeframe: self.lower.then_some(self.lower_timeframe),
            };
            self.pending = Some((request.clone(), optimise));
            Ok(Some(StrategyTesterAction::RequestHistory(request)))
        } else {
            self.start_job(symbol, timeframe, bars.to_vec(), Vec::new(), optimise)?;
            Ok(None)
        }
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
            if self.stale && self.report().is_some() {
                ui.colored_label(
                    ui.active_theme().ui.warning,
                    l10n::text(ui.ctx(), "Result is stale. Run again to update."),
                );
            }
            if self.pending.is_some() {
                ui.label(l10n::text(ui.ctx(), "Waiting for host history"));
            }
            if self.job.is_some() || self.pending.is_some() {
                ui.label(format!("{} / {} trials", self.completed.get(), self.total));
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
                            crate::ui_kit::FormGrid::new("strategy_inputs").show(ui, |ui| {
                                for (input, _) in self
                                    .inputs
                                    .iter_mut()
                                    .zip(shown)
                                    .filter(|(_, shown)| *shown)
                                {
                                    edited |= super::inputs::show_input(ui, input, &[]);
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
                                let stale = self.stale;
                                self.changed();
                                self.stale = stale;
                            }
                            match optimisation::grid(&self.ranges) {
                                Ok((count, _)) => {
                                    ui.label(format!("{count} trials"));
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
                            edited |= self.show_trials(ui);
                        }
                        Tab::Report => {}
                    });
            }
        }
        if edited {
            self.changed();
        }
        let valid_start = if self.start.trim().is_empty() {
            Ok(None)
        } else {
            DateTime::parse_from_rfc3339(&self.start)
                .map(|time| Some(time.with_timezone(&Utc)))
                .or_else(|_| {
                    NaiveDate::parse_from_str(&self.start, "%Y-%m-%d")
                        .map(|date| Some(date.and_hms_opt(0, 0, 0).unwrap().and_utc()))
                })
                .map_err(|_| "Enter a UTC date (YYYY-MM-DD) or an RFC 3339 timestamp")
        };
        match valid_start {
            Ok(start) => {
                self.settings.start_time = start;
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
        if let Some(job) = &mut self.job {
            match job.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                Poll::Pending => ui.ctx().request_repaint(),
                Poll::Ready(result) => {
                    self.job = None;
                    match result {
                        Ok(JobOutput::Report(report, bars)) => {
                            actions.push(StrategyTesterAction::Signals {
                                symbol: report.symbol.clone(),
                                timeframe: report.timeframe,
                                signals: report.signals(),
                            });
                            let from = bars.partition_point(|bar| {
                                self.settings
                                    .start_time
                                    .is_some_and(|start| bar.time < start)
                            });
                            self.report.set_report(Some(report), &bars[from..]);
                            self.stale = false;
                            self.tab = Tab::Report;
                        }
                        Ok(JobOutput::Trials(trials)) => {
                            self.trials = trials;
                            self.sort_trials();
                        }
                        Err(error) => self.error = Some(error.into()),
                    }
                    ui.ctx().request_repaint();
                }
            }
        }
        actions
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

#[cfg(test)]
#[path = "tester_tests.rs"]
mod tests;
