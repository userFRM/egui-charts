//! Price display modes and auto-scale strategies.
//!
//! [`PriceDisplayMode`] controls how prices appear to the user (absolute,
//! percentage change, points, or basis points).  [`AutoScaleMode`] controls
//! how the visible price range is calculated.

/// Price display modes for different price range visualizations.
use std::fmt;

/// How to display price values on the chart
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PriceDisplayMode {
    /// Absolute price values (default)
    /// Example: $50,123.45
    #[default]
    Absolute,

    /// Percentage change from reference price
    /// Example: +2.34% or -1.56%
    Percentage {
        /// Reference price for percentage calculation
        /// If None, uses first visible bar's close price
        reference_price: Option<i64>, // Using i64 to avoid f64 in Hash/Eq
    },

    /// Points change from reference price
    /// Example: +150 pts or -75 pts
    Points {
        /// Reference price for points calculation
        reference_price: Option<i64>,
    },

    /// Basis points (1/100 of 1%)
    /// Example: +234 bps or -156 bps
    BasisPoints { reference_price: Option<i64> },
}

impl PriceDisplayMode {
    /// Format a price value according to this display mode
    pub fn format(&self, price: f64, reference: Option<f64>) -> String {
        match self {
            Self::Absolute => {
                format!("{price:.2}")
            }
            Self::Percentage { reference_price } => {
                let ref_price = reference_price
                    .map(|r| r as f64)
                    .or(reference)
                    .unwrap_or(price);

                if ref_price == 0.0 {
                    return "N/A".to_string();
                }

                let change = ((price - ref_price) / ref_price) * 100.0;
                if change >= 0.0 {
                    format!("+{change:.2}%")
                } else {
                    format!("{change:.2}%")
                }
            }
            Self::Points { reference_price } => {
                let ref_price = reference_price
                    .map(|r| r as f64)
                    .or(reference)
                    .unwrap_or(price);

                let change = price - ref_price;
                if change >= 0.0 {
                    format!("+{change:.2} pts")
                } else {
                    format!("{change:.2} pts")
                }
            }
            Self::BasisPoints { reference_price } => {
                let ref_price = reference_price
                    .map(|r| r as f64)
                    .or(reference)
                    .unwrap_or(price);

                if ref_price == 0.0 {
                    return "N/A".to_string();
                }

                let change = ((price - ref_price) / ref_price) * 10000.0;
                if change >= 0.0 {
                    format!("+{change:.0} bps")
                } else {
                    format!("{change:.0} bps")
                }
            }
        }
    }

    /// Convert price to display value (for calculations)
    pub fn to_display_val(&self, price: f64, reference: Option<f64>) -> f64 {
        match self {
            Self::Absolute => price,
            Self::Percentage { reference_price } => {
                let ref_price = reference_price
                    .map(|r| r as f64)
                    .or(reference)
                    .unwrap_or(price);

                if ref_price == 0.0 {
                    return 0.0;
                }

                ((price - ref_price) / ref_price) * 100.0
            }
            Self::Points { reference_price } => {
                let ref_price = reference_price
                    .map(|r| r as f64)
                    .or(reference)
                    .unwrap_or(price);

                price - ref_price
            }
            Self::BasisPoints { reference_price } => {
                let ref_price = reference_price
                    .map(|r| r as f64)
                    .or(reference)
                    .unwrap_or(price);

                if ref_price == 0.0 {
                    return 0.0;
                }

                ((price - ref_price) / ref_price) * 10000.0
            }
        }
    }
}

impl fmt::Display for PriceDisplayMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absolute => write!(f, "Absolute"),
            Self::Percentage { .. } => write!(f, "Percentage"),
            Self::Points { .. } => write!(f, "Points"),
            Self::BasisPoints { .. } => write!(f, "Basis Points"),
        }
    }
}

