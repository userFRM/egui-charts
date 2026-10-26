//! Merge and split the displayed TPO chart profiles through their existing composite ranges.

use super::context_menu::{ContextMenuAction, MenuEntry, MenuItem};
use crate::model::{BarClock, ChartType, ChartTypeParams};
use crate::widget::{Chart, SeriesId};
use chrono::{DateTime, Utc};
use egui::Pos2;
use std::ops::RangeInclusive;

pub(super) struct TpoMenu {
    params: ChartTypeParams,
    clock: BarClock,
    symbol: String,
    previous: Option<RangeInclusive<DateTime<Utc>>>,
    next: Option<RangeInclusive<DateTime<Utc>>>,
    split: Option<RangeInclusive<DateTime<Utc>>>,
}

impl TpoMenu {
    pub(super) fn open(chart: &Chart, pos: Pos2) -> Option<Self> {
        if chart.chart_type() != ChartType::TimePriceOpportunity || !chart.is_main_series_visible()
        {
            return None;
        }
        let params = chart.chart_type_params().clone();
        let clock = profile_clock(chart);
        let bars = chart.bars(SeriesId::MAIN);
        let (start, end) = chart.state().visible_range();
        let config = params.tpo_config(chart.instrument().and_then(|i| i.tick_size));
        let open = crate::chart::rendering::candles::tpo::profile_at(
            bars,
            start..end,
            chart.state().revision(),
            &config,
            &clock,
            &chart.get_chart_mapping(),
            pos,
        )?;
        let composite = |time| params.tpo_composites.iter().position(|r| r.contains(&time));
        let mut groups: Vec<RangeInclusive<DateTime<Utc>>> = Vec::new();
        for i in
            crate::model::tpo::session_starts(bars, &clock.sessions(), params.tpo_split_sessions)
        {
            let time = bars[i].time;
            match groups.last_mut() {
                Some(last)
                    if composite(*last.start()).is_some()
                        && composite(*last.start()) == composite(time) =>
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
        let current = range(at);
        Some(Self {
            previous: at
                .checked_sub(1)
                .map(|i| *range(i).start()..=*current.end()),
            next: (at + 1 < groups.len()).then(|| *current.start()..=*range(at + 1).end()),
            split: (groups[at].start() != groups[at].end()).then_some(current),
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
                "Split profile",
                ContextMenuAction::SplitTpoProfile,
                self.split.is_some(),
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
            || profile_clock(chart) != self.clock
        {
            return;
        }
        let (range, split) = match action {
            ContextMenuAction::MergeTpoPrevious => (&self.previous, false),
            ContextMenuAction::MergeTpoNext => (&self.next, false),
            ContextMenuAction::SplitTpoProfile => (&self.split, true),
            _ => return,
        };
        let Some(range) = range else { return };
        let mut params = self.params.clone();
        if split {
            params.tpo_composites.retain(|r| r != range);
        } else {
            // Replace both composites, retaining any host endpoints beyond the loaded history.
            params
                .tpo_composites
                .retain(|r| r.end() < range.start() || r.start() > range.end());
            params.tpo_composites.push(range.clone());
        }
        chart.set_chart_type_params(params);
    }
}

fn profile_clock(chart: &Chart) -> BarClock {
    let mut clock = chart.bar_clock();
    if let Some(zone) = chart
        .instrument()
        .filter(|i| i.session.is_none())
        .and_then(|i| i.time_zone.as_ref())
    {
        clock = clock.with_timezone(zone.tz());
    }
    clock
}
