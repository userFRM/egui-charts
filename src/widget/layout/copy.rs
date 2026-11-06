//! Copies of a chart's studies and settings to the other charts of a layout, undone and redone
//! together.

use super::ChartLayout;
use crate::config::ChartConfig;
use crate::model::ChartType;
use crate::scales::instrument_price_precision;
use crate::studies::{Indicator, IndicatorFactory, InputValue, StudySource, StudyTemplateSet};
use crate::widget::settings::{ChartSettingsState, PrecisionMode};
use crate::widget::study_templates::NO_STUDY;
use crate::widget::{Chart, SeriesId};

/// What [`ChartLayout::apply_copy`] takes from one chart to the others of the layout.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LayoutCopy {
    /// Adds this study to each other chart, next to the studies it has, with its inputs, styles,
    /// interval visibility and scale. A study on the source's price pane joins each chart's
    /// price pane; any other study gets a new pane at the bottom of each chart.
    Study(SeriesId),
    /// Replaces the studies of each other chart with those of the source, with their links and
    /// shared panes, as [`Chart::apply_study_template_set_with`] does.
    Studies,
    /// Applies these settings to every chart, the source included. Each other chart keeps its
    /// symbol, interval and chart type, the sizes and other parameters of that type, and the
    /// precision of its instrument unless the source was given another one.
    Settings(Box<ChartSettingsState>),
}

/// A chart of the layout that a [`LayoutCopy`] cannot fully serve, for the host to show before it
/// applies the copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutCopyIssue {
    /// The chart's slot in the layout.
    pub slot: usize,
    /// What the chart lacks.
    pub kind: LayoutCopyIssueKind,
}

/// Why a chart cannot fully take a [`LayoutCopy`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LayoutCopyIssueKind {
    /// The chart holds no bars yet. It takes the copy, and its studies draw once bars arrive.
    NoHistory,
    /// The chart's bars are too few for this study to draw, though it draws on the source. The
    /// chart takes the copy.
    ShortHistory(String),
    /// The chart is skipped: this study reads another study's output, which a copy of it alone
    /// would not have. Copy the whole set with [`LayoutCopy::Studies`] instead.
    LinkedInput(String),
    /// The chart is skipped: this study cannot be built, for the given reason, for example a
    /// name the factory does not know.
    UnsupportedStudy {
        /// The study's name.
        study: String,
        /// Why it cannot be built.
        reason: String,
    },
    /// The chart is skipped: the copy would take it past its
    /// [study cap](Chart::set_study_limit).
    StudyLimit,
    /// The chart is skipped: it refuses these settings, for the given reason.
    UnsupportedSettings(String),
}

impl LayoutCopyIssueKind {
    /// Whether [`ChartLayout::apply_copy`] leaves the chart as it is.
    pub fn skips_chart(&self) -> bool {
        !matches!(self, Self::NoHistory | Self::ShortHistory(_))
    }
}

impl std::fmt::Display for LayoutCopyIssueKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHistory => write!(f, "no history yet; studies draw once bars arrive"),
            Self::ShortHistory(study) => write!(f, "too little history to draw {study}"),
            Self::LinkedInput(study) => {
                write!(f, "skipped: {study} reads another study's output")
            }
            Self::UnsupportedStudy { study, reason } => write!(f, "skipped: {study}: {reason}"),
            Self::StudyLimit => write!(f, "skipped: study limit reached"),
            Self::UnsupportedSettings(reason) => write!(f, "skipped: {reason}"),
        }
    }
}

/// Whether `study` has a value to draw.
fn draws(study: &dyn Indicator) -> bool {
    study
        .values()
        .columns()
        .iter()
        .flatten()
        .any(|v| v.is_finite())
}

/// The studies a copy takes from the source chart.
struct CopiedStudies {
    set: StudyTemplateSet,
    /// Whether each study draws on the source, by its position in the set.
    draws: Vec<bool>,
    /// Whether the other charts keep the studies they have.
    adding: bool,
}

