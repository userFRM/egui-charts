# Order-flow charts and studies

The library consumes the host's historical and streaming executions, quotes and synchronized
book through the existing [`Datafeed`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.Datafeed.html) and [`Sink`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Sink.html). It supplies no market data service.

## Try execution coverage and depth loss

On the published guide, select **Run example** to open an isolated chart with synthetic data.
Each minute contains 8 bought units, 3 sold units and 1 with no known aggressor: volume is
12 and delta is +5. **Retain last 6 bars only** rebuilds the execution source with a shorter
history. Its coverage begins at 00:06 UTC, and earlier candles remain visible without invented
footprint cells. At narrow widths, zoom in until the cells fit; the chart uses candles while
they would be too small to read. **Lose latest book** removes that bar's depth, leaving an
unavailable readout until a synchronized snapshot is supplied again. Clear it to restore
the sample snapshot. Depth and executions have separate coverage.

On a tick, volume, range or delta interval, the chart matches the executions of its trade pages
to the bars of its history: a bar shows cells only when the held executions rebuild its open,
high, low, close and volume, grouped from one of the bars' first executions. A page may start
anywhere inside a bar; the bars it does not rebuild stay candles, without cells from another bar.

<!-- example -->

The source beside the running example supplies candles independently, pushes chronological
fills through `Footprint` and `OrderFlowInput`, and reads `complete_from` and the last book.
It uses the default footprint presentation with the theme's text colour. This is a finite integration sample; a production
host owns retrieval, stream sequencing, retention and recovery. See
[recording and replay](order-flow-replay.md#try-a-recording) for the same live sink driven by recorded events.

[`Footprint::new(chart.bar_clock(), tick_size, ticks_per_row)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Footprint.html#method.new) aggregates executions without
estimating their aggressor from candles. Replay chronological historical trades once and append
new executions with [`push`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Footprint.html#method.push); rebuild on corrections. Unknown aggressors contribute to total
volume and remain outside bid/ask delta. [`Chart::show_footprint`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.show_footprint) draws volume, delta, trade count and
average size columns, profiles, grouped rows, imbalances, POC and value area in place of the price series. Its
[`FootprintConfig`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.FootprintConfig.html) controls the presentation: cell colour, number abbreviation and zero display,
value-sized fonts, extreme ask/bid ratios, the pullback range and the summary below each bar. Automatic grouping affects
display only. By default a bar is a ladder: sell volume left and buy volume right of a narrow candle body, each
side shaded in four strengths by where its volume falls in that side's range. The POC row is filled, a side that
dominates under the imbalance settings gets a strip along its outer edge, and two dotted lines mark the value area.
`text_color` defaults to white, so set the theme's text colour on a light chart.

## Liquidity

Call [`Chart::set_liquidity_heatmap(true)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_liquidity_heatmap) on a chart with a feed and an instrument tick size.
It starts the default [`DepthHistory`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistory.html) if the host has not configured one. Call
[`Chart::set_depth_history`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_depth_history) first to choose cadence, grouping and retention. The book is sampled
where depth arrives, independently of UI frames. No column covers a period when the book was
lost. A merged historical column averages size over its span and uses its best retained row for
the inside-market path; it cannot restore individual quotes from before merging.

The renderer stores one texel per time column and grouped price level in texture atlases. New
or corrected columns upload only their column. One colour scale encodes size, the same for bids
and asks, at `ln(1 + size) / ln(1 + cut_off)`; its contrast against the chart background grows
with size and sizes near zero fade away. The cut-off is by default the size that 96% of the held
rows do not exceed, shared by every column; the held columns redraw only when it moves by more
than 10%. Bid and ask size at a crossed price are summed. Best bid and ask are 2 point lines in
the chart's bullish and bearish colours. Linear scales draw a quad per tile and column.
Logarithmic scales map each row edge individually to preserve price geometry. Expired history
releases texture slots. No tile side exceeds the backend limit. The current book appears as a size
profile at the right of the pane; a key and the row's exact price, side and size appear while the
pointer is over it.

[`set_volume_bubbles(Some(Default::default()))`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_volume_bubbles) overlays executed volume using the same feed's
trades. Disabling the heatmap releases its textures and retains configured depth history.
Hosts composing their own panes can retain [`chart::overlays::LiquidityHeatmap`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.LiquidityHeatmap.html) and call [`render`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.LiquidityHeatmap.html#method.render)
with their price mapping, time-to-x conversion and [`OrderBookView`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.OrderBookView.html), whose size tiers mark large
orders.

### Individual orders

A feed that sends order events through [`Sink::push_order_snapshot`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Sink.html#method.push_order_snapshot) and [`push_order_event`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Sink.html#method.push_order_event) lets the
book, the heatmap and the ladder count individual resting orders. [`RestingOrder::with_participant`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.RestingOrder.html#method.with_participant)
and [`with_placed`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.RestingOrder.html#method.with_placed) carry the participant id and placement time the venue supplies; neither is ever
inferred from changes between snapshots. The demo's ESZ2026 futures contract is fed this way:
`examples/demo/orders.rs` turns each change of its synthetic book into order events with placement
times and rests a few large orders beside them, so the size tiers, the largest-order profile and
the Book Size study's age inputs can be tried there. The other demo instruments send aggregate
depth. One [`OrderFilter`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFilter.html) selects the orders everywhere:

- [`min_size`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFilter.html#structfield.min_size) leaves out smaller orders before they are summed into levels.
- [`min_age`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFilter.html#structfield.min_age) keeps orders that have rested that long at the time of the book, from their placement
  time. An order without one cannot be shown to have rested that long and is left out.
- [`max_age`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFilter.html#structfield.max_age) keeps orders that have rested less than that long, the most recently placed ones. A
  minimum and a maximum of the same age split the orders that carry a placement time, so fresh and
  persistent size add up; an order without one is in neither.
- [`exclude_participants`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFilter.html#structfield.exclude_participants) leaves out those participants. Orders without a participant stay.

Book size takes the filter through [`OrderFlowStudy::with_order_filter`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.OrderFlowStudy.html#method.with_order_filter) or the study's Minimum order
size, Minimum resting seconds and Maximum resting seconds inputs, the heatmap through
[`DepthHistoryConfig::orders`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.orders), and the ladder and the heatmap's profile through
[`Chart::set_order_book_view`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_order_book_view). [`OrderQueue::filtered`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderQueue.html#method.filtered), read through [`Chart::order_queue`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.order_queue), gives
hosts the same summed book, so all of them show the same sizes.

With aggregate depth only, or when a participant or age filter finds no order that carries the
field, there is nothing to filter. Book size has no value (NaN), the heatmap samples no column,
the ladder has no sizes, and the pane says Order-level depth unavailable, instead of showing
unfiltered sizes as filtered ones. [`OrderFilterUnavailable`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/enum.OrderFilterUnavailable.html) names the reason.

[`OrderBookView::profile`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.OrderBookView.html#structfield.profile) switches the heatmap's profile between the total resting size at a price
([`BookProfile::Total`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/enum.BookProfile.html#variant.Total)) and the largest known individual order there ([`LargestOrder`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/enum.BookProfile.html#variant.LargestOrder)).
[`OrderBookView::tiers`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.OrderBookView.html#structfield.tiers) sets two sizes, notable and large: a price whose largest known order
reaches one is drawn in that tier's colour, in the profile and in the ladder's size cell. A price
without order data is not highlighted. The colours are the active theme's
[`ChartColors::order_notable`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/theme/struct.ChartColors.html#structfield.order_notable) and [`order_large`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/theme/struct.ChartColors.html#structfield.order_large), so they follow a change of theme;
[`OrderBookView::tier_colors`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.OrderBookView.html#structfield.tier_colors) sets others.

The heatmap marks its cells with the same tiers and colours. A history of individual orders
([`DepthHistoryConfig::orders`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.orders), [`Some(OrderFilter::default())`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFilter.html#method.default) for every order) records the largest
accepted order of each row in [`DepthColumn::largest_bid_orders`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthColumn.html#structfield.largest_bid_orders) and [`largest_ask_orders`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthColumn.html#structfield.largest_ask_orders), and a
cell whose largest order reaches a tier takes that tier's colour over its shading. A merged column
keeps the larger of the two, so a large order stays marked at every zoom, over the whole span of
the merged column. A history of aggregate depth records no order and marks no cell.

### Large books at 50 ms

To show 1,000 levels a side with a column every 50 ms, configure the history before enabling the
heatmap:

```rust
use chrono::TimeDelta;
use egui_charts::data::{DepthHistory, DepthHistoryConfig};

let config = DepthHistoryConfig {
    cadence: TimeDelta::milliseconds(50),
    rows: 1000,
    columns: 240,
    ..Default::default()
};
chart.set_depth_history(Some(DepthHistory::new(config).unwrap()));
chart.set_liquidity_heatmap(true);
```

The heatmap shows a new column for every 50 ms period in which the host sent an update, so the
visible cadence is that of the feed, up to 20 columns a second. `cargo bench --bench heatmap`
measures it: 400 depth deltas a second, each changing 8 levels near the touch, into a book of
1,000 levels a side, in a 1440 by 900 window showing the last three one-minute bars, on an Intel
Core i7-10700KF (16 threads, release build, 60 s of market time):

| Scenario | Sink per delta | Frame p50 / p95 / max | Texture upload | Textures | Heap retained |
|---|---:|---:|---:|---:|---:|
| 100 levels, 1 s columns (defaults) | 0.4 µs | 0.3 / 0.4 / 1.6 ms | 11 KB/s | 0.8 MB | 0.6 MB |
| 1,000 levels, 50 ms columns | 1.8 µs | 1.0 / 2.2 / 3.5 ms | 394 KB/s | 7.3 MB | 20 MB |
| the same, history filled to every resolution | 2.2 µs | 2.1 / 3.9 / 5.8 ms | 367 KB/s | 16.8 MB | 56 MB |

Depth deltas sort their changes and merge only the touched suffix of each side, reusing buffers.
Small updates near the touch do not copy the whole book and allocate nothing once those buffers
reach their working size. Empty sides of a delta leave the corresponding book side untouched.
A change buffer with capacity beyond 10,000 entries is released after the delta.

The sink time is the aggregation of a delta into the live book and, once per period, the book
into a depth column. A frame includes layout, painting, applying the update and tessellation;
the chart draws about 1,000 columns of 2,000 rows in those frames. The upload of one frame is at
most 74 KB once the history is full, and the first frame that draws a loaded history uploads all
of it (2.4 MB here). A filled history holds 1,687 columns covering 50 minutes, bounded by
`columns * 8 * rows * 2 * 16` bytes, 61 MB for this configuration. A history of individual orders
also records each row's largest order: 24 bytes a row instead of 16.

The supported budget is this configuration on a machine of that class: a frame under 5 ms and
under 60 MB of heap for 1,000 levels at 50 ms. The library degrades by dropping resolution, never
by skipping updates silently:

- Updates faster than the cadence coalesce. A column holds the book as it stood at the end of
  its period, and nothing is queued.
- A column keeps only the [`rows`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.rows) levels nearest each side's inside market. The live book and the
  order book readers keep all of theirs.
- When a resolution holds more than [`columns`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.columns) columns, its two oldest merge into one of twice
  the span, so age lowers the time resolution (50 ms, 100 ms, up to 6.4 s) and never the price
  resolution. Memory stays bounded.
- A frame uploads only new or changed columns, in tiles no larger than the backend's texture
  limit (256 at most), and an unchanged history uploads nothing.
- The library never changes these settings by itself. A host that cannot afford the budget on its
  machines raises [`cadence`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.cadence), lowers [`rows`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.rows) or [`columns`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.columns), or raises [`ticks_per_row`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.ticks_per_row).

## Execution profiles

[`TradeProfileRanges::new`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradeProfileRanges.html#method.new) takes ordered, non-overlapping half-open ranges of bar opening times.
Supply exchange session boundaries, including any pre-market or post-market splits, aligned
with the footprint's bar clock. [`trade_profiles`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/fn.trade_profiles.html) sums the existing exact rows over each range.
Bars before [`Footprint::complete_from`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Footprint.html#method.complete_from), whose executions are not all held, are left out, and
[`TradeProfile::complete`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradeProfile.html#structfield.complete) is false for a range that starts before it; the chart labels such a
profile "Partial executions".
[`merge_next`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradeProfileRanges.html#method.merge_next) combines neighbours; [`split`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradeProfileRanges.html#method.split) divides a composite at an interior bar boundary.
[`ui::trade_profile_menu`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/fn.trade_profile_menu.html) exposes these operations to a host's menu and returns whether it changed
the ranges. Recompute profiles when trades or ranges change.

Each [`TradeProfile`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradeProfile.html) holds bid, ask and unknown volume per level, the developing POC after every
contributing bar, and contiguous value area through its [`volume`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradeProfile.html#structfield.volume). POC ties choose the lower
price. [`nodes`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradeProfile.html#method.nodes) identifies strict local maxima and minima against adjacent price rows; missing
rows count as zero, and endpoints and ties are not nodes. The renderer selects bid/ask stacks,
total volume or signed delta through the first [`FootprintConfig::modes`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.FootprintConfig.html#structfield.modes) entry. It draws the
value area, POC, developing POC and volume nodes on the host's price mapping.
[`developing_value_area`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradeProfile.html#method.developing_value_area) returns the inclusive value area rows after each
contributing bar of the footprint the profile came from, by the final area's share and tie rules. The Profile section of the Symbol settings can trace that area and extend each
completed session's POC, VAH and VAL to the first later bar that trades in their row; all are off by default, and sessions labelled "Partial executions" do not extend.

## Stream studies

The factory supplies four studies, each in its own pane:

| Factory title | Definition and defaults |
| --- | --- |
| Quoted Spread | `(ask - bid) / instrument_tick_size`, last, arithmetic mean over accepted quotes and maximum per bar. A crossed quote has negative spread. |
| Book Size | Last synchronized snapshot's summed bid and ask size, plus bid minus ask. Nearest 10 levels per side by default. A positive Price distance uses that inclusive distance from each side's inside market instead of the level count. |
| Trade Rate | Execution count per bar, count per second over 60 seconds and count per second over 3 seconds. Count volume switches all three to executed size. Zero-size executions count as trades. |
| Levels Traded Through | Sum of absolute changes of best bid and best ask, in integer instrument ticks, plus their total. Per bar by default; Window seconds selects a rolling window instead. This measures inside-market movement, not an inference about how an order swept the book. |

Rolling windows include events in `(now - window, now]`. Rates divide by the full configured
window even during startup. Feed heartbeats expire rates after queued executions are merged
and expire rolling movement in bars with quotes. A bar without the relevant events remains
missing; heartbeats consume no event capacity. Newly attached rate studies use the last
accepted heartbeat when replaying retained executions. Quotes seed the movement path, and a
lost book breaks it so the next quote does not count an unobserved jump. A loss leaves book size missing until
a new synchronized snapshot arrives, including recovery within the same bar.

[`Indicator::order_flow_inputs`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.order_flow_inputs) returns the study's [`OrderFlowInput`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html) sources. Charts connect quote and
book inputs before feed updates are coalesced and feed rates from the deduplicated trade store.
A historical page rebuilds rate inputs from retained trades in timestamp order; a bar that lost
an execution to the store's limit is left out, and a rolling rate gives no value while its window
reaches back to the loss. Custom hosts
can push chronological events and book loss through the same handle, then call [`calculate`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#tymethod.calculate) or
[`update`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.update). Cloned studies share their market-data source, which receives each event once.
Replacing aggregation inputs creates a new source; hosts replay their history, while a chart replays the executions it still retains
for a newly attached rate study, and for one restored by undo, which then shows the values it would have had if it had stayed. On a tick, volume, range or delta interval, once the store has dropped executions, it shows those values on the chart's bars whose executions are all held and leaves the bars that lost one empty. Other studies keep their sources and accumulated bar totals
when the catalogue changes, including totals of executions that aged out of the raw store.
Quote and book history before a study was connected must come from the host. Bars alone give
no substitute values. Counts of orders require fills grouped by order by the host; [`Tick`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/quote/struct.Tick.html) has
an execution id, not an order id.

Inputs retain up to 11,000 bar outputs; the bar after that drops the oldest at once, down to the
newest 10,000, the longest span of Rolling Volume and Delta. They also retain at most 100,000
events per rolling window. If a window loses events at that limit, it gives no value until those
events would have expired. This keeps memory bounded without silently understating a rate. The
chart's existing trade store also retains at most 100,000 executions. Depth history has its own
configurable retention and merge tiers.

Clearing a source or dropping its oldest bars invalidates the plotted history of its studies, so
earlier cached values cannot survive as stale data after a reset or retention change: the chart
recalculates them in full, and Size-Filtered CVD starts again from the oldest retained bar.
Drops come once per 1,001 new bars; between them a new bar updates the studies from that bar.

### Footprints and books for host studies

[`OrderFlowInput::footprint()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html#method.footprint) holds each bar's executions by price, one level per instrument
tick, as [`Footprint`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Footprint.html) aggregates them. [`OrderFlowInput::book(levels)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html#method.book) holds the nearest `levels` of
each side of each bar's last synchronized book. A study returns them from
[`Indicator::order_flow_inputs`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.order_flow_inputs) and reads them where it calculates, from the first bar that
[`update`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.update) names:

```rust
let flow = self.inputs[0].footprints_from(open);
let books = self.inputs[1].books_from(open);
let poc = flow.bars.get(&open).and_then(FootprintBar::poc); // level n lies at n * flow.tick_size
let best_bid = books.bars.get(&open).and_then(|book| book.bids.first());
```

The chart gives both its instrument's tick size (hosts without a feed call [`set_tick_size`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html#method.set_tick_size)),
builds the footprint from the executions it receives and from the trades it still holds when the
source is connected, and passes each synchronized book update before coalescing. The footprint
follows the chart's bars on every interval, and the book keeps only bars of an interval measured
in time. [`OrderFlowBars`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowBars.html) says what a copy covers: [`complete_from`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowBars.html#structfield.complete_from) is
the oldest bar held in full, since a bar cut by retention or data loss is left out rather than
shown short, and a later bar that is absent had no executions or no book; [`cutoff`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowBars.html#structfield.cutoff) is the replay
cutoff; [`revision`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowBars.html#structfield.revision) changes whenever the bars may have. A bar's book is the one after its last
synchronized update; a loss after it, or an update with an invalid level, leaves the bar without
one. Footprint and book sources keep at most 11,000 bars and 110,000 price rows, then drop their
oldest bars at once, down to 10,000 bars and 100,000 rows.

### Volume profile studies

Vol Profile VR, FR, Session, Periodic, Anchored and Auto Anchored each return a footprint
source from [`Indicator::order_flow_inputs`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.order_flow_inputs). Where it holds the executions of a profile's bars,
the profile sums them, in rows of whole ticks: the Height input rounded to whole ticks, at least
one, or the traded ticks over Row count, rounded up, with rows counted from multiples of their
height as the volume footprint groups them and widened where that would exceed Row count. A
profile whose executions start after its first bar sums those held and is noted "Partial
executions". A profile without executions, one calculated on another interval, and one whose bars
do not each hold their own executions, as on price-based chart types, keep the estimate from
bars. [`Indicator::needs_order_flow`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.needs_order_flow) is `false` for these studies, so they keep their Interval
input. A host without a feed gives the source the tick size with [`OrderFlowInput::set_tick_size`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html#method.set_tick_size)
and pushes executions with [`OrderFlowInput::push_trade`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html#method.push_trade) and the bar clock it gives the study
through [`Indicator::set_bar_clock`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.set_bar_clock) (by default [`BarClock::new`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.BarClock.html#method.new) of the study's interval), then
updates the study from the first bar that changed.

### Replay

While [`Chart::set_replay_time`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_replay_time) shows bars up to a cutoff, every input reads as of it: later bars
are left out, and the bar containing the cutoff, drawn from [`set_replay_source`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_replay_source), holds only what
came before it. Multi-Range Delta, Rolling Volume and Delta, Size-Filtered CVD and footprint
sources sum the held executions before the cutoff unless the trade store dropped one of that
bar's; a book source shows that bar's book only when the bar's last update came before the
cutoff; the other studies leave the bar empty. Leaving replay restores the live values. Studies
added during replay are connected when the feed resumes. A feed that replays history through its
[`Sink`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Sink.html) ([`Sink::reset_replay`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Sink.html#method.reset_replay)) clears the inputs at each seek and fills them again, so no cutoff
applies.

## Execution detectors

Enable `detectors` and connect [`services::OrderFlowDetectors`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/services/struct.OrderFlowDetectors.html) through [`Chart::set_signal_source`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_signal_source).
Track each instrument with its tick size, then push the same symbol-tagged [`MarketEvent`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/enum.MarketEvent.html) to the
chart and the service. Historical trades go through [`push_history`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/services/struct.OrderFlowDetectors.html#method.push_history); the host supplies all data.
Settings hold separate enable, alert and sound choices for each detector, all off until the host
sets them; [`DetectorSettings::validate`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/services/struct.DetectorSettings.html#method.validate) checks settings restored from storage before
[`set_settings`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/services/struct.OrderFlowDetectors.html#method.set_settings) applies them. Alert delivery is a host integration; these choices do not
themselves send notifications.

Stop runs require five same-side fills, each no more than 20 ms after its predecessor, travelling
at least three ticks without reversing. When any aggressor ids are known, three distinct ids are
required. A continuing burst updates its marker at the last fill, with fill count, distance,
size and duration in the tooltip. Unknown-side fills are excluded.

Stop sweeps use closed bars of the latest subscribed time interval: directional delta at least
25% of volume, three diagonal imbalances at 3:1, a body at least 60% of range, and volume at least
1.5 times the mean of the previous ten traded bars. Unknown-side fills are excluded. The service
retains the latest 100,000 sided executions and rebuilds this category on interval changes;
the first retained bar is excluded because it may be partial. Trade-based intervals have no
stop sweeps. Another feed event at or after the next boundary closes a bar; no wall clock runs.

Icebergs count exact order refills at the same price and side within 100 ms of depletion, or
an increase within 100 ms of execution without keeping priority. Unrelated adds and cancels
do not erase pending matches; concurrent depletions at a price each await a matching replacement
within that window. Two refills report by default; later refills update the same marker.
Aggregate depth estimates a refill count as whole opening displayed sizes executed minus one,
confirming only on the next synchronized
book update that still shows the level. Leaving that inside price breaks the run. Estimates say
so in their tooltips. Order snapshots and any depth loss discard unfinished refill sequences;
replayed order events and duplicate trade ids do not count again. Loaded trade history alone
cannot establish refills against a historical book.

Absorption opens at the first execution against an inside level and closes after a second
without executions there. Its volume must reach the configured fixed threshold and the opening
shown size. Automatic thresholds use the nearest-rank 99th percentile of the last 1,000 closed
episodes, after 100 have closed. A qualifying level still shows at least half its opening size,
or received added size in the order-level book. A trade beyond it breaks the episode. Its
side-coloured one-price zone runs from the first to the last execution. Quotes or book events
can close an idle episode on feed time. Book loss discards episodes without reporting them.

The liquidity tracker opens a one-price zone when a level within ten ticks of the inside
reaches its threshold. Automatic thresholds are four times the median non-zero size within ten
ticks on both sides; an even number uses the mean of the middle pair. A wall has traded at least
10% of its peak, retains half its peak and sees the inside move three ticks away. Below 20% of
peak, a level is pulled when withdrawals reach 80% of peak, or swept when executions cover half
the drop and a later trade prints beyond it. These outcomes end its zone. Aggregate withdrawals
are each size drop minus executions since the preceding update; individual orders use cancels
and reductions. Fresh liquidity is size added in one update within five ticks, using its own
threshold with the same automatic rule. Snapshots establish a baseline, not fresh arrivals.

Only individual orders can show spoofing: an order added within five ticks, at least the large
level threshold, is withdrawn below 10% within two seconds with less than 10% executed. Aggregate
depth reports pulled liquidity only. Order-level tooltips name the largest resting order at the
level. All depth rules discard open episodes on lost sequencing or a depth-source change; open
liquidity zones are removed without reporting an outcome. Order snapshots restart episodes
because buffered events cannot reconstruct their original displayed sizes. Completed signals
remain available to subscriptions, subject to the shared 5,000-signal retention limit.

[`SignalShape::PriceZone`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/enum.SignalShape.html#variant.PriceZone) draws a side-coloured line at an exact price, with finite prices and
ordered times checked by the chart. A zone with coincident endpoints stays visible and hoverable.
