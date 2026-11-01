//! Alerts: the alerts panel, the alert dialog and headless evaluation on the host's feed.

use super::core::{Direction, SidePanel};
use super::layout::Region;
use super::{Action, ExportContents, Terminal};
use chrono::Utc;
use egui_charts::Chart;
use egui_charts::data::HistoryRequest;
use egui_charts::model::{Alert, AlertCondition, AlertId, AlertSource, BarClock, Timeframe};
use egui_charts::scales::PriceScaleMode;
use egui_charts::services::{AlertFeed, UserSettings};
use egui_charts::studies::{Indicator, StudyData, StudySource};
use egui_charts::ui::Control;
use egui_charts::ui::context_menu::{ChartContextAction, ContextMenuAction};
use egui_charts::ui::dialogs::{
    AlertDialog, AlertDialogAction, DrawingContextMenuAction, SeriesContextMenuAction,
    StudyContextMenuAction,
};
use egui_charts::ui::top_toolbar::TopToolbarAction;
use egui_charts::ui::widget_bar::{AlertPanelAction, AlertsPanel, RightBarIcon, WidgetBarAction};
use egui_charts::widget::SeriesId;
use std::collections::{HashMap, HashSet};

type Subject = (String, Timeframe, Option<String>);

#[derive(Default)]
pub(super) struct State {
    panel: AlertsPanel,
    dialog: AlertDialog,
    definitions: HashMap<AlertId, Definition>,
    feeds: HashMap<Subject, AlertFeed>,
}

/// Studies keep their creation inputs and chart-local ids, including dependencies. They are
/// recalculated from the subject's bars, without building or painting an offscreen chart.
struct Definition {
    sources: Vec<AlertSource>,
    studies: Vec<(SeriesId, Box<dyn Indicator>)>,
    clock: BarClock,
    timeframe: Timeframe,
    scale: PriceScaleMode,
    extended_hours: bool,
    conversion: Option<String>,
    count: usize,
}

fn sources(alert: &Alert) -> Vec<AlertSource> {
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
        .cloned()
        .collect()
}

impl Definition {
    fn new(alert: &Alert, chart: &Chart) -> Self {
        let sources = sources(alert);
        let mut required: HashSet<_> = sources
            .iter()
            .filter_map(|source| match source {
                AlertSource::Study { id, .. } => id.parse::<usize>().ok().map(SeriesId),
                _ => None,
            })
            .collect();
        loop {
            let before = required.len();
            for (id, study) in chart.indicators() {
                if required.contains(&id)
                    && let Some(StudySource::Study { id, .. }) = study.source()
                {
                    required.insert(SeriesId(id));
                }
            }
            if before == required.len() {
                break;
            }
        }
        Self {
            sources,
            // shortcut: stream-backed studies need independent event histories before headless evaluation.
            studies: chart
                .indicators()
                .filter(|(id, study)| required.contains(id) && study.order_flow_inputs().is_empty())
                .map(|(id, study)| (id, study.clone_box()))
                .collect(),
            clock: if chart.timeframe() == alert.timeframe.or(chart.timeframe()) {
                chart.bar_clock()
            } else {
                BarClock::new(alert.timeframe.unwrap_or_default())
            },
            timeframe: alert.timeframe.or(chart.timeframe()).unwrap_or_default(),
            scale: chart.config().right_axis_scale_mode,
            extended_hours: chart.extended_hours(),
            conversion: chart.conversion().map(str::to_owned),
            count: chart.bars(SeriesId::MAIN).len().max(1000),
        }
    }

    fn request(&self, symbol: &str, timeframe: Timeframe) -> HistoryRequest {
        let mut request = AlertFeed::history_request(symbol, timeframe);
        request.clock = if self.timeframe == timeframe {
            self.clock.clone()
        } else {
            BarClock::new(timeframe)
        };
        request.count = self.count;
        request.extended_hours = self.extended_hours;
        request.conversion = self.conversion.clone();
        request
    }

