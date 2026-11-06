use super::Chart;
use crate::chart::rendering;
use crate::config::{BackgroundStyle, WatermarkPos};
use egui::{Rect, Ui};

impl Chart {
    /// The colour [`Self::draw_background`] paints the plot with, the midpoint of a gradient, or
    /// `None` when something else paints it.
    pub(super) fn plot_background(&self) -> Option<egui::Color32> {
        if self.config.skip_background {
            return None;
        }
        Some(match self.config.background_style {
            BackgroundStyle::Solid => self.config.background_color,
            BackgroundStyle::VerticalGradient {
                top_color: first,
                bottom_color: second,
            }
            | BackgroundStyle::HorizontalGradient {
                left_color: first,
                right_color: second,
            } => first.lerp_to_gamma(second, 0.5),
        })
    }

    /// Draw the chart background (solid or gradient)
    pub(super) fn draw_background(&self, painter: &egui::Painter, rect: Rect) {
        // Skip background when chart is inside a container that handles its own background
        if self.config.skip_background {
            return;
        }

        match self.config.background_style {
            BackgroundStyle::Solid => {
                painter.rect_filled(rect, 0.0, self.config.background_color);
            }
            BackgroundStyle::VerticalGradient {
                top_color,
                bottom_color,
            } => {
                // Draw vertical gradient using a mesh
                let mesh = egui::Mesh {
                    indices: vec![0, 1, 2, 2, 3, 0],
                    vertices: vec![
                        egui::epaint::Vertex {
                            pos: rect.left_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: top_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: top_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: bottom_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.left_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: bottom_color,
                        },
                    ],
                    texture_id: egui::TextureId::default(),
                };
                painter.add(egui::Shape::mesh(mesh));
            }
            BackgroundStyle::HorizontalGradient {
                left_color,
                right_color,
            } => {
                // Draw horizontal gradient using a mesh
                let mesh = egui::Mesh {
                    indices: vec![0, 1, 2, 2, 3, 0],
                    vertices: vec![
                        egui::epaint::Vertex {
                            pos: rect.left_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: left_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: right_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: right_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.left_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: left_color,
                        },
                    ],
                    texture_id: egui::TextureId::default(),
                };
                painter.add(egui::Shape::mesh(mesh));
            }
        }
    }

    /// Paints the symbol watermark: the custom text at the configured size, else the symbol and
    /// the interval at twice that size with, when asked for, the instrument's description below
    /// them at 11/12 of it.
    pub(super) fn draw_watermark(&self, ui: &Ui, painter: &egui::Painter, rect: Rect) {
        if !self.config.show_watermark {
            return;
        }
        let size = self.config.watermark_font_size;
        let line = |text: &str, size: f32| {
            rendering::WatermarkLine::new(
                text,
                crate::styles::typography::font_size(size),
                self.config.watermark_color,
            )
        };
        let lines = if let Some(custom) = &self.config.watermark_text {
            custom.lines().map(|text| line(text, size)).collect()
        } else if self.symbol.is_empty() {
            Vec::new()
        } else {
            let title = match self.timeframe {
                Some(timeframe) => format!("{}, {timeframe}", self.symbol),
                None => self.symbol.clone(),
            };
            let description = self
                .config
                .watermark_description
                .then(|| self.instrument()?.description.as_deref())
                .flatten()
                .filter(|text| !text.is_empty());
            std::iter::once(line(&title, size * 2.0))
                .chain(description.map(|text| line(text, size * 11.0 / 12.0)))
                .collect()
        };
        let align = match self.config.watermark_pos {
            WatermarkPos::Center => egui::Align2::CENTER_CENTER,
            WatermarkPos::TopLeft => egui::Align2::LEFT_TOP,
            WatermarkPos::TopRight => egui::Align2::RIGHT_TOP,
            WatermarkPos::BottomLeft => egui::Align2::LEFT_BOTTOM,
            WatermarkPos::BottomRight => egui::Align2::RIGHT_BOTTOM,
        };
        let watermark = rendering::Watermark {
            lines,
            align,
            padding: 20.0,
            ..Default::default()
        };
        rendering::render_watermark(ui, painter, rect, &watermark);
    }
}
