//! The Rhai front end: compiles a script into its declarations and a per-bar program, and runs
//! that program on the runtime's bars.

mod alerts;
mod calls;
mod drawings;
pub(crate) mod engine;
mod errors;
pub(crate) mod syntax;

use crate::ScriptError;
use crate::runtime::program::{Literal, ScriptSpec, Span};
use crate::runtime::series::SeriesRef;
use crate::runtime::{self, BarError, Program, bindings};
use ::rhai::{
    AST, Array, Blob, Dynamic, Engine, EvalAltResult, EvalContext, ImmutableString, Map, Position,
    Scope,
};
use egui_charts::studies::{IndicatorFactory, InputKind, InputValue, StudySource};
use engine::{BarsHandle, Handle};
use std::sync::{Arc, Mutex};

/// A compiled script: its declarations, its program and the engine that runs it.
pub(crate) struct Compiled {
    pub name: String,
    pub spec: Arc<ScriptSpec>,
    pub series_spans: Arc<[Span]>,
    pub history_spans: Arc<[Span]>,
    /// Where each `state` declaration is, for size and memory reports.
    state_spans: Vec<Span>,
    /// Whether the script places orders, which makes it a strategy.
    pub orders: bool,
    engine: Engine,
    ast: AST,
    source: String,
}

/// Compile `source` in two passes: the first reads the declarations and reports syntax errors,
/// the second compiles with every declared name in scope, refuses any other name and checks the
/// studies the script calls against `factory`. A call to a function nothing defines is refused
/// too, as is an `alert` call whose message is not a plain string and a drawing call that names
/// no declared plot by a plain string; the error returned is the first in the source.
pub(crate) fn compile(
    name: String,
    source: &str,
    factory: &IndicatorFactory,
) -> Result<Compiled, ScriptError> {
    let pass = |scope: &Scope, catalogue: Option<syntax::Catalogue>| {
        let strict = catalogue.is_some();
        let collector = Arc::new(Mutex::new(syntax::Collector::new(catalogue)));
        let mut engine = engine::engine();
        syntax::register(&mut engine, &collector);
        engine.set_strict_variables(strict);
        let ast = engine
            .compile_with_scope(scope, source)
            .map_err(|error| errors::compile(&error, source))?;
        let mut collector = collector.lock().unwrap_or_else(|e| e.into_inner());
        let spec = std::mem::take(&mut collector.spec);
        let called = std::mem::take(&mut collector.called);
        Ok::<_, ScriptError>((engine, ast, spec, called))
    };
    let (_, ast, spec, called) = pass(&Scope::new(), None)?;
    let orders = places_orders(source, &ast);
    if spec.plots.is_empty() && !orders {
        return Err(ScriptError::at(
            "a script needs at least one plot".into(),
            Span { line: 1, column: 1 },
            None,
        ));
    }
    let defaults: Vec<_> = spec
        .inputs
        .iter()
        .map(|input| input.default.clone())
        .collect();
    let mut catalogue = syntax::Catalogue::new();
    for study in called {
        catalogue
            .entry(study)
            .or_insert_with_key(|study| bindings::prototype(factory, study));
    }
    // The first of the errors in the source, a name, a call, an alert or a drawing.
    let unknown = calls::unknown(source, &ast);
    let alerts = alerts::declared(source, &ast);
    let drawings = drawings::called(source, &ast, &spec.plots);
    let strict = pass(&scope(&spec, &defaults, false, false), Some(catalogue));
    let first = (strict.as_ref().err().into_iter())
        .chain(&unknown)
        .chain(alerts.as_ref().err())
        .chain(drawings.as_ref().err());
    if let Some(error) = first.min_by_key(|error| (error.line, error.column)) {
        return Err(error.clone());
    }
    let (engine, ast, mut spec, _) = strict?;
    spec.alerts = alerts?;
    spec.calls = drawings?;
    Ok(Compiled {
        name,
        series_spans: syntax::spans(source, "series", spec.series.iter().map(|s| &*s.name)).into(),
        history_spans: syntax::spans(source, "bars", spec.histories.iter().map(|h| &*h.name))
            .into(),
        state_spans: syntax::spans(source, "state", spec.states.iter().map(|s| &*s.name)),
        orders,
        spec: Arc::new(spec),
        engine,
        ast,
        source: source.to_owned(),
    })
}

