//! Resumable seeded averages used by composite studies, and the average a study can draw over its
//! own output.

use crate::studies::{FillBoundary, IndicatorValue, StudyFill, palette};
use egui::Color32;
use std::collections::VecDeque;

/// The default colours of the average and its upper and lower band.
pub(super) const SMOOTHING_COLORS: [Color32; 3] = [palette::YELLOW, palette::GREEN, palette::GREEN];

/// The average a study draws over its output. The `Debug` names are the choices its type input
/// lists.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SmoothingType {
    None,
    Sma,
    SmaWithBands,
    Ema,
    Smma,
    Wma,
    Vwma,
}

impl std::fmt::Debug for SmoothingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::None => "None",
            Self::Sma => "SMA",
            Self::SmaWithBands => "SMA + Bollinger Bands",
            Self::Ema => "EMA",
            Self::Smma => "SMMA (RMA)",
            Self::Wma => "WMA",
            Self::Vwma => "VWMA",
        })
    }
}

impl SmoothingType {
    /// Every type, in the order the type input lists them.
    pub(super) const ALL: [Self; 7] = [
        Self::None,
        Self::Sma,
        Self::SmaWithBands,
        Self::Ema,
        Self::Smma,
        Self::Wma,
        Self::Vwma,
    ];
    /// The positions in [`ALL`](Self::ALL) that use a length.
    pub(super) const WITH_LENGTH: [usize; 6] = [1, 2, 3, 4, 5, 6];
    /// The position in [`ALL`](Self::ALL) that uses a band deviation.
    pub(super) const WITH_BANDS: [usize; 1] = [2];

    /// The lines this type adds after the study's own.
    pub(super) fn lines(self) -> usize {
        match self {
            Self::None => 0,
            Self::SmaWithBands => 3,
            _ => 1,
        }
    }

    /// The names of the lines this type adds, with the average called `average`.
    pub(super) fn line_names(self, average: &str) -> Vec<String> {
        let names = [average, "Upper band", "Lower band"];
        names[..self.lines()]
            .iter()
            .map(|&n| n.to_owned())
            .collect()
    }

    /// The band between this type's upper and lower line, for a study with one line of its own.
    pub(super) fn fills(self) -> Vec<StudyFill> {
        if self != Self::SmaWithBands {
            return Vec::new();
        }
        vec![StudyFill {
            first: FillBoundary::Plot(2),
            second: FillBoundary::Plot(3),
            color: palette::GREEN.gamma_multiply_u8(26),
            inverted_color: None,
        }]
    }
}

/// Resumable [`SmoothingType`] average of a study's defined values.
#[derive(Clone, Default)]
pub(super) struct Smoothed {
    seeded: Smoothing,
    /// The last `length` values and the volumes of their bars.
    window: VecDeque<(f64, f64)>,
}

impl Smoothed {
    /// The study's `value`, followed by what `kind` adds to it. An average with too few values
    /// yet is NaN, as is a volume-weighted one over bars without volume.
    pub(super) fn next(
        &mut self,
        kind: SmoothingType,
        length: usize,
        deviation: f64,
        value: f64,
        volume: f64,
    ) -> IndicatorValue {
        let n = length as f64;
        let average = match kind {
            SmoothingType::None => return IndicatorValue::Single(value),
            SmoothingType::Ema => self.seeded.next(value, length, 2.0 / (n + 1.0)),
            SmoothingType::Smma => self.seeded.next(value, length, 1.0 / n),
            SmoothingType::Sma | SmoothingType::SmaWithBands => {
                self.push(length, value, volume);
                self.full(length, |window| {
                    window.iter().map(|(v, _)| v).sum::<f64>() / n
                })
            }
            // Weights 1 to n, the newest value heaviest.
            SmoothingType::Wma => {
                self.push(length, value, volume);
                self.full(length, |window| {
                    let weighted: f64 = (1..).zip(window).map(|(w, (v, _))| f64::from(w) * v).sum();
                    weighted / (n * (n + 1.0) / 2.0)
                })
            }
            SmoothingType::Vwma => {
                self.push(length, value, volume);
                self.full(length, |window| {
                    let weighted: f64 = window.iter().map(|(v, volume)| v * volume).sum();
                    weighted / window.iter().map(|(_, volume)| volume).sum::<f64>()
                })
            }
        };
        if kind != SmoothingType::SmaWithBands {
            return IndicatorValue::Multiple(vec![value, average]);
        }
        // The population standard deviation of the window the average covers.
        let band = deviation
            * self
                .full(length, |window| {
                    window
                        .iter()
                        .map(|(v, _)| (v - average).powi(2))
                        .sum::<f64>()
                        / n
                })
                .sqrt();
        IndicatorValue::Multiple(vec![value, average, average + band, average - band])
    }

