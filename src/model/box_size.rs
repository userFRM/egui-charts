//! Box sizes derived once from historical bars.

use super::Bar;
use serde::{Deserialize, Serialize};

/// How a price-based chart obtains its size in price units.
/// Resolve from the loaded history when selecting the method, then keep the result while
/// bars stream in. Reapply the parameters to explicitly recalculate from newer history.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum BoxSizeMethod {
    /// Use the chart type's explicit size in price units.
    #[default]
    Fixed,
    /// Wilder's average true range, seeded with the mean of the first `period` true ranges,
    /// then smoothed through the remaining history and multiplied by `multiplier`.
    Atr {
        /// Number of true ranges in the seed and smoothing period; at least 1.
        period: usize,
        /// Positive, finite multiple of ATR.
        multiplier: f64,
    },
    /// A percentage of the absolute latest close; 1 means 1%.
    Percentage(f64),
}

impl BoxSizeMethod {
    /// Resolve a positive finite size, or `None` for invalid inputs or insufficient history.
    /// The first bar's true range is its high minus low. Later true ranges are the largest
    /// of high minus low, absolute high minus previous close, and absolute low minus previous close.
    pub fn resolve(self, fixed: f64, bars: &[Bar]) -> Option<f64> {
        let positive = |n: f64| n.is_finite() && n > 0.0;
        let size = match self {
            Self::Fixed => fixed,
            Self::Percentage(percent) if positive(percent) => {
                super::last_with_value(bars)?.close.abs() * (percent / 100.0)
            }
            // Whitespace bars have no range and are skipped, as the price-based builders skip them.
            Self::Atr { period, multiplier } if period > 0 && positive(multiplier) => {
                let (mut average, mut previous, mut count) = (0.0, None::<f64>, 0);
                for bar in bars.iter().filter(|bar| !bar.is_whitespace()) {
                    let mut range = bar.high - bar.low;
                    if !bar.high.is_finite()
                        || !bar.low.is_finite()
                        || !bar.close.is_finite()
                        || range < 0.0
                    {
                        return None;
                    }
                    if let Some(previous) = previous {
                        range = range
                            .max((bar.high - previous).abs())
                            .max((bar.low - previous).abs());
                    }
                    if count < period {
                        average += range / period as f64;
                    } else {
                        average += (range - average) / period as f64;
                    }
                    previous = Some(bar.close);
                    count += 1;
                }
                if count < period {
                    return None;
                }
                average * multiplier
            }
            _ => return None,
        };
        positive(size).then_some(size)
    }
}

/// Input used to build Renko, Kagi and Line Break charts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PriceBasedSource {
    /// Use each bar's close, including for Renko wick extremes.
    #[default]
    Close,
    /// Use highs and lows. With no intrabar ordering, a continuation takes priority over a
    /// reversal in the same bar; at most one direction is processed per source bar.
    HighLow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_methods_match_wilder_atr_and_last_close_percentage() {
        let time = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        // TR = 2, 4, 6, 8. The three-bar seed is 4; next ATR is (2*4 + 8)/3 = 16/3.
        let bars: Vec<Bar> = [(99.0, 101.0), (98.0, 102.0), (97.0, 103.0), (96.0, 104.0)]
            .into_iter()
            .enumerate()
            .map(|(i, (low, high))| {
                Bar::new(
                    time + chrono::Duration::minutes(i as i64),
                    100.0,
                    high,
                    low,
                    100.0,
                    1.0,
                )
            })
            .collect();
        for (method, fixed, data, expected) in [
            (BoxSizeMethod::Fixed, 0.25, &bars[..], Some(0.25)),
            (BoxSizeMethod::Percentage(2.5), 99.0, &bars[..], Some(2.5)),
            (
                BoxSizeMethod::Atr {
                    period: 3,
                    multiplier: 1.5,
                },
                99.0,
                &bars[..],
                Some(8.0),
            ),
            (
                BoxSizeMethod::Atr {
                    period: 3,
                    multiplier: 1.0,
                },
                99.0,
                &bars[..3],
                Some(4.0),
            ),
            (
                BoxSizeMethod::Atr {
                    period: 3,
                    multiplier: 1.0,
                },
                99.0,
                &bars[..2],
                None,
            ),
            (
                BoxSizeMethod::Atr {
                    period: 0,
                    multiplier: 1.0,
                },
                99.0,
                &bars[..],
                None,
            ),
            (BoxSizeMethod::Percentage(1.0), 99.0, &bars[..0], None),
            (
                BoxSizeMethod::Percentage(f64::INFINITY),
                99.0,
                &bars[..],
                None,
            ),
            (BoxSizeMethod::Fixed, 0.0, &bars[..], None),
        ] {
            assert_eq!(method.resolve(fixed, data), expected, "{method:?}");
        }
        let expected = 16.0 / 3.0;
        assert!(
            (super::super::RenkoConfig::from_atr(&bars, 3, 1.0).brick_size - expected).abs()
                < 1e-12
        );
        assert!(
            (super::super::KagiConfig::from_atr(&bars, 3, 1.0).reversal_amount - expected).abs()
                < 1e-12
        );
        let mut gap = bars[..2].to_vec();
        gap[1] = Bar::new(gap[1].time, 110.0, 112.0, 109.0, 111.0, 1.0);
        // Gap TR = max(3, |112-100|, |109-100|) = 12; seed = (2+12)/2 = 7.
        assert_eq!(
            BoxSizeMethod::Atr {
                period: 2,
                multiplier: 1.0
            }
            .resolve(0.0, &gap),
            Some(7.0)
        );
    }
}
