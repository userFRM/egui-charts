//! Chart history records UI actions, not ordinary host API calls.

use super::price_series::{PriceSeries, SeriesOptions};
use super::{Chart, ChartElementId, ChartEvent, SeriesId};
use crate::config::{ChartConfig, ChartOptions};
#[cfg(feature = "drawings")]
use crate::drawings::DrawingManager;
#[cfg(feature = "drawings")]
use crate::drawings::services::HistoryService;
use crate::model::{ChartType, ChartTypeParams, SeriesAlerts, SeriesScale, Timeframe};
use crate::scales::PriceScale;
use crate::studies::{Indicator, StudySettingsSnapshot};

#[derive(Default)]
pub(super) struct ChartHistory {
    undo: Vec<Action>,
    redo: Vec<Action>,
    pending: Option<Pending>,
    changed: bool,
    /// User edits recorded, undone or redone, kept when the history is cleared.
    pub(super) edits: u64,
    /// The edits among them that changed drawings.
    pub(super) drawing_edits: u64,
}

struct Pending {
    depth: usize,
    before: Action,
    #[cfg(feature = "drawings")]
    standalone_drawings: HistoryService,
    study_order: Vec<SeriesId>,
    /// Added series removed during the action, with their bars, price formatters and bar limits.
    removed_series: Vec<HeldSeries>,
}

#[derive(Default)]
struct Action {
    symbol: Option<String>,
    series: Option<SeriesSet>,
    config: Option<ChartConfig>,
    /// The main series of the other state, when the action removed it.
    main_series: Option<MainSeries>,
    main_series_visible: Option<bool>,
    price_targets_visible: Option<bool>,
    options: Option<ChartOptions>,
    view: Option<(f32, f32)>,
    scale: Option<PriceScale>,
    /// Scales that differ from the chart's, by assignment; `None` where the chart has none.
    series_scales: std::collections::HashMap<crate::model::SeriesScale, Option<PriceScale>>,
    panes: Option<super::pane_layout::PaneLayout>,
    chart_type: Option<(ChartType, ChartTypeParams)>,
    interval: Option<Option<Timeframe>>,
    studies: Vec<StudyChange>,
    #[cfg(feature = "drawings")]
    drawings: HistoryService,
}

/// The added series of one state of the chart: their draw order, display settings and
/// visibility, the scale of the main series, and the series the other state lacks, whole, with
/// their price formatters and bar limits.
struct SeriesSet {
    props: Vec<(SeriesId, SeriesOptions, bool)>,
    main_scale: SeriesScale,
    main_alerts: SeriesAlerts,
    held: Vec<HeldSeries>,
}

impl SeriesSet {
    fn capture(chart: &Chart) -> Self {
        Self {
            props: chart
                .price_series
                .iter()
                .map(|item| (item.id, item.options.clone(), item.visible))
                .collect(),
            main_scale: chart.main_series_scale,
            main_alerts: chart.main_series_style.alerts,
            held: Vec::new(),
        }
    }

    fn same(&self, other: &Self) -> bool {
        self.main_scale == other.main_scale
            && self.main_alerts == other.main_alerts
            && self.props == other.props
    }
}

/// An added series that one state of the chart lacks, with all its bars, whatever the replay
/// cutoff, and the price formatter and bar limit that the chart keeps apart from it.
struct HeldSeries {
    series: PriceSeries,
    formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    bar_limit: Option<std::num::NonZeroUsize>,
}

/// Whether the main series is present in one state of the chart, with the price lines, price
/// formatter and bar limit that the state without it lacks. Its bars and markers stay in the
/// chart, which reads them only while the series is present; the bars also give the remaining
/// series their time axis while it is absent.
#[derive(Default)]
struct MainSeries {
    present: bool,
    price_lines: Vec<(super::PriceLineId, crate::model::PriceLine)>,
    formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    bar_limit: Option<std::num::NonZeroUsize>,
    /// The time axis the absent state kept, for the redo of a removal whose undo reloads the feed.
    bars: Vec<crate::model::Bar>,
}

struct StudyChange {
    id: SeriesId,
    before: Option<Study>,
}

struct Study {
    index: usize,
    settings: StudySettingsSnapshot,
    indicator: Option<Box<dyn Indicator>>,
    inputs: String,
    autoscale: bool,
}

impl ChartHistory {
    #[cfg(feature = "drawings")]
    pub(super) fn visit_chart_points(
        &mut self,
        visit: &mut impl FnMut(&mut crate::drawings::ChartPoint),
    ) {
        for action in self.undo.iter_mut().chain(&mut self.redo) {
            action.drawings.visit_chart_points(visit);
        }
        if let Some(pending) = &mut self.pending {
            pending.standalone_drawings.visit_chart_points(visit);
            pending.before.drawings.visit_chart_points(visit);
        }
    }

    /// Whether a user action is in progress.
    pub(super) fn recording(&self) -> bool {
        self.pending.is_some()
    }

    /// Forgets every action, keeping the edit counts.
    pub(super) fn clear(&mut self) {
        *self = Self {
            edits: self.edits,
            drawing_edits: self.drawing_edits,
            ..Self::default()
        };
    }

    fn push(&mut self, action: Action) {
        self.edits += 1;
        #[cfg(feature = "drawings")]
        if action.drawings.can_undo() {
            self.drawing_edits += 1;
        }
        self.redo.clear();
        if self.undo.len() == 100 {
            self.undo.remove(0);
        }
        self.undo.push(action);
    }

    #[cfg(feature = "drawings")]
    pub(super) fn start_drawing_action(
        &self,
        drawings: &mut DrawingManager,
    ) -> Option<HistoryService> {
        if self.pending.is_some() {
            return None;
        }
        let mut history = HistoryService::with_max_size(0);
        drawings.swap_history(&mut history);
        Some(history)
    }

    #[cfg(feature = "drawings")]
    pub(super) fn finish_drawing_action(
        &mut self,
        drawings: &mut DrawingManager,
        history: Option<HistoryService>,
    ) {
        if let Some(mut history) = history {
            drawings.swap_history(&mut history);
            if history.can_undo() {
                self.push(Action {
                    drawings: history,
                    ..Action::default()
                });
            }
        }
    }
}

impl Study {
    fn capture(chart: &Chart, id: SeriesId) -> Option<Self> {
        let index = chart
            .study_ids
            .iter()
            .position(|candidate| *candidate == id)?;
        Some(Self {
            index,
            settings: chart.studies.indicators()[index].snapshot_settings(),
            indicator: None,
            inputs: chart.study_inputs[index].clone(),
            autoscale: chart.study_autoscale[index],
        })
    }

    fn same(&self, other: &Self) -> bool {
        self.index == other.index
            && self.inputs == other.inputs
            && self.autoscale == other.autoscale
            && self.settings == other.settings
    }
}

impl Chart {
    /// Apply one user action from a host toolbar, menu, dialog or intent as one undo step.
    /// Nested actions belong to the outer action. Unchanged chart/study settings add no history.
    ///
    /// Chart-owned drawing interactions and shortcuts record themselves. Ordinary host calls
    /// (including data/feed updates, setters and `drawings_mut`) do not enter chart history;
    /// wrap only edits originating from a user action in this method. Drawing commands and
    /// bulk snapshots are transferred from the drawing manager, retaining its sync events.
    /// History restores the edited study or settings snapshot, not market data or host state.
    /// Hosts without a chart-owned feed should use [`Self::take_history_changed`] to reload
    /// bars and synchronize their interval/style/settings controls after undo or redo.
    pub fn user_action<R>(&mut self, edit: impl FnOnce(&mut Self) -> R) -> R {
        self.begin_user_action();
        let result = edit(self);
        self.end_user_action();
        result
    }

