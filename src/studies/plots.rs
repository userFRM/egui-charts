//! Bands between study outputs or fixed levels.

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
