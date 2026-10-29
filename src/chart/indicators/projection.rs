//! Mapped geometry and painting for projected study bands.

use crate::studies::StudyProjection;

pub(crate) fn projection_rect(
    projection: &StudyProjection,
    coords: &crate::chart::coords::ChartMapping,
    price_to_y: impl Fn(f64) -> f32,
) -> Option<egui::Rect> {
    if projection.bars.start >= projection.bars.end {
        return None;
    }
    let x =
        |index: f64| coords.idx_to_x(index as usize) + index.fract() as f32 * coords.bar_spacing;
    let rect = egui::Rect::from_two_pos(
        egui::pos2(x(projection.bars.start), price_to_y(projection.top)),
        egui::pos2(x(projection.bars.end), price_to_y(projection.bottom)),
    );
    (rect.min.is_finite() && rect.max.is_finite()).then_some(rect)
}

pub(crate) fn paint(
    painter: &egui::Painter,
    study: &dyn crate::studies::Indicator,
    coords: &crate::chart::coords::ChartMapping,
    price: &crate::scales::PriceMap,
) {
    for projection in study.projections() {
        let style = study.plot_style(projection.plot);
        let Some(rect) = projection_rect(projection, coords, |p| price.price_to_y(p))
            .filter(|rect| style.visible && rect.intersects(painter.clip_rect()))
        else {
            continue;
        };
        let color = study
            .plot_color(projection.plot, projection.bars.start as usize)
            .unwrap_or(style.color);
        painter.rect_filled(rect, 0.0, study.projection_fill_color(projection));
        let stroke = egui::Stroke::new(style.width, color);
        for [start, end] in [
            [rect.left_top(), rect.right_top()],
            [rect.left_bottom(), rect.right_bottom()],
            [rect.right_top(), rect.right_bottom()],
        ] {
            if let Some((dash, gap)) = style.pattern.dash_pattern() {
                crate::painting::dashed_line::draw_dashed_line(
                    painter, start, end, stroke, dash, gap,
                );
            } else {
                painter.line_segment([start, end], stroke);
            }
        }
    }
}
