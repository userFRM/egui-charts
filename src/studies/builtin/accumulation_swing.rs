use crate::model::Bar;
/// Accumulation Swing Index (ASI)
/// Developed by Welles Wilder
/// Cumulative sum of swing index values
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct AccumulationSwingIndex {
    /// Limit move (typically high value for stocks, actual limit for futures)
    limit_move: f64,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl AccumulationSwingIndex {
    pub fn new(limit_move: f64) -> Self {
        Self {
            limit_move,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.bullish, // Teal
            visible: true,
        }
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    fn calculate_swing_idx(current: &Bar, prev: &Bar, limit: f64) -> f64 {
        let c = current.close;
        let cy = prev.close;
        let h = current.high;
        let l = current.low;
        let o = current.open;
        let oy = prev.open;

        let k = (h - cy).abs().max((l - cy).abs());
        let r;

        let h_cy = (h - cy).abs();
        let l_cy = (l - cy).abs();
        let h_l = h - l;
        let cy_oy = (cy - oy).abs();

        if h_cy >= l_cy && h_cy >= h_l {
            r = h_cy - 0.5 * l_cy + 0.25 * cy_oy;
        } else if l_cy >= h_cy && l_cy >= h_l {
            r = l_cy - 0.5 * h_cy + 0.25 * cy_oy;
        } else {
            r = h_l + 0.25 * cy_oy;
        }

        if r == 0.0 {
            return 0.0;
        }

        50.0 * ((c - cy) + 0.5 * (c - o) + 0.25 * (cy - oy)) / r * (k / limit)
    }
}

impl Default for AccumulationSwingIndex {
    fn default() -> Self {
        Self::new(1000.0)
    }
}

impl Indicator for AccumulationSwingIndex {
    fn name(&self) -> &str {
        "ASI"
    }

    fn desc(&self) -> &str {
        "Accumulation Swing Index - Cumulative swing index"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < 2 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        let mut cumulative = 0.0;
        self.values.push(IndicatorValue::Single(0.0)); // First bar

        for i in 1..data.len() {
            let si = Self::calculate_swing_idx(&data[i], &data[i - 1], self.limit_move);
            cumulative += si;
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
        vec!["ASI".to_string()]
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
    fn test_asi_calculation() {
        let mut asi = AccumulationSwingIndex::new(100.0);

        let data = vec![
            make_bar(100.0, 102.0, 98.0, 101.0),
            make_bar(101.0, 104.0, 100.0, 103.0),
            make_bar(103.0, 106.0, 102.0, 105.0),
        ];

        asi.calculate(&data);

        assert_eq!(asi.values.len(), 3);

        // In uptrend, ASI should increase
        if let (IndicatorValue::Single(v1), IndicatorValue::Single(v2)) =
            (&asi.values[1], &asi.values[2])
        {
            assert!(v2 > v1, "ASI should increase in uptrend");
        }
    }
}
