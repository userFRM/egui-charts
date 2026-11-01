//! The result of a strategy run, and the performance metrics computed from it.
//!
//! A host runs its strategy wherever it likes (a backtest against the headless engine, a forward
//! test, its own analytics) and describes the result as a [`StrategyReport`]: the trades and the
//! equity curve. The library computes every metric itself with [`StrategyReport::performance`], with
//! one definition each, so reports from different sources compare.

use super::{Bar, OrderSide, Timeframe};
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use serde::{Deserialize, Deserializer, Serialize};

/// A year of 365.25 days, in milliseconds.
const YEAR_MS: f64 = 31_557_600_000.0;

/// Keep spreadsheet formula prefixes literal; CSV quoting alone does not prevent evaluation.
fn csv_text(value: &str) -> String {
    if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

/// Reads none as NaN: serde_json writes a non-finite number as `null`, which a plain `f64` refuses.
/// [`StrategyReport::performance`] skips a NaN like any non-finite value.
fn null_as_nan<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or(f64::NAN))
}

/// [`null_as_nan`] for the values of an equity curve.
fn curve_null_as_nan<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<(DateTime<Utc>, f64)>, D::Error> {
    let curve = Vec::<(DateTime<Utc>, Option<f64>)>::deserialize(deserializer)?;
    Ok(curve
        .into_iter()
        .map(|(t, v)| (t, v.unwrap_or(f64::NAN)))
        .collect())
}

/// The result of one strategy run, from any source: a backtest the host runs against the headless
/// engine, a forward test, or the host's own analytics. Build it with [`StrategyReport::new`] and set
/// the remaining fields directly; [`StrategyReport::performance`] computes the metrics.
///
/// # Example
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use egui_charts::model::{StrategyExit, StrategyReport, OrderSide, StrategyTrade, Timeframe};
///
/// let hour = |h| Utc.with_ymd_and_hms(2024, 1, 2, h, 0, 0).unwrap();
/// let mut report = StrategyReport::new("SMA cross", "AAPL", Timeframe::Hour1, 10_000.0);
/// report.currency = Some("USD".into());
/// report.properties.push(("Fast length".into(), "10".into()));
///
/// // The host computes each trade's net profit: here (190 - 185) * 10 shares less 2 of fees.
/// let mut trade = StrategyTrade::new(OrderSide::Buy, 10.0, hour(1), 185.0);
/// trade.fees = 2.0;
/// trade.exit = Some(StrategyExit::new(hour(3), 190.0, 48.0));
/// report.trades.push(trade);
/// report.equity = vec![(hour(0), 10_000.0), (hour(1), 9_998.0), (hour(2), 10_030.0), (hour(3), 10_048.0)];
///
/// let performance = report.performance();
/// assert_eq!(performance.net_profit, 48.0);
/// assert_eq!(performance.win_rate, Some(1.0));
/// assert_eq!(performance.max_drawdown, Some(2.0));
/// // Three hours of equity are too short to annualise.
/// assert_eq!(performance.sharpe_ratio, None);
/// assert!(report.trades_csv().starts_with("number,side,quantity,"));
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StrategyReport {
    /// What the run is called, as the host names it.
    pub name: String,
    /// The instrument the trades are in.
    pub symbol: String,
    /// The bar interval the run used.
    pub timeframe: Timeframe,
    /// Account value at the start, in `currency`. Positive and finite, or
    /// [`StrategyPerformance::net_profit_fraction`] is unavailable.
    #[serde(deserialize_with = "null_as_nan")]
    pub initial_capital: f64,
    /// Currency of capital, profits, fees and equity, such as "USD". Display only.
    #[serde(default)]
    pub currency: Option<String>,
    /// Annual risk-free rate as a fraction (0.02 for 2%), used by the Sharpe and Sortino ratios.
    /// Finite and above -1, or the ratios are unavailable.
    #[serde(default, deserialize_with = "null_as_nan")]
    pub risk_free_rate: f64,
    /// Settings of the run as the host wants them shown (parameters, commission, sizing), in order.
    /// Display only.
    #[serde(default)]
    pub properties: Vec<(String, String)>,
    /// Every trade, closed or open, in any order.
    #[serde(default)]
    pub trades: Vec<StrategyTrade>,
    /// Account value marked to market at each bar close, oldest first, one point per bar, in
    /// strictly increasing time order.
    #[serde(default, deserialize_with = "curve_null_as_nan")]
    pub equity: Vec<(DateTime<Utc>, f64)>,
}

/// One round trip. A host that scales out reports each partial exit as its own trade with its share
/// of the entry.
///
/// A trade is valid when `quantity` is finite and positive, `entry_price` and `fees` are finite, and,
/// if closed, the exit price and profit are finite and the exit is not before the entry. Prices may
/// be zero or negative. Invalid trades are left out of the metrics and the CSV, and counted in
/// [`StrategyPerformance::skipped_trades`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StrategyTrade {
    /// Long or short.
    pub side: OrderSide,
    /// Size of the position: positive and finite.
    #[serde(deserialize_with = "null_as_nan")]
    pub quantity: f64,
    /// When the position was opened.
    pub entry_time: DateTime<Utc>,
    /// Average fill price of the entry.
    #[serde(deserialize_with = "null_as_nan")]
    pub entry_price: f64,
    /// `None` while the trade is open.
    #[serde(default)]
    pub exit: Option<StrategyExit>,
    /// Commission and other costs of entry and exit, in the report's currency. Already included in
    /// the exit's profit.
    #[serde(default, deserialize_with = "null_as_nan")]
    pub fees: f64,
    /// The host's name for the entry, such as the rule that opened it. Display only.
    #[serde(default)]
    pub label: Option<String>,
}

/// How and when a trade was closed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StrategyExit {
    /// When the position was closed.
    pub time: DateTime<Utc>,
    /// Average fill price of the exit.
    #[serde(deserialize_with = "null_as_nan")]
    pub price: f64,
    /// Net profit of the trade after fees, in the report's currency, as the host computed it: the
    /// library does not model point values, inverse contracts or currency conversion.
    #[serde(deserialize_with = "null_as_nan")]
    pub profit: f64,
}

