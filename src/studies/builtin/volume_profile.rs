//! Volume profiles scoped to the viewport, saved times or exchange calendar periods.
//!
//! A profile distributes the volume of each chart bar over the price rows between its low and
//! high, which estimates where the volume traded. With a finer source interval the study asks
//! the host for that history, through [`Indicator::data_requests`], and distributes each finer
//! bar instead; a chart bar without finer bars keeps the estimate, so no volume is lost.
//! With an instrument tick size and Whole ticks per row, Row count picks whole tick rows too.
//! While the source interval follows the chart, the study reads the interval the chart
//! instrument suggests for the length of its profiles,
//! [`Instrument::profile_resolutions`](crate::model::Instrument::profile_resolutions).
//! [`ProfileInput`] tells how much of a profile the finer bars cover.
//! The chart notes each profile "Estimated from bars", also when finer bars cover all of it:
//! they narrow the estimate without making it exact.
//! Where the chart holds the executions of a profile's bars, through
//! [`Indicator::order_flow_inputs`], the profile sums them instead, in rows of whole ticks: the
//! Height input, or Height in ticks, rounded to whole ticks, or, with Whole ticks per row, the
//! whole ticks per row that cover the traded ticks in the number of rows closest to Row count.
//! A profile whose executions start after its first bar sums those held and reports itself
//! partial; one without them, or whose bars do not each hold their own executions, keeps the bars.
//!
//! Row fills stay translucent so price bars remain readable: the painter limits saved opaque colours
//! to the opacity of the default value area rows. The point of control is a 2 px line in the theme's
//! foreground colour until a line colour is chosen.
//! Colours list the three developing plots before POC and the four row colours. Setting five
//! colours edits the profile colours; other lengths start with the developing plots.
//! Developing plots retain source-bar cumulative volume, so a changed row height bins the
//! original bar distributions again without scanning the profile's price history.
//! Each profile's final developing levels use the histogram's per-bar arithmetic, including ties.

use super::AutoAnchor;
use super::anchor_period::{AnchorPeriod, ExchangeDays};
use crate::data::{
    DevelopingVolume, FootprintBar, OrderFlowFootprints, OrderFlowInput, ProfileDisplay,
    ProfileLevel, ProfileLevelKind, ProfileSource, StudyProfile, VolumeProfile,
    VolumeProfileConfig, grid_row,
};
use crate::model::{
    Bar, BarClock, CustomProfileSession, CustomProfileSessions, EventKind, Instrument,
    ProfileResolution, ProfileSession, Session, SessionOverlap, SessionProvider, TimeScaleEvent,
    Timeframe, TimezoneId,
};
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, InputKind, InputValue, PlotStyle, PlotType,
    StudyData, StudyDataRequest, StudyInput,
};
use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeDelta, Utc};
use egui::Color32;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::ops::{Bound, Range};
use std::sync::Arc;

/// What a volume profile was built from.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ProfileInput {
    /// The chart bars the profile covers.
    pub bars: Range<usize>,
    /// The share of those bars, from 0 to 1, whose volume comes from finer bars of the source
    /// interval. The volume of the other bars is an estimate spread over each bar's high to
    /// low range, so 0 marks a profile that is an estimate throughout, unless its
    /// [`source`](crate::data::VolumeProfile::source) is its executions, which take no finer bars.
    pub coverage: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Visible,
    Fixed,
    Session,
    Periodic,
    Anchored,
    Auto,
}

#[derive(Clone, Debug)]
struct ProfileStudy {
    kind: Kind,
    hd: bool,
    rows: usize,
    value_area: f64,
    row_size: f64,
    /// Instrument ticks per row, in place of `row_size`; 0 for `row_size`.
    row_ticks: usize,
    /// Whole ticks per row closest to Row count while no height is set.
    whole_ticks: bool,
    /// Share of a profile's span that its widest row takes; a visible profile fills its strip.
    width: f64,
    right: bool,
    show_values: bool,
    extend: [bool; 3],
    display: ProfileDisplay,
    start_time: f64,
    end_time: f64,
    extend_right: bool,
    period: PeriodUnit,
    period_count: usize,
    /// The open of the first bar of the last profile counted in bars or trading days, so that
    /// loading older bars leaves every period where it was; `None` until counted.
    period_start: Option<DateTime<Utc>>,
    /// Start the periods again at the first bar of each trading year.
    restart: bool,
    anchor: Anchor,
    /// The chart's earnings, dividend and split events, from `set_time_scale_events`.
    events: Vec<(EventKind, DateTime<Utc>)>,
    length: usize,
    interval: Timeframe,
    /// Bar calendar, from `set_bar_clock`, by which each bar's executions are found.
    clock: BarClock,
    source: Option<Timeframe>,
    /// Read the interval the instrument suggests while `source` follows the chart.
    feed_source: bool,
    adaptive: bool,
    /// The Sessions choice: 0 for the exchange trading day, 1 for regular hours, 2 for regular
    /// and extended hours, 3 for `custom`, 4 for the pre-market, 5 for the post-market and 6 for
    /// each segment of the day.
    sessions: usize,
    /// The hours of the Custom choice, kept while another choice is selected.
    custom: [CustomProfileSession; CustomProfileSessions::MAX],
    custom_count: usize,
    overlap: SessionOverlap,
    clip_hours: bool,
    /// Leave the bars outside regular, extended or custom hours out of each session profile.
    hours_only: bool,
    tick_size: Option<f64>,
    /// The instrument's volume decimals, which row values are written in, from `set_instrument`.
    volume_decimals: Option<u8>,
    resolutions: Vec<ProfileResolution>,
    /// The finer interval and data start stated before the chart emptied the profile to wait
    /// for its full recompute, still stated until it has bars again.
    held: Option<(Option<Timeframe>, Option<DateTime<Utc>>)>,
    /// Chart bars in view, from `set_visible_range`.
    view: Range<usize>,
    days: ExchangeDays,
    /// The instrument's exchange zone, UTC without one, and trading hours, from `set_instrument`.
    hours: (TimezoneId, Option<Session>),
    data: Vec<Bar>,
    fine: Vec<Bar>,
    /// Each bar's executions by price, one level per tick. Clones share the chart-fed source.
    flow: OrderFlowInput,
    ranges: Vec<Range<usize>>,
    cache: RefCell<Vec<CachedProfile>>,
    /// Levels of finished profiles, kept while their bars and row layout hold.
    levels: RefCell<LevelCache>,
    values: IndicatorValues,
    developing: BTreeMap<usize, DevelopingRange>,
    developing_start: Option<usize>,
    colors: Vec<Color32>,
    show_rows: bool,
    /// Point of control, value area high and low visibility.
    lines: [bool; 3],
    line_colors: [Color32; 2],
    visible: bool,
}

#[derive(Clone, Debug)]
struct CachedProfile {
    plot: StudyProfile,
    /// Requested layout; `plot.config` holds the effective whole-tick height.
    layout: VolumeProfileConfig,
    opening: Option<VolumeProfile>,
    coverage: f64,
    dirty: bool,
}

#[derive(Clone, Debug)]
struct DevelopingProfile {
    end: usize,
    span: Option<(f64, f64)>,
    input_span: Option<(f64, f64)>,
    profile: Option<VolumeProfile>,
    source: DevelopingVolume,
}

#[derive(Clone, Debug)]
struct DevelopingRange {
    before: DevelopingProfile,
    after: DevelopingProfile,
    /// Exact histogram before the final bar, in the final bar's row layout.
    opening: Option<VolumeProfile>,
    layout: VolumeProfileConfig,
}

/// The kinds of level the extend inputs carry, in their order.
const LEVELS: [ProfileLevelKind; 3] = [
    ProfileLevelKind::PointOfControl,
    ProfileLevelKind::ValueAreaHigh,
    ProfileLevelKind::ValueAreaLow,
];

/// Levels of finished profiles, kept while their bars and row layout hold.
#[derive(Clone, Debug, Default)]
struct LevelCache {
    /// The row layout the levels were found with.
    config: Option<VolumeProfileConfig>,
    /// Each finished profile's end and levels, by its first bar.
    profiles: BTreeMap<usize, (usize, Vec<Level>)>,
}

/// A level of a finished profile while it extends.
#[derive(Clone, Debug)]
struct Level {
    kind: ProfileLevelKind,
    price: f64,
    /// The profile's grid, as the bottom of its first row and the row height.
    grid: (f64, f64),
    /// The level's row of the grid.
    row: f64,
    /// The first bar after the profile that touches the level, once found.
    touch: Option<usize>,
    /// Bars from the profile's end up to this one do not touch the level.
    searched: usize,
}

impl Level {
    /// Whether `bar` trades in the level's row, or for the value area high on the row's top
    /// edge, where its line lies. Prices fall in rows as the profile binned its volume. A bar
    /// without volume, such as the one a heartbeat opens, traded nowhere, and a bar with a NaN
    /// high or low trades in no row.
    fn touched_by(&self, bar: &Bar) -> bool {
        let (low, on_edge) = grid_row(self.grid.0, self.grid.1, bar.low);
        let high = grid_row(self.grid.0, self.grid.1, bar.high).0;
        bar.volume > 0.0
            && (low <= self.row && self.row <= high
                || self.kind == ProfileLevelKind::ValueAreaHigh
                    && on_edge
                    && low == self.row + 1.0
                    && self.row < high)
    }
}

const PERIODS: [AnchorPeriod; 7] = [
    AnchorPeriod::Session,
    AnchorPeriod::Week,
    AnchorPeriod::Month,
    AnchorPeriod::Quarter,
    AnchorPeriod::Year,
    AnchorPeriod::Decade,
    AnchorPeriod::Century,
];

/// What the Period input of a periodic profile counts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PeriodUnit {
    /// Calendar periods of exchange trading days.
    Calendar(AnchorPeriod),
    /// Chart bars.
    Bar,
    /// Minutes, as the chart's bar calendar opens bars of that many minutes.
    Minute,
    /// Hours, as the chart's bar calendar opens bars of that many hours.
    Hour,
}

/// The Period choices in their saved order: the seven calendar units, then the others.
const PERIOD_UNITS: [PeriodUnit; 10] = [
    PeriodUnit::Calendar(AnchorPeriod::Session),
    PeriodUnit::Calendar(AnchorPeriod::Week),
    PeriodUnit::Calendar(AnchorPeriod::Month),
    PeriodUnit::Calendar(AnchorPeriod::Quarter),
    PeriodUnit::Calendar(AnchorPeriod::Year),
    PeriodUnit::Calendar(AnchorPeriod::Decade),
    PeriodUnit::Calendar(AnchorPeriod::Century),
    PeriodUnit::Bar,
    PeriodUnit::Minute,
    PeriodUnit::Hour,
];
/// Where an auto anchored profile starts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Anchor {
    /// Found in the bars, as the auto anchored VWAP finds its anchor.
    Bars(AutoAnchor),
    /// The first bar at or after the latest chart event of this kind that a bar has reached.
    Event(EventKind),
}

/// The Anchor choices with their labels, in their saved order: the anchors found in the bars,
/// then the chart events.
const ANCHORS: [(Anchor, &str); 14] = [
    (Anchor::Bars(AutoAnchor::Auto), "Auto"),
    (Anchor::Bars(AutoAnchor::HighestHigh), "Highest high"),
    (Anchor::Bars(AutoAnchor::LowestLow), "Lowest low"),
    (Anchor::Bars(AutoAnchor::HighestVolume), "Highest volume"),
    (Anchor::Bars(AutoAnchor::Session), "Session"),
    (Anchor::Bars(AutoAnchor::Week), "Week"),
    (Anchor::Bars(AutoAnchor::Month), "Month"),
    (Anchor::Bars(AutoAnchor::Quarter), "Quarter"),
    (Anchor::Bars(AutoAnchor::Year), "Year"),
    (Anchor::Bars(AutoAnchor::Decade), "Decade"),
    (Anchor::Bars(AutoAnchor::Century), "Century"),
    (Anchor::Event(EventKind::Earnings), "Earnings"),
    (Anchor::Event(EventKind::Dividend), "Dividends"),
    (Anchor::Event(EventKind::Split), "Splits"),
];
const DISPLAYS: [ProfileDisplay; 3] = [
    ProfileDisplay::Total,
    ProfileDisplay::UpDown,
    ProfileDisplay::Delta,
];

/// UTC times in Inputs, with a fractional second only when one is present.
const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.f";

/// Saved milliseconds as text, empty for the unset endpoints.
pub(crate) fn time_text(ms: f64) -> String {
    if ms.abs() >= 8e15 {
        return String::new();
    }
    DateTime::from_timestamp_millis(ms as i64)
        .map_or_else(String::new, |time| time.format(TIME_FORMAT).to_string())
}

/// UTC date and time to the minute or finer, or `unset` for empty text.
fn parse_time(text: &str, unset: f64) -> Result<f64, &'static str> {
    if text.is_empty() {
        return Ok(unset);
    }
    chrono::NaiveDateTime::parse_from_str(text, TIME_FORMAT)
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M"))
        .map(|time| time.and_utc().timestamp_millis() as f64)
        .map_err(|_| "time must read YYYY-MM-DD HH:MM:SS")
}

impl ProfileStudy {
    fn custom_sessions(&self) -> CustomProfileSessions {
        CustomProfileSessions {
            sessions: self.custom[..self.custom_count].to_vec(),
            overlap: self.overlap,
            trading_hours_only: self.clip_hours,
        }
    }

    fn new(kind: Kind, rows: usize) -> Self {
        Self {
            kind,
            hd: false,
            rows: rows.max(1),
            value_area: 0.7,
            row_size: 0.0,
            row_ticks: 0,
            whole_ticks: true,
            width: 0.3,
            right: kind == Kind::Visible,
            show_values: false,
            extend: [false; 3],
            display: ProfileDisplay::UpDown,
            start_time: -8e15,
            end_time: 8e15,
            extend_right: false,
            period: PeriodUnit::Calendar(AnchorPeriod::Week),
            period_count: 1,
            period_start: None,
            restart: true,
            anchor: Anchor::Bars(AutoAnchor::Auto),
            events: Vec::new(),
            length: 100,
            interval: Timeframe::Min1,
            clock: BarClock::new(Timeframe::Min1),
            source: None,
            feed_source: true,
            adaptive: false,
            sessions: 0,
            custom: std::array::from_fn(|i| {
                if i == 0 {
                    CustomProfileSession::new(
                        NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
                        NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
                    )
                } else {
                    CustomProfileSession::new(NaiveTime::MIN, NaiveTime::MIN)
                }
            }),
            custom_count: 1,
            overlap: SessionOverlap::OwnProfile,
            clip_hours: false,
            hours_only: false,
            tick_size: None,
            volume_decimals: None,
            resolutions: Vec::new(),
            held: None,
            view: 0..0,
            days: ExchangeDays::default(),
            hours: (TimezoneId::utc(), None),
            data: Vec::new(),
            fine: Vec::new(),
            flow: OrderFlowInput::footprint(),
            ranges: Vec::new(),
            cache: RefCell::new(Vec::new()),
            levels: RefCell::default(),
            values: IndicatorValues::new(),
            developing: BTreeMap::new(),
            developing_start: None,
            colors: vec![
                crate::studies::palette::WARM_ORANGE,
                crate::studies::palette::LIGHT_BLUE,
                crate::studies::palette::LIGHT_BLUE,
                Color32::PLACEHOLDER,
                Color32::from_rgba_unmultiplied(0, 188, 212, 110),
                Color32::from_rgba_unmultiplied(233, 30, 99, 110),
                Color32::from_rgba_unmultiplied(0, 188, 212, 163),
                Color32::from_rgba_unmultiplied(233, 30, 99, 163),
            ],
            show_rows: true,
            lines: [true, false, false],
            line_colors: [Color32::PLACEHOLDER; 2],
            visible: true,
        }
    }

