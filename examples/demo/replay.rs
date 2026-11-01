//! Host replay wiring: source history, start selection, checkpoints and layout groups.

use super::{DemoApp, live::LiveFeed};
use chrono::{DateTime, Utc};
use egui_charts::data::{MarketEvent, RecordingWriter, ReplayDriver, ReplayEvent};
use egui_charts::model::{Bar, BarClock, Timeframe};
use egui_charts::ui::replay::{ReplayAction, ReplayController, ReplayStartAction};
use egui_charts::widget::SeriesId;
use std::num::NonZeroU64;
use std::sync::Arc;

/// The demo records its raw executions, quotes and sequenced book messages in memory.
/// Native hosts can give the same writer a buffered file instead.
pub(super) struct Capture {
    writer: RecordingWriter<Vec<u8>>,
    sequence: u64,
}

impl Capture {
    pub(super) fn new() -> Self {
        Self {
            writer: RecordingWriter::new(Vec::new()).expect("memory recording"),
            sequence: 0,
        }
    }

    pub(super) fn push(&mut self, symbol: &str, time: DateTime<Utc>, event: MarketEvent) {
        self.writer
            .write(&ReplayEvent::new(symbol, time, self.sequence, event))
            .expect("generated market event");
        self.sequence += 1;
    }

    pub(super) fn bytes(&self) -> Vec<u8> {
        self.writer.get_ref().clone()
    }
}

impl DemoApp {
    pub(super) fn advance_replay(&mut self) {
        let Some(replay) = self.replay.as_mut() else {
            return;
        };
        let mut playback_time = None;
        if let Some(driver) = self.chart.replay_driver().cloned() {
            let now = Utc::now();
            let elapsed = self
                .replay_updated
                .replace(now)
                .and_then(|previous| (now - previous).to_std().ok())
                .unwrap_or_default();
            let speed = if replay.state.is_playing() {
                replay.state.speed.multiplier()
            } else {
                0.0
            };
            if let Err(error) = driver
                .set_speed(f64::from(speed))
                .and_then(|()| driver.advance(elapsed))
            {
                replay.handle_action(ReplayAction::Pause);
                self.notify(&error.to_string());
                return;
            }
            playback_time = Some(driver.time() + chrono::TimeDelta::nanoseconds(1));
            if replay.state.is_playing() {
                let clock = source_clock(&self.chart, self.replay_interval);
                replay.seek_cutoff(driver.time() + chrono::TimeDelta::nanoseconds(1), &clock);
                replay.state.playback_state = if driver.is_finished() {
                    egui_charts::ui::replay::PlaybackState::Finished
                } else {
                    egui_charts::ui::replay::PlaybackState::Playing
                };
            }
        } else {
            replay.update();
        }
        if let Some(time) = playback_time {
            self.apply_replay_window(time, false);
        } else {
            self.push_replay_window_to_chart();
        }
    }

    pub(super) fn toggle_replay(&mut self) {
        if self.replay.is_some() && self.replay_control.is_user_dismissed() {
            self.replay_control.show_control();
        } else if self.replay.is_some() {
            self.stop_replay();
        } else {
            self.replay_start
                .open(self.chart.bars(SeriesId::MAIN).first().map(|bar| bar.time));
        }
    }

    pub(super) fn start_replay(&mut self) {
        if self.chart.bars(SeriesId::MAIN).is_empty() {
            return;
        }
        self.live_was_paused = self.live.paused();
        self.live.set_paused(true);
        self.bars = self.chart.bars(SeriesId::MAIN).to_vec();
        self.replay_interval = self.state.timeframe;
        let mut replay = ReplayController::new(self.bars.clone(), Vec::new())
            .with_symbol(self.state.symbol.clone());
        replay.handle_action(ReplayAction::EnterReplayMode);
        self.replay = Some(replay);
        self.state.replay_active = true;
        self.replay_control.set_symbol(&self.state.symbol);
        self.replay_control.show_control();
        self.configure_replay_intervals();
        self.push_replay_window_to_chart();
    }

