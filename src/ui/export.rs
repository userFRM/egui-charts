//! Chart export functionality for screenshots and saving charts
//!
//! This module provides widgets and traits for exporting charts to multiple formats:
//! - PNG images (with optional transparency)
//! - SVG vector graphics
//! - Clipboard copy
//!
//! # Example
//!
//! ```no_run
//! use egui_open_trading_charts_rs::{ExportDialog, ExportResponse, ExportFormat};
//!
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
//! ```

use crate::ext::UiExt;
use egui::Context;
use std::path::PathBuf;

/// Export format options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// PNG raster image format
    Png,
    /// SVG vector graphics format
    Svg,
    /// Copy to system clipboard
    Clipboard,
    /// CSV data export (OHLCV + optional indicators)
    Csv,
}

impl ExportFormat {
    /// Returns the human-readable name of this format
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Svg => "SVG",
            Self::Clipboard => "Clipboard",
            Self::Csv => "CSV",
        }
    }

    /// Returns the file extension for this format (if applicable)
    pub fn file_extension(&self) -> Option<&'static str> {
        match self {
            Self::Png => Some("png"),
            Self::Svg => Some("svg"),
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
/// use egui_open_trading_charts_rs::{ExportConfig, ExportFormat};
///
/// let config = ExportConfig {
///     format: ExportFormat::Png,
///     include_title: true,
///     include_timestamp: true,
///     include_watermark: false,
///     width: Some(1920),
///     height: Some(1080),
///     transparent_background: false,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Export format (PNG, SVG, or Clipboard)
    pub format: ExportFormat,
    /// Include chart title in the export
    pub include_title: bool,
    /// Include ts in the export
    pub include_timestamp: bool,
    /// Include watermark in the export
    pub include_watermark: bool,
    /// Export width in pixels (None = use current chart size)
    pub width: Option<u32>,
    /// Export height in pixels (None = use current chart size)
    pub height: Option<u32>,
    /// Use transparent background (PNG only, ignored for other formats)
    pub transparent_background: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            format: ExportFormat::Png,
            include_title: true,
            include_timestamp: true,
            include_watermark: false,
            width: None,
            height: None,
            transparent_background: false,
        }
    }
}

