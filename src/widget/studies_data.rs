//! Host-fed histories and subscriptions used by studies.

use super::Chart;
use crate::data::inbox::Inbox;
use crate::data::merge::{Origin, merge};
use crate::data::{
    Datafeed, FeedError, HistoryPage, HistoryRequest, MetricPage, MetricRequest,
    MetricSubscription, Subscription, SubscriptionId,
};
use crate::model::{Bar, BarClock, Instrument, Timeframe};
use crate::studies::{StudyData, StudyDataRequest};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(super) struct StudyDataCache {
    series: Vec<Series>,
}

struct Series {
    request: StudyDataRequest,
    bars: Vec<Bar>,
    instrument: Option<Instrument>,
    clock: BarClock,
    error: Option<FeedError>,
    stream: Option<Stream>,
    from_ticks: BTreeMap<chrono::DateTime<chrono::Utc>, Origin>,
}

struct Stream {
    feed: Arc<dyn Datafeed>,
    inbox: Arc<Inbox>,
    subscription: Option<SubscriptionId>,
    pending: bool,
    next_key: u64,
    /// Kept intact until success, including while newer live data arrives.
    history: Option<HistoryRequest>,
    /// The chart's currency or unit conversion, which `clear_feed` replaces the stream on.
    conversion: Option<String>,
    /// Where older bars continue after an empty page.
    cursor: Option<chrono::DateTime<chrono::Utc>>,
    retry_at: Option<f64>,
    failures: u32,
    end: bool,
    wake: Arc<Mutex<Option<egui::ViewportId>>>,
}

impl Stream {
    fn request(&mut self, source: &StudyDataRequest) {
        self.next_key += 1;
        self.pending = true;
        if let Some(history) = &self.history {
            if let Some(metric) = &source.metric {
                self.feed.metric_history(
                    MetricRequest {
                        symbol: history.symbol.clone(),
                        metric: metric.clone(),
                        before: history.before,
                        count: history.count,
                    },
                    self.inbox.reply(self.next_key),
                );
            } else {
                self.feed
                    .history(history.clone(), self.inbox.reply(self.next_key));
            }
        } else {
            super::feed::resolve(
                &*self.feed,
                &source.symbol,
                self.conversion.as_deref(),
                self.inbox.reply(self.next_key),
            );
        }
    }

    fn failed(&mut self, error: &FeedError, now: f64) {
        if error.is_recoverable() {
            let delay = super::feed::retry_delay(self.failures, error);
            self.retry_at = Some(now + delay.as_secs_f64());
            self.failures = self.failures.saturating_add(1);
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        *self.wake.lock().unwrap_or_else(|error| error.into_inner()) = None;
        if let Some(id) = self.subscription {
            self.inbox.unsubscribe(id);
            self.feed.unsubscribe(id);
        }
    }
}

fn resolved(
    request: &StudyDataRequest,
    symbol: &str,
    timeframe: Option<Timeframe>,
) -> StudyDataRequest {
    StudyDataRequest {
        metric: request.metric.clone(),
        symbol: if request.symbol.is_empty() {
            symbol.to_owned()
        } else {
            request.symbol.clone()
        },
        timeframe: Some(request.timeframe.or(timeframe).unwrap_or_default()),
    }
}

fn clock(request: &StudyDataRequest, instrument: Option<&Instrument>, extended: bool) -> BarClock {
    if request.metric.is_some() {
        return BarClock::new(Timeframe::Ticks(std::num::NonZeroU64::MIN));
    }
    let timeframe = request.timeframe.unwrap_or_default();
    let mut clock = BarClock::new(timeframe);
    if let Some(instrument) = instrument {
        let zone = instrument
            .time_zone
            .clone()
            .unwrap_or_else(crate::model::TimezoneId::utc);
        if timeframe.duration_ms() < Timeframe::Day1.duration_ms() {
            clock = clock.with_timezone(zone.clone());
        }
        if let Some(session) = &instrument.session {
            clock = clock.with_session(zone, session.clone(), extended);
        }
    }
    clock
}

impl StudyDataCache {
    pub(super) fn snapshot(&self) -> Self {
        Self {
            series: self
                .series
                .iter()
                .map(|series| Series {
                    request: series.request.clone(),
                    bars: series.bars.clone(),
                    instrument: series.instrument.clone(),
                    clock: series.clock.clone(),
                    error: series.error.clone(),
                    stream: None,
                    from_ticks: series.from_ticks.clone(),
                })
                .collect(),
        }
    }
    pub(super) fn clear_feed(&mut self) {
        self.series.retain(|series| series.stream.is_none());
    }
    pub(super) fn pause(&mut self) {
        for series in &mut self.series {
            if let Some(stream) = &mut series.stream {
                stream.pending = false;
                stream.retry_at = None;
                stream.history = None;
                *stream
                    .wake
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = None;
                if let Some(id) = stream.subscription.take() {
                    stream.inbox.unsubscribe(id);
                    stream.feed.unsubscribe(id);
                }
            }
        }
    }

    pub(super) fn data(
        &self,
        requests: Vec<StudyDataRequest>,
        symbol: &str,
        timeframe: Option<Timeframe>,
        replay: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Vec<StudyData<'_>> {
        requests
            .into_iter()
            .filter_map(|request| {
                let key = resolved(&request, symbol, timeframe);
                let series = self.series.iter().find(|series| series.request == key)?;
                Some(StudyData {
                    request,
                    bars: match replay {
                        Some(time) => super::replay::completed(&series.bars, &series.clock, time),
                        None => &series.bars,
                    },
                    clock: series.clock.clone(),
                    instrument: series.instrument.as_ref(),
                    timeframe: key.timeframe.unwrap(),
                })
            })
            .collect()
    }
}

impl Chart {
    /// Histories required by feed-backed series, study inputs and profile drawings, with chart
    /// defaults resolved. Hosts without a connected feed supply these through [`Self::set_study_data`]. A connected
    /// feed receives resolve, history and subscription calls automatically on the next frame.
    pub fn study_data_requests(&self) -> Vec<StudyDataRequest> {
        let mut requests = self.fed_requests();
        for (_, study) in self.indicators() {
            for request in study.data_requests() {
                let request = resolved(&request, self.symbol(), self.timeframe());
                if !requests.contains(&request) {
                    requests.push(request);
                }
            }
        }
        #[cfg(feature = "drawings")]
        for (request, _) in self.profile_histories(&self.drawings) {
            let request = resolved(&request, self.symbol(), self.timeframe());
            if !requests.contains(&request) {
                requests.push(request);
            }
        }
        requests
    }

    /// Replace a study history with host-owned historical or streaming bars. Bars are sorted,
    /// deduplicated and validated like chart data. This takes ownership of this request from
    /// the connected feed; the host supplies further replacements. Metadata sets its calendar.
    /// The studies that read the history are recomputed on it. Bars that change only the last of
    /// the previous ones or add newer ones, as a stream does, leave their values drawn until
    /// their next full recompute; other bars clear them until it, which is then due at once.
    pub fn set_study_data(
        &mut self,
        request: StudyDataRequest,
        bars: Vec<Bar>,
        instrument: Option<Instrument>,
    ) {
        let request = resolved(&request, self.symbol(), self.timeframe());
        let clock = clock(&request, instrument.as_ref(), self.extended_hours());
        let (bars, _) = super::series::clean(bars, &clock);
        // Bars that differ from the previous ones at most from the last of those on, as when the
        // host streams a history, leave the earlier values right.
        let live = self.study_data.series.iter().any(|old| {
            old.request == request
                && old.stream.is_none()
                && old.instrument == instrument
                && old.clock == clock
                && old.bars.split_last().is_some_and(|(last, earlier)| {
                    bars.get(earlier.len())
                        .is_some_and(|bar| bar.time == last.time)
                        && bars[..earlier.len()] == *earlier
                })
        });
        let mut changes = Changes::default();
        changes.note(&request, !live);
        self.study_data
            .series
            .retain(|series| series.request != request);
        self.study_data.series.push(Series {
            request,
            bars,
            instrument,
            clock,
            error: None,
            stream: None,
            from_ticks: BTreeMap::new(),
        });
        self.restart_study_data(&changes);
    }

    /// The last error for a requested study history. Recoverable resolve and page failures retry
    /// with exponential backoff, respecting rate limits; success clears the error. An unavailable
    /// source leaves gaps.
    pub fn study_data_error(&self, request: &StudyDataRequest) -> Option<&FeedError> {
        let request = resolved(request, self.symbol(), self.timeframe());
        self.study_data
            .series
            .iter()
            .find(|series| series.request == request)?
            .error
            .as_ref()
    }

    pub(super) fn invalidate_study_data(&mut self) {
        for (at, study) in self.studies.indicators().iter().enumerate() {
            if !study.data_requests().is_empty() {
                self.pending_from[at] = Some(0);
                self.next_refresh[at] = f64::NEG_INFINITY;
            }
        }
        self.element_studies.dirty = true;
        self.fill_fed_series();
    }

    /// [`Self::study_data_requests`], each with how far back the connected feed loads it: to
    /// the earliest start among its readers, a study stating one through
    /// `Indicator::data_start` and any other counting as the chart's first bar. `None` while
    /// the chart has no bars. Allocates only the requests, since it runs every frame.
    fn study_data_reaches(&self) -> Vec<(StudyDataRequest, Option<chrono::DateTime<chrono::Utc>>)> {
        // Price-based chart types run their studies on elements; the source studies asked
        // here are not given the view, so their histories load in full.
        let full = self.chart_type.is_time_independent();
        let first = self.state.data().bars.first().map(|bar| bar.time);
        let mut reaches: Vec<(StudyDataRequest, Option<_>)> = self
            .fed_requests()
            .into_iter()
            .map(|request| (request, first))
            .collect();
        for (_, study) in self.indicators() {
            for declared in study.data_requests() {
                let start = study.data_start(&declared).filter(|_| !full);
                let start = first.and(start.or(first));
                let request = resolved(&declared, self.symbol(), self.timeframe());
                match reaches.iter_mut().find(|(known, _)| *known == request) {
                    Some((_, reach)) => *reach = (*reach).min(start),
                    None => reaches.push((request, start)),
                }
            }
        }
        #[cfg(feature = "drawings")]
        for (declared, start) in self.profile_histories(&self.drawings) {
            let start = first.and(Some(start));
            let request = resolved(&declared, self.symbol(), self.timeframe());
            match reaches.iter_mut().find(|(known, _)| *known == request) {
                Some((_, reach)) => *reach = (*reach).min(start),
                None => reaches.push((request, start)),
            }
        }
        reaches
    }

