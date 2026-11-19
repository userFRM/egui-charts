//! Percentage change over calendar periods, of the chart symbol and of the symbols it is
//! compared with.
//!
//! Each reference is the last finite close on or before the same trading date a period earlier,
//! a number of days, weeks, months or years; month subtraction clamps to the last day of a
//! shorter month, and year to date uses the last close of the previous calendar year. Dates
//! follow the instrument's trading calendar, as VWAP does. On an interval shorter than a day the
//! study requests the symbol's daily bars and takes the later of the daily close and the chart's
//! own close on or before each date, the daily close on the same date. A connected feed loads
//! those daily bars back to the oldest period's date, one year before the latest with the
//! default periods. Missing history and a zero reference close have no value.
//!
//! The table has a row for the chart symbol and one for each symbol of the Symbol list, whose
//! daily bars the study requests on every interval and measures from that symbol's own latest
//! close, and a column for each period of the Timeframe list. The six plots keep the chart
//! symbol's standard periods.

use super::anchor_period::ExchangeDays;
use crate::model::{Bar, Instrument, Timeframe};
use crate::scales::StudyFormat;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, PlotStyle, StudyCell, StudyData, StudyDataRequest,
    StudyTable, TablePosition,
};
use chrono::{DateTime, Datelike, Days, Months, NaiveDate, NaiveTime, Utc};
use egui::Color32;

/// The standard periods: the plots, in this order, and the default Timeframe list.
const PERIODS: [(&str, Period); 6] = [
    ("1W", Period::Days(7)),
    ("1M", Period::Months(1)),
    ("3M", Period::Months(3)),
    ("6M", Period::Months(6)),
    ("YTD", Period::YearToDate),
    ("1Y", Period::Months(12)),
];

/// Behind each row's label, the grey the table paints behind Performance's headings.
const LABEL_FILL: Color32 = Color32::from_rgba_unmultiplied_const(120, 123, 134, 51);
/// Behind a change without a value.
const MISSING_FILL: Color32 = Color32::from_rgba_unmultiplied_const(120, 123, 134, 26);

/// A calendar period a change is measured over.
#[derive(Clone, Copy, Debug)]
enum Period {
    /// The same date this many days earlier.
    Days(u64),
    /// The same date this many months earlier, clamped to the last day of a shorter month.
    Months(u32),
    /// The last day of the previous calendar year.
    YearToDate,
}

impl Period {
    /// The date the period starts from before `date`, or `None` before the calendar's start.
    fn target(self, date: NaiveDate) -> Option<NaiveDate> {
        match self {
            Self::Days(days) => date.checked_sub_days(Days::new(days)),
            Self::Months(months) => date.checked_sub_months(Months::new(months)),
            Self::YearToDate => {
                NaiveDate::from_ymd_opt(date.year(), 1, 1).and_then(|day| day.pred_opt())
            }
        }
    }
}

/// Latest close's percentage change over calendar periods, of the chart symbol and of listed
/// symbols, shown as a table.
#[derive(Clone, Debug)]
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
    /// The oldest period start of any row, kept while the study is emptied for a recompute.
    oldest_target: Option<NaiveDate>,
    table: StudyTable,
    colors: [Color32; 6],
    visible: bool,
    /// The symbols to compare, separated by commas.
    symbols: String,
    /// Whether the chart symbol has the first row.
    include_chart_symbol: bool,
    /// The table's periods, separated by commas.
    periods: String,
    positive_color: Color32,
    negative_color: Color32,
    /// The change, in percent, from which a cell takes its full colour.
    color_cutoff: f64,
    table_position: TablePosition,
    /// The table's width in percent of the pane, 0 to fit its cells.
    table_width: f64,
    /// The table's height in percent of the pane, 0 to fit its rows.
    table_height: f64,
    /// The chart symbol, which labels its row; empty on a chart without a symbol.
    chart_symbol: String,
    /// Each compared symbol, in list order, with the UTC date and finite close of its daily bars.
    compared: Vec<(String, Vec<(NaiveDate, f64)>)>,
}

