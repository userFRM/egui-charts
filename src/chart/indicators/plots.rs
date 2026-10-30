//! Mapped study geometry shared by painting and hit testing.

use crate::studies::{StudyCandle, StudyZone};

pub(crate) fn zone_rect(
    zone: &StudyZone,
    coords: &crate::chart::coords::ChartMapping,
    price_to_y: impl Fn(f64) -> f32,
) -> Option<egui::Rect> {
    let last = zone
        .bars
        .end
        .checked_sub(1)
        .filter(|&l| l >= zone.bars.start)?;
    let rect = egui::Rect::from_two_pos(
        egui::pos2(coords.idx_to_x(zone.bars.start), price_to_y(zone.top)),
        egui::pos2(coords.idx_to_x(last), price_to_y(zone.bottom)),
    );
    (rect.min.is_finite() && rect.max.is_finite()).then_some(rect)
}

pub(crate) fn candle_geometry(
    candle: &StudyCandle,
    coords: &crate::chart::coords::ChartMapping,
    price_to_y: impl Fn(f64) -> f32,
) -> Option<(egui::Rect, [egui::Pos2; 2])> {
    use egui::{Pos2, Rect};
    if candle.bars.is_empty() {
        return None;
    }
    let half = coords.bar_spacing * 0.5;
    let inset = (coords.bar_spacing * 0.1).min(2.0);
    let left = coords.idx_to_x(candle.bars.start) - half + inset;
    let right = coords.idx_to_x(candle.bars.end - 1) + half - inset;
    let x = (left + right) * 0.5;
    let (open, close) = (price_to_y(candle.bar.open), price_to_y(candle.bar.close));
    let wick = [
        Pos2::new(x, price_to_y(candle.bar.high)),
        Pos2::new(x, price_to_y(candle.bar.low)),
    ];
    if ![left, right, open, close, wick[0].y, wick[1].y]
        .iter()
        .all(|v| v.is_finite())
    {
        return None;
    }
    let top = open.min(close);
    let body = Rect::from_min_max(
        Pos2::new(left, top),
        Pos2::new(right, open.max(close).max(top + 1.0)),
    );
    Some((body, wick))
}

pub(crate) fn paint_candles(
    context: &crate::chart::renderers::RenderContext,
    study: &dyn crate::studies::Indicator,
    price: &crate::scales::PriceMap,
    coords: &crate::chart::coords::ChartMapping,
) {
    let painter = context.painter.with_clip_rect(context.rect);
    let candles = study.candles();
    let start = candles
        .partition_point(|c| c.bars.end <= coords.x_to_idx(context.rect.left()).saturating_sub(1));
    for candle in &candles[start..] {
        if coords.idx_to_x(candle.bars.start) - coords.bar_spacing * 0.5 > context.rect.right() {
            break;
        }
        let style = study.plot_style(candle.plot);
        if !style.visible {
            continue;
        }
        if let Some((body, wick)) = candle_geometry(candle, coords, |p| price.price_to_y(p)) {
            let stroke = egui::Stroke::new(style.width, style.color);
            painter.line_segment(wick, stroke);
            painter.rect(
                body,
                0.0,
                style.color.gamma_multiply_u8(77),
                stroke,
                egui::StrokeKind::Inside,
            );
        }
    }
}

pub(crate) fn paint_zones(
    painter: &egui::Painter,
    study: &dyn crate::studies::Indicator,
    price: &crate::scales::PriceMap,
    coords: &crate::chart::coords::ChartMapping,
) {
    let clip = painter.clip_rect();
    for zone in study.zones() {
        let style = study.plot_style(zone.plot);
        let Some(rect) = zone_rect(zone, coords, |p| price.price_to_y(p))
            .filter(|rect| style.visible && rect.intersects(clip.expand(style.width)))
        else {
            continue;
        };
        let stroke = egui::Stroke::new(style.width, style.color);
        painter.rect_filled(rect, 0.0, style.color.gamma_multiply(0.15));
        painter.line_segment([rect.left_top(), rect.right_top()], stroke);
        painter.line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
    }
}

/// Anchor and level prices of drawings reaching the visible bars, for all study scales.
#[cfg(feature = "drawings")]
pub(crate) fn mapped_drawings(
    study: &dyn crate::studies::Indicator,
    coords: &crate::chart::coords::ChartMapping,
    price_to_y: impl Fn(f64) -> f32,
) -> Vec<crate::drawings::Drawing> {
    let style = study.plot_style(0);
    if !style.visible {
        return Vec::new();
    }
    study
        .drawings()
        .iter()
        .filter(|drawing| drawing.visible)
        .map(|drawing| {
            let mut drawing = drawing.clone();
            drawing.color = style.color.to_srgba_unmultiplied();
            drawing.stroke_width = style.width;
            drawing.line_style = style.pattern;
            drawing.update_screen_coords(
                |bar: f32| {
                    let index = bar as usize;
                    coords.idx_to_x(index) + (bar - index as f32) * coords.bar_spacing
                },
                &price_to_y,
            );
            drawing
        })
        .collect()
}

#[cfg(feature = "drawings")]
pub(crate) fn paint_drawings(
    context: &crate::chart::renderers::RenderContext,
    study: &dyn crate::studies::Indicator,
    price: &crate::scales::PriceMap,
    coords: &crate::chart::coords::ChartMapping,
) {
    let bar_to_x = |bar: f32| {
        let index = bar as usize;
        coords.idx_to_x(index) + (bar - index as f32) * coords.bar_spacing
    };
    let price_to_y = |p| price.price_to_y(p);
    let view = crate::drawings::rendering::ChartView {
        instrument: None,
        bars: &[],
        revision: 0,
        bar_to_x: &bar_to_x,
        price_to_y: &price_to_y,
        y_to_price: &|y| price.y_to_price(y),
        time_axis: false,
        executions: None,
    };
    for drawing in mapped_drawings(study, coords, price_to_y) {
        drawing.render(
            &context.painter.with_clip_rect(context.rect),
            context.rect,
            &view,
        );
    }
}
