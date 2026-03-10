//! Extension traits for egui types -- the ergonomic API layer.
//!
//! This module provides convenience extension traits that add domain-specific
//! methods to egui's core types, following the patterns established by the
//! rerun viewer. These traits are the recommended way to access theme-aware
//! colors, render common UI patterns, and handle interactions.
//!
//! # Traits
//!
//! | Trait | Extended Type | Purpose |
//! |---|---|---|
//! | [`UiExt`] | `egui::Ui` | 60+ methods for icons, labels, buttons, layout, spacing |
//! | [`ContextExt`] | `egui::Context` | Dark mode detection, device type queries |
//! | [`ResponseExt`] | `egui::Response` | Click/hover handlers, cursor control |
//! | [`HasDesignTokens`] | `egui::Context` / `egui::Ui` | Theme-aware color access |
//! | [`ButtonWidgetExt`] | `egui::Button` | `fill_width()`, `min_width()`, `exact_size()` |
//! | [`TextEditWidgetExt`] | `egui::TextEdit` | `fill_width()`, `fixed_width()`, `char_limit()` |
//! | [`RoundingExt`] | Widgets with rounding | Semantic rounding helpers |
//!
//! # Usage
//!
//! ```rust,ignore
//! use egui_charts::ext::{UiExt, ContextExt, ResponseExt, HasDesignTokens};
//!
//! fn my_widget(ui: &mut egui::Ui) {
//!     // UiExt: convenience UI elements
//!     ui.section_header("Settings");
//!     ui.error_label("Something went wrong");
//!     let r = ui.primary_button("Save");
//!
//!     // HasDesignTokens: theme-aware colors
//!     let bg = ui.chart_bg();
//!     let bullish = ui.bullish_color();
//!
//!     // ContextExt: global queries
//!     let is_dark = ui.ctx().is_dark_mode();
//!
//!     // ResponseExt: interaction helpers
//!     r.on_click(|| println!("Saved!"));
//! }
//! ```

mod context_ext;
mod design_tokens;
mod response_ext;
mod ui_ext;
mod widget_ext;

pub use context_ext::ContextExt;
pub use design_tokens::{DesignTokens, HasDesignTokens};
pub use response_ext::ResponseExt;
pub use ui_ext::UiExt;
pub use widget_ext::{ButtonWidgetExt, RoundingExt, TextEditWidgetExt};
