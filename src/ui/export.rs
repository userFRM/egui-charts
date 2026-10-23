//! Chart export functionality for screenshots and saving charts
//!
//! This module provides widgets and traits for exporting charts to multiple formats:
//! - PNG images
//! - CSV of the bar data
//! - Clipboard copy
//!
//! # Example
//!
//! ```no_run
//! use egui_charts::ui::export::{ExportDialog, ExportResponse};
//!
//! # fn update(ctx: &egui::Context) {
//! let mut export_dialog = ExportDialog::new();
//!
//! // In your egui update loop
//! match export_dialog.show(ctx) {
//!     ExportResponse::Export { config, save_path } => {
//!         // Trigger export with config
//!         println!("Exporting to: {:?}", save_path);
//!     }
//!     ExportResponse::Closed => {
//!         // Dialog was closed without exporting
//!     }
//!     ExportResponse::None => {
//!         // Dialog is not open or no action taken
//!     }
//! }
//! # }
//! ```

use crate::ext::UiExt;
use crate::theme::ThemeExt;
use crate::ui_kit::FormGrid;
use crate::ui_kit::dialog::{DialogFooter, dialog_frame, title_row};
use egui::{Context, Id, Key, Modifiers, RichText};
use std::path::PathBuf;

/// Id of the file path field, so a failed export can focus it.
fn path_field() -> Id {
    Id::NULL.with("egui_charts::export_dialog::path")
}

/// Export format options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// PNG raster image format
    Png,
    /// Copy to system clipboard
    Clipboard,
    /// CSV export of the bar data (time and OHLCV)
    Csv,
}

impl ExportFormat {
    /// Returns the human-readable name of this format
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Clipboard => "Clipboard",
            Self::Csv => "CSV",
        }
    }

    /// Returns the file extension for this format (if applicable)
    pub fn file_extension(&self) -> Option<&'static str> {
        match self {
            Self::Png => Some("png"),
            Self::Clipboard => None,
            Self::Csv => Some("csv"),
        }
    }

    /// Returns whether this format exports data (vs image/screenshot)
    pub fn is_data_format(&self) -> bool {
        matches!(self, Self::Csv)
    }
}

/// Response from the export dialog
///
/// Use this to handle user actions from the export dialog.
#[derive(Debug, Clone)]
#[must_use = "Export dialog response should be handled"]
pub enum ExportResponse {
    /// User requested export with the given configuration
    Export {
        /// The export configuration chosen by the user
        config: ExportConfig,
        /// Optional save path (None for clipboard exports)
        save_path: Option<PathBuf>,
    },
    /// Dialog was closed without exporting
    Closed,
    /// No action taken (dialog not open or still being configured)
    None,
}

/// Export configuration for customizing chart exports
///
/// # Example
///
/// ```
/// use egui_charts::ui::export::{ExportConfig, ExportFormat};
///
/// let config = ExportConfig {
///     format: ExportFormat::Png,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Export format (PNG, CSV or Clipboard)
    pub format: ExportFormat,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            format: ExportFormat::Png,
        }
    }
}

/// Export dialog for configuring and executing chart exports
///
/// The dialog offers:
/// - Format selection (PNG, CSV, Clipboard)
/// - File save path entry
///
/// The dialog only collects the request; the host performs the export. Use
/// [`with_formats`](Self::with_formats) to offer only the formats the host
/// can produce.
///
/// # Example
///
/// ```no_run
/// use egui_charts::ui::export::{ExportDialog, ExportResponse};
///
/// # fn update(ctx: &egui::Context) {
/// let mut export_dialog = ExportDialog::new();
///
/// // Open the dialog
/// export_dialog.open();
///
/// // In your egui update loop
/// match export_dialog.show(ctx) {
///     ExportResponse::Export { config, save_path } => {
///         // Perform the actual export
///         println!("Exporting with config: {:?}", config);
///     }
///     ExportResponse::Closed => {
///         println!("Dialog closed");
///     }
///     ExportResponse::None => {}
/// }
/// # }
/// ```
pub struct ExportDialog {
    is_open: bool,
    config: ExportConfig,
    formats: Vec<ExportFormat>,
    save_path: String,
    export_status: ExportStatus,
}