/// Export dialog for configuring and executing chart exports
///
/// This dialog provides a full UI for configuring export options including:
/// - Format selection (PNG, SVG, Clipboard)
/// - Size configuration
/// - Export options (title, ts, watermark)
/// - File save path selection
///
/// # Example
///
/// ```no_run
/// use egui_open_trading_charts_rs::{ExportDialog, ExportResponse};
///
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
/// ```
pub struct ExportDialog {
    is_open: bool,
    config: ExportConfig,
    save_path: Option<PathBuf>,
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
            save_path: None,
            export_status: ExportStatus::Idle,
        }
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
    /// # use egui_open_trading_charts_rs::{ExportDialog, ExportResponse};
    /// # let ctx = todo!();
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
    /// ```
    #[must_use = "Export dialog response should be handled"]
    pub fn show(&mut self, ctx: &Context) -> ExportResponse {
        if !self.is_open {
            return ExportResponse::None;
        }

        let mut response = ExportResponse::None;

        egui::Window::new("Export Chart")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                self.draw_format_selection(ui);
                self.draw_export_options(ui);
                self.draw_size_options(ui);
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
            ui.radio_value(&mut self.config.format, ExportFormat::Png, "PNG");
            ui.radio_value(&mut self.config.format, ExportFormat::Svg, "SVG");
            ui.radio_value(&mut self.config.format, ExportFormat::Csv, "CSV");
            ui.radio_value(
                &mut self.config.format,
                ExportFormat::Clipboard,
                "Clipboard",
            );
        });
        ui.separator();
    }

    fn draw_export_options(&mut self, ui: &mut egui::Ui) {
        if self.config.format.is_data_format() {
            // CSV data export has no image-specific options
            ui.label("Exports visible OHLCV bar data and active indicator values.");
            ui.separator();
            return;
        }

        ui.checkbox(&mut self.config.include_title, "Include title");
        ui.checkbox(&mut self.config.include_timestamp, "Include ts");
        ui.checkbox(&mut self.config.include_watermark, "Include watermark");

        if self.config.format == ExportFormat::Png {
            ui.checkbox(
                &mut self.config.transparent_background,
                "Transparent background",
            );
        }
        ui.separator();
    }

    fn draw_size_options(&mut self, ui: &mut egui::Ui) {
        if self.config.format.is_data_format() {
            // CSV data export has no size options
            return;
        }

        ui.label("Export Size:");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.config.width, None, "Current size");
            if ui.radio(self.config.width.is_some(), "Custom").clicked() {
                self.config.width = Some(1920);
                self.config.height = Some(1080);
            }
        });

        if let Some(width) = &mut self.config.width {
            ui.horizontal(|ui| {
                ui.label("Width:");
                ui.add(egui::DragValue::new(width).range(100..=7680));
                ui.label("Height:");
                let height = self.config.height.get_or_insert(1080);
                ui.add(egui::DragValue::new(height).range(100..=4320));
            });
        }
        ui.separator();
    }

    fn draw_save_path(&mut self, ui: &mut egui::Ui) {
        if self.config.format == ExportFormat::Clipboard {
            return;
        }

        ui.horizontal(|ui| {
            ui.label("Save to:");
            if let Some(path) = &self.save_path {
                ui.label(path.display().to_string());
            } else {
                ui.label("(Select location)");
            }
            if ui.button("Browse...").clicked() {
                self.save_path = Some(PathBuf::from(format!(
                    "chart_export.{}",
                    self.config.format.file_extension().unwrap_or("png")
                )));
            }
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
                if self.config.format == ExportFormat::Clipboard || self.save_path.is_some() {
                    response = ExportResponse::Export {
                        config: self.config.clone(),
                        save_path: self.save_path.clone(),
                    };
                    self.export_status = ExportStatus::Exporting;
                } else {
                    self.export_status =
                        ExportStatus::Error("Please select a save location".to_string());
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
/// # use egui_open_trading_charts_rs::{ExportButton, ExportFormat};
/// # let ui = todo!();
/// if ExportButton::new(ExportFormat::Png).show(ui) {
///     // Trigger PNG export
///     println!("Export PNG clicked");
/// }
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
/// use egui_open_trading_charts_rs::{Exportable, ExportConfig, ExportFormat};
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
///             ExportFormat::Svg => {
///                 // Render chart to SVG bytes
///                 Ok(vec![])  // Replace with actual SVG bytes
///             }
///             ExportFormat::Clipboard => {
///                 // Same as PNG
///                 self.export_png(1920, 1080, false)
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
    /// * `config` - Export configuration specifying format, size, and options
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
    ///
    /// # Arguments
    ///
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `transparent` - Whether to use a transparent background
    fn export_png(&self, width: u32, height: u32, transparent: bool) -> Result<Vec<u8>, String> {
        self.export(&ExportConfig {
            format: ExportFormat::Png,
            width: Some(width),
            height: Some(height),
            transparent_background: transparent,
            ..Default::default()
        })
    }

    /// Convenience method to export as SVG
    fn export_svg(&self) -> Result<Vec<u8>, String> {
        self.export(&ExportConfig {
            format: ExportFormat::Svg,
            ..Default::default()
        })
    }

    /// Convenience method to copy to clipboard
    ///
    /// Note: This default implementation just exports as PNG.
    /// Actual clipboard integration requires platform-specific code.
    fn copy_to_clipboard(&self, _ctx: &Context) -> Result<(), String> {
        let _bytes = self.export_png(1920, 1080, false)?;
        // In a real implementation, this would use a clipboard library
        // like arboard or copypasta
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_format_as_str() {
        assert_eq!(ExportFormat::Png.as_str(), "PNG");
        assert_eq!(ExportFormat::Svg.as_str(), "SVG");
        assert_eq!(ExportFormat::Clipboard.as_str(), "Clipboard");
        assert_eq!(ExportFormat::Csv.as_str(), "CSV");
    }

    #[test]
    fn test_export_format_file_extension() {
        assert_eq!(ExportFormat::Png.file_extension(), Some("png"));
        assert_eq!(ExportFormat::Svg.file_extension(), Some("svg"));
        assert_eq!(ExportFormat::Clipboard.file_extension(), None);
        assert_eq!(ExportFormat::Csv.file_extension(), Some("csv"));
    }

    #[test]
    fn test_export_format_is_data_format() {
        assert!(!ExportFormat::Png.is_data_format());
        assert!(!ExportFormat::Svg.is_data_format());
        assert!(!ExportFormat::Clipboard.is_data_format());
        assert!(ExportFormat::Csv.is_data_format());
    }

    #[test]
    fn test_export_config_default() {
        let config = ExportConfig::default();
        assert_eq!(config.format, ExportFormat::Png);
        assert!(config.include_title);
        assert!(config.include_timestamp);
        assert!(!config.include_watermark);
        assert_eq!(config.width, None);
        assert_eq!(config.height, None);
        assert!(!config.transparent_background);
    }

    #[test]
    fn test_export_dialog_creation() {
        let dialog = ExportDialog::new();
        assert!(!dialog.is_open());
        assert_eq!(dialog.config.format, ExportFormat::Png);
    }

    #[test]
    fn test_export_dialog_open_close() {
        let mut dialog = ExportDialog::new();
        assert!(!dialog.is_open());

        dialog.open();
        assert!(dialog.is_open());

        dialog.close();
        assert!(!dialog.is_open());
    }
}
