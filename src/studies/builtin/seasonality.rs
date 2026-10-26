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
use crate::studies::palette;
use crate::studies::{
    Indicator, IndicatorValues, PlotPattern, PlotStyle, PlotType, StudyCell, StudyProjection,
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
    table: StudyTable,
    projection: Vec<StudyProjection>,
    /// One per line: Expected, Previous close, Average, Standard deviation, Months.
    colors: [Color32; 5],
    /// Expected line and box colour of a month whose average change is negative.
    negative: Color32,
    visible: bool,
}

impl Seasonality {
    /// Seasonality over every loaded year.
    pub fn new() -> Self {
        let gray = palette::GRAY;
        Self {
            start_year: 1900,
            values: IndicatorValues::new(),
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
            negative: palette::DOWN,
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
        let mut values = IndicatorValues::missing(data.len());
        if !data.is_empty() {
            // Every bar holds a value for each of the five plots.
            values.set_value(4, 0, f64::NAN);
        }
        // Per calendar month, the year and percentage change of each complete month.
        let mut history: [Vec<(i32, f64)>; 12] = Default::default();
        let mut month: Option<((i32, u32), f64)> = None;
        let mut previous_close: Option<f64> = None;
        for (i, bar) in data.iter().enumerate() {
            if !bar.close.is_finite() {
                continue;
            }
            let date = bar.time.date_naive();
            let key = (date.year(), date.month0());
            if let Some((done, last)) = month.filter(|(other, _)| *other != key) {
                let change = previous_close.map(|base| (last / base - 1.0) * 100.0);
                if let Some(change) = change.filter(|c| c.is_finite() && done.0 >= self.start_year)
                {
                    history[done.1 as usize].push((done.0, change));
                }
                previous_close = Some(last);
            }
            month = Some((key, bar.close));
            let Some(base) = previous_close else {
                continue;
            };
            let past: Vec<f64> = history[key.1 as usize]
                .iter()
                .filter(|(year, _)| *year < key.0)
                .map(|&(_, change)| change)
                .collect();
            if past.is_empty() {
                continue;
            }
            let n = past.len() as f64;
            let average = past.iter().sum::<f64>() / n;
            let variance = past.iter().map(|c| (c - average).powi(2)).sum::<f64>() / n;
            for (line, value) in [
                base * (1.0 + average / 100.0),
                base,
                average,
                variance.sqrt(),
                n,
            ]
            .into_iter()
            .enumerate()
            {
                values.set_value(line, i, value);
            }
        }
        // The developing month's change belongs in the latest table, never in its projection.
        if let Some(((year, month), last)) = month.filter(|((year, _), _)| *year >= self.start_year)
            && let Some(change) = previous_close
                .map(|base| (last / base - 1.0) * 100.0)
                .filter(|c| c.is_finite())
        {
            history[month as usize].push((year, change));
        }
        let mut years: BTreeMap<i32, [Option<f64>; 12]> = data
            .iter()
            .filter(|bar| bar.close.is_finite() && bar.time.year() >= self.start_year)
            .map(|bar| (bar.time.year(), [None; 12]))
            .collect();
        for (month, changes) in history.iter().enumerate() {
            for &(year, change) in changes {
                years.entry(year).or_insert([None; 12])[month] = Some(change);
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
        let mut summaries = [Vec::new(), Vec::new(), Vec::new()];
        for changes in &history {
            let n = changes.len() as f64;
            let mean = changes.iter().map(|(_, change)| change).sum::<f64>() / n;
            let deviation = (changes
                .iter()
                .map(|(_, change)| (change - mean).powi(2))
                .sum::<f64>()
                / n)
                .sqrt();
            let positive =
                changes.iter().filter(|(_, change)| *change > 0.0).count() as f64 / n * 100.0;
            for (row, value) in summaries.iter_mut().zip([mean, deviation, positive]) {
                row.push(StudyCell::Percent(value.is_finite().then_some(value)));
            }
        }
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
        if let Some(i) = data.iter().rposition(|bar| bar.close.is_finite())
            && let (Some(expected), Some(base)) = (values.value(0, i), values.value(1, i))
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
        self.values = values;
    }

    fn table(&self) -> Option<&StudyTable> {
        Some(&self.table)
    }

    fn projections(&self) -> &[StudyProjection] {
        &self.projection
    }

    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: if line == 1 {
                PlotPattern::Dashed
            } else {
                PlotPattern::Solid
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
