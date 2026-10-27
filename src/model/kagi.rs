//! Kagi chart transformation.
//!
//! Kagi charts display price movements as a series of connected vertical
//! lines whose thickness changes based on trend direction:
//!
//! - **Thick** -- uptrend; price has risen above the prior shoulder, the
//!   high where the last falling line began.
//! - **Thin** -- downtrend; price has fallen below the prior waist, the
//!   low where the last rising line began.
//!
//! The thickness changes at the crossing itself, so a line that crosses
//! the shoulder or waist is drawn in two parts.
//!
//! A new line segment is created whenever price reverses by at least the
//! configured [`reversal_amount`](KagiConfig::reversal_amount). Time is
//! ignored -- only price movement matters.
//!
//! Use [`to_kagi_lines`] to transform a slice of [`Bar`]s.

use super::PriceBasedSource;
use crate::model::{Bar, PriceBuilder};
use chrono::{DateTime, Utc};

/// A single vertical segment in a Kagi chart.
#[derive(Debug, Clone)]
pub struct KagiLine {
    /// Timestamp when this segment was created (from the source bar).
    pub ts: DateTime<Utc>,
    /// Price at the beginning of this segment.
    pub start_price: f64,
    /// Price at the end of this segment.
    pub end_price: f64,
    /// Line thickness at the end of the segment, indicating trend state.
    pub thickness: KagiThickness,
    /// Price where the segment crosses the prior shoulder or waist and takes `thickness`; the
    /// part from `start_price` to it has the opposite thickness. `None` when the whole segment
    /// has `thickness`.
    pub crossing: Option<f64>,
}

/// Kagi line thickness, encoding the current trend direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KagiThickness {
    /// Thin line after a break below the previous significant low.
    Thin,
    /// Thick line after a break above the previous significant high.
    Thick,
}

impl KagiLine {
    /// Convert to a Bar for rendering compatibility
    pub fn to_bar(&self) -> Bar {
        Bar {
            time: self.ts,
            open: self.start_price,
            high: self.start_price.max(self.end_price),
            low: self.start_price.min(self.end_price),
            close: self.end_price,
            volume: 0.0, // Kagi doesn't use volume
            colors: None,
            payload: None,
        }
    }

    /// Check if this line is moving up
    pub fn is_up(&self) -> bool {
        self.end_price > self.start_price
    }
}

/// Configuration for the Kagi chart transformation.
///
/// The reversal amount can be set directly, derived from ATR, or
/// computed as a percentage of price.
#[derive(Debug, Clone)]
pub struct KagiConfig {
    /// Reversal amount - price movement needed to create a new line
    pub reversal_amount: f64,
    /// Price input; high/low gives continuation priority when a bar crosses both thresholds.
    pub source: PriceBasedSource,
}

impl Default for KagiConfig {
    fn default() -> Self {
        Self {
            reversal_amount: 1.0,
            source: PriceBasedSource::Close,
        }
    }
}

impl KagiConfig {
    /// Create a Kagi config with an explicit reversal amount in price units.
    pub fn new(reversal_amount: f64) -> Self {
        Self {
            reversal_amount,
            source: PriceBasedSource::Close,
        }
    }

    /// Calculate reversal amount from ATR (Avg True Range)
    pub fn from_atr(bars: &[Bar], period: usize, multiplier: f64) -> Self {
        Self {
            reversal_amount: super::BoxSizeMethod::Atr { period, multiplier }
                .resolve(0.0, bars)
                .unwrap_or(f64::NAN),
            source: PriceBasedSource::Close,
        }
    }

    /// Calculate reversal amount as percentage of price
    pub fn from_percentage(base_price: f64, percentage: f64) -> Self {
        Self {
            reversal_amount: base_price.abs() * (percentage / 100.0),
            source: PriceBasedSource::Close,
        }
    }
}

/// Transform bars into Kagi lines
///
/// Returns no lines when the reversal amount is not a positive finite number.
///
/// # Safety Limits
/// - At most the newest 10,000 lines
pub fn to_kagi_lines(bars: &[Bar], config: &KagiConfig) -> Vec<KagiLine> {
    let reversal = config.reversal_amount;
    if bars.is_empty() || !(reversal > 0.0 && reversal.is_finite()) {
        return Vec::new();
    }

    let mut builder = KagiBuilder::new(reversal, config.source);
    for bar in bars {
        builder.push(bar);
    }
    builder.elements()
}

pub(crate) const MAX_LINES: usize = 10_000;

/// The line in progress and the trend state.
#[derive(Clone)]
struct KagiState {
    curr_price: f64,
    line_start_price: f64,
    line_direction_up: bool,
    last_ts: DateTime<Utc>,
    /// The prior shoulder of a rising line or the prior waist of a falling one, where the
    /// line before it began; `None` on the first line.
    level: Option<f64>,
    /// Where the line in progress crossed `level`.
    crossing: Option<f64>,
    is_thick: bool,
}

impl KagiState {
    fn line(&self) -> KagiLine {
        KagiLine {
            ts: self.last_ts,
            start_price: self.line_start_price,
            end_price: self.curr_price,
            thickness: if self.is_thick {
                KagiThickness::Thick
            } else {
                KagiThickness::Thin
            },
            crossing: self.crossing,
        }
    }

