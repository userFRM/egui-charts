//! Alerts panel UI
//!
//! Widget for managing and displaying price, indicator, volume, and event alerts.
use crate::ext::HasDesignTokens;
use crate::ext::UiExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::stubs::{
    AlertCondition, AlertManager, AlertStatus, EventAlert, IndicatorAlert, IndicatorCondition,
    PriceAlert, VolumeAlert,
};
use crate::ui_kit::{Card, EmptyState, PanelHeader};
use egui::{Color32, Response, RichText, Ui};

/// Configuration for the alerts panel
#[derive(Clone, Debug)]
pub struct AlertsPanelConfig {
    /// Show price alerts section
    pub show_price_alerts: bool,
    /// Show indicator alerts section
    pub show_indicator_alerts: bool,
    /// Show volume alerts section
    pub show_volume_alerts: bool,
    /// Show event alerts section
    pub show_event_alerts: bool,
    /// Max height of the panel
    pub max_height: f32,
    /// Colors
    pub active_color: Color32,
    pub triggered_color: Color32,
    pub disabled_color: Color32,
    pub expired_color: Color32,
}

impl Default for AlertsPanelConfig {
    fn default() -> Self {
        Self {
            show_price_alerts: true,
            show_indicator_alerts: true,
            show_volume_alerts: true,
            show_event_alerts: true,
            max_height: 400.0,
            active_color: DESIGN_TOKENS.semantic.extended.success,
            triggered_color: DESIGN_TOKENS.semantic.extended.warning,
            disabled_color: DESIGN_TOKENS.semantic.extended.disabled,
            expired_color: DESIGN_TOKENS.semantic.extended.error,
        }
    }
}

/// Response from alert panel interactions
#[derive(Debug, Clone, Copy)]
pub enum AlertPanelAction {
    /// No action
    None,
    /// Add new price alert
    AddPriceAlert,
    /// Add new indicator alert
    AddIndicatorAlert,
    /// Add new volume alert
    AddVolumeAlert,
    /// Add new event alert
    AddEventAlert,
    /// Remove alert by ID and type
    RemoveAlert { id: usize, alert_type: AlertType },
    /// Toggle alert active/disabled
    ToggleAlert { id: usize, alert_type: AlertType },
    /// Edit alert
    EditAlert { id: usize, alert_type: AlertType },
}

/// Type of alert for identification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertType {
    Price,
    Indicator,
    Volume,
    Event,
}

/// Reusable widget for rendering a single alert row
struct AlertRowWidget {
    status_color: Color32,
    description: String,
    subtitle: Option<String>,
    subtitle_color: Color32,
    alert_id: usize,
    alert_type: AlertType,
    show_toggle: bool,
}

impl AlertRowWidget {
    fn show(self, ui: &mut Ui) -> AlertPanelAction {
        let mut action = AlertPanelAction::None;

        Card::new().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("*").color(self.status_color));
                self.render_content(ui);
                action = self.render_actions(ui);
            });
        });

        action
    }

    fn render_content(&self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.strong_label(&self.description);
            if let Some(ref subtitle) = self.subtitle {
                ui.label(RichText::new(subtitle).small().color(self.subtitle_color));
            }
        });
    }

    fn render_actions(&self, ui: &mut Ui) -> AlertPanelAction {
        let mut action = AlertPanelAction::None;

        ui.right_aligned(|ui| {
            if ui.small_button("X").clicked() {
                action = AlertPanelAction::RemoveAlert {
                    id: self.alert_id,
                    alert_type: self.alert_type,
                };
            }
            if self.show_toggle && ui.small_button("||").clicked() {
                action = AlertPanelAction::ToggleAlert {
                    id: self.alert_id,
                    alert_type: self.alert_type,
                };
            }
        });

        action
    }
}

/// Alerts panel widget
pub struct AlertsPanel {
    config: AlertsPanelConfig,
    /// Current editing state
    _edit_state: EditState,
    /// Add price alert dialog
    new_price_alert: Option<PriceAlertForm>,
    /// Add indicator alert dialog
    new_indicator_alert: Option<IndicatorAlertForm>,
    /// Add volume alert dialog
    new_volume_alert: Option<VolumeAlertForm>,
}

/// Form data for creating a new price alert
struct PriceAlertForm {
    symbol: String,
    price: String,
    condition: AlertCondition,
    message: String,
    repeating: bool,
}

impl Default for PriceAlertForm {
    fn default() -> Self {
        Self {
            symbol: String::new(),
            price: String::new(),
            condition: AlertCondition::CrossesAbove,
            message: String::new(),
            repeating: false,
        }
    }
}

