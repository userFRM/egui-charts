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
//! - Standard deviation = sample standard deviation of the changes, dividing by one less than
//!   their number; a single change has none
//! - Months = number of changes
//! - Expected = previous month's close * (1 + Average / 100)
//!
//! The values are constant within a month, so the Expected and Previous close lines draw a box
//! over the month from the previous close to the projected close, filled in the positive
//! colour when the average change is zero or more and in the negative colour otherwise. A month
//! without a previous close or without a change in an earlier year has no value. Months are
//! calendar months of the bar's UTC date, the date a feed gives a daily or longer bar.
//!
//! The heatmap includes each loaded year and monthly average, sample standard deviation
//! and percentage positive, an unchanged month counting as positive. Its current month uses the
//! latest close; projections use only earlier years. The final projection extends through the
//! remaining calendar-month slots of the time axis, without adding bars to the source history.
//!
//! A new study shows the heatmap in a pane of its own and keeps the Expected and Previous close
//! lines and the boxes on the price chart; moved to the price pane, it draws everything there.
//!
//! Month and Average cells use the change's sign colour, increasing in opacity up to the
//! cutoff. Positive cells use their distance from 50%; standard deviations have no sign tint.
//!
//! # Inputs
//! Starting year defaults to 2015. Ignored months accepts comma-separated YYYY-MM entries,
//! excluded from averages, deviations, percentages and projections and shown as Skipped.
//! Positive colour and Negative colour, `#089981` and `#F23645` by default, set rising and
//! falling cells, Expected line segments and the remaining-month box. Historical bands keep
//! their own Style tab colours.
//! Colour intensity cutoff (%) defaults to 10. Show averages, Show standard deviation and
//! Show percent positive default to on. Table position defaults to bottom centre; Table
//! width (%) and Table height (%) default to 100 and 95 of the study's pane; 0 fits the cells
//! and rows.
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

use super::performance::{LABEL_FILL, share};
use crate::model::Bar;
use crate::model::LineStyle;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, PlotStyle, PlotType, StudyCell, StudyProjection,
    StudyTable, TablePosition,
};
use chrono::{Datelike, Months};
use egui::Color32;
use std::collections::BTreeMap;

/// Behind a month without a change or one excluded from the calculation.
const EMPTY_FILL: Color32 = Color32::from_rgba_unmultiplied_const(120, 123, 134, 128);

/// Monthly projection from the average change of the same month in earlier years. The
/// projection is drawn on the price chart and the heatmap in the study's own pane.
#[derive(Clone, Debug)]
pub struct Seasonality {
    start_year: i32,
    /// Year and month pairs left out of the calculation, as typed.
    ignored_months: String,
    /// `ignored_months` read as (year, zero-based month).
    ignored: Vec<(i32, u32)>,
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
    /// Colour of a rising or unchanged month's line, box and cells.
    positive_color: Color32,
    /// Colour of a falling month's line, box and cells.
    negative_color: Color32,
    /// The change, in percent, from which a cell or box takes its full colour.
    color_cutoff: f64,
    show_averages: bool,
    show_std_dev: bool,
    show_positive: bool,
    table_position: TablePosition,
    /// The table's width in percent of the pane, 0 to fit its cells.
    table_width: f64,
    /// The table's height in percent of the pane, 0 to fit its rows.
    table_height: f64,
    visible: bool,
}

