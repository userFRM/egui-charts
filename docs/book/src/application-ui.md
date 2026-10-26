# Application UI and trading

Enable `ui` to compose application toolbars, panels, dialogs and reusable `ui_kit`
controls around a chart. These are components for your layout. Keep them alive
between frames and handle the actions returned by their `show` methods.

## Compose the workspace

The `ui` module contains top and drawing toolbars, timeframe controls, side panels,
symbol and comparison dialogs, replay controls and trading components. The host
chooses their placement and applies their actions to the chart. Use
[host data services](host-services.md) for symbol catalogs and external analytics,
and [strategy reports](strategy-report.md) for host-produced backtest results.

The [README terminal example](https://github.com/userFRM/egui-charts#a-terminal-around-it)
shows a toolbar beside a feed-backed chart. The repository's
[demo source](https://github.com/userFRM/egui-charts/tree/main/examples/demo)
shows a larger composition.

## Connect trading services

The `services` module defines the host contracts for accounts and broker operations.
The UI offers broker selection, account views, order tickets, a depth ladder and
trading lines. A host broker supplies account state and receives confirmed orders.
Authentication, connections, execution and account persistence belong to your
application. Rendering trading controls does not create a brokerage connection.

Consult the [API reference](api-reference.md) for the service traits and each
component's action enum before wiring it into your application.
