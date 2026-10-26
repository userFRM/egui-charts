//! Tabs of the chart settings dialog.

/// Settings dialog tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    /// Symbol-specific settings (candle colors, precision, timezone)
    Symbol,
    /// Status line visibility options (OHLC, volume, change)
    StatusLine,
    /// Scale modes and price line settings
    ScalesAndLines,
    /// Canvas appearance (background, grid, crosshair, watermark, margins)
    Canvas,
    /// Buy and Sell buttons, instant orders, fill sounds, notifications, and the positions,
    /// orders and executions drawn on the chart
    Trading,
    /// Alert display settings
    Alerts,
    /// Corporate, economic and news event visibility.
    Events,
}

impl SettingsTab {
    /// Values in display order.
    pub fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::Symbol,
            SettingsTab::StatusLine,
            SettingsTab::ScalesAndLines,
            SettingsTab::Canvas,
            SettingsTab::Trading,
            SettingsTab::Alerts,
            SettingsTab::Events,
        ]
    }

    /// Display name for this value.
    pub fn name(&self) -> &'static str {
        match self {
            SettingsTab::Symbol => "Symbol",
            SettingsTab::StatusLine => "Status line",
            SettingsTab::ScalesAndLines => "Scales and lines",
            SettingsTab::Canvas => "Canvas",
            SettingsTab::Trading => "Trading",
            SettingsTab::Alerts => "Alerts",
            SettingsTab::Events => "Events",
        }
    }
}
