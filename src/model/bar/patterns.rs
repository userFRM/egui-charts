//! Candlestick Pattern Detection
//!
//! Japanese candlestick patterns for technical analysis.
//! These methods extend the Bar type with pattern recognition capabilities.

use super::bar::Bar;

/// Extension trait for candlestick pattern detection
pub trait CandlePatterns {
    // === Single Bar Patterns ===

    /// Returns true if this is a hammer pattern
    ///
    /// Hammer characteristics:
    /// - Small body (< 30% of range)
    /// - Long lower wick (> 2x body)
    /// - Little to no upper wick
    /// - Typically appears in downtrends (bullish reversal)
    fn is_hammer(&self) -> bool;

    /// Returns true if this is an inverted hammer pattern
    ///
    /// Inverted Hammer characteristics:
    /// - Small body (< 30% of range)
    /// - Long upper wick (> 2x body)
    /// - Little to no lower wick
    /// - Typically appears in downtrends (bullish reversal)
    fn is_inverted_hammer(&self) -> bool;

    /// Returns true if this is a shooting star pattern
    ///
    /// Shooting Star characteristics:
    /// - Small body (< 30% of range)
    /// - Long upper wick (> 2x body)
    /// - Little to no lower wick
    /// - Body is near the low
    /// - Typically appears in uptrends (bearish reversal)
    fn is_shooting_star(&self) -> bool;

    /// Returns true if this is a spinning top pattern
    ///
    /// Spinning Top characteristics:
    /// - Small body (< 30% of range)
    /// - Upper and lower wicks of similar length
    /// - Indicates indecision in the market
    fn is_spinning_top(&self) -> bool;

    /// Returns true if this is a marubozu (very long body, little to no wicks)
    ///
    /// Marubozu characteristics:
    /// - Large body (> 85% of range)
    /// - Very small or no wicks
    /// - Strong directional move
    fn is_marubozu(&self) -> bool;

    // === Two Bar Patterns ===

    /// Returns true if current bar engulfs the previous bar (bullish engulfing)
    ///
    /// Bullish Engulfing:
    /// - Previous bar is bearish
    /// - Current bar is bullish
    /// - Current body completely engulfs previous body
    fn is_bullish_engulfing(&self, prev: &Bar) -> bool;

    /// Returns true if current bar engulfs the previous bar (bearish engulfing)
    ///
    /// Bearish Engulfing:
    /// - Previous bar is bullish
    /// - Current bar is bearish
    /// - Current body completely engulfs previous body
    fn is_bearish_engulfing(&self, prev: &Bar) -> bool;

    // === Three Bar Patterns ===

    /// Returns true if this forms a morning star pattern with two previous bars
    ///
    /// Morning Star (bullish reversal):
    /// 1. Large bearish candle
    /// 2. Small-bodied candle (gap down)
    /// 3. Large bullish candle
    fn is_morning_star(&self, prev1: &Bar, prev2: &Bar) -> bool;

    /// Returns true if this forms an evening star pattern with two previous bars
    ///
    /// Evening Star (bearish reversal):
    /// 1. Large bullish candle
    /// 2. Small-bodied candle (gap up)
    /// 3. Large bearish candle
    fn is_evening_star(&self, prev1: &Bar, prev2: &Bar) -> bool;
}

impl CandlePatterns for Bar {
    fn is_hammer(&self) -> bool {
        let body_pct = self.body_percentage();
        let lower = self.lower_wick();
        let upper = self.upper_wick();
        let body = self.body_height();

        body_pct < 0.3 && lower > 2.0 * body && upper < body * 0.5
    }

    fn is_inverted_hammer(&self) -> bool {
        let body_pct = self.body_percentage();
        let lower = self.lower_wick();
        let upper = self.upper_wick();
        let body = self.body_height();

        body_pct < 0.3 && upper > 2.0 * body && lower < body * 0.5
    }

    fn is_shooting_star(&self) -> bool {
        let body_pct = self.body_percentage();
        let lower = self.lower_wick();
        let upper = self.upper_wick();
        let body = self.body_height();

        body_pct < 0.3 && upper > 2.0 * body && lower < body * 0.3
    }