/// Metrics of a [`StrategyReport`], from [`StrategyReport::performance`].
///
/// Fractions throughout: 0.4 is a 40% win rate. A metric that is undefined for the run is `None`,
/// never 0 or infinity, and no `Option` metric is ever NaN or infinite. The sums add finite values and
/// are finite unless they exceed the range of `f64`.
///
/// Trade metrics use the closed valid trades, with `p` the net profit of each. Curve metrics use the
/// usable equity curve: the points with a finite value, provided they are in strictly increasing time
/// order. A curve out of order is unusable as a whole, so the curve metrics are `None` rather than
/// computed on part of it.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct StrategyPerformance {
    /// Valid trades with an exit.
    pub closed_trades: usize,
    /// Valid trades without an exit. They enter no trade metric; their value is in the equity curve.
    pub open_trades: usize,
    /// Closed trades with `p > 0`.
    pub winning_trades: usize,
    /// Closed trades with `p < 0`. A breakeven trade is neither a winner nor a loser.
    pub losing_trades: usize,
    /// Sum of `p`; 0 without closed trades.
    pub net_profit: f64,
    /// `net_profit / initial_capital`. `None` when the initial capital is not positive and finite.
    pub net_profit_fraction: Option<f64>,
    /// Sum of `p` over the winners.
    pub gross_profit: f64,
    /// Sum of `-p` over the losers, a positive number.
    pub gross_loss: f64,
    /// Sum of the fees of the closed trades, already included in `net_profit`.
    pub total_fees: f64,
    /// `gross_profit / gross_loss`. `None` without a losing trade.
    pub profit_factor: Option<f64>,
    /// `winning_trades / closed_trades`, breakeven trades included in the denominator. `None` without
    /// closed trades.
    pub win_rate: Option<f64>,
    /// `net_profit / closed_trades`. `None` without closed trades.
    pub average_trade: Option<f64>,
    /// Largest positive net profit, or `None` without a winning trade.
    pub largest_winning_trade: Option<f64>,
    /// Most negative net profit, or `None` without a losing trade.
    pub largest_losing_trade: Option<f64>,
    /// Longest winning run in exit-time order; ties keep input order. Breakeven trades break runs;
    /// open and invalid trades do not participate.
    pub max_consecutive_wins: usize,
    /// Longest losing run, with the same ordering and breaks as `max_consecutive_wins`.
    pub max_consecutive_losses: usize,
    /// The largest fall of equity from its highest earlier point, in money, over the curve from its
    /// first point. `None` without a usable equity point.
    pub max_drawdown: Option<f64>,
    /// The largest fall of equity as a fraction of its highest earlier point, found separately from
    /// `max_drawdown`, so the two can come from different points. Above 1 when equity falls below zero.
    /// `None` without a usable equity point or when the first point is not positive.
    pub max_drawdown_fraction: Option<f64>,
    /// Annualised Sharpe ratio: the mean excess return per step of the curve over the sample standard
    /// deviation of the returns, times the square root of `periods_per_year`. Returns are simple
    /// returns between consecutive usable points, and the excess is over the risk-free rate per step,
    /// `(1 + risk_free_rate)^(1 / periods_per_year) - 1`. `None` when `periods_per_year` is, with
    /// fewer than 2 returns, when the returns do not vary beyond rounding (a deviation of at most 16
    /// machine epsilons, as for a curve growing at a constant rate), when equity before the last
    /// point is not positive, or when the risk-free rate is not finite or not above -1.
    pub sharpe_ratio: Option<f64>,
    /// Annualised Sortino ratio: as `sharpe_ratio`, over the downside deviation instead, the root mean
    /// square of the shortfalls of the returns below the risk-free rate per step, taken over all
    /// returns. `None` when `sharpe_ratio` is, or when no return falls short.
    pub sortino_ratio: Option<f64>,
    /// Steps of the usable curve per year: the number of returns over the span from the first to the
    /// last point, in years of 365.25 days. Counting the bars a run actually contains follows the
    /// interval and the market's sessions without configuration.
    ///
    /// `None` unless the curve spans at least 13 weeks. The span leaves out the closed time before
    /// the first point and after the last, so a short run overstates the periods per year of a
    /// market with sessions: two sessions of 1-minute bars of a stock count about 224,000 a year
    /// instead of about 98,000, and the ratios come out about 1.5 times too high. Thirteen weeks
    /// contain every weekly closed period (nights, weekends) many times over, so from 13 weeks on the
    /// estimate is at most about 6% high, and the ratios about 3%, a small fraction of the sampling
    /// error of a ratio measured over a quarter.
    pub periods_per_year: Option<f64>,
    /// Invalid trades, left out of every metric (see [`StrategyTrade`]).
    pub skipped_trades: usize,
    /// Equity points with a non-finite value, or every point when the curve is not in strictly
    /// increasing time order.
    pub skipped_equity_points: usize,
}

/// Source-bar comparison of a run, from [`StrategyReport::comparison`].
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct StrategyComparison {
    /// Fee-free fractional return: last close / first open - 1. Requires a positive first open.
    pub buy_and_hold_return: Option<f64>,
    /// Initial capital invested at the first open, marked at each bar close. Times are the source
    /// bar timestamps, as in a chart. Empty without positive finite capital or a defined return.
    pub buy_and_hold_equity: Vec<(DateTime<Utc>, f64)>,
    /// One result per input trade, in input order. `None` for an invalid trade, absent source bars,
    /// or an invalid bar during the trade. See [`StrategyExcursion`] for the bar-resolution limits.
    pub excursions: Vec<Option<StrategyExcursion>>,
    /// Invalid OHLC bars, or all bars when timestamps are not strictly increasing.
    pub skipped_bars: usize,
}

/// Largest unrealised loss and gain relative to a trade's entry, before fees.
///
/// Both are nonnegative price movements times quantity, for a linear contract with unit point
/// value. They need conversion for other contracts or currencies. Entry and exit fills participate.
/// Source highs and lows participate from the bar containing entry through bars opening strictly
/// before exit; open trades use all remaining bars. At intrabar fills these are whole-bar estimates:
/// OHLC cannot distinguish movement before entry or after exit. Time bars end after their interval
/// (calendar months for monthly bars); execution bars end at the next source open, with the final
/// bar assumed to cover the run's remaining fills. Use fills at bar boundaries for exact spans.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StrategyExcursion {
    /// Maximum adverse excursion, as a positive loss magnitude.
    pub adverse: f64,
    /// Maximum favourable excursion, as a positive gain magnitude.
    pub favourable: f64,
}

impl StrategyReport {
    /// A report with no trades, no equity, no properties, no currency and a risk-free rate of 0.
    pub fn new(
        name: impl Into<String>,
        symbol: impl Into<String>,
        timeframe: Timeframe,
        initial_capital: f64,
    ) -> Self {
        Self {
            name: name.into(),
            symbol: symbol.into(),
            timeframe,
            initial_capital,
            currency: None,
            risk_free_rate: 0.0,
            properties: Vec::new(),
            trades: Vec::new(),
            equity: Vec::new(),
        }
    }

