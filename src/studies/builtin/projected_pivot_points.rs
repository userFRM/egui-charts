//! Projected Pivot Points.
//!
//! The pivot levels the next period will use if the current one ended now. The confirmed levels
//! are those of [`PivotPoints`](super::PivotPoints): lines, constant within a period, from the
//! previous period. The projected levels are drawn beyond the last bar, from the period so far.
//!
//! # Construction
//!
//! With H, L and C the high, low and latest close of the period so far, the projected levels are
//! those of the chosen [`PivotMethod`] for a previous period of high H, low L and close C, and,
//! for the methods that use the open of the previous period, its open. The methods that weight
//! the open of the period being projected (Woodie) take C as its open, the latest price known.
//! Each projected level is a box from the last bar as wide as the period so far has bars, so the
//! projection is state-distinct from the confirmed lines: it moves with every bar until the
//! period ends, and its levels become the confirmed lines of the next period. A level the
//! method does not define is absent. Overlay indicator with the inputs of Pivot Points Standard.
//!
//! # Example
//!
//! ```rust
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, PivotTimeframe, ProjectedPivotPoints};
//!
//! // Two hourly bars of one day: P = (12 + 8 + 11) / 3 for the next day.
//! let bars: Vec<Bar> = [(10.0, 8.0, 9.0), (12.0, 9.0, 11.0)]
//!     .into_iter()
//!     .enumerate()
//!     .map(|(i, (high, low, close))| {
//!         let time = Utc.with_ymd_and_hms(2026, 3, 2, 9 + i as u32, 0, 0).unwrap();
//!         Bar::new(time, close, high, low, close, 1.0)
//!     })
//!     .collect();
//! let mut pivots = ProjectedPivotPoints::new().with_timeframe(PivotTimeframe::Daily);
//! pivots.calculate(&bars);
//! assert!((pivots.projections()[0].top - 31.0 / 3.0).abs() < 1e-12);
//! ```

use super::pivot_points::{PivotMethod, PivotPoints, PivotTimeframe};
use crate::model::{Bar, Instrument, Timeframe};
use crate::studies::{Indicator, IndicatorValues, StudyProjection};
use egui::Color32;

/// Projected Pivot Points: the next period's pivot levels from the period so far.
#[derive(Clone)]
pub struct ProjectedPivotPoints {
    pivots: PivotPoints,
    projections: Vec<StudyProjection>,
}

impl ProjectedPivotPoints {
    /// Traditional pivots, timeframe Auto, for the last 15 periods.
    pub fn new() -> Self {
        Self {
            pivots: PivotPoints::new(),
            projections: Vec::new(),
        }
    }

    /// The level formulas.
    pub fn with_method(mut self, method: PivotMethod) -> Self {
        self.pivots = self.pivots.with_method(method);
        self
    }

    /// The calendar period of each set of pivots.
    pub fn with_timeframe(mut self, timeframe: PivotTimeframe) -> Self {
        self.pivots = self.pivots.with_timeframe(timeframe);
        self
    }

    /// The number of most recent periods that show confirmed pivots, raised to 1 or more.
    pub fn with_pivots_back(mut self, pivots_back: usize) -> Self {
        self.pivots = self.pivots.with_pivots_back(pivots_back);
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for ProjectedPivotPoints {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for ProjectedPivotPoints {
    fn inputs(&self) -> Vec<crate::studies::StudyInput> {
        self.pivots.inputs()
    }

    fn set_input(
        &mut self,
        id: &str,
        value: crate::studies::InputValue,
    ) -> Result<(), &'static str> {
        self.pivots.set_input(id, value)?;
        self.projections.clear();
        Ok(())
    }

    fn name(&self) -> &str {
        "Projected Pivot Points"
    }

    fn desc(&self) -> &str {
        "Projected Pivot Points - Next period's pivot levels from the period so far"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.pivots.update(data, from);
        self.projections.clear();
        if let Some((method, period, start)) = self.pivots.latest_period() {
            let last = data.len() - 1;
            let width = (data.len() - start) as f64;
            let levels = PivotPoints::levels(method, period, period.close);
            self.projections.extend(
                levels
                    .into_iter()
                    .enumerate()
                    .filter(|(_, price)| price.is_finite())
                    .map(|(plot, price)| StudyProjection {
                        bars: last as f64..last as f64 + width,
                        top: price,
                        bottom: price,
                        plot,
                    }),
            );
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.pivots.set_timeframe(timeframe);
    }

    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        self.pivots.set_instrument(instrument)
    }

    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        self.pivots.plot_color(line, bar)
    }

    fn projections(&self) -> &[StudyProjection] {
        &self.projections
    }

    fn values(&self) -> &IndicatorValues {
        self.pivots.values()
    }

    fn colors(&self) -> Vec<Color32> {
        self.pivots.colors()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        self.pivots.set_colors(colors);
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn is_visible(&self) -> bool {
        self.pivots.is_visible()
    }

    fn set_visible(&mut self, visible: bool) {
        self.pivots.set_visible(visible);
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_names(&self) -> Vec<String> {
        self.pivots.line_names()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// Hourly bars of two days, daily periods. The first day has high 12, low 8 and close 11. By
    /// the third bar of the second day its high is 14, low 9 and close 13, so the projection is
    /// P = (14 + 9 + 13) / 3 = 12, R1 = 2P - L = 15 and S1 = 2P - H = 10, three bars wide from the
    /// last bar, while the confirmed lines hold the first day's P = 31/3.
    #[test]
    fn projects_from_the_period_so_far_beside_the_confirmed_levels() {
        let hour = |day, h| Utc.with_ymd_and_hms(2026, 3, day, h, 0, 0).unwrap();
        let data: Vec<Bar> = [
            (hour(2, 9), 10.0, 8.0, 9.0),
            (hour(2, 10), 12.0, 9.0, 11.0),
            (hour(3, 9), 11.0, 9.0, 10.0),
            (hour(3, 10), 14.0, 10.0, 12.0),
            (hour(3, 11), 13.0, 10.0, 13.0),
        ]
        .into_iter()
        .map(|(time, high, low, close)| Bar::new(time, close, high, low, close, 1.0))
        .collect();
        let mut pivots = ProjectedPivotPoints::new().with_timeframe(PivotTimeframe::Daily);
        pivots.calculate(&data);
        let projected: Vec<_> = pivots.projections()[..3]
            .iter()
            .map(|p| (p.bars.clone(), p.top, p.plot))
            .collect();
        assert_eq!(
            projected,
            [
                (4.0..7.0, 12.0, 0),
                (4.0..7.0, 15.0, 1),
                (4.0..7.0, 10.0, 2)
            ]
        );
        assert!((pivots.values().value(0, 4).unwrap() - 31.0 / 3.0).abs() < 1e-12);
        // A bar later moves the projection, the confirmed lines staying.
        let mut more = data.clone();
        more.push(Bar::new(hour(3, 12), 13.0, 15.0, 12.0, 13.0, 1.0));
        pivots.update(&more, 5);
        assert_eq!(pivots.projections()[0].bars, 5.0..9.0);
        assert!((pivots.projections()[0].top - (15.0 + 9.0 + 13.0) / 3.0).abs() < 1e-12);
    }
}
