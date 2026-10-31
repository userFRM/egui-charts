//! A chart's state as a serializable snapshot, and named layouts of charts as versioned
//! documents.

use super::{Chart, ChartLayoutMode, PaneId, SeriesOptions};
use crate::drawings::persistence::{DrawingStorage, SaveError, SkippedDrawing};
use crate::model::{ChartType, ChartTypeParams, Marker, MarkerOptions, PriceLine, Timeframe};
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
/// [`Chart::set_data`] on [`Chart::restored_series_id`] of [`id`](Self::id).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedSeries {
    /// The series' id when saved, which [`SavedPane::members`] and drawings in its pane use.
    pub id: usize,
    /// Its name, chart type, colours and scale assignment. A style's autoscale function is not
    /// saved.
    pub options: SeriesOptions,
    /// Whether it is shown.
    pub visible: bool,
    /// The position of its pane in [`ChartSnapshot::panes`].
    pub pane: usize,
    /// Its markers and price lines.
    pub annotations: SavedAnnotations,
}

/// One pane of a [`ChartSnapshot`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedPane {
    /// Whether it is the price pane.
    pub main: bool,
    /// Its share of the chart height; see [`Chart::pane_stretch_factor`].
    pub stretch: f32,
    /// Explicit collapse state, or automatic collapse in a narrow chart.
    #[serde(default)]
    pub collapsed: Option<bool>,
    /// Keep the pane when its last study leaves.
    #[serde(default)]
    pub preserve: bool,
    /// Saved study and series ids in legend order; empty in version 1 layouts.
    #[serde(default)]
    pub members: Vec<usize>,
    /// The pane's price scale, including its manual range.
    #[serde(default)]
    pub scale: Option<crate::scales::PriceScale>,
}