/// Internal status tracking for export operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportStatus {
    /// Ready to export
    Idle,
    /// Export in progress
    Exporting,
    /// Export completed successfully
    Success,
    /// Export failed with error message
    Error(String),
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl ExportDialog {
    /// Creates a new export dialog with default configuration
    pub fn new() -> Self {
        Self {
            is_open: false,
            config: ExportConfig::default(),
            formats: vec![
                ExportFormat::Png,
                ExportFormat::Csv,
                ExportFormat::Clipboard,
            ],
            save_path: String::new(),
            export_status: ExportStatus::Idle,
        }
    }

    /// Offers only `formats`, in the given order, for hosts that cannot
    /// perform every export. Selects the first of them if the current format
    /// is not offered.
    pub fn with_formats(mut self, formats: &[ExportFormat]) -> Self {
        self.formats = formats.to_vec();
        if !self.formats.contains(&self.config.format)
            && let Some(&first) = self.formats.first()
        {
            self.config.format = first;
        }
        self
    }

    /// Opens the export dialog
    ///
    /// This resets the export status to idle and shows the dialog.
    pub fn open(&mut self) {
        self.is_open = true;
        self.export_status = ExportStatus::Idle;
    }

    /// Closes the export dialog
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Returns whether the dialog is currently open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Shows the export dialog and returns the user's action
    ///
    /// This displays a dialog with export configuration options.
    /// Returns [`ExportResponse`] indicating what action the user took.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use egui_charts::ui::export::{ExportDialog, ExportResponse};
    /// # fn update(ctx: &egui::Context) {
    /// let mut dialog = ExportDialog::new();
    /// dialog.open();
    ///
    /// match dialog.show(ctx) {
    ///     ExportResponse::Export { config, save_path } => {
    ///         // Perform export
    ///     }
    ///     ExportResponse::Closed => {}
    ///     ExportResponse::None => {}
    /// }
    /// # }
    /// ```
    #[must_use = "Export dialog response should be handled"]
    pub fn show(&mut self, ctx: &Context) -> ExportResponse {
        if !self.is_open {
            return ExportResponse::None;
        }

        let mut response = ExportResponse::None;
        // Escape and Enter belong to a focused widget or an open popup first. egui drops focus and
        // closes popups on Escape before the frame runs, so their state is read from the previous
        // frame. Only a fresh press acts: the repeats of a key held after it left a field or
        // closed a popup must not go on to close the dialog.
        let busy_id = Id::NULL.with("egui_charts::export_dialog::busy");
        let busy = ctx.data(|d| d.get_temp::<bool>(busy_id)).unwrap_or(false);

        // Centred horizontally like the crate's other dialogs, but movable. The top edge is the
        // pivot, so the window grows downwards as its content changes instead of around its centre.
        let r = ctx.content_rect();
        let frame = dialog_frame(&ctx.global_style());
        egui::Window::new("Export Chart")
            .title_bar(false)
            .resizable(false)
            .default_width(340.0 + frame.total_margin().sum().x)
            .frame(frame)
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(egui::pos2(r.center().x, r.top() + r.height() * 0.2))
            .show(ctx, |ui| response = self.draw_contents(ui));

        let consume = |key| {
            ctx.input_mut(|i| {
                let fresh = i.events.iter().any(|e| {
                    matches!(e, egui::Event::Key { key: k, pressed: true, repeat: false, modifiers, .. }
                        if *k == key && modifiers.is_none())
                });
                fresh && i.consume_key(Modifiers::NONE, key)
            })
        };
        if matches!(response, ExportResponse::None) && !busy {
            if consume(Key::Escape) {
                response = ExportResponse::Closed;
            } else if consume(Key::Enter) {
                response = self.request_export(ctx);
            }
        }

        let busy = ctx.memory(|m| m.focused().is_some()) || egui::Popup::is_any_open(ctx);
        ctx.data_mut(|d| d.insert_temp(busy_id, busy));

        match response {
            ExportResponse::Closed | ExportResponse::Export { .. } => self.close(),
            ExportResponse::None => {}
        }
        response
    }

    fn draw_contents(&mut self, ui: &mut egui::Ui) -> ExportResponse {
        let theme = ui.active_theme();
        if title_row(ui, "Export chart", true) {
            return ExportResponse::Closed;
        }

        let mut response = ExportResponse::None;
        FormGrid::new("export_form").show(ui, |ui| {
            let label = ui.label("Format");
            ui.horizontal(|ui| {
                for &format in &self.formats {
                    ui.radio_value(&mut self.config.format, format, format.as_str());
                }
            })
            .response
            .labelled_by(label.id);
            ui.end_row();

            if self.config.format != ExportFormat::Clipboard {
                let hint = format!(
                    "File path, such as chart_export.{}",
                    self.config.format.file_extension().unwrap_or("png")
                );
                let label = ui.label("File");
                let field = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.save_path)
                            .id(path_field())
                            .hint_text(hint)
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(label.id);
                if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    response = self.request_export(ui.ctx());
                }
                ui.end_row();
            }
        });

        if self.config.format.is_data_format() {
            ui.add_space(theme.spacing.md);
            ui.label(
                RichText::new("Exports the bar data: time, open, high, low, close, and volume.")
                    .size(theme.text.small)
                    .color(theme.ui.text_secondary),
            );
        }
        self.draw_status(ui);

        let (export, cancel) = DialogFooter::new("Export").secondary("Cancel").show(ui);
        if export {
            response = self.request_export(ui.ctx());
        } else if cancel {
            response = ExportResponse::Closed;
        }
        response
    }

    fn draw_status(&self, ui: &mut egui::Ui) {
        match &self.export_status {
            ExportStatus::Idle => {}
            ExportStatus::Exporting => {
                ui.spinner();
                ui.label("Exporting...");
            }
            ExportStatus::Success => {
                ui.success_label("Export successful!");
            }
            ExportStatus::Error(msg) => {
                ui.add_space(ui.active_theme().spacing.md);
                ui.error_label(format!("Error: {msg}"));
            }
        }
    }

    /// The export response for the current choices, or `None` with an error status and the path
    /// field focused when a file format has no path.
    fn request_export(&mut self, ctx: &Context) -> ExportResponse {
        let to_clipboard = self.config.format == ExportFormat::Clipboard;
        let path = self.save_path.trim();
        if to_clipboard || !path.is_empty() {
            self.export_status = ExportStatus::Exporting;
            return ExportResponse::Export {
                config: self.config.clone(),
                save_path: (!to_clipboard).then(|| PathBuf::from(path)),
            };
        }
        self.export_status = ExportStatus::Error("Please enter a save path".to_string());
        ctx.memory_mut(|m| m.request_focus(path_field()));
        ExportResponse::None
    }

    /// Marks the current export as successful
    ///
    /// Call this after completing an export to show success status in the dialog.
    pub fn mark_success(&mut self) {
        self.export_status = ExportStatus::Success;
    }

    /// Marks the current export as failed with an error message
    ///
    /// Call this if an export fails to show the error in the dialog.
    ///
    /// # Arguments
    ///
    /// * `error` - A description of what went wrong
    pub fn mark_error(&mut self, error: String) {
        self.export_status = ExportStatus::Error(error);
    }

    /// Resets the export status to idle
    pub fn reset_status(&mut self) {
        self.export_status = ExportStatus::Idle;
    }
}

