use crate::model::math::Moments;
use crate::model::{Bar, PriceSource};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Trend Strength Index: the Pearson correlation of the source price with bar position over
/// the last `period` bars, from -1 for a straight fall to 1 for a straight rise.
///
/// A bar has no value until `period` bars exist, when the window holds a non-finite price, or
/// when the prices of the window are all equal, as the correlation is then undefined. A bar
/// with a non-finite price still takes its place in the window, so the correlation is always
/// taken against true bar positions. The latest value equals
/// [`LinearRegressionChannel::correlation`](super::LinearRegressionChannel::correlation) of
/// the same period.
///
/// Values below zero are drawn in the plot's Below zero colour, the down colour by default, and
/// other values in the plot colour, the up colour by default. The pane draws a zero guide and
/// guides at 1 and -1 in the up and down colours, and shades between the line and zero from
/// transparent at 0 to the up colour at 10% opacity at 1 and the down colour at -1. Values read
/// to two decimals.
#[derive(Clone, Debug)]
pub struct TrendStrengthIndex {
    period: usize,
    source: PriceSource,
    values: IndicatorValues,
    stepper: Stepper<VecDeque<Bar>>,
    /// Colour of values at or above zero, and of the plot.
    positive_color: Color32,
    /// Colour of values below zero.
    negative_color: Color32,
    visible: bool,
}

impl TrendStrengthIndex {
    /// Create with a window of `period` bars, clamped to at least 2.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            source: PriceSource::Close,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            positive_color: palette::UP,
            negative_color: palette::DOWN,
            visible: true,
        }
    }

    /// Select the input price. Defaults to close.
    pub fn with_source(mut self, source: PriceSource) -> Self {
        self.source = source;
        self
    }

    /// Draw every value in `color`.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.positive_color = color;
        self.negative_color = color;
        self
    }
}

