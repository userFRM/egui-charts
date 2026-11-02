use super::{Layout, QuoteSide, semibold};
use crate::localization as l10n;
use crate::theme::ThemeExt;
use egui::{Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};
use std::sync::Arc;

/// The Sell and Buy buttons with the spread between them and, under it, `quantity` in a box.
/// Returns the button clicked and the quantity box, when clicked.
pub(super) fn paint_quotes(
    ui: &mut Ui,
    layout: &mut Layout<'_>,
    bid: f64,
    ask: f64,
    precision: usize,
    thousands_sep: Option<char>,
    quantity: Option<f64>,
    id: egui::Id,
) -> (Option<QuoteSide>, Option<Rect>) {
    let spread =
        crate::scales::group_thousands(format!("{:.precision$}", ask - bid), thousands_sep);
    let theme = ui.active_theme();
    let spread_galley = layout.painter.layout_no_wrap(
        spread,
        FontId::proportional(theme.text.tiny),
        layout.colors.text,
    );
    let gap = (spread_galley.size().x + 12.0).max(36.0);
    let quote = |side: QuoteSide, price: f64, color: Color32| {
        let price = layout
            .painter
            .layout_job(egui::text::LayoutJob::single_section(
                crate::scales::group_thousands(format!("{price:.precision$}"), thousands_sep),
                semibold(FontId::proportional(theme.text.ohlc), color),
            ));
        let label = layout.painter.layout_no_wrap(
            l10n::text(
                ui.ctx(),
                if side == QuoteSide::Sell {
                    "SELL"
                } else {
                    "BUY"
                },
            ),
            FontId::proportional(theme.text.small),
            color,
        );
        (price, label)
    };
    let sell = quote(QuoteSide::Sell, bid, theme.chart.candle_down);
    let buy = quote(QuoteSide::Buy, ask, theme.ui.accent);
    let width = 59.0_f32
        .max(sell.0.size().x + 12.0)
        .max(buy.0.size().x + 12.0);
    let size = Vec2::new(width, 34.0);
    let sell_rect = layout.reserve(size);
    let sell_response = quote_box(
        ui,
        layout.painter,
        sell_rect,
        sell,
        theme.chart.candle_down,
        &l10n::text(ui.ctx(), "Sell at bid"),
        id.with("sell"),
    );
    let spread_rect = Rect::from_min_size(
        Pos2::new(sell_rect.right(), sell_rect.top()),
        Vec2::new(gap, 34.0),
    );
    layout.painter.rect_filled(spread_rect, 0.0, theme.chart.bg);
    let quantity = quantity.map(|quantity| {
        let text = layout.painter.layout_no_wrap(
            compact_quantity(quantity),
            FontId::proportional(theme.text.small),
            layout.colors.text,
        );
        let rect = Rect::from_center_size(
            Pos2::new(spread_rect.center().x, spread_rect.bottom() - 9.0),
            Vec2::new((text.size().x + 10.0).max(gap - 8.0), 18.0),
        );
        let response = ui
            .interact(
                rect.intersect(layout.painter.clip_rect()),
                id.with("quantity"),
                Sense::click(),
            )
            .on_hover_text(l10n::text(ui.ctx(), "Order quantity"));
        let label = l10n::text(ui.ctx(), "Order quantity");
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        let fill = if response.hovered() {
            theme.ui.btn_bg_active
        } else {
            theme.ui.btn_bg_hover
        };
        layout.painter.rect_filled(rect, theme.radius.sm, fill);
        layout
            .painter
            .galley(rect.center() - text.size() / 2.0, text, layout.colors.text);
        (rect, response.clicked())
    });
    // With the quantity box under it, the spread sits at the top.
    let spread_y = match quantity {
        Some((rect, _)) => (spread_rect.top() + rect.top() - spread_galley.size().y) / 2.0,
        None => spread_rect.center().y - spread_galley.size().y / 2.0,
    };
    layout.painter.galley(
        Pos2::new(
            spread_rect.center().x - spread_galley.size().x / 2.0,
            spread_y,
        ),
        spread_galley,
        layout.colors.text,
    );
    layout.pos.x = sell_rect.right() + gap;
    let buy_rect = layout.reserve(size);
    let buy_response = quote_box(
        ui,
        layout.painter,
        buy_rect,
        buy,
        theme.ui.accent,
        &l10n::text(ui.ctx(), "Buy at ask"),
        id.with("buy"),
    );
    layout.pos.x = buy_rect.right() + 8.0;
    let dropdown = layout.reserve(Vec2::new(28.0, size.y));
    layout.painter.rect(
        dropdown,
        4.0,
        theme.chart.bg,
        Stroke::new(1.0, theme.ui.border),
        egui::StrokeKind::Inside,
    );
    let c = dropdown.center();
    let points = [
        c + egui::vec2(-4.0, -2.0),
        c + egui::vec2(0.0, 1.5),
        c + egui::vec2(4.0, -2.0),
    ];
    for pair in points.windows(2) {
        layout
            .painter
            .line_segment([pair[0], pair[1]], Stroke::new(1.5, theme.ui.text));
    }
    for point in points {
        layout.painter.circle_filled(point, 0.75, theme.ui.text);
    }
    let dropdown = ui
        .interact(dropdown, id.with("ticket"), Sense::click())
        .on_hover_text(l10n::text(ui.ctx(), "Order ticket"));
    let label = l10n::text(ui.ctx(), "Order ticket");
    dropdown.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    let action = if dropdown.clicked() {
        Some(QuoteSide::Ticket)
    } else if sell_response.clicked() {
        Some(QuoteSide::Sell)
    } else if buy_response.clicked() {
        Some(QuoteSide::Buy)
    } else {
        None
    };
    let clicked = quantity.and_then(|(rect, clicked)| clicked.then_some(rect));
    (action, clicked)
}

