//! The Data source dialog: the first-visit choice, and the steps that describe a profile,
//! test it against the service and connect it.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{DateTime, Utc};
use egui::{RichText, TextEdit};
use egui_charts::data::{FeedError, FeedState, FeedStatus};
use egui_charts::model::{Bar, BarClock, Timeframe, TimezoneId};
use egui_charts::theme::ThemeExt as _;
use egui_charts::ui_kit::alert::Alert;
use egui_charts::ui_kit::buttons::Button;
use egui_charts::ui_kit::dialog::DialogFooter;
use egui_charts::ui_kit::ext::UiExt as _;
use egui_charts::ui_kit::form::{FormGrid, FormRow};
use egui_charts::ui_kit::modal::Modal;
use egui_charts::ui_kit::tab_bar::{TabBar, TabLabel};
use serde_json::Value;

use super::super::storage::DemoStorage;
use super::feed::{self, Reading};
use super::profile::{
    self, BarFields, Clock, ClockUnit, ConditionRule, DateFormat, FieldPath, History, Interval,
    Layout, PLACEHOLDERS, Profile, RangeFormat, Route, RouteKind, Rows, Segment, Stream, TimeSpec,
    TimeUnit, TradeFields, at,
};
use super::shape::{self, Proposal};
use super::store::{self, Saved};
use super::stream::{self, Routed, Session, StreamEvent};
use super::time::{self, Stamp};
use super::{Failure, Secrets, bars, conceal, endpoint, http, tree};

/// The height the footer takes below the steps.
const FOOTER: f32 = 64.0;
/// The height of the line above the footer.
const LINE: f32 = 24.0;
/// The height of a tree that scrolls. A nested scroll area would otherwise take only what is
/// left of the dialog's view below it.
const TREE: f32 = 200.0;
/// The height of the log once it scrolls.
const LOG: f32 = 140.0;
/// The lines the log keeps.
const LOG_LINES: usize = 200;
/// The largest file an import reads.
const LARGEST_FILE: u64 = 1 << 20;
/// What the dialog says about `Connect` while own data is on.
const RECONNECTS: &str =
    "Connect reconnects with this profile. Studies and drawings on the chart are not kept.";

/// What the dialog asks the application to do.
pub(crate) enum Outcome {
    /// Show `profile` on the chart, read with the key values `secrets`.
    Connect {
        profile: Box<Profile>,
        secrets: Secrets,
    },
    /// Go back to the synthetic session.
    Synthetic,
    /// Write `text` to a file named `file`: an exported profile.
    Export { file: String, text: String },
}

/// The dialog and everything it edits. Its tests against the service (a history request, a
/// stream session) belong to it and end when it closes.
pub(crate) struct DataSourceDialog {
    storage: Arc<DemoStorage>,
    can_save: bool,
    /// The saved state as it was last written.
    saved: Saved,
    open: bool,
    /// Whether `startup` has acted.
    started: bool,
    view: View,
    step: Step,
    draft: Draft,
    /// What the check before a Test or a connection found. Nothing is sent while it holds a
    /// line.
    report: Vec<String>,
    /// Why the last import gave no profile.
    notice: Option<String>,
    /// Where the text of a picked file arrives.
    import: Option<Picked>,
    /// The symbol and the interval of both Tests, as places in the lists of the draft.
    symbol: usize,
    interval: usize,
    history: HistoryTest,
    live: LiveTest,
}

/// What the file picker answers: the text of the file, nothing when the user gave up, or why
/// the file was not read.
type Picked = Receiver<Result<Option<String>, String>>;

/// Where the answer of a history Test arrives.
type Answer = Arc<Mutex<Option<Result<String, Failure>>>>;

/// What decides the route of a record and the condition values it offers.
type Basis = (FieldPath, FieldPath, Vec<Route>);

/// The events of a session, each with the time it arrived.
type Events = Arc<Mutex<Vec<(DateTime<Utc>, StreamEvent)>>>;

/// What the dialog shows.
#[derive(Clone, Copy, PartialEq)]
enum View {
    /// The first-visit choice.
    Choice,
    /// The steps that describe a profile.
    Steps,
    /// The text of the file an export writes.
    Export,
}

/// One of the three steps.
#[derive(Clone, Copy, PartialEq)]
enum Step {
    Source,
    History,
    Live,
}

impl TabLabel for Step {
    fn tab_label(&self) -> &str {
        match self {
            Self::Source => "Source",
            Self::History => "History",
            Self::Live => "Live",
        }
    }
}

/// What the dialog edits. Nothing of it reaches the chart or the saved state before `Save` or
/// `Connect`.
struct Draft {
    profile: Profile,
    /// The value of each key, in the order of `profile.keys`.
    values: Vec<String>,
    /// The choice `Keep keys in this browser`.
    keep: bool,
    /// The name of the saved profile the draft was loaded from. `None` for a new or an
    /// imported one.
    origin: Option<String>,
    /// The symbols as typed.
    symbols: String,
    /// The time zone as typed. The profile keeps the last valid one.
    zone: String,
    /// The history and the stream as they were when their switch was turned off.
    history_off: Option<History>,
    stream_off: Option<Stream>,
}

impl Draft {
    /// A draft of `profile` with the key values `values`.
    fn of(profile: Profile, mut values: Vec<String>, keep: bool, origin: Option<String>) -> Self {
        values.resize(profile.keys.len(), String::new());
        Self {
            symbols: profile.symbols.join(", "),
            zone: profile.time_zone.name().to_owned(),
            profile,
            values,
            keep,
            origin,
            history_off: None,
            stream_off: None,
        }
    }

    /// A new profile.
    fn blank() -> Self {
        Self::of(Profile::default(), Vec::new(), false, None)
    }

    /// The saved profile `profile`, with the values of its keys when they were kept.
    fn saved(saved: &Saved, profile: &Profile) -> Self {
        let kept = saved.keys.iter().find(|(name, _)| *name == profile.name);
        let kept = kept.map(|(_, secrets)| secrets);
        let value = |key: &String| {
            let held = kept.and_then(|secrets| secrets.iter().find(|(name, _)| name == key));
            held.map(|(_, value)| value.clone()).unwrap_or_default()
        };
        let values = profile.keys.iter().map(value).collect();
        let origin = Some(profile.name.clone());
        Self::of(profile.clone(), values, kept.is_some(), origin)
    }

    /// The profile in use, else the one used last, else a new one.
    fn current(saved: &Saved) -> Self {
        let named = |name: &String| saved.profiles.iter().find(|profile| profile.name == *name);
        let profile = saved.current.as_ref().and_then(named);
        profile.map_or_else(Self::blank, |profile| Self::saved(saved, profile))
    }

    /// The key values, one pair per key in the order of the profile.
    fn secrets(&self) -> Secrets {
        let pairs = self.profile.keys.iter().zip(&self.values);
        pairs
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }
}

/// The templates a check covers.
#[derive(Clone, Copy, PartialEq)]
enum Scope {
    History,
    Stream,
    Both,
}

/// A place a click on a tree node fills.
#[derive(Clone, Copy, PartialEq)]
enum Slot {
    Rows,
    Header,
    Records,
    RouteBy,
    Time,
    Clock,
    Open,
    High,
    Low,
    Close,
    Volume,
    Price,
    Size,
    Condition,
}

impl Slot {
    fn label(self) -> &'static str {
        match self {
            Self::Rows => "rows",
            Self::Header => "header",
            Self::Records => "records",
            Self::RouteBy => "route by",
            Self::Time => "time",
            Self::Clock => "time of day",
            Self::Open => "open",
            Self::High => "high",
            Self::Low => "low",
            Self::Close => "close",
            Self::Volume => "volume",
            Self::Price => "price",
            Self::Size => "size",
            Self::Condition => "condition",
        }
    }
}

/// The reading of a time field, as the choice beside the slot names it.
#[derive(Clone, Copy, PartialEq)]
enum TimeForm {
    Seconds,
    Milliseconds,
    Microseconds,
    Nanoseconds,
    Iso8601,
    Compact,
    Dashed,
}

impl TimeForm {
    const ALL: [Self; 7] = [
        Self::Seconds,
        Self::Milliseconds,
        Self::Microseconds,
        Self::Nanoseconds,
        Self::Iso8601,
        Self::Compact,
        Self::Dashed,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Seconds => "Unix seconds",
            Self::Milliseconds => "Unix milliseconds",
            Self::Microseconds => "Unix microseconds",
            Self::Nanoseconds => "Unix nanoseconds",
            Self::Iso8601 => "ISO 8601 text",
            Self::Compact => "Date 20261111",
            Self::Dashed => "Date 2026-11-11",
        }
    }

    /// The form `spec` reads its field in.
    fn of(spec: &TimeSpec) -> Self {
        match spec {
            TimeSpec::Instant { unit, .. } => match unit {
                TimeUnit::Seconds => Self::Seconds,
                TimeUnit::Milliseconds => Self::Milliseconds,
                TimeUnit::Microseconds => Self::Microseconds,
                TimeUnit::Nanoseconds => Self::Nanoseconds,
                TimeUnit::Iso8601 => Self::Iso8601,
            },
            TimeSpec::Wall { format, .. } => match format {
                DateFormat::Compact => Self::Compact,
                DateFormat::Dashed => Self::Dashed,
            },
        }
    }

    /// Reads the field of `spec` in this form. A time of day stays with a date.
    fn apply(self, spec: &mut TimeSpec) {
        let (field, clock) = match std::mem::take(spec) {
            TimeSpec::Instant { field, .. } => (field, None),
            TimeSpec::Wall { date, clock, .. } => (date, clock),
        };
        let instant = |unit| TimeSpec::Instant {
            field: field.clone(),
            unit,
        };
        let wall = |format| TimeSpec::Wall {
            date: field.clone(),
            format,
            clock: clock.clone(),
        };
        *spec = match self {
            Self::Seconds => instant(TimeUnit::Seconds),
            Self::Milliseconds => instant(TimeUnit::Milliseconds),
            Self::Microseconds => instant(TimeUnit::Microseconds),
            Self::Nanoseconds => instant(TimeUnit::Nanoseconds),
            Self::Iso8601 => instant(TimeUnit::Iso8601),
            Self::Compact => wall(DateFormat::Compact),
            Self::Dashed => wall(DateFormat::Dashed),
        };
    }
}

/// The time and the fields of a bar or of a trade, by slot.
struct Fields<'a> {
    time: &'a mut TimeSpec,
    paths: Vec<(Slot, &'a mut FieldPath)>,
}

impl<'a> Fields<'a> {
    /// The slots of a bar.
    fn bar(fields: &'a mut BarFields) -> Self {
        let BarFields {
            time,
            open,
            high,
            low,
            close,
            volume,
        } = fields;
        let paths = vec![
            (Slot::Open, open),
            (Slot::High, high),
            (Slot::Low, low),
            (Slot::Close, close),
            (Slot::Volume, volume),
        ];
        Self { time, paths }
    }

    /// The slots of a trade.
    fn trade(fields: &'a mut TradeFields) -> Self {
        let paths = vec![
            (Slot::Price, &mut fields.price),
            (Slot::Size, &mut fields.size),
            (Slot::Condition, &mut fields.condition),
        ];
        Self {
            time: &mut fields.time,
            paths,
        }
    }

    /// The slots of the records of a route, when they carry any.
    fn route(kind: &'a mut RouteKind) -> Option<Self> {
        match kind {
            RouteKind::Ignore => None,
            RouteKind::Bars(fields) => Some(Self::bar(fields)),
            RouteKind::Trades(fields) => Some(Self::trade(fields)),
        }
    }

    /// The slots, in the order they are drawn. A date has a time of day beside it.
    fn slots(&self) -> Vec<Slot> {
        let mut slots = vec![Slot::Time];
        if matches!(self.time, TimeSpec::Wall { .. }) {
            slots.push(Slot::Clock);
        }
        slots.extend(self.paths.iter().map(|(slot, _)| *slot));
        slots
    }

    /// The path `slot` holds, empty when nothing is mapped. `None` for a slot these fields do
    /// not have.
    fn path(&self, slot: Slot) -> Option<FieldPath> {
        match (slot, &*self.time) {
            (Slot::Time, TimeSpec::Instant { field, .. }) => Some(field.clone()),
            (Slot::Time, TimeSpec::Wall { date, .. }) => Some(date.clone()),
            (Slot::Clock, TimeSpec::Wall { clock, .. }) => Some(
                clock
                    .as_ref()
                    .map(|clock| clock.field.clone())
                    .unwrap_or_default(),
            ),
            _ => {
                let held = self.paths.iter().find(|(held, _)| *held == slot);
                held.map(|(_, path)| FieldPath::clone(path))
            }
        }
    }

    /// Maps `slot` to the node at `path`, whose value is `value`.
    fn assign(&mut self, slot: Slot, path: FieldPath, value: &Value) {
        match slot {
            Slot::Time => *self.time = time::guess(path, value),
            Slot::Clock => {
                if let TimeSpec::Wall { clock, .. } = &mut *self.time {
                    let unit = clock.take().map(|clock| clock.unit).unwrap_or_default();
                    *clock = Some(Clock { field: path, unit });
                }
            }
            _ => {
                if let Some((_, held)) = self.paths.iter_mut().find(|(held, _)| *held == slot) {
                    **held = path;
                }
            }
        }
    }

    /// Leaves `slot` unmapped.
    fn clear(&mut self, slot: Slot) {
        match slot {
            Slot::Time => *self.time = TimeSpec::default(),
            Slot::Clock => {
                if let TimeSpec::Wall { clock, .. } = &mut *self.time {
                    *clock = None;
                }
            }
            _ => {
                if let Some((_, held)) = self.paths.iter_mut().find(|(held, _)| *held == slot) {
                    held.clear();
                }
            }
        }
    }

    /// The slot that becomes active once `slot` is filled: the next empty one, or `slot`
    /// itself. The time of day is optional and is never chosen this way.
    fn next(&self, slot: Slot) -> Slot {
        let order: Vec<Slot> = self.slots();
        let order: Vec<Slot> = order
            .into_iter()
            .filter(|held| *held != Slot::Clock)
            .collect();
        let from = order.iter().position(|held| *held == slot);
        let after = &order[from.map_or(0, |at| at + 1)..];
        let empty = |held: &&Slot| self.path(**held).is_some_and(|path| path.is_empty());
        after.iter().find(empty).copied().unwrap_or(slot)
    }

    /// The mapped nodes and the names of their slots.
    fn marks(&self) -> Vec<(FieldPath, &'static str)> {
        let named = |slot: Slot| {
            Some((
                self.path(slot).filter(|path| !path.is_empty())?,
                slot.label(),
            ))
        };
        self.slots().into_iter().filter_map(named).collect()
    }