/// Whether `source`, which compiled into `ast`, calls an order function it does not define.
fn places_orders(source: &str, ast: &AST) -> bool {
    const ORDERS: [&str; 4] = ["entry", "exit_order", "exit_relative", "cancel"];
    let tokens = syntax::tokens(source);
    tokens.windows(2).any(|pair| match pair {
        [
            (syntax::Token::Word(name), _),
            (syntax::Token::Symbol('('), _),
        ] => ORDERS.contains(name) && !ast.iter_functions().any(|function| function.name == *name),
        _ => false,
    })
}

/// The names a script reads, in a fixed order: `na`, the price series, the inputs with
/// `inputs`, the declared series, the other histories and the states at their initial values.
/// All but the states are constants at run time; to compile they are plain variables, so the
/// optimizer folds no input's default into the program.
fn scope(spec: &ScriptSpec, inputs: &[InputValue], fed: bool, constants: bool) -> Scope<'static> {
    let mut scope = Scope::with_capacity(state_slots(spec).end);
    let push = |scope: &mut Scope, name: &str, value: Dynamic| {
        if constants {
            scope.push_constant_dynamic(name.to_owned(), value);
        } else {
            scope.push_dynamic(name.to_owned(), value);
        }
    };
    push(&mut scope, syntax::NA.0, Dynamic::from_float(f64::NAN));
    for (name, series, _) in syntax::PRICES {
        push(&mut scope, name, Dynamic::from(Handle(series)));
    }
    for (at, input) in spec.inputs.iter().enumerate() {
        let value = inputs.get(at).unwrap_or(&input.default);
        let value = match (value, &input.kind) {
            (InputValue::Length(value), _) => Dynamic::from_int(*value as i64),
            (InputValue::Integer(value), _) => Dynamic::from_int(i64::from(*value)),
            (InputValue::Number(value), _) => Dynamic::from_float(*value),
            (InputValue::Toggle(value), _) => Dynamic::from_bool(*value),
            (InputValue::Color(color), _) => Dynamic::from(drawings::ColorValue(*color)),
            (InputValue::Choice(at), InputKind::Choice(choices)) => {
                Dynamic::from(choices.get(*at).cloned().unwrap_or_default())
            }
            (InputValue::Source(_), _) if fed && input.id == "source" => {
                Dynamic::from(Handle(SeriesRef::Fed))
            }
            (InputValue::Source(StudySource::Price(price)), _) => {
                Dynamic::from(Handle(SeriesRef::Price(*price)))
            }
            (InputValue::Text(text) | InputValue::Symbol(text), _) => Dynamic::from(text.clone()),
            (InputValue::Timeframe(interval), _) => {
                Dynamic::from(interval.map_or_else(String::new, |interval| interval.to_string()))
            }
            _ => Dynamic::from_float(f64::NAN),
        };
        push(&mut scope, &input.id, value);
    }
    for (at, series) in spec.series.iter().enumerate() {
        push(
            &mut scope,
            &series.name,
            Dynamic::from(Handle(SeriesRef::Declared(at))),
        );
    }
    for (at, history) in spec.histories.iter().enumerate() {
        push(&mut scope, &history.name, Dynamic::from(BarsHandle(at)));
    }
    for state in &spec.states {
        let value = match &state.initial {
            Literal::Number(value) => Dynamic::from_float(*value),
            Literal::Int(value) => Dynamic::from_int(*value),
            Literal::Bool(value) => Dynamic::from_bool(*value),
            Literal::Text(value) => Dynamic::from(value.clone()),
        };
        scope.push_dynamic(state.name.clone(), value);
    }
    scope
}

impl Program for Compiled {
    type Slots = Scope<'static>;

    fn slots(&self, inputs: &[InputValue], fed: bool) -> Scope<'static> {
        scope(&self.spec, inputs, fed, true)
    }

    fn bar(&self, scope: &mut Scope<'static>) -> Result<(), BarError> {
        let prefix = scope.len();
        runtime::with(|run| run.count_bar(address(scope), prefix));
        let result = self.engine.run_ast_with_scope(scope, &self.ast);
        scope.rewind(prefix);
        let mut kept = Vec::with_capacity(self.spec.states.len());
        let mut oversized = None;
        for (state, &span) in self.spec.states.iter().zip(&self.state_spans) {
            let Some(value) = scope.get_mut(&state.name) else {
                continue;
            };
            // A state carries a series' value on this bar, not a handle that would read the next.
            if value.is::<Handle>()
                && let Some(series) = value.clone().try_cast::<Handle>()
            {
                *value = Dynamic::from_float(series.value());
            }
            // Rhai checks a map's size where a function passes it, not where an index adds a key.
            if let Err(error) = self.engine.ensure_data_size_within_limits(value) {
                let message = errors::runtime(&error, &self.source).message;
                oversized.get_or_insert(BarError { message, span });
            }
            shrink(value);
            kept.push((held(value), span));
        }
        runtime::with(|run| run.kept = kept);
        // A state past a size limit is reported at its declaration, also when the count inside
        // the bar found it first.
        match oversized {
            Some(error) => Err(error),
            None => result.map_err(|error| errors::runtime(&error, &self.source)),
        }
    }
}

