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
[`Strategy::default_order_size`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/trait.Strategy.html#method.default_order_size) suggests the strategy's usual size; the tester applies it on selection,
while [`Backtest::run`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#method.run) uses its settings alone. Select the strategy before restoring saved settings.

Commission is an absolute fill-value fraction, an amount per unit, or an amount per order.
Each fill pays once, even when reversing or closing several FIFO lots; trade fees and net
profits include their quantity shares of entry and exit commission. Open trades retain
their unallocated entry fees, already deducted from equity. Market and stop orders slip
against their side once per fill, by ticks times tick size, ticks of the price increment or an
absolute-price fraction. Limits, including the limit execution of a stop-limit order, never slip.

[`StrategyContext::exit_trailing`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.StrategyContext.html#method.exit_trailing)
protects a position with an optional fixed stop from its average fill price and a trailing stop.
The trail starts at an activation distance in the position's favour and follows the best price
the bar path reaches at an offset. It can activate and fill within one bar, and fills and slips
like other stops. Activation at a closing fill carries into the next bar. Requesting it with new
distances keeps the best price; closing the position forgets it.

A report is in `settings.currency`, or in [`Backtest::instrument_currency`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#structfield.instrument_currency), the currency of the
prices, when that is unset. When the settings name another currency, the engine converts what it
derives from prices into it: sizing, funding, margin and liquidation, fraction commissions,
profits and equity, at the rate in force at each fill and at the last price of each equity mark.
Supply [`Backtest::exchange_rates`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#structfield.exchange_rates), the host's answer to [`Backtest::exchange_rate_request`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#method.exchange_rate_request); each
point's close applies from its time until the next point. The initial capital, cash order sizes
and per-unit and per-order commissions are account amounts already. A run that needs a rate where
none applies, or where it is not finite and positive, reports [`StrategyReport::unavailable`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#structfield.unavailable)
instead of trades and equity. Each trade records the rate its profit converted at, and an open
trade the latest rate, in [`StrategyTrade::exchange_rate`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyTrade.html#structfield.exchange_rate);
its excursions convert at it. The buy-and-hold benchmark stays unconverted.

A strategy naming a [`calculation_timeframe`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/trait.Strategy.html#method.calculation_timeframe) reads the instrument's own bars of that interval from
[`Backtest::calculation_bars`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#structfield.calculation_bars), placed on [`Backtest::bar_clock`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#structfield.bar_clock). The tester asks its host for these bars;
pass the same instrument calendar to [`StrategyTester::set_bar_clock`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyTester.html#method.set_bar_clock). Each chart bar reads the last calculation
bar closed by its close. Without supplied calculation bars, Technical Ratings places no orders
on another Indicator timeframe. Its default remains the chart interval, which needs no extra history.

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

The report properties *Long margin calls* and *Short margin calls* name each side's rule:
`Four times the shortfall at price points`, `Whole position at the margin crossing`, or `Off`
when that side requires no funding. The tester shows the rule below each funding field.

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
Without a start time, the existing equity timestamps are retained.
[`settings.end_time`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.BacktestSettings.html#structfield.end_time) ends the run: chart bars that have not closed by then are not decided on,
filled or marked and do not reach [`Strategy::prepare`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/trait.Strategy.html#method.prepare). Trades still open remain open.
An execution bar closes when the next one opens, so the newest one does not count as closed.
[`Backtest::update`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#method.update) ignores the end. When comparing a bounded run in the panel, supply only its eligible
chart bars: those opening at or after the start and closing by the end.

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
        StrategyReportAction::ExportCsv { filename, content } => {
            // The host may save or download `content` under the suggested `filename`.
            // This example copies it.
            let _ = filename;
            ui.ctx().copy_text(content);
        }
        _ => {}
    }
}
```

Call [`set_report`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyReportPanel.html#method.set_report) when a finished report or its source bars change. It computes and caches
metrics, clears selection, resets the date range to the whole run and opens Summary. Mount [`show`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyReportPanel.html#method.show) in your panel below the chart,
with a separate egui id scope for each report component. [`set_report(None, &[])`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyReportPanel.html#method.set_report) clears
it; [`show`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyReportPanel.html#method.show) then takes no space and shows no controls. [`report()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyReportPanel.html#method.report) reads the current run.
Selecting a trade returns its symbol, interval, original index and entry/exit timestamps.
An open trade has no exit boundary. The host owns navigation, highlighting, report panel
placement, data loading and CSV storage. Chart entry/exit markers use the signals service.
The assembled terminal and its demo mounting remain separate integration work.

The Summary tab shows the selected range and its trade figures in All, Long and Short columns,
followed by open profit, drawdown, run-up, ratio, growth and buy-and-hold figures for that range. Charts of equity,
drawdown from peak and run-up from trough default to account units; **Percent of initial capital**
switches all three to percentages of the selected range's initial capital, with equity starting at 100. The
optional fee-free buy-and-hold comparison uses the same equity scale. Drawdown and run-up follow
usable intrabar ranges, falling back to closes when ranges are unavailable. Replacing the report
resets the switch to account units; the switch requires positive finite initial capital.
The intrabar figures and all three charts follow the date range. Curve positions and hover
values retain submillisecond timestamps. Time labels along the axis follow its span. Hover a curve
for timestamped values. The virtualized trade list uses entry-time order, keeps equal-time
trades in input order, filters to the range's trades and shows fill prices, net profit,
cumulative profit, fees, labels and excursions. Cumulative profit adds the net profit of the
listed trades closed in the range, in list order up to and including the row. An open trade
has an empty cell and adds nothing, as does a trade that closes at or after the range's end,
which the range counts as open like the Summary.
Fill prices retain their precision and fill times retain fractional seconds.
Properties includes run metadata and the host's ordered settings. Invalid inputs are
counted visibly. An undefined or non-finite metric displays as `N/A`, including when
converting a fraction to a displayed percentage overflows.
Both exports cover the whole run whatever date range is selected.
Export trades CSV writes valid trades using
[`StrategyReport::trades_csv_with_excursions`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.trades_csv_with_excursions): the columns of
[`StrategyReport::trades_csv`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.trades_csv) with the displayed excursions in the `mae` and `mfe` cells,
empty when unavailable, and the same quoting and UTC timestamps. The last column,
`cumulative_profit`, is the whole run's running total of `profit` in row order, empty on an
open trade, which adds nothing, and when the total is not finite. Export summary CSV returns
[`StrategyReportAction::ExportSummaryCsv`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/enum.StrategyReportAction.html#variant.ExportSummaryCsv) with the whole-run metrics and
properties, as [`StrategyReport::summary_csv`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.summary_csv) writes them: a dot as the decimal
separator, no thousands separator, and an empty cell for an unavailable value. Percent
values keep 15 significant decimal digits, so a rate of `0.07` reads `7`, and a zero is
always `0`. Both exports prefix a text cell that starts with `=`, `+`, `-`, `@`, a tab or a
carriage return with an apostrophe, so a spreadsheet shows it as text; numeric cells,
including negative numbers, are unchanged. The summary CSV ends with a `side` column:
`all`, `long` or `short` on trade figures, with one row per side in that order, and empty
on whole-run figures and properties. Largest position uses the unit `quantity` and holding
times use `bars`.
Both export actions carry a suggested file name, such as
`First_run_2_EUR___USD_1min_trades_2026-11-21_14-03-07.csv`: the run's name, symbol and
interval, `trades` or `summary`, and the local time of the export to the second, with every
character other than an ASCII letter, a digit, `-` or `_` replaced by `_`. Two exports of one
run within the same second get the same name.

With `ui` and `strategies`, [`StrategyTester`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyTester.html)
keeps the engine and strategy of a completed Run. As `show` receives new closed bars, it
continues the report to the same result as a fresh Run over the longer history and returns
[`StrategyTesterAction::Signals`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/enum.StrategyTesterAction.html#variant.Signals) to replace the chart's fill markers. The report keeps its
open tab, buy-and-hold toggle, date range and selected entry when that entry remains;
[`selected_trade`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/struct.StrategyTester.html#method.selected_trade) returns its index in the report's trades, so a chart highlight can
follow it. A corrected processed chart bar rebuilds the run; a shorter history ends
continuation and marks the report stale. For a run on host history, a changed or missing
chart bar at or before its last host bar also ends continuation and marks it stale, since
newer host history is needed. Chart bars that close later within the host history are
already part of the run.

Runs with finer fill bars, currency conversion or another calculation interval request the finer
bars, exchange rates or calculation bars for each new chart bar with [`StrategyTesterAction::RequestHistory`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/strategy_report/enum.StrategyTesterAction.html#variant.RequestHistory) and stay stale while
waiting. Answer through `set_history`, `set_history_with_rates` or `set_history_with_calculation`;
continuation uses the reply's finer bars, rates and calculation bars with the chart history
captured when requested. A calculation reply starts with the bar containing the requested start,
so it can update the last held interval bar. A failed
request ends continuation and preserves the last report as stale. The host supplies these
data; the library fetches no market history or rates. End dates still bound the run.
Optimisation results do not continue. Form edits, symbol or interval changes, Run and
`set_report` end continuation; editing optimisation ranges or running an optimisation
keeps the single run following its chart. Continuation leaves the tester's form tab open.

A host running its own engine uses [`Backtest::update`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#method.update)
while the final bar is forming, or [`Backtest::update_closed_async`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/strategies/struct.Backtest.html#method.update_closed_async)
when every bar is closed. Closed-bar updates respect the historical end date and do not
make forming-bar decisions, even when live recalculation is enabled.

## Date ranges and analysis

From (UTC) and To (UTC) select whole days in `YYYY-MM-DD` format, including the To day.
Invalid dates leave the last valid range displayed. Whole run clears the filter; supplying
another report also resets it. The summary, charts, trade list and the Periods, Distribution
and Trade times pages all follow this range. The chart span changes with the range.
A report whose figures are unavailable shows its reason instead of date controls, summary,
analysis pages and exports; `within` retains that reason.

[`StrategyReport::within(range)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.within)
provides the same calculations without UI, including intraday ranges. Its start is included
and its end excluded. It keeps valid trades closed in that interval, including trades entered
earlier, and trades entered before the end that are still open there, with their exits removed.
The panel's trade list retains their original fills and original indices for chart selection.
Only usable equity points inside the range remain. Drawdown and ratios start at its first
point. If the range starts after the curve's first point, its first included equity becomes
initial capital; without a point capital is NaN and the net return is unavailable. Otherwise
capital stays as supplied. Empty or reversed ranges contain no trades or equity. Invalid
inputs are omitted, leaving skipped counts at zero; the panel keeps the whole-run warning.
Holding times use the whole run's source bars, including bars before the range for a trade
entered earlier and closed inside it.

[`StrategyReport::analysis(bars)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.analysis)
returns [`StrategyAnalysis`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyAnalysis.html), usable without egui.
Its period, histogram and time-group records are [`StrategyPeriod`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyPeriod.html),
[`StrategyResultBin`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyResultBin.html) and
[`StrategyTradeGroup`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyTradeGroup.html). Combine it with `within` for a period.
Pass the source bars for that range, or an empty slice without a benchmark. Months and years
use UTC boundaries and include periods with equity points or closed-trade exits. Each lists
net profit and trade count by exit time. The equity return is the last point in the period
divided by the last point before it, minus one; without an earlier point, its own first point
is the base. Buy-and-hold uses valid closes the same way, with the first valid open as its
initial base. Missing periods do not reset either base. A period without a point or bar,
without a positive base or with a non-finite return shows N/A. Source validation is the same
as `comparison`. Successive period returns compound to last equity / first equity - 1, or
last close / first open - 1 for buy-and-hold.

The distribution uses each closed trade's net profit divided by quantity times absolute
entry price. Non-finite results, including zero entry value, do not enter the histogram.
Entry value stays in the instrument's currency, so the results of a run
converted into another account currency also carry the exchange rate.
Bin widths follow 0.001, 0.002, 0.005, 0.01, 0.02, 0.05 and so on, choosing the first width
that fits every result in at most 20 consecutive bins. Lower edges are included, upper edges
excluded. Results within a billionth of an edge (of the width at zero) snap to it to remove
binary rounding noise; a loss stays below zero. Winners, losers and breakevens have their
own colours and hover counts. Run counts use exit order, with ties in input order and
breakevens breaking both runs: `win_runs[n - 1]` counts winning runs of exactly `n` trades,
and `loss_runs` counts losing runs. Open and invalid trades do not participate.

Trade times group closed valid trades by entry hour, weekday (Monday first) and month,
all in UTC, with count, net profit and win rate. Breakevens count in the win-rate denominator.
Empty groups have zero trades and profit and an unavailable win rate.

```rust
use chrono::{TimeZone, Utc};
use egui_charts::model::{OrderSide, StrategyExit, StrategyReport, StrategyTrade, Timeframe};

let day = |m, d| Utc.with_ymd_and_hms(2024, m, d, 0, 0, 0).unwrap();
let mut report = StrategyReport::new("Swing", "SAMPLE", Timeframe::Day1, 1_000.0);
for (entry, exit, profit) in [(day(1, 2), day(1, 9), 50.0), (day(2, 1), day(2, 8), -30.0)] {
    let mut trade = StrategyTrade::new(OrderSide::Buy, 10.0, entry, 100.0);
    trade.exit = Some(StrategyExit::new(exit, 100.0 + profit / 10.0, profit));
    report.trades.push(trade);
}
report.equity = vec![
    (day(1, 1), 1_000.0),
    (day(1, 31), 1_050.0),
    (day(2, 1), 1_050.0),
    (day(2, 15), 1_000.0),
    (day(2, 29), 1_020.0),
];

// February starts from the account value on 1 February.
let february = report.within(day(2, 1)..day(3, 1)).performance();
assert_eq!(february.all.closed_trades, 1);
assert_eq!(february.all.net_profit, -30.0);
assert_eq!(february.max_drawdown, Some(50.0));

let analysis = report.analysis(&[]);
assert_eq!(analysis.months.len(), 2);
assert_eq!(analysis.months[0].trades.net_profit, 50.0);
// 2 January 2024 is a Tuesday, the second weekday group.
assert_eq!(analysis.by_weekday[1].closed_trades, 1);
```

## Metric definitions

[`StrategyReport::performance`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.performance) retains the base definitions on [`StrategyPerformance`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyPerformance.html):
net and gross profit/loss use the host's net closed-trade profits; drawdown uses running
equity peaks. Sharpe uses sample standard deviation, Sortino uses the root mean square
of downside excess returns over all steps, and both use the curve's actual steps per
365.25-day year and require at least 13 weeks. Non-finite equity points are skipped;
unordered finite points invalidate the entire curve. [`equity_curve()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.equity_curve) exposes those same
validated points for rendering.

Drawdown and run-up are reported separately from closing marks and from equity inside each bar.
Close-to-close drawdown is the largest fall below an earlier closing peak. Intrabar drawdown
also uses [`StrategyReport::equity_ranges`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#structfield.equity_ranges), one [`EquityRange`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.EquityRange.html) per mark: its bar's low, high,
largest fall and largest rise along the execution path, including finer bars and the close.
Each bar's depth is the larger of the earlier peak less its low and its own fall. Peak-relative
percentages take the larger of `(earlier peak - low) / earlier peak` and
`fall / max(earlier peak, high)`. If the largest fall starts below both peaks, this second
term understates that fall's own percentage. A separate percentage divides the maximum
intrabar money drawdown by initial capital. Missing or unusable ranges make intrabar metrics
unavailable; old saved reports load with empty ranges. Hosts knowing only the extremes can
supply `high - low` for both fall and rise, yielding upper bounds.

For capital 10,000 and 100 units bought at 100, a bar dipping to 70 and closing at 100 has
close-to-close drawdown 0 and intrabar drawdown 3,000 (30%). Open profit reconciles equity with
closed trades: last usable equity less initial capital and closed net profit, including entry
fees already paid. It is exactly 0 without an open trade, or unavailable when an open trade
has no usable equity mark. Holding those units at a final close of 90 gives open profit -1,000
and closed net profit 0. The summary and summary CSV list open profit as a whole-run row and
label both drawdown measures explicitly. Trade figures retain their All, Long and Short columns;
whole-run CSV rows have an empty `side`.

Run-up is the largest rise above an earlier equity trough. Close-to-close run-up uses closing
marks; intrabar run-up takes, at each bar, the larger of its high less the earlier trough and
its own rise. Both run-up percentages divide by initial capital and require positive finite
capital. A path of equity 10,000, 12,000, 7,000, 10,000 has largest fall 5,000 but largest rise
3,000 (7,000 to 10,000), so knowing only the high and low cannot recover both movements.
The summary and CSV distinguish close-to-close and intrabar run-up, and the run-up chart shows
each bar's rise from its trough.

The largest winner is the greatest positive net profit, and the largest loser is the most
negative net profit. Either is unavailable without a trade of that sign. Consecutive wins
and losses use exit-time order, ties retaining input order. Breakeven trades break both
runs; invalid and open trades do not participate. For profits `5, 3, -1, -7, -2, 0, 4, 4, 0, 2`,
net profit is 8, gross profit 18, gross loss 10, largest winner 5, largest loser -7, and the
longest winning and losing runs are 2 and 3.

### Figures by side

Read the trade figures through `performance.all`, `performance.long` and `performance.short`,
each a `StrategyTradeStats`. Migration: replace `performance.net_profit` with
`performance.all.net_profit`, and likewise for the other trade fields. Summary CSV readers
that select by figure name should also select `side = all` for the former aggregate value.

Each side applies the same validity rules. A positive closed-trade net profit wins, a
negative one loses, and zero is even. Even trades count as closed and enter the win rate's
denominator and average trade, without entering either winning or losing figures. Open
trades enter only the open count and largest position. Average winning trade is gross
profit divided by winners; average losing trade is gross loss divided by losers, a positive
amount. Their ratio needs both. The largest winner's fraction uses gross profit; the largest
loser's fraction uses gross loss and a positive loss magnitude. Net profit over the largest
loss is a multiple, negative for a net loss. With closed trades long +50, long -20, short +30
and short 0, there are 2 winners, 1 loser and 1 even trade. The average winner is 40, average
loser 20 and their ratio 2. Longs and shorts each net 30; long profit factor is 2.5 and short
profit factor is unavailable because there is no short loss.

Largest position is the greatest summed quantity held at once, including open trades.
Holdings include entry and exclude exit, so reversing in one fill never adds the closing
and opening positions. A trade closed at its entry time still counts at that instant.
Long and short quantities add in the aggregate, allowing host reports with both held.
Two overlapping longs of 2 and 3 units therefore give 5.

Holding times come from the source bars passed to
[`StrategyReport::comparison`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.comparison).
Bars held are the exit bar's position minus the entry bar's: entry on bar 3 and exit on bar 7
holds 4 bars, and entry and exit within one bar holds 0. A fill belongs to the last source
bar opening at or before it, including in a session gap. Bars with invalid prices still
count. Averages cover closed trades, winners and losers of each side; open trades and
entries before the first source bar are excluded. A reversal ends one trade's count at its
bar and starts the new trade's count there.

Return over maximum drawdown divides net profit by the maximum drawdown in money; it needs
a positive drawdown, without an annualisation minimum. Compound annual growth uses
`(last / first)^(1 / years) - 1` over the usable equity curve, with 365.25 days per year.
It requires at least 13 weeks, a positive first value and a nonnegative final value; a zero
final value gives -100%. Equity 1,000, 900, 1,210 across two years grows 10% per year;
net profit 60 divided by drawdown 100 gives 0.6. Buy-and-hold profit multiplies its fractional
return by positive finite initial capital. Outperformance subtracts that profit from the
strategy's closed-trade net profit, excluding open trades. A 25% return on 1,000 makes 250,
so net profit 60 outperforms it by -190.

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
before fees: price moves times quantity, times the trade's `exchange_rate` into the report's
currency. They require host conversion for point values or inverse contracts. At rate 1, with
low 85 and high 115, a long entry at 100 of 2 units has MAE/MFE 30/30; a short entry at 110
of 3 units has 15/75.

The bar containing entry and subsequent bars opening strictly before exit participate, so
an exit at a bar's open does not include that bar's later range. Time bars end after their
interval, using calendar months for monthly bars. Execution bars end at the next source
open; the final execution bar is assumed to cover the run's remaining fills.
Open trades use all remaining source bars.
At least one held bar is required. An invalid held bar, or an `exchange_rate` that is not
finite and positive, makes the excursion unavailable.
OHLC cannot resolve movement on either side of an intrabar fill: these are bar-resolution
estimates in that case. Use fills at bar boundaries for exact spans, or finer source data
in host analytics when exact intrabar excursions are required.
