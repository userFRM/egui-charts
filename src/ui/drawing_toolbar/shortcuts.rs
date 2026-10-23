//! Keyboard shortcuts for drawing tools.

use crate::drawings::DrawingToolType;
use egui::{Context, Key, KeyboardShortcut, ModifierNames, Modifiers};

/// Keyboard shortcuts for drawing tools
///
/// # Examples
///
/// ```
/// use egui_charts::ui::drawing_toolbar::DrawingToolShortcuts;
/// use egui_charts::drawings::DrawingToolType;
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
    /// use egui::{Event, Key, Modifiers, RawInput};
    /// use egui_charts::drawings::DrawingToolType;
    /// use egui_charts::ui::drawing_toolbar::DrawingToolShortcuts;
    ///
    /// let shortcuts = DrawingToolShortcuts::new();
    /// let ctx = egui::Context::default();
    ///
    /// // A frame in which the user pressed Alt+T.
    /// let mut input = RawInput::default();
    /// input.events.push(Event::Key {
    ///     key: Key::T,
    ///     physical_key: None,
    ///     pressed: true,
    ///     repeat: false,
    ///     modifiers: Modifiers::ALT,
    /// });
    ///
    /// let mut triggered = None;
    /// ctx.run_ui(input, |ui| triggered = shortcuts.check(ui.ctx()))
    ///     .drop_without_applying_deltas();
    /// assert_eq!(triggered, Some(DrawingToolType::TrendLine));
    /// ```
    pub fn check(&self, ctx: &Context) -> Option<DrawingToolType> {
        for (shortcut, tool) in &self.shortcuts {
            if ctx.input_mut(|i| i.consume_shortcut(shortcut)) {
                return Some(*tool);
            }
        }
        None
    }

    /// The shortcut that selects `tool`, written the way the platform writes it ("Alt+T"), for
    /// the hint next to the tool in a flyout.
    pub(super) fn hint(&self, ctx: &Context, tool: DrawingToolType) -> Option<String> {
        self.shortcuts
            .iter()
            .find(|(_, bound)| *bound == tool)
            .map(|(shortcut, _)| ctx.format_shortcut(shortcut))
    }

    /// Add a custom shortcut
    ///
    /// # Examples
    ///
    /// ```
    /// # use egui_charts::ui::drawing_toolbar::DrawingToolShortcuts;
    /// # use egui_charts::drawings::DrawingToolType;
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
    /// Returns a list of (shortcut_text, tool_name) pairs, such as ("Alt+T", "Trend Line").
    ///
    /// # Examples
    ///
    /// ```
    /// # use egui_charts::ui::drawing_toolbar::DrawingToolShortcuts;
    /// let shortcuts = DrawingToolShortcuts::new();
    /// for (key, tool) in shortcuts.help_text() {
    ///     println!("{}: {}", key, tool);
    /// }
    /// ```
    pub fn help_text(&self) -> Vec<(String, String)> {
        self.shortcuts
            .iter()
            .map(|(shortcut, tool)| {
                let key_text = shortcut.format(&ModifierNames::NAMES, false);
                (key_text, tool.as_str().to_string())
            })
            .collect()
    }
}
