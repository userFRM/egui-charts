#![doc = include_str!("../README.md")]
#![warn(missing_docs, missing_debug_implementations)]

mod rhai;
mod runtime;
mod study;
#[cfg(test)]
mod tests;

pub use runtime::limits::Limits;

use egui_charts::model::Bar;
use egui_charts::studies::{CustomIndicator, IndicatorFactory};
use runtime::program::Span;
use std::sync::{Arc, Mutex};

/// A script compiled into a study, checked when it loads. Cheap to clone.
#[derive(Clone)]
pub struct Script {
    compiled: Arc<rhai::Compiled>,
    /// The latest runtime error or stop of any study built from this script.
    last_error: Arc<Mutex<Option<ScriptError>>>,
    limits: Limits,
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
    /// every name the script uses must be declared, so the study's inputs and plots exist before
    /// it runs. `factory` is the registry the study is registered in, which later scripts call
    /// studies from.
    pub fn compile(
        name: impl Into<String>,
        source: &str,
        factory: &IndicatorFactory,
    ) -> Result<Self, ScriptError> {
        let _ = factory;
        Ok(Self {
            compiled: Arc::new(rhai::compile(name.into(), source)?),
            last_error: Arc::default(),
            limits: Limits::default(),
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

    /// A new study with the default inputs. It calculates away from the UI thread: its
    /// [`calculation`](egui_charts::studies::Indicator::calculation) is `Running` until the
    /// values are ready, and the chart draws the previous values meanwhile.
    pub fn study(&self) -> CustomIndicator {
        study::build(self)
    }

    /// Calculate on `bars` with the default inputs and return the first runtime error or limit
    /// stop, so a host can refuse a script before it replaces the study on a chart. Blocks the
    /// calling thread, for headless hosts and tests; natively the calculation runs on a thread
    /// of its own, whose stack fits the deepest scripts.
    pub fn check(&self, bars: &[Bar]) -> Result<(), ScriptError> {
        study::check(self, bars)
    }

    /// The latest runtime error or limit stop of any study built from this script, cleared
    /// when one of them completes a calculation from bar 0.
    pub fn last_error(&self) -> Option<ScriptError> {
        self.last_error
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
