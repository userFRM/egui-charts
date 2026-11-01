# Builder guide

Create charts once and call `Chart::show` every frame. The host supplies historical
and streaming data and owns network connections, files, accounts and script execution.
The library computes studies and strategy metrics and renders the supplied data.

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

For asynchronous history and streaming, implement `data::Datafeed` and pass it to
`ChartBuilder::with_feed`. `history` answers its `Reply<HistoryPage>` with the newest
requested bars strictly before `HistoryRequest::before`, oldest first. Set
`end_of_history` only when no older page remains. Keep I/O off the UI thread: a
request's reply can be sent later from the host's worker.

In `subscribe`, retain the `Subscription::sink` and push bars with `push_bar`, or
executions with `push_tick`. Ticks build bars using the chart's interval. Stop
producing when `Sink::is_active` becomes false. Symbol and interval changes end
the old subscription; implement `unsubscribe` when the backend needs explicit
cleanup. The sink wakes the chart when data arrives. `Chart::feed_state` exposes
loading, failures and rejected updates. See `examples/live_feed.rs` for a complete
paged feed with a cancellable producer. With the optional `http` feature,
`data::http::HttpDatafeed::new(address)` connects to a column-based HTTP server
without another `Datafeed` implementation. `examples/network_feed.rs` starts a
matching local server; run it with `cargo run --features http --example network_feed`.

Apply a `theme::Theme` to egui at startup with `theme::apply_to_egui`. Render with
`chart.show(ui)` inside a host panel. Drain `Chart::drain_events` after showing the
chart to handle clicks, crosshair movement and visible-range changes.

## HTTP endpoint contract

Enable `http` and attach `Arc::new(data::http::HttpDatafeed::new(address))` with
`ChartBuilder::with_feed`. The same adapter implements `services::SymbolSearch`:
pass that `Arc` to the host's symbol search dialog. The address is the endpoint
root, optionally with a path prefix. The host supplies and operates the server,
its market data and authorization. Browser servers must allow the app's origin
with CORS. The adapter uses GET with percent-encoded query values.

| Endpoint | Request | JSON response |
|---|---|---|
| `/config` | No parameters | `{"supported_resolutions":["1","15","1D"],"exchanges":[{"value":"X"}],"symbols_types":[{"value":"stock"}]}` |
| `/symbols` | `symbol`; optional `conversion` | One symbol object as below |
| `/search` | `query`, `type`, `exchange`, `offset`, `limit` | Array of symbol objects, best first, after the requested offset, at most the limit; empty filters mean all |
| `/history` | `symbol`, `resolution`, `countback`, `extended_hours`; optional `from`, `to`, `conversion` | Column history as below |

A symbol object requires `name` (`symbol` is also accepted). Optional `ticker`
is the identifier used for subsequent requests. Metadata fields are `description`,
`exchange`, `type` (lowercase model symbol type), `minmov`, `pricescale`,
`volume_precision`, `currency_code`, `timezone`, `session`, `supported_resolutions`
and `data_status` (`streaming`, `delayed_streaming` or `endofday`). Price tick size
is `minmov / pricescale`, with `minmov` defaulting to one. Time zones use the
library's `TimezoneId` parser. Sessions accept `24x7` or
`HHMM-HHMM[,HHMM-HHMM]:days` windows (one day suffix for all windows), where 1 is Sunday and 7 is Saturday; an overnight
window belongs to its closing day. Omitted days mean Monday through Friday. Unsupported
session syntax or unavailable time zones report invalid data rather than guessing.

Configuration lists may be omitted (unknown, unrestricted) or empty (none).
Exchange and type entries use their `value` field. Optional feed services are
reported as unavailable: this adapter supplies bars and symbol search. Resolution
strings are minutes (`1`, `15`, `240`), seconds (`1S`, `45S`), days (`1D`), weeks
(`1W`, `2W`) or calendar months (`1M`, `3M`). Subsecond and execution-based bars
are not supported by this seconds-based endpoint contract. `resolve_converted`
passes the chosen conversion to `/symbols` so metadata and bars share the same unit.

History timestamps and bounds are Unix seconds in UTC; bounds may include a
fraction. `from` is inclusive and `to` exclusive, and an omitted bound is
unbounded. Return the newest `countback` bars within those bounds. Keep interval,
extended-hours and conversion semantics on the server; return an HTTP error for
an unsupported request rather than serving different prices under those labels.
For example:

```json
{"s":"ok","t":[60,120],"o":[10,11],"h":[12,13],"l":[9,10],"c":[11,12],"v":[100,200]}
```

Each index is one bar; all supplied columns must have equal lengths. `v` may be
omitted when volume is unavailable (zero). Invalid lengths, prices or timestamps
report `FeedError::InvalidData`. The adapter orders bars, enforces bounds and keeps
at most the requested count. Optional `end_of_history: true` says no older bars
exist. An empty window may answer `{"s":"no_data","nextTime":0}` to continue
at that UTC instant. The adapter advances this bar timestamp by one second
(or half the gap to `to`, if smaller) to form an exclusive continuation bound
that includes the named bar and still moves backward. `nextTime` must be earlier
than the requested `to`.
Without continuation, `no_data` ends paging. `{"s":"error","errmsg":"..."}`
reports invalid data. HTTP 404 maps to symbol not found, 429 to rate limited
(with `Retry-After` seconds or HTTP date), and 408 or 5xx to a recoverable network
error. Other unsuccessful statuses report invalid data. Historical request retries
follow the chart's normal retry policy.

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

`studies::CustomIndicator` accepts a calculation closure. Return one value per
input bar, preserving unavailable warm-up values with `IndicatorValue::None`:

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

The chart recalculates the closure when bars change. Implement `studies::Indicator`
directly when incremental calculation is needed. `CustomIndicator::with_inputs`
declares validated editable settings. Additional symbol and interval histories use
`studies::StudyDataRequest`.

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

`load_drawings` rejects unreadable files and symbol mismatches without replacing
current drawings. Report skipped records to the user. The host stores the JSON in
its own file or service. For automatic per-symbol persistence, implement
`services::Storage` and attach it with `Chart::set_storage`. With the `ui` feature,
`Chart::snapshot` and `Chart::restore` additionally save and restore chart settings,
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
bar close. Build a `model::StrategyReport` with `StrategyTrade` and `StrategyExit`;
`StrategyReport::performance` computes the metrics and `trades_csv` exports fills.
For a long trade of two units from 100 to 110 with total fees of 2, report net
profit `(110 - 100) * 2 - 2 = 18`. Fees are already included in the exit's profit.
Execution rules, slippage, contract sizing and data retrieval belong to the host.

The library supplies no scripting runtime. Evaluate scripts in a host runtime and
return their per-bar results through `CustomIndicator`, with missing values for
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
