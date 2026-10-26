//! Drawing properties tabs.

mod coordinates;
mod inputs;
mod style;
mod text;
mod visibility;

pub use coordinates::{CoordinatePoint, CoordinatesState, CoordinatesTab};
pub use style::StyleTab;
pub use text::TextTab;
pub use visibility::VisibilityTab;

use crate::ui_kit::tab_bar::TabLabel;

/// Tab selection for drawing properties
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PropertiesTab {
    #[default]
    /// Drawing line, fill and color settings.
    Style,
    /// Drawing anchor coordinates.
    Coordinates,
    /// Tool-specific levels, sizing and calculation settings.
    Inputs,
    /// Interval visibility settings.
    Visibility,
    /// Drawing text and its appearance.
    Text,
}

impl PropertiesTab {
    /// Display label for this value.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Style => "Style",
            Self::Coordinates => "Coordinates",
            Self::Inputs => "Inputs",
            Self::Visibility => "Visibility",
            Self::Text => "Text",
        }
    }

    /// Values in display order.
    pub fn all() -> [Self; 5] {
        [
            Self::Style,
            Self::Inputs,
            Self::Coordinates,
            Self::Visibility,
            Self::Text,
        ]
    }
}

impl TabLabel for PropertiesTab {
    fn tab_label(&self) -> &str {
        self.label()
    }
}

pub(super) use inputs::InputsTab;
