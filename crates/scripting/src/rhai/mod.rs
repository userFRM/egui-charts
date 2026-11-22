//! The Rhai front end: compiles a script into its declarations and a per-bar program, and runs
//! that program on the runtime's bars.

pub(crate) mod engine;
mod errors;
pub(crate) mod syntax;

use crate::ScriptError;
use crate::runtime::program::{Literal, ScriptSpec, Span};
use crate::runtime::series::SeriesRef;
use crate::runtime::{self, BarError, Program};
use ::rhai::{AST, Array, Blob, Dynamic, Engine, ImmutableString, Map, Scope};
use egui_charts::studies::{InputKind, InputValue, StudySource};
use engine::Handle;
use std::sync::{Arc, Mutex};

/// A compiled script: its declarations, its program and the engine that runs it.
pub(crate) struct Compiled {
    pub name: String,
    pub spec: Arc<ScriptSpec>,
    pub series_spans: Arc<[Span]>,
    /// Where each `state` declaration is, for size and memory reports.
    state_spans: Vec<Span>,
    engine: Engine,
    ast: AST,
    source: String,
}

/// Compile `source` in two passes: the first reads the declarations and reports syntax errors,
/// the second compiles with every declared name in scope and refuses any other name.
pub(crate) fn compile(name: String, source: &str) -> Result<Compiled, ScriptError> {
    let pass = |strict: bool, scope: &Scope| {
        let collector = Arc::new(Mutex::new(syntax::Collector::default()));
        let mut engine = engine::engine();
        syntax::register(&mut engine, &collector);
        engine.set_strict_variables(strict);
        let ast = engine
            .compile_with_scope(scope, source)
            .map_err(|error| errors::compile(&error, source))?;
        let spec = std::mem::take(&mut collector.lock().unwrap_or_else(|e| e.into_inner()).spec);
        Ok::<_, ScriptError>((engine, ast, spec))
    };
    let (_, _, spec) = pass(false, &Scope::new())?;
    if spec.plots.is_empty() {
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
    let (engine, ast, spec) = pass(true, &scope(&spec, &defaults, false, false))?;
    Ok(Compiled {
        name,
        series_spans: syntax::spans(source, "series", spec.series.iter().map(|s| &*s.name)).into(),
        state_spans: syntax::spans(source, "state", spec.states.iter().map(|s| &*s.name)),
        spec: Arc::new(spec),
        engine,
        ast,
        source: source.to_owned(),
    })
}

/// The names a script reads, in a fixed order: `na`, the price series, the inputs with
/// `inputs`, the declared series and the states at their initial values. All but the states
/// are constants at run time; to compile they are plain variables, so the optimizer folds no
/// input's default into the program.
fn scope(spec: &ScriptSpec, inputs: &[InputValue], fed: bool, constants: bool) -> Scope<'static> {
    let mut scope =
        Scope::with_capacity(11 + spec.inputs.len() + spec.series.len() + spec.states.len());
    let push = |scope: &mut Scope, name: &str, value: Dynamic| {
        if constants {
            scope.push_constant_dynamic(name.to_owned(), value);
        } else {
            scope.push_dynamic(name.to_owned(), value);
        }
    };
    push(&mut scope, "na", Dynamic::from_float(f64::NAN));
    for (name, series) in syntax::PRICES {
        push(&mut scope, name, Dynamic::from(Handle(series)));
    }
    for (at, input) in spec.inputs.iter().enumerate() {
        let value = inputs.get(at).unwrap_or(&input.default);
        let value = match (value, &input.kind) {
            (InputValue::Length(value), _) => Dynamic::from_int(*value as i64),
            (InputValue::Integer(value), _) => Dynamic::from_int(i64::from(*value)),
            (InputValue::Number(value), _) => Dynamic::from_float(*value),
            (InputValue::Toggle(value), _) => Dynamic::from_bool(*value),
            (InputValue::Choice(at), InputKind::Choice(choices)) => {
                Dynamic::from(choices.get(*at).cloned().unwrap_or_default())
            }
            (InputValue::Source(_), _) if fed && input.id == "source" => {
                Dynamic::from(Handle(SeriesRef::Fed))
            }
            (InputValue::Source(StudySource::Price(price)), _) => {
                Dynamic::from(Handle(SeriesRef::Price(*price)))
            }
            (InputValue::Text(text), _) => Dynamic::from(text.clone()),
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
            kept.push((held(value), span));
        }
        runtime::with(|run| run.kept = kept);
        result.map_err(|error| errors::runtime(&error, &self.source))?;
        oversized.map_or(Ok(()), Err)
    }
}

/// The bytes `value` holds beyond its own size: the text of strings and keys, and the items of
/// arrays and maps.
fn held(value: &Dynamic) -> usize {
    let item = std::mem::size_of::<Dynamic>();
    if let Some(text) = value.read_lock::<ImmutableString>() {
        text.len()
    } else if let Some(array) = value.read_lock::<Array>() {
        array.iter().map(|value| item + held(value)).sum()
    } else if let Some(map) = value.read_lock::<Map>() {
        map.iter()
            .map(|(key, value)| key.len() + item + held(value))
            .sum()
    } else if let Some(blob) = value.read_lock::<Blob>() {
        blob.len()
    } else {
        0
    }
}
