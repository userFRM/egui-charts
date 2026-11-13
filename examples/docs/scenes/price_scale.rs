use eframe::egui;
use egui_charts::{
    Chart,
    model::{Bar, SeriesScale},
    scales::PriceScaleMode,
    widget::{SeriesId, SeriesOptions},
};

/// Edit either of two independent scales on the same plot.
pub struct PriceScale {
    comparison: SeriesId,
    selected: SeriesId,
}

impl PriceScale {
    /// Add a positive-valued comparison with its own labelled price scale.
    pub fn new(chart: &mut Chart) -> Self {
        let bars = chart
            .bars(SeriesId::MAIN)
            .iter()
            .map(|bar| Bar::from_value(bar.time, bar.close * 10.0))
            .collect();
        let comparison = chart.add_series(SeriesOptions {
            name: "COMPARISON".into(),
            scale: SeriesScale::Separate(1),
            ..Default::default()
        });
        chart.set_data(comparison, bars);
        Self {
            comparison,
            selected: SeriesId::MAIN,
        }
    }

    /// Change only the selected scale; keep user edits between frames.
    pub fn show(&mut self, ui: &mut egui::Ui, chart: &mut Chart) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Edit scale:");
            ui.selectable_value(&mut self.selected, SeriesId::MAIN, "Main");
            ui.selectable_value(&mut self.selected, self.comparison, "Comparison");
        });
        let scale = chart.series_price_scale_mut(self.selected).unwrap();
        let previous = *scale.options();
        let options = scale.options_mut();
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("mode")
                .selected_text(options.mode.to_string())
                .show_ui(ui, |ui| {
                    for mode in [
                        PriceScaleMode::Normal,
                        PriceScaleMode::Logarithmic,
                        PriceScaleMode::Percentage,
                        PriceScaleMode::IndexedTo100,
                    ] {
                        ui.selectable_value(&mut options.mode, mode, mode.to_string());
                    }
                });
            ui.checkbox(&mut options.invert_scale, "Invert");
        });
        ui.horizontal_wrapped(|ui| {
            let margins = &mut options.scale_margins;
            ui.label("Margins:");
            ui.add(
                egui::DragValue::new(&mut margins.top)
                    .range(0.0..=0.4)
                    .speed(0.005)
                    .prefix("Top "),
            );
            ui.add(
                egui::DragValue::new(&mut margins.bottom)
                    .range(0.0..=0.4)
                    .speed(0.005)
                    .prefix("Bottom "),
            );
        });
        // A manual range uses the previous mode's units; refit after an option edit.
        if *options != previous {
            scale.reset();
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Crop vertically").clicked() {
                let range = scale.range();
                let inset = range.length() * 0.25;
                scale.set_range(range.min + inset, range.max - inset);
            }
            if ui.button("Autoscale").clicked() {
                scale.reset();
            }
            ui.label(if scale.is_manual() {
                "Manual range"
            } else {
                "Automatic range"
            });
        });
    }
}