    /// Draws one row of a slot grid per slot, read on `record`.
    fn rows(
        &mut self,
        ui: &mut egui::Ui,
        record: Option<&Value>,
        zone: &TimezoneId,
        active: &mut Slot,
    ) {
        for slot in self.slots() {
            let path = self.path(slot).unwrap_or_default();
            slot_cells(ui, slot, active, &path, false, record);
            let mut clear = false;
            ui.horizontal(|ui| {
                clear = clear_button(ui, !path.is_empty());
                match slot {
                    Slot::Time if !path.is_empty() => {
                        let mut form = TimeForm::of(self.time);
                        let label = |form: &TimeForm| form.label().to_owned();
                        if ui.combo_select("time_form", &mut form, TimeForm::ALL, label) {
                            form.apply(self.time);
                        }
                        if let Some(record) = record {
                            ui.weak(stamp_text(record, self.time, zone));
                        }
                    }
                    Slot::Clock => {
                        if let TimeSpec::Wall {
                            clock: Some(clock), ..
                        } = &mut *self.time
                        {
                            let units =
                                [ClockUnit::Milliseconds, ClockUnit::Seconds, ClockUnit::Text];
                            let label = |unit: &ClockUnit| clock_label(unit).to_owned();
                            ui.combo_select("clock_unit", &mut clock.unit, units, label);
                        }
                    }
                    _ => {}
                }
            });
            if clear {
                self.clear(slot);
            }
            ui.end_row();
        }
    }
}

/// The Test of the History step and what it found.
struct HistoryTest {
    /// Where the answer of the request in flight arrives, and the endpoint it was sent to.
    pending: Option<(Answer, String)>,
    /// The endpoint of the last answer.
    endpoint: String,
    /// Why the last Test gave no sample.
    error: Option<String>,
    /// Whether the last Test proposed the mapping.
    proposed: bool,
    /// The last response that was JSON.
    sample: Option<Value>,
    /// Counts the samples, so that each one gets trees of its own.
    samples: u64,
    /// The ways the sample can be read.
    proposals: Vec<Proposal>,
    /// The sample read with the mapping of the draft.
    preview: Option<Preview>,
    /// The active slot.
    slot: Slot,
}

impl Default for HistoryTest {
    fn default() -> Self {
        Self {
            pending: None,
            endpoint: String::new(),
            error: None,
            proposed: false,
            sample: None,
            samples: 0,
            proposals: Vec::new(),
            preview: None,
            slot: Slot::Rows,
        }
    }
}

/// A sample read with a mapping, kept until one of the two changes.
struct Preview {
    history: History,
    zone: TimezoneId,
    timeframe: Timeframe,
    samples: u64,
    /// The first row, laid out as the mapping says.
    row: Option<Value>,
    reading: Result<Reading, String>,
}

/// The Test of the Live step and its log.
struct LiveTest {
    /// The session of the Test that runs. Dropping it closes its socket.
    session: Option<Session>,
    /// Where the events of the session arrive. Every Test has its own.
    queue: Events,
    /// The address of the Test without its query.
    endpoint: String,
    /// Whether a socket of the Test has opened.
    opened: bool,
    /// The last close, while no socket of the Test has opened.
    failure: Option<String>,
    /// The newest lines, oldest first.
    log: VecDeque<Line>,
    /// Counts the lines of the Test.
    lines: u64,
    /// What the lines were last routed with.
    basis: Option<Basis>,
    /// The oldest message that has a record without a route. It is kept apart from the log, so
    /// that it cannot scroll away.
    unrouted: Option<Message>,
    /// The number of the selected line.
    selected: Option<u64>,
    /// The record of the selected message that is shown, counted from zero.
    record: usize,
    /// The selected route.
    route: usize,
    /// The active slot.
    slot: Slot,
    /// A condition value being typed.
    condition: String,
}

impl Default for LiveTest {
    fn default() -> Self {
        Self {
            session: None,
            queue: Arc::default(),
            endpoint: String::new(),
            opened: false,
            failure: None,
            log: VecDeque::new(),
            lines: 0,
            basis: None,
            unrouted: None,
            selected: None,
            record: 0,
            route: 0,
            slot: Slot::RouteBy,
            condition: String::new(),
        }
    }
}

/// One line of the log.
struct Line {
    number: u64,
    at: DateTime<Utc>,
    kind: LineKind,
}

/// What a line of the log reports.
enum LineKind {
    /// The connection opened or closed, or a message could not be read.
    Note {
        badge: &'static str,
        text: String,
        /// Whether the line reports a fault.
        fault: bool,
    },
    Message(Message),
}

/// One message of the stream.
#[derive(Clone)]
struct Message {
    text: String,
    /// The parsed message, or its text when it is not JSON.
    root: Value,
    /// Its records as the stream of the draft routes them.
    routed: Vec<Routed>,
}

impl Message {
    /// Whether a record of the message matches no route.
    fn unrouted(&self) -> bool {
        let lost = |record: &Routed| matches!(record, Routed::Unrouted { .. });
        self.routed.iter().any(lost)
    }
}

impl DataSourceDialog {
    /// A closed dialog over the state saved in `storage`. `can_save` is false when the
    /// application runs without storage: the first-visit choice is then not offered, because
    /// its answer could not be remembered, and nothing is saved.
    pub(crate) fn new(storage: Arc<DemoStorage>, can_save: bool) -> Self {
        let saved = Saved::load(&storage);
        Self {
            draft: Draft::current(&saved),
            storage,
            can_save,
            saved,
            open: false,
            started: false,
            view: View::Steps,
            step: Step::Source,
            report: Vec::new(),
            notice: None,
            import: None,
            symbol: 0,
            interval: 0,
            history: HistoryTest::default(),
            live: LiveTest::default(),
        }
    }

