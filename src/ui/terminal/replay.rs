//! Replay: the replay start picker and the floating replay control across the chart slots.
//!
//! The replay steps through the bars of the slot it started on, the source slot. Every other slot
//! of the same synchronization group follows one cutoff, so all of them show the same moment. A
//! finer update interval is requested through the terminal's [`Datafeed`](crate::data::Datafeed)
//! and applied to each slot with [`Chart::set_replay_source`](crate::Chart::set_replay_source)
//! when the feed answers. Every broker is told the replay time and close
//! ([`Broker::observe`]), so a simulated account fills at the prices on the chart.

use super::core::Direction;
use super::layout::Region;
use super::{Action, Terminal};
use crate::Chart;
use crate::data::inbox::Inbox;
use crate::data::{FeedError, HistoryPage, HistoryRequest};
use crate::model::{Bar, BarClock, Timeframe};
use crate::services::{Broker, UserSettings};
use crate::ui::Control;
use crate::ui::floating_replay_control::{FloatingReplayControl, ReplayControlAction};
use crate::ui::replay::{
    ReplayAction, ReplayController, ReplaySession, ReplayStartAction, ReplayStartPicker,
};
use crate::ui::top_toolbar::TopToolbarAction;
use crate::ui::trading::BrokerTile;
use crate::widget::{ChartEvent, SeriesId};
use chrono::{DateTime, TimeDelta, Utc};
use std::sync::Arc;

/// Bars asked for in one page of finer history.
const PAGE: usize = 5_000;
/// Finer bars kept for one slot, the newest ones: older history is not paged in.
const SOURCE_LIMIT: usize = 200_000;
/// Seconds a saved session waits for the history of its symbol before it is dropped.
const RESUME_WAIT: f64 = 10.0;

#[derive(Default)]
pub(super) struct State {
    /// Steps through the source bars; `Some` while a replay runs.
    controller: Option<ReplayController>,
    picker: ReplayStartPicker,
    /// Waiting for a click on a bar of the source slot to start there.
    selecting: bool,
    /// The slot the replay runs on. Replay is bound to it while it runs or waits for a click.
    source: usize,
    /// The source slot's bars at its own interval, kept because its chart shows only the bars
    /// the replay has revealed.
    base: Vec<Bar>,
    /// The interval of `base`.
    base_interval: Option<Timeframe>,
    /// The interval of the source bars the cursor steps through.
    interval: Timeframe,
    /// The source slot's interval when last seen, to follow a change of it.
    display: Option<Timeframe>,
    /// The position to resume with the interval of its source bars, as kept in the settings.
    session: Option<(ReplaySession, Timeframe)>,
    /// A saved session waiting for the history of its symbol, and the time it was asked for.
    resume: Option<(ReplaySession, Timeframe, f64)>,
    /// The slots holding the replay cutoff.
    slots: Vec<usize>,
    /// The time of the last replayed bar the brokers were told.
    observed: Option<DateTime<Utc>>,
    control: FloatingReplayControl,
    /// Where the feed answers for finer history arrive.
    inbox: Arc<Inbox>,
    next_key: u64,
    /// Finer history on its way.
    fetches: Vec<Fetch>,
}

/// What to do with finer history once the feed has delivered it for every slot.
enum Intent {
    /// Give the slots their finer source; the cursor stays on its bars.
    Sources,
    /// Step through the finer bars of the source slot from the cutoff the chart shows.
    Seek,
    /// Resume a saved session on the finer bars, which starts the replay.
    Restore(ReplaySession),
}

struct Fetch {
    intent: Intent,
    interval: Timeframe,
    /// The bars needed start here, which is where playback is or resumes.
    since: DateTime<Utc>,
    jobs: Vec<Job>,
}

/// The finer history of one slot.
struct Job {
    slot: usize,
    symbol: String,
    clock: BarClock,
    extended_hours: bool,
    bars: Vec<Bar>,
    /// The key of the request in flight.
    pending: Option<u64>,
    done: bool,
}

impl Terminal {
    pub(super) fn replay_init(&mut self) {
        self.set_control_visible(Control::Replay, true);
    }