fn quote_box(
    ui: &mut Ui,
    painter: &egui::Painter,
    rect: Rect,
    (price, label): (Arc<egui::Galley>, Arc<egui::Galley>),
    color: Color32,
    name: &str,
    id: egui::Id,
) -> Response {
    let response = ui.interact(rect.intersect(painter.clip_rect()), id, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    painter.rect(
        rect,
        5.0,
        ui.active_theme().chart.bg,
        Stroke::new(1.0, color),
        egui::StrokeKind::Inside,
    );
    painter.galley(
        Pos2::new(rect.center().x - price.size().x / 2.0, rect.top() + 2.0),
        price,
        color,
    );
    painter.galley(
        Pos2::new(
            rect.center().x - label.size().x / 2.0,
            rect.bottom() - label.size().y - 2.0,
        ),
        label,
        color,
    );
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            5.0,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    response
}

fn compact_quantity(quantity: f64) -> String {
    for (divisor, suffix) in [(1_000_000_000.0, "B"), (1_000_000.0, "M"), (1_000.0, "K")] {
        if quantity.abs() >= divisor {
            let number = format!("{:.2}", quantity / divisor);
            return format!(
                "{}{suffix}",
                number.trim_end_matches('0').trim_end_matches('.')
            );
        }
    }
    quantity.to_string()
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::super::{Bar, Indicator, Instrument, Legend, Level1Quote};
    use super::*;
    use chrono::{TimeZone, Utc};
    use egui::{Context, Event, Key, PointerButton, Shape};

    #[test]
    fn quote_controls_and_collapse_belong_to_each_legend() {
        for (_, theme) in crate::theme::presets() {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let bars = [
                Bar::new(
                    Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                    100.0,
                    110.0,
                    90.0,
                    105.0,
                    1_000.0,
                ),
                Bar::new(
                    Utc.timestamp_opt(1_700_003_600, 0).unwrap(),
                    105.0,
                    110.0,
                    90.0,
                    95.0,
                    2_000.0,
                ),
            ];
            let instrument = Instrument::new("SAMPLE").with_price_decimals(2);
            let mut study = crate::studies::BollingerBands::new(2, 2.0);
            study.calculate(&bars);
            let studies: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
            let quote = Level1Quote {
                ts: bars[1].time,
                symbol: "SAMPLE".into(),
                bid: 104.0,
                ask: 106.0,
                bid_size: 1.0,
                ask_size: 1.0,
            };
            let frame = |selected: usize, events| {
                let mut actions = [None; 2];
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        for (index, action) in actions.iter_mut().enumerate() {
                            let mut legend = Legend::new("SAMPLE", "1h", &bars[selected]);
                            legend.previous_close = selected.checked_sub(1).map(|i| bars[i].close);
                            legend.quote = Some(&quote);
                            legend.instrument = Some(&instrument);
                            legend.studies = &studies;
                            legend.study_inputs = &["2 2"];
                            legend.bar_index = selected;
                            *action = legend.show(
                                ui,
                                Rect::from_min_size(
                                    Pos2::new(index as f32 * 600.0, 0.0),
                                    Vec2::new(600.0, 400.0),
                                ),
                            );
                        }
                    },
                );
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                (shapes, actions)
            };
            let texts = |shapes: &[egui::epaint::ClippedShape]| {
                shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) => Some(text.galley.text().to_owned()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            let (first, _) = frame(0, vec![]);
            let (second, _) = frame(1, vec![]);
            let a = texts(&first);
            let b = texts(&second);
            for (text, values) in [
                ("C 105.00", &a),
                ("Vol 1.00K", &a),
                ("C 95.00", &b),
                ("Vol 2.00K", &b),
                ("-10.00 (-9.52%)", &b),
            ] {
                assert!(values.iter().any(|value| value == text), "{text}");
            }
            for text in [
                "BB",
                "2 2",
                "BB Upper(2, 2): 110.00",
                "BB Middle(2, 2): 100.00",
                "BB Lower(2, 2): 90.00",
            ] {
                assert!(b.iter().any(|value| value == text), "{text}");
            }
            for text in ["104.00", "106.00", "2.00", "SELL", "BUY"] {
                assert!(a.iter().any(|value| value == text) && b.iter().any(|value| value == text));
            }
            let titles: Vec<_> = second
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Text(text) if text.galley.text() == "SAMPLE · 1h" => Some(text),
                    _ => None,
                })
                .collect();
            assert_eq!(titles.len(), 2);
            for (index, title) in titles.iter().enumerate() {
                assert_eq!(title.pos.x, index as f32 * 600.0 + 31.0);
                assert_eq!(title.galley.job.sections[0].format.font_id.size, 13.0);
            }
            let buttons: Vec<_> = second
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Rect(rect) if rect.stroke.width > 0.0 => Some(rect.rect),
                    _ => None,
                })
                .collect();
            assert_eq!(buttons.len(), 8);
            for (index, side) in [
                (4, QuoteSide::Sell),
                (5, QuoteSide::Buy),
                (6, QuoteSide::Ticket),
            ] {
                let pos = buttons[index].center();
                frame(1, vec![Event::PointerMoved(pos)]);
                frame(
                    1,
                    vec![Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    }],
                );
                let (_, actions) = frame(
                    1,
                    vec![Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    }],
                );
                assert_eq!(actions, [None, Some(side)]);
            }
            let key = |key, pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Default::default(),
            };
            // Tab through both legends, then activate each control in the second one.
            for (index, side) in [
                (4, Some(QuoteSide::Sell)),
                (5, Some(QuoteSide::Buy)),
                (6, Some(QuoteSide::Ticket)),
                (7, None),
            ] {
                let mut reached = false;
                for _ in 0..24 {
                    frame(1, vec![key(Key::Tab, true)]);
                    frame(1, vec![key(Key::Tab, false)]);
                    reached = ctx
                        .memory(|memory| memory.focused())
                        .and_then(|id| ctx.read_response(id))
                        .is_some_and(|response| response.rect == buttons[index]);
                    if reached {
                        break;
                    }
                }
                assert!(reached, "legend control must remain keyboard reachable");
                let (_, actions) = frame(1, vec![key(Key::Enter, true)]);
                assert_eq!(actions, [None, side]);
                frame(1, vec![key(Key::Enter, false)]);
            }
            let (collapsed, _) = frame(1, vec![]);
            let text = texts(&collapsed);
            assert_eq!(text.iter().filter(|text| *text == "Vol 2.00K").count(), 1);
            assert_eq!(text.iter().filter(|text| *text == "BB").count(), 1);
            assert_eq!(text.iter().filter(|text| *text == "104.00").count(), 2);
            frame(1, vec![key(Key::Enter, true)]);
            let (restored, _) = frame(1, vec![key(Key::Enter, false)]);
            assert_eq!(
                texts(&restored).iter().filter(|text| *text == "BB").count(),
                2
            );
        }
    }
}
