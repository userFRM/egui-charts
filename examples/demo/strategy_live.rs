//! A host-owned live run, fed by the displayed chart and delivered through the alert service.

use super::{DemoApp, SeriesId};
use egui_charts::model::{
    Alert, AlertCondition, AlertFrequency, AlertId, AlertStatus, CustomAlertEvent, OrderSide,
    OrderType,
};
use egui_charts::strategies::{
    Backtest, BacktestSettings, Strategy, StrategyContext, StrategyFactory,
};
use egui_charts::studies::InputValue;

/// Ends the id of a run in the name of its custom event source, which raises an alert call each
/// time the run's position turns, and in the id of that source's alert.
const TURNS: &str = ": turns";

struct Momentum;

impl Strategy for Momentum {
    fn on_bar(&mut self, context: &mut StrategyContext<'_>) {
        if context.is_after_fill() {
            return;
        }
        let [.., previous, bar] = context.bars() else {
            return;
        };
        if bar.close > previous.close && context.position() <= 0.0 {
            context.entry("Long", OrderSide::Buy, None, OrderType::Market);
        } else if bar.close < previous.close && context.position() >= 0.0 {
            context.entry("Short", OrderSide::Sell, None, OrderType::Market);
        }
    }
}

/// A live run, the strategy instance that continues it, and the id its alerts name.
pub(super) struct LiveStrategy {
    pub(super) run: Backtest,
    pub(super) id: String,
    script: Box<dyn Strategy>,
    /// Cancelling the pending Add alert dialog stops this run.
    awaiting_alert: bool,
}

impl DemoApp {
    /// Runs the demo's momentum strategy with the tester's settings and its own two alerts.
    pub(super) fn start_live_strategy(&mut self) {
        let id = format!("Live momentum {}", chrono::Utc::now().to_rfc3339());
        let settings = self.strategy_report.settings().clone();
        if !self.start_live_run(
            id.clone(),
            "Live momentum (synthetic data)",
            settings,
            Box::new(Momentum),
        ) {
            return;
        }
        let (symbol, timeframe) = match &self.live_strategy {
            Some(live) => (live.run.symbol.clone(), live.run.timeframe),
            None => return,
        };

        // The run's fills and its alert calls. Both are named after the run, so the alerts of a
        // run that ended never fire again.
        let turns = format!("{id}{TURNS}");
        self.alert_host
            .engine()
            .connect_custom_events(turns.clone());
        for (key, condition, name) in [
            (
                id.clone(),
                AlertCondition::StrategyFills(id.clone()),
                "Live strategy fills",
            ),
            (
                turns.clone(),
                AlertCondition::CustomEvents(turns),
                "Live strategy turns",
            ),
        ] {
            let mut alert = Alert::new(&symbol, condition);
            alert.id = Some(AlertId(key));
            alert.name = name.into();
            alert.timeframe = Some(timeframe);
            self.alerts.save(alert);
        }
    }

    /// Primes on the latest 121 chart bars and continues without an end, replacing any live run.
    /// Returns false when the chart has no bars.
    fn start_live_run(
        &mut self,
        id: String,
        name: &str,
        settings: BacktestSettings,
        mut script: Box<dyn Strategy>,
    ) -> bool {
        self.stop_live_strategy();
        let bars = self.chart.bars(SeriesId::MAIN);
        let Some(first) = bars.get(bars.len().saturating_sub(121)) else {
            return false;
        };
        let mut run = Backtest::new(
            name,
            self.chart.symbol(),
            self.state.timeframe,
            settings.initial_capital,
        );
        run.settings = settings;
        // A live run primes on the latest 121 bars and runs on without an end.
        run.settings.start_time = Some(first.time);
        run.settings.end_time = None;
        run.instrument_currency = self
            .chart
            .instrument()
            .and_then(|instrument| instrument.currency.clone());
        let source = &bars[bars.partition_point(|bar| bar.time < first.time)..];
        // Prime history before any alert watches the run, so only subsequent executions notify.
        run.update(script.as_mut(), source, &[]);
        self.strategy_session = self.timeframe_toolbar.state.session_type;
        self.strategy_trade = None;
        self.live_strategy = Some(LiveStrategy {
            run,
            id,
            script,
            awaiting_alert: false,
        });
        self.update_live_strategy();
        true
    }

