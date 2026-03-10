//! Notification System
//!
//! Non-blocking toast notifications that appear and auto-dismiss.
//!
//! # Usage
//!
//! ## Global Toasts
//!
//! ```ignore
//! use open_trading_charts::ui_kit::notifications::{add_toast, Toast, show_notifications};
//!
//! // Add a toast
//! add_toast(Toast::success("Item saved successfully!").duration(3.0));
//! add_toast(Toast::error("Failed to connect").permanent());
//!
//! // Show notifications each frame
//! fn update(ctx: &egui::Context) {
//!     show_notifications(ctx);
//! }
//! ```
//!
//! ## Local Toasts
//!
//! ```ignore
//! use open_trading_charts::ui_kit::notifications::{Toast, Toasts, NotificationPanel};
//!
//! // In your state
//! struct MyState {
//!     toasts: Toasts,
//! }
//!
//! // Add a toast
//! state.toasts.success("Operation completed!");
//!
//! // Show notifications
//! NotificationPanel::new()
//!     .position(NotificationPosition::TopRight)
//!     .show_with_toasts(ctx, &mut state.toasts);
//! ```
//!
//! ## Convenience Functions
//!
//! ```ignore
//! use open_trading_charts::ui_kit::notifications::{toast_info, toast_success, toast_warning, toast_error};
//!
//! toast_info("Processing...");
//! toast_success("Done!");
//! toast_warning("This might take a while");
//! toast_error("Something went wrong");
//! ```

mod panel;
mod toast;
mod toasts;

pub use panel::{
    NotificationPanel, NotificationPanelConfig, NotificationPosition, show_notifications,
    show_notifications_at,
};
pub use toast::{Toast, ToastKind};
pub use toasts::{
    Toasts, add_toast, cleanup_expired_toasts, clear_toasts, get_toasts, remove_toast, toast_count,
    toast_error, toast_info, toast_success, toast_warning,
};
