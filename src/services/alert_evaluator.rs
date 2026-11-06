//! Saved alert definitions evaluated without constructing a chart.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};

use super::{AlertData, AlertEngine, AlertFeed};
use crate::data::{Datafeed, FeedError, HistoryRequest};
use crate::model::{
    Alert, AlertId, AlertInputs, AlertSource, Bar, BarClock, Instrument, Timeframe,
};
use crate::studies::{
    ConfiguredStudy, Indicator, IndicatorFactory, StudyData, StudyDataRequest, StudySource,
};

/// History, subscriptions and independent study state for the alert service. Both interactive
/// hosts and headless hosts poll this evaluator with the same saved definitions. Chart navigation
/// and replay never replace its saved study definitions. Dropping it releases all subscriptions.
pub struct AlertEvaluator {
    feed: Arc<dyn Datafeed>,
    runs: HashMap<(AlertId, String, Timeframe), Evaluation>,
    feeds: HashMap<Subject, AlertFeed>,
    #[cfg(feature = "ui")]
    storage: Option<Arc<dyn super::Storage>>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Subject {
    symbol: String,
    timeframe: Timeframe,
    metric: Option<String>,
    timezone: Option<crate::model::TimezoneId>,
    extended_hours: bool,
    conversion: Option<String>,
    count: usize,
    start: Option<DateTime<Utc>>,
    /// Event readers belong to one alert, including when its OHLC inputs match another alert.
    owner: Option<(AlertId, String, Timeframe)>,
    calendar: Option<crate::model::AlertCalendar>,
}

struct Evaluation {
    alert: Alert,
    timeframe: Timeframe,
    inputs: AlertInputs,
    studies: Vec<(usize, Box<dyn Indicator>)>,
    error: Option<String>,
    initialized: bool,
    ready: bool,
    start: Option<DateTime<Utc>>,
}

impl Evaluation {
    fn new(alert: &Alert, timeframe: Timeframe, factory: &IndicatorFactory) -> Self {
        let mut inputs = alert.inputs.as_deref().cloned().unwrap_or_default();
        // A captured span counts bars of the source chart's interval, as in `prepare_alert`.
        if timeframe != alert.timeframe.unwrap_or_default() {
            inputs.start = None;
            inputs.visible = None;
        }
        // `ids` with the saved studies they take as input, transitively.
        let with_inputs = |mut ids: HashSet<usize>| loop {
            let before = ids.len();
            for study in &inputs.studies {
                if !ids.contains(&study.id) {
                    continue;
                }
                for (_, input) in &study.inputs {
                    if let crate::model::InputValue::Source(StudySource::Study { id, .. }) = input {
                        ids.insert(*id);
                    }
                }
            }
            if ids.len() == before {
                break ids;
            }
        };
        let tested = with_inputs(
            alert
                .rules()
                .filter(|(_, _, interval)| *interval == timeframe)
                .flat_map(|(source, condition, _)| {
                    std::iter::once(source).chain(condition.partners().into_iter().flatten())
                })
                .filter_map(|source| match source {
                    AlertSource::Study { id, .. } => id.parse::<usize>().ok(),
                    _ => None,
                })
                .collect(),
        );
        // The fired message comes from the first interval.
        let required = if timeframe == alert.timeframe.unwrap_or_default() {
            with_inputs(tested.iter().copied().chain(alert.plot_studies()).collect())
        } else {
            tested.clone()
        };
        let mut result = Self {
            alert: alert.clone(),
            timeframe,
            start: inputs.start,
            inputs,
            studies: Vec::new(),
            error: None,
            initialized: false,
            ready: false,
        };
        for id in required {
            let restored = (|| {
                let saved = result
                    .inputs
                    .studies
                    .iter()
                    .find(|study| study.id == id)
                    .ok_or_else(|| format!("Alert has no saved inputs for study {id}"))?;
                let study = factory
                    .create_registered(&saved.name)
                    .ok_or_else(|| format!("Unknown alert study: {}", saved.name))?;
                let mut study = ConfiguredStudy::new(study);
                for (id, value) in &saved.inputs {
                    study
                        .set_input(id, value.clone())
                        .map_err(|error| error.to_string())?;
                }
                study.set_timeframe(timeframe);
                Ok::<_, String>((id, Box::new(study) as Box<dyn Indicator>))
            })();
            match restored {
                Ok(study) => result.studies.push(study),
                Err(error) if tested.contains(&id) => result.error = Some(error),
                // A study only the message reads and that cannot be made reads `NaN`, instead of
                // stopping the alert's other studies.
                Err(_) => {}
            }
        }
        result
    }

