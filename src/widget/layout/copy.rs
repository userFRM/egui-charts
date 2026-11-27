//! Copies of a chart's studies and settings to the other charts of a layout, undone and redone
//! together.

use super::{ChartLayout, MAX_CHARTS};
use crate::chart::overlays::BubbleGrouping;
use crate::config::ChartConfig;
use crate::model::{Bar, ChartType, ChartTypeParams, FootprintIntensity, TradeProfileSettings};
use crate::studies::{Indicator, IndicatorFactory, InputValue, StudySource, StudyTemplateSet};
use crate::widget::settings::ChartSettingsState;
use crate::widget::studies::reads_study;
use crate::widget::study_templates::{Keep, NO_STUDY};
use crate::widget::{Chart, SeriesId};

/// What [`ChartLayout::apply_copy`] takes from one chart to the others of the layout.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LayoutCopy {
    /// Adds this study to each other chart, next to the studies it has, with its inputs, styles,
    /// interval visibility, scale and input caption. A study on the source's price pane joins
    /// each chart's price pane; any other study gets a new pane at the bottom of each chart. A
    /// study on an additional scale gets one numbered apart from the scales each chart has.
    Study(SeriesId),
    /// Replaces the studies of each other chart with those of the source, with their links,
    /// shared panes and input captions, as [`Chart::apply_study_template_set_with`] does, but
    /// for the studies whose [user edits](Chart::set_indicator_user_edit_enabled) are off: each
    /// chart keeps its own, which count against its study cap, and the source's are not copied,
    /// so that copying again adds none of them twice. The source's other studies are added
    /// beside the kept ones, their additional scales numbered apart from those the kept studies
    /// use.
    Studies,
    /// Applies these settings to every chart, the source included. Each other chart takes the
    /// source's look for every chart type: the chart type styles; the footprint colours, columns,
    /// display switches, marker thresholds, text sizes and statistics rows, and its intensity
    /// unless the source measures it against a volume; the session volume profile colours, mode,
    /// node labels, point of control and value area lines with their extensions, developing
    /// levels, alignment, width, row values and session box; the TPO colours, drawing switches,
    /// captions and price scale tags, including period columns and the legacy poor highs and lows
    /// flag, block mode, outside value area opacity and initial balance minutes; and the Renko
    /// wicks and the real price labels of the price-based types. What suits each chart's own
    /// prices, sizes and sessions stays per chart: footprint row sizes, minimum imbalance volume
    /// and single print volume, profile sessions and rows, TPO row size, session splitting,
    /// period length and profile grouping, composites and manual profile splits, every value area
    /// share, and the sizes, sources, reversals, line counts, one step back rule and base of the
    /// other chart types. Each other chart keeps its symbol, interval and chart type, its own
    /// price-to-bar ratio when the settings lock it, its quantity unit and order-flow sizes, and
    /// its bubble grouping when the source uses a price step. It keeps the precision of its
    /// instrument unless the source was given another one. A chart without a feed has no order
    /// flow to give: from it, each chart keeps its order-flow layers and settings. A chart that
    /// has not been drawn yet fits its bars on its first frame, and a lock in the settings then
    /// locks that view. Each other chart keeps its futures data choices, since only its host can
    /// reload its history for them.
    Settings(Box<ChartSettingsState>),
}

/// A chart of the layout that a [`LayoutCopy`] cannot fully serve, for the host to show before it
/// applies the copy.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    /// The chart's bars, or the elements of its price-based chart type, are too few for this study
    /// to draw, though it draws on the source, on its elements for a price-based chart type. A
    /// price-based source that has not painted since its studies changed draws none yet. A study
    /// waiting for a history the chart asks for once it has the study, and a price-based chart
    /// that has not painted yet, are not reported. The chart takes the copy.
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
    /// The chart is skipped: the copy would add studies reading another study's output past its
    /// [chained study cap](Chart::set_chained_study_limit).
    ChainedStudyLimit,
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
            Self::ChainedStudyLimit => write!(f, "skipped: chained study limit reached"),
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
    /// The input caption each study shows on the source, by its position in the set.
    captions: Vec<String>,
    /// Which studies the other charts keep.
    keep: Keep,
}