    /// Computes the metrics of the run. Sorts closed trades for streaks; call once per report,
    /// not once per frame.
    pub fn performance(&self) -> StrategyPerformance {
        let finite = |x: f64| Some(x).filter(|x| x.is_finite());
        let mut p = StrategyPerformance::default();
        for trade in &self.trades {
            if !trade.is_valid() {
                p.skipped_trades += 1;
                continue;
            }
            let Some(exit) = &trade.exit else {
                p.open_trades += 1;
                continue;
            };
            p.closed_trades += 1;
            p.net_profit += exit.profit;
            p.total_fees += trade.fees;
            if exit.profit > 0.0 {
                p.winning_trades += 1;
                p.gross_profit += exit.profit;
                p.largest_winning_trade =
                    Some(p.largest_winning_trade.unwrap_or(0.0).max(exit.profit));
            } else if exit.profit < 0.0 {
                p.losing_trades += 1;
                p.gross_loss -= exit.profit;
                p.largest_losing_trade =
                    Some(p.largest_losing_trade.unwrap_or(0.0).min(exit.profit));
            }
        }
        let mut exits: Vec<_> = self
            .trades
            .iter()
            .filter(|t| t.is_valid())
            .filter_map(|t| t.exit.as_ref())
            .collect();
        exits.sort_by_key(|e| e.time);
        let (mut wins, mut losses) = (0, 0);
        for exit in exits {
            wins = if exit.profit > 0.0 { wins + 1 } else { 0 };
            losses = if exit.profit < 0.0 { losses + 1 } else { 0 };
            p.max_consecutive_wins = p.max_consecutive_wins.max(wins);
            p.max_consecutive_losses = p.max_consecutive_losses.max(losses);
        }
        let closed = p.closed_trades as f64;
        let capital = self.initial_capital;
        p.net_profit_fraction = (capital.is_finite() && capital > 0.0)
            .then(|| p.net_profit / capital)
            .and_then(finite);
        p.profit_factor = finite(p.gross_profit / p.gross_loss);
        p.win_rate = finite(p.winning_trades as f64 / closed);
        p.average_trade = finite(p.net_profit / closed);

        let curve = self.equity_curve();
        p.skipped_equity_points = self.equity.len() - curve.len();
        let (Some(&(start, first)), Some(&(end, _))) = (curve.first(), curve.last()) else {
            return p;
        };
        let (mut peak, mut drawdown, mut fraction) = (first, 0.0_f64, 0.0_f64);
        for &(_, e) in &curve {
            peak = peak.max(e);
            drawdown = drawdown.max(peak - e);
            fraction = fraction.max((peak - e) / peak);
        }
        p.max_drawdown = finite(drawdown);
        p.max_drawdown_fraction = (first > 0.0).then_some(fraction).and_then(finite);

        let steps = curve.len() - 1;
        if end - start < TimeDelta::weeks(13) {
            return p;
        }
        let years = (end - start).num_milliseconds() as f64 / YEAR_MS;
        let periods_per_year = steps as f64 / years;
        p.periods_per_year = finite(periods_per_year);
        // Simple returns from a non-positive equity have no meaning, and a rate of -1 or below has no
        // per-step equivalent.
        let rate = self.risk_free_rate;
        if steps < 2 || rate <= -1.0 || curve[..steps].iter().any(|&(_, e)| e <= 0.0) {
            return p;
        }
        let returns: Vec<f64> = curve.windows(2).map(|w| w[1].1 / w[0].1 - 1.0).collect();
        let n = steps as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let deviation =
            (returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
        let rate_per_step = (1.0 + rate).powf(1.0 / periods_per_year) - 1.0;
        let downside = (returns
            .iter()
            .map(|r| (r - rate_per_step).min(0.0).powi(2))
            .sum::<f64>()
            / n)
            .sqrt();
        // Returns that are equal up to rounding do not vary.
        if deviation > 16.0 * f64::EPSILON {
            let excess = (mean - rate_per_step) * periods_per_year.sqrt();
            p.sharpe_ratio = finite(excess / deviation);
            p.sortino_ratio = finite(excess / downside);
        }
        p
    }

    /// Finite equity points in strictly increasing time order, using the same validation as
    /// [`Self::performance`]. Empty if the finite points are out of order.
    pub fn equity_curve(&self) -> Vec<(DateTime<Utc>, f64)> {
        let curve: Vec<_> = self
            .equity
            .iter()
            .copied()
            .filter(|(_, e)| e.is_finite())
            .collect();
        if curve.windows(2).any(|w| w[1].0 <= w[0].0) {
            Vec::new()
        } else {
            curve
        }
    }

    /// Computes buy-and-hold and excursions from the complete source bars of this run's symbol and
    /// interval, oldest first. The host supplies exactly the run's bars, including every held bar;
    /// this method neither fetches data nor infers missing sessions. Bar times denote opens.
    ///
    /// Non-finite or inverted OHLC bars are skipped and counted. Unordered or duplicate timestamps
    /// make the whole source unusable. Volume is unused. Buy-and-hold uses the first valid open and
    /// last valid close, with fractional units, no fees or reinvested dividends. A non-finite
    /// calculation is unavailable. Excursions require at least one held source bar; invalid held
    /// bars make that trade's excursion unavailable rather than understating risk.
    pub fn comparison(&self, bars: &[Bar]) -> StrategyComparison {
        let mut result = StrategyComparison {
            excursions: vec![None; self.trades.len()],
            ..Default::default()
        };
        if bars.windows(2).any(|w| w[1].time <= w[0].time) {
            result.skipped_bars = bars.len();
            return result;
        }
        let valid = |b: &Bar| {
            [b.open, b.high, b.low, b.close]
                .iter()
                .all(|p| p.is_finite())
                && b.low <= b.open.min(b.close)
                && b.high >= b.open.max(b.close)
        };
        let source: Vec<_> = bars.iter().filter(|b| valid(b)).collect();
        result.skipped_bars = bars.len() - source.len();
        if let (Some(first), Some(last)) = (source.first(), source.last()) {
            let fraction = last.close / first.open - 1.0;
            if first.open > 0.0 && fraction.is_finite() {
                result.buy_and_hold_return = Some(fraction);
                if self.initial_capital.is_finite() && self.initial_capital > 0.0 {
                    result.buy_and_hold_equity = source
                        .iter()
                        .map(|b| (b.time, self.initial_capital * (b.close / first.open)))
                        .collect();
                    if result
                        .buy_and_hold_equity
                        .iter()
                        .any(|(_, e)| !e.is_finite())
                    {
                        result.buy_and_hold_equity.clear();
                    }
                }
            }
        }
        for (i, trade) in self.trades.iter().enumerate().filter(|(_, t)| t.is_valid()) {
            let mut start = bars.partition_point(|b| b.time < trade.entry_time);
            if let Some(previous) = start.checked_sub(1).and_then(|i| bars.get(i)) {
                let close = match self.timeframe {
                    Timeframe::Month1 => previous.time.checked_add_months(chrono::Months::new(1)),
                    Timeframe::Months(count) => previous
                        .time
                        .checked_add_months(chrono::Months::new(count.get())),
                    tf if tf.is_trade_based() => bars.get(start).map(|b| b.time),
                    tf => previous.time.checked_add_signed(tf.duration()),
                };
                if close.is_none_or(|close| trade.entry_time < close) {
                    start -= 1;
                }
            }
            let end = trade
                .exit
                .as_ref()
                .map_or(bars.len(), |e| bars.partition_point(|b| b.time < e.time));
            let held = &bars[start..end];
            if held.is_empty() || held.iter().any(|b| !valid(b)) {
                continue;
            }
            let mut low = trade.entry_price;
            let mut high = trade.entry_price;
            for bar in held {
                low = low.min(bar.low);
                high = high.max(bar.high);
            }
            if let Some(exit) = &trade.exit {
                low = low.min(exit.price);
                high = high.max(exit.price);
            }
            let (adverse, favourable) = match trade.side {
                OrderSide::Buy => (trade.entry_price - low, high - trade.entry_price),
                OrderSide::Sell => (high - trade.entry_price, trade.entry_price - low),
            };
            let (adverse, favourable) = (adverse * trade.quantity, favourable * trade.quantity);
            if adverse.is_finite() && favourable.is_finite() {
                result.excursions[i] = Some(StrategyExcursion {
                    adverse,
                    favourable,
                });
            }
        }
        result
    }

    /// The valid trades as CSV, sorted by entry time (trades entered at the same time keep their
    /// order), one header line and one line per trade, each ending with `\n`.
    ///
    /// Columns: `number` (from 1, in this order), `side` (`long` or `short`), `quantity`,
    /// `entry_time`, `entry_price`, `exit_time`, `exit_price`, `profit`, `fees`, `label`. Times are
    /// RFC 3339 in UTC (`2024-01-02T00:00:00Z`), numbers use the shortest form that reads back to the
    /// same value, and an open trade leaves the three exit columns empty.
    /// Text cells starting with `=`, `+`, `-`, `@`, tab or carriage return gain a leading apostrophe
    /// so spreadsheets treat them as text. Numeric cells, including negative numbers, are unchanged.
    pub fn trades_csv(&self) -> String {
        const IN_MEMORY: &str = "writing CSV to memory cannot fail";
        let mut trades: Vec<&StrategyTrade> = self.trades.iter().filter(|t| t.is_valid()).collect();
        trades.sort_by_key(|t| t.entry_time);
        let time = |t: DateTime<Utc>| csv_text(&t.to_rfc3339_opts(SecondsFormat::AutoSi, true));
        let mut csv = csv::Writer::from_writer(Vec::new());
        csv.write_record([
            "number",
            "side",
            "quantity",
            "entry_time",
            "entry_price",
            "exit_time",
            "exit_price",
            "profit",
            "fees",
            "label",
        ])
        .expect(IN_MEMORY);
        for (i, trade) in trades.into_iter().enumerate() {
            let (exit_time, exit_price, profit) = match &trade.exit {
                Some(exit) => (
                    time(exit.time),
                    exit.price.to_string(),
                    exit.profit.to_string(),
                ),
                None => Default::default(),
            };
            let side = match trade.side {
                OrderSide::Buy => "long",
                OrderSide::Sell => "short",
            };
            csv.write_record([
                (i + 1).to_string(),
                side.to_string(),
                trade.quantity.to_string(),
                time(trade.entry_time),
                trade.entry_price.to_string(),
                exit_time,
                exit_price,
                profit,
                trade.fees.to_string(),
                csv_text(trade.label.as_deref().unwrap_or_default()),
            ])
            .expect(IN_MEMORY);
        }
        String::from_utf8(csv.into_inner().expect(IN_MEMORY)).expect("every field is a String")
    }

    /// The summary metrics and the properties of the run as CSV, one header line and one line per
    /// metric or property, each ending with `\n`. `performance` and `comparison` are this report's
    /// [`Self::performance`] and [`Self::comparison`], the figures the report panel shows.
    ///
    /// Columns: `section` (`summary` or `properties`), `name` (as the panel labels it), `value`,
    /// `unit` and `currency`. The unit is `money` (in `currency`, empty when the report has none),
    /// `percent` (100 is 100%), `ratio`, `count` or `text`. Percent values round to 15 significant
    /// decimal digits to suppress binary multiplication noise; if rounding would overflow `f64`,
    /// the unrounded finite value is kept. Numbers use the shortest form that reads back to the
    /// resulting value, with a dot as the decimal separator and no thousands separator. Every
    /// numeric zero is `0`, and an unavailable value is an empty cell, so numeric columns stay
    /// numeric in a spreadsheet.
    ///
    /// Text cells starting with `=`, `+`, `-`, `@`, tab or carriage return gain a leading apostrophe
    /// so spreadsheets treat them as text. This also applies to property names and currencies;
    /// numeric cells, including negative numbers, are unchanged by this protection.
    pub fn summary_csv(
        &self,
        performance: &StrategyPerformance,
        comparison: &StrategyComparison,
    ) -> String {
        const IN_MEMORY: &str = "writing CSV to memory cannot fail";
        let currency = csv_text(self.currency.as_deref().unwrap_or_default());
        let mut csv = csv::Writer::from_writer(Vec::new());
        csv.write_record(["section", "name", "value", "unit", "currency"])
            .expect(IN_MEMORY);
        let mut row = |section: &str, name: &str, value: String, unit: &str| {
            let name = csv_text(name);
            let value = if unit == "text" {
                csv_text(&value)
            } else {
                value
            };
            let currency = if unit == "money" { &currency } else { "" };
            csv.write_record([section, &name, &value, unit, currency])
                .expect(IN_MEMORY);
        };
        let number = |value: Option<f64>, percent: bool| {
            value
                .map(|v| if percent { v * 100.0 } else { v })
                .filter(|v| v.is_finite())
                .map_or_else(String::new, |v| {
                    let v = if percent {
                        let rounded = format!("{v:.14e}")
                            .parse::<f64>()
                            .expect("a formatted f64 parses as f64");
                        if rounded.is_finite() { rounded } else { v }
                    } else {
                        v
                    };
                    if v == 0.0 { "0".into() } else { v.to_string() }
                })
        };
        let p = performance;
        for (name, value, unit) in [
            ("Net profit", Some(p.net_profit), "money"),
            ("Net return", p.net_profit_fraction, "percent"),
            ("Gross profit", Some(p.gross_profit), "money"),
            ("Gross loss", Some(p.gross_loss), "money"),
            ("Profit factor", p.profit_factor, "ratio"),
            ("Win rate", p.win_rate, "percent"),
            ("Average trade", p.average_trade, "money"),
            ("Fees", Some(p.total_fees), "money"),
            ("Maximum drawdown", p.max_drawdown, "money"),
            ("Maximum drawdown %", p.max_drawdown_fraction, "percent"),
            ("Sharpe ratio", p.sharpe_ratio, "ratio"),
            ("Sortino ratio", p.sortino_ratio, "ratio"),
            (
                "Buy-and-hold return",
                comparison.buy_and_hold_return,
                "percent",
            ),
            ("Largest winning trade", p.largest_winning_trade, "money"),
            ("Largest losing trade", p.largest_losing_trade, "money"),
        ] {
            row("summary", name, number(value, unit == "percent"), unit);
        }
        for (name, count) in [
            ("Consecutive wins", p.max_consecutive_wins),
            ("Consecutive losses", p.max_consecutive_losses),
            ("Closed trades", p.closed_trades),
            ("Open trades", p.open_trades),
            ("Skipped trades", p.skipped_trades),
            ("Skipped equity points", p.skipped_equity_points),
            ("Skipped source bars", comparison.skipped_bars),
        ] {
            row("summary", name, count.to_string(), "count");
        }
        row("properties", "Name", self.name.clone(), "text");
        row("properties", "Symbol", self.symbol.clone(), "text");
        row("properties", "Interval", self.timeframe.to_string(), "text");
        row(
            "properties",
            "Initial capital",
            number(Some(self.initial_capital), false),
            "money",
        );
        row(
            "properties",
            "Currency",
            self.currency.clone().unwrap_or_default(),
            "text",
        );
        row(
            "properties",
            "Risk-free rate",
            number(Some(self.risk_free_rate), true),
            "percent",
        );
        for (name, value) in &self.properties {
            row("properties", name, value.clone(), "text");
        }
        String::from_utf8(csv.into_inner().expect(IN_MEMORY)).expect("every field is a String")
    }
}

impl StrategyTrade {
    /// An open trade with no fees and no label.
    pub fn new(
        side: OrderSide,
        quantity: f64,
        entry_time: DateTime<Utc>,
        entry_price: f64,
    ) -> Self {
        Self {
            side,
            quantity,
            entry_time,
            entry_price,
            exit: None,
            fees: 0.0,
            label: None,
        }
    }

