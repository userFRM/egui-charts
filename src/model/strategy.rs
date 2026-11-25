//! The result of a strategy run, and the performance metrics computed from it.
//!
//! A host runs its strategy wherever it likes (a backtest against the headless engine, a forward
//! test, its own analytics) and describes the result as a [`StrategyReport`]: the trades and the
//! equity curve. The library computes every metric itself with [`StrategyReport::performance`], with
//! one definition each, so reports from different sources compare.

use super::{Bar, OrderSide, Timeframe};
#[cfg(feature = "csv")]
use chrono::SecondsFormat;
use chrono::{DateTime, Datelike, Months, TimeDelta, TimeZone, Timelike, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeSet, ops::Range};

/// A year of 365.25 days, in milliseconds.
const YEAR_MS: f64 = 31_557_600_000.0;

/// Keep spreadsheet formula prefixes literal; CSV quoting alone does not prevent evaluation.
#[cfg(feature = "csv")]
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

/// The exchange rate of a trade in the instrument's own currency.
fn unit_rate() -> f64 {
    1.0
}

/// Leaves the rate of an unconverted trade out of saved reports, which stay as they were.
fn is_unit_rate(rate: &f64) -> bool {
    *rate == 1.0
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

/// Finite points in strictly increasing time order; empty if the finite points are out of order.
fn usable_curve(points: &[(DateTime<Utc>, f64)]) -> Vec<(DateTime<Utc>, f64)> {
    let curve: Vec<_> = points
        .iter()
        .copied()
        .filter(|(_, value)| value.is_finite())
        .collect();
    if curve.windows(2).any(|w| w[1].0 <= w[0].0) {
        Vec::new()
    } else {
        curve
    }
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
/// assert_eq!(performance.all.net_profit, 48.0);
/// assert_eq!(performance.all.win_rate, Some(1.0));
/// assert_eq!(performance.max_drawdown, Some(2.0));
/// // Three hours of equity are too short to annualise.
/// assert_eq!(performance.sharpe_ratio, None);
/// # #[cfg(feature = "csv")]
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
    /// [`StrategyTradeStats::net_profit_fraction`] is unavailable.
    #[serde(deserialize_with = "null_as_nan")]
    pub initial_capital: f64,
    /// Currency of capital, profits, fees, equity and margin, such as "USD". Display only.
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
    /// Decimals of the prices in the list of trades and in the trade markers of
    /// [`Self::signals`], such as 2 for a price increment of 0.01. Each price shows at least
    /// these, up to 12, and more where it has them, such as 0.00001234; `None` shows each price
    /// with its own decimals. A backtest sets it from its price increment; for a host's own run,
    /// set it from `scales::instrument_price_precision`. Display only.
    #[serde(default)]
    pub price_decimals: Option<u8>,
    /// Every trade, closed or open, in any order.
    #[serde(default)]
    pub trades: Vec<StrategyTrade>,
    /// Account value marked to market at each bar close, oldest first, one point per bar, in
    /// strictly increasing time order.
    #[serde(default, deserialize_with = "curve_null_as_nan")]
    pub equity: Vec<(DateTime<Utc>, f64)>,
    /// Margin the open position used at each equity mark, at the times of [`Self::equity`]: its
    /// absolute market value times its side's funding fraction in the account currency, zero
    /// while flat or unfunded.
    /// Empty when no margin was recorded, including older saved reports; margin metrics are then
    /// unavailable.
    #[serde(default, deserialize_with = "curve_null_as_nan")]
    pub margin: Vec<(DateTime<Utc>, f64)>,
    /// Largest margin used along each bar's observed price path, including before each fill
    /// and at the close, at the times of [`Self::margin`].
    #[serde(default, deserialize_with = "curve_null_as_nan")]
    pub peak_margin: Vec<(DateTime<Utc>, f64)>,
    /// Time and quantity closed of each margin call, in fill order.
    #[serde(default, deserialize_with = "curve_null_as_nan")]
    pub margin_calls: Vec<(DateTime<Utc>, f64)>,
    /// How equity moved inside each bar: one range per equity point in the same order, or empty
    /// when unknown. Usable when lengths match and each finite point has four finite range values,
    /// `low <= value <= high` and nonnegative fall and rise. Skipped points' ranges are ignored.
    /// Otherwise the intrabar metrics are unavailable.
    #[serde(default)]
    pub equity_ranges: Vec<EquityRange>,
    /// Why the run has no figures, such as exchange rates into its currency that the host did
    /// not supply. Such a report has no trades, equity or margin data, and its metrics describe nothing;
    /// show the reason instead. Display only.
    #[serde(default)]
    pub unavailable: Option<String>,
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
    /// Units of the report's currency per unit of the instrument's currency at which a backtest
    /// converted this trade's profit: the rate at its exit, or for an open trade the latest rate
    /// of the run. 1 when the report is in the instrument's currency, and for trades a host
    /// builds unless it sets another. [`StrategyReport::comparison`] converts the trade's
    /// excursions at it; a rate that is not finite and positive leaves them unavailable. Price,
    /// quantity and fees are not affected.
    #[serde(
        default = "unit_rate",
        skip_serializing_if = "is_unit_rate",
        deserialize_with = "null_as_nan"
    )]
    pub exchange_rate: f64,
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
    /// Net profit of the trade after fees, in the report's currency, as the host or a backtest
    /// computed it. A backtest converts price differences from the instrument's currency at each
    /// exit's exchange rate; the library does not model point values or inverse contracts.
    #[serde(deserialize_with = "null_as_nan")]
    pub profit: f64,
}

/// Trade figures of a [`StrategyPerformance`], over every valid trade or the trades of one side.
///
/// `p` is each closed valid trade's net profit: positive wins, negative loses, zero is even.
/// Even trades count as closed, in the win rate's denominator and in the average trade, never
/// as winners or losers. Open trades count only in `open_trades` and `largest_position`;
/// their value is in the equity curve. Sums add finite values and are finite unless they exceed
/// the range of `f64`; no `Option` figure is ever NaN or infinite.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct StrategyTradeStats {
    /// Valid trades with an exit.
    pub closed_trades: usize,
    /// Valid trades without an exit. They enter no other figure but `largest_position`; their
    /// value is in the equity curve.
    pub open_trades: usize,
    /// Closed trades with `p > 0`.
    pub winning_trades: usize,
    /// Closed trades with `p < 0`.
    pub losing_trades: usize,
    /// Closed trades with `p == 0`.
    pub even_trades: usize,
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
    /// `winning_trades / closed_trades`, even trades included in the denominator. `None` without
    /// closed trades.
    pub win_rate: Option<f64>,
    /// `net_profit / closed_trades`. `None` without closed trades.
    pub average_trade: Option<f64>,
    /// `gross_profit / winning_trades`. `None` without a winning trade.
    pub average_winning_trade: Option<f64>,
    /// `gross_loss / losing_trades`, a positive number. `None` without a losing trade.
    pub average_losing_trade: Option<f64>,
    /// `average_winning_trade / average_losing_trade`. `None` unless both are available.
    pub win_loss_ratio: Option<f64>,
    /// Largest positive net profit, or `None` without a winning trade.
    pub largest_winning_trade: Option<f64>,
    /// `largest_winning_trade / gross_profit`. `None` without a winning trade.
    pub largest_winning_fraction: Option<f64>,
    /// Most negative net profit, or `None` without a losing trade.
    pub largest_losing_trade: Option<f64>,
    /// `-largest_losing_trade / gross_loss`, a positive number. `None` without a losing trade.
    pub largest_losing_fraction: Option<f64>,
    /// `net_profit / -largest_losing_trade`, negative for a net loss. `None` without a losing trade.
    pub net_profit_to_largest_loss: Option<f64>,
    /// Longest winning run of the set in exit-time order; ties keep input order. Even trades
    /// break runs; open and invalid trades do not participate.
    pub max_consecutive_wins: usize,
    /// Longest losing run, with the same ordering and breaks as `max_consecutive_wins`.
    pub max_consecutive_losses: usize,
    /// Largest summed quantity of valid trades held at one time, open trades included. A trade
    /// is held from entry up to its exit, the exit excluded, so a reversal never adds both
    /// positions; a trade closed at its entry time counts at that instant. Over all trades,
    /// long and short quantities add. `None` without a valid trade.
    pub largest_position: Option<f64>,
}

/// Metrics of a [`StrategyReport`], from [`StrategyReport::performance`].
///
/// Fractions throughout: 0.4 is a 40% win rate. A metric that is undefined for the run is `None`,
/// never 0 or infinity, and no `Option` metric is ever NaN or infinite.
///
/// Trade figures come for every valid trade in `all` and for each side in `long` and `short`,
/// with the rules of [`StrategyTradeStats`]. Curve metrics use the usable equity curve: the points
/// with a finite value, provided they are in strictly increasing time order. A curve out of order
/// is unusable as a whole, so its curve metrics are `None` rather than computed on part of it.
/// Margin metrics use [`StrategyReport::margin`], [`StrategyReport::peak_margin`] and
/// [`StrategyReport::margin_calls`], and are all `None` when `margin` is empty.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct StrategyPerformance {
    /// Trade figures over every valid trade.
    pub all: StrategyTradeStats,
    /// Trade figures over the long trades, [`OrderSide::Buy`].
    pub long: StrategyTradeStats,
    /// Trade figures over the short trades, [`OrderSide::Sell`].
    pub short: StrategyTradeStats,
    /// Open trades' profit after their entry fees: last usable equity less initial capital and
    /// closed net profit. Exactly 0 without a valid open trade. `None` with open trades but no
    /// usable equity or a non-finite result.
    pub open_profit: Option<f64>,
    /// Largest close-to-close fall below an earlier equity peak, in money, from the curve's
    /// first point. `None` without a usable equity point.
    pub max_drawdown: Option<f64>,
    /// The largest fall of equity as a fraction of its highest earlier point, found separately from
    /// `max_drawdown`, so the two can come from different points. Above 1 when equity falls below zero.
    /// `None` without a usable equity point or when the first point is not positive.
    pub max_drawdown_fraction: Option<f64>,
    /// Mean depth of the close-to-close drawdown phases, in money. A phase starts at an equity
    /// peak, at the last point before equity falls below it, and ends at the first point where
    /// equity is back at or above that peak; its depth is the peak less the lowest point in
    /// between. A phase still open at the last point counts with its lowest point so far.
    /// `None` without a phase, or when the mean is not finite.
    pub average_drawdown: Option<f64>,
    /// Mean length of those drawdown phases in calendar days of 24 hours, from the peak to the
    /// recovery, or to the last point for a phase still open there. `None` without a phase.
    pub average_drawdown_days: Option<f64>,
    /// Largest fall using [`StrategyReport::equity_ranges`]: at each bar the larger of the earlier
    /// peak less its low and its own [`EquityRange::fall`]. At least `max_drawdown`.
    /// `None` without usable ranges.
    pub intrabar_max_drawdown: Option<f64>,
    /// Largest intrabar drawdown fraction, measured separately from its money maximum: the larger
    /// of (earlier peak - low) / earlier peak and fall / max(earlier peak, high). A fall starting
    /// below both peaks has a lower fraction than its own. `None` without usable ranges or when
    /// the first usable range's high is not positive.
    pub intrabar_max_drawdown_fraction: Option<f64>,
    /// Intrabar maximum drawdown divided by initial capital. `None` without usable ranges or
    /// positive finite capital, or if the result is not finite.
    pub intrabar_max_drawdown_initial_capital_fraction: Option<f64>,
    /// Largest close-to-close rise above an earlier equity trough, in money. `None` without
    /// a usable equity point.
    pub max_run_up: Option<f64>,
    /// `max_run_up / initial_capital`. `None` without a run-up, positive finite capital or a
    /// finite result.
    pub max_run_up_fraction: Option<f64>,
    /// Mean height of the close-to-close run-up phases, in money. A phase starts at a trough:
    /// the lowest point of a drawdown phase or the curve's first point, at the last point equity
    /// was there. It ends at the next peak, the highest point before the following drawdown
    /// phase starts, at the first point equity was there; its height is that peak less the
    /// trough. A phase still open at the last point counts with its highest point so far.
    /// `None` without a phase, or when the mean is not finite.
    pub average_run_up: Option<f64>,
    /// Mean length of those run-up phases in calendar days of 24 hours, from the trough to the
    /// peak. `None` without a phase.
    pub average_run_up_days: Option<f64>,
    /// Largest rise using [`StrategyReport::equity_ranges`]: at each bar the larger of its high
    /// less the earlier trough and its own [`EquityRange::rise`]. At least `max_run_up`.
    /// `None` without usable ranges.
    pub intrabar_max_run_up: Option<f64>,
    /// `intrabar_max_run_up / initial_capital`, subject to the conditions of `max_run_up_fraction`.
    pub intrabar_max_run_up_fraction: Option<f64>,
    /// `all.net_profit / max_drawdown`, both in money. `None` without a usable equity point or when
    /// the maximum drawdown is 0.
    pub return_over_max_drawdown: Option<f64>,
    /// Compound annual growth as a fraction: `(last / first)^(1 / years) - 1`, in years of
    /// 365.25 days; -1 when the curve ends at 0. `None` unless the usable curve spans at least
    /// 13 weeks, when its first point is not positive, or when its last point is negative.
    pub compound_annual_growth: Option<f64>,
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
    /// Margin calls that closed a finite positive quantity. `None` without margin data.
    pub margin_calls: Option<usize>,
    /// Sum of quantities those calls closed, zero without one. `None` without margin data.
    pub liquidated_quantity: Option<f64>,
    /// Largest quantity one margin call closed. `None` without a margin call.
    pub largest_liquidation: Option<f64>,
    /// Mean of finite [`StrategyReport::margin`] values, flat marks counting as zero.
    /// `None` without a finite mark.
    pub average_margin: Option<f64>,
    /// Largest finite [`StrategyReport::peak_margin`] value. `None` without margin data or a
    /// finite peak.
    pub largest_margin: Option<f64>,
    /// `largest_margin + intrabar_max_drawdown`: capital to fund the largest margin and survive
    /// the largest intrabar drawdown. Uses `max_drawdown` when the report has no usable
    /// [`StrategyReport::equity_ranges`]. `None` without margin or drawdown.
    pub account_size_required: Option<f64>,
    /// `all.net_profit / account_size_required`. `None` unless the required size is positive.
    pub account_size_return: Option<f64>,
    /// `all.net_profit / average_margin`. `None` unless the average margin is positive.
    pub margin_efficiency: Option<f64>,
}

/// Average holding times of closed trades in source bars, from [`StrategyReport::comparison`].
///
/// Bars held are the difference between the positions of the bars containing entry and exit:
/// entry on bar 3 and exit on bar 7 holds 4 bars; entry and exit within one bar holds 0.
/// A time belongs to the last source bar opening at or before it, so a fill in a closed session
/// counts on the bar before. Bars with invalid prices still count. Open trades and trades
/// entered before the first bar are left out.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct StrategyHolding {
    /// Average over closed trades, even trades included. `None` without one.
    pub average_bars: Option<f64>,
    /// Average over winning trades. `None` without one.
    pub average_winning_bars: Option<f64>,
    /// Average over losing trades. `None` without one.
    pub average_losing_bars: Option<f64>,
}

/// Source-bar comparison of a run, from [`StrategyReport::comparison`].
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct StrategyComparison {
    /// Fee-free fractional return: last close / first open - 1. Requires a positive first open.
    pub buy_and_hold_return: Option<f64>,
    /// `initial_capital * buy_and_hold_return`, in money. `None` without a return or without
    /// positive finite initial capital.
    pub buy_and_hold_profit: Option<f64>,
    /// Net profit of closed valid trades minus `buy_and_hold_profit`: negative when passive
    /// holding makes more. Open trades do not count. `None` without `buy_and_hold_profit`.
    pub buy_and_hold_outperformance: Option<f64>,
    /// Initial capital invested at the first open, marked at each bar close. Times are the source
    /// bar timestamps, as in a chart. Empty without positive finite capital or a defined return.
    pub buy_and_hold_equity: Vec<(DateTime<Utc>, f64)>,
    /// One result per input trade, in input order. `None` for an invalid trade, absent source bars,
    /// or an invalid bar during the trade. See [`StrategyExcursion`] for the bar-resolution limits.
    pub excursions: Vec<Option<StrategyExcursion>>,
    /// Holding times of every valid trade. Empty of values without usable source bars.
    pub all: StrategyHolding,
    /// Holding times of long trades.
    pub long: StrategyHolding,
    /// Holding times of short trades.
    pub short: StrategyHolding,
    /// Invalid OHLC bars, or all bars when timestamps are not strictly increasing.
    pub skipped_bars: usize,
}

/// Largest unrealised loss and gain relative to a trade's entry, before fees.
///
/// Both are nonnegative price movements times quantity, converted into the report's currency at
/// the trade's [`StrategyTrade::exchange_rate`], for a linear contract with unit point value;
/// other contracts need conversion by the host. Entry and exit fills participate.
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

