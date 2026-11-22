//! Run-time limits: the clock checks of a run and the bytes its buffers hold.

use std::time::Duration;
use web_time::Instant;

/// Run-time limits of the studies built from a script. A run that passes one stops, keeps the
/// study's previous values and reports the limit; see [`Script::with_limits`](crate::Script::with_limits).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Limits {
    /// Longest time one bar may run; 500 ms by default.
    pub bar: Duration,
    /// Longest time a full recompute may run; 60 s by default.
    pub full: Duration,
    /// Most bytes one study's buffers may hold; 256 MiB by default.
    pub memory: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            bar: Duration::from_millis(500),
            full: Duration::from_secs(60),
            memory: 256 << 20,
        }
    }
}

/// Which limit stopped a run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Stop {
    Bar,
    Full,
    Memory,
    /// The study was dropped while its run went on.
    Cancelled,
}

/// Operations between two clock reads inside a bar.
pub(crate) const OPERATIONS_PER_CHECK: u64 = 256;

/// Bars between two clock reads of the bar loop.
pub(crate) const BARS_PER_CHECK: usize = 64;

/// The clock of one run.
#[derive(Clone, Debug)]
pub(crate) struct Clock {
    pub limits: Limits,
    deadline: Option<Instant>,
    bar_started: Option<Instant>,
}

impl Clock {
    /// A clock whose full-recompute limit starts now.
    pub fn start(limits: Limits) -> Self {
        Self {
            limits,
            deadline: Instant::now().checked_add(limits.full),
            bar_started: None,
        }
    }

    /// Start the next bar: its clock starts at its first read.
    pub fn next_bar(&mut self) {
        self.bar_started = None;
    }

    /// The check of a bar's `operations`-th operation: every [`OPERATIONS_PER_CHECK`]
    /// operations it reads the clock against both time limits.
    pub fn operation(&mut self, operations: u64) -> Option<Stop> {
        if !operations.is_multiple_of(OPERATIONS_PER_CHECK) {
            return None;
        }
        let now = Instant::now();
        let started = *self.bar_started.get_or_insert(now);
        if now.duration_since(started) > self.limits.bar || self.limits.bar.is_zero() {
            return Some(Stop::Bar);
        }
        self.past_deadline(now).then_some(Stop::Full)
    }

    /// The bar loop's check against the full-recompute limit.
    pub fn bars(&self) -> Option<Stop> {
        self.past_deadline(Instant::now()).then_some(Stop::Full)
    }

    fn past_deadline(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|deadline| now >= deadline) || self.limits.full.is_zero()
    }

    /// The message of a stop at `limit`, without its position.
    pub fn message(&self, limit: Stop) -> String {
        match limit {
            Stop::Bar => format!(
                "time limit of {} for one bar reached",
                duration(self.limits.bar)
            ),
            Stop::Full => format!(
                "time limit of {} for a full recompute reached",
                duration(self.limits.full)
            ),
            Stop::Memory => format!("memory limit of {} reached", bytes(self.limits.memory)),
            Stop::Cancelled => "the study was removed".to_owned(),
        }
    }
}

fn duration(time: Duration) -> String {
    if time.subsec_millis() == 0 && time.as_secs() > 0 {
        format!("{} s", time.as_secs())
    } else {
        format!("{} ms", time.as_millis())
    }
}

fn bytes(bytes: usize) -> String {
    if bytes.is_multiple_of(1 << 20) {
        format!("{} MiB", bytes >> 20)
    } else {
        format!("{bytes} bytes")
    }
}
