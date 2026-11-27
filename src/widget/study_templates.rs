//! A chart's studies saved and applied as a set, with their links and shared panes.

use super::{Chart, PaneId, SeriesId};
use crate::model::SeriesScale;
use crate::studies::{
    Indicator, IndicatorFactory, InputValue, StudyScale, StudySource, StudyTemplate,
    StudyTemplateSet,
};

/// The position saved for a link to a study the chart no longer has: past the end of any set,
/// and still a `usize` where that is 32 bits wide.
pub(super) const NO_STUDY: usize = u32::MAX as usize;

/// Which of a chart's studies stay when a [`StudyTemplateSet`] is placed on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Keep {
    /// None: the set replaces them all.
    None,
    /// Those whose user edits are off, which no user command removes. The set's own such studies
    /// are left out, so that placing it again adds none of them twice.
    ReadOnly,
    /// All of them: the set is added beside them.
    All,
}

impl Keep {
    pub(super) fn keeps(self, study: &dyn Indicator) -> bool {
        match self {
            Self::None => false,
            Self::ReadOnly => !study.user_edit_enabled(),
            Self::All => true,
        }
    }

    /// Whether `study`, built from the set, is placed.
    pub(super) fn adds(self, study: &dyn Indicator) -> bool {
        self != Self::ReadOnly || study.user_edit_enabled()
    }
}

impl Chart {
    /// The chart's studies as a [`StudyTemplateSet`], in the order they were added. A link to
    /// another study's plot is saved as that study's position in the set, or past the end of
    /// the set when the chart no longer has that study, and the panes record which studies
    /// share a pane. The set keeps no symbol or interval; set
    /// [`symbol`](StudyTemplateSet::symbol) or [`interval`](StudyTemplateSet::interval) to
    /// have applying it show them. Apply the set to this or another chart with
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
        // Without the original pane a study over the price series is no overlay.
        let main_pane = self.main_series_pane();
        let price_pane: Vec<_> = self
            .indicators()
            .enumerate()
            .filter(|(_, (id, study))| {
                study.is_overlay() || self.series_pane(*id) == Some(main_pane)
            })
            .map(|(at, _)| at)
            .collect();
        let mut panes: Vec<Vec<_>> = self
            .panes()
            .into_iter()
            .filter(|pane| *pane != PaneId::MAIN && *pane != main_pane)
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
            ..StudyTemplateSet::default()
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
    /// other studies are placed as [`add_indicator`](Self::add_indicator) places them. A set
    /// with a [`symbol`](StudyTemplateSet::symbol) or an
    /// [`interval`](StudyTemplateSet::interval) first shows them as
    /// [`set_symbol`](Self::set_symbol) and [`set_timeframe`](Self::set_timeframe) do; a set
    /// without them keeps the chart's. The studies are computed at once from the bars. The
    /// chart's volume setting follows the set: a volume study in it becomes the chart's volume,
    /// and without one the setting is off, as when the volume study is removed.
    ///
    /// Fails without changing the chart when the set holds more studies than the
    /// [study cap](Self::set_study_limit) allows or adds studies reading another study's output
    /// past the [chained study cap](Self::set_chained_study_limit), when its panes name a position
    /// twice or past its studies, or when a study cannot be built, with the error of
    /// [`StudyTemplate::create_with`]. Wrap the call in [`user_action`](Self::user_action) when
    /// it applies a user's choice, so that one undo restores the previous studies, symbol and
    /// interval; there the added studies go under drawings as
    /// [`add_indicator`](Self::add_indicator) describes.
    pub fn apply_study_template_set_with(
        &mut self,
        set: &StudyTemplateSet,
        factory: &IndicatorFactory,
    ) -> Result<Vec<SeriesId>, String> {
        self.place_study_template_set(set, factory, Keep::None)
            .map(|ids| ids.into_iter().flatten().collect())
    }

