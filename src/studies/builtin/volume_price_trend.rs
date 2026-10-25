use super::over_finite_bars;
use crate::model::Bar;
/// Volume Price Trend (VPT)
/// Cumulative volume indicator that considers price changes
/// Similar to OBV but uses percentage changes
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct VolumePriceTrend {
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl VolumePriceTrend {
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.bullish, // Teal
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// The values over `data`, every bar of which has a finite close and volume.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.is_empty() {
            return values;
        }

        // First bar starts at 0
        let mut cumulative = 0.0;
        values.push(IndicatorValue::Single(cumulative));

        for i in 1..data.len() {
            let prev_close = data[i - 1].close;
            if prev_close != 0.0 {
                let pct_change = (data[i].close - prev_close) / prev_close;
                cumulative += data[i].volume * pct_change;
            }
            values.push(IndicatorValue::Single(cumulative));
        }

        values
    }
}

impl Default for VolumePriceTrend {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for VolumePriceTrend {
    fn value_format(&self) -> crate::studies::StudyFormat {
        crate::studies::StudyFormat::Volume
    }

    fn name(&self) -> &str {
        "VPT"
    }

    fn desc(&self) -> &str {
        "Volume Price Trend - Volume weighted by price change"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.close.is_finite() && b.volume.is_finite(),
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
        vec!["VPT".to_string()]
    }
}

/// Length of the NVI and PVI signal line, the published one-year EMA of the index.
const SIGNAL_LEN: f64 = 255.0;

/// The index from 1000, moved by the close's percentage change on the bars where `counts(volume,
/// previous volume)` holds, and its EMA signal line seeded with the first index value.
fn volume_index(data: &[Bar], counts: fn(f64, f64) -> bool) -> IndicatorValues {
    over_finite_bars(
        data,
        |b| b.close.is_finite() && b.volume.is_finite(),
        |data| compute_volume_index(data, counts),
    )
}

fn compute_volume_index(data: &[Bar], counts: fn(f64, f64) -> bool) -> IndicatorValues {
    let Some(first) = data.first() else {
        return IndicatorValues::new();
    };
    let alpha = 2.0 / (SIGNAL_LEN + 1.0);
    let (mut index, mut signal) = (1000.0, 1000.0);
    let mut values = IndicatorValues::new();
    values.push_lines(&[index, signal]);
    let mut prev = first;
    for bar in &data[1..] {
        if counts(bar.volume, prev.volume) && prev.close != 0.0 {
            index += index * ((bar.close - prev.close) / prev.close);
        }
        signal += (index - signal) * alpha;
        values.push_lines(&[index, signal]);
        prev = bar;
    }
    values
}

/// Negative Volume Index (NVI)
///
/// Starts at 1000 and moves by the close's percentage change only on bars whose volume fell.
/// Outputs `Multiple([index, signal])`: the signal line is the 255-bar EMA of the index, seeded
/// with the first index value so that it starts at bar 0 like the index.
#[derive(Clone)]
pub struct NegativeVolumeIndex {
    values: IndicatorValues,
    color: Color32,
    signal_color: Color32,
    visible: bool,
}

impl NegativeVolumeIndex {
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.error, // Red
            signal_color: DESIGN_TOKENS.semantic.extended.warning, // Orange
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
        self.values = volume_index(data, |volume, prev| volume < prev);
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color, self.signal_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.color = color;
        }
        if let Some(&color) = colors.get(1) {
            self.signal_color = color;
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

    fn line_cnt(&self) -> usize {
        2
    }

    fn line_names(&self) -> Vec<String> {
        vec!["NVI".to_string(), "Signal".to_string()]
    }
}

/// Positive Volume Index (PVI)
///
/// Starts at 1000 and moves by the close's percentage change only on bars whose volume rose.
/// Outputs `Multiple([index, signal])`: the signal line is the 255-bar EMA of the index, seeded
/// with the first index value so that it starts at bar 0 like the index.
#[derive(Clone)]
pub struct PositiveVolumeIndex {
    values: IndicatorValues,
    color: Color32,
    signal_color: Color32,
    visible: bool,
}

impl PositiveVolumeIndex {
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.success, // Green
            signal_color: DESIGN_TOKENS.semantic.extended.warning, // Orange
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
        self.values = volume_index(data, |volume, prev| volume > prev);
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color, self.signal_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.color = color;
        }
        if let Some(&color) = colors.get(1) {
            self.signal_color = color;
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

    fn line_cnt(&self) -> usize {
        2
    }

    fn line_names(&self) -> Vec<String> {
        vec!["PVI".to_string(), "Signal".to_string()]
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
            colors: None,
            payload: None,
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
        let IndicatorValue::Single(v) = vpt.values.to_rows()[0] else {
            panic!("no value: {:?}", vpt.values.to_rows()[0]);
        };
        assert_eq!(v, 0.0);
    }

    /// Index and signal line against the published definition. On a bar whose volume fell (NVI)
    /// or rose (PVI), the index is multiplied by close / previous close (Fosback, "Stock Market
    /// Logic", 1976). The index starts at 1000 and the signal is its 255-bar EMA, about one
    /// trading year, seeded with its first value as the crate's EMA is. Expected values were
    /// computed independently in exact rational arithmetic. Hand check, NVI bar 1: the index
    /// rises 5% to 1050 and the signal is 1000 + (1050 - 1000) * 2 / 256 = 1000.390625.
    #[test]
    fn nvi_pvi_match_published_definition() {
        // Volume down, up, unchanged with a zero close, down after a zero close, unchanged, up after
        // a zero close (the index holds without a previous close), down, up.
        let data: Vec<Bar> = [
            (100.0, 1000.0),
            (105.0, 800.0),
            (108.0, 1200.0),
            (0.0, 1200.0),
            (50.0, 900.0),
            (0.0, 900.0),
            (40.0, 1100.0),
            (44.0, 700.0),
            (33.0, 1400.0),
        ]
        .iter()
        .map(|&(close, volume)| make_bar(close, volume))
        .collect();
        let nvi = [
            (1000.0, 1000.0),
            (1050.0, 1000.390625),
            (1050.0, 1000.7781982421875),
            (1050.0, 1001.1627435684204),
            (1050.0, 1001.5442846342921),
            (1050.0, 1001.9228449105867),
            (1050.0, 1002.2984476847228),
            (1155.0, 1003.4914285621859),
            (1155.0, 1004.6750892765438),
        ];
        let pvi = [
            (1000.0, 1000.0),
            (1000.0, 1000.0),
            (1028.5714285714287, 1000.2232142857143),
            (1028.5714285714287, 1000.4446847098214),
            (1028.5714285714287, 1000.6644248962402),
            (1028.5714285714287, 1000.8824483624527),
            (1028.5714285714287, 1001.0987685203353),
            (1028.5714285714287, 1001.3133986769844),
            (771.4285714285714, 999.5174234641062),
        ];
        let cases = [
            (
                Box::new(NegativeVolumeIndex::new()) as Box<dyn Indicator>,
                nvi,
            ),
            (Box::new(PositiveVolumeIndex::new()), pvi),
        ];
        for (mut study, expected) in cases {
            study.calculate(&data);
            assert_eq!(study.line_cnt(), 2, "{}", study.name());
            assert_eq!(study.values().len(), expected.len(), "{}", study.name());
            for (bar, (value, &(index, signal))) in
                study.values().to_rows().iter().zip(&expected).enumerate()
            {
                let IndicatorValue::Multiple(lines) = value else {
                    panic!(
                        "{} bar {bar}: expected index and signal, got {value:?}",
                        study.name()
                    );
                };
                assert!(
                    (lines[0] - index).abs() < 1e-12 && (lines[1] - signal).abs() < 1e-12,
                    "{} bar {bar}: got {lines:?}, expected [{index}, {signal}]",
                    study.name()
                );
            }
        }
    }
}