/// Equity movements inside one bar, aligned with [`StrategyReport::equity`]. Backtests record
/// the path their fills walk. Hosts may leave [`StrategyReport::equity_ranges`] empty.
///
/// If only the extremes are known, use `high - low` for fall and rise to obtain upper bounds.
///
/// # Example
///
/// ```
/// use egui_charts::model::EquityRange;
///
/// // 100 units held from 100 through a dip to 70 and a recovery to 100.
/// let range = EquityRange::new(7000.0, 10_000.0, 3000.0, 3000.0);
/// assert_eq!((range.low, range.fall), (7000.0, 3000.0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EquityRange {
    /// Lowest equity reached, including the close.
    #[serde(deserialize_with = "null_as_nan")]
    pub low: f64,
    /// Highest equity reached, including the close.
    #[serde(deserialize_with = "null_as_nan")]
    pub high: f64,
    /// Largest fall below an earlier value in this bar, nonnegative.
    #[serde(deserialize_with = "null_as_nan")]
    pub fall: f64,
    /// Largest rise above an earlier value in this bar, nonnegative.
    #[serde(deserialize_with = "null_as_nan")]
    pub rise: f64,
}

impl EquityRange {
    /// Supply a bar's extremes and the largest fall and rise inside it.
    pub fn new(low: f64, high: f64, fall: f64, rise: f64) -> Self {
        Self {
            low,
            high,
            fall,
            rise,
        }
    }
}

/// Drawdown and run-up of one usable equity point.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CurveExcursion {
    pub(crate) drawdown: f64,
    pub(crate) drawdown_fraction: f64,
    pub(crate) run_up: f64,
}

/// Calendar, result and entry-time breakdowns from [`StrategyReport::analysis`].
/// Trade figures use closed valid trades; curve returns use [`StrategyReport::equity_curve`].
/// Every calendar boundary and entry-time group uses UTC.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StrategyAnalysis {
    /// Months containing a usable equity point or a closed trade's exit, oldest first.
    pub months: Vec<StrategyPeriod>,
    /// Years containing a usable equity point or a closed trade's exit, oldest first.
    pub years: Vec<StrategyPeriod>,
    /// Result-bin width as a fraction of entry value. The first width on the 0.001, 0.002,
    /// 0.005, 0.01, 0.02, 0.05, ... ladder that fits the results in at most 20 bins;
    /// 0.001 without results.
    pub result_bin_width: f64,
    /// Consecutive bins of net profit / (quantity × absolute entry price), lowest first.
    /// Trades with a non-finite result, including a zero entry value, are omitted. Entry value
    /// stays in the instrument's currency, so a converted run's results carry the exchange rate.
    pub result_bins: Vec<StrategyResultBin>,
    /// Entry `n - 1` counts winning runs of exactly `n` trades, in exit order, with the same
    /// breaks as [`StrategyTradeStats::max_consecutive_wins`]. Length is the longest run.
    pub win_runs: Vec<usize>,
    /// Losing runs, counted in the same way as `win_runs`.
    pub loss_runs: Vec<usize>,
    /// Closed trades grouped by entry hour, from 00:00 to 23:00 UTC.
    pub by_hour: [StrategyTradeGroup; 24],
    /// Closed trades grouped by entry weekday, Monday first.
    pub by_weekday: [StrategyTradeGroup; 7],
    /// Closed trades grouped by entry month, January first.
    pub by_month: [StrategyTradeGroup; 12],
}

/// A calendar month or year in [`StrategyAnalysis`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StrategyPeriod {
    /// Midnight UTC on the first day of the period.
    pub start: DateTime<Utc>,
    /// Last equity in the period / last equity before it - 1. Without an earlier point the
    /// period's first point is the base. `None` without a point, a positive base or a finite return.
    pub equity_return: Option<f64>,
    /// Last valid source close in the period / last valid close before it - 1. Without an
    /// earlier valid bar, the period's first valid open is the base. Bars follow
    /// [`StrategyReport::comparison`]'s validation. `None` without a valid bar in the period,
    /// a positive base or a finite return, or when the source timestamps are unusable.
    pub buy_and_hold_return: Option<f64>,
    /// Closed valid trades attributed by exit time.
    pub trades: StrategyTradeGroup,
}

/// The closed valid trades in one analysis group.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct StrategyTradeGroup {
    /// Number of closed trades, including breakevens.
    pub closed_trades: usize,
    /// Trades with positive net profit.
    pub winning_trades: usize,
    /// Sum of net profits; zero without trades.
    pub net_profit: f64,
    /// Winning trades / closed trades, or `None` without closed trades.
    pub win_rate: Option<f64>,
}

/// A result interval in [`StrategyAnalysis::result_bins`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct StrategyResultBin {
    /// Included lower edge, a multiple of the bin width; the next edge is excluded. Values
    /// within a billionth of an edge (of the width at zero) snap to it to remove binary rounding
    /// noise. A loss always stays below the bin starting at zero.
    pub lower: f64,
    /// Trades with positive net profit.
    pub winners: usize,
    /// Trades with negative net profit.
    pub losers: usize,
    /// Trades with zero net profit.
    pub breakeven: usize,
}

fn finite(x: f64) -> Option<f64> {
    Some(x).filter(|x| x.is_finite())
}

fn trade_stats(trades: &[&StrategyTrade], capital: f64) -> StrategyTradeStats {
    let mut p = StrategyTradeStats::default();
    for trade in trades {
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
            p.largest_winning_trade = Some(p.largest_winning_trade.unwrap_or(0.0).max(exit.profit));
        } else if exit.profit < 0.0 {
            p.losing_trades += 1;
            p.gross_loss -= exit.profit;
            p.largest_losing_trade = Some(p.largest_losing_trade.unwrap_or(0.0).min(exit.profit));
        } else {
            p.even_trades += 1;
        }
    }
    let mut exits: Vec<_> = trades.iter().filter_map(|t| t.exit.as_ref()).collect();
    exits.sort_by_key(|e| e.time);
    let (mut wins, mut losses) = (0, 0);
    for exit in exits {
        wins = if exit.profit > 0.0 { wins + 1 } else { 0 };
        losses = if exit.profit < 0.0 { losses + 1 } else { 0 };
        p.max_consecutive_wins = p.max_consecutive_wins.max(wins);
        p.max_consecutive_losses = p.max_consecutive_losses.max(losses);
    }
    let closed = p.closed_trades as f64;
    p.net_profit_fraction = (capital.is_finite() && capital > 0.0)
        .then(|| p.net_profit / capital)
        .and_then(finite);
    p.profit_factor = finite(p.gross_profit / p.gross_loss);
    p.win_rate = finite(p.winning_trades as f64 / closed);
    p.average_trade = finite(p.net_profit / closed);

    p.average_winning_trade = finite(p.gross_profit / p.winning_trades as f64);
    p.average_losing_trade = finite(p.gross_loss / p.losing_trades as f64);
    p.win_loss_ratio = p
        .average_winning_trade
        .zip(p.average_losing_trade)
        .map(|(w, l)| w / l)
        .and_then(finite);
    p.largest_winning_fraction = p
        .largest_winning_trade
        .map(|w| w / p.gross_profit)
        .and_then(finite);
    p.largest_losing_fraction = p
        .largest_losing_trade
        .map(|l| -l / p.gross_loss)
        .and_then(finite);
    p.net_profit_to_largest_loss = p
        .largest_losing_trade
        .map(|l| p.net_profit / -l)
        .and_then(finite);

    // Earlier exits precede entries at the same time; zero-length trades exit after entries.
    let mut events = Vec::with_capacity(2 * trades.len());
    for trade in trades {
        events.push((trade.entry_time, 1, trade.quantity));
        if let Some(exit) = &trade.exit {
            let order = if exit.time == trade.entry_time { 2 } else { 0 };
            events.push((exit.time, order, -trade.quantity));
        }
    }
    events.sort_by_key(|&(time, order, _)| (time, order));
    let (mut held, mut largest) = (0.0_f64, 0.0_f64);
    for (_, _, quantity) in events {
        held += quantity;
        largest = largest.max(held);
    }
    p.largest_position = (!trades.is_empty()).then_some(largest).and_then(finite);
    p
}

/// Totals of the close-to-close drawdown and run-up phases of a usable equity curve.
#[derive(Default)]
struct Phases {
    drawdowns: usize,
    /// Sum of the drawdown depths, in money.
    drawdown: f64,
    drawdown_seconds: f64,
    run_ups: usize,
    /// Sum of the run-up heights, in money.
    run_up: f64,
    run_up_seconds: f64,
}

/// Finds the drawdown and run-up phases [`StrategyPerformance::average_drawdown`] defines.
fn curve_phases(curve: &[(DateTime<Utc>, f64)]) -> Phases {
    let mut phases = Phases::default();
    let (Some(&(start, first)), Some(&(end, _))) = (curve.first(), curve.last()) else {
        return phases;
    };
    // The running peak and the last time equity was at it.
    let (mut peak, mut peak_time) = (first, start);
    // The run-up in progress: the lowest point since that peak with the last time at it, and
    // the highest point since then with the first time at it.
    let (mut trough, mut trough_time) = (first, start);
    let (mut top, mut top_time) = (first, start);
    let mut below = false;
    for &(time, equity) in &curve[1..] {
        if equity < peak {
            if !below || equity <= trough {
                if !below && top > trough {
                    // The run-up ended at the peak this drawdown starts from.
                    phases.run_ups += 1;
                    phases.run_up += top - trough;
                    phases.run_up_seconds += (top_time - trough_time).as_seconds_f64();
                }
                below = true;
                (trough, trough_time, top, top_time) = (equity, time, equity, time);
            } else if equity > top {
                (top, top_time) = (equity, time);
            }
        } else {
            if below {
                phases.drawdowns += 1;
                phases.drawdown += peak - trough;
                phases.drawdown_seconds += (time - peak_time).as_seconds_f64();
                below = false;
            }
            if equity > top {
                (top, top_time) = (equity, time);
            } else if top == trough {
                // Flat since the first point: the run-up starts at the last such point.
                (trough_time, top_time) = (time, time);
            }
            (peak, peak_time) = (equity, time);
        }
    }
    // Phases still open at the last point count with their extremes so far.
    if below {
        phases.drawdowns += 1;
        phases.drawdown += peak - trough;
        phases.drawdown_seconds += (end - peak_time).as_seconds_f64();
    }
    if top > trough {
        phases.run_ups += 1;
        phases.run_up += top - trough;
        phases.run_up_seconds += (top_time - trough_time).as_seconds_f64();
    }
    phases
}

/// How a summary figure reads, named as the `unit` column of [`StrategyReport::summary_csv`].
#[cfg(feature = "csv")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SummaryUnit {
    Money,
    Percent,
    Ratio,
    Count,
    Quantity,
    Bars,
    Days,
}

#[cfg(feature = "csv")]
impl SummaryUnit {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Money => "money",
            Self::Percent => "percent",
            Self::Ratio => "ratio",
            Self::Count => "count",
            Self::Quantity => "quantity",
            Self::Bars => "bars",
            Self::Days => "days",
        }
    }
}

