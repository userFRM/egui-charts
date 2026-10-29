use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Trend Intensity Index (TII), after M. H. Pee (Technical Analysis of Stocks & Commodities, June 2002).
///
/// Over the last `period` bars, the share of the deviations of the close from its `2 * period` bar simple moving average that lies above it, weighted by size: `100 * SD+ / (SD+ + SD-)`.
/// Values above 50 indicate an uptrend, below 50 a downtrend.
/// The first value is on bar `3 * period - 2`; when every deviation is zero the value is 50.
#[derive(Clone)]
pub struct TrendIntensityIndex {
    /// Number of deviations summed; the moving average spans twice as many bars.
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl TrendIntensityIndex {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::BLUE, // Indigo/Blue
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for TrendIntensityIndex {
    fn default() -> Self {
        Self::new(30)
    }
}

impl TrendIntensityIndex {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        let n = self.period;
        let ma_len = n.saturating_mul(2);
        let first = ma_len.saturating_add(n) - 2;
        // Deviation of each close from the moving average ending on its bar, once it exists.
        let dev: Vec<f64> = data
            .iter()
            .enumerate()
            .map(|(i, bar)| {
                if i + 1 < ma_len {
                    return 0.0;
                }
                // Summing the differences keeps a flat window's deviation exactly zero.
                let window = &data[i + 1 - ma_len..=i];
                window.iter().map(|b| bar.close - b.close).sum::<f64>() / ma_len as f64
            })
            .collect();
        for i in 0..data.len() {
            if i < first {
                values.push(IndicatorValue::None);
                continue;
            }
            let (mut up, mut down) = (0.0, 0.0);
            for &d in &dev[i + 1 - n..=i] {
                if d > 0.0 {
                    up += d;
                } else {
                    down -= d;
                }
            }
            let total = up + down;
            let tii = if total > 0.0 {
                100.0 * up / total
            } else {
                50.0
            };
            values.push(IndicatorValue::Single(tii));
        }

        values
    }
}

impl Indicator for TrendIntensityIndex {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "TII"
    }

    fn desc(&self) -> &str {
        "Trend Intensity Index - Measures trend strength"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(
                self.period
                    .saturating_mul(2)
                    .saturating_add(self.period)
                    .saturating_sub(2),
                |b| b.close.is_finite(),
            ),
            |data| over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data)),
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
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

    fn line_names(&self) -> Vec<String> {
        vec![format!("TII({})", self.period)]
    }
}

/// Qstick (Quantitative Candlestick)
/// Moving avg of the open-close difference
/// Positive = bullish, Negative = bearish
#[derive(Clone)]
pub struct Qstick {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl Qstick {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: palette::PURPLE,
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for Qstick {
    fn default() -> Self {
        Self::new(8)
    }
}

impl Qstick {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];

                // Calculate avg of (close - open)
                let avg: f64 =
                    window.iter().map(|b| b.close - b.open).sum::<f64>() / self.period as f64;

                values.push(IndicatorValue::Single(avg));
            }
        }
        values
    }
}

impl Indicator for Qstick {
    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Price
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Qstick"
    }

    fn desc(&self) -> &str {
        "Qstick - Moving avg of candlestick body"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            |_data, from| from.saturating_sub(self.period - 1),
            |data| self.compute(data),
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
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

    fn line_names(&self) -> Vec<String> {
        vec![format!("Qstick({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(open: f64, close: f64) -> Bar {
        let high = open.max(close) + 1.0;
        let low = open.min(close) - 1.0;
        Bar {
            time: Utc::now(),
            open,
            high,
            low,
            close,
            volume: 1000.0,
            colors: None,
            payload: None,
        }
    }

    /// Expected values follow M. H. Pee, "Trend Intensity Index", Technical Analysis of Stocks & Commodities, June 2002, computed independently in exact arithmetic.
    #[test]
    fn tii_matches_published_definition() {
        let rising: Vec<f64> = (0..90).map(|i| 100.0 + i as f64).collect();
        let falling: Vec<f64> = rising.iter().rev().copied().collect();
        let mixed = [
            10.0, 12.0, 11.0, 14.0, 13.0, 15.0, 12.0, 16.0, 14.0, 13.0, 17.0, 15.0,
        ];
        // (period, closes, first bar with a value, values from that bar on)
        let cases: [(usize, &[f64], usize, &[f64]); 4] = [
            (30, &rising, 88, &[100.0, 100.0]),
            (30, &falling, 88, &[0.0, 0.0]),
            (3, &[0.1; 9], 7, &[50.0, 50.0]),
            (
                3,
                &mixed,
                7,
                &[600.0 / 7.0, 75.0, 75.0, 75.0, 1800.0 / 23.0],
            ),
        ];
        for (period, closes, first, expected) in cases {
            let data: Vec<Bar> = closes.iter().map(|&c| make_bar(c, c)).collect();
            let mut tii = TrendIntensityIndex::new(period);
            tii.calculate(&data);
            let got: Vec<Option<f64>> = tii
                .values()
                .to_rows()
                .iter()
                .map(|v| match v {
                    IndicatorValue::Single(x) => Some(*x),
                    _ => None,
                })
                .collect();
            assert_eq!(got.len(), closes.len());
            assert!(got[..first].iter().all(Option::is_none), "{got:?}");
            for (i, want) in expected.iter().enumerate() {
                let v = got[first + i].expect("value after warm-up");
                assert!((v - want).abs() < 1e-12, "bar {}: {v} != {want}", first + i);
            }
        }
    }

    #[test]
    fn test_qstick() {
        let mut qs = Qstick::new(5);

        // Bullish candles (close > open)
        let data: Vec<Bar> = (0..10)
            .map(|i| make_bar(100.0 + i as f64, 102.0 + i as f64))
            .collect();

        qs.calculate(&data);

        assert_eq!(qs.values.len(), 10);

        // Should be positive for bullish candles
        let rows = qs.values.to_rows();
        let IndicatorValue::Single(v) = rows.last().unwrap() else {
            panic!("no value: {:?}", qs.values.to_rows().last().unwrap());
        };
        assert!(*v > 0.0);
    }
}