    pub(super) fn configure_replay_intervals(&mut self) {
        let mut intervals = vec![self.state.timeframe];
        if self.state.timeframe.duration_ms() > Timeframe::Min1.duration_ms() {
            intervals.push(Timeframe::Min1);
        }
        // The feed holds its recent trades, so replay can also advance one trade at a time.
        if !self.state.timeframe.is_trade_based() {
            intervals.push(Timeframe::Ticks(NonZeroU64::MIN));
        }
        self.replay_control
            .set_update_intervals(self.replay_interval, intervals);
    }

    pub(super) fn checkpoint_replay(&mut self) {
        if let Some(session) = self.replay.as_ref().and_then(|replay| replay.session()) {
            self.replay_session = Some((session, self.replay_interval));
        }
    }

    pub(super) fn stop_replay(&mut self) {
        self.checkpoint_replay();
        self.replay_start.is_open = false;
        self.replay_selecting = false;
        if self.replay.take().is_some() {
            self.paper.set_replay_time(None);
            self.chart.set_replay_time(None);
            for extra in &mut self.extra_charts {
                extra.chart.set_replay_time(None);
            }
            self.replay_updated = None;
            self.replay_applied = None;
            self.replay_slots.clear();
            self.chart.set_feed_paused(false);
            self.live.set_paused(self.live_was_paused);
            self.state.replay_active = false;
            self.replay_control.hide_control();
        }
    }

    pub(super) fn push_replay_window_to_chart(&mut self) {
        self.push_replay_window(false);
    }

    pub(super) fn push_market_replay_window(&mut self) {
        if let Some(driver) = self.chart.replay_driver() {
            self.apply_replay_window(driver.time() + chrono::TimeDelta::nanoseconds(1), false);
        }
    }

    /// Applies the cursor to the charts and the paper account, which restarts when a replay
    /// starts or the cutoff moves back. `kept` keeps it on a move back that undoes no fill.
    fn push_replay_window(&mut self, kept: bool) {
        let Some(replay) = self.replay.as_ref() else {
            return;
        };
        let clock = source_clock(&self.chart, self.replay_interval);
        let Some(time) = replay.cutoff(&clock) else {
            return;
        };
        self.apply_replay_window(time, kept);
    }

    fn apply_replay_window(&mut self, time: DateTime<Utc>, kept: bool) {
        let Some(replay) = self.replay.as_ref() else {
            return;
        };
        let previous = self.replay_applied.replace(time);
        if previous.is_none_or(|previous| time < previous && !kept) {
            self.paper.reset();
            if self.trading.has_broker() {
                self.trading.set_service(self.paper.clone());
            }
        }
        if let Some(previous) = previous.filter(|previous| *previous < time) {
            for bar in replay
                .visible_bars()
                .iter()
                .filter(|bar| bar.time >= previous)
            {
                self.paper.set_replay_time(Some(bar.time));
                self.paper.set_price(self.chart.symbol(), bar.close);
            }
        }
        let source = self.layout_slot;
        let slots: Vec<_> = (0..self.extra_charts.len() + 1)
            .filter(|&slot| self.layout.sync_group(slot) == self.layout.sync_group(source))
            .collect();
        for (index, extra) in self.extra_charts.iter_mut().enumerate() {
            let slot = if index >= source { index + 1 } else { index };
            if slots.contains(&slot) && !self.replay_slots.contains(&slot) {
                // A copied slot may contain only the visible replay prefix.
                let bars = extra.live.snapshot(
                    extra.chart.timeframe().unwrap_or_default(),
                    extra.chart.bar_clock(),
                    extra.chart.extended_hours(),
                );
                extra.chart.set_data(SeriesId::MAIN, bars);
                extra.chart.set_replay_time(Some(time));
                let (bars, clock) = replay_source(&extra.live, self.replay_interval, &extra.chart);
                extra.chart.set_replay_source(bars, clock);
            }
        }
        let mut charts: Vec<_> = self
            .extra_charts
            .iter_mut()
            .map(|extra| &mut extra.chart)
            .collect();
        charts.insert(source, &mut self.chart);
        for &slot in &self.replay_slots {
            if !slots.contains(&slot)
                && let Some(chart) = charts.get_mut(slot)
            {
                chart.set_replay_time(None);
            }
        }
        self.layout.set_replay_time(&mut charts, source, Some(time));
        self.replay_slots = slots;
        if let Some(bar) = replay.visible_bars().last() {
            self.paper.set_replay_time(Some(bar.time));
            self.paper.set_price(self.chart.symbol(), bar.close);
        }
        self.checkpoint_replay();
    }

