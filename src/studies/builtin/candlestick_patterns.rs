//! All Candlestick Patterns
//!
//! Detects the common Japanese candlestick patterns described by Steve Nison and Thomas
//! Bulkowski, filtered by the prevailing trend, and marks each bar where one completes.
//!
//! # Definitions
//!
//! Each bar has a body from `min(open, close)` to `max(open, close)`, an upper shadow
//! `high - max(open, close)`, a lower shadow `min(open, close) - low` and a range
//! `high - low`. The body average is the EMA of the body over 14 bars, seeded with their
//! SMA. A body is long above the average and small below it. A bar is white when its close
//! is above its open and black when below. A doji body is at most 5% of a nonzero range;
//! a doji has a doji body and shadows within twice each other's length. A shadow is
//! present when it exceeds 5% of the body. Prices are equal when they differ by at most
//! 5% of the body average.
//!
//! The trend rule decides whether a bar is in a downtrend or an uptrend: `Sma50` compares
//! the close with SMA(close, 50) (below is down, above is up), `Sma50Sma200` also requires
//! SMA(close, 50) to be below or above SMA(close, 200), and `NoDetection` treats every bar
//! as both. Before the averages exist no bar is in either trend. A pattern that requires a
//! trend checks it on the bar named below.
//!
//! | Abbr. | Pattern | Side | Rule (`b` the bar, `p1`, `p2`, ... the bars before it) |
//! |---|---|---|---|
//! | AB | Abandoned Baby | bearish | `p2` white in an uptrend, `p1` a doji body whose low gaps above `p2`'s high, `b` black whose high gaps below `p1`'s low |
//! | AB | Abandoned Baby | bullish | mirror: `p2` black in a downtrend, `p1` doji body gapping below, `b` white gapping above |
//! | DCC | Dark Cloud Cover | bearish | `p1` long white in an uptrend; `b` black, opening at or above `p1`'s high, closing below the middle of `p1`'s body and above its open |
//! | D | Doji | neutral | a doji that is neither a dragonfly nor a gravestone doji |
//! | DS | Doji Star | bearish | `b` in an uptrend; `p1` long white; `b` a doji body gapping above `p1`'s body |
//! | DS | Doji Star | bullish | `b` in a downtrend; `p1` long black; `b` a doji body gapping below `p1`'s body |
//! | DTG | Downside Tasuki Gap | bearish | `b` in a downtrend; `p2` long black; `p1` small black, its body gapping below `p2`'s; `b` white, opening inside `p1`'s body and closing inside the gap |
//! | DD | Dragonfly Doji | bullish | a doji body with an upper shadow no longer than the body |
//! | BE | Engulfing | bearish | `b` long black in an uptrend; `p1` small white; `b`'s body covers `p1`'s, extending past at least one end |
//! | BE | Engulfing | bullish | `b` long white in a downtrend; `p1` small black; `b`'s body covers `p1`'s, extending past at least one end |
//! | EDS | Evening Doji Star | bearish | as Evening Star with a doji body for `p1` |
//! | ES | Evening Star | bearish | `b` in an uptrend; `p2` long white; `p1` small, its body gapping above `p2`'s; `b` long black, its body gapping below `p1`'s and closing into the lower half of `p2`'s body |
//! | FTM | Falling Three Methods | bearish | `p4` long black in a downtrend; `p3`, `p2`, `p1` small white, opening and closing within `p4`'s range; `b` long black closing below `p4`'s close |
//! | FW | Falling Window | bearish | `p1` in a downtrend; `b`'s high below `p1`'s low, neither range zero |
//! | GD | Gravestone Doji | bearish | a doji body with a lower shadow no longer than the body |
//! | H | Hammer | bullish | in a downtrend: a small nonzero body above the range midpoint, lower shadow at least twice the body, no upper shadow |
//! | HM | Hanging Man | bearish | the hammer shape in an uptrend |
//! | BH | Harami | bearish | `p1` long white in an uptrend; `b` small black within `p1`'s body, shadows included |
//! | BH | Harami | bullish | `p1` long black in a downtrend; `b` small white within `p1`'s body, shadows included |
//! | HC | Harami Cross | bearish | as the bearish Harami with a doji body for `b` |
//! | HC | Harami Cross | bullish | as the bullish Harami with a doji body for `b` |
//! | IH | Inverted Hammer | bullish | in a downtrend: a small nonzero body below the range midpoint, upper shadow at least twice the body, no lower shadow |
//! | K | Kicking | bearish | `p1` a white marubozu, `b` a black marubozu whose high gaps below `p1`'s low |
//! | K | Kicking | bullish | `p1` a black marubozu, `b` a white marubozu whose low gaps above `p1`'s high |
//! | LLS | Long Lower Shadow | bullish | lower shadow longer than 75% of the range |
//! | LUS | Long Upper Shadow | bearish | upper shadow longer than 75% of the range |
//! | MB | Marubozu Black | bearish | long black body, each shadow at most 5% of the body |
//! | MW | Marubozu White | bullish | long white body, each shadow at most 5% of the body |
//! | MDS | Morning Doji Star | bullish | as Morning Star with a doji body for `p1` |
//! | MS | Morning Star | bullish | `b` in a downtrend; `p2` long black; `p1` small, its body gapping below `p2`'s; `b` long white, its body gapping above `p1`'s and closing into the upper half of `p2`'s body |
//! | ON | On Neck | bearish | `b` in a downtrend; `p1` long black; `b` small white, opening below `p1`'s close and closing equal to `p1`'s low |
//! | P | Piercing | bullish | `p1` long black in a downtrend; `b` white, opening at or below `p1`'s low, closing above the middle of `p1`'s body and below its open |
//! | RTM | Rising Three Methods | bullish | `p4` long white in an uptrend; `p3`, `p2`, `p1` small black, opening and closing within `p4`'s range; `b` long white closing above `p4`'s close |
//! | RW | Rising Window | bullish | `p1` in an uptrend; `b`'s low above `p1`'s high, neither range zero |
//! | SS | Shooting Star | bearish | the inverted hammer shape in an uptrend |
//! | STB | Spinning Top Black | neutral | black, not a doji body, each shadow at least 34% of the range |
//! | STW | Spinning Top White | neutral | white, not a doji body, each shadow at least 34% of the range |
//! | 3BC | Three Black Crows | bearish | three long black bars, each closing lower, opening within the previous body, with a lower shadow under 5% of its range |
//! | 3WS | Three White Soldiers | bullish | three long white bars, each closing higher, opening within the previous body, with an upper shadow under 5% of its range |
//! | 3S | Tri-Star | bearish | three dojis, `p2` in an uptrend, `p1`'s body gapping above both neighbours' bodies |
//! | 3S | Tri-Star | bullish | three dojis, `p2` in a downtrend, `p1`'s body gapping below both neighbours' bodies |
//! | TB | Tweezer Bottom | bullish | `p1` long black in a downtrend; `b` white with a low equal to `p1`'s, not a doji body unless it has both shadows |
//! | TT | Tweezer Top | bearish | `p1` long white in an uptrend; `b` black with a high equal to `p1`'s, not a doji body unless it has both shadows |
//! | UTG | Upside Tasuki Gap | bullish | `b` in an uptrend; `p2` long white; `p1` small white, its body gapping above `p2`'s; `b` black, opening inside `p1`'s body and closing inside the gap |
//!
//! # Outputs
//!
//! Three lines: Bullish at the low of a bar with a bullish pattern (an upward triangle),
//! Bearish at the high of a bar with a bearish one (a downward triangle) and Neutral at the
//! high of a bar with a doji or spinning top (a circle). Each mark is labelled with the
//! abbreviations of the patterns found on the bar, one per line; on a bar with a bearish
//! pattern the neutral ones join the bearish label, so the two never overlap. Every pattern has a toggle,
//! all on by default, and the pattern type marks both sides (the default, neutral patterns
//! included) or only the bullish or only the bearish patterns.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{CandlestickPatterns, Indicator, PatternTrend};
//!
//! let mut patterns = CandlestickPatterns::new(PatternTrend::Sma50Sma200);
//! patterns.calculate(&bars);
//! let labels: Vec<String> = (0..bars.len()).filter_map(|bar| patterns.plot_text(0, bar)).collect();
//! ```

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// How [`CandlestickPatterns`] decides whether a bar is in a downtrend or an uptrend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PatternTrend {
    /// The close below or above SMA(close, 50).
    #[default]
    Sma50,
    /// The close below SMA(close, 50) below SMA(close, 200), or the reverse.
    Sma50Sma200,
    /// Every bar counts as both, so trend conditions always hold.
    NoDetection,
}

