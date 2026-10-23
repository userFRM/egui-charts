//! Actions from the typed alert dialog.

use crate::model::Alert;

/// The complete alert handed to the host. Prefer [`Alert`] in new code.
pub type AlertData = Alert;

/// Dialog result; only a submitted alert reaches the host service.
#[derive(Clone, Debug)]
pub enum AlertDialogAction {
    /// No change.
    None,
    /// Dismissed without saving.
    Cancel,
    /// Create or replace the alert, identified by its optional host id.
    Create(Box<Alert>),
}
