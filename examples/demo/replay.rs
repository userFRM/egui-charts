//! Host replay wiring: source history, start selection, checkpoints and layout scope.

use super::{DemoApp, live::LiveFeed};
use chrono::{DateTime, Utc};
use egui_charts::data::{MarketEvent, RecordingWriter, ReplayDriver, ReplayEvent};
use egui_charts::model::{Bar, BarClock, Timeframe};
use egui_charts::ui::floating_replay_control::ReplayControlAction;
use egui_charts::ui::replay::{
    ReplayAction, ReplayController, ReplayScope, ReplayStartAction, ReplayToolbarAction,
    ReplayTradingChoice, paint_start_line,
};
use egui_charts::ui::strategy_report::StrategyReportAction;
use egui_charts::ui::timeframe_toolbar::SessionType;
use egui_charts::widget::{ChartLinkGroups, SeriesId};
use std::collections::VecDeque;
use std::num::NonZeroU64;
use std::sync::Arc;

/// Keep at most five minutes and 8 MiB of encoded events per symbol subscription, in chunks
/// beginning with a book snapshot. Checkpoint before the next execution after a minute or 1 MiB.
const CAPTURE_BYTES: usize = 8 * 1024 * 1024;

/// A replay move held while the replay trading prompt asks about the account.
pub(super) enum ReplayMove {
    /// A step back or a jump to the start from the replay bar.
    Control(ReplayControlAction),
    /// A move back or a finished replay's restart from the standalone toolbar.
    Toolbar(ReplayToolbarAction),
    /// A new start at this bar of [`DemoApp::replay_start_bars`].
    StartAt(usize),
    /// A new start at this time of the open recording.
    Recording(DateTime<Utc>),
}

pub(super) struct Capture {
    chunks: VecDeque<(DateTime<Utc>, RecordingWriter<Vec<u8>>)>,
    bytes: usize,
    sequence: u64,
    pub(super) detectors: Option<Arc<egui_charts::services::OrderFlowDetectors>>,
}

impl Capture {
    pub(super) fn new() -> Self {
        Self {
            chunks: VecDeque::new(),
            bytes: 0,
            sequence: 0,
            detectors: None,
        }
    }

    pub(super) fn needs_checkpoint(&self, time: DateTime<Utc>) -> bool {
        self.chunks.back().is_none_or(|(start, writer)| {
            time - *start >= chrono::TimeDelta::minutes(1) || writer.get_ref().len() >= 1024 * 1024
        })
    }

    pub(super) fn push(&mut self, symbol: &str, time: DateTime<Utc>, event: MarketEvent) {
        if let Some(detectors) = &self.detectors {
            detectors.push(symbol, time, &event);
        }
        self.record(symbol, time, event);
    }

    /// A retention checkpoint repeats known book state without resetting live detectors.
    pub(super) fn record(&mut self, symbol: &str, time: DateTime<Utc>, event: MarketEvent) {
        if matches!(
            event,
            MarketEvent::DepthSnapshot { .. } | MarketEvent::OrderSnapshot { .. }
        ) {
            let writer = RecordingWriter::new(Vec::new()).expect("memory recording");
            self.bytes += writer.get_ref().len();
            self.chunks.push_back((time, writer));
        }
        // Executions from before subscription have no recorded book to replay with them.
        let Some((_, writer)) = self.chunks.back_mut() else {
            return;
        };
        let before = writer.get_ref().len();
        writer
            .write(&ReplayEvent::new(symbol, time, self.sequence, event))
            .expect("generated market event");
        self.bytes += writer.get_ref().len() - before;
        self.sequence += 1;
        while self.chunks.len() > 1
            && (self.bytes > CAPTURE_BYTES
                || self
                    .chunks
                    .front()
                    .is_some_and(|(start, _)| time - *start > chrono::TimeDelta::minutes(5)))
        {
            self.bytes -= self.chunks.pop_front().unwrap().1.get_ref().len();
        }
    }

    pub(super) fn read_into(&self, events: &mut Vec<ReplayEvent>) {
        for (_, writer) in &self.chunks {
            let mut reader = egui_charts::data::RecordingReader::new(writer.get_ref().as_slice())
                .expect("memory recording");
            while let Some(event) = reader.read().expect("memory recording") {
                events.push(event);
            }
        }
    }
}

const MAX_RECORDING_BYTES: usize = CAPTURE_BYTES + 8192;
const MAX_RECORDING_EVENTS: usize = 200_000;
const RECORDING_MAGIC: &[u8; 8] = b"EGCDEMO1";

/// Local file delivery and bounded decoding stay separate from the current workspace.
#[derive(Default)]
pub(super) struct RecordingFiles {
    selected: Option<std::sync::mpsc::Receiver<Result<Option<Vec<u8>>, String>>>,
    parsing: Option<RecordingImport>,
    pub(super) opened: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RecordingInstrument {
    symbol: String,
    description: Option<String>,
    exchange: Option<String>,
    currency: Option<String>,
    unit: Option<String>,
    tick_size: f64,
    price_decimals: Option<u8>,
    #[serde(default)]
    time_zone: Option<egui_charts::model::TimezoneId>,
    #[serde(default)]
    session: Option<egui_charts::model::Session>,
}

impl RecordingInstrument {
    fn instrument(&self) -> egui_charts::model::Instrument {
        let mut instrument =
            egui_charts::model::Instrument::new(&self.symbol).with_tick_size(self.tick_size);
        instrument.description = self.description.clone();
        instrument.exchange = self.exchange.clone();
        instrument.currency = self.currency.clone();
        instrument.unit = self.unit.clone();
        instrument.price_decimals = self.price_decimals;
        instrument.time_zone = self.time_zone.clone();
        instrument.session = self.session.clone();
        instrument
    }
}

struct RecordingImport {
    instrument: RecordingInstrument,
    bytes: Vec<u8>,
    offset: usize,
    header: usize,
    events: Vec<ReplayEvent>,
}

struct LoadedRecording {
    driver: Arc<ReplayDriver>,
    instrument: egui_charts::model::Instrument,
    bars: Vec<Bar>,
}

impl RecordingImport {
    fn new(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() > MAX_RECORDING_BYTES {
            return Err("Recording exceeds the 8 MiB event limit".into());
        }
        if bytes.get(..8) != Some(RECORDING_MAGIC.as_slice()) {
            return Err("Choose a supported demo recording (.egcreplay)".into());
        }
        let length = bytes.get(8..12).ok_or("Truncated recording metadata")?;
        let length = u32::from_le_bytes(length.try_into().unwrap()) as usize;
        if length > 4096 {
            return Err("Recording metadata exceeds 4 KiB".into());
        }
        let header = 12 + length;
        let instrument: RecordingInstrument = serde_json::from_slice(
            bytes
                .get(12..header)
                .ok_or("Truncated recording metadata")?,
        )
        .map_err(|e| format!("Invalid recording metadata: {e}"))?;
        if instrument.symbol.is_empty()
            || !instrument.tick_size.is_finite()
            || instrument.tick_size <= 0.0
            || instrument.price_decimals.is_some_and(|n| n > 12)
        {
            return Err("Invalid recording instrument or tick size".into());
        }
        // Validate the binary version before any workspace state can change.
        egui_charts::data::RecordingReader::new(bytes.get(header..).ok_or("Missing event stream")?)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            instrument,
            bytes,
            offset: header + 8,
            header,
            events: Vec::new(),
        })
    }

    /// At most 512 records or 64 KiB per UI frame, plus one book snapshot capped at 1 MiB.
    fn read_batch(&mut self) -> Result<bool, String> {
        use std::io::Read;
        let start_offset = self.offset;
        for _ in 0..512 {
            if self.offset == self.bytes.len() {
                return Ok(true);
            }
            if self.events.len() == MAX_RECORDING_EVENTS {
                return Err("Recording exceeds 200000 events".into());
            }
            let start = self.offset;
            let length = self
                .bytes
                .get(start..start + 4)
                .ok_or("Truncated event length")?;
            let length = u32::from_le_bytes(length.try_into().unwrap()) as usize;
            if length > 1024 * 1024 {
                return Err("A recording event exceeds 1 MiB".into());
            }
            let end = start + 4 + length;
            let frame = self
                .bytes
                .get(start..end)
                .ok_or("Truncated recording event")?;
            let header = &self.bytes[self.header..self.header + 8];
            let mut reader = egui_charts::data::RecordingReader::new(header.chain(frame))
                .map_err(|e| e.to_string())?;
            let event = reader
                .read()
                .map_err(|e| e.to_string())?
                .ok_or("Missing recording event")?;
            if event.symbol != self.instrument.symbol {
                return Err("Recording contains a different instrument".into());
            }
            if self
                .events
                .last()
                .is_some_and(|last| (last.time, last.sequence) >= (event.time, event.sequence))
            {
                return Err("Recording events must be ordered by time and sequence".into());
            }
            self.events.push(event);
            self.offset = end;
            if self.offset - start_offset >= 64 * 1024 {
                break;
            }
        }
        Ok(self.offset == self.bytes.len())
    }

    fn finish(self) -> Result<LoadedRecording, String> {
        if !matches!(
            self.events.first().map(|e| &e.event),
            Some(MarketEvent::DepthSnapshot { .. } | MarketEvent::OrderSnapshot { .. })
        ) {
            return Err("Recording must begin with a book snapshot".into());
        }
        if self.events.last().is_some_and(|e| {
            e.time
                .checked_add_signed(chrono::TimeDelta::nanoseconds(1))
                .is_none()
        }) {
            return Err("Recording time is outside the playback range".into());
        }
        let instrument = self.instrument.instrument();
        let trades: Vec<_> = self
            .events
            .iter()
            .filter_map(|e| match &e.event {
                MarketEvent::Trade(trade) => Some(trade.clone()),
                _ => None,
            })
            .collect();
        if trades.is_empty() {
            return Err("Recording has no executions to replay".into());
        }
        let bars = egui_charts::data::aggregate_trades_with_tick_size(
            &trades,
            BarClock::new(Timeframe::Ticks(NonZeroU64::MIN)),
            instrument.tick_size,
        )
        .map_err(|e| e.to_string())?;
        let driver =
            ReplayDriver::new(self.events, vec![instrument.clone()]).map_err(|e| e.to_string())?;
        Ok(LoadedRecording {
            driver: Arc::new(driver),
            instrument,
            bars,
        })
    }
}

fn recording_bytes(
    instrument: &egui_charts::model::Instrument,
    events: &[ReplayEvent],
) -> Result<Vec<u8>, String> {
    if !events
        .iter()
        .any(|event| matches!(event.event, MarketEvent::Trade(_)))
    {
        return Err("No captured executions yet; wait for the feed to load".into());
    }
    let metadata = RecordingInstrument {
        symbol: instrument.symbol.clone(),
        description: instrument.description.clone(),
        exchange: instrument.exchange.clone(),
        currency: instrument.currency.clone(),
        unit: instrument.unit.clone(),
        tick_size: instrument
            .tick_size
            .ok_or("Recording requires an instrument tick size")?,
        price_decimals: instrument.price_decimals,
        time_zone: instrument.time_zone.clone(),
        session: instrument.session.clone(),
    };
    let metadata = serde_json::to_vec(&metadata).map_err(|e| e.to_string())?;
    if metadata.len() > 4096 {
        return Err("Recording metadata exceeds 4 KiB".into());
    }
    let mut bytes = RECORDING_MAGIC.to_vec();
    bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&metadata);
    let mut writer = RecordingWriter::new(bytes).map_err(|e| e.to_string())?;
    for event in events {
        writer.write(event).map_err(|e| e.to_string())?;
    }
    let bytes = writer.finish().map_err(|e| e.to_string())?;
    if bytes.len() > MAX_RECORDING_BYTES {
        return Err("Recording exceeds the 8 MiB event limit".into());
    }
    Ok(bytes)
}