/// Which signals [`CandlestickPatterns`] marks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PatternType {
    /// Bullish, bearish and neutral patterns.
    #[default]
    Both,
    /// Bullish patterns only.
    Bullish,
    /// Bearish patterns only.
    Bearish,
}

/// Shadows at most this fraction of the body are absent; also the marubozu tolerance.
const SHADOW: f64 = 0.05;
/// A doji body is at most this fraction of the range.
const DOJI_BODY: f64 = 0.05;
/// Prices within this fraction of the body average are equal (tweezers, on neck).
const EQUAL: f64 = 0.05;
/// How many times the body a hammer's or shooting star's long shadow is at least.
const FACTOR: f64 = 2.0;
/// Bars in the body average.
const BODY_AVERAGE: usize = 14;

/// One bar with its body average and trend, the inputs every rule reads.
#[derive(Clone, Copy)]
struct Candle {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    average: f64,
    down: bool,
    up: bool,
}

impl Candle {
    fn top(&self) -> f64 {
        self.open.max(self.close)
    }
    fn bottom(&self) -> f64 {
        self.open.min(self.close)
    }
    fn body(&self) -> f64 {
        self.top() - self.bottom()
    }
    fn middle(&self) -> f64 {
        self.bottom() + self.body() / 2.0
    }
    fn upper(&self) -> f64 {
        self.high - self.top()
    }
    fn lower(&self) -> f64 {
        self.bottom() - self.low
    }
    fn range(&self) -> f64 {
        self.high - self.low
    }
    fn white(&self) -> bool {
        self.open < self.close
    }
    fn black(&self) -> bool {
        self.open > self.close
    }
    fn long(&self) -> bool {
        self.body() > self.average
    }
    fn small(&self) -> bool {
        self.body() < self.average
    }
    fn has_upper(&self) -> bool {
        self.upper() > SHADOW * self.body()
    }
    fn has_lower(&self) -> bool {
        self.lower() > SHADOW * self.body()
    }
    fn doji_body(&self) -> bool {
        self.range() > 0.0 && self.body() <= self.range() * DOJI_BODY
    }
    fn doji(&self) -> bool {
        let (upper, lower) = (self.upper(), self.lower());
        // Each shadow differs from the other by less than its own length.
        self.doji_body()
            && (upper == lower || ((upper - lower).abs() < lower && (lower - upper).abs() < upper))
    }
    fn marubozu(&self) -> bool {
        self.long() && self.upper() <= SHADOW * self.body() && self.lower() <= SHADOW * self.body()
    }
    /// The hammer and hanging man shape.
    fn hammer(&self) -> bool {
        self.small()
            && self.body() > 0.0
            && self.bottom() > (self.high + self.low) / 2.0
            && self.lower() >= FACTOR * self.body()
            && !self.has_upper()
    }
    /// The inverted hammer and shooting star shape.
    fn inverted_hammer(&self) -> bool {
        self.small()
            && self.body() > 0.0
            && self.top() < (self.high + self.low) / 2.0
            && self.upper() >= FACTOR * self.body()
            && !self.has_lower()
    }
    fn spinning_top(&self) -> bool {
        self.lower() >= self.range() * 0.34
            && self.upper() >= self.range() * 0.34
            && !self.doji_body()
    }
    fn equal(&self, a: f64, b: f64) -> bool {
        (a - b).abs() <= self.average * EQUAL
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Bullish,
    Bearish,
    Neutral,
}

/// A pattern's toggle label, chart abbreviation, side, bar count and rule over exactly
/// that many bars, oldest first.
struct Pattern {
    label: &'static str,
    abbr: &'static str,
    side: Side,
    bars: usize,
    found: fn(&[Candle]) -> bool,
}

const fn pattern(
    label: &'static str,
    abbr: &'static str,
    side: Side,
    bars: usize,
    found: fn(&[Candle]) -> bool,
) -> Pattern {
    Pattern {
        label,
        abbr,
        side,
        bars,
        found,
    }
}

use Side::{Bearish, Bullish, Neutral};

/// Every pattern, in the order of the toggles.
const PATTERNS: [Pattern; 44] = [
    pattern("Abandoned Baby - Bearish", "AB", Bearish, 3, |c| {
        matches!(c, [p2, p1, b] if p2.up && p2.white() && p1.doji_body() && p2.high < p1.low
            && b.black() && p1.low > b.high)
    }),
    pattern("Abandoned Baby - Bullish", "AB", Bullish, 3, |c| {
        matches!(c, [p2, p1, b] if p2.down && p2.black() && p1.doji_body() && p2.low > p1.high
            && b.white() && p1.high < b.low)
    }),
    pattern("Dark Cloud Cover - Bearish", "DCC", Bearish, 2, |c| {
        matches!(c, [p, b] if p.up && p.white() && p.long() && b.black() && b.open >= p.high
            && b.close < p.middle() && b.close > p.open)
    }),
    pattern(
        "Doji",
        "D",
        Neutral,
        1,
        |c| matches!(c, [b] if b.doji() && b.upper() > b.body() && b.lower() > b.body()),
    ),
    pattern(
        "Doji Star - Bearish",
        "DS",
        Bearish,
        2,
        |c| matches!(c, [p, b] if b.up && p.white() && p.long() && b.doji_body() && b.bottom() > p.top()),
    ),
    pattern(
        "Doji Star - Bullish",
        "DS",
        Bullish,
        2,
        |c| matches!(c, [p, b] if b.down && p.black() && p.long() && b.doji_body() && b.top() < p.bottom()),
    ),
    pattern("Downside Tasuki Gap - Bearish", "DTG", Bearish, 3, |c| {
        matches!(c, [p2, p1, b] if b.down && p2.long() && p2.black() && p1.small() && p1.black()
            && p1.top() < p2.bottom() && b.white() && b.open > p1.bottom() && b.open < p1.top()
            && b.close > p1.top() && b.close < p2.bottom())
    }),
    pattern(
        "Dragonfly Doji - Bullish",
        "DD",
        Bullish,
        1,
        |c| matches!(c, [b] if b.doji_body() && b.upper() <= b.body()),
    ),
    pattern("Engulfing - Bearish", "BE", Bearish, 2, |c| {
        matches!(c, [p, b] if b.up && b.black() && b.long() && p.white() && p.small()
            && b.close <= p.open && b.open >= p.close && (b.close < p.open || b.open > p.close))
    }),
    pattern("Engulfing - Bullish", "BE", Bullish, 2, |c| {
        matches!(c, [p, b] if b.down && b.white() && b.long() && p.black() && p.small()
            && b.close >= p.open && b.open <= p.close && (b.close > p.open || b.open < p.close))
    }),
    pattern(
        "Evening Doji Star - Bearish",
        "EDS",
        Bearish,
        3,
        |c| matches!(c, [p2, p1, b] if p1.doji_body() && evening_star(p2, p1, b)),
    ),
    pattern(
        "Evening Star - Bearish",
        "ES",
        Bearish,
        3,
        |c| matches!(c, [p2, p1, b] if p1.small() && evening_star(p2, p1, b)),
    ),
    pattern("Falling Three Methods - Bearish", "FTM", Bearish, 5, |c| {
        matches!(c, [p4, inner @ .., b] if p4.down && p4.long() && p4.black()
            && inner.iter().all(|p| p.small() && p.white() && p.open > p4.low && p.close < p4.high)
            && b.long() && b.black() && b.close < p4.close)
    }),
    pattern(
        "Falling Window - Bearish",
        "FW",
        Bearish,
        2,
        |c| matches!(c, [p, b] if p.down && p.range() != 0.0 && b.range() != 0.0 && b.high < p.low),
    ),
    pattern(
        "Gravestone Doji - Bearish",
        "GD",
        Bearish,
        1,
        |c| matches!(c, [b] if b.doji_body() && b.lower() <= b.body()),
    ),
    pattern(
        "Hammer - Bullish",
        "H",
        Bullish,
        1,
        |c| matches!(c, [b] if b.down && b.hammer()),
    ),
    pattern(
        "Hanging Man - Bearish",
        "HM",
        Bearish,
        1,
        |c| matches!(c, [b] if b.up && b.hammer()),
    ),
    pattern(
        "Harami - Bearish",
        "BH",
        Bearish,
        2,
        |c| matches!(c, [p, b] if p.up && p.long() && p.white() && b.black() && b.small() && inside(p, b)),
    ),
    pattern(
        "Harami - Bullish",
        "BH",
        Bullish,
        2,
        |c| matches!(c, [p, b] if p.down && p.long() && p.black() && b.white() && b.small() && inside(p, b)),
    ),
    pattern(
        "Harami Cross - Bearish",
        "HC",
        Bearish,
        2,
        |c| matches!(c, [p, b] if p.up && p.long() && p.white() && b.doji_body() && inside(p, b)),
    ),
    pattern(
        "Harami Cross - Bullish",
        "HC",
        Bullish,
        2,
        |c| matches!(c, [p, b] if p.down && p.long() && p.black() && b.doji_body() && inside(p, b)),
    ),
    pattern(
        "Inverted Hammer - Bullish",
        "IH",
        Bullish,
        1,
        |c| matches!(c, [b] if b.down && b.inverted_hammer()),
    ),
    pattern(
        "Kicking - Bearish",
        "K",
        Bearish,
        2,
        |c| matches!(c, [p, b] if p.white() && p.marubozu() && b.black() && b.marubozu() && p.low > b.high),
    ),
    pattern(
        "Kicking - Bullish",
        "K",
        Bullish,
        2,
        |c| matches!(c, [p, b] if p.black() && p.marubozu() && b.white() && b.marubozu() && p.high < b.low),
    ),
    pattern(
        "Long Lower Shadow - Bullish",
        "LLS",
        Bullish,
        1,
        |c| matches!(c, [b] if b.lower() > b.range() * 0.75),
    ),
    pattern(
        "Long Upper Shadow - Bearish",
        "LUS",
        Bearish,
        1,
        |c| matches!(c, [b] if b.upper() > b.range() * 0.75),
    ),
    pattern(
        "Marubozu Black - Bearish",
        "MB",
        Bearish,
        1,
        |c| matches!(c, [b] if b.black() && b.marubozu()),
    ),
    pattern(
        "Marubozu White - Bullish",
        "MW",
        Bullish,
        1,
        |c| matches!(c, [b] if b.white() && b.marubozu()),
    ),
    pattern(
        "Morning Doji Star - Bullish",
        "MDS",
        Bullish,
        3,
        |c| matches!(c, [p2, p1, b] if p1.doji_body() && morning_star(p2, p1, b)),
    ),
    pattern(
        "Morning Star - Bullish",
        "MS",
        Bullish,
        3,
        |c| matches!(c, [p2, p1, b] if p1.small() && morning_star(p2, p1, b)),
    ),
    pattern("On Neck - Bearish", "ON", Bearish, 2, |c| {
        matches!(c, [p, b] if b.down && p.black() && p.long() && b.white() && b.small()
            && b.open < p.close && b.range() != 0.0 && b.equal(b.close, p.low))
    }),
    pattern("Piercing - Bullish", "P", Bullish, 2, |c| {
        matches!(c, [p, b] if p.down && p.black() && p.long() && b.white() && b.open <= p.low
            && b.close > p.middle() && b.close < p.open)
    }),
    pattern("Rising Three Methods - Bullish", "RTM", Bullish, 5, |c| {
        matches!(c, [p4, inner @ .., b] if p4.up && p4.long() && p4.white()
            && inner.iter().all(|p| p.small() && p.black() && p.open < p4.high && p.close > p4.low)
            && b.long() && b.white() && b.close > p4.close)
    }),
    pattern(
        "Rising Window - Bullish",
        "RW",
        Bullish,
        2,
        |c| matches!(c, [p, b] if p.up && p.range() != 0.0 && b.range() != 0.0 && b.low > p.high),
    ),
    pattern(
        "Shooting Star - Bearish",
        "SS",
        Bearish,
        1,
        |c| matches!(c, [b] if b.up && b.inverted_hammer()),
    ),
    pattern(
        "Spinning Top Black",
        "STB",
        Neutral,
        1,
        |c| matches!(c, [b] if b.black() && b.spinning_top()),
    ),
    pattern(
        "Spinning Top White",
        "STW",
        Neutral,
        1,
        |c| matches!(c, [b] if b.white() && b.spinning_top()),
    ),
    pattern("Three Black Crows - Bearish", "3BC", Bearish, 3, |c| {
        matches!(c, [p2, p1, b] if [p2, p1, b].iter().all(|x| x.long() && x.black() && x.lower() < x.range() * SHADOW)
            && b.close < p1.close && p1.close < p2.close
            && b.open > p1.close && b.open < p1.open && p1.open > p2.close && p1.open < p2.open)
    }),
    pattern("Three White Soldiers - Bullish", "3WS", Bullish, 3, |c| {
        matches!(c, [p2, p1, b] if [p2, p1, b].iter().all(|x| x.long() && x.white() && x.upper() < x.range() * SHADOW)
            && b.close > p1.close && p1.close > p2.close
            && b.open < p1.close && b.open > p1.open && p1.open < p2.close && p1.open > p2.open)
    }),
    pattern("Tri-Star - Bearish", "3S", Bearish, 3, |c| {
        matches!(c, [p2, p1, b] if p2.doji() && p1.doji() && b.doji() && p2.up
            && p1.bottom() > p2.top() && p1.bottom() > b.top())
    }),
    pattern("Tri-Star - Bullish", "3S", Bullish, 3, |c| {
        matches!(c, [p2, p1, b] if p2.doji() && p1.doji() && b.doji() && p2.down
            && p1.top() < p2.bottom() && p1.top() < b.bottom())
    }),
    pattern("Tweezer Bottom - Bullish", "TB", Bullish, 2, |c| {
        matches!(c, [p, b] if p.down && p.long() && p.black() && b.white()
            && (!b.doji_body() || (b.has_upper() && b.has_lower())) && b.equal(b.low, p.low))
    }),
    pattern("Tweezer Top - Bearish", "TT", Bearish, 2, |c| {
        matches!(c, [p, b] if p.up && p.long() && p.white() && b.black()
            && (!b.doji_body() || (b.has_upper() && b.has_lower())) && b.equal(b.high, p.high))
    }),
    pattern("Upside Tasuki Gap - Bullish", "UTG", Bullish, 3, |c| {
        matches!(c, [p2, p1, b] if b.up && p2.long() && p2.white() && p1.small() && p1.white()
            && p1.bottom() > p2.top() && b.black() && b.open > p1.bottom() && b.open < p1.top()
            && b.close > p2.top() && b.close < p1.bottom())
    }),
];

/// `b`'s range, shadows included, within `p`'s body.
fn inside(p: &Candle, b: &Candle) -> bool {
    b.high <= p.top() && b.low >= p.bottom()
}

/// The evening star rule after the middle bar's body test.
fn evening_star(p2: &Candle, p1: &Candle, b: &Candle) -> bool {
    b.up && p2.long()
        && p2.white()
        && p1.bottom() > p2.top()
        && b.long()
        && b.black()
        && b.bottom() <= p2.middle()
        && b.bottom() > p2.bottom()
        && p1.bottom() > b.top()
}

/// The morning star rule after the middle bar's body test.
fn morning_star(p2: &Candle, p1: &Candle, b: &Candle) -> bool {
    b.down
        && p2.long()
        && p2.black()
        && p1.top() < p2.bottom()
        && b.long()
        && b.white()
        && b.top() >= p2.middle()
        && b.top() < p2.top()
        && p1.top() < b.bottom()
}

/// All Candlestick Patterns: the common candlestick patterns, each with a toggle, marked
/// and labelled at the bar that completes them.
#[derive(Clone)]
pub struct CandlestickPatterns {
    trend: PatternTrend,
    pattern_type: PatternType,
    patterns: [bool; PATTERNS.len()],
    /// One bit per pattern of [`PATTERNS`] found on each source bar.
    found: Vec<u64>,
    values: IndicatorValues,
    stepper: Stepper<State>,
    colors: [Color32; 3],
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    closes: std::collections::VecDeque<f64>,
    bodies: std::collections::VecDeque<f64>,
    average: Option<f64>,
    candles: std::collections::VecDeque<Candle>,
}

impl CandlestickPatterns {
    /// Detect every pattern with the given trend rule.
    pub fn new(trend: PatternTrend) -> Self {
        Self {
            trend,
            pattern_type: PatternType::Both,
            patterns: [true; PATTERNS.len()],
            found: Vec::new(),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: [palette::BLUE, palette::RED, palette::GRAY],
            visible: true,
        }
    }

