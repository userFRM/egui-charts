use chrono::TimeDelta;
use eframe::egui;
use egui_charts::{
    Chart, ChartBuilder,
    data::{Datafeed, HistoryPage, HistoryRequest, Reply, Sink, Subscription},
    model::{Bar, Tick, Timeframe},
    studies::EMA,
    widget::SeriesId,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

/// Draw an exponential average over price and relative strength below it.
pub fn builtins() -> Chart {
    use egui_charts::studies::{EMA, RSI};

    let mut chart = crate::first_chart::first_chart();
    chart.config_mut().show_volume = false;
    chart.add_indicator(Box::new(EMA::new(20)));
    chart.add_indicator(Box::new(RSI::new(14)));
    chart
}

/// Keep warm-up and unavailable closes at their original time slots.
pub fn custom() -> Chart {
    use eframe::egui::Color32;
    use egui_charts::{
        model::Bar,
        studies::{CustomIndicator, IndicatorValue},
        widget::SeriesId,
    };

    let mut chart = crate::first_chart::first_chart();
    chart.config_mut().show_volume = false;
    let mut bars = chart.bars(SeriesId::MAIN).to_vec();
    for bar in &mut bars[35..39] {
        *bar = Bar::whitespace(bar.time);
    }
    chart.set_data(SeriesId::MAIN, bars);
    chart.set_visible_bars(120);
    chart.add_indicator(Box::new(
        CustomIndicator::new("Price change (10)", |bars| {
            bars.iter()
                .enumerate()
                .map(|(i, bar)| {
                    i.checked_sub(10)
                        .filter(|&before| !bar.is_whitespace() && !bars[before].is_whitespace())
                        .map_or(IndicatorValue::None, |before| {
                            IndicatorValue::Single(bar.close - bars[before].close)
                        })
                })
                .collect()
        })
        .with_overlay(false)
        .with_color(Color32::from_rgb(126, 87, 194))
        .with_description("Close minus the close ten slots earlier; gaps stay missing"),
    ));
    chart
}

struct LocalFeed {
    bars: Vec<Bar>,
    sink: Mutex<Option<Sink>>,
    pages: AtomicUsize,
}

impl Datafeed for LocalFeed {
    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let end = self
            .bars
            .partition_point(|bar| request.before.is_none_or(|before| bar.time < before));
        let lower = self
            .bars
            .partition_point(|bar| request.since.is_some_and(|since| bar.time < since));
        let start = end.saturating_sub(request.count.min(200)).max(lower);
        self.pages.fetch_add(1, Ordering::Relaxed);
        reply.send(Ok(HistoryPage::new(
            self.bars[start..end].to_vec(),
            start == 0,
        )));
    }

    fn subscribe(&self, subscription: Subscription) {
        *self.sink.lock().unwrap() = Some(subscription.sink);
    }
}

/// Answer history locally and drive a synthetic stream from the frame clock.
pub struct LiveData {
    feed: Arc<LocalFeed>,
    started: Option<f64>,
    last_step: Option<i64>,
}

impl LiveData {
    /// Connect one chart to a finite history and an initially idle stream.
    pub fn new() -> (Chart, Self) {
        let start = chrono::DateTime::UNIX_EPOCH + TimeDelta::days(20_000);
        let bars = (0..1_200)
            .map(|i| {
                let open = 100.0 + f64::from(i) * 0.01 + (f64::from(i) * 0.1).sin();
                let close = open + (f64::from(i) * 0.3).sin() * 0.4;
                Bar::new(
                    start + TimeDelta::minutes(i.into()),
                    open,
                    open.max(close) + 0.2,
                    open.min(close) - 0.2,
                    close,
                    100.0,
                )
            })
            .collect();
        let feed = Arc::new(LocalFeed {
            bars,
            sink: Mutex::new(None),
            pages: AtomicUsize::new(0),
        });
        let mut chart = ChartBuilder::new()
            .with_symbol("SYNTH")
            .with_timeframe(Timeframe::Min1)
            .with_feed(feed.clone())
            .with_initial_bars(200)
            .build();
        chart.set_visible_bars(30);
        chart.add_indicator(Box::new(EMA::new(20)));
        (
            chart,
            Self {
                feed,
                started: None,
                last_step: None,
            },
        )
    }

