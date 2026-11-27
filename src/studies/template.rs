//! Serializable inputs and presentation of studies, alone or as a chart's set.

use crate::model::Timeframe;
use crate::studies::{
    BollingerBands, CCI, ConfiguredStudy, EMA, Indicator, IndicatorFactory, InputValue, MACD,
    MARibbon, MoneyFlowIndex, OnBalanceVolume, RSI, Stochastic, StochasticRSI, StudyInput,
    StudyScale, StudySettings, Volume, WilliamsAlligator,
};

/// Serializable inputs and presentation of one study. Hosts may resolve `name` through their
/// own study factory when they supply custom studies.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct StudyTemplate {
    /// Study factory name.
    pub name: String,
    /// Typed inputs keyed by their stable study-local identifiers.
    pub inputs: Vec<(String, InputValue)>,
    /// Plot styles, precision and interval visibility.
    pub settings: StudySettings,
    /// Whether the user has the study shown. Its interval visibility in
    /// [`settings`](Self::settings) still hides it on the intervals it leaves out.
    pub visible: bool,
    /// The value scale, or `None` to keep the study's own, as templates saved before the scale
    /// was kept do.
    #[serde(default)]
    pub scale: Option<StudyScale>,
    /// A metric's host key, display name, unit, format and declared plot and colours. Restored without requiring a
    /// catalogue entry; its history is requested again from the host. Absent in older saves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<crate::data::MetricSeries>,
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
/// every study takes its default place. Sets saved without a [`symbol`](Self::symbol) or an
/// [`interval`](Self::interval) keep the chart's when applied.
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
    /// The symbol the chart shows when the set is applied, or `None` to keep the chart's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// The interval the chart shows when the set is applied, or `None` to keep the chart's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<Timeframe>,
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
            #[serde(default)]
            symbol: Option<String>,
            #[serde(default)]
            interval: Option<Timeframe>,
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
                    symbol,
                    interval,
                } = serde::Deserialize::deserialize(serde::de::value::MapAccessDeserializer::new(
                    map,
                ))?;
                Ok(StudyTemplateSet {
                    studies,
                    price_pane,
                    panes,
                    symbol,
                    interval,
                })
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

impl StudyTemplateSet {
    /// The built-in sets with their names, in the order a menu lists them, for a host to offer
    /// beside the user's own sets and apply as it applies those, for example with
    /// `Chart::apply_chosen_study_template_set`:
    ///
    /// - "Williams Alligator": [`WilliamsAlligator`].
    /// - "Displaced EMA": an [`EMA`] drawn 5 bars ahead.
    /// - "Moving average ribbon": [`MARibbon`].
    /// - "Oscillators": [`RSI`], [`StochasticRSI`] and [`CCI`].
    /// - "Swing trading": [`BollingerBands`], [`MACD`] and [`Stochastic`].
    /// - "Volume analysis": [`OnBalanceVolume`] and [`MoneyFlowIndex`].
    ///
    /// Each set starts with a [`Volume`] study, so that applying it keeps the chart's volume,
    /// and its studies take their default inputs, apart from the displaced EMA's offset. The
    /// overlays are drawn on the price pane and every other study has a pane of its own. No set
    /// keeps a symbol or an interval.
    pub fn builtin() -> Vec<(&'static str, Self)> {
        let mut displaced = ConfiguredStudy::new(Box::new(EMA::default()));
        displaced
            .set_input("plot_offset", InputValue::Integer(5))
            .expect("an EMA reads a price, so it takes an offset");
        let sets: [(&'static str, Vec<Box<dyn Indicator>>); 6] = [
            (
                "Williams Alligator",
                vec![Box::new(WilliamsAlligator::default())],
            ),
            ("Displaced EMA", vec![Box::new(displaced)]),
            ("Moving average ribbon", vec![Box::new(MARibbon::default())]),
            (
                "Oscillators",
                vec![
                    Box::new(RSI::default()),
                    Box::new(StochasticRSI::default()),
                    Box::new(CCI::default()),
                ],
            ),
            (
                "Swing trading",
                vec![
                    Box::new(BollingerBands::default()),
                    Box::new(MACD::default()),
                    Box::new(Stochastic::default()),
                ],
            ),
            (
                "Volume analysis",
                vec![
                    Box::new(OnBalanceVolume::default()),
                    Box::new(MoneyFlowIndex::default()),
                ],
            ),
        ];
        sets.into_iter()
            .map(|(name, studies)| {
                // Configured as a chart configures a study it adds, so that the set applies as one
                // saved from a chart: a bare volume study would keep its average plot hidden.
                let studies: Vec<Box<dyn Indicator>> =
                    std::iter::once(Box::new(Volume::default()) as Box<dyn Indicator>)
                        .chain(studies)
                        .map(|study| {
                            if study.has_settings() {
                                study
                            } else {
                                Box::new(ConfiguredStudy::new(study))
                            }
                        })
                        .collect();
                let (price_pane, own): (Vec<_>, Vec<_>) =
                    (0..studies.len()).partition(|&at| studies[at].is_overlay());
                let set = Self {
                    studies: studies
                        .iter()
                        .map(|study| StudyTemplate::from_study(study.as_ref()))
                        .collect(),
                    price_pane,
                    panes: own.into_iter().map(|at| vec![at]).collect(),
                    ..Self::default()
                };
                (name, set)
            })
            .collect()
    }
}

impl StudyTemplate {
    /// Capture the factory key, [`Indicator::registered_name`], calculation inputs, presentation
    /// and value scale, without computed values. A host metric keeps its descriptor too.
    /// An input that reads another study's plot keeps that study's chart id;
    /// `Chart::study_template_set` saves a chart's studies with the links between them.
    pub fn from_study(study: &dyn Indicator) -> Self {
        Self {
            name: study.registered_name().into(),
            inputs: study
                .inputs()
                .into_iter()
                .map(|input| (input.id, input.value))
                .collect(),
            settings: study.settings(),
            visible: study.user_visible(),
            scale: Some(study.scale()),
            metric: study.metric().cloned(),
        }
    }

    /// Reconstruct a built-in study or a saved host metric. Unknown names or invalid inputs
    /// leave the host chart intact.
    /// Hosts reconstruct the entire set successfully before replacing their current studies.
    pub fn create(&self) -> Result<Box<dyn Indicator>, crate::studies::StudyError> {
        self.create_with(&IndicatorFactory::new())
    }

    /// Reconstruct a study through `factory`, which resolves [`name`](Self::name) to a built-in
    /// or host-registered study. Uses the saved settings rather than the factory's host defaults,
    /// which each input the template names reports as its default instead.
    /// Fails like [`create`](Self::create).
    /// A saved metric is reconstructed from its descriptor, even without a catalogue entry.
    /// The study retains the factory key it was restored under, including a built-in's new key
    /// when an upgrade replaces an older study.
    pub fn create_with(
        &self,
        factory: &IndicatorFactory,
    ) -> Result<Box<dyn Indicator>, crate::studies::StudyError> {
        self.build(factory, true)
    }