    /// Whether the dialog is on screen. The application leaves the keyboard to it.
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the dialog on the profile in use, else on the one used last, else on a new one.
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.view = View::Steps;
    }

    /// What the saved state asks for when the application starts. It acts on its first call
    /// and returns `None` on every later one. It opens the first-visit choice when that was
    /// never answered. When own data was in use it returns the connection to make if the keys
    /// of its profile were kept, and otherwise opens the dialog on that profile so that the
    /// keys can be entered.
    pub(crate) fn startup(&mut self) -> Option<Outcome> {
        if std::mem::replace(&mut self.started, true) || !self.can_save {
            return None;
        }
        if !self.saved.asked {
            self.open = true;
            self.view = View::Choice;
            return None;
        }
        if !self.saved.own {
            return None;
        }
        let current = self.saved.current.as_ref();
        let named = |name: &String| self.saved.profiles.iter().find(|saved| saved.name == *name);
        let Some(profile) = current.and_then(named) else {
            // The profile that was in use is gone.
            self.saved.own = false;
            self.saved.save(&self.storage);
            return None;
        };
        let draft = Draft::saved(&self.saved, profile);
        let secrets = draft.secrets();
        let report = findings(&draft.profile, &secrets, Scope::Both);
        if draft.profile.problem().is_none() && report.is_empty() {
            let profile = Box::new(draft.profile);
            return Some(Outcome::Connect { profile, secrets });
        }
        // The chart stays synthetic until the keys are entered and the profile connects.
        self.draft = draft;
        self.forget_tests();
        self.report = report;
        self.step = Step::Source;
        self.open();
        None
    }

    /// Draws the dialog while it is open and returns what the user decided in this frame.
    /// `running` is the state of the chart's feed while own data is on; the dialog shows its
    /// status and its last error above the steps.
    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        running: Option<&FeedState>,
    ) -> Option<Outcome> {
        if !self.open {
            return None;
        }
        self.poll_import();
        self.poll_history();
        self.poll_live();
        let mut open = true;
        let mut outcome = None;
        let modal = Modal::new("data_source", "Data source");
        let modal = match self.view {
            View::Choice => modal.width(420.0),
            View::Steps | View::Export => modal.width(860.0).height(620.0),
        };
        modal.show(ctx, &mut open, |ui| match self.view {
            View::Choice => self.choice(ui),
            View::Steps => outcome = self.steps(ui, running),
            View::Export => outcome = self.export(ui),
        });
        // The close button, Escape or a click beside the dialog. A button of the dialog that
        // closed it has done so itself.
        if !open && self.open {
            self.close();
        }
        outcome
    }

    /// Closes the dialog. The draft stays in memory; the Tests end.
    fn close(&mut self) {
        if self.view == View::Choice {
            // Closing the choice counts as the answer `Use synthetic data`.
            self.saved.asked = true;
            self.saved.save(&self.storage);
        }
        self.open = false;
        self.view = View::Steps;
        self.live.session = None;
        self.history.pending = None;
    }

    /// Forgets the Tests of the draft that was replaced: their samples, their log and the
    /// session, whose socket closes.
    fn forget_tests(&mut self) {
        self.history = HistoryTest::default();
        self.live = LiveTest::default();
        self.symbol = 0;
        self.interval = 0;
        self.report.clear();
    }

    /// The first-visit choice.
    fn choice(&mut self, ui: &mut egui::Ui) {
        ui.label(
            "This demo runs on synthetic data. You can also chart your own REST history and \
             WebSocket stream, mapped here in your browser.",
        );
        let footer = DialogFooter::new("Connect my own data").secondary("Use synthetic data");
        let (own, synthetic) = footer.show(ui);
        ui.weak("You can change this later under Data source in the menu.");
        if own {
            // The answer is saved at the click, so a visitor who gives up is not asked again.
            self.saved.asked = true;
            self.saved.save(&self.storage);
            self.draft = Draft::blank();
            self.forget_tests();
            self.step = Step::Source;
            self.view = View::Steps;
        }
        if synthetic {
            self.close();
        }
    }

    /// The text of the file an export writes, before it is saved.
    fn export(&mut self, ui: &mut egui::Ui) -> Option<Outcome> {
        let text = store::export(&self.draft.profile);
        ui.label(
            "This is the whole file: the addresses, headers and messages as typed. Key values \
             are not in it.",
        );
        let height = (ui.available_height() - FOOTER).max(0.0);
        let scroll = egui::ScrollArea::vertical().id_salt("data_source_file");
        scroll
            .auto_shrink([false, false])
            .max_height(height)
            .show(ui, |ui| {
                let mut shown = text.as_str();
                ui.add(
                    TextEdit::multiline(&mut shown)
                        .code_editor()
                        .desired_width(f32::INFINITY),
                );
            });
        let (save, back) = DialogFooter::new("Save file").secondary("Back").show(ui);
        if save || back {
            self.view = View::Steps;
        }
        let file = format!("{}.json", store::file_stem(&self.draft.profile.name));
        save.then_some(Outcome::Export { file, text })
    }

    /// The steps, with the status above them and the footer below.
    fn steps(&mut self, ui: &mut egui::Ui, running: Option<&FeedState>) -> Option<Outcome> {
        let ctx = ui.ctx().clone();
        if let Some(running) = running {
            self.status(ui, running);
        }
        if !self.report.is_empty() {
            Alert::warning(self.report.join("\n")).show(ui);
        }
        let dismissed = self.notice.as_ref().is_some_and(|notice| {
            let alert = Alert::error(notice).with_title("Import").dismissible(true);
            alert.show(ui).dismissed
        });
        if dismissed {
            self.notice = None;
        }
        TabBar::new(&[Step::Source, Step::History, Step::Live], &mut self.step).show(ui);

        let problem = self.draft.profile.problem();
        let taken = self.name_taken();
        let line = problem.is_some() || running.is_some();
        let below = FOOTER + if line { LINE } else { 0.0 };
        let height = (ui.available_height() - below).max(0.0);
        let mut outcome = None;
        let scroll = egui::ScrollArea::both().id_salt("data_source_step");
        scroll
            .auto_shrink([false, false])
            .max_height(height)
            .show(ui, |ui| {
                // The margin keeps the fields clear of the scroll bar.
                let margin = egui::Margin {
                    right: 12,
                    ..egui::Margin::ZERO
                };
                egui::Frame::new()
                    .inner_margin(margin)
                    .show(ui, |ui| match self.step {
                        Step::Source => outcome = self.source(ui, &ctx, running.is_some()),
                        Step::History => self.history(ui, &ctx),
                        Step::Live => self.live(ui, &ctx),
                    });
            });

        if let Some(problem) = &problem {
            let warning = ui.active_theme().ui.warning;
            ui.add(egui::Label::new(RichText::new(problem).color(warning)).truncate());
        } else if running.is_some() {
            ui.add(egui::Label::new(RichText::new(RECONNECTS).weak()).truncate());
        }
        let mut save = false;
        let footer = DialogFooter::new("Connect").secondary("Cancel");
        let footer = footer.primary_enabled(problem.is_none() && !taken);
        let (connect, cancel) = footer.show_with_left(ui, |ui| {
            if self.can_save {
                let named = !self.draft.profile.name.is_empty() && !taken;
                save = ui
                    .add(Button::new("Save").outlined().enabled(named))
                    .clicked();
            }
        });
        if cancel {
            self.close();
        }
        if save {
            self.store_draft();
        }
        if connect {
            outcome = self.connect();
        }
        outcome
    }

    /// The state of the chart's feed while own data is on.
    fn status(&self, ui: &mut egui::Ui, running: &FeedState) {
        let state = match running.status {
            None => "no live stream",
            Some(FeedStatus::Live) => "live",
            Some(FeedStatus::Connecting) => "connecting",
            // Disconnected. The session opens the connection again by itself.
            Some(_) => "disconnected, reconnecting",
        };
        let name = self.saved.current.clone().unwrap_or_default();
        let mut parts = vec![name, state.to_owned()];
        if running.loading_history {
            parts.push("loading history".to_owned());
        }
        if running.rejected_bars > 0 {
            let bars = usize::try_from(running.rejected_bars).unwrap_or(usize::MAX);
            parts.push(format!("{} dropped as invalid", counted(bars, "bar")));
        }
        // The chart keeps its most recent error. A connection error no longer describes a stream
        // that is live again.
        let live = running.status == Some(FeedStatus::Live);
        let error = running.last_error.as_ref();
        let error = error.filter(|error| !(live && matches!(error, FeedError::Network(_))));
        ui.horizontal_wrapped(|ui| {
            ui.label(parts.join(" · "));
            if let Some(error) = error {
                ui.error_label(error.to_string());
            }
        });
    }

    /// Whether another saved profile has the name of the draft.
    fn name_taken(&self) -> bool {
        let draft = &self.draft;
        let other = |saved: &Profile| {
            saved.name == draft.profile.name && Some(&saved.name) != draft.origin.as_ref()
        };
        self.saved.profiles.iter().any(other)
    }

    /// Stores the draft: it replaces the saved profile it was loaded from, or is appended when
    /// it is new or imported. Its key values are kept when the box is ticked and forgotten
    /// otherwise.
    fn store_draft(&mut self) {
        let profile = self.draft.profile.clone();
        let name = profile.name.clone();
        let origin = self.draft.origin.as_ref();
        let saved = &mut self.saved;
        let place =
            origin.and_then(|origin| saved.profiles.iter().position(|held| held.name == *origin));
        match place {
            Some(at) => {
                let old = std::mem::replace(&mut saved.profiles[at], profile).name;
                if old != name {
                    // A changed name renames: the kept keys and the profile in use follow it.
                    for (kept, _) in &mut saved.keys {
                        if *kept == old {
                            kept.clone_from(&name);
                        }
                    }
                    if saved.current.as_ref() == Some(&old) {
                        saved.current = Some(name.clone());
                    }
                }
            }
            None => saved.profiles.push(profile),
        }
        // Key values are kept only by choice.
        saved.keys.retain(|(kept, _)| *kept != name);
        if self.draft.keep {
            saved.keys.push((name.clone(), self.draft.secrets()));
        }
        self.draft.origin = Some(name);
        self.saved.save(&self.storage);
    }

    /// Checks the templates, stores the draft and asks the application to connect it.
    fn connect(&mut self) -> Option<Outcome> {
        let secrets = self.draft.secrets();
        self.report = findings(&self.draft.profile, &secrets, Scope::Both);
        if !self.report.is_empty() {
            return None;
        }
        self.store_draft();
        self.saved.own = true;
        self.saved.current = Some(self.draft.profile.name.clone());
        self.saved.save(&self.storage);
        self.close();
        let profile = Box::new(self.draft.profile.clone());
        Some(Outcome::Connect { profile, secrets })
    }

    /// Removes the saved profile the draft was loaded from.
    fn delete(&mut self) {
        let Some(name) = self.draft.origin.take() else {
            return;
        };
        let saved = &mut self.saved;
        saved.profiles.retain(|profile| profile.name != name);
        saved.keys.retain(|(kept, _)| *kept != name);
        if saved.current.as_ref() == Some(&name) {
            saved.current = None;
        }
        saved.save(&self.storage);
        self.draft = Draft::current(&self.saved);
        self.forget_tests();
    }

    /// The Source step: what the profile is and what it offers.
    fn source(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, on: bool) -> Option<Outcome> {
        /// What a button beside the profile list asks for.
        #[derive(Clone, Copy)]
        enum Pick {
            Load,
            New,
            Import,
            Export,
            Delete,
        }
        let names = self
            .saved
            .profiles
            .iter()
            .map(|saved| Some(saved.name.clone()));
        let names: Vec<Option<String>> = names.collect();
        let mut chosen = self.draft.origin.clone();
        // The profile on the chart stays.
        let on_chart = self.saved.own && self.saved.current == chosen;
        let deletable = chosen.is_some() && !on_chart;
        let idle = self.import.is_none();
        let mut pick = None;
        FormRow::new("Profile").show(ui, |ui| {
            let label =
                |name: &Option<String>| name.clone().unwrap_or_else(|| "New profile".into());
            if ui.combo_select("data_source_profile", &mut chosen, names, label) {
                pick = Some(Pick::Load);
            }
            let buttons = [
                ("New", Pick::New, true),
                ("Import", Pick::Import, idle),
                ("Export", Pick::Export, true),
                ("Delete", Pick::Delete, deletable),
            ];
            for (label, action, enabled) in buttons {
                if ui
                    .add(Button::new(label).outlined().small().enabled(enabled))
                    .clicked()
                {
                    pick = Some(action);
                }
            }
        });
        match pick {
            Some(Pick::Load) => {
                let named =
                    |name: String| self.saved.profiles.iter().find(|saved| saved.name == name);
                if let Some(profile) = chosen.and_then(named) {
                    self.draft = Draft::saved(&self.saved, profile);
                    self.forget_tests();
                }
            }
            Some(Pick::New) => {
                self.draft = Draft::blank();
                self.forget_tests();
            }
            Some(Pick::Import) => self.pick_file(ctx),
            Some(Pick::Export) => self.view = View::Export,
            Some(Pick::Delete) => self.delete(),
            None => {}
        }

        let taken = self.name_taken();
        let draft = &mut self.draft;
        FormRow::new("Name").show(ui, |ui| {
            ui.add(TextEdit::singleline(&mut draft.profile.name).desired_width(f32::INFINITY));
        });
        if taken {
            ui.error_label("Another saved profile has this name.");
        }
        let symbols = FormRow::new("Symbols").description(
            "Spelled as the service expects them, separated by commas or line ends. The chart \
             opens on the first.",
        );
        symbols.show(ui, |ui| {
            let field = TextEdit::multiline(&mut draft.symbols).desired_rows(2);
            if ui.add(field.desired_width(f32::INFINITY)).changed() {
                draft.profile.symbols = listed(&draft.symbols);
            }
        });
        let intervals = FormRow::new("Intervals").description(
            "A chart interval and the text the service expects for it, such as 1m, 60 or 60000. \
             The chart opens on the first row.",
        );
        intervals.show(ui, |ui| interval_rows(ui, &mut draft.profile.intervals));
        let zone = FormRow::new("Time zone").description(
            "The zone of every time that names none, and of bounds printed as dates: UTC, a \
             name such as America/New_York or an offset such as +05:30.",
        );
        zone.show(ui, |ui| {
            if ui
                .add(TextEdit::singleline(&mut draft.zone).desired_width(240.0))
                .changed()
                && let Ok(zone) = draft.zone.trim().parse()
            {
                draft.profile.time_zone = zone;
            }
        });
        if draft.zone.trim().parse::<TimezoneId>().is_err() {
            let kept = draft.profile.time_zone.name();
            ui.error_label(format!("This is not a time zone. {kept} stays in use."));
        }
        let keys = FormRow::new("Keys").description(
            "Written as {name} in an address, a header, a body or a message. A key typed \
             there as it is becomes part of the profile and is saved and exported with it.",
        );
        keys.show(ui, |ui| {
            key_rows(ui, &mut draft.profile.keys, &mut draft.values)
        });
        let keep = egui::Checkbox::new(&mut draft.keep, "Keep keys in this browser");
        ui.add_enabled(self.can_save, keep);
        if draft.keep {
            ui.weak("Kept as plain text in this browser's storage for this site.");
        }
        ui.weak("Keys are sent only to the endpoints of this profile and are never exported.");

        if on
            && ui
                .add(Button::new("Use synthetic data").outlined())
                .clicked()
        {
            self.saved.own = false;
            self.saved.save(&self.storage);
            self.close();
            return Some(Outcome::Synthetic);
        }
        None
    }

    /// Opens the system's file picker. Its answer arrives through `self.import`.
    fn pick_file(&mut self, ctx: &egui::Context) {
        let (answer, picked) = std::sync::mpsc::channel();
        self.import = Some(picked);
        let ctx = ctx.clone();
        let too_large = || "The file is larger than 1 MiB.".to_owned();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            let read = || -> Result<Option<String>, String> {
                let picker = rfd::FileDialog::new().add_filter("Profile", &["json"]);
                let Some(path) = picker.pick_file() else {
                    return Ok(None);
                };
                // Refused before it is read.
                let size = std::fs::metadata(&path)
                    .map_err(|error| error.to_string())?
                    .len();
                if size > LARGEST_FILE {
                    return Err(too_large());
                }
                let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
                Ok(Some(text))
            };
            let _ = answer.send(read());
            ctx.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let picker = rfd::AsyncFileDialog::new().add_filter("Profile", &["json"]);
            let read = match picker.pick_file().await {
                Some(file) if file.inner().size() > LARGEST_FILE as f64 => Err(too_large()),
                Some(file) => {
                    let text = String::from_utf8(file.read().await);
                    text.map(Some)
                        .map_err(|_| "The file is not UTF-8 text.".to_owned())
                }
                None => Ok(None),
            };
            let _ = answer.send(read);
            ctx.request_repaint();
        });
    }

    /// Takes the answer of the file picker when it has arrived.
    fn poll_import(&mut self) {
        let Some(picked) = self.import.take() else {
            return;
        };
        match picked.try_recv() {
            Ok(Ok(Some(text))) => self.import_text(&text),
            Ok(Ok(None)) => {}
            Ok(Err(error)) => self.notice = Some(error),
            Err(TryRecvError::Empty) => self.import = Some(picked),
            Err(TryRecvError::Disconnected) => {
                self.notice = Some("The file selection failed.".to_owned());
            }
        }
    }

    /// Makes the profile of an exported file the draft, as a new profile without key values.
    fn import_text(&mut self, text: &str) {
        let mut profile = match store::import(text) {
            Ok(profile) => profile,
            Err(error) => {
                self.notice = Some(error);
                return;
            }
        };
        // An import never replaces a saved profile.
        let base = profile.name.clone();
        let mut number = 2;
        while self
            .saved
            .profiles
            .iter()
            .any(|saved| saved.name == profile.name)
        {
            profile.name = format!("{base} {number}");
            number += 1;
        }
        self.draft = Draft::of(profile, Vec::new(), false, None);
        self.forget_tests();
        self.notice = None;
        self.step = Step::Source;
        self.view = View::Steps;
        // Names the keys to enter.
        self.report = findings(&self.draft.profile, &self.draft.secrets(), Scope::Both);
    }

    /// The symbol and the interval of the Tests, when the draft lists both.
    fn target(&self) -> Option<(String, Timeframe)> {
        let profile = &self.draft.profile;
        let symbol = profile.symbols.get(self.symbol)?;
        Some((symbol.clone(), profile.intervals.get(self.interval)?.chart))
    }

    /// The two choices beside `Test`: a symbol and an interval of the draft.
    fn target_choices(&mut self, ui: &mut egui::Ui) {
        let profile = &self.draft.profile;
        self.symbol = self.symbol.min(profile.symbols.len().saturating_sub(1));
        self.interval = self.interval.min(profile.intervals.len().saturating_sub(1));
        let symbol = |at: &usize| profile.symbols.get(*at).cloned().unwrap_or_default();
        ui.combo_select(
            "test_symbol",
            &mut self.symbol,
            0..profile.symbols.len(),
            symbol,
        );
        let interval = |at: &usize| {
            let row = profile.intervals.get(*at);
            row.map(|row| row.chart.to_string()).unwrap_or_default()
        };
        let rows = 0..profile.intervals.len();
        ui.combo_select("test_interval", &mut self.interval, rows, interval);
    }

    /// The History step: the request, its Test and the mapping of the response.
    fn history(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let keys = key_names(&self.draft.profile.keys);
        let mut on = self.draft.profile.history.is_some();
        if ui.checkbox(&mut on, "Use REST history").changed() {
            let draft = &mut self.draft;
            if on {
                draft.profile.history = Some(draft.history_off.take().unwrap_or_default());
            } else {
                draft.history_off = draft.profile.history.take();
            }
        }
        let Some(history) = &mut self.draft.profile.history else {
            return;
        };
        FormRow::new("URL").show(ui, |ui| {
            let hint = "https://api.example.com/bars?symbol={symbol}&interval={interval}&to={to}";
            let field = TextEdit::singleline(&mut history.url).hint_text(hint);
            ui.add(field.desired_width(f32::INFINITY));
        });
        ui.hint_label(format!(
            "Placeholders: {{symbol}}, {{interval}}, {{from}}, {{to}}, {{limit}}{keys}. A key \
             typed here instead of as {{name}} is saved and exported with the profile."
        ));
        FormRow::new("Headers").show(ui, |ui| header_rows(ui, &mut history.headers));
        let body = FormRow::new("Body").description(
            "Empty sends GET. Anything else is sent with POST, as application/json unless a \
             header names another content type.",
        );
        body.show(ui, |ui| {
            let field = TextEdit::multiline(&mut history.body).desired_rows(2);
            ui.add(field.desired_width(f32::INFINITY));
        });
        FormRow::new("Bounds").show(ui, |ui| {
            let forms = [
                RangeFormat::Seconds,
                RangeFormat::Milliseconds,
                RangeFormat::Microseconds,
                RangeFormat::Nanoseconds,
                RangeFormat::Iso8601,
                RangeFormat::Date,
                RangeFormat::CompactDate,
            ];
            let label = |form: &RangeFormat| range_label(form).to_owned();
            ui.combo_select("range_format", &mut history.range, forms, label);
            ui.weak("the form of {from} and {to}");
        });
        FormRow::new("Page size").show(ui, |ui| {
            ui.add(egui::DragValue::new(&mut history.page_size).range(1..=100_000));
            ui.weak("the most bars asked for in one request");
        });

        let ready = self.target().is_some();
        let waiting = self.history.pending.is_some();
        let mut test = false;
        ui.horizontal(|ui| {
            self.target_choices(ui);
            test = ui
                .add(Button::new("Test").primary().enabled(ready))
                .clicked();
            if waiting {
                ui.weak("waiting for the response");
            }
        });
        if test {
            self.test_history(ctx);
        }
        self.history_mapping(ui);
    }

    /// Sends the request a chart's first load sends. The answer arrives through
    /// `self.history.pending`.
    fn test_history(&mut self, ctx: &egui::Context) {
        let secrets = self.draft.secrets();
        self.report = findings(&self.draft.profile, &secrets, Scope::History);
        let Some((symbol, timeframe)) = self.target() else {
            return;
        };
        if !self.report.is_empty() {
            return;
        }
        let profile = &self.draft.profile;
        // The clock a chart in the profile's zone has.
        let clock = BarClock::new(timeframe).with_timezone(profile.time_zone.clone());
        let test = &mut self.history;
        test.proposed = false;
        test.error = None;
        // A Test in flight is superseded: its answer goes to a slot nothing reads.
        test.pending = None;
        match feed::latest(profile, &secrets, &symbol, timeframe, &clock, Utc::now()) {
            Err(error) => test.error = Some(error),
            Ok(request) => {
                let answer = Answer::default();
                let shown = conceal(endpoint(&request.url), &secrets);
                test.pending = Some((answer.clone(), shown));
                let repaint = ctx.clone();
                http::fetch(request, move |result| {
                    // A reason can repeat the endpoint, and a key in its path.
                    let result = result.map_err(|failure| match failure {
                        Failure::Refused(reason) => Failure::Refused(conceal(&reason, &secrets)),
                        failure => failure,
                    });
                    *answer.lock().unwrap_or_else(PoisonError::into_inner) = Some(result);
                    repaint.request_repaint();
                });
            }
        }
    }

    /// Takes the answer of the history Test when it has arrived.
    fn poll_history(&mut self) {
        let test = &mut self.history;
        let Some((answer, endpoint)) = &test.pending else {
            return;
        };
        let Some(result) = answer.lock().unwrap_or_else(PoisonError::into_inner).take() else {
            return;
        };
        test.endpoint.clone_from(endpoint);
        test.pending = None;
        let failed = |failure: Failure| format!("{}: {failure}", test.endpoint);
        let body = match result {
            Ok(body) => body,
            Err(failure) => {
                test.error = Some(failed(failure));
                return;
            }
        };
        let sample: Value = match serde_json::from_str(&body) {
            Ok(sample) => sample,
            Err(error) => {
                test.error = Some(failed(Failure::Unreadable(format!("not JSON: {error}"))));
                return;
            }
        };
        test.proposals = shape::propose(&sample);
        if let Some(history) = &mut self.draft.profile.history {
            // A mapping the visitor made stays. An empty one takes the most likely reading.
            let unmapped = history.rows == Rows::default() && history.bar == BarFields::default();
            if unmapped && let Some(first) = test.proposals.first() {
                history.rows = first.rows.clone();
                history.bar = first.bar.clone();
                test.proposed = true;
            }
            test.slot = Fields::bar(&mut history.bar).next(Slot::Rows);
        }
        test.sample = Some(sample);
        test.samples += 1;
    }

    /// The preview of the sample, its two trees and the slots of the mapping.
    fn history_mapping(&mut self, ui: &mut egui::Ui) {
        let zone = self.draft.profile.time_zone.clone();
        let timeframe = self
            .target()
            .map_or(Timeframe::Min1, |(_, timeframe)| timeframe);
        let test = &mut self.history;
        let Some(history) = &mut self.draft.profile.history else {
            return;
        };
        if let Some(sample) = &test.sample {
            let fresh = |preview: &Preview| {
                preview.history == *history
                    && preview.zone == zone
                    && preview.timeframe == timeframe
                    && preview.samples == test.samples
            };
            if !test.preview.as_ref().is_some_and(fresh) {
                let clock = BarClock::new(timeframe).with_timezone(zone.clone());
                let rows = shape::rows(sample, &history.rows);
                test.preview = Some(Preview {
                    history: history.clone(),
                    zone: zone.clone(),
                    timeframe,
                    samples: test.samples,
                    row: rows.ok().and_then(|rows| rows.into_iter().next()),
                    reading: feed::read(sample, history, &zone, &clock, timeframe, Utc::now()),
                });
            }
        }

        if let Some(error) = &test.error {
            Alert::error(error).show(ui);
        } else if test.proposed {
            Alert::info("Proposed mapping: confirm it or click the fields to use").show(ui);
        }
        match test.preview.as_ref().map(|preview| &preview.reading) {
            Some(Ok(reading)) => preview_table(ui, reading, &zone),
            Some(Err(error)) => {
                // Shown as a failed request is: the endpoint, then the reason.
                let failure = Failure::Unreadable(error.clone());
                Alert::error(format!("{}: {failure}", test.endpoint)).show(ui);
            }
            None => {}
        }
        let others = test
            .proposals
            .iter()
            .filter(|proposal| proposal.rows != history.rows);
        let others: Vec<&Proposal> = others.collect();
        let mut taken = None;
        if !others.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.weak("Other ways to read the response:");
                for proposal in others {
                    let text = format!(
                        "{} at {}, {}",
                        layout_label(&proposal.rows.layout),
                        place(&proposal.rows.path),
                        counted(proposal.count, "row")
                    );
                    if ui.add(Button::new(text).outlined().small()).clicked() {
                        taken = Some((proposal.rows.clone(), proposal.bar.clone()));
                    }
                }
            });
        }
        if let Some((rows, bar)) = taken {
            history.rows = rows;
            history.bar = bar;
        }

        let row = test
            .preview
            .as_ref()
            .and_then(|preview| preview.row.as_ref());
        let mut clicked_response = None;
        let mut clicked_row = None;
        if let Some(sample) = &test.sample {
            let mut response = vec![(history.rows.path.clone(), Slot::Rows.label())];
            if let Layout::Arrays {
                header: Some(header),
            } = &history.rows.layout
            {
                response.push((header.clone(), Slot::Header.label()));
            }
            let fields = Fields::bar(&mut history.bar).marks();
            let samples = test.samples;
            ui.columns(2, |columns| {
                columns[0].strong("Response");
                tree_area("response_tree").show(&mut columns[0], |ui| {
                    let id = egui::Id::unique(("data_source_response", samples));
                    clicked_response = tree::show(ui, id, sample, &response);
                });
                columns[1].strong("First row");
                match row {
                    Some(row) => {
                        tree_area("row_tree").show(&mut columns[1], |ui| {
                            let id = egui::Id::unique(("data_source_row", samples));
                            clicked_row = tree::show(ui, id, row, &fields);
                        });
                    }
                    None => {
                        columns[1].weak("The mapping finds no row.");
                    }
                }
            });
        } else {
            ui.weak("Test fetches the newest page and shows it here, to map by clicking.");
        }

        let mut clear = None;
        FormGrid::new("history_slots").columns(4).show(ui, |ui| {
            slot_cells(
                ui,
                Slot::Rows,
                &mut test.slot,
                &history.rows.path,
                true,
                test.sample.as_ref(),
            );
            ui.horizontal(|ui| {
                if clear_button(ui, !history.rows.path.is_empty()) {
                    clear = Some(Slot::Rows);
                }
                ui.weak(layout_label(&history.rows.layout));
            });
            ui.end_row();
            if let Layout::Arrays { header } = &history.rows.layout {
                let path = header.clone().unwrap_or_default();
                slot_cells(
                    ui,
                    Slot::Header,
                    &mut test.slot,
                    &path,
                    false,
                    test.sample.as_ref(),
                );
                if clear_button(ui, header.is_some()) {
                    clear = Some(Slot::Header);
                }
                ui.end_row();
            }
            Fields::bar(&mut history.bar).rows(ui, row, &zone, &mut test.slot);
        });
        match clear {
            Some(Slot::Rows) => history.rows.path.clear(),
            Some(_) => history.rows.layout = Layout::Arrays { header: None },
            None => {}
        }

        // A click fills the active slot when the node is in the tree of that slot.
        if let (Some(path), Some(sample)) = (clicked_response, &test.sample) {
            let filled = match test.slot {
                Slot::Rows => {
                    history.rows = rows_at(sample, path, &history.rows.layout);
                    true
                }
                Slot::Header => match &mut history.rows.layout {
                    Layout::Arrays { header } => {
                        *header = Some(path);
                        true
                    }
                    _ => false,
                },
                _ => false,
            };
            if filled {
                test.slot = Fields::bar(&mut history.bar).next(test.slot);
            }
        }
        if let (Some(path), Some(row)) = (clicked_row, row) {
            let mut fields = Fields::bar(&mut history.bar);
            if fields.slots().contains(&test.slot) {
                let value = at(row, &path).cloned().unwrap_or(Value::Null);
                fields.assign(test.slot, path, &value);
                test.slot = fields.next(test.slot);
            }
        }
    }

    /// The Live step: the connection, its Test, the log and the routes.
    fn live(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let keys = key_names(&self.draft.profile.keys);
        let mut on = self.draft.profile.stream.is_some();
        if ui.checkbox(&mut on, "Use a WebSocket stream").changed() {
            let draft = &mut self.draft;
            if on {
                draft.profile.stream = Some(draft.stream_off.take().unwrap_or_default());
            } else {
                draft.stream_off = draft.profile.stream.take();
                self.live.session = None;
            }
        }
        let Some(stream) = &mut self.draft.profile.stream else {
            return;
        };
        FormRow::new("URL").show(ui, |ui| {
            let field = TextEdit::singleline(&mut stream.url).hint_text("wss://stream.example.com");
            ui.add(field.desired_width(f32::INFINITY));
        });
        ui.hint_label(format!(
            "A browser sends no headers when it opens a WebSocket. Write {{name}} in the URL, \
             in a subprotocol or in a connect message to send the key of that name there. \
             Placeholders: {{symbol}}, {{interval}}{keys}. A key typed here instead of as \
             {{name}} is saved and exported with the profile."
        ));
        let protocols = FormRow::new("Subprotocols").description(
            "Offer one only when the service answers with it: a connection that offers \
             subprotocols ends when the server selects none.",
        );
        protocols.show(ui, |ui| {
            text_rows(ui, "protocol", &mut stream.protocols, "Add subprotocol")
        });
        let connect = FormRow::new("Connect messages").description(
            "Sent as typed, in this order, every time the socket opens: the login first, then \
             the subscribe message.",
        );
        connect.show(ui, |ui| {
            text_rows(ui, "connect", &mut stream.connect, "Add message")
        });
        let keep_alive = FormRow::new("Keep-alive")
            .description("A message and its period in seconds. An empty message sends none.");
        keep_alive.show(ui, |ui| {
            ui.add(TextEdit::singleline(&mut stream.keep_alive).desired_width(280.0));
            ui.add(egui::DragValue::new(&mut stream.keep_alive_every).range(1..=3600));
        });

        let ready = self.target().is_some();
        let running = self.live.session.is_some();
        let mut pressed = false;
        ui.horizontal(|ui| {
            self.target_choices(ui);
            let button = Button::new(if running { "Stop" } else { "Test" }).primary();
            pressed = ui.add(button.enabled(running || ready)).clicked();
        });
        if pressed && running {
            self.live.session = None;
        } else if pressed {
            self.test_live(ctx);
        }
        self.live_mapping(ui);
    }

    /// Starts a session as the feed does for a subscription. Its events arrive through
    /// `self.live.queue`.
    fn test_live(&mut self, ctx: &egui::Context) {
        let secrets = self.draft.secrets();
        self.report = findings(&self.draft.profile, &secrets, Scope::Stream);
        let profile = &self.draft.profile;
        let (Some((symbol, timeframe)), Some(stream)) = (self.target(), &profile.stream) else {
            return;
        };
        if !self.report.is_empty() {
            return;
        }
        let interval = profile.service_interval(timeframe).unwrap_or_default();
        let mut values = vec![
            ("symbol".to_owned(), symbol),
            ("interval".to_owned(), interval.to_owned()),
        ];
        values.extend(secrets.iter().cloned());
        let borrowed: Vec<(&str, &str)> = values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let url = profile::fill_url(&stream.url, &borrowed);
        // A new queue, so that an event of an earlier session cannot reach the new log.
        let queue = Events::default();
        let events = queue.clone();
        let repaint = ctx.clone();
        let shown = conceal(endpoint(&url), &secrets);
        let session = Session::open(stream.clone(), values, move |event| {
            // A reason can repeat the endpoint, and a key in its path.
            let event = match event {
                StreamEvent::Closed { reason, retry_in } => StreamEvent::Closed {
                    reason: conceal(&reason, &secrets),
                    retry_in,
                },
                event => event,
            };
            let mut events = events.lock().unwrap_or_else(PoisonError::into_inner);
            events.push((Utc::now(), event));
            repaint.request_repaint();
        });
        self.live = LiveTest {
            session: Some(session),
            queue,
            endpoint: shown,
            route: self.live.route,
            slot: self.live.slot,
            ..LiveTest::default()
        };
    }

    /// Moves the events of the session into the log and routes the log with the stream of the
    /// draft, so that it shows what the feed would do with the mapping as it stands.
    fn poll_live(&mut self) {
        let live = &mut self.live;
        let events =
            std::mem::take(&mut *live.queue.lock().unwrap_or_else(PoisonError::into_inner));
        let Some(stream) = &mut self.draft.profile.stream else {
            return;
        };
        for (at, event) in events {
            let note = |badge, text, fault| LineKind::Note { badge, text, fault };
            let kind = match event {
                StreamEvent::Open { again: false } => {
                    live.opened = true;
                    live.failure = None;
                    note("open", "connected, connect messages sent".to_owned(), false)
                }
                StreamEvent::Open { again: true } => note(
                    "open",
                    "reconnected, connect messages sent again".to_owned(),
                    false,
                ),
                StreamEvent::Message { text, .. } => {
                    let parsed = serde_json::from_str(&text);
                    let root = parsed.unwrap_or_else(|_| Value::String(text.clone()));
                    let routed = stream::route(stream, &text);
                    offer_conditions(stream, &routed);
                    // A new log selects its first message by itself.
                    if live.selected.is_none() {
                        live.selected = Some(live.lines);
                    }
                    LineKind::Message(Message { text, root, routed })
                }
                StreamEvent::Unreadable { reason } => note("unreadable", reason, true),
                StreamEvent::Closed { reason, retry_in } => {
                    let seconds = retry_in.as_secs();
                    let text = format!("{}: {reason}. Reconnecting in {seconds} s", live.endpoint);
                    if !live.opened {
                        live.failure = Some(text.clone());
                    }
                    note("closed", text, true)
                }
            };
            let number = live.lines;
            live.log.push_back(Line { number, at, kind });
            live.lines += 1;
            if live.log.len() > LOG_LINES {
                live.log.pop_front();
            }
        }

        // An edit of the place of the records, of the routing field or of a route: every line
        // is routed again, and offers its condition values again.
        let basis = basis(stream);
        if live.basis.as_ref() != Some(&basis) {
            for line in &mut live.log {
                if let LineKind::Message(message) = &mut line.kind {
                    message.routed = stream::route(stream, &message.text);
                    offer_conditions(stream, &message.routed);
                }
            }
            if let Some(message) = &mut live.unrouted {
                message.routed = stream::route(stream, &message.text);
            }
            live.basis = Some(basis);
        }

        // The oldest message with a record without a route stays in view until every record of
        // it has one. The next one then takes its place.
        if live
            .unrouted
            .as_ref()
            .is_some_and(|message| !message.unrouted())
        {
            live.unrouted = None;
        }
        if live.unrouted.is_none() {
            let lost = |line: &Line| match &line.kind {
                LineKind::Message(message) if message.unrouted() => Some(message.clone()),
                _ => None,
            };
            live.unrouted = live.log.iter().find_map(lost);
        }
    }

    /// The first message without a route, the log, its two trees, the routes and their slots.
    fn live_mapping(&mut self, ui: &mut egui::Ui) {
        let zone = self.draft.profile.time_zone.clone();
        let timeframe = self
            .target()
            .map_or(Timeframe::Min1, |(_, timeframe)| timeframe);
        let live = &mut self.live;
        let Some(stream) = &mut self.draft.profile.stream else {
            return;
        };
        if let Some(failure) = &live.failure {
            Alert::error(failure).show(ui);
        }

        // The first message without a route.
        let mut added = None;
        if let Some(message) = &live.unrouted {
            let text = cut(&message.text, 300);
            Alert::warning(format!("No route for this message:\n{text}")).show(ui);
            let lost = message.routed.iter().find_map(|record| match record {
                Routed::Unrouted { record } => Some(record),
                _ => None,
            });
            ui.horizontal_wrapped(|ui| {
                if stream.route_by.is_empty() {
                    let button = Button::new("Route every message here").outlined().small();
                    if ui.add(button).clicked() {
                        added = Some(String::new());
                    }
                    ui.weak("or click the field that tells the kinds of message apart");
                    return;
                }
                match lost.and_then(|record| routing_value(stream, &message.root, record)) {
                    Some(value) => {
                        let text = format!("Add route for \"{value}\"");
                        if ui.add(Button::new(text).outlined().small()).clicked() {
                            added = Some(value);
                        }
                    }
                    None => {
                        let field = printed(&stream.route_by);
                        ui.weak(format!("The record has no field {field}."));
                    }
                }
            });
        }
        if let Some(value) = added {
            // The new route ignores its records until the visitor chooses its kind.
            let kind = RouteKind::Ignore;
            stream.routes.push(Route { value, kind });
            live.route = stream.routes.len() - 1;
        }

        ui.strong("Log");
        let mut selected = None;
        let log = egui::ScrollArea::vertical()
            .id_salt("data_source_log")
            .max_height(LOG)
            .min_scrolled_height(LOG);
        log.auto_shrink([false, true])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if live.log.is_empty() {
                    let waiting = live.session.is_some();
                    ui.weak(if waiting {
                        "Waiting for the connection."
                    } else {
                        "Test connects and lists every message here."
                    });
                }
                let fault = ui.active_theme().ui.error;
                for line in &live.log {
                    ui.horizontal(|ui| {
                        let arrived = line.at.with_timezone(&zone).naive_local();
                        ui.weak(arrived.format("%H:%M:%S").to_string());
                        match &line.kind {
                            LineKind::Note {
                                badge,
                                text,
                                fault: true,
                            } => {
                                ui.label(RichText::new(*badge).small().color(fault));
                                ui.add(egui::Label::new(text.as_str()).truncate());
                            }
                            LineKind::Note { badge, text, .. } => {
                                ui.label(RichText::new(*badge).small());
                                ui.add(egui::Label::new(text.as_str()).truncate());
                            }
                            LineKind::Message(message) => {
                                for (text, dim) in badges(stream, &message.routed) {
                                    let badge = RichText::new(text).small();
                                    ui.label(if dim { badge.weak() } else { badge.strong() });
                                }
                                let text = RichText::new(message.text.as_str()).monospace();
                                let chosen = live.selected == Some(line.number);
                                let button = egui::Button::new(text).frame(false).truncate();
                                if ui.add(button.selected(chosen)).clicked() {
                                    selected = Some(line.number);
                                }
                            }
                        }
                    });
                }
            });
        if let Some(number) = selected {
            live.selected = Some(number);
            live.record = 0;
        }

        // The selected message, and its records as a session splits it.
        let chosen = |line: &&Line| Some(line.number) == live.selected;
        let message = live
            .log
            .iter()
            .find(chosen)
            .and_then(|line| match &line.kind {
                LineKind::Message(message) => Some((line.number, message)),
                LineKind::Note { .. } => None,
            });
        let records = message.map_or_else(Vec::new, |(_, message)| split(stream, &message.text));
        live.record = live.record.min(records.len().saturating_sub(1));
        let shown = live.record;
        let record = records.get(shown);
        live.route = live.route.min(stream.routes.len().saturating_sub(1));

        let mut in_message = vec![(stream.records.clone(), Slot::Records.label())];
        let mut in_record = Vec::new();
        if !stream.route_by.is_empty() {
            in_message.push((stream.route_by.clone(), Slot::RouteBy.label()));
            in_record.push((stream.route_by.clone(), Slot::RouteBy.label()));
        }
        let route = stream.routes.get_mut(live.route);
        if let Some(fields) = route.and_then(|route| Fields::route(&mut route.kind)) {
            in_record.extend(fields.marks());
        }
        let mut clicked_message = None;
        let mut clicked_record = None;
        let mut step = 0_isize;
        ui.columns(2, |columns| {
            columns[0].strong("Message");
            match message {
                Some((number, message)) => {
                    tree_area("message_tree").show(&mut columns[0], |ui| {
                        let id = egui::Id::unique(("data_source_message", number));
                        clicked_message = tree::show(ui, id, &message.root, &in_message);
                    });
                }
                None => {
                    columns[0].weak("Select a message of the log.");
                }
            }
            columns[1].strong("Record");
            if records.len() > 1 {
                columns[1].horizontal(|ui| {
                    ui.label(format!("record {} of {}", shown + 1, records.len()));
                    if ui
                        .add(Button::new("Previous").small().enabled(shown > 0))
                        .clicked()
                    {
                        step = -1;
                    }
                    let more = shown + 1 < records.len();
                    if ui.add(Button::new("Next").small().enabled(more)).clicked() {
                        step = 1;
                    }
                });
            }
            if let (Some((number, _)), Some(record)) = (message, record) {
                tree_area("record_tree").show(&mut columns[1], |ui| {
                    let id = egui::Id::unique(("data_source_record", number, shown));
                    clicked_record = tree::show(ui, id, record, &in_record);
                });
            }
        });
        live.record = shown.saturating_add_signed(step);

        let root = message.map(|(_, message)| &message.root);
        let mut clear = None;
        FormGrid::new("live_slots").columns(4).show(ui, |ui| {
            slot_cells(
                ui,
                Slot::Records,
                &mut live.slot,
                &stream.records,
                true,
                root,
            );
            if clear_button(ui, !stream.records.is_empty()) {
                clear = Some(Slot::Records);
            }
            ui.end_row();
            // The routing field is looked up in the record and then in the message.
            let holder = record.filter(|record| at(record, &stream.route_by).is_some());
            let base = holder.or(root);
            slot_cells(
                ui,
                Slot::RouteBy,
                &mut live.slot,
                &stream.route_by,
                false,
                base,
            );
            if clear_button(ui, !stream.route_by.is_empty()) {
                clear = Some(Slot::RouteBy);
            }
            ui.end_row();
        });
        match clear {
            Some(Slot::Records) => stream.records.clear(),
            Some(_) => stream.route_by.clear(),
            None => {}
        }

        ui.strong("Routes");
        let mut removed = None;
        for (index, route) in stream.routes.iter_mut().enumerate() {
            ui.push_id(("route", index), |ui| {
                ui.horizontal(|ui| {
                    let name = format!("Route {}", index + 1);
                    if ui
                        .add(Button::new(name).small().selected(live.route == index))
                        .clicked()
                    {
                        live.route = index;
                    }
                    let value = TextEdit::singleline(&mut route.value);
                    ui.add(
                        value
                            .hint_text("value of the routing field")
                            .desired_width(180.0),
                    );
                    let kinds = [
                        ("Ignore", RouteKind::Ignore),
                        ("Bars", RouteKind::Bars(BarFields::default())),
                        ("Trades", RouteKind::Trades(TradeFields::default())),
                    ];
                    for (label, kind) in kinds {
                        let held =
                            std::mem::discriminant(&route.kind) == std::mem::discriminant(&kind);
                        if ui.add(Button::new(label).small().selected(held)).clicked() && !held {
                            route.kind = kind;
                            live.route = index;
                            // The fields of the new kind are mapped next.
                            let fields = Fields::route(&mut route.kind);
                            live.slot =
                                fields.map_or(live.slot, |fields| fields.next(Slot::Records));
                        }
                    }
                    if ui.add(Button::new("Remove").small()).clicked() {
                        removed = Some(index);
                    }
                });
            });
        }
        if let Some(index) = removed {
            stream.routes.remove(index);
        }
        if ui
            .add(Button::new("Add route").outlined().small())
            .clicked()
        {
            stream.routes.push(Route::default());
            live.route = stream.routes.len() - 1;
        }

        // The slots of the selected route, and what the feed reads from the record shown.
        if let Some(route) = stream.routes.get_mut(live.route) {
            if let Some(mut fields) = Fields::route(&mut route.kind) {
                FormGrid::new("route_slots").columns(4).show(ui, |ui| {
                    fields.rows(ui, record, &zone, &mut live.slot);
                });
            }
            if let Some(record) = record {
                let clock = BarClock::new(timeframe).with_timezone(zone.clone());
                let read = match &route.kind {
                    RouteKind::Ignore => None,
                    RouteKind::Bars(fields) => {
                        let bar = bars::bar(record, fields, &zone, &clock, timeframe);
                        Some(bar.map(|bar| bar_text(&bar, &zone)))
                    }
                    RouteKind::Trades(fields) => {
                        let trade = bars::trade(record, fields, &zone);
                        Some(trade.map(|trade| {
                            let at = trade.time.with_timezone(&zone).naive_local();
                            format!(
                                "trade at {}, price {}, size {}, {}",
                                at.format("%Y-%m-%d %H:%M:%S%.3f"),
                                number(trade.price),
                                number(trade.size),
                                rule_label(trade.rule)
                            )
                        }))
                    }
                };
                if let Some(read) = read {
                    let read = read.unwrap_or_else(|| "not readable".to_owned());
                    ui.label(format!("The feed reads: {read}"));
                }
            }
            if let RouteKind::Trades(fields) = &mut route.kind
                && !fields.condition.is_empty()
            {
                condition_rows(ui, &mut fields.conditions, &mut live.condition);
            }
        }

        // A click fills the active slot when the node is in the tree of that slot.
        let mut filled = false;
        if let (Some(path), Some((_, message))) = (clicked_message, message) {
            match live.slot {
                Slot::Records => {
                    stream.records = path;
                    filled = true;
                }
                Slot::RouteBy if above_records(&stream.records, &message.root, &path) => {
                    stream.route_by = path;
                    filled = true;
                }
                _ => {}
            }
        }
        if let (Some(path), Some(record)) = (clicked_record, record) {
            let route = stream.routes.get_mut(live.route);
            let fields = route.and_then(|route| Fields::route(&mut route.kind));
            match (live.slot, fields) {
                (Slot::RouteBy, _) => {
                    stream.route_by = path;
                    filled = true;
                }
                (slot, Some(mut fields)) if fields.slots().contains(&slot) => {
                    let value = at(record, &path).cloned().unwrap_or(Value::Null);
                    fields.assign(slot, path, &value);
                    live.slot = fields.next(slot);
                }
                _ => {}
            }
        }
        if filled {
            // The fields of the selected route are mapped next.
            let route = stream.routes.get_mut(live.route);
            if let Some(fields) = route.and_then(|route| Fields::route(&mut route.kind)) {
                live.slot = fields.next(live.slot);
            }
        }
    }
}

