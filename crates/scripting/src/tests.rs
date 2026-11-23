//! Scripts against independent implementations: the native studies and `studies::helpers`,
//! hand computed values, and the update contract of `Indicator`.

use crate::{Limits, Script, examples};
use egui_charts::model::{Bar, BarClock, PriceSource, Timeframe};
use egui_charts::studies::{
    Calculation, ConfiguredStudy, CustomIndicator, Indicator, IndicatorFactory, InputValue, MACD,
    RSI, Stochastic, StudyChart, StudyData, StudyDataRequest, StudySource, StudyTemplate, helpers,
};
use std::time::Duration;

// `wasm-pack test --chrome` skips a suite left on its default, Node.js.
#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

/// `n` one-minute bars from 2026-01-02 00:00 UTC of a xorshift random walk around 100.
fn bars(n: usize, seed: u64) -> Vec<Bar> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 2001) as f64 / 1000.0 - 1.0
    };
    let start = chrono::DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")
        .unwrap()
        .to_utc();
    let mut close = 100.0;
    (0..n)
        .map(|i| {
            let open = close;
            close += next();
            let high = open.max(close) + next().abs() * 0.5;
            let low = open.min(close) - next().abs() * 0.5;
            let volume = 1000.0 + next() * 400.0;
            let time = start + chrono::Duration::minutes(i as i64);
            Bar::new(time, open, high, low, close, volume)
        })
        .collect()
}

/// [`bars`] with missing closes at bars 100, 101 and 2000.
fn with_gaps(n: usize, seed: u64) -> Vec<Bar> {
    let mut bars = bars(n, seed);
    for at in [100, 101, 2000] {
        if let Some(bar) = bars.get_mut(at) {
            bar.close = f64::NAN;
        }
    }
    bars
}

/// Ask `study` again, as a host does once a frame, until its calculation no longer runs.
fn settle(study: &mut dyn Indicator, bars: &[Bar]) -> Option<Calculation> {
    while let Some(Calculation::Running(_)) = study.calculation() {
        std::thread::sleep(Duration::from_millis(1));
        study.update(bars, bars.len());
    }
    study.calculation()
}

fn compile(source: &str) -> Script {
    Script::compile("Test", source, &IndicatorFactory::new())
        .unwrap_or_else(|error| panic!("{source}: {error}"))
}

/// A settled study of `script` calculated on `bars`.
fn calculated(script: &Script, bars: &[Bar]) -> CustomIndicator {
    let mut study = script.study();
    study.calculate(bars);
    settle(&mut study, bars);
    study
}

/// Whether two columns are equal bit for bit, every missing value equal to every other.
fn same(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.to_bits() == b.to_bits() || a.is_nan() && b.is_nan())
}

/// What a plot stores of `values`: missing for non-finite ones.
fn plotted(values: Vec<f64>) -> Vec<f64> {
    values
        .into_iter()
        .map(|value| if value.is_finite() { value } else { f64::NAN })
        .collect()
}

/// Done when 2 of #3631: scripts that restate built-in studies match them bar for bar.
#[test]
fn restated_studies_match_native_bar_for_bar() {
    let rsi = |period: usize, bars: &[Bar]| {
        let mut native = RSI::new(period);
        native.calculate(bars);
        native.values().line(0).to_vec()
    };
    let relative_strength = examples()[0].1;
    for fixture in [bars(3000, 1), with_gaps(3000, 1)] {
        let closes: Vec<f64> = fixture.iter().map(|bar| bar.close).collect();
        let mut rows: Vec<(String, Option<usize>, Vec<f64>)> = vec![
            (relative_strength.to_owned(), None, rsi(14, &fixture)),
            // Fails if the optimizer folds an input's default into the program.
            (relative_strength.to_owned(), Some(5), rsi(5, &fixture)),
        ];
        let mut macd = MACD::new(12, 26, 9);
        macd.calculate(&fixture);
        for (line, plot) in ["m", "s", "m - s"].into_iter().enumerate() {
            rows.push((
                format!(
                    "let m = ema(close, 12) - ema(close, 26);\nlet s = ema(m, 9);\n\
                     plot(\"x\", {plot});"
                ),
                None,
                macd.values().line(line).to_vec(),
            ));
        }
        // The native study leaves bars with a missing price out of its windows.
        if closes.iter().all(|close| close.is_finite()) {
            let mut stochastic = Stochastic::new(14, 3, 3);
            stochastic.calculate(&fixture);
            for (line, plot) in ["k", "sma(k, 3)"].into_iter().enumerate() {
                rows.push((
                    format!(
                        "let top = highest(high, 14);\nlet bottom = lowest(low, 14);\n\
                         let range = top - bottom;\n\
                         let fast = if bar_index < 13 {{ na }} else if range == 0.0 {{ 50.0 }} \
                         else {{ clamp(100.0 * (close - bottom) / range, 0.0, 100.0) }};\n\
                         let k = sma(fast, 3);\nplot(\"x\", {plot});"
                    ),
                    None,
                    stochastic.values().line(line).to_vec(),
                ));
            }
        }
        // The last length is far beyond the bars: a window allocated by its length up front
        // aborts the process.
        for n in [1, 2, 14, 1e12 as usize] {
            for (name, expected) in [
                ("sma", helpers::sma(&closes, n)),
                ("ema", helpers::ema(&closes, n)),
                ("rma", helpers::rma(&closes, n)),
                ("wma", helpers::wma(&closes, n)),
                ("sum", helpers::sum(&closes, n)),
                ("stdev", helpers::stdev(&closes, n)),
                ("highest", helpers::highest(closes.iter().copied(), n)),
                ("lowest", helpers::lowest(closes.iter().copied(), n)),
                ("change", helpers::change(&closes, n)),
            ] {
                rows.push((format!("plot(\"x\", {name}(close, {n}));"), None, expected));
            }
        }
        let average = helpers::sma(&closes, 5);
        for (name, crossings) in [
            ("crossover", helpers::crossover(&closes, &average)),
            ("crossunder", helpers::crossunder(&closes, &average)),
        ] {
            rows.push((
                format!("plot(\"x\", if {name}(close, sma(close, 5)) {{ 1 }} else {{ 0 }});"),
                None,
                crossings
                    .into_iter()
                    .map(|c| f64::from(u8::from(c)))
                    .collect(),
            ));
        }
        for (source, length, expected) in rows {
            let mut study = compile(&source).study();
            if let Some(length) = length {
                study
                    .set_input("length", InputValue::Length(length))
                    .unwrap();
            }
            study.calculate(&fixture);
            settle(&mut study, &fixture);
            assert!(
                same(study.values().line(0), &plotted(expected)),
                "{source} with length {length:?}"
            );
        }
    }
}

