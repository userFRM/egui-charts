//! A host-run sample backtest and the report's navigation, signals and CSV delivery.

use super::{Bar, DemoApp, SeriesId, Timeframe, panel_frame};
use egui_charts::model::{
    BarClock, OrderSide, SignalId, StrategyExit, StrategyReport, StrategyTrade,
};
use egui_charts::services::{Events, SignalEvent, Signals};
use egui_charts::ui::strategy_report::{
    StrategyHistoryRequest, StrategyReportAction, StrategyTesterAction,
};
use std::sync::Arc;

/// Buys two units every 40 bars, holds for 20 and pays one currency unit per fill.
/// Fills use opens; equity uses completed closes at their source bar timestamps.
fn sample_backtest(symbol: &str, timeframe: Timeframe, bars: &[Bar]) -> StrategyReport {
    let mut report =
        StrategyReport::new("20-bar holds (synthetic data)", symbol, timeframe, 10_000.0);
    report.properties = vec![
        ("Quantity".into(), "2 units".into()),
        ("Holding / cycle".into(), "20 / 40 bars".into()),
        ("Commission".into(), "1 account unit per fill".into()),
        (
            "Equity sampling".into(),
            "Source closes, after fills".into(),
        ),
    ];
    for start in (0..bars.len()).step_by(40) {
        let Some(exit) = bars.get(start + 20) else {
            break;
        };
        let entry = &bars[start];
        let mut trade = StrategyTrade::new(OrderSide::Buy, 2.0, entry.time, entry.open);
        trade.fees = 2.0;
        trade.exit = Some(StrategyExit::new(
            exit.time,
            exit.open,
            (exit.open - entry.open) * 2.0 - trade.fees,
        ));
        report.trades.push(trade);
    }
    if let Some(first) = bars.first() {
        // Capital immediately before the first fill keeps its fee in drawdown.
        report.equity.push((
            first.time - chrono::TimeDelta::nanoseconds(1),
            report.initial_capital,
        ));
    }
    for bar in bars {
        let equity = report
            .trades
            .iter()
            .fold(report.initial_capital, |equity, trade| {
                let exit = trade.exit.as_ref().expect("sample round trip");
                equity
                    + if bar.time >= exit.time {
                        exit.profit
                    } else if bar.time >= trade.entry_time {
                        (bar.close - trade.entry_price) * trade.quantity - 1.0
                    } else {
                        0.0
                    }
            });
        report.equity.push((bar.time, equity));
    }
    report
}

impl DemoApp {
    /// Supplies completed bars on the chart's session and adjustment basis, with warm-up history.
    fn strategy_history(
        &self,
        request: &StrategyHistoryRequest,
    ) -> Result<(Vec<Bar>, Vec<Bar>), String> {
        if self.replay.is_some() {
            return Err("Stop replay before requesting synthetic backtest history.".into());
        }
        if request.symbol != self.chart.symbol()
            || Some(request.timeframe) != self.chart.timeframe()
        {
            return Err("The requested strategy symbol or interval is no longer selected.".into());
        }
        if self.live.loading() {
            return Err(
                "Synthetic history is still loading. Retry when chart loading finishes.".into(),
            );
        }
        let clock_for = |timeframe| {
            let mut clock = BarClock::new(timeframe);
            if self.chart.bar_clock().has_session() {
                clock = clock.with_session(
                    crate::sampledata::us_equity_zone(),
                    crate::sampledata::us_equity_session(),
                    self.chart.extended_hours(),
                );
            }
            clock
        };
        let history = |timeframe: Timeframe, fills: bool| {
            if timeframe.is_trade_based() {
                return Err("Synthetic backtest history requires a time interval.".to_owned());
            }
            let clock = clock_for(timeframe);
            let mut bars =
                self.live
                    .snapshot(timeframe, clock.clone(), self.chart.extended_hours());
            bars.retain(|bar| bar.time < clock.bar_open(self.live.now()));
            let label = if fills { "Finer fill" } else { "Decision" };
            let first = bars.first().ok_or_else(|| {
                format!("{label} history has no completed {timeframe} bars in this session.")
            })?;
            if request.start < first.time {
                return Err(format!(
                    "{label} history at {timeframe} starts {}. Choose a later start or a longer interval.",
                    first.time.format("%Y-%m-%d %H:%M:%S UTC")
                ));
            }
            if bars.last().is_none_or(|bar| bar.time < request.start) {
                return Err("The start date is after the last completed synthetic bar.".into());
            }
            Ok(bars)
        };
        let bars = history(request.timeframe, false)?;
        let lower = match request.lower_timeframe {
            Some(timeframe)
                if timeframe.duration_ms() > 0
                    && timeframe.duration_ms() < request.timeframe.duration_ms() =>
            {
                let lower = history(timeframe, true)?;
                if egui_charts::data::resample(
                    &lower,
                    clock_for(timeframe),
                    clock_for(request.timeframe),
                )
                .is_none()
                {
                    return Err("Finer fill bars must fit completely inside decision bars. Choose a dividing interval.".into());
                }
                lower
            }
            Some(_) => {
                return Err(
                    "Choose a time-based fill interval shorter than the decision interval.".into(),
                );
            }
            None => Vec::new(),
        };
        Ok((bars, lower))
    }

