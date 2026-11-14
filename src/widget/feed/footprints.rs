//! Prepared pages use the normal request lifecycle and retain their rows outside the tape.

use super::pump::non_progressing;
use super::{FeedLink, MIN_SCROLL_BACK_PAGE, Request};
use crate::data::{FeedError, Footprint, FootprintPage, FootprintRequest};
use crate::model::ChartType;
use crate::widget::Chart;

impl Chart {
    pub(super) fn scroll_footprints(&mut self, link: &mut FeedLink) {
        if self.chart_type != ChartType::VolumeFootprint
            || link.clock.timeframe().is_none_or(|tf| tf.is_trade_based())
            || link.footprints_complete
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
        let before = link
            .footprint
            .as_ref()
            .filter(|flow| flow.prepared_before().is_some())
            .and_then(|flow| flow.bars().first_key_value().map(|(time, _)| *time))
            .into_iter()
            .chain(link.footprint_cursor)
            .min();
        if let Some(before) = before
            && (!self.older_bars_drawn
                || self
                    .state
                    .shown()
                    .get(self.state.visible_range().0)
                    .is_none_or(|bar| bar.time > before))
        {
            return;
        }
        let request = FootprintRequest {
            symbol: self.symbol.clone(),
            timeframe: link.clock.timeframe().unwrap(),
            clock: link.clock.clone(),
            tick_size,
            ticks_per_row: 1,
            before,
            since: before
                .and_then(|before| self.pending_scroll_time.filter(|since| *since < before)),
            count: if before.is_some() {
                link.initial_bars.max(MIN_SCROLL_BACK_PAGE)
            } else {
                link.initial_bars
            },
            extended_hours: self.extended_hours,
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
            link.footprints_complete = true;
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
        if page.bars.len() > request.count {
            return Err(FeedError::InvalidData(format!(
                "footprint page has {} bars, exceeding the requested {}",
                page.bars.len(),
                request.count
            )));
        }
        if page.bars.iter().any(|bar| {
            request.before.is_some_and(|before| bar.time >= before)
                || request.since.is_some_and(|since| bar.time < since)
                || bar.time >= page.complete_before
        }) {
            return Err(invalid(
                "footprint page lies outside its requested range or execution cutoff",
            ));
        }
        let empty = page.bars.is_empty();
        let continuation = page.continue_before.filter(|_| empty);
        if let Some(error) = non_progressing(request.before, continuation) {
            return Err(error);
        }
        if (!empty || continuation.is_some())
            && request
                .before
                .is_some_and(|before| page.complete_before != before)
        {
            return Err(invalid(
                "footprint page must cover through the requested boundary",
            ));
        }
        let mut flow = Footprint::from_bars(
            request.clock.clone(),
            request.tick_size,
            request.ticks_per_row,
            page.bars,
        )?;
        if !empty {
            if let Some(held) = link
                .footprint
                .as_mut()
                .filter(|flow| flow.prepared_before().is_some())
            {
                held.prepend_prepared(flow);
            } else {
                flow.prepare_history(page.complete_before, &link.trades, link.trades_from_start);
                flow.rebuild_prepared(&link.trades);
                link.footprint = Some(flow);
            }
        }
        link.footprint_cursor = continuation;
        link.footprints_complete |= page.end_of_history || (empty && continuation.is_none());
        Ok(())
    }
}
