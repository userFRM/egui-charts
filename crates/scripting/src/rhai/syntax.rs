//! The declarations as custom syntax: `display`, `input`, `state`, `series`, `bars`, `plot` and
//! `fill`, and the expression `study`. Their literal arguments are read when the script compiles
//! into a [`ScriptSpec`] in source order; at run time `series` and `plot` store their value for the
//! bar, `study` gives the line it reads and the others do nothing.
//!
//! A parse callback's error lands on the token after the last one it read, so each check runs
//! as early as the tokens it needs allow: a kind or option name before it is read, a literal
//! once the token after it is in view.

use super::engine::{BarsHandle, Handle, span};
use crate::runtime::bindings::{self, Arg};
use crate::runtime::{self, data, program::*};
use ::rhai::{
    Dynamic, Engine, EvalAltResult, EvalContext, Expression, ImmutableString, LexError, ParseError,
    ParseErrorType, Position,
};
use ecolor::Color32;
use egui_charts::model::{PriceSource, Timeframe};
use egui_charts::scales::StudyFormat;
use egui_charts::studies::{
    FillBoundary, InputKind, InputValue, PlotType, StudyInput, StudyScale, StudySource,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Most plots, inputs, series, states and other histories a script may declare.
pub(crate) const MAX_PLOTS: usize = 32;
pub(crate) const MAX_INPUTS: usize = 32;
pub(crate) const MAX_SERIES: usize = 64;
pub(crate) const MAX_STATES: usize = 64;
pub(crate) const MAX_HISTORIES: usize = 40;
/// Most choices a choice input may list and most inputs a study call may give: a declaration
/// walks its tokens again for each one.
const MAX_CHOICES: usize = 64;
const MAX_GIVEN: usize = 64;

/// The names every script reads, in the order the scope holds them after `na`.
pub(crate) const PRICES: [(&str, SeriesRef); 10] = [
    ("open", SeriesRef::Price(PriceSource::Open)),
    ("high", SeriesRef::Price(PriceSource::High)),
    ("low", SeriesRef::Price(PriceSource::Low)),
    ("close", SeriesRef::Price(PriceSource::Close)),
    ("volume", SeriesRef::Volume),
    ("hl2", SeriesRef::Price(PriceSource::HL2)),
    ("hlc3", SeriesRef::Price(PriceSource::HLC3)),
    ("ohlc4", SeriesRef::Price(PriceSource::OHLC4)),
    ("time", SeriesRef::Time),
    ("bar_index", SeriesRef::BarIndex),
];

use crate::runtime::series::SeriesRef;

/// The plot styles a script names, and the plot types they draw.
const STYLES: [(&str, PlotType); 13] = [
    ("line", PlotType::Line),
    ("line_with_breaks", PlotType::LineWithBreaks),
    ("histogram", PlotType::Histogram),
    ("circles", PlotType::Circles),
    ("step_line", PlotType::StepLine),
    ("step_line_with_breaks", PlotType::StepLineWithBreaks),
    ("step_line_with_diamonds", PlotType::StepLineWithDiamonds),
    ("columns", PlotType::Columns),
    ("crosses", PlotType::Crosses),
    ("area", PlotType::Area),
    ("area_with_breaks", PlotType::AreaWithBreaks),
    ("triangle_up", PlotType::TriangleUp),
    ("triangle_down", PlotType::TriangleDown),
];

/// The input kinds and their argument counts.
const KINDS: [(&str, usize); 9] = [
    ("length", 4),
    ("int", 4),
    ("float", 4),
    ("bool", 2),
    ("choice", 3),
    ("source", 2),
    ("text", 2),
    ("symbol", 2),
    ("interval", 2),
];

/// The ids the chart gives inputs of its own.
const RESERVED: [&str; 4] = [
    "plot_offset",
    "calculation_symbol",
    "calculation_timeframe",
    "wait_for_close",
];

/// The studies a script calls by name, `None` for a name nothing is registered as.
pub(crate) type Catalogue = HashMap<String, Option<Prototype>>;

/// The declarations read so far.
#[derive(Debug, Default)]
pub(crate) struct Collector {
    pub spec: ScriptSpec,
    /// The names of the studies called, while there is no catalogue to find them in.
    pub called: Vec<String>,
    catalogue: Option<Catalogue>,
    displayed: bool,
}

impl Collector {
    pub fn new(catalogue: Option<Catalogue>) -> Self {
        Self {
            catalogue,
            ..Self::default()
        }
    }
}

type Shared = Arc<Mutex<Collector>>;

/// What a declaration has read: the kind of each token it asked for, and its text.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Symbol,
    Ident,
    Text,
    Int,
    Float,
    Bool,
    Expr,
}

impl Kind {
    fn marker(self, symbol: &str) -> &str {
        match self {
            Self::Symbol => symbol,
            Self::Ident => "$ident$",
            Self::Text => "$string$",
            Self::Int => "$int$",
            Self::Float => "$float$",
            Self::Bool => "$bool$",
            Self::Expr => "$expr$",
        }
    }

    fn code(self) -> i64 {
        self as i64
    }

    fn from_code(code: i64) -> Self {
        [
            Self::Symbol,
            Self::Ident,
            Self::Text,
            Self::Int,
            Self::Float,
            Self::Bool,
            Self::Expr,
        ][code as usize]
    }
}

