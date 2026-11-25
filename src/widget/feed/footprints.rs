//! Prepared pages use the normal request lifecycle and retain their rows outside the tape.

use super::pump::non_progressing;
use super::{FeedLink, MIN_SCROLL_BACK_PAGE, Request, SCROLL_BACK_MARGIN};
use crate::data::{FeedError, FootprintPage, FootprintRequest, FootprintState, StudyDataRequest};
use crate::model::ChartType;
use crate::widget::Chart;
use chrono::{DateTime, TimeDelta, Utc};

impl Chart {
    /// Sizes the first prepared footprint page from the visible bars plus up to one viewport
    /// of prefetch, widened for footprint studies and profile anchors. By default it uses the
    /// initial candle count. Set before showing the chart; it survives feed replacement and
    /// does not cancel outstanding pages, shorten candle history or change trade demand.
    pub fn set_footprint_history_by_view(&mut self, enabled: bool) {
        self.footprint_history_by_view = enabled;
    }

    fn footprint_by_view(&self) -> bool {
        self.footprint_history_by_view && self.replay.is_none() && self.market_replay.is_none()
    }

    /// Inclusive bar opens needed by the chart and its footprint consumers.
    fn footprint_demand(&self, studies: bool) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let shown = self.state.shown();
        let (first, end) = self.state.visible_range();
        let viewport = self.chart_type == ChartType::VolumeFootprint || studies;
        let mut from = shown.get(first).filter(|_| viewport).map(|bar| bar.time);
        let mut through = end
            .checked_sub(1)
            .and_then(|i| shown.get(i))
            .filter(|_| viewport)
            .map(|b| b.time);
        if studies {
            let main = StudyDataRequest {
                symbol: String::new(),
                timeframe: None,
                metric: None,
            };
            let bars = &self.state.data().bars;
            for study in self.studies.indicators() {
                if study.order_flow_inputs().iter().any(|i| i.is_footprint()) {
                    let start = study
                        .data_start(&main)
                        .or_else(|| bars.first().map(|b| b.time));
                    from = from.into_iter().chain(start).min();
                }
            }
            through = bars.last().map(|b| b.time).or(through);
        }
        #[cfg(feature = "drawings")]
        if self.footprint_by_view()
            && !self.drawings.is_hidden()
            && !self.drawings.drawings.is_empty()
        {
            let interval = self.timeframe.unwrap_or_default().as_str();
            for span in self
                .drawings
                .unfiltered_drawings()
                .filter(|drawing| drawing.is_visible_on_timeframe(&interval))
                .filter_map(|drawing| drawing.execution_span(shown))
            {
                from = from.into_iter().chain(span.first().map(|b| b.time)).min();
                through = through.into_iter().chain(span.last().map(|b| b.time)).max();
            }
        }
        from.into_iter()
            .chain(self.pending_scroll_time)
            .min()
            .zip(through)
    }

    fn initial_footprint_count(&self, studies: bool, limit: usize) -> usize {
        // No time bounds yet: the source, not the last visible candle, owns the live join.
        // A restored view can be far behind it. Page that view by time once the join is known.
        let (first, end) = self.state.visible_range();
        let width = if first < end {
            end - first
        } else {
            self.desired_visible_bars
                .unwrap_or_else(|| self.get_visible_bars())
                .max(1)
        };
        let wanted = self
            .footprint_demand(studies)
            .map_or(width, |(from, through)| {
                let bars = &self.state.data().bars;
                let first = bars.partition_point(|bar| bar.time < from);
                bars.partition_point(|bar| bar.time <= through)
                    .saturating_sub(first)
                    .max(width)
            });
        wanted
            .saturating_add(width.min(SCROLL_BACK_MARGIN))
            .min(limit)
    }

    pub(super) fn scroll_footprints(&mut self, link: &mut FeedLink) {
        let studies = link
            .study_inputs
            .iter()
            .any(|(input, _)| input.is_footprint());
        let cutoff = link
            .footprint
            .as_ref()
            .and_then(|flow| flow.live.prepared_before());
        // A profile can keep reading the prepared rows after switching back to candles.
        // Don't start a new prepared page for an otherwise raw-only drawing.
        let reader = self.chart_type == ChartType::VolumeFootprint
            || studies
            || (self.footprint_by_view() && cutoff.is_some());
        if !reader
            || link.clock.timeframe().is_none_or(|tf| tf.is_trade_based())
            || link.footprints_stopped
            || link.subscription.is_none()
            || link
                .pending
                .iter()
                .any(|(_, r, _)| matches!(r, Request::Footprints(_)))
            || link
                .retries
                .iter()
                .any(|(_, r, _)| matches!(r, Request::Footprints(_)))
        {
            return;
        }
        let Some(tick_size) = link
            .instrument
            .as_ref()
            .and_then(|i| i.tick_size)
            .filter(|tick| tick.is_finite() && *tick > 0.0)
        else {
            return;
        };
        let (before, since) = if let Some(cutoff) = cutoff {
            let Some((from, through)) = self.footprint_demand(studies) else {
                return;
            };
            // The bar the cutoff joins holds the live executions.
            let boundary = link.clock.instant_open(cutoff);
            let before =
                (link.clock.next_open(through)).map_or(boundary, |next| next.min(boundary));
            let Some((from, before)) = link.footprint_gap(link.clock.bar_open(from), before) else {
                return;
            };
            if !studies
                && !self.older_bars_drawn
                && link
                    .footprint
                    .as_ref()
                    .and_then(|flow| flow.view().covered_from())
                    .is_some_and(|held| before <= held)
            {
                return;
            }
            (Some(before), Some(from))
        } else {
            if link.footprints_complete {
                return;
            }
            (None, None)
        };
        let request = FootprintRequest {
            symbol: self.symbol.clone(),
            timeframe: link.clock.timeframe().unwrap(),
            clock: link.clock.clone(),
            tick_size,
            ticks_per_row: 1,
            before,
            since,
            count: if before.is_some() {
                link.initial_bars.max(MIN_SCROLL_BACK_PAGE)
            } else if self.footprint_by_view() {
                self.initial_footprint_count(studies, link.initial_bars)
            } else {
                link.initial_bars
            },
            extended_hours: self.extended_hours,
            overnight_hours: self.overnight_hours,
            adjusted: link.adjusted,
            conversion: self.conversion.clone(),
        };
        let reply = link.reply(Request::Footprints(request.clone()));
        link.feed.footprints(request, reply);
    }

    pub(super) fn apply_footprints(
        &mut self,
        link: &mut FeedLink,
        request: &FootprintRequest,
        page: FootprintPage,
    ) {
        let result = self.merge_footprints(link, request, page);
        if let Err(error) = result {
            link.footprints_stopped = true;
            self.feed.last_error = Some(error);
        }
    }

    fn merge_footprints(
        &mut self,
        link: &mut FeedLink,
        request: &FootprintRequest,
        page: FootprintPage,
    ) -> Result<(), FeedError> {
        let invalid = |message: &str| FeedError::InvalidData(message.to_owned());
        let available = page.is_available();
        let partial = page.partial_slots().is_some();
        let supplied = available || partial;
        let empty = page.bars.is_empty();
        if !supplied && (!empty || page.continue_before.is_none() || page.end_of_history) {
            return Err(invalid(
                "unavailable footprint history must name a span and carry no bars or end-of-history claim",
            ));
        }
        if page.bars.len() > request.count {
            return Err(invalid("footprint page exceeds the requested bar count"));
        }
        if let Some(slots) = page.partial_slots()
            && (empty
                || slots.is_empty()
                || slots.len() > page.bars.len()
                || link.clock.bar_open(page.complete_before) != page.complete_before)
        {
            return Err(invalid(
                "partial footprint history requires supplied slots and an aligned cutoff",
            ));
        }
        // Marked slots must be sealed; an unmarked boundary bar can still seed live trades.
        let partial_end = (partial && request.before.is_none()).then(|| {
            request
                .clock
                .instant_open(page.complete_before)
                .min(page.complete_before)
        });
        // The bar of the last execution the page can hold: on a daily or longer clock with a
        // session a bar opening after the cutoff can hold executions before it.
        let last = (page
            .complete_before
            .checked_sub_signed(TimeDelta::nanoseconds(1)))
        .map(|last| request.clock.instant_open(last));
        if page.bars.iter().any(|bar| {
            request.before.is_some_and(|before| bar.time >= before)
                || request.since.is_some_and(|since| bar.time < since)
                || last.is_none_or(|last| bar.time > last)
        }) || partial_end.is_some_and(|end| {
            page.partial_slots()
                .is_some_and(|slots| slots.iter().any(|&slot| slot >= end))
        }) {
            return Err(invalid(
                "footprint page lies outside its requested range or execution cutoff",
            ));
        }
        let continuation = page.continue_before.filter(|_| empty);
        if let Some(error) = non_progressing(request.before, continuation) {
            return Err(error);
        }
        let from = page
            .bars
            .iter()
            .map(|bar| bar.time)
            .min()
            .or(continuation)
            .or(request.since.filter(|_| empty));
        let Some(from) = from else {
            // A legacy empty terminal answer has no explicit coverage interval.
            link.footprints_complete = true;
            return Ok(());
        };
        let before = page.complete_before;
        // Bars were checked against the cutoff above: a trading date's bar can open after it.
        if (empty && from >= before)
            || request.since.is_some_and(|since| from < since)
            || request.before.is_some_and(|end| before != end)
            || link.clock.bar_open(from) != from
        {
            return Err(invalid(
                "footprint coverage must answer a nonempty suffix of the requested range",
            ));
        }
        let prepared = link
            .footprint
            .as_ref()
            .and_then(|flow| flow.live.prepared_before());
        if prepared.is_some_and(|cutoff| {
            link.clock.bar_open(before) != before || before > link.clock.instant_open(cutoff)
        }) {
            return Err(invalid(
                "internal footprint fills must be aligned and precede the live boundary",
            ));
        }
        let retain = link
            .footprint
            .as_ref()
            .is_some_and(|flow| flow.owned().is_none())
            || link
                .study_inputs
                .iter()
                .any(|(input, _)| input.is_footprint());
        let mut incoming = FootprintState::from_page(request, page.bars, retain)?;
        if let Some(slots) = page.partial_slots {
            incoming.mark_partial(slots)?;
        }
        if prepared.is_some() {
            let held = link.footprint.as_mut().unwrap();
            if supplied {
                held.insert_prepared(incoming, from, before)?;
            }
            held.record_prepared_span(from, before, supplied);
        } else {
            incoming.prepare_history(before, &link.trades, link.trades_from_start);
            incoming.record_prepared_span(from, before, supplied);
            incoming.live.rebuild_prepared(&link.trades);
            link.footprint = Some(incoming);
        }
        // Requests name bars, and the initial page's cutoff lies in the bar the trades continue.
        let end = prepared.map_or_else(|| link.clock.instant_open(before), |_| before);
        if !supplied && from < end {
            link.suppress_footprint_span(from, end);
        }
        let held = link.footprint.as_ref().unwrap();
        for (input, _) in &link.study_inputs {
            if input.is_footprint() {
                input.apply_prepared_span(
                    &link.clock,
                    &link.trades,
                    &self.state.data().bars,
                    held,
                    from..before,
                    supplied,
                )?;
            }
        }
        link.footprints_complete |=
            page.end_of_history || (available && empty && continuation.is_none());
        Ok(())
    }
}

impl FeedLink {
    /// Skip only context-local unavailable intervals, without walking retained bars.
    fn footprint_gap(
        &self,
        from: DateTime<Utc>,
        mut before: DateTime<Utc>,
    ) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let flow = self.footprint.as_ref()?;
        while from < before {
            let (start, end) = flow.history_gap(from, before)?;
            if self.footprints_complete
                && flow.view().covered_from().is_some_and(|held| end <= held)
            {
                return None;
            }
            match self
                .footprint_unavailable
                .iter()
                .rev()
                .find(|(left, right)| *left < end && *right > start)
            {
                Some(&(_, right)) if right < end => return Some((right.max(start), end)),
                Some(&(left, _)) => before = left,
                None => return Some((start, end)),
            }
        }
        None
    }

    fn suppress_footprint_span(&mut self, mut from: DateTime<Utc>, mut before: DateTime<Utc>) {
        let first = self
            .footprint_unavailable
            .partition_point(|(_, end)| *end < from);
        let mut last = first;
        while let Some(&(start, end)) = self.footprint_unavailable.get(last) {
            if start > before {
                break;
            }
            from = from.min(start);
            before = before.max(end);
            last += 1;
        }
        self.footprint_unavailable
            .splice(first..last, [(from, before)]);
    }
}
