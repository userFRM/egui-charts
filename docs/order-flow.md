# Order-flow charts and studies

The library consumes the host's historical and streaming executions, quotes and synchronized
book through the existing `Datafeed` and `Sink`. It supplies no market data service.

`Footprint::new(chart.bar_clock(), tick_size, ticks_per_row)` aggregates executions without
estimating their aggressor from candles. Replay chronological historical trades once and append
new executions with `push`; rebuild on corrections. Unknown aggressors contribute to total
volume and remain outside bid/ask delta. `Chart::show_footprint` draws the four numeric modes,
profiles, grouped rows, imbalances, POC and value area in place of the price series. Its
`FootprintConfig` controls the presentation: cell colour, number abbreviation and zero display,
value-sized fonts, extreme ask/bid ratios and the pullback range. Automatic grouping affects
display only.

## Liquidity

Call `Chart::set_liquidity_heatmap(true)` on a chart with a feed and an instrument tick size.
It starts the default `DepthHistory` if the host has not configured one. Call
`Chart::set_depth_history` first to choose cadence, grouping and retention. The book is sampled
where depth arrives, independently of UI frames. No column covers a period when the book was
lost. A merged historical column averages size over its span and uses its best retained row for
the inside-market path; it cannot restore individual quotes from before merging.

The renderer stores one texel per time column and grouped price level in texture atlases. New
or corrected columns upload only their column. Each column uses `ln(1 + size) / ln(1 + max_size)`
contrast; bid and ask size at a crossed price are summed and their colours blended. Linear
scales draw a quad per tile and column. Logarithmic scales map each row edge individually to
preserve price geometry. Expired history releases texture slots. No tile side exceeds the
backend limit. The current book appears as a size profile at the right of the pane.

`set_volume_bubbles(Some(Default::default()))` overlays executed volume using the same feed's
trades. Disabling the heatmap releases its textures and retains configured depth history.
Hosts composing their own panes can retain `chart::overlays::LiquidityHeatmap` and call `render`
with their price mapping and time-to-x conversion.

## Execution profiles

`TradeProfileRanges::new` takes ordered, non-overlapping half-open ranges of bar opening times.
Supply exchange session boundaries, including any pre-market or post-market splits, aligned
with the footprint's bar clock. `trade_profiles` sums the existing exact rows over each range.
`merge_next` combines neighbours; `split` divides a composite at an interior bar boundary.
`ui::trade_profile_menu` exposes these operations to a host's menu and returns whether it changed
the ranges. Recompute profiles when trades or ranges change.

Each `TradeProfile` holds bid, ask and unknown volume per level, the developing POC after every
contributing bar, and contiguous value area through its `volume`. POC ties choose the lower
price. `nodes` identifies strict local maxima and minima against adjacent price rows; missing
rows count as zero, and endpoints and ties are not nodes. The renderer selects bid/ask stacks,
total volume or signed delta through the first `FootprintConfig::modes` entry. It draws the
value area, POC, developing POC and volume nodes on the host's price mapping.

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

`Indicator::order_flow_input` returns the study's `OrderFlowInput`. Charts connect quote and
book inputs before feed updates are coalesced and feed rates from the deduplicated trade store.
A historical page rebuilds rate inputs from retained trades in timestamp order. Custom hosts
can push chronological events and book loss through the same handle, then call `calculate` or
`update`. Cloned studies share their market-data source, which receives each event once.
Replacing aggregation inputs creates a new source; hosts replay their history, while a chart replays the executions it still retains
for a newly attached rate study. Other studies keep their sources and accumulated bar totals
when the catalogue changes, including totals of executions that aged out of the raw store.
Quote and book history before a study was connected must come from the host. Bars alone give
no substitute values. Counts of orders require fills grouped by order by the host; `Tick` has
an execution id, not an order id.

Inputs retain 10,000 bar outputs and at most 100,000 events per rolling window. If a window loses
events at that limit, it gives no value until those events would have expired. This keeps memory
bounded without silently understating a rate. The chart's existing trade store also retains at
most 100,000 executions. Depth history has its own configurable retention and merge tiers.

Clearing a source or evicting its oldest retained bar invalidates the affected plotted history,
so earlier cached values cannot survive as stale data after a reset or retention change.
