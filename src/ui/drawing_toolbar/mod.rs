//! Drawing Toolbar.
//!
//! A professional left sidebar with categorized drawing tools.
//!
//! # Architecture
//!
//! ```text
//! drawing_toolbar/
//! ├── mod.rs              # This file - module exports
//! ├── toolbar.rs          # Main DrawingToolbar coordinator
//! ├── actions.rs          # Action enum for toolbar events
//! ├── state.rs            # Toolbar state management
//! ├── config.rs           # Toolbar configuration
//! ├── icons.rs            # Icon mappings for tools
//! ├── data.rs             # Tool data definitions
//! ├── shortcuts.rs        # Keyboard shortcuts
//! ├── submenu_builder.rs  # Submenu builder framework
//! │
//! ├── components/         # Reusable UI components
//! │   ├── pair_btn.rs  # Split icon+arrow button
//! │   ├── icon_btn.rs  # Single icon btns
//! │   ├── separator.rs    # Visual separators
//! │   └── svg_helpers.rs  # SVG rendering utilities
//! │
//! ├── categories/         # Tool category modules
//! │   ├── cursors.rs      # Cursor tools
//! │   ├── trend_lines.rs  # Lines, channels, pitchforks
//! │   ├── fibonacci.rs    # Fibonacci & Gann tools
//! │   ├── patterns.rs     # Patterns, Elliott, cycles
//! │   ├── projection.rs   # Projection, volume, measurer
//! │   ├── shapes.rs       # Brushes, arrows, shapes
//! │   ├── annotation.rs   # Text & annotations
//! │   └── icons_emojis.rs # Icons/emoji picker
//! │
//! └── utilities/          # Bottom toolbar utilities
//!     ├── measure.rs      # Measure tool
//!     ├── zoom.rs         # Zoom in/out
//!     ├── magnet.rs       # Magnet mode dropdown
//!     ├── stay_in_drawing.rs # Keep drawing toggle
//!     ├── lock.rs         # Lock drawings toggle
//!     ├── hide_menu.rs    # Hide objects dropdown
//!     ├── remove_menu.rs  # Remove objects dropdown
//!     └── favorites.rs    # Favorites toggle
//! ```
//!
//! # Example
//!
//! ```ignore
//! use egui_open_trading_charts_rs::ui::drawing_toolbar::{DrawingToolbar, DrawingToolbarConfig};
//!
//! let mut toolbar = DrawingToolbar::default();
//!
//! // In your UI update loop:
//! let action = toolbar.show_with_action(ui);
//! match action {
//!     DrawingToolbarAction::SelectTool(tool) => {
//!         // Handle tool selection
//!     }
//!     _ => {}
//! }
//! ```

// Core module declarations
pub mod actions;
pub mod config;
pub mod data;
pub mod icons;
pub mod shortcuts;
pub mod state;
pub mod submenu_builder;
mod toolbar;

// Modular components and utilities
pub mod categories;
pub mod components;
pub mod utilities;

/// Minimal drawing template placeholder (frontend services not yet available)
#[derive(Debug, Clone)]
pub struct DrawingTemplate {
    pub id: String,
    pub name: String,
    pub drawing_type: String,
    pub is_default: bool,
}

// Re-export public types for convenient access
pub use actions::DrawingToolbarAction;
pub use components::{PairButton, draw_icon_btn, draw_separator, draw_toggle_btn};
pub use config::DrawingToolbarConfig;
pub use shortcuts::DrawingToolShortcuts;
pub use state::{MagnetType, ToolbarState};
pub use submenu_builder::{SubmenuBuilder, SubmenuItem, ToggleConfig};
pub use toolbar::{DrawingToolbar, DrawingToolbarDomainState};

// Re-export category trait for extensibility
pub use categories::ToolCategory;
