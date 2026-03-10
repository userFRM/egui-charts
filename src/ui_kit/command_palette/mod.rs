//! Command Palette Component
//!
//! A searchable command palette (Ctrl+K / Cmd+K style) for quick command access.
//!
//! # Usage
//!
//! ```ignore
//! use open_trading_charts::ui_kit::command_palette::{Command, CommandPaletteHandler};
//! use egui::{Key, KeyboardShortcut, Modifiers};
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
//! if let Some(command_id) = handler.update(ctx) {
//!     match command_id.as_str() {
//!         "new_chart" => create_new_chart(),
//!         "settings" => open_settings(),
//!         "add_indicator" => show_indicator_dialog(),
//!         _ => {}
//!     }
//! }
//! ```

mod command;
mod fuzzy;
mod palette;

pub use command::Command;
pub use fuzzy::{FuzzyMatch, fuzzy_match, fuzzy_search};
pub use palette::{CommandPalette, CommandPaletteHandler};