/// What keeps the templates of `profile` from being sent as they stand, one sentence per
/// finding: the name of a key that has no value, a name in braces that is neither a
/// placeholder nor a key, and a placeholder of history requests in a template of the stream.
/// Braces around any other text, such as those of a JSON message, are not names.
fn findings(profile: &Profile, secrets: &Secrets, scope: Scope) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut scan = |template: &str, place: &str, stream: bool| {
        for name in names(template) {
            let finding = if profile.keys.iter().any(|key| key == name) {
                let value = secrets.iter().find(|(key, _)| key == name);
                if value.is_some_and(|(_, value)| !value.is_empty()) {
                    continue;
                }
                format!("The key \"{name}\" has no value. Enter it under Source.")
            } else if !PLACEHOLDERS.contains(&name) {
                format!(
                    "\"{{{name}}}\" in the {place} is neither a placeholder nor a key of this \
                     profile. Add the key under Source or correct the text."
                )
            } else if stream && matches!(name, "from" | "to" | "limit") {
                format!("\"{{{name}}}\" in the {place} is filled for history requests only.")
            } else {
                continue;
            };
            if !found.contains(&finding) {
                found.push(finding);
            }
        }
    };
    if let Some(history) = profile.history.as_ref().filter(|_| scope != Scope::Stream) {
        scan(&history.url, "history URL", false);
        for (name, value) in &history.headers {
            scan(value, &format!("history header {name}"), false);
        }
        scan(&history.body, "history body", false);
    }
    if let Some(stream) = profile.stream.as_ref().filter(|_| scope != Scope::History) {
        scan(&stream.url, "stream URL", true);
        for (index, protocol) in stream.protocols.iter().enumerate() {
            scan(protocol, &format!("subprotocol {}", index + 1), true);
        }
        for (index, message) in stream.connect.iter().enumerate() {
            scan(message, &format!("connect message {}", index + 1), true);
        }
        scan(&stream.keep_alive, "keep-alive message", true);
    }
    found
}

