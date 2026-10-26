//! Confirmation and text-entry dialogs.
//!
//! Use [`ConfirmDialog`] for a decision or [`PromptDialog`] for text input; their
//! result enums distinguish submission, cancellation and an ongoing interaction.

mod confirm;
mod prompt;

pub use confirm::{ConfirmDialog, ConfirmResult, confirm, confirm_danger};
pub use prompt::{PromptDialog, PromptResult, prompt, prompt_with_message};
