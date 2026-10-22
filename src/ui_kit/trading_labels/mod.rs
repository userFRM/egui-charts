//! Trading label widgets: `PnlLabel` and `SideLabel`.
//!
//! Pre-styled labels for common trading UI patterns:
//! - `PnlLabel`: a profit or loss coloured by its sign (green, red or neutral)
//! - `SideLabel`: coloured buy and sell or long and short badges

use egui::{Color32, Response, RichText, Ui};

use crate::scales::{CurrencyFormatter, PriceFormatter};
use crate::theme::ThemeExt;

/// The theme's success colour for a gain, its error colour for a loss.
fn side_color(ui: &Ui, gain: bool) -> Color32 {
    let colors = ui.active_theme().ui;
    if gain { colors.success } else { colors.error }
}

/// `magnitude` (a formatted absolute value) with `+` for a gain or `-` for a loss, and no sign
/// when it shows zero. The value is also the one the label is coloured by: zero when the text
/// shows zero.
fn signed(value: f64, magnitude: String) -> (f64, String) {
    if !magnitude.bytes().any(|b| matches!(b, b'1'..=b'9')) {
        return (0.0, magnitude);
    }
    let sign = if value < 0.0 { "-" } else { "+" };
    (value, format!("{sign}{magnitude}"))
}

// =============================================================================
// PnlLabel
// =============================================================================

/// Auto-colored profit/loss label.
///
/// Colors text with the theme's success or error colour by the value's sign, and zero in the
/// normal text colour.
///
/// ```
/// # use egui_charts::ui_kit::trading_labels::PnlLabel;
/// # egui::__run_test_ui(|ui| {
/// PnlLabel::currency(1234.5).show(ui);       // "+$1,234.50" in green
/// PnlLabel::currency(-50.0).show(ui);        // "-$50.00" in red
/// PnlLabel::percent(5.34).show(ui);          // "+5.34%" in green
/// PnlLabel::value(-10.0, "custom").show(ui); // "custom" in red
/// # });
/// ```
pub struct PnlLabel {
    value: f64,
    text: String,
    strong: bool,
    size: Option<f32>,
}

impl PnlLabel {
    /// Display a currency P&L value (e.g., "+$1,234.56" or "-$98.10")
    pub fn currency(value: f64) -> Self {
        let (value, text) = signed(value, CurrencyFormatter::usd().format(value.abs()));
        Self {
            value,
            text,
            strong: false,
            size: None,
        }
    }

    /// Display a percentage P&L value given in percent units (`5.34` shows "+5.34%")
    pub fn percent(value: f64) -> Self {
        let (value, text) = signed(value, format!("{:.2}%", value.abs()));
        Self {
            value,
            text,
            strong: false,
            size: None,
        }
    }

    /// Display custom text colored by the given value's sign
    pub fn value(value: f64, text: impl Into<String>) -> Self {
        Self {
            value,
            text: text.into(),
            strong: false,
            size: None,
        }
    }

    /// Make the label bold
    #[must_use]
    pub fn strong(mut self) -> Self {
        self.strong = true;
        self
    }

    /// Set the font size
    #[must_use]
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Show the label
    pub fn show(self, ui: &mut Ui) -> Response {
        let color = if self.value > 0.0 {
            side_color(ui, true)
        } else if self.value < 0.0 {
            side_color(ui, false)
        } else {
            ui.visuals().text_color()
        };
        let mut rt = RichText::new(&self.text).color(color);
        if self.strong {
            rt = rt.strong();
        }
        if let Some(s) = self.size {
            rt = rt.size(s);
        }
        ui.label(rt)
    }
}

// =============================================================================
// SideLabel
// =============================================================================

/// Colored buy/sell or long/short badge.
///
/// Uses the theme's success colour for buy/long and its error colour for sell/short.
///
/// ```
/// # use egui_charts::ui_kit::trading_labels::SideLabel;
/// # egui::__run_test_ui(|ui| {
/// SideLabel::buy().show(ui);   // "BUY" in green
/// SideLabel::sell().show(ui);  // "SELL" in red
/// SideLabel::long().show(ui);  // "LONG" in green
/// SideLabel::short().show(ui); // "SHORT" in red
/// # });
/// ```
pub struct SideLabel {
    is_buy: bool,
    text: String,
    size: Option<f32>,
}

impl SideLabel {
    /// "BUY" label in bullish color
    pub fn buy() -> Self {
        Self {
            is_buy: true,
            text: "BUY".to_string(),
            size: None,
        }
    }

    /// "SELL" label in bearish color
    pub fn sell() -> Self {
        Self {
            is_buy: false,
            text: "SELL".to_string(),
            size: None,
        }
    }

    /// "LONG" label in bullish color
    pub fn long() -> Self {
        Self {
            is_buy: true,
            text: "LONG".to_string(),
            size: None,
        }
    }

    /// "SHORT" label in bearish color
    pub fn short() -> Self {
        Self {
            is_buy: false,
            text: "SHORT".to_string(),
            size: None,
        }
    }

    /// Custom side label with the given text
    pub fn new(is_buy: bool, text: impl Into<String>) -> Self {
        Self {
            is_buy,
            text: text.into(),
            size: None,
        }
    }

    /// Set the font size
    #[must_use]
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Show the label
    pub fn show(self, ui: &mut Ui) -> Response {
        let color = side_color(ui, self.is_buy);
        let mut rt = RichText::new(&self.text).color(color);
        if let Some(s) = self.size {
            rt = rt.size(s);
        }
        ui.label(rt)
    }
}

#[cfg(test)]
mod tests {
    use super::PnlLabel;

    #[test]
    fn pnl_text_signs_before_the_symbol_and_groups_thousands() {
        let wrong: Vec<_> = [
            (PnlLabel::currency(1234.56), "+$1,234.56"),
            (PnlLabel::currency(-98.10), "-$98.10"),
            (PnlLabel::currency(1_234_567.891), "+$1,234,567.89"),
            (PnlLabel::currency(0.0), "$0.00"),
            (PnlLabel::currency(-0.0), "$0.00"),
            (PnlLabel::currency(-0.001), "$0.00"),
            (PnlLabel::percent(5.34), "+5.34%"),
            (PnlLabel::percent(-1.28), "-1.28%"),
            (PnlLabel::percent(0.0), "0.00%"),
            (PnlLabel::percent(-0.0), "0.00%"),
        ]
        .into_iter()
        .filter(|(label, expected)| label.text != *expected)
        .map(|(label, expected)| (label.text, expected))
        .collect();
        assert!(wrong.is_empty(), "(got, expected): {wrong:?}");
        // Text that shows zero takes the zero colour, whatever the value was.
        assert_eq!(PnlLabel::currency(-0.001).value, 0.0);
        assert_eq!(PnlLabel::percent(0.004).value, 0.0);
    }
}
