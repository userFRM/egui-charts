//! SuperTrend Indicator
//!
//! SuperTrend is a trend-following indicator that uses ATR to calculate
//! dynamic support and resistance levels. It's useful for identifying
//! trend direction and potential entry/exit points.
//!
//! # Components
//! - Up Trend: the SuperTrend line while the trend is bullish (below price)
//! - Down Trend: the SuperTrend line while the trend is bearish (above price)
//! - Body Middle: (open + close) / 2 on each bar with a trend value; hidden, it bounds the background
//!
//! Each plotted bar has a value on exactly one of the two lines; the other is NaN.
//! The background between the trend line and the body middle is green in an up trend and red in a
//! down trend at 90 % transparency, with a gap at each trend change.
//!
//! The first bar with an ATR starts a down trend on the upper band. A down trend turns up when a
//! close rises above the upper band, and an up trend turns down when a close falls below the lower
//! band.
//! The first bar with a finite high, low and close has no value; with an ATR length of 1
//! its down trend still decides the second bar.
//!
//! # Interpretation
//! - SuperTrend below price (green): Bullish trend
//! - SuperTrend above price (red): Bearish trend
//! - Price crossing SuperTrend signals trend change
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{SuperTrend, Indicator};
//!
//! let mut st = SuperTrend::new(10, 3.0);
//! st.calculate(&bars);
//! ```

use super::clamp_param;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// SuperTrend indicator
#[derive(Clone, Debug)]
pub struct SuperTrend {
    atr_period: usize,
    multiplier: f64,
    values: IndicatorValues,
    stepper: Stepper<State>,
    bullish_color: Color32,
    bearish_color: Color32,
    middle_color: Color32,
    visible: bool,
}

#[derive(Clone, Debug)]
struct State {
    usable: usize,
    previous: f64,
    atr: f64,
    upper: f64,
    lower: f64,
    trend: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            usable: 0,
            previous: 0.0,
            atr: -0.0,
            upper: 0.0,
            lower: 0.0,
            trend: 0.0,
        }
    }
}

impl SuperTrend {
    /// Create a new SuperTrend indicator
    ///
    /// # Arguments
    /// * `atr_period` - ATR period (typically 10)
    /// * `multiplier` - ATR multiplier (typically 3.0), clamped into 0.01 to `f64::MAX`, NaN to 0.01
    pub fn new(atr_period: usize, multiplier: f64) -> Self {
        Self {
            atr_period: atr_period.max(1),
            multiplier: clamp_param(multiplier, 0.01, f64::MAX),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            bullish_color: palette::GREEN, // Green
            bearish_color: palette::RED,   // Red
            middle_color: palette::BLUE,
            visible: true,
        }
    }

