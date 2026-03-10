//! Standard Dialog Components
//!
//! Pre-built dialog patterns for common use cases.

mod confirm;
mod prompt;

pub use confirm::{ConfirmDialog, ConfirmResult, confirm, confirm_danger};
pub use prompt::{PromptDialog, PromptResult, prompt, prompt_with_message};
