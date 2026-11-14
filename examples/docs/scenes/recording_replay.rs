use chrono::{DateTime, TimeDelta, Utc};
use eframe::egui;
use egui_charts::{
    Chart, ChartBuilder,
    data::{MarketEvent, RecordingReader, RecordingWriter, ReplayDriver, ReplayEvent},
    model::{Instrument, Level1Quote, OrderBook, OrderBookLevel, OrderSide, Tick, Timeframe},
    widget::SeriesId,
};
use std::{io, sync::Arc, time::Duration};

/// Record raw events in memory, preserving identifiers and nanosecond times.
pub fn recording() -> io::Result<(Vec<u8>, Vec<ReplayEvent>)> {
    let start = DateTime::UNIX_EPOCH + TimeDelta::days(20_000) + TimeDelta::nanoseconds(123);
    let level = |price, size| OrderBookLevel {
        price,
        size,
        order_cnt: None,
    };
    let mut events = vec![ReplayEvent::new(
        "SAMPLE",
        start,
        0,
        MarketEvent::DepthSnapshot {
            sequence: 40,
            book: OrderBook {
                ts: start,
                symbol: "SAMPLE".into(),
                bids: vec![level(100.0, 15.0)],
                asks: vec![level(100.25, 9.0)],
            },
        },
    )];
    events.push(ReplayEvent::new(
        "SAMPLE",
        start,
        1,
        MarketEvent::Quote(Level1Quote {
            ts: start,
            symbol: "SAMPLE".into(),
            bid: 100.0,
            bid_size: 15.0,
            ask: 100.25,
            ask_size: 9.0,
        }),
    ));
    for i in 0..12 {
        let time = start + TimeDelta::seconds(i as i64);
        let tick = Tick::new(
            time,
            100.0 + (i % 4) as f64 * 0.25,
            2.0,
            Some(OrderSide::Buy),
        )
        .with_id(i + 1)
        .with_aggressor_order_id(100 + i)
        .with_resting_order_id(200 + i);
        events.push(ReplayEvent::new(
            "SAMPLE",
            time,
            i + 2,
            MarketEvent::Trade(tick),
        ));
    }
    let time = start + TimeDelta::seconds(5);
    events.push(ReplayEvent::new(
        "SAMPLE",
        time,
        20,
        MarketEvent::DepthDelta {
            sequence: 41,
            book: OrderBook {
                ts: time,
                symbol: "SAMPLE".into(),
                bids: vec![level(100.0, 12.0)],
                asks: vec![],
            },
        },
    ));
    events.sort_by_key(|event| (event.time, event.sequence));
    let mut writer = RecordingWriter::new(Vec::new())?;
    for event in &events {
        writer.write(event)?;
    }
    Ok((writer.finish()?, events))
}

/// Drive the same sink as live data, using the recorded clock and book sequences.
pub struct RecordingReplay {
    driver: Arc<ReplayDriver>,
    start: DateTime<Utc>,
    last_frame: Option<f64>,
    speed: f64,
    bytes: usize,
}

impl RecordingReplay {
    /// Load the encoded stream and pause at the first observation.
    pub fn new() -> (Chart, Self) {
        let (bytes, events) = recording().expect("valid in-memory sample");
        let mut reader = RecordingReader::new(bytes.as_slice()).unwrap();
        for event in &events {
            assert_eq!(reader.read().unwrap().as_ref(), Some(event));
        }
        assert!(reader.read().unwrap().is_none());
        let mut instrument = Instrument::new("SAMPLE");
        instrument.tick_size = Some(0.25);
        let driver = Arc::new(ReplayDriver::load(bytes.as_slice(), vec![instrument]).unwrap());
        driver.set_speed(0.0).unwrap();
        let start = driver.time();
        // A paused advance cannot deliver the starting events.
        driver.seek(start);
        let mut chart = ChartBuilder::new()
            .with_feed(driver.clone())
            .with_symbol("SAMPLE")
            .with_timeframe(Timeframe::Sec1)
            .build();
        chart.set_visible_bars(16);
        (
            chart,
            Self {
                driver,
                start,
                last_frame: None,
                speed: 1.0,
                bytes: bytes.len(),
            },
        )
    }