    fn inputs(&self) -> Vec<StudyInput> {
        let mut inputs = vec![
            StudyInput::new(
                "row_count",
                "Row count",
                InputValue::Length(self.rows),
                InputKind::Length(1, 1000),
            ),
            StudyInput::new(
                "value_area",
                "Value area (%)",
                InputValue::Number(self.value_area * 100.0),
                InputKind::Number(0.01, 100.0),
            ),
            StudyInput::new(
                "row_size",
                "Height (0 = equal)",
                InputValue::Number(self.row_size),
                InputKind::Number(0.0, f64::MAX),
            ),
            StudyInput::new(
                "row_ticks",
                "Height in ticks (0 = Height)",
                InputValue::Length(self.row_ticks),
                InputKind::Length(0, 100_000),
            ),
            StudyInput::new(
                "whole_tick_rows",
                "Whole ticks per row",
                InputValue::Toggle(self.whole_ticks),
                InputKind::Toggle,
            ),
            StudyInput::new(
                "display",
                "Volume",
                InputValue::Choice(DISPLAYS.iter().position(|d| *d == self.display).unwrap()),
                InputKind::Choice(vec!["Total".into(), "Up / down".into(), "Delta".into()]),
            ),
            StudyInput::new(
                "source_interval",
                "Source interval",
                InputValue::Timeframe(self.source),
                InputKind::Timeframe,
            ),
            StudyInput::new(
                "feed_source",
                "Feed suggested interval",
                InputValue::Toggle(self.feed_source),
                InputKind::Toggle,
            ),
        ];
        if matches!(self.kind, Kind::Fixed | Kind::Anchored) {
            inputs.push(StudyInput::new(
                "start_time",
                "Start (YYYY-MM-DD HH:MM:SS UTC)",
                InputValue::Text(time_text(self.start_time)),
                InputKind::Text,
            ));
        }
        if self.kind == Kind::Fixed {
            inputs.push(StudyInput::new(
                "end_time",
                "End (YYYY-MM-DD HH:MM:SS UTC)",
                InputValue::Text(time_text(self.end_time)),
                InputKind::Text,
            ));
        }
        if self.kind == Kind::Fixed {
            inputs.push(StudyInput::new(
                "extend_right",
                "Extend right",
                InputValue::Toggle(self.extend_right),
                InputKind::Toggle,
            ));
        }
        if self.kind == Kind::Periodic {
            inputs.push(StudyInput::new(
                "period_count",
                "Period count",
                InputValue::Length(self.period_count),
                InputKind::Length(1, 10_000),
            ));
            let units = PERIOD_UNITS.iter().map(|unit| match unit {
                PeriodUnit::Calendar(period) => period.label().to_owned(),
                PeriodUnit::Bar => "Bar".into(),
                PeriodUnit::Minute => "Minute".into(),
                PeriodUnit::Hour => "Hour".into(),
            });
            inputs.push(StudyInput::new(
                "period",
                "Period",
                InputValue::Choice(PERIOD_UNITS.iter().position(|u| *u == self.period).unwrap()),
                InputKind::Choice(units.collect()),
            ));
            inputs.push(
                StudyInput::new(
                    "restart_yearly",
                    "Restart each year",
                    InputValue::Toggle(self.restart),
                    InputKind::Toggle,
                )
                // Trading days, weeks, months, quarters and bars.
                .shown_when("period", [0, 1, 2, 3, 7]),
            );
        }
        if self.kind == Kind::Auto {
            inputs.push(StudyInput::new(
                "anchor",
                "Anchor",
                InputValue::Choice(
                    ANCHORS
                        .iter()
                        .position(|(anchor, _)| *anchor == self.anchor)
                        .unwrap(),
                ),
                InputKind::Choice(ANCHORS.iter().map(|(_, label)| (*label).into()).collect()),
            ));
            inputs.push(StudyInput::new(
                "length",
                "Lookback bars",
                InputValue::Length(self.length),
                InputKind::Length(1, usize::MAX),
            ));
        }
        if self.kind == Kind::Session {
            inputs.push(StudyInput::new(
                "adaptive_rows",
                "Adapt rows to zoom",
                InputValue::Toggle(self.adaptive),
                InputKind::Toggle,
            ));
            inputs.push(StudyInput::new(
                "sessions",
                "Sessions",
                InputValue::Choice(self.sessions),
                InputKind::Choice(vec![
                    "Trading day".into(),
                    "Regular hours".into(),
                    "Regular and extended hours".into(),
                    "Custom hours".into(),
                    "Pre-market only".into(),
                    "Post-market only".into(),
                    "Each segment of the day".into(),
                ]),
            ));
            for (id, label, time) in [
                ("session_start", "Session start", self.custom[0].start),
                ("session_end", "Session end", self.custom[0].end),
            ] {
                inputs.push(
                    StudyInput::new(id, label, InputValue::TimeOfDay(time), InputKind::TimeOfDay)
                        .shown_when("sessions", [3]),
                );
            }
            let zone = self.custom[0].zone.as_ref().map_or("", TimezoneId::name);
            inputs.push(
                StudyInput::new(
                    "session_zone",
                    "Session time zone",
                    InputValue::TimeZone(zone.into()),
                    InputKind::TimeZone { exchange: true },
                )
                .shown_when("sessions", [3]),
            );
            let mut custom_inputs = vec![
                StudyInput::new(
                    "custom_sessions",
                    "Custom sessions",
                    InputValue::Choice(self.custom_count - 1),
                    InputKind::Choice(
                        (1..=CustomProfileSessions::MAX)
                            .map(|n| n.to_string())
                            .collect(),
                    ),
                )
                .with_default(InputValue::Choice(0))
                .shown_when("sessions", [3]),
            ];
            for i in 1..CustomProfileSessions::MAX {
                let session = &self.custom[i];
                for (suffix, label, value, kind) in [
                    (
                        "start",
                        "start",
                        InputValue::TimeOfDay(session.start),
                        InputKind::TimeOfDay,
                    ),
                    (
                        "end",
                        "end",
                        InputValue::TimeOfDay(session.end),
                        InputKind::TimeOfDay,
                    ),
                    (
                        "zone",
                        "time zone",
                        InputValue::TimeZone(
                            session.zone.as_ref().map_or("", TimezoneId::name).into(),
                        ),
                        InputKind::TimeZone { exchange: true },
                    ),
                ] {
                    let default = if suffix == "zone" {
                        InputValue::TimeZone(String::new())
                    } else {
                        InputValue::TimeOfDay(NaiveTime::MIN)
                    };
                    custom_inputs.push(
                        StudyInput::new(
                            format!("session_{}_{suffix}", i + 1),
                            format!("Session {} {label}", i + 1),
                            value,
                            kind,
                        )
                        .with_default(default)
                        .shown_when(
                            "custom_sessions",
                            (i..CustomProfileSessions::MAX).collect::<Vec<_>>(),
                        ),
                    );
                }
            }
            custom_inputs.extend([
                StudyInput::new(
                    "session_overlap",
                    "Overlapping hours",
                    InputValue::Choice(usize::from(self.overlap == SessionOverlap::InBoth)),
                    InputKind::Choice(vec!["Own profile".into(), "In both sessions".into()]),
                )
                .with_default(InputValue::Choice(0))
                .shown_when("sessions", [3]),
                StudyInput::new(
                    "clip_hours",
                    "Clip to trading hours",
                    InputValue::Toggle(self.clip_hours),
                    InputKind::Toggle,
                )
                .with_default(InputValue::Toggle(false))
                .shown_when("sessions", [3]),
            ]);
            inputs.extend(custom_inputs);
            inputs.push(
                StudyInput::new(
                    "hours_only",
                    "Leave out bars outside the hours",
                    InputValue::Toggle(self.hours_only),
                    InputKind::Toggle,
                )
                .shown_when("sessions", [1, 2, 3]),
            );
        }
        inputs.push(StudyInput::new(
            "width",
            "Width (%)",
            InputValue::Number(self.width * 100.0),
            InputKind::Number(1.0, 100.0),
        ));
        if matches!(self.kind, Kind::Session | Kind::Periodic) {
            for (i, (id, label)) in [
                ("extend_poc", "Extend POC right"),
                ("extend_vah", "Extend VAH right"),
                ("extend_val", "Extend VAL right"),
            ]
            .into_iter()
            .enumerate()
            {
                inputs.push(StudyInput::new(
                    id,
                    label,
                    InputValue::Toggle(self.extend[i]),
                    InputKind::Toggle,
                ));
            }
        }
        for (id, label, value, kind) in [
            (
                "show_rows",
                "Histogram",
                InputValue::Toggle(self.show_rows),
                InputKind::Toggle,
            ),
            (
                "up_color",
                "Up",
                InputValue::Color(self.colors[4]),
                InputKind::Color,
            ),
            (
                "down_color",
                "Down",
                InputValue::Color(self.colors[5]),
                InputKind::Color,
            ),
            (
                "va_up_color",
                "Value area up",
                InputValue::Color(self.colors[6]),
                InputKind::Color,
            ),
            (
                "va_down_color",
                "Value area down",
                InputValue::Color(self.colors[7]),
                InputKind::Color,
            ),
            (
                "show_poc",
                "POC",
                InputValue::Toggle(self.lines[0]),
                InputKind::Toggle,
            ),
            (
                "poc_color",
                "POC colour",
                InputValue::Color(self.colors[3]),
                InputKind::Color,
            ),
            (
                "show_vah",
                "VAH",
                InputValue::Toggle(self.lines[1]),
                InputKind::Toggle,
            ),
            (
                "vah_color",
                "VAH colour",
                InputValue::Color(self.line_colors[0]),
                InputKind::Color,
            ),
            (
                "show_val",
                "VAL",
                InputValue::Toggle(self.lines[2]),
                InputKind::Toggle,
            ),
            (
                "val_color",
                "VAL colour",
                InputValue::Color(self.line_colors[1]),
                InputKind::Color,
            ),
        ] {
            inputs.push(StudyInput::new(id, label, value, kind).in_style_tab());
        }
        inputs.push(
            StudyInput::new(
                "placement",
                "Placement",
                InputValue::Choice(usize::from(self.right)),
                InputKind::Choice(vec!["Left".into(), "Right".into()]),
            )
            .in_style_tab(),
        );
        inputs.push(
            StudyInput::new(
                "show_values",
                "Values",
                InputValue::Toggle(self.show_values),
                InputKind::Toggle,
            )
            .in_style_tab(),
        );
        if self.hd {
            inputs.retain(|input| {
                !matches!(
                    input.id.as_str(),
                    "row_count" | "row_size" | "row_ticks" | "whole_tick_rows" | "adaptive_rows"
                )
            });
        }
        inputs
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), crate::studies::StudyError> {
        let schema = self
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .ok_or_else(|| crate::studies::StudyError::rejected(&self.inputs(), id))?;
        let value = crate::studies::inputs::upgrade_saved_value(&schema.kind, value);
        if !schema.accepts(&value) {
            return Err(crate::studies::StudyError::rejected(&self.inputs(), id));
        }
        let window = (0..CustomProfileSessions::MAX).find_map(|i| {
            let prefix = if i == 0 {
                "session_".to_owned()
            } else {
                format!("session_{}_", i + 1)
            };
            id.strip_prefix(&prefix)
                .filter(|field| matches!(*field, "start" | "end" | "zone"))
                .map(|field| (i, field))
        });
        let (i, field) = window.unwrap_or((0, id));
        match (field, value) {
            ("start", InputValue::TimeOfDay(time)) if window.is_some() => {
                self.custom[i].start = time
            }
            ("end", InputValue::TimeOfDay(time)) if window.is_some() => self.custom[i].end = time,
            ("zone", InputValue::TimeZone(text)) if window.is_some() => {
                self.custom[i].zone = if text.is_empty() {
                    None
                } else {
                    Some(TimezoneId::named(&text).ok_or_else(|| {
                        crate::studies::StudyError::InvalidInput {
                            id: id.into(),
                            reason: "names no known time zone".into(),
                        }
                    })?)
                }
            }

            ("show_rows", InputValue::Toggle(v)) => self.show_rows = v,
            ("extend_right", InputValue::Toggle(v)) => self.extend_right = v,
            ("show_values", InputValue::Toggle(v)) => self.show_values = v,
            ("placement", InputValue::Choice(v)) => self.right = v == 1,
            ("up_color", InputValue::Color(v)) => self.colors[4] = v,
            ("down_color", InputValue::Color(v)) => self.colors[5] = v,
            ("va_up_color", InputValue::Color(v)) => self.colors[6] = v,
            ("va_down_color", InputValue::Color(v)) => self.colors[7] = v,
            ("show_poc", InputValue::Toggle(v)) => self.lines[0] = v,
            ("poc_color", InputValue::Color(v)) => self.colors[3] = v,
            ("show_vah", InputValue::Toggle(v)) => self.lines[1] = v,
            ("vah_color", InputValue::Color(v)) => self.line_colors[0] = v,
            ("show_val", InputValue::Toggle(v)) => self.lines[2] = v,
            ("val_color", InputValue::Color(v)) => self.line_colors[1] = v,
            ("row_count", InputValue::Length(v)) => self.rows = v,
            ("value_area", InputValue::Number(v)) => self.value_area = v / 100.0,
            ("row_size", InputValue::Number(v)) => self.row_size = v,
            ("row_ticks", InputValue::Length(v)) => self.row_ticks = v,
            ("whole_tick_rows", InputValue::Toggle(v)) => self.whole_ticks = v,
            ("display", InputValue::Choice(v)) => self.display = DISPLAYS[v],
            ("source_interval", InputValue::Timeframe(v)) => self.source = v,
            ("feed_source", InputValue::Toggle(v)) => self.feed_source = v,
            ("adaptive_rows", InputValue::Toggle(v)) => self.adaptive = v,
            ("sessions", InputValue::Choice(v)) => self.sessions = v,
            ("hours_only", InputValue::Toggle(v)) => self.hours_only = v,
            ("custom_sessions", InputValue::Choice(n @ 0..=4)) => self.custom_count = n + 1,
            ("session_overlap", InputValue::Choice(n @ 0..=1)) => {
                self.overlap = if n == 0 {
                    SessionOverlap::OwnProfile
                } else {
                    SessionOverlap::InBoth
                }
            }
            ("clip_hours", InputValue::Toggle(clip)) => self.clip_hours = clip,
            ("start_time", InputValue::Text(text)) => {
                self.start_time = parse_time(&text, -8e15).map_err(|reason| {
                    crate::studies::StudyError::InvalidInput {
                        id: id.into(),
                        reason: reason.into(),
                    }
                })?
            }
            ("end_time", InputValue::Text(text)) => {
                self.end_time = parse_time(&text, 8e15).map_err(|reason| {
                    crate::studies::StudyError::InvalidInput {
                        id: id.into(),
                        reason: reason.into(),
                    }
                })?
            }
            ("period", InputValue::Choice(v)) => self.period = PERIOD_UNITS[v],
            ("period_count", InputValue::Length(v)) => self.period_count = v,
            ("restart_yearly", InputValue::Toggle(v)) => self.restart = v,
            ("anchor", InputValue::Choice(v)) => self.anchor = ANCHORS[v].0,
            ("length", InputValue::Length(v)) => self.length = v,
            ("width", InputValue::Number(v)) => self.width = v / 100.0,
            ("extend_poc", InputValue::Toggle(v)) => self.extend[0] = v,
            ("extend_vah", InputValue::Toggle(v)) => self.extend[1] = v,
            ("extend_val", InputValue::Toggle(v)) => self.extend[2] = v,
            _ => return Err(crate::studies::StudyError::invalid_input(id)),
        }
        self.values.clear();
        self.data.clear();
        self.held = None;
        // An edited study counts its trading days and bars afresh.
        self.period_start = None;
        self.fine.clear();
        self.ranges.clear();
        self.cache.get_mut().clear();
        *self.levels.get_mut() = LevelCache::default();
        Ok(())
    }

    /// An explicit interval shorter than the chart's, or the shortest suggested interval
    /// that serves the profile length while the source follows the chart; the held one while
    /// emptied for a recompute.
    fn finer(&self) -> Option<Timeframe> {
        let shorter = |source: &Timeframe| {
            !source.is_trade_based() && source.duration_ms() < self.interval.duration_ms()
        };
        // The chart interval may have changed since.
        if let Some((timeframe, _)) = self.held.filter(|_| self.data.is_empty()) {
            return timeframe.filter(shorter);
        }
        if self.source.is_some() || !self.feed_source || self.resolutions.is_empty() {
            return self.source.filter(shorter);
        }
        ProfileResolution::suggested(&self.resolutions, self.interval, self.span()?)
    }

    /// Profile length from first open to last close: visible bars for a visible profile,
    /// the longest range otherwise.
    fn span(&self) -> Option<TimeDelta> {
        let span = |range: &Range<usize>| {
            let end = range.end.min(self.data.len());
            let first = self.data.get(range.start).filter(|_| range.start < end)?;
            Some(self.close(self.data[end - 1].time) - first.time)
        };
        if self.kind == Kind::Visible {
            span(&self.view)
        } else {
            self.ranges.iter().filter_map(span).max()
        }
    }

    /// The close of the chart bar that opens at `open`, saturating past the calendar's end.
    fn close(&self, open: DateTime<Utc>) -> DateTime<Utc> {
        open.checked_add_signed(self.interval.duration())
            .unwrap_or(DateTime::<Utc>::MAX_UTC)
    }

    fn data_requests(&self) -> Vec<StudyDataRequest> {
        self.finer()
            .map(|timeframe| StudyDataRequest {
                symbol: String::new(),
                timeframe: Some(timeframe),
                metric: None,
            })
            .into_iter()
            .collect()
    }

