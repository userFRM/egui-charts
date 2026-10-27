//! A chart's state as a serializable snapshot, and named layouts of charts as versioned
//! documents.

use super::{Chart, ChartLayoutMode, PaneId};
use crate::drawings::persistence::{DrawingStorage, SaveError, SkippedDrawing};
use crate::model::{ChartType, ChartTypeParams, Timeframe};
use crate::studies::StudyTemplate;
use crate::widget::settings::ChartSettingsState;
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
    /// Saved study ids in legend order; empty in version 1 layouts.
    #[serde(default)]
    pub members: Vec<usize>,
    /// The pane's price scale, including its manual range.
    #[serde(default)]
    pub scale: Option<crate::scales::PriceScale>,
}

/// What a chart shows and how: everything a user sets on it, without the bars, which come from
/// the host's feed. Take one with [`Chart::snapshot`] and apply it with [`Chart::restore`].
/// Host series, markers, price lines and the trading and alert settings of the settings dialog
/// belong to the host and are not part of it.
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
    /// The display settings of the settings dialog, the price scale's mode and margins included.
    pub settings: ChartSettingsState,
    /// Complete appearance settings; absent in version 1 layouts.
    #[serde(default)]
    pub config: Option<crate::config::ChartConfig>,
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
    /// study or when a drawing cannot be saved; see
    /// [`save_drawings`](Self::save_drawings).
    pub fn snapshot(&self) -> Result<ChartSnapshot, SaveError> {
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
        let studies: Vec<_> = self
            .indicators()
            .enumerate()
            .map(|(index, (id, study))| SavedStudy {
                id: id.0,
                study: StudyTemplate::from_study(study),
                caption: self.indicator_inputs(id).unwrap_or_default().to_owned(),
                autoscale: self.study_autoscale[index],
                pane: self
                    .series_pane(id)
                    .and_then(|pane| panes.iter().position(|p| *p == pane))
                    .unwrap_or(0),
            })
            .collect();
        for saved in &studies {
            saved
                .study
                .create()
                .map_err(|error| SaveError::Json(serde::ser::Error::custom(error)))?;
        }
        let (bar_spacing, right_offset) = self.get_time_scale_state();
        Ok(ChartSnapshot {
            symbol: self.symbol.clone(),
            interval: self.timeframe,
            extended_hours: self.extended_hours,
            chart_type: self.chart_type,
            chart_type_params: self.chart_type_params.clone(),
            main_series_visible: self.main_series_visible,
            settings: ChartSettingsState::from_chart(self),
            config: Some(self.config.clone()),
            options: Some(self.chart_options.clone()),
            price_scale: Some(self.state.price_scale().clone()),
            maximized: self
                .pane_layout
                .maximized
                .and_then(|id| panes.iter().position(|pane| *pane == id)),
            studies,
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
        })
    }

    /// Shows what `snapshot` captured: its symbol, interval, chart type, settings, studies,
    /// panes and view, and its drawings. Studies are
    /// built through [`StudyTemplate::create`]. Clears undo history and returns the drawings
    /// that could not be read. Fails without changing the chart for an unknown study, invalid
    /// settings or an unreadable drawings document.
    pub fn restore(&mut self, snapshot: &ChartSnapshot) -> Result<Vec<SkippedDrawing>, String> {
        let studies = snapshot
            .studies
            .iter()
            .map(|saved| saved.study.create())
            .collect::<Result<Vec<_>, _>>()?;
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
                .collect::<std::collections::HashSet<_>>()
                .len()
                != snapshot.studies.len()
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
            || !snapshot.bar_spacing.is_finite()
            || snapshot.bar_spacing <= 0.0
            || !snapshot.right_offset.is_finite()
            || snapshot
                .maximized
                .is_some_and(|pane| pane >= snapshot.panes.len())
        {
            return Err("Invalid layout geometry".into());
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
        snapshot.settings.apply_to_chart(self)?;
        if let Some(config) = &snapshot.config {
            self.config = config.clone();
        }
        if let Some(options) = &snapshot.options {
            self.set_chart_options(options.clone());
        }
        if let Some(scale) = &snapshot.price_scale {
            *self.state.price_scale_mut() = scale.clone();
        }
        let replaced: Vec<_> = self.indicators().map(|(id, _)| id).collect();
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
        for saved in &snapshot.studies {
            let id = super::SeriesId(self.restored_studies[&saved.id]);
            for (input, value) in &saved.study.inputs {
                if let crate::studies::InputValue::Source(crate::studies::StudySource::Study {
                    id: parent,
                    plot,
                }) = value
                {
                    let source = crate::studies::StudySource::Study {
                        id: self
                            .restored_studies
                            .get(parent)
                            .copied()
                            .unwrap_or(usize::MAX),
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
                let skipped = self.load_drawings(&json).map_err(|e| e.to_string())?;
                self.remap_drawing_panes();
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
    pub const VERSION: u32 = 2;

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
            1 | Self::VERSION => serde_json::from_value(document.layout),
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
    use crate::model::{Bar, BarData};
    use crate::studies::{MACD, RSI, SMA};
    use chrono::{TimeZone, Utc};

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
        let migrated = SavedLayout::from_json(&legacy.to_string()).unwrap();
        restored.restore(&migrated.charts[0]).unwrap();
        assert!(!restored.chart_options().tooltip.show_volume_details);
        legacy["version"] = 1.into();
        for key in ["config", "options", "price_scale", "maximized"] {
            legacy["charts"][0].as_object_mut().unwrap().remove(key);
        }
        let migrated = SavedLayout::from_json(&legacy.to_string()).unwrap();
        restored.restore(&migrated.charts[0]).unwrap();
        assert_eq!(restored.chart_type(), ChartType::Line);

        let newer = json.replacen("\"version\":2", "\"version\":99", 1);
        assert!(SavedLayout::from_json(&newer).is_err());
    }
}
