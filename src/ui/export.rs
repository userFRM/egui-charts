//! Chart export functionality for screenshots and saving charts
//!
//! This module provides widgets and traits for exporting charts to multiple formats:
//! - PNG and SVG images
//! - CSV of the bar data and visible study plots
//! - Clipboard copy of the image
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
//!         // User closed the dialog
//!     }
//!     ExportResponse::None => {
//!         // Dialog is not open or no action taken
//!     }
//! }
//! # }
//! ```

use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::ui_kit::FormGrid;
use crate::ui_kit::dialog::{DialogFooter, dialog_frame, title_row};
use crate::ui_kit::ext::UiExt;
use crate::widget::PixelDensity;
use egui::{Context, Id, Key, Modifiers, RichText};
use std::path::PathBuf;

/// Id of the file path field, so a failed export can focus it.
fn path_field() -> Id {
    Id::NULL.with("egui_charts::export_dialog::path")
}

/// Whether `format` is a raster image, which takes a pixel density.
fn is_raster(format: ExportFormat) -> bool {
    matches!(format, ExportFormat::Png | ExportFormat::Clipboard)
}

/// The largest layout width and height in points at `pixels_per_point`, so the output stays
/// within 7680 by 4320 pixels.
fn max_size(pixels_per_point: u32) -> [u32; 2] {
    [7680 / pixels_per_point, 4320 / pixels_per_point]
}

/// Scales `size` down to fit `max`, keeping its proportion and at least 100 per side.
fn fit(size: [u32; 2], max: [u32; 2]) -> [u32; 2] {
    let scale = (f64::from(max[0]) / f64::from(size[0]))
        .min(f64::from(max[1]) / f64::from(size[1]))
        .min(1.0);
    [0, 1].map(|i| ((f64::from(size[i]) * scale).round() as u32).clamp(100, max[i]))
}

/// Export format options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// PNG raster image format
    Png,
    /// SVG vector image format, from [`Chart::export_svg`](crate::widget::Chart::export_svg)
    Svg,
    /// Copy to system clipboard
    Clipboard,
    /// CSV export of loaded bars and visible study plots
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

/// What an image export shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ExportScope {
    /// The chart the export is for, from [`Chart::export_image`](crate::widget::Chart::export_image)
    /// or its PNG and SVG forms.
    #[default]
    Chart,
    /// The whole layout the host shows, from
    /// [`Chart::export_layout_image`](crate::widget::Chart::export_layout_image) or its PNG and SVG
    /// forms.
    Layout,
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
    /// User closed the dialog
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
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Export format: PNG, SVG, CSV or Clipboard
    pub format: ExportFormat,
    /// Size, header, background, overlay and density options for image exports (PNG, SVG and
    /// clipboard).
    pub image: crate::widget::ImageExportOptions,
    /// What an image export shows. The dialog asks for [`ExportScope::Layout`] only while the
    /// host shows a layout ([`ExportDialog::set_layout_size`]) and for an image format; otherwise
    /// it asks for [`ExportScope::Chart`].
    pub scope: ExportScope,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            format: ExportFormat::Png,
            image: Default::default(),
            scope: ExportScope::Chart,
        }
    }
}

