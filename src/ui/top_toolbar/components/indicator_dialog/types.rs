//! Study browser tabs and category labels.
//!
//! Use these values to select the dialog's section or group studies in a host picker.

/// Indicator tabs in the dialog sidebar
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorTab {
    /// Technical indicators (SMA, RSI, MACD, etc.)
    Indicators,
    /// Indicator profiles (saved combinations)
    Profiles,
    /// Chart patterns (head-and-shoulders, triangles, etc.)
    Patterns,
}

impl IndicatorTab {
    /// Return all available tabs
    pub fn all() -> &'static [IndicatorTab] {
        &[
            IndicatorTab::Indicators,
            IndicatorTab::Profiles,
            IndicatorTab::Patterns,
        ]
    }

    /// Get the display name for this tab
    pub fn name(&self) -> &'static str {
        match self {
            IndicatorTab::Indicators => "Indicators",
            IndicatorTab::Profiles => "Profiles",
            IndicatorTab::Patterns => "Patterns",
        }
    }
}

impl crate::ui_kit::tab_bar::TabLabel for IndicatorTab {
    fn tab_label(&self) -> &str {
        self.name()
    }
}

/// Indicator categories for filtering the indicator list
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorCategory {
    /// User-favorited indicators
    Favorites,
    /// Built-in platform indicators
    BuiltIns,
    /// Community-contributed indicators
    Community,
    /// Premium/paid indicators
    Premium,
}

impl IndicatorCategory {
    /// Return all available categories
    pub fn all() -> &'static [IndicatorCategory] {
        &[
            IndicatorCategory::Favorites,
            IndicatorCategory::BuiltIns,
            IndicatorCategory::Community,
            IndicatorCategory::Premium,
        ]
    }

    /// Get the display name for this category
    pub fn name(&self) -> &'static str {
        match self {
            IndicatorCategory::Favorites => "Favorites",
            IndicatorCategory::BuiltIns => "Built-ins",
            IndicatorCategory::Community => "Community",
            IndicatorCategory::Premium => "Premium",
        }
    }

    /// Get the sidebar icon for this category
    pub fn icon(&self) -> &'static crate::icons::Icon {
        use crate::icons::icons as embedded_icons;
        match self {
            IndicatorCategory::Favorites => &embedded_icons::STAR_EMPTY,
            IndicatorCategory::BuiltIns => &embedded_icons::INDICATORS,
            IndicatorCategory::Community => &embedded_icons::USERS,
            IndicatorCategory::Premium => &embedded_icons::CROWN,
        }
    }
}
