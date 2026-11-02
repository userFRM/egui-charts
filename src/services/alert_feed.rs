//! Historical and streaming inputs for headless alert evaluation.

use crate::data::FeedError;
#[cfg(feature = "ui")]
use crate::model::AlertSource;
use crate::model::{Bar, Timeframe};
use chrono::{DateTime, Utc};

/// Historical and streaming inputs for an alert without a rendered chart. Uses the same
/// validation, trade aggregation and history/live merge rules as a chart. Dropping it stops
/// its subscription and discards late replies. Available with `alert-engine`.
pub struct AlertFeed {
    feed: std::sync::Arc<dyn crate::data::Datafeed>,
    inbox: std::sync::Arc<crate::data::inbox::Inbox>,
    wake: std::sync::Arc<std::sync::atomic::AtomicBool>,
    request: crate::data::HistoryRequest,
    metric: Option<String>,
    instrument: Option<crate::model::Instrument>,
    calendar: Option<crate::model::AlertCalendar>,
    subscription: Option<crate::data::SubscriptionId>,
    bars: Vec<Bar>,
    origins: std::collections::BTreeMap<DateTime<Utc>, crate::data::merge::Origin>,
    pending: bool,
    ready: bool,
    retry: Option<DateTime<Utc>>,
    stopped: bool,
    #[cfg(feature = "ui")]
    drawings: Option<(
        std::sync::Arc<dyn super::Storage>,
        super::document::StoredDocument,
    )>,
    #[cfg(feature = "ui")]
    stored: Option<crate::drawings::persistence::DrawingStorage>,
    inputs: Vec<crate::data::OrderFlowInput>,
    end: bool,
}

impl AlertFeed {
    /// A latest-history request for 1,000 bars. Set its public calendar, session, conversion
    /// and count fields before passing it to [`new`](Self::new) to match the source chart.
    pub fn history_request(
        symbol: impl Into<String>,
        timeframe: Timeframe,
    ) -> crate::data::HistoryRequest {
        crate::data::HistoryRequest {
            symbol: symbol.into(),
            timeframe,
            clock: crate::model::BarClock::new(timeframe),
            before: None,
            since: None,
            count: 1000,
            extended_hours: false,
            conversion: None,
        }
    }

    /// Requests the symbol's metadata, then its history and updates. `metric` selects a study's
    /// metric series; `None` selects price bars. Calls return promptly as `Datafeed` requires.
    pub fn new(
        feed: std::sync::Arc<dyn crate::data::Datafeed>,
        mut request: crate::data::HistoryRequest,
        metric: Option<String>,
    ) -> Self {
        if metric.is_some() {
            request.clock = crate::model::BarClock::exact();
        }
        let inbox = std::sync::Arc::<crate::data::inbox::Inbox>::default();
        if let Some(conversion) = &request.conversion
            && metric.is_none()
        {
            feed.resolve_converted(&request.symbol, conversion, inbox.reply(1));
        } else {
            feed.resolve(&request.symbol, inbox.reply(1));
        }
        Self {
            feed,
            inbox,
            wake: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            request,
            metric,
            instrument: None,
            calendar: None,
            subscription: None,
            bars: Vec::new(),
            origins: Default::default(),
            pending: true,
            ready: false,
            retry: None,
            stopped: false,
            #[cfg(feature = "ui")]
            drawings: None,
            #[cfg(feature = "ui")]
            stored: None,
            inputs: Vec::new(),
            end: false,
        }
    }

    #[cfg(feature = "studies")]
    pub(super) fn set_calendar(&mut self, calendar: Option<crate::model::AlertCalendar>) {
        self.calendar = calendar;
    }