    pub(super) fn run_sample_backtest(&mut self) {
        self.stop_live_strategy();
        let bars = self.chart.bars(SeriesId::MAIN);
        // The newest bar can still change. Bound this synchronous demonstration to 121 opens.
        let completed = &bars[..bars.len().saturating_sub(1)];
        let source = &completed[completed.len().saturating_sub(121)..];
        if source.len() < 21 {
            self.notify("Sample backtest needs at least 21 completed bars");
            return;
        }
        let mut report = sample_backtest(&self.state.symbol, self.state.timeframe, source);
        report.currency = self.chart.instrument().and_then(|i| i.currency.clone());
        if let Some(tick) = self.chart.instrument().and_then(|i| i.tick_size) {
            let mut settings = self.strategy_report.settings().clone();
            settings.price_increment = tick;
            if let Err(error) = self.strategy_report.set_settings(settings) {
                log::warn!("Cannot use the instrument's price increment: {error}");
            }
        }
        self.strategy_report.set_report(Some(report), source);
        self.strategy_report
            .select_strategy("Consecutive Up/Down")
            .expect("built-in strategy");
        self.strategy_session = self.timeframe_toolbar.state.session_type;
        self.strategy_trade = None;
        self.refresh_strategy_signals();
    }

    pub(super) fn show_strategy_report(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        self.update_live_strategy();
        if self.strategy_report.report().is_none() {
            return;
        }
        let chart_type = self.chart.chart_type();
        if self.live_strategy.is_none() && self.strategy_report.settings().chart_type != chart_type
        {
            let mut settings = self.strategy_report.settings().clone();
            settings.chart_type = chart_type;
            settings.recalculate_on_history &= !chart_type.transforms_data();
            if let Err(error) = self.strategy_report.set_settings(settings) {
                log::warn!("Cannot update strategy candles: {error}");
            }
        }
        let mut clear = false;
        let action = egui::Panel::bottom("strategy_report")
            .resizable(true)
            .default_size(330.0)
            .size_range(100.0..=(ui.available_height() * 0.7).max(100.0))
            .frame(panel_frame(bg))
            .show(ui, |ui| {
                ui.take_available_space();
                ui.horizontal_wrapped(|ui| {
                    clear = ui.button("Clear report").clicked();
                    if self.live_strategy.is_some() {
                        if ui.button("Stop forward test").clicked() {
                            self.stop_live_strategy();
                        }
                    } else if ui.button("Forward test").clicked() {
                        self.start_live_strategy();
                    }
                });
                if self.live_strategy.is_some() {
                    return vec![StrategyTesterAction::Report(
                        self.strategy_report.show_report(ui),
                    )];
                }
                let bars = self.chart.bars(SeriesId::MAIN);
                self.strategy_report.show(
                    ui,
                    &self.state.symbol,
                    self.state.timeframe,
                    &bars[..bars.len().saturating_sub(1)],
                )
            })
            .inner;
        if self.live_strategy.is_none() && self.strategy_report.settings().chart_type != chart_type
        {
            let kind = self.strategy_report.settings().chart_type;
            self.state.chart_type = kind;
            self.top_toolbar.chart_type_selector.current = kind;
            self.chart.set_chart_type(kind);
            self.size_price_based_charts();
        }
        if clear {
            self.stop_live_strategy();
            self.strategy_report.set_report(None, &[]);
            self.strategy_trade = None;
            self.refresh_strategy_signals();
        } else {
            for action in action {
                match action {
                    StrategyTesterAction::Report(action) => self.handle_strategy_action(action),
                    StrategyTesterAction::Signals { .. } => {
                        self.strategy_session = self.timeframe_toolbar.state.session_type;
                        self.strategy_trade = None;
                        self.refresh_strategy_signals();
                    }
                    StrategyTesterAction::RequestHistory(request) => {
                        match self.strategy_history(&request) {
                            Ok((bars, lower)) => {
                                if let Err(error) =
                                    self.strategy_report.set_history(&request, bars, lower)
                                {
                                    self.strategy_report.history_failed(&request, error);
                                }
                            }
                            Err(error) => {
                                self.strategy_report.history_failed(&request, error);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_strategy_action(&mut self, action: StrategyReportAction) {
        match action {
            StrategyReportAction::SelectTrade(trade) => {
                self.stop_replay();
                if self.state.symbol != trade.symbol {
                    self.switch_symbol(&trade.symbol);
                }
                // A symbol switch resets the session; the trade's bar may only exist in the report's.
                if self.timeframe_toolbar.state.session_type != self.strategy_session {
                    self.timeframe_toolbar.set_session(self.strategy_session);
                    self.apply_session();
                }
                if self.state.timeframe != trade.timeframe {
                    self.switch_timeframe(trade.timeframe);
                }
                self.chart.scroll_to_time(trade.entry_time);
                self.strategy_trade = Some(trade.trade_index);
                self.refresh_strategy_signals();
            }
            StrategyReportAction::ExportCsv(csv) => self.save_text("strategy-trades.csv", &csv),
            StrategyReportAction::ExportSummaryCsv { filename, content } => {
                self.save_text(&filename, &content)
            }
            _ => {}
        }
    }

    pub(super) fn highlight_strategy_trade(&self, ui: &egui::Ui) {
        let Some(report) = self.strategy_report.report().filter(|report| {
            report.symbol == self.chart.symbol() && Some(report.timeframe) == self.chart.timeframe()
        }) else {
            return;
        };
        let Some(trade) = self.strategy_trade.and_then(|i| report.trades.get(i)) else {
            return;
        };
        let Some(from) = self.chart.time_to_index(trade.entry_time) else {
            return;
        };
        let mapping = self.chart.get_chart_mapping();
        let Some(left) = self.chart.index_to_x(from) else {
            return;
        };
        let right = trade
            .exit
            .as_ref()
            .and_then(|exit| self.chart.time_to_index(exit.time))
            .and_then(|index| self.chart.index_to_x(index))
            .unwrap_or(mapping.rect.right());
        let rect = egui::Rect::from_min_max(
            egui::pos2(left, mapping.rect.top()),
            egui::pos2(right, mapping.rect.bottom()),
        )
        .intersect(mapping.rect);
        if rect.is_positive() {
            ui.painter()
                .rect_filled(rect, 0.0, self.theme.ui.accent.linear_multiply(0.12));
        }
    }

    /// Keep the existing live analytics when a finished report also supplies fill markers.
    pub(super) fn refresh_strategy_signals(&mut self) {
        for (chart, live) in std::iter::once((&mut self.chart, &self.live)).chain(
            self.extra_charts
                .iter_mut()
                .map(|extra| (&mut extra.chart, &extra.live)),
        ) {
            let live: Arc<dyn Signals> = if self.api_samples {
                live.clone()
            } else {
                live.detectors()
            };
            let source: Arc<dyn Signals> = match self.strategy_report.report() {
                Some(report) => Arc::new(ReportSignals {
                    report: report.clone(),
                    live,
                }),
                None => live,
            };
            chart.set_signal_source(Some(source));
        }
    }
}

struct ReportSignals {
    report: StrategyReport,
    live: Arc<dyn Signals>,
}

impl Signals for ReportSignals {
    fn subscribe(&self, symbol: &str, timeframe: Timeframe, events: Events<SignalEvent>) {
        self.live.subscribe(symbol, timeframe, events.clone());
        if self.report.symbol != symbol || self.report.timeframe != timeframe {
            return;
        }
        for signal in self.report.signals() {
            events.push(SignalEvent::Upsert(signal));
        }
    }

    fn clicked(&self, id: &SignalId) {
        self.live.clicked(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, FullOutput, PointerButton, Pos2, RawInput, Rect, Shape};

    fn source(slope: f64, count: usize) -> Vec<Bar> {
        (0..count)
            .map(|i| {
                let price = 100.0 + slope * i as f64;
                Bar::new(
                    chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(i as i64),
                    price,
                    price + 1.0,
                    price - 1.0,
                    price + 0.5,
                    100.0,
                )
            })
            .collect()
    }

    #[test]
    fn sample_equity_marks_fees_fills_and_flat_periods() {
        // A 20-point rise at quantity 2 earns 40 less 2 fees; a 5-point fall loses 12.
        // Each held close is 0.5 above its open, adding 1 to equity before exit.
        for (slope, profit, marks) in [
            (
                1.0,
                114.0,
                [
                    10000.0, 10002.0, 10038.0, 10038.0, 10038.0, 10038.0, 10076.0, 10076.0,
                    10076.0, 10076.0, 10114.0, 10114.0, 10114.0,
                ],
            ),
            (
                -0.25,
                -36.0,
                [
                    10000.0, 9999.5, 9990.5, 9988.0, 9988.0, 9988.0, 9978.5, 9976.0, 9976.0,
                    9976.0, 9966.5, 9964.0, 9964.0,
                ],
            ),
        ] {
            let bars = source(slope, 121);
            let report = sample_backtest("SAMPLE", Timeframe::Min1, &bars);
            let performance = report.performance();
            assert_eq!(performance.net_profit, profit);
            assert_eq!(performance.closed_trades, 3);
            assert_eq!(performance.total_fees, 6.0);
            assert_eq!(performance.skipped_equity_points, 0);
            assert_eq!(report.equity_curve().len(), 122);
            assert_eq!(report.equity[0].1, 10_000.0);
            for (index, expected) in [0, 1, 19, 20, 39, 40, 59, 60, 79, 80, 99, 100, 120]
                .into_iter()
                .zip(marks)
            {
                assert_eq!(report.equity[index + 1], (bars[index].time, expected));
            }
            for (length, trades) in [(0, 0), (20, 0), (21, 1), (40, 1), (60, 1), (61, 2)] {
                assert_eq!(
                    sample_backtest("SAMPLE", Timeframe::Min1, &bars[..length])
                        .trades
                        .len(),
                    trades
                );
            }
        }
    }

    fn text_at(output: &FullOutput, label: &str) -> Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing {label}"))
    }

    #[test]
    fn requested_history_runs_and_failures_keep_the_completed_report() {
        let ctx = egui::Context::default();
        let now = "2026-11-03T15:00:00Z".parse().unwrap();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        app.switch_timeframe(Timeframe::Hour1);
        super::super::tests::settle(&mut app, &ctx);
        app.run_sample_backtest();
        let frame = |app: &mut DemoApp, events| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                    events,
                    ..Default::default()
                },
                |ui| app.show_strategy_report(ui, app.theme.ui.panel_bg),
            )
        };
        let click = |app: &mut DemoApp, label| {
            let output = frame(app, vec![]);
            let pos = text_at(&output, label);
            output.drop_without_applying_deltas();
            for pressed in [true, false] {
                frame(
                    app,
                    vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                )
                .drop_without_applying_deltas();
            }
        };
        let start = now - chrono::TimeDelta::days(2);
        let mut settings = app.strategy_report.settings().clone();
        settings.start_time = Some(start);
        app.strategy_report.set_settings(settings.clone()).unwrap();
        for _ in 0..3 {
            frame(&mut app, vec![]).drop_without_applying_deltas();
        }
        click(&mut app, "Run");
        for _ in 0..100 {
            frame(&mut app, vec![]).drop_without_applying_deltas();
        }
        assert!(
            !app.strategy_report.is_stale(),
            "supported history must complete the requested run"
        );
        let report = app.strategy_report.report().unwrap().clone();
        assert!(report.trades.iter().all(|trade| trade.entry_time >= start));
        assert_eq!(report.equity.first().unwrap().0, start);
        assert_eq!(report.equity.len(), 49);
        assert_eq!(report.equity.last().unwrap().0, now);
        // The finer sample is the same minute source aggregated into decision-hour OHLCV.
        let mut actions = Vec::new();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            actions = app
                .strategy_report
                .show(ui, "SAMPLE", Timeframe::Hour1, &[]);
        });
        let pos = text_at(&output, "Run");
        output.drop_without_applying_deltas();
        for pressed in [true, false] {
            ctx.run_ui(
                RawInput {
                    events: vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    actions = app
                        .strategy_report
                        .show(ui, "SAMPLE", Timeframe::Hour1, &[]);
                },
            )
            .drop_without_applying_deltas();
        }
        let mut request = actions
            .into_iter()
            .find_map(|action| match action {
                StrategyTesterAction::RequestHistory(request) => Some(request),
                _ => None,
            })
            .expect("tester requests host history");
        request.lower_timeframe = Some(Timeframe::Min1);
        let (bars, lower) = app.strategy_history(&request).unwrap();
        let hours = egui_charts::data::resample(&lower, Timeframe::Min1, Timeframe::Hour1).unwrap();
        assert_eq!(&bars[1..], &hours[1..]);
        assert!(lower.len() > bars.len() * 50);
        request.lower_timeframe = Some(Timeframe::Sec1);
        assert!(
            app.strategy_history(&request)
                .unwrap_err()
                .contains("Finer fill history")
        );
        settings.start_time = Some("1990-01-01T00:00:00Z".parse().unwrap());
        app.strategy_report.set_settings(settings).unwrap();
        click(&mut app, "Run");
        let output = frame(&mut app, vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, Shape::Text(text) if text.galley.text().starts_with("Decision history at"))));
        output.drop_without_applying_deltas();
        assert_eq!(app.strategy_report.report(), Some(&report));
    }

