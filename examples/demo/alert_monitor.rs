//! Live alert sources retain their interval and inputs independently of chart navigation and replay.

use super::{DemoApp, DemoSymbol, LiveFeed};
use egui_charts::model::{Alert, AlertCondition, AlertSource, AlertStatus, Timeframe};
use egui_charts::widget::{Chart, SeriesId};
use std::collections::HashMap;
use std::sync::Arc;

pub(super) struct AlertMonitor {
    chart: Chart,
    live: Arc<LiveFeed>,
    studies: HashMap<String, SeriesId>,
    sources: HashMap<String, Arc<LiveFeed>>,
    alert_sources: Vec<AlertSource>,
}

impl AlertMonitor {
    fn new(source: &Chart, feed: &LiveFeed, symbol: &str, alert: &Alert) -> Self {
        let live = Arc::new(feed.fork());
        live.set_symbol(symbol);
        let timeframe = source.timeframe().unwrap_or(Timeframe::Min1);
        let mut chart = egui_charts::ChartBuilder::new()
            .with_symbol(symbol)
            .with_timeframe(timeframe)
            .with_chart_options(source.chart_options().clone())
            .build();
        chart.set_extended_hours(source.extended_hours());
        chart.set_instrument(Some(live.instrument()));
        let ids: Vec<_> = chart.indicators().map(|(id, _)| id).collect();
        for id in ids {
            chart.remove_series(id);
        }
        let mut studies: HashMap<String, SeriesId> = HashMap::new();
        let mut pending: Vec<_> = source
            .indicators()
            .map(|(id, study)| (id, study.clone_box()))
            .collect();
        while !pending.is_empty() {
            let Some(at) = pending.iter().position(|(_, study)| match study.source() {
                Some(egui_charts::studies::StudySource::Study { id, .. }) => {
                    !pending.iter().any(|(key, _)| key.0 == id)
                }
                _ => true,
            }) else {
                break;
            };
            let (id, mut study) = pending.remove(at);
            if let Some(egui_charts::studies::StudySource::Study { id: parent, plot }) =
                study.source()
            {
                let parent = studies
                    .get(&parent.to_string())
                    .map_or(usize::MAX, |id| id.0);
                study
                    .set_input(
                        "source",
                        egui_charts::studies::InputValue::Source(
                            egui_charts::studies::StudySource::Study { id: parent, plot },
                        ),
                    )
                    .expect("copied study source");
            }
            studies.insert(id.0.to_string(), chart.add_indicator(study));
        }
        if source.symbol() == symbol {
            chart.drawings_mut().drawings = source.drawings().drawings.clone();
        }
        let sources = chart
            .indicators()
            .flat_map(|(_, study)| study.data_requests())
            .filter_map(|request| {
                if request.symbol.is_empty()
                    || request.symbol == symbol
                    || !DemoSymbol::catalog()
                        .iter()
                        .any(|spec| spec.symbol == request.symbol)
                {
                    return None;
                }
                let feed = Arc::new(live.fork());
                feed.set_symbol(&request.symbol);
                Some((request.symbol, feed))
            })
            .collect();
        Self {
            chart,
            live,
            studies,
            sources,
            alert_sources: alert_sources(alert).cloned().collect(),
        }
    }

    fn for_symbol(source: &Self, live: &LiveFeed, symbol: &str, alert: &Alert) -> Self {
        let live = live.fork();
        let (session, adjusted) = source.live.view(source.chart.extended_hours());
        live.set_view(session, adjusted);
        let mut next = Self::new(&source.chart, &live, symbol, alert);
        next.studies = source
            .studies
            .iter()
            .filter_map(|(original, copied)| {
                next.studies
                    .get(&copied.0.to_string())
                    .map(|id| (original.clone(), *id))
            })
            .collect();
        next
    }

