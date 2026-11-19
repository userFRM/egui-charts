//! A replaceable strategy report, independent of chart navigation and host storage.

pub(crate) mod inputs;
#[cfg(feature = "strategies")]
mod optimisation;
mod plot;
#[cfg(feature = "strategies")]
mod properties;
#[cfg(feature = "strategies")]
mod tester;
#[cfg(feature = "strategies")]
pub use tester::{StrategyHistoryRequest, StrategyTester, StrategyTesterAction};
#[cfg(test)]
mod tests;

use crate::localization as l10n;
use crate::model::{
    Bar, OrderSide, StrategyComparison, StrategyPerformance, StrategyReport, SummaryUnit,
    Timeframe, run_figures, trade_figures,
};
use crate::theme::ThemeExt;
use crate::ui_kit::{
    EmptyState,
    buttons::Button,
    tab_bar::{TabBar, TabLabel},
};
use chrono::{DateTime, Utc};
use egui::{RichText, Ui};
use egui_extras::{Column, TableBuilder};

/// A trade to reveal on the chart. The host switches symbol and interval, scrolls to the entry and
/// highlights the span; an open trade has no exit boundary.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StrategyTradeSelection {
    /// Index into the report's original `trades`, independent of display sorting or invalid rows.
    pub trade_index: usize,
    /// Instrument of the report.
    pub symbol: String,
    /// Source interval of the report.
    pub timeframe: Timeframe,
    /// Start of the trade's span.
    pub entry_time: DateTime<Utc>,
    /// End of the trade's span, or `None` while open.
    pub exit_time: Option<DateTime<Utc>>,
}

