//! Drawing alert levels, ray geometry and names, including levels fitted to a bar range.

use super::{Drawing, DrawingToolType, PitchforkConfig};
use crate::model::{Bar, math::Moments};
use crate::scales::PriceScaleMode;

impl Drawing {
    /// [`Self::alert_levels`] with the main-series bars needed by a regression drawing or an
    /// anchored VWAP. The fitted median is level 0, upper band 1 and lower band 2. Regression
    /// uses the same source prices, bounds and axis interpolation between fitted endpoints as
    /// its painted lines, and is empty when the anchored range has fewer than two bars or
    /// invalid geometry. The anchored VWAP is level 0 and the upper and lower lines of each
    /// enabled band `n` (from zero) levels `2n + 1` and `2n + 2`, at the values the painted lines
    /// have at `bar`; they are empty before the anchor, past the last bar, before volume trades
    /// and at a bar whose source price or volume is not finite. An anchored VWAP set to count
    /// executions counts every bar at its source price here;
    /// [`Chart::alert_value`](crate::Chart::alert_value) reads the chart's executions.
    pub fn alert_levels_with_bars(
        &self,
        bar: f64,
        scale: PriceScaleMode,
        bars: &[Bar],
    ) -> Vec<(usize, String, f64)> {
        self.alert_levels_with_executions(bar, scale, bars, None)
    }

