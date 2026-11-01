//! The per-frame request and answer engine of a feed-backed chart: what [`Chart::pump`] asks the
//! feed for, how the answers are applied and how failed requests are retried.

use super::{
    FeedLink, MIN_SCROLL_BACK_PAGE, Request, SCROLL_BACK_MARGIN, TRADE_CAPACITY, lock, resolve,
    retry_delay,
};
use crate::data::inbox::Answer;
use crate::data::merge::Origin;
use crate::data::trades::Trades;
use crate::data::{
    DepthHistory, DepthPage, DepthRequest, FeedCapabilities, FeedError, FeedState, HistoryPage,
    HistoryRequest, Subscription, TradePage, TradeRequest,
};
use crate::model::{BarClock, ChartType, Instrument, Tick, Timeframe};
use crate::widget::{Chart, SeriesId};
use std::sync::Arc;

impl Chart {
    /// Issues what the feed must be asked and applies what it sent since the last call. Called
    /// first in [`show`](Self::show); does nothing without a feed.
    pub(crate) fn pump(&mut self, ctx: &egui::Context) {
        // Session profiles are summed by the studies' code.
        let executions = (self.chart_type == ChartType::VolumeFootprint
            || (self.chart_type == ChartType::SessionVolumeProfile && cfg!(feature = "studies")))
            && self
                .timeframe
                .is_none_or(|timeframe| !timeframe.is_trade_based());
        if let Some(link) = self
            .feed_link
            .as_mut()
            .filter(|link| executions != link.footprint.is_some())
        {
            link.footprint = if executions { link.footprint() } else { None };
        }
        if self.feed_link.as_ref().is_some_and(|link| link.paused) {
            return;
        }
        self.pump_study_data(
            ctx,
            self.feed_link.as_ref().map(|link| Arc::clone(&link.feed)),
        );
        let clock = self.bar_clock();
        let Some(mut link) = self.feed_link.take() else {
            return;
        };
        if link.clock != clock {
            link.reload_pending = true;
        }
        if link.reload_pending {
            self.reload(&mut link, clock);
        }
        // The chart's own viewport, also after it moved to another: off the UI thread
        // `request_repaint` wakes the root one.
        if lock(&link.wake).replace(ctx.viewport_id()).is_none() {
            let (ctx, wake) = (ctx.clone(), Arc::clone(&link.wake));
            link.inbox.set_waker(move || {
                let viewport = *lock(&wake);
                if let Some(viewport) = viewport {
                    ctx.request_repaint_of(viewport);
                }
            });
        }

        let mut inputs: Vec<crate::data::OrderFlowInput> = Vec::new();
        for study in self.studies.indicators() {
            for input in study.order_flow_inputs() {
                if !inputs.iter().any(|old| input.same_source(old)) {
                    inputs.push(input.clone());
                }
            }
        }
        if inputs.len() != link.study_inputs.len()
            || inputs
                .iter()
                .zip(&link.study_inputs)
                .any(|(a, (b, _))| !a.same_source(b))
        {
            let previous = std::mem::take(&mut link.study_inputs);
            link.study_inputs = inputs
                .iter()
                .map(|input| {
                    let seen = previous
                        .iter()
                        .find(|(old, _)| input.same_source(old))
                        .map(|(_, seen)| *seen);
                    if let Some(seen) = seen {
                        (input.clone(), seen)
                    } else {
                        input.set_tick_size(link.instrument.as_ref().and_then(|i| i.tick_size));
                        input.replace_trades(&link.clock, &link.trades);
                        if let Some(now) = link.time {
                            input.advance_time(&link.clock, now);
                        }
                        (input.clone(), u64::MAX)
                    }
                })
                .collect();
            if let Some(id) = link.subscription {
                link.inbox.study_inputs(id, inputs);
            }
        }
        let now = ctx.input(|input| input.time);
        let drained = link.inbox.drain();
        for (key, answer) in drained.answers {
            let Some(at) = link
                .pending
                .iter()
                .position(|(pending, _, _)| *pending == key)
            else {
                continue;
            };
            let (_, request, failures) = link.pending.swap_remove(at);
            match request.clone() {
                Request::Capabilities => match result::<FeedCapabilities>(answer) {
                    Ok(capabilities) => link.capabilities = Some(capabilities),
                    // The default restricts nothing, as a feed that reports none does.
                    Err(error) if !error.is_recoverable() => {
                        link.capabilities = Some(FeedCapabilities::default());
                    }
                    Err(error) => self.request_failed(&mut link, request, failures, error, now),
                },
                Request::Resolve => match result::<Instrument>(answer) {
                    Ok(instrument) => self.subscribe(&mut link, instrument),
                    Err(error) => {
                        // History asked in the meantime is in the choice's currency or unit,
                        // which the failed instrument cannot label.
                        if self.conversion.is_some() && !error.is_recoverable() {
                            self.rule_out(&mut link);
                        }
                        self.request_failed(&mut link, request, failures, error, now);
                    }
                },
                Request::History {
                    request: asked,
                    scroll_back,
                } => match result::<HistoryPage>(answer) {
                    Ok(page) => self.apply_page(&mut link, &asked, page, scroll_back),
                    Err(error) => self.request_failed(&mut link, request, failures, error, now),
                },
                Request::Depth(request) => match result::<DepthPage>(answer) {
                    Ok(mut page) => {
                        page.columns
                            .retain(|c| request.before.is_none_or(|before| c.time < before));
                        page.columns.sort_by_key(|c| c.time);
                        if page.columns.len() > request.count {
                            page.columns.drain(..page.columns.len() - request.count);
                        }
                        let empty = page.columns.is_empty();
                        let continuation = page.continue_before.filter(|_| empty);
                        if let Some(error) = non_progressing(request.before, continuation) {
                            link.depth_complete = true;
                            self.feed.last_error = Some(error);
                        } else {
                            link.depth_cursor = continuation;
                            link.depth_complete |=
                                page.end_of_history || (empty && page.continue_before.is_none());
                            if let Some(history) = &mut link.depth_history {
                                if let Err(error) = history.merge_page(page.columns) {
                                    link.depth_complete = true;
                                    self.feed.last_error = Some(error);
                                }
                                // A page that reached no column before the boundary cannot move it.
                                link.depth_complete |= continuation.is_none()
                                    && request.before.is_some_and(|before| {
                                        history.columns().next().is_none_or(|c| c.time >= before)
                                    });
                            }
                        }
                    }
                    Err(error) => self.request_failed(
                        &mut link,
                        Request::Depth(request),
                        failures,
                        error,
                        now,
                    ),
                },
                Request::Trades(asked) => match result::<TradePage>(answer) {
                    Ok(page) => self.apply_trades(&mut link, &asked, page),
                    Err(error) if error.is_recoverable() => {
                        self.request_failed(&mut link, request, failures, error, now);
                    }
                    Err(error) => {
                        log::debug!("no trade history for {}: {error}", self.symbol);
                        link.trades_complete = true;
                    }
                },
            }
        }

        let mut resnapshot = false;
        for (id, updates) in drained.updates {
            if link.subscription != Some(id) {
                continue;
            }
            if updates.reset {
                link.time = None;
                self.bar_close_time = None;
                self.server_time_offset = None;
                self.replace_main_data(Vec::new());
                self.quote = None;
                self.session_quote = None;
                link.trades = Trades::new(TRADE_CAPACITY);
                link.trades_from_start = false;
                link.order_book = None;
                link.order_queue = None;
                link.depth_history.iter_mut().for_each(DepthHistory::clear);
                link.rebuild_bubbles();
            }
            if !updates.bars.is_empty() {
                let incoming = updates.bars.into_values().collect();
                let merged = self.apply_by(&link.clock, incoming);
                self.feed.late_ticks += merged.late_ticks;
                if merged.first_changed.is_some() {
                    self.data_updated(SeriesId::MAIN);
                }
            }
            if let Some(quote) = updates.quote
                && self.quote.as_ref().is_none_or(|old| quote.ts >= old.ts)
            {
                self.set_quote(Some(quote));
            }
            let feed = &mut self.feed;
            feed.status = updates.status.or(feed.status);
            feed.symbol_catalog_revision = feed
                .symbol_catalog_revision
                .wrapping_add(u64::from(updates.symbols_changed));
            feed.last_error = updates.error.or(feed.last_error.take());
            feed.late_ticks += updates.late_ticks;
            feed.repeated_ticks += updates.repeated_ticks;
            feed.rejected_bars += updates.rejected;
            feed.rejected_depth += updates.rejected_depth;
            let bubbles = &mut link.bubbles;
            let footprint = &mut link.footprint;
            let inputs = &link.study_inputs;
            let clock = &link.clock;
            let lost_before = link.trades.lost_until();
            let mut evicted = false;
            let fold = &mut |trade: &Tick, added| {
                bubbles.iter_mut().for_each(|b| b.fold(trade, added));
                evicted |= !added;
                if added {
                    if let Some(flow) = footprint {
                        let _ = flow.push(trade);
                    }
                    if clock.timeframe().is_none_or(|tf| !tf.is_trade_based()) {
                        for (input, _) in inputs {
                            input.push_trade(clock, trade);
                        }
                    }
                }
            };
            link.trades.append(updates.trades, fold);
            // Trades dropped for capacity, here or before they reached this store, leave the footprint.
            if let (Some(flow), Some(lost)) = (&mut link.footprint, link.trades.lost_until())
                && (evicted || Some(lost) != lost_before)
            {
                flow.drop_through(lost, link.trades.iter());
            }
            // Trades before the oldest held one may never have arrived.
            if let (Some(first), Some(flow)) = (link.unconfirmed_start(), &mut link.footprint) {
                flow.received_from(first);
            }
            link.time = link.time.max(updates.time);
            if let Some(now) = link.time {
                for (input, _) in &link.study_inputs {
                    input.advance_time(&link.clock, now);
                }
            }
            link.order_book = updates.depth.or(link.order_book.take());
            if updates.orders_changed {
                link.order_queue = updates.orders;
            }
            if let Some(history) = &mut link.depth_history {
                updates
                    .depth_columns
                    .into_iter()
                    .for_each(|c| history.push(c));
            }
            resnapshot |= updates.resnapshot;
        }
        if resnapshot && let Some(id) = link.subscription {
            link.feed.request_depth_snapshot(id);
        }

        let mut at = 0;
        while at < link.retries.len() {
            if link.retries[at].0 > now {
                at += 1;
                continue;
            }
            let (_, request, failures) = link.retries.swap_remove(at);
            let feed = Arc::clone(&link.feed);
            match request.clone() {
                Request::Capabilities => {
                    let reply = link.reply(request);
                    link.pending.last_mut().unwrap().2 = failures;
                    feed.capabilities(reply);
                }
                Request::Resolve => {
                    let reply = link.reply(request);
                    link.pending.last_mut().unwrap().2 = failures;
                    resolve(&*feed, &self.symbol, self.conversion.as_deref(), reply);
                }
                Request::History {
                    request: history, ..
                } => {
                    self.feed.loading_history = true;
                    let reply = link.reply(request);
                    link.pending.last_mut().unwrap().2 = failures;
                    feed.history(history, reply);
                }
                Request::Depth(depth) => {
                    let reply = link.reply(request);
                    link.pending.last_mut().unwrap().2 = failures;
                    feed.depth_history(depth, reply);
                }
                Request::Trades(trades) => {
                    let reply = link.reply(request);
                    link.pending.last_mut().unwrap().2 = failures;
                    feed.trades(trades, reply);
                }
            }
        }
        if let Some(deadline) = link.retries.iter().map(|r| r.0).min_by(f64::total_cmp) {
            ctx.request_repaint_after(
                std::time::Duration::try_from_secs_f64((deadline - now).max(0.0))
                    .unwrap_or(std::time::Duration::MAX),
            );
        }
        for (input, seen) in &mut link.study_inputs {
            let revision = input.revision();
            if revision != *seen {
                for (at, study) in self.studies.indicators().iter().enumerate() {
                    if study
                        .order_flow_inputs()
                        .iter()
                        .any(|other| input.same_source(other))
                    {
                        let from = input.changed_since(*seen).map_or(0, |time| {
                            self.state
                                .data()
                                .bars
                                .partition_point(|bar| link.clock.bar_open(bar.time) < time)
                        });
                        self.pending_from[at] =
                            Some(self.pending_from[at].map_or(from, |old| old.min(from)));
                    }
                }
                *seen = revision;
            }
        }
        self.scroll_back(&mut link);
        self.scroll_trades(&mut link);
        self.scroll_depth(&mut link);
        self.feed.trade_history_complete = link.trades_complete;
        self.feed.depth_history_complete = link.depth_complete || link.depth_history.is_none();
        self.feed_link = Some(link);
    }