/// Why a declaration's parse stopped: it needs a token, or it is wrong.
enum Stop {
    Want(Kind, &'static str),
    Fail(String),
}

type Step<T> = Result<T, Stop>;

/// A literal argument.
#[derive(Clone, Debug, PartialEq)]
enum Lit {
    Text(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Ident(String),
}

impl Lit {
    fn describe(&self) -> &'static str {
        match self {
            Self::Text(_) => "a string",
            Self::Int(_) => "an int",
            Self::Float(_) => "a number",
            Self::Bool(_) => "a bool",
            Self::Ident(_) => "a name",
        }
    }

    /// A number literal, an int converted; `na` is missing.
    fn number(&self) -> Option<f64> {
        match self {
            Self::Int(value) => Some(*value as f64),
            Self::Float(value) => Some(*value),
            Self::Ident(name) if name == "na" => Some(f64::NAN),
            _ => None,
        }
    }
}

/// The tokens a declaration has read, which it walks again on every call.
struct Cursor<'a> {
    tokens: Vec<(Kind, &'a str)>,
    at: usize,
    look: &'a str,
}

impl<'a> Cursor<'a> {
    fn next(&mut self, want: Kind, symbol: &'static str) -> Step<&'a str> {
        let token = self.tokens.get(self.at).ok_or(Stop::Want(want, symbol))?;
        self.at += 1;
        Ok(token.1)
    }

    /// Whether every read token is used, so the next one is the look-ahead.
    fn at_end(&self) -> bool {
        self.at == self.tokens.len()
    }

    fn symbol(&mut self, symbol: &'static str) -> Step<()> {
        self.next(Kind::Symbol, symbol).map(|_| ())
    }

    fn ident(&mut self) -> Step<&'a str> {
        self.next(Kind::Ident, "")
    }

    fn expr(&mut self) -> Step<()> {
        self.next(Kind::Expr, "").map(|_| ())
    }

    /// A name, or a string where one is in view, and whether it is a string.
    fn name(&mut self) -> Step<(&'a str, bool)> {
        let quoted = match self.tokens.get(self.at) {
            Some((kind, _)) => *kind == Kind::Text,
            None => self.look.starts_with('"'),
        };
        let kind = if quoted { Kind::Text } else { Kind::Ident };
        Ok((self.next(kind, "")?, quoted))
    }

