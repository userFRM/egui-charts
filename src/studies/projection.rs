//! Sparse study boxes with fractional logical endpoints, including future whitespace.

/// A projected price band whose endpoints can fall between bar centres.
#[derive(Clone, Debug, PartialEq)]
pub struct StudyProjection {
    /// Logical endpoints, in the same coordinates as the chart's time axis.
    pub bars: std::ops::Range<f64>,
    /// Upper price.
    pub top: f64,
    /// Lower price.
    pub bottom: f64,
    /// Output supplying visibility, stroke and colour.
    pub plot: usize,
}

#[cfg(feature = "widget")]
pub(crate) fn paint(
    painter: &egui::Painter,
    study: &dyn super::Indicator,
    coords: &crate::chart::coords::ChartMapping,
    price: &crate::scales::PriceMap,
) {
    for projection in study.projections() {
        let style = study.plot_style(projection.plot);
        if !style.visible {
            continue;
        }
        let x = |index: f64| {
            coords.rect.right()
                - (coords.base_idx as f32 + coords.right_offset - index as f32 + 0.5)
                    * coords.bar_spacing
                - 1.0
        };
        let rect = egui::Rect::from_two_pos(
            egui::pos2(x(projection.bars.start), price.price_to_y(projection.top)),
            egui::pos2(x(projection.bars.end), price.price_to_y(projection.bottom)),
        );
        if !rect.min.is_finite() || !rect.max.is_finite() || !rect.intersects(painter.clip_rect()) {
            continue;
        }
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
