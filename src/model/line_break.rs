//! Three-Line Break chart transformation.
//!
//! A Line Break chart (also called "N-Line Break") draws a new line only
//! when the closing price exceeds the high or low of the previous *N*
//! lines. This filters out minor noise while preserving significant
//! reversals.
//!
//! - [`to_line_break_lines`] converts OHLCV bars into line-break lines.
//! - [`detect_signal`] classifies the relationship between the last two
//!   lines as a continuation or reversal.

use super::bar::Bar;
use chrono::{DateTime, Utc};

/// Configuration for the Line Break chart transformation.
///
/// The `line_cnt` parameter (typically 3) controls how many prior lines
/// must be exceeded for a reversal to register.
#[derive(Debug, Clone)]
pub struct LineBreakConfig {
    /// Number of lines to look back for reversal
    pub line_cnt: usize,
}

impl Default for LineBreakConfig {
    fn default() -> Self {
        Self { line_cnt: 3 }
    }
}

impl LineBreakConfig {
    /// Create a Line Break config with the given look-back count.
    pub fn new(line_cnt: usize) -> Self {
        Self { line_cnt }
    }
}

/// Direction of a single line in a Line Break chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineDirection {
    Up,
    Down,
}

/// A single line (block) in a Line Break chart.
///
/// Each line spans from its open price (the previous line's close) to
/// its close price, colored by direction.
#[derive(Debug, Clone)]
pub struct LineBreakLine {
    /// Open price (previous line's close)
    pub open: f64,
    /// Close price
    pub close: f64,
    /// Direction of the line
    pub direction: LineDirection,
    /// Ts when line was created
    pub ts: DateTime<Utc>,
}

impl LineBreakLine {
    /// Get the high of the line
    pub fn high(&self) -> f64 {
        self.open.max(self.close)
    }

    /// Get the low of the line
    pub fn low(&self) -> f64 {
        self.open.min(self.close)
    }

    /// Check if this is a bullish (up) line
    pub fn is_bullish(&self) -> bool {
        self.direction == LineDirection::Up
    }

    /// Check if this is a bearish (down) line
    pub fn is_bearish(&self) -> bool {
        self.direction == LineDirection::Down
    }
}

/// Convert bar data to Line Break lines
///
/// # Safety Limits
/// - Maximum 10,000 lines to prevent memory issues
pub fn to_line_break_lines(data: &[Bar], config: &LineBreakConfig) -> Vec<LineBreakLine> {
    if data.is_empty() {
        return Vec::new();
    }

    const MAX_LINES: usize = 10_000;
    let mut lines: Vec<LineBreakLine> = Vec::new();

    // Initialize with first bar
    let first = &data[0];
    let initial_direction = if first.close >= first.open {
        LineDirection::Up
    } else {
        LineDirection::Down
    };

    lines.push(LineBreakLine {
        open: first.open,
        close: first.close,
        direction: initial_direction,
        ts: first.time,
    });

    for bar in data.iter().skip(1) {
        if lines.is_empty() || lines.len() >= MAX_LINES {
            continue;
        }

        let last_line = lines.last().unwrap();
        let curr_direction = last_line.direction;

        match curr_direction {
            LineDirection::Up => {
                // For a continuation, close must exceed the last line's high
                if bar.close > last_line.high() {
                    lines.push(LineBreakLine {
                        open: last_line.close,
                        close: bar.close,
                        direction: LineDirection::Up,
                        ts: bar.time,
                    });
                }
                // For a reversal, close must break below the low of the last N lines
                else {
                    let lookback = config.line_cnt.min(lines.len());
                    let reversal_low = lines[lines.len() - lookback..]
                        .iter()
                        .map(|l| l.low())
                        .fold(f64::INFINITY, f64::min);

                    if bar.close < reversal_low {
                        lines.push(LineBreakLine {
                            open: last_line.close,
                            close: bar.close,
                            direction: LineDirection::Down,
                            ts: bar.time,
                        });
                    }
                }
            }
            LineDirection::Down => {
                // For a continuation, close must break below the last line's low
                if bar.close < last_line.low() {
                    lines.push(LineBreakLine {
                        open: last_line.close,
                        close: bar.close,
                        direction: LineDirection::Down,
                        ts: bar.time,
                    });
                }
                // For a reversal, close must exceed the high of the last N lines
                else {
                    let lookback = config.line_cnt.min(lines.len());
                    let reversal_high = lines[lines.len() - lookback..]
                        .iter()
                        .map(|l| l.high())
                        .fold(f64::NEG_INFINITY, f64::max);

                    if bar.close > reversal_high {
                        lines.push(LineBreakLine {
                            open: last_line.close,
                            close: bar.close,
                            direction: LineDirection::Up,
                            ts: bar.time,
                        });
                    }
                }
            }
        }
    }

    lines
}

