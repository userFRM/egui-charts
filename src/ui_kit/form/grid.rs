//! Form grid — a 2-column grid with standard spacing for label + control rows.
//!
//! Eliminates the repeated `Grid::new("id").num_columns(2).spacing([lg, md])` boilerplate
//! used across 25+ form dialogs.
//!
//! ```ignore
//! FormGrid::new("canvas_bg").show(ui, |ui| {
//!     ui.label("Type");
//!     ComboBox::from_id_salt("bg_type")...;
//!     ui.end_row();
//! });
//! ```

use crate::tokens::DESIGN_TOKENS;
use egui::Ui;

/// A pre-configured 2-column grid for form layouts.
///
/// Default: 2 columns, `[lg, md]` spacing (horizontal, vertical).
pub struct FormGrid {
    id: String,
    columns: usize,
    h_spacing: f32,
    v_spacing: f32,
}

impl FormGrid {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            columns: 2,
            h_spacing: DESIGN_TOKENS.spacing.lg,
            v_spacing: DESIGN_TOKENS.spacing.md,
        }
    }

    /// Override the number of columns (default: 2).
    #[must_use]
    pub fn columns(mut self, n: usize) -> Self {
        self.columns = n;
        self
    }

    /// Override spacing: `[horizontal, vertical]`.
    #[must_use]
    pub fn spacing(mut self, h: f32, v: f32) -> Self {
        self.h_spacing = h;
        self.v_spacing = v;
        self
    }

    /// Show the grid. The closure should use `ui.label(...); ui.add(...); ui.end_row();` rows.
    pub fn show(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
        egui::Grid::new(self.id)
            .num_columns(self.columns)
            .spacing([self.h_spacing, self.v_spacing])
            .show(ui, add_contents);
    }
}
