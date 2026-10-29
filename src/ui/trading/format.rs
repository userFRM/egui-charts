//! The text of the numbers the trading panels show, per symbol.

use super::widgets::grouped;

/// Host text for the numbers of the order ticket, the depth ladder and the account tables.
///
/// Every method receives the symbol the number belongs to, so differently formatted instruments
/// keep their own text. Return `None` to keep the built-in text: the symbol's price decimals for
/// prices, spreads and pip values, and the decimals of the quantity step for quantities.
/// Install it with [`Trading::set_formatter`](super::Trading::set_formatter).
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

/// How the numbers of one symbol are written.
#[derive(Clone, Copy)]
pub(super) struct Numbers<'a> {
    pub symbol: &'a str,
    /// Decimals of the symbol's prices.
    pub decimals: usize,
    pub formatter: Option<&'a dyn TradingFormatter>,
}

impl Numbers<'_> {
    pub fn price(&self, price: f64) -> String {
        self.formatter
            .and_then(|formatter| formatter.price(self.symbol, price))
            .unwrap_or_else(|| grouped(price, self.decimals, false))
    }

    /// `quantity` at `decimals`, unless the formatter writes it.
    pub fn quantity(&self, quantity: f64, decimals: usize) -> String {
        self.formatter
            .and_then(|formatter| formatter.quantity(self.symbol, quantity))
            .unwrap_or_else(|| grouped(quantity, decimals, false))
    }

    pub fn spread(&self, spread: f64) -> String {
        self.formatter
            .and_then(|formatter| formatter.spread(self.symbol, spread))
            .unwrap_or_else(|| grouped(spread, self.decimals, false))
    }

    pub fn pip_value(&self, value: f64) -> String {
        self.formatter
            .and_then(|formatter| formatter.pip_value(self.symbol, value))
            .unwrap_or_else(|| grouped(value, self.decimals.max(2), false))
    }
}
