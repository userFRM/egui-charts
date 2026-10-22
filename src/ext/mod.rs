//! Extension traits for egui types -- the ergonomic API layer.
//!
//! This module provides convenience extension traits that add domain-specific
//! methods to egui's core types, following the patterns established by the
//! rerun viewer. These traits are the recommended way to render common UI
//! patterns and handle interactions. Theme colours come from `ui.visuals()` and
//! [`ThemeExt::active_theme`](crate::theme::ThemeExt::active_theme).
//!
//! # Traits
//!
//! | Trait | Extended Type | Purpose |
//! |---|---|---|
//! | [`UiExt`] | `egui::Ui` | Methods for icons, labels, buttons, layout and spacing |
//! | [`ResponseExt`] | `egui::Response` | Click/hover handlers, cursor control |
//! | [`ButtonWidgetExt`] | `egui::Button` | `fill_width()`, `min_width()`, `exact_size()` |
//! | [`TextEditWidgetExt`] | `egui::TextEdit` | `fill_width()`, `fixed_width()`, `char_limit()` |
//! | [`RoundingExt`] | Widgets with rounding | Semantic rounding helpers |
//!
//! # Usage
//!
//! ```rust
//! use egui_charts::ext::{UiExt, ResponseExt};
//!
//! fn my_widget(ui: &mut egui::Ui) {
//!     // UiExt: convenience UI elements
//!     ui.section_header("Settings");
//!     ui.error_label("Something went wrong");
//!     let r = ui.primary_button("Save");
//!
//!     // ResponseExt: interaction helpers
//!     r.on_click(|| println!("Saved!"));
//! }
//! ```

mod response_ext;
mod ui_ext;
mod widget_ext;

pub use response_ext::ResponseExt;
pub use ui_ext::UiExt;
pub use widget_ext::{ButtonWidgetExt, RoundingExt, TextEditWidgetExt};
