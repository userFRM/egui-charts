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
    runs: HashMap<(AlertId, String), Evaluation>,
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
    owner: Option<(AlertId, String)>,
    calendar: Option<crate::model::AlertCalendar>,
}

struct Evaluation {
    alert: Alert,
    inputs: AlertInputs,
    studies: Vec<(usize, Box<dyn Indicator>)>,
    error: Option<String>,
    initialized: bool,
    start: Option<DateTime<Utc>>,
}

impl Evaluation {
    fn new(alert: &Alert, factory: &IndicatorFactory) -> Self {
        let inputs = alert.inputs.as_deref().cloned().unwrap_or_default();
        let mut required: HashSet<_> = alert
            .sources()
            .filter_map(|source| match source {
                AlertSource::Study { id, .. } => id.parse::<usize>().ok(),
                _ => None,
            })
            .collect();
        loop {
            let before = required.len();
            for study in &inputs.studies {
                if !required.contains(&study.id) {
                    continue;
                }
                for (_, input) in &study.inputs {
                    if let crate::model::InputValue::Source(StudySource::Study { id, .. }) = input {
                        required.insert(*id);
                    }
                }
            }
            if required.len() == before {
                break;
            }
        }
        let mut result = Self {
            alert: alert.clone(),
            start: inputs.start,
            inputs,
            studies: Vec::new(),
            error: None,
            initialized: false,
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
                    .create(&saved.name)
                    .ok_or_else(|| format!("Unknown alert study: {}", saved.name))?;
                let mut study = ConfiguredStudy::new(study);
                for (id, value) in &saved.inputs {
                    study.set_input(id, value.clone()).map_err(str::to_owned)?;
                }
                study.set_timeframe(alert.timeframe.unwrap_or_default());
                Ok::<_, String>((id, Box::new(study) as Box<dyn Indicator>))
            })();
            match restored {
                Ok(study) => result.studies.push(study),
                Err(error) => result.error = Some(error),
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
                .or(self.alert.timeframe)
                .unwrap_or_default(),
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
                .then(|| (self.alert.id.clone().unwrap(), symbol.to_owned())),
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
            study.set_timeframe(self.alert.timeframe.unwrap_or_default());
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
                    let start = study
                        .data_start(&request)
                        .map_or(first, |start| start.max(first));
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
                let key = (id.clone(), symbol.clone());
                armed.insert(key.clone());
                if self.runs.get(&key).is_none_or(|run| run.alert != alert) {
                    // Replacing an event definition must also replace its subscription.
                    self.feeds
                        .retain(|subject, _| subject.owner.as_ref() != Some(&key));
                    self.runs
                        .insert(key.clone(), Evaluation::new(&alert, factory));
                }
                let run = self.runs.get_mut(&key).unwrap();
                if let Some(error) = run.error.take() {
                    errors.push(FeedError::InvalidData(error));
                    run.studies.clear();
                }
            }
        }
        self.runs.retain(|key, _| armed.contains(key));
        for ((_, symbol), run) in &self.runs {
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
        for ((id, symbol), run) in &mut self.runs {
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
            let reader = &self.feeds[&subject];
            let all = reader.bars();
            let from = all.partition_point(|bar| run.start.is_some_and(|start| bar.time < start));
            let bars = &all[from..];
            engine.update_alert(
                id,
                symbol,
                subject.timeframe,
                bars,
                now,
                |source, at| match source {
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
                        match live.iter().find(|data| reader.matches(data)) {
                            Some(data) => data.drawings.iter().find_map(|drawing| {
                                let level = drawing.alert_level(source)?;
                                let index = crate::model::index_at_with_timeframe(
                                    data.bars,
                                    bars[at].time,
                                    Some(subject.timeframe),
                                    reader
                                        .instrument()
                                        .and_then(|instrument| instrument.time_zone.as_ref()),
                                )?;
                                drawing
                                    .alert_levels(index, scale)
                                    .into_iter()
                                    .find(|(index, ..)| *index == level)
                                    .map(|(.., value)| value)
                            }),
                            None => reader.drawing_value(source, from + at, scale),
                        }
                    }
                    _ => None,
                },
            );
        }
        // Once alerts and removed watchlist members release their readers in the same frame.
        self.runs
            .retain(|(id, symbol), _| engine.subjects(id).contains(symbol));
        let active: HashSet<_> = self
            .runs
            .iter()
            .flat_map(|((_, symbol), run)| {
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