    /// Drops what the chart held for the old symbol or timeframe and asks for the new one.
    fn reload(&mut self, link: &mut FeedLink, clock: BarClock) {
        link.reload_pending = false;
        if let Some(id) = link.subscription.take() {
            link.inbox.unsubscribe(id);
            link.feed.unsubscribe(id);
        }
        // The feed-wide answer does not depend on the symbol or interval.
        let feed_wide = |request: &Request| matches!(request, Request::Capabilities);
        link.pending.retain(|(_, request, _)| feed_wide(request));
        link.retries.retain(|(_, request, _)| feed_wide(request));
        self.legend_instrument = link
            .instrument
            .take()
            .filter(|instrument| instrument.symbol == self.symbol);
        self.state.price_scale_mut().set_price_precision(None);
        link.trades = Trades::new(TRADE_CAPACITY);
        for study in self.studies.indicators() {
            for input in study.order_flow_inputs() {
                input.clear();
            }
        }
        link.study_inputs.clear();
        link.time = None;
        link.order_book = None;
        link.order_queue = None;
        link.depth_history.iter_mut().for_each(DepthHistory::clear);
        link.depth_complete = false;
        (link.bar_cursor, link.trade_cursor, link.depth_cursor) = (None, None, None);
        link.trades_complete = false;
        link.trades_from_start = false;
        self.set_quote(None);
        link.paging_stopped = false;
        let timeframe = self.timeframe.unwrap_or_default();
        link.clock = clock;
        link.rebuild_bubbles();
        // Timestamp anchors survive the empty interval between history requests.
        self.replace_main_data(Vec::new());
        self.feed = FeedState::default();
        if link.capabilities.is_none() && link.pending.is_empty() && link.retries.is_empty() {
            let reply = link.reply(Request::Capabilities);
            link.feed.capabilities(reply);
        }
        if self.symbol.is_empty() {
            return;
        }
        let reply = link.reply(Request::Resolve);
        resolve(&*link.feed, &self.symbol, self.conversion.as_deref(), reply);
        if supports(self.legend_instrument.as_ref(), timeframe) {
            self.request_latest(link);
        } else {
            self.rule_out(link);
        }
    }