impl Default for TrendStrengthIndex {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for TrendStrengthIndex {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(self.source))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 2, usize::MAX),
        source: choice("Source", PriceSource::all())
    ; {});

    fn name(&self) -> &str {
        "Trend Strength Index"
    }

    fn desc(&self) -> &str {
        "Trend Strength Index - Rolling correlation of price with bar position (-1 to 1)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, source) = (self.period, self.source);
        self.stepper
            .run(data, from, &mut self.values, |window, bar| {
                let price = source.compute(bar.open, bar.high, bar.low, bar.close);
                window.push_back(Bar::new(bar.time, price, price, price, price, 0.0));
                if window.len() > period {
                    window.pop_front();
                }
                if window.len() < period || window.iter().any(|bar| !bar.close.is_finite()) {
                    return IndicatorValue::None;
                }
                Moments::of(window.make_contiguous(), PriceSource::Close)
                    .fit()
                    .and_then(|fit| fit.correlation)
                    .map_or(IndicatorValue::None, IndicatorValue::Single)
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.positive_color, self.negative_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.positive_color = color;
        }
        if let Some(&color) = colors.get(1) {
            self.negative_color = color;
        }
    }

    fn plot_variants(&self, _line: usize) -> Vec<crate::studies::PlotVariant> {
        vec![crate::studies::PlotVariant::new(
            "Below zero",
            self.negative_color,
        )]
    }

    fn plot_variant(&self, _line: usize, bar: usize) -> Option<usize> {
        (self.values.value(0, bar)? < 0.0).then_some(0)
    }

    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        self.plot_variant(line, bar).map(|_| self.negative_color)
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Fixed(2)
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            // A margin keeps the guides at 1 and -1 inside the pane.
            range: Some((-1.1, 1.1)),
            levels: vec![
                (0.0, palette::GRAY.gamma_multiply_u8(128), "Zero"),
                (1.0, palette::UP, "Bullish Band"),
                (-1.0, palette::DOWN, "Bearish Band"),
            ],
            ..Default::default()
        }
    }

    fn gradients(&self) -> Vec<crate::studies::StudyGradient> {
        [
            (
                "Bullish shading",
                [1.0, 0.0],
                [palette::UP.gamma_multiply_u8(26), Color32::TRANSPARENT],
            ),
            (
                "Bearish shading",
                [0.0, -1.0],
                [Color32::TRANSPARENT, palette::DOWN.gamma_multiply_u8(26)],
            ),
        ]
        .into_iter()
        .map(|(name, range, colors)| crate::studies::StudyGradient {
            name,
            plot: 0,
            baseline: 0.0,
            range,
            colors,
        })
        .collect()
    }

    fn is_overlay(&self) -> bool {
        false
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
        vec![format!("Trend Strength({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn bars(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                Bar::new(
                    Utc.timestamp_opt(i as i64 * 60, 0).unwrap(),
                    c,
                    c,
                    c,
                    c,
                    1.0,
                )
            })
            .collect()
    }

    fn rolling(closes: &[f64], period: usize) -> Vec<Option<f64>> {
        let mut tsi = TrendStrengthIndex::new(period);
        tsi.calculate(&bars(closes));
        tsi.values()
            .to_rows()
            .into_iter()
            .map(|v| match v {
                IndicatorValue::Single(v) => Some(v),
                _ => None,
            })
            .collect()
    }

    /// Period 3 over 1, 2, 4, 3, 3, 3 against positions 0, 1, 2, from the centred sums
    /// r = sxy / sqrt(sxx * syy), with sxx = 2: 1, 2, 4 has sxy 3 and syy 14/3; 2, 4, 3 has
    /// sxy 1 and syy 2; 4, 3, 3 has sxy -1 and syy 2/3; 3, 3, 3 is flat. A straight fall is -1.
    #[test]
    fn rolling_correlation_matches_hand_values() {
        let got = rolling(&[1.0, 2.0, 4.0, 3.0, 3.0, 3.0], 3);
        let want = [
            None,
            None,
            Some(3.0 / (2.0 * 14.0 / 3.0_f64).sqrt()),
            Some(1.0 / (2.0 * 2.0_f64).sqrt()),
            Some(-1.0 / (2.0 * 2.0 / 3.0_f64).sqrt()),
            None,
        ];
        for (got, want) in got.iter().zip(want) {
            match (got, want) {
                (Some(got), Some(want)) => assert!((got - want).abs() < 1e-12, "{got} {want}"),
                _ => assert_eq!(*got, want),
            }
        }
        assert_eq!(rolling(&[5.0, 4.0, 3.0, 2.0], 4)[3], Some(-1.0));
    }

    /// Values below zero take the plot's Below zero colour and other values the plot colour,
    /// through an edit, undo of a later change and a template; a template saved with one colour
    /// draws every value in it. Period 3 over 1, 2, 3, 2, 1 gives no value, no value, 1, 0, -1.
    #[test]
    fn values_are_coloured_by_their_sign() {
        use crate::studies::{ConfiguredStudy, StudyTemplate};
        let data = bars(&[1.0, 2.0, 3.0, 2.0, 1.0]);
        let colours = |study: &dyn Indicator| -> Vec<Color32> {
            (0..data.len())
                .map(|bar| {
                    study
                        .plot_color(0, bar)
                        .unwrap_or(study.plot_style(0).color)
                })
                .collect()
        };
        let (up, down) = (palette::UP, palette::DOWN);
        let (gold, blue) = (Color32::GOLD, Color32::BLUE);
        let mut study = ConfiguredStudy::new(Box::new(TrendStrengthIndex::new(3)));
        study.calculate(&data);
        assert_eq!(colours(&study), [up, up, up, up, down]);
        let mut settings = study.settings();
        settings.plots[0].color = gold;
        settings.plot_variants[0][0].color = blue;
        study.apply_settings(&[], settings).unwrap();
        study.calculate(&data);
        let edited = [gold, gold, gold, gold, blue];
        assert_eq!(colours(&study), edited);
        let saved = study.snapshot_settings();
        study.set_colors(vec![up, down]);
        study.restore_settings(&saved).unwrap();
        study.calculate(&data);
        assert_eq!(colours(&study), edited);
        let mut restored = StudyTemplate::from_study(&study).create().unwrap();
        restored.calculate(&data);
        assert_eq!(colours(restored.as_ref()), edited);
        // Saved in gold before the study had a Below zero colour.
        let saved = r#"{"name":"Trend Strength Index","inputs":[["period",{"Length":3}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TrendStrengthIndex.html","plots":[{"color":[255,215,0,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":3,"hidden_intervals":[],"interval_range":"All","levels":[{"value":null,"color":[60,62,67,128],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":false,"level_fill_color":[16,13,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;
        let mut restored = serde_json::from_str::<StudyTemplate>(saved)
            .unwrap()
            .create()
            .unwrap();
        restored.calculate(&data);
        assert_eq!(colours(restored.as_ref()), [gold; 5]);
    }

    /// A new study shades both sides of zero; a save from before the band guides and shading keeps
    /// its zero guide and three decimals, with the new guides and shading hidden.
    #[test]
    fn older_saves_keep_their_zero_guide_decimals_and_no_shading() {
        use crate::studies::{ConfiguredStudy, LevelStyle, StudyTemplate};
        let ramps = |study: &dyn Indicator| -> Vec<_> {
            study
                .gradients()
                .iter()
                .map(|fill| (fill.name, fill.plot, fill.baseline, fill.range, fill.colors))
                .collect()
        };
        let (bull, bear) = (
            palette::UP.gamma_multiply_u8(26),
            palette::DOWN.gamma_multiply_u8(26),
        );
        let clear = Color32::TRANSPARENT;
        let study = ConfiguredStudy::new(Box::new(TrendStrengthIndex::default()));
        assert_eq!(
            ramps(&study),
            [
                ("Bullish shading", 0, 0.0, [1.0, 0.0], [bull, clear]),
                ("Bearish shading", 0, 0.0, [0.0, -1.0], [clear, bear]),
            ]
        );
        assert!(study.settings().gradients.iter().all(|fill| fill.visible));
        // Saved in gold before the study had a Below zero colour, band guides or shading.
        let saved = r#"{"name":"Trend Strength Index","inputs":[["period",{"Length":3}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.TrendStrengthIndex.html","plots":[{"color":[255,215,0,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":3,"hidden_intervals":[],"interval_range":"All","levels":[{"value":null,"color":[60,62,67,128],"pattern":"Dashed","width":1.0,"visible":true,"price_line":false}],"fill_between_levels":false,"level_fill_color":[16,13,0,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Independent"}"#;
        let template = serde_json::from_str::<StudyTemplate>(saved).unwrap();
        let declared = study.settings().levels;
        let hidden = |level: LevelStyle| LevelStyle {
            visible: false,
            ..level
        };
        let restored = template.create().unwrap();
        assert_eq!(
            restored.settings().levels,
            [
                template.settings.levels[0],
                hidden(declared[1]),
                hidden(declared[2])
            ]
        );
        assert!(
            restored
                .settings()
                .gradients
                .iter()
                .all(|fill| !fill.visible)
        );
        assert!(
            restored
                .gradients()
                .iter()
                .all(|fill| fill.colors == [clear; 2])
        );
        assert_eq!(restored.precision(), Some(3));
        // It keeps its range until a band guide is shown, which then opens a margin around it.
        assert_eq!(study.pane().range, Some((-1.1, 1.1)));
        assert_eq!(restored.pane().range, Some((-1.0, 1.0)));
        let mut restored = StudyTemplate::from_study(restored.as_ref())
            .create()
            .unwrap();
        assert_eq!(restored.pane().range, Some((-1.0, 1.0)));
        let mut shown = restored.settings();
        shown.levels[2].visible = true;
        restored.apply_settings(&[], shown).unwrap();
        assert_eq!(restored.pane().range, Some((-1.1, 1.1)));
        let unset = saved.replace(r#""precision":3"#, r#""precision":null"#);
        let restored = serde_json::from_str::<StudyTemplate>(&unset)
            .unwrap()
            .create()
            .unwrap();
        assert_eq!(restored.precision(), Some(3));
    }

    /// A missing price blanks every window that holds it and keeps later bar positions true.
    #[test]
    fn missing_price_blanks_its_windows() {
        let got = rolling(&[1.0, 2.0, f64::NAN, 4.0, 5.0, 6.0], 3);
        assert_eq!(got[..5], [None; 5]);
        assert!((got[5].unwrap() - 1.0).abs() < 1e-12);
    }

    /// A new study's pane draws its guides at 1 and -1 whole inside the pane, in both themes.
    #[test]
    fn band_guides_paint_inside_the_pane() {
        use crate::widget::test_frames::{chart, run};
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = egui::Context::default();
            // Closes that cycle keep the line near zero, away from the guides.
            let mut chart = chart(&[100.0, 108.0, 104.0], &[110.0, 115.0, 113.0]);
            chart.set_theme(&ctx, &theme);
            chart.add_indicator(Box::new(TrendStrengthIndex::default()));
            run(&ctx, &mut chart, 0.0, Vec::new());
            let (shapes, _) = run(&ctx, &mut chart, 0.1, Vec::new());
            let pane = chart.get_rendered_indicator_panes()[0].clone();
            let rect = pane.chart_rect;
            for (value, color) in [(1.0, palette::UP), (-1.0, palette::DOWN)] {
                let y = rect.bottom()
                    - ((value - pane.y_min) / (pane.y_max - pane.y_min)) as f32 * rect.height();
                let dashes: Vec<_> = shapes
                    .iter()
                    .filter_map(|(clip, shape)| match shape {
                        egui::Shape::LineSegment { points, stroke }
                            if stroke.color == color
                                && points[0].y == points[1].y
                                && (points[0].y - y).abs() < 0.5 =>
                        {
                            Some((*clip, points[0].y, stroke.width))
                        }
                        _ => None,
                    })
                    .collect();
                assert!(!dashes.is_empty(), "{value}");
                for (clip, y, width) in dashes {
                    assert!(
                        y - width / 2.0 >= clip.top() && y + width / 2.0 <= clip.bottom(),
                        "{value}: {y} in {clip:?}"
                    );
                }
            }
        }
    }
}
