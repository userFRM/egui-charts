//! Extension traits for common egui controls and layouts.
//!
//! Import [`UiExt`] for chart-styled buttons, icons, labels and spacing,
//! [`ResponseExt`] for interaction callbacks, or the widget extension traits for
//! sizing and corner rounding.

mod response_ext;
mod ui_ext;
mod widget_ext;

pub use response_ext::ResponseExt;
pub use ui_ext::UiExt;
pub use widget_ext::{ButtonWidgetExt, RoundingExt, TextEditWidgetExt};
