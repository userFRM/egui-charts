//! A replaceable strategy report, independent of chart navigation and host storage.

mod analysis;
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
    Bar, OrderSide, StrategyAnalysis, StrategyComparison, StrategyPerformance, StrategyReport,
    SummaryUnit, Timeframe, run_figures, trade_figures,
};
use crate::theme::ThemeExt;
use crate::ui_kit::{
    EmptyState,
    buttons::Button,
    tab_bar::{TabBar, TabLabel},
};
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeDelta, Utc};
use egui::{RichText, Ui};
use egui_extras::{Column, TableBuilder};
use std::ops::Range;

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
    /// with the excursions the list of trades shows, under the suggested file name.
    ExportCsv {
        /// File name from the run's name, symbol and interval, `trades` and the local time of
        /// the export to the second, ending in `.csv`.
        filename: String,
        /// The CSV text.
        content: String,
    },
    /// Save the whole-run summary metrics and run properties, as [`StrategyReport::summary_csv`]
    /// writes them, under the suggested file name.
    ExportSummaryCsv {
        /// File name from the run's name, symbol and interval, `summary` and the local time of
        /// the export to the second, ending in `.csv`.
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
    Periods,
    Distribution,
    TradeTimes,
    Properties,
}

impl TabLabel for Tab {
    fn tab_label(&self) -> &str {
        match self {
            Self::Summary => "Summary",
            Self::Trades => "List of trades",
            Self::Periods => "Periods",
            Self::Distribution => "Distribution",
            Self::TradeTimes => "Trade times",
            Self::Properties => "Properties",
        }
    }
}

#[derive(Debug)]
struct ReportData {
    report: StrategyReport,
    bars: Vec<Bar>,
    /// Whole-run figures for exports, skipped-input warnings and trade excursions.
    performance: StrategyPerformance,
    comparison: StrategyComparison,
    range: Option<Range<DateTime<Utc>>>,
    /// Figures of the range, or of the whole run without one.
    shown: StrategyPerformance,
    shown_comparison: StrategyComparison,
    analysis: StrategyAnalysis,
    initial_capital: f64,
    equity: Vec<(DateTime<Utc>, f64)>,
    drawdown: Vec<(DateTime<Utc>, f64)>,
    margin: Vec<(DateTime<Utc>, f64)>,
    run_up: Vec<(DateTime<Utc>, f64)>,
    /// Original indices of valid trades in the range, sorted by entry.
    trades: Vec<usize>,
    /// Net profit of the trades closed in the range up to each row of `trades`; `None` on a
    /// trade that is open in the range.
    cumulative: Vec<Option<f64>>,
    /// Listed trades closed in the range, in exit order with equal exits in list order: exit
    /// time, number in the list, side, and net profit of the listed trades closed up to and
    /// including it.
    closes: Vec<(DateTime<Utc>, usize, OrderSide, f64)>,
    /// Run-up and drawdown of each listed trade with an excursion, for the equity chart.
    marks: Vec<plot::TradeMark>,
}

impl ReportData {
    fn new(report: StrategyReport, bars: &[Bar]) -> Self {
        let performance = report.performance();
        let comparison = report.comparison(bars);
        let analysis = report.analysis(bars);
        let mut data = Self {
            initial_capital: report.initial_capital,
            report,
            bars: bars.to_vec(),
            shown: performance.clone(),
            performance,
            shown_comparison: StrategyComparison::default(),
            comparison,
            range: None,
            analysis,
            equity: Vec::new(),
            drawdown: Vec::new(),
            margin: Vec::new(),
            run_up: Vec::new(),
            trades: Vec::new(),
            cumulative: Vec::new(),
            closes: Vec::new(),
            marks: Vec::new(),
        };
        data.select(None);
        data
    }

