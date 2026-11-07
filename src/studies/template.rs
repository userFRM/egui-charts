//! Serializable inputs and presentation of studies, alone or as a chart's set.

use crate::studies::{
    ConfiguredStudy, Indicator, IndicatorFactory, InputValue, StudyInput, StudyScale, StudySettings,
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
    pub fn create(&self) -> Result<Box<dyn Indicator>, crate::studies::StudyError> {
        self.create_with(&IndicatorFactory::new())
    }

    /// Reconstruct a study through `factory`, which resolves [`name`](Self::name) to a built-in
    /// or host-registered study. Uses the saved settings rather than the factory's host defaults.
    /// Fails like [`create`](Self::create).
    pub fn create_with(
        &self,
        factory: &IndicatorFactory,
    ) -> Result<Box<dyn Indicator>, crate::studies::StudyError> {
        // A host study registered under a built-in's name takes its own template as saved,
        // where an upgrade for that built-in would rename or convert its inputs.
        let own = factory.create_registered(&self.name);
        let own_inputs = own.as_ref().map_or_else(Vec::new, |study| study.inputs());
        let upgraded = self.upgraded(&own_inputs);
        let template = upgraded.as_ref().unwrap_or(self);
        let (study, declared) = match own {
            Some(study) if template.name == self.name => (study, own_inputs),
            _ => {
                let study = factory.create_registered(&template.name).ok_or_else(|| {
                    crate::studies::StudyError::UnknownStudy(template.name.clone())
                })?;
                let declared = study.inputs();
                (study, declared)
            }
        };
        // A host study registered under a built-in's name never had the inputs an upgrade adds
        // for that built-in.
        let template = if template.inputs.iter().all(|(id, _)| {
            self.inputs.iter().any(|(saved, _)| saved == id)
                || declared.iter().any(|input| input.id == *id)
        }) {
            template
        } else {
            self
        };
        // Automatic drawings saved before their swings could follow volatility used the fixed
        // percentage they were saved with.
        let saved = |id| template.inputs.iter().any(|(saved, _)| saved == id);
        let percent_swings = (declared.iter().any(|input| input.id == "swing_size")
            && !saved("swing_size")
            && saved("deviation_percent"))
        .then(|| ("swing_size".to_owned(), InputValue::Choice(1)));
        let inputs: Vec<_> = percent_swings
            .into_iter()
            .chain(template.inputs.iter().cloned())
            .collect();
        let mut study = ConfiguredStudy::new(study);
        study.apply_settings(&inputs, template.settings.clone())?;
        study.set_visible(template.visible);
        if let Some(scale) = template.scale {
            study.set_scale(scale);
        }
        Ok(Box::new(study))
    }

    /// This template under the current input ids of a built-in study whose inputs changed after
    /// it was saved, with inputs added since set to draw what it drew, or `None` when it needs no
    /// change. `own` holds the inputs of the study registered under the saved name; a saved input
    /// that study takes as saved is neither renamed nor converted.
    fn upgraded(&self, own: &[StudyInput]) -> Option<Self> {
        let has = |id: &str| self.inputs.iter().any(|(input, _)| input == id);
        let declares = |id: &str| own.iter().any(|input| input.id == id);
        // Profile endpoints saved in Unix milliseconds that the study does not take as numbers.
        let millis = |id: &str, value: &InputValue| match (id, value) {
            ("start_time" | "end_time", InputValue::Number(ms))
                if !own
                    .iter()
                    .any(|input| input.id == id && input.accepts(value)) =>
            {
                Some(*ms)
            }
            _ => None,
        };
        let bands = self
            .inputs
            .iter()
            .find_map(|(id, value)| match (id.as_str(), value) {
                ("band_count", InputValue::Length(count)) => Some(*count),
                _ => None,
            })
            .unwrap_or(0);
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
            "MA Channel" if has("offset_percent") && !declares("offset_percent") => {
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
            // Every TPO profile drew its midpoint before it had a setting.
            "Time Price Opportunity" if !has("midpoint") => {
                template
                    .inputs
                    .push(("midpoint".to_owned(), InputValue::Toggle(true)));
            }
            // An RSI drew no average and no middle line before it opened with both; the hidden
            // level keeps the styles saved for the other two and stays within their band. Its
            // average then chose from None, SMA and EMA, under ids of its own, before it took the
            // shared smoothing inputs.
            "RSI" if !has("smoothing") => {
                template.inputs.retain(|(id, _)| id != "ma_type");
                for (id, _) in &mut template.inputs {
                    if id == "ma_length" {
                        *id = "smoothing_length".to_owned();
                    }
                }
                let smoothing = self.inputs.iter().find_map(|(id, value)| match value {
                    InputValue::Choice(2) if id == "ma_type" => Some(3),
                    InputValue::Choice(choice) if id == "ma_type" => Some(*choice),
                    _ => None,
                });
                template.inputs.push((
                    "smoothing".to_owned(),
                    InputValue::Choice(smoothing.unwrap_or(0)),
                ));
                let declared = crate::studies::RSI::new(14);
                if template.settings.levels.is_empty() {
                    let (levels, fill, color) = super::settings::declared_levels(&declared);
                    template.settings.levels = levels.into_iter().take(2).collect();
                    template.settings.fill_between_levels = fill;
                    template.settings.level_fill_color = color;
                }
                if let [upper, lower] = template.settings.levels[..] {
                    let pane = declared.pane();
                    let upper = upper.value.unwrap_or(pane.levels[0].0);
                    let lower = lower.value.unwrap_or(pane.levels[1].0);
                    let band = upper.min(lower)..=upper.max(lower);
                    template.settings.levels.push(crate::studies::LevelStyle {
                        value: (!band.contains(&pane.levels[2].0)).then_some(upper),
                        color: crate::studies::palette::MUTED,
                        pattern: crate::model::LineStyle::Dashed,
                        width: 1.0,
                        visible: false,
                    });
                }
            }
            // A VWAP opened without bands, and no band was filled, before it opened with a filled
            // one.
            "VWAP" if !has("band_count") || (self.settings.fills.is_empty() && bands > 0) => {
                if !has("band_count") {
                    template
                        .inputs
                        .push(("band_count".to_owned(), InputValue::Length(0)));
                }
                // Hidden, each fill keeps the colour its band is filled in once shown. Each band
                // draws two of the saved lines, so their number also bounds a corrupt band count.
                template.settings.fills = (0..bands.min(self.settings.plots.len()))
                    .map(|band| crate::studies::FillStyle {
                        color: self
                            .settings
                            .plots
                            .get(1 + 2 * band)
                            .map_or(egui::Color32::TRANSPARENT, |plot| {
                                plot.color.gamma_multiply_u8(13)
                            }),
                        inverted_color: None,
                        visible: false,
                    })
                    .collect();
            }
            // Profile endpoints were Unix milliseconds before they read as UTC dates and times.
            "Vol Profile FR" | "Vol Profile Anchored"
                if self
                    .inputs
                    .iter()
                    .any(|(id, value)| millis(id, value).is_some()) =>
            {
                for (id, value) in &mut template.inputs {
                    if let Some(ms) = millis(id, value) {
                        *value = InputValue::Text(super::builtin::time_text(ms));
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
                "Vol Profile Session",
                vec![
                    (
                        "calculation_timeframe",
                        InputValue::Timeframe(Some(crate::model::Timeframe::Min5)),
                    ),
                    ("wait_for_close", Toggle(false)),
                ],
                vec![
                    (
                        "calculation_timeframe",
                        InputValue::Timeframe(Some(crate::model::Timeframe::Min5)),
                    ),
                    ("wait_for_close", Toggle(false)),
                ],
            ),
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
            // Saved before the midpoint had a setting, when every profile drew it.
            (
                "Time Price Opportunity",
                vec![("display", Choice(1))],
                vec![("display", Choice(1)), ("midpoint", Toggle(true))],
            ),
            (
                "Time Price Opportunity",
                vec![("midpoint", Toggle(false))],
                vec![("midpoint", Toggle(false))],
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

    /// An RSI and a VWAP saved before their defaults gained an average and a band restore with the
    /// plots they had.
    #[test]
    fn saved_studies_keep_the_plots_they_had_before_their_defaults_gained_more() {
        let mut rsi = ConfiguredStudy::new(Box::new(RSI::new(9)));
        let mut settings = rsi.settings();
        settings.levels[0].value = Some(80.0);
        rsi.apply_settings(&[], settings).unwrap();
        let mut saved = StudyTemplate::from_study(&rsi);
        saved.inputs.retain(|(id, _)| !id.starts_with("smoothing"));
        saved.settings.levels.truncate(2);
        let restored = saved.create().unwrap();
        assert_eq!(restored.line_count(), 1);
        assert_eq!(
            restored
                .pane()
                .levels
                .iter()
                .map(|l| l.0)
                .collect::<Vec<_>>(),
            [80.0, 30.0, 50.0]
        );
        assert!(!restored.settings().levels[2].visible);

        // Saved with an EMA from the shorter list of types, under the average's earlier ids.
        let mut older = StudyTemplate::from_study(&RSI::default());
        older.inputs.retain(|(id, _)| !id.starts_with("smoothing"));
        older
            .inputs
            .push(("ma_type".to_owned(), InputValue::Choice(2)));
        older
            .inputs
            .push(("ma_length".to_owned(), InputValue::Length(5)));
        let inputs = older.create().unwrap().inputs();
        let value = |id: &str| {
            inputs
                .iter()
                .find(|input| input.id == id)
                .unwrap()
                .value
                .clone()
        };
        assert_eq!(value("smoothing"), InputValue::Choice(3));
        assert_eq!(value("smoothing_length"), InputValue::Length(5));

        // Saved without bands, with an older flag instead of the band inputs, and with bands that
        // were not filled yet.
        let plain = StudyTemplate::from_study(&VolumeWeightedAvgPrice::new());
        assert_eq!(plain.create().unwrap().line_count(), 1);
        let mut older = plain.clone();
        older.inputs = vec![("daily".to_owned(), InputValue::Toggle(false))];
        assert_eq!(older.create().unwrap().line_count(), 1);
        let bands = VolumeWeightedAvgPrice::new().with_std_dev_bands([1.0, 2.0]);
        let mut unfilled = StudyTemplate::from_study(&bands);
        unfilled.settings.fills.clear();
        let restored = unfilled.create().unwrap();
        assert_eq!(restored.line_count(), 5);
        assert!(restored.fills().is_empty());
        // Saved since then, a filled band stays filled.
        let filled = StudyTemplate::from_study(&bands).create().unwrap();
        assert_eq!(filled.fills().len(), 2);
    }

    /// A host study registered under a built-in's name replaces it (`IndicatorFactory::register`),
    /// so its own template restores through that factory as that study, with the inputs and plots
    /// it was saved with, also where an upgrade of the built-in's templates adds, renames or
    /// converts an input.
    #[test]
    fn host_studies_named_like_upgraded_builtins_restore_from_their_templates() {
        use crate::studies::{CustomIndicator, IndicatorValue, InputKind};
        use InputValue::{Length, Number};
        let millis = || InputKind::Number(0.0, 1e13);
        let rows = [
            ("RSI", vec![], 1),
            ("VWAP", vec![], 1),
            ("Trading Sessions", vec![], 1),
            // Built-in templates with `offset_percent` restore as Envelopes, which draws three
            // lines too.
            (
                "MA Channel",
                vec![
                    ("period", Length(20), InputKind::Length(1, 500)),
                    ("offset_percent", Number(2.5), InputKind::Number(0.0, 100.0)),
                ],
                3,
            ),
            // Built-in templates with numeric endpoints restore them as dates and times.
            (
                "Vol Profile FR",
                vec![
                    ("start_time", Number(1_772_838_000_500.0), millis()),
                    ("end_time", Number(1_772_841_600_000.0), millis()),
                ],
                1,
            ),
            (
                "Vol Profile Anchored",
                vec![("start_time", Number(1_772_838_000_500.0), millis())],
                1,
            ),
        ];
        let mut factory = IndicatorFactory::new();
        for (name, inputs, lines) in rows.clone() {
            let schema: Vec<_> = inputs
                .into_iter()
                .map(|(id, value, kind)| StudyInput::new(id, id, value, kind))
                .collect();
            factory.register(name, move || {
                let close = |bars: &[Bar], _: &[StudyInput]| {
                    bars.iter()
                        .map(|bar| IndicatorValue::Single(bar.close))
                        .collect()
                };
                Box::new(
                    CustomIndicator::new(name, |_| Vec::new())
                        .with_line_count(lines)
                        .with_inputs(schema.clone(), close)
                        .unwrap(),
                )
            });
        }
        for (name, inputs, lines) in rows {
            let saved = StudyTemplate::from_study(factory.create(name).unwrap().as_ref());
            let restored = saved
                .create_with(&factory)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!((restored.name(), restored.line_count()), (name, lines));
            let restored = restored.inputs();
            for (id, value, _) in inputs {
                let input = restored.iter().find(|input| input.id == id);
                assert_eq!(input.map(|input| &input.value), Some(&value), "{name} {id}");
            }
        }
    }

    /// An RSI saved before it had a middle level draws the lines and band it was saved with. With
    /// its levels edited to 40 and 20 it draws those two and fills from 20 to 40; saved before
    /// levels were editable it draws 70 and 30 and fills between them. Both fill in the RSI's band
    /// colour.
    #[test]
    fn rsi_saved_before_the_middle_level_draws_its_saved_levels_and_band() {
        let mut edited = ConfiguredStudy::new(Box::new(RSI::new(14)));
        let mut settings = edited.settings();
        settings.levels[0].value = Some(40.0);
        settings.levels[1].value = Some(20.0);
        edited.apply_settings(&[], settings).unwrap();
        let mut edited = StudyTemplate::from_study(&edited);
        edited.inputs.retain(|(id, _)| !id.starts_with("smoothing"));
        edited.settings.levels.truncate(2);
        // Settings of that age had no level fields, which read as their serde defaults.
        let mut older = StudyTemplate::from_study(&RSI::new(14));
        older.inputs.retain(|(id, _)| !id.starts_with("smoothing"));
        older.settings.levels.clear();
        older.settings.fill_between_levels = false;
        older.settings.level_fill_color = Color32::default();
        // The lines drawn, the band, which a hidden level still bounds, and its fill.
        let drawn = |template: &StudyTemplate| {
            let study = template.create().unwrap();
            let pane = study.pane();
            let lines: Vec<f64> = pane
                .levels
                .iter()
                .filter(|level| level.1 != Color32::TRANSPARENT)
                .map(|level| level.0)
                .collect();
            let values = pane.levels.iter().map(|level| level.0);
            let band = (
                values.clone().fold(f64::INFINITY, f64::min),
                values.fold(f64::NEG_INFINITY, f64::max),
            );
            (
                lines,
                band,
                pane.fill_between_levels,
                study.level_fill_color(),
            )
        };
        let fill = RSI::new(14).level_fill_color();
        assert_eq!(
            [drawn(&edited), drawn(&older)],
            [
                (vec![40.0, 20.0], (20.0, 40.0), true, fill),
                (vec![70.0, 30.0], (30.0, 70.0), true, fill),
            ]
        );
    }

    /// A VWAP saved with a band before bands were filled restores with its fill hidden; shown
    /// again, the fill takes the band colour at 5% opacity, as on a VWAP added now.
    #[test]
    fn vwap_saved_before_its_fill_shows_the_band_colour_once_the_fill_is_shown() {
        let mut saved = StudyTemplate::from_study(&VolumeWeightedAvgPrice::default());
        saved.settings.fills.clear();
        let mut study = saved.create().unwrap();
        assert!(study.fills().is_empty());
        let mut settings = study.settings();
        settings.fills[0].visible = true;
        study.apply_settings(&[], settings).unwrap();
        let colors: Vec<Color32> = study.fills().iter().map(|fill| fill.color).collect();
        assert_eq!(
            colors,
            [crate::studies::palette::GREEN.gamma_multiply_u8(13)]
        );
    }

    /// A VWAP template with a band count no VWAP accepts fails to restore like any invalid input,
    /// instead of sizing its band styles by that count.
    #[test]
    fn vwap_templates_with_a_corrupt_band_count_fail_to_restore() {
        let mut saved = StudyTemplate::from_study(&VolumeWeightedAvgPrice::default());
        saved.settings.fills.clear();
        for (id, value) in &mut saved.inputs {
            if id == "band_count" {
                *value = InputValue::Length(usize::MAX);
            }
        }
        assert!(matches!(
            saved.create(),
            Err(crate::studies::StudyError::InvalidInput { id, .. }) if id == "band_count"
        ));
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