#[derive(Clone, Debug, Default)]
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
            oldest_target: None,
            table: StudyTable {
                title: "Performance".into(),
                ..Default::default()
            },
            colors: [palette::BLUE; 6],
            visible: true,
            symbols: String::new(),
            include_chart_symbol: true,
            periods: "1W, 1M, 3M, 6M, YTD, 1Y".into(),
            positive_color: palette::UP,
            negative_color: palette::DOWN,
            color_cutoff: 10.0,
            table_position: TablePosition::BottomRight,
            table_width: 0.0,
            table_height: 0.0,
            chart_symbol: String::new(),
            compared: Vec::new(),
        }
    }

    /// The chart symbol's daily bars, on intervals shorter than a day.
    fn daily_request(&self) -> Option<StudyDataRequest> {
        self.timeframe
            .filter(|timeframe| timeframe.duration_ms() < Timeframe::Day1.duration_ms())
            .map(|_| daily_bars(""))
    }

    /// The symbols of the Symbol list, each once, in list order.
    fn compared_symbols(&self) -> Vec<&str> {
        let mut symbols = Vec::new();
        for symbol in self.symbols.split(',').map(str::trim) {
            if !symbol.is_empty() && !symbols.contains(&symbol) {
                symbols.push(symbol);
            }
        }
        symbols
    }

    /// The oldest date a period starts from: the standard and listed periods before the chart
    /// symbol's latest close, and the listed periods before each compared symbol's own.
    fn oldest_start(&self) -> Option<NaiveDate> {
        let listed = table_periods(&self.periods);
        let mut starts = Vec::new();
        if let Some(&(date, _)) = self.history.last() {
            starts.extend(PERIODS.iter().map(|(_, period)| period.target(date)));
            starts.extend(listed.iter().map(|(_, period)| period.target(date)));
        }
        for (_, closes) in &self.compared {
            if let Some(&(date, _)) = closes.last() {
                starts.extend(listed.iter().map(|(_, period)| period.target(date)));
            }
        }
        starts.into_iter().flatten().min()
    }

    /// Fills the table from the latest closes: a row for the chart symbol when it is included,
    /// then one for each compared symbol, under a column for each listed period, each change on
    /// its sign's colour at an intensity growing up to the cutoff.
    fn fill_table(&mut self) {
        let periods = table_periods(&self.periods);
        let changes = |latest: Option<&(NaiveDate, f64)>, histories: [&[(NaiveDate, f64)]; 2]| {
            periods
                .iter()
                .map(|&(_, period)| {
                    latest.and_then(|&(date, close)| change(close, date, period, histories))
                })
                .collect::<Vec<_>>()
        };
        let mut rows = Vec::new();
        if self.include_chart_symbol {
            let label = if self.chart_symbol.is_empty() {
                "Chart".to_owned()
            } else {
                self.chart_symbol.clone()
            };
            let histories = [&self.history[..], &self.daily[..]];
            rows.push((label, changes(self.history.last(), histories)));
        }
        for (symbol, closes) in &self.compared {
            rows.push((symbol.clone(), changes(closes.last(), [&closes[..], &[]])));
        }
        let fill = |change: Option<f64>| match change {
            Some(change) => {
                let color = if change < 0.0 {
                    self.negative_color
                } else {
                    self.positive_color
                };
                color.gamma_multiply((change.abs() / self.color_cutoff).min(1.0) as f32)
            }
            None => MISSING_FILL,
        };
        self.table.cell_colors = rows
            .iter()
            .map(|(_, values)| {
                std::iter::once(LABEL_FILL)
                    .chain(values.iter().map(|&value| fill(value)))
                    .collect()
            })
            .collect();
        self.table.columns = std::iter::once("Symbol".to_owned())
            .chain(periods.into_iter().map(|(label, _)| label))
            .collect();
        self.table.rows = rows
            .into_iter()
            .map(|(label, values)| {
                std::iter::once(StudyCell::Text(label))
                    .chain(values.into_iter().map(StudyCell::Percent))
                    .collect()
            })
            .collect();
        self.table.position = self.table_position;
        self.table.width = share(self.table_width);
        self.table.height = share(self.table_height);
        self.oldest_target = self.oldest_start().or(self.oldest_target);
    }
}

