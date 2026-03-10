use super::portfolio::Portfolio;
use super::trade::{Trade, TradeStatus};

/// Comprehensive performance metrics
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    // Return metrics
    pub total_return: f64,
    pub total_return_pct: f64,
    pub annualized_return: f64,
    pub cagr: f64,

    // Risk metrics
    pub volatility: f64,
    pub annualized_volatility: f64,
    pub max_drawdown: f64,
    pub max_drawdown_duration_days: f64,
    pub avg_drawdown: f64,

    // Risk-adjusted returns
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub calmar_ratio: f64,
    pub omega_ratio: f64,

    // Trade statistics
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub win_rate: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub largest_win: f64,
    pub largest_loss: f64,
    pub avg_trade: f64,
    pub profit_factor: f64,
    pub expectancy: f64,
    pub payoff_ratio: f64,

    // Time metrics
    pub avg_trade_duration_bars: f64,
    pub avg_winning_duration_bars: f64,
    pub avg_losing_duration_bars: f64,
    pub percent_time_in_market: f64,

    // Streak metrics
    pub max_consecutive_wins: usize,
    pub max_consecutive_losses: usize,
    pub curr_streak: isize,

    // MAE/MFE
    pub avg_mae: f64,
    pub avg_mfe: f64,
    pub mae_mfe_ratio: f64,

    // Recovery
    pub recovery_factor: f64,
    pub ulcer_idx: f64,

    // Portfolio metrics
    pub final_equity: f64,
    pub peak_equity: f64,
    pub total_commission: f64,
    pub total_slippage: f64,
}

impl PerformanceMetrics {
    /// Calculate all metrics from portfolio
    pub fn calculate(portfolio: &Portfolio, risk_free_rate: f64, trading_days: usize) -> Self {
        let closed_trades: Vec<&Trade> = portfolio
            .trades
            .iter()
            .filter(|t| t.status == TradeStatus::Closed)
            .collect();

        let mut metrics = Self::default();

        // Basic portfolio metrics
        metrics.final_equity = portfolio.equity();
        metrics.peak_equity = portfolio.peak_equity;
        metrics.max_drawdown = portfolio.max_drawdown;

        // Return metrics
        metrics.total_return = metrics.final_equity - portfolio.initial_capital;
        metrics.total_return_pct = if portfolio.initial_capital > 0.0 {
            (metrics.total_return / portfolio.initial_capital) * 100.0
        } else {
            0.0
        };

        // Annualized return
        let years = trading_days as f64 / 252.0;
        if years > 0.0 {
            metrics.cagr = ((metrics.final_equity / portfolio.initial_capital).powf(1.0 / years)
                - 1.0)
                * 100.0;
            metrics.annualized_return = metrics.cagr;
        }

        // Trade statistics
        metrics.total_trades = closed_trades.len();
        if metrics.total_trades == 0 {
            return metrics;
        }

        let winners: Vec<&&Trade> = closed_trades.iter().filter(|t| t.pnl > 0.0).collect();
        let losers: Vec<&&Trade> = closed_trades.iter().filter(|t| t.pnl <= 0.0).collect();

        metrics.winning_trades = winners.len();
        metrics.losing_trades = losers.len();
        metrics.win_rate = (metrics.winning_trades as f64 / metrics.total_trades as f64) * 100.0;

        // Win/Loss avgs
        if !winners.is_empty() {
            metrics.avg_win = winners.iter().map(|t| t.pnl).sum::<f64>() / winners.len() as f64;
            metrics.largest_win = winners
                .iter()
                .map(|t| t.pnl)
                .fold(f64::NEG_INFINITY, f64::max);
        }

        if !losers.is_empty() {
            metrics.avg_loss = losers.iter().map(|t| t.pnl).sum::<f64>() / losers.len() as f64;
            metrics.largest_loss = losers.iter().map(|t| t.pnl).fold(f64::INFINITY, f64::min);
        }

        // Avg trade
        metrics.avg_trade =
            closed_trades.iter().map(|t| t.pnl).sum::<f64>() / metrics.total_trades as f64;

        // Profit factor
        let gross_profit: f64 = winners.iter().map(|t| t.pnl).sum();
        let gross_loss: f64 = losers.iter().map(|t| t.pnl.abs()).sum();
        metrics.profit_factor = if gross_loss > 0.0 {
            gross_profit / gross_loss
        } else if gross_profit > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };

