//! A chart's state as a serializable snapshot, and named layouts of charts as versioned
//! documents.

use super::{Chart, ChartLayoutMode, PaneId, SeriesOptions};
use crate::drawings::persistence::{DrawingStorage, SaveError, SkippedDrawing};
use crate::model::{
    ChartType, ChartTypeParams, Marker, MarkerOptions, PriceLine, SeriesStyle, Timeframe,
};
use crate::studies::{IndicatorFactory, StudyTemplate};
use crate::widget::settings::{ChartSettingsState, WatermarkMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One study of a [`ChartSnapshot`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedStudy {
    /// The study's id when saved, used by study sources and drawings in its pane.
    pub id: usize,
    /// Its name, inputs, styles and visibility.
    pub study: StudyTemplate,
    /// The caption after its title in the legend.
    pub caption: String,
    /// Whether it widens the automatic range of the price scale.
    pub autoscale: bool,
    /// The position of its pane in [`ChartSnapshot::panes`].
    pub pane: usize,
}

/// The markers and price lines of one bar series of a [`ChartSnapshot`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SavedAnnotations {
    /// The markers from [`Chart::set_markers`], with their colours.
    pub markers: Vec<Marker>,
    /// Their fit and layer.
    pub options: MarkerOptions,
    /// The price lines in drawing order. [`Chart::restore`] hands out new `PriceLineId`s.
    pub price_lines: Vec<PriceLine>,
}

/// One bar series added with [`Chart::add_series`] to a [`ChartSnapshot`], without its bars.
/// [`Chart::restore`] adds it again with a new id: supply its bars through
/// [`Chart::set_data`] on [`Chart::restored_series_id`] of [`id`](Self::id), or set
/// [`fed`](Self::fed) to reconnect its symbol or spread to the chart feed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedSeries {
    /// The series' id when saved, which [`SavedPane::members`] and drawings in its pane use.
    pub id: usize,
    /// Its name, chart type, colours and scale assignment. A style's autoscale function is not
    /// saved.
    pub options: SeriesOptions,
    /// Whether it is shown.
    pub visible: bool,
    /// Reconnect the symbol or spread in `options.name` to the chart feed on restore.
    /// Older layouts leave series host-fed.
    #[serde(default)]
    pub fed: bool,
    /// Its price scale, including options and a manual range; absent in older layouts.
    #[serde(default)]
    pub price_scale: Option<crate::scales::PriceScale>,
    /// The position of its pane in [`ChartSnapshot::panes`].
    pub pane: usize,
    /// Its markers and price lines.
    pub annotations: SavedAnnotations,
}

/// One pane of a [`ChartSnapshot`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedPane {
    /// Whether it is the original price pane. A layout may omit that pane.
    pub main: bool,
    /// Its share of the chart height; see [`Chart::pane_stretch_factor`].
    pub stretch: f32,
    /// Explicit collapse state, or automatic collapse in a narrow chart.
    #[serde(default)]
    pub collapsed: Option<bool>,
    /// Keep the pane when its last study leaves.
    #[serde(default)]
    pub preserve: bool,
    /// Saved study and series ids in draw order, with the main series as id 0 in its assigned
    /// pane. Older layouts leave the price pane empty for the default order.
    /// Empty in version 1 layouts.
    #[serde(default)]
    pub members: Vec<usize>,
    /// The pane's price scale, including its manual range.
    #[serde(default)]
    pub scale: Option<crate::scales::PriceScale>,
    /// Additional named scales in this pane, including options and manual ranges.
    #[serde(default)]
    pub scales: Vec<(crate::model::SeriesScale, crate::scales::PriceScale)>,
    /// Axis placement and unused-axis visibility for this pane.
    #[serde(default)]
    pub axes: crate::model::PaneAxisOptions,
}

/// What a chart shows and how: everything a user sets on it, without the bars, which come from
/// the host's feed. Take one with [`Chart::snapshot`] and apply it with [`Chart::restore`].
/// The trading and alert settings of the settings dialog belong to the host and are not part
/// of it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChartSnapshot {
    /// The main series' assignment within its saved pane. `None` in layouts saved before it was
    /// recorded, which keep the restoring chart's assignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_scale: Option<crate::model::SeriesScale>,
    /// The symbol shown.
    pub symbol: String,
    /// The interval shown, `None` for a chart without one.
    pub interval: Option<Timeframe>,
    /// Whether extended hours are requested.
    pub extended_hours: bool,
    /// The chart type.
    pub chart_type: ChartType,
    /// The parameters of the price-based chart types and of the baseline.
    pub chart_type_params: ChartTypeParams,
    /// Whether the main series is drawn.
    pub main_series_visible: bool,
    /// Visibility and style of the main series' alert levels; absent in older layouts.
    #[serde(default)]
    pub main_series_alerts: crate::model::SeriesAlerts,
    /// Raw numeric display settings of the main series; absent in older layouts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_series_display_format: Option<crate::model::SeriesDisplayFormat>,
    /// The display settings of the settings dialog, the price scale's mode and margins included.
    pub settings: ChartSettingsState,
    /// Complete appearance settings; absent in version 1 layouts.
    #[serde(default)]
    pub config: Option<crate::config::ChartConfig>,
    /// The palette `config`'s colours followed, so that [`Chart::restore`] can tell the colours
    /// the user edited from those that follow the active theme; absent in older layouts.
    #[serde(default)]
    pub palette: Option<crate::config::ChartConfig>,
    /// Interaction and time axis settings; absent in version 1 layouts.
    #[serde(default)]
    pub options: Option<crate::config::ChartOptions>,
    /// The main price scale, including its manual range and transform.
    #[serde(default)]
    pub price_scale: Option<crate::scales::PriceScale>,
    /// Position of the maximized pane, or no maximized pane.
    #[serde(default)]
    pub maximized: Option<usize>,
    /// The studies in the order they were added, including the volume study.
    pub studies: Vec<SavedStudy>,
    /// The bar series added to the chart in draw order, `None` in layouts saved before version
    /// 3, which leave the chart's added series in place. When omitted, their saved main-pane
    /// order references are ignored; the host retains ownership of those series.
    #[serde(default)]
    pub series: Option<Vec<SavedSeries>>,
    /// The markers and price lines of the main series, `None` in layouts saved before version 3.
    #[serde(default)]
    pub main_annotations: Option<SavedAnnotations>,
    /// The panes from top to bottom, including preserved empty panes.
    pub panes: Vec<SavedPane>,
    /// The space between bars in points.
    pub bar_spacing: f32,
    /// The space after the last bar, in bars.
    pub right_offset: f32,
    /// The symbol's drawings as a drawings document (see [`DrawingStorage::to_json`]).
    /// Version 1 layouts may omit drawings kept in per-symbol storage.
    pub drawings: Option<serde_json::Value>,
}

/// Why [`Chart::restore`] did not show a snapshot.
#[derive(Debug)]
#[non_exhaustive]
pub enum RestoreError {
    /// A saved study cannot be rebuilt, or reads a study or plot the snapshot does not have.
    Study {
        /// The study's position in [`ChartSnapshot::studies`].
        index: usize,
        /// Why.
        error: crate::studies::StudyError,
    },
    /// The saved chart settings are invalid.
    Settings(super::settings::ChartSettingsError),
    /// The saved panes, scales, series or view are out of range; the text says which.
    Layout(&'static str),
    /// The saved drawings cannot be read.
    Drawings(crate::drawings::persistence::LoadError),
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Study { index, error } => write!(f, "saved study {index}: {error}"),
            Self::Settings(error) => write!(f, "{error}"),
            Self::Layout(reason) => f.write_str(reason),
            Self::Drawings(error) => write!(f, "saved drawings: {error}"),
        }
    }
}
impl std::error::Error for RestoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Study { error, .. } => Some(error),
            Self::Settings(error) => Some(error),
            Self::Drawings(error) => Some(error),
            Self::Layout(_) => None,
        }
    }
}

impl Chart {
    /// Captures the chart's state. Fails while stored drawings are loading, for an unregistered
    /// study, when a drawing cannot be saved (see [`save_drawings`](Self::save_drawings)) or
    /// when [`restore`](Self::restore) would refuse the result.
    pub fn snapshot(&self) -> Result<ChartSnapshot, SaveError> {
        self.snapshot_with(&IndicatorFactory::new())
    }

