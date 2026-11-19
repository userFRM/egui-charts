use super::{Layout, Legend, QuoteSide, StyleColors, semibold};
use crate::localization as l10n;
use crate::model::Level1Quote;
use crate::theme::ThemeExt;
use egui::{Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};
use std::sync::Arc;

/// The prices the Sell and Buy buttons show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum QuotePrices {
    /// Sell at the bid, Buy at the ask, with their spread between them.
    BidAsk { bid: f64, ask: f64 },
    /// Both buttons show the last price, without a spread.
    Last(f64),
}

impl QuotePrices {
    pub(super) fn magnitude(self) -> f64 {
        match self {
            Self::BidAsk { bid, ask } => bid.abs().max(ask.abs()),
            Self::Last(price) => price.abs(),
        }
    }
}

/// A valid two-sided quote, else the last price when finite.
pub(crate) fn quote_prices(quote: Option<&Level1Quote>, last: Option<f64>) -> Option<QuotePrices> {
    if let Some(quote) = quote {
        let (bid, ask) = (quote.bid, quote.ask);
        if bid.is_finite() && ask.is_finite() && bid <= ask && quote.spread().is_finite() {
            return Some(QuotePrices::BidAsk { bid, ask });
        }
    }
    last.filter(|price| price.is_finite())
        .map(QuotePrices::Last)
}

