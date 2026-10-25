//! Calculation order for studies that consume another study's output.

use super::{Chart, SeriesId};
use crate::model::Bar;
use crate::studies::{Indicator, InputValue, StudySource};
use web_time::Instant;

impl Chart {
    pub(super) fn recalculate_studies(&mut self) {
        let requests = self
            .studies
            .indicators()
            .iter()
            .flat_map(|study| study.data_requests())
            .collect();
        let sources = self.study_data.data(requests, &self.symbol, self.timeframe);
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
            if plot >= self.studies.indicators()[parent_at].line_cnt() {
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
                        ordered.contains(&parent) && plot < studies[parent].line_cnt()
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
    let due = order
        .iter()
        .copied()
        .filter(|&at| {
            let parent_ready = match studies[at].source() {
                Some(crate::studies::StudySource::Study { id, .. }) => ids
                    .iter()
                    .position(|candidate| candidate.0 == id)
                    .is_some_and(|parent| pending_from[parent].is_none()),
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
    for at in order {
        let parent = match studies[at].source() {
            Some(crate::studies::StudySource::Study { id, .. }) => {
                ids.iter().position(|candidate| candidate.0 == id)
            }
            _ => None,
        };
        if parent.is_some_and(|parent| pending_from[parent].is_some()) {
            continue;
        }
        let Some(from) = pending_from[at] else {
            continue;
        };
        let len = studies[at].values().len();
        let incremental =
            studies[at].is_incremental() && len > 0 && from + 1 >= len && bars.len() >= len;
        if !incremental && due != Some(at) {
            continue;
        }
        let started = Instant::now();
        super::studies::update(studies, ids, at, bars, from, sources);
        pending_from[at] = None;
        if !incremental {
            let cost = started.elapsed().as_secs_f64();
            next_refresh[at] = now + cost + REFRESH_INTERVAL.max(4.0 * cost);
        }
        for child in 0..studies.len() {
            if matches!(studies[child].source(), Some(crate::studies::StudySource::Study { id, .. }) if id == ids[at].0)
            {
                pending_from[child] = Some(pending_from[child].map_or(from, |old| old.min(from)));
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
    use crate::model::{Bar, BarData};
    use crate::studies::{InputValue, RSI, SMA};
    use crate::widget::{Chart, SeriesId, test_frames::shapes};
    use chrono::{Duration, Utc};

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
            ("Open", 4.0, 2.0),
            ("High", 11.0, 7.0),
            ("Low", 4.0, 2.0),
            ("Close", 6.0, 4.0),
            ("HL2", 7.5, 4.5),
            ("HLC3", 7.0, 13.0 / 3.0),
            ("OHLC4", 6.25, 3.75),
        ] {
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            let id = chart.add_indicator(Box::new(SMA::new(2)));
            let value =
                serde_json::from_str(&format!(r#"{{"Source":{{"Price":"{source}"}}}}"#)).unwrap();
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input("source", value)
                .unwrap();
            settle(&mut chart);
            assert_eq!(
                chart.indicators().next().unwrap().1.values().value(0, 2),
                Some(expected),
                "{source}"
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
                "{source}"
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
        let value: InputValue = serde_json::from_str(&format!(
            r#"{{"Source":{{"Study":{{"id":{},"plot":0}}}}}}"#,
            rsi.0
        ))
        .unwrap();
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
}