    /// The symbol read next of `choices`, the look-ahead deciding when it is not read yet.
    fn either(&mut self, choices: &[&'static str]) -> Step<&'a str> {
        if self.at_end() {
            let symbol = choices
                .iter()
                .find(|choice| **choice == self.look)
                .unwrap_or(&choices[choices.len() - 1]);
            return Err(Stop::Want(Kind::Symbol, symbol));
        }
        self.next(Kind::Symbol, "")
    }

    /// A literal: a string, a number with an optional `-`, `true`, `false` or a name.
    fn literal(&mut self) -> Step<Lit> {
        let negative = self
            .tokens
            .get(self.at)
            .is_some_and(|token| *token == (Kind::Symbol, "-"));
        if negative {
            self.at += 1;
        }
        let Some(&(kind, text)) = self.tokens.get(self.at) else {
            let look = self.look;
            let kind = if look.starts_with(|c: char| c.is_ascii_digit()) {
                if look.parse::<i64>().is_ok() {
                    Kind::Int
                } else {
                    Kind::Float
                }
            } else if negative {
                return Err(Stop::Fail("expecting a number after `-`".into()));
            } else if look == "-" {
                return Err(Stop::Want(Kind::Symbol, "-"));
            } else if look.starts_with('"') {
                Kind::Text
            } else if look == "true" || look == "false" {
                Kind::Bool
            } else if look.starts_with(|c: char| c.is_alphabetic() || c == '_') {
                Kind::Ident
            } else {
                return Err(Stop::Fail(
                    "expecting a literal: a number, a string, true, false or na".into(),
                ));
            };
            return Err(Stop::Want(kind, ""));
        };
        self.at += 1;
        let sign = if negative { -1.0 } else { 1.0 };
        Ok(match kind {
            Kind::Text => Lit::Text(text.to_owned()),
            Kind::Int => Lit::Int(text.parse::<i64>().unwrap_or(0) * sign as i64),
            Kind::Float => Lit::Float(text.parse::<f64>().unwrap_or(f64::NAN) * sign),
            Kind::Bool => Lit::Bool(text == "true"),
            _ => Lit::Ident(text.to_owned()),
        })
    }

    /// `, NAME = LITERAL` options up to the closing `)`, each name checked before it is read.
    fn options(&mut self, names: &[&str]) -> Step<Vec<(&'a str, Lit)>> {
        let mut options: Vec<(&str, Lit)> = Vec::new();
        while self.either(&[",", ")"])? == "," {
            if self.at_end() && !names.contains(&self.look) {
                return Err(Stop::Fail(format!(
                    "unknown option {}; expected one of {}",
                    self.look,
                    names.join(", ")
                )));
            }
            let name = self.ident()?;
            if self.at_end() && options.iter().any(|(given, _)| *given == name) {
                return Err(Stop::Fail(format!("option {name} is given twice")));
            }
            self.symbol("=")?;
            let value = self.literal()?;
            options.push((name, value));
        }
        Ok(options)
    }
}

/// Register the declarations on `engine`, reading them into `collector` as they parse.
pub(crate) fn register(engine: &mut Engine, collector: &Shared) {
    let declare =
        |engine: &mut Engine,
         key: &'static str,
         eval: fn(&mut EvalContext, &[Expression], &Dynamic) -> RhaiResult| {
            let collector = collector.clone();
            engine.register_custom_syntax_with_state_raw(
                key,
                move |symbols, look, state| parse(&collector, key, symbols, look, state),
                false,
                eval,
            );
        };
    declare(engine, "display", nothing);
    declare(engine, "input", nothing);
    declare(engine, "state", nothing);
    declare(engine, "series", store_series);
    declare(engine, "bars", nothing);
    declare(engine, "study", read_study);
    declare(engine, "plot", store_plot);
    declare(engine, "fill", nothing);
}

type RhaiResult = Result<Dynamic, Box<EvalAltResult>>;

fn nothing(_: &mut EvalContext, _: &[Expression], _: &Dynamic) -> RhaiResult {
    Ok(Dynamic::UNIT)
}

/// The number a `series` or `plot` stores: a number, an int, a series' current value or `na`.
fn number(
    context: &mut EvalContext,
    expression: &Expression,
    what: impl FnOnce() -> String,
) -> Result<f64, Box<EvalAltResult>> {
    let value = context.eval_expression_tree(expression)?;
    if let Some(value) = value.clone().try_cast::<f64>() {
        Ok(value)
    } else if let Some(value) = value.clone().try_cast::<i64>() {
        Ok(value as f64)
    } else if let Some(series) = value.clone().try_cast::<Handle>() {
        Ok(series.value())
    } else {
        Err(EvalAltResult::ErrorRuntime(
            format!("{} holds numbers; got {}", what(), type_name(&value)).into(),
            expression.position(),
        )
        .into())
    }
}

/// A value's type in a script's terms.
pub(crate) fn type_name(value: &Dynamic) -> &'static str {
    match value.type_name() {
        "f64" => "number",
        "i64" => "int",
        "bool" => "bool",
        "string" | "&str" | "ImmutableString" => "string",
        "()" => "nothing",
        "array" => "array",
        "map" => "map",
        _ if value.is::<Handle>() => "series",
        _ if value.is::<BarsHandle>() => "bars",
        _ => "value",
    }
}

fn store_series(context: &mut EvalContext, inputs: &[Expression], state: &Dynamic) -> RhaiResult {
    let at = state.as_int().unwrap_or(0) as usize;
    let name = inputs[0].get_string_value().unwrap_or_default().to_owned();
    let value = number(context, &inputs[1], || format!("series {name}"))?;
    runtime::with(|run| {
        if let Some(slot) = run.columns.declared[at].last_mut() {
            *slot = value;
        }
    });
    Ok(Dynamic::UNIT)
}

fn store_plot(context: &mut EvalContext, inputs: &[Expression], state: &Dynamic) -> RhaiResult {
    let at = state.as_int().unwrap_or(0) as usize;
    let title = inputs[0].get_string_value().unwrap_or_default().to_owned();
    let value = number(context, &inputs[1], || format!("plot \"{title}\""))?;
    runtime::with(|run| run.row[at] = value);
    Ok(Dynamic::UNIT)
}

/// The line a `study` call reads with the inputs it gives on this bar, as a series.
fn read_study(context: &mut EvalContext, inputs: &[Expression], state: &Dynamic) -> RhaiResult {
    let site = state.as_int().unwrap_or(0) as usize;
    let spec = runtime::with(|run| run.spec.clone()).unwrap_or_default();
    let given = spec
        .studies
        .get(site)
        .map_or(&[][..], |study| &study.inputs);
    let mut args = Vec::with_capacity(given.len());
    for input in given {
        let value = context.eval_expression_tree(&inputs[input.place])?;
        args.push(arg(&value, input.series));
    }
    let name = inputs[0].position();
    match runtime::with(|run| run.study(site, args, span(name))) {
        None => Ok(Dynamic::from_float(f64::NAN)),
        Some(Ok(line)) => Ok(Dynamic::from(Handle(line))),
        Some(Err((at, message))) => {
            let position = at.map_or(name, |at| inputs[given[at].place].position());
            Err(EvalAltResult::ErrorRuntime(message.into(), position).into())
        }
    }
}

/// What a study's input is given by `value`: an input that takes a `series` reads its column, any
/// other its value on the bar.
fn arg(value: &Dynamic, series: bool) -> Arg {
    let handle = value.read_lock::<Handle>().map(|handle| *handle);
    match handle {
        Some(handle) if series => Arg::Series(handle.0),
        Some(handle) => Arg::Number(handle.value()),
        None if series => Arg::Other(type_name(value)),
        None => {
            if let Ok(value) = value.as_float() {
                Arg::Number(value)
            } else if let Ok(value) = value.as_int() {
                Arg::Int(value)
            } else if let Ok(value) = value.as_bool() {
                Arg::Bool(value)
            } else if let Some(text) = value.read_lock::<ImmutableString>() {
                Arg::Text(text.to_string())
            } else {
                Arg::Other(type_name(value))
            }
        }
    }
}

fn parse(
    collector: &Shared,
    key: &str,
    symbols: &[ImmutableString],
    look: &str,
    state: &mut Dynamic,
) -> Result<Option<ImmutableString>, ParseError> {
    let mut kinds: Vec<i64> = match state.clone().try_cast::<::rhai::Array>() {
        Some(kinds) => kinds
            .into_iter()
            .filter_map(|kind| kind.as_int().ok())
            .collect(),
        None => Vec::new(),
    };
    let mut cursor = Cursor {
        tokens: kinds
            .iter()
            .zip(&symbols[1..])
            .map(|(kind, text)| (Kind::from_code(*kind), text.as_str()))
            .collect(),
        at: 0,
        look,
    };
    let mut collector = collector.lock().unwrap_or_else(|e| e.into_inner());
    let declared = match key {
        "display" => display(&mut cursor, &mut collector),
        "input" => input(&mut cursor, &mut collector),
        "state" => state_decl(&mut cursor, &mut collector),
        "series" => series(&mut cursor, &mut collector),
        "bars" => bars(&mut cursor, &mut collector),
        "study" => study(&mut cursor, &mut collector),
        "plot" => plot(&mut cursor, &mut collector),
        _ => fill(&mut cursor, &mut collector),
    };
    match declared {
        Ok(index) => {
            *state = index.map_or(Dynamic::UNIT, |index| Dynamic::from_int(index as i64));
            Ok(None)
        }
        Err(Stop::Want(kind, symbol)) => {
            kinds.push(kind.code());
            *state = Dynamic::from_array(kinds.into_iter().map(Dynamic::from_int).collect());
            Ok(Some(kind.marker(symbol).into()))
        }
        Err(Stop::Fail(message)) => Err(ParseError(
            Box::new(ParseErrorType::BadInput(LexError::ImproperSymbol(
                String::new(),
                message,
            ))),
            Position::NONE,
        )),
    }
}

/// The literal an option or argument has, checked against what it takes.
fn expect<T>(lit: &Lit, what: &str, read: impl FnOnce(&Lit) -> Option<T>) -> Step<T> {
    read(lit).ok_or_else(|| Stop::Fail(format!("{what}; got {}", lit.describe())))
}

fn text(lit: &Lit) -> Option<String> {
    match lit {
        Lit::Text(text) => Some(text.clone()),
        _ => None,
    }
}

fn int(lit: &Lit) -> Option<i64> {
    match lit {
        Lit::Int(value) => Some(*value),
        _ => None,
    }
}

fn color(lit: &Lit) -> Option<Color32> {
    let hex = text(lit)?;
    let hex = hex.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    let alpha = if hex.len() == 8 { byte(6)? } else { 255 };
    Some(Color32::from_rgba_unmultiplied(
        byte(0)?,
        byte(2)?,
        byte(4)?,
        alpha,
    ))
}

const COLOR: &str = "color takes \"#RRGGBB\" or \"#RRGGBBAA\"";

/// `display(overlay = .., format = .., scale = ..)`.
fn display(cursor: &mut Cursor, collector: &mut Collector) -> Step<Option<usize>> {
    if cursor.tokens.is_empty() && collector.displayed {
        return Err(Stop::Fail("display is declared twice".into()));
    }
    cursor.symbol("(")?;
    let empty = if cursor.at_end() {
        cursor.look == ")"
    } else {
        cursor.tokens[cursor.at] == (Kind::Symbol, ")")
    };
    let options = if empty {
        cursor.symbol(")")?;
        Vec::new()
    } else {
        first_options(cursor, &["overlay", "format", "scale"])?
    };
    let display = &mut collector.spec.display;
    for (name, value) in options {
        match name {
            "overlay" => {
                display.overlay = expect(&value, "overlay takes true or false", |lit| match lit {
                    Lit::Bool(value) => Some(*value),
                    _ => None,
                })?;
            }
            "format" => {
                display.format = Some(expect(
                    &value,
                    "format takes \"price\", \"volume\", \"percent\" or decimals from 0 to 12",
                    |lit| match lit {
                        Lit::Text(text) if text == "price" => Some(StudyFormat::Price),
                        Lit::Text(text) if text == "volume" => Some(StudyFormat::Volume),
                        Lit::Text(text) if text == "percent" => Some(StudyFormat::Percent),
                        Lit::Int(decimals @ 0..=12) => Some(StudyFormat::Fixed(*decimals as usize)),
                        _ => None,
                    },
                )?);
            }
            _ => {
                display.scale = Some(expect(
                    &value,
                    "scale takes \"main\", \"independent\" or \"none\"",
                    |lit| match lit {
                        Lit::Text(text) if text == "main" => Some(StudyScale::Main),
                        Lit::Text(text) if text == "independent" => Some(StudyScale::Independent),
                        Lit::Text(text) if text == "none" => Some(StudyScale::None),
                        _ => None,
                    },
                )?);
            }
        }
    }
    collector.displayed = true;
    Ok(None)
}

/// `NAME = LITERAL` options separated by commas up to `)`, the first without a comma.
fn first_options<'a>(cursor: &mut Cursor<'a>, names: &[&str]) -> Step<Vec<(&'a str, Lit)>> {
    if cursor.at_end() && !names.contains(&cursor.look) {
        return Err(Stop::Fail(format!(
            "unknown option {}; expected one of {}",
            cursor.look,
            names.join(", ")
        )));
    }
    let name = cursor.ident()?;
    cursor.symbol("=")?;
    let value = cursor.literal()?;
    let mut options = vec![(name, value)];
    for (name, value) in cursor.options(names)? {
        if options.iter().any(|(given, _)| *given == name) {
            return Err(Stop::Fail(format!("option {name} is given twice")));
        }
        options.push((name, value));
    }
    Ok(options)
}

/// A name a declaration gives, checked before the token after it is read.
fn new_name<'a>(cursor: &mut Cursor<'a>, collector: &Collector) -> Step<&'a str> {
    let name = cursor.ident()?;
    if cursor.at_end() {
        let spec = &collector.spec;
        if name == "na" || PRICES.iter().any(|(price, _)| *price == name) {
            return Err(Stop::Fail(format!("{name} is a built-in name")));
        }
        if spec.inputs.iter().any(|input| input.id == name)
            || spec.states.iter().any(|state| state.name == name)
            || spec.series.iter().any(|series| series.name == name)
            || spec.histories.iter().any(|history| history.name == name)
        {
            return Err(Stop::Fail(format!("{name} is declared twice")));
        }
    }
    Ok(name)
}