    fn subject(&self, symbol: &str, request: Option<&StudyDataRequest>) -> Subject {
        Subject {
            symbol: request
                .filter(|request| !request.symbol.is_empty())
                .map_or(symbol, |request| &request.symbol)
                .into(),
            timeframe: request
                .and_then(|request| request.timeframe)
                .unwrap_or(self.timeframe),
            metric: request.and_then(|request| request.metric.clone()),
            timezone: self.inputs.timezone.clone(),
            extended_hours: self.inputs.extended_hours,
            conversion: self.inputs.conversion.clone(),
            count: self.inputs.count.max(1),
            start: self.inputs.start,
            calendar: self.inputs.calendar.clone(),
            owner: self
                .studies
                .iter()
                .any(|(_, study)| !study.order_flow_inputs().is_empty())
                .then(|| {
                    (
                        self.alert.id.clone().unwrap(),
                        symbol.to_owned(),
                        self.timeframe,
                    )
                }),
        }
    }

    fn event_inputs(&self, request: Option<&StudyDataRequest>) -> Vec<crate::data::OrderFlowInput> {
        self.studies
            .iter()
            .filter(|(_, study)| {
                let requests = study.data_requests();
                request.map_or(requests.is_empty(), |request| requests.contains(request))
            })
            .flat_map(|(_, study)| study.order_flow_inputs().iter().cloned())
            .collect()
    }

    fn calculate(
        &mut self,
        bars: &[Bar],
        clock: &BarClock,
        instrument: Option<&Instrument>,
        data: &[StudyData<'_>],
    ) {
        let mut pending: Vec<_> = self.studies.iter().map(|(id, _)| *id).collect();
        while !pending.is_empty() {
            let Some(at) = pending.iter().position(|id| {
                let study = &self.studies.iter().find(|(key, _)| key == id).unwrap().1;
                match study.source() {
                    Some(StudySource::Study { id, .. }) => !pending.contains(&id),
                    _ => true,
                }
            }) else {
                break;
            };
            let id = pending.remove(at);
            let source = self
                .studies
                .iter()
                .find(|(key, _)| *key == id)
                .unwrap()
                .1
                .source();
            let values = match source {
                Some(StudySource::Study { id, plot }) => self
                    .studies
                    .iter()
                    .find(|(key, _)| *key == id)
                    .and_then(|(_, study)| study.values().columns().get(plot))
                    .cloned(),
                _ => None,
            };
            let study = &mut self
                .studies
                .iter_mut()
                .find(|(key, _)| *key == id)
                .unwrap()
                .1;
            study.set_timeframe(self.timeframe);
            study.set_bar_clock(clock);
            study.set_instrument(instrument);
            study.set_visible_range(self.inputs.visible.map_or(0..bars.len(), |(start, end)| {
                bars.partition_point(|bar| bar.time < start)
                    ..bars.partition_point(|bar| bar.time <= end)
            }));
            if matches!(source, Some(StudySource::Study { .. })) {
                study.update_source(bars, 0, values.as_deref().unwrap_or(&[]));
            } else {
                study.update_data(bars, 0, data);
            }
        }
    }

    fn requests(&self, first: DateTime<Utc>) -> Vec<(StudyDataRequest, DateTime<Utc>)> {
        self.studies
            .iter()
            .flat_map(|(_, study)| {
                study.data_requests().into_iter().map(|request| {
                    let start = study.data_start(&request).unwrap_or(first);
                    (request, start)
                })
            })
            .collect()
    }
}

impl Subject {
    fn open(&self, feed: Arc<dyn Datafeed>) -> AlertFeed {
        let mut reader = AlertFeed::new(feed, self.request(), self.metric.clone());
        reader.set_calendar(self.calendar.clone());
        reader
    }

    fn request(&self) -> HistoryRequest {
        let mut request = AlertFeed::history_request(&self.symbol, self.timeframe);
        request.clock = match &self.timezone {
            Some(zone) => request.clock.with_timezone(zone.clone()),
            None => request.clock.with_local_timezone(),
        };
        request.extended_hours = self.extended_hours;
        request.conversion = self.conversion.clone();
        request.count = self.count.max(1000);
        request
    }
}

impl AlertEvaluator {
    /// Connect to the host's live feed. Study constructors come from the factory passed to `poll`.
    pub fn new(feed: Arc<dyn Datafeed>) -> Self {
        Self {
            feed,
            runs: HashMap::new(),
            feeds: HashMap::new(),
            #[cfg(feature = "ui")]
            storage: None,
        }
    }

