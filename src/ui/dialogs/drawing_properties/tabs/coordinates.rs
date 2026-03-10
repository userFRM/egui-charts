//! Coordinates tab - price and time points.

use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::FormGrid;
use egui::Ui;

/// Coordinate point data
#[derive(Clone, Debug, Default)]
pub struct CoordinatePoint {
    pub price: String,
    pub date: String,
    pub time: String,
}

/// Coordinates state
#[derive(Clone, Debug, Default)]
pub struct CoordinatesState {
    pub points: Vec<CoordinatePoint>,
}

/// Coordinates tab renderer
pub struct CoordinatesTab;

impl CoordinatesTab {
    pub fn show(ui: &mut Ui, state: &mut CoordinatesState) {
        ui.space_md();

        if state.points.is_empty() {
            state.points.push(CoordinatePoint::default());
            state.points.push(CoordinatePoint::default());
        }

        for (i, point) in state.points.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.strong(format!("Point {}", i + 1));
                ui.space_sm();

                FormGrid::new(format!("coord_grid_{i}"))
                    .spacing(DESIGN_TOKENS.spacing.lg, DESIGN_TOKENS.spacing.sm)
                    .show(ui, |ui| {
                        ui.label("Price:");
                        ui.add(
                            egui::TextEdit::singleline(&mut point.price)
                                .desired_width(DESIGN_TOKENS.sizing.dialog.input_width),
                        );
                        ui.end_row();

                        ui.label("Date:");
                        ui.add(
                            egui::TextEdit::singleline(&mut point.date)
                                .desired_width(DESIGN_TOKENS.sizing.dialog.input_width)
                                .hint_text("YYYY-MM-DD"),
                        );
                        ui.end_row();

                        ui.label("Time:");
                        ui.add(
                            egui::TextEdit::singleline(&mut point.time)
                                .desired_width(DESIGN_TOKENS.sizing.dialog.input_width)
                                .hint_text("HH:MM"),
                        );
                        ui.end_row();
                    });
            });
            ui.space_sm();
        }
    }
}
