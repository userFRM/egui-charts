//! Drawing properties tabs.

mod coordinates;
mod style;
mod text;
mod visibility;

pub use coordinates::{CoordinatesState, CoordinatesTab};
pub use style::StyleTab;
pub use text::TextTab;
pub use visibility::VisibilityTab;

use crate::ui_kit::tab_bar::TabLabel;

/// Tab selection for drawing properties
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PropertiesTab {
    #[default]
    Style,
    Coordinates,
    Visibility,
    Text,
}

impl PropertiesTab {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Style => "Style",
            Self::Coordinates => "Coordinates",
            Self::Visibility => "Visibility",
            Self::Text => "Text",
        }
    }

    pub fn all() -> [Self; 4] {
        [Self::Style, Self::Coordinates, Self::Visibility, Self::Text]
    }
}

impl TabLabel for PropertiesTab {
    fn tab_label(&self) -> &str {
        self.label()
    }
}
