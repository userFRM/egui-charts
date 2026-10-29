//! Volume classified by the direction of host-supplied lower-timeframe candles.
//!
//! A rising candle contributes to up volume; a falling candle to down volume. When open
//! equals close, a close at or above the previous intrabar close counts as up, otherwise
//! down. Up volume is positive, down volume negative and delta their sum. Missing intrabar
//! history has no value; the parent candle's volume is never substituted for it.

use crate::model::LineStyle;
use crate::model::{Bar, BarClock, Timeframe};
use crate::scales::StudyFormat;
use crate::studies::palette;
use crate::studies::{
    Indicator, IndicatorValues, PlotStyle, PlotType, StudyData, StudyDataRequest,
};
use egui::Color32;

/// Up and down volume from intrabars supplied by the host, in a separate pane.
#[derive(Clone)]
pub struct UpDownVolume {
    clock: BarClock,
    values: IndicatorValues,
    colors: [Color32; 3],
    visible: bool,
}

impl UpDownVolume {
    /// One-minute intrabars on a one-minute chart until the chart supplies its interval.
    /// Hosts answer [`Indicator::data_requests`] through the chart or `update_data`.
    pub fn new() -> Self {
        Self {
            clock: BarClock::new(Timeframe::Min1),
            values: IndicatorValues::new(),
            colors: [
                palette::UP.gamma_multiply(0.35),
                palette::DOWN.gamma_multiply(0.35),
                palette::GRAY,
            ],
            visible: true,
        }
    }

    fn lower_timeframe(&self) -> Option<Timeframe> {
        let tf = self.clock.timeframe()?;
        if matches!(tf, Timeframe::Ticks(_)) || tf.duration_ms() < Timeframe::Sec1.duration_ms() {
            return None;
        }
        let lower = if tf.duration_ms() < Timeframe::Min1.duration_ms() {
            Timeframe::Sec1
        } else if tf.duration_ms() < Timeframe::Day1.duration_ms() {
            Timeframe::Min1
        } else if tf == Timeframe::Day1 {
            Timeframe::Min5
        } else {
            Timeframe::Hour1
        };
        Some(if tf.duration_ms() % lower.duration_ms() == 0 {
            lower
        } else {
            Timeframe::Sec1
        })
    }
    fn calculate_intrabars(&mut self, data: &[Bar], intrabars: &[Bar], from: usize) {
        let from = from.min(self.values.len()).min(data.len());
        if from == 0 || intrabars.is_empty() {
            self.values = IndicatorValues::missing(data.len());
        } else {
            self.values.truncate(from);
            self.values
                .resize(data.len(), crate::studies::IndicatorValue::None);
        }
        if intrabars.is_empty() {
            return;
        }
        let Some(tf) = self
            .clock
            .timeframe()
            .filter(|tf| !matches!(tf, Timeframe::Ticks(_)))
        else {
            return;
        };
        let daily = tf.duration_ms() % Timeframe::Day1.duration_ms() == 0;
        let sessions = self.clock.for_timeframe(Timeframe::Min1).sessions();
        let key = |bar: &Bar| {
            if daily {
                BarClock::new(tf).bar_open(
                    sessions
                        .trading_date(bar.time)
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc(),
                )
            } else {
                self.clock.bar_open(bar.time)
            }
        };
        let parent = |bar: &Bar| {
            if daily {
                BarClock::new(tf).bar_open(bar.time)
            } else {
                self.clock.bar_open(bar.time)
            }
        };
        let mut at = data.get(from).map_or(0, |bar| {
            intrabars.partition_point(|intrabar| key(intrabar) < parent(bar))
        });
        let mut previous = at.checked_sub(1).map(|i| intrabars[i].close);
        for (i, bar) in data.iter().enumerate().skip(from) {
            let parent = parent(bar);
            let (mut up, mut down, mut count) = (0.0, 0.0, 0);
            while let Some(intrabar) = intrabars.get(at).filter(|b| key(b) <= parent) {
                let rising = intrabar.close > intrabar.open
                    || (intrabar.close == intrabar.open
                        && previous.is_none_or(|close| intrabar.close >= close));
                if key(intrabar) == parent {
                    if rising {
                        up += intrabar.volume;
                    } else {
                        down -= intrabar.volume;
                    }
                    count += 1;
                }
                previous = Some(intrabar.close);
                at += 1;
            }
            if count > 0 && bar.close.is_finite() {
                for (line, value) in [up, down, up + down].into_iter().enumerate() {
                    self.values.set_value(line, i, value);
                }
            }
        }
    }
}

