//! Configuration for the alert creation dialog.

use crate::tokens::DESIGN_TOKENS;

/// Configuration for the alert dialog
#[derive(Clone, Debug)]
pub struct AlertDialogConfig {
    /// Dialog width
    pub width: f32,
    /// Maximum scrolling body height
    pub height: f32,
}

impl Default for AlertDialogConfig {
    fn default() -> Self {
        Self {
            width: DESIGN_TOKENS.sizing.dialog.alert_width,
            height: 580.0,
        }
    }
}
