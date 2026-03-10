use super::portfolio::Portfolio;
use super::trade::Trade;
use crate::model::Bar;
use chrono::{DateTime, Utc};

/// Signal type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalType {
    Buy,
    Sell,
    Short,
    Cover,
    Exit,
}

/// Trading signal
#[derive(Debug, Clone)]
pub struct Signal {
    pub signal_type: SignalType,
    pub symbol: String,
    pub quantity: Option<f64>,
    pub price: Option<f64>,
    pub ts: DateTime<Utc>,
    pub reason: Option<String>,
}

impl Signal {
    pub fn buy(symbol: impl Into<String>) -> Self {
        Self {
            signal_type: SignalType::Buy,
            symbol: symbol.into(),
            quantity: None,
            price: None,
            ts: Utc::now(),
            reason: None,
        }
    }

    pub fn sell(symbol: impl Into<String>) -> Self {
        Self {
            signal_type: SignalType::Sell,
            symbol: symbol.into(),
            quantity: None,
            price: None,
            ts: Utc::now(),
            reason: None,
        }
    }

    pub fn short(symbol: impl Into<String>) -> Self {
        Self {
            signal_type: SignalType::Short,
            symbol: symbol.into(),
            quantity: None,
            price: None,
            ts: Utc::now(),
            reason: None,
        }
    }

    pub fn cover(symbol: impl Into<String>) -> Self {
        Self {
            signal_type: SignalType::Cover,
            symbol: symbol.into(),
            quantity: None,
            price: None,
            ts: Utc::now(),
            reason: None,
        }
    }

    pub fn exit(symbol: impl Into<String>) -> Self {
        Self {
            signal_type: SignalType::Exit,
            symbol: symbol.into(),
            quantity: None,
            price: None,
            ts: Utc::now(),
            reason: None,
        }
    }

    pub fn with_quantity(mut self, qty: f64) -> Self {
        self.quantity = Some(qty);
        self
    }

    pub fn with_price(mut self, price: f64) -> Self {
        self.price = Some(price);
        self
    }

    pub fn with_timestamp(mut self, ts: DateTime<Utc>) -> Self {
        self.ts = ts;
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

/// Context provided to strategy on each bar
#[derive(Debug)]
pub struct StrategyContext<'a> {
    /// Current bar index
    pub bar_idx: usize,
    /// Current bar data
    pub bar: &'a Bar,
    /// All historical bars up to current
    pub bars: &'a [Bar],
    /// Current portfolio state
    pub portfolio: &'a Portfolio,
    /// Symbol being traded
    pub symbol: &'a str,
}

impl<'a> StrategyContext<'a> {
    /// Get bar at offset (negative for past bars)
    pub fn bar_at(&self, offset: isize) -> Option<&Bar> {
        let idx = self.bar_idx as isize + offset;
        if idx >= 0 && (idx as usize) < self.bars.len() {
            Some(&self.bars[idx as usize])
        } else {
            None
        }
    }

    /// Get close price at offset
    pub fn close(&self, offset: isize) -> Option<f64> {
        self.bar_at(offset).map(|b| b.close)
    }

    /// Get high price at offset
    pub fn high(&self, offset: isize) -> Option<f64> {
        self.bar_at(offset).map(|b| b.high)
    }

    /// Get low price at offset
    pub fn low(&self, offset: isize) -> Option<f64> {
        self.bar_at(offset).map(|b| b.low)
    }

    /// Get open price at offset
    pub fn open(&self, offset: isize) -> Option<f64> {
        self.bar_at(offset).map(|b| b.open)
    }

    /// Get volume at offset
    pub fn volume(&self, offset: isize) -> Option<f64> {
        self.bar_at(offset).map(|b| b.volume)
    }

    /// Calculate simple moving avg
    pub fn sma(&self, period: usize) -> Option<f64> {
        if self.bar_idx + 1 < period {
            return None;
        }

        let start = self.bar_idx + 1 - period;
        let sum: f64 = self.bars[start..=self.bar_idx]
            .iter()
            .map(|b| b.close)
            .sum();
        Some(sum / period as f64)
    }

    /// Calculate highest high over period
    pub fn highest_high(&self, period: usize) -> Option<f64> {
        if self.bar_idx + 1 < period {
            return None;
        }

        let start = self.bar_idx + 1 - period;
        self.bars[start..=self.bar_idx]
            .iter()
            .map(|b| b.high)
            .fold(None, |acc, h| match acc {
                None => Some(h),
                Some(max) => Some(f64::max(max, h)),
            })
    }

    /// Calculate lowest low over period
    pub fn lowest_low(&self, period: usize) -> Option<f64> {
        if self.bar_idx + 1 < period {
            return None;
        }

        let start = self.bar_idx + 1 - period;
        self.bars[start..=self.bar_idx]
            .iter()
            .map(|b| b.low)
            .fold(None, |acc, l| match acc {
                None => Some(l),
                Some(min) => Some(f64::min(min, l)),
            })
    }

    /// Check if we have a long position
    pub fn is_long(&self) -> bool {
        self.portfolio
            .get_pos(self.symbol)
            .map(|p| matches!(p.side, super::portfolio::PosSide::Long))
            .unwrap_or(false)
    }

    /// Check if we have a short position
    pub fn is_short(&self) -> bool {
        self.portfolio
            .get_pos(self.symbol)
            .map(|p| matches!(p.side, super::portfolio::PosSide::Short))
            .unwrap_or(false)
    }

    /// Check if we are flat
    pub fn is_flat(&self) -> bool {
        !self.portfolio.has_pos(self.symbol)
    }

