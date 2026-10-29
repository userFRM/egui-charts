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
            let backoff = std::time::Duration::from_secs(1 << self.failures.min(6))
                .min(std::time::Duration::from_secs(60));
            let delay = match error {
                FeedError::RateLimited {
                    retry_after: Some(after),
                } => backoff.max(*after),
                _ => backoff,
            };
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
    /// Histories required by the current study inputs, with chart defaults resolved. Hosts
    /// without a connected feed supply these through [`Self::set_study_data`]. A connected
    /// feed receives resolve, history and subscription calls automatically on the next frame.
    pub fn study_data_requests(&self) -> Vec<StudyDataRequest> {
        let mut requests = Vec::new();
        for (_, study) in self.indicators() {
            for request in study.data_requests() {
                let request = resolved(&request, self.symbol(), self.timeframe());
                if !requests.contains(&request) {
                    requests.push(request);
                }
            }
        }
        requests
    }

    /// Replace a study history with host-owned historical or streaming bars. Bars are sorted,
    /// deduplicated and validated like chart data. This takes ownership of this request from
    /// the connected feed; the host supplies further replacements. Metadata sets its calendar.
    pub fn set_study_data(
        &mut self,
        request: StudyDataRequest,
        bars: Vec<Bar>,
        instrument: Option<Instrument>,
    ) {
        let request = resolved(&request, self.symbol(), self.timeframe());
        let clock = clock(&request, instrument.as_ref(), self.extended_hours());
        let (bars, _) = super::series::clean(bars, &clock);
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
        self.invalidate_study_data();
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
    }

    pub(super) fn pump_study_data(&mut self, ctx: &egui::Context, feed: Option<Arc<dyn Datafeed>>) {
        let requests = self.study_data_requests();
        let previous = self.study_data.series.len();
        self.study_data.series.retain(|series| {
            requests.contains(&series.request)
                && series.stream.as_ref().is_none_or(|stream| {
                    feed.as_ref()
                        .is_some_and(|feed| Arc::ptr_eq(feed, &stream.feed))
                })
        });
        let removed = self.study_data.series.len() != previous;
        if let Some(feed) = &feed {
            for request in &requests {
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
        let now = ctx.input(|input| input.time);
        let first = self.state.data().bars.first().map(|bar| bar.time);
        let count = self.state.data().bars.len().max(300);
        let mut changed = removed || self.study_data.series.len() != previous;
        for series in &mut self.study_data.series {
            let Some(stream) = &mut series.stream else {
                let clock = clock(&series.request, series.instrument.as_ref(), extended);
                if clock != series.clock {
                    series.clock = clock;
                    changed = true;
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
                            changed = true;
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
                    changed = true;
                }
                if !updates.bars.is_empty() {
                    merge(
                        &mut series.bars,
                        &mut series.from_ticks,
                        &series.clock,
                        updates.bars.into_values().collect(),
                    );
                    changed = true;
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
            if !stream.pending
                && stream.retry_at.is_none()
                && !stream.end
                && series.error.is_none()
                && series.instrument.is_some()
                && (series.bars.is_empty()
                    || first
                        .zip(series.bars.first())
                        .is_some_and(|(first, bar)| first < bar.time))
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
                    conversion: stream.conversion.clone(),
                });
                stream.request(&series.request);
            }
        }
        if changed {
            self.invalidate_study_data();
        }
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
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
            (0..study.values().len())
                .map(|at| study.values().value(0, at))
                .collect::<Vec<_>>()
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
    }
}