        // Payoff ratio
        metrics.payoff_ratio = if metrics.avg_loss.abs() > 0.0 {
            metrics.avg_win / metrics.avg_loss.abs()
        } else {
            f64::INFINITY
        };

        // Expectancy
        let win_rate_decimal = metrics.win_rate / 100.0;
        metrics.expectancy =
            (win_rate_decimal * metrics.avg_win) + ((1.0 - win_rate_decimal) * metrics.avg_loss);

        // Duration metrics
        let durations: Vec<usize> = closed_trades.iter().map(|t| t.bars_held).collect();
        if !durations.is_empty() {
            metrics.avg_trade_duration_bars =
                durations.iter().sum::<usize>() as f64 / durations.len() as f64;
        }

        let winner_durations: Vec<usize> = winners.iter().map(|t| t.bars_held).collect();
        if !winner_durations.is_empty() {
            metrics.avg_winning_duration_bars =
                winner_durations.iter().sum::<usize>() as f64 / winner_durations.len() as f64;
        }

        let loser_durations: Vec<usize> = losers.iter().map(|t| t.bars_held).collect();
        if !loser_durations.is_empty() {
            metrics.avg_losing_duration_bars =
                loser_durations.iter().sum::<usize>() as f64 / loser_durations.len() as f64;
        }

        // Streak metrics
        let (max_wins, max_losses, current) = Self::calculate_streaks(&closed_trades);
        metrics.max_consecutive_wins = max_wins;
        metrics.max_consecutive_losses = max_losses;
        metrics.curr_streak = current;

        // MAE/MFE
        let maes: Vec<f64> = closed_trades.iter().map(|t| t.mae).collect();
        let mfes: Vec<f64> = closed_trades.iter().map(|t| t.mfe).collect();

        if !maes.is_empty() {
            metrics.avg_mae = maes.iter().sum::<f64>() / maes.len() as f64;
        }
        if !mfes.is_empty() {
            metrics.avg_mfe = mfes.iter().sum::<f64>() / mfes.len() as f64;
        }
        if metrics.avg_mfe.abs() > 0.0 {
            metrics.mae_mfe_ratio = metrics.avg_mae.abs() / metrics.avg_mfe;
        }

        // Volatility from equity curve
        if portfolio.equity_curve.len() > 1 {
            let returns = Self::calculate_returns(&portfolio.equity_curve);
            metrics.volatility = Self::std_dev(&returns);
            metrics.annualized_volatility = metrics.volatility * (252.0_f64).sqrt();

            // Sharpe ratio
            let excess_return = metrics.annualized_return - risk_free_rate;
            if metrics.annualized_volatility > 0.0 {
                metrics.sharpe_ratio = excess_return / metrics.annualized_volatility;
            }

            // Sortino ratio (downside deviation)
            let downside_returns: Vec<f64> =
                returns.iter().filter(|&&r| r < 0.0).copied().collect();
            if !downside_returns.is_empty() {
                let downside_dev = Self::std_dev(&downside_returns);
                if downside_dev > 0.0 {
                    metrics.sortino_ratio = excess_return / (downside_dev * (252.0_f64).sqrt());
                }
            }

            // Ulcer index
            metrics.ulcer_idx = Self::calculate_ulcer_idx(&portfolio.equity_curve);
        }

        // Calmar ratio
        if metrics.max_drawdown > 0.0 {
            metrics.calmar_ratio = metrics.annualized_return / metrics.max_drawdown;
        }

        // Recovery factor
        if metrics.max_drawdown > 0.0 {
            let dd_amount = (portfolio.peak_equity * metrics.max_drawdown) / 100.0;
            if dd_amount > 0.0 {
                metrics.recovery_factor = metrics.total_return / dd_amount;
            }
        }

        // Commission and slippage
        metrics.total_commission = closed_trades.iter().map(|t| t.commission).sum();
        metrics.total_slippage = closed_trades.iter().map(|t| t.slippage).sum();