    /// The chart bar ranges of the profiles in `visible`: the clamped view for a visible
    /// profile, the ranges overlapping it otherwise.
    fn ranges_in(&self, visible: Range<usize>) -> impl Iterator<Item = Range<usize>> + '_ {
        let visible = visible.start.min(self.data.len())..visible.end.min(self.data.len());
        let own = (self.kind == Kind::Visible && !visible.is_empty()).then(|| visible.clone());
        // A visible profile has no ranges of its own.
        let overlapping = self.ranges.iter().filter(move |range| {
            !visible.is_empty() && range.start < visible.end && range.end > visible.start
        });
        own.into_iter().chain(overlapping.cloned())
    }

    /// The open of the first chart bar a profile in view reads, from which its finer bars are
    /// needed; the open of the newest bar with no profile in view, the held start while emptied
    /// for a recompute, and `None` without bars otherwise; the first bar while a level extends.
    fn data_start(&self) -> Option<DateTime<Utc>> {
        if self.data.is_empty() {
            return self.held.and_then(|(_, start)| start);
        }
        // A finished profile anywhere left of the view can carry a level into it.
        if self.extend.contains(&true) {
            return self.data.first().map(|bar| bar.time);
        }
        self.ranges_in(self.view.clone())
            .map(|range| range.start)
            .min()
            .map_or(self.data.last(), |at| self.data.get(at))
            .map(|bar| bar.time)
    }

    /// Takes the finer bars, replacing all of them from bar 0 and from the first changed chart
    /// bar on otherwise, so a reply for another interval is dropped.
    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
        let had_finer = !self.fine.is_empty();
        let wanted = self.finer();
        match sources
            .iter()
            .find(|source| Some(source.timeframe) == wanted)
        {
            Some(source) if from > 0 && !data.is_empty() => {
                let cut = data[from.min(data.len() - 1)].time;
                self.fine
                    .truncate(self.fine.partition_point(|bar| bar.time < cut));
                let skip = source.bars.partition_point(|bar| bar.time < cut);
                self.fine.extend_from_slice(&source.bars[skip..]);
            }
            Some(source) => self.fine = source.bars.to_vec(),
            None => self.fine.clear(),
        }
        self.update(
            data,
            if had_finer && self.fine.is_empty() {
                0
            } else {
                from
            },
        );
    }

    /// What each profile in `visible` was built from.
    fn profile_inputs(&self, visible: Range<usize>) -> Vec<ProfileInput> {
        self.profiles(visible)
            .into_iter()
            .map(|profile| ProfileInput {
                coverage: self
                    .cache
                    .borrow()
                    .iter()
                    .find(|cached| cached.plot.bars == profile.bars)
                    .map_or(0.0, |cached| cached.coverage),
                bars: profile.bars,
            })
            .collect()
    }

    /// The executions the chart holds from the bar before chart bar `start` on, while the source
    /// opens its bars by this study's calendar; executions keyed by another calendar, as the
    /// chart's are while the study runs on another interval, cannot be assigned to these bars.
    fn held_executions(&self, start: usize) -> Option<OrderFlowFootprints> {
        let from = start
            .checked_sub(1)
            .map_or(DateTime::<Utc>::MIN_UTC, |before| {
                self.clock.bar_open(self.data[before].time)
            });
        self.flow.footprint_snapshot_for_clock(from, &self.clock)
    }

    /// The executions of chart bars `range` in `flow`, merged per tick, and whether `flow` holds
    /// all of them. `None` unless it holds some and every bar that can hold them is a bar of
    /// `flow`: stamped at its open, in a slot of its own and apart from the bars beside the
    /// range, with no executions held in a slot that no bar of the range opens. Bars of
    /// price-based chart types and bars the chart lacks so keep the estimate.
    fn executions_of(
        &self,
        range: &Range<usize>,
        flow: &OrderFlowFootprints,
    ) -> Option<(FootprintBar, bool)> {
        let open = |bar: &Bar| self.clock.bar_open(bar.time);
        let (start, end) = (
            open(&self.data[range.start]),
            open(&self.data[range.end - 1]),
        );
        let before = range.start.checked_sub(1).map(|i| open(&self.data[i]));
        // A bar beside the range in one of its slots shares that slot's executions.
        if start > end
            || before.is_some_and(|before| before >= start)
            || self
                .data
                .get(range.end)
                .is_some_and(|after| open(after) <= end)
        {
            return None;
        }
        // Rows before the first bar's slot belong to bars the chart has dropped or never held.
        let lower = before.map_or(Bound::Included(start), Bound::Excluded);
        let mut held = flow.bars((lower, Bound::Included(end))).peekable();
        let first = held.peek()?.time;
        // Bars stamped before the first held bar hold none of the executions.
        let from = range.start + self.data[range.clone()].partition_point(|bar| bar.time < first);
        let mut opens = Vec::with_capacity(range.end - from);
        for bar in &self.data[from..range.end] {
            let open = open(bar);
            if open != bar.time || opens.last().is_some_and(|&last| last >= open) {
                return None;
            }
            opens.push(open);
        }
        let mut opens = opens.into_iter().peekable();
        let mut merged = FootprintBar::empty(start);
        for bar in held {
            let time = bar.time;
            while opens.next_if(|&open| open < time).is_some() {}
            // Executions in a slot that no bar of the range opens belong to bars it lacks.
            if opens.peek() != Some(&time) {
                return None;
            }
            merged.absorb_grouped(bar, 1);
        }
        let complete = flow.span_is_complete(start, end);
        Some((merged, complete))
    }

    /// The profile of chart bars `range` from the executions in `flow`, in rows of whole ticks
    /// (`VolumeProfile::of_executions`). `None` without them ([`Self::executions_of`]), with a
    /// volume that is not finite, or past 10,000 rows.
    fn executed_profile(
        &self,
        range: &Range<usize>,
        config: &VolumeProfileConfig,
        flow: &OrderFlowFootprints,
    ) -> Option<(VolumeProfile, VolumeProfileConfig)> {
        let tick = flow.tick_size?;
        let (bar, complete) = self.executions_of(range, flow)?;
        let mut config = *config;
        if self.whole_ticks && config.row_size().is_none() {
            let (&first, &last) = (bar.levels.keys().next()?, bar.levels.keys().next_back()?);
            if let Some(ticks) =
                crate::data::whole_tick_rows(first as f64, last as f64 + 1.0, config.rows())
            {
                config = config
                    .with_row_size(Some(ticks as f64 * tick))
                    .unwrap_or(config);
            }
        }
        VolumeProfile::of_executions(&bar, &config, tick, tick, complete)
            .map(|profile| (profile, config))
    }

    /// The choice of the Sessions input and the calendar it divides intraday bars by: `None` for
    /// the trading day, on daily and longer bars, which carry a trading date rather than hours,
    /// for regular or extended hours without instrument hours, and for the pre-market,
    /// post-market and segment choices without extended hours.
    fn session_clock(&self) -> Option<(ProfileSession, BarClock)> {
        if self.interval.duration_ms() >= Timeframe::Day1.duration_ms() {
            return None;
        }
        let choice = match self.sessions {
            1 => ProfileSession::Regular,
            2 => ProfileSession::Extended,
            4 => ProfileSession::PreMarket,
            5 => ProfileSession::PostMarket,
            6 => ProfileSession::Segments,
            _ => return None,
        };
        let (zone, hours) = &self.hours;
        let clock = choice.session_clock(&BarClock::new(self.interval), zone, hours.as_ref())?;
        Some((choice, clock))
    }

    fn period_ranges(&self, period: AnchorPeriod) -> Vec<Range<usize>> {
        let days = self.days.of(&self.data);
        runs(days.into_iter().map(|day| period.key(day)))
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.data = data.to_vec();
        self.values = IndicatorValues::missing(data.len());
        self.developing.clear();
        self.developing_start = None;
        self.cache.get_mut().clear();
        *self.levels.get_mut() = LevelCache::default();
        self.refresh_ranges();
        self.refresh_developing(0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if data.is_empty() && !self.data.is_empty() {
            // Keeps requesting what it read, so the chart keeps those bars for the recompute.
            self.held = Some((self.finer(), self.data_start()));
        }
        let from = from.min(data.len()).min(self.data.len());
        let times_changed = self.data.len() != data.len()
            || self.data[from..]
                .iter()
                .zip(&data[from..])
                .any(|(a, b)| a.time != b.time);
        self.data.truncate(from);
        self.data.extend_from_slice(&data[from..]);
        self.cache.get_mut().retain_mut(|cached| {
            if cached.plot.bars.end.saturating_sub(1) > from {
                return false;
            }
            cached.dirty |= cached.plot.bars.end > from;
            true
        });
        // Profiles with a bar from `from` on are found again; the search for a level's first
        // trade resumes where a bar may have changed.
        let levels = self.levels.get_mut();
        levels.profiles.retain(|_, (end, _)| *end <= from);
        for level in levels.profiles.values_mut().flat_map(|(_, levels)| levels) {
            if level.touch.is_some_and(|touch| touch >= from) {
                level.touch = None;
            }
            level.searched = level.searched.min(from);
        }
        // A change of instrument, bar calendar or events reaches the study as an update from bar 0
        // over the same bars.
        if times_changed
            || from == 0
            || (self.kind == Kind::Auto
                && matches!(
                    self.anchor,
                    Anchor::Bars(
                        AutoAnchor::HighestHigh | AutoAnchor::LowestLow | AutoAnchor::HighestVolume
                    )
                ))
        {
            self.refresh_ranges();
        }
        // Restore the previous endpoint to cumulative arithmetic when its range grows.
        // Finer inputs also stop at the changed bar's open.
        let developing_from = if times_changed {
            from.saturating_sub(1)
        } else {
            from
        };
        self.refresh_developing(developing_from);
    }

    fn estimated_profile(
        &self,
        range: Range<usize>,
        config: VolumeProfileConfig,
        span: (f64, f64),
    ) -> Option<VolumeProfile> {
        let mut profile = VolumeProfile::empty(span.0, span.1, config.rows(), config.row_size())?;
        let finer = self.finer().is_some() && !self.fine.is_empty();
        for i in range {
            let bar = &self.data[i];
            let end = self
                .data
                .get(i + 1)
                .map_or_else(|| self.close(bar.time), |next| next.time);
            let inside = if finer {
                &self.fine[self.fine.partition_point(|fine| fine.time < bar.time)
                    ..self.fine.partition_point(|fine| fine.time < end)]
            } else {
                &[]
            };
            for input in if inside.is_empty() {
                &self.data[i..=i]
            } else {
                inside
            } {
                profile.add(input);
            }
        }
        Some(profile)
    }

    /// Resume each affected range before its changing bar; moving starts discard their prefix.
    fn refresh_developing(&mut self, from: usize) {
        let moving = matches!(self.kind, Kind::Visible | Kind::Auto);
        let start = if self.kind == Kind::Visible {
            Some(self.view.start.min(self.data.len()))
        } else {
            self.ranges.first().map(|range| range.start)
        };
        let keep = if moving && self.developing_start != start {
            0
        } else {
            from
        };
        self.developing_start = start;
        self.developing.split_off(&keep);
        let ranges: Vec<_> = if self.kind == Kind::Visible {
            let range = self.view.start.min(self.data.len())..self.view.end.min(self.data.len());
            if range.is_empty() {
                Vec::new()
            } else {
                vec![range]
            }
        } else {
            let first = self.ranges.partition_point(|range| range.end <= keep);
            self.ranges[first..].to_vec()
        };
        self.values.truncate(keep);
        self.values.resize(self.data.len(), IndicatorValue::None);
        // Empty ranges cannot set a zoom-adapted height.
        let Some(base) = self.config(&[]) else {
            return;
        };
        let finer = self.finer().is_some() && !self.fine.is_empty();
        for range in ranges.into_iter().filter(|range| range.end > keep) {
            let cached = self.developing.remove(&range.start);
            let mut state = cached
                .as_ref()
                .and_then(|cached| {
                    if cached.after.end <= keep {
                        Some(cached.after.clone())
                    } else if cached.before.end <= keep {
                        Some(cached.before.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or(DevelopingProfile {
                    end: range.start,
                    span: None,
                    input_span: None,
                    profile: None,
                    source: DevelopingVolume::default(),
                });
            let mut before = state.clone();
            let mut opening = None;
            let mut layout = base;
            for i in state.end..range.end {
                if i + 1 == range.end {
                    before = state.clone();
                }
                let bar_span = crate::data::price_span(&self.data[i..=i]);
                if let Some((low, high)) = bar_span {
                    state.span = Some(
                        state
                            .span
                            .map_or((low, high), |(lo, hi)| (lo.min(low), hi.max(high))),
                    );
                }
                let config = self.range_config(state.span, base, false);
                let end = self
                    .data
                    .get(i + 1)
                    .map_or_else(|| self.close(self.data[i].time), |bar| bar.time);
                let inside = if finer {
                    &self.fine[self
                        .fine
                        .partition_point(|bar| bar.time < self.data[i].time)
                        ..self.fine.partition_point(|bar| bar.time < end)]
                } else {
                    &[]
                };
                let added = if inside.is_empty() {
                    &self.data[i..=i]
                } else {
                    inside
                };
                let previous_span = state.input_span;
                state.source.add(added);
                if let Some((low, high)) = crate::data::price_span(added) {
                    state.input_span = Some(
                        state
                            .input_span
                            .map_or((low, high), |(lo, hi)| (lo.min(low), hi.max(high))),
                    );
                }
                if let Some(span) = state.input_span {
                    if let Some(profile) = &mut state.profile
                        && profile
                            .extend_developing(
                                span,
                                config.rows(),
                                config.row_size(),
                                added,
                                &state.source,
                            )
                            .is_none()
                    {
                        state.profile = None;
                    }
                    if state.profile.is_none() && previous_span.is_none() {
                        state.profile = VolumeProfile::from_bars(added, config);
                    } else if state.profile.is_none()
                        && let Some(mut profile) =
                            VolumeProfile::empty(span.0, span.1, config.rows(), config.row_size())
                    {
                        // Grid recovery uses the index; only unrepresentable arithmetic needs source bars.
                        state.profile = if state.source.fill(&mut profile).is_some() {
                            Some(profile)
                        } else {
                            self.estimated_profile(range.start..i + 1, config, span)
                        };
                    }
                }
                let histogram = if i + 1 == range.end {
                    layout = config;
                    opening = cached
                        .as_ref()
                        .filter(|cached| {
                            cached.before.end <= keep
                                && cached.before.end <= i
                                && cached.layout == config
                                && (!finer || cached.before.end == i)
                        })
                        .and_then(|cached| {
                            self.data[cached.before.end..i]
                                .iter()
                                .try_fold(cached.opening.clone()?, |profile, bar| {
                                    profile.with_bar(bar)
                                })
                        })
                        .or_else(|| {
                            let bars = &self.data[range.start..i];
                            if finer {
                                VolumeProfile::of_finer(
                                    bars,
                                    self.data[i].time,
                                    &self.fine,
                                    config.rows(),
                                    config.row_size(),
                                )
                                .0
                            } else {
                                VolumeProfile::from_bars(bars, config)
                            }
                        });
                    // Keep the histogram's rounding and tie choices without rebuilding it on every tick.
                    opening
                        .clone()
                        .and_then(|profile| {
                            added
                                .iter()
                                .try_fold(profile, |profile, bar| profile.with_bar(bar))
                        })
                        .or_else(|| {
                            // A moving grid needs the prefix in the final layout for subsequent ticks.
                            opening =
                                self.estimated_profile(range.start..i, config, state.input_span?);
                            opening.clone().map(|mut profile| {
                                added.iter().for_each(|bar| profile.add(bar));
                                profile
                            })
                        })
                } else {
                    None
                };
                let profile = if i + 1 == range.end {
                    histogram.as_ref()
                } else {
                    state.profile.as_ref()
                };
                if i >= keep
                    && bar_span.is_some()
                    && let Some(profile) = profile
                    && let Some((poc, (low, high))) = profile.poc_and_value_area(self.value_area)
                {
                    for (line, row) in [poc as f64 + 0.5, high as f64 + 1.0, low as f64]
                        .into_iter()
                        .enumerate()
                    {
                        self.values
                            .set_value(line, i, profile.low() + row * profile.row_height());
                    }
                }
                state.end = i + 1;
            }
            self.developing.insert(
                range.start,
                DevelopingRange {
                    before,
                    after: state,
                    opening,
                    layout,
                },
            );
        }
    }

    fn refresh_ranges(&mut self) {
        let data = &self.data;
        self.ranges = match self.kind {
            Kind::Visible => Vec::new(),
            Kind::Fixed | Kind::Anchored => {
                let start = data
                    .partition_point(|bar| (bar.time.timestamp_millis() as f64) < self.start_time);
                let end = if self.kind == Kind::Fixed && !self.extend_right {
                    data.partition_point(|bar| {
                        (bar.time.timestamp_millis() as f64) <= self.end_time
                    })
                } else {
                    data.len()
                };
                if start < end {
                    std::iter::once(start..end).collect()
                } else {
                    Vec::new()
                }
            }
            Kind::Session
                if self.sessions == 3
                    && self.interval.duration_ms() < Timeframe::Day1.duration_ms() =>
            {
                let regular = self.hours.1.as_ref().map(|hours| {
                    BarClock::new(self.interval).with_session(
                        self.hours.0.clone(),
                        hours.clone(),
                        false,
                    )
                });
                self.custom_sessions()
                    .with_zone(&self.hours.0)
                    .parts(
                        data,
                        0..data.len(),
                        self.interval,
                        regular.as_ref(),
                        self.hours_only,
                    )
                    .into_iter()
                    .map(|p| p.bars)
                    .collect()
            }
            Kind::Session => match self.session_clock() {
                // A session starts at each bar whose trading date differs from the bar before;
                // the choice keeps the parts of it that it profiles.
                Some((choice, clock)) => {
                    let sessions = clock.sessions();
                    let breaks = sessions.session_breaks(data);
                    let starts: Vec<_> = std::iter::once(0)
                        .chain(breaks.into_iter().map(|(i, _)| i))
                        .chain([data.len()])
                        .collect();
                    starts
                        .windows(2)
                        .filter(|pair| pair[0] < pair[1])
                        .flat_map(|pair| {
                            choice.profiled(data, pair[0]..pair[1], &sessions, self.hours_only)
                        })
                        .collect()
                }
                None => self.period_ranges(AnchorPeriod::Session),
            },
            Kind::Periodic => match self.period {
                // Trading days and bars lie on no grid fixed in time, so they are counted from
                // the first bar of the newest profile, which loading older bars leaves in place.
                PeriodUnit::Calendar(AnchorPeriod::Session) | PeriodUnit::Bar => {
                    let units = if self.period == PeriodUnit::Bar {
                        (0..data.len()).map(|i| i..i + 1).collect()
                    } else {
                        self.period_ranges(AnchorPeriod::Session)
                    };
                    if self.restart {
                        let years: Vec<_> = self.days.of(data).iter().map(Datelike::year).collect();
                        restarted(
                            units,
                            &years,
                            self.period_count,
                            data,
                            &mut self.period_start,
                        )
                    } else {
                        counted(units, self.period_count, data, &mut self.period_start)
                    }
                }
                PeriodUnit::Calendar(period) => {
                    let count = self.period_count as i64;
                    // Years, decades and centuries start at a year already.
                    let restart = self.restart
                        && matches!(
                            period,
                            AnchorPeriod::Week | AnchorPeriod::Month | AnchorPeriod::Quarter
                        );
                    let group = |day: NaiveDate| {
                        let index = period.index(day);
                        if restart {
                            let first = period.index(day.with_ordinal(1).unwrap());
                            (day.year(), (index - first).div_euclid(count))
                        } else {
                            (0, index.div_euclid(count))
                        }
                    };
                    runs(self.days.of(data).into_iter().map(group))
                }
                unit => {
                    let seconds = if unit == PeriodUnit::Minute { 60 } else { 3600 };
                    let length = NonZeroU64::new(seconds * self.period_count as u64).unwrap();
                    let clock = self.clock.for_timeframe(Timeframe::Custom(length));
                    runs(data.iter().map(|bar| clock.bar_open(bar.time)))
                }
            },
            Kind::Auto => match self.anchor {
                // A later event, such as an announced earnings date, waits for its bar.
                Anchor::Event(kind) => match data.last() {
                    Some(last) => self
                        .events
                        .iter()
                        .filter(|(event, time)| *event == kind && *time <= last.time)
                        .map(|(_, time)| *time)
                        .max()
                        .map(|time| data.partition_point(|bar| bar.time < time)..data.len())
                        .into_iter()
                        .collect(),
                    None => Vec::new(),
                },
                Anchor::Bars(anchor) => {
                    let period = match anchor {
                        AutoAnchor::Auto => {
                            let day = Timeframe::Day1.duration_ms();
                            Some(match self.interval.duration_ms() {
                                ms if ms < day => AnchorPeriod::Session,
                                ms if ms <= day => AnchorPeriod::Month,
                                ms if ms <= 10 * day => AnchorPeriod::Quarter,
                                ms if ms <= 60 * day => AnchorPeriod::Year,
                                _ => AnchorPeriod::Decade,
                            })
                        }
                        AutoAnchor::HighestHigh
                        | AutoAnchor::LowestLow
                        | AutoAnchor::HighestVolume => None,
                        _ => ANCHORS
                            .iter()
                            .position(|(choice, _)| *choice == self.anchor)
                            .map(|i| PERIODS[i - 4]),
                    };
                    if let Some(period) = period {
                        self.period_ranges(period).pop().into_iter().collect()
                    } else {
                        let metric = |bar: &Bar| match anchor {
                            AutoAnchor::HighestHigh => bar.high,
                            AutoAnchor::LowestLow => -bar.low,
                            _ => bar.volume,
                        };
                        (data.len().saturating_sub(self.length)..data.len())
                            .filter(|&i| metric(&data[i]).is_finite())
                            .max_by(|&a, &b| metric(&data[a]).total_cmp(&metric(&data[b])))
                            .map(|start| start..data.len())
                            .into_iter()
                            .collect()
                    }
                }
            },
        };
        // A profile whose range changed holds no levels.
        let ranges = &self.ranges;
        self.levels.get_mut().profiles.retain(|start, (end, _)| {
            ranges
                .binary_search_by_key(start, |range| range.start)
                .is_ok_and(|i| ranges[i].end == *end)
        });
    }

    /// Keeps the chart's earnings, dividend and split events. Returns `true` when they changed
    /// under an event anchor, whose profile must then be found again from bar 0.
    fn set_events(&mut self, events: &[TimeScaleEvent]) -> bool {
        let corporate = || {
            events
                .iter()
                .filter(|event| {
                    matches!(
                        event.kind,
                        EventKind::Earnings | EventKind::Dividend | EventKind::Split
                    )
                })
                .map(|event| (event.kind, event.time))
        };
        // Called every frame: no allocation while the events are unchanged.
        if self.events.iter().copied().eq(corporate()) {
            return false;
        }
        self.events = corporate().collect();
        matches!(self.anchor, Anchor::Event(_))
    }

    /// One row height for the chart-bar envelope of every session in view, rounded up
    /// to 1, 2 or 5 times a power of ten ticks, or of price without a tick size.
    fn adaptive_height(&self, ranges: &[Range<usize>]) -> Option<f64> {
        let (low, high) = ranges
            .iter()
            .filter_map(|range| self.data.get(range.clone()))
            .flatten()
            .filter(|bar| bar.low.is_finite() && bar.high.is_finite() && bar.high >= bar.low)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), bar| {
                (low.min(bar.low), high.max(bar.high))
            });
        let raw = (high - low) / self.rows as f64;
        if !(raw.is_finite() && raw > 0.0) {
            return None;
        }
        match self.tick_size {
            Some(tick) => nice_ceil((raw / tick).max(1.0)).map(|ticks| tick * ticks),
            None => nice_ceil(raw),
        }
    }

    fn profiles(&self, visible: Range<usize>) -> Vec<StudyProfile> {
        let ranges: Vec<_> = self.ranges_in(visible).collect();
        if ranges.is_empty() {
            return Vec::new();
        }
        self.config(&ranges)
            .map_or_else(Vec::new, |config| self.profiles_of(ranges, config))
    }

    /// The row layout, value area and colours of profiles built while `ranges` are in view;
    /// `None` for settings no profile takes.
    fn config(&self, ranges: &[Range<usize>]) -> Option<VolumeProfileConfig> {
        // Rows of whole instrument ticks, where the instrument has a tick size, replace Height.
        let ticks = self.tick_size.filter(|_| self.row_ticks > 0);
        let height = ticks
            .map(|tick| self.row_ticks as f64 * tick)
            .or((self.row_size > 0.0).then_some(self.row_size));
        let mut config = VolumeProfileConfig::new(self.rows, self.value_area)
            .and_then(|c| c.with_row_size(height))?;
        if self.adaptive
            && let Some(height) = self.adaptive_height(ranges)
        {
            config = config.with_row_size(Some(height)).unwrap_or(config);
        }
        config.show_rows = self.show_rows;
        config.show_values = self.show_values;
        config.show_poc = self.lines[0];
        config.show_value_area_high = self.lines[1];
        config.show_value_area_low = self.lines[2];
        config.value_area_line_colors = self.line_colors.map(|color| color.to_srgba_unmultiplied());
        config.display = self.display;
        config.up_color = self.colors[4].to_srgba_unmultiplied();
        config.down_color = self.colors[5].to_srgba_unmultiplied();
        config.value_area_colors = Some([
            self.colors[6].to_srgba_unmultiplied(),
            self.colors[7].to_srgba_unmultiplied(),
        ]);
        Some(config)
    }

    /// Apply whole instrument ticks to Row count when neither height nor adaptation sets it.
    fn range_config(
        &self,
        span: Option<(f64, f64)>,
        config: VolumeProfileConfig,
        adaptive: bool,
    ) -> VolumeProfileConfig {
        if self.whole_ticks
            && !adaptive
            && config.row_size().is_none()
            && let Some(tick) = self.tick_size
            && let Some((low, high)) = span
            && let Some(ticks) =
                crate::data::whole_tick_rows(low / tick, high / tick, config.rows())
        {
            return config
                .with_row_size(Some(ticks as f64 * tick))
                .unwrap_or(config);
        }
        config
    }

    /// The profiles of `ranges` under `config`, from the cache where it holds them.
    fn profiles_of(
        &self,
        ranges: Vec<Range<usize>>,
        config: VolumeProfileConfig,
    ) -> Vec<StudyProfile> {
        // A visible profile fills its strip at the right of the view.
        let width = if self.kind == Kind::Visible {
            1.0
        } else {
            self.width as f32
        };
        let mut cache = self.cache.borrow_mut();
        let mut profiles = Vec::new();
        // Copied once, from the bar before the first profile built here on.
        let mut held: Option<Option<OrderFlowFootprints>> = None;
        for range in ranges {
            let effective = self.range_config(
                crate::data::price_span(&self.data[range.clone()]),
                config,
                self.adaptive,
            );
            if let Some(plot) = cache.iter().find(|cached| {
                cached.plot.bars == range && cached.layout == config && !cached.dirty
            }) {
                // The decimals only format the values, so a cached profile takes the current ones.
                profiles.push(StudyProfile {
                    volume_decimals: self.volume_decimals,
                    ..plot.plot.clone()
                });
            } else if let Some((profile, effective)) = held
                .get_or_insert_with(|| self.held_executions(range.start))
                .as_ref()
                .and_then(|flow| self.executed_profile(&range, &config, flow))
            {
                let plot = StudyProfile {
                    bars: range.clone(),
                    profile: Arc::new(profile),
                    config: effective,
                    color: self.colors[3].to_srgba_unmultiplied(),
                    right_aligned: self.right,
                    strip: (self.kind == Kind::Visible).then_some(self.width as f32),
                    width,
                    volume_decimals: self.volume_decimals,
                };
                profiles.push(plot.clone());
                cache.retain(|cached| cached.plot.bars != range);
                cache.push(CachedProfile {
                    plot,
                    layout: config,
                    opening: None,
                    coverage: 0.0,
                    dirty: false,
                });
            } else if self.finer().is_some() && !self.fine.is_empty() {
                let end = self.data.get(range.end).map_or_else(
                    || self.close(self.data[range.end - 1].time),
                    |next| next.time,
                );
                let (profile, covered) = VolumeProfile::of_finer(
                    &self.data[range.clone()],
                    end,
                    &self.fine,
                    effective.rows(),
                    effective.row_size(),
                );
                let coverage = covered as f64 / range.len() as f64;
                let Some(profile) = profile else {
                    continue;
                };
                let plot = StudyProfile {
                    bars: range.clone(),
                    profile: Arc::new(profile),
                    config: effective,
                    color: self.colors[3].to_srgba_unmultiplied(),
                    right_aligned: self.right,
                    strip: (self.kind == Kind::Visible).then_some(self.width as f32),
                    width,
                    volume_decimals: self.volume_decimals,
                };
                profiles.push(plot.clone());
                cache.retain(|cached| cached.plot.bars != range);
                cache.push(CachedProfile {
                    plot,
                    layout: config,
                    opening: None,
                    coverage,
                    dirty: false,
                });
            } else {
                let opening = cache
                    .iter()
                    .filter(|cached| {
                        cached.plot.bars.start == range.start
                            && cached.plot.bars.end <= range.end
                            && cached.layout == config
                            && cached.plot.config == effective
                    })
                    .max_by_key(|cached| cached.plot.bars.end)
                    .and_then(|cached| {
                        let mut opening = cached.opening.clone()?;
                        for bar in &self.data[cached.plot.bars.end - 1..range.end - 1] {
                            opening = opening.with_bar(bar)?;
                        }
                        Some(opening)
                    })
                    .or_else(|| {
                        VolumeProfile::from_bars(&self.data[range.start..range.end - 1], effective)
                    });
                let profile = opening
                    .as_ref()
                    .and_then(|p| p.with_bar(&self.data[range.end - 1]))
                    .or_else(|| VolumeProfile::from_bars(&self.data[range.clone()], effective));
                let Some(profile) = profile else {
                    continue;
                };
                let plot = StudyProfile {
                    bars: range.clone(),
                    profile: Arc::new(profile),
                    config: effective,
                    color: self.colors[3].to_srgba_unmultiplied(),
                    right_aligned: self.right,
                    strip: (self.kind == Kind::Visible).then_some(self.width as f32),
                    width,
                    volume_decimals: self.volume_decimals,
                };
                profiles.push(plot.clone());
                cache.retain(|cached| cached.plot.bars != range);
                cache.push(CachedProfile {
                    plot,
                    layout: config,
                    opening,
                    coverage: 0.0,
                    dirty: false,
                });
            }
        }
        // Keep neighbouring viewport queries from fitting, painting and hit testing cached.
        let keep = profiles.len().saturating_add(4);
        let excess = cache.len().saturating_sub(keep);
        cache.drain(..excess);
        profiles
    }

    /// The levels of finished profiles that reach into `visible`, each from the bar after its
    /// profile to the first later bar that touches it (see [`Level::touched_by`]), or to the last
    /// bar.
    fn levels(&self, visible: Range<usize>) -> Vec<ProfileLevel> {
        let len = self.data.len();
        if !self.extend.contains(&true) {
            return Vec::new();
        }
        let in_view: Vec<_> = self.ranges_in(visible.clone()).collect();
        // Levels lie on the rows that the profiles in view are drawn with.
        let Some(config) = (!in_view.is_empty())
            .then(|| self.config(&in_view))
            .flatten()
        else {
            return Vec::new();
        };
        let mut cache = self.levels.borrow_mut();
        if cache.config != Some(config) {
            *cache = LevelCache {
                config: Some(config),
                profiles: BTreeMap::new(),
            };
        }
        // A finished profile anywhere left of the view's end can carry a level into it.
        // shortcut: a change of the adaptive row height rebuilds every finished profile once;
        // keep levels per height if zooming over many sessions shows the cost.
        let missing: Vec<_> = self
            .ranges
            .iter()
            .filter(|range| range.end < len && range.start < visible.end)
            .filter(|range| {
                cache
                    .profiles
                    .get(&range.start)
                    .is_none_or(|(end, _)| *end != range.end)
            })
            .cloned()
            .collect();
        if !missing.is_empty() {
            let plots = self.profiles_of(missing.clone(), config);
            for range in missing {
                let levels = plots
                    .iter()
                    .find(|plot| plot.bars == range)
                    .map_or_else(Vec::new, |plot| self.levels_of(plot));
                cache.profiles.insert(range.start, (range.end, levels));
            }
        }
        let color = self.colors[3].to_srgba_unmultiplied();
        let mut found = Vec::new();
        for (end, levels) in cache
            .profiles
            .range_mut(..visible.end)
            .map(|(_, entry)| entry)
        {
            let end = *end;
            if end >= len || end >= visible.end {
                continue;
            }
            for level in levels {
                if level.touch.is_none() && level.searched < len {
                    let from = level.searched.max(end);
                    level.touch = self.data[from..]
                        .iter()
                        .position(|bar| level.touched_by(bar))
                        .map(|i| from + i);
                    level.searched = len;
                }
                let last = level.touch.unwrap_or(len - 1);
                if last >= visible.start {
                    found.push(ProfileLevel {
                        bars: end..last + 1,
                        price: level.price,
                        kind: level.kind,
                        color: match level.kind {
                            ProfileLevelKind::ValueAreaHigh
                                if self.line_colors[0] != Color32::PLACEHOLDER =>
                            {
                                self.line_colors[0].to_srgba_unmultiplied()
                            }
                            ProfileLevelKind::ValueAreaLow
                                if self.line_colors[1] != Color32::PLACEHOLDER =>
                            {
                                self.line_colors[1].to_srgba_unmultiplied()
                            }
                            _ => color,
                        },
                    });
                }
            }
        }
        found
    }

    /// The extended levels of the finished profile `plot`. There are none for a profile without
    /// volume, or for one from executions that miss its first bars, whose levels are not the
    /// session's.
    fn levels_of(&self, plot: &StudyProfile) -> Vec<Level> {
        let profile = &plot.profile;
        if profile.source() == (ProfileSource::Executions { complete: false }) {
            return Vec::new();
        }
        let Some((poc, (va_low, va_high))) = profile.poc_and_value_area(plot.config.value_area())
        else {
            return Vec::new();
        };
        let (low, height) = (profile.low(), profile.row_height());
        LEVELS
            .into_iter()
            .zip(self.extend)
            .filter(|(_, on)| *on)
            .map(|(kind, _)| {
                let (price, row) = match kind {
                    ProfileLevelKind::PointOfControl => (low + (poc as f64 + 0.5) * height, poc),
                    ProfileLevelKind::ValueAreaHigh => {
                        (low + (va_high + 1) as f64 * height, va_high)
                    }
                    ProfileLevelKind::ValueAreaLow => (low + va_low as f64 * height, va_low),
                };
                Level {
                    kind,
                    price,
                    grid: (low, height),
                    row: row as f64,
                    touch: None,
                    searched: plot.bars.end,
                }
            })
            .collect()
    }
}

/// The least of 1, 2 and 5 times a power of ten that reaches a positive `value`.
fn nice_ceil(value: f64) -> Option<f64> {
    let power = 10_f64.powi(value.log10().floor() as i32);
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|step| step * power)
        .find(|step| *step >= value * (1.0 - 1e-9))
}

/// The ranges of consecutive bars whose keys are equal, in order.
fn runs<K: PartialEq>(keys: impl IntoIterator<Item = K>) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut previous = None;
    for (i, key) in keys.into_iter().enumerate() {
        match ranges.last_mut() {
            Some(range) if previous.as_ref() == Some(&key) => range.end = i + 1,
            _ => ranges.push(i..i + 1),
        }
        previous = Some(key);
    }
    ranges
}

