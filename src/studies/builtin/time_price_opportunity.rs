//! Market Profile letters of each session drawn beside the session's bars.

use crate::model::{
    Bar, CustomProfileSession, CustomProfileSessions, Instrument, LineStyle, ProfileSession,
    SessionOverlap, TPOColorMode, TPOConfig, TPODisplayMode, TPOProfilePeriod, TimezoneId,
};
use crate::studies::{Indicator, IndicatorValues, InputKind, InputValue, StudyInput};
use chrono::{DateTime, NaiveTime, SecondsFormat, Utc};
use egui::Color32;
use std::ops::RangeInclusive;

/// Display choices of the study, in input order. Saved choices are indices, so new ones go last.
const DISPLAYS: [TPODisplayMode; 3] = [
    TPODisplayMode::Letters,
    TPODisplayMode::Blocks,
    TPODisplayMode::Both,
];

/// Index of `mode` among [`DISPLAYS`].
fn display_choice(mode: TPODisplayMode) -> InputValue {
    InputValue::Choice(DISPLAYS.iter().position(|d| *d == mode).unwrap())
}

/// Profile period choices of the study, in input order.
const PERIODS: [TPOProfilePeriod; 3] = [
    TPOProfilePeriod::Session,
    TPOProfilePeriod::Week,
    TPOProfilePeriod::Month,
];

/// Styles of the volume profile's level lines, in input order.
const LINE_STYLES: [LineStyle; 3] = [LineStyle::Solid, LineStyle::Dashed, LineStyle::Dotted];

/// Index of `style` among [`LINE_STYLES`].
fn line_style_choice(style: LineStyle) -> InputValue {
    InputValue::Choice(LINE_STYLES.iter().position(|s| *s == style).unwrap_or(0))
}

/// A colour input that reads [`Color32::PLACEHOLDER`] while the colour follows the theme or
/// the profile.
fn color_input(color: Option<Color32>) -> InputValue {
    InputValue::Color(color.unwrap_or(Color32::PLACEHOLDER))
}

/// The colour a colour input picked, or `None` for [`Color32::PLACEHOLDER`].
fn picked(color: Color32) -> Option<Color32> {
    (color != Color32::PLACEHOLDER).then_some(color)
}

/// A merge or split list as its input holds it, after the period length and row size it was
/// made for. The profiles take it only while the period length still matches: a merge joins
/// whole sessions and a split falls at a period's start, whatever the rows. `None` is a list
/// saved before the two were recorded, which applies whatever they are.
type ProfileEdits<T> = (Option<(usize, usize)>, Vec<T>);

/// Each session's TPO profile on the price pane, starting at the session's first bar, with its
/// point of control, value area and initial balance.
///
/// The chart builds the profiles of the whole sessions it shows from its bars and calendar with
/// [`to_tpo_profiles`](crate::model::to_tpo_profiles), through the same code and cache as the
/// TPO chart type, so on the unsplit trading day the study's profile of a session equals the chart
/// type's under the same settings. [`Indicator::tpo`] exposes those settings; the study has no
/// plotted values.
///
/// Its Hours input keeps each profile to the trading day, the regular hours, custom hours, or the
/// pre-market or post-market alone, counting only the bars of those hours, and custom hours keep
/// whole periods on the period grid. Split extended hours divides the trading day into its
/// pre-market, regular and post-market profiles. Profiles longer than one day keep the trading
/// day, whatever Hours says.
#[derive(Clone, Debug)]
pub struct TimePriceOpportunity {
    row_ticks: usize,
    instrument_tick: Option<f64>,
    hours: usize,
    custom: [CustomProfileSession; CustomProfileSessions::MAX],
    custom_count: usize,
    overlap: SessionOverlap,
    clip_hours: bool,
    composites: ProfileEdits<RangeInclusive<DateTime<Utc>>>,
    splits: ProfileEdits<DateTime<Utc>>,
    config: TPOConfig,
    values: IndicatorValues,
    visible: bool,
}