/// What a chart shows and how: everything a user sets on it, without the bars, which come from
/// the host's feed. Take one with [`Chart::snapshot`] and apply it with [`Chart::restore`].
/// The trading and alert settings of the settings dialog belong to the host and are not part
/// of it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChartSnapshot {
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
    /// 3, which leave the chart's added series in place.
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
                options: item.options.clone(),
                visible: item.visible,
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
            symbol: self.symbol.clone(),
            interval: self.timeframe,
            extended_hours: self.extended_hours,
            chart_type: self.chart_type,
            chart_type_params: self.chart_type_params.clone(),
            main_series_visible: self.main_series_visible,
            main_series_alerts: self.main_series_style.alerts,
            settings: ChartSettingsState::from_chart(self),
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
                        members: pane.members.iter().map(|id| id.0).collect(),
                        scale: (*id != PaneId::MAIN).then(|| pane.scale.clone()),
                    }
                })
                .collect(),
            bar_spacing,
            right_offset,
            drawings,
        };
        // A layout that restore would refuse is an error now rather than at the next load.
        check(&snapshot, factory)
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
    /// [`set_data`](Self::set_data), as for the chart's own feed.
    pub fn restore(&mut self, snapshot: &ChartSnapshot) -> Result<Vec<SkippedDrawing>, String> {
        self.restore_with(snapshot, &IndicatorFactory::new())
    }

    /// Shows what `snapshot` captured like [`restore`](Self::restore), building the studies
    /// through `factory`, so that studies it registers are restored with their inputs, styles
    /// and source links.
    pub fn restore_with(
        &mut self,
        snapshot: &ChartSnapshot,
        factory: &IndicatorFactory,
    ) -> Result<Vec<SkippedDrawing>, String> {
        let (studies, drawings) = check(snapshot, factory)?;
        self.set_symbol(snapshot.symbol.clone());
        if let Some(interval) = snapshot.interval {
            self.set_timeframe(interval);
        } else {
            self.timeframe = None;
            self.drawings.curr_timeframe.clear();
        }
        self.set_extended_hours(snapshot.extended_hours);
        self.set_chart_type(snapshot.chart_type);
        self.set_chart_type_params(snapshot.chart_type_params.clone());
        self.set_main_series_visible(snapshot.main_series_visible);
        self.main_series_style.alerts = snapshot.main_series_alerts;
        snapshot.settings.apply_to_chart(self)?;
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
        if let Some(scale) = &snapshot.price_scale {
            *self.state.price_scale_mut() = scale.clone();
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
            .retain(|pane| pane.id == PaneId::MAIN || !pane.members.is_empty());
        let mut panes: HashMap<usize, PaneId> = HashMap::new();
        for (at, pane) in snapshot.panes.iter().enumerate() {
            panes.insert(
                at,
                if pane.main {
                    PaneId::MAIN
                } else {
                    self.add_pane()
                },
            );
        }
        for (saved, study) in snapshot.studies.iter().zip(studies) {
            let id = self.add_indicator(study);
            self.set_indicator_inputs(id, saved.caption.clone());
            self.set_indicator_autoscale(id, saved.autoscale);
            self.restored_studies.insert(saved.id, id.0);
            self.move_series_to_pane(id, panes[&saved.pane]);
        }
        for saved in snapshot.series.iter().flatten() {
            let id = self.add_series(saved.options.clone());
            self.set_series_visible(id, saved.visible);
            self.restore_annotations(id, &saved.annotations);
            self.restored_studies.insert(saved.id, id.0);
            self.move_series_to_pane(id, panes[&saved.pane]);
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
            pane.preserve = saved.preserve;
            if !saved.members.is_empty() {
                pane.members.sort_by_key(|id| {
                    saved
                        .members
                        .iter()
                        .position(|old| self.restored_studies.get(old) == Some(&id.0))
                        .unwrap_or(usize::MAX)
                });
            }
            if let Some(scale) = &saved.scale {
                pane.scale = scale.clone();
            }
        }
        self.pane_layout.maximized = snapshot.maximized.map(|at| panes[&at]);
        self.apply_synced_time_scale(snapshot.bar_spacing, snapshot.right_offset);
        let skipped = match drawings {
            Some(json) => {
                let skipped = self
                    .load_drawings_remapped(&json, true)
                    .map_err(|e| e.to_string())?;
                self.history.drawing_edits += 1;
                self.save_restored_drawings();
                skipped
            }
            None => Vec::new(),
        };
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

/// The studies of a checked snapshot, built in order, and its drawings document.
type Checked = (Vec<Box<dyn crate::studies::Indicator>>, Option<String>);

/// Checks `snapshot` as [`Chart::restore_with`] does before changing the chart.
fn check(snapshot: &ChartSnapshot, factory: &IndicatorFactory) -> Result<Checked, String> {
    let studies = snapshot
        .studies
        .iter()
        .map(|saved| saved.study.create_with(factory))
        .collect::<Result<Vec<_>, _>>()?;
    check_source_links(&snapshot.studies, &studies)?;
    snapshot.settings.validate()?;
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
        || snapshot.panes.iter().filter(|pane| pane.main).count() != 1
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
            .filter_map(|pane| pane.scale.as_ref())
            .any(|scale| !valid_scale(scale))
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
        return Err("Invalid layout geometry".into());
    }
    // JSON has no NaN or infinity: serde writes them as null, which a number field then
    // refuses, so the next save of such a value could not be loaded.
    if serde_json::to_value(snapshot)
        .and_then(serde_json::from_value::<ChartSnapshot>)
        .is_err()
    {
        return Err("Layout values must be finite".into());
    }
    if let Some(options) = &snapshot.options
        && !(options.crosshair.vert_line_width > 0.0 && options.crosshair.horz_line_width > 0.0)
    {
        return Err("Crosshair width must be above zero".into());
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
        return Err("Bar spacing limits must be 0 or at least 0.01".into());
    }
    let font_sizes = &crate::styles::typography::FONT_SIZES;
    if snapshot
        .options
        .iter()
        .map(|options| options.tooltip.font_size)
        .chain(snapshot.config.iter().flat_map(|config| {
            std::iter::once(config.watermark_font_size).chain(config.axis_font_size)
        }))
        .any(|size| !font_sizes.contains(&size))
    {
        return Err("Font sizes must be between 1 and 256".into());
    }
    let drawings = match &snapshot.drawings {
        Some(drawings) => {
            let json = drawings.to_string();
            let (storage, _) = DrawingStorage::from_json(&json).map_err(|e| e.to_string())?;
            if storage.symbol != snapshot.symbol {
                return Err("The drawings belong to another symbol".into());
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
) -> Result<(), String> {
    let mut parents = vec![Vec::new(); studies.len()];
    for (at, saved) in studies.iter().enumerate() {
        for (_, value) in &saved.study.inputs {
            if let crate::studies::InputValue::Source(crate::studies::StudySource::Study {
                id,
                plot,
            }) = value
            {
                let parent = studies
                    .iter()
                    .position(|other| other.id == *id)
                    .filter(|parent| *plot < built[*parent].line_count())
                    .ok_or("A study source names no study or plot of the layout")?;
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
    if resolved.contains(&false) {
        return Err("Study sources form a cycle".into());
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

        restored.restore_with(&snapshot, &factory).unwrap();
        assert_eq!(restored.symbol(), "A");
        let names: Vec<_> = restored
            .indicators()
            .map(|(_, study)| study.name().to_owned())
            .collect();
        assert_eq!(names, ["Bar range", "RSI"]);
        let new_spread = restored.restored_series_id(spread.0).unwrap();
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
        let migrated = SavedLayout::from_json(&legacy.to_string()).unwrap();
        restored.restore(&migrated.charts[0]).unwrap();
        assert!(!restored.chart_options().tooltip.show_volume_details);
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
}
