use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// True Strength Index (TSI), William Blau, "Momentum, Direction, and Divergence" (1995).
///
/// `TSI = 100 * EMA(EMA(m, long), short) / EMA(EMA(|m|, long), short)` with `m = close - previous close`,
/// plus a signal line, the EMA of the TSI. Every EMA starts at its first input, and the price changes start
/// at the first change, bar 1. Values start on bar `long + short`; earlier bars are `None`.
#[derive(Clone)]
pub struct TrueStrengthIndex {
    /// Long period (typically 25)
    long_period: usize,
    /// Short period (typically 13)
    short_period: usize,
    /// Signal line period (typically 7)
    signal_period: usize,
    values: IndicatorValues,
    stepper: Stepper<(usize, Option<f64>, [Smoothing; 5])>,
    /// Colors: [TSI, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl TrueStrengthIndex {
    /// Create with `long_period`, `short_period`, `signal_period` in bars, each clamped to at
    /// least one.
    pub fn new(long_period: usize, short_period: usize, signal_period: usize) -> Self {
        Self {
            long_period: long_period.max(1),
            short_period: short_period.max(1),
            signal_period: signal_period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::BLUE,   // Blue - TSI
                palette::ORANGE, // Signal
            ],
            visible: true,
        }
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Default for TrueStrengthIndex {
    fn default() -> Self {
        Self::new(25, 13, 7)
    }
}

impl Indicator for TrueStrengthIndex {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        long_period: length("Long period", 1, usize::MAX),
        short_period: length("Short period", 1, usize::MAX),
        signal_period: length("Signal period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "TSI"
    }

    fn desc(&self) -> &str {
        "True Strength Index - Double-smoothed momentum"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                state.0 += 1;
                let previous = state.1.replace(bar.close);
                let Some(previous) = previous else {
                    return IndicatorValue::None;
                };
                let change = bar.close - previous;
                let first_alpha = 2.0 / (self.long_period as f64 + 1.0);
                let second_alpha = 2.0 / (self.short_period as f64 + 1.0);
                let first = state.2[0].next(change, 1, first_alpha);
                let second = state.2[1].next(first, 1, second_alpha);
                let abs_first = state.2[2].next(change.abs(), 1, first_alpha);
                let abs_second = state.2[3].next(abs_first, 1, second_alpha);
                let tsi = if abs_second != 0.0 {
                    100.0 * second / abs_second
                } else {
                    0.0
                };
                let signal = state.2[4].next(tsi, 1, 2.0 / (self.signal_period as f64 + 1.0));
                if state.0
                    < self
                        .long_period
                        .saturating_add(self.short_period)
                        .saturating_add(1)
                {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Multiple(vec![tsi, signal])
                }
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 2 {
            self.colors = colors;
        }
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

    fn line_count(&self) -> usize {
        2
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("TSI({},{})", self.long_period, self.short_period),
            format!("Signal({})", self.signal_period),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: close,
            high: close,
            low: close,
            close,
            volume: 1000.0,
            colors: None,
            payload: None,
        }
    }

    /// Blau, "Momentum, Direction, and Divergence" (1995): TSI = 100 * EMA(EMA(m, r), s) / EMA(EMA(|m|, r), s),
    /// m = close - previous close, signal = EMA(TSI, q), each EMA seeded with its first input, the change on bar 1.
    /// Expected values computed independently from that definition.
    #[test]
    fn matches_blau_from_the_first_price_change() {
        let data: Vec<Bar> = (0..45)
            .map(|i| make_bar(100.0 + ((7 * i) % 11) as f64 * 0.5 + 0.25 * i as f64))
            .collect();
        let expected: [(usize, f64, f64); 37] = [
            (8, 7.733160885593845, 23.105328984649127),
            (9, 22.895446351093035, 23.00038766787108),
            (10, 13.559429204247552, 18.279908436059316),
            (11, -3.963663540739121, 7.158122447660097),
            (12, 15.901736419454355, 11.529929433557227),
            (13, 8.499761248063109, 10.014845340810169),
            (14, 27.695613274277775, 18.855229307543972),
            (15, 19.32380136640505, 19.08951533697451),
            (16, 1.7491579409064582, 10.419336638940484),
            (17, 19.782709407674567, 15.101023023307526),
            (18, 11.553161103469721, 13.327092063388623),
            (19, -5.360304089082964, 3.983393987152829),
            (20, 15.152793499512779, 9.568093743332804),
            (21, 7.999846354811868, 8.783970049072336),
            (22, -8.191866413110832, 0.29605181798075186),
            (23, 13.469528628351672, 6.882790223166212),
            (24, 6.781726711786284, 6.832258467476247),
            (25, 26.70845833486408, 16.770358401170164),
            (26, 18.61951357662223, 17.694935988896198),
            (27, 1.2122951925180985, 9.453615590707148),
            (28, 19.4702811998816, 14.461948395294375),
            (29, 11.331080578433813, 12.896514486864094),
            (30, -5.528067840318502, 3.684223323272796),
            (31, 15.056684499060852, 9.370453911166823),
            (32, 7.93217590340769, 8.651314907287256),
            (33, -8.242630490746267, 0.20434220827049465),
            (34, 13.440660068325553, 6.822501138298024),
            (35, 6.761481101120903, 6.791991119709463),
            (36, 26.696890975662672, 16.744441047686067),
            (37, 18.61129062462302, 17.677865836154545),
            (38, 1.2060431173990807, 9.441954476776813),
            (39, 19.466650340338127, 14.45430240855747),
            (40, 11.328503246861152, 12.89140282770931),
            (41, -5.5300128725456, 3.680694977581855),
            (42, 15.05557110285549, 9.368133040218673),
            (43, 7.931392380445344, 8.64976271033201),
            (44, -8.24321803448801, 0.20327233792199983),
        ];

        let mut tsi = TrueStrengthIndex::new(5, 3, 3);
        tsi.calculate(&data);

        assert_eq!(tsi.values.len(), data.len());
        assert!(
            tsi.values.to_rows()[..8]
                .iter()
                .all(|v| matches!(v, IndicatorValue::None))
        );
        for (bar, main, signal) in expected {
            let IndicatorValue::Multiple(ref got) = tsi.values.to_rows()[bar] else {
                panic!(
                    "bar {bar}: expected a value, got {:?}",
                    tsi.values.to_rows()[bar]
                );
            };
            assert_eq!(got.len(), tsi.line_count());
            assert!(
                (got[0] - main).abs() < 1e-12,
                "bar {bar}: TSI {} vs {main}",
                got[0]
            );
            assert!(
                (got[1] - signal).abs() < 1e-12,
                "bar {bar}: signal {} vs {signal}",
                got[1]
            );
        }
    }
}