impl CopiedStudies {
    /// `None` for a copy of settings, or of a study the chart does not have.
    fn new(source: &Chart, copy: &LayoutCopy) -> Option<Self> {
        let set = source.study_template_set();
        let draws: Vec<_> = source.indicators().map(|(_, study)| draws(study)).collect();
        match copy {
            LayoutCopy::Study(id) => {
                let at = source.indicators().position(|(study, _)| study == *id)?;
                let on_price_pane = set.price_pane.contains(&at);
                Some(Self {
                    set: StudyTemplateSet {
                        studies: vec![set.studies[at].clone()],
                        price_pane: if on_price_pane { vec![0] } else { Vec::new() },
                        panes: if on_price_pane {
                            Vec::new()
                        } else {
                            vec![vec![0]]
                        },
                    },
                    draws: vec![draws[at]],
                    adding: true,
                })
            }
            LayoutCopy::Studies => Some(Self {
                set,
                draws,
                adding: false,
            }),
            LayoutCopy::Settings(_) => None,
        }
    }

    fn issues(&self, chart: &Chart, factory: &IndicatorFactory) -> Vec<LayoutCopyIssueKind> {
        let mut issues = Vec::new();
        let kept = if self.adding {
            chart.indicators().count()
        } else {
            0
        };
        if chart
            .study_limit
            .is_some_and(|limit| kept + self.set.studies.len() > limit)
        {
            issues.push(LayoutCopyIssueKind::StudyLimit);
        }
        let no_bars = chart.state.data().bars.is_empty();
        if no_bars {
            issues.push(LayoutCopyIssueKind::NoHistory);
        }
        for (template, source_draws) in self.set.studies.iter().zip(&self.draws) {
            let linked = template.inputs.iter().any(|(_, value)| {
                matches!(value, InputValue::Source(StudySource::Study { id, .. }) if *id != NO_STUDY)
            });
            if linked && self.adding {
                issues.push(LayoutCopyIssueKind::LinkedInput(template.name.clone()));
                continue;
            }
            match template.create_with(factory) {
                Err(error) => issues.push(LayoutCopyIssueKind::UnsupportedStudy {
                    study: template.name.clone(),
                    reason: error.to_string(),
                }),
                // A linked study's values follow its parent, which reports its own shortage.
                Ok(study) if *source_draws && !no_bars && !linked => {
                    if !draws(chart.prepare_indicator(study).as_ref()) {
                        issues.push(LayoutCopyIssueKind::ShortHistory(template.name.clone()));
                    }
                }
                Ok(_) => {}
            }
        }
        issues
    }
}

/// Applies the settings of the chart with `from` as its configuration and `from_decimals` as its
/// instrument's precision to another chart. That chart keeps the parameters of its chart type,
/// whose sizes and base prices suit its own prices, and takes the colours of `from` for its
/// candles when the settings describe another style of candles and so do not hold them.
///
/// shortcut: a precision chosen equal to the source instrument's follows each chart's instrument,
/// as a chart does not keep whether its precision was chosen; keep that choice to tell them apart.
fn apply_to_other_chart(
    settings: &ChartSettingsState,
    from: &ChartConfig,
    from_decimals: Option<u8>,
    chart: &mut Chart,
) {
    let mut settings = settings.clone();
    if settings.precision.decimals().map(|value| value as u8) == from_decimals {
        settings.precision = PrecisionMode::Default;
    }
    if let Some(params) = settings.chart_type_params.take() {
        let mut own = chart.chart_type_params().clone();
        own.styles = params.styles;
        own.footprint_colors = params.footprint_colors;
        if &own != chart.chart_type_params() {
            chart.set_chart_type_params(own);
        }
    }
    if settings.apply_to_chart(chart).is_ok()
        && matches!(
            settings.chart_type,
            ChartType::Heikin | ChartType::HollowCandles
        )
    {
        let to = chart.config_mut();
        to.bullish_color = from.bullish_color;
        to.bearish_color = from.bearish_color;
        to.bullish_border_color = from.bullish_border_color;
        to.bearish_border_color = from.bearish_border_color;
        to.bullish_wick_color = from.bullish_wick_color;
        to.bearish_wick_color = from.bearish_wick_color;
        to.candle_border_width = from.candle_border_width;
    }
}

