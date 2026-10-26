# Your first chart

Create a chart once, give it bars ordered from oldest to newest, and show it on
every frame. This complete native application uses the dependencies from
[installation](installation.md).

```rust,no_run
use chrono::{TimeDelta, Utc};
use eframe::egui;
use egui_charts::{Chart, model::{Bar, BarData}};

fn main() -> eframe::Result {
    let start = Utc::now() - TimeDelta::minutes(2);
    let bars = vec![
        Bar::new(start, 100.0, 103.0, 99.0, 102.0, 800.0),
        Bar::new(start + TimeDelta::minutes(1),
                 102.0, 104.0, 101.0, 103.0, 600.0),
    ];
    let mut chart = Chart::new(BarData::from_bars(bars));
    chart.set_symbol("SAMPLE");

    eframe::run_ui_native("My chart", Default::default(), move |ui, _| {
        egui::CentralPanel::default().show(ui, |ui| chart.show(ui));
    })
}
```

This example opens a native window, so CI compiles it without launching it.
All other examples that can run headlessly are executed as documentation tests.

## Keep state between frames

The closure owns the chart. Recreating it inside the frame callback would discard
its view and interaction state. `Chart::show` handles panning, zooming and the
crosshair; the host chooses its panel and available space.

## Replace history or update the last bar

Use `SeriesId::MAIN` to address the main series. `set_data` replaces its history;
`update` replaces a bar with the same timestamp or appends a later one.

```rust
use chrono::DateTime;
use egui_charts::{Chart, model::{Bar, BarData}, widget::SeriesId};

let mut chart = Chart::new(BarData::new());
let time = DateTime::UNIX_EPOCH;
chart.set_data(SeriesId::MAIN, vec![
    Bar::new(time, 100.0, 102.0, 99.0, 101.0, 10.0),
]);
chart.update(SeriesId::MAIN,
    Bar::new(time, 100.0, 103.0, 99.0, 102.0, 15.0));
assert_eq!(chart.bars(SeriesId::MAIN).len(), 1);
assert_eq!(chart.bars(SeriesId::MAIN)[0].close, 102.0);
```

For a live application, keep retrieval off the UI thread and use the
[historical and streaming data contract](data.md). The
[builder guide](builder-guide.md) covers events, persistence and synchronization.
