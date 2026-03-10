//! Cyclic tool rendering implementations
//!
//! Includes: cyclic lines, time cycles, and sine line.

use crate::drawings::domain::Drawing;
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};

impl Drawing {
    /// Renders cyclic vertical lines with customizable period and phase.
    /// Features: repeating vertical lines, cycle number labels, phase shift support.
    pub(crate) fn render_cyclic_lines(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let p1 = self.points[0];
        let p2 = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let period = (p2.x - p1.x).abs();
        if period < 1.0 {
            return;
        }

        // Alternating fill between cycle lines
        let fill_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 20);

        // Extend left from anchor point
        let mut x = p1.x;
        while x >= chart_rect.min.x {
            x -= period;
        }
        x += period; // Back to first visible cycle

        let font = egui::FontId::proportional(typography::XS);
        let mut fill_toggle = false;

        // Calculate starting cycle number (negative for lines left of anchor)
        let first_cycle = ((p1.x - x) / period).round() as i32;
        let mut cycle_num = -first_cycle;

        while x <= chart_rect.max.x {
            // Draw vertical line
            painter.vline(
                x,
                chart_rect.y_range(),
                Stroke::new(stroke::HAIRLINE, color),
            );

            // Alternating fill between lines
            if fill_toggle && x + period <= chart_rect.max.x {
                let fill_rect = Rect::from_min_max(
                    Pos2::new(x, chart_rect.min.y),
                    Pos2::new(x + period, chart_rect.max.y),
                );
                painter.rect_filled(fill_rect, 0.0, fill_color);
            }
            fill_toggle = !fill_toggle;

            // Cycle number label at top
            let label = format!("{}", cycle_num);
            let label_bg = Rect::from_center_size(
                Pos2::new(x, chart_rect.min.y + DESIGN_TOKENS.spacing.xl),
                egui::vec2(
                    DESIGN_TOKENS.sizing.technical_labels.cycle_label_width / 2.0,
                    DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                ),
            );
            painter.rect_filled(
                label_bg,
                DESIGN_TOKENS.rounding.sm,
                DESIGN_TOKENS
                    .semantic
                    .extended
                    .chart_axis_bg
                    .gamma_multiply(0.78),
            );
            painter.text(
                Pos2::new(x, chart_rect.min.y + 12.0),
                egui::Align2::CENTER_CENTER,
                label,
                font.clone(),
                color,
            );

            x += period;
            cycle_num += 1;
        }

