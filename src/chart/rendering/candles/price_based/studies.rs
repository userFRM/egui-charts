//! Study results calculated from rendered element OHLC, alongside the source-bar results.

use crate::model::Bar;
use crate::studies::IndicatorRegistry;
use std::sync::Arc;

pub(crate) struct ElementStudies {
    pub(crate) studies: IndicatorRegistry,
    pub(crate) pending_from: Vec<Option<usize>>,
    pub(crate) next_refresh: Vec<f64>,
    pub(crate) dirty: bool,
    pub(crate) bars: Arc<[Bar]>,
    timeframe: Option<crate::model::Timeframe>,
}

impl Default for ElementStudies {
    fn default() -> Self {
        Self {
            studies: IndicatorRegistry::new(),
            pending_from: Vec::new(),
            next_refresh: Vec::new(),
            dirty: true,
            bars: Arc::from([]),
            timeframe: None,
        }
    }
}

impl ElementStudies {
    pub(crate) fn sync(
        &mut self,
        source: &IndicatorRegistry,
        bars: Arc<[Bar]>,
        timeframe: Option<crate::model::Timeframe>,
    ) {
        self.dirty |= self.timeframe != timeframe;
        self.timeframe = timeframe;
        if std::mem::take(&mut self.dirty) {
            self.studies = IndicatorRegistry::new();
            for study in source.indicators() {
                let mut study = study.clone_box();
                study.update(&[], 0);
                self.studies.add(study);
            }
            self.pending_from = vec![Some(0); source.indicators().len()];
            self.next_refresh = vec![f64::NEG_INFINITY; source.indicators().len()];
        }
        if !Arc::ptr_eq(&self.bars, &bars) {
            let shared = self.bars.len().min(bars.len());
            let from = self.bars[..shared]
                .iter()
                .zip(&bars[..shared])
                .position(|(old, new)| old != new)
                .unwrap_or(shared);
            if from < shared || self.bars.len() != bars.len() {
                for pending in &mut self.pending_from {
                    *pending = Some(pending.map_or(from, |old| old.min(from)));
                }
            }
            self.bars = bars;
        }
    }
}
