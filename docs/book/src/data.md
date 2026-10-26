# Historical and streaming data

Choose the simplest path that fits your source: supply bars directly, or attach a
`Datafeed` for asynchronous history and live subscriptions. The host owns credentials,
connections, retries and data acquisition.

## Bars already in memory

Create a `Chart` from `BarData`. Replace history with `set_data`, or send forming
bars through `update`. The [first chart](first-chart.md#replace-history-or-update-the-last-bar)
shows both. Keep timestamps ordered and give a forming bar the same opening time
on each update.

## Page history through a feed

Only `Datafeed::history` is required. This in-memory adapter returns the newest
requested bars strictly before the request boundary, in chronological order:

```rust
use egui_charts::data::{Datafeed, HistoryPage, HistoryRequest, Reply};
use egui_charts::model::Bar;

struct InMemory(Vec<Bar>);

impl Datafeed for InMemory {
    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let end = self.0.partition_point(|bar| {
            request.before.is_none_or(|before| bar.time < before)
        });
        let start = end.saturating_sub(request.count);
        reply.send(Ok(HistoryPage {
            bars: self.0[start..end].to_vec(),
            end_of_history: start == 0,
        }));
    }
}
```

Pass an `Arc` of your feed to `ChartBuilder::with_feed`. The chart calls feed
methods from `show` on the UI thread. Return promptly; a worker or browser task
can answer `Reply` later. Set `end_of_history` only when no earlier page remains.
The chart asks for older pages as the viewport reaches the start of loaded history.

## Stream through the subscription sink

Implement `subscribe` and retain its `Subscription::sink` in your producer. Push
forming bars with `push_bar`, or executions with `push_tick` to build bars using
the selected interval. The sink wakes the chart when data arrives. For execution
history and book data, continue with [order flow](order-flow.md).

Stop producing when `Sink::is_active` becomes false. Symbol and interval changes
end the old subscription. Implement `unsubscribe` when your backend needs explicit
cleanup. Never wait for a worker that may be waking the chart from inside
`unsubscribe`: dropping a chart may happen while an egui context lock is held.

## Surface failures in your application

Read `Chart::feed_state` for loading, failures and rejected updates. Feed methods
return failures through the reply rather than blocking a frame. Your host decides
how to retry or present a disconnected source.

The repository's [live feed example](https://github.com/userFRM/egui-charts/blob/main/examples/live_feed.rs)
implements paging and a cancellable execution producer. The [builder guide](builder-guide.md)
connects feed setup to events and the rest of the application.