/// A trade figure with its values for all trades, longs and shorts.
#[cfg(feature = "csv")]
pub(crate) type SideFigure = (&'static str, SummaryUnit, [Option<f64>; 3]);
#[cfg(feature = "csv")]
type SideValue = fn(&StrategyTradeStats, &StrategyHolding) -> Option<f64>;

/// Trade figures in the order the panel and the CSV list them.
#[cfg(feature = "csv")]
pub(crate) fn trade_figures(p: &StrategyPerformance, c: &StrategyComparison) -> Vec<SideFigure> {
    use SummaryUnit::*;
    let sides = [(&p.all, &c.all), (&p.long, &c.long), (&p.short, &c.short)];
    let figures: [(&str, SummaryUnit, SideValue); 27] = [
        ("Net profit", Money, |s, _| Some(s.net_profit)),
        ("Net return", Percent, |s, _| s.net_profit_fraction),
        ("Gross profit", Money, |s, _| Some(s.gross_profit)),
        ("Gross loss", Money, |s, _| Some(s.gross_loss)),
        ("Profit factor", Ratio, |s, _| s.profit_factor),
        ("Fees", Money, |s, _| Some(s.total_fees)),
        ("Closed trades", Count, |s, _| Some(s.closed_trades as f64)),
        ("Open trades", Count, |s, _| Some(s.open_trades as f64)),
        ("Winning trades", Count, |s, _| {
            Some(s.winning_trades as f64)
        }),
        ("Losing trades", Count, |s, _| Some(s.losing_trades as f64)),
        ("Even trades", Count, |s, _| Some(s.even_trades as f64)),
        ("Win rate", Percent, |s, _| s.win_rate),
        ("Average trade", Money, |s, _| s.average_trade),
        ("Average winning trade", Money, |s, _| {
            s.average_winning_trade
        }),
        ("Average losing trade", Money, |s, _| s.average_losing_trade),
        ("Average win / average loss", Ratio, |s, _| s.win_loss_ratio),
        ("Largest winning trade", Money, |s, _| {
            s.largest_winning_trade
        }),
        ("Largest winner % of gross profit", Percent, |s, _| {
            s.largest_winning_fraction
        }),
        ("Largest losing trade", Money, |s, _| s.largest_losing_trade),
        ("Largest loser % of gross loss", Percent, |s, _| {
            s.largest_losing_fraction
        }),
        ("Net profit / largest loss", Ratio, |s, _| {
            s.net_profit_to_largest_loss
        }),
        ("Consecutive wins", Count, |s, _| {
            Some(s.max_consecutive_wins as f64)
        }),
        ("Consecutive losses", Count, |s, _| {
            Some(s.max_consecutive_losses as f64)
        }),
        ("Largest position", Quantity, |s, _| s.largest_position),
        ("Average bars in trades", Bars, |_, h| h.average_bars),
        ("Average bars in winning trades", Bars, |_, h| {
            h.average_winning_bars
        }),
        ("Average bars in losing trades", Bars, |_, h| {
            h.average_losing_bars
        }),
    ];
    figures
        .into_iter()
        .map(|(label, unit, value)| (label, unit, sides.map(|(s, h)| value(s, h))))
        .collect()
}

/// Figures of the whole run, after the trade figures.
#[cfg(feature = "csv")]
pub(crate) fn run_figures(
    p: &StrategyPerformance,
    c: &StrategyComparison,
) -> Vec<(&'static str, SummaryUnit, Option<f64>)> {
    use SummaryUnit::*;
    vec![
        ("Open profit", Money, p.open_profit),
        ("Maximum drawdown (close-to-close)", Money, p.max_drawdown),
        (
            "Maximum drawdown % (close-to-close)",
            Percent,
            p.max_drawdown_fraction,
        ),
        (
            "Average drawdown (close-to-close)",
            Money,
            p.average_drawdown,
        ),
        (
            "Average drawdown duration in days (close-to-close)",
            Days,
            p.average_drawdown_days,
        ),
        (
            "Maximum drawdown (intrabar)",
            Money,
            p.intrabar_max_drawdown,
        ),
        (
            "Maximum drawdown % (intrabar)",
            Percent,
            p.intrabar_max_drawdown_fraction,
        ),
        (
            "Maximum drawdown % of initial capital (intrabar)",
            Percent,
            p.intrabar_max_drawdown_initial_capital_fraction,
        ),
        ("Maximum run-up (close-to-close)", Money, p.max_run_up),
        (
            "Maximum run-up % (close-to-close)",
            Percent,
            p.max_run_up_fraction,
        ),
        ("Average run-up (close-to-close)", Money, p.average_run_up),
        (
            "Average run-up duration in days (close-to-close)",
            Days,
            p.average_run_up_days,
        ),
        ("Maximum run-up (intrabar)", Money, p.intrabar_max_run_up),
        (
            "Maximum run-up % (intrabar)",
            Percent,
            p.intrabar_max_run_up_fraction,
        ),
        (
            "Return over maximum drawdown",
            Ratio,
            p.return_over_max_drawdown,
        ),
        ("Compound annual growth", Percent, p.compound_annual_growth),
        ("Sharpe ratio", Ratio, p.sharpe_ratio),
        ("Sortino ratio", Ratio, p.sortino_ratio),
        ("Buy-and-hold return", Percent, c.buy_and_hold_return),
        ("Buy-and-hold profit", Money, c.buy_and_hold_profit),
        (
            "Outperformance of buy-and-hold",
            Money,
            c.buy_and_hold_outperformance,
        ),
        ("Margin calls", Count, p.margin_calls.map(|n| n as f64)),
        ("Liquidated quantity", Quantity, p.liquidated_quantity),
        ("Largest liquidation", Quantity, p.largest_liquidation),
        ("Average margin used", Money, p.average_margin),
        ("Largest margin used", Money, p.largest_margin),
        ("Account size required", Money, p.account_size_required),
        ("Return on account size", Percent, p.account_size_return),
        ("Margin efficiency", Ratio, p.margin_efficiency),
    ]
}

impl StrategyReport {
    /// Labelled entry and exit markers for the host's signals service. Invalid trades are skipped;
    /// open trades have only an entry. Directions describe the fill side, including short exits.
    /// Ids remain stable for this report's original trade indices. When replacing a run, remove
    /// its old signals before publishing the replacement, including when it has no trades.
    pub fn signals(&self) -> Vec<super::Signal> {
        use super::{Direction, Signal, SignalId, SignalShape};
        let mut signals = Vec::new();
        for (index, trade) in self
            .trades
            .iter()
            .enumerate()
            .filter(|(_, trade)| trade.is_valid())
        {
            let entry = match trade.side {
                OrderSide::Buy => Direction::Up,
                OrderSide::Sell => Direction::Down,
            };
            let exit = match trade.side {
                OrderSide::Buy => Direction::Down,
                OrderSide::Sell => Direction::Up,
            };
            for (fill, time, price, direction) in
                std::iter::once(("Entry", trade.entry_time, trade.entry_price, entry)).chain(
                    trade
                        .exit
                        .as_ref()
                        .map(|value| ("Exit", value.time, value.price, exit)),
                )
            {
                let label = format!("{fill} {}", index + 1);
                signals.push(Signal {
                    id: SignalId(format!(
                        "strategy:{}:{}:{}:{index}:{fill}",
                        self.symbol, self.timeframe, self.name
                    )),
                    shape: SignalShape::Marker {
                        time,
                        price: Some(price),
                        direction,
                    },
                    category: "Strategy trades".into(),
                    label: Some(label.clone()),
                    tooltip: Some(format!(
                        "{} · {label} · {} units @ {}{}",
                        self.name,
                        Self::quantity_text(trade.quantity),
                        self.price_text(price),
                        trade
                            .label
                            .as_ref()
                            .map_or_else(String::new, |label| format!(" · {label}"))
                    )),
                });
            }
        }
        signals
    }

    /// `price` as the list of trades and the trade markers write it: with at least
    /// [`price_decimals`](Self::price_decimals), up to 12, and more where the price has them.
    pub(crate) fn price_text(&self, price: f64) -> String {
        let least = self
            .price_decimals
            .map_or(0, |decimals| usize::from(decimals.min(12)));
        fill_text(price, least, usize::MAX)
    }

    /// `quantity` with its own decimals, up to eight, as the account tables write quantities.
    pub(crate) fn quantity_text(quantity: f64) -> String {
        fill_text(quantity, 0, 8)
    }

    /// A report with no trades, equity, equity ranges, margin data, properties, currency or price
    /// decimals, a risk-free rate of zero and figures available.
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
            price_decimals: None,
            trades: Vec::new(),
            equity: Vec::new(),
            margin: Vec::new(),
            peak_margin: Vec::new(),
            margin_calls: Vec::new(),
            equity_ranges: Vec::new(),
            unavailable: None,
        }
    }

    /// Computes the metrics of the run. Sorts trades for streaks and the largest position; call once per report,
    /// not once per frame.
    pub fn performance(&self) -> StrategyPerformance {
        let valid: Vec<&StrategyTrade> = self.trades.iter().filter(|t| t.is_valid()).collect();
        let side = |side: OrderSide| -> Vec<&StrategyTrade> {
            valid.iter().copied().filter(|t| t.side == side).collect()
        };
        let capital = self.initial_capital;
        let mut p = StrategyPerformance {
            all: trade_stats(&valid, capital),
            long: trade_stats(&side(OrderSide::Buy), capital),
            short: trade_stats(&side(OrderSide::Sell), capital),
            skipped_trades: self.trades.len() - valid.len(),
            ..Default::default()
        };

        if !self.margin.is_empty() {
            let calls: Vec<_> = self
                .margin_calls
                .iter()
                .map(|&(_, quantity)| quantity)
                .filter(|q| q.is_finite() && *q > 0.0)
                .collect();
            p.margin_calls = Some(calls.len());
            p.liquidated_quantity =
                finite(calls.iter().fold(0.0, |total, quantity| total + quantity));
            p.largest_liquidation = calls.iter().copied().reduce(f64::max);
            let margins: Vec<_> = self
                .margin
                .iter()
                .map(|&(_, m)| m)
                .filter(|m| m.is_finite())
                .collect();
            p.average_margin = (!margins.is_empty())
                .then(|| margins.iter().sum::<f64>() / margins.len() as f64)
                .and_then(finite);
            p.largest_margin = self
                .peak_margin
                .iter()
                .map(|&(_, m)| m)
                .filter(|m| m.is_finite())
                .reduce(f64::max);
            p.margin_efficiency = p
                .average_margin
                .filter(|m| *m > 0.0)
                .and_then(|m| finite(p.all.net_profit / m));
        }

        let curve = self.equity_curve();
        p.skipped_equity_points = self.equity.len() - curve.len();
        p.open_profit = if p.all.open_trades == 0 {
            Some(0.0)
        } else {
            curve
                .last()
                .and_then(|&(_, e)| finite(e - capital - p.all.net_profit))
        };
        let (Some(&(start, first)), Some(&(end, last))) = (curve.first(), curve.last()) else {
            return p;
        };
        let largest = |curve: &[(DateTime<Utc>, CurveExcursion)],
                       value: fn(&CurveExcursion) -> f64| {
            finite(curve.iter().map(|(_, x)| value(x)).fold(f64::NAN, f64::max))
        };
        let of_capital = |amount: Option<f64>| {
            amount
                .filter(|_| capital.is_finite() && capital > 0.0)
                .and_then(|a| finite(a / capital))
        };
        let close = self.excursion_curve(false).unwrap_or_default();
        p.max_drawdown = largest(&close, |x| x.drawdown);
        p.max_drawdown_fraction = largest(&close, |x| x.drawdown_fraction);
        p.max_run_up = largest(&close, |x| x.run_up);
        p.max_run_up_fraction = of_capital(p.max_run_up);
        if let Some(intrabar) = self.excursion_curve(true) {
            p.intrabar_max_drawdown = largest(&intrabar, |x| x.drawdown);
            p.intrabar_max_drawdown_fraction = largest(&intrabar, |x| x.drawdown_fraction);
            p.intrabar_max_drawdown_initial_capital_fraction = of_capital(p.intrabar_max_drawdown);
            p.intrabar_max_run_up = largest(&intrabar, |x| x.run_up);
            p.intrabar_max_run_up_fraction = of_capital(p.intrabar_max_run_up);
        }

        p.return_over_max_drawdown = p
            .max_drawdown
            .filter(|&d| d > 0.0)
            .map(|d| p.all.net_profit / d)
            .and_then(finite);
        p.account_size_required = p
            .largest_margin
            .zip(p.intrabar_max_drawdown.or(p.max_drawdown))
            .and_then(|(margin, drawdown)| finite(margin + drawdown));
        p.account_size_return = p
            .account_size_required
            .filter(|size| *size > 0.0)
            .and_then(|size| finite(p.all.net_profit / size));
        let phases = curve_phases(&curve);
        let average =
            |total: f64, count: usize| (count > 0).then(|| total / count as f64).and_then(finite);
        p.average_drawdown = average(phases.drawdown, phases.drawdowns);
        p.average_drawdown_days = average(phases.drawdown_seconds / 86_400.0, phases.drawdowns);
        p.average_run_up = average(phases.run_up, phases.run_ups);
        p.average_run_up_days = average(phases.run_up_seconds / 86_400.0, phases.run_ups);

        let steps = curve.len() - 1;
        if end - start < TimeDelta::weeks(13) {
            return p;
        }
        let years = (end - start).num_milliseconds() as f64 / YEAR_MS;
        p.compound_annual_growth = (first > 0.0 && last >= 0.0)
            .then(|| (last / first).powf(1.0 / years) - 1.0)
            .and_then(finite);
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

    /// Limits the run to `range`, including its start and excluding its end. Keeps valid trades
    /// closed in it and those still open at its end, removing exits at or after the end.
    ///
    /// Keeps only usable equity points inside the range, so drawdown and ratios start at its
    /// first point, retaining each point's equity range at its original index. Mismatched range
    /// lengths leave the result's ranges empty. When the range starts after the curve's first
    /// point, initial capital becomes the first included equity value, or NaN without one,
    /// making the net return unavailable.
    /// Otherwise the run's capital is kept. Invalid inputs are omitted, so skipped counts are
    /// zero; consult the original report for them. An empty or reversed range keeps no trades
    /// or points. Margin marks, peaks and calls are limited to the range too. All other fields
    /// are copied. Combine with [`Self::performance`],
    /// [`Self::comparison`] or [`Self::analysis`] to compute figures for the period.
    pub fn within(&self, range: Range<DateTime<Utc>>) -> StrategyReport {
        // Exhaustive so each new report field must decide how it follows a range.
        let Self {
            name,
            symbol,
            timeframe,
            initial_capital,
            currency,
            risk_free_rate,
            properties,
            price_decimals,
            unavailable,
            trades,
            equity: _,
            margin,
            peak_margin,
            margin_calls,
            equity_ranges,
        } = self;
        let curve = self.equity_curve();
        let equity: Vec<_> = curve
            .iter()
            .copied()
            .filter(|(t, _)| range.contains(t))
            .collect();
        let equity_ranges = if equity_ranges.len() == self.equity.len() && !curve.is_empty() {
            self.equity
                .iter()
                .zip(equity_ranges)
                .filter(|((time, value), _)| value.is_finite() && range.contains(time))
                .map(|(_, range)| *range)
                .collect()
        } else {
            Vec::new()
        };
        let initial_capital = if curve.first().is_some_and(|&(t, _)| t < range.start) {
            equity.first().map_or(f64::NAN, |&(_, e)| e)
        } else {
            *initial_capital
        };
        let trades = trades
            .iter()
            .filter(|t| t.is_valid() && t.in_range(&range))
            .map(|t| {
                let mut t = t.clone();
                if t.exit.as_ref().is_some_and(|e| e.time >= range.end) {
                    t.exit = None;
                }
                t
            })
            .collect();
        Self {
            name: name.clone(),
            symbol: symbol.clone(),
            timeframe: *timeframe,
            initial_capital,
            currency: currency.clone(),
            risk_free_rate: *risk_free_rate,
            properties: properties.clone(),
            price_decimals: *price_decimals,
            unavailable: unavailable.clone(),
            trades,
            equity,
            margin: margin
                .iter()
                .copied()
                .filter(|(t, _)| range.contains(t))
                .collect(),
            peak_margin: peak_margin
                .iter()
                .copied()
                .filter(|(t, _)| range.contains(t))
                .collect(),
            margin_calls: margin_calls
                .iter()
                .copied()
                .filter(|(t, _)| range.contains(t))
                .collect(),
            equity_ranges,
        }
    }

    /// Groups closed valid trades by calendar month/year, result and UTC entry time. `bars` are
    /// the run's source bars as for [`Self::comparison`], used for each period's passive return;
    /// pass an empty slice without them. Combine with [`Self::within`] to analyse a range.
    /// Sorts trades and bins results; call once per report or range, not once per frame.
    pub fn analysis(&self, bars: &[Bar]) -> StrategyAnalysis {
        let closed: Vec<_> = self
            .trades
            .iter()
            .filter(|t| t.is_valid())
            .filter_map(|t| t.exit.as_ref().map(|e| (t, e)))
            .collect();
        let mut result = StrategyAnalysis {
            months: Vec::new(),
            years: Vec::new(),
            result_bin_width: 0.001,
            result_bins: Vec::new(),
            win_runs: Vec::new(),
            loss_runs: Vec::new(),
            by_hour: Default::default(),
            by_weekday: Default::default(),
            by_month: Default::default(),
        };
        for &(t, e) in &closed {
            for group in [
                &mut result.by_hour[t.entry_time.hour() as usize],
                &mut result.by_weekday[t.entry_time.weekday().num_days_from_monday() as usize],
                &mut result.by_month[t.entry_time.month0() as usize],
            ] {
                group.add(e.profit);
            }
        }
        let mut exits: Vec<_> = closed.iter().map(|&(_, e)| e).collect();
        exits.sort_by_key(|e| e.time);
        let (mut wins, mut losses) = (0, 0);
        for e in exits {
            if e.profit > 0.0 {
                wins += 1;
            } else {
                finish_run(&mut result.win_runs, &mut wins);
            }
            if e.profit < 0.0 {
                losses += 1;
            } else {
                finish_run(&mut result.loss_runs, &mut losses);
            }
        }
        finish_run(&mut result.win_runs, &mut wins);
        finish_run(&mut result.loss_runs, &mut losses);

        let curve = self.equity_curve();
        let source: Vec<_> = if bars.windows(2).any(|w| w[1].time <= w[0].time) {
            Vec::new()
        } else {
            bars.iter().filter(|b| valid_bar(b)).collect()
        };
        let times = || {
            curve
                .iter()
                .map(|p| p.0)
                .chain(closed.iter().map(|(_, e)| e.time))
        };
        let months: BTreeSet<_> = times().map(|t| (t.year(), t.month())).collect();
        let years: BTreeSet<_> = times().map(|t| t.year()).collect();
        let period = |year, month, length| {
            let start = Utc
                .with_ymd_and_hms(year, month, 1, 0, 0, 0)
                .single()
                .expect("the first of a month in range");
            let end = start.checked_add_months(Months::new(length));
            let a = curve.partition_point(|p| p.0 < start);
            let b = end.map_or(curve.len(), |end| curve.partition_point(|p| p.0 < end));
            let equity_return = (a < b)
                .then(|| {
                    let base = if a > 0 { curve[a - 1].1 } else { curve[a].1 };
                    (base > 0.0).then(|| curve[b - 1].1 / base - 1.0)
                })
                .flatten()
                .filter(|r| r.is_finite());
            let a = source.partition_point(|b| b.time < start);
            let b = end.map_or(source.len(), |end| source.partition_point(|b| b.time < end));
            let buy_and_hold_return = (a < b)
                .then(|| {
                    let base = if a > 0 {
                        source[a - 1].close
                    } else {
                        source[a].open
                    };
                    (base > 0.0).then(|| source[b - 1].close / base - 1.0)
                })
                .flatten()
                .filter(|r| r.is_finite());
            let mut trades = StrategyTradeGroup::default();
            for &(_, e) in &closed {
                if e.time >= start && end.is_none_or(|end| e.time < end) {
                    trades.add(e.profit);
                }
            }
            StrategyPeriod {
                start,
                equity_return,
                buy_and_hold_return,
                trades,
            }
        };
        result.months = months.into_iter().map(|(y, m)| period(y, m, 1)).collect();
        result.years = years.into_iter().map(|y| period(y, 1, 12)).collect();

        let returns: Vec<_> = closed
            .iter()
            .filter_map(|(t, e)| {
                let value = e.profit / (t.quantity * t.entry_price.abs());
                value.is_finite().then_some((value, e.profit))
            })
            .collect();
        if !returns.is_empty() {
            for i in 0.. {
                let mantissa = [1.0, 2.0, 5.0][i as usize % 3];
                let exponent = i / 3 - 3;
                let width = if exponent < 0 {
                    mantissa / 10f64.powi(-exponent)
                } else {
                    mantissa * 10f64.powi(exponent)
                };
                let lo = returns
                    .iter()
                    .map(|&(r, p)| result_bin(r, p, width))
                    .fold(f64::INFINITY, f64::min);
                let hi = returns
                    .iter()
                    .map(|&(r, p)| result_bin(r, p, width))
                    .fold(f64::NEG_INFINITY, f64::max);
                if hi - lo + 1.0 > 20.0 || lo.abs().max(hi.abs()) > 2f64.powi(52) {
                    continue;
                }
                result.result_bin_width = width;
                result.result_bins = (0..(hi - lo + 1.0) as usize)
                    .map(|k| StrategyResultBin {
                        lower: (lo + k as f64) * width,
                        winners: 0,
                        losers: 0,
                        breakeven: 0,
                    })
                    .collect();
                for &(r, p) in &returns {
                    let bin = &mut result.result_bins[(result_bin(r, p, width) - lo) as usize];
                    if p > 0.0 {
                        bin.winners += 1;
                    } else if p < 0.0 {
                        bin.losers += 1;
                    } else {
                        bin.breakeven += 1;
                    }
                }
                break;
            }
        }
        result
    }

    /// Finite equity points in strictly increasing time order, using the same validation as
    /// [`Self::performance`]. Empty if the finite points are out of order.
    pub fn equity_curve(&self) -> Vec<(DateTime<Utc>, f64)> {
        usable_curve(&self.equity)
    }

    /// [`Self::margin`] validated as [`Self::equity_curve`] validates equity, for the panel.
    #[cfg(feature = "ui")]
    pub(crate) fn margin_curve(&self) -> Vec<(DateTime<Utc>, f64)> {
        usable_curve(&self.margin)
    }

    /// Composes per-point drawdown and run-up from closes or usable ranges, for metrics and charts.
    pub(crate) fn excursion_curve(
        &self,
        intrabar: bool,
    ) -> Option<Vec<(DateTime<Utc>, CurveExcursion)>> {
        let curve = self.equity_curve();
        let ranges: Vec<_> = if !intrabar {
            curve
                .iter()
                .map(|&(t, e)| (t, EquityRange::new(e, e, 0.0, 0.0)))
                .collect()
        } else if curve.is_empty() || self.equity_ranges.len() != self.equity.len() {
            return None;
        } else {
            let mut ranges = Vec::with_capacity(curve.len());
            for (&(time, value), range) in self.equity.iter().zip(&self.equity_ranges) {
                if !value.is_finite() {
                    continue;
                }
                if ![range.low, range.high, range.fall, range.rise]
                    .iter()
                    .all(|v| v.is_finite())
                    || range.low > value
                    || value > range.high
                    || range.fall < 0.0
                    || range.rise < 0.0
                {
                    return None;
                }
                ranges.push((time, *range));
            }
            ranges
        };
        let (_, first) = *ranges.first()?;
        let positive = first.high > 0.0;
        let (mut peak, mut trough) = (first.high, first.low);
        let mut result = Vec::with_capacity(ranges.len());
        for (index, &(time, r)) in ranges.iter().enumerate() {
            let (drawdown, fraction, run_up) = if index == 0 {
                (r.fall, r.fall / r.high, r.rise)
            } else {
                (
                    (peak - r.low).max(r.fall),
                    ((peak - r.low) / peak).max(r.fall / peak.max(r.high)),
                    (r.high - trough).max(r.rise),
                )
            };
            peak = peak.max(r.high);
            trough = trough.min(r.low);
            result.push((
                time,
                CurveExcursion {
                    drawdown,
                    drawdown_fraction: if positive { fraction } else { f64::NAN },
                    run_up,
                },
            ));
        }
        Some(result)
    }

    /// Computes buy-and-hold and excursions from the complete source bars of this run's symbol and
    /// interval, oldest first. The host supplies exactly the run's bars, including every held bar;
    /// this method neither fetches data nor infers missing sessions. Bar times denote opens.
    ///
    /// Non-finite or inverted OHLC bars are skipped and counted. Unordered or duplicate timestamps
    /// make the whole source unusable. Volume is unused. Buy-and-hold uses the first valid open and
    /// last valid close, with fractional units, no fees or reinvested dividends. A non-finite
    /// calculation is unavailable. Excursions require at least one held source bar; invalid held
    /// bars make that trade's excursion unavailable rather than understating risk. Holding times
    /// count source bars as [`StrategyHolding`] defines. Buy-and-hold profit needs positive finite
    /// capital; its outperformance compares it with the net profit of closed trades.
    pub fn comparison(&self, bars: &[Bar]) -> StrategyComparison {
        let mut result = StrategyComparison {
            excursions: vec![None; self.trades.len()],
            ..Default::default()
        };
        if bars.windows(2).any(|w| w[1].time <= w[0].time) {
            result.skipped_bars = bars.len();
            return result;
        }
        let source: Vec<_> = bars.iter().filter(|b| valid_bar(b)).collect();
        result.skipped_bars = bars.len() - source.len();
        let net_profit = self
            .trades
            .iter()
            .filter(|t| t.is_valid())
            .filter_map(|t| t.exit.as_ref())
            .fold(0.0, |sum, exit| sum + exit.profit);
        if let (Some(first), Some(last)) = (source.first(), source.last()) {
            let fraction = last.close / first.open - 1.0;
            if first.open > 0.0 && fraction.is_finite() {
                result.buy_and_hold_return = Some(fraction);
                if self.initial_capital.is_finite() && self.initial_capital > 0.0 {
                    result.buy_and_hold_profit = finite(self.initial_capital * fraction);
                    result.buy_and_hold_outperformance = result
                        .buy_and_hold_profit
                        .map(|profit| net_profit - profit)
                        .and_then(finite);
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
        let bar_of = |time: DateTime<Utc>| bars.partition_point(|b| b.time <= time).checked_sub(1);
        let holding = |side: Option<OrderSide>| {
            // Span totals can exceed an index's range even when their average fits.
            let mut sums = [(0_u128, 0_usize); 3];
            for trade in self
                .trades
                .iter()
                .filter(|t| t.is_valid() && side.is_none_or(|s| t.side == s))
            {
                let Some(exit) = &trade.exit else { continue };
                let (Some(entry_bar), Some(exit_bar)) =
                    (bar_of(trade.entry_time), bar_of(exit.time))
                else {
                    continue;
                };
                for (counted, sum) in [true, exit.profit > 0.0, exit.profit < 0.0]
                    .into_iter()
                    .zip(&mut sums)
                {
                    if counted {
                        sum.0 += (exit_bar - entry_bar) as u128;
                        sum.1 += 1;
                    }
                }
            }
            let average =
                |(bars, trades): (u128, usize)| (trades > 0).then(|| bars as f64 / trades as f64);
            StrategyHolding {
                average_bars: average(sums[0]),
                average_winning_bars: average(sums[1]),
                average_losing_bars: average(sums[2]),
            }
        };
        result.all = holding(None);
        result.long = holding(Some(OrderSide::Buy));
        result.short = holding(Some(OrderSide::Sell));
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
            if held.is_empty() || held.iter().any(|b| !valid_bar(b)) {
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
            let rate = trade.exchange_rate;
            let (adverse, favourable) = (
                adverse * trade.quantity * rate,
                favourable * trade.quantity * rate,
            );
            if rate > 0.0 && adverse.is_finite() && favourable.is_finite() {
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
    /// `entry_time`, `entry_price`, `exit_time`, `exit_price`, `profit`, `fees`, `label`, `mae`,
    /// `mfe` and `cumulative_profit`. Times are RFC 3339 in UTC (`2024-01-02T00:00:00Z`), numbers
    /// use the shortest form that reads back to the same value, and an open trade leaves the three
    /// exit columns empty. `mae` and `mfe` stay empty here; [`Self::trades_csv_with_excursions`]
    /// fills them. `cumulative_profit` is the sum of `profit` over the rows up to and including
    /// its own; an open trade adds nothing and leaves it empty, as does a sum that is not finite.
    /// Text cells starting with `=`, `+`, `-`, `@`, tab or carriage return gain a leading apostrophe
    /// so spreadsheets treat them as text. Numeric cells, including negative numbers, are unchanged.
    #[cfg(feature = "csv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "csv")))]
    pub fn trades_csv(&self) -> String {
        self.trades_csv_with_excursions(&StrategyComparison::default())
    }

    /// [`Self::trades_csv`] with each trade's maximum adverse and favourable excursion in `mae`
    /// and `mfe`, as the report panel's list of trades shows them. `comparison` is this report's
    /// [`Self::comparison`] over the bars of the run; a trade without an excursion there leaves
    /// both cells empty.
    #[cfg(feature = "csv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "csv")))]
    pub fn trades_csv_with_excursions(&self, comparison: &StrategyComparison) -> String {
        const IN_MEMORY: &str = "writing CSV to memory cannot fail";
        let mut trades: Vec<_> = self
            .trades
            .iter()
            .enumerate()
            .filter(|(_, trade)| trade.is_valid())
            .collect();
        trades.sort_by_key(|(_, trade)| trade.entry_time);
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
            "mae",
            "mfe",
            "cumulative_profit",
        ])
        .expect(IN_MEMORY);
        let mut total = 0.0;
        for (i, (index, trade)) in trades.into_iter().enumerate() {
            let cumulative = trade
                .exit
                .as_ref()
                .map(|exit| {
                    total += exit.profit;
                    total
                })
                .filter(|total| total.is_finite())
                .map_or_else(String::new, |total| total.to_string());
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
            let (mae, mfe) = comparison
                .excursions
                .get(index)
                .and_then(Option::as_ref)
                .map_or_else(Default::default, |excursion| {
                    (
                        excursion.adverse.to_string(),
                        excursion.favourable.to_string(),
                    )
                });
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
                mae,
                mfe,
                cumulative,
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
    /// `unit`, `currency` and `side`. The unit is `money` (in `currency`, empty when the report
    /// has none), `percent` (100 is 100%), `ratio`, `count`, `quantity` (instrument units, as
    /// trade quantities), `bars` (source bars), `days` (calendar days of 24 hours) or `text`.
    /// Each trade figure has three rows with `side` `all`, `long` and `short` in that order;
    /// whole-run figures and properties leave `side` empty. Percent values round to 15 significant
    /// decimal digits to suppress binary multiplication noise; if rounding would overflow `f64`,
    /// the unrounded finite value is kept. Numbers use the shortest form that reads back to the
    /// resulting value, with a dot as the decimal separator and no thousands separator. Every
    /// numeric zero is `0`, and an unavailable value is an empty cell, so numeric columns stay
    /// numeric in a spreadsheet.
    ///
    /// Text cells starting with `=`, `+`, `-`, `@`, tab or carriage return gain a leading apostrophe
    /// so spreadsheets treat them as text. This also applies to property names and currencies;
    /// numeric cells, including negative numbers, are unchanged by this protection.
    #[cfg(feature = "csv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "csv")))]
    pub fn summary_csv(
        &self,
        performance: &StrategyPerformance,
        comparison: &StrategyComparison,
    ) -> String {
        const IN_MEMORY: &str = "writing CSV to memory cannot fail";
        let currency = csv_text(self.currency.as_deref().unwrap_or_default());
        let mut csv = csv::Writer::from_writer(Vec::new());
        csv.write_record(["section", "name", "value", "unit", "currency", "side"])
            .expect(IN_MEMORY);
        let mut row = |section: &str, name: &str, value: String, unit: &str, side: &str| {
            let name = csv_text(name);
            let value = if unit == "text" {
                csv_text(&value)
            } else {
                value
            };
            let currency = if unit == "money" { &currency } else { "" };
            csv.write_record([section, &name, &value, unit, currency, side])
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
        for (name, unit, values) in trade_figures(p, comparison) {
            for (side, value) in ["all", "long", "short"].into_iter().zip(values) {
                row(
                    "summary",
                    name,
                    number(value, unit == SummaryUnit::Percent),
                    unit.name(),
                    side,
                );
            }
        }
        for (name, unit, value) in run_figures(p, comparison) {
            row(
                "summary",
                name,
                number(value, unit == SummaryUnit::Percent),
                unit.name(),
                "",
            );
        }
        for (name, count) in [
            ("Skipped trades", p.skipped_trades),
            ("Skipped equity points", p.skipped_equity_points),
            ("Skipped source bars", comparison.skipped_bars),
        ] {
            row("summary", name, count.to_string(), "count", "");
        }
        row("properties", "Name", self.name.clone(), "text", "");
        row("properties", "Symbol", self.symbol.clone(), "text", "");
        row(
            "properties",
            "Interval",
            self.timeframe.to_string(),
            "text",
            "",
        );
        row(
            "properties",
            "Initial capital",
            number(Some(self.initial_capital), false),
            "money",
            "",
        );
        row(
            "properties",
            "Currency",
            self.currency.clone().unwrap_or_default(),
            "text",
            "",
        );
        row(
            "properties",
            "Risk-free rate",
            number(Some(self.risk_free_rate), true),
            "percent",
            "",
        );
        for (name, value) in &self.properties {
            row("properties", name, value.clone(), "text", "");
        }
        String::from_utf8(csv.into_inner().expect(IN_MEMORY)).expect("every field is a String")
    }
}

/// `value` with `least` decimals or its own, whichever is more, up to `most`. Its own decimals
/// are those of its shortest form at 15 significant digits, which drops binary rounding digits.
fn fill_text(value: f64, least: usize, most: usize) -> String {
    let rounded = format!("{value:.14e}").parse::<f64>().unwrap_or(value);
    let own = rounded
        .to_string()
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    // Adding zero turns a negative zero positive.
    format!("{:.*}", own.max(least).min(most), rounded + 0.0)
}

fn valid_bar(b: &Bar) -> bool {
    [b.open, b.high, b.low, b.close]
        .iter()
        .all(|p| p.is_finite())
        && b.low <= b.open.min(b.close)
        && b.high >= b.open.max(b.close)
}

impl StrategyTradeGroup {
    fn add(&mut self, profit: f64) {
        self.closed_trades += 1;
        self.winning_trades += usize::from(profit > 0.0);
        self.net_profit += profit;
        self.win_rate = Some(self.winning_trades as f64 / self.closed_trades as f64);
    }
}

fn finish_run(runs: &mut Vec<usize>, length: &mut usize) {
    if *length > 0 {
        runs.resize(runs.len().max(*length), 0);
        runs[*length - 1] += 1;
        *length = 0;
    }
}

fn result_bin(result: f64, profit: f64, width: f64) -> f64 {
    let q = result / width;
    let edge = q.round();
    let i = if (q - edge).abs() <= 1e-9 * edge.abs().max(1.0) {
        edge
    } else {
        q.floor()
    };
    if profit < 0.0 { i.min(-1.0) } else { i }
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
            exchange_rate: 1.0,
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

    /// Closed in the range, or entered before its end and still open there.
    pub(crate) fn in_range(&self, range: &Range<DateTime<Utc>>) -> bool {
        !range.is_empty()
            && match &self.exit {
                Some(exit) if exit.time < range.end => exit.time >= range.start,
                _ => self.entry_time < range.end,
            }
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
    use chrono::Weekday;

    #[test]
    fn report_signals_preserve_valid_fill_times_prices_indices_and_sides() {
        use crate::model::{Direction, SignalShape};
        let start = DateTime::UNIX_EPOCH + chrono::TimeDelta::nanoseconds(123);
        let mut report = StrategyReport::new("Fills", "TEST", Timeframe::Min1, 1000.0);
        for (side, exit) in [
            (OrderSide::Buy, true),
            (OrderSide::Sell, true),
            (OrderSide::Sell, false),
        ] {
            let mut trade = StrategyTrade::new(side, 2.0, start, -0.00001234);
            trade.label = Some("Rule / close".into());
            if exit {
                trade.exit = Some(StrategyExit::new(
                    start + chrono::TimeDelta::milliseconds(250),
                    0.25,
                    0.0,
                ));
            }
            report.trades.push(trade);
        }
        let mut invalid = report.trades[0].clone();
        invalid.quantity = 0.0;
        report.trades.insert(1, invalid);
        let signals = report.signals();
        assert_eq!(signals.len(), 5);
        for (index, (signal, direction)) in signals
            .iter()
            .zip([
                Direction::Up,
                Direction::Down,
                Direction::Down,
                Direction::Up,
                Direction::Down,
            ])
            .enumerate()
        {
            let entry = index == 0 || index == 2 || index == 4;
            assert_eq!(
                signal.shape,
                SignalShape::Marker {
                    time: if entry {
                        start
                    } else {
                        start + chrono::TimeDelta::milliseconds(250)
                    },
                    price: Some(if entry { -0.00001234 } else { 0.25 }),
                    direction,
                }
            );
            assert!(signal.tooltip.as_ref().unwrap().contains("Rule / close"));
        }
        assert_eq!(signals.last().unwrap().label.as_deref(), Some("Entry 4"));
        assert_eq!(signals, report.signals(), "stable marker ids");
    }

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

    fn date(year: i32, month: u32, day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap()
    }

    /// Fixture D crosses a year boundary; input order differs from exit order.
    fn fixture_d() -> StrategyReport {
        let mut r = report(1000.0);
        let mut trades: Vec<_> = [
            (
                date(2023, 11, 30, 14),
                Some(date(2023, 12, 14, 10)),
                50.0,
                2.0,
            ),
            (
                date(2023, 12, 31, 22),
                Some(date(2024, 1, 1, 3)),
                150.0,
                0.0,
            ),
            (
                date(2024, 1, 16, 9),
                Some(date(2024, 1, 30, 15)),
                -140.0,
                3.0,
            ),
            (date(2024, 2, 1, 9), Some(date(2024, 2, 9, 12)), 60.0, 0.0),
            (date(2024, 2, 5, 10), None, 0.0, 0.0),
            (date(2024, 1, 2, 9), Some(date(2024, 1, 3, 9)), 0.0, 0.0),
        ]
        .into_iter()
        .map(|(entry, exit, profit, fees)| {
            let mut t = StrategyTrade::new(OrderSide::Buy, 10.0, entry, 100.0);
            t.exit = exit.map(|time| StrategyExit::new(time, 100.0 + profit / 10.0, profit));
            t.fees = fees;
            t
        })
        .collect();
        let mut invalid = trades[0].clone();
        invalid.quantity = 0.0;
        trades.push(invalid);
        r.trades = [3, 6, 0, 5, 2, 4, 1].map(|i| trades[i].clone()).to_vec();
        r.equity = [
            (date(2023, 11, 30, 0), 1000.0),
            (date(2023, 12, 15, 0), 1300.0),
            (date(2023, 12, 31, 0), 1050.0),
            (date(2024, 1, 15, 0), 1200.0),
            (date(2024, 1, 31, 0), 960.0),
            (date(2024, 2, 10, 0), 1080.0),
        ]
        .to_vec();
        r.equity_ranges = vec![
            EquityRange::new(1000.0, 1000.0, 0.0, 0.0),
            EquityRange::new(900.0, 1400.0, 100.0, 500.0),
            EquityRange::new(1000.0, 1350.0, 350.0, 50.0),
            EquityRange::new(1050.0, 1250.0, 150.0, 200.0),
            EquityRange::new(900.0, 1220.0, 320.0, 60.0),
            EquityRange::new(950.0, 1150.0, 70.0, 200.0),
        ];
        r
    }

    fn side(fraction: Option<f64>) -> StrategyTradeStats {
        StrategyTradeStats {
            net_profit_fraction: fraction,
            ..Default::default()
        }
    }

    fn expected_a() -> StrategyPerformance {
        let all = StrategyTradeStats {
            closed_trades: 5,
            open_trades: 1,
            winning_trades: 2,
            losing_trades: 2,
            even_trades: 1,
            net_profit: 44.5,
            net_profit_fraction: Some(0.00445),
            gross_profit: 76.5,
            gross_loss: 32.0,
            total_fees: 5.0,
            profit_factor: Some(2.390625),
            win_rate: Some(0.4),
            average_trade: Some(8.9),
            average_winning_trade: Some(38.25),
            average_losing_trade: Some(16.0),
            win_loss_ratio: Some(2.390625),
            largest_winning_trade: Some(48.0),
            largest_winning_fraction: Some(48.0 / 76.5),
            largest_losing_trade: Some(-21.0),
            largest_losing_fraction: Some(0.65625),
            net_profit_to_largest_loss: Some(44.5 / 21.0),
            max_consecutive_wins: 1,
            max_consecutive_losses: 1,
            largest_position: Some(7.0),
        };
        StrategyPerformance {
            long: StrategyTradeStats {
                open_trades: 0,
                largest_position: Some(5.0),
                ..all.clone()
            },
            short: StrategyTradeStats {
                open_trades: 1,
                largest_position: Some(2.0),
                ..side(Some(0.0))
            },
            all,
            open_profit: Some(45.5),
            max_run_up: Some(130.0),
            max_run_up_fraction: Some(0.013),
            max_drawdown: Some(90.0),
            max_drawdown_fraction: Some(0.008928571428571428),
            average_drawdown: Some(47.5),
            average_drawdown_days: Some(1.0 / 12.0),
            average_run_up: Some(70.0),
            average_run_up_days: Some(0.052083333333333336),
            return_over_max_drawdown: Some(44.5 / 90.0),
            ..Default::default()
        }
    }

    /// 100 units entered at 100 dip to 70, recover to 100, then close at 90 while held.
    fn fixture_intrabar() -> StrategyReport {
        let mut r = report(10_000.0);
        r.trades.push(StrategyTrade::new(
            OrderSide::Buy,
            100.0,
            t0() + TimeDelta::hours(1),
            100.0,
        ));
        r.equity = curve(&[10_000.0, 10_000.0, 9_000.0], TimeDelta::hours(1));
        r.equity_ranges = vec![
            EquityRange::new(10_000.0, 10_000.0, 0.0, 0.0),
            EquityRange::new(7_000.0, 10_000.0, 3_000.0, 3_000.0),
            EquityRange::new(9_000.0, 10_000.0, 1_000.0, 0.0),
        ];
        r
    }

    fn expected_d() -> StrategyPerformance {
        StrategyPerformance {
            all: StrategyTradeStats {
                open_trades: 1,
                largest_position: Some(100.0),
                ..side(Some(0.0))
            },
            long: StrategyTradeStats {
                open_trades: 1,
                largest_position: Some(100.0),
                ..side(Some(0.0))
            },
            short: side(Some(0.0)),
            return_over_max_drawdown: Some(0.0),
            open_profit: Some(-1000.0),
            max_run_up: Some(0.0),
            max_run_up_fraction: Some(0.0),
            intrabar_max_run_up: Some(3000.0),
            intrabar_max_run_up_fraction: Some(0.3),
            max_drawdown: Some(1000.0),
            max_drawdown_fraction: Some(0.1),
            average_drawdown: Some(1000.0),
            average_drawdown_days: Some(1.0 / 24.0),
            intrabar_max_drawdown: Some(3000.0),
            intrabar_max_drawdown_fraction: Some(0.3),
            intrabar_max_drawdown_initial_capital_fraction: Some(0.3),
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

    fn close(a: f64, b: f64) -> bool {
        a == b || (a - b).abs() <= 1e-12 * a.abs().max(b.abs())
    }

    fn option_close(a: Option<f64>, b: Option<f64>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => close(a, b),
            (a, b) => a.is_none() && b.is_none(),
        }
    }

    fn stats_fields(s: &StrategyTradeStats) -> (Vec<usize>, Vec<Option<f64>>) {
        let StrategyTradeStats {
            closed_trades,
            open_trades,
            winning_trades,
            losing_trades,
            even_trades,
            net_profit,
            net_profit_fraction,
            gross_profit,
            gross_loss,
            total_fees,
            profit_factor,
            win_rate,
            average_trade,
            average_winning_trade,
            average_losing_trade,
            win_loss_ratio,
            largest_winning_trade,
            largest_winning_fraction,
            largest_losing_trade,
            largest_losing_fraction,
            net_profit_to_largest_loss,
            max_consecutive_wins,
            max_consecutive_losses,
            largest_position,
        } = *s;
        (
            vec![
                closed_trades,
                open_trades,
                winning_trades,
                losing_trades,
                even_trades,
                max_consecutive_wins,
                max_consecutive_losses,
            ],
            vec![
                Some(net_profit),
                net_profit_fraction,
                Some(gross_profit),
                Some(gross_loss),
                Some(total_fees),
                profit_factor,
                win_rate,
                average_trade,
                average_winning_trade,
                average_losing_trade,
                win_loss_ratio,
                largest_winning_trade,
                largest_winning_fraction,
                largest_losing_trade,
                largest_losing_fraction,
                net_profit_to_largest_loss,
                largest_position,
            ],
        )
    }

    fn assert_matches(case: &str, actual: &StrategyPerformance, expected: &StrategyPerformance) {
        let fields = |p: &StrategyPerformance| {
            let StrategyPerformance {
                all,
                long,
                short,
                open_profit,
                intrabar_max_drawdown,
                intrabar_max_drawdown_fraction,
                intrabar_max_drawdown_initial_capital_fraction,
                max_run_up,
                max_run_up_fraction,
                intrabar_max_run_up,
                intrabar_max_run_up_fraction,
                max_drawdown,
                max_drawdown_fraction,
                average_drawdown,
                average_drawdown_days,
                average_run_up,
                average_run_up_days,
                return_over_max_drawdown,
                compound_annual_growth,
                sharpe_ratio,
                sortino_ratio,
                periods_per_year,
                skipped_trades,
                skipped_equity_points,
                margin_calls: _,
                liquidated_quantity,
                largest_liquidation,
                average_margin,
                largest_margin,
                account_size_required,
                account_size_return,
                margin_efficiency,
            } = p;
            let mut counts = vec![*skipped_trades, *skipped_equity_points];
            let mut numbers = vec![
                *open_profit,
                *intrabar_max_drawdown,
                *intrabar_max_drawdown_fraction,
                *intrabar_max_drawdown_initial_capital_fraction,
                *max_run_up,
                *max_run_up_fraction,
                *intrabar_max_run_up,
                *intrabar_max_run_up_fraction,
                *max_drawdown,
                *max_drawdown_fraction,
                *average_drawdown,
                *average_drawdown_days,
                *average_run_up,
                *average_run_up_days,
                *return_over_max_drawdown,
                *compound_annual_growth,
                *sharpe_ratio,
                *sortino_ratio,
                *periods_per_year,
                *liquidated_quantity,
                *largest_liquidation,
                *average_margin,
                *largest_margin,
                *account_size_required,
                *account_size_return,
                *margin_efficiency,
            ];
            for stats in [all, long, short] {
                let (c, n) = stats_fields(stats);
                counts.extend(c);
                numbers.extend(n);
            }
            (counts, numbers)
        };
        let (actual_counts, actual_numbers) = fields(actual);
        let (expected_counts, expected_numbers) = fields(expected);
        let same = actual_counts == expected_counts
            && actual.margin_calls == expected.margin_calls
            && actual.liquidated_quantity.map(f64::is_sign_negative)
                == expected.liquidated_quantity.map(f64::is_sign_negative)
            && actual_numbers
                .into_iter()
                .zip(expected_numbers)
                .all(|pair| match pair {
                    (Some(a), Some(b)) => close(a, b),
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

        let mut flat_stretches = report(10_000.0);
        flat_stretches.equity = curve(
            &[100.0, 100.0, 110.0, 110.0, 110.0, 90.0, 90.0, 110.0],
            TimeDelta::days(1),
        );

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
            open_profit: None,
            max_run_up: None,
            max_run_up_fraction: None,
            max_drawdown: None,
            max_drawdown_fraction: None,
            average_drawdown: None,
            average_drawdown_days: None,
            average_run_up: None,
            average_run_up_days: None,
            return_over_max_drawdown: None,
            ..expected_a()
        };
        // Only finite positive calls count: 3 + 1.5 = 4.5 units, largest 3.
        let mut margin = fixture_a();
        margin.margin = curve(&[0.0, 400.0, f64::NAN, 200.0], TimeDelta::hours(1));
        margin.peak_margin = curve(
            &[0.0, 450.0, 600.0, 250.0, f64::INFINITY],
            TimeDelta::hours(1),
        );
        let at = |h| t0() + TimeDelta::hours(h);
        margin.margin_calls = vec![
            (at(2), 3.0),
            (at(3), 1.5),
            (at(3), f64::NAN),
            (at(3), f64::INFINITY),
            (at(3), 0.0),
            (at(3), -1.0),
        ];
        let saved_margin = margin.clone();
        let mut no_margin_marks = margin.clone();
        no_margin_marks.margin.clear();
        let mut unfunded = fixture_a();
        unfunded.margin = curve(&[0.0; 4], TimeDelta::hours(1));
        unfunded.peak_margin = unfunded.margin.clone();
        let mut margin_without_equity = margin.clone();
        margin_without_equity.equity.clear();
        let margin_metrics = StrategyPerformance {
            margin_calls: Some(2),
            liquidated_quantity: Some(4.5),
            largest_liquidation: Some(3.0),
            average_margin: Some(200.0),
            largest_margin: Some(600.0),
            account_size_required: Some(690.0),
            account_size_return: Some(0.0644927536231884),
            margin_efficiency: Some(0.2225),
            ..expected_a()
        };
        // Fixture D trades only long: `long` repeats `all`, and `short` holds no trade.
        let long_only = |all: StrategyTradeStats, curve: StrategyPerformance| StrategyPerformance {
            long: all.clone(),
            short: side(Some(0.0)),
            all,
            ..curve
        };
        let d_all = StrategyTradeStats {
            closed_trades: 5,
            open_trades: 1,
            winning_trades: 3,
            losing_trades: 1,
            even_trades: 1,
            net_profit: 120.0,
            net_profit_fraction: Some(0.12),
            gross_profit: 260.0,
            gross_loss: 140.0,
            total_fees: 5.0,
            profit_factor: Some(1.8571428571428572),
            win_rate: Some(0.6),
            average_trade: Some(24.0),
            average_winning_trade: Some(260.0 / 3.0),
            average_losing_trade: Some(140.0),
            win_loss_ratio: Some(260.0 / 3.0 / 140.0),
            largest_winning_trade: Some(150.0),
            largest_winning_fraction: Some(150.0 / 260.0),
            largest_losing_trade: Some(-140.0),
            largest_losing_fraction: Some(1.0),
            net_profit_to_largest_loss: Some(120.0 / 140.0),
            max_consecutive_wins: 2,
            max_consecutive_losses: 1,
            largest_position: Some(20.0),
        };
        let d_half = StrategyTradeStats {
            closed_trades: 2,
            open_trades: 1,
            winning_trades: 1,
            losing_trades: 1,
            even_trades: 0,
            net_profit: -80.0,
            net_profit_fraction: Some(-0.06666666666666667),
            gross_profit: 60.0,
            gross_loss: 140.0,
            total_fees: 3.0,
            profit_factor: Some(0.42857142857142855),
            win_rate: Some(0.5),
            average_trade: Some(-40.0),
            average_winning_trade: Some(60.0),
            average_losing_trade: Some(140.0),
            win_loss_ratio: Some(60.0 / 140.0),
            largest_winning_trade: Some(60.0),
            largest_winning_fraction: Some(1.0),
            largest_losing_trade: Some(-140.0),
            largest_losing_fraction: Some(1.0),
            net_profit_to_largest_loss: Some(-80.0 / 140.0),
            max_consecutive_wins: 1,
            max_consecutive_losses: 1,
            largest_position: Some(20.0),
        };
        // The trade from 16 to 30 January, losing 140 after 3 in fees, alone in a range.
        let d_loss = |fraction: f64, open_trades| StrategyTradeStats {
            closed_trades: 1,
            open_trades,
            losing_trades: 1,
            net_profit: -140.0,
            net_profit_fraction: Some(fraction),
            gross_loss: 140.0,
            total_fees: 3.0,
            profit_factor: Some(0.0),
            win_rate: Some(0.0),
            average_trade: Some(-140.0),
            average_losing_trade: Some(140.0),
            largest_losing_trade: Some(-140.0),
            largest_losing_fraction: Some(1.0),
            net_profit_to_largest_loss: Some(-1.0),
            max_consecutive_losses: 1,
            largest_position: Some(10.0),
            ..Default::default()
        };
        // Each tuple is (close drawdown, fraction, return/drawdown), then
        // (open profit, close run-up, capital), then (intrabar drawdown, fraction, run-up).
        let curve_from =
            |(drawdown, fraction, return_over), (open, rise, capital), (fall, rate, up)| {
                StrategyPerformance {
                    max_drawdown: Some(drawdown),
                    max_drawdown_fraction: Some(fraction),
                    return_over_max_drawdown: return_over,
                    open_profit: Some(open),
                    max_run_up: Some(rise),
                    max_run_up_fraction: Some(rise / capital),
                    intrabar_max_drawdown: Some(fall),
                    intrabar_max_drawdown_fraction: Some(rate),
                    intrabar_max_drawdown_initial_capital_fraction: Some(fall / capital),
                    intrabar_max_run_up: Some(up),
                    intrabar_max_run_up_fraction: Some(up / capital),
                    ..Default::default()
                }
            };
        let mut unavailable = report(1000.0);
        unavailable.unavailable = Some("Exchange rate needed".into());
        let unavailable = unavailable.within(date(2024, 1, 1, 0)..date(2024, 2, 1, 0));
        assert_eq!(
            unavailable.unavailable.as_deref(),
            Some("Exchange rate needed")
        );

        let mut d_no_equity = fixture_d();
        d_no_equity.equity.clear();
        let mut d_unordered = fixture_d();
        d_unordered.equity.reverse();
        let mut d_skipped = fixture_d();
        for (index, point) in [
            (1, (date(2023, 12, 1, 0), f64::NAN)),
            (3, (date(2023, 12, 15, 0), f64::NAN)),
            (5, (date(2023, 12, 1, 0), f64::NAN)),
        ] {
            d_skipped.equity.insert(index, point);
            d_skipped.equity_ranges.insert(
                index,
                EquityRange::new(f64::NAN, f64::NAN, f64::NAN, f64::NAN),
            );
        }
        let mut d_missing = fixture_d();
        d_missing.equity_ranges.pop();
        // The peak of 1400 before January is excluded. Inside the range the peak is
        // 1250, so the fall to 900 is 350; the rise from 900 to 1150 is 250.
        let part = d_skipped.within(date(2024, 1, 5, 0)..date(2024, 3, 1, 0));
        assert_eq!(part.performance().intrabar_max_drawdown, Some(350.0));
        assert_eq!(part.performance().intrabar_max_run_up, Some(250.0));
        assert_eq!(part.equity_ranges, fixture_d().equity_ranges[3..]);
        let part = d_missing.within(date(2024, 1, 5, 0)..date(2024, 3, 1, 0));
        assert!(part.equity_ranges.is_empty());
        assert_eq!(part.performance().intrabar_max_drawdown, None);
        let mut non_positive_start = report(10_000.0);
        non_positive_start.equity = curve(&[-100.0, -400.0], TimeDelta::weeks(13));
        let mut negative_end = report(10_000.0);
        negative_end.equity = curve(&[100.0, -50.0], TimeDelta::hours(8766));
        let mut zero_end = report(10_000.0);
        zero_end.equity = curve(&[100.0, 0.0], TimeDelta::hours(8766));
        let no_intrabar = StrategyPerformance {
            intrabar_max_drawdown: None,
            intrabar_max_run_up: None,
            intrabar_max_run_up_fraction: None,
            intrabar_max_drawdown_fraction: None,
            intrabar_max_drawdown_initial_capital_fraction: None,
            ..expected_d()
        };
        // Largest margin 10000 plus the intrabar fall of 3000, not the close drawdown of 1000.
        let mut intrabar_margin = fixture_intrabar();
        intrabar_margin.margin = curve(&[0.0, 9_000.0, 9_000.0], TimeDelta::hours(1));
        intrabar_margin.peak_margin = curve(&[0.0, 10_000.0, 9_000.0], TimeDelta::hours(1));
        let mut missing = fixture_intrabar();
        missing.equity_ranges.truncate(2);
        let mut unbracketed = fixture_intrabar();
        unbracketed.equity_ranges[2] = EquityRange::new(9500.0, 10_000.0, 500.0, 0.0);
        let mut nan_fall = fixture_intrabar();
        nan_fall.equity_ranges[1].fall = f64::NAN;
        let mut skipped = fixture_intrabar();
        skipped
            .equity
            .insert(2, (t0() + TimeDelta::minutes(90), f64::NAN));
        skipped
            .equity_ranges
            .insert(2, EquityRange::new(f64::NAN, f64::NAN, f64::NAN, f64::NAN));
        let rows = [
            ("margin data", margin, margin_metrics.clone()),
            (
                "calls and peaks without margin marks",
                no_margin_marks,
                expected_a(),
            ),
            (
                "zero margin without calls",
                unfunded,
                StrategyPerformance {
                    margin_calls: Some(0),
                    liquidated_quantity: Some(0.0),
                    average_margin: Some(0.0),
                    largest_margin: Some(0.0),
                    account_size_required: Some(90.0),
                    account_size_return: Some(0.49444444444444446),
                    ..expected_a()
                },
            ),
            (
                "margin without equity",
                margin_without_equity,
                StrategyPerformance {
                    max_drawdown: None,
                    max_drawdown_fraction: None,
                    return_over_max_drawdown: None,
                    open_profit: None,
                    max_run_up: None,
                    max_run_up_fraction: None,
                    average_drawdown: None,
                    average_drawdown_days: None,
                    average_run_up: None,
                    average_run_up_days: None,
                    account_size_required: None,
                    account_size_return: None,
                    ..margin_metrics
                },
            ),
            ("D", fixture_intrabar(), expected_d()),
            (
                "D with margin data",
                intrabar_margin,
                StrategyPerformance {
                    margin_calls: Some(0),
                    liquidated_quantity: Some(0.0),
                    average_margin: Some(6_000.0),
                    largest_margin: Some(10_000.0),
                    account_size_required: Some(13_000.0),
                    account_size_return: Some(0.0),
                    margin_efficiency: Some(0.0),
                    ..expected_d()
                },
            ),
            ("D with one range missing", missing, no_intrabar.clone()),
            (
                "D with a low above its close",
                unbracketed,
                no_intrabar.clone(),
            ),
            ("D with a NaN fall", nan_fall, no_intrabar),
            (
                "D with a skipped point",
                skipped,
                StrategyPerformance {
                    skipped_equity_points: 1,
                    ..expected_d()
                },
            ),
            (
                "Unavailable range",
                unavailable,
                StrategyPerformance {
                    open_profit: Some(0.0),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    ..Default::default()
                },
            ),
            (
                "D unordered equity",
                d_unordered.within(date(2000, 1, 1, 0)..date(2100, 1, 1, 0)),
                long_only(d_all.clone(), StrategyPerformance::default()),
            ),
            (
                "D whole run",
                fixture_d().within(date(2000, 1, 1, 0)..date(2100, 1, 1, 0)),
                long_only(
                    d_all,
                    StrategyPerformance {
                        average_drawdown: Some(340.0),
                        average_drawdown_days: Some(57.0),
                        average_run_up: Some(210.0),
                        average_run_up_days: Some(12.5),
                        ..curve_from(
                            (340.0, 340.0 / 1300.0, Some(120.0 / 340.0)),
                            (-40.0, 300.0, 1000.0),
                            (500.0, 500.0 / 1400.0, 500.0),
                        )
                    },
                ),
            ),
            (
                "D second half",
                fixture_d().within(date(2024, 1, 5, 0)..date(2024, 3, 1, 0)),
                long_only(
                    d_half.clone(),
                    StrategyPerformance {
                        average_drawdown: Some(240.0),
                        average_drawdown_days: Some(26.0),
                        average_run_up: Some(120.0),
                        average_run_up_days: Some(10.0),
                        ..curve_from(
                            (240.0, 0.2, Some(-80.0 / 240.0)),
                            (-40.0, 120.0, 1200.0),
                            (350.0, 0.28, 250.0),
                        )
                    },
                ),
            ),
            (
                "D ending inside a trade",
                fixture_d().within(date(2024, 1, 5, 0)..date(2024, 2, 5, 0)),
                // Capital 1200 + closed profit -140 + open profit -100 = last equity 960.
                long_only(
                    d_loss(-0.11666666666666667, 1),
                    StrategyPerformance {
                        average_drawdown: Some(240.0),
                        average_drawdown_days: Some(16.0),
                        ..curve_from(
                            (240.0, 0.2, Some(-140.0 / 240.0)),
                            (-100.0, 0.0, 1200.0),
                            (350.0, 0.28, 200.0),
                        )
                    },
                ),
            ),
            (
                "D range edges",
                fixture_d().within(date(2024, 1, 15, 0)..date(2024, 1, 30, 15)),
                long_only(
                    StrategyTradeStats {
                        open_trades: 1,
                        net_profit_fraction: Some(0.0),
                        largest_position: Some(10.0),
                        ..Default::default()
                    },
                    curve_from((0.0, 0.0, None), (0.0, 0.0, 1200.0), (150.0, 0.12, 200.0)),
                ),
            ),
            (
                "D exit at start",
                fixture_d().within(date(2024, 1, 30, 15)..date(2024, 1, 31, 1)),
                long_only(
                    d_loss(-0.14583333333333334, 0),
                    curve_from(
                        (0.0, 0.0, None),
                        (0.0, 0.0, 960.0),
                        (320.0, 320.0 / 1220.0, 60.0),
                    ),
                ),
            ),
            (
                "D after run",
                fixture_d().within(date(2030, 1, 1, 0)..date(2031, 1, 1, 0)),
                StrategyPerformance {
                    short: side(None),
                    ..long_only(
                        StrategyTradeStats {
                            open_trades: 1,
                            largest_position: Some(10.0),
                            ..Default::default()
                        },
                        StrategyPerformance::default(),
                    )
                },
            ),
            (
                "D empty range",
                fixture_d().within(date(2024, 1, 15, 0)..date(2024, 1, 15, 0)),
                StrategyPerformance {
                    open_profit: Some(0.0),
                    ..Default::default()
                },
            ),
            (
                "D reversed range",
                fixture_d().within(date(2024, 2, 15, 0)..date(2024, 1, 15, 0)),
                StrategyPerformance {
                    open_profit: Some(0.0),
                    ..Default::default()
                },
            ),
            (
                "D without equity, second half",
                d_no_equity.within(date(2024, 1, 5, 0)..date(2024, 3, 1, 0)),
                long_only(
                    StrategyTradeStats {
                        net_profit_fraction: Some(-0.08),
                        ..d_half
                    },
                    StrategyPerformance::default(),
                ),
            ),
            (
                "streaks ordered by exit, with breakevens, open and invalid trades",
                streaks,
                StrategyPerformance {
                    all: StrategyTradeStats {
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
                        even_trades: 2,
                        average_winning_trade: Some(3.6),
                        average_losing_trade: Some(10.0 / 3.0),
                        win_loss_ratio: Some(1.08),
                        largest_winning_fraction: Some(5.0 / 18.0),
                        largest_losing_fraction: Some(0.7),
                        net_profit_to_largest_loss: Some(8.0 / 7.0),
                        largest_position: Some(11.0),
                        ..Default::default()
                    },
                    long: StrategyTradeStats {
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
                        even_trades: 2,
                        average_winning_trade: Some(3.6),
                        average_losing_trade: Some(10.0 / 3.0),
                        win_loss_ratio: Some(1.08),
                        largest_winning_fraction: Some(5.0 / 18.0),
                        largest_losing_fraction: Some(0.7),
                        net_profit_to_largest_loss: Some(8.0 / 7.0),
                        largest_position: Some(11.0),
                        ..Default::default()
                    },
                    short: side(Some(0.0)),
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
                    open_profit: Some(0.0),
                    max_run_up: Some(950.0),
                    max_run_up_fraction: Some(0.095),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    return_over_max_drawdown: Some(0.0),
                    max_drawdown: Some(100.0),
                    max_drawdown_fraction: Some(0.5),
                    average_drawdown: Some(75.0),
                    average_drawdown_days: Some(0.0625),
                    average_run_up: Some(950.0),
                    average_run_up_days: Some(1.0 / 24.0),
                    ..Default::default()
                },
            ),
            (
                "C hourly over 13 weeks, rate 0.02",
                fixture_c(hourly_13_weeks(), 0.02),
                StrategyPerformance {
                    open_profit: Some(0.0),
                    max_run_up: Some(6642.0),
                    max_run_up_fraction: Some(0.6642),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    return_over_max_drawdown: Some(0.0),
                    max_drawdown: Some(82.0),
                    max_drawdown_fraction: Some(0.008104368452263293),
                    average_drawdown: Some(73.29268292682927),
                    average_drawdown_days: Some(0.24638663053297202),
                    average_run_up: Some(91.07046070460704),
                    average_run_up_days: Some(0.15616531165311653),
                    compound_annual_growth: Some(6.5726938646276025),
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
                    open_profit: Some(0.0),
                    max_run_up: Some(885.0),
                    max_run_up_fraction: Some(0.0885),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    return_over_max_drawdown: Some(0.0),
                    max_drawdown: Some(82.0),
                    max_drawdown_fraction: Some(0.008104368452263293),
                    average_drawdown: Some(72.41304347826087),
                    average_drawdown_days: Some(7.934782608695652),
                    average_run_up: Some(90.32608695652173),
                    average_run_up_days: Some(5.239130434782608),
                    compound_annual_growth: Some(0.0819914006635325),
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
                    open_profit: Some(0.0),
                    max_run_up: Some(6642.0),
                    max_run_up_fraction: Some(0.6642),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    return_over_max_drawdown: Some(0.0),
                    max_drawdown: Some(82.0),
                    max_drawdown_fraction: Some(0.008104368452263293),
                    average_drawdown: Some(73.29268292682927),
                    average_drawdown_days: Some(0.24638663053297202),
                    average_run_up: Some(91.07046070460704),
                    average_run_up_days: Some(0.15616531165311653),
                    compound_annual_growth: Some(6.5726938646276025),
                    periods_per_year: Some(8766.0),
                    ..Default::default()
                },
            ),
            (
                "equity below zero",
                negative_equity,
                StrategyPerformance {
                    open_profit: Some(0.0),
                    max_run_up: Some(80.0),
                    max_run_up_fraction: Some(0.008),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    return_over_max_drawdown: Some(0.0),
                    compound_annual_growth: Some(-0.6992589390030648),
                    max_drawdown: Some(150.0),
                    max_drawdown_fraction: Some(1.5),
                    periods_per_year: Some(2.9938524590163933),
                    average_drawdown: Some(150.0),
                    average_drawdown_days: Some(366.0),
                    average_run_up: Some(80.0),
                    average_run_up_days: Some(244.0),
                    ..Default::default()
                },
            ),
            (
                "equity starting at zero",
                zero_start,
                StrategyPerformance {
                    open_profit: Some(0.0),
                    max_run_up: Some(10.0),
                    max_run_up_fraction: Some(0.001),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    return_over_max_drawdown: Some(0.0),
                    max_drawdown: Some(5.0),
                    average_drawdown: Some(5.0),
                    average_drawdown_days: Some(1.0 / 24.0),
                    average_run_up: Some(10.0),
                    average_run_up_days: Some(1.0 / 24.0),
                    ..Default::default()
                },
            ),
            (
                "one winning trade, negative capital",
                negative_capital,
                StrategyPerformance {
                    open_profit: Some(0.0),
                    all: StrategyTradeStats {
                        closed_trades: 1,
                        winning_trades: 1,
                        net_profit: 5.0,
                        gross_profit: 5.0,
                        win_rate: Some(1.0),
                        average_trade: Some(5.0),
                        largest_winning_trade: Some(5.0),
                        max_consecutive_wins: 1,
                        average_winning_trade: Some(5.0),
                        largest_winning_fraction: Some(1.0),
                        largest_position: Some(1.0),
                        ..Default::default()
                    },
                    long: StrategyTradeStats {
                        closed_trades: 1,
                        winning_trades: 1,
                        net_profit: 5.0,
                        gross_profit: 5.0,
                        win_rate: Some(1.0),
                        average_trade: Some(5.0),
                        largest_winning_trade: Some(5.0),
                        max_consecutive_wins: 1,
                        average_winning_trade: Some(5.0),
                        largest_winning_fraction: Some(1.0),
                        largest_position: Some(1.0),
                        ..Default::default()
                    },
                    short: side(None),
                    ..Default::default()
                },
            ),
            (
                "constant growth, rate 0.02",
                constant_growth,
                StrategyPerformance {
                    open_profit: Some(0.0),
                    max_run_up: Some(0.09003600840125614),
                    max_run_up_fraction: Some(9.003600840125613e-06),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    max_drawdown: Some(0.0),
                    max_drawdown_fraction: Some(0.0),
                    compound_annual_growth: Some(0.0008912060071077921),
                    periods_per_year: Some(8.908536585365853),
                    average_run_up: Some(0.09003600840125614),
                    average_run_up_days: Some(369.0),
                    ..Default::default()
                },
            ),
            (
                "non-finite capital, rate, trade fields and equity",
                non_finite,
                StrategyPerformance {
                    open_profit: Some(0.0),
                    all: side(None),
                    long: side(None),
                    short: side(None),
                    skipped_trades: 1,
                    skipped_equity_points: 1,
                    ..Default::default()
                },
            ),
            (
                "non-positive start over 13 weeks",
                non_positive_start,
                StrategyPerformance {
                    open_profit: Some(0.0),
                    max_run_up: Some(0.0),
                    max_run_up_fraction: Some(0.0),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    max_drawdown: Some(300.0),
                    periods_per_year: Some(4.013736263736264),
                    average_drawdown: Some(300.0),
                    average_drawdown_days: Some(91.0),
                    return_over_max_drawdown: Some(0.0),
                    ..Default::default()
                },
            ),
            (
                "ends below zero after a year",
                negative_end,
                StrategyPerformance {
                    open_profit: Some(0.0),
                    max_run_up: Some(0.0),
                    max_run_up_fraction: Some(0.0),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    max_drawdown: Some(150.0),
                    max_drawdown_fraction: Some(1.5),
                    average_drawdown: Some(150.0),
                    average_drawdown_days: Some(365.25),
                    periods_per_year: Some(1.0),
                    return_over_max_drawdown: Some(0.0),
                    ..Default::default()
                },
            ),
            (
                "ends at zero after a year",
                zero_end,
                StrategyPerformance {
                    open_profit: Some(0.0),
                    max_run_up: Some(0.0),
                    max_run_up_fraction: Some(0.0),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    max_drawdown: Some(100.0),
                    max_drawdown_fraction: Some(1.0),
                    average_drawdown: Some(100.0),
                    average_drawdown_days: Some(365.25),
                    periods_per_year: Some(1.0),
                    return_over_max_drawdown: Some(0.0),
                    compound_annual_growth: Some(-1.0),
                    ..Default::default()
                },
            ),
            (
                "flat stretches at a peak and at a trough",
                flat_stretches,
                StrategyPerformance {
                    open_profit: Some(0.0),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
                    max_drawdown: Some(20.0),
                    max_drawdown_fraction: Some(0.18181818181818182),
                    max_run_up: Some(20.0),
                    max_run_up_fraction: Some(0.002),
                    return_over_max_drawdown: Some(0.0),
                    average_drawdown: Some(20.0),
                    average_drawdown_days: Some(3.0),
                    average_run_up: Some(15.0),
                    average_run_up_days: Some(1.0),
                    ..Default::default()
                },
            ),
            (
                "empty",
                report(10_000.0),
                StrategyPerformance {
                    open_profit: Some(0.0),
                    all: side(Some(0.0)),
                    long: side(Some(0.0)),
                    short: side(Some(0.0)),
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
        // Reports saved before margin fields existed retain unavailable margin metrics.
        let mut saved = serde_json::to_value(&saved_margin).unwrap();
        for key in ["margin", "peak_margin", "margin_calls"] {
            saved.as_object_mut().unwrap().remove(key);
        }
        let old: StrategyReport = serde_json::from_value(saved).unwrap();
        assert_matches(
            "saved before margin data",
            &old.performance(),
            &expected_a(),
        );
    }

    #[test]
    fn reports_saved_before_equity_ranges_load_without_intrabar_figures() {
        // Report JSON saved on origin/main at 6d5462888, before equity ranges.
        let saved = r#"{"name":"Held through a 30% dip","symbol":"HELD","timeframe":"Hour1","initial_capital":10000.0,"currency":null,"risk_free_rate":0.0,"properties":[["Order size","Fixed units: 1"],["Pyramiding","1"],["Commission","Fraction: 0"],["Slippage","Fraction: 0"],["Leverage","1"],["Long funding fraction","0"],["Short funding fraction","1"],["Long margin calls","Off"],["Short margin calls","Four times the shortfall at price points"],["Verify limit crossing","false"],["Price increment","0.01"],["Liquidation unit step","1"],["Order execution delay","0"],["Historical tick decisions","false"],["Decision chart type","Candles"],["Original-price execution","false"],["Fill on close","false"],["Recalculate after fills","false"],["Recalculate on every update","false"],["Start time","All supplied history"],["End time","Newest supplied bar"],["Lower-interval bars","0"],["Rejected orders","0"]],"trades":[{"side":"Buy","quantity":100.0,"entry_time":"1970-01-01T01:00:00Z","entry_price":100.0,"exit":null,"fees":0.0,"label":"Held"}],"equity":[["1969-12-31T23:00:00Z",10000.0],["1970-01-01T00:00:00Z",10000.0],["1970-01-01T01:00:00Z",10000.0],["1970-01-01T02:00:00Z",9000.0]]}"#;
        let loaded: StrategyReport = serde_json::from_str(saved).unwrap();
        assert!(loaded.equity_ranges.is_empty());
        assert_matches(
            "saved before equity ranges",
            &loaded.performance(),
            &StrategyPerformance {
                intrabar_max_drawdown: None,
                intrabar_max_run_up: None,
                intrabar_max_run_up_fraction: None,
                intrabar_max_drawdown_fraction: None,
                intrabar_max_drawdown_initial_capital_fraction: None,
                ..expected_d()
            },
        );
    }

    #[test]
    fn analysis_matches_reference() {
        let group = |closed_trades, winning_trades, net_profit, win_rate| StrategyTradeGroup {
            closed_trades,
            winning_trades,
            net_profit,
            win_rate,
        };
        let empty = StrategyAnalysis {
            months: vec![],
            years: vec![],
            result_bin_width: 0.001,
            result_bins: vec![],
            win_runs: vec![],
            loss_runs: vec![],
            by_hour: Default::default(),
            by_weekday: Default::default(),
            by_month: Default::default(),
        };
        let bars: Vec<_> = fixture_d()
            .equity
            .iter()
            .zip([
                (100.0_f64, 100.0_f64),
                (100.0, 120.0),
                (120.0, 108.0),
                (108.0, 90.0),
                (90.0, 99.0),
                (99.0, 81.0),
            ])
            .map(|(&(t, _), (o, c))| Bar::new(t, o, o.max(c), o.min(c), c, 1.0))
            .collect();
        // Bases: equity 1000, 1000, 1050, 960; passive 100, 100, 108, 99.
        let mut expected = StrategyAnalysis {
            months: [
                (2023, 11, 0.0, 0.0, group(0, 0, 0.0, None)),
                (
                    2023,
                    12,
                    0.050000000000000044,
                    0.08000000000000007,
                    group(1, 1, 50.0, Some(1.0)),
                ),
                (
                    2024,
                    1,
                    -0.08571428571428574,
                    -0.08333333333333337,
                    group(3, 1, 10.0, Some(1.0 / 3.0)),
                ),
                (
                    2024,
                    2,
                    0.125,
                    -0.18181818181818177,
                    group(1, 1, 60.0, Some(1.0)),
                ),
            ]
            .into_iter()
            .map(|(y, m, r, b, trades)| StrategyPeriod {
                start: date(y, m, 1, 0),
                equity_return: Some(r),
                buy_and_hold_return: Some(b),
                trades,
            })
            .collect(),
            years: vec![
                StrategyPeriod {
                    start: date(2023, 1, 1, 0),
                    equity_return: Some(0.050000000000000044),
                    buy_and_hold_return: Some(0.08000000000000007),
                    trades: group(1, 1, 50.0, Some(1.0)),
                },
                StrategyPeriod {
                    start: date(2024, 1, 1, 0),
                    equity_return: Some(0.02857142857142847),
                    buy_and_hold_return: Some(-0.25),
                    trades: group(4, 2, 70.0, Some(0.5)),
                },
            ],
            result_bin_width: 0.02,
            result_bins: (0..15)
                .map(|k| StrategyResultBin {
                    lower: (k as f64 - 7.0) * 0.02,
                    winners: usize::from([9, 10, 14].contains(&k)),
                    losers: usize::from(k == 0),
                    breakeven: usize::from(k == 7),
                })
                .collect(),
            win_runs: vec![1, 1],
            loss_runs: vec![1],
            ..empty.clone()
        };
        expected.by_hour[9] = group(3, 1, -80.0, Some(1.0 / 3.0));
        expected.by_hour[14] = group(1, 1, 50.0, Some(1.0));
        expected.by_hour[22] = group(1, 1, 150.0, Some(1.0));
        expected.by_weekday[1] = group(2, 0, -140.0, Some(0.0));
        expected.by_weekday[3] = group(2, 2, 110.0, Some(1.0));
        expected.by_weekday[6] = group(1, 1, 150.0, Some(1.0));
        expected.by_month[0] = group(2, 0, -140.0, Some(0.0));
        expected.by_month[1] = group(1, 1, 60.0, Some(1.0));
        expected.by_month[10] = group(1, 1, 50.0, Some(1.0));
        expected.by_month[11] = group(1, 1, 150.0, Some(1.0));
        let mut sparse_report = fixture_d();
        sparse_report.equity.retain(|(t, _)| t.month() != 12);
        let sparse_bars: Vec<_> = bars
            .iter()
            .filter(|b| b.time.month() != 12)
            .cloned()
            .collect();
        let mut sparse_expected = expected.clone();
        sparse_expected.months[1].equity_return = None;
        sparse_expected.months[1].buy_and_hold_return = None;
        sparse_expected.months[2].equity_return = Some(-0.04);
        sparse_expected.months[2].buy_and_hold_return = Some(-0.01);
        sparse_expected.years[0].equity_return = Some(0.0);
        sparse_expected.years[0].buy_and_hold_return = Some(0.0);
        sparse_expected.years[1].equity_return = Some(0.08);
        sparse_expected.years[1].buy_and_hold_return = Some(-0.19);
        let mut unordered = bars.clone();
        unordered.reverse();
        let mut no_benchmark = expected.clone();
        for p in no_benchmark
            .months
            .iter_mut()
            .chain(&mut no_benchmark.years)
        {
            p.buy_and_hold_return = None;
        }
        let mut zero_price = report(1000.0);
        let mut t = StrategyTrade::new(OrderSide::Buy, 1.0, date(2024, 1, 2, 9), 0.0);
        t.exit = Some(StrategyExit::new(date(2024, 1, 2, 10), 5.0, 5.0));
        zero_price.trades.push(t);
        let g = group(1, 1, 5.0, Some(1.0));
        let mut zero_expected = StrategyAnalysis {
            months: vec![StrategyPeriod {
                start: date(2024, 1, 1, 0),
                equity_return: None,
                buy_and_hold_return: None,
                trades: g,
            }],
            win_runs: vec![1],
            ..empty.clone()
        };
        zero_expected.years = zero_expected.months.clone();
        zero_expected.by_hour[9] = g;
        zero_expected.by_weekday[1] = g;
        zero_expected.by_month[0] = g;
        let group_close = |a: &StrategyTradeGroup, b: &StrategyTradeGroup| {
            a.closed_trades == b.closed_trades
                && a.winning_trades == b.winning_trades
                && close(a.net_profit, b.net_profit)
                && option_close(a.win_rate, b.win_rate)
        };
        for (case, r, source, expected) in [
            ("D with source bars", fixture_d(), bars, expected),
            (
                "D with December missing from both curves",
                sparse_report,
                sparse_bars,
                sparse_expected,
            ),
            (
                "D with unordered bars",
                fixture_d(),
                unordered,
                no_benchmark,
            ),
            ("empty", report(10_000.0), vec![], empty),
            ("zero entry price", zero_price, vec![], zero_expected),
        ] {
            let actual = r.analysis(&source);
            for (a, b) in [
                (&actual.months, &expected.months),
                (&actual.years, &expected.years),
            ] {
                assert_eq!(a.len(), b.len(), "{case}");
                for (a, b) in a.iter().zip(b) {
                    assert!(
                        a.start == b.start
                            && option_close(a.equity_return, b.equity_return)
                            && option_close(a.buy_and_hold_return, b.buy_and_hold_return)
                            && group_close(&a.trades, &b.trades),
                        "{case}: {a:?} != {b:?}"
                    );
                }
            }
            assert!(
                close(actual.result_bin_width, expected.result_bin_width),
                "{case}"
            );
            assert_eq!(
                actual.result_bins.len(),
                expected.result_bins.len(),
                "{case}"
            );
            for (a, b) in actual.result_bins.iter().zip(&expected.result_bins) {
                assert!(
                    close(a.lower, b.lower)
                        && a.winners == b.winners
                        && a.losers == b.losers
                        && a.breakeven == b.breakeven,
                    "{case}: {a:?} != {b:?}"
                );
            }
            assert_eq!(actual.win_runs, expected.win_runs, "{case}");
            assert_eq!(actual.loss_runs, expected.loss_runs, "{case}");
            for (a, b) in actual
                .by_hour
                .iter()
                .chain(&actual.by_weekday)
                .chain(&actual.by_month)
                .zip(
                    expected
                        .by_hour
                        .iter()
                        .chain(&expected.by_weekday)
                        .chain(&expected.by_month),
                )
            {
                assert!(group_close(a, b), "{case}: {a:?} != {b:?}");
            }
        }
    }

    #[test]
    fn trade_sides_holding_and_exposure_match_reference() {
        let bars: Vec<_> = (0..10)
            .map(|i| {
                Bar::new(
                    t0() + TimeDelta::hours(i),
                    100.0,
                    130.0,
                    90.0,
                    if i == 9 { 125.0 } else { 100.0 },
                    1.0,
                )
            })
            .collect();
        let trade = |side, quantity, entry, exit: Option<i64>, profit, fees| {
            let mut t = StrategyTrade::new(side, quantity, t0() + TimeDelta::minutes(entry), 100.0);
            t.exit =
                exit.map(|exit| StrategyExit::new(t0() + TimeDelta::minutes(exit), 100.0, profit));
            t.fees = fees;
            t
        };
        let mut sides = report(1_000.0);
        sides.trades = vec![
            trade(OrderSide::Buy, 2.0, 0, Some(240), 50.0, 1.0),
            trade(OrderSide::Buy, 3.0, 60, Some(180), -20.0, 1.0),
            trade(OrderSide::Sell, 1.0, 180, Some(420), 30.0, 0.5),
            trade(OrderSide::Sell, 4.0, 450, Some(465), 0.0, 0.5),
        ];
        sides.equity = vec![
            (t0(), 1000.0),
            (t0() + TimeDelta::hours(8766), 900.0),
            (t0() + TimeDelta::hours(17532), 1210.0),
        ];
        let long = StrategyTradeStats {
            closed_trades: 2,
            winning_trades: 1,
            losing_trades: 1,
            net_profit: 30.0,
            net_profit_fraction: Some(0.03),
            gross_profit: 50.0,
            gross_loss: 20.0,
            total_fees: 2.0,
            profit_factor: Some(2.5),
            win_rate: Some(0.5),
            average_trade: Some(15.0),
            average_winning_trade: Some(50.0),
            average_losing_trade: Some(20.0),
            win_loss_ratio: Some(2.5),
            largest_winning_trade: Some(50.0),
            largest_winning_fraction: Some(1.0),
            largest_losing_trade: Some(-20.0),
            largest_losing_fraction: Some(1.0),
            net_profit_to_largest_loss: Some(1.5),
            max_consecutive_wins: 1,
            max_consecutive_losses: 1,
            largest_position: Some(5.0),
            ..Default::default()
        };
        let expected_sides = StrategyPerformance {
            open_profit: Some(0.0),
            max_run_up: Some(310.0),
            max_run_up_fraction: Some(0.31),
            all: StrategyTradeStats {
                closed_trades: 4,
                winning_trades: 2,
                losing_trades: 1,
                even_trades: 1,
                net_profit: 60.0,
                net_profit_fraction: Some(0.06),
                gross_profit: 80.0,
                gross_loss: 20.0,
                total_fees: 3.0,
                profit_factor: Some(4.0),
                win_rate: Some(0.5),
                average_trade: Some(15.0),
                average_winning_trade: Some(40.0),
                average_losing_trade: Some(20.0),
                win_loss_ratio: Some(2.0),
                largest_winning_trade: Some(50.0),
                largest_winning_fraction: Some(0.625),
                largest_losing_trade: Some(-20.0),
                largest_losing_fraction: Some(1.0),
                net_profit_to_largest_loss: Some(3.0),
                max_consecutive_wins: 2,
                max_consecutive_losses: 1,
                largest_position: Some(5.0),
                ..Default::default()
            },
            long: long.clone(),
            short: StrategyTradeStats {
                closed_trades: 2,
                winning_trades: 1,
                even_trades: 1,
                net_profit: 30.0,
                net_profit_fraction: Some(0.03),
                gross_profit: 30.0,
                total_fees: 1.0,
                win_rate: Some(0.5),
                average_trade: Some(15.0),
                average_winning_trade: Some(30.0),
                largest_winning_trade: Some(30.0),
                largest_winning_fraction: Some(1.0),
                max_consecutive_wins: 1,
                largest_position: Some(4.0),
                ..Default::default()
            },
            max_drawdown: Some(100.0),
            max_drawdown_fraction: Some(0.1),
            average_drawdown: Some(100.0),
            average_drawdown_days: Some(730.5),
            average_run_up: Some(310.0),
            average_run_up_days: Some(365.25),
            return_over_max_drawdown: Some(0.6),
            compound_annual_growth: Some(0.1),
            periods_per_year: Some(1.0),
            sharpe_ratio: Some(0.3889087296526013),
            sortino_ratio: Some(1.7284832429004509),
            ..Default::default()
        };
        let mut overlap = report(1_000.0);
        overlap.trades = sides.trades[..2].to_vec();
        let mut reversal = report(1_000.0);
        reversal.trades = vec![
            trade(OrderSide::Buy, 2.0, 0, Some(120), 10.0, 0.0),
            trade(OrderSide::Sell, 3.0, 120, None, 0.0, 0.0),
            trade(OrderSide::Buy, 1.0, 300, Some(300), -1.0, 0.0),
        ];
        let reversed = StrategyTradeStats {
            closed_trades: 2,
            open_trades: 1,
            winning_trades: 1,
            losing_trades: 1,
            net_profit: 9.0,
            net_profit_fraction: Some(0.009),
            gross_profit: 10.0,
            gross_loss: 1.0,
            profit_factor: Some(10.0),
            win_rate: Some(0.5),
            average_trade: Some(4.5),
            average_winning_trade: Some(10.0),
            average_losing_trade: Some(1.0),
            win_loss_ratio: Some(10.0),
            largest_winning_trade: Some(10.0),
            largest_winning_fraction: Some(1.0),
            largest_losing_trade: Some(-1.0),
            largest_losing_fraction: Some(1.0),
            net_profit_to_largest_loss: Some(9.0),
            max_consecutive_wins: 1,
            max_consecutive_losses: 1,
            largest_position: Some(4.0),
            ..Default::default()
        };
        let holding = |average, wins, losses| StrategyHolding {
            average_bars: Some(average),
            average_winning_bars: Some(wins),
            average_losing_bars: losses,
        };
        for (case, report, expected, holds, benchmark) in [
            (
                "sides",
                sides,
                expected_sides,
                [
                    holding(2.5, 4.0, Some(2.0)),
                    holding(3.0, 4.0, Some(2.0)),
                    holding(2.0, 4.0, None),
                ],
                (250.0, -190.0),
            ),
            (
                "overlapping longs",
                overlap,
                StrategyPerformance {
                    open_profit: Some(0.0),
                    all: long.clone(),
                    long,
                    short: side(Some(0.0)),
                    ..Default::default()
                },
                [
                    holding(3.0, 4.0, Some(2.0)),
                    holding(3.0, 4.0, Some(2.0)),
                    StrategyHolding::default(),
                ],
                (250.0, -220.0),
            ),
            (
                "reversal, zero-length and open trades",
                reversal,
                StrategyPerformance {
                    long: StrategyTradeStats {
                        open_trades: 0,
                        largest_position: Some(2.0),
                        ..reversed.clone()
                    },
                    short: StrategyTradeStats {
                        open_trades: 1,
                        largest_position: Some(3.0),
                        ..side(Some(0.0))
                    },
                    all: reversed,
                    ..Default::default()
                },
                [
                    holding(1.0, 2.0, Some(0.0)),
                    holding(1.0, 2.0, Some(0.0)),
                    StrategyHolding::default(),
                ],
                (250.0, -241.0),
            ),
        ] {
            assert_matches(case, &report.performance(), &expected);
            let c = report.comparison(&bars);
            assert_eq!([c.all, c.long, c.short], holds, "{case}");
            assert!(
                (c.buy_and_hold_profit.unwrap() - benchmark.0).abs() < 1e-12,
                "{case}"
            );
            assert!(
                (c.buy_and_hold_outperformance.unwrap() - benchmark.1).abs() < 1e-12,
                "{case}"
            );
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn holding_totals_preserve_averages_beyond_32_bit_range() {
        // Invalid prices keep excursions short; timestamps still count toward holding.
        let bars: Vec<_> = (0..100_000)
            .map(|i| {
                Bar::new(
                    t0() + TimeDelta::minutes(i),
                    f64::NAN,
                    f64::NAN,
                    f64::NAN,
                    f64::NAN,
                    1.0,
                )
            })
            .collect();
        let mut r = report(1_000.0);
        r.timeframe = Timeframe::Min1;
        let mut trade = closed(0.0, 0.0);
        trade.exit.as_mut().unwrap().time = bars.last().unwrap().time;
        r.trades = vec![trade; 50_000];
        // 50,000 * 99,999 = 4,999,950,000 held bars, beyond a 32-bit index.
        let c = r.comparison(&bars);
        assert_eq!(c.all.average_bars, Some(99_999.0));
        assert_eq!(c.long, c.all);
        assert_eq!(c.short, StrategyHolding::default());
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
        for (case, source, skipped, expected_return, expected_excursions, holding_all) in [
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
                StrategyHolding {
                    average_bars: Some(2.0),
                    average_losing_bars: Some(2.0),
                    ..Default::default()
                },
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
                StrategyHolding {
                    average_bars: Some(1.0),
                    average_losing_bars: Some(1.0),
                    ..Default::default()
                },
            ),
            (
                "empty",
                vec![],
                0,
                None,
                vec![None; 4],
                StrategyHolding::default(),
            ),
            (
                "unordered",
                vec![bars[1].clone(), bars[0].clone()],
                2,
                None,
                vec![None; 4],
                StrategyHolding::default(),
            ),
            (
                "duplicate",
                vec![bars[0].clone(), bars[0].clone()],
                2,
                None,
                vec![None; 4],
                StrategyHolding::default(),
            ),
        ] {
            let c = r.comparison(&source);
            assert_eq!(c.skipped_bars, skipped, "{case}");
            assert_eq!(c.all, holding_all, "{case}");
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
                assert!(
                    (c.buy_and_hold_profit.unwrap() - 200.0).abs() < 1e-9,
                    "{case}"
                );
                assert!(
                    (c.buy_and_hold_outperformance.unwrap() + 240.0).abs() < 1e-9,
                    "{case}"
                );
                assert_eq!(
                    c.buy_and_hold_equity
                        .iter()
                        .map(|p| p.1)
                        .collect::<Vec<_>>(),
                    [1100.0, 900.0, 1200.0]
                );
            } else {
                assert!(c.buy_and_hold_equity.is_empty());
                assert_eq!(c.buy_and_hold_profit, None);
                assert_eq!(c.buy_and_hold_outperformance, None);
            }
        }
        let mut invalid_bars = bars.clone();
        invalid_bars[1].high = f64::NAN;
        let c = r.comparison(&invalid_bars);
        assert_eq!(c.skipped_bars, 1);
        assert_eq!(c.all.average_bars, Some(2.0));
        assert_eq!(c.excursions[0], None); // A missing held bar must not understate risk.
        for capital in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            r.initial_capital = capital;
            let c = r.comparison(&bars);
            assert!(c.buy_and_hold_equity.is_empty());
            assert_eq!(c.buy_and_hold_profit, None);
            assert_eq!(c.buy_and_hold_outperformance, None);
        }
        r.initial_capital = 1_000.0;
        let mut zero = bars.clone();
        zero[0].open = 0.0;
        zero[0].low = 0.0;
        let c = r.comparison(&zero);
        assert_eq!(c.buy_and_hold_return, None);
        assert_eq!(c.buy_and_hold_profit, None);
        assert_eq!(c.buy_and_hold_outperformance, None);

        // For two units entered at 100, the first bar's [95, 115] gives MAE/MFE 10/30.
        // The second bar adds a low of 85, giving 30/30. A fill at its open excludes its range.
        for (case, entry_minutes, exit_minutes, expected, holding) in [
            ("entry before the source", -60, 60, Some((10.0, 30.0)), None),
            ("intrabar round trip", 30, 45, Some((10.0, 30.0)), Some(0.0)),
            (
                "intrabar entry across bars",
                30,
                120,
                Some((30.0, 30.0)),
                Some(2.0),
            ),
            (
                "exit at the next open",
                30,
                60,
                Some((10.0, 30.0)),
                Some(1.0),
            ),
            (
                "entry at the next open",
                60,
                120,
                Some((30.0, 24.0)),
                Some(1.0),
            ),
            ("entry after the source ends", 240, 300, None, Some(0.0)),
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
            assert_eq!(r.comparison(&bars).all.average_bars, holding, "{case}");
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
    #[cfg(feature = "csv")]
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
        let comparison = StrategyComparison {
            excursions: vec![
                Some(StrategyExcursion {
                    adverse: 0.5,
                    favourable: 3.25,
                }),
                None,
                Some(StrategyExcursion {
                    adverse: 6.0,
                    favourable: 16.0,
                }),
                None,
            ],
            ..Default::default()
        };
        assert_eq!(
            r.trades_csv_with_excursions(&comparison),
            "number,side,quantity,entry_time,entry_price,exit_time,exit_price,profit,fees,label,mae,mfe,cumulative_profit\n\
             1,long,1,2024-01-02T01:00:00Z,100,2024-01-02T02:00:00Z,101,-1.25,0.25,,6,16,-1.25\n\
             2,long,1,2024-01-02T01:00:00Z,100,2024-01-02T03:00:00Z,101,3,0.5,,,,1.75\n\
             3,short,2,2024-01-02T05:00:00Z,101.5,,,,0,\"Breakout, short\",0.5,3.25,\n"
        );
        assert_eq!(
            r.trades_csv(),
            "number,side,quantity,entry_time,entry_price,exit_time,exit_price,profit,fees,label,mae,mfe,cumulative_profit\n\
             1,long,1,2024-01-02T01:00:00Z,100,2024-01-02T02:00:00Z,101,-1.25,0.25,,,,-1.25\n\
             2,long,1,2024-01-02T01:00:00Z,100,2024-01-02T03:00:00Z,101,3,0.5,,,,1.75\n\
             3,short,2,2024-01-02T05:00:00Z,101.5,,,,0,\"Breakout, short\",,,\n"
        );
        // 10 units bought at 100 and sold at 110 through a bar from 90 to 120, converted at
        // 0.5 EUR per USD: profit, drawdown and run-up are 50, 50 and 100 EUR. A rate that is
        // not positive leaves the excursions unavailable rather than understating risk.
        r.currency = Some("EUR".into());
        let bar = Bar::new(t0() + TimeDelta::hours(1), 100.0, 120.0, 90.0, 110.0, 0.0);
        for (rate, excursions) in [(0.5, "50,100"), (0.0, ",")] {
            let mut trade =
                StrategyTrade::new(OrderSide::Buy, 10.0, t0() + TimeDelta::hours(1), 100.0);
            trade.exit = Some(StrategyExit::new(t0() + TimeDelta::hours(2), 110.0, 50.0));
            trade.label = Some("L / X".into());
            trade.exchange_rate = rate;
            r.trades = vec![trade];
            assert_eq!(
                r.trades_csv_with_excursions(&r.comparison(std::slice::from_ref(&bar))),
                format!(
                    "number,side,quantity,entry_time,entry_price,exit_time,exit_price,profit,fees,label,mae,mfe,cumulative_profit\n\
                     1,long,10,2024-01-02T01:00:00Z,100,2024-01-02T02:00:00Z,110,50,0,L / X,{excursions},50\n"
                ),
                "rate {rate}"
            );
        }
        // Entry order passes over an open trade and excludes the invalid trade's 100.
        let largest = f64::MAX.to_string();
        for (case, profits, expected) in [
            (
                "continues past an open trade",
                vec![Some(10.0), None, Some(-4.0), Some(1.5)],
                vec!["10", "", "6", "7.5"],
            ),
            (
                "a total beyond f64 is unavailable",
                vec![Some(f64::MAX), Some(f64::MAX)],
                vec![largest.as_str(), ""],
            ),
            (
                "retains the sum's precision",
                vec![Some(0.1), Some(0.2)],
                vec!["0.1", "0.30000000000000004"],
            ),
        ] {
            let mut trades: Vec<_> = profits
                .iter()
                .enumerate()
                .map(|(hour, profit)| {
                    let entry = t0() + TimeDelta::hours(hour as i64);
                    let mut trade = StrategyTrade::new(OrderSide::Buy, 1.0, entry, 100.0);
                    trade.exit = profit.map(|profit| StrategyExit::new(entry, 101.0, profit));
                    trade
                })
                .collect();
            trades.reverse();
            let mut invalid = closed(100.0, 0.0);
            invalid.quantity = f64::NAN;
            trades.insert(1, invalid);
            r.trades = trades;
            let csv = r.trades_csv();
            let cells: Vec<String> = csv::Reader::from_reader(csv.as_bytes())
                .records()
                .map(|row| row.unwrap()[12].to_owned())
                .collect();
            assert_eq!(cells, expected, "{case}");
        }
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
    #[cfg(feature = "csv")]
    fn summary_csv_writes_metrics_and_properties_with_unavailable_values() {
        let mut r = fixture_a();
        r.currency = Some("USD".into());
        r.properties
            .push(("Fast, length".into(), "say \"hi\"".into()));
        assert_eq!(
            r.summary_csv(&r.performance(), &r.comparison(&[])),
            "section,name,value,unit,currency,side\n\
             summary,Net profit,44.5,money,USD,all\n\
             summary,Net profit,44.5,money,USD,long\n\
             summary,Net profit,0,money,USD,short\n\
             summary,Net return,0.445,percent,,all\n\
             summary,Net return,0.445,percent,,long\n\
             summary,Net return,0,percent,,short\n\
             summary,Gross profit,76.5,money,USD,all\n\
             summary,Gross profit,76.5,money,USD,long\n\
             summary,Gross profit,0,money,USD,short\n\
             summary,Gross loss,32,money,USD,all\n\
             summary,Gross loss,32,money,USD,long\n\
             summary,Gross loss,0,money,USD,short\n\
             summary,Profit factor,2.390625,ratio,,all\n\
             summary,Profit factor,2.390625,ratio,,long\n\
             summary,Profit factor,,ratio,,short\n\
             summary,Fees,5,money,USD,all\n\
             summary,Fees,5,money,USD,long\n\
             summary,Fees,0,money,USD,short\n\
             summary,Closed trades,5,count,,all\n\
             summary,Closed trades,5,count,,long\n\
             summary,Closed trades,0,count,,short\n\
             summary,Open trades,1,count,,all\n\
             summary,Open trades,0,count,,long\n\
             summary,Open trades,1,count,,short\n\
             summary,Winning trades,2,count,,all\n\
             summary,Winning trades,2,count,,long\n\
             summary,Winning trades,0,count,,short\n\
             summary,Losing trades,2,count,,all\n\
             summary,Losing trades,2,count,,long\n\
             summary,Losing trades,0,count,,short\n\
             summary,Even trades,1,count,,all\n\
             summary,Even trades,1,count,,long\n\
             summary,Even trades,0,count,,short\n\
             summary,Win rate,40,percent,,all\n\
             summary,Win rate,40,percent,,long\n\
             summary,Win rate,,percent,,short\n\
             summary,Average trade,8.9,money,USD,all\n\
             summary,Average trade,8.9,money,USD,long\n\
             summary,Average trade,,money,USD,short\n\
             summary,Average winning trade,38.25,money,USD,all\n\
             summary,Average winning trade,38.25,money,USD,long\n\
             summary,Average winning trade,,money,USD,short\n\
             summary,Average losing trade,16,money,USD,all\n\
             summary,Average losing trade,16,money,USD,long\n\
             summary,Average losing trade,,money,USD,short\n\
             summary,Average win / average loss,2.390625,ratio,,all\n\
             summary,Average win / average loss,2.390625,ratio,,long\n\
             summary,Average win / average loss,,ratio,,short\n\
             summary,Largest winning trade,48,money,USD,all\n\
             summary,Largest winning trade,48,money,USD,long\n\
             summary,Largest winning trade,,money,USD,short\n\
             summary,Largest winner % of gross profit,62.7450980392157,percent,,all\n\
             summary,Largest winner % of gross profit,62.7450980392157,percent,,long\n\
             summary,Largest winner % of gross profit,,percent,,short\n\
             summary,Largest losing trade,-21,money,USD,all\n\
             summary,Largest losing trade,-21,money,USD,long\n\
             summary,Largest losing trade,,money,USD,short\n\
             summary,Largest loser % of gross loss,65.625,percent,,all\n\
             summary,Largest loser % of gross loss,65.625,percent,,long\n\
             summary,Largest loser % of gross loss,,percent,,short\n\
             summary,Net profit / largest loss,2.119047619047619,ratio,,all\n\
             summary,Net profit / largest loss,2.119047619047619,ratio,,long\n\
             summary,Net profit / largest loss,,ratio,,short\n\
             summary,Consecutive wins,1,count,,all\n\
             summary,Consecutive wins,1,count,,long\n\
             summary,Consecutive wins,0,count,,short\n\
             summary,Consecutive losses,1,count,,all\n\
             summary,Consecutive losses,1,count,,long\n\
             summary,Consecutive losses,0,count,,short\n\
             summary,Largest position,7,quantity,,all\n\
             summary,Largest position,5,quantity,,long\n\
             summary,Largest position,2,quantity,,short\n\
             summary,Average bars in trades,,bars,,all\n\
             summary,Average bars in trades,,bars,,long\n\
             summary,Average bars in trades,,bars,,short\n\
             summary,Average bars in winning trades,,bars,,all\n\
             summary,Average bars in winning trades,,bars,,long\n\
             summary,Average bars in winning trades,,bars,,short\n\
             summary,Average bars in losing trades,,bars,,all\n\
             summary,Average bars in losing trades,,bars,,long\n\
             summary,Average bars in losing trades,,bars,,short\n\
             summary,Open profit,45.5,money,USD,\n\
             summary,Maximum drawdown (close-to-close),90,money,USD,\n\
             summary,Maximum drawdown % (close-to-close),0.892857142857143,percent,,\n\
             summary,Average drawdown (close-to-close),47.5,money,USD,\n\
             summary,Average drawdown duration in days (close-to-close),0.08333333333333333,days,,\n\
             summary,Maximum drawdown (intrabar),,money,USD,\n\
             summary,Maximum drawdown % (intrabar),,percent,,\n\
             summary,Maximum drawdown % of initial capital (intrabar),,percent,,\n\
             summary,Maximum run-up (close-to-close),130,money,USD,\n\
             summary,Maximum run-up % (close-to-close),1.3,percent,,\n\
             summary,Average run-up (close-to-close),70,money,USD,\n\
             summary,Average run-up duration in days (close-to-close),0.052083333333333336,days,,\n\
             summary,Maximum run-up (intrabar),,money,USD,\n\
             summary,Maximum run-up % (intrabar),,percent,,\n\
             summary,Return over maximum drawdown,0.49444444444444446,ratio,,\n\
             summary,Compound annual growth,,percent,,\n\
             summary,Sharpe ratio,,ratio,,\n\
             summary,Sortino ratio,,ratio,,\n\
             summary,Buy-and-hold return,,percent,,\n\
             summary,Buy-and-hold profit,,money,USD,\n\
             summary,Outperformance of buy-and-hold,,money,USD,\n\
             summary,Margin calls,,count,,\n\
             summary,Liquidated quantity,,quantity,,\n\
             summary,Largest liquidation,,quantity,,\n\
             summary,Average margin used,,money,USD,\n\
             summary,Largest margin used,,money,USD,\n\
             summary,Account size required,,money,USD,\n\
             summary,Return on account size,,percent,,\n\
             summary,Margin efficiency,,ratio,,\n\
             summary,Skipped trades,0,count,,\n\
             summary,Skipped equity points,0,count,,\n\
             summary,Skipped source bars,0,count,,\n\
             properties,Name,Test,text,,\n\
             properties,Symbol,TEST,text,,\n\
             properties,Interval,1h,text,,\n\
             properties,Initial capital,10000,money,USD,\n\
             properties,Currency,USD,text,,\n\
             properties,Risk-free rate,0,percent,,\n\
             properties,\"Fast, length\",\"say \"\"hi\"\"\",text,,\n"
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
            for side in [&mut p.all, &mut p.long, &mut p.short] {
                side.net_profit_fraction = Some(fraction);
                side.win_rate = Some(fraction);
                side.largest_winning_fraction = Some(fraction);
                side.largest_losing_fraction = Some(fraction);
            }
            p.compound_annual_growth = Some(fraction);
            p.max_drawdown_fraction = Some(fraction);
            p.account_size_return = Some(fraction);
            p.intrabar_max_drawdown_fraction = Some(fraction);
            p.intrabar_max_drawdown_initial_capital_fraction = Some(fraction);
            p.max_run_up_fraction = Some(fraction);
            p.intrabar_max_run_up_fraction = Some(fraction);
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
        let stats = StrategyTradeStats {
            net_profit: -0.0,
            gross_profit: -0.0,
            gross_loss: -0.0,
            total_fees: -0.0,
            profit_factor: Some(-0.0),
            average_trade: Some(-0.0),
            average_winning_trade: Some(-0.0),
            average_losing_trade: Some(-0.0),
            win_loss_ratio: Some(-0.0),
            largest_winning_trade: Some(-0.0),
            largest_losing_trade: Some(-0.0),
            net_profit_to_largest_loss: Some(-0.0),
            ..Default::default()
        };
        let p = StrategyPerformance {
            all: stats.clone(),
            long: stats.clone(),
            short: stats,
            average_margin: Some(-0.0),
            largest_margin: Some(-0.0),
            account_size_required: Some(-0.0),
            margin_efficiency: Some(-0.0),
            max_drawdown: Some(-0.0),
            average_drawdown: Some(-0.0),
            open_profit: Some(-0.0),
            intrabar_max_drawdown: Some(-0.0),
            max_run_up: Some(-0.0),
            average_run_up: Some(-0.0),
            intrabar_max_run_up: Some(-0.0),
            return_over_max_drawdown: Some(-0.0),
            sharpe_ratio: Some(-0.0),
            sortino_ratio: Some(-0.0),
            ..Default::default()
        };
        let csv = r.summary_csv(
            &p,
            &StrategyComparison {
                buy_and_hold_profit: Some(-0.0),
                buy_and_hold_outperformance: Some(-0.0),
                ..Default::default()
            },
        );
        for row in csv::Reader::from_reader(csv.as_bytes()).records() {
            let row = row.unwrap();
            if matches!(&row[3], "money" | "ratio") {
                assert_eq!(&row[2], "0", "{}", &row[1]);
            }
        }
        // Nothing to compute from: counts are zero, everything else is an empty cell.
        let empty = report(0.0);
        let csv = empty.summary_csv(&empty.performance(), &empty.comparison(&[]));
        assert!(csv.contains("summary,Win rate,,percent,,all\n"), "{csv}");
        assert!(csv.contains("summary,Net profit,0,money,,all\n"), "{csv}");
        assert!(csv.contains("properties,Currency,,text,,\n"), "{csv}");
    }
}
