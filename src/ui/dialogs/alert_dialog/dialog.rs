//! Alert creation dialog - modal popup for creating alerts.

use egui::{Context, Ui, Vec2};

use super::actions::{AlertData, AlertDialogAction};
use super::config::AlertDialogConfig;
use super::tabs::AlertTab;
use crate::ext::UiExt;
use crate::ui_kit::FormGrid;
use crate::ui_kit::dialog::{DialogFrame, dialog_header};
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
                multiplier: "2.0".to_string(),
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

        DialogFrame::new(
            "Create Alert",
            Vec2::new(self.config.width, self.config.height),
        )
        .show(ctx, |ui| {
            action = self.render_contents(ui);
        });

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
        ui.space_sm();
        ui.separator();

        // Content area
        egui::ScrollArea::vertical()
            .max_height(self.config.height - 150.0)
            .show(ui, |ui| {
                ui.space_lg();
                match self.active_tab {
                    AlertTab::Price => self.render_price_form(ui),
                    AlertTab::Indicator => self.render_indicator_form(ui),
                    AlertTab::Volume => self.render_volume_form(ui),
                }
                ui.space_lg();
            });

        // Footer with buttons
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.space_lg();
            ui.horizontal(|ui| {
                ui.space_lg();

                // Cancel button
                if ui.button("Cancel").clicked() {
                    action = AlertDialogAction::Cancel;
                    self.is_open = false;
                }

                ui.right_aligned(|ui| {
                    ui.space_lg();

                    // Create button (primary)
                    if ui.primary_button("Create Alert").clicked()
                        && let Some(data) = self.collect_form_data()
                    {
                        action = AlertDialogAction::Create(data);
                        self.is_open = false;
                    }
                });
            });
            ui.separator();
        });

        action
    }

    fn render_price_form(&mut self, ui: &mut Ui) {
        FormGrid::new("price_alert_form")
            .spacing(self.config.row_spacing, self.config.row_spacing)
            .show(ui, |ui| {
                ui.label("Symbol:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.price_form.symbol)
                        .desired_width(self.config.input_width),
                );
                ui.end_row();

                ui.label("Price:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.price_form.price)
                        .desired_width(self.config.input_width),
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
                    self.config.input_width,
                );
                ui.end_row();

                ui.label("Message:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.price_form.message)
                        .desired_width(self.config.input_width)
                        .desired_rows(2),
                );
                ui.end_row();

                ui.label("Options:");
                ui.checkbox(&mut self.price_form.repeating, "Only trigger once");
                ui.end_row();
            });
    }

    fn render_indicator_form(&mut self, ui: &mut Ui) {
        FormGrid::new("indicator_alert_form")
            .spacing(self.config.row_spacing, self.config.row_spacing)
            .show(ui, |ui| {
                ui.label("Symbol:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.indicator_form.symbol)
                        .desired_width(self.config.input_width),
                );
                ui.end_row();

                ui.label("Indicator A:");
                ui.combo_str_select_width(
                    "indicator_a",
                    &mut self.indicator_form.indicator_a,
                    &["SMA(20)", "EMA(20)", "RSI(14)", "MACD", "Volume"],
                    self.config.input_width,
                );
                ui.end_row();

                ui.label("Compare to:");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.indicator_form.use_threshold, false, "Indicator");
                    ui.radio_value(&mut self.indicator_form.use_threshold, true, "Value");
                });
                ui.end_row();

                if self.indicator_form.use_threshold {
                    ui.label("Value:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.indicator_form.threshold)
                            .desired_width(self.config.input_width),
                    );
                    ui.end_row();
                } else {
                    ui.label("Indicator B:");
                    ui.combo_str_select_width(
                        "indicator_b",
                        &mut self.indicator_form.indicator_b,
                        &["SMA(50)", "EMA(50)", "Price", "Volume MA"],
                        self.config.input_width,
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
                    self.config.input_width,
                );
                ui.end_row();

                ui.label("Message:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.indicator_form.message)
                        .desired_width(self.config.input_width)
                        .desired_rows(2),
                );
                ui.end_row();

                ui.label("Options:");
                ui.checkbox(&mut self.indicator_form.repeating, "Only trigger once");
                ui.end_row();
            });
    }

    fn render_volume_form(&mut self, ui: &mut Ui) {
        FormGrid::new("volume_alert_form")
            .spacing(self.config.row_spacing, self.config.row_spacing)
            .show(ui, |ui| {
                ui.label("Symbol:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.volume_form.symbol)
                        .desired_width(self.config.input_width),
                );
                ui.end_row();

                ui.label("Multiplier:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.volume_form.multiplier)
                        .desired_width(self.config.input_width),
                );
                ui.end_row();

                ui.label("Lookback:");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.volume_form.lookback)
                            .desired_width(60.0),
                    );
                    ui.label("bars");
                });
                ui.end_row();

                ui.label("Message:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.volume_form.message)
                        .desired_width(self.config.input_width)
                        .desired_rows(2),
                );
                ui.end_row();
            });
    }

    fn collect_form_data(&self) -> Option<AlertData> {
        match self.active_tab {
            AlertTab::Price => {
                let price = self.price_form.price.parse().ok()?;
                Some(AlertData::Price {
                    symbol: self.price_form.symbol.clone(),
                    price,
                    condition: self.price_form.condition.clone(),
                    message: self.price_form.message.clone(),
                    repeating: self.price_form.repeating,
                })
            }
            AlertTab::Indicator => {
                let threshold = if self.indicator_form.use_threshold {
                    self.indicator_form.threshold.parse().ok()
                } else {
                    None
                };
                let indicator_b = if !self.indicator_form.use_threshold
                    && !self.indicator_form.indicator_b.is_empty()
                {
                    Some(self.indicator_form.indicator_b.clone())
                } else {
                    None
                };
                Some(AlertData::Indicator {
                    symbol: self.indicator_form.symbol.clone(),
                    indicator_a: self.indicator_form.indicator_a.clone(),
                    indicator_b,
                    threshold,
                    condition: self.indicator_form.condition.clone(),
                    message: self.indicator_form.message.clone(),
                    repeating: self.indicator_form.repeating,
                })
            }
            AlertTab::Volume => {
                let multiplier = self.volume_form.multiplier.parse().ok()?;
                let lookback = self.volume_form.lookback.parse().ok()?;
                Some(AlertData::Volume {
                    symbol: self.volume_form.symbol.clone(),
                    multiplier,
                    lookback,
                    message: self.volume_form.message.clone(),
                })
            }
        }
    }
}
