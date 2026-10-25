# Host data services

The host owns instrument catalogs, analytics, analyst estimates and market heatmap values. These APIs render and route that data without acquiring it or computing analytics.

## Symbol search

Connect an `Arc<dyn services::SymbolSearch>` with `SymbolSearchDialog::set_search_service` and `CompareSymbolsDialog::set_search_service`. Both dialogs keep accepting a local symbol list when no service is attached. The service returns promptly and answers through `data::Reply<Vec<model::Instrument>>`; only the current text and filters can supply visible results.

Apply `SymbolQuery::offset` before `limit`, preserving ordering between pages. A page shorter than the limit ends scrolling requests. The dialogs ask for another page when the last result comes into view. Instrument identifiers must match the identifiers accepted by the bar feed. `Vec<Instrument>` implements the same search contract for a local catalog.

`SymbolSearchDialog::set_storage` connects recent selections through `StoreKind::RecentSymbols`. Call `show` every frame, including while closed, so asynchronous storage can finish. Recent instruments retain their names and metadata, with the newest first. `record_symbol` also records symbols opened elsewhere. Storage is read before a write; a failed read prevents replacement of the stored catalog. `set_exchange_filter` restricts service queries to a venue. An empty, unfiltered query shows recents; choosing a category or venue browses the host catalog.

## Signals

Connect `Chart::set_signal_source(Some(source))` to an `Arc<dyn services::Signals>`. Each subscription starts empty. Push current signals first, then `SignalEvent::Upsert`, `Remove` and `Clear`. Identities belong to the subscription; an upsert replaces its prior identity. Markers carry a time, optional price and direction. Levels carry a price and optional start time. Zones carry price bounds and time bounds.

The chart ends a subscription immediately on a symbol or interval change. Stop producing when `Events::is_active` becomes false. A burst over 10,000 pushes between frames ends the stream; the chart discards its state and subscribes again. Invalid prices or reversed zone bounds are discarded. No analytics run in the library.

The chart's Signals menu controls category visibility without deleting data. Hosts can also use `set_signal_category_visible`. Hover text comes from `Signal::tooltip`; clicks call `Signals::clicked`. Signal times are projected against the displayed history on every frame, including after history replacement and on price-based charts. A marker with a supplied price can appear in visible whitespace without a loaded bar.

## Analyst targets

Implement optional `Datafeed::price_targets` to answer `services::PriceTargets`. Its default returns `FeedError::NotSupported`, which shows neither targets nor an error. Prices must be finite, ordered `low <= average <= high`, and accompanied by an analyst count and publication and horizon dates. The horizon must follow publication.

The Symbol tab's **Analyst price targets** setting, or `Chart::set_price_targets_visible`, enables projections. They start at the last close with a value and end at midnight UTC on the horizon date. Leave enough future whitespace with `set_visible_logical_range` to see that date and its labels. Automatic price fitting includes the target extremes while the projections reach the visible time range. The host price formatter receives instrument prices, including on percentage and indexed scales. Percentage change is `(target - close) / close * 100`; a zero close omits the percentage. Labels stay below the legend and quote controls in narrow charts. Hovering an endpoint or visible label shows the count and dates, including when the horizon is offscreen.

A symbol change clears the targets and ignores its old replies. `reload_price_targets` requests updated estimates; `take_price_targets_error` reports errors other than unsupported data. Hiding targets retains their answer. Price-based charts do not draw projections.

The demo's quick search has **Toggle API samples** for host signals and **Show analyst price targets** for SAMPLE's daily projection. Its synthetic feed implements these contracts on native and browser builds.

## Market heatmap

`ui::widget_bar::MarketHeatmapPanel` owns a public list of `MarketHeatmapEntry` values. Supply each symbol, group, positive tile weight and percentage change, then call `show`. It returns the selected symbol for the host to open. Mutate the entries and request a repaint when host data changes; no new panel is needed.

Groups and their tiles share area in proportion to their weights. Invalid weights have no tile. Colours diverge from the theme background towards its rising/falling colours, saturating at five percent; hover shows the original value and percentage. Group captions overlay the tiles so they do not change their proportional layout. Add `RightBarIcon::MarketHeatmap` to the terminal rail when the host supplies this panel. The demo fills it with synthetic weights and updates its percentage changes from its quotes.
