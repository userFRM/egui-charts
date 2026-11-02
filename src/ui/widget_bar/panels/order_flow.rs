//! Settings, feed-time counts and notifications for the order-flow detector service.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use egui::Ui;

use crate::localization as l10n;
use crate::services::{
    CountAlert, CountWindow, DetectionKind, Detector, OrderFlowDetectors, Threshold,
};
use crate::theme::ThemeExt;
use crate::ui_kit::PanelHeader;
use crate::ui_kit::notifications::{Toast, Toasts};

/// A panel connected to [`OrderFlowDetectors`], available with `ui` and `detectors`.
/// Call [`poll`](Self::poll) every frame even while hidden. The host feeds the service, applies
/// [`show`](Self::show)'s visibility changes to its chart, renders the shared toasts and plays
/// the sounds requested by `poll`.
pub struct OrderFlowPanel {
    service: Arc<OrderFlowDetectors>,
    visible: [bool; DetectionKind::ALL.len()],
}

impl OrderFlowPanel {
    /// Connect to the same detector service used as the chart's signal source.
    pub fn new(service: Arc<OrderFlowDetectors>) -> Self {
        Self {
            service,
            visible: [true; DetectionKind::ALL.len()],
        }
    }

    /// Follow a different chart's service while retaining the panel's visibility choices.
    pub fn set_service(&mut self, service: Arc<OrderFlowDetectors>) {
        self.service = service;
    }

    /// Drain live alerts once, append one toast per alert and keep at most five on screen.
    /// Returns each kind whose alert requested a sound, in delivery order; the host plays it.
    pub fn poll(&mut self, ctx: &egui::Context, toasts: &mut Toasts) -> Vec<DetectionKind> {
        let mut sounds = Vec::new();
        for alert in self.service.take_alerts() {
            let notification = alert.notification;
            let detail = match alert.count {
                Some((window, count)) => l10n::format_text(
                    ctx,
                    "{window}: {count}",
                    &[
                        ("window", &l10n::text(ctx, window.label())),
                        ("count", &count.to_string()),
                    ],
                ),
                None => l10n::format_text(
                    ctx,
                    "Detected at {time} UTC",
                    &[("time", notification.time.format("%H:%M:%S%.3f").to_string())],
                ),
            };
            let title = l10n::format_text(
                ctx,
                "{symbol}: {kind}",
                &[
                    ("symbol", notification.symbol.as_deref().unwrap_or_default()),
                    ("kind", &l10n::text(ctx, &notification.title)),
                ],
            );
            toasts.add(Toast::info(detail).with_title(title));
            if alert.sound {
                sounds.push(alert.kind);
            }
        }
        let extra = toasts.len().saturating_sub(5);
        toasts.toasts_mut().drain(..extra);
        sounds
    }