/// `units`, consecutive bar ranges, grouped `count` at a time, a group starting at the first unit
/// that opens at or after `start` (the first unit when there is none, or no `start`). `start`
/// becomes the open of the last group's first bar; without units it is left as it was, since
/// the chart empties a study before recomputing it.
fn counted(
    units: Vec<Range<usize>>,
    count: usize,
    data: &[Bar],
    start: &mut Option<DateTime<Utc>>,
) -> Vec<Range<usize>> {
    if count <= 1 || units.is_empty() {
        return units;
    }
    let at = start.map_or(0, |start| data.partition_point(|bar| bar.time < start));
    let first = match units.partition_point(|unit| unit.start < at) {
        first if first == units.len() => 0,
        first => first,
    };
    // Whole groups of `count` lead up to the group at `first`; the oldest takes what is left.
    let (head, rest) = units.split_at(first % count);
    let groups: Vec<_> = std::iter::once(head)
        .filter(|head| !head.is_empty())
        .chain(rest.chunks(count))
        .map(|group| group[0].start..group[group.len() - 1].end)
        .collect();
    *start = groups.last().map(|last| data[last.start].time);
    groups
}

/// `units`, consecutive bar ranges each within one year of `years` (each bar's trading year),
/// grouped `count` at a time from the first unit of each year, the oldest year's as [`counted`]
/// groups them from `start`, since its first bar may not be loaded yet.
fn restarted(
    units: Vec<Range<usize>>,
    years: &[i32],
    count: usize,
    data: &[Bar],
    start: &mut Option<DateTime<Utc>>,
) -> Vec<Range<usize>> {
    let mut groups = Vec::new();
    for (i, year) in runs(units.iter().map(|unit| years[unit.start]))
        .into_iter()
        .enumerate()
    {
        let units = units[year].to_vec();
        groups.extend(if i == 0 {
            counted(units, count, data, start)
        } else {
            counted(units, count, data, &mut None)
        });
    }
    groups
}

macro_rules! profile_study {
    ($ty:ident, $kind:ident, $name:literal, $doc:literal, $hd:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug)]
        pub struct $ty {
            profile: ProfileStudy,
        }
        impl $ty {
            /// Create a profile targeting this many price rows, at least one.
            /// Counts above 1,000 produce no profile; settings accept 1 to 1,000.
            pub fn new(row_count: usize) -> Self {
                let mut profile = ProfileStudy::new(Kind::$kind, row_count);
                profile.hd = $hd;
                profile.adaptive = $hd;
                Self { profile }
            }

            /// What each profile in the `visible` chart bars was built from, with the share of
            /// its bars that the finer source interval covers.
            pub fn profile_inputs(&self, visible: Range<usize>) -> Vec<ProfileInput> {
                self.profile.profile_inputs(visible)
            }
        }
        impl Default for $ty {
            fn default() -> Self {
                Self::new(24)
            }
        }
        impl Indicator for $ty {
            fn name(&self) -> &str {
                $name
            }
            fn desc(&self) -> &str {
                $doc
            }
            fn inputs(&self) -> Vec<StudyInput> {
                self.profile
                    .inputs()
                    .into_iter()
                    .zip(Self::default().profile.inputs())
                    .map(|(input, default)| input.with_default(default.value))
                    .collect()
            }
            fn set_input(
                &mut self,
                id: &str,
                value: InputValue,
            ) -> Result<(), crate::studies::StudyError> {
                self.profile.set_input(id, value)
            }
            fn calculate(&mut self, data: &[Bar]) {
                self.profile.calculate(data);
            }
            fn update(&mut self, data: &[Bar], from: usize) {
                self.profile.update(data, from);
            }
            fn data_requests(&self) -> Vec<StudyDataRequest> {
                self.profile.data_requests()
            }
            fn data_start(&self, _: &StudyDataRequest) -> Option<DateTime<Utc>> {
                self.profile.data_start()
            }
            fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
                self.profile.update_data(data, from, sources);
            }
            fn order_flow_inputs(&self) -> &[OrderFlowInput] {
                std::slice::from_ref(&self.profile.flow)
            }
            fn needs_order_flow(&self) -> bool {
                false
            }
            // The finer bars are chosen below the interval the profile is calculated on, so they
            // apply on any Interval.
            fn requests_allow_other_intervals(&self) -> bool {
                true
            }
            fn volume_profiles(&self, visible: Range<usize>) -> Vec<StudyProfile> {
                self.profile.profiles(visible)
            }
            fn profile_levels(&self, visible: Range<usize>) -> Vec<ProfileLevel> {
                self.profile.levels(visible)
            }
            fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
                self.profile.resolutions =
                    instrument.map_or_else(Vec::new, |i| i.profile_resolutions.clone());
                let tick_size = instrument
                    .and_then(|i| i.tick_size)
                    .filter(|t| t.is_finite() && *t > 0.0);
                let tick_changed = self.profile.tick_size != tick_size;
                self.profile.tick_size = tick_size;
                // Formats the row values only, so it changes no calculation.
                self.profile.volume_decimals = instrument.and_then(|i| i.volume_decimals);
                let hours = instrument.map_or((TimezoneId::utc(), None), |i| {
                    let zone = i.time_zone.clone().unwrap_or_else(TimezoneId::utc);
                    (zone, i.session.clone())
                });
                // Sessions of regular or extended hours read what the trading day ignores: the
                // extended windows, holidays and date-specific hours.
                let changed = tick_changed || self.profile.hours != hours;
                self.profile.hours = hours;
                self.profile.days.set(instrument) || changed
            }
            fn set_visible_range(&mut self, range: Range<usize>) -> bool {
                if self.profile.view != range {
                    let from = if self.profile.view.start == range.start {
                        self.profile.view.end.min(range.end).saturating_sub(1)
                    } else {
                        0
                    };
                    self.profile.view = range;
                    if self.profile.kind == Kind::Visible {
                        self.profile
                            .refresh_developing(from.min(self.profile.data.len()));
                    }
                }
                false
            }
            fn set_timeframe(&mut self, timeframe: Timeframe) {
                self.profile.interval = timeframe;
                self.profile.clock = self.profile.clock.for_timeframe(timeframe);
            }
            fn set_time_scale_events(&mut self, events: &[TimeScaleEvent]) -> bool {
                self.profile.set_events(events)
            }
            fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
                let changed = self.profile.clock != *clock;
                self.profile.clock = clock.clone();
                changed
            }
            fn values(&self) -> &IndicatorValues {
                &self.profile.values
            }
            fn colors(&self) -> Vec<Color32> {
                self.profile.colors.clone()
            }
            fn set_colors(&mut self, colors: Vec<Color32>) {
                let offset = if colors.len() == 5 { 3 } else { 0 };
                for (stored, color) in self.profile.colors.iter_mut().skip(offset).zip(colors) {
                    *stored = color;
                }
                self.profile.cache.get_mut().clear();
            }
            fn line_names(&self) -> Vec<String> {
                ["Developing POC", "Developing VA High", "Developing VA Low"]
                    .map(str::to_owned)
                    .to_vec()
            }
            fn plot_style(&self, line: usize) -> PlotStyle {
                PlotStyle {
                    color: self
                        .profile
                        .colors
                        .get(line)
                        .copied()
                        .unwrap_or(Color32::WHITE),
                    width: self.line_style(line).width,
                    pattern: crate::model::LineStyle::Solid,
                    plot_type: if line < 3 {
                        PlotType::StepLine
                    } else {
                        PlotType::Line
                    },
                    visible: line >= 3,
                }
            }
            fn is_visible(&self) -> bool {
                self.profile.visible
            }
            fn set_visible(&mut self, visible: bool) {
                self.profile.visible = visible;
            }
            fn clone_box(&self) -> Box<dyn Indicator> {
                Box::new(self.clone())
            }
            fn clone_for_export(&self) -> Box<dyn Indicator> {
                let mut frozen = self.clone();
                frozen.profile.flow = self.profile.flow.frozen();
                Box::new(frozen)
            }
        }
    };
}
profile_study!(
    VolProfileVisible,
    Visible,
    "Vol Profile VR",
    "Volume distributed over the bars currently visible in the chart.",
    false
);
profile_study!(
    VolProfileFixed,
    Fixed,
    "Vol Profile FR",
    "Volume between inclusive saved start and end times; all loaded bars until times are chosen.",
    false
);
profile_study!(
    VolProfileSession,
    Session,
    "Vol Profile Session",
    "One volume profile per exchange trading day, chosen session hours or segment of the day, including the whole session of visible bars.",
    false
);
profile_study!(
    VolProfilePeriodic,
    Periodic,
    "Vol Profile Periodic",
    "One volume profile per chosen number of calendar periods, bars, minutes or hours, one week by default.",
    false
);
profile_study!(
    VolProfileAnchored,
    Anchored,
    "Vol Profile Anchored",
    "Volume from a saved start time to the latest bar; all loaded bars until an anchor is chosen.",
    false
);
profile_study!(
    VolProfileAutoAnchored,
    Auto,
    "Vol Profile Auto Anchored",
    "Volume from the latest calendar anchor, an extreme in the chosen lookback, or the latest earnings, dividend or split the chart holds.",
    false
);

profile_study!(
    VolProfileSessionHd,
    Session,
    "Vol Profile Session HD",
    "Volume profiles per session with rows that follow the zoom.",
    true
);

