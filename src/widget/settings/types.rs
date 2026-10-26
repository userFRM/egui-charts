//! Values of the chart display settings.

/// Background type for chart
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum BackgroundType {
    #[default]
    /// Solid.
    Solid,
    /// Vertical gradient.
    VerticalGradient,
    /// Horizontal gradient.
    HorizontalGradient,
}

impl BackgroundType {
    /// Available values in menu order.
    pub fn all() -> &'static [BackgroundType] {
        &[
            BackgroundType::Solid,
            BackgroundType::VerticalGradient,
            BackgroundType::HorizontalGradient,
        ]
    }

    /// Display name for settings menus.
    pub fn name(&self) -> &'static str {
        match self {
            BackgroundType::Solid => "Solid",
            BackgroundType::VerticalGradient => "Gradient vertical",
            BackgroundType::HorizontalGradient => "Gradient horizontal",
        }
    }
}

/// Grid lines display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum GridLinesMode {
    #[default]
    /// None.
    None,
    /// Horizontal.
    Horizontal,
    /// Vertical.
    Vertical,
    /// Both.
    Both,
}

impl GridLinesMode {
    /// Available values in menu order.
    pub fn all() -> &'static [GridLinesMode] {
        &[
            GridLinesMode::None,
            GridLinesMode::Horizontal,
            GridLinesMode::Vertical,
            GridLinesMode::Both,
        ]
    }

    /// Display name for settings menus.
    pub fn name(&self) -> &'static str {
        match self {
            GridLinesMode::None => "None",
            GridLinesMode::Horizontal => "Horizontal",
            GridLinesMode::Vertical => "Vertical",
            GridLinesMode::Both => "Both",
        }
    }
}

/// Button visibility mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ButtonVisibility {
    /// Always visible.
    AlwaysVisible,
    #[default]
    /// Visible on mouse over.
    VisibleOnMouseOver,
    /// Hidden.
    Hidden,
}

impl ButtonVisibility {
    /// Available values in menu order.
    pub fn all() -> &'static [ButtonVisibility] {
        &[
            ButtonVisibility::AlwaysVisible,
            ButtonVisibility::VisibleOnMouseOver,
            ButtonVisibility::Hidden,
        ]
    }

    /// Display name for settings menus.
    pub fn name(&self) -> &'static str {
        match self {
            ButtonVisibility::AlwaysVisible => "Always visible",
            ButtonVisibility::VisibleOnMouseOver => "Visible on mouse over",
            ButtonVisibility::Hidden => "Hidden",
        }
    }
}

/// Watermark display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum WatermarkMode {
    #[default]
    /// Replay mode.
    ReplayMode,
    /// Symbol.
    Symbol,
    /// Symbol and description.
    SymbolAndDescription,
    /// Custom.
    Custom,
}

impl WatermarkMode {
    /// Available values in menu order.
    pub fn all() -> &'static [WatermarkMode] {
        &[
            WatermarkMode::ReplayMode,
            WatermarkMode::Symbol,
            WatermarkMode::SymbolAndDescription,
            WatermarkMode::Custom,
        ]
    }

    /// Display name for settings menus.
    pub fn name(&self) -> &'static str {
        match self {
            WatermarkMode::ReplayMode => "Hidden",
            WatermarkMode::Symbol => "Symbol",
            WatermarkMode::SymbolAndDescription => "Symbol and desc",
            WatermarkMode::Custom => "Custom",
        }
    }
}

/// Precision mode for price display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PrecisionMode {
    #[default]
    /// Default.
    Default,
    /// Use 2 decimal places.
    Decimals2,
    /// Use 3 decimal places.
    Decimals3,
    /// Use 4 decimal places.
    Decimals4,
    /// Use 5 decimal places.
    Decimals5,
    /// Use 6 decimal places.
    Decimals6,
    /// Use 8 decimal places.
    Decimals8,
}

impl PrecisionMode {
    /// Available values in menu order.
    pub fn all() -> &'static [PrecisionMode] {
        &[
            PrecisionMode::Default,
            PrecisionMode::Decimals2,
            PrecisionMode::Decimals3,
            PrecisionMode::Decimals4,
            PrecisionMode::Decimals5,
            PrecisionMode::Decimals6,
            PrecisionMode::Decimals8,
        ]
    }

    /// Display name for settings menus.
    pub fn name(&self) -> &'static str {
        match self {
            PrecisionMode::Default => "Default",
            PrecisionMode::Decimals2 => "2 decimals",
            PrecisionMode::Decimals3 => "3 decimals",
            PrecisionMode::Decimals4 => "4 decimals",
            PrecisionMode::Decimals5 => "5 decimals",
            PrecisionMode::Decimals6 => "6 decimals",
            PrecisionMode::Decimals8 => "8 decimals",
        }
    }

    /// Explicit precision, or None to follow the instrument.
    pub fn decimals(&self) -> Option<usize> {
        match self {
            PrecisionMode::Default => None,
            PrecisionMode::Decimals2 => Some(2),
            PrecisionMode::Decimals3 => Some(3),
            PrecisionMode::Decimals4 => Some(4),
            PrecisionMode::Decimals5 => Some(5),
            PrecisionMode::Decimals6 => Some(6),
            PrecisionMode::Decimals8 => Some(8),
        }
    }
}
