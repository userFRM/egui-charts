//! Market Profile letters of each session drawn beside the session's bars.

use crate::model::{
    Bar, CustomProfileSession, Instrument, ProfileSession, TPOConfig, TPODisplayMode, TimezoneId,
};
use crate::studies::{Indicator, IndicatorValues, InputKind, InputValue, StudyInput};
use chrono::{NaiveTime, TimeDelta};
use egui::Color32;

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
        // Study inputs have no time of day, so custom hours take minutes after midnight.
        let minutes = |time: NaiveTime| (time - NaiveTime::MIN).num_minutes() as usize;
        let zone = self.custom.zone.as_ref().map_or("", TimezoneId::name);
        vec![
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
                "Session start, minutes after midnight",
                InputValue::Length(minutes(self.custom.start)),
                InputKind::Length(0, 1439),
            )
            .with_default(InputValue::Length(0))
            .shown_when("hours", [2]),
            StudyInput::new(
                "session_end",
                "Session end, minutes after midnight",
                InputValue::Length(minutes(self.custom.end)),
                InputKind::Length(0, 1439),
            )
            .with_default(InputValue::Length(0))
            .shown_when("hours", [2]),
            StudyInput::new(
                "session_zone",
                "Session time zone (empty = exchange)",
                InputValue::Text(zone.into()),
                InputKind::Text,
            )
            .with_default(InputValue::Text(String::new()))
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
        ]
    }
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), crate::studies::StudyError> {
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
            ("session_start", InputValue::Length(minutes @ 0..=1439)) => {
                self.custom.start = NaiveTime::MIN + TimeDelta::minutes(minutes as i64);
                self.update_session();
            }
            ("session_end", InputValue::Length(minutes @ 0..=1439)) => {
                self.custom.end = NaiveTime::MIN + TimeDelta::minutes(minutes as i64);
                self.update_session();
            }
            ("session_zone", InputValue::Text(text)) => {
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
    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}
