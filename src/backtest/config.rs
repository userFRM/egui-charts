/// Commission calculation model
#[derive(Debug, Clone, Default)]
pub enum CommissionModel {
    /// No commission
    #[default]
    None,
    /// Fixed amount per trade
    Fixed(f64),
    /// Percentage of trade value
    Percentage(f64),
    /// Per share/contract commission
    PerShare(f64),
    /// Tiered commission (breakpoints, rate per tier)
    Tiered(Vec<(f64, f64)>),
}

impl CommissionModel {
    /// Calculate commission for a trade
    pub fn calculate(&self, price: f64, quantity: f64) -> f64 {
        match self {
            CommissionModel::None => 0.0,
            CommissionModel::Fixed(amount) => *amount,
            CommissionModel::Percentage(pct) => price * quantity * (pct / 100.0),
            CommissionModel::PerShare(rate) => quantity * rate,
            CommissionModel::Tiered(tiers) => {
                let trade_val = price * quantity;
                for (threshold, rate) in tiers.iter().rev() {
                    if trade_val >= *threshold {
                        return trade_val * (rate / 100.0);
                    }
                }
                // Use first tier if below all thresholds
                tiers
                    .first()
                    .map(|(_, rate)| trade_val * (rate / 100.0))
                    .unwrap_or(0.0)
            }
        }
    }
}

/// Slippage calculation model
#[derive(Debug, Clone, Default)]
pub enum SlippageModel {
    /// No slippage
    #[default]
    None,
    /// Fixed amount per trade
    Fixed(f64),
    /// Percentage of price
    Percentage(f64),
    /// Basis points
    BasisPoints(f64),
    /// Volume-dependent slippage (shares, impact factor)
    VolumeImpact { impact_factor: f64 },
}

impl SlippageModel {
    /// Calculate slippage for a trade
    pub fn calculate(&self, price: f64, quantity: f64, avg_volume: Option<f64>) -> f64 {
        match self {
            SlippageModel::None => 0.0,
            SlippageModel::Fixed(amount) => *amount,
            SlippageModel::Percentage(pct) => price * (pct / 100.0),
            SlippageModel::BasisPoints(bps) => price * (bps / 10000.0),
            SlippageModel::VolumeImpact { impact_factor } => {
                if let Some(volume) = avg_volume {
                    let participation = quantity / volume;
                    price * participation * impact_factor
                } else {
                    0.0
                }
            }
        }
    }
}

/// Backtest configuration
#[derive(Debug, Clone)]
pub struct BacktestConfig {
    /// Starting capital
    pub initial_capital: f64,
    /// Commission model
    pub commission: CommissionModel,
    /// Slippage model
    pub slippage: SlippageModel,
    /// Allow shorting
    pub allow_short: bool,
    /// Allow fractional shares
    pub fractional_shares: bool,
    /// Max position size as percentage of equity
    pub max_pos_pct: f64,
    /// Max leverage
    pub max_leverage: f64,
    /// Margin requirement for short positions
    pub margin_requirement: f64,
    /// Risk-free rate for Sharpe calculation
    pub risk_free_rate: f64,
    /// Trading hours only
    pub trading_hours_only: bool,
    /// Symbol being backtested
    pub symbol: String,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            initial_capital: 100_000.0,
            commission: CommissionModel::None,
            slippage: SlippageModel::None,
            allow_short: true,
            fractional_shares: true,
            max_pos_pct: 100.0,
            max_leverage: 1.0,
            margin_requirement: 0.5,
            risk_free_rate: 0.0,
            trading_hours_only: false,
            symbol: String::new(),
        }
    }
}

impl BacktestConfig {
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            ..Default::default()
        }
    }

    pub fn with_capital(mut self, capital: f64) -> Self {
        self.initial_capital = capital;
        self
    }

    pub fn with_commission(mut self, commission: CommissionModel) -> Self {
        self.commission = commission;
        self
    }

    pub fn with_slippage(mut self, slippage: SlippageModel) -> Self {
        self.slippage = slippage;
        self
    }

    pub fn with_max_pos_pct(mut self, pct: f64) -> Self {
        self.max_pos_pct = pct;
        self
    }

    pub fn with_leverage(mut self, leverage: f64) -> Self {
        self.max_leverage = leverage;
        self
    }

    pub fn no_short(mut self) -> Self {
        self.allow_short = false;
        self
    }

    pub fn with_risk_free_rate(mut self, rate: f64) -> Self {
        self.risk_free_rate = rate;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commission_fixed() {
        let model = CommissionModel::Fixed(9.95);
        assert!((model.calculate(100.0, 100.0) - 9.95).abs() < 0.01);
    }

    #[test]
    fn test_commission_percentage() {
        let model = CommissionModel::Percentage(0.1); // 0.1%
        assert!((model.calculate(100.0, 100.0) - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_commission_per_share() {
        let model = CommissionModel::PerShare(0.01);
        assert!((model.calculate(100.0, 100.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_slippage_bps() {
        let model = SlippageModel::BasisPoints(10.0); // 10 bps
        assert!((model.calculate(100.0, 100.0, None) - 0.1).abs() < 0.01);
    }

    #[test]
    fn test_config_builder() {
        let config = BacktestConfig::new("AAPL")
            .with_capital(50_000.0)
            .with_commission(CommissionModel::Fixed(5.0))
            .no_short();

        assert_eq!(config.symbol, "AAPL");
        assert!((config.initial_capital - 50_000.0).abs() < 0.01);
        assert!(!config.allow_short);
    }
}