#[derive(Clone, Debug)]
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
    /// Seasonality from 2015 on.
    pub fn new() -> Self {
        let gray = palette::GRAY;
        Self {
            start_year: 2015,
            ignored_months: String::new(),
            ignored: Vec::new(),
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
                ..Default::default()
            },
            projection: Vec::new(),
            colors: [palette::UP, gray, gray, gray, gray],
            positive_color: palette::UP,
            negative_color: palette::DOWN,
            color_cutoff: 10.0,
            show_averages: true,
            show_std_dev: true,
            show_positive: true,
            table_position: TablePosition::BottomCenter,
            table_width: 100.0,
            table_height: 95.0,
            visible: true,
        }
    }

    /// Counts the changes of months from `year` on.
    pub fn with_start_year(mut self, year: i32) -> Self {
        self.start_year = year;
        self
    }

    /// Colours of rising or unchanged months and of falling months: their Expected line,
    /// remaining-month box and cells.
    pub fn with_colors(mut self, positive: Color32, negative: Color32) -> Self {
        self.colors[0] = positive;
        self.positive_color = positive;
        self.negative_color = negative;
        self
    }
}

impl Default for Seasonality {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads comma-separated YYYY-MM entries as years and zero-based months; empty entries do
/// not exclude anything.
fn ignored_months(list: &str) -> Result<Vec<(i32, u32)>, String> {
    let mut months = Vec::new();
    for entry in list
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        let month = entry
            .split_once('-')
            .filter(|(year, month)| {
                year.len() == 4
                    && month.len() == 2
                    && year
                        .bytes()
                        .chain(month.bytes())
                        .all(|b| b.is_ascii_digit())
            })
            .and_then(|(year, month)| {
                let month: u32 = month.parse().ok()?;
                (1..=12)
                    .contains(&month)
                    .then(|| (year.parse().ok(), month - 1))
            })
            .and_then(|(year, month)| Some((year?, month)))
            .ok_or_else(|| {
                format!("cannot read {entry:?}: write each month as YYYY-MM, such as 2020-03")
            })?;
        months.push(month);
    }
    Ok(months)
}

impl Indicator for Seasonality {
    fn is_overlay(&self) -> bool {
        false
    }

    fn plots_on_price(&self) -> bool {
        true
    }

