# egui-charts-scripting

Scripts that add studies to [egui-charts](https://github.com/userFRM/egui-charts). A script is a
short program that runs once per bar, oldest bar first, and writes the study's outputs for that
bar. It reads the bar's prices and their history, its own inputs and its own persistent state. It
cannot read files, the network, the clock, a random source or later bars, so it gives the same
outputs for the same bars on every run, and a live update gives the outputs of a full calculation
bit for bit.

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
registers its saved scripts at start-up restores charts with script studies unchanged.

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
and maps. Functions see only their parameters, so a function that needs a series takes it as one:
`fn slope(s, n) { (s - s[n]) / n }`. There are no anonymous functions or closures: an array
function such as `filter` takes a declared function as `Fn("name")`.

## Values

| Type | Written as | Missing value |
| --- | --- | --- |
| number | `1.5`, results of arithmetic on series | `na` |
| int | `14`, `length` and `int` inputs | none |
| bool | `true`, comparisons | none |
| string | `"text"`, `choice` and `text` inputs | none |
| series | `open high low close volume hl2 hlc3 ohlc4 time bar_index`, `series` declarations, `source` inputs | `na` on a bar |

A series has one value per bar. In arithmetic, a comparison or a helper argument it stands for
its value on the current bar; `s[n]` is its value `n` bars ago, and `s[0]` the current one. An
offset before the first bar gives `na`; a negative offset is an error, since a script never reads
a later bar. Every loaded bar can be read: there is no lookback limit. `time` is the bar's open
time in milliseconds since 1970-01-01 UTC, and `bar_index` the bar's number from 0. Price series,
inputs and declared series are constants; a script cannot assign to them.

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
  `"triangle_up"` and `"triangle_down"`. Titles are unique and not empty. A script has at least
  one plot and at most 32; a constant line is a plot of a literal.
- `fill(A, B, color = "#...", inverted = "#...");` fills the band between two plots, named by
  title, or numbers. The plots come before the fill.

A script declares at most 32 inputs, 64 states and 64 series, and a choice input lists at most 64
choices.

## Helpers

Every helper takes numbers, ints and series where a number is expected.

| Group | Helpers |
| --- | --- |
| Missing values | `na`, `is_na(x)`, `nz(x)`, `nz(x, y)` |
| Math | `abs`, `sign`, `sqrt`, `exp`, `ln`, `log10`, `pow(x, y)`, `min(a, b)`, `max(a, b)`, `clamp(x, lo, hi)`, `round(x)`, `round(x, digits)`, `floor`, `ceil` |
| Streaming | `sma(x, n)`, `ema(x, n)`, `rma(x, n)`, `wma(x, n)`, `sum(x, n)`, `stdev(x, n)`, `highest(x, n)`, `lowest(x, n)`, `change(x)`, `change(x, n)`, `crossover(a, b)`, `crossunder(a, b)` |
| Time, in UTC | `year(t)`, `month(t)`, `day(t)`, `weekday(t)` (Monday 1 to Sunday 7), `hour(t)`, `minute(t)` |

The math helpers return numbers, `na` for a missing argument; `min` and `max` give `na` when
either argument is missing, and `sign(0)` is 0. `round(x, d)` rounds half away from zero at `d`
decimals. The time helpers return ints and refuse a missing time.

A streaming helper keeps its own history, which belongs to its call: two calls of `sma` keep
two windows, and a call that does not run on a bar sees `na` there. Its length must stay the same
from bar to bar at one call; a length below 1 counts as 1, except for `change`, and a negative
length is an error. On every bar it gives exactly what the function of the same name in
`egui_charts::studies::helpers` gives for the whole series up to that bar: `sma`, `sum`, `wma` and
`stdev` over complete windows of finite values, `ema` seeded by the first finite value, `rma` by
the mean of the first `n`, `highest` and `lowest` ignoring missing values, and the crossings as
bools.

## Errors

`Script::compile` refuses a script with its first error at a one-based line and character
column: a syntax error, a wrong declaration, or a name the script does not declare. A runtime
error, such as a type mismatch or a negative history offset, stops the bar it happens on: the
bars before keep their values, the later ones are missing, and `Script::last_error` reports the
error with its line, column and bar. A host checks a script on the chart's bars before it replaces
a study, with `Script::check`, or with the new study itself, which it calculates off the chart
until its calculation is done.

## Limits

A script study never calculates on the UI thread: natively each study has a worker thread, and on
the web, which has no threads, each call runs whole bars for 4 ms. The chart keeps drawing the
previous values meanwhile, and the legend shows the progress, then the last compute time, with a
slow notice from one second on. A live tick recalculates only the newest bar.

`Limits`, set with `Script::with_limits`, bound each study's runs: by default 500 ms for one bar,
60 s for a full recompute and 256 MiB for the study's buffers and its script's values. Values count
with the room they hold, not their length: those the states keep, after each bar, and inside a bar,
every 256 operations at the next variable read, the variables in scope there, with what the last
count found in each call waiting on the running one. A run that passes a limit keeps none of its
bars: the study keeps its previous values, and the legend and `Script::last_error` say which limit
stopped it and where. It runs again when its inputs change or its bars change from the first one on.
Whether a run close to a limit finishes depends on the machine and its load. A script with long
logic on every bar costs time on every bar: on 100,000 bars a restated built-in study takes several
hundred milliseconds to recompute, off the UI thread.

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
