//! The engine scripts run on: Rhai's standard package and nothing that reaches outside the
//! script, the size limits, the series type with its operators and history, the bars of other
//! histories, and the helpers, the orders of strategies and the alerts among them.

use super::syntax::{self, PRICES};
use crate::runtime::orders::{OrderCall, Readings};
use crate::runtime::series::{Field, SeriesRef};
use crate::runtime::{self, bindings, program::Span};
use ::rhai::packages::{Package, StandardPackage};
use ::rhai::{
    Array, Dynamic, Engine, EvalAltResult, FLOAT, INT, ImmutableString, Map, NativeCallContext,
    Position,
};
use chrono::{Datelike, Timelike};
use egui_charts::model::OrderSide;
use std::fmt::Write;

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
/// Longest array, counting the items of the arrays in it.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_ARRAY_SIZE: usize = 10_000;
/// Longest array on the web. Rhai copies, drops and measures a nested value with a call per level,
/// so this limit and the map limit, which count nested items, also bound how deep a value nests:
/// the page's 1 MiB stack holds the deepest value they admit, written to JSON as deep in the
/// deepest call as a script may nest an expression, with an eighth to spare in a debug build.
#[cfg(target_arch = "wasm32")]
pub(crate) const MAX_ARRAY_SIZE: usize = 200;
/// Largest map, counting the entries of the maps in it.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_MAP_SIZE: usize = 1_000;
/// Largest map on the web, for the stack as [`MAX_ARRAY_SIZE`].
#[cfg(target_arch = "wasm32")]
pub(crate) const MAX_MAP_SIZE: usize = 50;
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

    /// The value `offset` bars ago, read at the calling position.
    fn at(self, context: &NativeCallContext, offset: INT) -> Result<FLOAT> {
        let span = span(context.call_position());
        runtime::with(|run| {
            run.mark(span);
            run.columns.at(self.0, run.bar, offset)
        })
        .unwrap_or(Ok(f64::NAN))
        .map_err(Into::into)
    }
}

/// The bars of another history, as a script sees the name of a `bars` declaration: each field is
/// a series.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BarsHandle(pub usize);

impl BarsHandle {
    fn field(&mut self, field: Field) -> Handle {
        Handle(SeriesRef::History { at: self.0, field })
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
    ($engine:expr, $name:expr) => {
        clamp!(@a $engine, $name, FLOAT);
        clamp!(@a $engine, $name, INT);
        clamp!(@a $engine, $name, Handle);
    };
    (@a $engine:expr, $name:expr, $a:ty) => {
        clamp!(@b $engine, $name, $a, FLOAT);
        clamp!(@b $engine, $name, $a, INT);
        clamp!(@b $engine, $name, $a, Handle);
    };
    (@b $engine:expr, $name:expr, $a:ty, $b:ty) => {
        clamp!(@c $engine, $name, $a, $b, FLOAT);
        clamp!(@c $engine, $name, $a, $b, INT);
        clamp!(@c $engine, $name, $a, $b, Handle);
    };
    (@c $engine:expr, $name:expr, $a:ty, $b:ty, $c:ty) => {
        $engine.register_fn($name, |x: $a, lo: $b, hi: $c| clamp(x.num(), lo.num(), hi.num()));
    };
}

/// Registers streaming helper `$name(x, n)` for a number, an int or a series `x` and an int
/// or whole-number length.
macro_rules! streaming {
    ($engine:expr, $name:expr) => {
        streaming!(@x $engine, $name, FLOAT);
        streaming!(@x $engine, $name, INT);
        streaming!(@x $engine, $name, Handle);
    };
    (@x $engine:expr, $name:expr, $x:ty) => {
        let name = $name;
        $engine.register_fn(name, move |context: NativeCallContext, x: $x, n: INT| {
            step(&context, name, n, x.num(), f64::NAN)
        });
        $engine.register_fn(name, move |context: NativeCallContext, x: $x, n: FLOAT| {
            step(&context, name, whole(name, n)?, x.num(), f64::NAN)
        });
    };
}

/// Registers crossing helper `$name(a, b)` for every pair of numbers, ints and series.
macro_rules! crossing {
    ($engine:expr, $name:expr) => {
        crossing!(@a $engine, $name, FLOAT);
        crossing!(@a $engine, $name, INT);
        crossing!(@a $engine, $name, Handle);
    };
    (@a $engine:expr, $name:expr, $a:ty) => {
        crossing!(@b $engine, $name, $a, FLOAT);
        crossing!(@b $engine, $name, $a, INT);
        crossing!(@b $engine, $name, $a, Handle);
    };
    (@b $engine:expr, $name:expr, $a:ty, $b:ty) => {
        let name = $name;
        $engine.register_fn(name, move |context: NativeCallContext, a: $a, b: $b| {
            step(&context, name, 0, a.num(), b.num()).map(|crosses| crosses != 0.0)
        });
    };
}

/// The group a helper is listed under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Group {
    Missing,
    Math,
    Streaming,
    Time,
    Orders,
    Alerts,
}

