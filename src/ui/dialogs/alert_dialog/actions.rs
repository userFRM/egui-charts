//! Actions from the alert creation dialog.

/// Alert data for creation
#[derive(Clone, Debug)]
pub enum AlertData {
    Price {
        symbol: String,
        price: f64,
        condition: String,
        message: String,
        repeating: bool,
    },
    Indicator {
        symbol: String,
        indicator_a: String,
        indicator_b: Option<String>,
        threshold: Option<f64>,
        condition: String,
        message: String,
        repeating: bool,
    },
    Volume {
        symbol: String,
        multiplier: f64,
        lookback: u32,
        message: String,
    },
}

/// Actions from the alert dialog
#[derive(Clone, Debug)]
pub enum AlertDialogAction {
    None,
    Cancel,
    Create(AlertData),
}