    /// Applies `set` as a user's choice, built from the built-in studies, as one
    /// [user action](Self::user_action) that a single undo reverses. The studies whose
    /// [user edits](Self::set_indicator_user_edit_enabled) are off stay, since no user command
    /// removes them, and count against the [study caps](Self::set_study_limit), and the set's own
    /// such studies are left out, so that applying it again adds none of them twice. The set's
    /// other studies replace the chart's others and are placed beside the kept ones as
    /// [`apply_study_template_set_with`](Self::apply_study_template_set_with) places them, each
    /// additional scale of theirs numbered apart from those the kept studies use. Returns the ids
    /// of the studies it added, in the set's order, and fails as
    /// [`apply_study_template_set`](Self::apply_study_template_set) does.
    pub fn apply_chosen_study_template_set(
        &mut self,
        set: &StudyTemplateSet,
    ) -> Result<Vec<SeriesId>, String> {
        self.apply_chosen_study_template_set_with(set, &IndicatorFactory::new())
    }

    /// Applies `set` as a user's choice through `factory`, which resolves built-in and
    /// host-registered studies, with the same permissions, study caps and undo behaviour as
    /// [`apply_chosen_study_template_set`](Self::apply_chosen_study_template_set).
    pub fn apply_chosen_study_template_set_with(
        &mut self,
        set: &StudyTemplateSet,
        factory: &IndicatorFactory,
    ) -> Result<Vec<SeriesId>, String> {
        self.user_action(|chart| chart.place_study_template_set(set, factory, Keep::ReadOnly))
            .map(|ids| ids.into_iter().flatten().collect())
    }