/// Form data for creating a new indicator alert
struct IndicatorAlertForm {
    indicator_a: String,
    indicator_b: String,
    threshold: String,
    condition: IndicatorCondition,
    symbol: String,
    message: String,
    repeating: bool,
    use_threshold: bool,
}

impl Default for IndicatorAlertForm {
    fn default() -> Self {
        Self {
            indicator_a: String::new(),
            indicator_b: String::new(),
            threshold: String::new(),
            condition: IndicatorCondition::CrossAbove,
            symbol: String::new(),
            message: String::new(),
            repeating: false,
            use_threshold: false,
        }
    }
}

/// Form data for creating a new volume alert
#[derive(Default)]
struct VolumeAlertForm {
    symbol: String,
    multiplier: String,
    lookback: String,
    message: String,
}

#[derive(Default)]
struct EditState {
    _editing_id: Option<usize>,
    _editing_type: Option<AlertType>,
}

impl Default for AlertsPanel {
    fn default() -> Self {
        Self::new(AlertsPanelConfig::default())
    }
}

impl AlertsPanel {
    pub fn new(config: AlertsPanelConfig) -> Self {
        Self {
            config,
            _edit_state: EditState::default(),
            new_price_alert: None,
            new_indicator_alert: None,
            new_volume_alert: None,
        }
    }

    /// Set configuration
    pub fn with_config(mut self, config: AlertsPanelConfig) -> Self {
        self.config = config;
        self
    }

    /// Render the alerts panel
    pub fn show(&mut self, ui: &mut Ui, manager: &AlertManager) -> AlertPanelAction {
        let mut action = AlertPanelAction::None;

        egui::ScrollArea::vertical()
            .max_height(self.config.max_height)
            .show(ui, |ui| {
                // Header
                PanelHeader::new("Alerts").show(ui, |ui| {
                    let active_cnt = manager
                        .price_alerts()
                        .iter()
                        .filter(|a| a.status == AlertStatus::Active)
                        .count()
                        + manager
                            .indicator_alerts()
                            .iter()
                            .filter(|a| a.status == AlertStatus::Active)
                            .count()
                        + manager
                            .volume_alerts()
                            .iter()
                            .filter(|a| a.status == AlertStatus::Active)
                            .count();

                    ui.label(
                        RichText::new(format!("{} active", active_cnt))
                            .small()
                            .color(self.config.active_color),
                    );
                });

                // Price Alerts Section
                if self.config.show_price_alerts {
                    action = self.render_price_alerts_section(ui, manager, action);
                }

                // Indicator Alerts Section
                if self.config.show_indicator_alerts {
                    action = self.render_indicator_alerts_section(ui, manager, action);
                }

                // Volume Alerts Section
                if self.config.show_volume_alerts {
                    action = self.render_volume_alerts_section(ui, manager, action);
                }

                // Event Alerts Section
                if self.config.show_event_alerts {
                    action = self.render_event_alerts_section(ui, manager, action);
                }
            });

        action
    }

    fn render_price_alerts_section(
        &mut self,
        ui: &mut Ui,
        manager: &AlertManager,
        mut action: AlertPanelAction,
    ) -> AlertPanelAction {
        egui::CollapsingHeader::new("Price Alerts")
            .default_open(true)
            .show(ui, |ui| {
                // Add button
                if ui.button("+ Add Price Alert").clicked() {
                    self.new_price_alert = Some(PriceAlertForm::default());
                }

                // Render add form if active
                if let Some(form) = &mut self.new_price_alert {
                    ui.space_sm();
                    ui.group(|ui| {
                        ui.label("New Price Alert");

                        ui.horizontal(|ui| {
                            ui.label("Symbol:");
                            ui.text_edit_singleline(&mut form.symbol);
                        });

                        ui.horizontal(|ui| {
                            ui.label("Price:");
                            ui.text_edit_singleline(&mut form.price);
                        });

                        ui.horizontal(|ui| {
                            ui.label("Condition:");
                            ui.combo_select(
                                "price_condition",
                                &mut form.condition,
                                [
                                    AlertCondition::CrossesAbove,
                                    AlertCondition::CrossesBelow,
                                    AlertCondition::Above,
                                    AlertCondition::Below,
                                ],
                                |v| v.as_str().to_string(),
                            );
                        });

                        ui.horizontal(|ui| {
                            ui.label("Message:");
                            ui.text_edit_singleline(&mut form.message);
                        });

                        ui.checkbox(&mut form.repeating, "Repeating");
                    });

                    let mut should_create = false;
                    let mut should_cancel = false;
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            should_create = true;
                        }
                        if ui.button("Cancel").clicked() {
                            should_cancel = true;
                        }
                    });
                    if should_create {
                        action = AlertPanelAction::AddPriceAlert;
                        self.new_price_alert = None;
                    } else if should_cancel {
                        self.new_price_alert = None;
                    }
                }

