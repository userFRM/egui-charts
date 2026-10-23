//! Chart history records UI actions, not ordinary host API calls.

use super::{Chart, ChartElementId, SeriesId};
use crate::config::{ChartConfig, ChartOptions};
use crate::drawings::DrawingManager;
use crate::drawings::services::HistoryService;
use crate::model::{ChartType, ChartTypeParams, Timeframe};
use crate::scales::PriceScale;
use crate::studies::Indicator;

#[derive(Default)]
pub(super) struct ChartHistory {
    undo: Vec<Action>,
    redo: Vec<Action>,
    pending: Option<Pending>,
    changed: bool,
}

struct Pending {
    depth: usize,
    before: Action,
    standalone_drawings: HistoryService,
    study_order: Vec<SeriesId>,
}

#[derive(Default)]
struct Action {
    config: Option<ChartConfig>,
    options: Option<ChartOptions>,
    view: (f32, f32),
    scale: Option<PriceScale>,
    chart_type: Option<(ChartType, ChartTypeParams)>,
    interval: Option<Option<Timeframe>>,
    studies: Vec<StudyChange>,
    drawings: HistoryService,
}

struct StudyChange {
    id: SeriesId,
    before: Option<Study>,
}

struct Study {
    index: usize,
    indicator: Box<dyn Indicator>,
    inputs: String,
    autoscale: bool,
}

