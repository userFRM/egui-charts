use super::*;
use crate::model::Bar;
use chrono::{TimeZone, Utc};

/// A host registers a study under its name, replacing a study of that name, a built-in
/// included; a blank name registers nothing.
#[test]
fn test_custom_indicator() {
    let mut factory = IndicatorFactory::new();
    let names = |factory: &IndicatorFactory| {
        let mut names = factory.list();
        names.sort();
        names
    };
    let before = names(&factory);
    for blank in ["", " \t"] {
        assert!(!factory.register(blank, || Box::new(SMA::new(10))));
        assert!(factory.create(blank).is_none());
    }
    assert_eq!(names(&factory), before);

    assert!(factory.register("Custom", || Box::new(SMA::new(10))));
    assert!(factory.has("Custom"));
    assert!(factory.create("Custom").is_some());
    assert!(factory.register("RSI", || Box::new(SMA::new(5))));
    assert_eq!(factory.create("RSI").unwrap().name(), "SMA");
}

/// Every registered name must construct, and the produced indicator's
/// `name()` must round-trip back to the key it was registered under. This
/// guards the factory's core invariant: a name returned by `list()` is a
/// name that `create()` honors, and the key matches the indicator identity. Bar studies
/// must also opt into live updates; profiles use a cached driver and the visible average
/// can change its whole line.
#[test]
fn every_registered_study_round_trips_and_supports_live_updates() {
    let factory = IndicatorFactory::new();
    let bars: Vec<_> = (0..3)
        .map(|i| {
            Bar::new(
                Utc.timestamp_opt(i * 60, 0).unwrap(),
                100.0,
                101.0,
                99.0,
                100.0,
                1.0,
            )
        })
        .collect();

    for name in factory.list() {
        let mut indicator = factory
            .create(&name)
            .unwrap_or_else(|| panic!("registered name {name:?} failed to construct"));
        assert_eq!(
            indicator.name(),
            name,
            "indicator constructed for key {name:?} reports a different name()",
        );
        if !name.starts_with("Vol Profile ") && name != "Visible Average Price" {
            indicator.calculate(&bars);
            assert!(
                indicator.is_incremental(),
                "{name} must update on every tick"
            );
        }
    }

    let mut factory = factory;
    let defaults: Vec<_> = factory
        .list()
        .iter()
        .map(|name| {
            let mut template = StudyTemplate::from_study(factory.create(name).unwrap().as_ref());
            for plot in &mut template.settings.plots {
                plot.width = 4.0;
            }
            template.settings.precision = Some(3);
            template
        })
        .collect();
    factory.set_defaults(&defaults).unwrap();
    for template in defaults {
        let study = factory.create(&template.name).unwrap();
        assert_eq!(study.settings(), template.settings, "{}", template.name);
    }
}

/// Defaults affect future creations only; failed batches and constructor changes cannot leave
/// partially applied or stale settings behind.
#[test]
fn host_defaults_are_atomic_and_follow_the_constructor_lifetime() {
    let mut factory = IndicatorFactory::new();
    let before = factory.create("SMA").unwrap();
    let mut defaults = StudyTemplate::from_study(before.as_ref());
    defaults.inputs = vec![("period".into(), InputValue::Length(3))];
    defaults.settings.plots[0].width = 4.0;
    factory
        .set_defaults(std::slice::from_ref(&defaults))
        .unwrap();
    assert_eq!(before.plot_style(0).width, 1.0);
    assert_eq!(factory.create("SMA").unwrap().plot_style(0).width, 4.0);

    let invalid = |edit: fn(&mut StudyTemplate)| {
        let mut template = defaults.clone();
        edit(&mut template);
        template
    };
    let unknown_name = invalid(|t| t.name = "Missing".into());
    let unknown_input = invalid(|t| t.inputs[0].0 = "missing".into());
    let zero_length = invalid(|t| t.inputs[0].1 = InputValue::Length(0));
    let wrong_type = invalid(|t| t.inputs[0].1 = InputValue::Toggle(true));
    let precision = invalid(|t| t.settings.precision = Some(13));
    let width = invalid(|t| t.settings.plots[0].width = f32::NAN);
    let plots = invalid(|t| t.settings.plots.clear());
    let mut old_input =
        StudyTemplate::from_study(factory.create("LinReg Channel").unwrap().as_ref());
    old_input.inputs = vec![("multiplier".into(), InputValue::Number(3.0))];
    let mut replacement = defaults.clone();
    replacement.settings.plots[0].width = 6.0;
    for template in [
        unknown_name,
        unknown_input,
        zero_length,
        wrong_type,
        precision,
        width,
        plots,
    ] {
        assert!(
            factory
                .set_defaults(&[replacement.clone(), template])
                .is_err()
        );
        assert_eq!(factory.create("SMA").unwrap().plot_style(0).width, 4.0);
    }
    factory.set_defaults(&[old_input]).unwrap();
    let inputs = factory.create("LinReg Channel").unwrap().inputs();
    for id in ["upper_multiplier", "lower_multiplier"] {
        let input = inputs.iter().find(|input| input.id == id).unwrap();
        assert_eq!(input.value, InputValue::Number(3.0));
    }
    factory.set_defaults(&[]).unwrap();
    factory
        .set_defaults(&[defaults.clone(), replacement])
        .unwrap();
    assert_eq!(factory.create("SMA").unwrap().plot_style(0).width, 6.0);
    assert!(factory.clear_defaults("SMA"));
    assert!(!factory.clear_defaults("SMA"));
    assert_eq!(factory.create("SMA").unwrap().plot_style(0).width, 1.0);
    factory
        .set_defaults(std::slice::from_ref(&defaults))
        .unwrap();
    factory.register("SMA", || Box::new(SMA::new(7)));
    assert!(!factory.clear_defaults("SMA"));
    assert_eq!(
        factory.create("SMA").unwrap().inputs()[0].value,
        InputValue::Length(7)
    );
    factory
        .set_defaults(std::slice::from_ref(&defaults))
        .unwrap();
    assert!(factory.unregister("SMA"));
    factory.register("SMA", || Box::new(SMA::new(8)));
    assert_eq!(
        factory.create("SMA").unwrap().inputs()[0].value,
        InputValue::Length(8)
    );
    factory.set_defaults(&[defaults]).unwrap();
    factory.clear();
    factory.register("SMA", || Box::new(SMA::new(9)));
    assert_eq!(
        factory.create("SMA").unwrap().inputs()[0].value,
        InputValue::Length(9)
    );
}

