//! Pine Script editor dialog.
//!
//! Provides a code editor with compile/run functionality, basic syntax
//! highlighting, error display, and an output panel for plot/strategy results.

mod editor;

pub use editor::{PineEditor, PineEditorAction};
