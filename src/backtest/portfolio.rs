use super::trade::{Trade, TradeSide, TradeStatus};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Pos direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosSide {
    Long,
    Short,
    Flat,
}

/// A position in a security
#[derive(Debug, Clone)]
pub struct Pos {
    pub symbol: String,
    pub side: PosSide,
    pub quantity: f64,
    pub avg_entry_price: f64,
    pub curr_price: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub trade_cnt: usize,
}

impl Pos {
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            side: PosSide::Flat,
            quantity: 0.0,
            avg_entry_price: 0.0,
            curr_price: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            trade_cnt: 0,
        }
    }

    /// Add to position
    pub fn add(&mut self, quantity: f64, price: f64, side: TradeSide) {
        let new_side = match side {
            TradeSide::Long => PosSide::Long,
            TradeSide::Short => PosSide::Short,
        };

        if self.side == PosSide::Flat {
            // New position
            self.side = new_side;
            self.quantity = quantity;
            self.avg_entry_price = price;
        } else if self.side == new_side {
            // Adding to existing position
            let total_cost = self.avg_entry_price * self.quantity + price * quantity;
            self.quantity += quantity;
            self.avg_entry_price = total_cost / self.quantity;
        } else {
            // Reducing opposite position
            if quantity >= self.quantity {
                // Closing and potentially reversing
                let closed_qty = self.quantity;
                self.realize_pnl(closed_qty, price);

                let remaining = quantity - closed_qty;
                if remaining > 0.0 {
                    self.side = new_side;
                    self.quantity = remaining;
                    self.avg_entry_price = price;
                } else {
                    self.side = PosSide::Flat;
                    self.quantity = 0.0;
                    self.avg_entry_price = 0.0;
                }
            } else {
                // Partial close
                self.realize_pnl(quantity, price);
                self.quantity -= quantity;
            }
        }

        self.trade_cnt += 1;
        self.update_unrealized(price);
    }

    /// Update unrealized P&L
    pub fn update_unrealized(&mut self, curr_price: f64) {
        self.curr_price = curr_price;

        if self.side == PosSide::Flat {
            self.unrealized_pnl = 0.0;
        } else {
            let price_diff = curr_price - self.avg_entry_price;
            self.unrealized_pnl = match self.side {
                PosSide::Long => price_diff * self.quantity,
                PosSide::Short => -price_diff * self.quantity,
                PosSide::Flat => 0.0,
            };
        }
    }

    /// Realize P&L from closing position
    fn realize_pnl(&mut self, quantity: f64, price: f64) {
        let price_diff = price - self.avg_entry_price;
        let pnl = match self.side {
            PosSide::Long => price_diff * quantity,
            PosSide::Short => -price_diff * quantity,
            PosSide::Flat => 0.0,
        };
        self.realized_pnl += pnl;
    }

    /// Get total P&L
    pub fn total_pnl(&self) -> f64 {
        self.realized_pnl + self.unrealized_pnl
    }

    /// Get market value of position
    pub fn market_val(&self) -> f64 {
        self.quantity * self.curr_price
    }

    /// Check if position is flat
    pub fn is_flat(&self) -> bool {
        self.side == PosSide::Flat || self.quantity.abs() < 1e-10
    }
}

/// Portfolio tracking
#[derive(Debug, Clone)]
pub struct Portfolio {
    pub cash: f64,
    pub initial_capital: f64,
    pub positions: HashMap<String, Pos>,
    pub trades: Vec<Trade>,
    pub equity_curve: Vec<(DateTime<Utc>, f64)>,
    pub drawdown_curve: Vec<(DateTime<Utc>, f64)>,
    pub peak_equity: f64,
    pub curr_drawdown: f64,
    pub max_drawdown: f64,
    next_trade_id: usize,
}