/// Export dialog for configuring and executing chart exports
///
/// The dialog offers:
/// - Format selection among the formats the host offers: PNG, SVG, CSV and Clipboard
/// - Image size in output pixels, which the dialog divides by the pixel density for
///   [`ImageExportOptions::size`](crate::widget::ImageExportOptions::size), with the chart's or
///   layout's size and a proportion lock, pixel density, scope, title, timestamp, watermark,
///   crosshair, overlays and transparent background
/// - File save path entry
///
/// Call [`set_chart_size`](Self::set_chart_size) and [`set_layout_size`](Self::set_layout_size)
/// before [`show`](Self::show), for example each frame, so the dialog can offer the current size
/// and the layout scope.
///
/// The dialog only collects the request; the host performs the export. Use
/// [`with_formats`](Self::with_formats) to offer only the formats the host
/// can produce. The dialog stays open after a request; report its result with
/// [`mark_success`](Self::mark_success) or [`mark_error`](Self::mark_error).
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
#[derive(Debug)]
pub struct ExportDialog {
    is_open: bool,
    config: ExportConfig,
    formats: Vec<ExportFormat>,
    save_path: String,
    export_status: ExportStatus,
    /// Size in points of the chart the host shows, offered as the current size.
    chart_size: Option<egui::Vec2>,
    /// Size in points of the layout the host shows; a size offers the layout scope.
    layout_size: Option<egui::Vec2>,
    /// Width over height kept while one side is edited; `None` while the lock is off.
    proportion: Option<f64>,
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
            chart_size: None,
            layout_size: None,
            proportion: None,
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

    /// Sets the size in points of the chart the export is for, which the dialog offers as the
    /// current image size. `None`, the default, or a size that is not finite and positive offers
    /// none.
    pub fn set_chart_size(&mut self, size: Option<egui::Vec2>) {
        self.chart_size = size.filter(|size| size.is_finite() && size.min_elem() > 0.0);
    }

    /// Sets the size in points of the layout the host shows around the chart. With a size the
    /// dialog offers the layout scope and, with that scope, the layout's size as the current image
    /// size. `None`, the default, or a size that is not finite and positive means the host shows
    /// the chart alone: the dialog hides the scope and asks for [`ExportScope::Chart`].
    pub fn set_layout_size(&mut self, size: Option<egui::Vec2>) {
        self.layout_size = size.filter(|size| size.is_finite() && size.min_elem() > 0.0);
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
        self.show_with(ctx, |_, _| {})
    }

    /// Shows the dialog with host controls below its title.
    /// `content` receives the currently selected format; the host keeps its own control state.
    #[must_use = "Export dialog response should be handled"]
    pub fn show_with(
        &mut self,
        ctx: &Context,
        mut content: impl FnMut(&mut egui::Ui, ExportFormat),
    ) -> ExportResponse {
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
        let frame = dialog_frame(ctx);
        egui::Window::new("Export Chart")
            .title_bar(false)
            .resizable(false)
            .default_width(340.0 + frame.total_margin().sum().x)
            .frame(frame)
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(egui::pos2(r.center().x, r.top() + r.height() * 0.2))
            .show(ctx, |ui| response = self.draw_contents(ui, &mut content));

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

        if matches!(response, ExportResponse::Closed) {
            self.close();
        }
        response
    }

