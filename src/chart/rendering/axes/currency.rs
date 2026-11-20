//! Currency selector above the price ticks.

use crate::chart::rendering::axes::axis_font;
use crate::theme::ThemeExt;
use egui::{Align2, Pos2, Rect, Response, Sense, Stroke, Ui};

/// Paints the currency and unit button at the top of an axis column and returns its response
/// with the host's currency choice and unit choice, if any. The host supplies available
/// conversions and replaces the prices and instrument metadata after a choice; the renderer
/// never converts prices. `unit` follows the currency in the button. The button opens a menu
/// while it shows a currency or `unit_choices` is not empty: the current currency and the
/// other `choices` while a currency shows, then the current unit and the other
/// `unit_choices`. Pass the response rectangle to price tick occlusion.
pub fn render_axis_currency(
    ui: &mut Ui,
    axis: Rect,
    currency: &str,
    unit: Option<&str>,
    choices: &[String],
    unit_choices: &[String],
) -> (Response, Option<String>, Option<String>) {
    let unit = unit.filter(|unit| !unit.is_empty());
    let theme = ui.active_theme();
    let header = match (currency.is_empty(), unit) {
        (false, Some(unit)) => format!("{currency} · {unit}"),
        (true, Some(unit)) => unit.to_owned(),
        _ => currency.to_owned(),
    };
    let font = axis_font(ui.ctx());
    let galley = ui
        .painter()
        .layout_no_wrap(header.clone(), font.clone(), theme.chart.axis_text);
    let menu = opens_menu(currency, unit_choices);
    let width = galley.size().x + 16.0 + if menu { 10.0 } else { 0.0 };
    let rect = Rect::from_min_max(
        Pos2::new((axis.right() - width).max(axis.left()), axis.top()),
        Pos2::new(axis.right(), (axis.top() + 24.0).min(axis.bottom())),
    );
    let response = ui.interact(
        rect,
        ui.scope_id().with(("axis_currency", axis.top().to_bits())),
        Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &header));
    let painter = ui.painter().with_clip_rect(axis.intersect(ui.clip_rect()));
    for offset in 0..18 {
        let y = rect.bottom() + offset as f32;
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(rect.left(), y), Pos2::new(rect.right(), y + 1.0)),
            0.0,
            theme.chart.bg.gamma_multiply(1.0 - offset as f32 / 18.0),
        );
    }
    painter.rect_filled(rect, 0.0, theme.chart.bg);
    painter.text(
        Pos2::new(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        header,
        font,
        theme.chart.axis_text,
    );
    if menu {
        let x = rect.left() + 8.0 + galley.size().x + 4.0;
        painter.add(egui::Shape::line(
            vec![
                Pos2::new(x, rect.center().y - 1.5),
                Pos2::new(x + 3.0, rect.center().y + 1.5),
                Pos2::new(x + 6.0, rect.center().y - 1.5),
            ],
            Stroke::new(1.0, theme.chart.axis_text),
        ));
    }
    let (mut chosen, mut chosen_unit) = (None, None);
    if menu {
        egui::Popup::menu(&response)
            .id(currency_menu_id(ui, axis))
            .show(|ui| {
                if !currency.is_empty() {
                    let _ = ui.selectable_label(true, currency);
                    for choice in choices.iter().filter(|choice| choice.as_str() != currency) {
                        if ui.selectable_label(false, choice).clicked() {
                            chosen = Some(choice.clone());
                            ui.close();
                        }
                    }
                }
                if !unit_choices.is_empty() {
                    if !currency.is_empty() {
                        ui.separator();
                    }
                    if let Some(unit) = unit {
                        let _ = ui.selectable_label(true, unit);
                    }
                    for choice in unit_choices
                        .iter()
                        .filter(|choice| Some(choice.as_str()) != unit)
                    {
                        if ui.selectable_label(false, choice).clicked() {
                            chosen_unit = Some(choice.clone());
                            ui.close();
                        }
                    }
                }
            });
    }
    (response, chosen, chosen_unit)
}

/// Whether the header button showing `currency` opens a menu: it lists the currency or units.
pub(crate) fn opens_menu(currency: &str, unit_choices: &[String]) -> bool {
    !currency.is_empty() || !unit_choices.is_empty()
}

/// The memory id of the currency menu of the price axis `axis`, to keep a label that shows on
/// hover visible while its menu is open.
pub(crate) fn currency_menu_id(ui: &Ui, axis: Rect) -> egui::Id {
    ui.scope_id()
        .with(("axis_currency_menu", axis.top().to_bits()))
}
