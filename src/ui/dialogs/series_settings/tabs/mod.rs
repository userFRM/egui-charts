//! Series settings tabs for the Settings dialog.
//!
//! Tabs: Symbol, Status line, Scales and lines, Canvas

mod canvas;
mod scales;
mod status_line;
mod symbol;

pub use canvas::CanvasTab;
pub use scales::ScalesTab;
pub use status_line::StatusLineTab;
pub use symbol::SymbolTab;

use crate::icons::{Icon, icons as embedded_icons};

/// Tab selection for series settings
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SeriesSettingsTab {
    #[default]
    Symbol,
    StatusLine,
    ScalesAndLines,
    Canvas,
}

impl SeriesSettingsTab {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Symbol => "Symbol",
            Self::StatusLine => "Status line",
            Self::ScalesAndLines => "Scales and lines",
            Self::Canvas => "Canvas",
        }
    }

    pub fn icon(&self) -> &'static Icon {
        match self {
            Self::Symbol => &embedded_icons::SETTINGS_SYMBOL,
            Self::StatusLine => &embedded_icons::SETTINGS_STATUS_LINE,
            Self::ScalesAndLines => &embedded_icons::SETTINGS_SCALES_LINES,
            Self::Canvas => &embedded_icons::SETTINGS_CANVAS,
        }
    }

    pub fn all() -> [Self; 4] {
        [
            Self::Symbol,
            Self::StatusLine,
            Self::ScalesAndLines,
            Self::Canvas,
        ]
    }
}
