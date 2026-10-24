//! Image loader that rasterises [`Icon`](super::Icon)s with pixel-aligned strokes.
//!
//! A plain SVG rasteriser scales the 1.5 unit stroke of the 24 unit grid with the icon, so at the 11 to 32 point
//! sizes the interface uses, strokes fall between pixel rows and blur into two or three partly covered pixels. This
//! loader draws every stroke a whole number of physical pixels wide, one logical pixel for the nominal 1.5 unit
//! weight, and moves horizontal and vertical segments onto the pixel grid. Fills are drawn as authored.

use std::sync::Arc;

use egui::load::{BytesPoll, ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint};
use egui::{ColorImage, Context};
use resvg::tiny_skia::{
    FillRule, Paint, Path, PathBuilder, PathSegment, Pixmap, Point, StrokeDash, Transform,
};
use resvg::usvg;

/// URI prefix of icon images: `bytes://egui_charts/icon/<logical width>/<name>`.
pub(super) const URI_PREFIX: &str = "bytes://egui_charts/icon/";

/// Stroke width, in grid units, that draws one logical pixel wide.
const NOMINAL_STROKE: f32 = 1.5;

/// Registers the icon loader on `ctx` once.
pub(crate) fn install(ctx: &Context) {
    if ctx.is_loader_installed(IconLoader::ID) {
        return;
    }
    ctx.add_image_loader(Arc::new(IconLoader));
    // Icons painted earlier went through the generic SVG loader, whose textures would otherwise stay cached.
    ctx.forget_all_images();
}

struct IconLoader;

impl IconLoader {
    const ID: &'static str = egui::generate_loader_id!(IconLoader);
}

impl ImageLoader for IconLoader {
    fn id(&self) -> &str {
        Self::ID
    }

    fn load(&self, ctx: &Context, uri: &str, size_hint: SizeHint) -> ImageLoadResult {
        let Some(logical_width) = uri
            .strip_prefix(URI_PREFIX)
            .and_then(|rest| rest.split_once('/'))
            .and_then(|(width, _)| width.parse::<f32>().ok())
        else {
            return Err(LoadError::NotSupported);
        };
        match ctx.try_load_bytes(uri)? {
            BytesPoll::Ready { bytes, .. } => {
                let image =
                    rasterize(&bytes, logical_width, size_hint).map_err(LoadError::Loading)?;
                Ok(ImagePoll::Ready {
                    image: Arc::new(image),
                })
            }
            BytesPoll::Pending { size } => Ok(ImagePoll::Pending { size }),
        }
    }

    // The texture loader caches the result, so there is nothing to forget here.
    fn forget(&self, _uri: &str) {}

    fn forget_all(&self) {}

    fn byte_size(&self) -> usize {
        0
    }
}

/// Rasterises an icon shown `logical_width` points wide at the size `size_hint` asks for.
fn rasterize(svg: &[u8], logical_width: f32, size_hint: SizeHint) -> Result<ColorImage, String> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let source = tree.size();
    // Icons keep their aspect ratio inside the requested box, which is `logical_width` points wide.
    let (scale, box_width) = match size_hint {
        SizeHint::Size { width, height, .. } => (
            (width as f32 / source.width()).min(height as f32 / source.height()),
            width as f32,
        ),
        SizeHint::Width(width) => (width as f32 / source.width(), width as f32),
        SizeHint::Height(height) => {
            let scale = height as f32 / source.height();
            (scale, source.width() * scale)
        }
        SizeHint::Scale(scale) => (scale.into_inner(), source.width() * scale.into_inner()),
    };
    let (width, height) = (
        (source.width() * scale).round() as u32,
        (source.height() * scale).round() as u32,
    );
    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| format!("Failed to create SVG Pixmap of size {width}x{height}"))?;
    let to_pixels = Transform::from_scale(scale, scale);
    if is_plain(tree.root()) {
        let pixels_per_point = box_width / logical_width;
        draw_group(tree.root(), to_pixels, pixels_per_point, &mut pixmap);
    } else {
        resvg::render(&tree, to_pixels, &mut pixmap.as_mut());
    }
    Ok(
        ColorImage::from_rgba_premultiplied([width as _, height as _], pixmap.data())
            .with_source_size(egui::vec2(source.width(), source.height())),
    )
}