    /// Captures the chart's state like [`snapshot`](Self::snapshot), checking through `factory`
    /// that each study can be built again, so that custom studies it registers are accepted.
    pub fn snapshot_with(&self, factory: &IndicatorFactory) -> Result<ChartSnapshot, SaveError> {
        if self
            .drawing_store
            .as_ref()
            .is_some_and(|store| !store.ready(&self.symbol))
        {
            return Err(SaveError::Json(serde::ser::Error::custom(
                "Drawings are still loading",
            )));
        }
        let drawings =
            Some(serde_json::to_value(self.save_drawings()?).expect("drawings were validated"));
        let panes = self.panes();
        let pane_of = |id| {
            self.series_pane(id)
                .and_then(|pane| panes.iter().position(|p| *p == pane))
                .unwrap_or(0)
        };
        let studies: Vec<_> = self
            .indicators()
            .enumerate()
            .map(|(index, (id, study))| SavedStudy {
                id: id.0,
                study: StudyTemplate::from_study(study),
                caption: self.indicator_inputs(id).unwrap_or_default().to_owned(),
                autoscale: self.study_autoscale[index],
                pane: pane_of(id),
            })
            .collect();
        let series = self
            .price_series
            .iter()
            .map(|item| SavedSeries {
                id: item.id.0,
                options: {
                    let mut options = item.options.clone();
                    options.style = saved_style(options.style);
                    options
                },
                visible: item.visible,
                fed: self.is_fed(item.id),
                price_scale: self.series_price_scale(item.id).cloned(),
                pane: pane_of(item.id),
                annotations: SavedAnnotations {
                    markers: item.markers.to_vec(),
                    options: item.marker_options,
                    price_lines: item.lines.iter().map(|(_, line)| line.clone()).collect(),
                },
            })
            .collect();
        let (bar_spacing, right_offset) = self.get_time_scale_state();
        let snapshot = ChartSnapshot {
            main_scale: Some(self.main_series_scale),
            symbol: self.symbol.clone(),
            interval: self.timeframe,
            extended_hours: self.extended_hours,
            chart_type: self.chart_type,
            chart_type_params: self.chart_type_params.clone(),
            main_series_visible: self.main_series_visible,
            main_series_alerts: self.main_series_style.alerts,
            main_series_display_format: saved_style(self.main_series_style.clone()).display_format,
            settings: {
                let mut settings = ChartSettingsState::from_chart(self);
                settings.adjust_for_dividends = self.dividend_adjusted();
                settings
            },
            config: Some(self.config.clone()),
            palette: Some(self.followed_palette()),
            options: Some(self.chart_options.clone()),
            price_scale: Some(self.state.price_scale().clone()),
            maximized: self
                .pane_layout
                .maximized
                .and_then(|id| panes.iter().position(|pane| *pane == id)),
            studies,
            series: Some(series),
            main_annotations: Some(SavedAnnotations {
                markers: self.markers(super::SeriesId::MAIN).to_vec(),
                options: self.marker_options,
                price_lines: self
                    .price_lines
                    .iter()
                    .map(|(_, line)| line.clone())
                    .collect(),
            }),
            panes: panes
                .iter()
                .map(|id| {
                    let pane = self
                        .pane_layout
                        .panes
                        .iter()
                        .find(|pane| pane.id == *id)
                        .expect("listed pane");
                    SavedPane {
                        main: *id == PaneId::MAIN,
                        stretch: pane.stretch,
                        collapsed: pane.collapsed,
                        preserve: pane.preserve,
                        members: if *id == PaneId::MAIN {
                            self.main_pane_order(self.pane_layout.main.clone())
                                .iter()
                                .map(|id| id.0)
                                .collect()
                        } else {
                            self.pane_series(*id).iter().map(|id| id.0).collect()
                        },
                        scale: (*id != PaneId::MAIN || *id != self.main_series_pane())
                            .then(|| pane.scale.clone()),
                        scales: if *id == PaneId::MAIN {
                            let mut scales: Vec<(
                                crate::model::SeriesScale,
                                crate::scales::PriceScale,
                            )> = Vec::new();
                            for member in self.pane_series(*id) {
                                if let Some(super::PaneScale::Named(key)) =
                                    self.source_axis_key(*id, member)
                                    && key != self.main_series_scale
                                    && !scales.iter().any(|(other, _)| *other == key)
                                {
                                    // A study can change assignment before a frame creates the scale.
                                    let scale =
                                        self.series_scales.get(&key).cloned().unwrap_or_else(
                                            || crate::scales::PriceScale::new(Default::default()),
                                        );
                                    scales.push((key, scale));
                                }
                            }
                            scales
                        } else {
                            pane.scales.clone()
                        },
                        axes: pane.axes,
                    }
                })
                .collect(),
            bar_spacing,
            right_offset,
            drawings,
        };
        // A layout that restore would refuse is an error now rather than at the next load.
        check(&snapshot, factory, self.main_series_scale)
            .map_err(|error| SaveError::Json(serde::ser::Error::custom(error)))?;
        Ok(snapshot)
    }

    /// Shows what `snapshot` captured: its symbol, interval, chart type, settings, studies,
    /// added series, panes and view, and its drawings. Studies are built through
    /// [`StudyTemplate::create`]. Clears undo history and returns the drawings that could not
    /// be read. Fails without changing the chart for an unknown study, a study source that
    /// names no study, no plot of it or itself, a series or study id used twice, invalid
    /// settings or options, a number that is not finite or an unreadable drawings document.
    ///
    /// The added series of the chart are replaced by those of the snapshot, which come back
    /// without bars and under new ids: look each up with
    /// [`restored_series_id`](Self::restored_series_id) and supply its bars with
    /// [`set_data`](Self::set_data). Feed-backed series reconnect automatically.
    ///
    /// A snapshot of another symbol than the one the chart shows drops the chart's bars before
    /// its studies are computed; a chart with no symbol keeps the bars it holds. Supply the new
    /// bars with [`set_data`](Self::set_data) or let the feed load them. In a layout saved
    /// without the main series and the original pane the series stays out of the panes and is
    /// not drawn, and its bars come the same way, for the studies and the time axis.
    ///
    /// Errors distinguish a saved [`RestoreError::Study`], invalid [`RestoreError::Settings`],
    /// [`RestoreError::Layout`] geometry and unreadable [`RestoreError::Drawings`].
    pub fn restore(
        &mut self,
        snapshot: &ChartSnapshot,
    ) -> Result<Vec<SkippedDrawing>, RestoreError> {
        self.restore_with(snapshot, &IndicatorFactory::new())
    }