    /// Begin a user action spanning a host handler that also borrows non-chart state.
    /// Pair with [`Self::end_user_action`]; prefer [`Self::user_action`] when possible.
    pub fn begin_user_action(&mut self) {
        self.apply_config_scale_mode();
        if let Some(pending) = &mut self.history.pending {
            pending.depth += 1;
            return;
        }
        #[cfg(feature = "drawings")]
        let mut standalone_drawings = HistoryService::with_max_size(0);
        self.sync_panes();
        #[cfg(feature = "drawings")]
        self.drawings.swap_history(&mut standalone_drawings);
        self.history.pending = Some(Pending {
            depth: 1,
            before: Action {
                symbol: Some(self.symbol.clone()),
                series: Some(SeriesSet::capture(self)),
                config: Some(self.config.clone()),
                main_series_visible: Some(self.main_series_visible),
                price_targets_visible: Some(self.show_price_targets),
                options: Some(self.chart_options.clone()),
                view: Some(self.get_time_scale_state()),
                scale: Some(self.state.price_scale().clone()),
                series_scales: self
                    .series_scales
                    .iter()
                    .map(|(id, scale)| (*id, Some(scale.clone())))
                    .collect(),
                panes: Some(self.pane_layout.clone()),
                chart_type: Some((self.chart_type, self.chart_type_params.clone())),
                interval: Some(self.timeframe),
                ..Action::default()
            },
            #[cfg(feature = "drawings")]
            standalone_drawings,
            study_order: self.study_ids.clone(),
            removed_series: Vec::new(),
        });
    }

    /// Finish the matching [`Self::begin_user_action`], recording only changed components.
    pub fn end_user_action(&mut self) {
        #[cfg(feature = "studies")]
        if !self.config.show_volume
            && let Some(id) = self.volume_study
        {
            // Capture the removed study before the volume setting's action closes.
            self.remove_series(id);
        }
        let Some(mut pending) = self.history.pending.take() else {
            return;
        };
        if pending.depth > 1 {
            pending.depth -= 1;
            self.history.pending = Some(pending);
            return;
        }
        self.apply_config_scale_mode();
        #[cfg(feature = "drawings")]
        self.drawings.swap_history(&mut pending.standalone_drawings);
        self.sync_panes();
        let mut action = pending.before;
        #[cfg(feature = "drawings")]
        {
            action.drawings = pending.standalone_drawings;
        }
        if action.symbol.as_ref() == Some(&self.symbol) {
            action.symbol = None;
        }
        if let Some(before) = &mut action.series {
            if before.same(&SeriesSet::capture(self)) {
                action.series = None;
            } else {
                before.held = pending
                    .removed_series
                    .into_iter()
                    .filter(|item| before.props.iter().any(|(id, ..)| *id == item.series.id))
                    .collect();
            }
        }
        if action.view == Some(self.get_time_scale_state()) {
            action.view = None;
        }
        if action.config.as_ref() == Some(&self.config) {
            action.config = None;
        }
        if action.price_targets_visible == Some(self.show_price_targets) {
            action.price_targets_visible = None;
        }
        if action.main_series_visible == Some(self.main_series_visible) {
            action.main_series_visible = None;
        }
        if action.options.as_ref() == Some(&self.chart_options) {
            action.options = None;
        }
        if action.chart_type.as_ref() == Some(&(self.chart_type, self.chart_type_params.clone())) {
            action.chart_type = None;
        }
        if action.interval == Some(self.timeframe) {
            action.interval = None;
        }
        if let Some(scale) = &action.scale {
            let current = self.state.price_scale();
            if scale.options() == current.options()
                && scale.is_manual() == current.is_manual()
                && (!scale.is_manual() || scale.range() == current.range())
            {
                action.scale = None;
            }
        }
        let known: Vec<_> = action.series_scales.keys().copied().collect();
        action.series_scales.retain(|id, scale| {
            let (Some(scale), Some(current)) = (scale, self.series_scales.get(id)) else {
                return true;
            };
            scale.options() != current.options()
                || scale.is_manual() != current.is_manual()
                || (scale.is_manual() && scale.range() != current.range())
        });
        for id in self.series_scales.keys().filter(|id| !known.contains(id)) {
            action.series_scales.insert(*id, None);
        }
        if let Some(panes) = &action.panes
            && panes.maximized == self.pane_layout.maximized
            && panes.main == self.pane_layout.main
            && panes.panes.len() == self.pane_layout.panes.len()
            && panes
                .panes
                .iter()
                .zip(&self.pane_layout.panes)
                .all(|(a, b)| {
                    a.id == b.id
                        && a.members == b.members
                        && a.stretch == b.stretch
                        && a.height == b.height
                        && a.preserve == b.preserve
                        && a.collapsed == b.collapsed
                        && a.scale.options() == b.scale.options()
                        && a.scale.is_manual() == b.scale.is_manual()
                        && (!a.scale.is_manual() || a.scale.range() == b.scale.range())
                })
        {
            action.panes = None;
        }
        action.studies.retain(
            |change| match (&change.before, Study::capture(self, change.id)) {
                (None, None) => false,
                (Some(before), Some(after)) => !before.same(&after),
                _ => true,
            },
        );
        #[cfg(feature = "drawings")]
        let drawings_changed = action.drawings.can_undo();
        #[cfg(not(feature = "drawings"))]
        let drawings_changed = false;
        if action.symbol.is_none()
            && action.series.is_none()
            && action.view.is_none()
            && action.config.is_none()
            && action.main_series.is_none()
            && action.main_series_visible.is_none()
            && action.price_targets_visible.is_none()
            && action.options.is_none()
            && action.scale.is_none()
            && action.series_scales.is_empty()
            && action.panes.is_none()
            && action.chart_type.is_none()
            && action.interval.is_none()
            && action.studies.is_empty()
            && !drawings_changed
        {
            return;
        }
        // Edits after a symbol change belong to the new symbol, so they are undone first.
        #[cfg(feature = "drawings")]
        let later = action
            .symbol
            .is_some()
            .then(|| Action {
                drawings: std::mem::take(&mut action.drawings),
                ..Action::default()
            })
            .filter(|later| later.drawings.can_undo());
        self.history.push(action);
        #[cfg(feature = "drawings")]
        if let Some(later) = later {
            self.history.push(later);
        }
    }

    /// Ends the drawing record of the pending action before another symbol's drawings take
    /// over. What it recorded belongs to the old symbol, so it becomes an action of its own.
    #[cfg(feature = "drawings")]
    pub(super) fn detach_pending_drawings(&mut self) {
        let Some(pending) = &mut self.history.pending else {
            return;
        };
        self.drawings.swap_history(&mut pending.standalone_drawings);
        let recorded = std::mem::replace(
            &mut pending.standalone_drawings,
            HistoryService::with_max_size(0),
        );
        if recorded.can_undo() {
            self.history.push(Action {
                drawings: recorded,
                ..Action::default()
            });
        }
    }