    /// Draw settings and counts for the current symbol at `now`, its current feed time.
    /// Changes take effect through `set_settings` on the next events. Return category visibility
    /// changes for the host to apply with [`Chart::set_signal_category_visible`](crate::widget::Chart::set_signal_category_visible).
    pub fn show(
        &mut self,
        ui: &mut Ui,
        symbol: &str,
        now: DateTime<Utc>,
    ) -> Vec<(DetectionKind, bool)> {
        PanelHeader::new(&l10n::text(ui.ctx(), "Order flow")).show(ui, |_| {});
        let colors = ui.active_theme().ui;
        ui.colored_label(colors.text_secondary, symbol);
        let mut settings = self.service.settings();
        let before = settings.clone();
        let visible_before = self.visible;
        egui::ScrollArea::vertical()
            .id_salt("order_flow_settings")
            .show(ui, |ui| {
                ui.push_id("big_trades", |ui| {
                    detector(ui, "Big trades", &mut settings.big_trades, |ui, rule| {
                        threshold(ui, "Size", &mut rule.threshold);
                        check(ui, "Body zones", &mut rule.body_zones);
                        check(ui, "Wick zones", &mut rule.wick_zones);
                    });
                });
                ui.push_id("stop_runs", |ui| {
                    detector(ui, "Stop runs", &mut settings.stop_runs, |ui, _| {
                        ui.label(l10n::text(
                            ui.ctx(),
                            "5 fills, 3 ticks, at most 20 ms apart",
                        ));
                    });
                    self.counts(
                        ui,
                        symbol,
                        DetectionKind::StopRun,
                        now,
                        &mut settings.stop_run_count_alert,
                    );
                });
                ui.push_id("stop_sweeps", |ui| {
                    detector(ui, "Stop sweeps", &mut settings.stop_sweeps, |ui, _| {
                        ui.label(l10n::text(
                            ui.ctx(),
                            "Closed bars: delta, imbalance, body and volume",
                        ));
                    });
                });
                ui.push_id("icebergs", |ui| {
                    detector(ui, "Icebergs", &mut settings.icebergs, |ui, rule| {
                        ui.horizontal_wrapped(|ui| {
                            let label = ui.label(l10n::text(ui.ctx(), "Minimum refills"));
                            ui.add(egui::DragValue::new(&mut rule.refills).range(1..=u32::MAX))
                                .labelled_by(label.id);
                        });
                    });
                    self.counts(
                        ui,
                        symbol,
                        DetectionKind::Iceberg,
                        now,
                        &mut settings.iceberg_count_alert,
                    );
                });
                ui.push_id("absorption", |ui| {
                    detector(ui, "Absorption", &mut settings.absorption, |ui, rule| {
                        threshold(ui, "Size", &mut rule.threshold);
                    });
                });
                ui.push_id("liquidity", |ui| {
                    detector(
                        ui,
                        "Resting liquidity",
                        &mut settings.liquidity,
                        |ui, rule| {
                            threshold(ui, "Size", &mut rule.threshold);
                            threshold(ui, "Fresh size", &mut rule.fresh_threshold);
                        },
                    );
                });
                ui.separator();
                ui.collapsing(l10n::text(ui.ctx(), "Chart visibility"), |ui| {
                    for (index, kind) in DetectionKind::ALL.into_iter().enumerate() {
                        check(ui, kind.category(), &mut self.visible[index]);
                    }
                });
            });
        if settings != before {
            self.service.set_settings(settings);
        }
        DetectionKind::ALL
            .into_iter()
            .zip(self.visible)
            .zip(visible_before)
            .filter_map(|((kind, visible), before)| (visible != before).then_some((kind, visible)))
            .collect()
    }

    fn counts(
        &self,
        ui: &mut Ui,
        symbol: &str,
        kind: DetectionKind,
        now: DateTime<Utc>,
        alert: &mut Option<CountAlert>,
    ) {
        let counts = self.service.counts(symbol, kind, now);
        for window in [
            CountWindow::Total,
            CountWindow::FiveMinutes,
            CountWindow::ThirtySeconds,
        ] {
            ui.label(l10n::format_text(
                ui.ctx(),
                "{window}: {count}",
                &[
                    ("window", &l10n::text(ui.ctx(), window.label())),
                    ("count", &window.count(counts).to_string()),
                ],
            ));
        }
        let mut enabled = alert.is_some();
        if check(ui, "Count alert", &mut enabled) {
            *alert = enabled.then(CountAlert::default);
        }
        if let Some(alert) = alert {
            ui.horizontal_wrapped(|ui| {
                let label = ui.label(l10n::text(ui.ctx(), "Minimum count"));
                ui.add(egui::DragValue::new(&mut alert.minimum).range(1..=u64::MAX))
                    .labelled_by(label.id);
            });
            egui::ComboBox::from_id_salt("count_window")
                .selected_text(l10n::text(ui.ctx(), alert.window.label()))
                .show_ui(ui, |ui| {
                    for window in [
                        CountWindow::Total,
                        CountWindow::FiveMinutes,
                        CountWindow::ThirtySeconds,
                    ] {
                        ui.selectable_value(
                            &mut alert.window,
                            window,
                            l10n::text(ui.ctx(), window.label()),
                        );
                    }
                });
            check(ui, "Count alert sound", &mut alert.sound);
        }
    }
}

fn check(ui: &mut Ui, label: &str, value: &mut bool) -> bool {
    ui.checkbox(value, l10n::text(ui.ctx(), label)).changed()
}