    /// [`create_with`](Self::create_with), with the inputs reporting the factory's host defaults
    /// as their defaults only when `host_defaults` is set.
    pub(crate) fn build(
        &self,
        factory: &IndicatorFactory,
        host_defaults: bool,
    ) -> Result<Box<dyn Indicator>, crate::studies::StudyError> {
        // A host study registered under a built-in's name takes its own template as saved,
        // where an upgrade for that built-in would rename or convert its inputs.
        let own = self
            .metric
            .as_ref()
            .map(|metric| Box::new(super::MetricStudy::new(metric.clone())) as Box<dyn Indicator>)
            .or_else(|| factory.create_registered(&self.name));
        let own_inputs = own.as_ref().map_or_else(Vec::new, |study| study.inputs());
        let study_name = own
            .as_ref()
            .map_or(self.name.as_str(), |study| study.name());
        let upgraded = if self.metric.is_none() {
            self.upgraded(study_name, &own_inputs)
        } else {
            None
        };
        let template = upgraded.as_ref().unwrap_or(self);
        let registered = template.name.as_str();
        let (mut study, declared) = match own {
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
        let mut inputs: Vec<_> = percent_swings
            .into_iter()
            .chain(template.inputs.iter().cloned())
            .collect();
        // Resetting the inputs restores what a study created under the host's defaults starts
        // with. An input the template leaves out, such as one added after it was saved, keeps its
        // constructor's value and declared default, which a drawing stored with the study matches
        // to find its pane.
        let mut defaults = host_defaults
            .then(|| factory.input_defaults(registered))
            .flatten()
            .unwrap_or_default();
        defaults.retain(|(id, _)| inputs.iter().any(|(named, _)| named == id));
        // Keep a host's presentation wrapper when it can retain the factory key too.
        let mut study = if study.has_settings()
            && (study.registered_name() == registered || study.set_registered_name(registered))
        {
            study
        } else {
            Box::new(ConfiguredStudy::new(study))
        };
        study.set_registered_name(registered);
        if !defaults.is_empty() {
            study.set_input_defaults(&defaults);
        }
        // A study's own Source input once kept another study's output that never reached its
        // calculation, which read the input's default instead. An output the study now refuses
        // there, or in `source` while it runs on other bars, keeps that default.
        let output = |value: &InputValue| {
            matches!(
                value,
                InputValue::Source(crate::studies::StudySource::Study { .. })
            )
        };
        if inputs.iter().any(|(_, value)| output(value)) {
            let mut probe = study.clone_box();
            for (id, value) in &inputs {
                if matches!(id.as_str(), "calculation_symbol" | "calculation_timeframe") {
                    let _ = probe.set_input(id, value.clone());
                }
            }
            for (id, value) in &mut inputs {
                let declared = declared.iter().find(|input| {
                    input.id == *id && matches!(input.kind, crate::studies::InputKind::Source)
                });
                if let Some(input) = declared
                    && output(value)
                    && probe.set_input(id, value.clone()).is_err()
                {
                    *value = input.default.clone();
                }
            }
        }
        // A host study registered under a built-in's name never had the presentation an upgrade
        // gives that built-in's older templates either. The options profile reads the host's
        // option chain, so only a host registers it, and its own older templates are upgraded.
        let settings = if factory.is_host(&self.name) && self.name != "Options Volume Profile" {
            &self.settings
        } else {
            &template.settings
        };
        // A study whose outputs cannot follow another symbol or interval restores on the chart's
        // bars, without the inputs it saved for them.
        if !study.draws_on_other_bars() {
            inputs.retain(|(id, _)| !super::settings::CHART_INPUT_IDS[1..].contains(&id.as_str()));
        }
        // Studies saved before an input's range narrowed restore with the value they were saved
        // with, from the bottom of the range the input took then; only edits are bounded.
        const NARROWED: [(&str, &str); 8] = [
            ("BB", "std_dev"),
            ("BBTrend", "multiplier"),
            ("LinReg Slope", "period"),
            ("MFI", "period"),
            ("RCI", "smoothing_deviation"),
            ("SuperTrend", "multiplier"),
            ("Woodies CCI", "turbo_length"),
            ("Woodies CCI", "cci_length"),
        ];
        let saved = inputs
            .iter()
            .filter(|(id, value)| {
                NARROWED.contains(&(study.name(), id.as_str()))
                    && match value {
                        InputValue::Length(value) => *value >= 1,
                        InputValue::Number(value) => *value >= 0.0,
                        _ => false,
                    }
            })
            .cloned()
            .collect();
        crate::studies::inputs::restoring(saved, || {
            study.apply_settings(&inputs, settings.clone())
        })?;
        study.set_visible(template.visible);
        if let Some(scale) = template.scale {
            study.set_scale(scale);
        }
        Ok(study)
    }

    /// This template under the current input ids of a built-in study whose inputs changed after
    /// it was saved, with inputs added since set to draw what it drew, or `None` when it needs no
    /// change. Hours saved as minute counts, zones saved as text and whole numbers are converted
    /// for the inputs `own` declares as a time of day, a time zone or a decimal. `study` names
    /// the study being restored, which a host may register under another name. `own` holds the
    /// inputs of the study registered under the saved name, or the chart's study a stored pane is
    /// matched against; a saved input that study takes as saved is neither renamed nor converted.
    pub(crate) fn upgraded(&self, study: &str, own: &[StudyInput]) -> Option<Self> {
        let has = |id: &str| self.inputs.iter().any(|(input, _)| input == id);
        let declares = |id: &str| own.iter().any(|input| input.id == id);
        let out_of_range = |id: &str| {
            self.inputs.iter().any(|(saved, value)| {
                saved == id
                    && matches!(value, InputValue::Number(_))
                    && own
                        .iter()
                        .any(|input| input.id == id && !input.accepts(value))
            })
        };
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
        // Guides a study drew none of before it declared them stay hidden, and their band
        // unfilled; both keep the styles they take once shown.
        let hidden_levels = |settings: &mut StudySettings, declared: &dyn Indicator| {
            let (levels, _, color) = super::settings::declared_levels(declared);
            settings.levels = levels
                .into_iter()
                .map(|level| crate::studies::LevelStyle {
                    visible: false,
                    ..level
                })
                .collect();
            settings.fill_between_levels = false;
            settings.level_fill_color = color;
        };
        let hidden_fills = |declared: &dyn Indicator| {
            declared
                .fills()
                .iter()
                .map(|fill| crate::studies::FillStyle {
                    inverted_color: fill.inverted_color,
                    visible: false,
                    ..crate::studies::FillStyle::new(fill.color)
                })
                .collect()
        };
        let value = |id: &str| {
            self.inputs
                .iter()
                .find_map(|(input, value)| (input == id).then_some(value))
        };
        let shared_smoothing = declares("smoothing")
            && value("smooth1").is_some()
            && value("smooth1") == value("smooth2");
        // A saved Zig Zag deviation that the study declares but now refuses.
        let refused_deviation = value("deviation_percent").and_then(|saved| match saved {
            InputValue::Number(deviation)
                if own
                    .iter()
                    .any(|input| input.id == "deviation_percent" && !input.accepts(saved)) =>
            {
                Some(*deviation)
            }
            _ => None,
        });
        let mut template = self.clone();
        let mut changed = true;
        match study {
            // Older Knoxville saves drew only momentum and its confirmation marks, and saves
            // from before the window rule took pivot sides and the momentum pane's own scale.
            "Knoxville Divergence"
                if (declares("price_lines") && !has("price_lines"))
                    || ((has("left") || has("right")) && !declares("left")) =>
            {
                if declares("price_lines") && !has("price_lines") {
                    template
                        .inputs
                        .push(("price_lines".to_owned(), InputValue::Toggle(false)));
                }
                if !declares("left") {
                    template
                        .inputs
                        .retain(|(id, _)| !matches!(id.as_str(), "left" | "right"));
                    // Marks at prices must share their pane's price scale.
                    if template.scale == Some(StudyScale::Independent) {
                        template.scale = None;
                    }
                }
            }
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
            // `period` also set the ADX smoothing before the smoothing had an input of its
            // own. The last `period` saved is the one the study applied.
            "ADX" | "DMI" if has("period") && !has("adx_smoothing") => {
                let period = self.inputs.iter().rev().find(|(id, _)| id == "period");
                template
                    .inputs
                    .extend(period.map(|(_, value)| ("adx_smoothing".to_owned(), value.clone())));
            }
            // `period` also set the ATR period before that had an input of its own.
            "Chandelier" if has("period") && !has("atr_period") => {
                let period = self.inputs.iter().rev().find(|(id, _)| id == "period");
                template
                    .inputs
                    .extend(period.map(|(_, value)| ("atr_period".to_owned(), value.clone())));
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
            // A Moving Average Channel saved before band styles were saved fills its band as it
            // opened then, fainter than it opens now.
            "MA Channel" if self.settings.fills.is_empty() => {
                template.settings.fills = vec![crate::studies::FillStyle::new(
                    crate::studies::palette::LIGHT_BLUE.gamma_multiply_u8(13),
                )];
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
            // Every TPO profile drew its midpoint before it had a setting, and shaded its volume
            // rows by the TPO value area before the volume profile had settings.
            "Time Price Opportunity" if !has("midpoint") || !has("volume_tpo_value_area") => {
                if !has("midpoint") {
                    template
                        .inputs
                        .push(("midpoint".to_owned(), InputValue::Toggle(true)));
                }
                if !has("volume_tpo_value_area") {
                    template
                        .inputs
                        .push(("volume_tpo_value_area".to_owned(), InputValue::Toggle(true)));
                }
            }
            // Missed pivots were swings before they could follow a period.
            "Missed Pivot Levels" if !has("period") => {
                template
                    .inputs
                    .push(("period".to_owned(), InputValue::Choice(0)));
            }
            // Higher-interval candles drew their saved interval before one could follow the chart's.
            "Higher Interval Candles" if !has("auto_interval") => {
                template
                    .inputs
                    .push(("auto_interval".to_owned(), InputValue::Toggle(false)));
            }
            // A majority rule drew the signed direction before it could report the percentage, and
            // no guide before it opened with one at 50; hidden, the guide leaves the pane as saved.
            "MajRule" if !has("mode") || self.settings.levels.is_empty() => {
                if !has("mode") {
                    template
                        .inputs
                        .push(("mode".to_owned(), InputValue::Choice(1)));
                }
                if self.settings.levels.is_empty() {
                    let declared = crate::studies::MajorityRule::default();
                    let (levels, ..) = super::settings::declared_levels(&declared);
                    template.settings.levels = levels
                        .into_iter()
                        .map(|level| crate::studies::LevelStyle {
                            visible: false,
                            ..level
                        })
                        .collect();
                }
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
                        price_line: false,
                    });
                }
            }
            // An SMI drew no guides and no shading before it declared them. It listed its two
            // smoothing lengths under ids of their own before an equal pair shared one input.
            "SMI"
                if self.settings.levels.is_empty()
                    || self.settings.gradients.is_empty()
                    || shared_smoothing =>
            {
                if shared_smoothing {
                    template.inputs.retain(|(id, _)| id != "smooth2");
                    for (id, _) in &mut template.inputs {
                        if id == "smooth1" {
                            *id = "smoothing".to_owned();
                        }
                    }
                }
                let declared = crate::studies::StochasticMomentumIndex::default();
                if self.settings.levels.is_empty() {
                    hidden_levels(&mut template.settings, &declared);
                }
                if self.settings.gradients.is_empty() {
                    template.settings.gradients = declared
                        .gradients()
                        .iter()
                        .map(|fill| crate::studies::GradientStyle {
                            colors: fill.colors,
                            visible: false,
                        })
                        .collect();
                }
            }
            // A Stochastic RSI and a Connors RSI drew no guides before they declared them.
            "StochRSI" if self.settings.levels.is_empty() => {
                hidden_levels(
                    &mut template.settings,
                    &crate::studies::StochasticRSI::default(),
                );
            }
            "Connors RSI" if self.settings.levels.is_empty() => {
                hidden_levels(
                    &mut template.settings,
                    &crate::studies::ConnorsRSI::default(),
                );
            }
            // A Stochastic saved before its guides had styles drew the two it declared then,
            // without a band; the middle guide is added hidden as for one saved with those two.
            "Stochastic" if self.settings.levels.is_empty() => {
                let declared = crate::studies::Stochastic::default();
                let (levels, _, color) = super::settings::declared_levels(&declared);
                template.settings.levels = levels.into_iter().take(2).collect();
                template.settings.fill_between_levels = false;
                template.settings.level_fill_color = color;
            }
            // A Donchian channel filled no band before it declared one; hidden, the band keeps the
            // colour it is filled in once shown.
            "Donchian" if self.settings.fills.is_empty() => {
                template.settings.fills = crate::studies::DonchianChannels::default()
                    .fills()
                    .iter()
                    .map(|fill| crate::studies::FillStyle {
                        visible: false,
                        ..crate::studies::FillStyle::new(fill.color)
                    })
                    .collect();
            }
            // Older oscillators keep their unshaded appearance until their new fills are shown.
            "Aroon Oscillator" if self.settings.fills.is_empty() => {
                template.settings.fills = hidden_fills(&crate::studies::AroonOscillator::default());
            }
            "%B" if self.settings.fills.is_empty() || out_of_range("multiplier") => {
                if self.settings.fills.is_empty() {
                    template.settings.fills = hidden_fills(&crate::studies::PercentB::default());
                }
                if out_of_range("multiplier") {
                    for (id, value) in &mut template.inputs {
                        if let ("multiplier", InputValue::Number(v)) = (id.as_str(), value) {
                            *v = v.clamp(0.001, 50.0);
                        }
                    }
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
                        visible: false,
                        ..crate::studies::FillStyle::new(
                            self.settings
                                .plots
                                .get(1 + 2 * band)
                                .map_or(egui::Color32::TRANSPARENT, |plot| {
                                    plot.color.gamma_multiply_u8(13)
                                }),
                        )
                    })
                    .collect();
            }
            // A CCI drew no average before it opened with one.
            "CCI" if !has("smoothing") => {
                template
                    .inputs
                    .push(("smoothing".to_owned(), InputValue::Choice(0)));
            }
            // A Rank Correlation had no average, and drew no levels, before it opened with an
            // average, three levels and the band between them filled.
            "RCI" if !has("smoothing") || self.settings.levels.is_empty() => {
                if !has("smoothing") {
                    template
                        .inputs
                        .push(("smoothing".to_owned(), InputValue::Choice(0)));
                }
                if self.settings.levels.is_empty() {
                    let declared = crate::studies::RankCorrelationIndex::default();
                    let (levels, ..) = super::settings::declared_levels(&declared);
                    template.settings.levels = levels
                        .into_iter()
                        .map(|level| crate::studies::LevelStyle {
                            visible: false,
                            ..level
                        })
                        .collect();
                }
            }
            // An auto anchored VWAP drew one line before it had bands; hidden, the bands and their
            // fill keep the styles they take once shown.
            "VWAP Auto Anchored" if !has("band_multiplier") && self.settings.plots.len() == 1 => {
                let declared = crate::studies::VwapAutoAnchored::new().settings();
                template.inputs.extend([
                    ("percentage_bands".to_owned(), InputValue::Toggle(false)),
                    ("band_multiplier".to_owned(), InputValue::Number(1.0)),
                ]);
                for mut plot in declared.plots.into_iter().skip(1) {
                    plot.visible = false;
                    template.settings.plots.push(plot);
                }
                template
                    .settings
                    .untagged_plots
                    .extend(declared.untagged_plots);
                template.settings.fills = declared.fills;
                for fill in &mut template.settings.fills {
                    fill.visible = false;
                }
            }
            // An Elliott Wave saved before its swings could be sized by price counted bars, showed
            // the developing wave and drew no projections or impulse drawings.
            "Elliott Wave" if !has("swing_mode") => {
                for (id, value) in [
                    ("swing_mode", InputValue::Choice(0)),
                    ("swing_move", InputValue::Number(5.0)),
                    ("show_developing", InputValue::Toggle(true)),
                    ("projections", InputValue::Toggle(false)),
                    ("as_drawings", InputValue::Toggle(false)),
                ] {
                    if !has(id) {
                        template.inputs.push((id.to_owned(), value));
                    }
                }
            }
            // Seasonality saved before its Positive colour input coloured rising months with its
            // Expected plot. One saved before placement inputs also counted from 1900, sat at the
            // lower right fitted to its cells and drew falling months in their original red.
            "Seasonality" if !has("positive_color") => {
                let positive = self
                    .settings
                    .plots
                    .first()
                    .map_or(crate::studies::palette::UP, |plot| plot.color);
                let mut added = vec![("positive_color", InputValue::Color(positive))];
                if !has("table_position") {
                    added.extend([
                        ("start_year", InputValue::Integer(1900)),
                        ("table_position", InputValue::Choice(8)),
                        ("table_width", InputValue::Number(0.0)),
                        ("table_height", InputValue::Number(0.0)),
                        (
                            "negative_color",
                            InputValue::Color(egui::Color32::from_rgb(242, 55, 69)),
                        ),
                    ]);
                }
                for (id, value) in added {
                    if !has(id) {
                        template.inputs.push((id.to_owned(), value));
                    }
                }
            }
            // Performance saved before it took inputs showed the chart symbol alone over the six
            // standard periods, fitted at the lower right, falling changes in the palette red.
            "Performance" if !has("periods") => {
                for (id, value) in [
                    ("symbols", InputValue::Text(String::new())),
                    (
                        "periods",
                        InputValue::Text("1W, 1M, 3M, 6M, YTD, 1Y".into()),
                    ),
                    (
                        "negative_color",
                        InputValue::Color(crate::studies::palette::DOWN),
                    ),
                    ("table_position", InputValue::Choice(8)),
                    ("table_width", InputValue::Number(0.0)),
                    ("table_height", InputValue::Number(0.0)),
                ] {
                    if !has(id) {
                        template.inputs.push((id.to_owned(), value));
                    }
                }
            }
            // Chart patterns saved before developing boundaries had their own plot drew them
            // in its default style.
            "Continuation Chart Patterns" | "Reversal Chart Patterns"
                if self.settings.plots.len() == 2 =>
            {
                template.settings.plots.push(
                    crate::studies::ChartPatterns::new(crate::studies::PatternFamily::Reversal)
                        .plot_style(2),
                );
            }
            // The unused navy plot colour in older saves still drew rising columns green.
            "Accelerator Oscillator"
                if self.settings.plot_variants.iter().all(Vec::is_empty)
                    && self.settings.plots.first().map(|plot| plot.color)
                        == Some(egui::Color32::from_rgb(0, 0, 128)) =>
            {
                template.settings.plots[0].color = crate::studies::AcceleratorOscillator::new()
                    .plot_style(0)
                    .color;
            }
            // Before state colours were editable every impulse used the saved plot's style.
            "Impulse"
                if self.settings.plot_variants.iter().all(Vec::is_empty)
                    && !self.settings.plots.is_empty() =>
            {
                let plot = self.settings.plots[0];
                template.settings.plot_variants = vec![
                    crate::studies::ElderImpulseSystem::default()
                        .plot_variants(0)
                        .into_iter()
                        .map(|mut variant| {
                            variant.color = plot.color;
                            variant.width = plot.width;
                            variant
                        })
                        .collect(),
                ];
            }
            // An options profile kept one Sides choice for every measure, coloured its bars with
            // its first two line colours and showed its readouts with their lines unticked in the
            // style tab, before each measure kept its own choice, the bars took colours of their
            // own and the readouts followed a switch and the style tab.
            "Options Volume Profile" if has("expiry") && !has("put_call") => {
                template.inputs = self
                    .inputs
                    .iter()
                    .flat_map(|(id, value)| match id.as_str() {
                        "sides" => ["volume_sides", "interest_sides", "gamma_sides"]
                            .map(|id| (id.to_owned(), value.clone()))
                            .to_vec(),
                        _ => vec![(id.clone(), value.clone())],
                    })
                    .collect();
                template
                    .inputs
                    .push(("put_call".to_owned(), InputValue::Toggle(true)));
                if let [calls, puts, _, _, _, _, _, _] = template.settings.plots.as_slice() {
                    for (id, color) in [
                        ("call_color", calls.color),
                        ("put_color", puts.color),
                        ("net_positive_color", calls.color),
                        ("net_negative_color", puts.color),
                    ] {
                        template
                            .inputs
                            .push((id.to_owned(), InputValue::Color(color)));
                    }
                }
                if let [ratio, _, net, _, _, _, _, _] = template.settings.plots.as_mut_slice() {
                    ratio.visible = true;
                    net.visible = true;
                }
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
            // Older ratings saved their configurable component lengths and switches.
            "Technical Ratings" if has("fast") && !declares("fast") => {
                const REMOVED: [&str; 17] = [
                    "fast",
                    "medium",
                    "slow",
                    "simple",
                    "exponential",
                    "rsi",
                    "rsi_period",
                    "stochastic",
                    "stochastic_period",
                    "cci",
                    "cci_period",
                    "macd",
                    "macd_fast",
                    "macd_slow",
                    "macd_signal",
                    "momentum",
                    "momentum_period",
                ];
                template
                    .inputs
                    .retain(|(id, _)| !REMOVED.contains(&id.as_str()));
                // Older panes showed all groups; now they show the rating selected by the basis.
                for plot in template.settings.plots.iter_mut().take(2) {
                    plot.visible = false;
                }
            }
            // Multi-Range Delta ranges included their largest size before they excluded their
            // limit. The smallest number above that size with one more decimal place, 9.1 for 9,
            // keeps each saved range counting the sizes it counted that are written to that
            // precision and stays short; where that rounds back to the size, the next larger
            // number keeps the range exact.
            "Multi-Range Delta"
                if !declares("max0") && ["max0", "max1", "max2"].into_iter().any(has) =>
            {
                for (id, value) in &mut template.inputs {
                    if let Some(index) = id.strip_prefix("max") {
                        *id = format!("limit{index}");
                        if let InputValue::Number(max) = value
                            && *max < f64::MAX
                        {
                            let text = max.to_string();
                            let point = if text.contains('.') { "" } else { "." };
                            *max = match format!("{text}{point}1").parse::<f64>() {
                                Ok(limit) if limit > *max => limit,
                                _ => max.next_up(),
                            };
                        }
                    }
                }
            }
            // Zig Zag took deviations from 0 up, and drew no labels and stopped at its last pivot
            // before either had a setting; the automatic drawings share its deviation.
            "ZigZag"
            | "Auto Trend Line"
            | "Auto Parallel Channel"
            | "Auto Fibonacci Retracement"
            | "Auto Fibonacci Extension"
            | "Auto Pitchfork"
                if refused_deviation.is_some()
                    || (study == "ZigZag" && !has("extend_to_last_bar")) =>
            {
                if let Some(deviation) = refused_deviation {
                    for (id, value) in &mut template.inputs {
                        if id == "deviation_percent" {
                            *value = InputValue::Number(deviation.clamp(
                                crate::studies::ZigZag::MIN_DEVIATION,
                                crate::studies::ZigZag::MAX_DEVIATION,
                            ));
                        }
                    }
                }
                if study == "ZigZag" && !has("extend_to_last_bar") {
                    for (id, value) in [
                        ("extend_to_last_bar", InputValue::Toggle(false)),
                        ("price_labels", InputValue::Toggle(false)),
                        ("change_labels", InputValue::Toggle(false)),
                        ("change_mode", InputValue::Choice(1)),
                        ("volume_labels", InputValue::Toggle(false)),
                    ] {
                        if !has(id) {
                            template.inputs.push((id.to_owned(), value));
                        }
                    }
                }
            }
            // The former Zig Zag construction's inputs and single plot give way to the
            // defaults and separate support/resistance styles of the breakout study.
            #[cfg(feature = "drawings")]
            "Auto Trend Line" if has("deviation_percent") && !declares("deviation_percent") => {
                const REMOVED: [&str; 5] = [
                    "swing_size",
                    "atr_multiple",
                    "atr_period",
                    "deviation_percent",
                    "depth",
                ];
                template
                    .inputs
                    .retain(|(id, _)| !REMOVED.contains(&id.as_str()));
                let declared = crate::studies::AutoTrendLines::new().settings();
                template.settings.plots = declared.plots;
                template.settings.plot_variants = declared.plot_variants;
                if self
                    .settings
                    .help_url
                    .as_deref()
                    .is_some_and(|url| url.ends_with("/struct.AutoDrawing.html"))
                {
                    template.settings.help_url = declared.help_url;
                }
            }
            // An earlier key migrates only when it still resolves to the renamed built-in.
            now if IndicatorFactory::renamed(&self.name) == Some(now) => {
                template.name = now.to_owned();
            }
            _ => changed = false,
        }
        // Hours saved as minute counts and zones saved as text, before inputs had a time of day
        // and a time zone, and whole numbers saved for inputs that now take decimals, in the
        // form `own` declares them now.
        for (id, value) in &mut template.inputs {
            if let Some(input) = own.iter().find(|input| input.id == *id) {
                let upgraded = super::inputs::upgrade_saved_value(&input.kind, value.clone());
                changed |= upgraded != *value;
                *value = upgraded;
            }
        }
        // Profile inputs added since a save, set to draw what it drew.
        if matches!(
            study,
            "Vol Profile VR"
                | "Vol Profile FR"
                | "Vol Profile Session"
                | "Vol Profile Session HD"
                | "Vol Profile Periodic"
                | "Vol Profile Anchored"
                | "Vol Profile Auto Anchored"
        ) && declares("show_rows")
        {
            if declares("whole_tick_rows") && !has("whole_tick_rows") {
                template
                    .inputs
                    .push(("whole_tick_rows".to_owned(), InputValue::Toggle(false)));
                changed = true;
            }
            if study == "Vol Profile Periodic" && !has("restart_yearly") {
                template
                    .inputs
                    .push(("restart_yearly".to_owned(), InputValue::Toggle(false)));
                changed = true;
            }
            if self.settings.plots.is_empty() {
                let declared = crate::studies::VolProfileSession::default();
                template.settings.plots = (0..3).map(|line| declared.plot_style(line)).collect();
                template.settings.untagged_plots.clear();
                changed = true;
            }
        }
        // Options profile inputs added since a save, set to draw what it drew: every strike
        // unless a limit was saved, and no current price line, whose style it lacks.
        if study == "Options Volume Profile" && declares("current_price") {
            for (id, value) in [
                ("strikes_around_price", InputValue::Length(0)),
                ("current_price", InputValue::Toggle(false)),
            ] {
                if !has(id) {
                    template.inputs.push((id.to_owned(), value));
                    changed = true;
                }
            }
            if template.settings.plots.len() == 8 {
                template.settings.plots.push(crate::studies::PlotStyle {
                    visible: true,
                    color: crate::studies::palette::GRAY,
                    width: 1.0,
                    pattern: crate::model::LineStyle::Dotted,
                    plot_type: crate::studies::PlotType::Line,
                });
                template.settings.untagged_plots.push(8);
                changed = true;
            }
        }
        // An options profile drew puts divided by calls in its puts colour before they had one
        // of their own.
        if study == "Options Volume Profile" && declares("ratio_color") && !has("ratio_color") {
            let puts = template
                .inputs
                .iter()
                .find_map(|(id, value)| (id == "put_color").then_some(value))
                .or_else(|| {
                    own.iter()
                        .find(|input| input.id == "put_color")
                        .map(|input| &input.default)
                })
                .cloned();
            if let Some(puts) = puts {
                template.inputs.push(("ratio_color".to_owned(), puts));
                changed = true;
            }
        }
        // An options profile drew a wall while its Call wall or Put wall input, off by default,
        // and its line in the style tab were both on, before the line alone switched it. A save
        // with neither the inputs nor the put/call switch, listed since walls were listed for
        // every measure, had its walls off.
        if study == "Options Volume Profile" && declares("expiry") && !declares("call_wall") {
            for (line, id) in [(3, "call_wall"), (4, "put_wall")] {
                let saved = self.inputs.iter().find_map(|(saved, value)| match value {
                    InputValue::Toggle(on) if saved == id => Some(*on),
                    _ => None,
                });
                if let Some(plot) = template.settings.plots.get_mut(line)
                    && plot.visible
                    && !saved.unwrap_or(has("put_call"))
                {
                    plot.visible = false;
                    changed = true;
                }
            }
            let listed = template.inputs.len();
            template
                .inputs
                .retain(|(id, _)| id != "call_wall" && id != "put_wall");
            changed |= template.inputs.len() != listed;
        }
        // A single candlestick pattern study saved while it drew the bullish, bearish and
        // neutral lines of All Candlestick Patterns, and took the trend rule whatever its rule
        // reads, keeps the style of its side's line and drops a rule it now ignores.
        if let Some(line) = crate::studies::CandlestickPatterns::pattern_line(study) {
            if has("trend") && !declares("trend") {
                template.inputs.retain(|(id, _)| id != "trend");
                changed = true;
            }
            if self.settings.plots.len() == 3 {
                let settings = &mut template.settings;
                settings.plots = vec![self.settings.plots[line]];
                settings.plot_variants = vec![
                    self.settings
                        .plot_variants
                        .get(line)
                        .cloned()
                        .unwrap_or_default(),
                ];
                for flagged in [&mut settings.untagged_plots, &mut settings.price_line_plots] {
                    *flagged = if flagged.contains(&line) {
                        vec![0]
                    } else {
                        Vec::new()
                    };
                }
                changed = true;
            }
        }
        changed.then_some(template)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, PriceSource, SeriesScale, Session, SessionWindow, TimezoneId};
    use crate::studies::{
        AverageLine, AverageType, CVD, CloseToCloseVol, EhlersFisher, HL2, HLC3, MACombination,
        MARibbon, PriceChannel, RSI, RankCorrelationIndex, TradingSessions, VolumeWeightedAvgPrice,
        WilliamsR,
    };
    use chrono::{NaiveTime, TimeZone, Utc, Weekday};
    use egui::Color32;