impl CopiedStudies {
    /// `None` for a copy of settings, or of a study the chart does not have.
    fn new(source: &Chart, copy: &LayoutCopy) -> Option<Self> {
        let set = source.study_template_set();
        // A price-based chart builds its element results when it paints; until it repaints after
        // a study change, the held results do not describe its current studies.
        let rendered = source.rendered_studies().indicators();
        let current = !source.chart_type().is_time_independent() || !source.element_studies.dirty;
        let draws: Vec<_> = (0..source.indicators().count())
            .map(|at| current && rendered.get(at).is_some_and(|study| draws(study.as_ref())))
            .collect();
        let captions: Vec<_> = source
            .indicators()
            .map(|(id, _)| {
                source
                    .host_indicator_inputs(id)
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
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
                        ..StudyTemplateSet::default()
                    },
                    draws: vec![draws[at]],
                    captions: vec![captions[at].clone()],
                    keep: Keep::All,
                })
            }
            LayoutCopy::Studies => Some(Self {
                set,
                draws,
                captions,
                keep: Keep::ReadOnly,
            }),
            LayoutCopy::Settings(_) => None,
        }
    }

    fn issues(&self, chart: &Chart, factory: &IndicatorFactory) -> Vec<LayoutCopyIssueKind> {
        let mut issues = Vec::new();
        let kept = chart
            .indicators()
            .filter(|(_, study)| self.keep.keeps(*study))
            .count();
        let added = self
            .set
            .studies
            .iter()
            .filter(|template| match template.create_with(factory) {
                Ok(study) => self.keep.adds(study.as_ref()),
                // A study that cannot be built refuses the chart anyway.
                Err(_) => true,
            })
            .count();
        if chart.study_limit.is_some_and(|limit| kept + added > limit) {
            issues.push(LayoutCopyIssueKind::StudyLimit);
        }
        let kept_chained = chart
            .indicators()
            .filter(|(_, study)| self.keep.keeps(*study) && reads_study(*study))
            .count();
        let added_chained = self
            .set
            .studies
            .iter()
            .filter_map(|template| template.create_with(factory).ok())
            .filter(|study| self.keep.adds(study.as_ref()) && reads_study(study.as_ref()))
            .count();
        if chart.passes_chained_study_limit(kept_chained, added_chained) {
            issues.push(LayoutCopyIssueKind::ChainedStudyLimit);
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
            if linked && self.keep == Keep::All {
                issues.push(LayoutCopyIssueKind::LinkedInput(template.name.clone()));
                continue;
            }
            match template.create_with(factory) {
                Err(error) => issues.push(LayoutCopyIssueKind::UnsupportedStudy {
                    study: template.name.clone(),
                    reason: error.to_string(),
                }),
                Ok(study) if !self.keep.adds(study.as_ref()) => {}
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
        let sources = chart.study_data.data(
            requests,
            &chart.symbol,
            chart.timeframe,
            chart.replay_time(),
        );
        let elements = chart.chart_type().is_time_independent();
        let bars: &[Bar] = if elements {
            &chart.element_studies.bars
        } else {
            &chart.state.data().bars
        };
        crate::widget::studies::calculate(&mut built, &ids, bars, &sources);
        // The elements of a price-based chart are built when it paints, so a chart that has not
        // painted yet has no elements to count, which is not the same as too few.
        let unbuilt = elements && chart.element_studies.dirty && bars.is_empty();
        // The chart asks for the histories it lacks once it has the study, and a study that reads
        // a waiting study waits with it.
        let mut waiting = vec![false; built.len()];
        for at in crate::widget::studies::order(&built, &ids) {
            let parent = match built[at].source() {
                Some(StudySource::Study { id, .. }) => ids.iter().position(|item| item.0 == id),
                _ => None,
            };
            waiting[at] = parent.is_some_and(|parent| waiting[parent])
                || built[at]
                    .data_requests()
                    .iter()
                    .any(|request| sources.iter().all(|source| &source.request != request));
        }
        for ((study, id), waiting) in built.iter().zip(&ids).zip(waiting) {
            if self.draws[id.0] && !no_bars && !unbuilt && !waiting && !draws(study.as_ref()) {
                let name = self.set.studies[id.0].name.clone();
                issues.push(LayoutCopyIssueKind::ShortHistory(name));
            }
        }
        issues
    }
}

/// Applies the settings of the chart with `from` as its configuration to another chart, which
/// takes the source's chart type look and keeps what suits its own prices, sizes and sessions, as
/// [`LayoutCopy::Settings`] lists. It keeps its own price-to-bar ratio when the settings lock it,
/// its quantity unit and order-flow sizes, and its bubble grouping when the source uses a price
/// step, which suit its own instrument. It keeps all its order-flow settings when `from_feed` is
/// false, since a chart without a feed holds none. It takes the colours of `from` for its candles
/// when the settings describe another style of candles and so do not hold them. A precision left
/// at its default follows each chart's instrument.
fn apply_to_other_chart(
    settings: &ChartSettingsState,
    from: &ChartConfig,
    from_feed: bool,
    chart: &mut Chart,
) {
    let mut settings = settings.clone();
    if let Some(source) = settings.chart_type_params.take() {
        let own = chart.chart_type_params().clone();
        // Every field is named, so that a new one has to be sorted into the look, which follows
        // the source, or the values that suit each chart's own prices, sizes and sessions.
        let params = ChartTypeParams {
            footprint_modes: source.footprint_modes,
            footprint_profile: source.footprint_profile,
            footprint_color_by: source.footprint_color_by,
            // A volume is in the source's sizes; the other measures reach every chart.
            footprint_intensity: match source.footprint_intensity {
                FootprintIntensity::Volume(_) => own.footprint_intensity,
                intensity => intensity,
            },
            footprint_colors: source.footprint_colors,
            footprint_imbalance_colors: source.footprint_imbalance_colors,
            footprint_step_colors: source.footprint_step_colors,
            footprint_auto_row_height: source.footprint_auto_row_height,
            footprint_min_text_height: source.footprint_min_text_height,
            footprint_imbalance: source.footprint_imbalance,
            footprint_poc: source.footprint_poc,
            footprint_imbalance_ratio: source.footprint_imbalance_ratio,
            footprint_diagonal: source.footprint_diagonal,
            footprint_stacked_imbalances: source.footprint_stacked_imbalances,
            footprint_stack: source.footprint_stack,
            footprint_zones_until_broken: source.footprint_zones_until_broken,
            footprint_naked_poc: source.footprint_naked_poc,
            footprint_delta_poc: source.footprint_delta_poc,
            footprint_unfinished_auctions: source.footprint_unfinished_auctions,
            footprint_cluster_share: source.footprint_cluster_share,
            footprint_delta_percent: source.footprint_delta_percent,
            footprint_ratio: source.footprint_ratio,
            footprint_extend_bars: source.footprint_extend_bars,
            footprint_extreme_ratio: source.footprint_extreme_ratio,
            footprint_pullback: source.footprint_pullback,
            footprint_abbreviate: source.footprint_abbreviate,
            footprint_show_zero: source.footprint_show_zero,
            footprint_font_size: source.footprint_font_size,
            footprint_info_box: source.footprint_info_box,
            footprint_statistics: source.footprint_statistics,
            footprint_statistics_rows: source.footprint_statistics_rows,
            footprint_session_cumulative_delta: source.footprint_session_cumulative_delta,
            profile: TradeProfileSettings {
                mode: source.profile.mode,
                developing_poc: source.profile.developing_poc,
                developing_value_area: source.profile.developing_value_area,
                extend_poc: source.profile.extend_poc,
                extend_vah: source.profile.extend_vah,
                extend_val: source.profile.extend_val,
                nodes: source.profile.nodes,
                value_area_guides: source.profile.value_area_guides,
                poc: source.profile.poc,
                poc_color: source.profile.poc_color,
                value_area_high: source.profile.value_area_high,
                value_area_high_color: source.profile.value_area_high_color,
                value_area_low: source.profile.value_area_low,
                value_area_low_color: source.profile.value_area_low_color,
                up_volume_color: source.profile.up_volume_color,
                down_volume_color: source.profile.down_volume_color,
                value_area_up_color: source.profile.value_area_up_color,
                value_area_down_color: source.profile.value_area_down_color,
                right_aligned: source.profile.right_aligned,
                width: source.profile.width,
                values: source.profile.values,
                values_color: source.profile.values_color,
                session_box: source.profile.session_box,
                session_box_color: source.profile.session_box_color,
                // Sessions, rows and the value area share suit the chart's own instrument.
                session: own.profile.session,
                rows: own.profile.rows,
                value_area: own.profile.value_area,
            },
            styles: source.styles,
            renko_wicks: source.renko_wicks,
            renko_real_price: source.renko_real_price,
            kagi_real_price: source.kagi_real_price,
            line_break_real_price: source.line_break_real_price,
            pnf_real_price: source.pnf_real_price,
            tpo_initial_balance_minutes: source.tpo_initial_balance_minutes,
            tpo_poor_highs_lows: source.tpo_poor_highs_lows,
            tpo_poor_high: source.tpo_poor_high,
            tpo_poor_low: source.tpo_poor_low,
            tpo_extend_poor: source.tpo_extend_poor,
            tpo_extend_poc: source.tpo_extend_poc,
            tpo_extend_vah: source.tpo_extend_vah,
            tpo_extend_val: source.tpo_extend_val,
            tpo_extend_single_prints: source.tpo_extend_single_prints,
            tpo_initial_balance_extensions: source.tpo_initial_balance_extensions,
            tpo_single_prints: source.tpo_single_prints,
            tpo_midpoint: source.tpo_midpoint,
            tpo_letters: source.tpo_letters,
            tpo_statistics: source.tpo_statistics,
            tpo_statistics_rows: source.tpo_statistics_rows,
            tpo_colored_blocks: source.tpo_colored_blocks,
            tpo_block_colors: source.tpo_block_colors,
            tpo_block_color: source.tpo_block_color,
            tpo_outside_value_area_opacity: source.tpo_outside_value_area_opacity,
            tpo_split_periods: source.tpo_split_periods,
            tpo_initial_balance: source.tpo_initial_balance,
            tpo_labels: source.tpo_labels,
            tpo_level_tags: source.tpo_level_tags,
            tpo_poc: source.tpo_poc,
            tpo_value_area_levels: source.tpo_value_area_levels,
            tpo_open_price: source.tpo_open_price,
            tpo_close_price: source.tpo_close_price,
            tpo_volume_profile: source.tpo_volume_profile,
            // Row sizes, value area shares, volumes, sessions and the way price-based elements
            // are built suit the chart's own instrument.
            footprint_row_ticks: own.footprint_row_ticks,
            footprint_row_atr: own.footprint_row_atr,
            footprint_row_atr_length: own.footprint_row_atr_length,
            footprint_value_area: own.footprint_value_area,
            footprint_imbalance_min_volume: own.footprint_imbalance_min_volume,
            footprint_single_print_max: own.footprint_single_print_max,
            renko_size_method: own.renko_size_method,
            kagi_size_method: own.kagi_size_method,
            range_size_method: own.range_size_method,
            pnf_size_method: own.pnf_size_method,
            renko_source: own.renko_source,
            kagi_source: own.kagi_source,
            line_break_source: own.line_break_source,
            pnf_source: own.pnf_source,
            renko_brick_size: own.renko_brick_size,
            kagi_reversal: own.kagi_reversal,
            range_size: own.range_size,
            pnf_box_size: own.pnf_box_size,
            pnf_reversal: own.pnf_reversal,
            line_break_count: own.line_break_count,
            pnf_one_step_back: own.pnf_one_step_back,
            baseline: own.baseline,
            columns_base: own.columns_base,
            tpo_period_minutes: own.tpo_period_minutes,
            tpo_row_ticks: own.tpo_row_ticks,
            tpo_value_area: own.tpo_value_area,
            tpo_split_sessions: own.tpo_split_sessions,
            tpo_session: own.tpo_session,
            tpo_profile_period: own.tpo_profile_period,
            tpo_composites: own.tpo_composites,
            tpo_splits: own.tpo_splits,
        };
        if &params != chart.chart_type_params() {
            chart.apply_chart_type_params(params);
        }
    }
    let own = ChartSettingsState::from_chart(chart).order_flow;
    if !from_feed {
        // A chart without a feed holds no order flow: each chart keeps all of its own.
        settings.order_flow = own;
    } else {
        // Order-flow sizes and the quantity unit are in the source's sizes: each chart keeps its own.
        let flow = &mut settings.order_flow;
        flow.heatmap.lower_cutoff = own.heatmap.lower_cutoff;
        flow.heatmap.full_intensity_size = own.heatmap.full_intensity_size;
        flow.order_book.quantity_unit = own.order_book.quantity_unit;
        flow.order_book.tiers = own.order_book.tiers;
        flow.order_book.filter.min_size = own.order_book.filter.min_size;
        flow.volume_bubbles.min_size_filter = own.volume_bubbles.min_size_filter;
        // A price step is in the source's prices too; other groupings reach every chart.
        if matches!(flow.volume_bubbles.grouping, BubbleGrouping::PriceStep(_)) {
            flow.volume_bubbles.grouping = own.volume_bubbles.grouping;
        }
    }
    // Only each chart's host can reload its history for the futures data choices.
    settings.back_adjust_futures = chart.back_adjust_futures;
    settings.settlement_as_close = chart.settlement_as_close;
    let own_ratio = chart.state().price_scale().bar_ratio();
    // A chart not drawn yet holds the scale's placeholder range, not a view of its own.
    // shortcut: a chart drawn before its bars changed counts as drawn; track a fit since the last
    // data change if hidden tabs switch symbols.
    let unfitted = chart.last_rendered_price_rect.is_negative()
        && own_ratio.is_none()
        && !chart.state().price_scale().is_manual();
    if settings.apply_to_chart(chart).is_err() {
        return;
    }
    if settings.scales_and_lines.lock_price_to_bar_ratio {
        // The ratio is in the source's prices per bar: keep this chart's own lock, or lock it at
        // its own view on its next frame.
        chart.price_scale_mut().set_bar_ratio(own_ratio);
        chart.last_ratio_lock = own_ratio.is_some();
    }
    if unfitted {
        // Its first frame fits its bars, and a lock then locks that view.
        chart.price_scale_mut().reset();
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
    /// The whole copy is one edit of the charts it changed and of the source, and a copy that
    /// changes no chart leaves no step to undo. [`Chart::undo`] on any of these charts reverses
    /// it on all of them at the next [`Self::show`], and [`Chart::redo`] brings it back; a chart
    /// edited since the copy undoes those later edits with it, which its own redo restores. A
    /// chart that was edited after the undo has no copy to redo and keeps that edit. A symbol or
    /// interval such an undo or redo restores on a chart reaches the charts linked to it.
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
        let from_feed = from.feed_link.is_some();
        let edit = self.edits.start();
        // Only the charts the copy changes take part in the edit, so that it leaves no empty step.
        let mut changed = false;
        let count = self.preset_charts(charts);
        for (slot, chart) in charts.iter_mut().enumerate().take(count) {
            if slot == source
                || issues
                    .iter()
                    .any(|i| i.slot == slot && i.kind.skips_chart())
            {
                continue;
            }
            // A chart inside a user action of the host's cannot tell yet whether this changes it.
            let nested = chart.history.recording();
            let edits = chart.edit_count();
            chart.user_action(|chart| {
                if nested {
                    chart.history.join_layout_edit(edit.clone());
                }
                // The issues above already refused what would fail here.
                match (copy, &studies) {
                    (LayoutCopy::Settings(settings), _) => {
                        apply_to_other_chart(settings, &from_config, from_feed, chart);
                    }
                    (_, Some(studies)) => {
                        let ids = chart
                            .place_study_template_set(&studies.set, factory, studies.keep)
                            .unwrap_or_default();
                        for (id, caption) in ids.into_iter().zip(&studies.captions) {
                            if let Some(id) = id {
                                chart.set_indicator_inputs(id, caption.clone());
                            }
                        }
                    }
                    _ => {}
                }
            });
            if nested {
                changed = true;
            } else if chart.edit_count() != edits {
                chart.history.join_last_to_layout_edit(edit.clone());
                changed = true;
            }
        }
        let source_refused = issues.iter().any(|i| i.slot == source);
        if let LayoutCopy::Settings(settings) = copy
            && !source_refused
        {
            let chart = &mut charts[source];
            chart.user_action(|chart| {
                if changed {
                    chart.history.join_layout_edit(edit.clone());
                }
                settings.apply_to_chart(chart).ok();
            });
        } else if changed {
            // An undo from the chart the user copied from reverses the copy too.
            charts[source].user_action(|chart| chart.history.join_layout_edit(edit));
        }
        issues
    }

    fn preset_charts(&self, charts: &[&mut Chart]) -> usize {
        charts.len().min(MAX_CHARTS).min(self.mode.chart_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Bar, BarData, FootprintColor, FootprintIntensity, FootprintMode, FootprintStatistic,
        Instrument, NodeSensitivity, ProfileRows, ProfileSession, TPOLabels, Timeframe,
    };
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
        a.set_indicator_inputs(overlay, "7 close");
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
        assert_eq!(charts[1].indicator_inputs(sma), Some("7 close"));
        assert_eq!(
            charts[0].panes(),
            source_panes,
            "the source keeps its panes"
        );
        assert_eq!(charts[0].indicators().count(), 2, "and its studies");
        charts[1]
            .indicator_mut(rsi)
            .unwrap()
            .set_input("period", InputValue::Length(4))
            .unwrap();
        assert_eq!(charts[1].indicator_inputs(rsi), Some("4 close"));

        // An edit on another chart after the copy goes back with it, and its redo returns it.
        charts[2].user_action(|chart| chart.add_indicator(Box::new(SMA::new(9))));
        show(&mut layout, &mut charts);
        let width = charts[0].last_rendered_price_rect.width();
        let mut sizes = layout.sizes(layout.mode);
        sizes.weights[1] = vec![3.0, 1.0];
        layout.set_active(2);
        layout.set_sizes(&[sizes], false).unwrap();
        show(&mut layout, &mut charts);
        assert!(charts[0].last_rendered_price_rect.width() > width);

        assert!(charts[1].undo(), "the target undoes the last copy");
        show(&mut layout, &mut charts);
        assert_eq!(
            charts[0].last_rendered_price_rect.width(),
            width,
            "the same frame renders the sizes restored by the queued undo"
        );
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
        assert_eq!(charts[1].indicator_inputs(sma), Some("7 close"));
    }

    /// A replacement layout follows old copies from either the source or a target. Clones
    /// showing other charts leave those steps queued for the layout showing their owners.
    #[test]
    fn copy_made_before_a_layout_clone_undoes_on_every_chart() {
        for (copy_set, busy) in [(false, false), (true, false), (false, true), (true, true)] {
            for slot in [0, 1] {
                let mut charts = [chart(50), chart(50), chart(50)];
                let sma = charts[0].add_indicator(Box::new(SMA::new(7)));
                let factory = IndicatorFactory::new();
                let mut layout = layout();
                let copy = if copy_set {
                    LayoutCopy::Studies
                } else {
                    LayoutCopy::Study(sma)
                };
                assert_eq!(
                    layout.apply_copy(&mut charts.each_mut(), 0, &copy, &factory),
                    []
                );
                layout = layout.clone();
                if busy {
                    charts[2].begin_user_action();
                }
                assert!(charts[slot].undo());
                show(&mut layout, &mut charts.each_mut());
                if busy {
                    charts[2].end_user_action();
                    show(&mut layout, &mut charts.each_mut());
                }
                assert_eq!(
                    charts.each_ref().map(|chart| chart.indicators().count()),
                    [1, 0, 0],
                    "set {copy_set}, slot {slot}"
                );
                assert!(charts[slot].redo());
                show(&mut layout, &mut charts.each_mut());
                assert_eq!(
                    charts.each_ref().map(|chart| chart.indicators().count()),
                    [1, 1, 1]
                );
            }
        }

        let mut first = [chart(50), chart(50), chart(50)];
        let mut second = [chart(50), chart(50), chart(50)];
        let sma = first[0].add_indicator(Box::new(SMA::new(7)));
        let rsi = second[0].add_indicator(Box::new(RSI::new(5)));
        let factory = IndicatorFactory::new();
        let mut original = layout();
        let mut clone = original.clone();
        original.apply_copy(&mut first.each_mut(), 0, &LayoutCopy::Study(sma), &factory);
        clone.apply_copy(&mut second.each_mut(), 0, &LayoutCopy::Study(rsi), &factory);
        for redo in [false, true] {
            let untouched = second.each_ref().map(|chart| chart.indicators().count());
            assert!(if redo {
                first[1].redo()
            } else {
                first[1].undo()
            });
            show(&mut clone, &mut second.each_mut());
            assert_eq!(
                second.each_ref().map(|chart| chart.indicators().count()),
                untouched
            );
            assert!(if redo {
                second[0].redo()
            } else {
                second[0].undo()
            });
            show(&mut original, &mut first.each_mut());
            let counts = if redo { [1, 1, 1] } else { [1, 0, 0] };
            assert_eq!(
                first.each_ref().map(|chart| chart.indicators().count()),
                counts
            );
            show(&mut clone, &mut second.each_mut());
            assert_eq!(
                second.each_ref().map(|chart| chart.indicators().count()),
                counts
            );
        }
    }

    /// Undoing a copy from one chart also undoes a later symbol or interval change of another
    /// chart that received it. The chart sharing that chart's group follows the restored value,
    /// as it follows a keyboard undo of the change.
    #[test]
    fn copy_undo_from_another_chart_keeps_a_symbol_group_together() {
        use crate::widget::{ChartLink, ChartLinkGroups};
        let state = |chart: &Chart| (chart.symbol().to_owned(), chart.timeframe());
        let original = ("SAMPLE".to_owned(), Some(Timeframe::Min1));
        for interval in [false, true] {
            let mut charts = [chart(50), chart(50), chart(50)];
            let sma = charts[0].add_indicator(Box::new(SMA::new(7)));
            let factory = IndicatorFactory::new();
            let mut layout = layout();
            let group = ChartLink::Group("1".to_owned());
            let linked = if interval {
                ChartLinkGroups {
                    interval: group,
                    ..Default::default()
                }
            } else {
                ChartLinkGroups {
                    symbol: group,
                    ..Default::default()
                }
            };
            layout.set_link_groups(0, linked.clone(), true);
            layout.set_link_groups(1, linked, true);
            show(&mut layout, &mut charts.each_mut());
            assert_eq!(
                layout.apply_copy(&mut charts.each_mut(), 0, &LayoutCopy::Study(sma), &factory),
                []
            );
            layout.set_active(1);
            charts[1].user_action(|chart| {
                if interval {
                    chart.set_timeframe(Timeframe::Hour1);
                } else {
                    chart.set_symbol("OTHER");
                }
            });
            show(&mut layout, &mut charts.each_mut());
            assert_ne!(state(&charts[1]), original);
            assert_eq!(
                state(&charts[0]),
                state(&charts[1]),
                "the group follows the change"
            );
            layout.set_active(2);
            assert!(charts[2].undo());
            for _ in 0..2 {
                show(&mut layout, &mut charts.each_mut());
            }
            assert_eq!(
                charts.each_ref().map(state),
                [original.clone(), original.clone(), original.clone()],
                "interval {interval}"
            );
        }
    }

    /// A copy made through a clone of the layout has its own id, so undoing it leaves the copies
    /// made through the original alone.
    #[test]
    fn copy_through_a_cloned_layout_undoes_only_itself() {
        let (mut a, mut b, mut c) = (chart(50), chart(50), chart(50));
        let sma = a.add_indicator(Box::new(SMA::new(7)));
        let rsi = a.add_indicator(Box::new(RSI::new(5)));
        let factory = IndicatorFactory::new();
        let mut layout = layout();
        let mut charts = [&mut a, &mut b, &mut c];
        layout.apply_copy(&mut charts, 0, &LayoutCopy::Study(sma), &factory);
        assert_eq!(names(charts[1]), ["SMA"]);
        assert_eq!(names(charts[2]), ["SMA"]);

        // The host replaces the layout with a clone, and slot 2 takes no more studies.
        let mut clone = layout.clone();
        charts[2].set_study_limit(Some(1));
        let issues = clone.apply_copy(&mut charts, 0, &LayoutCopy::Study(rsi), &factory);
        assert!(issues.iter().any(|i| i.slot == 2), "slot 2 is at its limit");
        assert_eq!(names(charts[1]), ["SMA", "RSI"]);
        assert_eq!(names(charts[2]), ["SMA"]);

        assert!(charts[1].undo(), "the target undoes the RSI copy");
        show(&mut clone, &mut charts);
        show(&mut layout, &mut charts);
        assert_eq!(names(charts[1]), ["SMA"]);
        assert_eq!(names(charts[2]), ["SMA"]);
    }

    /// The set replaces the studies each chart lets the user edit, keeping links, shared panes and
    /// captions; only the charts of the preset take it. The source's read-only studies stay its
    /// own, so that copying again changes nothing.
    #[test]
    fn study_set_copy_replaces_editable_studies_with_links_and_panes() {
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
        a.set_indicator_inputs(parent, "5");
        a.set_indicator_inputs(child, "3");
        b.add_indicator(Box::new(SMA::new(20)));
        let read_only = b.add_indicator(Box::new(crate::studies::EMA::new(9)));
        b.set_indicator_user_edit_enabled(read_only, false);
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..ChartLayout::default()
        };
        let factory = IndicatorFactory::new();
        let mut charts = [&mut a, &mut b, &mut hidden];
        let issues = layout.apply_copy(&mut charts, 0, &LayoutCopy::Studies, &factory);
        assert_eq!(issues, []);
        assert_eq!(
            names(charts[1]),
            ["EMA", "RSI", "SMA"],
            "a read-only study stays"
        );
        assert_eq!(charts[2].indicators().count(), 0, "hidden slot");
        let ids: Vec<_> = charts[1].indicators().skip(1).map(|(id, _)| id).collect();
        assert_eq!(charts[1].series_pane(ids[0]), charts[1].series_pane(ids[1]));
        let captions: Vec<_> = ids
            .iter()
            .map(|id| charts[1].indicator_inputs(*id))
            .collect();
        assert_eq!(captions, [Some("5"), Some("3")]);
        assert_eq!(
            charts[1].indicators().nth(2).unwrap().1.source(),
            Some(StudySource::Study {
                id: ids[0].0,
                plot: 0
            })
        );
        assert!(charts[1].undo());
        assert_eq!(names(charts[1]), ["SMA", "EMA"]);
        // The kept study counts against the cap.
        charts[1].set_study_limit(Some(2));
        assert_eq!(
            layout.copy_issues(&charts, 0, &LayoutCopy::Studies, &factory),
            [LayoutCopyIssue {
                slot: 1,
                kind: LayoutCopyIssueKind::StudyLimit
            }]
        );
        charts[1].set_study_limit(None);
        // Only the studies on studies the set adds count, beside those the chart keeps.
        charts[1].set_chained_study_limit(Some(0));
        assert_eq!(
            layout.copy_issues(&charts, 0, &LayoutCopy::Studies, &factory),
            [LayoutCopyIssue {
                slot: 1,
                kind: LayoutCopyIssueKind::ChainedStudyLimit
            }]
        );
        charts[1].set_chained_study_limit(Some(1));
        assert_eq!(
            layout.copy_issues(&charts, 0, &LayoutCopy::Studies, &factory),
            []
        );
        charts[1].set_chained_study_limit(None);
        let own = charts[0].add_indicator(Box::new(crate::studies::EMA::new(3)));
        charts[0].set_indicator_user_edit_enabled(own, false);
        for _ in 0..2 {
            layout.apply_copy(&mut charts, 0, &LayoutCopy::Studies, &factory);
            assert_eq!(names(charts[1]), ["EMA", "RSI", "SMA"]);
        }
    }

    /// A copied study on an additional scale gets one apart from the scales the other chart
    /// numbers alike, and studies of a set that share one still share one.
    #[test]
    fn copied_scales_take_numbers_the_chart_does_not_use() {
        use crate::model::SeriesScale;
        use crate::studies::StudyScale;
        let separate = |key| StudyScale::Shared(SeriesScale::Separate(key));
        let scales = |chart: &Chart| {
            chart
                .indicators()
                .map(|(_, study)| study.scale())
                .collect::<Vec<_>>()
        };
        let (mut a, mut b) = (chart(50), chart(50));
        let first = a.add_indicator(Box::new(SMA::new(7)));
        let second = a.add_indicator(Box::new(SMA::new(3)));
        a.set_indicator_scale(first, separate(0));
        a.set_indicator_scale(second, separate(0));
        let rsi = b.add_indicator(Box::new(RSI::new(5)));
        b.set_indicator_scale(rsi, separate(0));
        b.set_indicator_user_edit_enabled(rsi, false);
        let factory = IndicatorFactory::new();
        let mut layout = pair();
        let mut charts = [&mut a, &mut b];
        layout.apply_copy(&mut charts, 0, &LayoutCopy::Study(first), &factory);
        assert_eq!(scales(charts[1]), [separate(0), separate(1)]);
        layout.apply_copy(&mut charts, 0, &LayoutCopy::Studies, &factory);
        assert_eq!(scales(charts[1]), [separate(0), separate(1), separate(1)]);
    }

    /// A copy keeps the user's visibility switch apart from the interval settings: a study hidden
    /// only on the source's interval is drawn on a chart whose interval they allow, and a study
    /// switched off stays off there too.
    #[test]
    fn copies_keep_the_visibility_switch_apart_from_interval_visibility() {
        let visible = |chart: &Chart| {
            chart
                .indicators()
                .map(|(_, study)| study.is_visible())
                .collect::<Vec<_>>()
        };
        for set in [false, true] {
            let (mut a, mut b) = (chart(50), chart(50));
            b.set_timeframe(Timeframe::Min5);
            let by_interval = a.add_indicator(Box::new(SMA::new(7)));
            let study = a.indicator_mut(by_interval).unwrap();
            let mut settings = study.settings();
            settings.hidden_intervals = vec![Timeframe::Min1];
            study.apply_settings(&[], settings).unwrap();
            let off = a.add_indicator(Box::new(SMA::new(3)));
            a.set_indicator_visible(off, false);
            assert_eq!(visible(&a), [false, false]);
            let factory = IndicatorFactory::new();
            let mut layout = pair();
            let mut charts = [&mut a, &mut b];
            if set {
                layout.apply_copy(&mut charts, 0, &LayoutCopy::Studies, &factory);
            } else {
                for id in [by_interval, off] {
                    layout.apply_copy(&mut charts, 0, &LayoutCopy::Study(id), &factory);
                }
            }
            assert_eq!(visible(charts[1]), [true, false], "set {set}");
        }
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

    /// Settings reach every chart whatever its style of candles, give it the source's look for
    /// every chart type, and leave its chart type, the values that suit its own prices, sizes and
    /// sessions and the precision of its instrument alone.
    #[test]
    fn settings_copy_reaches_every_chart_and_reverses_as_one_edit() {
        let (mut a, mut b) = (chart(50), chart(50));
        b.set_chart_type(ChartType::HollowCandles);
        let mut params = b.chart_type_params().clone();
        params.renko_brick_size = 0.25;
        params.profile.session = ProfileSession::Regular;
        params.profile.rows = ProfileRows::Ticks(4);
        params.profile.value_area = Some(0.6);
        params.tpo_row_ticks = 3;
        params.tpo_value_area = 60.0;
        params.tpo_poor_highs_lows = true;
        params.tpo_poor_high = true;
        params.tpo_poor_low = true;
        params.tpo_split_sessions = true;
        params.tpo_session = ProfileSession::Regular;
        params.tpo_split_periods = true;
        params.tpo_period_minutes = 15;
        params.tpo_profile_period = crate::model::TPOProfilePeriod::Week;
        params.footprint_row_ticks = 4;
        params.footprint_row_atr = true;
        params.footprint_row_atr_length = 20;
        params.footprint_value_area = Some(0.6);
        params.footprint_imbalance_min_volume = 25.0;
        params.footprint_single_print_max = Some(5.0);
        params.footprint_intensity = FootprintIntensity::Visible;
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let composites = vec![start..=start + Duration::days(2)];
        let splits = vec![start + Duration::hours(3)];
        params.tpo_composites = composites.clone();
        params.tpo_splits = splits.clone();
        b.set_chart_type_params(params);
        a.set_instrument(Some(Instrument::new("SAMPLE").with_tick_size(0.01)));
        b.set_instrument(Some(Instrument::new("SAMPLE").with_tick_size(0.0001)));
        a.config_mut().bullish_color = egui::Color32::RED;
        a.set_chart_type(ChartType::Heikin);
        let mut params = a.chart_type_params().clone();
        let profile = &mut params.profile;
        profile.poc_color = Some(egui::Color32::RED);
        profile.value_area_high_color = Some(egui::Color32::GREEN);
        profile.value_area_low_color = Some(egui::Color32::BLUE);
        profile.up_volume_color = Some(egui::Color32::YELLOW);
        profile.down_volume_color = Some(egui::Color32::BROWN);
        profile.value_area_up_color = Some(egui::Color32::DARK_GREEN);
        profile.value_area_down_color = Some(egui::Color32::DARK_RED);
        profile.mode = FootprintMode::Delta;
        profile.poc = false;
        profile.extend_poc = true;
        profile.extend_vah = true;
        profile.extend_val = true;
        profile.value_area_guides = true;
        profile.value_area_high = false;
        profile.value_area_low = false;
        profile.developing_poc = true;
        profile.developing_value_area = true;
        profile.nodes = Some(NodeSensitivity::default());
        profile.right_aligned = true;
        profile.width = 0.5;
        profile.values = true;
        profile.values_color = Some(egui::Color32::LIGHT_BLUE);
        profile.session_box = false;
        profile.session_box_color = Some(egui::Color32::GRAY);
        params.tpo_block_colors = [
            egui::Color32::RED,
            egui::Color32::GREEN,
            egui::Color32::BLUE,
            egui::Color32::WHITE,
        ];
        params.tpo_block_color = egui::Color32::BLACK;
        params.tpo_poc = false;
        params.tpo_volume_profile = true;
        params.tpo_value_area_levels = false;
        params.tpo_open_price = false;
        params.tpo_close_price = true;
        params.tpo_initial_balance = false;
        params.tpo_initial_balance_minutes = 30;
        params.tpo_extend_poor = true;
        params.tpo_extend_poc = true;
        params.tpo_extend_vah = true;
        params.tpo_extend_val = true;
        params.tpo_extend_single_prints = true;
        params.tpo_initial_balance_extensions = true;
        params.tpo_single_prints = true;
        params.tpo_midpoint = true;
        params.tpo_letters = false;
        params.tpo_statistics = true;
        params.tpo_statistics_rows.total_volume = true;
        params.tpo_colored_blocks = false;
        params.tpo_outside_value_area_opacity = 80;
        let labels = TPOLabels {
            poc: false,
            initial_balance_high: true,
            poor_high: true,
            prices: true,
            ..TPOLabels::default()
        };
        params.tpo_labels = labels;
        params.tpo_level_tags = true;
        params.footprint_modes = vec![FootprintMode::BidAsk, FootprintMode::Delta];
        params.footprint_profile = true;
        params.footprint_color_by = FootprintColor::Delta;
        params.footprint_step_colors[0][3] = Some(egui::Color32::GOLD);
        params.footprint_intensity = FootprintIntensity::Volume(500.0);
        params.footprint_auto_row_height = 24.0;
        params.footprint_min_text_height = 14.0;
        params.footprint_imbalance = false;
        params.footprint_poc = false;
        params.footprint_imbalance_ratio = 4.0;
        params.footprint_diagonal = false;
        params.footprint_stacked_imbalances = true;
        params.footprint_stack = 2;
        params.footprint_zones_until_broken = true;
        params.footprint_naked_poc = true;
        params.footprint_delta_poc = true;
        params.footprint_unfinished_auctions = true;
        params.footprint_cluster_share = Some(0.2);
        params.footprint_delta_percent = Some(10.0);
        params.footprint_ratio = Some(2.0);
        params.footprint_extend_bars = Some(5);
        params.footprint_extreme_ratio = true;
        params.footprint_pullback = true;
        params.footprint_abbreviate = false;
        params.footprint_show_zero = false;
        params.footprint_font_size = [9.0, 14.0];
        params.footprint_info_box = false;
        params.footprint_statistics = true;
        params.footprint_statistics_rows = vec![FootprintStatistic::Trades];
        params.footprint_session_cumulative_delta = true;
        params.renko_wicks = true;
        params.renko_real_price = false;
        params.kagi_real_price = false;
        params.line_break_real_price = false;
        params.pnf_real_price = false;
        a.set_chart_type_params(params);
        let mut settings = ChartSettingsState::from_chart(&a);
        settings.chart_basic_styles.background_color = egui::Color32::KHAKI;
        settings.candle_colors.body_up = egui::Color32::GOLD;
        settings.back_adjust_futures = true;
        settings.settlement_as_close = false;
        let factory = IndicatorFactory::new();
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..ChartLayout::default()
        };
        let mut charts = [&mut a, &mut b];
        let before = charts[1].config().background_color;
        let params_before = charts[1].chart_type_params().clone();
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
        let params = charts[1].chart_type_params();
        let profile = &params.profile;
        assert_eq!(
            [
                profile.poc_color,
                profile.value_area_high_color,
                profile.value_area_low_color,
                profile.up_volume_color,
                profile.down_volume_color,
                profile.value_area_up_color,
                profile.value_area_down_color,
            ],
            [
                egui::Color32::RED,
                egui::Color32::GREEN,
                egui::Color32::BLUE,
                egui::Color32::YELLOW,
                egui::Color32::BROWN,
                egui::Color32::DARK_GREEN,
                egui::Color32::DARK_RED,
            ]
            .map(Some),
            "the profile colours follow the source"
        );
        assert_eq!(
            (params.tpo_block_colors, params.tpo_block_color),
            (
                [
                    egui::Color32::RED,
                    egui::Color32::GREEN,
                    egui::Color32::BLUE,
                    egui::Color32::WHITE,
                ],
                egui::Color32::BLACK,
            ),
            "the TPO block colours follow the source"
        );
        assert_eq!(
            (
                params.tpo_poc,
                params.tpo_volume_profile,
                params.tpo_initial_balance,
                params.tpo_initial_balance_minutes,
            ),
            (false, true, false, 30),
            "the TPO display switches and initial balance minutes follow the source"
        );
        assert_eq!(
            [
                params.tpo_value_area_levels,
                params.tpo_open_price,
                params.tpo_close_price,
                params.tpo_poor_high,
                params.tpo_poor_low,
                params.tpo_poor_highs_lows,
                params.tpo_extend_poor,
                params.tpo_extend_poc,
                params.tpo_extend_vah,
                params.tpo_extend_val,
                params.tpo_extend_single_prints,
                params.tpo_initial_balance_extensions,
                params.tpo_single_prints,
                params.tpo_midpoint,
                params.tpo_letters,
                params.tpo_statistics,
                params.tpo_colored_blocks,
                params.tpo_split_periods,
            ],
            [
                false, false, true, false, false, false, true, true, true, true, true, true, true,
                true, false, true, false, false,
            ],
            "the other TPO display switches follow the source, including the legacy poor flag"
        );
        assert_eq!(params.tpo_outside_value_area_opacity, 80);
        assert_eq!(
            (
                &profile.session,
                profile.rows,
                profile.value_area,
                params.tpo_row_ticks,
                params.tpo_value_area,
            ),
            (
                &ProfileSession::Regular,
                ProfileRows::Ticks(4),
                Some(0.6),
                3,
                60.0
            ),
            "the profile and TPO content settings stay the chart's own"
        );
        assert_eq!(
            (
                params.tpo_split_sessions,
                &params.tpo_session,
                params.tpo_period_minutes,
                params.tpo_profile_period,
            ),
            (
                true,
                &ProfileSession::Regular,
                15,
                crate::model::TPOProfilePeriod::Week
            ),
            "the TPO session and period grouping stay the chart's own"
        );
        let statistics_rows = crate::model::TPOStatisticsRows {
            total_volume: true,
            ..Default::default()
        };
        assert_eq!(
            (
                params.tpo_labels,
                params.tpo_level_tags,
                params.tpo_statistics_rows
            ),
            (labels, true, statistics_rows),
            "the TPO captions, price scale tags and statistics rows follow the source"
        );
        assert_eq!(
            [
                profile.poc,
                profile.extend_poc,
                profile.extend_vah,
                profile.extend_val,
                profile.value_area_guides,
                profile.value_area_high,
                profile.value_area_low,
                profile.developing_poc,
                profile.developing_value_area,
                profile.right_aligned,
                profile.values,
                profile.session_box,
            ],
            [
                false, true, true, true, true, false, false, true, true, true, true, false
            ],
            "the profile level, extension, placement and label switches follow the source"
        );
        assert_eq!(
            (
                profile.mode,
                profile.nodes,
                profile.width,
                profile.values_color,
                profile.session_box_color,
            ),
            (
                FootprintMode::Delta,
                Some(NodeSensitivity::default()),
                0.5,
                Some(egui::Color32::LIGHT_BLUE),
                Some(egui::Color32::GRAY),
            ),
            "the profile mode, node labels, width and label colours follow the source"
        );
        assert_eq!(
            [
                params.footprint_profile,
                params.footprint_imbalance,
                params.footprint_poc,
                params.footprint_diagonal,
                params.footprint_stacked_imbalances,
                params.footprint_zones_until_broken,
                params.footprint_naked_poc,
                params.footprint_delta_poc,
                params.footprint_unfinished_auctions,
                params.footprint_extreme_ratio,
                params.footprint_pullback,
                params.footprint_abbreviate,
                params.footprint_show_zero,
                params.footprint_info_box,
                params.footprint_statistics,
                params.footprint_session_cumulative_delta,
            ],
            [
                true, false, false, false, true, true, true, true, true, true, true, false, false,
                false, true, true,
            ],
            "the footprint display switches follow the source"
        );
        assert_eq!(
            (
                &params.footprint_modes,
                params.footprint_color_by,
                params.footprint_step_colors,
                params.footprint_auto_row_height,
                params.footprint_min_text_height,
                params.footprint_font_size,
                &params.footprint_statistics_rows,
            ),
            (
                &vec![FootprintMode::BidAsk, FootprintMode::Delta],
                FootprintColor::Delta,
                [[None, None, None, Some(egui::Color32::GOLD)], [None; 4]],
                24.0,
                14.0,
                [9.0, 14.0],
                &vec![FootprintStatistic::Trades],
            ),
            "the footprint columns, colouring, text sizes and statistics rows follow the source"
        );
        assert_eq!(
            (
                params.footprint_imbalance_ratio,
                params.footprint_stack,
                params.footprint_cluster_share,
                params.footprint_delta_percent,
                params.footprint_ratio,
                params.footprint_extend_bars,
            ),
            (4.0, 2, Some(0.2), Some(10.0), Some(2.0), Some(5)),
            "the footprint marker thresholds follow the source"
        );
        assert_eq!(
            [
                params.renko_wicks,
                params.renko_real_price,
                params.kagi_real_price,
                params.line_break_real_price,
                params.pnf_real_price,
            ],
            [true, false, false, false, false],
            "the Renko wicks and real price labels follow the source"
        );
        assert_eq!(
            (
                params.footprint_row_ticks,
                params.footprint_row_atr,
                params.footprint_row_atr_length,
                params.footprint_value_area,
                params.footprint_imbalance_min_volume,
                params.footprint_single_print_max,
                params.footprint_intensity,
            ),
            (
                4,
                true,
                20,
                Some(0.6),
                25.0,
                Some(5.0),
                FootprintIntensity::Visible
            ),
            "the footprint rows, value area share and volumes stay the chart's own"
        );
        assert_eq!(
            (&params.tpo_composites, &params.tpo_splits),
            (&composites, &splits),
            "the TPO composites and splits stay the chart's own"
        );
        // Neither chose a precision, so each writes its own instrument's.
        let precision = |chart: &Chart| chart.state().price_scale().price_precision();
        assert_eq!(precision(charts[0]), Some(2));
        assert_eq!(precision(charts[1]), Some(4));

        // Only the source takes the futures data choices; the other chart keeps its own.
        let futures = |chart: &Chart| {
            let settings = ChartSettingsState::from_chart(chart);
            (settings.back_adjust_futures, settings.settlement_as_close)
        };
        assert_eq!(
            (futures(charts[0]), futures(charts[1])),
            ((true, false), (false, true))
        );

        assert!(charts[0].undo());
        show(&mut layout, &mut charts);
        assert_eq!(charts[1].config().background_color, before);
        assert_eq!(charts[0].config().background_color, before);
        assert_eq!(
            charts[1].chart_type_params(),
            &params_before,
            "one undo restores every parameter"
        );

        for (source, own, expected) in [
            (
                FootprintIntensity::Bar,
                FootprintIntensity::Visible,
                FootprintIntensity::Bar,
            ),
            (
                FootprintIntensity::Visible,
                FootprintIntensity::Bar,
                FootprintIntensity::Visible,
            ),
            (
                FootprintIntensity::Volume(500.0),
                FootprintIntensity::Bar,
                FootprintIntensity::Bar,
            ),
            (
                FootprintIntensity::Volume(500.0),
                FootprintIntensity::Volume(20.0),
                FootprintIntensity::Volume(20.0),
            ),
        ] {
            for (chart, intensity) in charts.iter_mut().zip([source, own]) {
                let mut params = chart.chart_type_params().clone();
                params.footprint_intensity = intensity;
                chart.set_chart_type_params(params);
            }
            let copy = LayoutCopy::Settings(Box::new(ChartSettingsState::from_chart(charts[0])));
            assert_eq!(layout.apply_copy(&mut charts, 0, &copy, &factory), []);
            assert_eq!(
                charts[1].chart_type_params().footprint_intensity,
                expected,
                "only a volume-based intensity stays per chart: {source:?}, {own:?}"
            );
        }
    }

    /// Settings every chart already has change none of them and leave no step to undo.
    #[test]
    fn unchanged_settings_copy_leaves_no_step() {
        let (mut a, mut b) = (chart(50), chart(50));
        let mut layout = pair();
        let mut charts = [&mut a, &mut b];
        show(&mut layout, &mut charts);
        let edits = charts.each_ref().map(|chart| chart.edit_count());
        let copy = LayoutCopy::Settings(Box::new(ChartSettingsState::from_chart(charts[0])));
        assert_eq!(
            layout.apply_copy(&mut charts, 0, &copy, &IndicatorFactory::new()),
            []
        );
        assert_eq!(charts.each_ref().map(|chart| chart.edit_count()), edits);
        assert!(!charts[0].can_undo() && !charts[1].can_undo());
    }

    /// Every chart takes the scale mode of the source, whichever of the four modes either has.
    #[test]
    fn settings_copy_gives_every_chart_the_source_scale_mode() {
        use crate::scales::PriceScaleMode::*;
        let modes = [Normal, Logarithmic, Percentage, IndexedTo100];
        let mode = |chart: &Chart| chart.state().price_scale().options().mode;
        for source in 0..modes.len() {
            let mut charts = modes.map(|mode| {
                let mut chart = chart(50);
                let mut config = chart.config().clone();
                config.right_axis_scale_mode = mode;
                chart.update_config(config);
                chart
            });
            let mut layout = ChartLayout {
                mode: ChartLayoutMode::Four,
                ..ChartLayout::default()
            };
            show(&mut layout, &mut charts.each_mut());
            assert_eq!(charts.each_ref().map(mode), modes);
            let settings = ChartSettingsState::from_chart(&charts[source]);
            let copy = LayoutCopy::Settings(Box::new(settings));
            let factory = IndicatorFactory::new();
            layout.apply_copy(&mut charts.each_mut(), source, &copy, &factory);
            show(&mut layout, &mut charts.each_mut());
            assert_eq!(charts.each_ref().map(mode), [modes[source]; 4]);
        }
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

    /// A right margin kept in pixels reaches the other charts in pixels, which each keeps at its
    /// own bar spacing: a chart that already keeps the same pixels keeps its margin and its view,
    /// and one with a margin in bars takes the pixels at its newest bar.
    #[test]
    fn settings_copy_keeps_a_pixel_right_margin_at_another_zoom() {
        let mut options = crate::config::ChartOptions::default();
        options.time_scale.right_offset_pixels = Some(60.0);
        let factory = IndicatorFactory::new();
        // 60 pixels at 12 points per bar are 5 bars.
        for (pixels, kept) in [
            (Some(60.0), (2.5, Some(60.0), (12.0, -20.0))),
            (None, (2.5, Some(60.0), (12.0, 5.0))),
        ] {
            let mut a = chart(50).with_chart_options(options.clone());
            let mut own = options.clone();
            own.time_scale.right_offset_pixels = pixels;
            let mut b = chart(50).with_chart_options(own);
            b.apply_synced_time_scale(12.0, -20.0);
            let copy = LayoutCopy::Settings(Box::new(ChartSettingsState::from_chart(&a)));
            let mut charts = [&mut a, &mut b];
            assert_eq!(pair().apply_copy(&mut charts, 0, &copy, &factory), []);
            let time_scale = &charts[1].chart_options().time_scale;
            assert_eq!(
                (
                    time_scale.right_offset,
                    time_scale.right_offset_pixels,
                    charts[1].get_time_scale_state(),
                ),
                kept,
                "{pixels:?}"
            );
        }
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
                    Bar::new(
                        start + Duration::minutes(i),
                        close,
                        0.016,
                        0.009,
                        close,
                        1.0,
                    )
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

    /// Order-flow sizes, the quantity unit and a bubble price step describe the source's
    /// instrument: a chart of another symbol keeps its own, and takes the rest, such as the
    /// heatmap's brightness and a bubble grouping that holds no price.
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
        // A bubble level of 1 in ES prices; BTCUSD groups its own trades by 0.5.
        a.set_volume_bubbles(Some(VolumeBubblesConfig {
            min_size_filter: Some(10.0),
            grouping: BubbleGrouping::PriceStep(1.0),
            ..VolumeBubblesConfig::default()
        }));
        let own_step = || VolumeBubblesConfig {
            grouping: BubbleGrouping::PriceStep(0.5),
            ..VolumeBubblesConfig::default()
        };
        b.set_volume_bubbles(Some(own_step()));
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
                flow.volume_bubbles.grouping,
            ),
            (
                Some("BTC"),
                None,
                0.0,
                0.0,
                None,
                None,
                BubbleGrouping::PriceStep(0.5)
            ),
        );
        assert_eq!(flow.heatmap.brightness, 0.5);

        let no_price = [
            BubbleGrouping::Automatic,
            BubbleGrouping::Ungrouped,
            BubbleGrouping::TimeWindow(Duration::seconds(5)),
            BubbleGrouping::Executions(5),
        ];
        // A source price step preserves every destination grouping, including one without a price.
        for grouping in no_price {
            charts[1].set_volume_bubbles(Some(VolumeBubblesConfig {
                grouping,
                ..VolumeBubblesConfig::default()
            }));
            assert_eq!(pair().apply_copy(&mut charts, 0, &copy, &factory), []);
            let flow = ChartSettingsState::from_chart(charts[1]).order_flow;
            assert_eq!(flow.volume_bubbles.grouping, grouping);
        }

        // A grouping that holds no price reaches every chart.
        for grouping in no_price {
            charts[0].set_volume_bubbles(Some(VolumeBubblesConfig {
                grouping,
                ..VolumeBubblesConfig::default()
            }));
            charts[1].set_volume_bubbles(Some(own_step()));
            let copy = LayoutCopy::Settings(Box::new(ChartSettingsState::from_chart(charts[0])));
            assert_eq!(pair().apply_copy(&mut charts, 0, &copy, &factory), []);
            let flow = ChartSettingsState::from_chart(charts[1]).order_flow;
            assert_eq!(flow.volume_bubbles.grouping, grouping);
        }
    }

    /// A study that reads another study's output and needs more bars than a chart has is reported,
    /// as a study reading prices is.
    #[test]
    fn short_history_is_reported_for_a_linked_study() {
        let (mut a, mut short, mut long) = (chart(50), chart(20), chart(50));
        let rsi = a.add_indicator(Box::new(RSI::new(5)));
        let mean = a.add_indicator(Box::new(SMA::new(30)));
        a.set_indicator_source(mean, StudySource::Study { id: rsi.0, plot: 0 })
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

    /// A study copied to a Renko chart draws on its bricks, not its bars: it is reported when they
    /// are too few for it to draw.
    #[test]
    fn short_history_counts_the_bricks_of_a_renko_chart() {
        use crate::widget::test_frames::frame;
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        // Flat closes make no brick past the first. Closes rising by the default brick of 1 make
        // one brick per bar after the first, enough for an SMA of 7. A chart that has not painted
        // has built no bricks yet, so it is not reported either.
        for (rise, painted, short) in [(0.0, true, true), (1.0, true, false), (1.0, false, false)] {
            let mut renko = Chart::new(BarData::from_bars(
                (0..50)
                    .map(|i| {
                        let close = 10.0 + rise * i as f64;
                        let time = start + Duration::minutes(i);
                        Bar::new(time, close, close + 0.1, close - 0.1, close, 1.0)
                    })
                    .collect(),
            ));
            renko.set_timeframe(Timeframe::Min1);
            renko.config_mut().show_volume = false;
            renko.set_chart_type(ChartType::Renko);
            let ctx = egui::Context::default();
            if painted {
                frame(&ctx, &mut renko, 0.0, vec![]);
            }
            let mut a = chart(50);
            let sma = a.add_indicator(Box::new(SMA::new(7)));
            let factory = IndicatorFactory::new();
            let mut charts = [&mut a, &mut renko];
            let issues = pair().apply_copy(&mut charts, 0, &LayoutCopy::Study(sma), &factory);
            let expected = short.then(|| LayoutCopyIssue {
                slot: 1,
                kind: LayoutCopyIssueKind::ShortHistory("SMA".into()),
            });
            assert_eq!(
                issues,
                Vec::from_iter(expected),
                "rise {rise}, painted {painted}"
            );
            frame(&ctx, charts[1], 1.0, vec![]);
            frame(&ctx, charts[1], 2.0, vec![]);
            let shown = charts[1]
                .rendered_studies()
                .indicators()
                .iter()
                .any(|study| draws(study.as_ref()));
            assert_eq!(shown, !short, "rise {rise}, painted {painted}");
        }
    }

    /// A chart asks for the histories a study reads once it has the study, so a study waiting for
    /// them on a chart with enough bars is not short of history, nor is a study reading it.
    #[test]
    fn study_waiting_for_its_candles_is_not_short_of_history() {
        let (mut a, mut b) = (chart(50), chart(50));
        let delta = a.add_indicator(Box::new(crate::studies::VolumeDelta::new()));
        let requests = a.study_data_requests();
        assert_eq!(requests.len(), 1, "{requests:?}");
        // One rising one-minute candle in each bar.
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let candles = (0..50)
            .map(|i| Bar::new(start + Duration::minutes(i), 10.0, 16.0, 9.0, 11.0, 1.0))
            .collect();
        a.set_study_data(requests[0].clone(), candles, None);
        a.recalculate_studies();
        assert!(a.indicators().all(|(_, study)| draws(study)));
        let factory = IndicatorFactory::new();
        let charts = [&mut a, &mut b];
        let issues = pair().copy_issues(&charts, 0, &LayoutCopy::Study(delta), &factory);
        assert_eq!(issues, []);

        // A study that averages the waiting one waits with it.
        let mean = charts[0].add_indicator(Box::new(SMA::new(3)));
        charts[0]
            .set_indicator_source(
                mean,
                StudySource::Study {
                    id: delta.0,
                    plot: 0,
                },
            )
            .unwrap();
        charts[0].recalculate_studies();
        assert!(charts[0].indicators().all(|(_, study)| draws(study)));
        let issues = pair().copy_issues(&charts, 0, &LayoutCopy::Studies, &factory);
        assert_eq!(issues, []);
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
                assert!(if *redo {
                    charts[1].redo()
                } else {
                    charts[1].undo()
                });
            }
            show(&mut layout, &mut charts);
            let counts = charts.each_ref().map(|chart| chart.indicators().count());
            assert_eq!(counts, expected, "steps {steps:?}");
        }
    }

    /// Undoing a settings copy restores the precision, event markers and price line colour it
    /// changed, on both charts after replacing the layout with a clone, and redo applies them again.
    /// A chosen precision equal to the instruments' own is a change too.
    #[test]
    fn settings_copy_undo_restores_precision_events_and_price_line() {
        let (mut a, mut b) = (chart(50), chart(50));
        for chart in [&mut a, &mut b] {
            chart.set_instrument(Some(Instrument::new("SAMPLE").with_tick_size(0.01)));
        }
        let mut settings = ChartSettingsState::from_chart(&a);
        settings.precision = PrecisionMode::Decimals2;
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
        let before = (PrecisionMode::Default, false, None);
        let applied = (PrecisionMode::Decimals2, true, Some(egui::Color32::GOLD));
        assert_eq!([state(&a), state(&b)], [before, before]);
        let factory = IndicatorFactory::new();
        let mut layout = pair();
        let mut charts = [&mut a, &mut b];
        let copy = LayoutCopy::Settings(Box::new(settings));
        assert_eq!(layout.apply_copy(&mut charts, 0, &copy, &factory), []);
        assert_eq!([state(charts[0]), state(charts[1])], [applied, applied]);
        layout = layout.clone();
        assert!(charts[0].undo());
        show(&mut layout, &mut charts);
        assert_eq!([state(charts[0]), state(charts[1])], [before, before]);
        assert!(charts[0].redo());
        show(&mut layout, &mut charts);
        assert_eq!([state(charts[0]), state(charts[1])], [applied, applied]);
    }

    /// A chart without a feed holds no order flow, so settings from it leave the order-flow
    /// layers and settings of a chart on a feed as they were. A source on a feed with its layers
    /// off still switches them off on the others.
    #[test]
    fn settings_from_a_chart_without_a_feed_keep_the_order_flow_of_others() {
        use crate::chart::overlays::{LiquidityHeatmapConfig, VolumeBubblesConfig};
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
        // Heatmap, book profile and bubbles shown, at brightness 0.5 and opacity 200.
        let shown = || {
            let mut chart = build("ES");
            chart.set_liquidity_heatmap(true);
            chart.set_liquidity_heatmap_config(LiquidityHeatmapConfig {
                brightness: 0.5,
                ..LiquidityHeatmapConfig::default()
            });
            chart.set_order_book_visible(true);
            chart.set_volume_bubbles(Some(VolumeBubblesConfig {
                opacity: 200,
                ..VolumeBubblesConfig::default()
            }));
            chart
        };
        // All three layers off, at brightness 0.8 and opacity 90.
        let mut off = build("NQ");
        off.set_liquidity_heatmap_config(LiquidityHeatmapConfig {
            brightness: 0.8,
            ..LiquidityHeatmapConfig::default()
        });
        off.set_order_book_visible(false);
        off.set_volume_bubbles(Some(VolumeBubblesConfig {
            opacity: 90,
            ..VolumeBubblesConfig::default()
        }));
        off.set_volume_bubbles(None);
        let factory = IndicatorFactory::new();
        for (mut source, expected) in [
            // Host bars, no feed.
            (chart(50), (true, true, true, 0.5, 200)),
            (off, (false, false, false, 0.8, 90)),
        ] {
            let mut other = shown();
            let copy = LayoutCopy::Settings(Box::new(ChartSettingsState::from_chart(&source)));
            let mut charts = [&mut source, &mut other];
            assert_eq!(pair().apply_copy(&mut charts, 0, &copy, &factory), []);
            let flow = ChartSettingsState::from_chart(charts[1]).order_flow;
            let seen = (
                flow.show_heatmap,
                flow.show_order_book,
                flow.show_volume_bubbles,
                flow.heatmap.brightness,
                flow.volume_bubbles.opacity,
            );
            assert_eq!(seen, expected);
        }
    }

    /// A chart that has not painted holds the scale's placeholder range, not a view of its own:
    /// settings that lock the ratio or keep a manual scale let it fit its bars first. A chart that
    /// has painted keeps the range it shows.
    #[test]
    fn settings_copy_lets_an_unpainted_chart_fit_its_bars() {
        use crate::widget::test_frames::frame;
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        for (locked, painted) in [(true, false), (false, false), (false, true)] {
            let mut a = chart(50);
            let ctx_a = egui::Context::default();
            frame(&ctx_a, &mut a, 0.0, vec![]);
            if locked {
                let (spacing, height) = (
                    a.state.time_scale().bar_spacing(),
                    a.get_rendered_price_rect().height(),
                );
                a.state
                    .price_scale_mut()
                    .set_bar_ratio_locked(true, spacing, height);
            } else {
                // A scale the user dragged.
                a.state.price_scale_mut().set_range(8.0, 17.0);
            }
            // Bars from 4809 to 4816, in a tab shown or not shown yet.
            let mut b = Chart::new(BarData::from_bars(
                (0..50)
                    .map(|i| {
                        let close = 4810.0 + (i % 5) as f64;
                        let time = start + Duration::minutes(i);
                        Bar::new(time, close, 4816.0, 4809.0, close, 1.0)
                    })
                    .collect(),
            ));
            b.set_timeframe(Timeframe::Min1);
            b.config_mut().show_volume = false;
            let ctx_b = egui::Context::default();
            if painted {
                frame(&ctx_b, &mut b, 0.0, vec![]);
            }
            let shown = b.state.price_scale().range();
            let copy = LayoutCopy::Settings(Box::new(ChartSettingsState::from_chart(&a)));
            let factory = IndicatorFactory::new();
            let mut charts = [&mut a, &mut b];
            assert_eq!(pair().apply_copy(&mut charts, 0, &copy, &factory), []);
            frame(&ctx_b, charts[1], 1.0, vec![]);
            frame(&ctx_b, charts[1], 2.0, vec![]);
            let scale = charts[1].state.price_scale();
            let range = scale.range();
            if painted {
                assert!(scale.is_manual(), "painted: keeps the range it shows");
                assert_eq!((range.min, range.max), (shown.min, shown.max));
            } else {
                // A fit holds 4809 to 4816, and ten times their span is 70.
                assert!(
                    range.min <= 4809.0 && range.max >= 4816.0 && range.max - range.min < 70.0,
                    "locked {locked}: {range:?}"
                );
                assert_eq!(scale.bar_ratio_locked(), locked, "{range:?}");
            }
        }
    }

    /// A price-based source draws its studies on its elements, which can outnumber its bars or
    /// fall short of them, so the history check follows what it draws. A source that has not
    /// painted has built no elements, so none of its studies draws and nothing is checked.
    #[test]
    fn short_history_follows_what_a_price_based_source_draws() {
        use crate::widget::test_frames::frame;
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let short = LayoutCopyIssue {
            slot: 1,
            kind: LayoutCopyIssueKind::ShortHistory("SMA".into()),
        };
        // 20 bars rising by 10 with the default brick of 1 make 190 bricks, enough for an SMA of
        // 30, which 20 bars are not. 50 bars rising by 0.1 make 4 bricks, at 11, 12, 13 and 14,
        // too few for an SMA of 7.
        let mut seen = Vec::new();
        for (bars, rise, length, other_bars, painted) in [
            (20, 10.0, 30, 20, true),
            (50, 0.1, 7, 3, true),
            (20, 10.0, 30, 20, false),
        ] {
            let mut source = Chart::new(BarData::from_bars(
                (0..bars)
                    .map(|i| {
                        let close = 10.0 + rise * i as f64;
                        let time = start + Duration::minutes(i);
                        Bar::new(time, close, close + 0.05, close - 0.05, close, 1.0)
                    })
                    .collect(),
            ));
            source.set_timeframe(Timeframe::Min1);
            source.config_mut().show_volume = false;
            source.set_chart_type(ChartType::Renko);
            let sma = source.add_indicator(Box::new(SMA::new(length)));
            let ctx = egui::Context::default();
            let frames = if painted { 3 } else { 0 };
            for time in 0..frames {
                frame(&ctx, &mut source, time as f64, vec![]);
            }
            let shown = source
                .rendered_studies()
                .indicators()
                .iter()
                .any(|study| draws(study.as_ref()));
            let mut other = chart(other_bars);
            let factory = IndicatorFactory::new();
            let charts = [&mut source, &mut other];
            let issues = pair().copy_issues(&charts, 0, &LayoutCopy::Study(sma), &factory);
            seen.push((shown, issues));
        }
        let expected = [(true, vec![short]), (false, vec![]), (false, vec![])];
        assert_eq!(seen, expected);
    }
}