                ui.space_sm();

                // List existing alerts
                for alert in manager.price_alerts() {
                    let result = self.render_price_alert(ui, alert);
                    if !matches!(result, AlertPanelAction::None) {
                        action = result;
                    }
                }

                if manager.price_alerts().is_empty() {
                    EmptyState::new("No price alerts").show(ui);
                }
            });

        action
    }

    fn render_price_alert(&self, ui: &mut Ui, alert: &PriceAlert) -> AlertPanelAction {
        let status_color = self.status_color(alert.status);
        let description = format!(
            "{}: {} {:.2}",
            alert.symbol,
            alert.condition.as_str(),
            alert.target_price
        );
        let weak_color = ui.visuals().weak_text_color();

        let action = AlertRowWidget {
            status_color,
            description,
            subtitle: alert.message.clone(),
            subtitle_color: weak_color,
            alert_id: alert.id,
            alert_type: AlertType::Price,
            show_toggle: true,
        }
        .show(ui);

        ui.space_xs();
        action
    }

    fn render_indicator_alerts_section(
        &mut self,
        ui: &mut Ui,
        manager: &AlertManager,
        mut action: AlertPanelAction,
    ) -> AlertPanelAction {
        egui::CollapsingHeader::new("Indicator Alerts")
            .default_open(true)
            .show(ui, |ui| {
                if ui.button("+ Add Indicator Alert").clicked() {
                    self.new_indicator_alert = Some(IndicatorAlertForm::default());
                }

                // Render add form if active
                if let Some(form) = &mut self.new_indicator_alert {
                    ui.space_sm();
                    ui.group(|ui| {
                        ui.label("New Indicator Alert");

                        ui.horizontal(|ui| {
                            ui.label("Indicator A:");
                            ui.text_edit_singleline(&mut form.indicator_a);
                        });

                        ui.checkbox(&mut form.use_threshold, "Use Threshold (vs Crossover)");

                        if form.use_threshold {
                            ui.horizontal(|ui| {
                                ui.label("Threshold:");
                                ui.text_edit_singleline(&mut form.threshold);
                            });
                        } else {
                            ui.horizontal(|ui| {
                                ui.label("Indicator B:");
                                ui.text_edit_singleline(&mut form.indicator_b);
                            });
                        }

                        ui.horizontal(|ui| {
                            ui.label("Symbol:");
                            ui.text_edit_singleline(&mut form.symbol);
                        });
                    });

                    let mut should_create = false;
                    let mut should_cancel = false;
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            should_create = true;
                        }
                        if ui.button("Cancel").clicked() {
                            should_cancel = true;
                        }
                    });
                    if should_create {
                        action = AlertPanelAction::AddIndicatorAlert;
                        self.new_indicator_alert = None;
                    } else if should_cancel {
                        self.new_indicator_alert = None;
                    }
                }

                ui.space_sm();

                for alert in manager.indicator_alerts() {
                    let result = self.render_indicator_alert(ui, alert);
                    if !matches!(result, AlertPanelAction::None) {
                        action = result;
                    }
                }

                if manager.indicator_alerts().is_empty() {
                    EmptyState::new("No indicator alerts").show(ui);
                }
            });

        action
    }

    fn render_indicator_alert(&self, ui: &mut Ui, alert: &IndicatorAlert) -> AlertPanelAction {
        let status_color = self.status_color(alert.status);
        let description = Self::format_indicator_description(alert);
        let symbol_text = alert.symbol.clone();
        let weak_color = ui.visuals().weak_text_color();

        let action = AlertRowWidget {
            status_color,
            description,
            subtitle: symbol_text,
            subtitle_color: weak_color,
            alert_id: alert.id,
            alert_type: AlertType::Indicator,
            show_toggle: false,
        }
        .show(ui);

        ui.space_xs();
        action
    }

    fn format_indicator_description(alert: &IndicatorAlert) -> String {
        if let Some(ref ind_b) = alert.indicator_b {
            format!(
                "{} {} {}",
                alert.indicator_a,
                alert.condition.as_str(),
                ind_b
            )
        } else if let Some(thresh) = alert.threshold {
            format!(
                "{} {} {:.2}",
                alert.indicator_a,
                alert.condition.as_str(),
                thresh
            )
        } else {
            alert.indicator_a.clone()
        }
    }

    fn render_volume_alerts_section(
        &mut self,
        ui: &mut Ui,
        manager: &AlertManager,
        mut action: AlertPanelAction,
    ) -> AlertPanelAction {
        egui::CollapsingHeader::new("Volume Alerts")
            .default_open(true)
            .show(ui, |ui| {
                if ui.button("+ Add Volume Alert").clicked() {
                    self.new_volume_alert = Some(VolumeAlertForm {
                        multiplier: "2.0".to_string(),
                        lookback: "20".to_string(),
                        ..Default::default()
                    });
                }

                // Render add form if active
                if let Some(form) = &mut self.new_volume_alert {
                    ui.space_sm();
                    ui.group(|ui| {
                        ui.label("New Volume Spike Alert");

                        ui.horizontal(|ui| {
                            ui.label("Symbol:");
                            ui.text_edit_singleline(&mut form.symbol);
                        });

                        ui.horizontal(|ui| {
                            ui.label("Spike Multiplier:");
                            ui.text_edit_singleline(&mut form.multiplier);
                        });

                        ui.horizontal(|ui| {
                            ui.label("Lookback Periods:");
                            ui.text_edit_singleline(&mut form.lookback);
                        });
                    });

                    let mut should_create = false;
                    let mut should_cancel = false;
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            should_create = true;
                        }
                        if ui.button("Cancel").clicked() {
                            should_cancel = true;
                        }
                    });
                    if should_create {
                        action = AlertPanelAction::AddVolumeAlert;
                        self.new_volume_alert = None;
                    } else if should_cancel {
                        self.new_volume_alert = None;
                    }
                }

                ui.space_sm();

                for alert in manager.volume_alerts() {
                    let result = self.render_volume_alert(ui, alert);
                    if !matches!(result, AlertPanelAction::None) {
                        action = result;
                    }
                }

                if manager.volume_alerts().is_empty() {
                    EmptyState::new("No volume alerts").show(ui);
                }
            });

        action
    }

    fn render_volume_alert(&self, ui: &mut Ui, alert: &VolumeAlert) -> AlertPanelAction {
        let status_color = self.status_color(alert.status);
        let description = format!(
            "{}: Volume > {:.1}x avg ({})",
            alert.symbol, alert.spike_multiplier, alert.lookback_periods
        );
        let weak_color = ui.visuals().weak_text_color();

        let action = AlertRowWidget {
            status_color,
            description,
            subtitle: None,
            subtitle_color: weak_color,
            alert_id: alert.id,
            alert_type: AlertType::Volume,
            show_toggle: false,
        }
        .show(ui);

        ui.space_xs();
        action
    }

    fn render_event_alerts_section(
        &mut self,
        ui: &mut Ui,
        manager: &AlertManager,
        mut action: AlertPanelAction,
    ) -> AlertPanelAction {
        egui::CollapsingHeader::new("Event Alerts")
            .default_open(false)
            .show(ui, |ui| {
                if ui.button("+ Add Event Alert").clicked() {
                    action = AlertPanelAction::AddEventAlert;
                }

                ui.space_sm();

                for alert in manager.event_alerts() {
                    let result = self.render_event_alert(ui, alert);
                    if !matches!(result, AlertPanelAction::None) {
                        action = result;
                    }
                }

                if manager.event_alerts().is_empty() {
                    EmptyState::new("No event alerts").show(ui);
                }
            });

        action
    }

    fn render_event_alert(&self, ui: &mut Ui, alert: &EventAlert) -> AlertPanelAction {
        let status_color = self.status_color(alert.status);
        let description = format!(
            "{}: {} min before ({})",
            alert.event_type, alert.minutes_before, alert.min_impact
        );
        let weak_color = ui.visuals().weak_text_color();

        let action = AlertRowWidget {
            status_color,
            description,
            subtitle: None,
            subtitle_color: weak_color,
            alert_id: alert.id,
            alert_type: AlertType::Event,
            show_toggle: false,
        }
        .show(ui);

        ui.space_xs();
        action
    }

    fn status_color(&self, status: AlertStatus) -> Color32 {
        match status {
            AlertStatus::Active => self.config.active_color,
            AlertStatus::Triggered => self.config.triggered_color,
            AlertStatus::Disabled => self.config.disabled_color,
            AlertStatus::Expired => self.config.expired_color,
        }
    }

    /// Get the form data for creating a new price alert
    pub fn get_price_alert_form(&self) -> Option<(&str, f64, AlertCondition, &str, bool)> {
        self.new_price_alert.as_ref().and_then(|form| {
            form.price.parse::<f64>().ok().map(|price| {
                (
                    form.symbol.as_str(),
                    price,
                    form.condition,
                    form.message.as_str(),
                    form.repeating,
                )
            })
        })
    }

    /// Get form data for indicator alert
    pub fn get_indicator_alert_form(&self) -> Option<IndicatorAlertFormData> {
        self.new_indicator_alert
            .as_ref()
            .map(|form| IndicatorAlertFormData {
                indicator_a: form.indicator_a.clone(),
                indicator_b: if form.use_threshold {
                    None
                } else {
                    Some(form.indicator_b.clone())
                },
                threshold: if form.use_threshold {
                    form.threshold.parse().ok()
                } else {
                    None
                },
                condition: form.condition,
                symbol: if form.symbol.is_empty() {
                    None
                } else {
                    Some(form.symbol.clone())
                },
                message: if form.message.is_empty() {
                    None
                } else {
                    Some(form.message.clone())
                },
                repeating: form.repeating,
            })
    }

    /// Get form data for volume alert
    pub fn get_volume_alert_form(&self) -> Option<VolumeAlertFormData> {
        self.new_volume_alert.as_ref().and_then(|form| {
            let multiplier = form.multiplier.parse().ok()?;
            let lookback = form.lookback.parse().ok()?;
            Some(VolumeAlertFormData {
                symbol: form.symbol.clone(),
                multiplier,
                lookback,
                message: if form.message.is_empty() {
                    None
                } else {
                    Some(form.message.clone())
                },
            })
        })
    }
}

