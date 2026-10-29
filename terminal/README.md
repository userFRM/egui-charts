# egui-charts-terminal

The assembled terminal of [egui-charts](..): toolbars, side panels, bottom dock, dialogs, menus and one or more charts on your `Datafeed`, performing the actions their controls emit. It is an application layer on top of the library and uses only its public API. The package is not published: run it, or copy it into your application.

```rust,no_run
use std::sync::Arc;

use eframe::egui;
use egui_charts::data::{Datafeed, HistoryPage, HistoryRequest, Reply, Subscription};
use egui_charts_terminal::Terminal;

/// Your market data: history on request, then live updates.
struct MyFeed;

impl Datafeed for MyFeed {
    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        // Up to `request.count` bars that open before `request.before`, or the latest bars
        // when it is `None`. Fetch them on another thread or task and reply from there.
        let _ = request;
        reply.send(Ok(HistoryPage::default()));
    }

    fn subscribe(&self, subscription: Subscription) {
        // Push live bars, trades and depth into `subscription.sink` from your connection
        // until `Sink::is_active` turns false.
        let _ = subscription;
    }
}

fn main() -> eframe::Result {
    let mut terminal = Terminal::new(Arc::new(MyFeed)).with_symbol("AAPL");
    eframe::run_ui_native("My terminal", eframe::NativeOptions::default(), move |ui, _frame| {
        for action in terminal.show(ui) {
            // Actions you intercepted, host entries and actions without a default.
            let _ = action;
        }
    })
}
```


`with_intercept` returns chosen actions to your app instead of performing them, and `run` performs one after your own checks. Storage, symbol search, alerts, brokers, signals, marks, quotes, notifications and chats are passed as services with the `with_` methods, and file export and sounds as callbacks; a control whose service is absent stays hidden. `examples/terminal.rs` runs a terminal on a synthetic feed: `cargo run --example terminal` in this directory. The components also work on their own: each returns an action for your app to apply, as the [demo](../examples/demo) does. The library does not connect to a market data provider.