    pub(super) fn replay_poll(&mut self, ctx: &egui::Context) {
        let wake = ctx.clone();
        self.replay.inbox.set_waker(move || wake.request_repaint());
        self.replay_receive();
        if self
            .replay
            .resume
            .as_ref()
            .is_some_and(|(_, _, asked)| ctx.input(|input| input.time) - asked > RESUME_WAIT)
        {
            self.replay.resume = None;
            self.toast("Saved replay could not load its history");
        }
        let source = self.replay.source;
        if source >= self.charts.len() {
            self.replay_stop();
            // Otherwise every later frame would close the start picker again.
            self.replay.source = self.active;
            return;
        }
        if self.replay.selecting || self.replay.controller.is_some() {
            // The replay belongs to its slot; clicking another does not move it.
            if self.active != source {
                self.activate_slot(source);
            }
        }
        let Some(controller) = &self.replay.controller else {
            self.replay_observe_live();
            return;
        };
        let chart = &self.charts[source];
        if controller.state.symbol.as_deref() != Some(chart.symbol()) {
            self.replay_stop();
            return;
        }
        if chart.timeframe() != self.replay.display {
            // Playback stays on its own interval when the display interval changes.
            self.replay.display = chart.timeframe();
            self.replay_configure_intervals();
        }
        if !self.keyboard_dialog_open() {
            let action = self.replay.control.shortcut(ctx);
            if action != ReplayControlAction::None {
                self.emit(Action::Replay(action));
            }
        }
        if let Some(controller) = &mut self.replay.controller {
            controller.update();
            if controller.state.is_playing() {
                ctx.request_repaint();
            }
        }
        self.replay_push();
    }

    pub(super) fn replay_show(&mut self, ui: &mut egui::Ui, region: Region) {
        if region != Region::Windows {
            return;
        }
        let ctx = ui.ctx().clone();
        let source = self.replay.source;
        let plot = self
            .charts
            .get(source)
            .map_or(egui::Rect::NOTHING, Chart::get_rendered_price_rect);
        if let Some(controller) = &self.replay.controller {
            let state = &controller.state;
            self.replay.control.sync_from(
                state.is_active(),
                state.is_playing(),
                state.speed.multiplier(),
                state.curr_bar,
                state.total_bars,
            );
            if plot.is_positive() {
                self.replay.control.set_dock_rect(plot);
            }
            let action = self.replay.control.show(&ctx);
            if action != ReplayControlAction::None {
                self.emit(Action::Replay(action));
            }
        }
        let action = self.replay.picker.show(&ctx, self.replay.session.is_some());
        if action != ReplayStartAction::None {
            self.emit(Action::ReplayStart(action));
        }
        if self.replay.selecting {
            self.replay_select_bar(&ctx, plot);
        }
        self.replay_resume_when_loaded(&ctx);
    }

