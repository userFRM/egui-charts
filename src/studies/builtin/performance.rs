//! Percentage change over calendar periods, of the chart symbol and of the symbols it is
//! compared with.
//!
//! Each reference is the last finite close on or before the same trading date a period earlier,
//! a number of days, weeks, months or years; month subtraction clamps to the last day of a
//! shorter month, and year to date starts on 1 January, from the latest close on or before it,
//! or from the latest earlier close on 1 January itself. Dates
//! follow the instrument's trading calendar, as VWAP does. On every interval but one day the
//! study requests the symbol's daily bars. Below a day it takes the later of the daily close and
//! the chart's own close on or before each date, the daily close on the same date. On bars longer
//! than a day, which are dated by their first day, it measures from daily closes alone: each bar
//! counts as closed on the date of the last daily close inside it, and without daily bars a
//! period has no value. A connected feed loads those daily bars back to the oldest period's date,
//! five years before the latest with the default periods. Missing history and a zero reference
//! close have no value.
//!
//! The table has a row for the chart symbol and one for each other symbol of the Symbol list,
//! whose daily bars the study requests on every interval and measures from that symbol's own
//! latest close, and a column for each period of the Timeframe list. With Include chart symbol
//! off, a listed chart symbol has a row of its own. The six plots keep the chart symbol's
//! standard periods. A new study compares `AAPL, NASDAQ:AMZN, EURUSD, ES1!, BTCUSD` over
//! `1W, 1M, 3M, 6M, 12M, YTD, 5Y` in a pane of its own, in a table at the middle centre, 100%
//! of that pane wide and 95% high, with falling changes in `#F23745`.
//!
//! A row is labelled with the description of its symbol's instrument when the host gives one,
//! and with the ticker otherwise. Each change is written in its sign's colour, the Positive
//! colour for no change, over a fill of that colour that runs from 90% transparency at no change
//! to 70% at Colour intensity cutoff (%) and beyond. A change without a value reads "No data".
//! Resting the pointer on a change shows the date of the close it is measured from.

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

/// The standard periods: the plots, in this order.
const PERIODS: [(&str, Period); 6] = [
    ("1W", Period::Days(7)),
    ("1M", Period::Months(1)),
    ("3M", Period::Months(3)),
    ("6M", Period::Months(6)),
    ("YTD", Period::YearToDate),
    ("1Y", Period::Months(12)),
];

/// Behind each row's label, the grey the table paints behind Performance's headings.
pub(super) const LABEL_FILL: Color32 = Color32::from_rgba_unmultiplied_const(120, 123, 134, 51);
/// Behind a change without a value.
const MISSING_FILL: Color32 = Color32::from_rgba_unmultiplied_const(120, 123, 134, 26);

/// A calendar period a change is measured over.
#[derive(Clone, Copy, Debug)]
enum Period {
    /// The same date this many days earlier.
    Days(u64),
    /// The same date this many months earlier, clamped to the last day of a shorter month.
    Months(u32),
    /// 1 January of the date's year, or the day before on 1 January itself, whose own close
    /// is still developing.
    YearToDate,
}

impl Period {
    /// The date the period starts from before `date`, or `None` before the calendar's start.
    fn target(self, date: NaiveDate) -> Option<NaiveDate> {
        match self {
            Self::Days(days) => date.checked_sub_days(Days::new(days)),
            Self::Months(months) => date.checked_sub_months(Months::new(months)),
            Self::YearToDate => NaiveDate::from_ymd_opt(date.year(), 1, 1)
                .filter(|first| *first < date)
                .or_else(|| date.pred_opt()),
        }
    }
}

/// Latest close's percentage change over calendar periods, of the chart symbol and of listed
/// symbols, shown as a table. The study opens in a pane of its own.
#[derive(Clone, Debug)]
pub struct Performance {
    days: ExchangeDays,
    values: IndicatorValues,
    stepper: Stepper<State>,
    history: Vec<(NaiveDate, f64)>,
    before_last_history: usize,
    intraday: bool,
    /// The chart interval, which decides whether daily bars are requested and whether the
    /// chart's own bars may start a period.
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
    /// The chart instrument's description, which labels its row when given.
    chart_description: Option<String>,
    /// The compared symbols daily bars were last supplied or found missing for, in list order.
    compared: Vec<Compared>,
}

