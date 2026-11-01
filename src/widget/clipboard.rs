//! Drawing clipboard uses the existing validated, timestamp-based storage format; the study
//! clipboard holds a study's name, inputs and presentation.

use crate::drawings::persistence::{DrawingStorage, LoadError, SaveError, stored_to_drawing_with};
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
    /// three-bar offset. Copies belong to this chart's symbol; timestamps and study panes are
    /// mapped against this chart. Selects the copies. Before changing any drawing, rejects text
    /// that is not drawing clipboard text ([`LoadError::NotClipboard`]), invalid data or a
    /// version from a newer release, drawings that could not all be read ([`LoadError::Skipped`])
    /// and a chart without the history to place them ([`LoadError::NeedsHistory`]). The host
    /// handles the system clipboard and forwards its paste event here. Copies of a disabled
    /// tool, and copies that saving would refuse, are left out.
    pub fn paste_drawings(&mut self, text: &str) -> Result<(), LoadError> {
        let json = text.strip_prefix(PREFIX).ok_or(LoadError::NotClipboard)?;
        let (storage, skipped) = DrawingStorage::from_json(json)?;
        if !skipped.is_empty() {
            return Err(LoadError::Skipped(skipped));
        }
        let mut copies = Vec::new();
        for stored in storage.drawings() {
            for point in &stored.points {
                if point
                    .time()
                    .and_then(|time| self.time_to_index(time))
                    .is_none()
                {
                    return Err(LoadError::NeedsHistory);
                }
            }
            let mut copy = stored_to_drawing_with(
                stored,
                |point| {
                    self.time_to_index(point.time().expect("validated timestamp"))
                        .expect("validated time mapping") as f32
                },
                0,
            );
            if let Some(pane) = &stored.pane {
                copy.pane = self.stored_pane_owner(pane);
            }
            copies.push(copy);
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
    /// name. Rejects a paste beyond the [study cap](Self::set_study_limit), other text, a name
    /// `factory` does not know, and inputs or styles the study does not accept, before changing
    /// the chart. Study output references need host remapping and are rejected; hosts restore
    /// those inputs through [`Self::add_indicator`].
    #[cfg(feature = "studies")]
    #[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
    pub fn paste_indicator(
        &mut self,
        text: &str,
        factory: &crate::studies::IndicatorFactory,
    ) -> Result<SeriesId, serde_json::Error> {
        use crate::studies::Indicator;
        use serde::de::Error;
        if self.study_limit_reached() {
            return Err(serde_json::Error::custom(Self::STUDY_LIMIT_REACHED));
        }
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

    /// Clipboard paste maps times into the destination before offsetting, keeps properties,
    /// and adds the whole selection to that symbol as one undo step.
    #[test]
    fn drawings_paste_across_symbols_with_destination_anchors_and_history() {
        use crate::drawings::persistence::LoadError;
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType, ToolAvailability};
        use crate::model::{BarData, LineStyle};
        use crate::widget::{Chart, SeriesId};
        use chrono::{Duration, TimeZone, Utc};

        let mut source = test_frames::chart(&[100.0], &[110.0]);
        source.set_symbol("SOURCE");
        for (start, price) in [(20.5, 105.0), (40.0, 107.0)] {
            let mut line = Drawing::new(0, DrawingToolType::TrendLine);
            line.chart_points = vec![
                ChartPoint::new(start, price),
                ChartPoint::new(start + 10.0, price + 1.0),
            ];
            line.color = [20, 80, 160, 200];
            line.stroke_width = 4.0;
            line.line_style = LineStyle::Dashed;
            line.label = Some("Copied line".into());
            line.notes = Some("Keep these notes".into());
            source.drawings_mut().add_drawing(line).unwrap();
        }
        let ids: Vec<_> = source.drawings().drawings.iter().map(|d| d.id).collect();
        source.drawings_mut().select_drawings(ids);
        let copied = source.copy_selected_drawings().unwrap().unwrap();
        let saved = source.save_drawings().unwrap().to_json().unwrap();
        let mut target = Chart::new(BarData::new());
        target.set_symbol("DESTINATION");
        assert!(matches!(
            target.paste_drawings(&copied),
            Err(LoadError::NeedsHistory)
        ));
        assert!(target.drawings().drawings.is_empty());
        assert!(!target.can_undo());
        // Destination starts ten days later: source index 20.5 maps to 10.5, then 13.5.
        target.set_data(SeriesId::MAIN, source.state().data().bars[10..].to_vec());
        assert!(matches!(
            target.load_drawings(&saved),
            Err(LoadError::OtherSymbol)
        ));
        target.paste_drawings(&copied).unwrap();
        assert_eq!(target.drawings().drawings.len(), 2);
        let epoch = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        for (original, pasted) in source
            .drawings()
            .drawings
            .iter()
            .zip(&target.drawings().drawings)
        {
            assert!(target.drawings().is_selected(pasted.id));
            assert!(pasted.stored_id.is_none());
            assert!(pasted.created_at.is_none());
            assert_eq!(pasted.notes, original.notes);
            let mut expected = original.clone();
            expected.z_order = pasted.z_order;
            assert_eq!(
                serde_json::to_value(pasted).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
            for (from, to) in original.chart_points.iter().zip(&pasted.chart_points) {
                assert_eq!(to.bar_idx, from.bar_idx - 10.0 + 3.0);
                assert_eq!(to.price, from.price);
                // Daily source bars start at epoch; the ordinary offset adds three days.
                assert_eq!(
                    target.drawing_point_time(to),
                    Some(
                        epoch + Duration::seconds(((from.bar_idx as f64 + 3.0) * 86_400.0) as i64)
                    )
                );
            }
        }
        let pasted_ids: Vec<_> = target.drawings().drawings.iter().map(|d| d.id).collect();
        assert_ne!(pasted_ids[0], pasted_ids[1]);
        let pasted = serde_json::to_value(target.save_drawings().unwrap()).unwrap();
        assert_eq!(pasted["symbol"], "DESTINATION");
        assert!(target.undo());
        assert!(target.drawings().drawings.is_empty());
        assert!(!target.can_undo());
        assert!(target.redo());
        assert_eq!(
            target
                .drawings()
                .drawings
                .iter()
                .map(|d| d.id)
                .collect::<Vec<_>>(),
            pasted_ids
        );
        assert!(target.undo());
        target.drawings_mut().registry.set_availability(
            DrawingToolType::TrendLine,
            ToolAvailability {
                visible: true,
                enabled: false,
            },
        );
        target.paste_drawings(&copied).unwrap();
        assert!(target.drawings().drawings.is_empty());
        assert!(!target.can_undo());
        // The copies stay with the destination when navigating between symbol drawing sets.
        target
            .drawings_mut()
            .registry
            .set_availability(DrawingToolType::TrendLine, ToolAvailability::default());
        target.paste_drawings(&copied).unwrap();
        target.set_symbol("SOURCE");
        assert!(target.drawings().drawings.is_empty());
        target.set_symbol("DESTINATION");
        assert_eq!(target.drawings().drawings.len(), 2);
        assert_eq!(source.drawings().drawings.len(), 2);
    }

    /// A drawing copied or saved in a study pane lands in the pane of the same study on a chart
    /// whose studies were added in another order, and in the price pane on a chart without it,
    /// although study ids are local to each chart.
    #[test]
    fn drawings_in_study_panes_follow_their_study_across_charts() {
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
        use crate::model::Bar;
        use crate::studies::{CustomIndicator, IndicatorValue};
        use crate::widget::PaneId;
        let study = |name: &'static str| {
            let value = if name == "Gamma" { 40.0 } else { 3.0 };
            Box::new(
                CustomIndicator::new(name, move |bars: &[Bar]| {
                    vec![IndicatorValue::Single(value); bars.len()]
                })
                .with_overlay(false),
            )
        };
        let mut source = test_frames::chart(&[10.0], &[20.0]);
        source.set_symbol("SOURCE");
        let gamma = source.add_indicator(study("Gamma"));
        source.add_indicator(study("Delta"));
        let mut line = Drawing::new(0, DrawingToolType::HorizontalLine);
        line.chart_points = vec![ChartPoint::new(50.0, 40.0)];
        line.pane = Some(gamma.0);
        let id = source.drawings_mut().add_drawing(line).unwrap();
        source.drawings_mut().select(id);
        let copied = source.copy_selected_drawings().unwrap().unwrap();
        let saved = source.save_drawings().unwrap().to_json().unwrap();
        let ctx = egui::Context::default();
        for (studies, paste) in [
            (&["Delta", "Gamma"][..], true),
            (&["Delta", "Gamma"], false),
            (&["Delta"], true),
            (&["Delta"], false),
        ] {
            let mut target = test_frames::chart(&[10.0], &[20.0]);
            target.set_symbol(if paste { "DESTINATION" } else { "SOURCE" });
            let ids: Vec<_> = studies
                .iter()
                .map(|name| target.add_indicator(study(name)))
                .collect();
            if paste {
                target.paste_drawings(&copied).unwrap();
            } else {
                target.load_drawings(&saved).unwrap();
            }
            test_frames::frame(&ctx, &mut target, 0.0, Vec::new());
            let drawing = &target.drawings().drawings[0];
            let case = format!("{studies:?}, paste {paste}");
            match studies.iter().position(|name| *name == "Gamma") {
                Some(at) => {
                    assert_eq!(
                        target.drawing_pane(drawing.id),
                        target.series_pane(ids[at]),
                        "{case}"
                    );
                    let x = target.get_rendered_price_rect().center().x;
                    let on_line = egui::pos2(x, drawing.points[0].y);
                    assert_eq!(
                        target.drawings().hit_test(on_line),
                        Some(drawing.id),
                        "{case}"
                    );
                }
                None => {
                    assert_eq!(drawing.pane, None, "{case}");
                    assert_eq!(
                        target.drawing_pane(drawing.id),
                        Some(PaneId::MAIN),
                        "{case}"
                    );
                }
            }
        }
    }
}