/// A `study(...)` call is the study it names with the inputs it gives and otherwise those the
/// study declares, read at one line: bit for bit on every bar, whenever the call is first made.
#[test]
fn study_calls_equal_their_studies() {
    let fixture = bars(300, 1);
    let native = |name: &str, inputs: &[(&str, InputValue)], line: usize| {
        let mut study = ConfiguredStudy::new(IndicatorFactory::new().create(name).unwrap());
        for (id, value) in inputs {
            study.set_input(id, value.clone()).unwrap();
        }
        study.calculate(&fixture);
        study.values().line(line).to_vec()
    };
    let average = |name: &str| native(name, &[("period", InputValue::Length(10))], 0);
    let rsi = |period| native("RSI", &[("period", InputValue::Length(period))], 0);
    let minus = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(a, b)| a - b).collect();
    let chained = {
        let mut study = RSI::new(14);
        study.update_source(&fixture, 0, &average("EMA"));
        study.values().line(0).to_vec()
    };
    let mut host = IndicatorFactory::new();
    let mut defaults = StudyTemplate::from_study(host.create("RSI").unwrap().as_ref());
    defaults.inputs = vec![("period".into(), InputValue::Length(5))];
    host.set_defaults(&[defaults]).unwrap();
    for (source, expected) in [
        // Not the host's default of 5.
        (r#"plot("x", study("RSI"));"#, rsi(14)),
        (r#"plot("x", study("RSI", "period" = 4 + 1.0));"#, rsi(5)),
        (
            r#"plot("x", study("RSI", source = hl2));"#,
            native(
                "RSI",
                &[(
                    "source",
                    InputValue::Source(StudySource::Price(PriceSource::HL2)),
                )],
                0,
            ),
        ),
        (
            r#"plot("x", study("RSI", source = study("EMA", period = 10)));"#,
            chained.clone(),
        ),
        (
            r#"plot("x", study("MACD", fast_period = 5, oscillator_average = "Simple", output = "Signal"));"#,
            native(
                "MACD",
                &[
                    ("fast_period", InputValue::Length(5)),
                    ("oscillator_average", InputValue::Choice(1)),
                ],
                1,
            ),
        ),
        (
            r#"plot("x", study("BB", std_dev = 2.5, output = "BB Lower"));"#,
            native("BB", &[("std_dev", InputValue::Number(2.5))], 2),
        ),
        (
            r#"plot("x", study("BB", std_dev = 3, output = "BB Upper(20, 3)"));"#,
            native("BB", &[("std_dev", InputValue::Number(3.0))], 0),
        ),
        // The default title where the titles differ by their length alone.
        (
            r#"plot("x", study("GMMA", "periods [1]" = 6, output = "EMA(5)"));"#,
            native("GMMA", &[("periods [1]", InputValue::Length(6))], 1),
        ),
        // Two studies, and two sets of inputs at one call.
        (
            r#"plot("x", study("SMA", period = 10) - study("EMA", period = 10));"#,
            minus(&average("SMA"), &average("EMA")),
        ),
        (
            r#"fn r(n) { study("RSI", period = n) } plot("x", r(5) - r(14));"#,
            minus(&rsi(5), &rsi(14)),
        ),
        // A series stands for its value on the bar.
        (
            r#"series n = if bar_index < 150 { 5 } else { 14 }; plot("x", study("RSI", period = n));"#,
            [&rsi(5)[..150], &rsi(14)[150..]].concat(),
        ),
        // The bars before the first call have values too.
        (
            r#"plot("x", if bar_index < 200 { na } else { study("RSI")[150] });"#,
            [&[f64::NAN; 200][..], &rsi(14)[50..150]].concat(),
        ),
    ] {
        let script = Script::compile("Test", source, &host).unwrap();
        let study = calculated(&script, &fixture);
        assert!(
            same(study.values().line(0), &expected),
            "{source}: {:?}",
            script.last_error()
        );
    }
    // The output another study feeds the script.
    let script = compile(
        "input source = source(\"Source\", close);\nplot(\"x\", study(\"RSI\", source = source));",
    );
    let fed = average("EMA");
    let mut study = script.study();
    study.update_source(&fixture, 0, &fed);
    while let Some(Calculation::Running(_)) = study.calculation() {
        std::thread::sleep(Duration::from_millis(1));
        study.update_source(&fixture, fixture.len(), &fed);
    }
    assert!(
        same(study.values().line(0), &chained),
        "{:?}",
        script.last_error()
    );
}

/// A `bars` declaration requests its history and reads on each chart bar the latest of its bars
/// closed by then, on the latest chart bar the one still forming when the study does not wait for
/// it, after every change of the chart and of the history.
#[test]
fn other_histories_align_to_chart_bars() {
    let bar = |minutes: i64, close: f64| {
        let midnight = chrono::DateTime::from_timestamp_secs(1_767_312_000).unwrap();
        let time = midnight + chrono::Duration::minutes(minutes);
        Bar::new(time, close, close, close, close, 1.0)
    };
    fn request(timeframe: Timeframe) -> StudyDataRequest {
        StudyDataRequest {
            symbol: "SPY".into(),
            timeframe: Some(timeframe),
            metric: None,
        }
    }
    fn history(timeframe: Timeframe, bars: &[Bar], wait_for_close: bool) -> StudyData<'_> {
        StudyData {
            request: request(timeframe),
            bars,
            clock: BarClock::new(timeframe),
            instrument: None,
            timeframe,
            chart: Some(StudyChart {
                clock: BarClock::new(Timeframe::Min30),
                wait_for_close,
            }),
        }
    }
    let mut study = compile(
        "input of = symbol(\"Symbol\", \"SPY\");\ninput every = interval(\"Interval\", \"1h\");\n\
         bars h = history(of, every);\nplot(\"x\", h.close);",
    )
    .study();
    assert_eq!(study.data_requests(), [request(Timeframe::Hour1)]);
    assert!(
        ConfiguredStudy::new(study.clone_box())
            .inputs()
            .iter()
            .any(|input| input.id == "wait_for_close")
    );
    // Chart bars at 23:30, 00:00, 00:30 and so on, under the hours from 00:00.
    let chart: Vec<_> = (-1..6).map(|at| bar(at * 30, 0.0)).collect();
    let days = [bar(-1440, 1.0), bar(0, 2.0)];
    let na = f64::NAN;
    for (step, len, from, hours, wait, expected) in [
        (
            "the hour of 02:00 is open",
            6,
            0,
            Some(&[10.0, 11.0, 12.0][..]),
            true,
            vec![na, na, 10.0, 10.0, 11.0, 11.0],
        ),
        (
            "no wait for it",
            6,
            0,
            Some(&[10.0, 11.0, 12.0][..]),
            false,
            vec![na, na, 10.0, 10.0, 11.0, 12.0],
        ),
        // 02:00 is no longer the latest bar.
        (
            "02:30 appended",
            7,
            6,
            Some(&[10.0, 11.0, 12.0][..]),
            false,
            vec![na, na, 10.0, 10.0, 11.0, 11.0, 12.0],
        ),
        (
            "open hour changed",
            7,
            0,
            Some(&[10.0, 11.0, 12.5][..]),
            false,
            vec![na, na, 10.0, 10.0, 11.0, 11.0, 12.5],
        ),
        (
            "first hour changed",
            7,
            0,
            Some(&[9.5, 11.0, 12.5][..]),
            false,
            vec![na, na, 9.5, 9.5, 11.0, 11.0, 12.5],
        ),
        (
            "02:30 removed",
            6,
            6,
            Some(&[9.5, 11.0, 12.5][..]),
            false,
            vec![na, na, 9.5, 9.5, 11.0, 12.5],
        ),
        // A replay moves the end of a history without a full recompute.
        (
            "02:00 no longer read",
            6,
            6,
            Some(&[9.5, 11.0][..]),
            false,
            vec![na, na, 9.5, 9.5, 11.0, 11.0],
        ),
        ("history gone", 6, 0, None, false, vec![na; 6]),
    ] {
        let hours = hours.map(|closes: &[f64]| {
            let hours = closes.iter().zip(0..);
            hours
                .map(|(&close, hour)| bar(hour * 60, close))
                .collect::<Vec<_>>()
        });
        // The chart also supplies what other studies request.
        let mut histories = vec![history(Timeframe::Day1, &days, wait)];
        if let Some(hours) = &hours {
            histories.push(history(Timeframe::Hour1, hours, wait));
        }
        study.update_data(&chart[..len], from, &histories);
        settle(&mut study, &chart[..len]);
        assert!(
            same(study.values().line(0), &expected),
            "{step}: {:?}",
            study.values().line(0)
        );
    }
}

