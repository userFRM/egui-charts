//! A chart's studies saved and applied as a set, with their links and shared panes.

use super::{Chart, PaneId, SeriesId};
use crate::studies::{IndicatorFactory, InputValue, StudySource, StudyTemplate, StudyTemplateSet};

/// The position saved for a link to a study the chart no longer has: past the end of any set,
/// and still a `usize` where that is 32 bits wide.
const NO_STUDY: usize = u32::MAX as usize;

impl Chart {
    /// The chart's studies as a [`StudyTemplateSet`], in the order they were added. A link to
    /// another study's plot is saved as that study's position in the set, or past the end of
    /// the set when the chart no longer has that study, and the panes record which studies
    /// share a pane. Apply the set to this or another chart with
    /// [`apply_study_template_set`](Self::apply_study_template_set).
    pub fn study_template_set(&self) -> StudyTemplateSet {
        let ids: Vec<_> = self.indicators().map(|(id, _)| id).collect();
        let position = |id: SeriesId| ids.iter().position(|study| *study == id);
        let studies = self
            .indicators()
            .map(|(_, study)| {
                let mut template = StudyTemplate::from_study(study);
                for (_, value) in &mut template.inputs {
                    if let InputValue::Source(StudySource::Study { id, .. }) = value {
                        *id = position(SeriesId(*id)).unwrap_or(NO_STUDY);
                    }
                }
                template
            })
            .collect();
        let price_pane: Vec<_> = self
            .indicators()
            .enumerate()
            .filter(|(_, (_, study))| study.is_overlay())
            .map(|(at, _)| at)
            .collect();
        let mut panes: Vec<Vec<_>> = self
            .panes()
            .into_iter()
            .filter(|pane| *pane != PaneId::MAIN)
            .map(|pane| {
                self.pane_series(pane)
                    .into_iter()
                    .filter_map(position)
                    .collect::<Vec<_>>()
            })
            .filter(|pane| !pane.is_empty())
            .collect();
        // A study taken off the price pane since the last frame gets a pane of its own at the
        // bottom in the next one.
        let placed: Vec<_> = price_pane
            .iter()
            .chain(panes.iter().flatten())
            .copied()
            .collect();
        panes.extend(
            (0..ids.len())
                .filter(|at| !placed.contains(at))
                .map(|at| vec![at]),
        );
        StudyTemplateSet {
            studies,
            price_pane,
            panes,
        }
    }

    /// Replaces the chart's studies with those of `set`, built from the built-in studies, as
    /// [`apply_study_template_set_with`](Self::apply_study_template_set_with) does with its
    /// factory.
    pub fn apply_study_template_set(
        &mut self,
        set: &StudyTemplateSet,
    ) -> Result<Vec<SeriesId>, String> {
        self.apply_study_template_set_with(set, &IndicatorFactory::new())
    }

