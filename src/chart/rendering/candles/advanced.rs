//! Advanced chart placeholders
//!
//! These chart types require tick/order flow data for full implementation.
//! Current implementations are approximations from OHLCV data.
//!
//! Includes: Volume Footprint, TPO (Market Profile), Session Volume

use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

use super::helpers::PriceCoords;
use crate::chart::renderers::PriceScale;
use crate::model::Bar;

const SESSION_SIZE: usize = 10;

/// Volume Footprint placeholder - shows volume profile approximation from OHLCV
///
/// # Note
/// Renders a simplified stand-in visual. A full implementation requires
/// tick-level or order-flow data; this approximation uses OHLCV bars only.
pub(super) fn render_volume_footprint_placeholder(
    painter: &Painter,
    price_rect: Rect,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    price_scale: &PriceScale,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let coords = PriceCoords::new(price_scale.min_price, price_scale.max_price, price_rect);

    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        draw_footprint_bar(
            painter,
            x,
            bar,
            &coords,
            bar_width,
            bullish_color,
            bearish_color,
        );
    }
}

/// TPO (Time Price Opportunity) placeholder - Market Profile approximation
///
/// # Note
/// Renders a simplified stand-in visual. True TPO profiles require intraday
/// tick data; this approximation distributes letters across the OHLCV range.
pub(super) fn render_tpo_placeholder(
    painter: &Painter,
    price_rect: Rect,
    visible_data: &[Bar],
    start_idx: usize,
    _bar_width: f32,
    price_scale: &PriceScale,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    color: Color32,
) {
    let coords = PriceCoords::new(price_scale.min_price, price_scale.max_price, price_rect);
    let letters = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L'];

    for (i, bar) in visible_data.iter().enumerate() {
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);
        draw_tpo_bar(painter, x, bar, &coords, letters[i % letters.len()], color);
    }
}

/// Session Volume placeholder - shows volume aggregated by session
///
/// # Note
/// Renders a simplified stand-in visual. A full implementation would use
/// exchange session boundaries; this approximation groups bars in fixed-size
/// chunks.
pub(super) fn render_session_volume_placeholder(
    painter: &Painter,
    price_rect: Rect,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    price_scale: &PriceScale,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let coords = PriceCoords::new(price_scale.min_price, price_scale.max_price, price_rect);
    let sessions: Vec<_> = visible_data.chunks(SESSION_SIZE).collect();

    for (session_idx, session_bars) in sessions.iter().enumerate() {
        if session_bars.is_empty() {
            continue;
        }

        let session_start = start_idx + session_idx * SESSION_SIZE;
        draw_session_profile(
            painter,
            session_bars,
            session_start,
            &coords,
            bar_width,
            chart_rect_min_x,
            &idx_to_coord,
            bullish_color,
            bearish_color,
        );
    }
}

// === Helper Functions ===

fn draw_footprint_bar(
    painter: &Painter,
    x: f32,
    bar: &Bar,
    coords: &PriceCoords,
    bar_width: f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let y_high = coords.price_to_y(bar.high);
    let y_low = coords.price_to_y(bar.low);
    let y_mid = (y_high + y_low) / 2.0;

    let is_bullish = bar.close > bar.open;
    let color = if is_bullish {
        bullish_color
    } else {
        bearish_color
    };
    let half_width = bar_width * 0.4;

    // Upper half - buyer volume approximation
    let upper_rect = Rect::from_min_max(
        Pos2::new(x - half_width, y_high),
        Pos2::new(x + half_width, y_mid),
    );
    painter.add(Shape::rect_filled(
        upper_rect,
        0.0,
        bullish_color.gamma_multiply(0.6),
    ));

    // Lower half - seller volume approximation
    let lower_rect = Rect::from_min_max(
        Pos2::new(x - half_width, y_mid),
        Pos2::new(x + half_width, y_low),
    );
    painter.add(Shape::rect_filled(
        lower_rect,
        0.0,
        bearish_color.gamma_multiply(0.6),
    ));

    // POC line (Point of Control - price with most volume)
    let poc_y = if is_bullish {
        y_mid - (y_mid - y_high) * 0.3
    } else {
        y_mid + (y_low - y_mid) * 0.3
    };
    painter.add(Shape::line_segment(
        [
            Pos2::new(x - half_width, poc_y),
            Pos2::new(x + half_width, poc_y),
        ],
        Stroke::new(DESIGN_TOKENS.stroke.thick, color),
    ));
}

fn draw_tpo_bar(
    painter: &Painter,
    x: f32,
    bar: &Bar,
    coords: &PriceCoords,
    letter: char,
    color: Color32,
) {
    let y_high = coords.price_to_y(bar.high);
    let y_low = coords.price_to_y(bar.low);
    let block_height = ((y_low - y_high) / 4.0).max(10.0);

    let mut y = y_high;
    while y < y_low {
        painter.text(
            Pos2::new(x, y + block_height / 2.0),
            egui::Align2::CENTER_CENTER,
            letter.to_string(),
            egui::FontId::monospace(10.0),
            color.gamma_multiply(0.8),
        );
        y += block_height;
    }
}

fn draw_session_profile(
    painter: &Painter,
    session_bars: &[Bar],
    session_start: usize,
    coords: &PriceCoords,
    bar_width: f32,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let first_x = idx_to_coord(session_start, chart_rect_min_x);
    let last_x = idx_to_coord(session_start + session_bars.len() - 1, chart_rect_min_x);
    let session_width = (last_x - first_x).max(bar_width * 2.0);

    let session_high = session_bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let session_low = session_bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    let total_volume: f64 = session_bars.iter().map(|b| b.volume).sum();

    let y_high = coords.price_to_y(session_high);
    let y_low = coords.price_to_y(session_low);

    let is_bullish = session_bars
        .last()
        .map(|b| b.close > b.open)
        .unwrap_or(true);
    let color = if is_bullish {
        bullish_color
    } else {
        bearish_color
    };

    let profile_width = (total_volume / 1e6).min((session_width * 0.8) as f64) as f32;
    let profile_rect = Rect::from_min_max(
        Pos2::new(first_x, y_high),
        Pos2::new(first_x + profile_width.max(5.0), y_low),
    );

    painter.add(Shape::rect_filled(
        profile_rect,
        2.0,
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 60),
    ));
    painter.add(Shape::rect_stroke(
        profile_rect,
        2.0,
        Stroke::new(DESIGN_TOKENS.stroke.hairline, color.gamma_multiply(0.8)),
        egui::epaint::StrokeKind::Inside,
    ));
}
