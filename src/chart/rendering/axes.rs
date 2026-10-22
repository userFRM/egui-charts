//! Price and time axis rendering

use crate::drawings::rendering::utils::draw_dashed_line;
use crate::scales::PriceMap;
use crate::theme::{ThemeExt, contrast};
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};

/// Gap between the plot's right edge and the start of the price axis labels.
pub(crate) const PRICE_LABEL_GAP: f32 = 8.0;

/// Gap between the plot's bottom edge and the top of the time axis labels.
pub(crate) const TIME_LABEL_GAP: f32 = 5.0;

/// Padding between an axis tag's text and its edge.
const TAG_PADDING: Vec2 = Vec2::new(4.0, 2.0);

/// The font of every axis label and tag: the theme's small text.
pub(crate) fn axis_font(ctx: &egui::Context) -> FontId {
    FontId::proportional(ctx.active_theme().text.small)
}

/// Paints `text` on a `fill` tag in the axis font and returns the tag's rect.
///
/// The text sits where an axis label placed at `anchor` with `align` would, and the tag pads it.
/// A tag that would end past `max_x` moves left to end there, and one that overlaps `covered`, a
/// tag painted before it, widens to hide it across its width.
pub(crate) fn paint_axis_tag(
    painter: &Painter,
    anchor: Pos2,
    align: Align2,
    max_x: f32,
    covered: Option<Rect>,
    text: String,
    text_color: Color32,
    fill: Color32,
) -> Rect {
    let theme = painter.ctx().active_theme();
    let font = FontId::proportional(theme.text.small);
    let galley = painter.layout_no_wrap(text, font, text_color);
    let mut tag = align
        .anchor_size(anchor, galley.size())
        .expand2(TAG_PADDING);
    tag = tag.translate(Vec2::new((max_x - tag.max.x).min(0.0), 0.0));
    let text_pos = tag.min + TAG_PADDING;
    if let Some(covered) = covered.filter(|covered| covered.intersects(tag)) {
        tag.min.x = tag.min.x.min(covered.min.x);
        tag.max.x = tag.max.x.max(covered.max.x);
    }
    painter.rect_filled(tag, theme.radius.xs, fill);
    painter.galley(text_pos, galley, text_color);
    tag
}

/// Renders the last price line and, with `show_right_axis`, its tag in the price axis, which ends
/// at `axis_right`. Returns the tag's rect.
pub fn render_last_price_line(
    painter: &Painter,
    price_rect: Rect,
    price_map: &PriceMap,
    last_close: f64,
    last_open: f64,
    bullish_color: Color32,
    bearish_color: Color32,
    show_right_axis: bool,
    axis_right: f32,
) -> Option<Rect> {
    let y = price_map.price_to_y(last_close);
    // The newest bar can be scrolled or zoomed off the scale.
    if !(price_rect.min.y..=price_rect.max.y).contains(&y) {
        return None;
    }

    let price_color = if last_close >= last_open {
        bullish_color
    } else {
        bearish_color
    };

    // Draw dashed horizontal line at last price
    draw_dashed_line(
        painter,
        Pos2::new(price_rect.min.x, y),
        Pos2::new(price_rect.max.x, y),
        Stroke::new(DESIGN_TOKENS.stroke.hairline, price_color),
        4.0,
        3.0,
    );

    if !show_right_axis {
        return None;
    }
    let transform = price_map.transform();
    let label = transform.label(transform.to_scale(last_close), 2);
    // Black or white, whichever reads better on the candle colour.
    let text_color =
        if contrast(Color32::WHITE, price_color) >= contrast(Color32::BLACK, price_color) {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
    Some(paint_axis_tag(
        painter,
        Pos2::new(price_rect.max.x + PRICE_LABEL_GAP, y),
        Align2::LEFT_CENTER,
        axis_right,
        None,
        label,
        text_color,
        price_color,
    ))
}