/// An action for the host that mounts a [`StrategyReportPanel`].
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum StrategyReportAction {
    /// No host action this frame.
    #[default]
    None,
    /// Reveal this trade on the chart.
    SelectTrade(StrategyTradeSelection),
    /// Save these valid trades as CSV, written by [`StrategyReport::trades_csv_with_excursions`]
    /// with the excursions the list of trades shows.
    ExportCsv(String),
    /// Save the summary metrics and run properties shown, as [`StrategyReport::summary_csv`]
    /// writes them, under the suggested file name.
    ExportSummaryCsv {
        /// File name derived from the run's name and symbol, ending in `.csv`.
        filename: String,
        /// The CSV text.
        content: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Tab {
    #[default]
    Summary,
    Trades,
    Properties,
}

impl TabLabel for Tab {
    fn tab_label(&self) -> &str {
        match self {
            Self::Summary => "Summary",
            Self::Trades => "List of trades",
            Self::Properties => "Properties",
        }
    }
}

#[derive(Debug)]
struct ReportData {
    report: StrategyReport,
    performance: StrategyPerformance,
    comparison: StrategyComparison,
    equity: Vec<(DateTime<Utc>, f64)>,
    drawdown: Vec<(DateTime<Utc>, f64)>,
    run_up: Vec<(DateTime<Utc>, f64)>,
    trades: Vec<usize>,
}

/// Standalone summary, equity, drawdown and run-up charts, trade list and run properties.
///
/// Call [`Self::set_report`] once when the host supplies or replaces a run. Metrics and display
/// ordering are cached there. Mount [`Self::show`] in the host's panel below the chart; without a
/// report it draws no controls and takes no space. Give each instance its own egui id scope.
#[derive(Debug, Default)]
pub struct StrategyReportPanel {
    data: Option<ReportData>,
    tab: Tab,
    selected: Option<usize>,
    compare: bool,
    percent: bool,
}

impl StrategyReportPanel {
    /// Replace or clear the report in one call, resetting its tab and selection. `bars` must be the
    /// complete source bars for this run's symbol and interval; pass an empty slice if unavailable.
    pub fn set_report(&mut self, report: Option<StrategyReport>, bars: &[Bar]) {
        self.tab = Tab::Summary;
        self.selected = None;
        self.compare = false;
        self.percent = false;
        self.data = report.map(|report| {
            let performance = report.performance();
            let comparison = report.comparison(bars);
            let equity = report.equity_curve();
            let excursions = report
                .excursion_curve(true)
                .or_else(|| report.excursion_curve(false))
                .unwrap_or_default();
            let drawdown = excursions
                .iter()
                .map(|&(t, x)| (t, 0.0 - x.drawdown))
                .filter(|(_, d)| d.is_finite())
                .collect();
            let run_up = excursions
                .iter()
                .map(|&(t, x)| (t, x.run_up))
                .filter(|(_, r)| r.is_finite())
                .collect();
            let mut trades: Vec<_> = report
                .trades
                .iter()
                .enumerate()
                .filter(|(_, t)| t.is_valid())
                .map(|(i, _)| i)
                .collect();
            trades.sort_by_key(|&i| report.trades[i].entry_time);
            ReportData {
                report,
                performance,
                comparison,
                equity,
                drawdown,
                run_up,
                trades,
            }
        });
    }

    /// The currently supplied report, or `None` when cleared.
    pub fn report(&self) -> Option<&StrategyReport> {
        self.data.as_ref().map(|d| &d.report)
    }

    /// Draw the report and return this frame's host intent. No report draws nothing.
    /// A report with [`StrategyReport::unavailable`] shows that reason instead of its summary and exports.
    pub fn show(&mut self, ui: &mut Ui) -> StrategyReportAction {
        let Some(data) = &self.data else {
            return StrategyReportAction::None;
        };
        let mut action = StrategyReportAction::None;
        let theme = ui.active_theme();
        let unavailable = data.report.unavailable.as_deref();
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(l10n::text(ui.ctx(), "Strategy report"))
                    .strong()
                    .size(theme.text.heading),
            );
            ui.label(RichText::new(&data.report.name).strong());
            ui.label(format!(
                "{} · {}",
                data.report.symbol, data.report.timeframe
            ));
            if unavailable.is_none() {
                if Button::new("Export trades CSV").show(ui).clicked() {
                    action = StrategyReportAction::ExportCsv(
                        data.report.trades_csv_with_excursions(&data.comparison),
                    );
                }
                if Button::new("Export summary CSV").show(ui).clicked() {
                    action = StrategyReportAction::ExportSummaryCsv {
                        filename: summary_filename(&data.report),
                        content: data.report.summary_csv(&data.performance, &data.comparison),
                    };
                }
            }
        });
        TabBar::new(&[Tab::Summary, Tab::Trades, Tab::Properties], &mut self.tab).show(ui);
        if let Some(reason) = unavailable {
            ui.colored_label(theme.ui.warning, l10n::text(ui.ctx(), reason));
        } else if data.performance.skipped_trades
            + data.performance.skipped_equity_points
            + data.comparison.skipped_bars
            > 0
        {
            ui.colored_label(
                theme.ui.warning,
                l10n::format_text(
                    ui.ctx(),
                    "Skipped input: {trades} trades, {equity} equity points, {bars} source bars",
                    &[
                        ("trades", data.performance.skipped_trades.to_string()),
                        ("equity", data.performance.skipped_equity_points.to_string()),
                        ("bars", data.comparison.skipped_bars.to_string()),
                    ],
                ),
            );
        }
        match self.tab {
            Tab::Summary => {
                if unavailable.is_none() {
                    egui::ScrollArea::vertical()
                        .id_salt("summary")
                        .show(ui, |ui| {
                            summary(ui, data, &mut self.compare, &mut self.percent);
                        });
                }
            }
            Tab::Trades => {
                if let Some(index) = trades(ui, data, &mut self.selected) {
                    let trade = &data.report.trades[index];
                    action = StrategyReportAction::SelectTrade(StrategyTradeSelection {
                        trade_index: index,
                        symbol: data.report.symbol.clone(),
                        timeframe: data.report.timeframe,
                        entry_time: trade.entry_time,
                        exit_time: trade.exit.as_ref().map(|e| e.time),
                    });
                }
            }
            Tab::Properties => {
                egui::ScrollArea::both()
                    .id_salt("properties")
                    .show(ui, |ui| {
                        egui::Grid::new("run_properties")
                            .spacing([theme.spacing.xxl, theme.spacing.sm])
                            .striped(true)
                            .show(ui, |ui| {
                                property(ui, "Name", &data.report.name);
                                property(ui, "Symbol", &data.report.symbol);
                                property(ui, "Interval", &data.report.timeframe.to_string());
                                property(
                                    ui,
                                    "Initial capital",
                                    &money(Some(data.report.initial_capital), &data.report),
                                );
                                property(
                                    ui,
                                    "Currency",
                                    data.report.currency.as_deref().unwrap_or("N/A"),
                                );
                                property(
                                    ui,
                                    "Risk-free rate",
                                    &percent(Some(data.report.risk_free_rate)),
                                );
                                for (name, value) in &data.report.properties {
                                    property(ui, name, value);
                                }
                            });
                    });
            }
        }
        action
    }
}