impl Group {
    /// The heading the reference lists the group under.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Missing => "Missing values",
            Self::Math => "Math",
            Self::Streaming => "Streaming",
            Self::Time => "Time, in UTC",
            Self::Orders => "Orders",
            Self::Alerts => "Alerts",
        }
    }
}

/// One form of a helper: how a script calls it, what it gives and how the engine registers it.
pub(crate) struct Helper {
    /// The name a script calls.
    pub name: &'static str,
    /// The group the reference lists it under.
    pub group: Group,
    /// The call as a script writes it, such as `sma(x, n)`.
    pub signature: &'static str,
    /// What it gives.
    pub summary: &'static str,
    /// Registers this form under the name, for every number, int and series argument.
    register: fn(&mut Engine, &'static str),
}

/// Every helper form, in the order the reference lists them. The engine registers these and no
/// other helper, and the compile check knows their names.
pub(crate) const HELPERS: &[Helper] = &[
    Helper {
        name: "is_na",
        group: Group::Missing,
        signature: "is_na(x)",
        summary: "Whether x is missing.",
        register: |engine, name| {
            engine.register_fn(name, |x: FLOAT| x.is_nan());
            engine.register_fn(name, |_: INT| false);
            engine.register_fn(name, |x: Handle| x.value().is_nan());
        },
    },
    Helper {
        name: "nz",
        group: Group::Missing,
        signature: "nz(x)",
        summary: "x, or 0 when x is missing.",
        register: |engine, name| unary!(engine, name, |x: f64| if x.is_nan() { 0.0 } else { x }),
    },
    Helper {
        name: "nz",
        group: Group::Missing,
        signature: "nz(x, y)",
        summary: "x, or y when x is missing.",
        register: |engine, name| {
            binary!(engine, name, |x: f64, y: f64| if x.is_nan() {
                y
            } else {
                x
            })
        },
    },
    Helper {
        name: "abs",
        group: Group::Math,
        signature: "abs(x)",
        summary: "The absolute value of x.",
        register: |engine, name| unary!(engine, name, f64::abs),
    },
    Helper {
        name: "sign",
        group: Group::Math,
        signature: "sign(x)",
        summary: "1 when x is above 0, -1 when it is below and 0 for 0.",
        register: |engine, name| {
            unary!(engine, name, |x: f64| if x == 0.0 {
                0.0
            } else {
                x.signum()
            })
        },
    },
    Helper {
        name: "sqrt",
        group: Group::Math,
        signature: "sqrt(x)",
        summary: "The square root of x, na below 0.",
        register: |engine, name| unary!(engine, name, f64::sqrt),
    },
    Helper {
        name: "exp",
        group: Group::Math,
        signature: "exp(x)",
        summary: "e to the power x.",
        register: |engine, name| unary!(engine, name, f64::exp),
    },
    Helper {
        name: "ln",
        group: Group::Math,
        signature: "ln(x)",
        summary: "The natural logarithm of x, na below 0.",
        register: |engine, name| unary!(engine, name, f64::ln),
    },
    Helper {
        name: "log10",
        group: Group::Math,
        signature: "log10(x)",
        summary: "The base 10 logarithm of x, na below 0.",
        register: |engine, name| unary!(engine, name, f64::log10),
    },
    Helper {
        name: "pow",
        group: Group::Math,
        signature: "pow(x, y)",
        summary: "x to the power y.",
        register: |engine, name| binary!(engine, name, f64::powf),
    },
    Helper {
        name: "min",
        group: Group::Math,
        signature: "min(a, b)",
        summary: "The smaller of a and b, na when either is missing.",
        register: |engine, name| {
            binary!(engine, name, |a: f64, b: f64| if a.is_nan() || b.is_nan() {
                f64::NAN
            } else {
                a.min(b)
            })
        },
    },
    Helper {
        name: "max",
        group: Group::Math,
        signature: "max(a, b)",
        summary: "The larger of a and b, na when either is missing.",
        register: |engine, name| {
            binary!(engine, name, |a: f64, b: f64| if a.is_nan() || b.is_nan() {
                f64::NAN
            } else {
                a.max(b)
            })
        },
    },
    Helper {
        name: "clamp",
        group: Group::Math,
        signature: "clamp(x, lo, hi)",
        summary: "x held between lo and hi, na when a bound is missing; an error when lo is above \
                  hi.",
        register: |engine, name| {
            clamp!(engine, name);
        },
    },
    Helper {
        name: "round",
        group: Group::Math,
        signature: "round(x)",
        summary: "x rounded to a whole number, half away from zero.",
        register: |engine, name| unary!(engine, name, f64::round),
    },
    Helper {
        name: "round",
        group: Group::Math,
        signature: "round(x, digits)",
        summary: "x rounded at digits decimals, half away from zero.",
        register: |engine, name| {
            engine.register_fn(name, |x: FLOAT, digits: INT| round(x, digits));
            engine.register_fn(name, |x: INT, digits: INT| round(x.num(), digits));
            engine.register_fn(name, |x: Handle, digits: INT| round(x.value(), digits));
        },
    },
    Helper {
        name: "floor",
        group: Group::Math,
        signature: "floor(x)",
        summary: "The largest whole number not above x.",
        register: |engine, name| unary!(engine, name, f64::floor),
    },
    Helper {
        name: "ceil",
        group: Group::Math,
        signature: "ceil(x)",
        summary: "The smallest whole number not below x.",
        register: |engine, name| unary!(engine, name, f64::ceil),
    },
    Helper {
        name: "sma",
        group: Group::Streaming,
        signature: "sma(x, n)",
        summary: "The mean of the last n values of x; na until there are n and while one is \
                  missing.",
        register: |engine, name| {
            streaming!(engine, name);
        },
    },
    Helper {
        name: "ema",
        group: Group::Streaming,
        signature: "ema(x, n)",
        summary: "The exponential average of x with weight 2 / (n + 1), from its first finite \
                  value; na on a bar where x is missing.",
        register: |engine, name| {
            streaming!(engine, name);
        },
    },
    Helper {
        name: "rma",
        group: Group::Streaming,
        signature: "rma(x, n)",
        summary: "Wilder's average of x with weight 1 / n, seeded by the mean of its first n \
                  finite values; na on a bar where x is missing.",
        register: |engine, name| {
            streaming!(engine, name);
        },
    },
    Helper {
        name: "wma",
        group: Group::Streaming,
        signature: "wma(x, n)",
        summary: "The mean of the last n values of x, weighted n for the newest down to 1 for the \
                  oldest; na until there are n and while one is missing.",
        register: |engine, name| {
            streaming!(engine, name);
        },
    },
    Helper {
        name: "sum",
        group: Group::Streaming,
        signature: "sum(x, n)",
        summary: "The sum of the last n values of x; na until there are n and while one is \
                  missing.",
        register: |engine, name| {
            streaming!(engine, name);
        },
    },
    Helper {
        name: "stdev",
        group: Group::Streaming,
        signature: "stdev(x, n)",
        summary: "The population standard deviation of the last n values of x; na until there \
                  are n and while one is missing.",
        register: |engine, name| {
            streaming!(engine, name);
        },
    },
    Helper {
        name: "highest",
        group: Group::Streaming,
        signature: "highest(x, n)",
        summary: "The highest of the last n values of x, ignoring missing ones.",
        register: |engine, name| {
            streaming!(engine, name);
        },
    },
    Helper {
        name: "lowest",
        group: Group::Streaming,
        signature: "lowest(x, n)",
        summary: "The lowest of the last n values of x, ignoring missing ones.",
        register: |engine, name| {
            streaming!(engine, name);
        },
    },
    Helper {
        name: "change",
        group: Group::Streaming,
        signature: "change(x)",
        summary: "x minus its value on the bar before, na when either is missing.",
        register: |engine, name| {
            engine.register_fn(name, move |context: NativeCallContext, x: FLOAT| {
                step(&context, name, 1, x, f64::NAN)
            });
            engine.register_fn(name, move |context: NativeCallContext, x: INT| {
                step(&context, name, 1, x.num(), f64::NAN)
            });
            engine.register_fn(name, move |context: NativeCallContext, x: Handle| {
                step(&context, name, 1, x.value(), f64::NAN)
            });
        },
    },
    Helper {
        name: "change",
        group: Group::Streaming,
        signature: "change(x, n)",
        summary: "x minus its value n bars before, na when either is missing.",
        register: |engine, name| {
            streaming!(engine, name);
        },
    },
    Helper {
        name: "crossover",
        group: Group::Streaming,
        signature: "crossover(a, b)",
        summary: "Whether a rises above b on this bar after being below it, bars where they are \
                  equal passed over; a missing value starts over.",
        register: |engine, name| {
            crossing!(engine, name);
        },
    },
    Helper {
        name: "crossunder",
        group: Group::Streaming,
        signature: "crossunder(a, b)",
        summary: "Whether a falls below b on this bar after being above it, bars where they are \
                  equal passed over; a missing value starts over.",
        register: |engine, name| {
            crossing!(engine, name);
        },
    },
    Helper {
        name: "year",
        group: Group::Time,
        signature: "year(t)",
        summary: "The year of time t; a missing time is an error.",
        register: |engine, name| time(engine, name, |time| i64::from(time.year())),
    },
    Helper {
        name: "month",
        group: Group::Time,
        signature: "month(t)",
        summary: "The month of time t, 1 to 12; a missing time is an error.",
        register: |engine, name| time(engine, name, |time| i64::from(time.month())),
    },
    Helper {
        name: "day",
        group: Group::Time,
        signature: "day(t)",
        summary: "The day of the month of time t, 1 to 31; a missing time is an error.",
        register: |engine, name| time(engine, name, |time| i64::from(time.day())),
    },
    Helper {
        name: "weekday",
        group: Group::Time,
        signature: "weekday(t)",
        summary: "The day of the week of time t, Monday 1 to Sunday 7; a missing time is an error.",
        register: |engine, name| {
            time(engine, name, |time| {
                i64::from(time.weekday().number_from_monday())
            })
        },
    },
    Helper {
        name: "hour",
        group: Group::Time,
        signature: "hour(t)",
        summary: "The hour of time t, 0 to 23; a missing time is an error.",
        register: |engine, name| time(engine, name, |time| i64::from(time.hour())),
    },
    Helper {
        name: "minute",
        group: Group::Time,
        signature: "minute(t)",
        summary: "The minute of time t, 0 to 59; a missing time is an error.",
        register: |engine, name| time(engine, name, |time| i64::from(time.minute())),
    },
    Helper {
        name: "entry",
        group: Group::Orders,
        signature: "entry(id, side)",
        summary: "An entry at market of the backtest's order size. side is \"long\" or \"short\".",
        register: |engine, name| {
            engine.register_fn(
                name,
                |context: NativeCallContext, id: ImmutableString, side: ImmutableString| {
                    entry(&context, &id, &side, Map::new())
                },
            );
        },
    },
    Helper {
        name: "entry",
        group: Group::Orders,
        signature: "entry(id, side, options)",
        summary: "An entry; options is a map of qty, the quantity, limit, a limit price, and stop, a \
                  stop price, each optional. With a limit and a stop it is a stop-limit order.",
        register: |engine, name| {
            engine.register_fn(
                name,
                |context: NativeCallContext,
                 id: ImmutableString,
                 side: ImmutableString,
                 options: Map| entry(&context, &id, &side, options),
            );
        },
    },
    Helper {
        name: "exit_order",
        group: Group::Orders,
        signature: "exit_order(id)",
        summary: "An exit of the whole position at market.",
        register: |engine, name| {
            engine.register_fn(name, |context: NativeCallContext, id: ImmutableString| {
                exit(&context, &id, Map::new())
            });
        },
    },
    Helper {
        name: "exit_order",
        group: Group::Orders,
        signature: "exit_order(id, options)",
        summary: "An exit; options is a map of qty, the quantity to close, profit, a take-profit \
                  price, and loss, a stop-loss price, each optional.",
        register: |engine, name| {
            engine.register_fn(
                name,
                |context: NativeCallContext, id: ImmutableString, options: Map| {
                    exit(&context, &id, options)
                },
            );
        },
    },
    Helper {
        name: "exit_relative",
        group: Group::Orders,
        signature: "exit_relative(id, side, profit, loss)",
        summary: "Closes the position on side at profit and loss distances from its average entry \
                  price.",
        register: |engine, name| {
            engine.register_fn(
                name,
                |context: NativeCallContext,
                 id: ImmutableString,
                 side: ImmutableString,
                 profit: Dynamic,
                 loss: Dynamic|
                 -> Result<()> {
                    let side = order_side(&context, &side)?;
                    let profit = number(&context, "profit", &profit)?;
                    let loss = number(&context, "loss", &loss)?;
                    let id = id.to_string();
                    order(
                        &context,
                        OrderCall::Relative {
                            id,
                            side,
                            profit,
                            loss,
                        },
                    );
                    Ok(())
                },
            );
        },
    },
    Helper {
        name: "cancel",
        group: Group::Orders,
        signature: "cancel(id)",
        summary: "Cancels the working order id.",
        register: |engine, name| {
            engine.register_fn(name, |context: NativeCallContext, id: ImmutableString| {
                let id = id.to_string();
                order(&context, OrderCall::Cancel { id });
            });
        },
    },
    Helper {
        name: "position",
        group: Group::Orders,
        signature: "position()",
        summary: "The open position: positive long, negative short, 0 flat; 0 in a study.",
        register: |engine, name| {
            engine.register_fn(name, || reading(|readings| readings.position, 0.0));
        },
    },
    Helper {
        name: "equity",
        group: Group::Orders,
        signature: "equity()",
        summary: "The account's equity; na in a study.",
        register: |engine, name| {
            engine.register_fn(name, || reading(|readings| readings.equity, f64::NAN));
        },
    },
    Helper {
        name: "price",
        group: Group::Orders,
        signature: "price()",
        summary: "The close on a bar decision, the fill price right after a fill; na in a study.",
        register: |engine, name| {
            engine.register_fn(name, || reading(|readings| readings.price, f64::NAN));
        },
    },
    Helper {
        name: "after_fill",
        group: Group::Orders,
        signature: "after_fill()",
        summary: "Whether this decision follows a fill; false in a study.",
        register: |engine, name| {
            engine.register_fn(name, || reading(|readings| readings.after_fill, false));
        },
    },
    Helper {
        name: "alert",
        group: Group::Alerts,
        signature: "alert(message, condition)",
        summary: "An alert condition named message, a string written in the call, which holds on a \
                  bar when the last call with that message on the bar gives true; condition is \
                  true or false. The alert dialog offers it as the study's condition. A script \
                  declares at most 32 messages.",
        register: |engine, name| {
            engine.register_fn(
                name,
                |context: NativeCallContext,
                 message: ImmutableString,
                 condition: Dynamic|
                 -> Result<()> {
                    let at = runtime::with(|run| {
                        (run.spec.alerts.iter()).position(|declared| **declared == *message)
                    })
                    .flatten();
                    let Some(at) = at else {
                        return Err(refused(
                            &context,
                            format!(
                                "alert {message:?} is not declared; write its message as a \
                                 string in the call"
                            ),
                        ));
                    };
                    let Ok(condition) = condition.as_bool() else {
                        let kind = syntax::type_name(&condition);
                        return Err(refused(
                            &context,
                            format!("alert {message:?} takes true or false; got {kind}"),
                        ));
                    };
                    let span = span(context.call_position());
                    runtime::with(|run| {
                        let column = run.spec.plots.len() + at;
                        run.row[column] = if condition { 1.0 } else { 0.0 };
                        run.mark(span);
                    });
                    Ok(())
                },
            );
        },
    },
];

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
    // Rhai compares and writes arrays and maps through the engine at every level of nesting, a
    // few kilobytes of stack each, so a value nested a few hundred deep overflows the stack.
    // These come first in every lookup and reach the engine only for the other items.
    engine.register_fn("==", |context: NativeCallContext, a: Array, b: Array| {
        equal(&context, a.into(), b.into())
    });
    engine.register_fn("!=", |context: NativeCallContext, a: Array, b: Array| {
        equal(&context, a.into(), b.into()).map(|equal| !equal)
    });
    engine.register_fn("==", |context: NativeCallContext, a: Map, b: Map| {
        equal(&context, a.into(), b.into())
    });
    engine.register_fn("!=", |context: NativeCallContext, a: Map, b: Map| {
        equal(&context, a.into(), b.into()).map(|equal| !equal)
    });
    for name in ["print", "to_string", "debug", "to_debug"] {
        engine.register_fn(name, |context: NativeCallContext, array: Array| {
            written(&context, array.into())
        });
        engine.register_fn(name, |context: NativeCallContext, map: Map| {
            written(&context, map.into())
        });
    }

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

