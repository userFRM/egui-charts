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
    /// symbol, interval and chart type, the sizes and other parameters of that type, its own
    /// price-to-bar ratio when the settings lock it, its quantity unit and order-flow sizes, and
    /// the precision of its instrument unless the source was given another one.
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
        let mut built: Vec<Box<dyn Indicator>> = Vec::new();
        let mut ids = Vec::new();
        for (at, template) in self.set.studies.iter().enumerate() {
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
                Ok(study) => {
                    built.push(chart.configure_indicator(study));
                    // The set's links name a study by its position in the set.
                    ids.push(SeriesId(at));
                }
            }
        }
        let requests = built
            .iter()
            .flat_map(|study| study.data_requests())
            .collect();
        let sources =
            chart
                .study_data
                .data(requests, &chart.symbol, chart.timeframe, chart.replay_time());
        crate::widget::studies::calculate(&mut built, &ids, &chart.state.data().bars, &sources);
        for (study, id) in built.iter().zip(&ids) {
            if self.draws[id.0] && !no_bars && !draws(study.as_ref()) {
                let name = self.set.studies[id.0].name.clone();
                issues.push(LayoutCopyIssueKind::ShortHistory(name));
            }
        }
        issues
    }
}

/// Applies the settings of the chart with `from` as its configuration and `from_decimals` as its
/// instrument's precision to another chart. That chart keeps the parameters of its chart type,
/// whose sizes and base prices suit its own prices, its own price-to-bar ratio when the settings
/// lock it, and its quantity unit and order-flow sizes, which suit its own instrument. It takes
/// the colours of `from` for its candles when the settings describe another style of candles and
/// so do not hold them.
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
    // Order-flow sizes and the quantity unit are in the source's sizes: each chart keeps its own.
    let own = ChartSettingsState::from_chart(chart).order_flow;
    let flow = &mut settings.order_flow;
    flow.heatmap.lower_cutoff = own.heatmap.lower_cutoff;
    flow.heatmap.full_intensity_size = own.heatmap.full_intensity_size;
    flow.order_book.quantity_unit = own.order_book.quantity_unit;
    flow.order_book.tiers = own.order_book.tiers;
    flow.order_book.filter.min_size = own.order_book.filter.min_size;
    flow.volume_bubbles.min_size_filter = own.volume_bubbles.min_size_filter;
    let own_ratio = chart.state().price_scale().bar_ratio();
    if settings.apply_to_chart(chart).is_err() {
        return;
    }
    if settings.scales_and_lines.lock_price_to_bar_ratio {
        // The ratio is in the source's prices per bar: keep this chart's own lock, or lock it at
        // its own view on its next frame.
        chart.price_scale_mut().set_bar_ratio(own_ratio);
        chart.last_ratio_lock = own_ratio.is_some();
    }
    if matches!(
        settings.chart_type,
        ChartType::Heikin | ChartType::HollowCandles | ChartType::VolumeCandles
    ) {
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

    fn pair() -> ChartLayout {
        ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..ChartLayout::default()
        }
    }

    /// A volume candles source's settings hold only its volume candle styles, so the other charts
    /// take its candle colours from its configuration.
    #[test]
    fn settings_copy_from_volume_candles_gives_candle_colours() {
        let (mut a, mut b) = (chart(50), chart(50));
        a.config_mut().bullish_color = egui::Color32::RED;
        a.set_chart_type(ChartType::VolumeCandles);
        let copy = LayoutCopy::Settings(Box::new(ChartSettingsState::from_chart(&a)));
        let factory = IndicatorFactory::new();
        let mut charts = [&mut a, &mut b];
        assert_eq!(pair().apply_copy(&mut charts, 0, &copy, &factory), []);
        assert_eq!(charts[1].config().bullish_color, egui::Color32::RED);
    }

    /// A locked price-to-bar ratio is in the source's prices. A chart whose prices are a thousandth
    /// of the source's locks at its own view, and a chart that has its own lock keeps it.
    #[test]
    fn settings_copy_keeps_a_price_per_bar_each_chart_can_show() {
        use crate::widget::test_frames::frame;
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let mut b = Chart::new(BarData::from_bars(
            (0..50)
                .map(|i| {
                    let close = (10.0 + (i % 5) as f64) / 1000.0;
                    Bar::new(start + Duration::minutes(i), close, 0.016, 0.009, close, 1.0)
                })
                .collect(),
        ));
        b.set_timeframe(Timeframe::Min1);
        b.config_mut().show_volume = false;
        let mut a = chart(50);
        let (ctx_a, ctx_b) = (egui::Context::default(), egui::Context::default());
        frame(&ctx_a, &mut a, 0.0, vec![]);
        frame(&ctx_b, &mut b, 0.0, vec![]);
        // The source locks its scale at its own view, as ticking the setting on it does.
        let (spacing, height) = (
            a.state.time_scale().bar_spacing(),
            a.get_rendered_price_rect().height(),
        );
        a.state
            .price_scale_mut()
            .set_bar_ratio_locked(true, spacing, height);
        let settings = ChartSettingsState::from_chart(&a);
        let ratio = settings.scales_and_lines.price_to_bar_ratio;
        let copy = LayoutCopy::Settings(Box::new(settings));
        let factory = IndicatorFactory::new();
        let mut charts = [&mut a, &mut b];

        assert_eq!(pair().apply_copy(&mut charts, 0, &copy, &factory), []);
        assert_eq!(charts[0].state.price_scale().bar_ratio(), Some(ratio));
        frame(&ctx_b, charts[1], 1.0, vec![]);
        let scale = charts[1].state.price_scale();
        let (own_lock, range) = (scale.bar_ratio(), scale.range());
        assert!(own_lock.is_some(), "locked at its own view");
        // Its bars span 0.009 to 0.016: ten times the span it fits them in is still below 0.0875.
        assert!(range.max - range.min < 0.0875, "{range:?}");

        assert_eq!(pair().apply_copy(&mut charts, 0, &copy, &factory), []);
        assert_eq!(charts[1].state.price_scale().bar_ratio(), own_lock);
    }

    /// Order-flow sizes and the quantity unit describe the source's instrument: a chart of another
    /// symbol keeps its own, and takes the rest, such as the heatmap's brightness.
    #[test]
    fn settings_copy_keeps_each_symbols_order_flow_sizes() {
        use crate::chart::overlays::{LiquidityHeatmapConfig, OrderBookView, VolumeBubblesConfig};
        struct Feed;
        impl crate::data::Datafeed for Feed {
            fn history(
                &self,
                _: crate::data::HistoryRequest,
                reply: crate::data::Reply<crate::data::HistoryPage>,
            ) {
                reply.send(Ok(crate::data::HistoryPage::new(Vec::new(), true)));
            }
        }
        let build = |symbol: &str| {
            crate::widget::ChartBuilder::new()
                .with_feed(std::sync::Arc::new(Feed))
                .with_symbol(symbol)
                .with_timeframe(Timeframe::Min1)
                .build()
        };
        let (mut a, mut b) = (build("ES"), build("BTCUSD"));
        let mut book = OrderBookView {
            quantity_unit: Some("contracts".into()),
            tiers: Some(crate::data::OrderSizeTiers {
                notable: 100.0,
                large: 500.0,
            }),
            ..OrderBookView::default()
        };
        book.filter.min_size = 5.0;
        a.set_order_book_view(book);
        a.set_liquidity_heatmap_config(LiquidityHeatmapConfig {
            brightness: 0.5,
            lower_cutoff: 50.0,
            full_intensity_size: Some(1000.0),
        });
        a.set_volume_bubbles(Some(VolumeBubblesConfig {
            min_size_filter: Some(10.0),
            ..VolumeBubblesConfig::default()
        }));
        b.set_order_book_view(OrderBookView {
            quantity_unit: Some("BTC".into()),
            ..OrderBookView::default()
        });
        let copy = LayoutCopy::Settings(Box::new(ChartSettingsState::from_chart(&a)));
        let factory = IndicatorFactory::new();
        let mut charts = [&mut a, &mut b];
        assert_eq!(pair().apply_copy(&mut charts, 0, &copy, &factory), []);
        let flow = ChartSettingsState::from_chart(charts[1]).order_flow;
        assert_eq!(
            (
                flow.order_book.quantity_unit.as_deref(),
                flow.order_book.tiers,
                flow.order_book.filter.min_size,
                flow.heatmap.lower_cutoff,
                flow.heatmap.full_intensity_size,
                flow.volume_bubbles.min_size_filter,
            ),
            (Some("BTC"), None, 0.0, 0.0, None, None),
        );
        assert_eq!(flow.heatmap.brightness, 0.5);
    }

    /// A study that reads another study's output and needs more bars than a chart has is reported,
    /// as a study reading prices is.
    #[test]
    fn short_history_is_reported_for_a_linked_study() {
        let (mut a, mut short, mut long) = (chart(50), chart(20), chart(50));
        let rsi = a.add_indicator(Box::new(RSI::new(5)));
        let mean = a.add_indicator(Box::new(SMA::new(30)));
        a.set_indicator_source(
            mean,
            StudySource::Study {
                id: rsi.0,
                plot: 0,
            },
        )
        .unwrap();
        a.move_indicator_to_pane(mean, rsi);
        a.recalculate_studies();
        assert!(a.indicators().all(|(_, study)| draws(study)));
        let factory = IndicatorFactory::new();
        let mut layout = layout();
        let mut charts = [&mut a, &mut short, &mut long];
        // RSI(5) has values from the sixth bar, so an SMA of 30 of them needs 35 bars.
        assert_eq!(
            layout.apply_copy(&mut charts, 0, &LayoutCopy::Studies, &factory),
            [LayoutCopyIssue {
                slot: 1,
                kind: LayoutCopyIssueKind::ShortHistory("SMA".into()),
            }],
        );
        charts[1].recalculate_studies();
        let drawn: Vec<_> = charts[1]
            .indicators()
            .map(|(_, study)| (study.name().to_owned(), draws(study)))
            .collect();
        assert_eq!(drawn, [("RSI".to_owned(), true), ("SMA".to_owned(), false)]);
    }
}