impl TimePriceOpportunity {
    fn custom_sessions(&self) -> CustomProfileSessions {
        CustomProfileSessions {
            sessions: self.custom[..self.custom_count].to_vec(),
            overlap: self.overlap,
            trading_hours_only: self.clip_hours,
        }
    }

    /// Profiles of `period_minutes` periods, raised to the bar interval when shorter.
    pub fn new(period_minutes: u32) -> Self {
        Self {
            row_ticks: 0,
            instrument_tick: None,
            hours: 0,
            custom: std::array::from_fn(|_| {
                CustomProfileSession::new(NaiveTime::MIN, NaiveTime::MIN)
            }),
            custom_count: 1,
            overlap: SessionOverlap::OwnProfile,
            clip_hours: false,
            composites: (None, Vec::new()),
            splits: (None, Vec::new()),
            config: TPOConfig {
                // A zero row lets the chart fit the rows to the visible range.
                tick_size: 0.0,
                period_minutes: period_minutes.max(1),
                ..Default::default()
            },
            values: IndicatorValues::new(),
            visible: true,
        }
    }

    /// Input values that store the profile menu's merges and splits. With `made_for`, the
    /// period length and row size the edits were made for, each list that holds any is saved
    /// after them as `<minutes>,<ticks per row>|<list>`.
    pub(crate) fn profile_edit_inputs(
        made_for: Option<(usize, usize)>,
        composites: &[RangeInclusive<DateTime<Utc>>],
        splits: &[DateTime<Utc>],
    ) -> [(String, InputValue); 2] {
        let instant = |time: &DateTime<Utc>| time.to_rfc3339_opts(SecondsFormat::AutoSi, true);
        let text = |list: Vec<String>| {
            let list = list.join(";");
            match made_for {
                Some((minutes, ticks)) if !list.is_empty() => {
                    InputValue::Text(format!("{minutes},{ticks}|{list}"))
                }
                _ => InputValue::Text(list),
            }
        };
        [
            (
                "composites".into(),
                text(
                    composites
                        .iter()
                        .map(|r| format!("{}/{}", instant(r.start()), instant(r.end())))
                        .collect(),
                ),
            ),
            ("splits".into(), text(splits.iter().map(instant).collect())),
        ]
    }

    /// Ranges of a `composites` list, or `None` when one is not `<start>/<end>` in order.
    fn parse_composites(text: &str) -> Option<Vec<RangeInclusive<DateTime<Utc>>>> {
        if text.is_empty() {
            return Some(Vec::new());
        }
        text.split(';')
            .map(|range| {
                let (start, end) = range.split_once('/')?;
                let start = DateTime::parse_from_rfc3339(start)
                    .ok()?
                    .with_timezone(&Utc);
                let end = DateTime::parse_from_rfc3339(end).ok()?.with_timezone(&Utc);
                (start <= end).then_some(start..=end)
            })
            .collect()
    }

    /// Instants of a `splits` list, or `None` when one does not parse.
    fn parse_splits(text: &str) -> Option<Vec<DateTime<Utc>>> {
        if text.is_empty() {
            return Some(Vec::new());
        }
        text.split(';')
            .map(|time| {
                DateTime::parse_from_rfc3339(time)
                    .ok()
                    .map(|time| time.with_timezone(&Utc))
            })
            .collect()
    }

    /// Hands the profiles the edits made for the current period length.
    fn update_edits(&mut self) {
        let now = self.config.period_minutes as usize;
        let live = |made_for: Option<(usize, usize)>| made_for.is_none_or(|made| made.0 == now);
        self.config.composites = if live(self.composites.0) {
            self.composites.1.clone()
        } else {
            Vec::new()
        };
        self.config.splits = if live(self.splits.0) {
            self.splits.1.clone()
        } else {
            Vec::new()
        };
    }

