//! Modal System
//!
//! A composable modal system with standard dialog patterns.
//!
//! # Usage
//!
//! ## Basic Modal
//!
//! ```ignore
//! use open_trading_charts::ui_kit::modal::{Modal, ModalHandler};
//!
//! // In your state
//! struct MyState {
//!     delete_modal: ModalHandler<ItemId>,
//! }
//!
//! // Open the modal
//! if delete_btn.clicked() {
//!     state.delete_modal.open_with(item_id);
//! }
//!
//! // Show the modal
//! state.delete_modal.show(ctx, |ui, payload| {
//!     if let Some(id) = payload {
//!         ui.label(format!("Delete item {}?", id));
//!         if ui.button("Delete").clicked() {
//!             // Handle delete
//!         }
//!     }
//! });
//! ```
//!
//! ## Confirm Dialog
//!
//! ```ignore
//! use open_trading_charts::ui_kit::modal::dialogs::{ConfirmDialog, ConfirmResult};
//!
//! let result = ConfirmDialog::new("delete_confirm", "Confirm Delete", "Are you sure?")
//!     .danger()
//!     .confirm_text("Delete")
//!     .show(ctx, &mut open);
//!
//! match result {
//!     ConfirmResult::Confirmed => { /* delete */ },
//!     ConfirmResult::Cancelled => { /* cancelled */ },
//!     ConfirmResult::None => { /* still open */ },
//! }
//! ```
//!
//! ## Prompt Dialog
//!
//! ```ignore
//! use open_trading_charts::ui_kit::modal::dialogs::{PromptDialog, PromptResult};
//!
//! let result = PromptDialog::new("rename_prompt", "Rename Item")
//!     .message("Enter a new name:")
//!     .initial_value(&current_name)
//!     .show(ctx, &mut open);
//!
//! if let PromptResult::Submitted(new_name) = result {
//!     // Handle rename
//! }
//! ```

pub mod dialogs;
mod modal;
mod modal_handler;

pub use modal::{Modal, ModalResponse, SimpleModal, show_modal};
pub use modal_handler::ModalHandler;
