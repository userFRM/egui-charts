//! Columns with one value per bar, and handles that read them at an offset.

use super::data::History;
use egui_charts::model::{Bar, PriceSource};

/// A column a script reads with history: a price field of the chart's bars, the column the
/// chart feeds into the `source` input, a series the script declares, the line of a study it
/// calls, or a field of another history.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SeriesRef {
    Price(PriceSource),
    Volume,
    Time,
    BarIndex,
    Fed,
    Declared(usize),
    Study(usize),
    History { at: usize, field: Field },
}

/// A field of the bars of another history.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Field {
    Price(PriceSource),
    Volume,
    Time,
}

/// The bars a run works on and the columns it reads and writes. Every column holds one value
/// per bar run so far; the bars, the fed column and the lines of studies hold every bar of the
/// run.
#[derive(Clone, Debug, Default)]
pub(crate) struct Columns {
    pub bars: Vec<Bar>,
    pub fed: Option<Vec<f64>>,
    pub declared: Vec<Vec<f64>>,
    /// The line read of each study the script called so far.
    pub studies: Vec<Vec<f64>>,
    /// The other histories the script declares, `None` for one the chart has not supplied.
    pub histories: Vec<Option<History>>,
    /// The bar of each other history that a bar reads, `usize::MAX` for none.
    pub aligned: Vec<Vec<usize>>,
}

impl Field {
    /// The field's value on `bar`.
    fn of(self, bar: &Bar) -> f64 {
        match self {
            Self::Price(source) => source.compute(bar.open, bar.high, bar.low, bar.close),
            Self::Volume => bar.volume,
            Self::Time => bar.time.timestamp_millis() as f64,
        }
    }
}

impl Columns {
    /// The value of `series` on `bar`, missing past the columns' end and for a history without a
    /// bar to read there.
    pub fn value(&self, series: SeriesRef, bar: usize) -> f64 {
        let price = |field: Field| self.bars.get(bar).map_or(f64::NAN, |bar| field.of(bar));
        let column = |column: Option<&Vec<f64>>| {
            column
                .and_then(|column| column.get(bar))
                .copied()
                .unwrap_or(f64::NAN)
        };
        match series {
            SeriesRef::Price(source) => price(Field::Price(source)),
            SeriesRef::Volume => price(Field::Volume),
            SeriesRef::Time => price(Field::Time),
            SeriesRef::BarIndex => bar as f64,
            SeriesRef::Fed => column(self.fed.as_ref()),
            SeriesRef::Declared(at) => column(self.declared.get(at)),
            SeriesRef::Study(at) => column(self.studies.get(at)),
            SeriesRef::History { at, field } => {
                let history = self.histories.get(at).and_then(Option::as_ref);
                let aligned = self.aligned.get(at).and_then(|aligned| aligned.get(bar));
                history
                    .zip(aligned)
                    .and_then(|(history, &aligned)| history.bars.get(aligned))
                    .map_or(f64::NAN, |bar| field.of(bar))
            }
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

    /// Keep the first `len` bars of every column the script writes and of the lines of the first
    /// `studies` studies.
    pub fn truncate(&mut self, len: usize, studies: usize) {
        self.studies.truncate(studies);
        for column in self.declared.iter_mut().chain(&mut self.studies) {
            column.truncate(len);
        }
        for aligned in &mut self.aligned {
            aligned.truncate(len);
        }
    }
}