impl ChartLayout {
    /// The charts of the current preset, other than slot `source`, that cannot fully take `copy`
    /// from that slot, for the host to show before [`Self::apply_copy`]. The result says what
    /// each chart lacks: an empty result means every chart takes the whole copy. Changes
    /// nothing. Empty for a source slot or study that does not exist, which copies nothing.
    /// `factory` resolves the names of built-in and host-registered studies.
    pub fn copy_issues(
        &self,
        charts: &[&mut Chart],
        source: usize,
        copy: &LayoutCopy,
        factory: &IndicatorFactory,
    ) -> Vec<LayoutCopyIssue> {
        let Some(from) = charts.get(source) else {
            return Vec::new();
        };
        let studies = CopiedStudies::new(from, copy);
        let mut issues = Vec::new();
        for (slot, chart) in charts.iter().enumerate().take(self.preset_charts(charts)) {
            let kinds = match (copy, &studies) {
                (LayoutCopy::Settings(settings), _) => settings
                    .validate()
                    .err()
                    .map(|error| LayoutCopyIssueKind::UnsupportedSettings(error.to_string()))
                    .into_iter()
                    .collect(),
                (_, Some(studies)) if slot != source => studies.issues(chart, factory),
                _ => Vec::new(),
            };
            issues.extend(kinds.into_iter().map(|kind| LayoutCopyIssue { slot, kind }));
        }
        issues
    }

    /// Copies `copy` from slot `source` to the other charts of the current preset, and returns
    /// the [issues](Self::copy_issues) of the charts it reached or skipped. The source keeps its
    /// study ids, and charts the preset hides keep what they have.
    ///
    /// The whole copy is one edit. [`Chart::undo`] on any chart it reached reverses it on all of
    /// them at the next [`Self::show`], and [`Chart::redo`] brings it back; a chart edited since
    /// the copy undoes those later edits with it, which its own redo restores. A chart that was
    /// edited after the undo has no copy to redo and keeps that edit.
    pub fn apply_copy(
        &mut self,
        charts: &mut [&mut Chart],
        source: usize,
        copy: &LayoutCopy,
        factory: &IndicatorFactory,
    ) -> Vec<LayoutCopyIssue> {
        let issues = self.copy_issues(charts, source, copy, factory);
        let Some(from) = charts.get(source) else {
            return issues;
        };
        let studies = CopiedStudies::new(from, copy);
        if studies.is_none() && !matches!(copy, LayoutCopy::Settings(_)) {
            return issues;
        }
        let from_config = from.config().clone();
        let from_decimals = from.instrument().and_then(instrument_price_precision);
        let edit = self.edits.start();
        let mut reached = false;
        let count = self.preset_charts(charts);
        for (slot, chart) in charts.iter_mut().enumerate().take(count) {
            if slot == source
                || issues
                    .iter()
                    .any(|i| i.slot == slot && i.kind.skips_chart())
            {
                continue;
            }
            reached = true;
            chart.user_action(|chart| {
                chart.history.join_layout_edit(edit.clone());
                // The issues above already refused what would fail here.
                match (copy, &studies) {
                    (LayoutCopy::Settings(settings), _) => {
                        apply_to_other_chart(settings, &from_config, from_decimals, chart);
                    }
                    (_, Some(studies)) => {
                        chart
                            .place_study_template_set(&studies.set, factory, !studies.adding)
                            .ok();
                    }
                    _ => {}
                }
            });
        }
        let source_refused = issues.iter().any(|i| i.slot == source);
        if let LayoutCopy::Settings(settings) = copy
            && !source_refused
        {
            let chart = &mut charts[source];
            chart.user_action(|chart| {
                chart.history.join_layout_edit(edit.clone());
                settings.apply_to_chart(chart).ok();
            });
        } else if reached {
            // An undo from the chart the user copied from reverses the copy too.
            charts[source].user_action(|chart| chart.history.join_layout_edit(edit));
        }
        issues
    }

