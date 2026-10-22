//! Forecast tool rendering

use crate::drawings::domain::{ChartPoint, Drawing};
use crate::drawings::rendering::{ChartView, bars_between};
use crate::model::Bar;
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Renders a forecast from the start anchor to the target anchor, with the
    /// predicted change and whether the price has reached the target in time.
    pub(crate) fn render_forecast(&self, painter: &egui::Painter, view: &ChartView) {
        let (&[start, end, ..], &[from, to, ..]) =
            (self.points.as_slice(), self.chart_points.as_slice())
        else {
            return;
        };

        // Trend line
        painter.line_segment([start, end], Stroke::new(self.stroke_width, self.color32()));

        let price_change = to.price - from.price;
        let price_change_pct = if from.price != 0.0 {
            price_change / from.price * 100.0
        } else {
            0.0
        };
        let bar_count = (to.bar_idx - from.bar_idx).round();

        let (status, status_color) = match forecast_outcome(view.bars, from, to) {
            Some(true) => ("SUCCESS", DESIGN_TOKENS.semantic.extended.bullish),
            Some(false) => ("FAILED", DESIGN_TOKENS.semantic.extended.bearish),
            None => (
                "PENDING",
                DESIGN_TOKENS.semantic.extended.chart_text_secondary,
            ),
        };

        // Info box above an upward target, below a downward one
        let box_size = egui::vec2(220.0, 50.0);
        let top = if price_change >= 0.0 {
            end.y - DESIGN_TOKENS.spacing.md - box_size.y
        } else {
            end.y + DESIGN_TOKENS.spacing.md
        };
        let box_rect = Rect::from_min_size(Pos2::new(end.x - box_size.x / 2.0, top), box_size);
        if !painter.clip_rect().intersects(box_rect) {
            return;
        }

        // Status header bar
        let header_rect = Rect::from_min_size(box_rect.min, egui::vec2(box_size.x, 25.0));
        painter.rect_filled(header_rect, DESIGN_TOKENS.rounding.md, status_color);
        painter.text(
            header_rect.center(),
            egui::Align2::CENTER_CENTER,
            status,
            egui::FontId::proportional(typography::MD),
            Color32::WHITE,
        );

        // White info section
        let info_rect =
            Rect::from_min_max(Pos2::new(box_rect.min.x, header_rect.max.y), box_rect.max);
        painter.rect_filled(info_rect, DESIGN_TOKENS.rounding.md, Color32::WHITE);

        // Price change and duration (dark text on white)
        painter.text(
            info_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{price_change:+.2} ({price_change_pct:+.2}%) in {bar_count} bars"),
            egui::FontId::proportional(typography::LG),
            DESIGN_TOKENS.semantic.chart.bg, // Dark chart background color for contrast
        );
    }
}

/// Whether the price reached the target after the start bar and by the
/// target bar: `Some(true)` once a bar trades at the target, `Some(false)`
/// once a bar after the target bar exists without that, `None` while the
/// target bar is ahead or still forming (or the target is not after the start).
fn forecast_outcome(bars: &[Bar], from: ChartPoint, to: ChartPoint) -> Option<bool> {
    let (start, end) = (from.bar_idx.round(), to.bar_idx.round());
    if end <= start {
        return None;
    }
    let (_, window) = bars_between(bars, start + 1.0, end)?;
    let up = to.price >= from.price;
    let reached = window.iter().any(|bar| {
        if up {
            bar.high >= to.price
        } else {
            bar.low <= to.price
        }
    });
    if reached {
        Some(true)
    } else {
        ((end as usize).saturating_add(1) < bars.len()).then_some(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::rendering::sample_bars;

    /// Expected verdicts read off the sample bars by hand.
    #[test]
    fn forecast_outcome_checks_bars_up_to_target() {
        let bars = sample_bars();
        let cases = [
            // Up from bar 0: bar 3 (high 14) reaches 13.8 by bar 3.
            ((0.0, 11.0), (3.0, 13.8), Some(true)),
            // Up from bar 0: no high reaches 14.5 by bar 3.
            ((0.0, 11.0), (3.0, 14.5), Some(false)),
            // Target bar 9 is still ahead, but bar 4 (high 15) already reached 14.5.
            ((0.0, 11.0), (9.0, 14.5), Some(true)),
            // Target bar 9 is still ahead and no high reaches 16 yet.
            ((0.0, 11.0), (9.0, 16.0), None),
            // Down from bar 3: bar 5 (low 12) reaches 12.2.
            ((3.0, 13.5), (5.0, 12.2), Some(true)),
            // Down from bar 3: no low reaches 11.5 by bar 5.
            ((3.0, 13.5), (5.0, 11.5), Some(false)),
            // Target bar 6 is the last, still forming bar: not decided yet.
            ((3.0, 13.5), (6.0, 11.5), None),
        ];
        for ((from_bar, from_price), (to_bar, to_price), want) in cases {
            let from = ChartPoint::new(from_bar, from_price);
            let to = ChartPoint::new(to_bar, to_price);
            assert_eq!(
                forecast_outcome(&bars, from, to),
                want,
                "{from:?} -> {to:?}"
            );
        }
    }
}
