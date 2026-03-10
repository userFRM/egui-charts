//! Actions from the drawing properties dialog.

use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Unique identifier for a drawing
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DrawingId(pub usize);

/// Line style options
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DrawingLineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl DrawingLineStyle {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Dashed => "Dashed",
            Self::Dotted => "Dotted",
        }
    }

    pub fn all() -> [Self; 3] {
        [Self::Solid, Self::Dashed, Self::Dotted]
    }
}

/// Drawing properties that can be edited
#[derive(Clone, Debug)]
pub struct DrawingProps {
    /// Primary color
    pub color: Color32,
    /// Line width (1-10)
    pub line_width: f32,
    /// Line style
    pub line_style: DrawingLineStyle,
    /// Fill color (for shapes)
    pub fill_color: Option<Color32>,
    /// Fill enabled
    pub fill_enabled: bool,
    /// Extend line left
    pub extend_left: bool,
    /// Extend line right
    pub extend_right: bool,
    /// Show price labels
    pub show_price: bool,
    /// Text label
    pub label: String,
    /// Font size
    pub font_size: f32,
    /// Visible on all timeframes
    pub visible_all_timeframes: bool,
    /// Specific timeframes (if not all)
    pub visible_timeframes: Vec<String>,
}

impl Default for DrawingProps {
    fn default() -> Self {
        Self {
            color: DESIGN_TOKENS.semantic.extended.accent,
            line_width: 1.0,
            line_style: DrawingLineStyle::Solid,
            fill_color: None,
            fill_enabled: false,
            extend_left: false,
            extend_right: false,
            show_price: false,
            label: String::new(),
            font_size: 12.0,
            visible_all_timeframes: true,
            visible_timeframes: Vec::new(),
        }
    }
}

/// Actions from the drawing properties dialog
#[derive(Clone, Debug)]
pub enum DrawingPropertiesAction {
    None,
    Cancel,
    Apply(DrawingId, DrawingProps),
    Delete(DrawingId),
    Clone(DrawingId),
}