impl Default for UpDownVolume {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for UpDownVolume {
    fn name(&self) -> &str {
        "Up/Down Volume"
    }
    fn desc(&self) -> &str {
        "Up and down volume from lower-timeframe candles supplied by the host"
    }
    fn data_requests(&self) -> Vec<StudyDataRequest> {
        self.lower_timeframe()
            .map(|timeframe| StudyDataRequest {
                metric: None,
                symbol: String::new(),
                timeframe: Some(timeframe),
            })
            .into_iter()
            .collect()
    }
    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
        let from = from.min(self.values.len()).min(data.len());
        let request = self.data_requests().pop();
        let source = sources
            .iter()
            .find(|source| Some(&source.request) == request.as_ref());
        let intrabars = if let Some(source) = source {
            if Some(source.timeframe) != self.lower_timeframe()
                || (from == 0
                    && (source
                        .bars
                        .windows(2)
                        .any(|pair| pair[0].time >= pair[1].time)
                        || source.bars.iter().any(|bar| {
                            !bar.open.is_finite()
                                || !bar.close.is_finite()
                                || !bar.volume.is_finite()
                                || bar.volume < 0.0
                        })))
            {
                return;
            }
            source.bars
        } else {
            &[]
        };
        self.calculate_intrabars(data, intrabars, from);
    }
    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.set_bar_clock(&self.clock.for_timeframe(timeframe));
    }
    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let changed = self.clock != *clock;
        self.clock = clock.clone();
        changed
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.calculate_intrabars(data, &[], 0);
    }
    fn is_incremental(&self) -> bool {
        true
    }
    fn is_overlay(&self) -> bool {
        false
    }
    fn value_format(&self) -> StudyFormat {
        StudyFormat::Volume
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: LineStyle::Solid,
            plot_type: if line == 2 {
                PlotType::Crosses
            } else {
                PlotType::Columns
            },
            visible: true,
        }
    }
    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        (line == 2)
            .then(|| {
                self.values.value(2, bar).map(|delta| {
                    let [r, g, b, _] =
                        self.colors[usize::from(delta < 0.0)].to_srgba_unmultiplied();
                    Color32::from_rgb(r, g, b)
                })
            })
            .flatten()
    }
    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
    }
    fn is_visible(&self) -> bool {
        self.visible
    }
    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
    fn line_names(&self) -> Vec<String> {
        ["Up Volume", "Down Volume", "Delta"]
            .map(String::from)
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Bar;
    use crate::studies::*;
    use chrono::{TimeZone, Utc};

    /// Candle direction, doji ties across parent bars and missing intrabar history stay distinct.
    #[test]
    fn up_down_volume_uses_host_intrabar_directions() {
        use crate::model::{BarClock, Timeframe};
        use crate::studies::{StudyData, StudyDataRequest};
        let requested_interval = |study: &dyn Indicator| {
            study
                .data_requests()
                .first()
                .and_then(|request| request.timeframe)
        };
        let update = |study: &mut dyn Indicator, data: &[Bar], timeframe, bars: &[Bar]| {
            study.update_data(
                data,
                0,
                &[StudyData {
                    request: StudyDataRequest {
                        metric: None,
                        symbol: String::new(),
                        timeframe: Some(timeframe),
                    },
                    bars,
                    clock: BarClock::new(timeframe),
                    instrument: None,
                    timeframe,
                }],
            );
        };
        let mut study = ConfiguredStudy::new(
            IndicatorFactory::new()
                .create("Up/Down Volume")
                .expect("Up/Down Volume study"),
        );
        study.set_timeframe(Timeframe::Day1);
        assert_eq!(requested_interval(&study), Some(Timeframe::Min5));
        study.set_bar_clock(&BarClock::new(Timeframe::Min5));
        assert_eq!(requested_interval(&study), Some(Timeframe::Min1));
        study.set_bar_clock(&BarClock::new(Timeframe::Ms100));
        assert_eq!(requested_interval(&study), None);
        study.set_bar_clock(&BarClock::new(Timeframe::Min5));
        let bars: Vec<_> = [0, 5, 10, 15]
            .into_iter()
            .map(|m| {
                Bar::new(
                    Utc.timestamp_opt(m * 60, 0).unwrap(),
                    10.0,
                    10.0,
                    10.0,
                    10.0,
                    999.0,
                )
            })
            .collect();
        let intrabars: Vec<_> = [
            (0, 10.0, 11.0, 10.0),
            (1, 11.0, 10.0, 20.0),
            (2, 10.0, 10.0, 30.0),
            (5, 9.0, 9.0, 40.0),
            (6, 9.0, 9.0, 50.0),
            (15, 12.0, 13.0, 0.0),
        ]
        .into_iter()
        .map(|(m, o, c, v)| {
            Bar::new(
                Utc.timestamp_opt(m * 60, 0).unwrap(),
                o,
                o.max(c),
                o.min(c),
                c,
                v,
            )
        })
        .collect();
        update(&mut study, &bars, Timeframe::Min1, &intrabars);
        // First parent: up=10+30, down=-20, delta=20. Next: -40+50=10.
        for (i, want) in [
            [Some(40.0), Some(-20.0), Some(20.0)],
            [Some(50.0), Some(-40.0), Some(10.0)],
            [None; 3],
            [Some(0.0); 3],
        ]
        .into_iter()
        .enumerate()
        {
            for (line, expected) in want.into_iter().enumerate() {
                assert_eq!(
                    study.values().value(line, i),
                    expected,
                    "bar {i}, line {line}"
                );
            }
        }
        // A stale interval is unavailable; malformed snapshots leave the accepted values intact.
        update(&mut study, &bars, Timeframe::Min5, &intrabars);
        assert_eq!(study.values().value(0, 0), None);
        update(&mut study, &bars, Timeframe::Min1, &intrabars);
        for invalid in [
            {
                let mut invalid = intrabars.clone();
                invalid[1].time = invalid[0].time;
                invalid
            },
            {
                let mut invalid = intrabars.clone();
                invalid[0].volume = -1.0;
                invalid
            },
            {
                let mut invalid = intrabars.clone();
                invalid[0].open = f64::NAN;
                invalid
            },
        ] {
            update(&mut study, &bars, Timeframe::Min1, &invalid);
            assert_eq!(study.values().value(2, 0), Some(20.0));
        }
        let mut corrected = intrabars;
        corrected[0].volume = 15.0;
        update(&mut study, &bars, Timeframe::Min1, &corrected);
        assert_eq!(study.values().value(2, 0), Some(25.0));
        update(&mut study, &bars, Timeframe::Min1, &[]);
        assert_eq!(study.values().value(0, 0), None);
        // Custom intervals must nest their intrabars without rounding long bars to dates.
        for (seconds, lower) in [(90, Timeframe::Sec1), (90_000, Timeframe::Hour1)] {
            let timeframe = Timeframe::Custom(std::num::NonZeroU64::new(seconds).unwrap());
            study.set_timeframe(timeframe);
            assert_eq!(requested_interval(&study), Some(lower));
            let bar = |second| {
                Bar::new(
                    Utc.timestamp_opt(second, 0).unwrap(),
                    1.0,
                    2.0,
                    1.0,
                    2.0,
                    1.0,
                )
            };
            let seconds = seconds as i64;
            update(
                &mut study,
                &[bar(0), bar(seconds)],
                lower,
                &[0, seconds - lower.duration_ms() / 1000, seconds].map(bar),
            );
            assert_eq!(study.values().value(0, 0), Some(2.0));
            assert_eq!(study.values().value(0, 1), Some(1.0));
        }
        #[cfg(feature = "widget")]
        {
            use crate::model::{BarData, Instrument};
            use crate::widget::{Chart, test_frames::frame};
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            chart.set_timeframe(Timeframe::Min5);
            chart.set_symbol("OLD");
            chart.set_instrument(Some(Instrument::new("OLD")));
            chart.add_indicator(Box::new(UpDownVolume::new()));
            chart.set_symbol("NEW");
            chart.set_instrument(Some(Instrument::new("NEW")));
            let request = chart.study_data_requests().pop().unwrap();
            assert_eq!(request.symbol, "NEW");
            chart.set_study_data(request, vec![bars[0].clone()], Some(Instrument::new("NEW")));
            frame(&egui::Context::default(), &mut chart, 0.0, Vec::new());
            assert_eq!(
                chart.indicators().next().unwrap().1.values().value(0, 0),
                Some(999.0)
            );
        }
    }
}
