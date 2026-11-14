//! Host watermarks: lines of text and an image, aligned within any chart pane.

use crate::config::FontWeight;
use egui::emath::GuiRounding as _;
use egui::{Align2, Color32, FontFamily, FontId, Painter, Rect, Shape, Ui, Vec2};

/// One line of text of a [`Watermark`].
#[derive(Clone, Debug, PartialEq)]
pub struct WatermarkLine {
    /// Text, on one line.
    pub text: String,
    /// Font size in points; the line is 1.2 times as tall.
    pub size: f32,
    /// Font family, proportional by default. Named families must already be installed in the
    /// host's [`egui::FontDefinitions`]. Independent of the chart's axis family.
    pub family: FontFamily,
    /// Default [`FontWeight::Normal`].
    pub weight: FontWeight,
    /// Colour, usually translucent.
    pub color: Color32,
}

impl WatermarkLine {
    /// Creates a proportional line of normal weight.
    pub fn new(text: impl Into<String>, size: f32, color: Color32) -> Self {
        Self {
            text: text.into(),
            size,
            family: FontFamily::Proportional,
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

    /// Sets the font family from the fonts installed by the host.
    #[must_use]
    pub fn with_family(mut self, family: FontFamily) -> Self {
        self.family = family;
        self
    }
}

/// An image and lines of text painted behind the series, stacked in that order and aligned
/// together in a pane; see [`Chart::set_pane_watermark`](crate::Chart::set_pane_watermark).
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
    let painter = &painter.with_clip_rect(rect.intersect(painter.clip_rect()));
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
                FontId::new(line.size, line.family.clone()),
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
        let image_rect = row(top, size).round_to_pixels(ui.pixels_per_point());
        if let Ok(egui::load::TexturePoll::Ready { texture }) =
            image.load_for_size(ui.ctx(), inner.size())
        {
            egui::paint_texture_at(painter, image_rect, image.image_options(), &texture);
        }
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
    use crate::widget::test_frames::{chart, shapes};
    use egui::load::SizedTexture;

    /// With 10 points of padding, a 100 by 50 image, a 40 point bold line (48 tall) and a 20
    /// point line (24 tall) form a block 122 tall. Top, centre and bottom alignment put its top
    /// at the padded pane's top, 61 above its centre and 122 above its bottom; each part sits on
    /// its left edge, centre or right edge, and each line is centred in its row. The bold line is
    /// painted twice, 0.6 points apart.
    #[test]
    fn watermark_stacks_its_image_and_lines_at_the_alignment() {
        let (bold, plain) = (Color32::from_rgb(1, 2, 3), Color32::from_rgb(4, 5, 6));
        let texture = egui::TextureId::User(7);
        let ctx = egui::Context::default();
        let family = FontFamily::Name("Host Mono".into());
        let mut fonts = egui::FontDefinitions::default();
        fonts.families.insert(
            family.clone(),
            fonts.families[&FontFamily::Monospace].clone(),
        );
        ctx.set_fonts(fonts);
        let mut chart = chart(&[100.0], &[110.0]);
        chart.config_mut().axis_font_family = FontFamily::Monospace;
        for (align, custom) in [
            (Align2::LEFT_TOP, false),
            (Align2::CENTER_CENTER, false),
            (Align2::RIGHT_BOTTOM, false),
            (Align2::LEFT_TOP, true),
            (Align2::CENTER_CENTER, true),
            (Align2::RIGHT_BOTTOM, true),
        ] {
            let title = WatermarkLine::new("AAPL", 40.0, bold).with_weight(FontWeight::Bold);
            let detail = WatermarkLine::new("Apple Inc", 20.0, plain);
            let title = if custom {
                title.with_family(family.clone())
            } else {
                title
            };
            let detail = if custom {
                detail.with_family(FontFamily::Monospace)
            } else {
                detail
            };
            chart.set_watermark(Some(
                Watermark::default()
                    .with_image(
                        egui::Image::from_texture(SizedTexture::new(
                            texture,
                            Vec2::new(200.0, 100.0),
                        ))
                        .max_size(Vec2::new(100.0, 50.0)),
                    )
                    .with_line(title.clone())
                    .with_line(detail.clone())
                    .with_align(align)
                    .with_padding(10.0),
            ));
            let painted = shapes(&ctx, &mut chart, 0.0, Vec::new());
            let pane = chart.last_rendered_price_rect.shrink(10.0);
            let top = match align.y() {
                egui::Align::Min => pane.top(),
                egui::Align::Center => pane.center().y - 61.0,
                egui::Align::Max => pane.bottom() - 122.0,
            };
            let edge = |part: Rect| match align.x() {
                egui::Align::Min => part.left() - pane.left(),
                egui::Align::Center => part.center().x - pane.center().x,
                egui::Align::Max => part.right() - pane.right(),
            };
            let image = painted
                .iter()
                .find(|shape| shape.texture_id() == texture)
                .expect("the image")
                .visual_bounding_rect();
            assert_eq!(image.size(), Vec2::new(100.0, 50.0), "{align:?}");
            // The image snaps to whole pixels, so a centred image may move half a pixel.
            assert!((image.top() - top).abs() <= 0.5, "{align:?}");
            assert!(edge(image).abs() < 0.01, "{align:?}");
            for (color, centre, copies) in
                [(bold, top + 50.0 + 24.0, 2), (plain, top + 122.0 - 12.0, 1)]
            {
                // The layout box, not the glyphs' ink.
                let texts: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Text(t) if t.fallback_color == color => {
                            let line = if color == bold { &title } else { &detail };
                            assert_eq!(
                                t.galley.job.sections[0].format.font_id,
                                FontId::new(line.size, line.family.clone())
                            );
                            Some(t.galley.rect.translate(t.pos.to_vec2()))
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(texts.len(), copies, "{align:?}");
                assert!((texts[0].center().y - centre).abs() < 0.01, "{align:?}");
                assert!(edge(texts[copies - 1]).abs() < 0.01, "{align:?}");
            }
        }
    }
}
