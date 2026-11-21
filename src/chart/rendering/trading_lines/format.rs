//! Host and built-in number text shared by trading lines and panels.

use crate::scales::{CurrencyFormatter, PriceFormatter};

/// Host text for the numbers of the order ticket, the depth ladder, the account tables, fill
/// marks, the order and fill notifications, and the quantities of the order, bracket and
/// position lines on the chart.
///
/// Every method receives the symbol the number belongs to, so differently formatted instruments
/// keep their own text. Return `None` to keep the built-in text: the symbol's price decimals for
/// prices, spreads and pip values, and for quantities the decimals of the quantity step in the
/// ticket and the depth ladder, or the quantity's own decimals, up to eight, in the account
/// tables, fill marks, notifications, confirmations and trading lines.
/// Install it with `ui::trading::Trading::set_formatter`, which also passes it to the trading
/// lines `Trading` draws, or set [`TradingLineOptions::formatter`](crate::chart::TradingLineOptions::formatter)
/// when calling [`render_trading_lines`](crate::chart::render_trading_lines) directly.
///
/// ```
/// use egui_charts::chart::{TradingFormatter, TradingLineOptions};
///
/// struct Lots;
/// impl TradingFormatter for Lots {
///     fn quantity(&self, _symbol: &str, quantity: f64) -> Option<String> {
///         Some(format!("{} lots", quantity / 100_000.0))
///     }
/// }
///
/// let mut options = TradingLineOptions::new("USD", 0.00001);
/// options.formatter = Some(&Lots);
/// ```
pub trait TradingFormatter: Send + Sync {
    /// A price, such as a limit price, a bid or an average price.
    fn price(&self, _symbol: &str, _price: f64) -> Option<String> {
        None
    }

    /// A quantity, in the symbol's trading units.
    fn quantity(&self, _symbol: &str, _quantity: f64) -> Option<String> {
        None
    }

    /// The difference between the ask and the bid, in price units.
    fn spread(&self, _symbol: &str, _spread: f64) -> Option<String> {
        None
    }

    /// The value of one pip of the order in the account's currency.
    fn pip_value(&self, _symbol: &str, _value: f64) -> Option<String> {
        None
    }
}

impl std::fmt::Debug for dyn TradingFormatter + '_ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TradingFormatter")
    }
}

/// `value` with thousands separators and `decimals` decimals, or a dash when not finite. With
/// `sign`, a value that shows a nonzero digit carries an explicit sign; one that rounds to zero
/// is unsigned either way.
pub(crate) fn grouped(value: f64, decimals: usize, sign: bool) -> String {
    if !value.is_finite() {
        return "-".to_owned();
    }
    let magnitude = CurrencyFormatter::new("", decimals).format(value.abs());
    let shown = magnitude.bytes().any(|b| matches!(b, b'1'..=b'9'));
    match (shown, value < 0.0, sign) {
        (true, true, _) => format!("-{magnitude}"),
        (true, false, true) => format!("+{magnitude}"),
        _ => magnitude,
    }
}

/// Decimals that show `quantity` without rounding, up to 8.
pub(crate) fn quantity_decimals(quantity: f64) -> usize {
    crate::scales::decimals(quantity).min(8)
}

/// `quantity` of `symbol` in the host's text, else at its own decimals, up to eight,
/// with thousands separators, as the account tables write it.
pub(crate) fn quantity_text(
    formatter: Option<&dyn TradingFormatter>,
    symbol: &str,
    quantity: f64,
) -> String {
    formatter
        .and_then(|formatter| formatter.quantity(symbol, quantity))
        .unwrap_or_else(|| grouped(quantity, quantity_decimals(quantity), false))
}

/// `quantity` at 15 significant digits, the most an f64 writes faithfully, so a computed
/// 0.3 - 0.1 reaches the host's formatter as 0.2, not 0.19999999999999998.
pub(super) fn settled(quantity: f64) -> f64 {
    format!("{quantity:.14e}").parse().unwrap_or(quantity)
}