    fn push(&mut self, length: usize, value: f64, volume: f64) {
        self.window.push_back((value, volume));
        if self.window.len() > length {
            self.window.pop_front();
        }
    }

    fn full(&self, length: usize, f: impl FnOnce(&VecDeque<(f64, f64)>) -> f64) -> f64 {
        if self.window.len() < length {
            return f64::NAN;
        }
        f(&self.window)
    }
}

#[derive(Clone, Default)]
pub(super) struct Smoothing {
    count: usize,
    value: f64,
}

impl Smoothing {
    pub(super) fn next(&mut self, input: f64, seed: usize, alpha: f64) -> f64 {
        if !input.is_finite() {
            return f64::NAN;
        }
        if self.count < seed {
            self.count += 1;
            self.value += input;
            if self.count < seed {
                return f64::NAN;
            }
            self.value /= seed as f64;
        } else {
            self.value += (input - self.value) * alpha;
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Bar;
    use crate::studies::{Indicator, InputValue, WilliamsR};

    fn bars(closes_and_volumes: &[(f64, f64)]) -> Vec<Bar> {
        closes_and_volumes
            .iter()
            .enumerate()
            .map(|(i, &(close, volume))| {
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                Bar::new(time, close, 10.0, 0.0, close, volume)
            })
            .collect()
    }

    /// With high 10 and low 0, %R(1) of closes 9, 7, 6, 2 is -10, -30, -40, -80, on volumes 1, 3,
    /// 2, 4. Over the last three, by hand: the SMA is -150/3; the population deviation of -30, -40,
    /// -80 is sqrt(1400/3), doubled for the bands; the EMA and SMMA start from the mean of the
    /// first three, -80/3, and add 1/2 and 1/3 of the step to -80; the WMA is (-30 - 80 - 240) / 6
    /// and the VWMA (-90 - 80 - 320) / 9. A corrected last bar and an appended one resume to what
    /// a fresh calculation gives.
    #[test]
    fn every_type_follows_its_definition_through_history_and_updates() {
        let band = 2.0 * (1400.0f64 / 3.0).sqrt();
        let fixture = [(9.0, 1.0), (7.0, 3.0), (6.0, 2.0), (2.0, 4.0)];
        for (smoothing, want) in [
            (1, vec![-50.0]),
            (2, vec![-50.0, -50.0 + band, -50.0 - band]),
            (3, vec![-160.0 / 3.0]),
            (4, vec![-400.0 / 9.0]),
            (5, vec![-350.0 / 6.0]),
            (6, vec![-490.0 / 9.0]),
        ] {
            let mut study = WilliamsR::new(1);
            study
                .set_input("smoothing", InputValue::Choice(smoothing))
                .unwrap();
            study
                .set_input("smoothing_length", InputValue::Length(3))
                .unwrap();
            let mut tick = fixture;
            tick[3] = (5.0, 9.0);
            study.calculate(&bars(&tick));
            study.update(&bars(&fixture), 3);
            assert_eq!(study.values().value(1, 1), None, "{smoothing}: warm-up");
            for (line, want) in (1..).zip(&want) {
                let got = study.values().value(line, 3).unwrap();
                assert!((got - want).abs() < 1e-9, "{smoothing} line {line}: {got}");
            }

            let appended = [fixture.as_slice(), &[(8.0, 5.0)]].concat();
            study.update(&bars(&appended), 4);
            let mut fresh = study.clone();
            fresh.calculate(&bars(&appended));
            assert_eq!(
                format!("{:?}", study.values()),
                format!("{:?}", fresh.values())
            );
        }

        // A length edit reaches the history: the last two values average -60.
        let mut study = WilliamsR::new(1);
        for (id, value) in [
            ("smoothing", InputValue::Choice(1)),
            ("smoothing_length", InputValue::Length(2)),
        ] {
            study.set_input(id, value).unwrap();
        }
        study.calculate(&bars(&fixture));
        assert!((study.values().value(1, 3).unwrap() + 60.0).abs() < 1e-9);
    }

    /// A volume-weighted average over bars without volume is undefined, not zero or infinite.
    #[test]
    fn the_volume_weighted_average_needs_volume() {
        let mut study = WilliamsR::new(1);
        for (id, value) in [
            ("smoothing", InputValue::Choice(6)),
            ("smoothing_length", InputValue::Length(2)),
        ] {
            study.set_input(id, value).unwrap();
        }
        study.calculate(&bars(&[(9.0, 0.0), (7.0, 0.0), (6.0, 3.0)]));
        assert_eq!(study.plot_value(1, 1), None);
        assert!((study.plot_value(1, 2).unwrap() + 40.0).abs() < 1e-9);
    }
}
