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
high, low, close and volume, grouped from one of the bars' first executions. On volume and delta
intervals a fill split by a boundary opens the next bar with its rest, which the executions cannot
show, so a grouping there also needs the bar before to close at another price than every execution
at its start, or the page to end with the last bar before the live executions and rebuild it. A
page may start anywhere inside a bar; the bars it does not rebuild stay candles, without cells
from another bar.

The host's trade page must reach the last execution of the newest history bar. The chart cannot
tell a page that stops short of it from one that holds the whole bar, and the cells of that bar and
of the bars before it may then be shifted onto executions of their neighbours. When the host pushes
complete bars instead of executions, the oldest pushed bar that the history lacks, or that differs
from the history bar in its slot, takes the place of the first live execution; a pushed bar equal
to the history bar in its slot changes nothing. Page executions without ids at the time of the
first live execution cannot be told from the live ones, so the chart leaves them out; the bar
before then counts as one the page does not reach, and bars only its end would prove draw candles.
The chart keeps every instant where executions may be missing this way, or where a stream began
before its trade page reached it, across interval changes and resumes: no grouping takes such an
instant in, only the newest executions between two of them that prove bars draw cells, and the bar
holding one draws a candle. When the interval changes again before the trade page answers, that
page fills the time of both changes, and until it answers no grouping crosses the time of the
first. After three or more such changes the time of those in between stays without executions, and
no grouping crosses it.
History bars newer than everything the trade page reaches draw candles too, so a host should answer
both pages up to the same time.