    /// Missing permissions inherit the study's declaration; explicit permissions survive saves.
    #[test]
    fn legacy_settings_keep_a_read_only_host_study() {
        #[derive(Clone, Default)]
        struct Locked(crate::studies::IndicatorValues);
        impl Indicator for Locked {
            fn name(&self) -> &str {
                "Locked"
            }
            fn user_edit_enabled(&self) -> bool {
                false
            }
            fn calculate(&mut self, _: &[Bar]) {}
            fn values(&self) -> &crate::studies::IndicatorValues {
                &self.0
            }
            fn colors(&self) -> Vec<Color32> {
                vec![Color32::RED]
            }
            fn set_colors(&mut self, _: Vec<Color32>) {}
            fn set_visible(&mut self, _: bool) {}
            fn clone_box(&self) -> Box<dyn Indicator> {
                Box::new(self.clone())
            }
        }
        let mut factory = IndicatorFactory::new();
        factory.register("Locked", || Box::<Locked>::default());
        for (name, declared) in [("Locked", false), ("SMA", true)] {
            for saved in [None, Some(true), Some(false)] {
                let study = factory.create(name).unwrap();
                let mut json =
                    serde_json::to_value(StudyTemplate::from_study(study.as_ref())).unwrap();
                if let Some(enabled) = saved {
                    json["settings"]["user_edit_enabled"] = enabled.into();
                } else {
                    json["settings"]
                        .as_object_mut()
                        .unwrap()
                        .remove("user_edit_enabled");
                }
                let template: StudyTemplate = serde_json::from_value(json).unwrap();
                let restored = template.create_with(&factory).unwrap();
                let expected = saved.unwrap_or(declared);
                assert_eq!(restored.user_edit_enabled(), expected, "{name} {saved:?}");
                let captured = StudyTemplate::from_study(restored.as_ref());
                let json = serde_json::to_value(&captured).unwrap();
                assert_eq!(json["settings"]["user_edit_enabled"], expected);
                assert_eq!(
                    captured.create_with(&factory).unwrap().user_edit_enabled(),
                    expected
                );
            }
        }
    }

