//! Toast notifications with optional automatic dismissal.
//!
//! Keep a [`Toasts`] queue in host state, add messages as events occur and call
//! [`NotificationPanel::show_with_toasts`] each frame to display them. Two or more toasts
//! collapse into a stack showing the newest; a pill expands it and a button beside it dismisses
//! every dismissible toast.
//!
//! Each toast is announced by screen readers on Linux, Windows and macOS as a live region (polite,
//! or assertive for errors); on the web nothing is announced, since eframe does not forward
//! accessibility updates there.

mod panel;
mod toast;
mod toasts;

pub use panel::{NotificationPanel, NotificationPanelConfig, NotificationPosition};
pub use toast::{Toast, ToastKind, TradingToast};
pub use toasts::Toasts;