    /// Whether this trade meets the validity rules documented on [`StrategyTrade`].
    pub fn is_valid(&self) -> bool {
        self.quantity.is_finite()
            && self.quantity > 0.0
            && self.entry_price.is_finite()
            && self.fees.is_finite()
            && self.exit.as_ref().is_none_or(|exit| {
                exit.price.is_finite() && exit.profit.is_finite() && exit.time >= self.entry_time
            })
    }
}

impl StrategyExit {
    /// An exit at `time` and `price` with the trade's net `profit` after fees.
    pub fn new(time: DateTime<Utc>, price: f64, profit: f64) -> Self {
        Self {
            time,
            price,
            profit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeDelta, TimeZone, Weekday};

    // Reference values below are computed independently from the definitions documented on
    // `StrategyPerformance` in double precision, not with this code.

    fn t0() -> DateTime<Utc> {
        // A Tuesday.
        Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap()
    }

    fn report(capital: f64) -> StrategyReport {
        StrategyReport::new("Test", "TEST", Timeframe::Hour1, capital)
    }

    fn curve(values: &[f64], step: TimeDelta) -> Vec<(DateTime<Utc>, f64)> {
        let at = |i: usize| t0() + step * i as i32;
        values
            .iter()
            .enumerate()
            .map(|(i, &v)| (at(i), v))
            .collect()
    }

    fn closed(profit: f64, fees: f64) -> StrategyTrade {
        let mut trade = StrategyTrade::new(OrderSide::Buy, 1.0, t0(), 100.0);
        trade.fees = fees;
        trade.exit = Some(StrategyExit::new(t0() + TimeDelta::hours(1), 101.0, profit));
        trade
    }

    /// Fixture A: one breakeven trade, one open trade, ten hourly equity points.
    fn fixture_a() -> StrategyReport {
        let mut r = report(10_000.0);
        let profits = [48.0, -11.0, 0.0, -21.0, 28.5];
        let fees = [1.0, 0.5, 0.0, 2.0, 1.5];
        r.trades = profits
            .iter()
            .zip(fees)
            .map(|(&p, f)| closed(p, f))
            .collect();
        let mut open = StrategyTrade::new(OrderSide::Sell, 2.0, t0(), 100.0);
        open.fees = 3.0;
        r.trades.push(open);
        r.equity = curve(
            &[
                10000.0, 10050.0, 10020.0, 10080.0, 9990.0, 10010.0, 10100.0, 10060.0, 10120.0,
                10090.0,
            ],
            TimeDelta::hours(1),
        );
        r
    }

    fn expected_a() -> StrategyPerformance {
        StrategyPerformance {
            closed_trades: 5,
            open_trades: 1,
            winning_trades: 2,
            losing_trades: 2,
            net_profit: 44.5,
            net_profit_fraction: Some(0.00445),
            gross_profit: 76.5,
            gross_loss: 32.0,
            total_fees: 5.0,
            profit_factor: Some(2.390625),
            win_rate: Some(0.4),
            average_trade: Some(8.9),
            largest_winning_trade: Some(48.0),
            largest_losing_trade: Some(-21.0),
            max_consecutive_wins: 1,
            max_consecutive_losses: 1,
            max_drawdown: Some(90.0),
            max_drawdown_fraction: Some(0.008928571428571428),
            ..Default::default()
        }
    }

    /// Fixture C: integer equity `10000 + 3i + (37i mod 101)` at the given times.
    fn fixture_c(times: impl Iterator<Item = DateTime<Utc>>, rate: f64) -> StrategyReport {
        let mut r = report(10_000.0);
        r.risk_free_rate = rate;
        r.equity = times
            .enumerate()
            .map(|(i, t)| (t, (10_000 + 3 * i + (37 * i) % 101) as f64))
            .collect();
        r
    }

    /// Hourly points spanning exactly 13 weeks, the shortest span that is annualised.
    fn hourly_13_weeks() -> impl Iterator<Item = DateTime<Utc>> {
        (0..2185).map(|h| t0() + TimeDelta::hours(h))
    }

    fn assert_matches(case: &str, actual: &StrategyPerformance, expected: &StrategyPerformance) {
        let close = |a: f64, b: f64| a == b || (a - b).abs() <= 1e-12 * a.abs().max(b.abs());
        let counts = |p: &StrategyPerformance| {
            [
                p.closed_trades,
                p.open_trades,
                p.winning_trades,
                p.losing_trades,
                p.skipped_trades,
                p.skipped_equity_points,
                p.max_consecutive_wins,
                p.max_consecutive_losses,
            ]
        };
        let sums =
            |p: &StrategyPerformance| [p.net_profit, p.gross_profit, p.gross_loss, p.total_fees];
        let options = |p: &StrategyPerformance| {
            [
                p.net_profit_fraction,
                p.profit_factor,
                p.win_rate,
                p.average_trade,
                p.max_drawdown,
                p.max_drawdown_fraction,
                p.sharpe_ratio,
                p.sortino_ratio,
                p.periods_per_year,
                p.largest_winning_trade,
                p.largest_losing_trade,
            ]
        };
        let same = counts(actual) == counts(expected)
            && sums(actual)
                .iter()
                .zip(sums(expected))
                .all(|(&a, b)| close(a, b))
            && options(actual)
                .iter()
                .zip(options(expected))
                .all(|pair| match pair {
                    (Some(a), Some(b)) => close(*a, b),
                    (a, b) => a.is_none() && b.is_none(),
                });
        assert!(
            same,
            "{case}:\n actual   {actual:?}\n expected {expected:?}"
        );
    }

    #[test]
    fn performance_matches_reference() {
        let mut invalid = fixture_a();
        let mut nan_profit = closed(5.0, 0.0);
        nan_profit.exit.as_mut().unwrap().profit = f64::NAN;
        let mut no_quantity = closed(5.0, 0.0);
        no_quantity.quantity = 0.0;
        let mut exit_before_entry = closed(5.0, 0.0);
        exit_before_entry.entry_time = t0() + TimeDelta::hours(2);
        invalid
            .trades
            .extend([nan_profit, no_quantity, exit_before_entry]);
        invalid
            .equity
            .insert(5, (t0() + TimeDelta::minutes(270), f64::INFINITY));

        let mut repeated_time = fixture_a();
        repeated_time.equity[5].0 = repeated_time.equity[4].0;

        let mut fixture_b = report(10_000.0);
        fixture_b.equity = curve(&[100.0, 50.0, 1000.0, 900.0], TimeDelta::hours(1));

        let weekdays = (0..)
            .map(|d| t0() + TimeDelta::days(d))
            .filter(|t| !matches!(t.weekday(), Weekday::Sat | Weekday::Sun))
            .take_while(|t| *t <= Utc.with_ymd_and_hms(2025, 1, 3, 0, 0, 0).unwrap());
        let mut weekday_report = fixture_c(weekdays, 0.0);
        weekday_report.timeframe = Timeframe::Day1;

        let mut negative_equity = report(10_000.0);
        negative_equity.equity = curve(&[100.0, -50.0, 25.0, 30.0], TimeDelta::days(122));

        let mut zero_start = report(10_000.0);
        zero_start.equity = curve(&[0.0, 10.0, 5.0], TimeDelta::hours(1));

        let mut negative_capital = report(-1_000.0);
        negative_capital.trades.push(closed(5.0, 0.0));

        let mut constant_growth = report(10_000.0);
        constant_growth.risk_free_rate = 0.02;
        let growth: Vec<f64> = (0..10).map(|i| 100.0 * 1.0001_f64.powf(i as f64)).collect();
        constant_growth.equity = curve(&growth, TimeDelta::days(41));

        // A non-finite number in every kind of field: one invalid trade, one skipped equity point.
        let mut non_finite = report(f64::NAN);
        non_finite.risk_free_rate = f64::NAN;
        non_finite.equity = vec![(t0(), f64::NAN)];
        let mut trade = closed(5.0, f64::NEG_INFINITY);
        trade.quantity = f64::INFINITY;
        trade.entry_price = f64::NAN;
        trade.exit.as_mut().unwrap().price = f64::NAN;
        non_finite.trades.push(trade);

        // In exit order: ++---0++0+. Sum 8, winners 18, losses 10; runs 2 and 3.
        // The input order differs, and open/invalid trades do not interrupt a run.
        let mut streaks = report(10_000.0);
        let chronological: Vec<_> = [5.0, 3.0, -1.0, -7.0, -2.0, 0.0, 4.0, 4.0, 0.0, 2.0]
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let mut t = closed(p, 0.0);
                t.exit.as_mut().unwrap().time += TimeDelta::hours(i as i64);
                t
            })
            .collect();
        streaks.trades = [9, 1, 6, 3, 8, 0, 5, 2, 7, 4]
            .map(|i| chronological[i].clone())
            .to_vec();
        streaks
            .trades
            .push(StrategyTrade::new(OrderSide::Buy, 1.0, t0(), 100.0));
        let mut invalid_streak = closed(99.0, 0.0);
        invalid_streak.quantity = 0.0;
        streaks.trades.push(invalid_streak);

