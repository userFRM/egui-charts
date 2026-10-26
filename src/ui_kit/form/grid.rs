//! Form grid: a two-column grid with standard spacing for label and control rows.
//!
//! Replaces the repeated `Grid::new("id").num_columns(2).spacing([lg, md])` setup in the
//! settings, properties and alert dialogs.
//!
//! ```
//! # use egui_charts::ui_kit::FormGrid;
//! # egui::__run_test_ui(|ui| {
//! # let mut bg_type = "Solid";
//! FormGrid::new("canvas_bg").show(ui, |ui| {
//!     ui.label("Type");
//!     egui::ComboBox::from_id_salt("bg_type")
//!         .selected_text(bg_type)
//!         .show_ui(ui, |ui| {
//!             ui.selectable_value(&mut bg_type, "Solid", "Solid");
//!             ui.selectable_value(&mut bg_type, "Gradient", "Gradient");
//!         });
//!     ui.end_row();
//! });
//! # });
//! ```

use crate::theme::ThemeExt;
use egui::Ui;

/// A pre-configured 2-column grid for form layouts.
///
/// Default: 2 columns, `[lg, md]` spacing (horizontal, vertical).
pub struct FormGrid {
    id: String,
    columns: usize,
    spacing: Option<[f32; 2]>,
}

impl FormGrid {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            columns: 2,
            spacing: None,
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
        self.spacing = Some([h, v]);
        self
    }

    /// Show the grid. The closure should use `ui.label(...); ui.add(...); ui.end_row();` rows.
    pub fn show(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
        let theme = ui.active_theme();
        egui::Grid::new(self.id)
            .num_columns(self.columns)
            .spacing(self.spacing.unwrap_or([theme.spacing.lg, theme.spacing.md]))
            .show(ui, add_contents);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Theme, apply_to_egui};

    #[test]
    fn spacing_follows_the_theme_unless_the_host_sets_it() {
        let ctx = egui::Context::default();
        for custom in [false, true] {
            let mut theme = Theme::light();
            theme.spacing.lg = 21.0;
            theme.spacing.md = 17.0;
            apply_to_egui(&ctx, &theme);
            let expected = if custom { [3.0, 5.0] } else { [21.0, 17.0] };
            let mut cells = [egui::Rect::NOTHING; 4];
            for _ in 0..3 {
                ctx.run_ui(Default::default(), |ui| {
                    let grid = FormGrid::new(format!("spacing_{custom}"));
                    let grid = if custom { grid.spacing(3.0, 5.0) } else { grid };
                    grid.show(ui, |ui| {
                        for (at, cell) in cells.iter_mut().enumerate() {
                            *cell = ui
                                .allocate_exact_size(egui::vec2(80.0, 20.0), egui::Sense::hover())
                                .0;
                            if at % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                })
                .drop_without_applying_deltas();
            }
            assert_eq!(cells[1].left() - cells[0].right(), expected[0]);
            assert_eq!(cells[2].top() - cells[0].bottom(), expected[1]);
        }
    }
}
