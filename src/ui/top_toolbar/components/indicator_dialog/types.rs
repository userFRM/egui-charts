//! Study browser tabs and category labels.
//!
//! Use these values to select the dialog's section or group studies in a host picker.

/// The pages of the built-in studies, chosen above the results. A page is offered while the
/// dialog's factory registers studies for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorTab {
    /// Technical indicators (SMA, RSI, MACD, etc.)
    Indicators,
    /// Volume and time price profiles, in [`IndicatorFactory::PROFILES`](crate::studies::IndicatorFactory::PROFILES)
    Profiles,
    /// Pattern detectors, in [`IndicatorFactory::PATTERNS`](crate::studies::IndicatorFactory::PATTERNS)
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

/// Indicator categories for filtering the indicator list. The sidebar offers favourites and
/// built-ins always, and the user's scripts and community studies while the factory registers
/// some.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorCategory {
    /// User-favorited indicators
    Favorites,
    /// The user's own studies, in [`IndicatorFactory::PERSONAL`](crate::studies::IndicatorFactory::PERSONAL)
    MyScripts,
    /// Built-in platform indicators
    BuiltIns,
    /// Community-contributed indicators, in [`IndicatorFactory::COMMUNITY`](crate::studies::IndicatorFactory::COMMUNITY)
    Community,
    /// Premium/paid indicators
    Premium,
}

impl IndicatorCategory {
    /// Return all available categories
    pub fn all() -> &'static [IndicatorCategory] {
        &[
            IndicatorCategory::Favorites,
            IndicatorCategory::MyScripts,
            IndicatorCategory::BuiltIns,
            IndicatorCategory::Community,
            IndicatorCategory::Premium,
        ]
    }

    /// Get the display name for this category
    pub fn name(&self) -> &'static str {
        match self {
            IndicatorCategory::Favorites => "Favorites",
            IndicatorCategory::MyScripts => "My scripts",
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
            IndicatorCategory::MyScripts => &embedded_icons::CODE,
            IndicatorCategory::BuiltIns => &embedded_icons::INDICATORS,
            IndicatorCategory::Community => &embedded_icons::USERS,
            IndicatorCategory::Premium => &embedded_icons::CROWN,
        }
    }
}
