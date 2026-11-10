# Calendar and viewport studies

## Try retained market inputs

On the published guide, select **Run example** to inspect the execution and book inputs a
host study can read. **Retain last 6 bars only** moves the execution coverage boundary
without changing the supplied candles; **Lose latest book** makes the latest depth
unavailable. The source reads `OrderFlowInput::footprints_from` and `books_from` rather than
estimating aggressors or book sizes from OHLCV. Each covered minute has volume 12 and delta
+5; its unknown-side unit adds volume only.

<!-- example -->

This example covers market-event inputs. The calendar and lower-interval contracts below
remain independent; the [history and live studies guide](https://userfrm.github.io/egui-charts/docs/studies/live-data/)
has a runnable history-paging and EMA scene. See [order-flow coverage](order-flow.md#footprints-and-books-for-host-studies)
for the handles a custom study returns from `Indicator::order_flow_inputs`.

## Calendar and lower-interval data

[`IndicatorFactory`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/factory/struct.IndicatorFactory.html) includes Performance, Seasonality, Visible Average Price and Up/Down Volume.
Performance compares the latest close with the last close on or before each calendar target in
the instrument's trading calendar. On an interval shorter than a day it declares the chart
symbol's daily bars in [`data_requests`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.data_requests) and reads each period's starting close from them, so the
longer periods fill without scrolling back. Where the chart's own bars hold a later close on or
before the target, as when the daily bars stop earlier, that close counts. Daily and longer
charts, and hosts that supply no daily bars, use the chart's loaded bars. A connected feed pages
the daily bars back to the start of the date one year before the last chart bar's trading date,
the oldest period, and stops there or at the end of its history, whatever its page size and the
chart's first bar. This also applies on price-based charts such as Renko: a study's stated
start before the first chart bar is honored, while a later start loads to the first chart bar
because the source study has no view of the rendered elements. Missing history and zero
reference prices show N/A.

With Use Daily Bars enabled on an intraday chart, Pivot Points Standard and Projected Pivot
Points request the symbol's daily bars. A connected feed loads through the full period before
the oldest retained pivot period, including yearly and aligned multi-year periods. Its start
is UTC midnight, matching the dates used to group the daily bars, and loading stops there or
at the end of the feed's history. Hosts supplying snapshots include that earlier period.

The chart supplies Visible Average Price with its visible source range before autoscaling.
Direct callers use [`set_visible_range(start..end)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.set_visible_range) before [`calculate`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#tymethod.calculate). The end is exclusive;
only finite prices in that range contribute to the arithmetic mean. The resulting horizontal
price remains available to the legend and last-value label when the latest bar is outside
the viewport.

Up/Down Volume, Volume Delta and Cumulative Volume Delta need actual intrabars from the host.
Their [`data_requests`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.data_requests) declaration uses one second on second charts, one minute on minute and
hour charts, five minutes on daily charts and one hour on longer intervals. A custom interval
that cannot contain whole candles of that size uses one-second intrabars. The Lower interval
input chooses another interval; one that is not shorter than the chart's or does not divide it
reads the automatic one. Subsecond and tick-count charts have no request. 24-hour Volume
requests the automatic interval only where chart bars cannot hold exactly 24 hours, on intervals
longer than a day or that do not divide one, and sums chart bars until it is supplied. Its Units input sums the supplied volume (default) or, for
volume in base units, an estimate of the quote amount: each candle's volume times its own close,
labelled with [`Instrument::currency`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.Instrument.html#structfield.currency), which the host must set. Without it, or with a non-finite or
negative value in the window, the quote amount is missing. Supply
the same symbol and session as the chart. Read the request again after a symbol, interval or
input change:

```rust
use egui_charts::{Chart, model::{Bar, Instrument}, studies::StudyDataRequest};

fn supply_snapshots(
    chart: &mut Chart,
    instrument: Option<Instrument>,
    host_history_and_stream_snapshot: impl Fn(&StudyDataRequest) -> Vec<Bar>,
) {
    for request in chart.study_data_requests() {
        // The host obtains a snapshot, including the developing intrabar.
        let bars = host_history_and_stream_snapshot(&request);
        chart.set_study_data(request, bars, instrument.clone());
    }
}
```

A response replaces the prior snapshot. The chart sorts, deduplicates and validates it through
its shared study-data path; a connected [`Datafeed`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.Datafeed.html) loads and subscribes automatically.
A live stream failure is available through `Chart::study_data_error` while older study history
keeps paging. A new live bar, a stream reset or a renewed subscription clears that stream
error; an outstanding history error takes precedence and keeps its existing retry policy.
The fixed range and anchored volume profile drawings add the finer interval that
[`Instrument::profile_resolutions`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.Instrument.html#structfield.profile_resolutions) suggests for their span to these requests. A connected feed
loads it back to the first bar of the span. Profiles still estimate volume from those bars;
a chart bar without finer inputs keeps its own estimate.
Direct callers supply [`StudyData`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.StudyData.html) to [`Indicator::update_data`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.update_data) with sorted, unique bars;
invalid prices, negative volume or unordered timestamps leave accepted values unchanged.
Pass `from = 0` whenever a requested history changes. A nonzero `from` reuses those histories
and updates the changed chart bars; the chart supplies this invalidation automatically.
An unavailable or stale interval leaves gaps. Include the earlier intrabar when its close is
needed to classify a doji. Empty history clears the values, and 24-hour Volume returns to
summing chart bars; missing parent intervals remain unavailable, while supplied zero volume
produces zero. Accounts, feed selection, retrieval, subscriptions and retries belong to the
host. No split is inferred from a parent candle's direction or total volume.

[`Indicator::table`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.table) exposes labelled percentages independently of plots, including Performance's
six periods and Seasonality's years and summary rows. The shared chart renderer paints them
in the study pane and in image exports, with scrolling when the table exceeds the pane.
[`Indicator::projections`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.projections) exposes fractional price boxes for future whitespace, included in
selection and autoscaling; Seasonality ends its current projection at the next UTC month boundary,
using the same interpolation and final observed bar spacing as the time axis. Its heatmap
uses the latest available changes, while each price projection keeps the earlier-year statistics
that were available at that month's opening.