/// `input NAME = KIND(LABEL, DEFAULT, ...)`.
fn input(cursor: &mut Cursor, collector: &mut Collector) -> Step<Option<usize>> {
    let id = new_name(cursor, collector)?;
    if cursor.at_end() && RESERVED.contains(&id) {
        return Err(Stop::Fail(format!(
            "input {id} is reserved by the chart for an input of its own"
        )));
    }
    cursor.symbol("=")?;
    if cursor.at_end() && !KINDS.iter().any(|(kind, _)| *kind == cursor.look) {
        return Err(Stop::Fail(format!(
            "unknown input kind {}; expected one of {}",
            cursor.look,
            KINDS.map(|(kind, _)| kind).join(", ")
        )));
    }
    let kind = cursor.ident()?;
    let count = KINDS
        .iter()
        .find(|(name, _)| *name == kind)
        .map_or(0, |(_, count)| *count);
    cursor.symbol("(")?;
    let mut args = Vec::new();
    let mut choices = Vec::new();
    loop {
        if kind == "choice" && args.len() == 2 {
            choices = choice_list(cursor)?;
            args.push(Lit::Int(0));
        } else {
            args.push(cursor.literal()?);
        }
        let more = args.len() < count;
        if cursor.at_end() && (cursor.look == ",") != more {
            return Err(Stop::Fail(format!(
                "{kind} takes {count} arguments: {}",
                signature(kind)
            )));
        }
        if cursor.either(&[",", ")"])? == ")" {
            break;
        }
    }
    if collector.spec.inputs.len() == MAX_INPUTS {
        return Err(Stop::Fail(format!(
            "a script declares at most {MAX_INPUTS} inputs"
        )));
    }
    let label = expect(&args[0], "the label is a string", text)?;
    if label.trim().is_empty() {
        return Err(Stop::Fail("the label is empty".into()));
    }
    let (kind, default) = input_kind(kind, &args, choices)?;
    let input = StudyInput::new(id, label.clone(), default.clone(), kind.clone());
    if !input.accepts(&default) {
        return Err(Stop::Fail(format!(
            "input {id} has default {} outside what it takes",
            show(&default)
        )));
    }
    collector.spec.inputs.push(InputDecl {
        id: id.to_owned(),
        label,
        kind,
        default,
    });
    Ok(None)
}

