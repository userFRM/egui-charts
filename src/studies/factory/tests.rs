use super::*;
use crate::model::Bar;
use chrono::{TimeZone, Utc};

#[test]
fn test_factory() {
    let factory = IndicatorFactory::new();

    // Canonical built-in keys are present.
    assert!(factory.has("RSI"));
    assert!(factory.has("MACD"));

    // Creation yields the matching indicator.
    let rsi = factory.create("RSI");
    assert!(rsi.is_some());
    assert_eq!(rsi.unwrap().name(), "RSI");

    let list = factory.list();
    assert!(!list.is_empty());
}

#[test]
fn test_custom_indicator() {
    let mut factory = IndicatorFactory::new();

    factory.register("Custom", || Box::new(SMA::new(10)));

    assert!(factory.has("Custom"));
    assert!(factory.create("Custom").is_some());
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
}

/// Confirming the settings dialog without an edit hands each plot's colour back through
/// `set_colors`, which must leave every drawn colour as it was.
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

/// Each registered name is a study of its own: on one series, no two names give the same
/// values on every bar, even with their lines in another order. Fixed profiles use a saved end.
#[test]
fn no_two_names_compute_the_same_study() {
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
        if study.order_flow_input().is_some() {
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
                    InputValue::Number(bars[599].time.timestamp_millis() as f64),
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
    assert!(duplicates.is_empty(), "same values: {duplicates:?}");
}