    /// Runs the tester's strategy live and opens its fill-alert dialog; Cancel stops the run.
    pub(super) fn add_strategy_alert(
        &mut self,
        strategy: &str,
        inputs: Vec<(String, InputValue)>,
        settings: BacktestSettings,
    ) {
        if self.replay.is_some() {
            self.notify("Stop replay to add a strategy alert");
            return;
        }
        // The demo's tester uses the default factory, so its names resolve here.
        let Some(mut script) = StrategyFactory::new().create(strategy) else {
            return;
        };
        for (input, value) in inputs {
            if let Err(error) = script.set_input(&input, value) {
                self.notify(format!("{strategy}: {error}"));
                return;
            }
        }
        let id = format!("{strategy} {}", chrono::Utc::now().to_rfc3339());
        if !self.start_live_run(id.clone(), &format!("{strategy} live"), settings, script) {
            return;
        }
        self.open_alert(None);
        if !self.alert_dialog.is_open {
            self.stop_live_strategy();
            return;
        }
        self.alert_dialog.alert.condition = AlertCondition::StrategyFills(id);
        self.alert_dialog.alert.frequency = AlertFrequency::EveryTime;
        if let Some(live) = &mut self.live_strategy {
            live.awaiting_alert = true;
        }
    }

    /// Stops only a run whose pending Add alert dialog was cancelled.
    pub(super) fn strategy_alert_closed(&mut self, saved: bool) {
        let Some(live) = self
            .live_strategy
            .as_mut()
            .filter(|live| live.awaiting_alert)
        else {
            return;
        };
        live.awaiting_alert = false;
        if !saved {
            self.stop_live_strategy();
        }
    }

    pub(super) fn stop_live_strategy(&mut self) {
        let Some(LiveStrategy { id, .. }) = self.live_strategy.take() else {
            return;
        };
        for mut alert in self.alert_host.engine().alerts() {
            if alert.id.as_ref().is_some_and(|key| key.0.starts_with(&id))
                || matches!(&alert.condition, AlertCondition::StrategyFills(run) if *run == id)
            {
                alert.status = AlertStatus::Disabled;
                self.alerts.save(alert);
            }
        }
    }

