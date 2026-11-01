//! Serializable inputs and presentation of studies, alone or as a chart's set.

use crate::studies::{
    ConfiguredStudy, Indicator, IndicatorFactory, InputValue, StudyScale, StudySettings,
};

/// Serializable inputs and presentation of one study. Hosts may resolve `name` through their
/// own study factory when they supply custom studies.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StudyTemplate {
    /// Study factory name.
    pub name: String,
    /// Typed inputs keyed by their stable study-local identifiers.
    pub inputs: Vec<(String, InputValue)>,
    /// Plot styles, precision and interval visibility.
    pub settings: StudySettings,
    /// Whether the study is visible.
    pub visible: bool,
    /// The value scale, or `None` to keep the study's own, as templates saved before the scale
    /// was kept do.
    #[serde(default)]
    pub scale: Option<StudyScale>,
}

/// The studies of a chart, with the links between them and the panes they share, as
/// `Chart::study_template_set` saves them and `Chart::apply_study_template_set` applies them.
///
/// An input that reads another study's plot (`StudySource::Study`) names that study by its
/// position in [`studies`](Self::studies), and a position past the end names no study. A
/// position appears at most once in [`price_pane`](Self::price_pane) and
/// [`panes`](Self::panes) together; a study listed in neither is placed where
/// `Chart::add_indicator` places a new study. Legend captions, the price auto-scale choice,
/// pane heights and the place of the price pane among the panes are not kept.
///
/// A list of study templates also reads as a set: its sources name positions in the list and
/// every study takes its default place.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[non_exhaustive]
pub struct StudyTemplateSet {
    /// The studies, in the order they were added to the chart.
    pub studies: Vec<StudyTemplate>,
    /// Positions in `studies` of the studies drawn on the price pane.
    pub price_pane: Vec<usize>,
    /// The other panes that hold studies of the set, from top to bottom, each with the
    /// positions in `studies` of its studies in legend order.
    pub panes: Vec<Vec<usize>>,
}

impl<'de> serde::Deserialize<'de> for StudyTemplateSet {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            studies: Vec<StudyTemplate>,
            #[serde(default)]
            price_pane: Vec<usize>,
            #[serde(default)]
            panes: Vec<Vec<usize>>,
        }
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = StudyTemplateSet;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a study template set or a list of study templates")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                seq: A,
            ) -> Result<Self::Value, A::Error> {
                let studies = serde::Deserialize::deserialize(
                    serde::de::value::SeqAccessDeserializer::new(seq),
                )?;
                Ok(StudyTemplateSet {
                    studies,
                    ..StudyTemplateSet::default()
                })
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                let Fields {
                    studies,
                    price_pane,
                    panes,
                } = serde::Deserialize::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))?;
                Ok(StudyTemplateSet {
                    studies,
                    price_pane,
                    panes,
                })
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

impl StudyTemplate {
    /// Capture the current calculation inputs, presentation and value scale, without computed
    /// values. An input that reads another study's plot keeps that study's chart id;
    /// `Chart::study_template_set` saves a chart's studies with the links between them.
    pub fn from_study(study: &dyn Indicator) -> Self {
        Self {
            name: study.name().into(),
            inputs: study
                .inputs()
                .into_iter()
                .map(|input| (input.id, input.value))
                .collect(),
            settings: study.settings(),
            visible: study.is_visible(),
            scale: Some(study.scale()),
        }
    }

    /// Reconstruct a built-in study. Unknown names or invalid inputs leave the host chart intact.
    /// Hosts reconstruct the entire set successfully before replacing their current studies.
    pub fn create(&self) -> Result<Box<dyn Indicator>, String> {
        self.create_with(&IndicatorFactory::new())
    }

