//! Market Profile letters of each session drawn beside the session's bars.

use crate::model::{Bar, Instrument, TPOConfig, TPODisplayMode};
use crate::studies::{Indicator, IndicatorValues, InputKind, InputValue, StudyInput};
use egui::Color32;

/// Each session's TPO profile on the price pane, starting at the session's first bar, with its
/// point of control, value area and initial balance.
///
/// The chart builds the profiles of the whole sessions it shows from its bars and calendar with
/// [`to_tpo_profiles`](crate::model::to_tpo_profiles), through the same code and cache as the
/// TPO chart type, so the study's profile of a session equals the chart type's under the same
/// settings. [`Indicator::tpo`] exposes those settings; the study has no plotted values.
#[derive(Clone)]
pub struct TimePriceOpportunity {
    row_ticks: usize,
    instrument_tick: Option<f64>,
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
                "split_sessions",
                "Split extended hours",
                InputValue::Toggle(self.config.split_sessions),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.split_sessions)),
            StudyInput::new(
                "single_prints",
                "Single prints",
                InputValue::Toggle(self.config.show_single_prints),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(defaults.show_single_prints)),
            StudyInput::new(
                "display",
                "Display",
                InputValue::Choice(usize::from(
                    self.config.display_mode == TPODisplayMode::Blocks,
                )),
                InputKind::Choice(vec!["Letters".into(), "Blocks".into()]),
            )
            .with_default(InputValue::Choice(usize::from(
                defaults.display_mode == TPODisplayMode::Blocks,
            ))),
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
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
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
            ("split_sessions", InputValue::Toggle(split)) => self.config.split_sessions = split,
            ("single_prints", InputValue::Toggle(show)) => self.config.show_single_prints = show,
            ("display", InputValue::Choice(choice @ 0..=1)) => {
                self.config.display_mode = [TPODisplayMode::Letters, TPODisplayMode::Blocks][choice]
            }
            ("sessions", InputValue::Choice(choice @ 0..=1)) => {
                self.config.latest_session_only = choice == 1
            }
            _ => return Err("invalid TPO input"),
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
