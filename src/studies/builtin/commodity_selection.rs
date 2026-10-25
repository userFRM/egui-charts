use super::{clamp_param, over_finite_bars};
use crate::model::Bar;
use crate::studies::{ADX, ATR, Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Commodity Selection Index (CSI), J. Welles Wilder, New Concepts in Technical Trading Systems (1978).
///
/// `CSI = ADXR * ATR(n) * (V / sqrt(M)) * (1 / (150 + C)) * 100`, where `ADXR = (ADX + ADX n bars
/// earlier) / 2`, `V` is the money value of a one-unit price move (the point value), `M` the margin
/// and `C` the commission, both in money. With the defaults (V = 1, M = 1, C = 0) values are not
/// comparable across instruments; set the contract with [`Self::with_point_value`],
/// [`Self::with_margin`] and [`Self::with_commission`]. The first value is on bar `3n - 1`; earlier
/// bars are `None`.
#[derive(Clone)]
pub struct CommoditySelectionIndex {
    /// Period for calculation
    period: usize,
    point_value: f64,
    margin: f64,
    commission: f64,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl CommoditySelectionIndex {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            point_value: 1.0,
            margin: 1.0,
            commission: 0.0,
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.brown, // Brown
            visible: true,
        }
    }

    /// Set `V`, the money value of a one-unit price move, 1 by default, clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn with_point_value(mut self, point_value: f64) -> Self {
        self.point_value = clamp_param(point_value, 0.0, f64::MAX);
        self
    }

    /// Set `M`, the margin in money, 1 by default, clamped into `f64::EPSILON` to `f64::MAX`, NaN to `f64::EPSILON`.
    pub fn with_margin(mut self, margin: f64) -> Self {
        self.margin = clamp_param(margin, f64::EPSILON, f64::MAX);
        self
    }

    /// Set `C`, the commission in money, 0 by default, clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn with_commission(mut self, commission: f64) -> Self {
        self.commission = clamp_param(commission, 0.0, f64::MAX);
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        let mut adx = ADX::new(self.period);
        adx.calculate(data);
        let mut atr = ATR::new(self.period);
        atr.calculate(data);
        let adx_at = |i: usize| adx.values().value(0, i);
        let scale = self.point_value / self.margin.sqrt() / (150.0 + self.commission) * 100.0;

        for i in 0..data.len() {
            let adxr = i
                .checked_sub(self.period)
                .and_then(|lagged| Some((adx_at(i)? + adx_at(lagged)?) / 2.0));
            let atr_i = atr.values().value(0, i);
            values.push(match (adxr, atr_i) {
                (Some(adxr), Some(atr)) => IndicatorValue::Single(adxr * atr * scale),
                _ => IndicatorValue::None,
            });
        }
        values
    }
}

impl Default for CommoditySelectionIndex {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for CommoditySelectionIndex {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        point_value: number("Point value", 0.0, f64::MAX),
        margin: number("Margin", f64::EPSILON, f64::MAX),
        commission: number("Commission", 0.0, f64::MAX)
    ; {});

    fn name(&self) -> &str {
        "CSI"
    }

    fn desc(&self) -> &str {
        "Commodity Selection Index - Pos sizing metric"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| [b.high, b.low, b.close].iter().all(|p| p.is_finite()),
            |data| self.compute(data),
        );
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
        vec![format!("CSI({})", self.period)]
    }
}

/// Trend Detection Index (TDI), M. H. Pee, Technical Analysis of Stocks & Commodities, October 2001.
///
/// With `mom = close - close[n]`, `TDI = |sum(mom, n)| - (sum(|mom|, 2n) - sum(|mom|, n))`.
/// Positive values mean the market is trending, negative values mean it is consolidating.
/// The first value is on bar `3n - 1`; earlier bars are `None`.
#[derive(Clone)]
pub struct TrendDetectionIndex {
    period: usize,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl TrendDetectionIndex {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.info, // Indigo (using INFO blue as closest match)
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for TrendDetectionIndex {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Indicator for TrendDetectionIndex {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "TDI"
    }

    fn desc(&self) -> &str {
        "Trend Detection Index - Positive when trending, negative when consolidating"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let n = self.period;
        // mom[i] = close[i] - close[i - n], defined from bar n.
        let mom: Vec<f64> = (0..data.len())
            .map(|i| {
                if i >= n {
                    data[i].close - data[i - n].close
                } else {
                    0.0
                }
            })
            .collect();
        // The 2n-bar window of momentum starts at bar n, so the first value is on bar 3n - 1.
        let first = n.saturating_mul(3) - 1;

        for i in 0..data.len() {
            if i < first {
                self.values.push(IndicatorValue::None);
                continue;
            }
            let net: f64 = mom[i + 1 - n..=i].iter().sum();
            let older_abs: f64 = mom[i + 1 - 2 * n..=i - n].iter().map(|m| m.abs()).sum();
            self.values
                .push(IndicatorValue::Single(net.abs() - older_abs));
        }
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
        vec![format!("TDI({})", self.period)]
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
            high: close + 1.0,
            low: close - 1.0,
            close,
            volume: 1000.0,
            colors: None,
            payload: None,
        }
    }

    /// Reference values from M. H. Pee, "Trend Detection Index", Technical Analysis of Stocks &
    /// Commodities, October 2001: mom = close - close[n], TDI = |sum(mom, n)| - (sum(|mom|, 2n) - sum(|mom|, n)).
    #[test]
    fn tdi_matches_published_definition() {
        let flat_then_rising: Vec<f64> = (0..60)
            .map(|i| 100.0 + (i as f64 - 39.0).max(0.0))
            .collect();
        let reversal = [
            10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 14.0, 13.0, 12.0, 11.0, 12.0, 13.0,
        ];
        let cases: [(usize, &[f64], &[f64]); 2] = [
            (20, &flat_then_rising, &[210.0]),
            (3, &reversal, &[-6.0, 0.0, 2.0, -2.0]),
        ];
        for (period, closes, expected) in cases {
            let data: Vec<Bar> = closes.iter().map(|&c| make_bar(c)).collect();
            let mut tdi = TrendDetectionIndex::new(period);
            tdi.calculate(&data);
            let first = 3 * period - 1;
            assert_eq!(tdi.values.len(), closes.len());
            for (k, want) in expected.iter().enumerate() {
                match tdi.values.to_rows()[first + k] {
                    IndicatorValue::Single(got) => {
                        assert!(
                            (got - want).abs() < 1e-12,
                            "period {period}, bar {}: {got} != {want}",
                            first + k
                        )
                    }
                    _ => panic!("period {period}, bar {}: no value", first + k),
                }
            }
            assert!(
                tdi.values.to_rows()[..first]
                    .iter()
                    .all(|v| matches!(v, IndicatorValue::None))
            );
        }
    }
}