    /// Reconstruct a study through `factory`, which resolves [`name`](Self::name) to a built-in
    /// or host-registered study. Fails like [`create`](Self::create).
    pub fn create_with(&self, factory: &IndicatorFactory) -> Result<Box<dyn Indicator>, String> {
        let upgraded = self.upgraded();
        let template = upgraded.as_ref().unwrap_or(self);
        let study = factory
            .create(&template.name)
            .ok_or_else(|| format!("Unknown study: {}", template.name))?;
        // Automatic drawings saved before their swings could follow volatility used the fixed
        // percentage they were saved with.
        let saved = |id| template.inputs.iter().any(|(saved, _)| saved == id);
        let percent_swings = (study.inputs().iter().any(|input| input.id == "swing_size")
            && !saved("swing_size")
            && saved("deviation_percent"))
        .then(|| ("swing_size".to_owned(), InputValue::Choice(1)));
        let inputs: Vec<_> = percent_swings
            .into_iter()
            .chain(template.inputs.iter().cloned())
            .collect();
        let mut study = ConfiguredStudy::new(study);
        study
            .apply_settings(&inputs, template.settings.clone())
            .map_err(str::to_owned)?;
        study.set_visible(template.visible);
        if let Some(scale) = template.scale {
            study.set_scale(scale);
        }
        Ok(Box::new(study))
    }

    /// This template under the current input ids of a built-in study whose inputs changed after
    /// it was saved, with inputs added since set to draw what it drew, or `None` when it needs no
    /// change.
    fn upgraded(&self) -> Option<Self> {
        let has = |id: &str| self.inputs.iter().any(|(input, _)| input == id);
        let mut template = self.clone();
        match self.name.as_str() {
            // One multiplier set both bands before each had its own.
            "LinReg Channel" if has("multiplier") => {
                template.inputs = self
                    .inputs
                    .iter()
                    .flat_map(|(id, value)| match id.as_str() {
                        "multiplier" => vec![
                            ("upper_multiplier".to_owned(), value.clone()),
                            ("lower_multiplier".to_owned(), value.clone()),
                        ],
                        _ => vec![(id.clone(), value.clone())],
                    })
                    .collect();
            }
            // Before it averaged highs and lows, MA Channel drew the close average with percentage
            // bands, which Envelopes draws with a simple average.
            "MA Channel" if has("offset_percent") => {
                template.name = "Envelopes".to_owned();
                for (id, _) in &mut template.inputs {
                    if id == "offset_percent" {
                        *id = "percent".to_owned();
                    }
                }
            }
            // Pivot levels had no labels before they could be labelled.
            "Pivot Points Standard" | "Intraday Pivot Points" | "Projected Pivot Points"
                if !has("show_labels") =>
            {
                template
                    .inputs
                    .push(("show_labels".to_owned(), InputValue::Toggle(false)));
            }
            // Sessions were only highlighted before they could be boxed and named.
            "Trading Sessions" if !has("boxes") => {
                for id in ["boxes", "name_labels"] {
                    template
                        .inputs
                        .push((id.to_owned(), InputValue::Toggle(false)));
                }
            }
            // Profile endpoints were Unix milliseconds before they read as UTC dates and times.
            "Vol Profile FR" | "Vol Profile Anchored"
                if self.inputs.iter().any(|(id, value)| {
                    matches!(id.as_str(), "start_time" | "end_time")
                        && matches!(value, InputValue::Number(_))
                }) =>
            {
                for (id, value) in &mut template.inputs {
                    if let ("start_time" | "end_time", InputValue::Number(ms)) =
                        (id.as_str(), &*value)
                    {
                        *value = InputValue::Text(super::builtin::time_text(*ms));
                    }
                }
            }
            _ => return None,
        }
        Some(template)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, PriceSource, SeriesScale, Session, SessionWindow, TimezoneId};
    use crate::studies::{
        AverageLine, AverageType, CVD, CloseToCloseVol, EhlersFisher, HL2, HLC3, MACombination,
        MARibbon, PriceChannel, RSI, TradingSessions, VolumeWeightedAvgPrice,
    };
    use chrono::{NaiveTime, TimeZone, Utc, Weekday};
    use egui::Color32;

    /// What a chart shows of a calculated study: its name, values, line names, crossing
    /// markers, line styles, value scale and drawings, as text.
    fn shown(study: &dyn Indicator) -> String {
        let styles: Vec<_> = (0..study.line_count())
            .map(|line| study.line_style(line))
            .collect();
        #[cfg(feature = "drawings")]
        let drawings = format!("{:?}", study.drawings());
        #[cfg(not(feature = "drawings"))]
        let drawings = "";
        format!(
            "{} {:?} {:?} {:?} {:?} {:?} {drawings}",
            study.name(),
            study.values(),
            study.line_names(),
            study.markers(),
            styles,
            study.scale(),
        )
    }

