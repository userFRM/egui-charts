use super::Chart;
use crate::chart::rendering;
use crate::config::{BackgroundStyle, WatermarkPos};
use egui::{Color32, Pos2, Rect, Ui};

/// The colour at `pos` of a background of `style` whose gradient spans `frame`; `color` when it
/// is solid.
pub(in crate::widget) fn background_at(
    frame: Rect,
    style: BackgroundStyle,
    color: Color32,
    pos: Pos2,
) -> Color32 {
    match style {
        BackgroundStyle::Solid => color,
        BackgroundStyle::VerticalGradient {
            top_color,
            bottom_color,
        } => top_color.lerp_to_gamma(
            bottom_color,
            egui::remap_clamp(pos.y, frame.y_range(), 0.0..=1.0),
        ),
        BackgroundStyle::HorizontalGradient {
            left_color,
            right_color,
        } => left_color.lerp_to_gamma(
            right_color,
            egui::remap_clamp(pos.x, frame.x_range(), 0.0..=1.0),
        ),
    }
}

/// Fills `rect` with the part behind it of a background of `style` whose gradient spans `frame`,
/// so that rects filled one by one join without a seam; `color` when it is solid.
pub(in crate::widget) fn paint_background(
    painter: &egui::Painter,
    frame: Rect,
    rect: Rect,
    style: BackgroundStyle,
    color: Color32,
) {
    if style == BackgroundStyle::Solid {
        painter.rect_filled(rect, 0.0, color);
        return;
    }
    let vertices = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ]
    .map(|pos| egui::epaint::Vertex {
        pos,
        uv: egui::epaint::WHITE_UV,
        color: background_at(frame, style, color, pos),
    });
    painter.add(egui::Shape::mesh(egui::Mesh {
        indices: vec![0, 1, 2, 2, 3, 0],
        vertices: vertices.to_vec(),
        texture_id: egui::TextureId::default(),
    }));
}

impl Chart {
    /// The background colour at `pos` within the frame painted by [`Self::draw_background`],
    /// or `None` when something else paints it.
    pub(super) fn plot_background(&self, rect: Rect, pos: Pos2) -> Option<Color32> {
        (!self.config.skip_background).then(|| {
            background_at(
                rect,
                self.config.background_style,
                self.config.background_color,
                pos,
            )
        })
    }

    /// Draw the chart background (solid or gradient)
    pub(super) fn draw_background(&self, painter: &egui::Painter, rect: Rect) {
        // Skip background when chart is inside a container that handles its own background
        if self.config.skip_background {
            return;
        }
        paint_background(
            painter,
            rect,
            rect,
            self.config.background_style,
            self.config.background_color,
        );
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