    fn preset_charts(&self, charts: &[&mut Chart]) -> usize {
        charts.len().min(16).min(self.mode.chart_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, BarData, Instrument, Timeframe};
    use crate::studies::{RSI, SMA};
    use crate::widget::settings::PrecisionMode;
    use crate::widget::{ChartLayoutMode, PaneId};
    use chrono::{DateTime, Duration};

    fn chart(bars: i64) -> Chart {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let mut chart = Chart::new(BarData::from_bars(
            (0..bars)
                .map(|i| {
                    let close = 10.0 + (i % 5) as f64;
                    Bar::new(start + Duration::minutes(i), close, 16.0, 9.0, close, 1.0)
                })
                .collect(),
        ));
        chart.set_symbol("SAMPLE");
        chart.set_timeframe(Timeframe::Min1);
        // Without the volume study a frame would add, the lists below hold only the copies.
        chart.config_mut().show_volume = false;
        chart
    }

    fn layout() -> ChartLayout {
        ChartLayout {
            mode: ChartLayoutMode::Three,
            ..ChartLayout::default()
        }
    }

    fn show(layout: &mut ChartLayout, charts: &mut [&mut Chart]) {
        egui::Context::default()
            .run_ui(egui::RawInput::default(), |ui| {
                layout.show(ui, charts);
            })
            .drop_without_applying_deltas();
    }

    fn names(chart: &Chart) -> Vec<String> {
        chart
            .indicators()
            .map(|(_, s)| s.name().to_owned())
            .collect()
    }

    /// One study reaches each other chart next to its studies, on the same kind of pane, and the
    /// whole copy reverses from any chart, even one edited since.
    #[test]
    fn one_study_copy_keeps_its_placement_and_reverses_as_one_edit() {
        let (mut a, mut b, mut c) = (chart(50), chart(50), chart(50));
        let overlay = a.add_indicator(Box::new(SMA::new(7)));
        let paned = a.add_indicator(Box::new(RSI::new(5)));
        c.add_indicator(Box::new(SMA::new(3)));
        let source_panes = a.panes();
        let factory = IndicatorFactory::new();
        let mut layout = layout();
        let mut charts = [&mut a, &mut b, &mut c];

        layout.apply_copy(&mut charts, 0, &LayoutCopy::Study(overlay), &factory);
        layout.apply_copy(&mut charts, 0, &LayoutCopy::Study(paned), &factory);
        assert_eq!(names(charts[1]), ["SMA", "RSI"]);
        assert_eq!(names(charts[2]), ["SMA", "SMA", "RSI"]);
        assert_eq!(charts[1].panes().len(), source_panes.len());
        let [(sma, _), (rsi, _)] = charts[1].indicators().collect::<Vec<_>>()[..] else {
            panic!("two studies");
        };
        assert_eq!(charts[1].series_pane(sma), Some(PaneId::MAIN));
        assert_ne!(charts[1].series_pane(rsi), Some(PaneId::MAIN));
        assert_eq!(
            charts[0].panes(),
            source_panes,
            "the source keeps its panes"
        );
        assert_eq!(charts[0].indicators().count(), 2, "and its studies");

        // An edit on another chart after the copy goes back with it, and its redo returns it.
        charts[2].user_action(|chart| chart.add_indicator(Box::new(SMA::new(9))));
        assert!(charts[1].undo(), "the target undoes the last copy");
        show(&mut layout, &mut charts);
        assert_eq!(names(charts[1]), ["SMA"]);
        assert_eq!(names(charts[2]), ["SMA", "SMA"]);

        assert!(charts[0].undo(), "the source undoes the first copy");
        show(&mut layout, &mut charts);
        assert_eq!(names(charts[1]), Vec::<String>::new());
        assert_eq!(names(charts[2]), ["SMA"]);
        assert_eq!(charts[0].indicators().count(), 2);

        assert!(charts[0].redo());
        show(&mut layout, &mut charts);
        assert_eq!(names(charts[1]), ["SMA"]);
        assert_eq!(names(charts[2]), ["SMA", "SMA"]);
    }

    /// The set replaces what each chart had, keeping links and shared panes; only the charts of
    /// the preset take it.
    #[test]
    fn study_set_copy_replaces_studies_with_links_and_panes() {
        let (mut a, mut b, mut hidden) = (chart(50), chart(50), chart(50));
        let parent = a.add_indicator(Box::new(RSI::new(5)));
        let child = a.add_indicator(Box::new(SMA::new(3)));
        a.set_indicator_source(
            child,
            StudySource::Study {
                id: parent.0,
                plot: 0,
            },
        )
        .unwrap();
        a.move_indicator_to_pane(child, parent);
        b.add_indicator(Box::new(SMA::new(20)));
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..ChartLayout::default()
        };
        let factory = IndicatorFactory::new();
        let mut charts = [&mut a, &mut b, &mut hidden];
        let issues = layout.apply_copy(&mut charts, 0, &LayoutCopy::Studies, &factory);
        assert_eq!(issues, []);
        assert_eq!(names(charts[1]), ["RSI", "SMA"]);
        assert_eq!(charts[2].indicators().count(), 0, "hidden slot");
        let ids: Vec<_> = charts[1].indicators().map(|(id, _)| id).collect();
        assert_eq!(charts[1].series_pane(ids[0]), charts[1].series_pane(ids[1]));
        assert_eq!(
            charts[1].indicators().nth(1).unwrap().1.source(),
            Some(StudySource::Study {
                id: ids[0].0,
                plot: 0
            })
        );
    }