/// Whether `group` holds only solid-colour paths without group opacity, clipping, masks or filters, which
/// [`draw_group`] renders exactly; anything else is left to resvg.
fn is_plain(group: &usvg::Group) -> bool {
    let solid = |paint: &usvg::Paint| matches!(paint, usvg::Paint::Color(_));
    !group.should_isolate()
        && group.children().iter().all(|node| match node {
            usvg::Node::Group(group) => is_plain(group),
            usvg::Node::Path(path) => {
                path.fill().is_none_or(|fill| solid(fill.paint()))
                    && path.stroke().is_none_or(|stroke| solid(stroke.paint()))
            }
            _ => false,
        })
}

fn draw_group(
    group: &usvg::Group,
    to_pixels: Transform,
    pixels_per_point: f32,
    pixmap: &mut Pixmap,
) {
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => draw_group(group, to_pixels, pixels_per_point, pixmap),
            usvg::Node::Path(path) if path.is_visible() => {
                let transform = path.abs_transform().post_concat(to_pixels);
                if path.paint_order() == usvg::PaintOrder::FillAndStroke {
                    fill(path, transform, pixmap);
                    stroke(path, transform, to_pixels.sx, pixels_per_point, pixmap);
                } else {
                    stroke(path, transform, to_pixels.sx, pixels_per_point, pixmap);
                    fill(path, transform, pixmap);
                }
            }
            _ => {}
        }
    }
}

fn solid_paint(paint: &usvg::Paint, opacity: usvg::Opacity) -> Paint<'static> {
    let mut solid = Paint::default();
    if let usvg::Paint::Color(c) = paint {
        solid.set_color_rgba8(c.red, c.green, c.blue, opacity.to_u8());
    }
    solid.anti_alias = true;
    solid
}

fn fill(path: &usvg::Path, transform: Transform, pixmap: &mut Pixmap) {
    let Some(fill) = path.fill() else { return };
    let rule = match fill.rule() {
        usvg::FillRule::NonZero => FillRule::Winding,
        usvg::FillRule::EvenOdd => FillRule::EvenOdd,
    };
    let paint = solid_paint(fill.paint(), fill.opacity());
    pixmap.fill_path(path.data(), &paint, rule, transform, None);
}

/// Strokes `path` a whole number of physical pixels wide, in proportion to its authored width.
fn stroke(
    path: &usvg::Path,
    transform: Transform,
    scale: f32,
    pixels_per_point: f32,
    pixmap: &mut Pixmap,
) {
    let Some(stroke) = path.stroke() else { return };
    let Some(pixels) = path.data().clone().transform(transform) else {
        return;
    };
    let mut style = stroke.to_tiny_skia();
    style.width = (stroke.width().get() / NOMINAL_STROKE * pixels_per_point)
        .round()
        .max(1.0);
    // Dashes keep their length relative to the icon.
    style.dash = stroke.dasharray().and_then(|dashes| {
        StrokeDash::new(
            dashes.iter().map(|d| d * scale).collect(),
            stroke.dashoffset() * scale,
        )
    });
    let Some(pixels) = snap_to_pixels(&pixels, style.width) else {
        return;
    };
    let paint = solid_paint(stroke.paint(), stroke.opacity());
    pixmap.stroke_path(&pixels, &paint, &style, Transform::identity(), None);
}

