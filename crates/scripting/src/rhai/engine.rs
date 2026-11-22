//! The engine scripts run on: Rhai's standard package and nothing that reaches outside the
//! script, the size limits, the series type with its operators and history, and the helpers.

use crate::runtime::{self, program::Span, series::SeriesRef};
use ::rhai::packages::{Package, StandardPackage};
use ::rhai::{Engine, EvalAltResult, FLOAT, INT, NativeCallContext, Position};
use chrono::{Datelike, Timelike};

/// Deepest nesting of script function calls.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_CALL_LEVELS: usize = 32;
/// Deepest nesting of script function calls on the web, where scripts run on the page's thread:
/// its 1 MiB stack holds the deepest scripts this far in a debug build, with a quarter to spare.
#[cfg(target_arch = "wasm32")]
pub(crate) const MAX_CALL_LEVELS: usize = 6;
/// Deepest nesting of expressions, at the top level and in functions.
pub(crate) const MAX_EXPR_DEPTHS: (usize, usize) = (64, 32);
/// Longest string, in bytes.
pub(crate) const MAX_STRING_SIZE: usize = 10_000;
/// Longest array.
pub(crate) const MAX_ARRAY_SIZE: usize = 10_000;
/// Largest map.
pub(crate) const MAX_MAP_SIZE: usize = 1_000;
/// Most script functions.
pub(crate) const MAX_FUNCTIONS: usize = 64;
/// Most variables in scope at once.
pub(crate) const MAX_VARIABLES: usize = 256;

type Result<T> = std::result::Result<T, Box<EvalAltResult>>;

/// A column with history, as a script sees `close` or a declared series: its value on the
/// current bar in arithmetic, `s[n]` its value `n` bars ago.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Handle(pub SeriesRef);

impl Handle {
    /// The value on the bar being calculated.
    pub fn value(self) -> f64 {
        runtime::with(|run| run.columns.value(self.0, run.bar)).unwrap_or(f64::NAN)
    }

    fn at(self, offset: INT) -> Result<FLOAT> {
        runtime::with(|run| run.columns.at(self.0, run.bar, offset))
            .unwrap_or(Ok(f64::NAN))
            .map_err(Into::into)
    }
}

/// The value a number, an int or a series stands for on the current bar.
pub(crate) trait Num {
    fn num(&self) -> f64;
}

impl Num for FLOAT {
    fn num(&self) -> f64 {
        *self
    }
}

impl Num for INT {
    fn num(&self) -> f64 {
        *self as f64
    }
}

impl Num for Handle {
    fn num(&self) -> f64 {
        self.value()
    }
}

/// Registers `$f(f64) -> R` for a number, an int and a series.
macro_rules! unary {
    ($engine:expr, $name:expr, $f:expr) => {{
        let f = $f;
        $engine.register_fn($name, move |a: FLOAT| f(a.num()));
        $engine.register_fn($name, move |a: INT| f(a.num()));
        $engine.register_fn($name, move |a: Handle| f(a.num()));
    }};
}

/// Registers `$f(f64, f64) -> R` for every pair of numbers, ints and series, or with `series`
/// only the pairs with a series, for operators the engine defines on numbers itself.
macro_rules! binary {
    ($engine:expr, $name:expr, $f:expr) => {{
        let f = $f;
        binary!(@pair $engine, $name, f, FLOAT, FLOAT);
        binary!(@pair $engine, $name, f, FLOAT, INT);
        binary!(@pair $engine, $name, f, INT, FLOAT);
        binary!(@pair $engine, $name, f, INT, INT);
        binary!(series $engine, $name, f);
    }};
    (series $engine:expr, $name:expr, $f:expr) => {{
        let f = $f;
        binary!(@pair $engine, $name, f, Handle, Handle);
        binary!(@pair $engine, $name, f, Handle, FLOAT);
        binary!(@pair $engine, $name, f, Handle, INT);
        binary!(@pair $engine, $name, f, FLOAT, Handle);
        binary!(@pair $engine, $name, f, INT, Handle);
    }};
    (@pair $engine:expr, $name:expr, $f:ident, $a:ty, $b:ty) => {
        $engine.register_fn($name, move |a: $a, b: $b| $f(a.num(), b.num()));
    };
}

