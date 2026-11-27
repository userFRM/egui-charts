//! Scripts against independent implementations: the native studies and `studies::helpers`,
//! hand computed values, and the update contract of `Indicator`.

use crate::{
    EntryKind, Limits, Profile, Reference, Script, ScriptError, TokenClass, examples, tokens,
};
use ecolor::Color32;
use egui_charts::model::{Bar, BarClock, PriceSource, Timeframe};
use egui_charts::studies::{
    Calculation, ConfiguredStudy, CrossDirection, CustomIndicator, Indicator, IndicatorFactory,
    IndicatorValue, InputKind, InputValue, LabelPlacement, MACD, PlotType, RSI, Stochastic,
    StudyChart, StudyData, StudyDataRequest, StudyInput, StudyLabel, StudySegment, StudySource,
    StudyTemplate, StudyZone, helpers,
};
use std::collections::HashSet;
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

/// One-minute bars from 1970 with these opens and closes, high and low at the larger and the
/// smaller of the two.
fn opens_closes(prices: &[(f64, f64)]) -> Vec<Bar> {
    let bar = |(i, &(open, close)): (usize, &(f64, f64))| {
        let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
        Bar::new(time, open, open.max(close), open.min(close), close, 1.0)
    };
    prices.iter().enumerate().map(bar).collect()
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
        // Keys named as declarations stay keys when the bars run without the declarations.
        ("#{state: 1, display: 2}.len() == 2", true),
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
        // Colours and drawings, which a bar may make for an earlier one.
        (
            "colours and drawings",
            r##"display(overlay = true);
            plot("x", close, color = if close > close[1] { rgb(0, 200, 0) } else { na });
            plot("b", close, style = "bar_color", color = if close > open { "#2962FF" } else { na });
            plot("m", na, color = "#FF6D00");
            series hh = highest(high, 20);
            series ll = lowest(low, 20);
            if bar_index >= 20 && close > hh[1] {
                label("m", bar_index - 1, high[1], "Break");
                zone("m", bar_index - 5, bar_index, high, low);
            }
            if bar_index >= 20 && close < ll[1] {
                segment("m", bar_index - 20, low[20], bar_index, low);
                marker(bar_index, low, "down");
            }"##,
        ),
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
            let fresh = calculated(&script, bars);
            assert_eq!(study.values().len(), bars.len(), "{title}: {step}");
            for (line, expected) in fresh.values().columns().iter().enumerate() {
                assert!(
                    same(study.values().line(line), expected),
                    "{title}: {step}, plot {line}"
                );
                for bar in 0..bars.len() {
                    assert_eq!(
                        study.plot_color(line, bar),
                        fresh.plot_color(line, bar),
                        "{title}: {step}, plot {line}, bar {bar}"
                    );
                }
            }
            assert_eq!(study.labels(), fresh.labels(), "{title}: {step}");
            assert_eq!(
                study.price_segments(),
                fresh.price_segments(),
                "{title}: {step}"
            );
            assert_eq!(study.zones(), fresh.zones(), "{title}: {step}");
            // `IndicatorMarker` has no `PartialEq`.
            let markers = |study: &CustomIndicator| format!("{:?}", study.markers());
            assert_eq!(markers(study), markers(&fresh), "{title}: {step}");
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
    let alerts: String = (0..33)
        .map(|i| format!("alert(\"a{i}\", true);\n"))
        .chain(["plot(\"x\", 1.0);".into()])
        .collect();
    let mut factory = IndicatorFactory::new();
    let script = compile("plot(\"x\", close);");
    factory.register("Script", move || Box::new(script.study()));
    for (source, line, column, fragment) in [
        // The `)` where the operand belongs.
        ("plot(\"x\", close +);", 1, 18, "Unexpected ')'"),
        ("", 1, 1, "needs at least one plot"),
        // A reading is not an order, so the script still needs a plot.
        ("let p = position();", 1, 1, "needs at least one plot"),
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
        // An alert's message is read when the script compiles: the token after the `(`.
        (
            "let m = \"x\";\nalert(m, true);\nplot(\"x\", 1.0);",
            2,
            7,
            "a string in double quotes",
        ),
        (
            "alert('x', true);\nplot(\"x\", 1.0);",
            1,
            7,
            "a string in double quotes",
        ),
        (
            "alert(\"a\\\"b\", true);\nplot(\"x\", 1.0);",
            1,
            7,
            "without escapes",
        ),
        (
            "alert(\" \", true);\nplot(\"x\", 1.0);",
            1,
            7,
            "the message is empty",
        ),
        // The `)` where the condition belongs.
        (
            "alert(\"x\");\nplot(\"x\", 1.0);",
            1,
            10,
            "a message and a condition",
        ),
        // The 33rd message.
        (alerts.as_str(), 33, 7, "at most 32 alerts"),
        // An alert is no plot.
        ("alert(\"x\", true);", 1, 1, "needs at least one plot"),
        // The `;` after the call: bar colours repaint the price bars.
        (
            "plot(\"x\", close, style = \"bar_color\");",
            1,
            38,
            "display(overlay = true) comes before it",
        ),
        // The `;` after the declaration.
        (
            "input c = color(\"C\", \"red\");\nplot(\"x\", 1.0);",
            1,
            28,
            "the default of a color input is \"#RRGGBB\" or \"#RRGGBBAA\"; got \"red\"",
        ),
        (
            "input c = color(\"C\", \"#12345\");\nplot(\"x\", 1.0);",
            1,
            31,
            "the default of a color input",
        ),
        // A drawing's plot is read when the script compiles: the token after the `(`.
        (
            "plot(\"x\", close);\nlabel(\"Nope\", bar_index, close, \"t\");",
            2,
            7,
            "no plot \"Nope\" is declared",
        ),
        (
            "plot(\"x\", close);\nlet t = \"x\";\nzone(t, 0, bar_index, high, low);",
            3,
            6,
            "zone names its plot by a title in double quotes, without escapes",
        ),
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
/// An error's span is the name or number at its line and column, counted in characters, or the
/// single character there, clamped to the text.
#[test]
fn script_error_spans_cover_the_token_at_the_position() {
    let error = |line, column| ScriptError {
        message: String::new(),
        line,
        column,
        bar: None,
    };
    for (text, line, column, expected) in [
        ("let x = foo(1);", 1, 9, 8..11),
        // Two-byte characters before the line and before the column: `foo` starts at byte 25.
        ("let \u{e9} = 1;\nlet y = \u{e9} + foo;", 2, 13, 25..28),
        ("x + ;", 1, 5, 4..5),
        // No position.
        ("let x = 1;", 0, 0, 0..0),
        ("a\nb", 3, 1, 3..3),
        ("let x = 1;", 1, 40, 10..10),
    ] {
        assert_eq!(
            error(line, column).span(text),
            expected,
            "{text:?} {line}:{column}"
        );
    }
    let text = "plot(\"x\", nope(close));";
    let compiled = Script::compile("t", text, &IndicatorFactory::new()).unwrap_err();
    assert_eq!(&text[compiled.span(text)], "nope", "{compiled}");
}

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

    // The names of the orders and readings stay free for a script's own variables, inputs and
    // functions, and only a call of an order the script does not define makes it a strategy,
    // which needs no plot.
    for (source, orders) in [
        ("entry(\"L\", \"long\");", true),
        ("let entry = close; plot(\"x\", entry);", false),
        (
            "input price = source(\"Price\", close); plot(\"x\", price);",
            false,
        ),
        (
            "fn entry(a, b) { a + b } plot(\"x\", entry(close, 1.0));",
            false,
        ),
    ] {
        assert_eq!(compile(source).places_orders(), orders, "{source}");
    }
    let fixture = bars(30, 1);
    let own = compile("fn entry(a, b) { a + b } plot(\"x\", entry(close, 1.0));");
    let expected: Vec<_> = fixture.iter().map(|bar| bar.close + 1.0).collect();
    assert!(same(calculated(&own, &fixture).values().line(0), &expected));
}

/// The reference lists what a script compiles with: the studies a script can call, each input and
/// output as written, each helper and series as written, and a host study's text intact in both
/// formats.
#[test]
fn reference_names_compile_as_listed() {
    let host = "Host \"quoted\"";
    let mut factory = IndicatorFactory::new();
    factory.register(host, move || {
        let input = StudyInput::new(
            "smoothing length",
            "Smoothing",
            InputValue::Length(3),
            InputKind::Length(1, 9),
        );
        let none = |bars: &[Bar]| vec![IndicatorValue::None; bars.len()];
        let study = CustomIndicator::new(host, none)
            .with_inputs(vec![input], move |bars, _, _, _| none(bars))
            .unwrap()
            .with_description("a *b* _c_ `d` \\ e\nf")
            .with_overlay(false)
            .with_line_names(vec!["Line \"one\"".into()]);
        Box::new(study)
    });
    let reference = Reference::new(&factory);
    let entries = reference.entries();

    let callable = |name: &String| {
        factory.create(name).is_some_and(|mut study| {
            study.calculate(&[]);
            study.line_count() > 0
                && study.calculation().is_none()
                && study.data_requests().is_empty()
                && study.order_flow_inputs().is_empty()
        })
    };
    let mut expected: Vec<_> = factory.list().into_iter().filter(callable).collect();
    // Its list of compared symbols requests their histories; a call can empty it.
    expected.push("Performance".into());
    expected.sort();
    let studies: Vec<_> = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::Study)
        .collect();
    let names: Vec<_> = studies.iter().map(|study| study.name.clone()).collect();
    assert_eq!(names, expected);
    for (name, listed) in [
        ("RSI", true),
        ("Time Price Opportunity", false),
        ("Book Size", false),
    ] {
        assert_eq!(names.iter().any(|listed| listed == name), listed, "{name}");
    }
    for study in &studies {
        let created = factory.create(&study.name).unwrap();
        let items = |kind| study.items.iter().filter(move |item| item.kind == kind);
        let ids: HashSet<_> = items(EntryKind::Input).map(|item| &item.name).collect();
        for input in created.inputs() {
            assert!(ids.contains(&input.id), "{}: {}", study.name, input.id);
        }
        let outputs: Vec<_> = items(EntryKind::Output).collect();
        assert_eq!(outputs.len(), created.line_names().len(), "{}", study.name);
        // Inputs that take no value from a script are listed by id alone.
        let given: Vec<_> = items(EntryKind::Input)
            .map(|item| {
                if item.signature.contains(" = ") {
                    item.signature.clone()
                } else {
                    format!("{} = 0", item.signature)
                }
            })
            .collect();
        let mut source = String::new();
        for chunk in given.chunks(60) {
            source += &format!("let v = study({:?}, {});\n", study.name, chunk.join(", "));
        }
        for output in outputs {
            source += &format!("let v = study({:?}, {});\n", study.name, output.signature);
        }
        source += "plot(\"x\", 1.0);";
        if let Err(error) = Script::compile("Test", &source, &factory) {
            panic!("{source}: {error}");
        }
    }
    let performance = studies.iter().find(|study| study.name == "Performance");
    let summary = &performance.unwrap().summary;
    assert!(summary.contains("A call gives symbols = \"\""), "{summary}");
    let call = |inputs: &str| {
        let source = format!("plot(\"x\", study(\"Performance\"{inputs}));");
        compile(&source).check(&bars(30, 11))
    };
    assert_eq!(call(", symbols = \"\""), Ok(()));
    assert!(call("").unwrap_err().message.contains("needs histories"));

    let fixture = bars(30, 11);
    for entry in entries
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::Helper | EntryKind::Series))
    {
        let value = match entry.signature.split_once('(') {
            None => entry.signature.clone(),
            Some((name, parameters)) => {
                let arguments: Vec<_> = parameters
                    .trim_end_matches(')')
                    .split(", ")
                    .filter(|parameter| !parameter.is_empty())
                    .map(|parameter| match parameter {
                        "x" | "a" => "close",
                        "b" => "open",
                        "t" => "time",
                        "y" => "1.0",
                        "n" => "2",
                        "lo" => "0.0",
                        "hi" => "200.0",
                        "digits" => "1",
                        "id" => "\"o\"",
                        "side" => "\"long\"",
                        "options" => "#{}",
                        "profit" | "loss" => "1.0",
                        "message" => "\"m\"",
                        "condition" => "true",
                        "r" | "g" => "10",
                        "alpha" => "0.5",
                        "plot" => "\"x\"",
                        "bar" | "bar1" | "bar2" => "bar_index",
                        "price" | "price1" | "price2" | "top" | "bottom" => "close",
                        "text" => "\"t\"",
                        "placement" => "\"below\"",
                        "direction" => "\"up\"",
                        other => panic!("{}: no argument for {other}", entry.signature),
                    })
                    .collect();
                format!("{name}({})", arguments.join(", "))
            }
        };
        let script = compile(&format!("let v = {value}; plot(\"x\", 1.0);"));
        assert_eq!(script.check(&fixture), Ok(()), "{value}");
    }

    let markdown = reference.to_markdown();
    assert!(markdown.starts_with("# Script reference\n\n"), "{markdown}");
    for written in [
        "\n### Host \"quoted\"\n\n`study(\"Host \\\"quoted\\\"\")`, Custom. a \\*b\\* \\_c\\_ \\`d\\` \\\\ \
         e f. Drawn in a pane of its own.\n",
        "\n- `\"smoothing length\" = 3`: Smoothing: an int from 1 to 9.\n",
    ] {
        assert!(markdown.contains(written), "{written}");
    }
    let json = reference.to_json();
    let json = ::rhai::Engine::new_raw().parse_json(&json, true).unwrap();
    assert_eq!(json["format"].as_int(), Ok(1));
    let listed = json["entries"].clone().into_array().unwrap();
    assert_eq!(listed.len(), entries.len());
    let field = |entry: &::rhai::Dynamic, key: &str| {
        let entry = entry.clone().cast::<::rhai::Map>();
        entry[key].clone()
    };
    let text = |value: ::rhai::Dynamic| value.into_string().unwrap();
    let host = listed
        .iter()
        .find(|entry| text(field(entry, "name")) == host)
        .unwrap();
    assert_eq!(text(field(host, "kind")), "study");
    assert_eq!(
        text(field(host, "summary")),
        "a *b* _c_ `d` \\ e\nf. Drawn in a pane of its own."
    );
    let items = field(host, "items").into_array().unwrap();
    assert_eq!(text(field(&items[0], "name")), "smoothing length");
    assert_eq!(
        text(field(&items[0], "signature")),
        "\"smoothing length\" = 3"
    );
    let output = items.last().unwrap();
    assert_eq!(
        (text(field(output, "kind")), text(field(output, "name"))),
        ("output".to_owned(), "Line \"one\"".to_owned())
    );
}