    pub(super) fn set_replay_interval(&mut self, interval: Timeframe) {
        if self.replay.is_none() {
            return;
        }
        let (bars, clock) = replay_source(&self.live, interval, &self.chart);
        if bars.is_empty() {
            self.notify("No source history for that update interval");
            return;
        }
        let cutoff = self.chart.replay_time();
        if !interval.is_trade_based() && self.chart.replay_driver().is_some() {
            self.chart.set_market_replay(None);
            self.chart.set_replay_time(cutoff);
            for extra in &mut self.extra_charts {
                if extra.chart.replay_driver().is_some() {
                    extra.chart.set_market_replay(None);
                    extra.chart.set_replay_time(cutoff);
                }
            }
        }
        let mut next =
            ReplayController::new(bars.clone(), Vec::new()).with_symbol(self.state.symbol.clone());
        if let Some(cutoff) = cutoff
            && !next.seek_cutoff(cutoff, &clock)
        {
            self.notify("No source history at the replay cursor");
            return;
        }
        if let Some(replay) = &self.replay {
            next.state.set_speed(replay.state.speed);
            if replay.state.is_playing() {
                next.handle_action(ReplayAction::Play);
            }
        }
        self.replay = Some(next);
        self.replay_interval = interval;
        // One-trade updates move the cutoff back only to just after the last trade before it.
        self.push_replay_window(matches!(interval, Timeframe::Ticks(count) if count.get() == 1));
        self.chart.set_replay_source(bars, clock);
        if interval.is_trade_based() && self.chart.replay_driver().is_none() {
            let mut instruments = vec![self.live.instrument()];
            let mut events = self.live.recording();
            // Concatenate decoded events, rather than binary streams with separate headers.
            let mut reader = egui_charts::data::RecordingReader::new(events.as_slice())
                .expect("memory recording");
            let mut merged = Vec::new();
            while let Some(event) = reader.read().expect("memory recording") {
                merged.push(event);
            }
            for (index, extra) in self.extra_charts.iter().enumerate() {
                let slot = if index >= self.layout_slot {
                    index + 1
                } else {
                    index
                };
                if !self.replay_slots.contains(&slot)
                    || instruments
                        .iter()
                        .any(|instrument| instrument.symbol == extra.chart.symbol())
                {
                    continue;
                }
                instruments.push(extra.live.instrument());
                events = extra.live.recording();
                let mut reader = egui_charts::data::RecordingReader::new(events.as_slice())
                    .expect("memory recording");
                while let Some(event) = reader.read().expect("memory recording") {
                    merged.push(event);
                }
            }
            let driver =
                Arc::new(ReplayDriver::new(merged, instruments).expect("captured market data"));
            if let Some(time) = self.chart.replay_time() {
                driver.seek(time - chrono::TimeDelta::nanoseconds(1));
            }
            self.chart.set_market_replay(Some(driver));
        }
        let driver = self.chart.replay_driver().cloned();
        for (index, extra) in self.extra_charts.iter_mut().enumerate() {
            let slot = if index >= self.layout_slot {
                index + 1
            } else {
                index
            };
            if self.replay_slots.contains(&slot) {
                let (bars, clock) = replay_source(&extra.live, interval, &extra.chart);
                extra.chart.set_replay_source(bars, clock);
                if let Some(driver) = &driver {
                    extra.chart.set_market_replay(Some(driver.clone()));
                }
            }
        }
        self.configure_replay_intervals();
    }

