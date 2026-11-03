use eframe::egui;
use egui_charts::{
    Chart,
    model::{Bar, ChartType, SeriesScale},
    widget::{PaneId, SeriesId, SeriesOptions},
};

/// Move an added series while keeping an empty destination available.
pub struct Panes {
    series: SeriesId,
    pane: PaneId,
}

impl Panes {
    /// Put host-supplied sample volume in a pane below the price series.
    pub fn new(chart: &mut Chart) -> Self {
        chart.config_mut().show_volume = false;
        let bars = chart
            .bars(SeriesId::MAIN)
            .iter()
            .map(|bar| Bar::from_value(bar.time, bar.volume))
            .collect();
        let series = chart.add_series(SeriesOptions {
            name: "SAMPLE VOLUME".into(),
            chart_type: ChartType::Columns,
            columns_base: Some(0.0),
            scale: SeriesScale::Separate(1),
            ..Default::default()
        });
        chart.set_data(series, bars);
        let pane = chart.add_pane();
        chart.set_preserve_empty_pane(pane, true);
        chart.move_series_to_pane(series, pane);
        chart.set_pane_stretch_factor(PaneId::MAIN, 3.0);
        chart.set_pane_stretch_factor(pane, 1.0);
        Self { series, pane }
    }

    /// Try placement, fixed height and proportional sizing on one shared time axis.
    pub fn show(&mut self, ui: &mut egui::Ui, chart: &mut Chart) {
        ui.horizontal_wrapped(|ui| {
            let in_main = chart.series_pane(self.series) == Some(PaneId::MAIN);
            if ui
                .button(if in_main {
                    "Move below"
                } else {
                    "Move to main"
                })
                .clicked()
            {
                chart.move_series_to_pane(
                    self.series,
                    if in_main { self.pane } else { PaneId::MAIN },
                );
            }
            if ui.button("Lower pane: 140 pt").clicked() {
                chart.set_pane_height(self.pane, 140.0);
            }
            if ui.button("Restore 3:1").clicked() {
                chart.set_pane_stretch_factor(PaneId::MAIN, 3.0);
                chart.set_pane_stretch_factor(self.pane, 1.0);
            }
            ui.checkbox(&mut chart.pane_options_mut().resizable, "Resizable");
        });
        ui.label("Drag the separator; double-click it to reset proportions.");
        if let Some(height) = chart.pane_height(self.pane) {
            ui.label(format!("Lower pane last drawn: {height:.0} pt"));
        }
    }
}
