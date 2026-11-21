//! Study registration, settings and calculation order, including dependent studies.

use super::{Chart, ChartElementId, SeriesId};
use crate::model::Bar;
use crate::studies::{Indicator, InputKind, InputValue, RefusedStudy, StudyError, StudySource};
use web_time::Instant;

impl Chart {
    /// Caps the studies at `limit`, or lifts the cap with `None`. The cap applies to
    /// [`try_add_indicator`](Self::try_add_indicator), which the terminal's dialogs use, to
    /// pasting a copied study, to applying a study template set, to copying studies from another
    /// chart of a layout and to the object tree's Duplicate; studies already on the chart stay. A
    /// user action that turns the volume setting on, such as the Toggle volume shortcut, leaves
    /// [`ChartConfig::show_volume`](crate::config::ChartConfig::show_volume) off at the cap unless
    /// a Volume study is already on the chart. The same applies to the
    /// [analyst targets setting](Self::set_price_targets_visible), served by an Analyst price
    /// forecast study. Both settings stay uncapped outside user actions, as do restores and
    /// undo/redo.
    /// [`set_chained_study_limit`](Self::set_chained_study_limit) caps, on the same paths, the
    /// studies that read another study's output; a study is refused when it would pass either cap.
    pub fn set_study_limit(&mut self, limit: Option<usize>) {
        self.study_limit = limit;
    }

    /// The cap set with [`set_study_limit`](Self::set_study_limit); `None` when uncapped.
    pub fn study_limit(&self) -> Option<usize> {
        self.study_limit
    }

    /// Caps at `limit` the studies that read another study's output ([`StudySource::Study`]),
    /// each on its own or as part of a chain, or lifts the cap with `None`, the default. Every
    /// such study counts once, also when the study it reads has left the chart. The paths the
    /// [study cap](Self::set_study_limit) covers refuse a study reading another study's output
    /// past it, and [`set_indicator_source`](Self::set_indicator_source) and the study settings
    /// dialog refuse another study's output as the source of a study that reads none. Studies
    /// already on the chart stay when the cap is below their number, and a study that reads one
    /// study's output can still switch to another's.
    pub fn set_chained_study_limit(&mut self, limit: Option<usize>) {
        self.chained_study_limit = limit;
    }

    /// The cap set with [`set_chained_study_limit`](Self::set_chained_study_limit); `None` when
    /// uncapped.
    pub fn chained_study_limit(&self) -> Option<usize> {
        self.chained_study_limit
    }

    /// Whether the chart holds as many studies as its cap allows.
    pub(crate) fn study_limit_reached(&self) -> bool {
        self.study_limit
            .is_some_and(|limit| self.study_ids.len() >= limit)
    }

    /// Whether adding `added` studies that read another study's output, beside the `kept` such
    /// studies that stay on the chart, passes the chained study cap. Adding none never does, so
    /// studies already past the cap stay and studies reading prices are still added.
    pub(super) fn passes_chained_study_limit(&self, kept: usize, added: usize) -> bool {
        added > 0
            && self
                .chained_study_limit
                .is_some_and(|limit| kept + added > limit)
    }

    /// Whether the chart holds as many studies reading another study's output as its chained
    /// study cap allows, so that it takes no further one.
    pub(crate) fn chained_study_limit_reached(&self) -> bool {
        let chained = self
            .indicators()
            .filter(|(_, study)| reads_study(*study))
            .count();
        self.passes_chained_study_limit(chained, 1)
    }

    /// Adds a study like [`add_indicator`](Self::add_indicator), unless the chart already holds
    /// as many as [`study_limit`](Self::study_limit) allows or, for a study that reads another
    /// study's output, as many of those as [`chained_study_limit`](Self::chained_study_limit)
    /// allows. A refused study leaves the chart unchanged and comes back in
    /// [`StudyError::StudyLimitReached`] or [`StudyError::ChainedStudyLimitReached`], the first
    /// when both caps refuse it.
    pub fn try_add_indicator(
        &mut self,
        indicator: Box<dyn Indicator>,
    ) -> Result<SeriesId, StudyError> {
        if self.study_limit_reached() {
            return Err(StudyError::StudyLimitReached(RefusedStudy::new(indicator)));
        }
        if reads_study(indicator.as_ref()) && self.chained_study_limit_reached() {
            return Err(StudyError::ChainedStudyLimitReached(RefusedStudy::new(
                indicator,
            )));
        }
        Ok(self.add_indicator(indicator))
    }

    /// `indicator` set to this chart's interval, instrument and events, not yet computed.
    pub(super) fn configure_indicator(
        &self,
        mut indicator: Box<dyn Indicator>,
    ) -> Box<dyn Indicator> {
        if !indicator.has_settings() {
            indicator = Box::new(crate::studies::ConfiguredStudy::new(indicator));
        }
        if let Some(timeframe) = self.timeframe() {
            indicator.set_timeframe(timeframe);
        }
        indicator.set_value_formatter(self.inherited_study_formatter(indicator.value_format()));
        indicator.set_instrument(self.instrument());
        indicator.set_chart_symbol(&self.symbol);
        indicator.set_time_scale_events(held_events(
            &self.time_scale_events,
            self.frozen_source_events,
        ));
        indicator.set_bar_clock(&self.bar_clock());
        indicator
    }

    /// `indicator` set to this chart's interval, instrument and events and computed from its bars.
    pub(super) fn prepare_indicator(&self, indicator: Box<dyn Indicator>) -> Box<dyn Indicator> {
        let mut indicator = self.configure_indicator(indicator);
        if let Some(crate::studies::StudySource::Study { id, plot }) = indicator.source() {
            let source = self
                .study_ids
                .iter()
                .position(|candidate| candidate.0 == id)
                .and_then(|at| self.studies.indicators()[at].values().columns().get(plot));
            indicator.update_source(
                &self.state.data().bars,
                0,
                source.map_or(&[], Vec::as_slice),
            );
        } else {
            let sources = self.study_data.data(
                indicator.data_requests(),
                &self.symbol,
                self.timeframe,
                self.replay_time(),
            );
            indicator.update_data(&self.state.data().bars, 0, &sources);
        }
        indicator
    }

    /// Adds a study, computed at once from the bars. An [`Indicator::is_overlay`] study uses
    /// the original price pane or, after its removal, the surviving main series' pane. Other
    /// studies start in new panes, as do overlays with neither destination. A study that reads
    /// another study's output ([`StudySource::Study`]) opens in that study's pane when the source
    /// has a pane of its own, and takes the source's scale when the two share a pane; move it
    /// afterwards with [`move_indicator_to_pane`](Self::move_indicator_to_pane).
    /// Returns the study's id, which stays valid when other studies are removed. Host code adds
    /// past both caps; the interface goes through [`try_add_indicator`](Self::try_add_indicator).
    pub fn add_indicator(&mut self, indicator: Box<dyn Indicator>) -> SeriesId {
        let id = self.insert_indicator(indicator);
        self.place_on_source(id);
        id
    }

    /// Puts a study reading another study into its source's pane when the source has its own,
    /// and on its source's scale when both share a pane. A study reading prices or a missing
    /// study keeps its place.
    pub(super) fn place_on_source(&mut self, id: SeriesId) {
        let Some(StudySource::Study { id: source, .. }) = self
            .indicators()
            .find(|(study, _)| *study == id)
            .and_then(|(_, study)| study.source())
        else {
            return;
        };
        let Some((source, overlay, scale)) = self
            .indicators()
            .find(|(study, _)| study.0 == source)
            .map(|(study, item)| (study, item.is_overlay(), item.scale()))
        else {
            return;
        };
        if !overlay && let Some(pane) = self.series_pane(source) {
            self.move_series_keeping_layers(id, pane);
        }
        let own = self
            .indicators()
            .find(|(study, _)| *study == id)
            .map(|(_, study)| study.scale());
        if self.series_pane(id).is_some()
            && self.series_pane(id) == self.series_pane(source)
            && own != Some(scale)
        {
            self.set_indicator_scale(id, scale);
        }
    }

    /// Adds a study where [`add_indicator`](Self::add_indicator) places a study reading prices,
    /// regardless of its source. Restores use this before their saved source ids are remapped.
    pub(super) fn insert_indicator(&mut self, indicator: Box<dyn Indicator>) -> SeriesId {
        let indicator = self.prepare_indicator(indicator);
        let id = SeriesId(self.next_series);
        self.next_series += 1;
        self.record_study(id);
        let original = self.panes().contains(&super::PaneId::MAIN);
        let target = (!original && indicator.is_overlay())
            .then(|| self.series_pane(SeriesId::MAIN))
            .flatten();
        let overlay = indicator.is_overlay() && original;
        self.studies.add(indicator);
        #[cfg(feature = "studies")]
        self.adopt_price_target_study();
        self.readout_revision = crate::model::next_revision();
        self.study_ids.push(id);
        self.study_autoscale.push(true);
        self.study_inputs.push(String::new());
        self.study_captions.push(std::sync::OnceLock::new());
        self.pending_from.push(None);
        self.stale.push(false);
        self.next_refresh.push(f64::NEG_INFINITY);
        if let Some(pane) = target {
            self.studies
                .indicators_mut()
                .last_mut()
                .expect("study was added")
                .set_overlay(false);
            self.pane_layout
                .panes
                .iter_mut()
                .find(|item| item.id == pane)
                .expect("main series belongs to pane")
                .members
                .push(id);
        } else if !overlay {
            self.add_study_pane(id);
        }
        id
    }

    /// Sets the caption the legends show after the name of study `id`, in place of the one the
    /// chart derives from its inputs. A host caption does not follow later changes of the
    /// study's inputs; an empty one returns the study to the derived caption. Returns `false`
    /// for an id that is not a study of the chart.
    pub fn set_indicator_inputs(&mut self, id: SeriesId, inputs: impl Into<String>) -> bool {
        let Some(index) = self.study_ids.iter().position(|candidate| *candidate == id) else {
            return false;
        };
        self.record_study(id);
        self.study_inputs[index] = inputs.into();
        true
    }

