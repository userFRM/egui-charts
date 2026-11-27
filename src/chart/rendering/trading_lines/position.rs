use super::{
    Button, COVERAGE_CELL, ExitButtons, HEIGHT, LABEL_INSET, Level, NODE_INSET, PADDING, PnlUnit,
    TradingLabelAlignment, TradingLineAction, TradingLineOptions, coverage_text, direction,
    exit_buttons_width, label_fill, line_left, line_width, menu_entry, offered_exits,
    paint_coverage, paint_glyph, pnl_text, quantity_text,
};
use crate::chart::coords::ChartMapping;
use crate::chart::rendering::render_price_line;
use crate::localization as l10n;
use crate::model::{
    Capabilities, LineStyle, OrderRequest, OrderSide, OrderType, Position, PositionBracket,
    PriceLine,
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
    held: (bool, bool),
    coverage: Option<(f64, f64)>,
    tags: &mut Vec<(f64, Color32)>,
) -> Option<TradingLineAction> {
    let theme = ui.ctx().active_theme();
    let side = if position.quantity > 0.0 {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    let look = options
        .position_appearance
        .iter()
        .find(|(symbol, _)| symbol == &position.symbol)
        .map(|(_, look)| look);
    let color = look
        .and_then(|look| look.color)
        .unwrap_or_else(|| crate::theme::trading_side_color(&theme, side));
    let label_color = look.and_then(|look| look.label_color).unwrap_or(color);
    let quantity_color = look.and_then(|look| look.quantity_color).unwrap_or(color);
    let button_color = look.and_then(|look| look.button_color).unwrap_or(color);
    let close = look.and_then(|look| look.close).unwrap_or(options.close);
    let size = position.quantity.abs();
    let entry = position.average_price;
    let y = mapping.price_to_y(entry);
    if !y.is_finite() || !mapping.is_y_visible(y) {
        return None;
    }
    let painter = ui.painter().with_clip_rect(mapping.rect);
    let width = line_width(
        look.and_then(|look| look.line_width)
            .unwrap_or(options.line_width),
    );
    let line = PriceLine::new(entry, color).with_width(width).with_style(
        look.and_then(|look| look.line_style)
            .unwrap_or(LineStyle::Solid),
    );
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
            direction(side),
            options,
        )
    };
    let exits = offered_exits(options.take_profit, options.stop_loss);
    let quantity = layout(
        quantity_text(options.formatter, &position.symbol, size),
        Color32::WHITE,
    );
    let text = look
        .and_then(|look| look.text.as_ref())
        .map(|text| layout(text.clone(), label_color));
    let close_width = if close { HEIGHT } else { 0.0 };
    let fixed_width = exit_buttons_width(ui, exits)
        + width(&quantity)
        + coverage.map_or(0.0, |_| COVERAGE_CELL)
        + text.as_deref().map_or(0.0, width)
        + close_width;
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
            (PnlUnit::Money, Some(pnl)) => {
                pnl_text(ui.ctx(), unit, pnl, gain, entry, direction(side), options)
            }
            _ => result(unit, gain),
        };
        let text_color = if value > 0.0 {
            theme.ui.take_profit
        } else if value < 0.0 {
            theme.ui.error
        } else {
            theme.ui.text_secondary
        };
        let galley = if coverage.is_some() {
            // Keep the exit and close controls reachable when the coverage cell narrows the text.
            let mut job = egui::text::LayoutJob::simple_singleline(text, font.clone(), text_color);
            job.wrap.max_width =
                (mapping.rect.width() - 16.0 - fixed_width - 2.0 * PADDING).max(0.0);
            job.wrap.max_rows = 1;
            painter.layout_job(job)
        } else {
            layout(text, text_color)
        };
        Some((galley, text_color))
    });
    let total = fixed_width + pnl.as_ref().map_or(0.0, |(galley, _)| width(galley));
    let reverse = close && options.reverse;
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
    let line_left = line_left(
        mapping.rect,
        label.left(),
        options.extend_lines,
        look.and_then(|look| look.line_length)
            .unwrap_or(options.line_length),
    );
    render_price_line(
        &painter,
        mapping.rect,
        &mapping.price,
        &line,
        line_left,
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
        Rect::from_min_size(
            pos2((right - total).max(mapping.rect.left()), top),
            vec2(HEIGHT, HEIGHT),
        )
    } else {
        cell(left, HEIGHT)
    };
    let hover_rect = if reverse {
        label.union(reverse_rect)
    } else {
        label
    };
    let hovered = ui.rect_contains_pointer(hover_rect.intersect(mapping.rect));
    let stroke = Stroke::new(1.0, label_color);
    let mut action = None;
    if options.menu.is_some() || look.and_then(|look| look.hover_text.as_ref()).is_some() {
        let response = ui.interact(
            label.intersect(mapping.rect),
            key.with("menu"),
            if options.menu.is_some() {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let response = if let Some(text) = look.and_then(|look| look.hover_text.as_ref()) {
            response.on_hover_text(text)
        } else {
            response
        };
        if let Some(capabilities) = options.menu {
            let closable = look.and_then(|look| look.close) != Some(false);
            response.context_menu(|ui| {
                action = position_menu(ui, position, capabilities, held, closable);
            });
        }
    }

    if reverse && hovered {
        let rect = reverse_rect;
        let response = ui
            .interact(
                rect.intersect(mapping.rect),
                key.with("reverse"),
                Sense::click(),
            )
            .on_hover_text(
                look.and_then(|look| look.reverse_hover_text.clone())
                    .unwrap_or_else(|| l10n::text(ui.ctx(), "Reverse position")),
            );
        painter.rect(
            rect,
            4.0,
            label_fill(&theme, button_color),
            Stroke::new(1.0, button_color),
            egui::StrokeKind::Inside,
        );
        if response.hovered() {
            painter.rect_filled(rect.shrink(1.0), 1.0, button_color.gamma_multiply(0.2));
        }
        paint_glyph(&painter, Button::Reverse, rect.center(), button_color);
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
        line_left,
        preview_right: right,
        pnl: &|unit, price| result(unit, (price - entry) * direction(side)).0,
        take_profit_hover_text: look.and_then(|look| look.take_profit_hover_text.as_deref()),
        stop_loss_hover_text: look.and_then(|look| look.stop_loss_hover_text.as_deref()),
    }
    .show(ui, mapping, options, tags);
    if let Some((exit, price)) = released {
        let request = exit_request(
            position,
            if exit == Level::TakeProfit {
                PositionBracket::TakeProfit
            } else {
                PositionBracket::StopLoss
            },
            price,
        );
        action = action.or(Some(TradingLineAction::Exit(position.clone(), request)));
    }

    if !exits.is_empty() {
        x += HEIGHT * 0.65;
    }
    let group_rect = Rect::from_min_max(pos2(x, label.top()), pos2(right, label.bottom()));
    painter.rect(
        group_rect,
        4.0,
        label_fill(&theme, label_color),
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
        quantity_color,
    );
    painter.galley(
        pos2(rect.left() + PADDING, y - quantity.size().y / 2.0),
        quantity,
        Color32::WHITE,
    );
    if let Some(coverage) = coverage {
        let rect = cell(x, COVERAGE_CELL);
        x = rect.right();
        painter.vline(rect.left(), rect.y_range(), stroke);
        paint_coverage(&painter, &theme, rect.center(), coverage);
        ui.interact(
            rect.intersect(mapping.rect),
            key.with("coverage"),
            Sense::hover(),
        )
        .on_hover_text(coverage_text(ui.ctx(), coverage));
    }
    if let Some(text) = text {
        let rect = cell(x, width(&text));
        x = rect.right();
        painter.vline(rect.left(), rect.y_range(), stroke);
        painter.galley(
            pos2(rect.left() + PADDING, y - text.size().y / 2.0),
            text,
            label_color,
        );
    }
    if let Some((pnl, text_color)) = pnl {
        let rect = cell(x, width(&pnl));
        x = rect.right();
        painter.vline(rect.left(), rect.y_range(), stroke);
        if pnl.elided {
            ui.interact(
                rect.intersect(mapping.rect),
                key.with("pnl"),
                Sense::hover(),
            )
            .on_hover_text(pnl.job.text.clone());
        }
        painter.galley(
            pos2(rect.left() + PADDING, y - pnl.size().y / 2.0),
            pnl,
            text_color,
        );
    }
    if close {
        let rect = cell(x, HEIGHT);
        let response = ui
            .interact(
                rect.intersect(mapping.rect),
                key.with("close"),
                Sense::click(),
            )
            .on_hover_text(
                look.and_then(|look| look.close_hover_text.clone())
                    .unwrap_or_else(|| l10n::text(ui.ctx(), "Close position")),
            );
        painter.vline(rect.left(), rect.y_range(), stroke);
        if response.hovered() {
            painter.rect_filled(rect.shrink(1.0), 1.0, button_color.gamma_multiply(0.2));
        }
        paint_glyph(&painter, Button::Cancel, rect.center(), button_color);
        if response.clicked() {
            action = action.or(Some(TradingLineAction::Close(position.clone())));
        }
    }
    action
}

/// An exit on the other side for the position's full size, with its bracket role retained.
pub(crate) fn exit_request(
    position: &Position,
    bracket: PositionBracket,
    price: f64,
) -> OrderRequest {
    let mut request = OrderRequest::new(
        &position.symbol,
        if position.quantity > 0.0 {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        },
        position.quantity.abs(),
        match bracket {
            PositionBracket::TakeProfit => OrderType::Limit(price),
            PositionBracket::StopLoss => OrderType::Stop(price),
        },
    );
    request.position_bracket = Some(bracket);
    request
}

fn position_menu(
    ui: &mut Ui,
    position: &Position,
    capabilities: Capabilities,
    held: (bool, bool),
    closable: bool,
) -> Option<TradingLineAction> {
    const FIXED: &str = "The position cannot be closed or reversed on the chart";
    if menu_entry(
        ui,
        "Close position",
        capabilities.close && closable,
        if capabilities.close {
            FIXED
        } else {
            "The account does not support closing positions"
        },
    ) {
        return Some(TradingLineAction::Close(position.clone()));
    }
    if menu_entry(
        ui,
        "Reverse position",
        capabilities.reverse && closable,
        if capabilities.reverse {
            FIXED
        } else {
            "The account does not support reversing positions"
        },
    ) {
        return Some(TradingLineAction::Reverse(position.clone()));
    }
    let offers = capabilities.position_exits
        && ((capabilities.limit && !held.0) || (capabilities.stop && !held.1));
    let why = if !capabilities.limit && !capabilities.stop {
        "The account does not support take-profit or stop-loss orders"
    } else if !capabilities.position_exits {
        "The account does not support exits on positions"
    } else {
        "The position has every exit the account supports"
    };
    if menu_entry(ui, "Protect position", offers, why) {
        return Some(TradingLineAction::Protect(position.clone()));
    }
    None
}
