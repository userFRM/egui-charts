//! Connection Status Indicator
//!
//! Displays a small indicator showing the WebSocket connection state.
//! Used in the top toolbar to show real-time data connectivity.

use crate::icons::{Icon, icons};
use crate::styles::icons as icon_sizes;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::stubs::ConnectionState;
use egui::{Color32, Response, RichText, Ui, Vec2, Widget};

/// Connection status indicator widget
pub struct ConnectionStatusIndicator<'a> {
    /// Current connection state
    state: &'a ConnectionState,
    /// Whether to show text label
    show_label: bool,
}

impl<'a> ConnectionStatusIndicator<'a> {
    /// Create a new connection status indicator
    pub fn new(state: &'a ConnectionState) -> Self {
        Self {
            state,
            show_label: false,
        }
    }

    /// Show text label alongside the indicator
    #[must_use]
    pub fn with_label(mut self) -> Self {
        self.show_label = true;
        self
    }

    /// Get color for the current state
    fn state_color(&self) -> Color32 {
        match self.state {
            ConnectionState::Connected => DESIGN_TOKENS.semantic.extended.bullish,
            ConnectionState::Connecting => DESIGN_TOKENS.semantic.status.caution,
            ConnectionState::Reconnecting { .. } => DESIGN_TOKENS.semantic.status.caution,
            ConnectionState::Disconnected => DESIGN_TOKENS.semantic.ui.text_muted_dark,
            ConnectionState::Failed(_) => DESIGN_TOKENS.semantic.extended.bearish,
        }
    }

    /// Get icon for the current state
    fn state_icon(&self) -> &'static Icon {
        match self.state {
            ConnectionState::Connected => &icons::STATUS_CONNECTED,
            ConnectionState::Connecting | ConnectionState::Reconnecting { .. } => {
                &icons::STATUS_CONNECTING
            }
            ConnectionState::Disconnected => &icons::STATUS_DISCONNECTED,
            ConnectionState::Failed(_) => &icons::STATUS_ERROR,
        }
    }

    /// Get label text for the current state
    fn state_label(&self) -> &'static str {
        match self.state {
            ConnectionState::Connected => "Live",
            ConnectionState::Connecting => "Connecting...",
            ConnectionState::Reconnecting { attempt: _ } => {
                // Can't include attempt in static str, so just show generic
                "Reconnecting..."
            }
            ConnectionState::Disconnected => "Offline",
            ConnectionState::Failed(_) => "Error",
        }
    }
}

impl<'a> Widget for ConnectionStatusIndicator<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let color = self.state_color();
        let icon = self.state_icon();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = DESIGN_TOKENS.spacing.xs;

            // Icon - use SVG with tint color
            ui.add(icon.as_image_tinted(Vec2::splat(icon_sizes::XS), color));

            // Optional label
            if self.show_label {
                ui.label(RichText::new(self.state_label()).color(color).size(11.0));
            }
        })
        .response
    }
}

/// Show a connection status indicator in the UI
///
/// This is a convenience function for showing the indicator inline.
pub fn show_connection_status(ui: &mut Ui, state: &ConnectionState) -> Response {
    ConnectionStatusIndicator::new(state).ui(ui)
}

/// Show a connection status indicator with label
pub fn show_connection_status_with_label(ui: &mut Ui, state: &ConnectionState) -> Response {
    ConnectionStatusIndicator::new(state).with_label().ui(ui)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_icons() {
        let connected = ConnectionStatusIndicator::new(&ConnectionState::Connected);
        assert_eq!(connected.state_icon(), &icons::STATUS_CONNECTED);

        let disconnected = ConnectionStatusIndicator::new(&ConnectionState::Disconnected);
        assert_eq!(disconnected.state_icon(), &icons::STATUS_DISCONNECTED);

        let failed_state = ConnectionState::Failed("error".to_string());
        let failed = ConnectionStatusIndicator::new(&failed_state);
        assert_eq!(failed.state_icon(), &icons::STATUS_ERROR);
    }
}
