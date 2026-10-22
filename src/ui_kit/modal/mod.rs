//! Modal System
//!
//! A composable modal system with standard dialog patterns.
//!
//! # Usage
//!
//! ## Basic Modal
//!
//! ```
//! use egui_charts::ui_kit::modal::ModalHandler;
//!
//! // In your state
//! struct MyState {
//!     delete_modal: ModalHandler<u64>,
//! }
//! # let mut state = MyState { delete_modal: ModalHandler::new("delete_modal") };
//! # let item_id = 7;
//!
//! // Open the modal
//! state.delete_modal.open_with(item_id);
//!
//! // Show the modal
//! # egui::__run_test_ctx(|ctx| {
//! state.delete_modal.show(ctx, |ui, payload| {
//!     if let Some(id) = payload {
//!         ui.label(format!("Delete item {}?", id));
//!         if ui.button("Delete").clicked() {
//!             // Handle delete
//!         }
//!     }
//! });
//! # });
//! ```
//!
//! ## Confirm Dialog
//!
//! ```
//! use egui_charts::ui_kit::modal::dialogs::{ConfirmDialog, ConfirmResult};
//!
//! # egui::__run_test_ctx(|ctx| {
//! # let mut open = true;
//! let result = ConfirmDialog::new("delete_confirm", "Confirm Delete", "Are you sure?")
//!     .danger()
//!     .confirm_text("Delete")
//!     .show(ctx, &mut open);
//!
//! match result {
//!     ConfirmResult::Confirmed => { /* delete */ }
//!     ConfirmResult::Cancelled => { /* cancelled */ }
//!     ConfirmResult::None => { /* still open */ }
//! }
//! # });
//! ```
//!
//! ## Prompt Dialog
//!
//! ```
//! use egui_charts::ui_kit::modal::dialogs::{PromptDialog, PromptResult};
//!
//! # egui::__run_test_ctx(|ctx| {
//! # let mut open = true;
//! # let current_name = "My Layout";
//! let result = PromptDialog::new("rename_prompt", "Rename Item")
//!     .message("Enter a new name:")
//!     .initial_value(current_name)
//!     .show(ctx, &mut open);
//!
//! if let PromptResult::Submitted(new_name) = result {
//!     // Handle rename
//! }
//! # });
//! ```

pub mod dialogs;
mod modal;
mod modal_handler;

pub use modal::{Modal, ModalResponse, SimpleModal, show_modal};
pub use modal_handler::ModalHandler;