    engine.register_indexer_get(
        |context: NativeCallContext, series: &mut Handle, offset: INT| series.at(&context, offset),
    );
    engine.register_indexer_get(
        |context: NativeCallContext, series: &mut Handle, offset: FLOAT| {
            series.at(&context, whole("history", offset)?)
        },
    );
    engine.register_indexer_get(
        |context: NativeCallContext, series: &mut Handle, offset: Handle| {
            series.at(&context, whole("history", offset.value())?)
        },
    );
    engine
        .register_indexer_get(|_: &mut FLOAT, _: INT| -> Result<FLOAT> { Err(NO_HISTORY.into()) });
    engine.register_indexer_get(|_: &mut FLOAT, _: Handle| -> Result<FLOAT> {
        Err(NO_HISTORY.into())
    });
    engine.register_indexer_get(|_: &mut bool, _: INT| -> Result<FLOAT> { Err(NO_HISTORY.into()) });

    engine.register_type_with_name::<BarsHandle>("bars");
    // The fields of the chart's bars; `bar_index` is none.
    let fields: Vec<_> = PRICES
        .into_iter()
        .filter_map(|(name, series, _)| match series {
            SeriesRef::Price(price) => Some((name, Field::Price(price))),
            SeriesRef::Volume => Some((name, Field::Volume)),
            SeriesRef::Time => Some((name, Field::Time)),
            _ => None,
        })
        .collect();
    for &(name, field) in &fields {
        engine.register_get(
            name,
            move |context: NativeCallContext, bars: &mut BarsHandle| {
                runtime::with(|run| run.mark(span(context.call_position())));
                bars.field(field)
            },
        );
    }
    // Rhai asks this for a field that has no getter.
    engine.register_indexer_get(move |bars: &mut BarsHandle, name: ImmutableString| {
        let field = fields.iter().find(|(field, _)| *field == name);
        field.map(|(_, field)| bars.field(*field)).ok_or_else(|| {
            let names: Vec<_> = fields.iter().map(|(name, _)| *name).collect();
            let names = names.join(", ");
            Box::<EvalAltResult>::from(format!(
                "bars have no field {name}; expected one of {names}"
            ))
        })
    });

