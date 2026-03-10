//! Configuration for the chart control bar.

use crate::tokens::DESIGN_TOKENS;

/// Configuration for the floating chart control bar
#[derive(Clone, Debug)]
pub struct ChartControlBarConfig {
    /// Button size (square)
    pub button_size: f32,
    /// Gap between buttons
    pub button_gap: f32,
    /// Bar padding
    pub padding: f32,
    /// Bar corner radius
    pub rounding: f32,
    /// Offset from bottom-right corner
    pub offset_x: f32,
    pub offset_y: f32,
}

impl Default for ChartControlBarConfig {
    fn default() -> Self {
        Self {
            button_size: DESIGN_TOKENS.sizing.button_sm,
            button_gap: DESIGN_TOKENS.spacing.xs,
            padding: DESIGN_TOKENS.spacing.sm,
            rounding: DESIGN_TOKENS.rounding.button,
            offset_x: DESIGN_TOKENS.spacing.lg,
            offset_y: DESIGN_TOKENS.spacing.lg,
        }
    }
}