fn signature(kind: &str) -> &'static str {
    match kind {
        "length" | "int" | "float" => "label, default, min and max",
        "choice" => "label, default and a list of choices",
        "source" => "label and one of open, high, low, close, hl2, hlc3 and ohlc4",
        _ => "label and default",
    }
}

fn show(value: &InputValue) -> String {
    match value {
        InputValue::Length(value) => value.to_string(),
        InputValue::Integer(value) => value.to_string(),
        InputValue::Number(value) => value.to_string(),
        InputValue::Text(value) | InputValue::Symbol(value) => format!("{value:?}"),
        other => format!("{other:?}"),
    }
}

/// `[ "a", "b", ... ]`.
fn choice_list(cursor: &mut Cursor) -> Step<Vec<String>> {
    cursor.symbol("[")?;
    let mut choices = Vec::new();
    loop {
        if choices.len() == MAX_CHOICES {
            return Err(Stop::Fail(format!(
                "a choice input lists at most {MAX_CHOICES} choices"
            )));
        }
        let choice = cursor.literal()?;
        let choice = expect(&choice, "choices are strings", text)?;
        if cursor.at_end() && choices.contains(&choice) {
            return Err(Stop::Fail(format!("choice {choice:?} is listed twice")));
        }
        choices.push(choice);
        if cursor.either(&[",", "]"])? == "]" {
            return Ok(choices);
        }
    }
}

/// The input kind of `kind` with literal arguments `args`, and its default.
fn input_kind(kind: &str, args: &[Lit], choices: Vec<String>) -> Step<(InputKind, InputValue)> {
    let int_in = |lit: &Lit, what: &str, range: std::ops::RangeInclusive<i64>| {
        expect(lit, &format!("{what} is an int"), int).and_then(|value| {
            if range.contains(&value) {
                Ok(value)
            } else {
                Err(Stop::Fail(format!(
                    "{what} {value} is outside {} to {}",
                    range.start(),
                    range.end()
                )))
            }
        })
    };
    let number = |lit: &Lit, what: &str| {
        expect(lit, &format!("{what} is a number"), |lit| match lit {
            Lit::Int(value) => Some(*value as f64),
            Lit::Float(value) => Some(*value),
            _ => None,
        })
        .and_then(|value| {
            if value.is_finite() {
                Ok(value)
            } else {
                Err(Stop::Fail(format!("{what} is not finite")))
            }
        })
    };
    let bounds = |min: f64, max: f64| {
        if min > max {
            Err(Stop::Fail(format!("min {min} is above max {max}")))
        } else {
            Ok(())
        }
    };
    Ok(match kind {
        "length" => {
            let range = 0..=i64::from(i32::MAX);
            let min = int_in(&args[2], "min", range.clone())?;
            let max = int_in(&args[3], "max", range)?;
            bounds(min as f64, max as f64)?;
            let default = expect(&args[1], "the default is an int", int)?;
            (
                InputKind::Length(min as usize, max as usize),
                InputValue::Length(usize::try_from(default).unwrap_or(usize::MAX)),
            )
        }
        "int" => {
            let range = i64::from(i32::MIN)..=i64::from(i32::MAX);
            let min = int_in(&args[2], "min", range.clone())?;
            let max = int_in(&args[3], "max", range.clone())?;
            bounds(min as f64, max as f64)?;
            let default = int_in(&args[1], "the default", range)?;
            (
                InputKind::Integer(min as i32, max as i32),
                InputValue::Integer(default as i32),
            )
        }
        "float" => {
            let min = number(&args[2], "min")?;
            let max = number(&args[3], "max")?;
            bounds(min, max)?;
            let default = number(&args[1], "the default")?;
            (InputKind::Number(min, max), InputValue::Number(default))
        }
        "bool" => {
            let default = expect(&args[1], "the default is true or false", |lit| match lit {
                Lit::Bool(value) => Some(*value),
                _ => None,
            })?;
            (InputKind::Toggle, InputValue::Toggle(default))
        }
        "choice" => {
            let default = expect(&args[1], "the default is a string", text)?;
            let at = choices
                .iter()
                .position(|choice| *choice == default)
                .ok_or_else(|| Stop::Fail(format!("the default {default:?} is not a choice")))?;
            (InputKind::Choice(choices), InputValue::Choice(at))
        }
        "source" => {
            let price = expect(
                &args[1],
                "the default is one of open, high, low, close, hl2, hlc3 and ohlc4",
                |lit| match lit {
                    Lit::Ident(name) => {
                        PRICES[..8].iter().find_map(|(price, series)| match series {
                            SeriesRef::Price(source) if price == name => Some(*source),
                            _ => None,
                        })
                    }
                    _ => None,
                },
            )?;
            (
                InputKind::Source,
                InputValue::Source(StudySource::Price(price)),
            )
        }
        _ => {
            let default = expect(&args[1], "the default is a string", text)?;
            match kind {
                "symbol" => (InputKind::Symbol, InputValue::Symbol(default)),
                "interval" => {
                    let interval = interval(&default)?;
                    (InputKind::Timeframe, InputValue::Timeframe(interval))
                }
                _ => (InputKind::Text, InputValue::Text(default)),
            }
        }
    })
}

