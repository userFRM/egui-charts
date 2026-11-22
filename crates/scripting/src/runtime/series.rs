//! Columns with one value per bar, and handles that read them at an offset.

use egui_charts::model::{Bar, PriceSource};

/// A column a script reads with history: a price field of the chart's bars, the column the
/// chart feeds into the `source` input, or a series the script declares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SeriesRef {
    Price(PriceSource),
    Volume,
    Time,
    BarIndex,
    Fed,
    Declared(usize),
}

/// The bars a run works on and the columns it reads and writes. Every column holds one value
/// per bar run so far; the bars and the fed column hold every bar of the run.
#[derive(Clone, Debug, Default)]
pub(crate) struct Columns {
    pub bars: Vec<Bar>,
    pub fed: Option<Vec<f64>>,
    pub declared: Vec<Vec<f64>>,
}

impl Columns {
    /// The value of `series` on `bar`, missing past the columns' end.
    pub fn value(&self, series: SeriesRef, bar: usize) -> f64 {
        let price = |read: fn(&Bar) -> f64| self.bars.get(bar).map_or(f64::NAN, read);
        match series {
            SeriesRef::Price(source) => self.bars.get(bar).map_or(f64::NAN, |bar| {
                source.compute(bar.open, bar.high, bar.low, bar.close)
            }),
            SeriesRef::Volume => price(|bar| bar.volume),
            SeriesRef::Time => price(|bar| bar.time.timestamp_millis() as f64),
            SeriesRef::BarIndex => bar as f64,
            SeriesRef::Fed => self
                .fed
                .as_ref()
                .and_then(|fed| fed.get(bar))
                .copied()
                .unwrap_or(f64::NAN),
            SeriesRef::Declared(at) => self.declared[at].get(bar).copied().unwrap_or(f64::NAN),
        }
    }

    /// The value of `series` `offset` bars before `bar`: missing before the first bar, an error
    /// for a negative offset, which would read a later bar.
    pub fn at(&self, series: SeriesRef, bar: usize, offset: i64) -> Result<f64, String> {
        if offset < 0 {
            return Err(format!(
                "history offset {offset} is negative; 0 is the current bar"
            ));
        }
        Ok(usize::try_from(offset)
            .ok()
            .and_then(|offset| bar.checked_sub(offset))
            .map_or(f64::NAN, |bar| self.value(series, bar)))
    }

    /// Keep the first `len` bars of every column the script writes.
    pub fn truncate(&mut self, len: usize) {
        for column in &mut self.declared {
            column.truncate(len);
        }
    }
}