    pub(super) fn replay_perform(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::TopToolbar(TopToolbarAction::ToggleReplay) => self.replay_toggle(),
            Action::Replay(action) => self.replay_transport(action),
            Action::ReplayStart(action) => self.replay_choose_start(action),
            action => return Some(action),
        }
        None
    }

    pub(super) fn replay_settings(&mut self, settings: &mut UserSettings, direction: Direction) {
        match direction {
            Direction::Load => self.replay.session = settings.replay_session.clone(),
            Direction::Save => {
                self.replay_checkpoint();
                settings.replay_session = self.replay.session.clone();
            }
        }
    }

    pub(super) fn replay_modal(&self) -> bool {
        self.replay.picker.is_open || self.replay.selecting
    }

    fn replay_toggle(&mut self) {
        if self.replay.controller.is_some() && self.replay.control.is_user_dismissed() {
            self.replay.control.show_control();
        } else if self.replay.controller.is_some() {
            self.replay_stop();
        } else {
            let first = self
                .chart()
                .bars(SeriesId::MAIN)
                .first()
                .map(|bar| bar.time);
            self.replay.picker.open(first);
        }
    }

    fn replay_choose_start(&mut self, action: ReplayStartAction) {
        let bars = self.chart().bars(SeriesId::MAIN);
        let index = match action {
            ReplayStartAction::SelectBar => {
                self.replay_stop();
                self.replay.selecting = true;
                self.replay.source = self.active;
                self.chart_mut().drawings_mut().set_active_tool(None);
                return;
            }
            ReplayStartAction::Resume => return self.replay_resume(),
            ReplayStartAction::Date(time) => bars.partition_point(|bar| bar.time < time),
            ReplayStartAction::Random => {
                use std::hash::BuildHasher;
                std::collections::hash_map::RandomState::new().hash_one(Utc::now()) as usize
                    % bars.len().max(1)
            }
            ReplayStartAction::First => 0,
            _ => return,
        };
        if index >= bars.len() {
            self.toast("No history on or after the selected date");
            return;
        }
        self.replay_start_at(index);
    }

    /// Starts the replay on the active slot with the cursor on bar `index`, paused.
    fn replay_start_at(&mut self, index: usize) {
        self.replay_stop();
        let source = self.active;
        let Some(interval) = self.charts[source].timeframe() else {
            return;
        };
        let bars = self.charts[source].bars(SeriesId::MAIN).to_vec();
        if bars.is_empty() {
            return;
        }
        let mut controller =
            ReplayController::new(bars, Vec::new()).with_symbol(self.charts[source].symbol());
        controller.handle_action(ReplayAction::EnterReplayMode);
        controller.handle_action(ReplayAction::JumpToBar(index));
        self.replay.source = source;
        self.replay_install(controller, interval);
    }

    /// Enters replay on the source slot with `controller`, which steps through bars of
    /// `interval`.
    fn replay_install(&mut self, controller: ReplayController, interval: Timeframe) {
        let source = self.replay.source;
        let chart = &self.charts[source];
        self.replay.base = chart.bars(SeriesId::MAIN).to_vec();
        self.replay.base_interval = chart.timeframe();
        self.replay.display = chart.timeframe();
        self.replay.interval = interval;
        self.replay.control.set_symbol(chart.symbol());
        self.replay.control.show_control();
        self.replay.controller = Some(controller);
        self.replay_active = true;
        self.replay_configure_intervals();
        self.replay_push();
    }

    fn replay_configure_intervals(&mut self) {
        let display = self.charts[self.replay.source]
            .timeframe()
            .unwrap_or_default();
        let mut intervals = vec![display];
        if display.duration_ms() > Timeframe::Min1.duration_ms() {
            intervals.push(Timeframe::Min1);
        }
        self.replay
            .control
            .set_update_intervals(self.replay.interval, intervals);
    }

    /// Keeps the position to resume with, while a replay runs.
    fn replay_checkpoint(&mut self) {
        if let Some(session) = self
            .replay
            .controller
            .as_ref()
            .and_then(|controller| controller.session())
        {
            self.replay.session = Some((session, self.replay.interval));
        }
    }

    fn replay_stop(&mut self) {
        self.replay_checkpoint();
        self.replay.picker.is_open = false;
        self.replay.selecting = false;
        self.replay.resume = None;
        self.replay.fetches.clear();
        if self.replay.controller.take().is_none() {
            return;
        }
        for slot in std::mem::take(&mut self.replay.slots) {
            if let Some(chart) = self.charts.get_mut(slot) {
                chart.set_replay_time(None);
            }
        }
        if let Some(chart) = self.charts.get_mut(self.replay.source) {
            chart.set_replay_time(None);
        }
        self.replay_active = false;
        self.replay.control.hide_control();
        self.replay_observe_live();
    }

    fn replay_transport(&mut self, action: ReplayControlAction) {
        if action == ReplayControlAction::Stop {
            return self.replay_stop();
        }
        let Some(controller) = self.replay.controller.as_mut() else {
            return;
        };
        match action {
            ReplayControlAction::Play => controller.handle_action(ReplayAction::Play),
            ReplayControlAction::Pause => controller.handle_action(ReplayAction::Pause),
            ReplayControlAction::StepForward | ReplayControlAction::NextBar => {
                controller.handle_action(ReplayAction::StepForward);
            }
            ReplayControlAction::StepBackward | ReplayControlAction::PreviousBar => {
                controller.handle_action(ReplayAction::StepBackward);
            }
            ReplayControlAction::GoToStart => controller.handle_action(ReplayAction::JumpToStart),
            ReplayControlAction::GoToEnd => controller.handle_action(ReplayAction::JumpToEnd),
            ReplayControlAction::SetSpeed(speed) => {
                controller.handle_action(ReplayAction::SetSpeed(speed));
            }
            ReplayControlAction::SetUpdateInterval(interval) => self.replay_set_interval(interval),
            ReplayControlAction::Close => self.replay.control.dismiss(),
            ReplayControlAction::None => return,
            _ => {}
        }
        self.replay_push();
    }

    /// Moves the cursor onto the bars of `interval`, keeping the replay time.
    fn replay_set_interval(&mut self, interval: Timeframe) {
        let source = self.replay.source;
        let Some(since) = self.charts[source].replay_time() else {
            return;
        };
        self.replay.fetches.clear();
        let slots = self.replay_group(source);
        let since = self.replay_since(since);
        self.replay_fetch(Intent::Seek, interval, &slots, since);
    }

    fn replay_resume(&mut self) {
        let Some((session, interval)) = self.replay.session.clone() else {
            return;
        };
        self.replay_stop();
        self.replay.source = self.active;
        if session.symbol == self.chart().symbol() {
            self.replay_restore(session, interval);
        } else {
            self.switch_symbol(&session.symbol);
            let now = self.ctx().input(|input| input.time);
            self.replay.resume = Some((session, interval, now));
        }
    }

    /// Starts the saved session once the chart has loaded the symbol it was saved with.
    fn replay_resume_when_loaded(&mut self, ctx: &egui::Context) {
        let source = self.replay.source;
        let loaded = self.chart_events.iter().any(|(slot, event)| {
            *slot == source
                && matches!(event, ChartEvent::DataLoaded { series, bars }
                    if *series == SeriesId::MAIN && *bars > 0)
        });
        if loaded && let Some((session, interval, _)) = self.replay.resume.take() {
            ctx.request_repaint();
            self.replay_restore(session, interval);
        }
    }

    /// Fetches the source bars `session` was saved with, then starts the replay on them.
    fn replay_restore(&mut self, session: ReplaySession, interval: Timeframe) {
        let slots = self.replay_group(self.replay.source);
        let since = self.replay_since(session.time);
        self.replay_fetch(Intent::Restore(session), interval, &slots, since);
    }

    /// The time the finer bars for a cursor at `time` start at: the open of the source slot's bar
    /// the cursor is in lies up to one of its intervals before.
    fn replay_since(&self, time: DateTime<Utc>) -> DateTime<Utc> {
        let interval = self.charts[self.replay.source]
            .timeframe()
            .unwrap_or_default();
        time - TimeDelta::milliseconds(interval.duration_ms())
    }

    /// The slots whose cutoff moves with `source`.
    fn replay_group(&self, source: usize) -> Vec<usize> {
        (0..self.charts.len().min(16))
            .filter(|&slot| self.layout.sync_group(slot) == self.layout.sync_group(source))
            .collect()
    }

    /// Asks the feed for the bars of `interval` that `slots` need from `since` on, and finishes
    /// at once when none needs any.
    fn replay_fetch(
        &mut self,
        intent: Intent,
        interval: Timeframe,
        slots: &[usize],
        since: DateTime<Utc>,
    ) {
        let source = self.replay.source;
        let jobs: Vec<_> = slots
            .iter()
            .copied()
            .filter(|&slot| {
                let own = if slot == source {
                    // A restored session may start before the bars the chart has loaded.
                    self.replay
                        .base_interval
                        .filter(|_| !matches!(intent, Intent::Restore(_)))
                } else {
                    self.charts[slot].timeframe()
                };
                own != Some(interval)
            })
            .map(|slot| {
                let chart = &self.charts[slot];
                Job {
                    slot,
                    symbol: chart.symbol().to_owned(),
                    clock: source_clock(chart, interval),
                    extended_hours: chart.extended_hours(),
                    bars: Vec::new(),
                    pending: None,
                    done: false,
                }
            })
            .collect();
        let mut fetch = Fetch {
            intent,
            interval,
            since,
            jobs,
        };
        if fetch.jobs.is_empty() {
            return self.replay_finish(fetch);
        }
        for index in 0..fetch.jobs.len() {
            self.replay_request(&mut fetch, index, None);
        }
        self.replay.fetches.push(fetch);
    }

    /// Asks for the page of bars of `fetch.jobs[index]` that open before `before`.
    fn replay_request(&mut self, fetch: &mut Fetch, index: usize, before: Option<DateTime<Utc>>) {
        let key = self.replay.next_key;
        self.replay.next_key += 1;
        let job = &mut fetch.jobs[index];
        job.pending = Some(key);
        let request = HistoryRequest {
            symbol: job.symbol.clone(),
            timeframe: fetch.interval,
            clock: job.clock.clone(),
            before,
            count: PAGE,
            extended_hours: job.extended_hours,
        };
        self.feed
            .history(request, self.replay.inbox.reply::<HistoryPage>(key));
    }

    /// Takes the pages the feed answered, asks for the next ones and finishes what is complete.
    fn replay_receive(&mut self) {
        for (key, answer) in self.replay.inbox.drain().answers {
            let Ok(answer) = answer.downcast::<Result<HistoryPage, FeedError>>() else {
                continue;
            };
            let Some(position) = self
                .replay
                .fetches
                .iter()
                .position(|fetch| fetch.jobs.iter().any(|job| job.pending == Some(key)))
            else {
                continue;
            };
            let mut fetch = self.replay.fetches.remove(position);
            let index = fetch
                .jobs
                .iter()
                .position(|job| job.pending == Some(key))
                .expect("the job was found above");
            let since = fetch.since;
            let job = &mut fetch.jobs[index];
            job.pending = None;
            match *answer {
                Ok(page) => {
                    let oldest = page.bars.iter().map(|bar| bar.time).min();
                    job.bars.extend(page.bars);
                    job.done = page.end_of_history
                        || job.bars.len() >= SOURCE_LIMIT
                        || oldest.is_none_or(|oldest| oldest <= since);
                    if !job.done {
                        self.replay_request(&mut fetch, index, oldest);
                    }
                }
                Err(error) => {
                    self.toast(format!("No source history for the replay: {error}"));
                    continue;
                }
            }
            if fetch.jobs.iter().all(|job| job.done) {
                self.replay_finish(fetch);
            } else {
                self.replay.fetches.insert(position, fetch);
            }
        }
    }

    /// Applies finer history that every slot has now received.
    fn replay_finish(&mut self, fetch: Fetch) {
        let source = self.replay.source;
        let Fetch {
            intent,
            interval,
            jobs,
            ..
        } = fetch;
        let mut sources: Vec<(usize, Vec<Bar>, BarClock)> = jobs
            .into_iter()
            .map(|job| {
                let mut bars = job.bars;
                bars.sort_by_key(|bar| bar.time);
                bars.dedup_by_key(|bar| bar.time);
                (job.slot, bars, job.clock)
            })
            .collect();
        if matches!(intent, Intent::Sources) {
            if interval == self.replay.interval {
                self.replay_apply(sources);
            }
            return;
        }
        let bars = match sources.iter().position(|(slot, ..)| *slot == source) {
            Some(index) => sources[index].1.clone(),
            None => self.replay.base.clone(),
        };
        if bars.is_empty() {
            self.toast("No source history for that update interval");
            return;
        }
        let symbol = self.charts[source].symbol().to_owned();
        let clock = source_clock(&self.charts[source], interval);
        let mut next = ReplayController::new(bars, Vec::new()).with_symbol(symbol);
        match intent {
            Intent::Restore(session) => {
                if !next.restore_session(&session) {
                    self.replay.session = Some((session, interval));
                    self.toast("Saved replay needs source history that is no longer available");
                    return;
                }
                self.replay.slots = self.replay_group(source);
                self.replay_install(next, interval);
            }
            _ => {
                if let Some(cutoff) = self.charts[source].replay_time()
                    && !next.seek_cutoff(cutoff, &clock)
                {
                    self.toast("No source history at the replay cursor");
                    return;
                }
                if let Some(old) = &self.replay.controller {
                    next.state.set_speed(old.state.speed);
                    if old.state.is_playing() {
                        next.handle_action(ReplayAction::Play);
                    }
                }
                self.replay.controller = Some(next);
                self.replay.interval = interval;
                self.replay_push();
                self.replay_configure_intervals();
            }
        }
        // A slot at the source interval needs no finer bars: clear the ones it had.
        for slot in self.replay.slots.clone() {
            if !sources.iter().any(|(job, ..)| *job == slot) {
                let clock = source_clock(&self.charts[slot], interval);
                sources.push((slot, Vec::new(), clock));
            }
        }
        self.replay_apply(sources);
    }

    fn replay_apply(&mut self, sources: Vec<(usize, Vec<Bar>, BarClock)>) {
        for (slot, bars, clock) in sources {
            // Slots whose interval does not nest in the source keep windowing their own bars.
            self.charts[slot].set_replay_source(bars, clock);
        }
    }

    /// Moves every slot of the source's group to the cutoff of the cursor and tells the brokers
    /// the market at the bars it revealed.
    fn replay_push(&mut self) {
        let Some(controller) = &self.replay.controller else {
            return;
        };
        let source = self.replay.source;
        let clock = source_clock(&self.charts[source], self.replay.interval);
        let Some(time) = controller.cutoff(&clock) else {
            return;
        };
        let previous = self.charts[source].replay_time();
        let symbol = self.charts[source].symbol().to_owned();
        let visible = controller.visible_bars();
        // A forward step reveals the bars after the previous cutoff, each of which a broker
        // sees in turn; a start or a rewind shows it where the cursor is now.
        let revealed = match previous {
            Some(previous) if previous < time => visible.partition_point(|bar| bar.time < previous),
            // The same cutoff at a coarser interval: its last bar opens earlier, but the replay
            // has not gone back.
            Some(previous)
                if previous == time
                    && visible.last().map(|bar| bar.time) < self.replay.observed =>
            {
                visible.len()
            }
            _ => visible.len().saturating_sub(1),
        };
        for bar in &visible[revealed..] {
            observe(&self.services.brokers, &symbol, bar.close, Some(bar.time));
            self.replay.observed = Some(bar.time);
        }
        let group = self.replay_group(source);
        for &slot in &self.replay.slots {
            if !group.contains(&slot)
                && let Some(chart) = self.charts.get_mut(slot)
            {
                chart.set_replay_time(None);
            }
        }
        let mut charts: Vec<&mut Chart> = self.charts.iter_mut().collect();
        self.layout.set_replay_time(&mut charts, source, Some(time));
        let joined: Vec<_> = group
            .iter()
            .copied()
            .filter(|slot| *slot != source && !self.replay.slots.contains(slot))
            .collect();
        self.replay.slots = group;
        self.replay_checkpoint();
        if !joined.is_empty() {
            let interval = self.replay.interval;
            let since = self.replay_since(time);
            self.replay_fetch(Intent::Sources, interval, &joined, since);
        }
    }

    /// Tells the brokers the last close of the active chart, outside replay.
    fn replay_observe_live(&self) {
        let chart = self.chart();
        // The bars of a symbol being switched to are not there until its history arrives.
        if chart
            .instrument()
            .is_none_or(|instrument| instrument.symbol != chart.symbol())
        {
            return;
        }
        if let Some(bar) = chart
            .bars(SeriesId::MAIN)
            .iter()
            .rfind(|bar| !bar.is_whitespace())
        {
            observe(&self.services.brokers, chart.symbol(), bar.close, None);
        }
    }

    /// The "click a bar" prompt, and the start on a click on a bar of the source slot.
    fn replay_select_bar(&mut self, ctx: &egui::Context, plot: egui::Rect) {
        let mut cancel = false;
        egui::Area::new(egui::Id::unique("egui_charts_terminal_replay_select_bar"))
            .fixed_pos(plot.min + egui::vec2(12.0, 36.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label("Click a bar to start replay");
                    cancel = crate::ui_kit::buttons::Button::new("Cancel")
                        .secondary()
                        .show(ui)
                        .clicked();
                });
            });
        let source = self.replay.source;
        if cancel
            || ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.replay.selecting = false;
            return;
        }
        let click = self
            .chart_events
            .iter()
            .find_map(|(slot, event)| match event {
                ChartEvent::Click { position, button }
                    if *slot == source
                        && *button == egui::PointerButton::Primary
                        && position.series == SeriesId::MAIN =>
                {
                    Some(position.time)
                }
                _ => None,
            });
        let Some(time) = click else {
            return;
        };
        ctx.request_repaint();
        let bars = self.charts[source].bars(SeriesId::MAIN);
        if bars.first().is_none_or(|bar| time < bar.time) || time > bars[bars.len() - 1].time {
            return;
        }
        let index = bars
            .partition_point(|bar| bar.time <= time)
            .saturating_sub(1);
        self.replay.selecting = false;
        self.replay_start_at(index);
    }
}