/// Completion offers what may stand before the cursor: names, the input kinds after
/// `input NAME =`, registry names in the string of `study("`, and the input ids and output titles
/// of the innermost study call; nothing in a comment, another string, after a dot or a number.
#[test]
fn completion_follows_the_cursor() {
    let reference = Reference::new(&IndicatorFactory::new());
    // Past the end of every text, so at its end.
    let end = usize::MAX;
    for (text, cursor, expected) in [
        ("plot(close)", 2, &["plot"][..]),
        // Inside the two bytes of the e with an acute accent, so before it.
        ("pl\u{e9}", 3, &["plot"]),
        ("pl", 0, &[]),
        ("plot ", end, &[]),
        ("", 0, &[]),
        ("study(\"RS", end, &["RSI", "RSI Divergence Indicator"]),
        ("study(\"RSI\"", end, &[]),
        ("study(\"RSI\", per", end, &["period"]),
        ("study(\"RSI\", \"per", end, &["period"]),
        ("f(study(\"RSI\"), per", end, &[]),
        (
            "study(\"RSI\", source = study(\"MACD\", fa",
            end,
            &["fast_period"],
        ),
        ("study(\"RSI\", period = sm", end, &["sma"]),
        ("study(\"BB\", output = \"BB L", end, &["BB Lower"]),
        ("ro", end, &["round"]),
        ("input n = le", end, &["length"]),
        ("// sm", end, &[]),
        ("/* x */ pl", end, &["plot"]),
        ("plot(\"sm", end, &[]),
        ("close.cl", end, &[]),
        ("2pl", end, &[]),
    ] {
        let found = reference.complete(text, cursor);
        let names: Vec<_> = found.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, expected, "{text:?} at {cursor}");
    }
}