    pub(super) fn show_replay_start(&mut self, ctx: &egui::Context) {
        let action = self.replay_start.show(ctx, self.replay_session.is_some());
        if action != ReplayStartAction::None {
            ctx.request_repaint();
        }
        match action {
            ReplayStartAction::None => {}
            ReplayStartAction::SelectBar => {
                self.replay_selecting = true;
                self.chart.drawings_mut().set_active_tool(None);
                self.chart.drain_events().for_each(drop);
            }
            ReplayStartAction::Resume => {
                if let Some((session, interval)) = self.replay_session.clone() {
                    if session.symbol != self.state.symbol {
                        self.switch_symbol(&session.symbol);
                    }
                    let clock = source_clock(&self.chart, interval);
                    let bars = self
                        .live
                        .snapshot(interval, clock, self.chart.extended_hours());
                    let mut replay = ReplayController::new(bars, Vec::new())
                        .with_symbol(self.state.symbol.clone());
                    if !replay.restore_session(&session) {
                        self.replay_session = Some((session, interval));
                        self.notify(
                            "Saved replay needs source history that is no longer available",
                        );
                        return;
                    }
                    self.start_replay();
                    self.replay = Some(replay);
                    self.replay_interval = interval;
                    self.push_replay_window_to_chart();
                    self.set_replay_interval(interval);
                }
            }
            choice => {
                let bars = self.chart.bars(SeriesId::MAIN);
                let index = match choice {
                    ReplayStartAction::Date(time) => bars.partition_point(|bar| bar.time < time),
                    ReplayStartAction::Random => {
                        use std::hash::BuildHasher;
                        std::collections::hash_map::RandomState::new().hash_one(chrono::Utc::now())
                            as usize
                            % bars.len().max(1)
                    }
                    _ => 0,
                };
                if index >= bars.len() {
                    self.notify("No history on or after the selected date");
                    return;
                }
                self.start_replay_at(index);
            }
        }
        if self.replay_selecting {
            let plot = self.chart.get_rendered_price_rect();
            egui::Area::new(egui::Id::unique("replay_select_bar"))
                .fixed_pos(plot.min + egui::vec2(12.0, 36.0))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label("Click a bar to start replay");
                        if egui_charts::ui_kit::buttons::Button::new("Cancel")
                            .secondary()
                            .show(ui)
                            .clicked()
                            || ctx.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            })
                        {
                            self.replay_selecting = false;
                        }
                    });
                });
        }
    }

    pub(super) fn handle_replay_bar_click(
        &mut self,
        ctx: &egui::Context,
        time: chrono::DateTime<chrono::Utc>,
    ) {
        if !self.replay_selecting {
            return;
        }
        ctx.request_repaint();
        let bars = self.chart.bars(SeriesId::MAIN);
        let index = bars
            .partition_point(|bar| bar.time <= time)
            .saturating_sub(1);
        if bars.first().is_none_or(|bar| time < bar.time) || time > bars.last().unwrap().time {
            return;
        }
        self.replay_selecting = false;
        self.start_replay_at(index);
    }

    /// Starts replay at bar `index` of the main series, as picking that bar does.
    pub(crate) fn start_replay_at(&mut self, index: usize) {
        self.start_replay();
        if let Some(replay) = &mut self.replay {
            replay.handle_action(ReplayAction::JumpToBar(index));
        }
        self.push_replay_window_to_chart();
    }
}

