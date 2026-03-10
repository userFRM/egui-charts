//! Alert dialog tabs.

use crate::ui_kit::tab_bar::TabLabel;

/// Tab selection for alert type
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AlertTab {
    #[default]
    Price,
    Indicator,
    Volume,
}

impl AlertTab {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Price => "Price",
            Self::Indicator => "Indicator",
            Self::Volume => "Volume",
        }
    }

    pub fn all() -> [Self; 3] {
        [Self::Price, Self::Indicator, Self::Volume]
    }
}

impl TabLabel for AlertTab {
    fn tab_label(&self) -> &str {
        self.label()
    }
}