    /// Follow drawing changes in host storage using the same reader as background price alerts.
    #[cfg(feature = "ui")]
    pub fn set_storage(&mut self, storage: Arc<dyn super::Storage>) {
        self.storage = Some(storage);
    }

    /// Poll live inputs and evaluate ready alerts. With `ui`, `ctx` wakes the host on new data
    /// and follows stored drawings. Headless hosts pass an empty `live` slice.
    /// `live` lends compatible loaded chart data; event studies retain independent streams.
    /// Errors leave affected sources unavailable and are returned for the host to report.
    pub fn poll(
        &mut self,
        engine: &AlertEngine,
        factory: &IndicatorFactory,
        live: &[AlertData<'_>],
        #[cfg(feature = "ui")] ctx: &egui::Context,
        now: DateTime<Utc>,
    ) -> Vec<FeedError> {
        engine.tick(now);
        let mut armed = HashSet::new();
        let mut errors = Vec::new();
        for alert in engine.alerts() {
            let Some(id) = &alert.id else {
                continue;
            };
            for symbol in engine.subjects(id) {
                for timeframe in alert.timeframes() {
                    let key = (id.clone(), symbol.clone(), timeframe);
                    armed.insert(key.clone());
                    match self.runs.get_mut(&key) {
                        // Most polls find the alert unchanged; comparing it copies nothing.
                        Some(run) if run.alert == alert => {}
                        // A firing or a marker option leaves the studies and streams as they are.
                        Some(run) if run.alert.same_monitoring(&alert) => {
                            run.alert.clone_from(&alert)
                        }
                        _ => {
                            // Replacing an event definition must also replace its subscription.
                            self.feeds
                                .retain(|subject, _| subject.owner.as_ref() != Some(&key));
                            self.runs
                                .insert(key.clone(), Evaluation::new(&alert, timeframe, factory));
                        }
                    }
                    let run = self.runs.get_mut(&key).unwrap();
                    if let Some(error) = run.error.take() {
                        errors.push(FeedError::InvalidData(error));
                        run.studies.clear();
                    }
                }
            }
        }
        self.runs.retain(|key, _| armed.contains(key));
        for ((_, symbol, _), run) in &self.runs {
            let subject = run.subject(symbol, None);
            let reader = self.feeds.entry(subject.clone()).or_insert_with(|| {
                let mut reader = subject.open(self.feed.clone());
                if let Some(start) = subject.start {
                    reader.require_start(start);
                }
                reader
            });
            if let Some(start) = run.start {
                reader.require_start(start);
            }
            reader.set_inputs(run.event_inputs(None));
            #[cfg(feature = "ui")]
            if run
                .alert
                .sources()
                .any(|source| matches!(source, AlertSource::Drawing(_)))
                && let Some(storage) = &self.storage
            {
                reader.set_storage(storage.clone());
            }
        }
        for (subject, reader) in &mut self.feeds {
            reader.follow(if subject.owner.is_none() { live } else { &[] });
            errors.extend(reader.poll(
                #[cfg(feature = "ui")]
                ctx,
                now,
            ));
        }
        for ((_, symbol, _), run) in &mut self.runs {
            run.ready = false;
            let subject = run.subject(symbol, None);
            let reader = &self.feeds[&subject];
            let all = reader.bars();
            let bars =
                &all[all.partition_point(|bar| run.start.is_some_and(|start| bar.time < start))..];
            let Some(first) = bars.first() else {
                continue;
            };
            run.start.get_or_insert(first.time);
            if !run.initialized {
                // Establish the main span for data_start; these outputs are never evaluated.
                run.calculate(bars, reader.clock(), reader.instrument(), &[]);
                run.initialized = true;
            }
            let requests = run.requests(first.time);
            for (request, start) in &requests {
                let subject = run.subject(symbol, Some(request));
                let reader = self
                    .feeds
                    .entry(subject.clone())
                    .or_insert_with(|| subject.open(self.feed.clone()));
                reader.require_start(*start);
                let mut inputs = run.event_inputs(Some(request));
                if subject == run.subject(symbol, None) {
                    inputs.extend(run.event_inputs(None));
                }
                reader.set_inputs(inputs);
            }
            if requests
                .iter()
                .any(|(request, _)| !self.feeds[&run.subject(symbol, Some(request))].ready())
            {
                continue;
            }
            let reader = &self.feeds[&subject];
            let all = reader.bars();
            let bars =
                &all[all.partition_point(|bar| run.start.is_some_and(|start| bar.time < start))..];
            let data: Vec<_> = requests
                .iter()
                .map(|(request, _)| {
                    let reader = &self.feeds[&run.subject(symbol, Some(request))];
                    StudyData {
                        request: request.clone(),
                        bars: reader.bars(),
                        clock: reader.clock().clone(),
                        instrument: reader.instrument(),
                        timeframe: run.subject(symbol, Some(request)).timeframe,
                    }
                })
                .collect();
            run.calculate(bars, reader.clock(), reader.instrument(), &data);
            // data_start may change after calculation, for example when a profile's span moves.
            let reaches = run.requests(bars[0].time);
            for (request, start) in &reaches {
                let subject = run.subject(symbol, Some(request));
                self.feeds
                    .entry(subject.clone())
                    .or_insert_with(|| subject.open(self.feed.clone()))
                    .require_start(*start);
            }
            if reaches
                .iter()
                .any(|(request, _)| !self.feeds[&run.subject(symbol, Some(request))].ready())
            {
                continue;
            }
            run.ready = true;
        }
        let mut subjects: HashMap<_, Vec<_>> = HashMap::new();
        for ((id, symbol, _), run) in &self.runs {
            let reader = &self.feeds[&run.subject(symbol, None)];
            let from = reader
                .bars()
                .partition_point(|bar| run.start.is_some_and(|start| bar.time < start));
            subjects
                .entry((id, symbol))
                .or_default()
                .push((run, reader, from));
        }
        for ((id, symbol), contexts) in subjects {
            let inputs: Vec<_> = contexts
                .iter()
                .map(|(run, reader, from)| {
                    (
                        reader.clock(),
                        if run.ready {
                            &reader.bars()[*from..]
                        } else {
                            &[]
                        },
                    )
                })
                .collect();
            let exchange = contexts
                .iter()
                .find_map(|(_, reader, _)| reader.instrument()?.exchange.as_deref());
            let value = |timeframe: Timeframe, source: &AlertSource, at: usize| {
                let (run, reader, from) = contexts
                    .iter()
                    .find(|(run, _, _)| run.timeframe == timeframe)?;
                let bars = &reader.bars()[*from..];
                match source {
                    AlertSource::Study { id, output } => run
                        .studies
                        .iter()
                        .find(|(key, _)| key.to_string() == *id)
                        .and_then(|(_, study)| study.plot_value(*output, at)),
                    #[cfg(feature = "ui")]
                    AlertSource::Drawing(_) => {
                        let scale = if run.inputs.logarithmic {
                            crate::scales::PriceScaleMode::Logarithmic
                        } else {
                            crate::scales::PriceScaleMode::Normal
                        };
                        let holder = live
                            .iter()
                            .filter(|data| {
                                reader.matches_view(data)
                                    && data
                                        .drawings
                                        .iter()
                                        .any(|drawing| drawing.alert_level(source).is_some())
                            })
                            .min_by_key(|data| !reader.matches(data));
                        match holder {
                            Some(data) => data.drawings.iter().find_map(|drawing| {
                                let level = drawing.alert_level(source)?;
                                let index = crate::model::index_at_with_timeframe(
                                    data.bars,
                                    bars[at].time,
                                    Some(data.request.timeframe),
                                    reader
                                        .instrument()
                                        .and_then(|instrument| instrument.time_zone.as_ref()),
                                )?;
                                drawing
                                    .alert_levels_with_bars(index, scale, data.bars)
                                    .into_iter()
                                    .find(|(index, ..)| *index == level)
                                    .map(|(.., value)| value)
                            }),
                            // Removed from its open chart, whatever storage still holds.
                            None if live.iter().any(|data| reader.matches(data)) => None,
                            None => reader.drawing_value(source, *from + at, scale),
                        }
                    }
                    _ => None,
                }
            };
            engine.update_matching(Some(id), symbol, exchange, &inputs, now, value);
        }
        // Once alerts and removed watchlist members release their readers in the same frame.
        self.runs
            .retain(|(id, symbol, _), _| engine.subjects(id).contains(symbol));
        let active: HashSet<_> = self
            .runs
            .iter()
            .flat_map(|((_, symbol, _), run)| {
                std::iter::once(run.subject(symbol, None)).chain(
                    run.studies
                        .iter()
                        .flat_map(|(_, study)| study.data_requests())
                        .map(|request| run.subject(symbol, Some(&request))),
                )
            })
            .collect();
        self.feeds.retain(|subject, _| active.contains(subject));
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{HistoryPage, Reply, Subscription, SubscriptionId};
    use crate::model::AlertStatus;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Feed {
        requests: Mutex<Vec<HistoryRequest>>,
        subscriptions: Mutex<Vec<Subscription>>,
        stopped: Mutex<Vec<SubscriptionId>>,
    }

    fn bar(minute: i64, close: f64) -> Bar {
        let time = DateTime::from_timestamp(minute * 60, 0).unwrap();
        Bar::new(time, close, close, close, close, 10.0)
    }

    impl Datafeed for Feed {
        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            let bars = if request.timeframe == Timeframe::Min5 {
                vec![bar(-5, 50.0), bar(0, 150.0)]
            } else {
                vec![bar(-1, 200.0), bar(0, 300.0)]
            };
            self.requests.lock().unwrap().push(request);
            reply.send(Ok(HistoryPage::new(bars, true)));
        }

        fn subscribe(&self, subscription: Subscription) {
            self.subscriptions.lock().unwrap().push(subscription);
        }

        fn unsubscribe(&self, id: SubscriptionId) {
            self.stopped.lock().unwrap().push(id);
        }
    }

    /// A market with more history than any request reaches.
    #[derive(Default)]
    struct DeepFeed(Mutex<Vec<Timeframe>>);

    impl Datafeed for DeepFeed {
        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            self.0.lock().unwrap().push(request.timeframe);
            let step = request.timeframe.duration_ms() / 60_000;
            let last = request
                .before
                .map_or(0, |before| before.timestamp() / 60 - step);
            let bars = (0..request.count as i64)
                .rev()
                .map(|n| bar(last - n * step, 100.0))
                .collect();
            reply.send(Ok(HistoryPage::new(bars, false)));
        }

        fn subscribe(&self, _: Subscription) {}

        fn unsubscribe(&self, _: SubscriptionId) {}
    }