/// Confirming the settings dialog without an edit must preserve every drawn colour,
/// including per-bar palettes that contain more colours than the study has plots.
#[test]
fn unchanged_settings_keep_every_colour() {
    use crate::studies::ConfiguredStudy;

    let bars: Vec<Bar> = (0..300)
        .map(|i| {
            let c = 100.0 + 10.0 * (i as f64 / 9.0).sin();
            let time = Utc.timestamp_opt(i * 3600, 0).unwrap();
            Bar::new(time, c - 0.5, c + 1.0, c - 1.0, c, 1000.0 + i as f64)
        })
        .collect();
    let colours = |study: &ConfiguredStudy| -> Vec<_> {
        (0..study.line_count())
            .flat_map(|line| {
                let style = study.plot_style(line).color;
                (0..bars.len()).map(move |bar| study.plot_color(line, bar).unwrap_or(style))
            })
            .collect()
    };
    for name in IndicatorFactory::new().list() {
        let mut study = ConfiguredStudy::new(IndicatorFactory::new().create(&name).unwrap());
        study.calculate(&bars);
        let before = colours(&study);
        study.apply_settings(&[], study.settings()).unwrap();
        study.calculate(&bars);
        assert_eq!(colours(&study), before, "{name}");
    }
}

/// Any line is accepted by `plot_style` and `line_style`: one past the last, which renderers
/// do not draw, gets a style instead of a panic, through the study a template recreates.
#[test]
fn every_study_styles_a_line_past_its_last() {
    use crate::studies::StudyTemplate;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let factory = IndicatorFactory::new();
    let mut panicked: Vec<String> = factory
        .list()
        .into_iter()
        .filter(|name| {
            let study = StudyTemplate::from_study(factory.create(name).unwrap().as_ref())
                .create()
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            [study.line_count(), usize::MAX].into_iter().any(|line| {
                catch_unwind(AssertUnwindSafe(|| {
                    study.plot_style(line);
                    study.line_style(line);
                }))
                .is_err()
            })
        })
        .collect();
    panicked.sort();
    assert_eq!(panicked, Vec::<String>::new());
}

