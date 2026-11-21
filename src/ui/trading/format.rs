//! The text of the numbers the trading panels show, per symbol.

use super::widgets::grouped;

pub use crate::chart::TradingFormatter;

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
