//! Reusable UI components for building chart interfaces.
//!
//! Generic, domain-agnostic building blocks: buttons, toolbars, dialogs,
//! forms, modals, etc. The chart-specific UI in [`crate::ui`] composes
//! these components with domain logic.

pub mod alert;
pub mod autocomplete;
pub mod buttons;
pub mod color_picker;
pub mod command_palette;
pub mod dialog;
pub mod empty_state;
pub mod filter;
pub mod form;
pub mod frames;
pub mod list_item;
pub mod loading;
pub mod modal;
pub mod notifications;
pub mod panel_header;
pub mod section;
pub mod sidebar_layout;
pub mod status_message;
pub mod tab_bar;
pub mod toolbar;
pub mod trading_labels;

// Convenience re-exports for commonly used types
pub use buttons::{IconButton, SplitButton};
pub use color_picker::{ColorPicker, ColorPickerState};
pub use empty_state::EmptyState;
pub use form::FormGrid;
pub use frames::Card;
pub use loading::LoadingIndicator;
pub use panel_header::PanelHeader;
pub use toolbar::ResponsiveToolbar;
