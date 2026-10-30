//! The widgets the trading panels share: the order ticket, the depth ladder, the executions and
//! the account panel draw their fields, switches, menus and figures with them.

use crate::localization as l10n;
use egui::{
    Align, Align2, Color32, FontId, Layout, Rect, Response, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};

use crate::model::{OrderSide, TimeInForce, TradingRules};
use crate::scales::{CurrencyFormatter, PriceFormatter};
use crate::theme::{Theme, ThemeExt, readable_color};
use crate::ui::top_toolbar::buttons::rule_color;

pub(super) use crate::theme::trading_side_color as side_color;

/// Height shared by the ticket's fields, menus and order-type row.
pub(super) const FIELD_HEIGHT: f32 = 34.0;

/// Size of section titles between the body and heading text roles.
pub(super) const SECTION_TITLE_SIZE: f32 = 15.0;

/// The fill of a button that places an order on a side.
pub(super) fn side_fill(theme: &Theme, side: OrderSide) -> Color32 {
    match side {
        OrderSide::Buy => theme.ui.accent,
        OrderSide::Sell => theme.ui.danger_bg,
    }
}

/// The fill of a switch's track and of its chosen segment.
pub(super) fn track(ui: &Ui) -> (Color32, Color32) {
    let colors = ui.active_theme().ui;
    if ui.visuals().dark_mode {
        (colors.border, rule_color(&ui.active_theme()))
    } else {
        (colors.panel_bg_secondary, colors.panel_bg)
    }
}

pub(super) fn side_name(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "Buy",
        OrderSide::Sell => "Sell",
    }
}

/// The name of a time in force in the ticket's menu.
pub(super) fn time_in_force_name(time_in_force: TimeInForce) -> &'static str {
    match time_in_force {
        TimeInForce::Day => "Day",
        TimeInForce::Week => "Week",
        TimeInForce::Month => "Month",
        TimeInForce::GoodTillCancelled => "Good till cancelled",
        TimeInForce::ImmediateOrCancel => "Immediate or cancel",
        TimeInForce::FillOrKill => "Fill or kill",
    }
}

/// A full-width menu of the times in force the broker accepts for the symbol.
pub(super) fn time_in_force_menu(
    ui: &mut Ui,
    id: &str,
    value: &mut TimeInForce,
    rules: &TradingRules,
) {
    let theme = ui.active_theme();
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = FIELD_HEIGHT;
        // Keep the menu's field styling local to this control and its popup.
        let widgets = &mut ui.visuals_mut().widgets;
        for state in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            state.weak_bg_fill = theme.ui.panel_bg;
            state.corner_radius = 6.into();
        }
        widgets.inactive.bg_stroke = Stroke::new(1.0, field_border(&theme));
        widgets.hovered.bg_stroke = Stroke::new(1.0, field_hover_border(&theme));
        widgets.active.bg_stroke = Stroke::new(1.0, theme.ui.accent);
        widgets.open.bg_stroke = widgets.active.bg_stroke;
        egui::ComboBox::from_id_salt(id)
            .width(ui.available_width())
            .selected_text(l10n::text(ui.ctx(), time_in_force_name(*value)))
            .icon(|ui, rect, visuals, open| {
                chevron(ui, rect.center(), visuals.fg_stroke.color, open);
            })
            .show_ui(ui, |ui| {
                for option in &rules.time_in_force {
                    ui.selectable_value(
                        value,
                        *option,
                        l10n::text(ui.ctx(), time_in_force_name(*option)),
                    );
                }
            });
    });
}

/// A click area over `rect`, announced as `label`.
pub(super) fn hit(
    ui: &mut Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    selected: Option<bool>,
) -> Response {
    let response = ui.interact(rect, ui.make_persistent_id(id), Sense::click());
    let translated_label = l10n::text(ui.ctx(), label);
    response.widget_info(|| match selected {
        Some(selected) => {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, &translated_label)
        }
        None => WidgetInfo::labeled(Role::Button, ui.is_enabled(), &translated_label),
    });
    response
}

pub(super) fn text(ui: &Ui, at: egui::Pos2, align: Align2, text: &str, size: f32, color: Color32) {
    ui.painter().text(
        at,
        align,
        l10n::text(ui.ctx(), text),
        FontId::proportional(size),
        color,
    );
}

