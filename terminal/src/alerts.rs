//! Alerts: the alerts panel, the alert dialog and headless evaluation on the host's feed.

use super::core::SidePanel;
use super::layout::Region;
use super::{Action, ExportContents, Terminal};
use chrono::Utc;
use egui_charts::config::KeyboardAction;
use egui_charts::model::{Alert, AlertCondition};
use egui_charts::services::AlertEvaluator;
use egui_charts::ui::Control;
use egui_charts::ui::context_menu::{ChartContextAction, ContextMenuAction};
use egui_charts::ui::dialogs::{
    AlertDialog, AlertDialogAction, DrawingContextMenuAction, SeriesContextMenuAction,
    StudyContextMenuAction,
};
use egui_charts::ui::top_toolbar::TopToolbarAction;
use egui_charts::ui::widget_bar::{AlertPanelAction, AlertsPanel, RightBarIcon, WidgetBarAction};
use egui_charts::widget::{SeriesId, ShortcutRequest};

#[derive(Default)]
pub(super) struct State {
    panel: AlertsPanel,
    dialog: AlertDialog,
    evaluator: Option<AlertEvaluator>,
}

impl Terminal {
    pub(super) fn alerts_init(&mut self) {
        self.alerts.evaluator = Some(AlertEvaluator::new(self.feed.clone()));
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
        let Some(service) = self.services.alerts.clone() else {
            return;
        };
        let lists = self.panels.market.watchlist.lists();
        if let Some(engine) = service.engine() {
            for (name, _) in &self.alerts.dialog.watchlists {
                if !lists.iter().any(|list| &list.name == name) {
                    engine.set_watchlist(name, Vec::new());
                }
            }
            for list in lists {
                engine.set_watchlist(&list.name, list.symbols().map(str::to_owned).collect());
            }
        }
        self.alerts.dialog.watchlists = lists
            .iter()
            .map(|list| (list.name.clone(), list.name.clone()))
            .collect();
        let settings = &self.studies.settings.alerts;
        self.alerts.panel.set_sound(settings.play_sound.then(|| {
            if settings.sound.is_empty() {
                self.services.sounds.first().cloned().unwrap_or_default()
            } else {
                settings.sound.clone()
            }
        }));
        self.alerts
            .panel
            .set_notification_seconds(settings.notification_seconds as f32);
        self.alerts
            .panel
            .set_symbol(self.charts[self.active].symbol());
        let inputs = self.charts[self.active].alert_inputs();
        self.alerts.panel.set_sources(inputs.sources.clone());
        self.alerts
            .panel
            .set_inputs(inputs, self.charts[self.active].timeframe());
        self.alerts.panel.poll(ctx, &mut self.toasts);
        if let Some(engine) = service.engine() {
            let evaluator = self.alerts.evaluator.as_mut().unwrap();
            if let Some(storage) = &self.services.storage {
                evaluator.set_storage(storage.clone());
            }
            let live: Vec<_> = self
                .charts
                .iter()
                .filter_map(|chart| chart.alert_data())
                .collect();
            for error in evaluator.poll(
                engine,
                self.studies.indicators.factory(),
                &live,
                ctx,
                Utc::now(),
            ) {
                self.toasts.error(format!("Alert data: {error}"));
            }
            self.alerts.panel.poll(ctx, &mut self.toasts);
            if engine
                .alerts()
                .iter()
                .any(|alert| alert.status == egui_charts::model::AlertStatus::Active)
            {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
        }
        if let Some(sound) = self.alerts.panel.take_sound() {
            self.play_sound(&sound, self.studies.settings.alerts.sound_volume);
        }
    }

    pub(super) fn alerts_show(&mut self, ui: &mut egui::Ui, region: Region) {
        match region {
            Region::Right if self.panel == Some(SidePanel::Alerts) => {
                let mut action = AlertPanelAction::None;
                egui::Panel::right("terminal_alerts")
                    .default_size(340.0)
                    .max_size(ui.available_width() * 0.5)
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
            Region::Chart if self.services.alerts.is_some() => {
                let chart = self.chart();
                let settings = &self.studies.settings.alerts;
                let series = chart.series_settings().alerts;
                if settings.show_alerts && series.visible {
                    let mapping = chart.get_chart_mapping();
                    let decimals = chart
                        .instrument()
                        .and_then(egui_charts::scales::instrument_price_precision)
                        .map(usize::from)
                        .unwrap_or(2);
                    let currency = chart
                        .instrument()
                        .and_then(|i| i.currency.as_deref())
                        .unwrap_or("");
                    if let Some(alert) = egui_charts::chart::render_alert_levels(
                        ui,
                        &mapping,
                        self.alerts.panel.alerts().iter().filter(|alert| {
                            alert.symbol == chart.symbol() && alert.watchlist.is_none()
                        }),
                        if series.color != egui_charts::model::SeriesAlerts::default().color {
                            series.color
                        } else {
                            settings.alert_color
                        },
                        settings.show_alert_labels && series.labels,
                        settings.show_active_only,
                        |price| format!("{price:.decimals$} {currency}"),
                    ) {
                        self.emit(Action::AlertDialog(AlertDialogAction::Create(Box::new(
                            alert,
                        ))));
                    }
                }
            }
            _ => {}
        }
    }

    fn open_alert(&mut self, alert: Option<Alert>) {
        self.alerts.dialog.sources = self.chart().alert_sources();
        self.alerts.dialog.timezone = self
            .chart()
            .chart_options()
            .time_scale
            .timezone
            .to_tz()
            .unwrap_or_else(egui_charts::model::TimezoneId::utc);
        if let Some(mut alert) = alert {
            self.chart().prepare_alert(&mut alert);
            self.alerts.dialog.edit(alert);
        } else {
            let chart = self.chart();
            let symbol = chart.symbol().to_owned();
            if let Some(bar) = chart.bars(SeriesId::MAIN).last() {
                self.alerts.dialog.open(symbol, bar.close);
                self.charts[self.active].prepare_alert(&mut self.alerts.dialog.alert);
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
                self.open_alert(None)
            }
            Action::WidgetBar(WidgetBarAction::Clicked(RightBarIcon::Alerts)) => {
                self.open_panel(SidePanel::Alerts)
            }
            Action::Alerts(
                AlertPanelAction::EditAlert(alert) | AlertPanelAction::DuplicateAlert(alert),
            )
            | Action::ChartMenu(ChartContextAction::Drawing(DrawingContextMenuAction::AddAlert(
                alert,
            ))) => self.open_alert(Some(*alert)),
            Action::ChartMenu(ChartContextAction::Chart(ContextMenuAction::AddAlert {
                symbol,
                price,
            })) => {
                let mut alert = Alert::new(symbol, AlertCondition::Crossing(price));
                self.chart().prepare_alert(&mut alert);
                self.alerts.panel.save(alert);
            }
            Action::Shortcut(ShortcutRequest::Action(KeyboardAction::CreateAlert)) => {
                let ctx = self.ctx();
                let mapping = self.chart().get_chart_mapping();
                if let Some(pos) = ctx
                    .pointer_hover_pos()
                    .filter(|pos| mapping.rect.contains(*pos))
                {
                    if ctx
                        .layer_id_at(pos)
                        .is_some_and(|layer| layer.order == egui::Order::Background)
                    {
                        let price = mapping.y_to_price(pos.y);
                        if price.is_finite() {
                            self.emit(Action::ChartMenu(ChartContextAction::Chart(
                                ContextMenuAction::AddAlert {
                                    symbol: self.chart().symbol().to_owned(),
                                    price,
                                },
                            )));
                        }
                    }
                } else {
                    self.open_alert(None);
                }
            }
            Action::AlertDialog(AlertDialogAction::Create(alert))
            | Action::ChartMenu(ChartContextAction::Study {
                action: StudyContextMenuAction::AddAlert(alert),
                ..
            }) => {
                let mut alert = *alert;
                self.chart().prepare_alert(&mut alert);
                self.alerts.panel.save(alert);
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

    pub(super) fn alerts_modal(&self) -> bool {
        self.alerts.dialog.is_open
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{SIZE, click, frame};
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
    use egui_charts::widget::SeriesId;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Feed {
        subscriptions: Mutex<Vec<Subscription>>,
        history: Mutex<HashMap<(String, Timeframe), Vec<Bar>>>,
        hold_other: Mutex<bool>,
        held: Mutex<Option<Reply<HistoryPage>>>,
        page_size: Mutex<Option<usize>>,
        requests: Mutex<Vec<HistoryRequest>>,
        snapshots: Mutex<Vec<SubscriptionId>>,
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
        fn resolve(&self, symbol: &str, reply: Reply<egui_charts::model::Instrument>) {
            use chrono::{NaiveTime, Weekday};
            use egui_charts::model::{Instrument, Session, SessionWindow};
            let mut instrument = Instrument::new(symbol);
            instrument.tick_size = Some(1.0);
            if symbol == "SHOWN" {
                instrument.session = Some(Session::new([SessionWindow::new(
                    [
                        Weekday::Mon,
                        Weekday::Tue,
                        Weekday::Wed,
                        Weekday::Thu,
                        Weekday::Fri,
                    ],
                    NaiveTime::from_hms_opt(9, 32, 0).unwrap(),
                    NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
                )]));
            }
            reply.send(Ok(instrument));
        }
        fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
            if request.symbol == "OTHER" || request.symbol == "THIRD" {
                assert_eq!(
                    request.clock,
                    egui_charts::model::BarClock::new(request.timeframe),
                    "an always-open member must not inherit the creation chart's session"
                );
            }
            self.requests.lock().unwrap().push(request.clone());
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
                let bars: Vec<_> = bars
                    .into_iter()
                    .filter(|bar| {
                        request.before.is_none_or(|before| bar.time < before)
                            && request.since.is_none_or(|since| bar.time >= since)
                    })
                    .collect();
                let count = self.page_size.lock().unwrap().unwrap_or(request.count);
                let start = bars.len().saturating_sub(count);
                reply.send(Ok(HistoryPage::new(bars[start..].to_vec(), start == 0)));
            }
        }
        fn subscribe(&self, subscription: Subscription) {
            self.subscriptions.lock().unwrap().push(subscription);
        }
        fn request_depth_snapshot(&self, id: SubscriptionId) {
            self.snapshots.lock().unwrap().push(id);
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
        let sounds = Arc::new(Mutex::new(Vec::new()));
        let played = sounds.clone();
        let engine = Arc::new(
            AlertEngine::new()
                .on_trigger(move |_, event| triggers.lock().unwrap().push(event.clone())),
        );
        let mut terminal = Terminal::new(feed.clone())
            .with_symbol("SHOWN")
            .with_timeframe(Timeframe::Min5)
            .with_alerts(engine.clone())
            .with_sounds(vec!["Bell".into()], move |name, volume| {
                played.lock().unwrap().push((name.to_owned(), volume));
            });
        terminal.studies.settings.alerts.play_sound = true;
        terminal.studies.settings.alerts.sound = "Bell".into();
        terminal.studies.settings.alerts.sound_volume = 0.35;
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
        let mut settings = egui_charts::services::UserSettings::default();
        settings.watchlists = vec![egui_charts::model::Watchlist::from_text(
            "list",
            "OTHER,THIRD",
        )];
        terminal.panels_settings(&mut settings, crate::core::Direction::Load);
        terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
            list,
        ))));
        frames(&mut terminal);
        assert_eq!(
            terminal.alerts.dialog.watchlists,
            [("list".into(), "list".into())]
        );
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
        assert_eq!(*sounds.lock().unwrap(), vec![("Bell".into(), 0.35); 2]);

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
    fn alert_watchlists_follow_panel_edits_and_settings_replacement() {
        use egui_charts::model::Watchlist;
        use egui_charts::services::UserSettings;

        let engine = Arc::new(AlertEngine::new());
        let mut terminal = Terminal::new(Arc::new(Feed::default()))
            .with_symbol("SHOWN")
            .with_alerts(engine.clone());
        frames(&mut terminal);
        let mut settings = UserSettings::default();
        settings.watchlists = vec![Watchlist::from_text("list", "OTHER")];
        terminal.panels_settings(&mut settings, crate::core::Direction::Load);
        let mut alert = Alert::new("", AlertCondition::Above(100.0));
        alert.watchlist = Some("list".into());
        terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
            alert,
        ))));
        frames(&mut terminal);
        let id = engine.alerts()[0].id.clone().unwrap();
        assert_eq!(engine.subjects(&id), ["OTHER"]);

        terminal.panels_mut().watchlist.add_symbol_to(0, "THIRD");
        frames(&mut terminal);
        assert_eq!(engine.subjects(&id), ["OTHER", "THIRD"]);
        assert_eq!(
            terminal.user_settings().watchlists[0].to_text(),
            "OTHER,THIRD"
        );

        settings.watchlists = vec![Watchlist::from_text("renamed", "THIRD")];
        terminal.panels_settings(&mut settings, crate::core::Direction::Load);
        frames(&mut terminal);
        assert!(engine.subjects(&id).is_empty());
        assert_eq!(
            terminal.alerts.dialog.watchlists,
            [("renamed".into(), "renamed".into())]
        );
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
        *feed.page_size.lock().unwrap() = Some(1);
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
            .send(Ok(HistoryPage::new(vec![bar(5, 20.0)], false)));
        frame(&mut terminal, SIZE, vec![]);
        assert_eq!(
            engine.alerts()[0].status,
            AlertStatus::Active,
            "a short page must finish loading before evaluation"
        );
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

        let mut failures = Vec::new();
        for (name, period, start) in [
            ("SMA", 2, None),
            ("SMA", 2000, None),
            ("Vol Profile FR", 0, Some(1440)),
        ] {
            let feed = Arc::new(Feed::default());
            feed.history.lock().unwrap().insert(
                ("THIRD".into(), Timeframe::Day1),
                vec![bar(0, 10.0), bar(1440, 10.0), bar(2880, 10.0)],
            );
            feed.history.lock().unwrap().insert(
                ("OTHER".into(), Timeframe::Min1),
                (0..4320).map(|minute| bar(minute, 10.0)).collect(),
            );
            let engine = Arc::new(AlertEngine::new());
            let mut terminal = Terminal::new(feed.clone())
                .with_symbol("SHOWN")
                .with_alerts(engine.clone());
            frames(&mut terminal);
            let factory = egui_charts::studies::IndicatorFactory::new();
            let mut study =
                egui_charts::studies::ConfiguredStudy::new(factory.create(name).unwrap());
            use egui_charts::studies::Indicator;
            if let Some(start) = start {
                study
                    .set_input(
                        "source_interval",
                        InputValue::Timeframe(Some(Timeframe::Min1)),
                    )
                    .unwrap();
                study
                    .set_input(
                        "start_time",
                        InputValue::Text(format!("1970-01-0{} 00:00:00", start / 1440 + 1)),
                    )
                    .unwrap();
            } else {
                study
                    .set_input("period", InputValue::Length(period))
                    .unwrap();
                study
                    .set_input("calculation_symbol", InputValue::Symbol("OTHER".into()))
                    .unwrap();
                study
                    .set_input(
                        "calculation_timeframe",
                        InputValue::Timeframe(Some(Timeframe::Min1)),
                    )
                    .unwrap();
            }
            let id = terminal.chart_mut().add_indicator(Box::new(study));
            let mut alert = Alert::new("THIRD", AlertCondition::Above(1.0));
            alert.timeframe = Some(Timeframe::Day1);
            alert.source = AlertSource::Study {
                id: id.0.to_string(),
                output: 0,
            };
            // The profile reads the member's own finer series.
            if start.is_some() {
                feed.history.lock().unwrap().insert(
                    ("THIRD".into(), Timeframe::Min1),
                    (0..4320).map(|minute| bar(minute, 10.0)).collect(),
                );
            }
            *feed.page_size.lock().unwrap() = Some(700);
            terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
                alert,
            ))));
            // A ready latest page is insufficient while the required older pages are pending.
            for _ in 0..3 {
                frame(&mut terminal, SIZE, vec![]);
            }
            if engine.alerts()[0].status != AlertStatus::Active {
                failures.push(format!("{name} {period}: early evaluation"));
            }
            for _ in 0..15 {
                frame(&mut terminal, SIZE, vec![]);
            }
            if start.is_none() && engine.alerts()[0].status != AlertStatus::Triggered {
                failures.push(format!("{name} {period}: incomplete coverage"));
            }
            if let Some(start) = start {
                let befores: Vec<_> = feed
                    .requests
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|request| {
                        request.symbol == "THIRD" && request.timeframe == Timeframe::Min1
                    })
                    .filter_map(|request| request.before)
                    .collect();
                if !befores
                    .iter()
                    .any(|before| *before <= bar(start + 700, 0.0).time)
                    || befores.iter().any(|before| *before <= bar(start, 0.0).time)
                {
                    failures.push(format!("{name}: ignored data_start {befores:?}"));
                }
            }
        }
        assert!(failures.is_empty(), "auxiliary inputs: {failures:?}");
    }

    #[test]
    fn restored_study_alert_does_not_borrow_a_chart_local_id() {
        let mut failures = Vec::new();
        for path in [
            "legacy",
            "duplicate",
            "edit",
            "restore",
            "events",
            "reset",
            "history",
            "visible",
        ] {
            let feed = Arc::new(Feed::default());
            if matches!(path, "history" | "visible") {
                feed.history.lock().unwrap().insert(
                    ("OTHER".into(), Timeframe::Min5),
                    (0..1100).map(|at| bar(at * 5, at as f64 + 1.0)).collect(),
                );
            }
            let mut engine = Arc::new(AlertEngine::new());
            let mut terminal = Terminal::new(feed.clone())
                .with_symbol("OTHER")
                .with_timeframe(Timeframe::Min5)
                .with_alerts(engine.clone());
            let study = if path == "visible" {
                terminal
                    .chart_mut()
                    .add_indicator(Box::new(egui_charts::studies::VisibleAveragePrice::new()))
            } else if path == "history" {
                terminal
                    .chart_mut()
                    .add_indicator(Box::new(egui_charts::studies::EMA::new(1000)))
            } else if matches!(path, "events" | "reset") {
                terminal.chart_mut().add_indicator(Box::new(
                    egui_charts::studies::OrderFlowStudy::new(
                        egui_charts::data::OrderFlowKind::Spread,
                    ),
                ))
            } else {
                terminal.chart_mut().add_indicator(Box::new(SMA::new(2)))
            };
            frames(&mut terminal);
            if matches!(path, "history" | "visible") {
                let bars = (800..1100).map(|at| bar(at * 5, at as f64 + 1.0)).collect();
                terminal
                    .chart_mut()
                    .set_data(egui_charts::widget::SeriesId::MAIN, bars);
                if path == "visible" {
                    terminal.chart_mut().set_visible_logical_range(
                        egui_charts::model::LogicalRange::new(0.0, 49.0),
                    );
                }
                frames(&mut terminal);
            }
            let mut alert = Alert::new(
                "OTHER",
                AlertCondition::Above(if path == "history" { 0.0 } else { 25.0 }),
            );
            if path == "visible" {
                alert.condition = AlertCondition::Below(900.0);
                alert.status = AlertStatus::Disabled;
            }
            if path == "history" {
                alert.frequency = AlertFrequency::EveryTime;
            }
            alert.timeframe = Some(Timeframe::Min5);
            alert.source = AlertSource::Study {
                id: study.0.to_string(),
                output: 0,
            };
            if matches!(path, "events" | "reset") {
                let input = terminal
                    .chart()
                    .indicators()
                    .find(|(id, _)| *id == study)
                    .unwrap()
                    .1
                    .order_flow_inputs()[0]
                    .clone();
                input.push_quote(
                    &egui_charts::model::BarClock::new(Timeframe::Min5),
                    &egui_charts::model::Level1Quote {
                        ts: bar(5, 40.0).time,
                        symbol: "OTHER".into(),
                        bid: 10.0,
                        ask: 50.0,
                        bid_size: 1.0,
                        ask_size: 1.0,
                    },
                    1.0,
                );
            }
            if path == "legacy" {
                engine = Arc::new(AlertEngine::new().with_alerts(vec![alert]));
                terminal = terminal.with_alerts(engine.clone());
            } else {
                terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
                    alert,
                ))));
            }
            frames(&mut terminal);
            if matches!(path, "events" | "reset")
                && engine.alerts()[0].status != AlertStatus::Active
            {
                failures.push("event source isolation");
            }
            if path == "visible" {
                let mut alerts = engine.alerts();
                alerts[0].status = AlertStatus::Active;
                let json = serde_json::to_string(&alerts).unwrap();
                let restored =
                    Arc::new(AlertEngine::new().with_alerts(serde_json::from_str(&json).unwrap()));
                terminal = Terminal::new(feed)
                    .with_symbol("SHOWN")
                    .with_alerts(restored.clone());
                frames(&mut terminal);
                if restored.alerts()[0].status != AlertStatus::Triggered {
                    failures.push(path);
                }
                continue;
            }
            if path == "history" {
                let before = terminal.alerts.panel.log().len();
                terminal.chart_mut().set_symbol("SHOWN");
                frames(&mut terminal);
                terminal.chart_mut().set_symbol("OTHER");
                frames(&mut terminal);
                if before != 1 || terminal.alerts.panel.log().len() != before {
                    failures.push(path);
                }
                continue;
            }
            if !matches!(path, "events" | "reset") {
                terminal
                    .chart_mut()
                    .indicator_mut(study)
                    .unwrap()
                    .set_input("period", InputValue::Length(3))
                    .unwrap();
            }
            let saved = engine.alerts()[0].clone();
            match path {
                "duplicate" | "edit" => {
                    let action = if path == "duplicate" {
                        let mut copy = saved;
                        copy.id = None;
                        egui_charts::ui::widget_bar::AlertPanelAction::DuplicateAlert(Box::new(
                            copy,
                        ))
                    } else {
                        egui_charts::ui::widget_bar::AlertPanelAction::EditAlert(Box::new(saved))
                    };
                    terminal.run(Action::Alerts(action));
                    let edited = terminal.alerts.dialog.alert.clone();
                    terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
                        edited,
                    ))));
                }
                "restore" => {
                    let json = serde_json::to_string(&engine.alerts()).unwrap();
                    let restored = Arc::new(
                        AlertEngine::new().with_alerts(serde_json::from_str(&json).unwrap()),
                    );
                    terminal = Terminal::new(feed.clone())
                        .with_symbol("SHOWN")
                        .with_alerts(restored);
                }
                _ => {}
            }
            terminal.chart_mut().set_symbol("SHOWN");
            frames(&mut terminal);
            if path == "reset" {
                for subscription in feed
                    .subscriptions
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|subscription| subscription.symbol == "OTHER")
                {
                    subscription.sink.reset_replay(bar(10, 40.0).time);
                }
            }
            feed.push("OTHER", Timeframe::Min5, bar(10, 40.0));
            if matches!(path, "events" | "reset") {
                for subscription in feed
                    .subscriptions
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|subscription| subscription.symbol == "OTHER")
                {
                    subscription
                        .sink
                        .push_quote(egui_charts::model::Level1Quote {
                            ts: bar(10, 40.0).time,
                            symbol: "OTHER".into(),
                            bid: 10.0,
                            ask: 40.0,
                            bid_size: 1.0,
                            ask_size: 1.0,
                        });
                }
            }
            frames(&mut terminal);
            let alerts = terminal
                .services
                .alerts
                .as_ref()
                .unwrap()
                .engine()
                .unwrap()
                .alerts();
            let expected = if path == "legacy" {
                AlertStatus::Active
            } else {
                AlertStatus::Triggered
            };
            if alerts.last().unwrap().status != expected {
                failures.push(path);
            }
        }
        assert!(failures.is_empty(), "creation inputs lost on {failures:?}");
    }

    #[test]
    fn hidden_feeds_keep_creation_sessions_and_do_not_handoff_to_a_different_view() {
        let feed = Arc::new(Feed::default());
        let engine = Arc::new(AlertEngine::new());
        let mut terminal = Terminal::new(feed.clone())
            .with_symbol("SHOWN")
            .with_timeframe(Timeframe::Min5)
            .with_alerts(engine.clone());
        frames(&mut terminal);
        for extended in [false, true] {
            terminal.chart_mut().set_extended_hours(extended);
            let mut alert = Alert::new("OTHER", AlertCondition::Above(1000.0));
            alert.timeframe = Some(Timeframe::Min5);
            terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
                alert,
            ))));
            frames(&mut terminal);
        }
        assert_eq!(
            feed.count("OTHER", Timeframe::Min5),
            2,
            "distinct sessions need distinct feeds"
        );
        let requests = feed.requests.lock().unwrap();
        assert!(
            requests
                .iter()
                .any(|request| request.symbol == "OTHER" && request.extended_hours)
        );
        assert!(
            requests
                .iter()
                .any(|request| request.symbol == "OTHER" && !request.extended_hours)
        );
        drop(requests);
        terminal.chart_mut().set_symbol("OTHER");
        frames(&mut terminal);
        assert_eq!(
            feed.count("OTHER", Timeframe::Min5),
            2,
            "only the matching session hands off to the live chart"
        );

        // An always-open creation symbol must not erase another member's trading windows.
        let feed = Arc::new(Feed::default());
        let mut terminal = Terminal::new(feed.clone())
            .with_symbol("OTHER")
            .with_timeframe(Timeframe::Min5)
            .with_alerts(Arc::new(AlertEngine::new()));
        frames(&mut terminal);
        terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
            Alert::new("SHOWN", AlertCondition::Above(1000.0)),
        ))));
        frames(&mut terminal);
        let requests = feed.requests.lock().unwrap();
        let request = requests
            .iter()
            .find(|request| request.symbol == "SHOWN")
            .unwrap();
        assert!(
            request.clock.has_session(),
            "members retain their own exchange calendar"
        );
    }

    #[test]
    fn failed_background_stream_reconnects_and_ignores_its_old_sink() {
        use egui_charts::data::FeedError;
        use egui_charts::services::AlertFeed;
        for error in [
            FeedError::RateLimited {
                retry_after: Some(std::time::Duration::ZERO),
            },
            FeedError::NotSupported,
        ] {
            let host = Arc::new(Feed::default());
            let ctx = egui::Context::default();
            let mut request = AlertFeed::history_request("OTHER", Timeframe::Min5);
            request.since = Some(bar(5, 0.0).time);
            let mut reader = AlertFeed::new(host.clone(), request, None);
            for _ in 0..3 {
                assert!(reader.poll(&ctx, Utc::now()).is_empty());
            }
            assert_eq!(
                reader.bars().first().unwrap().time,
                bar(5, 0.0).time,
                "a coverage goal must not erase an explicit history bound"
            );
            let (id, old) = {
                let subscriptions = host.subscriptions.lock().unwrap();
                (subscriptions[0].id, subscriptions[0].sink.clone())
            };
            let book = egui_charts::model::OrderBook {
                symbol: "OTHER".into(),
                ts: bar(5, 0.0).time,
                bids: vec![],
                asks: vec![],
            };
            old.push_depth_snapshot(1, book.clone());
            old.push_depth_delta(3, &book);
            assert!(reader.poll(&ctx, Utc::now()).is_empty());
            assert_eq!(
                *host.snapshots.lock().unwrap(),
                [id],
                "a lost book must request a replacement snapshot"
            );
            old.report_error(error.clone());
            assert_eq!(
                reader.poll(&ctx, Utc::now()).as_slice(),
                std::slice::from_ref(&error)
            );
            assert!(reader.bars().is_empty());
            assert_eq!(
                host.count("OTHER", Timeframe::Min5),
                usize::from(error.is_recoverable())
            );
            old.push_bar(bar(10, 999.0));
            for _ in 0..3 {
                assert!(reader.poll(&ctx, Utc::now()).is_empty());
            }
            if error.is_recoverable() {
                assert_eq!(reader.bars().last().unwrap().close, 20.0);
                host.push("OTHER", Timeframe::Min5, bar(10, 40.0));
                assert!(reader.poll(&ctx, Utc::now()).is_empty());
                assert_eq!(reader.bars().last().unwrap().close, 40.0);
            } else {
                assert!(reader.bars().is_empty());
            }
        }
    }

    #[test]
    fn menu_and_quick_alerts_keep_their_symbols() {
        for (size, dark) in [
            (SIZE, false),
            (SIZE, true),
            (egui::vec2(480.0, 640.0), false),
            (egui::vec2(480.0, 640.0), true),
        ] {
            let engine = Arc::new(AlertEngine::new());
            let mut terminal = Terminal::new(Arc::new(Feed::default()))
                .with_symbol("SHOWN")
                .with_alerts(engine.clone());
            for _ in 0..5 {
                frame(&mut terminal, size, vec![]);
            }
            if dark {
                terminal.run(Action::TopToolbar(
                    egui_charts::ui::top_toolbar::TopToolbarAction::ThemeToggle,
                ));
            }
            assert!(
                terminal
                    .run(Action::ChartMenu(
                        egui_charts::ui::context_menu::ChartContextAction::Chart(
                            egui_charts::ui::context_menu::ContextMenuAction::AddAlert {
                                symbol: "OTHER".into(),
                                price: 25.0,
                            }
                        )
                    ))
                    .is_none()
            );
            for _ in 0..5 {
                frame(&mut terminal, size, vec![]);
            }
            assert!(
                !terminal.alerts.dialog.is_open,
                "price menus save immediately"
            );
            assert_eq!(engine.alerts().len(), 1, "size {size:?}, dark {dark}");
            assert_eq!(engine.alerts()[0].symbol, "OTHER");
            terminal.run(Action::WidgetBar(
                egui_charts::ui::widget_bar::WidgetBarAction::Clicked(
                    egui_charts::ui::widget_bar::RightBarIcon::Alerts,
                ),
            ));
            for _ in 0..5 {
                frame(&mut terminal, size, vec![]);
            }
            assert!(
                terminal.chart().get_rendered_price_rect().width() >= 100.0,
                "the alerts panel must leave room to read the chart at {size:?}"
            );
            let screen = frame(&mut terminal, size, vec![]);
            click(&mut terminal, size, screen.text_at("Quick alert").unwrap());
            for _ in 0..5 {
                frame(&mut terminal, size, vec![]);
            }
            let screen = frame(&mut terminal, size, vec![]);
            click(&mut terminal, size, screen.text_at("Create alert").unwrap());
            for _ in 0..5 {
                frame(&mut terminal, size, vec![]);
            }
            assert_eq!(
                engine.alerts().len(),
                2,
                "quick form at {size:?}, dark {dark}"
            );
            assert_eq!(engine.alerts()[1].symbol, "SHOWN");
            assert_eq!(engine.alerts()[1].timeframe, terminal.chart().timeframe());
            assert!(
                serde_json::to_value(&engine.alerts()[1]).unwrap()["inputs"].is_object(),
                "quick creation must save its inputs"
            );
            let study = terminal.chart_mut().add_indicator(Box::new(SMA::new(2)));
            let mut alert = Alert::new("SHOWN", AlertCondition::Above(100.0));
            alert.source = AlertSource::Study {
                id: study.0.to_string(),
                output: 0,
            };
            terminal.run(Action::ChartMenu(
                egui_charts::ui::context_menu::ChartContextAction::Study {
                    id: study,
                    action: egui_charts::ui::dialogs::StudyContextMenuAction::AddAlert(Box::new(
                        alert,
                    )),
                },
            ));
            frames(&mut terminal);
            assert!(
                !terminal.alerts.dialog.is_open,
                "study menus save immediately"
            );
            assert_eq!(engine.alerts().len(), 3);
            terminal.run(Action::TopToolbar(
                egui_charts::ui::top_toolbar::TopToolbarAction::CreateAlert,
            ));
            for _ in 0..5 {
                frame(&mut terminal, size, vec![]);
            }
            let screen = frame(&mut terminal, size, vec![]);
            let save = screen.text_at("Save alert").unwrap();
            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains(save));
            click(&mut terminal, size, save);
            for _ in 0..5 {
                frame(&mut terminal, size, vec![]);
            }
            assert_eq!(engine.alerts().len(), 4, "the bell opens the shared editor");
        }
    }

    #[test]
    fn alt_a_creates_a_host_alert_at_the_hovered_price_and_dragging_submits_its_new_level() {
        use crate::test_support::key;
        use egui::{Event, Key, Modifiers, PointerButton};
        use egui_charts::config::KeyboardAction;
        for dark in [false, true] {
            let engine = Arc::new(AlertEngine::new());
            let mut terminal = Terminal::new(Arc::new(Feed::default()))
                .with_symbol("OTHER")
                .with_alerts(engine.clone());
            terminal.theme_choice = Some(dark);
            frames(&mut terminal);
            let map = terminal.chart().get_chart_mapping();
            let point = map.rect.center();
            let range = map.price.range();
            let expected = (range.min + range.max) / 2.0;
            frame(
                &mut terminal,
                SIZE,
                vec![Event::PointerMoved(point), key(Key::A, Modifiers::ALT)],
            );
            frames(&mut terminal);
            assert_eq!(engine.alerts().len(), 1);
            assert_eq!(
                engine.alerts()[0].condition,
                AlertCondition::Crossing(expected)
            );
            assert_eq!(engine.alerts()[0].status, AlertStatus::Active);
            let target = point + egui::vec2(0.0, 45.0);
            let button = |pos, pressed| Event::PointerButton {
                pos,
                pressed,
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            };
            let scale = terminal.chart().get_time_scale_state();
            frame(&mut terminal, SIZE, vec![button(point, true)]);
            frame(
                &mut terminal,
                SIZE,
                vec![Event::PointerMoved(point + egui::vec2(0.0, 5.0))],
            );
            frame(&mut terminal, SIZE, vec![Event::PointerMoved(target)]);
            frame(&mut terminal, SIZE, vec![button(target, false)]);
            frames(&mut terminal);
            let AlertCondition::Crossing(level) = engine.alerts()[0].condition else {
                panic!("expected crossing");
            };
            let expected_drag =
                expected - 45.0 / f64::from(map.rect.height()) * (range.max - range.min);
            assert!(
                (level - expected_drag).abs() < 1e-6,
                "{level} expected {expected_drag}"
            );
            assert_eq!(
                terminal.chart().get_time_scale_state(),
                scale,
                "dragging an alert must not pan the chart"
            );
            for (chart_visible, series_visible) in [(false, true), (true, false)] {
                terminal.studies.settings.alerts.show_alerts = chart_visible;
                terminal
                    .chart_mut()
                    .series_style_mut(SeriesId::MAIN)
                    .unwrap()
                    .alerts
                    .visible = series_visible;
                frames(&mut terminal);
                frame(&mut terminal, SIZE, vec![Event::PointerMoved(target)]);
                frame(&mut terminal, SIZE, vec![button(target, true)]);
                frame(
                    &mut terminal,
                    SIZE,
                    vec![Event::PointerMoved(target + egui::vec2(0.0, 45.0))],
                );
                frame(
                    &mut terminal,
                    SIZE,
                    vec![button(target + egui::vec2(0.0, 45.0), false)],
                );
                frames(&mut terminal);
                assert_eq!(
                    engine.alerts()[0].condition,
                    AlertCondition::Crossing(level),
                    "hidden levels cannot be dragged"
                );
            }
            terminal.studies.settings.alerts.show_alerts = true;
            terminal
                .chart_mut()
                .series_style_mut(SeriesId::MAIN)
                .unwrap()
                .alerts
                .visible = true;
            terminal
                .chart_mut()
                .shortcuts_mut()
                .bindings
                .retain(|(_, action)| *action != KeyboardAction::CreateAlert);
            terminal.chart_mut().shortcuts_mut().bindings.insert(
                0,
                (
                    egui::KeyboardShortcut::new(Modifiers::ALT, Key::B),
                    KeyboardAction::CreateAlert,
                ),
            );
            for (keycode, count) in [(Key::A, 1), (Key::B, 2)] {
                frame(
                    &mut terminal,
                    SIZE,
                    vec![Event::PointerMoved(point), key(keycode, Modifiers::ALT)],
                );
                frames(&mut terminal);
                assert_eq!(
                    engine.alerts().len(),
                    count,
                    "alert creation follows the chart map"
                );
            }
            terminal.run(Action::TopToolbar(
                egui_charts::ui::top_toolbar::TopToolbarAction::OpenSettings,
            ));
            frames(&mut terminal);
            frame(&mut terminal, SIZE, vec![key(Key::B, Modifiers::ALT)]);
            frames(&mut terminal);
            assert_eq!(engine.alerts().len(), 2, "settings own their keys");
            assert!(!terminal.alerts.dialog.is_open);
        }
    }

    #[test]
    fn hidden_drawing_alert_reads_its_symbols_stored_line() {
        let tick_bar = |nanos, close| {
            let time = chrono::DateTime::from_timestamp(0, nanos).unwrap();
            Bar::new(time, close, close, close, close, 1.0)
        };
        for (timeframe, history, next) in [
            (
                Timeframe::Min5,
                vec![bar(0, 10.0), bar(5, 20.0)],
                bar(10, 1.0),
            ),
            (
                "1t".parse().unwrap(),
                vec![tick_bar(100, 10.0), tick_bar(200, 20.0)],
                tick_bar(300, 1.0),
            ),
        ] {
            use crate::test_support::MemoryStorage;
            use egui_charts::drawings::{ChartPoint, Drawing, DrawingToolType};
            let feed = Arc::new(Feed::default());
            feed.history
                .lock()
                .unwrap()
                .insert(("OTHER".into(), timeframe), history);
            let storage = Arc::new(MemoryStorage::default());
            let engine = Arc::new(AlertEngine::new());
            let mut terminal = Terminal::new(feed.clone())
                .with_symbol("OTHER")
                .with_timeframe(timeframe)
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
            price.timeframe = Some(timeframe);
            terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
                price,
            ))));
            frames(&mut terminal);
            let mut alert = Alert::new("OTHER", AlertCondition::Above(25.0));
            alert.timeframe = Some(timeframe);
            alert.source = source;
            terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
                alert,
            ))));
            frames(&mut terminal);
            assert_eq!(engine.alerts().last().unwrap().status, AlertStatus::Active);
            // The stored line through (0, 10) and (1, 20) has value 30 at bar 2.
            feed.push("OTHER", timeframe, next);
            frames(&mut terminal);
            assert_eq!(
                engine.alerts().last().unwrap().status,
                AlertStatus::Triggered
            );
            struct Unavailable;
            impl egui_charts::services::Storage for Unavailable {
                fn list(&self, _: egui_charts::services::StoreKind, _: Reply<Vec<String>>) {
                    panic!("alert reader must only load drawings");
                }
                fn save(
                    &self,
                    _: egui_charts::services::StoreKind,
                    _: &str,
                    _: String,
                    _: Reply<()>,
                ) {
                    panic!("alert reader must not write drawings");
                }
                fn delete(&self, _: egui_charts::services::StoreKind, _: &str, _: Reply<()>) {
                    panic!("alert reader must not delete drawings");
                }
                fn load(
                    &self,
                    _: egui_charts::services::StoreKind,
                    _: &str,
                    reply: Reply<Option<String>>,
                ) {
                    reply.send(Err(egui_charts::data::FeedError::Network("offline".into())));
                }
            }
            let source = engine.alerts().last().unwrap().source.clone();
            let mut reader = egui_charts::services::AlertFeed::new(
                feed.clone(),
                egui_charts::services::AlertFeed::history_request("OTHER", timeframe),
                None,
            );
            reader.set_storage(storage);
            for _ in 0..4 {
                reader.poll(terminal.ctx.as_ref().unwrap(), Utc::now());
            }
            assert_eq!(
                reader.drawing_value(&source, 2, egui_charts::scales::PriceScaleMode::Normal),
                Some(30.0)
            );
            reader.set_storage(Arc::new(Unavailable));
            for _ in 0..3 {
                reader.poll(terminal.ctx.as_ref().unwrap(), Utc::now());
            }
            assert_eq!(
                reader.drawing_value(&source, 2, egui_charts::scales::PriceScaleMode::Normal),
                None
            );

            // A shown drawing works without storage, including alongside independent events.
            for events in [false, true] {
                let engine = Arc::new(AlertEngine::new());
                let mut terminal = Terminal::new(feed.clone())
                    .with_symbol("OTHER")
                    .with_timeframe(timeframe)
                    .with_alerts(engine.clone());
                frames(&mut terminal);
                let mut line = Drawing::new(9, DrawingToolType::HorizontalLine);
                line.completed = true;
                line.chart_points = vec![ChartPoint::new(0.0, 30.0)];
                let mut alert = Alert::new("OTHER", AlertCondition::Above(25.0));
                alert.source = line.alert_source(0);
                if events {
                    let id = terminal.chart_mut().add_indicator(Box::new(
                        egui_charts::studies::OrderFlowStudy::new(
                            egui_charts::data::OrderFlowKind::Spread,
                        ),
                    ));
                    alert
                        .additional_conditions
                        .push(egui_charts::model::AlertRule {
                            source: AlertSource::Study {
                                id: id.0.to_string(),
                                output: 0,
                            },
                            condition: AlertCondition::Above(1.0),
                        });
                }
                terminal.chart_mut().drawings_mut().drawings.push(line);
                terminal.run(Action::AlertDialog(AlertDialogAction::Create(Box::new(
                    alert,
                ))));
                frames(&mut terminal);
                if events {
                    let last = terminal
                        .chart()
                        .bars(egui_charts::widget::SeriesId::MAIN)
                        .last()
                        .unwrap();
                    for subscription in feed
                        .subscriptions
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|subscription| subscription.symbol == "OTHER")
                    {
                        subscription.sink.push_bar(last.clone());
                        subscription
                            .sink
                            .push_quote(egui_charts::model::Level1Quote {
                                ts: last.time,
                                symbol: "OTHER".into(),
                                bid: 10.0,
                                ask: 40.0,
                                bid_size: 1.0,
                                ask_size: 1.0,
                            });
                    }
                    frames(&mut terminal);
                }
                assert_eq!(
                    engine.alerts()[0].status,
                    AlertStatus::Triggered,
                    "live drawings need no storage service, events: {events}, interval: {timeframe}"
                );
            }
        }
    }
}