    /// Asks for the latest page of history.
    fn request_latest(&mut self, link: &mut FeedLink) {
        self.feed.loading_history = true;
        let request = HistoryRequest {
            symbol: self.symbol.clone(),
            timeframe: self.timeframe.unwrap_or_default(),
            clock: link.clock.clone(),
            before: None,
            since: None,
            count: link.initial_bars,
            extended_hours: self.extended_hours,
            conversion: self.conversion.clone(),
        };
        let reply = link.reply(Request::History {
            request: request.clone(),
            scroll_back: false,
        });
        link.feed.history(request, reply);
    }

    /// Shows no bars for an interval the instrument rules out, and drops any history requested
    /// for it before the instrument was known.
    fn rule_out(&mut self, link: &mut FeedLink) {
        let history = |request: &Request| matches!(request, Request::History { .. });
        link.pending.retain(|(_, request, _)| !history(request));
        link.retries.retain(|(_, request, _)| !history(request));
        self.replace_main_data(Vec::new());
        self.feed.loading_history = false;
        self.feed.last_error = Some(FeedError::NotSupported);
    }

    /// Subscribes to live updates once the symbol resolved, and asks for the recent trades.
    /// An interval the instrument rules out gets neither.
    fn subscribe(&mut self, link: &mut FeedLink, instrument: Instrument) {
        self.state
            .price_scale_mut()
            .set_price_precision(crate::scales::instrument_price_precision(&instrument));
        let timeframe = self.timeframe.unwrap_or_default();
        let withheld = !supports(self.legend_instrument.as_ref(), timeframe);
        link.instrument = Some(instrument);
        link.rebuild_bubbles();
        if !supports(link.instrument.as_ref(), timeframe) {
            self.rule_out(link);
            return;
        }
        // The previous answer for the symbol ruled the interval out; this one allows it.
        if withheld {
            self.request_latest(link);
        }
        let tick_size = link.instrument.as_ref().and_then(|i| i.tick_size);
        for (input, _) in &link.study_inputs {
            input.set_tick_size(tick_size);
        }
        let sink = link.inbox.subscribe(
            self.symbol.clone(),
            link.clock.clone(),
            TRADE_CAPACITY,
            tick_size,
        );
        let config = link.depth_history.as_ref().map(|h| h.config().clone());
        link.inbox.sample_depth(sink.id, config);
        link.inbox.study_inputs(
            sink.id,
            link.study_inputs
                .iter()
                .map(|(input, _)| input.clone())
                .collect(),
        );
        link.subscription = Some(sink.id);
        let subscription = Subscription {
            id: sink.id,
            symbol: self.symbol.clone(),
            timeframe,
            clock: link.clock.clone(),
            extended_hours: self.extended_hours,
            conversion: self.conversion.clone(),
            sink,
        };
        link.feed.subscribe(subscription);
        let request = TradeRequest {
            symbol: self.symbol.clone(),
            before: None,
            since: None,
            count: TRADE_CAPACITY,
            conversion: self.conversion.clone(),
        };
        let reply = link.reply(Request::Trades(request.clone()));
        link.feed.trades(request, reply);
    }