    #[test]
    fn saved_start_reaches_back_only_on_the_source_interval() {
        let feed = Arc::new(DeepFeed::default());
        let mut evaluator = AlertEvaluator::new(feed.clone());
        let mut alert: Alert = serde_json::from_value(serde_json::json!({
            "symbol":"SYM", "timeframe":"Min5", "condition":{"Above":1000.0},
            "additional_timeframes":["Min1"],
            "additional_conditions":[{"source":"Volume","condition":{"Above":1000.0}}]
        }))
        .unwrap();
        // 6000 minutes needs a second page of 1000 five-minute bars and six of one-minute bars.
        alert.inputs = Some(Box::new(AlertInputs {
            start: Some(bar(-6000, 0.0).time),
            ..Default::default()
        }));
        let engine = AlertEngine::new().with_alerts(vec![alert]);
        #[cfg(feature = "ui")]
        let ctx = egui::Context::default();
        for _ in 0..8 {
            evaluator.poll(
                &engine,
                &IndicatorFactory::new(),
                &[],
                #[cfg(feature = "ui")]
                &ctx,
                DateTime::from_timestamp(1000, 0).unwrap(),
            );
        }
        let pages = feed.0.lock().unwrap();
        let count = |timeframe| pages.iter().filter(|tf| **tf == timeframe).count();
        assert_eq!(count(Timeframe::Min5), 2, "the source chart's span");
        assert_eq!(count(Timeframe::Min1), 1, "only the initial bar count");
    }

