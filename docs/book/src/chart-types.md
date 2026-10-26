# Chart types and series

`ChartType` selects the main series renderer. The available types include
candlesticks, OHLC bars, line and area charts, Heikin Ashi, Renko, Kagi,
Point & Figure, Line Break, range bars and Market Profile (TPO).

```rust
use egui_charts::{Chart, ChartType, model::BarData};

let mut chart = Chart::new(BarData::new());
chart.set_chart_type(ChartType::Line);
```

Price transforms use the supplied history and the chart type's parameters. Choose
parameters with `Chart::set_chart_type_params`; use the
[API reference](api-reference.md) for `ChartTypeParams` and each transform's definition.
Execution-based intervals and price-transform renderers are separate concepts:
selecting a renderer does not acquire trades or replace the feed's interval.

## Add a series

`Chart::add_series` adds another series; retain its returned `SeriesId` for updates
and removal. `SeriesOptions` controls its appearance and scale. Update each series
through its own id, and use `SeriesScale` to select shared or independent pricing.
The main series remains `SeriesId::MAIN`.

[Scales and navigation](scales.md) explains coordinate sharing. The repository's
[basic chart example](https://github.com/userFRM/egui-charts/blob/main/examples/basic_chart.rs)
shows data, chart events and an additional price line.
