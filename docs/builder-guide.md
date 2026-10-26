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
paged feed with a cancellable producer.

Apply a `theme::Theme` to egui at startup with `theme::apply_to_egui`. Render with
`chart.show(ui)` inside a host panel. Drain `Chart::drain_events` after showing the
chart to handle clicks, crosshair movement and visible-range changes.

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
`studies::StudyDataRequest`; see the study-data guide in the repository.

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
viewport only when the source changes to avoid feedback between differing widths.

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

Run `cargo run --example <name>` from the repository. All use the public API and
generated sample data, and CI compiles each through its `--all-targets` checks.

| Name | Workflow |
|---|---|
| `live_feed` | Paged historical bars and streaming executions |
| `custom_indicator` | Ten-bar price change with explicit warm-up values |
| `drawing_persistence` | Editable drawing JSON, save/load and error reporting |
| `backtesting` | Host buy-and-hold run, fees, equity and strategy metrics |
| `scripting` | Minimal host evaluator for `close - close[N]`, applied as a custom study |
| `synchronized_charts` | UTC crosshair and viewport sharing across different intervals |

The backtest also runs without egui: `cargo run --example backtesting
--no-default-features`. The demo's execution-based history remains separate from
the standalone examples' shared OHLCV generator.