/// The names a template refers to as `{name}`: lower-case letters, digits and `_` between
/// braces, which is the alphabet of key names.
fn names(template: &str) -> impl Iterator<Item = &str> {
    template.split('{').skip(1).filter_map(|after| {
        let (name, _) = after.split_once('}')?;
        let plain = |byte: u8| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_');
        (!name.is_empty() && name.bytes().all(plain)).then_some(name)
    })
}

/// The key names as the hint line of a step lists them after the placeholders.
fn key_names(keys: &[String]) -> String {
    let mut names = String::new();
    for key in keys.iter().filter(|key| !key.is_empty()) {
        // Writing into a `String` cannot fail.
        let _ = write!(names, ", {{{key}}}");
    }
    names
}

/// The entries of a text split at commas and line ends: each one trimmed, empty and repeated
/// ones dropped.
fn listed(text: &str) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    for entry in text.split([',', '\n', '\r']).map(str::trim) {
        if !entry.is_empty() && !entries.iter().any(|held| held == entry) {
            entries.push(entry.to_owned());
        }
    }
    entries
}

/// A path as the dialog prints it: `data.candles`, `[0]`, `t`.
fn printed(path: &[Segment]) -> String {
    let mut text = String::new();
    for segment in path {
        match segment {
            Segment::Key(key) => {
                if !text.is_empty() {
                    text.push('.');
                }
                text.push_str(key);
            }
            Segment::Index(index) => {
                // Writing into a `String` cannot fail.
                let _ = write!(text, "[{index}]");
            }
        }
    }
    text
}

/// Where a path leads, in words: the path, or the root for an empty one.
fn place(path: &[Segment]) -> String {
    if path.is_empty() {
        "the root".to_owned()
    } else {
        printed(path)
    }
}

/// `count` with its noun: `1 bar`, `3 bars`.
fn counted(count: usize, noun: &str) -> String {
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {noun}{plural}")
}