impl VolProfileFixed {
    /// Include bars from `start` through `end`. Reversed times select no bars.
    pub fn with_range(
        mut self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        self.profile.start_time = start.timestamp_millis() as f64;
        self.profile.end_time = end.timestamp_millis() as f64;
        self.profile.data.clear();
        self.profile.values.clear();
        self.profile.cache.get_mut().clear();
        self
    }
}
impl VolProfileAnchored {
    /// Include bars at or after `start`, retaining the time when history is prepended.
    pub fn with_anchor(mut self, start: chrono::DateTime<chrono::Utc>) -> Self {
        self.profile.start_time = start.timestamp_millis() as f64;
        self.profile.data.clear();
        self.profile.values.clear();
        self.profile.cache.get_mut().clear();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hourly bars in three UTC sessions. With height 2, day one holds 2, 4, 2 in
    /// [98,100), [100,102), [102,104), so its POC is 101 and its 70% area is [98,104).
    pub(super) fn level_bars() -> Vec<Bar> {
        let time: DateTime<Utc> = "2024-01-02T10:00:00Z".parse().unwrap();
        [
            (0, 99.0, 99.0, 99.0, 99.0, 2.0),
            (1, 101.0, 101.0, 101.0, 101.0, 4.0),
            (2, 103.0, 103.0, 103.0, 103.0, 2.0),
            (24, 103.0, 104.0, 103.0, 104.0, 1.0),
            (25, 104.0, 104.0, 103.0, 103.0, 1.0),
            (48, 101.0, 101.0, 101.0, 101.0, 1.0),
            (49, 99.0, 99.0, 99.0, 99.0, 1.0),
        ]
        .into_iter()
        .map(|(hour, open, high, low, close, volume)| {
            Bar::new(
                time + TimeDelta::hours(hour),
                open,
                high,
                low,
                close,
                volume,
            )
        })
        .collect()
    }

    /// Finished session and periodic levels survive untouched sessions, end at independent
    /// row revisits and stay consistent through corrections, appends, recomputes and eviction.
    #[test]
    fn finished_profiles_extend_their_levels_to_the_first_bar_trading_in_their_row() {
        use crate::data::ProfileLevelKind::{
            PointOfControl as P, ValueAreaHigh as H, ValueAreaLow as V,
        };
        use crate::studies::ConfiguredStudy;
        let bars = level_bars();
        let levels = |study: &dyn Indicator, view| -> Vec<_> {
            study
                .profile_levels(view)
                .into_iter()
                .map(|l| (l.kind, l.price, l.bars))
                .collect()
        };
        let expected = vec![
            (P, 101.0, 3..6),
            (H, 104.0, 3..4),
            (V, 98.0, 3..7),
            (P, 103.0, 5..7),
            (H, 104.0, 5..7),
            (V, 102.0, 5..7),
        ];
        let mut periodic = VolProfilePeriodic::new(24);
        periodic.set_input("period", InputValue::Choice(0)).unwrap();
        for mut study in [
            Box::new(VolProfileSession::new(24)) as Box<dyn Indicator>,
            Box::new(periodic),
        ] {
            study.calculate(&bars);
            assert!(study.profile_levels(0..7).is_empty());
            study
                .set_input("row_size", InputValue::Number(2.0))
                .unwrap();
            for id in ["extend_poc", "extend_vah", "extend_val"] {
                study.set_input(id, InputValue::Toggle(true)).unwrap();
            }
            for (len, from, wanted) in [
                (
                    5,
                    0,
                    vec![(P, 101.0, 3..5), (H, 104.0, 3..4), (V, 98.0, 3..5)],
                ),
                (
                    6,
                    5,
                    vec![
                        (P, 101.0, 3..6),
                        (H, 104.0, 3..4),
                        (V, 98.0, 3..6),
                        (P, 103.0, 5..6),
                        (H, 104.0, 5..6),
                        (V, 102.0, 5..6),
                    ],
                ),
                (7, 6, expected.clone()),
            ] {
                if from == 0 {
                    study.calculate(&bars[..len]);
                } else {
                    study.update(&bars[..len], from);
                }
                assert_eq!(
                    levels(study.as_ref(), 0..len),
                    wanted,
                    "{} length {len}",
                    study.name()
                );
                let mut fresh = study.clone_box();
                fresh.calculate(&bars[..len]);
                assert_eq!(levels(fresh.as_ref(), 0..len), wanted);
            }
            // Bar 5 now trades only at 104: clear of day one's POC row, and on day two's VAH
            // line, the top edge of its row, which ends that line.
            let mut corrected = bars.clone();
            corrected[5] = Bar::new(bars[5].time, 104.0, 104.0, 104.0, 104.0, 1.0);
            study.update(&corrected, 5);
            let mut wanted = expected.clone();
            wanted[0].2 = 3..7;
            wanted[4].2 = 5..6;
            assert_eq!(levels(study.as_ref(), 0..7), wanted);
            let mut fresh = study.clone_box();
            fresh.calculate(&corrected);
            assert_eq!(levels(fresh.as_ref(), 0..7), wanted);
            // Correcting a finished profile changes its own levels; moving a boundary bar
            // into the earlier session changes both profiles' spans.
            for (from, replacement, first) in [
                (
                    1,
                    Bar::new(bars[1].time, 99.0, 99.0, 99.0, 99.0, 4.0),
                    vec![(P, 99.0, 3..7), (H, 100.0, 3..7), (V, 98.0, 3..7)],
                ),
                (
                    3,
                    Bar {
                        time: bars[2].time + TimeDelta::hours(1),
                        ..bars[3].clone()
                    },
                    vec![(P, 101.0, 4..6), (H, 104.0, 4..5), (V, 100.0, 4..6)],
                ),
            ] {
                study.calculate(&bars);
                assert_eq!(levels(study.as_ref(), 0..7), expected);
                let mut corrected = bars.clone();
                corrected[from] = replacement;
                study.update(&corrected, from);
                let wanted: Vec<_> = first
                    .into_iter()
                    .chain(expected[3..].iter().cloned())
                    .collect();
                assert_eq!(levels(study.as_ref(), 0..7), wanted);
                let mut fresh = study.clone_box();
                fresh.calculate(&corrected);
                assert_eq!(levels(fresh.as_ref(), 0..7), wanted);
            }
            study.calculate(&bars);
            let visible: Vec<_> = expected
                .iter()
                .filter(|(_, _, r)| r.end > 5)
                .cloned()
                .collect();
            assert_eq!(levels(study.as_ref(), 5..7), visible);
            assert!(levels(study.as_ref(), 7..9).is_empty());
            study.set_visible_range(5..7);
            let request = StudyDataRequest {
                symbol: String::new(),
                timeframe: None,
                metric: None,
            };
            assert_eq!(study.data_start(&request), Some(bars[0].time));
            for id in ["extend_poc", "extend_vah"] {
                study.set_input(id, InputValue::Toggle(false)).unwrap();
            }
            study.calculate(&bars);
            assert_eq!(
                levels(study.as_ref(), 0..7),
                vec![(V, 98.0, 3..7), (V, 102.0, 5..7)]
            );
            study
                .set_input("extend_val", InputValue::Toggle(false))
                .unwrap();
            study.calculate(&bars);
            assert_eq!(study.data_start(&request), Some(bars[5].time));
            for id in ["extend_poc", "extend_vah", "extend_val"] {
                study.set_input(id, InputValue::Toggle(true)).unwrap();
            }
            let mut wrapped = ConfiguredStudy::new(study);
            wrapped.calculate(&bars);
            assert_eq!(levels(&wrapped, 0..7), expected);
            assert!(wrapped.remove_oldest_bars(1));
            assert!(wrapped.profile_levels(0..6).is_empty());
            assert!(wrapped.volume_profiles(0..6).is_empty());
            wrapped.calculate(&bars[1..]);
            assert_eq!(
                levels(&wrapped, 0..6),
                vec![
                    (P, 101.0, 2..5),
                    (H, 104.0, 2..3),
                    (V, 100.0, 2..5),
                    (P, 103.0, 4..6),
                    (H, 104.0, 4..6),
                    (V, 102.0, 4..6),
                ]
            );
        }
        // The first finished session has no levels if retained executions miss its first bar.
        // With all executions, its two-tick rows put the POC at 100.5 and end it at bar 5.
        for skip in [0, 1] {
            let mut study = VolProfileSession::new(24);
            study.set_timeframe(Timeframe::Hour1);
            study
                .set_input("row_size", InputValue::Number(2.0))
                .unwrap();
            study
                .set_input("extend_poc", InputValue::Toggle(true))
                .unwrap();
            let flow = &study.order_flow_inputs()[0];
            flow.set_tick_size(Some(1.0));
            let clock = BarClock::new(Timeframe::Hour1);
            for bar in &bars[skip..] {
                flow.push_trade(
                    &clock,
                    &crate::model::Tick::new(bar.time, bar.close, bar.volume, None),
                );
            }
            study.calculate(&bars);
            let profiles = study.volume_profiles(0..7);
            assert_eq!(
                profiles[0].profile.source(),
                ProfileSource::Executions {
                    complete: skip == 0
                }
            );
            let got = levels(&study, 0..7);
            if skip == 0 {
                assert_eq!(got, vec![(P, 100.5, 3..6), (P, 102.5, 5..7)]);
            } else {
                assert_eq!(got, vec![(P, 102.5, 5..7)]);
            }
        }
        // With cent rows, a row edge computed from the profile's low can round a few steps off
        // the traded price it stands for. Day one trades only at its first price and day two
        // only at its second, which ends day one's levels in that row or on that line. A second
        // bar without volume or with a NaN high trades nowhere.
        for (first, (low, high, volume), wanted) in [
            (
                32.01,
                (32.02, 32.02, 1.0),
                vec![(P, 1..3), (H, 1..2), (V, 1..3)],
            ),
            (
                32.16,
                (32.16, 32.16, 1.0),
                vec![(P, 1..2), (H, 1..2), (V, 1..2)],
            ),
            (
                32.16,
                (32.16, 32.16, 0.0),
                vec![(P, 1..3), (H, 1..3), (V, 1..3)],
            ),
            (
                32.01,
                (32.02, f64::NAN, 1.0),
                vec![(P, 1..3), (H, 1..3), (V, 1..3)],
            ),
        ] {
            let bars: Vec<_> = [
                (first, first, first, 1.0),
                (low, high, low, volume),
                (40.0, 40.0, 40.0, 1.0),
            ]
            .into_iter()
            .zip([0, 3, 5])
            .map(|((low, high, close, volume), i)| {
                Bar::new(bars[i].time, close, high, low, close, volume)
            })
            .collect();
            let mut study = VolProfileSession::new(24);
            study
                .set_input("row_size", InputValue::Number(0.01))
                .unwrap();
            for id in ["extend_poc", "extend_vah", "extend_val"] {
                study.set_input(id, InputValue::Toggle(true)).unwrap();
            }
            study.calculate(&bars);
            let got: Vec<_> = study
                .profile_levels(0..3)
                .into_iter()
                .filter(|level| level.bars.start == 1)
                .map(|level| (level.kind, level.bars))
                .collect();
            assert_eq!(
                got, wanted,
                "{first} then {low}..{high} with volume {volume}"
            );
        }
    }

    /// Profiles sum the executions held for their bars, in rows of whole ticks within the row
    /// count or of the row height, as a footprint with that many ticks per row sums them. One
    /// whose executions start inside it is partial, and rows of bars before the chart's first bar
    /// are left out; one without them, or whose bars they cannot be assigned to exactly, keeps the
    /// estimate.
    #[test]
    fn profiles_read_the_executions_of_their_bars() {
        use crate::data::{Footprint, ProfileSource};
        use crate::model::{
            BarClock,
            OrderSide::{Buy, Sell},
            Tick,
        };
        let time = |at: &str| at.parse::<DateTime<Utc>>().unwrap();
        let bar = |at: &str| Bar::new(time(at), 100.0, 105.0, 99.0, 102.0, 10.0);
        let trade = |at: &str, price, size, side| Tick::new(time(at), price, size, side);
        let bars: Vec<Bar> = [
            "2026-03-06T23:58:00Z",
            "2026-03-06T23:59:00Z",
            "2026-03-07T00:00:00Z",
            "2026-03-07T00:01:00Z",
        ]
        .map(bar)
        .into();
        // Bar 2 trades 2 bought at 100 and 1 sold at 101; bar 3 trades 1 bought at 101 and 3 of
        // unknown side at 104.
        let trades = vec![
            trade("2026-03-07T00:00:10Z", 100.0, 2.0, Some(Buy)),
            trade("2026-03-07T00:00:20Z", 101.0, 1.0, Some(Sell)),
            trade("2026-03-07T00:01:10Z", 101.0, 1.0, Some(Buy)),
            trade("2026-03-07T00:01:20Z", 104.0, 3.0, None),
        ];
        let negative = vec![
            trade("2026-03-07T00:00:10Z", -1.0, 1.0, Some(Buy)),
            trade("2026-03-07T00:00:20Z", -3.0, 2.0, Some(Sell)),
        ];
        // Range bars of these executions end at 00:00:20 and 00:01:30.
        let ranged = vec![bar("2026-03-07T00:00:20Z"), bar("2026-03-07T00:01:30Z")];
        let range_trades = vec![
            trade("2026-03-07T00:00:10Z", 100.0, 1.0, Some(Buy)),
            trade("2026-03-07T00:00:40Z", 101.0, 1.0, Some(Buy)),
            trade("2026-03-07T00:01:20Z", 102.0, 1.0, Some(Buy)),
        ];
        let gap = vec![bars[0].clone(), bars[1].clone(), bars[3].clone()];
        // A buy of 5 at 100 in the bar at 23:59, which a bar limit has dropped from the chart.
        let dropped = [
            vec![trade("2026-03-06T23:59:30Z", 100.0, 5.0, Some(Buy))],
            trades.clone(),
        ]
        .concat();
        let mut shared = bars.clone();
        shared.insert(3, bars[2].clone());
        let fixed = |rows, first: &Bar, last: &Bar| -> Box<dyn Indicator> {
            Box::new(VolProfileFixed::new(rows).with_range(first.time, last.time))
        };
        let mut height = fixed(24, &bars[2], &bars[3]);
        height
            .set_input("row_size", InputValue::Number(1.6))
            .unwrap();
        let minute = BarClock::new(Timeframe::Min1);
        let (complete, partial, estimated) = (
            ProfileSource::Executions { complete: true },
            ProfileSource::Executions { complete: false },
            ProfileSource::Estimated,
        );
        // Ticks per row, low, and volume, buys and sells per row.
        type Rows = (u32, f64, Vec<f64>, Vec<f64>, Vec<f64>);
        let per_tick: Rows = (
            1,
            99.5,
            vec![2.0, 2.0, 0.0, 0.0, 3.0],
            vec![2.0, 1.0, 0.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0, 0.0, 0.0],
        );
        type Expected = Vec<(Range<usize>, ProfileSource, Option<Rows>)>;
        type Case = (
            &'static str,
            Box<dyn Indicator>,
            Vec<Bar>,
            Vec<Tick>,
            BarClock,
            Range<usize>,
            Expected,
        );
        let wide = vec![
            trade("2026-03-07T00:00:10Z", 100.0, 1.0, Some(Buy)),
            trade("2026-03-07T00:00:20Z", 199.0, 1.0, Some(Buy)),
        ];
        let ends = |n: usize| {
            std::iter::once(1.0)
                .chain(std::iter::repeat_n(0.0, n - 2))
                .chain([1.0])
                .collect::<Vec<_>>()
        };
        let rows: Vec<Case> = vec![
            (
                "closest ticks at 30 rows",
                fixed(30, &bars[2], &bars[3]),
                bars.clone(),
                wide.clone(),
                minute.clone(),
                0..4,
                vec![(
                    2..4,
                    complete,
                    Some((3, 98.5, ends(34), ends(34), vec![0.0; 34])),
                )],
            ),
            (
                "closest ticks at 25 rows",
                fixed(25, &bars[2], &bars[3]),
                bars.clone(),
                wide,
                minute.clone(),
                0..4,
                vec![(
                    2..4,
                    complete,
                    Some((4, 99.5, ends(25), ends(25), vec![0.0; 25])),
                )],
            ),
            // Levels 100..=104 take 5 rows of 24.
            (
                "single ticks within the row count",
                fixed(24, &bars[2], &bars[3]),
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![(2..4, complete, Some(per_tick.clone()))],
            ),
            // ceil(5 / 2) = 3 ticks: rows 33 (99..=101) and 34 (102..=104).
            (
                "the traded ticks over the row count",
                fixed(2, &bars[2], &bars[3]),
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![(
                    2..4,
                    complete,
                    Some((3, 98.5, vec![4.0, 3.0], vec![3.0, 0.0], vec![1.0, 0.0])),
                )],
            ),
            // Levels 101..=104 take 2 ticks per row, spanning rows 50, 51 and 52.
            (
                "levels across a multiple keep the closest whole ticks",
                fixed(2, &bars[3], &bars[3]),
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![(
                    3..4,
                    complete,
                    Some((
                        2,
                        99.5,
                        vec![1.0, 0.0, 3.0],
                        vec![1.0, 0.0, 0.0],
                        vec![0.0, 0.0, 0.0],
                    )),
                )],
            ),
            // round(1.6) = 2 ticks: rows 50, 51, 52.
            (
                "the row height in whole ticks",
                height,
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![(
                    2..4,
                    complete,
                    Some((
                        2,
                        99.5,
                        vec![4.0, 0.0, 3.0],
                        vec![3.0, 0.0, 0.0],
                        vec![1.0, 0.0, 0.0],
                    )),
                )],
            ),
            // Held from 00:00, after the profile's first bar at 23:59.
            (
                "executions from inside the profile",
                fixed(24, &bars[1], &bars[3]),
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![(1..4, partial, Some(per_tick.clone()))],
            ),
            (
                "a session before the executions",
                Box::new(VolProfileSession::new(24)),
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![
                    (0..2, estimated, None),
                    (2..4, complete, Some(per_tick.clone())),
                ],
            ),
            // Levels -3 and -1 in 2-tick rows: rows -2 (-4..=-3) and -1 (-2..=-1).
            (
                "negative prices group downwards",
                fixed(2, &bars[2], &bars[3]),
                bars.clone(),
                negative,
                minute.clone(),
                0..4,
                vec![(
                    2..4,
                    complete,
                    Some((2, -4.5, vec![2.0, 1.0], vec![0.0, 1.0], vec![2.0, 0.0])),
                )],
            ),
            (
                "executions of another calendar",
                fixed(24, &bars[2], &bars[3]),
                bars.clone(),
                trades.clone(),
                BarClock::new(Timeframe::Min5),
                0..4,
                vec![(2..4, estimated, None)],
            ),
            (
                "executions of a dropped bar before the first bar",
                Box::new(VolProfileVisible::new(24)),
                bars[2..].to_vec(),
                dropped,
                minute.clone(),
                0..2,
                vec![(0..2, complete, Some(per_tick.clone()))],
            ),
            (
                "executions of a bar the chart lacks",
                fixed(24, &bars[1], &bars[3]),
                gap,
                trades.clone(),
                minute.clone(),
                0..3,
                vec![(1..3, estimated, None)],
            ),
            (
                "a bar before sharing the first bar's slot",
                Box::new(VolProfileVisible::new(24)),
                shared.clone(),
                trades.clone(),
                minute.clone(),
                3..5,
                vec![(3..5, estimated, None)],
            ),
            (
                "a bar after sharing the last bar's slot",
                Box::new(VolProfileVisible::new(24)),
                shared,
                trades.clone(),
                minute.clone(),
                2..3,
                vec![(2..3, estimated, None)],
            ),
            (
                "a bar stamped inside its slot",
                fixed(24, &ranged[1], &ranged[1]),
                ranged,
                range_trades,
                minute.clone(),
                0..2,
                vec![(1..2, estimated, None)],
            ),
            (
                "unordered bar opens keep the estimate",
                Box::new(VolProfileVisible::new(24)),
                vec![
                    bars[3].clone(),
                    bar("2026-03-07T00:02:00Z"),
                    bars[2].clone(),
                ],
                trades.clone(),
                minute.clone(),
                1..3,
                vec![(1..3, estimated, None)],
            ),
        ];
        for (label, mut study, bars, trades, clock, view, expected) in rows {
            let flow = study.order_flow_inputs()[0].clone();
            flow.set_tick_size(Some(1.0));
            for trade in &trades {
                flow.push_trade(&clock, trade);
            }
            study.calculate(&bars);
            let got = study.volume_profiles(view);
            assert_eq!(got.len(), expected.len(), "{label}");
            for (got, (range, source, rows)) in got.iter().zip(expected) {
                assert_eq!(got.bars, range, "{label}");
                assert_eq!(got.profile.source(), source, "{label}");
                let Some((ticks, low, volume, up, down)) = rows else {
                    continue;
                };
                let profile = &*got.profile;
                assert_eq!(
                    (profile.low(), profile.row_height()),
                    (low, f64::from(ticks)),
                    "{label}"
                );
                assert_eq!(profile.volume(), volume, "{label}");
                assert_eq!(profile.up(), up, "{label}");
                assert_eq!(profile.down(), down, "{label}");
                // The same executions, grouped by a footprint as they arrive.
                let mut footprint = Footprint::new(clock.clone(), 1.0, ticks).unwrap();
                for trade in &trades {
                    footprint.push(trade).unwrap();
                }
                let span = footprint.profile(bars[range.start].time, bars[range.end - 1].time);
                let independent = VolumeProfile::from_executions(
                    &span,
                    footprint.price_step(),
                    footprint.tick_size(),
                    source == complete,
                );
                assert_eq!(Some(profile), independent.as_ref(), "{label}");
            }
        }
        // A later execution in the last bar shows at the next update, as a fresh calculation has.
        let mut study = fixed(24, &bars[2], &bars[3]);
        let flow = study.order_flow_inputs()[0].clone();
        flow.set_tick_size(Some(1.0));
        for trade in &trades {
            flow.push_trade(&minute, trade);
        }
        study.calculate(&bars);
        assert_eq!(
            study.volume_profiles(0..4)[0].profile.volume(),
            [2.0, 2.0, 0.0, 0.0, 3.0]
        );
        flow.push_trade(&minute, &trade("2026-03-07T00:01:30Z", 104.0, 2.0, None));
        study.update(&bars, 3);
        let got = study.volume_profiles(0..4);
        assert_eq!(got[0].profile.volume(), [2.0, 2.0, 0.0, 0.0, 5.0]);
        let mut fresh = study.clone_box();
        fresh.calculate(&bars);
        assert_eq!(
            format!("{got:?}"),
            format!("{:?}", fresh.volume_profiles(0..4))
        );
    }

    /// Zoom changes the shared tick grid without reusing cached rows or adding volume.
    #[test]
    fn adaptive_session_rows_follow_the_sessions_in_view() {
        let at = |time: &str| time.parse::<DateTime<Utc>>().unwrap();
        let bars = [
            Bar::new(
                at("2026-03-02T09:00:00Z"),
                100.0,
                110.0,
                100.0,
                110.0,
                100.0,
            ),
            Bar::new(at("2026-03-03T09:00:00Z"), 100.0, 101.0, 100.0, 101.0, 40.0),
            Bar::new(at("2026-03-03T10:00:00Z"), 101.0, 102.0, 101.0, 102.0, 40.0),
        ];
        let mut study = VolProfileSession::new(4);
        study
            .set_input("adaptive_rows", InputValue::Toggle(true))
            .unwrap();
        study.set_instrument(Some(&Instrument::new("SYNTH").with_tick_size(0.25)));
        study.calculate(&bars);
        for (view, height, expected) in [
            (
                0..3,
                2.5,
                vec![vec![25.0, 25.0, 25.0, 25.0, 0.0], vec![80.0]],
            ),
            (1..3, 0.5, vec![vec![20.0, 20.0, 20.0, 20.0, 0.0]]),
            (
                0..3,
                2.5,
                vec![vec![25.0, 25.0, 25.0, 25.0, 0.0], vec![80.0]],
            ),
        ] {
            let profiles = study.volume_profiles(view);
            assert_eq!(profiles.len(), expected.len());
            for (plot, volume) in profiles.iter().zip(expected) {
                assert_eq!(plot.profile.row_height(), height);
                assert_eq!(plot.profile.volume(), volume);
            }
        }
        let mut hd = crate::studies::IndicatorFactory::new()
            .create("Vol Profile Session HD")
            .expect("HD has its own catalogue entry");
        for id in [
            "row_count",
            "row_size",
            "row_ticks",
            "whole_tick_rows",
            "adaptive_rows",
        ] {
            assert!(!hd.inputs().iter().any(|input| input.id == id));
        }
        assert!(hd.set_input("row_count", InputValue::Length(4)).is_err());
        hd.set_instrument(Some(&Instrument::new("SYNTH").with_tick_size(0.25)));
        hd.calculate(&bars);
        for (view, height, expected) in [
            (
                0..3,
                0.5,
                vec![
                    [vec![5.0; 20], vec![0.0]].concat(),
                    vec![20.0, 20.0, 20.0, 20.0, 0.0],
                ],
            ),
            (1..3, 0.25, vec![[vec![10.0; 8], vec![0.0]].concat()]),
        ] {
            let profiles = hd.volume_profiles(view);
            assert_eq!(profiles.len(), expected.len());
            for (profile, expected) in profiles.iter().zip(expected) {
                assert_eq!(profile.profile.row_height(), height);
                assert_eq!(profile.profile.volume(), expected);
            }
        }
        let mut live = bars.to_vec();
        live.push(Bar::new(
            at("2026-03-03T11:00:00Z"),
            101.0,
            102.0,
            100.0,
            102.0,
            20.0,
        ));
        study.update(&live, 3);
        let profiles = study.volume_profiles(1..4);
        assert_eq!(profiles[0].profile.row_height(), 0.5);
        assert_eq!(profiles[0].profile.volume(), [25.0, 25.0, 25.0, 25.0, 0.0]);
        let mut fresh = study.clone_box();
        fresh.calculate(&live);
        assert_eq!(
            format!("{profiles:?}"),
            format!("{:?}", fresh.volume_profiles(1..4))
        );

        study.set_input("row_count", InputValue::Length(3)).unwrap();
        for (instrument, height, volume) in [
            (
                Some(Instrument::new("SYNTH").with_tick_size(0.25)),
                1.25,
                vec![50.0, 30.0],
            ),
            (None, 1.0, vec![40.0, 40.0, 0.0]),
        ] {
            study.set_instrument(instrument.as_ref());
            study.calculate(&bars);
            let profiles = study.volume_profiles(1..3);
            assert_eq!(profiles[0].profile.row_height(), height);
            assert_eq!(profiles[0].profile.volume(), volume);
        }
        let mut flat = VolProfileSession::new(4);
        flat.set_input("adaptive_rows", InputValue::Toggle(true))
            .unwrap();
        flat.calculate(&[Bar::new(bars[0].time, 105.0, 105.0, 105.0, 105.0, 10.0)]);
        let profiles = flat.volume_profiles(0..1);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].config.row_size(), None);
        assert_eq!(profiles[0].profile.volume(), [10.0]);
    }

    /// Suggestions serve an inclusive profile length; explicit choices and the opt-out win. A
    /// profile emptied to wait for its recompute drops a held interval the chart has reached.
    #[test]
    fn feed_suggested_interval_follows_the_profile_length() {
        let start: DateTime<Utc> = "2026-03-02T09:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..4)
            .map(|hour| {
                Bar::new(
                    start + TimeDelta::hours(hour),
                    100.0,
                    101.0,
                    99.0,
                    100.0,
                    10.0,
                )
            })
            .collect();
        let instrument = Instrument::new("SYNTH").with_profile_resolutions([
            ProfileResolution::new(
                Timeframe::Volume(std::num::NonZeroU64::new(1).unwrap()),
                None,
            ),
            ProfileResolution::new(Timeframe::Min1, Some(TimeDelta::hours(2))),
            ProfileResolution::new(Timeframe::Min5, Some(TimeDelta::hours(4))),
            ProfileResolution::new(Timeframe::Min15, None),
            ProfileResolution::new(Timeframe::Hour1, None),
            ProfileResolution::new(
                Timeframe::Ticks(std::num::NonZeroU64::new(1).unwrap()),
                None,
            ),
        ]);
        for case in 0..11 {
            let mut study: Box<dyn Indicator> = match case {
                1 => Box::new(VolProfileFixed::new(4).with_range(bars[0].time, bars[1].time)),
                2..=4 => Box::new(VolProfileVisible::new(4)),
                _ => Box::new(VolProfileFixed::new(4)),
            };
            study.set_timeframe(if case == 8 {
                Timeframe::Min5
            } else {
                Timeframe::Hour1
            });
            if case == 5 {
                study
                    .set_input(
                        "source_interval",
                        InputValue::Timeframe(Some(Timeframe::Min15)),
                    )
                    .unwrap();
            }
            if case == 6 {
                study
                    .set_input("feed_source", InputValue::Toggle(false))
                    .unwrap();
            }
            study.set_instrument((case != 7).then_some(&instrument));
            if case != 9 {
                study.calculate(&bars);
            }
            study.set_visible_range(match case {
                2 => 0..1,
                3 => 0..9,
                4 => 4..9,
                _ => 0..4,
            });
            if case == 10 {
                // Emptied to wait for a recompute, then the chart moves to the held interval.
                study.update(&[], 0);
                study.set_timeframe(Timeframe::Min5);
            }
            let expected = match case {
                0 | 3 => Some(Timeframe::Min5),
                1 | 2 => Some(Timeframe::Min1),
                5 => Some(Timeframe::Min15),
                _ => None,
            };
            assert_eq!(
                study.data_requests().first().and_then(|r| r.timeframe),
                expected,
                "case {case}"
            );
        }
    }

    /// Range selection uses bar opens and inclusive saved times; calendar profiles retain
    /// whole exchange periods while a visible profile includes only the requested bars.
    #[test]
    fn profiles_use_only_their_selected_bars() {
        let times = [
            "2026-03-06T22:00:00Z",
            "2026-03-06T23:00:00Z",
            "2026-03-07T00:00:00Z",
            "2026-03-09T01:00:00Z",
        ];
        let bars: Vec<_> = times
            .iter()
            .enumerate()
            .map(|(i, t)| {
                Bar::new(
                    t.parse().unwrap(),
                    1.0,
                    2.0,
                    0.0,
                    if i == 2 { 0.5 } else { 1.5 },
                    (i + 1) as f64 * 10.0,
                )
            })
            .collect();
        type Case = (Box<dyn Indicator>, Vec<(Range<usize>, f64)>);
        let mut extended = VolProfileFixed::new(2).with_range(bars[1].time, bars[2].time);
        extended
            .set_input("extend_right", InputValue::Toggle(true))
            .unwrap();
        assert_eq!(
            extended
                .inputs()
                .iter()
                .find(|input| input.id == "end_time")
                .unwrap()
                .value,
            InputValue::Text(time_text(bars[2].time.timestamp_millis() as f64))
        );
        let cases: Vec<Case> = vec![
            (Box::new(extended), vec![(1..4, 90.0)]),
            (Box::new(VolProfileVisible::new(2)), vec![(1..4, 90.0)]),
            (
                Box::new(VolProfileFixed::new(2).with_range(bars[1].time, bars[2].time)),
                vec![(1..3, 50.0)],
            ),
            (
                Box::new(VolProfileAnchored::new(2).with_anchor(bars[2].time)),
                vec![(2..4, 70.0)],
            ),
            (
                Box::new(VolProfileSession::new(2)),
                vec![(0..2, 30.0), (2..3, 30.0), (3..4, 40.0)],
            ),
            (
                Box::new(VolProfilePeriodic::new(2)),
                vec![(0..3, 60.0), (3..4, 40.0)],
            ),
            (Box::new(VolProfileAutoAnchored::new(2)), vec![(3..4, 40.0)]),
        ];
        let request = StudyDataRequest {
            symbol: String::new(),
            timeframe: None,
            metric: None,
        };
        for (mut study, expected) in cases {
            let defaults = crate::studies::IndicatorFactory::new()
                .create(study.name())
                .unwrap()
                .inputs();
            study
                .set_input("value_area", InputValue::Number(90.0))
                .unwrap();
            for (input, default) in study.inputs().iter().zip(&defaults) {
                assert_eq!(
                    input.default,
                    default.value,
                    "{}: {}",
                    study.name(),
                    input.id
                );
            }
            // Saved UTC times, and the session study's empty custom zone, are displayed and
            // accepted back without changing the range.
            let times: Vec<_> = study
                .inputs()
                .into_iter()
                .filter(|input| {
                    matches!(input.value, InputValue::Text(_) | InputValue::TimeZone(_))
                })
                .map(|input| (input.id, input.value))
                .collect();
            let text = |text: &str| InputValue::Text(text.into());
            let expected_times = match study.name() {
                "Vol Profile FR" => vec![
                    ("start_time", text("2026-03-06 23:00:00")),
                    ("end_time", text("2026-03-07 00:00:00")),
                ],
                "Vol Profile Anchored" => vec![("start_time", text("2026-03-07 00:00:00"))],
                "Vol Profile Session" => [
                    "session_zone",
                    "session_2_zone",
                    "session_3_zone",
                    "session_4_zone",
                    "session_5_zone",
                ]
                .map(|id| (id, InputValue::TimeZone(String::new())))
                .to_vec(),
                _ => Vec::new(),
            };
            assert_eq!(
                times
                    .iter()
                    .map(|(id, value)| (id.as_str(), value.clone()))
                    .collect::<Vec<_>>(),
                expected_times,
                "{}",
                study.name()
            );
            for (id, value) in times {
                study.set_input(&id, value).unwrap();
            }
            study
                .set_input("value_area", InputValue::Number(70.0))
                .unwrap();
            study.calculate(&bars);
            let got = study.volume_profiles(1..4);
            assert_eq!(got.len(), expected.len(), "{}", study.name());
            let first = expected[0].0.start;
            for (got, (range, volume)) in got.iter().zip(expected) {
                assert_eq!(got.bars, range, "{}", study.name());
                assert_eq!(got.profile.volume(), [volume / 2.0, volume / 2.0]);
                let down = if range.contains(&2) { 15.0 } else { 0.0 };
                assert_eq!(got.profile.down(), [down, down]);
                assert_eq!(got.profile.up(), [volume / 2.0 - down, volume / 2.0 - down]);
            }
            let cached = study.volume_profiles(1..4);
            assert!(Arc::ptr_eq(&got[0].profile, &cached[0].profile));
            // Finer bars are needed from the first bar of the first profile in view, and with
            // no profile in view from the newest bar on.
            for (view, at) in [(1..4, first), (4..4, 3)] {
                study.set_visible_range(view);
                assert_eq!(
                    study.data_start(&request),
                    Some(bars[at].time),
                    "{}",
                    study.name()
                );
            }
            let mut live = bars.clone();
            for step in 0..3 {
                let from = match step {
                    0 => {
                        live[3].close = 0.5;
                        live[3].volume += 5.0;
                        3
                    }
                    1 => {
                        live.push(Bar::new(
                            live[3].time + chrono::Duration::hours(1),
                            1.0,
                            3.0,
                            -1.0,
                            1.5,
                            8.0,
                        ));
                        3
                    }
                    _ => {
                        live[0].volume += 9.0;
                        0
                    }
                };
                study.update(&live, from);
                let mut fresh = study.clone_box();
                fresh.calculate(&live);
                assert_eq!(
                    format!("{:?}", study.volume_profiles(1..live.len())),
                    format!("{:?}", fresh.volume_profiles(1..live.len())),
                    "{} step {step}",
                    study.name()
                );
            }
        }
        // Fractions survive the editor, junk changes nothing, and empty fields select all bars.
        let mut fixed = VolProfileFixed::new(2).with_range(
            bars[1].time + chrono::Duration::milliseconds(500),
            bars[2].time,
        );
        let start = fixed
            .inputs()
            .into_iter()
            .find(|input| input.id == "start_time")
            .unwrap();
        assert_eq!(
            start.value,
            InputValue::Text("2026-03-06 23:00:00.500".into())
        );
        fixed.set_input("start_time", start.value).unwrap();
        fixed.calculate(&bars);
        assert_eq!(fixed.volume_profiles(0..4)[0].bars, 2..3);
        assert!(
            fixed
                .set_input("start_time", InputValue::Text("23:00".into()))
                .is_err()
        );
        assert_eq!(fixed.volume_profiles(0..4)[0].bars, 2..3);
        fixed
            .set_input("start_time", InputValue::Text("2026-03-06 23:00".into()))
            .unwrap();
        fixed.calculate(&bars);
        assert_eq!(fixed.volume_profiles(0..4)[0].bars, 1..3);
        fixed
            .set_input("start_time", InputValue::Text(String::new()))
            .unwrap();
        fixed
            .set_input("end_time", InputValue::Text(String::new()))
            .unwrap();
        fixed.calculate(&bars);
        assert_eq!(fixed.volume_profiles(0..4)[0].bars, 0..4);
        let mut visible = VolProfileVisible::new(2);
        visible.calculate(&bars);
        assert_eq!(
            visible.volume_profiles(1..3)[0].profile.volume(),
            [25.0, 25.0]
        );
        assert_eq!(
            visible.volume_profiles(0..1)[0].profile.volume(),
            [5.0, 5.0]
        );
        assert!(visible.volume_profiles(4..5).is_empty());
        let mut tick = bars.clone();
        tick[2].close = 1.5;
        tick[2].volume = 50.0;
        visible.update(&tick, 2);
        let got = visible.volume_profiles(1..3);
        assert_eq!(got[0].profile.volume(), [35.0, 35.0]);
        assert_eq!(got[0].profile.down(), [0.0, 0.0]);
        assert!(visible.values().columns().is_empty());

        let mut session = VolProfileSession::new(2);
        session
            .set_instrument(Some(&Instrument::new("SYNTH").with_time_zone(
                crate::model::TimezoneId::fixed_offset(-5 * 3600).unwrap(),
            )));
        session.calculate(&bars);
        let got = session.volume_profiles(1..3);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].bars, 0..3);
        assert_eq!(got[0].profile.volume(), [30.0, 30.0]);
    }

    /// Each session starts at the first bar at or after its open: the exchange day's, the
    /// regular or extended hours', or the custom start's in its own zone or the exchange's, east
    /// or west of UTC, across a clock change and in an hour it skips. Bars outside the hours join
    /// the session before them unless left out; the pre-market, post-market and segment choices
    /// profile only their own bars, and fall back to the trading day without extended hours.
    /// Daily bars and an instrument without hours keep the trading day.
    #[cfg(feature = "timezones")]
    #[test]
    fn session_profiles_start_at_the_chosen_hours() {
        use crate::model::SessionWindow;
        use chrono::Weekday::*;
        let at = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let new_york = TimezoneId::named("America/New_York").unwrap();
        let week = [Mon, Tue, Wed, Thu, Fri];
        let exchange = Instrument::new("X")
            .with_time_zone(new_york.clone())
            .with_session(
                Session::new([SessionWindow::new(week, at(9, 30), at(16, 0))])
                    .with_extended_windows([
                        SessionWindow::new(week, at(4, 0), at(9, 30)),
                        SessionWindow::new(week, at(16, 0), at(20, 0)),
                    ]),
            );
        let regular_only = Instrument::new("X")
            .with_time_zone(new_york.clone())
            .with_session(Session::new([SessionWindow::new(
                week,
                at(9, 30),
                at(16, 0),
            )]));
        let zoned = Instrument::new("X").with_time_zone(new_york);
        // Half-hourly bars from Thursday 5 March 2026 to Tuesday 10 March. New York clocks move
        // from UTC-5 to UTC-4 at 07:00 UTC on Sunday 8 March.
        let start: DateTime<Utc> = "2026-03-05T00:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..288)
            .map(|i| {
                let time = start + TimeDelta::minutes(30 * i);
                Bar::new(time, 100.0, 101.0, 99.0, 100.0, 1.0)
            })
            .collect();
        let starts = |study: &VolProfileSession| {
            let profiles = study.volume_profiles(0..bars.len());
            let starts: Vec<_> = profiles
                .iter()
                .map(|profile| bars[profile.bars.start].time.format("%d %H:%M").to_string())
                .collect();
            starts.join(", ")
        };
        let spans = |study: &VolProfileSession| {
            let at = |i: usize| bars[i].time.format("%d %H:%M").to_string();
            let profiles = study.volume_profiles(0..bars.len());
            let spans: Vec<_> = profiles
                .iter()
                .map(|profile| format!("{}-{}", at(profile.bars.start), at(profile.bars.end - 1)))
                .collect();
            spans.join(", ")
        };
        let calculated =
            |sessions, custom: Option<(usize, usize, &str)>, hours_only, instrument| {
                let mut study = VolProfileSession::new(24);
                study.set_timeframe(Timeframe::Min30);
                study.set_instrument(Some(instrument));
                study
                    .set_input("sessions", InputValue::Choice(sessions))
                    .unwrap();
                if hours_only {
                    study
                        .set_input("hours_only", InputValue::Toggle(true))
                        .unwrap();
                }
                if let Some((open, close, zone)) = custom {
                    study
                        .set_input("session_start", InputValue::Length(open))
                        .unwrap();
                    study
                        .set_input("session_end", InputValue::Length(close))
                        .unwrap();
                    study
                        .set_input("session_zone", InputValue::Text(zone.into()))
                        .unwrap();
                }
                study.calculate(&bars);
                study
            };
        let default_custom = calculated(3, None, true, &exchange);
        let got: Vec<_> = default_custom
            .volume_profiles(0..bars.len())
            .into_iter()
            .flat_map(|profile| profile.bars)
            .collect();
        let zone = exchange.time_zone.as_ref().unwrap();
        let expected: Vec<_> = bars
            .iter()
            .enumerate()
            .filter(|(_, bar)| {
                let local = bar.time.with_timezone(zone).time();
                (at(9, 30)..at(16, 0)).contains(&local)
            })
            .map(|(i, _)| i)
            .collect();
        assert_eq!(got, expected);
        let trading_day = "05 00:00, 05 05:00, 06 05:00, 07 05:00, 08 05:00, 09 04:00, 10 04:00";
        let plus_0530 = "05 00:00, 05 04:00, 06 04:00, 07 04:00, 08 04:00, 09 04:00, 10 04:00";
        for (sessions, custom, instrument, expected) in [
            (0, None, &exchange, trading_day),
            (
                1,
                None,
                &exchange,
                "05 00:00, 05 14:30, 06 14:30, 09 13:30, 10 13:30",
            ),
            (
                2,
                None,
                &exchange,
                "05 00:00, 05 09:00, 06 09:00, 09 08:00, 10 08:00",
            ),
            // 18:00 to 17:00 New York time, every day.
            (
                3,
                Some((1080, 1020, "")),
                &exchange,
                "05 00:00, 05 23:00, 06 23:00, 07 23:00, 08 22:00, 09 22:00, 10 22:00",
            ),
            (
                3,
                Some((1080, 1020, "UTC")),
                &exchange,
                "05 00:00, 05 18:00, 06 18:00, 07 18:00, 08 18:00, 09 18:00, 10 18:00",
            ),
            // 02:30 does not exist in New York on 8 March, so that session opens at 03:00.
            (
                3,
                Some((150, 960, "")),
                &exchange,
                "05 00:00, 05 07:30, 06 07:30, 07 07:30, 08 07:00, 09 06:30, 10 06:30",
            ),
            // 08:00 at UTC+9 is 23:00 UTC the day before.
            (
                3,
                Some((480, 900, "+09:00")),
                &exchange,
                "05 00:00, 05 23:00, 06 23:00, 07 23:00, 08 23:00, 09 23:00, 10 23:00",
            ),
            // 09:30 at UTC+5:30 is 04:00 UTC.
            (3, Some((570, 960, "+05:30")), &exchange, plus_0530),
            (1, None, &zoned, trading_day),
        ] {
            let study = calculated(sessions, custom, false, instrument);
            assert_eq!(starts(&study), expected, "sessions {sessions} {custom:?}");
        }
        // In UTC, Thursday and Friday's pre-market runs from 09:00 to 14:30, their regular hours
        // to 21:00 and their post-market to 01:00; Monday and Tuesday's an hour earlier. The bars
        // at 00:00 and 00:30 on the 5th are Wednesday's post-market.
        for (sessions, custom, hours_only, instrument, expected) in [
            (
                1,
                None,
                true,
                &exchange,
                "05 14:30-05 20:30, 06 14:30-06 20:30, 09 13:30-09 19:30, 10 13:30-10 19:30",
            ),
            (
                2,
                None,
                true,
                &exchange,
                "05 00:00-05 00:30, 05 09:00-06 00:30, 06 09:00-07 00:30, 09 08:00-09 23:30, \
                 10 08:00-10 23:30",
            ),
            // 17:00 to 18:00 New York time is left out.
            (
                3,
                Some((1080, 1020, "")),
                true,
                &exchange,
                "05 00:00-05 21:30, 05 23:00-06 21:30, 06 23:00-07 21:30, 07 23:00-08 20:30, \
                 08 22:00-09 20:30, 09 22:00-10 20:30, 10 22:00-10 23:30",
            ),
            (
                4,
                None,
                false,
                &exchange,
                "05 09:00-05 14:00, 06 09:00-06 14:00, 09 08:00-09 13:00, 10 08:00-10 13:00",
            ),
            (
                5,
                None,
                false,
                &exchange,
                "05 00:00-05 00:30, 05 21:00-06 00:30, 06 21:00-07 00:30, 09 20:00-09 23:30, \
                 10 20:00-10 23:30",
            ),
            (
                6,
                None,
                false,
                &exchange,
                "05 00:00-05 00:30, 05 09:00-05 14:00, 05 14:30-05 20:30, 05 21:00-06 00:30, \
                 06 09:00-06 14:00, 06 14:30-06 20:30, 06 21:00-07 00:30, 09 08:00-09 13:00, \
                 09 13:30-09 19:30, 09 20:00-09 23:30, 10 08:00-10 13:00, 10 13:30-10 19:30, \
                 10 20:00-10 23:30",
            ),
            // Without extended hours the trading day stays.
            (
                6,
                None,
                false,
                &regular_only,
                "05 00:00-05 04:30, 05 05:00-06 04:30, 06 05:00-07 04:30, 07 05:00-08 04:30, \
                 08 05:00-09 03:30, 09 04:00-10 03:30, 10 04:00-10 23:30",
            ),
        ] {
            let study = calculated(sessions, custom, hours_only, instrument);
            assert_eq!(spans(&study), expected, "sessions {sessions} {hours_only}");
        }
        // A zone no build knows is refused and changes nothing.
        let mut study = calculated(3, Some((570, 960, "+05:30")), false, &exchange);
        assert!(
            study
                .set_input("session_zone", InputValue::Text("Mars/Base".into()))
                .is_err()
        );
        assert_eq!(starts(&study), plus_0530);
        study.calculate(&bars);
        assert_eq!(starts(&study), plus_0530);
        // Daily bars carry a trading date rather than hours, so each keeps its own profile.
        let days: Vec<_> = (0..5)
            .map(|day| {
                let time = start + TimeDelta::days(day - 3);
                Bar::new(time, 100.0, 101.0, 99.0, 100.0, 1.0)
            })
            .collect();
        let mut daily = VolProfileSession::new(24);
        daily.set_timeframe(Timeframe::Day1);
        daily.set_instrument(Some(&exchange));
        daily.set_input("sessions", InputValue::Choice(3)).unwrap();
        daily.calculate(&days);
        let ranges: Vec<_> = daily
            .volume_profiles(0..5)
            .into_iter()
            .map(|profile| profile.bars)
            .collect();
        assert_eq!(ranges, [0..1, 1..2, 2..3, 3..4, 4..5]);
    }

    /// Weeks and longer group on a calendar fixed in time, minutes and hours as the chart's bar
    /// calendar opens bars of that length, and trading days and bars from the first bar of the
    /// newest profile, so loading older bars leaves every period where it was. A new study
    /// counts trading days and bars from its first bar. Restarted each year in the exchange's time
    /// zone, weeks and longer group from the period holding 1 January, and trading days and bars
    /// from each year's first bar, the oldest year's as before.
    #[test]
    fn periodic_counts_keep_their_period_starts_when_older_bars_load() {
        let at = |time: &str| time.parse::<DateTime<Utc>>().unwrap();
        let bar = |time| Bar::new(time, 100.0, 101.0, 99.0, 100.0, 1.0);
        let year: Vec<_> = (0..7)
            .map(|day| bar(at("2025-12-29T00:00:00Z") + TimeDelta::days(day)))
            .collect();
        for (restart, expected) in [
            (None, vec![0..3, 3..7]),
            (Some(false), std::iter::once(0..7).collect()),
        ] {
            let mut study = VolProfilePeriodic::default();
            study.set_timeframe(Timeframe::Day1);
            if let Some(restart) = restart {
                study
                    .set_input("restart_yearly", InputValue::Toggle(restart))
                    .unwrap();
            }
            study.calculate(&year);
            assert_eq!(
                study
                    .volume_profiles(0..7)
                    .into_iter()
                    .map(|profile| profile.bars)
                    .collect::<Vec<_>>(),
                expected
            );
        }
        // Weekdays from a Monday, at 15:00 UTC.
        let weekdays = |monday: &str, count| -> Vec<_> {
            let monday = at(&format!("{monday}T15:00:00Z"));
            (0..count)
                .map(|day| bar(monday + TimeDelta::days(day / 5 * 7 + day % 5)))
                .collect()
        };
        let hours: Vec<_> = (0..16)
            .map(|hour| bar(at("2026-03-05T02:00:00Z") + TimeDelta::hours(hour)))
            .collect();
        let minutes = |start| -> Vec<_> {
            (0..250)
                .map(|minute| bar(at(start) + TimeDelta::minutes(minute)))
                .collect()
        };
        let west = BarClock::new(Timeframe::Hour1)
            .with_timezone(TimezoneId::fixed_offset(-5 * 3600).unwrap());
        let exchange =
            Instrument::new("X").with_time_zone(TimezoneId::fixed_offset(-5 * 3600).unwrap());
        let periodic = |period,
                        count,
                        restart,
                        timeframe,
                        clock: Option<&BarClock>,
                        instrument: Option<&Instrument>| {
            let mut study = VolProfilePeriodic::new(24);
            study.set_timeframe(timeframe);
            if let Some(clock) = clock {
                study.set_bar_clock(clock);
            }
            study.set_instrument(instrument);
            study
                .set_input("period", InputValue::Choice(period))
                .unwrap();
            study
                .set_input("period_count", InputValue::Length(count))
                .unwrap();
            study
                .set_input("restart_yearly", InputValue::Toggle(restart))
                .unwrap();
            study
        };
        for (
            period,
            count,
            restart,
            bars,
            timeframe,
            clock,
            instrument,
            k,
            suffix,
            prepended,
            fresh,
        ) in [
            // Weeks 2922 to 2927 since Monday 5 January 1970 pair as 1461, 1462 and 1463.
            (
                1,
                2,
                false,
                weekdays("2026-01-05", 30),
                Timeframe::Day1,
                None,
                None,
                10,
                vec![10..20, 20..30],
                vec![0..10, 10..20, 20..30],
                vec![0..10, 10..20, 20..30],
            ),
            // Four hours from UTC midnight open at 04:00, 08:00, 12:00 and 16:00.
            (
                9,
                4,
                false,
                hours.clone(),
                Timeframe::Hour1,
                None,
                None,
                4,
                vec![4..6, 6..10, 10..14, 14..16],
                vec![0..2, 2..6, 6..10, 10..14, 14..16],
                vec![0..2, 2..6, 6..10, 10..14, 14..16],
            ),
            // From midnight at UTC-5 they open at 05:00, 09:00, 13:00 and 17:00 UTC.
            (
                9,
                4,
                false,
                hours,
                Timeframe::Hour1,
                Some(west),
                None,
                4,
                vec![4..7, 7..11, 11..15, 15..16],
                vec![0..3, 3..7, 7..11, 11..15, 15..16],
                vec![0..3, 3..7, 7..11, 11..15, 15..16],
            ),
            // The newest of the first profiles starts at bar 230, so older bars group back
            // from there.
            (
                7,
                100,
                false,
                minutes("2026-03-05T00:00:00Z"),
                Timeframe::Min1,
                None,
                None,
                130,
                vec![130..230, 230..250],
                vec![0..30, 30..130, 130..230, 230..250],
                vec![0..100, 100..200, 200..250],
            ),
            // From Thursday 8 January, the newest pair of trading days starts on Friday 16.
            (
                0,
                2,
                false,
                weekdays("2026-01-05", 10),
                Timeframe::Day1,
                None,
                None,
                3,
                vec![3..5, 5..7, 7..9, 9..10],
                vec![0..1, 1..3, 3..5, 5..7, 7..9, 9..10],
                vec![0..2, 2..4, 4..6, 6..8, 8..10],
            ),
            // 1 January 2025 lies in week 2869, so 15 to 26 December are weeks 50 and 51 of
            // 2025 and 29 to 31 December week 52; 1 January 2026 lies in week 2921, which
            // starts 2026 on its 1 January.
            (
                1,
                2,
                true,
                weekdays("2025-12-15", 20),
                Timeframe::Day1,
                None,
                None,
                7,
                vec![7..10, 10..13, 13..20],
                vec![0..10, 10..13, 13..20],
                vec![0..10, 10..13, 13..20],
            ),
            // At the exchange, five hours west of UTC, bars 0 to 119 are the last two hours of
            // 2025, whose profiles keep their starts as without a restart when older bars load;
            // 2026 counts from its own first bar.
            (
                7,
                100,
                true,
                minutes("2026-01-01T03:00:00Z"),
                Timeframe::Min1,
                None,
                Some(&exchange),
                30,
                vec![30..120, 120..220, 220..250],
                vec![0..30, 30..120, 120..220, 220..250],
                vec![0..100, 100..120, 120..220, 220..250],
            ),
        ] {
            let profiles = |study: &VolProfilePeriodic, shift| -> Vec<_> {
                study
                    .volume_profiles(0..bars.len())
                    .into_iter()
                    .map(|profile| profile.bars.start + shift..profile.bars.end + shift)
                    .collect()
            };
            let case = format!("period {period} count {count} restart {restart}");
            let mut study = periodic(
                period,
                count,
                restart,
                timeframe,
                clock.as_ref(),
                instrument,
            );
            study.calculate(&bars[k..]);
            assert_eq!(profiles(&study, k), suffix, "{case}");
            // The chart empties a study before it recomputes it with the older bars.
            study.update(&[], 0);
            study.update(&bars, 0);
            assert_eq!(profiles(&study, 0), prepended, "{case}");
            let mut new = periodic(
                period,
                count,
                restart,
                timeframe,
                clock.as_ref(),
                instrument,
            );
            new.calculate(&bars);
            assert_eq!(profiles(&new, 0), fresh, "{case}");
        }
    }

    /// Prefix volumes set the developing levels, restarting at the session, view or anchor.
    #[test]
    fn developing_levels_restart_with_each_profile() {
        let start = "2026-03-05T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let mut mismatches = Vec::new();
        for (case, prices, expected) in [
            (
                "expanding rows",
                [
                    (2.0, 4.0, 1.0, 3.0, 150.0),
                    (8.0, 8.0, 8.0, 8.0, 200.0),
                    (12.0, 13.0, 11.0, 12.0, 100.0),
                ],
                8.25,
            ),
            (
                "equal densities",
                [
                    (0.0, 3.0, 0.0, 3.0, 100.0),
                    (3.0, 18.0, 3.0, 18.0, 500.0),
                    (0.0, 24.0, 0.0, 18.0, 0.0),
                ],
                3.5,
            ),
        ] {
            let mut source: Vec<_> = prices
                .into_iter()
                .enumerate()
                .map(|(i, (open, high, low, close, volume))| {
                    Bar::new(
                        start + TimeDelta::hours(i as i64),
                        open,
                        high,
                        low,
                        close,
                        volume,
                    )
                })
                .collect();
            let mut study = VolProfileAnchored::default();
            study.set_timeframe(Timeframe::Hour1);
            study.calculate(&source);
            let histogram = study.volume_profiles(0..source.len()).remove(0).profile;
            let (poc, (low, high)) = histogram.poc_and_value_area(0.7).unwrap();
            assert_eq!(
                histogram.low() + (poc as f64 + 0.5) * histogram.row_height(),
                expected,
                "{case}"
            );
            for (line, row) in [poc as f64 + 0.5, high as f64 + 1.0, low as f64]
                .into_iter()
                .enumerate()
            {
                let expected = Some(histogram.low() + row * histogram.row_height());
                let actual = study.values().value(line, source.len() - 1);
                if actual != expected {
                    mismatches.push(format!(
                        "{case} line {line}: {actual:?}, expected {expected:?}"
                    ));
                }
            }
            // Repeated ticks and appends must retain the same earlier values as a full calculation.
            for append in [false, true, true] {
                if append {
                    let mut last = source.last().unwrap().clone();
                    last.time += TimeDelta::hours(1);
                    source.push(last);
                }
                let from = source.len() - 1;
                source[from].volume += 10.0;
                study.update(&source, from);
                let mut fresh = VolProfileAnchored::default();
                fresh.set_timeframe(Timeframe::Hour1);
                fresh.calculate(&source);
                for line in 0..3 {
                    for i in 0..source.len() {
                        assert_eq!(
                            study.values().value(line, i),
                            fresh.values().value(line, i),
                            "{case} line {line} bar {i}"
                        );
                    }
                }
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
        let bars: Vec<_> = [
            (0, 10.0, 100.0),
            (1, 12.0, 300.0),
            (2, 11.0, 200.0),
            (24, 20.0, 50.0),
        ]
        .into_iter()
        .map(|(hour, price, volume)| {
            Bar::new(
                start + TimeDelta::hours(hour),
                price,
                price,
                price,
                price,
                volume,
            )
        })
        .collect();
        let instrument = Instrument::new("X").with_tick_size(1.0);
        let session = [
            Some([10.5, 11.0, 10.0]),
            Some([12.5, 13.0, 12.0]),
            Some([12.5, 13.0, 11.0]),
            Some([20.5, 21.0, 20.0]),
        ];
        let mut auto = VolProfileAutoAnchored::default();
        auto.set_input("anchor", InputValue::Choice(4)).unwrap();
        let mut periodic = VolProfilePeriodic::default();
        periodic.set_input("period", InputValue::Choice(0)).unwrap();
        type Case = (Box<dyn Indicator>, Range<usize>, [Option<[f64; 3]>; 4]);
        let cases: Vec<Case> = vec![
            (Box::new(periodic), 0..4, session),
            (Box::new(VolProfileSession::default()), 0..4, session),
            (Box::new(auto), 0..4, [None, None, None, session[3]]),
            (
                Box::new(VolProfileVisible::default()),
                1..3,
                [None, session[1], session[2], None],
            ),
            (
                Box::new(VolProfileFixed::default()),
                0..4,
                [session[0], session[1], session[2], Some([12.5, 13.0, 11.0])],
            ),
            (
                Box::new(VolProfileAnchored::default()),
                0..4,
                [session[0], session[1], session[2], Some([12.5, 13.0, 11.0])],
            ),
        ];
        for (mut study, view, expected) in cases {
            study.set_instrument(Some(&instrument));
            study.set_timeframe(Timeframe::Hour1);
            study.set_visible_range(view);
            study.calculate(&bars);
            assert_eq!(
                study.line_names(),
                ["Developing POC", "Developing VA High", "Developing VA Low"]
            );
            for line in 0..3 {
                let style = study.plot_style(line);
                assert_eq!(style.plot_type, crate::studies::PlotType::StepLine);
                assert!(!style.visible);
                for (i, values) in expected.iter().enumerate() {
                    assert_eq!(
                        study.values().value(line, i),
                        values.map(|values| values[line]),
                        "{} {line} bar {i}",
                        study.name()
                    );
                }
            }
            if study.name() == "Vol Profile VR" {
                assert!(!study.set_visible_range(0..3));
                for (i, values) in session.iter().enumerate() {
                    for line in 0..3 {
                        assert_eq!(
                            study.values().value(line, i),
                            if i == 3 {
                                None
                            } else {
                                values.map(|v| v[line])
                            }
                        );
                    }
                }
            }
        }
    }

    /// Height in ticks sizes the rows of every profile kind in whole ticks of the instrument's
    /// tick size, in place of Height, which holds at 0 ticks and without a tick size.
    #[test]
    fn session_rows_follow_whole_ticks_of_the_instrument() {
        let at = "2026-03-05T15:00:00Z".parse().unwrap();
        let session = || Box::new(VolProfileSession::new(24)) as Box<dyn Indicator>;
        for (mut study, tick, row_ticks, height, expected, rows, whole, count) in [
            (session(), Some(0.25), 4, 0.0, Some(1.0), 24, true, None),
            (session(), Some(0.01), 4, 0.0, Some(0.04), 24, true, None),
            (session(), Some(1e-8), 4, 0.0, Some(4e-8), 24, true, None),
            (session(), Some(1e6), 4, 0.0, Some(4e6), 24, true, None),
            (session(), Some(0.25), 0, 0.5, Some(0.5), 24, true, None),
            (session(), None, 4, 0.5, Some(0.5), 24, true, None),
            (session(), None, 4, 0.0, None, 24, true, None),
            (
                Box::new(VolProfilePeriodic::new(24)) as Box<dyn Indicator>,
                Some(0.25),
                4,
                0.0,
                Some(1.0),
                24,
                true,
                None,
            ),
            (
                Box::new(VolProfileVisible::new(24)) as Box<dyn Indicator>,
                Some(0.25),
                4,
                0.0,
                Some(1.0),
                24,
                true,
                None,
            ),
            (
                Box::new(VolProfileFixed::new(24)) as Box<dyn Indicator>,
                Some(0.25),
                4,
                0.0,
                Some(1.0),
                24,
                true,
                None,
            ),
            (
                Box::new(VolProfileAnchored::new(24)) as Box<dyn Indicator>,
                Some(0.25),
                4,
                0.0,
                Some(1.0),
                24,
                true,
                None,
            ),
            (
                Box::new(VolProfileAutoAnchored::new(24)) as Box<dyn Indicator>,
                Some(0.25),
                4,
                0.0,
                Some(1.0),
                24,
                true,
                None,
            ),
            (
                Box::new(VolProfileVisible::new(30)) as Box<dyn Indicator>,
                Some(0.01),
                0,
                0.0,
                Some(3.0 * 0.01),
                30,
                true,
                Some(34),
            ),
            (
                Box::new(VolProfileFixed::new(30)) as Box<dyn Indicator>,
                Some(0.01),
                0,
                0.0,
                Some(3.0 * 0.01),
                30,
                true,
                Some(34),
            ),
            (
                Box::new(VolProfileSession::new(30)) as Box<dyn Indicator>,
                Some(0.01),
                0,
                0.0,
                Some(3.0 * 0.01),
                30,
                true,
                Some(34),
            ),
            (
                Box::new(VolProfilePeriodic::new(30)) as Box<dyn Indicator>,
                Some(0.01),
                0,
                0.0,
                Some(3.0 * 0.01),
                30,
                true,
                Some(34),
            ),
            (
                Box::new(VolProfileAnchored::new(30)) as Box<dyn Indicator>,
                Some(0.01),
                0,
                0.0,
                Some(3.0 * 0.01),
                30,
                true,
                Some(34),
            ),
            (
                Box::new(VolProfileAutoAnchored::new(30)) as Box<dyn Indicator>,
                Some(0.01),
                0,
                0.0,
                Some(3.0 * 0.01),
                30,
                true,
                Some(34),
            ),
            // A high on the row edge opens an empty top row, as an explicit height does.
            (
                session(),
                Some(0.01),
                0,
                0.0,
                Some(0.04),
                25,
                true,
                Some(26),
            ),
            (session(), Some(0.01), 0, 0.0, None, 30, false, Some(30)),
            (session(), None, 0, 0.0, None, 30, true, Some(30)),
        ] {
            study
                .set_input("row_count", InputValue::Length(rows))
                .unwrap();
            study
                .set_input("whole_tick_rows", InputValue::Toggle(whole))
                .unwrap();
            let mut instrument = Instrument::new("X");
            instrument.tick_size = tick;
            study.set_instrument(Some(&instrument));
            study
                .set_input("row_ticks", InputValue::Length(row_ticks))
                .unwrap();
            study
                .set_input("row_size", InputValue::Number(height))
                .unwrap();
            let (low, high) = (1000.0 * tick.unwrap_or(0.01), 1100.0 * tick.unwrap_or(0.01));
            let bars = [Bar::new(at, low, high, low, high, 1.0)];
            study.calculate(&bars);
            let profiles = study.volume_profiles(0..1);
            if let Some(count) = count {
                assert_eq!(
                    profiles[0].profile.volume().len(),
                    count,
                    "{}",
                    study.name()
                );
            }
            assert_eq!(
                profiles[0].config.row_size(),
                expected,
                "{} tick {tick:?} ticks {row_ticks} height {height}",
                study.name()
            );
            if tick == Some(0.01) && row_ticks == 0 && height == 0.0 && whole && rows == 30 {
                instrument.tick_size = Some(0.02);
                assert!(
                    study.set_instrument(Some(&instrument)),
                    "tick changes refresh the study"
                );
                study.update(&bars, 0);
                assert_eq!(study.volume_profiles(0..1)[0].config.row_size(), Some(0.04));
            }
        }
    }

    /// A session profile finds its sessions again when the chart's instrument arrives after the
    /// bars, without new bars: hourly bars from 20:00 UTC on 5 March to 03:00 on 6 March span
    /// two UTC days and one day at UTC-5.
    #[cfg(feature = "widget")]
    #[test]
    fn session_profiles_follow_an_instrument_given_after_the_bars() {
        use crate::model::BarData;
        use crate::widget::{Chart, test_frames::shapes};
        let first: DateTime<Utc> = "2026-03-05T20:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..8)
            .map(|hour| Bar::new(first + TimeDelta::hours(hour), 10.0, 11.0, 9.0, 10.0, 100.0))
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.add_indicator(Box::new(VolProfileSession::new(24)));
        let ctx = egui::Context::default();
        let ranges = |chart: &Chart| -> Vec<Range<usize>> {
            let study = chart.indicators().next().unwrap().1;
            study
                .volume_profiles(0..8)
                .into_iter()
                .map(|profile| profile.bars)
                .collect()
        };
        shapes(&ctx, &mut chart, 1.0, Vec::new());
        assert_eq!(ranges(&chart), [0..4, 4..8]);
        let zone = TimezoneId::fixed_offset(-5 * 3600).unwrap();
        chart.set_instrument(Some(Instrument::new("X").with_time_zone(zone)));
        shapes(&ctx, &mut chart, 2.0, Vec::new());
        assert_eq!(ranges(&chart), std::iter::once(0..8).collect::<Vec<_>>());
    }

    /// The Earnings, Dividends and Splits anchors start the auto anchored profile at the first
    /// bar at or after the latest chart event of their kind that a bar has reached, and draw
    /// nothing without one. Daily bars open on 2 to 6 March. Of the dividends on 3 March and at
    /// noon on 4 March the later one starts the profile on 5 March, the earnings on 6 March start
    /// it on 6 March. A dividend on 6 March moves it there, one on 9 March waits for its bar, and
    /// without events the profile goes.
    #[cfg(feature = "widget")]
    #[test]
    fn auto_anchored_profiles_start_at_the_latest_chart_event() {
        use crate::model::{BarData, EventKind, TimeScaleEvent};
        use crate::widget::{Chart, test_frames::shapes};
        let at = |day: i64, hour: i64| {
            "2026-03-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap()
                + TimeDelta::days(day - 1)
                + TimeDelta::hours(hour)
        };
        let bars: Vec<_> = (2..=6)
            .map(|day| Bar::new(at(day, 0), 10.0, 11.0, 9.0, 10.0, 100.0))
            .collect();
        let event = |kind: EventKind, time: DateTime<Utc>| {
            TimeScaleEvent::new(format!("{kind:?} {time}"), time, kind, "")
        };
        let first = vec![
            event(EventKind::Dividend, at(3, 0)),
            event(EventKind::Dividend, at(4, 12)),
            event(EventKind::Earnings, at(6, 0)),
        ];
        let later = [first.clone(), vec![event(EventKind::Dividend, at(6, 0))]].concat();
        let future = [later.clone(), vec![event(EventKind::Dividend, at(9, 0))]].concat();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let id = chart.add_indicator(Box::new(VolProfileAutoAnchored::new(24)));
        let ctx = egui::Context::default();
        let span = |start| std::iter::once(start..5).collect::<Vec<_>>();
        // The Anchor choice when it changes, the chart's events and the profiles' bars. Without a
        // new choice only the events change, which must find the profile again by themselves.
        for (now, (anchor, events, expected)) in (1..).zip([
            (Some(13), first.clone(), vec![]),
            (Some(11), first.clone(), span(4)),
            (Some(12), first, span(3)),
            (None, later, span(4)),
            (None, future, span(4)),
            (None, Vec::new(), vec![]),
            // An event before loaded history starts at its first bar; other kinds do not move it.
            (
                Some(13),
                vec![
                    event(EventKind::Split, at(1, 0)),
                    event(EventKind::News, at(6, 0)),
                ],
                span(0),
            ),
        ]) {
            chart.set_time_scale_events(events);
            if let Some(anchor) = anchor {
                chart
                    .indicator_mut(id)
                    .unwrap()
                    .set_input("anchor", InputValue::Choice(anchor))
                    .unwrap();
            }
            shapes(&ctx, &mut chart, f64::from(now), Vec::new());
            let study = chart.indicators().next().unwrap().1;
            let got: Vec<_> = study
                .volume_profiles(0..5)
                .into_iter()
                .map(|profile| profile.bars)
                .collect();
            assert_eq!(got, expected, "anchor {anchor:?} at {now}");
        }
    }
}

#[cfg(all(test, feature = "widget"))]
mod rendering_tests {
    use super::*;
    use crate::chart::rendering::{ChartMapping, IndicatorRenderer, RenderContext};
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::{model::Bar, studies::Indicator};

    #[test]
    fn frozen_profiles_keep_deferred_rows_levels_and_autoscale() {
        use crate::chart::rendering::fit_study_mapping;
        use crate::studies::ConfiguredStudy;
        let bars = super::tests::level_bars();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 400.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(90.0, 110.0),
            0.0,
            400.0,
            false,
        );
        let coords = ChartMapping::new(rect, 100.0, 0, 1, 0.0, price);
        let profiles = |study: &dyn Indicator| {
            study
                .volume_profiles(0..7)
                .into_iter()
                .map(|plot| {
                    (
                        plot.profile.source(),
                        plot.profile.low(),
                        plot.profile.high(),
                        plot.profile.volume().to_vec(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let levels = |study: &dyn Indicator| {
            study
                .profile_levels(0..7)
                .into_iter()
                .map(|level| (level.kind, level.price, level.bars))
                .collect::<Vec<_>>()
        };
        for prepared in [false, true] {
            if prepared && !cfg!(feature = "widget") {
                continue;
            }
            for first_read in 0..3 {
                let mut study = ConfiguredStudy::new(Box::new(VolProfileSession::new(24)));
                study.set_timeframe(Timeframe::Hour1);
                study
                    .set_input("row_size", InputValue::Number(2.0))
                    .unwrap();
                study
                    .set_input("extend_poc", InputValue::Toggle(true))
                    .unwrap();
                let input = study.order_flow_inputs()[0].clone();
                input.set_tick_size(Some(1.0));
                let clock = BarClock::new(Timeframe::Hour1);
                if prepared {
                    #[cfg(feature = "widget")]
                    {
                        let mut rows = crate::data::Footprint::new(clock.clone(), 1.0, 1).unwrap();
                        for bar in &bars {
                            rows.push(&crate::model::Tick::new(
                                bar.time, bar.close, bar.volume, None,
                            ))
                            .unwrap();
                        }
                        let trades = crate::data::trades::Trades::new(1);
                        let mut history = crate::data::FootprintState::new(rows);
                        history.prepare_history(
                            bars.last().unwrap().time + chrono::TimeDelta::hours(1),
                            &trades,
                            true,
                        );
                        history.share_prepared();
                        input.replace_trades(&clock, &trades, &bars, Some(&history));
                    }
                } else {
                    for bar in &bars {
                        input.push_trade(
                            &clock,
                            &crate::model::Tick::new(bar.time, bar.close, bar.volume, None),
                        );
                    }
                }
                study.calculate(&bars);
                let expected_profiles = profiles(&study);
                let expected_levels = levels(&study);
                assert!(!expected_levels.is_empty());
                let expected_scale = fit_study_mapping(&study, &coords, None).price.range();
                let mut frozen = study.clone_for_export();
                frozen.calculate(&bars);
                input.clear();
                study.calculate(&bars);
                assert_ne!(profiles(&study), expected_profiles);
                match first_read {
                    0 => {
                        assert_eq!(profiles(frozen.as_ref()), expected_profiles);
                    }
                    1 => {
                        assert_eq!(levels(frozen.as_ref()), expected_levels);
                    }
                    _ => {
                        assert_eq!(
                            fit_study_mapping(frozen.as_ref(), &coords, None)
                                .price
                                .range(),
                            expected_scale
                        );
                    }
                }
                assert_eq!(profiles(frozen.as_ref()), expected_profiles);
                assert_eq!(levels(frozen.as_ref()), expected_levels);
                assert_eq!(
                    fit_study_mapping(frozen.as_ref(), &coords, None)
                        .price
                        .range(),
                    expected_scale
                );
            }
        }
    }

    /// Hidden value area boundaries leave clicks to the plot beneath, while rows and the point
    /// of control still select the study.
    #[test]
    fn hidden_value_area_lines_do_not_select() {
        let time = |minute| chrono::DateTime::UNIX_EPOCH + TimeDelta::minutes(minute);
        let bars = [
            Bar::new(time(0), 10.0, 12.0, 10.0, 12.0, 100.0),
            Bar::new(time(1), 20.0, 20.0, 10.0, 10.0, 100.0),
            Bar::new(time(2), 12.0, 20.0, 12.0, 20.0, 100.0),
        ];
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(9.0, 21.0),
            0.0,
            200.0,
            false,
        );
        let coords = ChartMapping::new(rect, 200.0, 0, 1, 0.0, price);
        // Rows 0 and 1 hold 60, the rest 22.5. VA is 10..16; POC is 10.5.
        for (inputs, right, x, value, expected) in [
            (vec![], false, 390.0, 10.0, false),
            (vec![], false, 390.0, 16.0, false),
            (vec![("show_vah", true)], false, 390.0, 16.0, true),
            (vec![("show_vah", true)], false, 390.0, 10.0, false),
            (vec![("show_val", true)], false, 390.0, 10.0, true),
            (vec![("show_val", true)], false, 390.0, 16.0, false),
            (vec![], false, 390.0, 10.5, true),
            (vec![("show_poc", false)], false, 390.0, 10.5, false),
            (vec![], false, 20.0, 12.5, true),
            (vec![("show_rows", false)], false, 20.0, 12.5, false),
            (vec![], true, 390.0, 15.5, true),
            (vec![], true, 10.0, 15.5, false),
        ] {
            let mut study = VolProfileSession::new(10);
            study
                .set_input("placement", InputValue::Choice(usize::from(right)))
                .unwrap();
            for (id, value) in inputs {
                study.set_input(id, InputValue::Toggle(value)).unwrap();
            }
            study.calculate(&bars);
            let hit = crate::chart::indicators::hit_test_indicator(
                egui::pos2(x, price.price_to_y(value)),
                &study,
                0,
                0..3,
                &coords,
                |value| price.price_to_y(value),
            );
            assert_eq!(hit.is_some(), expected, "selection at ({x}, {value})");
        }
    }

    /// Extensions use the profile's colour in either theme and stop at the touching bar's
    /// centre, starting at the first later bar's left edge.
    #[test]
    fn extended_levels_paint_hairlines_in_the_profile_colour_of_either_theme() {
        let bars = super::tests::level_bars();
        let light = crate::theme::Theme::light();
        let dark = crate::theme::Theme::dark();
        let saved = Color32::from_rgb(41, 98, 255);
        for (theme, color, plot_width) in [
            (&light, None, 400.0),
            (&dark, None, 400.0),
            (&light, Some(saved), 400.0),
            (&dark, Some(saved), 400.0),
            (&light, None, 120.0),
            (&dark, None, 120.0),
        ] {
            let mut study = VolProfileSession::new(24);
            study
                .set_input("row_size", InputValue::Number(2.0))
                .unwrap();
            for id in ["extend_poc", "extend_vah", "extend_val"] {
                study.set_input(id, InputValue::Toggle(true)).unwrap();
            }
            if let Some(color) = color {
                study
                    .set_input("poc_color", InputValue::Color(color))
                    .unwrap();
            }
            study.calculate(&bars);
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(plot_width, 200.0));
            let price = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(96.0, 106.0),
                0.0,
                200.0,
                false,
            );
            let coords = ChartMapping::new(rect, 50.0, 0, 6, 0.0, price);
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, theme);
            let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                IndicatorRenderer::render(
                    &RenderContext::new(ui.painter(), rect),
                    &indicators,
                    &bars,
                    &price,
                    &coords,
                );
            });
            let lines: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::LineSegment { points, stroke } => {
                        assert_eq!(shape.clip_rect, rect);
                        Some((
                            points[0].x,
                            points[1].x,
                            points[0].y,
                            stroke.width,
                            stroke.color,
                        ))
                    }
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            let color = color.unwrap_or(theme.ui.text);
            let [r, g, b, _] = color.to_srgba_unmultiplied();
            let va = Color32::from_rgba_unmultiplied(r, g, b, 200);
            let expected: Vec<_> = [
                (199.0, 324.0, 100.0, 1.0, color),
                (199.0, 224.0, 40.0, 1.0, va),
                (199.0, 374.0, 160.0, 1.0, va),
                (299.0, 374.0, 60.0, 1.0, color),
                (299.0, 374.0, 40.0, 1.0, va),
                (299.0, 374.0, 80.0, 1.0, va),
            ]
            .into_iter()
            .filter(|(_, right, _, _, _)| right + plot_width - 400.0 >= 0.0)
            .collect();
            assert_eq!(lines.len(), expected.len());
            for ((x0, x1, y, width, color), (left, right, price_y, stroke, ink)) in
                lines.into_iter().zip(expected)
            {
                assert!(
                    (x0 - (left + plot_width - 400.0)).abs() < 1e-3
                        && (x1 - (right + plot_width - 400.0)).abs() < 1e-3
                        && (y - price_y).abs() < 1e-3
                );
                assert_eq!((width, color), (stroke, ink));
            }
        }
    }

    /// Painted rows determine the profile's selection and independent price bounds.
    #[test]
    fn visible_profile_paints_price_rows() {
        let bars = [Bar::new(
            chrono::DateTime::UNIX_EPOCH,
            10.0,
            12.0,
            10.0,
            11.0,
            20.0,
        )];
        let mut study = VolProfileVisible::new(2);
        study.calculate(&bars);
        let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 200.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(9.0, 13.0),
            0.0,
            200.0,
            false,
        );
        let coords = ChartMapping::new(rect, 40.0, 0, 0, 0.0, price);
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            IndicatorRenderer::render(
                &RenderContext::new(ui.painter(), rect),
                &indicators,
                &bars,
                &price,
                &coords,
            );
        });
        let rows = output
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape,
            egui::Shape::Rect(r) if (r.rect.height() - 49.0).abs() < 0.01)
            })
            .count();
        // The rows are estimated from the bar, as noted at the top left of the
        // right-aligned box: x from 280 (400 less 30%) plus 3, y of 12 = 50.
        let notes: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == "Estimated from bars" => Some(t.pos),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        assert_eq!(notes, [egui::pos2(283.0, 50.0)]);
        assert!(rows >= 2, "profile rows {rows}");
        // Equal row volumes put the POC at y=125 and fill the strip from x=280; the line crosses
        // the plot, past the rows.
        for (x, y, expected) in [
            (395.0, 125.0, true),
            (285.0, 75.0, true),
            (5.0, 125.0, true),
            (5.0, 75.0, false),
        ] {
            let hit = crate::chart::indicators::hit_test_indicator(
                egui::pos2(x, y),
                indicators[0].as_ref(),
                0,
                0..1,
                &coords,
                |value| price.price_to_y(value),
            );
            assert_eq!(hit.is_some(), expected, "profile selection at ({x}, {y})");
        }

        // The first bar is outside the painted viewport and must not stretch its scale.
        let mut independent =
            crate::studies::ConfiguredStudy::new(Box::new(VolProfileVisible::new(2)));
        independent.set_scale(crate::studies::StudyScale::Independent);
        independent.calculate(&[
            Bar {
                high: 1000.0,
                ..bars[0].clone()
            },
            bars[0].clone(),
            bars[0].clone(),
        ]);
        let coords = ChartMapping {
            base_idx: 10,
            ..coords
        };
        let mapping = crate::chart::rendering::study_mapping(&independent, &coords);
        assert!((mapping.price_to_y(12.0) - 200.0 / 12.0).abs() < 0.01);
        assert!((mapping.price_to_y(10.0) - 200.0 * 11.0 / 12.0).abs() < 0.01);
        assert!(
            crate::chart::indicators::hit_test_indicator(
                egui::pos2(395.0, mapping.price_to_y(10.5)),
                &independent,
                0,
                0..3,
                &mapping,
                |value| mapping.price_to_y(value),
            )
            .is_some()
        );
    }

    /// A new session profile paints cyan and pink rows, stronger in the value area, under a 2 px
    /// point of control in the foreground colour of either theme. Saved colours keep theirs. Its
    /// widest row takes 30% of a session that fills the plot, or the share its width input sets,
    /// as do fixed range, anchored and auto anchored profiles of the same bars.
    #[test]
    fn session_profile_paints_its_default_and_saved_colours() {
        let time = |minute| chrono::DateTime::UNIX_EPOCH + TimeDelta::minutes(minute);
        // Rows 10 and 11 hold most of the volume, up and down; rows 12 to 19 hold the rest of each.
        let bars = [
            Bar::new(time(0), 10.0, 12.0, 10.0, 12.0, 100.0),
            Bar::new(time(1), 20.0, 20.0, 10.0, 10.0, 100.0),
            Bar::new(time(2), 12.0, 20.0, 12.0, 20.0, 100.0),
        ];
        let saved = vec![
            Color32::from_rgb(41, 98, 255),
            Color32::from_rgba_unmultiplied(51, 74, 212, 89),
            Color32::from_rgba_unmultiplied(214, 140, 35, 89),
            Color32::from_rgb(51, 74, 212),
            Color32::from_rgb(214, 140, 35),
        ];
        let defaults = [
            [0, 188, 212, 110],
            [233, 30, 99, 110],
            [0, 188, 212, 163],
            [233, 30, 99, 163],
        ];
        let stored = [
            [51, 74, 212, 89],
            [214, 140, 35, 89],
            [51, 74, 212, 163],
            [214, 140, 35, 163],
        ];
        let light = crate::theme::Theme::light();
        let dark = crate::theme::Theme::dark();
        // Study, theme, saved colours, the rows outside and inside the value area, the line
        // colour, the width input and where the widest row ends: each profile spans the plot,
        // 0 to 400.
        let session = || Box::new(VolProfileSession::new(10)) as Box<dyn Indicator>;
        let cases = [
            (
                session(),
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                120.0,
            ),
            (session(), &dark, None, defaults, dark.ui.text, None, 120.0),
            (
                session(),
                &light,
                Some(saved.clone()),
                stored,
                saved[0],
                None,
                120.0,
            ),
            (
                session(),
                &light,
                None,
                defaults,
                light.ui.text,
                Some(90.0),
                360.0,
            ),
            (
                Box::new(VolProfileAutoAnchored::new(10)) as Box<dyn Indicator>,
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                120.0,
            ),
            (
                Box::new(VolProfileAnchored::new(10)) as Box<dyn Indicator>,
                &dark,
                None,
                defaults,
                dark.ui.text,
                Some(50.0),
                200.0,
            ),
            (
                Box::new(VolProfileFixed::new(10)) as Box<dyn Indicator>,
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                120.0,
            ),
        ]
        .into_iter()
        .map(|(study, theme, colors, rows, poc, width, right_edge)| {
            (
                study,
                theme,
                colors,
                rows,
                poc,
                width,
                right_edge,
                vec![],
                true,
                1,
                false,
            )
        })
        .chain([
            (
                session(),
                &light,
                None,
                stored,
                saved[0],
                None,
                120.0,
                [
                    "poc_color",
                    "up_color",
                    "down_color",
                    "va_up_color",
                    "va_down_color",
                ]
                .into_iter()
                .zip(&saved)
                .map(|(id, color)| (id, InputValue::Color(*color)))
                .collect(),
                true,
                1,
                false,
            ),
            (
                session(),
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                120.0,
                vec![("show_rows", InputValue::Toggle(false))],
                false,
                1,
                false,
            ),
            (
                session(),
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                120.0,
                vec![("show_poc", InputValue::Toggle(false))],
                true,
                0,
                false,
            ),
            (
                session(),
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                120.0,
                vec![
                    ("show_vah", InputValue::Toggle(true)),
                    ("vah_color", InputValue::Color(Color32::BLUE)),
                ],
                true,
                1,
                true,
            ),
        ])
        .map(|(a, b, c, d, e, f, g, h, i, j, k)| {
            (a, b, c, d, e, f, g, h, i, j, k, 0.0, None::<Vec<String>>)
        })
        .chain([
            (
                Box::new(VolProfileVisible::new(10)) as Box<dyn Indicator>,
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                400.0,
                vec![],
                true,
                1,
                false,
                280.0,
                None,
            ),
            (
                Box::new(VolProfileVisible::new(10)) as Box<dyn Indicator>,
                &light,
                None,
                defaults,
                light.ui.text,
                Some(50.0),
                400.0,
                vec![],
                true,
                1,
                false,
                200.0,
                None,
            ),
            (
                Box::new(VolProfileVisible::new(10)) as Box<dyn Indicator>,
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                120.0,
                vec![("placement", InputValue::Choice(0))],
                true,
                1,
                false,
                0.0,
                None,
            ),
            (
                session(),
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                400.0,
                vec![("placement", InputValue::Choice(1))],
                true,
                1,
                false,
                280.0,
                None,
            ),
            (
                session(),
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                120.0,
                vec![
                    ("show_values", InputValue::Toggle(true)),
                    ("display", InputValue::Choice(0)),
                ],
                true,
                1,
                false,
                0.0,
                Some([vec!["60.00".to_owned(); 2], vec!["22.50".to_owned(); 8]].concat()),
            ),
            (
                session(),
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                120.0,
                vec![("show_values", InputValue::Toggle(true))],
                true,
                1,
                false,
                0.0,
                Some(
                    [
                        vec!["50.00 / 10.00".to_owned(); 2],
                        vec!["12.50 / 10.00".to_owned(); 8],
                    ]
                    .concat(),
                ),
            ),
            (
                session(),
                &light,
                None,
                defaults,
                light.ui.text,
                None,
                80.0,
                vec![
                    ("show_values", InputValue::Toggle(true)),
                    ("display", InputValue::Choice(2)),
                ],
                true,
                1,
                false,
                0.0,
                Some([vec!["+40.00".to_owned(); 2], vec!["+2.50".to_owned(); 8]].concat()),
            ),
        ]);

        for (
            mut study,
            theme,
            colors,
            rows,
            poc,
            width,
            right_edge,
            inputs,
            rows_drawn,
            line_count,
            vah,
            left_edge,
            texts,
        ) in cases
        {
            for (id, value) in inputs {
                study.set_input(id, value).unwrap();
            }

            if let Some(colors) = colors {
                study.set_colors(colors);
            }
            if let Some(width) = width {
                study
                    .set_input("width", InputValue::Number(width))
                    .expect("width");
            }
            let display = study
                .inputs()
                .into_iter()
                .find(|input| input.id == "display")
                .unwrap()
                .value;
            study.calculate(&bars);
            let indicators = vec![study];
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 200.0));
            let price = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(9.0, 21.0),
                0.0,
                200.0,
                false,
            );
            // A view inside the session: its first bar starts at the plot's left edge and its last
            // ends past the right.
            let coords = ChartMapping::new(rect, 200.0, 0, 1, 0.0, price);
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, theme);
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                IndicatorRenderer::render(
                    &RenderContext::new(ui.painter(), rect),
                    &indicators,
                    &bars,
                    &price,
                    &coords,
                );
            });
            let fills: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(row) => Some(row.fill),
                    _ => None,
                })
                .collect();
            let leftmost = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(row) => Some(row.rect.left()),
                    _ => None,
                })
                .fold(f32::MAX, f32::min);
            if rows_drawn {
                assert!(
                    (leftmost - left_edge).abs() < 1e-3,
                    "{leftmost} to {left_edge}"
                );
            }
            if let Some(mut expected) = texts {
                let mut got: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(t) if t.galley.text() != "Estimated from bars" => {
                            Some(t.galley.text().to_owned())
                        }
                        _ => None,
                    })
                    .collect();
                got.sort();
                expected.sort();
                assert_eq!(got, expected);
            }
            let widest = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(row) => Some(row.rect.right()),
                    _ => None,
                })
                .fold(f32::MIN, f32::max);
            let lines: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Path(line) => Some(line.stroke.clone()),
                    _ => None,
                })
                .collect();
            let hairlines: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::LineSegment { points, stroke }
                        if stroke.color == Color32::BLUE =>
                    {
                        Some(*points)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(hairlines.len(), usize::from(vah));
            if vah {
                assert!((hairlines[0][0].y - price.price_to_y(16.0)).abs() < 1e-3);
            }
            let captions: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if matches!(text.galley.text(), "VAH" | "VAL") => {
                        Some(text.galley.text())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(captions, if vah { vec!["VAH"] } else { vec![] });
            output.drop_without_applying_deltas();
            assert!(
                !rows_drawn || (widest - right_edge).abs() < 1e-3,
                "{widest} to {right_edge}"
            );
            assert_eq!(!fills.is_empty(), rows_drawn);
            for (_, [r, g, b, a]) in rows.into_iter().enumerate().filter(|(side, _)| {
                rows_drawn && (display == InputValue::Choice(1) || side % 2 == 0)
            }) {
                // Colours pass through the study's stored and painted forms as Color32 does.
                let [r, g, b, a] =
                    Color32::from_rgba_unmultiplied(r, g, b, a).to_srgba_unmultiplied();
                let row = Color32::from_rgba_unmultiplied(r, g, b, a);
                assert!(fills.contains(&row), "{row:?} among {fills:?}");
            }
            assert_eq!(lines.len(), line_count);
            for line in lines {
                assert_eq!(line.width, 2.0);
                assert_eq!(line.color, egui::epaint::ColorMode::Solid(poc));
            }
        }
    }

    /// A box that starts at the pane's left edge, under the chart legend, keeps its source note
    /// at the right, short of the value area labels.
    #[test]
    fn profile_from_the_left_edge_notes_its_source_clear_of_the_legend() {
        let bars: Vec<_> = (0..10)
            .map(|i| {
                let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(i);
                Bar::new(time, 10.0, 12.0, 10.0, 11.0, 20.0)
            })
            .collect();
        let mut study = VolProfileFixed::new(2);
        study.calculate(&bars);
        let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 200.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(9.0, 13.0),
            0.0,
            200.0,
            false,
        );
        let coords = ChartMapping {
            base_idx: 9,
            ..ChartMapping::new(rect, 40.0, 0, 0, 0.0, price)
        };
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            IndicatorRenderer::render(
                &RenderContext::new(ui.painter(), rect),
                &indicators,
                &bars,
                &price,
                &coords,
            );
        });
        let note = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == "Estimated from bars" => {
                Some((t.pos.x + t.galley.size().x, t.pos.y))
            }
            _ => None,
        });
        output.drop_without_applying_deltas();
        // Last bar's right edge at 399 (400 less half a 40-point bar, 1, plus half a bar), first
        // bar's left edge clamped to 0; the note ends 50 short of 399, at the y of 12 = 50.
        let (right, top) = note.expect("source note");
        assert!((right - 349.0).abs() < 0.01, "note ends at {right}");
        assert_eq!(top, 50.0);
    }

    /// A finer source interval is requested only when it is shorter than the chart's; its bars
    /// replace the estimate for the chart bars they fall in, which keep the estimate where there
    /// are none, so the total volume stays and the coverage is the share of bars with finer
    /// bars. A reply for another interval is dropped.
    #[test]
    fn finer_bars_replace_the_estimate_where_they_exist() {
        use crate::model::BarClock;
        let at = |minutes: i64| {
            "2026-03-02T09:00:00Z"
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap()
                + chrono::Duration::minutes(minutes)
        };
        // Two hourly bars over 100 to 110, and half-hour bars for the first only: 70 at the
        // bottom row and 30 at the top row.
        let chart = [
            Bar::new(at(0), 100.0, 110.0, 100.0, 105.0, 100.0),
            Bar::new(at(60), 100.0, 110.0, 100.0, 105.0, 100.0),
        ];
        let fine = [
            Bar::new(at(0), 100.0, 100.9, 100.0, 100.5, 70.0),
            Bar::new(at(30), 109.5, 110.0, 109.1, 109.5, 30.0),
        ];
        let source = |bars: &[Bar], timeframe, hinted| {
            let mut study = VolProfileFixed::new(10);
            study.set_timeframe(Timeframe::Hour1);
            if hinted {
                study.set_instrument(Some(
                    &Instrument::new("SYNTH")
                        .with_profile_resolutions([ProfileResolution::new(timeframe, None)]),
                ));
            } else {
                study
                    .set_input("source_interval", InputValue::Timeframe(Some(timeframe)))
                    .unwrap();
            }
            study.calculate(&chart);
            study.update_data(
                &chart,
                0,
                &[StudyData {
                    request: StudyDataRequest {
                        symbol: String::new(),
                        timeframe: Some(timeframe),
                        metric: None,
                    },
                    bars,
                    clock: BarClock::new(timeframe),
                    instrument: None,
                    timeframe,
                    chart: None,
                }],
            );
            study
        };
        let expected = [80.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 40.0];
        for (interval, hinted) in [
            (Timeframe::Hour1, false),
            (Timeframe::Hour1, true),
            (Timeframe::Custom(std::num::NonZeroU64::MAX), false),
        ] {
            let mut study = source(&fine, Timeframe::Min30, hinted);
            study.set_timeframe(interval);
            assert_eq!(study.data_requests()[0].timeframe, Some(Timeframe::Min30));
            let volume = study.volume_profiles(0..2)[0].profile.volume().to_vec();
            assert!(
                volume
                    .iter()
                    .zip(expected)
                    .all(|(v, e)| (v - e).abs() < 1e-9),
                "{volume:?}"
            );
            assert_eq!(
                study.profile_inputs(0..2),
                [ProfileInput {
                    bars: 0..2,
                    coverage: 0.5
                }]
            );
        }

        // Losing the finer source changes the whole estimate, even on a tail update.
        let mut tail = source(&fine, Timeframe::Min30, false);
        tail.update_data(&chart, 1, &[]);
        let fresh = source(&[], Timeframe::Min30, false);
        for line in 0..3 {
            for i in 0..chart.len() {
                assert_eq!(tail.values().value(line, i), fresh.values().value(line, i));
            }
        }

        // A longer source interval is not requested, and another interval's reply is dropped.
        let mut longer = VolProfileFixed::new(10);
        longer.set_timeframe(Timeframe::Hour1);
        longer
            .set_input(
                "source_interval",
                InputValue::Timeframe(Some(Timeframe::Day1)),
            )
            .unwrap();
        assert!(longer.data_requests().is_empty());
        let mut other = source(&fine, Timeframe::Min30, false);
        other
            .set_input(
                "source_interval",
                InputValue::Timeframe(Some(Timeframe::Min15)),
            )
            .unwrap();
        other.calculate(&chart);
        other.update_data(&chart, 0, &[]);
        assert_eq!(other.profile_inputs(0..2)[0].coverage, 0.0);
    }
}
