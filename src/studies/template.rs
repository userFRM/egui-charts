//! Serializable inputs and presentation of one study.

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

impl StudyTemplate {
    /// Capture the current calculation inputs, presentation and value scale, without computed values.
    /// Hosts remap chart-local study source ids when saving or restoring a set of studies.
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
        let study = factory
            .create(&self.name)
            .ok_or_else(|| format!("Unknown study: {}", self.name))?;
        let mut study = ConfiguredStudy::new(study);
        study
            .apply_settings(&self.inputs, self.settings.clone())
            .map_err(str::to_owned)?;
        study.set_visible(self.visible);
        if let Some(scale) = self.scale {
            study.set_scale(scale);
        }
        Ok(Box::new(study))
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
    }
}