/// Auto-scale strategy for price axis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AutoScaleMode {
    /// Scale to fit all data in the dataset
    AllData,

    /// Scale to fit only visible data (default)
    #[default]
    VisibleData,

    /// Scale to fit visible data with padding
    VisibleDataWithPadding {
        /// Padding percentage (0.0 - 1.0)
        /// 0.1 = 10% padding on top and bottom
        padding_percent: u8, // 0-100, using u8 for Hash/Eq
    },

    /// Custom fixed range
    Custom {
        min: i64, // Using i64 to avoid f64 in Hash/Eq
        max: i64,
    },

    /// Lock current range (no auto-scaling)
    Locked,
}

impl AutoScaleMode {
    /// Calculate price range based on this mode
    pub fn calculate_range(
        &self,
        visible_min: f64,
        visible_max: f64,
        all_data_min: f64,
        all_data_max: f64,
    ) -> (f64, f64) {
        match self {
            Self::AllData => (all_data_min, all_data_max),

            Self::VisibleData => (visible_min, visible_max),

            Self::VisibleDataWithPadding { padding_percent } => {
                let padding = *padding_percent as f64 / 100.0;
                let range = visible_max - visible_min;
                let padding_amount = range * padding;
                (visible_min - padding_amount, visible_max + padding_amount)
            }

            Self::Custom { min, max } => (*min as f64, *max as f64),

            Self::Locked => (visible_min, visible_max),
        }
    }

    /// Create visible data mode with standard padding (10%)
    pub fn visible_with_standard_padding() -> Self {
        Self::VisibleDataWithPadding {
            padding_percent: 10,
        }
    }
}

impl fmt::Display for AutoScaleMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllData => write!(f, "All Data"),
            Self::VisibleData => write!(f, "Visible Data"),
            Self::VisibleDataWithPadding { padding_percent } => {
                write!(f, "Visible + {padding_percent}% padding")
            }
            Self::Custom { .. } => write!(f, "Custom Range"),
            Self::Locked => write!(f, "Locked"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_display_absolute() {
        let mode = PriceDisplayMode::Absolute;
        assert_eq!(mode.format(50123.45, None), "50123.45");
    }

    #[test]
    fn test_price_display_percentage() {
        let mode = PriceDisplayMode::Percentage {
            reference_price: Some(50000),
        };
        assert_eq!(mode.format(51000.0, None), "+2.00%");
        assert_eq!(mode.format(49000.0, None), "-2.00%");
    }

    #[test]
    fn test_price_display_points() {
        let mode = PriceDisplayMode::Points {
            reference_price: Some(50000),
        };
        assert_eq!(mode.format(50150.0, None), "+150.00 pts");
        assert_eq!(mode.format(49925.0, None), "-75.00 pts");
    }

    #[test]
    fn test_price_display_basis_points() {
        let mode = PriceDisplayMode::BasisPoints {
            reference_price: Some(50000),
        };
        // 1% = 100 bps, so 2% = 200 bps
        let formatted = mode.format(51000.0, None);
        assert!(formatted.contains("200"));
        assert!(formatted.contains("bps"));
    }

    #[test]
    fn test_auto_scale_visible_with_padding() {
        let mode = AutoScaleMode::VisibleDataWithPadding {
            padding_percent: 10,
        };
        let (min, max) = mode.calculate_range(100.0, 200.0, 50.0, 250.0);

        // Range is 100, 10% padding = 10
        assert_eq!(min, 90.0); // 100 - 10
        assert_eq!(max, 210.0); // 200 + 10
    }

    #[test]
    fn test_auto_scale_all_data() {
        let mode = AutoScaleMode::AllData;
        let (min, max) = mode.calculate_range(100.0, 200.0, 50.0, 250.0);
        assert_eq!(min, 50.0);
        assert_eq!(max, 250.0);
    }

    #[test]
    fn test_auto_scale_custom() {
        let mode = AutoScaleMode::Custom {
            min: 1000,
            max: 2000,
        };
        let (min, max) = mode.calculate_range(100.0, 200.0, 50.0, 250.0);
        assert_eq!(min, 1000.0);
        assert_eq!(max, 2000.0);
    }
}
