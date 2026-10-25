//! Bands between study outputs or fixed levels, and shapes stored per shape instead of per bar.

use egui::Color32;

/// One boundary of a study band, in the study's value scale.
#[derive(Clone, Copy, Debug)]
pub enum FillBoundary {
    /// A zero-based output line, including its offset and visibility.
    Plot(usize),
    /// A constant level across the pane.
    Level(f64),
}

/// A band between two plots or levels.
#[derive(Clone, Copy, Debug)]
pub struct StudyFill {
    /// First boundary.
    pub first: FillBoundary,
    /// Second boundary.
    pub second: FillBoundary,
    /// Colour when the first value is at least the second.
    pub color: Color32,
    /// Optional colour when the first value is below the second.
    pub inverted_color: Option<Color32>,
}

/// One OHLCV candle spanning an exclusive range of source bar indices.
#[derive(Clone, Debug)]
pub struct StudyCandle {
    /// Loaded source bars covered by this candle; gaps do not invent positions.
    pub bars: std::ops::Range<usize>,
    /// Aggregated period, stamped with its opening calendar label.
    pub bar: crate::model::Bar,
    /// Output whose colour, stroke width and visibility apply to this candle.
    pub plot: usize,
}

/// A price band between two source bars, stored once instead of as a value on every bar.
#[derive(Clone, Debug, PartialEq)]
pub struct StudyZone {
    /// Source bars the band spans, from the centre of the first to the centre of the last.
    pub bars: std::ops::Range<usize>,
    /// Upper price.
    pub top: f64,
    /// Lower price.
    pub bottom: f64,
    /// Output whose colour, stroke width and visibility apply to this zone: its edges in that
    /// colour and its band at 15% opacity.
    pub plot: usize,
}

#[cfg(feature = "widget")]
impl StudyZone {
    pub(crate) fn rect(
        &self,
        coords: &crate::chart::coords::ChartMapping,
        price_to_y: impl Fn(f64) -> f32,
    ) -> Option<egui::Rect> {
        let last = self
            .bars
            .end
            .checked_sub(1)
            .filter(|&l| l >= self.bars.start)?;
        let rect = egui::Rect::from_two_pos(
            egui::pos2(coords.idx_to_x(self.bars.start), price_to_y(self.top)),
            egui::pos2(coords.idx_to_x(last), price_to_y(self.bottom)),
        );
        (rect.min.is_finite() && rect.max.is_finite()).then_some(rect)
    }
}

#[cfg(feature = "widget")]
impl StudyCandle {
    pub(crate) fn geometry(
        &self,
        coords: &crate::chart::coords::ChartMapping,
        price_to_y: impl Fn(f64) -> f32,
    ) -> Option<(egui::Rect, [egui::Pos2; 2])> {
        use egui::{Pos2, Rect};
        if self.bars.is_empty() {
            return None;
        }
        let half = coords.bar_spacing * 0.5;
        let inset = (coords.bar_spacing * 0.1).min(2.0);
        let left = coords.idx_to_x(self.bars.start) - half + inset;
        let right = coords.idx_to_x(self.bars.end - 1) + half - inset;
        let x = (left + right) * 0.5;
        let (open, close) = (price_to_y(self.bar.open), price_to_y(self.bar.close));
        let wick = [
            Pos2::new(x, price_to_y(self.bar.high)),
            Pos2::new(x, price_to_y(self.bar.low)),
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
}

#[cfg(feature = "widget")]
pub(crate) fn paint_candles(
    context: &crate::chart::renderers::RenderContext,
    study: &dyn super::Indicator,
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
        if let Some((body, wick)) = candle.geometry(coords, |p| price.price_to_y(p)) {
            let stroke = egui::Stroke::new(style.width, style.color);
            painter.line_segment(wick, stroke);
            painter.rect(
                body,
                0.0,
                style.color.gamma_multiply(0.12),
                stroke,
                egui::StrokeKind::Inside,
            );
        }
    }
}

#[cfg(feature = "widget")]
pub(crate) fn paint_zones(
    painter: &egui::Painter,
    study: &dyn super::Indicator,
    price: &crate::scales::PriceMap,
    coords: &crate::chart::coords::ChartMapping,
) {
    let clip = painter.clip_rect();
    for zone in study.zones() {
        let style = study.plot_style(zone.plot);
        let Some(rect) = zone
            .rect(coords, |p| price.price_to_y(p))
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
#[cfg(feature = "widget")]
pub(crate) fn drawing_prices(
    study: &dyn super::Indicator,
    bars: std::ops::Range<usize>,
) -> impl Iterator<Item = f64> + '_ {
    study
        .drawings()
        .iter()
        .filter_map(move |drawing| {
            if !study.plot_style(0).visible || !drawing.visible || bars.is_empty() {
                return None;
            }
            let first = drawing
                .chart_points
                .iter()
                .map(|p| p.bar_idx)
                .reduce(f32::min)?;
            let last = drawing
                .chart_points
                .iter()
                .map(|p| p.bar_idx)
                .reduce(f32::max)?;
            let levels = drawing.fib_levels();
            let left = drawing.extend_left
                || levels
                    .as_ref()
                    .is_some_and(|(c, _)| c.extend_left || c.show_background);
            let right = drawing.extend_right
                || drawing.tool_type == crate::drawings::DrawingToolType::Pitchfork
                || levels
                    .as_ref()
                    .is_some_and(|(c, _)| c.extend_right || c.show_background);
            if (!left && first >= bars.end as f32) || (!right && last < bars.start as f32) {
                return None;
            }
            Some(
                drawing
                    .chart_points
                    .iter()
                    .map(|p| p.price)
                    .chain(levels.into_iter().flat_map(|(_, prices)| prices)),
            )
        })
        .flatten()
        .filter(|price| price.is_finite())
}

/// Apply the study's presentation and pane mapping before rendering or hit testing.
#[cfg(feature = "widget")]
pub(crate) fn mapped_drawings(
    study: &dyn super::Indicator,
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
        .map(|drawing| {
            let mut drawing = drawing.clone();
            drawing.color = style.color.to_srgba_unmultiplied();
            drawing.stroke_width = style.width;
            drawing.line_style = match style.pattern {
                super::PlotPattern::Solid => crate::drawings::LineStyle::Solid,
                super::PlotPattern::Dashed => crate::drawings::LineStyle::Dashed,
                super::PlotPattern::Dotted => crate::drawings::LineStyle::Dotted,
                super::PlotPattern::LargeDashed => crate::drawings::LineStyle::LargeDashed,
                super::PlotPattern::SparseDotted => crate::drawings::LineStyle::SparseDotted,
            };
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

#[cfg(feature = "widget")]
pub(crate) fn paint_drawings(
    context: &crate::chart::renderers::RenderContext,
    study: &dyn super::Indicator,
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
    };
    for drawing in mapped_drawings(study, coords, price_to_y) {
        drawing.render(
            &context.painter.with_clip_rect(context.rect),
            context.rect,
            &view,
        );
    }
}