/// Tells every broker the market: `price` of `symbol` at `replay_time`, or live.
fn observe(
    brokers: &[(BrokerTile, Arc<dyn Broker>)],
    symbol: &str,
    price: f64,
    replay_time: Option<DateTime<Utc>>,
) {
    for (_, broker) in brokers {
        broker.observe(symbol, price, replay_time);
    }
}

/// The clock of bars of `interval` in the zone `chart` shows its time axis in.
fn source_clock(chart: &Chart, interval: Timeframe) -> BarClock {
    use crate::scales::TimezoneMode;
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
    use super::super::test_support::{MemoryStorage, SIZE, TestFeed, click, frame};
    use super::*;
    use crate::ChartBuilder;
    use crate::data::inbox::Inbox;
    use crate::model::{Execution, OrderRequest, OrderSide, OrderType};
    use crate::services::Listener;
    use crate::services::{AccountEvent, PaperAccount};
    use crate::widget::ChartLayoutMode;
    use chrono::TimeZone;

    fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
            .unwrap()
    }

    fn settle(terminal: &mut Terminal) {
        for _ in 0..4 {
            frame(terminal, SIZE, vec![]);
        }
    }

    fn run(terminal: &mut Terminal, action: Action) {
        assert!(terminal.run(action).is_none());
        settle(terminal);
    }

    fn transport(terminal: &mut Terminal, action: ReplayControlAction) {
        run(terminal, Action::Replay(action));
    }

    /// The paper account's fills and open positions, as the account panel would be told.
    fn account(paper: &PaperAccount) -> (Vec<Execution>, usize) {
        use crate::services::Broker;
        let inbox = Arc::new(Inbox::default());
        let (_listener, events) = Listener::new(&inbox, 2);
        paper.connect(events);
        let events: Vec<_> = inbox
            .drain()
            .answers
            .into_iter()
            .map(|(_, event)| *event.downcast::<AccountEvent>().unwrap())
            .collect();
        let fills = events
            .iter()
            .filter_map(|event| match event {
                AccountEvent::Execution(fill) => Some(fill.clone()),
                _ => None,
            })
            .collect();
        let positions = events
            .iter()
            .filter(|event| matches!(event, AccountEvent::Position(_)))
            .count();
        (fills, positions)
    }

    fn buy(paper: &PaperAccount, symbol: &str) {
        use crate::services::Broker;
        let inbox = Arc::new(Inbox::default());
        let mut order = OrderRequest::new(symbol, OrderSide::Buy, 1.0, OrderType::Market);
        order.time_in_force = crate::model::TimeInForce::GoodTillCancelled;
        paper.place(order, inbox.reply(0));
        assert!(inbox.drain().answers.pop().is_some());
    }

    /// The test feed has 500 daily bars from 2024-08-20 to 2026-01-01 (bar `i` opens
    /// `500 - i` days before 2026-01-02) with close `180 * (1 + 0.02 * sin(0.3 * i))`.
    #[test]
    fn a_clicked_bar_starts_the_replay_and_a_paper_account_fills_at_replay_closes() {
        let paper = Arc::new(PaperAccount::new("USD", 100_000.0));
        let mut terminal = Terminal::new(Arc::new(TestFeed))
            .with_symbol("AAPL")
            .with_broker(BrokerTile::new("paper", "Paper"), paper.clone());
        settle(&mut terminal);
        // A position held live, at the price the chart showed.
        buy(&paper, "AAPL");
        assert_eq!(account(&paper).1, 1);

        run(
            &mut terminal,
            Action::TopToolbar(TopToolbarAction::ToggleReplay),
        );
        assert!(terminal.replay_modal());
        run(
            &mut terminal,
            Action::ReplayStart(ReplayStartAction::SelectBar),
        );
        assert!(terminal.replay.selecting && terminal.replay_modal());

        // The bar four from the last: bar 496, which opens on 2025-12-29.
        let map = terminal.chart().get_chart_mapping();
        let shown = terminal.chart().state().shown().len();
        let pos = egui::pos2(map.idx_to_x(shown - 4), map.rect.center().y);
        click(&mut terminal, SIZE, pos);
        settle(&mut terminal);

        assert!(!terminal.replay.selecting);
        assert!(terminal.replay_active);
        let controller = terminal.replay.controller.as_ref().unwrap();
        assert_eq!(controller.state.curr_bar, 496);
        assert!(!controller.state.is_playing());
        assert_eq!(terminal.chart().replay_time(), Some(at(2025, 12, 30, 0, 0)));
        assert_eq!(terminal.chart().bars(SeriesId::MAIN).len(), 497);

        // The live position is gone and a new order fills at the bar's close.
        let (fills, positions) = account(&paper);
        assert_eq!((fills.len(), positions), (0, 0));
        buy(&paper, "AAPL");
        let (fills, positions) = account(&paper);
        assert_eq!(positions, 1);
        assert_eq!(fills[0].time, at(2025, 12, 29, 0, 0));
        assert!(
            (fills[0].price - 176.7212276).abs() < 1e-6,
            "{}",
            fills[0].price
        );

        // A step forward keeps the position and moves the price to the next close.
        transport(&mut terminal, ReplayControlAction::StepForward);
        assert_eq!(terminal.chart().replay_time(), Some(at(2025, 12, 31, 0, 0)));
        assert_eq!(account(&paper).1, 1);
        buy(&paper, "AAPL");
        let (fills, _) = account(&paper);
        assert_eq!(fills[1].time, at(2025, 12, 30, 0, 0));
        assert!(
            (fills[1].price - 176.4283814).abs() < 1e-6,
            "{}",
            fills[1].price
        );

        // Going back takes the future positions with it.
        transport(&mut terminal, ReplayControlAction::GoToStart);
        assert_eq!(account(&paper), (Vec::new(), 0));

        transport(&mut terminal, ReplayControlAction::Stop);
        assert!(!terminal.replay_active && terminal.chart().replay_time().is_none());
        assert_eq!(terminal.chart().bars(SeriesId::MAIN).len(), 500);
    }

    #[test]
    fn slots_share_one_cutoff_through_a_finer_source_and_a_resumed_session() {
        let storage = Arc::new(MemoryStorage::default());
        let two_slots = |symbol: &str| {
            let mut terminal = Terminal::new(Arc::new(TestFeed))
                .with_symbol(symbol)
                .with_storage(storage.clone());
            terminal.layout.mode = ChartLayoutMode::TwoHorizontal;
            terminal
        };
        let paper = Arc::new(PaperAccount::new("USD", 100_000.0));
        let mut terminal =
            two_slots("TEST").with_broker(BrokerTile::new("paper", "Paper"), paper.clone());
        settle(&mut terminal);
        let slot = ChartBuilder::new().with_feed(terminal.feed.clone()).build();
        terminal.charts.push(slot);
        terminal.wire_slot(1);
        terminal.charts[1].set_symbol("TEST");
        terminal.charts[1].set_timeframe(Timeframe::Day1);
        settle(&mut terminal);

        // Start on the last daily bar: the cutoff is the end of that day for every slot.
        let last = Action::ReplayStart(ReplayStartAction::Date(at(2026, 1, 1, 0, 0)));
        run(&mut terminal, last);
        let end = at(2026, 1, 2, 0, 0);
        assert_eq!(terminal.charts[0].replay_time(), Some(end));
        assert_eq!(terminal.charts[1].replay_time(), Some(end));

        // Minute bars fill the last day from the finer source. The feed holds 500 of them.
        transport(
            &mut terminal,
            ReplayControlAction::SetUpdateInterval(Timeframe::Min1),
        );
        assert_eq!(terminal.replay.interval, Timeframe::Min1);
        assert_eq!(
            terminal.replay.controller.as_ref().unwrap().state.curr_bar,
            499
        );
        assert_eq!(terminal.charts[0].replay_time(), Some(end));
        for chart in &terminal.charts {
            assert_eq!(chart.bars(SeriesId::MAIN).last().unwrap().volume, 649_400.0);
        }

        // Back at the daily interval on the same cutoff, nothing was rewound.
        buy(&paper, "TEST");
        let daily = ReplayControlAction::SetUpdateInterval(Timeframe::Day1);
        transport(&mut terminal, daily);
        assert_eq!(terminal.charts[0].replay_time(), Some(end));
        assert_eq!(account(&paper).1, 1, "the position is kept");
        transport(
            &mut terminal,
            ReplayControlAction::SetUpdateInterval(Timeframe::Min1),
        );

        // Two minutes back, the slots still share the cutoff.
        transport(&mut terminal, ReplayControlAction::StepBackward);
        transport(&mut terminal, ReplayControlAction::StepBackward);
        let cutoff = at(2026, 1, 1, 23, 58);
        assert_eq!(terminal.charts[0].replay_time(), Some(cutoff));
        assert_eq!(terminal.charts[1].replay_time(), Some(cutoff));
        assert_eq!(account(&paper).1, 0, "a rewind takes the position back");
        drop(terminal);

        // After a restart the session resumes paused, also from another symbol.
        let mut restored = two_slots("OTHER");
        settle(&mut restored);
        restored.chart_mut().set_symbol("OTHER");
        settle(&mut restored);
        let (session, interval) = restored.replay.session.clone().unwrap();
        assert_eq!(
            (session.symbol.as_str(), session.time),
            ("TEST", at(2026, 1, 1, 23, 57))
        );
        assert_eq!(interval, Timeframe::Min1);
        run(
            &mut restored,
            Action::ReplayStart(ReplayStartAction::Resume),
        );
        settle(&mut restored);
        assert_eq!(restored.chart().symbol(), "TEST");
        assert_eq!(restored.chart().replay_time(), Some(cutoff));
        let controller = restored.replay.controller.as_ref().unwrap();
        assert!(!controller.state.is_playing());
        assert_eq!(controller.state.curr_bar, 497);
    }
}