    /// Set colors for bullish and bearish trends
    pub fn with_colors(mut self, bullish: Color32, bearish: Color32) -> Self {
        self.bullish_color = bullish;
        self.bearish_color = bearish;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for SuperTrend {
    fn default() -> Self {
        Self::new(10, 3.0)
    }
}

impl Indicator for SuperTrend {
    crate::studies::inputs::study_inputs!(self;
        atr_period: length("ATR Length", 1, usize::MAX),
        multiplier: number("Factor", 0.01, f64::MAX, 0.01)
    ; {});

    fn name(&self) -> &str {
        "SuperTrend"
    }

    fn desc(&self) -> &str {
        "SuperTrend - Trend following indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let period = self.atr_period;
        let atr_mult = 1.0 / period as f64;
        let multiplier = self.multiplier;
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.high.is_finite() || !bar.low.is_finite() || !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                let tr = if state.usable == 0 {
                    bar.high - bar.low
                } else {
                    ATR::true_range(bar, state.previous)
                };
                let previous = state.previous;
                state.previous = bar.close;
                state.usable += 1;
                if state.usable <= period {
                    state.atr += tr;
                    if state.usable < period {
                        return IndicatorValue::None;
                    }
                    state.atr /= period as f64;
                } else {
                    state.atr += atr_mult * (tr - state.atr);
                }
                let hl2 = (bar.high + bar.low) / 2.0;
                let upper = hl2 + multiplier * state.atr;
                let lower = hl2 - multiplier * state.atr;
                let bullish = if state.usable == period {
                    state.upper = upper;
                    state.lower = lower;
                    // A down trend until a close rises above the upper band, as published.
                    false
                } else {
                    let was_upper = state.trend == state.upper;
                    if upper < state.upper || previous > state.upper {
                        state.upper = upper;
                    }
                    if lower > state.lower || previous < state.lower {
                        state.lower = lower;
                    }
                    if was_upper {
                        bar.close > state.upper
                    } else {
                        bar.close.partial_cmp(&state.lower) != Some(std::cmp::Ordering::Less)
                    }
                };
                state.trend = if bullish { state.lower } else { state.upper };
                // The first bar has no previous close; its trend still seeds the next bar.
                if state.usable == 1 {
                    return IndicatorValue::None;
                }
                let middle = (bar.open + bar.close) / 2.0;
                if bullish {
                    IndicatorValue::Multiple(vec![state.trend, f64::NAN, middle])
                } else {
                    IndicatorValue::Multiple(vec![f64::NAN, state.trend, middle])
                }
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn alert_conditions(&self) -> Vec<String> {
        ["Trend turns up", "Trend turns down", "Trend change"]
            .map(str::to_owned)
            .into()
    }

    fn alert_condition(&self, condition: usize, bar: usize) -> Option<bool> {
        let up = |bar| {
            if self.values.value(0, bar).is_some() {
                Some(true)
            } else if self.values.value(1, bar).is_some() {
                Some(false)
            } else {
                None
            }
        };
        let now = up(bar)?;
        let before = up(bar.checked_sub(1)?)?;
        match condition {
            0 => Some(now && !before),
            1 => Some(!now && before),
            2 => Some(now != before),
            _ => None,
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.bullish_color, self.bearish_color, self.middle_color]
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        let middle = line == 2;
        crate::studies::PlotStyle {
            color: self.colors().get(line).copied().unwrap_or(Color32::WHITE),
            width: self.line_style(line).width,
            pattern: crate::model::LineStyle::Solid,
            // Each trend line holds values only while its trend lasts; the body middle bounds
            // the background.
            plot_type: if middle {
                crate::studies::PlotType::Line
            } else {
                crate::studies::PlotType::LineWithBreaks
            },
            visible: !middle,
        }
    }

    /// Shade each trend to the body middle at a tenth of full opacity, leaving a gap at reversals.
    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        use crate::studies::{FillBoundary, StudyFill};
        vec![
            StudyFill {
                first: FillBoundary::Plot(0),
                second: FillBoundary::Plot(2),
                color: palette::GREEN.gamma_multiply_u8(26),
                inverted_color: None,
            },
            StudyFill {
                first: FillBoundary::Plot(1),
                second: FillBoundary::Plot(2),
                color: palette::RED.gamma_multiply_u8(26),
                inverted_color: None,
            },
        ]
    }

    fn draws_only_plots(&self) -> bool {
        true
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.bullish_color = colors[0];
        }
        if colors.len() > 1 {
            self.bearish_color = colors[1];
        }
        if colors.len() > 2 {
            self.middle_color = colors[2];
        }
    }

    fn is_overlay(&self) -> bool {
        true // SuperTrend is drawn on main chart
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
        vec![
            "Up Trend".to_string(),
            "Down Trend".to_string(),
            "Body Middle".to_string(),
        ]
    }
}

#[cfg(all(test, feature = "widget"))]
mod tests {
    use super::*;
    use crate::chart::coords::ChartMapping;
    use crate::chart::rendering::{IndicatorRenderer, RenderContext};
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::studies::{ConfiguredStudy, FillStyle, StudyTemplate};
    use egui::{Context, Pos2, RawInput, Rect, Shape};