    fn update_tick_size(&mut self) {
        self.config.tick_size = self
            .instrument_tick
            .map_or(0.0, |tick| tick * self.row_ticks as f64);
        self.config.instrument_tick = self.instrument_tick;
    }

    /// The bars the profiles keep: those of the Hours choice, or each segment of the trading day
    /// when Split extended hours divides it.
    fn update_session(&mut self) {
        self.config.session = match self.hours {
            _ if self.config.joins_sessions() => ProfileSession::Chart,
            0 if self.config.split_sessions => ProfileSession::Segments,
            1 => ProfileSession::Regular,
            2 => ProfileSession::Custom(self.custom_sessions()),
            3 => ProfileSession::PreMarket,
            4 => ProfileSession::PostMarket,
            _ => ProfileSession::Chart,
        };
    }
}

impl TimePriceOpportunity {
    /// The volume profile's inputs, shown while it is drawn. Saved choices are indices, so new
    /// ones go last.
    fn volume_inputs(&self) -> Vec<StudyInput> {
        let volume = &self.config.volume_profile;
        let toggle = |id: &str, label: &str, on: bool, parent: &str| {
            StudyInput::new(id, label, InputValue::Toggle(on), InputKind::Toggle)
                .with_default(InputValue::Toggle(false))
                .shown_when(parent, [1])
        };
        let color = |id: &str, label: &str, color: Option<Color32>, parent: &str| {
            StudyInput::new(id, label, color_input(color), InputKind::Color)
                .with_default(InputValue::Color(Color32::PLACEHOLDER))
                .shown_when(parent, [1])
                .in_style_tab()
        };
        let style = |id: &str, label: &str, style: LineStyle, parent: &str| {
            StudyInput::new(
                id,
                label,
                line_style_choice(style),
                InputKind::Choice(vec!["Solid".into(), "Dashed".into(), "Dotted".into()]),
            )
            .with_default(InputValue::Choice(0))
            .shown_when(parent, [1])
            .in_style_tab()
        };
        let mut inputs = Vec::new();
        for (id, label, on, line_color, line_style) in [
            (
                "volume_poc",
                "Volume point of control",
                volume.poc,
                volume.poc_color,
                volume.poc_style,
            ),
            (
                "volume_value_area_high",
                "Volume value area high",
                volume.value_area_high,
                volume.value_area_high_color,
                volume.value_area_high_style,
            ),
            (
                "volume_value_area_low",
                "Volume value area low",
                volume.value_area_low,
                volume.value_area_low_color,
                volume.value_area_low_style,
            ),
        ] {
            inputs.extend([
                toggle(id, label, on, "volume_profile"),
                color(
                    &format!("{id}_color"),
                    &format!("{label} colour"),
                    line_color,
                    id,
                ),
                style(
                    &format!("{id}_style"),
                    &format!("{label} style"),
                    line_style,
                    id,
                ),
            ]);
        }
        inputs.extend([
            toggle(
                "volume_values",
                "Volume values",
                volume.values,
                "volume_profile",
            ),
            color(
                "volume_values_color",
                "Volume values colour",
                volume.values_color,
                "volume_values",
            ),
            color(
                "volume_profile_color",
                "Volume colour",
                volume.color,
                "volume_profile",
            ),
            color(
                "volume_value_area_color",
                "Volume value area colour",
                volume.value_area_color,
                "volume_profile",
            ),
            StudyInput::new(
                "volume_placement",
                "Volume placement",
                InputValue::Choice(usize::from(!volume.left)),
                InputKind::Choice(vec!["Left".into(), "Right".into()]),
            )
            .with_default(InputValue::Choice(1))
            .shown_when("volume_profile", [1]),
            // Saved studies from before these inputs shade rows by the TPO value area.
            StudyInput::new(
                "volume_tpo_value_area",
                "Volume rows by TPO value area",
                InputValue::Toggle(volume.tpo_value_area_rows),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(false))
            .shown_when("display", Vec::new()),
        ]);
        inputs
    }

