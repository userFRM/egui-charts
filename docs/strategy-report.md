# Strategy reports

A host produces a `model::StrategyReport` from its own strategy execution or analytics.
The model serializes without egui and works with `default-features = false`. The library
computes the performance metrics; it never runs the strategy or fetches market data.
`examples/backtesting.rs` demonstrates both a headless run and, with `ui`, a report window.

With the `ui` feature, keep a `ui::strategy_report::StrategyReportPanel` in your host:

```rust
use egui_charts::model::{Bar, StrategyReport};
use egui_charts::ui::strategy_report::{StrategyReportAction, StrategyReportPanel};

fn replace(panel: &mut StrategyReportPanel, run: StrategyReport, source: &[Bar]) {
    // Source bars must cover exactly this run, for its symbol and interval.
    // Passing [] keeps trade/equity metrics available without comparison or excursions.
    panel.set_report(Some(run), source);
}

fn render(ui: &mut egui::Ui, panel: &mut StrategyReportPanel) {
    match panel.show(ui) {
        StrategyReportAction::SelectTrade(selection) => {
            // The host switches to selection.symbol and selection.timeframe, scrolls to
            // selection.entry_time and highlights through selection.exit_time.
            // trade_index addresses the original report, including skipped input rows.
            let _ = selection;
        }
        StrategyReportAction::ExportCsv(csv) => {
            // The host may save or download this string. This example copies it.
            ui.ctx().copy_text(csv);
        }
        _ => {}
    }
}
```

Call `set_report` when a finished report or its source bars change. It computes and caches
metrics, clears selection and opens Summary. Mount `show` in your panel below the chart,
with a separate egui id scope for each report component. `set_report(None, &[])` clears
it; `show` then takes no space and shows no controls. `report()` reads the current run.
Selecting a trade returns its symbol, interval, original index and entry/exit timestamps.
An open trade has no exit boundary. The host owns navigation, highlighting, report panel
placement, data loading and CSV storage. Chart entry/exit markers use the signals service.
The assembled terminal and its demo mounting remain separate integration work.

The Summary tab shows all performance metrics, equity and drawdown in account units,
and an optional fee-free buy-and-hold comparison on the same equity scale. Curve positions
and hover values retain submillisecond timestamps. Hover a curve
for timestamped values. The virtualized trade list uses entry-time order, keeps equal-time
trades in input order, and shows fill prices, net profit, fees, labels and excursions.
Fill prices retain their precision and fill times retain fractional seconds.
Properties includes run metadata and the host's ordered settings. Invalid inputs are
counted visibly. An undefined or non-finite metric displays as `N/A`, including when
converting a fraction to a displayed percentage overflows.
CSV exports only valid trades using `StrategyReport::trades_csv`, retaining its existing
columns, quoting and UTC timestamps. Export summary CSV returns
`StrategyReportAction::ExportSummaryCsv` with a suggested file name and the metrics and
properties shown, as `StrategyReport::summary_csv` writes them: a dot as the decimal
separator, no thousands separator, and an empty cell for an unavailable value.

## Metric definitions

`StrategyReport::performance` retains the base definitions on `StrategyPerformance`:
net and gross profit/loss use the host's net closed-trade profits; drawdown uses running
equity peaks. Sharpe uses sample standard deviation, Sortino uses the root mean square
of downside excess returns over all steps, and both use the curve's actual steps per
365.25-day year and require at least 13 weeks. Non-finite equity points are skipped;
unordered finite points invalidate the entire curve. `equity_curve()` exposes those same
validated points for rendering.

The largest winner is the greatest positive net profit, and the largest loser is the most
negative net profit. Either is unavailable without a trade of that sign. Consecutive wins
and losses use exit-time order, ties retaining input order. Breakeven trades break both
runs; invalid and open trades do not participate. For profits `5, 3, -1, -7, -2, 0, 4, 4, 0, 2`,
net profit is 8, gross profit 18, gross loss 10, largest winner 5, largest loser -7, and the
longest winning and losing runs are 2 and 3.

`StrategyReport::comparison(source)` takes the complete source bars of this run's symbol
and interval, with timestamps denoting bar opens. The host must supply exactly the run's
bars, including every held bar; the library cannot detect a missing session or a wrong
symbol from OHLC alone. Non-finite or inverted OHLC bars are skipped and counted; unordered
or duplicate timestamps invalidate the whole source. Volume is not used.

Buy-and-hold invests all initial capital in fractional units at the first valid open and
marks them at each valid close, without fees or dividends. Fractional return is
`last_close / first_open - 1`; the open must be positive. Its curve additionally requires
positive finite initial capital. For capital 1,000, first open 100, and closes 110, 90, 120,
the benchmark owns 10 units, equity is 1,100, 900, 1,200, and return is 20%. Curve timestamps
are the bars' timestamps, as in a price chart. This independent benchmark is not sized to
the strategy's exposure.

Excursions use entry price `E`, source low `L` and high `H` over the held bars, and quantity
`Q`, including entry and exit fills in the extrema. Long MAE is `max(E-L, 0)*Q` and MFE is
`max(H-E, 0)*Q`; short MAE is `max(H-E, 0)*Q` and MFE is `max(E-L, 0)*Q`. Values are nonnegative,
before fees, in price times quantity units. They require host conversion for point values,
inverse contracts or different account currencies. With low 85 and high 115, a long entry
at 100 of 2 units has MAE/MFE 30/30; a short entry at 110 of 3 units has 15/75.

The bar containing entry and subsequent bars opening strictly before exit participate, so
an exit at a bar's open does not include that bar's later range. Time bars end after their
interval, using calendar months for monthly bars. Execution bars end at the next source
open; the final execution bar is assumed to cover the run's remaining fills.
Open trades use all remaining source bars.
At least one held bar is required, and an invalid held bar makes the excursion unavailable.
OHLC cannot resolve movement on either side of an intrabar fill: these are bar-resolution
estimates in that case. Use fills at bar boundaries for exact spans, or finer source data
in host analytics when exact intrabar excursions are required.