/// The classified pieces of a text are its keywords, declarations, built-in names, numbers,
/// strings with their quotes and comments, in order and on character boundaries.
#[test]
fn tokens_cover_the_text_in_order() {
    use TokenClass::*;
    for (text, expected) in [
        (
            "input n = int(\"N\", 14, 1, 50);",
            &[
                ("input", Declaration),
                ("\"N\"", Text),
                ("14", Number),
                ("1", Number),
                ("50", Number),
            ][..],
        ),
        (
            "plot(\"x\", sma(close, 14)); // note",
            &[
                ("plot", Declaration),
                ("\"x\"", Text),
                ("sma", Builtin),
                ("close", Builtin),
                ("14", Number),
                ("// note", Comment),
            ],
        ),
        (
            "if x > 1.5 { let y = na; }",
            &[
                ("if", Keyword),
                ("1.5", Number),
                ("let", Keyword),
                ("na", Builtin),
            ],
        ),
        (
            "bars day = history(\"\", \"1D\"); plot(\"c\", day.close[1]);",
            &[
                ("bars", Declaration),
                ("\"\"", Text),
                ("\"1D\"", Text),
                ("plot", Declaration),
                ("\"c\"", Text),
                ("close", Builtin),
                ("1", Number),
            ],
        ),
        (
            "let e = \"\u{e9}\";",
            &[("let", Keyword), ("\"\u{e9}\"", Text)],
        ),
        // Unterminated, so to the end of the text.
        ("let s = \"open", &[("let", Keyword), ("\"open", Text)]),
        ("/* a\nb", &[("/* a\nb", Comment)]),
        ("", &[]),
    ] {
        let found = tokens(text);
        assert!(
            found
                .windows(2)
                .all(|pair| pair[0].0.end <= pair[1].0.start),
            "{text:?}: {found:?}"
        );
        assert!(
            found
                .iter()
                .all(|(range, _)| text.is_char_boundary(range.start)
                    && text.is_char_boundary(range.end)),
            "{text:?}: {found:?}"
        );
        let found: Vec<_> = found
            .into_iter()
            .map(|(range, class)| (&text[range], class))
            .collect();
        assert_eq!(found, expected, "{text:?}");
    }
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
        // The same past a declaration the bars run without, which keeps its place.
        (
            "state s = 0; plot(\"x\", if bar_index == 20 { close[-1] } else { close });",
            50,
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
        // A condition is true or false; the call's name.
        (
            "plot(\"x\", if bar_index < 20 { close } else { alert(\"Up\", close); close });",
            46,
            20,
            "alert \"Up\" takes true or false; got series",
        ),
        // A colour expression, which a string written in the call does not start.
        (
            "plot(\"x\", close, color = if bar_index < 20 { \"#FF0000\" } else { \"red\" });",
            26,
            20,
            "the color of plot \"x\" is \"#RRGGBB\", \"#RRGGBBAA\", rgb(r, g, b), \
             rgba(r, g, b, alpha) or na; got \"red\"",
        ),
        (
            "plot(\"x\", close, color = if bar_index < 20 { na } else { 1.0 });",
            26,
            20,
            "got number",
        ),
        // A drawing's bar, text and words; the call's name.
        (
            "plot(\"x\", close); if bar_index == 20 { label(\"x\", bar_index - 21, close, \"t\"); }",
            40,
            20,
            "label bar -1 is before the first bar",
        ),
        (
            "plot(\"x\", close); if bar_index == 20 { label(\"x\", bar_index + 0.5, close, \"t\"); }",
            40,
            20,
            "label bar 20.5 is not a whole number",
        ),
        (
            "plot(\"x\", close); if bar_index == 20 { label(\"x\", bar_index + 10001, close, \"t\"); }",
            40,
            20,
            "label bar 10021 is more than 10000 bars after the bar calculated",
        ),
        (
            "plot(\"x\", close); if bar_index == 20 { label(\"x\", bar_index, close, \"t\", \"left\"); }",
            40,
            20,
            "label placement \"left\" is \"above\", \"below\", \"above_left\" or \"above_right\"",
        ),
        (
            "plot(\"x\", close); if bar_index == 20 { label(\"x\", bar_index, close, 5); }",
            40,
            20,
            "label text is a string; got int",
        ),
        (
            "plot(\"x\", close); if bar_index == 20 { marker(bar_index, close, \"sideways\"); }",
            40,
            20,
            "marker direction \"sideways\" is \"up\" or \"down\"",
        ),
        // Calls the compile check does not read as drawings: a method, and a call through a
        // function pointer, whose title is checked when it runs.
        (
            "plot(\"x\", close); if bar_index == 20 { \"x\".label(bar_index, close, \"t\"); }",
            0,
            20,
            "label draws only when called as label(...) in a script that defines no function label",
        ),
        (
            "plot(\"x\", close); if bar_index == 20 { call(Fn(\"label\"), \"y\", bar_index, close, \"t\"); } \
             if false { label(\"x\", 0, close, \"t\"); }",
            0,
            20,
            "no plot \"y\" is declared",
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

#[test]
fn alert_conditions_follow_the_last_call_on_each_bar() {
    let prices = [(100.0, 101.0), (101.0, 102.0), (102.0, 99.0), (99.0, 103.0)];
    let script = compile(
        "alert(\"Up\", close > open);\n\
         if bar_index == 1 { alert(\"Up\", false); }\n\
         if bar_index >= 2 { alert(\"Late\", close > 100.0); }\n\
         plot(\"Close\", close);",
    );
    let conditions = |study: &CustomIndicator, bars: usize| -> Vec<Vec<Option<bool>>> {
        (0..2)
            .map(|condition| {
                (0..bars)
                    .map(|bar| study.alert_condition(condition, bar))
                    .collect()
            })
            .collect()
    };
    let (yes, no) = (Some(true), Some(false));
    let mut study = calculated(&script, &opens_closes(&prices));
    assert_eq!(study.alert_conditions(), ["Up", "Late"]);
    // Bar 1 calls Up with true, then false; Late runs from bar 2, where 99 is not above 100.
    assert_eq!(
        conditions(&study, 4),
        [vec![yes, no, no, yes], vec![None, None, no, yes]]
    );
    assert_eq!(study.alert_condition(2, 0), None);
    assert_eq!(study.alert_condition(0, 4), None);
    // A new bar, then the same bar as it forms further: each as a full calculation gives.
    for (bar, up, late) in [((103.0, 104.0), yes, yes), ((103.0, 102.0), no, yes)] {
        let bars = opens_closes(&[prices.as_slice(), &[bar]].concat());
        study.update(&bars, 4);
        settle(&mut study, &bars);
        let fresh = calculated(&script, &bars);
        assert_eq!(conditions(&study, 5), conditions(&fresh, 5), "{bar:?}");
        let last = (study.alert_condition(0, 4), study.alert_condition(1, 4));
        assert_eq!(last, (up, late), "{bar:?}");
    }
    // `alert` stays free as a name, and a script's own function is called.
    for (source, plot) in [
        (
            "let alert = close > open; plot(\"x\", if alert { 1.0 } else { 0.0 });",
            [1.0, 1.0, 0.0, 1.0],
        ),
        (
            "fn alert(m, c) { 7.0 } plot(\"x\", alert(\"m\", true));",
            [7.0; 4],
        ),
        (
            "input alert = bool(\"Alert\", true); let m = #{ alert: 2.0 }; plot(\"x\", m.alert);",
            [2.0; 4],
        ),
    ] {
        let study = calculated(&compile(source), &opens_closes(&prices));
        assert!(study.alert_conditions().is_empty(), "{source}");
        assert_eq!(study.values().line(0), plot, "{source}");
    }
}

/// Items 1 and 2 of "Per-bar colours in scripts" on #3631: a plot's colour follows an expression
/// on each bar, the plot's own colour standing where it gives na, and the bar and background
/// colour styles paint nothing there.
#[test]
fn colours_follow_their_expressions_on_each_bar() {
    let prices = [(100.0, 101.0), (101.0, 102.0), (102.0, 99.0), (99.0, 103.0)];
    let script = compile(
        "display(overlay = true);\n\
         input up = color(\"Up\", \"#26A69A\");\n\
         plot(\"Close\", close, color = if close > open { up } else if bar_index == 2 { rgb(300, -5, 12.6) } else { na });\n\
         plot(\"Bars\", close, style = \"bar_color\", color = if close > open { \"#2962FF80\" } else { na });\n\
         plot(\"Shade\", 1.0, style = \"background_color\", color = rgba(255, 0, 0, 0.5));\n\
         plot(\"Fixed\", open, color = \"#FF9800\");",
    );
    let colors = |study: &CustomIndicator, line: usize| -> Vec<Option<Color32>> {
        let bars = study.values().len();
        (0..bars).map(|bar| study.plot_color(line, bar)).collect()
    };
    let up = Some(Color32::from_rgb(0x26, 0xA6, 0x9A));
    // Bar 2 falls: 300 is held to 255, -5 to 0, and 12.6 rounds to 13.
    let held = Some(Color32::from_rgb(255, 0, 13));
    let blue = Some(Color32::from_rgba_unmultiplied(0x29, 0x62, 0xFF, 0x80));
    let bars = opens_closes(&prices);
    let mut study = calculated(&script, &bars);
    assert_eq!(colors(&study, 0), [up, up, held, up]);
    assert_eq!(colors(&study, 1), [blue, blue, None, blue]);
    assert!(same(
        study.values().line(1),
        &[101.0, 102.0, f64::NAN, 103.0]
    ));
    // Half of 255 rounds to 128.
    let shade = Some(Color32::from_rgba_unmultiplied(255, 0, 0, 128));
    assert_eq!(colors(&study, 2), [shade; 4]);
    assert_eq!(study.values().line(2), [1.0; 4]);
    assert_eq!(colors(&study, 3), [None; 4]);
    assert_eq!(
        study.plot_style(3).color,
        Color32::from_rgb(0xFF, 0x98, 0x00)
    );
    assert_eq!(study.plot_style(1).plot_type, PlotType::BarColor);
    assert_eq!(study.plot_style(2).plot_type, PlotType::BackgroundColor);

    let mine = Some(Color32::from_rgb(1, 2, 3));
    (study.set_input("up", InputValue::Color(Color32::from_rgb(1, 2, 3)))).unwrap();
    study.update(&bars, 0);
    settle(&mut study, &bars);
    assert_eq!(colors(&study, 0), [mine, mine, held, mine]);
    // A falling bar after them has no colour, and no bar colour value.
    let bars = opens_closes(&[prices.as_slice(), &[(103.0, 102.0)]].concat());
    study.update(&bars, 4);
    settle(&mut study, &bars);
    assert_eq!(colors(&study, 0), [mine, mine, held, mine, None]);
    assert_eq!(colors(&study, 1), [blue, blue, None, blue, None]);
    assert!(study.values().line(1)[4].is_nan());
}

/// Labels, lines, bands and markers follow the calls that make them, on the bars they name: the
/// labels in order of their bar, and the drawings of a forming bar made again with each change.
#[test]
fn drawings_follow_their_calls() {
    let prices = [(100.0, 101.0), (101.0, 102.0), (102.0, 99.0), (99.0, 103.0)];
    let script = compile(
        "plot(\"Marks\", na, color = \"#FF6D00\");\n\
         plot(\"Close\", close);\n\
         if close < open { label(\"Marks\", bar_index - 1, high[1], \"Top\"); }\n\
         if bar_index == 3 {\n\
             label(\"Close\", bar_index, low, \"Low\", \"below\");\n\
             label(\"Marks\", 0, open[3], \"First \" + rgb(255, 0, 13));\n\
             segment(\"Marks\", 0, low[3], bar_index, high);\n\
             zone(\"Marks\", bar_index, 1, low[2], high[1]);\n\
             marker(bar_index, low, \"up\");\n\
         }\n\
         marker(bar_index, na, \"down\");\n\
         label(\"Marks\", na, close, \"none\");",
    );
    let label = |bar, price, text: &str, placement, plot| StudyLabel {
        bar,
        price,
        text: text.into(),
        placement,
        plot,
    };
    // Bar 2 falls from 102 to 99 and labels bar 1 at its high; bar 3 labels its low and bar 0's
    // open, which comes first, made last, with a colour written as text.
    let first = label(0, 100.0, "First #FF000DFF", LabelPlacement::Above, 0);
    let top = label(1, 102.0, "Top", LabelPlacement::Above, 0);
    let low = label(3, 99.0, "Low", LabelPlacement::Below, 1);
    let markers = |study: &CustomIndicator| -> Vec<(usize, f64, CrossDirection)> {
        let markers = study.markers().iter();
        markers
            .map(|marker| (marker.bar, marker.value, marker.direction))
            .collect()
    };
    let mut study = calculated(&script, &opens_closes(&prices));
    let made = vec![first, top, low];
    assert_eq!(study.labels(), made);
    // From bar 0's low to bar 3's high.
    let segment = StudySegment {
        start_bar: 0,
        start_price: 100.0,
        end_bar: 3,
        end_price: 103.0,
        plot: 0,
    };
    assert_eq!(study.price_segments(), [segment]);
    // Bars 3 and 1 in order, between bar 1's low and bar 2's high.
    let zone = StudyZone {
        bars: 1..4,
        top: 102.0,
        bottom: 101.0,
        plot: 0,
    };
    assert_eq!(study.zones(), std::slice::from_ref(&zone));
    assert_eq!(markers(&study), [(3, 99.0, CrossDirection::Up)]);
    // A new bar, then the same bar as it forms further: falling, it labels bar 3 at its high,
    // after the label bar 3 made itself, and rising again it takes that label back.
    let later = [
        made.clone(),
        vec![label(3, 103.0, "Top", LabelPlacement::Above, 0)],
    ]
    .concat();
    for (bar, labels) in [
        ((103.0, 104.0), &made),
        ((103.0, 102.0), &later),
        ((103.0, 104.0), &made),
    ] {
        let bars = opens_closes(&[prices.as_slice(), &[bar]].concat());
        study.update(&bars, 4);
        settle(&mut study, &bars);
        assert_eq!(study.labels(), *labels, "{bar:?}");
        assert_eq!(study.price_segments(), [segment], "{bar:?}");
        assert_eq!(study.zones(), std::slice::from_ref(&zone), "{bar:?}");
        assert_eq!(markers(&study), [(3, 99.0, CrossDirection::Up)], "{bar:?}");
    }
}

/// A script that draws says so before it runs, so the chart offers no input that would leave its
/// drawings behind: no Offset with any drawing, and no Symbol, Interval or Wait for interval close
/// with labels or lines. A name that no call of a helper uses draws nothing.
#[test]
fn drawing_scripts_answer_the_chart_contracts() {
    let bars = opens_closes(&[(100.0, 101.0), (101.0, 99.0)]);
    for (source, only_plots, other_bars) in [
        ("plot(\"x\", close);", true, true),
        // Colours move with their plot.
        (
            "display(overlay = true);\n\
             plot(\"b\", close, style = \"bar_color\", color = if close > open { \"#2962FF\" } else { na });",
            true,
            true,
        ),
        (
            "plot(\"x\", close);\nzone(\"x\", bar_index, bar_index, high, low);",
            false,
            true,
        ),
        (
            "plot(\"x\", close);\nmarker(bar_index, low, \"up\");",
            false,
            true,
        ),
        (
            "plot(\"x\", close);\nlabel(\"x\", bar_index, high, \"t\");",
            false,
            false,
        ),
        (
            "plot(\"x\", close);\nsegment(\"x\", 0, close, bar_index, close);",
            false,
            false,
        ),
        // The names stay free, and a script's own function is called.
        ("let label = close; plot(\"x\", label);", true, true),
        ("fn zone(a) { a * 2.0 } plot(\"x\", zone(1.0));", true, true),
        (
            "input marker = bool(\"M\", true); let m = #{ segment: 3.0 }; plot(\"x\", m.segment);",
            true,
            true,
        ),
        ("fn label(a) { a } plot(\"x\", label(close));", true, true),
    ] {
        let script = compile(source);
        assert_eq!(script.check(&bars), Ok(()), "{source}");
        let study = ConfiguredStudy::new(Box::new(script.study()));
        let answers = (study.draws_only_plots(), study.draws_on_other_bars());
        assert_eq!(answers, (only_plots, other_bars), "{source}");
        let ids: Vec<_> = study.inputs().into_iter().map(|input| input.id).collect();
        let offered = |id: &str| ids.iter().any(|offered| offered == id);
        assert_eq!(offered("plot_offset"), only_plots, "{source}");
        for id in [
            "calculation_symbol",
            "calculation_timeframe",
            "wait_for_close",
        ] {
            assert_eq!(offered(id), other_bars, "{source}: {id}");
        }
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
    // A label of 9,000 bytes on every bar, kept by the study once its bar is done.
    let drawn = "plot(\"x\", close);\nlet t = \"x\"; t.pad(9000, \"y\");\n\
                 label(\"x\", bar_index, close, t);";
    // A map of 100 keys of about 10,000 bytes, which the size limits admit since they do not count
    // keys, and which counts 1,000,790 bytes.
    let map =
        "let k = \"x\"; k.pad(9990, \"y\");\nlet m = #{}; for i in 0..100 { m[k + i] = 1; }\n";
    // Two copies of the map at each call level, counted at the first read of each call; fails if a
    // call is counted only after a clock read.
    let deep = format!(
        "{map}fn down(n, a0, a1) {{ if n > 0 {{ down(n - 1, a0, a1) }} else {{ 0 }} }}\n\
         plot(\"x\", down(30, m, m));"
    );
    // Copies of the map bound after a block released 20 counted variables, each counted at the
    // next read; fails if they wait for a clock read while `spin` runs, or if they are taken for
    // the released variables.
    let released: String = (0..20).map(|i| format!("let a{i} = 0; ")).collect();
    let copies: String = (0..20).map(|i| format!("let b{i} = m; ")).collect();
    let bound = format!(
        "{map}{{ {released}a0 += a19; }}\n{copies}\n\
         fn spin(x) {{ let z = 0; while z < 100000000 {{ z += 1; }} z }}\nplot(\"x\", spin(0));"
    );
    // Calls written `f!()` run in their caller's scope: `hold` holds a state map of 2.5 MB, counted
    // once, under the limit, and `grow` grows it past the limit, counted while it runs, before it
    // empties it; the state's declaration holds the most. Fails at line 7 if a call counts its
    // caller's values again, and with no stop if it does not count those it grows.
    let shared = "state m = 0;\nlet k = \"x\"; k.pad(9990, \"y\");\n\
                  fn hold() { let z = 0; while z < 300 { z += 1; } m.len() }\n\
                  fn grow(x) { for i in 0..1000 { m[x + i] = 1; } let n = m.len(); m = #{}; n }\n\
                  m = #{}; for i in 0..250 { m[k + i] = 1; }\n\
                  let w = 0; while w < 300 { w += 1; }\n\
                  let h = hold!();\nplot(\"x\", h + grow!(k));";
    // The time limits are about 20 times what a row takes in a debug build, so a busy machine
    // stays clear of them and a row whose memory limit stops working fails in seconds.
    for (script, fixture, mib, line, full) in [
        (source.as_str(), &fixture[..], 16, 1, 10),
        (windows, &fixture[..1000], 1, 2, 10),
        (kept, &fixture[..1000], 1, 1, 10),
        (room, &fixture[..300], 64, 1, 10),
        (bar, &fixture[..1], 16, 2, 10),
        (cut, &fixture[..1], 2, 2, 10),
        (levels, &fixture[..1], 8, 5, 10),
        (studies, &fixture[..1000], 1, 2, 10),
        (drawn, &fixture[..1000], 8, 3, 25),
        (deep.as_str(), &fixture[..1], 16, 4, 10),
        (bound.as_str(), &fixture[..1], 16, 4, 10),
        (shared, &fixture[..1], 4, 1, 10),
    ] {
        let mut capped = limited(
            script,
            Limits {
                bar: Duration::from_secs(10),
                full: Duration::from_secs(full),
                memory: mib << 20,
            },
        )
        .study();
        capped.calculate(fixture);
        assert_eq!(
            stopped(settle(&mut capped, fixture)),
            format!("memory limit of {mib} MiB reached at line {line}")
        );
    }
    // The default memory limit; 64 series on 100,000 bars take one to three minutes in a debug
    // build on a busy machine, past the default limit for a full recompute.
    let mut study = limited(
        &source,
        Limits {
            full: Duration::from_secs(600),
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

/// The profile of a full recalculation counts each line's runtime operations, the helper call
/// and the plot here, once a bar, and charges the arithmetic between them to the next one. An
/// update of the last bar and a study of no bars keep it, and the next full recalculation counts
/// afresh.
#[test]
fn profile_counts_the_calls_on_each_line_of_a_full_recalculation() {
    let source = "let a = sma(close, 5);\nlet b = 1.0 + 2.0;\nplot(\"x\", a);";
    let mut fixture = bars(100, 10);
    let script = compile(source).with_profiling(true);
    let mut study = calculated(&script, &fixture);
    let profile = script
        .profile()
        .expect("a profile of the full recalculation");
    let calls = |profile: &Profile| -> Vec<_> {
        (profile.lines().iter())
            .map(|&(line, calls, _)| (line, calls))
            .collect()
    };
    assert_eq!(calls(&profile), [(1, 100), (3, 100)]);
    assert!(
        profile
            .lines()
            .iter()
            .all(|&(.., time)| time > Duration::ZERO)
    );

    fixture[99].close += 1.0;
    study.update(&fixture, 99);
    settle(&mut study, &fixture);
    // A layout check builds a study and calculates it on no bars.
    calculated(&script, &[]);
    assert_eq!(script.profile().as_ref(), Some(&profile));
    fixture[0].close += 1.0;
    study.update(&fixture, 0);
    settle(&mut study, &fixture);
    let profile = script
        .profile()
        .expect("a profile of the second recalculation");
    assert_eq!(calls(&profile), [(1, 100), (3, 100)]);

    let unprofiled = compile(source);
    calculated(&unprofiled, &fixture);
    assert_eq!(unprofiled.profile(), None);
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

/// The valid bars of the backtester as a series: 1 for a bar of finite prices whose low and high
/// hold its open and close, 0 otherwise.
const VALID: &str = "series valid = if is_na(open) || is_na(high) || is_na(low) || is_na(close)
    || low > min(open, close) || high < max(open, close) { 0.0 } else { 1.0 };\n";

/// "Consecutive Up/Down" with its counts kept in states.
const CONSECUTIVE_STATES: &str = r#"input length = length("Consecutive closes", 3, 1, 1000);
state ups = 0;
state downs = 0;
if close > close[1] { ups += 1; } else { ups = 0; }
if close < close[1] { downs += 1; } else { downs = 0; }
if !after_fill() {
    if ups >= length { entry("Long", "long"); }
    if downs >= length { entry("Short", "short"); }
}"#;

fn backtest() -> egui_charts::strategies::Backtest {
    egui_charts::strategies::Backtest::new("Run", "TEST", Timeframe::Min1, 10_000.0)
}

/// Strategies restated as scripts trade as the library's strategies do on the same bars and
/// settings, through reruns after fills and historical tick decisions: the same orders, prices,
/// quantities and input properties.
#[test]
fn script_strategies_match_library_strategies() {
    use egui_charts::strategies::{BacktestSettings, Strategy, StrategyFactory};
    let consecutive = format!(
        "input length = length(\"Consecutive closes\", 3, 1, 1000);\n{VALID}\
         series up = if close > close[1] {{ 1.0 }} else {{ 0.0 }};
         series down = if close < close[1] {{ 1.0 }} else {{ 0.0 }};
         let whole = sum(valid, length + 1) == length + 1;
         let rising = sum(up, length) == length;
         let falling = sum(down, length) == length;
         if !after_fill() && whole {{
             if rising {{ entry(\"Long\", \"long\"); }}
             if falling {{ entry(\"Short\", \"short\"); }}
         }}"
    );
    let channel = format!(
        "input length = length(\"Channel length\", 20, 1, 1000);\n{VALID}\
         let top = highest(high, length);
         let bottom = lowest(low, length);
         let whole = sum(valid, length) == length;
         if !after_fill() && whole {{
             entry(\"Long\", \"long\", #{{ stop: top }});
             entry(\"Short\", \"short\", #{{ stop: bottom }});
         }}"
    );
    let volatility = r#"input length = length("ATR length", 14, 1, 1000);
        input multiplier = float("ATR multiplier", 2.0, 0.0, 1000000.0);
        let range = if bar_index == 0 { high - low }
            else { max(max(high - low, abs(high - close[1])), abs(low - close[1])) };
        let atr = rma(range, length);
        if !after_fill() && !is_na(atr) {
            let distance = atr * multiplier;
            entry("Long", "long", #{ stop: close + distance });
            entry("Short", "short", #{ stop: close - distance });
        }"#;
    let mut after_fills = BacktestSettings::default();
    after_fills.recalculate_after_fills = true;
    let mut on_history = BacktestSettings::default();
    on_history.recalculate_on_history = true;
    let mut rows = Vec::new();
    for fixture in [bars(3000, 7), with_gaps(3000, 7)] {
        for settings in [
            BacktestSettings::default(),
            after_fills.clone(),
            on_history.clone(),
        ] {
            let name = "Consecutive Up/Down";
            rows.push((name, consecutive.as_str(), fixture.clone(), settings, None));
        }
    }
    // Reruns after fills restore the state before the bar, so no rise counts twice.
    rows.extend([
        (
            "Consecutive Up/Down",
            CONSECUTIVE_STATES,
            with_gaps(3000, 7),
            after_fills,
            None,
        ),
        (
            "Channel Breakout",
            channel.as_str(),
            bars(3000, 8),
            BacktestSettings::default(),
            None,
        ),
        // Stops twice the average range away rarely fill on this walk; half of it does.
        (
            "Volatility Expansion Close",
            volatility,
            bars(3000, 9),
            BacktestSettings::default(),
            Some(("multiplier", InputValue::Number(0.5))),
        ),
    ]);
    for (name, source, fixture, settings, input) in rows {
        let mut engine = backtest();
        engine.settings = settings;
        let mut native = StrategyFactory::new().create(name).unwrap();
        let mut script = compile(source).strategy();
        if let Some((id, value)) = input {
            native.set_input(id, value.clone()).unwrap();
            script.set_input(id, value).unwrap();
        }
        let expected = engine.run(native.as_mut(), &fixture, &[]);
        let report = engine.run(&mut script, &fixture, &[]);
        assert!(!expected.trades.is_empty(), "{name}");
        // Debug text, since a missing value in a report differs from itself.
        let (report, expected) = (format!("{report:?}"), format!("{expected:?}"));
        assert!(report == expected, "{name}: {source}\n{report}\n{expected}");
    }
}

/// A script strategy continued bar by bar reports what a fresh run on the same bars reports, its
/// states kept from the decisions that set them, and through a corrected forming bar and back
/// what the library's strategy reports under the same calls.
#[test]
fn script_strategies_continue_as_fresh_runs() {
    use egui_charts::strategies::StrategyFactory;
    // The bars held count only while the position each decision read was open.
    let holding = "state held = 0;
        if position() > 0.0 { held += 1; } else { held = 0; }
        if position() == 0.0 && close > close[1] { entry(\"Long\", \"long\"); }
        if held >= 3 { exit_order(\"Out\"); }";
    let fixture = bars(300, 7);
    let mut continued = backtest();
    let mut script = compile(holding).strategy();
    for n in 200..=300 {
        continued.update(&mut script, &fixture[..n], &[]);
        let fresh = backtest().run(&mut compile(holding).strategy(), &fixture[..n - 1], &[]);
        assert!(fresh.trades.len() > 10);
        // Debug text, since a missing value in a report differs from itself.
        let (continued, fresh) = (
            format!("{:?}", continued.report()),
            format!("{:?}", Some(fresh)),
        );
        assert!(continued == fresh, "{n} bars\n{continued}\n{fresh}");
    }

    let fixture = bars(3000, 7);
    let mut corrected = fixture.clone();
    let last = corrected.last_mut().unwrap();
    last.close = last.open;
    let mut steps: Vec<&[Bar]> = (2000..=3000).map(|n| &fixture[..n]).collect();
    steps.extend([corrected.as_slice(), fixture.as_slice()]);
    let mut native = StrategyFactory::new()
        .create("Consecutive Up/Down")
        .unwrap();
    let mut script = compile(CONSECUTIVE_STATES).strategy();
    let (mut expected, mut run) = (backtest(), backtest());
    for engine in [&mut expected, &mut run] {
        engine.settings.recalculate_on_every_update = true;
    }
    for bars in steps {
        expected.update(native.as_mut(), bars, &[]);
        run.update(&mut script, bars, &[]);
        assert_eq!(run.report(), expected.report(), "{} bars", bars.len());
    }
    assert!(expected.report().unwrap().trades.len() > 10);
}

/// A decision that runs earlier bars again runs each with the readings it was last decided with,
/// so a state that reads them is rebuilt as it was: after a fill on a forming bar decided on
/// before, after a called study revised earlier bars, and for a declared history, which runs the
/// previous bar again.
#[test]
fn bars_run_again_keep_the_readings_they_were_decided_with() {
    let flat_count = "state flat = 0;
        if position() == 0.0 { flat += 1; }
        if after_fill() {
            if position() > 0.0 { exit_order(\"Out\", #{ qty: flat }); }
        } else if bar_index == 2 && position() == 0.0 {
            entry(\"Long\", \"long\", #{ qty: 10 });
        }";
    let minute = |i: i64, close: f64| {
        let time = chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(i);
        Bar::new(time, 100.0, close.max(100.0), 100.0, close, 1.0)
    };
    let mut live = backtest();
    live.settings.recalculate_on_every_update = true;
    live.settings.recalculate_after_fills = true;
    let mut strategy = compile(flat_count).strategy();
    // Bars 0 and 1 are decided flat and bar 2 last with the position open, so every exit closes
    // 2. The entry of bar 2 fills at the forming bar's open, 100, and each exit at the next price
    // the forming bar reaches: 100, then 101 and 102 on the next updates.
    let fills: Vec<Vec<_>> = [100.0, 101.0, 102.0]
        .into_iter()
        .map(|close| {
            let bars: Vec<_> = (0..3)
                .map(|i| minute(i, 100.0))
                .chain([minute(3, close)])
                .collect();
            (live.update(&mut strategy, &bars, &[]).iter())
                .map(|fill| (fill.order_id.clone(), fill.quantity, fill.price))
                .collect()
        })
        .collect();
    let out = |price| ("Out".to_owned(), 2.0, price);
    assert_eq!(
        fills,
        [
            vec![("Long".to_owned(), 10.0, 100.0), out(100.0)],
            vec![out(101.0)],
            vec![out(102.0)],
        ]
    );

    // The entry of bar 30 fills at bar 31's open, so bars 0 to 30 are decided flat and the exit
    // of bar 200 closes 31 of the 50, whatever the script declares and never reads.
    let held = "state flat = 0;
        if position() == 0.0 { flat += 1; }
        if bar_index == 30 && position() == 0.0 { entry(\"Long\", \"long\", #{ qty: 50 }); }
        if bar_index == 200 { exit_order(\"Out\", #{ qty: flat }); }";
    let fixture = bars(260, 7);
    for unread in [
        "",
        "let zz = study(\"ZigZag\");\n",
        "bars day = history(\"\", \"1D\");\n",
    ] {
        let report = backtest().run(
            &mut compile(&format!("{unread}{held}")).strategy(),
            &fixture,
            &[],
        );
        let closed: Vec<_> = (report.trades.iter())
            .filter(|trade| trade.exit.is_some())
            .map(|trade| trade.quantity)
            .collect();
        assert_eq!(closed, [31.0], "{unread}");
    }
}

/// Orders and readings reach the backtester as the script calls them, checked by hand on six
/// bars; an error or a limit ends the orders and withholds the report with its line and bar.
#[test]
fn script_orders_and_readings_reach_the_backtest() {
    // (open, high, low, close) of six one-minute bars.
    let ladder: Vec<_> = [
        (100.0, 100.0, 100.0, 100.0),
        (100.0, 101.0, 100.0, 101.0),
        (103.0, 105.0, 103.0, 104.0),
        (104.0, 107.0, 104.0, 106.0),
        (106.0, 106.0, 101.0, 101.0),
        (101.0, 101.0, 100.0, 100.0),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (open, high, low, close))| {
        let time = chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(i as i64);
        Bar::new(time, open, high, low, close, 0.0)
    })
    .collect();
    let momentum = "if close > close[1] && position() == 0.0 { entry(\"Long\", \"long\", \
                    #{ qty: equity() / 1000.0 }); } else if close < close[1] && position() > 0.0 \
                    { exit_order(\"Close\"); }";
    let stop = "if bar_index == 1 { entry(\"Long\", \"long\", #{ qty: 10, stop: 106.0 }); }";
    let cancelled = format!("{stop}\nif bar_index == 2 {{ cancel(\"Long\"); }}");
    let busy = "if bar_index == 1 { entry(\"Long\", \"long\", #{ qty: 10 }); }\n\
                if bar_index == 3 { let i = 0; loop { i += 1; } }\n\
                if bar_index == 4 { exit_order(\"Close\"); }";
    let bar_limit = Limits {
        bar: Duration::from_millis(50),
        ..Limits::default()
    };
    let no_time = Limits {
        full: Duration::ZERO,
        ..Limits::default()
    };
    // The trades as entry price, quantity and the exit's price and profit, or the line and the
    // end of the error that withholds them.
    type Trades = Vec<(f64, f64, Option<(f64, f64)>)>;
    for (source, limits, expected) in [
        // Bar 1 rises with 10,000 flat: 10 fill at bar 2's open, 103; bar 4 falls, so the exit
        // fills at bar 5's open, 101.
        (
            momentum,
            Limits::default(),
            Ok::<Trades, (usize, &str)>(vec![(103.0, 10.0, Some((101.0, -20.0)))]),
        ),
        // Bar 2's path 103, 103, 105, 104 reaches the take profit 104 on the way up.
        (
            "if bar_index == 1 { entry(\"Long\", \"long\", #{ qty: 10 }); \
             exit_relative(\"Bracket\", \"long\", 1.0, 50.0); }",
            Limits::default(),
            Ok(vec![(103.0, 10.0, Some((104.0, 10.0)))]),
        ),
        // Bar 2's high 105 misses the stop; bar 3's path 104, 104, 107 crosses it.
        (stop, Limits::default(), Ok(vec![(106.0, 10.0, None)])),
        (&cancelled, Limits::default(), Ok(vec![])),
        // Lows 103, 104 and 101 stay above the limit; bar 5's path 101, 101, 100 reaches it.
        (
            "if bar_index == 1 { entry(\"Long\", \"long\", #{ qty: 10, limit: 100.5 }); }",
            Limits::default(),
            Ok(vec![(100.5, 10.0, None)]),
        ),
        (
            "entry(\"Long\", \"up\");",
            Limits::default(),
            Err((
                1,
                "column 1: side takes \"long\" or \"short\"; got \"up\", on bar 0",
            )),
        ),
        (
            "entry(\"Long\", \"long\", #{ size: 1 });",
            Limits::default(),
            Err((
                1,
                "column 1: entry takes the options qty, limit and stop; got size, on bar 0",
            )),
        ),
        (
            "entry(\"Long\", \"long\", #{ qty: true });",
            Limits::default(),
            Err((1, "column 1: qty takes a number; got bool, on bar 0")),
        ),
        // The entry of bar 1 filled before the stop at bar 3, which withholds the report. The
        // column is that of the operation the clock was read at.
        (
            busy,
            bar_limit,
            Err((2, ": time limit of 50 ms for one bar reached, on bar 3")),
        ),
        (
            momentum,
            no_time,
            Err((
                0,
                "time limit of 0 ms for a full recompute reached, on bar 0",
            )),
        ),
    ] {
        let script = compile(source).with_limits(limits);
        assert!(script.places_orders(), "{source}");
        let report = backtest().run(&mut script.strategy(), &ladder, &[]);
        let trades: Trades = (report.trades.iter())
            .map(|trade| {
                let exit = trade.exit.as_ref().map(|exit| (exit.price, exit.profit));
                (trade.entry_price, trade.quantity, exit)
            })
            .collect();
        match expected {
            Ok(expected) => {
                assert_eq!(trades, expected, "{source}");
                assert_eq!(report.unavailable, None, "{source}");
                assert_eq!(script.last_error(), None, "{source}");
            }
            Err((line, end)) => {
                assert_eq!(trades, [], "{source}");
                assert!(report.equity.is_empty(), "{source}");
                let error = script.last_error().expect("the error is reported");
                assert_eq!(report.unavailable, Some(error.to_string()), "{source}");
                assert_eq!(error.line, line, "{source}: {error}");
                assert!(error.to_string().ends_with(end), "{source}: {error}");
            }
        }
    }

    // A live run's updates each get the full time limit, so decisions whose summed time passes
    // it over many updates never stop the strategy.
    let script = compile("let i = 0; while i < 300 { i += 1; }\nentry(\"Long\", \"long\");")
        .with_limits(Limits {
            full: Duration::from_millis(20),
            ..Limits::default()
        });
    let mut strategy = script.strategy();
    let mut live = backtest();
    let bars: Vec<_> = (0..200)
        .map(|i| {
            let time = chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(i);
            Bar::new(time, 100.0, 101.0, 99.0, 100.0, 0.0)
        })
        .collect();
    for end in 1..=bars.len() {
        live.update(&mut strategy, &bars[..end], &[]);
    }
    assert_eq!(script.last_error(), None);
    assert_eq!(live.report().unwrap().unavailable, None);

    // With a decision after each fill, the entry fills at bar 2's open, 103, and the decision
    // right after it reruns bar 1 at that price, placing a take profit at 104, which the path to
    // 105 reaches.
    let price_exit = compile(
        "if bar_index == 1 { if after_fill() { exit_order(\"Out\", #{ profit: price() + 1.0 }); \
         } else { entry(\"Long\", \"long\", #{ qty: 10 }); } }",
    );
    let mut engine = backtest();
    engine.settings.recalculate_after_fills = true;
    let report = engine.run(&mut price_exit.strategy(), &ladder, &[]);
    let [trade] = report.trades.as_slice() else {
        panic!("{:?}", report.trades);
    };
    let exit = trade.exit.as_ref().map(|exit| (exit.price, exit.profit));
    assert_eq!((trade.entry_price, exit), (103.0, Some((104.0, 10.0))));

    // In a study the orders are checked and ignored, and the readings are those of a flat
    // account.
    let script = compile("plot(\"Position\", position());\nentry(\"Long\", \"long\");");
    assert!(script.places_orders());
    assert_eq!(script.check(&ladder), Ok(()));
    let study = calculated(&script, &ladder);
    assert_eq!(study.values().line(0), [0.0; 6]);

    // A strategy script needs no plot, and its study then calculates a row without a line for
    // every bar, so that the chart updates it from its last bar.
    let plotless = compile("entry(\"Long\", \"long\");");
    let study = calculated(&plotless, &ladder);
    assert!(matches!(study.calculation(), Some(Calculation::Done(_))));
    assert_eq!(plotless.last_error(), None);
    assert_eq!(
        (study.values().len(), study.values().columns().len()),
        (6, 0)
    );
}