/// `name-symbol-summary.csv`, with runs of characters other than letters and digits as one `-`.
fn summary_filename(report: &StrategyReport) -> String {
    let name = format!("{} {}", report.name, report.symbol);
    let mut words: Vec<_> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() {
        words.push("strategy");
    }
    format!("{}-summary.csv", words.join("-"))
}

fn number(value: Option<f64>) -> String {
    value
        .filter(|v| v.is_finite())
        .map_or_else(|| "N/A".into(), |v| format!("{v:.2}"))
}

fn percent(value: Option<f64>) -> String {
    value
        .map(|v| v * 100.0)
        .filter(|v| v.is_finite())
        .map_or_else(|| "N/A".into(), |v| format!("{v:.2}%"))
}

fn money(value: Option<f64>, report: &StrategyReport) -> String {
    let text = number(value);
    match &report.currency {
        Some(currency) if text != "N/A" => format!("{text} {currency}"),
        _ => text,
    }
}

fn property(ui: &mut Ui, label: &str, value: &str) {
    ui.label(l10n::text(ui.ctx(), label));
    ui.label(value);
    ui.end_row();
}

fn summary(ui: &mut Ui, data: &ReportData, compare: &mut bool, as_percent: &mut bool) {
    let report = &data.report;
    let theme = ui.active_theme();
    let cell = |value: Option<f64>, unit: SummaryUnit| match unit {
        SummaryUnit::Money => money(value, report),
        SummaryUnit::Percent => percent(value),
        SummaryUnit::Ratio | SummaryUnit::Bars => number(value),
        SummaryUnit::Count | SummaryUnit::Quantity => value
            .filter(|v| v.is_finite())
            .map_or_else(|| "N/A".into(), |v| v.to_string()),
    };
    let pairs = if ui.available_width() >= 760.0 { 2 } else { 1 };
    egui::ScrollArea::horizontal()
        .id_salt("summary_sides")
        .show(ui, |ui| {
            egui::Grid::new("performance_by_side")
                .striped(true)
                .spacing([theme.spacing.xxl, theme.spacing.sm])
                .show(ui, |ui| {
                    ui.label("");
                    for side in ["All", "Long", "Short"] {
                        ui.label(RichText::new(l10n::text(ui.ctx(), side)).strong());
                    }
                    ui.end_row();
                    for (label, unit, values) in trade_figures(&data.performance, &data.comparison)
                    {
                        ui.label(
                            RichText::new(l10n::text(ui.ctx(), label))
                                .color(theme.ui.text_secondary),
                        );
                        for value in values {
                            ui.label(RichText::new(cell(value, unit)).strong());
                        }
                        ui.end_row();
                    }
                });
            ui.add_space(theme.spacing.md);
            let metrics = run_figures(&data.performance, &data.comparison);
            egui::Grid::new("performance")
                .striped(true)
                .spacing([theme.spacing.xxl, theme.spacing.sm])
                .show(ui, |ui| {
                    for (i, &(label, unit, value)) in metrics.iter().enumerate() {
                        ui.label(
                            RichText::new(l10n::text(ui.ctx(), label))
                                .color(theme.ui.text_secondary),
                        );
                        ui.label(RichText::new(cell(value, unit)).strong());
                        if (i + 1) % pairs == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
    ui.label(
        RichText::new(l10n::text(
            ui.ctx(),
            "N/A means unavailable. Ratios and compound growth require at least 13 weeks of equity; holding times and buy-and-hold figures require the source bars.",
        ))
        .color(theme.ui.text_muted)
        .size(theme.text.small),
    );
    ui.add_space(theme.spacing.md);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(l10n::text(ui.ctx(), "Equity")).strong());
        if !data.comparison.buy_and_hold_equity.is_empty() {
            ui.checkbox(compare, l10n::text(ui.ctx(), "Compare buy-and-hold"));
            if *compare {
                ui.colored_label(theme.ui.warning, l10n::text(ui.ctx(), "Buy-and-hold"));
            }
        }
        if report.initial_capital.is_finite() && report.initial_capital > 0.0 {
            ui.checkbox(
                as_percent,
                l10n::text(ui.ctx(), "Percent of initial capital"),
            );
        }
        if *as_percent {
            ui.label("%");
        } else if let Some(currency) = &report.currency {
            ui.label(currency);
        }
    });
    let scale = if *as_percent {
        100.0 / report.initial_capital
    } else {
        1.0
    };
    plot::show(
        ui,
        &data.equity,
        if *compare {
            &data.comparison.buy_and_hold_equity
        } else {
            &[]
        },
        theme.ui.accent,
        175.0,
        scale,
    );
    ui.label(RichText::new(l10n::text(ui.ctx(), "Drawdown from peak")).strong());
    plot::show(ui, &data.drawdown, &[], theme.ui.error, 125.0, scale);
    ui.label(RichText::new(l10n::text(ui.ctx(), "Run-up from trough")).strong());
    plot::show(ui, &data.run_up, &[], theme.ui.success, 125.0, scale);
}

fn trades(ui: &mut Ui, data: &ReportData, selected: &mut Option<usize>) -> Option<usize> {
    if data.trades.is_empty() {
        EmptyState::new("No valid trades").show(ui);
        return None;
    }
    ui.label(l10n::text(ui.ctx(), "Select a trade to reveal it on the chart. Times are UTC; MAE/MFE are before fees in price × quantity units."));
    let mut clicked = None;
    let ctx = ui.ctx().clone();
    egui::ScrollArea::horizontal()
        .id_salt("trade_columns")
        .show(ui, |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .sense(egui::Sense::click())
                .column(Column::initial(65.0))
                .column(Column::initial(65.0))
                .column(Column::initial(80.0))
                .column(Column::initial(150.0))
                .column(Column::initial(80.0))
                .column(Column::initial(150.0))
                .columns(Column::initial(85.0), 5)
                .column(Column::remainder().at_least(100.0))
                .header(28.0, |mut header| {
                    for label in [
                        "Trade",
                        "Side",
                        "Quantity",
                        "Entry (UTC)",
                        "Entry price",
                        "Exit (UTC)",
                        "Exit price",
                        "Net profit",
                        "Fees",
                        "MAE",
                        "MFE",
                        "Label",
                    ] {
                        header.col(|ui| {
                            ui.strong(l10n::text(ui.ctx(), label));
                        });
                    }
                })
                .body(|body| {
                    body.rows(28.0, data.trades.len(), |mut row| {
                        let ordinal = row.index();
                        let index = data.trades[ordinal];
                        let t = &data.report.trades[index];
                        let excursion = data.comparison.excursions[index].as_ref();
                        row.set_selected(*selected == Some(index));
                        row.col(|ui| {
                            if ui
                                .selectable_label(
                                    *selected == Some(index),
                                    (ordinal + 1).to_string(),
                                )
                                .clicked()
                            {
                                clicked = Some(index);
                            }
                        });
                        let time = |t: DateTime<Utc>| t.format("%Y-%m-%d %H:%M:%S%.f").to_string();
                        for value in [
                            l10n::text(
                                &ctx,
                                match t.side {
                                    OrderSide::Buy => "Long",
                                    OrderSide::Sell => "Short",
                                },
                            )
                            .to_string(),
                            t.quantity.to_string(),
                            time(t.entry_time),
                            t.entry_price.to_string(),
                            t.exit.as_ref().map_or_else(
                                || l10n::text(&ctx, "Open").to_string(),
                                |e| time(e.time),
                            ),
                            t.exit
                                .as_ref()
                                .map_or_else(|| "N/A".into(), |e| e.price.to_string()),
                            money(t.exit.as_ref().map(|e| e.profit), &data.report),
                            money(Some(t.fees), &data.report),
                            number(excursion.map(|e| e.adverse)),
                            number(excursion.map(|e| e.favourable)),
                            t.label.clone().unwrap_or_default(),
                        ] {
                            row.col(|ui| {
                                ui.label(value);
                            });
                        }
                        if row.response().clicked() {
                            clicked = Some(index);
                        }
                    });
                });
        });
    if let Some(index) = clicked {
        *selected = Some(index);
    }
    clicked
}
