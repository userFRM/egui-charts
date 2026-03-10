//! Strategy types and state management for Pine Script backtesting
//!
//! Contains trade tracking, position management, and performance metrics.

use super::types::TradeDirection;

/// A single trade in the strategy
#[derive(Debug, Clone)]
pub struct Trade {
    pub id: String,
    pub direction: TradeDirection,
    pub entry_bar: usize,
    pub entry_price: f64,
    pub quantity: f64,
    pub exit_bar: Option<usize>,
    pub exit_price: Option<f64>,
    pub profit_loss: Option<f64>,
}

/// Current position state
#[derive(Debug, Clone)]
pub struct Pos {
    pub direction: TradeDirection,
    pub quantity: f64,
    pub entry_price: f64,
    pub entry_bar: usize,
}

/// Strategy performance metrics
#[derive(Debug, Clone, Default)]
pub struct StrategyMetrics {
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub net_profit: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
}

/// Strategy state tracking
#[derive(Clone)]
pub struct StrategyState {
    /// Current open position (if any)
    pub position: Option<Pos>,
    /// Trade history
    pub trades: Vec<Trade>,
    /// Initial capital
    pub initial_capital: f64,
    /// Current equity
    pub equity: f64,
    /// Equity curve
    pub equity_curve: Vec<f64>,
    /// Computed metrics
    pub metrics: StrategyMetrics,
}

impl Default for StrategyState {
    fn default() -> Self {
        Self {
            position: None,
            trades: Vec::new(),
            initial_capital: 100_000.0,
            equity: 100_000.0,
            equity_curve: Vec::new(),
            metrics: StrategyMetrics::default(),
        }
    }
}

impl StrategyState {
    /// Create new strategy state with initial capital
    pub fn new(initial_capital: f64) -> Self {
        Self {
            initial_capital,
            equity: initial_capital,
            ..Default::default()
        }
    }

    /// Update metrics after trades
    pub fn update_metrics(&mut self) {
        let closed_trades: Vec<_> = self
            .trades
            .iter()
            .filter(|t| t.profit_loss.is_some())
            .collect();

        self.metrics.total_trades = closed_trades.len();
        self.metrics.winning_trades = closed_trades
            .iter()
            .filter(|t| t.profit_loss.unwrap_or(0.0) > 0.0)
            .count();
        self.metrics.losing_trades = closed_trades
            .iter()
            .filter(|t| t.profit_loss.unwrap_or(0.0) < 0.0)
            .count();

        self.metrics.gross_profit = closed_trades
            .iter()
            .filter_map(|t| t.profit_loss)
            .filter(|&pl| pl > 0.0)
            .sum();
        self.metrics.gross_loss = closed_trades
            .iter()
            .filter_map(|t| t.profit_loss)
            .filter(|&pl| pl < 0.0)
            .map(|pl| pl.abs())
            .sum();

        self.metrics.net_profit = self.metrics.gross_profit - self.metrics.gross_loss;

        if self.metrics.total_trades > 0 {
            self.metrics.win_rate =
                self.metrics.winning_trades as f64 / self.metrics.total_trades as f64;
        }

        if self.metrics.gross_loss > 0.0 {
            self.metrics.profit_factor = self.metrics.gross_profit / self.metrics.gross_loss;
        }

        // Calculate max drawdown from equity curve
        if !self.equity_curve.is_empty() {
            let mut peak = self.equity_curve[0];
            let mut max_dd = 0.0;
            for &equity in &self.equity_curve {
                if equity > peak {
                    peak = equity;
                }
                let drawdown = (peak - equity) / peak;
                if drawdown > max_dd {
                    max_dd = drawdown;
                }
            }
            self.metrics.max_drawdown = max_dd;
        }
    }
}

/// Plot output from indicator
#[derive(Debug, Clone)]
pub struct PlotOutput {
    pub name: String,
    pub values: Vec<f64>,
    pub color: Option<String>,
}
