//! Alert levels for drawings whose lines depend on three anchors or a fitted bar range.

use super::{Drawing, DrawingToolType, PitchforkConfig};
use crate::model::{Bar, math::Moments};
use crate::scales::PriceScaleMode;

impl Drawing {
    /// [`Self::alert_levels`] with the main-series bars needed by a regression drawing. The
    /// fitted median is level 0, upper band 1 and lower band 2. Regression uses the same closes,
    /// residual deviation and axis interpolation between fitted endpoints as its painted lines.
    /// Empty when the anchored range has fewer than two bars or invalid geometry.
    pub fn alert_levels_with_bars(
        &self,
        bar: f64,
        scale: PriceScaleMode,
        bars: &[Bar],
    ) -> Vec<(usize, String, f64)> {
        if self.tool_type != DrawingToolType::RegressionTrend {
            return self.alert_levels(bar, scale);
        }
        let &[a, b, ..] = self.chart_points.as_slice() else {
            return Vec::new();
        };
        if !bar.is_finite() || !a.bar_idx.is_finite() || !b.bar_idx.is_finite() {
            return Vec::new();
        }
        let Some((first, bars)) =
            crate::drawings::rendering::bars_between(bars, a.bar_idx, b.bar_idx)
        else {
            return Vec::new();
        };
        let Some(fit) = Moments::of(bars).fit() else {
            return Vec::new();
        };
        if !fit.intercept.is_finite() || !fit.slope.is_finite() || !fit.sigma.is_finite() {
            return Vec::new();
        }
        let deviations = self.regression_trend.unwrap_or_default().deviations();
        let last = (bars.len() - 1) as f64;
        [
            ("Median", 0.0),
            ("Upper", deviations),
            ("Lower", -deviations),
        ]
        .into_iter()
        .enumerate()
        .filter_map(|(level, (name, k))| {
            let start = fit.intercept + k * fit.sigma;
            let end = start + fit.slope * last;
            axis_line(
                [first as f64, start],
                [first as f64 + last, end],
                bar,
                scale,
            )
            .map(|price| (level, name.to_owned(), price))
        })
        .collect()
    }

    pub(super) fn fork_alert_levels(
        &self,
        bar: f64,
        scale: PriceScaleMode,
    ) -> Vec<(usize, String, f64)> {
        let &[a, b, c, ..] = self.chart_points.as_slice() else {
            return Vec::new();
        };
        let log = scale == PriceScaleMode::Logarithmic;
        let points = [a, b, c].map(|p| {
            [
                f64::from(p.bar_idx),
                if log { p.price.ln() } else { p.price },
            ]
        });
        if !bar.is_finite() || points.iter().flatten().any(|v| !v.is_finite()) {
            return Vec::new();
        }
        let [a, b, c] = points;
        let between =
            |a: [f64; 2], b: [f64; 2], t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        let middle = between(b, c, 0.5);
        let origin = match self.tool_type {
            DrawingToolType::SchiffPitchfork => [a[0], (a[1] + b[1]) / 2.0],
            DrawingToolType::ModifiedSchiffPitchfork => between(a, b, 0.5),
            DrawingToolType::InsidePitchfork => middle,
            _ => a,
        };
        let direction = if self.tool_type == DrawingToolType::InsidePitchfork {
            let mid = between(a, b, 0.5);
            [c[0] - mid[0], c[1] - mid[1]]
        } else {
            [middle[0] - origin[0], middle[1] - origin[1]]
        };
        let config = self
            .pitchfork
            .clone()
            .unwrap_or_else(PitchforkConfig::legacy);
        let fan = self.tool_type == DrawingToolType::Pitchfan;
        let mut levels = Vec::new();
        for (index, level) in config.levels.iter().enumerate().filter(|(_, l)| {
            l.visible && (0.0..=crate::drawings::validation::MAX_PITCHFORK_LEVEL).contains(&l.value)
        }) {
            for (side, anchor) in
                [b, c]
                    .into_iter()
                    .enumerate()
                    .take(if level.value == 0.0 { 1 } else { 2 })
            {
                let value = f64::from(level.value);
                let (start, end) = if fan {
                    (origin, between(middle, anchor, value))
                } else {
                    let start = between(origin, anchor, value);
                    (start, [start[0] + direction[0], start[1] + direction[1]])
                };
                let dx = end[0] - start[0];
                let back = !fan
                    && if dx < 0.0 {
                        self.extend_right
                    } else {
                        self.extend_left
                    };
                if dx == 0.0 || (!back && (bar - start[0]) * dx < 0.0) {
                    continue;
                }
                let axis = start[1] + (bar - start[0]) * (end[1] - start[1]) / dx;
                let price = if log { axis.exp() } else { axis };
                if price.is_finite() && (!log || price > 0.0) {
                    let name = if level.value == 0.0 {
                        "Median".to_owned()
                    } else {
                        format!(
                            "{}: {} line",
                            level.value,
                            if side == 0 { "first" } else { "second" }
                        )
                    };
                    levels.push((2 * index + side, name, price));
                }
            }
        }
        levels
    }
}

/// A straight painted segment in axis coordinates, extended to the requested bar.
fn axis_line(a: [f64; 2], b: [f64; 2], bar: f64, scale: PriceScaleMode) -> Option<f64> {
    let log = scale == PriceScaleMode::Logarithmic;
    let [from, to] = [a[1], b[1]].map(|p| if log { p.ln() } else { p });
    if !from.is_finite() || !to.is_finite() || a[0] == b[0] {
        return None;
    }
    let value = from + (bar - a[0]) * (to - from) / (b[0] - a[0]);
    let price = if log { value.exp() } else { value };
    (price.is_finite() && (!log || price > 0.0)).then_some(price)
}
