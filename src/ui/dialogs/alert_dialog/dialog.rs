//! Alert creation dialog - modal popup for creating alerts.

use egui::{Context, Ui};

use super::actions::{AlertData, AlertDialogAction};
use super::config::AlertDialogConfig;
use super::fields;
use super::tabs::AlertTab;
use crate::ext::UiExt;
use crate::ui_kit::FormGrid;
use crate::ui_kit::dialog::{DialogFooter, dialog_header};
use crate::ui_kit::modal::Modal;
use crate::ui_kit::status_message::{MessageKind, status_message};
use crate::ui_kit::tab_bar::TabBar;

/// Form data for price alerts
#[derive(Clone, Debug, Default)]
pub struct PriceAlertFormData {
    pub symbol: String,
    pub price: String,
    pub condition: String,
    pub message: String,
    pub repeating: bool,
}

/// Form data for indicator alerts
#[derive(Clone, Debug, Default)]
pub struct IndicatorAlertFormData {
    pub symbol: String,
    pub indicator_a: String,
    pub indicator_b: String,
    pub use_threshold: bool,
    pub threshold: String,
    pub condition: String,
    pub message: String,
    pub repeating: bool,
}

/// Form data for volume alerts
#[derive(Clone, Debug, Default)]
pub struct VolumeAlertFormData {
    pub symbol: String,
    pub multiplier: String,
    pub lookback: String,
    pub message: String,
}

/// Alert creation dialog
pub struct AlertDialog {
    pub config: AlertDialogConfig,
    pub is_open: bool,
    pub active_tab: AlertTab,
    pub price_form: PriceAlertFormData,
    pub indicator_form: IndicatorAlertFormData,
    pub volume_form: VolumeAlertFormData,
}

impl Default for AlertDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertDialog {
    pub fn new() -> Self {
        Self {
            config: AlertDialogConfig::default(),
            is_open: false,
            active_tab: AlertTab::Price,
            price_form: PriceAlertFormData {
                condition: "Crossing".to_string(),
                ..Default::default()
            },
            indicator_form: IndicatorAlertFormData {
                condition: "Greater Than".to_string(),
                ..Default::default()
            },
            volume_form: VolumeAlertFormData {
                multiplier: "2".to_string(),
                lookback: "20".to_string(),
                ..Default::default()
            },
        }
    }

