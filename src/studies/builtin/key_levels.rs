//! Key Levels.
//!
//! Prices at which swing highs and lows repeatedly formed, found by clustering the pivots of
//! [`PivotPointsHighLow`](super::PivotPointsHighLow). Highs and lows share one pool, so a
//! resistance that later held as a support is one level.
//!
//! # Rule
//!
//! Pivots are confirmed `right` bars after their bar, with `left` and `right` bars on each side.
//! The latest 1000 are kept. Sorted by price (older first among equals), the lowest pivot not
//! yet in a level opens one and every following pivot priced at most `tolerance` percent above it
//! (price <= lowest + |lowest| * tolerance / 100) joins it, so a level never spans more than the
//! tolerance. A cluster of at least `min_touches` pivots is a level at the mean of their prices;
//! smaller clusters are not shown.
//!
//! Each level is a zone from its earliest pivot bar to the last bar, in the resistance colour
//! when at or above the last close and the support colour below it. The table lists the levels
//! from the highest down, with their number of touches and each contributing pivot as
//! `bar@price`, bar being the zero-based index of the bar. The defaults are 10 bars on each side,
//! 2 touches and a tolerance of 0.25%. Overlay indicator without per-bar values.

use super::clamp_param;
use super::pivot_points_high_low::Swings;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, StudyCell, StudyTable, StudyZone,
};
use egui::Color32;
use std::collections::VecDeque;

/// The most pivots that are clustered.
const MAX_PIVOTS: usize = 1000;

/// Key Levels: prices at which swing highs and lows repeatedly formed.
#[derive(Clone)]
pub struct KeyLevels {
    left: usize,
    right: usize,
    min_touches: usize,
    tolerance: f64,
    values: IndicatorValues,
    stepper: Stepper<State>,
    zones: Vec<StudyZone>,
    table: StudyTable,
    /// Resistance, support.
    colors: [Color32; 2],
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    source: usize,
    swings: Swings,
    /// Bar and price of each pivot, oldest first.
    pivots: VecDeque<(usize, f64)>,
    close: Option<f64>,
}

impl KeyLevels {
    /// Pivots with `left` and `right` bars on each side (at least 1), clustered within
    /// `tolerance` percent (0 to 100, NaN to 0) into levels of at least `min_touches` pivots
    /// (at least 2).
    pub fn new(left: usize, right: usize, min_touches: usize, tolerance: f64) -> Self {
        Self {
            left: left.max(1),
            right: right.max(1),
            min_touches: min_touches.max(2),
            tolerance: clamp_param(tolerance, 0.0, 100.0),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            zones: Vec::new(),
            table: StudyTable::default(),
            colors: [palette::RED, palette::GREEN],
            visible: true,
        }
    }

    /// The clusters of `pivots` of at least `min_touches` within `tolerance` percent, lowest
    /// first, as the pivots (sorted by bar) of each.
    fn clusters(&self, pivots: &VecDeque<(usize, f64)>) -> Vec<Vec<(usize, f64)>> {
        let mut sorted: Vec<_> = pivots.iter().copied().collect();
        sorted.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let mut clusters: Vec<Vec<(usize, f64)>> = Vec::new();
        let mut limit = f64::NEG_INFINITY;
        for pivot in sorted {
            match clusters.last_mut().filter(|_| pivot.1 <= limit) {
                Some(cluster) => cluster.push(pivot),
                None => {
                    limit = pivot.1 + pivot.1.abs() * self.tolerance / 100.0;
                    clusters.push(vec![pivot]);
                }
            }
        }
        clusters.retain(|cluster| cluster.len() >= self.min_touches);
        for cluster in &mut clusters {
            cluster.sort_by_key(|pivot| pivot.0);
        }
        clusters
    }
}

/// Construct with the conventional default parameters.
impl Default for KeyLevels {
    fn default() -> Self {
        Self::new(10, 10, 2, 0.25)
    }
}

impl Indicator for KeyLevels {
    crate::studies::inputs::study_inputs!(self;
        left: length("Pivot left bars", 1, usize::MAX),
        right: length("Pivot right bars", 1, usize::MAX),
        min_touches: length("Minimum touches", 2, usize::MAX),
        tolerance: number("Price tolerance (%)", 0.0, 100.0)
    ; {});

    fn name(&self) -> &str {
        "Key Levels"
    }