impl Portfolio {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            cash: initial_capital,
            initial_capital,
            positions: HashMap::new(),
            trades: Vec::new(),
            equity_curve: Vec::new(),
            drawdown_curve: Vec::new(),
            peak_equity: initial_capital,
            curr_drawdown: 0.0,
            max_drawdown: 0.0,
            next_trade_id: 0,
        }
    }

    /// Get current equity (cash + positions)
    pub fn equity(&self) -> f64 {
        let positions_val: f64 = self.positions.values().map(|p| p.market_val()).sum();
        self.cash + positions_val
    }

    /// Get total unrealized P&L
    pub fn unrealized_pnl(&self) -> f64 {
        self.positions.values().map(|p| p.unrealized_pnl).sum()
    }

    /// Get total realized P&L
    pub fn realized_pnl(&self) -> f64 {
        self.positions.values().map(|p| p.realized_pnl).sum()
    }

    /// Open a new trade
    pub fn open_trade(
        &mut self,
        symbol: &str,
        side: TradeSide,
        price: f64,
        quantity: f64,
        ts: DateTime<Utc>,
        commission: f64,
        slippage: f64,
    ) -> Option<usize> {
        let fill_price = match side {
            TradeSide::Long => price + slippage,
            TradeSide::Short => price - slippage,
        };

        let trade_val = fill_price * quantity;
        let total_cost = trade_val + commission;

        // Check if we have enough cash
        if total_cost > self.cash && matches!(side, TradeSide::Long) {
            return None;
        }

        // Update cash
        match side {
            TradeSide::Long => self.cash -= total_cost,
            TradeSide::Short => self.cash += trade_val - commission, // Receive short proceeds
        }

        // Update position
        let position = self
            .positions
            .entry(symbol.to_string())
            .or_insert_with(|| Pos::new(symbol));
        position.add(quantity, fill_price, side);

        // Create trade record
        let trade_id = self.next_trade_id;
        self.next_trade_id += 1;

        let mut trade = Trade::new(trade_id, symbol.to_string(), side, fill_price, quantity, ts);
        trade.commission = commission;
        trade.slippage = slippage;

        self.trades.push(trade);

        Some(trade_id)
    }

    /// Close a trade
    pub fn close_trade(
        &mut self,
        symbol: &str,
        price: f64,
        quantity: f64,
        ts: DateTime<Utc>,
        commission: f64,
        slippage: f64,
    ) -> bool {
        if let Some(position) = self.positions.get_mut(symbol) {
            if position.is_flat() {
                return false;
            }

            // Determine close side (opposite of position)
            let close_side = match position.side {
                PosSide::Long => TradeSide::Short,
                PosSide::Short => TradeSide::Long,
                PosSide::Flat => return false,
            };

            let fill_price = match close_side {
                TradeSide::Long => price + slippage,
                TradeSide::Short => price - slippage,
            };

            let close_qty = quantity.min(position.quantity);

            // Update cash
            match close_side {
                TradeSide::Short => {
                    self.cash += fill_price * close_qty - commission;
                }
                TradeSide::Long => {
                    self.cash -= fill_price * close_qty + commission;
                }
            }

            position.add(close_qty, fill_price, close_side);

            // Find and close the corresponding trade
            for trade in self.trades.iter_mut().rev() {
                if trade.symbol == symbol && trade.status == TradeStatus::Open {
                    trade.commission += commission;
                    trade.close(fill_price, ts);
                    break;
                }
            }

            true
        } else {
            false
        }
    }

    /// Update all positions with current prices
    pub fn update_prices(&mut self, prices: &HashMap<String, f64>, ts: DateTime<Utc>) {
        for (symbol, position) in self.positions.iter_mut() {
            if let Some(&price) = prices.get(symbol) {
                position.update_unrealized(price);
            }
        }

        // Update equity curve
        let equity = self.equity();
        self.equity_curve.push((ts, equity));

        // Update drawdown
        if equity > self.peak_equity {
            self.peak_equity = equity;
        }

        self.curr_drawdown = if self.peak_equity > 0.0 {
            (self.peak_equity - equity) / self.peak_equity * 100.0
        } else {
            0.0
        };

        if self.curr_drawdown > self.max_drawdown {
            self.max_drawdown = self.curr_drawdown;
        }

        self.drawdown_curve.push((ts, self.curr_drawdown));
    }

    /// Get position for a symbol
    pub fn get_pos(&self, symbol: &str) -> Option<&Pos> {
        self.positions.get(symbol)
    }

    /// Check if we have a position in a symbol
    pub fn has_pos(&self, symbol: &str) -> bool {
        self.positions
            .get(symbol)
            .map(|p| !p.is_flat())
            .unwrap_or(false)
    }

    /// Get all closed trades
    pub fn closed_trades(&self) -> Vec<&Trade> {
        self.trades
            .iter()
            .filter(|t| t.status == TradeStatus::Closed)
            .collect()
    }

    /// Get all open trades
    pub fn open_trades(&self) -> Vec<&Trade> {
        self.trades
            .iter()
            .filter(|t| t.status == TradeStatus::Open)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portfolio_creation() {
        let portfolio = Portfolio::new(100_000.0);
        assert!((portfolio.cash - 100_000.0).abs() < 0.01);
        assert!((portfolio.equity() - 100_000.0).abs() < 0.01);
    }

    #[test]
    fn test_open_long_trade() {
        let mut portfolio = Portfolio::new(100_000.0);
        let result =
            portfolio.open_trade("AAPL", TradeSide::Long, 150.0, 100.0, Utc::now(), 10.0, 0.0);

        assert!(result.is_some());
        assert!(portfolio.has_pos("AAPL"));

        let position = portfolio.get_pos("AAPL").unwrap();
        assert_eq!(position.side, PosSide::Long);
        assert!((position.quantity - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_close_trade() {
        let mut portfolio = Portfolio::new(100_000.0);
        portfolio.open_trade("AAPL", TradeSide::Long, 100.0, 100.0, Utc::now(), 0.0, 0.0);

        let closed = portfolio.close_trade("AAPL", 110.0, 100.0, Utc::now(), 0.0, 0.0);
        assert!(closed);

        // Should have made profit
        assert!(portfolio.equity() > 100_000.0);
    }

    #[test]
    fn test_pos_averaging() {
        let mut position = Pos::new("AAPL");

        // Buy 100 at 100
        position.add(100.0, 100.0, TradeSide::Long);
        assert!((position.avg_entry_price - 100.0).abs() < 0.01);

        // Buy 100 more at 110
        position.add(100.0, 110.0, TradeSide::Long);
        assert!((position.avg_entry_price - 105.0).abs() < 0.01);
        assert!((position.quantity - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_drawdown_tracking() {
        let mut portfolio = Portfolio::new(100_000.0);
        let mut prices = HashMap::new();

        prices.insert("AAPL".to_string(), 100.0);
        portfolio.open_trade("AAPL", TradeSide::Long, 100.0, 100.0, Utc::now(), 0.0, 0.0);
        portfolio.update_prices(&prices, Utc::now());

        // Price drops 10%
        prices.insert("AAPL".to_string(), 90.0);
        portfolio.update_prices(&prices, Utc::now());

        assert!(portfolio.curr_drawdown > 0.0);
    }
}