    pub(super) fn update_live_strategy(&mut self) {
        let Some(LiveStrategy {
            run, id, script, ..
        }) = &mut self.live_strategy
        else {
            return;
        };
        if self.replay.is_some()
            || self.chart.symbol() != run.symbol
            || self.chart.timeframe() != Some(run.timeframe)
            || self.timeframe_toolbar.state.session_type != self.strategy_session
        {
            return;
        }
        let bars = self.chart.bars(SeriesId::MAIN);
        let source = &bars[bars.partition_point(|bar| {
            run.settings
                .start_time
                .is_some_and(|start| bar.time < start)
        })..];
        let fills = run.update(script.as_mut(), source, &[]);
        let exchange = self
            .chart
            .instrument()
            .and_then(|instrument| instrument.exchange.as_deref());
        let now = chrono::Utc::now();
        let engine = self.alert_host.engine();
        engine.strategy_fills(id, &run.symbol, exchange, run.timeframe, &fills, now);
        let turns: Vec<_> = fills
            .iter()
            .filter_map(|fill| {
                let before = fill.position
                    - match fill.side {
                        OrderSide::Buy => fill.quantity,
                        OrderSide::Sell => -fill.quantity,
                    };
                let side = match fill.position {
                    position if position > 0.0 && before <= 0.0 => "long",
                    position if position < 0.0 && before >= 0.0 => "short",
                    _ => return None,
                };
                let mut event = CustomAlertEvent::new(
                    fill.sequence,
                    fill.time,
                    format!("Momentum turned {side} at {}", fill.price),
                );
                event.price = Some(fill.price);
                Some(event)
            })
            .collect();
        let raised = format!("{id}{TURNS}");
        engine.custom_events(&raised, &run.symbol, exchange, run.timeframe, &turns, now);
        if self.strategy_report.report() != run.report() {
            self.strategy_report
                .set_report(run.report().cloned(), source);
            self.refresh_strategy_signals();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ui_click, ui_frame};
    use super::*;
    use egui_charts::model::{Bar, BarClock, Timeframe};
    use egui_charts::strategies::Commission;

    /// Clicks the one text `label` that a frame of the strategy panel and the dialogs shows.
    fn click(app: &mut DemoApp, ctx: &egui::Context, label: &str) {
        let size = egui::vec2(1440.0, 900.0);
        let bg = app.theme.ui.panel_bg;
        let mut draw = |ui: &mut egui::Ui| {
            app.show_strategy_report(ui, bg);
            app.show_dialogs(ui.ctx());
        };
        for _ in 0..3 {
            ui_frame(ctx, size, vec![], &mut draw);
        }
        let found: Vec<_> = ui_frame(ctx, size, vec![], &mut draw)
            .into_iter()
            .filter(|(text, _)| text == label)
            .collect();
        let [(_, rect)] = found.as_slice() else {
            panic!("{label}: {found:?}");
        };
        ui_click(ctx, size, rect.center(), &mut draw);
    }

    #[test]
    fn tester_add_alert_runs_the_strategy_live_and_alerts_on_its_fills() {
        let ctx = egui::Context::default();
        let start = "2026-10-27T19:29:50Z".parse().unwrap();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            start,
            true,
        );
        app.switch_timeframe(Timeframe::Min1);
        let snapshot = |app: &DemoApp| {
            app.live.snapshot(
                Timeframe::Min1,
                BarClock::new(Timeframe::Min1),
                app.chart.extended_hours(),
            )
        };
        app.chart.set_data(SeriesId::MAIN, snapshot(&app));
        app.chart.set_feed_paused(true);
        app.run_sample_backtest();
        let mut settings = app.strategy_report.settings().clone();
        settings.commission = Commission::PerOrder(2.5);
        settings.pyramiding = 2;
        app.strategy_report.set_settings(settings.clone()).unwrap();
        // One close decides; pyramiding also distinguishes this strategy from Momentum.
        app.strategy_report
            .set_input("length", InputValue::Length(1))
            .unwrap();

        // A fill alert needs live data, so replay cannot start its run.
        app.start_replay_at(app.chart.bars(SeriesId::MAIN).len() - 1);
        click(&mut app, &ctx, "Add alert");
        assert!(
            !app.alert_dialog.is_open,
            "replay cannot start a live strategy alert"
        );
        assert!(app.live_strategy.is_none());
        assert!(app.alert_host.engine().alerts().is_empty());
        app.stop_replay();
        app.chart.set_data(SeriesId::MAIN, snapshot(&app));
        app.chart.set_feed_paused(true);

        click(&mut app, &ctx, "Add alert");
        assert!(app.alert_dialog.is_open, "Add alert opens the alert dialog");
        assert!(app.live_strategy.is_some());
        click(&mut app, &ctx, "Cancel");
        assert!(!app.alert_dialog.is_open);
        assert!(
            app.live_strategy.is_none(),
            "a cancelled dialog stops the run"
        );
        assert!(app.alert_host.engine().alerts().is_empty());

        click(&mut app, &ctx, "Add alert");
        let id = app.live_strategy.as_ref().expect("a live run").id.clone();
        assert_eq!(
            app.alert_dialog.strategy_runs,
            vec![(id.clone(), "Consecutive Up/Down live".to_owned())]
        );
        assert_eq!(
            app.alert_dialog.alert.condition,
            AlertCondition::StrategyFills(id.clone())
        );
        assert_eq!(app.alert_dialog.alert.frequency, AlertFrequency::EveryTime);
        click(&mut app, &ctx, "Save alert");
        assert!(!app.alert_dialog.is_open);
        assert!(app.live_strategy.is_some(), "a saved alert keeps its run");
        let [saved] = <[Alert; 1]>::try_from(app.alert_host.engine().alerts()).unwrap();
        assert_eq!(saved.condition, AlertCondition::StrategyFills(id.clone()));
        assert_eq!(saved.status, AlertStatus::Active);

        let run = &app.live_strategy.as_ref().unwrap().run;
        settings.start_time = run.settings.start_time;
        settings.end_time = None;
        assert_eq!(run.settings, settings);
        assert_eq!(run.symbol, app.chart.symbol());
        assert_eq!(Some(run.timeframe), app.chart.timeframe());
        let mut reference = Backtest::new(
            &run.name,
            &run.symbol,
            run.timeframe,
            run.settings.initial_capital,
        );
        reference.settings = run.settings.clone();
        reference.instrument_currency = run.instrument_currency.clone();
        let from = reference.settings.start_time.unwrap();
        let source = |bars: &[Bar]| bars[bars.partition_point(|bar| bar.time < from)..].to_vec();
        let mut script = StrategyFactory::new()
            .create("Consecutive Up/Down")
            .unwrap();
        script.set_input("length", InputValue::Length(1)).unwrap();
        // Primed on the bars the run primed on; that batch never notifies.
        reference.update(
            script.as_mut(),
            &source(app.chart.bars(SeriesId::MAIN)),
            &[],
        );
        // Editing the tester after Save leaves the live snapshot's inputs and settings intact.
        app.strategy_report
            .set_input("length", InputValue::Length(3))
            .unwrap();
        settings.commission = Commission::PerOrder(0.0);
        app.strategy_report.set_settings(settings).unwrap();
        let mut fills = Vec::new();
        for second in (10..=600).step_by(10) {
            app.advance_sample_to(start + chrono::TimeDelta::seconds(second));
            let bars = snapshot(&app);
            fills.extend(reference.update(script.as_mut(), &source(&bars), &[]));
            app.chart.set_data(SeriesId::MAIN, bars);
            app.update_live_strategy();
            app.alerts.poll(&ctx, &mut app.toasts);
        }
        assert!(!fills.is_empty(), "ten minutes of bars must fill orders");
        let triggers: Vec<_> = app
            .alerts
            .log()
            .iter()
            .filter(|trigger| Some(&trigger.id) == saved.id.as_ref())
            .collect();
        assert_eq!(triggers.len(), fills.len());
        for (trigger, fill) in triggers.iter().zip(&fills) {
            assert_eq!((trigger.time, trigger.price), (fill.time, Some(fill.price)));
            let side = match fill.side {
                OrderSide::Buy => "Buy ",
                OrderSide::Sell => "Sell ",
            };
            assert!(trigger.message.starts_with(side), "{}", trigger.message);
        }
        app.stop_live_strategy();
        assert_eq!(
            app.alert_host.engine().alerts()[0].status,
            AlertStatus::Disabled,
            "a stopped run's fill alert is disabled"
        );
    }

