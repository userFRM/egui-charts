//! Drawing clipboard uses the existing validated, timestamp-based storage format; the study
//! clipboard holds a study's name, inputs and presentation.

use crate::drawings::persistence::{DrawingStorage, SaveError, stored_to_drawing};
use crate::model::index_at;
use crate::studies::{InputValue, StudyScale, StudySettings};
use crate::widget::{Chart, SeriesId};

const PREFIX: &str = "egui-charts-drawings\n";
const STUDY_PREFIX: &str = "egui-charts-study\n";

/// A copied study, recreated by its catalogue name.
#[derive(serde::Serialize, serde::Deserialize)]
struct CopiedStudy {
    name: String,
    inputs: Vec<(String, InputValue)>,
    settings: StudySettings,
    overlay: bool,
    scale: StudyScale,
}

impl Chart {
    /// Copy the selected drawings as versioned text for the host's system clipboard.
    /// Returns `None` for an empty selection. Anchors retain timestamps across intervals.
    pub fn copy_selected_drawings(&self) -> Result<Option<String>, SaveError> {
        let ids: Vec<_> = self.drawings.selected_ids().collect();
        if ids.is_empty() {
            return Ok(None);
        }
        let storage = self.store_drawings(
            self.drawings
                .drawings
                .iter()
                .filter(|d| ids.contains(&d.id)),
        )?;
        Ok(Some(format!("{PREFIX}{}", storage.to_json()?)))
    }

    /// Paste drawing clipboard text as one undoable addition, with new identities and a
    /// three-bar offset. Selects the copies. Rejects invalid or partially unreadable data,
    /// another symbol, or a chart without a time mapping before changing any drawing.
    /// The host handles the system clipboard and forwards its paste event here.
    pub fn paste_drawings(&mut self, text: &str) -> Result<(), serde_json::Error> {
        let json = text
            .strip_prefix(PREFIX)
            .ok_or_else(|| serde::de::Error::custom("not a drawing clipboard"))?;
        let (storage, skipped) = DrawingStorage::from_json(json)?;
        if storage.symbol != self.symbol() || !skipped.is_empty() {
            return Err(serde::de::Error::custom(
                "clipboard has another symbol or unreadable drawings",
            ));
        }
        let mut copies = Vec::new();
        for stored in storage.drawings() {
            for point in &stored.points {
                if chrono::DateTime::from_timestamp_millis(point.ts_ms)
                    .and_then(|time| index_at(self.state.shown(), time))
                    .is_none()
                {
                    return Err(serde::de::Error::custom(
                        "clipboard needs loaded chart history",
                    ));
                }
            }
            copies.push(stored_to_drawing(
                stored,
                |ts| {
                    index_at(
                        self.state.shown(),
                        chrono::DateTime::from_timestamp_millis(ts).expect("validated timestamp"),
                    )
                    .expect("validated time mapping") as f32
                },
                0,
            ));
        }
        self.user_action(|chart| chart.drawings.paste_drawings(&copies));
        Ok(())
    }

    /// Copy study `id` with its inputs, styles, interval visibility, placement and scale as
    /// versioned text for the host's system clipboard. Returns `None` for an unknown id.
    pub fn copy_indicator(&self, id: SeriesId) -> Option<String> {
        let (_, study) = self.indicators().find(|(study, _)| *study == id)?;
        let copied = CopiedStudy {
            name: study.name().to_owned(),
            inputs: study
                .inputs()
                .into_iter()
                .map(|input| (input.id, input.value))
                .collect(),
            settings: study.settings(),
            overlay: study.is_overlay(),
            scale: study.scale(),
        };
        serde_json::to_string(&copied)
            .ok()
            .map(|json| format!("{STUDY_PREFIX}{json}"))
    }

