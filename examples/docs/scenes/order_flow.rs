use chrono::{DateTime, TimeDelta, Utc};
use eframe::egui;
use egui_charts::{
    Chart,
    chart::overlays::FootprintConfig,
    data::{Footprint, OrderFlowInput},
    model::{Bar, BarClock, BarData, OrderBook, OrderBookLevel, OrderSide, Tick, Timeframe},
    theme::ThemeExt,
};

/// Compare complete executions with a shorter retained history and a lost book.
pub struct ExecutionDepth {
    start: DateTime<Utc>,
    recent: bool,
    lost_depth: bool,
    flow: Footprint,
    executions: OrderFlowInput,
    books: OrderFlowInput,
}

impl ExecutionDepth {
    /// Supply candles independently of the retained execution and depth histories.
    pub fn new() -> (Chart, Self) {
        let start = DateTime::UNIX_EPOCH + TimeDelta::days(20_000);
        let bars = (0..12)
            .map(|i| {
                let price = 100.0 + f64::from(i) * 0.25;
                Bar::new(
                    start + TimeDelta::minutes(i.into()),
                    price,
                    price + 0.5,
                    price,
                    price + 0.5,
                    12.0,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_symbol("SAMPLE");
        chart.set_timeframe(Timeframe::Min1);
        chart.set_visible_bars(14);
        let mut scene = Self {
            start,
            recent: false,
            lost_depth: false,
            flow: Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap(),
            executions: OrderFlowInput::footprint(),
            books: OrderFlowInput::book(1),
        };
        scene.rebuild();
        (chart, scene)
    }

    fn rebuild(&mut self) {
        let start = self.start;
        let clock = BarClock::new(Timeframe::Min1);
        self.flow = Footprint::new(clock.clone(), 0.25, 1).unwrap();
        self.executions.clear();
        self.executions.set_tick_size(Some(0.25));
        self.books.clear();
        self.books.set_tick_size(Some(0.25));
        for i in (if self.recent { 6 } else { 0 })..12 {
            let time = start + TimeDelta::minutes(i.into());
            let price = 100.0 + f64::from(i) * 0.25;
            for (second, price, size, side) in [
                (0, price, 3.0, Some(OrderSide::Sell)),
                (10, price + 0.25, 8.0, Some(OrderSide::Buy)),
                (20, price + 0.5, 1.0, None),
            ] {
                let tick = Tick::new(time + TimeDelta::seconds(second), price, size, side);
                self.flow.push(&tick).unwrap();
                self.executions.push_trade(&clock, &tick);
            }
            let level = |price, size| OrderBookLevel {
                price,
                size,
                order_cnt: None,
            };
            let book = OrderBook {
                ts: time,
                symbol: "SAMPLE".into(),
                bids: vec![level(price, 15.0)],
                asks: vec![level(price + 0.25, 9.0)],
            };
            self.books.push_book(&clock, time, Some(&book));
        }
        if self.lost_depth {
            self.books.push_book(
                &clock,
                start + TimeDelta::minutes(11) + TimeDelta::seconds(30),
                None,
            );
        }
    }

    /// Read coverage from the sources; let the chart fall back to candles outside it.
    pub fn show(&mut self, ui: &mut egui::Ui, chart: &mut Chart) {
        let start = self.start;
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            changed |= ui
                .checkbox(&mut self.recent, "Retain last 6 bars only")
                .changed();
            changed |= ui
                .checkbox(&mut self.lost_depth, "Lose latest book")
                .changed();
        });
        if changed {
            self.rebuild();
        }
        let flow = self.executions.footprints_from(start);
        let books = self.books.books_from(start);
        let from = |time: Option<DateTime<Utc>>| {
            time.map_or_else(
                || "unavailable".into(),
                |t| t.format("%H:%M UTC").to_string(),
            )
        };
        ui.label(format!(
            "Complete from: executions {}, depth {}",
            from(flow.complete_from),
            from(books.complete_from)
        ));
        ui.label("Each retained bar: 12 volume = 8 buy + 3 sell + 1 unknown; delta = +5.");
        if let Some(book) = books.bars.get(&(start + TimeDelta::minutes(11))) {
            ui.label(format!(
                "Latest depth: bid {:.2} × {:.0}, ask {:.2} × {:.0}",
                book.bids[0].price, book.bids[0].size, book.asks[0].price, book.asks[0].size
            ));
        } else {
            ui.label("Latest depth unavailable until a synchronized snapshot arrives.");
        }
        let config = FootprintConfig {
            text_color: ui.active_theme().chart.axis_text,
            ..Default::default()
        };
        chart.show_footprint(ui, &self.flow, &config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorter_execution_history_keeps_candles_and_book_loss_removes_latest_sizes() {
        let (chart, mut scene) = ExecutionDepth::new();
        let start = scene.start;
        scene.recent = true;
        scene.lost_depth = true;
        scene.rebuild();
        let flow = scene.executions.footprints_from(start);
        assert_eq!(flow.complete_from, Some(start + TimeDelta::minutes(6)));
        assert_eq!(flow.bars.len(), 6);
        for bar in flow.bars.values() {
            assert_eq!((bar.volume(), bar.delta()), (12.0, 5.0));
        }
        assert_eq!(chart.bars(egui_charts::widget::SeriesId::MAIN).len(), 12);
        assert!(
            !scene
                .books
                .books_from(start)
                .bars
                .contains_key(&(start + TimeDelta::minutes(11)))
        );
    }
}
