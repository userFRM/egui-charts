//! Percentage change over standard calendar periods, using the chart's own history.
//!
//! Each reference is the last finite close on or before the same trading date one week,
//! one, three or six months, or one year earlier. Month subtraction clamps to the last day
//! of a shorter month. Year to date uses the last close of the previous calendar year.
//! Dates follow the instrument's trading calendar, as VWAP does. On an interval shorter than a
//! day the study requests the symbol's daily bars and takes the later of the daily close and the
//! chart's own close on or before each date, the daily close on the same date. Missing history
//! and a zero reference close have no value.

use super::anchor_period::ExchangeDays;
use crate::model::{Bar, Instrument, Timeframe};
use crate::scales::StudyFormat;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, PlotStyle, StudyCell, StudyData, StudyDataRequest,
    StudyTable,
};
use chrono::{Datelike, Days, Months, NaiveDate};
use egui::Color32;

const PERIODS: [&str; 6] = ["1W", "1M", "3M", "6M", "YTD", "1Y"];

/// Latest close's percentage change over six calendar periods, shown as a table.
#[derive(Clone)]
pub struct Performance {
    days: ExchangeDays,
    values: IndicatorValues,
    stepper: Stepper<State>,
    history: Vec<(NaiveDate, f64)>,
    before_last_history: usize,
    intraday: bool,
    /// The chart interval, which decides whether daily bars are requested.
    timeframe: Option<Timeframe>,
    /// Trading date and finite close of each requested daily bar, in time order.
    daily: Vec<(NaiveDate, f64)>,
    table: StudyTable,
    colors: [Color32; 6],
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    source: usize,
    previous_date: Option<NaiveDate>,
    intraday: bool,
    latest: [Option<f64>; 6],
}

impl Performance {
    /// Performance over the chart's loaded bars, and on intraday charts over the daily bars the
    /// host supplies for [`Indicator::data_requests`], in UTC until an instrument is supplied.
    pub fn new() -> Self {
        Self {
            days: ExchangeDays::default(),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            history: Vec::new(),
            before_last_history: 0,
            intraday: false,
            timeframe: None,
            daily: Vec::new(),
            table: StudyTable {
                title: "Performance".into(),
                columns: vec!["Period".into(), "Change".into()],
                rows: Vec::new(),
            },
            colors: [palette::BLUE; 6],
            visible: true,
        }
    }
}

impl Default for Performance {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for Performance {
    fn name(&self) -> &str {
        "Performance"
    }
    fn desc(&self) -> &str {
        "Percentage change over standard calendar periods"
    }
    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        self.days.set(instrument)
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.timeframe = Some(timeframe);
        // Daily and longer charts read their own bars; drop daily bars of an earlier interval.
        if self.data_requests().is_empty() {
            self.daily.clear();
        }
    }

