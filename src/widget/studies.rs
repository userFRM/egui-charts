//! Calculation order for studies that consume another study's output.

use super::{Chart, ChartElementId, SeriesId};
use crate::model::Bar;
use crate::studies::{Indicator, InputValue, StudySource};
use web_time::Instant;

impl Chart {
    /// Caps the studies at `limit`, or lifts the cap with `None`. The cap applies to
    /// [`try_add_indicator`](Self::try_add_indicator) and to pasting a copied study, which the
    /// terminal's dialogs and shortcuts use, and to applying a study template set; studies
    /// already on the chart stay.
    pub fn set_study_limit(&mut self, limit: Option<usize>) {
        self.study_limit = limit;
    }

    /// The cap set with [`set_study_limit`](Self::set_study_limit); `None` when uncapped.
    pub fn study_limit(&self) -> Option<usize> {
        self.study_limit
    }

    /// Whether the chart holds as many studies as its cap allows.
    pub(crate) fn study_limit_reached(&self) -> bool {
        self.study_limit
            .is_some_and(|limit| self.study_ids.len() >= limit)
    }

    /// Adds a study like [`add_indicator`](Self::add_indicator), unless the chart already holds
    /// as many as [`study_limit`](Self::study_limit) allows. A refused study leaves the chart
    /// unchanged.
    pub fn try_add_indicator(
        &mut self,
        indicator: Box<dyn Indicator>,
    ) -> Result<SeriesId, &'static str> {
        if self.study_limit_reached() {
            return Err(Self::STUDY_LIMIT_REACHED);
        }
        Ok(self.add_indicator(indicator))
    }

    pub(crate) const STUDY_LIMIT_REACHED: &'static str = "Study limit reached";

    /// Adds a study, computed at once from the bars, which [`show`](Self::show) draws: on the
    /// main pane if [`Indicator::is_overlay`], otherwise in a new pane at the bottom. Returns the
    /// study's id, which stays valid when other studies are removed. Host code adds past the
    /// cap; the interface goes through [`try_add_indicator`](Self::try_add_indicator).
    pub fn add_indicator(&mut self, mut indicator: Box<dyn Indicator>) -> SeriesId {
        if !indicator.has_settings() {
            indicator = Box::new(crate::studies::ConfiguredStudy::new(indicator));
        }
        if let Some(timeframe) = self.timeframe() {
            indicator.set_timeframe(timeframe);
        }
        if indicator.value_format() == crate::scales::StudyFormat::Volume {
            indicator.set_value_formatter(self.volume_formatter.clone());
        }
        indicator.set_instrument(self.instrument());
        indicator.set_time_scale_events(held_events(
            &self.time_scale_events,
            self.frozen_source_events,
        ));
        indicator.set_bar_clock(&self.bar_clock());
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
        let id = SeriesId(self.next_series);
        self.next_series += 1;
        self.record_study(id);
        let overlay = indicator.is_overlay();
        self.studies.add(indicator);
        self.study_ids.push(id);
        self.study_autoscale.push(true);
        self.study_inputs.push(String::new());
        self.pending_from.push(None);
        self.next_refresh.push(f64::NEG_INFINITY);
        if !overlay {
            self.add_study_pane(id);
        }
        id
    }

    /// Set the input caption displayed beside a study in the legend.
    pub fn set_indicator_inputs(&mut self, id: SeriesId, inputs: impl Into<String>) -> bool {
        let Some(index) = self.study_ids.iter().position(|candidate| *candidate == id) else {
            return false;
        };
        self.record_study(id);
        self.study_inputs[index] = inputs.into();
        true
    }

    /// The host-supplied input caption of a study, if its id is still present.
    pub fn indicator_inputs(&self, id: SeriesId) -> Option<&str> {
        self.study_ids
            .iter()
            .position(|candidate| *candidate == id)
            .map(|index| self.study_inputs[index].as_str())
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
        self.study_ids.remove(at);
        self.study_autoscale.remove(at);
        self.study_inputs.remove(at);
        self.pending_from.remove(at);
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
        self.studies.indicators_mut()[at].set_visible(visible);
        true
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
    /// has no values until it is recomputed. On price-based types these remain the source-bar
    /// results; [`Self::study_values_at`] returns the displayed element results.
    pub fn indicators(&self) -> impl Iterator<Item = (SeriesId, &dyn Indicator)> {
        let studies = self.studies.indicators().iter().map(|study| study.as_ref());
        self.study_ids.iter().copied().zip(studies)
    }

    /// The study `id`, to change its settings. Its values reflect the bars as of the last
    /// [`show`](Self::show); the next one recomputes it in full.
    pub fn indicator_mut(&mut self, id: SeriesId) -> Option<&mut (dyn Indicator + 'static)> {
        let at = self.study_ids.iter().position(|study| *study == id)?;
        self.record_study(id);
        self.pending_from[at] = Some(0);
        self.next_refresh[at] = f64::NEG_INFINITY;
        Some(self.studies.indicators_mut()[at].as_mut())
    }

    /// Updates every study with a pending change, as of `now` in seconds. An incremental study
    /// whose change starts at its last bar or later is updated at once. Any other change
    /// recomputes the study in full, for at most one study per call: of the studies whose change
    /// starts at the first bar or whose `next_refresh` has passed, the one whose `next_refresh`
    /// is earliest. Its next full recompute then waits, from the end of this one,
    /// the longer of a quarter second and four times what this one took. Until its turn, a
    /// study whose change starts at the first bar has no values, since its values belong to
    /// other bars. Returns how long until the next waiting study is due, zero if one is due now.
    pub(super) fn refresh_studies(&mut self, now: f64) -> Option<f64> {
        let instrument = self.instrument().cloned();
        let clock = self.bar_clock();
        let bars = &self.state.data().bars;
        let events = held_events(&self.time_scale_events, self.frozen_source_events);
        for (at, study) in self.studies.indicators_mut().iter_mut().enumerate() {
            let instrument_changed = study.set_instrument(instrument.as_ref());
            let clock_changed = study.set_bar_clock(&clock);
            let events_changed = study.set_time_scale_events(events);
            if instrument_changed || clock_changed || events_changed {
                self.pending_from[at] = Some(0);
                self.element_studies.dirty = true;
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
    pub fn set_indicator_source(
        &mut self,
        id: SeriesId,
        source: StudySource,
    ) -> Result<(), &'static str> {
        let at = self
            .study_ids
            .iter()
            .position(|candidate| *candidate == id)
            .ok_or("unknown study")?;
        if let StudySource::Study { id: parent, plot } = source {
            let parent_at = self
                .study_ids
                .iter()
                .position(|candidate| candidate.0 == parent)
                .ok_or("unknown source study")?;
            if plot >= self.studies.indicators()[parent_at].line_count() {
                return Err("unknown source plot");
            }
            let mut next = Some(parent);
            for _ in 0..=self.study_ids.len() {
                let Some(parent) = next else {
                    break;
                };
                if parent == id.0 {
                    return Err("cyclic study source");
                }
                let index = self
                    .study_ids
                    .iter()
                    .position(|candidate| candidate.0 == parent)
                    .ok_or("unknown source study")?;
                next = match self.studies.indicators()[index].source() {
                    Some(StudySource::Study { id, .. }) => Some(id),
                    _ => None,
                };
            }
            if next.is_some() {
                return Err("cyclic study source");
            }
        }
        let mut edited = self.studies.indicators()[at].clone_box();
        edited.set_input("source", InputValue::Source(source))?;
        self.record_study(id);
        self.studies.indicators_mut()[at] = edited;
        self.pending_from[at] = Some(0);
        self.next_refresh[at] = f64::NEG_INFINITY;
        self.element_studies.dirty = true;
        Ok(())
    }
}

/// The typed events studies read, without the mark source page an export froze at their end: a page
/// covers only the visible range, so restarts on it would move while scrolling.
fn held_events(
    events: &[crate::model::TimeScaleEvent],
    frozen: usize,
) -> &[crate::model::TimeScaleEvent] {
    &events[..events.len().saturating_sub(frozen)]
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
    next_refresh: &mut [f64],
    now: f64,
    sources: &[crate::studies::StudyData<'_>],
) -> Option<f64> {
    let order = super::studies::order(studies, ids);
    for at in 0..studies.len() {
        if !order.contains(&at) {
            studies[at].update_source(bars, 0, &[]);
            pending_from[at] = None;
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
        // Retained parent values can feed a waiting consumer while another tail update is pending.
        // Replaced histories and edited sources must finish their full invalidation first.
        if parent.is_some_and(|parent| pending_from[parent] == Some(0)) {
            continue;
        }
        let Some(from) = pending_from[at] else {
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
                    parent_ready
                        && pending_from[at].is_some_and(|from| {
                            let len = studies[at].values().len();
                            !(studies[at].is_incremental()
                                && len > 0
                                && from + 1 >= len
                                && bars.len() >= len)
                                && (from == 0 || now >= next_refresh[at])
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
        super::studies::update(studies, ids, at, bars, from, sources);
        pending_from[at] = None;
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
                    pending_from[child] =
                        Some(pending_from[child].map_or(from, |old| old.min(from)));
                }
            }
        }
    }
    let mut wait = None::<f64>;
    for (at, study) in studies.iter_mut().enumerate() {
        let Some(from) = pending_from[at] else {
            continue;
        };
        let left = if from == 0 {
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

/// Least time in seconds between two full recomputes of a study while bars stream; a study
/// whose recompute took longer than a quarter of it waits four times that instead.
const REFRESH_INTERVAL: f64 = 0.25;

#[cfg(all(test, feature = "studies"))]
mod tests {
    use crate::model::{Bar, BarData, PriceSource};
    use crate::studies::Indicator;
    use crate::studies::{InputValue, RSI, SMA, StudySource};
    use crate::widget::test_frames::frame;
    use crate::widget::{Chart, SeriesId, test_frames::shapes};
    use chrono::{DateTime, Duration, TimeDelta, Utc};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

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
        chart.update(
            SeriesId::MAIN,
            Bar::new(bars[3].time, 0.5, 0.5, 0.5, 0.5, 1.0),
        );
        settle(&mut chart);
        assert_eq!(output(&chart, 3), Some(0.0));
        chart
            .indicator_mut(rsi)
            .unwrap()
            .set_input("period", InputValue::Length(2))
            .unwrap();
        settle(&mut chart);
        // Wilder averages after +1, -1, -0.5 are gain=0.25, loss=0.5: RSI=100/3.
        assert!((output(&chart, 3).unwrap() - (50.0 + 100.0 / 3.0) / 2.0).abs() < 1e-8);
        assert!(
            chart
                .set_indicator_source(
                    rsi,
                    crate::studies::StudySource::Study {
                        id: mean.0,
                        plot: 0
                    }
                )
                .is_err()
        );
        let csv = chart.export_csv().unwrap();
        assert!(csv.contains(",41.67"), "{csv}");
        chart.user_action(|chart| chart.remove_series(rsi));
        settle(&mut chart);
        assert_eq!(output(&chart, 3), None);
        assert!(chart.undo());
        settle(&mut chart);
        assert!((output(&chart, 3).unwrap() - 125.0 / 3.0).abs() < 1e-8);
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
    /// running; waiting studies have no stale values and request another frame until the last
    /// study is current.
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
        // The second round replaces the bars while the waits of the first are still running.
        for (round, close) in [(0, 200.0), (1, 300.0)] {
            if round == 1 {
                // Model unequal measured costs without depending on the runner clock.
                chart.next_refresh.copy_from_slice(&[3.0, 2.0, 1.0]);
            }
            for counter in &calls {
                counter.store(0, Ordering::Relaxed);
            }
            chart.set_data(
                SeriesId::MAIN,
                (0..100)
                    .map(|i| bar(i * 60, close + (i * 7 % 13) as f64))
                    .collect(),
            );
            for k in 1..=3 {
                let now = if round == 0 { k as f64 } else { 3.0 } / 60.0;
                let wait = chart.refresh_studies(now);
                let counts: Vec<_> = calls
                    .iter()
                    .map(|counter| counter.load(Ordering::Relaxed))
                    .collect();
                assert_eq!(counts.iter().sum::<usize>(), k, "round {round}, call {k}");
                for (j, (_, study)) in chart.indicators().enumerate() {
                    assert!(counts[j] <= 1, "round {round}, call {k}, study {j}");
                    assert_eq!(
                        study.values().is_empty(),
                        counts[j] == 0,
                        "round {round}, call {k}, study {j}"
                    );
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
        assert!(chart.try_add_indicator(Box::new(SMA::new(7))).is_err());
        assert!(
            chart
                .paste_indicator(&copied, &crate::studies::IndicatorFactory::new())
                .is_err()
        );
        assert_eq!(chart.indicators().count(), 2);
        assert_eq!(chart.can_undo(), undoable);
        chart.add_indicator(Box::new(SMA::new(7)));
        assert_eq!(chart.indicators().count(), 3);
        chart.set_study_limit(None);
        assert!(chart.try_add_indicator(Box::new(SMA::new(8))).is_ok());
    }
}