    pub(super) fn pump_study_data(&mut self, ctx: &egui::Context, feed: Option<Arc<dyn Datafeed>>) {
        let requests = self.study_data_reaches();
        // Each changed history, and whether its bars changed before its last one.
        let mut changes = Changes::default();
        self.study_data.series.retain(|series| {
            let kept = requests
                .iter()
                .any(|(request, _)| *request == series.request)
                && series.stream.as_ref().is_none_or(|stream| {
                    feed.as_ref()
                        .is_some_and(|feed| Arc::ptr_eq(feed, &stream.feed))
                });
            if !kept {
                changes.note(&series.request, true);
            }
            kept
        });
        if let Some(feed) = &feed {
            for (request, _) in &requests {
                if request.symbol.is_empty() {
                    continue;
                }
                if self
                    .study_data
                    .series
                    .iter()
                    .any(|series| &series.request == request)
                {
                    continue;
                }
                let inbox = Arc::new(Inbox::default());
                let wake = Arc::new(Mutex::new(Some(ctx.viewport_id())));
                let repaint = Arc::clone(&wake);
                let ctx = ctx.clone();
                inbox.set_waker(move || {
                    let viewport = *repaint.lock().unwrap_or_else(|error| error.into_inner());
                    if let Some(viewport) = viewport {
                        ctx.request_repaint_of(viewport);
                    }
                });
                super::feed::resolve(
                    &**feed,
                    &request.symbol,
                    self.conversion.as_deref(),
                    inbox.reply(0),
                );
                changes.note(request, true);
                self.study_data.series.push(Series {
                    request: request.clone(),
                    bars: Vec::new(),
                    instrument: None,
                    clock: clock(request, None, self.extended_hours()),
                    error: None,
                    from_ticks: BTreeMap::new(),
                    stream: Some(Stream {
                        feed: Arc::clone(feed),
                        inbox,
                        subscription: None,
                        pending: true,
                        next_key: 0,
                        history: None,
                        conversion: self.conversion.clone(),
                        cursor: None,
                        retry_at: None,
                        failures: 0,
                        end: false,
                        wake,
                    }),
                });
            }
        }
        let extended = self.extended_hours();
        let adjusted = self.dividend_adjusted();
        let now = ctx.input(|input| input.time);
        let count = self.state.data().bars.len().max(300);
        for series in &mut self.study_data.series {
            let Some(stream) = &mut series.stream else {
                let clock = clock(&series.request, series.instrument.as_ref(), extended);
                if clock != series.clock {
                    series.clock = clock;
                    changes.note(&series.request, true);
                }
                continue;
            };
            *stream
                .wake
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(ctx.viewport_id());
            let drained = stream.inbox.drain();
            for (key, answer) in drained.answers {
                if !stream.pending || key != stream.next_key {
                    continue;
                }
                stream.pending = false;
                if stream.history.is_none() {
                    let result = answer
                        .downcast::<Result<Instrument, FeedError>>()
                        .map(|result| *result)
                        .unwrap_or(Err(FeedError::NotSupported));
                    match result {
                        Ok(instrument) => {
                            let tf = series.request.timeframe.unwrap();
                            if series.request.metric.is_none()
                                && instrument
                                    .supported_timeframes
                                    .as_ref()
                                    .is_some_and(|intervals| !intervals.contains(&tf))
                            {
                                series.error = Some(FeedError::NotSupported);
                                continue;
                            }
                            series.clock = clock(&series.request, Some(&instrument), extended);
                            series.instrument = Some(instrument);
                            series.error = None;
                            stream.failures = 0;
                        }
                        Err(error) => {
                            stream.failed(&error, now);
                            series.error = Some(error);
                        }
                    }
                } else {
                    let result = if series.request.metric.is_some() {
                        answer
                            .downcast::<Result<MetricPage, FeedError>>()
                            .map(|result| {
                                result.map(|page| {
                                    HistoryPage::new(
                                        page.points.into_iter().map(Bar::from).collect(),
                                        page.end_of_history,
                                    )
                                })
                            })
                            .unwrap_or(Err(FeedError::NotSupported))
                    } else {
                        answer
                            .downcast::<Result<HistoryPage, FeedError>>()
                            .map(|result| *result)
                            .unwrap_or(Err(FeedError::NotSupported))
                    };
                    match result {
                        Ok(mut page) => {
                            let previous = stream.history.take().and_then(|request| request.before);
                            page.bars.retain(|bar| {
                                previous
                                    .is_none_or(|before| series.clock.bar_open(bar.time) < before)
                            });
                            series.error = None;
                            stream.failures = 0;
                            let empty = page.bars.is_empty();
                            let continuation = page.continue_before.filter(|_| empty);
                            stream.cursor = continuation;
                            stream.end =
                                page.end_of_history || (empty && page.continue_before.is_none());
                            if continuation.is_some_and(|at| previous.is_some_and(|b| at >= b)) {
                                stream.cursor = None;
                                stream.end = true;
                                series.error = Some(FeedError::InvalidData(
                                    "history continuation does not lie before the request".into(),
                                ));
                            }
                            let incoming = page
                                .bars
                                .into_iter()
                                .map(|bar| (bar, Origin::History))
                                .collect();
                            merge(
                                &mut series.bars,
                                &mut series.from_ticks,
                                &series.clock,
                                incoming,
                            );
                            if previous.is_some()
                                && continuation.is_none()
                                && series.bars.first().map(|bar| bar.time) >= previous
                            {
                                stream.end = true;
                            }
                            changes.note(&series.request, true);
                        }
                        Err(error) => {
                            stream.failed(&error, now);
                            series.error = Some(error);
                        }
                    }
                }
            }
            for (_, updates) in drained.updates {
                if updates.reset {
                    series.bars.clear();
                    series.from_ticks.clear();
                    series.error = None;
                    stream.pending = false;
                    stream.history = None;
                    stream.retry_at = None;
                    stream.failures = 0;
                    stream.end = false;
                    changes.note(&series.request, true);
                }
                if !updates.bars.is_empty() {
                    let len = series.bars.len();
                    let merged = merge(
                        &mut series.bars,
                        &mut series.from_ticks,
                        &series.clock,
                        updates.bars.into_values().collect(),
                    );
                    // Live bars that change at most the last one leave the earlier values right;
                    // into an empty history they are its first bars.
                    if let Some(first) = merged.first_changed {
                        changes.note(&series.request, len == 0 || first + 1 < len);
                    }
                }
                if updates.error.is_some() {
                    series.error = updates.error;
                }
            }
            if stream.subscription.is_none()
                && series.instrument.is_some()
                && series.error.is_none()
            {
                let tf = series.request.timeframe.unwrap();
                let sink = stream.inbox.subscribe(
                    series.request.symbol.clone(),
                    series.clock.clone(),
                    super::feed::TRADE_CAPACITY,
                    series.instrument.as_ref().and_then(|i| i.tick_size),
                );
                stream.subscription = Some(sink.id);
                if let Some(metric) = &series.request.metric {
                    stream.feed.subscribe_metric(MetricSubscription {
                        id: sink.id,
                        symbol: series.request.symbol.clone(),
                        metric: metric.clone(),
                        sink,
                    });
                } else {
                    stream.feed.subscribe(Subscription {
                        id: sink.id,
                        symbol: series.request.symbol.clone(),
                        timeframe: tf,
                        clock: series.clock.clone(),
                        extended_hours: extended,
                        conversion: stream.conversion.clone(),
                        sink,
                    });
                }
            }
            if let Some(deadline) = stream.retry_at {
                if now >= deadline {
                    stream.retry_at = None;
                    stream.request(&series.request);
                } else {
                    ctx.request_repaint_after(
                        std::time::Duration::try_from_secs_f64(deadline - now)
                            .unwrap_or(std::time::Duration::MAX),
                    );
                }
            }
            let reach = requests
                .iter()
                .find(|(request, _)| *request == series.request)
                .and_then(|(_, reach)| *reach);
            if !stream.pending
                && stream.retry_at.is_none()
                && !stream.end
                && series.error.is_none()
                && series.instrument.is_some()
                && (series.bars.is_empty()
                    || reach
                        .zip(series.bars.first())
                        .is_some_and(|(reach, bar)| reach < bar.time))
            {
                stream.history = Some(HistoryRequest {
                    symbol: series.request.symbol.clone(),
                    timeframe: series.request.timeframe.unwrap(),
                    clock: series.clock.clone(),
                    before: series
                        .bars
                        .first()
                        .map(|bar| bar.time)
                        .into_iter()
                        .chain(stream.cursor)
                        .min(),
                    since: None,
                    count,
                    extended_hours: extended,
                    adjusted,
                    conversion: stream.conversion.clone(),
                });
                stream.request(&series.request);
            }
        }
        if !changes.0.is_empty() {
            self.restart_study_data(&changes);
        }
    }

    /// Restarts the studies that read a changed history: at once and without their values after
    /// a change before its last bar, as for a new history, and otherwise keeping their values until
    /// their next full recompute. Studies that read only other histories stay as they are.
    fn restart_study_data(&mut self, changes: &Changes) {
        for (at, study) in self.studies.indicators().iter().enumerate() {
            let mut read = None::<bool>;
            for request in study.data_requests() {
                let request = resolved(&request, &self.symbol, self.timeframe);
                if let Some((_, hard)) = changes.0.iter().find(|(changed, _)| *changed == request) {
                    read = Some(read.unwrap_or(false) || *hard);
                }
            }
            match read {
                Some(true) => {
                    self.pending_from[at] = Some(0);
                    self.next_refresh[at] = f64::NEG_INFINITY;
                    self.element_studies.dirty = true;
                }
                Some(false) => {
                    self.stale[at] = true;
                    // Its copy on the elements of a price-based chart type keeps its values too;
                    // while another type is shown the copies are not kept current and are rebuilt.
                    if self.chart_type.is_time_independent()
                        && let Some(stale) = self.element_studies.stale.get_mut(at)
                    {
                        *stale = true;
                    } else {
                        self.element_studies.dirty = true;
                    }
                }
                None => {}
            }
        }
        self.fill_fed_series();
    }
}

/// The study histories one pump changed, each with whether it changed before its last bar.
#[derive(Default)]
struct Changes(Vec<(StudyDataRequest, bool)>);

impl Changes {
    fn note(&mut self, request: &StudyDataRequest, hard: bool) {
        match self.0.iter_mut().find(|(changed, _)| changed == request) {
            Some((_, known)) => *known |= hard,
            None => self.0.push((request.clone(), hard)),
        }
    }
}

#[cfg(all(test, feature = "studies"))]
pub(super) mod tests {
    use super::*;
    use crate::data::{Reply, Sink};
    use crate::studies::{InputValue, SMA};
    use crate::widget::{ChartBuilder, test_frames::shapes};
    use chrono::{DateTime, Duration};

