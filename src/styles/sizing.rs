//! Component sizing - Fixed Desktop Constants
//!
//! This module provides fixed desktop sizing constants.
//!
//! # Usage
//!
//! ```ignore
//! use crate::styles::sizing;
//!
//! // All sizing uses fixed desktop constants
//! let height = sizing::toolbar::TOP_HEIGHT;  // 38px
//! let width = sizing::toolbar::LEFT_WIDTH;   // 52px
//! ```

// =============================================================================
// Toolbar - Fixed Desktop Constants
// =============================================================================

/// Toolbar sizing - fixed desktop dimensions
pub mod toolbar {
    /// Top toolbar height: 38px
    pub const TOP_HEIGHT: f32 = 38.0;

    /// Left drawing toolbar width: 52px
    pub const LEFT_WIDTH: f32 = 52.0;

    /// Right widget bar width: 52px
    pub const RIGHT_WIDTH: f32 = 52.0;

    /// Bottom timeframe bar height: 38px
    pub const BOTTOM_HEIGHT: f32 = 38.0;

    /// Right panel width: 300px
    pub const RIGHT_PANEL_WIDTH: f32 = 300.0;

    /// Right panel min width: 180px
    pub const RIGHT_PANEL_MIN_WIDTH: f32 = 180.0;

    /// Right panel max width: 350px
    pub const RIGHT_PANEL_MAX_WIDTH: f32 = 350.0;

    /// Button size: 32px (desktop)
    pub const BUTTON_SIZE: f32 = 32.0;

    /// Icon size: 18px (desktop)
    pub const ICON_SIZE: f32 = 18.0;
}

// =============================================================================
// Auth Dialog - Fixed Desktop Constants
// =============================================================================

/// Auth dialog sizing - fixed desktop dimensions
pub mod auth_dialog {
    /// Dialog width: 360px
    pub const WIDTH: f32 = 360.0;

    /// Button height: 36px (pointer mode)
    pub const BUTTON_HEIGHT: f32 = 36.0;

    /// Input field height: 32px (pointer mode)
    pub const INPUT_HEIGHT: f32 = 32.0;

    /// Dialog padding: 24px
    pub const PADDING: f32 = 24.0;
}

// =============================================================================
// Footprint Chart - Fixed Desktop Constants
// =============================================================================

/// Footprint chart sizing - fixed desktop dimensions
pub mod footprint {
    /// Minimum cell height: 14px
    pub const MIN_CELL_HEIGHT: f32 = 14.0;

    /// Volume text font size: 10px
    pub const VOLUME_FONT_SIZE: f32 = 10.0;

    /// Delta text font size: 11px
    pub const DELTA_FONT_SIZE: f32 = 11.0;

    /// POC line width: 2px
    pub const POC_LINE_WIDTH: f32 = 2.0;

    /// Imbalance indicator width: 3px
    pub const IMBALANCE_INDICATOR_WIDTH: f32 = 3.0;

    /// Minimum bar width: 40px
    pub const MIN_BAR_WIDTH: f32 = 40.0;

    /// Cell padding: 2px
    pub const CELL_PADDING: f32 = 2.0;
}

// =============================================================================
// Chart Layout - Fixed Desktop Constants
// =============================================================================

/// Chart layout sizing - fixed desktop padding and margins
pub mod chart {
    /// Chart padding: 40px (desktop)
    pub const PADDING: f32 = 40.0;

    /// Top padding with OHLC: 40px
    pub const TOP_PADDING_WITH_OHLC: f32 = 40.0;

    /// Top padding without OHLC: 20px
    pub const TOP_PADDING_NO_OHLC: f32 = 20.0;

    /// Bottom padding with time labels: 30px
    pub const BOTTOM_PADDING_WITH_TIME: f32 = 30.0;

    /// Bottom padding without time labels: 20px
    pub const BOTTOM_PADDING_NO_TIME: f32 = 20.0;

    /// Right axis width: 70px
    pub const RIGHT_AXIS_WIDTH: f32 = 70.0;

    /// Minimum chart width: 100px
    pub const MIN_CHART_WIDTH: f32 = 100.0;

    /// Minimum chart height: 150px
    pub const MIN_CHART_HEIGHT: f32 = 150.0;
}

// =============================================================================
// Indicator Pane - Constants for indicator panel heights
// =============================================================================

/// Indicator pane sizing constants
pub mod indicator {
    /// Oscillator pane height (RSI, MACD, etc.)
    pub const OSCILLATOR_HEIGHT: f32 = 100.0;

    /// Multi-line indicator height (multiple overlays)
    pub const MULTI_LINE_HEIGHT: f32 = 120.0;
}

// =============================================================================
// Candle - Helper functions (static values in DESIGN_TOKENS)
// =============================================================================

/// Candlestick helper functions
pub mod candle {
    use crate::tokens::DESIGN_TOKENS;
    use egui::Color32;

    /// Calculate bar width from bar spacing
    #[inline]
    pub fn bar_width_from_spacing(bar_spacing: f32) -> f32 {
        bar_spacing * DESIGN_TOKENS.sizing.candle.body_width_ratio
    }

    /// Get wick width based on pixel density
    #[inline]
    pub fn wick_width_for_dpi(pixels_per_point: f32) -> f32 {
        if pixels_per_point > 1.5 {
            DESIGN_TOKENS.sizing.candle.wick_width_hidpi
        } else {
            DESIGN_TOKENS.sizing.candle.wick_width
        }
    }

    /// Apply minimum body height
    #[inline]
    pub fn ensure_min_body_height(body_top: f32, body_bottom: f32) -> f32 {
        body_bottom.max(body_top + DESIGN_TOKENS.sizing.candle.min_body_height)
    }

    /// Apply volume alpha to a color
    #[inline]
    pub fn with_volume_alpha(color: Color32) -> Color32 {
        Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            DESIGN_TOKENS.sizing.candle.volume_alpha,
        )
    }
}
