use std::sync::Arc;

use eframe::egui;
use egui_charts::{
    Chart, localization,
    model::TimezoneId,
    scales::{CustomPriceFormatter, TimezoneMode},
    theme::{self, Theme, ThemeExt},
    widget::SeriesId,
};

/// Change presentation while keeping the same synthetic UTC history.
#[derive(Default)]
pub struct Presentation {
    high_contrast: bool,
    monospace: bool,
    german: bool,
    price_units: bool,
    offset: bool,
    host_dark: Option<bool>,
}

impl Presentation {
    /// Draw host controls before the chart; only apply settings when they change.
    pub fn show(&mut self, ui: &mut egui::Ui, chart: &mut Chart) {
        let dark = ui.visuals().dark_mode;
        let theme_changed = self.host_dark.replace(dark) != Some(dark);
        ui.horizontal_wrapped(|ui| {
            if ui
                .checkbox(&mut self.high_contrast, "High contrast")
                .changed()
                || theme_changed
            {
                chart.set_theme_override(self.high_contrast.then(Theme::high_contrast));
            }
            if ui.checkbox(&mut self.monospace, "Monospace font").changed() || theme_changed {
                if self.monospace {
                    let mut fonts = egui::FontDefinitions::default();
                    fonts.families.insert(
                        egui::FontFamily::Proportional,
                        fonts.families[&egui::FontFamily::Monospace].clone(),
                    );
                    ui.ctx().set_fonts(fonts);
                } else {
                    theme::apply_to_egui(ui.ctx(), &ui.active_theme());
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.checkbox(&mut self.german, "German UI lookup").changed() {
                if self.german {
                    localization::set_translator(ui.ctx(), |english| match english {
                        "Reset view" => Some("Ansicht zurücksetzen".into()),
                        _ => None,
                    });
                } else {
                    localization::clear_translator(ui.ctx());
                }
                chart.set_locale(self.german.then(|| "de-DE".into()));
            }
            if ui.checkbox(&mut self.price_units, "Price units").changed() {
                chart.set_price_formatter(self.price_units.then(|| {
                    Arc::new(CustomPriceFormatter::new(|price| format!("{price:.2} u")))
                        as Arc<dyn egui_charts::scales::PriceFormatter>
                }));
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.checkbox(&mut self.offset, "UTC +05:45").changed() {
                let mut options = chart.chart_options().clone();
                options.time_scale.timezone = if self.offset {
                    TimezoneMode::Timezone(TimezoneId::fixed_offset(5 * 3600 + 45 * 60).unwrap())
                } else {
                    TimezoneMode::Utc
                };
                chart.set_chart_options(options);
            }
            if ui
                .button(localization::text(ui.ctx(), "Reset view"))
                .clicked()
            {
                chart.reset_chart_view();
            }
        });
        let time = chart.bars(SeriesId::MAIN)[0].time;
        ui.label(format!("Stored UTC: {}", time.format("%F %T")));
        ui.label(format!("Display: {}", chart.format_time(time)));
    }
}