    /// Adds the studies of `set` that `keep` adds in place of the chart's own, but for those
    /// `keep` keeps, which count against the study caps with them. Returns the id of each study of
    /// the set by its position, `None` for those left out, and fails as
    /// [`apply_study_template_set_with`](Self::apply_study_template_set_with) does.
    pub(super) fn place_study_template_set(
        &mut self,
        set: &StudyTemplateSet,
        factory: &IndicatorFactory,
        keep: Keep,
    ) -> Result<Vec<Option<SeriesId>>, String> {
        let mut placed = vec![false; set.studies.len()];
        for &at in set.price_pane.iter().chain(set.panes.iter().flatten()) {
            match placed.get_mut(at) {
                Some(seen) if !*seen => *seen = true,
                _ => return Err("Study template panes name a missing or repeated study".into()),
            }
        }
        let mut studies = set
            .studies
            .iter()
            .enumerate()
            .map(|(at, template)| -> Result<_, String> {
                let mut study = template
                    .create_with(factory)
                    .map_err(|error| error.to_string())?;
                // Expose saved overlay pins before additional scales are renumbered.
                if set.price_pane.contains(&at) {
                    study.set_overlay(true);
                    if let Some(scale) = template.scale {
                        study.set_scale(scale);
                    }
                }
                Ok(keep.adds(study.as_ref()).then_some(study))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let kept = self
            .indicators()
            .filter(|(_, study)| keep.keeps(*study))
            .count();
        if self
            .study_limit
            .is_some_and(|limit| kept + studies.iter().flatten().count() > limit)
        {
            return Err("Study limit reached".into());
        }
        let kept_chained = self
            .indicators()
            .filter(|(_, study)| keep.keeps(*study) && super::studies::reads_study(*study))
            .count();
        let added_chained = studies
            .iter()
            .flatten()
            .filter(|study| super::studies::reads_study(study.as_ref()))
            .count();
        if self.passes_chained_study_limit(kept_chained, added_chained) {
            return Err("Chained study limit reached".into());
        }
        if let Some(symbol) = &set.symbol {
            self.set_symbol(symbol.clone());
        }
        if let Some(interval) = set.interval {
            self.set_timeframe(interval);
        }
        let outer = self.begin_study_batch();
        if keep != Keep::All {
            let replaced: Vec<_> = self
                .indicators()
                .filter(|(_, study)| !keep.keeps(*study))
                .map(|(id, _)| id)
                .collect();
            for id in replaced {
                self.remove_series(id);
            }
        }
        if keep != Keep::None {
            self.renumber_scales(&mut studies);
        }
        #[cfg(feature = "drawings")]
        let stack = self.pane_stack(SeriesId(self.next_series));
        let ids: Vec<_> = studies
            .into_iter()
            .map(|study| study.map(|study| self.insert_indicator(study)))
            .collect();
        for (id, template) in ids.iter().zip(&set.studies) {
            let Some(id) = *id else {
                continue;
            };
            for (input, value) in &template.inputs {
                if let InputValue::Source(StudySource::Study { id: parent, plot }) = value {
                    let parent = ids.get(*parent).copied().flatten();
                    let plot = self
                        .indicators()
                        .find(|(study, _)| Some(*study) == parent)
                        .map_or(*plot, |(_, read)| {
                            StudyTemplate::upgraded_output(read, *plot)
                        });
                    let source = StudySource::Study {
                        id: parent.map_or(usize::MAX, |parent| parent.0),
                        plot,
                    };
                    let study = self.indicator_mut(id).expect("added study");
                    // An input restored on its default in place of the saved output keeps it.
                    let own = study.inputs();
                    if !own
                        .iter()
                        .any(|own| own.id == *input && own.value == *value)
                    {
                        continue;
                    }
                    study
                        .set_input(input, InputValue::Source(source))
                        .expect("validated study source");
                }
            }
        }
        for id in set.price_pane.iter().filter_map(|&at| ids[at]) {
            self.move_series_keeping_layers(id, self.main_series_pane());
        }
        for pane in &set.panes {
            let pane: Vec<_> = pane.iter().filter_map(|&at| ids[at]).collect();
            if let Some((&first, rest)) = pane.split_first() {
                self.move_to_new_pane_keeping_layers(first);
                if let Some(pane) = self.series_pane(first).filter(|pane| *pane != PaneId::MAIN) {
                    for &id in rest {
                        self.move_series_keeping_layers(id, pane);
                    }
                }
            }
        }
        // Studies listed in neither field take their default place after their links resolve.
        let unlisted: Vec<_> = super::studies::order(self.studies.indicators(), &self.study_ids)
            .into_iter()
            .map(|at| self.study_ids[at])
            .filter(|id| {
                ids.iter()
                    .zip(&placed)
                    .any(|(added, placed)| *added == Some(*id) && !placed)
            })
            .collect();
        for id in unlisted {
            self.place_on_source(id);
        }
        #[cfg(feature = "drawings")]
        if let Some(stack) = stack {
            self.keep_stack(stack);
        }
        // Removing the volume study turns the volume setting off; applying one turns it on.
        let volume = self
            .indicators()
            .find(|(_, study)| study.is_volume())
            .map(|(id, _)| id);
        // A study added beside the chart's own leaves the setting alone unless it is the volume.
        if keep != Keep::All || (volume.is_some() && self.volume_study.is_none()) {
            self.volume_study = volume;
            self.volume_setting = self.volume_study.is_some();
            self.config.show_volume = self.volume_setting;
        }
        self.recalculate_studies();
        self.end_study_batch(outer);
        Ok(ids)
    }

    /// Gives each additional scale of `studies` a number this chart does not use yet, keeping
    /// its own where it is free, as the studies' numbers come from another chart or a template.
    /// Studies that share a scale still share one, and only with each other.
    fn renumber_scales(&self, studies: &mut [Option<Box<dyn Indicator>>]) {
        let mut used: Vec<_> = self
            .indicators()
            .filter_map(|(_, study)| match study.scale() {
                StudyScale::Shared(scale) => Some(scale),
                _ => None,
            })
            .chain(self.series().map(|(_, options)| options.scale))
            .collect();
        let mut renumbered: Vec<(SeriesScale, SeriesScale)> = Vec::new();
        for study in studies.iter_mut().flatten() {
            let StudyScale::Shared(scale @ (SeriesScale::Separate(_) | SeriesScale::Overlay(_))) =
                study.scale()
            else {
                continue;
            };
            let to = match renumbered.iter().find(|(from, _)| *from == scale) {
                Some(&(_, to)) => to,
                None => {
                    let numbered = |key| match scale {
                        SeriesScale::Overlay(_) => SeriesScale::Overlay(key),
                        _ => SeriesScale::Separate(key),
                    };
                    let to = std::iter::once(scale)
                        .chain((0..).map(numbered))
                        .find(|scale| !used.contains(scale))
                        .expect("a number is free");
                    used.push(to);
                    renumbered.push((scale, to));
                    to
                }
            };
            study.set_scale(StudyScale::Shared(to));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_frames;
    use crate::studies::{CCI, EMA, MACD, RSI, SMA, StudySource, StudyTemplateSet, Volume};
    use crate::widget::{Chart, SeriesId};

    /// Studies saved on the price pane keep their placement, pinned scales and fitted table
    /// sizes on restore. A separate Seasonality pane's implicit scale does not become a pin.
    #[test]
    fn studies_saved_on_the_price_pane_restore_there() {
        use crate::model::SeriesScale;
        use crate::studies::{StudyScale, TablePosition};
        use crate::widget::PaneId;

        let saved: StudyTemplateSet = serde_json::from_str(r#"{"studies":[{"name":"Performance","inputs":[["symbols",{"Text":"AAPL, NASDAQ:AMZN, EURUSD, ES1!, BTCUSD"}],["include_chart_symbol",{"Toggle":true}],["periods",{"Text":"1W, 1M, 3M, 6M, 12M, YTD, 5Y"}],["positive_color",{"Color":[8,153,129,255]}],["negative_color",{"Color":[242,55,69,255]}],["color_cutoff",{"Number":10.0}],["table_position",{"Choice":4}],["table_width",{"Number":100.0}],["table_height",{"Number":95.0}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.Performance.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[0,1,2,3,4,5],"price_line_plots":[],"plot_variants":[[],[],[],[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"},{"name":"Seasonality","inputs":[["start_year",{"Integer":2015}],["ignored_months",{"Text":""}],["positive_color",{"Color":[8,153,129,255]}],["negative_color",{"Color":[242,54,69,255]}],["color_cutoff",{"Number":10.0}],["show_averages",{"Toggle":true}],["show_std_dev",{"Toggle":true}],["show_positive",{"Toggle":true}],["table_position",{"Choice":7}],["table_width",{"Number":0.0}],["table_height",{"Number":0.0}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.Seasonality.html","plots":[{"color":[8,153,129,255],"width":1.0,"pattern":"Dashed","plot_type":"JoinedStepLine","visible":true},{"color":[120,123,134,255],"width":1.0,"pattern":"Dashed","plot_type":"JoinedStepLine","visible":true},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedStepLine","visible":false},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedStepLine","visible":false},{"color":[120,123,134,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedStepLine","visible":false}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[1,10,8,16],"untagged_plots":[0,1,2,3,4],"price_line_plots":[],"plot_variants":[[],[],[],[],[]],"background":null,"background_visible":true,"fills":[{"color":[2,31,26,51],"inverted_color":[48,11,14,51],"visible":true,"independent":true}],"gradients":[]},"visible":true,"scale":{"Shared":"Left"}}],"price_pane":[0,1],"panes":[]}"#).unwrap();
        let mut chart = test_frames::chart(&[1.0, 4.0, 7.0], &[2.0, 5.0, 8.0]);
        let ids = chart.apply_study_template_set(&saved).unwrap();
        assert_eq!(chart.panes().len(), 1);
        for (id, position, width, height) in [
            (ids[0], TablePosition::MiddleCenter, Some(1.0), Some(0.95)),
            (ids[1], TablePosition::BottomCenter, None, None),
        ] {
            assert_eq!(chart.series_pane(id), Some(PaneId::MAIN));
            let study = chart
                .indicators()
                .find(|(found, _)| *found == id)
                .unwrap()
                .1;
            assert!(study.is_overlay());
            let table = study.table().unwrap();
            assert_eq!(
                (table.position, table.width, table.height),
                (position, width, height)
            );
        }
        assert_eq!(
            chart
                .indicators()
                .find(|(found, _)| *found == ids[1])
                .unwrap()
                .1
                .scale(),
            StudyScale::Shared(SeriesScale::Left)
        );
        #[cfg(feature = "ui")]
        {
            let mut restored = test_frames::chart(&[1.0, 4.0, 7.0], &[2.0, 5.0, 8.0]);
            restored.restore(&chart.snapshot().unwrap()).unwrap();
            assert_eq!(restored.panes().len(), 1);
            for (id, study) in restored.indicators() {
                assert_eq!(restored.series_pane(id), Some(PaneId::MAIN));
                assert!(study.is_overlay());
                if study.name() == "Seasonality" {
                    assert_eq!(study.scale(), StudyScale::Shared(SeriesScale::Left));
                }
            }
        }
        for scale in [
            StudyScale::None,
            StudyScale::Shared(SeriesScale::Separate(0)),
            StudyScale::Shared(SeriesScale::Left),
        ] {
            assert!(chart.set_indicator_scale(ids[1], scale));
            assert_eq!(
                chart
                    .indicators()
                    .find(|(found, _)| *found == ids[1])
                    .unwrap()
                    .1
                    .scale(),
                scale
            );
        }

        for (overlay, scale) in [
            (false, StudyScale::None),
            (true, StudyScale::Main),
            (true, StudyScale::None),
            (true, StudyScale::Shared(SeriesScale::Left)),
            (true, StudyScale::Shared(SeriesScale::Separate(0))),
            (true, StudyScale::Shared(SeriesScale::Overlay(0))),
        ] {
            let mut source = test_frames::chart(&[1.0, 4.0, 7.0], &[2.0, 5.0, 8.0]);
            let id = source.add_indicator(Box::new(crate::studies::Seasonality::new()));
            if overlay {
                assert!(source.move_series_to_pane(id, PaneId::MAIN));
                assert!(source.set_indicator_scale(id, scale));
            }
            let check = |mut restored: Chart, id| {
                let study = restored
                    .indicators()
                    .find(|(found, _)| *found == id)
                    .unwrap()
                    .1;
                assert_eq!(study.is_overlay(), overlay);
                assert_eq!(study.scale(), scale);
                assert!(restored.move_series_to_pane(id, PaneId::MAIN));
                assert_eq!(
                    restored
                        .indicators()
                        .find(|(found, _)| *found == id)
                        .unwrap()
                        .1
                        .scale(),
                    if overlay { scale } else { StudyScale::Main }
                );
            };
            let set = source.study_template_set();
            let mut restored = test_frames::chart(&[1.0, 4.0, 7.0], &[2.0, 5.0, 8.0]);
            let ids = restored.apply_study_template_set(&set).unwrap();
            check(restored, ids[0]);
            #[cfg(feature = "ui")]
            {
                let mut restored = test_frames::chart(&[1.0, 4.0, 7.0], &[2.0, 5.0, 8.0]);
                restored.restore(&source.snapshot().unwrap()).unwrap();
                let restored_id = restored.restored_series_id(id.0).unwrap();
                check(restored, restored_id);
            }
            #[cfg(feature = "drawings")]
            {
                let mut restored = test_frames::chart(&[1.0, 4.0, 7.0], &[2.0, 5.0, 8.0]);
                let pasted = restored
                    .paste_indicator(
                        &source.copy_indicator(id).unwrap(),
                        &crate::studies::IndicatorFactory::new(),
                    )
                    .unwrap();
                check(restored, pasted);
            }
            if scale == StudyScale::Shared(SeriesScale::Separate(0)) {
                let ids = source
                    .place_study_template_set(
                        &set,
                        &crate::studies::IndicatorFactory::new(),
                        super::Keep::All,
                    )
                    .unwrap();
                assert_eq!(
                    source
                        .indicators()
                        .find(|(id, _)| Some(*id) == ids[0])
                        .unwrap()
                        .1
                        .scale(),
                    StudyScale::Shared(SeriesScale::Separate(1))
                );
            }
        }
    }

    /// New studies inherit host defaults while explicit edits and saved studies keep their
    /// settings through templates, copies, layout restore and undo.
    #[test]
    fn host_defaults_yield_to_explicit_and_saved_study_settings() {
        use crate::model::PriceSource;
        use crate::studies::{IndicatorFactory, InputValue, StudySource, StudyTemplate};

        let mut chart = test_frames::chart(&[1.0, 4.0, 7.0], &[2.0, 5.0, 8.0]);
        chart.add_indicator(Box::new(SMA::new(1)));
        let saved = chart.study_template_set();
        let mut factory = IndicatorFactory::new();
        let mut defaults = saved.studies[0].clone();
        defaults.inputs = vec![
            ("period".into(), InputValue::Length(3)),
            (
                "source".into(),
                InputValue::Source(StudySource::Price(PriceSource::Open)),
            ),
            ("plot_offset".into(), InputValue::Integer(1)),
        ];
        defaults.settings.plots[0].width = 4.0;
        defaults.settings.show_legend_values = false;
        factory.set_defaults(&[defaults]).unwrap();

        let added = chart.user_action(|chart| chart.add_indicator(factory.create("SMA").unwrap()));
        let study = chart.indicators().last().unwrap().1;
        // The three opens average to (1 + 4 + 7) / 3 = 4.
        assert_eq!(study.values().value(0, 2), Some(4.0));
        assert_eq!(study.plot_offset(0), 1);
        assert_eq!(study.plot_style(0).width, 4.0);
        assert!(!study.legend_visible(0));
        assert!(chart.undo());
        assert_eq!(chart.indicators().count(), 1);
        assert!(chart.redo());
        assert_eq!(
            chart.indicators().last().unwrap().1.plot_style(0).width,
            4.0
        );

        let study = chart.indicator_mut(added).unwrap();
        let mut settings = study.settings();
        settings.plots[0].width = 2.0;
        study
            .apply_settings(
                &[
                    ("period".into(), InputValue::Length(2)),
                    (
                        "source".into(),
                        InputValue::Source(StudySource::Price(PriceSource::Close)),
                    ),
                    ("plot_offset".into(), InputValue::Integer(0)),
                ],
                settings,
            )
            .unwrap();
        chart.recalculate_studies();
        let study = chart.indicators().last().unwrap().1;
        // The final two closes average to (5 + 8) / 2 = 6.5.
        assert_eq!(study.values().value(0, 2), Some(6.5));
        assert_eq!(study.plot_offset(0), 0);
        assert_eq!(study.plot_style(0).width, 2.0);

        let ids = chart
            .user_action(|chart| chart.apply_study_template_set_with(&saved, &factory))
            .unwrap();
        assert_eq!(ids.len(), 1);
        let study = chart.indicators().next().unwrap().1;
        assert_eq!(study.settings(), saved.studies[0].settings);
        assert_eq!(study.values().value(0, 2), Some(8.0));
        assert_eq!(study.plot_offset(0), 0);
        assert!(chart.undo());
        assert_eq!(chart.indicators().count(), 2);
        assert!(chart.redo());
        assert_eq!(chart.indicators().count(), 1);

        #[cfg(feature = "drawings")]
        {
            let copied = chart.copy_indicator(ids[0]).unwrap();
            let pasted = chart.paste_indicator(&copied, &factory).unwrap();
            let study = chart.indicators().find(|(id, _)| *id == pasted).unwrap().1;
            assert_eq!(study.plot_offset(0), 0);
            assert_eq!(study.values().value(0, 2), Some(8.0));
            assert_eq!(study.settings(), saved.studies[0].settings);
        }
        #[cfg(feature = "ui")]
        {
            let snapshot = serde_json::to_string(&chart.snapshot().unwrap()).unwrap();
            chart
                .restore_with(&serde_json::from_str(&snapshot).unwrap(), &factory)
                .unwrap();
            for (_, study) in chart.indicators() {
                assert_eq!(study.settings(), saved.studies[0].settings);
                assert_eq!(study.plot_offset(0), 0);
                assert_eq!(study.values().value(0, 2), Some(8.0));
            }
        }
        // Template reconstruction alone also ignores defaults, even for omitted inputs.
        let mut legacy: StudyTemplate = saved.studies[0].clone();
        legacy.inputs.clear();
        assert_eq!(legacy.create_with(&factory).unwrap().plot_offset(0), 0);
    }

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
        // Every failing set also keeps a symbol and an interval, which the chart must not show.
        let mut saved = chart.study_template_set();
        saved.symbol = Some("MSFT".into());
        saved.interval = Some(crate::model::Timeframe::Hour1);
        let layout = |chart: &Chart| {
            (
                chart.symbol().to_owned(),
                chart.timeframe(),
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
            (unknown, None, "unknown study \"Missing\""),
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
        assert_eq!(panes, [vec![SeriesId::MAIN], vec![ids[0], ids[1]]]);
    }

    /// A template taken after the empty original pane was removed keeps a moving average
    /// over the price series it was added to, on this chart and on a chart with its price pane.
    #[test]
    fn template_keeps_studies_over_the_relocated_price_series() {
        use crate::widget::PaneId;

        let mut chart = test_frames::chart(&[100.0], &[110.0]);
        let price = chart.add_pane();
        assert!(chart.move_series_to_pane(SeriesId::MAIN, price));
        assert!(chart.remove_pane(PaneId::MAIN));
        let sma = chart.add_indicator(Box::new(SMA::new(3)));
        assert_eq!(
            chart.series_pane(sma),
            Some(price),
            "added over the price series"
        );
        let set = chart.study_template_set();
        let ids = chart.apply_study_template_set(&set).unwrap();
        let mut fresh = test_frames::chart(&[100.0], &[110.0]);
        let fresh_ids = fresh.apply_study_template_set(&set).unwrap();
        assert_eq!(
            [chart.series_pane(ids[0]), fresh.series_pane(fresh_ids[0])],
            [Some(price), Some(PaneId::MAIN)]
        );
    }

    /// A set saved before sets kept a symbol and an interval applies over the chart's own, and
    /// a set that keeps them shows them, with one undo restoring the symbol, interval and studies.
    #[test]
    fn study_template_sets_show_only_the_symbol_and_interval_they_keep() {
        use crate::model::Timeframe;

        let mut chart = test_frames::chart(&[1.0, 4.0, 7.0], &[2.0, 5.0, 8.0]);
        chart.set_symbol("AAPL");
        chart.set_timeframe(Timeframe::Day1);
        chart.add_indicator(Box::new(EMA::new(5)));
        let mut kept = serde_json::to_value(chart.study_template_set()).unwrap();
        assert!(kept.get("symbol").is_none() && kept.get("interval").is_none());
        kept["symbol"] = "MSFT".into();
        kept["interval"] = "Hour1".into();
        let kept: StudyTemplateSet = serde_json::from_value(kept).unwrap();
        let saved_before: StudyTemplateSet = serde_json::from_str(
            r#"{"studies":[{"name":"SMA","inputs":[["period",{"Length":3}]],"settings":{"plots":[{"color":[255,235,59,255],"width":2.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":null,"hidden_intervals":[]},"visible":true}],"price_pane":[0],"panes":[]}"#,
        )
        .unwrap();
        let shown = |chart: &Chart| {
            (
                chart.symbol().to_owned(),
                chart.timeframe(),
                chart
                    .indicators()
                    .map(|(_, study)| study.name().to_owned())
                    .collect::<Vec<_>>(),
            )
        };

        chart
            .user_action(|chart| chart.apply_study_template_set(&saved_before))
            .unwrap();
        let before = (
            "AAPL".to_owned(),
            Some(Timeframe::Day1),
            vec!["SMA".to_owned()],
        );
        assert_eq!(shown(&chart), before);
        let study = chart.indicators().next().unwrap().1;
        assert_eq!(
            study.plot_style(0).color,
            egui::Color32::from_rgb(255, 235, 59)
        );
        // The three closes average to (2 + 5 + 8) / 3 = 5.
        assert_eq!(study.values().value(0, 2), Some(5.0));

        chart
            .user_action(|chart| chart.apply_study_template_set(&kept))
            .unwrap();
        assert_eq!(
            shown(&chart),
            (
                "MSFT".to_owned(),
                Some(Timeframe::Hour1),
                vec!["EMA".to_owned()]
            )
        );
        assert!(chart.undo());
        assert_eq!(shown(&chart), before);
    }

    /// Every built-in set applies through the default factory in place of the chart's studies,
    /// with the studies, offsets and panes it declares, and its volume study can still show its
    /// average.
    #[test]
    fn builtin_study_template_sets_apply_with_their_studies_and_panes() {
        use crate::studies::InputValue;

        // The set's name, its studies with the offset of their first plot, and the chart's
        // panes as positions in the set, the price pane first, where the main series leads.
        type Row = (
            &'static str,
            &'static [(&'static str, isize)],
            &'static [&'static [usize]],
        );
        let expected: [Row; 6] = [
            (
                "Williams Alligator",
                &[("Volume", 0), ("Williams Alligator", 8)],
                &[&[0, 1]],
            ),
            ("Displaced EMA", &[("Volume", 0), ("EMA", 5)], &[&[0, 1]]),
            (
                "Moving average ribbon",
                &[("Volume", 0), ("MA Ribbon", 0)],
                &[&[0, 1]],
            ),
            (
                "Oscillators",
                &[("Volume", 0), ("RSI", 0), ("StochRSI", 0), ("CCI", 0)],
                &[&[0], &[1], &[2], &[3]],
            ),
            (
                "Swing trading",
                &[("Volume", 0), ("BB", 0), ("MACD", 0), ("Stochastic", 0)],
                &[&[0, 1], &[2], &[3]],
            ),
            (
                "Volume analysis",
                &[("Volume", 0), ("OBV", 0), ("MFI", 0)],
                &[&[0], &[1], &[2]],
            ),
        ];
        let sets = StudyTemplateSet::builtin();
        assert_eq!(
            sets.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            expected.map(|(name, ..)| name)
        );
        for ((name, set), (_, studies, panes)) in sets.iter().zip(expected) {
            let mut chart = test_frames::chart(&[1.0], &[2.0]);
            chart.add_indicator(Box::new(CCI::new(20)));
            let ids = chart
                .apply_study_template_set(set)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let shown: Vec<_> = chart
                .indicators()
                .map(|(_, study)| (study.name().to_owned(), study.plot_offset(0)))
                .collect();
            let declared: Vec<_> = studies
                .iter()
                .map(|&(study, offset)| (study.to_owned(), offset))
                .collect();
            assert_eq!(shown, declared, "{name}");
            let layout: Vec<_> = chart
                .panes()
                .into_iter()
                .map(|pane| chart.pane_series(pane))
                .collect();
            let declared: Vec<Vec<_>> = panes
                .iter()
                .enumerate()
                .map(|(at, pane)| {
                    (at == 0)
                        .then_some(SeriesId::MAIN)
                        .into_iter()
                        .chain(pane.iter().map(|&position| ids[position]))
                        .collect()
                })
                .collect();
            assert_eq!(layout, declared, "{name}");
            // Turned on, the volume average is drawn, as on a volume study added to a chart.
            chart
                .indicator_mut(ids[0])
                .unwrap()
                .set_input("show_average", InputValue::Toggle(true))
                .unwrap();
            assert!(
                chart.indicators().next().unwrap().1.plot_style(2).visible,
                "{name}"
            );
        }
    }
}