        let no_curve = StrategyPerformance {
            max_drawdown: None,
            max_drawdown_fraction: None,
            ..expected_a()
        };
        let rows = [
            (
                "streaks ordered by exit, with breakevens, open and invalid trades",
                streaks,
                StrategyPerformance {
                    closed_trades: 10,
                    open_trades: 1,
                    winning_trades: 5,
                    losing_trades: 3,
                    net_profit: 8.0,
                    net_profit_fraction: Some(0.0008),
                    gross_profit: 18.0,
                    gross_loss: 10.0,
                    profit_factor: Some(1.8),
                    win_rate: Some(0.5),
                    average_trade: Some(0.8),
                    largest_winning_trade: Some(5.0),
                    largest_losing_trade: Some(-7.0),
                    max_consecutive_wins: 2,
                    max_consecutive_losses: 3,
                    skipped_trades: 1,
                    ..Default::default()
                },
            ),
            ("A", fixture_a(), expected_a()),
            (
                "A with invalid trades and an infinite equity value",
                invalid,
                StrategyPerformance {
                    skipped_trades: 3,
                    skipped_equity_points: 1,
                    ..expected_a()
                },
            ),
            (
                "A with a repeated timestamp",
                repeated_time,
                StrategyPerformance {
                    skipped_equity_points: 10,
                    ..no_curve
                },
            ),
            (
                "B",
                fixture_b,
                StrategyPerformance {
                    net_profit_fraction: Some(0.0),
                    max_drawdown: Some(100.0),
                    max_drawdown_fraction: Some(0.5),
                    ..Default::default()
                },
            ),
            (
                "C hourly over 13 weeks, rate 0.02",
                fixture_c(hourly_13_weeks(), 0.02),
                StrategyPerformance {
                    net_profit_fraction: Some(0.0),
                    max_drawdown: Some(82.0),
                    max_drawdown_fraction: Some(0.008104368452263293),
                    periods_per_year: Some(8766.0),
                    sharpe_ratio: Some(5.864663773071857),
                    sortino_ratio: Some(7.742250169261054),
                    ..Default::default()
                },
            ),
            (
                "C weekdays over 367 days, rate 0",
                weekday_report,
                StrategyPerformance {
                    net_profit_fraction: Some(0.0),
                    max_drawdown: Some(82.0),
                    max_drawdown_fraction: Some(0.008104368452263293),
                    periods_per_year: Some(261.74591280653954),
                    sharpe_ratio: Some(1.08244877567277),
                    sortino_ratio: Some(1.433886945862868),
                    ..Default::default()
                },
            ),
            (
                "C hourly, rate -1",
                fixture_c(hourly_13_weeks(), -1.0),
                StrategyPerformance {
                    net_profit_fraction: Some(0.0),
                    max_drawdown: Some(82.0),
                    max_drawdown_fraction: Some(0.008104368452263293),
                    periods_per_year: Some(8766.0),
                    ..Default::default()
                },
            ),
            (
                "equity below zero",
                negative_equity,
                StrategyPerformance {
                    net_profit_fraction: Some(0.0),
                    max_drawdown: Some(150.0),
                    max_drawdown_fraction: Some(1.5),
                    periods_per_year: Some(2.9938524590163933),
                    ..Default::default()
                },
            ),
            (
                "equity starting at zero",
                zero_start,
                StrategyPerformance {
                    net_profit_fraction: Some(0.0),
                    max_drawdown: Some(5.0),
                    ..Default::default()
                },
            ),
            (
                "one winning trade, negative capital",
                negative_capital,
                StrategyPerformance {
                    closed_trades: 1,
                    winning_trades: 1,
                    net_profit: 5.0,
                    gross_profit: 5.0,
                    win_rate: Some(1.0),
                    average_trade: Some(5.0),
                    largest_winning_trade: Some(5.0),
                    max_consecutive_wins: 1,
                    ..Default::default()
                },
            ),
            (
                "constant growth, rate 0.02",
                constant_growth,
                StrategyPerformance {
                    net_profit_fraction: Some(0.0),
                    max_drawdown: Some(0.0),
                    max_drawdown_fraction: Some(0.0),
                    periods_per_year: Some(8.908536585365853),
                    ..Default::default()
                },
            ),
            (
                "non-finite capital, rate, trade fields and equity",
                non_finite,
                StrategyPerformance {
                    skipped_trades: 1,
                    skipped_equity_points: 1,
                    ..Default::default()
                },
            ),
            (
                "empty",
                report(10_000.0),
                StrategyPerformance {
                    net_profit_fraction: Some(0.0),
                    ..Default::default()
                },
            ),
        ];
        for (case, report, expected) in rows {
            assert_matches(case, &report.performance(), &expected);
            let loaded: StrategyReport =
                serde_json::from_str(&serde_json::to_string(&report).unwrap())
                    .unwrap_or_else(|e| panic!("{case}: {e}"));
            assert_matches(
                &format!("{case}, saved and loaded"),
                &loaded.performance(),
                &expected,
            );
        }
    }