/// The Sell and Buy buttons with the spread between them and, under it, `quantity` in a box.
/// Prices and the spread use `formatter`, else `precision` decimals grouped by `thousands_sep`.
/// Returns the button clicked and the quantity box, when clicked.
pub(super) fn paint_quotes(
    ui: &mut Ui,
    layout: &mut Layout<'_>,
    prices: QuotePrices,
    precision: usize,
    thousands_sep: Option<char>,
    formatter: Option<&dyn crate::scales::PriceFormatter>,
    quantity: Option<(f64, Option<String>)>,
    id: egui::Id,
) -> (Option<QuoteSide>, Option<Rect>) {
    let text = |price: f64| {
        formatter.map_or_else(
            || crate::scales::group_thousands(format!("{price:.precision$}"), thousands_sep),
            |formatter| formatter.format(price),
        )
    };
    let (bid, ask, spread, sell_name, buy_name) = match prices {
        QuotePrices::BidAsk { bid, ask } => {
            (bid, ask, text(ask - bid), "Sell at bid", "Buy at ask")
        }
        QuotePrices::Last(price) => (
            price,
            price,
            String::new(),
            "Sell at market",
            "Buy at market",
        ),
    };

    let theme = ui.active_theme();
    let spread_galley = layout.painter.layout_no_wrap(
        spread,
        FontId::proportional(theme.text.tiny),
        layout.colors.text,
    );
    let quantity = quantity.map(|(quantity, text)| {
        layout.painter.layout_no_wrap(
            text.unwrap_or_else(|| compact_quantity(quantity)),
            FontId::proportional(theme.text.small),
            layout.colors.text,
        )
    });
    let gap = (spread_galley.size().x + 12.0)
        .max(36.0)
        .max(quantity.as_ref().map_or(0.0, |text| text.size().x + 18.0));
    let quote = |side: QuoteSide, price: f64, color: Color32| {
        let price = layout
            .painter
            .layout_job(egui::text::LayoutJob::single_section(
                text(price),
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
        &l10n::text(ui.ctx(), sell_name),
        id.with("sell"),
    );
    let spread_rect = Rect::from_min_size(
        Pos2::new(sell_rect.right(), sell_rect.top()),
        Vec2::new(gap, 34.0),
    );
    layout.painter.add(layout.fill(spread_rect, 1.0));
    let quantity = quantity.map(|text| {
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
        &l10n::text(ui.ctx(), buy_name),
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

/// What the user did with the quote panel this frame.
pub(crate) struct QuotePanelResponse {
    pub action: Option<QuoteSide>,
    /// The quantity box, when clicked.
    pub quantity: Option<Rect>,
    /// The new place while the grip is dragged.
    pub moved: Option<[f32; 2]>,
    /// The grip was released this frame.
    pub released: bool,
}

impl Legend<'_> {
    /// The shared Sell and Buy controls inside `room`, at shares of its remaining room, with a
    /// grip to move them. Dialogs and menus remain above the chart's own painting layer.
    pub(crate) fn show_quote_panel(
        &self,
        ui: &mut Ui,
        colors: &StyleColors,
        room: Rect,
        place: [f32; 2],
        prices: QuotePrices,
        id: egui::Id,
    ) -> QuotePanelResponse {
        let theme = ui.active_theme();
        let size = ui
            .data_mut(|data| data.get_temp::<Vec2>(id))
            .unwrap_or(Vec2::ZERO);
        let free = (room.size() - size).max(Vec2::ZERO);
        let [x, y] = place.map(|v| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        });
        let at = room.min + egui::vec2(x * free.x, y * free.y);
        let painter = ui
            .painter()
            .with_clip_rect(room.expand(8.0).intersect(ui.clip_rect()));
        let background = painter.add(egui::Shape::Noop);
        let grip = Rect::from_min_size(at + Vec2::splat(4.0), egui::vec2(14.0, 34.0));
        let response = ui
            .interact(
                grip.intersect(painter.clip_rect()),
                id.with("grip"),
                Sense::drag(),
            )
            .on_hover_cursor(egui::CursorIcon::Grab)
            .on_hover_text(l10n::text(ui.ctx(), "Move Buy and Sell"));
        let label = l10n::text(ui.ctx(), "Move Buy and Sell");
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        if response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        let dots = &crate::tokens::DESIGN_TOKENS.sizing.floating_toolbar;
        for col in 0..2 {
            for row in 0..3 {
                painter.circle_filled(
                    grip.center()
                        + egui::vec2(
                            (col as f32 - 0.5) * dots.dot_spacing,
                            (row as f32 - 1.0) * dots.dot_spacing,
                        ),
                    dots.dot_size,
                    theme.ui.text.gamma_multiply(0.4),
                );
            }
        }
        let content = egui::pos2(grip.right(), grip.top());
        let bounds = Rect::from_min_max(content, egui::pos2(f32::INFINITY, content.y + 34.0));
        let chart_background = |_: Pos2| theme.chart.bg;
        let mut layout = Layout::new(
            &painter,
            bounds,
            colors,
            f32::INFINITY,
            &chart_background,
            &[],
        );
        let (action, quantity) = paint_quotes(
            ui,
            &mut layout,
            prices,
            self.price_precision(prices.magnitude()),
            self.thousands_sep,
            self.formatters.price,
            self.quantity.clone(),
            id,
        );
        let measured = egui::vec2(layout.pos.x - 8.0 + 4.0 - at.x, 42.0);
        painter.set(
            background,
            egui::Shape::Rect(egui::epaint::RectShape::new(
                Rect::from_min_size(at, measured),
                theme.radius.md,
                theme.chart.bg,
                Stroke::new(1.0, theme.ui.border),
                egui::StrokeKind::Inside,
            )),
        );
        if size != measured {
            // shortcut: the first frame places by the top-left corner; measure first if visible.
            ui.data_mut(|data| data.insert_temp(id, measured));
            ui.ctx().request_repaint();
        }
        let moved = response.dragged().then(|| {
            let moved = at + response.drag_delta() - room.min;
            [
                if free.x > 0.0 {
                    (moved.x / free.x).clamp(0.0, 1.0)
                } else {
                    x
                },
                if free.y > 0.0 {
                    (moved.y / free.y).clamp(0.0, 1.0)
                } else {
                    y
                },
            ]
        });
        QuotePanelResponse {
            action,
            quantity,
            moved,
            released: response.drag_stopped(),
        }
    }
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
    let response = ui
        .interact(rect.intersect(painter.clip_rect()), id, Sense::click())
        .on_hover_text(name);
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
            // The title, the inputs, then the basis and the upper and lower band, without names.
            let row = |shapes: &[egui::epaint::ClippedShape]| {
                let text = |shape: &egui::epaint::ClippedShape| match &shape.shape {
                    Shape::Text(text) if text.pos.x < 600.0 => Some(text.clone()),
                    _ => None,
                };
                let title = shapes
                    .iter()
                    .filter_map(text)
                    .find(|text| text.galley.text() == "BB")
                    .unwrap();
                let mut row: Vec<_> = shapes
                    .iter()
                    .filter_map(text)
                    .filter(|text| text.pos.y == title.pos.y)
                    .collect();
                row.sort_by(|a, b| a.pos.x.total_cmp(&b.pos.x));
                row.iter()
                    .map(|text| text.galley.text().to_owned())
                    .collect::<Vec<_>>()
            };
            assert_eq!(row(&second), ["BB", "2 2", "100.00", "110.00", "90.00"]);
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