    /// Merges a page of history and ends scroll-back when the feed has nothing older. An empty
    /// page that names where older bars continue moves the boundary there instead.
    fn apply_page(
        &mut self,
        link: &mut FeedLink,
        asked: &HistoryRequest,
        mut page: HistoryPage,
        scroll_back: bool,
    ) {
        // The oldest bars were evicted since the page was asked for: merged, it would leave
        // the evicted bars as a gap. `scroll_back` asks again before the oldest bar held.
        if scroll_back && asked.before != self.scroll_back_before(link) {
            self.feed.loading_history = false;
            return;
        }
        let received = page.bars.len();
        page.bars.retain(|bar| {
            asked
                .before
                .is_none_or(|before| link.clock.bar_open(bar.time) < before)
        });
        if scroll_back && page.bars.len() < received {
            log::warn!(
                "the feed of {} sent bars at or after the requested before; those bars were dropped",
                self.symbol
            );
        }
        let oldest = self.bars(SeriesId::MAIN).first().map(|bar| bar.time);
        let empty = page.bars.is_empty();
        let continuation = page.continue_before.filter(|_| empty);
        if let Some(error) = non_progressing(asked.before, continuation) {
            link.paging_stopped = true;
            self.feed.last_error = Some(error);
        }
        link.bar_cursor = continuation
            .filter(|_| !link.paging_stopped)
            .map(|at| (at, oldest));
        self.feed.history_complete |= (page.end_of_history
            || (empty && page.continue_before.is_none()))
            && !self.bar_limit_reached();
        let incoming = page.bars.into_iter().map(|bar| (bar, Origin::History));
        self.apply_by(&link.clock, incoming.collect());
        let older = self.bars(SeriesId::MAIN).first().map(|bar| bar.time) < oldest;
        // A nonempty page with no valid bar older than the oldest held would be requested
        // again on every frame.
        let stale = scroll_back && !empty && !older && !self.bar_limit_reached();
        if stale {
            log::warn!(
                "the feed of {} sent no valid bar older than the oldest one held; scroll-back ended",
                self.symbol
            );
        }
        self.feed.loading_history = false;
        self.feed.history_complete |= stale;
        self.data_loaded(SeriesId::MAIN);
    }

