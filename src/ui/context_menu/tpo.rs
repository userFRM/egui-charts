//! Merge, split and reset displayed TPO profiles through composite ranges and split instants.

use super::{ContextMenuAction, MenuEntry, MenuItem};
use crate::model::{BarClock, ChartType, ChartTypeParams};
use crate::widget::{Chart, SeriesId};
use chrono::{DateTime, Utc};
use egui::Pos2;
use std::ops::RangeInclusive;

#[derive(Debug)]
pub(super) struct TpoMenu {
    params: ChartTypeParams,
    clock: BarClock,
    symbol: String,
    /// Merged range and whether it needs a composite; joining two plain parts of one session
    /// only removes their split.
    previous: Option<(RangeInclusive<DateTime<Utc>>, bool)>,
    next: Option<(RangeInclusive<DateTime<Utc>>, bool)>,
    /// Merges and splits after dividing at a selected period after the profile's first.
    split: Option<(Vec<RangeInclusive<DateTime<Utc>>>, Vec<DateTime<Utc>>)>,
}

impl TpoMenu {
    pub(super) fn open(chart: &Chart, pos: Pos2) -> Option<Self> {
        if chart.chart_type() != ChartType::TimePriceOpportunity || !chart.is_main_series_visible()
        {
            return None;
        }
        let params = chart.chart_type_params().clone();
        let clock = chart.profile_clock();
        let bars = chart.bars(SeriesId::MAIN);
        let (start, end) = chart.state().visible_range();
        let tick = chart.instrument().and_then(|i| i.tick_size);
        let config = params.tpo_config(tick);
        let (open, period) = crate::chart::rendering::candles::tpo::profile_at(
            bars,
            start..end,
            chart.state().revision(),
            &config,
            tick,
            &clock,
            &chart.get_chart_mapping(),
            pos,
        )?;
        let composite = |time| params.tpo_composites.iter().position(|r| r.contains(&time));
        let sessions = clock.sessions();
        let opens: Vec<_> =
            crate::model::tpo::session_starts(bars, &sessions, config.split_hours(), &[])
                .into_iter()
                .map(|i| bars[i].time)
                .collect();
        let mut groups: Vec<RangeInclusive<DateTime<Utc>>> = Vec::new();
        for i in crate::model::tpo::session_starts(
            bars,
            &sessions,
            config.split_hours(),
            &params.tpo_splits,
        ) {
            let time = bars[i].time;
            let previous = bars[..i]
                .iter()
                .rposition(|bar| bar.high.is_finite() && bar.low.is_finite() && bar.high >= bar.low)
                .map(|j| bars[j].time);
            match groups.last_mut() {
                Some(last)
                    if (composite(*last.start()).is_some()
                        && composite(*last.start()) == composite(time))
                        || (opens.binary_search(&time).is_ok()
                            && !crate::model::tpo::split_between(
                                &params.tpo_splits,
                                previous,
                                time,
                            )
                            && config
                                .profile_period
                                .key(&sessions, *last.start())
                                .is_some()
                            && config.profile_period.key(&sessions, *last.start())
                                == config.profile_period.key(&sessions, time)) =>
                {
                    *last = *last.start()..=time;
                }
                _ => groups.push(time..=time),
            }
        }
        let at = groups.iter().position(|r| *r.start() == open)?;
        let range = |i: usize| {
            let group = &groups[i];
            composite(*group.start())
                .map_or_else(|| group.clone(), |i| params.tpo_composites[i].clone())
        };
        let merge = |first: usize, second: usize| {
            let composite_kept = composite(*groups[first].start()).is_some()
                || composite(*groups[second].start()).is_some()
                || opens.binary_search(groups[second].start()).is_ok();
            (*range(first).start()..=*range(second).end(), composite_kept)
        };
        let split = period.filter(|time| *time > open).map(|time| {
            let mut splits = params.tpo_splits.clone();
            if opens.binary_search(&time).is_err()
                || config.profile_period != crate::model::TPOProfilePeriod::Session
            {
                splits.push(time);
                splits.sort_unstable();
                splits.dedup();
            }
            let mut composites = params.tpo_composites.clone();
            if let Some(i) = composite(open) {
                let range = composites.remove(i);
                let previous = crate::model::tpo::session_starts(
                    bars,
                    &sessions,
                    config.split_hours(),
                    &splits,
                )
                .into_iter()
                .map(|j| bars[j].time)
                .rfind(|t| *t < time && range.contains(t))
                .unwrap();
                let mut parts = Vec::new();
                if *range.start() < previous {
                    parts.push(*range.start()..=previous);
                }
                if time < *range.end() {
                    parts.push(time..=*range.end());
                }
                composites.splice(i..i, parts);
            }
            (composites, splits)
        });
        Some(Self {
            previous: at.checked_sub(1).map(|i| merge(i, at)),
            next: (at + 1 < groups.len()).then(|| merge(at, at + 1)),
            split,
            params,
            clock,
            symbol: chart.symbol().to_owned(),
        })
    }

    pub(super) fn entries(&self) -> Vec<MenuEntry<ContextMenuAction>> {
        [
            (
                "Merge with previous profile",
                ContextMenuAction::MergeTpoPrevious,
                self.previous.is_some(),
            ),
            (
                "Merge with next profile",
                ContextMenuAction::MergeTpoNext,
                self.next.is_some(),
            ),
            (
                "Split profile at selected period",
                ContextMenuAction::SplitTpoProfile,
                self.split.is_some(),
            ),
            (
                "Reset merges and splits",
                ContextMenuAction::ResetProfileEdits,
                !self.params.tpo_composites.is_empty() || !self.params.tpo_splits.is_empty(),
            ),
        ]
        .into_iter()
        .map(|(label, action, enabled)| {
            let mut item = MenuItem::new(label, action);
            item.enabled = enabled;
            MenuEntry::Item(item)
        })
        .collect()
    }

    pub(super) fn apply(&self, chart: &mut Chart, action: &ContextMenuAction) {
        if chart.chart_type() != ChartType::TimePriceOpportunity
            || chart.chart_type_params() != &self.params
            || chart.symbol() != self.symbol
            || chart.profile_clock() != self.clock
        {
            return;
        }
        let mut params = self.params.clone();
        match action {
            ContextMenuAction::MergeTpoPrevious | ContextMenuAction::MergeTpoNext => {
                let merge = if *action == ContextMenuAction::MergeTpoPrevious {
                    &self.previous
                } else {
                    &self.next
                };
                let Some((range, composite)) = merge else {
                    return;
                };
                params
                    .tpo_splits
                    .retain(|time| !(range.start() < time && time <= range.end()));
                if *composite {
                    // Keep host endpoints beyond the loaded history when replacing composites.
                    params
                        .tpo_composites
                        .retain(|r| r.end() < range.start() || r.start() > range.end());
                    params.tpo_composites.push(range.clone());
                }
            }
            ContextMenuAction::SplitTpoProfile => {
                let Some((composites, splits)) = &self.split else {
                    return;
                };
                params.tpo_composites = composites.clone();
                params.tpo_splits = splits.clone();
            }
            ContextMenuAction::ResetProfileEdits => {
                params.tpo_composites.clear();
                params.tpo_splits.clear();
            }
            _ => return,
        }
        chart.apply_chart_type_params(params);
    }
}
