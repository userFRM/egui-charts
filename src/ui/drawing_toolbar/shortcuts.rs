//! Keyboard shortcuts for drawing tools.

use crate::drawings::DrawingToolType;
use egui::{Context, Key, KeyboardShortcut, Modifiers};

/// Keyboard shortcuts for drawing tools
///
/// # Examples
///
/// ```
/// use egui_open_trading_charts_rs::ui::drawing_toolbar::DrawingToolShortcuts;
/// use egui_open_trading_charts_rs::drawing::DrawingToolType;
/// use egui::{KeyboardShortcut, Key, Modifiers};
///
/// let mut shortcuts = DrawingToolShortcuts::new();
///
/// // Add custom shortcut
/// shortcuts.add_shortcut(
///     KeyboardShortcut::new(Modifiers::CTRL, Key::D),
///     DrawingToolType::TrendLine
/// );
/// ```
pub struct DrawingToolShortcuts {
    shortcuts: Vec<(KeyboardShortcut, DrawingToolType)>,
}

impl Default for DrawingToolShortcuts {
    fn default() -> Self {
        Self {
            shortcuts: vec![
                (
                    KeyboardShortcut::new(Modifiers::ALT, Key::T),
                    DrawingToolType::TrendLine,
                ),
                (
                    KeyboardShortcut::new(Modifiers::ALT, Key::H),
                    DrawingToolType::HorizontalLine,
                ),
                (
                    KeyboardShortcut::new(Modifiers::ALT, Key::V),
                    DrawingToolType::VerticalLine,
                ),
                (
                    KeyboardShortcut::new(Modifiers::ALT, Key::F),
                    DrawingToolType::FibonacciRetracement,
                ),
                (
                    KeyboardShortcut::new(Modifiers::ALT, Key::R),
                    DrawingToolType::Rect,
                ),
                (
                    KeyboardShortcut::new(Modifiers::ALT, Key::L),
                    DrawingToolType::TextLabel,
                ),
                (
                    KeyboardShortcut::new(Modifiers::ALT, Key::P),
                    DrawingToolType::Pitchfork,
                ),
            ],
        }
    }
}

impl DrawingToolShortcuts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check for keyboard shortcuts and return the tool if one was triggered
    ///
    /// # Examples
    ///
    /// ```
    /// # use egui_open_trading_charts_rs::ui::drawing_toolbar::DrawingToolShortcuts;
    /// # use egui::Context;
    /// let shortcuts = DrawingToolShortcuts::new();
    /// // In your UI update loop:
    /// // if let Some(tool) = shortcuts.check(&ctx) {
    /// //     println!("Shortcut triggered: {:?}", tool);
    /// // }
    /// ```
    pub fn check(&self, ctx: &Context) -> Option<DrawingToolType> {
        for (shortcut, tool) in &self.shortcuts {
            if ctx.input_mut(|i| i.consume_shortcut(shortcut)) {
                return Some(*tool);
            }
        }
        None
    }

    /// Add a custom shortcut
    ///
    /// # Examples
    ///
    /// ```
    /// # use egui_open_trading_charts_rs::ui::drawing_toolbar::DrawingToolShortcuts;
    /// # use egui_open_trading_charts_rs::drawing::DrawingToolType;
    /// # use egui::{KeyboardShortcut, Key, Modifiers};
    /// let mut shortcuts = DrawingToolShortcuts::new();
    /// shortcuts.add_shortcut(
    ///     KeyboardShortcut::new(Modifiers::CTRL, Key::T),
    ///     DrawingToolType::TrendLine
    /// );
    /// ```
    pub fn add_shortcut(&mut self, shortcut: KeyboardShortcut, tool: DrawingToolType) {
        self.shortcuts.push((shortcut, tool));
    }

    /// Get help text for shortcuts
    ///
    /// Returns a list of (shortcut_text, tool_name) pairs.
    ///
    /// # Examples
    ///
    /// ```
    /// # use egui_open_trading_charts_rs::ui::drawing_toolbar::DrawingToolShortcuts;
    /// let shortcuts = DrawingToolShortcuts::new();
    /// for (key, tool) in shortcuts.help_text() {
    ///     println!("{}: {}", key, tool);
    /// }
    /// ```
    pub fn help_text(&self) -> Vec<(String, String)> {
        self.shortcuts
            .iter()
            .map(|(shortcut, tool)| {
                let key_text = format!("{:?}", shortcut);
                (key_text, tool.as_str().to_string())
            })
            .collect()
    }
}