    /// Get current position quantity
    pub fn pos_size(&self) -> f64 {
        self.portfolio
            .get_pos(self.symbol)
            .map(|p| p.quantity)
            .unwrap_or(0.0)
    }

    /// Get current equity
    pub fn equity(&self) -> f64 {
        self.portfolio.equity()
    }

    /// Get unrealized P&L for current position
    pub fn unrealized_pnl(&self) -> f64 {
        self.portfolio
            .get_pos(self.symbol)
            .map(|p| p.unrealized_pnl)
            .unwrap_or(0.0)
    }
}

/// Strategy trait for implementing trading strategies
pub trait Strategy: Send {
    /// Called once at the start of backtest with all data
    fn init(&mut self, _data: &[Bar]) {}

    /// Called on each bar, returns trading signals
    fn on_bar(&mut self, ctx: &StrategyContext) -> Vec<Signal>;

    /// Called after a trade is filled
    fn on_fill(&mut self, _trade: &Trade) {}

    /// Get strategy name
    fn name(&self) -> &str;

    /// Get strategy params as key-value pairs
    fn params(&self) -> Vec<(String, String)> {
        Vec::new()
    }
}

/// Example: Simple Moving Avg Crossover Strategy
pub struct SmaCrossover {
    fast_period: usize,
    slow_period: usize,
    fast_sma: Vec<f64>,
    slow_sma: Vec<f64>,
}

impl SmaCrossover {
    pub fn new(fast_period: usize, slow_period: usize) -> Self {
        Self {
            fast_period,
            slow_period,
            fast_sma: Vec::new(),
            slow_sma: Vec::new(),
        }
    }

    fn calculate_sma(data: &[Bar], period: usize) -> Vec<f64> {
        let mut sma = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i + 1 < period {
                sma.push(f64::NAN);
            } else {
                let sum: f64 = data[i + 1 - period..=i].iter().map(|b| b.close).sum();
                sma.push(sum / period as f64);
            }
        }

        sma
    }
}

impl Strategy for SmaCrossover {
    fn name(&self) -> &str {
        "SMA Crossover"
    }

    fn params(&self) -> Vec<(String, String)> {
        vec![
            ("fast_period".to_string(), self.fast_period.to_string()),
            ("slow_period".to_string(), self.slow_period.to_string()),
        ]
    }

    fn init(&mut self, data: &[Bar]) {
        self.fast_sma = Self::calculate_sma(data, self.fast_period);
        self.slow_sma = Self::calculate_sma(data, self.slow_period);
    }

    fn on_bar(&mut self, ctx: &StrategyContext) -> Vec<Signal> {
        let idx = ctx.bar_idx;

        // Need at least 2 bars to detect crossover
        if idx < 1 {
            return Vec::new();
        }

        let fast_current = self.fast_sma.get(idx).copied().unwrap_or(f64::NAN);
        let fast_prev = self.fast_sma.get(idx - 1).copied().unwrap_or(f64::NAN);
        let slow_current = self.slow_sma.get(idx).copied().unwrap_or(f64::NAN);
        let slow_prev = self.slow_sma.get(idx - 1).copied().unwrap_or(f64::NAN);

        if fast_current.is_nan() || slow_current.is_nan() {
            return Vec::new();
        }

        let mut signals = Vec::new();

        // Golden cross: fast crosses above slow
        if fast_prev <= slow_prev && fast_current > slow_current {
            if ctx.is_short() {
                signals.push(Signal::cover(ctx.symbol).with_reason("Golden cross"));
            }
            if ctx.is_flat() || ctx.is_short() {
                signals.push(Signal::buy(ctx.symbol).with_reason("Golden cross"));
            }
        }

        // Death cross: fast crosses below slow
        if fast_prev >= slow_prev && fast_current < slow_current && ctx.is_long() {
            signals.push(Signal::sell(ctx.symbol).with_reason("Death cross"));
        }

        signals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_bars(prices: Vec<f64>) -> Vec<Bar> {
        prices
            .into_iter()
            .map(|p| Bar {
                time: Utc::now(),
                open: p,
                high: p + 1.0,
                low: p - 1.0,
                close: p,
                volume: 1000.0,
            })
            .collect()
    }

    #[test]
    fn test_signal_creation() {
        let signal = Signal::buy("AAPL").with_quantity(100.0).with_reason("Test");

        assert_eq!(signal.signal_type, SignalType::Buy);
        assert_eq!(signal.symbol, "AAPL");
        assert_eq!(signal.quantity, Some(100.0));
        assert_eq!(signal.reason, Some("Test".to_string()));
    }

    #[test]
    fn test_sma_crossover_strategy() {
        let mut strategy = SmaCrossover::new(5, 10);
        let bars = create_test_bars(vec![
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0, 111.0,
        ]);

        strategy.init(&bars);

        assert_eq!(strategy.fast_sma.len(), bars.len());
        assert_eq!(strategy.slow_sma.len(), bars.len());
    }

    #[test]
    fn test_ctx_sma() {
        let bars = create_test_bars(vec![10.0, 20.0, 30.0, 40.0, 50.0]);
        let portfolio = super::super::portfolio::Portfolio::new(100_000.0);

        let ctx = StrategyContext {
            bar_idx: 4,
            bar: &bars[4],
            bars: &bars,
            portfolio: &portfolio,
            symbol: "TEST",
        };

        let sma = ctx.sma(5).unwrap();
        assert!((sma - 30.0).abs() < 0.01); // Avg of 10,20,30,40,50
    }
}
