//! Trading label widgets — PnlLabel and SideLabel
//!
//! Pre-styled labels for common trading UI patterns:
//! - `PnlLabel` — auto-colored profit/loss display (green/red/neutral)
//! - `SideLabel` — colored buy/sell or long/short badges

use egui::{Response, RichText, Ui};

use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;

// =============================================================================
// PnlLabel
// =============================================================================

/// Auto-colored profit/loss label.
///
/// Colors text using the theme's bullish/bearish colors based on value sign.
///
/// ```ignore
/// PnlLabel::currency(123.45).show(ui);     // "+$123.45" in green
/// PnlLabel::currency(-50.0).show(ui);      // "-$50.00" in red
/// PnlLabel::percent(0.0534).show(ui);      // "+5.34%" in green
/// PnlLabel::value(-10.0, "custom").show(ui); // "custom" in red
/// ```
pub struct PnlLabel {
    value: f64,
    text: String,
    strong: bool,
    size: Option<f32>,
}

impl PnlLabel {
    /// Display a currency P&L value (e.g., "+$123.45")
    pub fn currency(value: f64) -> Self {
        let sign = if value >= 0.0 { "+" } else { "" };
        Self {
            value,
            text: format!("{}${:.2}", sign, value),
            strong: false,
            size: None,
        }
    }

    /// Display a percentage P&L value (e.g., "+5.34%")
    pub fn percent(value: f64) -> Self {
        let sign = if value >= 0.0 { "+" } else { "" };
        Self {
            value,
            text: format!("{}{:.2}%", sign, value),
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
        let color = ui.pnl_color(self.value);
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
/// Uses bullish color for buy/long, bearish for sell/short.
///
/// ```ignore
/// SideLabel::buy().show(ui);    // "BUY" in green
/// SideLabel::sell().show(ui);   // "SELL" in red
/// SideLabel::long().show(ui);   // "LONG" in green
/// SideLabel::short().show(ui);  // "SHORT" in red
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
        let color = if self.is_buy {
            DESIGN_TOKENS.semantic.extended.bullish
        } else {
            DESIGN_TOKENS.semantic.extended.bearish
        };
        let mut rt = RichText::new(&self.text).color(color);
        if let Some(s) = self.size {
            rt = rt.size(s);
        }
        ui.label(rt)
    }
}