        // Draw anchor points with white stroke
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p1,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p2,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );

        // Period label at bottom center of first cycle
        let period_bars = if !self.chart_points.is_empty() && self.chart_points.len() >= 2 {
            (self.chart_points[1].bar_idx - self.chart_points[0].bar_idx).abs() as i32
        } else {
            (period / 10.0) as i32 // Approximate
        };
        let period_label = format!("Period: {} bars", period_bars);
        let period_bg = Rect::from_center_size(
            Pos2::new(
                (p1.x + p2.x) / 2.0,
                chart_rect.max.y - DESIGN_TOKENS.spacing.xl - DESIGN_TOKENS.spacing.sm,
            ),
            egui::vec2(
                DESIGN_TOKENS.sizing.technical_labels.channel_label_width
                    + DESIGN_TOKENS.spacing.section_lg
                    + DESIGN_TOKENS.spacing.sm,
                DESIGN_TOKENS.sizing.technical_labels.elliott_label_size,
            ),
        );
        painter.rect_filled(
            period_bg,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.86),
        );
        painter.text(
            period_bg.center(),
            egui::Align2::CENTER_CENTER,
            period_label,
            font,
            color,
        );
    }

    /// Renders time cycles with circular/arc projections.
    /// Features: concentric arcs, time-based circles, harmonic relationships.
    pub(crate) fn render_time_cycles(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let center = self.points[0];
        let edge = self.points[1];
        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        let base_radius = ((edge.x - center.x).powi(2) + (edge.y - center.y).powi(2)).sqrt();
        if base_radius < 5.0 {
            return;
        }

        // Draw multiple concentric circles (harmonic multiples)
        let harmonics = [0.5, 1.0, 1.5, 2.0, 2.5, 3.0];
        let font = egui::FontId::proportional(typography::XS);

        for (i, &mult) in harmonics.iter().enumerate() {
            let radius = base_radius * mult;
            let alpha = if (mult - 1.0).abs() < 0.01 { 255 } else { 100 };
            let circle_color =
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], alpha);

            // Draw arc (semicircle facing right for time projection)
            let segments = 32;
            for j in 0..segments {
                let angle1 = -std::f32::consts::FRAC_PI_2
                    + std::f32::consts::PI * j as f32 / segments as f32;
                let angle2 = -std::f32::consts::FRAC_PI_2
                    + std::f32::consts::PI * (j + 1) as f32 / segments as f32;

                let p1 = Pos2::new(
                    center.x + radius * angle1.cos(),
                    center.y + radius * angle1.sin(),
                );
                let p2 = Pos2::new(
                    center.x + radius * angle2.cos(),
                    center.y + radius * angle2.sin(),
                );

                // Only draw if within chart bounds
                if p1.x >= chart_rect.min.x && p2.x >= chart_rect.min.x {
                    let stroke_w = if (mult - 1.0).abs() < 0.01 {
                        stroke::THICK
                    } else {
                        stroke::HAIRLINE
                    };
                    painter.line_segment([p1, p2], Stroke::new(stroke_w, circle_color));
                }
            }

            // Label at right edge of arc
            let label_x = center.x + radius;
            if label_x
                <= chart_rect.max.x
                    + DESIGN_TOKENS.sizing.technical_labels.line_label_width
                    + DESIGN_TOKENS.spacing.lg
            {
                let label = format!("{}x", mult);
                let label_bg = Rect::from_center_size(
                    Pos2::new(
                        label_x + DESIGN_TOKENS.spacing.xl + DESIGN_TOKENS.spacing.sm,
                        center.y,
                    ),
                    egui::vec2(
                        DESIGN_TOKENS.sizing.technical_labels.cycle_label_width
                            - DESIGN_TOKENS.spacing.lg,
                        DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
                    ),
                );
                painter.rect_filled(
                    label_bg,
                    DESIGN_TOKENS.rounding.sm,
                    DESIGN_TOKENS
                        .semantic
                        .extended
                        .chart_axis_bg
                        .gamma_multiply(0.78),
                );
                painter.text(
                    label_bg.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    font.clone(),
                    circle_color,
                );
            }

            // Fill between 1x and 2x circles with subtle color
            if i == 1 {
                // This is the 1x circle
                let fill_color = Color32::from_rgba_unmultiplied(
                    self.color[0],
                    self.color[1],
                    self.color[2],
                    15,
                );
                // Draw filled arc between 1x and 2x
                let inner_r = radius;
                let outer_r = base_radius * 2.0;
                for j in 0..segments {
                    let angle1 = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::PI * j as f32 / segments as f32;
                    let angle2 = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::PI * (j + 1) as f32 / segments as f32;

                    let inner1 = Pos2::new(
                        center.x + inner_r * angle1.cos(),
                        center.y + inner_r * angle1.sin(),
                    );
                    let inner2 = Pos2::new(
                        center.x + inner_r * angle2.cos(),
                        center.y + inner_r * angle2.sin(),
                    );
                    let outer1 = Pos2::new(
                        center.x + outer_r * angle1.cos(),
                        center.y + outer_r * angle1.sin(),
                    );
                    let outer2 = Pos2::new(
                        center.x + outer_r * angle2.cos(),
                        center.y + outer_r * angle2.sin(),
                    );

                    if inner1.x >= chart_rect.min.x {
                        let mesh = egui::Mesh {
                            indices: vec![0, 1, 2, 0, 2, 3],
                            vertices: vec![
                                egui::epaint::Vertex {
                                    pos: inner1,
                                    uv: egui::epaint::WHITE_UV,
                                    color: fill_color,
                                },
                                egui::epaint::Vertex {
                                    pos: inner2,
                                    uv: egui::epaint::WHITE_UV,
                                    color: fill_color,
                                },
                                egui::epaint::Vertex {
                                    pos: outer2,
                                    uv: egui::epaint::WHITE_UV,
                                    color: fill_color,
                                },
                                egui::epaint::Vertex {
                                    pos: outer1,
                                    uv: egui::epaint::WHITE_UV,
                                    color: fill_color,
                                },
                            ],
                            texture_id: egui::TextureId::default(),
                        };
                        painter.add(egui::Shape::mesh(mesh));
                    }
                }
            }
        }

        // Draw center anchor point
        painter.circle_filled(center, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            center,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::THICK, Color32::WHITE),
        );

        // Draw edge point
        painter.circle_filled(edge, DESIGN_TOKENS.rounding.md, color);
        painter.circle_stroke(
            edge,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );

        // Base cycle info
        let bars = if !self.chart_points.is_empty() && self.chart_points.len() >= 2 {
            (self.chart_points[1].bar_idx - self.chart_points[0].bar_idx).abs() as i32
        } else {
            (base_radius / 10.0) as i32
        };
        let info_label = format!("Cycle: {} bars", bars);
        let info_bg = Rect::from_center_size(
            Pos2::new(
                center.x,
                center.y - base_radius - DESIGN_TOKENS.spacing.xxl - DESIGN_TOKENS.spacing.sm,
            ),
            egui::vec2(
                DESIGN_TOKENS.sizing.technical_labels.channel_label_width
                    + DESIGN_TOKENS.spacing.xxl,
                DESIGN_TOKENS.sizing.technical_labels.elliott_label_size,
            ),
        );
        painter.rect_filled(
            info_bg,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.86),
        );
        painter.text(
            info_bg.center(),
            egui::Align2::CENTER_CENTER,
            info_label,
            font,
            color,
        );
    }

    /// Renders a sine wave line for cyclical analysis.
    /// Features: amplitude/wavelength controls, phase offset, smooth curve.
    pub(crate) fn render_sine_line(&self, painter: &egui::Painter, chart_rect: Rect) {
        if self.points.len() < 2 {
            return;
        }
        let p1 = self.points[0]; // Start/center of wave
        let p2 = self.points[1]; // Defines wavelength and amplitude

        let color = Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        );

        // Calculate wavelength (horizontal distance between points = half wavelength)
        let wavelength = (p2.x - p1.x).abs() * 2.0;
        if wavelength < 10.0 {
            return;
        }

        // Calculate amplitude (vertical distance from center)
        let amplitude = (p2.y - p1.y).abs();
        let center_y = p1.y;

        // Phase offset (0 starts at zero crossing going up)
        let phase = if p2.y < p1.y {
            0.0 // Going up from p1 to p2
        } else {
            std::f32::consts::PI // Going down
        };

        // Draw sine wave extending across chart
        let segments = 200;
        let start_x = chart_rect.min.x;
        let end_x = chart_rect.max.x;
        let x_range = end_x - start_x;

        let mut prev_point: Option<Pos2> = None;

        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let x = start_x + t * x_range;

            // Calculate phase at this x position
            let wave_phase = ((x - p1.x) / wavelength) * 2.0 * std::f32::consts::PI + phase;
            let y = center_y + amplitude * wave_phase.sin();

            let current = Pos2::new(x, y);

            if let Some(prev) = prev_point {
                painter.line_segment([prev, current], Stroke::new(self.stroke_width, color));
            }
            prev_point = Some(current);
        }

        // Draw zero/center line (dashed)
        let zero_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 80);
        let dash_len = DESIGN_TOKENS.spacing.lg;
        let gap_len = DESIGN_TOKENS.spacing.md + DESIGN_TOKENS.spacing.xs;
        let mut x = chart_rect.min.x;
        while x < chart_rect.max.x {
            let end = (x + dash_len).min(chart_rect.max.x);
            painter.line_segment(
                [Pos2::new(x, center_y), Pos2::new(end, center_y)],
                Stroke::new(stroke::HAIRLINE, zero_color),
            );
            x += dash_len + gap_len;
        }

        // Draw amplitude guides (dashed horizontal lines at peaks)
        let peak_y = center_y - amplitude;
        let trough_y = center_y + amplitude;
        x = chart_rect.min.x;
        while x < chart_rect.max.x {
            let end = (x + dash_len).min(chart_rect.max.x);
            painter.line_segment(
                [Pos2::new(x, peak_y), Pos2::new(end, peak_y)],
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, zero_color),
            );
            painter.line_segment(
                [Pos2::new(x, trough_y), Pos2::new(end, trough_y)],
                Stroke::new(DESIGN_TOKENS.stroke.extra_thin, zero_color),
            );
            x += dash_len + gap_len;
        }

        // Draw anchor points
        painter.circle_filled(p1, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p1,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );
        painter.circle_filled(p2, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            p2,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::MEDIUM, Color32::WHITE),
        );

        // Info labels
        let font = egui::FontId::proportional(typography::XS);

        // Wavelength label
        let wave_bars = if !self.chart_points.is_empty() && self.chart_points.len() >= 2 {
            ((self.chart_points[1].bar_idx - self.chart_points[0].bar_idx).abs() * 2.0) as i32
        } else {
            (wavelength / 10.0) as i32
        };
        let wavelength_label = format!("λ: {} bars", wave_bars);
        let wave_bg = Rect::from_center_size(
            Pos2::new(
                p1.x + wavelength / 4.0,
                center_y + DESIGN_TOKENS.spacing.xxl + DESIGN_TOKENS.spacing.sm,
            ),
            egui::vec2(
                DESIGN_TOKENS.sizing.technical_labels.pattern_label_width
                    + DESIGN_TOKENS.spacing.lg,
                DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
            ),
        );
        painter.rect_filled(
            wave_bg,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.78),
        );
        painter.text(
            wave_bg.center(),
            egui::Align2::CENTER_CENTER,
            wavelength_label,
            font.clone(),
            color,
        );

        // Amplitude label
        let amp_price = if !self.chart_points.is_empty() && self.chart_points.len() >= 2 {
            (self.chart_points[1].price - self.chart_points[0].price).abs()
        } else {
            (amplitude / 10.0) as f64
        };
        let amp_label = format!("A: {:.2}", amp_price);
        let amp_bg = Rect::from_center_size(
            Pos2::new(
                p2.x + DESIGN_TOKENS.spacing.xxl + DESIGN_TOKENS.spacing.sm,
                (p1.y + p2.y) / 2.0,
            ),
            egui::vec2(
                DESIGN_TOKENS.sizing.technical_labels.channel_offset_x,
                DESIGN_TOKENS.sizing.technical_labels.pattern_label_height,
            ),
        );
        painter.rect_filled(
            amp_bg,
            DESIGN_TOKENS.rounding.sm,
            DESIGN_TOKENS
                .semantic
                .extended
                .chart_axis_bg
                .gamma_multiply(0.78),
        );
        painter.text(
            amp_bg.center(),
            egui::Align2::CENTER_CENTER,
            amp_label,
            font,
            color,
        );
    }
}