/// Catalogue studies have distinct values except ADX and DMI, which expose the same Wilder
/// components in a different order. Fixed profiles use a saved end.
#[test]
fn catalogue_studies_are_distinct_except_for_adx_and_dmi_components() {
    use crate::model::Bar;
    use chrono::{TimeZone, Utc};

    // Two bars per session over more than 52 weeks distinguish session and calendar studies;
    // 800 bars warm up the longest study, and a step every 160 bars opens a price gap.
    let bars: Vec<Bar> = (0..800)
        .map(|i: i64| {
            let x = i as f64;
            let close =
                100.0 + (x * 0.13).sin() * 8.0 + (x * 0.029).cos() * 5.0 + (i / 160) as f64 * 6.0;
            let open = close - (x * 0.7).sin() * 1.5;
            Bar {
                time: Utc.timestamp_opt((i / 2 * 48 + i % 2) * 3_600, 0).unwrap(),
                open,
                high: close.max(open) + 0.5 + (x * 0.3).sin().abs(),
                low: close.min(open) - 0.5 - (x * 0.5).cos().abs(),
                close,
                volume: 1_000.0 + ((i * 37) % 211) as f64 * 9.0,
                colors: None,
                payload: None,
            }
        })
        .collect();
    let factory = IndicatorFactory::new();
    let mut names = factory.list();
    names.sort();
    let mut seen = HashMap::new();
    let mut duplicates = Vec::new();
    for name in names {
        let mut study = factory.create(&name).unwrap();
        if study.needs_order_flow() {
            continue;
        }
        // External series have no values until the host supplies their distinct inputs.
        if !study.data_requests().is_empty() {
            continue;
        }
        // Fixed and anchored profiles include all history until a range is chosen.
        if name == "Vol Profile FR" {
            study
                .set_input(
                    "end_time",
                    InputValue::Text(bars[599].time.format("%Y-%m-%d %H:%M:%S").to_string()),
                )
                .unwrap();
        }
        study.calculate(&bars);
        // Studies that draw only shapes differ by their zones, projections, table or profile.
        let shapes = vec![
            format!("{:?}", study.zones()),
            format!("{:?}", study.projections()),
            format!("{:?}", study.table()),
            study.tpo().is_some().to_string(),
            format!("{:?}", study.volume_profiles(200..400)),
        ];
        #[cfg(feature = "drawings")]
        let shapes = shapes
            .into_iter()
            .chain(std::iter::once(format!("{:?}", study.drawings())))
            .collect();
        let lines: Vec<Vec<String>> = std::iter::once(shapes)
            .chain(study.values().to_rows().iter().map(|v| {
                let mut xs = match v {
                    IndicatorValue::Single(x) => vec![*x],
                    IndicatorValue::Multiple(xs) => xs.clone(),
                    IndicatorValue::None => Vec::new(),
                };
                xs.sort_by(f64::total_cmp);
                xs.iter().map(|x| format!("{x:.9e}")).collect()
            }))
            .collect();
        if let Some(first) = seen.insert(lines, name.clone()) {
            duplicates.push(format!("{first} and {name}"));
        }
    }
    assert_eq!(duplicates, ["ADX and DMI"]);
}

/// The study browser opens these studies with the lengths most often used for them.
#[test]
fn catalogue_studies_open_with_their_usual_lengths() {
    let rows = [
        ("SMA", "period", 9),
        ("EMA", "period", 9),
        ("WMA", "period", 9),
        ("HMA", "period", 9),
        ("DEMA", "period", 9),
        ("TEMA", "period", 9),
        ("LSMA", "period", 25),
        ("SMMA", "period", 7),
        ("ROC", "period", 9),
        ("TRIX", "period", 18),
        ("CMO", "period", 9),
        ("HV", "period", 10),
        ("TSI", "signal_period", 13),
    ];
    for (name, id, length) in rows {
        let inputs = IndicatorFactory::new().create(name).unwrap().inputs();
        let input = inputs.iter().find(|input| input.id == id);
        assert_eq!(
            input.map(|input| &input.value),
            Some(&crate::studies::InputValue::Length(length)),
            "{name} {id}"
        );
    }
}

/// The study browser opens the channel studies with their usual inputs, and every band they
/// fill lies between two of their drawn plots in a colour that shows.
#[test]
fn channel_studies_open_with_their_usual_inputs_and_fill_their_bands() {
    use crate::studies::{FillBoundary, InputValue};

    let rows = [
        (
            "Envelopes",
            vec![("percent", InputValue::Number(10.0))],
            vec![(0, 2)],
        ),
        (
            "LinReg Channel",
            vec![
                ("period", InputValue::Length(100)),
                ("upper_multiplier", InputValue::Number(2.0)),
                ("lower_multiplier", InputValue::Number(2.0)),
            ],
            vec![(0, 1), (1, 2)],
        ),
        (
            "MA Channel",
            vec![
                ("upper_length", InputValue::Length(20)),
                ("lower_length", InputValue::Length(20)),
                ("upper_offset", InputValue::Integer(0)),
                ("lower_offset", InputValue::Integer(0)),
            ],
            vec![(0, 1)],
        ),
    ];
    for (name, inputs, bands) in rows {
        let study = IndicatorFactory::new().create(name).unwrap();
        let values = study.inputs();
        for (id, value) in inputs {
            let input = values.iter().find(|input| input.id == id);
            assert_eq!(input.map(|input| &input.value), Some(&value), "{name} {id}");
        }
        let fills: Vec<_> = study
            .fills()
            .iter()
            .map(|fill| {
                let (FillBoundary::Plot(first), FillBoundary::Plot(second)) =
                    (fill.first, fill.second)
                else {
                    panic!("{name}: {fill:?}")
                };
                for line in [first, second] {
                    assert!(
                        line < study.line_count() && study.plot_style(line).visible,
                        "{name}: plot {line} is not drawn"
                    );
                }
                assert_ne!(fill.color.a(), 0, "{name}: transparent fill");
                (first, second)
            })
            .collect();
        assert_eq!(fills, bands, "{name}");
    }
}
