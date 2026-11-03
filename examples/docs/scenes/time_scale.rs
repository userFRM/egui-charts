use chrono::TimeDelta;
use eframe::egui;
use egui_charts::{
    Chart,
    model::LogicalRange,
    widget::{SeriesId, VisibleTimeRange},
};

/// Fit, scroll and address the same history in logical or UTC coordinates.
pub fn show(ui: &mut egui::Ui, chart: &mut Chart) {
    // Range setters use the plot width from the last show, including after a resize.
    let ready = chart.time_scale_size().x > 0.0;
    ui.add_enabled_ui(ready, |ui| {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Fit history").clicked() {
                chart.fit_content();
            }
            if ui.button("Back 30 bars").clicked() {
                chart.scroll_to_position(-30.0, true);
            }
            if ui.button("Live edge").clicked() {
                chart.scroll_to_real_time(true);
            }
            if ui.button("Reset time").clicked() {
                chart.reset_time_scale();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Logical 20..59").clicked() {
                chart.set_visible_logical_range(LogicalRange {
                    left: 20.0,
                    right: 59.0,
                });
            }
            if ui.button("UTC minutes 20..59").clicked() {
                let start = chart.bars(SeriesId::MAIN)[0].time;
                chart.apply_synced_time_range(VisibleTimeRange {
                    from: start + TimeDelta::minutes(20),
                    to: start + TimeDelta::minutes(59),
                });
            }
            if ui.button("Future space").clicked() {
                chart.set_visible_logical_range(LogicalRange {
                    left: 80.0,
                    right: 139.0,
                });
            }
        });
    });
    if ready {
        if let Some(range) = chart.visible_logical_range() {
            ui.label(format!("Logical: {:.2} .. {:.2}", range.left, range.right));
        }
        if let Some(range) = chart.visible_time_range() {
            ui.label(format!(
                "UTC: {} .. {}",
                range.from.format("%F %T"),
                range.to.format("%F %T")
            ));
        }
        if let Some(x) = chart.index_to_x(30.0) {
            let time = chart.index_to_time(30.0).unwrap();
            let index = chart.time_to_index(time).unwrap();
            ui.label(format!(
                "Index {index:.0} = {} UTC; x = {x:.1} pt",
                time.format("%T")
            ));
        }
    } else {
        ui.label("Waiting for the first plot layout.");
    }
}