impl ChartHistory {
    fn push(&mut self, action: Action) {
        self.redo.clear();
        if self.undo.len() == 100 {
            self.undo.remove(0);
        }
        self.undo.push(action);
    }

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
            indicator: chart.studies.indicators()[index].clone_box(),
            inputs: chart.study_inputs[index].clone(),
            autoscale: chart.study_autoscale[index],
        })
    }

    fn same(&self, other: &Self) -> bool {
        let inputs = |study: &dyn Indicator| {
            study
                .inputs()
                .into_iter()
                .map(|input| (input.id, input.value))
                .collect::<Vec<_>>()
        };
        self.index == other.index
            && self.inputs == other.inputs
            && self.autoscale == other.autoscale
            && self.indicator.is_visible() == other.indicator.is_visible()
            && self.indicator.settings() == other.indicator.settings()
            && self.indicator.colors() == other.indicator.colors()
            && inputs(self.indicator.as_ref()) == inputs(other.indicator.as_ref())
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
        if let Some(pending) = &mut self.history.pending {
            pending.depth += 1;
            return;
        }
        let mut standalone_drawings = HistoryService::with_max_size(0);
        self.drawings.swap_history(&mut standalone_drawings);
        self.history.pending = Some(Pending {
            depth: 1,
            before: Action {
                config: Some(self.config.clone()),
                options: Some(self.chart_options.clone()),
                view: self.get_time_scale_state(),
                scale: Some(self.state.price_scale().clone()),
                chart_type: Some((self.chart_type, self.chart_type_params.clone())),
                interval: Some(self.timeframe),
                ..Action::default()
            },
            standalone_drawings,
            study_order: self.study_ids.clone(),
        });
    }

    /// Finish the matching [`Self::begin_user_action`], recording only changed components.
    pub fn end_user_action(&mut self) {
        let Some(mut pending) = self.history.pending.take() else {
            return;
        };
        if pending.depth > 1 {
            pending.depth -= 1;
            self.history.pending = Some(pending);
            return;
        }
        self.drawings.swap_history(&mut pending.standalone_drawings);
        let mut action = pending.before;
        action.drawings = pending.standalone_drawings;
        if action.config.as_ref() == Some(&self.config) {
            action.config = None;
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
        action.studies.retain(
            |change| match (&change.before, Study::capture(self, change.id)) {
                (None, None) => false,
                (Some(before), Some(after)) => !before.same(&after),
                _ => true,
            },
        );
        if action.config.is_none()
            && action.options.is_none()
            && action.scale.is_none()
            && action.chart_type.is_none()
            && action.interval.is_none()
            && action.studies.is_empty()
            && !action.drawings.can_undo()
        {
            return;
        }
        self.history.push(action);
    }

    pub(super) fn record_study(&mut self, id: SeriesId) {
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
    pub fn take_history_changed(&mut self) -> bool {
        std::mem::take(&mut self.history.changed)
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
                self.drawings.curr_timeframe.clear();
            }
            *interval = current;
        }
        if let Some(config) = &mut action.config {
            std::mem::swap(&mut self.config, config);
        }
        if let Some(options) = &mut action.options {
            let current = &self.chart_options.time_scale;
            let target = &options.time_scale;
            let spacing_changed = current.bar_spacing != target.bar_spacing;
            let offset_changed = current.right_offset != target.right_offset
                || current.right_offset_pixels != target.right_offset_pixels;
            let mut view = self.get_time_scale_state();
            std::mem::swap(&mut view, &mut action.view);
            let spacing = if spacing_changed {
                view.0
            } else {
                action.view.0
            };
            let offset = if offset_changed {
                view.1
            } else {
                action.view.1
            };
            std::mem::swap(&mut self.chart_options, options);
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
        if let Some(scale) = &mut action.scale {
            std::mem::swap(self.state.price_scale_mut(), scale);
        }
        // Restore by original index after removing every touched study, so bulk removal and
        // templates retain order and stable ids while unrelated host studies stay present.
        let current: Vec<_> = action
            .studies
            .iter()
            .map(|change| Study::capture(self, change.id))
            .collect();
        for change in &action.studies {
            self.remove_series(change.id);
        }
        let mut restore: Vec<_> = action
            .studies
            .iter_mut()
            .filter_map(|change| change.before.take().map(|study| (change.id, study)))
            .collect();
        restore.sort_by_key(|(_, study)| study.index);
        for (id, mut study) in restore {
            if let Some(timeframe) = self.timeframe {
                study.indicator.set_timeframe(timeframe);
            }
            study.indicator.set_instrument(self.instrument());
            study.indicator.calculate(&self.state.data().bars);
            let at = study.index.min(self.study_ids.len());
            // The registry exposes a slice; append then rotate to insert at the saved position.
            self.studies.add(study.indicator);
            self.studies.indicators_mut()[at..].rotate_right(1);
            self.study_ids.insert(at, id);
            self.study_inputs.insert(at, study.inputs);
            self.study_autoscale.insert(at, study.autoscale);
            self.pending_from.insert(at, None);
            self.next_refresh.insert(at, f64::NEG_INFINITY);
        }
        for (change, before) in action.studies.iter_mut().zip(current) {
            change.before = before;
        }
        if let Some(ChartElementId::Indicator(id)) = self.selection.selected_id()
            && !self.study_ids.contains(&id)
        {
            self.selection.deselect();
        }
        self.drawings.swap_history(&mut action.drawings);
        if redo {
            while self.drawings.redo() {}
        } else {
            while self.drawings.undo() {}
        }
        self.drawings.swap_history(&mut action.drawings);
        if redo {
            self.history.undo.push(action);
        } else {
            self.history.redo.push(action);
        }
        self.history.changed = true;
        true
    }

    pub(super) fn shift_drawing_history(&mut self, from: f32, delta: f32) {
        self.drawings.shift_bar_indices(from, delta);
        for action in self.history.undo.iter_mut().chain(&mut self.history.redo) {
            action.drawings.shift_bar_indices(from, delta);
        }
        if let Some(pending) = &mut self.history.pending {
            pending.standalone_drawings.shift_bar_indices(from, delta);
        }
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::drawings::manager::DrawingSyncAction;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::model::BarData;
    use crate::studies::{InputValue, RSI, SMA};
    use crate::widget::test_frames::chart;
    use egui::Color32;

    #[test]
    fn study_edits_and_bulk_removal_restore_ids_order_and_settings() {
        let mut chart = chart(&[10.0; 30], &[20.0; 30]);
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
        assert!(chart.redo());
        assert_eq!(chart.indicator_inputs(rsi), Some("5"));
        assert_eq!(
            chart.indicators().next().unwrap().1.settings().plots[0].color,
            Color32::RED
        );
        assert!(!chart.study_autoscale[0]);
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
            chart.chart_options.time_scale.timezone = crate::scales::TimezoneMode::nyse();
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
                            == crate::scales::TimezoneMode::nyse(),
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
            options.time_scale.timezone = crate::scales::TimezoneMode::nyse();
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
    fn host_updates_and_unchanged_actions_do_not_enter_or_clear_history() {
        let mut chart = Chart::new(BarData::new());
        chart.set_timeframe(Timeframe::Min1);
        chart.config_mut().show_grid = false;
        let host = chart.add_indicator(Box::new(SMA::new(3)));
        chart
            .drawings_mut()
            .add_synced_drawing(Drawing::new(0, DrawingToolType::TrendLine));
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
            chart.indicator_mut(host).unwrap();
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
    fn drawing_snapshots_keep_host_loads_sync_events_and_prepended_coordinates() {
        let mut chart = chart(&[10.0, 11.0], &[20.0, 21.0]);
        chart.user_action(|chart| {
            let mut drawing = Drawing::new(0, DrawingToolType::HorizontalLine);
            drawing.chart_points = vec![ChartPoint::new(1.0, 15.0)];
            chart.drawings_mut().add_synced_drawing(drawing);
        });
        chart.user_action(|chart| chart.drawings_mut().clear_all());
        let mut host = Drawing::new(0, DrawingToolType::TrendLine);
        host.stored_id = Some("host".into());
        chart.drawings_mut().add_synced_drawing(host);
        chart.drawings.pending_sync.lock().unwrap().clear();
        // A feed prepend moves commands on both chart stacks, as it does live drawings.
        chart.shift_drawing_history(f32::NEG_INFINITY, 3.0);
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
        chart.shift_drawing_history(f32::NEG_INFINITY, 2.0);
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

    use egui::{Context, Event, Key, Modifiers, RawInput};

    #[test]
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
                    .add_synced_drawing(Drawing::new(0, DrawingToolType::TrendLine))
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
}