    /// Emit one tick per quarter second and expose history and latest-bar navigation.
    pub fn show(&mut self, ui: &mut egui::Ui, chart: &mut Chart) {
        let sink = self.feed.sink.lock().unwrap().clone();
        if let Some(sink) = sink.filter(Sink::is_active) {
            let now = ui.input(|input| input.time);
            let started = *self.started.get_or_insert(now);
            let step = ((now - started) * 4.0).floor() as i64;
            if self.last_step != Some(step) {
                self.last_step = Some(step);
                let last = self.feed.bars.last().unwrap();
                sink.push_tick(Tick::new(
                    last.time + TimeDelta::minutes(1) + TimeDelta::seconds(step * 2),
                    last.close + (step as f64 * 0.3).sin(),
                    1.0,
                    None,
                ));
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Load older").clicked()
                && let Some(first) = chart.bars(SeriesId::MAIN).first()
            {
                chart.scroll_to_time(first.time - TimeDelta::minutes(1));
            }
            if ui.button("Latest").clicked() {
                chart.scroll_to_real_time(false);
            }
            ui.label(format!(
                "{} history pages",
                self.feed.pages.load(Ordering::Relaxed)
            ));
        });
        ui.label("Synthetic ticks: 250 ms apart; a new minute bar every 7.5 s.");
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_study_keeps_warmup_and_missing_values_aligned() {
        let chart = custom();
        let (_, study) = chart.indicators().next().unwrap();
        let values = study.values();
        assert_eq!(values.len(), 120);
        for i in (0..10).chain(35..39).chain(45..49) {
            assert_eq!(values.value(0, i), None, "slot {i}");
        }
        // Slot 10 closes at 101.8; slot 0 closes at 99.4.
        assert!((values.value(0, 10).unwrap() - 2.4).abs() < 1e-12);
        assert!(values.value(0, 39).is_some());
        assert!(values.value(0, 49).is_some());
    }

    #[test]
    fn live_scene_pages_history_and_updates_a_forming_bar() {
        let (mut chart, mut live) = LiveData::new();
        let ctx = egui::Context::default();
        let frame = |chart: &mut Chart, live: &mut LiveData, time| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    live.show(ui, chart);
                    chart.show(ui);
                },
            );
            output.textures_delta.clear();
        };
        for i in 0..5 {
            frame(&mut chart, &mut live, f64::from(i) * 0.01);
        }
        let started = live.started.unwrap();
        let forming = chart.bars(SeriesId::MAIN).last().unwrap().clone();
        frame(&mut chart, &mut live, started + 0.25);
        let updated = chart.bars(SeriesId::MAIN).last().unwrap();
        assert_eq!(updated.time, forming.time);
        assert_eq!(updated.open, forming.open);
        assert_eq!(updated.volume, 2.0);
        assert!((updated.close - forming.close - 0.2955202066613396).abs() < 1e-12);

        frame(&mut chart, &mut live, started + 7.5);
        let next = chart.bars(SeriesId::MAIN).last().unwrap();
        assert_eq!(next.time, forming.time + TimeDelta::minutes(1));
        assert_eq!(next.volume, 1.0);

        let first = chart.bars(SeriesId::MAIN)[0].time;
        let pages = live.feed.pages.load(Ordering::Relaxed);
        chart.scroll_to_time(first - TimeDelta::minutes(1));
        for i in 0..5 {
            frame(&mut chart, &mut live, started + 7.51 + f64::from(i) * 0.01);
        }
        assert!(live.feed.pages.load(Ordering::Relaxed) > pages);
        assert!(chart.bars(SeriesId::MAIN)[0].time < first);
        assert!(
            chart
                .bars(SeriesId::MAIN)
                .windows(2)
                .all(|pair| pair[0].time < pair[1].time)
        );
        let (_, ema) = chart.indicators().next().unwrap();
        assert_eq!(ema.values().len(), chart.bars(SeriesId::MAIN).len());
    }
}
