# Strategy reports

A host produces a [`model::StrategyReport`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html) with [`strategies::Backtest`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html) (the `strategies`
feature), or supplies one from its own strategy execution or analytics. The report serializes
without egui and its performance metrics work with `default-features = false`. The host
supplies the market data.
`examples/backtesting.rs` demonstrates both a headless run and, with [`ui`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/index.html), a report window.

## Try a host-produced report

On the published guide, select **Run example**. The host supplies two synthetic round trips
of 10 units bought at 100, sold at 105 and 98, with a fee of 2 on each round trip.
The net results are `(105 - 100) * 10 - 2 = 48` and `(98 - 100) * 10 - 2 = -22`.
The library computes net profit 26, a 50% win rate and profit factor `48 / 22`, about 2.18.
The supplied equity curve is 10,000, 10,048, 10,026, so maximum drawdown is 22.
It is too short to annualize: Sharpe displays N/A.

<!-- example -->

Change **Fee per round trip**, then select **Deliver host report**. The displayed report
changes only on delivery; repainting computes presentation metrics without running a
strategy or submitting orders. `host_result` stands for the host's execution or analytics;
`HostReport::show` renders the delivered `StrategyReport`. The small renderer uses the
existing widget bundle and needs no strategy engine or application UI feature. Hosts
wanting the complete report panel can use `StrategyReportPanel`, described below.
[Host data services](host-data-services.md#try-a-report-handoff) shows the same handoff in
the context of other host-owned services.

## Execute a strategy in the host

With the `strategies` feature, configure a run through [`Backtest::settings`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#structfield.settings):

```rust
use egui_charts::model::{Bar, OrderSide, OrderType, Timeframe};
use egui_charts::strategies::{Backtest, Commission, OrderSize, Slippage, Strategy, StrategyContext};

struct Long;
impl Strategy for Long {
    fn inputs(&self) -> Vec<(String, String)> {
        vec![("Direction".into(), "Long".into())]
    }
    fn on_bar(&mut self, ctx: &mut StrategyContext<'_>) {
        if ctx.position() == 0.0 {
            ctx.entry("Long", OrderSide::Buy, None, OrderType::Market);
        }
    }
}
let mut engine = Backtest::new("Long", "SAMPLE", Timeframe::Hour1, 10_000.0);
engine.settings.currency = Some("USD".into());
engine.settings.risk_free_rate = 0.02;
engine.settings.order_size = OrderSize::EquityFraction(0.1);
engine.settings.commission = Commission::PerOrder(1.0);
engine.settings.slippage = Slippage::Ticks { ticks: 1, tick_size: 0.01 };
engine.settings.leverage = 2.0;
let bars: Vec<Bar> = vec![]; // The host supplies history independently of the visible chart.
let lower: Vec<Bar> = vec![]; // Optional finer bars for the same instrument.
let report = engine.run(&mut Long, &bars, &lower);
```

An entry's explicit positive quantity overrides the default size. `None` uses fixed units,
a fraction of current marked equity, or cash divided by the absolute execution price.
Cash and equity sizing reject a zero execution price. The default pyramiding limit is one
open entry; zero disables entries. A partial exit retains its entry slot until that lot is
fully closed. An opposite entry reverses the position and starts a new entry count.

Commission is an absolute fill-value fraction, an amount per unit, or an amount per order.
Each fill pays once, even when reversing or closing several FIFO lots; trade fees and net
profits include their quantity shares of entry and exit commission. Open trades retain
their unallocated entry fees, already deducted from equity. Market and stop orders slip
against their side once per fill, by ticks times tick size or an absolute-price fraction.
Limits, including the limit execution of a stop-limit order, never slip.

Long and short exposure fund separately. [`long_funding`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.long_funding) and [`short_funding`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.short_funding)
are the fractions of absolute position market value that equity must cover, one over leverage
when unset. Zero disables that side's checks and values above one are accepted. An entry is
rejected if the funding of its resulting position plus commission exceeds equity after slippage,
using the funding of its own side, including on a reversal. The report counts rejected entries.

At each observed price point, equity below the required funding closes four times the shortfall
in units, truncated to [`liquidation_increment`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.liquidation_increment), oldest lots first. The fill is labelled `Margin call`,
with exit fees and market slippage. For example, capital 1,000 and 40 long units entered at 100
with 20% funding have equity 600 and require 720 at a price of 90; the shortfall of 120 buys 6.67
units of funding, truncated to 6, so 24 units close and 16 stay open. A deficit on a position
smaller than one step closes the whole position.

Runs with leverage above one and no fraction for the position's side keep whole-position
liquidation. Along every segment of the price path, equity below margin closes the whole position
at the crossing. An order at the boundary can avert the deficit; a gap already below margin closes
at its open. For example, capital 1,000 and 15 long units entered at 100 with leverage 2 give
equity `15p - 500` and margin `7.5p` at positive price `p`; a decline crosses at `p = 200/3`,
leaving equity 500 before exit costs, even if the bar later recovers.

[`require_limit_cross`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.require_limit_cross) makes limit entries, triggered stop-limits and profit-taking exits wait until
the price trades through the limit by one [`price_increment`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.price_increment). They then fill at the limit, or at
the open of a bar that gaps to or beyond that threshold. An order that can first fill at a price
already at or beyond the threshold, as after a delay or a triggered stop, fills at that price.
[`order_delay`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.order_delay) holds a new order back for that many further price points, so market orders
placed at a close fill at the next bar's open with zero, its first extreme with one and its second
extreme with two. Requesting a working order again with the same quantity, kind and prices leaves
it as it is, its delay included; a request that changes any of them restarts the delay.

[`recalculate_on_history`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.recalculate_on_history) replays each bar as its open, two extremes and close, or as those four
points of each finer bar, and decides at each with a candle holding only the prices reached so far.
`prepare` then receives no bars and `prepare_update` refreshes calculations before each decision.
It is unavailable on transformed chart types. [`chart_type`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.chart_type) set to Heikin Ashi derives decision candles
from the original bars you supply; these candles also set fills and equity unless
[`original_price_fills`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.original_price_fills) keeps those on the original prices. A whitespace
bar stays empty in those candles, and the next one continues from the last bar with a value.

[`recalculate_after_fills`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.recalculate_after_fills) calls the strategy again after every fill, with [`price()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.StrategyContext.html#method.price) equal
to the execution price and [`equity()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.StrategyContext.html#method.equity) marked there after commission. [`is_after_fill()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.StrategyContext.html#method.is_after_fill)
distinguishes it from a closed-bar decision. During this call, [`bars()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.StrategyContext.html#method.bars) contains only
previously closed chart bars, excluding the unfinished bar. Newly created orders wait for
a subsequent price point; they cannot repeatedly fill on the same tick. [`fill_on_close`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.fill_on_close)
tries working orders again on the closing tick: market orders fill and priced orders fill if the close satisfies them. All settings and [`Strategy::inputs()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/trait.Strategy.html#method.inputs) appear in
the report's Properties tab and summary CSV. Capital, currency and risk-free rate use the
report's existing metadata fields, so each appears once.

[`Backtest::run(strategy, bars, lower)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#method.run) uses valid lower-interval bars in timestamp order
within each chart bar's time span, with the end excluded. Duplicate finer timestamps use
the first valid bar. A chart bar without valid finer coverage uses its own path; gaps
between supplied finer bars fill at the next finer open. Monthly spans use calendar
months; execution bars end at the next chart open, with the final one covering remaining
finer bars. Only fills use finer timestamps; decisions and equity stay on the chart
interval. Without finer bars the existing nearest-extreme-first chart path is unchanged.

[`settings.start_time`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.start_time) selects the first eligible chart-bar open. Earlier chart bars cannot
place or fill orders, but still reach [`Strategy::prepare`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/trait.Strategy.html#method.prepare) and [`Strategy::on_bar`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/trait.Strategy.html#tymethod.on_bar) to warm
both precomputed studies and incremental strategy state. The host loads
both histories and can supply more data than its chart has loaded; no account or market
data service is required by the engine. With a start time, equity starts at that time and
subsequent values are stamped at chart-bar closes, preserving the starting capital point.
The final execution bar has no known closing time; its equity mark is provisionally labelled
one nanosecond after its last supplied price timestamp so the curve stays ordered.
Without a start time, the existing equity timestamps are retained. When comparing a
bounded run in the panel, supply only its eligible chart bars as the comparison source.

Migration: [`Backtest::initial_capital`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.initial_capital) and [`Backtest::fill_on_close`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.fill_on_close) move to
[`Backtest::settings`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#structfield.settings). [`Backtest::new`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#method.new) keeps its starting-capital argument. Add `&[]` as the
third argument to existing [`run`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#method.run) calls, or supply finer bars. Runs that intentionally stack
entries must set [`settings.pyramiding`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.pyramiding) to their limit; the default is now one. Entries also
need enough margin and commission in the account, with leverage one by default. Starting
capital must be finite.

With the [`ui`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/index.html) feature, keep a [`ui::strategy_report::StrategyReportPanel`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyReportPanel.html) in your host:

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

Call [`set_report`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyReportPanel.html#method.set_report) when a finished report or its source bars change. It computes and caches
metrics, clears selection and opens Summary. Mount [`show`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyReportPanel.html#method.show) in your panel below the chart,
with a separate egui id scope for each report component. [`set_report(None, &[])`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyReportPanel.html#method.set_report) clears
it; [`show`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyReportPanel.html#method.show) then takes no space and shows no controls. [`report()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyReportPanel.html#method.report) reads the current run.
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
CSV exports only valid trades using [`StrategyReport::trades_csv`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.trades_csv), retaining its existing
columns, quoting and UTC timestamps. Export summary CSV returns
[`StrategyReportAction::ExportSummaryCsv`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/enum.StrategyReportAction.html#variant.ExportSummaryCsv) with a suggested file name and the metrics and
properties shown, as [`StrategyReport::summary_csv`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.summary_csv) writes them: a dot as the decimal
separator, no thousands separator, and an empty cell for an unavailable value. Percent
values keep 15 significant decimal digits, so a rate of `0.07` reads `7`, and a zero is
always `0`. Both exports prefix a text cell that starts with `=`, `+`, `-`, `@`, a tab or a
carriage return with an apostrophe, so a spreadsheet shows it as text; numeric cells,
including negative numbers, are unchanged.

## Metric definitions

[`StrategyReport::performance`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.performance) retains the base definitions on [`StrategyPerformance`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyPerformance.html):
net and gross profit/loss use the host's net closed-trade profits; drawdown uses running
equity peaks. Sharpe uses sample standard deviation, Sortino uses the root mean square
of downside excess returns over all steps, and both use the curve's actual steps per
365.25-day year and require at least 13 weeks. Non-finite equity points are skipped;
unordered finite points invalidate the entire curve. [`equity_curve()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.equity_curve) exposes those same
validated points for rendering.

The largest winner is the greatest positive net profit, and the largest loser is the most
negative net profit. Either is unavailable without a trade of that sign. Consecutive wins
and losses use exit-time order, ties retaining input order. Breakeven trades break both
runs; invalid and open trades do not participate. For profits `5, 3, -1, -7, -2, 0, 4, 4, 0, 2`,
net profit is 8, gross profit 18, gross loss 10, largest winner 5, largest loser -7, and the
longest winning and losing runs are 2 and 3.

[`StrategyReport::comparison(source)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.comparison) takes the complete source bars of this run's symbol
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