    /// Sets one of [`Self::volume_inputs`], or returns `false` for another id or value.
    fn set_volume_input(&mut self, id: &str, value: InputValue) -> bool {
        let volume = &mut self.config.volume_profile;
        match (id, value) {
            ("volume_poc", InputValue::Toggle(on)) => volume.poc = on,
            ("volume_poc_color", InputValue::Color(color)) => volume.poc_color = picked(color),
            ("volume_poc_style", InputValue::Choice(choice @ 0..=2)) => {
                volume.poc_style = LINE_STYLES[choice]
            }
            ("volume_value_area_high", InputValue::Toggle(on)) => volume.value_area_high = on,
            ("volume_value_area_high_color", InputValue::Color(color)) => {
                volume.value_area_high_color = picked(color)
            }
            ("volume_value_area_high_style", InputValue::Choice(choice @ 0..=2)) => {
                volume.value_area_high_style = LINE_STYLES[choice]
            }
            ("volume_value_area_low", InputValue::Toggle(on)) => volume.value_area_low = on,
            ("volume_value_area_low_color", InputValue::Color(color)) => {
                volume.value_area_low_color = picked(color)
            }
            ("volume_value_area_low_style", InputValue::Choice(choice @ 0..=2)) => {
                volume.value_area_low_style = LINE_STYLES[choice]
            }
            ("volume_values", InputValue::Toggle(on)) => volume.values = on,
            ("volume_values_color", InputValue::Color(color)) => {
                volume.values_color = picked(color)
            }
            ("volume_profile_color", InputValue::Color(color)) => volume.color = picked(color),
            ("volume_value_area_color", InputValue::Color(color)) => {
                volume.value_area_color = picked(color)
            }
            ("volume_placement", InputValue::Choice(choice @ 0..=1)) => volume.left = choice == 0,
            ("volume_tpo_value_area", InputValue::Toggle(on)) => volume.tpo_value_area_rows = on,
            _ => return false,
        }
        true
    }
}

impl Default for TimePriceOpportunity {
    fn default() -> Self {
        Self::new(30)
    }
}