    fn desc(&self) -> &str {
        "Key Levels - Prices where swing highs and lows repeatedly formed"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (left, right) = (self.left, self.right);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.source;
                state.source += 1;
                for (bar, price) in state
                    .swings
                    .push(index, bar, left, right)
                    .into_iter()
                    .flatten()
                {
                    state.pivots.push_back((bar, price));
                    if state.pivots.len() > MAX_PIVOTS {
                        state.pivots.pop_front();
                    }
                }
                if bar.close.is_finite() {
                    state.close = Some(bar.close);
                }
                IndicatorValue::None
            });
        let state = self.stepper.state();
        let clusters = self.clusters(&state.pivots);
        let mean = |cluster: &[(usize, f64)]| {
            cluster.iter().map(|pivot| pivot.1).sum::<f64>() / cluster.len() as f64
        };
        self.zones = clusters
            .iter()
            .map(|cluster| {
                let price = mean(cluster);
                StudyZone {
                    bars: cluster[0].0..state.source,
                    top: price,
                    bottom: price,
                    plot: usize::from(state.close.is_some_and(|close| price < close)),
                }
            })
            .collect();
        let rows = clusters
            .iter()
            .rev()
            .map(|cluster| {
                let pivots: Vec<_> = cluster
                    .iter()
                    .map(|&(bar, price)| format!("{bar}@{}", self.format_value(price)))
                    .collect();
                vec![
                    StudyCell::Text(self.format_value(mean(cluster))),
                    StudyCell::Text(cluster.len().to_string()),
                    StudyCell::Text(pivots.join(", ")),
                ]
            })
            .collect();
        self.table = StudyTable {
            title: "Key levels".to_string(),
            columns: ["Level", "Touches", "Pivots"].map(String::from).to_vec(),
            rows,
        };
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn zones(&self) -> &[StudyZone] {
        &self.zones
    }

    fn table(&self) -> Option<&StudyTable> {
        Some(&self.table)
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (line, color) in self.colors.iter_mut().zip(colors) {
            *line = color;
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        vec!["Resistance".to_string(), "Support".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// Pivots of one bar on each side, hand-labelled: highs 100.0 (bar 1), 100.2 (bar 5) and
    /// 106 (bar 8); lows 90 (bar 3), 96 (bar 6) and 100.1 (bar 9). With a tolerance of 0.25% the
    /// pivots 100.0, 100.1 and 100.2 (limit 100.25) form one level of three touches at 100.1,
    /// mixing highs and a low; 90, 96 and 106 stand alone and are dropped at two touches. The last
    /// close 101 is above the level, so it is a support. A tolerance of 0.12% (limit 100.12)
    /// leaves 100.0 and 100.1, and 100.2 alone.
    #[test]
    fn nearby_pivots_form_a_level() {
        let data: Vec<Bar> = [
            (95.0, 94.0, 94.5),
            (100.0, 96.0, 98.0),
            (95.0, 93.0, 94.0),
            (97.0, 90.0, 95.0),
            (98.0, 94.0, 96.0),
            (100.2, 97.0, 99.0),
            (99.0, 96.0, 97.0),
            (104.0, 101.0, 103.0),
            (106.0, 103.0, 105.0),
            (104.0, 100.1, 101.0),
            (105.0, 102.0, 101.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (high, low, close))| {
            let time = Utc.timestamp_opt(i as i64 * 60, 0).unwrap();
            Bar::new(time, close, high, low, close, 1.0)
        })
        .collect();
        let mut levels = KeyLevels::new(1, 1, 2, 0.25);
        levels.calculate(&data);
        let zone = &levels.zones()[0];
        assert_eq!(levels.zones().len(), 1);
        assert_eq!(zone.bars, 1..11);
        assert!((zone.top - 100.1).abs() < 1e-9);
        assert_eq!(zone.plot, 1);
        let table = levels.table().unwrap();
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0][1], StudyCell::Text("3".into()));
        assert_eq!(
            table.rows[0][2],
            StudyCell::Text("1@100.00, 5@100.20, 9@100.10".into())
        );
        let mut tight = KeyLevels::new(1, 1, 2, 0.12);
        tight.calculate(&data);
        assert_eq!(
            tight.table().unwrap().rows[0][1],
            StudyCell::Text("2".into())
        );
    }
}