    /// Shows what `snapshot` captured like [`restore`](Self::restore), building the studies
    /// through `factory`, so that studies it registers are restored with their inputs, styles
    /// and source links.
    pub fn restore_with(
        &mut self,
        snapshot: &ChartSnapshot,
        factory: &IndicatorFactory,
    ) -> Result<Vec<SkippedDrawing>, RestoreError> {
        let (studies, drawings) = check(snapshot, factory, self.main_series_scale)?;
        self.layout_identity = crate::model::next_revision();
        let main_present = snapshot.panes.iter().any(|pane| pane.main);
        let main_source_present =
            main_present || snapshot.panes.iter().any(|pane| pane.members.contains(&0));
        if !main_source_present {
            self.remove_series(super::SeriesId::MAIN);
        } else if !self.main_series_present {
            self.main_series_present = true;
            self.follow_feed_pause();
        }
        let other_symbol = !self.symbol.is_empty() && snapshot.symbol != self.symbol;
        self.set_symbol(snapshot.symbol.clone());
        // The studies below must not read the previous symbol's bars. With a feed its reload
        // would clear them only on the next frame, and without one nothing would.
        if other_symbol {
            self.replace_main_data(Vec::new());
        }
        if let Some(interval) = snapshot.interval {
            self.set_timeframe(interval);
        } else {
            self.timeframe = None;
            self.drawings.curr_timeframe.clear();
        }
        self.set_extended_hours(snapshot.extended_hours);
        self.set_dividend_adjusted(snapshot.settings.adjust_for_dividends);
        self.set_chart_type(snapshot.chart_type);
        self.set_chart_type_params(snapshot.chart_type_params.clone());
        self.set_main_series_visible(snapshot.main_series_visible);
        self.main_series_style.alerts = snapshot.main_series_alerts;
        self.main_series_style.display_format = snapshot.main_series_display_format;
        let mut settings = snapshot.settings.clone();
        if settings.chart_type == crate::model::ChartType::HollowCandles {
            // Settings saved before hollow candles had a style hold the switches of the candles.
            let style = &snapshot.chart_type_params.styles.hollow;
            settings.candle_colors.show_body = style.body_visible;
            settings.candle_colors.show_border = style.border_visible;
            settings.candle_colors.show_wick = style.wick_visible;
        }
        settings
            .apply_to_chart(self)
            .map_err(RestoreError::Settings)?;
        if snapshot.chart_type == crate::model::ChartType::VolumeCandles
            && settings.chart_type_params.is_none()
            && snapshot
                .chart_type_params
                .styles
                .volume_candles
                .is_saved_without_style()
        {
            // Settings saved before they named a chart type apply as candle settings, of which
            // volume candles drew only the body colours.
            let colors = &settings.candle_colors;
            let mut params = self.chart_type_params.clone();
            params.styles.volume_candles = crate::model::VolumeCandlesStyle::drawn_before_style(
                colors.body_up,
                colors.body_down,
            );
            self.set_chart_type_params(params);
        }
        if let Some(config) = &snapshot.config {
            self.config = config.clone();
            // Layouts saved before the description followed the symbol held it as fixed text.
            if snapshot.settings.watermark.mode == WatermarkMode::SymbolAndDescription {
                self.config.watermark_text = None;
                self.config.watermark_description = true;
            }
            self.follow_restored_palette(snapshot.palette.as_ref());
        }
        if let Some(options) = &snapshot.options {
            self.set_chart_options(options.clone());
        }
        if let Some(main_scale) = snapshot.main_scale {
            self.main_series_scale = main_scale;
            self.series_scales.remove(&main_scale);
        }
        if let Some(scale) = &snapshot.price_scale {
            *self.state.price_scale_mut() = scale.clone();
        }
        if self.panes().contains(&PaneId::MAIN) {
            self.move_series_to_pane(super::SeriesId::MAIN, PaneId::MAIN);
        } else {
            self.leave_panes(super::SeriesId::MAIN);
        }
        let mut replaced: Vec<_> = self.indicators().map(|(id, _)| id).collect();
        if snapshot.series.is_some() {
            replaced.extend(self.series().map(|(id, _)| id));
        }
        let show_volume = self.config.show_volume;
        for id in replaced {
            self.remove_series(id);
        }
        self.config.show_volume = show_volume;
        self.restored_studies.clear();
        self.pane_layout
            .panes
            .retain(|pane| (main_present && pane.id == PaneId::MAIN) || !pane.members.is_empty());
        if main_present
            && !self
                .pane_layout
                .panes
                .iter()
                .any(|pane| pane.id == PaneId::MAIN)
        {
            self.pane_layout
                .panes
                .push(super::pane::Pane::new(PaneId::MAIN));
        }
        let mut panes: HashMap<usize, PaneId> = HashMap::new();
        for (at, pane) in snapshot.panes.iter().enumerate() {
            panes.insert(
                at,
                if pane.main {
                    PaneId::MAIN
                } else {
                    let id = self.add_pane();
                    self.pane_layout
                        .panes
                        .last_mut()
                        .expect("pane was added")
                        .preserve = true;
                    id
                },
            );
        }
        // Restore ownership before added-series ranges can write to the main scale.
        if let Some(at) = snapshot
            .panes
            .iter()
            .position(|pane| pane.members.contains(&0))
        {
            self.move_series_to_pane(super::SeriesId::MAIN, panes[&at]);
        }
        for (saved, study) in snapshot.studies.iter().zip(studies) {
            let id = self.add_indicator(study);
            self.set_indicator_inputs(id, saved.caption.clone());
            self.set_indicator_autoscale(id, saved.autoscale);
            self.restored_studies.insert(saved.id, id.0);
            self.move_series_to_pane(id, panes[&saved.pane]);
        }
        for saved in snapshot.series.iter().flatten() {
            let id = if saved.fed {
                self.add_feed_series(saved.options.clone())
            } else {
                self.add_series(saved.options.clone())
            };
            self.set_series_visible(id, saved.visible);
            self.restore_annotations(id, &saved.annotations);
            self.restored_studies.insert(saved.id, id.0);
            self.move_series_to_pane(id, panes[&saved.pane]);
            if let Some(scale) = &saved.price_scale {
                *self.series_price_scale_mut(id).expect("restored series") = scale.clone();
            }
        }
        if let Some(main) = &snapshot.main_annotations {
            self.restore_annotations(super::SeriesId::MAIN, main);
        }
        for saved in &snapshot.studies {
            let id = super::SeriesId(self.restored_studies[&saved.id]);
            for (input, value) in &saved.study.inputs {
                if let crate::studies::InputValue::Source(crate::studies::StudySource::Study {
                    id: parent,
                    plot,
                }) = value
                {
                    let source = crate::studies::StudySource::Study {
                        id: self.restored_studies[parent],
                        plot: *plot,
                    };
                    self.indicator_mut(id)
                        .expect("restored study")
                        .set_input(input, crate::studies::InputValue::Source(source))
                        .expect("validated study source");
                }
            }
        }
        self.recalculate_studies();
        for (at, saved) in snapshot.panes.iter().enumerate() {
            let id = panes[&at];
            self.move_pane(id, at);
            self.set_pane_stretch_factor(id, saved.stretch);
            let pane = self
                .pane_layout
                .panes
                .iter_mut()
                .find(|pane| pane.id == id)
                .expect("restored pane");
            pane.collapsed = saved.collapsed;
            pane.axes = saved.axes;
            pane.preserve = saved.preserve;
            if !saved.main && !saved.members.is_empty() {
                pane.members.sort_by_key(|id| {
                    saved
                        .members
                        .iter()
                        .position(|old| {
                            (*old == 0 && *id == super::SeriesId::MAIN)
                                || self.restored_studies.get(old) == Some(&id.0)
                        })
                        .unwrap_or(usize::MAX)
                });
            }
            if let Some(scale) = &saved.scale {
                pane.scale = scale.clone();
            }
            for (key, scale) in &saved.scales {
                if saved.main {
                    self.series_scales.insert(*key, scale.clone());
                } else {
                    *pane.assigned_scale_mut(*key) = scale.clone();
                }
            }
            if saved.main && !saved.members.is_empty() {
                self.pane_layout.main = saved
                    .members
                    .iter()
                    .filter_map(|id| {
                        if *id == super::SeriesId::MAIN.0 {
                            Some(super::SeriesId::MAIN)
                        } else {
                            self.restored_studies.get(id).copied().map(super::SeriesId)
                        }
                    })
                    .collect();
            }
        }
        self.pane_layout.maximized = snapshot.maximized.map(|at| panes[&at]);
        self.apply_synced_time_scale(snapshot.bar_spacing, snapshot.right_offset);
        let skipped = match drawings {
            Some(json) => {
                let skipped = self
                    .load_drawings_remapped(&json, true)
                    .map_err(RestoreError::Drawings)?;
                self.history.drawing_edits += 1;
                self.save_restored_drawings();
                skipped
            }
            None => Vec::new(),
        };
        // A layout without the original pane has just left it: the main bars are live again.
        self.follow_feed_pause();
        self.history.clear();
        Ok(skipped)
    }
}

impl Chart {
    fn restore_annotations(&mut self, series: super::SeriesId, saved: &SavedAnnotations) {
        self.set_markers(series, saved.markers.clone());
        self.set_marker_options(series, saved.options);
        self.set_price_lines(series, saved.price_lines.clone());
    }

    /// The id a series or study of a restored snapshot has now, given its [`SavedSeries::id`] or
    /// [`SavedStudy::id`], or `None` for an id the last [`restore`](Self::restore) did not add.
    pub fn restored_series_id(&self, saved: usize) -> Option<super::SeriesId> {
        self.restored_studies
            .get(&saved)
            .copied()
            .map(super::SeriesId)
    }
}

/// Finite equivalents of ignored values, without changing the live series' style.
fn saved_style(mut style: SeriesStyle) -> SeriesStyle {
    if let Some(format) = &mut style.display_format
        && !format.min_move.is_finite()
    {
        format.min_move = 0.0;
    }
    if !style.base_line_width.is_finite() {
        style.base_line_width = 0.0;
    }
    style
}

/// The studies of a checked snapshot, built in order, and its drawings document.
type Checked = (Vec<Box<dyn crate::studies::Indicator>>, Option<String>);

