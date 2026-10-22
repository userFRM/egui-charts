//! Command Palette Component
//!
//! A searchable command palette (Ctrl+K / Cmd+K style) for quick command access.
//!
//! # Usage
//!
//! ```
//! use egui::{Key, KeyboardShortcut, Modifiers};
//! use egui_charts::ui_kit::command_palette::{Command, CommandPaletteHandler};
//!
//! // Create a handler with commands
//! let mut handler = CommandPaletteHandler::new(vec![
//!     Command::new("new_chart", "New Chart")
//!         .shortcut(KeyboardShortcut::new(Modifiers::COMMAND, Key::N))
//!         .category("File"),
//!     Command::new("settings", "Open Settings")
//!         .category("Preferences"),
//!     Command::new("add_indicator", "Add Indicator")
//!         .category("Trading"),
//! ]);
//!
//! // In your update loop
//! # egui::__run_test_ctx(|ctx| {
//! if let Some(command_id) = handler.update(ctx) {
//!     match command_id.as_str() {
//!         "new_chart" => { /* create a new chart */ }
//!         "settings" => { /* open settings */ }
//!         "add_indicator" => { /* show the indicator dialog */ }
//!         _ => {}
//!     }
//! }
//! # });
//! ```

mod command;
mod fuzzy;
mod palette;

pub use command::Command;
pub use fuzzy::{FuzzyMatch, fuzzy_match, fuzzy_search};
pub use palette::{CommandPalette, CommandPaletteHandler};