    /// Open the dialog with default symbol
    pub fn open(&mut self, symbol: String) {
        self.is_open = true;
        self.price_form.symbol = symbol.clone();
        self.indicator_form.symbol = symbol.clone();
        self.volume_form.symbol = symbol;
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Sync dialog open/close state from an external flag.
    ///
    /// Opening requires symbol context, so only close is synced here.
    pub fn sync_open_state(&mut self, should_be_open: bool) {
        if !should_be_open && self.is_open {
            self.close();
        }
    }

    /// Show the dialog
    pub fn show(&mut self, ctx: &Context) -> AlertDialogAction {
        if !self.is_open {
            return AlertDialogAction::None;
        }

        let mut action = AlertDialogAction::None;

        let mut open = true;
        Modal::new("create_alert", "")
            .width(self.config.width + 32.0)
            .height(self.config.height + 32.0)
            .show(ctx, &mut open, |ui| {
                action = self.render_contents(ui);
            });
        if !open {
            self.is_open = false;
            action = AlertDialogAction::Cancel;
        }

        action
    }

    fn render_contents(&mut self, ui: &mut Ui) -> AlertDialogAction {
        let mut action = AlertDialogAction::None;

        // Title bar
        if dialog_header(ui, "Create Alert") {
            action = AlertDialogAction::Cancel;
            self.is_open = false;
        }
        ui.separator();

        // Tabs
        ui.space_sm();
        TabBar::new(&AlertTab::all(), &mut self.active_tab).show(ui);

        // Content area
        egui::ScrollArea::vertical()
            .max_height((ui.available_height() - 80.0).max(0.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.space_lg();
                match self.active_tab {
                    AlertTab::Price => self.render_price_form(ui),
                    AlertTab::Indicator => self.render_indicator_form(ui),
                    AlertTab::Volume => self.render_volume_form(ui),
                }
                ui.space_lg();
            });

        let form = self.collect_form_data();
        let (create, cancel) = DialogFooter::new("Create")
            .secondary("Cancel")
            .primary_enabled(form.is_ok())
            .show_with_left(ui, |ui| {
                if let Err(error) = &form {
                    status_message(ui, MessageKind::Error, error);
                }
            });
        if cancel {
            action = AlertDialogAction::Cancel;
            self.is_open = false;
        } else if create && let Ok(data) = form {
            action = AlertDialogAction::Create(data);
            self.is_open = false;
        }

        action
    }

    fn input_width(&self, ui: &Ui) -> f32 {
        let label_width = ui
            .painter()
            .layout_no_wrap(
                match self.active_tab {
                    AlertTab::Price => "Condition:",
                    AlertTab::Indicator => "Compare to:",
                    AlertTab::Volume => "Multiplier:",
                }
                .into(),
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            )
            .size()
            .x;
        (ui.available_width() - label_width - self.config.row_spacing).max(0.0)
    }

    fn render_price_form(&mut self, ui: &mut Ui) {
        let input_width = self.input_width(ui);
        FormGrid::new("price_alert_form")
            .spacing(self.config.row_spacing, self.config.row_spacing)
            .show(ui, |ui| {
                ui.label("Symbol:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.price_form.symbol)
                        .desired_width(input_width),
                );
                ui.end_row();

                ui.label("Price:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.price_form.price)
                        .desired_width(input_width),
                );
                ui.end_row();

                ui.label("Condition:");
                ui.combo_str_select_width(
                    "price_condition",
                    &mut self.price_form.condition,
                    &[
                        "Crossing",
                        "Crossing Up",
                        "Crossing Down",
                        "Greater Than",
                        "Less Than",
                    ],
                    input_width,
                );
                ui.end_row();

                ui.label("Message:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.price_form.message)
                        .desired_width(input_width)
                        .desired_rows(2),
                );
                ui.end_row();

                ui.label("Options:");
                ui.checkbox(&mut self.price_form.repeating, "Only trigger once");
                ui.end_row();
            });
    }

    fn render_indicator_form(&mut self, ui: &mut Ui) {
        let input_width = self.input_width(ui);
        FormGrid::new("indicator_alert_form")
            .spacing(self.config.row_spacing, self.config.row_spacing)
            .show(ui, |ui| {
                ui.label("Symbol:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.indicator_form.symbol)
                        .desired_width(input_width),
                );
                ui.end_row();

                ui.label("Indicator A:");
                ui.combo_str_select_width(
                    "indicator_a",
                    &mut self.indicator_form.indicator_a,
                    &["SMA(20)", "EMA(20)", "RSI(14)", "MACD", "Volume"],
                    input_width,
                );
                ui.end_row();

                ui.label("Compare to:");
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.radio_value(&mut self.indicator_form.use_threshold, false, "Indicator");
                    ui.radio_value(&mut self.indicator_form.use_threshold, true, "Value");
                });
                ui.end_row();

                if self.indicator_form.use_threshold {
                    ui.label("Value:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.indicator_form.threshold)
                            .desired_width(input_width),
                    );
                    ui.end_row();
                } else {
                    ui.label("Indicator B:");
                    ui.combo_str_select_width(
                        "indicator_b",
                        &mut self.indicator_form.indicator_b,
                        &["SMA(50)", "EMA(50)", "Price", "Volume MA"],
                        input_width,
                    );
                    ui.end_row();
                }

                ui.label("Condition:");
                ui.combo_str_select_width(
                    "indicator_condition",
                    &mut self.indicator_form.condition,
                    &[
                        "Greater Than",
                        "Less Than",
                        "Crossing",
                        "Crossing Up",
                        "Crossing Down",
                    ],
                    input_width,
                );
                ui.end_row();

                ui.label("Message:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.indicator_form.message)
                        .desired_width(input_width)
                        .desired_rows(2),
                );
                ui.end_row();

                ui.label("Options:");
                ui.checkbox(&mut self.indicator_form.repeating, "Only trigger once");
                ui.end_row();
            });
    }

    fn render_volume_form(&mut self, ui: &mut Ui) {
        let input_width = self.input_width(ui);
        FormGrid::new("volume_alert_form")
            .spacing(self.config.row_spacing, self.config.row_spacing)
            .show(ui, |ui| {
                ui.label("Symbol:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.volume_form.symbol)
                        .desired_width(input_width),
                );
                ui.end_row();

                ui.label("Multiplier:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.volume_form.multiplier)
                        .desired_width(input_width),
                );
                ui.end_row();

                ui.label("Lookback:");
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let unit_width = ui
                        .painter()
                        .layout_no_wrap(
                            "bars".into(),
                            egui::TextStyle::Body.resolve(ui.style()),
                            ui.visuals().text_color(),
                        )
                        .size()
                        .x;
                    ui.add(
                        egui::TextEdit::singleline(&mut self.volume_form.lookback)
                            .desired_width((input_width - unit_width - 4.0).max(0.0)),
                    );
                    ui.label("bars");
                });
                ui.end_row();

                ui.label("Message:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.volume_form.message)
                        .desired_width(input_width)
                        .desired_rows(2),
                );
                ui.end_row();
            });
    }

    fn collect_form_data(&self) -> Result<AlertData, &'static str> {
        match self.active_tab {
            AlertTab::Price => {
                let price = fields::level(&self.price_form.price).ok_or("Enter a price")?;
                Ok(AlertData::Price {
                    symbol: self.price_form.symbol.clone(),
                    price,
                    condition: self.price_form.condition.clone(),
                    message: self.price_form.message.clone(),
                    repeating: self.price_form.repeating,
                })
            }
            AlertTab::Indicator => {
                let form = &self.indicator_form;
                let indicator_a =
                    fields::indicator(&form.indicator_a).ok_or("Choose Indicator A")?;
                let (indicator_b, threshold) = if form.use_threshold {
                    let threshold = fields::level(&form.threshold).ok_or("Enter a threshold")?;
                    (None, Some(threshold))
                } else {
                    let indicator_b =
                        fields::indicator(&form.indicator_b).ok_or("Choose Indicator B")?;
                    (Some(indicator_b), None)
                };
                Ok(AlertData::Indicator {
                    symbol: form.symbol.clone(),
                    indicator_a,
                    indicator_b,
                    threshold,
                    condition: form.condition.clone(),
                    message: form.message.clone(),
                    repeating: form.repeating,
                })
            }
            AlertTab::Volume => {
                let multiplier = fields::multiplier(&self.volume_form.multiplier)
                    .ok_or("Enter a multiplier above 0")?;
                let lookback = fields::lookback(&self.volume_form.lookback)
                    .ok_or("Enter a lookback of at least 1 bar")?;
                Ok(AlertData::Volume {
                    symbol: self.volume_form.symbol.clone(),
                    multiplier,
                    lookback,
                    message: self.volume_form.message.clone(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_controls_fill_the_dialog_width_on_every_tab() {
        for tab in AlertTab::all() {
            for width in [360.0, 560.0] {
                let ctx = Context::default();
                let mut dialog = AlertDialog::new();
                dialog.active_tab = tab;
                let mut form_width = 0.0;
                for _ in 0..3 {
                    ctx.run_ui(Default::default(), |ui| {
                        ui.set_width(width);
                        let form = ui.vertical(|ui| match tab {
                            AlertTab::Price => dialog.render_price_form(ui),
                            AlertTab::Indicator => dialog.render_indicator_form(ui),
                            AlertTab::Volume => dialog.render_volume_form(ui),
                        });
                        form_width = form.response.rect.width();
                    })
                    .drop_without_applying_deltas();
                }
                assert!(
                    (form_width - width).abs() < 16.0,
                    "{tab:?}: {form_width} at {width}"
                );
            }
        }
        for size in [egui::vec2(1440.0, 900.0), egui::vec2(600.0, 360.0)] {
            let ctx = Context::default();
            let mut dialog = AlertDialog::new();
            dialog.open("BTCUSD".into());
            dialog.active_tab = AlertTab::Indicator;
            for frame in 0..3 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| {
                        dialog.show(ui.ctx());
                    },
                );
                let visible = |label| {
                    output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text() == label
                    && shape.clip_rect.contains_rect(egui::Rect::from_min_size(text.pos, text.galley.size()))
                ))
                };
                let footer = visible("Create");
                let message = visible("Message:");
                output.drop_without_applying_deltas();
                if frame > 0 {
                    assert!(footer, "footer is reachable at {size:?}");
                    if size.y == 900.0 {
                        assert!(message, "the default height fits the tallest form");
                    }
                }
            }
        }
    }

    #[test]
    fn create_needs_a_valid_form() {
        use AlertTab::{Indicator, Price, Volume};
        type Edit = fn(&mut AlertDialog);
        const PRICE: Result<(), &str> = Err("Enter a price");
        const THRESHOLD: Result<(), &str> = Err("Enter a threshold");
        const INDICATOR_A: Result<(), &str> = Err("Choose Indicator A");
        const INDICATOR_B: Result<(), &str> = Err("Choose Indicator B");
        const MULTIPLIER: Result<(), &str> = Err("Enter a multiplier above 0");
        const LOOKBACK: Result<(), &str> = Err("Enter a lookback of at least 1 bar");
        let rows: [(AlertTab, Edit, Result<(), &str>); 14] = [
            (Price, |d| d.price_form.price = "NaN".into(), PRICE),
            (Price, |d| d.price_form.price = "inf".into(), PRICE),
            (Price, |d| d.price_form.price = "1e400".into(), PRICE),
            (Price, |d| d.price_form.price.clear(), PRICE),
            (Price, |d| d.price_form.price = "-12.5".into(), Ok(())),
            (
                Indicator,
                |d| d.indicator_form.threshold = "1,5".into(),
                THRESHOLD,
            ),
            (Indicator, |d| d.indicator_form.threshold.clear(), THRESHOLD),
            (
                Indicator,
                |d| d.indicator_form.indicator_a.clear(),
                INDICATOR_A,
            ),
            (
                Indicator,
                |d| d.indicator_form.use_threshold = false,
                INDICATOR_B,
            ),
            (
                Indicator,
                |d| {
                    d.indicator_form.use_threshold = false;
                    d.indicator_form.indicator_b = "SMA(50)".into();
                },
                Ok(()),
            ),
            (
                Volume,
                |d| d.volume_form.multiplier = "-2".into(),
                MULTIPLIER,
            ),
            (
                Volume,
                |d| d.volume_form.multiplier = "NaN".into(),
                MULTIPLIER,
            ),
            (Volume, |d| d.volume_form.lookback = "0".into(), LOOKBACK),
            (Volume, |_| {}, Ok(())),
        ];
        for (index, (tab, edit, expected)) in rows.into_iter().enumerate() {
            let mut dialog = AlertDialog::new();
            dialog.active_tab = tab;
            dialog.price_form.price = "100".into();
            dialog.indicator_form.use_threshold = true;
            dialog.indicator_form.threshold = "1".into();
            dialog.indicator_form.indicator_a = "SMA(20)".into();
            edit(&mut dialog);
            assert_eq!(
                dialog.collect_form_data().map(drop),
                expected,
                "row {index}"
            );
        }
    }
}
