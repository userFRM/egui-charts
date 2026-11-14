//! Seasonality
//!
//! Projects each month from how the same calendar month moved in earlier years: the month's
//! historical average percentage change, applied to the previous month's close, marks where
//! the month would close if it behaved like an average year.
//!
//! # Calculation
//! The change of a month is `(close of its last bar / close of the last bar before it - 1) * 100`.
//! For every bar of month `m` in year `y`, over the changes of month `m` in the complete years
//! from the starting year to `y - 1`:
//!
//! - Average = mean of the changes
//! - Standard deviation = population standard deviation of the changes
//! - Months = number of changes
//! - Expected = previous month's close * (1 + Average / 100)
//!
//! The values are constant within a month, so the Expected and Previous close lines draw a box
//! over the month from the previous close to the projected close, filled in the positive
//! colour when the average change is zero or more and in the negative colour otherwise. A month
//! without a previous close or without a change in an earlier year has no value. Months are
//! calendar months of the bar's UTC date, the date a feed gives a daily or longer bar.
//!
//! The heatmap includes each loaded year and monthly average, population standard deviation
//! and percentage positive. Its current month uses the latest close; projections use only
//! earlier years. The final projection extends through the remaining calendar-month slots
//! of the time axis, without adding bars to the source history.
//!
//! # Example
//! ```
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, Seasonality};
//!
//! // Month-end closes: January 2023 rises 10% from December 2022.
//! let bars: Vec<Bar> = [(2022, 12, 100.0), (2023, 1, 110.0), (2023, 12, 200.0), (2024, 1, 201.0)]
//!     .into_iter()
//!     .map(|(y, m, c)| Bar::new(Utc.with_ymd_and_hms(y, m, 28, 0, 0, 0).unwrap(), c, c, c, c, 1.0))
//!     .collect();
//! let mut study = Seasonality::new();
//! study.calculate(&bars);
//! // January 2024 projects 10% over the December 2023 close.
//! assert!((study.values().value(0, 3).unwrap() - 220.0).abs() < 1e-9);
//! assert_eq!(study.values().value(1, 3), Some(200.0));
//! assert_eq!(study.values().value(4, 3), Some(1.0));
//! ```

use crate::model::Bar;
use crate::model::LineStyle;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, PlotStyle, PlotType, StudyCell, StudyProjection,
    StudyTable,
};
use chrono::{Datelike, Months};
use egui::Color32;
use std::collections::BTreeMap;

/// Monthly projection from the average change of the same month in earlier years, drawn
/// on the price chart.
#[derive(Clone)]
pub struct Seasonality {
    start_year: i32,
    values: IndicatorValues,
    stepper: Stepper<State>,
    history: [Vec<(i32, f64)>; 12],
    year_counts: BTreeMap<i32, usize>,
    before_last_history: [usize; 12],
    before_last_year: Option<i32>,
    table: StudyTable,
    projection: Vec<StudyProjection>,
    /// One per line: Expected, Previous close, Average, Standard deviation, Months.
    colors: [Color32; 5],
    /// Expected line and box colour of a month whose average change is negative.
    negative: Color32,
    visible: bool,
}

#[derive(Clone)]
struct State {
    source: usize,
    latest_source: Option<usize>,
    month: Option<((i32, u32), f64)>,
    previous_close: Option<f64>,
    row: [f64; 5],
}

impl Default for State {
    fn default() -> Self {
        Self {
            source: 0,
            latest_source: None,
            month: None,
            previous_close: None,
            row: [f64::NAN; 5],
        }
    }
}

impl Seasonality {
    /// Seasonality over every loaded year.
    pub fn new() -> Self {
        let gray = palette::GRAY;
        Self {
            start_year: 1900,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            history: Default::default(),
            year_counts: BTreeMap::new(),
            before_last_history: [0; 12],
            before_last_year: None,
            table: StudyTable {
                title: "Seasonality".into(),
                columns: [
                    "Year", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct",
                    "Nov", "Dec",
                ]
                .map(String::from)
                .to_vec(),
                rows: Vec::new(),
            },
            projection: Vec::new(),
            colors: [palette::UP, gray, gray, gray, gray],
            negative: Color32::from_rgb(242, 55, 69),
            visible: true,
        }
    }