    /// [`Self::alert_levels_with_bars`], with an anchored VWAP reading the supplied executions
    /// when its settings count them.
    pub(crate) fn alert_levels_with_executions(
        &self,
        bar: f64,
        scale: PriceScaleMode,
        bars: &[Bar],
        executions: Option<crate::data::FootprintView<'_>>,
    ) -> Vec<(usize, String, f64)> {
        if self.tool_type == DrawingToolType::AnchoredVWAP {
            return self.vwap_alert_levels(bar, bars, executions);
        }
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
        let config = self.regression_trend.unwrap_or_default();
        let Some(fit) = Moments::of(bars, config.source).fit() else {
            return Vec::new();
        };
        if !fit.intercept.is_finite() || !fit.slope.is_finite() || !fit.sigma.is_finite() {
            return Vec::new();
        }
        let [above, below] = fit.bounds(bars, config.band_deviations());
        let last = (bars.len() - 1) as f64;
        [("Median", 0.0), ("Upper", above), ("Lower", -below)]
            .into_iter()
            .enumerate()
            .filter_map(|(level, (name, offset))| {
                let start = fit.intercept + offset;
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

    fn vwap_alert_levels(
        &self,
        bar: f64,
        bars: &[Bar],
        executions: Option<crate::data::FootprintView<'_>>,
    ) -> Vec<(usize, String, f64)> {
        let Some(anchor) = self.chart_points.first() else {
            return Vec::new();
        };
        // Painted lines start at the rounded anchor and have a value on each loaded bar.
        let bar = bar.round();
        if !(f64::from(anchor.bar_idx).round() <= bar && bar < bars.len() as f64) {
            return Vec::new();
        }
        let Some((_, span)) =
            crate::drawings::rendering::bars_between(bars, anchor.bar_idx, bar as f32)
        else {
            return Vec::new();
        };
        let config = self.vwap.unwrap_or_default();
        let Some((vwap, unit)) =
            crate::drawings::rendering::anchored_vwap_through(span, &config, executions)
        else {
            return Vec::new();
        };
        let bands = config
            .bands
            .iter()
            .enumerate()
            .filter(|(_, band)| band.enabled);
        std::iter::once((0, "VWAP".to_owned(), vwap))
            .chain(bands.flat_map(|(index, band)| {
                let offset = band.multiplier() * unit;
                [
                    (
                        2 * index + 1,
                        format!("Upper band {}", index + 1),
                        vwap + offset,
                    ),
                    (
                        2 * index + 2,
                        format!("Lower band {}", index + 1),
                        vwap - offset,
                    ),
                ]
            }))
            .filter(|(.., price)| price.is_finite())
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
                if !fan && !config.extend_lines && (bar - start[0]) * dx > dx * dx {
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

    /// The rays of a speed resistance fan at `bar`, numbered and named as [`Self::alert_levels`].
    pub(super) fn speed_fan_alert_levels(
        &self,
        bar: f64,
        scale: PriceScaleMode,
    ) -> Vec<(usize, String, f64)> {
        let &[a, b, ..] = self.chart_points.as_slice() else {
            return Vec::new();
        };
        let config = self.fibonacci_config();
        let shared = config.time_levels.is_none();
        // Separate price and time settings reverse the fan by mirroring its anchor prices.
        let (start, end) = if config.reverse && !shared {
            ((a.bar_idx, b.price), (b.bar_idx, a.price))
        } else {
            ((a.bar_idx, a.price), (b.bar_idx, b.price))
        };
        let log = scale == PriceScaleMode::Logarithmic;
        let axis = |price: f64| if log { price.ln() } else { price };
        let (x0, y0) = (f64::from(start.0), axis(start.1));
        let (run, rise) = (f64::from(end.0) - x0, axis(end.1) - y0);
        if !bar.is_finite() || [x0, y0, run, rise].iter().any(|v| !v.is_finite()) {
            return Vec::new();
        }
        let ray = |dx: f64, dy: f64| {
            if dx == 0.0 || (bar - x0) * dx < 0.0 {
                return None;
            }
            let value = y0 + (bar - x0) * dy / dx;
            let price = if log { value.exp() } else { value };
            (price.is_finite() && (!log || price > 0.0)).then_some(price)
        };
        let price_rays = config
            .levels
            .iter()
            .enumerate()
            .filter_map(|(index, level)| {
                if !level.visible || !level.value.is_finite() {
                    return None;
                }
                let price = if level.value == 1.0 && run == 0.0 {
                    ray(1.0, 0.0)
                } else {
                    ray(run, rise * (1.0 - f64::from(level.value)))
                }?;
                Some((
                    2 * index,
                    format!("{}: price level", level.label(config.levels_as_percent)),
                    price,
                ))
            });
        let time_rays = config
            .fan_time_levels()
            .iter()
            .enumerate()
            .filter_map(|(index, level)| {
                // The shared table's time ray at zero duplicates its price ray and is not painted.
                if !level.visible || !level.value.is_finite() || (shared && level.value == 0.0) {
                    return None;
                }
                let price = ray(run * (1.0 - f64::from(level.value)), rise)?;
                Some((
                    2 * index + 1,
                    format!("{}: time level", level.label(config.levels_as_percent)),
                    price,
                ))
            });
        let mut levels: Vec<_> = price_rays.chain(time_rays).collect();
        levels.sort_by_key(|(level, ..)| *level);
        levels
    }

    /// The Gann fan rays at `bar`, numbered and named as [`Self::alert_levels`].
    pub(super) fn gann_fan_alert_levels(
        &self,
        bar: f64,
        scale: PriceScaleMode,
    ) -> Vec<(usize, String, f64)> {
        let &[a, b, ..] = self.chart_points.as_slice() else {
            return Vec::new();
        };
        let log = scale == PriceScaleMode::Logarithmic;
        let axis = |price: f64| if log { price.ln() } else { price };
        let (x0, y0) = (f64::from(a.bar_idx), axis(a.price));
        let (run, rise) = (f64::from(b.bar_idx) - x0, axis(b.price) - y0);
        // The renderer's fallback angle for anchors on one bar has no price slope.
        if !bar.is_finite()
            || [x0, y0, run, rise].iter().any(|v| !v.is_finite())
            || run == 0.0
            || (bar - x0) * run < 0.0
        {
            return Vec::new();
        }
        self.gann_config()
            .fan_ratios
            .iter()
            .enumerate()
            .filter(|(_, level)| level.visible && level.value.is_finite() && level.value > 0.0)
            .filter_map(|(index, level)| {
                let value = y0 + (bar - x0) * rise / run * f64::from(level.value);
                let price = if log { value.exp() } else { value };
                (price.is_finite() && (!log || price > 0.0)).then(|| {
                    (
                        index,
                        crate::drawings::rendering::gann_ratio_label(level.value),
                        price,
                    )
                })
            })
            .collect()
    }

    /// The alert source name template of level `level` of [`Self::alert_levels`] and the text
    /// that fills `{value}`: a fork setting or a speed fan ratio as its drawing labels it.
    /// `None` for a fork median and other drawings. With `{tool}` and `{id}` filled, its English
    /// keeps the drawing and level name that earlier alerts and presets saved.
    pub(crate) fn level_line(&self, level: usize) -> Option<(&'static str, String)> {
        if self.tool_type == DrawingToolType::FibonacciSpeedFan {
            let config = self.fibonacci_config();
            let levels = if level.is_multiple_of(2) {
                &config.levels[..]
            } else {
                config.fan_time_levels()
            };
            let label = levels.get(level / 2)?.label(config.levels_as_percent);
            return Some((
                [
                    "{tool} {id}: {value}: price level",
                    "{tool} {id}: {value}: time level",
                ][level % 2],
                label,
            ));
        }
        if !is_fork(self.tool_type) {
            return None;
        }
        let config = self
            .pitchfork
            .clone()
            .unwrap_or_else(PitchforkConfig::legacy);
        let value = config.levels.get(level / 2)?.value;
        (value != 0.0).then(|| {
            (
                [
                    "{tool} {id}: {value}: first line",
                    "{tool} {id}: {value}: second line",
                ][level % 2],
                value.to_string(),
            )
        })
    }
}

/// Whether [`Drawing::fork_alert_levels`] gives the alert levels of `tool`.
pub(super) fn is_fork(tool: DrawingToolType) -> bool {
    use DrawingToolType as T;
    matches!(
        tool,
        T::Pitchfork
            | T::SchiffPitchfork
            | T::ModifiedSchiffPitchfork
            | T::InsidePitchfork
            | T::Pitchfan
    )
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