    /// Starts recording the pending action's drawing edits on the new symbol's drawings.
    #[cfg(feature = "drawings")]
    pub(super) fn attach_pending_drawings(&mut self) {
        if let Some(pending) = &mut self.history.pending {
            let mut recording = HistoryService::with_max_size(0);
            self.drawings.swap_history(&mut recording);
            pending.standalone_drawings = recording;
        }
    }

    /// Keeps a bar series removed by the pending action, bars, price formatter and bar limit
    /// included, so undo restores it. Outside an action they are dropped.
    pub(super) fn retain_removed_series(&mut self, series: PriceSeries) {
        let held = self.hold_series(series);
        if let Some(pending) = &mut self.history.pending {
            pending.removed_series.push(held);
        }
    }

    /// Takes `series`, which has left the chart, with all its bars, its price formatter and its
    /// bar limit.
    fn hold_series(&mut self, mut series: PriceSeries) -> HeldSeries {
        self.replay_release(&mut series);
        let mut held = HeldSeries {
            series,
            formatter: None,
            bar_limit: None,
        };
        self.swap_series_settings(held.series.id, &mut held.formatter, &mut held.bar_limit);
        held
    }

    /// Gives a held series its price formatter and bar limit back as it enters the chart, under
    /// the cutoff of a running replay.
    fn restore_series(&mut self, mut held: HeldSeries) -> PriceSeries {
        self.swap_series_settings(held.series.id, &mut held.formatter, &mut held.bar_limit);
        self.replay_admit(&mut held.series);
        held.series
    }

    /// Takes the main series out of the chart and pauses its feed. The pending action keeps its
    /// price lines, formatter and bar limit, so undo restores them.
    pub(super) fn remove_main_series(&mut self) {
        let mut removed = MainSeries::default();
        self.swap_main_series(&mut removed);
        if let Some(pending) = &mut self.history.pending {
            pending.before.main_series = Some(removed);
        }
    }

    /// Exchanges the main series' presence, price lines, formatter and bar limit with `main`,
    /// so the exchange reverses itself, and pauses or resumes the feed to match. Removing the
    /// series again before the resumed feed sent its bars, or after it failed to, restores the
    /// bars the series left behind, so the remaining series keep their time axis.
    fn swap_main_series(&mut self, main: &mut MainSeries) {
        std::mem::swap(&mut self.main_series_present, &mut main.present);
        std::mem::swap(&mut self.price_lines, &mut main.price_lines);
        self.swap_series_settings(SeriesId::MAIN, &mut main.formatter, &mut main.bar_limit);
        let paused = self.feed_link.as_ref().is_some_and(|link| link.host_paused);
        self.set_feed_paused(paused);
        if self.main_series_present {
            // The resumed feed's reload empties the bars until its page arrives.
            if self.feed_link.as_ref().is_some_and(|link| !link.paused) {
                main.bars = self.state.data().bars.clone();
            }
        } else {
            let kept = std::mem::take(&mut main.bars);
            if self.state.data().bars.is_empty() && !kept.is_empty() {
                self.replace_main_data(kept);
            }
        }
    }

    /// Exchanges the price formatter and bar limit of the bar series `id` with `formatter` and
    /// `bar_limit`, so the exchange reverses itself.
    fn swap_series_settings(
        &mut self,
        id: SeriesId,
        formatter: &mut Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
        bar_limit: &mut Option<std::num::NonZeroUsize>,
    ) {
        *formatter = match formatter.take() {
            Some(formatter) => self.series_formatters.insert(id, formatter),
            None => self.series_formatters.remove(&id),
        };
        *bar_limit = match bar_limit.take() {
            Some(limit) => self.bar_limits.insert(id, limit),
            None => self.bar_limits.remove(&id),
        };
    }

    pub(super) fn record_study(&mut self, id: SeriesId) {
        self.element_studies.dirty = true;
        if self
            .history
            .pending
            .as_ref()
            .is_some_and(|pending| !pending.before.studies.iter().any(|change| change.id == id))
        {
            let mut before = Study::capture(self, id);
            if let Some(study) = &mut before {
                study.index = self
                    .history
                    .pending
                    .as_ref()
                    .unwrap()
                    .study_order
                    .iter()
                    .position(|candidate| *candidate == id)
                    .unwrap_or(study.index);
            }
            self.history
                .pending
                .as_mut()
                .unwrap()
                .before
                .studies
                .push(StudyChange { id, before });
        }
    }

    pub(super) fn retain_removed_study(&mut self, id: SeriesId, indicator: Box<dyn Indicator>) {
        let change = if let Some(pending) = &mut self.history.pending {
            pending
                .before
                .studies
                .iter_mut()
                .find(|change| change.id == id)
        } else {
            // Host removal does not record an action, but existing edits must remain replayable.
            self.history
                .undo
                .iter_mut()
                .rev()
                .chain(self.history.redo.iter_mut().rev())
                .flat_map(|action| &mut action.studies)
                .find(|change| change.id == id && change.before.is_some())
        };
        if let Some(study) = change.and_then(|change| change.before.as_mut()) {
            study.indicator = Some(indicator);
        }
    }

    /// Exchanges the added series with those of `set`. A series the chart lacks comes from the
    /// set whole, and one the set lacks goes into it, so the exchange reverses itself.
    fn swap_series(&mut self, set: &mut SeriesSet) {
        let now = SeriesSet::capture(self);
        let mut pool = std::mem::take(&mut self.price_series);
        let mut held = std::mem::take(&mut set.held);
        for (id, options, visible) in set.props.drain(..) {
            let mut item = if let Some(at) = pool.iter().position(|item| item.id == id) {
                pool.swap_remove(at)
            } else if let Some(at) = held.iter().position(|item| item.series.id == id) {
                self.restore_series(held.swap_remove(at))
            } else {
                continue;
            };
            item.options = options;
            item.visible = visible;
            item.revision = crate::model::next_revision();
            self.price_series.push(item);
        }
        // The series the set lacks leave the chart with their formatters and limits.
        for item in pool {
            held.push(self.hold_series(item));
        }
        self.main_series_scale = set.main_scale;
        self.main_series_style.alerts = set.main_alerts;
        *set = SeriesSet { held, ..now };
        // `set.props` now lists the series shown before the exchange, in draw order.
        for (id, ..) in &set.props {
            if self.price_series.iter().all(|item| item.id != *id) {
                self.host_events.push(ChartEvent::SeriesRemoved(*id));
            }
        }
        for item in &self.price_series {
            if set.props.iter().all(|(id, ..)| *id != item.id) {
                self.host_events.push(ChartEvent::SeriesAdded(item.id));
            }
        }
        if let Some(ChartElementId::Series(id)) = self.selection.selected_id()
            && self.price_series.iter().all(|item| item.id != id)
        {
            self.selection.deselect();
        }
    }

    /// Whether a chart user action can be undone (also drives toolbar availability).
    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    /// Whether a chart user action can be redone (also drives toolbar availability).
    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    /// Undo the most recent chart user action, across drawings, studies and settings.
    /// Returns false while a user action is in progress; finish that action first.
    pub fn undo(&mut self) -> bool {
        self.step_chart_history(false)
    }

    /// Redo the most recently undone chart user action.
    /// Returns false while a user action is in progress; finish that action first.
    pub fn redo(&mut self) -> bool {
        self.step_chart_history(true)
    }

