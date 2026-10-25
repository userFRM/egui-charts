//! Host-fed histories and subscriptions used by studies.

use super::Chart;
use crate::data::inbox::Inbox;
use crate::data::merge::{Origin, merge};
use crate::data::{Datafeed, FeedError, HistoryPage, HistoryRequest, Subscription, SubscriptionId};
use crate::model::{Bar, BarClock, Instrument, Timeframe};
use crate::studies::{StudyData, StudyDataRequest};
use std::collections::BTreeSet;
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
    from_ticks: BTreeSet<chrono::DateTime<chrono::Utc>>,
}

struct Stream {
    feed: Arc<dyn Datafeed>,
    inbox: Arc<Inbox>,
    subscription: Option<SubscriptionId>,
    pending: bool,
    end: bool,
    wake: Arc<Mutex<Option<egui::ViewportId>>>,
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
        symbol: if request.symbol.is_empty() {
            symbol.to_owned()
        } else {
            request.symbol.clone()
        },
        timeframe: Some(request.timeframe.or(timeframe).unwrap_or_default()),
    }
}

fn clock(timeframe: Timeframe, instrument: Option<&Instrument>, extended: bool) -> BarClock {
    let mut clock = BarClock::new(timeframe);
    if let Some(instrument) = instrument {
        let zone = instrument
            .time_zone
            .clone()
            .unwrap_or_else(crate::model::TimezoneId::utc);
        if timeframe.duration_ms() < Timeframe::Day1.duration_ms() {
            clock = clock.with_timezone(zone.tz());
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
            if let Some(stream) = &mut series.stream
                && let Some(id) = stream.subscription.take()
            {
                stream.inbox.unsubscribe(id);
                stream.feed.unsubscribe(id);
            }
        }
    }

    pub(super) fn data(
        &self,
        requests: Vec<StudyDataRequest>,
        symbol: &str,
        timeframe: Option<Timeframe>,
    ) -> Vec<StudyData<'_>> {
        requests
            .into_iter()
            .filter_map(|request| {
                let key = resolved(&request, symbol, timeframe);
                let series = self.series.iter().find(|series| series.request == key)?;
                Some(StudyData {
                    request,
                    bars: &series.bars,
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
        let clock = clock(
            request.timeframe.unwrap(),
            instrument.as_ref(),
            self.extended_hours(),
        );
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
            from_ticks: BTreeSet::new(),
        });
        self.invalidate_study_data();
    }

    /// The last error for a requested study history. An unavailable source leaves gaps.
    pub fn study_data_error(&self, request: &StudyDataRequest) -> Option<&FeedError> {
        let request = resolved(request, self.symbol(), self.timeframe());
        self.study_data
            .series
            .iter()
            .find(|series| series.request == request)?
            .error
            .as_ref()
    }

    fn invalidate_study_data(&mut self) {
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
                feed.resolve(&request.symbol, inbox.reply(0));
                self.study_data.series.push(Series {
                    request: request.clone(),
                    bars: Vec::new(),
                    instrument: None,
                    clock: BarClock::new(request.timeframe.unwrap()),
                    error: None,
                    from_ticks: BTreeSet::new(),
                    stream: Some(Stream {
                        feed: Arc::clone(feed),
                        inbox,
                        subscription: None,
                        pending: true,
                        end: false,
                        wake,
                    }),
                });
            }
        }
        let extended = self.extended_hours();
        let first = self.state.data().bars.first().map(|bar| bar.time);
        let count = self.state.data().bars.len().max(300);
        let mut changed = removed || self.study_data.series.len() != previous;
        for series in &mut self.study_data.series {
            let Some(stream) = &mut series.stream else {
                let clock = clock(
                    series.request.timeframe.unwrap(),
                    series.instrument.as_ref(),
                    extended,
                );
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
                stream.pending = false;
                if key == 0 {
                    let result = answer
                        .downcast::<Result<Instrument, FeedError>>()
                        .map(|result| *result)
                        .unwrap_or(Err(FeedError::NotSupported));
                    match result {
                        Ok(instrument) => {
                            let tf = series.request.timeframe.unwrap();
                            if instrument
                                .supported_timeframes
                                .as_ref()
                                .is_some_and(|intervals| !intervals.contains(&tf))
                            {
                                series.error = Some(FeedError::NotSupported);
                                continue;
                            }
                            series.clock = clock(tf, Some(&instrument), extended);
                            series.instrument = Some(instrument);
                        }
                        Err(error) => series.error = Some(error),
                    }
                } else {
                    let result = answer
                        .downcast::<Result<HistoryPage, FeedError>>()
                        .map(|result| *result)
                        .unwrap_or(Err(FeedError::NotSupported));
                    match result {
                        Ok(page) => {
                            let previous = series.bars.first().map(|bar| bar.time);
                            stream.end = page.end_of_history || page.bars.is_empty();
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
                                && series.bars.first().map(|bar| bar.time) >= previous
                            {
                                stream.end = true;
                            }
                            changed = true;
                        }
                        Err(error) => series.error = Some(error),
                    }
                }
            }
            for (_, updates) in drained.updates {
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
                );
                stream.subscription = Some(sink.id);
                stream.feed.subscribe(Subscription {
                    id: sink.id,
                    symbol: series.request.symbol.clone(),
                    timeframe: tf,
                    clock: series.clock.clone(),
                    extended_hours: extended,
                    sink,
                });
            }
            if !stream.pending
                && !stream.end
                && series.error.is_none()
                && series.instrument.is_some()
                && (series.bars.is_empty()
                    || first
                        .zip(series.bars.first())
                        .is_some_and(|(first, bar)| first < bar.time))
            {
                stream.pending = true;
                stream.feed.history(
                    HistoryRequest {
                        symbol: series.request.symbol.clone(),
                        timeframe: series.request.timeframe.unwrap(),
                        clock: series.clock.clone(),
                        before: series.bars.first().map(|bar| bar.time),
                        count,
                        extended_hours: extended,
                    },
                    stream.inbox.reply(1),
                );
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
    }
    impl Datafeed for Feed {
        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            let (prices, minutes) = match request.symbol.as_str() {
                "REMOTE" => (vec![10.0, 14.0, 20.0], 60),
                "OTHER" => (vec![100.0, 200.0, 300.0], 60),
                _ => (vec![1.0; 5], 30),
            };
            reply.send(Ok(HistoryPage {
                bars: bars(&prices, minutes),
                end_of_history: true,
            }));
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
        let request = chart.study_data_requests().pop().unwrap();
        chart.set_study_data(request, bars(&[2.0, 4.0, 8.0], 60), None);
        settle(&mut chart);
        assert_eq!(output(&chart)[4], Some(6.0));
    }
}
