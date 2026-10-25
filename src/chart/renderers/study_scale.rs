//! Independent study mapping and its axis beside the price pane.

use super::ChartMapping;
use crate::scales::{PriceMap, PriceRange, PriceTransform};
use crate::studies::{Indicator, PlotType, StudyScale};
use crate::theme::ThemeExt;
use egui::{Align2, Painter, Pos2, Rect};

/// Keep independent study units out of the main price map, including hidden scales.
pub(crate) fn study_mapping(study: &dyn Indicator, coords: &ChartMapping) -> ChartMapping {
    if study.scale() == StudyScale::Main {
        return *coords;
    }
    let start = coords.x_to_idx(coords.rect.left()).saturating_sub(1);
    let end = coords.x_to_idx(coords.rect.right()).saturating_add(2);
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for line in 0..study.line_cnt() {
        let plot = study.plot_style(line);
        if !plot.visible
            || matches!(
                plot.plot_type,
                PlotType::BarColor | PlotType::BackgroundColor
            )
        {
            continue;
        }
        for bar in start..end {
            if let Some(value) = study.plot_value(line, bar) {
                low = low.min(value);
                high = high.max(value);
            }
        }
        if low.is_finite()
            && matches!(
                plot.plot_type,
                PlotType::Histogram | PlotType::Columns | PlotType::Area
            )
        {
            low = low.min(0.0);
            high = high.max(0.0);
        }
    }
    for price in crate::studies::plots::drawing_prices(study, start..end) {
        low = low.min(price);
        high = high.max(price);
    }
    if !low.is_finite() {
        low = 0.0;
        high = 1.0;
    }
    let padding = crate::scales::auto_scale_span(low, high) * 0.1;
    let price = PriceMap::new(
        PriceTransform::default(),
        PriceRange::new(
            (low - padding).max(-f64::MAX),
            (high + padding).min(f64::MAX),
        ),
        coords.rect.top(),
        coords.rect.bottom(),
        false,
    )
    .with_price_precision(Some(study.precision().unwrap_or(2).min(18) as u8));
    ChartMapping { price, ..*coords }
}

/// Paint an independent study axis in its reserved gutter to the left of the plot.
pub(crate) fn render_study_axis(
    painter: &Painter,
    study: &dyn Indicator,
    coords: &ChartMapping,
    axis: Rect,
    show_values: bool,
) -> f32 {
    let painter = painter.with_clip_rect(axis);
    let theme = painter.ctx().active_theme();
    let font = crate::chart::rendering::axes::axis_font(painter.ctx());
    let x = axis.right() - 8.0;
    let mut width = 0.0_f32;
    let mut labels = Vec::new();
    if show_values && let Some(bar) = study.values().len().checked_sub(1) {
        for line in 0..study.line_cnt() {
            let plot = study.plot_style(line);
            if plot.visible
                && !matches!(
                    plot.plot_type,
                    PlotType::BarColor | PlotType::BackgroundColor
                )
                && let Some(value) = study.plot_value(line, bar)
            {
                labels.push((value, Some(study.plot_style(line).color)));
            }
        }
    }
    let mut occupied = Vec::new();
    for (value, color) in labels.into_iter().chain(
        coords
            .price
            .marks()
            .into_iter()
            .map(|mark| (mark.price, None)),
    ) {
        let y = coords.price_to_y(value);
        let label = study.format_value(value);
        let galley = painter.layout_no_wrap(
            label,
            font.clone(),
            color.map_or(theme.chart.axis_text, |color| {
                crate::chart::rendering::axes::tag_text_color(color)
            }),
        );
        let rect = Align2::RIGHT_CENTER
            .anchor_size(Pos2::new(x, y), galley.size())
            .expand(2.0);
        width = width.max(rect.width() + 8.0);
        if !axis.contains_rect(rect) || occupied.iter().any(|other: &Rect| other.intersects(rect)) {
            continue;
        }
        painter.rect_filled(rect, 0.0, color.unwrap_or(theme.chart.bg));
        painter.galley(
            rect.min + egui::vec2(2.0, 2.0),
            galley,
            theme.chart.axis_text,
        );
        occupied.push(rect);
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::renderers::{IndicatorRenderer, RenderContext};
    use crate::studies::{CustomIndicator, IndicatorValue, StudyFormat};

    #[test]
    fn independent_and_hidden_scales_map_volume_without_price_units() {
        let rect = Rect::from_min_max(Pos2::new(80.0, 0.0), Pos2::new(280.0, 120.0));
        let main = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(100.0, 200.0),
            0.0,
            120.0,
            false,
        );
        let coords = ChartMapping::new(rect, 40.0, 0, 1, 0.0, main);
        for scale in [StudyScale::Independent, StudyScale::None] {
            let mut study = CustomIndicator::new("Volume overlay", |_| {
                vec![
                    IndicatorValue::Single(1000.0),
                    IndicatorValue::Single(3000.0),
                ]
            })
            .with_color(egui::Color32::BLUE)
            .with_value_format(StudyFormat::Volume)
            .with_scale(scale);
            study.calculate(&[]);
            let mapping = study_mapping(&study, &coords);
            // Visible values 1000..3000 plus 10% padding give 800..3200.
            assert!((mapping.price_to_y(1000.0) - 110.0).abs() < 0.001);
            assert!((mapping.price_to_y(3000.0) - 10.0).abs() < 0.001);
            let bars =
                vec![
                    crate::model::Bar::new(chrono::Utc::now(), 150.0, 150.0, 150.0, 150.0, 1.0);
                    2
                ];
            let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
            let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                IndicatorRenderer::render(
                    &RenderContext::new(ui.painter(), rect),
                    &indicators,
                    &bars,
                    &main,
                    &coords,
                );
                if scale == StudyScale::Independent {
                    render_study_axis(
                        ui.painter(),
                        indicators[0].as_ref(),
                        &mapping,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(80.0, 120.0)),
                        true,
                    );
                }
            });
            let painted = output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::LineSegment {points, stroke} if stroke.color == egui::Color32::BLUE && (points[0].y - 110.0).abs() < 0.001 && (points[1].y - 10.0).abs() < 0.001));
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            assert!(painted);
            assert_eq!(
                labels.iter().any(|label| label == "3.00K"),
                scale == StudyScale::Independent,
                "{labels:?}"
            );
        }
    }
}