/// The interval `text` names, `None` for the chart's.
fn interval(text: &str) -> Step<Option<Timeframe>> {
    data::interval(text).ok_or_else(|| {
        Stop::Fail(format!(
            "{text:?} is no interval; write it as \"5min\", \"1h\", \"1D\" or \"\" for the chart's"
        ))
    })
}

/// `state NAME = LITERAL`.
fn state_decl(cursor: &mut Cursor, collector: &mut Collector) -> Step<Option<usize>> {
    let name = new_name(cursor, collector)?;
    cursor.symbol("=")?;
    let initial = match cursor.literal()? {
        Lit::Text(text) => Literal::Text(text),
        Lit::Int(value) => Literal::Int(value),
        Lit::Float(value) => Literal::Number(value),
        Lit::Bool(value) => Literal::Bool(value),
        Lit::Ident(name) if name == "na" => Literal::Number(f64::NAN),
        Lit::Ident(_) => {
            return Err(Stop::Fail(
                "a state starts from a literal: a number, a string, true, false or na".into(),
            ));
        }
    };
    if collector.spec.states.len() == MAX_STATES {
        return Err(Stop::Fail(format!(
            "a script declares at most {MAX_STATES} states"
        )));
    }
    collector.spec.states.push(StateDecl {
        name: name.to_owned(),
        initial,
    });
    Ok(None)
}

/// `series NAME = EXPR`.
fn series(cursor: &mut Cursor, collector: &mut Collector) -> Step<Option<usize>> {
    let name = new_name(cursor, collector)?;
    cursor.symbol("=")?;
    cursor.expr()?;
    if collector.spec.series.len() == MAX_SERIES {
        return Err(Stop::Fail(format!(
            "a script declares at most {MAX_SERIES} series"
        )));
    }
    collector.spec.series.push(SeriesDecl {
        name: name.to_owned(),
    });
    Ok(Some(collector.spec.series.len() - 1))
}

/// `bars NAME = history(SYMBOL, INTERVAL)`.
fn bars(cursor: &mut Cursor, collector: &mut Collector) -> Step<Option<usize>> {
    let name = new_name(cursor, collector)?;
    cursor.symbol("=")?;
    if cursor.at_end() && cursor.look != "history" {
        return Err(Stop::Fail("expecting history(SYMBOL, INTERVAL)".into()));
    }
    cursor.ident()?;
    cursor.symbol("(")?;
    let inputs = &collector.spec.inputs;
    let symbol = text_or_input(&cursor.literal()?, inputs, "a symbol", |kind| {
        matches!(kind, InputKind::Symbol)
    })?;
    cursor.symbol(",")?;
    let interval = text_or_input(&cursor.literal()?, inputs, "an interval", |kind| {
        matches!(kind, InputKind::Timeframe)
    })?;
    if let TextRef::Literal(text) = &interval {
        self::interval(text)?;
    }
    cursor.symbol(")")?;
    if collector.spec.histories.len() == MAX_HISTORIES {
        return Err(Stop::Fail(format!(
            "a script declares at most {MAX_HISTORIES} histories"
        )));
    }
    collector.spec.histories.push(HistoryDecl {
        name: name.to_owned(),
        symbol,
        interval,
    });
    Ok(None)
}

/// The string `lit`, or the input of `inputs` it names, which holds `what`.
fn text_or_input(
    lit: &Lit,
    inputs: &[InputDecl],
    what: &str,
    holds: fn(&InputKind) -> bool,
) -> Step<TextRef> {
    let Lit::Ident(name) = lit else {
        let takes = format!("history takes {what} as a string or an input");
        return expect(lit, &takes, text).map(TextRef::Literal);
    };
    inputs
        .iter()
        .position(|input| input.id == *name && holds(&input.kind))
        .map(TextRef::Input)
        .ok_or_else(|| Stop::Fail(format!("{name} is not {what} input declared above")))
}