    /// The caption the legends show after the name of study `id` while
    /// [`ChartConfig::show_indicator_inputs`](crate::config::ChartConfig::show_indicator_inputs)
    /// is on: the host's from [`set_indicator_inputs`](Self::set_indicator_inputs), else one built
    /// from the study's shown inputs. `None` if `id` is not a study of the chart.
    pub fn indicator_inputs(&self, id: SeriesId) -> Option<&str> {
        let at = self
            .study_ids
            .iter()
            .position(|candidate| *candidate == id)?;
        Some(self.study_caption(at))
    }

    /// The caption the legends show for the study at `at`: the host's, else the derived one.
    pub(crate) fn study_caption(&self, at: usize) -> &str {
        match self.study_inputs[at].as_str() {
            "" => self.study_captions[at]
                .get_or_init(|| derived_caption(self.studies.indicators()[at].as_ref())),
            host => host,
        }
    }

    /// The caption the host set for study `id` with
    /// [`set_indicator_inputs`](Self::set_indicator_inputs), empty if none. Copies and saved
    /// layouts keep this one, so that a copy derives its own.
    #[cfg(feature = "studies")]
    pub(crate) fn host_indicator_inputs(&self, id: SeriesId) -> Option<&str> {
        self.study_ids
            .iter()
            .position(|candidate| *candidate == id)
            .map(|index| self.study_inputs[index].as_str())
    }

    /// The studies that read the output of study `id`, directly or through other studies, in
    /// chart order. A study whose user edits are off is not among them, nor are the studies
    /// reading it, as no user command removes such a study.
    pub(super) fn study_dependents(&self, id: SeriesId) -> Vec<SeriesId> {
        if !self.study_ids.contains(&id) {
            return Vec::new();
        }
        let mut removed = vec![id];
        loop {
            let before = removed.len();
            for (study, indicator) in self.study_ids.iter().zip(self.studies.indicators()) {
                let reads_removed = matches!(indicator.source(),
                    Some(StudySource::Study { id: source, .. })
                        if removed.iter().any(|removed| removed.0 == source));
                if reads_removed && indicator.user_edit_enabled() && !removed.contains(study) {
                    removed.push(*study);
                }
            }
            if removed.len() == before {
                break;
            }
        }
        self.study_ids
            .iter()
            .filter(|study| removed[1..].contains(study))
            .copied()
            .collect()
    }

    pub(super) fn take_study(&mut self, id: SeriesId) -> Option<Box<dyn Indicator>> {
        let at = self.study_ids.iter().position(|study| *study == id)?;
        self.record_study(id);
        #[cfg(feature = "studies")]
        if self.volume_study == Some(id) {
            self.volume_study = None;
            self.config.show_volume = false;
            self.volume_setting = false;
        }
        let indicator = self.studies.remove_indicator(at);
        #[cfg(feature = "studies")]
        if indicator
            .as_ref()
            .is_some_and(|study| is_price_target_study(study.as_ref()))
            && !self
                .studies
                .indicators()
                .iter()
                .any(|study| is_price_target_study(study.as_ref()))
        {
            self.show_price_targets = false;
        }
        self.readout_revision = crate::model::next_revision();
        self.study_ids.remove(at);
        self.study_autoscale.remove(at);
        self.study_inputs.remove(at);
        self.study_captions.remove(at);
        self.pending_from.remove(at);
        self.stale.remove(at);
        self.next_refresh.remove(at);
        if self.selection.selected_id() == Some(ChartElementId::Indicator(id)) {
            self.selection.deselect();
        }
        indicator
    }

    /// Shows or hides the study `id`; a hidden study keeps its values. Returns `false` for an id
    /// that is not a study of the chart.
    pub fn set_indicator_visible(&mut self, id: SeriesId, visible: bool) -> bool {
        let Some(at) = self.study_ids.iter().position(|study| *study == id) else {
            return false;
        };
        self.record_study(id);
        self.readout_revision = crate::model::next_revision();
        self.studies.indicators_mut()[at].set_visible(visible);
        true
    }

    /// Hides or shows all studies without changing each study's [`Indicator::is_visible`].
    /// The group flag gates painting, legends, fitting, hits, selection and readouts, except
    /// [`CursorDataPolicy::IncludeHidden`](super::CursorDataPolicy::IncludeHidden). Panes behave
    /// as when their studies are hidden individually. New studies join the hidden group;
    /// calculation, data requests and alerts continue. Hiding deselects a selected study.
    /// Inside [`Self::user_action`] this is one undo step. The flag is kept on restore but
    /// is not saved in a chart snapshot.
    pub fn set_indicators_hidden(&mut self, hidden: bool) {
        self.indicators_hidden = hidden;
        self.readout_revision = crate::model::next_revision();
        if hidden
            && matches!(
                self.selection.selected_id(),
                Some(ChartElementId::Indicator(_))
            )
        {
            self.selection.deselect();
        }
    }

    /// Whether [`Self::set_indicators_hidden`] hides every study.
    pub fn indicators_hidden(&self) -> bool {
        self.indicators_hidden
    }

    /// Whether the study's own visibility and its group's visibility allow it to be shown.
    pub(super) fn shows_study(&self, study: &dyn Indicator) -> bool {
        !self.indicators_hidden && study.is_visible()
    }

    /// Whether an existing study permits user edits. Unknown ids return false.
    pub fn indicator_user_edit_enabled(&self, id: SeriesId) -> bool {
        self.indicators()
            .any(|(candidate, study)| candidate == id && study.user_edit_enabled())
    }

    /// Enable or disable user editing, retaining the choice in study snapshots and saved state.
    /// User undo and redo retain the host's latest permission.
    /// Host APIs can still update the study. Returns false for an unknown id or rejected settings.
    pub fn set_indicator_user_edit_enabled(&mut self, id: SeriesId, enabled: bool) -> bool {
        let Some(study) = self.indicator_mut(id) else {
            return false;
        };
        let mut settings = study.settings();
        settings.user_edit_enabled = Some(enabled);
        study.apply_settings(&[], settings).is_ok()
    }

    /// Includes or excludes study `id` from the main price auto-scale, without hiding it.
    /// Defaults to `true`; only visible overlay studies contribute. Returns `false` for an id
    /// that is not a study of the chart.
    pub fn set_indicator_autoscale(&mut self, id: SeriesId, autoscale: bool) -> bool {
        let Some(at) = self.study_ids.iter().position(|study| *study == id) else {
            return false;
        };
        self.record_study(id);
        self.study_autoscale[at] = autoscale;
        true
    }

    /// Pins study `id` to the pane's price scale, a scale of its own or no scale. Returns `false`
    /// for an id that is not a study of the chart, or a custom presentation that keeps its scale.
    pub fn set_indicator_scale(&mut self, id: SeriesId, scale: crate::studies::StudyScale) -> bool {
        let Some(at) = self.study_ids.iter().position(|study| *study == id) else {
            return false;
        };
        self.record_study(id);
        if let crate::studies::StudyScale::Shared(key) = scale {
            self.ensure_series_scale(key);
        }
        self.studies.indicators_mut()[at].set_scale(scale)
    }

    /// The chart's studies with their ids, in the order they were added. Their values reflect the
    /// bars as of the last [`show`](Self::show), or of the call to
    /// [`add_indicator`](Self::add_indicator). A study that is not updated incrementally can lag
    /// the bars by its wait between full recomputes, at least 0.25 s; after the bars changed from
    /// the first one on, as when `set_data` loads other bars or older bars are loaded in front, it
    /// has no values until it is recomputed. After a bar limit or
    /// [`remove_oldest_bars`](Self::remove_oldest_bars) evicts the oldest bars, it keeps its
    /// values on the bars that remain until its next full recompute, with none for the bars added
    /// since, unless it draws candles, zones, markers, labels, price segments, projections,
    /// provisional segments or drawings; so it does when its order-flow input drops its oldest
    /// bars, or when live bars change only the last bar of a history it reads. On price-based
    /// types these remain the source-bar results; [`Self::study_values_at`] returns the displayed
    /// element results.
    pub fn indicators(&self) -> impl Iterator<Item = (SeriesId, &dyn Indicator)> {
        let studies = self.studies.indicators().iter().map(|study| study.as_ref());
        self.study_ids.iter().copied().zip(studies)
    }

    /// The study `id`, to change its settings. Its values reflect the bars as of the last
    /// [`show`](Self::show); the next one recomputes it in full. Changes report
    /// [`ChartEvent::Study`](super::ChartEvent::Study) before the next chart notification.
    pub fn indicator_mut(&mut self, id: SeriesId) -> Option<&mut (dyn Indicator + 'static)> {
        let at = self.study_ids.iter().position(|study| *study == id)?;
        self.record_study(id);
        Some(self.edit_study(at))
    }

    /// Settings dialog previews keep recomputation and history bookkeeping without reporting
    /// [`ChartEvent::Study`](super::ChartEvent::Study); the host reports the committed edit.
    #[cfg(feature = "ui")]
    pub(crate) fn preview_indicator_mut(
        &mut self,
        id: SeriesId,
    ) -> Option<&mut (dyn Indicator + 'static)> {
        let at = self.study_ids.iter().position(|study| *study == id)?;
        self.record_study(id);
        self.host_events
            .studies
            .retain(|(watched, _)| *watched != id);
        Some(self.edit_study(at))
    }

    /// Marks the study for a full recompute at the next `show`.
    fn edit_study(&mut self, at: usize) -> &mut (dyn Indicator + 'static) {
        self.pending_from[at] = Some(0);
        self.next_refresh[at] = f64::NEG_INFINITY;
        self.readout_revision = crate::model::next_revision();
        self.studies.indicators_mut()[at].as_mut()
    }

