use super::clamp_param;
use crate::model::Bar;
/// Commodity Selection Index (CSI)
/// Developed by Welles Wilder
/// Combines ADXR, ATR, and margin requirements for position sizing
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct CommoditySelectionIndex {
    /// Period for calculation
    period: usize,
    /// Commission/cost factor
    cost: f64,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl CommoditySelectionIndex {
    /// `cost` is clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, cost: f64) -> Self {
        Self {
            period: period.max(1),
            cost: clamp_param(cost, 0.0, f64::MAX),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.brown, // Brown
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn calculate_atr(&self, data: &[Bar], end_idx: usize) -> f64 {
        if end_idx < self.period {
            return 0.0;
        }

        let mut sum = 0.0;
        for i in (end_idx - self.period + 1)..=end_idx {
            let tr = if i > 0 {
                let prev_close = data[i - 1].close;
                let hl = data[i].high - data[i].low;
                let hc = (data[i].high - prev_close).abs();
                let lc = (data[i].low - prev_close).abs();
                hl.max(hc).max(lc)
            } else {
                data[i].high - data[i].low
            };
            sum += tr;
        }

        sum / self.period as f64
    }

    fn calculate_adx(&self, data: &[Bar], end_idx: usize) -> f64 {
        if end_idx < self.period * 2 {
            return 0.0;
        }

        // Simplified ADX calculation for CSI
        let mut plus_dm_sum = 0.0;
        let mut minus_dm_sum = 0.0;
        let mut tr_sum = 0.0;

        for i in (end_idx - self.period + 1)..=end_idx {
            if i > 0 {
                let up_move = data[i].high - data[i - 1].high;
                let down_move = data[i - 1].low - data[i].low;

                if up_move > down_move && up_move > 0.0 {
                    plus_dm_sum += up_move;
                }
                if down_move > up_move && down_move > 0.0 {
                    minus_dm_sum += down_move;
                }

                let prev_close = data[i - 1].close;
                let tr = (data[i].high - data[i].low)
                    .max((data[i].high - prev_close).abs())
                    .max((data[i].low - prev_close).abs());
                tr_sum += tr;
            }
        }

        if tr_sum == 0.0 {
            return 0.0;
        }

        let plus_di = plus_dm_sum / tr_sum * 100.0;
        let minus_di = minus_dm_sum / tr_sum * 100.0;

        let di_sum = plus_di + minus_di;
        if di_sum == 0.0 {
            return 0.0;
        }

        (plus_di - minus_di).abs() / di_sum * 100.0 // Simplified - actual ADX is smoothed DX
    }
}

impl Default for CommoditySelectionIndex {
    fn default() -> Self {
        Self::new(14, 0.002) // 0.2% cost
    }
}

impl Indicator for CommoditySelectionIndex {
    fn name(&self) -> &str {
        "CSI"
    }

    fn desc(&self) -> &str {
        "Commodity Selection Index - Pos sizing metric"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let min_period = self.period.saturating_mul(2);
        if data.len() < min_period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < min_period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let atr = self.calculate_atr(data, i);
                let adx = self.calculate_adx(data, i);

                // CSI = ADXR * ATR14 / √(COMMISSION + COST)
                // Simplified formula
                let cost_factor = (self.cost).sqrt();
                let csi = if cost_factor > 0.0 && data[i].close > 0.0 {
                    adx * atr / cost_factor / data[i].close * 100.0
                } else {
                    0.0
                };

                self.values.push(IndicatorValue::Single(csi));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl TrendDetectionIndex {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
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

    fn values(&self) -> &[IndicatorValue] {
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
        }
    }

    #[test]
    fn test_csi() {
        let mut csi = CommoditySelectionIndex::new(5, 0.001);

        let data: Vec<Bar> = (0..30).map(|i| make_bar(100.0 + i as f64)).collect();

        csi.calculate(&data);

        assert_eq!(csi.values.len(), 30);
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
                match tdi.values[first + k] {
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
                tdi.values[..first]
                    .iter()
                    .all(|v| matches!(v, IndicatorValue::None))
            );
        }
    }
}