/// Moves the horizontal and vertical line segments of `path` so that a stroke `width` pixels wide covers whole
/// pixels. Curve handles move with the end point they belong to, so joined curves keep their tangents.
fn snap_to_pixels(path: &Path, width: f32) -> Option<Path> {
    let snap = |v: f32| (v - width / 2.0).round() + width / 2.0;
    let segments: Vec<PathSegment> = path.segments().collect();
    // Index of the segment whose end point each segment ends on; a close ends on its subpath's move.
    let mut end = Vec::with_capacity(segments.len());
    let mut start = 0;
    for (i, segment) in segments.iter().enumerate() {
        if matches!(segment, PathSegment::MoveTo(_)) {
            start = i;
        }
        end.push(if matches!(segment, PathSegment::Close) {
            start
        } else {
            i
        });
    }
    let point = |i: usize| match segments[end[i]] {
        PathSegment::MoveTo(p)
        | PathSegment::LineTo(p)
        | PathSegment::QuadTo(_, p)
        | PathSegment::CubicTo(_, _, p) => p,
        PathSegment::Close => unreachable!("a close ends on its subpath's move"),
    };
    let mut shift = vec![Point::zero(); segments.len()];
    for i in 1..segments.len() {
        if !matches!(segments[i], PathSegment::LineTo(_) | PathSegment::Close) {
            continue;
        }
        let (a, b) = (end[i - 1], end[i]);
        let (pa, pb) = (point(a), point(b));
        // A zero-length segment, such as the close after a circle's last curve, has no direction.
        if (pa.x - pb.x).abs() < 1e-3 && (pa.y - pb.y).abs() < 1e-3 {
            continue;
        }
        if (pa.y - pb.y).abs() < 1e-3 {
            shift[a].y = snap(pa.y) - pa.y;
            shift[b].y = snap(pb.y) - pb.y;
        }
        if (pa.x - pb.x).abs() < 1e-3 {
            shift[a].x = snap(pa.x) - pa.x;
            shift[b].x = snap(pb.x) - pb.x;
        }
    }
    let mut builder = PathBuilder::new();
    for (i, segment) in segments.iter().enumerate() {
        let to = point(i) + shift[end[i]];
        match *segment {
            PathSegment::MoveTo(_) => builder.move_to(to.x, to.y),
            PathSegment::LineTo(_) => builder.line_to(to.x, to.y),
            PathSegment::QuadTo(c, _) => {
                let mid = Point::from_xy(
                    (shift[end[i - 1]].x + shift[end[i]].x) / 2.0,
                    (shift[end[i - 1]].y + shift[end[i]].y) / 2.0,
                );
                let c = c + mid;
                builder.quad_to(c.x, c.y, to.x, to.y);
            }
            PathSegment::CubicTo(c1, c2, _) => {
                let (c1, c2) = (c1 + shift[end[i - 1]], c2 + shift[end[i]]);
                builder.cubic_to(c1.x, c1.y, c2.x, c2.y, to.x, to.y);
            }
            PathSegment::Close => builder.close(),
        }
    }
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::icons;

    /// A straight 1.5 unit stroke covers whole pixels, one row or column per logical pixel, at every icon size.
    #[test]
    fn straight_strokes_cover_whole_pixels() {
        for (icon, vertical) in [
            (icons::HORIZONTAL_LINE, false),
            (icons::VERTICAL_LINE, true),
        ] {
            for (size, pixels_per_point) in [
                (16.0, 1.0),
                (18.0, 1.0),
                (20.0, 1.0),
                (24.0, 1.0),
                (18.0, 2.0),
            ] {
                let n = (size * pixels_per_point) as usize;
                let hint = SizeHint::Size {
                    width: n as u32,
                    height: n as u32,
                    maintain_aspect_ratio: true,
                };
                let image = rasterize(icon.svg_bytes(), size, hint).unwrap();
                // Cross the left or top segment (2.75 to 10.5 units) a quarter of the way along the icon.
                let alphas: Vec<u8> = (0..n)
                    .map(|j| if vertical { (j, n / 4) } else { (n / 4, j) })
                    .map(|(x, y)| image.pixels[y * n + x].a())
                    .filter(|&a| a > 0)
                    .collect();
                assert_eq!(
                    alphas,
                    vec![255; pixels_per_point as usize],
                    "{} at {size} pt and {pixels_per_point}x",
                    icon.name()
                );
            }
        }
    }
    /// Curves stay where they are authored, including the point a closed curve ends on.
    #[test]
    fn closed_curves_keep_their_shape() {
        let circle = PathBuilder::from_circle(10.3, 10.3, 5.2).unwrap();
        let snapped = snap_to_pixels(&circle, 1.0).unwrap();
        assert!(circle.segments().eq(snapped.segments()));
    }
}
