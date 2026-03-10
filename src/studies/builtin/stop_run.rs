//! # Stop Run Indicator
//!
//! Detects stop hunt patterns where price briefly breaks a swing
//! high/low and then reverses.

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Stop Run / Stop Hunt Indicator
///
/// Identifies potential stop run patterns:
/// - Price breaks recent swing high/low
/// - Quick reversal after the break
/// - Optional volume confirmation
#[derive(Clone)]
pub struct StopRunIndicator {
    /// Lookback period for swing detection
    swing_period: usize,
    /// Minimum ATR multiple for valid break
    break_threshold: f64,
    /// Bars to look for reversal
    reversal_window: usize,
    /// Require volume confirmation
    require_volume_confirmation: bool,
    /// Volume threshold (multiple of average)
    volume_threshold: f64,
    /// Calculated values
    values: Vec<IndicatorValue>,
    /// Bullish stop run color (stop loss hunted, then up)
    bullish_color: Color32,
    /// Bearish stop run color (stop loss hunted, then down)
    bearish_color: Color32,
    /// Visibility
    visible: bool,
    /// Detected stop runs
    detections: Vec<StopRunDetection>,
}

/// A detected stop run
#[derive(Debug, Clone)]
pub struct StopRunDetection {
    /// Bar index of the stop run candle
    pub bar_idx: usize,
    /// Price that was hunted
    pub hunted_price: f64,
    /// Type of stop run
    pub run_type: StopRunType,
    /// Volume at the stop run
    pub volume: f64,
    /// Reversal magnitude (how far it reversed)
    pub reversal_magnitude: f64,
    /// Confidence score
    pub confidence: f64,
}

/// Type of stop run
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopRunType {
    /// Broke below swing low, then reversed up (bullish)
    BullishStopRun,
    /// Broke above swing high, then reversed down (bearish)
    BearishStopRun,
}

impl StopRunIndicator {
    /// Create a new stop run indicator
    pub fn new(swing_period: usize) -> Self {
        Self {
            swing_period,
            break_threshold: 0.5, // 0.5 ATR minimum break
            reversal_window: 3,
            require_volume_confirmation: true,
            volume_threshold: 1.5,
            values: Vec::new(),
            bullish_color: DESIGN_TOKENS.semantic.chart.bullish,
            bearish_color: DESIGN_TOKENS.semantic.chart.bearish,
            visible: true,
            detections: Vec::new(),
        }
    }

    /// Set break threshold (ATR multiple)
    pub fn with_break_threshold(mut self, threshold: f64) -> Self {
        self.break_threshold = threshold;
        self
    }

    /// Set reversal window
    pub fn with_reversal_window(mut self, window: usize) -> Self {
        self.reversal_window = window;
        self
    }

    /// Require volume confirmation
    pub fn with_volume_confirmation(mut self, require: bool) -> Self {
        self.require_volume_confirmation = require;
        self
    }

    /// Set colors
    pub fn with_colors(mut self, bullish: Color32, bearish: Color32) -> Self {
        self.bullish_color = bullish;
        self.bearish_color = bearish;
        self
    }

    /// Get detected stop runs
    pub fn detections(&self) -> &[StopRunDetection] {
        &self.detections
    }

    /// Find swing high in range
    fn find_swing_high(&self, bars: &[Bar], end_idx: usize) -> Option<(usize, f64)> {
        if end_idx < self.swing_period {
            return None;
        }

        let start_idx = end_idx.saturating_sub(self.swing_period);
        let mut max_high = f64::MIN;
        let mut max_idx = start_idx;

        for i in start_idx..end_idx {
            if bars[i].high > max_high {
                max_high = bars[i].high;
                max_idx = i;
            }
        }

        // Swing high should be in the middle-ish, not at edges
        if max_idx > start_idx && max_idx < end_idx - 1 {
            Some((max_idx, max_high))
        } else {
            None
        }
    }

    /// Find swing low in range
    fn find_swing_low(&self, bars: &[Bar], end_idx: usize) -> Option<(usize, f64)> {
        if end_idx < self.swing_period {
            return None;
        }

        let start_idx = end_idx.saturating_sub(self.swing_period);
        let mut min_low = f64::MAX;
        let mut min_idx = start_idx;

        for i in start_idx..end_idx {
            if bars[i].low < min_low {
                min_low = bars[i].low;
                min_idx = i;
            }
        }

        // Swing low should be in the middle-ish
        if min_idx > start_idx && min_idx < end_idx - 1 {
            Some((min_idx, min_low))
        } else {
            None
        }
    }

    /// Calculate ATR for a range
    fn calculate_atr(&self, bars: &[Bar], end_idx: usize, period: usize) -> f64 {
        if end_idx < period {
            return 0.0;
        }

        let start_idx = end_idx.saturating_sub(period);
        let mut sum = 0.0;

        for i in start_idx..end_idx {
            let high_low = bars[i].high - bars[i].low;
            let high_close = if i > 0 {
                (bars[i].high - bars[i - 1].close).abs()
            } else {
                0.0
            };
            let low_close = if i > 0 {
                (bars[i].low - bars[i - 1].close).abs()
            } else {
                0.0
            };

            let tr = high_low.max(high_close).max(low_close);
            sum += tr;
        }

        sum / period as f64
    }

    /// Calculate average volume
    fn calculate_avg_volume(&self, bars: &[Bar], end_idx: usize, period: usize) -> f64 {
        if end_idx < period {
            return bars[..end_idx].iter().map(|b| b.volume).sum::<f64>() / end_idx.max(1) as f64;
        }

        let start_idx = end_idx.saturating_sub(period);
        bars[start_idx..end_idx]
            .iter()
            .map(|b| b.volume)
            .sum::<f64>()
            / period as f64
    }
}