/// A compared symbol, the description its instrument gives, and the UTC date and finite close of
/// its daily bars, in time order.
#[derive(Clone, Debug)]
struct Compared {
    symbol: String,
    description: Option<String>,
    closes: Vec<(NaiveDate, f64)>,
}

#[derive(Clone, Debug, Default)]
struct State {
    source: usize,
    previous_date: Option<NaiveDate>,
    intraday: bool,
    latest: [Option<f64>; 6],
}

impl Performance {
    /// Performance over the chart's loaded bars, and on every interval but one day over the daily
    /// bars the host supplies for [`Indicator::data_requests`], in UTC until an instrument is
    /// supplied.
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
            symbols: "AAPL, NASDAQ:AMZN, EURUSD, ES1!, BTCUSD".into(),
            include_chart_symbol: true,
            periods: "1W, 1M, 3M, 6M, 12M, YTD, 5Y".into(),
            positive_color: palette::UP,
            negative_color: Color32::from_rgb(242, 55, 69),
            color_cutoff: 10.0,
            table_position: TablePosition::MiddleCenter,
            table_width: 100.0,
            table_height: 95.0,
            chart_symbol: String::new(),
            chart_description: None,
            compared: Vec::new(),
        }
    }

    /// The chart symbol's daily bars, on every interval but one day, whose own bars are daily.
    fn daily_request(&self) -> Option<StudyDataRequest> {
        self.timeframe
            .filter(|timeframe| timeframe.duration_ms() != Timeframe::Day1.duration_ms())
            .map(|_| daily_bars(""))
    }

    /// Whether each chart bar spans more than a day. Such a bar is dated by its first day and
    /// closes at its end, so its periods are measured from daily closes alone.
    fn long_bars(&self) -> bool {
        self.timeframe
            .is_some_and(|timeframe| timeframe.duration_ms() > Timeframe::Day1.duration_ms())
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
        for compared in &self.compared {
            if let Some(&(date, _)) = compared.closes.last() {
                starts.extend(listed.iter().map(|(_, period)| period.target(date)));
            }
        }
        starts.into_iter().flatten().min()
    }

    /// Fills the table from the latest closes: a row for the chart symbol when it is included,
    /// then one for each other compared symbol, labelled with its description when it has one,
    /// under a column for each listed period. Each change is written in its sign's colour over
    /// that colour at 10% opacity, growing to 30% at the cutoff, with the date of the close it is
    /// measured from as its tooltip; a change without a value reads "No data".
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
            let label = match &self.chart_description {
                Some(description) => description.clone(),
                None if self.chart_symbol.is_empty() => "Chart".to_owned(),
                None => self.chart_symbol.clone(),
            };
            let long = self.long_bars();
            let latest = self.history.last().map(|&(date, close)| {
                if long {
                    (close_date(&self.daily, date, None), close)
                } else {
                    (date, close)
                }
            });
            let own = if long { &[][..] } else { &self.history[..] };
            rows.push((label, changes(latest.as_ref(), [own, &self.daily[..]])));
        }
        for symbol in self.compared_symbols() {
            // The chart symbol's row stands for a listed copy of it.
            if self.include_chart_symbol && symbol == self.chart_symbol {
                continue;
            }
            // A listed symbol has its row before any daily bars are supplied for it.
            let supplied = self.compared.iter().find(|other| other.symbol == symbol);
            let label = supplied
                .and_then(|supplied| supplied.description.as_deref())
                .unwrap_or(symbol);
            let closes = supplied.map_or(&[][..], |supplied| &supplied.closes[..]);
            rows.push((label.to_owned(), changes(closes.last(), [closes, &[]])));
        }
        let table = &mut self.table;
        table.rows.clear();
        table.cell_colors.clear();
        table.cell_text_colors.clear();
        table.cell_tooltips.clear();
        for (label, values) in rows {
            let mut cells = vec![StudyCell::Text(label)];
            let mut fills = vec![LABEL_FILL];
            let mut text_colors = vec![None];
            let mut tooltips = vec![String::new()];
            for value in values {
                let Some((change, date)) = value else {
                    cells.push(StudyCell::Text("No data".into()));
                    fills.push(MISSING_FILL);
                    text_colors.push(None);
                    tooltips.push(String::new());
                    continue;
                };
                let color = if change < 0.0 {
                    self.negative_color
                } else {
                    self.positive_color
                };
                let reached = (change.abs() / self.color_cutoff).min(1.0);
                cells.push(StudyCell::Percent(Some(change)));
                fills.push(color.gamma_multiply((0.1 + 0.2 * reached) as f32));
                text_colors.push(Some(color));
                tooltips.push(date.format("%d %b %Y").to_string());
            }
            table.rows.push(cells);
            table.cell_colors.push(fills);
            table.cell_text_colors.push(text_colors);
            table.cell_tooltips.push(tooltips);
        }
        self.table.columns = std::iter::once("Symbol".to_owned())
            .chain(periods.into_iter().map(|(label, _)| label))
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