    /// Starts the next line at the current price, turning against the line just closed.
    fn reverse(&mut self, price: f64) {
        self.level = Some(self.line_start_price);
        self.crossing = None;
        self.line_start_price = self.curr_price;
        self.curr_price = price;
        self.line_direction_up = !self.line_direction_up;
    }
}

/// Builds Kagi lines bar by bar; `reversal` must be a positive finite number. The line in
/// progress is kept apart from the closed lines and added by `elements`.
#[derive(Clone)]
pub(crate) struct KagiBuilder {
    reversal: f64,
    source: PriceBasedSource,
    state: Option<KagiState>,
    lines: Vec<KagiLine>,
}

impl KagiBuilder {
    pub(crate) fn new(reversal: f64, source: PriceBasedSource) -> Self {
        Self {
            reversal,
            source,
            state: None,
            lines: Vec::new(),
        }
    }
}

impl PriceBuilder for KagiBuilder {
    type Element = KagiLine;

    fn push(&mut self, bar: &Bar) {
        let reversal = self.reversal;
        let Some(st) = self.state.as_mut() else {
            self.state = Some(KagiState {
                curr_price: bar.close,
                line_start_price: bar.close,
                line_direction_up: true,
                last_ts: bar.time,
                level: None,
                crossing: None,
                is_thick: false,
            });
            return;
        };
        st.last_ts = bar.time;
        let price = match self.source {
            PriceBasedSource::Close => bar.close,
            PriceBasedSource::HighLow if st.line_direction_up => {
                if bar.high > st.curr_price {
                    bar.high
                } else {
                    bar.low
                }
            }
            PriceBasedSource::HighLow => {
                if bar.low < st.curr_price {
                    bar.low
                } else {
                    bar.high
                }
            }
        };

        let up = st.line_direction_up;
        if up && price < st.curr_price - reversal || !up && price > st.curr_price + reversal {
            self.lines.push(st.line());
            st.reverse(price);
        } else if up && price > st.curr_price || !up && price < st.curr_price {
            st.curr_price = price;
        } else {
            return;
        }

        // A rising thin line turns thick above the shoulder, a falling thick line thin below
        // the waist.
        let up = st.line_direction_up;
        if let Some(level) = st.level
            && st.is_thick != up
            && (up && st.curr_price > level || !up && st.curr_price < level)
        {
            st.is_thick = up;
            st.crossing = Some(level);
        }

        // Bound memory by dropping the oldest lines; the state never reads them.
        if self.lines.len() == 2 * MAX_LINES {
            self.lines.drain(..MAX_LINES);
        }
    }

    fn elements(&self) -> Vec<KagiLine> {
        let mut lines = self.lines.clone();
        // The line in progress, unless it has not moved
        if let Some(st) = self
            .state
            .as_ref()
            .filter(|st| st.line_start_price != st.curr_price)
        {
            lines.push(st.line());
        }
        lines.drain(..lines.len().saturating_sub(MAX_LINES));
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn thickness_changes_where_a_line_crosses_the_prior_shoulder_or_waist() {
        use KagiThickness::{Thick, Thin};
        let start = Utc::now();
        // 100 up to 105, down to 102, up through the 105 shoulder to 107 in one bar, down to
        // 103 and on through the 102 waist to 101.
        let bars: Vec<Bar> = [100.0, 105.0, 102.0, 107.0, 103.0, 101.0]
            .into_iter()
            .enumerate()
            .map(|(i, close)| Bar {
                time: start + Duration::minutes(i as i64),
                open: close,
                high: close,
                low: close,
                close,
                volume: 0.0,
                colors: None,
                payload: None,
            })
            .collect();
        let lines: Vec<_> = to_kagi_lines(&bars, &KagiConfig::new(1.0))
            .into_iter()
            .map(|l| (l.start_price, l.end_price, l.thickness, l.crossing))
            .collect();
        assert_eq!(
            lines,
            [
                (100.0, 105.0, Thin, None),
                (105.0, 102.0, Thin, None),
                (102.0, 107.0, Thick, Some(105.0)),
                (107.0, 101.0, Thin, Some(102.0)),
            ]
        );
    }

    #[test]
    fn test_kagi_line_to_bar() {
        let line = KagiLine {
            ts: Utc::now(),
            start_price: 100.0,
            end_price: 105.0,
            thickness: KagiThickness::Thick,
            crossing: None,
        };

        let bar = line.to_bar();
        assert_eq!(bar.open, 100.0);
        assert_eq!(bar.close, 105.0);
        assert_eq!(bar.high, 105.0);
        assert_eq!(bar.low, 100.0);
    }

    #[test]
    fn test_kagi_line_direction() {
        let up_line = KagiLine {
            ts: Utc::now(),
            start_price: 100.0,
            end_price: 105.0,
            thickness: KagiThickness::Thick,
            crossing: None,
        };
        assert!(up_line.is_up());

        let down_line = KagiLine {
            ts: Utc::now(),
            start_price: 105.0,
            end_price: 100.0,
            thickness: KagiThickness::Thin,
            crossing: None,
        };
        assert!(!down_line.is_up());
    }
}