    /// Issues name what a chart lacks before anything changes, and a chart with a blocking issue
    /// is left alone while the others take the copy.
    #[test]
    fn issues_are_reported_before_applying_and_skip_only_refusing_charts() {
        use LayoutCopyIssueKind as Kind;
        let (mut a, mut empty, mut short, mut full) = (chart(50), chart(0), chart(2), chart(50));
        let parent = a.add_indicator(Box::new(RSI::new(5)));
        let sma = a.add_indicator(Box::new(SMA::new(7)));
        let linked = a.add_indicator(Box::new(SMA::new(3)));
        a.set_indicator_source(
            linked,
            StudySource::Study {
                id: parent.0,
                plot: 0,
            },
        )
        .unwrap();
        full.set_study_limit(Some(0));
        short.config_mut().show_volume = true;
        let factory = IndicatorFactory::new();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::Four,
            ..ChartLayout::default()
        };
        let mut charts = [&mut a, &mut empty, &mut short, &mut full];

        let copy = LayoutCopy::Study(sma);
        let expected = [
            LayoutCopyIssue {
                slot: 1,
                kind: Kind::NoHistory,
            },
            LayoutCopyIssue {
                slot: 2,
                kind: Kind::ShortHistory("SMA".into()),
            },
            LayoutCopyIssue {
                slot: 3,
                kind: Kind::StudyLimit,
            },
        ];
        assert_eq!(layout.copy_issues(&charts, 0, &copy, &factory), expected);
        assert_eq!(
            charts[1].indicators().count(),
            0,
            "previewing changes nothing"
        );

        assert_eq!(layout.apply_copy(&mut charts, 0, &copy, &factory), expected);
        assert_eq!(names(charts[1]), ["SMA"]);
        assert_eq!(names(charts[2]), ["SMA"]);
        assert!(charts[2].config().show_volume, "adding keeps its volume");
        assert_eq!(charts[3].indicators().count(), 0);
        assert!(!charts[3].can_undo());

