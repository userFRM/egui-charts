# Calendar and viewport studies

`IndicatorFactory` includes Performance, Seasonality, Visible Average Price and Up/Down Volume.
Performance uses the chart's own loaded bars. It compares the latest close with the last close
on or before each calendar target in the instrument's trading calendar. Missing history and
zero reference prices show N/A. An intraday chart needs enough loaded history to cover the
requested period; the study does not obtain separate daily history.

The chart supplies Visible Average Price with its visible source range before autoscaling.
Direct callers use `set_visible_range(start..end)` before `calculate`. The end is exclusive;
only finite prices in that range contribute to the arithmetic mean. The resulting horizontal
price remains available to the legend and last-value label when the latest bar is outside
the viewport.

Up/Down Volume needs actual intrabars from the host. Its `lower_timeframe` request uses one
second on second charts, one minute on minute and hour charts, five minutes on daily charts
and one hour on longer intervals. A custom interval that cannot contain whole candles of
that size uses one-second intrabars. Subsecond and tick-count charts have no request. Supply
the same symbol and session as the chart. Read the request again after a symbol or interval change:

```rust,ignore
let requests: Vec<_> = chart.indicators()
    .filter_map(|(id, study)| study.lower_timeframe().map(|interval| (id, interval)))
    .collect();
for (id, interval) in requests {
    // The host obtains a sorted, duplicate-free snapshot, including the developing intrabar.
    let bars = host_history_and_stream_snapshot(interval);
    chart.indicator_mut(id).unwrap().set_lower_timeframe_bars(interval, bars)?;
}
```

A response replaces the prior snapshot atomically. Include the earlier intrabar when its
close is needed to classify a doji. Empty history clears the values; missing parent intervals
remain unavailable, while supplied zero volume produces zero. A stale interval, duplicate or
unordered timestamps, non-finite open/close/volume or negative volume returns an error and
retains the accepted snapshot. Accounts, feed selection, retrieval, subscriptions and retries
belong to the host. No split is inferred from a parent candle's direction or total volume.

`Indicator::table` exposes labelled percentages independently of plots, including Performance's
six periods and Seasonality's years and summary rows. The shared chart renderer paints them
in the study pane and in image exports, with scrolling when the table exceeds the pane.
`Indicator::projections` exposes fractional price boxes for future whitespace, included in
selection and autoscaling; Seasonality ends its current projection at the next UTC month boundary,
using the same interpolation and final observed bar spacing as the time axis. Its heatmap
uses the latest available changes, while each price projection keeps the earlier-year statistics
that were available at that month's opening.
