//! A host-owned live run, fed by the displayed chart and delivered through the alert service.

use super::{DemoApp, SeriesId};
use egui_charts::model::{Alert, AlertCondition, AlertId, AlertStatus, OrderSide, OrderType};
use egui_charts::strategies::{Backtest, Strategy, StrategyContext};

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
            context.entry("Long", OrderSide::Buy, 2.0, OrderType::Market);
        } else if bar.close < previous.close && context.position() >= 0.0 {
            context.entry("Short", OrderSide::Sell, 2.0, OrderType::Market);
        }
    }
}

impl DemoApp {
    pub(super) fn start_live_strategy(&mut self) {
        self.stop_live_strategy();
        let bars = self.chart.bars(SeriesId::MAIN);
        let Some(first) = bars.get(bars.len().saturating_sub(121)) else {
            return;
        };
        let id = format!("Live momentum {}", chrono::Utc::now().to_rfc3339());
        let mut run = Backtest::new(
            "Live momentum (synthetic data)",
            self.chart.symbol(),
            self.state.timeframe,
            10_000.0,
        );
        run.settings.start_time = Some(first.time);
        run.settings.recalculate_on_every_update = true;
        run.settings.currency = self
            .chart
            .instrument()
            .and_then(|instrument| instrument.currency.clone());
        let source = &bars[bars.partition_point(|bar| bar.time < first.time)..];
        // Prime history before arming the alert, so only subsequent executions notify.
        run.update(&mut Momentum, source, &[]);
        let mut alert = Alert::new(&run.symbol, AlertCondition::StrategyFills(id.clone()));
        alert.id = Some(AlertId(id.clone()));
        alert.name = "Live strategy fills".into();
        alert.timeframe = Some(run.timeframe);
        self.alerts.save(alert);
        self.strategy_session = self.timeframe_toolbar.state.session_type;
        self.strategy_trade = None;
        self.live_strategy = Some((run, id));
        self.update_live_strategy();
    }

    pub(super) fn stop_live_strategy(&mut self) {
        if let Some((_, id)) = self.live_strategy.take()
            && let Some(mut alert) = self
                .alert_host
                .engine()
                .alerts()
                .into_iter()
                .find(|alert| alert.id.as_ref() == Some(&AlertId(id.clone())))
        {
            alert.status = AlertStatus::Disabled;
            self.alerts.save(alert);
        }
    }

    pub(super) fn update_live_strategy(&mut self) {
        let Some((run, id)) = &mut self.live_strategy else {
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
        let fills = run.update(&mut Momentum, source, &[]);
        self.alert_host.engine().strategy_fills(
            id,
            &run.symbol,
            self.chart
                .instrument()
                .and_then(|instrument| instrument.exchange.as_deref()),
            run.timeframe,
            &fills,
            chrono::Utc::now(),
        );
        if self.strategy_report.report() != run.report() {
            self.strategy_report
                .set_report(run.report().cloned(), source);
            self.refresh_strategy_signals();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_charts::model::{BarClock, Timeframe};

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
        app.start_live_strategy();
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
        let mut reference = app.live_strategy.clone().unwrap().0;
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
            assert_eq!(app.alerts.log().len(), expected, "second {second}");
        }
        assert!(expected > 0, "the live feed must exercise executions");
        assert_eq!(app.strategy_report.report(), reference.report());
        app.stop_live_strategy();
        assert!(app.live_strategy.is_none());
        assert_eq!(
            app.alert_host.engine().alerts()[0].status,
            AlertStatus::Disabled
        );
    }
}