    #[test]
    fn comparison_matches_source_bar_reference() {
        // Investing 1,000 at 100 buys 10 units: closes 110, 90, 120 give 1,100, 900, 1,200.
        let bars = [
            (100.0, 115.0, 95.0, 110.0),
            (110.0, 112.0, 85.0, 90.0),
            (90.0, 125.0, 88.0, 120.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (o, h, l, c))| {
            super::super::Bar::new(t0() + TimeDelta::hours(i as i64), o, h, l, c, 1.0)
        })
        .collect::<Vec<_>>();
        let mut r = report(1_000.0);
        let mut long = StrategyTrade::new(OrderSide::Buy, 2.0, t0(), 100.0);
        long.exit = Some(StrategyExit::new(t0() + TimeDelta::hours(2), 90.0, -20.0));
        let mut short = long.clone();
        short.side = OrderSide::Sell;
        short.entry_price = 110.0;
        short.quantity = 3.0;
        let open = StrategyTrade::new(OrderSide::Buy, 1.0, t0() + TimeDelta::hours(2), 90.0);
        let mut invalid = long.clone();
        invalid.quantity = 0.0;
        r.trades = vec![long, short, open, invalid];
        let mut sessions = bars.clone();
        sessions[2].time += TimeDelta::days(3);
        for (case, source, skipped, expected_return, expected_excursions) in [
            (
                "complete",
                bars.clone(),
                0,
                Some(0.2),
                vec![
                    Some((30.0, 30.0)),
                    Some((15.0, 75.0)),
                    Some((2.0, 35.0)),
                    None,
                ],
            ),
            (
                "session gap does not change price-based results",
                sessions,
                0,
                Some(0.2),
                vec![
                    Some((30.0, 30.0)),
                    Some((15.0, 75.0)),
                    Some((2.0, 35.0)),
                    None,
                ],
            ),
            ("empty", vec![], 0, None, vec![None; 4]),
            (
                "unordered",
                vec![bars[1].clone(), bars[0].clone()],
                2,
                None,
                vec![None; 4],
            ),
            (
                "duplicate",
                vec![bars[0].clone(), bars[0].clone()],
                2,
                None,
                vec![None; 4],
            ),
        ] {
            let c = r.comparison(&source);
            assert_eq!(c.skipped_bars, skipped, "{case}");
            assert!(
                match (c.buy_and_hold_return, expected_return) {
                    (Some(a), Some(b)) => (a - b).abs() < 1e-12,
                    (a, b) => a == b,
                },
                "{case}"
            );
            assert_eq!(
                c.excursions
                    .iter()
                    .map(|e| e.as_ref().map(|e| (e.adverse, e.favourable)))
                    .collect::<Vec<_>>(),
                expected_excursions,
                "{case}"
            );
            if expected_return.is_some() {
                assert_eq!(
                    c.buy_and_hold_equity
                        .iter()
                        .map(|p| p.1)
                        .collect::<Vec<_>>(),
                    [1100.0, 900.0, 1200.0]
                );
            } else {
                assert!(c.buy_and_hold_equity.is_empty());
            }
        }
        let mut invalid_bars = bars.clone();
        invalid_bars[1].high = f64::NAN;
        let c = r.comparison(&invalid_bars);
        assert_eq!(c.skipped_bars, 1);
        assert_eq!(c.excursions[0], None); // A missing held bar must not understate risk.
        for capital in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            r.initial_capital = capital;
            assert!(r.comparison(&bars).buy_and_hold_equity.is_empty());
        }
        let mut zero = bars.clone();
        zero[0].open = 0.0;
        zero[0].low = 0.0;
        assert_eq!(r.comparison(&zero).buy_and_hold_return, None);