impl DemoApp {
    pub(super) fn show_recording_controls(&mut self, ui: &mut egui::Ui) {
        use egui_charts::icons::icons;
        ui.label("Local .egcreplay file: instrument and recorded events");
        if super::commands::menu_item(ui, "Download captured recording", &icons::DATABASE, None)
            .clicked()
        {
            let mut events = Vec::new();
            self.live.read_recording(&mut events);
            match recording_bytes(&self.live.instrument(), &events) {
                Ok(bytes) => self.write_export(None, "egcreplay", &bytes),
                Err(error) => self.notify(&error),
            }
            ui.close();
        }
        let busy =
            self.recording_files.selected.is_some() || self.recording_files.parsing.is_some();
        ui.add_enabled_ui(!busy, |ui| {
            if super::commands::menu_item(ui, "Open local recording", &icons::FOLDER_OPEN, None)
                .clicked()
            {
                self.pick_recording(ui.ctx());
                ui.close();
            }
        });
        if busy {
            ui.label("Reading recording…");
        }
    }

    fn pick_recording(&mut self, ctx: &egui::Context) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.recording_files.selected = Some(rx);
        let ctx = ctx.clone();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            use std::io::Read;
            let result = (|| {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Market recording", &["egcreplay"])
                    .pick_file()
                else {
                    return Ok(None);
                };
                let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
                if file.metadata().map_err(|e| e.to_string())?.len() > MAX_RECORDING_BYTES as u64 {
                    return Err("Recording exceeds the 8 MiB event limit".into());
                }
                let mut bytes = Vec::new();
                file.take(MAX_RECORDING_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                Ok(Some(bytes))
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let result = if let Some(file) = rfd::AsyncFileDialog::new()
                .add_filter("Market recording", &["egcreplay"])
                .pick_file()
                .await
            {
                if file.inner().size() > MAX_RECORDING_BYTES as f64 {
                    Err("Recording exceeds the 8 MiB event limit".into())
                } else {
                    Ok(Some(file.read().await))
                }
            } else {
                Ok(None)
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    pub(super) fn poll_recording(&mut self, ctx: &egui::Context) {
        if let Some(rx) = self.recording_files.selected.take() {
            match rx.try_recv() {
                Ok(Ok(Some(bytes))) => match RecordingImport::new(bytes) {
                    Ok(import) => self.recording_files.parsing = Some(import),
                    Err(error) => self.notify(&error),
                },
                Ok(Ok(None)) => {}
                Ok(Err(error)) => self.notify(&error),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.recording_files.selected = Some(rx)
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.notify("Recording selection failed")
                }
            }
        }
        let Some(mut import) = self.recording_files.parsing.take() else {
            return;
        };
        match import.read_batch() {
            Ok(false) => {
                self.recording_files.parsing = Some(import);
                ctx.request_repaint();
            }
            Ok(true) => match import
                .finish()
                .and_then(|loaded| self.open_recording(loaded))
            {
                Ok(()) => {}
                Err(error) => self.notify(&error),
            },
            Err(error) => self.notify(&error),
        }
    }

    fn open_recording(&mut self, loaded: LoadedRecording) -> Result<(), String> {
        // A file picked before own data was entered can arrive after it.
        if self.synthetic_only("Replay") {
            return Ok(());
        }
        if loaded.instrument.symbol != self.chart.symbol() {
            return Err(format!(
                "Open the {} chart before importing this recording",
                loaded.instrument.symbol
            ));
        }
        self.stop_replay();
        self.live_was_paused = self.live.paused();
        self.live.set_paused(true);
        // Starts chosen on the replay bar pick from the recording; Exit replay restores the history.
        self.bars = loaded.bars.clone();
        self.state.replay_active = true;
        self.replay_control.set_symbol(&self.state.symbol);
        self.replay_control.show_control();
        let mut replay = ReplayController::new(loaded.bars.clone(), Vec::new())
            .with_symbol(self.state.symbol.clone());
        replay.handle_action(ReplayAction::EnterReplayMode);
        self.replay = Some(replay);
        self.open_replay_account();
        self.replay_interval = Timeframe::Ticks(NonZeroU64::MIN);
        self.recording_files.opened = true;
        loaded.driver.seek(loaded.bars[0].time);
        self.chart.set_conversion(None);
        self.chart.set_market_replay(Some(loaded.driver));
        self.chart.set_instrument(Some(loaded.instrument));
        self.chart.set_signal_source(None);
        self.chart
            .set_replay_source(loaded.bars, source_clock(&self.chart, self.replay_interval));
        self.configure_replay_intervals();
        self.push_market_replay_window();
        Ok(())
    }
}

/// Group membership, detached slots and synchronization switches kept with a demo layout.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct LayoutGroups {
    groups: [Option<String>; 16],
    #[serde(default)]
    links: [ChartLinkGroups; 16],
    detached: [bool; 16],
    sync: [bool; 6],
    #[serde(default)]
    cents: [bool; 16],
    /// Whether each chart shows the continuous 24/7 session, which the chart's extended hours
    /// choice cannot tell from the extended session. Layouts saved without it have `None`.
    #[serde(default)]
    continuous: Option<[bool; 16]>,
    #[serde(default)]
    pub(super) sizes: Vec<egui_charts::widget::ChartLayoutSizing>,
}

impl DemoApp {
    pub(super) fn show_price_unit_controls(&mut self, ui: &mut egui::Ui) {
        use egui_charts::icons::icons;
        ui.label("Synthetic prices: 1 USD = 100 US cents");
        ui.add_enabled_ui(self.replay.is_none(), |ui| {
            for (label, cents) in [("USD", false), ("US cents", true)] {
                let selected = self.chart.instrument().is_some_and(|instrument| {
                    if cents {
                        instrument.unit.as_deref() == Some("US cents")
                    } else {
                        instrument.currency.as_deref() == Some("USD") && instrument.unit.is_none()
                    }
                });
                if super::commands::menu_item(
                    ui,
                    label,
                    &icons::TEXT,
                    selected.then_some(&icons::CHECK),
                )
                .clicked()
                {
                    self.user_action(|app| {
                        app.chart.set_conversion(conversion(
                            cents,
                            app.timeframe_toolbar.state.adjust_for_dividends,
                        ));
                        app.regenerate_bars();
                        app.time_and_sales.clear();
                    });
                    ui.close();
                }
            }
        })
        .response
        .on_disabled_hover_text("Exit replay before changing its price unit");
    }

    pub(super) fn layout_groups(&self) -> LayoutGroups {
        let sync = &self.layout.sync;
        let live_in_slot = |slot: usize| {
            if slot == self.layout_slot {
                Some(&self.live)
            } else {
                self.extra_charts
                    .get(if slot < self.layout_slot {
                        slot
                    } else {
                        slot - 1
                    })
                    .map(|extra| &extra.live)
            }
        };
        LayoutGroups {
            sizes: self.layout.saved_sizes(),
            links: self.layout.saved_link_groups(),
            groups: std::array::from_fn(|slot| self.layout.sync_group(slot).map(str::to_owned)),
            detached: std::array::from_fn(|slot| self.layout.is_detached(slot)),
            cents: std::array::from_fn(|slot| {
                live_in_slot(slot)
                    .is_some_and(|live| live.instrument().unit.as_deref() == Some("US cents"))
            }),
            continuous: Some(std::array::from_fn(|slot| {
                live_in_slot(slot).is_some_and(|live| live.view(false).0 == SessionType::Full24x7)
            })),
            sync: [
                sync.sync_symbol,
                sync.sync_timeframe,
                sync.sync_crosshair,
                sync.sync_time_axis,
                sync.sync_date_range,
                sync.sync_drawings,
            ],
        }
    }

    pub(super) fn restore_layout_groups(&mut self, groups: Option<LayoutGroups>) {
        // A copy drops a divider drag in progress, which would overwrite the restored sizes.
        self.layout = self.layout.clone();
        self.layout.reset_sizes(true);
        if let Some(groups) = &groups
            && let Err(error) = self.layout.set_sizes(&groups.sizes, true)
        {
            self.toasts
                .error(format!("Could not open chart sizes: {error}"));
        }
        let cents = groups.as_ref().map_or([false; 16], |groups| groups.cents);
        let continuous = groups.as_ref().and_then(|groups| groups.continuous);
        for (slot, cents) in cents.into_iter().enumerate() {
            let (chart, live) = if slot == self.layout_slot {
                (&mut self.chart, &self.live)
            } else if let Some(extra) = self.extra_charts.get_mut(if slot < self.layout_slot {
                slot
            } else {
                slot - 1
            }) {
                (&mut extra.chart, &extra.live)
            } else {
                continue;
            };
            live.set_price_unit(cents);
            let adjusted = chart.conversion().is_some_and(|c| c.ends_with("adjusted"));
            chart.set_conversion(conversion(cents, adjusted));
            let extended = chart.extended_hours();
            // Layouts without the record hold the 24/7 session as extended hours.
            let session = if continuous.map_or(extended, |continuous| continuous[slot]) {
                SessionType::Full24x7
            } else if extended {
                SessionType::ETH
            } else {
                SessionType::RTH
            };
            if live.view(extended).0 != session {
                live.set_view(session, adjusted);
                chart.reload_feed();
                chart.set_instrument(Some(live.instrument()));
            }
        }
        let Some(groups) = groups else {
            for slot in 0..16 {
                self.layout.set_sync_group(slot, None);
                self.layout
                    .set_link_groups(slot, ChartLinkGroups::default(), true);
                self.layout.set_detached(slot, false);
            }
            self.layout.sync = Default::default();
            self.top_toolbar.chart_grid_selector.sync = self.layout.sync.clone();
            return;
        };
        for slot in 0..16 {
            self.layout
                .set_sync_group(slot, groups.groups[slot].clone());
            self.layout
                .set_link_groups(slot, groups.links[slot].clone(), true);
            self.layout.set_detached(slot, groups.detached[slot]);
        }
        let sync = &mut self.layout.sync;
        [
            sync.sync_symbol,
            sync.sync_timeframe,
            sync.sync_crosshair,
            sync.sync_time_axis,
            sync.sync_date_range,
            sync.sync_drawings,
        ] = groups.sync;
        self.top_toolbar.chart_grid_selector.sync = sync.clone();
    }

    pub(super) fn advance_replay(&mut self) {
        let Some(replay) = self.replay.as_mut() else {
            return;
        };
        let mut playback_time = None;
        if let Some(driver) = self.chart.replay_driver().cloned() {
            // Keep live host analytics out of recorded market playback.
            self.chart.set_signal_source(None);
            for extra in &mut self.extra_charts {
                if extra.chart.replay_driver().is_some() {
                    extra.chart.set_signal_source(None);
                }
            }
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
                self.notify(error.to_string());
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
            self.apply_replay_window(time);
        } else {
            self.push_replay_window_to_chart();
        }
    }

    pub(super) fn toggle_replay(&mut self) {
        if self.synthetic_only("Replay") {
            return;
        }
        if self.replay.is_some() && self.replay_control.is_user_dismissed() {
            self.replay_control.show_control();
        } else if self.replay.is_some() || self.replay_selecting {
            self.stop_replay();
        } else {
            self.select_replay_start();
        }
    }

    /// Opens the replay bar and waits for a click on a bar, keeping any running replay paused
    /// until the click restarts it.
    fn select_replay_start(&mut self) {
        self.handle_replay_control(ReplayControlAction::Pause);
        self.replay_selecting = true;
        self.state.replay_active = true;
        self.chart.drawings_mut().set_active_tool(None);
        self.chart.drain_events().for_each(drop);
        if self.replay.is_none() {
            self.replay_interval = self.state.timeframe;
            self.configure_replay_intervals();
        }
        self.replay_control.set_symbol(&self.state.symbol);
        self.replay_control.show_control();
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
        replay.handle_action(ReplayAction::SetSpeed(self.replay_control.state.speed));
        self.replay = Some(replay);
        self.open_replay_account();
        self.state.replay_active = true;
        self.replay_control.set_symbol(&self.state.symbol);
        self.replay_control.show_control();
        self.configure_replay_intervals();
        self.push_replay_window_to_chart();
    }

    /// Starts the replay session's account from the settings of the replay bar's start menu,
    /// in place of the one before, and connects it where the live account was connected.
    fn open_replay_account(&mut self) {
        use egui_charts::services::ReplayAccountSettings;
        // The start menu keeps its settings valid; the default covers a value it let through.
        let account = self
            .replay_control
            .replay_account()
            .account()
            .or_else(|| ReplayAccountSettings::default().account())
            .expect("the default replay account is valid");
        let account = Arc::new(account);
        if self.trading.has_broker() {
            self.trading.set_service(account.clone());
        }
        self.replay_paper = Some(account);
    }

    /// Opens the results of the replay session when its account holds a trade.
    fn open_replay_results(&mut self) {
        let (Some(account), Some(replay)) = (&self.replay_paper, &self.replay) else {
            return;
        };
        let symbol = replay.state.symbol.clone().unwrap_or_default();
        let report = account.report("Replay trading", symbol, self.replay_interval);
        if !report.trades.is_empty() {
            self.replay_results.open(report, replay.visible_bars());
        }
    }

    /// Holds `held` and opens the replay trading prompt when the move would leave orders or
    /// trades of the replay account behind: a move back to a bar at `target` before the
    /// account's last change, or with `None` a new start while the account has any history.
    pub(super) fn hold_replay_move(
        &mut self,
        held: ReplayMove,
        target: Option<DateTime<Utc>>,
    ) -> bool {
        let Some(last) = self
            .replay_paper
            .as_ref()
            .and_then(|account| account.last_change())
        else {
            return false;
        };
        if target.is_some_and(|target| target >= last) {
            return false;
        }
        // The cursor stays where it was until the answer.
        self.handle_replay_control(ReplayControlAction::Pause);
        self.replay_move = Some(held);
        self.replay_prompt.is_open = true;
        true
    }

    /// The replay trading prompt and the results of a session, drawn after the replay bar.
    pub(super) fn show_replay_trading(&mut self, ctx: &egui::Context) {
        match self.replay_prompt.show(ctx) {
            ReplayTradingChoice::Stay => self.replay_move = None,
            choice @ (ReplayTradingChoice::NewAccount | ReplayTradingChoice::EndSession) => {
                if choice == ReplayTradingChoice::EndSession {
                    self.open_replay_results();
                }
                // Either answer leaves the old account behind, so the held move passes.
                self.open_replay_account();
                match self.replay_move.take() {
                    Some(ReplayMove::Control(action)) => self.handle_replay_control(action),
                    Some(ReplayMove::Toolbar(action)) => self.handle_replay_toolbar(action),
                    Some(ReplayMove::StartAt(index)) => self.start_replay_at(index),
                    Some(ReplayMove::Recording(time)) => self.seek_recording(time),
                    None => {}
                }
                ctx.request_repaint();
            }
            _ => {}
        }
        match self.replay_results.show(ctx) {
            StrategyReportAction::ExportCsv(csv) => self.save_text("replay-trades.csv", &csv),
            StrategyReportAction::ExportSummaryCsv { filename, content } => {
                self.save_text(&filename, &content);
            }
            _ => {}
        }
    }

    pub(super) fn configure_replay_intervals(&mut self) {
        let intervals = self.replay_update_intervals();
        self.replay_control
            .set_update_intervals(self.replay_interval, intervals);
    }

    /// The update intervals the replay bar offers, from coarse to fine: the chart interval, one
    /// day, minute, second and one trade where they apply, and always the active one.
    fn replay_update_intervals(&self) -> Vec<Timeframe> {
        if self.recording_files.opened {
            return vec![self.replay_interval];
        }
        let mut intervals = vec![self.state.timeframe];
        if matches!(
            self.state.timeframe,
            Timeframe::Week1 | Timeframe::Month1 | Timeframe::Weeks(_) | Timeframe::Months(_)
        ) {
            intervals.push(Timeframe::Day1);
        }
        if self.state.timeframe.duration_ms() > Timeframe::Min1.duration_ms() {
            intervals.push(Timeframe::Min1);
        }
        if (Timeframe::Sec1.duration_ms() + 1..Timeframe::Day1.duration_ms())
            .contains(&self.state.timeframe.duration_ms())
        {
            intervals.push(Timeframe::Sec1);
        }
        // The feed holds its recent trades, so replay can also advance one trade at a time.
        if !self.state.timeframe.is_trade_based() {
            intervals.push(Timeframe::Ticks(NonZeroU64::MIN));
        }
        if !intervals.contains(&self.replay_interval) {
            // An interval kept from another chart interval. A trade-based one goes last.
            let step = self.replay_interval.duration_ms();
            let at = intervals
                .iter()
                .position(|listed| step > 0 && listed.duration_ms() < step)
                .unwrap_or(intervals.len());
            intervals.insert(at, self.replay_interval);
        }
        intervals
    }

    /// Chooses a common time step for every replayed chart. Calendar charts use daily updates;
    /// fixed intervals use their greatest common divisor, never an approximate month length.
    fn automatic_replay_interval(&self) -> Timeframe {
        let mut interval = self.state.timeframe;
        for other in self
            .extra_charts
            .iter()
            .filter(|_| self.replay_scope == ReplayScope::AllCharts)
            .filter_map(|extra| extra.chart.timeframe())
        {
            interval = common_replay_interval(interval, other);
        }
        if matches!(
            interval,
            Timeframe::Week1 | Timeframe::Month1 | Timeframe::Weeks(_) | Timeframe::Months(_)
        ) {
            Timeframe::Day1
        } else {
            interval
        }
    }

    pub(super) fn set_automatic_replay_interval(&mut self) {
        if self.recording_files.opened {
            return;
        }
        let interval = self.automatic_replay_interval();
        self.set_replay_interval(interval);
        if self.replay_interval == interval {
            self.replay_control.set_automatic_update_interval(true);
        }
    }

    /// Carries the replay over a change of the chart interval. An update interval that is finer
    /// than the new chart interval and divides it stays, with its source rebuilt for the new
    /// chart; any other is replaced by the chart interval. Without history of the new interval
    /// at the cursor, replay moves to the first bar of that history and says so.
    pub(super) fn follow_replay_timeframe(&mut self) {
        let timeframe = self.state.timeframe;
        if self.replay.is_none() {
            // The bar may be open to pick a start, which begins at the chart interval.
            self.replay_interval = timeframe;
        }
        if self.replay_control.uses_automatic_update_interval() && !self.recording_files.opened {
            self.replay_interval = self.automatic_replay_interval();
        }
        // A recording and a trade-by-trade replay play their executions on any chart interval.
        let candles = !self.recording_files.opened && self.chart.replay_driver().is_none();
        if let (true, Some(replay), Some(cutoff)) =
            (candles, self.replay.as_ref(), self.chart.replay_time())
        {
            let (speed, playing) = (replay.state.speed, replay.state.is_playing());
            if self.bars.is_empty() {
                self.stop_replay();
                self.notify(format!("No {timeframe} history: replay stopped"));
                return;
            }
            let symbol = self.state.symbol.clone();
            let mut step = None;
            if divides(self.replay_interval, timeframe) {
                let (bars, clock) = replay_source(&self.live, self.replay_interval, &self.chart);
                let mut next = ReplayController::new(bars.clone(), Vec::new()).with_symbol(&symbol);
                if next.seek_cutoff(cutoff, &clock) && self.chart.set_replay_source(bars, clock) {
                    step = Some((self.replay_interval, next));
                }
            }
            let (interval, mut next) = if let Some(step) = step {
                step
            } else {
                let clock = self.chart.bar_clock();
                // The chart's own bars need no source, and one of the old interval must go.
                self.chart.set_replay_source(Vec::new(), clock.clone());
                let mut next =
                    ReplayController::new(self.bars.clone(), Vec::new()).with_symbol(symbol);
                if !next.seek_cutoff(cutoff, &clock) {
                    next.handle_action(ReplayAction::JumpToStart);
                    self.notify(format!(
                        "No {timeframe} history at the replay cursor: replay moved to the first {timeframe} bar"
                    ));
                }
                (timeframe, next)
            };
            next.state.set_speed(speed);
            if playing {
                next.handle_action(ReplayAction::Play);
            }
            self.replay = Some(next);
            self.replay_interval = interval;
            for (index, extra) in self.extra_charts.iter_mut().enumerate() {
                let slot = if index >= self.layout_slot {
                    index + 1
                } else {
                    index
                };
                if self.replay_slots.contains(&slot) {
                    let (bars, clock) = replay_source(&extra.live, interval, &extra.chart);
                    extra.chart.set_replay_source(bars, clock);
                }
            }
        }
        self.configure_replay_intervals();
        self.push_replay_window_to_chart();
    }

    pub(super) fn checkpoint_replay(&mut self) {
        if self.recording_files.opened {
            return;
        }
        if let Some(session) = self.replay.as_ref().and_then(|replay| replay.session()) {
            self.replay_session = Some((session, self.replay_interval));
        }
    }

    pub(super) fn stop_replay(&mut self) {
        self.checkpoint_replay();
        self.replay_start.is_open = false;
        self.replay_selecting = false;
        self.open_replay_results();
        self.replay_prompt.is_open = false;
        self.replay_move = None;
        if self.replay.take().is_some() {
            let market = self.chart.replay_driver().is_some();
            // The live account comes back as it was left.
            if self.replay_paper.take().is_some() && self.trading.has_broker() {
                self.trading.set_service(self.paper.clone());
            }
            self.chart.set_replay_time(None);
            restore_conversion(&mut self.chart, &self.live);
            for extra in &mut self.extra_charts {
                extra.chart.set_replay_time(None);
                restore_conversion(&mut extra.chart, &extra.live);
            }
            self.replay_updated = None;
            self.replay_applied = None;
            self.replay_slots.clear();
            self.chart.set_feed_paused(false);
            self.live.set_paused(self.live_was_paused);
            if self.recording_files.opened {
                self.bars = self.chart.bars(SeriesId::MAIN).to_vec();
            }
            self.recording_files.opened = false;
            if market {
                self.refresh_strategy_signals();
            }
        }
        self.state.replay_active = false;
        self.replay_control.hide_control();
    }

    pub(super) fn push_market_replay_window(&mut self) {
        if let Some(driver) = self.chart.replay_driver() {
            if let Some(first) = self
                .replay
                .as_ref()
                .and_then(|replay| replay.visible_bars().first())
                && driver.time() < first.time
            {
                driver.seek(first.time);
            }
            self.apply_replay_window(driver.time() + chrono::TimeDelta::nanoseconds(1));
        }
    }

    /// Applies the cursor to the charts and the replay account. A move back that would leave
    /// orders or trades of the account behind is held by [`Self::hold_replay_move`] first.
    pub(super) fn push_replay_window_to_chart(&mut self) {
        let Some(replay) = self.replay.as_ref() else {
            return;
        };
        let clock = source_clock(&self.chart, self.replay_interval);
        let Some(time) = replay.cutoff(&clock) else {
            return;
        };
        self.apply_replay_window(time);
    }

    fn apply_replay_window(&mut self, time: DateTime<Utc>) {
        let paper = self.active_paper().clone();
        let Some(replay) = self.replay.as_ref() else {
            return;
        };
        let previous = self.replay_applied.replace(time);
        // The account keeps its clock at a bar before its last change, which only a change of
        // the update interval reaches: the cursor itself has not moved back.
        if let Some(previous) = previous.filter(|previous| *previous < time) {
            for bar in replay
                .visible_bars()
                .iter()
                .filter(|bar| bar.time >= previous)
            {
                paper.set_replay_time(Some(bar.time));
                paper.set_price(self.chart.symbol(), bar.close);
            }
        }
        let source = self.layout_slot;
        // A recording stays on its own chart, independently of the layout replay choice.
        let scope = if self.recording_files.opened {
            ReplayScope::ThisChart
        } else {
            self.replay_scope
        };
        let slots: Vec<_> = (0..self.extra_charts.len() + 1)
            .filter(|&slot| scope == ReplayScope::AllCharts || slot == source)
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
            .map(|extra| &mut *extra.chart)
            .collect();
        charts.insert(source, &mut self.chart);
        self.layout
            .set_replay_time(&mut charts, source, scope, Some(time));
        self.replay_slots = slots;
        if let Some(bar) = replay.visible_bars().last() {
            paper.set_replay_time(Some(bar.time));
            paper.set_price(self.chart.symbol(), bar.close);
        }
        if let Some(replay) = &mut self.replay {
            replay.state.scope = scope;
        }
        self.checkpoint_replay();
    }

    pub(super) fn set_replay_interval(&mut self, interval: Timeframe) {
        if self.replay.is_none() || self.recording_files.opened {
            return;
        }
        let (mut bars, clock) = if interval.is_trade_based() {
            (Vec::new(), source_clock(&self.chart, interval))
        } else {
            replay_source(&self.live, interval, &self.chart)
        };
        if bars.is_empty() && !interval.is_trade_based() {
            self.notify("No source history for that update interval");
            return;
        }
        let market_driver = if interval.is_trade_based() {
            let instrument = self.live.instrument();
            let mut merged = Vec::new();
            self.live.read_recording(&mut merged);
            let trades: Vec<_> = merged
                .iter()
                .filter_map(|event| match &event.event {
                    MarketEvent::Trade(trade) => Some(trade.clone()),
                    _ => None,
                })
                .collect();
            bars = egui_charts::data::aggregate_trades_with_tick_size(
                &trades,
                clock.clone(),
                instrument.tick_size,
            )
            .expect("recorded demo executions");
            if let Some(driver) = self.chart.replay_driver().cloned() {
                Some(driver)
            } else {
                let mut instruments = vec![instrument];
                for (index, extra) in self.extra_charts.iter().enumerate() {
                    let slot = if index >= self.layout_slot {
                        index + 1
                    } else {
                        index
                    };
                    if !self.replay_slots.contains(&slot)
                        || extra.live.instrument().unit != instruments[0].unit
                        || instruments
                            .iter()
                            .any(|instrument| instrument.symbol == extra.chart.symbol())
                    {
                        continue;
                    }
                    instruments.push(extra.live.instrument());
                    extra.live.read_recording(&mut merged);
                }
                let driver =
                    Arc::new(ReplayDriver::new(merged, instruments).expect("captured market data"));
                Some(driver)
            }
        } else {
            None
        };
        let mut cutoff = self.chart.replay_time();
        if let Some(driver) = &market_driver {
            let Some((from, to)) = driver.time_range() else {
                self.notify("No recorded market events available yet");
                return;
            };
            bars.retain(|bar| bar.time >= from && bar.time <= to);
            if bars.is_empty() {
                self.notify("No recorded executions available yet");
                return;
            }
            let after = chrono::TimeDelta::nanoseconds(1);
            cutoff = cutoff.map(|time| time.clamp(bars[0].time + after, to + after));
            if self.chart.replay_driver().is_none() {
                self.notify(format!(
                    "Recorded market replay: {} to {} UTC",
                    from.format("%H:%M:%S"),
                    to.format("%H:%M:%S")
                ));
            }
        }
        if !interval.is_trade_based() && self.chart.replay_driver().is_some() {
            self.chart.set_market_replay(None);
            restore_conversion(&mut self.chart, &self.live);
            self.chart.set_replay_time(cutoff);
            for extra in &mut self.extra_charts {
                if extra.chart.replay_driver().is_some() {
                    extra.chart.set_market_replay(None);
                    restore_conversion(&mut extra.chart, &extra.live);
                    extra.chart.set_replay_time(cutoff);
                }
            }
            self.refresh_strategy_signals();
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
        self.replay_control.set_automatic_update_interval(false);
        self.push_replay_window_to_chart();
        self.chart.set_replay_source(bars, clock);
        if let Some(driver) = market_driver {
            if let Some(time) = self.chart.replay_time() {
                driver.seek(time - chrono::TimeDelta::nanoseconds(1));
            }
            if self.chart.replay_driver().is_none() {
                self.chart.set_conversion(None);
                self.chart.set_market_replay(Some(driver));
                self.chart.set_signal_source(None);
            }
        }
        let driver = self.chart.replay_driver().cloned();
        let unit = self.live.instrument().unit;
        for (index, extra) in self.extra_charts.iter_mut().enumerate() {
            let slot = if index >= self.layout_slot {
                index + 1
            } else {
                index
            };
            if self.replay_slots.contains(&slot) {
                let (bars, clock) = replay_source(&extra.live, interval, &extra.chart);
                extra.chart.set_replay_source(bars, clock);
                if let Some(driver) = &driver
                    && extra.live.instrument().unit == unit
                {
                    extra.chart.set_conversion(None);
                    extra.chart.set_market_replay(Some(driver.clone()));
                    extra.chart.set_signal_source(None);
                }
            }
        }
        self.configure_replay_intervals();
    }

    pub(super) fn show_replay_start(&mut self, ctx: &egui::Context) {
        let dialog = self.replay_start.is_open;
        let action = self.replay_start.show(ctx);
        if action != ReplayStartAction::None {
            ctx.request_repaint();
        }
        self.apply_replay_start(action);
        if !self.replay_selecting || dialog {
            return;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.cancel_replay_selection();
            ctx.request_repaint();
        } else if !egui::Popup::is_any_open(ctx)
            && let Some(pointer) = ctx.pointer_hover_pos()
            && let Some(index) = self.replay_start_bar(pointer)
        {
            let plot = self.chart.get_rendered_price_rect();
            let x = self.chart.get_chart_mapping().idx_to_x(index);
            let layer = egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::NULL.with("replay_start_line"),
            );
            paint_start_line(
                &ctx.layer_painter(layer).with_clip_rect(plot),
                plot,
                egui::pos2(x, pointer.y),
            );
        }
    }

    /// The shown bar the start line marks and a click starts at, for a pointer anywhere in the
    /// rendered price pane, its volume band included: the bar under it, or the last bar after it.
    pub(super) fn replay_start_bar(&self, pointer: egui::Pos2) -> Option<usize> {
        let shown = self.chart.state().shown().len();
        (shown > 0 && self.chart.get_rendered_price_rect().contains(pointer)).then(|| {
            let index = self.chart.get_chart_mapping().x_to_idx(pointer.x);
            index.min(shown - 1)
        })
    }

    fn cancel_replay_selection(&mut self) {
        if self.replay.is_some() {
            self.replay_selecting = false;
        } else {
            self.stop_replay();
        }
    }

    /// Carries out a choice of the replay bar's start control or the date dialog. A running replay
    /// restarts from it, keeping its update interval.
    pub(super) fn apply_replay_start(&mut self, action: ReplayStartAction) {
        match action {
            ReplayStartAction::None => {}
            ReplayStartAction::SelectBar if self.replay_selecting => {
                self.cancel_replay_selection();
            }
            ReplayStartAction::SelectBar => self.select_replay_start(),
            ReplayStartAction::SelectDate => {
                let first = self.replay_start_bars().first().map(|bar| bar.time);
                self.replay_start.open_for_chart(&self.chart, first);
            }
            ReplayStartAction::Resume => {
                if let Some((session, interval)) = self.replay_session.clone() {
                    self.replay_selecting = false;
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
                        self.cancel_replay_selection();
                        self.notify(
                            "Saved replay needs source history that is no longer available",
                        );
                        return;
                    }
                    self.replay_scope = replay.state.scope;
                    self.start_replay();
                    self.replay = Some(replay);
                    self.replay_interval = interval;
                    self.push_replay_window_to_chart();
                    self.set_replay_interval(interval);
                }
            }
            choice => {
                let bars = self.replay_start_bars();
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
        self.replay_selecting = false;
        // The click picked a bar, not the series under it.
        self.chart.clear_selection();
        if self.recording_files.opened {
            // Its bars are executions: the clicked bar ends at its last one, or a quiet bar at its
            // open.
            // shortcut: a tick bar opens at its first execution, so a click on one goes there;
            // count the bar's executions if tick charts need it whole.
            let clock = self.chart.bar_clock();
            let end = self
                .bars
                .partition_point(|bar| clock.bar_open(bar.time) <= time);
            let last = self.bars[..end]
                .last()
                .map_or(time, |bar| bar.time.max(time));
            return self.seek_recording(last);
        }
        // A trade-by-trade replay shows only the recorded bars, so the time finds the bar.
        let index = self
            .replay_start_bars()
            .partition_point(|bar| bar.time <= time)
            .saturating_sub(1);
        self.start_replay_at(index);
    }

    /// The bars a replay start picks from: a running replay hides the history after its cursor
    /// from the chart.
    fn replay_start_bars(&self) -> &[Bar] {
        if self.replay.is_some() {
            &self.bars
        } else {
            self.chart.bars(SeriesId::MAIN)
        }
    }

    /// Starts replay at bar `index` of [`Self::replay_start_bars`], as picking that bar does,
    /// ending a replay already running but keeping its update interval. An open recording stays
    /// open and moves to that execution.
    pub(crate) fn start_replay_at(&mut self, index: usize) {
        if self.recording_files.opened {
            if let Some(time) = self.bars.get(index).map(|bar| bar.time) {
                self.seek_recording(time);
            }
            return;
        }
        if self.hold_replay_move(ReplayMove::StartAt(index), None) {
            return;
        }
        let automatic = self.replay_control.uses_automatic_update_interval();
        let interval = self.replay.is_some().then_some(self.replay_interval);
        self.stop_replay();
        self.start_replay();
        if let Some(replay) = &mut self.replay {
            replay.handle_action(ReplayAction::JumpToBar(index));
        }
        self.push_replay_window_to_chart();
        if automatic {
            self.set_automatic_replay_interval();
        } else if let Some(interval) = interval {
            self.set_replay_interval(interval);
        }
    }

    /// Restarts the open recording paused at `time`, kept within its executions. As any new
    /// start, it asks first while the replay account has history.
    fn seek_recording(&mut self, time: DateTime<Utc>) {
        if self.hold_replay_move(ReplayMove::Recording(time), None) {
            return;
        }
        let clock = source_clock(&self.chart, self.replay_interval);
        let (Some(driver), Some(replay), Some(first)) = (
            self.chart.replay_driver().cloned(),
            self.replay.as_mut(),
            self.bars.first(),
        ) else {
            return;
        };
        let Some((_, last)) = driver.time_range() else {
            return;
        };
        let time = time.clamp(first.time, last);
        driver.seek(time);
        replay.handle_action(ReplayAction::Pause);
        replay.seek_cutoff(time + chrono::TimeDelta::nanoseconds(1), &clock);
        self.replay_start.is_open = false;
        self.replay_selecting = false;
        self.replay_applied = None;
        self.replay_control.show_control();
        self.push_market_replay_window();
    }
}

fn restore_conversion(chart: &mut egui_charts::Chart, live: &LiveFeed) {
    chart.set_conversion(conversion(
        live.instrument().unit.as_deref() == Some("US cents"),
        live.view(chart.extended_hours()).1,
    ));
}

/// The existing adjustment and the price unit travel together through the host feed contract.
pub(super) fn conversion(cents: bool, adjusted: bool) -> Option<String> {
    match (cents, adjusted) {
        (false, false) => None,
        (false, true) => Some("adjusted".into()),
        (true, false) => Some("cents".into()),
        (true, true) => Some("cents-adjusted".into()),
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

/// A fixed common step of two chart intervals, capped at a day for calendar charts.
fn common_replay_interval(a: Timeframe, b: Timeframe) -> Timeframe {
    if a.is_trade_based() || b.is_trade_based() {
        return Timeframe::Ticks(NonZeroU64::MIN);
    }
    let normalize = |interval| match interval {
        Timeframe::Week1 | Timeframe::Month1 | Timeframe::Weeks(_) | Timeframe::Months(_) => {
            Timeframe::Day1
        }
        _ => interval,
    };
    let (a, b) = (normalize(a), normalize(b));
    let mut left = a.duration_ms();
    let mut right = b.duration_ms();
    while right != 0 {
        (left, right) = (right, left % right);
    }
    // Keep the source interval's identity when it is already the common step.
    if left == a.duration_ms() {
        return a;
    }
    if left == b.duration_ms() {
        return b;
    }
    // A fixed duration must not become a calendar week or month of the same nominal length.
    Timeframe::all()
        .into_iter()
        .find(|interval| interval.duration_ms() == left && left < Timeframe::Day1.duration_ms())
        .unwrap_or_else(|| {
            if left % 1000 == 0 {
                Timeframe::Custom(NonZeroU64::new((left / 1000) as u64).unwrap())
            } else {
                Timeframe::Ticks(NonZeroU64::MIN)
            }
        })
}

/// Whether replay can step by `update` on a chart of `chart` bars: a finer time interval, of a
/// day at most, that divides it. Trade-based intervals have no duration and never do.
fn divides(update: Timeframe, chart: Timeframe) -> bool {
    let (step, bar) = (update.duration_ms(), chart.duration_ms());
    (1..=Timeframe::Day1.duration_ms()).contains(&step) && step < bar && bar % step == 0
}

fn source_clock(chart: &egui_charts::Chart, interval: Timeframe) -> BarClock {
    use egui_charts::scales::TimezoneMode;
    // The chart's own bars close where its replay completes them, which the session decides.
    if chart.timeframe() == Some(interval) {
        return chart.bar_clock();
    }
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

pub(super) fn book_snapshot(
    market: &super::market::Market,
    orders: Option<&super::orders::Orders>,
    time: DateTime<Utc>,
) -> MarketEvent {
    match orders {
        Some(orders) => {
            let (sequence, orders) = orders.snapshot();
            MarketEvent::OrderSnapshot { sequence, orders }
        }
        None => {
            let (sequence, book) = market.snapshot(time);
            MarketEvent::DepthSnapshot { sequence, book }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_charts::model::{Bar, OrderSide};
    use egui_charts::ui::floating_replay_control::ReplayControlAction;

    fn replay_test_ui(app: &mut DemoApp, ui: &mut egui::Ui) {
        app.show_top_toolbar(ui, app.theme.ui.panel_bg);
        app.reserve_replay_strip(ui, app.theme.ui.panel_bg);
        app.show_replay_control(ui.ctx());
        app.show_replay_trading(ui.ctx());
    }

    fn replay_ui_texts(app: &mut DemoApp, ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts = super::super::tests::ui_frame(ctx, egui::vec2(1440.0, 900.0), vec![], |ui| {
                replay_test_ui(app, ui);
            });
        }
        texts
    }

    fn click_replay_ui(app: &mut DemoApp, ctx: &egui::Context, label: &str) {
        let shown = replay_ui_texts(app, ctx);
        let size = egui::vec2(1440.0, 900.0);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| replay_test_ui(app, ui),
        );
        let point = shown
            .iter()
            .find(|(text, _)| text == label)
            .map(|(_, rect)| rect.center())
            .or_else(|| {
                output
                    .platform_output
                    .accesskit_update
                    .as_ref()?
                    .nodes
                    .iter()
                    .find_map(|(_, node)| {
                        if node.label() != Some(label) {
                            return None;
                        }
                        let rect = node.bounds()?;
                        Some(egui::pos2(
                            (rect.x0 + rect.x1) as f32 / 2.0,
                            (rect.y0 + rect.y1) as f32 / 2.0,
                        ))
                    })
            })
            .unwrap_or_else(|| panic!("missing replay control: {label}"));
        output.drop_without_applying_deltas();
        super::super::tests::ui_click(ctx, size, point, |ui| replay_test_ui(app, ui));
    }

    /// Local files must round-trip their price basis and book events, and reject bad input before replacement.
    #[test]
    fn local_recordings_roundtrip_and_invalid_imports_keep_the_workspace() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        super::super::tests::settle(&mut app, &ctx);
        app.live.set_paused(true);
        app.chart.set_conversion(Some("cents".into()));
        app.regenerate_bars();
        super::super::tests::settle(&mut app, &ctx);
        app.live.set_paused(false);
        app.live
            .advance(app.live.now() + chrono::TimeDelta::seconds(20));
        app.live.set_paused(true);
        let mut events = Vec::new();
        app.live.read_recording(&mut events);
        assert!(
            events
                .iter()
                .any(|e| matches!(e.event, MarketEvent::Quote(_)))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e.event, MarketEvent::Trade(_)))
        );
        let end = events.last().unwrap().time;
        let sequence = events.last().unwrap().sequence;
        let quote = events
            .iter()
            .find_map(|event| match &event.event {
                MarketEvent::Quote(quote) => Some(quote.clone()),
                _ => None,
            })
            .unwrap();
        // Supplied quotes and executions exercise batching and stepping even in a quiet session.
        for i in 1..=600 {
            let mut quote = quote.clone();
            quote.ts = end + chrono::TimeDelta::milliseconds(i);
            events.push(ReplayEvent::new(
                &app.state.symbol,
                quote.ts,
                sequence + i as u64,
                if i % 100 == 0 {
                    MarketEvent::Trade(egui_charts::model::Tick::new(
                        quote.ts,
                        quote.ask,
                        1.0,
                        Some(OrderSide::Buy),
                    ))
                } else {
                    MarketEvent::Quote(quote)
                },
            ));
        }
        let first_price = events
            .iter()
            .find_map(|event| match &event.event {
                MarketEvent::Trade(trade) => Some(trade.price),
                _ => None,
            })
            .unwrap();
        let bytes = recording_bytes(&app.live.instrument(), &events).unwrap();
        let mut import = RecordingImport::new(bytes.clone()).unwrap();
        let mut batches = 1;
        while !import.read_batch().unwrap() {
            batches += 1;
        }
        assert!(batches > 1, "parsing yields between bounded batches");
        assert_eq!(import.events, events);
        let loaded = import.finish().unwrap();
        assert_eq!(loaded.instrument.unit.as_deref(), Some("US cents"));
        assert_eq!(loaded.instrument.tick_size, app.live.instrument().tick_size);
        assert_eq!(loaded.instrument.session, app.live.instrument().session);
        assert_eq!(loaded.instrument.time_zone, app.live.instrument().time_zone);
        let first = loaded.bars[0].time;
        let last = loaded.bars.last().unwrap().time;
        app.chart.set_conversion(None);
        app.regenerate_bars();
        super::super::tests::settle(&mut app, &ctx);
        let (tx, rx) = std::sync::mpsc::channel();
        app.recording_files.selected = Some(rx);
        tx.send(Ok(Some(bytes.clone()))).unwrap();
        for _ in 0..16 {
            super::super::tests::settle(&mut app, &ctx);
            if app.chart.replay_driver().is_some() {
                break;
            }
        }
        let driver = app.chart.replay_driver().unwrap().clone();
        assert_eq!(
            app.chart.instrument().unwrap().unit.as_deref(),
            Some("US cents")
        );
        assert!(
            !app.layout_groups().cents[app.layout_slot],
            "an imported price basis must not change the saved live workspace"
        );
        assert_eq!(driver.time(), first);
        assert_eq!(
            app.chart.bars(SeriesId::MAIN).last().map(|bar| bar.close),
            Some(first_price),
            "imported executions must reach the chart at its cursor"
        );
        app.handle_replay_control(ReplayControlAction::StepForward);
        assert!(driver.time() > first);
        app.handle_replay_control(ReplayControlAction::Play);
        assert!(driver.speed() > 0.0);
        app.handle_replay_control(ReplayControlAction::Pause);
        assert_eq!(driver.speed(), 0.0);
        app.handle_replay_control(ReplayControlAction::GoToEnd);
        assert!(driver.time() >= last);
        app.handle_replay_control(ReplayControlAction::GoToStart);
        assert_eq!(driver.time(), first);
        super::super::tests::settle(&mut app, &ctx);
        let bars = app.chart.bars(SeriesId::MAIN).to_vec();
        let range = driver.time_range();
        let mut oversized_frame = bytes.clone();
        let mut checked = RecordingImport::new(bytes.clone()).unwrap();
        oversized_frame[checked.offset..checked.offset + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        let mut unsupported = bytes.clone();
        unsupported[0] = b'?';
        for invalid in [
            unsupported,
            oversized_frame,
            bytes[..bytes.len() - 1].to_vec(),
            vec![0; MAX_RECORDING_BYTES + 1],
        ] {
            let result = RecordingImport::new(invalid.clone()).and_then(|mut import| {
                while !import.read_batch()? {}
                import.finish()
            });
            assert!(result.is_err());
            let (tx, rx) = std::sync::mpsc::channel();
            app.recording_files.selected = Some(rx);
            tx.send(Ok(Some(invalid))).unwrap();
            for _ in 0..16 {
                super::super::tests::settle(&mut app, &ctx);
                if app.recording_files.selected.is_none() && app.recording_files.parsing.is_none() {
                    break;
                }
            }
            assert_eq!(app.chart.bars(SeriesId::MAIN), bars);
            assert!(Arc::ptr_eq(app.chart.replay_driver().unwrap(), &driver));
            assert_eq!(driver.time_range(), range);
        }
        while !checked.read_batch().unwrap() {}
        let mut other = checked.finish().unwrap();
        other.instrument.symbol = "OTHER".into();
        assert!(app.open_recording(other).unwrap_err().contains("OTHER"));
        assert!(Arc::ptr_eq(app.chart.replay_driver().unwrap(), &driver));
        app.stop_replay();
        super::super::tests::settle(&mut app, &ctx);
        assert!(app.chart.replay_driver().is_none());
        assert_eq!(
            app.chart.instrument().unwrap().currency.as_deref(),
            Some("USD")
        );
        assert!(app.chart.instrument().unwrap().unit.is_none());
        assert!(!app.chart.bars(SeriesId::MAIN).is_empty());
    }

    /// A fixed unit conversion must reach source prices, feed metadata, replay and saved layouts.
    #[test]
    fn cents_prices_survive_streaming_replay_and_layout_restore() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-10-27T23:59:59Z".parse().unwrap(),
            true,
        );
        super::super::tests::settle(&mut app, &ctx);
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::TwoHorizontal);
        super::super::tests::settle(&mut app, &ctx);
        app.live.set_paused(true);
        let display_interval = app.state.timeframe;
        let raw = app.live.snapshot(
            app.state.timeframe,
            app.chart.bar_clock(),
            app.chart.extended_hours(),
        );
        let raw_tick = app.live.instrument().tick_size.unwrap();
        let raw_quote = app.live.quote(app.live.now()).unwrap();
        app.chart.set_conversion(Some("cents".into()));
        app.regenerate_bars();
        super::super::tests::settle(&mut app, &ctx);
        assert_eq!(app.chart.conversion(), Some("cents"));
        assert_eq!(
            app.chart.instrument().unwrap().unit.as_deref(),
            Some("US cents")
        );
        assert!((app.live.instrument().tick_size.unwrap() - raw_tick * 100.0).abs() < 1e-9);
        for (raw, cents) in raw.iter().zip(app.chart.bars(SeriesId::MAIN)) {
            assert_eq!((raw.time, raw.volume), (cents.time, cents.volume));
            for (usd, cents) in [raw.open, raw.high, raw.low, raw.close].into_iter().zip([
                cents.open,
                cents.high,
                cents.low,
                cents.close,
            ]) {
                assert!((usd * 100.0 - cents).abs() < 1e-7);
            }
        }
        assert!(
            (app.live.quote(app.live.now()).unwrap().last - raw_quote.last * 100.0).abs() < 1e-7
        );
        let saved = app.capture_layout().unwrap();
        app.storage
            .save_layout_groups("Unit sample", &app.layout_groups())
            .unwrap();
        app.chart.set_conversion(None);
        app.regenerate_bars();
        assert!(app.restore_layout("Unit sample".into(), saved, Ok(Vec::new())));
        app.regenerate_bars();
        super::super::tests::settle(&mut app, &ctx);
        app.advance_sample_to(app.live.now() + chrono::TimeDelta::minutes(2));
        super::super::tests::settle(&mut app, &ctx);
        let mut events = Vec::new();
        app.live.read_recording(&mut events);
        assert!(
            events.iter().any(
                |e| matches!(&e.event, MarketEvent::Trade(t) if t.price > raw_quote.last * 50.0)
            )
        );
        app.start_replay();
        app.set_replay_interval(Timeframe::Ticks(NonZeroU64::MIN));
        super::super::tests::settle(&mut app, &ctx);
        assert!(app.chart.replay_driver().is_some());
        assert!(
            !app.chart.bars(SeriesId::MAIN).is_empty(),
            "converted executions must appear during market replay"
        );
        assert!(app.extra_charts[0].chart.replay_driver().is_none());
        assert_eq!(
            app.extra_charts[0]
                .chart
                .instrument()
                .unwrap()
                .currency
                .as_deref(),
            Some("USD")
        );
        assert_eq!(
            app.chart.instrument().unwrap().unit.as_deref(),
            Some("US cents")
        );
        app.switch_timeframe(Timeframe::Min5);
        super::super::tests::settle(&mut app, &ctx);
        assert_eq!(
            app.chart.instrument().unwrap().unit.as_deref(),
            Some("US cents")
        );
        assert_eq!(app.live.instrument().unit.as_deref(), Some("US cents"));
        assert!(app.chart.replay_driver().is_some());
        app.switch_timeframe(display_interval);
        app.stop_replay();
        app.chart.set_conversion(None);
        app.regenerate_bars();
        super::super::tests::settle(&mut app, &ctx);
        assert_eq!(
            app.chart.instrument().unwrap().currency.as_deref(),
            Some("USD")
        );
        let restored = &app.chart.bars(SeriesId::MAIN)[0];
        // Streaming can evict the oldest minutes, so compare the same historical bar by time.
        let original = raw
            .iter()
            .find(|bar| bar.time == restored.time)
            .expect("the restored historical bar must exist in the original snapshot");
        assert!((restored.close - original.close).abs() < 1e-7);
    }

    /// Runs one frame of `draw` at 1440 by 900 and returns the texts it paints with their rects.
    fn painted(
        app: &mut DemoApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        draw: fn(&mut DemoApp, &mut egui::Ui),
    ) -> Vec<(String, egui::Rect)> {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| draw(app, ui),
        );
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                )),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        texts
    }

    /// Clicks the painted text `label`, such as a button's, and returns the next frame's texts.
    /// A dialog paints nothing in its first frame, so the second is read.
    fn click_text(
        app: &mut DemoApp,
        ctx: &egui::Context,
        label: &str,
        draw: fn(&mut DemoApp, &mut egui::Ui),
    ) -> Vec<(String, egui::Rect)> {
        painted(app, ctx, vec![], draw);
        let texts = painted(app, ctx, vec![], draw);
        let pos = texts
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("{label} is not painted"))
            .1
            .center();
        painted(app, ctx, vec![egui::Event::PointerMoved(pos)], draw);
        for pressed in [true, false] {
            let click = egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            painted(app, ctx, vec![click], draw);
        }
        painted(app, ctx, vec![], draw)
    }

    /// A whole demo frame.
    fn demo(app: &mut DemoApp, ui: &mut egui::Ui) {
        eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
    }

    /// The demo paused on three one-minute bars closing at 100, 80 and 110, with the paper
    /// account connected, its panel open and market orders sent without the ticket.
    fn paper_demo(ctx: &egui::Context) -> (Box<DemoApp>, String) {
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-10-27T12:05:14Z".parse().unwrap(),
            true,
        );
        super::super::tests::settle(&mut app, ctx);
        app.live.set_paused(true);
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
        app.connect_paper();
        app.trading.display.instant_orders = true;
        (app, symbol)
    }

    /// Three whole frames, which also load the trading rules of a newly connected account, so
    /// that the next market order is placed at once. Returns the texts of the last.
    fn frames(app: &mut DemoApp, ctx: &egui::Context) -> Vec<String> {
        for _ in 0..2 {
            painted(app, ctx, vec![], demo);
        }
        let texts = painted(app, ctx, vec![], demo);
        texts.into_iter().map(|(text, _)| text).collect()
    }

    fn market_order(app: &mut DemoApp, ctx: &egui::Context, symbol: &str, side: OrderSide) {
        app.trading.trade(symbol.to_owned(), side);
        app.trading.poll(ctx, &mut app.toasts);
    }

    fn cursor(app: &DemoApp) -> usize {
        app.replay.as_ref().expect("replay is on").state.curr_bar
    }

    /// A step back to a bar before the replay account's last fill waits for the answer of the
    /// replay trading prompt: the cursor and the position stay until then, Escape drops the
    /// step, and a new account takes it. A step back that passes no fill needs no answer.
    #[test]
    fn a_replay_step_back_past_a_fill_asks_before_the_account_changes() {
        let has = |texts: &[String], text: &str| texts.iter().any(|painted| painted == text);
        let ctx = egui::Context::default();
        let (mut app, symbol) = paper_demo(&ctx);
        app.start_replay();
        app.handle_replay_control(ReplayControlAction::NextBar);
        frames(&mut app, &ctx);
        market_order(&mut app, &ctx, &symbol, OrderSide::Buy);
        assert!(has(&frames(&mut app, &ctx), "Long"), "bought on bar 1");

        // Forward and back to the bar of the fill: nothing is left behind.
        app.handle_replay_control(ReplayControlAction::NextBar);
        app.handle_replay_control(ReplayControlAction::PreviousBar);
        let texts = frames(&mut app, &ctx);
        assert_eq!(cursor(&app), 1);
        assert!(!has(&texts, "Trade from here"), "no fill is passed");
        assert!(has(&texts, "Long"), "the position stays");

        // Back past the fill.
        app.handle_replay_control(ReplayControlAction::PreviousBar);
        let texts = frames(&mut app, &ctx);
        assert!(
            has(&texts, "Trade from here") && has(&texts, "End session"),
            "the prompt asks first"
        );
        assert_eq!(cursor(&app), 1, "the cursor waits for the answer");
        assert!(has(&texts, "Long"), "the account waits for the answer");

        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        painted(&mut app, &ctx, vec![escape], demo);
        let texts = frames(&mut app, &ctx);
        assert!(!has(&texts, "Trade from here"), "Escape closes the prompt");
        assert_eq!(cursor(&app), 1, "Escape drops the step");
        assert!(has(&texts, "Long"), "Escape keeps the account");

        app.handle_replay_control(ReplayControlAction::PreviousBar);
        click_text(&mut app, &ctx, "Trade from here", demo);
        let texts = frames(&mut app, &ctx);
        assert!(!has(&texts, "Trade from here"));
        assert_eq!(cursor(&app), 0, "the held step is taken");
        assert!(
            has(&texts, "No open positions"),
            "a new account trades from the earlier bar"
        );
    }

    /// Leaving replay, and ending the session from the prompt, show the results of the replay
    /// account with its closed trade: bought at 100 and sold at 110 with 0.1% commission, the
    /// trade nets 10 - 0.10 - 0.11 = 9.79. Ending the session from the prompt keeps replay on
    /// and takes the held step.
    #[test]
    fn ending_a_replay_session_shows_its_results() {
        for from_prompt in [false, true] {
            let ctx = egui::Context::default();
            let (mut app, symbol) = paper_demo(&ctx);
            let settings =
                egui_charts::services::ReplayAccountSettings::new("EUR", 50_000.0, 0.001);
            assert!(app.replay_control.set_replay_account(settings));
            app.start_replay();
            frames(&mut app, &ctx);
            market_order(&mut app, &ctx, &symbol, OrderSide::Buy);
            app.handle_replay_control(ReplayControlAction::GoToEnd);
            market_order(&mut app, &ctx, &symbol, OrderSide::Sell);
            if from_prompt {
                app.handle_replay_control(ReplayControlAction::PreviousBar);
                click_text(&mut app, &ctx, "End session", demo);
                assert_eq!(cursor(&app), 1, "the held step is taken");
            } else {
                app.handle_replay_control(ReplayControlAction::Stop);
                assert!(app.replay.is_none());
            }
            let texts = frames(&mut app, &ctx);
            assert!(
                texts.iter().any(|text| text == "Replay trading results"),
                "the results open (from the prompt: {from_prompt})"
            );
            click_text(&mut app, &ctx, "List of trades", demo);
            let texts = frames(&mut app, &ctx);
            let row = [
                "Long",
                "1",
                "2024-01-01 00:00:00",
                "100",
                "2024-01-01 00:02:00",
                "110",
                "9.79 EUR",
                "9.79 EUR", // Cumulative profit; the fees column now starts beyond the frame.
            ];
            assert!(
                texts.windows(row.len()).any(|texts| texts == row),
                "the list holds the trade (from the prompt: {from_prompt}): {texts:?}"
            );
        }
    }

    #[test]
    fn replay_trades_its_own_account_and_keeps_the_live_one() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-10-27T12:05:14Z".parse().unwrap(),
            true,
        );
        super::super::tests::settle(&mut app, &ctx);
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
        app.api_samples = true;
        app.refresh_strategy_signals();
        let signals = |app: &mut DemoApp| {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                app.chart.show(ui);
            });
            let shown = output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Signals")
            });
            output.drop_without_applying_deltas();
            shown
        };
        assert!(signals(&mut app), "live host analytics are available");
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
        let live = app.paper.save();
        let settings = egui_charts::services::ReplayAccountSettings::new("EUR", 50_000.0, 0.001);
        assert!(app.replay_control.set_replay_account(settings));
        app.start_replay();
        assert!(
            empty(&mut app),
            "the replay account starts without the live position"
        );
        position(&mut app);
        let panel: fn(&mut DemoApp, &mut egui::Ui) = |app, ui| {
            let symbol = app.chart.symbol().to_owned();
            app.trading.show_account_panel(ui, &symbol);
        };
        // The balance is the capital less the commission: 50,000 - 100 x 0.001.
        let balance = |app: &mut DemoApp| {
            painted(app, &ctx, vec![], panel);
            let texts = painted(app, &ctx, vec![], panel);
            let at = texts
                .iter()
                .position(|(text, _)| text == "Account balance")
                .expect("the panel shows the balance");
            assert!(
                texts.iter().any(|(text, _)| text == "EUR"),
                "the replay account is in EUR"
            );
            texts[at - 1].0.clone()
        };
        assert_eq!(balance(&mut app), "49,999.90");
        app.handle_replay_control(ReplayControlAction::GoToEnd);
        app.trading.trade(symbol.clone(), OrderSide::Sell);
        app.trading.poll(&ctx, &mut app.toasts);
        // Sold at 110: 49,999.90 + 10 - 110 x 0.001.
        assert_eq!(balance(&mut app), "50,009.79");
        app.stop_replay();
        assert_eq!(app.paper.save(), live, "replay left the live account alone");
        assert!(!empty(&mut app), "the live position is shown again");

        // One-trade updates move the cutoff back only to just after the last trade: no fill is undone.
        for _ in 0..4 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                app.chart.show(ui);
            })
            .drop_without_applying_deltas();
        }
        super::super::tests::finish_feed(&app.live);
        // Quiet periods can outlast two seconds; capture executions before selecting trade updates.
        app.advance_sample_to(app.live.now() + chrono::TimeDelta::seconds(10));
        let mut events = Vec::new();
        app.live.read_recording(&mut events);
        assert!(
            events
                .iter()
                .any(|event| matches!(event.event, MarketEvent::Trade(_))),
            "trade replay requires captured executions"
        );
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
            !signals(&mut app),
            "unrecorded live analytics cannot appear over recorded executions"
        );
        app.refresh_strategy_signals();
        app.advance_replay();
        assert!(
            !signals(&mut app),
            "refreshing a report cannot restore live analytics during market replay"
        );
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
        app.handle_replay_control(ReplayControlAction::GoToStart);
        let first = app.replay.as_ref().unwrap().visible_bars()[0].time;
        assert_eq!(
            driver.time(),
            first,
            "the transport starts at the first recorded execution"
        );
        app.handle_replay_control(ReplayControlAction::SetUpdateInterval(app.state.timeframe));
        assert!(signals(&mut app), "candle replay restores host analytics");
        app.handle_replay_control(ReplayControlAction::SetUpdateInterval(trade));
        assert_eq!(
            app.chart.replay_driver().unwrap().time_range(),
            range,
            "returning to trade updates must retain the recording"
        );
        app.stop_replay();
        super::super::tests::settle(&mut app, &ctx);
        assert!(signals(&mut app), "leaving replay restores host analytics");
    }

    /// The feed keeps trades from 14:00, the hour two hours before 16:30: an hourly chart's
    /// one-trade source keeps them all, a four-hour chart's leaves out the bar from 12:00, which
    /// lost its first two hours, and keeps the trades from 16:00.
    #[test]
    fn trade_sources_leave_out_a_bar_the_feed_holds_in_part() {
        let time = |text: &str| text.parse::<chrono::DateTime<chrono::Utc>>().unwrap();
        let live = LiveFeed::new("SAMPLE", time("2024-03-15T16:30:00Z"));
        super::super::tests::finish_feed(&live);
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

    /// Bars the chart shows during a market replay, after the frames that build them.
    fn shown_after_frames(app: &mut DemoApp, ctx: &egui::Context) -> Vec<Bar> {
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
        app.chart.bars(SeriesId::MAIN).to_vec()
    }

    /// With an exchange session, the chart shows the bar the replay is on. Under UTC with
    /// extended hours, the daily bar of Tuesday 10 November 2026 closes with its post-market at
    /// 01:00 UTC on Wednesday, an hour after its date ends. With regular hours in New York, the
    /// hourly bar from 10:30 on Wednesday 11 November closes at 11:30.
    #[test]
    fn session_replay_shows_the_bar_at_the_cursor() {
        for (session, zone, timeframe, time) in [
            (
                SessionType::ETH,
                "UTC",
                Timeframe::Day1,
                "2026-11-10T00:00:00Z",
            ),
            (
                SessionType::RTH,
                "America/New_York",
                Timeframe::Hour1,
                "2026-11-11T15:30:00Z",
            ),
        ] {
            let ctx = egui::Context::default();
            let mut app = DemoApp::new_at(
                &eframe::CreationContext::_new_kittest(ctx.clone()),
                "2026-11-12T15:00:00Z".parse().unwrap(),
                true,
            );
            super::super::tests::settle(&mut app, &ctx);
            app.timeframe_toolbar.set_session(session);
            app.apply_session();
            super::super::apply_timezone(&mut app.chart, &mut app.timeframe_toolbar, zone);
            app.switch_timeframe(timeframe);
            super::super::tests::settle(&mut app, &ctx);
            app.start_replay();
            let time: DateTime<Utc> = time.parse().unwrap();
            let at = app.bars.iter().position(|bar| bar.time == time).unwrap();
            app.replay
                .as_mut()
                .unwrap()
                .handle_action(ReplayAction::JumpToBar(at));
            app.push_replay_window_to_chart();
            let shown = app.chart.bars(SeriesId::MAIN).last().map(|bar| bar.time);
            assert_eq!(shown, Some(time), "{session:?} in {zone}");
        }
    }

    /// A change of the chart interval keeps a replay stepping by an interval the new chart can
    /// show: its own, or a finer one that divides it, which the update list then offers.
    #[test]
    fn interval_change_keeps_a_usable_update_step() {
        let trade = Timeframe::Ticks(NonZeroU64::MIN);
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-12T15:00:00Z".parse().unwrap(),
            true,
        );
        super::super::tests::settle(&mut app, &ctx);
        app.switch_timeframe(Timeframe::Hour1);
        app.start_replay_at(10);
        let cutoff = app.chart.replay_time().unwrap();

        app.switch_timeframe(Timeframe::Min1);
        assert_eq!(app.replay_interval, Timeframe::Min1);
        assert_eq!(
            app.replay_update_intervals(),
            [Timeframe::Min1, Timeframe::Sec1, trade]
        );
        assert_eq!(app.chart.replay_time(), Some(cutoff), "the cursor stays");
        let shown = app.chart.bars(SeriesId::MAIN).len();
        assert!(shown > 1);
        app.handle_replay_control(ReplayControlAction::NextBar);
        assert_eq!(app.chart.bars(SeriesId::MAIN).len(), shown + 1);

        app.switch_timeframe(Timeframe::Hour1);
        assert_eq!(app.replay_interval, Timeframe::Min1, "a finer step stays");
        // A fresh hourly replay, which has no finer source yet.
        app.stop_replay();
        app.start_replay_at(10);
        assert_eq!(app.replay_interval, Timeframe::Hour1);
        app.switch_timeframe(Timeframe::Day1);
        assert_eq!(app.replay_interval, Timeframe::Hour1);
        assert_eq!(
            app.replay_update_intervals(),
            [Timeframe::Day1, Timeframe::Hour1, Timeframe::Min1, trade]
        );
        // The first step opens the next day, whose bar the second step then changes.
        app.handle_replay_control(ReplayControlAction::NextBar);
        let forming = app.chart.bars(SeriesId::MAIN).last().unwrap().clone();
        let at = app.replay.as_ref().unwrap().state.curr_bar;
        let hours = replay_source(&app.live, Timeframe::Hour1, &app.chart).0;
        app.handle_replay_control(ReplayControlAction::NextBar);
        let stepped = app.chart.bars(SeriesId::MAIN).last().unwrap();
        assert_eq!(stepped.time, forming.time);
        assert_eq!(stepped.volume, forming.volume + hours[at + 1].volume);
        assert_eq!(stepped.close, hours[at + 1].close);
    }

    #[test]
    fn replay_scope_menu_controls_the_layout_and_saved_session() {
        use super::super::tests::{settle, ui_click};
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-12T15:00:00Z".parse().unwrap(),
            true,
        );
        settle(&mut app, &ctx);
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::Four);
        settle(&mut app, &ctx);
        app.activate_chart_slot(1);
        app.layout.set_sync_group(2, Some("Other".into()));
        app.layout.set_sync_group(3, Some("Other".into()));
        let size = egui::vec2(1440.0, 900.0);
        let texts = |app: &mut DemoApp| replay_ui_texts(app, &ctx);
        let click = |app: &mut DemoApp, label: &str| click_replay_ui(app, &ctx, label);
        let assert_scope = |app: &DemoApp, scope| {
            assert!(app.chart.replay_time().is_some());
            let expected = if scope == ReplayScope::AllCharts {
                vec![0, 1, 2, 3]
            } else {
                vec![app.layout_slot]
            };
            assert_eq!(app.replay_slots, expected);
            for extra in &app.extra_charts {
                assert_eq!(
                    extra.chart.replay_time(),
                    (scope == ReplayScope::AllCharts).then(|| app.chart.replay_time().unwrap())
                );
            }
            assert_eq!(app.replay.as_ref().unwrap().state.scope, scope);
            assert_eq!(app.replay_session.as_ref().unwrap().0.scope, scope);
        };
        app.toggle_replay();
        click(&mut app, "Replay start");
        click(&mut app, "This chart");
        ui_click(&ctx, size, egui::pos2(1400.0, 400.0), |ui| {
            replay_test_ui(&mut app, ui)
        });
        let start = app.chart.bars(SeriesId::MAIN)[10].time;
        app.handle_replay_bar_click(&ctx, start);
        assert_scope(&app, ReplayScope::ThisChart);

        for scope in [ReplayScope::AllCharts, ReplayScope::ThisChart] {
            click(&mut app, "Replay start");
            click(
                &mut app,
                if scope == ReplayScope::AllCharts {
                    "All charts"
                } else {
                    "This chart"
                },
            );
            assert_scope(&app, scope);
            // A new start and replacement source controllers keep the menu's choice.
            click(&mut app, "First available bar");
            assert_scope(&app, scope);
            app.set_replay_interval(app.state.timeframe);
            assert_scope(&app, scope);
            let cutoff = app.chart.replay_time();
            app.stop_replay();
            app.toggle_replay();
            click(&mut app, "Replay start");
            // Choose the opposite scope before Resume to prove it adopts the saved one.
            click(
                &mut app,
                if scope == ReplayScope::AllCharts {
                    "This chart"
                } else {
                    "All charts"
                },
            );
            click(&mut app, "Resume saved session");
            assert_scope(&app, scope);
            assert_eq!(app.chart.replay_time(), cutoff);
        }

        for recording in [true, false] {
            app.recording_files.opened = recording;
            if !recording {
                app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::Single);
            }
            click(&mut app, "Replay start");
            let shown = texts(&mut app);
            assert!(
                !shown.iter().any(|(text, _)| matches!(
                    text.as_str(),
                    "CHARTS" | "This chart" | "All charts"
                ))
            );
            ui_click(&ctx, size, egui::pos2(1400.0, 400.0), |ui| {
                replay_test_ui(&mut app, ui)
            });
        }
    }

    #[test]
    fn replay_toolbar_menu_drives_the_layout() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-12T15:00:00Z".parse().unwrap(),
            true,
        );
        app.switch_timeframe(Timeframe::Min1);
        super::super::tests::settle(&mut app, &ctx);
        app.ensure_chart_slots(egui_charts::widget::ChartLayoutMode::Four);
        super::super::tests::settle(&mut app, &ctx);
        app.connect_paper();
        app.trading.display.instant_orders = true;
        app.start_replay_at(10);
        let click = |app: &mut DemoApp, label: &str| click_replay_ui(app, &ctx, label);
        click(&mut app, "Menu");
        click(&mut app, "Replay toolbar");
        // Close the host menu before using the docked toolbar.
        super::super::tests::ui_click(
            &ctx,
            egui::vec2(1440.0, 900.0),
            egui::pos2(1400.0, 400.0),
            |ui| replay_test_ui(&mut app, ui),
        );
        let before = app.chart.replay_time().unwrap();
        let at = app.replay.as_ref().unwrap().state.curr_bar;
        click(&mut app, "Step forward");
        assert_eq!(app.replay.as_ref().unwrap().state.curr_bar, at + 1);
        assert_eq!(
            app.chart.replay_time(),
            Some(before + chrono::TimeDelta::minutes(1))
        );
        assert!(
            app.extra_charts
                .iter()
                .all(|extra| extra.chart.replay_time() == app.chart.replay_time())
        );
        // Both controls show 1x; the toolbar's speed selector is above the floating bar.
        click(&mut app, "1x");
        click(&mut app, "50x");
        assert_eq!(app.replay.as_ref().unwrap().state.speed.multiplier(), 50.0);
        assert_eq!(
            replay_ui_texts(&mut app, &ctx)
                .iter()
                .filter(|(text, _)| text == "50x")
                .count(),
            2
        );
        // Backward toolbar actions use the same account safeguard as the replay bar.
        let symbol = app.state.symbol.clone();
        for _ in 0..2 {
            market_order(&mut app, &ctx, &symbol, OrderSide::Buy);
        }
        assert!(app.replay_paper.as_ref().unwrap().last_change().is_some());
        let cutoff = app.chart.replay_time();
        click(&mut app, "Step backward");
        assert_eq!(app.chart.replay_time(), cutoff);
        assert!(app.replay_prompt.is_open);
        click(&mut app, "Trade from here");
        assert_eq!(app.replay.as_ref().unwrap().state.curr_bar, at);
        assert_eq!(app.chart.replay_time(), Some(before));
        click(&mut app, "Stop");
        assert!(app.replay.is_none());
        assert!(app.replay_slots.is_empty());
        assert!(app.chart.replay_time().is_none());
        assert!(
            app.extra_charts
                .iter()
                .all(|extra| extra.chart.replay_time().is_none())
        );
        assert!(
            !replay_ui_texts(&mut app, &ctx)
                .iter()
                .any(|(text, _)| text == "50x")
        );
    }

    /// The bar offers daily and second updates where supported, with Automatic on every time
    /// chart. A daily update grows a weekly bar from completed daily bars only.
    #[test]
    fn replay_update_choices_and_weekly_daily_steps() {
        use super::super::tests::{ui_click, ui_frame};
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-12T15:00:00Z".parse().unwrap(),
            true,
        );
        super::super::tests::settle(&mut app, &ctx);
        let size = egui::vec2(1440.0, 900.0);
        let frame = |app: &mut DemoApp, events| {
            ui_frame(&ctx, size, events, |ui| app.show_replay_control(ui.ctx()))
        };
        let texts = |app: &mut DemoApp| {
            let mut texts = Vec::new();
            for _ in 0..3 {
                texts = frame(app, vec![]);
            }
            texts
        };
        let click = |app: &mut DemoApp, label: &str| {
            let point = texts(app)
                .iter()
                .find(|(text, _)| text == label)
                .unwrap_or_else(|| panic!("missing replay control: {label}"))
                .1
                .center();
            ui_click(&ctx, size, point, |ui| app.show_replay_control(ui.ctx()));
        };
        let key = |pressed, key| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut offered = Vec::new();
        for timeframe in [
            Timeframe::Min1,
            Timeframe::Hour1,
            Timeframe::Day1,
            Timeframe::Week1,
            Timeframe::Month1,
        ] {
            app.stop_replay();
            app.switch_timeframe(timeframe);
            super::super::tests::settle(&mut app, &ctx);
            app.start_replay_at(10);
            click(&mut app, &format!("{timeframe} updates"));
            let shown = texts(&mut app);
            let has = |label| shown.iter().any(|(text, _)| text == label);
            offered.push((has("Automatic"), has("1D updates"), has("1s updates")));
            frame(
                &mut app,
                vec![key(true, egui::Key::Escape), key(false, egui::Key::Escape)],
            );
        }
        assert_eq!(
            offered,
            [
                (true, false, true),
                (true, false, true),
                (true, true, false),
                (true, true, false),
                (true, true, false)
            ]
        );

        app.stop_replay();
        app.switch_timeframe(Timeframe::Week1);
        super::super::tests::settle(&mut app, &ctx);
        app.start_replay_at(2);
        assert_eq!(
            app.replay_session.as_ref().unwrap().0.scope,
            ReplayScope::AllCharts
        );
        let days = replay_source(&app.live, Timeframe::Day1, &app.chart).0;
        click(&mut app, "1W updates");
        click(&mut app, "1D updates");
        assert_eq!(app.replay_interval, Timeframe::Day1);
        let step = |app: &mut DemoApp| {
            // Return focus from the selector to the chart before its transport shortcut.
            if let Some(id) = ctx.memory(|memory| memory.focused()) {
                ctx.memory_mut(|memory| memory.surrender_focus(id));
            }
            frame(
                app,
                vec![
                    key(true, egui::Key::ArrowRight),
                    key(false, egui::Key::ArrowRight),
                ],
            );
        };
        step(&mut app);
        let first = app.replay.as_ref().unwrap().state.curr_bar;
        step(&mut app);
        assert_eq!(app.replay.as_ref().unwrap().state.curr_bar, first + 1);
        let shown = app.chart.bars(SeriesId::MAIN).last().unwrap();
        let (a, b) = (&days[first], &days[first + 1]);
        assert_eq!(
            (shown.open, shown.high, shown.low, shown.close, shown.volume),
            (
                a.open,
                a.high.max(b.high),
                a.low.min(b.low),
                b.close,
                a.volume + b.volume
            )
        );

        click(&mut app, "1D updates");
        click(&mut app, "Automatic");
        assert!(app.replay_control.uses_automatic_update_interval());
        // Automatic follows both changes; a fixed hourly step would have stayed hourly.
        app.switch_timeframe(Timeframe::Hour1);
        super::super::tests::settle(&mut app, &ctx);
        assert_eq!(app.replay_interval, Timeframe::Hour1);
        app.switch_timeframe(Timeframe::Day1);
        super::super::tests::settle(&mut app, &ctx);
        assert_eq!(app.replay_interval, Timeframe::Day1);

        // Recent execution history supplies genuine one-second updates near the live edge.
        app.stop_replay();
        app.switch_timeframe(Timeframe::Min1);
        super::super::tests::settle(&mut app, &ctx);
        app.start_replay_at(app.chart.bars(SeriesId::MAIN).len() - 3);
        click(&mut app, "Automatic");
        click(&mut app, "1s updates");
        assert_eq!(app.replay_interval, Timeframe::Sec1);
        let before = app.chart.replay_time().unwrap();
        step(&mut app);
        assert_eq!(
            app.chart.replay_time(),
            Some(before + chrono::TimeDelta::seconds(1))
        );

        // Hand values: gcd(2,17)=1 minute, gcd(4,6)=2 minutes; calendar periods use a day.
        for (a, b, expected) in [
            ("2min", "17min", Timeframe::Min1),
            ("4min", "6min", "2min".parse().unwrap()),
            ("1W", "1M", Timeframe::Day1),
            ("25h", "1h", Timeframe::Hour1),
            ("25h", "25h", "25h".parse().unwrap()),
            ("14d", "21d", "7d".parse().unwrap()),
            ("30d", "30d", "30d".parse().unwrap()),
            ("100ms", "250ms", Timeframe::Ticks(NonZeroU64::MIN)),
        ] {
            assert_eq!(
                common_replay_interval(a.parse().unwrap(), b.parse().unwrap()),
                expected
            );
        }
    }

    /// A chart interval whose history starts after the replay cursor moves the replay to its
    /// first bar and says so, instead of leaving an empty chart.
    #[test]
    fn interval_change_before_its_history_moves_the_replay_to_the_first_bar() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-12T15:00:00Z".parse().unwrap(),
            true,
        );
        super::super::tests::settle(&mut app, &ctx);
        app.switch_timeframe(Timeframe::Day1);
        app.start_replay_at(10);
        app.switch_timeframe(Timeframe::Min1);
        assert_eq!(
            app.toasts.toasts().last().unwrap().message,
            "No 1min history at the replay cursor: replay moved to the first 1min bar"
        );
        let first = app.bars[0].time;
        let shown: Vec<_> = (app.chart.bars(SeriesId::MAIN).iter())
            .map(|bar| bar.time)
            .collect();
        assert_eq!(shown, [first]);
        assert_eq!(app.replay_interval, Timeframe::Min1);
        assert!(app.replay.is_some() && app.state.replay_active);
    }

    /// A click on a bar while a trade-by-trade replay runs restarts it with that bar complete,
    /// as a click does with candle updates.
    #[test]
    fn trade_replay_click_restarts_at_the_clicked_bar() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-10-27T12:05:14Z".parse().unwrap(),
            true,
        );
        super::super::tests::settle(&mut app, &ctx);
        app.advance_sample_to(app.live.now() + chrono::TimeDelta::seconds(300));
        // Hand the chart the advanced minutes, which the feed would deliver on later frames.
        let bars = app.live.snapshot(
            app.state.timeframe,
            app.chart.bar_clock(),
            app.chart.extended_hours(),
        );
        app.chart.set_data(SeriesId::MAIN, bars);
        app.start_replay();
        app.handle_replay_control(ReplayControlAction::GoToEnd);
        let trade = Timeframe::Ticks(NonZeroU64::MIN);
        app.handle_replay_control(ReplayControlAction::SetUpdateInterval(trade));
        let shown = shown_after_frames(&mut app, &ctx);
        let traded: Vec<_> = shown.iter().filter(|bar| bar.volume > 0.0).collect();
        assert!(traded.len() >= 3, "{} bars shown", shown.len());
        let clicked = traded[traded.len() / 2].time;
        app.handle_replay_control(ReplayControlAction::Start(ReplayStartAction::SelectBar));
        app.handle_replay_bar_click(&ctx, clicked);
        assert_eq!(app.replay_interval, trade);
        let cutoff = app.chart.replay_time().unwrap();
        assert!(
            clicked < cutoff && cutoff <= clicked + chrono::TimeDelta::minutes(1),
            "clicked the bar at {clicked}, the replay cutoff is {cutoff}"
        );
    }

    /// Starts chosen on the replay bar while an imported recording replays stay in that recording
    /// and place its clock at the chosen execution or bar, until Exit replay.
    #[test]
    fn recording_replay_starts_stay_in_the_recording() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-10-27T12:05:14Z".parse().unwrap(),
            true,
        );
        super::super::tests::settle(&mut app, &ctx);
        app.advance_sample_to(app.live.now() + chrono::TimeDelta::seconds(180));
        let mut events = Vec::new();
        app.live.read_recording(&mut events);
        let bytes = recording_bytes(&app.live.instrument(), &events).unwrap();
        let mut import = RecordingImport::new(bytes).unwrap();
        while !import.read_batch().unwrap() {}
        let loaded = import.finish().unwrap();
        let executions: Vec<_> = loaded.bars.iter().map(|bar| bar.time).collect();
        let (first, middle, last) = (
            executions[0],
            executions[executions.len() / 2],
            *executions.last().unwrap(),
        );
        app.open_recording(loaded).unwrap();
        let start = |app: &mut DemoApp, action| {
            app.handle_replay_control(ReplayControlAction::GoToEnd);
            app.handle_replay_control(ReplayControlAction::Start(action));
            assert!(app.recording_files.opened, "{action:?}");
            app.chart.replay_driver().unwrap().time()
        };
        assert_eq!(start(&mut app, ReplayStartAction::First), first);
        assert_eq!(app.replay.as_ref().unwrap().state.curr_bar, 0);
        assert_eq!(start(&mut app, ReplayStartAction::Date(middle)), middle);
        assert!((first..=last).contains(&start(&mut app, ReplayStartAction::Random)));
        app.handle_replay_control(ReplayControlAction::GoToEnd);
        let shown = shown_after_frames(&mut app, &ctx);
        assert!(shown.len() >= 3, "{} bars shown", shown.len());
        let clicked = shown[shown.len() / 2].time;
        start(&mut app, ReplayStartAction::SelectBar);
        app.handle_replay_bar_click(&ctx, clicked);
        assert!(!app.replay_selecting && app.recording_files.opened);
        let cutoff = app.chart.replay_time().unwrap();
        assert!(
            clicked < cutoff && cutoff <= clicked + chrono::TimeDelta::minutes(1),
            "clicked the bar at {clicked}, the replay cutoff is {cutoff}"
        );
        assert_eq!(
            shown_after_frames(&mut app, &ctx).last().unwrap().time,
            clicked
        );
        // Exit replay closes the recording and leaves the live history in the demo's bars.
        app.stop_replay();
        assert!(!app.recording_files.opened && app.chart.replay_driver().is_none());
        assert_eq!(app.bars, app.chart.bars(SeriesId::MAIN));
    }
}
