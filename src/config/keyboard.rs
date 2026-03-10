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
#[derive(Debug, Clone, Copy)]
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
            pan_amount: 3.0,
            zoom_step: 0.2,
        }
    }
}

/// Keyboard actions that can be performed on the chart
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardAction {
    // Navigation
    /// Pan left (Left arrow)
    PanLeft,
    /// Pan right (Right arrow)
    PanRight,
    /// Pan up (Up arrow) - for vertical panning in price scale
    PanUp,
    /// Pan down (Down arrow) - for vertical panning in price scale
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
    /// Fit content to visible range (F)
    FitContent,
    /// Reset zoom to default (R)
    ResetZoom,
    /// Reset chart (includes zoom and pan) (Ctrl+R)
    ResetChart,

    // Scale modes
    /// Toggle auto-scale (A)
    ToggleAutoScale,
    /// Toggle logarithmic scale (L / Alt+L)
    ToggleLogScale,
    /// Toggle percentage scale (%)
    TogglePercentageScale,

    // Visual toggles
    /// Toggle crosshair (C)
    ToggleCrosshair,
    /// Toggle grid (G / Alt+G)
    ToggleGrid,
    /// Toggle volume (V)
    ToggleVolume,

    // Chart type
    /// Cycle through chart types (T)
    CycleChartType,

    // Drawing tools
    /// Enable/disable drawing mode (D)
    ToggleDrawingMode,
    /// Delete selected drawing (Delete, Backspace)
    DeleteSelectedDrawing,
    /// Escape/cancel current action (Escape)
    Cancel,

    // Timeframe shortcuts (number keys)
    /// 1-minute timeframe (1)
    SetTimeframe1Min,
    /// 5-minute timeframe (2)
    SetTimeframe5Min,
    /// 15-minute timeframe (3)
    SetTimeframe15Min,
    /// 30-minute timeframe (4)
    SetTimeframe30Min,
    /// 1-hour timeframe (5)
    SetTimeframe1Hour,
    /// 4-hour timeframe (6)
    SetTimeframe4Hour,
    /// Daily timeframe (7)
    SetTimeframeDaily,
    /// Weekly timeframe (8)
    SetTimeframeWeekly,
    /// Monthly timeframe (9)
    SetTimeframeMonthly,

    // App-level shortcuts
    /// Create alert (Alt+A)
    CreateAlert,
    /// Open indicator search (/)
    OpenIndicatorSearch,
    /// Open symbol search (Alt+S)
    OpenSymbolSearch,
    /// Save layout (Ctrl+S)
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
            {
                if let Some(action) = Self::from_key_press(*key, modifiers) {
                    return Some(action);
                }
            }
        }
        None
    }

    /// Map egui key press to keyboard action
    /// Returns None if the key is not mapped
    pub fn from_key_press(key: Key, modifiers: &egui::Modifiers) -> Option<Self> {
        match (
            key,
            modifiers.ctrl || modifiers.command,
            modifiers.shift,
            modifiers.alt,
        ) {
            // Navigation
            (Key::ArrowLeft, false, _, false) => Some(Self::PanLeft),
            (Key::ArrowRight, false, _, false) => Some(Self::PanRight),
            (Key::ArrowUp, false, _, false) => Some(Self::PanUp),
            (Key::ArrowDown, false, _, false) => Some(Self::PanDown),

            // Zoom
            (Key::Plus, false, _, false) | (Key::Equals, false, _, false) => Some(Self::ZoomIn),
            (Key::Minus, false, _, false) => Some(Self::ZoomOut),
            (Key::PageUp, false, _, false) => Some(Self::PageUp),
            (Key::PageDown, false, _, false) => Some(Self::PageDown),

            // Reset/Fit
            (Key::Home, false, _, false) | (Key::End, false, _, false) => {
                Some(Self::ScrollToRealTime)
            }
            (Key::F, false, _, false) => Some(Self::FitContent),
            (Key::R, false, _, false) => Some(Self::ResetZoom),
            (Key::R, true, _, false) => Some(Self::ResetChart),

            // Scale modes
            (Key::A, false, _, false) => Some(Self::ToggleAutoScale),
            (Key::L, false, _, false) | (Key::L, false, false, true) => Some(Self::ToggleLogScale),

            // Visual toggles
            (Key::C, false, _, false) => Some(Self::ToggleCrosshair),
            (Key::G, false, _, false) | (Key::G, false, false, true) => Some(Self::ToggleGrid),
            (Key::V, false, _, false) => Some(Self::ToggleVolume),

            // Chart type
            (Key::T, false, _, false) => Some(Self::CycleChartType),

            // Drawing tools
            (Key::D, false, _, false) => Some(Self::ToggleDrawingMode),
            (Key::Delete, false, _, false) | (Key::Backspace, false, _, false) => {
                Some(Self::DeleteSelectedDrawing)
            }
            (Key::Escape, false, _, false) => Some(Self::Cancel),

            // Timeframe shortcuts (number keys, no modifiers)
            (Key::Num1, false, false, false) => Some(Self::SetTimeframe1Min),
            (Key::Num2, false, false, false) => Some(Self::SetTimeframe5Min),
            (Key::Num3, false, false, false) => Some(Self::SetTimeframe15Min),
            (Key::Num4, false, false, false) => Some(Self::SetTimeframe30Min),
            (Key::Num5, false, false, false) => Some(Self::SetTimeframe1Hour),
            (Key::Num6, false, false, false) => Some(Self::SetTimeframe4Hour),
            (Key::Num7, false, false, false) => Some(Self::SetTimeframeDaily),
            (Key::Num8, false, false, false) => Some(Self::SetTimeframeWeekly),
            (Key::Num9, false, false, false) => Some(Self::SetTimeframeMonthly),

            // App-level shortcuts
            (Key::A, false, false, true) => Some(Self::CreateAlert),
            (Key::Slash, false, false, false) => Some(Self::OpenIndicatorSearch),
            (Key::S, false, false, true) => Some(Self::OpenSymbolSearch),
            (Key::S, true, false, false) => Some(Self::SaveLayout),
            (Key::Slash, false, true, false) => Some(Self::ShowKeyboardHelp), // ? = Shift+/

            // Multi-chart focus cycling
            (Key::Tab, false, false, false) => Some(Self::CycleChartFocus),
            (Key::Tab, false, true, false) => Some(Self::CycleChartFocusBack),

            _ => None,
        }
    }

    /// Get a human-readable desc of the keyboard shortcut
    pub fn desc(&self) -> &'static str {
        match self {
            Self::PanLeft => "Pan left (Left)",
            Self::PanRight => "Pan right (Right)",
            Self::PanUp => "Pan up (Up)",
            Self::PanDown => "Pan down (Down)",
            Self::ZoomIn => "Zoom in (+/=)",
            Self::ZoomOut => "Zoom out (-)",
            Self::PageUp => "Zoom in more (Page Up)",
            Self::PageDown => "Zoom out more (Page Down)",
            Self::ScrollToRealTime => "Jump to latest (Home/End)",
            Self::FitContent => "Fit to screen (F)",
            Self::ResetZoom => "Reset zoom (R)",
            Self::ResetChart => "Reset chart (Ctrl+R)",
            Self::ToggleAutoScale => "Toggle auto-scale (A)",
            Self::ToggleLogScale => "Toggle log scale (L)",
            Self::TogglePercentageScale => "Toggle percentage scale (%)",
            Self::ToggleCrosshair => "Toggle crosshair (C)",
            Self::ToggleGrid => "Toggle grid (G)",
            Self::ToggleVolume => "Toggle volume (V)",
            Self::CycleChartType => "Cycle chart type (T)",
            Self::ToggleDrawingMode => "Toggle drawing mode (D)",
            Self::DeleteSelectedDrawing => "Delete drawing (Del)",
            Self::Cancel => "Cancel (Esc)",
            Self::SetTimeframe1Min => "1-minute timeframe (1)",
            Self::SetTimeframe5Min => "5-minute timeframe (2)",
            Self::SetTimeframe15Min => "15-minute timeframe (3)",
            Self::SetTimeframe30Min => "30-minute timeframe (4)",
            Self::SetTimeframe1Hour => "1-hour timeframe (5)",
            Self::SetTimeframe4Hour => "4-hour timeframe (6)",
            Self::SetTimeframeDaily => "Daily timeframe (7)",
            Self::SetTimeframeWeekly => "Weekly timeframe (8)",
            Self::SetTimeframeMonthly => "Monthly timeframe (9)",
            Self::CreateAlert => "Create alert (Alt+A)",
            Self::OpenIndicatorSearch => "Open indicators (/)",
            Self::OpenSymbolSearch => "Symbol search (Alt+S)",
            Self::SaveLayout => "Save layout (Ctrl+S)",
            Self::ShowKeyboardHelp => "Keyboard shortcuts (?)",
            Self::CycleChartFocus => "Next chart (Tab)",
            Self::CycleChartFocusBack => "Previous chart (Shift+Tab)",
        }
    }

    /// Human-readable label for the action (used in shortcuts dialog)
    pub fn label(&self) -> &'static str {
        match self {
            Self::PanLeft => "Pan Left",
            Self::PanRight => "Pan Right",
            Self::PanUp => "Pan Up",
            Self::PanDown => "Pan Down",
            Self::ZoomIn => "Zoom In",
            Self::ZoomOut => "Zoom Out",
            Self::PageUp => "Zoom In (Large)",
            Self::PageDown => "Zoom Out (Large)",
            Self::ScrollToRealTime => "Jump to Latest",
            Self::FitContent => "Fit to Screen",
            Self::ResetZoom => "Reset Zoom",
            Self::ResetChart => "Reset Chart",
            Self::ToggleAutoScale => "Toggle Auto-Scale",
            Self::ToggleLogScale => "Toggle Log Scale",
            Self::TogglePercentageScale => "Toggle Percentage Scale",
            Self::ToggleCrosshair => "Toggle Crosshair",
            Self::ToggleGrid => "Toggle Grid",
            Self::ToggleVolume => "Toggle Volume",
            Self::CycleChartType => "Cycle Chart Type",
            Self::ToggleDrawingMode => "Toggle Drawing Mode",
            Self::DeleteSelectedDrawing => "Delete Drawing",
            Self::Cancel => "Cancel / Escape",
            Self::SetTimeframe1Min => "1 Minute",
            Self::SetTimeframe5Min => "5 Minutes",
            Self::SetTimeframe15Min => "15 Minutes",
            Self::SetTimeframe30Min => "30 Minutes",
            Self::SetTimeframe1Hour => "1 Hour",
            Self::SetTimeframe4Hour => "4 Hours",
            Self::SetTimeframeDaily => "Daily",
            Self::SetTimeframeWeekly => "Weekly",
            Self::SetTimeframeMonthly => "Monthly",
            Self::CreateAlert => "Create Alert",
            Self::OpenIndicatorSearch => "Open Indicators",
            Self::OpenSymbolSearch => "Symbol Search",
            Self::SaveLayout => "Save Layout",
            Self::ShowKeyboardHelp => "Keyboard Shortcuts",
            Self::CycleChartFocus => "Next Chart",
            Self::CycleChartFocusBack => "Previous Chart",
        }
    }

    /// Key combination display text (used in shortcuts dialog badges)
    pub fn shortcut_key(&self) -> &'static str {
        match self {
            Self::PanLeft => "←",
            Self::PanRight => "→",
            Self::PanUp => "↑",
            Self::PanDown => "↓",
            Self::ZoomIn => "+ / =",
            Self::ZoomOut => "-",
            Self::PageUp => "Page Up",
            Self::PageDown => "Page Down",
            Self::ScrollToRealTime => "Home / End",
            Self::FitContent => "F",
            Self::ResetZoom => "R",
            Self::ResetChart => "Ctrl+R",
            Self::ToggleAutoScale => "A",
            Self::ToggleLogScale => "L",
            Self::TogglePercentageScale => "%",
            Self::ToggleCrosshair => "C",
            Self::ToggleGrid => "G",
            Self::ToggleVolume => "V",
            Self::CycleChartType => "T",
            Self::ToggleDrawingMode => "D",
            Self::DeleteSelectedDrawing => "Del / Backspace",
            Self::Cancel => "Esc",
            Self::SetTimeframe1Min => "1",
            Self::SetTimeframe5Min => "2",
            Self::SetTimeframe15Min => "3",
            Self::SetTimeframe30Min => "4",
            Self::SetTimeframe1Hour => "5",
            Self::SetTimeframe4Hour => "6",
            Self::SetTimeframeDaily => "7",
            Self::SetTimeframeWeekly => "8",
            Self::SetTimeframeMonthly => "9",
            Self::CreateAlert => "Alt+A",
            Self::OpenIndicatorSearch => "/",
            Self::OpenSymbolSearch => "Alt+S",
            Self::SaveLayout => "Ctrl+S",
            Self::ShowKeyboardHelp => "?",
            Self::CycleChartFocus => "Tab",
            Self::CycleChartFocusBack => "Shift+Tab",
        }
    }

    /// All keyboard actions grouped by category (for the shortcuts dialog)
    pub fn all_by_category() -> Vec<(&'static str, Vec<KeyboardAction>)> {
        vec![
            (
                "Navigation",
                vec![Self::PanLeft, Self::PanRight, Self::PanUp, Self::PanDown],
            ),
            (
                "Zoom",
                vec![Self::ZoomIn, Self::ZoomOut, Self::PageUp, Self::PageDown],
            ),
            (
                "Reset / Fit",
                vec![
                    Self::ScrollToRealTime,
                    Self::FitContent,
                    Self::ResetZoom,
                    Self::ResetChart,
                ],
            ),
            (
                "Scale Modes",
                vec![
                    Self::ToggleAutoScale,
                    Self::ToggleLogScale,
                    Self::TogglePercentageScale,
                ],
            ),
            (
                "Visual Toggles",
                vec![
                    Self::ToggleCrosshair,
                    Self::ToggleGrid,
                    Self::ToggleVolume,
                    Self::CycleChartType,
                ],
            ),
            (
                "Drawing Tools",
                vec![
                    Self::ToggleDrawingMode,
                    Self::DeleteSelectedDrawing,
                    Self::Cancel,
                ],
            ),
            (
                "Timeframes",
                vec![
                    Self::SetTimeframe1Min,
                    Self::SetTimeframe5Min,
                    Self::SetTimeframe15Min,
                    Self::SetTimeframe30Min,
                    Self::SetTimeframe1Hour,
                    Self::SetTimeframe4Hour,
                    Self::SetTimeframeDaily,
                    Self::SetTimeframeWeekly,
                    Self::SetTimeframeMonthly,
                ],
            ),
            (
                "App",
                vec![
                    Self::CreateAlert,
                    Self::OpenIndicatorSearch,
                    Self::OpenSymbolSearch,
                    Self::SaveLayout,
                    Self::ShowKeyboardHelp,
                ],
            ),
            (
                "Multi-Chart",
                vec![Self::CycleChartFocus, Self::CycleChartFocusBack],
            ),
        ]
    }
}

impl fmt::Display for KeyboardAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.desc())
    }
}
