# Installation

Use Rust 1.95 or newer. egui-charts uses egui 0.37; use the matching eframe
version so both dependencies share the same egui types.

## Choose the chart widget

For a chart without application toolbars, built-in studies or drawing tools:

```toml
[dependencies]
egui-charts = { git = "https://github.com/userFRM/egui-charts", default-features = false, features = ["widget"] }
eframe = "0.37"
chrono = "0.4"
```

The guide follows unreleased `main`. For a published release, replace `git` with
`version = "0.2"` and consult that release's API documentation.

## Enable what your application uses

| Feature | What it enables |
| --- | --- |
| `widget` | The egui chart, configuration and theme system |
| `studies` | Built-in studies and `IndicatorFactory`; also works headlessly |
| `drawings` | Drawing tools, interaction, undo and persistence; enables `widget` |
| `icons` | Embedded SVG icons; enables `widget` |
| `ui` | Toolbars, panels, dialogs, `ui_kit` and extension helpers; enables `icons`, `studies` and `drawings` |
| `locales` | Localized month and weekday names and AM/PM markers |
| `timezones` | IANA zones and daylight-saving rules |
| `serde` | Serialization for `Theme` |

The defaults are `ui`, `locales` and `timezones`. Omit `default-features = false`
and the explicit feature list for that setup. Add `studies` or `drawings` to the
widget-only list for those capabilities without the full application UI.

With `default-features = false` and no features, the headless engine provides
models, data contracts, scales, services and the indicator trait layer without egui.

## Native and browser hosts

The widget runs in an egui host. The [first chart](first-chart.md) uses eframe.
The repository's [demo](https://github.com/userFRM/egui-charts/tree/main/examples/demo)
shows an eframe WebAssembly app built with Trunk. Host windowing, event loops,
network runtimes and browser startup remain application concerns.
