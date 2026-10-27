//! Interactive alert levels. Changes are host intents; rendering never changes alert status.

use egui::{Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::chart::coords::ChartMapping;
use crate::model::{Alert, AlertCondition, AlertSource, AlertStatus};

/// Draw price thresholds and return a replacement alert after a drag.
/// Supply only alerts for this pane, resolving symbol or watchlist membership in the host.
/// With `active_only` unset, triggered, disabled and expired alerts are drawn dimmed.
/// Call after drawing the chart, then submit the result through the host's alerts service.
/// Non-price sources require the host to resolve their own pane and coordinates.
/// `format` supplies the instrument's precision and currency for labels.
pub fn render_alert_levels<'a>(
    ui: &mut Ui,
    mapping: &ChartMapping,
    alerts: impl IntoIterator<Item = &'a Alert>,
    color: Color32,
    labels: bool,
    active_only: bool,
    format: impl Fn(f64) -> String,
) -> Option<Alert> {
    let mut changed = None;
    let painter = ui.painter().with_clip_rect(mapping.rect);
    for alert in alerts
        .into_iter()
        .filter(|a| !active_only || a.status == AlertStatus::Active)
    {
        let Some(id) = &alert.id else { continue };
        let active = alert.status == AlertStatus::Active;
        let color = if active {
            color
        } else {
            color.gamma_multiply(0.5)
        };
        let rules = std::iter::once((&alert.source, &alert.condition)).chain(
            alert
                .additional_conditions
                .iter()
                .map(|r| (&r.source, &r.condition)),
        );
        for (index, (source, condition)) in rules.enumerate() {
            if *source != AlertSource::Price {
                continue;
            }
            let levels = match condition {
                AlertCondition::EnteringChannel { lower, upper }
                | AlertCondition::ExitingChannel { lower, upper } => vec![*lower, *upper],
                _ => condition.level().into_iter().collect(),
            };
            for (edge, level) in levels
                .into_iter()
                .enumerate()
                .filter(|(_, v)| v.is_finite())
            {
                let key = ui.scope_id().with(("alert_level", &id.0, index, edge));
                let staged = ui
                    .ctx()
                    .data(|data| data.get_temp::<f64>(key))
                    .unwrap_or(level);
                let y = mapping.price_to_y(staged);
                if !y.is_finite() || !mapping.is_y_visible(y) {
                    continue;
                }
                let hit = Rect::from_min_max(
                    pos2(mapping.rect.left(), y - 6.0),
                    pos2(mapping.rect.right(), y + 6.0),
                )
                .intersect(mapping.rect);
                let response = ui
                    .interact(hit, key, Sense::drag())
                    .on_hover_text("Drag alert level");
                let mut value = staged;
                if response.dragged()
                    && let Some(pointer) = response.interact_pointer_pos()
                {
                    value = mapping
                        .y_to_price(pointer.y.clamp(mapping.rect.top(), mapping.rect.bottom()));
                    if value.is_finite() {
                        ui.ctx().data_mut(|data| data.insert_temp(key, value));
                    }
                }
                if response.drag_stopped() {
                    ui.ctx().data_mut(|data| data.remove::<f64>(key));
                    if value.is_finite() && value != level {
                        let mut updated = alert.clone();
                        let condition = if index == 0 {
                            &mut updated.condition
                        } else {
                            &mut updated.additional_conditions[index - 1].condition
                        };
                        match condition {
                            AlertCondition::Crossing(v)
                            | AlertCondition::CrossingUp(v)
                            | AlertCondition::CrossingDown(v)
                            | AlertCondition::Above(v)
                            | AlertCondition::Below(v) => *v = value,
                            AlertCondition::EnteringChannel { lower, upper }
                            | AlertCondition::ExitingChannel { lower, upper } => {
                                if edge == 0 {
                                    *lower = value;
                                } else {
                                    *upper = value;
                                }
                            }
                            _ => continue,
                        }
                        if updated.is_valid() {
                            changed = Some(updated);
                        }
                    }
                }
                let y = mapping.price_to_y(value);
                painter.hline(mapping.rect.x_range(), y, Stroke::new(1.0, color));
                if labels {
                    let name = if alert.name.is_empty() {
                        "Alert"
                    } else {
                        &alert.name
                    };
                    let text = format!("{name}  {}", format(value));
                    let font = FontId::proportional(12.0);
                    let text_color = if active {
                        ui.visuals().text_color()
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    let galley = painter.layout_no_wrap(text, font, text_color);
                    let label = Rect::from_min_size(
                        pos2(mapping.rect.left() + 8.0, y - galley.size().y - 4.0),
                        galley.size() + vec2(8.0, 2.0),
                    );
                    painter.rect_filled(label, 2.0, ui.visuals().panel_fill);
                    painter.galley(label.min + vec2(4.0, 1.0), galley, text_color);
                }
            }
        }
    }
    changed
}

/// Resolve Alt+A at a hovered chart price without handling keys owned by a field or popup.
/// The host decides whether to open a dialog or save a default alert at the returned price.
pub fn alert_shortcut_price(ui: &mut Ui, mapping: &ChartMapping) -> Option<f64> {
    let pointer = ui.ctx().pointer_hover_pos()?;
    if !ui.rect_contains_pointer(mapping.rect)
        || ui.ctx().egui_wants_keyboard_input()
        || egui::Popup::is_any_open(ui.ctx())
    {
        return None;
    }
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::A)) {
        let price = mapping.y_to_price(pointer.y);
        return price.is_finite().then_some(price);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AlertId;
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::theme::Theme;

    #[test]
    fn inactive_levels_follow_the_filter_and_stay_dimmed() {
        for theme in [Theme::light(), Theme::dark()] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            for status in [
                AlertStatus::Active,
                AlertStatus::Triggered,
                AlertStatus::Disabled,
                AlertStatus::Expired,
            ] {
                let mut alert = Alert::new("SAMPLE", AlertCondition::Crossing(100.0));
                alert.id = Some(AlertId("level".into()));
                alert.status = status;
                for active_only in [false, true] {
                    let mut output = ctx.run_ui(Default::default(), |ui| {
                        let rect = ui.max_rect();
                        let map = ChartMapping::new(
                            rect,
                            6.0,
                            0,
                            0,
                            0.0,
                            PriceMap::new(
                                PriceTransform::default(),
                                PriceRange::new(90.0, 110.0),
                                rect.top(),
                                rect.bottom(),
                                false,
                            ),
                        );
                        render_alert_levels(
                            ui,
                            &map,
                            [&alert],
                            Color32::GOLD,
                            true,
                            active_only,
                            |price| price.to_string(),
                        );
                    });
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    let visible = !active_only || status == AlertStatus::Active;
                    let lines: Vec<_> = shapes
                        .iter()
                        .filter_map(|s| match s.shape {
                            egui::Shape::LineSegment { stroke, .. } => Some(stroke.color),
                            _ => None,
                        })
                        .collect();
                    let expected = if status == AlertStatus::Active {
                        Color32::GOLD
                    } else {
                        Color32::GOLD.gamma_multiply(0.5)
                    };
                    assert_eq!(
                        lines,
                        if visible { vec![expected] } else { vec![] },
                        "{status:?}, active_only={active_only}"
                    );
                    let label = shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(text) if text.galley.text() == "Alert  100"));
                    assert_eq!(label, visible);
                }
            }
        }
    }
}
