use super::{
    Button, ExitButtons, HEIGHT, LABEL_INSET, Level, NODE_INSET, PADDING, PnlUnit,
    TradingLabelAlignment, TradingLineAction, TradingLineOptions, direction, exit_buttons_width,
    label_fill, offered_exits, paint_glyph, pnl_text,
};
use crate::chart::coords::ChartMapping;
use crate::chart::rendering::render_price_line;
use crate::localization as l10n;
use crate::model::{
    LineStyle, OrderRequest, OrderSide, OrderType, Position, PositionBracket, PriceLine,
};
use crate::theme::ThemeExt;
use egui::{Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};

/// Draws a position's line at its average price and its label at the right of the plot: from
/// left to right a reverse button while the label is hovered, TP and SL buttons, the quantity,
/// the profit or loss and a close button. Dragging TP or SL out of the label shows the exit at
/// the pointer's price; releasing it asks for that exit order. Returns what the user asked for.
pub(super) fn show_position(
    ui: &mut Ui,
    mapping: &ChartMapping,
    options: &TradingLineOptions<'_>,
    position: &Position,
    tags: &mut Vec<(f64, Color32)>,
) -> Option<TradingLineAction> {
    let theme = ui.ctx().active_theme();
    let side = if position.quantity > 0.0 {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    let color = crate::theme::trading_side_color(&theme, side);
    let size = position.quantity.abs();
    let entry = position.average_price;
    let y = mapping.price_to_y(entry);
    if !y.is_finite() || !mapping.is_y_visible(y) {
        return None;
    }
    let painter = ui.painter().with_clip_rect(mapping.rect);
    let line = PriceLine::new(entry, color).with_style(LineStyle::Solid);
    tags.push((entry, color));

    let key = ui.scope_id().with(("trading_position", &position.symbol));
    let font = FontId::proportional(ui.ctx().active_theme().text.ohlc);
    let layout = |text: String, color| painter.layout_no_wrap(text, font.clone(), color);
    let width = |galley: &egui::Galley| galley.size().x + 2.0 * PADDING;
    // The result the position makes when the price moves by `gain` in its favour.
    let result = |unit, gain: f64| {
        pnl_text(
            ui.ctx(),
            unit,
            gain * size * options.point_value,
            gain,
            entry,
            options,
        )
    };
    let pnl = options.pnl.and_then(|unit| {
        let gain = options
            .last_price
            .map(|last| (last - entry) * direction(side))
            .or_else(|| {
                position
                    .unrealized_pnl
                    .map(|pnl| pnl / (size * options.point_value))
            })?;
        let (text, value) = match (unit, position.unrealized_pnl) {
            // The broker's own figure, when it reports one.
            (PnlUnit::Money, Some(pnl)) => pnl_text(ui.ctx(), unit, pnl, gain, entry, options),
            _ => result(unit, gain),
        };
        let text_color = if value > 0.0 {
            theme.ui.take_profit
        } else if value < 0.0 {
            theme.ui.error
        } else {
            theme.ui.text_secondary
        };
        Some((layout(text, text_color), text_color))
    });
    let exits = offered_exits(options.take_profit, options.stop_loss);
    let quantity = layout(format!("{size}"), Color32::WHITE);
    let close = if options.close { HEIGHT } else { 0.0 };
    let total = exit_buttons_width(ui, exits)
        + width(&quantity)
        + pnl.as_ref().map_or(0.0, |(galley, _)| width(galley))
        + close;
    let reverse = options.close && options.reverse;
    let label_width = total + if reverse { HEIGHT * 1.6 } else { 0.0 };
    let inset = LABEL_INSET.min((mapping.rect.width() - total - HEIGHT * 1.6).max(8.0));
    let (left, right) = if options.label_alignment == TradingLabelAlignment::Right
        || label_width > mapping.rect.width() - 16.0
    {
        let right = mapping.rect.right() - inset;
        // Preserve the rounding at the edge where the reverse button moves to another row.
        let left = right - total - if reverse { HEIGHT * 1.6 } else { 0.0 };
        (left, right)
    } else {
        let left = options
            .label_alignment
            .left(mapping.rect, label_width, inset);
        (left, left + label_width)
    };
    let label = Rect::from_min_max(pos2(left, y - HEIGHT / 2.0), pos2(right, y + HEIGHT / 2.0));
    render_price_line(
        &painter,
        mapping.rect,
        &mapping.price,
        &line,
        if options.extend_lines {
            mapping.rect.left()
        } else {
            label.left()
        },
        false,
        f32::INFINITY,
    );
    let cell = |x: f32, w: f32| Rect::from_min_size(pos2(x, label.top()), vec2(w, HEIGHT));
    let reverse_rect = if left < mapping.rect.left() {
        let top = if label.bottom() + 4.0 + HEIGHT <= mapping.rect.bottom() {
            label.bottom() + 4.0
        } else {
            label.top() - 4.0 - HEIGHT
        };
        Rect::from_min_size(pos2(right - total, top), vec2(HEIGHT, HEIGHT))
    } else {
        cell(left, HEIGHT)
    };
    let hover_rect = if reverse {
        label.union(reverse_rect)
    } else {
        label
    };
    let hovered = ui.rect_contains_pointer(hover_rect.intersect(mapping.rect));
    let stroke = Stroke::new(1.0, color);
    let mut action = None;

    if reverse && hovered {
        let rect = reverse_rect;
        let response = ui
            .interact(
                rect.intersect(mapping.rect),
                key.with("reverse"),
                Sense::click(),
            )
            .on_hover_text(l10n::text(ui.ctx(), "Reverse position"));
        painter.rect(
            rect,
            4.0,
            label_fill(&theme, color),
            stroke,
            egui::StrokeKind::Inside,
        );
        if response.hovered() {
            painter.rect_filled(rect.shrink(1.0), 1.0, color.gamma_multiply(0.2));
        }
        paint_glyph(&painter, Button::Reverse, rect.center(), color);
        if response.clicked() {
            action = Some(TradingLineAction::Reverse(position.clone()));
        }
    }

    let (mut x, released) = ExitButtons {
        key,
        exits,
        x: right - total,
        y,
        entry,
        line_left: if options.extend_lines {
            mapping.rect.left()
        } else {
            label.left()
        },
        preview_right: right,
        pnl: &|unit, price| result(unit, (price - entry) * direction(side)).0,
    }
    .show(ui, mapping, options, tags);
    if let Some((exit, price)) = released {
        let mut request = OrderRequest::new(
            position.symbol.clone(),
            match side {
                OrderSide::Buy => OrderSide::Sell,
                OrderSide::Sell => OrderSide::Buy,
            },
            size,
            if exit == Level::TakeProfit {
                OrderType::Limit(price)
            } else {
                OrderType::Stop(price)
            },
        );
        request.position_bracket = Some(if exit == Level::TakeProfit {
            PositionBracket::TakeProfit
        } else {
            PositionBracket::StopLoss
        });
        action = action.or(Some(TradingLineAction::Exit(position.clone(), request)));
    }

    if !exits.is_empty() {
        x += HEIGHT * 0.65;
    }
    let group_rect = Rect::from_min_max(pos2(x, label.top()), pos2(right, label.bottom()));
    painter.rect(
        group_rect,
        4.0,
        label_fill(&theme, color),
        stroke,
        egui::StrokeKind::Inside,
    );
    painter.circle_filled(pos2(mapping.rect.right() - NODE_INSET, y), 3.0, color);
    let rect = cell(x, width(&quantity));
    x = rect.right();
    painter.rect_filled(
        rect,
        egui::CornerRadius {
            nw: 4,
            sw: 4,
            ne: 0,
            se: 0,
        },
        color,
    );
    painter.galley(
        pos2(rect.left() + PADDING, y - quantity.size().y / 2.0),
        quantity,
        Color32::WHITE,
    );
    if let Some((pnl, text_color)) = pnl {
        let rect = cell(x, width(&pnl));
        x = rect.right();
        painter.vline(rect.left(), rect.y_range(), stroke);
        painter.galley(
            pos2(rect.left() + PADDING, y - pnl.size().y / 2.0),
            pnl,
            text_color,
        );
    }
    if options.close {
        let rect = cell(x, HEIGHT);
        let response = ui
            .interact(
                rect.intersect(mapping.rect),
                key.with("close"),
                Sense::click(),
            )
            .on_hover_text(l10n::text(ui.ctx(), "Close position"));
        painter.vline(rect.left(), rect.y_range(), stroke);
        if response.hovered() {
            painter.rect_filled(rect.shrink(1.0), 1.0, color.gamma_multiply(0.2));
        }
        paint_glyph(&painter, Button::Cancel, rect.center(), color);
        if response.clicked() {
            action = action.or(Some(TradingLineAction::Close(position.clone())));
        }
    }
    action
}