    /// Retries transient failures, retaining the exact page boundary and independent attempts.
    fn request_failed(
        &mut self,
        link: &mut FeedLink,
        request: Request,
        failures: u32,
        error: FeedError,
        now: f64,
    ) {
        log::warn!("feed request for {} failed: {error}", self.symbol);
        if matches!(request, Request::History { .. }) {
            self.feed.loading_history = false;
        }
        if error.is_recoverable() {
            let delay = retry_delay(failures, &error);
            link.retries.push((
                now + delay.as_secs_f64(),
                request,
                failures.saturating_add(1),
            ));
        } else if matches!(request, Request::Depth(_)) {
            link.depth_complete = true;
            if error == FeedError::NotSupported {
                return;
            }
        } else {
            link.paging_stopped = true;
        }
        self.feed.last_error = Some(error);
    }

    /// Puts a page of older trades in front of the held ones. A page that ends history, names a
    /// continuation, or adds nothing new cannot move the boundary, so the same request is never
    /// repeated. A page that ends history also confirms that the held trades start where trade
    /// history starts.
    fn apply_trades(&mut self, link: &mut FeedLink, asked: &TradeRequest, page: TradePage) {
        let empty = page.trades.is_empty();
        let continuation = page.continue_before.filter(|_| empty);
        if let Some(error) = non_progressing(asked.before, continuation) {
            link.trades_complete = true;
            self.feed.last_error = Some(error);
            return;
        }
        let end = page.end_of_history || (empty && page.continue_before.is_none());
        // The oldest held trade's bar is complete once nothing is left before it.
        let reached_start = end && !link.trades_from_start;
        link.trades_from_start |= end;
        let bubbles = &mut link.bubbles;
        let mut changed = false;
        let fold = &mut |trade: &Tick, added| {
            changed |= added;
            bubbles.iter_mut().for_each(|b| b.fold(trade, added));
        };
        self.feed.rejected_bars += link.trades.prepend(page.trades, fold);
        if changed {
            for (input, _) in &link.study_inputs {
                input.replace_trades(&link.clock, &link.trades);
            }
        }
        if (changed || reached_start) && link.footprint.is_some() {
            link.footprint = link.footprint();
        }
        link.trade_cursor = continuation;
        link.trades_complete |=
            end || (continuation.is_none() && !changed) || link.trades.room() == 0;
    }