        let copy = LayoutCopy::Study(linked);
        let issues = layout.apply_copy(&mut charts, 0, &copy, &factory);
        let refused = issues
            .iter()
            .filter(|i| i.kind == Kind::LinkedInput("SMA".into()))
            .count();
        assert_eq!(refused, 3);
        assert_eq!(names(charts[1]), ["SMA"], "a linked study copies nowhere");
        assert_eq!(names(charts[2]), ["SMA"]);
    }

    /// Settings reach every chart whatever its style of candles, and leave its chart type, the
    /// parameters of that type and the precision of its instrument alone.
    #[test]
    fn settings_copy_reaches_every_chart_and_reverses_as_one_edit() {
        let (mut a, mut b) = (chart(50), chart(50));
        b.set_chart_type(ChartType::HollowCandles);
        let mut params = b.chart_type_params().clone();
        params.renko_brick_size = 0.25;
        b.set_chart_type_params(params);
        a.set_instrument(Some(Instrument::new("SAMPLE").with_tick_size(0.01)));
        b.set_instrument(Some(Instrument::new("SAMPLE").with_tick_size(0.0001)));
        a.config_mut().bullish_color = egui::Color32::RED;
        a.set_chart_type(ChartType::Heikin);
        let mut settings = ChartSettingsState::from_chart(&a);
        settings.chart_basic_styles.background_color = egui::Color32::KHAKI;
        settings.candle_colors.body_up = egui::Color32::GOLD;
        let factory = IndicatorFactory::new();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..ChartLayout::default()
        };
        let mut charts = [&mut a, &mut b];
        let before = charts[1].config().background_color;
        let copy = LayoutCopy::Settings(Box::new(settings));
        assert_eq!(layout.apply_copy(&mut charts, 0, &copy, &factory), []);
        for chart in &charts {
            assert_eq!(chart.config().background_color, egui::Color32::KHAKI);
            assert_eq!(
                chart.chart_type_params().styles.heikin.body_up,
                egui::Color32::GOLD
            );
            assert_eq!(chart.config().bullish_color, egui::Color32::RED);
        }
        assert_eq!(charts[1].chart_type(), ChartType::HollowCandles);
        assert_eq!(charts[1].chart_type_params().renko_brick_size, 0.25);
        let precision = |chart: &Chart| ChartSettingsState::from_chart(chart).precision;
        assert_eq!(precision(charts[0]), PrecisionMode::Decimals2);
        assert_eq!(precision(charts[1]), PrecisionMode::Decimals4);

        assert!(charts[0].undo());
        show(&mut layout, &mut charts);
        assert_eq!(charts[1].config().background_color, before);
        assert_eq!(charts[0].config().background_color, before);
    }

    /// The steps taken on one chart before the layout's next frame leave every chart in the state
    /// of the last one.
    #[test]
    fn undo_and_redo_before_the_next_frame_keep_the_copy() {
        let cases: [(&[bool], [usize; 3]); 2] = [
            (&[false, true], [1, 1, 1]),
            (&[false, true, false], [1, 0, 0]),
        ];
        for (steps, expected) in cases {
            let (mut a, mut b, mut c) = (chart(50), chart(50), chart(50));
            let sma = a.add_indicator(Box::new(SMA::new(7)));
            let factory = IndicatorFactory::new();
            let mut layout = layout();
            let mut charts = [&mut a, &mut b, &mut c];
            layout.apply_copy(&mut charts, 0, &LayoutCopy::Study(sma), &factory);
            for redo in steps {
                assert!(if *redo { charts[1].redo() } else { charts[1].undo() });
            }
            show(&mut layout, &mut charts);
            let counts = charts.each_ref().map(|chart| chart.indicators().count());
            assert_eq!(counts, expected, "steps {steps:?}");
        }
    }

    /// Undoing a settings copy restores the precision, event markers and price line colour it
    /// changed, on the source and on the other chart, and redo applies them again.
    #[test]
    fn settings_copy_undo_restores_precision_events_and_price_line() {
        let (mut a, mut b) = (chart(50), chart(50));
        for chart in [&mut a, &mut b] {
            chart.set_instrument(Some(Instrument::new("SAMPLE").with_tick_size(0.01)));
        }
        let mut settings = ChartSettingsState::from_chart(&a);
        settings.precision = PrecisionMode::Decimals4;
        settings.events.show_earnings = true;
        settings.scales_and_lines.symbol_price_color = Some(egui::Color32::GOLD);
        let state = |chart: &Chart| {
            (
                ChartSettingsState::from_chart(chart).precision,
                chart.event_visibility().show_earnings,
                chart
                    .series_style(SeriesId::MAIN)
                    .and_then(|style| style.price_line_color),
            )
        };
        let before = (PrecisionMode::Decimals2, false, None);
        let applied = (PrecisionMode::Decimals4, true, Some(egui::Color32::GOLD));
        assert_eq!([state(&a), state(&b)], [before, before]);
        let factory = IndicatorFactory::new();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..ChartLayout::default()
        };
        let mut charts = [&mut a, &mut b];
        let copy = LayoutCopy::Settings(Box::new(settings));
        assert_eq!(layout.apply_copy(&mut charts, 0, &copy, &factory), []);
        assert_eq!([state(charts[0]), state(charts[1])], [applied, applied]);
        assert!(charts[0].undo());
        show(&mut layout, &mut charts);
        assert_eq!([state(charts[0]), state(charts[1])], [before, before]);
        assert!(charts[0].redo());
        show(&mut layout, &mut charts);
        assert_eq!([state(charts[0]), state(charts[1])], [applied, applied]);
    }
}