    for helper in HELPERS {
        (helper.register)(&mut engine, helper.name);
    }
    engine
}

/// Whether `a` equals `b` as Rhai's `==` says: arrays and maps of equal items in the same order,
/// other values by the engine's `==`. Items of different types without an `==` are unequal in an
/// array and an error in a map. The levels are kept on a list, so the stack stays flat however
/// deep the values nest.
fn equal(context: &NativeCallContext, a: Dynamic, b: Dynamic) -> Result<bool> {
    enum Level {
        Array(std::vec::IntoIter<Dynamic>, std::vec::IntoIter<Dynamic>),
        Map(<Map as IntoIterator>::IntoIter, Map),
    }
    let mut levels = Vec::new();
    let mut next = Some((a, b, true));
    while let Some((mut a, mut b, fallback)) = next {
        if a.is_array() && b.is_array() {
            let (a, b) = (a.cast::<Array>(), b.cast::<Array>());
            if a.len() != b.len() {
                return Ok(false);
            }
            levels.push(Level::Array(a.into_iter(), b.into_iter()));
        } else if a.is_map() && b.is_map() {
            let (a, b) = (a.cast::<Map>(), b.cast::<Map>());
            if a.len() != b.len() {
                return Ok(false);
            }
            levels.push(Level::Map(a.into_iter(), b));
        } else {
            match context.call_native_fn_raw("==", true, &mut [&mut a, &mut b]) {
                Ok(equal) if equal.as_bool().unwrap_or(false) => {}
                Ok(_) => return Ok(false),
                Err(error)
                    if fallback
                        && a.type_id() != b.type_id()
                        && matches!(&*error, EvalAltResult::ErrorFunctionNotFound(signature, _)
                            if signature.starts_with("==")) =>
                {
                    return Ok(false);
                }
                Err(error) => return Err(error),
            }
        }
        // The next pair of items, from the innermost level with items left.
        next = loop {
            match levels.last_mut() {
                None => break None,
                Some(Level::Array(a, b)) => {
                    if let Some((a, b)) = a.next().zip(b.next()) {
                        break Some((a, b, true));
                    }
                }
                Some(Level::Map(a, b)) => {
                    if let Some((key, a)) = a.next() {
                        let Some(b) = b.remove(&key) else {
                            return Ok(false);
                        };
                        break Some((a, b, false));
                    }
                }
            }
            levels.pop();
        };
    }
    Ok(true)
}