    #[test]
    fn interval_feeds_calculate_saved_studies_independently_and_release_together() {
        let feed = Arc::new(Feed::default());
        let mut evaluator = AlertEvaluator::new(feed.clone());
        let mut factory = IndicatorFactory::new();
        factory.register("Pair mean", || Box::new(crate::studies::SMA::new(2)));
        // Means are (50+150)/2=100 on five minutes and (200+300)/2=250 on one minute.
        let mut alert: Alert = serde_json::from_value(serde_json::json!({
            "symbol":"SYM", "timeframe":"Min5", "source":{"Study":{"id":"7","output":0}},
            "condition":{"Above":90.0}, "additional_timeframes":["Min1"],
            "additional_conditions":[{"source":{"Study":{"id":"7","output":0}},"condition":{"Below":110.0}}]
        })).unwrap();
        alert.message = "{{value}} on {{timeframe}}".into();
        alert.inputs = Some(Box::new(AlertInputs {
            studies: vec![crate::model::AlertStudy {
                id: 7,
                name: "Pair mean".into(),
                inputs: Vec::new(),
            }],
            ..Default::default()
        }));
        #[cfg(feature = "ui")]
        let drawing_bars = [bar(-15, 80.0), bar(0, 90.0)];
        #[cfg(feature = "ui")]
        let drawing = {
            use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
            let mut drawing = Drawing::new(9, DrawingToolType::TrendLine);
            drawing.completed = true;
            drawing.extend_right = true;
            drawing.chart_points = vec![ChartPoint::new(0.0, 80.0), ChartPoint::new(1.0, 90.0)];
            drawing
        };
        #[cfg(feature = "ui")]
        {
            alert.additional_conditions.push(crate::model::AlertRule {
                source: drawing.alert_source(0),
                condition: crate::model::AlertCondition::Below(95.0),
            });
            let mut stored = serde_json::to_value(&alert).unwrap();
            stored["additional_timeframes"] = serde_json::json!(["Min1", "Min1"]);
            alert = serde_json::from_value(stored).unwrap();
        }
        #[cfg(feature = "ui")]
        // The first chart of this symbol lacks the drawing and must not hide the chart holding it.
        let live = [
            AlertData {
                request: AlertFeed::history_request("SYM", Timeframe::Hour1),
                bars: &[],
                history_complete: true,
                drawings: &[],
            },
            AlertData {
                request: AlertFeed::history_request("SYM", Timeframe::Min15),
                bars: &drawing_bars,
                history_complete: true,
                drawings: std::slice::from_ref(&drawing),
            },
        ];
        #[cfg(not(feature = "ui"))]
        let live: [AlertData<'_>; 0] = [];
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let sink = delivered.clone();
        let engine = AlertEngine::new()
            .with_alerts(vec![alert])
            .on_trigger(move |alert, event| {
                sink.lock().unwrap().push((alert.clone(), event.clone()));
            });
        #[cfg(feature = "ui")]
        let ctx = egui::Context::default();
        let poll = |evaluator: &mut AlertEvaluator| {
            assert!(
                evaluator
                    .poll(
                        &engine,
                        &factory,
                        &live,
                        #[cfg(feature = "ui")]
                        &ctx,
                        DateTime::from_timestamp(1000, 0).unwrap()
                    )
                    .is_empty()
            );
        };
        for _ in 0..6 {
            poll(&mut evaluator);
        }
        assert_eq!(engine.alerts()[0].status, AlertStatus::Active);
        let requests = feed.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| request.symbol == "SYM"));
        assert!(
            requests
                .iter()
                .any(|request| request.timeframe == Timeframe::Min1)
        );
        assert!(
            requests
                .iter()
                .any(|request| request.timeframe == Timeframe::Min5)
        );
        drop(requests);
        let push = |timeframe, minute, close| {
            feed.subscriptions
                .lock()
                .unwrap()
                .iter()
                .find(|subscription| subscription.timeframe == timeframe)
                .unwrap()
                .sink
                .push_bar(bar(minute, close));
        };
        // Make the first processed clause false and the other true, then swap both in one poll.
        let first = evaluator
            .runs
            .values()
            .next()
            .unwrap()
            .subject("SYM", None)
            .timeframe;
        if first == Timeframe::Min5 {
            push(Timeframe::Min5, 0, 30.0); // (50+30)/2=40, below the primary threshold.
            poll(&mut evaluator);
            push(Timeframe::Min1, 1, -140.0); // (300-140)/2=80, below the secondary threshold.
            poll(&mut evaluator);
        }
        push(
            Timeframe::Min5,
            0,
            if first == Timeframe::Min5 {
                150.0
            } else {
                30.0
            },
        );
        push(
            Timeframe::Min1,
            1,
            if first == Timeframe::Min5 {
                200.0
            } else {
                -140.0
            },
        );
        poll(&mut evaluator);
        assert_eq!(
            engine.alerts()[0].status,
            AlertStatus::Active,
            "a poll must not combine old and new snapshots"
        );
        assert!(delivered.lock().unwrap().is_empty());

