//! Price label and price note rendering

use crate::drawings::domain::Drawing;
use crate::drawings::rendering::ChartView;
use crate::scales::{
    DefaultPriceFormatter, PriceFormatter, instrument_price_precision, price_decimals,
};
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

/// Midpoint of the side of `rect` that faces `target`: left or right when the target lies more
/// to the side than above or below, measured against the box's own proportions.
pub(super) fn facing_point(rect: Rect, target: Pos2) -> Pos2 {
    let d = target - rect.center();
    if d.x.abs() * rect.height() > d.y.abs() * rect.width() {
        if d.x > 0.0 {
            rect.right_center()
        } else {
            rect.left_center()
        }
    } else if d.y >= 0.0 {
        rect.center_bottom()
    } else {
        rect.center_top()
    }
}

/// White or black, whichever reads on `fill`, by the rule the price axis tags use. The chart
/// behind a translucent fill is not known here, so the colour the user picked decides.
pub(super) fn label_text_color(fill: Color32) -> Color32 {
    if crate::theme::contrast(Color32::WHITE, fill.to_opaque()) >= 2.0 {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

/// Joins a label to the anchor it annotates: a thin line from the side facing the anchor, ending
/// in a small dot. An anchor under the label needs no line.
pub(super) fn paint_anchor_line(
    painter: &egui::Painter,
    label: Rect,
    anchor: Pos2,
    color: Color32,
) {
    if !label.contains(anchor) {
        painter.line_segment(
            [facing_point(label, anchor), anchor],
            Stroke::new(stroke::HAIRLINE, color),
        );
        painter.circle_filled(anchor, 3.0, color);
    }
}

impl Drawing {
    /// The first anchor's price in the chart's price format, else in the instrument's precision,
    /// else by its magnitude; `None` when the price is not finite.
    fn anchor_price_text(&self, view: &ChartView) -> Option<String> {
        let price = match self.chart_points.first() {
            Some(point) => point.price,
            None => (view.y_to_price)(self.points.first()?.y),
        };
        if !price.is_finite() {
            return None;
        }
        if let Some(formatter) = view.price_formatter {
            return Some(formatter.format(price));
        }
        let precision = view
            .instrument
            .and_then(instrument_price_precision)
            .map_or_else(|| price_decimals(price), usize::from);
        Some(DefaultPriceFormatter::with_precision(precision).format(price))
    }

    /// Renders a price label badge.
    /// Features: colored badge, price display, arrow indicator.
    pub(crate) fn render_price_label(&self, painter: &egui::Painter) {
        if self.points.is_empty() {
            return;
        }
        let pos = self.points[0];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Get price from chart_points if available
        let price_text = if !self.chart_points.is_empty() {
            format!("{:.2}", self.chart_points[0].price)
        } else {
            self.text.as_deref().unwrap_or("0.00").to_string()
        };

        let font = self.annotation_font();
        let galley = painter.layout_no_wrap(price_text.clone(), font.clone(), Color32::WHITE);
        let text_size = galley.size();
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.label_padding_x,
            DESIGN_TOKENS.sizing.annotation.label_padding_y,
        );

        // Arrow pointing left
        let arrow_width = DESIGN_TOKENS.sizing.annotation.pointer_size;
        let box_height = text_size.y + padding.y * 2.0;
        let box_rect = Rect::from_min_size(
            Pos2::new(pos.x + arrow_width, pos.y - box_height / 2.0),
            egui::vec2(text_size.x + padding.x * 2.0, box_height),
        );

        // Arrow triangle
        let arrow_points = vec![
            pos,
            Pos2::new(pos.x + arrow_width, pos.y - box_height / 2.0),
            Pos2::new(pos.x + arrow_width, pos.y + box_height / 2.0),
        ];
        let arrow_shape =
            egui::epaint::PathShape::convex_polygon(arrow_points, color, Stroke::NONE);
        painter.add(arrow_shape);

        // Badge rectangle
        painter.rect_filled(box_rect, DESIGN_TOKENS.rounding.sm, color);

        // Price text
        painter.text(
            box_rect.center(),
            egui::Align2::CENTER_CENTER,
            &price_text,
            font,
            Color32::WHITE,
        );
    }

    /// Renders a price note: the anchor's price and any typed text in a solid label at the second
    /// anchor, joined to the anchor by a thin line.
    pub(crate) fn render_price_note(&self, painter: &egui::Painter, view: &ChartView) {
        let Some(&anchor) = self.points.first() else {
            return;
        };
        let mut text = self.anchor_price_text(view).unwrap_or_default();
        if let Some(typed) = self.text.as_deref().filter(|typed| !typed.is_empty()) {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(typed);
        }
        if text.is_empty() {
            return;
        }
        let color = self.color32();
        let fill = self.fill_color32().unwrap_or(color);
        let text_color = label_text_color(fill);
        let galley = painter.layout_no_wrap(text, self.annotation_font(), text_color);
        let padding = egui::vec2(
            DESIGN_TOKENS.sizing.annotation.label_padding_x,
            DESIGN_TOKENS.sizing.annotation.label_padding_y,
        );
        let corner = self.points.get(1).copied().unwrap_or(anchor);
        let label = Rect::from_min_size(corner, galley.size() + padding * 2.0);
        paint_anchor_line(painter, label, anchor, color);
        painter.rect_filled(label, DESIGN_TOKENS.rounding.sm, fill);
        painter.galley(label.min + padding, galley, text_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, DrawingToolType};
    use crate::model::Instrument;
    use crate::scales::CustomPriceFormatter;

    /// Expected texts follow the published rules: the tick's decimals, explicit decimals over the
    /// tick, and by magnitude without an instrument (2 decimals from 100, 4 from 1).
    #[test]
    fn anchor_prices_follow_the_instrument() {
        let host = CustomPriceFormatter::new(|price| format!("host:{price:.1}"));
        let host = &host as &dyn PriceFormatter;
        for (tick_size, decimals, formatter, price, expected) in [
            (Some(0.00001), None, None, 1.234567, Some("1.23457")),
            (Some(1.0), None, None, 4321.6, Some("4322")),
            (Some(0.25), Some(3), None, 99.5, Some("99.500")),
            (None, None, None, 4321.6, Some("4321.60")),
            (None, None, None, 1.234567, Some("1.2346")),
            (None, None, None, -0.0, Some("0.000000")),
            (Some(1.0), None, Some(host), 4321.6, Some("host:4321.6")),
            (None, None, None, f64::NAN, None),
            (None, None, None, f64::INFINITY, None),
        ] {
            let mut instrument = Instrument::new("X");
            instrument.tick_size = tick_size;
            instrument.price_decimals = decimals;
            let view = ChartView {
                price_formatter: formatter,
                instrument: (tick_size.is_some() || decimals.is_some()).then_some(&instrument),
                bars: &[],
                revision: 0,
                bar_to_x: &|bar| bar,
                price_to_y: &|price| price as f32,
                y_to_price: &|_| f64::NAN,
                time_axis: true,
                executions: None,
            };
            let mut drawing = Drawing::new(7, DrawingToolType::PriceLabel);
            drawing.chart_points.push(ChartPoint::new(0.0, price));
            assert_eq!(
                drawing.anchor_price_text(&view).as_deref(),
                expected,
                "{price}"
            );
            // Standalone renders hold screen points only.
            drawing.chart_points.clear();
            drawing.points.push(Pos2::new(0.0, 0.0));
            assert_eq!(drawing.anchor_price_text(&view), None, "{price}");
        }
    }
}
