/// Shadows at most this fraction of the body are absent; also the marubozu tolerance.
const SHADOW: f64 = 0.05;
/// A doji body is at most this fraction of the range.
const DOJI_BODY: f64 = 0.05;
/// Prices within this fraction of the body average are equal (tweezers, on neck).
const EQUAL: f64 = 0.05;
/// How many times the body a hammer's or shooting star's long shadow is at least.
const FACTOR: f64 = 2.0;
/// Bars in the body average.
pub(super) const BODY_AVERAGE: usize = 14;

/// One bar with its body average and trend, the inputs every rule reads.
#[derive(Clone, Copy)]
pub(super) struct Candle {
    pub(super) open: f64,
    pub(super) high: f64,
    pub(super) low: f64,
    pub(super) close: f64,
    pub(super) average: f64,
    pub(super) down: bool,
    pub(super) up: bool,
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
pub(super) enum Side {
    Bullish,
    Bearish,
    Neutral,
}

/// A pattern's toggle label, chart abbreviation, side, bar count and rule over exactly
/// that many bars, oldest first.
pub(super) struct Pattern {
    pub(super) label: &'static str,
    pub(super) abbr: &'static str,
    pub(super) side: Side,
    pub(super) bars: usize,
    pub(super) found: fn(&[Candle]) -> bool,
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
pub(super) const PATTERNS: [Pattern; 44] = [
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