    fn refresh(&mut self, now: chrono::DateTime<chrono::Utc>, paused: bool) -> std::time::Duration {
        self.live.set_paused(paused);
        let mut delay = self.live.advance(now);
        for feed in self.sources.values() {
            feed.set_paused(paused);
            delay = delay.min(feed.advance(now));
        }
        let timeframe = self.chart.timeframe().unwrap_or(Timeframe::Min1);
        let bars = self.live.snapshot(
            timeframe,
            self.chart.bar_clock(),
            self.chart.extended_hours(),
        );
        let requested: Vec<_> = self
            .chart
            .indicators()
            .flat_map(|(_, study)| study.data_requests())
            .map(|request| {
                let available = request.symbol.is_empty()
                    || request.symbol == self.chart.symbol()
                    || self.sources.contains_key(&request.symbol);
                let feed = self.sources.get(&request.symbol).unwrap_or(&self.live);
                let timeframe = request.timeframe.unwrap_or(timeframe);
                let instrument = feed.instrument();
                let clock = egui_charts::model::BarClock::new(timeframe);
                let clock = match &self.chart.chart_options().time_scale.timezone {
                    egui_charts::config::TimezoneMode::Local => clock.with_local_timezone(),
                    zone => clock.with_timezone(
                        zone.to_tz()
                            .unwrap_or(egui_charts::model::TimezoneId::utc()),
                    ),
                };
                let clock = if let Some(session) = &instrument.session {
                    clock.with_session(
                        instrument
                            .time_zone
                            .clone()
                            .unwrap_or(egui_charts::model::TimezoneId::utc()),
                        session.clone(),
                        self.chart.extended_hours(),
                    )
                } else {
                    clock
                };
                let values = if !available {
                    Vec::new()
                } else if request.metric.as_deref() == Some("open_interest") {
                    feed.metric_snapshot()
                        .into_iter()
                        .map(egui_charts::model::Bar::from)
                        .collect()
                } else if request.metric.is_some() {
                    Vec::new()
                } else {
                    feed.snapshot(timeframe, clock.clone(), self.chart.extended_hours())
                };
                (request, values, clock, instrument, timeframe)
            })
            .collect();
        let sources: Vec<_> = requested
            .iter()
            .map(
                |(request, bars, clock, instrument, timeframe)| egui_charts::studies::StudyData {
                    request: request.clone(),
                    bars,
                    clock: clock.clone(),
                    instrument: Some(instrument),
                    timeframe: *timeframe,
                },
            )
            .collect();
        if self.chart.bars(SeriesId::MAIN) != bars || !requested.is_empty() {
            self.chart.set_data(SeriesId::MAIN, bars.clone());
            let mut pending: Vec<_> = self.chart.indicators().map(|(id, _)| id).collect();
            while !pending.is_empty() {
                let Some(at) = pending.iter().position(|id| {
                    let study = self
                        .chart
                        .indicators()
                        .find(|(key, _)| key == id)
                        .unwrap()
                        .1;
                    match study.source() {
                        Some(egui_charts::studies::StudySource::Study { id, .. }) => {
                            !pending.contains(&SeriesId(id))
                        }
                        _ => true,
                    }
                }) else {
                    break;
                };
                let id = pending.remove(at);
                let source = self
                    .chart
                    .indicators()
                    .find(|(key, _)| *key == id)
                    .unwrap()
                    .1
                    .source();
                let values = match source {
                    Some(egui_charts::studies::StudySource::Study { id, plot }) => self
                        .chart
                        .indicators()
                        .find(|(key, _)| key.0 == id)
                        .and_then(|(_, study)| study.values().columns().get(plot))
                        .cloned(),
                    _ => None,
                };
                let study = self.chart.indicator_mut(id).unwrap();
                if matches!(
                    source,
                    Some(egui_charts::studies::StudySource::Study { .. })
                ) {
                    study.update_source(&bars, 0, values.as_deref().unwrap_or(&[]));
                } else {
                    study.update_data(&bars, 0, &sources);
                }
            }
        }
        delay
    }
}

fn alert_sources(alert: &Alert) -> impl Iterator<Item = &AlertSource> {
    std::iter::once((&alert.source, &alert.condition))
        .chain(
            alert
                .additional_conditions
                .iter()
                .map(|rule| (&rule.source, &rule.condition)),
        )
        .flat_map(|(source, condition)| {
            std::iter::once(source).chain(match condition {
                AlertCondition::Compare { other, .. } => Some(other),
                _ => None,
            })
        })
}

impl DemoApp {
    pub(super) fn monitor_alerts(&mut self, ctx: &egui::Context) {
        let now = chrono::Utc::now();
        self.alert_host.expire(now);
        let alerts = self.alerts.alerts();
        self.alert_monitors
            .retain(|(id, _), _| alerts.iter().any(|alert| alert.id.as_ref() == Some(id)));
        for alert in alerts
            .iter()
            .filter(|alert| alert.status == AlertStatus::Active)
        {
            let Some(id) = &alert.id else { continue };
            if self.alert_monitors.iter().any(|((current, _), monitor)| {
                current == id && monitor.alert_sources.iter().ne(alert_sources(alert))
            }) {
                self.alert_monitors.retain(|(current, _), _| current != id);
            }
            let symbols: Vec<_> = alert.watchlist.as_ref().map_or_else(
                || vec![alert.symbol.clone()],
                |name| {
                    self.watchlist
                        .lists()
                        .iter()
                        .filter(|list| list.name == *name)
                        .flat_map(|list| list.symbols().map(str::to_owned))
                        .collect()
                },
            );
            for symbol in symbols {
                if !DemoSymbol::catalog()
                    .iter()
                    .any(|spec| spec.symbol == symbol)
                {
                    continue;
                }
                let key = (id.clone(), symbol.clone());
                if !self.alert_monitors.contains_key(&key) {
                    let monitor = self
                        .alert_monitors
                        .iter()
                        .find(|((current, _), _)| current == id)
                        .map_or_else(
                            || AlertMonitor::new(&self.chart, &self.live, &symbol, alert),
                            |(_, source)| {
                                AlertMonitor::for_symbol(source, &self.live, &symbol, alert)
                            },
                        );
                    self.alert_monitors.insert(key.clone(), monitor);
                }
                let monitor = self.alert_monitors.get_mut(&key).unwrap();
                if self.replay.is_none()
                    && self.chart.symbol() == symbol
                    && self.chart.timeframe() == monitor.chart.timeframe()
                {
                    monitor.chart.drawings_mut().drawings = self.chart.drawings().drawings.clone();
                }
                let delay = monitor.refresh(now, self.live.paused() && self.replay.is_none());
                if !monitor.live.paused() {
                    ctx.request_repaint_after(delay);
                }
                self.alert_host.evaluate(
                    &monitor.chart,
                    now,
                    Some(id),
                    self.watchlist.lists(),
                    &monitor.studies,
                );
            }
        }
    }
}