impl Indicator for StopRunIndicator {
    fn name(&self) -> &str {
        "Stop Run"
    }

    fn desc(&self) -> &str {
        "Stop Run Detector - Identifies stop hunt patterns"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();
        self.detections.clear();

        if data.len() < self.swing_period + self.reversal_window {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // First swing_period bars have no signals
        for _ in 0..self.swing_period {
            self.values.push(IndicatorValue::None);
        }

        for idx in self.swing_period..data.len() {
            let bar = &data[idx];
            let atr = self.calculate_atr(data, idx, self.swing_period);
            let avg_volume = self.calculate_avg_volume(data, idx, self.swing_period);
            let break_distance = atr * self.break_threshold;

            let mut detected = None;

            // Check for bullish stop run (break below swing low, then reverse up)
            if let Some((_, swing_low)) = self.find_swing_low(data, idx) {
                // Did we break below the swing low?
                if bar.low < swing_low - break_distance {
                    // Check for reversal (close back above swing low or strong close)
                    let reversed = bar.close > swing_low || bar.close > bar.open;
                    let reversal_magnitude = bar.close - bar.low;

                    // Volume confirmation
                    let volume_ok = !self.require_volume_confirmation
                        || bar.volume > avg_volume * self.volume_threshold;

                    if reversed && volume_ok {
                        let confidence =
                            (reversal_magnitude / atr).min(1.0) * if volume_ok { 1.0 } else { 0.7 };

                        detected = Some(StopRunDetection {
                            bar_idx: idx,
                            hunted_price: swing_low,
                            run_type: StopRunType::BullishStopRun,
                            volume: bar.volume,
                            reversal_magnitude,
                            confidence,
                        });
                    }
                }
            }

            // Check for bearish stop run (break above swing high, then reverse down)
            if detected.is_none() {
                if let Some((_, swing_high)) = self.find_swing_high(data, idx) {
                    // Did we break above the swing high?
                    if bar.high > swing_high + break_distance {
                        // Check for reversal
                        let reversed = bar.close < swing_high || bar.close < bar.open;
                        let reversal_magnitude = bar.high - bar.close;

                        // Volume confirmation
                        let volume_ok = !self.require_volume_confirmation
                            || bar.volume > avg_volume * self.volume_threshold;

                        if reversed && volume_ok {
                            let confidence = (reversal_magnitude / atr).min(1.0)
                                * if volume_ok { 1.0 } else { 0.7 };

                            detected = Some(StopRunDetection {
                                bar_idx: idx,
                                hunted_price: swing_high,
                                run_type: StopRunType::BearishStopRun,
                                volume: bar.volume,
                                reversal_magnitude,
                                confidence,
                            });
                        }
                    }
                }
            }

            if let Some(detection) = detected {
                let value = match detection.run_type {
                    StopRunType::BullishStopRun => detection.confidence,
                    StopRunType::BearishStopRun => -detection.confidence,
                };
                self.values.push(IndicatorValue::Single(value));
                self.detections.push(detection);
            } else {
                self.values.push(IndicatorValue::None);
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.bullish_color, self.bearish_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.bullish_color = colors[0];
        }
        if colors.len() > 1 {
            self.bearish_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        true // Stop runs are marked on the main chart
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
        vec!["Stop Run".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_stop_run_bars() -> Vec<Bar> {
        let ts = Utc::now();
        let mut bars = Vec::new();

        // Build up to a swing low at 100
        for i in 0..5 {
            bars.push(Bar {
                time: ts,
                open: 105.0 - i as f64,
                high: 106.0 - i as f64,
                low: 104.0 - i as f64,
                close: 104.5 - i as f64,
                volume: 1000.0,
            });
        }

        // Swing low at 99
        bars.push(Bar {
            time: ts,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.5,
            volume: 1000.0,
        });

        // Move back up
        for i in 0..3 {
            bars.push(Bar {
                time: ts,
                open: 100.5 + i as f64,
                high: 102.0 + i as f64,
                low: 100.0 + i as f64,
                close: 101.5 + i as f64,
                volume: 1000.0,
            });
        }

        // STOP RUN: Break below swing low and reverse
        bars.push(Bar {
            time: ts,
            open: 100.0,
            high: 105.0,    // Strong reversal
            low: 95.0,      // Breaks well below 99 swing low (needs to exceed ATR * threshold)
            close: 104.0,   // Strong close above swing low
            volume: 3000.0, // High volume
        });

        bars
    }

    #[test]
    fn test_stop_run_detection() {
        let mut indicator = StopRunIndicator::new(5).with_volume_confirmation(false);

        let bars = create_stop_run_bars();
        indicator.calculate(&bars);

        // Should detect the bullish stop run
        let bullish_runs: Vec<_> = indicator
            .detections()
            .iter()
            .filter(|d| d.run_type == StopRunType::BullishStopRun)
            .collect();

        assert!(!bullish_runs.is_empty(), "Should detect bullish stop run");
    }

    #[test]
    fn test_swing_finding() {
        let ts = Utc::now();
        let bars: Vec<Bar> = (0..10)
            .map(|i| {
                let price = if i == 5 { 95.0 } else { 100.0 }; // Swing low at index 5
                Bar {
                    time: ts,
                    open: price,
                    high: price + 1.0,
                    low: price - 1.0,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect();

        let indicator = StopRunIndicator::new(5);
        let swing = indicator.find_swing_low(&bars, 8);

        assert!(swing.is_some());
        let (idx, price) = swing.unwrap();
        assert_eq!(idx, 5);
        assert_eq!(price, 94.0); // Low is price - 1.0
    }
}
