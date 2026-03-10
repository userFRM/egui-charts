//! Realtime (Jump to Latest) button rendering

use crate::config::RealtimeButtonPos;
use crate::styles::typography;
use egui::{Color32, FontId, Painter, Pos2, Rect, Vec2};

/// Renders "Jump to Latest" button
pub fn render_realtime_btn(
    painter: &Painter,
    price_rect: Rect,
    near_live_edge: bool,
    show_btn: bool,
    btn_size: (f32, f32),
    position: RealtimeButtonPos,
    btn_color: Color32,
    hover_color: Color32,
    text_color: Color32,
    btn_text: Option<&str>,
    is_hovered: bool,
) {
    if near_live_edge || !show_btn {
        return;
    }

    let (width, height) = btn_size;
    let btn_size_vec = Vec2::new(width, height);
    let margin = 10.0;

    let btn_pos = calculate_button_position(price_rect, btn_size_vec, margin, position);
    let btn_rect = Rect::from_min_size(btn_pos, btn_size_vec);

    let bg_color = if is_hovered { hover_color } else { btn_color };

    painter.rect_filled(btn_rect, 4.0, bg_color);

    let text = btn_text.unwrap_or("Go to Realtime");
    painter.text(
        btn_rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(typography::SM_MD),
        text_color,
    );
}

fn calculate_button_position(
    price_rect: Rect,
    btn_size: Vec2,
    margin: f32,
    position: RealtimeButtonPos,
) -> Pos2 {
    match position {
        RealtimeButtonPos::TopLeft => {
            Pos2::new(price_rect.min.x + margin, price_rect.min.y + margin)
        }
        RealtimeButtonPos::TopCenter => Pos2::new(
            price_rect.center().x - btn_size.x / 2.0,
            price_rect.min.y + margin,
        ),
        RealtimeButtonPos::TopRight => Pos2::new(
            price_rect.max.x - btn_size.x - margin,
            price_rect.min.y + margin,
        ),
        RealtimeButtonPos::BottomLeft => Pos2::new(
            price_rect.min.x + margin,
            price_rect.max.y - btn_size.y - margin,
        ),
        RealtimeButtonPos::BottomCenter => Pos2::new(
            price_rect.center().x - btn_size.x / 2.0,
            price_rect.max.y - btn_size.y - margin,
        ),
        RealtimeButtonPos::BottomRight => Pos2::new(
            price_rect.max.x - btn_size.x - margin,
            price_rect.max.y - btn_size.y - margin,
        ),
    }
}
