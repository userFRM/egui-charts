# The data model

The core types live in `egui_charts::model`. Keep market data in UTC and let the
chart's formatters and instrument calendar control its presentation.

## Bars and series

`Bar` holds a UTC timestamp and `f64` open, high, low, close and volume values.
Its timestamp denotes the opening time. `BarData` owns a vector of bars; supply
ordered history with unique opening times. `BarData::from_bars` is a container
constructor, not an ingestion validator.

```rust
use chrono::{DateTime, TimeDelta};
use egui_charts::model::{Bar, BarData};

let time = DateTime::UNIX_EPOCH;
let first = Bar::new(time, 100.0, 104.0, 98.0, 103.0, 20.0);
let next = Bar::new(time + TimeDelta::minutes(1),
                    103.0, 105.0, 102.0, 104.0, 12.0);
let data = BarData::from_bars(vec![first, next]);
assert_eq!(data.len(), 2);
```

Use `Bar::from_value` for a single value and `Bar::whitespace` for a time-axis
slot with no value. A bar may carry per-bar colours and an optional host JSON
payload; the chart returns the payload through bar lookups and pointer/data events.
Do not replace missing observations with invented OHLC values.

## Instruments and intervals

`Instrument` describes the feed's symbol, tick size, price precision and trading
calendar. Resolve it consistently with the identifiers accepted by your feed.
`Timeframe` describes time intervals and execution-based intervals. Tick-count,
volume, range and delta bars need executions; OHLC history cannot reconstruct them.
Range aggregation also needs the instrument tick size.

The bar clock applies the instrument's session rules. A chart's display time zone
changes labels; it is not a reason to rewrite UTC timestamps. Named zones require
`timezones`; UTC, system local time and explicit fixed offsets remain available
without it.

## Chart state belongs to the widget

`Chart` owns the view, configured series, studies and interactions. `SeriesId::MAIN`
identifies its main series. Keep local bar indices inside their chart: synchronized
charts exchange UTC positions because intervals and history lengths can differ.
See [scales and navigation](scales.md) and the [builder guide](builder-guide.md).