/// Registers `clamp` for every triple of numbers, ints and series.
macro_rules! clamp {
    ($engine:expr) => {
        clamp!(@a $engine, FLOAT);
        clamp!(@a $engine, INT);
        clamp!(@a $engine, Handle);
    };
    (@a $engine:expr, $a:ty) => {
        clamp!(@b $engine, $a, FLOAT);
        clamp!(@b $engine, $a, INT);
        clamp!(@b $engine, $a, Handle);
    };
    (@b $engine:expr, $a:ty, $b:ty) => {
        clamp!(@c $engine, $a, $b, FLOAT);
        clamp!(@c $engine, $a, $b, INT);
        clamp!(@c $engine, $a, $b, Handle);
    };
    (@c $engine:expr, $a:ty, $b:ty, $c:ty) => {
        $engine.register_fn("clamp", |x: $a, lo: $b, hi: $c| clamp(x.num(), lo.num(), hi.num()));
    };
}

/// Registers streaming helper `$name(x, n)` for a number, an int or a series `x` and an int
/// or whole-number length.
macro_rules! streaming {
    ($engine:expr, $name:literal) => {
        streaming!(@x $engine, $name, FLOAT);
        streaming!(@x $engine, $name, INT);
        streaming!(@x $engine, $name, Handle);
    };
    (@x $engine:expr, $name:literal, $x:ty) => {
        $engine.register_fn($name, |context: NativeCallContext, x: $x, n: INT| {
            step(&context, $name, n, x.num(), f64::NAN)
        });
        $engine.register_fn($name, |context: NativeCallContext, x: $x, n: FLOAT| {
            step(&context, $name, whole($name, n)?, x.num(), f64::NAN)
        });
    };
}

/// Registers crossing helper `$name(a, b)` for every pair of numbers, ints and series.
macro_rules! crossing {
    ($engine:expr, $name:literal) => {
        crossing!(@a $engine, $name, FLOAT);
        crossing!(@a $engine, $name, INT);
        crossing!(@a $engine, $name, Handle);
    };
    (@a $engine:expr, $name:literal, $a:ty) => {
        crossing!(@b $engine, $name, $a, FLOAT);
        crossing!(@b $engine, $name, $a, INT);
        crossing!(@b $engine, $name, $a, Handle);
    };
    (@b $engine:expr, $name:literal, $a:ty, $b:ty) => {
        $engine.register_fn($name, |context: NativeCallContext, a: $a, b: $b| {
            step(&context, $name, 0, a.num(), b.num()).map(|crosses| crosses != 0.0)
        });
    };
}

