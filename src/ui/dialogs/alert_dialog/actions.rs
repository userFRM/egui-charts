//! Actions from the typed alert dialog.

use crate::model::Alert;

/// Dialog result; only a submitted alert reaches the host service.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum AlertDialogAction {
    /// No change.
    None,
    /// Dismissed without saving.
    Cancel,
    /// Create or replace the alert, identified by its optional host id.
    Create(Box<Alert>),
}