    fn is_spinning_top(&self) -> bool {
        let body_pct = self.body_percentage();
        let lower = self.lower_wick();
        let upper = self.upper_wick();

        if body_pct >= 0.3 {
            return false;
        }

        // Wicks should be similar length (within 50% of each other)
        let wick_ratio = if lower > upper {
            upper / lower
        } else if upper > 0.0 {
            lower / upper
        } else {
            return false;
        };

        wick_ratio > 0.5
    }

    fn is_marubozu(&self) -> bool {
        self.body_percentage() > 0.85
    }

    fn is_bullish_engulfing(&self, prev: &Bar) -> bool {
        if !prev.is_bearish() || !self.is_bullish() {
            return false;
        }

        let prev_body_top = prev.open;
        let prev_body_bottom = prev.close;
        let curr_body_top = self.close;
        let curr_body_bottom = self.open;

        curr_body_top > prev_body_top && curr_body_bottom < prev_body_bottom
    }

    fn is_bearish_engulfing(&self, prev: &Bar) -> bool {
        if !prev.is_bullish() || !self.is_bearish() {
            return false;
        }

        let prev_body_top = prev.close;
        let prev_body_bottom = prev.open;
        let curr_body_top = self.open;
        let curr_body_bottom = self.close;

        curr_body_top > prev_body_top && curr_body_bottom < prev_body_bottom
    }

    fn is_morning_star(&self, prev1: &Bar, prev2: &Bar) -> bool {
        // prev2 (oldest) should be bearish with large body
        if !prev2.is_bearish() || prev2.body_percentage() < 0.5 {
            return false;
        }

        // prev1 (middle) should have small body
        if prev1.body_percentage() > 0.3 {
            return false;
        }

        // Current should be bullish with large body
        if !self.is_bullish() || self.body_percentage() < 0.5 {
            return false;
        }

        // Current close should penetrate into first candle's body
        self.close > (prev2.open + prev2.close) / 2.0
    }

    fn is_evening_star(&self, prev1: &Bar, prev2: &Bar) -> bool {
        // prev2 (oldest) should be bullish with large body
        if !prev2.is_bullish() || prev2.body_percentage() < 0.5 {
            return false;
        }

        // prev1 (middle) should have small body
        if prev1.body_percentage() > 0.3 {
            return false;
        }

        // Current should be bearish with large body
        if !self.is_bearish() || self.body_percentage() < 0.5 {
            return false;
        }

        // Current close should penetrate into first candle's body
        self.close < (prev2.open + prev2.close) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_hammer_pattern() {
        // Hammer: small body at top, long lower wick, no upper wick
        // open=99, high=100, low=90, close=100 => body=1, range=10, lower_wick=9, upper_wick=0
        // body_pct=0.1 < 0.3 ✓, lower(9) > 2*body(2) ✓, upper(0) < body*0.5(0.5) ✓
        let hammer = Bar::new(Utc::now(), 99.0, 100.0, 90.0, 100.0, 1000.0);
        assert!(hammer.is_hammer());

        // Not a hammer: equal wicks (body in middle of range)
        let not_hammer = Bar::new(Utc::now(), 95.0, 100.0, 90.0, 95.0, 1000.0);
        assert!(!not_hammer.is_hammer());
    }

    #[test]
    fn test_marubozu_pattern() {
        // Marubozu: body is almost the entire range
        let marubozu = Bar::new(Utc::now(), 100.0, 110.0, 100.0, 110.0, 1000.0);
        assert!(marubozu.is_marubozu());

        // Not marubozu: significant wicks
        let not_marubozu = Bar::new(Utc::now(), 102.0, 110.0, 100.0, 108.0, 1000.0);
        assert!(!not_marubozu.is_marubozu());
    }

    #[test]
    fn test_engulfing_patterns() {
        let bearish_bar = Bar::new(Utc::now(), 105.0, 107.0, 99.0, 100.0, 1000.0);
        let bullish_engulfing = Bar::new(Utc::now(), 98.0, 108.0, 97.0, 106.0, 1000.0);

        assert!(bullish_engulfing.is_bullish_engulfing(&bearish_bar));
        assert!(!bullish_engulfing.is_bearish_engulfing(&bearish_bar));
    }
}
