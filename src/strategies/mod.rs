//! Strategies that trade on closed bars, and the [`Backtest`] engine that runs them.
//!
//! A [`Strategy`] computes the studies it needs once, then decides on each closed bar by placing
//! named entry and exit orders through a [`StrategyContext`]. [`Backtest::run`] fills those orders
//! along each bar's price path and returns a [`StrategyReport`](crate::model::StrategyReport), so
//! the report's metrics, its panel and its CSV export work on the result. Needs no egui, so a
//! server can run the same code.

mod backtest;

pub use backtest::{Backtest, Strategy, StrategyContext};