pub(super) fn muted(ui: &mut Ui, text: &str) {
    let color = ui.active_theme().ui.text_muted;
    ui.label(
        RichText::new(l10n::text(ui.ctx(), text))
            .color(color)
            .size(ui.active_theme().text.body),
    );
}

pub(super) fn title(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(l10n::text(ui.ctx(), text))
            .strong()
            .size(SECTION_TITLE_SIZE),
    );
}

/// A label with a small chevron that opens a menu.
pub(super) fn dropdown(ui: &mut Ui, label: String) -> Response {
    let color = ui.active_theme().ui.text_muted;
    let galley = ui.painter().layout_no_wrap(
        l10n::text(ui.ctx(), label),
        FontId::proportional(ui.active_theme().text.body),
        color,
    );
    let size = vec2(galley.size().x + 16.0, galley.size().y.max(18.0));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, ui.is_enabled(), galley.text()));
    let color = if response.hovered() {
        ui.active_theme().ui.text
    } else {
        color
    };
    let at = rect.left_center() - vec2(0.0, galley.size().y / 2.0);
    ui.painter().galley(at, galley, color);
    chevron(ui, pos2(rect.right() - 5.0, rect.center().y), color, false);
    response
}

/// A chevron pointing up or down.
pub(super) fn chevron(ui: &Ui, center: egui::Pos2, color: Color32, up: bool) {
    let rise = if up { -2.0 } else { 2.0 };
    ui.painter().line(
        vec![
            center + vec2(-4.0, -rise),
            center + vec2(0.0, rise),
            center + vec2(4.0, -rise),
        ],
        Stroke::new(1.3, color),
    );
}

/// The least prominent field outline that still reads on its background.
fn field_border(theme: &Theme) -> Color32 {
    readable_color(
        theme.ui.border,
        theme.ui.border_control,
        &[theme.ui.panel_bg],
        3.0,
    )
}

/// Hover stays stronger than rest in both light and dark palettes.
fn field_hover_border(theme: &Theme) -> Color32 {
    let toward = if theme.dark {
        theme.ui.panel_bg
    } else {
        theme.ui.text
    };
    theme.ui.border_control.lerp_to_gamma(toward, 0.175)
}

/// A bordered input box the width of `ui`, laid out from the left.
pub(super) fn field(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let theme = ui.active_theme();
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), FIELD_HEIGHT), Sense::hover());
    let enabled = ui.is_enabled();
    let fill = if !enabled && !theme.dark {
        theme.ui.panel_bg.lerp_to_gamma(theme.ui.text, 0.1)
    } else {
        theme.ui.panel_bg
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    let inner = rect.shrink2(vec2(10.0, 0.0));
    let layout = Layout::left_to_right(Align::Center);
    ui.scope_builder(UiBuilder::new().max_rect(inner).layout(layout), |ui| {
        add(ui);
    });
    let focused = ui
        .memory(|memory| memory.focused())
        .and_then(|id| ui.ctx().read_response(id))
        .is_some_and(|response| {
            response.has_focus()
                && response.layer_id == ui.layer_id()
                && rect.intersects(response.rect)
        });
    let border = if !enabled {
        theme.ui.border
    } else if focused {
        theme.ui.accent
    } else if ui.rect_contains_pointer(rect) {
        field_hover_border(&theme)
    } else {
        field_border(&theme)
    };
    ui.painter()
        .rect_stroke(rect, 6.0, Stroke::new(1.0, border), StrokeKind::Inside);
}

/// A number typed in place at `places` decimals. Invalid input invalidates the order until
/// corrected instead of silently submitting the previously entered value.
pub(super) fn number(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut f64,
    places: usize,
) -> Response {
    let id = ui.make_persistent_id(id);
    let editing = ui.memory(|memory| memory.has_focus(id));
    let mut text = match ui.data(|data| data.get_temp::<String>(id)) {
        Some(text) if editing => text,
        _ => format!("{value:.places$}"),
    };
    let width = (ui.available_width() * 0.5).max(48.0);
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .frame(egui::Frame::NONE)
            .desired_width(width)
            .font(FontId::proportional(ui.active_theme().text.title)),
    );
    if response.changed() {
        *value = text
            .trim()
            .replace(',', "")
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .unwrap_or(f64::NAN);
    }
    if response.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, text));
    }
    response
}

