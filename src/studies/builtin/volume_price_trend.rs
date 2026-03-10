use crate::model::Bar;
/// Volume Price Trend (VPT)
/// Cumulative volume indicator that considers price changes
/// Similar to OBV but uses percentage changes
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct VolumePriceTrend {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl VolumePriceTrend {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.bullish, // Teal
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for VolumePriceTrend {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for VolumePriceTrend {
    fn name(&self) -> &str {
        "VPT"
    }

    fn desc(&self) -> &str {
        "Volume Price Trend - Volume weighted by price change"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // First bar starts at 0
        let mut cumulative = 0.0;
        self.values.push(IndicatorValue::Single(cumulative));

        for i in 1..data.len() {
            let prev_close = data[i - 1].close;
            if prev_close != 0.0 {
                let pct_change = (data[i].close - prev_close) / prev_close;
                cumulative += data[i].volume * pct_change;
            }
            self.values.push(IndicatorValue::Single(cumulative));
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
        vec!["VPT".to_string()]
    }
}

/// Negative Volume Index (NVI)
/// Focuses on days with declining volume
#[derive(Clone)]
pub struct NegativeVolumeIndex {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl NegativeVolumeIndex {
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

impl Default for NegativeVolumeIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for NegativeVolumeIndex {
    fn name(&self) -> &str {
        "NVI"
    }

    fn desc(&self) -> &str {
        "Negative Volume Index - Tracks price on low volume days"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Start at 1000
        let mut nvi = 1000.0;
        self.values.push(IndicatorValue::Single(nvi));

        for i in 1..data.len() {
            // Only update NVI when volume decreases
            if data[i].volume < data[i - 1].volume && data[i - 1].close != 0.0 {
                let pct_change = (data[i].close - data[i - 1].close) / data[i - 1].close;
                nvi += nvi * pct_change;
            }
            self.values.push(IndicatorValue::Single(nvi));
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
        vec!["NVI".to_string()]
    }
}

/// Positive Volume Index (PVI)
/// Focuses on days with rising volume
#[derive(Clone)]
pub struct PositiveVolumeIndex {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl PositiveVolumeIndex {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.success, // Green
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for PositiveVolumeIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for PositiveVolumeIndex {
    fn name(&self) -> &str {
        "PVI"
    }

    fn desc(&self) -> &str {
        "Positive Volume Index - Tracks price on high volume days"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        // Start at 1000
        let mut pvi = 1000.0;
        self.values.push(IndicatorValue::Single(pvi));

        for i in 1..data.len() {
            // Only update PVI when volume increases
            if data[i].volume > data[i - 1].volume && data[i - 1].close != 0.0 {
                let pct_change = (data[i].close - data[i - 1].close) / data[i - 1].close;
                pvi += pvi * pct_change;
            }
            self.values.push(IndicatorValue::Single(pvi));
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
        vec!["PVI".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64, volume: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: close,
            high: close,
            low: close,
            close,
            volume,
        }
    }

    #[test]
    fn test_vpt() {
        let mut vpt = VolumePriceTrend::new();

        let data = vec![
            make_bar(100.0, 1000.0),
            make_bar(105.0, 1200.0), // +5% change
            make_bar(102.0, 800.0),  // -2.86% change
        ];

        vpt.calculate(&data);

        assert_eq!(vpt.values.len(), 3);

        // First VPT should be 0
        if let IndicatorValue::Single(v) = vpt.values[0] {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn test_nvi_pvi() {
        let mut nvi = NegativeVolumeIndex::new();
        let mut pvi = PositiveVolumeIndex::new();

        let data = vec![
            make_bar(100.0, 1000.0),
            make_bar(105.0, 800.0),  // Volume down, price up
            make_bar(108.0, 1200.0), // Volume up, price up
        ];

        nvi.calculate(&data);
        pvi.calculate(&data);

        assert_eq!(nvi.values.len(), 3);
        assert_eq!(pvi.values.len(), 3);
    }
}
