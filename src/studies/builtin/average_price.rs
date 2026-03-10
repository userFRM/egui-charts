use crate::model::Bar;
/// Avg Price Indicators
/// Simple price calculations useful for analysis
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// OHLC4 - Avg of Open, High, Low, Close
#[derive(Clone)]
pub struct OHLC4 {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl OHLC4 {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.purple, // Purple
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for OHLC4 {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for OHLC4 {
    fn name(&self) -> &str {
        "OHLC4"
    }

    fn desc(&self) -> &str {
        "OHLC4 - (Open + High + Low + Close) / 4"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        for bar in data {
            let ohlc4 = (bar.open + bar.high + bar.low + bar.close) / 4.0;
            self.values.push(IndicatorValue::Single(ohlc4));
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
        true
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
        vec!["OHLC4".to_string()]
    }
}

/// HLC3 - Avg of High, Low, Close
/// Note: Same formula as [`TypicalPrice`](super::TypicalPrice); this variant
/// uses the standard TradingView naming convention.
#[derive(Clone)]
pub struct HLC3 {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl HLC3 {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.info, // Blue
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for HLC3 {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for HLC3 {
    fn name(&self) -> &str {
        "HLC3"
    }

    fn desc(&self) -> &str {
        "HLC3 - (High + Low + Close) / 3"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        for bar in data {
            let hlc3 = (bar.high + bar.low + bar.close) / 3.0;
            self.values.push(IndicatorValue::Single(hlc3));
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
        true
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
        vec!["HLC3".to_string()]
    }
}

/// HL2 - Avg of High and Low (Midpoint)
/// Note: Same formula as [`MedianPrice`](super::MedianPrice); this variant
/// uses the standard TradingView naming convention.
#[derive(Clone)]
pub struct HL2 {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl HL2 {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.warning, // Orange
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for HL2 {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for HL2 {
    fn name(&self) -> &str {
        "HL2"
    }

    fn desc(&self) -> &str {
        "HL2 - (High + Low) / 2 - Midpoint price"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        for bar in data {
            let hl2 = (bar.high + bar.low) / 2.0;
            self.values.push(IndicatorValue::Single(hl2));
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
        true
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
        vec!["HL2".to_string()]
    }
}

/// True Range (not avgd)
#[derive(Clone)]
pub struct TrueRange {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl TrueRange {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.error, // Red
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for TrueRange {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for TrueRange {
    fn name(&self) -> &str {
        "TR"
    }

    fn desc(&self) -> &str {
        "True Range - Max of H-L, |H-Cp|, |L-Cp|"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // First bar: just H-L
        self.values
            .push(IndicatorValue::Single(data[0].high - data[0].low));

        for i in 1..data.len() {
            let prev_close = data[i - 1].close;
            let hl = data[i].high - data[i].low;
            let hc = (data[i].high - prev_close).abs();
            let lc = (data[i].low - prev_close).abs();
            let tr = hl.max(hc).max(lc);
            self.values.push(IndicatorValue::Single(tr));
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
        vec!["TR".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open,
            high,
            low,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_ohlc4() {
        let mut ohlc4 = OHLC4::new();
        let data = vec![make_bar(100.0, 110.0, 90.0, 105.0)];

        ohlc4.calculate(&data);

        // OHLC4 = (100 + 110 + 90 + 105) / 4 = 101.25
        if let IndicatorValue::Single(v) = ohlc4.values[0] {
            assert!((v - 101.25).abs() < 0.01);
        }
    }

    #[test]
    fn test_hlc3() {
        let mut hlc3 = HLC3::new();
        let data = vec![make_bar(100.0, 110.0, 90.0, 105.0)];

        hlc3.calculate(&data);

        // HLC3 = (110 + 90 + 105) / 3 = 101.67
        if let IndicatorValue::Single(v) = hlc3.values[0] {
            assert!((v - 101.67).abs() < 0.01);
        }
    }

    #[test]
    fn test_hl2() {
        let mut hl2 = HL2::new();
        let data = vec![make_bar(100.0, 110.0, 90.0, 105.0)];

        hl2.calculate(&data);

        // HL2 = (110 + 90) / 2 = 100
        if let IndicatorValue::Single(v) = hl2.values[0] {
            assert!((v - 100.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_true_range() {
        let mut tr = TrueRange::new();
        let data = vec![
            make_bar(100.0, 105.0, 95.0, 102.0),
            make_bar(102.0, 108.0, 99.0, 106.0),
        ];

        tr.calculate(&data);

        assert_eq!(tr.values.len(), 2);
    }
}