/// `study(NAME, ID = EXPR, ..., output = TITLE)`, where an id may be a string.
fn study(cursor: &mut Cursor, collector: &mut Collector) -> Step<Option<usize>> {
    cursor.symbol("(")?;
    let name = expect(&cursor.literal()?, "the study's name is a string", text)?;
    let prototype = match &collector.catalogue {
        None => None,
        Some(catalogue) => match catalogue.get(&name) {
            Some(Some(prototype)) => Some(prototype),
            _ => return Err(Stop::Fail(format!("no study {name:?} is registered"))),
        },
    };
    if let Some(prototype) = prototype
        && cursor.at_end()
    {
        let study = prototype.lock();
        if study.calculation().is_some() {
            return Err(Stop::Fail(format!(
                "study {name:?} calculates on its own, so a script cannot read it"
            )));
        }
        if study.line_count() == 0 {
            return Err(Stop::Fail(format!("study {name:?} has no line to read")));
        }
    }
    let mut given: Vec<(&str, usize)> = Vec::new();
    let mut output = None;
    // The expressions of a call are its name, then an id and a value for each option.
    let mut place = 2;
    while cursor.either(&[",", ")"])? == "," {
        let (id, quoted) = cursor.name()?;
        if id == "output" && !quoted {
            if cursor.at_end() && output.is_some() {
                return Err(Stop::Fail("output is given twice".into()));
            }
            cursor.symbol("=")?;
            let title = expect(&cursor.literal()?, "output is a string", text)?;
            // A call that gives no input has the lines of the defaults.
            if let Some(prototype) = prototype
                && given.is_empty()
                && cursor.at_end()
                && cursor.look == ")"
            {
                let names = prototype.lock().line_names();
                if bindings::line(&names, &names, &title).is_none() {
                    return Err(Stop::Fail(bindings::no_line(&name, &names, &title)));
                }
            }
            output = Some(title);
        } else {
            if cursor.at_end() {
                if given.iter().any(|(given, _)| *given == id) {
                    return Err(Stop::Fail(format!("input {id} is given twice")));
                }
                if let Some(prototype) = prototype
                    && !prototype.lock().inputs().iter().any(|input| input.id == id)
                {
                    return Err(Stop::Fail(format!("study {name:?} has no input {id}")));
                }
            }
            if given.len() == MAX_GIVEN {
                return Err(Stop::Fail(format!(
                    "a study call gives at most {MAX_GIVEN} inputs"
                )));
            }
            cursor.symbol("=")?;
            cursor.expr()?;
            given.push((id, place));
        }
        place += 2;
    }
    let Some(prototype) = prototype else {
        collector.called.push(name);
        return Ok(Some(0));
    };
    let inputs = prototype.lock().inputs();
    let inputs = given.into_iter().map(|(id, place)| {
        // The chart's own input follows a study's of the same id.
        let input = inputs.iter().rev().find(|input| input.id == id);
        StudyArg {
            id: id.to_owned(),
            place,
            series: input.is_some_and(|input| matches!(input.kind, InputKind::Source)),
        }
    });
    collector.spec.studies.push(StudyDecl {
        inputs: inputs.collect(),
        name,
        prototype: prototype.clone(),
        output,
    });
    Ok(Some(collector.spec.studies.len() - 1))
}

/// `plot(TITLE, EXPR, color = .., style = .., offset = ..)`.
fn plot(cursor: &mut Cursor, collector: &mut Collector) -> Step<Option<usize>> {
    cursor.symbol("(")?;
    let title = cursor.literal()?;
    let title = expect(&title, "the title is a string", text)?;
    if cursor.at_end() {
        if title.trim().is_empty() {
            return Err(Stop::Fail("the title is empty".into()));
        }
        if collector.spec.plots.iter().any(|plot| plot.title == title) {
            return Err(Stop::Fail(format!("plot {title:?} is declared twice")));
        }
    }
    cursor.symbol(",")?;
    cursor.expr()?;
    let mut plot = PlotDecl {
        title,
        color: None,
        style: PlotType::Line,
        offset: 0,
    };
    for (name, value) in cursor.options(&["color", "style", "offset"])? {
        match name {
            "color" => plot.color = Some(expect(&value, COLOR, color)?),
            "style" => {
                plot.style = expect(&value, "style takes a plot style such as \"line\"", |lit| {
                    let style = text(lit)?;
                    STYLES
                        .iter()
                        .find(|(name, _)| *name == style)
                        .map(|(_, style)| *style)
                })?;
            }
            _ => {
                let offset = expect(&value, "offset is an int", int)?;
                plot.offset = isize::try_from(offset)
                    .ok()
                    .filter(|offset| offset.unsigned_abs() <= 10_000)
                    .ok_or_else(|| {
                        Stop::Fail(format!("offset {offset} is outside -10000 to 10000"))
                    })?;
            }
        }
    }
    if collector.spec.plots.len() == MAX_PLOTS {
        return Err(Stop::Fail(format!(
            "a script declares at most {MAX_PLOTS} plots"
        )));
    }
    collector.spec.plots.push(plot);
    Ok(Some(collector.spec.plots.len() - 1))
}

