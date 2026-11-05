# Builder guide

Create charts once and call [`Chart::show`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.show) every frame. The host supplies historical
and streaming data and owns network connections, files, accounts and script execution.
The library computes studies and strategy metrics and renders the supplied data.

Continue from [your first chart](site/src/content/docs/first-chart.mdx) with the
[second-series tutorial](https://userfrm.github.io/egui-charts/docs/tutorial/second-series/).
The [interaction guide](site/src/content/docs/interaction.mdx) covers pointer,
keyboard, crosshair and draining events. Choose a complete runnable recipe:

| Task | Recipe |
|---|---|
| Add host annotations | [Price lines and markers](https://userfrm.github.io/egui-charts/docs/recipes/annotations/) |
| Separate price and volume units | [Price and volume](https://userfrm.github.io/egui-charts/docs/recipes/price-volume/) |
| Read chart events into host UI | [Host legend and tooltip](https://userfrm.github.io/egui-charts/docs/recipes/host-readout/) |
| Share cursor and view between charts | [Synchronize two charts](https://userfrm.github.io/egui-charts/docs/recipes/synchronized/) |
| Receive image bytes and deliver them | [Export an image](https://userfrm.github.io/egui-charts/docs/recipes/export/) |

## History and streaming

For data already in memory, construct a chart from ordered UTC bars:

```rust
use chrono::{TimeDelta, TimeZone, Utc};
use egui_charts::{Chart, model::{Bar, BarData, Timeframe}, widget::SeriesId};

let start = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap();
let mut chart = Chart::new(BarData::from_bars(vec![
    Bar::new(start, 100.0, 102.0, 99.0, 101.0, 1_200.0),
]));
chart.set_symbol("SAMPLE");
chart.set_timeframe(Timeframe::Hour1);
// Updating the last timestamp replaces its forming bar; a later timestamp appends.
chart.update(SeriesId::MAIN, Bar::new(
    start + TimeDelta::hours(1), 101.0, 103.0, 100.0, 102.0, 900.0,
));
assert_eq!(chart.bars(SeriesId::MAIN).len(), 2);
```

For asynchronous history and streaming, implement [`data::Datafeed`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.Datafeed.html) and pass it to
[`ChartBuilder::with_feed`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.ChartBuilder.html#method.with_feed). [`history`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.Datafeed.html#tymethod.history) answers its [`Reply<HistoryPage>`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Reply.html) with the newest
requested bars strictly before [`HistoryRequest::before`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.HistoryRequest.html#structfield.before), oldest first. Set
[`end_of_history`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.HistoryPage.html#structfield.end_of_history) only when no older page remains. Keep I/O off the UI thread: a
request's reply can be sent later from the host's worker.

In [`subscribe`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.Datafeed.html#method.subscribe), retain the [`Subscription::sink`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Subscription.html#structfield.sink) and push bars with [`push_bar`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Sink.html#method.push_bar), or
executions with [`push_tick`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Sink.html#method.push_tick). Ticks build bars using the chart's interval. Stop
producing when [`Sink::is_active`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Sink.html#method.is_active) becomes false. Symbol and interval changes end
the old subscription; implement [`unsubscribe`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.Datafeed.html#method.unsubscribe) when the backend needs explicit
cleanup. The sink wakes the chart when data arrives. [`Chart::feed_state`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.feed_state) exposes
loading, failures and rejected updates. See `examples/live_feed.rs` for a complete
paged feed with a cancellable producer. With the optional `http` feature,
[`data::http::HttpDatafeed::new(address)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/http/struct.HttpDatafeed.html#method.new) connects to a column-based HTTP server
without another [`Datafeed`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.Datafeed.html) implementation. `examples/network_feed.rs` starts a
matching local server; run it with `cargo run --features http --example network_feed`.

Apply a [`theme::Theme`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/theme/struct.Theme.html) to egui at startup with [`theme::apply_to_egui`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/theme/fn.apply_to_egui.html). Render with
[`chart.show(ui)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.show) inside a host panel. Drain [`Chart::drain_events`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.drain_events) after showing the
chart to handle clicks, crosshair movement and visible-range changes.

## HTTP endpoint contract

Enable `http` and attach [`Arc::new(data::http::HttpDatafeed::new(address))`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/http/struct.HttpDatafeed.html#method.new) with
[`ChartBuilder::with_feed`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.ChartBuilder.html#method.with_feed). The same adapter implements [`services::SymbolSearch`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/services/trait.SymbolSearch.html):
pass that `Arc` to the host's symbol search dialog. The address is the endpoint
root, optionally with a path prefix. The host supplies and operates the server,
its market data and authorization. Browser servers must allow the app's origin
with CORS. The adapter uses GET with percent-encoded query values.

| Endpoint | Request | JSON response |
|---|---|---|
| `/config` | No parameters | `{"supported_resolutions":["1","15","1D"],"exchanges":[{"value":"X"}],"symbols_types":[{"value":"stock"}]}` |
| `/symbols` | `symbol`; optional `conversion` | One symbol object as below |
| `/search` | `query`, `type`, `exchange`, `offset`, [`limit`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/services/struct.SymbolQuery.html#structfield.limit); `country`, `sector`, `source` when set | Array of symbol objects, best first, after the requested offset, at most the limit; empty filters mean all |
| `/history` | `symbol`, `resolution`, `countback`, `extended_hours`; optional `from`, `to`, `conversion` | Column history as below |

A symbol object requires `name` (`symbol` is also accepted). Optional `ticker`
is the identifier used for subsequent requests. Metadata fields are `description`,
`exchange`, `country`, `sector`, `source`, `root`, `logo`, `exchange_logo`,
`type` (lowercase model symbol type), `minmov`, `pricescale`,
`volume_precision`, `currency_code`, `timezone`, `session`, `supported_resolutions`
and `data_status` (`streaming`, `delayed_streaming` or `endofday`). Price tick size
is `minmov / pricescale`, with `minmov` defaulting to one. Time zones use the
library's [`TimezoneId`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.TimezoneId.html) parser. Sessions accept `24x7` or
`HHMM-HHMM[,HHMM-HHMM]:days` windows (one day suffix for all windows), where 1 is Sunday and 7 is Saturday; an overnight
window belongs to its closing day. Omitted days mean Monday through Friday. Unsupported
session syntax or unavailable time zones report invalid data rather than guessing;
search leaves out a result it cannot read and lists the others.

Configuration lists may be omitted (unknown, unrestricted) or empty (none).
Exchange and type entries use their `value` field. Optional feed services are
reported as unavailable: this adapter supplies bars and symbol search. Resolution
strings are minutes (`1`, `15`, `240`), seconds (`1S`, `45S`), days (`1D`), weeks
(`1W`, `2W`) or calendar months (`1M`, `3M`). Subsecond and execution-based bars
are not supported by this seconds-based endpoint contract. [`resolve_converted`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.Datafeed.html#method.resolve_converted)
passes the chosen conversion to `/symbols` so metadata and bars share the same unit.

History timestamps and bounds are Unix seconds in UTC; bounds may include a
fraction. `from` is inclusive and `to` exclusive, and an omitted bound is
unbounded. A bar of whole days, weeks or months is dated by the UTC calendar
date of its timestamp, such as 00:00 UTC on its trading date or a session open
that falls on that date in UTC. The adapter
places it in the day, week or month holding that date in the time zone the
chart displays, and bounds such requests at 00:00 UTC on a date. Return the
newest `countback` bars within those bounds. Keep interval, extended-hours and
conversion semantics on the server; return an HTTP error for
an unsupported request rather than serving different prices under those labels.
For example:

```json
{"s":"ok","t":[60,120],"o":[10,11],"h":[12,13],"l":[9,10],"c":[11,12],"v":[100,200]}
```

Each index is one bar; all supplied columns must have equal lengths. `v` may be
omitted when volume is unavailable (zero). Columns of different lengths, or
`no_data` with rows, report [`FeedError::InvalidData`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/enum.FeedError.html#variant.InvalidData). An invalid row costs only its
own bar: the chart drops a bar whose high or low does not contain its open and
close, or whose volume is negative, and counts it in
[`FeedState::rejected_bars`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.FeedState.html#structfield.rejected_bars); the adapter drops a row whose timestamp is out of
range and logs a warning. The adapter orders bars, enforces bounds and keeps
at most the requested count. Optional `end_of_history: true` says no older bars
exist. An empty window may answer `{"s":"no_data","nextTime":0}` to continue
at the bar of that timestamp. The adapter advances the bar's time by one second
(or half the gap to `to`, if smaller) to form an exclusive continuation bound
that includes the named bar and still moves backward. `nextTime` must be earlier
than the requested `to`.
Without continuation, `no_data` ends paging. `{"s":"error","errmsg":"..."}`
reports invalid data. HTTP 404 maps to symbol not found, 429 to rate limited
(with `Retry-After` seconds or HTTP date), and 408 or 5xx to a recoverable network
error. Other unsuccessful statuses report invalid data. A request whose response
has not arrived within 10 seconds also reports a recoverable network error, and
a later response is discarded. Historical request retries follow the chart's
normal retry policy.

Live subscriptions poll the same history endpoint every five seconds, starting
with the latest 500 bars and then including the newest received timestamp so the
forming bar can change. Each response replaces full bars, including volume;
no trades are synthesized. The server must return the complete requested window
within 500 bars between refreshes. If a full response no longer overlaps the
previous refresh, the adapter reports invalid data and stops; reload history.
Recoverable live failures back off to 60 seconds and respect a longer
`Retry-After`; permanent failures stop the subscription. Unsubscribe and chart
drop cancel further requests and discard any pending response.

## Custom studies

[`studies::CustomIndicator`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.CustomIndicator.html) accepts a calculation closure. Return one value per
input bar, preserving unavailable warm-up values with [`IndicatorValue::None`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/enum.IndicatorValue.html#variant.None):

```rust
use egui_charts::studies::{CustomIndicator, Indicator, IndicatorValue};
use egui_charts::model::Bar;

let mut change = CustomIndicator::new("One-bar change", |bars| {
    bars.iter().enumerate().map(|(i, bar)| {
        if i == 0 {
            IndicatorValue::None
        } else {
            IndicatorValue::Single(bar.close - bars[i - 1].close)
        }
    }).collect()
}).with_overlay(false);
let bar = |close| Bar::new(chrono::DateTime::UNIX_EPOCH, close, close, close, close, 1.0);
change.calculate(&[bar(100.0), bar(103.0), bar(101.0)]);
assert_eq!(change.values().value(0, 0), None);
assert_eq!(change.values().value(0, 1), Some(3.0));
assert_eq!(change.values().value(0, 2), Some(-2.0));
// chart.add_indicator(Box::new(change));
```

The chart recalculates the closure when bars change. Implement [`studies::Indicator`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html)
directly when incremental calculation is needed. [`CustomIndicator::with_inputs`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.CustomIndicator.html#method.with_inputs)
declares validated editable settings. Additional symbol and interval histories use
[`studies::StudyDataRequest`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.StudyDataRequest.html).

## Save and load drawings

Enable `drawings`. Save through the chart so anchors retain timestamps when history
is prepended or the interval changes. Validate and serialize before replacing a
previous save. Load after setting the matching symbol; history may arrive later.

```rust
# #[cfg(feature = "drawings")] {
use egui_charts::{Chart, model::BarData};

let mut chart = Chart::new(BarData::new());
chart.set_symbol("SAMPLE");
let json = chart.save_drawings().unwrap().to_json().unwrap();
let skipped = chart.load_drawings(&json).unwrap();
assert!(skipped.is_empty());
# }
```

[`load_drawings`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.load_drawings) rejects unreadable files and symbol mismatches without replacing
current drawings. Report skipped records to the user. The host stores the JSON in
its own file or service. For automatic per-symbol persistence, implement
[`services::Storage`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/services/trait.Storage.html) and attach it with [`Chart::set_storage`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_storage). With the [`ui`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/index.html) feature,
[`Chart::snapshot`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.snapshot) and [`Chart::restore`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.restore) additionally save and restore chart settings,
studies and view.

## Synchronize charts

Exchange UTC positions between charts, including charts with different intervals
or history lengths. Local bar indices and spacing depend on each chart's data and
width. After showing the source chart:

```rust
# use egui_charts::{Chart, model::BarData};
# let source = Chart::new(BarData::new());
# let mut target = Chart::new(BarData::new());
target.set_synced_crosshair_time(source.hover_time());
if let Some(range) = source.visible_time_range() {
    target.apply_synced_time_range(range);
}
```

Pick the chart under the pointer as the source, clear crosshairs when the pointer
leaves, and request a repaint after changing a chart already drawn in this frame.
Each target must have been shown once to know its plot width. Synchronize the
viewport only when the source's view changes in a pan or zoom, not when the pointer
moves to another chart, to avoid feedback between differing widths and intervals.

## Strategy reports and scripts

Run a backtest in the host, recording entry and exit fills, fees and equity at each
bar close. Build a [`model::StrategyReport`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html) with [`StrategyTrade`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyTrade.html) and [`StrategyExit`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyExit.html);
[`StrategyReport::performance`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.performance) computes the metrics and [`trades_csv`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.StrategyReport.html#method.trades_csv) exports fills.
For a long trade of two units from 100 to 110 with total fees of 2, report net
profit `(110 - 100) * 2 - 2 = 18`. Fees are already included in the exit's profit.
Execution rules, slippage, contract sizing and data retrieval belong to the host.

The library supplies no scripting runtime. Evaluate scripts in a host runtime and
return their per-bar results through [`CustomIndicator`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.CustomIndicator.html), with missing values for
unavailable observations. Recalculate when the source bars or script inputs change;
report compilation and execution errors in the host UI. A closure may capture an
`Arc` holding the host evaluator; its calculation must be `Send + Sync`.

## Runnable examples

Run `cargo run --example <name>` from a source checkout. These examples use the
public API and generated sample data.

| Name | Workflow |
|---|---|
| `live_feed` | Paged historical bars and streaming executions |
| `network_feed` | Reusable HTTP column history and live bar refresh (`--features http`), natively and in the browser |
| `custom_indicator` | Ten-bar price change with explicit warm-up values |
| `drawing_persistence` | Editable drawing JSON, save/load and error reporting |
| `backtesting` | Host buy-and-hold run, fees, equity and strategy metrics |
| `scripting` | Minimal host evaluator for `close - close[N]`, applied as a custom study |
| `synchronized_charts` | UTC crosshair and viewport sharing across different intervals |

The backtest also runs without egui: `cargo run --example backtesting
--no-default-features`.
