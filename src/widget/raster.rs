//! CPU rasterisation of tessellated egui frames, for encoded chart images.

use egui::epaint::{ClippedPrimitive, Primitive, Vertex};
use egui::{Color32, ImageData, TextureFilter, TextureId, TexturesDelta};
use std::collections::HashMap;

/// A texture as the frame's deltas leave it.
struct Texture {
    size: [usize; 2],
    pixels: Vec<Color32>,
    nearest: bool,
}

impl Texture {
    /// Premultiplied colour at normalised `uv`, filtered as the texture options ask.
    /// Coordinates outside the texture clamp to its edge, the default wrap mode.
    fn sample(&self, uv: [f64; 2]) -> [f64; 4] {
        let [width, height] = self.size;
        let texel = |x: f64, y: f64| {
            let x = (x as isize).clamp(0, width as isize - 1) as usize;
            let y = (y as isize).clamp(0, height as isize - 1) as usize;
            self.pixels[y * width + x].to_array().map(f64::from)
        };
        let (x, y) = (uv[0] * width as f64, uv[1] * height as f64);
        if self.nearest {
            return texel(x.floor(), y.floor());
        }
        let (x, y) = (x - 0.5, y - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let [a, b, c, d] = [
            texel(x0, y0),
            texel(x0 + 1.0, y0),
            texel(x0, y0 + 1.0),
            texel(x0 + 1.0, y0 + 1.0),
        ];
        std::array::from_fn(|i| {
            (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + d[i] * fx) * fy
        })
    }
}

/// The textures `delta` sets, applying each full image and patch in order.
fn textures(delta: &TexturesDelta) -> HashMap<TextureId, Texture> {
    let mut textures = HashMap::new();
    for (id, deltas) in &delta.set {
        for delta in deltas {
            let ImageData::Color(image) = &delta.image;
            match delta.pos {
                None => {
                    textures.insert(
                        *id,
                        Texture {
                            size: image.size,
                            pixels: image.pixels.clone(),
                            nearest: delta.options.magnification == TextureFilter::Nearest,
                        },
                    );
                }
                Some([x, y]) => {
                    let Some(texture) = textures.get_mut(id) else {
                        continue;
                    };
                    let [width, height] = texture.size;
                    let columns = image.size[0].min(width.saturating_sub(x));
                    for row in 0..image.size[1].min(height.saturating_sub(y)) {
                        let to = (y + row) * width + x;
                        let from = row * image.size[0];
                        texture.pixels[to..to + columns]
                            .copy_from_slice(&image.pixels[from..from + columns]);
                    }
                }
            }
        }
    }
    textures
}

/// Draws `primitives` over transparent black at one pixel per point, sampling at pixel centres
/// and blending premultiplied colours in gamma space as egui's renderers do, and returns the
/// premultiplied pixels row by row. Meshes with a texture that `delta` does not set are skipped.
pub(super) fn rasterize(
    size: [u32; 2],
    primitives: &[ClippedPrimitive],
    delta: &TexturesDelta,
) -> Vec<Color32> {
    let [width, height] = size.map(|side| side as usize);
    let textures = textures(delta);
    let mut pixels = vec![Color32::TRANSPARENT; width * height];
    for ClippedPrimitive {
        clip_rect,
        primitive,
    } in primitives
    {
        let Primitive::Mesh(mesh) = primitive else {
            continue;
        };
        let Some(texture) = textures.get(&mesh.texture_id) else {
            continue;
        };
        // The scissor rectangle, rounded to pixels as egui's renderers round it.
        let clip_x = clip_rect.min.x.round().max(0.0) as usize..clip_rect.max.x.round() as usize;
        let clip_y = clip_rect.min.y.round().max(0.0) as usize..clip_rect.max.y.round() as usize;
        let clip = [
            clip_x.start..clip_x.end.min(width),
            clip_y.start..clip_y.end.min(height),
        ];
        for triangle in mesh.indices.as_chunks::<3>().0 {
            if let [Some(a), Some(b), Some(c)] = triangle.map(|i| mesh.vertices.get(i as usize)) {
                fill(&mut pixels, width, &clip, [a, b, c], texture);
            }
        }
    }
    pixels
}

/// Twice the signed area of `a`, `b`, `p`: positive when `p` is on the inner side of `a` to `b`
/// for a triangle with positive area.
fn edge(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

fn fill(
    pixels: &mut [Color32],
    width: usize,
    clip: &[std::ops::Range<usize>; 2],
    vertices: [&Vertex; 3],
    texture: &Texture,
) {
    let [mut a, mut b, c] = vertices;
    let pos = |vertex: &Vertex| [f64::from(vertex.pos.x), f64::from(vertex.pos.y)];
    let mut area = edge(pos(a), pos(b), pos(c));
    if !area.is_finite() || area == 0.0 {
        return;
    }
    if area < 0.0 {
        std::mem::swap(&mut a, &mut b);
        area = -area;
    }
    let vertices = [a, b, c];
    let points = vertices.map(pos);
    let edges = [(1, 2), (2, 0), (0, 1)].map(|(from, to)| (points[from], points[to]));
    // A pixel centre on an edge shared by two triangles belongs to exactly one of them, since
    // the triangles traverse the edge in opposite directions.
    let owns = edges.map(|(from, to)| {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        dy > 0.0 || (dy == 0.0 && dx > 0.0)
    });
    let span = |axis: usize, range: &std::ops::Range<usize>| {
        let min = points.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
        let max = points
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        let first = (min - 0.5).ceil().max(range.start as f64) as usize;
        let last = ((max - 0.5).floor() + 1.0).clamp(0.0, range.end as f64) as usize;
        first..last.max(first)
    };
    let colors = vertices.map(|vertex| vertex.color.to_array().map(f64::from));
    let uvs = vertices.map(|vertex| [f64::from(vertex.uv.x), f64::from(vertex.uv.y)]);
    let columns = span(0, &clip[0]);
    for y in span(1, &clip[1]) {
        for x in columns.clone() {
            let p = [x as f64 + 0.5, y as f64 + 0.5];
            let weights = edges.map(|(from, to)| edge(from, to, p));
            if (0..3).any(|i| weights[i] < 0.0 || (weights[i] == 0.0 && !owns[i])) {
                continue;
            }
            let weights = weights.map(|weight| weight / area);
            let mix = |values: &[[f64; 4]; 3], i: usize| {
                weights[0] * values[0][i] + weights[1] * values[1][i] + weights[2] * values[2][i]
            };
            let uv = std::array::from_fn(|i| {
                weights[0] * uvs[0][i] + weights[1] * uvs[1][i] + weights[2] * uvs[2][i]
            });
            let texel = texture.sample(uv);
            let source: [f64; 4] = std::array::from_fn(|i| mix(&colors, i) * texel[i] / 255.0);
            let pixel = &mut pixels[y * width + x];
            let keep = 1.0 - source[3] / 255.0;
            let blended: [u8; 4] = std::array::from_fn(|i| {
                (source[i] + f64::from(pixel.to_array()[i]) * keep)
                    .round()
                    .clamp(0.0, 255.0) as u8
            });
            *pixel =
                Color32::from_rgba_premultiplied(blended[0], blended[1], blended[2], blended[3]);
        }
    }
}
