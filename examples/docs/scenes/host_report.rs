use chrono::{DateTime, TimeDelta};
use eframe::egui;
use egui_charts::model::{OrderSide, StrategyExit, StrategyReport, StrategyTrade, Timeframe};

/// The host supplies fills, fees and marked equity independently of rendering.
pub fn host_result(fees: f64) -> StrategyReport {
    let start = DateTime::UNIX_EPOCH + TimeDelta::days(20_000);
    let mut report = StrategyReport::new("Host sample", "SAMPLE", Timeframe::Hour1, 10_000.0);
    report.currency = Some("USD".into());
    for (i, exit_price) in [105.0, 98.0].into_iter().enumerate() {
        let entry = start + TimeDelta::hours(i as i64 * 2);
        let mut trade = StrategyTrade::new(OrderSide::Buy, 10.0, entry, 100.0);
        trade.fees = fees;
        trade.exit = Some(StrategyExit::new(
            entry + TimeDelta::hours(1),
            exit_price,
            (exit_price - 100.0) * 10.0 - fees,
        ));
        report.trades.push(trade);
    }
    report.equity = vec![
        (start, 10_000.0),
        (start + TimeDelta::hours(1), 10_050.0 - fees),
        (start + TimeDelta::hours(3), 10_030.0 - 2.0 * fees),
    ];
    report
}

/// Retain a delivered report; repainting does not execute a strategy.
pub struct HostReport {
    report: StrategyReport,
    fees: f64,
    deliveries: usize,
}

impl Default for HostReport {
    fn default() -> Self {
        Self {
            report: host_result(2.0),
            fees: 2.0,
            deliveries: 1,
        }
    }
}

impl HostReport {
    /// Replace the report only on host delivery, then render the library's metrics.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::Slider::new(&mut self.fees, 0.0..=5.0).text("Fee per round trip"));
            if ui.button("Deliver host report").clicked() {
                self.report = host_result(self.fees);
                self.deliveries += 1;
            }
        });
        ui.label(format!(
            "Host deliveries: {}. Changing the fee takes effect on delivery.",
            self.deliveries
        ));
        ui.heading(&self.report.name);
        let p = self.report.performance();
        egui::Grid::new("host-report-metrics").show(ui, |ui| {
            for (label, value) in [
                ("Net profit (USD)", format!("{:.2}", p.net_profit)),
                (
                    "Gross profit / loss (USD)",
                    format!("{:.2} / {:.2}", p.gross_profit, p.gross_loss),
                ),
                ("Closed trades", p.closed_trades.to_string()),
                (
                    "Win rate",
                    p.win_rate
                        .map_or_else(|| "N/A".into(), |v| format!("{:.0}%", v * 100.0)),
                ),
                (
                    "Profit factor",
                    p.profit_factor
                        .map_or_else(|| "N/A".into(), |v| format!("{v:.2}")),
                ),
                (
                    "Maximum drawdown (USD)",
                    p.max_drawdown
                        .map_or_else(|| "N/A".into(), |v| format!("{v:.2}")),
                ),
                (
                    "Sharpe ratio",
                    p.sharpe_ratio
                        .map_or_else(|| "N/A".into(), |v| format!("{v:.2}")),
                ),
            ] {
                ui.label(label);
                ui.label(value);
                ui.end_row();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_delivery_matches_the_hand_calculated_fills_and_curve() {
        let report = host_result(2.0);
        let p = report.performance();
        assert_eq!(p.net_profit, 26.0);
        assert_eq!((p.gross_profit, p.gross_loss), (48.0, 22.0));
        assert_eq!(p.win_rate, Some(0.5));
        assert_eq!(p.max_drawdown, Some(22.0));
        assert_eq!(p.sharpe_ratio, None);
    }
}