    #[test]
    fn supertrend_shades_to_the_body_middle_without_bridging_a_trend_change() {
        let bars: Vec<_> = [
            [100.0, 101.0, 99.0, 100.0],
            [100.0, 103.0, 101.0, 103.0],
            [103.0, 105.0, 103.0, 105.0],
            [105.0, 106.0, 104.0, 105.0],
            [104.0, 104.0, 96.0, 97.0],
            [97.0, 98.0, 96.0, 97.0],
        ]
        .into_iter()
        .enumerate()
        .map(|(i, [open, high, low, close])| {
            Bar::new(
                chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(i as i64),
                open,
                high,
                low,
                close,
                1.0,
            )
        })
        .collect();
        let green = palette::GREEN.gamma_multiply_u8(26);
        let red = palette::RED.gamma_multiply_u8(26);
        let recoloured = Color32::from_rgba_premultiplied(0, 0, 51, 51);
        for row in ["default", "recoloured", "older save"] {
            let mut study = ConfiguredStudy::new(Box::new(SuperTrend::new(1, 1.0)));
            if row == "recoloured" {
                let mut settings = study.settings();
                settings.plots[0].color = Color32::WHITE;
                settings.fills[0].color = recoloured;
                settings.fills[1].visible = false;
                study.apply_settings(&[], settings).unwrap();
            }
            let mut study: Box<dyn Indicator> = if row == "older save" {
                let mut saved = StudyTemplate::from_study(&study);
                saved.settings.plots.truncate(2);
                saved.settings.plot_variants.truncate(2);
                saved.settings.fills.clear();
                saved.create().unwrap()
            } else {
                Box::new(study)
            };
            study.calculate(&bars);
            let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(240.0, 200.0));
            let map = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(90.0, 110.0),
                0.0,
                200.0,
                false,
            );
            let coords = ChartMapping::new(rect, 30.0, 0, bars.len(), 0.0, map);
            let mut output = Context::default().run_ui(RawInput::default(), |ui| {
                IndicatorRenderer::render_study(
                    &RenderContext::new(ui.painter(), rect),
                    study.as_ref(),
                    &bars,
                    0,
                    &map,
                    &coords,
                );
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let bands = |color| {
                shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Mesh(mesh)
                            if mesh.vertices.iter().all(|vertex| vertex.color == color) =>
                        {
                            Some(
                                mesh.vertices
                                    .iter()
                                    .map(|vertex| vertex.pos)
                                    .collect::<Vec<_>>(),
                            )
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            let check = |color, expected: Vec<Vec<Pos2>>| {
                let actual = bands(color);
                assert_eq!(actual.len(), expected.len(), "{row}");
                for (actual, expected) in actual.iter().zip(expected) {
                    assert_eq!(actual.len(), expected.len(), "{row}");
                    for (actual, expected) in actual.iter().zip(expected) {
                        assert!(
                            actual.distance(expected) < 1e-4,
                            "{row}: {actual:?} != {expected:?}"
                        );
                    }
                }
            };
            // ATR(1) is the true range: 2, 3, 2, 2, 9, 2. The up trend holds 99, 102,
            // 103 on bars 1..=3; the down trend holds 106, 99 on bars 4..=5.
            // y(price) = 10 * (110 - price); body middles are 101.5, 104, 105, 100.5, 97.
            let point = |bar, y| Pos2::new(coords.idx_to_x(bar), y);
            let up = if row == "older save" {
                vec![]
            } else {
                vec![
                    vec![
                        point(1, 110.0),
                        point(2, 80.0),
                        point(2, 60.0),
                        point(1, 85.0),
                    ],
                    vec![
                        point(2, 80.0),
                        point(3, 70.0),
                        point(3, 50.0),
                        point(2, 60.0),
                    ],
                ]
            };
            check(
                if row == "recoloured" {
                    recoloured
                } else {
                    green
                },
                up,
            );
            let down = if row == "default" {
                vec![vec![
                    point(4, 40.0),
                    point(5, 110.0),
                    point(5, 130.0),
                    point(4, 95.0),
                ]]
            } else {
                vec![]
            };
            check(red, down);
            assert!(!shapes.iter().any(|shape| matches!(
                &shape.shape, Shape::Path(path)
                    if path.stroke.color == egui::epaint::ColorMode::Solid(palette::BLUE)
            )));
            assert!(!study.settings().plots[2].visible);
            if row == "older save" {
                assert_eq!(study.line_count(), 3);
                assert_eq!(
                    study.settings().plots[2],
                    SuperTrend::default().plot_style(2)
                );
                assert_eq!(
                    study.settings().fills,
                    [green, red].map(|color| FillStyle {
                        visible: false,
                        ..FillStyle::new(color)
                    })
                );
            }
        }
    }
}