        // Final means are 100 and 80. The live line at minute 1 is 90+10/15, below 95;
        // evaluating it in one-minute coordinates would incorrectly produce 100.
        if first == Timeframe::Min5 {
            push(Timeframe::Min1, 1, -140.0);
        } else {
            push(Timeframe::Min5, 0, 150.0);
        }
        poll(&mut evaluator);
        assert_eq!(engine.alerts()[0].status, AlertStatus::Triggered);
        assert_eq!(feed.stopped.lock().unwrap().len(), 2);
        assert!(
            feed.subscriptions
                .lock()
                .unwrap()
                .iter()
                .all(|subscription| !subscription.sink.is_active())
        );
        let delivered = delivered.lock().unwrap();
        assert_eq!(delivered.len(), 1);
        assert_eq!(
            serde_json::to_value(&delivered[0].0).unwrap()["additional_timeframes"],
            serde_json::json!(vec!["Min1"; if cfg!(feature = "ui") { 2 } else { 1 }])
        );
        assert!(
            matches!(&delivered[0].1, crate::services::AlertEvent::TriggeredForSymbol { message, .. } if message == "100 on 5min")
        );
    }

    /// Polls `alert` on `feed` until it settles and returns the messages it fired with.
    fn fired_messages(
        feed: Arc<dyn Datafeed>,
        factory: &IndicatorFactory,
        alert: Alert,
    ) -> Vec<String> {
        let mut evaluator = AlertEvaluator::new(feed);
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let sink = delivered.clone();
        let engine = AlertEngine::new()
            .with_alerts(vec![alert])
            .on_trigger(move |_, event| {
                if let crate::services::AlertEvent::TriggeredForSymbol { message, .. } = event {
                    sink.lock().unwrap().push(message.clone());
                }
            });
        #[cfg(feature = "ui")]
        let ctx = egui::Context::default();
        for _ in 0..6 {
            let errors = evaluator.poll(
                &engine,
                factory,
                &[],
                #[cfg(feature = "ui")]
                &ctx,
                DateTime::from_timestamp(1000, 0).unwrap(),
            );
            assert!(errors.is_empty(), "{errors:?}");
        }
        delivered.lock().unwrap().clone()
    }