    /// Replaces the chart's studies with those of `set`, built through `factory`, which
    /// resolves the names of built-in and host-registered studies, and returns their ids in the
    /// set's order. Each link to a study of the set reads the new study, and a link past the end
    /// of the set reads nothing. The studies listed in
    /// [`price_pane`](StudyTemplateSet::price_pane) are drawn on the price pane, each list in
    /// [`panes`](StudyTemplateSet::panes) becomes a new pane at the bottom, in order, and the
    /// other studies are placed as [`add_indicator`](Self::add_indicator) places them. The
    /// studies are computed at once from the bars. The chart's volume setting follows the set:
    /// a volume study in it becomes the chart's volume, and without one the setting is off, as
    /// when the volume study is removed.
    ///
    /// Fails without changing the chart when the set holds more studies than the
    /// [study cap](Self::set_study_limit) allows, when its panes name a position twice or past
    /// its studies, or when a study cannot be built, with the error of
    /// [`StudyTemplate::create_with`]. Wrap the call in [`user_action`](Self::user_action) when
    /// it applies a user's choice, so that one undo restores the previous studies.
    pub fn apply_study_template_set_with(
        &mut self,
        set: &StudyTemplateSet,
        factory: &IndicatorFactory,
    ) -> Result<Vec<SeriesId>, String> {
        if self
            .study_limit
            .is_some_and(|limit| set.studies.len() > limit)
        {
            return Err(Self::STUDY_LIMIT_REACHED.into());
        }
        let mut placed = vec![false; set.studies.len()];
        for &at in set.price_pane.iter().chain(set.panes.iter().flatten()) {
            match placed.get_mut(at) {
                Some(seen) if !*seen => *seen = true,
                _ => return Err("Study template panes name a missing or repeated study".into()),
            }
        }
        let studies = set
            .studies
            .iter()
            .map(|template| template.create_with(factory))
            .collect::<Result<Vec<_>, _>>()?;
        let replaced: Vec<_> = self.indicators().map(|(id, _)| id).collect();
        for id in replaced {
            self.remove_series(id);
        }
        let ids: Vec<_> = studies
            .into_iter()
            .map(|study| self.add_indicator(study))
            .collect();
        for (id, template) in ids.iter().zip(&set.studies) {
            for (input, value) in &template.inputs {
                if let InputValue::Source(StudySource::Study { id: parent, plot }) = value {
                    let source = StudySource::Study {
                        id: ids.get(*parent).map_or(usize::MAX, |parent| parent.0),
                        plot: *plot,
                    };
                    self.indicator_mut(*id)
                        .expect("added study")
                        .set_input(input, InputValue::Source(source))
                        .expect("validated study source");
                }
            }
        }
        for &at in &set.price_pane {
            self.move_series_to_pane(ids[at], PaneId::MAIN);
        }
        for pane in &set.panes {
            if let Some((&first, rest)) = pane.split_first() {
                self.move_indicator_to_pane(ids[first], ids[first]);
                for &at in rest {
                    self.move_indicator_to_pane(ids[at], ids[first]);
                }
            }
        }
        // Removing the volume study turns the volume setting off; applying one turns it on.
        let volume = self
            .indicators()
            .find(|(_, study)| study.is_volume())
            .map(|(id, _)| id);
        self.volume_study = volume;
        self.volume_setting = self.volume_study.is_some();
        self.config.show_volume = self.volume_setting;
        self.recalculate_studies();
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_frames;
    use crate::studies::{CCI, EMA, MACD, RSI, SMA, StudySource, StudyTemplateSet, Volume};
    use crate::widget::{Chart, SeriesId};

    #[test]
    fn study_template_sets_keep_links_shared_panes_and_volume_on_another_chart() {
        let closes: Vec<f64> = (0..100).map(|i| 100.0 + f64::from(i)).collect();
        let opens: Vec<f64> = closes.iter().map(|close| close - 1.0).collect();
        let mut source = test_frames::chart(&opens, &closes);
        let linked = source.add_indicator(Box::new(SMA::new(3)));
        let rsi = source.add_indicator(Box::new(RSI::new(14)));
        source.add_indicator(Box::new(MACD::new(12, 26, 9)));
        source.add_indicator(Box::new(SMA::new(5)));
        source.add_indicator(Box::new(Volume::default()));
        source
            .set_indicator_source(linked, StudySource::Study { id: rsi.0, plot: 0 })
            .unwrap();
        assert!(source.move_indicator_to_pane(linked, rsi));
        let saved = serde_json::to_string(&source.study_template_set()).unwrap();

        let mut chart = test_frames::chart(&opens, &closes);
        chart.add_indicator(Box::new(EMA::new(5)));
        chart.add_indicator(Box::new(CCI::new(20)));
        chart.config_mut().show_volume = false;
        let set: StudyTemplateSet = serde_json::from_str(&saved).unwrap();
        let ids = chart.apply_study_template_set(&set).unwrap();

        let studies: Vec<_> = chart
            .indicators()
            .map(|(id, study)| (id, study.name().to_owned()))
            .collect();
        let names = ["SMA", "RSI", "MACD", "SMA", "Volume"].map(String::from);
        assert_eq!(studies, ids.iter().copied().zip(names).collect::<Vec<_>>());
        let panes: Vec<_> = chart
            .panes()
            .into_iter()
            .map(|pane| chart.pane_series(pane))
            .collect();
        assert_eq!(
            panes,
            [
                vec![SeriesId::MAIN, ids[3], ids[4]],
                vec![ids[1], ids[0]],
                vec![ids[2]],
            ]
        );
        let linked = chart.indicators().next().unwrap().1;
        assert_eq!(
            linked.source(),
            Some(StudySource::Study {
                id: ids[1].0,
                plot: 0
            })
        );
        // Closes that only rise have an RSI of 100, so its 3-bar mean is 100; the mean of the
        // last three closes is 198.
        assert_eq!(linked.values().value(0, 99), Some(100.0));
        assert!(chart.config().show_volume);
    }

    #[test]
    fn study_template_sets_that_cannot_be_applied_leave_the_chart_unchanged() {
        let mut chart = test_frames::chart(&[1.0], &[2.0]);
        chart.add_indicator(Box::new(EMA::new(5)));
        chart.add_indicator(Box::new(CCI::new(20)));
        let saved = chart.study_template_set();
        let layout = |chart: &Chart| {
            (
                chart.indicators().map(|(id, _)| id).collect::<Vec<_>>(),
                chart
                    .panes()
                    .into_iter()
                    .map(|pane| chart.pane_series(pane))
                    .collect::<Vec<_>>(),
            )
        };
        let before = layout(&chart);
        let mut unknown = saved.clone();
        unknown.studies[1].name = "Missing".into();
        let mut past_end = saved.clone();
        past_end.panes = vec![vec![1, 2]];
        let mut twice = saved.clone();
        twice.price_pane = vec![0, 1];
        let refused = "Study template panes name a missing or repeated study";
        for (set, limit, error) in [
            (unknown, None, "Unknown study: Missing"),
            (past_end, None, refused),
            (twice, None, refused),
            (saved, Some(1), "Study limit reached"),
        ] {
            chart.set_study_limit(limit);
            assert_eq!(chart.apply_study_template_set(&set), Err(error.to_owned()));
            assert_eq!(layout(&chart), before);
        }
    }

    #[test]
    fn stored_template_lists_apply_with_their_links_and_default_places() {
        let mut source = test_frames::chart(&[1.0], &[2.0]);
        let rsi = source.add_indicator(Box::new(RSI::new(14)));
        let mean = source.add_indicator(Box::new(SMA::new(3)));
        source
            .set_indicator_source(mean, StudySource::Study { id: rsi.0, plot: 0 })
            .unwrap();
        let list = serde_json::to_value(source.study_template_set().studies).unwrap();

        let mut chart = test_frames::chart(&[1.0], &[2.0]);
        let set: StudyTemplateSet = serde_json::from_value(list).unwrap();
        let ids = chart.apply_study_template_set(&set).unwrap();
        assert_eq!(
            chart.indicators().nth(1).unwrap().1.source(),
            Some(StudySource::Study {
                id: ids[0].0,
                plot: 0
            })
        );
        let panes: Vec<_> = chart
            .panes()
            .into_iter()
            .map(|pane| chart.pane_series(pane))
            .collect();
        assert_eq!(panes, [vec![SeriesId::MAIN, ids[1]], vec![ids[0]]]);
    }
}
