//! Percentage change over standard calendar periods, using the chart's own history.
//!
//! Each reference is the last finite close on or before the same trading date one week,
//! one, three or six months, or one year earlier. Month subtraction clamps to the last day
//! of a shorter month. Year to date uses the last close of the previous calendar year.
//! Dates follow the instrument's trading calendar, as VWAP does. Missing history and a zero
//! reference close have no value. No daily history is fetched for an intraday chart.

use super::anchor_period::ExchangeDays;
use crate::model::{Bar, Instrument};
use crate::studies::{Indicator, IndicatorValues, PlotStyle, StudyCell, StudyFormat, StudyTable};
use crate::tokens::DESIGN_TOKENS;
use chrono::{Datelike, Days, Months, NaiveDate};
use egui::Color32;

const PERIODS: [&str; 6] = ["1W", "1M", "3M", "6M", "YTD", "1Y"];

/// Latest close's percentage change over six calendar periods, shown as a table.
#[derive(Clone)]
pub struct Performance {
    days: ExchangeDays,
    values: IndicatorValues,
    table: StudyTable,
    colors: [Color32; 6],
    visible: bool,
}

impl Performance {
    /// Performance over the chart's loaded bars, in UTC until an instrument is supplied.
    pub fn new() -> Self {
        Self {
            days: ExchangeDays::default(),
            values: IndicatorValues::new(),
            table: StudyTable {
                title: "Performance".into(),
                columns: vec!["Period".into(), "Change".into()],
                rows: Vec::new(),
            },
            colors: [DESIGN_TOKENS.semantic.extended.info; 6],
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

    fn calculate(&mut self, data: &[Bar]) {
        self.values = IndicatorValues::missing(data.len());
        let dates = self.days.of(data);
        let mut history = Vec::new();
        let mut latest = [None; 6];
        for (i, (bar, date)) in data.iter().zip(dates).enumerate() {
            if !bar.close.is_finite() {
                continue;
            }
            let targets = [
                date.checked_sub_days(Days::new(7)),
                date.checked_sub_months(Months::new(1)),
                date.checked_sub_months(Months::new(3)),
                date.checked_sub_months(Months::new(6)),
                NaiveDate::from_ymd_opt(date.year(), 1, 1).and_then(|day| day.pred_opt()),
                date.checked_sub_months(Months::new(12)),
            ];
            for (line, target) in targets.into_iter().enumerate() {
                latest[line] = target.and_then(|target| {
                    let at = history
                        .partition_point(|(day, _): &(NaiveDate, f64)| *day <= target)
                        .checked_sub(1)?;
                    let reference = history[at].1;
                    let change = 100.0 * (bar.close - reference) / reference;
                    (reference != 0.0 && change.is_finite()).then_some(change)
                });
                self.values
                    .set_value(line, i, latest[line].unwrap_or(f64::NAN));
            }
            history.push((date, bar.close));
        }
        self.table.rows = PERIODS
            .into_iter()
            .zip(latest)
            .map(|(period, value)| vec![StudyCell::Text(period.into()), StudyCell::Percent(value)])
            .collect();
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