impl Default for Performance {
    fn default() -> Self {
        Self::new()
    }
}

/// The daily bars of `symbol`, the chart symbol when empty.
fn daily_bars(symbol: &str) -> StudyDataRequest {
    StudyDataRequest {
        symbol: symbol.into(),
        timeframe: Some(Timeframe::Day1),
        metric: None,
    }
}

/// The UTC date and finite close of each bar. Bars out of time order count as none supplied,
/// since the period lookup is a binary search.
fn closes(bars: &[Bar]) -> Vec<(NaiveDate, f64)> {
    let closes: Vec<_> = bars
        .iter()
        .filter(|bar| bar.close.is_finite())
        .map(|bar| (bar.time.date_naive(), bar.close))
        .collect();
    if closes.is_sorted_by_key(|(day, _)| *day) {
        closes
    } else {
        Vec::new()
    }
}

/// The percentage change to `close` on `date` from the latest close on or before the start of
/// `period` in either history, the second one's on the same date; `None` without such a close,
/// from a zero close or when the change is not finite.
fn change(
    close: f64,
    date: NaiveDate,
    period: Period,
    histories: [&[(NaiveDate, f64)]; 2],
) -> Option<f64> {
    let target = period.target(date)?;
    // `max_by_key` keeps the last of equal dates.
    let (_, reference) = histories
        .into_iter()
        .filter_map(|history| {
            let at = history.partition_point(|(day, _)| *day <= target);
            at.checked_sub(1).map(|at| history[at])
        })
        .max_by_key(|(day, _)| *day)?;
    let change = 100.0 * (close - reference) / reference;
    (reference != 0.0 && change.is_finite()).then_some(change)
}

/// The periods of a list such as "1W, 3M, YTD, 2Y", in any case: a positive count of days (D),
/// weeks (W), months (M) or years (Y), labelled by the count and the upper-case unit, or YTD.
/// Entries it cannot read, and repeats of a label, are skipped.
fn table_periods(list: &str) -> Vec<(String, Period)> {
    let mut periods: Vec<(String, Period)> = Vec::new();
    for entry in list.split(',') {
        let entry = entry.trim().to_ascii_uppercase();
        let period = if entry == "YTD" {
            Some((entry, Period::YearToDate))
        } else {
            entry.chars().next_back().and_then(|unit| {
                let count = entry[..entry.len() - unit.len_utf8()]
                    .parse::<u32>()
                    .ok()
                    .filter(|count| *count > 0)?;
                let period = match unit {
                    'D' => Period::Days(count.into()),
                    'W' => Period::Days(u64::from(count) * 7),
                    'M' => Period::Months(count),
                    'Y' => Period::Months(count.checked_mul(12)?),
                    _ => return None,
                };
                Some((format!("{count}{unit}"), period))
            })
        };
        if let Some(period) =
            period.filter(|(label, _)| periods.iter().all(|(seen, _)| seen != label))
        {
            periods.push(period);
        }
    }
    periods
}

/// A size input, in percent of the pane, as the table's share of it; 0 fits the cells.
fn share(percent: f64) -> Option<f32> {
    (percent > 0.0).then_some((percent / 100.0) as f32)
}

impl Indicator for Performance {
    crate::studies::inputs::study_inputs!(self;
        symbols: text("Symbol list"),
        include_chart_symbol: toggle("Include chart symbol"),
        periods: text("Timeframe list"),
        positive_color: color("Positive colour"),
        negative_color: color("Negative colour"),
        color_cutoff: number("Colour intensity cutoff (%)", 0.01, f64::MAX),
        table_position: choice("Table position", TablePosition::ALL),
        table_width: number("Table width (%)", 0.0, 100.0),
        table_height: number("Table height (%)", 0.0, 100.0)
    ; {});

