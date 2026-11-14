//! Currency selector above the price ticks.

use crate::chart::rendering::axes::axis_font;
use crate::theme::ThemeExt;
use egui::{Align2, Pos2, Rect, Response, Sense, Stroke, Ui};

/// Paints a currency button at the top of an axis column and returns its response and
/// the host currency choice, if any. The host supplies available conversions and replaces
/// the prices and instrument metadata after a choice; the renderer never converts prices.
/// `unit` appends the host's price unit without converting prices. An empty currency shows only
/// the unit and offers no conversion menu. Pass the response rectangle to price tick occlusion.
pub fn render_axis_currency(
    ui: &mut Ui,
    axis: Rect,
    currency: &str,
    unit: Option<&str>,
    choices: &[String],
) -> (Response, Option<String>) {
    let theme = ui.active_theme();
    let header = match (currency.is_empty(), unit.filter(|unit| !unit.is_empty())) {
        (false, Some(unit)) => format!("{currency} · {unit}"),
        (true, Some(unit)) => unit.to_owned(),
        _ => currency.to_owned(),
    };
    let font = axis_font(ui.ctx());
    let galley = ui
        .painter()
        .layout_no_wrap(header.clone(), font.clone(), theme.chart.axis_text);
    let width = galley.size().x + 16.0 + if currency.is_empty() { 0.0 } else { 10.0 };
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
    if !currency.is_empty() {
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
    let mut chosen = None;
    if !currency.is_empty() {
        egui::Popup::menu(&response).show(|ui| {
            let _ = ui.selectable_label(true, currency);
            for choice in choices.iter().filter(|choice| choice.as_str() != currency) {
                if ui.selectable_label(false, choice).clicked() {
                    chosen = Some(choice.clone());
                    ui.close();
                }
            }
        });
    }
    (response, chosen)
}
