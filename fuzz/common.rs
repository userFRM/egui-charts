//! Hostile series shared by the `bars` and `studies` targets.

use arbitrary::Arbitrary;
use chrono::{DateTime, Duration, Utc};
use egui_charts::model::Bar;

/// A price or volume: ordinary values, and the ones that break arithmetic.
#[derive(Arbitrary, Debug)]
pub enum Num {
    Ordinary(i32),
    Raw(f64),
    Nan,
    Infinite,
    NegativeInfinite,
    Zero,
    Largest,
    Smallest,
}

impl Num {
    fn value(&self) -> f64 {
        match *self {
            Num::Ordinary(n) => f64::from(n) / 100.0,
            Num::Raw(v) => v,
            Num::Nan => f64::NAN,
            Num::Infinite => f64::INFINITY,
            Num::NegativeInfinite => f64::NEG_INFINITY,
            Num::Zero => 0.0,
            Num::Largest => f64::MAX,
            Num::Smallest => f64::MIN_POSITIVE,
        }
    }
}

/// A bar time: a minute near a fixed origin, so that bars collide, repeat and arrive out of
/// order, or any second `chrono` can represent.
#[derive(Arbitrary, Debug)]
pub enum Time {
    Near(i16),
    Far(i64),
}

impl Time {
    fn value(&self) -> DateTime<Utc> {
        match *self {
            Time::Near(minutes) => origin() + Duration::minutes(i64::from(minutes)),
            Time::Far(seconds) => DateTime::from_timestamp(seconds, 0).unwrap_or_default(),
        }
    }
}

pub fn origin() -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000, 0).unwrap_or_default()
}

#[derive(Arbitrary, Debug)]
pub struct Prices {
    open: Num,
    high: Num,
    low: Num,
    close: Num,
    volume: Num,
}

impl Prices {
    pub fn at(&self, time: DateTime<Utc>) -> Bar {
        Bar::new(
            time,
            self.open.value(),
            self.high.value(),
            self.low.value(),
            self.close.value(),
            self.volume.value(),
        )
    }
}

#[derive(Arbitrary, Debug)]
pub struct FuzzBar {
    time: Time,
    prices: Prices,
}

impl FuzzBar {
    pub fn bar(&self) -> Bar {
        self.prices.at(self.time.value())
    }
}
