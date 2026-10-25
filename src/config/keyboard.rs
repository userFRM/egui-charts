//! Keyboard shortcut configuration and action mapping.
//!
//! [`KeyboardOptions`] toggles keyboard shortcuts on/off and configures
//! sensitivity.  [`KeyboardAction`] is the complete enum of actions that
//! can be triggered by key presses, with methods to map egui key events
//! to actions and to retrieve human-readable labels for a shortcuts dialog.

/// Keyboard Shortcuts.
///
/// Standard keyboard navigation for chart interaction.
use egui::Key;
use std::fmt;

/// Global keyboard shortcut configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyboardOptions {
    /// Enable keyboard shortcuts
    pub enabled: bool,

    /// Pan amount in bars for arrow keys
    pub pan_amount: f32,

    /// Zoom step for +/- keys
    pub zoom_step: f32,
}

impl Default for KeyboardOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            pan_amount: 1.0,
            zoom_step: 0.2,
        }
    }
}

/// Keyboard actions that can be performed on the chart
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardAction {
    /// Pan left by a page.
    PanLeftFast,
    /// Pan right by a page.
    PanRightFast,
    /// Undo the last edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// Open quick search.
    OpenQuickSearch,
    /// Open interval entry.
    OpenIntervalSearch,
    /// Go to a date.
    GoToDate,
    /// Take a chart snapshot.
    Snapshot,
    /// Invert the price scale.
    InvertScale,
    /// Copy the selected drawing.
    CopyDrawing,
    /// Paste a copied drawing.
    PasteDrawing,
    /// Hide or show drawings.
    ToggleDrawings,
    /// Maximize the active chart.
    MaximizeChart,
    /// Complete a multi-point drawing.
    CompleteDrawing,
    /// Select a drawing tool.
    DrawingTool(crate::config::DrawingToolType),

    // Navigation
    /// Pan left (Left arrow)
    PanLeft,
    /// Pan right (Right arrow)
    PanRight,
    /// Pan up in the price scale
    PanUp,
    /// Pan down in the price scale
    PanDown,

    // Zoom
    /// Zoom in (+, =)
    ZoomIn,
    /// Zoom out (-, _)
    ZoomOut,
    /// Page up (zoom in more)
    PageUp,
    /// Page down (zoom out more)
    PageDown,

    // Reset/Fit
    /// Scroll to real-time / latest data (Home, End)
    ScrollToRealTime,
    /// Fit content to visible range
    FitContent,
    /// Reset zoom to default
    ResetZoom,
    /// Reset chart (includes zoom and pan) (Alt+R)
    ResetChart,

    // Scale modes
    /// Toggle auto-scale
    ToggleAutoScale,
    /// Toggle logarithmic scale
    ToggleLogScale,
    /// Toggle percentage scale
    TogglePercentageScale,

    // Visual toggles
    /// Toggle crosshair
    ToggleCrosshair,
    /// Toggle grid
    ToggleGrid,
    /// Toggle volume
    ToggleVolume,

    // Chart type
    /// Cycle through chart types
    CycleChartType,

    // Drawing tools
    /// Enable/disable drawing mode
    ToggleDrawingMode,
    /// Delete the selected drawings, or the selected study when no drawing is selected
    /// (Delete, Backspace)
    DeleteSelectedDrawing,
    /// Escape/cancel current action (Escape)
    Cancel,

    // Timeframe commands (unbound by default)
    /// 1-minute timeframe
    SetTimeframe1Min,
    /// 5-minute timeframe
    SetTimeframe5Min,
    /// 15-minute timeframe
    SetTimeframe15Min,
    /// 30-minute timeframe
    SetTimeframe30Min,
    /// 1-hour timeframe
    SetTimeframe1Hour,
    /// 4-hour timeframe
    SetTimeframe4Hour,
    /// Daily timeframe
    SetTimeframeDaily,
    /// Weekly timeframe
    SetTimeframeWeekly,
    /// Monthly timeframe
    SetTimeframeMonthly,

    // App-level shortcuts
    /// Create alert (Alt+A)
    CreateAlert,
    /// Open indicator search (/)
    OpenIndicatorSearch,
    /// Open symbol search
    OpenSymbolSearch,
    /// Save layout
    SaveLayout,
    /// Show keyboard shortcuts help (?)
    ShowKeyboardHelp,

    // Multi-chart focus
    /// Cycle focus to next chart (Tab)
    CycleChartFocus,
    /// Cycle focus to previous chart (Shift+Tab)
    CycleChartFocusBack,
}