    fn name(&self) -> &str {
        "Performance"
    }
    fn desc(&self) -> &str {
        "Percentage change over standard calendar periods"
    }
    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        self.days.set(instrument)
    }

    /// The chart symbol labels its row; a new one relabels the table without a recalculation.
    fn set_chart_symbol(&mut self, symbol: &str) -> bool {
        if symbol != self.chart_symbol {
            symbol.clone_into(&mut self.chart_symbol);
            self.fill_table();
        }
        false
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.timeframe = Some(timeframe);
        // Daily and longer charts read their own bars; drop daily bars of an earlier interval.
        if self.daily_request().is_none() {
            self.daily.clear();
        }
    }

    /// The chart symbol's daily bars on intervals shorter than a day, then the daily bars of
    /// each compared symbol on every interval.
    fn data_requests(&self) -> Vec<StudyDataRequest> {
        self.daily_request()
            .into_iter()
            .chain(self.compared_symbols().into_iter().map(daily_bars))
            .collect()
    }

    /// The oldest period start of any row, for every request, as UTC because daily bars count by
    /// UTC date.
    fn data_start(&self, _: &StudyDataRequest) -> Option<DateTime<Utc>> {
        self.oldest_target
            .map(|target| target.and_time(NaiveTime::MIN).and_utc())
    }

    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
        if from == 0 {
            let supplied = |request: &StudyDataRequest| {
                sources
                    .iter()
                    .find(|source| source.request == *request)
                    .map_or_else(Vec::new, |source| closes(source.bars))
            };
            self.daily = self
                .daily_request()
                .map_or_else(Vec::new, |request| supplied(&request));
            self.compared = self
                .compared_symbols()
                .into_iter()
                .map(|symbol| (symbol.to_owned(), supplied(&daily_bars(symbol))))
                .collect();
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
                    for (line, (_, period)) in PERIODS.into_iter().enumerate() {
                        state.latest[line] = change(
                            bar.close,
                            date,
                            period,
                            [&self.history[..], &self.daily[..]],
                        );
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
        self.fill_table();
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
        PERIODS.map(|(label, _)| label.to_owned()).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Bar;
    use crate::studies::*;
    use chrono::{TimeZone, Utc};

    /// Calendar lookbacks use the last close on or before the target date, not bar counts, and
    /// a study saved before Performance had inputs shows the chart symbol over the six periods at
    /// the default place and size.
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
        // A literal template saved before Performance had inputs.
        let saved: StudyTemplate = serde_json::from_str(
            r#"{"name":"Performance","inputs":[],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.Performance.html","plots":[{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[41,98,255,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false}],"precision":null,"hidden_intervals":[],"timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"levels":[],"fill_between_levels":false,"level_fill_color":[3,6,16,16],"untagged_plots":[0,1,2,3,4,5],"price_line_plots":[],"plot_variants":[[],[],[],[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
        )
        .unwrap();
        let mut study = saved.create().expect("saved Performance");
        assert_eq!(study.settings(), saved.settings);
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
        let table = study.table().expect("Performance table");
        let mut chart_row = vec![StudyCell::Text("Chart".into())];
        chart_row
            .extend([20.0, 44.0, 80.0, 125.0, 80.0].map(|change| StudyCell::Percent(Some(change))));
        chart_row.push(StudyCell::Percent(None));
        assert_eq!(
            table.columns,
            ["Symbol", "1W", "1M", "3M", "6M", "YTD", "1Y"]
        );
        assert_eq!(table.rows, [chart_row]);
        assert_eq!(
            (table.position, table.width, table.height),
            (TablePosition::BottomRight, None, None)
        );
        #[cfg(feature = "widget")]
        {
            use crate::widget::{Chart, test_frames::shapes};
            let ctx = egui::Context::default();
            let mut chart = Chart::new(crate::model::BarData::from_bars(bars));
            let id = chart.add_indicator(study);
            let painted = shapes(&ctx, &mut chart, 0.0, Vec::new());
            for expected in ["Symbol", "Chart", "1W", "+44.00%", "N/A"] {
                assert!(painted.iter().any(|shape| matches!(shape, egui::Shape::Text(text) if text.galley.text() == expected)), "missing table cell {expected}");
            }
            chart.indicator_mut(id).unwrap().set_visible(false);
            let painted = shapes(&ctx, &mut chart, 1.0, Vec::new());
            assert!(!painted.iter().any(
                |shape| matches!(shape, egui::Shape::Text(text) if text.galley.text() == "Symbol")
            ));
        }
    }

    /// On an intraday chart each period starts from the requested daily bars, the daily close
    /// winning on the same date, and from the chart's own bars when none are supplied or they
    /// are out of time order.
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
        let without_daily = [Some(12.5), None, None, None, None, None];
        let reversed: Vec<_> = daily.iter().rev().cloned().collect();
        for (chart, supplied, expected) in [
            (today.to_vec(), &daily[..], from_daily),
            (week_before.clone(), &daily[..], from_daily),
            (week_before.clone(), &[][..], without_daily),
            (week_before, &reversed[..], without_daily),
        ] {
            let mut study = Performance::new();
            study.set_timeframe(Timeframe::Min1);
            assert_eq!(study.data_requests(), vec![request.clone()]);
            let source = StudyData {
                request: request.clone(),
                bars: supplied,
                clock: BarClock::new(Timeframe::Day1),
                instrument: None,
                timeframe: Timeframe::Day1,
            };
            study.update_data(&chart, 0, &[source]);
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

    /// The chart symbol and each listed symbol get a row under a column per listed period, each
    /// symbol measured from its own latest daily close and one without history showing N/A,
    /// and each change fills its cell with its sign's colour, at full intensity from the cutoff.
    #[test]
    fn performance_compares_symbols_over_the_listed_periods() {
        use crate::model::{BarClock, Timeframe};
        use egui::Color32;
        let bar = |(y, m, d, c)| {
            Bar::new(
                Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap(),
                c,
                c,
                c,
                c,
                1.0,
            )
        };
        // The chart bars of `performance_uses_calendar_dates_and_missing_history`.
        let chart = [
            (2023, 9, 29, 80.0),
            (2023, 12, 29, 100.0),
            (2024, 2, 28, 120.0),
            (2024, 2, 29, 125.0),
            (2024, 3, 22, 150.0),
            (2024, 3, 31, 180.0),
        ]
        .map(bar);
        // MSFT's latest close is on March 28, three days before the chart's.
        let msft = [
            (2023, 3, 28, 50.0),
            (2023, 9, 28, 30.0),
            (2023, 12, 28, 40.0),
            (2024, 2, 28, 120.0),
            (2024, 3, 21, 48.0),
            (2024, 3, 22, 75.0),
            (2024, 3, 28, 60.0),
        ]
        .map(bar);
        let daily = |symbol: &str| StudyDataRequest {
            symbol: symbol.into(),
            timeframe: Some(Timeframe::Day1),
            metric: None,
        };
        // GOOG is never supplied.
        let source = StudyData {
            request: daily("MSFT"),
            bars: &msft,
            clock: BarClock::new(Timeframe::Day1),
            instrument: None,
            timeframe: Timeframe::Day1,
        };
        let rows = |study: &Performance| -> Vec<(String, Vec<Option<f64>>)> {
            let table = study.table().unwrap();
            table
                .rows
                .iter()
                .map(|row| match &row[..] {
                    [StudyCell::Text(label), changes @ ..] => (
                        label.clone(),
                        changes
                            .iter()
                            .map(|cell| match cell {
                                StudyCell::Percent(change) => *change,
                                _ => panic!("a change cell"),
                            })
                            .collect(),
                    ),
                    _ => panic!("a labelled row"),
                })
                .collect()
        };
        let columns = |study: &Performance| study.table().unwrap().columns.clone();
        let mut study = Performance::new();
        study.set_chart_symbol("AAPL");
        study.set_timeframe(Timeframe::Min1);
        study
            .set_input("symbols", InputValue::Text("GOOG, MSFT, GOOG".into()))
            .unwrap();
        study
            .set_input("color_cutoff", InputValue::Number(40.0))
            .unwrap();
        assert_eq!(
            study.data_requests(),
            [daily(""), daily("GOOG"), daily("MSFT")]
        );
        study.update_data(&chart, 0, std::slice::from_ref(&source));
        assert_eq!(
            columns(&study),
            ["Symbol", "1W", "1M", "3M", "6M", "YTD", "1Y"]
        );
        // AAPL as in the first test. MSFT from March 28: 1W from March 21, 100 * 12 / 48 = 25
        // (from the chart's March 31 it would start on March 22: 100 * -15 / 75 = -20); 1M from
        // February 28, 100 * -60 / 120 = -50; 3M and YTD from December 28, 100 * 20 / 40 = 50;
        // 6M from September 28, 100 * 30 / 30 = 100; 1Y from 2023-03-28, 100 * 10 / 50 = 20.
        assert_eq!(
            rows(&study),
            [
                (
                    "AAPL".to_owned(),
                    vec![
                        Some(20.0),
                        Some(44.0),
                        Some(80.0),
                        Some(125.0),
                        Some(80.0),
                        None
                    ]
                ),
                ("GOOG".to_owned(), vec![None; 6]),
                (
                    "MSFT".to_owned(),
                    [25.0, -50.0, 50.0, 100.0, 50.0, 20.0].map(Some).to_vec()
                ),
            ]
        );
        let fills = &study.table().unwrap().cell_colors;
        // +20% at a 40% cutoff: the positive colour (8, 153, 129) at half intensity, each
        // channel times 0.5 plus 0.5, rounded down.
        assert_eq!(
            fills[0][1],
            Color32::from_rgba_premultiplied(4, 77, 65, 128)
        );
        // -50%: past the cutoff, the full default negative colour.
        assert_eq!(fills[2][2], Color32::from_rgb(242, 54, 69));
        // A new chart symbol relabels its row.
        study.set_chart_symbol("SPY");
        assert_eq!(rows(&study)[0].0, "SPY");
        // Removing a period removes its column.
        study
            .set_input("periods", InputValue::Text("1W, 1M, 6M, YTD, 1Y".into()))
            .unwrap();
        study.update_data(&chart, 0, std::slice::from_ref(&source));
        assert_eq!(columns(&study), ["Symbol", "1W", "1M", "6M", "YTD", "1Y"]);
        // Lower case reads; an unknown unit, a repeat and a zero count are skipped. MSFT's 5D
        // starts on March 23: 100 * -15 / 75 = -20; its 2Y on 2022-03-28, before its history.
        study
            .set_input(
                "periods",
                InputValue::Text("2y, ytd, 1X, 1w, 1W, 0M, 5D".into()),
            )
            .unwrap();
        study.update_data(&chart, 0, std::slice::from_ref(&source));
        assert_eq!(columns(&study), ["Symbol", "2Y", "YTD", "1W", "5D"]);
        assert_eq!(
            rows(&study)[2],
            (
                "MSFT".to_owned(),
                vec![None, Some(50.0), Some(25.0), Some(-20.0)]
            )
        );
        // Every request loads from MSFT's 2Y start, three days before the chart's 2022-03-31.
        assert_eq!(
            study.data_start(&daily("MSFT")),
            Some(Utc.with_ymd_and_hms(2022, 3, 28, 0, 0, 0).unwrap())
        );
        study
            .set_input("include_chart_symbol", InputValue::Toggle(false))
            .unwrap();
        study.update_data(&chart, 0, std::slice::from_ref(&source));
        let labels: Vec<_> = rows(&study).into_iter().map(|(label, _)| label).collect();
        assert_eq!(labels, ["GOOG", "MSFT"]);
        for (id, value) in [
            ("table_position", InputValue::Choice(0)),
            ("table_width", InputValue::Number(50.0)),
            ("table_height", InputValue::Number(25.0)),
        ] {
            study.set_input(id, value).unwrap();
        }
        let table = study.table().unwrap();
        assert_eq!(
            (table.position, table.width, table.height),
            (TablePosition::TopLeft, Some(0.5), Some(0.25))
        );
    }
}