/// The replay source of `interval` for `chart`, with its clock. The feed keeps trades from a
/// minute on, so a one-trade source leaves out the chart bar that opened before the minute of
/// its first trade: that bar lost earlier trades, and the chart shows its own bar there.
fn replay_source(
    live: &LiveFeed,
    interval: Timeframe,
    chart: &egui_charts::Chart,
) -> (Vec<Bar>, BarClock) {
    let clock = source_clock(chart, interval);
    let mut bars = live.snapshot(interval, clock.clone(), chart.extended_hours());
    if interval.is_trade_based()
        && let Some(first) = bars.first()
    {
        let from = BarClock::new(Timeframe::Min1).bar_open(first.time);
        let chart_clock = chart.bar_clock();
        // shortcut: a bar whose first minute had no trades is left out too; expose the feed's
        // retention start if that shows.
        let end = bars.partition_point(|bar| chart_clock.bar_open(bar.time) < from);
        bars.drain(..end);
    }
    (bars, clock)
}

fn source_clock(chart: &egui_charts::Chart, interval: Timeframe) -> BarClock {
    use egui_charts::scales::TimezoneMode;
    let clock = BarClock::new(interval);
    match &chart.chart_options().time_scale.timezone {
        TimezoneMode::Local => clock.with_local_timezone(),
        TimezoneMode::Exchange(fallback) => clock.with_timezone(
            chart
                .instrument()
                .and_then(|instrument| instrument.time_zone.as_ref())
                .cloned()
                .unwrap_or_else(|| fallback.clone()),
        ),
        zone => zone
            .to_tz()
            .map_or(clock.clone(), |zone| clock.with_timezone(zone)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_charts::model::{Bar, OrderSide};
    use egui_charts::ui::floating_replay_control::ReplayControlAction;

    #[test]
    fn paper_account_resets_on_replay_start_and_rewind_but_not_forward_steps() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        for _ in 0..4 {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    app.chart.show(ui);
                },
            )
            .drop_without_applying_deltas();
        }
        let start = chrono::DateTime::from_timestamp(1_704_067_200, 0).unwrap();
        let bars = [100.0, 80.0, 110.0]
            .into_iter()
            .enumerate()
            .map(|(i, price)| {
                Bar::new(
                    start + chrono::Duration::minutes(i as i64),
                    price,
                    price,
                    price,
                    price,
                    1.0,
                )
            })
            .collect();
        app.chart.set_data(SeriesId::MAIN, bars);
        let symbol = app.chart.symbol().to_owned();
        app.paper.set_price(&symbol, 100.0);
        app.trading.set_service(app.paper.clone());
        app.trading.display.instant_orders = true;
        let position = |app: &mut DemoApp| {
            // The first call after reconnecting requests the symbol's trading rules.
            app.trading.poll(&ctx, &mut app.toasts);
            app.trading.trade(symbol.clone(), OrderSide::Buy);
            app.trading.poll(&ctx, &mut app.toasts);
            app.trading.trade(symbol.clone(), OrderSide::Buy);
            app.trading.poll(&ctx, &mut app.toasts);
        };
        let empty = |app: &mut DemoApp| {
            app.trading.poll(&ctx, &mut app.toasts);
            let mut empty = false;
            for _ in 0..3 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        app.trading.show_account_panel(ui, &symbol);
                    },
                );
                empty = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text)
                        if text.galley.text() == "No open positions")
                });
                output.drop_without_applying_deltas();
            }
            empty
        };

        position(&mut app);
        assert!(!empty(&mut app));
        app.start_replay();
        assert!(empty(&mut app), "a new replay must clear the live position");
        for action in [
            ReplayControlAction::PreviousBar,
            ReplayControlAction::GoToStart,
        ] {
            position(&mut app);
            app.handle_replay_control(ReplayControlAction::GoToEnd);
            assert!(!empty(&mut app), "forward playback must keep the position");
            app.handle_replay_control(action);
            assert!(empty(&mut app), "rewinding must clear the future position");
        }
        position(&mut app);
        app.stop_replay();
        app.start_replay();
        assert!(
            empty(&mut app),
            "restarting must clear the previous session"
        );

        // One-trade updates move the cutoff back only to just after the last trade: no fill is undone.
        app.stop_replay();
        let bars = app.live.snapshot(
            app.state.timeframe,
            app.chart.bar_clock(),
            app.chart.extended_hours(),
        );
        app.chart.set_data(SeriesId::MAIN, bars);
        app.start_replay();
        app.handle_replay_control(ReplayControlAction::GoToEnd);
        position(&mut app);
        let before = app.chart.replay_time();
        let trade = Timeframe::Ticks(std::num::NonZeroU64::MIN);
        app.handle_replay_control(ReplayControlAction::SetUpdateInterval(trade));
        assert_eq!(app.replay_interval, trade);
        assert!(
            app.chart.replay_time() < before,
            "the cursor sits just after the last trade"
        );
        assert!(
            !empty(&mut app),
            "switching to trade updates must keep the position"
        );
        let driver = app.chart.replay_driver().unwrap().clone();
        let cursor = driver.time() + chrono::TimeDelta::milliseconds(50);
        driver.seek(cursor);
        app.handle_replay_control(ReplayControlAction::Close);
        assert_eq!(driver.time(), cursor, "closing the transport must not seek");
        assert!(
            !empty(&mut app),
            "closing the transport must keep the position"
        );
        let range = driver.time_range();
        assert!(range.is_some(), "the captured recording is not empty");
        app.handle_replay_control(ReplayControlAction::SetUpdateInterval(app.state.timeframe));
        app.handle_replay_control(ReplayControlAction::SetUpdateInterval(trade));
        assert_eq!(
            app.chart.replay_driver().unwrap().time_range(),
            range,
            "returning to trade updates must retain the recording"
        );
    }

    /// The feed keeps trades from 14:00, the hour two hours before 16:30: an hourly chart's
    /// one-trade source keeps them all, a four-hour chart's leaves out the bar from 12:00, which
    /// lost its first two hours, and keeps the trades from 16:00.
    #[test]
    fn trade_sources_leave_out_a_bar_the_feed_holds_in_part() {
        let time = |text: &str| text.parse::<chrono::DateTime<chrono::Utc>>().unwrap();
        let live = LiveFeed::new("SAMPLE", time("2024-03-15T16:30:00Z"));
        let trade = Timeframe::Ticks(std::num::NonZeroU64::MIN);
        for (timeframe, from) in [
            (Timeframe::Hour1, time("2024-03-15T14:00:00Z")),
            (Timeframe::Hour4, time("2024-03-15T16:00:00Z")),
        ] {
            let mut chart =
                egui_charts::Chart::new(egui_charts::model::BarData::from_bars(Vec::new()));
            chart.set_timeframe(timeframe);
            let all = live.snapshot(trade, BarClock::new(trade), chart.extended_hours());
            assert!(
                all[0].time < time("2024-03-15T14:01:00Z"),
                "the feed keeps the first minute"
            );
            let expected: Vec<_> = all.iter().filter(|bar| bar.time >= from).cloned().collect();
            assert!(!expected.is_empty());
            assert_eq!(
                replay_source(&live, trade, &chart).0,
                expected,
                "{timeframe}"
            );
        }
    }
}

pub(super) fn capture_book(
    capture: &mut Capture,
    symbol: &str,
    market: &super::market::Market,
    orders: Option<&super::orders::Orders>,
    time: DateTime<Utc>,
) {
    let event = match orders {
        Some(orders) => {
            let (sequence, orders) = orders.snapshot();
            MarketEvent::OrderSnapshot { sequence, orders }
        }
        None => {
            let (sequence, book) = market.snapshot(time);
            MarketEvent::DepthSnapshot { sequence, book }
        }
    };
    capture.push(symbol, time, event);
}