    /// A study written out as a template and read back is recreated as it was, with its
    /// builder settings and scale, also when the catalogue does not list it.
    #[test]
    fn templates_recreate_studies_with_their_settings() {
        let bars: Vec<Bar> = (0..400)
            .map(|i| {
                let c = 100.0 + 10.0 * (i as f64 / 9.0).sin();
                let time = Utc.timestamp_opt(i * 3600, 0).unwrap();
                Bar::new(time, c - 0.5, c + 1.0, c - 1.0, c, 1000.0 + i as f64)
            })
            .collect();
        let line = |kind, period| AverageLine::new(kind, period, Color32::RED);
        let hour = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
        let tokyo = TimezoneId::fixed_offset(9 * 3600).unwrap();
        let weekdays = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
        ];
        let rows: Vec<Box<dyn Indicator>> = vec![
            // Exported studies outside the catalogue.
            Box::new(CVD::default()),
            Box::new(CloseToCloseVol::default()),
            Box::new(HL2::default()),
            Box::new(HLC3::default()),
            Box::new(PriceChannel::default()),
            Box::new(EhlersFisher::default()),
            // Fewer averages than the ribbon that the name creates, with a crossing pair.
            Box::new(
                MACombination::new(vec![
                    line(AverageType::Simple, 5),
                    line(AverageType::Hull, 9).with_source(PriceSource::HLC3),
                    line(AverageType::Exponential, 21),
                ])
                .unwrap()
                .with_crossing_pair(2, 0)
                .unwrap(),
            ),
            // More averages than that ribbon.
            Box::new(
                MACombination::new((1..=14).map(|i| line(AverageType::Simple, 3 * i)).collect())
                    .unwrap(),
            ),
            // Settings made through builder methods.
            Box::new(TradingSessions::new().with_session(
                1,
                "Night",
                hour(1),
                hour(5),
                tokyo.clone(),
            )),
            Box::new(
                VolumeWeightedAvgPrice::new()
                    .with_time_zone(tokyo)
                    .with_session(Session::new([SessionWindow::new(
                        weekdays,
                        hour(17),
                        hour(16),
                    )])),
            ),
            Box::new(MARibbon::muted()),
        ];
        let mut studies: Vec<ConfiguredStudy> =
            rows.into_iter().map(ConfiguredStudy::new).collect();
        #[cfg(feature = "drawings")]
        studies.push(ConfiguredStudy::new({
            use crate::drawings::{FibonacciConfig, FibonacciLevel};
            use crate::studies::{AutoDrawing, AutoDrawingKind};
            let mut fibonacci = FibonacciConfig {
                reverse: true,
                show_prices: Some(false),
                show_levels: Some(true),
                levels_as_percent: false,
                labels_on_left: true,
                show_background: true,
                ..FibonacciConfig::default()
            };
            fibonacci.levels.truncate(2);
            fibonacci.levels[0].show_price = false;
            fibonacci
                .levels
                .push(FibonacciLevel::new(1.5, "150%", [10, 20, 30, 255]));
            Box::new(
                AutoDrawing::new(AutoDrawingKind::FibonacciRetracement, 2.0, 5)
                    .with_fibonacci_config(fibonacci),
            )
        }));
        let mut scaled = ConfiguredStudy::new(Box::new(RSI::new(14)));
        scaled.set_scale(crate::studies::StudyScale::Shared(SeriesScale::Left));
        studies.push(scaled);

        let mut differing = Vec::new();
        for (row, mut study) in studies.into_iter().enumerate() {
            let json = serde_json::to_string(&StudyTemplate::from_study(&study)).unwrap();
            let template: StudyTemplate = serde_json::from_str(&json).unwrap();
            match template.create() {
                Ok(mut restored) => {
                    study.calculate(&bars);
                    restored.calculate(&bars);
                    if shown(restored.as_ref()) != shown(&study) {
                        differing.push(format!("{row} {}", study.name()));
                    }
                }
                Err(error) => differing.push(format!("{row} {}: {error}", study.name())),
            }
        }
        assert_eq!(differing, Vec::<String>::new());

