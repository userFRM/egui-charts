//! A chart's state as a serializable snapshot, and named layouts of charts as versioned
//! documents.

use super::{Chart, ChartLayoutMode, PaneId, SeriesId};
use crate::drawings::persistence::{DrawingStorage, SaveError, SkippedDrawing};
use crate::model::{ChartType, ChartTypeParams, Timeframe};
use crate::ui::dialogs::templates::StudyTemplate;
use crate::ui::top_toolbar::components::settings_menu::ChartSettingsState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One study of a [`ChartSnapshot`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedStudy {
    /// The study's id when it was saved, which drawings in its pane refer to.
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
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPane {
    /// Whether it is the price pane.
    pub main: bool,
    /// Its share of the chart height; see [`Chart::pane_stretch_factor`].
    pub stretch: f32,
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
    /// The studies in the order they were added, except the volume study that
    /// [`ChartConfig::show_volume`](crate::config::ChartConfig::show_volume) adds.
    pub studies: Vec<SavedStudy>,
    /// The panes from top to bottom that hold the price series or a saved study.
    pub panes: Vec<SavedPane>,
    /// The space between bars in points.
    pub bar_spacing: f32,
    /// The space after the last bar, in bars.
    pub right_offset: f32,
    /// The symbol's drawings as a drawings document (see [`DrawingStorage::to_json`]), or `None`
    /// when the chart keeps them in storage of their own (see [`Chart::set_storage`]).
    pub drawings: Option<serde_json::Value>,
}

impl Chart {
    /// Captures the chart's state. Fails when a drawing cannot be saved; see
    /// [`save_drawings`](Self::save_drawings).
    pub fn snapshot(&self) -> Result<ChartSnapshot, SaveError> {
        let drawings = match self.drawing_store {
            Some(_) => None,
            None => {
                Some(serde_json::to_value(self.save_drawings()?).expect("drawings were validated"))
            }
        };
        let panes: Vec<_> = self
            .panes()
            .into_iter()
            .filter(|pane| {
                *pane == PaneId::MAIN
                    || self
                        .pane_series(*pane)
                        .iter()
                        .any(|id| self.saved_study(*id))
            })
            .collect();
        let studies = self
            .indicators()
            .filter(|(id, _)| self.saved_study(*id))
            .map(|(id, study)| SavedStudy {
                id: id.0,
                study: StudyTemplate::from_study(study),
                caption: self.indicator_inputs(id).unwrap_or_default().to_owned(),
                autoscale: self.study_autoscale[self.study_index(id)],
                pane: self
                    .series_pane(id)
                    .and_then(|pane| panes.iter().position(|p| *p == pane))
                    .unwrap_or(0),
            })
            .collect();
        let (bar_spacing, right_offset) = self.get_time_scale_state();
        Ok(ChartSnapshot {
            symbol: self.symbol.clone(),
            interval: self.timeframe,
            extended_hours: self.extended_hours,
            chart_type: self.chart_type,
            chart_type_params: self.chart_type_params.clone(),
            main_series_visible: self.main_series_visible,
            settings: ChartSettingsState::from_chart(self),
            studies,
            panes: panes
                .iter()
                .map(|pane| SavedPane {
                    main: *pane == PaneId::MAIN,
                    stretch: self.pane_stretch_factor(*pane).unwrap_or(1.0),
                })
                .collect(),
            bar_spacing,
            right_offset,
            drawings,
        })
    }

    /// Shows what `snapshot` captured: its symbol, interval, chart type, settings, studies,
    /// panes and view, and its drawings unless the chart keeps them in storage. Studies are
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
        let drawings = match (&snapshot.drawings, &self.drawing_store) {
            (Some(drawings), None) => {
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
        }
        self.set_extended_hours(snapshot.extended_hours);
        self.set_chart_type(snapshot.chart_type);
        self.set_chart_type_params(snapshot.chart_type_params.clone());
        self.set_main_series_visible(snapshot.main_series_visible);
        snapshot.settings.apply_to_chart(self)?;
        let replaced: Vec<_> = self
            .indicators()
            .map(|(id, _)| id)
            .filter(|id| self.saved_study(*id))
            .collect();
        for id in replaced {
            self.remove_series(id);
        }
        self.restored_studies.clear();
        let mut panes: HashMap<usize, PaneId> = HashMap::new();
        for (saved, study) in snapshot.studies.iter().zip(studies) {
            let id = self.add_indicator(study);
            self.set_indicator_inputs(id, saved.caption.clone());
            self.set_indicator_autoscale(id, saved.autoscale);
            self.restored_studies.insert(saved.id, id.0);
            if snapshot.panes.get(saved.pane).is_none_or(|pane| pane.main) {
                self.move_series_to_pane(id, PaneId::MAIN);
            } else if let Some(pane) = panes.get(&saved.pane) {
                self.move_series_to_pane(id, *pane);
            } else {
                if self.series_pane(id) == Some(PaneId::MAIN) {
                    self.move_indicator_to_pane(id, id);
                }
                if let Some(pane) = self.series_pane(id) {
                    panes.insert(saved.pane, pane);
                }
            }
        }
        for (at, saved) in snapshot.panes.iter().enumerate() {
            let pane = if saved.main {
                Some(PaneId::MAIN)
            } else {
                panes.get(&at).copied()
            };
            if let Some(pane) = pane {
                self.move_pane(pane, at);
                self.set_pane_stretch_factor(pane, saved.stretch);
            }
        }
        self.apply_synced_time_scale(snapshot.bar_spacing, snapshot.right_offset);
        let skipped = match drawings {
            Some(json) => {
                let skipped = self.load_drawings(&json).map_err(|e| e.to_string())?;
                self.remap_drawing_panes();
                skipped
            }
            None => Vec::new(),
        };
        self.history.clear();
        Ok(skipped)
    }

    /// Whether `id` is a study a snapshot holds: any but the volume study of the settings.
    fn saved_study(&self, id: SeriesId) -> bool {
        #[cfg(feature = "studies")]
        if self.volume_study == Some(id) {
            return false;
        }
        self.study_ids.contains(&id)
    }

    fn study_index(&self, id: SeriesId) -> usize {
        self.study_ids
            .iter()
            .position(|study| *study == id)
            .expect("a study of the chart")
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
    pub const VERSION: u32 = 1;

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
            Self::VERSION => serde_json::from_value(document.layout),
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
        chart.move_indicator_to_pane(macd, rsi);
        let pane = chart.series_pane(rsi).unwrap();
        chart.move_pane(pane, 0);
        chart.set_pane_stretch_factor(pane, 2.0);
        chart.apply_synced_time_scale(9.0, 4.0);
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
        let panes = restored.panes();
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[1], PaneId::MAIN);
        assert_eq!(restored.pane_series(panes[0]), [*rsi, *macd]);
        assert_eq!(restored.pane_stretch_factor(panes[0]), Some(2.0));
        assert_eq!(restored.get_time_scale_state(), (9.0, 4.0));
        let drawings = &restored.drawings().drawings;
        assert_eq!(drawings.len(), 1);
        assert_eq!(drawings[0].pane, Some(rsi.0));
        assert_eq!(drawings[0].chart_points[0].bar_idx, 3.0);
        assert!(!restored.can_undo());

        let newer = json.replacen("\"version\":1", "\"version\":2", 1);
        assert!(SavedLayout::from_json(&newer).is_err());
    }
}