    fn select(&mut self, range: Option<Range<DateTime<Utc>>>) {
        let part = range.clone().map(|r| self.report.within(r));
        let run = part.as_ref().unwrap_or(&self.report);
        let bars: Vec<_> = self
            .bars
            .iter()
            .filter(|b| range.as_ref().is_none_or(|r| r.contains(&b.time)))
            .cloned()
            .collect();
        self.initial_capital = run.initial_capital;
        self.shown = run.performance();
        self.shown_comparison = run.comparison(&bars);
        if range.is_some() {
            // A trade entered before the range holds bars from its entry, as in the whole run.
            let whole = run.comparison(&self.bars);
            self.shown_comparison.all = whole.all;
            self.shown_comparison.long = whole.long;
            self.shown_comparison.short = whole.short;
        }
        self.analysis = run.analysis(&bars);
        self.equity = run.equity_curve();
        self.margin = run.margin_curve();
        let excursions = run
            .excursion_curve(true)
            .or_else(|| run.excursion_curve(false))
            .unwrap_or_default();
        self.drawdown = excursions
            .iter()
            .map(|&(t, x)| (t, 0.0 - x.drawdown))
            .filter(|(_, d)| d.is_finite())
            .collect();
        self.run_up = excursions
            .iter()
            .map(|&(t, x)| (t, x.run_up))
            .filter(|(_, r)| r.is_finite())
            .collect();
        self.trades = self
            .report
            .trades
            .iter()
            .enumerate()
            .filter(|(_, t)| t.is_valid() && range.as_ref().is_none_or(|r| t.in_range(r)))
            .map(|(i, _)| i)
            .collect();
        self.trades
            .sort_by_key(|&i| self.report.trades[i].entry_time);
        // An exit at or after the range's end leaves the trade open in it, as `within` does.
        let closed = |i: usize| {
            self.report.trades[i]
                .exit
                .as_ref()
                .filter(|exit| range.as_ref().is_none_or(|r| exit.time < r.end))
        };
        let mut total = 0.0;
        self.cumulative = self
            .trades
            .iter()
            .map(|&i| {
                total += closed(i)?.profit;
                Some(total)
            })
            .collect();
        let mut closes: Vec<_> = self
            .trades
            .iter()
            .enumerate()
            .filter_map(|(ordinal, &i)| {
                let exit = closed(i)?;
                Some((
                    exit.time,
                    ordinal + 1,
                    self.report.trades[i].side,
                    exit.profit,
                ))
            })
            .collect();
        // Stable, so equal exits keep list order.
        closes.sort_by_key(|close| close.0);
        let mut total = 0.0;
        for close in &mut closes {
            total += close.3;
            close.3 = total;
        }
        self.closes = closes;
        self.marks = self
            .trades
            .iter()
            .filter_map(|&i| {
                let trade = &self.report.trades[i];
                let excursion = self.comparison.excursions[i].as_ref()?;
                let before = self.equity.partition_point(|p| p.0 < trade.entry_time);
                Some(plot::TradeMark {
                    entry: trade.entry_time,
                    exit: closed(i).map(|exit| exit.time),
                    base: self.equity.get(before.saturating_sub(1))?.1,
                    run_up: excursion.favourable,
                    drawdown: excursion.adverse,
                })
            })
            .collect();
        self.range = range;
    }

    fn dates(&self) -> [String; 2] {
        let times: Vec<_> = self
            .equity
            .iter()
            .map(|p| p.0)
            .chain(
                self.report
                    .trades
                    .iter()
                    .filter(|t| t.is_valid())
                    .flat_map(|t| {
                        std::iter::once(t.entry_time).chain(t.exit.as_ref().map(|e| e.time))
                    }),
            )
            .collect();
        [times.iter().min(), times.iter().max()]
            .map(|t| t.map_or_else(String::new, |t| t.format("%Y-%m-%d").to_string()))
    }
}

/// Standalone summary, equity, drawdown, run-up and margin charts, trade list, period,
/// distribution and trade time pages and run properties, for the whole run or a chosen date
/// range. Exports always cover the whole run.
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
    trade_marks: bool,
    range_text: [String; 2],
    range_error: bool,
}

impl StrategyReportPanel {
    /// Replace or clear the report in one call, resetting its tab and selection and the date
    /// range to the whole run. `bars` must be the complete source bars for this run's symbol and
    /// interval; pass an empty slice if unavailable.
    pub fn set_report(&mut self, report: Option<StrategyReport>, bars: &[Bar]) {
        self.tab = Tab::Summary;
        self.selected = None;
        self.compare = false;
        self.trade_marks = false;
        self.percent = false;
        self.range_error = false;
        self.data = report.map(|report| ReportData::new(report, bars));
        self.range_text = self
            .data
            .as_ref()
            .map_or_else(Default::default, ReportData::dates);
    }