/// `fill(A, B, color = .., inverted = ..)` between plot titles or levels.
fn fill(cursor: &mut Cursor, collector: &mut Collector) -> Step<Option<usize>> {
    cursor.symbol("(")?;
    let boundary = |cursor: &mut Cursor| -> Step<FillBoundary> {
        let lit = cursor.literal()?;
        if let Some(level) = lit.number().filter(|level| level.is_finite()) {
            return Ok(FillBoundary::Level(level));
        }
        let title = expect(&lit, "a fill boundary is a plot title or a number", text)?;
        collector
            .spec
            .plots
            .iter()
            .position(|plot| plot.title == title)
            .map(FillBoundary::Plot)
            .ok_or_else(|| Stop::Fail(format!("no plot {title:?} is declared before this fill")))
    };
    let first = boundary(cursor)?;
    cursor.symbol(",")?;
    let second = boundary(cursor)?;
    let mut fill = FillDecl {
        first,
        second,
        color: None,
        inverted: None,
    };
    for (name, value) in cursor.options(&["color", "inverted"])? {
        let color = Some(expect(&value, COLOR, color)?);
        match name {
            "color" => fill.color = color,
            _ => fill.inverted = color,
        }
    }
    collector.spec.fills.push(fill);
    Ok(None)
}

/// Where the declarations `keyword NAME` of `source` name each of `names`: the first such
/// declaration of each name outside comments and strings.
pub(crate) fn spans<'a>(
    source: &str,
    keyword: &str,
    names: impl IntoIterator<Item = &'a str>,
) -> Vec<Span> {
    let names: Vec<_> = names.into_iter().collect();
    let mut found = vec![Span::default(); names.len()];
    let mut previous = None;
    for (token, span) in tokens(source) {
        if previous == Some(Token::Word(keyword))
            && let Token::Word(word) = token
            && let Some(at) = names.iter().position(|name| *name == word)
            && found[at] == Span::default()
        {
            found[at] = span;
        }
        previous = Some(token);
    }
    found
}

/// A piece of a script's text outside comments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Token<'a> {
    /// A name, a keyword or a number.
    Word(&'a str),
    /// The text of a string or a character, as written.
    Text(&'a str),
    Symbol(char),
}

/// A script's text, its position moving over each character.
struct Reader<'a> {
    rest: &'a str,
    line: u32,
    column: u32,
}

impl<'a> Reader<'a> {
    fn next(&mut self) -> Option<char> {
        let c = self.rest.chars().next()?;
        self.rest = &self.rest[c.len_utf8()..];
        if c == '\n' {
            (self.line, self.column) = (self.line + 1, 0);
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn eat(&mut self, text: &str) -> bool {
        let found = self.rest.starts_with(text);
        if found {
            for _ in text.chars() {
                self.next();
            }
        }
        found
    }

    /// What was read since the text left was `from`.
    fn since(&self, from: &'a str) -> &'a str {
        &from[..from.len() - self.rest.len()]
    }

    /// A string between backticks up to its end or to the `${` of an expression in it; whether
    /// an expression follows.
    fn template(&mut self) -> (&'a str, bool) {
        let from = self.rest;
        loop {
            let text = self.since(from);
            if self.eat("${") {
                return (text, true);
            }
            if !self.eat("\\${") && matches!(self.next(), Some('`') | None) {
                return (text, false);
            }
        }
    }
}

/// The tokens of `source`, a script that compiled, with their positions.
pub(crate) fn tokens(source: &str) -> Vec<(Token<'_>, Span)> {
    let mut tokens = Vec::new();
    let mut reader = Reader {
        rest: source,
        line: 1,
        column: 0,
    };
    // The braces open in each expression of a string between backticks around this place.
    let mut braces: Vec<usize> = Vec::new();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    loop {
        let from = reader.rest;
        let span = Span {
            line: reader.line,
            column: reader.column + 1,
        };
        let Some(c) = reader.next() else {
            return tokens;
        };
        let token = match c {
            '/' if reader.eat("/") => {
                while !reader.rest.is_empty() && !reader.rest.starts_with(['\r', '\n']) {
                    reader.next();
                }
                // Rhai ends a comment, and its line, at a lone carriage return too.
                if reader.eat("\r") && !reader.rest.starts_with('\n') {
                    (reader.line, reader.column) = (reader.line + 1, 0);
                }
                continue;
            }
            '/' if reader.eat("*") => {
                let mut depth = 1;
                while depth > 0 && !reader.rest.is_empty() {
                    if reader.eat("/*") {
                        depth += 1;
                    } else if reader.eat("*/") {
                        depth -= 1;
                    } else {
                        reader.next();
                    }
                }
                continue;
            }
            '"' | '\'' => {
                let from = reader.rest;
                loop {
                    let text = reader.since(from);
                    match reader.next() {
                        Some('\\') => {
                            reader.next();
                        }
                        Some(next) if next != c => {}
                        _ => break Token::Text(text),
                    }
                }
            }
            '#' if from.trim_start_matches('#').starts_with('"') => {
                let hashes = from.len() - from.trim_start_matches('#').len();
                let end = format!("\"{}", "#".repeat(hashes));
                reader.eat(&from[1..=hashes]);
                let from = reader.rest;
                loop {
                    let text = reader.since(from);
                    if reader.eat(&end) || reader.next().is_none() {
                        break Token::Text(text);
                    }
                }
            }
            '`' => {
                let (text, expression) = reader.template();
                if expression {
                    braces.push(0);
                }
                Token::Text(text)
            }
            '}' if braces.last() == Some(&0) => {
                let (text, expression) = reader.template();
                if !expression {
                    braces.pop();
                }
                Token::Text(text)
            }
            c if c.is_whitespace() => continue,
            c if word(c) => {
                while reader.rest.starts_with(word) {
                    reader.next();
                }
                Token::Word(reader.since(from))
            }
            c => {
                match (c, braces.last_mut()) {
                    ('{', Some(open)) => *open += 1,
                    ('}', Some(open)) => *open -= 1,
                    _ => {}
                }
                Token::Symbol(c)
            }
        };
        tokens.push((token, span));
    }
}