    /// The start of the range to ask for below `before`: the time `scroll_to_time` is heading
    /// for, while it lies before `before`.
    fn range_start(
        &self,
        before: chrono::DateTime<chrono::Utc>,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        self.pending_scroll_time.filter(|time| *time < before)
    }

    /// Asks for trades older than the oldest held once the view reaches it, for the order flow
    /// that consumes them: bubbles, a footprint or a study.
    fn scroll_trades(&mut self, link: &mut FeedLink) {
        let consumed =
            link.bubbles.is_some() || link.footprint.is_some() || !link.study_inputs.is_empty();
        let asking = |r: &Request| matches!(r, Request::Trades(_));
        if !consumed
            || link.trades_complete
            || link.subscription.is_none()
            || link.pending.iter().any(|(_, r, _)| asking(r))
            || link.retries.iter().any(|(_, r, _)| asking(r))
        {
            return;
        }
        let Some(before) = [link.trades.oldest(), link.trade_cursor]
            .into_iter()
            .flatten()
            .min()
        else {
            return;
        };
        if !self.older_bars_drawn
            || self
                .state
                .shown()
                .get(self.state.visible_range().0)
                .is_none_or(|bar| bar.time > before)
        {
            return;
        }
        let request = TradeRequest {
            symbol: self.symbol.clone(),
            before: Some(before),
            since: self.range_start(before),
            count: link.trades.room().min(TRADE_CAPACITY),
            conversion: self.conversion.clone(),
        };
        let reply = link.reply(Request::Trades(request.clone()));
        link.feed.trades(request, reply);
    }