    /// Updates every study with a pending change, as of `now` in seconds. An incremental study
    /// whose change starts at its last bar or later is updated at once. Any other change
    /// recomputes the study in full, for at most one study per call: of the studies whose change
    /// starts at the first bar or whose `next_refresh` has passed, the one whose `next_refresh`
    /// is earliest. Its next full recompute then waits, from the end of this one,
    /// the longer of a quarter second and four times what this one took. Until its turn, a
    /// study whose change starts at the first bar has no values, since its values belong to
    /// other bars. A study whose values still hold, after an eviction, a drop of its order-flow
    /// input's oldest bars or a live bar of a history it reads, keeps them and waits for its
    /// `next_refresh` like a tail update instead. Returns how long until the next waiting study
    /// is due, zero if one is due now.
    pub(super) fn refresh_studies(&mut self, now: f64) -> Option<f64> {
        let instrument = self.instrument().cloned();
        let clock = self.bar_clock();
        let bars = &self.state.data().bars;
        let events = held_events(&self.time_scale_events, self.frozen_source_events);
        for (at, study) in self.studies.indicators_mut().iter_mut().enumerate() {
            let instrument_changed = study.set_instrument(instrument.as_ref());
            let symbol_changed = study.set_chart_symbol(&self.symbol);
            let clock_changed = study.set_bar_clock(&clock);
            let events_changed = study.set_time_scale_events(events);
            if instrument_changed || symbol_changed || clock_changed || events_changed {
                // The inputs a study offers can follow the chart's symbol and interval.
                self.study_captions[at].take();
                self.pending_from[at] = Some(0);
                self.element_studies.dirty = true;
                self.readout_revision = crate::model::next_revision();
            }
        }
        let requests = self
            .studies
            .indicators()
            .iter()
            .flat_map(|study| study.data_requests())
            .collect();
        let sources =
            self.study_data
                .data(requests, &self.symbol, self.timeframe, self.replay_time());
        refresh_values(
            self.studies.indicators_mut(),
            &self.study_ids,
            bars,
            &mut self.pending_from,
            &mut self.stale,
            &mut self.next_refresh,
            now,
            &sources,
        )
    }

    pub(super) fn refresh_element_studies(&mut self, now: f64) -> Option<f64> {
        let cutoff = self.replay_time();
        let cache = &mut self.element_studies;
        let requests = cache
            .studies
            .indicators()
            .iter()
            .flat_map(|study| study.data_requests())
            .collect();
        let sources = self
            .study_data
            .data(requests, &self.symbol, self.timeframe, cutoff);
        refresh_values(
            cache.studies.indicators_mut(),
            &self.study_ids,
            &cache.bars,
            &mut cache.pending_from,
            &mut cache.stale,
            &mut cache.next_refresh,
            now,
            &sources,
        )
    }

    pub(super) fn rendered_studies(&self) -> &crate::studies::IndicatorRegistry {
        if self.chart_type.is_time_independent() {
            &self.element_studies.studies
        } else {
            &self.studies
        }
    }

    pub(super) fn recalculate_studies(&mut self) {
        self.readout_revision = crate::model::next_revision();
        let requests = self
            .studies
            .indicators()
            .iter()
            .flat_map(|study| study.data_requests())
            .collect();
        let sources =
            self.study_data
                .data(requests, &self.symbol, self.timeframe, self.replay_time());
        calculate(
            self.studies.indicators_mut(),
            &self.study_ids,
            &self.state.data().bars,
            &sources,
        );
    }

    /// Select a price source or another study's undisplaced output. Rejects missing plots and
    /// dependency cycles without changing either study. Hidden sources continue calculating.
    /// Refuses another study's output for a study that reads none while the chart holds as many
    /// studies on studies as [`chained_study_limit`](Self::chained_study_limit) allows, with
    /// [`StudyError::InvalidInput`] for `source`.
    pub fn set_indicator_source(
        &mut self,
        id: SeriesId,
        source: StudySource,
    ) -> Result<(), StudyError> {
        let at = self
            .study_ids
            .iter()
            .position(|candidate| *candidate == id)
            .ok_or(StudyError::UnknownStudyId(id.0))?;
        if let StudySource::Study { id: parent, plot } = source {
            let parent_at = self
                .study_ids
                .iter()
                .position(|candidate| candidate.0 == parent)
                .ok_or(StudyError::UnknownSourceStudy(parent))?;
            if plot >= self.studies.indicators()[parent_at].line_count() {
                return Err(StudyError::UnknownSourcePlot { id: parent, plot });
            }
            let mut next = Some(parent);
            for _ in 0..=self.study_ids.len() {
                let Some(parent) = next else {
                    break;
                };
                if parent == id.0 {
                    return Err(StudyError::CyclicSource);
                }
                // A removed ancestor ends the chain; no cycle runs through it.
                let Some(index) = self
                    .study_ids
                    .iter()
                    .position(|candidate| candidate.0 == parent)
                else {
                    next = None;
                    break;
                };
                next = match self.studies.indicators()[index].source() {
                    Some(StudySource::Study { id, .. }) => Some(id),
                    _ => None,
                };
            }
            if next.is_some() {
                return Err(StudyError::CyclicSource);
            }
        }
        if matches!(source, StudySource::Study { .. })
            && !reads_study(self.studies.indicators()[at].as_ref())
            && self.chained_study_limit_reached()
        {
            return Err(StudyError::InvalidInput {
                id: "source".into(),
                reason: "would read another study's output past the chart's chained study cap"
                    .into(),
            });
        }
        let mut edited = self.studies.indicators()[at].clone_box();
        edited.set_input("source", InputValue::Source(source))?;
        self.record_study(id);
        self.studies.indicators_mut()[at] = edited;
        self.readout_revision = crate::model::next_revision();
        self.pending_from[at] = Some(0);
        self.next_refresh[at] = f64::NEG_INFINITY;
        self.element_studies.dirty = true;
        Ok(())
    }
}

/// Whether `study` reads another study's output, which the chained study cap counts.
pub(crate) fn reads_study(study: &dyn Indicator) -> bool {
    matches!(study.source(), Some(StudySource::Study { .. }))
}

/// Whether `study` is an Analyst price forecast, which the chart's targets setting follows.
#[cfg(feature = "studies")]
pub(super) fn is_price_target_study(study: &dyn Indicator) -> bool {
    study.name() == crate::studies::AnalystPriceForecast::NAME
}

/// The typed events studies read, without the mark source page an export froze at their end: a page
/// covers only the visible range, so restarts on it would move while scrolling.
fn held_events(
    events: &[crate::model::TimeScaleEvent],
    frozen: usize,
) -> &[crate::model::TimeScaleEvent] {
    &events[..events.len().saturating_sub(frozen)]
}

/// The caption the legends show after a study's name when the host set none: what identifies it
/// at a glance, in input order. Lengths and numbers, choices by their label, the price source,
/// the interval and the symbol; not flags, colours, text, zero values, list sizes, painted
/// widths, times of day, Style tab inputs, choices set to None or another study's output, which
/// mean nothing without their labels. The volume study names its moving average itself.
fn derived_caption(study: &dyn Indicator) -> String {
    if study.is_volume() {
        return String::new();
    }
    let price = |source: &StudySource| match source {
        StudySource::Price(price) => Some(format!("{price:?}").to_lowercase()),
        // The caption has no name for another study's output.
        _ => None,
    };
    let source = study.source();
    let inputs = study.inputs();
    let mut caption: Vec<_> = inputs
        .iter()
        // List sizes, painted widths and minutes after midnight mean nothing alone; the study's
        // source goes below.
        .filter(|input| {
            input.is_shown(&inputs)
                && !input.style
                && !matches!(input.id.as_str(), "band_count" | "event_count" | "width")
                && !matches!(input.kind, InputKind::Length(0, 1439))
                && (source.is_none() || input.id != "source")
        })
        .filter_map(|input| match (&input.kind, &input.value) {
            (InputKind::Choice(choices), InputValue::Choice(index)) => choices
                .get(*index)
                .filter(|label| label.as_str() != "None")
                .cloned(),
            (_, InputValue::Length(value)) => (*value != 0).then(|| value.to_string()),
            (_, InputValue::Integer(value)) => (*value != 0).then(|| value.to_string()),
            // Single precision reads short and turns an open upper limit into infinity.
            (_, InputValue::Number(value)) => {
                let value = *value as f32;
                (value != 0.0 && value.is_finite()).then(|| value.to_string())
            }
            (_, InputValue::Source(value)) => price(value),
            (_, InputValue::Timeframe(Some(value))) => Some(value.to_string()),
            (_, InputValue::Symbol(value)) if !value.is_empty() => Some(value.clone()),
            _ => None,
        })
        .collect();
    // An anchor or session choice identifies the study, which names its price only once edited.
    // Flags, which the caption leaves out, can precede that choice.
    let anchored = matches!(
        inputs
            .iter()
            .map(|input| &input.kind)
            .find(|kind| !matches!(kind, InputKind::Toggle)),
        Some(InputKind::Choice(_))
    );
    let edited = inputs
        .iter()
        .any(|input| matches!(input.kind, InputKind::Source) && input.value != input.default);
    if let Some(source) = source
        .as_ref()
        .filter(|_| !anchored || edited)
        .and_then(price)
    {
        caption.insert(caption.len().min(1), source);
    }
    caption.join(" ")
}

