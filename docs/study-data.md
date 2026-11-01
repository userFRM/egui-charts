# Calendar and viewport studies

`IndicatorFactory` includes Performance, Seasonality, Visible Average Price and Up/Down Volume.
Performance compares the latest close with the last close on or before each calendar target in
the instrument's trading calendar. On an interval shorter than a day it declares the chart
symbol's daily bars in `data_requests` and reads each period's starting close from them, so the
longer periods fill without scrolling back. Where the chart's own bars hold a later close on or
before the target, as when the daily bars stop earlier, that close counts. Daily and longer
charts, and hosts that supply no daily bars, use the chart's loaded bars. Missing history and
zero reference prices show N/A. A connected feed loads as many daily bars as the chart holds,
and at least 300, so on a symbol that trades every day a chart of fewer than 366 bars can leave
1Y at N/A.

The chart supplies Visible Average Price with its visible source range before autoscaling.
Direct callers use `set_visible_range(start..end)` before `calculate`. The end is exclusive;
only finite prices in that range contribute to the arithmetic mean. The resulting horizontal
price remains available to the legend and last-value label when the latest bar is outside
the viewport.

Up/Down Volume, Volume Delta and Cumulative Volume Delta need actual intrabars from the host.
Their `data_requests` declaration uses one second on second charts, one minute on minute and
hour charts, five minutes on daily charts and one hour on longer intervals. A custom interval
that cannot contain whole candles of that size uses one-second intrabars. The Lower interval
input chooses another interval; one that is not shorter than the chart's or does not divide it
reads the automatic one. Subsecond and tick-count charts have no request. 24-hour Volume
requests the automatic interval only where chart bars cannot hold exactly 24 hours, on intervals
longer than a day or that do not divide one, and sums chart bars until it is supplied. Its Units input sums the supplied volume (default) or, for
volume in base units, an estimate of the quote amount: each candle's volume times its own close,
labelled with `Instrument::currency`, which the host must set. Without it, or with a non-finite or
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
its shared study-data path; a connected `Datafeed` loads and subscribes automatically.
The fixed range and anchored volume profile drawings add the finer interval that
`Instrument::profile_resolutions` suggests for their span to these requests. A connected feed
loads it back to the first bar of the span. Profiles still estimate volume from those bars;
a chart bar without finer inputs keeps its own estimate.
Direct callers supply `StudyData` to `Indicator::update_data` with sorted, unique bars;
invalid prices, negative volume or unordered timestamps leave accepted values unchanged.
Pass `from = 0` whenever a requested history changes. A nonzero `from` reuses those histories
and updates the changed chart bars; the chart supplies this invalidation automatically.
An unavailable or stale interval leaves gaps. Include the earlier intrabar when its close is
needed to classify a doji. Empty history clears the values, and 24-hour Volume returns to
summing chart bars; missing parent intervals remain unavailable, while supplied zero volume
produces zero. Accounts, feed selection, retrieval, subscriptions and retries belong to the
host. No split is inferred from a parent candle's direction or total volume.

`Indicator::table` exposes labelled percentages independently of plots, including Performance's
six periods and Seasonality's years and summary rows. The shared chart renderer paints them
in the study pane and in image exports, with scrolling when the table exceeds the pane.
`Indicator::projections` exposes fractional price boxes for future whitespace, included in
selection and autoscaling; Seasonality ends its current projection at the next UTC month boundary,
using the same interpolation and final observed bar spacing as the time axis. Its heatmap
uses the latest available changes, while each price projection keeps the earlier-year statistics
that were available at that month's opening.
