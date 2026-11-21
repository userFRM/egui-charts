//! Market Profile letters of each session drawn beside the session's bars.

use crate::model::{
    Bar, CustomProfileSession, Instrument, ProfileSession, TPOColorMode, TPOConfig, TPODisplayMode,
    TimezoneId,
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
/// pre-market or post-market alone, counting only the bars of those hours, and Split extended
/// hours divides the trading day into its pre-market, regular and post-market profiles.
#[derive(Clone, Debug)]
pub struct TimePriceOpportunity {
    row_ticks: usize,
    instrument_tick: Option<f64>,
    hours: usize,
    custom: CustomProfileSession,
    config: TPOConfig,
    values: IndicatorValues,
    visible: bool,
}

impl TimePriceOpportunity {
    /// Profiles of `period_minutes` periods, raised to the bar interval when shorter.
    pub fn new(period_minutes: u32) -> Self {
        Self {
            row_ticks: 0,
            instrument_tick: None,
            hours: 0,
            custom: CustomProfileSession::new(NaiveTime::MIN, NaiveTime::MIN),
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

    /// Input values that store the profile menu's merges and splits.
    pub(crate) fn profile_edit_inputs(
        composites: &[RangeInclusive<DateTime<Utc>>],
        splits: &[DateTime<Utc>],
    ) -> [(String, InputValue); 2] {
        let instant = |time: &DateTime<Utc>| time.to_rfc3339_opts(SecondsFormat::AutoSi, true);
        [
            (
                "composites".into(),
                InputValue::Text(
                    composites
                        .iter()
                        .map(|r| format!("{}/{}", instant(r.start()), instant(r.end())))
                        .collect::<Vec<_>>()
                        .join(";"),
                ),
            ),
            (
                "splits".into(),
                InputValue::Text(splits.iter().map(instant).collect::<Vec<_>>().join(";")),
            ),
        ]
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
            0 if self.config.split_sessions => ProfileSession::Segments,
            1 => ProfileSession::Regular,
            2 => ProfileSession::Custom(self.custom.clone()),
            3 => ProfileSession::PreMarket,
            4 => ProfileSession::PostMarket,
            _ => ProfileSession::Chart,
        };
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
        let zone = self.custom.zone.as_ref().map_or("", TimezoneId::name);
        let mut inputs = vec![
            StudyInput::new(
                "period",
                "Period (minutes)",
                InputValue::Length(self.config.period_minutes as usize),
                InputKind::Length(1, 1440),
            )
            .with_default(InputValue::Length(30)),
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
                InputValue::TimeOfDay(self.custom.start),
                InputKind::TimeOfDay,
            )
            .with_default(InputValue::TimeOfDay(NaiveTime::MIN))
            .shown_when("hours", [2]),
            StudyInput::new(
                "session_end",
                "Session end",
                InputValue::TimeOfDay(self.custom.end),
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
                "value_area_levels",
                "Value area high and low",
                InputValue::Toggle(self.config.show_value_area),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_value_area)),
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
        for (id, value) in Self::profile_edit_inputs(&self.config.composites, &self.config.splits) {
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
        match (id, value) {
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
            ("hours", InputValue::Choice(choice @ 0..=4)) => {
                self.hours = choice;
                self.update_session();
            }
            ("session_start", InputValue::TimeOfDay(time)) => {
                self.custom.start = time;
                self.update_session();
            }
            ("session_end", InputValue::TimeOfDay(time)) => {
                self.custom.end = time;
                self.update_session();
            }
            ("session_zone", InputValue::TimeZone(text)) => {
                self.custom.zone = if text.is_empty() {
                    None
                } else {
                    Some(TimezoneId::named(&text).ok_or_else(|| {
                        crate::studies::StudyError::InvalidInput {
                            id: id.into(),
                            reason: "names no known time zone".into(),
                        }
                    })?)
                };
                self.update_session();
            }
            ("split_sessions", InputValue::Toggle(split)) => {
                self.config.split_sessions = split;
                self.update_session();
            }
            ("single_prints", InputValue::Toggle(show)) => self.config.show_single_prints = show,
            ("midpoint", InputValue::Toggle(show)) => self.config.show_midpoint = show,
            ("statistics", InputValue::Toggle(show)) => self.config.show_statistics = show,
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
            ("value_area_levels", InputValue::Toggle(show)) => self.config.show_value_area = show,
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
            ("composites", InputValue::Text(text)) => {
                let ranges = if text.is_empty() {
                    Some(Vec::new())
                } else {
                    text.split(';')
                        .map(|range| {
                            let (start, end) = range.split_once('/')?;
                            let start = DateTime::parse_from_rfc3339(start)
                                .ok()?
                                .with_timezone(&Utc);
                            let end = DateTime::parse_from_rfc3339(end).ok()?.with_timezone(&Utc);
                            (start <= end).then_some(start..=end)
                        })
                        .collect::<Option<Vec<_>>>()
                };
                self.config.composites = ranges
                    .ok_or_else(|| crate::studies::StudyError::rejected(&self.inputs(), id))?;
            }
            ("splits", InputValue::Text(text)) => {
                let instants = if text.is_empty() {
                    Some(Vec::new())
                } else {
                    text.split(';')
                        .map(|time| {
                            DateTime::parse_from_rfc3339(time)
                                .ok()
                                .map(|time| time.with_timezone(&Utc))
                        })
                        .collect::<Option<Vec<_>>>()
                };
                let mut instants = instants
                    .ok_or_else(|| crate::studies::StudyError::rejected(&self.inputs(), id))?;
                instants.sort_unstable();
                instants.dedup();
                self.config.splits = instants;
            }
            _ => return Err(crate::studies::StudyError::rejected(&self.inputs(), id)),
        }
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