/// The math helpers follow `f64` and propagate missing values, and take precedence over the
/// engine's own `min`, `max` and `sign`, whose `max(NaN, 1.0)` is 1.
#[test]
fn math_helpers_follow_f64_and_propagate_missing_values() {
    let bar = Bar::new(
        chrono::DateTime::UNIX_EPOCH,
        100.0,
        100.0,
        100.0,
        100.0,
        1.0,
    );
    for (expression, expected) in [
        ("max(1.0, na)", f64::NAN),
        ("max(na, 1.0)", f64::NAN),
        ("min(na, 1.0)", f64::NAN),
        ("min(2, 1.5)", 1.5),
        ("sign(-0.5)", -1.0),
        ("sign(0.0)", 0.0),
        ("round(1.25, 1)", 1.3),
        ("clamp(5.0, 0.0, 1.0)", 1.0),
        ("pow(2, 10)", 1024.0),
        ("abs(close - 101.0)", 1.0),
    ] {
        let study = calculated(
            &compile(&format!("plot(\"x\", {expression});")),
            std::slice::from_ref(&bar),
        );
        assert!(
            same(study.values().line(0), &[expected]),
            "{expression}: {:?}",
            study.values().line(0)
        );
    }
}

/// Arrays and maps compare and write as Rhai's own `==` and `to_string` do, which the engine
/// replaces to keep the stack flat: item by item, nested ones included, with keys in order.
#[test]
fn arrays_and_maps_compare_and_write_as_rhai_does() {
    for (expression, expected) in [
        (r#"[1, [2.5, "a"]] == [1, [2.5, "a"]]"#, true),
        (r#"[1, [2.5, "a"]] != [1, [2.5, "b"]]"#, true),
        (r#"[1, "a"] == [1, 2]"#, false),
        ("#{a: [1], b: #{c: ()}} == #{b: #{c: ()}, a: [1]}", true),
        ("#{a: 1} == #{b: 1}", false),
        (
            r#"`${[1, "a", [2.5, ()], #{b: [true], a: #{}}]}` == "[1, \"a\", [2.5, ()], #{\"a\": #{}, \"b\": [true]}]""#,
            true,
        ),
    ] {
        let source = format!("plot(\"x\", if {expression} {{ 1.0 }} else {{ 0.0 }});");
        let study = calculated(&compile(&source), &bars(1, 3));
        let expected = if expected { 1.0 } else { 0.0 };
        assert_eq!(study.values().line(0), [expected], "{expression}");
    }
}

/// The update contract `is_incremental` promises: appended bars, a changed last bar and a
/// change deep in history give the values of a fresh calculation, and an appended bar settles
/// within a second.
#[test]
fn updates_equal_full_calculations() {
    let mut rows: Vec<_> = examples()
        .iter()
        .map(|&(title, source)| (title, source, with_gaps(600, 2), 300))
        .collect();
    // The appended bar steps the call over the 20,000 bars it skipped.
    rows.push((
        "skipped call",
        "if bar_index < 20000 || bar_index == 40000 {\n    plot(\"x\", sma(close, 20000));\n}",
        bars(40_001, 2),
        40_000,
    ));
    for (title, source) in [
        ("study", r#"plot("x", study("RSI", period = 14));"#),
        (
            "chained study",
            r#"plot("x", study("RSI", source = study("EMA", period = 10)));"#,
        ),
        // Changes values it gave earlier bars.
        ("revising study", r#"plot("x", study("ZigZag"));"#),
        // As many studies as a script computes, the last for each value of the last bar.
        (
            "studies at the limit",
            r#"let s = 0.0; for n in 1..64 { s += study("SMA", period = n); }
            if bar_index == 599 { s += study("SMA", period = 100 + floor(close)); }
            plot("x", s);"#,
        ),
    ] {
        rows.push((title, source, bars(600, 2), 300));
    }
    for (title, source, fixture, start) in rows {
        let script = compile(source);
        let fresh = |bars: &[Bar]| calculated(&script, bars).values().columns().to_vec();
        let same_as_fresh = |study: &CustomIndicator, bars: &[Bar], step: &str| {
            let expected = fresh(bars);
            assert_eq!(study.values().len(), bars.len(), "{title}: {step}");
            for (line, expected) in expected.iter().enumerate() {
                assert!(
                    same(study.values().line(line), expected),
                    "{title}: {step}, plot {line}"
                );
            }
        };
        assert!(
            fresh(&fixture)
                .iter()
                .all(|line| line.len() == fixture.len())
        );
        let mut bars = fixture[..start].to_vec();
        let mut study = calculated(&script, &bars);
        for bar in &fixture[start..] {
            bars.push(bar.clone());
            let started = std::time::Instant::now();
            study.update(&bars, bars.len() - 1);
            settle(&mut study, &bars);
            let took = started.elapsed();
            assert!(
                took < Duration::from_secs(1),
                "{title}: appended bar took {took:?}"
            );
        }
        same_as_fresh(&study, &bars, "appended");
        for close in [101.5, 98.25] {
            let last = bars.len() - 1;
            bars[last].close = close;
            study.update(&bars, last);
            settle(&mut study, &bars);
            same_as_fresh(&study, &bars, "last bar changed");
        }
        bars[150].close += 3.0;
        study.update(&bars, 150);
        settle(&mut study, &bars);
        same_as_fresh(&study, &bars, "bar 150 changed");
    }
}

/// Done when 4 of #3631 for compile errors: a refused script names its line and column. For a
/// declaration the column is the token after the offending literal, where Rhai places a custom
/// syntax error.
#[test]
fn scripts_are_refused_at_their_line_and_column() {
    let choices = (0..65).map(|i| format!("\"c{i}\"")).collect::<Vec<_>>();
    let choices = format!(
        "input c = choice(\"C\", \"c0\", [{}]);\nplot(\"x\", 1.0);",
        choices.join(", ")
    );
    let histories: String = (10..51)
        .map(|i| format!("bars b{i} = history(\"\", \"1D\");\n"))
        .collect();
    let mut factory = IndicatorFactory::new();
    let script = compile("plot(\"x\", close);");
    factory.register("Script", move || Box::new(script.study()));
    for (source, line, column, fragment) in [
        // The `)` where the operand belongs.
        ("plot(\"x\", close +);", 1, 18, "Unexpected ')'"),
        ("", 1, 1, "needs at least one plot"),
        // The kind itself, the token after the `=`.
        (
            "input n = size(\"N\", 1, 0, 9);",
            1,
            11,
            "unknown input kind size",
        ),
        // The `=` after the id.
        (
            "input plot_offset = length(\"Offset\", 1, 0, 9);",
            1,
            19,
            "reserved by the chart",
        ),
        // The `;` after the declaration, once all its arguments are read.
        (
            "input n = length(\"N\", 0, 1, 9);",
            1,
            31,
            "default 0 outside",
        ),
        // The `,` after the repeated title.
        (
            "plot(\"x\", close);\nplot(\"x\", open);",
            2,
            9,
            "plot \"x\" is declared twice",
        ),
        // The first error, before a call that is refused too.
        (
            "plot(\"x\", clse + smaa(1));",
            1,
            11,
            "clse is not declared",
        ),
        // A branch no bar takes; the name of the call, counted by hand.
        (
            "if bar_index > 1000000 { smaa(close, 5); }\nplot(\"y\", close);",
            1,
            26,
            "no function smaa",
        ),
        // A method, after comments that end at a carriage return, which alone ends a line too.
        (
            "// one\r\nlet a = []; // two\rif false { a.psuh /* */ (1); }\nplot(\"x\", 1.0);",
            3,
            14,
            "no function psuh",
        ),
        // In the second expression of a string, after quotes that open none and a string that
        // ends in a function.
        (
            "let c = ['\"', '\\'']; fn f() { `${1}` }\n\
             let s = `a ${1} smab(1) ${#{b: 1}.b +\n  smaa(1)}`;\nplot(\"x\", 1.0);",
            3,
            3,
            "no function smaa",
        ),
        // A call in the caller's scope, the first error before an undeclared name.
        ("plot(\"x\", smaa!(clse, 5));", 1, 11, "no function smaa"),
        // The end of a range is no method, so the key of the same name does not define it.
        (
            "let m = #{n: 1};\nfor i in 0..n(2) {}\nplot(\"x\", m.n);",
            2,
            13,
            "no function n",
        ),
        // Nor does a variable that holds a function, which `f.call(1)` calls.
        (
            "fn g(x) { x }\nlet f = Fn(\"g\");\nplot(\"x\", f(1));",
            3,
            11,
            "no function f",
        ),
        // No clock, and no blocking of the thread, which the standard package's sleep does.
        ("plot(\"x\", timestamp());", 1, 11, "no function timestamp"),
        (
            "plot(\"x\", if bar_index == 20 { sleep(1); close } else { close });",
            1,
            32,
            "no function sleep",
        ),
        ("plot(\"x\", eval(\"1\"));", 1, 11, "eval"),
        ("import \"x\" as y;\nplot(\"x\", 1);", 1, 1, "import"),
        // A closure shares what it captures behind locks that sleep, and can hold itself.
        (
            "let a = [1];\nlet f = |x| { a.push(x); true };\nplot(\"x\", 1.0);",
            2,
            9,
            "anonymous functions",
        ),
        // Each step doubles a value that the size limits do not see into.
        (
            "fn g() { 1 } let f = Fn(\"g\"); for i in 0..20 { f = f.curry(f); }\nplot(\"x\", 1.0);",
            1,
            54,
            "curry",
        ),
        // The 65th choice, as the declaration walks its tokens again for each one.
        (
            choices.as_str(),
            1,
            choices.find("\"c64\"").unwrap() + 1,
            "at most 64 choices",
        ),
        // The token after the name.
        (
            "plot(\"x\", study(\"RS\", period = 14));",
            1,
            21,
            "no study \"RS\" is registered",
        ),
        (
            "plot(\"x\", study(RSI));",
            1,
            20,
            "the study's name is a string; got a name",
        ),
        (
            "plot(\"x\", study(\"RSI\" + \"a\"));",
            1,
            23,
            "Expecting ')'",
        ),
        // Values that would arrive later, and none to read.
        (
            "plot(\"x\", study(\"Script\"));",
            1,
            25,
            "study \"Script\" calculates on its own",
        ),
        (
            "plot(\"x\", study(\"Time Price Opportunity\"));",
            1,
            41,
            "has no line to read",
        ),
        // The `=` after the id.
        (
            "plot(\"x\", study(\"RSI\", perod = 14));",
            1,
            30,
            "study \"RSI\" has no input perod",
        ),
        (
            "plot(\"x\", study(\"RSI\", \"perods [0]\" = 5));",
            1,
            37,
            "study \"RSI\" has no input perods [0]",
        ),
        (
            "plot(\"x\", study(\"RSI\", period = 5, \"period\" = 6));",
            1,
            45,
            "input period is given twice",
        ),
        // The `)` after the title.
        (
            "plot(\"x\", study(\"RSI\", output = 3));",
            1,
            34,
            "output is a string; got an int",
        ),
        (
            "plot(\"x\", study(\"BB\", output = \"Lower\"));",
            1,
            39,
            "no output \"Lower\" with these inputs; it has \"BB Upper\", \"BB Middle\", \"BB Lower\"",
        ),
        // Every line of this study is an average, shown with its length.
        (
            "plot(\"x\", study(\"GMMA\", output = \"EMA\"));",
            1,
            39,
            "it has \"EMA(3)\", \"EMA(5)\"",
        ),
        (
            "bars h = hist(\"SPY\", \"1D\");\nplot(\"x\", h.close);",
            1,
            10,
            "expecting history(SYMBOL, INTERVAL)",
        ),
        (
            "bars h = history(spy, \"1D\");\nplot(\"x\", h.close);",
            1,
            21,
            "spy is not a symbol input declared above",
        ),
        (
            "bars h = history(\"SPY\", \"1X\");\nplot(\"x\", h.close);",
            1,
            29,
            "\"1X\" is no interval",
        ),
        (
            "input every = interval(\"Interval\", \"1X\");\nplot(\"x\", 1.0);",
            1,
            41,
            "\"1X\" is no interval",
        ),
        // The `;` after the 41st.
        (histories.as_str(), 41, 29, "at most 40 histories"),
    ] {
        let error = Script::compile("Test", source, &factory).unwrap_err();
        assert_eq!(
            (error.line, error.column, error.bar),
            (line, column, None),
            "{source:?}: {error}"
        );
        assert!(error.message.contains(fragment), "{source:?}: {error}");
    }
}

/// What a script may call compiles: its own functions, helpers, the functions and methods of
/// Rhai's values and the functions a map holds; and no text, comment or keyword before a `(` is
/// a call.
#[test]
fn defined_calls_compile() {
    let source = r####"
        // smaa(close), it's a "note
        /* smaa(1) /* ` */ smaa(2) ' " */
        display(overlay = true);
        input n = length("smaa(n)", 2, 1, 9);
        input i = int("I", 1, 0, 2);
        input x = float("F", 1.0, 0.0, 2.0);
        input b = bool("B", true);
        input c = choice("C", "a", ["a", "b"]);
        input s = source("S", close);
        input t = text("T", "smaa(1)");
        input of = symbol("Symbol", "");
        input every = interval("Interval", "1D");
        bars h = history(of, every);
        state kept = 0;
        series d = (close - open);
        plot("smaa(x)", later(close));
        plot("y", (close + open) / 2);
        fill("smaa(x)", "y");

        is_na(d); nz(d); nz(d, 1); abs(d); sign(d); sqrt(d); exp(d); ln(d); log10(d); pow(d, 2);
        min(d, 1); max(d, 1); clamp(d, 0, 1); round(d); round(d, 1); floor(d); ceil(d);
        sma(d, 2); ema(d, 2); rma(d, 2); wma(d, 2); sum(d, 2); stdev(d, 2); highest(d, 2);
        lowest(d, 2); change(d); change(d, 1); crossover(d, 0); crossunder(d, 0);
        year(time); month(time); day(time); weekday(time); hour(time); minute(time);
        study("RSI", source = study ("EMA"))[1]; h.close[1];

        fn later(x) { nz (x) }
        fn int.twice() { this * 2 }
        fn scoped() { 1 }
        fn area() { this.w * 2 }
        fn big(x) { x > 1 }
        let z = 3;
        z.twice(); scoped!(); later
            /* the next line */ (z);

        let a = [1, 2];
        a.push(3); a.map(Fn("later")).filter(Fn("big")).len(); a?.sort(); a.index_of(1);
        let m = #{w: 2, plain: Fn("area"), "quoted": Fn("area")};
        m["indexed"] = area;
        m.plain(); m.quoted(); m.indexed(); m.keys(); m.contains("w"); m.remove("w");
        "text".to_upper ().sub_string(1).split("x"); "1".parse_int(); 1.5.to_int(); 'c'.to_upper();
        (0..2).contains(1); type_of(z); print(z); call(Fn("later"), 1); Fn("later").call(1);
        is_def_fn("later", 1); is_def_var("z"); is_shared(z); PI();

        let quotes = ['"', '\'', "say ""smaa(1)"" \"smaa(2)\" \\", "a\
            smaa(3)", #"a" smaa(4) \"#, ##"a"# smaa(5)"##];
        `smaa(6) ${if z > 1 { `in ${later(z)} smaa(7)` } else { #{q: 1}.q }} smaa(8) \${smaa(9)} ``x`;

        while (z < 0) { break (1); }
        do { z += 1; } until (z > 5);
        for (v, k) in (0..2) {}
        if !(z > 9) && z in (0..9) && z !in (3..4) && z != (2) {}
        switch (z) { 1if (z > 1) => 1, _ => 2 }
        try { throw (1); } catch (e) { return (2); }
    "####;
    let script = Script::compile("Test", source, &IndicatorFactory::new());
    assert!(script.is_ok(), "{script:?}");
}

/// A script as deep as the limits admit: the top level nests calls as deep as it may around a
/// function that calls itself `calls` times, each call nested in its body as deep as a function
/// may, and runs `bottom` in the last call.
fn deepest(calls: usize, bottom: &str) -> String {
    let nest = |depth, inner: &str| {
        (0..depth).fold(inner.to_owned(), |inner, _| format!("max(1, {inner})"))
    };
    format!(
        "fn f(n) {{ if n > 0 {{ {} }} else {{ {bottom} }} }} plot(\"x\", {});",
        nest(11, "f(n - 1)"),
        nest(28, &format!("f({calls})"))
    )
}

/// Done when 4 of #3631 for runtime errors: the bars before an error keep their values, the
/// error names its line, column and bar, and later bars are missing.
#[test]
fn run_errors_keep_the_bars_before_them() {
    let fixture = bars(1001, 3);
    // Needs under 6 MiB of stack in a debug build.
    let deep = deepest(40, "0");
    // Column 0 leaves the column unasserted where the engine, not the script, places it.
    for (source, column, bar, fragment) in [
        (deep.as_str(), 0, 0, "more than 32 deep"),
        // The `[` of the offset, counted by hand.
        (
            "plot(\"x\", if bar_index == 20 { close[-1] } else { close });",
            37,
            20,
            "history offset -1 is negative",
        ),
        (
            "fn f(x) { f(x) } plot(\"x\", f(1));",
            0,
            0,
            "more than 32 deep",
        ),
        (
            "let s = \"a\"; for i in 0..20 { s += s; } plot(\"x\", 1.0);",
            0,
            0,
            "string",
        ),
        // The name compiles and its argument types are refused when the call runs.
        (
            "plot(\"x\", sma(\"text\", 5));",
            11,
            0,
            "no function sma takes (string, int)",
        ),
        (
            "let d = close - open; plot(\"x\", d[1]);",
            34,
            0,
            "series d = ...",
        ),
        ("plot(\"x\", \"text\");", 11, 0, "holds numbers; got string"),
        // The standard package's sleep blocks the thread, and this argument panics in it.
        (
            "plot(\"x\", if bar_index == 20 { call(Fn(\"sleep\"), 1e20); close } else { close });",
            0,
            20,
            "no function sleep",
        ),
        // An index adds a key without a size check; the state's declaration holds the map.
        (
            "state m = 0;\nif bar_index == 0 { m = #{}; }\nlet i = bar_index * 1;\n\
             m[\"k\" + i] = 1.0;\nplot(\"x\", close);",
            7,
            1000,
            "Size of object map",
        ),
        // The declaration is found after a quote that opens no string.
        (
            "let c = '\"'; state m = 0; if bar_index == 0 { m = #{}; } let i = bar_index * 1; \
             m[\"k\" + i] = 1.0; plot(\"x\", close);",
            20,
            1000,
            "Size of object map",
        ),
        // The same in one bar: the count of the running values holds a local map to the limit.
        (
            "let m = #{}; let i = 0; loop { m[\"k\" + i] = 1; i += 1; } plot(\"x\", 1.0);",
            0,
            0,
            "Size of object map",
        ),
        // The value at fault of a study call, and otherwise its name.
        (
            "plot(\"x\", if bar_index < 20 { close } else { study(\"RSI\", period = 0) });",
            68,
            20,
            "RSI input period takes an int from 1; got 0",
        ),
        (
            "series d = close - open; \
             plot(\"x\", if bar_index < 20 { close } else { study(\"RSI\", source = d) });",
            93,
            20,
            "cannot read the series d",
        ),
        (
            "plot(\"x\", if bar_index < 20 { close } else { study(\"RSI\", calculation_symbol = \"SPY\") });",
            52,
            20,
            "RSI needs histories a script cannot supply",
        ),
        (
            "plot(\"x\", if bar_index < 20 { close } else { study(\"Trade Rate\") });",
            52,
            20,
            "Trade Rate needs order flow a script cannot supply",
        ),
        // With an input given the lines are known when the call runs.
        (
            "plot(\"x\", if bar_index < 20 { close } else { study(\"RSI\", period = 5, output = \"K\") });",
            52,
            20,
            "RSI has no output \"K\" with these inputs; it has \"RSI\"",
        ),
        // One study for each of the bars before.
        (
            "let s = study(\"SMA\", period = bar_index + 1); plot(\"x\", close);",
            15,
            64,
            "at most 64 studies",
        ),
        // The dot, in a script whose history is missing.
        (
            "bars h = history(\"SPY\", \"1D\"); \
             plot(\"x\", if bar_index < 20 { close } else { h.foo });",
            78,
            20,
            "bars have no field foo; expected one of open, high, low, close, volume, hl2, hlc3, \
             ohlc4, time",
        ),
    ] {
        let script = compile(source);
        let error = script.check(&fixture).unwrap_err();
        assert_eq!((error.line, error.bar), (1, Some(bar)), "{source}: {error}");
        if column > 0 {
            assert_eq!(error.column, column, "{source}: {error}");
        }
        assert!(error.message.contains(fragment), "{source}: {error}");
        let study = calculated(&script, &fixture);
        let line = study.values().line(0);
        let closes: Vec<f64> = fixture[..bar].iter().map(|bar| bar.close).collect();
        assert_eq!(line.len(), fixture.len(), "{source}");
        assert!(same(&line[..bar], &closes), "{source}");
        assert!(line[bar..].iter().all(|value| value.is_nan()), "{source}");
        assert_eq!(script.last_error(), Some(error), "{source}");
    }
}

fn limited(source: &str, limits: Limits) -> Script {
    compile(source).with_limits(limits)
}

fn stopped(calculation: Option<Calculation>) -> String {
    match calculation {
        Some(Calculation::Stopped(message)) => message,
        other => panic!("expected a stop, got {other:?}"),
    }
}

/// The one-bar limit stops the newest bar and reports its line and bar, for work in the bar
/// and in the callbacks of array functions, a stop the script catches included.
#[test]
fn one_bar_limit_stops_the_newest_bar_at_its_line() {
    let fixture = bars(50, 4);
    for (source, line) in [
        (
            "plot(\"x\", close);\nif bar_index == 49 { let i = 0; loop { i += 1; } }",
            2,
        ),
        // 4 million callbacks, each counted by Rhai from a copy of the count that it drops.
        (
            "plot(\"x\", close);\nfn never(y) { false } \
             fn inner(x) { let b = []; b.pad(2000, 0); b.filter(Fn(\"never\")).len() > 0 } \
             if bar_index == 49 { let a = []; a.pad(2000, 0); a.filter(Fn(\"inner\")); }",
            2,
        ),
        // A stop in a callback reaches the script as a catchable error and leaves no position.
        (
            "plot(\"x\", close);\nfn spin(x) { loop {} } \
             if bar_index == 49 { try { [1].map(Fn(\"spin\")); } catch {} }",
            0,
        ),
    ] {
        let script = limited(
            source,
            Limits {
                bar: Duration::from_millis(50),
                ..Limits::default()
            },
        );
        let mut study = script.study();
        study.calculate(&fixture[..49]);
        assert!(
            matches!(
                settle(&mut study, &fixture[..49]),
                Some(Calculation::Done(_))
            ),
            "{source}"
        );
        study.update(&fixture, 49);
        let message = stopped(settle(&mut study, &fixture));
        assert!(message.contains("for one bar"), "{source}: {message}");
        let error = script.last_error().expect("the stop is reported");
        assert_eq!(
            (error.line, error.bar),
            (line, Some(49)),
            "{source}: {error}"
        );
    }
    // A bar of too few operations for a check: the study it calls is checked itself.
    let script = limited(
        "plot(\"x\", close);\nplot(\"y\", study(\"RSI\"));",
        Limits {
            bar: Duration::ZERO,
            ..Limits::default()
        },
    );
    let error = script.check(&fixture[..1]).unwrap_err();
    assert_eq!((error.line, error.bar), (2, Some(0)), "{error}");
    assert!(error.message.contains("for one bar"), "{error}");
}

/// The full-recompute limit stops runs of short bars between bars, and runs of long bars inside
/// one.
#[test]
fn full_recompute_limit_stops_long_runs() {
    let limits = Limits {
        full: Duration::from_millis(10),
        ..Limits::default()
    };
    for (source, n) in [
        ("plot(\"x\", close * 2.0);", 100_000),
        (
            "let s = 0.0; for i in 0..1000 { s += i; } plot(\"x\", s);",
            10_000,
        ),
    ] {
        let script = limited(source, limits);
        let fixture = bars(n, 5);
        let mut study = script.study();
        study.calculate(&fixture);
        let message = stopped(settle(&mut study, &fixture));
        assert!(message.contains("full recompute"), "{source}: {message}");
        let error = script.last_error().expect("the stop is reported");
        assert!(
            error.bar.is_some_and(|bar| bar < n - 1),
            "{source}: {error}"
        );
    }
}

/// The memory cap counts series buffers, helper windows, the lines of studies, the values states
/// keep and the values a bar holds while it runs at every call level, and stops a script whose
/// buffers pass it at the declaration or call holding the most, or at the line running when the
/// bar's own values hold more.
#[test]
fn memory_cap_stops_scripts_whose_buffers_pass_it() {
    let mut source: String = (0..64).map(|i| format!("series s{i} = close;\n")).collect();
    source += "plot(\"x\", s0);";
    let fixture = bars(100_000, 6);
    // 100 windows of 1,000 values, counted when created; fails if a window counts only the
    // values it holds so far.
    let windows = "let s = 0.0;\nfor i in 0..100 { s += sma(close, 1000); }\nplot(\"x\", s);";
    // A state map that gains a key of 9,000 bytes on every bar.
    let kept = "state m = 0;\nif bar_index == 0 { m = #{}; }\nlet k = \"x\"; k.pad(9000, \"y\");\n\
                let i = bar_index * 1;\nm[k + i] = 1;\nplot(\"x\", close);";
    // A state whose arrays keep their room after `clear`, 144 KB more on every bar.
    let room = "state s = 0;\nstate n = 0;\nif n == 0 { s = []; s.pad(1000, []); }\n\
                s[n].pad(9000, 0);\ns[n].clear();\nn += 1;\nplot(\"x\", s.len());";
    // The same in one bar, held by a local array.
    let bar = "let s = []; s.pad(1000, []);\n\
               for j in 0..1000 { s[j].pad(9000, 0); s[j].clear(); }\nplot(\"x\", s.len());";
    // Strings cut short keep their room too, 8 KB each.
    let cut = "let v = [];\n\
               for j in 0..400 { let t = \"x\"; t.pad(8000, \"y\"); t.truncate(24); v.push(t); }\n\
               plot(\"x\", v.len());";
    // 4 MB at each call level, below the limit at each and above it from the third on; a stop
    // inside a function is reported at the call the bar is running.
    let levels = "fn hold(n) {\n\
                  let v = []; v.pad(256, []); for j in 0..256 { v[j].pad(1000, 0); v[j].clear(); }\n\
                  if n > 0 { hold(n - 1) + v.len() } else { v.len() }\n\
                  }\nplot(\"x\", hold(8));";
    // 64 studies of 1,000 values and a copy of each read, 1,024,000 bytes beside the bars.
    let studies = "let s = 0.0;\nfor i in 1..65 { s += study(\"SMA\", period = i); }\n\
                   plot(\"x\", s);";
    for (script, fixture, mib, line) in [
        (source.as_str(), &fixture[..], 16, 1),
        (windows, &fixture[..1000], 1, 2),
        (kept, &fixture[..1000], 1, 1),
        (room, &fixture[..300], 64, 1),
        (bar, &fixture[..1], 16, 2),
        (cut, &fixture[..1], 2, 2),
        (levels, &fixture[..1], 8, 5),
        (studies, &fixture[..1000], 1, 2),
    ] {
        let mut capped = limited(
            script,
            Limits {
                memory: mib << 20,
                ..Limits::default()
            },
        )
        .study();
        capped.calculate(fixture);
        assert_eq!(
            stopped(settle(&mut capped, fixture)),
            format!("memory limit of {mib} MiB reached at line {line}")
        );
    }
    // The default memory limit, and no time limit a slow or busy machine could reach.
    let mut study = limited(
        &source,
        Limits {
            full: Duration::from_secs(3600),
            ..Limits::default()
        },
    )
    .study();
    study.calculate(&fixture);
    assert!(matches!(
        settle(&mut study, &fixture),
        Some(Calculation::Done(_))
    ));
}

/// A stopped run keeps the previous output, and the study runs again on an input change, one made
/// while the run was stopping included, not on a tick.
#[test]
fn stopped_runs_keep_the_previous_output_until_inputs_change() {
    let script = limited(
        "input slow = bool(\"Slow\", false);\nif slow { loop {} }\nplot(\"x\", close - close[1]);",
        Limits {
            bar: Duration::from_millis(50),
            ..Limits::default()
        },
    );
    let mut fixture = bars(200, 7);
    let differences = |bars: &[Bar]| -> Vec<f64> {
        (0..bars.len())
            .map(|at| {
                if at == 0 {
                    f64::NAN
                } else {
                    bars[at].close - bars[at - 1].close
                }
            })
            .collect()
    };
    let mut study = calculated(&script, &fixture);
    assert!(same(study.values().line(0), &differences(&fixture)));
    let before = study.values().line(0).to_vec();

    study.set_input("slow", InputValue::Toggle(true)).unwrap();
    study.update(&fixture, 0);
    stopped(settle(&mut study, &fixture));
    assert!(same(study.values().line(0), &before));

    fixture[199].close += 1.0;
    study.update(&fixture, 199);
    stopped(settle(&mut study, &fixture));
    assert!(same(study.values().line(0), &before));

    study.set_input("slow", InputValue::Toggle(false)).unwrap();
    study.update(&fixture, 0);
    assert!(matches!(
        settle(&mut study, &fixture),
        Some(Calculation::Done(_))
    ));
    assert!(same(study.values().line(0), &differences(&fixture)));

    // A change made while a run that stops is in flight, or as its stop arrives, runs once it
    // stops, as typing a length in the settings dialog passes through values that stop.
    study.set_input("slow", InputValue::Toggle(true)).unwrap();
    study.update(&fixture, 0);
    study.set_input("slow", InputValue::Toggle(false)).unwrap();
    study.update(&fixture, 0);
    assert!(matches!(
        settle(&mut study, &fixture),
        Some(Calculation::Done(_))
    ));
}

/// History reads reach the first loaded bar, with no lookback limit.
#[test]
fn history_reads_reach_the_first_loaded_bar() {
    let fixture = bars(30_000, 8);
    let first = fixture[0].close;
    for (source, expected) in [
        ("plot(\"x\", close[bar_index]);", first),
        (
            "series d = close * 2.0; plot(\"y\", d[bar_index]);",
            first * 2.0,
        ),
        ("plot(\"z\", close[bar_index + 1]);", f64::NAN),
    ] {
        let study = calculated(&compile(source), &fixture);
        assert!(
            same(study.values().line(0), &vec![expected; fixture.len()]),
            "{source}"
        );
    }
}

/// On the web a script runs on the page's thread, whose stack holds the deepest script until the
/// call limit stops it.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn web_deepest_script_stops_at_the_call_limit() {
    let error = compile(&deepest(40, "0")).check(&bars(2, 3)).unwrap_err();
    assert_eq!(error.bar, Some(0), "{error}");
    assert!(error.message.contains("more than 6 deep"), "{error}");
    // A study calculates in the deepest call.
    let calls = crate::rhai::engine::MAX_CALL_LEVELS - 1;
    let checked = compile(&deepest(calls, "study(\"RSI\")[0]")).check(&bars(1, 3));
    assert!(checked.is_ok(), "{checked:?}");
}

/// On the web the stack also holds the deepest value the size limits admit in the deepest call the
/// call limit admits: compared, written, and written to JSON as deep there as a function may nest
/// an expression, which needs the most.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn web_deepest_value_fits_in_the_deepest_call() {
    use crate::rhai::engine::{MAX_ARRAY_SIZE, MAX_CALL_LEVELS, MAX_MAP_SIZE};
    // One entry is left for the map the value is written to JSON in.
    let maps = MAX_MAP_SIZE - 1;
    // `#{}`, then `#{"x": ` and `}` for each map and `[` and `]` for each array.
    let written = 3 + 8 * maps + 2 * MAX_ARRAY_SIZE;
    let json = (0..11).fold("#{y: a}.to_json().len()".to_owned(), |inner, _| {
        format!("max(1, {inner})")
    });
    let bottom = format!(
        "let a = #{{}}; for i in 0..{maps} {{ a = #{{x: a}}; }} \
         for i in 0..{MAX_ARRAY_SIZE} {{ a = [a]; }} let b = a; \
         if a != b || `${{a}}`.len() != {written} {{ throw \"unequal or miswritten\"; }} {json}"
    );
    let checked = compile(&deepest(MAX_CALL_LEVELS - 1, &bottom)).check(&bars(1, 3));
    assert!(checked.is_ok(), "{checked:?}");
}

/// On the web a run proceeds in time slices over several calls instead of freezing the page.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn web_time_slices_finish_over_several_frames() {
    let fixture = bars(100_000, 9);
    let mut study = compile(examples()[0].1).study();
    study.calculate(&fixture);
    let Some(Calculation::Running(mut done)) = study.calculation() else {
        panic!(
            "a long run returns before it ends: {:?}",
            study.calculation()
        );
    };
    assert!(study.values().is_empty());
    loop {
        study.update(&fixture, fixture.len());
        match study.calculation() {
            Some(Calculation::Running(now)) => {
                assert!(now > done, "{now} after {done}");
                done = now;
            }
            Some(Calculation::Done(_)) => break,
            other => panic!("{other:?}"),
        }
    }
    let mut native = RSI::new(14);
    native.calculate(&fixture);
    assert!(same(study.values().line(0), native.values().line(0)));
}