    #[derive(Default)]
    struct Feed {
        sinks: Mutex<Vec<(String, Sink)>>,
        correction: Mutex<f64>,
        metrics: Mutex<Vec<MetricSubscription>>,
    }
    impl Datafeed for Feed {
        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            let (prices, minutes) = match request.symbol.as_str() {
                "REMOTE" => (vec![10.0, 14.0, 20.0], 60),
                "OTHER" => (
                    vec![100.0, 200.0, 300.0 + *self.correction.lock().unwrap()],
                    60,
                ),
                _ => (vec![1.0; 5], 30),
            };
            reply.send(Ok(HistoryPage::new(bars(&prices, minutes), true)));
        }
        fn metric_history(&self, _: MetricRequest, reply: Reply<MetricPage>) {
            reply.send(Ok(MetricPage {
                points: [(15, 10.0), (75, 20.0)]
                    .into_iter()
                    .map(|(minute, value)| crate::data::MetricPoint {
                        time: DateTime::UNIX_EPOCH + Duration::minutes(minute),
                        value,
                    })
                    .collect(),
                end_of_history: true,
            }));
        }
        fn subscribe_metric(&self, subscription: MetricSubscription) {
            self.metrics.lock().unwrap().push(subscription);
        }
        fn subscribe(&self, subscription: Subscription) {
            self.sinks
                .lock()
                .unwrap()
                .push((subscription.symbol, subscription.sink));
        }
    }
    fn bars(prices: &[f64], minutes: i64) -> Vec<Bar> {
        prices
            .iter()
            .enumerate()
            .map(|(at, &price)| {
                Bar::new(
                    DateTime::UNIX_EPOCH + Duration::minutes(at as i64 * minutes),
                    price,
                    price,
                    price,
                    price,
                    1.0,
                )
            })
            .collect()
    }

    /// Counts the heap allocations of each thread, so a test can bound its own.
    struct Counting;

    thread_local! {
        static ALLOCATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    #[global_allocator]
    static ALLOCATOR: Counting = Counting;

    // SAFETY: every call forwards to `System` with the caller's arguments; the counter only observes.
    unsafe impl std::alloc::GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            // SAFETY: forwarded unchanged.
            unsafe { std::alloc::System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
            // SAFETY: forwarded unchanged.
            unsafe { std::alloc::System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(
            &self,
            ptr: *mut u8,
            layout: std::alloc::Layout,
            new_size: usize,
        ) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            // SAFETY: forwarded unchanged.
            unsafe { std::alloc::System.realloc(ptr, layout, new_size) }
        }
    }

    /// The heap allocations `run` makes on this thread, not counting the drop of its result.
    pub(in crate::widget) fn allocations<T>(run: impl FnOnce() -> T) -> (usize, T) {
        let before = ALLOCATIONS.with(std::cell::Cell::get);
        let result = run();
        (ALLOCATIONS.with(std::cell::Cell::get) - before, result)
    }

    /// The chart rebuilds its session profiles on every frame, so grouping rows must not copy
    /// each footprint bar: two-tick rows cost no more allocations than the feed's own rows.
    #[test]
    fn grouped_profile_rows_allocate_as_feed_rows_do() {
        use crate::data::Footprint;
        use crate::model::{BarClock, ProfileRows, Tick, Timeframe};
        use crate::studies::{TradeProfileRanges, trade_profiles_with_rows};
        let start: DateTime<chrono::Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        // 300 one-minute bars, each trading the ten prices 100 to 109 once.
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for minute in 0..300 {
            for price in 100..110 {
                let at = start + Duration::minutes(minute);
                flow.push(&Tick::new(at, f64::from(price), 1.0, None))
                    .unwrap();
            }
        }
        let ranges = TradeProfileRanges::new(vec![start..start + Duration::minutes(300)]).unwrap();
        let (feed, feed_profiles) =
            allocations(|| trade_profiles_with_rows(&flow, &ranges, ProfileRows::Feed));
        let (ticks, profiles) =
            allocations(|| trade_profiles_with_rows(&flow, &ranges, ProfileRows::Ticks(2)));
        assert_eq!(profiles[0].volume.levels.len(), 5);
        assert!(
            ticks <= feed,
            "two-tick rows allocate {ticks} times, feed rows {feed}"
        );
        let (feed, _) = allocations(|| feed_profiles[0].developing_value_area(&flow, 0.7));
        let (ticks, _) = allocations(|| profiles[0].developing_value_area(&flow, 0.7));
        assert!(
            ticks <= feed,
            "the developing area allocates {ticks} times on two-tick rows, {feed} on feed rows"
        );
    }

    #[test]
    fn primary_axis_layout_does_not_allocate() {
        use crate::model::AxisSide;
        use crate::widget::PaneId;
        let mut chart = Chart::new(crate::model::BarData::from_bars(bars(&[100.0; 30], 1)));
        chart.config.show_volume = false;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        for side in [AxisSide::Left, AxisSide::Right] {
            chart.place_pane_scales(PaneId::MAIN, side);
            let (count, (primary, left, right, axes, layout)) = allocations(|| {
                let layout = chart.calculate_layout_rects(rect);
                (
                    chart.primary_axis_side(),
                    chart.pane_scale_count(PaneId::MAIN, AxisSide::Left),
                    chart.pane_scale_count(PaneId::MAIN, AxisSide::Right),
                    chart
                        .pane_scale_axes(PaneId::MAIN, rect, layout.price_rect)
                        .count(),
                    layout,
                )
            });
            assert_eq!(count, 0, "primary axis queries must not allocate");
            assert_eq!(primary, Some(side));
            assert_eq!(left, usize::from(side == AxisSide::Left));
            assert_eq!(right, usize::from(side == AxisSide::Right));
            assert_eq!(axes, 1);
            assert!(layout.price_rect.width() < rect.width());
        }
    }

    /// One execution a second from the epoch, across 50 prices, for `instrument`.
    fn replay_driver(seconds: i64, instrument: Instrument) -> Arc<crate::data::ReplayDriver> {
        use crate::data::{MarketEvent, ReplayEvent};
        let second = |s: i64| DateTime::UNIX_EPOCH + Duration::seconds(s);
        let events = (0..seconds)
            .map(|s| {
                let fill = crate::model::Tick::new(second(s), 100.0 + (s % 50) as f64, 1.0, None);
                let fill = MarketEvent::Trade(fill.with_id(s as u64));
                ReplayEvent::new("A", second(s), s as u64, fill)
            })
            .collect();
        Arc::new(crate::data::ReplayDriver::new(events, vec![instrument]).unwrap())
    }

    /// Hosts advance the driver once per frame, also while it is paused and routes nothing.
    #[test]
    fn paused_replay_driver_frame_does_not_allocate() {
        let plain = Instrument::new("A").with_tick_size(1.0);
        let mut trading = plain.clone();
        trading.session = Some(crate::model::Session::new([]));
        for instrument in [plain, trading] {
            let driver = replay_driver(3, instrument.clone());
            let mut chart = ChartBuilder::new()
                .with_feed(driver.clone())
                .with_symbol("A")
                .with_timeframe(Timeframe::Min1)
                .build();
            let ctx = egui::Context::default();
            for _ in 0..4 {
                chart.pump(&ctx);
            }
            driver.set_speed(0.0).unwrap();
            driver.advance(std::time::Duration::ZERO).unwrap();
            let (count, result) =
                allocations(|| driver.advance(std::time::Duration::from_millis(16)));
            result.unwrap();
            assert_eq!(
                count, 0,
                "paused advance allocations, {:?}",
                instrument.session
            );
        }
    }

    /// A playing frame that delivers no execution must not fold the held ones again.
    #[cfg(feature = "drawings")]
    #[test]
    fn market_playback_frame_does_not_refold_held_executions() {
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
        let driver = replay_driver(2_000, Instrument::new("A").with_tick_size(1.0));
        driver.set_speed(0.0).unwrap();
        driver.seek(DateTime::UNIX_EPOCH + Duration::seconds(1_999));
        let mut chart = Chart::new(crate::model::BarData::from_bars(bars(&[100.0; 3], 1)));
        chart.set_symbol("A");
        chart.set_timeframe(Timeframe::Min1);
        chart.set_market_replay(Some(driver.clone()));
        let mut drawing = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
        drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(30.0, 100.0)];
        chart.drawings_mut().drawings.push(drawing);
        let ctx = egui::Context::default();
        for _ in 0..4 {
            chart.pump(&ctx);
        }
        let frame = |chart: &mut Chart| {
            allocations(|| {
                let elapsed = std::time::Duration::from_millis(16);
                driver.advance(elapsed).unwrap();
                chart.pump(&ctx);
            })
            .0
        };
        frame(&mut chart);
        let paused = frame(&mut chart);
        driver.set_speed(1.0).unwrap();
        frame(&mut chart);
        let playing = frame(&mut chart);
        assert!(
            playing <= paused + 8,
            "allocations per frame: paused {paused}, playing {playing}"
        );
    }

