//! Series settings tabs for the Settings dialog.
//!
//! Tabs: Symbol, Status line, Scales and lines, Canvas, Trading, Alerts, Events

mod alerts;
mod canvas;
mod events;
mod scales;
mod status_line;
mod symbol;
mod trading;

pub use alerts::AlertsTab;
pub use canvas::CanvasTab;
pub use events::EventsTab;
pub use scales::ScalesTab;
pub use status_line::StatusLineTab;
pub use symbol::SymbolTab;
pub use trading::TradingTab;

use crate::icons::{Icon, icons as embedded_icons};

/// Tab selection for series settings
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SeriesSettingsTab {
    #[default]
    Symbol,
    StatusLine,
    ScalesAndLines,
    Canvas,
    Trading,
    Alerts,
    Events,
}

impl SeriesSettingsTab {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Symbol => "Symbol",
            Self::StatusLine => "Status line",
            Self::ScalesAndLines => "Scales and lines",
            Self::Canvas => "Canvas",
            Self::Trading => "Trading",
            Self::Alerts => "Alerts",
            Self::Events => "Events",
        }
    }

    pub fn icon(&self) -> &'static Icon {
        match self {
            Self::Symbol => &embedded_icons::SETTINGS_SYMBOL,
            Self::StatusLine => &embedded_icons::SETTINGS_STATUS_LINE,
            Self::ScalesAndLines => &embedded_icons::SETTINGS_SCALES_LINES,
            Self::Canvas => &embedded_icons::SETTINGS_CANVAS,
            Self::Trading => &embedded_icons::SETTINGS,
            Self::Alerts => &embedded_icons::ALERTS,
            Self::Events => &embedded_icons::SETTINGS_EVENTS,
        }
    }

    pub fn all() -> [Self; 7] {
        [
            Self::Symbol,
            Self::StatusLine,
            Self::ScalesAndLines,
            Self::Canvas,
            Self::Trading,
            Self::Alerts,
            Self::Events,
        ]
    }
}
