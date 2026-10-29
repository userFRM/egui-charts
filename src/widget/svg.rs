//! Vector output of exported chart frames, from the shapes before tessellation.

use super::raster::{Texture, png, textures};
use egui::epaint::{ClippedShape, Mesh, PathStroke, Shape, StrokeKind, TextShape, Vertex};
use egui::{Color32, Pos2, Rect, Stroke, TextureId, TexturesDelta, emath::TSTransform};
use std::collections::HashSet;
use std::fmt::Write;

/// Renders `shapes` as an SVG document of `size` points, embedding the textures that `delta`
/// sets as PNG. Fails on shapes the format cannot take over unchanged: host callbacks, host
/// textures without supplied pixels, rotated or underlined text, gradient strokes and non-linear
/// mesh colours.
pub(super) fn svg(
    size: [u32; 2],
    shapes: &[ClippedShape],
    delta: &TexturesDelta,
) -> Result<String, String> {
    let mut out = Svg {
        textures: textures(delta),
        defs: String::new(),
        body: String::new(),
        embedded: HashSet::new(),
        ids: 0,
    };
    let mut clip = None;
    for clipped in shapes {
        if clipped.transform_after_tessellation != TSTransform::IDENTITY {
            return Err("Shapes with a transform cannot be written as SVG".into());
        }
        if clip != Some(clipped.clip_rect) {
            if clip.is_some() {
                out.body.push_str("</g>");
            }
            let id = out.id();
            let r = clipped.clip_rect;
            let _ = write!(
                out.defs,
                r#"<clipPath id="c{id}"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath>"#,
                r.min.x,
                r.min.y,
                r.width(),
                r.height()
            );
            let _ = write!(out.body, r#"<g clip-path="url(#c{id})">"#);
            clip = Some(clipped.clip_rect);
        }
        out.shape(&clipped.shape)?;
    }
    if clip.is_some() {
        out.body.push_str("</g>");
    }
    let [width, height] = size;
    Ok(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><defs>{}</defs>{}</svg>"#,
        out.defs, out.body
    ))
}

/// Consecutive glyphs of one row that share colour, size and family.
struct Run {
    rgb: String,
    opacity: f32,
    size: f32,
    family: String,
    xs: Vec<f32>,
    y: f32,
    chars: String,
}

struct Svg {
    textures: std::collections::HashMap<TextureId, Texture>,
    defs: String,
    body: String,
    embedded: HashSet<(TextureId, Color32)>,
    ids: usize,
}

/// `rgb()` and the opacity of a premultiplied colour, or `None` for a transparent one.
fn color(color: Color32) -> Option<(String, f32)> {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    (a > 0).then(|| (format!("rgb({r},{g},{b})"), f32::from(a) / 255.0))
}

