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
#[non_exhaustive]
pub enum PrecisionMode {
    #[default]
    /// Default.
    Default,
    /// Whole numbers.
    Decimals0,
    /// Use 1 decimal place.
    Decimals1,
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
    /// Use 7 decimal places.
    Decimals7,
    /// Use 8 decimal places.
    Decimals8,
    /// Fractions of a whole unit, such as 32nds for `112'16`.
    Fraction(crate::model::PriceFraction),
    /// A number of decimal places the list lacks, such as one a host chose with
    /// [`PriceScale::set_price_precision`](crate::scales::PriceScale::set_price_precision).
    Other(u8),
}

impl PrecisionMode {
    /// Available values in menu order.
    pub fn all() -> &'static [PrecisionMode] {
        &[
            PrecisionMode::Default,
            PrecisionMode::Decimals0,
            PrecisionMode::Decimals1,
            PrecisionMode::Decimals2,
            PrecisionMode::Decimals3,
            PrecisionMode::Decimals4,
            PrecisionMode::Decimals5,
            PrecisionMode::Decimals6,
            PrecisionMode::Decimals7,
            PrecisionMode::Decimals8,
            PrecisionMode::Fraction(crate::model::PriceFraction::Halves),
            PrecisionMode::Fraction(crate::model::PriceFraction::Quarters),
            PrecisionMode::Fraction(crate::model::PriceFraction::Eighths),
            PrecisionMode::Fraction(crate::model::PriceFraction::Sixteenths),
            PrecisionMode::Fraction(crate::model::PriceFraction::ThirtySeconds),
            PrecisionMode::Fraction(crate::model::PriceFraction::HalfThirtySeconds),
            PrecisionMode::Fraction(crate::model::PriceFraction::QuarterThirtySeconds),
            PrecisionMode::Fraction(crate::model::PriceFraction::EighthThirtySeconds),
            PrecisionMode::Fraction(crate::model::PriceFraction::SixtyFourths),
            PrecisionMode::Fraction(crate::model::PriceFraction::OneTwentyEighths),
            PrecisionMode::Fraction(crate::model::PriceFraction::TwoFiftySixths),
        ]
    }

    /// Display name for settings menus.
    pub fn name(&self) -> std::borrow::Cow<'static, str> {
        match self {
            PrecisionMode::Default => "Default".into(),
            PrecisionMode::Decimals0 => "0 decimals".into(),
            PrecisionMode::Decimals1 => "1 decimal".into(),
            PrecisionMode::Decimals2 => "2 decimals".into(),
            PrecisionMode::Decimals3 => "3 decimals".into(),
            PrecisionMode::Decimals4 => "4 decimals".into(),
            PrecisionMode::Decimals5 => "5 decimals".into(),
            PrecisionMode::Decimals6 => "6 decimals".into(),
            PrecisionMode::Decimals7 => "7 decimals".into(),
            PrecisionMode::Decimals8 => "8 decimals".into(),
            PrecisionMode::Fraction(crate::model::PriceFraction::Halves) => "1/2".into(),
            PrecisionMode::Fraction(crate::model::PriceFraction::Quarters) => "1/4".into(),
            PrecisionMode::Fraction(crate::model::PriceFraction::Eighths) => "1/8".into(),
            PrecisionMode::Fraction(crate::model::PriceFraction::Sixteenths) => "1/16".into(),
            PrecisionMode::Fraction(crate::model::PriceFraction::ThirtySeconds) => "1/32".into(),
            PrecisionMode::Fraction(crate::model::PriceFraction::HalfThirtySeconds) => {
                "1/32 with halves".into()
            }
            PrecisionMode::Fraction(crate::model::PriceFraction::QuarterThirtySeconds) => {
                "1/32 with quarters".into()
            }
            PrecisionMode::Fraction(crate::model::PriceFraction::EighthThirtySeconds) => {
                "1/32 with eighths".into()
            }
            PrecisionMode::Fraction(crate::model::PriceFraction::SixtyFourths) => "1/64".into(),
            PrecisionMode::Fraction(crate::model::PriceFraction::OneTwentyEighths) => {
                "1/128".into()
            }
            PrecisionMode::Fraction(crate::model::PriceFraction::TwoFiftySixths) => "1/256".into(),
            PrecisionMode::Other(decimals) => format!("{decimals} decimals").into(),
        }
    }

    /// The scale's chosen fraction or decimals, or `Default` when neither is chosen.
    pub(crate) fn from_scale(scale: &crate::scales::PriceScale) -> Self {
        if let Some(fraction) = scale.chosen_fraction() {
            return Self::Fraction(fraction);
        }
        let Some(decimals) = scale.chosen_precision() else {
            return Self::Default;
        };
        Self::all()
            .iter()
            .copied()
            .find(|mode| mode.decimals() == Some(usize::from(decimals)))
            .unwrap_or(Self::Other(decimals))
    }

    /// The fraction prices are written in, if this mode is one.
    pub fn fraction(&self) -> Option<crate::model::PriceFraction> {
        match self {
            PrecisionMode::Fraction(fraction) => Some(*fraction),
            _ => None,
        }
    }

    /// Explicit decimals, or `None` for `Default`, which follows the instrument, and fractions.
    pub fn decimals(&self) -> Option<usize> {
        match self {
            PrecisionMode::Default | PrecisionMode::Fraction(_) => None,
            PrecisionMode::Decimals0 => Some(0),
            PrecisionMode::Decimals1 => Some(1),
            PrecisionMode::Decimals2 => Some(2),
            PrecisionMode::Decimals3 => Some(3),
            PrecisionMode::Decimals4 => Some(4),
            PrecisionMode::Decimals5 => Some(5),
            PrecisionMode::Decimals6 => Some(6),
            PrecisionMode::Decimals7 => Some(7),
            PrecisionMode::Decimals8 => Some(8),
            PrecisionMode::Other(decimals) => Some(usize::from(*decimals)),
        }
    }
}
