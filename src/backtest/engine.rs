use super::config::BacktestConfig;
use super::metrics::PerformanceMetrics;
use super::portfolio::Portfolio;
use super::strategy::{Signal, SignalType, Strategy, StrategyContext};
use super::trade::TradeSide;
use crate::model::Bar;
use std::collections::HashMap;

/// Result of a backtest run
#[derive(Debug)]
pub struct BacktestResult {
    /// Final portfolio state
    pub portfolio: Portfolio,
    /// Performance metrics
    pub metrics: PerformanceMetrics,
    /// Strategy name
    pub strategy_name: String,
    /// Strategy params
    pub strategy_params: Vec<(String, String)>,
    /// Configuration used
    pub config: BacktestConfig,
    /// Number of bars processed
    pub bars_processed: usize,
    /// Signals generated
    pub signals: Vec<Signal>,
}

impl BacktestResult {
    /// Get equity curve data for plotting
    pub fn equity_curve(&self) -> Vec<(chrono::DateTime<chrono::Utc>, f64)> {
        self.portfolio.equity_curve.clone()
    }

    /// Get drawdown curve data for plotting
    pub fn drawdown_curve(&self) -> Vec<(chrono::DateTime<chrono::Utc>, f64)> {
        self.portfolio.drawdown_curve.clone()
    }

    /// Get trade list
    pub fn trades(&self) -> &[super::trade::Trade] {
        &self.portfolio.trades
    }
}

/// Backtesting engine
pub struct BacktestEngine {
    config: BacktestConfig,
}

impl BacktestEngine {
    pub fn new(config: BacktestConfig) -> Self {
        Self { config }
    }

    /// Run a backtest with the given strategy and data
    pub fn run(&self, strategy: &mut dyn Strategy, data: &[Bar]) -> BacktestResult {
        let mut portfolio = Portfolio::new(self.config.initial_capital);
        let mut all_signals = Vec::new();

        // Initialize strategy
        strategy.init(data);

        // Process each bar
        for (bar_idx, bar) in data.iter().enumerate() {
            // Create strategy context
            let ctx = StrategyContext {
                bar_idx,
                bar,
                bars: &data[..=bar_idx],
                portfolio: &portfolio,
                symbol: &self.config.symbol,
            };

            // Get signals from strategy
            let signals = strategy.on_bar(&ctx);

            // Process signals
            for signal in signals {
                all_signals.push(signal.clone());
                self.process_signal(&mut portfolio, &signal, bar);
            }

            // Update portfolio with current prices
            let mut prices = HashMap::new();
            prices.insert(self.config.symbol.clone(), bar.close);
            portfolio.update_prices(&prices, bar.time);

            // Update trade bar counts
            for trade in portfolio.trades.iter_mut() {
                if trade.status == super::trade::TradeStatus::Open {
                    trade.bars_held += 1;
                    trade.update_excursions(bar.close);
                }
            }
        }

        // Calculate performance metrics
        let metrics =
            PerformanceMetrics::calculate(&portfolio, self.config.risk_free_rate, data.len());

        BacktestResult {
            portfolio,
            metrics,
            strategy_name: strategy.name().to_string(),
            strategy_params: strategy.params(),
            config: self.config.clone(),
            bars_processed: data.len(),
            signals: all_signals,
        }
    }

    fn process_signal(&self, portfolio: &mut Portfolio, signal: &Signal, bar: &Bar) {
        let price = signal.price.unwrap_or(bar.close);

        // Calculate commission and slippage
        let quantity = signal.quantity.unwrap_or_else(|| {
            // Default to position sizing based on equity and max position %
            let equity = portfolio.equity();
            let max_val = equity * (self.config.max_pos_pct / 100.0);
            (max_val / price).floor()
        });

        if quantity <= 0.0 {
            return;
        }

        let commission = self.config.commission.calculate(price, quantity);
        let slippage = self.config.slippage.calculate(price, quantity, None);

        match signal.signal_type {
            SignalType::Buy => {
                portfolio.open_trade(
                    &signal.symbol,
                    TradeSide::Long,
                    price,
                    quantity,
                    signal.ts,
                    commission,
                    slippage,
                );
            }
            SignalType::Sell => {
                // Close long position
                if portfolio.has_pos(&signal.symbol) {
                    let position = portfolio.get_pos(&signal.symbol).unwrap();
                    let close_qty = signal.quantity.unwrap_or(position.quantity);
                    portfolio.close_trade(
                        &signal.symbol,
                        price,
                        close_qty,
                        signal.ts,
                        commission,
                        slippage,
                    );
                }
            }
            SignalType::Short => {
                if !self.config.allow_short {
                    return;
                }
                portfolio.open_trade(
                    &signal.symbol,
                    TradeSide::Short,
                    price,
                    quantity,
                    signal.ts,
                    commission,
                    slippage,
                );
            }
            SignalType::Cover => {
                // Close short position
                if portfolio.has_pos(&signal.symbol) {
                    let position = portfolio.get_pos(&signal.symbol).unwrap();
                    let close_qty = signal.quantity.unwrap_or(position.quantity);
                    portfolio.close_trade(
                        &signal.symbol,
                        price,
                        close_qty,
                        signal.ts,
                        commission,
                        slippage,
                    );
                }
            }
            SignalType::Exit => {
                // Close any position
                if portfolio.has_pos(&signal.symbol) {
                    let position = portfolio.get_pos(&signal.symbol).unwrap();
                    let close_qty = position.quantity;
                    portfolio.close_trade(
                        &signal.symbol,
                        price,
                        close_qty,
                        signal.ts,
                        commission,
                        slippage,
                    );
                }
            }
        }
    }

