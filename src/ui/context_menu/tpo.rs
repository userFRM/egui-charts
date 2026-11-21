//! Merge, split and reset displayed TPO profiles through composite ranges and split instants.

use super::{ContextMenuAction, MenuEntry, MenuItem};
use crate::model::{BarClock, ChartType, ChartTypeParams, TPOConfig};
use crate::studies::TimePriceOpportunity;
use crate::widget::{Chart, SeriesId};
use chrono::{DateTime, Utc};
use egui::Pos2;
use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq)]
enum TpoOwner {
    /// Profile edits of the main TPO series.
    ChartType,
    /// Profile edits stored on a TPO study.
    Study(SeriesId),
}

#[derive(Debug)]
pub(super) struct TpoMenu {
    owner: TpoOwner,
    config: TPOConfig,
    params: ChartTypeParams,
    clock: BarClock,
    symbol: String,
    /// Merged range and whether it needs a composite; joining two plain parts of one session
    /// only removes their split.
    previous: Option<(RangeInclusive<DateTime<Utc>>, bool)>,
    next: Option<(RangeInclusive<DateTime<Utc>>, bool)>,
    /// Merges and splits after dividing at a selected period after the profile's first.
    #[allow(clippy::type_complexity)]
    split: Option<(Vec<RangeInclusive<DateTime<Utc>>>, Vec<DateTime<Utc>>)>,
}

