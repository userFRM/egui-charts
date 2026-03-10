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
    pub fn new(period: usize, cost: f64) -> Self {
        Self {
            period,
            cost,
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

        let min_period = self.period * 2;
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

/// Trend Detection Index (TDI)
/// Measures trend strength by comparing price movement to absolute movement
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
            period,
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
        "Trend Detection Index - Net vs absolute movement"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period + 1 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period {
                self.values.push(IndicatorValue::None);
            } else {
                // Net movement over period
                let net_move = data[i].close - data[i - self.period].close;

                // Sum of absolute movements
                let mut abs_move = 0.0;
                for j in (i + 1 - self.period)..=i {
                    abs_move += (data[j].close - data[j - 1].close).abs();
                }

                let tdi = if abs_move != 0.0 {
                    net_move.abs() - abs_move
                } else {
                    0.0
                };

                self.values.push(IndicatorValue::Single(tdi));
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

    #[test]
    fn test_tdi() {
        let mut tdi = TrendDetectionIndex::new(10);

        let data: Vec<Bar> = (0..20).map(|i| make_bar(100.0 + i as f64)).collect();

        tdi.calculate(&data);

        assert_eq!(tdi.values.len(), 20);
    }
}
