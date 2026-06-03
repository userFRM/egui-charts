//! Notification System
//!
//! Non-blocking toast notifications that appear and auto-dismiss.
//!
//! # Usage
//!
//! Each chart instance owns its own [`Toasts`] queue, so notifications never
//! leak across independent widgets sharing one process.
//!
//! ```ignore
//! use open_trading_charts::ui_kit::notifications::{NotificationPanel, NotificationPosition, Toasts};
//!
//! // In your state
//! struct MyState {
//!     toasts: Toasts,
//! }
//!
//! // Add a toast
//! state.toasts.success("Operation completed!");
//!
//! // Show notifications each frame
//! NotificationPanel::new()
//!     .position(NotificationPosition::TopRight)
//!     .show_with_toasts(ctx, &mut state.toasts);
//! ```

mod panel;
mod toast;
mod toasts;

pub use panel::{NotificationPanel, NotificationPanelConfig, NotificationPosition};
pub use toast::{Toast, ToastKind};
pub use toasts::Toasts;