        metrics
    }

    fn calculate_returns(equity_curve: &[(chrono::DateTime<chrono::Utc>, f64)]) -> Vec<f64> {
        let mut returns = Vec::with_capacity(equity_curve.len() - 1);

        for i in 1..equity_curve.len() {
            let prev = equity_curve[i - 1].1;
            let curr = equity_curve[i].1;
            if prev > 0.0 {
                returns.push((curr - prev) / prev);
            }
        }

        returns
    }

    fn std_dev(values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }

        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        variance.sqrt()
    }

    fn calculate_streaks(trades: &[&Trade]) -> (usize, usize, isize) {
        let mut max_wins = 0;
        let mut max_losses = 0;
        let mut curr_wins = 0;
        let mut curr_losses = 0;

        for trade in trades {
            if trade.pnl > 0.0 {
                curr_wins += 1;
                curr_losses = 0;
                max_wins = max_wins.max(curr_wins);
            } else {
                curr_losses += 1;
                curr_wins = 0;
                max_losses = max_losses.max(curr_losses);
            }
        }

        let current = if curr_wins > 0 {
            curr_wins as isize
        } else {
            -(curr_losses as isize)
        };

        (max_wins, max_losses, current)
    }

    fn calculate_ulcer_idx(equity_curve: &[(chrono::DateTime<chrono::Utc>, f64)]) -> f64 {
        if equity_curve.is_empty() {
            return 0.0;
        }

        let mut peak = equity_curve[0].1;
        let mut squared_dd_sum = 0.0;

        for &(_, equity) in equity_curve.iter() {
            peak = peak.max(equity);
            let dd_pct = if peak > 0.0 {
                ((peak - equity) / peak) * 100.0
            } else {
                0.0
            };
            squared_dd_sum += dd_pct.powi(2);
        }

        (squared_dd_sum / equity_curve.len() as f64).sqrt()
    }

    /// Format metrics as a report string
    pub fn report(&self) -> String {
        format!(
            r#"=== Performance Report ===

Returns:
  Total Return: ${:.2} ({:.2}%)
  CAGR: {:.2}%

Risk:
  Max Drawdown: {:.2}%
  Volatility (Ann.): {:.2}%

Risk-Adjusted:
  Sharpe Ratio: {:.2}
  Sortino Ratio: {:.2}
  Calmar Ratio: {:.2}

Trade Statistics:
  Total Trades: {}
  Win Rate: {:.2}%
  Profit Factor: {:.2}
  Avg Trade: ${:.2}
  Expectancy: ${:.2}

  Avg Win: ${:.2}
  Avg Loss: ${:.2}
  Largest Win: ${:.2}
  Largest Loss: ${:.2}

Streaks:
  Max Consecutive Wins: {}
  Max Consecutive Losses: {}

Costs:
  Total Commission: ${:.2}
  Total Slippage: ${:.2}

Final Equity: ${:.2}
"#,
            self.total_return,
            self.total_return_pct,
            self.cagr,
            self.max_drawdown,
            self.annualized_volatility,
            self.sharpe_ratio,
            self.sortino_ratio,
            self.calmar_ratio,
            self.total_trades,
            self.win_rate,
            self.profit_factor,
            self.avg_trade,
            self.expectancy,
            self.avg_win,
            self.avg_loss,
            self.largest_win,
            self.largest_loss,
            self.max_consecutive_wins,
            self.max_consecutive_losses,
            self.total_commission,
            self.total_slippage,
            self.final_equity,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_std_dev() {
        let values = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let std = PerformanceMetrics::std_dev(&values);
        assert!((std - 2.0).abs() < 0.1);
    }

    #[test]
    fn test_streaks() {
        use super::super::trade::{TradeSide, TradeStatus};
        use chrono::Utc;

        let mut trades = Vec::new();

        // WWWLLWW pattern
        let pnls = [100.0, 50.0, 75.0, -30.0, -20.0, 80.0, 90.0];
        for (i, &pnl) in pnls.iter().enumerate() {
            let mut trade = Trade::new(i, "TEST".into(), TradeSide::Long, 100.0, 10.0, Utc::now());
            trade.pnl = pnl;
            trade.status = TradeStatus::Closed;
            trades.push(trade);
        }

        let trade_refs: Vec<&Trade> = trades.iter().collect();
        let (max_wins, max_losses, current) = PerformanceMetrics::calculate_streaks(&trade_refs);

        assert_eq!(max_wins, 3);
        assert_eq!(max_losses, 2);
        assert_eq!(current, 2); // Current winning streak
    }

    #[test]
    fn test_metrics_calculation() {
        let portfolio = Portfolio::new(100_000.0);
        let metrics = PerformanceMetrics::calculate(&portfolio, 0.0, 252);

        assert!((metrics.final_equity - 100_000.0).abs() < 0.01);
        assert_eq!(metrics.total_trades, 0);
    }
}
