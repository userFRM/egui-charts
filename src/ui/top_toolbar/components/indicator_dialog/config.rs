//! Configuration for indicator dialog appearance and behavior

use crate::theme::Theme;
use egui::Color32;

/// Configuration for the indicator dialog
///
/// Each colour left `Color32::TRANSPARENT`, as in the default, follows the theme passed to
/// [`IndicatorDialog::show`](super::IndicatorDialog::show).
#[derive(Debug, Clone)]
pub struct IndicatorDialogConfig {
    /// Outer width of the dialog
    pub width: f32,
    /// Minimum outer height of the dialog, clamped to the window
    pub height: f32,
    /// Indicator names
    pub text_color: Color32,
    /// Indicator descriptions, badges and notes
    pub muted_color: Color32,
    /// Fill of the row under the pointer
    pub hover_color: Color32,
    /// Fill of the selected row
    pub sel_color: Color32,
    /// Star of a favourite indicator
    pub favorite_color: Color32,
}

impl IndicatorDialogConfig {
    /// Create config from `theme`
    ///
    /// Sets every colour from `theme`, so they no longer follow a theme change; start from
    /// `default()` to follow the theme.
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.ui;
        Self {
            text_color: ui.text,
            muted_color: ui.text_secondary,
            hover_color: ui.btn_bg_hover,
            sel_color: ui.selection_bg,
            favorite_color: ui.warning,
            ..Self::default()
        }
    }

    /// This configuration with each `TRANSPARENT` colour taken from `theme`.
    pub(super) fn resolved(&self, theme: &Theme) -> Self {
        let themed = Self::from_theme(theme);
        let or = |color: Color32, theme: Color32| {
            if color == Color32::TRANSPARENT {
                theme
            } else {
                color
            }
        };
        Self {
            text_color: or(self.text_color, themed.text_color),
            muted_color: or(self.muted_color, themed.muted_color),
            hover_color: or(self.hover_color, themed.hover_color),
            sel_color: or(self.sel_color, themed.sel_color),
            favorite_color: or(self.favorite_color, themed.favorite_color),
            ..self.clone()
        }
    }
}

impl Default for IndicatorDialogConfig {
    fn default() -> Self {
        Self {
            width: 750.0,
            height: 600.0,
            text_color: Color32::TRANSPARENT,
            muted_color: Color32::TRANSPARENT,
            hover_color: Color32::TRANSPARENT,
            sel_color: Color32::TRANSPARENT,
            favorite_color: Color32::TRANSPARENT,
        }
    }
}
