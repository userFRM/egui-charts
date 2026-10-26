<p class="eyebrow">RUST / EGUI 0.37</p>

# Financial charts. Your data.

egui-charts is a financial charting engine for egui. Embed an interactive chart
in your application, then add studies, drawings and the surrounding UI as you need them.
The same Rust code runs natively and in WebAssembly.

<div class="start-links">

[Build your first chart →](first-chart.md) · [Explore the API](api-reference.md)

</div>

## A chart inside your application

Keep a chart alive between frames and call `Chart::show` in your egui layout.
It handles painting, navigation and chart interaction. You supply the bars, or
connect a feed that answers history requests and sends live updates.

Start with the widget alone. Enable built-in studies and drawing tools when you
need them, or use the application UI components to compose toolbars, dialogs and panels.
The headless engine also works without egui.

## Bring your own data

Your application owns market data connections, credentials, accounts and durable
storage. The library provides the data contracts, chart rendering, study calculations
and host actions. It does not supply a market data feed or run trading strategies.

| You want to… | Start here |
| --- | --- |
| Put a chart in an egui app | [Installation](installation.md) and [your first chart](first-chart.md) |
| Connect a database or live source | [The data model](data-model.md) and [data integration](data.md) |
| Extend the analysis | [Studies](studies.md), [drawings](drawings.md) and [order flow](order-flow.md) |
| Build a larger workspace | [Application UI](application-ui.md) and the [builder guide](builder-guide.md) |

## Read the right version

This guide follows the repository's `main` branch, which contains changes beyond
the published 0.2.0 release. Use the Git dependency described in
[installation](installation.md) to follow these examples. For released APIs, read
[the 0.2.0 documentation](https://docs.rs/egui-charts/0.2.0).