impl TpoMenu {
    pub(super) fn open(chart: &Chart, pos: Pos2) -> Option<Self> {
        let params = chart.chart_type_params().clone();
        let bars = chart.bars(SeriesId::MAIN);
        let (start, end) = chart.state().visible_range();
        let tick = chart.instrument().and_then(|i| i.tick_size);
        let mapping = chart.get_chart_mapping();
        let hit = |config: &TPOConfig| {
            let (clock, config) = chart.tpo_division(config);
            let profile = crate::chart::rendering::candles::tpo::profile_at(
                bars,
                start..end,
                chart.state().revision(),
                &config,
                tick,
                &clock,
                &mapping,
                pos,
            )?;
            Some((clock, config, profile))
        };
        let (owner, (clock, config, (open, period))) =
            if chart.chart_type() == ChartType::TimePriceOpportunity {
                if !chart.is_main_series_visible() {
                    return None;
                }
                let config = params.tpo_config(tick);
                (TpoOwner::ChartType, hit(&config)?)
            } else {
                if chart.chart_type().is_time_independent()
                    || chart.chart_type() == ChartType::SessionVolumeProfile
                    || chart.indicators_hidden()
                {
                    return None;
                }
                chart
                    .indicators()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .find_map(|(id, study)| {
                        if !study.is_visible() || !chart.indicator_user_edit_enabled(id) {
                            return None;
                        }
                        let config = study.tpo()?;
                        Some((TpoOwner::Study(id), hit(config)?))
                    })?
            };
        if matches!(&config.session, crate::model::ProfileSession::Custom(custom)
            if custom.overlap == crate::model::SessionOverlap::InBoth && custom.sessions.len() > 1)
        {
            return Some(Self {
                owner,
                config,
                params,
                clock,
                symbol: chart.symbol().to_owned(),
                previous: None,
                next: None,
                split: None,
            });
        }
        let composite = |time| config.composites.iter().position(|r| r.contains(&time));
        let sessions = clock.sessions();
        // Use the renderer's parts, including hours excluded by the study's session choice.
        let starts = |config: &TPOConfig| {
            crate::model::tpo::profile_parts(bars, &sessions, config, clock.timeframe())
                .into_iter()
                .filter_map(|part| {
                    part.bars
                        .clone()
                        .find(|&i| {
                            bars[i].high.is_finite()
                                && bars[i].low.is_finite()
                                && bars[i].high >= bars[i].low
                        })
                        .map(|i| (i, part.opens, part.date))
                })
                .collect::<Vec<_>>()
        };
        let parts = starts(&config);
        let opens: Vec<_> = parts
            .iter()
            .filter(|(_, opens, _)| *opens)
            .map(|&(i, _, _)| bars[i].time)
            .collect();
        let mut groups: Vec<RangeInclusive<DateTime<Utc>>> = Vec::new();
        let mut group_period = None;
        for &(i, _, date) in &parts {
            let time = bars[i].time;
            let period = date.map_or_else(
                || config.profile_period.key(&sessions, time),
                |date| config.profile_period.date_key(date),
            );
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
                                &config.splits,
                                previous,
                                time,
                            )
                            && group_period.is_some()
                            && group_period == period) =>
                {
                    *last = *last.start()..=time;
                }
                _ => {
                    groups.push(time..=time);
                    group_period = period;
                }
            }
        }
        let at = groups.iter().position(|r| *r.start() == open)?;
        let range = |i: usize| {
            let group = &groups[i];
            composite(*group.start())
                .map_or_else(|| group.clone(), |i| config.composites[i].clone())
        };
        let merge = |first: usize, second: usize| {
            let composite_kept = composite(*groups[first].start()).is_some()
                || composite(*groups[second].start()).is_some()
                || opens.binary_search(groups[second].start()).is_ok();
            (*range(first).start()..=*range(second).end(), composite_kept)
        };
        let split = period.filter(|time| *time > open).map(|time| {
            let mut splits = config.splits.clone();
            if opens.binary_search(&time).is_err()
                || config.profile_period != crate::model::TPOProfilePeriod::Session
            {
                splits.push(time);
                splits.sort_unstable();
                splits.dedup();
            }
            let mut composites = config.composites.clone();
            if let Some(i) = composite(open) {
                let range = composites.remove(i);
                let split_config = TPOConfig {
                    splits: splits.clone(),
                    ..config.clone()
                };
                let previous = starts(&split_config)
                    .into_iter()
                    .map(|(j, _, _)| bars[j].time)
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
            owner,
            config,
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
                !self.config.composites.is_empty() || !self.config.splits.is_empty(),
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
        if chart.symbol() != self.symbol {
            return;
        }
        match self.owner {
            TpoOwner::ChartType
                if chart.chart_type() != ChartType::TimePriceOpportunity
                    || chart.chart_type_params() != &self.params
                    || chart.profile_clock() != self.clock =>
            {
                return;
            }
            TpoOwner::Study(id) => {
                let current = chart
                    .indicators()
                    .find(|(study_id, _)| *study_id == id)
                    .and_then(|(_, study)| study.tpo());
                if !chart.indicator_user_edit_enabled(id)
                    || current.is_none_or(|config| {
                        let (clock, config) = chart.tpo_division(config);
                        clock != self.clock || config != self.config
                    })
                {
                    return;
                }
            }
            _ => {}
        }
        let mut composites = self.config.composites.clone();
        let mut splits = self.config.splits.clone();
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
                splits.retain(|time| !(range.start() < time && time <= range.end()));
                if *composite {
                    // Keep host endpoints beyond the loaded history when replacing composites.
                    composites.retain(|r| r.end() < range.start() || r.start() > range.end());
                    composites.push(range.clone());
                }
            }
            ContextMenuAction::SplitTpoProfile => {
                let Some((new_composites, new_splits)) = &self.split else {
                    return;
                };
                composites = new_composites.clone();
                splits = new_splits.clone();
            }
            ContextMenuAction::ResetProfileEdits => {
                composites.clear();
                splits.clear();
            }
            _ => return,
        }
        match self.owner {
            TpoOwner::ChartType => {
                let mut params = self.params.clone();
                params.tpo_composites = composites;
                params.tpo_splits = splits;
                chart.apply_chart_type_params(params);
            }
            TpoOwner::Study(id) => {
                if let Some(study) = chart.indicator_mut(id) {
                    let settings = study.settings();
                    let _ = study.apply_settings(
                        &TimePriceOpportunity::profile_edit_inputs(&composites, &splits),
                        settings,
                    );
                }
            }
        }
    }
}