/// Two opposite arrows that swap what a field takes.
pub(super) fn swap(ui: &mut Ui, tooltip: &str) -> bool {
    let theme = ui.active_theme();
    let (rect, response) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    let translated_label = l10n::text(ui.ctx(), tooltip);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &translated_label));
    let color = if response.hovered() {
        theme.ui.text
    } else {
        theme.ui.text_muted
    };
    let stroke = Stroke::new(1.2, color);
    let c = rect.center();
    let painter = ui.painter();
    painter.line_segment([c + vec2(-6.0, -3.0), c + vec2(6.0, -3.0)], stroke);
    painter.line(
        vec![c + vec2(3.0, -6.0), c + vec2(6.0, -3.0), c + vec2(3.0, 0.0)],
        stroke,
    );
    painter.line_segment([c + vec2(6.0, 3.0), c + vec2(-6.0, 3.0)], stroke);
    painter.line(
        vec![
            c + vec2(-3.0, 0.0),
            c + vec2(-6.0, 3.0),
            c + vec2(-3.0, 6.0),
        ],
        stroke,
    );
    response
        .on_hover_text(l10n::text(ui.ctx(), tooltip))
        .clicked()
}

/// An on and off switch.
pub(super) fn toggle(ui: &mut Ui, on: &mut bool, label: &str) {
    let theme = ui.active_theme();
    let (rect, mut response) = ui.allocate_exact_size(vec2(39.0, 21.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let translated_label = l10n::text(ui.ctx(), label);
    response.widget_info(|| {
        WidgetInfo::selected(Role::Switch, ui.is_enabled(), *on, &translated_label)
    });
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let track = if *on {
        theme.ui.text
    } else {
        theme.ui.border_control
    };
    ui.painter().rect_filled(rect, rect.height() / 2.0, track);
    let x = egui::lerp(
        rect.left() + rect.height() / 2.0..=rect.right() - rect.height() / 2.0,
        t,
    );
    ui.painter().circle_filled(
        pos2(x, rect.center().y),
        7.0,
        if *on {
            theme.ui.btn_bg_active
        } else {
            theme.ui.panel_bg
        },
    );
}

/// A row of the order info: `label` at the left, `value` and `unit` at the right. Returns the
/// row's rectangle.
pub(super) fn info(ui: &mut Ui, label: &str, value: &str, unit: &str, color: Color32) -> Rect {
    let theme = ui.active_theme();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    let left = rect.left_center();
    text(
        ui,
        left,
        Align2::LEFT_CENTER,
        label,
        theme.text.body,
        if theme.dark {
            theme.ui.text_muted
        } else {
            theme.ui.text
        },
    );
    let mut right = rect.right_center();
    if !unit.is_empty() {
        let galley = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), unit),
            FontId::proportional(theme.text.small),
            theme.ui.text_muted,
        );
        right.x -= galley.size().x;
        ui.painter().galley(
            right - vec2(0.0, galley.size().y / 2.0),
            galley,
            theme.ui.text_muted,
        );
        right.x -= 4.0;
    }
    let galley = ui.painter().layout_no_wrap(
        l10n::text(ui.ctx(), value),
        FontId::proportional(ui.active_theme().text.body),
        color,
    );
    let at = right - vec2(galley.size().x, galley.size().y / 2.0);
    ui.painter().galley(at, galley, color);
    rect
}

/// `value` with thousands separators and `decimals` decimals, signed with `sign`.
pub(super) fn grouped(value: f64, decimals: usize, sign: bool) -> String {
    if !value.is_finite() {
        return "-".to_owned();
    }
    let magnitude = CurrencyFormatter::new("", decimals).format(value.abs());
    let shown = magnitude.bytes().any(|b| matches!(b, b'1'..=b'9'));
    match (shown, value < 0.0, sign) {
        (true, true, _) => format!("-{magnitude}"),
        (true, false, true) => format!("+{magnitude}"),
        _ => magnitude,
    }
}

