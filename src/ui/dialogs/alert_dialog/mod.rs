//! Alert creation dialog.
//!
//! Modal dialog for creating price, indicator, and volume alerts.

mod actions;
mod config;
mod dialog;
mod tabs;

pub use actions::{AlertData, AlertDialogAction};
pub use config::AlertDialogConfig;
pub use dialog::{AlertDialog, IndicatorAlertFormData, PriceAlertFormData, VolumeAlertFormData};
pub use tabs::AlertTab;

/// What the alert forms accept, shared by [`AlertDialog`] and the alerts panel.
pub(crate) mod fields {
    /// A price or threshold level: any finite number.
    pub(crate) fn level(text: &str) -> Option<f64> {
        text.parse().ok().filter(|level: &f64| level.is_finite())
    }

    /// A volume multiplier: finite and above zero.
    pub(crate) fn multiplier(text: &str) -> Option<f64> {
        level(text).filter(|&multiplier| multiplier > 0.0)
    }

    /// A lookback: a whole number of bars, at least one.
    pub(crate) fn lookback(text: &str) -> Option<u32> {
        text.parse().ok().filter(|&bars| bars >= 1)
    }
}
