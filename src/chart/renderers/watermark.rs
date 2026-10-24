//! Host watermarks: lines of text and an image, aligned anywhere in the price pane.

use crate::drawings::FontWeight;
use egui::{Align2, Color32, FontId, Painter, Rect, Shape, Ui, Vec2};

/// One line of text of a [`Watermark`].
#[derive(Clone, Debug, PartialEq)]
pub struct WatermarkLine {
    /// Text, on one line.
    pub text: String,
    /// Font size in points; the line is 1.2 times as tall.
    pub size: f32,
    /// Default [`FontWeight::Normal`].
    pub weight: FontWeight,
    /// Colour, usually translucent.
    pub color: Color32,
}

impl WatermarkLine {
    /// Creates a line of normal weight.
    pub fn new(text: impl Into<String>, size: f32, color: Color32) -> Self {
        Self {
            text: text.into(),
            size,
            weight: FontWeight::Normal,
            color,
        }
    }

    /// Sets the weight.
    #[must_use]
    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }
}

/// An image and lines of text painted behind the series, stacked in that order and aligned
/// together in the price pane; see [`Chart::set_watermark`](crate::Chart::set_watermark).
#[derive(Clone, Debug)]
pub struct Watermark {
    /// Lines of text, top to bottom.
    pub lines: Vec<WatermarkLine>,
    /// Image above the lines. By default it is scaled to fit the pane less the padding,
    /// keeping its aspect ratio; [`egui::Image::max_size`] limits it and
    /// [`egui::Image::tint`] makes it translucent.
    pub image: Option<egui::Image<'static>>,
    /// Where the image and the lines sit in the pane, and how the lines align with each
    /// other. Default [`Align2::CENTER_CENTER`].
    pub align: Align2,
    /// Distance from the pane's edges in points. Default 0.
    pub padding: f32,
}

impl Default for Watermark {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            image: None,
            align: Align2::CENTER_CENTER,
            padding: 0.0,
        }
    }
}

impl Watermark {
    /// Adds a line below the others.
    #[must_use]
    pub fn with_line(mut self, line: WatermarkLine) -> Self {
        self.lines.push(line);
        self
    }

    /// Sets the image.
    #[must_use]
    pub fn with_image(mut self, image: egui::Image<'static>) -> Self {
        self.image = Some(image);
        self
    }

    /// Sets the alignment.
    #[must_use]
    pub fn with_align(mut self, align: Align2) -> Self {
        self.align = align;
        self
    }

    /// Sets the distance from the pane's edges.
    #[must_use]
    pub fn with_padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }
}