/// Checks `snapshot` as [`Chart::restore_with`] does before changing the chart.
fn check(
    snapshot: &ChartSnapshot,
    factory: &IndicatorFactory,
    main_scale: crate::model::SeriesScale,
) -> Result<Checked, RestoreError> {
    let main_scale = snapshot.main_scale.unwrap_or(main_scale);
    let studies = snapshot
        .studies
        .iter()
        .enumerate()
        .map(|(index, saved)| {
            saved
                .study
                .create_with(factory)
                .map_err(|error| RestoreError::Study { index, error })
        })
        .collect::<Result<Vec<_>, _>>()?;
    check_source_links(&snapshot.studies, &studies)?;
    let main_pane = snapshot
        .panes
        .iter()
        .position(|pane| pane.members.contains(&0))
        .or_else(|| snapshot.panes.iter().position(|pane| pane.main));
    if snapshot
        .panes
        .iter()
        .flat_map(|pane| &pane.members)
        .filter(|id| **id == 0)
        .count()
        > 1
    {
        return Err(RestoreError::Layout(
            "Main series belongs to multiple panes",
        ));
    }
    for (at, pane) in snapshot.panes.iter().enumerate() {
        if pane.members.is_empty() {
            continue;
        }
        let expected: std::collections::HashSet<_> = std::iter::once(super::SeriesId::MAIN.0)
            .filter(|_| Some(at) == main_pane)
            .chain(
                snapshot
                    .studies
                    .iter()
                    .filter(|study| study.pane == at)
                    .map(|study| study.id),
            )
            .chain(
                snapshot
                    .series
                    .iter()
                    .flatten()
                    .filter(|series| series.pane == at)
                    .map(|series| series.id),
            )
            .collect();
        let actual: std::collections::HashSet<_> = pane.members.iter().copied().collect();
        if !expected.is_subset(&actual)
            || (snapshot.series.is_some() && actual != expected)
            || actual.len() != pane.members.len()
            || snapshot
                .studies
                .iter()
                .any(|study| study.pane != at && actual.contains(&study.id))
        {
            return Err(RestoreError::Layout("Invalid price pane draw order"));
        }
    }
    snapshot
        .settings
        .validate()
        .map_err(RestoreError::Settings)?;
    let valid_scale = |scale: &crate::scales::PriceScale| {
        let range = scale.range();
        let margins = scale.options().scale_margins;
        range.min.is_finite()
            && range.max.is_finite()
            && range.min < range.max
            && margins.top.is_finite()
            && margins.bottom.is_finite()
            && margins.top >= 0.0
            && margins.bottom >= 0.0
            && margins.top + margins.bottom < 1.0
            && scale.options().tick_marks.max_marks
                <= crate::scales::PriceMarkGeneratorConfig::MAX_MARKS
    };
    if snapshot
        .price_scale
        .as_ref()
        .is_some_and(|scale| !valid_scale(scale))
        || snapshot.panes.is_empty()
        || snapshot.panes.iter().filter(|pane| pane.main).count() > 1
        || (main_pane.is_none() && snapshot.main_series_visible)
        || snapshot.studies.iter().map(|study| study.id)
            .chain(snapshot.series.iter().flatten().map(|series| series.id))
            .any(|id| id == super::SeriesId::MAIN.0)
        || snapshot
            .studies
            .iter()
            .map(|study| study.id)
            .chain(snapshot.series.iter().flatten().map(|series| series.id))
            .collect::<std::collections::HashSet<_>>()
            .len()
            != snapshot.studies.len() + snapshot.series.iter().flatten().count()
        || snapshot
            .series
            .iter()
            .flatten()
            .any(|series| series.pane >= snapshot.panes.len())
        || snapshot
            .panes
            .iter()
            .flat_map(|pane| pane.scale.iter().chain(pane.scales.iter().map(|(_, scale)| scale)))
            .chain(snapshot.series.iter().flatten().filter_map(|series| series.price_scale.as_ref()))
            .any(|scale| !valid_scale(scale))
        || snapshot.panes.iter().any(|pane| {
            let keys: std::collections::HashSet<_> =
                pane.scales.iter().map(|(key, _)| *key).collect();
            (pane.main && keys.contains(&main_scale))
                || (!pane.main && keys.contains(&crate::model::SeriesScale::Right))
                || keys.len() != pane.scales.len()
        })
        || snapshot
            .studies
            .iter()
            .any(|study| study.pane >= snapshot.panes.len())
        || snapshot
            .panes
            .iter()
            .any(|pane| !pane.stretch.is_finite() || pane.stretch <= 0.0)
        // Pane heights divide by the sum of the stretch factors.
        || !snapshot
            .panes
            .iter()
            .map(|pane| pane.stretch)
            .sum::<f32>()
            .is_finite()
        || !snapshot.bar_spacing.is_finite()
        || snapshot.bar_spacing <= 0.0
        || !snapshot.right_offset.is_finite()
        || snapshot
            .maximized
            .is_some_and(|pane| pane >= snapshot.panes.len())
    {
        return Err(RestoreError::Layout("Invalid layout geometry"));
    }
    // JSON has no NaN or infinity: serde writes them as null, which a number field then
    // refuses, so the next save of such a value could not be loaded.
    if serde_json::to_value(snapshot)
        .and_then(serde_json::from_value::<ChartSnapshot>)
        .is_err()
    {
        return Err(RestoreError::Layout("Layout values must be finite"));
    }
    if let Some(options) = &snapshot.options
        && !(options.crosshair.vert_line_width > 0.0 && options.crosshair.horz_line_width > 0.0)
    {
        return Err(RestoreError::Layout("Crosshair width must be above zero"));
    }
    // A frame below this spacing shows a hundred bars per point; 0 sets no limit.
    if let Some(options) = &snapshot.options
        && [
            options.time_scale.min_bar_spacing,
            options.time_scale.max_bar_spacing,
        ]
        .into_iter()
        .any(|spacing| spacing != 0.0 && !(0.01..).contains(&spacing))
    {
        return Err(RestoreError::Layout(
            "Bar spacing limits must be 0 or at least 0.01",
        ));
    }
    let font_sizes = &crate::styles::typography::FONT_SIZES;
    if snapshot
        .options
        .iter()
        .map(|options| options.tooltip.font_size)
        .chain(snapshot.config.iter().flat_map(|config| {
            std::iter::once(config.watermark_font_size).chain(config.axis_font_size)
        }))
        .chain(
            std::iter::once(&snapshot.chart_type_params)
                .chain(snapshot.settings.chart_type_params.as_ref())
                .map(|params| params.styles.high_low.label_font_size)
                // Non-positive sizes hide the labels.
                .filter(|size| *size > 0.0),
        )
        .any(|size| !font_sizes.contains(&size))
    {
        return Err(RestoreError::Layout("Font sizes must be between 1 and 256"));
    }
    let drawings = match &snapshot.drawings {
        Some(drawings) => {
            let json = drawings.to_string();
            let (storage, _) = DrawingStorage::from_json(&json).map_err(RestoreError::Drawings)?;
            if storage.symbol != snapshot.symbol {
                return Err(RestoreError::Drawings(
                    crate::drawings::persistence::LoadError::OtherSymbol,
                ));
            }
            Some(json)
        }
        _ => None,
    };
    Ok((studies, drawings))
}

/// Checks the study outputs the saved `studies` read: each names a saved study and a plot it
/// has, and no study reads its own output, directly or through others. `built` holds the
/// studies reconstructed from `studies`, in the same order.
fn check_source_links(
    studies: &[SavedStudy],
    built: &[Box<dyn crate::studies::Indicator>],
) -> Result<(), RestoreError> {
    let mut parents = vec![Vec::new(); studies.len()];
    for (at, saved) in studies.iter().enumerate() {
        for (_, value) in &saved.study.inputs {
            if let crate::studies::InputValue::Source(crate::studies::StudySource::Study {
                id,
                plot,
            }) = value
            {
                let parent = studies.iter().position(|other| other.id == *id).ok_or(
                    RestoreError::Study {
                        index: at,
                        error: crate::studies::StudyError::UnknownSourceStudy(*id),
                    },
                )?;
                if *plot >= built[parent].line_count() {
                    return Err(RestoreError::Study {
                        index: at,
                        error: crate::studies::StudyError::UnknownSourcePlot {
                            id: *id,
                            plot: *plot,
                        },
                    });
                }
                parents[at].push(parent);
            }
        }
    }
    // Studies that read only resolved studies resolve in turn; any left over form a cycle.
    let mut resolved = vec![false; studies.len()];
    while let Some(at) = (0..studies.len())
        .find(|at| !resolved[*at] && parents[*at].iter().all(|parent| resolved[*parent]))
    {
        resolved[at] = true;
    }
    if let Some(index) = resolved.iter().position(|done| !done) {
        return Err(RestoreError::Study {
            index,
            error: crate::studies::StudyError::CyclicSource,
        });
    }
    Ok(())
}

/// A named arrangement of charts: the [`ChartLayoutMode`] and a [`ChartSnapshot`] of each chart,
/// as kept in [`StoreKind::Layouts`](crate::services::StoreKind::Layouts) documents.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedLayout {
    /// How the charts are arranged.
    pub mode: ChartLayoutMode,
    /// The charts in the order of their slots.
    pub charts: Vec<ChartSnapshot>,
}

#[derive(Serialize, Deserialize)]
struct Document<T> {
    version: u32,
    #[serde(flatten)]
    layout: T,
}

impl SavedLayout {
    /// The version of the document [`to_json`](Self::to_json) writes.
    pub const VERSION: u32 = 3;