    fn fills_visible(
        output: &FullOutput,
        chart: &egui_charts::Chart,
        trade: &StrategyTrade,
    ) -> [bool; 2] {
        let exit = trade.exit.as_ref().unwrap();
        [
            (trade.entry_time, trade.entry_price),
            (exit.time, exit.price),
        ]
        .map(|(time, price)| {
            let Some(index) = chart.time_to_index(time) else {
                return false;
            };
            let Some(x) = chart.index_to_x(index.floor()) else {
                return false;
            };
            let point = Pos2::new(x, chart.get_chart_mapping().price_to_y(price));
            output.shapes.iter().any(|shape| {
                let bounds = shape.shape.visual_bounding_rect();
                matches!(&shape.shape, Shape::Path(marker)
                    if marker.closed && marker.points.len() == 3
                        && marker.stroke.width == 1.0
                        && (marker.points[0].x - point.x).abs() < 0.1
                        // Sample bar marks are triangles of the same stroke at the same bars;
                        // a fill marker is the narrower one.
                        && (marker.points[2].x - marker.points[1].x - 10.0).abs() < 0.01)
                    && bounds.width() <= 16.0
                    && bounds.height() <= 16.0
                    && bounds.expand(0.1).contains(point)
            })
        })
    }

    #[test]
    fn report_host_selects_navigates_exports_replaces_and_clears() {
        for theme in [super::super::Theme::light(), super::super::Theme::dark()] {
            for width in [640.0, 1440.0] {
                let ctx = egui::Context::default();
                let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
                super::super::tests::settle(&mut app, &ctx);
                app.theme = theme.clone();
                egui_charts::theme::apply_to_egui(&ctx, &theme);
                app.live.set_paused(true);
                app.chart.set_feed_paused(true);
                // Detector triangles can share a fill's price; this test identifies report markers.
                for kind in egui_charts::services::DetectionKind::ALL {
                    app.chart
                        .set_signal_category_visible(kind.category(), false);
                }
                let bars = app.chart.bars(SeriesId::MAIN).to_vec();
                let live_marker = |output: &FullOutput| {
                    output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, Shape::Circle(circle)
                        if circle.radius == 4.0 && circle.fill == theme.ui.accent)
                    })
                };
                let frame = |app: &mut DemoApp, events| {
                    super::super::tests::finish_feed(&app.live);
                    for extra in &app.extra_charts {
                        super::super::tests::finish_feed(&extra.live);
                    }
                    ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                egui::vec2(width, 900.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            app.show_strategy_report(ui, theme.ui.panel_bg);
                            app.show_chart(ui, theme.chart.bg);
                        },
                    )
                };
                let click = |app: &mut DemoApp, pos| {
                    for pressed in [true, false] {
                        frame(
                            app,
                            vec![
                                Event::PointerMoved(pos),
                                Event::PointerButton {
                                    pos,
                                    button: PointerButton::Primary,
                                    pressed,
                                    modifiers: Default::default(),
                                },
                            ],
                        )
                        .drop_without_applying_deltas();
                    }
                };
                let empty = frame(&mut app, vec![]);
                assert!(!empty.shapes.iter().any(
                    |s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == "Clear report")
                ));
                empty.drop_without_applying_deltas();
                app.execute_search_command(&ctx, "backtest");
                for _ in 0..3 {
                    frame(&mut app, vec![]).drop_without_applying_deltas();
                }
                let output = frame(&mut app, vec![]);
                let forward = text_at(&output, "Forward test");
                output.drop_without_applying_deltas();
                click(&mut app, forward);
                assert!(app.live_strategy.is_some());
                for _ in 0..3 {
                    frame(&mut app, vec![]).drop_without_applying_deltas();
                }
                let output = frame(&mut app, vec![]);
                assert!(
                    !output
                        .shapes
                        .iter()
                        .any(|shape| matches!(&shape.shape, Shape::Text(text)
                            if text.galley.text() == "Strategy tester")),
                    "live refreshes must not offer tester jobs that they would cancel"
                );
                let stop = text_at(&output, "Stop forward test");
                output.drop_without_applying_deltas();
                click(&mut app, stop);
                assert!(app.live_strategy.is_none());
                let output = frame(&mut app, vec![]);
                text_at(&output, "Strategy tester");
                output.drop_without_applying_deltas();
                app.execute_search_command(&ctx, "backtest");
                let report = app
                    .strategy_report
                    .report()
                    .expect("host mounts finished report")
                    .clone();
                assert_eq!(report.trades.len(), 3);
                assert_eq!(
                    report.trades.last().unwrap().exit.as_ref().unwrap().time,
                    bars[bars.len() - 122 + 100].time
                );
                assert_eq!(report.equity.last().unwrap().0, bars[bars.len() - 2].time);
                for _ in 0..3 {
                    frame(&mut app, vec![]).drop_without_applying_deltas();
                }
                let output = frame(&mut app, vec![]);
                let trades_tab = text_at(&output, "List of trades");
                let last = report.trades.last().unwrap();
                assert_eq!(fills_visible(&output, &app.chart, last), [true, true]);
                output.drop_without_applying_deltas();
                click(&mut app, trades_tab);
                // Selection must restore the report scope after unrelated navigation.
                app.switch_symbol("BTCUSD");
                app.switch_timeframe(Timeframe::Hour1);
                app.live.set_paused(true);
                for _ in 0..3 {
                    frame(&mut app, vec![]).drop_without_applying_deltas();
                }
                let output = frame(&mut app, vec![]);
                assert_eq!(fills_visible(&output, &app.chart, last), [false, false]);
                let first = text_at(&output, "1");
                output.drop_without_applying_deltas();
                click(&mut app, first);
                assert_eq!(app.state.symbol, report.symbol);
                assert_eq!(app.chart.timeframe(), Some(report.timeframe));
                assert_eq!(
                    app.timeframe_toolbar.state.session_type,
                    super::super::SessionType::Full24x7
                );
                assert_eq!(app.strategy_trade, Some(0));
                assert!(
                    app.chart
                        .bars(SeriesId::MAIN)
                        .iter()
                        .any(|bar| bar.time == report.trades[0].entry_time)
                );
                for _ in 0..3 {
                    frame(&mut app, vec![]).drop_without_applying_deltas();
                }
                let range = app.chart.visible_time_range().unwrap();
                assert!(
                    range.from <= report.trades[0].entry_time
                        && range.to >= report.trades[0].entry_time
                );
                let output = frame(&mut app, vec![]);
                assert!(
                    output
                        .shapes
                        .iter()
                        .any(|shape| matches!(&shape.shape, Shape::Rect(r)
                    if r.fill == theme.ui.accent.linear_multiply(0.12)
                        && r.rect.height() == app.chart.get_chart_mapping().rect.height()))
                );
                let export = text_at(&output, "Export trades CSV");
                output.drop_without_applying_deltas();
                click(&mut app, export);
                assert_eq!(
                    std::fs::read_to_string(std::env::temp_dir().join("strategy-trades.csv"))
                        .unwrap(),
                    report.trades_csv()
                );
                app.execute_search_command(&ctx, "backtest");
                assert!(app.strategy_trade.is_none());
                let replacement = app.strategy_report.report().unwrap().clone();
                let last_trade = replacement.trades.last().unwrap();
                app.execute_search_command(&ctx, "samples");
                app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
                app.chart.fit_content();
                app.extra_charts[0].chart.fit_content();
                for _ in 0..3 {
                    frame(&mut app, vec![]).drop_without_applying_deltas();
                }
                let output = frame(&mut app, vec![]);
                assert_eq!(
                    std::iter::once(&app.chart)
                        .chain(app.extra_charts.iter().map(|extra| &extra.chart))
                        .filter(|chart| fills_visible(&output, chart, last_trade) == [true, true])
                        .count(),
                    2,
                    "new chart slots retain report fills"
                );
                assert!(live_marker(&output), "live analytics marker remains");
                let clear = text_at(&output, "Clear report");
                output.drop_without_applying_deltas();
                click(&mut app, clear);
                assert!(app.strategy_report.report().is_none());
                let output = frame(&mut app, vec![]);
                assert_eq!(
                    fills_visible(&output, &app.chart, last_trade),
                    [false, false]
                );
                assert!(
                    !output
                        .shapes
                        .iter()
                        .any(|s| matches!(&s.shape, Shape::Text(t)
                    if matches!(t.galley.text(), "Clear report" | "Export trades CSV")))
                );
                assert!(live_marker(&output), "live analytics marker remains");
                output.drop_without_applying_deltas();
                std::fs::remove_file(std::env::temp_dir().join("strategy-trades.csv")).unwrap();
            }
        }
    }
}