/// Paints `watermark` in `rect`. An image still loading is left out until it has loaded.
pub(crate) fn render_watermark(ui: &Ui, painter: &Painter, rect: Rect, watermark: &Watermark) {
    let inner = rect.shrink(watermark.padding.max(0.0));
    if !inner.is_positive() {
        return;
    }
    let image = watermark.image.as_ref().and_then(|image| {
        let size = image.load_and_calc_size(ui, inner.size())?;
        Some((image, size))
    });
    let galleys: Vec<_> = watermark
        .lines
        .iter()
        .filter(|line| line.size.is_finite() && line.size > 0.0)
        .map(|line| {
            let galley = painter.layout_no_wrap(
                line.text.clone(),
                FontId::proportional(line.size),
                line.color,
            );
            (line, galley)
        })
        .collect();
    let block = galleys.iter().fold(
        image.map_or(Vec2::ZERO, |(_, size)| size),
        |block, (line, galley)| Vec2::new(block.x.max(galley.size().x), block.y + 1.2 * line.size),
    );
    let block = watermark.align.align_size_within_rect(block, inner);
    let row = |top: f32, size: Vec2| {
        let column = Rect::from_min_max(egui::pos2(block.left(), top), block.right_bottom());
        Align2([watermark.align.x(), egui::Align::Min]).align_size_within_rect(size, column)
    };
    let mut top = block.top();
    if let Some((image, size)) = image {
        image.paint_at(ui, row(top, size));
        top += size.y;
    }
    for (line, galley) in galleys {
        let height = 1.2 * line.size;
        let at = row(top + (height - galley.size().y) / 2.0, galley.size()).min;
        let text = Shape::galley(at, galley.clone(), line.color);
        if line.weight == FontWeight::Bold {
            // Synthetic weight, as drawn text uses, keeps host font families intact.
            painter.add(Shape::galley(at + Vec2::new(0.6, 0.0), galley, line.color));
        }
        painter.add(text);
        top += height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, load::SizedTexture};

    /// In a 400 by 300 pane with 10 points of padding, a 100 by 50 image, a 40 point bold line
    /// (48 tall) and a 20 point line (24 tall) form a block 122 tall. Top, centre and bottom
    /// alignment put its top at 10, 150 - 61 = 89 and 290 - 122 = 168; each part sits on the
    /// left edge at 10, on the centre at 200 or on the right edge at 390, and each line is
    /// centred in its row. The bold line is painted twice, 0.6 points apart.
    #[test]
    fn watermark_stacks_its_image_and_lines_at_the_alignment() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0));
        let (bold, plain) = (Color32::from_rgb(1, 2, 3), Color32::from_rgb(4, 5, 6));
        for (align, top, x) in [
            (Align2::LEFT_TOP, 10.0, egui::Align::Min),
            (Align2::CENTER_CENTER, 89.0, egui::Align::Center),
            (Align2::RIGHT_BOTTOM, 168.0, egui::Align::Max),
        ] {
            let watermark = Watermark::default()
                .with_image(
                    egui::Image::from_texture(SizedTexture::new(
                        egui::TextureId::User(7),
                        Vec2::new(200.0, 100.0),
                    ))
                    .max_size(Vec2::new(100.0, 50.0)),
                )
                .with_line(WatermarkLine::new("AAPL", 40.0, bold).with_weight(FontWeight::Bold))
                .with_line(WatermarkLine::new("Apple Inc", 20.0, plain))
                .with_align(align)
                .with_padding(10.0);
            let ctx = egui::Context::default();
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| {
                    let painter = ui.painter().clone();
                    render_watermark(ui, &painter, rect, &watermark);
                },
            );
            let bounds: Vec<_> = output
                .shapes
                .iter()
                .map(|clipped| {
                    let bounds = match &clipped.shape {
                        // The layout box, not the glyphs' ink.
                        Shape::Text(t) => t.galley.rect.translate(t.pos.to_vec2()),
                        shape => shape.visual_bounding_rect(),
                    };
                    (clipped.shape.clone(), bounds)
                })
                .collect();
            output.drop_without_applying_deltas();
            let edge = |part: Rect| match x {
                egui::Align::Min => part.left() - 10.0,
                egui::Align::Center => part.center().x - 200.0,
                egui::Align::Max => part.right() - 390.0,
            };
            let image = bounds
                .iter()
                .find(|(shape, _)| !matches!(shape, Shape::Text(_)))
                .expect("the image")
                .1;
            assert_eq!(image.size(), Vec2::new(100.0, 50.0), "{align:?}");
            assert_eq!((image.top(), edge(image)), (top, 0.0), "{align:?}");
            for (color, centre, copies) in
                [(bold, top + 50.0 + 24.0, 2), (plain, top + 122.0 - 12.0, 1)]
            {
                let texts: Vec<_> = bounds
                    .iter()
                    .filter(
                        |(shape, _)| matches!(shape, Shape::Text(t) if t.fallback_color == color),
                    )
                    .map(|(_, rect)| *rect)
                    .collect();
                assert_eq!(texts.len(), copies, "{align:?}");
                assert!((texts[0].center().y - centre).abs() < 0.01, "{align:?}");
                assert!(edge(texts[copies - 1]).abs() < 0.01, "{align:?}");
            }
        }
    }
}
