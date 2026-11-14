//! Strategies over historical and live bars, and the [`Backtest`] engine that runs them.
//!
//! A [`Strategy`] computes the studies it needs once, then decides on each closed bar by placing
//! named entry and exit orders through a [`StrategyContext`]. [`Backtest::run`] fills those orders
//! along each bar's price path and returns a [`StrategyReport`](crate::model::StrategyReport), so
//! the report's metrics, its panel and its CSV export work on the result. Needs no egui, so a
//! server can run the same code.
//!
//! [`StrategyFactory`] supplies ten price-action strategies and, with `studies`, twelve
//! strategies using the chart studies' public values. Hosts can register their own constructors.
//! Use [`Strategy::study_inputs`] and [`Strategy::set_input`] to edit bounded parameters before
//! a run. Gap and inside/outside strategies take the host instrument's `tick_size` input; their
//! exits are distances from the actual position fill price. The engine's settings supply size
//! and pyramiding. Built-ins ignore fill callbacks; live tick recalculation follows the run settings.
//!
//! [`Backtest::update`] retains the execution state as the host appends bars or changes the
//! forming bar, and returns [`StrategyFill`] events. [`Backtest::report`] exposes its report.
//! Feed those events to `services::AlertEngine::strategy_fills` with both engine features enabled.
//!
//! [`optimise`] runs a lazy grid of input combinations on fresh strategies and yields each
//! combination's [`StrategyTrial`], using the same bars and backtest settings throughout.

mod backtest;
mod library;
mod optimize;

pub use library::StrategyFactory;
pub use optimize::{StrategyTrial, optimise};

pub use backtest::{
    Backtest, BacktestSettings, Commission, OrderSize, Slippage, Strategy, StrategyContext,
    StrategyFill,
};