    /// Keep the report controls and the selected entry as its trades grow or rebuild.
    #[cfg(feature = "strategies")]
    pub(super) fn continue_report(&mut self, report: StrategyReport, bars: &[Bar]) {
        self.selected = self.selected.and_then(|index| {
            let old = self.data.as_ref()?.report.trades.get(index)?;
            report.trades.iter().position(|trade| {
                trade.side == old.side
                    && trade.entry_time == old.entry_time
                    && (trade.label == old.label
                        // A closing exit appends " / {exit}" to the label of the open trade.
                        || (old.exit.is_none()
                            && trade
                                .label
                                .as_deref()
                                .zip(old.label.as_deref())
                                .and_then(|(label, entry)| label.strip_prefix(entry))
                                .is_some_and(|exit| exit.starts_with(" / "))))
            })
        });
        let range = self.data.as_ref().and_then(|data| data.range.clone());
        let mut data = ReportData::new(report, bars);
        if range.is_some() {
            data.select(range);
        } else if !self.range_error {
            self.range_text = data.dates();
        }
        self.data = Some(data);
    }

    /// The currently supplied report, or `None` when cleared.
    pub fn report(&self) -> Option<&StrategyReport> {
        self.data.as_ref().map(|d| &d.report)
    }

    /// Draw the report and return this frame's host intent. No report draws nothing.
    /// A report with [`StrategyReport::unavailable`] shows that reason instead of its summary and exports.
    pub fn show(&mut self, ui: &mut Ui) -> StrategyReportAction {
        let Some(data) = &mut self.data else {
            return StrategyReportAction::None;
        };
        let mut action = StrategyReportAction::None;
        let theme = ui.active_theme();
        let available = data.report.unavailable.is_none();
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
            if available {
                if Button::new("Export trades CSV").show(ui).clicked() {
                    action = StrategyReportAction::ExportCsv {
                        filename: export_filename(
                            &data.report,
                            "trades",
                            chrono::Local::now().naive_local(),
                        ),
                        content: data.report.trades_csv_with_excursions(&data.comparison),
                    };
                }
                if Button::new("Export summary CSV").show(ui).clicked() {
                    action = StrategyReportAction::ExportSummaryCsv {
                        filename: export_filename(
                            &data.report,
                            "summary",
                            chrono::Local::now().naive_local(),
                        ),
                        content: data.report.summary_csv(&data.performance, &data.comparison),
                    };
                }
            }
        });
        egui::ScrollArea::horizontal()
            .id_salt("report_tabs")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                TabBar::new(
                    &[
                        Tab::Summary,
                        Tab::Trades,
                        Tab::Periods,
                        Tab::Distribution,
                        Tab::TradeTimes,
                        Tab::Properties,
                    ],
                    &mut self.tab,
                )
                .show(ui);
            });
        if let Some(reason) = &data.report.unavailable {
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
        if self.tab != Tab::Properties && available {
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                for (label, value) in ["From (UTC)", "To (UTC)"]
                    .into_iter()
                    .zip(&mut self.range_text)
                {
                    let label = ui.label(l10n::text(ui.ctx(), label));
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(value)
                                .desired_width(90.0)
                                .hint_text("YYYY-MM-DD"),
                        )
                        .labelled_by(label.id)
                        .changed();
                }
                if Button::new("Whole run").show(ui).clicked() {
                    data.select(None);
                    self.range_text = data.dates();
                    self.range_error = false;
                    changed = false;
                }
            });
            if changed {
                let dates = self
                    .range_text
                    .each_ref()
                    .map(|text| NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d"));
                let range = match dates {
                    [Ok(from), Ok(to)] if from <= to => to.succ_opt().map(|end| {
                        from.and_hms_opt(0, 0, 0).unwrap().and_utc()
                            ..end.and_hms_opt(0, 0, 0).unwrap().and_utc()
                    }),
                    _ => None,
                };
                self.range_error = range.is_none();
                if let Some(range) = range {
                    data.select(Some(range));
                }
            }
            if self.range_error {
                ui.colored_label(
                    theme.ui.warning,
                    l10n::text(
                        ui.ctx(),
                        "Enter dates as YYYY-MM-DD, the start not after the end",
                    ),
                );
            }
        }
        match self.tab {
            Tab::Summary => {
                if available {
                    egui::ScrollArea::vertical()
                        .id_salt("summary")
                        .show(ui, |ui| {
                            summary(
                                ui,
                                data,
                                &mut self.compare,
                                &mut self.trade_marks,
                                &mut self.percent,
                            );
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
            Tab::Periods => {
                if available {
                    analysis::periods(ui, data);
                }
            }
            Tab::Distribution => {
                if available {
                    analysis::distribution(ui, data);
                }
            }
            Tab::TradeTimes => {
                if available {
                    analysis::times(ui, data);
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

/// Suggested export file name: the run's name, symbol and interval, `kind` and `time` to the
/// second, such as `First_run_2_EUR___USD_1min_trades_2026-11-21_14-03-07.csv`. Every character
/// other than an ASCII letter, a digit, `-` or `_` becomes `_`; the name and symbol keep their
/// first 64 characters, so the whole name stays far below file system limits, and an empty one
/// is left out.
fn export_filename(report: &StrategyReport, kind: &str, time: NaiveDateTime) -> String {
    let plain = |c: char| {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
            c
        } else {
            '_'
        }
    };
    let mut parts: Vec<String> = [report.name.as_str(), report.symbol.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .map(|part| part.chars().take(64).map(plain).collect())
        .collect();
    parts.push(report.timeframe.to_string().chars().map(plain).collect());
    parts.push(kind.to_owned());
    parts.push(time.format("%Y-%m-%d_%H-%M-%S").to_string());
    format!("{}.csv", parts.join("_"))
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

/// The title of a drawdown or run-up chart with the largest and the latest value it plots and
/// the summary's average phase, in the chart's units after `scale`.
fn chart_title(
    ui: &mut Ui,
    title: &str,
    values: impl Iterator<Item = f64> + Clone,
    average: Option<f64>,
    scale: f64,
) {
    let muted = ui.active_theme().ui.text_muted;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(l10n::text(ui.ctx(), title)).strong());
        let maximum = values.clone().fold(f64::NAN, f64::max);
        ui.label(
            RichText::new(l10n::format_text(
                ui.ctx(),
                "Maximum {maximum}, average {average}, current {current}",
                &[
                    ("maximum", number(Some(maximum * scale))),
                    ("average", number(average.map(|value| value * scale))),
                    ("current", number(values.last().map(|value| value * scale))),
                ],
            ))
            .color(muted),
        );
    });
}

fn summary(
    ui: &mut Ui,
    data: &ReportData,
    compare: &mut bool,
    trade_marks: &mut bool,
    as_percent: &mut bool,
) {
    let report = &data.report;
    if !data.initial_capital.is_finite() || data.initial_capital <= 0.0 {
        *as_percent = false;
    }
    let theme = ui.active_theme();
    let range_label = data.range.as_ref().map_or_else(
        || l10n::text(ui.ctx(), "Range: whole run").to_string(),
        |r| {
            l10n::format_text(
                ui.ctx(),
                "Range (UTC): {from} to {to}",
                &[
                    ("from", r.start.format("%Y-%m-%d").to_string()),
                    (
                        "to",
                        (r.end - TimeDelta::nanoseconds(1))
                            .format("%Y-%m-%d")
                            .to_string(),
                    ),
                ],
            )
        },
    );
    ui.label(
        RichText::new(range_label)
            .small()
            .color(theme.ui.text_muted),
    );
    let cell = |value: Option<f64>, unit: SummaryUnit| match unit {
        SummaryUnit::Money => money(value, report),
        SummaryUnit::Percent => percent(value),
        SummaryUnit::Ratio | SummaryUnit::Bars | SummaryUnit::Days => number(value),
        SummaryUnit::Count | SummaryUnit::Quantity => value
            .filter(|v| v.is_finite())
            .map_or_else(|| "N/A".into(), StrategyReport::quantity_text),
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
                    for (label, unit, values) in trade_figures(&data.shown, &data.shown_comparison)
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
            let metrics = run_figures(&data.shown, &data.shown_comparison);
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
            "N/A means unavailable. Ratios require two whole calendar months of equity and compound growth at least 13 weeks; holding times and buy-and-hold figures require the source bars.",
        ))
        .color(theme.ui.text_muted)
        .size(theme.text.small),
    );
    ui.add_space(theme.spacing.md);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(l10n::text(ui.ctx(), "Equity")).strong());
        if !data.shown_comparison.buy_and_hold_equity.is_empty() {
            ui.checkbox(compare, l10n::text(ui.ctx(), "Compare buy-and-hold"));
            if *compare {
                ui.colored_label(theme.ui.warning, l10n::text(ui.ctx(), "Buy-and-hold"));
            }
        }
        if !data.marks.is_empty() {
            ui.checkbox(
                trade_marks,
                l10n::text(ui.ctx(), "Trade run-up and drawdown"),
            );
        }
        if data.initial_capital.is_finite() && data.initial_capital > 0.0 {
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
        100.0 / data.initial_capital
    } else {
        1.0
    };
    plot::show(
        ui,
        &data.equity,
        if *compare {
            &data.shown_comparison.buy_and_hold_equity
        } else {
            &[]
        },
        theme.ui.accent,
        175.0,
        scale,
        if *trade_marks { &data.marks } else { &[] },
        |ui, time| {
            // The last listed trade closed at or before the point; ties name the last in the list.
            let i = data.closes.partition_point(|close| close.0 <= time);
            if let Some(&(_, number, side, profit)) = i.checked_sub(1).map(|i| &data.closes[i]) {
                ui.label(l10n::format_text(
                    ui.ctx(),
                    "Trade {number}: {side}",
                    &[
                        ("number", number.to_string()),
                        (
                            "side",
                            l10n::text(
                                ui.ctx(),
                                match side {
                                    OrderSide::Buy => "Long",
                                    OrderSide::Sell => "Short",
                                },
                            ),
                        ),
                    ],
                ));
                ui.label(l10n::format_text(
                    ui.ctx(),
                    "Cumulative profit: {profit}",
                    &[("profit", money(Some(profit), report))],
                ));
            }
        },
    );
    chart_title(
        ui,
        "Drawdown from peak",
        // Zero minus the plotted value: a zero drawdown stays a positive zero.
        data.drawdown.iter().map(|point| 0.0 - point.1),
        data.shown.average_drawdown,
        scale,
    );
    plot::show(
        ui,
        &data.drawdown,
        &[],
        theme.ui.error,
        125.0,
        scale,
        &[],
        |_, _| {},
    );
    chart_title(
        ui,
        "Run-up from trough",
        data.run_up.iter().map(|point| point.1),
        data.shown.average_run_up,
        scale,
    );
    plot::show(
        ui,
        &data.run_up,
        &[],
        theme.ui.success,
        125.0,
        scale,
        &[],
        |_, _| {},
    );
    if !data.margin.is_empty() {
        ui.label(RichText::new(l10n::text(ui.ctx(), "Margin used")).strong());
        plot::show(
            ui,
            &data.margin,
            &[],
            theme.ui.info,
            125.0,
            scale,
            &[],
            |_, _| {},
        );
    }
}

fn trades(ui: &mut Ui, data: &ReportData, selected: &mut Option<usize>) -> Option<usize> {
    if data.trades.is_empty() {
        EmptyState::new("No valid trades").show(ui);
        return None;
    }
    ui.label(l10n::text(ui.ctx(), "Select a trade to reveal it on the chart. Times are UTC; MAE/MFE are before fees in the report's currency."));
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
                .columns(Column::initial(85.0), 2)
                .column(Column::initial(120.0))
                .columns(Column::initial(85.0), 3)
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
                        "Cumulative profit",
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
                            StrategyReport::quantity_text(t.quantity),
                            time(t.entry_time),
                            data.report.price_text(t.entry_price),
                            t.exit.as_ref().map_or_else(
                                || l10n::text(&ctx, "Open").to_string(),
                                |e| time(e.time),
                            ),
                            t.exit
                                .as_ref()
                                .map_or_else(|| "N/A".into(), |e| data.report.price_text(e.price)),
                            money(t.exit.as_ref().map(|e| e.profit), &data.report),
                            data.cumulative[ordinal]
                                .map_or_else(String::new, |total| money(Some(total), &data.report)),
                            money(Some(t.fees), &data.report),
                            money(excursion.map(|e| e.adverse), &data.report),
                            money(excursion.map(|e| e.favourable), &data.report),
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