        // For two units entered at 100, the first bar's [95, 115] gives MAE/MFE 10/30.
        // The second bar adds a low of 85, giving 30/30. A fill at its open excludes its range.
        for (case, entry_minutes, exit_minutes, expected) in [
            ("intrabar round trip", 30, 45, Some((10.0, 30.0))),
            ("intrabar entry across bars", 30, 120, Some((30.0, 30.0))),
            ("exit at the next open", 30, 60, Some((10.0, 30.0))),
            ("entry at the next open", 60, 120, Some((30.0, 24.0))),
            ("entry after the source ends", 240, 300, None),
        ] {
            let mut r = report(1_000.0);
            let mut t = StrategyTrade::new(
                OrderSide::Buy,
                2.0,
                t0() + TimeDelta::minutes(entry_minutes),
                100.0,
            );
            t.exit = Some(StrategyExit::new(
                t0() + TimeDelta::minutes(exit_minutes),
                100.0,
                0.0,
            ));
            r.trades.push(t);
            assert_eq!(
                r.comparison(&bars).excursions[0]
                    .as_ref()
                    .map(|e| (e.adverse, e.favourable)),
                expected,
                "{case}"
            );
        }
        for (timeframe, entry) in [
            (Timeframe::Ms500, TimeDelta::milliseconds(250)),
            (Timeframe::Month1, TimeDelta::days(30)),
            ("3month".parse().unwrap(), TimeDelta::days(90)),
            (
                Timeframe::Ticks(std::num::NonZeroU64::new(10).unwrap()),
                TimeDelta::minutes(30),
            ),
        ] {
            let mut r = report(1_000.0);
            r.timeframe = timeframe;
            let mut t = StrategyTrade::new(OrderSide::Buy, 2.0, t0() + entry, 100.0);
            t.exit = Some(StrategyExit::new(
                t0() + entry + TimeDelta::milliseconds(1),
                100.0,
                0.0,
            ));
            r.trades.push(t);
            assert_eq!(
                r.comparison(&bars[..1]).excursions[0],
                Some(StrategyExcursion {
                    adverse: 10.0,
                    favourable: 30.0,
                }),
                "{timeframe} entry bar"
            );
        }
    }

    #[test]
    fn trades_csv_writes_valid_trades_in_entry_order() {
        let mut r = report(10_000.0);
        let mut late = StrategyTrade::new(OrderSide::Sell, 2.0, t0() + TimeDelta::hours(5), 101.5);
        late.label = Some("Breakout, short".into());
        let mut first = closed(-1.25, 0.25);
        first.entry_time = t0() + TimeDelta::hours(1);
        first.exit.as_mut().unwrap().time = t0() + TimeDelta::hours(2);
        let mut second = closed(3.0, 0.5);
        second.entry_time = first.entry_time;
        second.exit.as_mut().unwrap().time = t0() + TimeDelta::hours(3);
        let mut invalid = closed(1.0, 0.0);
        invalid.quantity = -1.0;
        r.trades = vec![late, invalid, first, second];
        assert_eq!(
            r.trades_csv(),
            "number,side,quantity,entry_time,entry_price,exit_time,exit_price,profit,fees,label\n\
             1,long,1,2024-01-02T01:00:00Z,100,2024-01-02T02:00:00Z,101,-1.25,0.25,\n\
             2,long,1,2024-01-02T01:00:00Z,100,2024-01-02T03:00:00Z,101,3,0.5,\n\
             3,short,2,2024-01-02T05:00:00Z,101.5,,,,0,\"Breakout, short\"\n"
        );
        for (label, expected) in [
            ("=1+1", "'=1+1"),
            ("+SUM(1,2)", "'+SUM(1,2)"),
            ("-1+2", "'-1+2"),
            ("@SUM(A1)", "'@SUM(A1)"),
            ("\t=1", "'\t=1"),
            ("\r=1", "'\r=1"),
            ("ordinary, \"quoted\"\nline", "ordinary, \"quoted\"\nline"),
            ("", ""),
            ("λ=1", "λ=1"),
            ("'=already text", "'=already text"),
        ] {
            let mut trade = closed(-1.25, -0.25);
            trade.entry_price = -100.0;
            trade.exit.as_mut().unwrap().price = -101.0;
            trade.label = Some(label.into());
            r.trades = vec![trade];
            let csv = r.trades_csv();
            let row = csv::Reader::from_reader(csv.as_bytes())
                .records()
                .next()
                .unwrap()
                .unwrap();
            assert_eq!(&row[9], expected, "{label:?}");
            for (column, expected) in [(4, "-100"), (6, "-101"), (7, "-1.25"), (8, "-0.25")] {
                assert_eq!(&row[column], expected, "numeric column {column}");
            }
        }
        for (year, expected) in [
            (-1, "'-0001-01-02T00:00:00Z"),
            (10_000, "'+10000-01-02T00:00:00Z"),
        ] {
            let time = Utc.with_ymd_and_hms(year, 1, 2, 0, 0, 0).unwrap();
            let mut trade = StrategyTrade::new(OrderSide::Buy, 1.0, time, 100.0);
            trade.exit = Some(StrategyExit::new(time, 100.0, 0.0));
            r.trades = vec![trade];
            let csv = r.trades_csv();
            let row = csv::Reader::from_reader(csv.as_bytes())
                .records()
                .next()
                .unwrap()
                .unwrap();
            assert_eq!(&row[3], expected);
            assert_eq!(&row[5], expected);
        }
    }

    #[test]
    fn summary_csv_writes_metrics_and_properties_with_unavailable_values() {
        let mut r = fixture_a();
        r.currency = Some("USD".into());
        r.properties
            .push(("Fast, length".into(), "say \"hi\"".into()));
        assert_eq!(
            r.summary_csv(&r.performance(), &r.comparison(&[])),
            "section,name,value,unit,currency\n\
             summary,Net profit,44.5,money,USD\n\
             summary,Net return,0.445,percent,\n\
             summary,Gross profit,76.5,money,USD\n\
             summary,Gross loss,32,money,USD\n\
             summary,Profit factor,2.390625,ratio,\n\
             summary,Win rate,40,percent,\n\
             summary,Average trade,8.9,money,USD\n\
             summary,Fees,5,money,USD\n\
             summary,Maximum drawdown,90,money,USD\n\
             summary,Maximum drawdown %,0.892857142857143,percent,\n\
             summary,Sharpe ratio,,ratio,\n\
             summary,Sortino ratio,,ratio,\n\
             summary,Buy-and-hold return,,percent,\n\
             summary,Largest winning trade,48,money,USD\n\
             summary,Largest losing trade,-21,money,USD\n\
             summary,Consecutive wins,1,count,\n\
             summary,Consecutive losses,1,count,\n\
             summary,Closed trades,5,count,\n\
             summary,Open trades,1,count,\n\
             summary,Skipped trades,0,count,\n\
             summary,Skipped equity points,0,count,\n\
             summary,Skipped source bars,0,count,\n\
             properties,Name,Test,text,\n\
             properties,Symbol,TEST,text,\n\
             properties,Interval,1h,text,\n\
             properties,Initial capital,10000,money,USD\n\
             properties,Currency,USD,text,\n\
             properties,Risk-free rate,0,percent,\n\
             properties,\"Fast, length\",\"say \"\"hi\"\"\",text,\n"
        );
        for (text, expected) in [
            ("=1+1", "'=1+1"),
            ("+SUM(1,2)", "'+SUM(1,2)"),
            ("-1+2", "'-1+2"),
            ("@SUM(A1)", "'@SUM(A1)"),
            ("\t=1", "'\t=1"),
            ("\r=1", "'\r=1"),
            ("ordinary, \"quoted\"\nline", "ordinary, \"quoted\"\nline"),
            ("", ""),
            ("λ=1", "λ=1"),
            ("'=already text", "'=already text"),
        ] {
            let mut r = report(1000.0);
            r.name = text.into();
            r.symbol = text.into();
            r.currency = Some(text.into());
            r.properties.push((text.into(), text.into()));
            r.trades.push(closed(-1.25, -0.25));
            let csv = r.summary_csv(&r.performance(), &r.comparison(&[]));
            let rows: Vec<_> = csv::Reader::from_reader(csv.as_bytes())
                .records()
                .map(Result::unwrap)
                .collect();
            for name in ["Name", "Symbol", "Currency"] {
                let row = rows.iter().find(|row| &row[1] == name).unwrap();
                assert_eq!(&row[2], expected, "{name}: {text:?}");
            }
            let property = rows.last().unwrap();
            assert_eq!(&property[1], expected, "property name: {text:?}");
            assert_eq!(&property[2], expected, "property value: {text:?}");
            for row in rows.iter().filter(|row| &row[3] == "money") {
                assert_eq!(&row[4], expected, "currency: {text:?}");
            }
            let loss = rows.iter().find(|row| &row[1] == "Net profit").unwrap();
            assert_eq!(&loss[2], "-1.25");
            let fees = rows.iter().find(|row| &row[1] == "Fees").unwrap();
            assert_eq!(&fees[2], "-0.25");
        }
        // Decimal reference: multiplying a fraction by 100 shifts its decimal point twice.
        for (fraction, expected) in [
            (0.07, "7"),
            (0.0725, "7.25"),
            (-0.0, "0"),
            (-0.0725, "-7.25"),
            (1e-20, "0.000000000000000001"),
            (1.0 / 3.0, "33.3333333333333"),
            (f64::NAN, ""),
            (f64::INFINITY, ""),
            (f64::MAX, ""),
        ] {
            r.risk_free_rate = fraction;
            let mut p = r.performance();
            p.net_profit_fraction = Some(fraction);
            p.win_rate = Some(fraction);
            p.max_drawdown_fraction = Some(fraction);
            let mut c = r.comparison(&[]);
            c.buy_and_hold_return = Some(fraction);
            let csv = r.summary_csv(&p, &c);
            let rows: Vec<_> = csv::Reader::from_reader(csv.as_bytes())
                .records()
                .map(Result::unwrap)
                .collect();
            for row in rows.iter().filter(|row| &row[3] == "percent") {
                assert_eq!(&row[2], expected, "{}: {fraction}", &row[1]);
            }
        }
        // Rounding the largest finite percent to 15 digits would overflow; retain its value.
        r.risk_free_rate = f64::MAX / 100.0;
        let csv = r.summary_csv(&r.performance(), &r.comparison(&[]));
        let rate = csv::Reader::from_reader(csv.as_bytes())
            .records()
            .map(Result::unwrap)
            .find(|row| &row[1] == "Risk-free rate")
            .unwrap();
        assert_eq!(rate[2].parse::<f64>().unwrap(), f64::MAX);
        r.initial_capital = -0.0;
        let p = StrategyPerformance {
            net_profit: -0.0,
            gross_profit: -0.0,
            gross_loss: -0.0,
            total_fees: -0.0,
            profit_factor: Some(-0.0),
            average_trade: Some(-0.0),
            largest_winning_trade: Some(-0.0),
            largest_losing_trade: Some(-0.0),
            max_drawdown: Some(-0.0),
            sharpe_ratio: Some(-0.0),
            sortino_ratio: Some(-0.0),
            ..Default::default()
        };
        let csv = r.summary_csv(&p, &StrategyComparison::default());
        for row in csv::Reader::from_reader(csv.as_bytes()).records() {
            let row = row.unwrap();
            if matches!(&row[3], "money" | "ratio") {
                assert_eq!(&row[2], "0", "{}", &row[1]);
            }
        }
        // Nothing to compute from: counts are zero, everything else is an empty cell.
        let empty = report(0.0);
        let csv = empty.summary_csv(&empty.performance(), &empty.comparison(&[]));
        assert!(csv.contains("summary,Win rate,,percent,\n"), "{csv}");
        assert!(csv.contains("summary,Net profit,0,money,\n"), "{csv}");
        assert!(csv.contains("properties,Currency,,text,\n"), "{csv}");
    }
}