    /// `prepare_alert` saves every study of the chart with the alert; a price alert's message
    /// names study 7, which no condition tests. One-minute closes 200 and 300: the mean is 250.
    /// Study 8 is not saved, the factory does not know study 9, study 11 reads study 12, which
    /// the factory does not know either, and nothing names study 10.
    #[test]
    fn plot_placeholder_reads_a_saved_study_no_condition_tests() {
        let mut factory = IndicatorFactory::new();
        factory.register("Pair mean", || Box::new(crate::studies::SMA::new(2)));
        factory.register("Unused", || panic!("a study nothing names is not built"));
        let saved = |id, name: &str| crate::model::AlertStudy {
            id,
            name: name.into(),
            inputs: Vec::new(),
        };
        let mut alert = Alert::new("SYM", crate::model::AlertCondition::Above(250.0));
        alert.timeframe = Some(Timeframe::Min1);
        let mut on_study_12 = saved(11, "Pair mean");
        on_study_12.inputs.push((
            "source".into(),
            crate::model::InputValue::Source(StudySource::Study { id: 12, plot: 0 }),
        ));
        alert.message =
            "{{close}} mean {{plot_7_0}} {{plot_8_0}} {{plot_9_0}} {{plot_11_0}}".into();
        alert.inputs = Some(Box::new(AlertInputs {
            studies: vec![
                saved(7, "Pair mean"),
                saved(9, "Unregistered"),
                saved(10, "Unused"),
                on_study_12,
                saved(12, "Unregistered"),
            ],
            ..Default::default()
        }));
        assert_eq!(
            fired_messages(Arc::new(Feed::default()), &factory, alert),
            ["300 mean 250 NaN NaN NaN"]
        );
    }

    /// A feed that keys symbols without a venue prefix and reports the exchange when it
    /// resolves them, as the `SymbolSearch` example and the demo do.
    struct Listed(Feed);

    impl Datafeed for Listed {
        fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
            reply.send(Ok(Instrument::new(symbol).with_exchange("NASDAQ")));
        }

        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            self.0.history(request, reply);
        }

        fn subscribe(&self, subscription: Subscription) {
            self.0.subscribe(subscription);
        }