    /// Drain a notification that undo/redo changed chart state. Hosts that own the data must
    /// reload it if [`Self::timeframe`] or [`Self::bar_clock`] changed, and synchronize their UI.
    /// Added series that the step removed or brought back are reported by
    /// [`drain_events`](Self::drain_events) as [`ChartEvent::SeriesRemoved`] and
    /// [`ChartEvent::SeriesAdded`].
    pub fn take_history_changed(&mut self) -> bool {
        std::mem::take(&mut self.history.changed)
    }

    /// A count that changes with every user edit of the chart, undo and redo included, and with
    /// every change of symbol. Compare it with the count at the last save to tell whether the
    /// chart changed since, for example to save a layout automatically.
    pub fn edit_count(&self) -> u64 {
        self.history.edits
    }

    fn step_chart_history(&mut self, redo: bool) -> bool {
        if self.history.pending.is_some() {
            return false;
        }
        let stack = if redo {
            &mut self.history.redo
        } else {
            &mut self.history.undo
        };
        let Some(mut action) = stack.pop() else {
            return false;
        };
        // Validate on temporary calculations before changing chart state. A host study may
        // reject a previously accepted input; failed replay leaves both history stacks intact.
        let restore: Option<Vec<_>> = action
            .studies
            .iter()
            .filter_map(|change| change.before.as_ref().map(|study| (change.id, study)))
            .map(|(id, study)| {
                let original = self
                    .study_ids
                    .iter()
                    .position(|candidate| *candidate == id)
                    .map(|at| self.studies.indicators()[at].as_ref())
                    .or(study.indicator.as_deref())
                    .or_else(|| {
                        self.history
                            .undo
                            .iter()
                            .chain(&self.history.redo)
                            .flat_map(|action| &action.studies)
                            .filter(|change| change.id == id)
                            .find_map(|change| change.before.as_ref()?.indicator.as_deref())
                    })?;
                let mut indicator = original.clone_box();
                indicator.restore_settings(&study.settings).ok()?;
                Some((
                    id,
                    study.index,
                    study.inputs.clone(),
                    study.autoscale,
                    indicator,
                ))
            })
            .collect();
        let Some(mut restore) = restore else {
            if redo {
                self.history.redo.push(action);
            } else {
                self.history.undo.push(action);
            }
            return false;
        };
        let current_view = self.get_time_scale_state();
        let mut current: Vec<_> = action
            .studies
            .iter()
            .map(|change| Study::capture(self, change.id))
            .collect();
        if let Some(symbol) = &mut action.symbol {
            let target = std::mem::replace(symbol, self.symbol.clone());
            self.change_symbol(target, true);
        }
        if let Some((kind, params)) = &mut action.chart_type {
            let current = (self.chart_type, self.chart_type_params.clone());
            self.set_chart_type(*kind);
            self.set_chart_type_params(params.clone());
            (*kind, *params) = current;
        }
        if let Some(interval) = &mut action.interval {
            let current = self.timeframe;
            if let Some(timeframe) = *interval {
                self.set_timeframe(timeframe);
            } else {
                self.timeframe = None;
                #[cfg(feature = "drawings")]
                self.drawings.curr_timeframe.clear();
            }
            *interval = current;
        }
        if let Some(config) = &mut action.config {
            std::mem::swap(&mut self.config, config);
        }
        if let Some(visible) = &mut action.price_targets_visible {
            std::mem::swap(&mut self.show_price_targets, visible);
        }
        // Before the visibility, which an absent main series ignores.
        if let Some(main) = &mut action.main_series {
            self.swap_main_series(main);
        }
        if let Some(visible) = &mut action.main_series_visible {
            let current = self.main_series_visible;
            self.set_main_series_visible(*visible);
            *visible = current;
        }
        if let Some(options) = &mut action.options {
            std::mem::swap(&mut self.chart_options, options);
            let (spacing, offset) = self.get_time_scale_state();
            let opt = &self.chart_options.time_scale;
            self.state.time_scale_mut().apply_options(
                spacing,
                opt.min_bar_spacing,
                opt.max_bar_spacing,
                opt.fix_left_edge,
                opt.fix_right_edge,
                offset,
                None,
            );
        }
        if let Some(view) = &mut action.view {
            self.apply_synced_time_scale(view.0, view.1);
            *view = current_view;
            self.pending_scroll_time = None;
            self.tracking_mode_active = false;
        }
        if let Some(scale) = &mut action.scale {
            std::mem::swap(self.state.price_scale_mut(), scale);
        }
        for (id, scale) in &mut action.series_scales {
            let current = self.series_scales.remove(id);
            if let Some(scale) = scale.take() {
                self.series_scales.insert(*id, scale);
            }
            *scale = current;
        }
        if let Some(set) = &mut action.series {
            self.swap_series(set);
        }
        self.element_studies.dirty |= !action.studies.is_empty();
        // Restore by original index after removing every touched study, so bulk removal and
        // templates retain order and stable ids while unrelated host studies stay present.
        let show_volume = self.config.show_volume;
        for (change, current) in action.studies.iter().zip(&mut current) {
            let indicator = self.take_study(change.id);
            if change.before.is_none()
                && let Some(current) = current
            {
                current.indicator = indicator;
            }
        }
        restore.sort_by_key(|(_, index, ..)| *index);
        for (id, index, inputs, autoscale, mut indicator) in restore {
            if let Some(timeframe) = self.timeframe {
                indicator.set_timeframe(timeframe);
            }
            indicator.set_instrument(self.instrument());
            indicator.set_bar_clock(&self.bar_clock());
            let at = index.min(self.study_ids.len());
            // The registry exposes a slice; append then rotate to insert at the saved position.
            self.studies.add(indicator);
            self.studies.indicators_mut()[at..].rotate_right(1);
            self.study_ids.insert(at, id);
            self.study_inputs.insert(at, inputs);
            self.study_autoscale.insert(at, autoscale);
            self.pending_from.insert(at, None);
            self.next_refresh.insert(at, f64::NEG_INFINITY);
        }
        self.recalculate_studies();
        // Replacing a study during history replay is not a volume-switch edit.
        self.config.show_volume = show_volume;
        for (change, before) in action.studies.iter_mut().zip(current) {
            change.before = before;
        }
        if let Some(panes) = &mut action.panes {
            std::mem::swap(&mut self.pane_layout, panes);
        }
        self.sync_panes();
        if let Some(ChartElementId::Indicator(id)) = self.selection.selected_id()
            && !self.study_ids.contains(&id)
        {
            self.selection.deselect();
        }
        #[cfg(feature = "drawings")]
        {
            self.drawings.swap_history(&mut action.drawings);
            if redo {
                while self.drawings.redo() {}
            } else {
                while self.drawings.undo() {}
            }
            self.drawings.swap_history(&mut action.drawings);
        }
        self.history.edits += 1;
        #[cfg(feature = "drawings")]
        if action.drawings.can_undo() || action.drawings.can_redo() {
            self.history.drawing_edits += 1;
        }
        if redo {
            self.history.undo.push(action);
        } else {
            self.history.redo.push(action);
        }
        self.history.changed = true;
        true
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    #[cfg(feature = "drawings")]
    use crate::drawings::manager::DrawingSyncAction;
    #[cfg(feature = "drawings")]
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::model::BarData;
    use crate::studies::{InputValue, RSI, SMA};
    use crate::widget::test_frames::chart;
    use egui::Color32;
    #[cfg(feature = "drawings")]
    use egui::{Context, Event, Key, Modifiers, RawInput};

    /// A valid host trend line for history actions that do not test its geometry.
    #[cfg(feature = "drawings")]
    fn host_line() -> Drawing {
        let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(1.0, 15.0), ChartPoint::new(2.0, 16.0)];
        drawing
    }