    /// The layout as a versioned JSON document.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&Document {
            version: Self::VERSION,
            layout: self,
        })
    }

    /// Reads a document [`to_json`](Self::to_json) wrote, migrating older versions. Fails on
    /// invalid JSON and on a version this release does not know.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let document: Document<serde_json::Value> = serde_json::from_str(json)?;
        match document.version {
            1..=Self::VERSION => serde_json::from_value(document.layout),
            version => Err(serde::de::Error::custom(format!(
                "unsupported layout version {version}, expected {}",
                Self::VERSION
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::model::{Bar, BarData, Marker, PriceLine};
    use crate::studies::{MACD, RSI, SMA};
    use chrono::{TimeZone, Utc};

    /// A custom study, the study reading its output and an added series with its markers and
    /// price lines come back through the public API; a document that cannot be rebuilt leaves
    /// the current chart as it was.
    #[test]
    fn restore_rebuilds_custom_studies_source_links_and_added_series() {
        use crate::studies::{CustomIndicator, IndicatorFactory, IndicatorValue, StudySource};
        let bars: Vec<_> = (0..30)
            .map(|i| {
                Bar::new(
                    Utc.timestamp_opt(i * 60, 0).unwrap(),
                    10.0,
                    11.0,
                    9.0,
                    10.0,
                    1.0,
                )
            })
            .collect();
        let mut factory = IndicatorFactory::new();
        factory.register("Bar range", || {
            Box::new(CustomIndicator::new("Bar range", |bars| {
                bars.iter()
                    .map(|bar| IndicatorValue::Single(bar.high - bar.low))
                    .collect()
            }))
        });
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        chart.set_symbol("A");
        let spread = chart.add_indicator(factory.create("Bar range").unwrap());
        let rsi = chart.add_indicator(Box::new(RSI::new(5)));
        chart
            .set_indicator_source(
                rsi,
                StudySource::Study {
                    id: spread.0,
                    plot: 0,
                },
            )
            .unwrap();
        let extra = chart.add_series(SeriesOptions {
            name: "Peer".into(),
            color: egui::Color32::GOLD,
            ..Default::default()
        });
        chart.set_markers(
            extra,
            vec![Marker::new(Utc.timestamp_opt(60, 0).unwrap()).with_color(egui::Color32::RED)],
        );
        chart.set_price_lines(extra, vec![PriceLine::new(12.5, egui::Color32::BLUE)]);
        chart.move_series_to_pane(extra, chart.series_pane(rsi).unwrap());
        chart.reorder_indicator(spread, super::super::VisualOrder::SendToBack);
        assert!(
            chart.snapshot().is_err(),
            "the built-in factory cannot rebuild Bar range"
        );
        let json = SavedLayout {
            mode: ChartLayoutMode::Single,
            charts: vec![chart.snapshot_with(&factory).unwrap()],
        }
        .to_json()
        .unwrap();
        let snapshot = SavedLayout::from_json(&json).unwrap().charts.remove(0);

        let mut restored = Chart::new(BarData::from_bars(bars));
        restored.set_symbol("B");
        assert!(restored.restore(&snapshot).is_err());
        assert_eq!(restored.symbol(), "B");
        let mut dangling = snapshot.clone();
        dangling.studies[1]
            .study
            .inputs
            .iter_mut()
            .for_each(|(_, value)| {
                if let crate::studies::InputValue::Source(StudySource::Study { id, .. }) = value {
                    *id = 999;
                }
            });
        assert!(restored.restore_with(&dangling, &factory).is_err());
        assert_eq!(restored.symbol(), "B");
        for members in [vec![0, 0], vec![0, 999], vec![0], vec![spread.0]] {
            let mut invalid = snapshot.clone();
            invalid
                .panes
                .iter_mut()
                .find(|pane| pane.main)
                .unwrap()
                .members = members;
            assert!(restored.restore_with(&invalid, &factory).is_err());
            assert_eq!(restored.symbol(), "B");
        }

        restored.restore_with(&snapshot, &factory).unwrap();
        assert_eq!(restored.symbol(), "A");
        let names: Vec<_> = restored
            .indicators()
            .map(|(_, study)| study.name().to_owned())
            .collect();
        assert_eq!(names, ["Bar range", "RSI"]);
        let new_spread = restored.restored_series_id(spread.0).unwrap();
        assert_eq!(
            restored.pane_layout.main,
            [new_spread, super::super::SeriesId::MAIN]
        );
        let new_rsi = restored.restored_series_id(rsi.0).unwrap();
        assert_eq!(
            restored.indicator_mut(new_rsi).unwrap().source(),
            Some(StudySource::Study {
                id: new_spread.0,
                plot: 0
            })
        );
        let new_extra = restored.restored_series_id(extra.0).unwrap();
        let (_, options) = restored.series().next().unwrap();
        assert_eq!(
            (options.name.as_str(), options.color),
            ("Peer", egui::Color32::GOLD)
        );
        assert_eq!(restored.markers(new_extra)[0].color, egui::Color32::RED);
        assert_eq!(
            restored.series_pane(new_extra),
            restored.series_pane(new_rsi)
        );
        let lines = &restored.price_series[0].lines;
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].1.price, 12.5);

        let mut legacy = snapshot.clone();
        legacy
            .panes
            .iter_mut()
            .find(|pane| pane.main)
            .unwrap()
            .members
            .clear();
        restored.restore_with(&legacy, &factory).unwrap();
        let new_spread = restored.restored_series_id(spread.0).unwrap();
        assert_eq!(
            restored.pane_layout.main,
            [super::super::SeriesId::MAIN, new_spread]
        );
    }

    #[test]
    fn layouts_restore_interval_type_studies_panes_view_and_pane_drawings() {
        let bars: Vec<_> = (0..60)
            .map(|i| {
                Bar::new(
                    Utc.timestamp_opt(i * 60, 0).unwrap(),
                    10.0,
                    11.0,
                    9.0,
                    10.0,
                    1.0,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        chart.set_symbol("A");
        chart.set_timeframe(Timeframe::Min5);
        chart.set_chart_type(ChartType::Line);
        let sma = chart.add_indicator(Box::new(SMA::new(7)));
        chart.set_indicator_inputs(sma, "7 close");
        chart.set_indicator_autoscale(sma, false);
        let rsi = chart.add_indicator(Box::new(RSI::new(9)));
        let macd = chart.add_indicator(Box::new(MACD::new(5, 13, 4)));
        chart
            .set_indicator_source(
                rsi,
                crate::studies::StudySource::Study { id: sma.0, plot: 0 },
            )
            .unwrap();
        chart.recalculate_studies();
        chart.move_indicator_to_pane(macd, rsi);
        let pane = chart.series_pane(rsi).unwrap();
        chart.move_pane(pane, 0);
        chart.set_pane_stretch_factor(pane, 2.0);
        let mut options = chart.chart_options().clone();
        options.time_scale.min_bar_spacing = 0.1;
        options.time_scale.max_bar_spacing = 12.0;
        options.time_scale.right_offset = 7.0;
        options.tooltip.show_volume_details = true;
        chart.set_chart_options(options);
        chart.apply_synced_time_scale(0.25, 4.0);
        let empty = chart.add_pane();
        chart.set_preserve_empty_pane(empty, true);
        chart.pane_layout.panes[0].collapsed = Some(true);
        chart.pane_layout.panes[0].scale.set_range(25.0, 75.0);
        chart.pane_layout.maximized = Some(pane);
        chart.price_scale_mut().set_range(8.0, 12.0);
        chart.config_mut().candle_width = 0.42;
        chart.config_mut().watermark_text = Some("Saved chart".into());
        chart.config_mut().grid_line_style = crate::model::LineStyle::LargeDashed;
        chart.config_mut().vertical_grid_line_style = Some(crate::model::LineStyle::SparseDotted);
        chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Magnet;
        chart.chart_options.crosshair.vert_line_style = crate::model::LineStyle::Dashed;
        chart.chart_options.crosshair.horz_line_style = crate::model::LineStyle::SparseDotted;
        chart.chart_options.crosshair.vert_label_background_color = egui::Color32::RED;
        chart.chart_options.crosshair.horz_label_background_color = egui::Color32::BLUE;
        chart.chart_options.time_scale.timezone =
            crate::scales::TimezoneMode::Timezone("+01:00".parse().unwrap());
        let mut drawing = Drawing::new(1, DrawingToolType::HorizontalLine);
        drawing.chart_points = vec![ChartPoint::new(3.0, 50.0)];
        drawing.completed = true;
        drawing.pane = Some(rsi.0);
        chart.drawings_mut().drawings.push(drawing);
        let json = SavedLayout {
            mode: ChartLayoutMode::Single,
            charts: vec![chart.snapshot().unwrap()],
        }
        .to_json()
        .unwrap();

        let mut restored = Chart::new(BarData::from_bars(bars));
        restored.add_indicator(Box::new(SMA::new(3)));
        let layout = SavedLayout::from_json(&json).unwrap();
        assert!(restored.restore(&layout.charts[0]).unwrap().is_empty());
        assert_eq!(restored.symbol(), "A");
        assert_eq!(restored.timeframe(), Some(Timeframe::Min5));
        assert_eq!(restored.chart_type(), ChartType::Line);
        let studies: Vec<_> = restored
            .indicators()
            .map(|(id, study)| (id, study.name().to_owned(), study.inputs()[0].value.clone()))
            .collect();
        let names: Vec<_> = studies.iter().map(|(_, name, _)| name.as_str()).collect();
        assert_eq!(names, ["SMA", "RSI", "MACD"]);
        assert_eq!(
            studies[0].2,
            chart.indicators().next().unwrap().1.inputs()[0].value
        );
        let [(sma, ..), (rsi, ..), (macd, ..)] = studies.as_slice() else {
            unreachable!()
        };
        assert_eq!(restored.indicator_inputs(*sma), Some("7 close"));
        assert!(!restored.study_autoscale[0]);
        assert_eq!(
            restored.indicator_mut(*rsi).unwrap().source(),
            Some(crate::studies::StudySource::Study { id: sma.0, plot: 0 })
        );
        let original = chart.indicators().nth(1).unwrap().1.values();
        let output = restored.indicators().nth(1).unwrap().1.values();
        assert!(!output.is_empty());
        for at in 0..original.len() {
            assert_eq!(output.value(0, at), original.value(0, at));
        }
        let panes = restored.panes();
        assert_eq!(panes.len(), 3);
        assert_eq!(panes[1], PaneId::MAIN);
        assert_eq!(restored.pane_series(panes[0]), [*rsi, *macd]);
        assert_eq!(restored.pane_stretch_factor(panes[0]), Some(2.0));
        assert_eq!(restored.get_time_scale_state(), (0.25, 4.0));
        restored.scroll_to_real_time(false);
        assert_eq!(restored.get_time_scale_state(), (0.25, 7.0));
        restored.apply_synced_time_scale(99.0, 4.0);
        assert_eq!(restored.get_time_scale_state(), (12.0, 4.0));
        restored.apply_synced_time_scale(0.25, 4.0);
        assert_eq!(restored.config(), chart.config());
        assert_eq!(restored.chart_options(), chart.chart_options());
        assert_eq!(
            restored.state().price_scale().range(),
            chart.state().price_scale().range()
        );
        assert!(restored.state().price_scale().is_manual());
        assert_eq!(restored.pane_layout.panes[0].collapsed, Some(true));
        assert_eq!(
            restored.pane_layout.panes[0].scale.range(),
            chart.pane_layout.panes[0].scale.range()
        );
        assert_eq!(restored.pane_layout.maximized, Some(panes[0]));
        assert!(restored.preserves_empty_pane(panes[2]));
        let drawings = &restored.drawings().drawings;
        assert_eq!(drawings.len(), 1);
        assert_eq!(drawings[0].pane, Some(rsi.0));
        assert_eq!(drawings[0].chart_points[0].bar_idx, 3.0);
        assert!(!restored.can_undo());

        // An absent interval clears an existing interval, and malformed geometry is atomic.
        let mut snapshot = layout.charts[0].clone();
        snapshot.interval = None;
        restored.restore(&snapshot).unwrap();
        assert_eq!(restored.timeframe(), None);
        for bad in [0.0, f32::NAN, f32::INFINITY] {
            snapshot.bar_spacing = bad;
            snapshot.symbol = "Invalid".into();
            assert!(restored.restore(&snapshot).is_err());
            assert_eq!(restored.symbol(), "A");
        }
        for key in ["price_scale", "panes"] {
            let mut invalid: serde_json::Value = serde_json::from_str(&json).unwrap();
            invalid["charts"][0]["symbol"] = "Invalid".into();
            invalid["charts"][0]["drawings"] = serde_json::Value::Null;
            let scale = if key == "price_scale" {
                &mut invalid["charts"][0]["price_scale"]
            } else {
                &mut invalid["charts"][0]["panes"][0]["scale"]
            };
            scale["options"]["tick_marks"]["max_marks"] = usize::MAX.into();
            let invalid = SavedLayout::from_json(&invalid.to_string()).unwrap();
            assert!(restored.restore(&invalid.charts[0]).is_err());
            assert_eq!(restored.symbol(), "A");
            assert_eq!(restored.timeframe(), None);
        }
        // Both theme palettes survive instead of being replaced by dialog defaults.
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            chart.set_theme(&egui::Context::default(), &theme);
            let snapshot = chart.snapshot().unwrap();
            restored.restore(&snapshot).unwrap();
            assert_eq!(restored.config(), chart.config());
        }
        let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
        legacy["charts"][0]["options"]["tooltip"]
            .as_object_mut()
            .unwrap()
            .remove("show_volume_details");
        let config = &mut legacy["charts"][0]["config"];
        config["watermark_text"] = "A · Apple".into();
        config
            .as_object_mut()
            .unwrap()
            .remove("watermark_description");
        legacy["charts"][0]["settings"]["watermark"]["mode"] = "SymbolAndDescription".into();
        // Layouts saved before `thousands_sep` existed group thousands like a new chart.
        assert!(
            legacy["charts"][0]["config"]
                .as_object_mut()
                .unwrap()
                .remove("thousands_sep")
                .is_some()
        );
        for section in ["config", "settings"] {
            let value = if section == "settings" {
                &mut legacy["charts"][0][section]["status_line"]
            } else {
                &mut legacy["charts"][0][section]
            };
            assert!(
                value
                    .as_object_mut()
                    .unwrap()
                    .remove("show_market_status")
                    .is_some()
            );
        }
        restored.config_mut().show_market_status = false;
        restored.config_mut().thousands_sep = None;
        let migrated = SavedLayout::from_json(&legacy.to_string()).unwrap();
        restored.restore(&migrated.charts[0]).unwrap();
        assert!(!restored.chart_options().tooltip.show_volume_details);
        assert_eq!(restored.config().thousands_sep, Some(','));
        assert!(restored.config().show_market_status);
        assert_eq!(restored.config().watermark_text, None);
        assert!(restored.config().watermark_description);
        legacy["version"] = 1.into();
        for key in ["config", "options", "price_scale", "maximized"] {
            legacy["charts"][0].as_object_mut().unwrap().remove(key);
        }
        let migrated = SavedLayout::from_json(&legacy.to_string()).unwrap();
        restored.restore(&migrated.charts[0]).unwrap();
        assert_eq!(restored.chart_type(), ChartType::Line);

        let newer = json.replacen("\"version\":3", "\"version\":99", 1);
        assert!(SavedLayout::from_json(&newer).is_err());
    }

    /// A document that restore accepts saves a layout that loads again: values the next save
    /// cannot store or the next restore refuses are refused before the chart changes, and a
    /// study whose range overflows the price scale still saves.
    #[test]
    fn every_restored_layout_saves_a_layout_that_loads() {
        use crate::widget::test_frames::{chart, frame};
        let mut original = chart(&[10.0], &[10.0, 11.0]);
        original.set_symbol("A");
        original.add_indicator(Box::new(RSI::new(9)));
        let document = serde_json::to_value(original.snapshot().unwrap()).unwrap();
        for (pointer, value) in [
            ("/config/candle_width", 1e39),
            ("/options/tooltip/magnifier_size", -1e300),
            ("/options/crosshair/horz_line_width", 0.0),
            ("/options/crosshair/vert_line_width", -1.0),
            ("/options/time_scale/max_bar_spacing", 1e-9),
            ("/options/tooltip/font_size", 1e5),
            ("/config/watermark_font_size", 1e5),
            ("/config/axis_font_size", 0.0),
        ] {
            let mut invalid = document.clone();
            *invalid.pointer_mut(pointer).unwrap() = value.into();
            let snapshot: ChartSnapshot = serde_json::from_value(invalid).unwrap();
            let mut restored = chart(&[10.0], &[11.0]);
            restored.set_symbol("B");
            assert!(restored.restore(&snapshot).is_err(), "{pointer} {value}");
            assert_eq!(restored.symbol(), "B", "{pointer} {value}");
        }

        // Bands half of f64::MAX from the closes overflow the autoscaled range with its margins.
        original.add_indicator(Box::new(crate::studies::BollingerBands::new(2, f64::MAX)));
        frame(&egui::Context::default(), &mut original, 0.0, Vec::new());
        let json = SavedLayout {
            mode: ChartLayoutMode::Single,
            charts: vec![original.snapshot().unwrap()],
        }
        .to_json()
        .unwrap();
        let layout = SavedLayout::from_json(&json).unwrap();
        chart(&[10.0], &[11.0]).restore(&layout.charts[0]).unwrap();
    }

    /// Ignored style values save as finite equivalents without changing the live chart or its
    /// labels. Finite values keep their saved representation, including negative zero.
    #[test]
    fn display_format_with_an_ignored_min_move_saves() {
        use crate::model::SeriesDisplayFormat;
        use crate::widget::SeriesId;
        use crate::widget::test_frames::{chart, shapes};

        for added in [false, true] {
            for (min_move, base_line_width, expected) in [
                (0.0, 1.0, "12.62"),
                (f64::NAN, 1.0, "12.62"),
                (f64::INFINITY, 1.0, "12.62"),
                (f64::NEG_INFINITY, 1.0, "12.62"),
                (f64::NAN, f32::NAN, "12.62"),
                (0.0, f32::NAN, "12.62"),
                (0.0, f32::INFINITY, "12.62"),
                (0.0, f32::NEG_INFINITY, "12.62"),
                (-0.0, -0.0, "12.62"),
                (-0.25, -1.0, "12.62"),
                (0.25, 2.0, "12.50"),
            ] {
                let mut live = chart(&[12.0], &[12.624]);
                let data = live.bars(SeriesId::MAIN).to_vec();
                let id = if added {
                    let id = live.add_series(SeriesOptions::default());
                    live.set_data(id, data.clone());
                    id
                } else {
                    SeriesId::MAIN
                };
                let style = live.series_style_mut(id).unwrap();
                style.display_format = Some(SeriesDisplayFormat {
                    min_move,
                    ..Default::default()
                });
                style.base_line_width = base_line_width;
                shapes(&egui::Context::default(), &mut live, 0.0, vec![]);
                assert_eq!(
                    live.format_series_price(id, 12.624).as_deref(),
                    Some(expected)
                );

                let snapshot = live.snapshot().unwrap();
                let style = live.series_style(id).unwrap();
                assert_eq!(
                    style.display_format.unwrap().min_move.to_bits(),
                    min_move.to_bits()
                );
                assert_eq!(style.base_line_width.to_bits(), base_line_width.to_bits());
                let saved_format = if added {
                    let style = &snapshot.series.as_ref().unwrap()[0].options.style;
                    let width = if base_line_width.is_finite() {
                        base_line_width
                    } else {
                        0.0
                    };
                    assert_eq!(style.base_line_width.to_bits(), width.to_bits());
                    style.display_format.unwrap()
                } else {
                    snapshot.main_series_display_format.unwrap()
                };
                let step = if min_move.is_finite() { min_move } else { 0.0 };
                assert_eq!(saved_format.min_move.to_bits(), step.to_bits());

                let json = SavedLayout {
                    mode: ChartLayoutMode::Single,
                    charts: vec![snapshot],
                }
                .to_json()
                .unwrap();
                let layout = SavedLayout::from_json(&json).unwrap();
                let mut restored = Chart::new(BarData::from_bars(data.clone()));
                restored.restore(&layout.charts[0]).unwrap();
                let id = if added {
                    restored.restored_series_id(id.0).unwrap()
                } else {
                    id
                };
                restored.set_data(id, data);
                let painted = shapes(&egui::Context::default(), &mut restored, 0.0, vec![]);
                if !min_move.is_finite() || (added && !base_line_width.is_finite()) {
                    let mut invalid = layout.charts[0].clone();
                    if added {
                        let style = &mut invalid.series.as_mut().unwrap()[0].options.style;
                        style.display_format.as_mut().unwrap().min_move = min_move;
                        style.base_line_width = base_line_width;
                    } else {
                        invalid
                            .main_series_display_format
                            .as_mut()
                            .unwrap()
                            .min_move = min_move;
                    }
                    assert!(matches!(
                        restored.restore(&invalid),
                        Err(RestoreError::Layout("Layout values must be finite"))
                    ));
                }
                assert_eq!(
                    restored.format_series_price(id, 12.624).as_deref(),
                    Some(expected)
                );
                assert!(
                    painted.iter().any(|shape| matches!(shape,
                        egui::Shape::Text(text) if text.galley.text() == expected
                    )),
                    "added {added}, step {min_move}, width {base_line_width}"
                );
            }
        }
    }

    /// A layout that does not record the main series' assignment keeps the chart's, and series
    /// saved on that scale share it again. A layout that records it restores it.
    #[test]
    fn layout_without_main_assignment_keeps_comparison_on_main_scale() {
        use crate::model::SeriesScale::{self, Left, Right};
        use crate::widget::SeriesId;
        let bars: Vec<_> = (0..30)
            .map(|i| {
                let price = 100.0 + i as f64;
                Bar::new(
                    Utc.timestamp_opt(i * 60, 0).unwrap(),
                    price,
                    price,
                    price,
                    price,
                    1.0,
                )
            })
            .collect();
        let on = |scale: SeriesScale| {
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            chart.config.show_volume = false;
            assert!(chart.set_series_scale(SeriesId::MAIN, scale));
            chart
        };
        // The main series' scale when saved, that of the restoring chart, whether the layout
        // records it, and the assignments of the main series and the comparison after restore.
        for (saved_on, into, recorded, main, comparison) in [
            (Left, Left, false, Left, Left),
            (Right, Right, false, Right, Right),
            (Left, Right, false, Right, Left),
            (Right, Left, false, Left, Right),
            (Left, Left, true, Left, Left),
            (Right, Right, true, Right, Right),
            (Left, Right, true, Left, Left),
            (Right, Left, true, Right, Right),
        ] {
            let case = format!("{saved_on:?} into {into:?}, recorded {recorded}");
            let mut saved_chart = on(saved_on);
            let saved_id = saved_chart.add_comparison("Peer", egui::Color32::GOLD);
            saved_chart.set_data(saved_id, bars.clone());
            assert_eq!(saved_chart.series_scale(saved_id), Some(saved_on), "{case}");
            let mut saved = serde_json::to_value(saved_chart.snapshot().unwrap()).unwrap();
            let field = saved.as_object_mut().unwrap().remove("main_scale");
            assert!(field.is_some(), "{case}: written");
            if recorded {
                saved["main_scale"] = field.unwrap();
            }
            let mut chart = on(into);
            chart
                .restore(&serde_json::from_value(saved).unwrap())
                .unwrap();
            let restored = chart.restored_series_id(saved_id.0).unwrap();
            assert_eq!(
                (
                    chart.series_scale(SeriesId::MAIN),
                    chart.series_scale(restored)
                ),
                (Some(main), Some(comparison)),
                "{case}"
            );
            assert_eq!(
                std::ptr::eq(
                    chart.series_price_scale(SeriesId::MAIN).unwrap(),
                    chart.series_price_scale(restored).unwrap(),
                ),
                main == comparison,
                "{case}: one scale"
            );
        }
    }

    /// A layout saved after the original price pane was removed with the main series in
    /// it restores with the bars the host supplies for its symbol, into a new chart and into a
    /// chart showing another symbol. The SMA(3) of bar 70 where every close is `c` is
    /// (c + c + c) / 3 = c.
    #[test]
    fn layout_without_price_pane_reads_the_supplied_bars() {
        use crate::widget::SeriesId;
        use crate::widget::test_frames::{chart, frame};

        let bars = |close: f64| {
            chart(&[close - 10.0], &[close])
                .bars(SeriesId::MAIN)
                .to_vec()
        };
        let sma_at_70 = |chart: &Chart, id: SeriesId| {
            chart
                .indicators()
                .find(|(study, _)| *study == id)?
                .1
                .plot_value(0, 70)
        };
        let mut saved = Chart::new(BarData::from_bars(bars(200.0)));
        saved.set_symbol("BBB");
        let sma = saved.add_indicator(Box::new(SMA::new(3)));
        assert!(saved.move_indicator_to_pane(sma, sma));
        assert!(saved.remove_pane(PaneId::MAIN));
        assert_eq!(
            sma_at_70(&saved, sma),
            Some(200.0),
            "the saved chart reads its bars"
        );
        saved.set_data(SeriesId::MAIN, bars(300.0));
        frame(&egui::Context::default(), &mut saved, 0.0, vec![]);
        assert_eq!(
            sma_at_70(&saved, sma),
            Some(300.0),
            "the chart that lost the pane still takes bars"
        );
        let snapshot = saved.snapshot().unwrap();

        let mut other = Chart::new(BarData::from_bars(bars(100.0)));
        other.set_symbol("AAA");
        let mut observed = Vec::new();
        for mut restored in [Chart::new(BarData::new()), other] {
            restored.restore(&snapshot).unwrap();
            let id = restored.restored_series_id(sma.0).unwrap();
            assert_eq!(sma_at_70(&restored, id), None, "no bars before the host's");
            restored.set_data(SeriesId::MAIN, bars(200.0));
            frame(&egui::Context::default(), &mut restored, 0.0, vec![]);
            assert_eq!(restored.series_pane(SeriesId::MAIN), None);
            assert!(!restored.is_main_series_visible());
            observed.push(sma_at_70(&restored, id));
        }
        assert_eq!(observed, [Some(200.0), Some(200.0)]);
    }

    /// A chart built with a feed from a layout saved without the original price pane
    /// requests its symbol's history, as one built from the same layout before the removal does.
    /// A chart whose feed runs keeps it when the pane goes: no reload, and the next bar arrives.
    #[test]
    fn layout_without_price_pane_loads_its_feed() {
        use crate::data::{Datafeed, HistoryPage, HistoryRequest, Reply, Sink, Subscription};
        use crate::widget::SeriesId;
        use crate::widget::test_frames::{chart, frame};
        use std::sync::{Arc, Mutex};

        struct Requests(Mutex<Vec<String>>, Vec<Bar>, Mutex<Option<Sink>>);
        impl Datafeed for Requests {
            fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
                self.0.lock().unwrap().push(request.symbol);
                reply.send(Ok(HistoryPage::new(self.1.clone(), true)));
            }
            fn subscribe(&self, subscription: Subscription) {
                *self.2.lock().unwrap() = Some(subscription.sink);
            }
        }
        let data = chart(&[100.0], &[110.0]).bars(SeriesId::MAIN).to_vec();
        let feed = || Arc::new(Requests(Mutex::default(), data.clone(), Mutex::default()));
        let mut saved = chart(&[100.0], &[110.0]);
        saved.set_symbol("BBB");
        saved.add_indicator(Box::new(RSI::new(3)));
        let with_price_pane = saved.snapshot().unwrap();
        assert!(saved.remove_pane(PaneId::MAIN));
        let without_price_pane = saved.snapshot().unwrap();
        let requested = |snapshot: &ChartSnapshot| {
            let feed = feed();
            let (mut chart, _) = crate::widget::ChartBuilder::new()
                .with_feed(feed.clone())
                .build_restored(snapshot)
                .unwrap();
            let ctx = egui::Context::default();
            for step in 0..3 {
                frame(&ctx, &mut chart, f64::from(step), vec![]);
            }
            feed.0.lock().unwrap().first().cloned()
        };
        assert_eq!(
            [requested(&with_price_pane), requested(&without_price_pane)],
            [Some("BBB".to_owned()), Some("BBB".to_owned())]
        );
        let feed = feed();
        let (mut live, _) = crate::widget::ChartBuilder::new()
            .with_feed(feed.clone())
            .build_restored(&with_price_pane)
            .unwrap();
        let ctx = egui::Context::default();
        for step in 0..3 {
            frame(&ctx, &mut live, f64::from(step), vec![]);
        }
        assert!(live.remove_pane(PaneId::MAIN));
        let mut next = data[data.len() - 1].clone();
        next.time += chrono::Duration::days(1);
        feed.2.lock().unwrap().as_ref().unwrap().push_bar(next);
        frame(&ctx, &mut live, 3.0, vec![]);
        assert_eq!(
            (
                feed.0.lock().unwrap().len(),
                live.bars(SeriesId::MAIN).len()
            ),
            (1, 101),
            "one history request, and the bar after the 100 it sent"
        );
    }

    /// The main bars that stay live without the original pane stay under their limit: after the
    /// removal of the pane holding the main series, its undo and redo, the removal of the pane
    /// emptied by removing the series, and in a chart restoring a layout without the pane.
    /// A limit of 80 keeps 80 - 80 / 8 = 70 of 100 bars.
    #[test]
    fn main_bar_limit_holds_without_price_pane() {
        use crate::widget::SeriesId;
        use crate::widget::test_frames::chart;

        let limit = std::num::NonZeroUsize::new(80);
        let data = chart(&[100.0], &[110.0]).bars(SeriesId::MAIN).to_vec();
        let limited = || {
            let mut chart = chart(&[100.0], &[110.0]);
            chart.add_indicator(Box::new(RSI::new(3)));
            chart.set_bar_limit(SeriesId::MAIN, limit);
            chart
        };
        let kept = |chart: &mut Chart| {
            chart.set_data(SeriesId::MAIN, data.clone());
            (
                chart.bar_limit(SeriesId::MAIN),
                chart.bars(SeriesId::MAIN).len(),
            )
        };
        let mut live = limited();
        assert!(live.user_action(|chart| chart.remove_pane(PaneId::MAIN)));
        let removed = kept(&mut live);
        assert!(live.undo());
        assert!(live.redo());
        let redone = kept(&mut live);
        let mut emptied = limited();
        assert!(emptied.remove_series(SeriesId::MAIN));
        assert!(emptied.remove_pane(PaneId::MAIN));
        let mut restored = limited();
        restored.restore(&live.snapshot().unwrap()).unwrap();
        assert_eq!(
            [removed, redone, kept(&mut emptied), kept(&mut restored)],
            [(limit, 70); 4],
            "removed, redone, emptied, restored"
        );
    }

    /// An HLC area layout or template saved before the style held each line and fill restores as
    /// it was drawn: fills in the saved candle colours at alpha round(0.5 * 255) = 128, the close
    /// line in the rising colour at width 4, and no line in the current default colours.
    #[test]
    fn hlc_area_layout_saved_before_its_style_restores_as_drawn() {
        use egui::epaint::ColorMode;
        use egui::{Color32, Shape};
        let (up, down) = (Color32::from_rgb(1, 100, 2), Color32::from_rgb(101, 2, 3));
        let bars: Vec<_> = [
            (10.0, 18.0, 8.0, 16.0),
            (16.0, 20.0, 11.0, 12.0),
            (12.0, 15.0, 9.0, 14.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (open, high, low, close))| {
            let time = Utc
                .timestamp_opt(1_700_000_000 + i as i64 * 86_400, 0)
                .unwrap();
            Bar::new(time, open, high, low, close, 0.0)
        })
        .collect();
        let mut saved = Chart::new(BarData::from_bars(bars.clone()));
        saved.set_chart_type(ChartType::HlcArea);
        saved.config_mut().show_volume = false;
        saved.config_mut().bullish_color = up;
        saved.config_mut().bearish_color = down;
        let former = serde_json::json!({"fill_opacity": 0.5, "line_width": 4.0});
        for template in [false, true] {
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            if template {
                let mut json =
                    serde_json::to_value(ChartSettingsState::from_chart(&saved)).unwrap();
                json["chart_type_params"]["styles"]["hlc_area"] = former.clone();
                let settings: ChartSettingsState = serde_json::from_value(json).unwrap();
                chart.set_chart_type(ChartType::HlcArea);
                chart.config_mut().show_volume = false;
                settings.apply_to_chart(&mut chart).unwrap();
            } else {
                let mut json = serde_json::to_value(saved.snapshot().unwrap()).unwrap();
                json["chart_type_params"]["styles"]["hlc_area"] = former.clone();
                json["settings"]["chart_type_params"]["styles"]["hlc_area"] = former.clone();
                let snapshot: ChartSnapshot = serde_json::from_value(json).unwrap();
                chart.restore(&snapshot).unwrap();
            }
            let ctx = egui::Context::default();
            crate::widget::test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
            let painted = crate::widget::test_frames::shapes(&ctx, &mut chart, 1.0, vec![]);
            let filled = |color| {
                painted
                    .iter()
                    .any(|s| matches!(s, Shape::Path(p) if p.fill == color))
            };
            let widths = |color| -> Vec<f32> {
                painted
                    .iter()
                    .filter_map(|s| match s {
                        Shape::Path(p)
                            if p.stroke.width > 0.0
                                && p.stroke.color == ColorMode::Solid(color) =>
                        {
                            Some(p.stroke.width)
                        }
                        _ => None,
                    })
                    .collect()
            };
            assert!(filled(Color32::from_rgba_unmultiplied(1, 100, 2, 128)));
            assert!(filled(Color32::from_rgba_unmultiplied(101, 2, 3, 128)));
            assert!(widths(up).contains(&4.0));
            for default in [
                Color32::from_rgb(8, 153, 129),
                Color32::from_rgb(242, 54, 69),
                Color32::from_rgb(134, 137, 147),
            ] {
                assert_eq!(widths(default), Vec::<f32>::new());
            }
            // The Symbol settings show what is drawn.
            let settings = ChartSettingsState::from_chart(&chart);
            let style = settings.chart_type_params.unwrap().styles.hlc_area;
            assert!(!style.high_line_visible && !style.low_line_visible);
            assert_eq!((style.close_line.color, style.close_line.width), (up, 4.0));
            assert_eq!(style.close_to_low_fill.a(), 128);
        }
    }
}