        fn unsubscribe(&self, id: SubscriptionId) {
            self.0.unsubscribe(id);
        }
    }

    #[test]
    fn exchange_placeholder_reads_the_resolved_instrument_exchange() {
        let mut alert = Alert::new("AAPL", crate::model::AlertCondition::Above(250.0));
        alert.timeframe = Some(Timeframe::Min1);
        alert.message = "{{exchange}}:{{ticker}} closed at {{close}}".into();
        assert_eq!(
            fired_messages(
                Arc::new(Listed(Feed::default())),
                &IndicatorFactory::new(),
                alert
            ),
            ["NASDAQ:AAPL closed at 300"]
        );
    }

    /// A repeating study alert: one study 7 named `name` on one minute, `Above(100)`.
    fn repeating_alert(name: &str) -> Alert {
        let mut alert: Alert = serde_json::from_value(serde_json::json!({
            "symbol":"SYM", "timeframe":"Min1", "source":{"Study":{"id":"7","output":0}},
            "condition":{"Above":100.0}, "frequency":"EveryTime"
        }))
        .unwrap();
        alert.inputs = Some(Box::new(AlertInputs {
            studies: vec![crate::model::AlertStudy {
                id: 7,
                name: name.into(),
                inputs: Vec::new(),
            }],
            ..Default::default()
        }));
        alert
    }

    #[test]
    fn repeating_alert_keeps_its_studies_after_firing() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let feed = Arc::new(Feed::default());
        let mut evaluator = AlertEvaluator::new(feed.clone());
        let built = Arc::new(AtomicUsize::new(0));
        let count = built.clone();
        let mut factory = IndicatorFactory::new();
        factory.register("Pair mean", move || {
            count.fetch_add(1, Ordering::Relaxed);
            Box::new(crate::studies::SMA::new(2))
        });
        // The one-minute mean (200+300)/2=250 is above 100 on the first evaluation.
        let engine = AlertEngine::new().with_alerts(vec![repeating_alert("Pair mean")]);
        #[cfg(feature = "ui")]
        let ctx = egui::Context::default();
        for _ in 0..4 {
            evaluator.poll(
                &engine,
                &factory,
                &[],
                #[cfg(feature = "ui")]
                &ctx,
                DateTime::from_timestamp(1000, 0).unwrap(),
            );
        }
        assert!(engine.alerts()[0].last_triggered_at.is_some(), "fired");
        assert_eq!(engine.alerts()[0].status, crate::model::AlertStatus::Active);
        // Turning the line across the chart off changes nothing the alert monitors.
        let mut edited = engine.alerts()[0].clone();
        edited.extend_line = false;
        let inbox = Arc::new(crate::data::inbox::Inbox::default());
        crate::services::Alerts::save(&engine, edited, inbox.reply(0));
        evaluator.poll(
            &engine,
            &factory,
            &[],
            #[cfg(feature = "ui")]
            &ctx,
            DateTime::from_timestamp(1000, 0).unwrap(),
        );
        assert!(!engine.alerts()[0].extend_line);
        assert_eq!(built.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn display_only_edit_keeps_the_order_flow_stream() {
        use crate::services::Alerts;
        let feed = Arc::new(Feed::default());
        let mut evaluator = AlertEvaluator::new(feed.clone());
        let mut factory = IndicatorFactory::new();
        factory.register("Spread", || {
            Box::new(crate::studies::OrderFlowStudy::new(
                crate::data::OrderFlowKind::Spread,
            ))
        });
        let engine = AlertEngine::new().with_alerts(vec![repeating_alert("Spread")]);
        #[cfg(feature = "ui")]
        let ctx = egui::Context::default();
        let poll = |evaluator: &mut AlertEvaluator| {
            evaluator.poll(
                &engine,
                &factory,
                &[],
                #[cfg(feature = "ui")]
                &ctx,
                DateTime::from_timestamp(1000, 0).unwrap(),
            );
        };
        for _ in 0..3 {
            poll(&mut evaluator);
        }
        assert_eq!(feed.requests.lock().unwrap().len(), 1);
        assert_eq!(feed.subscriptions.lock().unwrap().len(), 1);
        // Turning the line across the chart off changes no condition.
        let mut edited = engine.alerts()[0].clone();
        edited.extend_line = false;
        let inbox = Arc::new(crate::data::inbox::Inbox::default());
        engine.save(edited, inbox.reply(0));
        for _ in 0..3 {
            poll(&mut evaluator);
        }
        assert!(!engine.alerts()[0].extend_line);
        // Release the locks before asserting: dropping the evaluator unsubscribes through them.
        let counts = (
            feed.stopped.lock().unwrap().len(),
            feed.requests.lock().unwrap().len(),
            feed.subscriptions.lock().unwrap().len(),
        );
        assert_eq!(
            counts,
            (0, 1, 1),
            "(unsubscribed, history requests, subscriptions)"
        );
        // A changed condition is a new evaluation with its own stream.
        let mut edited = engine.alerts()[0].clone();
        edited.condition = crate::model::AlertCondition::Above(200.0);
        engine.save(edited, inbox.reply(1));
        for _ in 0..3 {
            poll(&mut evaluator);
        }
        let counts = (
            feed.stopped.lock().unwrap().len(),
            feed.requests.lock().unwrap().len(),
            feed.subscriptions.lock().unwrap().len(),
        );
        assert_eq!(counts, (1, 2, 2));
    }
}