    /// A study the chart wraps saves the switch its own `user_visible` reports, not whether its
    /// other rule hides it.
    #[test]
    fn templates_save_the_user_switch_of_a_wrapped_study() {
        #[derive(Clone, Default)]
        struct Muted(crate::studies::IndicatorValues);
        impl Indicator for Muted {
            fn name(&self) -> &str {
                "Muted"
            }
            fn calculate(&mut self, _: &[Bar]) {}
            fn values(&self) -> &crate::studies::IndicatorValues {
                &self.0
            }
            fn colors(&self) -> Vec<Color32> {
                vec![Color32::RED]
            }
            fn set_colors(&mut self, _: Vec<Color32>) {}
            fn is_visible(&self) -> bool {
                false
            }
            fn set_visible(&mut self, _: bool) {}
            fn user_visible(&self) -> bool {
                true
            }
            fn clone_box(&self) -> Box<dyn Indicator> {
                Box::new(self.clone())
            }
        }
        let study = ConfiguredStudy::new(Box::new(Muted::default()));
        assert!(StudyTemplate::from_study(&study).visible);
    }

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
            Box::new(HL2::default()),
            Box::new(HLC3::default()),
            Box::new(EhlersFisher::default()),
            // A catalogue study whose saved copies were written while it was outside the catalogue.
            Box::new(PriceChannel::default()),
            // A catalogue study with the bars per year it is annualized over.
            Box::new(CloseToCloseVol::default().with_trading_days(365.0)),
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
        let mut tpo = crate::studies::TimePriceOpportunity::default();
        // A literal save from before these controls keeps its palette, levels and boundaries.
        let legacy: StudyTemplate = serde_json::from_str(r#"{"name":"Time Price Opportunity","inputs":[["period",{"Length":30}],["row_ticks",{"Length":0}],["value_area",{"Number":70.0}],["split_sessions",{"Toggle":false}],["single_prints",{"Toggle":false}],["midpoint",{"Toggle":false}],["statistics",{"Toggle":false}],["display",{"Choice":0}],["sessions",{"Choice":0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TimePriceOpportunity.html","plots":[],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[16,16,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#).unwrap();
        let restored = legacy.create().unwrap();
        assert_eq!(restored.settings(), legacy.settings);
        let before = crate::model::TPOConfig {
            tick_size: 0.0,
            period_minutes: 30,
            initial_balance_minutes: 60,
            value_area_pct: 0.70,
            display_mode: crate::model::TPODisplayMode::Letters,
            split_periods: false,
            color_mode: crate::model::TPOColorMode::ByPeriod,
            period_colors: [
                Color32::from_rgb(214, 56, 100),
                Color32::from_rgb(90, 197, 97),
                Color32::from_rgb(84, 185, 209),
                Color32::from_rgb(93, 36, 245),
            ],
            solid_color: Color32::from_rgb(189, 189, 189),
            letter_color: None,
            outside_value_area_opacity: 0.45,
            show_poc: true,
            poc_color: None,
            show_value_area: true,
            show_value_area_high: true,
            show_value_area_low: true,
            show_midpoint: false,
            show_initial_balance: true,
            show_initial_balance_extensions: false,
            show_poor_high: false,
            show_poor_low: false,
            extend_poor_extremes: false,
            extend_poc: false,
            extend_value_area_high: false,
            extend_value_area_low: false,
            extend_single_prints: false,
            show_single_prints: false,
            show_statistics: false,
            statistics_rows: crate::model::TPOStatisticsRows::default(),
            show_opening_range: true,
            labels: crate::model::TPOLabels::default(),
            show_closing_price: false,
            show_volume_profile: false,
            volume_profile: crate::model::TPOVolumeProfile::saved_without_settings(),
            split_sessions: false,
            session: crate::model::ProfileSession::Chart,
            regular_hours: None,
            profile_period: crate::model::TPOProfilePeriod::Session,
            profile_periods: 1,
            custom_letters: None,
            composites: Vec::new(),
            splits: Vec::new(),
            latest_session_only: false,
            instrument_tick: None,
        };
        assert_eq!(
            format!("{:?}", restored.tpo()),
            format!("{:?}", Some(&before))
        );
        for (id, value) in [
            ("split_periods", InputValue::Toggle(true)),
            ("coloring", InputValue::Choice(1)),
            ("period_color_1", InputValue::Color(Color32::RED)),
            ("period_color_2", InputValue::Color(Color32::GREEN)),
            ("period_color_3", InputValue::Color(Color32::BLUE)),
            ("period_color_4", InputValue::Color(Color32::YELLOW)),
            ("block_color", InputValue::Color(Color32::GOLD)),
            ("outside_value_area_opacity", InputValue::Length(80)),
            ("poc", InputValue::Toggle(false)),
            ("value_area_levels", InputValue::Toggle(false)),
            ("initial_balance", InputValue::Toggle(false)),
            ("initial_balance_minutes", InputValue::Length(30)),
            ("initial_balance_extensions", InputValue::Toggle(true)),
            ("open_price", InputValue::Toggle(false)),
            ("close_price", InputValue::Toggle(true)),
            ("poor_high", InputValue::Toggle(true)),
            ("poor_low", InputValue::Toggle(true)),
            ("extend_poor", InputValue::Toggle(true)),
            ("extend_poc", InputValue::Toggle(true)),
            ("extend_vah", InputValue::Toggle(true)),
            ("extend_val", InputValue::Toggle(true)),
            ("extend_single_prints", InputValue::Toggle(true)),
            ("volume_profile", InputValue::Toggle(true)),
            ("profile_period", InputValue::Choice(2)),
            ("profile_periods", InputValue::Length(3)),
            ("value_area_high", InputValue::Toggle(false)),
            ("value_area_low", InputValue::Toggle(false)),
            ("letters_color", InputValue::Color(Color32::RED)),
            ("poc_color", InputValue::Color(Color32::GREEN)),
            ("volume_poc", InputValue::Toggle(true)),
            ("volume_poc_color", InputValue::Color(Color32::BLUE)),
            ("volume_poc_style", InputValue::Choice(1)),
            ("volume_value_area_high", InputValue::Toggle(true)),
            (
                "volume_value_area_high_color",
                InputValue::Color(Color32::KHAKI),
            ),
            ("volume_value_area_high_style", InputValue::Choice(2)),
            ("volume_value_area_low", InputValue::Toggle(true)),
            (
                "volume_value_area_low_color",
                InputValue::Color(Color32::BROWN),
            ),
            ("volume_value_area_low_style", InputValue::Choice(1)),
            ("volume_values", InputValue::Toggle(true)),
            ("volume_values_color", InputValue::Color(Color32::GRAY)),
            ("volume_profile_color", InputValue::Color(Color32::DARK_RED)),
            (
                "volume_value_area_color",
                InputValue::Color(Color32::DARK_BLUE),
            ),
            ("volume_placement", InputValue::Choice(0)),
            ("volume_tpo_value_area", InputValue::Toggle(true)),
            ("statistics_high_low_range", InputValue::Toggle(true)),
            ("statistics_poc", InputValue::Toggle(false)),
            ("statistics_value_area_high", InputValue::Toggle(false)),
            ("statistics_value_area_low", InputValue::Toggle(true)),
            ("statistics_value_area_range", InputValue::Toggle(true)),
            ("statistics_initial_balance_high", InputValue::Toggle(true)),
            ("statistics_initial_balance_low", InputValue::Toggle(true)),
            (
                "statistics_initial_balance_range",
                InputValue::Toggle(false),
            ),
            ("statistics_tpos_above", InputValue::Toggle(true)),
            ("statistics_tpos_below", InputValue::Toggle(false)),
            ("statistics_total_tpos", InputValue::Toggle(true)),
            ("statistics_total_volume", InputValue::Toggle(true)),
            ("statistics_rotation_factor", InputValue::Toggle(false)),
            ("statistics_range_extension", InputValue::Toggle(false)),
            // Merges and splits are saved after the period and row size they were made for.
            (
                "composites",
                InputValue::Text("30,0|1970-01-01T00:00:00Z/1970-01-02T00:00:00Z".into()),
            ),
            (
                "splits",
                InputValue::Text("30,0|1970-01-02T01:00:00Z".into()),
            ),
        ] {
            tpo.set_input(id, value.clone()).unwrap();
            assert_eq!(
                tpo.inputs()
                    .iter()
                    .find(|input| input.id == id)
                    .unwrap()
                    .value,
                value,
                "{id}"
            );
        }
        studies.push(ConfiguredStudy::new(Box::new(tpo)));
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
                    if study.tpo().is_some() {
                        assert_eq!(
                            format!("{:?}", restored.tpo()),
                            format!("{:?}", study.tpo())
                        );
                    }
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
                // A literal save from before drawings could keep their own style restores its
                // settings and the input ids that identify its study pane unchanged.
                (
                    serde_json::from_str(r#"{"name":"Auto Parallel Channel","inputs":[["swing_size",{"Choice":1}],["atr_multiple",{"Number":3.0}],["atr_period",{"Length":10}],["deviation_percent",{"Number":5.0}],["depth",{"Length":3}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.AutoDrawing.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#).unwrap(),
                    &[
                        ("swing_size", InputValue::Choice(1)),
                        ("deviation_percent", InputValue::Number(5.0)),
                        ("depth", InputValue::Length(3)),
                    ][..],
                ),
                (
                    before_atr,
                    &[edited[0].clone(), edited[3].clone(), edited[4].clone()][..],
                ),
                // A literal extension save from before swings could be found by bars keeps the
                // ATR multiple it was saved with.
                (
                    serde_json::from_str(r#"{"name":"Auto Fibonacci Extension","inputs":[["swing_size",{"Choice":0}],["atr_multiple",{"Number":3.0}],["atr_period",{"Length":10}],["deviation_percent",{"Number":5.0}],["depth",{"Length":10}],["fib_show_prices",{"Choice":0}],["fib_show_levels",{"Choice":0}],["fib_levels_as_percent",{"Toggle":false}],["fib_labels_on_left",{"Toggle":true}],["fib_reverse",{"Toggle":false}],["fib_extend_left",{"Toggle":false}],["fib_extend_right",{"Toggle":true}],["fib_background",{"Toggle":true}],["fib_background_opacity",{"Length":51}],["fib_logarithmic",{"Toggle":false}],["fib_level_count",{"Length":24}],["fib_visible_0",{"Toggle":true}],["fib_ratio_0",{"Number":0.0}],["fib_label_0",{"Text":""}],["fib_color_0",{"Color":[128,128,128,255]}],["fib_price_0",{"Toggle":true}],["fib_visible_1",{"Toggle":true}],["fib_ratio_1",{"Number":0.23600000143051147}],["fib_label_1",{"Text":""}],["fib_color_1",{"Color":[242,54,69,255]}],["fib_price_1",{"Toggle":true}],["fib_visible_2",{"Toggle":true}],["fib_ratio_2",{"Number":0.38199999928474426}],["fib_label_2",{"Text":""}],["fib_color_2",{"Color":[255,152,0,255]}],["fib_price_2",{"Toggle":true}],["fib_visible_3",{"Toggle":true}],["fib_ratio_3",{"Number":0.5}],["fib_label_3",{"Text":""}],["fib_color_3",{"Color":[76,175,80,255]}],["fib_price_3",{"Toggle":true}],["fib_visible_4",{"Toggle":true}],["fib_ratio_4",{"Number":0.6179999709129333}],["fib_label_4",{"Text":""}],["fib_color_4",{"Color":[8,153,129,255]}],["fib_price_4",{"Toggle":true}],["fib_visible_5",{"Toggle":true}],["fib_ratio_5",{"Number":0.7860000133514404}],["fib_label_5",{"Text":""}],["fib_color_5",{"Color":[0,188,212,255]}],["fib_price_5",{"Toggle":true}],["fib_visible_6",{"Toggle":true}],["fib_ratio_6",{"Number":1.0}],["fib_label_6",{"Text":""}],["fib_color_6",{"Color":[128,128,128,255]}],["fib_price_6",{"Toggle":true}],["fib_visible_7",{"Toggle":true}],["fib_ratio_7",{"Number":1.6180000305175781}],["fib_label_7",{"Text":""}],["fib_color_7",{"Color":[41,98,255,255]}],["fib_price_7",{"Toggle":true}],["fib_visible_8",{"Toggle":true}],["fib_ratio_8",{"Number":2.618000030517578}],["fib_label_8",{"Text":""}],["fib_color_8",{"Color":[242,54,69,255]}],["fib_price_8",{"Toggle":true}],["fib_visible_9",{"Toggle":true}],["fib_ratio_9",{"Number":3.618000030517578}],["fib_label_9",{"Text":""}],["fib_color_9",{"Color":[156,39,176,255]}],["fib_price_9",{"Toggle":true}],["fib_visible_10",{"Toggle":true}],["fib_ratio_10",{"Number":4.236000061035156}],["fib_label_10",{"Text":""}],["fib_color_10",{"Color":[233,30,99,255]}],["fib_price_10",{"Toggle":true}],["fib_visible_11",{"Toggle":false}],["fib_ratio_11",{"Number":1.2719999551773071}],["fib_label_11",{"Text":""}],["fib_color_11",{"Color":[255,152,0,255]}],["fib_price_11",{"Toggle":true}],["fib_visible_12",{"Toggle":false}],["fib_ratio_12",{"Number":1.4140000343322754}],["fib_label_12",{"Text":""}],["fib_color_12",{"Color":[242,54,69,255]}],["fib_price_12",{"Toggle":true}],["fib_visible_13",{"Toggle":false}],["fib_ratio_13",{"Number":2.2720000743865967}],["fib_label_13",{"Text":""}],["fib_color_13",{"Color":[255,152,0,255]}],["fib_price_13",{"Toggle":true}],["fib_visible_14",{"Toggle":false}],["fib_ratio_14",{"Number":2.4140000343322754}],["fib_label_14",{"Text":""}],["fib_color_14",{"Color":[76,175,80,255]}],["fib_price_14",{"Toggle":true}],["fib_visible_15",{"Toggle":false}],["fib_ratio_15",{"Number":2.0}],["fib_label_15",{"Text":""}],["fib_color_15",{"Color":[8,153,129,255]}],["fib_price_15",{"Toggle":true}],["fib_visible_16",{"Toggle":false}],["fib_ratio_16",{"Number":3.0}],["fib_label_16",{"Text":""}],["fib_color_16",{"Color":[0,188,212,255]}],["fib_price_16",{"Toggle":true}],["fib_visible_17",{"Toggle":false}],["fib_ratio_17",{"Number":3.2720000743865967}],["fib_label_17",{"Text":""}],["fib_color_17",{"Color":[128,128,128,255]}],["fib_price_17",{"Toggle":true}],["fib_visible_18",{"Toggle":false}],["fib_ratio_18",{"Number":3.4140000343322754}],["fib_label_18",{"Text":""}],["fib_color_18",{"Color":[41,98,255,255]}],["fib_price_18",{"Toggle":true}],["fib_visible_19",{"Toggle":false}],["fib_ratio_19",{"Number":4.0}],["fib_label_19",{"Text":""}],["fib_color_19",{"Color":[242,54,69,255]}],["fib_price_19",{"Toggle":true}],["fib_visible_20",{"Toggle":false}],["fib_ratio_20",{"Number":4.271999835968018}],["fib_label_20",{"Text":""}],["fib_color_20",{"Color":[156,39,176,255]}],["fib_price_20",{"Toggle":true}],["fib_visible_21",{"Toggle":false}],["fib_ratio_21",{"Number":4.414000034332275}],["fib_label_21",{"Text":""}],["fib_color_21",{"Color":[233,30,99,255]}],["fib_price_21",{"Toggle":true}],["fib_visible_22",{"Toggle":false}],["fib_ratio_22",{"Number":4.618000030517578}],["fib_label_22",{"Text":""}],["fib_color_22",{"Color":[255,152,0,255]}],["fib_price_22",{"Toggle":true}],["fib_visible_23",{"Toggle":false}],["fib_ratio_23",{"Number":4.763999938964844}],["fib_label_23",{"Text":""}],["fib_color_23",{"Color":[8,153,129,255]}],["fib_price_23",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.AutoDrawing.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#).unwrap(),
                    &[
                        ("swing_size", InputValue::Choice(0)),
                        ("atr_multiple", InputValue::Number(3.0)),
                        ("depth", InputValue::Length(10)),
                    ][..],
                ),
            ] {
                let restored = template.create().unwrap();
                assert_eq!(restored.settings(), template.settings);
                let inputs = restored.inputs();
                for (id, value) in expected {
                    let restored = inputs.iter().find(|input| input.id == *id).unwrap();
                    assert_eq!(&restored.value, value, "{id}");
                }
            }
        }
        #[cfg(feature = "drawings")]
        {
            // The literal single-line save on origin/main upgrades to the breakout study.
            let saved: StudyTemplate = serde_json::from_str(r#"{"name":"Auto Trend Line","inputs":[["swing_size",{"Choice":1}],["atr_multiple",{"Number":3.0}],["atr_period",{"Length":10}],["deviation_percent",{"Number":5.0}],["depth",{"Length":3}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.AutoDrawing.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#).unwrap();
            let restored = saved.create().unwrap();
            let defaults = crate::studies::AutoTrendLines::new().settings();
            assert_eq!(restored.settings().plots, defaults.plots);
            assert_eq!(restored.settings().help_url, defaults.help_url);
            let inputs = restored.inputs();
            for (id, expected) in [
                ("bars_to_breakout", InputValue::Length(3)),
                ("line_size", InputValue::Choice(2)),
                ("show_pivots", InputValue::Toggle(false)),
            ] {
                assert_eq!(
                    inputs.iter().find(|input| input.id == id).unwrap().value,
                    expected
                );
            }
            for id in [
                "swing_size",
                "atr_multiple",
                "atr_period",
                "deviation_percent",
                "depth",
            ] {
                assert!(inputs.iter().all(|input| input.id != id));
            }
        }
        // Literal two-plot saves from before developing boundaries had their own style keep
        // their input ids and styles, with the third plot in the style they already drew.
        for saved in [
            r#"{"name":"Continuation Chart Patterns","inputs":[["strength",{"Length":3}],["min_length",{"Length":8}],["max_length",{"Length":120}],["tolerance",{"Number":1.0}],["touches",{"Length":2}],["pole",{"Number":5.0}],["patterns[0]",{"Toggle":true}],["patterns[1]",{"Toggle":true}],["patterns[2]",{"Toggle":true}],["patterns[3]",{"Toggle":true}],["patterns[4]",{"Toggle":true}],["patterns[5]",{"Toggle":true}],["patterns[6]",{"Toggle":true}],["patterns[7]",{"Toggle":true}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ChartPatterns.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleUp","visible":false},{"color":[200,0,120,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleDown","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
            r#"{"name":"Reversal Chart Patterns","inputs":[["strength",{"Length":3}],["min_length",{"Length":10}],["max_length",{"Length":200}],["tolerance",{"Number":3.0}],["depth",{"Number":3.0}],["handle",{"Number":50.0}],["patterns[0]",{"Toggle":true}],["patterns[1]",{"Toggle":true}],["patterns[2]",{"Toggle":true}],["patterns[3]",{"Toggle":true}],["patterns[4]",{"Toggle":true}],["patterns[5]",{"Toggle":true}],["patterns[6]",{"Toggle":true}],["patterns[7]",{"Toggle":true}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ChartPatterns.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleUp","visible":false},{"color":[200,0,120,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleDown","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
        ] {
            let saved: StudyTemplate = serde_json::from_str(saved).unwrap();
            let restored = saved.create().unwrap();
            let mut expected = saved.settings.clone();
            expected.plots.push(crate::studies::PlotStyle {
                color: Color32::from_rgb(120, 123, 134),
                width: 1.0,
                pattern: crate::model::LineStyle::Dashed,
                plot_type: crate::studies::PlotType::Line,
                visible: true,
            });
            expected.plot_variants.push(Vec::new());
            assert_eq!(restored.settings(), expected, "{}", saved.name);
            let inputs = restored.inputs();
            if saved.name == "Missed Pivot Levels" {
                assert_eq!(
                    inputs
                        .iter()
                        .find(|input| input.id == "period")
                        .unwrap()
                        .value,
                    InputValue::Choice(0)
                );
                assert_eq!(Some(restored.scale()), saved.scale);
            }
            for (id, value) in &saved.inputs {
                // The saved calculation controls no longer apply to the pattern's drawings.
                if matches!(
                    id.as_str(),
                    "calculation_symbol" | "calculation_timeframe" | "wait_for_close"
                ) {
                    assert!(
                        !inputs.iter().any(|input| input.id == *id),
                        "{} {id}",
                        saved.name
                    );
                    continue;
                }
                assert_eq!(
                    inputs
                        .iter()
                        .find(|input| input.id == *id)
                        .map(|input| &input.value),
                    Some(value),
                    "{} {id}",
                    saved.name
                );
            }
        }
    }

    /// Main templates without Symbol or Offset keep every saved input, style and value.
    /// ZigZag and Trading Sessions, whose labels cannot follow another symbol or interval,
    /// restore without those inputs.
    #[test]
    fn templates_saved_before_symbol_and_offset_restore_unchanged() {
        let factory = IndicatorFactory::new();
        // Two bars per session over more than 52 weeks warm up every saved study.
        let bars: Vec<_> = (0..800)
            .map(|i: i64| {
                let x = i as f64;
                let close = 100.0
                    + (x * 0.13).sin() * 8.0
                    + (x * 0.029).cos() * 5.0
                    + (i / 160) as f64 * 6.0;
                let open = close - (x * 0.7).sin() * 1.5;
                crate::model::Bar::new(
                    chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::hours(i / 2 * 48 + i % 2),
                    open,
                    close.max(open) + 0.5 + (x * 0.3).sin().abs(),
                    close.min(open) - 0.5 - (x * 0.5).cos().abs(),
                    close,
                    1_000.0 + ((i * 37) % 211) as f64 * 9.0,
                )
            })
            .collect();
        // Literal saves captured on origin/main 1b270ce51, each with one edited input.
        for text in [
            r#"{"name":"PSAR","inputs":[["initial_af",{"Number":0.03}],["af_step",{"Number":0.02}],["max_af",{"Number":0.2}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ParabolicSAR.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Crosses","visible":true},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Crosses","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
            r#"{"name":"MA Cross","inputs":[["cross_first",{"Choice":0}],["cross_second",{"Choice":2}],["period_0",{"Length":9}],["kind_0",{"Choice":0}],["source_0",{"Choice":3}],["period_1",{"Length":21}],["kind_1",{"Choice":0}],["source_1",{"Choice":3}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.MACross.html","plots":[{"color":[255,109,0,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[67,160,71,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[16,7,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
            r#"{"name":"VWAP Auto Anchored","inputs":[["anchor",{"Choice":0}],["length",{"Length":5}],["source",{"Choice":5}],["percentage_bands",{"Toggle":false}],["band_multiplier",{"Number":1.0}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VwapAutoAnchored.html","plots":[{"color":[30,136,229,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[128,128,0,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[128,128,0,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[0,137,123,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[0,137,123,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[2,9,14,16],"untagged_plots":[3,4,5,6],"price_line_plots":[],"plot_variants":[[],[],[],[],[],[],[]],"background":null,"background_visible":true,"fills":[{"color":[4,9,4,13],"inverted_color":null,"visible":true,"independent":true}],"gradients":[]},"visible":true,"scale":"Main"}"#,
            r#"{"name":"Multi MA","inputs":[["cross_first",{"Choice":0}],["cross_second",{"Choice":0}],["period_0",{"Length":10}],["kind_0",{"Choice":0}],["source_0",{"Choice":3}],["period_1",{"Length":50}],["kind_1",{"Choice":0}],["source_1",{"Choice":3}],["period_2",{"Length":100}],["kind_2",{"Choice":0}],["source_2",{"Choice":3}],["period_3",{"Length":200}],["kind_3",{"Choice":0}],["source_3",{"Choice":3}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.MultipleMA.html","plots":[{"color":[246,195,9,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[251,152,0,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[251,101,0,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[246,12,12,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[15,12,1,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
            r#"{"name":"ZigZag","inputs":[["deviation_percent",{"Number":5.0}],["depth",{"Length":11}],["mode",{"Choice":0}],["price_labels",{"Toggle":false}],["change_labels",{"Toggle":false}],["change_mode",{"Choice":1}],["volume_labels",{"Toggle":false}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ZigZag.html","plots":[{"color":[41,98,255,255],"width":2.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
            #[cfg(feature = "timezones")]
            r#"{"name":"Trading Sessions","inputs":[["show [0]",{"Toggle":false}],["name [0]",{"Text":"Tokyo"}],["start [0]",{"Length":540}],["end [0]",{"Length":900}],["zone [0]",{"Text":"Asia/Tokyo"}],["show [1]",{"Toggle":true}],["name [1]",{"Text":"London"}],["start [1]",{"Length":510}],["end [1]",{"Length":990}],["zone [1]",{"Text":"Europe/London"}],["show [2]",{"Toggle":true}],["name [2]",{"Text":"New York"}],["start [2]",{"Length":570}],["end [2]",{"Length":960}],["zone [2]",{"Text":"America/New_York"}],["boxes",{"Toggle":true}],["name_labels",{"Toggle":true}],["range_labels",{"Toggle":false}],["average_labels",{"Toggle":false}],["open_lines",{"Toggle":false}],["close_lines",{"Toggle":false}],["average_lines",{"Toggle":false}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradingSessions.html","plots":[{"color":[6,15,38,38],"width":1.0,"pattern":"Solid","plot_type":"BackgroundColor","visible":true},{"color":[38,23,0,38],"width":1.0,"pattern":"Solid","plot_type":"BackgroundColor","visible":true},{"color":[1,23,19,38],"width":1.0,"pattern":"Solid","plot_type":"BackgroundColor","visible":true}],"precision":0,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[0,1,2,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
            #[cfg(not(feature = "timezones"))]
            r#"{"name":"Trading Sessions","inputs":[["show [0]",{"Toggle":false}],["name [0]",{"Text":"Tokyo"}],["start [0]",{"Length":540}],["end [0]",{"Length":900}],["zone [0]",{"Text":"+09:00"}],["show [1]",{"Toggle":true}],["name [1]",{"Text":"London"}],["start [1]",{"Length":510}],["end [1]",{"Length":990}],["zone [1]",{"Text":"+00:00"}],["show [2]",{"Toggle":true}],["name [2]",{"Text":"New York"}],["start [2]",{"Length":570}],["end [2]",{"Length":960}],["zone [2]",{"Text":"-05:00"}],["boxes",{"Toggle":true}],["name_labels",{"Toggle":true}],["range_labels",{"Toggle":false}],["average_labels",{"Toggle":false}],["open_lines",{"Toggle":false}],["close_lines",{"Toggle":false}],["average_lines",{"Toggle":false}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradingSessions.html","plots":[{"color":[6,15,38,38],"width":1.0,"pattern":"Solid","plot_type":"BackgroundColor","visible":true},{"color":[38,23,0,38],"width":1.0,"pattern":"Solid","plot_type":"BackgroundColor","visible":true},{"color":[1,23,19,38],"width":1.0,"pattern":"Solid","plot_type":"BackgroundColor","visible":true}],"precision":0,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[0,1,2,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
        ] {
            let saved: StudyTemplate = serde_json::from_str(text).unwrap();
            let mut restored = saved.create().unwrap();
            let inputs = restored.inputs();
            let follows = !matches!(saved.name.as_str(), "ZigZag" | "Trading Sessions");
            for (id, value) in &saved.inputs {
                if !follows
                    && matches!(
                        id.as_str(),
                        "calculation_symbol" | "calculation_timeframe" | "wait_for_close"
                    )
                {
                    assert!(
                        !inputs.iter().any(|input| input.id == *id),
                        "{} {id}",
                        saved.name
                    );
                    continue;
                }
                let expected = if saved.name == "Trading Sessions" {
                    let clock = |hour, minute| {
                        InputValue::TimeOfDay(
                            chrono::NaiveTime::from_hms_opt(hour, minute, 0).unwrap(),
                        )
                    };
                    match id.as_str() {
                        "start [0]" => clock(9, 0),
                        "end [0]" => clock(15, 0),
                        "start [1]" => clock(8, 30),
                        "end [1]" => clock(16, 30),
                        "start [2]" => clock(9, 30),
                        "end [2]" => clock(16, 0),
                        "zone [0]" | "zone [1]" | "zone [2]" => {
                            let InputValue::Text(zone) = value else {
                                panic!("a legacy session zone is text");
                            };
                            InputValue::TimeZone(zone.clone())
                        }
                        _ => value.clone(),
                    }
                } else {
                    value.clone()
                };
                assert_eq!(
                    inputs
                        .iter()
                        .find(|input| input.id == *id)
                        .map(|input| &input.value),
                    Some(&expected),
                    "{} {id}",
                    saved.name
                );
            }
            if follows {
                assert_eq!(
                    inputs
                        .iter()
                        .find(|input| input.id == "calculation_symbol")
                        .unwrap()
                        .value,
                    InputValue::Symbol(String::new()),
                    "{}",
                    saved.name
                );
            } else {
                assert!(restored.data_requests().is_empty(), "{}", saved.name);
            }
            if let Some(offset) = inputs.iter().find(|input| input.id == "plot_offset") {
                assert_eq!(offset.value, InputValue::Integer(0), "{}", saved.name);
            }
            assert_eq!(restored.settings(), saved.settings, "{}", saved.name);
            assert_eq!(restored.is_visible(), saved.visible, "{}", saved.name);
            assert_eq!(Some(restored.scale()), saved.scale, "{}", saved.name);
            let mut fresh = ConfiguredStudy::new(factory.create(&saved.name).unwrap());
            for (id, value) in &saved.inputs {
                if fresh
                    .inputs()
                    .iter()
                    .any(|input| input.id == *id && input.value != *value)
                {
                    fresh.set_input(id, value.clone()).unwrap();
                }
            }
            // Zig Zag saves from before its extension and labels retain the former drawing.
            if saved.name == "ZigZag" {
                for (id, value) in [
                    ("extend_to_last_bar", InputValue::Toggle(false)),
                    ("price_labels", InputValue::Toggle(false)),
                    ("change_labels", InputValue::Toggle(false)),
                    ("change_mode", InputValue::Choice(1)),
                    ("volume_labels", InputValue::Toggle(false)),
                ] {
                    fresh.set_input(id, value).unwrap();
                }
            }
            restored.calculate(&bars);
            fresh.calculate(&bars);
            for line in 0..restored.line_count() {
                for bar in 0..bars.len() {
                    assert_eq!(
                        restored.values().value(line, bar),
                        fresh.values().value(line, bar),
                        "{} {line} {bar}",
                        saved.name
                    );
                }
            }
            if saved.name == "MA Cross" {
                let zero = StudyTemplate::from_study(restored.as_ref())
                    .create()
                    .unwrap();
                assert_eq!(zero.plot_offset(0), 0);
                restored
                    .set_input("plot_offset", InputValue::Integer(2))
                    .unwrap();
                let again = StudyTemplate::from_study(restored.as_ref())
                    .create()
                    .unwrap();
                assert_eq!(again.plot_offset(0), 2);
            }
        }
    }

    /// Guards #3898: a study whose outputs cannot follow another symbol or interval, saved with
    /// one, restores on the chart's bars and requests no other series.
    #[test]
    fn templates_with_other_bars_inputs_restore_on_the_chart_bars() {
        let factory = IndicatorFactory::new();
        for name in ["Time Price Opportunity", "Analyst price forecast"] {
            let mut saved =
                StudyTemplate::from_study(&ConfiguredStudy::new(factory.create(name).unwrap()));
            saved.inputs.retain(|(id, _)| {
                !matches!(
                    id.as_str(),
                    "calculation_symbol" | "calculation_timeframe" | "wait_for_close"
                )
            });
            let expected = saved.inputs.clone();
            saved.inputs.extend([
                ("calculation_symbol".into(), InputValue::Symbol("B".into())),
                (
                    "calculation_timeframe".into(),
                    InputValue::Timeframe(Some(crate::model::Timeframe::Week1)),
                ),
                ("wait_for_close".into(), InputValue::Toggle(false)),
            ]);
            let restored = saved.create_with(&factory).unwrap();
            assert!(restored.data_requests().is_empty(), "{name}");
            assert!(
                !restored.inputs().iter().any(|input| matches!(
                    input.id.as_str(),
                    "calculation_symbol" | "calculation_timeframe" | "wait_for_close"
                )),
                "{name}"
            );
            assert_eq!(
                StudyTemplate::from_study(restored.as_ref()).inputs,
                expected,
                "{name}"
            );
        }
    }

    /// A template restores the inputs it holds whatever the study's default has become, with every
    /// line it showed, and an input it lacks takes the value the saved inputs had before the
    /// study offered it. Values outside a narrowed input range load clamped to that range, except
    /// those of the inputs that restore as saved, which a style edit keeps and an undone edit
    /// brings back.
    /// Multi-Range Delta ranges saved with an inclusive largest size restore with the smallest
    /// limit above it that has one more decimal place, and their names write it.
    #[test]
    fn templates_saved_before_split_lengths_restore_their_values() {
        use InputValue::{Choice, Length, Number, Text, Toggle};
        let rows = [
            (
                "CCI",
                vec![("period", Length(20))],
                vec![("period", Length(20)), ("smoothing", Choice(0))],
            ),
            (
                "%B",
                vec![("multiplier", Number(0.0))],
                vec![("multiplier", Number(0.001))],
            ),
            (
                "%B",
                vec![("multiplier", Number(80.0))],
                vec![("multiplier", Number(50.0))],
            ),
            // Saved before these ranges narrowed: restored as saved.
            (
                "BB",
                vec![("std_dev", Number(0.0))],
                vec![("std_dev", Number(0.0))],
            ),
            (
                "BB",
                vec![("std_dev", Number(80.0))],
                vec![("std_dev", Number(80.0))],
            ),
            (
                "BBTrend",
                vec![("multiplier", Number(0.0))],
                vec![("multiplier", Number(0.0))],
            ),
            (
                "Woodies CCI",
                vec![("turbo_length", Length(1)), ("cci_length", Length(30))],
                vec![("turbo_length", Length(1)), ("cci_length", Length(30))],
            ),
            (
                "MFI",
                vec![("period", Length(5000))],
                vec![("period", Length(5000))],
            ),
            (
                "SuperTrend",
                vec![("multiplier", Number(0.005))],
                vec![("multiplier", Number(0.005))],
            ),
            (
                "LinReg Slope",
                vec![("period", Length(1))],
                vec![("period", Length(1))],
            ),
            // Saved under the earlier name.
            (
                "RankCorr",
                vec![("smoothing_deviation", Number(80.0))],
                vec![("smoothing_deviation", Number(80.0))],
            ),
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
            // Saved with both lengths, in either order.
            (
                "ADX",
                vec![("adx_smoothing", Length(20)), ("period", Length(10))],
                vec![("period", Length(10)), ("adx_smoothing", Length(20))],
            ),
            (
                "ADX",
                vec![("period", Length(10)), ("adx_smoothing", Length(20))],
                vec![("period", Length(10)), ("adx_smoothing", Length(20))],
            ),
            (
                "DMI",
                vec![("adx_smoothing", Length(20)), ("period", Length(10))],
                vec![("period", Length(10)), ("adx_smoothing", Length(20))],
            ),
            (
                "DMI",
                vec![("period", Length(10)), ("adx_smoothing", Length(20))],
                vec![("period", Length(10)), ("adx_smoothing", Length(20))],
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
                "Chandelier",
                vec![("atr_period", Length(5)), ("period", Length(10))],
                vec![("period", Length(10)), ("atr_period", Length(5))],
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
            // Saved before the midpoint had a setting, when every profile drew it, and before the
            // volume profile had settings, when its rows were shaded by the TPO value area.
            (
                "Time Price Opportunity",
                vec![("display", Choice(1))],
                vec![
                    ("display", Choice(1)),
                    ("midpoint", Toggle(true)),
                    ("volume_tpo_value_area", Toggle(true)),
                ],
            ),
            (
                "Time Price Opportunity",
                vec![("midpoint", Toggle(false))],
                vec![
                    ("midpoint", Toggle(false)),
                    ("volume_tpo_value_area", Toggle(true)),
                ],
            ),
            (
                "Time Price Opportunity",
                vec![
                    ("midpoint", Toggle(false)),
                    ("volume_tpo_value_area", Toggle(false)),
                ],
                vec![
                    ("midpoint", Toggle(false)),
                    ("volume_tpo_value_area", Toggle(false)),
                ],
            ),
            // Saved before levels could follow a period, when they were swings.
            (
                "Missed Pivot Levels",
                vec![("left", Length(5))],
                vec![("left", Length(5)), ("period", Choice(0))],
            ),
            (
                "Missed Pivot Levels",
                vec![("period", Choice(2))],
                vec![("period", Choice(2))],
            ),
            // Saved before the interval could follow the chart's, as new studies do.
            (
                "Higher Interval Candles",
                vec![("interval", Choice(15))],
                vec![("interval", Choice(15)), ("auto_interval", Toggle(false))],
            ),
            (
                "Higher Interval Candles",
                vec![("auto_interval", Toggle(true))],
                vec![("auto_interval", Toggle(true))],
            ),
            // Saved before the bars per year could be set, when every volatility used 252.
            (
                "C2C Vol",
                vec![("period", Length(20))],
                vec![("period", Length(20)), ("trading_days", Number(252.0))],
            ),
            (
                "GK Vol",
                vec![("period", Length(20))],
                vec![("period", Length(20)), ("trading_days", Number(252.0))],
            ),
            (
                "Zero-Trend Vol",
                vec![("period", Length(20))],
                vec![("period", Length(20)), ("trading_days", Number(252.0))],
            ),
            (
                "Zero-Trend Vol",
                vec![("trading_days", Number(365.0))],
                vec![("trading_days", Number(365.0))],
            ),
            // Saved before the output could be chosen, when it was the signed direction.
            (
                "MajRule",
                vec![("period", Length(10))],
                vec![("period", Length(10)), ("mode", Choice(1))],
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
            // Saved before the levels could extend past their profiles, and before sessions,
            // leaving out the bars outside them and rows in ticks could be chosen.
            (
                "Vol Profile Session",
                vec![("row_count", Length(30))],
                vec![
                    ("row_count", Length(30)),
                    ("extend_poc", Toggle(false)),
                    ("extend_vah", Toggle(false)),
                    ("extend_val", Toggle(false)),
                    ("sessions", Choice(0)),
                    ("hours_only", Toggle(false)),
                    ("row_ticks", Length(0)),
                ],
            ),
            // Saved when the deviation took any number from 0 up: restored at the nearer bound.
            (
                "ZigZag",
                vec![("deviation_percent", Number(0.0))],
                vec![("deviation_percent", Number(0.000_01))],
            ),
            (
                "ZigZag",
                vec![("deviation_percent", Number(150.0))],
                vec![("deviation_percent", Number(100.0))],
            ),
            #[cfg(feature = "drawings")]
            (
                "Auto Pitchfork",
                vec![("deviation_percent", Number(0.0))],
                vec![("deviation_percent", Number(0.000_01))],
            ),
            // Saved before labels and the extension to the last bar, both on by default since.
            (
                "ZigZag",
                vec![("deviation_percent", Number(3.0))],
                vec![
                    ("deviation_percent", Number(3.0)),
                    ("price_labels", Toggle(false)),
                    ("change_labels", Toggle(false)),
                    ("change_mode", Choice(1)),
                    ("volume_labels", Toggle(false)),
                    ("extend_to_last_bar", Toggle(false)),
                ],
            ),
            (
                "ZigZag",
                vec![("extend_to_last_bar", Toggle(true))],
                vec![
                    ("extend_to_last_bar", Toggle(true)),
                    ("price_labels", Toggle(true)),
                ],
            ),
            // Saved before the twelve lengths were inputs.
            (
                "GMMA",
                vec![],
                vec![("periods [0]", Length(3)), ("periods [11]", Length(60))],
            ),
            // Saved before swings could be sized by price and the constructions drawn.
            (
                "Elliott Wave",
                vec![
                    ("left", Length(2)),
                    ("right", Length(3)),
                    ("history", Length(7)),
                ],
                vec![
                    ("left", Length(2)),
                    ("right", Length(3)),
                    ("history", Length(7)),
                    ("swing_mode", Choice(0)),
                    ("swing_move", Number(5.0)),
                    ("show_developing", Toggle(true)),
                    ("projections", Toggle(false)),
                    ("as_drawings", Toggle(false)),
                ],
            ),
            (
                "Elliott Wave",
                vec![
                    ("swing_mode", Choice(2)),
                    ("swing_move", Number(2.5)),
                    ("history", Length(7)),
                    ("show_developing", Toggle(false)),
                    ("projections", Toggle(true)),
                    ("as_drawings", Toggle(true)),
                ],
                vec![
                    ("swing_mode", Choice(2)),
                    ("swing_move", Number(2.5)),
                    ("history", Length(7)),
                    ("show_developing", Toggle(false)),
                    ("projections", Toggle(true)),
                    ("as_drawings", Toggle(true)),
                ],
            ),
            // Saved while each range included its largest size: the limit is the smallest number
            // above it with one more decimal place.
            (
                "Multi-Range Delta",
                vec![
                    ("min0", Number(0.0)),
                    ("max0", Number(9.0)),
                    ("min1", Number(10.0)),
                    ("max1", Number(99.0)),
                    ("min2", Number(100.0)),
                    ("max2", Number(f64::MAX)),
                ],
                vec![
                    ("min0", Number(0.0)),
                    ("limit0", Number(9.1)),
                    ("min1", Number(10.0)),
                    ("limit1", Number(99.1)),
                    ("min2", Number(100.0)),
                    ("limit2", Number(f64::MAX)),
                ],
            ),
        ];
        let factory = IndicatorFactory::new();
        // A study saved with the former default largest sizes 9 and 99 writes the limits it
        // restores as briefly (#4064).
        let multi_range = StudyTemplate {
            name: "Multi-Range Delta".to_owned(),
            inputs: vec![
                ("max0".to_owned(), Number(9.0)),
                ("max1".to_owned(), Number(99.0)),
            ],
            settings: factory.create("Multi-Range Delta").unwrap().settings(),
            visible: true,
            scale: None,
            metric: None,
        };
        assert_eq!(
            multi_range.create().unwrap().line_names(),
            [
                "Delta [0, 9.1) lots",
                "Delta [10, 99.1) lots",
                "Delta [100, ∞) lots"
            ]
        );
        let periods = (0..7).map(|period| {
            (
                "Vol Profile Periodic",
                vec![("period", Choice(period))],
                vec![
                    ("period", Choice(period)),
                    ("extend_poc", Toggle(false)),
                    ("extend_vah", Toggle(false)),
                    ("extend_val", Toggle(false)),
                    ("period_count", Length(1)),
                    ("restart_yearly", Toggle(false)),
                    ("row_ticks", Length(0)),
                ],
            )
        });
        for (name, saved, restored) in rows.into_iter().chain(periods) {
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
                metric: None,
            };
            let study = template.create().unwrap();
            let inputs = study.inputs();
            for (id, value) in restored {
                let input = inputs.iter().find(|input| input.id == id);
                assert_eq!(input.map(|input| &input.value), Some(&value), "{name} {id}");
            }
            // A style edit applies every input as it stands, which keeps its value.
            let values = |study: &dyn Indicator| {
                let inputs = study.inputs().into_iter();
                inputs
                    .map(|input| (input.id, input.value))
                    .collect::<Vec<_>>()
            };
            let mut edited = study.clone_box();
            edited
                .apply_settings(&values(study.as_ref()), study.settings())
                .unwrap();
            assert_eq!(values(edited.as_ref()), values(study.as_ref()), "{name}");
            // Undoing an edit, or cancelling its preview, brings every input back.
            let defaults: Vec<_> = (study.inputs().into_iter())
                .map(|input| (input.id, input.default))
                .collect();
            edited.apply_settings(&defaults, study.settings()).unwrap();
            edited.restore_settings(&study.snapshot_settings()).unwrap();
            assert_eq!(
                values(edited.as_ref()),
                values(study.as_ref()),
                "{name} undone"
            );
            assert!(
                (0..study.line_count()).all(|line| study.plot_style(line).visible),
                "{name}"
            );
        }
        // Literal study saved on main before Hours existed: all saved inputs and styles stay,
        // and the trading day remains selected with its extended hours split.
        let saved: StudyTemplate = serde_json::from_str(r#"{"name":"Time Price Opportunity","inputs":[["period",{"Length":30}],["row_ticks",{"Length":0}],["value_area",{"Number":70.0}],["split_sessions",{"Toggle":true}],["single_prints",{"Toggle":false}],["midpoint",{"Toggle":false}],["statistics",{"Toggle":false}],["display",{"Choice":0}],["sessions",{"Choice":0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TimePriceOpportunity.html","plots":[],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[16,16,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#).unwrap();
        let study = saved.create().unwrap();
        assert_eq!(study.settings(), saved.settings);
        let inputs = study.inputs();
        for (id, value) in saved
            .inputs
            .iter()
            .chain(&[("hours".to_owned(), Choice(0))])
        {
            assert_eq!(
                inputs
                    .iter()
                    .find(|input| input.id == *id)
                    .map(|input| &input.value),
                Some(value),
                "Time Price Opportunity {id}"
            );
        }

        // Literal single-period saves retain their lengths, plot visibility and styles.
        for text in [
            r#"{"name":"ADX","inputs":[["period",{"Length":20}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ADX.html","plots":[{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[255,109,0,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false}],"precision":2,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[15,3,4,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
            r#"{"name":"DMI","inputs":[["period",{"Length":20}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.DirectionalMovement.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[255,109,0,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[245,0,87,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
            r#"{"name":"Chandelier","inputs":[["period",{"Length":10}],["multiplier",{"Number":3.0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ChandelierExit.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[255,109,0,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
        ] {
            let saved: StudyTemplate = serde_json::from_str(text).unwrap();
            let study = saved.create().unwrap();
            assert_eq!(study.settings(), saved.settings, "{}", saved.name);
            assert_eq!(Some(study.scale()), saved.scale, "{}", saved.name);
            let inputs = study.inputs();
            let (other, period) = if saved.name == "Chandelier" {
                ("atr_period", 10)
            } else {
                ("adx_smoothing", 20)
            };
            for id in ["period", other] {
                assert_eq!(
                    inputs
                        .iter()
                        .find(|input| input.id == id)
                        .map(|input| &input.value),
                    Some(&Length(period)),
                    "{} {id}",
                    saved.name
                );
            }
        }

        // Literal Seasonality saves keep their year, colours and table position: one from before
        // its display inputs at the lower right, one from before its Positive colour input, whose
        // Expected plot coloured rising months.
        for (text, year, positive, negative, position) in [
            (
                r#"{"name":"Seasonality","inputs":[["start_year",{"Integer":1900}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.Seasonality.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Dashed","plot_type":"JoinedStepLine","visible":true},{"color":[120,123,134,255],"width":1.0,"pattern":"Dashed","plot_type":"JoinedStepLine","visible":true},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedStepLine","visible":false},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedStepLine","visible":false},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedStepLine","visible":false}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[1,10,8,16],"untagged_plots":[0,1,2,3,4],"price_line_plots":[],"plot_variants":[[],[],[],[],[]],"background":null,"background_visible":true,"fills":[{"color":[2,31,26,51],"inverted_color":[48,11,14,51],"visible":true,"independent":true}],"gradients":[]},"visible":true,"scale":"Main"}"#,
                1900,
                Color32::from_rgb(41, 98, 255),
                Color32::from_rgb(242, 55, 69),
                crate::studies::TablePosition::BottomRight,
            ),
            (
                r#"{"name":"Seasonality","inputs":[["start_year",{"Integer":2015}],["ignored_months",{"Text":""}],["negative_color",{"Color":[242,54,69,255]}],["color_cutoff",{"Number":10.0}],["show_averages",{"Toggle":true}],["show_std_dev",{"Toggle":true}],["show_positive",{"Toggle":true}],["table_position",{"Choice":7}],["table_width",{"Number":0.0}],["table_height",{"Number":0.0}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.Seasonality.html","plots":[{"color":[255,0,255,255],"width":1.0,"pattern":"Dashed","plot_type":"JoinedStepLine","visible":true},{"color":[120,123,134,255],"width":1.0,"pattern":"Dashed","plot_type":"JoinedStepLine","visible":true},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedStepLine","visible":false},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedStepLine","visible":false},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedStepLine","visible":false}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[1,10,8,16],"untagged_plots":[0,1,2,3,4],"price_line_plots":[],"plot_variants":[[],[],[],[],[]],"background":null,"background_visible":true,"fills":[{"color":[2,31,26,51],"inverted_color":[48,11,14,51],"visible":true,"independent":true}],"gradients":[]},"visible":true,"scale":"Main"}"#,
                2015,
                Color32::from_rgb(255, 0, 255),
                Color32::from_rgb(242, 54, 69),
                crate::studies::TablePosition::BottomCenter,
            ),
        ] {
            let saved: StudyTemplate = serde_json::from_str(text).unwrap();
            let mut study = saved.create().unwrap();
            let inputs = study.inputs();
            for (id, value) in [
                ("start_year", InputValue::Integer(year)),
                ("positive_color", InputValue::Color(positive)),
                ("negative_color", InputValue::Color(negative)),
            ] {
                assert_eq!(
                    inputs
                        .iter()
                        .find(|input| input.id == id)
                        .map(|input| &input.value),
                    Some(&value),
                    "Seasonality from {year} {id}"
                );
            }
            assert_eq!(study.settings(), saved.settings);
            // January 2021 rises 12%, past the cutoff: the full positive colour.
            let bars = [(2020, 12, 100.0), (2021, 1, 112.0)].map(|(year, month, close)| {
                Bar::new(
                    Utc.with_ymd_and_hms(year, month, 28, 0, 0, 0).unwrap(),
                    close,
                    close,
                    close,
                    close,
                    1.0,
                )
            });
            study.calculate(&bars);
            let table = study.table().unwrap();
            assert_eq!(
                (table.position, table.width, table.height),
                (position, None, None)
            );
            assert_eq!(table.cell_colors[1][1], positive);
        }

        let saved: StudyTemplate = serde_json::from_str(r#"{"name":"Vol Profile Periodic","inputs":[["row_count",{"Length":30}],["value_area",{"Number":70.0}],["row_size",{"Number":0.0}],["row_ticks",{"Length":0}],["display",{"Choice":1}],["source_interval",{"Timeframe":null}],["feed_source",{"Toggle":true}],["period_count",{"Length":1}],["period",{"Choice":1}],["restart_yearly",{"Toggle":false}],["width",{"Number":30.0}],["extend_poc",{"Toggle":false}],["extend_vah",{"Toggle":false}],["extend_val",{"Toggle":false}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VolProfilePeriodic.html","plots":[],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[4,16,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#).unwrap();
        for legacy in [
            saved.clone(),
            StudyTemplate {
                inputs: saved
                    .inputs
                    .iter()
                    .filter(|(id, _)| id != "restart_yearly")
                    .cloned()
                    .collect(),
                ..saved
            },
        ] {
            let restored = legacy.create().unwrap();
            assert_eq!(
                restored
                    .inputs()
                    .into_iter()
                    .find(|input| input.id == "restart_yearly")
                    .unwrap()
                    .value,
                Toggle(false)
            );
        }

        let hd: StudyTemplate = serde_json::from_str(r#"{"name":"Vol Profile Session HD","inputs":[["sessions",{"Choice":3}],["session_start",{"Length":570}],["session_end",{"Length":960}],["show_values",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VolProfileSessionHd.html","plots":[{"color":[255,152,0,255],"width":1.0,"pattern":"Solid","plot_type":"StepLine","visible":false},{"color":[33,150,243,255],"width":1.0,"pattern":"Solid","plot_type":"StepLine","visible":false},{"color":[33,150,243,255],"width":1.0,"pattern":"Solid","plot_type":"StepLine","visible":false}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[4,16,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#).unwrap();
        let study = hd.create().unwrap();
        assert_eq!(study.name(), "Vol Profile Session HD");
        assert!(!study.inputs().iter().any(|input| input.id == "row_count"));
        let template = StudyTemplate::from_study(study.as_ref());
        let restored =
            serde_json::from_str::<StudyTemplate>(&serde_json::to_string(&template).unwrap())
                .unwrap()
                .create()
                .unwrap();
        assert_eq!(
            serde_json::to_value(StudyTemplate::from_study(restored.as_ref())).unwrap(),
            serde_json::to_value(template).unwrap()
        );

        // A chart save of an anchored profile from before tick heights and width were inputs
        // keeps its settings and inputs and takes the defaults of both.
        let saved: StudyTemplate = serde_json::from_str(r#"{"name":"Vol Profile Session","inputs":[["row_count",{"Length":30}],["value_area",{"Number":70.0}],["row_size",{"Number":0.0}],["row_ticks",{"Length":0}],["display",{"Choice":1}],["source_interval",{"Timeframe":null}],["feed_source",{"Toggle":true}],["adaptive_rows",{"Toggle":false}],["sessions",{"Choice":0}],["session_start",{"Length":0}],["session_end",{"Length":0}],["session_zone",{"Text":""}],["hours_only",{"Toggle":false}],["width",{"Number":30.0}],["extend_poc",{"Toggle":false}],["extend_vah",{"Toggle":false}],["extend_val",{"Toggle":false}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VolProfileSession.html","plots":[],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[4,16,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#).unwrap();
        let restored = saved.create().unwrap();
        for (id, expected) in [
            ("custom_sessions", InputValue::Choice(0)),
            ("session_overlap", InputValue::Choice(0)),
            ("clip_hours", InputValue::Toggle(false)),
        ] {
            assert_eq!(
                restored
                    .inputs()
                    .iter()
                    .find(|input| input.id == id)
                    .unwrap()
                    .value,
                expected
            );
        }

        let mut expected = saved.settings.clone();
        expected.plots = (0..3)
            .map(|line| crate::studies::VolProfileSession::default().plot_style(line))
            .collect();
        expected.plot_variants = vec![vec![]; 3];
        assert_eq!(restored.settings(), expected);
        for (id, value) in &saved.inputs {
            let value = match (id.as_str(), value) {
                ("session_start" | "session_end", InputValue::Length(0)) => {
                    InputValue::TimeOfDay(NaiveTime::MIN)
                }
                ("session_zone", InputValue::Text(zone)) => InputValue::TimeZone(zone.clone()),
                (_, value) => value.clone(),
            };
            assert_eq!(
                restored
                    .inputs()
                    .iter()
                    .find(|input| &input.id == id)
                    .map(|input| &input.value),
                Some(&value),
                "{id}"
            );
        }
        assert_eq!(
            restored
                .inputs()
                .iter()
                .find(|input| input.id == "whole_tick_rows")
                .unwrap()
                .value,
            Toggle(false)
        );
        let saved: StudyTemplate = serde_json::from_str(
            r#"{"name":"Vol Profile Anchored","inputs":[["row_count",{"Length":30}],["value_area",{"Number":70.0}],["row_size",{"Number":0.0}],["display",{"Choice":1}],["source_interval",{"Timeframe":null}],["feed_source",{"Toggle":true}],["start_time",{"Text":""}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VolProfileAnchored.html","plots":[],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[4,16,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
        )
        .unwrap();
        let study = saved.create().unwrap();
        let mut expected_settings = saved.settings.clone();
        expected_settings.plots = (0..3)
            .map(|line| crate::studies::VolProfileSession::default().plot_style(line))
            .collect();
        expected_settings.plot_variants = vec![vec![]; 3];
        assert_eq!(study.settings(), expected_settings);
        assert!(study.settings().plots.iter().all(|plot| !plot.visible));
        let inputs = study.inputs();
        let value = |id: &str| {
            inputs
                .iter()
                .find(|input| input.id == id)
                .map(|input| &input.value)
        };
        for (id, kept) in &saved.inputs {
            assert_eq!(value(id), Some(kept), "{id}");
        }
        assert_eq!(value("whole_tick_rows"), Some(&Toggle(false)));
        assert_eq!(value("row_ticks"), Some(&Length(0)));
        assert_eq!(value("width"), Some(&Number(30.0)));
        for (id, expected) in [
            ("show_rows", Toggle(true)),
            ("show_poc", Toggle(true)),
            ("show_vah", Toggle(false)),
            ("show_val", Toggle(false)),
            ("poc_color", InputValue::Color(Color32::PLACEHOLDER)),
            ("vah_color", InputValue::Color(Color32::PLACEHOLDER)),
            ("val_color", InputValue::Color(Color32::PLACEHOLDER)),
        ] {
            assert_eq!(value(id), Some(&expected), "{id}");
        }
        let mut edited = study.clone_box();
        edited
            .set_input("up_color", InputValue::Color(Color32::BLUE))
            .unwrap();
        let mut settings = edited.settings();
        settings.plots[0].visible = true;
        edited.apply_settings(&[], settings).unwrap();
        let template = StudyTemplate::from_study(edited.as_ref());
        let encoded = serde_json::to_string(&template).unwrap();
        let restored = serde_json::from_str::<StudyTemplate>(&encoded)
            .unwrap()
            .create()
            .unwrap();
        assert_eq!(
            serde_json::to_value(StudyTemplate::from_study(restored.as_ref())).unwrap(),
            serde_json::to_value(template).unwrap()
        );

        // A Zig Zag as a chart saved it before the extension and the new label defaults.
        let saved: StudyTemplate = serde_json::from_str(
            r#"{"name":"ZigZag","inputs":[["deviation_percent",{"Number":3.0}],["depth",{"Length":10}],["mode",{"Choice":0}],["price_labels",{"Toggle":true}],["change_labels",{"Toggle":true}],["change_mode",{"Choice":1}],["volume_labels",{"Toggle":false}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ZigZag.html","plots":[{"color":[41,98,255,255],"width":2.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
        )
        .unwrap();
        let study = saved.create().unwrap();
        assert_eq!(study.settings(), saved.settings);
        let inputs = study.inputs();
        for (id, kept) in [
            ("deviation_percent", Number(3.0)),
            ("price_labels", Toggle(true)),
            ("change_labels", Toggle(true)),
            ("change_mode", Choice(1)),
            ("volume_labels", Toggle(false)),
            ("extend_to_last_bar", Toggle(false)),
        ] {
            let input = inputs.iter().find(|input| input.id == id);
            assert_eq!(input.map(|input| &input.value), Some(&kept), "ZigZag {id}");
        }
    }

    /// A Correlation Coefficient as a chart saved it before it opened as an area: a line with its
    /// zero guide alone, here dotted, 3 px wide and orange with its value line.
    const CORRELATION_ZERO_GUIDE: &str = r#"{"name":"Correlation Coefficient","inputs":[["symbol_0",{"Symbol":""}],["source_0",{"Choice":3}],["symbol_1",{"Symbol":""}],["source_1",{"Choice":3}],["length",{"Length":20}],["log_returns",{"Toggle":false}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/type.Correlation.html","plots":[{"color":[33,150,243,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[{"value":null,"color":[255,152,0,255],"pattern":"Dotted","width":3.0,"visible":true,"price_line":true}],"fill_between_levels":false,"level_fill_color":[2,9,15,16],"untagged_plots":[],"price_line_plots":[0],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;

    /// An RSI, a VWAP, an auto anchored VWAP and a Correlation Coefficient saved before their
    /// defaults gained an average, bands, and an area restore with the plots and guides they had,
    /// and a Rank Correlation saved before it had an average, levels and a filled band, and a
    /// Majority Rule saved before its guide at 50, restore without them. So do an SMI, a
    /// Stochastic RSI and a Connors RSI saved before their guides, a Stochastic saved before its
    /// guides had styles, a Donchian channel saved before its band, a Moving Average Channel saved
    /// before its band had a style and a band saved with its boundary plots hidden before it
    /// could be drawn without them. Studies whose default colours changed keep the saved ones. Older per-bar studies keep
    /// the colours and widths they drew before every state had its own style control. Renamed
    /// input labels retain the ids, values and styles of the saved studies. Booker Reversal and
    /// Ziv Ghost Pivots saved before their marks changed keep their colours and crosses, and
    /// Booker levels saved as whole numbers restore as the same decimals.
    #[test]
    fn saved_studies_keep_the_plots_they_had_before_their_defaults_gained_more() {
        // A metric descriptor saved before plot declarations and the Symbol input keeps its look.
        let saved: StudyTemplate = serde_json::from_str(
            r#"{"name":"Open interest","inputs":[],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":null,"plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedStepLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent","metric":{"id":"oi","name":"Open interest","unit":"","format":"Volume"}}"#,
        ).unwrap();
        let restored = saved.create().unwrap();
        assert_eq!(restored.settings(), saved.settings);
        assert_eq!(Some(restored.scale()), saved.scale);
        assert_eq!(
            restored.metric().unwrap().plot,
            crate::data::MetricPlot::StepLine
        );
        assert_eq!(restored.metric().unwrap().colors, None);
        assert_eq!(restored.data_requests()[0].symbol, "");
        let symbol = restored
            .inputs()
            .into_iter()
            .find(|input| input.id == "symbol")
            .unwrap();
        assert_eq!(symbol.value, InputValue::Symbol(String::new()));
        assert_eq!(symbol.default, symbol.value);
        let again = StudyTemplate::from_study(restored.as_ref())
            .create()
            .unwrap();
        assert_eq!(again.settings(), saved.settings);

        // This Knoxville template was saved before price lines and before the window rule. It
        // keeps its inputs and styles, without pivot sides and on the price scale, with lines
        // off; enabling them and saving again retains that choice.
        let saved: StudyTemplate = serde_json::from_str(
            r#"{"name":"Knoxville Divergence","inputs":[["momentum_period",{"Length":1}],["rsi_period",{"Length":2}],["left",{"Length":1}],["right",{"Length":1}],["lookback",{"Length":10}],["oversold",{"Length":30}],["overbought",{"Length":70}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.KnoxvilleDivergence.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleUp","visible":true},{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleDown","visible":true}],"precision":2,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
        ).unwrap();
        let bars =
            crate::studies::builtin::hlc_bars(&[10.0, 8.0, 9.0, 7.5, 8.5].map(|c| (c, c, c)));
        let mut restored = saved.create().unwrap();
        restored.calculate(&bars);
        assert_eq!(restored.settings().plots, saved.settings.plots);
        let inputs = restored.inputs();
        for (id, value) in [
            ("momentum_period", InputValue::Length(1)),
            ("rsi_period", InputValue::Length(2)),
            ("lookback", InputValue::Length(10)),
            ("oversold", InputValue::Length(30)),
            ("overbought", InputValue::Length(70)),
            ("price_lines", InputValue::Toggle(false)),
        ] {
            assert_eq!(
                inputs
                    .iter()
                    .find(|input| input.id == id)
                    .map(|input| &input.value),
                Some(&value),
                "{id}"
            );
        }
        assert!(
            !inputs
                .iter()
                .any(|input| matches!(input.id.as_str(), "left" | "right"))
        );
        assert!(restored.is_overlay());
        assert_eq!(restored.scale(), StudyScale::Main);
        assert!(restored.pane().levels.is_empty());
        assert!(
            !restored.plot_style(0).visible,
            "momentum is only a legend readout"
        );
        assert_eq!(restored.values().value(0, 3), Some(-1.5));
        assert_eq!(restored.values().value(1, 3), Some(7.5));
        assert_eq!(restored.values().value(1, 4), None);
        assert!(restored.price_segments().is_empty());
        restored
            .set_input("price_lines", InputValue::Toggle(true))
            .unwrap();
        restored.calculate(&bars);
        let mut again = StudyTemplate::from_study(restored.as_ref())
            .create()
            .unwrap();
        again.calculate(&bars);
        let mut fresh = crate::studies::KnoxvilleDivergence::default();
        for (id, value) in [("momentum_period", 1), ("rsi_period", 2), ("lookback", 10)] {
            fresh.set_input(id, InputValue::Length(value)).unwrap();
        }
        let mut fresh = ConfiguredStudy::new(Box::new(fresh));
        fresh.calculate(&bars);
        assert_eq!(
            fresh.price_segments(),
            [crate::studies::StudySegment {
                start_bar: 1,
                start_price: 8.0,
                end_bar: 3,
                end_price: 7.5,
                plot: 1,
            }]
        );
        assert_eq!(restored.price_segments(), fresh.price_segments());
        assert_eq!(again.price_segments(), fresh.price_segments());

        // Chart saves made before the input labels changed keep every saved id, value and
        // style while the editor describes the three study-local controls distinctly.
        for (saved, id, label) in [
            // Literal default study captured on origin/main 04d6d16e7, before period pivots.
            (
                r#"{"name":"Missed Pivot Levels","inputs":[["left",{"Length":10}],["right",{"Length":10}],["tolerance",{"Number":0.1}],["max_levels",{"Length":20}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.MissedPivots.html","plots":[{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[15,3,4,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
                "left",
                "Pivot left bars",
            ),
            // Saved before Elliott Wave gained its swing and display inputs.
            (
                r#"{"name":"Elliott Wave","inputs":[["left",{"Length":2}],["right",{"Length":3}],["history",{"Length":7}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ElliottWave.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Circles","visible":true},{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleUp","visible":true},{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleDown","visible":true},{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleDown","visible":false}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
                "left",
                "Pivot bars left",
            ),
            (
                r#"{"name":"LSMA","inputs":[["period",{"Length":25}],["offset",{"Integer":0}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.LSMA.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
                "offset",
                "Regression offset",
            ),
            (
                r#"{"name":"ALMA","inputs":[["period",{"Length":9}],["offset",{"Number":0.85}],["sigma",{"Number":6.0}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ALMA.html","plots":[{"color":[33,150,243,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[2,9,15,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
                "offset",
                "Weight offset",
            ),
            (
                r#"{"name":"Higher Interval Candles","inputs":[["auto_interval",{"Toggle":true}],["interval",{"Choice":14}],["extended_hours",{"Toggle":true}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.HigherIntervalCandles.html","plots":[{"color":[0,150,136,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[244,67,54,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[0,9,9,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
                "interval",
                "Candle interval",
            ),
        ] {
            let saved: StudyTemplate = serde_json::from_str(saved).unwrap();
            let restored = saved.create().unwrap();
            assert_eq!(restored.settings(), saved.settings, "{} styles", saved.name);
            let inputs = restored.inputs();
            for (id, value) in &saved.inputs {
                let input = inputs.iter().find(|input| input.id == *id).unwrap();
                assert_eq!(&input.value, value, "{} {id}", saved.name);
            }
            assert_eq!(
                inputs.iter().find(|input| input.id == id).unwrap().label,
                label
            );
        }

        // Actual chart saves from before state colours had their own controls retain the
        // colours and widths they drew, and saving the upgrade again keeps those choices.
        use crate::studies::palette;
        for (saved, plot, colours) in [
            (
                r#"{"name":"Accelerator Oscillator","inputs":[["source",{"Source":{"Price":"HL2"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.AcceleratorOscillator.html","plots":[{"color":[0,0,128,255],"width":1.0,"pattern":"Solid","plot_type":"Histogram","visible":true}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[0,0,8,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
                palette::GREEN,
                vec![palette::RED],
            ),
            (
                r#"{"name":"Chop Zone","inputs":[["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ChopZone.html","plots":[{"color":[38,198,218,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":true}],"precision":0,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[2,12,14,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
                palette::CYAN,
                vec![
                    palette::DARK_GREEN,
                    palette::LIGHT_GREEN,
                    palette::DEEP_TEAL,
                    palette::YELLOW,
                    palette::LIGHT_ORANGE,
                    palette::ORANGE,
                    palette::PINK,
                    palette::DEEP_RED,
                ],
            ),
            (
                r#"{"name":"Impulse","inputs":[["ema_period",{"Length":13}],["macd_fast",{"Length":12}],["macd_slow",{"Length":26}],["macd_signal",{"Length":9}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ElderImpulseSystem.html","plots":[{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[5,11,5,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
                palette::GREEN,
                vec![palette::GREEN, palette::GREEN],
            ),
            (
                r#"{"name":"Volatility Stop","inputs":[["length",{"Length":20}],["multiplier",{"Number":2.0}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VolatilityStop.html","plots":[{"color":[0,150,136,255],"width":1.0,"pattern":"Solid","plot_type":"Crosses","visible":true}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[0,9,9,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
                palette::DEEP_TEAL,
                vec![palette::BRIGHT_RED],
            ),
        ] {
            let saved: StudyTemplate = serde_json::from_str(saved).unwrap();
            let restored = saved.create().unwrap();
            let mut plots = saved.settings.plots.clone();
            plots[0].color = plot;
            assert_eq!(restored.settings().plots, plots, "{}", saved.name);
            let variants = restored.plot_variants(0);
            assert_eq!(
                variants.iter().map(|v| v.color).collect::<Vec<_>>(),
                colours,
                "{}",
                saved.name
            );
            assert!(variants.iter().all(|v| v.width == plots[0].width));
            let again = StudyTemplate::from_study(restored.as_ref())
                .create()
                .unwrap();
            assert_eq!(
                again.settings(),
                restored.settings(),
                "{} saved again",
                saved.name
            );
        }

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

        // Saved with a banded average by the release before the divergence input, an RSI restores
        // its four plots as they were styled and calculates no divergences.
        let banded: StudyTemplate = serde_json::from_str(
            r#"{"name":"RSI","inputs":[["period",{"Length":14}],["smoothing",{"Choice":2}],["smoothing_length",{"Length":14}],["smoothing_deviation",{"Number":2.0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.RSI.html","plots":[{"color":[126,87,194,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[253,216,53,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":true,"level_fill_color":[13,9,20,26],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[],[]],"background":null,"background_visible":true,"fills":[{"color":[8,18,8,26],"inverted_color":null,"visible":true}],"gradients":[]},"visible":true,"scale":"Independent"}"#,
        )
        .unwrap();
        let restored = banded.create().unwrap();
        assert_eq!(restored.settings().plots, banded.settings.plots);
        let divergence = restored.inputs().into_iter().find(|i| i.id == "divergence");
        assert_eq!(divergence.unwrap().value, InputValue::Toggle(false));
        // Saved since with the input on, it restores the four divergence plots too.
        let marked = ConfiguredStudy::new(Box::new(RSI::default().with_divergence(true)));
        let restored = StudyTemplate::from_study(&marked).create().unwrap();
        assert_eq!(restored.line_count(), 6);

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

        // A Correlation Coefficient opens as an area. Saved as a line, which then broke at every
        // gap, it restores as that line with breaks.
        let fresh =
            ConfiguredStudy::new(Box::new(crate::studies::Correlation::default())).settings();
        assert_eq!(fresh.plots[0].plot_type, crate::studies::PlotType::Area);
        let restored = serde_json::from_str::<StudyTemplate>(CORRELATION_ZERO_GUIDE)
            .unwrap()
            .create()
            .unwrap();
        assert_eq!(
            restored.settings().plots[0].plot_type,
            crate::studies::PlotType::LineWithBreaks
        );

        // An auto anchored VWAP as a chart saved it before the study had bands, its line edited
        // to orange at two pixels and its length to 30: it draws that line alone, and the hidden
        // bands are tagged as on a study added now.
        let saved = r#"{"name":"VWAP Auto Anchored","inputs":[["anchor",{"Choice":0}],["length",{"Length":30}],["source",{"Choice":5}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VwapAutoAnchored.html","plots":[{"color":[230,81,0,255],"width":2.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[2,9,14,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let saved: StudyTemplate = serde_json::from_str(saved).unwrap();
        let restored = saved.create().unwrap();
        assert_eq!(restored.plot_style(0), saved.settings.plots[0]);
        let lines = 0..restored.line_count();
        let drawn: Vec<_> = lines
            .clone()
            .filter(|&line| restored.plot_style(line).visible)
            .collect();
        let tagged: Vec<_> = lines.filter(|&line| restored.plot_tagged(line)).collect();
        assert_eq!((drawn, tagged), (vec![0], vec![0, 1, 2]));
        assert!(restored.fills().is_empty());
        let length = restored.inputs().into_iter().find(|i| i.id == "length");
        assert_eq!(length.unwrap().value, InputValue::Length(30));

        // A Rank Correlation as the release before its levels saved it draws none of them and
        // fills no band, one saved before it had an average draws none, and one saved since draws
        // the average, all three levels and the band.
        let older: StudyTemplate = serde_json::from_str(
            r#"{"name":"RankCorr","inputs":[["period",{"Length":14}],["smoothing",{"Choice":0}],["smoothing_length",{"Length":14}],["smoothing_deviation",{"Number":2.0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.RankCorrelationIndex.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
        )
        .unwrap();
        let shown = |study: &dyn Indicator| {
            let levels = study.pane().levels.into_iter();
            levels
                .filter(|level| level.1 != Color32::TRANSPARENT)
                .count()
        };
        let drawn = |template: &StudyTemplate| {
            let study = template.create().unwrap();
            let levels = (study.settings().levels.len(), shown(&*study));
            (study.line_count(), levels, study.pane().fill_between_levels)
        };
        assert_eq!(drawn(&older), (1, (3, 0), false));
        let mut oldest = older.clone();
        oldest.inputs.truncate(1);
        assert_eq!(drawn(&oldest), (1, (3, 0), false));
        let current = StudyTemplate::from_study(&RankCorrelationIndex::default());
        assert_eq!(drawn(&current), (2, (3, 3), true));

        // A Majority Rule saved before its guide at 50 draws none, one saved since draws it, and
        // one in direction mode has none to draw.
        let current = StudyTemplate::from_study(&crate::studies::MajorityRule::default());
        let mut older = current.clone();
        older.settings.levels.clear();
        assert_eq!(drawn(&older), (1, (1, 0), false));
        assert_eq!(drawn(&current), (1, (1, 1), false));
        let direction = crate::studies::MajorityRule::default()
            .with_mode(crate::studies::MajorityMode::Direction);
        assert_eq!(
            drawn(&StudyTemplate::from_study(&direction)),
            (1, (0, 0), false)
        );
        // An SMI, a Stochastic RSI and a Connors RSI as releases before their guides saved them
        // draw none and fill no band, and the SMI none of its shading either; a Stochastic saved
        // before its guides had styles draws its 80 and 20 guides without a band. Shown, each
        // guide, band and shaded zone takes the style of a study added now.
        let smi = r#"{"name":"SMI","inputs":[["period",{"Length":10}],["smooth1",{"Length":3}],["smooth2",{"Length":3}],["signal_period",{"Length":10}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.StochasticMomentumIndex.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[255,152,0,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[]},"visible":true,"scale":"Independent"}"#;
        let stoch_rsi = r#"{"name":"StochRSI","inputs":[["rsi_period",{"Length":14}],["stoch_period",{"Length":14}],["smooth_k",{"Length":3}],["smooth_d",{"Length":3}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.StochasticRSI.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[255,109,0,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[]},"visible":true,"scale":"Independent"}"#;
        let crsi = r#"{"name":"Connors RSI","inputs":[["rsi_period",{"Length":3}],["streak_period",{"Length":2}],["rank_period",{"Length":100}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ConnorsRSI.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[]},"visible":true,"scale":"Independent"}"#;
        let stochastic = r#"{"name":"Stochastic","inputs":[["k_period",{"Length":14}],["k_smooth",{"Length":3}],["d_period",{"Length":3}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":null,"plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[255,109,0,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":2,"hidden_intervals":[]},"visible":true,"scale":"Independent"}"#;
        for (saved, declared, guides, lines) in [
            (smi, 3, 0, 2),
            (stoch_rsi, 3, 0, 2),
            (crsi, 2, 0, 1),
            (stochastic, 3, 2, 2),
        ] {
            let older: StudyTemplate = serde_json::from_str(saved).unwrap();
            assert_eq!(
                drawn(&older),
                (lines, (declared, guides), false),
                "{}",
                older.name
            );
            let fresh = ConfiguredStudy::new(IndicatorFactory::new().create(&older.name).unwrap())
                .settings();
            let mut shown = older.create().unwrap().settings();
            for level in &mut shown.levels {
                level.visible = true;
            }
            for fill in &mut shown.gradients {
                fill.visible = true;
            }
            assert_eq!(
                (shown.levels, shown.level_fill_color, shown.gradients),
                (fresh.levels, fresh.level_fill_color, fresh.gradients),
                "{}",
                older.name
            );
        }
        // Its pre-shading save restored, an SMI draws neither shaded zone.
        let older: StudyTemplate = serde_json::from_str(smi).unwrap();
        assert!(
            older
                .create()
                .unwrap()
                .gradients()
                .iter()
                .all(|fill| fill.colors == [Color32::TRANSPARENT; 2])
        );

        // A Donchian channel as the release before its band saved it fills none, and the hidden
        // band keeps its declared colour.
        let saved = r#"{"name":"Donchian","inputs":[["period",{"Length":20}],["offset",{"Integer":0}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.DonchianChannels.html","plots":[{"color":[255,109,0,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[16,7,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let restored = serde_json::from_str::<StudyTemplate>(saved)
            .unwrap()
            .create()
            .unwrap();
        assert!(restored.fills().is_empty());
        let fresh = ConfiguredStudy::new(IndicatorFactory::new().create("Donchian").unwrap());
        let mut band = restored.settings().fills;
        band[0].visible = true;
        assert_eq!(band, fresh.settings().fills);

        // HMA, Advance/Decline Line and MA Channel as the release before their default colours
        // changed saved them keep the colours they drew, and an MA Channel saved before its band
        // had a style fills it as faintly as that release did.
        let hma = r#"{"name":"HMA","inputs":[["period",{"Length":9}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.HMA.html","plots":[{"color":[33,150,243,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[2,9,15,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let breadth = r#"{"name":"Advance/Decline Line","inputs":[["advancing_symbol",{"Symbol":"ADVANCING_ISSUES"}],["declining_symbol",{"Symbol":"DECLINING_ISSUES"}],["timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/type.AdvanceDeclineLine.html","plots":[{"color":[33,150,243,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[2,9,15,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;
        let channel = r#"{"name":"MA Channel","inputs":[["upper_length",{"Length":20}],["upper_offset",{"Integer":0}],["lower_length",{"Length":20}],["lower_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.MAChannel.html","plots":[{"color":[33,150,243,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[255,109,0,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[2,9,15,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[{"color":[2,8,12,13],"inverted_color":null,"visible":true,"independent":true}],"gradients":[]},"visible":true,"scale":"Main"}"#;
        for saved in [hma, breadth, channel] {
            let saved: StudyTemplate = serde_json::from_str(saved).unwrap();
            let restored = saved.create().unwrap();
            assert_eq!(restored.settings(), saved.settings, "{}", saved.name);
        }
        let mut older: StudyTemplate = serde_json::from_str(channel).unwrap();
        let band = std::mem::take(&mut older.settings.fills);
        assert_eq!(older.create().unwrap().settings().fills, band);

        // Bollinger Bands as a release before bands ignored their boundary plots saved them, the
        // upper and lower lines hidden and the band switch on: no band is drawn, and its switch
        // is off. Saved since with the same lines hidden, the band stays drawn.
        let saved = r#"{"name":"BB","inputs":[["period",{"Length":20}],["basis",{"Choice":0}],["std_dev",{"Number":2.0}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.BollingerBands.html","plots":[{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[8,153,129,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":false}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[15,3,4,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[{"color":[2,8,12,13],"inverted_color":null,"visible":true}],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let restored = serde_json::from_str::<StudyTemplate>(saved)
            .unwrap()
            .create()
            .unwrap();
        assert!(restored.fills().is_empty());
        assert!(!restored.settings().fills[0].visible);
        let mut since = ConfiguredStudy::new(IndicatorFactory::new().create("BB").unwrap());
        let mut settings = since.settings();
        settings.plots[0].visible = false;
        settings.plots[2].visible = false;
        since.apply_settings(&[], settings).unwrap();
        let again = StudyTemplate::from_study(&since).create().unwrap();
        assert_eq!(again.fills().len(), 1);
        let again = StudyTemplate::from_study(again.as_ref()).create().unwrap();
        assert_eq!(again.fills().len(), 1);

        // A Technical Ratings as the release before its table inputs saved it, its fast length
        // edited to 20 and its summary to orange: it upgrades its components, requests no other
        // interval and draws its table where and as it did.
        let saved: StudyTemplate = serde_json::from_str(
            r#"{"name":"Technical Ratings","inputs":[["fast",{"Length":20}],["medium",{"Length":50}],["slow",{"Length":200}],["simple",{"Toggle":true}],["exponential",{"Toggle":true}],["rsi",{"Toggle":true}],["rsi_period",{"Length":14}],["stochastic",{"Toggle":true}],["stochastic_period",{"Length":14}],["cci",{"Toggle":true}],["cci_period",{"Length":20}],["macd",{"Toggle":true}],["macd_fast",{"Length":12}],["macd_slow",{"Length":26}],["macd_signal",{"Length":9}],["momentum",{"Toggle":true}],["momentum_period",{"Length":10}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TechnicalRatings.html","plots":[{"color":[168,173,188,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":true},{"color":[168,173,188,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":true},{"color":[230,81,0,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":true},{"color":[168,173,188,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":false},{"color":[168,173,188,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":false},{"color":[168,173,188,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":false}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[{"value":null,"color":[46,78,123,128],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":null,"color":[120,77,77,128],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":false,"level_fill_color":[11,11,12,16],"untagged_plots":[0,1,2,3,4,5],"price_line_plots":[],"plot_variants":[[],[],[],[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
        )
        .unwrap();
        let mut restored = saved.create().unwrap();
        let mut plots = saved.settings.plots.clone();
        plots[0].visible = false;
        plots[1].visible = false;
        assert_eq!(restored.settings().plots, plots);
        assert!(restored.data_requests().is_empty());
        let inputs = restored.inputs();
        let value = |id: &str| inputs.iter().find(|i| i.id == id).unwrap().value.clone();
        assert!(!inputs.iter().any(|input| input.id == "fast"));
        assert_eq!(value("basis"), InputValue::Choice(0));
        let bars: Vec<_> = (0..260)
            .map(|i| {
                Bar::new(
                    chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::days(i),
                    100.0 + i as f64,
                    100.25 + i as f64,
                    99.75 + i as f64,
                    100.0 + i as f64,
                    1.0,
                )
            })
            .collect();
        let mut fresh = ConfiguredStudy::new(Box::new(crate::studies::TechnicalRatings::default()));
        restored.calculate(&bars);
        fresh.calculate(&bars);
        for at in 0..bars.len() {
            for line in 0..6 {
                assert_eq!(
                    restored.values().value(line, at),
                    fresh.values().value(line, at)
                );
            }
        }
        restored.calculate(&[]);
        let table = restored.table().unwrap();
        assert_eq!(
            (
                table.position,
                table.text_size,
                table.header_color,
                table.text_color
            ),
            (
                crate::studies::TablePosition::BottomRight,
                crate::studies::TableTextSize::Normal,
                Some(Color32::from_rgb(41, 91, 121)),
                Some(Color32::WHITE)
            )
        );
        assert_eq!(table.rows.len(), 1);
        assert_eq!(
            table.columns,
            [
                "Interval",
                "Moving averages",
                "Oscillators",
                "Summary",
                "Coverage"
            ]
        );
        // The same study as saved once it had table inputs, its buy and neutral colours edited,
        // keeps the colours of its columns: those inputs colour its table alone.
        let saved: StudyTemplate = serde_json::from_str(
            r#"{"name":"Technical Ratings","inputs":[["fast",{"Length":10}],["medium",{"Length":50}],["slow",{"Length":200}],["simple",{"Toggle":true}],["exponential",{"Toggle":true}],["rsi",{"Toggle":true}],["rsi_period",{"Length":14}],["stochastic",{"Toggle":true}],["stochastic_period",{"Length":14}],["cci",{"Toggle":true}],["cci_period",{"Length":20}],["macd",{"Toggle":true}],["macd_fast",{"Length":12}],["macd_slow",{"Length":26}],["macd_signal",{"Length":9}],["momentum",{"Toggle":true}],["momentum_period",{"Length":10}],["basis",{"Choice":0}],["table_intervals",{"Toggle":false}],["table_position",{"Choice":8}],["table_size",{"Choice":2}],["table_strong_buy_color",{"Color":[41,98,255,255]}],["table_buy_color",{"Color":[0,255,0,255]}],["table_neutral_color",{"Color":[255,215,0,255]}],["table_sell_color",{"Color":[239,154,154,255]}],["table_strong_sell_color",{"Color":[244,67,54,255]}],["table_header_color",{"Color":[41,91,121,255]}],["table_text_color",{"Color":[255,255,255,255]}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TechnicalRatings.html","plots":[{"color":[168,173,188,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":true},{"color":[168,173,188,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":true},{"color":[168,173,188,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":true},{"color":[168,173,188,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":false},{"color":[168,173,188,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":false},{"color":[168,173,188,255],"width":1.0,"pattern":"Solid","plot_type":"Columns","visible":false}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[{"value":null,"color":[46,78,123,128],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":null,"color":[120,77,77,128],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":false,"level_fill_color":[11,11,12,16],"untagged_plots":[0,1,2,3,4,5],"price_line_plots":[],"plot_variants":[[],[],[],[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
        )
        .unwrap();
        let mut restored = saved.create().unwrap();
        restored.calculate(&bars);
        for at in 0..bars.len() {
            assert_eq!(
                restored.plot_color(2, at),
                fresh.plot_color(2, at),
                "bar {at}"
            );
        }
        restored.calculate(&[]);
        assert_eq!(
            restored.table().unwrap().cell_colors,
            [vec![
                Color32::from_rgb(41, 91, 121),
                Color32::GOLD,
                Color32::GOLD,
                Color32::GOLD
            ]]
        );
        for basis in [0, 1, 2] {
            let mut study =
                ConfiguredStudy::new(Box::new(crate::studies::TechnicalRatings::default()));
            study.set_input("basis", InputValue::Choice(basis)).unwrap();
            assert_eq!(
                study
                    .settings()
                    .plots
                    .iter()
                    .map(|plot| plot.visible)
                    .collect::<Vec<_>>(),
                [false, false, true, false, false, false]
            );
            for (id, value) in [
                ("table_intervals", InputValue::Toggle(true)),
                ("table_timeframes [0]", InputValue::Timeframe(None)),
                (
                    "table_timeframes [1]",
                    InputValue::Timeframe(Some(crate::model::Timeframe::Min15)),
                ),
                (
                    "calculation_timeframe",
                    InputValue::Timeframe(Some(crate::model::Timeframe::Hour1)),
                ),
            ] {
                study.set_input(id, value).unwrap();
            }
            let saved = StudyTemplate::from_study(&study);
            let json = serde_json::to_string(&saved).unwrap();
            let again = serde_json::from_str::<StudyTemplate>(&json)
                .unwrap()
                .create()
                .unwrap();
            assert_eq!(
                serde_json::to_string(&StudyTemplate::from_study(again.as_ref())).unwrap(),
                json
            );
        }

        // Ziv Ghost Pivots opens with a character for each pivot. One saved while the pivots
        // were crosses keeps them, and its plots still name their characters for the Style tab.
        use crate::studies::PlotType;
        let factory = IndicatorFactory::new();
        let fresh = ConfiguredStudy::new(factory.create("Ziv Ghost Pivots").unwrap());
        let kinds: Vec<_> = fresh
            .settings()
            .plots
            .iter()
            .map(|plot| plot.plot_type)
            .collect();
        assert_eq!(
            kinds,
            [
                PlotType::Character('8'),
                PlotType::Character('d'),
                PlotType::Character('w'),
                PlotType::Character('m'),
                PlotType::Line,
            ]
        );
        // Saved on origin/main ebd125251.
        let saved: StudyTemplate = serde_json::from_str(
            r#"{"name":"Ziv Ghost Pivots","inputs":[["show_four_hour",{"Toggle":true}],["show_day",{"Toggle":true}],["show_week",{"Toggle":true}],["show_month",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ZivGhostPivots.html","plots":[{"color":[0,188,212,255],"width":1.0,"pattern":"Solid","plot_type":"Crosses","visible":true},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Crosses","visible":true},{"color":[103,58,183,255],"width":1.0,"pattern":"Solid","plot_type":"Crosses","visible":true},{"color":[233,30,99,255],"width":1.0,"pattern":"Solid","plot_type":"Crosses","visible":true},{"color":[0,188,212,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[0,12,13,16],"untagged_plots":[0,1,2,3,4],"price_line_plots":[],"plot_variants":[[],[],[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
        )
        .unwrap();
        let restored = saved.create().unwrap();
        assert_eq!(restored.settings().plots, saved.settings.plots);
        assert_eq!(restored.plot_style(1).plot_type, PlotType::Crosses);
        assert_eq!(restored.plot_character(1), Some('d'));

        // Booker Reversal opens with teal buys and red sells at half opacity and levels that
        // take decimals. One saved with green and red marks and whole levels keeps its colours,
        // restores the same levels as decimals and keeps the price scale tags it was saved with.
        let marks = |study: &dyn Indicator| {
            (0..2)
                .map(|line| study.plot_style(line))
                .map(|plot| (plot.color, plot.plot_type))
                .collect::<Vec<_>>()
        };
        let levels = |study: &dyn Indicator| {
            let inputs = study.inputs();
            ["stoch_upper", "stoch_lower"].map(|id| {
                let input = inputs.iter().find(|input| input.id == id);
                input.map(|input| input.value.clone())
            })
        };
        let fresh = ConfiguredStudy::new(factory.create("Booker Reversal").unwrap());
        assert_eq!(
            marks(&fresh),
            [
                (
                    Color32::from_rgba_unmultiplied(0, 150, 136, 128),
                    PlotType::TriangleUp
                ),
                (
                    Color32::from_rgba_unmultiplied(244, 67, 54, 128),
                    PlotType::TriangleDown
                ),
            ]
        );
        assert_eq!(
            levels(&fresh),
            [70.0, 30.0].map(|level| Some(InputValue::Number(level)))
        );
        // Saved on origin/main ebd125251, with the levels set to 72 and 25.
        let saved: StudyTemplate = serde_json::from_str(
            r#"{"name":"Booker Reversal","inputs":[["fast_period",{"Length":12}],["slow_period",{"Length":26}],["k_period",{"Length":70}],["slowing",{"Length":10}],["stoch_upper",{"Length":72}],["stoch_lower",{"Length":25}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.BookerReversal.html","plots":[{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleUp","visible":true},{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"TriangleDown","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[5,11,5,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
        )
        .unwrap();
        let restored = saved.create().unwrap();
        assert_eq!(restored.settings().plots, saved.settings.plots);
        assert!((0..2).all(|line| restored.plot_tagged(line)));
        assert_eq!(
            marks(restored.as_ref()),
            [
                (Color32::from_rgb(76, 175, 80), PlotType::TriangleUp),
                (Color32::from_rgb(242, 54, 69), PlotType::TriangleDown),
            ]
        );
        assert_eq!(
            levels(restored.as_ref()),
            [72.0, 25.0].map(|level| Some(InputValue::Number(level)))
        );
    }

    /// A Connors RSI saved with guides at 70, 50 and 30 and a Correlation Coefficient saved with
    /// guides at 1, -1 and 0, as they opened before they dropped the middle guide and the bounds,
    /// restore those guides with their saved styles and band and keep them when saved again. A
    /// Correlation Coefficient saved with its zero guide alone keeps that guide; one added now has
    /// only that guide.
    #[test]
    fn studies_saved_with_guides_they_no_longer_open_with_keep_them() {
        let gray = crate::studies::palette::GRAY;
        let orange = Color32::from_rgb(255, 152, 0);
        assert_eq!(
            ConfiguredStudy::new(Box::new(crate::studies::Correlation::default()))
                .pane()
                .levels,
            [(0.0, gray.gamma_multiply_u8(128), "Zero")]
        );
        let crsi = r#"{"name":"Connors RSI","inputs":[["rsi_period",{"Length":3}],["streak_period",{"Length":2}],["rank_period",{"Length":100}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.ConnorsRSI.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":55.0,"color":[255,152,0,255],"pattern":"Dotted","width":3.0,"visible":true,"price_line":true},{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":false,"price_line":false}],"fill_between_levels":true,"level_fill_color":[3,15,24,26],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;
        let correlation = r#"{"name":"Correlation Coefficient","inputs":[["symbol_0",{"Symbol":""}],["source_0",{"Choice":3}],["symbol_1",{"Symbol":""}],["source_1",{"Choice":3}],["length",{"Length":20}],["log_returns",{"Toggle":false}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/type.Correlation.html","plots":[{"color":[33,150,243,255],"width":1.0,"pattern":"Solid","plot_type":"Area","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[{"value":null,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":false,"price_line":false},{"value":-0.5,"color":[120,123,134,255],"pattern":"Dashed","width":2.0,"visible":true,"price_line":false},{"value":null,"color":[255,152,0,255],"pattern":"Dotted","width":3.0,"visible":true,"price_line":true}],"fill_between_levels":true,"level_fill_color":[2,9,15,16],"untagged_plots":[],"price_line_plots":[0],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;
        for (saved, levels, fill) in [
            (
                crsi,
                vec![
                    (70.0, gray, "Upper guide"),
                    (55.0, orange, "Middle guide"),
                    (30.0, Color32::TRANSPARENT, "Lower guide"),
                ],
                Some([0, 2]),
            ),
            (
                correlation,
                vec![
                    (1.0, Color32::TRANSPARENT, "Upper guide"),
                    (-0.5, gray, "Lower guide"),
                    (0.0, orange, "Zero"),
                ],
                None,
            ),
            (CORRELATION_ZERO_GUIDE, vec![(0.0, orange, "Zero")], None),
        ] {
            let saved: StudyTemplate = serde_json::from_str(saved).unwrap();
            let restored = saved.create().unwrap();
            assert_eq!(
                restored.settings().levels,
                saved.settings.levels,
                "{}",
                saved.name
            );
            assert_eq!(restored.pane().levels, levels, "{}", saved.name);
            assert_eq!(restored.level_fill_indices(), fill, "{}", saved.name);
            let again = StudyTemplate::from_study(restored.as_ref())
                .create()
                .unwrap();
            assert_eq!(again.pane().levels, levels, "{}", saved.name);
        }
    }

    /// A host study registered under a built-in's name replaces it (`IndicatorFactory::register`),
    /// so its own template restores through that factory as that study, with the inputs and plots
    /// and fill styles it was saved with, also where an upgrade of the built-in's templates adds,
    /// renames or converts an input or adds fill styles.
    #[test]
    fn host_studies_named_like_upgraded_builtins_restore_from_their_templates() {
        use crate::studies::{CustomIndicator, IndicatorValue, InputKind};
        use InputValue::{Length, Number};
        let millis = || InputKind::Number(0.0, 1e13);
        let rows = [
            ("RSI", vec![], 1),
            (
                "Echo",
                vec![
                    (
                        "source",
                        InputValue::Source(crate::studies::StudySource::Price(PriceSource::HL2)),
                        InputKind::Source,
                    ),
                    (
                        "basis",
                        InputValue::Source(crate::studies::StudySource::Price(PriceSource::OHLC4)),
                        InputKind::Source,
                    ),
                    (
                        "symbol",
                        InputValue::Symbol("OTHER".into()),
                        InputKind::Symbol,
                    ),
                ],
                1,
            ),
            // Built-in templates with the former component inputs drop them.
            (
                "Technical Ratings",
                vec![("fast", Length(20), InputKind::Length(1, 500))],
                1,
            ),
            // Built-in templates with a band and no fill styles restore with hidden fills.
            (
                "VWAP",
                vec![("band_count", Length(1), InputKind::Length(0, 3))],
                3,
            ),
            ("VWAP Auto Anchored", vec![], 1),
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
                Box::new(
                    CustomIndicator::new(name, |_| Vec::new())
                        .with_overlay(name != "Echo")
                        .with_line_count(lines)
                        .with_inputs(schema.clone(), |bars, _, source, _| {
                            bars.iter()
                                .enumerate()
                                .map(|(at, bar)| {
                                    IndicatorValue::Single(
                                        source.get(at).copied().unwrap_or(bar.close),
                                    )
                                })
                                .collect()
                        })
                        .unwrap(),
                )
            });
        }
        for (name, inputs, lines) in rows {
            let mut original = factory.create(name).unwrap();
            if name == "Technical Ratings" {
                original.set_input("fast", Length(21)).unwrap();
            }
            // Echo is a literal chart study saved on origin/main before source routing changed.
            let saved = if name == "Echo" {
                serde_json::from_str(r#"{"name":"Echo","inputs":[["source",{"Source":{"Price":"HL2"}}],["basis",{"Source":{"Price":"OHLC4"}}],["symbol",{"Symbol":"OTHER"}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":null,"plots":[{"color":[240,145,40,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[15,9,3,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#).unwrap()
            } else {
                StudyTemplate::from_study(original.as_ref())
            };
            let mut restored = saved
                .create_with(&factory)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!((restored.name(), restored.line_count()), (name, lines));
            if name == "Echo" {
                assert_eq!(restored.settings(), saved.settings);
                assert_eq!(
                    restored.source(),
                    Some(crate::studies::StudySource::Price(PriceSource::HL2))
                );
                assert_eq!(restored.scale(), crate::studies::StudyScale::Independent);
                restored.calculate(&[Bar::new(
                    chrono::DateTime::UNIX_EPOCH,
                    3.0,
                    5.0,
                    1.0,
                    4.0,
                    1.0,
                )]);
                assert_eq!(restored.values().value(0, 0), Some(3.0));
            }
            let fills = StudyTemplate::from_study(restored.as_ref()).settings.fills;
            assert_eq!(fills, saved.settings.fills, "{name}");
            let restored = restored.inputs();
            for (id, value, _) in inputs {
                let value = if name == "Technical Ratings" && id == "fast" {
                    Length(21)
                } else {
                    value
                };
                let input = restored.iter().find(|input| input.id == id);
                assert_eq!(input.map(|input| &input.value), Some(&value), "{name} {id}");
            }
        }
    }

    /// An RSI or a Williams %R saved before it had a middle level draws the lines and band it was
    /// saved with, and no middle line. With its levels edited, the RSI's to 40 and 20 and the
    /// Williams %R's to -10 and -30, it draws those two and fills between them; saved before
    /// levels were editable it draws 70 and 30, or -20 and -80, and fills between them. Each fills
    /// in its band colour.
    #[test]
    fn saved_before_the_middle_level_draws_its_saved_levels_and_band() {
        let mut edited = ConfiguredStudy::new(Box::new(RSI::new(14)));
        let mut settings = edited.settings();
        settings.levels[0].value = Some(40.0);
        settings.levels[1].value = Some(20.0);
        edited.apply_settings(&[], settings).unwrap();
        let mut rsi_edited = StudyTemplate::from_study(&edited);
        rsi_edited
            .inputs
            .retain(|(id, _)| !id.starts_with("smoothing"));
        rsi_edited.settings.levels.truncate(2);
        // Settings of that age had no level fields, which read as their serde defaults.
        let older = |study: &dyn Indicator| {
            let mut older = StudyTemplate::from_study(study);
            older.settings.levels.clear();
            older.settings.fill_between_levels = false;
            older.settings.level_fill_color = Color32::default();
            older
        };
        let mut rsi_older = older(&RSI::new(14));
        rsi_older
            .inputs
            .retain(|(id, _)| !id.starts_with("smoothing"));
        // A Williams %R saved with its levels edited to -10 and -30, both above its middle level.
        let williams_edited: StudyTemplate = serde_json::from_str(
            r#"{"name":"Williams %R","inputs":[["period",{"Length":14}],["smoothing",{"Choice":0}],["smoothing_length",{"Length":14}],["smoothing_deviation",{"Number":2.0}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.WilliamsR.html","plots":[{"color":[126,87,194,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[{"value":-10.0,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false},{"value":-30.0,"color":[120,123,134,255],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":true,"level_fill_color":[13,9,20,26],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#,
        )
        .unwrap();
        let williams_older = older(&WilliamsR::new(14));
        // The lines drawn, the band, which a hidden level still bounds unless the study names the
        // two levels that bound it, and its fill.
        let drawn = |template: &StudyTemplate| {
            let study = template.create().unwrap();
            let pane = study.pane();
            let lines: Vec<f64> = pane
                .levels
                .iter()
                .filter(|level| level.1 != Color32::TRANSPARENT)
                .map(|level| level.0)
                .collect();
            let indices = study.level_fill_indices();
            let values = pane
                .levels
                .iter()
                .enumerate()
                .filter(|(index, _)| indices.is_none_or(|indices| indices.contains(index)))
                .map(|(_, level)| level.0);
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
        let fill = Color32::from_rgba_unmultiplied(126, 87, 194, 26);
        assert_eq!(
            [&rsi_edited, &rsi_older, &williams_edited, &williams_older].map(drawn),
            [
                (vec![40.0, 20.0], (20.0, 40.0), true, fill),
                (vec![70.0, 30.0], (30.0, 70.0), true, fill),
                (vec![-10.0, -30.0], (-30.0, -10.0), true, fill),
                (vec![-20.0, -80.0], (-80.0, -20.0), true, fill),
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

    /// Trading Sessions and VWAP templates saved while session hours were minute counts and time
    /// zones were text restore with the same hours and zones, now times of day and time zones.
    #[test]
    fn session_hours_saved_as_minute_counts_restore_as_times_and_zones() {
        // Saved on origin/main: the first session 570 to 960 at -05:00, and a VWAP whose zones
        // are -06:00 and +09:00 and whose trading day starts at 1020.
        let sessions = r#"{"name":"Trading Sessions","inputs":[["show [0]",{"Toggle":true}],["name [0]",{"Text":"Tokyo"}],["start [0]",{"Length":570}],["end [0]",{"Length":960}],["zone [0]",{"Text":"-05:00"}],["show [1]",{"Toggle":true}],["name [1]",{"Text":"London"}],["start [1]",{"Length":510}],["end [1]",{"Length":990}],["zone [1]",{"Text":"Europe/London"}],["show [2]",{"Toggle":true}],["name [2]",{"Text":"New York"}],["start [2]",{"Length":570}],["end [2]",{"Length":960}],["zone [2]",{"Text":"America/New_York"}],["boxes",{"Toggle":true}],["name_labels",{"Toggle":true}],["range_labels",{"Toggle":false}],["average_labels",{"Toggle":false}],["open_lines",{"Toggle":false}],["close_lines",{"Toggle":false}],["average_lines",{"Toggle":false}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TradingSessions.html","plots":[{"color":[6,15,38,38],"width":1.0,"pattern":"Solid","plot_type":"BackgroundColor","visible":true},{"color":[38,23,0,38],"width":1.0,"pattern":"Solid","plot_type":"BackgroundColor","visible":true},{"color":[1,23,19,38],"width":1.0,"pattern":"Solid","plot_type":"BackgroundColor","visible":true}],"precision":0,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[0,1,2,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let vwap = r#"{"name":"VWAP","inputs":[["hide_daily",{"Toggle":false}],["anchor",{"Choice":0}],["time_zone",{"Text":"-06:00"}],["day_start",{"Length":1020}],["percentage_bands",{"Toggle":false}],["band_count",{"Length":1}],["band_0",{"Number":1.0}],["event_time_zone",{"Text":"+09:00"}],["event_count",{"Length":0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.VolumeWeightedAvgPrice.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[{"color":[4,9,4,13],"inverted_color":null,"visible":true,"independent":true}],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let time =
            |hour, minute| InputValue::TimeOfDay(NaiveTime::from_hms_opt(hour, minute, 0).unwrap());
        let zone = |name: &str| InputValue::TimeZone(name.into());
        // 570 = 9 x 60 + 30, 960 = 16 x 60, 510 = 8 x 60 + 30, 990 = 16 x 60 + 30, 1020 = 17 x 60.
        // The other two sessions' zones stand in as fixed offsets without `timezones`.
        let cases = [
            (
                sessions,
                vec![
                    ("start [0]", time(9, 30)),
                    ("end [0]", time(16, 0)),
                    ("zone [0]", zone("-05:00")),
                    ("start [1]", time(8, 30)),
                    ("end [1]", time(16, 30)),
                    ("start [2]", time(9, 30)),
                    ("end [2]", time(16, 0)),
                ],
            ),
            (
                vwap,
                vec![
                    ("time_zone", zone("-06:00")),
                    ("day_start", time(17, 0)),
                    ("event_time_zone", zone("+09:00")),
                ],
            ),
        ];
        for (saved, expected) in cases {
            let study = serde_json::from_str::<StudyTemplate>(saved)
                .unwrap()
                .create()
                .unwrap();
            let inputs = study.inputs();
            for (id, value) in expected {
                let restored = inputs.iter().find(|input| input.id == id);
                assert_eq!(restored.map(|input| &input.value), Some(&value), "{id}");
            }
        }
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
        let mut factory = IndicatorFactory::new();
        for (name, inputs, restored_name, expected) in rows {
            factory.register("Host channel", move || {
                IndicatorFactory::new().create(name).unwrap()
            });
            for saved_as in [name, "Host channel"] {
                let settings =
                    ConfiguredStudy::new(factory.create(restored_name).unwrap()).settings();
                let template = StudyTemplate {
                    name: saved_as.to_owned(),
                    inputs: inputs
                        .iter()
                        .chain(&configured)
                        .map(|(id, value)| ((*id).to_owned(), value.clone()))
                        .collect(),
                    settings,
                    visible: true,
                    scale: None,
                    metric: None,
                };
                let mut study = template
                    .create_with(&factory)
                    .unwrap_or_else(|error| panic!("{saved_as} {name}: {error}"));
                study.calculate(&bars);
                assert_eq!(study.name(), restored_name, "{saved_as}");
                assert!(
                    matches!(study.values().to_rows()[0], IndicatorValue::None),
                    "{saved_as} {name}"
                );
                for (bar, lines) in (1..).zip(expected) {
                    for (line, want) in lines.into_iter().enumerate() {
                        let got = study.values().value(line, bar).unwrap();
                        assert!(
                            (got - want).abs() < 1e-12,
                            "{saved_as} {name} bar {bar} line {line}: {got}"
                        );
                    }
                }
            }
        }
    }
}