/// Updates one study with its source column, keeping values indexed by the input bars.
pub(super) fn update(
    studies: &mut [Box<dyn Indicator>],
    ids: &[SeriesId],
    at: usize,
    bars: &[Bar],
    from: usize,
    sources: &[crate::studies::StudyData<'_>],
) {
    if let Some(StudySource::Study { id, plot }) = studies[at].source() {
        let values = ids
            .iter()
            .position(|candidate| candidate.0 == id)
            .and_then(|source| studies[source].values().columns().get(plot))
            .cloned()
            .unwrap_or_default();
        studies[at].update_source(bars, from, &values);
    } else {
        studies[at].update_data(bars, from, sources);
    }
}

pub(super) fn calculate(
    studies: &mut [Box<dyn Indicator>],
    ids: &[SeriesId],
    bars: &[Bar],
    sources: &[crate::studies::StudyData<'_>],
) {
    let order = order(studies, ids);
    for at in 0..studies.len() {
        if !order.contains(&at) {
            studies[at].update_source(bars, 0, &[]);
        }
    }
    for at in order {
        update(studies, ids, at, bars, 0, sources);
    }
}

/// Orders acyclic dependencies before consumers. Missing and cyclic inputs are left out.
pub(super) fn order(studies: &[Box<dyn Indicator>], ids: &[SeriesId]) -> Vec<usize> {
    let mut ordered = Vec::with_capacity(studies.len());
    loop {
        let before = ordered.len();
        for (at, study) in studies.iter().enumerate() {
            if ordered.contains(&at) {
                continue;
            }
            let ready = match study.source() {
                Some(StudySource::Study { id, plot }) => ids
                    .iter()
                    .position(|candidate| candidate.0 == id)
                    .is_some_and(|parent| {
                        ordered.contains(&parent) && plot < studies[parent].line_count()
                    }),
                _ => true,
            };
            if ready {
                ordered.push(at);
            }
        }
        if ordered.len() == before {
            break;
        }
    }
    ordered
}

pub(super) fn refresh_values(
    studies: &mut [Box<dyn Indicator>],
    ids: &[SeriesId],
    bars: &[Bar],
    pending_from: &mut [Option<usize>],
    stale: &mut [bool],
    next_refresh: &mut [f64],
    now: f64,
    sources: &[crate::studies::StudyData<'_>],
) -> Option<f64> {
    let order = super::studies::order(studies, ids);
    for at in 0..studies.len() {
        if !order.contains(&at) {
            studies[at].update_source(bars, 0, &[]);
            pending_from[at] = None;
            stale[at] = false;
        }
    }
    let mut recomputed = false;
    for &at in &order {
        let parent = match studies[at].source() {
            Some(crate::studies::StudySource::Study { id, .. }) => {
                ids.iter().position(|candidate| candidate.0 == id)
            }
            _ => None,
        };
        // Retained parent values can feed a waiting consumer while another tail update is pending,
        // and so can values kept for a full recompute, which a stream can keep pending on every
        // frame. Replaced histories and edited sources must finish their full invalidation first.
        if parent.is_some_and(|parent| pending_from[parent] == Some(0)) {
            continue;
        }
        let Some(from) = pending(pending_from, stale, at) else {
            continue;
        };
        let len = studies[at].values().len();
        let incremental =
            studies[at].is_incremental() && len > 0 && from + 1 >= len && bars.len() >= len;
        if !incremental {
            if recomputed {
                continue;
            }
            let due = order
                .iter()
                .copied()
                .filter(|&at| {
                    let parent_ready = match studies[at].source() {
                        Some(crate::studies::StudySource::Study { id, .. }) => ids
                            .iter()
                            .position(|candidate| candidate.0 == id)
                            .is_some_and(|parent| pending_from[parent] != Some(0)),
                        _ => true,
                    };
                    // Only a start at the first bar whose values belong to other bars is due at
                    // once; kept values wait like a tail update.
                    parent_ready
                        && pending(pending_from, stale, at).is_some_and(|from| {
                            let len = studies[at].values().len();
                            !(studies[at].is_incremental()
                                && len > 0
                                && from + 1 >= len
                                && bars.len() >= len)
                                && (pending_from[at] == Some(0) || now >= next_refresh[at])
                        })
                })
                .min_by(|&a, &b| next_refresh[a].total_cmp(&next_refresh[b]));
            if due != Some(at) {
                continue;
            }
        }
        let started = Instant::now();
        // A parent can repaint earlier bars even when only its input tail changed.
        let mut previous: Vec<(usize, Vec<f64>)> = Vec::new();
        for study in studies.iter() {
            if let Some(StudySource::Study { id, plot }) = study.source()
                && id == ids[at].0
                && !previous.iter().any(|(line, _)| *line == plot)
            {
                previous.push((plot, studies[at].values().line(plot).to_vec()));
            }
        }
        // The consumers of values kept for a full recompute keep theirs until their own turn.
        let kept = stale[at] && pending_from[at] != Some(0);
        super::studies::update(studies, ids, at, bars, from, sources);
        pending_from[at] = None;
        stale[at] = false;
        if !incremental {
            recomputed = true;
            let cost = started.elapsed().as_secs_f64();
            next_refresh[at] = now + cost + REFRESH_INTERVAL.max(4.0 * cost);
        }
        for (plot, previous) in previous {
            let values = studies[at].values().line(plot);
            let from = previous
                .iter()
                .zip(values)
                .position(|(old, new)| old.to_bits() != new.to_bits())
                .or_else(|| {
                    (previous.len() != values.len()).then_some(previous.len().min(values.len()))
                })
                .map_or(from, |changed| from.min(changed));
            for (child, study) in studies.iter().enumerate() {
                if matches!(study.source(), Some(StudySource::Study { id, plot: source_plot }) if id == ids[at].0 && source_plot == plot)
                {
                    if kept {
                        stale[child] = true;
                    } else {
                        pending_from[child] =
                            Some(pending_from[child].map_or(from, |old| old.min(from)));
                    }
                }
            }
        }
    }
    let mut wait = None::<f64>;
    for (at, study) in studies.iter_mut().enumerate() {
        if pending(pending_from, stale, at).is_none() {
            continue;
        }
        // Values kept for a full recompute stay drawn until it runs.
        let left = if pending_from[at] == Some(0) {
            if !study.values().is_empty() {
                study.update(&[], 0);
            }
            0.0
        } else {
            (next_refresh[at] - now).max(0.0)
        };
        wait = Some(wait.map_or(left, |wait| wait.min(left)));
    }
    wait
}

/// The first bar study `at` must be updated from: the first one while it draws values kept for
/// a full recompute.
fn pending(pending_from: &[Option<usize>], stale: &[bool], at: usize) -> Option<usize> {
    if stale[at] { Some(0) } else { pending_from[at] }
}

/// Least time in seconds between two full recomputes of a study while bars stream; a study
/// whose recompute took longer than a quarter of it waits four times that instead.
const REFRESH_INTERVAL: f64 = 0.25;

#[cfg(all(test, feature = "studies"))]
mod tests {
    use crate::model::{Bar, BarData, PriceSource};
    use crate::studies::Indicator;
    use crate::studies::{InputValue, RSI, SMA, StudyError, StudySource};
    use crate::widget::test_frames::frame;
    use crate::widget::{Chart, SeriesId, test_frames::shapes};
    use chrono::{DateTime, Duration, TimeDelta, Utc};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    /// Group hiding covers main and study panes, keeps own settings, and is undoable (#3152).
    #[test]
    fn hiding_all_studies_keeps_each_studys_own_visibility() {
        use crate::studies::{CustomIndicator, IndicatorValue};
        use crate::widget::ChartElementId;
        use crate::widget::test_frames::{chart, frame, shapes};
        use egui::{Color32, Shape, epaint::ColorMode};
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let colors = [
            Color32::from_rgb(1, 2, 3),
            Color32::from_rgb(4, 5, 6),
            Color32::from_rgb(7, 8, 9),
        ];
        let ids: Vec<_> = colors
            .iter()
            .enumerate()
            .map(|(idx, color)| {
                chart.add_indicator(Box::new(
                    CustomIndicator::new(format!("Line {idx}"), |bars| {
                        vec![IndicatorValue::Single(15.0); bars.len()]
                    })
                    .with_color(*color)
                    .with_overlay(idx != 2),
                ))
            })
            .collect();
        chart.set_indicator_visible(ids[0], false);
        let own = |chart: &Chart| {
            chart
                .indicators()
                .filter(|(id, _)| ids.contains(id))
                .map(|(_, study)| study.is_visible())
                .collect::<Vec<_>>()
        };
        let painted = |chart: &mut Chart, time| {
            let output = shapes(&ctx, chart, time, vec![]);
            colors.map(|color| {
                output.iter().any(|shape| match shape {
                    Shape::Path(path) => path.stroke.color == ColorMode::Solid(color),
                    _ => false,
                })
            })
        };
        frame(&ctx, &mut chart, 0.0, vec![]);
        assert_eq!(painted(&mut chart, 1.0), [false, true, true]);
        chart
            .selection
            .select(ChartElementId::Indicator(ids[1]), None);
        chart.user_action(|chart| chart.set_indicators_hidden(true));
        assert_eq!(own(&chart), [false, true, true]);
        assert_eq!(chart.selected_element(), None);
        assert_eq!(painted(&mut chart, 2.0), [false; 3]);
        assert!(!chart.is_series_visible(ids[1]));
        assert!(chart.undo());
        assert!(!chart.indicators_hidden());
        assert_eq!(painted(&mut chart, 3.0), [false, true, true]);
        assert!(chart.redo());
        assert!(chart.indicators_hidden());
        assert_eq!(painted(&mut chart, 4.0), [false; 3]);
        chart.user_action(|chart| chart.set_indicators_hidden(false));
        assert_eq!(own(&chart), [false, true, true]);
        assert_eq!(painted(&mut chart, 5.0), [false, true, true]);
    }

    /// SMA(1)(x) = x, including gaps and past values repainted as new swings arrive.
    #[test]
    fn study_sources_follow_repainting_output_during_appends() {
        let bars: Vec<_> = (0..800)
            .map(|i| {
                let close = 100.0 + 10.0 * (i as f64 / 12.0).sin();
                Bar::new(
                    chrono::DateTime::<Utc>::UNIX_EPOCH + Duration::minutes(i),
                    close,
                    close + 1.0,
                    close - 1.0,
                    close,
                    100.0,
                )
            })
            .collect();
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(bars[..500].to_vec()));
        let parent = chart.add_indicator(Box::new(crate::studies::ZigZag::default()));
        let child = chart.add_indicator(Box::new(SMA::new(1)));
        chart
            .set_indicator_source(
                child,
                StudySource::Study {
                    id: parent.0,
                    plot: 0,
                },
            )
            .unwrap();
        for frame in 0..6 {
            shapes(&ctx, &mut chart, frame as f64, Vec::new());
        }
        for (at, bar) in bars[500..].iter().enumerate() {
            chart.update(SeriesId::MAIN, bar.clone());
            shapes(&ctx, &mut chart, 6.0 + at as f64, Vec::new());
        }
        for frame in 306..312 {
            shapes(&ctx, &mut chart, frame as f64, Vec::new());
        }
        let output = |id| {
            chart
                .indicators()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1
                .values()
        };
        assert_eq!(output(parent).len(), bars.len());
        assert_eq!(output(child).len(), bars.len());
        for at in 0..bars.len() {
            assert_eq!(
                output(child).value(0, at).map(f64::to_bits),
                output(parent).value(0, at).map(f64::to_bits),
                "bar {at}: SMA(1) must equal its source"
            );
        }
    }

    /// One-bar means follow a continuous stream in dependency order, within one full refresh per pass.
    #[test]
    fn study_dependencies_refresh_during_continuous_ticks() {
        for (reverse, count, incremental_source) in [
            (false, 2, true),
            (true, 2, true),
            (false, 3, true),
            (true, 3, false),
        ] {
            let ctx = egui::Context::default();
            let bars: Vec<_> = (0..1_000)
                .map(|i| {
                    Bar::new(
                        chrono::DateTime::<Utc>::UNIX_EPOCH + Duration::minutes(i),
                        10.0,
                        10.0,
                        10.0,
                        10.0,
                        100.0,
                    )
                })
                .collect();
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            let mut ids: Vec<_> = (0..count)
                .map(|_| chart.add_indicator(Box::new(SMA::new(1))))
                .collect();
            if reverse {
                ids.reverse();
            }
            if !incremental_source {
                chart
                    .set_indicator_source(ids[0], StudySource::Price(PriceSource::High))
                    .unwrap();
            }
            for pair in ids.windows(2) {
                chart
                    .set_indicator_source(
                        pair[1],
                        StudySource::Study {
                            id: pair[0].0,
                            plot: 0,
                        },
                    )
                    .unwrap();
            }
            for time in 0..6 {
                shapes(&ctx, &mut chart, time as f64, Vec::new());
            }
            chart.next_refresh.fill(f64::NEG_INFINITY);
            for tick in 1..=60 {
                let close = 10.0 + tick as f64 * 0.01;
                let mut bar = bars.last().unwrap().clone();
                bar.close = close;
                bar.high = close;
                chart.update(SeriesId::MAIN, bar);
                let refresh = chart.next_refresh.clone();
                let pass = ctx.cumulative_pass_nr();
                let now = 6.0 + tick as f64;
                shapes(&ctx, &mut chart, now, Vec::new());
                assert!(
                    chart
                        .next_refresh
                        .iter()
                        .zip(&refresh)
                        .filter(|(a, b)| **a != **b)
                        .count()
                        <= (ctx.cumulative_pass_nr() - pass) as usize,
                    "at most one full recompute per pass"
                );
                // Model equal costs so refresh ordering does not depend on runner preemption.
                for (next, before) in chart.next_refresh.iter_mut().zip(refresh) {
                    if *next != before {
                        *next = now + super::REFRESH_INTERVAL;
                    }
                }
                for (id, study) in chart.indicators().filter(|(id, _)| ids.contains(id)) {
                    let value = study.values().value(0, 999).unwrap();
                    // SMA(1) equals its input. Each full stage can wait for the other full
                    // studies before its latest input reaches the next stage.
                    let full = count - usize::from(incremental_source);
                    let lag = full * (full - 1);
                    assert!(
                        value <= close + 1e-12 && value >= close - lag as f64 * 0.01 - 1e-12,
                        "{id:?}, tick {tick}: {value}, expected {close} within {lag} ticks"
                    );
                }
            }
        }
    }

    #[test]
    fn price_sources_and_study_dependencies_follow_edits_and_live_bars() {
        use crate::studies::{CustomIndicator, IndicatorValue, InputKind, StudyInput};
        let start = chrono::DateTime::<Utc>::UNIX_EPOCH;
        let bars: Vec<_> = (0..3)
            .map(|i| {
                Bar::new(
                    start + Duration::minutes(i),
                    1.0 + 2.0 * i as f64,
                    5.0 + 4.0 * i as f64,
                    1.0 + 2.0 * i as f64,
                    3.0 + 2.0 * i as f64,
                    1.0,
                )
            })
            .collect();
        let ctx = egui::Context::default();
        let settle = |chart: &mut Chart| {
            for frame in 0..6 {
                shapes(&ctx, chart, frame as f64, Vec::new());
            }
        };
        for (source, expected, shifted) in [
            (PriceSource::Open, 4.0, 2.0),
            (PriceSource::High, 11.0, 7.0),
            (PriceSource::Low, 4.0, 2.0),
            (PriceSource::Close, 6.0, 4.0),
            (PriceSource::HL2, 7.5, 4.5),
            (PriceSource::HLC3, 7.0, 13.0 / 3.0),
            (PriceSource::OHLC4, 6.25, 3.75),
        ] {
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            let id = chart.add_indicator(Box::new(SMA::new(2)));
            let value = InputValue::Source(StudySource::Price(source));
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input("source", value)
                .unwrap();
            settle(&mut chart);
            assert_eq!(
                chart.indicators().next().unwrap().1.values().value(0, 2),
                Some(expected),
                "{source:?}"
            );
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input("plot_offset", InputValue::Integer(1))
                .unwrap();
            settle(&mut chart);
            let study = chart.indicators().next().unwrap().1;
            assert!(
                (study.plot_value(0, 2).unwrap() - shifted).abs() < 1e-12,
                "{source:?}"
            );
            assert_eq!(study.values().value(0, 2), Some(expected));
        }
        // RSI(1) of 1, 2, 1, 2 is missing, 100, 0, 100. Its two-bar mean ends at 50.
        let bars: Vec<_> = [1.0, 2.0, 1.0, 2.0]
            .into_iter()
            .enumerate()
            .map(|(i, p)| Bar::new(start + Duration::minutes(i as i64), p, p, p, p, 1.0))
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        let mean = chart.add_indicator(Box::new(SMA::new(2)));
        let rsi = chart.add_indicator(Box::new(RSI::new(1)));
        // A custom study that plots its source reads the RSI once the chart supplies it.
        let echo = chart.add_indicator(Box::new(
            CustomIndicator::new("Echo", |_| Vec::new())
                .with_inputs(
                    vec![StudyInput::new(
                        "source",
                        "Source",
                        InputValue::Source(StudySource::Price(PriceSource::Close)),
                        InputKind::Source,
                    )],
                    |_, _, source, _| source.iter().copied().map(IndicatorValue::Single).collect(),
                )
                .unwrap(),
        ));
        chart
            .set_indicator_source(echo, StudySource::Study { id: rsi.0, plot: 0 })
            .unwrap();
        assert!(matches!(
            chart.set_indicator_source(echo, StudySource::Study { id: 999, plot: 0 }),
            Err(StudyError::UnknownSourceStudy(999))
        ));
        let echoed = |chart: &Chart| {
            let (_, study) = chart.indicators().find(|(id, _)| *id == echo).unwrap();
            (0..4)
                .map(|bar| study.values().value(0, bar))
                .collect::<Vec<_>>()
        };

        let value = InputValue::Source(StudySource::Study { id: rsi.0, plot: 0 });
        chart
            .indicator_mut(mean)
            .unwrap()
            .set_input("source", value)
            .unwrap();
        chart.set_indicator_visible(rsi, false);
        settle(&mut chart);
        let output = |chart: &Chart, bar| {
            chart
                .indicators()
                .find(|(id, _)| *id == mean)
                .unwrap()
                .1
                .values()
                .value(0, bar)
        };
        assert_eq!(output(&chart, 1), None);
        assert!((output(&chart, 3).unwrap() - 50.0).abs() < 1e-8);
        assert_eq!(echoed(&chart), [None, Some(100.0), Some(0.0), Some(100.0)]);
        chart.update(
            SeriesId::MAIN,
            Bar::new(bars[3].time, 0.5, 0.5, 0.5, 0.5, 1.0),
        );
        settle(&mut chart);
        assert_eq!(output(&chart, 3), Some(0.0));
        assert_eq!(echoed(&chart), [None, Some(100.0), Some(0.0), Some(0.0)]);
        assert!(matches!(
            chart.set_indicator_source(
                rsi,
                StudySource::Study {
                    id: echo.0,
                    plot: 0
                }
            ),
            Err(StudyError::CyclicSource)
        ));

        chart
            .indicator_mut(rsi)
            .unwrap()
            .set_input("period", InputValue::Length(2))
            .unwrap();
        settle(&mut chart);
        // Wilder averages after +1, -1, -0.5 are gain=0.25, loss=0.5: RSI=100/3.
        assert!((output(&chart, 3).unwrap() - (50.0 + 100.0 / 3.0) / 2.0).abs() < 1e-8);
        assert!(matches!(
            chart.set_indicator_source(
                rsi,
                StudySource::Study {
                    id: mean.0,
                    plot: 0
                }
            ),
            Err(StudyError::CyclicSource)
        ));
        #[cfg(feature = "csv")]
        {
            let csv = chart.export_csv().unwrap();
            assert!(csv.contains(",41.67"), "{csv}");
        }
        chart.user_action(|chart| chart.remove_series(rsi));
        assert!(chart.undo());
        settle(&mut chart);
        assert!((output(&chart, 3).unwrap() - 125.0 / 3.0).abs() < 1e-8);
        // A study whose own source is gone still takes consumers, which read its gaps.
        chart
            .indicator_mut(echo)
            .unwrap()
            .set_input(
                "source",
                InputValue::Source(StudySource::Study { id: 999, plot: 0 }),
            )
            .unwrap();
        chart
            .set_indicator_source(
                mean,
                StudySource::Study {
                    id: echo.0,
                    plot: 0,
                },
            )
            .unwrap();
        settle(&mut chart);
        assert_eq!(output(&chart, 3), None);
    }

    /// Removing a study removes the studies that read its output, directly or through a chain,
    /// as one undo step, and keeps its sources and a study whose user edits are off (#3464).
    #[test]
    fn removing_a_study_removes_the_studies_that_read_it() {
        let start = DateTime::<Utc>::UNIX_EPOCH;
        let bars: Vec<_> = [1.0, 2.0, 1.0, 2.0]
            .into_iter()
            .enumerate()
            .map(|(i, p)| Bar::new(start + Duration::minutes(i as i64), p, p, p, p, 1.0))
            .collect();
        // Studies 0 to 3: RSI(1), a two-bar mean of it, a two-bar mean of that mean, and a
        // three-bar mean of the closes. RSI(1) of 1, 2, 1, 2 is missing, 100, 0, 100, so the
        // first mean is missing, missing, 50, 50 and the second ends at 50.
        let mut failures = Vec::new();
        for (removed, locked, remaining) in [
            (0, None, vec![3]),
            (1, None, vec![0, 3]),
            (2, None, vec![0, 1, 3]),
            (0, Some(1), vec![1, 2, 3]),
        ] {
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            let rsi = chart.add_indicator(Box::new(RSI::new(1)));
            let mean = chart.add_indicator(Box::new(SMA::new(2)));
            let mean_of_mean = chart.add_indicator(Box::new(SMA::new(2)));
            let ids = [
                rsi,
                mean,
                mean_of_mean,
                chart.add_indicator(Box::new(SMA::new(3))),
            ];
            for (study, source) in [(mean, rsi), (mean_of_mean, mean)] {
                let source = StudySource::Study {
                    id: source.0,
                    plot: 0,
                };
                chart.set_indicator_source(study, source).unwrap();
            }
            if let Some(locked) = locked {
                assert!(chart.set_indicator_user_edit_enabled(ids[locked], false));
            }
            chart.recalculate_studies();
            let studies = |chart: &Chart| chart.indicators().map(|(id, _)| id).collect::<Vec<_>>();
            assert!(chart.user_action(|chart| chart.remove_series(ids[removed])));
            let expected: Vec<_> = remaining.iter().map(|at| ids[*at]).collect();
            if studies(&chart) != expected {
                failures.push(format!(
                    "removing {removed}, locked {locked:?}: {:?}",
                    studies(&chart)
                ));
            }
            assert!(chart.undo());
            assert_eq!(studies(&chart), ids);
            for id in [mean, mean_of_mean] {
                let (_, study) = chart.indicators().find(|(study, _)| *study == id).unwrap();
                assert!((study.values().value(0, 3).unwrap() - 50.0).abs() < 1e-8);
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// A study of another study shares its pane and scale without changing the price range.
    #[test]
    fn studies_on_study_output_open_with_their_source() {
        use crate::studies::{ConfiguredStudy, Volume};
        use crate::widget::test_frames;

        type Make = fn() -> Box<dyn Indicator>;
        type Row = (Make, Make, bool);
        let rows: [Row; 4] = [
            (|| Box::new(RSI::new(14)), || Box::new(SMA::new(9)), true),
            (|| Box::new(SMA::new(2)), || Box::new(SMA::new(9)), true),
            (
                || Box::new(Volume::default()),
                || Box::new(SMA::new(9)),
                true,
            ),
            (|| Box::new(SMA::new(2)), || Box::new(RSI::new(14)), false),
        ];
        for (parent, child, shared) in rows {
            let ctx = egui::Context::default();
            let mut chart = test_frames::chart(&[1000.0, 1040.0], &[1060.0, 1100.0]);
            chart.config_mut().show_volume = false;
            let parent = chart.add_indicator(parent());
            shapes(&ctx, &mut chart, 0.0, vec![]);
            let range = chart.state.price_scale().range();
            let panes = chart.panes().len();
            let mut child = ConfiguredStudy::new(child());
            child
                .set_input(
                    "source",
                    InputValue::Source(StudySource::Study {
                        id: parent.0,
                        plot: 0,
                    }),
                )
                .unwrap();
            let child = chart.add_indicator(Box::new(child));
            shapes(&ctx, &mut chart, 0.1, vec![]);
            assert_eq!(chart.state.price_scale().range(), range);
            assert_eq!(
                chart.series_pane(child) == chart.series_pane(parent),
                shared
            );
            assert_eq!(chart.panes().len(), panes + usize::from(!shared));
            if shared {
                let scale = |id| {
                    chart
                        .indicators()
                        .find(|(study, _)| *study == id)
                        .unwrap()
                        .1
                        .scale()
                };
                assert_eq!(scale(child), scale(parent));
            }
        }
    }

    /// A flat bar at `close`, stamped `seconds` after 10:00.
    fn bar(seconds: i64, close: f64) -> Bar {
        let t =
            "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap() + TimeDelta::seconds(seconds);
        Bar::new(t, close, close, close, close, 1.0)
    }

    /// RSI, counting the calls that recompute it.
    struct Counted(RSI, Arc<AtomicUsize>, std::time::Duration);

    impl Indicator for Counted {
        fn name(&self) -> &str {
            self.0.name()
        }
        fn calculate(&mut self, data: &[Bar]) {
            if !data.is_empty() {
                self.1.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(self.2);
            }
            self.0.calculate(data);
        }
        fn update(&mut self, data: &[Bar], from: usize) {
            if !data.is_empty() {
                self.1.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(self.2);
            }
            self.0.update(data, from);
        }
        fn values(&self) -> &crate::studies::IndicatorValues {
            self.0.values()
        }
        fn colors(&self) -> Vec<egui::Color32> {
            self.0.colors()
        }
        fn set_colors(&mut self, colors: Vec<egui::Color32>) {
            self.0.set_colors(colors);
        }
        fn is_overlay(&self) -> bool {
            self.0.is_overlay()
        }
        fn set_visible(&mut self, visible: bool) {
            self.0.set_visible(visible);
        }
        fn clone_box(&self) -> Box<dyn Indicator> {
            Box::new(Counted(self.0.clone(), Arc::clone(&self.1), self.2))
        }
    }

    /// A replaced series gets one full recompute per frame, even while every wait is still
    /// running, and waiting studies request another frame until the last study is current. They
    /// have no values after the bars are replaced. After a bar limit evicts the oldest bars they
    /// keep the values of the bars that remain, and wait for their next full recompute.
    #[test]
    fn full_recomputes_run_one_per_frame() {
        let mut chart = Chart::new(BarData::from_bars(
            (0..100).map(|i| bar(i * 60, i as f64)).collect(),
        ));
        let calls: Vec<_> = (0..3).map(|_| Arc::new(AtomicUsize::new(0))).collect();
        for counter in &calls {
            chart.add_indicator(Box::new(Counted(
                RSI::new(14),
                counter.clone(),
                std::time::Duration::ZERO,
            )));
            counter.store(0, Ordering::Relaxed);
        }
        let counts = || -> Vec<_> {
            calls
                .iter()
                .map(|counter| counter.load(Ordering::Relaxed))
                .collect()
        };
        // The second round replaces the bars while the waits of the first are still running. The
        // third appends a bar past a limit of 100, which keeps 100 - 100 / 8 = 88 bars: the oldest
        // 13 go, and a waiting study keeps the values of bars 13 to 99 as those of bars 0 to 86,
        // with none yet for the new bar 87.
        let mut evicted = RSI::new(14);
        let kept = |study: &dyn Indicator, evicted: &RSI| {
            // What the chart draws on bars 0 to 87: old bars 13 to 100.
            let drawn: Vec<_> = (0..88).map(|bar| study.plot_value(0, bar)).collect();
            let kept: Vec<_> = (13..101).map(|bar| evicted.plot_value(0, bar)).collect();
            study.values().len() == 87 && drawn == kept
        };
        for round in 0..3 {
            if round == 1 {
                // Model unequal measured costs without depending on the runner clock.
                chart.next_refresh.copy_from_slice(&[3.0, 2.0, 1.0]);
            }
            for counter in &calls {
                counter.store(0, Ordering::Relaxed);
            }
            if round < 2 {
                let close = [200.0, 300.0][round];
                chart.set_data(
                    SeriesId::MAIN,
                    (0..100)
                        .map(|i| bar(i * 60, close + (i * 7 % 13) as f64))
                        .collect(),
                );
            } else {
                evicted.calculate(chart.bars(SeriesId::MAIN));
                chart.set_bar_limit(SeriesId::MAIN, std::num::NonZeroUsize::new(100));
                chart.update(SeriesId::MAIN, bar(100 * 60, 400.0));
                assert_eq!(chart.bars(SeriesId::MAIN).len(), 88);
                // Before the waits of the second round end, every study draws what it kept.
                let wait = chart.refresh_studies(3.0 / 60.0);
                assert_eq!(counts(), [0, 0, 0]);
                assert!(chart.indicators().all(|(_, study)| kept(study, &evicted)));
                assert!(wait.is_some_and(|wait| wait > 0.0), "{wait:?}");
                // Old bar 14, by hand: the first 14 changes of 300 + 7i mod 13 gain 7 seven times
                // and lose 6 seven times, so RSI = 100 * 49 / (49 + 42).
                let (_, study) = chart.indicators().next().unwrap();
                let rsi = study.plot_value(0, 1).unwrap();
                assert!((rsi - 100.0 * 49.0 / 91.0).abs() < 1e-9, "{rsi}");
            }
            for k in 1..=3 {
                let now = [k as f64 / 60.0, 3.0 / 60.0, 1.0][round];
                let wait = chart.refresh_studies(now);
                let counts = counts();
                assert_eq!(counts.iter().sum::<usize>(), k, "round {round}, call {k}");
                for (j, (_, study)) in chart.indicators().enumerate() {
                    assert!(counts[j] <= 1, "round {round}, call {k}, study {j}");
                    if round < 2 || counts[j] == 1 {
                        assert_eq!(
                            study.values().is_empty(),
                            counts[j] == 0,
                            "round {round}, call {k}, study {j}"
                        );
                    } else {
                        assert!(kept(study, &evicted), "round {round}, call {k}, study {j}");
                    }
                }
                assert_eq!(wait, if k < 3 { Some(0.0) } else { None });
            }
        }
        let mut fresh = RSI::new(14);
        fresh.calculate(chart.bars(SeriesId::MAIN));
        for (_, study) in chart.indicators() {
            assert_eq!(
                format!("{:?}", study.values()),
                format!("{:?}", fresh.values())
            );
        }
    }

    /// The real cost advances the frame clock: re-arming from the frame start would make
    /// every frame due again when a recompute takes longer than the minimum wait.
    #[test]
    fn a_slow_full_recompute_does_not_take_every_frame() {
        let mut chart = Chart::new(BarData::from_bars(
            (0..100).map(|i| bar(i * 60, i as f64)).collect(),
        ));
        let calls = Arc::new(AtomicUsize::new(0));
        chart.add_indicator(Box::new(Counted(
            RSI::new(14),
            calls.clone(),
            std::time::Duration::from_millis(300),
        )));
        calls.store(0, Ordering::Relaxed);
        let mut time = 0.0;
        for i in 0..10 {
            chart.update(SeriesId::MAIN, bar(99 * 60, 50.0 + i as f64));
            let started = Instant::now();
            chart.refresh_studies(time);
            time += 1.0 / 60.0 + started.elapsed().as_secs_f64();
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    /// Bars set every frame, as a replay does: each study gets a turn despite recompute costs.
    /// Resetting the waits on `set_data` gave the first study every frame and the others none.
    #[test]
    fn set_data_every_frame_recomputes_each_study_in_turn() {
        for cost in [
            std::time::Duration::ZERO,
            std::time::Duration::from_millis(20),
        ] {
            let mut bars: Vec<Bar> = (0..100).map(|i| bar(i * 60, i as f64)).collect();
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            let calls: Vec<_> = (0..3).map(|_| Arc::new(AtomicUsize::new(0))).collect();
            for counter in &calls {
                chart.add_indicator(Box::new(Counted(RSI::new(14), counter.clone(), cost)));
                counter.store(0, Ordering::Relaxed);
            }
            for k in 0..120 {
                bars.push(bar((100 + k) * 60, 50.0 + k as f64));
                chart.set_data(SeriesId::MAIN, bars.clone());
                chart.refresh_studies(k as f64 / 60.0);
            }
            // Measured costs determine how many turns fit; no study may starve.
            let calls: Vec<_> = calls.iter().map(|c| c.load(Ordering::Relaxed)).collect();
            assert!(
                calls.iter().all(|&count| count > 0),
                "cost {cost:?}: {calls:?}"
            );
        }
    }

    /// Guards the cost and the freshness of studies while bars stream: a study that does not
    /// update incrementally is refreshed at most four times a second and once more after the
    /// stream stops, an incremental one on every frame, and both end equal to a fresh
    /// calculation. A refresh on every frame, a pending change lost after the last update, or
    /// the wait applied to an incremental study fails it.
    #[test]
    fn slow_studies_refresh_four_times_a_second_and_end_current() {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::from_bars(
            (0..100).map(|i| bar(i * 60, i as f64)).collect(),
        ));
        let sma = chart.add_indicator(Box::new(SMA::new(5)));
        let calls = Arc::new(AtomicUsize::new(0));
        let rsi = chart.add_indicator(Box::new(Counted(
            RSI::new(14),
            Arc::clone(&calls),
            std::time::Duration::ZERO,
        )));
        let values = |chart: &Chart, id| {
            let (_, study) = chart.indicators().find(|(study, _)| *study == id).unwrap();
            format!("{:?}", study.values())
        };
        let fresh = |chart: &Chart, mut study: Box<dyn Indicator>| {
            study.calculate(chart.bars(SeriesId::MAIN));
            format!("{:?}", study.values())
        };
        frame(&ctx, &mut chart, 0.0, Vec::new());
        calls.store(0, Ordering::Relaxed);
        let mut time = 0.0;
        for i in 0..60 {
            time += 1.0 / 60.0;
            // Alternately a new close of the last bar and a new bar.
            chart.update(
                SeriesId::MAIN,
                bar((100 + i / 2) * 60, 50.0 + (i * 7 % 13) as f64),
            );
            frame(&ctx, &mut chart, time, Vec::new());
            assert_eq!(
                values(&chart, sma),
                fresh(&chart, Box::new(SMA::new(5))),
                "SMA at {i}"
            );
        }
        assert!(
            calls.load(Ordering::Relaxed) <= 4,
            "{calls:?} refreshes while streaming"
        );
        let streamed = calls.load(Ordering::Relaxed);
        let stop = time;
        while time < stop + 0.25 {
            time += 1.0 / 60.0;
            frame(&ctx, &mut chart, time, Vec::new());
        }
        assert_eq!(
            calls.load(Ordering::Relaxed),
            streamed + 1,
            "one refresh after the stream"
        );
        assert_eq!(
            values(&chart, rsi),
            fresh(&chart, Box::new(RSI::new(14))),
            "RSI"
        );
    }

    /// A chart capped at two studies refuses a third from the interface and pastes, and stays
    /// as it was; host code and lifting the cap still add.
    #[cfg(feature = "drawings")]
    #[test]
    fn the_study_limit_refuses_additions_beyond_it() {
        let mut chart = super::super::test_frames::chart(&[100.0], &[110.0]);
        let copied = {
            let id = chart.add_indicator(Box::new(SMA::new(9)));
            let text = chart.copy_indicator(id).unwrap();
            chart.remove_series(id);
            text
        };
        chart.set_study_limit(Some(2));
        for period in [3, 5] {
            assert!(chart.try_add_indicator(Box::new(SMA::new(period))).is_ok());
        }
        let undoable = chart.can_undo();
        let period = |study: Box<dyn crate::studies::Indicator>| {
            study
                .inputs()
                .into_iter()
                .find(|input| input.id == "period")
                .map(|input| input.value)
        };
        let Err(StudyError::StudyLimitReached(refused)) =
            chart.try_add_indicator(Box::new(SMA::new(7)))
        else {
            panic!("the cap refuses a third study");
        };
        assert_eq!(period(refused.into_inner()), Some(InputValue::Length(7)));
        let Err(StudyError::StudyLimitReached(refused)) =
            chart.paste_indicator(&copied, &crate::studies::IndicatorFactory::new())
        else {
            panic!("the cap refuses a pasted study");
        };
        assert_eq!(period(refused.into_inner()), Some(InputValue::Length(9)));
        assert_eq!(chart.indicators().count(), 2);
        assert_eq!(chart.can_undo(), undoable);
        chart.add_indicator(Box::new(SMA::new(7)));
        assert_eq!(chart.indicators().count(), 3);
        chart.set_study_limit(None);
        assert!(chart.try_add_indicator(Box::new(SMA::new(8))).is_ok());
    }
    /// History restores user settings while retaining the host's latest edit permission.
    #[test]
    fn undo_keeps_host_study_lock() {
        for enabled in [false, true] {
            let mut chart = super::super::test_frames::chart(&[100.0], &[110.0]);
            let id = chart.add_indicator(Box::new(SMA::new(1)));
            assert!(chart.set_indicator_user_edit_enabled(id, !enabled));
            let before = chart.indicators().next().unwrap().1.settings();
            let mut edited = before.clone();
            edited.precision = Some(4);
            edited.show_legend_values = false;
            edited.plots[0].width = 3.0;
            chart.user_action(|chart| {
                chart
                    .indicator_mut(id)
                    .unwrap()
                    .apply_settings(&[], edited.clone())
                    .unwrap();
            });
            // The host changes the permission again before each step.
            for (redo, expected, host) in [(false, &before, enabled), (true, &edited, !enabled)] {
                assert!(chart.set_indicator_user_edit_enabled(id, host));
                assert!(if redo { chart.redo() } else { chart.undo() });
                assert_eq!(chart.indicator_user_edit_enabled(id), host, "redo={redo}");
                let restored = chart.indicators().next().unwrap().1.settings();
                assert_eq!(restored.precision, expected.precision);
                assert_eq!(restored.show_legend_values, expected.show_legend_values);
                assert_eq!(restored.plots, expected.plots);
            }
        }
    }

    /// Saved study presentation hides only legend values and keeps edit permissions across history.
    #[cfg(feature = "drawings")]
    #[test]
    fn study_metadata_keeps_values_and_blocks_user_settings() {
        use crate::studies::{IndicatorFactory, StudySettings};
        let custom = crate::studies::CustomIndicator::new("SMA", |_| Vec::new());
        assert!(
            serde_json::to_value(custom.settings()).unwrap()["help_url"].is_null(),
            "a shared name does not identify a built-in"
        );
        assert!(serde_json::to_value(SMA::new(1).settings()).unwrap()["help_url"].is_string());
        let articles: [(Box<dyn Indicator>, &str); 11] = [
            (
                Box::new(crate::studies::SMIErgodicIndicator::default()),
                "SMIErgodicIndicator",
            ),
            (
                Box::new(crate::studies::SMIErgodicOscillator::default()),
                "SMIErgodicOscillator",
            ),
            (
                Box::new(crate::studies::RisingWedge::default()),
                "RisingWedge",
            ),
            (
                Box::new(crate::studies::FallingWedge::default()),
                "FallingWedge",
            ),
            (Box::new(crate::studies::Ratio::default()), "Ratio"),
            (Box::new(crate::studies::Spread::default()), "Spread"),
            (
                Box::new(crate::studies::Correlation::default()),
                "Correlation",
            ),
            (
                Box::new(crate::studies::CorrelationLog::default()),
                "CorrelationLog",
            ),
            (
                Box::new(crate::studies::AdvanceDeclineRatio::default()),
                "AdvanceDeclineRatio",
            ),
            (
                Box::new(crate::studies::AdvanceDeclineLine::default()),
                "AdvanceDeclineLine",
            ),
            (
                Box::new(crate::studies::CumulativeVolumeIndex::default()),
                "CumulativeVolumeIndex",
            ),
        ];
        for (study, article) in articles {
            assert!(
                study
                    .help_url()
                    .unwrap()
                    .ends_with(&format!("/type.{article}.html"))
            );
        }

        let mut chart = super::super::test_frames::chart(&[100.0], &[110.0]);
        let id = chart.add_indicator(Box::new(SMA::new(1)));
        let study = chart.indicator_mut(id).unwrap();
        let mut saved = serde_json::to_value(study.settings()).unwrap();
        saved["show_legend_values"] = false.into();
        saved["user_edit_enabled"] = false.into();
        saved["help_url"] = "https://example.org/studies/average".into();
        let settings: StudySettings = serde_json::from_value(saved).unwrap();
        chart.user_action(|chart| {
            chart
                .indicator_mut(id)
                .unwrap()
                .apply_settings(&[], settings)
                .unwrap();
        });
        let study = chart
            .indicators()
            .find(|(candidate, _)| *candidate == id)
            .unwrap()
            .1;
        assert!(!study.legend_visible(0), "only the status value is hidden");
        assert!(study.plot_style(0).visible);
        assert!(study.plot_tagged(0));
        assert_eq!(study.plot_value(0, 0), Some(110.0));
        let copy = chart.copy_indicator(id).unwrap();
        let pasted = chart
            .paste_indicator(&copy, &IndicatorFactory::new())
            .unwrap();
        let copied = chart
            .indicators()
            .find(|(candidate, _)| *candidate == pasted)
            .unwrap()
            .1;
        assert!(!copied.legend_visible(0));
        let copied_settings = serde_json::to_value(copied.settings()).unwrap();
        assert_eq!(copied_settings["user_edit_enabled"], false);
        assert_eq!(
            copied_settings["help_url"],
            "https://example.org/studies/average"
        );
        #[cfg(feature = "ui")]
        {
            let mut dialog =
                crate::ui::dialogs::indicator_dialog::IndicatorSettingsDialog::default();
            assert!(!dialog.open_for_chart(&chart, id));
            let saved = chart.snapshot().unwrap();
            let saved = serde_json::to_string(&saved).unwrap();
            let mut restored = super::super::test_frames::chart(&[100.0], &[110.0]);
            restored
                .restore(&serde_json::from_str(&saved).unwrap())
                .unwrap();
            for (_, study) in restored.indicators() {
                let settings = serde_json::to_value(study.settings()).unwrap();
                assert_eq!(settings["show_legend_values"], false);
                assert_eq!(settings["user_edit_enabled"], false);
                assert_eq!(settings["help_url"], "https://example.org/studies/average");
            }
        }
        let mut legacy = copied_settings;
        for field in ["show_legend_values", "user_edit_enabled", "help_url"] {
            legacy.as_object_mut().unwrap().remove(field);
        }
        let legacy: StudySettings = serde_json::from_value(legacy).unwrap();
        let legacy = serde_json::to_value(legacy).unwrap();
        assert_eq!(legacy["show_legend_values"], true);
        assert!(legacy["user_edit_enabled"].is_null());
        assert!(legacy["help_url"].is_null());
        // The metadata is part of the same presentation snapshot as the plot styles.
        chart.user_action(|chart| {
            chart.remove_series(pasted);
        });
        assert!(chart.undo());
        assert!(chart.undo());
        assert!(chart.undo());
        assert!(
            chart
                .indicators()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1
                .legend_visible(0)
        );
        assert!(chart.redo());
        assert!(
            !chart
                .indicators()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1
                .legend_visible(0)
        );
    }

    #[test]
    fn api_errors_are_std_errors() {
        fn needs_error<E: std::error::Error + Send + Sync + 'static>() {}
        needs_error::<crate::model::ParseError>();
        needs_error::<crate::model::PnfBoxesError>();
        needs_error::<crate::widget::ExportError>();
        needs_error::<crate::studies::StudyError>();
        needs_error::<crate::widget::settings::ChartSettingsError>();
        #[cfg(feature = "ui")]
        needs_error::<crate::widget::RestoreError>();
        #[cfg(feature = "drawings")]
        needs_error::<crate::widget::DrawingError>();
    }

    /// Without a host caption, the legends show one built from the study's inputs, which follows
    /// every edit, undo and redo until the host sets its own (#3570).
    #[test]
    fn derived_caption_follows_settings_undo_and_redo() {
        let mut chart = Chart::new(BarData::new());
        let rsi = chart.add_indicator(Box::new(RSI::new(14)));
        // Without smoothing, its length and deviation are hidden.
        assert_eq!(chart.indicator_inputs(rsi), Some("14 close"));
        chart.user_action(|chart| {
            let study = chart.indicator_mut(rsi).unwrap();
            let settings = study.settings();
            study
                .apply_settings(&[("period".into(), InputValue::Length(5))], settings)
                .unwrap();
        });
        assert_eq!(chart.indicator_inputs(rsi), Some("5 close"));
        assert!(chart.undo());
        assert_eq!(chart.indicator_inputs(rsi), Some("14 close"));
        assert!(chart.redo());
        assert_eq!(chart.indicator_inputs(rsi), Some("5 close"));
        chart
            .indicator_mut(rsi)
            .unwrap()
            .set_input("smoothing", InputValue::Choice(1))
            .unwrap();
        assert_eq!(chart.indicator_inputs(rsi), Some("5 close SMA 14"));
        chart.set_indicator_inputs(rsi, "Mine");
        chart
            .indicator_mut(rsi)
            .unwrap()
            .set_input("period", InputValue::Length(7))
            .unwrap();
        assert_eq!(chart.indicator_inputs(rsi), Some("Mine"));
        chart.set_indicator_inputs(rsi, "");
        assert_eq!(chart.indicator_inputs(rsi), Some("7 close SMA 14"));
    }

    /// The derived caption names each input by its value and leaves out what means nothing
    /// without its label (#3570).
    #[test]
    fn derived_captions_by_input_kind() {
        use crate::model::Timeframe;
        use crate::studies::{CustomIndicator, InputKind as Kind, InputValue as Value, StudyInput};
        let input = |id: &str, value, kind| StudyInput::new(id, id, value, kind);
        let one = |value, kind| vec![input("value", value, kind)];
        let length =
            |id: &str, value, min, max| input(id, Value::Length(value), Kind::Length(min, max));
        let choice = |index, labels: [&str; 2]| {
            let labels = labels.map(String::from).into();
            input("mode", Value::Choice(index), Kind::Choice(labels))
        };
        let price = |price| Value::Source(StudySource::Price(price));
        let close = input("source", price(PriceSource::Close), Kind::Source);
        let rows = [
            (vec![length("length", 20, 1, 500)], "20"),
            (vec![length("offset", 0, 0, 500)], ""),
            // Minutes after midnight.
            (vec![length("start", 570, 0, 1439)], ""),
            (vec![length("width", 3, 1, 10)], ""),
            (vec![length("rows", 30, 1, 500).in_style_tab()], ""),
            (one(Value::Number(2.5), Kind::Number(0.0, 10.0)), "2.5"),
            (one(Value::Number(-1.5), Kind::Number(-10.0, 10.0)), "-1.5"),
            // An open upper limit.
            (
                one(Value::Number(f64::MAX), Kind::Number(0.0, f64::MAX)),
                "",
            ),
            (one(Value::Integer(-3), Kind::Integer(-10, 10)), "-3"),
            (vec![choice(1, ["Fast", "Slow"])], "Slow"),
            (vec![choice(0, ["None", "Slow"])], ""),
            (
                vec![input("source", price(PriceSource::HL2), Kind::Source)],
                "hl2",
            ),
            (
                one(Value::Timeframe(Some(Timeframe::Hour1)), Kind::Timeframe),
                "1h",
            ),
            (one(Value::Timeframe(None), Kind::Timeframe), ""),
            (one(Value::Symbol("OTHER".into()), Kind::Symbol), "OTHER"),
            (one(Value::Symbol(String::new()), Kind::Symbol), ""),
            (one(Value::Toggle(true), Kind::Toggle), ""),
            (one(Value::Text("hi".into()), Kind::Text), ""),
            (one(Value::Color(egui::Color32::RED), Kind::Color), ""),
            // The price follows the first input.
            (
                vec![
                    length("length", 20, 1, 500),
                    choice(1, ["Fast", "Slow"]),
                    close.clone(),
                ],
                "20 close Slow",
            ),
            // A study anchored by a choice names its price only once it is edited.
            (
                vec![
                    choice(0, ["Session", "Week"]),
                    length("length", 1, 1, 10),
                    close,
                ],
                "Session 1",
            ),
        ];
        let mut chart = Chart::new(BarData::new());
        for (at, (inputs, expected)) in rows.into_iter().enumerate() {
            let study = CustomIndicator::new("X", |_| Vec::new())
                .with_inputs(inputs, |_, _, _, _| Vec::new())
                .unwrap();
            let id = chart.add_indicator(Box::new(study));
            assert_eq!(chart.indicator_inputs(id), Some(expected), "row {at}");
        }
        let anchored = chart.indicators().last().unwrap().0;
        chart
            .indicator_mut(anchored)
            .unwrap()
            .set_input("source", price(PriceSource::HLC3))
            .unwrap();
        assert_eq!(chart.indicator_inputs(anchored), Some("Session hlc3 1"));
        // Another study's output has no name in the caption.
        let sma = chart.add_indicator(Box::new(SMA::new(7)));
        let rsi = chart.add_indicator(Box::new(RSI::new(9)));
        assert_eq!(chart.indicator_inputs(rsi), Some("9 close"));
        chart
            .set_indicator_source(rsi, StudySource::Study { id: sma.0, plot: 0 })
            .unwrap();
        assert_eq!(chart.indicator_inputs(rsi), Some("9"));
        assert_eq!(chart.indicator_inputs(sma), Some("7 close"));
    }

    /// Built-in studies read as before the chart derived their captions, and no caption shows a
    /// flag, an empty field, a list size or an open limit (#3570).
    #[test]
    fn derived_captions_of_built_in_studies() {
        let factory = crate::studies::IndicatorFactory::new();
        let mut chart = Chart::new(BarData::new());
        let mut caption = |name: &str| {
            let id = chart.add_indicator(factory.create(name).unwrap());
            chart.indicator_inputs(id).unwrap().to_owned()
        };
        for (name, expected) in [
            ("RSI", "14 close SMA 14"),
            ("VWAP", "Session 1"),
            ("Vol Profile Session", "24 70 Up / down Trading day"),
            ("Time Weighted Average Price", "Session"),
            ("Trend Strength Index", "14 close"),
            ("Higher Interval Candles", "1D"),
            ("Accelerator Oscillator", "hl2"),
            // Its legend names its moving average instead.
            ("Volume", ""),
        ] {
            assert_eq!(caption(name), expected, "{name}");
        }
        for name in factory.list() {
            let caption = caption(&name);
            assert_eq!(caption.trim(), caption, "{name}");
            assert!(
                !caption.contains("  ")
                    && !caption.contains("true")
                    && !caption.contains("false")
                    && !caption.contains("00000")
                    && !caption.split(' ').any(|item| item == "None"),
                "{name}: {caption}"
            );
        }
        let vwap = chart.add_indicator(factory.create("VWAP").unwrap());
        assert_eq!(chart.indicator_inputs(vwap), Some("Session 1"));
        chart
            .indicator_mut(vwap)
            .unwrap()
            .set_input(
                "source",
                InputValue::Source(StudySource::Price(PriceSource::Close)),
            )
            .unwrap();
        assert_eq!(chart.indicator_inputs(vwap), Some("Session close 1"));
    }
}
