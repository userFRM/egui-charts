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
    for id in ["use_upper", "use_lower"] {
        let input = inputs.iter().find(|input| input.id == id).unwrap();
        assert_eq!(input.value, InputValue::Toggle(true));
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

/// Confirming the settings dialog without an edit, or recreating the study from its template,
/// must preserve every drawn colour, including per-bar palettes that contain more colours than
/// the study has plots.
#[test]
fn unchanged_settings_keep_every_colour() {
    use crate::studies::{ConfiguredStudy, StudyTemplate};

    let bars: Vec<Bar> = (0..300)
        .map(|i| {
            let c = 100.0 + 10.0 * (i as f64 / 9.0).sin();
            let time = Utc.timestamp_opt(i * 3600, 0).unwrap();
            Bar::new(time, c - 0.5, c + 1.0, c - 1.0, c, 1000.0 + i as f64)
        })
        .collect();
    let colours = |study: &dyn Indicator| -> Vec<_> {
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
        // Ok without an edit applies every input as shown with the current settings.
        let inputs: Vec<_> = study
            .inputs()
            .into_iter()
            .map(|input| (input.id, input.value))
            .collect();
        study.apply_settings(&inputs, study.settings()).unwrap();
        study.calculate(&bars);
        assert_eq!(colours(&study), before, "{name}");
        let mut restored = StudyTemplate::from_study(&study).create().unwrap();
        restored.calculate(&bars);
        assert_eq!(colours(restored.as_ref()), before, "{name} template");
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
/// components in a different order, and Donchian and Price Channel, which draw the same highest
/// high, lowest low and midpoint with their own colours and fill. Fixed profiles use a saved end.
/// Single candlestick patterns are left out.
#[test]
fn catalogue_studies_are_distinct_except_adx_dmi_and_donchian_price_channel() {
    use crate::model::Bar;
    use chrono::{TimeZone, Utc};

    // Two bars per session over more than 52 weeks distinguish session and calendar studies;
    // 800 bars warm up the longest study, and a step every 160 bars opens a price gap.
    let bars: Vec<Bar> = (0..800)
        .map(|i: i64| {
            let x = i as f64;
            // The slower swing also gives extreme-filtered reversal studies signals.
            let close = 100.0
                + (x * 0.13).sin() * 8.0
                + (x * 0.03).sin() * 10.0
                + (x * 0.029).cos() * 5.0
                + (i / 160) as f64 * 6.0;
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
        // Many single patterns never complete on this fixture; their definition table tests
        // distinguish them and their factory names instead.
        if (0..CandlestickPatterns::PATTERN_COUNT)
            .any(|k| CandlestickPatterns::only(k).name() == name)
        {
            continue;
        }
        // A single chart formation repeats its family's study with the other formations off.
        if PatternFamily::ALL
            .iter()
            .flat_map(|family| family.kinds())
            .any(|&kind| ChartPatterns::only(kind).name() == name)
        {
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
        // Studies that draw only shapes differ by their zones, projections, table or profile, and
        // studies of constant columns by their colours.
        let shapes = vec![
            format!("{:?}", study.zones()),
            format!("{:?}", study.projections()),
            format!("{:?}", study.table()),
            study.tpo().is_some().to_string(),
            format!("{:?}", study.volume_profiles(200..400)),
            format!(
                "{:?}",
                (0..bars.len())
                    .map(|bar| study.plot_color(0, bar))
                    .collect::<Vec<_>>()
            ),
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
    assert_eq!(duplicates, ["ADX and DMI", "Donchian and Price Channel"]);
}

/// The study browser opens these studies with the lengths and switches most often used for them.
#[test]
fn catalogue_studies_open_with_their_usual_lengths() {
    use crate::studies::InputValue::{Choice, Length, Number, Toggle};
    let rows = [
        ("SMA", "period", Length(9)),
        ("EMA", "period", Length(9)),
        ("WMA", "period", Length(9)),
        ("HMA", "period", Length(9)),
        ("DEMA", "period", Length(9)),
        ("TEMA", "period", Length(9)),
        ("LSMA", "period", Length(25)),
        ("SMMA", "period", Length(7)),
        ("Hamming MA", "period", Length(10)),
        ("EMA Cross", "period_0", Length(9)),
        ("EMA Cross", "period_1", Length(26)),
        ("ROC", "period", Length(9)),
        ("TRIX", "period", Length(18)),
        ("CMO", "period", Length(9)),
        ("HV", "period", Length(10)),
        ("TSI", "signal_period", Length(13)),
        ("Correlation Coefficient", "length", Length(20)),
        ("Correlation - Log", "length", Length(25)),
        ("Knoxville Divergence", "momentum_period", Length(20)),
        ("Knoxville Divergence", "rsi_period", Length(21)),
        ("Knoxville Divergence", "lookback", Length(150)),
        ("Advance/Decline", "period", Length(10)),
        ("Missed Pivot Levels", "period", Choice(4)),
        ("Missed Pivot Levels", "days_back", Length(180)),
        ("Missed Pivot Levels", "show_touched", Toggle(false)),
        ("Missed Pivot Levels", "tolerance", Number(0.0)),
        ("Booker Reversal", "fast_period", Length(12)),
        ("Booker Reversal", "slow_period", Length(26)),
        ("Booker Reversal", "k_period", Length(70)),
        ("Booker Reversal", "slowing", Length(10)),
        ("Booker Reversal", "stoch_upper", Number(70.0)),
        ("Booker Reversal", "stoch_lower", Number(30.0)),
        ("Ziv Ghost Pivots", "show_four_hour", Toggle(true)),
        ("Ziv Ghost Pivots", "show_day", Toggle(true)),
        ("Ziv Ghost Pivots", "show_week", Toggle(true)),
        ("Ziv Ghost Pivots", "show_month", Toggle(true)),
    ];
    let factory = IndicatorFactory::new();
    for (name, id, value) in rows {
        assert!(
            factory.list().iter().any(|listed| listed == name),
            "{name} is not listed"
        );
        let inputs = factory.create(name).unwrap().inputs();
        let input = inputs.iter().find(|input| input.id == id);
        assert_eq!(input.map(|input| &input.value), Some(&value), "{name} {id}");
    }
    // A Hamming MA saved at the former default keeps its period.
    let saved: StudyTemplate = serde_json::from_str(r#"{"name":"Hamming MA","inputs":[["period",{"Length":20}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.HammingMA.html","plots":[{"color":[76,175,80,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[5,11,5,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#).unwrap();
    let inputs = saved.create().unwrap().inputs();
    assert_eq!(
        inputs
            .iter()
            .find(|input| input.id == "period")
            .map(|input| &input.value),
        Some(&Length(20))
    );
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

/// Studies that colour each bar by its state offer every state's colour in the Style tab,
/// draw each bar in its state's colour through an edit and a template, and restore settings
/// saved before those states had colours of their own with the colours they drew.
#[test]
fn per_bar_states_offer_the_colours_they_draw() {
    use crate::studies::{ConfiguredStudy, StudyTemplate, palette};
    use egui::Color32;

    // A parabola sweeps the EMA angle through every Chop Zone band, slowly enough that each
    // band holds bars; the swings after it give every impulse, trend and acceleration state.
    let bars: Vec<Bar> = (0..900)
        .map(|i| {
            let x = i as f64;
            let c = if i < 600 {
                100.0 + 5e-5 * (x - 300.0).powi(2)
            } else {
                104.5 + 10.0 * ((x - 600.0) / 9.0).sin()
            };
            let time = Utc.timestamp_opt(i * 3600, 0).unwrap();
            Bar::new(time, c - 0.5, c + 1.0, c - 1.0, c, 1000.0)
        })
        .collect();
    let drawn = |study: &dyn Indicator| -> Vec<Color32> {
        (0..bars.len())
            .filter(|&bar| study.plot_value(0, bar).is_some())
            .map(|bar| {
                study
                    .plot_color(0, bar)
                    .unwrap_or(study.plot_style(0).color)
            })
            .collect()
    };
    // Name, plot colour, the other states in the Style tab, the plot colour saved before they
    // existed, and whether such a save drew every bar in it.
    type Row = (
        &'static str,
        Color32,
        Vec<(&'static str, Color32)>,
        Color32,
        bool,
    );
    let rows: [Row; 4] = [
        (
            "Accelerator Oscillator",
            palette::GREEN,
            vec![("Decreasing bars", palette::RED)],
            Color32::from_rgb(0, 0, 128),
            false,
        ),
        (
            "Chop Zone",
            palette::CYAN,
            vec![
                ("Rising 3.57° to 5°", palette::DARK_GREEN),
                ("Rising 2.14° to 3.57°", palette::LIGHT_GREEN),
                ("Rising 0.71° to 2.14°", palette::DEEP_TEAL),
                ("Flat, under 0.71°", palette::YELLOW),
                ("Falling 0.71° to 2.14°", palette::LIGHT_ORANGE),
                ("Falling 2.14° to 3.57°", palette::ORANGE),
                ("Falling 3.57° to 5°", palette::PINK),
                ("Falling 5° or more", palette::DEEP_RED),
            ],
            palette::CYAN,
            false,
        ),
        (
            "Impulse",
            palette::GREEN,
            vec![("Bearish", palette::RED), ("Neutral", palette::BLUE)],
            palette::GREEN,
            true,
        ),
        (
            "Volatility Stop",
            palette::DEEP_TEAL,
            vec![("Downtrend", palette::BRIGHT_RED)],
            palette::DEEP_TEAL,
            false,
        ),
    ];
    let factory = IndicatorFactory::new();
    for (name, plot, states, saved, one_colour) in rows {
        let mut study = ConfiguredStudy::new(factory.create(name).unwrap());
        study.calculate(&bars);
        let settings = study.settings();
        assert_eq!(settings.plots[0].color, plot, "{name}");
        let offered: Vec<_> = settings.plot_variants[0]
            .iter()
            .map(|variant| (variant.name.as_str(), variant.color))
            .collect();
        assert_eq!(offered, states, "{name}");
        // Every state is drawn, and nothing else.
        let colours: Vec<Color32> = std::iter::once(plot)
            .chain(states.iter().map(|&(_, colour)| colour))
            .collect();
        let before = drawn(&study);
        let seen: std::collections::HashSet<_> = before.iter().copied().collect();
        let expected: std::collections::HashSet<_> = colours.iter().copied().collect();
        assert_eq!(seen, expected, "{name}");

        // Each control recolours exactly the bars of its own state.
        let opening = study.snapshot_settings();
        // Choosing navy now is an explicit edit, so a new save must keep it.
        let new = |state: usize| {
            if state == 0 {
                Color32::from_rgb(0, 0, 128)
            } else {
                Color32::from_rgb(1, 2, 3 + state as u8)
            }
        };
        let mut edited = study.settings();
        edited.plots[0].color = new(0);
        for (index, variant) in edited.plot_variants[0].iter_mut().enumerate() {
            variant.color = new(index + 1);
        }
        study.apply_settings(&[], edited).unwrap();
        study.calculate(&bars);
        let recoloured: Vec<_> = before
            .iter()
            .map(|colour| new(colours.iter().position(|c| c == colour).unwrap()))
            .collect();
        assert_eq!(drawn(&study), recoloured, "{name}");
        let mut restored = StudyTemplate::from_study(&study).create().unwrap();
        restored.calculate(&bars);
        assert_eq!(drawn(restored.as_ref()), recoloured, "{name} template");
        let changed = study.snapshot_settings();
        study.restore_settings(&opening).unwrap();
        study.calculate(&bars);
        assert_eq!(drawn(&study), before, "{name} cancel or undo");
        study.restore_settings(&changed).unwrap();
        study.calculate(&bars);
        assert_eq!(drawn(&study), recoloured, "{name} redo");

        // Older saves either omitted the variants or wrote an empty list, with default or
        // edited plot styles. Both keep the colours and widths they drew.
        for (variants, saved) in [Vec::new(), vec![Vec::new()]]
            .into_iter()
            .flat_map(|variants| [saved, Color32::BLACK].map(|saved| (variants.clone(), saved)))
        {
            let mut old =
                StudyTemplate::from_study(&ConfiguredStudy::new(factory.create(name).unwrap()));
            old.settings.plots[0].color = saved;
            old.settings.plots[0].width = 3.5;
            old.settings.plot_variants = variants;
            let mut restored = old.create().unwrap();
            restored.calculate(&bars);
            let expected = if one_colour {
                vec![saved; before.len()]
            } else {
                before
                    .iter()
                    .map(|&colour| {
                        if colour == plot && saved != Color32::from_rgb(0, 0, 128) {
                            saved
                        } else {
                            colour
                        }
                    })
                    .collect()
            };
            assert_eq!(drawn(restored.as_ref()), expected, "{name} saved before");
            assert_eq!(restored.plot_style(0).width, 3.5, "{name} saved width");
            assert!(
                restored
                    .plot_variants(0)
                    .iter()
                    .all(|variant| variant.width == 3.5)
            );
        }
    }
}

/// No catalogue study configured as the chart configures it lists two inputs under one id or
/// one label, so an edit by id reaches one input and every field of its Inputs tab says which
/// value it sets.
#[test]
fn configured_studies_list_every_input_id_and_label_once() {
    use crate::studies::ConfiguredStudy;

    let factory = IndicatorFactory::new();
    let mut repeated: Vec<(String, String)> = Vec::new();
    for name in factory.list() {
        let inputs = ConfiguredStudy::new(factory.create(&name).unwrap()).inputs();
        for (index, input) in inputs.iter().enumerate() {
            let earlier = &inputs[..index];
            if earlier.iter().any(|other| other.id == input.id) {
                repeated.push((name.clone(), format!("id {}", input.id)));
            }
            if earlier.iter().any(|other| other.label == input.label) {
                repeated.push((name.clone(), format!("label {}", input.label)));
            }
        }
    }
    repeated.sort();
    assert_eq!(repeated, Vec::<(String, String)>::new());
}

/// Every choice of a catalogue study, configured as the chart configures it, reads in words
/// under every value of its other choices, never as a code name with joined words such as
/// `Sma50Sma200` or `PreviousClose`. A study saved with any of its choices restores that choice
/// under the same label, and the indices saved before the labels changed keep their meaning.
#[test]
fn catalogue_choices_read_in_words_and_restore() {
    use crate::studies::{ConfiguredStudy, InputKind, InputValue, StudyTemplate};

    // A lower-case letter or a digit run into a capital that starts a word, as in
    // `PreviousClose` or `Sma50Sma200`. Interval labels such as `12M` pass.
    let joined = |label: &str| {
        label.as_bytes().windows(3).any(|w| {
            (w[0].is_ascii_lowercase() || w[0].is_ascii_digit())
                && w[1].is_ascii_uppercase()
                && w[2].is_ascii_lowercase()
        })
    };
    let choice = |study: &dyn Indicator, id: &str| {
        study
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .and_then(|input| match (input.kind, input.value) {
                (InputKind::Choice(labels), InputValue::Choice(index)) => {
                    Some((index, labels.get(index).cloned()))
                }
                _ => None,
            })
    };
    let factory = IndicatorFactory::new();
    let chosen = |name: &str, id: &str, index: usize| -> Box<dyn Indicator> {
        let mut study = Box::new(ConfiguredStudy::new(factory.create(name).unwrap()));
        study
            .set_input(id, InputValue::Choice(index))
            .unwrap_or_else(|error| panic!("{name} {id} {index}: {error}"));
        study
    };
    let restored = |study: &dyn Indicator| {
        let saved = serde_json::to_string(&StudyTemplate::from_study(study)).unwrap();
        serde_json::from_str::<StudyTemplate>(&saved)
            .unwrap()
            .create_with(&factory)
            .unwrap()
    };
    let mut names = factory.list();
    // Left out of the catalogue but recreated from saved layouts.
    names.extend(
        [
            "CVD",
            "HL2",
            "HLC3",
            "Price Channel",
            "Ehlers Fisher",
            "MA Combination",
        ]
        .map(String::from),
    );
    names.sort();
    names.dedup();
    let (mut wrong, mut lost) = (Vec::new(), Vec::new());
    for name in &names {
        for input in ConfiguredStudy::new(factory.create(name).unwrap()).inputs() {
            let InputKind::Choice(labels) = &input.kind else {
                continue;
            };
            for index in 0..labels.len() {
                let study = chosen(name, &input.id, index);
                for shown in study.inputs() {
                    if let InputKind::Choice(labels) = shown.kind {
                        wrong.extend(
                            labels
                                .into_iter()
                                .filter(|label| joined(label))
                                .map(|label| format!("{name} {}: {label}", shown.id)),
                        );
                    }
                }
                let before = choice(study.as_ref(), &input.id);
                let after = choice(restored(study.as_ref()).as_ref(), &input.id);
                if before.as_ref().map(|(at, _)| *at) != Some(index) || after != before {
                    lost.push(format!(
                        "{name} {} {index}: {before:?} -> {after:?}",
                        input.id
                    ));
                }
            }
        }
    }
    assert_eq!(lost, Vec::<String>::new());
    wrong.sort();
    wrong.dedup();
    assert_eq!(wrong, Vec::<String>::new());

    let saved = [
        ("All Candlestick Patterns", "trend", 0, "SMA50"),
        ("All Candlestick Patterns", "trend", 1, "SMA50, SMA200"),
        ("All Candlestick Patterns", "trend", 2, "No detection"),
        ("Pivot Points Standard", "timeframe", 6, "Biyearly"),
        ("Pivot Points Standard", "timeframe", 9, "Decennially"),
        ("VWAP Auto Anchored", "anchor", 1, "Highest high"),
        ("VWAP Auto Anchored", "anchor", 3, "Highest volume"),
        ("Vol Profile Auto Anchored", "anchor", 1, "Highest high"),
        ("Vol Profile Auto Anchored", "anchor", 2, "Lowest low"),
        ("Vol Profile Auto Anchored", "anchor", 3, "Highest volume"),
        ("BB", "basis", 4, "VWMA"),
        ("Volume", "color_rule", 1, "Previous close"),
        ("CVD", "mode", 0, "Close position"),
        ("CVD", "mode", 1, "Body ratio"),
        ("ZigZag", "mode", 0, "High/Low"),
        ("Keltner", "band", 0, "Average True Range"),
        ("Auto Key Levels", "start", 0, "Start of period"),
        ("Auto Key Levels", "start", 1, "Current bar"),
        ("Auto Key Levels", "anchor", 0, "Custom"),
        ("Auto Key Levels", "anchor", 1, "Earnings"),
        ("Auto Key Levels", "anchor", 2, "Split"),
        ("Auto Key Levels", "anchor", 3, "Dividends"),
        ("Auto Key Levels", "unit", 0, "Days"),
        ("Auto Key Levels", "unit", 1, "Weeks"),
        ("Auto Key Levels", "unit", 2, "Months"),
        ("Missed Pivot Levels", "period", 0, "Swings"),
        ("Missed Pivot Levels", "period", 1, "Daily"),
        ("Missed Pivot Levels", "period", 2, "Weekly"),
        ("Missed Pivot Levels", "period", 3, "Monthly"),
        (
            "Missed Pivot Levels",
            "period",
            4,
            "Daily, weekly and monthly",
        ),
    ];
    let got: Vec<_> = saved
        .iter()
        .map(|&(name, id, index, _)| {
            let label = choice(restored(chosen(name, id, index).as_ref()).as_ref(), id)
                .filter(|(at, _)| *at == index)
                .and_then(|(_, label)| label);
            (name, id, index, label)
        })
        .collect();
    let want: Vec<_> = saved
        .iter()
        .map(|&(name, id, index, label)| (name, id, index, Some(label.to_owned())))
        .collect();
    assert_eq!(got, want);

    // A template saved before choice labels changed reads its basis in words.
    let saved = r#"{"name":"BB","inputs":[["period",{"Length":20}],["basis",{"Choice":0}],["std_dev",{"Number":2.0}],["source",{"Source":{"Price":"Close"}}],["plot_offset",{"Integer":0}],["calculation_symbol",{"Symbol":""}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.BollingerBands.html","plots":[{"color":[242,54,69,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[8,153,129,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":false}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[15,3,4,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[{"color":[2,8,12,13],"inverted_color":null,"visible":true}],"gradients":[]},"visible":true,"scale":"Main"}"#;
    let restored = serde_json::from_str::<StudyTemplate>(saved)
        .unwrap()
        .create_with(&factory)
        .unwrap();
    assert_eq!(
        choice(restored.as_ref(), "basis"),
        Some((0, Some("SMA".to_owned())))
    );
}