The demo's synthetic instruments, TOKENA, TOKENB, STOCKA, STOCKB, STOCKC, STOCKD, INDEXA,
FOREXA, INDEXFZ2026, INDEXFH2027 and SAMPLE, answer
[`Datafeed::trades`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.Datafeed.html#method.trades)
and [`Datafeed::depth_history`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.Datafeed.html#method.depth_history)
for the time before their stream and declare both services. Their executions before the stream
are re-created from the minute bars `history` returns, so each minute's executions rebuild its
bar, and their depth columns hold a book quoted one tick around the price of those executions.
Scrolling a footprint chart or the liquidity heatmap back past the streamed session loads both,
and the feed answers as far back as its minute history reaches. The columns of INDEXFZ2026 before
its stream carry no largest orders, since its individual orders there are not known.

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
[`FootprintConfig`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.FootprintConfig.html) controls the presentation: cell colour and whether its shades and value-sized fonts compare within each bar, across the bars in view or to a set volume, number abbreviation and zero display,
value-sized fonts, extreme ask/bid ratios, the pullback range and the summary below each bar. Automatic grouping affects
display only. By default a bar is a ladder: sell volume left and buy volume right of a narrow candle body, each
side shaded in four strengths by where its volume falls in that side's range. The POC row is filled, a side that
dominates under the imbalance settings gets a strip along its outer edge, and two dotted lines mark the value area.
`text_color` defaults to white, so set the theme's text colour on a light chart.

### Prepared history

The built-in time-based footprint chart can request already aggregated history through
[`Datafeed::footprints`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.Datafeed.html#method.footprints).
Use [`FootprintPage::new`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.FootprintPage.html#method.new)
for owned bars, or [`FootprintPage::from_shared`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.FootprintPage.html#method.from_shared)
for an immutable [`FootprintSource`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/trait.FootprintSource.html).
Its bar views borrow sorted native rows; storage and byte conversion stay in the host. The
constructor docs include examples of both forms.
[`FootprintPage::bars`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.FootprintPage.html#structfield.bars)
is a [`FootprintBars`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/enum.FootprintBars.html);
its `iter` method reads either form.

A bounded request names `[since, before)`. Each reply answers one contiguous suffix ending
at `before`: its first bar starts the answered span, or an empty page uses
`with_continue_before(from)`. An available empty span certifies that no executions occurred.
Earlier unanswered history remains requestable; `count` still limits the number of bars.
Use [`FootprintPage::unavailable(from, before)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.FootprintPage.html#method.unavailable)
when the recording cannot supply that span. This is not empty history or a transport error.
The chart skips automatic retries of that span until refresh/reconnect and continues loading
other required history. `is_available()` is true only for complete/confirmed-empty replies.

The chart requests missing spans needed by its viewport or footprint studies, including
internal gaps. Internal fills use clock-aligned boundaries and leave the initial exclusive
prepared/live cutoff unchanged. Existing row owners remain retained; closing even an empty
gap updates coverage and invalidates affected study results. Older end-of-history does not
prevent an internal gap from being filled after resuming a paused feed. Resume retains
prepared owners; an explicit
[`Chart::reload_feed()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.reload_feed)
instead discards the old bars, trades and
prepared history and requests the latest pages again. Invalid replies preserve accepted
data and stop footprint requests until refresh; they never certify empty or unavailable history.

Use [`FootprintPage::partial`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.FootprintPage.html#method.partial)
for recorded rows whose executions are explicitly incomplete. It accepts `FootprintBars`
and a nonempty, strictly increasing list of unique, clock-aligned slot opens, each naming a
supplied bar. Even the initial `complete_before` must be aligned and close every marked
slot. On daily or longer session charts, the trading-date bar holding the cutoff cannot
be marked partial, whether the cutoff is inside its session or after its close. For an
08:30–15:00 session, Wednesday 00:00 still belongs to Tuesday's bar; marking Tuesday
is invalid even though its session has closed. If the bar holding the cutoff has
pre-cutoff executions, supply it unmarked to seed the live bar, as on a complete page.
Leaving that bar out certifies it had no executions before the cutoff. Admission
revalidates public payload fields; malformed declarations leave held rows and coverage
unchanged. `partial_slots()` borrows this metadata without allocating:
`Some(slots)` distinguishes a partial declaration from an unavailable page, although both
return false from `is_available()`. `Some([])` is invalid, not a complete page.

Unmarked slots retain complete/known-empty coverage. Unknown-side volume alone does not
imply partial history. Marked rows draw with dashed edges and a translated **Partial data**
label; hovering the rows or label explains their limits. Complete-only statistics, POC/value
area and other annotations, profiles and study readers exclude these slots and report a
crossing span incomplete. Replay refuses them even if raw history later arrives. No partial
prefix or missing executions are inferred.

These slots are terminal for automatic loading, including after pause/resume; other gaps and
older history can still load. Later pages or trades cannot replace or certify marked rows.
Use an explicit reload or feed replacement for corrections. A new interval needs its own
correctly marked reply. Frozen snapshots retain both the original coverage metadata and
shared row owners; exported images preserve the recorded values and partial marker.

A shared owner can serve several charts. Admission validates its clock, grid, rows, totals and
exclusive execution cutoff when each page is accepted. Of the shared page's bars, only an
unfinished boundary is copied into mutable rows; subsequent live trades use the existing accumulator.
The owner must remain unchanged, and reading it must not allocate or perform I/O.
Complete-only statistics and annotations use the same complete bars, including valid history
before a missing join. Only the cell painter additionally displays explicitly partial rows.
[`Footprint::bars()`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Footprint.html#method.bars)
keeps its existing owned
store API. Fixed range and anchored volume profile drawings also read these shared rows through
[`ChartView::executions`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/drawings/rendering/struct.ChartView.html#structfield.executions)
and group them into their rows as the volume profile studies below group executions.
An anchored VWAP drawing set to use executions reads the same rows: each bar held in full
adds every row at its price, and other bars add their volume at the drawing's source price.
A selected span across missing history stays labelled partial. Hosts constructing a `ChartView`
from an owned footprint pass `executions: Some((&footprint).into())`. Replay and independent
trade-driven studies still require individual executions.

## Liquidity

Call [`Chart::set_liquidity_heatmap(true)`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_liquidity_heatmap) on a chart with a feed and an instrument tick size.
It starts the default [`DepthHistory`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistory.html) if the host has not configured one. Call
[`Chart::set_depth_history`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_depth_history) first to choose cadence and grouping.
[`DepthHistoryConfig::with_retention`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#method.with_retention)
chooses the fewest columns that span a retention time. Columns outside it leave on samples and
pages, and the chart requests no depth page before its start. The cutoff follows the newest
column's end, so an idle feed keeps its history.
[`Chart::set_depth_retention`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_depth_retention)
applies the choice to the chart's history, as the Order flow settings tab does. `None` restores
600 columns per tier. The config's `span` reports its coverage and `max_memory` estimates its
maximum column memory.
[`Chart::clear_depth_history`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.clear_depth_history)
empties held columns and heatmap textures, as the tab's Release memory button requests; live
samples and historical pages within the retention fill it again. Hosts pass the chart's sampling
with [`SettingsDialog::set_depth_history_config`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/dialogs/chart_settings/struct.SettingsDialog.html#method.set_depth_history_config) when opening the dialog, and handle its
[`SettingsAction::ClearDepthHistory`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/ui/top_toolbar/components/settings_menu/actions/enum.SettingsAction.html#variant.ClearDepthHistory) by calling `Chart::clear_depth_history`. The dialog stays
open after that action; edited settings still apply only with Ok. The book is sampled
where depth arrives, independently of UI frames. No column covers a period that ends while the
aggregate or order-level book waits for a snapshot, so merging never averages an outage into the
sizes beside it. A merged historical column averages size over its span and uses its best
retained row for the inside-market path; it cannot restore individual quotes from before merging.

The renderer stores one texel per time column and grouped price level in texture atlases. New
or corrected columns upload only their column. One colour scale encodes size, the same for bids
and asks, at `ln(1 + size) / ln(1 + cut_off)`; its contrast against the chart background grows
with size and sizes near zero fade away. The cut-off is by default the size that 96% of the held
rows do not exceed, shared by every column; the held columns redraw only when it moves by more
than 10%. Bid and ask size at a crossed price are summed. Best bid and ask are 2 point lines in
the chart's bullish and bearish colours through the middle of each side's best row. With one tick
per row each line is on the best price itself. Each tile of a column draws as one quad; on a
logarithmic scale a tile is halved into a few quads where the scale bends by more than half a point
across it, which keeps every row within a point of its price at about the cost of a linear scale.
Expired history releases texture slots. No tile side exceeds the backend limit. The current book
appears as a size profile at the right of the pane; a key and the row's exact price, side and size
appear while the pointer is over it.

[`set_volume_bubbles(Some(Default::default()))`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/widget/struct.Chart.html#method.set_volume_bubbles) overlays executed volume using the same feed's
trades. Disabling the heatmap releases its textures and retains configured depth history.
Hosts composing their own panes can retain [`chart::overlays::LiquidityHeatmap`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.LiquidityHeatmap.html) and call [`render`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.LiquidityHeatmap.html#method.render)
with their price mapping, time-to-x conversion and [`OrderBookView`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/chart/overlays/struct.OrderBookView.html), whose size tiers mark large
orders.

### Individual orders

A feed that sends order events through [`Sink::push_order_snapshot`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Sink.html#method.push_order_snapshot) and [`push_order_event`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Sink.html#method.push_order_event) lets the
book, the heatmap and the ladder count individual resting orders. [`RestingOrder::with_participant`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.RestingOrder.html#method.with_participant)
and [`with_placed`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.RestingOrder.html#method.with_placed) carry the participant id and placement time the venue supplies; neither is ever
inferred from changes between snapshots. The demo's INDEXFZ2026 futures contract is fed this way:
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

let mut config = DepthHistoryConfig::default();
config.cadence = TimeDelta::milliseconds(50);
config.rows = 1000;
config.columns = 240;
chart.set_depth_history(Some(DepthHistory::new(config).unwrap()));
chart.set_liquidity_heatmap(true);
```

The heatmap shows a new column for every 50 ms period in which the host sent an update, so the
visible cadence is that of the feed, up to 20 columns a second. `cargo bench --bench heatmap`
measures it: 360 depth deltas a second (six per frame at 60 frames a second), each changing 8 levels
near the touch, into a book of 1,000 levels a side, in a 1440 by 900 window showing the last three
one-minute bars, on an Intel Core i7-10700KF (16 threads, release build, one run per case of 60 s of
market time on 18 November 2026):

| Scenario | Sink per delta | Frame p50 / p95 / max | Texture upload | Textures | Heap retained |
|---|---:|---:|---:|---:|---:|
| 100 levels, 1 s columns (defaults) | 0.71 µs | 0.323 / 0.370 / 1.448 ms | 11 KB/s | 0.8 MB | 0.0 MB |
| 1,000 levels, 50 ms columns | 2.04 µs | 1.170 / 3.079 / 5.204 ms | 394 KB/s | 7.3 MB | 18.0 MB |
| the same, history filled to every resolution | 1.43 µs | 1.469 / 3.819 / 8.892 ms | 367 KB/s | 16.8 MB | 54.7 MB |
| 1,000 levels, 50 ms columns, logarithmic price scale | 1.37 µs | 0.868 / 2.145 / 3.570 ms | 394 KB/s | 7.3 MB | 19.0 MB |

Heap is the net increase over the empty chart baseline; zero means other releases offset the added
allocations.

Depth deltas sort their changes and merge only the touched suffix of each side, reusing buffers.
Small updates near the touch do not copy the whole book and allocate nothing once those buffers
reach their working size. Empty sides of a delta leave the corresponding book side untouched.
A change buffer with capacity beyond 10,000 entries is released after the delta.

The sink time is the aggregation of a delta into the live book and, once per period, the book
into a depth column. A frame includes layout, painting, applying the update and tessellation;
the chart draws about 1,000 columns of 2,000 rows in those frames. On the logarithmic scale it draws
the same quads, one per tile of 256 rows, since the scale bends by far less than half a point across
a tile at these prices. The upload of one frame is at most 74 KB once the history is full, and the
first frame that draws a loaded history uploads all of it (2.4 MB here). A filled history holds
1,687 columns covering 50 minutes, bounded by `columns * 8 * rows * 2 * 16` bytes, 61 MB for this
configuration. A history of individual orders also records each row's largest order: 24 bytes a row
instead of 16.

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
- The library changes `columns` and `retention` only for a retention chosen in the Order flow
  tab, as `with_retention` does. A host that cannot afford the budget on its
  machines raises [`cadence`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.cadence), lowers [`rows`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.rows) or [`columns`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.columns), or raises [`ticks_per_row`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.DepthHistoryConfig.html#structfield.ticks_per_row).

## Execution profiles

[`TradeProfileRanges::new`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradeProfileRanges.html#method.new) takes ordered, non-overlapping half-open ranges of bar opening times.
Supply exchange session boundaries, including any pre-market or post-market splits, aligned
with the footprint's bar clock. [`trade_profiles`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/fn.trade_profiles.html) sums the existing exact rows over each range.
Incomplete bars are left out; complete prepared bars before a missing join still contribute.
[`TradeProfile::complete`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradeProfile.html#structfield.complete) is false when the range starts outside known coverage or includes incomplete bars; the chart labels such a
profile "Partial executions". [`Footprint::complete_from`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.Footprint.html#method.complete_from) names the complete suffix after the last gap, not every usable prepared bar.
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
contributing bar of the footprint the profile came from, by the final area's share and tie rules. The Profile section of the Symbol settings can trace that area and the developing POC as steps that hold until the next bar, and extend each
completed session's POC, VAH and VAL to the first later bar whose traded range reaches their drawn price; all are off by default, and sessions labelled "Partial executions" do not extend. The same section can hide the POC, choose the value area edges its guides draw, and colour the POC, each edge and the up and down volume inside and outside the value area; a level's colour also paints its extension. It can also grow each session's histogram from the right edge of the session, give it a share of the session's width, write each row's volume beside its bar when the row is tall enough for the text, and hide or recolour the box behind each session. Cropped profiles report the same POC centre and value area edges as their lines. Custom hours divide the day into up to five sessions; shared hours form a separate profile or stay in both sessions, whose profiles then overlap. Overlapping profiles cannot be merged or split. A switch clips custom sessions to the instrument's regular hours, counting periods from the later open. Saved single windows restore as the only session.

## Stream studies

Among others, the factory supplies these studies, each in its own pane:

| Factory title | Definition and defaults |
| --- | --- |
| Quoted Spread | `(ask - bid) / instrument_tick_size`, last, arithmetic mean over accepted quotes and maximum per bar. A crossed quote has negative spread. |
| Book Size | Last synchronized snapshot's summed bid and ask size, plus bid minus ask. Nearest 10 levels per side by default. A positive Price distance uses that inclusive distance from each side's inside market instead of the level count. |
| Trade Rate | Execution count per bar, count per second over 60 seconds and count per second over 3 seconds. Count volume switches all three to executed size. Zero-size executions count as trades. |
| Levels Traded Through | Sum of absolute changes of best bid and best ask, in integer instrument ticks, plus their total. Per bar by default; Window seconds selects a rolling window instead. This measures inside-market movement, not an inference about how an order swept the book. |
| Book Balance | Last synchronized bid and ask balance, `(bid - ask) / (bid + ask)` in percent, for two disjoint bands on a -100 to 100 scale. The near band takes the nearest 5 levels per side and the far band the next 10 by default. A positive Near price distance selects both bands by distance instead: the near band reaches that distance from each side's inside market and the far band reaches Far price distance further. The near edge belongs only to the near band. Empty or lost bands have no value. The individual-order filters apply as for Book Size. |
| Volatility Regime | Highest minus lowest execution price in a rolling 60-second window, ranked against the window values of the previous 100 bars. The percentile is the share of available earlier values that are smaller, from 0 to 100; decimal rounding ties do not count as smaller. The legend reads Low below 20, High above 80, and Normal at or between the boundaries. Window, lookback (1 to 1,000 bars) and both boundaries are inputs. Without an earlier value there is no rank. |

Rolling windows include events in `(now - window, now]`. Rates divide by the full configured
window even during startup. Feed heartbeats expire rates after queued executions are merged
and expire rolling movement in bars with quotes. A bar without the relevant events remains
missing; heartbeats consume no event capacity. Newly attached rate studies use the last
accepted heartbeat when replaying retained executions. Quotes seed the movement path, and a
lost book breaks it so the next quote does not count an unobserved jump. A loss leaves book size missing until
a new synchronized snapshot arrives, including recovery within the same bar.

Book Balance needs depth, or individual-order depth when its filter is active. Volatility
Regime needs live executions but not their side. Both appear in the Order flow category only
when those services are declared. Their input values use the same settings and template path
as the other studies, including saved layouts. Changing the range window starts a fresh source;
changing its lookback or regime boundaries reuses the held ranges. Bars without executions
remain gaps and do not enter the rank. A heartbeat can expire every execution in an existing
bar's window, leaving a zero range. Equal boundaries are normal; when the lower boundary
exceeds the upper one, the low check takes priority. The regime is a legend-only output,
available to study alerts; a host formatter or an explicit precision override can format it
numerically. A lost or overflowing window remains missing until the missing executions expire.

On history-only tick-count, range, volume and delta intervals, Trade Rate uses the recorded executions
when the tape reaches its start without a gap and reproduces every candle's key and OHLCV.
Range intervals use the instrument's tick size; without it the study stays blank. The
last candle may be unfinished. Each rate stops at its candle's last execution, even when
the next candle starts at the same provider timestamp. When a fill spans candles, each
candle counts the fill once (or just its allocated lots in volume mode). Both rate windows
count the original execution or its full size only once. A partial tape or a mismatched
candle leaves a gap; candle volume alone never supplies an execution count.

[`Indicator::order_flow_inputs`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.order_flow_inputs) returns the study's [`OrderFlowInput`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html) sources. Charts connect quote and
book inputs before feed updates are coalesced and feed rates from the deduplicated trade store.
A historical page rebuilds rate inputs from retained trades in timestamp order; a bar that lost
an execution to the store's limit is left out, and a rolling rate gives no value while its window
reaches back to the loss. Custom hosts
can push chronological events and book loss through the same handle, then call [`calculate`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#tymethod.calculate) or
[`update`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.update). Cloned studies share their market-data source, which receives each event once.
Replacing aggregation inputs creates a new source; hosts replay their history, while a chart replays the executions it still retains
for a newly attached rate study, and for one restored by undo, which then shows the values it would have had if it had stayed. On a tick, volume, range or delta interval, once the store has dropped executions, it shows those values on the chart's bars whose executions are all held and leaves the bars that lost one empty; a rolling rate gives no value while its window reaches an execution it leaves out. Other studies keep their sources and accumulated bar totals
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

[`footprint_snapshot`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html#method.footprint_snapshot)
returns an owned reader whose `bars(range)` and `span_is_complete(start, end)` methods read
after the input lock is released. It shares completed source bars and accepted prepared pages,
and copies only mutable rows. `footprints_from` remains available when a caller needs owned
price trees. Prepared pages have the chart's retention, independently of the raw-source limits.
On time intervals, a footprint input also requests `Datafeed::footprints`, including on a candle
chart. Studies attached after history arrives share the same pages; raw-page replies and eviction
preserve them, and only trades at or after the page's exclusive cutoff continue its totals.

### Volume profile studies

Vol Profile VR, FR, Session, Periodic, Anchored and Auto Anchored each return a footprint
source from [`Indicator::order_flow_inputs`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.order_flow_inputs). Where it holds the executions of a profile's bars,
the profile sums them, in rows of whole ticks: the Height input rounded to whole ticks, at least
one; with Whole ticks per row, the traded ticks over Row count rounded down or up, whichever lays
rows on multiples of their height closer to Row count, the taller on a tie, as the session volume
profile chart type lays out Number of rows; otherwise the traded ticks over Row count, rounded
up, with rows counted from multiples of their height as the volume footprint groups them and
widened where that would exceed Row count. A
profile whose executions start after its first bar sums those held and is noted "Partial
executions". A profile without executions, one calculated on another interval, and one whose bars
do not each hold their own executions, as on price-based chart types, keep the estimate from
bars. [`Indicator::needs_order_flow`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.needs_order_flow) is `false` for these studies, so they keep their Interval
input. A host without a feed gives the source the tick size with [`OrderFlowInput::set_tick_size`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html#method.set_tick_size)
and pushes executions with [`OrderFlowInput::push_trade`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html#method.push_trade) and the bar clock it gives the study
through [`Indicator::set_bar_clock`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.set_bar_clock) (by default [`BarClock::new`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/model/struct.BarClock.html#method.new) of the study's interval), then
updates the study from the first bar that changed.

Profile studies use the shared reader for painting, extended levels and autoscale. Frozen chart
exports call
[`Indicator::clone_for_export`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/trait.Indicator.html#method.clone_for_export);
profile studies and built-in `OrderFlowStudy` indicators such as Trade Rate use
[`OrderFlowInput::frozen`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/data/struct.OrderFlowInput.html#method.frozen)
there, so even a first query after later live updates sees the saved state. Ordinary study clones still
share their live input. Other indicators keep their existing clone behavior unless they override
the export hook.

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

Big trades ([`BigTradeRule`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/services/struct.BigTradeRule.html)) come in three kinds, each with its own switch: aggressive orders, the
fills of one explicit aggressor order id in a row or a single fill without one, on by default;
fills without aggressor ids that share an exchange timestamp and side, combined as the time and
sales panel combines them, off by default; and passive orders, the fills against one resting
order id across aggressive orders, off by default. A marker's label gives the size and the
kind, as in `600 passive`. A passive order is marked at the resting order's price, coloured by
its side, and its zone ends when a trade passes beyond it on the far side from that order.

Stop runs ([`StopRunRule`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/services/struct.StopRunRule.html)) require by default five same-side fills, each no more than 20 ms after its
predecessor, travelling at least three ticks without reversing. When any aggressor ids are
known, three distinct ids are required. The rule sets each of these limits and a minimum total
size, none by default, and the Order flow panel edits them. A continuing burst updates its
marker at the last fill, with fill count, distance, size and duration in the tooltip.
Unknown-side fills are excluded.

Stop sweeps use closed bars of the latest subscribed time interval: directional delta at least
25% of volume, three diagonal imbalances at 3:1, a body at least 60% of range, and volume at least
1.5 times the mean of the previous ten traded bars. Unknown-side fills are excluded. The service
retains the latest 100,000 sided executions and rebuilds this category on interval changes;
the first retained bar is excluded because it may be partial. Trade-based intervals have no
stop sweeps. Another feed event at or after the next boundary closes a bar; no wall clock runs.
A bar opened before the bar of the first live event since tracking or reset is still marked
without an alert, even when that live event closes it; a bar receiving live executions can alert.

Footprint conditions read the same bars. [`FootprintRule`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/services/struct.FootprintRule.html) marks a closed bar with a buy or sell
imbalance, or a stack of the rule's number of adjacent ones, using the rule's ratio, comparison
and minimum volume on rows of one tick whatever grouping the chart displays;
[`FootprintRule::follow_chart`](https://userfrm.github.io/egui-charts/docs/api/egui_charts/services/struct.FootprintRule.html#method.follow_chart) copies these from the chart's settings, as the demo does for its
active chart. A buy or sell zone rejection fires on a closed bar that trades into an active
stacked imbalance zone of an earlier bar without closing beyond its far edge, below a buy
zone's lowest row or above a sell zone's highest row; the first bar closing beyond it ends the
zone without firing. Detector zones use one-tick rows, retaining the newest 1,000 zones. A closed
bar skipped because conditions are disabled or its retained footprint is incomplete or missing
ends all active detector zones. Disabling and enabling inside a bar keeps them if its close is
read. A buy volume, sell volume or delta level fires on the execution that takes the
forming bar's value past it, from zero at the bar's open: a sell volume level of 100 fires on the
101st unit sold. Each condition alerts at most once per bar and rebuilds with the stop sweeps on
interval changes. Bars that opened before the bar of the first live event since tracking, such
as loaded history, are marked without an alert.

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
