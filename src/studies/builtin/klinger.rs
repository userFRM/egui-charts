use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Klinger Volume Oscillator, as defined by Stephen J. Klinger in "Identifying Trends With Volume Analysis", Technical Analysis of Stocks & Commodities, V.15:12 (December 1997).
///
/// The trend is up when a bar's high + low + close is above the previous bar's sum, down when it is below, and unchanged when the sums are equal. `dm` is the bar's range and `cm` the cumulative range of the current trend: the previous `cm` plus `dm` while the trend continues, the previous bar's `dm` plus `dm` when it changes. The volume force is `volume * |2 * (dm / cm - 1)| * trend * 100`, the oscillator is its short EMA minus its long EMA, and the signal line is an EMA of the oscillator.
///
/// There is no trend, and so no volume force, until a bar's sum first differs from the bar before. On that first trend bar `cm` is the bar's `dm`, and a bar with `cm = 0` has a volume force of 0. Each EMA starts at the mean of its first `n` inputs.
///
/// Values are `None` until the oscillator exists, then `[oscillator, signal]`, with the signal NaN until it exists.
#[derive(Clone)]
pub struct KlingerOscillator {
    /// Short EMA period (typically 34)
    short_period: usize,
    /// Long EMA period (typically 55)
    long_period: usize,
    /// Signal line period (typically 13)
    signal_period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [Klinger, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl KlingerOscillator {
    pub fn new() -> Self {
        Self {
            short_period: 34,
            long_period: 55,
            signal_period: 13,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue - Klinger
                DESIGN_TOKENS.semantic.extended.warning, // Orange - Signal
            ],
            visible: true,
        }
    }

    pub fn with_periods(mut self, short: usize, long: usize, signal: usize) -> Self {
        self.short_period = short.max(1);
        self.long_period = long.max(1);
        self.signal_period = signal.max(1);
        self
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    fn calculate_ema(data: &[f64], period: usize) -> Vec<f64> {
        if data.is_empty() || period == 0 {
            return Vec::new();
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = Vec::with_capacity(data.len());

        // Find first valid value
        let mut first_sum = 0.0;
        let mut count = 0;
        let mut first_valid_idx = 0;

        for (i, &value) in data.iter().enumerate() {
            if !value.is_nan() {
                first_sum += value;
                count += 1;
                if count == period {
                    first_valid_idx = i;
                    break;
                }
            }
        }
        if count < period {
            return vec![f64::NAN; data.len()];
        }

        // A NaN input has no value but does not end the average.
        let mut prev = first_sum / period as f64;
        for i in 0..data.len() {
            if i < first_valid_idx || data[i].is_nan() {
                ema.push(f64::NAN);
            } else {
                if i > first_valid_idx {
                    prev += (data[i] - prev) * multiplier;
                }
                ema.push(prev);
            }
        }

        ema
    }
}

impl Default for KlingerOscillator {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for KlingerOscillator {
    fn name(&self) -> &str {
        "Klinger"
    }

    fn desc(&self) -> &str {
        "Klinger Volume Oscillator - Long-term money flow trend"
    }

    fn calculate(&mut self, data: &[Bar]) {
        let mut volume_force = vec![f64::NAN; data.len()];
        // 0.0 until a bar's sum first differs from the bar before.
        let mut trend = 0.0;
        let mut cm = 0.0;
        // A bar with a non-finite price has no force and is skipped, not compared against.
        let mut prev: Option<&Bar> = None;
        for (i, bar) in data.iter().enumerate() {
            if !(bar.high + bar.low + bar.close).is_finite() {
                continue;
            }
            let Some(prev) = prev.replace(bar) else {
                continue;
            };
            let sum = bar.high + bar.low + bar.close;
            let prev_sum = prev.high + prev.low + prev.close;
            let bar_trend = if sum > prev_sum {
                1.0
            } else if sum < prev_sum {
                -1.0
            } else {
                trend
            };
            if bar_trend == 0.0 {
                continue;
            }
            let dm = bar.high - bar.low;
            cm = if trend == 0.0 {
                dm
            } else if bar_trend == trend {
                cm + dm
            } else {
                prev.high - prev.low + dm
            };
            trend = bar_trend;
            volume_force[i] = if cm == 0.0 {
                0.0
            } else {
                bar.volume * (2.0 * (dm / cm - 1.0)).abs() * trend * 100.0
            };
        }

        let short_ema = Self::calculate_ema(&volume_force, self.short_period);
        let long_ema = Self::calculate_ema(&volume_force, self.long_period);
        let klinger: Vec<f64> = short_ema
            .iter()
            .zip(&long_ema)
            .map(|(s, l)| s - l)
            .collect();
        let signal = Self::calculate_ema(&klinger, self.signal_period);

        self.values = klinger
            .iter()
            .zip(&signal)
            .map(|(&kvo, &signal)| {
                if kvo.is_nan() {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Multiple(vec![kvo, signal])
                }
            })
            .collect();
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 2 {
            self.colors = colors;
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
        vec![
            format!("KVO({},{})", self.short_period, self.long_period),
            format!("Signal({})", self.signal_period),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, close: f64, volume: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: (high + low) / 2.0,
            high,
            low,
            close,
            volume,
        }
    }

    /// Expected values follow Stephen J. Klinger, "Identifying Trends With Volume Analysis",
    /// Technical Analysis of Stocks & Commodities, V.15:12 (December 1997), computed
    /// independently with this file's EMA seeding (the mean of the first `n` inputs).
    #[test]
    fn matches_klingers_definition() {
        let mut data: Vec<Bar> = (0..80)
            .map(|i| {
                let close = 100.0 + ((7 * i) % 11) as f64 * 0.5 + 0.25 * i as f64;
                make_bar(
                    close + 1.0 + (i % 3) as f64 * 0.5,
                    close - 1.0 - (i % 2) as f64 * 0.5,
                    close,
                    1000.0 + 250.0 * (i % 4) as f64,
                )
            })
            .collect();
        // No range on bars 0 and 1 gives cm = 0 on the first trend bar; bar 40 repeats the prices of
        // bar 39, whose trend is up, so equal sums must keep that trend.
        for bar in &mut data[..2] {
            bar.high = bar.close;
            bar.low = bar.close;
        }
        data[40] = Bar {
            volume: data[40].volume,
            ..data[39]
        };

        // (KVO, signal) for bars 55 to 79; the trend flips many times along the series.
        let expected = [
            (-7848.617481565998, f64::NAN),
            (-4339.64602772666, f64::NAN),
            (-6205.425115696111, f64::NAN),
            (-1752.73323152573, f64::NAN),
            (-4040.127175139147, f64::NAN),
            (-6200.828960250648, f64::NAN),
            (-2795.50613638723, f64::NAN),
            (-5050.879964616739, f64::NAN),
            (-9170.953041291119, f64::NAN),
            (-5461.165195491776, f64::NAN),
            (-6452.141418963016, f64::NAN),
            (-9929.238900876226, f64::NAN),
            (-5248.866838205729, -5730.471499056624),
            (-6125.694142781424, -5786.931876731596),
            (-1845.4547310008347, -5223.863713055774),
            (-4086.5283414884398, -5061.387231403298),
            (-7313.215017025366, -5383.076915063593),
            (-3092.9035000634976, -5055.9092843492945),
            (-4119.682994057504, -4922.162671450468),
            (-6919.806629935592, -5207.540379805487),
            (-1342.7450352586602, -4655.42675915594),
            (-2503.1976252211534, -4347.965454308113),
            (-4561.556968612778, -4378.478527780208),
            (818.4952172891135, -3636.05370705602),
            (-1382.6286621873223, -3314.135843503349),
        ];

        let mut klinger = KlingerOscillator::new();
        klinger.calculate(&data);

        let values = klinger.values();
        assert_eq!(values.len(), 80);
        assert!(
            values[..55]
                .iter()
                .all(|v| matches!(v, IndicatorValue::None))
        );
        let close = |x: f64, y: f64| (x.is_nan() && y.is_nan()) || (x - y).abs() <= 1e-9 * y.abs();
        for (offset, &(kvo, signal)) in expected.iter().enumerate() {
            let bar = 55 + offset;
            match &values[bar] {
                IndicatorValue::Multiple(v) => {
                    assert_eq!(v.len(), 2, "bar {bar}");
                    assert!(close(v[0], kvo), "bar {bar} KVO: {} != {kvo}", v[0]);
                    assert!(
                        close(v[1], signal),
                        "bar {bar} signal: {} != {signal}",
                        v[1]
                    );
                }
                other => panic!("bar {bar}: expected two lines, got {other:?}"),
            }
        }
        assert_eq!(klinger.line_cnt(), 2);

        // A bar with a non-finite price has no value; the study carries on after it.
        data[60].high = f64::NAN;
        klinger.calculate(&data);
        assert!(matches!(klinger.values()[60], IndicatorValue::None));
        assert!(klinger.values()[61..].iter().all(|v| matches!(
            v,
            IndicatorValue::Multiple(l) if l[0].is_finite()
        )));

        // One bar short of 55 volume force values: no oscillator yet.
        klinger.calculate(&data[..55]);
        assert!(
            klinger
                .values()
                .iter()
                .all(|v| matches!(v, IndicatorValue::None))
        );
    }
}