/// The engine for running and compiling scripts, without the declarations.
pub(crate) fn engine() -> Engine {
    let mut engine = Engine::new_raw();
    engine.register_global_module(StandardPackage::new().as_shared_module());
    engine
        .set_max_call_levels(MAX_CALL_LEVELS)
        .set_max_expr_depths(MAX_EXPR_DEPTHS.0, MAX_EXPR_DEPTHS.1)
        .set_max_string_size(MAX_STRING_SIZE)
        .set_max_array_size(MAX_ARRAY_SIZE)
        .set_max_map_size(MAX_MAP_SIZE)
        .set_max_functions(MAX_FUNCTIONS)
        .set_max_variables(MAX_VARIABLES)
        // The optimizer copies a constant's value into every use, so a constant array read a
        // thousand times is held a thousand times while the script compiles.
        .set_optimization_level(::rhai::OptimizationLevel::None)
        // A closure shares what it captures behind locks that sleep on contention, which
        // panics on the web, and one stored in a value it captures is never freed.
        .set_allow_anonymous_fn(false)
        // Code compiled at run time would bypass the declarations.
        .disable_symbol("eval")
        // `f.curry(f)` doubles a value at each step, out of sight of the size limits.
        .disable_symbol("curry");
    // The standard package's `sleep` blocks the thread, which panics on the web. These come
    // first in every lookup, a function pointer's included, so a script has no such function.
    engine.register_fn("sleep", |_: INT| -> Result<()> {
        Err("no function sleep takes (int)".into())
    });
    engine.register_fn("sleep", |_: FLOAT| -> Result<()> {
        Err("no function sleep takes (number)".into())
    });
    // Rhai's count restarts in each callback of a function such as `filter`, from a copy it then
    // drops, so the run counts the calls itself.
    engine.on_progress(|_| {
        runtime::with(runtime::Run::operation)
            .unwrap_or(false)
            .then_some(::rhai::Dynamic::UNIT)
    });
    // The values of the running scope are counted at a variable read, where they are in reach.
    // Rhai marks `on_var` as an interface that may change, through a deprecation note.
    #[allow(deprecated)]
    engine.on_var(|_, _, context| super::count(&context).map(|()| None));

    engine.register_type_with_name::<Handle>("series");
    binary!(series engine, "+", |a: f64, b: f64| a + b);
    binary!(series engine, "-", |a: f64, b: f64| a - b);
    binary!(series engine, "*", |a: f64, b: f64| a * b);
    binary!(series engine, "/", |a: f64, b: f64| a / b);
    binary!(series engine, "%", |a: f64, b: f64| a % b);
    binary!(series engine, "**", f64::powf);
    binary!(series engine, "<", |a: f64, b: f64| a < b);
    binary!(series engine, "<=", |a: f64, b: f64| a <= b);
    binary!(series engine, ">", |a: f64, b: f64| a > b);
    binary!(series engine, ">=", |a: f64, b: f64| a >= b);
    binary!(series engine, "==", |a: f64, b: f64| a == b);
    binary!(series engine, "!=", |a: f64, b: f64| a != b);
    engine.register_fn("-", |a: Handle| -a.value());
    engine.register_fn("+", |a: Handle| a.value());

    engine.register_indexer_get(|series: &mut Handle, offset: INT| series.at(offset));
    engine.register_indexer_get(|series: &mut Handle, offset: FLOAT| {
        series.at(whole("history", offset)?)
    });
    engine.register_indexer_get(|series: &mut Handle, offset: Handle| {
        series.at(whole("history", offset.value())?)
    });
    engine
        .register_indexer_get(|_: &mut FLOAT, _: INT| -> Result<FLOAT> { Err(NO_HISTORY.into()) });
    engine.register_indexer_get(|_: &mut FLOAT, _: Handle| -> Result<FLOAT> {
        Err(NO_HISTORY.into())
    });
    engine.register_indexer_get(|_: &mut bool, _: INT| -> Result<FLOAT> { Err(NO_HISTORY.into()) });

    engine.register_fn("is_na", |x: FLOAT| x.is_nan());
    engine.register_fn("is_na", |_: INT| false);
    engine.register_fn("is_na", |x: Handle| x.value().is_nan());
    unary!(engine, "nz", |x: f64| if x.is_nan() { 0.0 } else { x });
    binary!(engine, "nz", |x: f64, y: f64| if x.is_nan() {
        y
    } else {
        x
    });

    unary!(engine, "abs", f64::abs);
    unary!(engine, "sign", |x: f64| if x == 0.0 {
        0.0
    } else {
        x.signum()
    });
    unary!(engine, "sqrt", f64::sqrt);
    unary!(engine, "exp", f64::exp);
    unary!(engine, "ln", f64::ln);
    unary!(engine, "log10", f64::log10);
    unary!(engine, "round", f64::round);
    unary!(engine, "floor", f64::floor);
    unary!(engine, "ceil", f64::ceil);
    binary!(engine, "pow", f64::powf);
    binary!(
        engine,
        "min",
        |a: f64, b: f64| if a.is_nan() || b.is_nan() {
            f64::NAN
        } else {
            a.min(b)
        }
    );
    binary!(
        engine,
        "max",
        |a: f64, b: f64| if a.is_nan() || b.is_nan() {
            f64::NAN
        } else {
            a.max(b)
        }
    );
    clamp!(engine);
    engine.register_fn("round", |x: FLOAT, digits: INT| round(x, digits));
    engine.register_fn("round", |x: INT, digits: INT| round(x.num(), digits));
    engine.register_fn("round", |x: Handle, digits: INT| round(x.value(), digits));

    streaming!(engine, "sma");
    streaming!(engine, "ema");
    streaming!(engine, "rma");
    streaming!(engine, "wma");
    streaming!(engine, "sum");
    streaming!(engine, "stdev");
    streaming!(engine, "highest");
    streaming!(engine, "lowest");
    streaming!(engine, "change");
    engine.register_fn("change", |context: NativeCallContext, x: FLOAT| {
        step(&context, "change", 1, x, f64::NAN)
    });
    engine.register_fn("change", |context: NativeCallContext, x: INT| {
        step(&context, "change", 1, x.num(), f64::NAN)
    });
    engine.register_fn("change", |context: NativeCallContext, x: Handle| {
        step(&context, "change", 1, x.value(), f64::NAN)
    });
    crossing!(engine, "crossover");
    crossing!(engine, "crossunder");

    time(&mut engine, "year", |time| i64::from(time.year()));
    time(&mut engine, "month", |time| i64::from(time.month()));
    time(&mut engine, "day", |time| i64::from(time.day()));
    time(&mut engine, "weekday", |time| {
        i64::from(time.weekday().number_from_monday())
    });
    time(&mut engine, "hour", |time| i64::from(time.hour()));
    time(&mut engine, "minute", |time| i64::from(time.minute()));
    engine
}

