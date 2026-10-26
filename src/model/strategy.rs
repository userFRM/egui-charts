//! The result of a strategy run, and the performance metrics computed from it.
//!
//! A host runs its strategy wherever it likes (a backtest against the headless engine, a forward
//! test, its own analytics) and describes the result as a [`StrategyReport`]: the trades and the
//! equity curve. The library computes every metric itself with [`StrategyReport::performance`], with
//! one definition each, so reports from different sources compare.

use super::Timeframe;
use crate::model::OrderSide;
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use serde::{Deserialize, Deserializer, Serialize};

/// A year of 365.25 days, in milliseconds.
const YEAR_MS: f64 = 31_557_600_000.0;

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

    /// Computes the metrics of the run. Linear in the number of trades and equity points: call it
    /// once per report, not once per frame.
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
            } else if exit.profit < 0.0 {
                p.losing_trades += 1;
                p.gross_loss -= exit.profit;
            }
        }
        let closed = p.closed_trades as f64;
        let capital = self.initial_capital;
        p.net_profit_fraction = (capital.is_finite() && capital > 0.0)
            .then(|| p.net_profit / capital)
            .and_then(finite);
        p.profit_factor = finite(p.gross_profit / p.gross_loss);
        p.win_rate = finite(p.winning_trades as f64 / closed);
        p.average_trade = finite(p.net_profit / closed);

        let mut curve: Vec<(DateTime<Utc>, f64)> = self
            .equity
            .iter()
            .copied()
            .filter(|(_, e)| e.is_finite())
            .collect();
        p.skipped_equity_points = self.equity.len() - curve.len();
        if curve.windows(2).any(|w| w[1].0 <= w[0].0) {
            p.skipped_equity_points = self.equity.len();
            curve.clear();
        }
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

    /// The valid trades as CSV, sorted by entry time (trades entered at the same time keep their
    /// order), one header line and one line per trade, each ending with `\n`.
    ///
    /// Columns: `number` (from 1, in this order), `side` (`long` or `short`), `quantity`,
    /// `entry_time`, `entry_price`, `exit_time`, `exit_price`, `profit`, `fees`, `label`. Times are
    /// RFC 3339 in UTC (`2024-01-02T00:00:00Z`), numbers use the shortest form that reads back to the
    /// same value, and an open trade leaves the three exit columns empty.
    pub fn trades_csv(&self) -> String {
        const IN_MEMORY: &str = "writing CSV to memory cannot fail";
        let mut trades: Vec<&StrategyTrade> = self.trades.iter().filter(|t| t.is_valid()).collect();
        trades.sort_by_key(|t| t.entry_time);
        let time = |t: DateTime<Utc>| t.to_rfc3339_opts(SecondsFormat::AutoSi, true);
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
                trade.label.clone().unwrap_or_default(),
            ])
            .expect(IN_MEMORY);
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

    fn is_valid(&self) -> bool {
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

        let no_curve = StrategyPerformance {
            max_drawdown: None,
            max_drawdown_fraction: None,
            ..expected_a()
        };
        let rows = [
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
    }
}
