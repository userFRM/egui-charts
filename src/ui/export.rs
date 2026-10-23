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
use crate::ui_kit::dialog::titled_window_size;
use egui::Context;
use std::path::PathBuf;

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
    /// This displays a modal dialog with export configuration options.
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

        // Centred horizontally like the crate's other dialogs, but movable. The top edge is the
        // pivot, so the window grows downwards as its content changes instead of around its centre.
        let r = ctx.content_rect();
        let frame = egui::Frame::window(&ctx.global_style());
        egui::Window::new("Export Chart")
            .collapsible(false)
            .resizable(false)
            // egui's default content size from before 0.35, frame and title bar excluded.
            .default_size(titled_window_size(ctx, &frame, egui::vec2(340.0, 420.0)))
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(egui::pos2(r.center().x, r.top() + r.height() * 0.2))
            .show(ctx, |ui| {
                self.draw_format_selection(ui);
                self.draw_export_options(ui);
                self.draw_save_path(ui);
                self.draw_status(ui);
                response = self.draw_action_buttons(ui);
            });

        if matches!(response, ExportResponse::Export { .. }) {
            self.close();
        }
        response
    }

    fn draw_format_selection(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Format:");
            for &format in &self.formats {
                ui.radio_value(&mut self.config.format, format, format.as_str());
            }
        });
        ui.separator();
    }

    fn draw_export_options(&self, ui: &mut egui::Ui) {
        if self.config.format.is_data_format() {
            ui.label("Exports the bar data: time, open, high, low, close, and volume.");
            ui.separator();
        }
    }

    fn draw_save_path(&mut self, ui: &mut egui::Ui) {
        if self.config.format == ExportFormat::Clipboard {
            return;
        }

        let hint = format!(
            "chart_export.{}",
            self.config.format.file_extension().unwrap_or("png")
        );
        ui.horizontal(|ui| {
            ui.label("Save to:");
            ui.add(egui::TextEdit::singleline(&mut self.save_path).hint_text(hint));
        });
        ui.separator();
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
                ui.error_label(format!("Error: {msg}"));
            }
        }
        ui.separator();
    }

    fn draw_action_buttons(&mut self, ui: &mut egui::Ui) -> ExportResponse {
        let mut response = ExportResponse::None;
        ui.horizontal(|ui| {
            if ui.button("Export").clicked() {
                let to_clipboard = self.config.format == ExportFormat::Clipboard;
                let path = self.save_path.trim();
                if to_clipboard || !path.is_empty() {
                    response = ExportResponse::Export {
                        config: self.config.clone(),
                        save_path: (!to_clipboard).then(|| PathBuf::from(path)),
                    };
                    self.export_status = ExportStatus::Exporting;
                } else {
                    self.export_status =
                        ExportStatus::Error("Please enter a save path".to_string());
                }
            }
            if ui.button("Cancel").clicked() {
                response = ExportResponse::Closed;
                self.close();
            }
        });
        response
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
    /// * `error` - A desc of what went wrong
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
/// Useful for toolbar actions or quick access btns.
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
///                 Ok(b"ts,open,high,low,close,volume\n".to_vec())
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