/// What a value without history says when a script reads an earlier bar of it.
pub(crate) const NO_HISTORY: &str =
    "this value has no history; declare it with `series NAME = ...` to read earlier bars";

/// Feed the streaming helper at the calling position.
fn step(context: &NativeCallContext, name: &str, length: INT, x: f64, b: f64) -> Result<FLOAT> {
    let span = span(context.call_position());
    runtime::with(|run| {
        let occurrence = run.occurrence(span);
        run.step((span, occurrence), name, length, x, b)
    })
    .unwrap_or(Ok(f64::NAN))
    .map_err(|message| EvalAltResult::ErrorRuntime(message.into(), context.call_position()).into())
}

/// A length or offset given as a number, which must be a whole one.
fn whole(name: &str, value: f64) -> Result<INT> {
    if value.fract() == 0.0 && value.abs() < 9.0e15 {
        Ok(value as INT)
    } else {
        Err(format!("{name} {value} is not a whole number").into())
    }
}

fn clamp(x: f64, lo: f64, hi: f64) -> Result<FLOAT> {
    if lo.is_nan() || hi.is_nan() {
        Ok(f64::NAN)
    } else if lo > hi {
        Err(format!("clamp bounds {lo} and {hi} are reversed").into())
    } else {
        Ok(x.clamp(lo, hi))
    }
}

fn round(x: f64, digits: INT) -> FLOAT {
    let scale = 10f64.powi(i32::try_from(digits).unwrap_or(i32::MAX));
    (x * scale).round() / scale
}

/// Registers time helper `name`, reading milliseconds since 1970 in UTC.
fn time(engine: &mut Engine, name: &'static str, read: fn(chrono::DateTime<chrono::Utc>) -> INT) {
    let at = move |millis: f64| -> Result<INT> {
        if !millis.is_finite() {
            return Err(format!("{name} of a missing time").into());
        }
        chrono::DateTime::from_timestamp_millis(millis as i64)
            .map(read)
            .ok_or_else(|| format!("{name} of time {millis} out of range").into())
    };
    engine.register_fn(name, move |t: FLOAT| at(t));
    engine.register_fn(name, move |t: INT| at(t.num()));
    engine.register_fn(name, move |t: Handle| at(t.value()));
}

/// The position Rhai reports as a span.
pub(crate) fn span(position: Position) -> Span {
    Span {
        line: position.line().unwrap_or(0) as u32,
        column: position.position().unwrap_or(0) as u32,
    }
}
