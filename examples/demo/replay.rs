//! Host replay wiring: source history, start selection, checkpoints and layout groups.

use super::DemoApp;
use egui_charts::model::{BarClock, Timeframe};
use egui_charts::ui::replay::{ReplayAction, ReplayController, ReplayStartAction};
use egui_charts::widget::{ChartEvent, SeriesId};

impl DemoApp {
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
        self.live_was_paused = if self.footprint.is_some() {
            self.feed_before_footprint
        } else {
            self.live.paused()
        };
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
            self.chart.set_replay_time(None);
            for extra in &mut self.extra_charts {
                extra.chart.set_replay_time(None);
            }
            self.replay_slots.clear();
            self.chart.set_feed_paused(self.footprint.is_some());
            self.live
                .set_paused(self.footprint.is_some() || self.live_was_paused);
            self.state.replay_active = false;
            self.replay_control.hide_control();
        }
    }

    pub(super) fn push_replay_window_to_chart(&mut self) {
        let Some(replay) = self.replay.as_ref() else {
            return;
        };
        let clock = source_clock(&self.chart, self.replay_interval);
        let Some(time) = replay.cutoff(&clock) else {
            return;
        };
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
                let clock = source_clock(&extra.chart, self.replay_interval);
                let bars = extra.live.snapshot(
                    self.replay_interval,
                    clock.clone(),
                    extra.chart.extended_hours(),
                );
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
        self.checkpoint_replay();
    }

    pub(super) fn set_replay_interval(&mut self, interval: Timeframe) {
        if self.replay.is_none() {
            return;
        }
        let clock = source_clock(&self.chart, interval);
        let bars = self
            .live
            .snapshot(interval, clock.clone(), self.chart.extended_hours());
        if bars.is_empty() {
            self.notify("No source history for that update interval");
            return;
        }
        let cutoff = self.chart.replay_time();
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
        self.push_replay_window_to_chart();
        self.chart.set_replay_source(bars, clock);
        for (index, extra) in self.extra_charts.iter_mut().enumerate() {
            let slot = if index >= self.layout_slot {
                index + 1
            } else {
                index
            };
            if self.replay_slots.contains(&slot) {
                let clock = source_clock(&extra.chart, interval);
                let bars =
                    extra
                        .live
                        .snapshot(interval, clock.clone(), extra.chart.extended_hours());
                extra.chart.set_replay_source(bars, clock);
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
                self.start_replay();
                if let Some(replay) = &mut self.replay {
                    replay.handle_action(ReplayAction::JumpToBar(index));
                }
                self.push_replay_window_to_chart();
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

    pub(super) fn handle_replay_bar_click(&mut self, ctx: &egui::Context) {
        if !self.replay_selecting {
            return;
        }
        let time = self.chart.drain_events().find_map(|event| match event {
            ChartEvent::Click {
                position,
                button: egui::PointerButton::Primary,
            } if position.series == SeriesId::MAIN => Some(position.time),
            _ => None,
        });
        if let Some(time) = time {
            ctx.request_repaint();
            let bars = self.chart.bars(SeriesId::MAIN);
            let index = bars
                .partition_point(|bar| bar.time <= time)
                .saturating_sub(1);
            if bars.first().is_none_or(|bar| time < bar.time) || time > bars.last().unwrap().time {
                return;
            }
            self.replay_selecting = false;
            self.start_replay();
            if let Some(replay) = &mut self.replay {
                replay.handle_action(ReplayAction::JumpToBar(index));
            }
            self.push_replay_window_to_chart();
        }
    }
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
                .and_then(|zone| zone.name().parse().ok())
                .unwrap_or(*fallback),
        ),
        zone => zone
            .to_tz()
            .map_or(clock.clone(), |zone| clock.with_timezone(zone)),
    }
}
