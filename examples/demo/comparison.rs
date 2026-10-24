//! Host-fed comparison lines aligned to the main series by bar time.

use super::live::LiveFeed;
use egui::Color32;
use egui_charts::model::{Bar, BarClock, Timeframe};
use egui_charts::studies::{Indicator, IndicatorValue, IndicatorValues};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Comparison {
    symbol: String,
    feed: Arc<LiveFeed>,
    color: Color32,
    timeframe: Timeframe,
    values: IndicatorValues,
    visible: bool,
}

impl Comparison {
    pub fn new(symbol: String, feed: Arc<LiveFeed>, color: Color32) -> Self {
        Self {
            symbol,
            feed,
            color,
            timeframe: Timeframe::Hour1,
            values: IndicatorValues::new(),
            visible: true,
        }
    }
}

impl Indicator for Comparison {
    fn name(&self) -> &str {
        &self.symbol
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.timeframe = timeframe;
    }

    fn calculate(&mut self, data: &[Bar]) {
        let bars = self
            .feed
            .snapshot(self.timeframe, BarClock::new(self.timeframe), true);
        let mut index = 0;
        self.values = data
            .iter()
            .map(|bar| {
                while index < bars.len() && bars[index].time < bar.time {
                    index += 1;
                }
                if let Some(other) = bars.get(index).filter(|other| other.time == bar.time) {
                    IndicatorValue::Single(other.close)
                } else {
                    IndicatorValue::None
                }
            })
            .collect::<Vec<_>>()
            .into();
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(color) = colors.first() {
            self.color = *color;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_aligns_history_and_live_prices_by_time() {
        let now = "2026-10-23T15:00:30Z".parse().unwrap();
        let feed = Arc::new(LiveFeed::new("AAPL", now));
        let mut study = Comparison::new("AAPL".into(), feed.clone(), Color32::BLUE);
        for timeframe in [Timeframe::Min1, Timeframe::Hour1] {
            study.set_timeframe(timeframe);
            let clock = BarClock::new(timeframe);
            let source = feed.snapshot(timeframe, clock.clone(), true);
            let mut missing = source[0].clone();
            missing.time -= chrono::TimeDelta::days(1);
            let mut main = vec![
                missing,
                source[source.len() - 3].clone(),
                source.last().unwrap().clone(),
            ];
            study.calculate(&main);
            assert_eq!(study.values().value(0, 0), None);
            assert_eq!(study.values().value(0, 1), Some(main[1].close));
            assert_eq!(study.values().value(0, 2), Some(main[2].close));

            feed.advance(now + chrono::TimeDelta::milliseconds(timeframe.duration_ms()));
            let live = feed.snapshot(timeframe, clock, true).pop().unwrap();
            assert!(live.time > main.last().unwrap().time);
            main.push(live.clone());
            study.calculate(&main);
            assert_eq!(study.values().value(0, 3), Some(live.close));
        }
    }
}