/// What `print`, `to_string`, `debug` and `to_debug` give for an array or a map, written as Rhai
/// writes it, the items of other types by the engine's `to_debug`. The levels are kept on a list,
/// so the stack stays flat however deep the value nests.
fn written(context: &NativeCallContext, value: Dynamic) -> ImmutableString {
    enum Level {
        Array(std::vec::IntoIter<Dynamic>, &'static str),
        Map(<Map as IntoIterator>::IntoIter, &'static str),
    }
    let engine = context.engine();
    let mut text = String::new();
    let mut levels = Vec::new();
    let mut next = Some(value);
    while let Some(mut value) = next {
        if value.is_array() {
            text.push('[');
            levels.push(Level::Array(value.cast::<Array>().into_iter(), ""));
        } else if value.is_map() {
            text.push_str("#{");
            levels.push(Level::Map(value.cast::<Map>().into_iter(), ""));
        } else {
            match context.call_native_fn_raw("to_debug", true, &mut [&mut value]) {
                Ok(written) if written.is_string() => {
                    text.push_str(&written.into_immutable_string().unwrap_or_default());
                }
                Ok(other) => text.push_str(engine.map_type_name(other.type_name())),
                Err(_) => text.push_str(engine.map_type_name(&format!("{value:?}"))),
            }
        }
        // The next item, from the innermost level with items left, after its separator.
        next = loop {
            match levels.last_mut() {
                None => break None,
                Some(Level::Array(items, separator)) => {
                    if let Some(item) = items.next() {
                        text.push_str(std::mem::replace(separator, ", "));
                        break Some(item);
                    }
                    text.push(']');
                }
                Some(Level::Map(items, separator)) => {
                    if let Some((key, item)) = items.next() {
                        let _ = write!(text, "{}{key:?}: ", std::mem::replace(separator, ", "));
                        break Some(item);
                    }
                    text.push('}');
                }
            }
            levels.pop();
        };
    }
    text.into()
}

/// What a value without history says when a script reads an earlier bar of it.
pub(crate) const NO_HISTORY: &str =
    "this value has no history; declare it with `series NAME = ...` to read earlier bars";

/// Feed the streaming helper at the calling position.
fn step(context: &NativeCallContext, name: &str, length: INT, x: f64, b: f64) -> Result<FLOAT> {
    let span = span(context.call_position());
    runtime::with(|run| {
        let occurrence = run.occurrence(span);
        let value = run.step((span, occurrence), name, length, x, b);
        run.mark(span);
        value
    })
    .unwrap_or(Ok(f64::NAN))
    .map_err(|message| EvalAltResult::ErrorRuntime(message.into(), context.call_position()).into())
}

/// A length or offset given as a number, which must be a whole one.
fn whole(name: &str, value: f64) -> Result<INT> {
    bindings::whole(value).ok_or_else(|| format!("{name} {value} is not a whole number").into())
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

/// An error of the call at the calling position.
fn refused(context: &NativeCallContext, message: String) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(message.into(), context.call_position()).into()
}

/// Place an entry on the strategy context.
fn entry(context: &NativeCallContext, id: &str, side: &str, options: Map) -> Result<()> {
    let side = order_side(context, side)?;
    let [qty, limit, stop] = order_options(context, "entry", ["qty", "limit", "stop"], options)?;
    let id = id.to_owned();
    order(
        context,
        OrderCall::Entry {
            id,
            side,
            qty,
            limit,
            stop,
        },
    );
    Ok(())
}

/// Place an exit on the strategy context.
fn exit(context: &NativeCallContext, id: &str, options: Map) -> Result<()> {
    let keys = ["qty", "profit", "loss"];
    let [qty, profit, loss] = order_options(context, "exit_order", keys, options)?;
    let id = id.to_owned();
    order(
        context,
        OrderCall::Exit {
            id,
            qty,
            profit,
            loss,
        },
    );
    Ok(())
}

/// The side an order names, `"long"` or `"short"`.
fn order_side(context: &NativeCallContext, side: &str) -> Result<OrderSide> {
    match side {
        "long" => Ok(OrderSide::Buy),
        "short" => Ok(OrderSide::Sell),
        other => Err(refused(
            context,
            format!("side takes \"long\" or \"short\"; got {other:?}"),
        )),
    }
}

/// The values of the options of `call`, in the order of `keys`: `None` for one not given, and an
/// error for a key outside `keys` or a value that is not a number.
fn order_options<const N: usize>(
    context: &NativeCallContext,
    call: &str,
    keys: [&str; N],
    options: Map,
) -> Result<[Option<f64>; N]> {
    let mut values = [None; N];
    for (key, value) in &options {
        let Some(at) = keys.iter().position(|known| *known == key.as_str()) else {
            let (last, first) = keys.split_last().expect("a call has options");
            let known = format!("{} and {last}", first.join(", "));
            return Err(refused(
                context,
                format!("{call} takes the options {known}; got {key}"),
            ));
        };
        values[at] = Some(number(context, key, value)?);
    }
    Ok(values)
}

/// The value a number, an int or a series stands for on the current bar, or an error naming
/// `name` for any other value.
fn number(context: &NativeCallContext, name: &str, value: &Dynamic) -> Result<f64> {
    if let Ok(value) = value.as_float() {
        Ok(value)
    } else if let Ok(value) = value.as_int() {
        Ok(value.num())
    } else if let Some(series) = value.read_lock::<Handle>() {
        Ok(series.value())
    } else {
        let kind = match context.engine().map_type_name(value.type_name()) {
            "f64" => "number",
            "i64" => "int",
            kind => kind,
        };
        Err(refused(
            context,
            format!("{name} takes a number; got {kind}"),
        ))
    }
}

/// Count the order call at the calling position, and place it while a strategy decides; a study
/// ignores it.
fn order(context: &NativeCallContext, call: OrderCall) {
    let span = span(context.call_position());
    runtime::with(|run| {
        run.mark(span);
        if let Some(decision) = &mut run.decision {
            decision.calls.push(call);
        }
    });
}

/// A reading of the decision being run, or `otherwise` in a study.
fn reading<T>(read: fn(&Readings) -> T, otherwise: T) -> T {
    runtime::with(|run| {
        run.decision
            .as_ref()
            .map(|decision| read(&decision.readings))
    })
    .flatten()
    .unwrap_or(otherwise)
}

/// The position Rhai reports as a span.
pub(crate) fn span(position: Position) -> Span {
    Span {
        line: position.line().unwrap_or(0) as u32,
        column: position.position().unwrap_or(0) as u32,
    }
}
