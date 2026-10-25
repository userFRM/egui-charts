//! Study value units and scale placement.

use crate::scales::{PriceFormatter, VolumeFormatter};

/// Units used in study legends, axes, tooltips and data windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StudyFormat {
    /// Price, following instrument precision unless explicitly overridden.
    Price,
    /// Signed volume with K, M and B suffixes.
    Volume,
    /// Percentage units: a value of 5 represents 5%, not 500%.
    Percent,
    /// A fixed number of decimal places, capped at twelve.
    Fixed(usize),
}

impl StudyFormat {
    /// Format a finite value with an optional precision override; non-finite values use a dash.
    pub fn format(self, value: f64, precision: Option<usize>) -> String {
        if !value.is_finite() {
            return "-".to_owned();
        }
        let decimals = precision
            .unwrap_or(match self {
                Self::Fixed(decimals) => decimals,
                Self::Volume if value.abs() < 1_000.0 => 0,
                _ => 2,
            })
            .min(if self == Self::Price { 18 } else { 12 });
        match self {
            Self::Volume => VolumeFormatter {
                precision: decimals,
            }
            .format(value),
            Self::Percent => format!("{value:.decimals$}%"),
            Self::Price | Self::Fixed(_) => format!("{value:.decimals$}"),
        }
    }
}

/// Value scale in the pane selected by [`super::Indicator::is_overlay`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StudyScale {
    /// Share the pane's main price scale and participate in its auto-range.
    Main,
    /// Fit this study's visible outputs independently and show its own axis.
    Independent,
    /// Fit independently without an axis or value tags.
    None,
}