    /// Depth pages have their own boundary and completion state, independent of bar history.
    fn scroll_depth(&mut self, link: &mut FeedLink) {
        if link.depth_complete
            || link.subscription.is_none()
            || link
                .pending
                .iter()
                .any(|(_, r, _)| matches!(r, Request::Depth(_)))
            || link
                .retries
                .iter()
                .any(|(_, r, _)| matches!(r, Request::Depth(_)))
        {
            return;
        }
        let Some(history) = &link.depth_history else {
            return;
        };
        let Some(tick) = link
            .instrument
            .as_ref()
            .and_then(|i| i.tick_size)
            .filter(|t| t.is_finite() && *t > 0.0)
        else {
            return;
        };
        let before = history
            .columns()
            .next()
            .map(|c| c.time)
            .into_iter()
            .chain(link.depth_cursor)
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
        let mut request = DepthRequest::new(
            self.symbol.clone(),
            history.config().columns,
            history.config().clone(),
            tick,
        );
        request.before = before;
        request.since = before.and_then(|before| self.range_start(before));
        request.conversion = self.conversion.clone();
        let reply = link.reply(Request::Depth(request.clone()));
        link.feed.depth_history(request, reply);
    }

    /// Whether the main series holds as many bars as its limit keeps after an eviction. Paging
    /// stops there, since older bars that took the series past the limit would be evicted first.
    fn bar_limit_reached(&self) -> bool {
        self.bar_limit(SeriesId::MAIN).is_some_and(|limit| {
            self.bars(SeriesId::MAIN).len() >= crate::widget::series::kept_bars(limit)
        })
    }

    /// Where the page before the oldest bar starts: that bar's slot, or the continuation an
    /// empty page named while that bar was the oldest. An eviction lapses the continuation.
    fn scroll_back_before(&self, link: &FeedLink) -> Option<chrono::DateTime<chrono::Utc>> {
        let first = self.bars(SeriesId::MAIN).first().map(|bar| bar.time);
        // Stored bars keep the feed's stamp, any instant inside the slot.
        let oldest = first.map(|time| link.clock.bar_open(time));
        let cursor = (link.bar_cursor)
            .filter(|(_, held)| *held == first)
            .map(|(at, _)| at);
        [oldest, cursor].into_iter().flatten().min()
    }

    /// Asks for the page before the oldest bar once the view nears it.
    fn scroll_back(&mut self, link: &mut FeedLink) {
        let feed = &self.feed;
        if feed.loading_history
            || feed.history_complete
            || link
                .retries
                .iter()
                .any(|(_, request, _)| matches!(request, Request::History { .. }))
            || link.paging_stopped
            || !self.older_bars_drawn
            || self.bar_limit_reached()
        {
            return;
        }
        let Some(before) = self.scroll_back_before(link) else {
            return;
        };
        if self.symbol.is_empty() || self.state.visible_range().0 >= SCROLL_BACK_MARGIN {
            return;
        }
        let request = HistoryRequest {
            symbol: self.symbol.clone(),
            timeframe: self.timeframe.unwrap_or_default(),
            clock: link.clock.clone(),
            before: Some(before),
            since: self.range_start(before),
            count: link.initial_bars.max(MIN_SCROLL_BACK_PAGE),
            extended_hours: self.extended_hours,
            conversion: self.conversion.clone(),
        };
        self.feed.loading_history = true;
        let reply = link.reply(Request::History {
            request: request.clone(),
            scroll_back: true,
        });
        link.feed.history(request, reply);
    }
}

/// The error for a continuation `at` that does not lie before the `before` it was asked for,
/// which would repeat the request.
fn non_progressing(
    before: Option<chrono::DateTime<chrono::Utc>>,
    at: Option<chrono::DateTime<chrono::Utc>>,
) -> Option<FeedError> {
    let (before, at) = (before?, at?);
    (at >= before).then(|| {
        FeedError::InvalidData(format!(
            "continuation {at} does not lie before the requested {before}"
        ))
    })
}

/// Whether `instrument` allows asking for bars of `timeframe`. An unknown instrument allows any.
fn supports(instrument: Option<&Instrument>, timeframe: Timeframe) -> bool {
    instrument
        .and_then(|instrument| instrument.supported_timeframes.as_ref())
        .is_none_or(|supported| supported.contains(&timeframe))
}

/// The result an answer carries; the key it came under names its type.
fn result<T: 'static>(answer: Answer) -> Result<T, FeedError> {
    match answer.downcast::<Result<T, FeedError>>() {
        Ok(result) => *result,
        Err(_) => Err(FeedError::Other("answer of another request".into())),
    }
}
