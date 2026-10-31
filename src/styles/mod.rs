//! UI sizing, typography, icon sizes and keyboard focus helpers.
//!
//! UI colours, text, spacing and radii come from the applied [`Theme`](crate::theme::Theme).
//! These modules retain fixed component dimensions and the default font sizes used by
//! the theme. See `docs/theme.md` for runtime customisation.

pub mod focus;
pub mod icons;
#[cfg(feature = "ui")]
pub(crate) mod responsive;
#[cfg(feature = "ui")]
#[cfg_attr(docsrs, doc(cfg(feature = "ui")))]
pub use responsive::LayoutContext;
pub mod sizing;
pub mod typography;

/// Give the scroll areas of `ui` the menus' thin scroll bar: 3 px at the right edge, in its own
/// column beside the rows, with a handle at 43 % of the text colour.
///
/// Floating bars are the ones egui draws with the handle opacities; the column they allocate
/// keeps the bar off the rows.
#[cfg(feature = "ui")]
pub(crate) fn menu_scroll_bar(ui: &mut egui::Ui) {
    let scroll = &mut ui.spacing_mut().scroll;
    scroll.floating = true;
    scroll.bar_width = 3.0;
    scroll.floating_width = 3.0;
    scroll.floating_allocated_width = 3.0 + scroll.bar_inner_margin;
    scroll.foreground_color = true;
    scroll.dormant_handle_opacity = 0.43;
    scroll.active_handle_opacity = 0.43;
    scroll.dormant_background_opacity = 0.0;
    scroll.active_background_opacity = 0.0;
    scroll.interact_background_opacity = 0.0;
    scroll.fade.strength = 0.0;
}

// =============================================================================
// Stroke Helpers
// =============================================================================

/// Common stroke width constants (legacy -- prefer `DESIGN_TOKENS.stroke.*`).
pub mod stroke {
    /// Hairline stroke (1px)
    pub const HAIRLINE: f32 = 1.0;

    /// Thin stroke (1.2px)
    pub const THIN: f32 = 1.2;

    /// Medium stroke (1.5px)
    pub const MEDIUM: f32 = 1.5;

    /// Thick stroke (2px)
    pub const THICK: f32 = 2.0;
}