    /// Follows this symbol's drawings in host storage, without writing to it.
    #[cfg(feature = "ui")]
    pub fn set_storage(&mut self, storage: std::sync::Arc<dyn super::Storage>) {
        if self
            .drawings
            .as_ref()
            .is_some_and(|(current, _)| std::sync::Arc::ptr_eq(current, &storage))
        {
            return;
        }
        let mut document = super::document::StoredDocument::open(
            storage.as_ref(),
            super::StoreKind::Drawings,
            &self.request.symbol,
        );
        document.follow_changes(storage.as_ref());
        self.stored = None;
        self.drawings = Some((storage, document));
    }

    /// Bars available once history reaches the requested count or its end. Live updates stay
    /// buffered until then so evaluation never mistakes an incomplete stream for history.
    pub fn bars(&self) -> &[Bar] {
        if self.ready { &self.bars } else { &[] }
    }

    /// Extend coverage to `start`, even when it needs more than the initial bar count.
    pub fn require_start(&mut self, start: DateTime<Utc>) {
        self.request.since = Some(self.request.since.map_or(start, |held| held.min(start)));
        if !self.end && self.bars.first().is_none_or(|bar| bar.time > start) {
            self.ready = false;
        }
    }

    #[cfg(feature = "studies")]
    pub(super) fn set_inputs(&mut self, inputs: Vec<crate::data::OrderFlowInput>) {
        self.inputs = inputs;
        if let Some(id) = self.subscription {
            self.inbox.study_inputs(id, self.inputs.clone());
        }
    }

    #[cfg(feature = "studies")]
    pub(super) fn ready(&self) -> bool {
        self.ready
    }

    /// Calendar shared by history, streaming aggregation and study calculation.
    pub fn clock(&self) -> &crate::model::BarClock {
        &self.request.clock
    }

    /// Metadata resolved by the host.
    pub fn instrument(&self) -> Option<&crate::model::Instrument> {
        self.instrument.as_ref()
    }

    /// A stored drawing's level on the evaluated bar, using the chart's interpolation and
    /// price-scale geometry. `None` while drawings are loading or for an unavailable level.
    #[cfg(feature = "ui")]
    pub fn drawing_value(
        &self,
        source: &AlertSource,
        at: usize,
        scale: crate::scales::PriceScaleMode,
    ) -> Option<f64> {
        let bars = self.bars();
        bars.get(at)?;
        self.stored
            .as_ref()?
            .drawings()
            .iter()
            .enumerate()
            .find_map(|(id, stored)| {
                let drawing = crate::drawings::persistence::stored_to_drawing_with(
                    stored,
                    |point| {
                        point
                            .time()
                            .and_then(|time| {
                                crate::model::index_at_with_timeframe(
                                    bars,
                                    time,
                                    Some(self.request.timeframe),
                                    self.instrument
                                        .as_ref()
                                        .and_then(|instrument| instrument.time_zone.as_ref()),
                                )
                            })
                            .unwrap_or(f64::NAN) as f32
                    },
                    id,
                );
                let level = drawing.alert_level(source)?;
                drawing
                    .alert_levels(at as f64, scale)
                    .into_iter()
                    .find(|(index, ..)| *index == level)
                    .map(|(.., value)| value)
                    .filter(|value| value.is_finite())
            })
    }