impl KeyboardAction {
    /// Scan all key presses in the current input state and return the first matching action.
    pub fn from_key_press_with_modifiers(input: &egui::InputState) -> Option<Self> {
        for event in &input.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
                && let Some(action) = Self::from_key_press(*key, modifiers)
            {
                return Some(action);
            }
        }
        None
    }

    /// Map egui key press to keyboard action
    /// Returns None if the key is not mapped
    pub fn from_key_press(key: Key, modifiers: &egui::Modifiers) -> Option<Self> {
        Self::all_by_category()
            .into_iter()
            .flat_map(|(_, actions)| actions)
            .find(|action| {
                action.shortcuts().iter().any(|shortcut| {
                    shortcut.logical_key == key && modifiers.matches_exact(shortcut.modifiers)
                })
            })
    }

    /// Human-readable description of the action. Use `shortcuts` for its bound keys.
    pub fn desc(&self) -> &str {
        self.label()
    }

    /// Human-readable label for the action (used in shortcuts dialog)
    pub fn label(&self) -> &str {
        match self {
            Self::PanLeftFast => "Pan left by a page",
            Self::PanRightFast => "Pan right by a page",
            Self::Undo => "Undo the last edit",
            Self::Redo => "Redo the last undone edit",
            Self::OpenQuickSearch => "Open quick search",
            Self::OpenIntervalSearch => "Open interval entry",
            Self::GoToDate => "Go to a date",
            Self::Snapshot => "Take a chart snapshot",
            Self::InvertScale => "Invert the price scale",
            Self::CopyDrawing => "Copy the selected drawing",
            Self::PasteDrawing => "Paste a copied drawing",
            Self::ToggleDrawings => "Hide or show drawings",
            Self::MaximizeChart => "Maximize the active chart",
            Self::CompleteDrawing => "Complete drawing",
            Self::DrawingTool(tool) => tool.as_str(),
            Self::PanLeft => "Pan left",
            Self::PanRight => "Pan right",
            Self::PanUp => "Pan up",
            Self::PanDown => "Pan down",
            Self::ZoomIn => "Zoom in",
            Self::ZoomOut => "Zoom out",
            Self::PageUp => "Zoom in more",
            Self::PageDown => "Zoom out more",
            Self::ScrollToRealTime => "Jump to latest",
            Self::FitContent => "Fit to screen",
            Self::ResetZoom => "Reset zoom",
            Self::ResetChart => "Reset chart",
            Self::ToggleAutoScale => "Toggle auto-scale",
            Self::ToggleLogScale => "Toggle log scale",
            Self::TogglePercentageScale => "Toggle percentage scale",
            Self::ToggleCrosshair => "Toggle crosshair",
            Self::ToggleGrid => "Toggle grid",
            Self::ToggleVolume => "Toggle volume",
            Self::CycleChartType => "Cycle chart type",
            Self::ToggleDrawingMode => "Toggle drawing mode",
            Self::DeleteSelectedDrawing => "Delete drawing or study",
            Self::Cancel => "Cancel",
            Self::SetTimeframe1Min => "1 minute",
            Self::SetTimeframe5Min => "5 minutes",
            Self::SetTimeframe15Min => "15 minutes",
            Self::SetTimeframe30Min => "30 minutes",
            Self::SetTimeframe1Hour => "1 hour",
            Self::SetTimeframe4Hour => "4 hours",
            Self::SetTimeframeDaily => "Daily",
            Self::SetTimeframeWeekly => "Weekly",
            Self::SetTimeframeMonthly => "Monthly",
            Self::CreateAlert => "Create alert",
            Self::OpenIndicatorSearch => "Open indicators",
            Self::OpenSymbolSearch => "Symbol search",
            Self::SaveLayout => "Save layout",
            Self::ShowKeyboardHelp => "Keyboard shortcuts",
            Self::CycleChartFocus => "Next chart",
            Self::CycleChartFocusBack => "Previous chart",
        }
    }

    /// The key combinations for this action, the first of which menus show. Empty for the
    /// actions the shortcuts dialog and the menus do not show.
    pub fn shortcuts(&self) -> &'static [egui::KeyboardShortcut] {
        use egui::{KeyboardShortcut, Modifiers};
        const fn key(key: Key) -> KeyboardShortcut {
            KeyboardShortcut::new(Modifiers::NONE, key)
        }
        match self {
            Self::PanLeft => const { &[key(Key::ArrowLeft)] },
            Self::PanRight => const { &[key(Key::ArrowRight)] },
            Self::PanLeftFast => {
                const { &[KeyboardShortcut::new(Modifiers::COMMAND, Key::ArrowLeft)] }
            }
            Self::PanRightFast => {
                const { &[KeyboardShortcut::new(Modifiers::COMMAND, Key::ArrowRight)] }
            }
            Self::ZoomIn => {
                const {
                    &[
                        key(Key::Plus),
                        key(Key::Equals),
                        KeyboardShortcut::new(Modifiers::COMMAND, Key::ArrowUp),
                    ]
                }
            }
            Self::ZoomOut => {
                const {
                    &[
                        key(Key::Minus),
                        KeyboardShortcut::new(Modifiers::COMMAND, Key::ArrowDown),
                    ]
                }
            }
            Self::PageUp => const { &[key(Key::PageUp)] },
            Self::PageDown => const { &[key(Key::PageDown)] },
            Self::ScrollToRealTime => const { &[key(Key::Home), key(Key::End)] },

            Self::ResetChart => const { &[KeyboardShortcut::new(Modifiers::ALT, Key::R)] },

            Self::ToggleLogScale => const { &[KeyboardShortcut::new(Modifiers::ALT, Key::L)] },
            Self::TogglePercentageScale => {
                const { &[KeyboardShortcut::new(Modifiers::ALT, Key::P)] }
            }
            Self::InvertScale => const { &[KeyboardShortcut::new(Modifiers::ALT, Key::I)] },
            Self::CreateAlert => const { &[KeyboardShortcut::new(Modifiers::ALT, Key::A)] },
            Self::OpenIndicatorSearch => const { &[key(Key::Slash)] },
            Self::OpenQuickSearch => const { &[KeyboardShortcut::new(Modifiers::COMMAND, Key::K)] },
            Self::GoToDate => const { &[KeyboardShortcut::new(Modifiers::ALT, Key::G)] },
            Self::Snapshot => const { &[KeyboardShortcut::new(Modifiers::ALT, Key::S)] },
            Self::Undo => const { &[KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)] },
            Self::Redo => {
                const {
                    &[
                        KeyboardShortcut::new(Modifiers::COMMAND, Key::Y),
                        KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z),
                    ]
                }
            }
            Self::CopyDrawing => const { &[KeyboardShortcut::new(Modifiers::COMMAND, Key::C)] },
            Self::PasteDrawing => const { &[KeyboardShortcut::new(Modifiers::COMMAND, Key::V)] },
            Self::ToggleDrawings => {
                const {
                    &[KeyboardShortcut::new(
                        Modifiers::COMMAND.plus(Modifiers::ALT),
                        Key::H,
                    )]
                }
            }
            Self::MaximizeChart => const { &[KeyboardShortcut::new(Modifiers::ALT, Key::Enter)] },
            Self::CycleChartFocus => const { &[key(Key::Tab)] },
            Self::CycleChartFocusBack => {
                const { &[KeyboardShortcut::new(Modifiers::SHIFT, Key::Tab)] }
            }
            Self::DeleteSelectedDrawing => const { &[key(Key::Delete), key(Key::Backspace)] },
            Self::Cancel => const { &[key(Key::Escape)] },
            Self::CompleteDrawing => const { &[key(Key::Enter)] },
            Self::ShowKeyboardHelp => {
                const {
                    &[
                        KeyboardShortcut::new(Modifiers::COMMAND, Key::Slash),
                        key(Key::Questionmark),
                        key(Key::F1),
                    ]
                }
            }
            Self::DrawingTool(crate::config::DrawingToolType::TrendLine) => {
                const { &[KeyboardShortcut::new(Modifiers::ALT, Key::T)] }
            }
            Self::DrawingTool(crate::config::DrawingToolType::HorizontalLine) => {
                const { &[KeyboardShortcut::new(Modifiers::ALT, Key::H)] }
            }
            Self::DrawingTool(crate::config::DrawingToolType::VerticalLine) => {
                const { &[KeyboardShortcut::new(Modifiers::ALT, Key::V)] }
            }
            Self::DrawingTool(crate::config::DrawingToolType::FibonacciRetracement) => {
                const { &[KeyboardShortcut::new(Modifiers::ALT, Key::F)] }
            }
            _ => &[],
        }
    }

    /// The first of [`shortcuts`](Self::shortcuts), for menus to show with
    /// [`egui::Context::format_shortcut`].
    pub fn shortcut(&self) -> Option<egui::KeyboardShortcut> {
        self.shortcuts().first().copied()
    }

    /// Short display text for default bindings. Menus format `shortcuts` for the platform.
    pub fn shortcut_key(&self) -> &'static str {
        match self {
            Self::PanLeft => "←",
            Self::PanRight => "→",
            Self::PanLeftFast => "Ctrl+Left",
            Self::PanRightFast => "Ctrl+Right",
            Self::ZoomIn => "+ / = / Ctrl+Up",
            Self::ZoomOut => "- / Ctrl+Down",
            Self::PageUp => "Page Up",
            Self::PageDown => "Page Down",
            Self::ScrollToRealTime => "Home / End",
            Self::ResetChart => "Alt+R",
            Self::ToggleLogScale => "Alt+L",
            Self::TogglePercentageScale => "Alt+P",
            Self::InvertScale => "Alt+I",
            Self::Undo => "Ctrl+Z",
            Self::Redo => "Ctrl+Y / Ctrl+Shift+Z",
            Self::CopyDrawing => "Ctrl+C",
            Self::PasteDrawing => "Ctrl+V",
            Self::ToggleDrawings => "Ctrl+Alt+H",
            Self::CreateAlert => "Alt+A",
            Self::OpenQuickSearch => "Ctrl+K",
            Self::OpenIndicatorSearch => "/",
            Self::GoToDate => "Alt+G",
            Self::Snapshot => "Alt+S",
            Self::MaximizeChart => "Alt+Enter",
            Self::DeleteSelectedDrawing => "Del / Backspace",
            Self::Cancel => "Esc",
            Self::CompleteDrawing => "Enter",
            Self::ShowKeyboardHelp => "Ctrl+/ / ? / F1",
            Self::CycleChartFocus => "Tab",
            Self::CycleChartFocusBack => "Shift+Tab",
            Self::DrawingTool(crate::config::DrawingToolType::TrendLine) => "Alt+T",
            Self::DrawingTool(crate::config::DrawingToolType::HorizontalLine) => "Alt+H",
            Self::DrawingTool(crate::config::DrawingToolType::VerticalLine) => "Alt+V",
            Self::DrawingTool(crate::config::DrawingToolType::FibonacciRetracement) => "Alt+F",
            _ => "",
        }
    }

    /// The actions with a bound key, grouped by category (for the shortcuts dialog).
    pub fn all_by_category() -> Vec<(&'static str, Vec<KeyboardAction>)> {
        use crate::config::DrawingToolType;
        vec![
            (
                "Navigation",
                vec![
                    Self::PanLeft,
                    Self::PanRight,
                    Self::PanLeftFast,
                    Self::PanRightFast,
                ],
            ),
            (
                "Zoom",
                vec![Self::ZoomIn, Self::ZoomOut, Self::PageUp, Self::PageDown],
            ),
            (
                "View",
                vec![
                    Self::ScrollToRealTime,
                    Self::FitContent,
                    Self::ResetChart,
                    Self::ToggleLogScale,
                    Self::TogglePercentageScale,
                    Self::InvertScale,
                ],
            ),
            (
                "Editing",
                vec![
                    Self::Undo,
                    Self::Redo,
                    Self::CopyDrawing,
                    Self::PasteDrawing,
                    Self::ToggleDrawings,
                ],
            ),
            (
                "Drawing tools",
                vec![
                    Self::DrawingTool(DrawingToolType::TrendLine),
                    Self::DrawingTool(DrawingToolType::HorizontalLine),
                    Self::DrawingTool(DrawingToolType::VerticalLine),
                    Self::DrawingTool(DrawingToolType::FibonacciRetracement),
                    Self::DeleteSelectedDrawing,
                    Self::Cancel,
                    Self::CompleteDrawing,
                ],
            ),
            (
                "App",
                vec![
                    Self::OpenQuickSearch,
                    Self::OpenIndicatorSearch,
                    Self::CreateAlert,
                    Self::GoToDate,
                    Self::Snapshot,
                    Self::MaximizeChart,
                    Self::CycleChartFocus,
                    Self::CycleChartFocusBack,
                    Self::ShowKeyboardHelp,
                ],
            ),
        ]
    }
}

impl fmt::Display for KeyboardAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.desc())
    }
}