/// Exported form data for indicator alerts
#[derive(Debug, Clone)]
pub struct IndicatorAlertFormData {
    pub indicator_a: String,
    pub indicator_b: Option<String>,
    pub threshold: Option<f64>,
    pub condition: IndicatorCondition,
    pub symbol: Option<String>,
    pub message: Option<String>,
    pub repeating: bool,
}

/// Exported form data for volume alerts
#[derive(Debug, Clone)]
pub struct VolumeAlertFormData {
    pub symbol: String,
    pub multiplier: f64,
    pub lookback: usize,
    pub message: Option<String>,
}

/// Compact inline alert status display
pub struct AlertStatusWidget;

impl AlertStatusWidget {
    /// Show a compact alert status indicator
    pub fn show(ui: &mut Ui, manager: &AlertManager) -> Response {
        let active_cnt = manager
            .price_alerts()
            .iter()
            .filter(|a| a.status == AlertStatus::Active)
            .count()
            + manager
                .indicator_alerts()
                .iter()
                .filter(|a| a.status == AlertStatus::Active)
                .count()
            + manager
                .volume_alerts()
                .iter()
                .filter(|a| a.status == AlertStatus::Active)
                .count();

        let triggered_cnt = manager
            .price_alerts()
            .iter()
            .filter(|a| a.status == AlertStatus::Triggered)
            .count()
            + manager
                .indicator_alerts()
                .iter()
                .filter(|a| a.status == AlertStatus::Triggered)
                .count();

        let color = if triggered_cnt > 0 {
            ui.warning_color()
        } else if active_cnt > 0 {
            ui.success_color()
        } else {
            ui.visuals().weak_text_color()
        };

        let text = format!("{} alerts", active_cnt);

        ui.label(RichText::new(text).color(color))
    }
}

/// Alert notification popup widget
pub struct AlertNotification {
    pub message: String,
    pub symbol: String,
    pub price: f64,
    pub condition: String,
    pub ts: chrono::DateTime<chrono::Utc>,
}

impl AlertNotification {
    pub fn new(
        symbol: impl Into<String>,
        price: f64,
        condition: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            symbol: symbol.into(),
            price,
            condition: condition.into(),
            ts: chrono::Utc::now(),
        }
    }

    pub fn show(&self, ui: &mut Ui) {
        use crate::styles::typography;
        egui::Frame::NONE
            .fill(ui.visuals().window_fill)
            .inner_margin(DESIGN_TOKENS.spacing.xl)
            .corner_radius(DESIGN_TOKENS.rounding.lg)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("!").size(typography::HERO));
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Alert Triggered")
                                .strong()
                                .color(ui.warning_color()),
                        );
                        ui.label(RichText::new(&self.symbol).strong().size(typography::XXL));
                        ui.label(format!("{} @ ${:.2}", self.condition, self.price));
                        if !self.message.is_empty() {
                            ui.label(
                                RichText::new(&self.message)
                                    .italics()
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    });
                });
            });
    }
}