    /// Mark both sides with neutral patterns, or only bullish or only bearish patterns.
    pub fn with_pattern_type(mut self, pattern_type: PatternType) -> Self {
        self.pattern_type = pattern_type;
        self
    }

    /// Detect only the patterns whose label, such as `"Hammer - Bullish"` or `"Doji"`,
    /// `keep` accepts.
    pub fn with_patterns(mut self, keep: impl Fn(&str) -> bool) -> Self {
        for (on, pattern) in self.patterns.iter_mut().zip(&PATTERNS) {
            *on = keep(pattern.label);
        }
        self
    }

    /// The patterns found on a source bar, from the last calculation.
    fn found(&self, bar: usize, side: Side) -> impl Iterator<Item = &'static Pattern> + '_ {
        let bits = self.found.get(bar).copied().unwrap_or(0);
        PATTERNS
            .iter()
            .enumerate()
            .filter(move |(k, p)| bits >> k & 1 == 1 && p.side == side)
            .map(|(_, p)| p)
    }
}

/// Construct with the conventional default trend rule and every pattern on.
impl Default for CandlestickPatterns {
    fn default() -> Self {
        Self::new(PatternTrend::Sma50)
    }
}

impl Indicator for CandlestickPatterns {
    crate::studies::inputs::study_inputs!(self;
        trend: choice("Detect trend based on", [PatternTrend::Sma50, PatternTrend::Sma50Sma200, PatternTrend::NoDetection]),
        pattern_type: choice("Pattern type", [PatternType::Both, PatternType::Bullish, PatternType::Bearish]),
        patterns[0]: toggle(PATTERNS[0].label),
        patterns[1]: toggle(PATTERNS[1].label),
        patterns[2]: toggle(PATTERNS[2].label),
        patterns[3]: toggle(PATTERNS[3].label),
        patterns[4]: toggle(PATTERNS[4].label),
        patterns[5]: toggle(PATTERNS[5].label),
        patterns[6]: toggle(PATTERNS[6].label),
        patterns[7]: toggle(PATTERNS[7].label),
        patterns[8]: toggle(PATTERNS[8].label),
        patterns[9]: toggle(PATTERNS[9].label),
        patterns[10]: toggle(PATTERNS[10].label),
        patterns[11]: toggle(PATTERNS[11].label),
        patterns[12]: toggle(PATTERNS[12].label),
        patterns[13]: toggle(PATTERNS[13].label),
        patterns[14]: toggle(PATTERNS[14].label),
        patterns[15]: toggle(PATTERNS[15].label),
        patterns[16]: toggle(PATTERNS[16].label),
        patterns[17]: toggle(PATTERNS[17].label),
        patterns[18]: toggle(PATTERNS[18].label),
        patterns[19]: toggle(PATTERNS[19].label),
        patterns[20]: toggle(PATTERNS[20].label),
        patterns[21]: toggle(PATTERNS[21].label),
        patterns[22]: toggle(PATTERNS[22].label),
        patterns[23]: toggle(PATTERNS[23].label),
        patterns[24]: toggle(PATTERNS[24].label),
        patterns[25]: toggle(PATTERNS[25].label),
        patterns[26]: toggle(PATTERNS[26].label),
        patterns[27]: toggle(PATTERNS[27].label),
        patterns[28]: toggle(PATTERNS[28].label),
        patterns[29]: toggle(PATTERNS[29].label),
        patterns[30]: toggle(PATTERNS[30].label),
        patterns[31]: toggle(PATTERNS[31].label),
        patterns[32]: toggle(PATTERNS[32].label),
        patterns[33]: toggle(PATTERNS[33].label),
        patterns[34]: toggle(PATTERNS[34].label),
        patterns[35]: toggle(PATTERNS[35].label),
        patterns[36]: toggle(PATTERNS[36].label),
        patterns[37]: toggle(PATTERNS[37].label),
        patterns[38]: toggle(PATTERNS[38].label),
        patterns[39]: toggle(PATTERNS[39].label),
        patterns[40]: toggle(PATTERNS[40].label),
        patterns[41]: toggle(PATTERNS[41].label),
        patterns[42]: toggle(PATTERNS[42].label),
        patterns[43]: toggle(PATTERNS[43].label)
    ; {});

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line.min(2)],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: match line {
                0 => crate::studies::PlotType::TriangleUp,
                1 => crate::studies::PlotType::TriangleDown,
                _ => crate::studies::PlotType::Circles,
            },
            visible: true,
        }
    }

    fn plot_text(&self, line: usize, bar: usize) -> Option<String> {
        let side = [Bullish, Bearish, Neutral].get(line).copied()?;
        let bearish = self.found(bar, Bearish).next().is_some();
        let text: Vec<&str> = match side {
            // The bearish and neutral marks share the high, so one label names both.
            Bearish => self
                .found(bar, Bearish)
                .chain(self.found(bar, Neutral))
                .map(|p| p.abbr)
                .collect(),
            Neutral if bearish => Vec::new(),
            _ => self.found(bar, side).map(|p| p.abbr).collect(),
        };
        (!text.is_empty()).then(|| text.join("\n"))
    }

    fn name(&self) -> &str {
        "All Candlestick Patterns"
    }

    fn desc(&self) -> &str {
        "All Candlestick Patterns - Japanese candlestick patterns with a trend filter"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let len = self.values.len();
        let start = if len > 0 && from >= len - 1 && data.len() >= len {
            from.min(len)
        } else {
            0
        };
        self.found.truncate(start);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if ![bar.open, bar.high, bar.low, bar.close]
                    .iter()
                    .all(|v| v.is_finite())
                {
                    self.found.push(0);
                    return IndicatorValue::None;
                }
                state.closes.push_back(bar.close);
                if state.closes.len() > 200 {
                    state.closes.pop_front();
                }
                let sma = |period| {
                    if state.closes.len() < period {
                        f64::NAN
                    } else {
                        state
                            .closes
                            .iter()
                            .skip(state.closes.len() - period)
                            .sum::<f64>()
                            / period as f64
                    }
                };
                let (down, up) = match self.trend {
                    PatternTrend::Sma50 => {
                        let mean = sma(50);
                        (bar.close < mean, bar.close > mean)
                    }
                    PatternTrend::Sma50Sma200 => {
                        let (fast, slow) = (sma(50), sma(200));
                        (
                            bar.close < fast && fast < slow,
                            bar.close > fast && fast > slow,
                        )
                    }
                    PatternTrend::NoDetection => (true, true),
                };
                let body = (bar.close - bar.open).abs();
                state.bodies.push_back(body);
                if state.bodies.len() > BODY_AVERAGE {
                    state.bodies.pop_front();
                }
                let average = match state.average {
                    Some(previous) => {
                        previous + (2.0 / (BODY_AVERAGE + 1) as f64) * (body - previous)
                    }
                    None if state.bodies.len() == BODY_AVERAGE => {
                        if state.bodies.iter().all(|v| v.is_finite()) {
                            state.bodies.iter().sum::<f64>() / BODY_AVERAGE as f64
                        } else {
                            f64::NAN
                        }
                    }
                    None => f64::NAN,
                };
                if state.bodies.len() == BODY_AVERAGE {
                    state.average = Some(average);
                }
                state.candles.push_back(Candle {
                    open: bar.open,
                    high: bar.high,
                    low: bar.low,
                    close: bar.close,
                    average,
                    down,
                    up,
                });
                let max_bars = PATTERNS.iter().map(|pattern| pattern.bars).max().unwrap();
                if state.candles.len() > max_bars {
                    state.candles.pop_front();
                }
                let candles = state.candles.make_contiguous();
                let marked = |side| match self.pattern_type {
                    PatternType::Both => true,
                    PatternType::Bullish => side == Bullish,
                    PatternType::Bearish => side == Bearish,
                };
                let mut bits = 0;
                for (bit, pattern) in PATTERNS.iter().enumerate() {
                    if self.patterns[bit]
                        && marked(pattern.side)
                        && candles.len() >= pattern.bars
                        && (pattern.found)(&candles[candles.len() - pattern.bars..])
                    {
                        bits |= 1 << bit;
                    }
                }
                self.found.push(bits);
                let found = |side| {
                    PATTERNS
                        .iter()
                        .enumerate()
                        .any(|(bit, pattern)| bits >> bit & 1 == 1 && pattern.side == side)
                };
                let (bull, bear, neutral) = (found(Bullish), found(Bearish), found(Neutral));
                IndicatorValue::Multiple(vec![
                    if bull { bar.low } else { f64::NAN },
                    if bear { bar.high } else { f64::NAN },
                    if neutral && !bear { bar.high } else { f64::NAN },
                ])
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (slot, color) in self.colors.iter_mut().zip(colors) {
            *slot = color;
        }
    }

    fn line_names(&self) -> Vec<String> {
        vec!["Bullish".into(), "Bearish".into(), "Neutral".into()]
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    type Ohlc = (f64, f64, f64, f64);
    /// Label, line, abbreviation, bars that form the pattern and a near miss.
    type Case = (
        &'static str,
        usize,
        &'static str,
        &'static [Ohlc],
        &'static [Ohlc],
    );

    /// Bars of body 1 through `closes`, each opening one point away from its close.
    fn walk(closes: impl IntoIterator<Item = f64>) -> Vec<Bar> {
        let mut prev: Option<f64> = None;
        closes
            .into_iter()
            .map(|close| {
                let open = prev.unwrap_or(close - 1.0);
                prev = Some(close);
                let (top, bottom) = (open.max(close), open.min(close));
                Bar::new(Utc::now(), open, top + 0.25, bottom - 0.25, close, 1.0)
            })
            .collect()
    }

    /// The text on `line` of the last of `bars` after `before`, with only `label` on.
    fn text(
        trend: PatternTrend,
        before: &[Bar],
        bars: &[Ohlc],
        label: &str,
        line: usize,
    ) -> Option<String> {
        let mut data = before.to_vec();
        data.extend(
            bars.iter()
                .map(|&(o, h, l, c)| Bar::new(Utc::now(), o, h, l, c, 1.0)),
        );
        let mut study =
            CandlestickPatterns::new(trend).with_patterns(|p| label.is_empty() || p == label);
        study.calculate(&data);
        study.plot_text(line, data.len() - 1)
    }

    /// Each pattern, after bars of body 1 that set the body average and without a trend
    /// filter, is marked on its own line with its abbreviation, and the near miss, which
    /// breaks one rule, is not.
    #[test]
    fn patterns_match_their_definitions() {
        let flat = walk((0..14).map(|i| 100.0 + f64::from(i % 2)));
        #[rustfmt::skip]
        let cases: [Case; 44] = [
            ("Abandoned Baby - Bearish", 1, "AB",
                &[(100.0, 103.0, 99.5, 102.5), (104.0, 105.0, 103.5, 104.0625), (103.0, 103.25, 100.0, 100.5)],
                &[(100.0, 103.0, 99.5, 102.5), (104.0, 105.0, 103.5, 104.0625), (103.0, 103.5, 100.0, 100.5)]),
            ("Abandoned Baby - Bullish", 0, "AB",
                &[(102.5, 103.0, 99.5, 100.0), (98.5, 99.0, 97.5, 98.5625), (99.5, 102.5, 99.25, 102.0)],
                &[(102.5, 103.0, 99.5, 100.0), (98.5, 99.0, 97.5, 98.5625), (99.5, 102.5, 99.0, 102.0)]),
            ("Dark Cloud Cover - Bearish", 1, "DCC",
                &[(100.0, 103.5, 99.5, 103.0), (104.0, 104.5, 101.0, 101.25)],
                &[(100.0, 103.5, 99.5, 103.0), (104.0, 104.5, 101.5, 101.75)]),
            ("Doji", 2, "D", &[(100.0, 102.0, 98.0, 100.0625)], &[(100.0, 102.0, 98.0, 100.5)]),
            ("Doji Star - Bearish", 1, "DS",
                &[(100.0, 103.25, 99.75, 103.0), (103.5, 104.5, 102.5, 103.5625)],
                &[(100.0, 103.25, 99.75, 103.0), (102.75, 103.75, 101.75, 102.8125)]),
            ("Doji Star - Bullish", 0, "DS",
                &[(103.0, 103.25, 99.75, 100.0), (99.5, 100.5, 98.5, 99.4375)],
                &[(103.0, 103.25, 99.75, 100.0), (100.25, 101.25, 99.25, 100.1875)]),
            ("Downside Tasuki Gap - Bearish", 1, "DTG",
                &[(104.0, 104.25, 99.75, 100.0), (99.5, 99.75, 98.75, 99.0), (99.25, 100.25, 98.75, 99.75)],
                &[(104.0, 104.25, 99.75, 100.0), (99.5, 99.75, 98.75, 99.0), (99.25, 100.5, 98.75, 100.25)]),
            ("Dragonfly Doji - Bullish", 0, "DD",
                &[(100.0, 100.0625, 96.0, 100.0625)], &[(100.0, 100.5, 96.0, 100.0625)]),
            ("Engulfing - Bearish", 1, "BE",
                &[(100.5, 101.25, 100.25, 101.0), (101.25, 101.5, 99.75, 100.0)],
                &[(100.5, 101.25, 100.25, 101.0), (100.75, 101.0, 99.25, 99.5)]),
            ("Engulfing - Bullish", 0, "BE",
                &[(101.0, 101.25, 100.25, 100.5), (100.25, 101.75, 100.0, 101.5)],
                &[(101.0, 101.25, 100.25, 100.5), (100.75, 102.0, 100.5, 102.0)]),
            ("Evening Doji Star - Bearish", 1, "EDS",
                &[(100.0, 104.25, 99.75, 104.0), (105.0, 106.0, 104.5, 105.0625), (104.5, 104.75, 101.0, 101.5)],
                &[(100.0, 104.25, 99.75, 104.0), (105.0, 106.0, 104.5, 105.5), (104.5, 104.75, 101.0, 101.5)]),
            ("Evening Star - Bearish", 1, "ES",
                &[(100.0, 104.25, 99.75, 104.0), (105.0, 106.0, 104.5, 105.5), (104.5, 104.75, 101.0, 101.5)],
                &[(100.0, 104.25, 99.75, 104.0), (105.0, 106.0, 104.5, 105.5), (104.5, 104.75, 102.0, 102.25)]),
            ("Falling Three Methods - Bearish", 1, "FTM",
                &[(104.0, 104.5, 99.5, 100.0), (101.0, 101.75, 100.75, 101.5), (101.0, 101.75, 100.75, 101.5),
                  (101.0, 101.75, 100.75, 101.5), (101.5, 101.75, 98.5, 99.0)],
                &[(104.0, 104.5, 99.5, 100.0), (101.0, 101.75, 100.75, 101.5), (101.0, 101.75, 100.75, 101.5),
                  (101.0, 101.75, 100.75, 101.5), (101.5, 101.75, 100.0, 100.25)]),
            ("Falling Window - Bearish", 1, "FW",
                &[(101.0, 101.5, 99.5, 100.0), (99.0, 99.25, 98.0, 98.5)],
                &[(101.0, 101.5, 99.5, 100.0), (99.0, 99.5, 98.0, 98.5)]),
            ("Gravestone Doji - Bearish", 1, "GD",
                &[(100.0, 104.0, 100.0, 100.0625)], &[(100.0, 104.0, 99.5, 100.0625)]),
            ("Hammer - Bullish", 0, "H", &[(100.0, 100.5, 98.0, 100.5)], &[(100.0, 100.75, 98.0, 100.5)]),
            ("Hanging Man - Bearish", 1, "HM", &[(100.0, 100.5, 98.0, 100.5)], &[(100.0, 100.75, 98.0, 100.5)]),
            ("Harami - Bearish", 1, "BH",
                &[(100.0, 104.5, 99.5, 104.0), (102.5, 103.0, 101.5, 102.0)],
                &[(100.0, 104.5, 99.5, 104.0), (102.5, 104.25, 101.5, 102.0)]),
            ("Harami - Bullish", 0, "BH",
                &[(104.0, 104.5, 99.5, 100.0), (101.5, 103.0, 101.0, 102.0)],
                &[(104.0, 104.5, 99.5, 100.0), (101.5, 103.0, 99.75, 102.0)]),
            ("Harami Cross - Bearish", 1, "HC",
                &[(100.0, 104.5, 99.5, 104.0), (102.0, 103.0, 101.0, 102.0625)],
                &[(100.0, 104.5, 99.5, 104.0), (102.0, 103.0, 101.0, 102.25)]),
            ("Harami Cross - Bullish", 0, "HC",
                &[(104.0, 104.5, 99.5, 100.0), (102.0, 103.0, 101.0, 102.0625)],
                &[(104.0, 104.5, 99.5, 100.0), (102.0, 103.0, 101.0, 102.25)]),
            ("Inverted Hammer - Bullish", 0, "IH", &[(100.0, 102.5, 100.0, 100.5)], &[(100.0, 102.5, 99.75, 100.5)]),
            ("Kicking - Bearish", 1, "K",
                &[(105.0, 107.0, 105.0, 107.0), (104.0, 104.0, 102.0, 102.0)],
                &[(105.0, 107.0, 105.0, 107.0), (105.0, 105.0, 103.0, 103.0)]),
            ("Kicking - Bullish", 0, "K",
                &[(104.0, 104.0, 102.0, 102.0), (105.0, 107.0, 105.0, 107.0)],
                &[(104.0, 104.0, 102.0, 102.0), (104.0, 106.0, 104.0, 106.0)]),
            ("Long Lower Shadow - Bullish", 0, "LLS", &[(100.0, 100.5, 96.0, 100.25)], &[(100.0, 101.0, 97.0, 100.25)]),
            ("Long Upper Shadow - Bearish", 1, "LUS", &[(100.0, 104.0, 99.5, 99.75)], &[(100.0, 103.0, 99.0, 99.75)]),
            ("Marubozu Black - Bearish", 1, "MB", &[(102.0, 102.0, 100.0, 100.0)], &[(102.0, 102.0, 99.75, 100.0)]),
            ("Marubozu White - Bullish", 0, "MW", &[(100.0, 102.0, 100.0, 102.0)], &[(100.0, 102.25, 100.0, 102.0)]),
            ("Morning Doji Star - Bullish", 0, "MDS",
                &[(104.0, 104.25, 99.75, 100.0), (98.5, 99.5, 98.0, 98.5625), (99.5, 103.0, 99.25, 102.5)],
                &[(104.0, 104.25, 99.75, 100.0), (99.0, 99.5, 98.0, 98.5), (99.5, 103.0, 99.25, 102.5)]),
            ("Morning Star - Bullish", 0, "MS",
                &[(104.0, 104.25, 99.75, 100.0), (99.0, 99.5, 98.0, 98.5), (99.5, 103.0, 99.25, 102.5)],
                &[(104.0, 104.25, 99.75, 100.0), (99.0, 99.5, 98.0, 98.5), (99.5, 102.0, 99.25, 101.75)]),
            ("On Neck - Bearish", 1, "ON",
                &[(104.0, 104.5, 99.75, 100.0), (99.0, 100.0, 98.75, 99.75)],
                &[(104.0, 104.5, 99.75, 100.0), (99.0, 100.5, 98.75, 100.25)]),
            ("Piercing - Bullish", 0, "P",
                &[(103.0, 103.5, 99.5, 100.0), (99.0, 102.0, 98.5, 101.75)],
                &[(103.0, 103.5, 99.5, 100.0), (99.0, 102.0, 98.5, 101.25)]),
            ("Rising Three Methods - Bullish", 0, "RTM",
                &[(100.0, 104.5, 99.5, 104.0), (103.0, 103.25, 102.25, 102.5), (103.0, 103.25, 102.25, 102.5),
                  (103.0, 103.25, 102.25, 102.5), (102.5, 105.25, 102.25, 105.0)],
                &[(100.0, 104.5, 99.5, 104.0), (103.0, 103.25, 102.25, 102.5), (103.0, 103.25, 102.25, 102.5),
                  (103.0, 103.25, 102.25, 102.5), (102.5, 104.0, 102.25, 103.75)]),
            ("Rising Window - Bullish", 0, "RW",
                &[(100.0, 101.5, 99.5, 101.0), (102.0, 103.0, 101.75, 102.5)],
                &[(100.0, 101.5, 99.5, 101.0), (102.0, 103.0, 101.5, 102.5)]),
            ("Shooting Star - Bearish", 1, "SS", &[(100.0, 102.5, 100.0, 100.5)], &[(100.0, 102.5, 99.75, 100.5)]),
            ("Spinning Top Black", 2, "STB", &[(100.5, 102.0, 98.0, 100.0)], &[(100.5, 101.75, 98.0, 100.0)]),
            ("Spinning Top White", 2, "STW", &[(100.0, 102.0, 98.0, 100.5)], &[(100.0, 101.75, 98.0, 100.5)]),
            ("Three Black Crows - Bearish", 1, "3BC",
                &[(104.0, 104.5, 102.0, 102.0), (103.0, 103.5, 101.0, 101.0), (102.0, 102.5, 100.0, 100.0)],
                &[(104.0, 104.5, 102.0, 102.0), (103.0, 103.5, 101.0, 101.0), (102.0, 102.5, 99.5, 100.0)]),
            ("Three White Soldiers - Bullish", 0, "3WS",
                &[(100.0, 102.0, 99.5, 102.0), (101.0, 103.0, 100.5, 103.0), (102.0, 104.0, 101.5, 104.0)],
                &[(100.0, 102.0, 99.5, 102.0), (101.0, 103.0, 100.5, 103.0), (102.0, 104.5, 101.5, 104.0)]),
            ("Tri-Star - Bearish", 1, "3S",
                &[(100.0, 101.0, 99.0, 100.0), (102.0, 103.0, 101.0, 102.0), (100.0, 101.0, 99.0, 100.0)],
                &[(100.0, 101.0, 99.0, 100.0), (100.0, 101.0, 99.0, 100.0), (100.0, 101.0, 99.0, 100.0)]),
            ("Tri-Star - Bullish", 0, "3S",
                &[(100.0, 101.0, 99.0, 100.0), (98.0, 99.0, 97.0, 98.0), (100.0, 101.0, 99.0, 100.0)],
                &[(100.0, 101.0, 99.0, 100.0), (100.0, 101.0, 99.0, 100.0), (100.0, 101.0, 99.0, 100.0)]),
            ("Tweezer Bottom - Bullish", 0, "TB",
                &[(103.0, 103.25, 99.5, 100.0), (100.0, 101.5, 99.5, 101.0)],
                &[(103.0, 103.25, 99.5, 100.0), (100.0, 101.5, 99.25, 101.0)]),
            ("Tweezer Top - Bearish", 1, "TT",
                &[(100.0, 103.5, 99.75, 103.0), (103.0, 103.5, 101.5, 102.0)],
                &[(100.0, 103.5, 99.75, 103.0), (103.0, 103.75, 101.5, 102.0)]),
            ("Upside Tasuki Gap - Bullish", 0, "UTG",
                &[(100.0, 104.25, 99.75, 104.0), (104.5, 105.25, 104.25, 105.0), (104.75, 105.25, 104.0, 104.25)],
                &[(100.0, 104.25, 99.75, 104.0), (104.5, 105.25, 104.25, 105.0), (104.75, 105.25, 103.5, 103.75)]),
        ];
        let none = PatternTrend::NoDetection;
        // Bars near 100 are in a downtrend after closes falling to 90 and in an uptrend after
        // closes rising to 99. A reversal needs the trend it ends, a continuation the trend
        // it continues, and the shapes below need none.
        let falling = walk((90..150).rev().map(f64::from));
        let rising = walk((40..100).map(f64::from));
        for (label, line, abbr, hit, miss) in cases {
            assert_eq!(
                text(none, &flat, hit, label, line).as_deref(),
                Some(abbr),
                "{label}"
            );
            assert_eq!(text(none, &flat, miss, label, 0), None, "{label} near miss");
            assert_eq!(text(none, &flat, miss, label, 1), None, "{label} near miss");
            assert_eq!(text(none, &flat, miss, label, 2), None, "{label} near miss");
            let trendless = [
                "D", "DD", "GD", "K", "LLS", "LUS", "MB", "MW", "STB", "STW", "3BC", "3WS",
            ];
            let continuation = ["DTG", "FTM", "FW", "ON", "RTM", "RW", "UTG"];
            let (with, against) = if (line == 0) != continuation.contains(&abbr) {
                (&falling, &rising)
            } else {
                (&rising, &falling)
            };
            let sma = PatternTrend::Sma50;
            assert_eq!(
                text(sma, with, hit, label, line).as_deref(),
                Some(abbr),
                "{label} in trend"
            );
            assert_eq!(
                text(sma, against, hit, label, line).as_deref(),
                trendless.contains(&abbr).then_some(abbr),
                "{label} against trend"
            );
        }

        // The trend is checked on the bar the definition names: a window rising from a bar
        // below SMA(50) is not marked, even when the gap carries the close above it.
        let window: &[Ohlc] = &[(90.0, 90.25, 88.75, 89.0), (120.0, 121.0, 119.5, 120.5)];
        let rw = "Rising Window - Bullish";
        assert_eq!(text(PatternTrend::Sma50, &falling, window, rw, 0), None);

        // A hammer needs a downtrend: the close below SMA(50), and for the stricter rule also
        // SMA(50) below SMA(200). After 150 rising and 60 falling closes SMA(50) is below the
        // close but above SMA(200).
        let hammer: &[Ohlc] = &[(89.0, 89.5, 87.0, 89.5)];
        let turned = walk((1..=150).chain((90..150).rev()).map(f64::from));
        let long_fall = walk((90..300).rev().map(f64::from));
        let h = "Hammer - Bullish";
        assert_eq!(
            text(PatternTrend::Sma50, &falling, hammer, h, 0).as_deref(),
            Some("H")
        );
        assert_eq!(text(PatternTrend::Sma50, &rising, hammer, h, 0), None);
        assert_eq!(
            text(PatternTrend::Sma50, &turned, hammer, h, 0).as_deref(),
            Some("H")
        );
        assert_eq!(text(PatternTrend::Sma50Sma200, &turned, hammer, h, 0), None);
        assert_eq!(
            text(PatternTrend::Sma50Sma200, &long_fall, hammer, h, 0).as_deref(),
            Some("H")
        );

        // Several patterns on a bar are listed in toggle order, one per line.
        let dragonfly: &[Ohlc] = &[(100.0, 100.0625, 96.0, 100.0625)];
        assert_eq!(
            text(none, &flat, dragonfly, "", 0).as_deref(),
            Some("DD\nH\nLLS")
        );

        // A neutral doji closing a bearish Tri-Star shares the high, so the bearish label names it.
        let tri_star: &[Ohlc] = &[
            (100.0, 101.0, 99.0, 100.0),
            (102.0, 103.0, 101.0, 102.0),
            (100.0, 101.0, 99.0, 100.0),
        ];
        assert_eq!(text(none, &flat, tri_star, "", 1).as_deref(), Some("3S\nD"));
        assert_eq!(text(none, &flat, tri_star, "", 2), None);

        // The pattern type keeps one side and drops the neutral patterns.
        let mut data = flat.clone();
        data.extend(
            tri_star
                .iter()
                .map(|&(o, h, l, c)| Bar::new(Utc::now(), o, h, l, c, 1.0)),
        );
        let mut study = CandlestickPatterns::new(none).with_pattern_type(PatternType::Bullish);
        study.calculate(&data);
        let last = data.len() - 1;
        assert_eq!(study.plot_text(1, last), None);
        assert_eq!(study.plot_text(2, last), None);
    }

    /// The third bar of a Tasuki gap opens strictly inside the second body and closes
    /// strictly inside the gap: an open outside the body or a close on either gap edge is
    /// no pattern. `calculate` and `update` share the rule.
    #[test]
    fn tasuki_gap_needs_open_in_body_and_close_in_gap() {
        let flat = walk((0..14).map(|i| 100.0 + f64::from(i % 2)));
        let down: [Ohlc; 2] = [(104.0, 104.25, 99.75, 100.0), (99.5, 99.75, 98.75, 99.0)];
        let up: [Ohlc; 2] = [
            (100.0, 104.25, 99.75, 104.0),
            (104.5, 105.25, 104.25, 105.0),
        ];
        #[rustfmt::skip]
        let cases: [(&str, usize, &str, [Ohlc; 2], Ohlc, bool); 10] = [
            ("Downside Tasuki Gap - Bearish", 1, "DTG", down, (99.25, 100.25, 98.75, 99.75), true),
            ("Downside Tasuki Gap - Bearish", 1, "DTG", down, (98.75, 99.75, 98.5, 99.75), false),
            ("Downside Tasuki Gap - Bearish", 1, "DTG", down, (99.75, 100.0, 99.5, 99.875), false),
            ("Downside Tasuki Gap - Bearish", 1, "DTG", down, (99.25, 99.75, 99.0, 99.5), false),
            ("Downside Tasuki Gap - Bearish", 1, "DTG", down, (99.25, 100.25, 99.0, 100.0), false),
            ("Upside Tasuki Gap - Bullish", 0, "UTG", up, (104.75, 105.25, 104.0, 104.25), true),
            ("Upside Tasuki Gap - Bullish", 0, "UTG", up, (104.25, 104.5, 104.0, 104.125), false),
            ("Upside Tasuki Gap - Bullish", 0, "UTG", up, (105.5, 105.75, 104.0, 104.25), false),
            ("Upside Tasuki Gap - Bullish", 0, "UTG", up, (104.75, 105.0, 104.5, 104.5), false),
            ("Upside Tasuki Gap - Bullish", 0, "UTG", up, (104.75, 105.0, 103.75, 104.0), false),
        ];
        for (label, line, abbr, before, third, hit) in cases {
            let bars = [before[0], before[1], third];
            assert_eq!(
                text(PatternTrend::NoDetection, &flat, &bars, label, line).as_deref(),
                hit.then_some(abbr),
                "{label} third bar {third:?}"
            );
        }
    }
}