/// Quick export button widget for inline export actions
///
/// A simple button that can trigger exports without opening the full dialog.
/// Useful for toolbar actions or quick access buttons.
///
/// # Example
///
/// ```no_run
/// # use egui_charts::ui::export::{ExportButton, ExportFormat};
/// # fn toolbar(ui: &mut egui::Ui) {
/// if ExportButton::new(ExportFormat::Png).show(ui) {
///     // Trigger PNG export
///     println!("Export PNG clicked");
/// }
/// # }
/// ```
pub struct ExportButton {
    format: ExportFormat,
}

impl ExportButton {
    /// Creates a new export button for the specified format
    pub fn new(format: ExportFormat) -> Self {
        Self { format }
    }

    /// Shows the export button in the UI
    ///
    /// Returns `true` if the button was clicked.
    #[must_use = "Button click should be handled"]
    pub fn show(self, ui: &mut egui::Ui) -> bool {
        ui.button(format!("Export {}", self.format.as_str()))
            .on_hover_text(format!("Export chart as {}", self.format.as_str()))
            .clicked()
    }
}

/// Trait for implementing chart export functionality
///
/// Implement this trait on your chart widgets to enable export to multiple formats.
///
/// # Example
///
/// ```no_run
/// use egui_charts::ui::export::{Exportable, ExportConfig, ExportFormat};
///
/// struct MyChart {
///     // chart data
/// }
///
/// impl Exportable for MyChart {
///     fn export(&self, config: &ExportConfig) -> Result<Vec<u8>, String> {
///         match config.format {
///             ExportFormat::Png => {
///                 // Render chart to PNG bytes
///                 Ok(vec![])  // Replace with actual PNG bytes
///             }
///             ExportFormat::Clipboard => {
///                 // Same as PNG
///                 self.export_png()
///             }
///             ExportFormat::Csv => {
///                 // Export bar data as CSV bytes
///                 Ok(b"time,open,high,low,close,volume\n".to_vec())
///             }
///         }
///     }
/// }
/// ```
pub trait Exportable {
    /// Exports the chart with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Export configuration specifying the format
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<u8>)` - The exported chart as raw bytes
    /// * `Err(String)` - An error message if export fails
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The format is not supported
    /// - Rendering fails
    /// - Memory allocation fails
    fn export(&self, config: &ExportConfig) -> Result<Vec<u8>, String>;

    /// Convenience method to export as PNG
    fn export_png(&self) -> Result<Vec<u8>, String> {
        self.export(&ExportConfig {
            format: ExportFormat::Png,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricting_formats_selects_an_offered_format() {
        let dialog = ExportDialog::new().with_formats(&[ExportFormat::Clipboard]);
        assert_eq!(dialog.config.format, ExportFormat::Clipboard);
    }
}
