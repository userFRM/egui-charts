//! Mapped geometry and painting for projected study bands.

use crate::studies::StudyProjection;

#[cfg(feature = "widget")]
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

#[cfg(feature = "widget")]
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
        painter.rect_filled(rect, 0.0, color.gamma_multiply(0.2));
        let stroke = egui::Stroke::new(style.width, color);
        painter.line_segment([rect.left_top(), rect.right_top()], stroke);
        painter.line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
        painter.line_segment([rect.right_top(), rect.right_bottom()], stroke);
    }
}
