//! Avg Price Indicators
//! Simple price calculations useful for analysis

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// OHLC4 - Avg of Open, High, Low, Close
#[derive(Clone)]
pub struct OHLC4 {
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl OHLC4 {
    /// Create with the default plot colors.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            color: palette::LIGHT_BLUE,
            visible: true,
        }
    }

    /// Set the plot color.
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
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let from = from.min(self.values.len()).min(data.len());
        self.values.truncate(from);
        for bar in &data[from..] {
            let ohlc4 = (bar.open + bar.high + bar.low + bar.close) / 4.0;
            self.values.push(IndicatorValue::Single(ohlc4));
        }
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
/// uses the common HLC3 name.
#[derive(Clone)]
pub struct HLC3 {
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl HLC3 {
    /// Create with the default plot colors.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            color: palette::ORANGE,
            visible: true,
        }
    }

    /// Set the plot color.
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
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let from = from.min(self.values.len()).min(data.len());
        self.values.truncate(from);
        for bar in &data[from..] {
            let hlc3 = (bar.high + bar.low + bar.close) / 3.0;
            self.values.push(IndicatorValue::Single(hlc3));
        }
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
/// uses the common HL2 name.
#[derive(Clone)]
pub struct HL2 {
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl HL2 {
    /// Create with the default plot colors.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            color: palette::ORANGE,
            visible: true,
        }
    }

    /// Set the plot color.
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
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let from = from.min(self.values.len()).min(data.len());
        self.values.truncate(from);
        for bar in &data[from..] {
            let hl2 = (bar.high + bar.low) / 2.0;
            self.values.push(IndicatorValue::Single(hl2));
        }
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
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl TrueRange {
    /// Create with the default plot colors.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            color: palette::RED,
            visible: true,
        }
    }

    /// Set the plot color.
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

impl TrueRange {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.is_empty() {
            return values;
        }

        // First bar: just H-L
        values.push(IndicatorValue::Single(data[0].high - data[0].low));

        for i in 1..data.len() {
            let prev_close = data[i - 1].close;
            let hl = data[i].high - data[i].low;
            let hc = (data[i].high - prev_close).abs();
            let lc = (data[i].low - prev_close).abs();
            let tr = hl.max(hc).max(lc);
            values.push(IndicatorValue::Single(tr));
        }

        values
    }
}

impl Indicator for TrueRange {
    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Price
    }

    fn name(&self) -> &str {
        "TR"
    }

    fn desc(&self) -> &str {
        "True Range - Max of H-L, |H-Cp|, |L-Cp|"
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
            usable_behind(1, |b| {
                b.high.is_finite() && b.low.is_finite() && b.close.is_finite()
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
                    |data| self.compute(data),
                )
            },
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
        vec!["TR".to_string()]
    }
}
