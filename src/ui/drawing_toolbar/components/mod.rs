//! Reusable UI components for the drawing toolbar.
//!
//! This module provides DRY components used throughout the toolbar:
//! - `PairButton` - Split button with icon and arrow (used for categories, dropdowns)
//! - `icon_btn` - Single icon btns with theme-aware styling
//! - `separator` - Visual separator lines
//! - `svg_helpers` - SVG rendering utilities with theme colors

pub mod icon_button;
pub mod pair_button;
pub mod separator;
pub mod svg_helpers;

// Re-export all public components
pub use icon_button::{draw_icon_btn, draw_icon_button_padded, draw_toggle_btn, draw_tool_button};
pub use pair_button::{ArrowDirection, PairButton, PairButtonResponse, draw_arrow};
pub use separator::{draw_separator, draw_separator_styled, draw_separator_with_margin};
pub use svg_helpers::{render_svg_at_rect_themed, render_svg_centered};
