//! The calls and time of each line of a script, recorded at the runtime operations the front end
//! marks: the time since the previous mark is charged to the line of the next one.

use super::program::Span;
use std::collections::BTreeMap;
use std::time::Duration;
use web_time::Instant;

/// Calls and time per line of a script's last full recalculation.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Profile {
    lines: Vec<(usize, u64, Duration)>,
}

impl Profile {
    /// One-based line, number of runtime operations and the time charged to them, by line.
    /// A line with no runtime operation has no row.
    pub fn lines(&self) -> &[(usize, u64, Duration)] {
        &self.lines
    }
}

/// What a run records while it profiles.
#[derive(Debug)]
pub(crate) struct Lines {
    /// The time of the last mark, or of the start of the run's latest slice.
    last: Instant,
    /// The operations marked on each line and the time charged to them.
    by_line: BTreeMap<u32, (u64, Duration)>,
}

impl Lines {
    pub fn new() -> Self {
        Self {
            last: Instant::now(),
            by_line: BTreeMap::new(),
        }
    }

    /// Restart the clock, so the time between two slices of a run is charged to no line.
    pub fn resume(&mut self) {
        self.last = Instant::now();
    }

    /// Count an operation at `span` and charge it the time since the last mark.
    pub fn mark(&mut self, span: Span) {
        if span.line == 0 {
            return;
        }
        let now = Instant::now();
        let (calls, time) = self.by_line.entry(span.line).or_default();
        *calls += 1;
        *time += now.duration_since(self.last);
        self.last = now;
    }

    pub fn profile(&self) -> Profile {
        let lines = self.by_line.iter();
        Profile {
            lines: lines
                .map(|(&line, &(calls, time))| (line as usize, calls, time))
                .collect(),
        }
    }
}
