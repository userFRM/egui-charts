# Calendar and viewport studies

## Try retained market inputs

On the published guide, select **Run example** to inspect the execution and book inputs a
host study can read. **Retain last 6 bars only** moves the execution coverage boundary
without changing the supplied candles; **Lose latest book** makes the latest depth
unavailable. The source reads
[`OrderFlowInput::footprints_from`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html#method.footprints_from)
and `books_from` rather than
estimating aggressors or book sizes from OHLCV. Each covered minute has volume 12 and delta
+5; its unknown-side unit adds volume only.

<!-- example -->

This example covers market-event inputs. The calendar and lower-interval contracts below
remain independent; the [history and live studies guide](https://userfrm.github.io/egui-charts/docs/studies/live-data/)
has a runnable history-paging and EMA scene. See [order-flow coverage](order-flow.md#footprints-and-books-for-host-studies)
for the handles a custom study returns from
[`Indicator::order_flow_inputs`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.order_flow_inputs).

## Calendar and lower-interval data

[`IndicatorFactory`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/factory/struct.IndicatorFactory.html) includes Performance, Seasonality, Visible Average Price and Up/Down Volume.
Performance compares the latest close with the last close on or before each calendar target in
the instrument's trading calendar. On every interval but one day it declares the chart
symbol's daily bars in [`data_requests`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.data_requests) and reads each period's starting close from them, so the
longer periods fill without scrolling back. Below a day, where the chart's own bars hold a later
close on or before the target, as when the daily bars stop earlier, that close counts, and hosts
that supply no daily bars get the chart's loaded bars. Bars longer than a day are dated by their
first day, so there each period is measured from daily closes alone, from the date of the last
daily close inside the bar, and reads No data without them. Daily charts use their own bars. A
connected feed pages the daily bars back to the start of the oldest period's date, five years before the last chart
bar's trading date with the default periods, and stops there or at the end of its history,
whatever its page size and the chart's first bar. This also applies on price-based charts such as Renko: a study's stated
start before the first chart bar is honored, while a later start loads to the first chart bar
because the source study has no view of the rendered elements. Missing history and zero
reference prices show No data.

Each symbol in Performance's Symbol list adds a row measured from that symbol's own latest daily
close: the study requests the symbol's daily bars on every interval, from the oldest date any
row's periods reach, and a symbol without them shows No data. While Include chart symbol is on, a
listed chart symbol shares the chart symbol's first row. Its Timeframe list sets the period
columns, such as 5D, 1W, 3M, YTD or 2Y; its six plots keep the chart symbol's standard periods.
A row shows the description of the instrument the host resolves or supplies for its symbol, and
the ticker without one; hovering a change shows the date of the close it is measured from.

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
hour charts, five minutes on charts of whole days such as 1D and 2D, and one hour on weekly,
monthly and other longer intervals. A custom interval that cannot contain whole candles of that
size uses one-second intrabars. The Lower interval input, shown as Auto with the interval it
picks while unset, chooses another interval: one no longer than the chart's is read as chosen,
also when it does not divide the chart interval, and a longer or trade-based one is not read and
the study shows a table saying so. Subsecond and tick-count charts have no request. 24-hour
Volume uses the same length-based rule, keeping one hour on whole-day custom charts, only
where chart bars cannot hold exactly 24 hours: on intervals longer than a day or that do not
divide one. It sums chart bars until those candles are supplied. Its Units input sums the supplied volume (default) or, for
volume in base units, an estimate of the quote amount: each candle's volume times its own price,
close unless the Price source input says otherwise, labelled with [`Instrument::currency`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.Instrument.html#structfield.currency), which the host must set.
The Target currency input converts each candle's amount at the rate in effect at its open; the
chart then requests the dated rates [`StudyDataRequest::exchange_rate`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.StudyDataRequest.html#method.exchange_rate) names, which the host supplies
like any metric. Without the currency or a rate, or with a non-finite or
negative value in the window, the quote amount is missing. A study calculated on another
Interval requests that Interval's bars first and, once they are supplied, the candles and
exchange rates it reads with them, as on a chart of that interval, so read the request again
after supplying them. A study calculated on another Symbol requests that symbol's bars first
and, once they are supplied, the bars of other intervals it reads with them for that symbol.
Supply each requested symbol with its own session. A study set to another Symbol, with or
without a price source, also requests its additional chart-symbol histories, such as Technical
Ratings' table intervals, on the chosen symbol. A study such as Ratio, Spread or Correlation
Coefficient that names another symbol also requests that symbol's bars on the Interval, with
the Interval's bars. Read the request again after a symbol, interval or input change:

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
A live stream failure is available through
[`Chart::study_data_error`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.study_data_error)
while older study history
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
An unavailable or stale interval leaves gaps. A doji at the previous intrabar's close takes that
intrabar's side, so include the earlier intrabars back to the last one that closed away from
its open or from the close before it. Empty history clears the values, and 24-hour Volume
returns to summing chart bars; missing parent intervals remain unavailable, while supplied zero
volume produces zero. Accounts, feed selection, retrieval, subscriptions and retries belong to the
host. No split is inferred from a parent candle's direction or total volume.

[`Indicator::table`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.table) exposes labelled percentages independently of plots, including Performance's
periods for each symbol and Seasonality's years and summary rows. The shared chart renderer
paints them in the study pane and in image exports, at the position and size each table states,
with scrolling when the table exceeds the space it has.
[`Indicator::projections`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.projections) exposes fractional price boxes for future whitespace, included in
selection and autoscaling; Seasonality ends its current projection at the next UTC month boundary,
using the same interpolation and final observed bar spacing as the time axis. Its heatmap
uses the latest available changes, while each price projection keeps the earlier-year statistics
that were available at that month's opening.
