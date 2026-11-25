# egui-charts-scripting

Scripts that add studies to [egui-charts](https://github.com/userFRM/egui-charts). A script is a
short program that runs once per bar, oldest bar first, and writes the study's outputs for that
bar. It reads the bar's prices and their history, the studies of the registry, other histories the
chart supplies, its own inputs and its own persistent state. It cannot read files, the network, the
clock, a random source or later bars, so it gives the same outputs for the same bars on every run,
and a live update gives the outputs of a full calculation bit for bit.

The language is [Rhai](https://rhai.rs) with a fixed set of declarations, series and helpers. A
script compiles into a `CustomIndicator`, so the chart's settings dialog shows its inputs and the
style tab its plots, and a host registers it in the study registry like any other study:

```rust
use egui_charts::model::Bar;
use egui_charts::studies::{Calculation, Indicator, IndicatorFactory};
use egui_charts_scripting::Script;

let source = r#"
    input length = length("Length", 3, 1, 100);
    plot("Average", sma(close, length));
"#;
let mut factory = IndicatorFactory::new();
let script = Script::compile("Average close", source, &factory)?;

let bars: Vec<Bar> = [1.0, 2.0, 3.0, 4.0]
    .into_iter()
    .enumerate()
    .map(|(i, close)| {
        let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
        Bar::new(time, close, close, close, close, 1.0)
    })
    .collect();
// A host refuses a script whose calculation fails before it shows it.
script.check(&bars)?;

let mut study = script.study();
study.calculate(&bars);
// The study calculates away from the UI thread; a chart asks again on every frame.
while matches!(study.calculation(), Some(Calculation::Running(_))) {
    study.update(&bars, bars.len());
}
assert_eq!(study.values().value(0, 2), Some(2.0));
assert_eq!(study.values().value(0, 3), Some(3.0));

let registered = script.clone();
assert!(factory.register_in(IndicatorFactory::PERSONAL, "Average close", move || {
    Box::new(registered.study())
}));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Layouts and study templates store the registered name and the input values, so a host that
registers its saved scripts at start-up restores charts with script studies unchanged. A script
that places orders is a strategy for the library's backtester; see [Strategies](#strategies).

`egui_charts_scripting::examples()` returns the scripts written for this crate: a relative
strength index restated from Wilder's averages, a moving average cross, a price channel breakout
and a volume spike.

## A script

```text
// Relative strength from Wilder's average gain and loss.
display(overlay = false, format = 2);

input length = length("Length", 14, 1, 500);

state previous = na;    // the last finite close
state count = 0;        // changes seen
state up = 0.0;         // average gain
state down = 0.0;       // average loss

let value = na;
if !is_na(close) {
    if !is_na(previous) {
        let change = close - previous;
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };
        count += 1;
        if count <= length {
            up += gain;
            down += loss;
            if count == length {
                up /= length;
                down /= length;
            }
        } else {
            up = (up * (length - 1) + gain) / length;
            down = (down * (length - 1) + loss) / length;
        }
        if count >= length {
            value = if up + down > 0.0 { 100.0 * up / (up + down) } else { 50.0 };
        }
    }
    previous = close;
}
plot("RSI", value, color = "#7E57C2");
plot("Upper", 70.0);
plot("Lower", 30.0);
fill("Upper", "Lower", color = "#7E57C220");
```

The whole text is the program of one bar. Rhai's expressions and control flow work: `let`, `if`
and `else` as statements and expressions, `while`, `loop`, `for i in 0..n`, `break`,
`continue`, `switch`, functions with `fn`, integer and floating-point arithmetic, strings, arrays
and maps. Functions see only their parameters, so a function that needs a series or `bars` takes
it as one: `fn slope(s, n) { (s - s[n]) / n }`. There are no anonymous functions or closures: an
array function such as `filter` takes a declared function as `Fn("name")`.

## Values

| Type | Written as | Missing value |
| --- | --- | --- |
| number | `1.5`, results of arithmetic on series | `na` |
| int | `14`, `length` and `int` inputs | none |
| bool | `true`, comparisons | none |
| string | `"text"`, `choice`, `text`, `symbol` and `interval` inputs | none |
| series | `open high low close volume hl2 hlc3 ohlc4 time bar_index`, `series` declarations, `source` inputs, `study(...)` calls, the fields of `bars` such as `NAME.close` | `na` on a bar |
| bars | `bars` declarations | none |

A series has one value per bar. In arithmetic, a comparison or a helper argument it stands for
its value on the current bar; `s[n]` is its value `n` bars ago, and `s[0]` the current one. An
offset before the first bar gives `na`; a negative offset is an error, since a script never reads
a later bar. Every loaded bar can be read: there is no lookback limit. `time` is the bar's open
time in milliseconds since 1970-01-01 UTC, and `bar_index` the bar's number from 0. Price series,
inputs, declared series and `bars` are constants; a script cannot assign to them.

Any other value has no history: after `let d = close - open;`, reading `d[1]` is an error that
asks to declare `series d = close - open;` instead.

`na` is a number. Arithmetic with `na` gives `na`, and every comparison with it is false except
`!=`, so a script tests with `is_na(x)`. `nz(x)` is `x`, or 0 when it is missing, and `nz(x, y)`
is `x` or `y`. A plot of `na` or of an infinite number leaves the bar empty. Dividing an int by
zero is an error; dividing a number by zero gives an infinite value, which plots as missing.

## Declarations

The arguments of a declaration are literals: numbers with an optional `-`, strings, `true`,
`false`, `na`, and a price name where a source is expected. Only the value of `series` and
`plot` is an expression. A declaration may stand anywhere a statement can, but it does not
depend on control flow: an `input` inside an `if` is still an input. Every declaration is read
and checked when the script compiles, before any bar runs.

- `display(overlay = BOOL, format = FORMAT, scale = SCALE);`, at most once, each option
  optional. `overlay = true` draws the study over the prices, the default a pane of its own.
  `format` is `"price"`, `"volume"`, `"percent"` or a number of decimals from 0 to 12, and
  `scale` is `"main"`, `"independent"` or `"none"`; both default to the study placement's own.
- `input NAME = KIND(LABEL, DEFAULT, ...);` declares an input with id `NAME`, which the settings
  dialog shows with `LABEL`. `NAME` holds its current value.

  | Kind | Value in the script |
  | --- | --- |
  | `length(label, default, min, max)` | int |
  | `int(label, default, min, max)` | int |
  | `float(label, default, min, max)` | number |
  | `bool(label, default)` | bool |
  | `choice(label, default, ["a", "b", ...])` | string, the chosen one |
  | `source(label, PRICE)`, `PRICE` one of `open high low close hl2 hlc3 ohlc4` | series |
  | `text(label, default)` | string |
  | `symbol(label, default)`, `""` for the chart's symbol | string |
  | `interval(label, default)`, `""` for the chart's interval | string, such as `"5min"`, `"1h"` or `"1D"` |

  The default must lie within the bounds and the label must not be empty. The ids
  `plot_offset`, `calculation_symbol`, `calculation_timeframe` and `wait_for_close` belong to
  inputs the chart adds itself. Only the input with id `source` can read another study's output
  when the chart offers it; other `source` inputs read price fields only.
- `state NAME = LITERAL;` declares a variable that keeps its value from one bar to the next: the
  literal before the first bar, and afterwards whatever the script assigned on the bar before. A
  state that holds a series at the end of a bar keeps that series' value instead, so
  `previous = close;` carries the close to the next bar.
- `series NAME = EXPR;` declares a series the script computes. Each time the statement runs it
  stores a number as `NAME`'s value on the current bar; a bar on which it does not run holds
  `na`.
- `plot(TITLE, EXPR, color = "#RRGGBB", style = STYLE, offset = INT);` declares an output, numbered
  from 0 in source order, and stores its value on the current bar; the last store on a bar wins.
  `color` also takes `"#RRGGBBAA"`. `STYLE` is one of `"line"`, `"line_with_breaks"`,
  `"histogram"`, `"circles"`, `"step_line"`, `"step_line_with_breaks"`,
  `"step_line_with_diamonds"`, `"columns"`, `"crosses"`, `"area"`, `"area_with_breaks"`,
  `"triangle_up"` and `"triangle_down"`. Titles are unique and not empty, and a constant line is a
  plot of a literal.
- `fill(A, B, color = "#...", inverted = "#...");` fills the band between two plots, named by
  title, or numbers. The plots come before the fill.

- `bars NAME = history(SYMBOL, INTERVAL);` declares another history, see
  [Other histories](#other-histories).

The [reference](#reference-and-completion) states how many of each a script may declare. The words
`display`, `input`, `state`, `series`, `bars`, `plot`, `fill` and `study` are reserved: a script
cannot use a variable or function of that name.

## Study calls

`study(NAME, ID = EXPR, ..., output = TITLE)` is an expression, the line of a study of the registry
as a series:

```rust
use egui_charts::studies::IndicatorFactory;
use egui_charts_scripting::Script;

let source = r#"
    let strength = study("RSI", period = 14);
    let lower = study("BB", std_dev = 2.5, output = "BB Lower");
    let smooth = study("RSI", source = study("EMA", period = 10));
    plot("Below the band", if close < lower { strength } else { na });
    plot("Rising", if smooth > smooth[1] { smooth } else { na });
"#;
Script::compile("Band strength", source, &IndicatorFactory::new())?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`NAME` is a name `IndicatorFactory::list` gives. `ID` is the id of an input `Indicator::inputs`
gives for that study or the settings dialog adds, such as `source`; it is written as a string when
it is not a name or is a reserved word, as in `"periods [0]" = 5`. An input the call does not give
has the default the study declares, whatever defaults the host set, so a script computes the same
on every host.

| Input | Value |
| --- | --- |
| length, integer | int, or a number without a fraction |
| number | number or int |
| checkbox | bool |
| choice | string, the label chosen |
| text, symbol | string |
| interval | string, as for `bars` |
| source | price series |

A series given to an input of another kind stands for its value on the current bar. The input
`source` of a study also takes the script's own input `source` and another `study` call, whose whole
column it reads. It takes no declared series, which the script computes bar by bar: the helpers
read those. Colours, times of day, time zones and the intervals a study picks itself take no value.

`output` is the title of a line as the chart shows it, such as `"BB Lower(20, 2.5)"`, or that title
without its last part in parentheses, `"BB Lower"`, where no other line has the same. Where several
lines share it, as the averages of a ribbon do, it is the title with the inputs given or the title
the same line has under the defaults. Without `output` the call reads the first line.

A study is computed once for each call in the source and each set of inputs given there, natively
and over every bar, so `study(...)[1]` is the study's value on the bar before, whether the call ran
on that bar or not. A script computes at most 64, so an input that changes on every bar fails on the
65th bar. Some studies cannot be called: those that calculate on their own, such as another script,
those without a line, those that need order flow and those that need other histories, as any study
does with the inputs `calculation_symbol` or `calculation_timeframe`. A called study sees the bars
alone: no symbol, instrument, session, interval or events of the chart.

A few studies change the values of earlier bars once later ones arrive, as a zigzag does when it
moves its last point. A script that calls one reads on every bar the values of the latest
calculation, which later bars shaped, and recalculates in full whenever they change.

## Other histories

`bars NAME = history(SYMBOL, INTERVAL);` declares the bars of another symbol or interval, which the
study requests from the chart:

```rust
use egui_charts::studies::IndicatorFactory;
use egui_charts_scripting::Script;

let source = r#"
    input other = symbol("Symbol", "SPY");
    bars day = history("", "1D");
    bars index = history(other, "");
    let strength = study("RSI", period = 14);
    plot("Trend", if close > day.close { strength } else { na });
    plot("Ratio", close / index.close);
"#;
Script::compile("Trend", source, &IndicatorFactory::new())?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`SYMBOL` is a string, `""` for the chart's symbol, or the name of a `symbol` input declared above,
and `INTERVAL` a string, `""` for the chart's interval, or the name of an `interval` input declared
above. An interval is written `"30s"`, `"5min"`, `"1h"`, `"1D"`, `"1W"` or `"1M"`. A plain number
counts minutes, and `m` in either case months: `"5min"` and `"5"` are five minutes, `"5m"` is five
months.

`NAME.open`, `high`, `low`, `close`, `volume`, `hl2`, `hlc3`, `ohlc4` and `time` are series. On each
bar of the chart they read the latest bar of the history that has closed when that bar closes, so
`NAME.close[1]` is what `NAME.close` was on the chart bar before. The latest chart bar is still
forming and reads the newest bar of the history, but of a longer interval the latest that has closed
by that chart bar's open, unless the study's Wait for interval close input is off. A field is `na`
where there is no such bar, and on every bar while the chart has not supplied the history.

A study of a script with `bars` reads prices alone in its input `source`, since the chart supplies a
study either histories or another study's output. `Script::check` calculates without other
histories.

## Strategies

A script that places orders is a strategy: `Script::places_orders` says whether it calls `entry`,
`exit_order`, `exit_relative` or `cancel` without defining a function of that name, and such a
script needs no plot. `Script::strategy` gives a `ScriptStrategy`, an
`egui_charts::strategies::Strategy` with the script's inputs, which `Backtest::run`, the strategy
tester and live runs take like any other, and which a host registers in a `StrategyFactory` under
the name it saves the script by:

```rust
use egui_charts::model::{Bar, Timeframe};
use egui_charts::strategies::Backtest;
use egui_charts::studies::IndicatorFactory;
use egui_charts_scripting::Script;

let source = r#"
    // Long while the close rises, flat once it falls.
    if close > close[1] && position() == 0.0 {
        entry("Long", "long", #{ qty: 10 });
    } else if close < close[1] && position() > 0.0 {
        exit_order("Close");
    }
"#;
let script = Script::compile("Momentum", source, &IndicatorFactory::new())?;
assert!(script.places_orders());

let bars: Vec<Bar> = [100.0, 101.0, 103.0, 102.0, 101.0]
    .into_iter()
    .enumerate()
    .map(|(i, close)| {
        let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
        Bar::new(time, close, close, close, close, 0.0)
    })
    .collect();
let backtest = Backtest::new("Momentum", "SAMPLE", Timeframe::Min1, 10_000.0);
let report = backtest.run(&mut script.strategy(), &bars, &[]);
// Bought at the next open after the rise to 101, sold at the next open after the fall to 102.
assert_eq!(report.trades.len(), 1);
assert_eq!(report.trades[0].entry_price, 103.0);
assert_eq!(report.trades[0].exit.as_ref().map(|exit| exit.profit), Some(-20.0));
# Ok::<(), Box<dyn std::error::Error>>(())
```

The orders are functions, so their names stay free for a script's own variables and inputs:

- `entry(id, side)` and `entry(id, side, options)` place an entry, `side` being `"long"` or
  `"short"`. `options` is a map of `qty`, the quantity, `limit`, a limit price, and `stop`, a stop
  price, each optional: with neither price the entry is at market, with both a stop-limit order,
  and without `qty` it takes the backtest's order size.
- `exit_order(id)` and `exit_order(id, options)` place an exit, of the whole position at market by
  default; `options` is a map of `qty`, the quantity to close, `profit`, a take-profit price, and
  `loss`, a stop-loss price. Rhai's own `exit` ends the script.
- `exit_relative(id, side, profit, loss)` closes the position on `side` at the distances `profit`
  and `loss` from its average entry price.
- `cancel(id)` cancels the working order `id`.

An option key outside a call's list, a value that is not a number, an int or a series, and a side
other than `"long"` and `"short"` are runtime errors at the call. The backtester's rules apply to
what is placed: an order with the id of a working order replaces it, and one with a missing or
infinite price or quantity is ignored, so an entry at `#{ limit: sma(close, 20) }` waits out the
average's first bars. Orders placed before the backtest's start time are ignored.

`position()`, `equity()`, `price()` and `after_fill()` read the decision: the open position,
positive long, negative short and 0 flat, the account's equity, the close of the bar decided on or
the fill price right after a fill, and whether the decision follows a fill.

Each decision runs the script on the bar that just closed, with the bars before it as its history,
and places its order calls in the order it made them. With fill recalculation, the decision right
after a fill runs the whole script on that bar again from the state before it, and so does every
later decision on the bar; the last one sets the states the next bar starts from. A script that
skips its orders after a fill updates its states above `if !after_fill()`, not below it.

A runtime error or a limit ends the strategy's orders from its bar until the next run: its working
orders are cancelled, so none fills on a later bar or live update, and the run's report is
withheld with the error as the reason, as `StrategyReport::unavailable` says, so a report never
reads as complete when its strategy stopped; `Script::last_error` has it at its line, column and
bar. `Limits::bar` bounds each decision as it bounds a study's bar, `Limits::full` the time of a
run's decisions together, and `Limits::memory` the bars and values the script holds. Natively a
strategy runs its script on a thread of its own, on the web on the caller's. A decision blocks the
caller: the strategy asks a cooperative run, such as the strategy tester's, to yield after 4 ms of
decisions, so a slow script keeps the window responsive.

A strategy script cannot read other histories: the strategy tester refuses one that declares
`bars`, and `Backtest::run` gives such a script `na` for their fields. A study of a strategy script,
from `Script::study`, checks its orders and ignores them, and its readings are those of a flat
account: 0, `na`, `na` and false.

## Helpers

Every helper takes numbers, ints and series where a number is expected, and the math helpers give
`na` for a missing argument. The [reference](#reference-and-completion) lists each helper with what
it gives, in five groups: missing values, such as `is_na(x)` and `nz(x)`; math; streaming helpers,
such as `sma(x, n)` and `crossover(a, b)`; the time helpers, which read a time in UTC; and the
[orders](#strategies) of strategies with their readings.

A streaming helper keeps its own history, which belongs to its call: two calls of `sma` keep
two windows, and a call that does not run on a bar sees `na` there. Its length must stay the same
from bar to bar at one call; a length below 1 counts as 1, except for `change`, and a negative
length is an error. On every bar it gives exactly what the function of the same name in
`egui_charts::studies::helpers` gives for the whole series up to that bar.

## Reference and completion

`Reference` lists every name a script can use: the declarations with their limits, the input kinds,
the series, the helpers and the studies of a registry that a script can call, each study with its
inputs and outputs written as a call takes them. `to_markdown` gives it as one document to read in an
editor, and `to_json` the same entries for other programs. `complete` gives
the entries that complete the word before a byte of a script: names, the input kinds after
`input NAME =`, registry names inside `study("`, and inside a study call the ids of its inputs and,
after `output = "`, the titles of its outputs:

```rust
use egui_charts::studies::IndicatorFactory;
use egui_charts_scripting::Reference;

let reference = Reference::new(&IndicatorFactory::new());
let text = "let strength = study(\"RSI\", per";
let names: Vec<_> = reference
    .complete(text, text.len())
    .into_iter()
    .map(|entry| entry.name)
    .collect();
assert_eq!(names, ["period"]);
```

`cargo run --example scripting -- --reference`, from `crates/scripting`, prints the reference for
the built-in studies.

## Errors

`Script::compile` refuses a script with its first error at a one-based line and character
column: a syntax error, a wrong declaration, a name the script does not declare, a study, one of its
inputs or, in a `study` call that gives no input, an output that does not exist, or a call to a
function or method that neither the script, the helpers nor Rhai's standard package defines, also
in a branch no bar takes. A method may be a function a map holds, so a method name the script also
writes outside a call, as a key, another name or a whole string, passes. The arguments of a call,
the values of a study's inputs and the function `Fn("name")` names are checked when they run. A
runtime error, such as a type mismatch or a negative history offset, stops the bar it happens on:
the bars before keep their values, the later ones are missing, and `Script::last_error` reports the
error with its line, column and bar. A host checks a script on the chart's bars before it replaces
a study, with `Script::check`, or with the new study itself, which it calculates off the chart until
its calculation is done.

## Editors

`tokens` classifies the keywords, declarations, built-in names, numbers, strings and comments of a
text by byte range, for an editor to colour, with the scanner the compile check uses, so both agree
on what is a string or a comment. `ScriptError::span` turns an error's line and column into the byte
range it points at, the name or number there or a single character, for an editor to underline:

```rust
use egui_charts::studies::IndicatorFactory;
use egui_charts_scripting::{Script, TokenClass, tokens};

let text = "plot(\"x\", close); // c";
let classes: Vec<_> = tokens(text)
    .into_iter()
    .map(|(range, class)| (&text[range], class))
    .collect();
assert_eq!(
    classes,
    [
        ("plot", TokenClass::Declaration),
        ("\"x\"", TokenClass::Text),
        ("close", TokenClass::Builtin),
        ("// c", TokenClass::Comment),
    ]
);
let text = "plot(\"x\", clse);";
let error = Script::compile("Close", text, &IndicatorFactory::new()).unwrap_err();
assert_eq!(&text[error.span(text)], "clse");
```

`Script::with_profiling(true)` makes the studies of a script record, for each line, the runtime
operations on it and the time they took: the streaming and crossing helpers, history reads with
`x[n]`, the fields of other histories and the `series`, `plot` and `study` statements, each charged
the time since the one before, so arithmetic and the other helpers count towards the next of them.
`Script::profile` returns what the last full recalculation of any of its studies on bars recorded
when it ended without an error or a limit; an update by a new bar leaves it as it was, and so does a
study calculated on no bars.

## Limits

A script study never calculates on the UI thread: natively each study has a worker thread, and on
the web, which has no threads, each call runs whole bars for 4 ms. The chart keeps drawing the
previous values meanwhile, and the legend shows the progress, then the last compute time, with a
slow notice from one second on. A live tick recalculates only the newest bar. The studies a script
calls count against its time limits but are not interrupted: a bar may pass its limit, and a call on
the web its 4 ms, by the calculation of one study, which costs the same on the chart.

`Limits`, set with `Script::with_limits`, bound each study's runs: by default 500 ms for one bar,
60 s for a full recompute and 256 MiB for the study's buffers and its script's values. The buffers
include the lines of every study called, with a copy of the one read, and the bars of other
histories, but not what a called study keeps for itself. Values count with the room they hold, not
their length: those the states keep, after each bar, and inside a bar, every 256 operations at the
next variable read, the variables in scope there, with what the last count found in each call
waiting on the running one. A run that passes a limit keeps none of its bars: the study keeps its
previous values, and the legend and `Script::last_error` say which limit stopped it and where. It
runs again when its inputs change or its bars change from the first one on. Whether a run close to a
limit finishes depends on the machine and its load. A script with long logic on every bar costs time
on every bar: on 100,000 bars a restated built-in study takes several hundred milliseconds to
recompute, off the UI thread, and a script that calls it with `study` a tenth of that.

`Limits` may gain fields, so a host starts from the defaults and sets the limits it changes:

```rust
use egui_charts::studies::IndicatorFactory;
use egui_charts_scripting::{Limits, Script};
use std::time::Duration;

let mut limits = Limits::default();
limits.bar = Duration::from_millis(100);
let script = Script::compile("Close", "plot(\"Close\", close);", &IndicatorFactory::new())?
    .with_limits(limits);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The size of what a script builds is limited too: strings to 10,000 bytes, arrays to 10,000 items
and maps to 1,000 entries, counting those of the arrays and maps nested in them, 256 variables in
scope, 64 functions and calls 32 deep. A value is held to these sizes where it is built, assigned
with an operator or returned by a function, and at each count of the variables, which catches one
grown by index, as in `a[i] = b`. Natively scripts run on threads of 32 MiB, and the deepest value
these limits admit, built and compared in the deepest call, needs under 16 MiB of it in a debug
build. On the web they run on the page's thread, whose stack is 1 MiB in a wasm32 build with the
default linker settings, so there arrays hold up to 200 items, maps 50 entries and calls go 6
deep; the deepest value in the deepest call needs under 900 KiB there in a debug build, measured in
Chrome and Firefox.

The memory limit guards against runaway growth in ordinary scripts, such as a state that grows on
every bar or a buffer built in a loop, and is not a bound against a hostile script. Inside a bar it
does not count a value no variable holds, such as the arguments of a call while they are evaluated
or the results `map` collects, nor what a waiting call bound after its last count. So what one bar
allocates has no fixed ceiling: it grows with the shape of the script until the time limit stops the
bar. For example, in a release build under the default limits, 250 copies of a 10 MB map passed to
one call reached about 2.4 GiB before the time limit stopped the bar, after 981 ms. The clock is
read every 256 operations and one operation may copy a large value, so a bar may also run past its
time limit, by up to about a second as measured. An allocation that fails ends the process natively
and the WebAssembly module on the web. A host that runs scripts it does not trust caps the memory of
the process or worker they run in.

A script has no `import`, no `eval`, no clock and no output: it reaches nothing outside itself.