/// The first `most` characters of `text`, with an ellipsis when it is longer.
fn cut(text: &str, most: usize) -> String {
    if text.chars().count() <= most {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(most).collect();
    cut.push('…');
    cut
}

/// A value in a few characters: a scalar as JSON prints it, a list or an object as its size.
fn brief(value: &Value) -> String {
    match value {
        Value::Array(list) => format!("[{}]", list.len()),
        Value::Object(members) => format!("{{{}}}", members.len()),
        scalar => cut(&scalar.to_string(), 40),
    }
}

/// A price or a size as the preview prints it. A value that could not be read is NaN.
fn number(value: f64) -> String {
    if value.is_nan() {
        "missing".to_owned()
    } else {
        value.to_string()
    }
}

/// A bar in one line, its time in `zone`.
fn bar_text(bar: &Bar, zone: &TimezoneId) -> String {
    let at = bar.time.with_timezone(zone).naive_local();
    let [open, high, low, close, volume] =
        [bar.open, bar.high, bar.low, bar.close, bar.volume].map(number);
    format!(
        "bar at {}, open {open}, high {high}, low {low}, close {close}, volume {volume}",
        at.format("%Y-%m-%d %H:%M:%S")
    )
}

/// What `time::stamp` reads from `record`: an instant in `zone` and in UTC, such as
/// `2026-11-11 09:30:00 America/New_York, 14:30:00 UTC`, so that a wrong unit or zone shows
/// before anything is charted.
fn stamp_text(record: &Value, spec: &TimeSpec, zone: &TimezoneId) -> String {
    match time::stamp(record, spec, zone) {
        Some(Stamp::At(instant)) => {
            let local = instant.with_timezone(zone).naive_local();
            format!(
                "{} {}, {} UTC",
                local.format("%Y-%m-%d %H:%M:%S"),
                zone.name(),
                instant.format("%H:%M:%S")
            )
        }
        Some(Stamp::On(date)) => format!("{date}, a bar dated on that day"),
        None => "not readable".to_owned(),
    }
}

fn range_label(form: &RangeFormat) -> &'static str {
    match form {
        RangeFormat::Seconds => "Unix seconds",
        RangeFormat::Milliseconds => "Unix milliseconds",
        RangeFormat::Microseconds => "Unix microseconds",
        RangeFormat::Nanoseconds => "Unix nanoseconds",
        RangeFormat::Iso8601 => "ISO 8601, 2026-11-11T14:30:00Z",
        RangeFormat::Date => "Date, 2026-11-11",
        RangeFormat::CompactDate => "Date, 20261111",
    }
}

fn clock_label(unit: &ClockUnit) -> &'static str {
    match unit {
        ClockUnit::Milliseconds => "Milliseconds since midnight",
        ClockUnit::Seconds => "Seconds since midnight",
        ClockUnit::Text => "Text HH:MM:SS",
    }
}

fn layout_label(layout: &Layout) -> &'static str {
    match layout {
        Layout::Objects => "list of objects",
        Layout::Arrays { header: None } => "list of arrays",
        Layout::Arrays { header: Some(_) } => "list of arrays with a header",
        Layout::Columns => "object of arrays",
    }
}

fn rule_label(rule: ConditionRule) -> &'static str {
    match rule {
        ConditionRule::PriceAndVolume => "Price and volume",
        ConditionRule::VolumeOnly => "Volume only",
        ConditionRule::Ignore => "Ignore",
    }
}

/// The rows at the node `path` of `sample`, laid out as the node says: an object holds one
/// list per field, a list of lists holds arrays, anything else holds objects. A header that
/// was chosen stays with a list of lists.
fn rows_at(sample: &Value, path: FieldPath, held: &Layout) -> Rows {
    let layout = match at(sample, &path) {
        Some(Value::Object(_)) => Layout::Columns,
        Some(Value::Array(list)) if matches!(list.first(), Some(Value::Array(_))) => match held {
            Layout::Arrays { header } => Layout::Arrays {
                header: header.clone(),
            },
            _ => Layout::Arrays { header: None },
        },
        _ => Layout::Objects,
    };
    Rows { path, layout }
}

/// The records of a message, as a session splits it. The routes are left out, so that every
/// record comes back, also one that a route would ignore.
fn split(stream: &Stream, message: &str) -> Vec<Value> {
    let unrouted = Stream {
        records: stream.records.clone(),
        ..Stream::default()
    };
    let record = |routed: Routed| match routed {
        Routed::Unrouted { record } | Routed::Record { record, .. } => Some(record),
        Routed::Ignored { .. } => None,
    };
    stream::route(&unrouted, message)
        .into_iter()
        .filter_map(record)
        .collect()
}

/// The value a session routes `record` by: its own, then the one at the root of its message,
/// then the record itself when it is a bare value.
fn routing_value(stream: &Stream, root: &Value, record: &Value) -> Option<String> {
    let own = at(record, &stream.route_by).and_then(profile::text);
    let named = own.or_else(|| at(root, &stream.route_by).and_then(profile::text));
    named.or_else(|| profile::text(record))
}

/// Whether the node at `path` of a message can be the routing field. A node at or below the
/// place of the records cannot: its path would name one record of this message.
fn above_records(records: &[Segment], root: &Value, path: &[Segment]) -> bool {
    if records.is_empty() {
        // The message is the record, or the list of records.
        !root.is_array()
    } else {
        !path.starts_with(records)
    }
}

/// What decides the route of a record and the condition values it offers: the place of the
/// records, the routing field and the routes without their rule lists.
fn basis(stream: &Stream) -> Basis {
    let mut routes = stream.routes.clone();
    for route in &mut routes {
        if let RouteKind::Trades(fields) = &mut route.kind {
            fields.conditions.clear();
        }
    }
    (stream.records.clone(), stream.route_by.clone(), routes)
}

/// Adds the condition values of the records in `routed` to the rule list of their trades
/// route, as `Price and volume`, in the order first seen.
fn offer_conditions(stream: &mut Stream, routed: &[Routed]) {
    for record in routed {
        let Routed::Record { route, record } = record else {
            continue;
        };
        let Some(RouteKind::Trades(fields)) =
            stream.routes.get_mut(*route).map(|route| &mut route.kind)
        else {
            continue;
        };
        for value in bars::conditions(record, fields) {
            if !fields.conditions.iter().any(|(listed, _)| *listed == value) {
                fields
                    .conditions
                    .push((value, ConditionRule::PriceAndVolume));
            }
        }
    }
}

/// The badges of a message: one per distinct outcome of its records, in message order, with
/// the count when several records share it. The flag dims the badge of a route that ignores.
fn badges(stream: &Stream, routed: &[Routed]) -> Vec<(String, bool)> {
    let mut outcomes: Vec<(Option<usize>, bool, usize)> = Vec::new();
    for record in routed {
        let (route, ignored) = match record {
            Routed::Record { route, .. } => (Some(*route), false),
            Routed::Ignored { route } => (Some(*route), true),
            Routed::Unrouted { .. } => (None, false),
        };
        match outcomes.iter_mut().find(|(held, ..)| *held == route) {
            Some((.., count)) => *count += 1,
            None => outcomes.push((route, ignored, 1)),
        }
    }
    if outcomes.is_empty() {
        return vec![("no records".to_owned(), true)];
    }
    let badge = |(route, ignored, count): (Option<usize>, bool, usize)| {
        let value = route.and_then(|route| Some((route, stream.routes.get(route)?)));
        let mut text = match value {
            Some((route, held)) if held.value.is_empty() => format!("route {}", route + 1),
            Some((_, held)) => held.value.clone(),
            None => "no route".to_owned(),
        };
        if count > 1 {
            // Writing into a `String` cannot fail.
            let _ = write!(text, " ×{count}");
        }
        (text, ignored)
    };
    outcomes.into_iter().map(badge).collect()
}

/// The first three cells of a slot's row: its button, the path it holds and the value found
/// there below `base`. `whole` says that an empty path is the whole of `base`, as for the
/// place of the rows; for a field it means that nothing is mapped.
fn slot_cells(
    ui: &mut egui::Ui,
    slot: Slot,
    active: &mut Slot,
    path: &[Segment],
    whole: bool,
    base: Option<&Value>,
) {
    let button = Button::new(slot.label()).outlined().small();
    if ui.add(button.selected(*active == slot)).clicked() {
        *active = slot;
    }
    if path.is_empty() && !whole {
        ui.weak("not mapped");
        ui.label("");
        return;
    }
    ui.monospace(place(path));
    match base.map(|base| at(base, path)) {
        Some(Some(found)) => ui.monospace(brief(found)),
        Some(None) => ui.warning_label("nothing there"),
        None => ui.label(""),
    };
}

/// The `Clear` button of a slot. Returns whether it was clicked.
fn clear_button(ui: &mut egui::Ui, enabled: bool) -> bool {
    ui.add(Button::new("Clear").small().enabled(enabled))
        .clicked()
}

/// A tree's own scroll area, which fills its column.
fn tree_area(salt: &str) -> egui::ScrollArea {
    let area = egui::ScrollArea::both().id_salt(salt);
    let area = area.max_height(TREE).min_scrolled_height(TREE);
    area.auto_shrink([false, true])
}

/// The first five bars of a reading and the last one, then how many it holds.
fn preview_table(ui: &mut egui::Ui, reading: &Reading, zone: &TimezoneId) {
    let bars = &reading.bars;
    let cut = bars.len() > 6;
    let shown: Vec<&Bar> = if cut {
        bars[..5].iter().chain(bars.last()).collect()
    } else {
        bars.iter().collect()
    };
    FormGrid::new("history_preview").columns(6).show(ui, |ui| {
        for heading in ["time", "open", "high", "low", "close", "volume"] {
            ui.weak(heading);
        }
        ui.end_row();
        for (index, bar) in shown.into_iter().enumerate() {
            if cut && index == 5 {
                ui.weak("…");
                ui.end_row();
            }
            let at = bar.time.with_timezone(zone).naive_local();
            ui.monospace(at.format("%Y-%m-%d %H:%M:%S").to_string());
            for value in [bar.open, bar.high, bar.low, bar.close, bar.volume] {
                ui.monospace(number(value));
            }
            ui.end_row();
        }
    });
    let skipped = counted(reading.skipped, "row");
    ui.label(format!("{}, {skipped} skipped", counted(bars.len(), "bar")));
}

/// The interval table: per row a chart interval and the text the service expects for it.
fn interval_rows(ui: &mut egui::Ui, rows: &mut Vec<Interval>) {
    ui.vertical(|ui| {
        let mut removed = None;
        for (index, row) in rows.iter_mut().enumerate() {
            ui.push_id(("interval", index), |ui| {
                ui.horizontal(|ui| {
                    let label = |chart: &Timeframe| chart.to_string();
                    ui.combo_select("chart", &mut row.chart, Timeframe::all(), label);
                    let service = TextEdit::singleline(&mut row.service);
                    ui.add(
                        service
                            .hint_text("as the service spells it")
                            .desired_width(160.0),
                    );
                    if ui.add(Button::new("Remove").small()).clicked() {
                        removed = Some(index);
                    }
                });
            });
        }
        if let Some(index) = removed {
            rows.remove(index);
        }
        if ui
            .add(Button::new("Add interval").outlined().small())
            .clicked()
        {
            // The first interval the table does not list yet, from one minute on.
            let all = Timeframe::all();
            let minute = all.iter().position(|chart| *chart == Timeframe::Min1);
            let mut from_minute = all.iter().cycle().skip(minute.unwrap_or(0)).take(all.len());
            let unused = |chart: &&Timeframe| !rows.iter().any(|row| row.chart == **chart);
            let chart = from_minute.find(unused).copied().unwrap_or_default();
            let service = String::new();
            rows.push(Interval { chart, service });
        }
    });
}

/// The keys: per row a name and a value, which is typed as a password.
fn key_rows(ui: &mut egui::Ui, keys: &mut Vec<String>, values: &mut Vec<String>) {
    ui.vertical(|ui| {
        let mut removed = None;
        for (index, (name, value)) in keys.iter_mut().zip(values.iter_mut()).enumerate() {
            ui.push_id(("key", index), |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        TextEdit::singleline(name)
                            .hint_text("key name")
                            .desired_width(140.0),
                    );
                    let value = TextEdit::singleline(value).hint_text("key value");
                    ui.add(value.password(true).desired_width(240.0));
                    if ui.add(Button::new("Remove").small()).clicked() {
                        removed = Some(index);
                    }
                });
            });
        }
        if let Some(index) = removed {
            keys.remove(index);
            values.remove(index);
        }
        if ui.add(Button::new("Add key").outlined().small()).clicked() {
            keys.push(String::new());
            values.push(String::new());
        }
    });
}

/// The request headers: per row a name and a value template.
fn header_rows(ui: &mut egui::Ui, rows: &mut Vec<(String, String)>) {
    ui.vertical(|ui| {
        let mut removed = None;
        for (index, (name, value)) in rows.iter_mut().enumerate() {
            ui.push_id(("header", index), |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        TextEdit::singleline(name)
                            .hint_text("name")
                            .desired_width(160.0),
                    );
                    let width = (ui.available_width() - 90.0).max(80.0);
                    ui.add(
                        TextEdit::singleline(value)
                            .hint_text("value")
                            .desired_width(width),
                    );
                    if ui.add(Button::new("Remove").small()).clicked() {
                        removed = Some(index);
                    }
                });
            });
        }
        if let Some(index) = removed {
            rows.remove(index);
        }
        if ui
            .add(Button::new("Add header").outlined().small())
            .clicked()
        {
            rows.push((String::new(), String::new()));
        }
    });
}

/// Rows of text that can be added and removed: the subprotocols and the connect messages.
fn text_rows(ui: &mut egui::Ui, salt: &str, rows: &mut Vec<String>, add: &str) {
    ui.vertical(|ui| {
        let mut removed = None;
        for (index, row) in rows.iter_mut().enumerate() {
            ui.push_id((salt, index), |ui| {
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 90.0).max(80.0);
                    ui.add(TextEdit::singleline(row).desired_width(width));
                    if ui.add(Button::new("Remove").small()).clicked() {
                        removed = Some(index);
                    }
                });
            });
        }
        if let Some(index) = removed {
            rows.remove(index);
        }
        if ui.add(Button::new(add).outlined().small()).clicked() {
            rows.push(String::new());
        }
    });
}

