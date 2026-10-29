//! Line Break chart transformation.
//!
//! A Line Break chart extends the current direction when the selected price
//! passes the last line's extreme. A reversal requires a break past the opposite
//! extreme of the configured number of preceding lines. Use
//! [`to_line_break_lines`] to transform OHLCV bars without rendering.
//!
//! - [`to_line_break_lines`] converts OHLCV bars into line-break lines.
//! - [`detect_signal`] classifies the relationship between the last two
//!   lines as a continuation or reversal.

use super::PriceBasedSource;
use super::PriceBuilder;
use super::bar::Bar;
use chrono::{DateTime, Utc};

/// Configuration for the Line Break chart transformation.
///
/// The `line_count` parameter (typically 3) controls how many prior lines
/// must be exceeded for a reversal to register.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LineBreakConfig {
    /// Number of lines to look back for reversal
    pub line_count: usize,
    /// Price input; high/low gives continuation priority when a bar crosses both thresholds.
    pub source: PriceBasedSource,
}

impl Default for LineBreakConfig {
    fn default() -> Self {
        Self {
            line_count: 3,
            source: PriceBasedSource::Close,
        }
    }
}

impl LineBreakConfig {
    /// Create a Line Break config with the given look-back count.
    pub fn new(line_count: usize) -> Self {
        Self {
            line_count,
            source: PriceBasedSource::Close,
        }
    }
}

/// Direction of a single line in a Line Break chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineDirection {
    /// An upward direction.
    Up,
    /// A downward direction.
    Down,
}

/// A single line (block) in a Line Break chart.
///
/// A continuation line opens at the previous line's close, a reversal line
/// at the previous line's open; it closes at the bar's close.
#[derive(Debug, Clone)]
pub struct LineBreakLine {
    /// Open price: the previous line's close for a continuation, its open for a reversal
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
/// - At most the newest 10,000 lines
pub fn to_line_break_lines(data: &[Bar], config: &LineBreakConfig) -> Vec<LineBreakLine> {
    let mut builder = LineBreakBuilder::new(config);
    for bar in data {
        builder.push(bar);
    }
    builder.elements()
}

pub(crate) const MAX_LINES: usize = 10_000;

/// Builds Line Break lines bar by bar.
#[derive(Clone)]
pub(crate) struct LineBreakBuilder {
    line_count: usize,
    source: PriceBasedSource,
    lines: Vec<LineBreakLine>,
}

impl LineBreakBuilder {
    pub(crate) fn new(config: &LineBreakConfig) -> Self {
        Self {
            line_count: config.line_count,
            source: config.source,
            lines: Vec::new(),
        }
    }
}

impl PriceBuilder for LineBreakBuilder {
    type Element = LineBreakLine;

    fn push(&mut self, bar: &Bar) {
        let Some(last_line) = self.lines.last() else {
            // The first bar is the first line
            self.lines.push(LineBreakLine {
                open: bar.open,
                close: bar.close,
                direction: if bar.close >= bar.open {
                    LineDirection::Up
                } else {
                    LineDirection::Down
                },
                ts: bar.time,
            });
            return;
        };
        let lines = &self.lines;
        let curr_direction = last_line.direction;

        let price = match self.source {
            PriceBasedSource::Close => bar.close,
            PriceBasedSource::HighLow if curr_direction == LineDirection::Up => {
                if bar.high > last_line.high() {
                    bar.high
                } else {
                    bar.low
                }
            }
            PriceBasedSource::HighLow => {
                if bar.low < last_line.low() {
                    bar.low
                } else {
                    bar.high
                }
            }
        };
        let new_line = match curr_direction {
            LineDirection::Up => {
                // For a continuation, close must exceed the last line's high
                if price > last_line.high() {
                    Some(LineBreakLine {
                        open: last_line.close,
                        close: price,
                        direction: LineDirection::Up,
                        ts: bar.time,
                    })
                }
                // For a reversal, close must break below the low of the last N lines
                else {
                    let lookback = self.line_count.min(lines.len());
                    let reversal_low = lines[lines.len() - lookback..]
                        .iter()
                        .map(|l| l.low())
                        .fold(f64::INFINITY, f64::min);

                    (price < reversal_low).then_some(LineBreakLine {
                        open: last_line.open,
                        close: price,
                        direction: LineDirection::Down,
                        ts: bar.time,
                    })
                }
            }
            LineDirection::Down => {
                // For a continuation, close must break below the last line's low
                if price < last_line.low() {
                    Some(LineBreakLine {
                        open: last_line.close,
                        close: price,
                        direction: LineDirection::Down,
                        ts: bar.time,
                    })
                }
                // For a reversal, close must exceed the high of the last N lines
                else {
                    let lookback = self.line_count.min(lines.len());
                    let reversal_high = lines[lines.len() - lookback..]
                        .iter()
                        .map(|l| l.high())
                        .fold(f64::NEG_INFINITY, f64::max);

                    (price > reversal_high).then_some(LineBreakLine {
                        open: last_line.open,
                        close: price,
                        direction: LineDirection::Up,
                        ts: bar.time,
                    })
                }
            }
        };
        self.lines.extend(new_line);

        // Bound memory by dropping the oldest lines; only the last `line_count` are read.
        let keep = MAX_LINES.max(self.line_count);
        if self.lines.len() >= keep.saturating_mul(2) {
            self.lines.drain(..self.lines.len() - keep);
        }
    }

    fn elements(&self) -> Vec<LineBreakLine> {
        self.lines[self.lines.len().saturating_sub(MAX_LINES)..].to_vec()
    }
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
                    colors: None,
                    payload: None,
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
                colors: None,
                payload: None,
            })
            .collect()
    }

    #[test]
    fn test_config_creation() {
        let config = LineBreakConfig::new(3);
        assert_eq!(config.line_count, 3);
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

    #[test]
    fn reversal_lines_open_at_the_last_line_open() {
        // (first open, closes after the first bar, expected (open, close) of every line)
        let rows = [
            (
                10.0,
                [11.0, 12.0, 13.0, 9.0],
                [(10.0, 11.0), (11.0, 12.0), (12.0, 13.0), (12.0, 9.0)],
            ),
            (
                13.0,
                [12.0, 11.0, 10.0, 14.0],
                [(13.0, 12.0), (12.0, 11.0), (11.0, 10.0), (11.0, 14.0)],
            ),
        ];
        for (open, closes, expected) in rows {
            let t0 = Utc::now();
            let bars: Vec<Bar> = closes
                .iter()
                .enumerate()
                .map(|(i, &c)| {
                    let o = if i == 0 { open } else { c };
                    Bar {
                        time: t0,
                        open: o,
                        high: o.max(c),
                        low: o.min(c),
                        close: c,
                        volume: 0.0,
                        colors: None,
                        payload: None,
                    }
                })
                .collect();
            let lines = to_line_break_lines(&bars, &LineBreakConfig::default());
            let got: Vec<(f64, f64)> = lines.iter().map(|l| (l.open, l.close)).collect();
            assert_eq!(got, expected);
        }
    }
}