    #[derive(Default)]
    struct RetryFeed {
        resolves: Mutex<Vec<crate::data::Reply<Instrument>>>,
        histories: Mutex<Vec<(HistoryRequest, crate::data::Reply<HistoryPage>)>>,
        metrics: Mutex<Vec<(MetricRequest, crate::data::Reply<MetricPage>)>>,
        sinks: Mutex<Vec<Sink>>,
    }
    impl Datafeed for RetryFeed {
        fn resolve(&self, _: &str, reply: Reply<Instrument>) {
            self.resolves.lock().unwrap().push(reply);
        }
        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            self.histories.lock().unwrap().push((request, reply));
        }
        fn metric_history(&self, request: MetricRequest, reply: Reply<MetricPage>) {
            self.metrics.lock().unwrap().push((request, reply));
        }
        fn subscribe(&self, subscription: Subscription) {
            self.sinks.lock().unwrap().push(subscription.sink);
        }
        fn subscribe_metric(&self, subscription: MetricSubscription) {
            self.sinks.lock().unwrap().push(subscription.sink);
        }
    }
    impl RetryFeed {
        fn pending(&self, metric: bool) -> usize {
            if metric {
                self.metrics.lock().unwrap().len()
            } else {
                self.histories.lock().unwrap().len()
            }
        }
        fn answer(
            &self,
            metric: bool,
            result: Result<HistoryPage, FeedError>,
        ) -> (Option<DateTime<chrono::Utc>>, usize) {
            if metric {
                let (request, reply) = self.metrics.lock().unwrap().remove(0);
                reply.send(result.map(|page| {
                    MetricPage {
                        points: page
                            .bars
                            .into_iter()
                            .map(|bar| crate::data::MetricPoint {
                                time: bar.time,
                                value: bar.close,
                            })
                            .collect(),
                        end_of_history: page.end_of_history,
                    }
                }));
                (request.before, request.count)
            } else {
                let (request, reply) = self.histories.lock().unwrap().remove(0);
                reply.send(result);
                (request.before, request.count)
            }
        }
    }

    #[test]
    fn study_sources_retry_original_requests_and_ignore_superseded_replies() {
        for metric in [false, true] {
            for (error, delay) in [
                (FeedError::Network("offline".into()), 1.0),
                (
                    FeedError::RateLimited {
                        retry_after: Some(std::time::Duration::from_secs(7)),
                    },
                    7.0,
                ),
                (FeedError::Unanswered, 1.0),
                (FeedError::NotSupported, 0.0),
            ] {
                let feed = Arc::new(RetryFeed::default());
                let mut chart = Chart::new(crate::model::BarData::from_bars(bars(&[1.0; 5], 30)));
                chart.set_symbol("REMOTE");
                if metric {
                    chart.add_indicator(Box::new(crate::studies::MetricStudy::new(
                        crate::data::MetricSeries {
                            id: "oi".into(),
                            name: "Open interest".into(),
                            unit: "contracts".into(),
                            format: crate::scales::StudyFormat::Volume,
                        },
                    )));
                } else {
                    let id = chart.add_indicator(Box::new(SMA::new(1)));
                    chart
                        .indicator_mut(id)
                        .unwrap()
                        .set_input("calculation_symbol", InputValue::Symbol("REMOTE".into()))
                        .unwrap();
                }
                let ctx = egui::Context::default();
                let pump = |chart: &mut Chart, time| {
                    ctx.run_ui(
                        egui::RawInput {
                            time: Some(time),
                            ..Default::default()
                        },
                        |_| {},
                    )
                    .drop_without_applying_deltas();
                    chart.pump_study_data(&ctx, Some(feed.clone()));
                };
                pump(&mut chart, 0.0);
                feed.resolves
                    .lock()
                    .unwrap()
                    .pop()
                    .unwrap()
                    .send(Err(error.clone()));
                pump(&mut chart, 0.25);
                let request = chart.study_data_requests().pop().unwrap();
                assert_eq!(chart.study_data_error(&request), Some(&error));
                pump(&mut chart, 0.25 + delay - 0.01);
                assert!(feed.resolves.lock().unwrap().is_empty());
                pump(&mut chart, 0.25 + delay);
                if !error.is_recoverable() {
                    pump(&mut chart, 120.0);
                    assert!(feed.resolves.lock().unwrap().is_empty());
                    continue;
                }
                assert_eq!(feed.resolves.lock().unwrap().len(), 1, "{error:?}");
                feed.resolves
                    .lock()
                    .unwrap()
                    .pop()
                    .unwrap()
                    .send(Ok(Instrument::new("REMOTE")));
                let now = 0.5 + delay;
                pump(&mut chart, now);
                assert_eq!(chart.study_data_error(&request), None);
                let mut newest = bars(&[20.0], 60);
                newest[0].time += Duration::hours(2);
                assert_eq!(
                    feed.answer(metric, Ok(HistoryPage::new(newest, false))).0,
                    None
                );
                pump(&mut chart, now + 0.25);
                let failed = feed.answer(metric, Err(error.clone()));
                pump(&mut chart, now + 0.5);
                feed.sinks.lock().unwrap()[0].push_bar(Bar::new(
                    DateTime::UNIX_EPOCH + Duration::hours(1),
                    55.0,
                    55.0,
                    55.0,
                    55.0,
                    1.0,
                ));
                pump(&mut chart, now + 0.5 + delay - 0.01);
                assert_eq!(feed.pending(metric), 0);
                pump(&mut chart, now + 0.5 + delay);
                assert_eq!(feed.pending(metric), 1);
                let retried = feed.answer(
                    metric,
                    Ok(HistoryPage::new(bars(&[12.0, 55.0, 99.0, 100.0], 60), true)),
                );
                assert_eq!(
                    retried, failed,
                    "retry must keep its boundary and count after a live prepend"
                );
                assert_eq!(retried.0, Some(DateTime::UNIX_EPOCH + Duration::hours(2)));
                pump(&mut chart, now + 0.75 + delay);
                assert_eq!(chart.study_data_error(&request), None);
                assert_eq!(
                    chart.study_data.series[0]
                        .bars
                        .iter()
                        .map(|bar| bar.close)
                        .collect::<Vec<_>>(),
                    [12.0, 55.0, 20.0]
                );
                // A page from an earlier seek must not change the current request.
                for time in [now + 10.0, now + 11.0] {
                    feed.sinks.lock().unwrap()[0].reset_replay(DateTime::UNIX_EPOCH);
                    pump(&mut chart, time);
                }
                feed.answer(metric, Err(FeedError::Network("old page".into())));
                pump(&mut chart, now + 12.0);
                assert_eq!(chart.study_data_error(&request), None);
                feed.answer(metric, Ok(HistoryPage::new(bars(&[99.0], 60), true)));
                pump(&mut chart, now + 13.0);
                assert_eq!(chart.study_data_error(&request), None);
                assert_eq!(chart.study_data.series[0].bars[0].close, 99.0);
                assert_eq!(feed.pending(metric), 0);
            }
        }
    }

    #[test]
    fn study_retries_back_off_and_cancel_with_their_source() {
        for action in ["pause", "host data", "remove", "replace"] {
            let feed = Arc::new(RetryFeed::default());
            let mut chart = Chart::new(crate::model::BarData::new());
            let id = chart.add_indicator(Box::new(SMA::new(1)));
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input("calculation_symbol", InputValue::Symbol("REMOTE".into()))
                .unwrap();
            let ctx = egui::Context::default();
            let pump = |chart: &mut Chart, time| {
                ctx.run_ui(
                    egui::RawInput {
                        time: Some(time),
                        ..Default::default()
                    },
                    |_| {},
                )
                .drop_without_applying_deltas();
                chart.pump_study_data(&ctx, Some(feed.clone()));
            };
            pump(&mut chart, 0.0);
            feed.resolves
                .lock()
                .unwrap()
                .pop()
                .unwrap()
                .send(Err(FeedError::Network("offline".into())));
            pump(&mut chart, 0.1);
            pump(&mut chart, 1.1);
            feed.resolves
                .lock()
                .unwrap()
                .pop()
                .unwrap()
                .send(Err(FeedError::Network("still offline".into())));
            pump(&mut chart, 1.2);
            pump(&mut chart, 2.2);
            assert!(
                feed.resolves.lock().unwrap().is_empty(),
                "second failure waits two seconds"
            );
            match action {
                "pause" => {
                    chart.set_feed_paused(true);
                }
                "host data" => {
                    let request = chart.study_data_requests().pop().unwrap();
                    chart.set_study_data(request, bars(&[99.0], 60), None);
                }
                "remove" => {
                    chart.remove_series(id);
                }
                "replace" => {
                    chart
                        .indicator_mut(id)
                        .unwrap()
                        .set_input("calculation_symbol", InputValue::Symbol("OTHER".into()))
                        .unwrap();
                }
                _ => unreachable!(),
            }
            pump(&mut chart, 60.0);
            assert_eq!(
                feed.resolves.lock().unwrap().len(),
                usize::from(action == "replace"),
                "{action}"
            );
        }
    }

    #[test]
    fn replay_reset_discards_future_study_bars_and_tick_origins() {
        for replacement in [false, true] {
            let feed = Arc::new(Feed::default());
            let mut chart = ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol("MAIN")
                .with_timeframe(Timeframe::Hour1)
                .build();
            let id = chart.add_indicator(Box::new(SMA::new(1)));
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input("calculation_symbol", InputValue::Symbol("REMOTE".into()))
                .unwrap();
            let ctx = egui::Context::default();
            for frame in 0..8 {
                shapes(&ctx, &mut chart, frame as f64, Vec::new());
            }
            let sink = feed
                .sinks
                .lock()
                .unwrap()
                .iter()
                .find(|(symbol, _)| symbol == "REMOTE")
                .unwrap()
                .1
                .clone();
            sink.set_server_time(DateTime::UNIX_EPOCH + Duration::hours(4));
            sink.push_tick(crate::model::Tick::new(
                DateTime::UNIX_EPOCH + Duration::hours(3),
                30.0,
                1.0,
                None,
            ));
            chart.pump_study_data(&ctx, Some(feed.clone()));
            assert!(!chart.study_data.series[0].from_ticks.is_empty());
            sink.reset_replay(DateTime::UNIX_EPOCH);
            if replacement {
                sink.push_tick(crate::model::Tick::new(
                    DateTime::UNIX_EPOCH,
                    99.0,
                    1.0,
                    None,
                ));
            }
            chart.pump_study_data(&ctx, Some(feed.clone()));
            let series = &chart.study_data.series[0];
            assert_eq!(series.bars.len(), usize::from(replacement));
            assert!(
                series
                    .from_ticks
                    .keys()
                    .all(|time| *time == DateTime::UNIX_EPOCH)
            );
            if replacement {
                assert_eq!(series.bars[0].close, 99.0);
            }
            assert_eq!(chart.pending_from[0], Some(0));
        }
    }

    /// Metric streams keep exact publication times, refresh dependent studies, and use
    /// the normal subscription lifetime when the chart's instrument changes.
    #[test]
    fn metric_history_and_live_points_refresh_the_pane_and_its_consumers() {
        use crate::data::{MetricPoint, MetricSeries};
        use crate::scales::StudyFormat;
        use crate::studies::{MetricStudy, StudySource};
        let feed = Arc::new(Feed::default());
        let mut chart = ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("MAIN")
            .with_timeframe(Timeframe::Min30)
            .build();
        let metric = chart.add_indicator(Box::new(MetricStudy::new(MetricSeries {
            id: "oi".into(),
            name: "Open interest".into(),
            unit: "contracts".into(),
            format: StudyFormat::Volume,
        })));
        let average = chart.add_indicator(Box::new(SMA::new(2)));
        chart
            .set_indicator_source(
                average,
                StudySource::Study {
                    id: metric.0,
                    plot: 0,
                },
            )
            .unwrap();
        let ctx = egui::Context::default();
        let settle = |chart: &mut Chart| {
            for frame in 0..12 {
                shapes(&ctx, chart, frame as f64, Vec::new());
            }
        };
        let output = |chart: &Chart, id| {
            let study = chart.indicators().find(|(key, _)| *key == id).unwrap().1;
            (0..5)
                .map(|at| study.values().value(0, at))
                .collect::<Vec<_>>()
        };
        settle(&mut chart);
        assert_eq!(
            output(&chart, metric),
            [None, Some(10.0), Some(10.0), Some(20.0), Some(20.0)]
        );
        assert_eq!(
            output(&chart, average),
            [None, None, Some(10.0), Some(15.0), Some(20.0)]
        );
        let sink = feed.metrics.lock().unwrap()[0].sink.clone();
        for (minutes, value) in [(15, 14.0), (110, 30.0)] {
            sink.push_metric(MetricPoint {
                time: DateTime::UNIX_EPOCH + Duration::minutes(minutes),
                value,
            });
        }
        for frame in 0..60 {
            let mut last = bars(&[1.0; 5], 30).pop().unwrap();
            last.close = 1.0 + frame as f64 * 0.001;
            last.high = last.close;
            chart.update(super::super::SeriesId::MAIN, last);
            shapes(&ctx, &mut chart, 12.0 + frame as f64 / 60.0, Vec::new());
        }
        assert_eq!(
            output(&chart, metric),
            [None, Some(14.0), Some(14.0), Some(20.0), Some(30.0)]
        );
        assert_eq!(
            output(&chart, average),
            [None, None, Some(14.0), Some(17.0), Some(25.0)]
        );
        chart.set_symbol("OTHER");
        settle(&mut chart);
        assert!(!sink.is_active());
        drop(chart);
        assert!(
            feed.metrics
                .lock()
                .unwrap()
                .iter()
                .all(|sub| !sub.sink.is_active())
        );
    }

    #[test]
    fn requested_intervals_align_confirmed_and_live_values_and_replace_subscriptions() {
        let feed = Arc::new(Feed::default());
        let mut chart = ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("MAIN")
            .with_timeframe(Timeframe::Min30)
            .build();
        let id = chart.add_indicator(Box::new(SMA::new(2)));
        let study = chart.indicator_mut(id).unwrap();
        study
            .set_input("calculation_symbol", InputValue::Symbol("REMOTE".into()))
            .unwrap();
        study
            .set_input(
                "calculation_timeframe",
                InputValue::Timeframe(Some(Timeframe::Hour1)),
            )
            .unwrap();
        let ctx = egui::Context::default();
        let settle = |chart: &mut Chart| {
            for frame in 0..10 {
                shapes(&ctx, chart, frame as f64, Vec::new());
            }
        };
        settle(&mut chart);
        let output = |chart: &Chart| {
            (0..5)
                .map(|at| {
                    chart
                        .indicators()
                        .find(|(candidate, _)| *candidate == id)
                        .unwrap()
                        .1
                        .values()
                        .value(0, at)
                })
                .collect::<Vec<_>>()
        };
        // Hourly SMA(2) first closes at 02:00 with (10+14)/2=12. The newest value is (14+20)/2=17.
        assert_eq!(output(&chart), [None, None, None, Some(12.0), Some(12.0)]);
        // The forming 02:30 bar has not reached 03:00, so its scheduled close cannot
        // confirm the 02:00 hourly value while waiting for the interval close.
        chart.set_data(super::super::SeriesId::MAIN, bars(&[1.0; 6], 30));
        settle(&mut chart);
        assert_eq!(
            chart
                .indicators()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1
                .values()
                .value(0, 5),
            Some(12.0)
        );
        chart.set_data(super::super::SeriesId::MAIN, bars(&[1.0; 5], 30));
        settle(&mut chart);
        chart
            .indicator_mut(id)
            .unwrap()
            .set_input("wait_for_close", InputValue::Toggle(false))
            .unwrap();
        settle(&mut chart);
        assert_eq!(output(&chart), [None, None, None, Some(12.0), Some(17.0)]);
        let sink = feed
            .sinks
            .lock()
            .unwrap()
            .iter()
            .find(|(symbol, _)| symbol == "REMOTE")
            .unwrap()
            .1
            .clone();
        sink.push_bar(bars(&[10.0, 14.0, 24.0], 60).pop().unwrap());
        settle(&mut chart);
        assert_eq!(output(&chart)[4], Some(19.0));
        chart
            .indicator_mut(id)
            .unwrap()
            .set_input("calculation_symbol", InputValue::Symbol("OTHER".into()))
            .unwrap();
        settle(&mut chart);
        assert!(!sink.is_active());
        assert_eq!(output(&chart)[4], Some(250.0));
        let other_sink = feed
            .sinks
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(symbol, _)| symbol == "OTHER")
            .unwrap()
            .1
            .clone();
        chart.set_extended_hours(!chart.extended_hours());
        settle(&mut chart);
        assert!(!other_sink.is_active());
        assert_eq!(output(&chart)[4], Some(250.0));
        chart.set_feed_paused(true);
        *feed.correction.lock().unwrap() = 100.0;
        chart.set_feed_paused(false);
        settle(&mut chart);
        assert_eq!(
            output(&chart)[4],
            Some(300.0),
            "resuming reloads missed bars"
        );
        let request = chart.study_data_requests().pop().unwrap();
        chart.set_study_data(request, bars(&[2.0, 4.0, 8.0], 60), None);
        settle(&mut chart);
        assert_eq!(output(&chart)[4], Some(6.0));
        chart
            .indicator_mut(id)
            .unwrap()
            .set_input("wait_for_close", InputValue::Toggle(true))
            .unwrap();
        for (name, minutes, stamp, missing, expected) in [
            (
                "stamped inside hourly slots",
                60,
                10,
                None,
                [None, None, None, Some(3.0), Some(3.0)],
            ),
            ("missing hourly slot", 60, 0, Some(1), [None; 5]),
            (
                "missing same-interval slot",
                30,
                0,
                Some(1),
                [None, None, Some(5.0), None, None],
            ),
        ] {
            let timeframe = if minutes == 60 {
                Timeframe::Hour1
            } else {
                Timeframe::Min30
            };
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input(
                    "calculation_timeframe",
                    InputValue::Timeframe(Some(timeframe)),
                )
                .unwrap();
            let source = bars(&[2.0, 4.0, 8.0], minutes)
                .into_iter()
                .enumerate()
                .filter(|(at, _)| Some(*at) != missing)
                .map(|(_, mut bar)| {
                    bar.time += Duration::minutes(stamp);
                    bar
                })
                .collect();
            let request = chart.study_data_requests().pop().unwrap();
            chart.set_study_data(request, source, None);
            settle(&mut chart);
            assert_eq!(output(&chart), expected, "{name}");
        }
    }

    /// Under replay, a study on another symbol or interval reads only the bars its history
    /// completed by the cursor. The chart's unfinished bar must not show the closing value of the
    /// other history's bar in the same slot, whichever way the cursor moves; leaving replay
    /// restores the full history.
    #[test]
    fn replay_hides_the_future_of_other_histories() {
        let hourly = bars(&[1.0; 3], 60);
        let mut chart = Chart::new(crate::model::BarData::from_bars(hourly));
        chart.set_symbol("LOCAL");
        chart.set_timeframe(Timeframe::Hour1);
        let id = chart.add_indicator(Box::new(SMA::new(1)));
        chart
            .indicator_mut(id)
            .unwrap()
            .set_input("calculation_symbol", InputValue::Symbol("REMOTE".into()))
            .unwrap();
        let request = chart.study_data_requests().pop().unwrap();
        chart.set_study_data(request, bars(&[10.0, 14.0, 24.0], 60), None);
        let ctx = egui::Context::default();
        let mut frame = 0.0;
        let mut values = |chart: &mut Chart| {
            for _ in 0..4 {
                frame += 1.0;
                shapes(&ctx, chart, frame, Vec::new());
            }
            let study = chart.indicators().next().unwrap().1;
            let shown: Vec<_> = (0..study.values().len())
                .map(|at| study.values().value(0, at))
                .collect();
            #[cfg(feature = "csv")]
            {
                let exported = chart.export_csv().unwrap();
                let mut reader = csv::Reader::from_reader(exported.as_bytes());
                let column = reader
                    .headers()
                    .unwrap()
                    .iter()
                    .position(|name| name == "SMA: SMA(1)")
                    .unwrap();
                let exported: Vec<Option<f64>> = reader
                    .records()
                    .map(|row| row.unwrap()[column].parse().ok())
                    .collect();
                assert_eq!(exported, shown, "export uses the same replay cutoff");
            }
            shown
        };
        let minute = |m: i64| DateTime::UNIX_EPOCH + Duration::minutes(m);
        chart.set_replay_time(Some(minute(90)));
        assert!(chart.set_replay_source(
            bars(&[1.0; 180], 1),
            crate::model::BarClock::new(Timeframe::Min1)
        ));
        // The other symbol's hour from 01:00 is unfinished at 01:30, so it has no value there.
        assert_eq!(values(&mut chart), [Some(10.0), None]);
        chart.set_replay_time(Some(minute(150)));
        assert_eq!(values(&mut chart), [Some(10.0), Some(14.0), None]);
        chart.set_replay_time(Some(minute(90)));
        assert_eq!(values(&mut chart), [Some(10.0), None]);
        chart.set_replay_time(None);
        assert_eq!(values(&mut chart), [Some(10.0), Some(14.0), Some(24.0)]);

        let driver = Arc::new(
            crate::data::ReplayDriver::new(
                (0..180)
                    .map(|m| {
                        crate::data::ReplayEvent::new(
                            chart.symbol(),
                            minute(m),
                            m as u64,
                            crate::data::MarketEvent::Trade(crate::model::Tick::new(
                                minute(m),
                                1.0,
                                1.0,
                                None,
                            )),
                        )
                    })
                    .collect(),
                vec![crate::model::Instrument::new(chart.symbol())],
            )
            .unwrap(),
        );
        driver.seek(minute(90));
        chart.set_market_replay(Some(driver.clone()));
        assert_eq!(values(&mut chart), [Some(10.0), None]);
        driver.seek(minute(150));
        assert_eq!(values(&mut chart), [Some(10.0), Some(14.0), None]);
        driver.seek(minute(90));
        assert_eq!(values(&mut chart), [Some(10.0), None]);
        chart.set_market_replay(None);
        assert_eq!(values(&mut chart), [Some(10.0), Some(14.0), Some(24.0)]);
    }

    /// A volume profile on a finer interval loads that history back to the first bar its
    /// profiles in view read, not to the chart's first bar, and loads more only when that bar
    /// moves earlier. Price-based chart types keep loading to the chart's first bar.
    #[test]
    fn finer_profile_history_stops_at_the_profile_span() {
        use crate::model::ChartType;
        let feed = Arc::new(RetryFeed::default());
        let mut chart = Chart::new(crate::model::BarData::from_bars(bars(&[1.0; 10], 60)));
        chart.set_symbol("MAIN");
        chart.set_timeframe(Timeframe::Hour1);
        let mut profile: Box<dyn crate::studies::Indicator> =
            Box::new(crate::studies::VolProfileVisible::new(10));
        profile
            .set_input(
                "source_interval",
                InputValue::Timeframe(Some(Timeframe::Min30)),
            )
            .unwrap();
        let id = chart.add_indicator(profile);
        let ctx = egui::Context::default();
        // Half-hour bars from 00:00 to 09:30, answered two at a time from the newest.
        let fine = bars(&[1.0; 20], 30);
        let hour = |hour: i64| Some(DateTime::UNIX_EPOCH + Duration::hours(hour));
        for (chart_type, view, expected) in [
            (ChartType::Candles, 8..10, vec![None, hour(9)]),
            (ChartType::Candles, 5..10, vec![hour(8), hour(7), hour(6)]),
            (ChartType::Candles, 9..10, vec![]),
            (
                ChartType::Renko,
                9..10,
                vec![hour(5), hour(4), hour(3), hour(2), hour(1)],
            ),
        ] {
            chart.set_chart_type(chart_type);
            chart
                .indicator_mut(id)
                .unwrap()
                .set_visible_range(view.clone());
            let mut befores = Vec::new();
            loop {
                ctx.run_ui(egui::RawInput::default(), |_| {})
                    .drop_without_applying_deltas();
                chart.pump_study_data(&ctx, Some(feed.clone()));
                let resolve = feed.resolves.lock().unwrap().pop();
                if let Some(reply) = resolve {
                    reply.send(Ok(Instrument::new("MAIN")));
                    continue;
                }
                let Some((request, reply)) = feed.histories.lock().unwrap().pop() else {
                    break;
                };
                befores.push(request.before);
                let end = fine
                    .partition_point(|bar| request.before.is_none_or(|before| bar.time < before));
                reply.send(Ok(HistoryPage::new(
                    fine[end.saturating_sub(2)..end].to_vec(),
                    end <= 2,
                )));
            }
            assert_eq!(befores, expected, "{chart_type:?} {view:?}");
        }
    }

    /// Guards #1667 and #1681: each full recompute of the profiles that read histories runs on
    /// its own frame, and a profile waiting for its turn keeps requesting the finer history it
    /// read and the start it read it from. So that history is loaded once from the newest bar,
    /// only back to the earliest profile start in view (the session open at 1970-01-13 00:00, or
    /// bar 305 at 17:00 for the visible range alone), and the recompute reads all 190 half-hour
    /// bars of volume 1 under bars 305 to 399. The feed answers ten hours a page.
    #[test]
    fn waiting_profiles_keep_their_finer_history() {
        use crate::model::ProfileResolution;
        use crate::studies::{Indicator, VolProfileSession, VolProfileVisible};
        let hour = |hour: i64| DateTime::UNIX_EPOCH + Duration::hours(hour);
        let hourly = bars(&[1.0; 400], 60);
        let suggested = |intervals: &[(Timeframe, Option<i64>)]| {
            Instrument::new("MAIN").with_profile_resolutions(intervals.iter().map(
                |&(timeframe, days)| {
                    ProfileResolution::new(timeframe, days.map(chrono::TimeDelta::days))
                },
            ))
        };
        let explicit = |mut study: Box<dyn Indicator>, timeframe| {
            study
                .set_input("source_interval", InputValue::Timeframe(Some(timeframe)))
                .unwrap();
            study
        };
        let session = || Box::new(VolProfileSession::default()) as Box<dyn Indicator>;
        let visible = || Box::new(VolProfileVisible::default()) as Box<dyn Indicator>;
        let finer = [(Timeframe::Min15, Some(1)), (Timeframe::Min30, None)];
        let half = [(Timeframe::Min30, None)];
        for (name, instrument, studies, main_page, last) in [
            (
                "session on 15 minutes, visible range on 30, as suggested",
                suggested(&finer),
                [session(), visible()],
                400,
                22,
            ),
            (
                "older chart bars arrive after the half-hour history",
                suggested(&half),
                [Box::new(SMA::new(5)) as Box<dyn Indicator>, visible()],
                200,
                22,
            ),
            (
                "both on suggested 30 minutes",
                suggested(&half),
                [session(), visible()],
                400,
                2,
            ),
            (
                "both on explicit 30 minutes",
                suggested(&[]),
                [
                    explicit(session(), Timeframe::Min30),
                    explicit(visible(), Timeframe::Min30),
                ],
                400,
                2,
            ),
            (
                "explicit 15 minute session, 30 minute visible range",
                suggested(&[]),
                [
                    explicit(session(), Timeframe::Min15),
                    explicit(visible(), Timeframe::Min30),
                ],
                400,
                22,
            ),
        ] {
            let feed = Arc::new(RetryFeed::default());
            let mut chart = ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol("MAIN")
                .with_timeframe(Timeframe::Hour1)
                .build();
            let id = studies.map(|study| chart.add_indicator(study))[1];
            let ctx = egui::Context::default();
            let mut befores = Vec::new();
            for frame in 0..150 {
                shapes(&ctx, &mut chart, frame as f64, Vec::new());
                for reply in feed.resolves.lock().unwrap().drain(..) {
                    reply.send(Ok(instrument.clone()));
                }
                let pending: Vec<_> = feed.histories.lock().unwrap().drain(..).collect();
                for (request, reply) in pending {
                    let minutes = request.timeframe.duration_ms() / 60_000;
                    let before = request.before.unwrap_or(hour(400));
                    if minutes == 60 {
                        // Older chart bars are held until the half-hour history has loaded.
                        if request.before.is_some() && frame < 60 {
                            feed.histories.lock().unwrap().push((request, reply));
                            continue;
                        }
                        let end = hourly.partition_point(|bar| bar.time < before);
                        let start = end.saturating_sub(main_page);
                        let page = hourly[start..end].to_vec();
                        reply.send(Ok(HistoryPage::new(page, start == 0)));
                        continue;
                    }
                    if minutes == 30 {
                        befores.push(request.before);
                    }
                    let page: Vec<_> = bars(&vec![1.0; 24_000 / minutes as usize], minutes)
                        .into_iter()
                        .filter(|bar| bar.time < before && bar.time >= before - Duration::hours(10))
                        .collect();
                    let end = page.first().is_none_or(|bar| bar.time == hour(0));
                    reply.send(Ok(HistoryPage::new(page, end)));
                }
            }
            let held = chart
                .study_data
                .series
                .iter()
                .find(|series| series.request.timeframe == Some(Timeframe::Min30))
                .map_or(0, |series| {
                    series
                        .bars
                        .iter()
                        .filter(|bar| bar.time >= hour(305))
                        .count()
                });
            let profile = chart.indicators().find(|(key, _)| *key == id).unwrap().1;
            let volume: f64 = profile
                .volume_profiles(305..400)
                .iter()
                .flat_map(|plot| plot.profile.volume())
                .sum();
            assert_eq!(
                (
                    befores.iter().filter(|before| before.is_none()).count(),
                    befores.last().copied().flatten(),
                    held,
                    volume,
                ),
                (1, Some(hour(12 * 24 + last)), 190, 190.0),
                "{name}"
            );
        }
    }

    /// Guards #2111: Performance's 1Y target on a one-minute chart lies far before the chart's
    /// first bar, so its daily history pages back to the target, whatever the feed's page size
    /// and even when a study with no stated start reads the same history. Then it stops.
    #[test]
    fn performance_daily_history_reaches_a_year_with_short_pages() {
        use crate::studies::Indicator;
        use chrono::Datelike;
        let flat = |time, close| Bar::new(time, close, close, close, close, 1.0);
        let open: DateTime<chrono::Utc> = "2024-03-15T13:30:00Z".parse().unwrap();
        // A close of 50 at 00:00 UTC on every weekday of the two years before 2024-03-15.
        let today: DateTime<chrono::Utc> = "2024-03-15T00:00:00Z".parse().unwrap();
        let daily: Vec<_> = (1..=730)
            .rev()
            .map(|day| today - Duration::days(day))
            .filter(|day| day.weekday().number_from_monday() <= 5)
            .map(|day| flat(day, 50.0))
            .collect();
        for shared in [false, true] {
            let feed = Arc::new(RetryFeed::default());
            let minutes = (0..390)
                .map(|minute| flat(open + Duration::minutes(minute), 100.0))
                .collect();
            let mut chart = Chart::new(crate::model::BarData::from_bars(minutes));
            chart.set_symbol("MAIN");
            chart.set_timeframe(Timeframe::Min1);
            let id = chart.add_indicator(Box::new(crate::studies::Performance::new()));
            if shared {
                let mut average = crate::studies::ConfiguredStudy::new(Box::new(SMA::new(2)));
                average
                    .set_input(
                        "calculation_timeframe",
                        InputValue::Timeframe(Some(Timeframe::Day1)),
                    )
                    .unwrap();
                chart.add_indicator(Box::new(average));
            }
            let ctx = egui::Context::default();
            let mut pages = 0;
            loop {
                ctx.run_ui(egui::RawInput::default(), |_| {})
                    .drop_without_applying_deltas();
                chart.pump_study_data(&ctx, Some(feed.clone()));
                let resolve = feed.resolves.lock().unwrap().pop();
                if let Some(reply) = resolve {
                    reply.send(Ok(Instrument::new("MAIN")));
                    continue;
                }
                let Some((request, reply)) = feed.histories.lock().unwrap().pop() else {
                    break;
                };
                // At most 250 bars a page, the newest before the request.
                let end = daily
                    .partition_point(|bar| request.before.is_none_or(|before| bar.time < before));
                pages += 1;
                reply.send(Ok(HistoryPage::new(
                    daily[end.saturating_sub(250)..end].to_vec(),
                    end <= 250,
                )));
            }
            chart.recalculate_studies();
            let study = chart
                .indicators()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1;
            // Every period starts from a close of 50: 100 * (100 - 50) / 50 = 100.
            let changes: Vec<_> = (0..6).map(|line| study.values().value(line, 389)).collect();
            assert_eq!(changes, [Some(100.0); 6], "shared {shared}");
            // The first page ends on 2023-03-31 and the second passes the target, 2023-03-15; the
            // two-year feed holds a third.
            assert_eq!(pages, 2, "shared {shared}");
        }
    }

    /// Guards #1668: the pass that runs every frame to resolve the study histories also works
    /// out how far back each loads, allocating for the requests alone. Two profiles read one
    /// finer history here; the session profile reads the half-hour bars from 00:00.
    #[test]
    fn study_data_reaches_allocate_only_the_requests() {
        use crate::studies::{Indicator, VolProfileSession, VolProfileVisible};
        let mut chart = Chart::new(crate::model::BarData::from_bars(bars(&[1.0; 30], 60)));
        chart.set_symbol("MAIN");
        chart.set_timeframe(Timeframe::Hour1);
        for mut profile in [
            Box::new(VolProfileSession::new(10)) as Box<dyn Indicator>,
            Box::new(VolProfileVisible::new(10)),
        ] {
            profile
                .set_input(
                    "source_interval",
                    InputValue::Timeframe(Some(Timeframe::Min30)),
                )
                .unwrap();
            chart.add_indicator(profile);
        }
        let ctx = egui::Context::default();
        for frame in 0..3 {
            shapes(&ctx, &mut chart, frame as f64, Vec::new());
        }
        let (requests, expected) = allocations(|| chart.study_data_requests());
        let (reaches, resolved) = allocations(|| chart.study_data_reaches());
        assert_eq!(reaches, requests);
        assert_eq!(
            resolved,
            expected
                .into_iter()
                .map(|request| (request, Some(DateTime::UNIX_EPOCH)))
                .collect::<Vec<_>>()
        );
    }

    /// Drawings request one finer history and page only to their earliest visible anchor.
    #[cfg(feature = "drawings")]
    #[test]
    fn drawing_profile_history_stops_at_the_profile_span() {
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
        use crate::model::{ChartType, ProfileResolution};
        use crate::studies::Indicator;
        let feed = Arc::new(RetryFeed::default());
        let mut chart = Chart::new(crate::model::BarData::from_bars(bars(&[1.0; 10], 60)));
        chart.set_symbol("MAIN");
        chart.set_timeframe(Timeframe::Hour1);
        chart.set_instrument(Some(Instrument::new("MAIN").with_profile_resolutions([
            ProfileResolution::new(Timeframe::Min30, Some(Duration::hours(12))),
        ])));
        let mut drawing = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
        drawing.chart_points = vec![ChartPoint::new(7.0, 1.0), ChartPoint::new(8.0, 1.0)];
        chart.drawings_mut().drawings.push(drawing);
        let request = StudyDataRequest {
            symbol: "MAIN".into(),
            timeframe: Some(Timeframe::Min30),
            metric: None,
        };
        assert_eq!(
            chart.study_data_requests().as_slice(),
            std::slice::from_ref(&request)
        );
        let ctx = egui::Context::default();
        let fine = bars(&[1.0; 20], 30);
        let hour = |h| Some(DateTime::UNIX_EPOCH + Duration::hours(h));
        for (anchor, expected) in [
            (7.0, vec![None, hour(9), hour(8)]),
            (5.0, vec![hour(7), hour(6)]),
        ] {
            chart.drawings_mut().drawings[0].chart_points[0].bar_idx = anchor;
            let mut befores = Vec::new();
            loop {
                ctx.run_ui(egui::RawInput::default(), |_| {})
                    .drop_without_applying_deltas();
                chart.pump_study_data(&ctx, Some(feed.clone()));
                if let Some(reply) = feed.resolves.lock().unwrap().pop() {
                    reply.send(Ok(Instrument::new("MAIN")));
                    continue;
                }
                let Some((asked, reply)) = feed.histories.lock().unwrap().pop() else {
                    break;
                };
                befores.push(asked.before);
                let end =
                    fine.partition_point(|bar| asked.before.is_none_or(|before| bar.time < before));
                reply.send(Ok(HistoryPage::new(
                    fine[end.saturating_sub(2)..end].to_vec(),
                    end <= 2,
                )));
            }
            assert_eq!(befores, expected, "anchor {anchor}");
        }
        // A study sharing the interval still controls its own earlier reach.
        let mut study = crate::studies::VolProfileFixed::new(2);
        study
            .set_input(
                "source_interval",
                InputValue::Timeframe(Some(Timeframe::Min30)),
            )
            .unwrap();
        study.set_timeframe(Timeframe::Hour1);
        study.calculate(&bars(&[1.0; 10], 60));
        study.set_visible_range(0..10);
        let id = chart.add_indicator(Box::new(study));
        assert_eq!(chart.study_data_requests(), [request]);
        assert_eq!(chart.study_data_reaches()[0].1, hour(0));
        chart.remove_series(id);
        chart.drawings_mut().drawings[0].visible = false;
        assert!(chart.study_data_requests().is_empty());
        chart.drawings_mut().drawings[0].visible = true;
        chart.set_chart_type(ChartType::Renko);
        assert!(chart.study_data_requests().is_empty());
    }

    /// Trades that move today's daily bar of a one-minute chart recompute Performance, which reads
    /// the daily bars, at most once per refresh interval and keep it drawn while it waits; an SMA
    /// calculated on the hourly bars is neither recomputed nor blanked, whichever was added first,
    /// on candles and on Renko bricks.
    #[test]
    fn live_daily_bars_refresh_only_their_studies_and_keep_them_drawn() {
        use crate::model::ChartType;
        use crate::studies::{Indicator, IndicatorValues, Performance};
        use std::sync::atomic::{AtomicUsize, Ordering};
        /// A study counting its calculations from the first bar.
        struct Counted(Box<dyn Indicator>, Arc<AtomicUsize>);
        impl Counted {
            fn count(&self, data: &[Bar], from: usize) {
                if from == 0 && !data.is_empty() {
                    self.1.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        impl Indicator for Counted {
            fn name(&self) -> &str {
                self.0.name()
            }
            fn calculate(&mut self, data: &[Bar]) {
                self.update_data(data, 0, &[]);
            }
            fn update(&mut self, data: &[Bar], from: usize) {
                self.count(data, from);
                self.0.update(data, from);
            }
            fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
                self.count(data, from);
                self.0.update_data(data, from, sources);
            }
            fn data_requests(&self) -> Vec<StudyDataRequest> {
                self.0.data_requests()
            }
            fn set_timeframe(&mut self, timeframe: Timeframe) {
                self.0.set_timeframe(timeframe);
            }
            fn is_incremental(&self) -> bool {
                self.0.is_incremental()
            }
            fn values(&self) -> &IndicatorValues {
                self.0.values()
            }
            fn colors(&self) -> Vec<egui::Color32> {
                self.0.colors()
            }
            fn set_colors(&mut self, colors: Vec<egui::Color32>) {
                self.0.set_colors(colors);
            }
            fn is_visible(&self) -> bool {
                self.0.is_visible()
            }
            fn set_visible(&mut self, visible: bool) {
                self.0.set_visible(visible);
            }
            fn clone_box(&self) -> Box<dyn Indicator> {
                Box::new(Counted(self.0.clone_box(), self.1.clone()))
            }
        }
        for (chart_type, performance_first) in [
            (ChartType::Candles, true),
            (ChartType::Candles, false),
            (ChartType::Renko, true),
            (ChartType::Renko, false),
        ] {
            let feed = Arc::new(Live::default());
            let mut chart = ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol("MAIN")
                .with_timeframe(Timeframe::Min1)
                .build();
            chart.set_chart_type(chart_type);
            let counts = [(); 2].map(|_| Arc::new(AtomicUsize::new(0)));
            let mut studies = [
                Counted(Box::new(Performance::new()), counts[0].clone()),
                Counted(Box::new(SMA::new(3)), counts[1].clone()),
            ]
            .map(Some);
            let mut ids = [None; 2];
            for at in if performance_first { [0, 1] } else { [1, 0] } {
                ids[at] = Some(chart.add_indicator(Box::new(studies[at].take().unwrap())));
            }
            let ids = ids.map(Option::unwrap);
            chart
                .indicator_mut(ids[1])
                .unwrap()
                .set_input(
                    "calculation_timeframe",
                    InputValue::Timeframe(Some(Timeframe::Hour1)),
                )
                .unwrap();
            let ctx = egui::Context::default();
            for frame in 0..10 {
                shapes(&ctx, &mut chart, frame as f64, Vec::new());
            }
            // The drawn values, on Renko those calculated on the bricks.
            let last = |chart: &Chart| {
                ids.map(|id| {
                    let at = chart
                        .indicators()
                        .position(|(other, _)| other == id)
                        .unwrap();
                    let values = chart.rendered_studies().indicators()[at].values();
                    values
                        .len()
                        .checked_sub(1)
                        .and_then(|at| values.value(0, at))
                })
            };
            // 1W against a daily close of 50 is 100 * (100 - 50) / 50 = 100, and hourly closes of
            // 100 give an SMA of 100 at the last bar or brick, which closes at 100.
            assert_eq!(
                last(&chart),
                [Some(100.0); 2],
                "{chart_type:?} {performance_first}"
            );
            let sink = feed.sink(Timeframe::Day1);
            let today: DateTime<chrono::Utc> = "2024-03-15T00:00:00Z".parse().unwrap();
            for count in &counts {
                count.store(0, Ordering::Relaxed);
            }
            let mut blank = [0; 2];
            // Half a second of trades at 60 frames a second, each moving today's daily bar.
            for frame in 0..30 {
                let close = 100.0 + frame as f64;
                sink.push_bar(Bar::new(today, 100.0, close, 100.0, close, 1.0));
                shapes(&ctx, &mut chart, 10.0 + frame as f64 / 60.0, Vec::new());
                for (blank, value) in blank.iter_mut().zip(last(&chart)) {
                    *blank += usize::from(value.is_none());
                }
            }
            let counts = counts.map(|count| count.load(Ordering::Relaxed));
            // On Renko the studies of the bricks recompute too.
            let limit = if chart_type == ChartType::Renko { 6 } else { 3 };
            assert!(
                counts[0] <= limit && counts[1] == 0 && blank == [0, 0],
                "{chart_type:?}, Performance first: {performance_first}, full recalculations of \
                 Performance and the SMA in half a second {counts:?}, frames without a last value \
                 {blank:?}"
            );
        }
    }

    /// A study calculated on an SMA of the hourly bars follows it while trades move the current
    /// hourly bar of a one-minute chart, although that SMA has a full recompute waiting on every
    /// frame. A second apart, longer than any refresh wait, the two take turns at the one full
    /// recompute per frame, so the study shows a close at most three frames old.
    /// Hand values: hourly closes of 100 and a current close of 200 + frame give an SMA(3) of
    /// (200 + close) / 3, which an SMA(1) repeats; the last of ten frames has a close of 209.
    #[test]
    fn a_study_on_a_live_history_study_follows_it() {
        let feed = Arc::new(Live::default());
        let mut chart = ChartBuilder::new()
            .with_feed(feed.clone())
            .with_symbol("MAIN")
            .with_timeframe(Timeframe::Min1)
            .build();
        let parent = chart.add_indicator(Box::new(SMA::new(3)));
        let study = chart.indicator_mut(parent).unwrap();
        let hourly = InputValue::Timeframe(Some(Timeframe::Hour1));
        study.set_input("calculation_timeframe", hourly).unwrap();
        study
            .set_input("wait_for_close", InputValue::Toggle(false))
            .unwrap();
        let child = chart.add_indicator(Box::new(SMA::new(1)));
        let source = crate::studies::StudySource::Study {
            id: parent.0,
            plot: 0,
        };
        chart.set_indicator_source(child, source).unwrap();
        let ctx = egui::Context::default();
        for frame in 0..10 {
            shapes(&ctx, &mut chart, frame as f64, Vec::new());
        }
        let sink = feed.sink(Timeframe::Hour1);
        let hour: DateTime<chrono::Utc> = "2024-03-15T14:00:00Z".parse().unwrap();
        for frame in 0..10 {
            let close = 200.0 + frame as f64;
            sink.push_bar(Bar::new(hour, 100.0, close, 100.0, close, 1.0));
            shapes(&ctx, &mut chart, 10.0 + frame as f64, Vec::new());
        }
        let (_, study) = chart.indicators().find(|(id, _)| *id == child).unwrap();
        let value = study.values().value(0, study.values().len() - 1);
        assert!(
            value.is_some_and(|value| (406.0 / 3.0..=409.0 / 3.0).contains(&value)),
            "{value:?}, expected the SMA of a close from 206 to 209"
        );
    }

    /// Daily bars of close 50 for the 400 days before 2024-03-15, hourly bars of close 100 up to
    /// 14:00 that day and minute bars closing at 102 and 100 in turn up to 14:00, at 100. Keeps
    /// the sink of every subscription.
    #[derive(Default)]
    struct Live(Mutex<Vec<(Timeframe, Sink)>>);

    impl Live {
        fn sink(&self, timeframe: Timeframe) -> Sink {
            let sinks = self.0.lock().unwrap();
            let found = sinks.iter().find(|(candidate, _)| *candidate == timeframe);
            found.unwrap().1.clone()
        }
    }

    impl Datafeed for Live {
        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            let flat = |time, close| Bar::new(time, close, close, close, close, 1.0);
            let today: DateTime<chrono::Utc> = "2024-03-15T00:00:00Z".parse().unwrap();
            let last = today + Duration::hours(14);
            let bars: Vec<_> = match request.timeframe {
                Timeframe::Day1 => (1..=400)
                    .rev()
                    .map(|day| flat(today - Duration::days(day), 50.0))
                    .collect(),
                Timeframe::Hour1 => (0..500)
                    .map(|hour| flat(last - Duration::hours(499 - hour), 100.0))
                    .collect(),
                _ => (0..5000)
                    .map(|minute| {
                        let close = 100.0 + 2.0 * ((4999 - minute) % 2) as f64;
                        flat(last - Duration::minutes(4999 - minute), close)
                    })
                    .collect(),
            };
            let bars = bars
                .into_iter()
                .filter(|bar| request.before.is_none_or(|before| bar.time < before))
                .collect();
            reply.send(Ok(HistoryPage::new(bars, true)));
        }
        fn subscribe(&self, subscription: Subscription) {
            self.0
                .lock()
                .unwrap()
                .push((subscription.timeframe, subscription.sink));
        }
    }

    /// Sessions before the footprint's first complete bar never extend, so the candle history
    /// before them must not add work to a frame that extends prior levels. The sessions that can
    /// extend still do, also when their histogram is left of the view.
    #[test]
    fn profile_extensions_do_not_scale_with_history_before_the_executions() {
        use crate::data::{TradePage, TradeRequest};
        use crate::model::{ChartType, OrderSide, Tick, TradeProfileSettings};
        fn open(day: i64, minute: i64) -> DateTime<chrono::Utc> {
            "2026-01-07T10:00:00Z"
                .parse::<DateTime<chrono::Utc>>()
                .unwrap()
                + Duration::days(day)
                + Duration::minutes(minute)
        }
        // Each day trades in a row of its own, so no later candle ends a level.
        fn price(day: i64) -> f64 {
            1000.0 + 10.0 * day as f64
        }
        // Sixty one-minute candles a day from 10:00 UTC on `0` days without executions and on
        // four days with one execution per candle, from the middle of the first of them.
        struct Executions(i64);
        impl Datafeed for Executions {
            fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
                reply.send(Ok(Instrument::new(symbol).with_tick_size(1.0)));
            }
            fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
                let bars = (-self.0..4)
                    .flat_map(|day| (0..60).map(move |m| (day, m)))
                    .map(|(day, m)| {
                        let p = price(day);
                        Bar::new(open(day, m), p, p, p, p, 1.0)
                    })
                    .collect();
                reply.send(Ok(HistoryPage::new(bars, true)));
            }
            fn trades(&self, _: TradeRequest, reply: Reply<TradePage>) {
                let trades = (0..4)
                    .flat_map(|day| (0..60).map(move |m| (day, m)))
                    .filter(|&(day, m)| day > 0 || m >= 30)
                    .map(|(day, m)| Tick::new(open(day, m), price(day), 1.0, Some(OrderSide::Buy)))
                    .collect();
                reply.send(Ok(TradePage::new(trades, true)));
            }
        }
        let frame = |ctx: &egui::Context, chart: &mut Chart| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                },
            );
            output.textures_delta.clear();
            output.shapes.len()
        };
        let run = |extend_poc: bool, earlier: i64| {
            let ctx = egui::Context::default();
            let mut chart = ChartBuilder::new()
                .with_feed(Arc::new(Executions(earlier)))
                .with_symbol("PROFILE")
                .with_timeframe(Timeframe::Min1)
                .with_chart_type(ChartType::SessionVolumeProfile)
                .build();
            chart.set_visible_bars(120);
            chart.set_trade_profile_settings(TradeProfileSettings {
                extend_poc,
                ..Default::default()
            });
            for _ in 0..5 {
                frame(&ctx, &mut chart);
            }
            let bars = chart.bars(crate::widget::SeriesId::MAIN);
            assert_eq!(bars.len() as i64, (earlier + 4) * 60);
            // The histogram of the first complete session, the second day, is left of the view.
            let second_day_end = bars.partition_point(|bar| bar.time < open(2, 0));
            let mapping = chart.get_chart_mapping();
            assert!(mapping.idx_to_x(second_day_end - 1) < mapping.rect.left());
            allocations(|| frame(&ctx, &mut chart))
        };
        let (plain_few, plain_shapes) = run(false, 10);
        let (plain_many, _) = run(false, 1000);
        let (extended_few, extended_shapes) = run(true, 10);
        let (extended_many, extended_many_shapes) = run(true, 1000);
        assert!(
            extended_many.saturating_sub(extended_few) <= plain_many.saturating_sub(plain_few),
            "990 earlier days add {} allocations with an extension, from {extended_few}, and {} \
             without, from {plain_few}",
            extended_many as i64 - extended_few as i64,
            plain_many as i64 - plain_few as i64,
        );
        // The second and third days extend; the partial first and the unfinished last do not.
        assert_eq!(extended_shapes, plain_shapes + 2);
        assert_eq!(extended_many_shapes, extended_shapes);
    }
}