/// Signal generated from the last two lines of a Line Break chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineBreakSignal {
    /// New bullish line (potential buy)
    Bullish,
    /// New bearish line (potential sell)
    Bearish,
    /// Reversal from down to up
    BullishReversal,
    /// Reversal from up to down
    BearishReversal,
    /// No signal
    None,
}

/// Classify the signal produced by the last two lines in the sequence.
///
/// Returns [`LineBreakSignal::None`] if fewer than two lines exist.
pub fn detect_signal(lines: &[LineBreakLine]) -> LineBreakSignal {
    if lines.len() < 2 {
        return LineBreakSignal::None;
    }

    let prev = &lines[lines.len() - 2];
    let current = &lines[lines.len() - 1];

    match (prev.direction, current.direction) {
        (LineDirection::Down, LineDirection::Up) => LineBreakSignal::BullishReversal,
        (LineDirection::Up, LineDirection::Down) => LineBreakSignal::BearishReversal,
        (LineDirection::Up, LineDirection::Up) => LineBreakSignal::Bullish,
        (LineDirection::Down, LineDirection::Down) => LineBreakSignal::Bearish,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_uptrend_bars() -> Vec<Bar> {
        let now = Utc::now();
        (0..10)
            .map(|i| {
                let base = 100.0 + (i as f64 * 2.0);
                Bar {
                    time: now,
                    open: base,
                    high: base + 3.0,
                    low: base - 1.0,
                    close: base + 2.0,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    fn create_volatile_bars() -> Vec<Bar> {
        let now = Utc::now();
        let prices = [
            (100.0, 102.0),
            (102.0, 105.0),
            (105.0, 103.0),
            (103.0, 108.0),
            (108.0, 100.0),
            (100.0, 98.0),
            (98.0, 105.0),
        ];

        prices
            .iter()
            .map(|&(open, close)| Bar {
                time: now,
                open,
                high: open.max(close) + 1.0,
                low: open.min(close) - 1.0,
                close,
                volume: 1000.0,
            })
            .collect()
    }

    #[test]
    fn test_config_creation() {
        let config = LineBreakConfig::new(3);
        assert_eq!(config.line_cnt, 3);
    }

    #[test]
    fn test_uptrend_conversion() {
        let bars = create_uptrend_bars();
        let config = LineBreakConfig::new(3);
        let lines = to_line_break_lines(&bars, &config);

        assert!(!lines.is_empty());

        // In a strong uptrend, all lines should be up
        for line in &lines {
            assert_eq!(line.direction, LineDirection::Up);
        }
    }

    #[test]
    fn test_reversal_detection() {
        let bars = create_volatile_bars();
        let config = LineBreakConfig::new(3);
        let lines = to_line_break_lines(&bars, &config);

        // Should have at least one reversal
        let mut has_reversal = false;
        for i in 1..lines.len() {
            if lines[i - 1].direction != lines[i].direction {
                has_reversal = true;
                break;
            }
        }
        assert!(has_reversal || lines.len() <= 1);
    }

    #[test]
    fn test_signal_detection() {
        let lines = vec![
            LineBreakLine {
                open: 100.0,
                close: 105.0,
                direction: LineDirection::Up,
                ts: Utc::now(),
            },
            LineBreakLine {
                open: 105.0,
                close: 98.0,
                direction: LineDirection::Down,
                ts: Utc::now(),
            },
        ];

        let signal = detect_signal(&lines);
        assert_eq!(signal, LineBreakSignal::BearishReversal);
    }
}
