use chrono::{DateTime, Utc};

/// Trade status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeStatus {
    Open,
    Closed,
    Cancelled,
}

/// Represents a single trade (round trip)
#[derive(Debug, Clone)]
pub struct Trade {
    pub id: usize,
    pub symbol: String,
    pub side: TradeSide,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub quantity: f64,
    pub entry_time: DateTime<Utc>,
    pub exit_time: Option<DateTime<Utc>>,
    pub commission: f64,
    pub slippage: f64,
    pub status: TradeStatus,
    pub pnl: f64,
    pub pnl_percent: f64,
    pub mae: f64, // Max Adverse Excursion
    pub mfe: f64, // Max Favorable Excursion
    pub bars_held: usize,
    pub tags: Vec<String>,
}

/// Trade direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSide {
    Long,
    Short,
}

impl Trade {
    pub fn new(
        id: usize,
        symbol: String,
        side: TradeSide,
        entry_price: f64,
        quantity: f64,
        entry_time: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            symbol,
            side,
            entry_price,
            exit_price: None,
            quantity,
            entry_time,
            exit_time: None,
            commission: 0.0,
            slippage: 0.0,
            status: TradeStatus::Open,
            pnl: 0.0,
            pnl_percent: 0.0,
            mae: 0.0,
            mfe: 0.0,
            bars_held: 0,
            tags: Vec::new(),
        }
    }

    /// Close the trade
    pub fn close(&mut self, exit_price: f64, exit_time: DateTime<Utc>) {
        self.exit_price = Some(exit_price);
        self.exit_time = Some(exit_time);
        self.status = TradeStatus::Closed;
        self.calculate_pnl();
    }

    /// Update MAE/MFE based on current price
    pub fn update_excursions(&mut self, curr_price: f64) {
        let unrealized = self.unrealized_pnl(curr_price);

        // MAE is the worst the trade got
        if unrealized < self.mae {
            self.mae = unrealized;
        }

        // MFE is the best the trade got
        if unrealized > self.mfe {
            self.mfe = unrealized;
        }
    }

    /// Calculate unrealized P&L at a given price
    pub fn unrealized_pnl(&self, curr_price: f64) -> f64 {
        let price_diff = curr_price - self.entry_price;
        match self.side {
            TradeSide::Long => price_diff * self.quantity,
            TradeSide::Short => -price_diff * self.quantity,
        }
    }

    /// Calculate final P&L
    fn calculate_pnl(&mut self) {
        if let Some(exit_price) = self.exit_price {
            let price_diff = exit_price - self.entry_price;
            let gross_pnl = match self.side {
                TradeSide::Long => price_diff * self.quantity,
                TradeSide::Short => -price_diff * self.quantity,
            };

            self.pnl = gross_pnl - self.commission - self.slippage;
            self.pnl_percent = if self.entry_price > 0.0 {
                (self.pnl / (self.entry_price * self.quantity)) * 100.0
            } else {
                0.0
            };
        }
    }

    /// Check if trade is profitable
    pub fn is_winner(&self) -> bool {
        self.pnl > 0.0
    }

    /// Get trade duration
    pub fn duration(&self) -> Option<chrono::Duration> {
        self.exit_time.map(|exit| exit - self.entry_time)
    }

    /// Add a tag to the trade
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_creation() {
        let trade = Trade::new(1, "AAPL".into(), TradeSide::Long, 150.0, 100.0, Utc::now());
        assert_eq!(trade.id, 1);
        assert_eq!(trade.symbol, "AAPL");
        assert_eq!(trade.entry_price, 150.0);
        assert_eq!(trade.quantity, 100.0);
        assert_eq!(trade.status, TradeStatus::Open);
    }

    #[test]
    fn test_trade_close_long() {
        let mut trade = Trade::new(1, "AAPL".into(), TradeSide::Long, 100.0, 10.0, Utc::now());
        trade.close(110.0, Utc::now());

        assert_eq!(trade.status, TradeStatus::Closed);
        assert!((trade.pnl - 100.0).abs() < 0.01); // 10 * 10 = 100
        assert!(trade.is_winner());
    }

    #[test]
    fn test_trade_close_short() {
        let mut trade = Trade::new(1, "AAPL".into(), TradeSide::Short, 100.0, 10.0, Utc::now());
        trade.close(90.0, Utc::now());

        assert_eq!(trade.status, TradeStatus::Closed);
        assert!((trade.pnl - 100.0).abs() < 0.01); // 10 * 10 = 100
        assert!(trade.is_winner());
    }

    #[test]
    fn test_unrealized_pnl() {
        let trade = Trade::new(1, "AAPL".into(), TradeSide::Long, 100.0, 10.0, Utc::now());
        assert!((trade.unrealized_pnl(110.0) - 100.0).abs() < 0.01);
        assert!((trade.unrealized_pnl(90.0) - (-100.0)).abs() < 0.01);
    }

    #[test]
    fn test_mae_mfe() {
        let mut trade = Trade::new(1, "AAPL".into(), TradeSide::Long, 100.0, 10.0, Utc::now());

        // Price goes up
        trade.update_excursions(110.0);
        assert!((trade.mfe - 100.0).abs() < 0.01);
        assert!((trade.mae - 0.0).abs() < 0.01);

        // Price goes down
        trade.update_excursions(95.0);
        assert!((trade.mfe - 100.0).abs() < 0.01);
        assert!((trade.mae - (-50.0)).abs() < 0.01);
    }
}