    /// Advance outside context locks, before drawing; seeking rebuilds prior events.
    pub fn show(&mut self, ui: &mut egui::Ui, chart: &mut Chart) {
        let now = ui.input(|input| input.time);
        let elapsed = self
            .last_frame
            .map_or(0.0, |previous| (now - previous).max(0.0));
        self.last_frame = Some(now);
        self.driver
            .advance(Duration::from_secs_f64(elapsed))
            .unwrap();
        if self.driver.is_finished() {
            self.driver.set_speed(0.0).unwrap();
        }
        let toggle =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowDown));
        let step =
            ui.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowRight));
        ui.horizontal_wrapped(|ui| {
            let playing = self.driver.speed() > 0.0;
            if ui.button(if playing { "Pause" } else { "Play" }).clicked() || toggle {
                self.driver
                    .set_speed(if playing { 0.0 } else { self.speed })
                    .unwrap();
            }
            if ui.button("Step 1 second").clicked() || step {
                self.driver.set_speed(0.0).unwrap();
                let end = self.driver.time_range().unwrap().1;
                self.driver
                    .seek((self.driver.time() + TimeDelta::seconds(1)).min(end));
            }
            if ui.button("Restart").clicked() {
                self.driver.set_speed(0.0).unwrap();
                self.driver.seek(self.start);
            }
        });
        if ui
            .add(egui::Slider::new(&mut self.speed, 0.25..=4.0).text("Speed"))
            .changed()
            && self.driver.speed() > 0.0
        {
            self.driver.set_speed(self.speed).unwrap();
        }
        ui.label(format!(
            "Recorded playback · {} bytes · {}",
            self.bytes,
            self.driver.time().format("%H:%M:%S UTC")
        ));
        ui.label("Round trip verified: all 15 events, including trade ids and nanoseconds.");
        ui.allocate_ui(
            egui::vec2(
                ui.available_width(),
                (ui.available_height() - 24.0).max(80.0),
            ),
            |ui| chart.show(ui),
        );
        // Feed messages are applied by show before reading the retained state.
        let count = chart.trades(SeriesId::MAIN).count();
        let bid = chart
            .order_book(SeriesId::MAIN)
            .and_then(OrderBook::best_bid)
            .map_or_else(
                || "unavailable".into(),
                |level| format!("{:.0}", level.size),
            );
        ui.label(format!(
            "Delivered fills: {count}/12 · best bid size: {bid}"
        ));
        if self.driver.speed() > 0.0 {
            ui.ctx().request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        scene: &mut RecordingReplay,
        chart: &mut Chart,
        time: f64,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                // A 390 px guide has 16 px outer padding and a 1 px card border on each side.
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(356.0, 320.0),
                )),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| scene.show(ui, chart));
            },
        );
        output.textures_delta.clear();
        output
    }

    #[test]
    fn recorded_scene_starts_with_a_fill_and_rewinds_book_and_tape() {
        let (mut chart, mut scene) = RecordingReplay::new();
        let ctx = egui::Context::default();
        for i in 0..5 {
            let _ = frame(&ctx, &mut scene, &mut chart, f64::from(i) * 0.01);
        }
        assert_eq!(chart.trades(SeriesId::MAIN).count(), 1);
        assert_eq!(
            chart
                .order_book(SeriesId::MAIN)
                .unwrap()
                .best_bid()
                .unwrap()
                .size,
            15.0
        );
        scene.driver.seek(scene.start + TimeDelta::seconds(6));
        for i in 0..3 {
            let _ = frame(&ctx, &mut scene, &mut chart, 0.1 + f64::from(i) * 0.01);
        }
        assert_eq!(chart.trades(SeriesId::MAIN).count(), 7);
        assert_eq!(
            chart
                .order_book(SeriesId::MAIN)
                .unwrap()
                .best_bid()
                .unwrap()
                .size,
            12.0
        );
        scene.driver.seek(scene.start);
        for i in 0..3 {
            let _ = frame(&ctx, &mut scene, &mut chart, 0.2 + f64::from(i) * 0.01);
        }
        assert_eq!(chart.trades(SeriesId::MAIN).count(), 1);
        assert_eq!(
            chart
                .order_book(SeriesId::MAIN)
                .unwrap()
                .best_bid()
                .unwrap()
                .size,
            15.0
        );
    }

    #[test]
    fn replay_speed_label_fits_the_phone_canvas() {
        let (mut chart, mut scene) = RecordingReplay::new();
        let ctx = egui::Context::default();
        chart.set_theme(&ctx, &egui_charts::theme::Theme::light());
        let output = frame(&ctx, &mut scene, &mut chart, 0.0);
        let rect = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Speed" => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                }
                _ => None,
            })
            .expect("the speed label is painted");
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(356.0, 320.0))
                .contains_rect(rect),
            "{rect:?}"
        );
    }
}
