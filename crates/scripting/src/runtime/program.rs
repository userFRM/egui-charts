//! What a script declares, independent of the language it was written in.

use ecolor::Color32;
use egui_charts::scales::StudyFormat;
use egui_charts::studies::{FillBoundary, InputKind, InputValue, PlotType, StudyScale};

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

/// Every declaration of a script, in source order.
#[derive(Clone, Debug, Default)]
pub(crate) struct ScriptSpec {
    pub display: Display,
    pub inputs: Vec<InputDecl>,
    pub states: Vec<StateDecl>,
    pub series: Vec<SeriesDecl>,
    pub plots: Vec<PlotDecl>,
    pub fills: Vec<FillDecl>,
}