    /// The chart symbol's daily bars, on intervals shorter than a day.
    fn data_requests(&self) -> Vec<StudyDataRequest> {
        self.timeframe
            .filter(|timeframe| timeframe.duration_ms() < Timeframe::Day1.duration_ms())
            .map(|_| StudyDataRequest {
                symbol: String::new(),
                timeframe: Some(Timeframe::Day1),
                metric: None,
            })
            .into_iter()
            .collect()
    }

    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
        if from == 0 {
            let request = self.data_requests().pop();
            self.daily = sources
                .iter()
                .find(|source| Some(&source.request) == request.as_ref())
                .map_or_else(Vec::new, |source| {
                    source
                        .bars
                        .iter()
                        .filter(|bar| bar.close.is_finite())
                        .map(|bar| (bar.time.date_naive(), bar.close))
                        .collect()
                });
        }
        self.update(data, from);
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update_data(data, 0, &[]);
    }

    fn update(&mut self, data: &[Bar], mut from: usize) {
        let len = self.values.len();
        let start = if len > 0 && from >= len - 1 && data.len() >= len {
            from.min(len)
        } else {
            0
        };
        if start == 0 {
            self.history.clear();
        } else if start < len {
            self.history.truncate(self.before_last_history);
        }
        loop {
            self.stepper
                .run(data, from, &mut self.values, |state, bar| {
                    let source = state.source;
                    state.source += 1;
                    if source + 1 == data.len() {
                        self.before_last_history = self.history.len();
                    }
                    let date = bar.time.date_naive();
                    state.intraday |= state.previous_date == Some(date);
                    state.previous_date = Some(date);
                    if !bar.close.is_finite() {
                        return IndicatorValue::None;
                    }
                    let date = self.days.day(bar, !self.intraday);
                    let targets = [
                        date.checked_sub_days(Days::new(7)),
                        date.checked_sub_months(Months::new(1)),
                        date.checked_sub_months(Months::new(3)),
                        date.checked_sub_months(Months::new(6)),
                        NaiveDate::from_ymd_opt(date.year(), 1, 1).and_then(|day| day.pred_opt()),
                        date.checked_sub_months(Months::new(12)),
                    ];
                    let before = |history: &[(NaiveDate, f64)], target| {
                        let at = history.partition_point(|(day, _)| *day <= target);
                        at.checked_sub(1).map(|at| history[at])
                    };
                    for (line, target) in targets.into_iter().enumerate() {
                        state.latest[line] = target.and_then(|target| {
                            // The later of the two closes; on the same date the daily one,
                            // since `max_by_key` keeps the last of equal keys.
                            let (_, reference) =
                                [before(&self.history, target), before(&self.daily, target)]
                                    .into_iter()
                                    .flatten()
                                    .max_by_key(|(day, _)| *day)?;
                            let change = 100.0 * (bar.close - reference) / reference;
                            (reference != 0.0 && change.is_finite()).then_some(change)
                        });
                    }
                    self.history.push((date, bar.close));
                    IndicatorValue::Multiple(
                        state.latest.map(|value| value.unwrap_or(f64::NAN)).to_vec(),
                    )
                });
            if self.intraday == self.stepper.state().intraday {
                break;
            }
            self.intraday = self.stepper.state().intraday;
            self.history.clear();
            from = 0;
        }
        self.table.rows = PERIODS
            .into_iter()
            .zip(self.stepper.state().latest)
            .map(|(period, value)| vec![StudyCell::Text(period.into()), StudyCell::Percent(value)])
            .collect();
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn table(&self) -> Option<&StudyTable> {
        Some(&self.table)
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Line,
            visible: false,
        }
    }
    fn value_format(&self) -> StudyFormat {
        StudyFormat::Percent
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
        PERIODS.map(String::from).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Bar;
    use crate::studies::*;
    use chrono::{TimeZone, Utc};

    /// Calendar lookbacks use the last close on or before the target date, not bar counts.
    #[test]
    fn performance_uses_calendar_dates_and_missing_history() {
        let bars: Vec<_> = [
            (2023, 9, 29, 80.0),
            (2023, 12, 29, 100.0),
            (2024, 2, 28, 120.0),
            (2024, 2, 29, 125.0),
            (2024, 3, 22, 150.0),
            (2024, 3, 31, 180.0),
        ]
        .map(|(y, m, d, c)| {
            Bar::new(
                Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap(),
                c,
                c,
                c,
                c,
                1.0,
            )
        })
        .to_vec();
        let mut study = IndicatorFactory::new()
            .create("Performance")
            .expect("Performance study");
        study.calculate(&bars);
        // March 24 -> March 22: 180/150-1 = 20%; February 29: 44%;
        // December 31 -> December 29: 80%; September 30 -> September 29: 125%.
        for (line, expected) in [
            Some(20.0),
            Some(44.0),
            Some(80.0),
            Some(125.0),
            Some(80.0),
            None,
        ]
        .into_iter()
        .enumerate()
        {
            let got = study.values().value(line, bars.len() - 1);
            match expected {
                Some(expected) => assert!(
                    (got.unwrap() - expected).abs() < 1e-10,
                    "line {line}: {got:?}"
                ),
                None => assert_eq!(got, None, "line {line}"),
            }
        }
        #[cfg(feature = "widget")]
        {
            use crate::widget::{Chart, test_frames::shapes};
            let ctx = egui::Context::default();
            let mut chart = Chart::new(crate::model::BarData::from_bars(bars));
            let id = chart.add_indicator(study);
            let painted = shapes(&ctx, &mut chart, 0.0, Vec::new());
            for expected in ["Change", "1W", "+44.00%", "N/A"] {
                assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::Text(text) if text.galley.text() == expected)), "missing table cell {expected}");
            }
            chart.indicator_mut(id).unwrap().set_visible(false);
            let painted = shapes(&ctx, &mut chart, 1.0, Vec::new());
            assert!(!painted.iter().any(
                |shape| matches!(shape, egui::Shape::Text(text) if text.galley.text() == "Change")
            ));
        }
    }

    /// On an intraday chart each period starts from the requested daily bars, the daily close
    /// winning on the same date, and from the chart's own bars when none are supplied.
    #[test]
    fn performance_takes_period_starts_from_daily_bars() {
        use crate::model::{BarClock, Timeframe};
        let bar = |(y, m, d, h, min, c)| {
            Bar::new(
                Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap(),
                c,
                c,
                c,
                c,
                1.0,
            )
        };
        let daily = [
            (2023, 3, 15, 0, 0, 50.0),
            (2023, 9, 15, 0, 0, 80.0),
            (2023, 12, 14, 0, 0, 90.0),
            (2023, 12, 29, 0, 0, 100.0),
            (2024, 2, 15, 0, 0, 125.0),
            (2024, 3, 8, 0, 0, 150.0),
            // Today's developing bar is never a period start.
            (2024, 3, 15, 0, 0, 999.0),
        ]
        .map(bar);
        let today = [
            (2024, 3, 15, 14, 30, 200.0),
            (2024, 3, 15, 14, 31, 210.0),
            (2024, 3, 15, 14, 32, 180.0),
        ]
        .map(bar);
        let week_before = [&[bar((2024, 3, 8, 20, 0, 160.0))][..], &today].concat();
        let request = StudyDataRequest {
            symbol: String::new(),
            timeframe: Some(Timeframe::Day1),
            metric: None,
        };
        let from_daily = [20.0, 44.0, 100.0, 125.0, 80.0, 260.0].map(Some);
        for (chart, supplied, expected) in [
            (today.to_vec(), true, from_daily),
            (week_before.clone(), true, from_daily),
            (
                week_before,
                false,
                [Some(12.5), None, None, None, None, None],
            ),
        ] {
            let mut study = Performance::new();
            study.set_timeframe(Timeframe::Min1);
            assert_eq!(study.data_requests(), vec![request.clone()]);
            let sources: Vec<_> = supplied
                .then(|| StudyData {
                    request: request.clone(),
                    bars: &daily,
                    clock: BarClock::new(Timeframe::Day1),
                    instrument: None,
                    timeframe: Timeframe::Day1,
                })
                .into_iter()
                .collect();
            study.update_data(&chart, 0, &sources);
            for (line, expected) in expected.into_iter().enumerate() {
                let got = study.values().value(line, chart.len() - 1);
                match expected {
                    Some(expected) => assert!(
                        (got.unwrap() - expected).abs() < 1e-10,
                        "line {line}: {got:?}"
                    ),
                    None => assert_eq!(got, None, "line {line}"),
                }
            }
            study.set_timeframe(Timeframe::Day1);
            assert!(study.data_requests().is_empty());
        }
    }
}