    /// The plots are prices on every pane.
    fn scale(&self) -> crate::studies::StudyScale {
        crate::studies::StudyScale::Main
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Price
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: Some(super::performance::table_pane_height(&self.table)),
            ..Default::default()
        }
    }

    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        start_year: integer("Starting year", 0, 9999),
        ignored_months: text("Ignored months"),
        positive_color: color("Positive colour"),
        negative_color: color("Negative colour"),
        color_cutoff: number("Colour intensity cutoff (%)", 0.01, f64::MAX),
        show_averages: toggle("Show averages"),
        show_std_dev: toggle("Show standard deviation"),
        show_positive: toggle("Show percent positive"),
        table_position: choice("Table position", TablePosition::ALL),
        table_width: number("Table width (%)", 0.0, 100.0),
        table_height: number("Table height (%)", 0.0, 100.0)
    ; changed, previous => {
        if changed == "ignored_months" {
            match ignored_months(&self.ignored_months) {
                Ok(months) => self.ignored = months,
                Err(reason) => {
                    // The input macro has already assigned the refused text.
                    if let crate::studies::InputValue::Text(text) = previous {
                        self.ignored_months = text;
                    }
                    return Err(crate::studies::StudyError::InvalidInput {
                        id: changed.into(),
                        reason,
                    });
                }
            }
        }
    });

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
                            .filter(|&&(year, _)| {
                                year < key.0 && !self.ignored.contains(&(year, key.1))
                            })
                            .map(|&(_, change)| change)
                            .collect();
                        if !past.is_empty() {
                            let n = past.len() as f64;
                            let average = past.iter().sum::<f64>() / n;
                            let deviation = if past.len() > 1 {
                                (past.iter().map(|c| (c - average).powi(2)).sum::<f64>()
                                    / (n - 1.0))
                                    .sqrt()
                            } else {
                                f64::NAN
                            };
                            state.row =
                                [base * (1.0 + average / 100.0), base, average, deviation, n];
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
        let fill = |value: f64, full: f64| {
            (if value < 0.0 {
                self.negative_color
            } else {
                self.positive_color
            })
            .gamma_multiply((value.abs() / full).min(1.0) as f32)
        };
        let mut summaries = [Vec::new(), Vec::new(), Vec::new()];
        let mut summary_colors = [Vec::new(), Vec::new(), Vec::new()];
        for (month, history) in self.history.iter().enumerate() {
            let current = developing
                .filter(|&(at, _, _)| at == month)
                .map(|(_, year, change)| (year, change));
            for (year, change) in history.iter().copied().chain(current) {
                years.entry(year).or_insert([None; 12])[month] = Some(change);
            }
            let changes = || {
                history
                    .iter()
                    .copied()
                    .chain(current)
                    .filter(|&(year, _)| !self.ignored.contains(&(year, month as u32)))
            };
            let n = changes().count() as f64;
            let mean = changes().map(|(_, change)| change).sum::<f64>() / n;
            let deviation = if n > 1.0 {
                (changes()
                    .map(|(_, change)| (change - mean).powi(2))
                    .sum::<f64>()
                    / (n - 1.0))
                    .sqrt()
            } else {
                f64::NAN
            };
            let positive =
                changes().filter(|(_, change)| *change >= 0.0).count() as f64 / n * 100.0;
            for (index, value) in [mean, deviation, positive].into_iter().enumerate() {
                let value = value.is_finite().then_some(value);
                summaries[index].push(StudyCell::Percent(value));
                summary_colors[index].push(match (index, value) {
                    (_, None) => EMPTY_FILL,
                    (0, Some(value)) => fill(value, self.color_cutoff),
                    (1, Some(_)) => Color32::TRANSPARENT,
                    (_, Some(value)) => fill(value - 50.0, 50.0),
                });
            }
        }
        self.table.rows.clear();
        self.table.cell_colors.clear();
        for (year, changes) in years {
            let mut row = vec![StudyCell::Text(year.to_string())];
            let mut colors = vec![LABEL_FILL];
            for (month, change) in changes.into_iter().enumerate() {
                let (cell, color) = match change {
                    Some(_) if self.ignored.contains(&(year, month as u32)) => {
                        (StudyCell::Text("Skipped".into()), EMPTY_FILL)
                    }
                    Some(change) => (
                        StudyCell::Percent(Some(change)),
                        fill(change, self.color_cutoff),
                    ),
                    None => (StudyCell::Percent(None), EMPTY_FILL),
                };
                row.push(cell);
                colors.push(color);
            }
            self.table.rows.push(row);
            self.table.cell_colors.push(colors);
        }
        for (((label, shown), cells), colors) in [
            ("Average", self.show_averages),
            ("Standard deviation", self.show_std_dev),
            ("Positive", self.show_positive),
        ]
        .into_iter()
        .zip(summaries)
        .zip(summary_colors)
        {
            if shown {
                self.table.rows.push(
                    std::iter::once(StudyCell::Text(label.into()))
                        .chain(cells)
                        .collect(),
                );
                self.table
                    .cell_colors
                    .push(std::iter::once(LABEL_FILL).chain(colors).collect());
            }
        }
        self.table.position = self.table_position;
        self.table.width = share(self.table_width);
        self.table.height = share(self.table_height);
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
                let step = if left == right {
                    1.0
                } else {
                    (data[right].time - data[left].time).as_seconds_f64() / (right - left) as f64
                };
                let to = right as f64 + (end.and_utc() - data[right].time).as_seconds_f64() / step;
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
        let alpha =
            ((0.1 + 0.4 * (average.abs() / self.color_cutoff).min(1.0)) * 255.0).round() as u8;
        (if average < 0.0 {
            self.negative_color
        } else {
            self.positive_color
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
        if line != 0 {
            return None;
        }
        Some(if self.values.value(2, bar)? < 0.0 {
            self.negative_color
        } else {
            self.positive_color
        })
    }

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        let tint =
            |color: Color32| Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 51);
        vec![crate::studies::StudyFill {
            first: crate::studies::FillBoundary::Plot(0),
            second: crate::studies::FillBoundary::Plot(1),
            color: tint(self.positive_color),
            inverted_color: Some(tint(self.negative_color)),
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

    /// The heatmap has its own pane while the projection box and both price plots stay above it.
    #[cfg(feature = "widget")]
    #[test]
    fn seasonality_heatmap_has_its_pane_and_projects_on_the_price_pane() {
        use crate::model::BarData;
        use crate::widget::{Chart, PaneId, test_frames};

        let mut close = 100.0;
        let bars = (0..39)
            .map(|i| {
                let month = i % 12 + 1;
                close *= if month % 2 == 1 { 1.06 } else { 0.97 };
                Bar::new(
                    Utc.with_ymd_and_hms(2021 + i / 12, month as u32, 1, 0, 0, 0)
                        .unwrap(),
                    close,
                    close,
                    close,
                    close,
                    1.0,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let id = chart.add_indicator(Box::new(Seasonality::new()));
        let ctx = egui::Context::default();
        test_frames::shapes(&ctx, &mut chart, 0.0, Vec::new());
        let painted = test_frames::shapes(&ctx, &mut chart, 0.1, Vec::new());
        assert_eq!(chart.panes().len(), 2);
        let pane = chart.series_pane(id).unwrap();
        assert_ne!(pane, PaneId::MAIN);
        let heading_y = painted
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::Text(text) if text.galley.text() == "Year" => Some(text.pos.y),
                _ => None,
            })
            .unwrap();
        let study = chart.indicators().next().unwrap().1;
        let fill = study.projection_fill_color(study.projections().last().unwrap());
        let projection = painted
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::Rect(rect) if rect.fill == fill => Some(rect.rect),
                _ => None,
            })
            .expect("the remaining-month projection is painted");
        assert!(projection.bottom() < heading_y);
        let previous_close: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::LineSegment { points, stroke } if stroke.color == palette::GRAY => {
                    Some(points)
                }
                _ => None,
            })
            .collect();
        assert!(!previous_close.is_empty());
        assert!(
            previous_close
                .iter()
                .all(|points| points.iter().all(|point| point.y < heading_y))
        );
        // Four years and three summary rows: 22 * (7 + 2) + 40.
        assert!((chart.pane_height(pane).unwrap() - 238.0).abs() < 0.5);
        assert_eq!(study.scale(), StudyScale::None);
        let mut options = chart.chart_options().clone();
        options.crosshair.mode = crate::config::CrosshairMode::Magnet;
        options.crosshair.horz_line_color = Color32::GOLD;
        options.crosshair.horz_line_style = crate::model::LineStyle::Solid;
        options.crosshair.vert_line_visible = false;
        chart.set_chart_options(options);
        let pointer = egui::pos2(projection.left(), heading_y - 35.0);
        let hovered = test_frames::shapes(
            &ctx,
            &mut chart,
            0.2,
            vec![egui::Event::PointerMoved(pointer)],
        );
        assert!(
            hovered.iter().any(|shape| matches!(shape,
                egui::Shape::LineSegment { points, stroke }
                if stroke.color == Color32::GOLD
                    && points.iter().all(|point| (point.y - pointer.y).abs() < 0.5)
            )),
            "the heatmap crosshair must not snap to price plots drawn in another pane"
        );
        assert!(chart.move_series_to_pane(id, PaneId::MAIN));
        assert_eq!(chart.panes().len(), 1);
        let study = chart.indicators().next().unwrap().1;
        assert!(study.is_overlay());
        assert_eq!(study.scale(), StudyScale::Main);
    }

    fn heatmap_bars() -> Vec<Bar> {
        [
            (2020, 12, 100.0),
            (2021, 1, 112.0),
            (2021, 3, 114.24),
            (2022, 1, 110.8128),
            (2022, 3, 115.245312),
            (2023, 1, 149.8189056),
        ]
        .into_iter()
        .map(|(year, month, close)| {
            Bar::new(
                Utc.with_ymd_and_hms(year, month, 28, 0, 0, 0).unwrap(),
                close,
                close,
                close,
                close,
                1.0,
            )
        })
        .collect()
    }

    /// User colours reach the cells, falling line and remaining-month box through settings,
    /// and a template round trip retains them.
    #[test]
    fn seasonality_cells_take_the_change_colours_through_settings_and_templates() {
        let bars = heatmap_bars();
        let mut study = ConfiguredStudy::new(Box::new(Seasonality::new()));
        study.calculate(&bars);
        let table = study.table().unwrap();
        let fills = &table.cell_colors;
        assert_eq!(fills.len(), table.rows.len());
        assert!(fills.iter().all(|row| row.len() == table.columns.len()));
        assert_eq!(fills[1][1].to_array(), [8, 153, 129, 255]);
        assert_eq!(fills[2][1].to_array(), [73, 16, 21, 77]);
        assert_eq!(
            fills[0][0],
            Color32::from_rgba_unmultiplied(120, 123, 134, 51)
        );
        let empty = Color32::from_rgba_unmultiplied(120, 123, 134, 128);
        assert_eq!(fills[0][1], empty);
        assert_eq!(fills[5][3], Color32::TRANSPARENT);

        assert_eq!(
            (table.position, table.width, table.height),
            (TablePosition::BottomCenter, Some(1.0), Some(0.95))
        );
        for (id, value) in [
            ("start_year", InputValue::Integer(2015)),
            ("positive_color", InputValue::Color(palette::UP)),
        ] {
            assert_eq!(
                study
                    .inputs()
                    .iter()
                    .find(|input| input.id == id)
                    .map(|input| &input.value),
                Some(&value),
                "{id}"
            );
        }

        // The Expected plot's Style colour is not the positive colour.
        let mut settings = study.settings();
        settings.plots[0].color = Color32::RED;
        study
            .apply_settings(
                &[
                    ("positive_color".into(), InputValue::Color(Color32::GOLD)),
                    ("negative_color".into(), InputValue::Color(palette::BLUE)),
                    ("ignored_months".into(), InputValue::Text("2021-01".into())),
                ],
                settings,
            )
            .unwrap();
        study.calculate(&bars);
        let text = serde_json::to_string(&StudyTemplate::from_study(&study)).unwrap();
        let saved: StudyTemplate = serde_json::from_str(&text).unwrap();
        let mut restored = saved.create().unwrap();
        for study in [&mut study as &mut dyn Indicator, restored.as_mut()] {
            // Through March 2022, whose one earlier March rose 2%: 18% of the positive colour.
            study.calculate(&bars[..5]);
            assert_eq!(
                study
                    .projection_fill_color(study.projections().last().unwrap())
                    .to_array(),
                [46, 39, 0, 46]
            );
            study.calculate(&bars);
            let table = study.table().unwrap();
            assert_eq!(table.cell_colors[2][1].to_array(), [12, 29, 77, 77]);
            assert_eq!(table.cell_colors[3][1], Color32::GOLD);
            assert_eq!(table.cell_colors[1][1], empty);
            assert_eq!(table.rows[1][1], StudyCell::Text("Skipped".into()));
            assert_eq!(study.plot_color(0, 4), Some(Color32::GOLD));
            assert_eq!(study.plot_color(0, 5), Some(palette::BLUE));
            assert_eq!(
                study
                    .projection_fill_color(study.projections().last().unwrap())
                    .to_array(),
                [9, 22, 56, 56]
            );
            assert_eq!(study.settings().plots[0].color, Color32::RED);
        }
    }

    /// Row switches, exclusions and the cutoff affect the calculated heatmap; malformed
    /// exclusions leave its input, values and table intact.
    #[test]
    fn seasonality_inputs_hide_rows_skip_months_and_refuse_malformed_lists() {
        struct Case {
            input: Option<(&'static str, InputValue)>,
            labels: &'static [&'static str],
            average: Option<f64>,
            jan_sd: Option<f64>,
            mar_sd: Option<f64>,
            skipped: bool,
            negative: [u8; 4],
            projection: [u8; 4],
        }
        let defaults = || Case {
            input: None,
            labels: &["Average", "Standard deviation", "Positive"],
            average: Some(13.0),
            jan_sd: Some(16.522711641858304),
            mar_sd: Some(std::f64::consts::SQRT_2),
            skipped: false,
            negative: [73, 16, 21, 77],
            projection: [2, 43, 36, 71],
        };
        let bars = heatmap_bars();
        for case in [
            defaults(),
            Case {
                input: Some(("show_averages", InputValue::Toggle(false))),
                labels: &["Standard deviation", "Positive"],
                average: None,
                ..defaults()
            },
            Case {
                input: Some(("show_std_dev", InputValue::Toggle(false))),
                labels: &["Average", "Positive"],
                jan_sd: None,
                mar_sd: None,
                ..defaults()
            },
            Case {
                input: Some(("show_positive", InputValue::Toggle(false))),
                labels: &["Average", "Standard deviation"],
                ..defaults()
            },
            Case {
                input: Some(("ignored_months", InputValue::Text("2023-01".into()))),
                average: Some(4.5),
                jan_sd: Some(10.606601717798213),
                skipped: true,
                ..defaults()
            },
            Case {
                input: Some((
                    "ignored_months",
                    InputValue::Text("2021-01, 2022-03".into()),
                )),
                average: Some(13.5),
                jan_sd: Some(23.33452377915607),
                mar_sd: None,
                projection: [53, 12, 15, 56],
                ..defaults()
            },
            Case {
                input: Some(("color_cutoff", InputValue::Number(30.0))),
                negative: [24, 5, 7, 26],
                projection: [1, 25, 21, 41],
                ..defaults()
            },
        ] {
            let mut study = Seasonality::new();
            if let Some((id, value)) = case.input {
                study.set_input(id, value).unwrap();
            }
            study.calculate(&bars);
            let table = study.table().unwrap();
            let labels: Vec<_> = table.rows[4..]
                .iter()
                .map(|row| match &row[0] {
                    StudyCell::Text(label) => label.as_str(),
                    _ => panic!("a row label"),
                })
                .collect();
            assert_eq!(labels, case.labels);
            for (label, column, expected) in [
                ("Average", 1, case.average),
                ("Standard deviation", 1, case.jan_sd),
                ("Standard deviation", 3, case.mar_sd),
            ] {
                let got = table
                    .rows
                    .iter()
                    .find(|row| row[0] == StudyCell::Text(label.into()))
                    .and_then(|row| match row[column] {
                        StudyCell::Percent(value) => value,
                        _ => panic!("a summary percentage"),
                    });
                match expected {
                    Some(expected) => assert!(
                        (got.unwrap() - expected).abs() < 1e-9,
                        "{label}, column {column}"
                    ),
                    None => assert_eq!(got, None, "{label}, column {column}"),
                }
            }
            if case.skipped {
                assert_eq!(table.rows[3][1], StudyCell::Text("Skipped".into()));
                assert_eq!(table.rows[6][1], StudyCell::Percent(Some(50.0)));
                assert_eq!(table.cell_colors[6][1], Color32::TRANSPARENT);
            } else {
                let StudyCell::Percent(Some(change)) = table.rows[3][1] else {
                    panic!("January change")
                };
                assert!((change - 30.0).abs() < 1e-9);
            }
            assert_eq!(table.cell_colors[2][1].to_array(), case.negative);
            assert_eq!(
                study
                    .projection_fill_color(study.projections().last().unwrap())
                    .to_array(),
                case.projection
            );
        }
        let mut study = Seasonality::new();
        study
            .set_input(
                "ignored_months",
                InputValue::Text("2021-01, 2022-01, 2023-01".into()),
            )
            .unwrap();
        study.calculate(&bars);
        let table = study.table().unwrap();
        for row in &table.rows[4..] {
            assert_eq!(row[1], StudyCell::Percent(None));
        }
        assert_eq!(study.values().value(4, 5), None);
        assert!(study.projections().is_empty());
        study
            .set_input(
                "ignored_months",
                InputValue::Text("2021-01,, 2021-01,".into()),
            )
            .unwrap();
        study.calculate(&bars);
        let before = study.table().unwrap().clone();
        let values = study.values().clone();
        for text in [
            "2023-13",
            "2023",
            "March 2023",
            "23-01",
            "2023-1",
            "2023-00",
            "2023/01",
            "-2023-01",
        ] {
            assert!(
                matches!(study.set_input("ignored_months", InputValue::Text(text.into())), Err(StudyError::InvalidInput { id, .. }) if id == "ignored_months")
            );
            assert_eq!(
                study
                    .inputs()
                    .iter()
                    .find(|input| input.id == "ignored_months")
                    .unwrap()
                    .value,
                InputValue::Text("2021-01,, 2021-01,".into())
            );
            assert_eq!(study.table().unwrap(), &before);
            for line in 0..5 {
                for bar in 0..bars.len() {
                    assert_eq!(study.values().value(line, bar), values.value(line, bar));
                }
            }
        }
        for (id, value) in [
            ("table_position", InputValue::Choice(0)),
            ("table_width", InputValue::Number(50.0)),
            ("table_height", InputValue::Number(25.0)),
        ] {
            study.set_input(id, value).unwrap();
        }
        study.calculate(&bars);
        let table = study.table().unwrap();
        assert_eq!(
            (table.position, table.width, table.height),
            (TablePosition::TopLeft, Some(0.5), Some(0.25))
        );
    }

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
        let unchanged: Vec<_> = [
            (2020, 12, 100.0),
            (2021, 1, 100.0),
            (2021, 12, 200.0),
            (2022, 1, 210.0),
            (2022, 12, 300.0),
            (2023, 1, 336.0),
        ]
        .into_iter()
        .map(|(y, m, c)| {
            Bar::new(
                Utc.with_ymd_and_hms(y, m, 28, 0, 0, 0).unwrap(),
                c,
                c,
                c,
                c,
                1.0,
            )
        })
        .collect();
        // January changes of 0%, 5% and 12% are all positive, including the developing month.
        for (data, rows) in [
            (
                bars.as_slice(),
                [
                    ("2023", 1, Some(10.0)),
                    ("2023", 2, Some(-10.0)),
                    ("2024", 1, Some(5.0)),
                    ("Average", 1, Some(7.5)),
                    ("Standard deviation", 1, Some(3.5355339059327378)),
                    ("Standard deviation", 2, None),
                    ("Positive", 1, Some(100.0)),
                    ("2022", 12, None),
                    ("2024", 2, None),
                ]
                .as_slice(),
            ),
            (
                unchanged.as_slice(),
                [
                    ("2021", 1, Some(0.0)),
                    ("2022", 1, Some(5.0)),
                    ("2023", 1, Some(12.0)),
                    ("Positive", 1, Some(100.0)),
                ]
                .as_slice(),
            ),
        ] {
            study.calculate(data);
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
            for &(label, month, expected) in rows {
                match expected {
                    Some(expected) => assert!(
                        (percent(label, month).unwrap() - expected).abs() < 1e-9,
                        "{label}"
                    ),
                    None => assert_eq!(percent(label, month), None, "{label}"),
                }
            }
        }
        study.calculate(&bars);
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
                assert!(price.range().min <= 200.0 && price.range().max >= 220.0);
                if !overlay {
                    assert_eq!(study.scale(), StudyScale::None);
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