/// The rule of each condition value of a trades route, and a row that adds a value by hand,
/// for one that did not occur during the Test.
fn condition_rows(ui: &mut egui::Ui, rules: &mut Vec<(String, ConditionRule)>, typed: &mut String) {
    ui.strong("Conditions");
    let mut removed = None;
    for (index, (value, rule)) in rules.iter_mut().enumerate() {
        ui.push_id(("condition", index), |ui| {
            ui.horizontal(|ui| {
                ui.monospace(value.as_str());
                let choices = [
                    ConditionRule::PriceAndVolume,
                    ConditionRule::VolumeOnly,
                    ConditionRule::Ignore,
                ];
                for choice in choices {
                    let button = Button::new(rule_label(choice)).small();
                    if ui.add(button.selected(*rule == choice)).clicked() {
                        *rule = choice;
                    }
                }
                if ui.add(Button::new("Remove").small()).clicked() {
                    removed = Some(index);
                }
            });
        });
    }
    if let Some(index) = removed {
        rules.remove(index);
    }
    ui.horizontal(|ui| {
        ui.add(
            TextEdit::singleline(typed)
                .hint_text("condition value")
                .desired_width(140.0),
        );
        let new = !typed.is_empty() && !rules.iter().any(|(value, _)| value == typed);
        if ui
            .add(Button::new("Add condition").outlined().small().enabled(new))
            .clicked()
        {
            rules.push((std::mem::take(typed), ConditionRule::PriceAndVolume));
        }
    });
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    use egui::accesskit;
    use egui_charts::ChartBuilder;
    use egui_charts::widget::SeriesId;

    use super::super::super::tests::AppStorage;
    use super::super::feed::MappedFeed;
    use super::super::feed::tests::{Server, pages, profile, shown, until};
    use super::super::stream::tests::{Peer, Step};
    use super::*;

    const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

    /// The headings of the log and of the trees, which scroll on their own below them.
    const HEADINGS: [&str; 5] = ["Log", "Response", "First row", "Message", "Record"];

    /// A control or a label of the dialog, whether or not it is scrolled into view.
    #[derive(Clone)]
    struct Node {
        id: accesskit::NodeId,
        role: accesskit::Role,
        /// The label of a control, the text of a label or the content of a field.
        text: String,
        /// The hint of a field.
        hint: String,
        rect: egui::Rect,
    }

    impl Node {
        /// Whether the node is a field that is typed into.
        fn field(&self) -> bool {
            use accesskit::Role;
            let fields = [
                Role::TextInput,
                Role::MultilineTextInput,
                Role::PasswordInput,
            ];
            fields.contains(&self.role)
        }
    }

    /// A text as it is painted: the text, its place and the part of the screen it is cut to.
    type Painted = (String, egui::Rect, egui::Rect);

    /// One frame of the dialog.
    struct Screen {
        /// The texts painted at least partly where they can be seen.
        texts: Vec<Painted>,
        nodes: Vec<Node>,
        /// What the user decided in the frame.
        outcome: Option<Outcome>,
    }

    impl Screen {
        /// Where `node` can be clicked: the middle of the part of its text that is in view,
        /// when enough of it is to be hit.
        fn visible(&self, node: &Node) -> Option<egui::Pos2> {
            let area = node.rect.expand(2.0);
            let own = |(text, rect, _): &&Painted| {
                (*text == node.text || *text == node.hint) && area.contains(rect.center())
            };
            let (_, rect, clip) = self.texts.iter().find(own)?;
            let shown = rect.intersect(*clip);
            (shown.width() >= 4.0 && shown.height() >= 4.0).then(|| shown.center())
        }

        /// Every text of the dialog, also what is scrolled out of view.
        fn all(&self) -> Vec<String> {
            let painted = self.texts.iter().map(|(text, ..)| text.clone());
            let held = self
                .nodes
                .iter()
                .flat_map(|node| [node.text.clone(), node.hint.clone()]);
            painted.chain(held).collect()
        }
    }

    /// Adds the texts of `shape` that reach into `clip`.
    fn painted(shape: &egui::Shape, clip: egui::Rect, texts: &mut Vec<Painted>) {
        match shape {
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    painted(shape, clip, texts);
                }
            }
            egui::Shape::Text(text) => {
                let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                if clip.intersects(rect) {
                    texts.push((text.galley.text().to_owned(), rect, clip));
                }
            }
            _ => {}
        }
    }

    /// One frame of the dialog with `events`.
    fn draw(
        ctx: &egui::Context,
        dialog: &mut DataSourceDialog,
        events: Vec<egui::Event>,
    ) -> Screen {
        ctx.enable_accesskit();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
            events,
            ..Default::default()
        };
        let mut outcome = None;
        let output = ctx.run_ui(input, |ui| {
            if let Some(decided) = dialog.show(ui.ctx(), None) {
                outcome = Some(decided);
            }
        });
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            painted(&clipped.shape, clipped.clip_rect, &mut texts);
        }
        let listed = |(id, node): &(accesskit::NodeId, accesskit::Node)| {
            // A label also lists each of its lines as a run of text, which is no widget.
            if node.role() == accesskit::Role::TextRun {
                return None;
            }
            let bounds = node.bounds()?;
            let corner = |x: f64, y: f64| egui::pos2(x as f32, y as f32);
            Some(Node {
                id: *id,
                role: node.role(),
                text: node.label().or(node.value()).unwrap_or_default().to_owned(),
                hint: node.placeholder().unwrap_or_default().to_owned(),
                rect: egui::Rect::from_min_max(
                    corner(bounds.x0, bounds.y0),
                    corner(bounds.x1, bounds.y1),
                ),
            })
        };
        let update = output.platform_output.accesskit_update.as_ref();
        let nodes = update.map_or_else(Vec::new, |update| {
            update.nodes.iter().filter_map(listed).collect()
        });
        output.drop_without_applying_deltas();
        Screen {
            texts,
            nodes,
            outcome,
        }
    }

    /// Asks the scroll area around `node` to bring it into view. The area moves in the frame
    /// after the one that asks, and draws its content at the new place in the frame after that.
    fn scroll_to(ctx: &egui::Context, dialog: &mut DataSourceDialog, node: &Node) {
        let request = egui::Event::AccessKitActionRequest(accesskit::ActionRequest {
            action: accesskit::Action::ScrollIntoView,
            target_tree: accesskit::TreeId::ROOT,
            target_node: node.id,
            data: None,
        });
        draw(ctx, dialog, vec![request]);
        draw(ctx, dialog, vec![]);
    }

    /// Scrolls the node `find` picks into view, as a visitor does, and returns where it can be
    /// clicked. Waits up to thirty seconds for the node to exist; `what` names it.
    fn reveal(
        ctx: &egui::Context,
        dialog: &mut DataSourceDialog,
        what: &str,
        find: impl Fn(&Screen) -> Option<Node>,
    ) -> egui::Pos2 {
        let start = Instant::now();
        loop {
            let screen = draw(ctx, dialog, vec![]);
            let Some(node) = find(&screen) else {
                assert!(
                    start.elapsed() < Duration::from_secs(30),
                    "missing control: {what}"
                );
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "{what} does not scroll into view"
            );
            if let Some(at) = screen.visible(&node) {
                // The dialog takes a frame to move to its place when its size changes, so a
                // control counts as in view where two frames in a row show it.
                let again = draw(ctx, dialog, vec![]);
                if find(&again).and_then(|node| again.visible(&node)) == Some(at) {
                    return at;
                }
                continue;
            }
            // The step scrolls to it, or the list it is in does.
            scroll_to(ctx, dialog, &node);
            let screen = draw(ctx, dialog, vec![]);
            if find(&screen).is_some_and(|node| screen.visible(&node).is_some()) {
                continue;
            }
            // The log or the tree it is in is itself out of view: the heading above it is
            // scrolled into view.
            let above = |heading: &&Node| {
                heading.role == accesskit::Role::Label
                    && HEADINGS.contains(&heading.text.as_str())
                    && heading.rect.bottom() <= node.rect.top() + 1.0
            };
            let nearest = |a: &&Node, b: &&Node| a.rect.bottom().total_cmp(&b.rect.bottom());
            if let Some(heading) = screen.nodes.iter().filter(above).max_by(nearest) {
                scroll_to(ctx, dialog, heading);
            }
        }
    }

    /// Clicks at `at` and returns what the user thereby decided.
    fn press(
        ctx: &egui::Context,
        dialog: &mut DataSourceDialog,
        at: egui::Pos2,
    ) -> Option<Outcome> {
        let moved = draw(ctx, dialog, vec![egui::Event::PointerMoved(at)]);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let clicked = draw(ctx, dialog, vec![button(true), button(false)]);
        moved.outcome.or(clicked.outcome)
    }

    /// Clicks the control whose text is `label` and returns what the user thereby decided.
    fn click(ctx: &egui::Context, dialog: &mut DataSourceDialog, label: &str) -> Option<Outcome> {
        let control = |screen: &Screen| {
            let named = |node: &&Node| {
                node.text == label && node.role != accesskit::Role::Label && !node.field()
            };
            screen.nodes.iter().find(named).cloned()
        };
        let at = reveal(ctx, dialog, label, control);
        press(ctx, dialog, at)
    }

    /// Clicks the node whose text starts with `start` in the tree drawn under `heading`.
    fn node(ctx: &egui::Context, dialog: &mut DataSourceDialog, heading: &str, start: &str) {
        let in_tree = |screen: &Screen| {
            let headings = screen.nodes.iter().filter(|node| {
                node.role == accesskit::Role::Label && HEADINGS.contains(&node.text.as_str())
            });
            let headings: Vec<&Node> = headings.collect();
            // A node belongs to the heading that starts nearest above it and not to its right.
            let owner = |node: &Node| {
                let before = |heading: &&&Node| {
                    heading.rect.top() <= node.rect.top()
                        && heading.rect.left() <= node.rect.left() + 1.0
                };
                let nearest = |a: &&&Node, b: &&&Node| {
                    let (a, b) = (a.rect.left_top(), b.rect.left_top());
                    a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y))
                };
                headings
                    .iter()
                    .filter(before)
                    .max_by(nearest)
                    .map(|heading| heading.text.as_str())
            };
            let wanted = |node: &&Node| {
                node.role == accesskit::Role::Button
                    && node.text.starts_with(start)
                    && owner(node) == Some(heading)
            };
            let topmost = |a: &&Node, b: &&Node| a.rect.top().total_cmp(&b.rect.top());
            screen.nodes.iter().filter(wanted).min_by(topmost).cloned()
        };
        let at = reveal(ctx, dialog, start, in_tree);
        press(ctx, dialog, at);
    }

    /// Clicks `label` in the row that shows `row`: the kind in the row of a route, the rule in
    /// the row of a condition.
    fn choose(ctx: &egui::Context, dialog: &mut DataSourceDialog, row: &str, label: &str) {
        let in_row = |screen: &Screen| {
            let rows = screen
                .nodes
                .iter()
                .filter(|node| node.text == row && node.role != accesskit::Role::Button);
            let rows: Vec<&Node> = rows.collect();
            let wanted = |node: &&Node| {
                let middle = node.rect.center().y;
                node.role == accesskit::Role::Button
                    && node.text == label
                    && rows
                        .iter()
                        .any(|shown| shown.rect.y_range().contains(middle))
            };
            screen.nodes.iter().find(wanted).cloned()
        };
        let at = reveal(ctx, dialog, label, in_row);
        press(ctx, dialog, at);
    }

    /// Draws frames until a text that contains `part` is in the dialog, scrolls it into view
    /// and returns it. Panics after thirty seconds, naming `part`.
    fn wait(ctx: &egui::Context, dialog: &mut DataSourceDialog, part: &str) -> String {
        let holds = |screen: &Screen| {
            screen
                .nodes
                .iter()
                .find(|node| node.text.contains(part))
                .cloned()
        };
        reveal(ctx, dialog, part, holds);
        let screen = draw(ctx, dialog, vec![]);
        holds(&screen)
            .unwrap_or_else(|| panic!("{part} went away"))
            .text
    }

    /// Clicks the empty field that shows the hint `hint` and types `text` into it.
    fn enter(ctx: &egui::Context, dialog: &mut DataSourceDialog, hint: &str, text: &str) {
        let field = |screen: &Screen| {
            let empty = |node: &&Node| node.field() && node.hint == hint && node.text.is_empty();
            screen.nodes.iter().find(empty).cloned()
        };
        let at = reveal(ctx, dialog, hint, field);
        press(ctx, dialog, at);
        draw(ctx, dialog, vec![egui::Event::Text(text.to_owned())]);
    }

    /// Everything a browser would keep for the demo, as one text.
    fn stored(storage: &DemoStorage) -> String {
        let mut kept = AppStorage::default();
        storage.persist(&mut kept);
        eframe::Storage::get_string(&kept, "templates").unwrap_or_default()
    }

    /// A visitor who has typed two addresses and a subscribe message gets from there to a
    /// connected profile by clicks alone: the proposed layout and fields, one field clicked in
    /// the first row, the routes made from the first message without one, the place of the
    /// records, the fields of a trade clicked in a record and the condition values of the log.
    #[test]
    fn a_profile_is_mapped_from_a_test_response_and_the_live_log() {
        const PAGE: &str = r#"{"data":{"candles":[{"t":1794407580,"o":100.75,"h":103.0,"l":100.5,"c":102.5,"q":1500},{"t":1794407640,"o":102.5,"h":102.75,"l":101.0,"c":101.25,"q":800},{"t":1794407700,"o":101.25,"h":101.5,"l":100.0,"c":100.25,"q":700}]}}"#;
        const STATUS: &str = r#"{"type":"status","message":"authenticated"}"#;
        const TRADES: &str = r#"{"type":"trade","data":[{"c":"@","p":101.75,"s":3,"t":1794407710000},{"c":"T","p":99.75,"s":2,"t":1794407715000}]}"#;
        let server = Server::start(|_, _| (200, PAGE.to_owned()));
        let peer = Peer::start(vec![vec![
            Step::Receive(1),
            Step::Send(STATUS),
            Step::Send(TRADES),
            Step::UntilClosed,
        ]]);
        let key = |name: &str| vec![Segment::Key(name.to_owned())];

        // What the visitor typed and saved: a name, a symbol, an interval, two addresses and
        // the subscribe message. No rows, no fields, no routes.
        let typed = Profile {
            name: "Local".to_owned(),
            symbols: vec!["ABC".to_owned()],
            intervals: vec![Interval {
                chart: Timeframe::Min1,
                service: "1m".to_owned(),
            }],
            history: Some(History {
                url: format!(
                    "{}/bars?symbol={{symbol}}&interval={{interval}}&to={{to}}&limit={{limit}}",
                    server.origin
                ),
                page_size: 3,
                ..History::default()
            }),
            stream: Some(Stream {
                url: format!("{}/stream", peer.url),
                connect: vec![r#"{"action":"subscribe","symbol":"{symbol}"}"#.to_owned()],
                ..Stream::default()
            }),
            ..Profile::default()
        };
        let storage = Arc::new(DemoStorage::default());
        let returning = Saved {
            asked: true,
            current: Some("Local".to_owned()),
            profiles: vec![typed.clone()],
            ..Saved::default()
        };
        returning.save(&storage);
        let ctx = egui::Context::default();
        let mut dialog = DataSourceDialog::new(storage.clone(), true);
        dialog.open();

        // History: Test proposes the place of the rows, their layout and the fields it knows.
        click(&ctx, &mut dialog, "History");
        click(&ctx, &mut dialog, "Test");
        wait(
            &ctx,
            &mut dialog,
            "Proposed mapping: confirm it or click the fields to use",
        );
        wait(&ctx, &mut dialog, "3 bars, 0 rows skipped");
        // The volume is mapped by a click on its field in the first row.
        click(&ctx, &mut dialog, "volume");
        node(&ctx, &mut dialog, "First row", "q:");

        // Live: Test connects, and the first message without a route is kept in view.
        click(&ctx, &mut dialog, "Live");
        click(&ctx, &mut dialog, "Test");
        wait(&ctx, &mut dialog, "No route for this message:");
        // The field that tells the kinds of message apart, clicked in the first message.
        click(&ctx, &mut dialog, "route by");
        node(&ctx, &mut dialog, "Message", "type:");
        // The status message gets a route that ignores it. The trades get a route of their
        // own, whose kind is chosen in its row.
        click(&ctx, &mut dialog, r#"Add route for "status""#);
        click(&ctx, &mut dialog, r#"Add route for "trade""#);
        choose(&ctx, &mut dialog, "trade", "Trades");
        // The records of the trades message are the elements of its `data`.
        let line = wait(&ctx, &mut dialog, r#"{"type":"trade""#);
        click(&ctx, &mut dialog, &line);
        click(&ctx, &mut dialog, "records");
        node(&ctx, &mut dialog, "Message", "data");
        // The fields of a trade, clicked in the record. Each click moves on to the next empty
        // slot.
        click(&ctx, &mut dialog, "time");
        for field in ["t:", "p:", "s:", "c:"] {
            node(&ctx, &mut dialog, "Record", field);
        }
        // Both condition values of the log are listed. `T` gets another rule in its row.
        choose(&ctx, &mut dialog, "T", "Volume only");

        let Some(Outcome::Connect { profile, secrets }) = click(&ctx, &mut dialog, "Connect")
        else {
            panic!("Connect did not connect");
        };
        let expected = Profile {
            history: Some(History {
                rows: Rows {
                    path: vec![
                        Segment::Key("data".to_owned()),
                        Segment::Key("candles".to_owned()),
                    ],
                    layout: Layout::Objects,
                },
                bar: BarFields {
                    time: TimeSpec::Instant {
                        field: key("t"),
                        unit: TimeUnit::Seconds,
                    },
                    open: key("o"),
                    high: key("h"),
                    low: key("l"),
                    close: key("c"),
                    volume: key("q"),
                },
                ..typed.history.clone().unwrap()
            }),
            stream: Some(Stream {
                records: key("data"),
                route_by: key("type"),
                routes: vec![
                    Route {
                        value: "status".to_owned(),
                        kind: RouteKind::Ignore,
                    },
                    Route {
                        value: "trade".to_owned(),
                        kind: RouteKind::Trades(TradeFields {
                            time: TimeSpec::Instant {
                                field: key("t"),
                                unit: TimeUnit::Milliseconds,
                            },
                            price: key("p"),
                            size: key("s"),
                            condition: key("c"),
                            conditions: vec![
                                ("@".to_owned(), ConditionRule::PriceAndVolume),
                                ("T".to_owned(), ConditionRule::VolumeOnly),
                            ],
                        }),
                    },
                ],
                ..typed.stream.clone().unwrap()
            }),
            ..typed.clone()
        };
        assert!(secrets.is_empty());
        assert_eq!(*profile, expected);
        // Connecting saved the profile and made own data the source in use.
        let saved = Saved::load(&storage);
        assert!(saved.own);
        assert_eq!(saved.current.as_deref(), Some("Local"));
        assert_eq!(saved.profiles, [expected]);
    }

    /// The life of a key: an exported file does not hold it, an import connects once it is
    /// entered again, and a browser keeps it only when the box is ticked.
    #[test]
    fn an_exported_profile_holds_no_key_and_connects_once_the_key_is_entered() {
        const SECRET: &str = "s3cret-7f3a";
        const J3: &str = r#"{"t":1794407580,"o":100.75,"h":103.0,"l":100.5,"c":102.5,"v":1500}"#;
        const J4: &str = r#"{"t":1794407640,"o":102.5,"h":102.75,"l":101.0,"c":101.25,"v":800}"#;
        const J5: &str = r#"{"t":1794407700,"o":101.25,"h":101.5,"l":100.0,"c":100.25,"v":700}"#;
        let server = Server::start(pages(vec![format!("[{J3},{J4},{J5}]"), "[]".to_owned()]));
        let mut profile = profile(&server.origin);
        profile.keys = vec!["key".to_owned()];
        profile.history.as_mut().unwrap().url.push_str("&key={key}");
        let ctx = egui::Context::default();

        // The first browser: the profile is saved there and its key is kept.
        let first = Arc::new(DemoStorage::default());
        let kept = Saved {
            asked: true,
            current: Some("Local".to_owned()),
            profiles: vec![profile.clone()],
            keys: vec![(
                "Local".to_owned(),
                vec![("key".to_owned(), SECRET.to_owned())],
            )],
            ..Saved::default()
        };
        kept.save(&first);
        assert!(stored(&first).contains(SECRET));
        let mut dialog = DataSourceDialog::new(first, true);
        dialog.open();
        click(&ctx, &mut dialog, "Export");
        let Some(Outcome::Export { file, text }) = click(&ctx, &mut dialog, "Save file") else {
            panic!("no file");
        };
        assert_eq!(file, "Local.json");
        assert!(!text.contains(SECRET));
        assert_eq!(store::import(&text), Ok(profile.clone()));

        // A fresh browser: the visitor takes own data at the first-visit choice and imports
        // the file. Nothing connects before the key is entered.
        let fresh = Arc::new(DemoStorage::default());
        let mut dialog = DataSourceDialog::new(fresh.clone(), true);
        assert!(dialog.startup().is_none() && dialog.is_open());
        click(&ctx, &mut dialog, "Connect my own data");
        // The system's file picker cannot be driven: the text of the file is handed over where
        // the answer of the picker arrives.
        let (picker, picked) = std::sync::mpsc::channel();
        picker.send(Ok(Some(text))).unwrap();
        dialog.import = Some(picked);
        wait(
            &ctx,
            &mut dialog,
            r#"The key "key" has no value. Enter it under Source."#,
        );
        assert!(click(&ctx, &mut dialog, "Connect").is_none());
        enter(&ctx, &mut dialog, "key value", SECRET);
        let Some(Outcome::Connect {
            profile: connected,
            secrets,
        }) = click(&ctx, &mut dialog, "Connect")
        else {
            panic!("Connect did not connect");
        };
        assert_eq!(*connected, profile);
        assert_eq!(secrets, [("key".to_owned(), SECRET.to_owned())]);
        // The chart the application then builds loads the history, with the key in the request.
        let mut chart = ChartBuilder::new()
            .with_feed(Arc::new(MappedFeed::new(*connected, secrets)))
            .with_symbol("ABC")
            .with_timeframe(Timeframe::Min1)
            .with_initial_bars(3)
            .build();
        until(
            &ctx,
            &mut chart,
            "the imported profile's history",
            |chart| {
                let feed = chart.feed_state(SeriesId::MAIN);
                feed.is_some_and(|feed| feed.history_complete)
            },
        );
        assert_eq!(shown(&chart).len(), 3);
        assert!(server.seen()[0].line.ends_with("&key=s3cret-7f3a"));

        // The key was typed and not kept: this browser's storage does not hold it, and its next
        // start opens the dialog for the key in place of connecting.
        assert!(!stored(&fresh).contains(SECRET));
        let mut dialog = DataSourceDialog::new(fresh.clone(), true);
        assert!(dialog.startup().is_none() && dialog.is_open());
        // Kept by choice, the key is stored, and the start after that connects by itself.
        enter(&ctx, &mut dialog, "key value", SECRET);
        click(&ctx, &mut dialog, "Keep keys in this browser");
        let connected = click(&ctx, &mut dialog, "Connect");
        assert!(matches!(connected, Some(Outcome::Connect { .. })));
        assert!(stored(&fresh).contains(SECRET));
        let mut dialog = DataSourceDialog::new(fresh.clone(), true);
        let Some(Outcome::Connect { secrets, .. }) = dialog.startup() else {
            panic!("the start did not connect");
        };
        assert_eq!(secrets, [("key".to_owned(), SECRET.to_owned())]);
        assert!(!dialog.is_open());
        // The box cleared and the profile saved, the key is forgotten.
        dialog.open();
        click(&ctx, &mut dialog, "Keep keys in this browser");
        click(&ctx, &mut dialog, "Save");
        assert!(!stored(&fresh).contains(SECRET));
    }

    /// The status line shows the connection error while the stream is down, and no longer once it
    /// is live again, although the chart keeps the error as its most recent one.
    #[test]
    fn the_status_line_drops_a_connection_error_once_the_stream_is_live() {
        let ctx = egui::Context::default();
        let dialog = DataSourceDialog::new(Arc::new(DemoStorage::default()), true);
        let lost = FeedError::Network("ws://127.0.0.1:9/stream: connection lost".to_owned());
        for (status, shown) in [(FeedStatus::Disconnected, true), (FeedStatus::Live, false)] {
            let mut running = FeedState::default();
            running.status = Some(status);
            running.last_error = Some(lost.clone());
            let output = ctx.run_ui(egui::RawInput::default(), |ui| dialog.status(ui, &running));
            let mut texts = Vec::new();
            for clipped in &output.shapes {
                painted(&clipped.shape, clipped.clip_rect, &mut texts);
            }
            let found = texts
                .iter()
                .any(|(text, ..)| text.contains("connection lost"));
            output.drop_without_applying_deltas();
            assert_eq!(found, shown, "{status:?}");
        }
    }

    /// A Test that fails says, in the step where the visitor pressed the button, which
    /// endpoint failed and why, and the dialog never shows a key.
    #[test]
    fn a_failed_test_names_the_endpoint_and_the_reason() {
        const SECRET: &str = "s3cret-7f3a";
        /// The answer of the service, a change of the mapping, and what the History step shows
        /// after the endpoint.
        type Row = (
            &'static str,
            Option<(u16, &'static str)>,
            fn(&mut Profile),
            &'static str,
        );
        /// A dialog on the base profile of the feed's tests, its key in the address and kept,
        /// changed by `edit`.
        fn open(origin: &str, edit: impl FnOnce(&mut Profile)) -> DataSourceDialog {
            let mut profile = profile(origin);
            profile.keys = vec!["key".to_owned()];
            profile.history.as_mut().unwrap().url.push_str("&key={key}");
            edit(&mut profile);
            let storage = Arc::new(DemoStorage::default());
            let kept = Saved {
                asked: true,
                current: Some("Local".to_owned()),
                profiles: vec![profile],
                keys: vec![(
                    "Local".to_owned(),
                    vec![("key".to_owned(), SECRET.to_owned())],
                )],
                ..Saved::default()
            };
            kept.save(&storage);
            let mut dialog = DataSourceDialog::new(storage, true);
            dialog.open();
            dialog
        }
        // An address where nothing listens.
        let closed = || {
            TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
        };
        let ctx = egui::Context::default();

        let rows: [Row; 4] = [
            (
                "unauthorised",
                Some((401, r#"{"error":"bad key"}"#)),
                |_| {},
                "rejected as unauthorised (HTTP 401): check the key and where it is sent",
            ),
            (
                "not JSON",
                Some((200, "<html>busy</html>")),
                |_| {},
                "unreadable response: not JSON: ",
            ),
            (
                "rows elsewhere",
                Some((200, r#"{"data":{"candles":{"t":[1794407580]}}}"#)),
                |profile| {
                    profile.history.as_mut().unwrap().rows.path = vec![
                        Segment::Key("data".to_owned()),
                        Segment::Key("candles".to_owned()),
                    ];
                },
                "unreadable response: expected a list at data.candles, found an object",
            ),
            ("nothing listens", None, |_| {}, "no response ("),
        ];
        for (name, answer, edit, reason) in rows {
            let server =
                answer.map(|(status, body)| Server::start(move |_, _| (status, body.to_owned())));
            let origin = server.as_ref().map_or_else(
                || format!("http://{}", closed()),
                |server| server.origin.clone(),
            );
            let mut dialog = open(&origin, edit);
            click(&ctx, &mut dialog, "History");
            click(&ctx, &mut dialog, "Test");
            let line = wait(&ctx, &mut dialog, &format!("{origin}/bars: "));
            assert!(
                line.contains(&format!("{origin}/bars: {reason}")),
                "{name}: {line}"
            );
            let texts = draw(&ctx, &mut dialog, vec![]).all();
            assert!(!texts.iter().any(|text| text.contains(SECRET)), "{name}");
            if let Some(server) = server {
                // The key did reach the endpoint, so its absence from the dialog means
                // something.
                assert!(
                    server.seen()[0].line.ends_with("&key=s3cret-7f3a"),
                    "{name}"
                );
            }
            if name == "rows elsewhere" {
                // The response is still drawn as a tree, so the place of the rows can be
                // clicked.
                assert!(
                    texts.iter().any(|text| text.starts_with("candles")),
                    "{name}"
                );
            }
        }

        // A name in braces that is neither a key nor a placeholder is reported, and nothing is
        // sent.
        let server = Server::start(|_, _| (200, "[]".to_owned()));
        let mut dialog = open(&server.origin, |profile| {
            profile
                .history
                .as_mut()
                .unwrap()
                .url
                .push_str("&token={token}");
        });
        click(&ctx, &mut dialog, "History");
        click(&ctx, &mut dialog, "Test");
        wait(
            &ctx,
            &mut dialog,
            r#""{token}" in the history URL is neither a placeholder nor a key of this profile. Add the key under Source or correct the text."#,
        );
        assert!(server.seen().is_empty());

        // The stream: an address where nothing listens is named without its query, and a key
        // in its path by its name.
        let address = closed();
        let mut dialog = open(&format!("http://{address}"), |profile| {
            profile.stream = Some(Stream {
                url: format!("ws://{address}/{{key}}/stream?key={{key}}"),
                ..Stream::default()
            });
        });
        click(&ctx, &mut dialog, "Live");
        click(&ctx, &mut dialog, "Test");
        let line = wait(
            &ctx,
            &mut dialog,
            &format!("ws://{address}/{{key}}/stream: cannot connect ("),
        );
        assert!(!line.contains(SECRET), "{line}");
        click(&ctx, &mut dialog, "Stop");
    }
}