        // Default sessions saved with `timezones` name their zones, and load without it.
        let mut sessions = StudyTemplate::from_study(&TradingSessions::new());
        let zones = sessions
            .inputs
            .iter_mut()
            .filter(|(id, _)| id.starts_with("zone "));
        for ((_, value), name) in zones.zip(["Asia/Tokyo", "Europe/London", "America/New_York"]) {
            *value = InputValue::Text(name.to_owned());
        }
        let mut restored = sessions.create().unwrap();
        let mut study = TradingSessions::new();
        restored.calculate(&bars);
        study.calculate(&bars);
        assert_eq!(shown(restored.as_ref()), shown(&study));

        // Automatic drawings keep their swing inputs, and those saved before swings could be
        // sized by volatility restore on the fixed percentage with their saved deviation.
        #[cfg(feature = "drawings")]
        {
            let mut study =
                ConfiguredStudy::new(IndicatorFactory::new().create("Auto Pitchfork").unwrap());
            let edited = [
                ("swing_size", InputValue::Choice(1)),
                ("atr_multiple", InputValue::Number(2.5)),
                ("atr_period", InputValue::Length(7)),
                ("deviation_percent", InputValue::Number(1.5)),
                ("depth", InputValue::Length(4)),
            ];
            for (id, value) in &edited {
                study.set_input(id, value.clone()).unwrap();
            }
            let saved = StudyTemplate::from_study(&study);
            let mut before_atr = saved.clone();
            before_atr.inputs.retain(|(id, _)| {
                !matches!(id.as_str(), "swing_size" | "atr_multiple" | "atr_period")
            });
            for (template, expected) in [
                (saved, &edited[..]),
                (
                    before_atr,
                    &[edited[0].clone(), edited[3].clone(), edited[4].clone()][..],
                ),
            ] {
                let inputs = template.create().unwrap().inputs();
                for (id, value) in expected {
                    let restored = inputs.iter().find(|input| input.id == *id).unwrap();
                    assert_eq!(&restored.value, value, "{id}");
                }
            }
        }
    }

    /// A template restores the inputs it holds whatever the study's default has become, with every
    /// line it showed, and an input it lacks takes the value the saved inputs had before the
    /// study offered it.
    #[test]
    fn templates_saved_before_split_lengths_restore_their_values() {
        use InputValue::{Choice, Length, Number, Text, Toggle};
        let rows = [
            (
                "SMA",
                vec![("period", Length(20))],
                vec![("period", Length(20))],
            ),
            (
                "ADX",
                vec![("period", Length(20))],
                vec![("period", Length(20)), ("adx_smoothing", Length(20))],
            ),
            (
                "DMI",
                vec![("period", Length(20))],
                vec![("period", Length(20)), ("adx_smoothing", Length(20))],
            ),
            (
                "Chandelier",
                vec![("period", Length(10)), ("multiplier", Number(2.0))],
                vec![
                    ("period", Length(10)),
                    ("atr_period", Length(10)),
                    ("multiplier", Number(2.0)),
                ],
            ),
            (
                "MA Cross",
                vec![("period_0", Length(5))],
                vec![
                    ("period_0", Length(5)),
                    ("cross_first", Choice(1)),
                    ("cross_second", Choice(2)),
                ],
            ),
            (
                "Multi MA",
                vec![("period_0", Length(30))],
                vec![("period_0", Length(30)), ("cross_first", Choice(0))],
            ),
            // Saved before levels were labelled and sessions boxed, both on by default since.
            (
                "Pivot Points Standard",
                vec![("pivots_back", Length(2))],
                vec![("pivots_back", Length(2)), ("show_labels", Toggle(false))],
            ),
            (
                "Projected Pivot Points",
                vec![("show_labels", Toggle(true))],
                vec![("show_labels", Toggle(true))],
            ),
            (
                "Trading Sessions",
                vec![("show [0]", Toggle(true))],
                vec![("boxes", Toggle(false)), ("name_labels", Toggle(false))],
            ),
            (
                "Trading Sessions",
                vec![("boxes", Toggle(true))],
                vec![("boxes", Toggle(true)), ("name_labels", Toggle(true))],
            ),
            // Saved as Unix milliseconds before the endpoints read as UTC dates and times.
            (
                "Vol Profile FR",
                vec![
                    ("start_time", Number(1_772_838_000_500.0)),
                    ("end_time", Number(8e15)),
                ],
                vec![
                    ("start_time", Text("2026-03-06 23:00:00.500".into())),
                    ("end_time", Text(String::new())),
                ],
            ),
            (
                "Vol Profile Anchored",
                vec![("start_time", Number(-8e15))],
                vec![("start_time", Text(String::new()))],
            ),
        ];
        let factory = IndicatorFactory::new();
        for (name, saved, restored) in rows {
            let mut settings = factory.create(name).unwrap().settings();
            for plot in &mut settings.plots {
                plot.visible = true;
            }
            let template = StudyTemplate {
                name: name.to_owned(),
                inputs: saved
                    .into_iter()
                    .map(|(id, value)| (id.to_owned(), value))
                    .collect(),
                settings,
                visible: true,
                scale: None,
            };
            let study = template.create().unwrap();
            let inputs = study.inputs();
            for (id, value) in restored {
                let input = inputs.iter().find(|input| input.id == id);
                assert_eq!(input.map(|input| &input.value), Some(&value), "{name} {id}");
            }
            assert!(
                (0..study.line_count()).all(|line| study.plot_style(line).visible),
                "{name}"
            );
        }
    }

    /// Templates and layouts saved before the channel studies' inputs changed load and draw the
    /// lines they drew.
    #[test]
    fn templates_saved_before_the_channel_inputs_changed_keep_their_lines() {
        use crate::studies::{IndicatorValue, StudySource};

        let bars: Vec<Bar> = [1.0, 4.0, 2.0, 4.0]
            .into_iter()
            .enumerate()
            .map(|(i, c)| Bar::new(Utc.timestamp_opt(i as i64, 0).unwrap(), c, c, c, c, 1.0))
            .collect();
        let configured = [
            (
                "source",
                InputValue::Source(StudySource::Price(PriceSource::Close)),
            ),
            ("plot_offset", InputValue::Integer(0)),
            ("calculation_symbol", InputValue::Symbol(String::new())),
            ("calculation_timeframe", InputValue::Timeframe(None)),
            ("wait_for_close", InputValue::Toggle(true)),
        ];
        let deviation = 2.0 / 3.0_f64.sqrt();
        // Upper, middle and lower line on bars 1 to 3; bar 0 has no value.
        let rows = [
            // A residual deviation of 2/sqrt(3) around the flat fit 10/3 of the last three closes.
            (
                "LinReg Channel",
                vec![
                    ("period", InputValue::Length(3)),
                    ("multiplier", InputValue::Number(1.0)),
                    ("extend", InputValue::Toggle(false)),
                ],
                "LinReg Channel",
                [[10.0 / 3.0 + deviation, 10.0 / 3.0, 10.0 / 3.0 - deviation]; 3],
            ),
            // The two-bar simple average of the closes, 2.5, 3 and 3, times 1.5 and 0.5.
            (
                "MA Channel",
                vec![
                    ("period", InputValue::Length(2)),
                    ("offset_percent", InputValue::Number(50.0)),
                ],
                "Envelopes",
                [[3.75, 2.5, 1.25], [4.5, 3.0, 1.5], [4.5, 3.0, 1.5]],
            ),
        ];
        let factory = IndicatorFactory::new();
        for (name, inputs, restored_name, expected) in rows {
            // Saved with the three plot styles of a study drawing the restored lines.
            let settings = ConfiguredStudy::new(factory.create(restored_name).unwrap()).settings();
            let template = StudyTemplate {
                name: name.to_owned(),
                inputs: inputs
                    .iter()
                    .chain(&configured)
                    .map(|(id, value)| ((*id).to_owned(), value.clone()))
                    .collect(),
                settings,
                visible: true,
                scale: None,
            };
            let mut study = template
                .create()
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            study.calculate(&bars);
            assert_eq!(study.name(), restored_name);
            assert!(
                matches!(study.values().to_rows()[0], IndicatorValue::None),
                "{name}"
            );
            for (bar, lines) in (1..).zip(expected) {
                for (line, want) in lines.into_iter().enumerate() {
                    let got = study.values().value(line, bar).unwrap();
                    assert!(
                        (got - want).abs() < 1e-12,
                        "{name} bar {bar} line {line}: {got}"
                    );
                }
            }
        }
    }
}