    /// Counts the changes of months from `year` on.
    pub fn with_start_year(mut self, year: i32) -> Self {
        self.start_year = year;
        self
    }

    /// Colours of a month whose average change is positive or zero, and negative.
    pub fn with_colors(mut self, positive: Color32, negative: Color32) -> Self {
        self.colors[0] = positive;
        self.negative = negative;
        self
    }
}

impl Default for Seasonality {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for Seasonality {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        start_year: integer("Starting year", 0, 9999)
    ; {});

    fn name(&self) -> &str {
        "Seasonality"
    }

    fn desc(&self) -> &str {
        "Seasonality - Monthly projection from the average change of the month in past years"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let len = self.values.len();
        if len > 0 && from == len && data.len() == len {
            return;
        }
        let start = if len > 0 && from >= len - 1 && data.len() >= len {
            from.min(len)
        } else {
            0
        };
        if start == 0 {
            for history in &mut self.history {
                history.clear();
            }
            self.year_counts.clear();
        } else if start < len {
            for (history, &len) in self.history.iter_mut().zip(&self.before_last_history) {
                history.truncate(len);
            }
            if let Some(year) = self.before_last_year {
                let count = self.year_counts.get_mut(&year).unwrap();
                *count -= 1;
                if *count == 0 {
                    self.year_counts.remove(&year);
                }
            }
        }
        self.before_last_year = None;
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let source = state.source;
                state.source += 1;
                let last = source + 1 == data.len();
                if last {
                    self.before_last_history = self.history.each_ref().map(Vec::len);
                    self.before_last_year = None;
                }
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                state.latest_source = Some(source);
                let date = bar.time.date_naive();
                let key = (date.year(), date.month0());
                if key.0 >= self.start_year {
                    *self.year_counts.entry(key.0).or_default() += 1;
                    if last {
                        self.before_last_year = Some(key.0);
                    }
                }
                let new_month = state.month.is_none_or(|(other, _)| other != key);
                if let Some((done, last)) = state.month.filter(|(other, _)| *other != key) {
                    let change = state.previous_close.map(|base| (last / base - 1.0) * 100.0);
                    if let Some(change) =
                        change.filter(|c| c.is_finite() && done.0 >= self.start_year)
                    {
                        self.history[done.1 as usize].push((done.0, change));
                    }
                    state.previous_close = Some(last);
                }
                state.month = Some((key, bar.close));
                if new_month {
                    state.row = [f64::NAN; 5];
                    if let Some(base) = state.previous_close {
                        let past: Vec<f64> = self.history[key.1 as usize]
                            .iter()
                            .filter(|(year, _)| *year < key.0)
                            .map(|&(_, change)| change)
                            .collect();
                        if !past.is_empty() {
                            let n = past.len() as f64;
                            let average = past.iter().sum::<f64>() / n;
                            let variance =
                                past.iter().map(|c| (c - average).powi(2)).sum::<f64>() / n;
                            state.row = [
                                base * (1.0 + average / 100.0),
                                base,
                                average,
                                variance.sqrt(),
                                n,
                            ];
                        }
                    }
                }
                IndicatorValue::Multiple(state.row.to_vec())
            });
        if !data.is_empty() {
            self.values.set_value(4, 0, f64::NAN);
        }
        let state = self.stepper.state();
        let developing = state
            .month
            .filter(|((year, _), _)| *year >= self.start_year)
            .and_then(|((year, month), last)| {
                state
                    .previous_close
                    .map(|base| (last / base - 1.0) * 100.0)
                    .filter(|c| c.is_finite())
                    .map(|change| (month as usize, year, change))
            });
        let mut years: BTreeMap<i32, [Option<f64>; 12]> = self
            .year_counts
            .keys()
            .map(|&year| (year, [None; 12]))
            .collect();
        let mut summaries = [Vec::new(), Vec::new(), Vec::new()];
        for (month, history) in self.history.iter().enumerate() {
            let current = developing
                .filter(|&(at, _, _)| at == month)
                .map(|(_, year, change)| (year, change));
            let changes = || history.iter().copied().chain(current);
            for (year, change) in changes() {
                years.entry(year).or_insert([None; 12])[month] = Some(change);
            }
            let n = changes().count() as f64;
            let mean = changes().map(|(_, change)| change).sum::<f64>() / n;
            let deviation = (changes()
                .map(|(_, change)| (change - mean).powi(2))
                .sum::<f64>()
                / n)
                .sqrt();
            let positive = changes().filter(|(_, change)| *change > 0.0).count() as f64 / n * 100.0;
            for (row, value) in summaries.iter_mut().zip([mean, deviation, positive]) {
                row.push(StudyCell::Percent(value.is_finite().then_some(value)));
            }
        }
        self.table.rows = years
            .into_iter()
            .map(|(year, changes)| {
                std::iter::once(StudyCell::Text(year.to_string()))
                    .chain(changes.map(StudyCell::Percent))
                    .collect()
            })
            .collect();
        for (label, cells) in ["Average", "Standard deviation", "Positive"]
            .into_iter()
            .zip(summaries)
        {
            self.table.rows.push(
                std::iter::once(StudyCell::Text(label.into()))
                    .chain(cells)
                    .collect(),
            );
        }
        self.projection.clear();
        if let Some(i) = self.stepper.state().latest_source
            && let (Some(expected), Some(base)) = (self.values.value(0, i), self.values.value(1, i))
        {
            let last = &data[i];
            if let Some(end) = last
                .time
                .date_naive()
                .with_day(1)
                .and_then(|day| day.checked_add_months(Months::new(1)))
                .and_then(|day| day.and_hms_opt(0, 0, 0))
            {
                // Match the time axis: interpolate inside history, extrapolate its last spacing.
                let right = data
                    .partition_point(|bar| bar.time < end.and_utc())
                    .min(data.len() - 1);
                let left = data[..right]
                    .iter()
                    .rposition(|bar| bar.time < data[right].time)
                    .unwrap_or(right);
                let seconds = |duration: chrono::TimeDelta| {
                    duration.num_seconds() as f64 + duration.subsec_nanos() as f64 / 1e9
                };
                let step = if left == right {
                    1.0
                } else {
                    seconds(data[right].time - data[left].time) / (right - left) as f64
                };
                let to = right as f64 + seconds(end.and_utc() - data[right].time) / step;
                if to.is_finite() && to > i as f64 {
                    self.projection.push(StudyProjection {
                        bars: i as f64..to,
                        top: expected.max(base),
                        bottom: expected.min(base),
                        plot: 0,
                    });
                }
            }
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn table(&self) -> Option<&StudyTable> {
        Some(&self.table)
    }

    fn projections(&self) -> &[StudyProjection] {
        &self.projection
    }

    fn projection_fill_color(&self, projection: &StudyProjection) -> Color32 {
        let average = self
            .values
            .value(2, projection.bars.start as usize)
            .unwrap_or(0.0);
        let alpha = ((0.1 + 0.4 * (average.abs() / 10.0).min(1.0)) * 255.0).round() as u8;
        (if average < 0.0 {
            self.negative
        } else {
            self.colors[0]
        })
        .gamma_multiply_u8(alpha)
    }

    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: if line < 2 {
                LineStyle::Dashed
            } else {
                LineStyle::Solid
            },
            plot_type: PlotType::StepLine,
            visible: line < 2,
        }
    }

    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        (line == 0 && self.values.value(2, bar)? < 0.0).then_some(self.negative)
    }

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        let tint =
            |color: Color32| Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 51);
        vec![crate::studies::StudyFill {
            first: crate::studies::FillBoundary::Plot(0),
            second: crate::studies::FillBoundary::Plot(1),
            color: tint(self.colors[0]),
            inverted_color: Some(tint(self.negative)),
        }]
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
        [
            "Expected",
            "Previous close",
            "Average",
            "Standard deviation",
            "Months",
        ]
        .map(String::from)
        .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Bar;
    use crate::studies::*;
    use chrono::{TimeZone, Utc};

    /// Latest table metrics include the developing month; projections use earlier years only.
    #[test]
    fn seasonality_heatmap_and_remaining_month_use_calendar_changes() {
        let bars: Vec<_> = [
            (2022, 12, 28, 100.0),
            (2023, 1, 28, 110.0),
            (2023, 2, 28, 99.0),
            (2023, 12, 28, 200.0),
            (2024, 1, 15, 210.0),
        ]
        .into_iter()
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
        .collect();
        let mut study = ConfiguredStudy::new(Box::new(Seasonality::new()));
        study.set_bar_clock(&crate::model::BarClock::new(crate::model::Timeframe::Day1));
        study.calculate(&bars);
        let table = study.table().expect("Seasonality heatmap");
        let percent = |label: &str, month| {
            let row = table
                .rows
                .iter()
                .find(|row| row.first() == Some(&StudyCell::Text(label.into())))
                .unwrap();
            let StudyCell::Percent(value) = row[month] else {
                panic!("percentage cell")
            };
            value
        };
        for (label, month, expected) in [
            ("2023", 1, 10.0),
            ("2023", 2, -10.0),
            ("2024", 1, 5.0),
            ("Average", 1, 7.5),
            ("Standard deviation", 1, 2.5),
            ("Positive", 1, 100.0),
        ] {
            assert!(
                (percent(label, month).unwrap() - expected).abs() < 1e-9,
                "{label}"
            );
        }
        assert_eq!(percent("2022", 12), None);
        assert_eq!(percent("2024", 2), None);
        let zone = study
            .projections()
            .last()
            .expect("remaining month projection");
        assert_eq!(zone.bars.start, 4.0);
        // The last two bars are 18 days apart. February 1 is 17/18 of a slot after January 15.
        assert!((zone.bars.end - (4.0 + 17.0 / 18.0)).abs() < 1e-10);
        assert!((zone.top - 220.0).abs() < 1e-9);
        assert_eq!(zone.bottom, 200.0);
        #[cfg(feature = "widget")]
        {
            use crate::chart::{coords::ChartMapping, indicators::hit_test_indicator};
            use crate::model::BarData;
            use crate::studies::ChartState;
            use egui::{Rect, pos2, vec2};

            let mut bars = bars;
            bars.extend(
                (16..=31).map(|day| {
                    Bar::whitespace(Utc.with_ymd_and_hms(2024, 1, day, 0, 0, 0).unwrap())
                }),
            );
            study.calculate(&bars);
            let mut state = ChartState::new(BarData::from_bars(bars));
            state.time_scale_mut().set_width(80.0);
            state.time_scale_mut().set_bar_spacing(8.0);
            state.time_scale_mut().set_right_offset(0.0);
            let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 200.0));
            for overlay in [true, false] {
                study.set_overlay(overlay);
                state.fit_price_scale([&study as &dyn Indicator]);
                let price = state.price_scale().map(Default::default(), 0.0, 200.0);
                if overlay {
                    assert!(price.range().min <= 200.0 && price.range().max >= 220.0);
                } else {
                    use crate::widget::{IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig};
                    let mut pane =
                        IndicatorPane::with_config(IndicatorPaneConfig::from_study(&study));
                    egui::Context::default()
                        .run_ui(Default::default(), |ui| {
                            let interaction = pane
                                .show_aligned_interactive(
                                    ui,
                                    &study,
                                    &[],
                                    12..21,
                                    IndicatorCoordParams::new(8.0, 0.0, 20, 12),
                                )
                                .unwrap();
                            assert!(interaction.y_min <= 200.0 && interaction.y_max >= 220.0);
                        })
                        .drop_without_applying_deltas();
                    continue;
                }
                let coords = ChartMapping::new(rect, 8.0, 12, 20, 0.0, price);
                assert!(
                    hit_test_indicator(
                        pos2(coords.idx_to_x(16), price.price_to_y(210.0)),
                        &study,
                        0,
                        12..21,
                        &coords,
                        |value| price.price_to_y(value),
                    )
                    .is_some(),
                    "the remaining-month band must be selectable in whitespace"
                );
            }
        }
    }
}