    #[test]
    fn study_history_records_settings_without_cloning_calculations() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        #[derive(Clone)]
        struct Counted(SMA, Arc<AtomicUsize>);
        impl Indicator for Counted {
            fn name(&self) -> &str {
                self.0.name()
            }
            fn calculate(&mut self, bars: &[crate::model::Bar]) {
                self.0.calculate(bars);
            }
            fn values(&self) -> &crate::studies::IndicatorValues {
                self.0.values()
            }
            fn colors(&self) -> Vec<Color32> {
                self.0.colors()
            }
            fn set_colors(&mut self, colors: Vec<Color32>) {
                self.0.set_colors(colors);
            }
            fn set_visible(&mut self, visible: bool) {
                self.0.set_visible(visible);
            }
            fn inputs(&self) -> Vec<crate::studies::StudyInput> {
                self.0.inputs()
            }
            fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
                self.0.set_input(id, value)
            }
            fn clone_box(&self) -> Box<dyn Indicator> {
                self.1.fetch_add(1, Ordering::Relaxed);
                Box::new(self.clone())
            }
        }

        let clones = Arc::new(AtomicUsize::new(0));
        let mut chart = chart(&[10.0; 30], &[20.0; 30]);
        let id = chart.add_indicator(Box::new(Counted(SMA::new(2), clones.clone())));
        let original = chart.indicators().next().unwrap().1.colors();
        for red in 1..=30 {
            chart.user_action(|chart| {
                chart
                    .indicator_mut(id)
                    .unwrap()
                    .set_colors(vec![Color32::from_rgb(red, 0, 0)])
            });
        }
        assert_eq!(
            clones.load(Ordering::Relaxed),
            0,
            "recording settings must not copy calculated histories"
        );
        // History must recalculate from today's closes, not restore the old output column.
        let mut bars = chart.bars(SeriesId::MAIN)[..4].to_vec();
        for (bar, close) in bars.iter_mut().zip([10.0, 14.0, 18.0, 22.0]) {
            bar.close = close;
            bar.high = close + 1.0;
            bar.low = close - 1.0;
            bar.open = close;
        }
        chart.set_data(SeriesId::MAIN, bars);
        for _ in 0..30 {
            assert!(chart.undo());
            assert_eq!(
                chart.indicators().next().unwrap().1.values().value(0, 3),
                Some(20.0)
            );
        }
        assert_eq!(chart.indicators().next().unwrap().1.colors(), original);
        for red in 1..=30 {
            assert!(chart.redo());
            assert_eq!(
                chart.indicators().next().unwrap().1.colors(),
                [Color32::from_rgb(red, 0, 0)]
            );
            assert_eq!(
                chart.indicators().next().unwrap().1.values().value(0, 3),
                Some(20.0)
            );
        }
        let before_removal = clones.load(Ordering::Relaxed);
        chart.user_action(|chart| {
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input("period", InputValue::Length(3))
                .unwrap();
            chart.remove_series(id);
        });
        assert_eq!(
            clones.load(Ordering::Relaxed),
            before_removal,
            "removal moves the original implementation into history"
        );
        assert!(chart.undo());
        assert_eq!(chart.study_ids, [id]);
        assert_eq!(
            chart.indicators().next().unwrap().1.values().value(0, 3),
            Some(20.0)
        );
        assert!(chart.redo());
        assert!(chart.indicators().next().is_none());
        assert!(chart.undo());
        // An ordinary host removal must not make earlier appearance edits unreplayable.
        assert!(chart.remove_series(id));
        assert!(chart.undo());
        assert_eq!(chart.study_ids, [id]);
        assert_eq!(
            chart.indicators().next().unwrap().1.colors(),
            [Color32::from_rgb(29, 0, 0)]
        );
        assert!(chart.redo());
        assert!(chart.indicators().next().is_none());
    }

    #[test]
    fn study_history_restores_visibility_before_an_interval_is_set() {
        let mut chart = chart(&[10.0; 4], &[20.0; 4]);
        let id = chart.add_indicator(Box::new(SMA::new(2)));
        let study = chart.indicator_mut(id).unwrap();
        let mut settings = study.settings();
        settings.hidden_intervals = vec![Timeframe::Min1];
        study.apply_settings(&[], settings).unwrap();
        assert!(chart.indicators().next().unwrap().1.is_visible());
        chart.user_action(|chart| {
            chart
                .indicator_mut(id)
                .unwrap()
                .set_colors(vec![Color32::RED]);
            chart.set_timeframe(Timeframe::Min1);
        });
        for redo in [false, true, false] {
            assert!(if redo { chart.redo() } else { chart.undo() });
            assert_eq!(chart.timeframe(), redo.then_some(Timeframe::Min1));
            assert_eq!(chart.indicators().next().unwrap().1.is_visible(), !redo);
        }
    }

    #[test]
    fn study_history_restores_inputs_with_dependent_limits() {
        use crate::studies::{AverageLine, AverageType, MACombination};

        let mut chart = chart(&[10.0, 14.0, 18.0, 22.0], &[10.0, 14.0, 18.0, 22.0]);
        let id = chart.add_indicator(Box::new(
            MACombination::new(vec![
                AverageLine::new(AverageType::Simple, 1, Color32::RED),
                AverageLine::new(AverageType::Simple, 2, Color32::BLUE),
            ])
            .unwrap(),
        ));
        chart.user_action(|chart| {
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input("kind_0", InputValue::Choice(5))
                .unwrap();
        });
        for redo in [false, true, false, true] {
            assert!(if redo { chart.redo() } else { chart.undo() });
            let study = chart.indicators().next().unwrap().1;
            let input = |id: &str| {
                study
                    .inputs()
                    .into_iter()
                    .find(|input| input.id == id)
                    .unwrap()
                    .value
            };
            assert_eq!(
                input("period_0"),
                InputValue::Length(if redo { 2 } else { 1 })
            );
            assert_eq!(
                input("kind_0"),
                InputValue::Choice(if redo { 5 } else { 0 })
            );
            assert_eq!(
                study.values().value(0, 3),
                Some(if redo { 70.0 / 3.0 } else { 22.0 })
            );
        }
    }

    #[test]
    fn study_edits_and_bulk_removal_restore_ids_order_and_settings() {
        let mut chart = chart(&[10.0; 30], &[20.0; 30]);
        chart.set_timeframe(Timeframe::Min1);
        let rsi = chart.user_action(|chart| {
            let id = chart.add_indicator(Box::new(RSI::new(14)));
            chart.set_indicator_inputs(id, "14");
            id
        });
        let sma = chart.user_action(|chart| chart.add_indicator(Box::new(SMA::new(3))));
        chart.user_action(|chart| {
            let study = chart.indicator_mut(rsi).unwrap();
            let mut settings = study.settings();
            settings.plots[0].color = Color32::RED;
            settings.plots[0].width = 4.0;
            settings.hidden_intervals = vec![Timeframe::Min1];
            study
                .apply_settings(&[("period".into(), InputValue::Length(5))], settings)
                .unwrap();
            chart.set_indicator_inputs(rsi, "5");
            chart.set_indicator_autoscale(rsi, false);
        });
        assert!(chart.undo());
        assert_eq!(chart.indicator_inputs(rsi), Some("14"));
        assert_eq!(
            chart.indicators().next().unwrap().1.inputs()[0].value,
            InputValue::Length(14)
        );
        assert!(chart.study_autoscale[0]);
        assert!(chart.indicators().next().unwrap().1.is_visible());
        assert!(chart.redo());
        assert_eq!(chart.indicator_inputs(rsi), Some("5"));
        assert_eq!(
            chart.indicators().next().unwrap().1.settings().plots[0].color,
            Color32::RED
        );
        assert!(!chart.study_autoscale[0]);
        assert!(!chart.indicators().next().unwrap().1.is_visible());
        chart.set_timeframe(Timeframe::Min15);
        assert!(
            chart.indicators().next().unwrap().1.is_visible(),
            "interval filtering must not overwrite the global visibility toggle"
        );
        chart.user_action(|chart| assert!(chart.move_indicator_to_pane(sma, rsi)));
        assert_eq!(chart.pane_layout.panes[1].members, [rsi, sma]);
        assert!(chart.undo());
        assert!(
            chart
                .indicators()
                .find(|(id, _)| *id == sma)
                .unwrap()
                .1
                .is_overlay()
        );
        assert_eq!(chart.pane_layout.panes[1].members, [rsi]);
        assert!(chart.redo());
        assert_eq!(chart.pane_layout.panes[1].members, [rsi, sma]);
        chart.user_action(|chart| {
            chart.remove_series(rsi);
            chart.remove_series(sma);
        });
        // A host study added after the action is independent of both sides of its history.
        let host = chart.add_indicator(Box::new(SMA::new(7)));
        let bars = chart.bars(SeriesId::MAIN)[..17].to_vec();
        chart.set_data(SeriesId::MAIN, bars);
        for _ in 0..2 {
            assert!(chart.undo());
            assert_eq!(chart.study_ids, [rsi, sma, host]);
            assert_eq!(chart.pane_layout.panes[1].members, [rsi, sma]);
            assert_eq!(chart.indicator_inputs(rsi), Some("5"));
            assert_eq!(chart.indicators().next().unwrap().1.values().len(), 17);
            assert!(chart.redo());
            assert_eq!(chart.study_ids, [host]);
        }
    }

    #[test]
    fn interleaved_interval_type_and_settings_follow_action_order() {
        let mut chart = Chart::new(BarData::new());
        chart.set_timeframe(Timeframe::Min1);
        let original = chart.config.clone();
        chart.user_action(|chart| chart.set_timeframe(Timeframe::Min15));
        chart.user_action(|chart| {
            chart.config_mut().background_color = Color32::RED;
            chart.price_scale_mut().options_mut().invert_scale = true;
            chart.chart_options.time_scale.timezone =
                crate::scales::TimezoneMode::Timezone("+05:45".parse().unwrap());
        });
        chart.user_action(|chart| chart.set_timeframe(Timeframe::Hour1));
        chart.user_action(|chart| chart.set_chart_type(ChartType::Line));
        chart.user_action(|chart| chart.config_mut().show_volume = !original.show_volume);
        for redo in [false, true, false] {
            let expected = [
                (
                    Timeframe::Hour1,
                    ChartType::Line,
                    true,
                    original.show_volume,
                ),
                (
                    Timeframe::Hour1,
                    ChartType::Candles,
                    true,
                    original.show_volume,
                ),
                (
                    Timeframe::Min15,
                    ChartType::Candles,
                    true,
                    original.show_volume,
                ),
                (
                    Timeframe::Min15,
                    ChartType::Candles,
                    false,
                    original.show_volume,
                ),
                (
                    Timeframe::Min1,
                    ChartType::Candles,
                    false,
                    original.show_volume,
                ),
            ];
            if redo {
                for (tf, kind, red, volume) in [
                    (
                        Timeframe::Min15,
                        ChartType::Candles,
                        false,
                        original.show_volume,
                    ),
                    (
                        Timeframe::Min15,
                        ChartType::Candles,
                        true,
                        original.show_volume,
                    ),
                    (
                        Timeframe::Hour1,
                        ChartType::Candles,
                        true,
                        original.show_volume,
                    ),
                    (
                        Timeframe::Hour1,
                        ChartType::Line,
                        true,
                        original.show_volume,
                    ),
                    (
                        Timeframe::Hour1,
                        ChartType::Line,
                        true,
                        !original.show_volume,
                    ),
                ] {
                    assert!(chart.redo());
                    assert_eq!(chart.timeframe(), Some(tf));
                    assert_eq!(chart.chart_type(), kind);
                    assert_eq!(chart.config.background_color == Color32::RED, red);
                    assert_eq!(chart.config.show_volume, volume);
                }
            } else {
                for (tf, kind, red, volume) in expected {
                    assert!(chart.undo());
                    assert_eq!(chart.timeframe(), Some(tf));
                    assert_eq!(chart.chart_type(), kind);
                    assert_eq!(chart.config.background_color == Color32::RED, red);
                    assert_eq!(chart.config.show_volume, volume);
                    assert_eq!(chart.state.price_scale().options().invert_scale, red);
                    assert_eq!(
                        chart.chart_options.time_scale.timezone
                            == crate::scales::TimezoneMode::Timezone("+05:45".parse().unwrap()),
                        red
                    );
                }
                assert!(!chart.can_undo());
            }
        }
    }

    #[test]
    fn display_options_restore_applied_spacing_but_keep_pan_for_timezone_edits() {
        let mut chart = chart(&[10.0], &[20.0]);
        chart.apply_synced_time_scale(9.0, -7.0);
        let edit_options = |chart: &mut Chart, spacing: Option<f32>| {
            let mut options = chart.chart_options().clone();
            if let Some(spacing) = spacing {
                options.time_scale.bar_spacing = spacing;
            }
            options.time_scale.timezone =
                crate::scales::TimezoneMode::Timezone("+05:45".parse().unwrap());
            let before = std::mem::replace(chart, Chart::new(BarData::new()));
            *chart = before.with_chart_options(options);
        };
        chart.user_action(|chart| edit_options(chart, Some(12.0)));
        assert_eq!(chart.get_time_scale_state().0, 12.0);
        assert!(chart.undo());
        assert_eq!(chart.get_time_scale_state().0, 9.0);
        assert!(chart.redo());
        assert_eq!(chart.get_time_scale_state().0, 12.0);
        // A display-only timezone edit uses the same builder but preserves the live view.
        chart.chart_options.time_scale.timezone = crate::scales::TimezoneMode::Utc;
        chart.user_action(|chart| edit_options(chart, None));
        chart.apply_synced_time_scale(15.0, -11.0);
        assert!(chart.undo());
        assert_eq!(chart.get_time_scale_state(), (15.0, -11.0));
        assert!(chart.redo());
        assert_eq!(chart.get_time_scale_state(), (15.0, -11.0));
    }

    #[test]
    #[cfg(feature = "drawings")]
    fn host_updates_and_unchanged_actions_do_not_enter_or_clear_history() {
        let mut chart = Chart::new(BarData::new());
        chart.set_timeframe(Timeframe::Min1);
        chart.config_mut().show_grid = false;
        let host = chart.add_indicator(Box::new(SMA::new(3)));
        chart
            .drawings_mut()
            .add_synced_drawing(host_line())
            .unwrap();
        assert!(!chart.can_undo());
        chart.user_action(|chart| {
            chart.user_action(|chart| chart.set_timeframe(Timeframe::Min15));
            chart.add_indicator(Box::new(RSI::new(14)));
        });
        assert!(chart.undo());
        assert!(!chart.can_undo(), "nested edits are one user action");
        assert_eq!(chart.study_ids, [host]);
        assert!(!chart.config.show_grid);
        chart.user_action(|chart| {
            assert!(
                !chart.undo(),
                "an open action cannot consume completed history"
            );
            chart.set_timeframe(Timeframe::Min1);
            chart.indicator_mut(host).unwrap().set_overlay(true);
            chart.set_indicator_scale(host, crate::studies::StudyScale::Main);
            chart
                .set_indicator_source(
                    host,
                    crate::studies::StudySource::Price(crate::model::PriceSource::Close),
                )
                .unwrap();
        });
        chart.set_data(SeriesId::MAIN, vec![]);
        chart.set_chart_type(ChartType::Area);
        assert!(!chart.can_undo());
        assert!(chart.can_redo(), "host and unchanged edits retain redo");
        assert!(chart.redo());
        assert_eq!(chart.chart_type(), ChartType::Area);
        assert_eq!(chart.timeframe(), Some(Timeframe::Min15));
        assert!(chart.undo());
        chart.user_action(|chart| chart.config_mut().show_volume = false);
        assert!(
            !chart.can_redo(),
            "a new user edit clears cross-domain redo"
        );
    }

    #[test]
    #[cfg(feature = "drawings")]
    fn drawing_snapshots_keep_host_loads_sync_events_and_prepended_coordinates() {
        let mut chart = chart(&[10.0, 11.0], &[20.0, 21.0]);
        chart.user_action(|chart| {
            let mut drawing = Drawing::new(0, DrawingToolType::HorizontalLine);
            drawing.chart_points = vec![ChartPoint::new(1.0, 15.0)];
            chart.drawings_mut().add_synced_drawing(drawing).unwrap();
        });
        chart.user_action(|chart| chart.drawings_mut().clear_all());
        let mut host = host_line();
        host.stored_id = Some("host".into());
        chart.drawings_mut().add_synced_drawing(host).unwrap();
        chart.drawings.pending_sync.lock().unwrap().clear();
        // Prepending real bars remaps commands on both stacks, as it does live drawings.
        let prepend = |chart: &mut Chart, count| {
            let mut bars = chart.bars(SeriesId::MAIN).to_vec();
            let first = bars[0].clone();
            for days in 1..=count {
                let mut older = first.clone();
                older.time -= chrono::Duration::days(days);
                bars.push(older);
            }
            chart.set_data(SeriesId::MAIN, bars);
        };
        prepend(&mut chart, 3);
        assert!(chart.undo());
        assert_eq!(chart.drawings.drawings.len(), 2);
        assert_eq!(chart.drawings.drawings[0].chart_points[0].bar_idx, 4.0);
        assert!(matches!(
            chart.drawings.pending_sync.lock().unwrap().as_slice(),
            [DrawingSyncAction::SaveDrawing(_)]
        ));
        assert!(chart.undo());
        assert_eq!(chart.drawings.drawings.len(), 1);
        assert_eq!(
            chart.drawings.drawings[0].stored_id.as_deref(),
            Some("host")
        );
        prepend(&mut chart, 2);
        assert!(chart.redo());
        assert_eq!(
            chart.drawings.drawings.last().unwrap().chart_points[0].bar_idx,
            6.0
        );
        assert!(chart.redo());
        assert_eq!(chart.drawings.drawings.len(), 1);
        assert_eq!(
            chart.drawings.drawings[0].stored_id.as_deref(),
            Some("host")
        );
    }

    #[test]
    #[cfg(feature = "drawings")]
    fn command_z_y_and_shift_z_share_the_chart_history_on_both_platforms() {
        for mac in [false, true] {
            let ctx = Context::default();
            ctx.set_os(if mac {
                egui::os::OperatingSystem::Mac
            } else {
                egui::os::OperatingSystem::Nix
            });
            let command = Modifiers {
                command: true,
                mac_cmd: mac,
                ctrl: !mac,
                ..Modifiers::NONE
            };
            let mut chart = Chart::new(BarData::new());
            chart.user_action(|chart| {
                chart
                    .drawings_mut()
                    .add_synced_drawing(host_line())
                    .unwrap()
            });
            let id = chart.user_action(|chart| chart.add_indicator(Box::new(RSI::new(14))));
            for (key, shift, studies, drawings) in [
                (Key::Z, false, 0, 1),
                (Key::Y, false, 1, 1),
                (Key::Z, false, 0, 1),
                (Key::Z, true, 1, 1),
                (Key::Z, false, 0, 1),
                (Key::Z, false, 0, 0),
                (Key::Y, false, 0, 1),
                (Key::Y, false, 1, 1),
            ] {
                let modifiers = Modifiers { shift, ..command };
                ctx.run_ui(
                    RawInput {
                        events: vec![
                            Event::ModifiersChanged(modifiers),
                            Event::Key {
                                key,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers,
                            },
                        ],
                        ..RawInput::default()
                    },
                    |ui| chart.dispatch_shortcuts(ui.ctx()),
                )
                .drop_without_applying_deltas();
                assert_eq!(
                    chart.indicators().count(),
                    studies,
                    "mac {mac}, {key:?}, shift {shift}"
                );
                assert_eq!(chart.drawings().drawings.len(), drawings);
                if studies == 1 {
                    assert_eq!(chart.indicators().next().unwrap().0, id);
                }
            }
        }
    }

    /// A user's symbol navigation and comparison edits undo and redo in order, each step finding
    /// the drawings, series, scales and symbol it was recorded against, and a removed comparison
    /// coming back with its price formatter and bar limit.
    #[test]
    #[cfg(feature = "drawings")]
    fn symbol_navigation_and_comparison_edits_undo_and_redo_in_order() {
        use crate::model::SeriesScale;
        use crate::scales::PriceScaleMode;
        let mut chart = chart(&[10.0, 11.0], &[20.0, 21.0]);
        chart.set_symbol("AAA");
        let draw = |chart: &mut Chart| {
            chart.user_action(|chart| {
                chart
                    .drawings_mut()
                    .add_synced_drawing(host_line())
                    .unwrap()
            })
        };
        draw(&mut chart);
        chart.user_action(|chart| chart.set_symbol("BBB"));
        assert!(chart.drawings().drawings.is_empty());
        let bars = chart.bars(SeriesId::MAIN).to_vec();
        let mode = chart.state.price_scale().options().mode;
        let limit = std::num::NonZeroUsize::new(1000);
        let comparison = chart.user_action(|chart| {
            let id = chart.add_comparison("CCC", Color32::GOLD);
            chart.set_data(id, bars.clone());
            chart.set_bar_limit(id, limit);
            assert!(chart.set_series_formatter(
                id,
                Some(std::sync::Arc::new(crate::scales::CurrencyFormatter::usd())),
            ));
            id
        });
        chart.user_action(|chart| {
            chart.set_series_visible(comparison, false);
            chart.set_series_scale(comparison, SeriesScale::Left);
        });
        chart.user_action(|chart| {
            chart.remove_series(comparison);
        });
        draw(&mut chart);

        let series_events = |chart: &mut Chart| {
            chart
                .drain_events()
                .filter(|event| {
                    matches!(
                        event,
                        ChartEvent::SeriesAdded(_) | ChartEvent::SeriesRemoved(_)
                    )
                })
                .collect::<Vec<_>>()
        };
        // Undo reverses removal then addition; redo follows the original edits.
        let added_then_removed = [
            ChartEvent::SeriesAdded(comparison),
            ChartEvent::SeriesRemoved(comparison),
        ];
        assert_eq!(series_events(&mut chart), added_then_removed);

        let comparison_state = |chart: &Chart| {
            (
                chart.series_scale(comparison),
                chart.is_series_visible(comparison),
                chart.bars(comparison).len(),
                chart.bar_limit(comparison),
                chart
                    .series_label_formatter(comparison, PriceScaleMode::Normal)
                    .is_some(),
            )
        };
        let absent = (None, false, 0, None, false);
        let undo = |chart: &mut Chart| assert!(chart.undo());
        undo(&mut chart);
        assert_eq!(
            (chart.symbol(), chart.drawings().drawings.len()),
            ("BBB", 0)
        );
        undo(&mut chart);
        assert_eq!(
            comparison_state(&chart),
            (Some(SeriesScale::Left), false, bars.len(), limit, true)
        );
        undo(&mut chart);
        assert_eq!(
            comparison_state(&chart),
            (Some(SeriesScale::Right), true, bars.len(), limit, true)
        );
        assert_eq!(
            chart.state.price_scale().options().mode,
            PriceScaleMode::Percentage
        );
        undo(&mut chart);
        assert_eq!(chart.series().count(), 0);
        assert_eq!(comparison_state(&chart), absent);
        assert_eq!(chart.state.price_scale().options().mode, mode);
        undo(&mut chart);
        assert_eq!(
            (chart.symbol(), chart.drawings().drawings.len()),
            ("AAA", 1)
        );
        undo(&mut chart);
        assert!(chart.drawings().drawings.is_empty());
        assert!(!chart.can_undo());

        assert_eq!(series_events(&mut chart), added_then_removed);
        assert!(chart.redo());
        assert_eq!(
            (chart.symbol(), chart.drawings().drawings.len()),
            ("AAA", 1)
        );
        assert!(chart.redo());
        assert_eq!(
            (chart.symbol(), chart.drawings().drawings.len()),
            ("BBB", 0)
        );
        assert!(chart.redo());
        assert_eq!(
            comparison_state(&chart),
            (Some(SeriesScale::Right), true, bars.len(), limit, true)
        );
        assert!(chart.redo());
        assert_eq!(
            comparison_state(&chart),
            (Some(SeriesScale::Left), false, bars.len(), limit, true)
        );
        assert!(chart.redo());
        assert_eq!(chart.series().count(), 0);
        assert_eq!(comparison_state(&chart), absent);
        assert!(chart.redo());
        assert_eq!(
            (chart.symbol(), chart.drawings().drawings.len()),
            ("BBB", 1)
        );
        assert!(!chart.can_redo());
        assert_eq!(series_events(&mut chart), added_then_removed);
    }

    #[test]
    fn series_alert_styles_stay_distinct_through_undo_and_snapshot() {
        let mut chart = chart(&[1.0], &[2.0]);
        let other = chart.add_series(crate::widget::price_series::SeriesOptions::default());
        let main_alerts = SeriesAlerts {
            visible: false,
            labels: false,
            color: Color32::RED,
        };
        let other_alerts = SeriesAlerts {
            color: Color32::BLUE,
            ..SeriesAlerts::default()
        };
        chart.user_action(|chart| {
            let mut settings = chart.series_settings();
            settings.alerts = main_alerts;
            chart.apply_series_settings(&settings);
            chart.series_style_mut(other).unwrap().alerts = other_alerts;
        });
        assert_eq!(chart.series_settings().alerts, main_alerts);
        assert_eq!(chart.series_style(other).unwrap().alerts, other_alerts);
        let mut restored = super::super::Chart::new(BarData::default());
        restored
            .restore(&chart.snapshot().unwrap())
            .expect("snapshot restores");
        assert_eq!(restored.series_settings().alerts, main_alerts);
        assert!(chart.undo());
        assert_eq!(chart.series_settings().alerts, SeriesAlerts::default());
        assert_eq!(
            chart.series_style(other).unwrap().alerts,
            SeriesAlerts::default()
        );
        assert!(chart.redo());
        assert_eq!(chart.series_settings().alerts, main_alerts);
        assert_eq!(chart.series_style(other).unwrap().alerts, other_alerts);
    }

    /// Undo of the main series' removal, as its legend row's Remove control records it, brings
    /// back its 100 bars, its visibility, its marker, its price line, its formatter and its bar
    /// limit, and the feed follows the host's pause again: running when the host left it
    /// running, also after a replay that began while the series was removed, and paused when
    /// the host paused it. Redo removes the series again, and a second undo restores it again.
    #[test]
    fn undoing_the_main_series_removal_restores_it_and_its_feed() {
        struct Feed;
        impl crate::data::Datafeed for Feed {
            fn history(
                &self,
                _: crate::data::HistoryRequest,
                _: crate::data::Reply<crate::data::HistoryPage>,
            ) {
            }
        }
        for (host_paused, replay) in [(false, false), (true, false), (false, true)] {
            let mut chart = chart(&[10.0], &[20.0]);
            chart.attach_feed(std::sync::Arc::new(Feed), 1);
            chart.set_feed_paused(host_paused);
            let first = chart.bars(SeriesId::MAIN)[0].time;
            let cutoff = chart.bars(SeriesId::MAIN)[50].time;
            chart.set_markers(SeriesId::MAIN, vec![crate::model::Marker::new(first)]);
            let line = chart
                .add_price_line(
                    SeriesId::MAIN,
                    crate::model::PriceLine::new(15.0, Color32::RED),
                )
                .unwrap();
            let limit = std::num::NonZeroUsize::new(1000);
            chart.set_bar_limit(SeriesId::MAIN, limit);
            assert!(chart.set_series_formatter(
                SeriesId::MAIN,
                Some(std::sync::Arc::new(crate::scales::CurrencyFormatter::usd())),
            ));
            let state = |chart: &Chart| {
                (
                    chart.bars(SeriesId::MAIN).len(),
                    chart.is_main_series_visible(),
                    chart.markers(SeriesId::MAIN).len(),
                    chart.price_line(line).is_some(),
                    chart.bar_limit(SeriesId::MAIN),
                    chart
                        .series_label_formatter(
                            SeriesId::MAIN,
                            crate::scales::PriceScaleMode::Normal,
                        )
                        .is_some(),
                    chart.feed_link.as_ref().unwrap().paused,
                )
            };
            let restored = (100, true, 1, true, limit, true, host_paused);
            let removed = (0, false, 0, false, None, false, true);
            let case = format!("host paused {host_paused}, replay {replay}");
            assert_eq!(state(&chart), restored, "{case}: before");
            assert!(chart.user_action(|chart| chart.remove_series(SeriesId::MAIN)));
            assert_eq!(state(&chart), removed, "{case}: removed");
            if replay {
                chart.set_replay_time(Some(cutoff));
            }
            assert!(chart.undo());
            if replay {
                assert!(
                    chart.feed_link.as_ref().unwrap().paused,
                    "{case}: in replay"
                );
                chart.set_replay_time(None);
            }
            assert_eq!(state(&chart), restored, "{case}: undone");
            assert!(chart.redo());
            assert_eq!(state(&chart), removed, "{case}: redone");
            assert!(chart.undo());
            assert_eq!(state(&chart), restored, "{case}: undone again");
        }
    }
}