    #[test]
    fn demo_live_feed_delivers_one_alert_for_each_strategy_fill() {
        let ctx = egui::Context::default();
        let start = "2026-10-27T19:29:50Z".parse().unwrap();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            start,
            true,
        );
        app.switch_timeframe(Timeframe::Min1);
        let snapshot = |app: &DemoApp| {
            app.live.snapshot(
                Timeframe::Min1,
                BarClock::new(Timeframe::Min1),
                app.chart.extended_hours(),
            )
        };
        app.chart.set_data(SeriesId::MAIN, snapshot(&app));
        let mut settings = app.strategy_report.settings().clone();
        settings.commission = Commission::PerOrder(2.5);
        settings.recalculate_on_every_update = true;
        app.strategy_report.set_settings(settings).unwrap();
        app.start_live_strategy();
        let id = app.live_strategy.as_ref().unwrap().id.clone();
        app.chart.set_feed_paused(true);
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                app.chart.show(ui);
            })
            .drop_without_applying_deltas();
        }
        app.alerts.poll(&ctx, &mut app.toasts);
        assert!(
            app.alerts.log().is_empty(),
            "priming history does not notify"
        );
        let mut reference = app.live_strategy.as_ref().unwrap().run.clone();
        let mut expected = 0;
        for second in 1..=180 {
            app.advance_sample_to(start + chrono::TimeDelta::seconds(second));
            let bars = snapshot(&app);
            let source = &bars
                [bars.partition_point(|bar| bar.time < reference.settings.start_time.unwrap())..];
            expected += reference.update(&mut Momentum, source, &[]).len();
            app.chart.set_data(SeriesId::MAIN, bars);
            app.update_live_strategy();
            app.update_live_strategy();
            app.alerts.poll(&ctx, &mut app.toasts);
            let fills = app.alerts.log().iter().filter(|t| t.id.0 == id).count();
            assert_eq!(fills, expected, "second {second}");
        }
        assert!(expected > 0, "the live feed must exercise executions");
        let report = reference.report().unwrap();
        // A reversal splits one order's fee between its closed and newly opened trades.
        assert_eq!(
            report.trades.iter().map(|trade| trade.fees).sum::<f64>(),
            2.5 * report.trades.len() as f64
        );
        // The momentum run takes the side of each bar's move, so its fills turn the position
        // and every turn is an alert call of its own after the fill it follows.
        let turns: Vec<_> = app
            .alerts
            .log()
            .iter()
            .filter(|t| t.id.0 == format!("{id}{TURNS}"))
            .collect();
        assert!(!turns.is_empty(), "the run must turn its position");
        assert!(
            turns
                .iter()
                .all(|t| t.message.starts_with("Momentum turned ") && t.symbol == reference.symbol)
        );
        assert_eq!(app.strategy_report.report(), reference.report());
        // The dialog offers a Forward test run; cancelling an unrelated dialog keeps it.
        app.open_alert(None);
        assert_eq!(
            app.alert_dialog.strategy_runs,
            vec![(id.clone(), "Live momentum (synthetic data)".to_owned())]
        );
        click(&mut app, &ctx, "Cancel");
        assert!(app.live_strategy.is_some());
        app.stop_live_strategy();
        assert!(app.live_strategy.is_none());
        let alerts = app.alert_host.engine().alerts();
        assert_eq!(alerts.len(), 2);
        assert!(alerts.iter().all(|a| a.status == AlertStatus::Disabled));

        let mut settings = app.strategy_report.settings().clone();
        settings.currency = Some("EUR".into());
        app.strategy_report.set_settings(settings).unwrap();
        app.start_live_strategy();
        let run = &app.live_strategy.as_ref().unwrap().run;
        let report = run.report().unwrap();
        assert_eq!(run.instrument_currency.as_deref(), Some("USD"));
        assert_eq!(report.currency.as_deref(), Some("EUR"));
        assert_eq!(
            report.unavailable.as_deref(),
            Some("Exchange rates from USD to EUR are unavailable")
        );
        assert!(report.trades.is_empty() && report.equity.is_empty());
    }
}