    fn draw_contents(
        &mut self,
        ui: &mut egui::Ui,
        content: &mut impl FnMut(&mut egui::Ui, ExportFormat),
    ) -> ExportResponse {
        let theme = ui.active_theme();
        if title_row(ui, "Export chart", true) {
            return ExportResponse::Closed;
        }

        content(ui, self.config.format);
        let mut response = ExportResponse::None;
        ui.add_enabled_ui(self.export_status != ExportStatus::Exporting, |ui| {
            FormGrid::new("export_form").show(ui, |ui| {
                let label = ui.label(l10n::text(ui.ctx(), "Format"));
                ui.horizontal(|ui| {
                    for &format in &self.formats {
                        if ui
                            .radio_value(
                                &mut self.config.format,
                                format,
                                l10n::text(ui.ctx(), format.as_str()),
                            )
                            .changed()
                        {
                            self.export_status = ExportStatus::Idle;
                        }
                    }
                })
                .response
                .labelled_by(label.id);
                ui.end_row();

                if !self.config.format.is_data_format() {
                    let ppp = if is_raster(self.config.format) {
                        self.config.image.pixel_density.pixels_per_point()
                    } else {
                        1
                    };
                    let max = max_size(ppp);
                    let fitted = fit(self.config.image.size, max);
                    let mut changed = fitted != self.config.image.size;
                    self.config.image.size = fitted;
                    if self.layout_size.is_some() {
                        let label = ui.label(l10n::text(ui.ctx(), "Scope"));
                        ui.horizontal(|ui| {
                            changed |= ui
                                .radio_value(
                                    &mut self.config.scope,
                                    ExportScope::Chart,
                                    l10n::text(ui.ctx(), "This chart"),
                                )
                                .changed();
                            changed |= ui
                                .radio_value(
                                    &mut self.config.scope,
                                    ExportScope::Layout,
                                    l10n::text(ui.ctx(), "Whole layout"),
                                )
                                .changed();
                        })
                        .response
                        .labelled_by(label.id);
                        ui.end_row();
                    }
                    ui.label(l10n::text(ui.ctx(), "Image size"));
                    ui.horizontal_wrapped(|ui| {
                        let size = &mut self.config.image.size;
                        // The fields show output pixels; the options keep the layout in points.
                        let mut pixels = size.map(|side| side * ppp);
                        ui.add(
                            egui::DragValue::new(&mut pixels[0])
                                .range(100 * ppp..=max[0] * ppp)
                                .speed(ppp)
                                .update_while_editing(false)
                                .suffix(l10n::text(ui.ctx(), " px")),
                        );
                        ui.label("×");
                        ui.add(
                            egui::DragValue::new(&mut pixels[1])
                                .range(100 * ppp..=max[1] * ppp)
                                .speed(ppp)
                                .update_while_editing(false)
                                .suffix(l10n::text(ui.ctx(), " px")),
                        );
                        let edited =
                            pixels.map(|side| (f64::from(side) / f64::from(ppp)).round() as u32);
                        let width_changed = edited[0] != size[0];
                        let height_changed = edited[1] != size[1];
                        *size = edited;
                        changed |= width_changed || height_changed;
                        if let Some(ratio) = self.proportion {
                            if width_changed {
                                let height = (f64::from(size[0]) / ratio).round() as u32;
                                size[1] = height.clamp(100, max[1]);
                                if height != size[1] {
                                    size[0] = (f64::from(size[1]) * ratio).round() as u32;
                                    size[0] = size[0].clamp(100, max[0]);
                                }
                            } else if height_changed {
                                let width = (f64::from(size[1]) * ratio).round() as u32;
                                size[0] = width.clamp(100, max[0]);
                                if width != size[0] {
                                    size[1] = (f64::from(size[0]) / ratio).round() as u32;
                                    size[1] = size[1].clamp(100, max[1]);
                                }
                            }
                        }
                        let current = if self.config.scope == ExportScope::Layout {
                            self.layout_size.or(self.chart_size)
                        } else {
                            self.chart_size
                        };
                        if let Some(current) = current
                            && ui.button(l10n::text(ui.ctx(), "Current size")).clicked()
                        {
                            *size = fit(
                                [current.x, current.y]
                                    .map(|s| (s.round().max(1.0) as u32).max(100)),
                                max,
                            );
                            if self.proportion.is_some() {
                                self.proportion = Some(f64::from(size[0]) / f64::from(size[1]));
                            }
                            changed = true;
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    let mut keep = self.proportion.is_some();
                    if ui
                        .checkbox(&mut keep, l10n::text(ui.ctx(), "Keep proportions"))
                        .changed()
                    {
                        let [width, height] = self.config.image.size;
                        self.proportion = keep.then(|| f64::from(width) / f64::from(height));
                        changed = true;
                    }
                    ui.end_row();
                    if is_raster(self.config.format) {
                        let label = ui.label(l10n::text(ui.ctx(), "Pixel density"));
                        ui.horizontal_wrapped(|ui| {
                            for density in PixelDensity::ALL {
                                if ui
                                    .radio_value(
                                        &mut self.config.image.pixel_density,
                                        density,
                                        format!("{}×", density.pixels_per_point()),
                                    )
                                    .changed()
                                {
                                    let ppp = density.pixels_per_point();
                                    self.config.image.size =
                                        fit(self.config.image.size, max_size(ppp));
                                    changed = true;
                                }
                            }
                            let ppp = self.config.image.pixel_density.pixels_per_point();
                            if ppp > 1 {
                                let [width, height] = self.config.image.size.map(|side| side * ppp);
                                ui.label(
                                    RichText::new(l10n::format_text(
                                        ui.ctx(),
                                        "{width} × {height} px",
                                        &[
                                            ("width", width.to_string()),
                                            ("height", height.to_string()),
                                        ],
                                    ))
                                    .size(theme.text.small)
                                    .color(theme.ui.text_secondary),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id);
                        ui.end_row();
                    }
                    ui.label(l10n::text(ui.ctx(), "Include"));
                    ui.vertical(|ui| {
                        changed |= ui
                            .checkbox(
                                &mut self.config.image.include_title,
                                l10n::text(ui.ctx(), "Chart title"),
                            )
                            .changed();
                        changed |= ui
                            .checkbox(
                                &mut self.config.image.include_timestamp,
                                l10n::text(ui.ctx(), "Export timestamp"),
                            )
                            .changed();
                        changed |= ui
                            .checkbox(
                                &mut self.config.image.include_watermark,
                                l10n::text(ui.ctx(), "Watermark"),
                            )
                            .changed();
                        changed |= ui
                            .checkbox(
                                &mut self.config.image.include_crosshair,
                                l10n::text(ui.ctx(), "Crosshair"),
                            )
                            .changed();
                        changed |= ui
                            .checkbox(
                                &mut self.config.image.include_foreground,
                                l10n::text(ui.ctx(), "Overlays"),
                            )
                            .changed();
                        changed |= ui
                            .checkbox(
                                &mut self.config.image.transparent_background,
                                l10n::text(ui.ctx(), "Transparent background"),
                            )
                            .changed();
                    });
                    ui.end_row();
                    if changed {
                        self.reset_status();
                    }
                }

                if self.config.format != ExportFormat::Clipboard {
                    let hint = l10n::format_text(
                        ui.ctx(),
                        "File path, such as chart_export.{extension}",
                        &[(
                            "extension",
                            self.config.format.file_extension().unwrap_or("png"),
                        )],
                    );
                    let label = ui.label(l10n::text(ui.ctx(), "File"));
                    let field = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.save_path)
                                .id(path_field())
                                .hint_text(l10n::text(ui.ctx(), hint))
                                .desired_width(f32::INFINITY),
                        )
                        .labelled_by(label.id);
                    if field.changed() {
                        self.reset_status();
                    }
                    if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        response = self.request_export(ui.ctx());
                    }
                    ui.end_row();
                }
            });
        });

        if self.config.format.is_data_format() {
            ui.add_space(theme.spacing.md);
            ui.label(
                RichText::new(l10n::text(ui.ctx(), "Exports loaded bars and visible study values, using the chart time zone and price precision."))
                    .size(theme.text.small)
                    .color(theme.ui.text_secondary),
            );
        }
        self.draw_status(ui);

        let (export, cancel) = DialogFooter::new("Export")
            .primary_enabled(self.export_status != ExportStatus::Exporting)
            .secondary("Cancel")
            .show(ui);
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
                ui.label(l10n::text(ui.ctx(), "Exporting..."));
            }
            ExportStatus::Success => {
                if self.config.format == ExportFormat::Clipboard {
                    ui.success_label("Export successful!");
                } else {
                    ui.success_label(l10n::format_text(
                        ui.ctx(),
                        "Export successful: {path}",
                        &[("path", self.save_path.trim())],
                    ));
                }
            }
            ExportStatus::Error(msg) => {
                ui.add_space(ui.active_theme().spacing.md);
                ui.error_label(l10n::format_text(
                    ui.ctx(),
                    "Error: {message}",
                    &[("message", msg.as_str())],
                ));
            }
        }
    }

    /// The export response for the current choices, or `None` with an error status and the path
    /// field focused when a file format has no path.
    fn request_export(&mut self, ctx: &Context) -> ExportResponse {
        if self.export_status == ExportStatus::Exporting {
            return ExportResponse::None;
        }
        let to_clipboard = self.config.format == ExportFormat::Clipboard;
        let path = self.save_path.trim();
        if to_clipboard || !path.is_empty() {
            self.export_status = ExportStatus::Exporting;
            let mut config = self.config.clone();
            if self.layout_size.is_none() || config.format.is_data_format() {
                config.scope = ExportScope::Chart;
            }
            return ExportResponse::Export {
                config,
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
#[derive(Debug)]
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
        ui.button(l10n::format_text(
            ui.ctx(),
            "Export {0}",
            &[("0", self.format.as_str().to_string())],
        ))
        .on_hover_text(l10n::format_text(
            ui.ctx(),
            "Export chart as {0}",
            &[("0", self.format.as_str().to_string())],
        ))
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
///             ExportFormat::Svg => {
///                 // Render chart to SVG text bytes
///                 Ok(b"<svg/>".to_vec())
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
            ..Default::default()
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

    /// Guards #3596 and #3961: image choices reach the request and survive reopening, layout
    /// scope is requested only while the host offers it and the format is an image, and the size
    /// fields hold the output pixels at every density, as the output size beside the density
    /// shows, while SVG keeps the document size in points.
    #[test]
    fn image_choices_reach_the_request_and_survive_reopening() {
        use egui::accesskit::Role;
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };

        let mut dialog = ExportDialog::new().with_formats(&[
            ExportFormat::Png,
            ExportFormat::Svg,
            ExportFormat::Csv,
            ExportFormat::Clipboard,
        ]);
        dialog.save_path = "chart.png".into();
        dialog.set_chart_size(Some(egui::vec2(1203.4, 611.6)));
        dialog.set_layout_size(Some(egui::vec2(1900.0, 1000.0)));
        dialog.open();
        let mut h = Harness::builder()
            .with_size(egui::vec2(800.0, 900.0))
            .with_context_setup(|ctx| {
                crate::icons::install_loader(ctx);
                crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
            })
            .build_ui_state(
                |ui, (dialog, request): &mut (ExportDialog, Option<ExportConfig>)| {
                    if let ExportResponse::Export { config, .. } = dialog.show(ui.ctx()) {
                        *request = Some(config);
                    }
                },
                (dialog, None),
            );
        h.run_steps(2);
        let click = |h: &mut Harness<'_, (ExportDialog, Option<ExportConfig>)>, label: &str| {
            h.get_by_label(label).click();
            h.run_steps(2);
        };
        let edit =
            |h: &mut Harness<'_, (ExportDialog, Option<ExportConfig>)>, side: usize, value: f64| {
                h.get_all_by_role(Role::SpinButton)
                    .nth(side)
                    .unwrap()
                    .set_value_accesskit(value);
                h.run_steps(2);
            };
        let fields = |h: &Harness<'_, (ExportDialog, Option<ExportConfig>)>| {
            h.get_all_by_role(Role::SpinButton)
                .map(|field| field.accesskit_node().numeric_value().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(fields(&h), [1440.0, 900.0]);
        click(&mut h, "Overlays");
        assert!(!h.state().0.config.image.include_foreground);
        click(&mut h, "2×");
        assert_eq!(h.state().0.config.image.pixel_density, PixelDensity::Two);
        h.get_by_label("2880 × 1800 px");
        assert_eq!(fields(&h), [2880.0, 1800.0]);
        click(&mut h, "Whole layout");
        assert_eq!(h.state().0.config.scope, ExportScope::Layout);
        click(&mut h, "Current size");
        assert_eq!(h.state().0.config.image.size, [1900, 1000]);
        click(&mut h, "Keep proportions");
        edit(&mut h, 0, 1901.0);
        assert_eq!(h.state().0.config.image.size, [951, 501]);
        assert_eq!(fields(&h), [1902.0, 1002.0]);
        h.get_by_label("1902 × 1002 px");
        edit(&mut h, 0, 1900.0);
        assert_eq!(h.state().0.config.image.size, [950, 500]);
        edit(&mut h, 1, 600.0);
        assert_eq!(h.state().0.config.image.size, [570, 300]);
        edit(&mut h, 0, 200.0);
        assert_eq!(h.state().0.config.image.size, [190, 100]);
        click(&mut h, "This chart");
        click(&mut h, "Current size");
        assert_eq!(h.state().0.config.image.size, [1203, 612]);
        edit(&mut h, 0, 1202.0);
        assert_eq!(h.state().0.config.image.size, [601, 306]);
        click(&mut h, "Whole layout");
        click(&mut h, "Cancel");
        assert!(!h.state().0.is_open());
        h.state_mut().0.open();
        h.run_steps(2);
        let dialog = &h.state().0;
        assert!(!dialog.config.image.include_foreground);
        assert_eq!(dialog.config.image.pixel_density, PixelDensity::Two);
        assert_eq!(dialog.config.scope, ExportScope::Layout);
        assert_eq!(dialog.config.image.size, [601, 306]);
        assert!(dialog.proportion.is_some());
        assert_eq!(
            h.get_by_role_and_label(Role::CheckBox, "Overlays")
                .accesskit_node()
                .toggled(),
            Some(egui::accesskit::Toggled::False)
        );
        assert_eq!(
            h.get_by_role_and_label(Role::RadioButton, "2×")
                .accesskit_node()
                .toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        assert_eq!(
            h.get_by_role_and_label(Role::RadioButton, "Whole layout")
                .accesskit_node()
                .toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        h.key_press(Key::Enter);
        h.run_steps(2);
        let config = h.state().1.as_ref().unwrap();
        assert_eq!(config.scope, ExportScope::Layout);
        assert_eq!(config.image.pixel_density, PixelDensity::Two);
        assert!(!config.image.include_foreground);
        assert_eq!(config.image.size, [601, 306]);

        h.state_mut().0.mark_success();
        h.state_mut().0.set_layout_size(None);
        h.run_steps(2);
        assert!(h.query_by_label("Whole layout").is_none());
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().1.as_ref().unwrap().scope, ExportScope::Chart);
        assert_eq!(h.state().0.config.scope, ExportScope::Layout);
        h.state_mut().0.mark_success();
        h.state_mut()
            .0
            .set_layout_size(Some(egui::vec2(1900.0, 1000.0)));
        h.run_steps(2);
        click(&mut h, "CSV");
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().1.as_ref().unwrap().scope, ExportScope::Chart);
        assert_eq!(h.state().0.config.scope, ExportScope::Layout);
        h.state_mut().0.mark_success();
        click(&mut h, "PNG");
        click(&mut h, "Keep proportions");
        edit(&mut h, 0, 6000.0);
        edit(&mut h, 1, 4000.0);
        click(&mut h, "3×");
        assert_eq!(h.state().0.config.image.size, [2160, 1440]);
        assert_eq!(fields(&h), [6480.0, 4320.0]);
        let width = h.get_all_by_role(Role::SpinButton).next().unwrap();
        assert_eq!(width.accesskit_node().max_numeric_value(), Some(7680.0));
        width.click();
        h.run_steps(2);
        for digit in ["7", "0", "0", "0"] {
            h.event(egui::Event::Text(digit.into()));
            h.run_steps(1);
        }
        // Enter applies the typed width and exports with it.
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state_mut().1.take().unwrap().image.size, [2333, 1440]);
        assert_eq!(h.state().0.config.image.size, [2333, 1440]);
        assert_eq!(fields(&h), [6999.0, 4320.0]);
        h.state_mut().0.mark_success();
        click(&mut h, "SVG");
        assert_eq!(fields(&h), [2333.0, 1440.0]);
        assert!(h.query_by_label("2×").is_none());
        h.get_by_label("Overlays");
        assert_eq!(h.state().0.config.image.pixel_density, PixelDensity::Three);
        click(&mut h, "Clipboard");
        h.get_by_label("3×");
        assert_eq!(fields(&h), [6999.0, 4320.0]);
        h.get_by_label("6999 × 4320 px");
        for size in [
            egui::vec2(f32::NAN, 100.0),
            egui::vec2(100.0, f32::INFINITY),
            egui::vec2(0.0, 100.0),
            egui::vec2(100.0, -1.0),
        ] {
            h.state_mut().0.set_chart_size(Some(size));
            h.state_mut().0.set_layout_size(Some(size));
            h.run_steps(2);
            assert!(h.query_by_label("Current size").is_none());
            assert!(h.query_by_label("Whole layout").is_none());
        }
        h.state_mut()
            .0
            .set_chart_size(Some(egui::vec2(80.4, 611.6)));
        h.run_steps(2);
        click(&mut h, "Current size");
        assert_eq!(h.state().0.config.image.size, [100, 612]);
    }

    #[test]
    fn export_results_stay_visible_until_closed() {
        for (format, path, success) in [
            (
                ExportFormat::Png,
                "chart.png",
                "Export successful: chart.png",
            ),
            (ExportFormat::Csv, "bars.csv", "Export successful: bars.csv"),
            (ExportFormat::Clipboard, "", "Export successful!"),
        ] {
            for (theme, width, error) in [
                (crate::theme::Theme::light(), 800.0, false),
                (crate::theme::Theme::dark(), 800.0, true),
                (crate::theme::Theme::light(), 320.0, true),
                (crate::theme::Theme::dark(), 320.0, false),
            ] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut dialog = ExportDialog::new();
                dialog.config.format = format;
                dialog.save_path = format!("  {path}  ");
                dialog.open();
                let frame = |dialog: &mut ExportDialog, key: Option<Key>| {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 600.0),
                        )),
                        events: key
                            .map(|key| egui::Event::Key {
                                key,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            })
                            .into_iter()
                            .collect(),
                        ..Default::default()
                    };
                    let mut response = ExportResponse::None;
                    let output = ctx.run_ui(input, |ui| response = dialog.show(ui.ctx()));
                    (response, output)
                };
                frame(&mut dialog, None).1.drop_without_applying_deltas();
                let (response, output) = frame(&mut dialog, Some(Key::Enter));
                output.drop_without_applying_deltas();
                let ExportResponse::Export { save_path, .. } = response else {
                    panic!("expected an export request for {format:?}");
                };
                assert_eq!(
                    save_path,
                    (format != ExportFormat::Clipboard).then(|| PathBuf::from(path))
                );
                assert!(
                    dialog.is_open(),
                    "submission must keep the result dialog open"
                );
                assert!(
                    matches!(dialog.request_export(&ctx), ExportResponse::None),
                    "a pending export must not be submitted again"
                );
                if error {
                    dialog.mark_error("Permission denied".into());
                } else {
                    dialog.mark_success();
                }
                frame(&mut dialog, None).1.drop_without_applying_deltas();
                let (response, output) = frame(&mut dialog, None);
                assert!(matches!(response, ExportResponse::None));
                let expected = if error {
                    "Error: Permission denied"
                } else {
                    success
                };
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == expected)
                }), "missing result: {expected}");
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape {
                        let rect = text.galley.rect.translate(text.pos.to_vec2());
                        assert!(
                            rect.left() >= 0.0 && rect.right() <= width,
                            "text outside {width}px window: {} at {rect:?}",
                            text.galley.text()
                        );
                    }
                }
                output.drop_without_applying_deltas();
                let (response, output) = frame(&mut dialog, Some(Key::Escape));
                output.drop_without_applying_deltas();
                assert!(matches!(response, ExportResponse::Closed));
                assert!(!dialog.is_open());
            }
        }
    }
}