/// The date of the close of a chart bar longer than a day, dated `date`, whose next bar is
/// dated `next`: the last daily close from `date` up to `next`, or `date` without one.
fn close_date(daily: &[(NaiveDate, f64)], date: NaiveDate, next: Option<NaiveDate>) -> NaiveDate {
    let end = next.map_or(daily.len(), |next| {
        daily.partition_point(|(day, _)| *day < next)
    });
    daily[..end]
        .last()
        .map(|(day, _)| *day)
        .filter(|day| *day >= date)
        .unwrap_or(date)
}

/// The trimmed description of `instrument`, or `None` when it has none or a blank one.
fn description(instrument: Option<&Instrument>) -> Option<String> {
    let description = instrument?.description.as_deref()?.trim();
    (!description.is_empty()).then(|| description.to_owned())
}

/// The percentage change to `close` on `date` from the latest close on or before the start of
/// `period` in either history, the second one's on the same date, and the date of that close;
/// `None` without such a close, from a zero close or when the change is not finite.
fn change(
    close: f64,
    date: NaiveDate,
    period: Period,
    histories: [&[(NaiveDate, f64)]; 2],
) -> Option<(f64, NaiveDate)> {
    let target = period.target(date)?;
    // `max_by_key` keeps the last of equal dates.
    let (day, reference) = histories
        .into_iter()
        .filter_map(|history| {
            let at = history.partition_point(|(day, _)| *day <= target);
            at.checked_sub(1).map(|at| history[at])
        })
        .max_by_key(|(day, _)| *day)?;
    let change = 100.0 * (close - reference) / reference;
    (reference != 0.0 && change.is_finite()).then_some((change, day))
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
pub(super) fn share(percent: f64) -> Option<f32> {
    (percent > 0.0).then_some((percent / 100.0) as f32)
}

/// The pane height that shows `table` whole at its text size: its rows under the title and
/// the headings, the 40 points the table leaves around itself, at most 420 points.
pub(super) fn table_pane_height(table: &StudyTable) -> f32 {
    let content = 2.0 * table.text_size.points() * (table.rows.len() + 2) as f32;
    // Fitted tables use at most half the pane, so leave room for their full content.
    let minimum = if table.height.is_none() {
        2.0 * content
    } else {
        0.0
    };
    (content + 40.0).max(minimum).min(420.0)
}

impl Indicator for Performance {
    fn is_overlay(&self) -> bool {
        false
    }

    /// The table has no value axis: the six plots are hidden until the user shows them.
    fn scale(&self) -> crate::studies::StudyScale {
        crate::studies::StudyScale::None
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: Some(table_pane_height(&self.table)),
            ..Default::default()
        }
    }

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
    /// The instrument's description labels the chart symbol's row; a new one relabels the table
    /// without a recalculation of its own.
    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        let description = description(instrument);
        if description != self.chart_description {
            self.chart_description = description;
            self.fill_table();
        }
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
        // Daily charts read their own bars; drop daily bars of an earlier interval.
        if self.daily_request().is_none() {
            self.daily.clear();
        }
    }

    /// The chart symbol's daily bars on every interval but one day, then the daily bars of
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
                sources.iter().find(|source| source.request == *request)
            };
            self.daily = self
                .daily_request()
                .and_then(|request| supplied(&request))
                .map_or_else(Vec::new, |source| closes(source.bars));
            self.compared = self
                .compared_symbols()
                .into_iter()
                .map(|symbol| {
                    let source = supplied(&daily_bars(symbol));
                    Compared {
                        symbol: symbol.to_owned(),
                        description: source.and_then(|source| description(source.instrument)),
                        closes: source.map_or_else(Vec::new, |source| closes(source.bars)),
                    }
                })
                .collect();
        }
        self.update(data, from);
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update_data(data, 0, &[]);
    }

    fn update(&mut self, data: &[Bar], mut from: usize) {
        let len = self.values.len();
        let long = self.long_bars();
        // A long bar's close date depends on the bar after it, so an append redoes the last bar.
        if long {
            from = from.min(len.saturating_sub(1));
        }
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
                    let (closed, own) = if long {
                        let next = data.get(source + 1).map(|next| next.time.date_naive());
                        (close_date(&self.daily, date, next), &[][..])
                    } else {
                        (date, &self.history[..])
                    };
                    for (line, (_, period)) in PERIODS.into_iter().enumerate() {
                        state.latest[line] =
                            change(bar.close, closed, period, [own, &self.daily[..]])
                                .map(|(change, _)| change);
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

    /// A new table fills its own pane below the candles, and can still move to the price pane.
    #[cfg(feature = "widget")]
    #[test]
    fn performance_opens_in_a_pane_of_its_own() {
        use crate::widget::{PaneId, test_frames};

        let mut chart = test_frames::chart(&[1.0, 4.0, 7.0], &[2.0, 5.0, 8.0]);
        let id = chart.add_indicator(Box::new(Performance::new()));
        let ctx = egui::Context::default();
        test_frames::shapes(&ctx, &mut chart, 0.0, Vec::new());
        let painted = test_frames::shapes(&ctx, &mut chart, 0.1, Vec::new());
        assert_eq!(chart.panes().len(), 2);
        let pane = chart.series_pane(id).unwrap();
        assert_ne!(pane, PaneId::MAIN);
        assert_eq!(chart.pane_series(pane), [id]);
        let heading_y = painted
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::Text(text) if text.galley.text() == "Symbol" => Some(text.pos.y),
                _ => None,
            })
            .unwrap();
        let candle_bottom = test_frames::candle_ys(&painted, palette::UP)
            .into_iter()
            .reduce(f32::max)
            .unwrap();
        assert!(heading_y > candle_bottom);
        // Six rows, headings and title at 22 points per row, with 40 points around the table.
        assert!((chart.pane_height(pane).unwrap() - 216.0).abs() < 0.5);
        assert_eq!(
            chart.indicators().next().unwrap().1.scale(),
            StudyScale::None
        );
        assert!(chart.move_series_to_pane(id, PaneId::MAIN));
        assert_eq!(chart.panes().len(), 1);
        assert_eq!(chart.series_pane(id), Some(PaneId::MAIN));
    }

    /// Calendar lookbacks use the last close on or before the target date, not bar counts, and
    /// a study saved before Performance had inputs shows the chart symbol alone over the six
    /// periods, fitted at the lower right, with falling changes in the red it had. Year to date
    /// starts from 1 January's close when the symbol trades then, and from the latest earlier
    /// close otherwise.
    #[test]
    fn performance_uses_calendar_dates_and_missing_history() {
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
        let bars: Vec<_> = [
            (2023, 9, 29, 80.0),
            (2023, 12, 29, 100.0),
            (2024, 2, 28, 120.0),
            (2024, 2, 29, 125.0),
            (2024, 3, 22, 150.0),
            (2024, 3, 31, 180.0),
        ]
        .map(bar)
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
        chart_row.push(StudyCell::Text("No data".into()));
        assert_eq!(
            table.columns,
            ["Symbol", "1W", "1M", "3M", "6M", "YTD", "1Y"]
        );
        assert_eq!(table.rows, [chart_row]);
        assert_eq!(
            (table.position, table.width, table.height),
            (TablePosition::BottomRight, None, None)
        );
        assert_eq!(
            study
                .inputs()
                .iter()
                .find(|input| input.id == "negative_color")
                .map(|input| &input.value),
            Some(&InputValue::Color(palette::DOWN))
        );
        #[cfg(feature = "widget")]
        {
            use crate::widget::{Chart, test_frames::shapes};
            let ctx = egui::Context::default();
            let mut chart = Chart::new(crate::model::BarData::from_bars(bars));
            let id = chart.add_indicator(study);
            let has = |painted: &[egui::Shape], wanted: &str| {
                painted.iter().any(
                    |shape| matches!(shape, egui::Shape::Text(text) if text.galley.text() == wanted),
                )
            };
            let painted = shapes(&ctx, &mut chart, 0.0, Vec::new());
            for expected in ["Symbol", "Chart", "1W", "+44.00%", "No data"] {
                assert!(has(&painted, expected), "missing table cell {expected}");
            }
            // Resting the pointer on the 1M change shows the date of the close it starts from. A
            // new tooltip is laid out unseen on its first frame.
            assert!(!has(&painted, "29 Feb 2024"));
            let centre = painted.iter().find_map(|shape| match shape {
                egui::Shape::Text(text) if text.galley.text() == "+44.00%" => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            });
            let moved = || vec![egui::Event::PointerMoved(centre.unwrap())];
            shapes(&ctx, &mut chart, 0.1, moved());
            let painted = shapes(&ctx, &mut chart, 0.2, moved());
            assert!(has(&painted, "29 Feb 2024"), "the date of the 1M close");
            chart.indicator_mut(id).unwrap().set_visible(false);
            let painted = shapes(&ctx, &mut chart, 1.0, Vec::new());
            assert!(!painted.iter().any(
                |shape| matches!(shape, egui::Shape::Text(text) if text.galley.text() == "Symbol")
            ));
        }
        // Year to date starts at 1 January: from its close when the symbol trades then, from the
        // latest earlier close otherwise. 1 March closes at 121: from 1 January's 110,
        // 100 * 11 / 110 = 10; from 31 December's 100, 21.
        let trades_new_year = [
            (2024, 12, 31, 100.0),
            (2025, 1, 1, 110.0),
            (2025, 3, 1, 121.0),
        ];
        let closed_new_year = [(2024, 12, 31, 100.0), (2025, 3, 1, 121.0)];
        for (closes, ytd, from) in [
            (&trades_new_year[..], 10.0, "01 Jan 2025"),
            (&closed_new_year[..], 21.0, "31 Dec 2024"),
        ] {
            let bars: Vec<_> = closes.iter().copied().map(bar).collect();
            let mut study = Performance::new();
            study
                .set_input("symbols", InputValue::Text(String::new()))
                .unwrap();
            study
                .set_input("periods", InputValue::Text("YTD".into()))
                .unwrap();
            study.calculate(&bars);
            let table = study.table().unwrap();
            assert_eq!(
                (&table.rows[0][1], table.cell_tooltips[0][1].as_str()),
                (&StudyCell::Percent(Some(ytd)), from),
                "{from}"
            );
            assert_eq!(
                study.values().value(4, bars.len() - 1),
                Some(ytd),
                "plot, {from}"
            );
        }
    }

    /// On an intraday chart each period starts from the requested daily bars, the daily close
    /// winning on the same date, and from the chart's own bars when none are supplied or they
    /// are out of time order, and on 1 January year to date starts from the year before.
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
        // On 1 January, year to date starts from the year before, never from the day's
        // developing daily bar.
        let new_year = [(2024, 12, 31, 0, 0, 100.0), (2025, 1, 1, 0, 0, 999.0)].map(bar);
        let new_year_day = [(2025, 1, 1, 14, 30, 105.0), (2025, 1, 1, 14, 31, 110.0)].map(bar);
        let ytd_only = [None, None, None, None, Some(10.0), None];
        for (chart, supplied, expected) in [
            (today.to_vec(), &daily[..], from_daily),
            (week_before.clone(), &daily[..], from_daily),
            (week_before.clone(), &[][..], without_daily),
            (week_before, &reversed[..], without_daily),
            (new_year_day.to_vec(), &new_year[..], ytd_only),
        ] {
            let mut study = Performance::new();
            study
                .set_input("symbols", InputValue::Text(String::new()))
                .unwrap();
            study.set_timeframe(Timeframe::Min1);
            assert_eq!(study.data_requests(), vec![request.clone()]);
            let source = StudyData {
                request: request.clone(),
                bars: supplied,
                clock: BarClock::new(Timeframe::Day1),
                instrument: None,
                timeframe: Timeframe::Day1,
                chart: None,
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

    /// Long bars end on their last daily close and start periods from daily closes alone.
    /// Appending a bar bounds the previous bar again, as a fresh calculation does.
    #[test]
    fn performance_measures_long_bars_from_daily_closes() {
        use crate::model::{BarClock, Timeframe};

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
        let daily = [
            (2023, 11, 29, 80.0),
            (2023, 12, 29, 100.0),
            (2024, 1, 29, 125.0),
            (2024, 1, 31, 120.0),
            (2024, 2, 22, 160.0),
            (2024, 2, 28, 190.0),
            (2024, 2, 29, 200.0),
        ]
        .map(bar);
        let monthly = [
            (2023, 11, 1, 80.0),
            (2023, 12, 1, 100.0),
            (2024, 1, 1, 120.0),
            (2024, 2, 1, 200.0),
        ]
        .map(bar);
        let weekly = [
            (2024, 2, 5, 140.0),
            (2024, 2, 12, 150.0),
            (2024, 2, 19, 170.0),
            (2024, 2, 26, 200.0),
        ]
        .map(bar);
        let new = |timeframe| {
            let mut study = Performance::new();
            study
                .set_input("symbols", InputValue::Text(String::new()))
                .unwrap();
            study
                .set_input(
                    "periods",
                    InputValue::Text("1W, 1M, 3M, 6M, YTD, 1Y".into()),
                )
                .unwrap();
            study.set_timeframe(timeframe);
            study
        };
        let request = StudyDataRequest {
            symbol: String::new(),
            timeframe: Some(Timeframe::Day1),
            metric: None,
        };
        let changes = [Some(25.0), Some(60.0), Some(150.0), None, Some(100.0), None];
        let dates = [
            "",
            "22 Feb 2024",
            "29 Jan 2024",
            "29 Nov 2023",
            "",
            "29 Dec 2023",
            "",
        ];
        for (timeframe, chart) in [(Timeframe::Month1, &monthly), (Timeframe::Week1, &weekly)] {
            for supplied in [&daily[..], &[][..]] {
                let mut study = new(timeframe);
                let source = StudyData {
                    request: request.clone(),
                    bars: supplied,
                    clock: BarClock::new(Timeframe::Day1),
                    instrument: None,
                    timeframe: Timeframe::Day1,
                    chart: None,
                };
                study.update_data(chart, 0, &[source]);
                let table = study.table().unwrap();
                for (cell, expected) in table.rows[0][1..].iter().zip(if supplied.is_empty() {
                    [None; 6]
                } else {
                    changes
                }) {
                    match expected {
                        Some(expected) => {
                            let StudyCell::Percent(Some(got)) = cell else {
                                panic!("a percentage: {cell:?}")
                            };
                            assert!(
                                (got - expected).abs() < 1e-10,
                                "{timeframe:?}: {got}, expected {expected}"
                            );
                        }
                        None => assert_eq!(*cell, StudyCell::Text("No data".into())),
                    }
                }
                assert_eq!(
                    table.cell_tooltips[0],
                    if supplied.is_empty() { [""; 7] } else { dates }
                );
                assert_eq!(study.data_requests(), std::slice::from_ref(&request));
                if timeframe == Timeframe::Month1 && !supplied.is_empty() {
                    // January closes at 120; all three starts use December's daily close, 100.
                    for line in [0, 1, 4] {
                        assert!((study.values().value(line, 2).unwrap() - 20.0).abs() < 1e-10);
                    }
                    assert_eq!(study.values().value(2, 2), None);
                }
            }
        }
        let daily = [daily.as_slice(), &[bar((2024, 3, 1, 210.0))]].concat();
        let grown = [monthly.as_slice(), &[bar((2024, 3, 1, 210.0))]].concat();
        let sources = [StudyData {
            request,
            bars: &daily,
            clock: BarClock::new(Timeframe::Day1),
            instrument: None,
            timeframe: Timeframe::Day1,
            chart: None,
        }];
        let mut updated = new(Timeframe::Month1);
        updated.update_data(&monthly, 0, &sources);
        updated.update_data(&grown, monthly.len(), &sources);
        let mut fresh = new(Timeframe::Month1);
        fresh.update_data(&grown, 0, &sources);
        for line in 0..6 {
            for bar in 0..grown.len() {
                assert_eq!(
                    updated.values().value(line, bar),
                    fresh.values().value(line, bar),
                    "line {line}, bar {bar}"
                );
            }
        }
        assert!((updated.values().value(1, 3).unwrap() - 60.0).abs() < 1e-10);
    }

    /// The chart symbol and each listed symbol get a row under a column per listed period, each
    /// symbol measured from its own latest daily close and one without history showing "No data".
    /// Each change is written in its sign's colour over a faint fill of it that deepens up to the
    /// cutoff, with the date of the close it is measured from as its tooltip, and a row is
    /// labelled with its instrument's description when it has one. A listed chart symbol shares
    /// its row while it is included.
    #[test]
    fn performance_compares_symbols_over_the_listed_periods() {
        use crate::model::{BarClock, Instrument, Timeframe};
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
        fn supplied<'a>(bars: &'a [Bar], instrument: Option<&'a Instrument>) -> StudyData<'a> {
            StudyData {
                request: StudyDataRequest {
                    symbol: "MSFT".into(),
                    timeframe: Some(Timeframe::Day1),
                    metric: None,
                },
                bars,
                clock: BarClock::new(Timeframe::Day1),
                instrument,
                timeframe: Timeframe::Day1,
                chart: None,
            }
        }
        let source = supplied(&msft, None);
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
                                StudyCell::Percent(Some(change)) => Some(*change),
                                StudyCell::Text(text) if text == "No data" => None,
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
        let six = || InputValue::Text("1W, 1M, 3M, 6M, YTD, 1Y".into());
        study
            .set_input("symbols", InputValue::Text("GOOG, MSFT, GOOG".into()))
            .unwrap();
        study.set_input("periods", six()).unwrap();
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
        // A change is written in its sign's colour over that colour at 10% plus 20% times the
        // change's share of the 40% cutoff: each channel times that, plus 0.5, rounded down.
        let table = study.table().unwrap();
        let grey = |alpha| Color32::from_rgba_unmultiplied(120, 123, 134, alpha);
        let tinted = |r, g, b, a| Color32::from_rgba_premultiplied(r, g, b, a);
        for (row, column, fill, text) in [
            // +20%, half the cutoff: 20% of the positive colour (8, 153, 129).
            (0, 1, tinted(2, 31, 26, 51), Some(palette::UP)),
            // +80%, beyond the cutoff: 30%.
            (0, 3, tinted(2, 46, 39, 77), Some(palette::UP)),
            // -50%, beyond the cutoff: 30% of the default negative colour (242, 55, 69).
            (
                2,
                2,
                tinted(73, 17, 21, 77),
                Some(Color32::from_rgb(242, 55, 69)),
            ),
            // No change: grey at 10% under the default text.
            (1, 1, grey(26), None),
            // A label: the same grey at 20%.
            (0, 0, grey(51), None),
        ] {
            assert_eq!(
                (
                    table.cell_colors[row][column],
                    table.cell_text_colors[row][column]
                ),
                (fill, text),
                "row {row}, column {column}"
            );
        }
        // The date of the close each change is measured from; none without a change.
        assert_eq!(
            table.cell_tooltips,
            [
                [
                    "",
                    "22 Mar 2024",
                    "29 Feb 2024",
                    "29 Dec 2023",
                    "29 Sep 2023",
                    "29 Dec 2023",
                    ""
                ],
                [""; 7],
                [
                    "",
                    "21 Mar 2024",
                    "28 Feb 2024",
                    "28 Dec 2023",
                    "28 Sep 2023",
                    "28 Dec 2023",
                    "28 Mar 2023"
                ],
            ]
        );
        // A row is labelled with the description its instrument gives: the chart's own, and a
        // compared symbol's from the source of its bars. A blank description keeps the ticker.
        let described =
            |symbol: &str, description: &str| Instrument::new(symbol).with_description(description);
        study.set_instrument(Some(&described("AAPL", "Apple Inc.")));
        assert_eq!(rows(&study)[0].0, "Apple Inc.");
        study.set_instrument(None);
        assert_eq!(rows(&study)[0].0, "AAPL");
        for (description, label) in [
            ("Microsoft Corporation", "Microsoft Corporation"),
            ("  ", "MSFT"),
        ] {
            let instrument = described("MSFT", description);
            study.update_data(&chart, 0, &[supplied(&msft, Some(&instrument))]);
            assert_eq!(rows(&study)[2].0, label);
        }
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
        // -20%, half the cutoff: 20% of the negative colour.
        assert_eq!(
            study.table().unwrap().cell_colors[2][4],
            tinted(48, 11, 14, 51)
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

        // With the chart symbol included, a listed copy of it is not a second row: the chart's
        // row comes first with the chart's own changes, the other listed symbols follow in list
        // order. Without it, the listed AAPL is a row of its own, without daily bars or changes.
        let aapl = (
            "AAPL".to_owned(),
            vec![
                Some(20.0),
                Some(44.0),
                Some(80.0),
                Some(125.0),
                Some(80.0),
                None,
            ],
        );
        let msft_row = (
            "MSFT".to_owned(),
            [25.0, -50.0, 50.0, 100.0, 50.0, 20.0].map(Some).to_vec(),
        );
        for (symbols, include, expected) in [
            ("AAPL, MSFT", true, [aapl.clone(), msft_row.clone()]),
            ("MSFT, AAPL", true, [aapl.clone(), msft_row.clone()]),
            ("MSFT", true, [aapl, msft_row.clone()]),
            (
                "AAPL, MSFT",
                false,
                [("AAPL".to_owned(), vec![None; 6]), msft_row],
            ),
        ] {
            let mut study = Performance::new();
            study.set_chart_symbol("AAPL");
            study.set_timeframe(Timeframe::Min1);
            study
                .set_input("symbols", InputValue::Text(symbols.into()))
                .unwrap();
            study.set_input("periods", six()).unwrap();
            study
                .set_input("include_chart_symbol", InputValue::Toggle(include))
                .unwrap();
            study.update_data(&chart, 0, std::slice::from_ref(&source));
            assert_eq!(rows(&study), expected, "{symbols}, include {include}");
        }

        // A new study compares five symbols over seven periods in a table that fills the pane.
        // On a daily chart that never moves, every period with a close to start from is a change
        // of zero on the positive colour at 10%; the bars start on 2023-09-29, after the 12M and
        // 5Y starts, and no compared symbol is supplied.
        let flat = chart
            .clone()
            .map(|bar| Bar::new(bar.time, 100.0, 100.0, 100.0, 100.0, 1.0));
        let symbols = ["AAPL", "NASDAQ:AMZN", "EURUSD", "ES1!", "BTCUSD"];
        let mut study = Performance::new();
        assert_eq!(study.data_requests(), symbols.map(daily));
        study.calculate(&flat);
        assert_eq!(
            columns(&study),
            ["Symbol", "1W", "1M", "3M", "6M", "12M", "YTD", "5Y"]
        );
        let unchanged = [true, true, true, true, false, true, false].map(|has| has.then_some(0.0));
        let mut expected = vec![("Chart".to_owned(), unchanged.to_vec())];
        expected.extend(symbols.map(|symbol| (symbol.to_owned(), vec![None; 7])));
        assert_eq!(rows(&study), expected);
        let table = study.table().unwrap();
        assert_eq!(table.cell_colors[0][1], tinted(1, 15, 13, 26));
        assert_eq!(
            (table.position, table.width, table.height),
            (TablePosition::MiddleCenter, Some(1.0), Some(0.95))
        );
        assert_eq!(
            study
                .inputs()
                .iter()
                .find(|input| input.id == "negative_color")
                .map(|input| &input.value),
            Some(&InputValue::Color(Color32::from_rgb(242, 55, 69)))
        );
    }
}
