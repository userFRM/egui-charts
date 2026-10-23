//! Column-wise study output, with NaN for a missing line value.

use super::IndicatorValue;

/// One contiguous allocation per output line. Every column has [`len`](Self::len) entries.
/// Missing and non-finite outputs are stored as NaN. Read a line without allocating with
/// [`line`](Self::line), or export owned rows with [`to_rows`](Self::to_rows).
#[derive(Clone, Debug, Default)]
pub struct IndicatorValues {
    columns: Vec<Vec<f64>>,
    len: usize,
}

impl IndicatorValues {
    #[cfg(feature = "studies")]
    pub(crate) fn set_value(&mut self, line: usize, bar: usize, value: f64) {
        assert!(bar < self.len);
        while self.columns.len() <= line {
            self.columns.push(vec![f64::NAN; self.len]);
        }
        self.columns[line][bar] = if value.is_finite() { value } else { f64::NAN };
    }

    #[cfg(feature = "studies")]
    pub(crate) fn append_from(&mut self, other: &Self, start: usize) {
        let start = start.min(other.len);
        let old_len = self.len;
        self.resize(old_len + other.len - start, IndicatorValue::None);
        for (line, values) in other.columns.iter().enumerate() {
            while self.columns.len() <= line {
                self.columns.push(vec![f64::NAN; self.len]);
            }
            self.columns[line][old_len..].copy_from_slice(&values[start..]);
        }
    }
    /// Output for `len` bars before any line has a value.
    pub fn missing(len: usize) -> Self {
        Self {
            columns: Vec::new(),
            len,
        }
    }

    #[cfg(feature = "studies")]
    pub(crate) fn set(&mut self, bar: usize, value: IndicatorValue) {
        assert!(bar < self.len);
        let values = match &value {
            IndicatorValue::None => &[][..],
            IndicatorValue::Single(value) => std::slice::from_ref(value),
            IndicatorValue::Multiple(values) => values,
        };
        while self.columns.len() < values.len() {
            self.columns.push(vec![f64::NAN; self.len]);
        }
        for (index, line) in self.columns.iter_mut().enumerate() {
            let value = values.get(index).copied().unwrap_or(f64::NAN);
            line[bar] = if value.is_finite() { value } else { f64::NAN };
        }
    }
    /// Empty output, with columns allocated as the first values arrive.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of source bars, including warm-up bars.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether no source bars have been calculated.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Output columns in the order of the study's line names.
    pub fn columns(&self) -> &[Vec<f64>] {
        &self.columns
    }

    /// A complete output line, or an empty slice if that line has no values yet.
    pub fn line(&self, line: usize) -> &[f64] {
        self.columns.get(line).map_or(&[], Vec::as_slice)
    }

    /// A finite value at a zero-based line and source bar, or `None` when missing.
    pub fn value(&self, line: usize, bar: usize) -> Option<f64> {
        self.line(line).get(bar).copied().filter(|v| v.is_finite())
    }

    /// Export rows for callers that need the legacy owned row format. This allocates;
    /// rendering and numerical consumers should read the columns directly.
    pub fn to_rows(&self) -> Vec<IndicatorValue> {
        (0..self.len)
            .map(|bar| {
                if self.columns.iter().all(|line| line[bar].is_nan()) {
                    IndicatorValue::None
                } else if self.columns.len() == 1 {
                    IndicatorValue::Single(self.columns[0][bar])
                } else {
                    IndicatorValue::Multiple(self.columns.iter().map(|line| line[bar]).collect())
                }
            })
            .collect()
    }

    /// Append one bar's line values, using NaN for missing or non-finite entries.
    pub fn push_lines(&mut self, values: &[f64]) {
        while self.columns.len() < values.len() {
            self.columns.push(vec![f64::NAN; self.len]);
        }
        for (index, line) in self.columns.iter_mut().enumerate() {
            let value = values.get(index).copied().unwrap_or(f64::NAN);
            line.push(if value.is_finite() { value } else { f64::NAN });
        }
        self.len += 1;
    }

    pub(crate) fn push(&mut self, value: IndicatorValue) {
        match value {
            IndicatorValue::None => self.push_lines(&[]),
            IndicatorValue::Single(v) => self.push_lines(&[v]),
            IndicatorValue::Multiple(v) => self.push_lines(&v),
        }
    }

    /// Remove all output bars and columns.
    pub fn clear(&mut self) {
        self.columns.clear();
        self.len = 0;
    }

    #[cfg(feature = "studies")]
    pub(crate) fn truncate(&mut self, len: usize) {
        self.len = self.len.min(len);
        for line in &mut self.columns {
            line.truncate(len);
        }
    }

    #[cfg(feature = "studies")]
    pub(crate) fn resize(&mut self, len: usize, value: IndicatorValue) {
        self.truncate(len);
        while self.len < len {
            self.push(value.clone());
        }
    }

    #[cfg(feature = "studies")]
    pub(crate) fn fill(&mut self, value: IndicatorValue) {
        let len = self.len;
        self.clear();
        self.resize(len, value);
    }

    pub(crate) fn extend(&mut self, values: impl IntoIterator<Item = IndicatorValue>) {
        for value in values {
            self.push(value);
        }
    }
}

impl From<Vec<IndicatorValue>> for IndicatorValues {
    fn from(values: Vec<IndicatorValue>) -> Self {
        let mut output = Self::new();
        output.extend(values);
        output
    }
}

impl FromIterator<IndicatorValue> for IndicatorValues {
    fn from_iter<T: IntoIterator<Item = IndicatorValue>>(iter: T) -> Self {
        let mut output = Self::new();
        output.extend(iter);
        output
    }
}