fn detector<R>(
    ui: &mut Ui,
    name: &str,
    detector: &mut Detector<R>,
    rule: impl FnOnce(&mut Ui, &mut R),
) {
    ui.separator();
    check(ui, name, &mut detector.enabled);
    ui.indent("rule", |ui| {
        rule(ui, &mut detector.rule);
        ui.horizontal_wrapped(|ui| {
            check(ui, "Alert", &mut detector.alert);
            check(ui, "Sound", &mut detector.sound);
        });
    });
}

fn threshold(ui: &mut Ui, label: &str, threshold: &mut Threshold) {
    ui.horizontal_wrapped(|ui| {
        let label_response = ui.label(l10n::text(ui.ctx(), label));
        let mut automatic = matches!(threshold, Threshold::Auto);
        if check(ui, "Automatic", &mut automatic) {
            *threshold = if automatic {
                Threshold::Auto
            } else {
                Threshold::Fixed(1.0)
            };
        }
        if let Threshold::Fixed(value) = threshold {
            ui.add(
                egui::DragValue::new(value)
                    .range(f64::MIN_POSITIVE..=f64::MAX)
                    .speed(0.1),
            )
            .labelled_by(label_response.id)
            .on_hover_text(l10n::text(ui.ctx(), label));
            if !value.is_finite() || *value <= 0.0 {
                *value = 1.0;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::MarketEvent;
    use crate::model::{Instrument, OrderSide, Tick};
    use crate::theme::{Theme, apply_to_egui};

    #[test]
    fn panel_edits_control_next_detections_and_poll_delivers_each_alert_once() {
        for width in [220.0, 300.0] {
            let ctx = egui::Context::default();
            apply_to_egui(&ctx, &Theme::dark());
            l10n::set_translator(&ctx, |key| match key {
                "Order flow" => Some("Auftragsfluss".into()),
                "{symbol}: {kind}" => Some("{kind} ({symbol})".into()),
                "Detected at {time} UTC" => Some("Erkannt um {time} UTC".into()),
                _ => None,
            });
            let service = Arc::new(OrderFlowDetectors::new());
            service
                .track(&Instrument::new("ES").with_tick_size(0.25))
                .unwrap();
            let mut settings = service.settings();
            settings.big_trades.enabled = false;
            settings.big_trades.rule.threshold = Threshold::Fixed(10.0);
            service.set_settings(settings);
            let now = DateTime::from_timestamp(0, 0).unwrap();
            let mut panel = OrderFlowPanel::new(service.clone());
            let mut draw = |events| {
                let mut used = egui::Rect::NOTHING;
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        panel.show(ui, "ES", now);
                        used = ui.min_rect();
                    },
                );
                assert!(used.right() <= width, "{used:?}");
                let texts: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| {
                        let egui::Shape::Text(text) = &shape.shape else {
                            return None;
                        };
                        Some((text.galley.text().to_owned(), text.visual_bounding_rect()))
                    })
                    .collect();
                output.drop_without_applying_deltas();
                texts
            };
            let mut texts = draw(Vec::new());
            assert!(texts.iter().any(|(text, _)| text == "Auftragsfluss"));
            for label in ["Big trades", "Alert", "Sound"] {
                let pos = texts
                    .iter()
                    .find(|(text, _)| text == label)
                    .unwrap()
                    .1
                    .center();
                texts = draw(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            assert!(service.settings().big_trades.enabled);
            assert!(service.settings().big_trades.alert);
            assert!(service.settings().big_trades.sound);
            for second in 1..=6 {
                let time = now + chrono::TimeDelta::seconds(second);
                service.push(
                    "ES",
                    time,
                    &MarketEvent::Trade(Tick::new(time, 100.0, 10.0, Some(OrderSide::Buy))),
                );
            }
            let mut toasts = Toasts::new();
            let sounds = panel.poll(&ctx, &mut toasts);
            assert_eq!(sounds, [DetectionKind::BigTrade; 6]);
            assert_eq!(toasts.len(), 5);
            assert!(
                toasts.toasts().iter().all(|toast| toast
                    .title
                    .as_deref()
                    .unwrap()
                    .ends_with("(ES)"))
            );
            assert!(
                toasts
                    .toasts()
                    .iter()
                    .all(|toast| toast.message.starts_with("Erkannt um "))
            );
            assert!(panel.poll(&ctx, &mut toasts).is_empty());
            assert_eq!(toasts.len(), 5);
        }
    }
}