    /// Applies replies and coalesced updates and wakes `ctx` when more arrive. Recoverable
    /// failures retry after the host's delay, or five seconds. Errors are returned for display.
    pub fn poll(
        &mut self,
        #[cfg(feature = "ui")] ctx: &egui::Context,
        now: DateTime<Utc>,
    ) -> Vec<FeedError> {
        use crate::data::merge::{Origin, merge};
        use crate::data::{
            HistoryPage, MetricPage, MetricRequest, MetricSubscription, Subscription,
        };
        #[cfg(feature = "ui")]
        {
            let repaint = ctx.clone();
            let viewport = ctx.viewport_id();
            let wake = std::sync::Arc::clone(&self.wake);
            self.inbox.set_waker(move || {
                if wake.load(std::sync::atomic::Ordering::Relaxed) {
                    repaint.request_repaint_of(viewport);
                }
            });
        }
        let mut errors = Vec::new();
        let drained = self.inbox.drain();
        for (key, answer) in drained.answers {
            self.pending = false;
            let result = if key == 1 {
                let result = *answer
                    .downcast::<Result<crate::model::Instrument, FeedError>>()
                    .expect("alert instrument");
                result.map(|mut instrument| {
                    if let Some(calendar) = &self.calendar {
                        if calendar.symbol == self.request.symbol {
                            instrument.time_zone = calendar.timezone.clone();
                            instrument.session = calendar.session.clone();
                        } else if calendar.session.is_none() {
                            instrument.session = None;
                        }
                    }
                    if let Some(session) = &instrument.session
                        && self.metric.is_none()
                    {
                        self.request.clock = self.request.clock.clone().with_session(
                            instrument
                                .time_zone
                                .clone()
                                .unwrap_or_else(crate::model::TimezoneId::utc),
                            session.clone(),
                            self.request.extended_hours,
                        );
                    }
                    self.instrument = Some(instrument);
                })
            } else {
                let result = if self.metric.is_some() {
                    (*answer
                        .downcast::<Result<MetricPage, FeedError>>()
                        .expect("alert metric history"))
                    .map(|page| {
                        (
                            page.points.into_iter().map(Bar::from).collect::<Vec<_>>(),
                            page.end_of_history,
                            None,
                        )
                    })
                } else {
                    (*answer
                        .downcast::<Result<HistoryPage, FeedError>>()
                        .expect("alert history"))
                    .map(|page| (page.bars, page.end_of_history, page.continue_before))
                };
                result.and_then(|(bars, end, continuation)| {
                    let before = bars
                        .iter()
                        .map(|bar| self.request.clock.bar_open(bar.time))
                        .min()
                        .or(continuation);
                    if !end
                        && before
                            .zip(self.request.before)
                            .is_some_and(|(next, previous)| next >= previous)
                    {
                        return Err(FeedError::InvalidData(
                            "Alert history did not advance".into(),
                        ));
                    }
                    let merged = merge(
                        &mut self.bars,
                        &mut self.origins,
                        &self.request.clock,
                        bars.into_iter().map(|bar| (bar, Origin::History)).collect(),
                    );
                    self.end = end || before.is_none();
                    self.ready = self.end
                        || self.request.since.map_or_else(
                            || self.bars.len() >= self.request.count,
                            |start| self.bars.first().is_some_and(|bar| bar.time <= start),
                        );
                    self.request.before = before;
                    if merged.rejected > 0 {
                        errors.push(FeedError::InvalidData("Invalid alert history bars".into()));
                    }
                    Ok(())
                })
            };
            if let Err(error) = result {
                self.failed(&error, now);
                errors.push(error);
            }
        }
        for (_, updates) in drained.updates {
            if updates.reset {
                for input in &self.inputs {
                    input.clear();
                }
                self.bars.clear();
                self.origins.clear();
                self.ready = false;
                self.request.before = None;
                self.end = false;
            }
            merge(
                &mut self.bars,
                &mut self.origins,
                &self.request.clock,
                updates.bars.into_values().collect(),
            );
            if let Some(error) = updates.error {
                // Discard pending replies and replace a failed subscription before evaluating again.
                self.unsubscribe();
                self.inbox = std::sync::Arc::default();
                self.wake.store(true, std::sync::atomic::Ordering::Relaxed);
                self.pending = false;
                self.ready = false;
                for input in &self.inputs {
                    input.clear();
                }
                self.bars.clear();
                self.origins.clear();
                self.request.before = None;
                self.end = false;
                self.failed(&error, now);
                errors.push(error);
            }
        }
        if !self.pending && !self.stopped && !self.ready && self.retry.is_none_or(|at| at <= now) {
            self.retry = None;
            self.pending = true;
            if self.instrument.is_none() {
                if let Some(conversion) = &self.request.conversion
                    && self.metric.is_none()
                {
                    self.feed.resolve_converted(
                        &self.request.symbol,
                        conversion,
                        self.inbox.reply(1),
                    );
                } else {
                    self.feed.resolve(&self.request.symbol, self.inbox.reply(1));
                }
            } else {
                if self.subscription.is_none() {
                    let clock = if self.metric.is_some() {
                        crate::model::BarClock::exact()
                    } else {
                        self.request.clock.clone()
                    };
                    let sink = self.inbox.subscribe(
                        self.request.symbol.clone(),
                        clock,
                        100_000,
                        self.instrument
                            .as_ref()
                            .and_then(|instrument| instrument.tick_size),
                    );
                    let id = sink.id;
                    for input in &self.inputs {
                        input.set_tick_size(
                            self.instrument
                                .as_ref()
                                .and_then(|instrument| instrument.tick_size),
                        );
                    }
                    self.inbox.study_inputs(id, self.inputs.clone());
                    self.subscription = Some(id);
                    if let Some(metric) = &self.metric {
                        self.feed.subscribe_metric(MetricSubscription {
                            id,
                            symbol: self.request.symbol.clone(),
                            metric: metric.clone(),
                            sink,
                        });
                    } else {
                        self.feed.subscribe(Subscription {
                            id,
                            symbol: self.request.symbol.clone(),
                            timeframe: self.request.timeframe,
                            clock: self.request.clock.clone(),
                            extended_hours: self.request.extended_hours,
                            conversion: self.request.conversion.clone(),
                            sink,
                        });
                    }
                }
                if let Some(metric) = &self.metric {
                    self.feed.metric_history(
                        MetricRequest {
                            symbol: self.request.symbol.clone(),
                            metric: metric.clone(),
                            before: self.request.before,
                            count: self.request.count,
                        },
                        self.inbox.reply(2),
                    );
                } else {
                    let mut request = self.request.clone();
                    request.since = None;
                    self.feed.history(request, self.inbox.reply(2));
                }
            }
        }
        #[cfg(feature = "ui")]
        if let Some((storage, document)) = &mut self.drawings {
            for received in document.poll(storage.as_ref(), ctx) {
                match received {
                    super::document::Received::Loaded(json) => {
                        let loaded = json
                            .as_deref()
                            .map(crate::drawings::persistence::DrawingStorage::from_json)
                            .transpose();
                        match loaded {
                            Ok(Some((stored, skipped)))
                                if stored.symbol == self.request.symbol && skipped.is_empty() =>
                            {
                                self.stored = Some(stored)
                            }
                            Ok(None) => {
                                self.stored =
                                    Some(crate::drawings::persistence::DrawingStorage::new(
                                        self.request.symbol.clone(),
                                    ))
                            }
                            _ => {
                                self.stored = None;
                                errors.push(FeedError::InvalidData(
                                    "Could not read alert drawings".into(),
                                ));
                            }
                        }
                    }
                    super::document::Received::Failed(error) => {
                        self.stored = None;
                        errors.push(FeedError::Other(error))
                    }
                    _ => {}
                }
            }
        }
        errors
    }

    fn failed(&mut self, error: &FeedError, now: DateTime<Utc>) {
        if error.is_recoverable() {
            let delay = match error {
                FeedError::RateLimited {
                    retry_after: Some(delay),
                } => chrono::TimeDelta::from_std(*delay).unwrap_or(chrono::TimeDelta::seconds(5)),
                _ => chrono::TimeDelta::seconds(5),
            };
            self.retry = now.checked_add_signed(delay);
        } else {
            self.stopped = true;
            self.unsubscribe();
        }
    }

    fn unsubscribe(&mut self) {
        // Unsubscribe may drop a pending reply while the caller holds the context's lock.
        self.wake.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(id) = self.subscription.take() {
            self.inbox.unsubscribe(id);
            self.feed.unsubscribe(id);
        }
    }
}

impl Drop for AlertFeed {
    fn drop(&mut self) {
        self.unsubscribe();
    }
}
