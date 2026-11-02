//! Controls for the demo's chart readouts and series annotations.

use super::DemoApp;
use egui_charts::config::TooltipMode;

impl DemoApp {
    pub(super) fn show_readout_controls(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Readout", |ui| {
            let options = &mut self.chart.chart_options_mut().tooltip;
            ui.checkbox(&mut options.enabled, "Show readout");
            for mode in [
                TooltipMode::Floating,
                TooltipMode::Tracking,
                TooltipMode::Magnifier,
            ] {
                ui.radio_value(&mut options.mode, mode, mode.to_string());
            }
            ui.separator();
            ui.radio_value(&mut options.on_long_press, false, "Hover to inspect");
            ui.radio_value(&mut options.on_long_press, true, "Hold to inspect");
            ui.small(if options.on_long_press {
                "Hold the primary button over a bar. On touch, press and wait."
            } else {
                "Move the pointer over a bar. On touch, press and wait."
            });
            if options.mode == TooltipMode::Magnifier {
                ui.add(
                    egui::Slider::new(&mut options.magnifier_size, 60.0..=360.0)
                        .text("Lens diameter"),
                );
                ui.add(egui::Slider::new(&mut options.magnifier_zoom, 1.0..=8.0).text("Zoom"));
            } else {
                ui.checkbox(&mut options.show_time, "Time");
                ui.checkbox(&mut options.show_ohlc, "OHLC");
                ui.checkbox(&mut options.show_volume, "Volume");
                ui.checkbox(&mut options.show_change, "Change");
                ui.checkbox(&mut options.show_indicators, "Studies");
                ui.checkbox(&mut options.show_volume_details, "Held volume details");
            }
        });
    }
}