fn fill(c: Color32) -> String {
    color(c).map_or_else(
        || r#"fill="none""#.to_owned(),
        |(rgb, opacity)| format!(r#"fill="{rgb}" fill-opacity="{opacity}""#),
    )
}

fn stroke(stroke: Stroke) -> String {
    match color(stroke.color).filter(|_| stroke.width > 0.0) {
        Some((rgb, opacity)) => format!(
            r#"stroke="{rgb}" stroke-opacity="{opacity}" stroke-width="{}" stroke-linejoin="round" stroke-linecap="round""#,
            stroke.width
        ),
        None => r#"stroke="none""#.to_owned(),
    }
}

fn path_stroke(stroke_: &PathStroke) -> Result<String, String> {
    match stroke_.color {
        egui::epaint::ColorMode::Solid(c) => Ok(stroke(Stroke::new(stroke_.width, c))),
        egui::epaint::ColorMode::UV(_) => Err("Gradient strokes cannot be written as SVG".into()),
    }
}

fn points(points: &[Pos2]) -> String {
    points
        .iter()
        .map(|p| format!("{},{}", p.x, p.y))
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl Svg {
    fn id(&mut self) -> usize {
        self.ids += 1;
        self.ids
    }

    fn shape(&mut self, shape: &Shape) -> Result<(), String> {
        match shape {
            Shape::Noop => {}
            Shape::Vec(shapes) => {
                for shape in shapes {
                    self.shape(shape)?;
                }
            }
            Shape::Circle(c) => {
                let _ = write!(
                    self.body,
                    r#"<circle cx="{}" cy="{}" r="{}" {} {}/>"#,
                    c.center.x,
                    c.center.y,
                    c.radius,
                    fill(c.fill),
                    stroke(c.stroke)
                );
            }
            Shape::Ellipse(e) => {
                let _ = write!(
                    self.body,
                    r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" transform="rotate({} {} {})" {} {}/>"#,
                    e.center.x,
                    e.center.y,
                    e.radius.x,
                    e.radius.y,
                    e.angle.to_degrees(),
                    e.center.x,
                    e.center.y,
                    fill(e.fill),
                    stroke(e.stroke)
                );
            }
            Shape::LineSegment {
                points: p,
                stroke: s,
            } => {
                let _ = write!(
                    self.body,
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" {}/>"#,
                    p[0].x,
                    p[0].y,
                    p[1].x,
                    p[1].y,
                    stroke(*s)
                );
            }
            Shape::Path(p) => {
                let tag = if p.closed { "polygon" } else { "polyline" };
                let fill = if p.closed {
                    fill(p.fill)
                } else {
                    r#"fill="none""#.to_owned()
                };
                let _ = write!(
                    self.body,
                    r#"<{tag} points="{}" {fill} {}/>"#,
                    points(&p.points),
                    path_stroke(&p.stroke)?
                );
            }
            Shape::QuadraticBezier(b) => {
                let [a, c, d] = b.points;
                self.curve(
                    format!("M{},{} Q{},{} {},{}", a.x, a.y, c.x, c.y, d.x, d.y),
                    b.closed,
                    b.fill,
                    &b.stroke,
                )?;
            }
            Shape::CubicBezier(b) => {
                let [a, c, d, e] = b.points;
                self.curve(
                    format!(
                        "M{},{} C{},{} {},{} {},{}",
                        a.x, a.y, c.x, c.y, d.x, d.y, e.x, e.y
                    ),
                    b.closed,
                    b.fill,
                    &b.stroke,
                )?;
            }
            Shape::Rect(r) => {
                if r.angle != 0.0 {
                    return Err("Rotated rectangles cannot be written as SVG".into());
                }
                if let Some(brush) = &r.brush {
                    if !r.stroke.is_empty() || r.corner_radius != egui::CornerRadius::ZERO {
                        return Err("Outlined or rounded images cannot be written as SVG".into());
                    }
                    return self.image(brush.fill_texture_id, r.rect, brush.uv, r.fill);
                }
                let half = r.stroke.width / 2.0;
                let rect = match r.stroke_kind {
                    StrokeKind::Inside => r.rect.shrink(half),
                    StrokeKind::Middle => r.rect,
                    StrokeKind::Outside => r.rect.expand(half),
                };
                let c = r.corner_radius;
                let filter = if r.blur_width > 0.0 {
                    let id = self.id();
                    let _ = write!(
                        self.defs,
                        r#"<filter id="b{id}" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="{}"/></filter>"#,
                        r.blur_width / 2.0
                    );
                    format!(r#" filter="url(#b{id})""#)
                } else {
                    String::new()
                };
                let radii = [c.nw, c.ne, c.se, c.sw].map(f32::from);
                let _ = if radii.iter().all(|&r| r == radii[0]) {
                    write!(
                        self.body,
                        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" {} {}{filter}/>"#,
                        rect.min.x,
                        rect.min.y,
                        rect.width(),
                        rect.height(),
                        radii[0],
                        fill(r.fill),
                        stroke(r.stroke)
                    )
                } else {
                    let [nw, ne, se, sw] = radii;
                    let (l, t, rt, b) = (rect.min.x, rect.min.y, rect.max.x, rect.max.y);
                    write!(
                        self.body,
                        r#"<path d="M{},{t} H{} A{ne},{ne} 0 0 1 {rt},{} V{} A{se},{se} 0 0 1 {},{b} H{} A{sw},{sw} 0 0 1 {l},{} V{} A{nw},{nw} 0 0 1 {},{t} Z" {} {}{filter}/>"#,
                        l + nw,
                        rt - ne,
                        t + ne,
                        b - se,
                        rt - se,
                        l + sw,
                        b - sw,
                        t + nw,
                        l + nw,
                        fill(r.fill),
                        stroke(r.stroke)
                    )
                };
            }
            Shape::Text(text) => self.text(text)?,
            Shape::Mesh(mesh) => self.mesh(mesh)?,
            Shape::Band(_) => return Err("Bands cannot be written as SVG".into()),
            Shape::Callback(_) => {
                return Err("Host paint callbacks cannot be written as SVG".into());
            }
        }
        Ok(())
    }

    fn curve(
        &mut self,
        d: String,
        closed: bool,
        fill_: Color32,
        stroke_: &PathStroke,
    ) -> Result<(), String> {
        let _ = write!(
            self.body,
            r#"<path d="{d}{}" {} {}/>"#,
            if closed { " Z" } else { "" },
            fill(fill_),
            path_stroke(stroke_)?
        );
        Ok(())
    }

    /// One `<text>` per run of glyphs of one colour, size and family, each glyph at the position
    /// the layout gave it.
    fn text(&mut self, text: &TextShape) -> Result<(), String> {
        if text.angle != 0.0 {
            return Err("Rotated text cannot be written as SVG".into());
        }
        if !text.underline.is_empty() {
            return Err("Underlined text cannot be written as SVG".into());
        }
        let job = &text.galley.job;
        let ends: Vec<_> = job
            .sections
            .iter()
            .map(|s| (job.text[..s.byte_range.end.0].chars().count(), &s.format))
            .collect();
        let mut index = 0;
        for placed in &text.galley.rows {
            let row = &placed.row;
            let origin = text.pos + placed.pos.to_vec2();
            let mut run: Option<Run> = None;
            let flush = |body: &mut String, run: &mut Option<Run>| {
                if let Some(run) = run.take() {
                    let xs: Vec<_> = run.xs.iter().map(f32::to_string).collect();
                    let _ = write!(
                        body,
                        r#"<text x="{}" y="{}" font-family="{}" font-size="{}" fill="{}" fill-opacity="{}" xml:space="preserve">{}</text>"#,
                        xs.join(" "),
                        run.y,
                        run.family,
                        run.size,
                        run.rgb,
                        run.opacity,
                        escape(&run.chars)
                    );
                }
            };
            for glyph in &row.glyphs {
                let format = ends
                    .iter()
                    .find(|(end, _)| index < *end)
                    .or(ends.last())
                    .map(|(_, format)| *format);
                index += 1;
                let Some(format) = format else { continue };
                let vertex = row.visuals.mesh.vertices.get(glyph.first_vertex as usize);
                let mut tint = vertex.map_or(Color32::PLACEHOLDER, |v| v.color);
                if tint == Color32::PLACEHOLDER {
                    tint = text.fallback_color;
                } else if let Some(over) = text.override_text_color.filter(|_| !glyph.is_color) {
                    tint = over;
                }
                let Some((rgb, opacity)) = color(tint) else {
                    continue;
                };
                let opacity = opacity * text.opacity_factor;
                let family = match &format.font_id.family {
                    egui::FontFamily::Proportional => "sans-serif".to_owned(),
                    egui::FontFamily::Monospace => "monospace".to_owned(),
                    egui::FontFamily::Name(name) => format!("'{name}', sans-serif"),
                };
                let x = origin.x + glyph.pos.x;
                let y = origin.y + glyph.pos.y;
                let size = format.font_id.size;
                match &mut run {
                    Some(r)
                        if r.rgb == rgb
                            && r.opacity == opacity
                            && r.size == size
                            && r.family == family
                            && r.y == y =>
                    {
                        r.xs.push(x);
                        r.chars.push(glyph.chr);
                    }
                    _ => {
                        flush(&mut self.body, &mut run);
                        run = Some(Run {
                            rgb,
                            opacity,
                            size,
                            family,
                            xs: vec![x],
                            y,
                            chars: glyph.chr.to_string(),
                        });
                    }
                }
            }
            flush(&mut self.body, &mut run);
            if placed.ends_with_newline {
                index += 1;
            }
        }
        Ok(())
    }

    fn mesh(&mut self, mesh: &Mesh) -> Result<(), String> {
        if mesh.is_empty() {
            return Ok(());
        }
        if mesh.texture_id == TextureId::default() {
            for triangle in mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] = triangle.map(|i| mesh.vertices.get(i as usize));
                let (Some(a), Some(b), Some(c)) = (a, b, c) else {
                    continue;
                };
                self.triangle([*a, *b, *c])?;
            }
            Ok(())
        } else {
            self.quad(mesh)
        }
    }

    /// A triangle of the font atlas' white texel: a flat colour or a linear colour ramp.
    fn triangle(&mut self, v: [Vertex; 3]) -> Result<(), String> {
        let pts = points(&v.map(|v| v.pos));
        if v.iter().all(|x| x.color == v[0].color) {
            let _ = write!(
                self.body,
                r#"<polygon points="{pts}" {}/>"#,
                fill(v[0].color)
            );
            return Ok(());
        }
        // Colours along one line in colour space, as every fade of the chart is: t is the
        // position on that line, linear across the triangle, so one gradient reproduces it.
        let rgba = |c: Color32| c.to_array().map(f32::from);
        let c0 = rgba(v[0].color);
        let far = v
            .iter()
            .map(|v| rgba(v.color))
            .max_by(|a, b| dist(a, &c0).total_cmp(&dist(b, &c0)))
            .expect("three vertices");
        let axis: Vec<f32> = (0..4).map(|i| far[i] - c0[i]).collect();
        let len2: f32 = axis.iter().map(|a| a * a).sum();
        let t: Vec<f32> = v
            .iter()
            .map(|v| {
                let c = rgba(v.color);
                (0..4).map(|i| (c[i] - c0[i]) * axis[i]).sum::<f32>() / len2
            })
            .collect();
        let linear = v.iter().zip(&t).all(|(v, t)| {
            let c = rgba(v.color);
            (0..4).all(|i| (c0[i] + axis[i] * t - c[i]).abs() < 2.0)
        });
        // Solve t = t0 + g . (p - p0) for the gradient vector g.
        let (p0, p1, p2) = (v[0].pos, v[1].pos, v[2].pos);
        let det = (p1.x - p0.x) * (p2.y - p0.y) - (p2.x - p0.x) * (p1.y - p0.y);
        if !linear || det.abs() < 1e-9 {
            return Err(
                "Meshes whose vertex colours do not fade along one line cannot be written as SVG"
                    .into(),
            );
        }
        let (dt1, dt2) = (t[1] - t[0], t[2] - t[0]);
        let gx = (dt1 * (p2.y - p0.y) - dt2 * (p1.y - p0.y)) / det;
        let gy = (dt2 * (p1.x - p0.x) - dt1 * (p2.x - p0.x)) / det;
        let g2 = gx * gx + gy * gy;
        if g2 < 1e-12 {
            return Err(
                "Meshes whose vertex colours do not fade along one line cannot be written as SVG"
                    .into(),
            );
        }
        // Gradient from t = 0 at (x1, y1) to t = 1 at (x2, y2).
        let (x1, y1) = (p0.x - gx * t[0] / g2, p0.y - gy * t[0] / g2);
        let (x2, y2) = (x1 + gx / g2, y1 + gy / g2);
        // A transparent end takes the other end's colour, so the ramp fades one colour.
        let stop = |offset: u8, end: Color32, other: Color32| {
            let [r, g, b, _] = if end.a() == 0 { other } else { end }.to_srgba_unmultiplied();
            format!(
                r#"<stop offset="{offset}" stop-color="rgb({r},{g},{b})" stop-opacity="{}"/>"#,
                f32::from(end.a()) / 255.0
            )
        };
        let (start, end) = (
            v[0].color,
            Color32::from_rgba_premultiplied(
                far[0] as u8,
                far[1] as u8,
                far[2] as u8,
                far[3] as u8,
            ),
        );
        let id = self.id();
        let _ = write!(
            self.defs,
            r#"<linearGradient id="g{id}" gradientUnits="userSpaceOnUse" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}">{}{}</linearGradient>"#,
            stop(0, start, end),
            stop(1, end, start)
        );
        let _ = write!(self.body, r#"<polygon points="{pts}" fill="url(#g{id})"/>"#);
        Ok(())
    }

    /// A mesh of one axis-aligned textured quad.
    fn quad(&mut self, mesh: &Mesh) -> Result<(), String> {
        let unsupported = "Only untilted, unflipped image quads can be written as SVG";
        let v = &mesh.vertices;
        if v.len() != 4 || mesh.indices.len() != 6 {
            return Err(unsupported.into());
        }
        let pos = Rect::from_points(&v.iter().map(|v| v.pos).collect::<Vec<_>>());
        let uv = Rect::from_points(&v.iter().map(|v| v.uv).collect::<Vec<_>>());
        let upright = v.iter().all(|v| {
            (v.pos.x == pos.min.x) == (v.uv.x == uv.min.x)
                && (v.pos.y == pos.min.y) == (v.uv.y == uv.min.y)
                && (v.pos.x == pos.min.x || v.pos.x == pos.max.x)
                && (v.pos.y == pos.min.y || v.pos.y == pos.max.y)
        });
        if !upright || v.iter().any(|x| x.color != v[0].color) {
            return Err(unsupported.into());
        }
        self.image(mesh.texture_id, pos, uv, v[0].color)
    }

    /// The `uv` part of a frame texture in `pos`, multiplied by `tint`.
    fn image(&mut self, key: TextureId, pos: Rect, uv: Rect, tint: Color32) -> Result<(), String> {
        let Some(texture) = self.textures.get(&key) else {
            return Err(format!(
                "{key:?} is not a texture of the frame; host textures cannot be written as SVG without their pixels in ImageExportOptions::textures"
            ));
        };
        let id = format!(
            "{}_{:08x}",
            self.texture_ids(key),
            u32::from_be_bytes(tint.to_array())
        );
        let [tw, th] = texture.size.map(|side| side as f32);
        let crop = Rect::from_min_max(
            Pos2::new(uv.min.x * tw, uv.min.y * th),
            Pos2::new(uv.max.x * tw, uv.max.y * th),
        );
        if self.embedded.insert((key, tint)) {
            let pixels = &self.textures[&key];
            // Multiply premultiplied channels in gamma space, as the frame's renderer does.
            let tinted: Vec<_> = pixels.pixels.iter().map(|pixel| *pixel * tint).collect();
            if tinted
                .iter()
                .any(|pixel| pixel.a() == 0 && *pixel != Color32::TRANSPARENT)
            {
                return Err(format!("Additive texture {key:?} cannot be written as SVG"));
            }
            let data = png([pixels.size[0] as u32, pixels.size[1] as u32], &tinted)?;
            let rendering = if pixels.nearest {
                r#" style="image-rendering:pixelated""#
            } else {
                ""
            };
            let _ = write!(
                self.defs,
                r#"<image id="t{id}" width="{tw}" height="{th}"{rendering} href="data:image/png;base64,{}"/>"#,
                base64(&data)
            );
        }
        let _ = write!(
            self.body,
            r##"<svg x="{}" y="{}" width="{}" height="{}" viewBox="{} {} {} {}" preserveAspectRatio="none" overflow="hidden"><use href="#t{id}"/></svg>"##,
            pos.min.x,
            pos.min.y,
            pos.width(),
            pos.height(),
            crop.min.x,
            crop.min.y,
            crop.width(),
            crop.height()
        );
        Ok(())
    }

    /// Keep managed and user ids distinct, including user ids with their highest bit set.
    fn texture_ids(&self, id: TextureId) -> String {
        match id {
            TextureId::Managed(n) => format!("m{n}"),
            TextureId::User(n) => format!("u{n}"),
        }
    }
}

fn dist(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    (0..4).map(|i| (a[i] - b[i]).powi(2)).sum()
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, byte)| n | u32::from(*byte) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}
