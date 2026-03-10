//! Backtesting Framework
//!
//! A comprehensive backtesting engine for strategy evaluation with:
//! - Event-driven architecture
//! - Realistic commission and slippage models
//! - Pos and portfolio tracking
//! - Extensive performance metrics

mod config;
mod engine;
mod metrics;
mod portfolio;
mod strategy;
mod trade;

pub use config::{BacktestConfig, CommissionModel, SlippageModel};
pub use engine::{BacktestEngine, BacktestResult};
pub use metrics::PerformanceMetrics;
pub use portfolio::{Portfolio, Pos, PosSide};
pub use strategy::{Signal, SignalType, SmaCrossover, Strategy, StrategyContext};
pub use trade::{Trade, TradeSide, TradeStatus};
