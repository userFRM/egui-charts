//! Strategy execution methods for Pine Script runtime
//!
//! Handles strategy.* methods for backtesting functionality.

use super::Runtime;
use crate::scripting::strategy::{Pos, Trade};
use crate::scripting::types::{RuntimeError, TradeDirection, Value};

impl Runtime {
    /// Handle strategy.* methods for backtesting
    pub(crate) fn call_strategy_method(
        &mut self,
        method: &str,
        args: &[Value],
    ) -> Result<Value, RuntimeError> {
        match method {
            // strategy.entry(id, direction, qty, limit, stop, comment)
            "entry" => {
                let id = if let Some(Value::String(s)) = args.first() {
                    s.clone()
                } else {
                    format!("trade_{}", self.strategy.trades.len())
                };

                // Direction: "long" or "short" (second arg), or strategy.long / strategy.short constant
                let direction = args
                    .get(1)
                    .map(|v| match v {
                        Value::String(s) if s.to_lowercase() == "long" => TradeDirection::Long,
                        Value::String(s) if s.to_lowercase() == "short" => TradeDirection::Short,
                        Value::Number(n) if *n > 0.0 => TradeDirection::Long, // strategy.long = 1
                        Value::Number(n) if *n < 0.0 => TradeDirection::Short, // strategy.short = -1
                        _ => TradeDirection::Long,
                    })
                    .unwrap_or(TradeDirection::Long);

                // Quantity (default 1.0)
                let qty = args.get(2).and_then(|v| v.as_num().ok()).unwrap_or(1.0);

                // Get current price
                let curr_price = self
                    .context
                    .bars
                    .get(self.context.curr_bar)
                    .map(|b| b.close)
                    .unwrap_or(0.0);

                // Close opposite position if exists
                if let Some(pos) = &self.strategy.position
                    && pos.direction != direction
                {
                    // Close existing position
                    self.close_pos(curr_price, Some(&id))?;
                }

                // Open new position if not already in same direction
                if self.strategy.position.is_none() {
                    self.strategy.position = Some(Pos {
                        direction,
                        quantity: qty,
                        entry_price: curr_price,
                        entry_bar: self.context.curr_bar,
                    });

                    self.strategy.trades.push(Trade {
                        id,
                        direction,
                        entry_bar: self.context.curr_bar,
                        entry_price: curr_price,
                        quantity: qty,
                        exit_bar: None,
                        exit_price: None,
                        profit_loss: None,
                    });
                }

                Ok(Value::Null)
            }

            // strategy.exit(id, from_entry, qty, limit, stop, comment)
            "exit" => {
                if self.strategy.position.is_none() {
                    return Ok(Value::Null);
                }

                let id = if let Some(Value::String(s)) = args.first() {
                    Some(s.clone())
                } else {
                    None
                };

                let curr_price = self
                    .context
                    .bars
                    .get(self.context.curr_bar)
                    .map(|b| b.close)
                    .unwrap_or(0.0);

                self.close_pos(curr_price, id.as_deref())?;

                Ok(Value::Null)
            }

            // strategy.close(id, comment, immediately)
            "close" => {
                if self.strategy.position.is_none() {
                    return Ok(Value::Null);
                }

                let curr_price = self
                    .context
                    .bars
                    .get(self.context.curr_bar)
                    .map(|b| b.close)
                    .unwrap_or(0.0);

                self.close_pos(curr_price, None)?;

                Ok(Value::Null)
            }

            // strategy.close_all(comment, immediately)
            "close_all" => {
                if self.strategy.position.is_some() {
                    let curr_price = self
                        .context
                        .bars
                        .get(self.context.curr_bar)
                        .map(|b| b.close)
                        .unwrap_or(0.0);

                    self.close_pos(curr_price, None)?;
                }

                Ok(Value::Null)
            }

            // strategy.pos_size - Returns current position size
            "pos_size" => {
                let size = self
                    .strategy
                    .position
                    .as_ref()
                    .map(|p| {
                        if p.direction == TradeDirection::Long {
                            p.quantity
                        } else {
                            -p.quantity
                        }
                    })
                    .unwrap_or(0.0);
                Ok(Value::Number(size))
            }

            // strategy.pos_avg_price - Avg entry price
            "pos_avg_price" => {
                let price = self
                    .strategy
                    .position
                    .as_ref()
                    .map(|p| p.entry_price)
                    .unwrap_or(0.0);
                Ok(Value::Number(price))
            }

            // strategy.netprofit - Net profit
            "netprofit" => Ok(Value::Number(self.strategy.metrics.net_profit)),

            // strategy.grossprofit
            "grossprofit" => Ok(Value::Number(self.strategy.metrics.gross_profit)),

            // strategy.grossloss
            "grossloss" => Ok(Value::Number(self.strategy.metrics.gross_loss)),

            // strategy.wintrades
            "wintrades" => Ok(Value::Number(self.strategy.metrics.winning_trades as f64)),

            // strategy.losstrades
            "losstrades" => Ok(Value::Number(self.strategy.metrics.losing_trades as f64)),

            // strategy.closedtrades
            "closedtrades" => {
                let closed = self
                    .strategy
                    .trades
                    .iter()
                    .filter(|t| t.exit_bar.is_some())
                    .count();
                Ok(Value::Number(closed as f64))
            }

            // strategy.opentrades
            "opentrades" => {
                let count = if self.strategy.position.is_some() {
                    1.0
                } else {
                    0.0
                };
                Ok(Value::Number(count))
            }

            // strategy.equity
            "equity" => {
                // Calculate current equity including open position P&L
                let mut equity = self.strategy.equity;

                if let Some(pos) = &self.strategy.position {
                    let curr_price = self
                        .context
                        .bars
                        .get(self.context.curr_bar)
                        .map(|b| b.close)
                        .unwrap_or(pos.entry_price);

                    let unrealized_pnl = match pos.direction {
                        TradeDirection::Long => (curr_price - pos.entry_price) * pos.quantity,
                        TradeDirection::Short => (pos.entry_price - curr_price) * pos.quantity,
                    };
                    equity += unrealized_pnl;
                }

                Ok(Value::Number(equity))
            }

            // strategy.initial_capital
            "initial_capital" => Ok(Value::Number(self.strategy.initial_capital)),

            // strategy.cancel(id) - Cancel a pending order by ID
            "cancel" => {
                // In the simplified runtime, orders fill immediately so cancel is a no-op
                Ok(Value::Null)
            }

            // strategy.cancel_all() - Cancel all pending orders
            "cancel_all" => Ok(Value::Null),

            // Constants
            "long" => Ok(Value::Number(1.0)),
            "short" => Ok(Value::Number(-1.0)),

            _ => Err(RuntimeError::FunctionNotFound(format!("strategy.{method}"))),
        }
    }

