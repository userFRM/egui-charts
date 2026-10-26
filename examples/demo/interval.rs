//! Keyboard interval entry for the demo host.

use eframe::egui;
use egui_charts::model::Timeframe;
use egui_charts::ui_kit::dialog::DialogFooter;
use egui_charts::ui_kit::modal::Modal;

impl super::DemoApp {
    pub(super) fn show_interval_entry(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.interval_entry.take() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        Modal::new("interval_entry", "Change interval")
            .width(320.0)
            .show(ctx, &mut open, |ui| {
                ui.label("Minutes, or an interval such as 30s, 2h, 1D, 10T, 100V, 10R or 100delta");
                let input_id = egui::Id::unique("interval_entry_input");
                if std::mem::take(&mut self.interval_focus) {
                    ui.memory_mut(|memory| memory.request_focus(input_id));
                }
                ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .id(input_id)
                        .desired_width(ui.available_width()),
                );
                apply = DialogFooter::new("Apply").show(ui).0
                    || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            });
        if apply {
            let value = if text.chars().all(|c| c.is_ascii_digit()) {
                format!("{text}min")
            } else {
                text.clone()
            };
            match value.parse::<Timeframe>() {
                Ok(timeframe) => {
                    self.user_action(|app| app.switch_timeframe(timeframe));
                    return;
                }
                Err(error) => self.notify(error),
            }
        }
        if open {
            self.interval_entry = Some(text);
        }
    }
}