    fn calculate(
        &mut self,
        bars: &[egui_charts::model::Bar],
        clock: &BarClock,
        instrument: Option<&egui_charts::model::Instrument>,
        data: &[StudyData<'_>],
    ) {
        let mut pending: Vec<_> = self.studies.iter().map(|(id, _)| *id).collect();
        while !pending.is_empty() {
            let Some(at) = pending.iter().position(|id| {
                let study = &self.studies.iter().find(|(key, _)| key == id).unwrap().1;
                match study.source() {
                    Some(StudySource::Study { id, .. }) => !pending.contains(&SeriesId(id)),
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
                    .find(|(key, _)| key.0 == id)
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
            if matches!(source, Some(StudySource::Study { .. })) {
                study.update_source(bars, 0, values.as_deref().unwrap_or(&[]));
            } else {
                study.update_data(bars, 0, data);
            }
        }
    }
}

/// A live chart takes over only once it holds data. Replay never supplies alert inputs.
fn live_chart<'a>(charts: &'a [Chart], symbol: &str, timeframe: Timeframe) -> Option<&'a Chart> {
    charts.iter().find(|chart| {
        chart.symbol() == symbol
            && chart.timeframe() == Some(timeframe)
            && chart.replay_time().is_none()
            && chart.instrument().is_some()
            && chart
                .feed_state(SeriesId::MAIN)
                .is_none_or(|feed| !feed.loading_history)
            && !chart.bars(SeriesId::MAIN).is_empty()
    })
}

impl Terminal {
    pub(super) fn alerts_init(&mut self) {
        let available = self.services.alerts.is_some();
        self.set_control_visible(Control::Alerts, available);
        if let Some(service) = &self.services.alerts {
            self.alerts.panel.set_service(service.clone());
        }
        if let Some(storage) = &self.services.storage {
            self.alerts.dialog.set_storage(storage.clone());
        }
    }

    pub(super) fn alerts_poll(&mut self, ctx: &egui::Context) {
        self.alerts.panel.poll(ctx, &mut self.toasts);
        let Some(service) = self.services.alerts.clone() else {
            return;
        };
        let Some(engine) = service.engine() else {
            return;
        };
        let now = Utc::now();
        engine.tick(now);
        let alerts = engine.alerts();
        self.alerts
            .definitions
            .retain(|id, _| alerts.iter().any(|alert| alert.id.as_ref() == Some(id)));
        let mut tasks = Vec::new();
        let mut needed = HashSet::new();
        for alert in &alerts {
            let Some(id) = &alert.id else {
                continue;
            };
            if self
                .alerts
                .definitions
                .get(id)
                .is_none_or(|definition| definition.sources != sources(alert))
            {
                let source = self
                    .charts
                    .iter()
                    .find(|chart| chart.symbol() == alert.symbol)
                    .unwrap_or(&self.charts[self.active]);
                self.alerts
                    .definitions
                    .insert(id.clone(), Definition::new(alert, source));
            }
            let definition = self.alerts.definitions.get_mut(id).unwrap();
            if let Some(timeframe) = alert.timeframe
                && timeframe != definition.timeframe
            {
                definition.timeframe = timeframe;
                definition.clock = BarClock::new(timeframe);
            }
            let definition = &*definition;
            for symbol in engine.subjects(id) {
                let timeframe = definition.timeframe;
                let subject = (symbol.clone(), timeframe, None);
                if live_chart(&self.charts, &symbol, timeframe).is_none() {
                    needed.insert(subject.clone());
                    self.alerts.feeds.entry(subject.clone()).or_insert_with(|| {
                        AlertFeed::new(
                            self.feed.clone(),
                            definition.request(&symbol, timeframe),
                            None,
                        )
                    });
                    if definition
                        .sources
                        .iter()
                        .any(|source| matches!(source, AlertSource::Drawing(_)))
                        && let Some(storage) = &self.services.storage
                    {
                        self.alerts
                            .feeds
                            .get_mut(&subject)
                            .unwrap()
                            .set_storage(storage.clone());
                    }
                }
                for request in definition
                    .studies
                    .iter()
                    .flat_map(|(_, study)| study.data_requests())
                {
                    let other = if request.symbol.is_empty() {
                        symbol.clone()
                    } else {
                        request.symbol
                    };
                    let interval = request.timeframe.unwrap_or(timeframe);
                    if request.metric.is_none()
                        && live_chart(&self.charts, &other, interval).is_some()
                    {
                        continue;
                    }
                    let subject = (other.clone(), interval, request.metric.clone());
                    needed.insert(subject.clone());
                    self.alerts.feeds.entry(subject).or_insert_with(|| {
                        AlertFeed::new(
                            self.feed.clone(),
                            definition.request(&other, interval),
                            request.metric,
                        )
                    });
                }
                tasks.push((id.clone(), symbol, timeframe));
            }
        }
        self.alerts.feeds.retain(|key, _| needed.contains(key));
        for (subject, feed) in &mut self.alerts.feeds {
            for error in feed.poll(ctx, now) {
                self.toasts
                    .error(format!("Alert data for {}: {error}", subject.0));
            }
        }
        for (id, symbol, timeframe) in &tasks {
            let visible = live_chart(&self.charts, symbol, *timeframe);
            let feed = self.alerts.feeds.get(&(symbol.clone(), *timeframe, None));
            let (bars, clock, instrument) = if let Some(chart) = visible {
                (
                    chart.bars(SeriesId::MAIN),
                    chart.bar_clock(),
                    chart.instrument(),
                )
            } else if let Some(feed) = feed {
                (feed.bars(), feed.clock().clone(), feed.instrument())
            } else {
                continue;
            };
            let definition = self.alerts.definitions.get_mut(id).unwrap();
            let data: Vec<_> = definition
                .studies
                .iter()
                .flat_map(|(_, study)| study.data_requests())
                .map(|request| {
                    let other = if request.symbol.is_empty() {
                        symbol
                    } else {
                        &request.symbol
                    };
                    let interval = request.timeframe.unwrap_or(*timeframe);
                    let chart = request
                        .metric
                        .is_none()
                        .then(|| live_chart(&self.charts, other, interval))
                        .flatten();
                    let feed =
                        self.alerts
                            .feeds
                            .get(&(other.clone(), interval, request.metric.clone()));
                    let (bars, clock, instrument) = if let Some(chart) = chart {
                        (
                            chart.bars(SeriesId::MAIN),
                            chart.bar_clock(),
                            chart.instrument(),
                        )
                    } else if let Some(feed) = feed {
                        (feed.bars(), feed.clock().clone(), feed.instrument())
                    } else {
                        (&[][..], BarClock::new(interval), None)
                    };
                    StudyData {
                        request,
                        bars,
                        clock,
                        instrument,
                        timeframe: interval,
                    }
                })
                .collect();
            definition.calculate(bars, &clock, instrument, &data);
            engine.update_alert(
                id,
                symbol,
                *timeframe,
                bars,
                now,
                |source, at| match source {
                    AlertSource::Study { id, output } => definition
                        .studies
                        .iter()
                        .find(|(key, _)| key.0.to_string() == *id)
                        .and_then(|(_, study)| study.plot_value(*output, at)),
                    AlertSource::Drawing(_) => visible
                        .and_then(|chart| chart.alert_value(source, at))
                        .or_else(|| {
                            feed.and_then(|feed| feed.drawing_value(source, at, definition.scale))
                        }),
                    _ => None,
                },
            );
        }
        // Release inputs of alerts that fired once during this frame, without a later repaint.
        let armed: HashSet<_> = tasks
            .iter()
            .filter(|(id, symbol, _)| engine.subjects(id).contains(symbol))
            .flat_map(|(id, symbol, timeframe)| {
                let definition = &self.alerts.definitions[id];
                std::iter::once((symbol.clone(), *timeframe, None)).chain(
                    definition
                        .studies
                        .iter()
                        .flat_map(|(_, study)| study.data_requests())
                        .map(|request| {
                            (
                                if request.symbol.is_empty() {
                                    symbol.clone()
                                } else {
                                    request.symbol
                                },
                                request.timeframe.unwrap_or(*timeframe),
                                request.metric,
                            )
                        }),
                )
            })
            .collect();
        self.alerts.feeds.retain(|key, _| armed.contains(key));
        self.alerts.panel.poll(ctx, &mut self.toasts);
        if alerts
            .iter()
            .any(|alert| alert.status == egui_charts::model::AlertStatus::Active)
        {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }

    pub(super) fn alerts_show(&mut self, ui: &mut egui::Ui, region: Region) {
        match region {
            Region::Right if self.panel == Some(SidePanel::Alerts) => {
                let mut action = AlertPanelAction::None;
                egui::Panel::right("terminal_alerts")
                    .default_size(340.0)
                    .show(ui, |ui| {
                        action = self.alerts.panel.show(ui);
                    });
                if action != AlertPanelAction::None {
                    self.emit(Action::Alerts(action));
                }
            }
            Region::Windows => {
                let action = self.alerts.dialog.show(ui.ctx());
                if !matches!(action, AlertDialogAction::None) {
                    self.emit(Action::AlertDialog(action));
                }
            }
            _ => {}
        }
    }

    fn open_alert(&mut self, alert: Option<Alert>, price: Option<f64>) {
        self.alerts.dialog.sources = self.chart().alert_sources();
        self.alerts.dialog.timezone = self
            .chart()
            .chart_options()
            .time_scale
            .timezone
            .to_tz()
            .unwrap_or_else(egui_charts::model::TimezoneId::utc);
        if let Some(alert) = alert {
            self.alerts.dialog.edit(alert);
        } else {
            let chart = self.chart();
            let symbol = chart.symbol().to_owned();
            let price = price.or_else(|| chart.bars(SeriesId::MAIN).last().map(|bar| bar.close));
            if let Some(price) = price {
                self.alerts.dialog.open(symbol, price);
            }
        }
    }

    pub(super) fn alerts_perform(&mut self, action: Action) -> Option<Action> {
        if self.services.alerts.is_none() {
            return Some(action);
        }
        match action {
            Action::TopToolbar(TopToolbarAction::CreateAlert)
            | Action::Alerts(AlertPanelAction::CreateAlert)
            | Action::ChartMenu(ChartContextAction::Series(SeriesContextMenuAction::AddAlert)) => {
                self.open_alert(None, None)
            }
            Action::WidgetBar(WidgetBarAction::Clicked(RightBarIcon::Alerts)) => {
                self.open_panel(SidePanel::Alerts)
            }
            Action::Alerts(
                AlertPanelAction::EditAlert(alert) | AlertPanelAction::DuplicateAlert(alert),
            )
            | Action::ChartMenu(ChartContextAction::Study {
                action: StudyContextMenuAction::AddAlert(alert),
                ..
            })
            | Action::ChartMenu(ChartContextAction::Drawing(DrawingContextMenuAction::AddAlert(
                alert,
            ))) => self.open_alert(Some(*alert), None),
            Action::ChartMenu(ChartContextAction::Chart(ContextMenuAction::AddAlert {
                price,
                ..
            })) => self.open_alert(None, Some(price)),
            Action::AlertDialog(AlertDialogAction::Create(alert)) => {
                let mut alert = *alert;
                alert.timeframe = alert.timeframe.or(self.chart().timeframe());
                let definition = Definition::new(&alert, self.chart());
                self.alerts.panel.save(alert.clone());
                if let Some(engine) = self
                    .services
                    .alerts
                    .as_ref()
                    .and_then(|service| service.engine())
                    && let Some(saved) = engine.alerts().iter().rev().find(|saved| {
                        let mut saved = (*saved).clone();
                        saved.id = alert.id.clone();
                        saved == alert
                    })
                    && let Some(id) = &saved.id
                {
                    if self
                        .alerts
                        .definitions
                        .get(id)
                        .is_none_or(|kept| kept.sources != definition.sources)
                    {
                        self.alerts.definitions.insert(id.clone(), definition);
                    }
                }
            }
            Action::Alerts(AlertPanelAction::ExportLog(csv)) => {
                if !self.save_file("alerts.csv".into(), ExportContents::Text(csv.clone())) {
                    return Some(Action::Alerts(AlertPanelAction::ExportLog(csv)));
                }
            }
            Action::AlertDialog(AlertDialogAction::Cancel | AlertDialogAction::None) => {}
            action => return Some(action),
        }
        None
    }

    pub(super) fn alerts_settings(&mut self, _settings: &mut UserSettings, _direction: Direction) {}

    pub(super) fn alerts_modal(&self) -> bool {
        self.alerts.dialog.is_open
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{SIZE, frame};
    use crate::{Action, Terminal};
    use chrono::Utc;
    use egui_charts::data::{
        Datafeed, HistoryPage, HistoryRequest, Reply, Subscription, SubscriptionId,
    };
    use egui_charts::model::{Alert, AlertCondition, AlertSource, Timeframe};
    use egui_charts::model::{AlertFrequency, AlertStatus, Bar};
    use egui_charts::services::AlertEngine;
    use egui_charts::studies::{InputValue, SMA};
    use egui_charts::ui::dialogs::AlertDialogAction;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Feed {
        subscriptions: Mutex<Vec<Subscription>>,
        history: Mutex<HashMap<(String, Timeframe), Vec<Bar>>>,
        hold_other: Mutex<bool>,
        held: Mutex<Option<Reply<HistoryPage>>>,
    }

    fn bar(minute: i64, close: f64) -> Bar {
        Bar::new(
            chrono::DateTime::from_timestamp(minute * 60, 0).unwrap(),
            close,
            close,
            close,
            close,
            1.0,
        )
    }

    impl Datafeed for Feed {
        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            let bars = self
                .history
                .lock()
                .unwrap()
                .entry((request.symbol.clone(), request.timeframe))
                .or_insert_with(|| vec![bar(0, 10.0), bar(5, 20.0)])
                .clone();
            if *self.hold_other.lock().unwrap() && request.symbol == "OTHER" {
                *self.held.lock().unwrap() = Some(reply);
            } else {
                reply.send(Ok(HistoryPage::new(bars, true)));
            }
        }
        fn subscribe(&self, subscription: Subscription) {
            self.subscriptions.lock().unwrap().push(subscription);
        }
        fn unsubscribe(&self, id: SubscriptionId) {
            self.subscriptions
                .lock()
                .unwrap()
                .retain(|subscription| subscription.id != id);
        }
    }

    impl Feed {
        fn push(&self, symbol: &str, timeframe: Timeframe, next: Bar) {
            self.history
                .lock()
                .unwrap()
                .entry((symbol.into(), timeframe))
                .or_default()
                .push(next.clone());
            let sinks: Vec<_> = self
                .subscriptions
                .lock()
                .unwrap()
                .iter()
                .filter(|subscription| {
                    subscription.symbol == symbol && subscription.timeframe == timeframe
                })
                .map(|subscription| subscription.sink.clone())
                .collect();
            for sink in sinks {
                sink.push_bar(next.clone());
            }
        }
        fn count(&self, symbol: &str, timeframe: Timeframe) -> usize {
            self.subscriptions
                .lock()
                .unwrap()
                .iter()
                .filter(|subscription| {
                    subscription.symbol == symbol && subscription.timeframe == timeframe
                })
                .count()
        }
    }

    fn frames(terminal: &mut Terminal) {
        for _ in 0..5 {
            frame(terminal, SIZE, vec![]);
        }
    }

    #[test]
    fn hidden_watchlist_studies_keep_inputs_share_feeds_and_handoff_without_repeating() {
        let feed = Arc::new(Feed::default());
        let fired = Arc::new(Mutex::new(Vec::new()));
        let triggers = fired.clone();
        let engine = Arc::new(
            AlertEngine::new()
                .on_trigger(move |_, event| triggers.lock().unwrap().push(event.clone())),
        );
        let mut terminal = Terminal::new(feed.clone())
            .with_symbol("SHOWN")
            .with_alerts(engine.clone());
        let study = terminal.chart_mut().add_indicator(Box::new(SMA::new(2)));
        let mut alert = Alert::new("OTHER", AlertCondition::Above(25.0));
        alert.timeframe = Some(Timeframe::Min5);
        alert.source = AlertSource::Study {
            id: study.0.to_string(),
            output: 0,
        };
        alert.frequency = AlertFrequency::EveryTime;
        frames(&mut terminal);
        terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
            alert.clone(),
        ))));
        let mut list = alert;
        list.symbol.clear();
        list.watchlist = Some("list".into());
        engine.set_watchlist("list", vec!["OTHER".into(), "THIRD".into()]);
        terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
            list,
        ))));
        frames(&mut terminal);
        assert_eq!(feed.count("OTHER", Timeframe::Min5), 1);
        assert_eq!(feed.count("THIRD", Timeframe::Min5), 1);
        assert!(fired.lock().unwrap().is_empty());

        // Creation length is two: after navigation the mean is (20 + 40) / 2 = 30,
        // even though the visible study is edited to length three, whose mean would be 70 / 3.
        terminal
            .chart_mut()
            .indicator_mut(study)
            .unwrap()
            .set_input("period", InputValue::Length(3))
            .unwrap();
        // A second alert on the same chart-local id retains the edited definition independently.
        let mut later = Alert::new("OTHER", AlertCondition::Above(25.0));
        later.timeframe = Some(Timeframe::Min5);
        later.source = AlertSource::Study {
            id: study.0.to_string(),
            output: 0,
        };
        later.frequency = AlertFrequency::EveryTime;
        terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
            later,
        ))));
        terminal.chart_mut().set_symbol("ELSEWHERE");
        feed.push("OTHER", Timeframe::Min5, bar(10, 40.0));
        frames(&mut terminal);
        assert_eq!(fired.lock().unwrap().len(), 2);
        feed.push("THIRD", Timeframe::Min5, bar(10, 40.0));
        frames(&mut terminal);
        assert_eq!(fired.lock().unwrap().len(), 3);

        // Loading that subject into a live chart releases the background subscription. Equal
        // bars and retained study outputs do not constitute another occurrence.
        terminal.chart_mut().set_timeframe(Timeframe::Min5);
        terminal.chart_mut().set_symbol("OTHER");
        frames(&mut terminal);
        assert_eq!(feed.count("OTHER", Timeframe::Min5), 1);
        assert_eq!(fired.lock().unwrap().len(), 3);
        terminal.chart_mut().set_symbol("ELSEWHERE");
        frames(&mut terminal);
        assert_eq!(feed.count("OTHER", Timeframe::Min5), 1);
        assert_eq!(fired.lock().unwrap().len(), 3);

        // Pausing the alerts on OTHER drops its only shared subscription immediately.
        for mut alert in engine.alerts() {
            alert.status = AlertStatus::Disabled;
            terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
                alert,
            ))));
        }
        frames(&mut terminal);
        assert_eq!(feed.count("OTHER", Timeframe::Min5), 0);
        assert_eq!(feed.count("THIRD", Timeframe::Min5), 0);
    }

    #[test]
    fn hidden_once_alert_waits_for_history_and_releases_its_feed_on_trigger_and_expiry() {
        let feed = Arc::new(Feed::default());
        let engine = Arc::new(AlertEngine::new());
        let mut terminal = Terminal::new(feed.clone())
            .with_symbol("SHOWN")
            .with_alerts(engine.clone());
        frames(&mut terminal);
        *feed.hold_other.lock().unwrap() = true;
        let mut alert = Alert::new("OTHER", AlertCondition::Above(30.0));
        alert.timeframe = Some(Timeframe::Min5);
        terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
            alert,
        ))));
        frames(&mut terminal);
        assert_eq!(feed.count("OTHER", Timeframe::Min5), 1);
        feed.push("OTHER", Timeframe::Min5, bar(10, 40.0));
        frames(&mut terminal);
        assert_eq!(
            engine.alerts()[0].status,
            AlertStatus::Active,
            "live data waits for the initial history"
        );
        *feed.hold_other.lock().unwrap() = false;
        feed.held
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(Ok(HistoryPage::new(vec![bar(0, 10.0), bar(5, 20.0)], true)));
        frames(&mut terminal);
        assert_eq!(engine.alerts()[0].status, AlertStatus::Triggered);
        assert_eq!(feed.count("OTHER", Timeframe::Min5), 0);
        let mut alert = Alert::new("THIRD", AlertCondition::Above(30.0));
        alert.timeframe = Some(Timeframe::Min5);
        alert.expires = Some(Utc::now() + chrono::TimeDelta::hours(1));
        terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
            alert,
        ))));
        frames(&mut terminal);
        assert_eq!(feed.count("THIRD", Timeframe::Min5), 1);
        engine.tick(Utc::now() + chrono::TimeDelta::hours(2));
        frames(&mut terminal);
        assert_eq!(feed.count("THIRD", Timeframe::Min5), 0);
    }

    #[test]
    fn hidden_drawing_alert_reads_its_symbols_stored_line() {
        use crate::test_support::MemoryStorage;
        use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
        let feed = Arc::new(Feed::default());
        let storage = Arc::new(MemoryStorage::default());
        let engine = Arc::new(AlertEngine::new());
        let mut terminal = Terminal::new(feed.clone())
            .with_symbol("OTHER")
            .with_timeframe(Timeframe::Min5)
            .with_storage(storage.clone())
            .with_alerts(engine.clone());
        frames(&mut terminal);
        let mut line = Drawing::new(7, DrawingToolType::TrendLine);
        line.completed = true;
        line.chart_points = vec![ChartPoint::new(0.0, 10.0), ChartPoint::new(1.0, 20.0)];
        terminal
            .chart_mut()
            .user_action(|chart| chart.drawings_mut().add_drawing(line).unwrap());
        let source = terminal
            .chart()
            .drawings()
            .drawings
            .last()
            .unwrap()
            .alert_source(0);
        frames(&mut terminal);
        assert!(terminal.flush_storage());
        assert!(
            storage
                .get(egui_charts::services::StoreKind::Drawings, "OTHER")
                .is_some()
        );
        terminal.chart_mut().set_symbol("SHOWN");
        frames(&mut terminal);
        // A price alert already owns this subject's shared reader when drawings are attached.
        let mut price = Alert::new("OTHER", AlertCondition::Above(1000.0));
        price.timeframe = Some(Timeframe::Min5);
        terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
            price,
        ))));
        frames(&mut terminal);
        let mut alert = Alert::new("OTHER", AlertCondition::Above(25.0));
        alert.timeframe = Some(Timeframe::Min5);
        alert.source = source;
        terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
            alert,
        ))));
        frames(&mut terminal);
        assert_eq!(engine.alerts().last().unwrap().status, AlertStatus::Active);
        // The stored line through (0, 10) and (1, 20) has value 30 at bar 2.
        feed.push("OTHER", Timeframe::Min5, bar(10, 1.0));
        frames(&mut terminal);
        assert_eq!(
            engine.alerts().last().unwrap().status,
            AlertStatus::Triggered
        );
    }
}