impl Indicator for TimePriceOpportunity {
    fn name(&self) -> &str {
        "Time Price Opportunity"
    }
    fn desc(&self) -> &str {
        "Market Profile of each session beside its bars"
    }
    fn inputs(&self) -> Vec<StudyInput> {
        let defaults = TPOConfig::default();
        let zone = self.custom[0].zone.as_ref().map_or("", TimezoneId::name);
        let mut inputs = vec![
            StudyInput::new(
                "period",
                "Period (minutes)",
                InputValue::Length(self.config.period_minutes as usize),
                InputKind::Length(1, 1440),
            )
            .with_default(InputValue::Length(30)),
            StudyInput::new(
                "profile_period",
                "Profile period",
                InputValue::Choice(
                    PERIODS
                        .iter()
                        .position(|period| *period == self.config.profile_period)
                        .unwrap_or(0),
                ),
                InputKind::Choice(vec!["Days".into(), "Weeks".into(), "Months".into()]),
            )
            .with_default(InputValue::Choice(0)),
            StudyInput::new(
                "profile_periods",
                "Profile length",
                InputValue::Length(self.config.profile_periods as usize),
                InputKind::Length(1, 100),
            )
            .with_default(InputValue::Length(1)),
            StudyInput::new(
                "row_ticks",
                "Ticks per row (0 for auto)",
                InputValue::Length(self.row_ticks),
                InputKind::Length(0, 100_000),
            )
            .with_default(InputValue::Length(0)),
            StudyInput::new(
                "value_area",
                "Value area (%)",
                InputValue::Number(self.config.value_area_pct * 100.0),
                InputKind::Number(1.0, 100.0),
            )
            .with_default(InputValue::Number(defaults.value_area_pct * 100.0)),
            StudyInput::new(
                "hours",
                "Hours",
                InputValue::Choice(self.hours),
                InputKind::Choice(vec![
                    "Trading day".into(),
                    "Regular hours".into(),
                    "Custom hours".into(),
                    "Pre-market only".into(),
                    "Post-market only".into(),
                ]),
            )
            .with_default(InputValue::Choice(0)),
            StudyInput::new(
                "session_start",
                "Session start",
                InputValue::TimeOfDay(self.custom[0].start),
                InputKind::TimeOfDay,
            )
            .with_default(InputValue::TimeOfDay(NaiveTime::MIN))
            .shown_when("hours", [2]),
            StudyInput::new(
                "session_end",
                "Session end",
                InputValue::TimeOfDay(self.custom[0].end),
                InputKind::TimeOfDay,
            )
            .with_default(InputValue::TimeOfDay(NaiveTime::MIN))
            .shown_when("hours", [2]),
            StudyInput::new(
                "session_zone",
                "Session time zone",
                InputValue::TimeZone(zone.into()),
                InputKind::TimeZone { exchange: true },
            )
            .with_default(InputValue::TimeZone(String::new()))
            .shown_when("hours", [2]),
            StudyInput::new(
                "split_sessions",
                "Split extended hours",
                InputValue::Toggle(self.config.split_sessions),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.split_sessions))
            .shown_when("hours", [0]),
            StudyInput::new(
                "single_prints",
                "Single prints",
                InputValue::Toggle(self.config.show_single_prints),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_single_prints)),
            StudyInput::new(
                "midpoint",
                "TPO Midpoint",
                InputValue::Toggle(self.config.show_midpoint),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_midpoint)),
            StudyInput::new(
                "statistics",
                "Statistics",
                InputValue::Toggle(self.config.show_statistics),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_statistics)),
            StudyInput::new(
                "display",
                "Display",
                display_choice(self.config.display_mode),
                InputKind::Choice(vec![
                    "Letters".into(),
                    "Blocks".into(),
                    "Letters and blocks".into(),
                ]),
            )
            .with_default(display_choice(defaults.display_mode)),
            StudyInput::new(
                "letters_color",
                "Letter colour",
                color_input(self.config.letter_color),
                InputKind::Color,
            )
            .with_default(InputValue::Color(Color32::PLACEHOLDER))
            .shown_when("display", [0, 2])
            .in_style_tab(),
            StudyInput::new(
                "sessions",
                "Sessions",
                InputValue::Choice(usize::from(self.config.latest_session_only)),
                InputKind::Choice(vec!["Visible".into(), "Latest".into()]),
            )
            .with_default(InputValue::Choice(usize::from(
                defaults.latest_session_only,
            ))),
            StudyInput::new(
                "split_periods",
                "Split display",
                InputValue::Toggle(self.config.split_periods),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.split_periods)),
            StudyInput::new(
                "coloring",
                "Block colours",
                InputValue::Choice(usize::from(self.config.color_mode == TPOColorMode::Solid)),
                InputKind::Choice(vec!["Period colours".into(), "Single colour".into()]),
            )
            .with_default(InputValue::Choice(0)),
            StudyInput::new(
                "period_color_1",
                "Period colour 1",
                InputValue::Color(self.config.period_colors[0]),
                InputKind::Color,
            )
            .with_default(InputValue::Color(defaults.period_colors[0]))
            .shown_when("coloring", vec![0]),
            StudyInput::new(
                "period_color_2",
                "Period colour 2",
                InputValue::Color(self.config.period_colors[1]),
                InputKind::Color,
            )
            .with_default(InputValue::Color(defaults.period_colors[1]))
            .shown_when("coloring", vec![0]),
            StudyInput::new(
                "period_color_3",
                "Period colour 3",
                InputValue::Color(self.config.period_colors[2]),
                InputKind::Color,
            )
            .with_default(InputValue::Color(defaults.period_colors[2]))
            .shown_when("coloring", vec![0]),
            StudyInput::new(
                "period_color_4",
                "Period colour 4",
                InputValue::Color(self.config.period_colors[3]),
                InputKind::Color,
            )
            .with_default(InputValue::Color(defaults.period_colors[3]))
            .shown_when("coloring", vec![0]),
            StudyInput::new(
                "block_color",
                "Block colour",
                InputValue::Color(self.config.solid_color),
                InputKind::Color,
            )
            .with_default(InputValue::Color(defaults.solid_color))
            .shown_when("coloring", vec![1]),
            StudyInput::new(
                "outside_value_area_opacity",
                "Opacity outside value area (%)",
                InputValue::Length(
                    (self.config.outside_value_area_opacity * 100.0).round() as usize
                ),
                InputKind::Length(0, 100),
            )
            .with_default(InputValue::Length(45)),
            StudyInput::new(
                "poc",
                "Point of control",
                InputValue::Toggle(self.config.show_poc),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_poc)),
            StudyInput::new(
                "poc_color",
                "Point of control colour",
                color_input(self.config.poc_color),
                InputKind::Color,
            )
            .with_default(InputValue::Color(Color32::PLACEHOLDER))
            .shown_when("poc", [1])
            .in_style_tab(),
            StudyInput::new(
                "value_area_levels",
                "Value area",
                InputValue::Toggle(self.config.show_value_area),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_value_area)),
            StudyInput::new(
                "value_area_high",
                "Value area high",
                InputValue::Toggle(self.config.show_value_area_high),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_value_area_high))
            .shown_when("value_area_levels", [1]),
            StudyInput::new(
                "value_area_low",
                "Value area low",
                InputValue::Toggle(self.config.show_value_area_low),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_value_area_low))
            .shown_when("value_area_levels", [1]),
            StudyInput::new(
                "initial_balance",
                "Initial balance",
                InputValue::Toggle(self.config.show_initial_balance),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_initial_balance)),
            StudyInput::new(
                "initial_balance_minutes",
                "Initial balance (minutes)",
                InputValue::Length(self.config.initial_balance_minutes as usize),
                InputKind::Length(1, 1440),
            )
            .with_default(InputValue::Length(60)),
            StudyInput::new(
                "initial_balance_extensions",
                "Initial balance extensions",
                InputValue::Toggle(self.config.show_initial_balance_extensions),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_initial_balance_extensions)),
            StudyInput::new(
                "open_price",
                "Open price",
                InputValue::Toggle(self.config.show_opening_range),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_opening_range)),
            StudyInput::new(
                "close_price",
                "Closing price",
                InputValue::Toggle(self.config.show_closing_price),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_closing_price)),
            StudyInput::new(
                "poor_high",
                "Poor high",
                InputValue::Toggle(self.config.show_poor_high),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_poor_high)),
            StudyInput::new(
                "poor_low",
                "Poor low",
                InputValue::Toggle(self.config.show_poor_low),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_poor_low)),
            StudyInput::new(
                "extend_poor",
                "Extend poor highs and lows until repaired",
                InputValue::Toggle(self.config.extend_poor_extremes),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.extend_poor_extremes)),
            StudyInput::new(
                "volume_profile",
                "Volume profile",
                InputValue::Toggle(self.config.show_volume_profile),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_volume_profile)),
        ];
        inputs.extend(self.volume_inputs());
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
            .shown_when("hours", [2]),
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
                    InputValue::TimeZone(session.zone.as_ref().map_or("", TimezoneId::name).into()),
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
            .shown_when("hours", [2]),
            StudyInput::new(
                "clip_hours",
                "Clip to trading hours",
                InputValue::Toggle(self.clip_hours),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(false))
            .shown_when("hours", [2]),
        ]);
        let position = inputs
            .iter()
            .position(|input| input.id == "session_zone")
            .unwrap()
            + 1;
        inputs.splice(position..position, custom_inputs);
        let (mut rows, mut default_rows) = (self.config.statistics_rows, defaults.statistics_rows);
        let statistics_rows = rows
            .switches()
            .into_iter()
            .zip(default_rows.switches())
            .map(|((name, label, shown), (_, _, default))| {
                StudyInput::new(
                    format!("statistics_{name}"),
                    label,
                    InputValue::Toggle(*shown),
                    InputKind::Toggle,
                )
                .with_default(InputValue::Toggle(*default))
                .shown_when("statistics", [1])
            });
        let position = inputs
            .iter()
            .position(|input| input.id == "statistics")
            .unwrap()
            + 1;
        inputs.splice(position..position, statistics_rows);
        let made_for = (self.config.period_minutes as usize, self.row_ticks);
        for (id, value) in
            Self::profile_edit_inputs(Some(made_for), &self.config.composites, &self.config.splits)
        {
            let label = if id == "composites" {
                "Merged profiles"
            } else {
                "Profile splits"
            };
            inputs.push(
                StudyInput::new(id, label, value, InputKind::Text)
                    .with_default(InputValue::Text(String::new()))
                    .shown_when("display", Vec::new()),
            );
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

            ("period", InputValue::Length(minutes @ 1..=1440)) => {
                self.config.period_minutes = minutes as u32
            }
            ("row_ticks", InputValue::Length(ticks @ 0..=100_000)) => {
                self.row_ticks = ticks;
                self.update_tick_size();
            }
            ("value_area", InputValue::Number(percent)) if (1.0..=100.0).contains(&percent) => {
                self.config.value_area_pct = percent / 100.0
            }
            ("hours", InputValue::Choice(choice @ 0..=4)) => self.hours = choice,
            ("custom_sessions", InputValue::Choice(n @ 0..=4)) => self.custom_count = n + 1,
            ("session_overlap", InputValue::Choice(n @ 0..=1)) => {
                self.overlap = if n == 0 {
                    SessionOverlap::OwnProfile
                } else {
                    SessionOverlap::InBoth
                }
            }
            ("clip_hours", InputValue::Toggle(clip)) => self.clip_hours = clip,
            ("split_sessions", InputValue::Toggle(split)) => self.config.split_sessions = split,
            ("single_prints", InputValue::Toggle(show)) => self.config.show_single_prints = show,
            ("midpoint", InputValue::Toggle(show)) => self.config.show_midpoint = show,
            ("statistics", InputValue::Toggle(show)) => self.config.show_statistics = show,
            (row, InputValue::Toggle(show)) if row.starts_with("statistics_") => {
                let name = &row["statistics_".len()..];
                for (id, _, shown) in self.config.statistics_rows.switches() {
                    if id == name {
                        *shown = show;
                    }
                }
            }
            ("display", InputValue::Choice(choice @ 0..=2)) => {
                self.config.display_mode = DISPLAYS[choice]
            }
            ("sessions", InputValue::Choice(choice @ 0..=1)) => {
                self.config.latest_session_only = choice == 1
            }
            ("split_periods", InputValue::Toggle(split)) => self.config.split_periods = split,
            ("coloring", InputValue::Choice(choice @ 0..=1)) => {
                self.config.color_mode = if choice == 0 {
                    TPOColorMode::ByPeriod
                } else {
                    TPOColorMode::Solid
                }
            }
            ("block_color", InputValue::Color(color)) => self.config.solid_color = color,
            ("outside_value_area_opacity", InputValue::Length(percent @ 0..=100)) => {
                self.config.outside_value_area_opacity = percent as f32 / 100.0
            }
            ("initial_balance_minutes", InputValue::Length(minutes @ 1..=1440)) => {
                self.config.initial_balance_minutes = minutes as u32
            }
            ("period_color_1", InputValue::Color(color)) => self.config.period_colors[0] = color,
            ("period_color_2", InputValue::Color(color)) => self.config.period_colors[1] = color,
            ("period_color_3", InputValue::Color(color)) => self.config.period_colors[2] = color,
            ("period_color_4", InputValue::Color(color)) => self.config.period_colors[3] = color,
            ("poc", InputValue::Toggle(show)) => self.config.show_poc = show,
            ("poc_color", InputValue::Color(color)) => self.config.poc_color = picked(color),
            ("value_area_levels", InputValue::Toggle(show)) => self.config.show_value_area = show,
            ("value_area_high", InputValue::Toggle(show)) => {
                self.config.show_value_area_high = show
            }
            ("value_area_low", InputValue::Toggle(show)) => self.config.show_value_area_low = show,
            ("profile_period", InputValue::Choice(choice @ 0..=2)) => {
                self.config.profile_period = PERIODS[choice]
            }
            ("profile_periods", InputValue::Length(count @ 1..=100)) => {
                self.config.profile_periods = count as u32
            }
            ("letters_color", InputValue::Color(color)) => self.config.letter_color = picked(color),
            ("initial_balance", InputValue::Toggle(show)) => {
                self.config.show_initial_balance = show
            }
            ("initial_balance_extensions", InputValue::Toggle(show)) => {
                self.config.show_initial_balance_extensions = show
            }
            ("open_price", InputValue::Toggle(show)) => self.config.show_opening_range = show,
            ("close_price", InputValue::Toggle(show)) => self.config.show_closing_price = show,
            ("poor_high", InputValue::Toggle(show)) => self.config.show_poor_high = show,
            ("poor_low", InputValue::Toggle(show)) => self.config.show_poor_low = show,
            ("extend_poor", InputValue::Toggle(show)) => self.config.extend_poor_extremes = show,
            ("volume_profile", InputValue::Toggle(show)) => self.config.show_volume_profile = show,
            (volume, value) if volume.starts_with("volume_") => {
                if !self.set_volume_input(volume, value) {
                    return Err(crate::studies::StudyError::rejected(&self.inputs(), id));
                }
            }
            ("composites" | "splits", InputValue::Text(text)) => {
                let rejected = || crate::studies::StudyError::rejected(&self.inputs(), id);
                let (made_for, text) = match text.split_once('|') {
                    Some((made_for, list)) => {
                        let (minutes, ticks) = made_for.split_once(',').ok_or_else(rejected)?;
                        let made_for = minutes.parse().ok().zip(ticks.parse().ok());
                        (Some(made_for.ok_or_else(rejected)?), list)
                    }
                    None => (None, text.as_str()),
                };
                if id == "composites" {
                    self.composites =
                        (made_for, Self::parse_composites(text).ok_or_else(rejected)?);
                } else {
                    let mut instants = Self::parse_splits(text).ok_or_else(rejected)?;
                    instants.sort_unstable();
                    instants.dedup();
                    self.splits = (made_for, instants);
                }
            }
            _ => return Err(crate::studies::StudyError::rejected(&self.inputs(), id)),
        }
        self.update_session();
        self.update_edits();
        Ok(())
    }
    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        self.instrument_tick = instrument.and_then(|instrument| instrument.tick_size);
        self.update_tick_size();
        false
    }
    fn tpo(&self) -> Option<&TPOConfig> {
        Some(&self.config)
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.values = IndicatorValues::missing(data.len());
    }

    fn update(&mut self, data: &[Bar], _from: usize) {
        self.calculate(data);
    }
    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn line_names(&self) -> Vec<String> {
        Vec::new()
    }
    fn colors(&self) -> Vec<Color32> {
        Vec::new()
    }
    fn set_colors(&mut self, _colors: Vec<Color32>) {}
    fn is_visible(&self) -> bool {
        self.visible
    }
    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
    // The chart draws its profiles from its own bars.
    fn draws_on_other_bars(&self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}
