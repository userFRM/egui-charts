//! Mapped study geometry shared by painting and hit testing.

use crate::studies::{StudyCandle, StudyZone};

pub(crate) fn zone_rect(
    zone: &StudyZone,
    whole_bars: bool,
    coords: &crate::chart::coords::ChartMapping,
    price_to_y: impl Fn(f64) -> f32,
) -> Option<egui::Rect> {
    let last = zone
        .bars
        .end
        .checked_sub(1)
        .filter(|&l| l >= zone.bars.start)?;
    let half = if whole_bars {
        coords.bar_spacing * 0.5
    } else {
        0.0
    };
    let rect = egui::Rect::from_two_pos(
        egui::pos2(
            coords.idx_to_x(zone.bars.start) - half,
            price_to_y(zone.top),
        ),
        egui::pos2(coords.idx_to_x(last) + half, price_to_y(zone.bottom)),
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
        let whole_bars = study.zones_cover_whole_bars();
        let Some(rect) = zone_rect(zone, whole_bars, coords, |p| price.price_to_y(p))
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

/// Pixels between a label and its point, and how far beyond the plot a label's point may be
/// for its text to still reach in.
const LABEL_GAP: f32 = 4.0;
const LABEL_REACH: f32 = 200.0;

pub(crate) fn paint_labels(
    painter: &egui::Painter,
    study: &dyn crate::studies::Indicator,
    price: &crate::scales::PriceMap,
    coords: &crate::chart::coords::ChartMapping,
) {
    use crate::studies::LabelPlacement;
    use egui::{Align2, vec2};
    let labels = study.labels();
    if labels.is_empty() {
        return;
    }
    let clip = painter.clip_rect();
    let first = coords.x_to_idx(clip.left() - LABEL_REACH);
    let last = coords
        .x_to_idx(clip.right() + LABEL_REACH)
        .saturating_add(1);
    for label in &labels[labels.partition_point(|label| label.bar < first)..] {
        if label.bar > last {
            break;
        }
        let style = study.plot_style(label.plot);
        let point = egui::pos2(coords.idx_to_x(label.bar), price.price_to_y(label.price));
        if !style.visible || !label.price.is_finite() || !point.is_finite() {
            continue;
        }
        let color = style.color.to_opaque();
        let galley =
            painter.layout_no_wrap(label.text.clone(), egui::FontId::proportional(11.0), color);
        let (anchor, offset) = match label.placement {
            LabelPlacement::AboveRight => (Align2::LEFT_BOTTOM, vec2(LABEL_GAP, -LABEL_GAP)),
            LabelPlacement::AboveLeft => (Align2::RIGHT_BOTTOM, vec2(-LABEL_GAP, -LABEL_GAP)),
            LabelPlacement::Above => (Align2::CENTER_BOTTOM, vec2(0.0, -LABEL_GAP)),
            LabelPlacement::Below => (Align2::CENTER_TOP, vec2(0.0, LABEL_GAP)),
        };
        let mut rect = anchor.anchor_size(point + offset, galley.size());
        // Every axis tag starts right of the plot, so a label inside it clears them. A label whose
        // point is off the plot stays there, so it is not dragged into view.
        if clip.x_range().contains(point.x) {
            let dx = (clip.right() - rect.right())
                .min(0.0)
                .max(clip.left() - rect.left());
            rect = rect.translate(vec2(dx, 0.0));
        }
        painter.galley(rect.min, galley, color);
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
        price_formatter: None,
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

#[cfg(all(test, feature = "studies", feature = "timezones"))]
mod tests {
    use super::*;
    use crate::chart::coords::ChartMapping;
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::studies::{Indicator, TradingSessions};
    use egui::{Pos2, Rect};

    /// On six 4-hour bars of 2024-07-15 from 00:00 UTC, the default New York session holds only
    /// the bar of 16:00, and its box covers that bar from edge to edge, as the Tokyo and London
    /// boxes cover their two bars. At 100 pixels a bar, bar `i` spans `100 * i - 1` to
    /// `100 * i + 99`.
    #[test]
    fn session_boxes_cover_their_bars_from_edge_to_edge() {
        let start: chrono::DateTime<chrono::Utc> = "2024-07-15T00:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..6)
            .map(|i| {
                let time = start + chrono::TimeDelta::hours(4 * i);
                crate::model::Bar::new(time, 10.0, 11.0, 9.0, 10.0, 1.0)
            })
            .collect();
        let mut sessions = TradingSessions::new();
        sessions.calculate(&bars);
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(0.0, 100.0),
            0.0,
            100.0,
            false,
        );
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(600.0, 100.0));
        let coords = ChartMapping::new(rect, 100.0, 0, 5, 0.0, price);
        let spans: Vec<_> = sessions
            .zones()
            .iter()
            .map(|zone| {
                let whole_bars = sessions.zones_cover_whole_bars();
                let rect = zone_rect(zone, whole_bars, &coords, |price| price as f32).unwrap();
                (zone.bars.clone(), zone.plot, rect.left(), rect.right())
            })
            .collect();
        assert_eq!(
            spans,
            [
                (0..2, 3, -1.0, 199.0),
                (2..4, 4, 199.0, 399.0),
                (4..5, 5, 399.0, 499.0)
            ]
        );
    }
}