    /// Run multiple param combinations for optimization
    pub fn optimize<F, S>(
        &self,
        data: &[Bar],
        param_grid: Vec<Vec<f64>>,
        strategy_factory: F,
    ) -> Vec<(Vec<f64>, BacktestResult)>
    where
        F: Fn(&[f64]) -> S,
        S: Strategy,
    {
        let mut results = Vec::new();

        // Generate all param combinations
        let combinations = Self::cartesian_product(&param_grid);

        for params in combinations {
            let mut strategy = strategy_factory(&params);
            let result = self.run(&mut strategy, data);
            results.push((params, result));
        }

        // Sort by Sharpe ratio (descending)
        results.sort_by(|a, b| {
            b.1.metrics
                .sharpe_ratio
                .partial_cmp(&a.1.metrics.sharpe_ratio)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results
    }

    fn cartesian_product(arrays: &[Vec<f64>]) -> Vec<Vec<f64>> {
        if arrays.is_empty() {
            return vec![vec![]];
        }

        let mut result = vec![vec![]];

        for array in arrays {
            let mut new_result = Vec::new();
            for r in &result {
                for item in array {
                    let mut new_r = r.clone();
                    new_r.push(*item);
                    new_result.push(new_r);
                }
            }
            result = new_result;
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_data() -> Vec<Bar> {
        // Create a simple uptrend
        (0..100)
            .map(|i| {
                let base_price = 100.0 + (i as f64 * 0.5);
                Bar {
                    time: Utc::now(),
                    open: base_price,
                    high: base_price + 2.0,
                    low: base_price - 1.0,
                    close: base_price + 1.0,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_engine_creation() {
        let config = BacktestConfig::new("AAPL").with_capital(100_000.0);

        let engine = BacktestEngine::new(config);
        assert!((engine.config.initial_capital - 100_000.0).abs() < 0.01);
    }

    #[test]
    fn test_backtest_run() {
        use super::super::strategy::SmaCrossover;

        let config = BacktestConfig::new("AAPL").with_capital(100_000.0);

        let engine = BacktestEngine::new(config);
        let data = create_test_data();
        let mut strategy = SmaCrossover::new(5, 20);

        let result = engine.run(&mut strategy, &data);

        assert_eq!(result.bars_processed, 100);
        assert_eq!(result.strategy_name, "SMA Crossover");
    }

    #[test]
    fn test_cartesian_product() {
        let arrays = vec![vec![1.0, 2.0], vec![10.0, 20.0]];

        let result = BacktestEngine::cartesian_product(&arrays);

        assert_eq!(result.len(), 4);
        assert!(result.contains(&vec![1.0, 10.0]));
        assert!(result.contains(&vec![1.0, 20.0]));
        assert!(result.contains(&vec![2.0, 10.0]));
        assert!(result.contains(&vec![2.0, 20.0]));
    }

    #[test]
    fn test_optimize() {
        use super::super::strategy::SmaCrossover;

        let config = BacktestConfig::new("AAPL").with_capital(100_000.0);

        let engine = BacktestEngine::new(config);
        let data = create_test_data();

        let param_grid = vec![vec![5.0, 10.0], vec![20.0, 30.0]];

        let results = engine.optimize(&data, param_grid, |params| {
            SmaCrossover::new(params[0] as usize, params[1] as usize)
        });

        assert_eq!(results.len(), 4);
    }
}
