//! What a script declares, independent of the language it was written in.

use ecolor::Color32;
use egui_charts::scales::StudyFormat;
use egui_charts::studies::{FillBoundary, Indicator, InputKind, InputValue, PlotType, StudyScale};
use std::sync::{Arc, Mutex, MutexGuard};

/// A position in the text the user wrote: one-based line and column in characters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Span {
    pub line: u32,
    pub column: u32,
}

/// A literal a declaration takes: `na` is a missing number.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Literal {
    Number(f64),
    Int(i64),
    Bool(bool),
    Text(String),
}

/// The study's placement and units, from `display(...)`; `None` keeps the default.
#[derive(Clone, Debug, Default)]
pub(crate) struct Display {
    pub overlay: bool,
    pub format: Option<StudyFormat>,
    pub scale: Option<StudyScale>,
}

/// An `input NAME = KIND(...)` declaration.
#[derive(Clone, Debug)]
pub(crate) struct InputDecl {
    pub id: String,
    pub label: String,
    pub kind: InputKind,
    pub default: InputValue,
}

/// A `state NAME = LITERAL` declaration.
#[derive(Clone, Debug)]
pub(crate) struct StateDecl {
    pub name: String,
    pub initial: Literal,
}

/// A `series NAME = EXPR` declaration.
#[derive(Clone, Debug)]
pub(crate) struct SeriesDecl {
    pub name: String,
}

/// A `plot(TITLE, EXPR, ...)` declaration, numbered in source order.
#[derive(Clone, Debug)]
pub(crate) struct PlotDecl {
    pub title: String,
    pub color: Option<Color32>,
    pub style: PlotType,
    pub offset: isize,
}

/// A `fill(A, B, ...)` declaration between two plots or levels.
#[derive(Clone, Debug)]
pub(crate) struct FillDecl {
    pub first: FillBoundary,
    pub second: FillBoundary,
    pub color: Option<Color32>,
    pub inverted: Option<Color32>,
}

/// A study a script calls, with the inputs its calls start from.
#[derive(Clone)]
pub(crate) struct Prototype(pub Arc<Mutex<Box<dyn Indicator>>>);

impl Prototype {
    /// The study, to read or clone.
    pub fn lock(&self) -> MutexGuard<'_, Box<dyn Indicator>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl std::fmt::Debug for Prototype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Prototype")
    }
}

/// An input a `study(...)` call gives.
#[derive(Clone, Debug)]
pub(crate) struct StudyArg {
    pub id: String,
    /// The place of its value among the expressions of the call.
    pub place: usize,
    /// Whether it takes a series; any other input takes a series' value on the bar.
    pub series: bool,
}

/// A `study(NAME, ID = EXPR, ..., output = TITLE)` call, numbered in the order the calls end.
#[derive(Clone, Debug)]
pub(crate) struct StudyDecl {
    pub name: String,
    pub prototype: Prototype,
    pub inputs: Vec<StudyArg>,
    /// The title of the line read; the first line when the call names none.
    pub output: Option<String>,
}

/// Where a `bars` declaration takes its symbol or its interval from: a literal or the script's
/// input at an index.
#[derive(Clone, Debug)]
pub(crate) enum TextRef {
    Literal(String),
    Input(usize),
}

/// A `bars NAME = history(SYMBOL, INTERVAL)` declaration.
#[derive(Clone, Debug)]
pub(crate) struct HistoryDecl {
    pub name: String,
    pub symbol: TextRef,
    pub interval: TextRef,
}

/// Every declaration and study call of a script, in source order.
#[derive(Clone, Debug, Default)]
pub(crate) struct ScriptSpec {
    pub display: Display,
    pub inputs: Vec<InputDecl>,
    pub states: Vec<StateDecl>,
    pub series: Vec<SeriesDecl>,
    pub histories: Vec<HistoryDecl>,
    pub studies: Vec<StudyDecl>,
    pub plots: Vec<PlotDecl>,
    pub fills: Vec<FillDecl>,
}
