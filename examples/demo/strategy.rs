//! A host-run sample backtest and the report's navigation, signals and CSV delivery.

use super::{Bar, DemoApp, LiveFeed, SeriesId, SessionType, Timeframe, panel_frame};
use egui_charts::model::{
    Direction, OrderSide, Signal, SignalId, SignalShape, StrategyExit, StrategyReport,
    StrategyTrade,
};
use egui_charts::services::{Events, SignalEvent, Signals};
use egui_charts::ui::strategy_report::StrategyReportAction;
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
    pub(super) fn run_sample_backtest(&mut self) {
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
        self.strategy_report.set_report(Some(report), source);
        self.strategy_session = self.timeframe_toolbar.state.session_type;
        self.strategy_trade = None;
        self.refresh_strategy_signals();
    }

    pub(super) fn show_strategy_report(&mut self, ui: &mut egui::Ui, bg: egui::Color32) {
        if self.strategy_report.report().is_none() {
            return;
        }
        let mut clear = false;
        let action = egui::Panel::bottom("strategy_report")
            .resizable(true)
            .default_size(330.0)
            .size_range(100.0..=(ui.available_height() * 0.7).max(100.0))
            .frame(panel_frame(bg))
            .show(ui, |ui| {
                clear = ui.button("Clear report").clicked();
                self.strategy_report.show(ui)
            })
            .inner;
        if clear {
            self.strategy_report.set_report(None, &[]);
            self.strategy_trade = None;
            self.refresh_strategy_signals();
        } else {
            self.handle_strategy_action(action);
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
                if self.timeframe_toolbar.state.session_type != SessionType::Full24x7
                    && !self
                        .chart
                        .bars(SeriesId::MAIN)
                        .iter()
                        .any(|bar| bar.time == trade.entry_time)
                {
                    self.timeframe_toolbar.set_session(SessionType::Full24x7);
                    self.regenerate_bars();
                }
                self.chart.scroll_to_time(trade.entry_time);
                self.strategy_trade = Some(trade.trade_index);
                self.refresh_strategy_signals();
            }
            StrategyReportAction::ExportCsv(csv) => self.save_text("strategy-trades.csv", &csv),
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
            let live = self.api_samples.then(|| live.clone());
            let source: Option<Arc<dyn Signals>> = match self.strategy_report.report() {
                Some(report) => Some(Arc::new(ReportSignals {
                    report: report.clone(),
                    live,
                })),
                None => live.map(|live| live as Arc<dyn Signals>),
            };
            chart.set_signal_source(source);
        }
    }
}

struct ReportSignals {
    report: StrategyReport,
    live: Option<Arc<LiveFeed>>,
}

impl Signals for ReportSignals {
    fn subscribe(&self, symbol: &str, timeframe: Timeframe, events: Events<SignalEvent>) {
        if let Some(live) = &self.live {
            live.subscribe(symbol, timeframe, events.clone());
        }
        if self.report.symbol != symbol || self.report.timeframe != timeframe {
            return;
        }
        for (index, trade) in self.report.trades.iter().enumerate() {
            for (fill, time, price, direction) in [
                ("Entry", trade.entry_time, trade.entry_price, Direction::Up),
                (
                    "Exit",
                    trade.exit.as_ref().expect("sample round trip").time,
                    trade.exit.as_ref().expect("sample round trip").price,
                    Direction::Down,
                ),
            ] {
                events.push(SignalEvent::Upsert(Signal {
                    id: SignalId(format!("sample-backtest-{index}-{fill}")),
                    shape: SignalShape::Marker {
                        time,
                        price: Some(price),
                        direction,
                    },
                    category: "Sample backtest".into(),
                    label: Some(format!("{fill} {}", index + 1)),
                    tooltip: Some(format!(
                        "{} · {fill} {} @ {price}",
                        self.report.name,
                        index + 1
                    )),
                }));
            }
        }
    }

    fn clicked(&self, id: &SignalId) {
        if let Some(live) = &self.live {
            live.clicked(id);
        }
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
    fn report_host_selects_navigates_exports_replaces_and_clears() {
        for theme in [super::super::Theme::light(), super::super::Theme::dark()] {
            for width in [640.0, 1440.0] {
                let ctx = egui::Context::default();
                let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
                app.theme = theme.clone();
                egui_charts::theme::apply_to_egui(&ctx, &theme);
                app.live.set_paused(true);
                app.chart.set_feed_paused(true);
                let bars = app.chart.bars(SeriesId::MAIN).to_vec();
                let frame = |app: &mut DemoApp, events| {
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
                text_at(&output, "Entry 3");
                text_at(&output, "Exit 3");
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
                assert!(
                    !output.shapes.iter().any(
                        |s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == "Entry 1")
                    )
                );
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
                app.execute_search_command(&ctx, "samples");
                app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
                app.chart.fit_content();
                app.extra_charts[0].chart.fit_content();
                for _ in 0..3 {
                    frame(&mut app, vec![]).drop_without_applying_deltas();
                }
                let output = frame(&mut app, vec![]);
                assert_eq!(
                    output
                        .shapes
                        .iter()
                        .filter(|s| {
                            matches!(&s.shape, Shape::Text(t) if t.galley.text() == "Exit 3")
                        })
                        .count(),
                    2,
                    "new chart slots retain report fills"
                );
                text_at(&output, "Host tick");
                let clear = text_at(&output, "Clear report");
                output.drop_without_applying_deltas();
                click(&mut app, clear);
                assert!(app.strategy_report.report().is_none());
                let output = frame(&mut app, vec![]);
                assert!(!output.shapes.iter().any(|s| matches!(&s.shape, Shape::Text(t)
                    if matches!(t.galley.text(), "Entry 1" | "Exit 1" | "Clear report" | "Export trades CSV"))));
                text_at(&output, "Host tick");
                output.drop_without_applying_deltas();
                std::fs::remove_file(std::env::temp_dir().join("strategy-trades.csv")).unwrap();
            }
        }
    }
}