    /// Paste study clipboard text from [`Self::copy_indicator`], on this chart or another, as
    /// one undoable addition, and return the new study's id. `factory` creates the study by its
    /// name. Rejects other text, a name `factory` does not know, and inputs or styles the study
    /// does not accept, before changing the chart. Study output references need host remapping
    /// and are rejected; hosts restore those inputs through [`Self::add_indicator`].
    #[cfg(feature = "studies")]
    #[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
    pub fn paste_indicator(
        &mut self,
        text: &str,
        factory: &crate::studies::IndicatorFactory,
    ) -> Result<SeriesId, serde_json::Error> {
        use crate::studies::Indicator;
        use serde::de::Error;
        let json = text
            .strip_prefix(STUDY_PREFIX)
            .ok_or_else(|| serde_json::Error::custom("not a study clipboard"))?;
        let copied: CopiedStudy = serde_json::from_str(json)?;
        if copied.inputs.iter().any(|(_, value)| {
            matches!(
                value,
                InputValue::Source(crate::studies::StudySource::Study { .. })
            )
        }) {
            return Err(serde_json::Error::custom(
                "study output references need host remapping before paste",
            ));
        }
        let mut study = crate::studies::ConfiguredStudy::new(
            factory
                .create(&copied.name)
                .ok_or_else(|| serde_json::Error::custom("unknown study"))?,
        );
        study
            .apply_settings(&copied.inputs, copied.settings)
            .map_err(serde_json::Error::custom)?;
        study.set_overlay(copied.overlay);
        study.set_scale(copied.scale);
        Ok(self.user_action(|chart| chart.add_indicator(Box::new(study))))
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use crate::studies::{IndicatorFactory, StudyScale};
    use crate::widget::test_frames;

    /// A copied study pastes onto another chart with its inputs, styles, interval visibility,
    /// placement and scale, as one undoable addition.
    #[test]
    fn copied_studies_paste_with_their_settings() {
        let mut source = test_frames::chart(&[100.0], &[110.0]);
        let id = source.add_indicator(Box::new(crate::studies::SMA::new(7)));
        let study = source.indicator_mut(id).unwrap();
        let mut settings = study.settings();
        settings.plots[0].width = 3.0;
        settings
            .hidden_intervals
            .push(crate::model::Timeframe::Min1);
        study.apply_settings(&[], settings).unwrap();
        source.move_indicator_to_pane(id, id);
        source.set_indicator_scale(id, StudyScale::None);
        let text = source.copy_indicator(id).unwrap();

        let mut target = test_frames::chart(&[50.0], &[60.0]);
        let factory = IndicatorFactory::new();
        assert!(target.paste_indicator("SMA", &factory).is_err());
        let pasted = target.paste_indicator(&text, &factory).unwrap();
        let find = |chart: &crate::widget::Chart, id| {
            chart
                .indicators()
                .find(|(study, _)| *study == id)
                .unwrap()
                .1
                .clone_box()
        };
        let (copied, pasted) = (find(&source, id), find(&target, pasted));
        let inputs = |study: &dyn crate::studies::Indicator| {
            study
                .inputs()
                .into_iter()
                .map(|input| (input.id, input.value))
                .collect::<Vec<_>>()
        };
        assert!(!inputs(copied.as_ref()).is_empty());
        assert_eq!(inputs(pasted.as_ref()), inputs(copied.as_ref()));
        assert_eq!(pasted.settings(), copied.settings());
        assert!(!pasted.is_overlay());
        assert_eq!(pasted.scale(), StudyScale::None);
        assert!(target.can_undo());
        let parent = source.add_indicator(Box::new(crate::studies::RSI::new(2)));
        source
            .set_indicator_source(
                id,
                crate::studies::StudySource::Study {
                    id: parent.0,
                    plot: 0,
                },
            )
            .unwrap();
        let text = source.copy_indicator(id).unwrap();
        let unrelated = target.add_indicator(Box::new(crate::studies::EMA::new(2)));
        assert_eq!(unrelated.0, parent.0);
        let before: Vec<_> = target.indicators().map(|(id, _)| id).collect();
        assert!(target.paste_indicator(&text, &factory).is_err());
        assert_eq!(
            target.indicators().map(|(id, _)| id).collect::<Vec<_>>(),
            before
        );
    }
}