/// The count inside a bar, made at a variable read, of the values in the scope running there, the
/// script's top level's or a function call's, which a call written `f!()` shares with its caller:
/// all of them on the first read after each clock read and on the first read of a call with a
/// scope of its own, and on any other read those bound since the scope's last count. They are held
/// to the size limits and counted against the memory limit with the last counts of the callers'
/// scopes. Rhai checks a value's size where a function returns it or an operator assigns it, not
/// where an index assigns into it or a variable copies it.
pub(crate) fn count(context: &EvalContext) -> Result<(), Box<EvalAltResult>> {
    let scope = context.scope();
    let len = scope.len();
    let stopped = runtime::with(|run| -> Result<bool, Box<EvalAltResult>> {
        let Some((from, clock, top)) = run.count_due(context.call_level(), address(scope), len)
        else {
            return Ok(false);
        };
        let states = if top { state_slots(&run.spec) } else { 0..0 };
        // A method call's receiver counts with the variables its call starts with.
        let this = context.this_ptr().filter(|_| from == 0);
        // Rhai lists the variables of a scope from the last one bound.
        let values = || scope.iter_raw().take(len - from).map(|(_, _, value)| value);
        for value in values().chain(this) {
            context.engine().ensure_data_size_within_limits(value)?;
        }
        let variables = (from..len)
            .rev()
            .zip(values())
            .map(|(at, value)| (held(value), !states.contains(&at)));
        Ok(run.count(variables, this.map_or(0, held), clock))
    });
    match stopped {
        Some(Err(error)) => Err(error),
        Some(Ok(true)) => Err(EvalAltResult::ErrorTerminated(Dynamic::UNIT, Position::NONE).into()),
        _ => Ok(()),
    }
}

/// The address of `scope`, which names it while its call runs.
fn address(scope: &Scope) -> usize {
    std::ptr::from_ref(scope).addr()
}

/// Where the states are in the scope of [`scope`], after `na`, the prices, the inputs, the
/// series and the other histories.
fn state_slots(spec: &ScriptSpec) -> std::ops::Range<usize> {
    let declared = spec.inputs.len() + spec.series.len() + spec.histories.len();
    let first = 1 + syntax::PRICES.len() + declared;
    first..first + spec.states.len()
}

/// The bytes `value` holds beyond its own size: the text of keys, the items of maps, and the
/// room strings, arrays and blobs hold, which a cleared one keeps.
fn held(value: &Dynamic) -> usize {
    let item = std::mem::size_of::<Dynamic>();
    if let Some(text) = value.read_lock::<ImmutableString>() {
        // The text shares a box with its counts of references, and has room of its own once it
        // is too long to fit there.
        let text: &Text = (*text).as_ref();
        let room = if text.is_inline() { 0 } else { text.capacity() };
        std::mem::size_of::<(usize, usize, Text)>() + room
    } else if let Some(array) = value.read_lock::<Array>() {
        array.capacity() * item + array.iter().map(held).sum::<usize>()
    } else if let Some(map) = value.read_lock::<Map>() {
        map.iter()
            .map(|(key, value)| key.len() + item + held(value))
            .sum()
    } else if let Some(blob) = value.read_lock::<Blob>() {
        blob.capacity()
    } else {
        0
    }
}

/// The string type of Rhai's strings and map keys, which Rhai names only as the keys of a map.
type Text = <Map as Keyed>::Key;

trait Keyed {
    type Key;
}

impl<K, V> Keyed for std::collections::BTreeMap<K, V> {
    type Key = K;
}

/// Free the room the strings in `value` hold beyond their text, which a string another value
/// shares keeps.
fn shrink(value: &mut Dynamic) {
    if let Some(mut text) = value.write_lock::<ImmutableString>() {
        if let Some(text) = text.get_mut() {
            text.shrink_to_fit();
        }
    } else if let Some(mut array) = value.write_lock::<Array>() {
        array.iter_mut().for_each(shrink);
    } else if let Some(mut map) = value.write_lock::<Map>() {
        map.values_mut().for_each(shrink);
    }
}
