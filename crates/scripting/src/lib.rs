#![doc = include_str!("../README.md")]
#![warn(missing_docs, missing_debug_implementations)]

mod reference;
mod rhai;
mod runtime;
mod strategy;
mod study;
#[cfg(test)]
mod tests;
mod tokens;

pub use reference::{EntryKind, Reference, ReferenceEntry};
pub use runtime::limits::Limits;
pub use runtime::profile::Profile;
pub use strategy::ScriptStrategy;
pub use tokens::{TokenClass, tokens};

use egui_charts::model::Bar;
use egui_charts::studies::{CustomIndicator, IndicatorFactory};
use runtime::program::Span;
use std::ops::Range;
use std::sync::{Arc, Mutex};

/// A script compiled into a study, checked when it loads. Cheap to clone.
#[derive(Clone)]
pub struct Script {
    compiled: Arc<rhai::Compiled>,
    /// The latest runtime error or stop of any study or strategy built from this script.
    last_error: Arc<Mutex<Option<ScriptError>>>,
    limits: Limits,
    /// Whether the studies built from now on record a profile.
    profiling: bool,
    /// The profile of the last full recalculation of any study built from this script.
    profile: Arc<Mutex<Option<Profile>>>,
}

impl std::fmt::Debug for Script {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Script")
            .field("name", &self.compiled.name)
            .finish_non_exhaustive()
    }
}

impl Script {
    /// Compile `source` as the study `name`. Every declaration is read and checked here, and
    /// every name the script uses must be declared and every function it calls defined, by the
    /// script, the helpers or Rhai's standard package, so the study's inputs and plots exist
    /// before it runs. The script's `study(...)` calls are built from the studies of `factory` as
    /// it is now, at the defaults they declare. A script that places orders needs no plot.
    pub fn compile(
        name: impl Into<String>,
        source: &str,
        factory: &IndicatorFactory,
    ) -> Result<Self, ScriptError> {
        Ok(Self {
            compiled: Arc::new(rhai::compile(name.into(), source, factory)?),
            last_error: Arc::default(),
            limits: Limits::default(),
            profiling: false,
            profile: Arc::default(),
        })
    }

    /// The study's name.
    pub fn name(&self) -> &str {
        &self.compiled.name
    }

    /// The same script, whose studies built from now on stop at `limits`.
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// The same script, whose studies built from now on record for each line the calls and time
    /// of the runtime operations on it when `on`; off by default.
    pub fn with_profiling(mut self, on: bool) -> Self {
        self.profiling = on;
        self
    }

    /// A new study with the default inputs. It calculates away from the UI thread: its
    /// [`calculation`](egui_charts::studies::Indicator::calculation) is `Running` until the
    /// values are ready, and the chart draws the previous values meanwhile.
    pub fn study(&self) -> CustomIndicator {
        study::build(self)
    }

    /// A strategy of this script for the backtester, with the default inputs. Register it with
    /// [`StrategyFactory::register`](egui_charts::strategies::StrategyFactory::register) under the
    /// name the host saves it by; see the [crate documentation](crate#strategies).
    pub fn strategy(&self) -> ScriptStrategy {
        ScriptStrategy::new(self)
    }

    /// Whether the script places orders, so that it is a strategy rather than a study: whether it
    /// calls `entry`, `exit_order`, `exit_relative` or `cancel` and defines no function of that
    /// name. A study built from such a script ignores its orders.
    pub fn places_orders(&self) -> bool {
        self.compiled.orders
    }

    /// Calculate on `bars` with the default inputs, without the other histories the script reads,
    /// and return the first runtime error or limit stop, so a host can refuse a script before it
    /// replaces the study on a chart. Blocks the calling thread, for headless hosts and tests;
    /// natively the calculation runs on a thread of its own, whose stack fits the deepest scripts.
    pub fn check(&self, bars: &[Bar]) -> Result<(), ScriptError> {
        study::check(self, bars)
    }

    /// The latest runtime error or limit stop of any study or strategy built from this script,
    /// cleared when one of them completes a full recalculation on bars or a strategy prepares a
    /// run.
    pub fn last_error(&self) -> Option<ScriptError> {
        self.last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// What the last full recalculation on bars of a study of this script that ended without an
    /// error or limit spent on each line, when profiling was on; `None` before one.
    pub fn profile(&self) -> Option<Profile> {
        self.profile
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

/// Why a script was refused or stopped, and where.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ScriptError {
    /// The problem, without its position.
    pub message: String,
    /// One-based line, or 0 when the error has no position.
    pub line: usize,
    /// One-based column in characters, or 0 when the error has no position.
    pub column: usize,
    /// The zero-based bar being calculated, for runtime errors.
    pub bar: Option<usize>,
}

impl ScriptError {
    pub(crate) fn at(message: String, span: Span, bar: Option<usize>) -> Self {
        Self {
            message,
            line: span.line as usize,
            column: span.column as usize,
            bar,
        }
    }

    /// The byte range of `text` the error points at: the name or number at its line and column,
    /// or the single character there. Empty at the start of the text for an error without a
    /// position, and at the end of the line for a column past it, or of the text for a line past
    /// it.
    pub fn span(&self, text: &str) -> Range<usize> {
        if self.line == 0 {
            return 0..0;
        }
        // Lines end at `\n`, as the scanner counts them.
        let mut start = 0;
        for _ in 1..self.line {
            match text[start..].find('\n') {
                Some(end) => start += end + 1,
                None => return text.len()..text.len(),
            }
        }
        let line = text[start..].split('\n').next().unwrap_or_default();
        if self.column == 0 {
            return start..start;
        }
        let Some((at, c)) = line.char_indices().nth(self.column - 1) else {
            return start + line.len()..start + line.len();
        };
        let at = start + at;
        let word = |c: char| c.is_alphanumeric() || c == '_';
        if !word(c) {
            return at..at + c.len_utf8();
        }
        text[..at].trim_end_matches(word).len()
            ..text.len() - text[at..].trim_start_matches(word).len()
    }
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "line {}, column {}: ", self.line, self.column)?;
        }
        f.write_str(&self.message)?;
        if let Some(bar) = self.bar {
            write!(f, ", on bar {bar}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ScriptError {}

/// Scripts written for this crate, as titles and texts: a restatement of the relative strength
/// index, a moving average cross, a price channel breakout and a volume spike.
pub fn examples() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "Relative strength",
            include_str!("scripts/relative_strength.rhai"),
        ),
        (
            "Moving average cross",
            include_str!("scripts/moving_average_cross.rhai"),
        ),
        (
            "Price channel breakout",
            include_str!("scripts/price_channel_breakout.rhai"),
        ),
        ("Volume spike", include_str!("scripts/volume_spike.rhai")),
    ]
}