/// `text` at `size` in the bundled face's semibold weight.
pub(super) fn semibold(text: &str, size: f32, color: Color32) -> egui::text::LayoutJob {
    let mut format = egui::TextFormat::simple(FontId::proportional(size), color);
    format.coords.push("wght", 600.0);
    egui::text::LayoutJob::single_section(text.to_owned(), format)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Field outlines follow the pointer and keyboard focus; a disabled field is filled.
    #[test]
    fn fields_show_hover_focus_and_disabled_states() {
        for theme in [Theme::dark(), Theme::light()] {
            for state in ["rest", "hover", "focus", "popup-focus", "disabled"] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut value = 1.0;
                let mut popup_value = 1.0;
                let mut frame = |events| {
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                egui::Pos2::ZERO,
                                vec2(300.0, 200.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            ui.set_width(200.0);
                            ui.add_enabled_ui(state != "disabled", |ui| {
                                field(ui, |ui| {
                                    let response = number(ui, "units", &mut value, 0);
                                    if state == "focus" {
                                        response.request_focus();
                                    }
                                });
                            });
                            if state == "popup-focus" {
                                egui::Area::new(egui::Id::new("field_popup"))
                                    .order(egui::Order::Foreground)
                                    .fixed_pos(pos2(4.0, 4.0))
                                    .show(ui.ctx(), |ui| {
                                        ui.set_width(100.0);
                                        number(ui, "popup_value", &mut popup_value, 0)
                                            .request_focus();
                                    });
                            }
                        },
                    )
                };
                let outline_of = |output: &egui::FullOutput, rect: Rect| {
                    output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(shape)
                            if shape.rect == rect && shape.stroke.width > 0.0 =>
                        {
                            Some(shape.stroke.color)
                        }
                        _ => None,
                    })
                };
                let first = frame(vec![]);
                let rect = first
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) if rect.rect.height() == FIELD_HEIGHT => {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .unwrap();
                let rest_outline = outline_of(&first, rect).unwrap();
                first.drop_without_applying_deltas();
                let output = frame(if state == "hover" {
                    vec![egui::Event::PointerMoved(rect.center())]
                } else {
                    vec![]
                });
                let outline = outline_of(&output, rect).unwrap();
                let fill = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(shape)
                            if shape.rect == rect && shape.fill != Color32::TRANSPARENT =>
                        {
                            Some(shape.fill)
                        }
                        _ => None,
                    })
                    .unwrap();
                if state == "focus" {
                    assert_eq!(outline, theme.ui.accent, "focused {state}");
                } else if state == "hover" {
                    assert!(crate::theme::contrast(outline, theme.ui.panel_bg) >= 3.0);
                    assert!(
                        crate::theme::contrast(outline, theme.ui.panel_bg)
                            > crate::theme::contrast(rest_outline, theme.ui.panel_bg),
                        "hover must strengthen the outline"
                    );
                } else if state == "popup-focus" {
                    assert_ne!(
                        outline, theme.ui.accent,
                        "a popup must not focus the field beneath it"
                    );
                } else if state == "rest" {
                    assert!(crate::theme::contrast(outline, theme.ui.panel_bg) >= 3.0);
                    assert!(
                        crate::theme::contrast(outline, theme.ui.panel_bg)
                            < crate::theme::contrast(theme.ui.border_control, theme.ui.panel_bg)
                    );
                } else if !theme.dark {
                    assert_ne!(
                        theme.ui.panel_bg.blend(fill),
                        theme.ui.panel_bg,
                        "disabled fields must have a fill"
                    );
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn figures_group_thousands_and_sign_only_what_they_show() {
        for (value, sign, text) in [
            (-123_456.0, false, "-123,456.00"),
            (-24_932.084, true, "-24,932.08"),
            (100_050.68, true, "+100,050.68"),
            (1_234_567.891, false, "1,234,567.89"),
            (-0.004, true, "0.00"),
            (0.0, true, "0.00"),
            (f64::NAN, false, "-"),
        ] {
            assert_eq!(grouped(value, 2, sign), text, "{value}");
        }
    }
}