    /// Close current position and calculate P&L
    pub(crate) fn close_pos(
        &mut self,
        exit_price: f64,
        trade_id: Option<&str>,
    ) -> Result<(), RuntimeError> {
        if let Some(pos) = self.strategy.position.take() {
            // Calculate profit/loss
            let pnl = match pos.direction {
                TradeDirection::Long => (exit_price - pos.entry_price) * pos.quantity,
                TradeDirection::Short => (pos.entry_price - exit_price) * pos.quantity,
            };

            // Update equity
            self.strategy.equity += pnl;
            self.strategy.equity_curve.push(self.strategy.equity);

            // Update the last matching trade
            for trade in self.strategy.trades.iter_mut().rev() {
                if trade.exit_bar.is_none() {
                    // Check if trade_id matches (if provided)
                    if trade_id.is_none()
                        || trade.id == trade_id.unwrap_or("")
                        || trade_id == Some("")
                    {
                        trade.exit_bar = Some(self.context.curr_bar);
                        trade.exit_price = Some(exit_price);
                        trade.profit_loss = Some(pnl);
                        break;
                    }
                }
            }

            // Update metrics
            self.strategy.metrics.total_trades += 1;
            self.strategy.metrics.net_profit += pnl;

            if pnl > 0.0 {
                self.strategy.metrics.winning_trades += 1;
                self.strategy.metrics.gross_profit += pnl;
            } else {
                self.strategy.metrics.losing_trades += 1;
                self.strategy.metrics.gross_loss += pnl.abs();
            }

            // Update win rate
            if self.strategy.metrics.total_trades > 0 {
                self.strategy.metrics.win_rate = self.strategy.metrics.winning_trades as f64
                    / self.strategy.metrics.total_trades as f64;
            }

            // Update profit factor
            if self.strategy.metrics.gross_loss > 0.0 {
                self.strategy.metrics.profit_factor =
                    self.strategy.metrics.gross_profit / self.strategy.metrics.gross_loss;
            }

            // Update max drawdown
            let peak = self
                .strategy
                .equity_curve
                .iter()
                .fold(self.strategy.initial_capital, |max, &e| f64::max(max, e));
            let curr_drawdown = peak - self.strategy.equity;
            self.strategy.metrics.max_drawdown =
                f64::max(self.strategy.metrics.max_drawdown, curr_drawdown);
        }

        Ok(())
    }
}
