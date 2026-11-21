//! Common drawing styles applied to renderer primitives.

use crate::drawings::domain::ColorSlot;
use crate::drawings::{ArrowStyle, Drawing, DrawingToolType, FontWeight};
use crate::model::LineStyle;
use crate::theme::ThemeExt;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, epaint::ColorMode, layers::ShapeIdx};

/// Baseline offset of the second text paint used for synthetic bold.
pub(super) const BOLD_OFFSET: f32 = 0.6;

/// Colours of the box around an annotation's text, from [`Drawing::text_box_colors`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextBoxColors {
    /// Colour of the text.
    pub(crate) text: Color32,
    /// Fill of the box; transparent when the background is off.
    pub(crate) fill: Color32,
    /// Border of the box, 1 px; `Stroke::NONE` when the border is off.
    pub(crate) border: Stroke,
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

/// The chart and panel colours a drawing reads on. A translucent chart, such as the one an image
/// exported without its background has, shows the panel behind it.
fn backgrounds(theme: &crate::theme::Theme) -> [Color32; 2] {
    [theme.ui.panel_bg.blend(theme.chart.bg), theme.ui.panel_bg]
}

/// The default colours of a tool that need another shade on a theme: slot, colour and shade.
type Shades = Vec<(ColorSlot, [u8; 4], [u8; 4])>;

/// Finds the [`Shades`] of a tool on a theme's text, chart and panel colours.
#[derive(Default)]
struct ThemeShades;

impl egui::cache::ComputerMut<(DrawingToolType, [Color32; 3]), Shades> for ThemeShades {
    fn compute(&mut self, (tool, [text, chart, panel]): (DrawingToolType, [Color32; 3])) -> Shades {
        let mut shades = Vec::new();
        let mut drawings = vec![Drawing::new(0, tool)];
        // Saved squares keep the shades of their former fan colours.
        if matches!(
            tool,
            DrawingToolType::GannSquare | DrawingToolType::GannFixed
        ) {
            let mut old = Drawing::new(0, tool);
            old.gann = None;
            drawings.push(old);
        }
        // Drawings saved before the current default keep their former colour's shade.
        if let Some(former) = Drawing::former_default_color(tool) {
            let mut old = Drawing::new(0, tool);
            old.color = former;
            old.text_color = old.text_color.map(|_| former);
            drawings.push(old);
        }
        for mut drawing in drawings {
            drawing.visit_theme_colors(|slot, &mut color| {
                let [r, g, b, a] = color;
                if a == u8::MAX {
                    let shade = crate::theme::readable_color(
                        Color32::from_rgb(r, g, b),
                        text,
                        &[chart, panel],
                        4.0,
                    )
                    .to_srgba_unmultiplied();
                    if shade != color && !shades.contains(&(slot, color, shade)) {
                        shades.push((slot, color, shade));
                    }
                }
            });
        }
        shades
    }
}

impl Drawing {
    /// Size annotations before laying out both the text and its surrounding box.
    pub(super) fn annotation_font(&self) -> egui::FontId {
        egui::FontId::proportional(crate::styles::typography::font_size(self.font_size))
    }

    /// Whether the tool's text sits on a solid label in the drawing colour rather than on a card
    /// in the theme's panel colour.
    pub(crate) fn solid_label(&self) -> bool {
        matches!(
            self.tool_type,
            DrawingToolType::PriceNote | DrawingToolType::Comment | DrawingToolType::Signpost
        )
    }

    /// The colour of a border or grid in the drawing's line colour. A tool's default colour takes
    /// the nearest shade that keeps 4:1 on the theme's chart and panel; a chosen colour is drawn
    /// as chosen.
    pub(crate) fn readable_line_color(&self, ctx: &egui::Context) -> Color32 {
        let color = self.color32();
        if !self.has_default_color() {
            return color;
        }
        let theme = ctx.active_theme();
        crate::theme::readable_color(color, theme.ui.text, &backgrounds(&theme), 4.0)
    }

    /// This drawing with each of its tool's default line, level and text colours in the shade
    /// that keeps 4:1 on the active theme's chart and panel, or `None` when it draws as stored:
    /// with [`Drawing::theme_shade`] off, or when no default colour of its tool needs a shade.
    /// Colours chosen for the drawing, fills and translucent colours are drawn as stored.
    pub(super) fn theme_shaded(&self, ctx: &egui::Context) -> Option<Drawing> {
        if !self.theme_shade {
            return None;
        }
        let theme = ctx.active_theme();
        let [chart, panel] = backgrounds(&theme);
        // Projections saved without settings used the same blue palette as a trend line.
        let tool = if self.tool_type == DrawingToolType::ProjectionTool && self.projection.is_none()
        {
            DrawingToolType::TrendLine
        } else {
            self.tool_type
        };
        let key = (tool, [theme.ui.text, chart, panel]);
        // Nothing below reads the context, which `memory_mut` holds locked.
        ctx.memory_mut(|memory| {
            let shades = memory
                .caches
                .cache::<egui::cache::FrameCache<Shades, ThemeShades>>()
                .get(key);
            if shades.is_empty() {
                return None;
            }
            let mut drawing = self.clone();
            drawing.visit_theme_colors(|slot, color| {
                if let Some(&(.., shade)) = shades
                    .iter()
                    .find(|(s, default, _)| *s == slot && default == color)
                {
                    *color = shade;
                }
            });
            Some(drawing)
        })
    }

    /// Colours of the box around an annotation's text. Unset colours follow the tool: a card
    /// takes the active theme's panel colour with its text and border in the drawing colour, and
    /// a solid label takes the drawing colour with white or black text, whichever reads on it.
    /// Text without a background takes the drawing colour. A pin's popup leaves the drawing colour
    /// to the pin: its text and border are the theme's.
    pub(crate) fn text_box_colors(&self, ctx: &egui::Context) -> TextBoxColors {
        let rgba = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
        let pin = self.tool_type == DrawingToolType::AnchoredNote;
        let (fill, text) = if self.background {
            // The theme is read only for a card without a fill colour.
            let fill = self.fill_color32().unwrap_or_else(|| {
                if self.solid_label() {
                    self.color32()
                } else {
                    ctx.active_theme().ui.panel_bg
                }
            });
            let text = if self.solid_label() {
                label_text_color(fill)
            } else {
                self.color32()
            };
            (fill, text)
        } else {
            (Color32::TRANSPARENT, self.color32())
        };
        TextBoxColors {
            text: match self.text_color {
                Some(color) => rgba(color),
                None if pin => ctx.active_theme().ui.text,
                None => text,
            },
            fill,
            border: if self.border {
                Stroke::new(
                    crate::styles::stroke::HAIRLINE,
                    match self.border_color {
                        Some(color) => rgba(color),
                        None if pin => ctx.active_theme().ui.border,
                        None => self.readable_line_color(ctx),
                    },
                )
            } else {
                Stroke::NONE
            },
        }
    }

    /// Lays annotation text out once, at the drawing's font size and slant, for both sizing its
    /// box and painting it.
    pub(super) fn annotation_galley(
        &self,
        painter: &Painter,
        text: String,
        color: Color32,
    ) -> std::sync::Arc<egui::Galley> {
        let format = egui::TextFormat {
            italics: self.italic,
            ..egui::TextFormat::simple(self.annotation_font(), color)
        };
        painter.layout_job(egui::text::LayoutJob::simple_format(text, format))
    }

    /// The drawing's text, or the localised word "Text", so an empty annotation stays selectable.
    pub(super) fn display_text(&self, ctx: &egui::Context) -> String {
        match self.text.as_deref() {
            Some(text) if !text.is_empty() => text.to_owned(),
            _ => crate::localization::text(ctx, "Text"),
        }
    }

    /// Apply line patterns and font weight without changing the renderer's widths or fills.
    pub(super) fn finish_style(&self, painter: &Painter, start: usize, rect: Rect) {
        let styled = self.fill_color.is_some()
            || self.line_style != LineStyle::Solid
            || self.font_weight == FontWeight::Bold
            || self.extend_left
            || self.extend_right;
        let clip = crate::painting::dashed_line::finite_clip(painter);
        let pixels_per_point = painter.pixels_per_point();
        painter.ctx().graphics_mut(|g| {
            let list = g.entry(painter.layer_id());
            // Segments that continue one another become one path, which the tessellator joins
            // with half the vertices and a dash pattern runs along without restarting.
            let mut previous: Option<(ShapeIdx, Pos2, Stroke, Rect)> = None;
            for index in start..list.next_idx().0 {
                let mut segment = None;
                list.mutate_shape(ShapeIdx(index), |s| {
                    if styled
                        && matches!(
                            self.tool_type,
                            DrawingToolType::TrendAngle | DrawingToolType::Arrow
                        )
                        && let Shape::LineSegment { points, .. } = &mut s.shape
                        && self.points.get(..2) == Some(points.as_slice())
                        && let Some((a, b)) = self.trend_line_ends(rect)
                    {
                        *points = [a, b];
                    }
                    if let Shape::LineSegment { points, stroke } = s.shape {
                        segment = Some((points, stroke, s.clip_rect));
                    }
                });
                if let Some((points, stroke, clip)) = segment {
                    if let Some((first, end, old_stroke, old_clip)) = previous
                        && end == points[0]
                        && old_stroke == stroke
                        && old_clip == clip
                    {
                        list.mutate_shape(first, |s| match &mut s.shape {
                            Shape::LineSegment { points: old, .. } => {
                                s.shape = Shape::line(vec![old[0], old[1], points[1]], stroke)
                            }
                            Shape::Path(p) => p.points.push(points[1]),
                            _ => {}
                        });
                        list.reset_shape(ShapeIdx(index));
                        previous = Some((first, points[1], stroke, clip));
                    } else {
                        previous = Some((ShapeIdx(index), points[1], stroke, clip));
                    }
                } else {
                    previous = None;
                }
            }
            if styled {
                for index in start..list.next_idx().0 {
                    list.mutate_shape(ShapeIdx(index), |s| {
                        self.style_shape(&mut s.shape, clip, pixels_per_point)
                    });
                }
            }
        });
        self.render_end_markers(painter, rect);
        painter.ctx().graphics_mut(|g| {
            let list = g.entry(painter.layer_id());
            for index in start..list.next_idx().0 {
                list.mutate_shape(ShapeIdx(index), |s| {
                    fewer_circle_sides(&mut s.shape, pixels_per_point, s.clip_rect)
                });
            }
        });
    }

    /// Restyles one shape. Dashes are generated only inside `clip`, so a drawing reaching far off
    /// screen costs no more than its visible part; without a finite clip nothing is dashed.
    fn style_shape(&self, shape: &mut Shape, clip: Option<Rect>, ppp: f32) {
        if let Shape::Vec(shapes) = shape {
            for s in shapes {
                self.style_shape(s, clip, ppp);
            }
            return;
        }
        if let Shape::Text(t) = shape {
            // A price note weights its price and its typed text apart, in its renderer.
            if self.font_weight == FontWeight::Bold && self.tool_type != DrawingToolType::PriceNote
            {
                // Synthetic weight keeps host font families and fallback glyphs intact. The copy
                // moves along the baseline, which text turned along a line turns with it.
                let mut weighted = t.clone();
                weighted.pos += egui::Vec2::angled(t.angle) * BOLD_OFFSET;
                *shape = Shape::Vec(vec![Shape::Text(t.clone()), Shape::Text(weighted)]);
            }
            return;
        }
        let fork = matches!(
            self.tool_type,
            DrawingToolType::Pitchfork
                | DrawingToolType::SchiffPitchfork
                | DrawingToolType::ModifiedSchiffPitchfork
                | DrawingToolType::InsidePitchfork
                | DrawingToolType::Pitchfan
        );
        let custom_fork = fork && self.pitchfork.is_some();
        let custom_projection = self.tool_type == DrawingToolType::ProjectionTool;
        // Keep body fills of annotations and legacy forks; analysis labels keep their own fill.
        let fill = |color: &mut Color32| {
            if color.a() > 0
                && !custom_fork
                && matches!(
                    self.tool_type,
                    DrawingToolType::Pitchfork
                        | DrawingToolType::SchiffPitchfork
                        | DrawingToolType::ModifiedSchiffPitchfork
                        | DrawingToolType::InsidePitchfork
                        | DrawingToolType::TextLabel
                        | DrawingToolType::AnchoredText
                        | DrawingToolType::Note
                        | DrawingToolType::Callout
                        | DrawingToolType::PriceLabel
                        | DrawingToolType::FlagNote
                        | DrawingToolType::PriceNote
                        | DrawingToolType::Table
                        | DrawingToolType::Comment
                        | DrawingToolType::Signpost
                        | DrawingToolType::Tweet
                        | DrawingToolType::Idea
                )
                && let Some(configured) = self.fill_color32()
            {
                *color = configured;
            }
        };
        // The Gann fan dashes each line in its renderer.
        let dash = if custom_projection
            || custom_fork
            || matches!(
                self.tool_type,
                DrawingToolType::RegressionTrend
                    | DrawingToolType::Sticker
                    | DrawingToolType::FibonacciRetracement
                    | DrawingToolType::FibonacciExtension
                    | DrawingToolType::FibonacciChannel
                    | DrawingToolType::FibonacciTimeZones
                    | DrawingToolType::TrendBasedFibTime
                    | DrawingToolType::FibonacciSpeedFan
                    | DrawingToolType::FibonacciCircles
                    | DrawingToolType::FibonacciSpeedResistanceArcs
                    | DrawingToolType::FibonacciWedge
                    | DrawingToolType::AnchoredVWAP
                    | DrawingToolType::GannFan
            ) {
            None
        } else {
            self.line_style.dash_pattern()
        };
        let mut path = Vec::new();
        let stroke = match shape {
            Shape::LineSegment { points, stroke } => {
                path.extend(*points);
                *stroke
            }
            Shape::Path(p) => {
                fill(&mut p.fill);
                let ColorMode::Solid(color) = &p.stroke.color else {
                    return;
                };
                let stroke = Stroke::new(p.stroke.width, *color);
                if dash.is_none() || stroke.is_empty() {
                    return;
                }
                path = p.points.clone();
                if p.closed
                    && let Some(&first) = path.first()
                {
                    path.push(first);
                }
                p.stroke.width = 0.0;
                stroke
            }
            Shape::Rect(r) => {
                fill(&mut r.fill);
                // A forecast's card frames stay solid; its line takes the pattern.
                if dash.is_none()
                    || r.stroke.is_empty()
                    || self.tool_type == DrawingToolType::Forecast
                {
                    return;
                }
                egui::epaint::tessellator::path::rounded_rectangle(
                    &mut path,
                    egui::epaint::RoundedRect::new(r.rect, r.corner_radius),
                );
                if let Some(&first) = path.first() {
                    path.push(first);
                }
                let stroke = r.stroke;
                r.stroke = Stroke::NONE;
                stroke
            }
            Shape::Circle(c) => {
                // Anchor outlines stay solid, just like the forecast's card frames.
                if dash.is_none()
                    || c.stroke.is_empty()
                    || self.tool_type == DrawingToolType::Forecast
                {
                    return;
                }
                path = ellipse_arc(
                    c.center,
                    egui::Vec2::splat(c.radius),
                    [0.0, std::f64::consts::TAU],
                    clip.unwrap_or(Rect::NOTHING).expand(c.stroke.width + 2.0),
                    ppp,
                );
                let stroke = c.stroke;
                c.stroke = Stroke::NONE;
                stroke
            }
            Shape::Ellipse(e) => {
                if dash.is_none() || e.stroke.is_empty() {
                    return;
                }
                path = ellipse_arc(
                    e.center,
                    e.radius,
                    [0.0, std::f64::consts::TAU],
                    clip.unwrap_or(Rect::NOTHING).expand(e.stroke.width + 2.0),
                    ppp,
                );
                let stroke = e.stroke;
                e.stroke = Stroke::NONE;
                stroke
            }
            _ => return,
        };
        if let Some((dash, gap)) = dash
            && !stroke.is_empty()
        {
            let mut shapes = clip.map_or_else(Vec::new, |clip| {
                crate::painting::dashed_line::dashed_polyline_shapes(&path, stroke, dash, gap, clip)
            });
            if !matches!(shape, Shape::LineSegment { .. }) {
                shapes.insert(0, std::mem::replace(shape, Shape::Noop));
            }
            *shape = Shape::Vec(shapes);
        }
    }

    fn render_end_markers(&self, painter: &Painter, rect: Rect) {
        use DrawingToolType as T;
        if !self.arrow_start_style.is_visible() && !self.arrow_end_style.is_visible() {
            return;
        }
        let Some(&a) = self.points.first() else {
            return;
        };
        let b = self.points.get(1).copied().unwrap_or(a);
        if matches!(self.tool_type, T::FlatTopBottom | T::DisjointChannel) {
            // Both lines run from the first anchor's side to the second's, as extended.
            let Some(outline) = self.wedge_channel_outline(rect) else {
                return;
            };
            for [left, right] in outline.lines {
                let [start, end] = if b.x < a.x {
                    [right, left]
                } else {
                    [left, right]
                };
                self.render_marker(painter, start, start - end, self.arrow_start_style);
                self.render_marker(painter, end, end - start, self.arrow_end_style);
            }
            return;
        }
        let ends = match self.tool_type {
            T::TrendLine | T::InfoLine | T::TrendAngle | T::Ray | T::ExtendedLine | T::Arrow => {
                self.trend_line_ends(rect).map(|(a, b)| [a, b])
            }
            T::HorizontalLine => Some([Pos2::new(rect.min.x, a.y), Pos2::new(rect.max.x, a.y)]),
            T::HorizontalRay => Some([a, Pos2::new(rect.max.x, a.y)]),
            T::VerticalLine => Some([Pos2::new(a.x, rect.min.y), Pos2::new(a.x, rect.max.y)]),
            T::Polyline | T::Path => self.points.last().map(|&b| [a, b]),
            _ => None,
        };
        if let Some(mut ends) = ends {
            if matches!(
                self.tool_type,
                T::TrendLine | T::InfoLine | T::TrendAngle | T::ExtendedLine | T::Ray | T::Arrow
            ) && b.x < a.x
            {
                ends.swap(0, 1);
            }
            let [start, end] = ends;
            let (start_direction, end_direction) =
                if matches!(self.tool_type, T::Polyline | T::Path) && self.points.len() > 2 {
                    (a - b, end - self.points[self.points.len() - 2])
                } else {
                    (start - end, end - start)
                };
            self.render_marker(painter, start, start_direction, self.arrow_start_style);
            self.render_marker(painter, end, end_direction, self.arrow_end_style);
        }
    }

    /// The size `render_marker` paints an end marker in `style` at.
    fn end_marker_size(&self, style: ArrowStyle) -> f32 {
        (self.stroke_width * style.size_multiplier()).max(4.0)
    }

    /// The radius of the smallest circle about its tip that holds an end marker in `style`:
    /// an arrowhead's back corners, a square's corners, half the size of the other markers.
    pub(super) fn end_marker_reach(&self, style: ArrowStyle) -> f32 {
        let size = self.end_marker_size(style);
        match style {
            ArrowStyle::None => 0.0,
            ArrowStyle::Arrow => size * 5_f32.sqrt() / 2.0,
            ArrowStyle::OpenArrow => size * 5_f32.sqrt() / 2.0 + self.stroke_width / 2.0,
            ArrowStyle::Circle | ArrowStyle::Diamond => size / 2.0,
            ArrowStyle::Square => size * std::f32::consts::FRAC_1_SQRT_2,
        }
    }

    fn render_marker(
        &self,
        painter: &Painter,
        tip: Pos2,
        direction: egui::Vec2,
        style: ArrowStyle,
    ) {
        if direction.length_sq() < 1e-6 {
            return;
        }
        let color = self.color32();
        let size = self.end_marker_size(style);
        let along = direction.normalized() * size;
        let across = egui::vec2(-along.y, along.x) / 2.0;
        let base = tip - along;
        match style {
            ArrowStyle::None => {}
            ArrowStyle::Arrow => {
                painter.add(Shape::convex_polygon(
                    vec![tip, base + across, base - across],
                    color,
                    Stroke::NONE,
                ));
            }
            ArrowStyle::OpenArrow => {
                painter.add(Shape::line(
                    vec![base + across, tip, base - across],
                    self.stroke(),
                ));
            }
            ArrowStyle::Circle => {
                painter.circle_filled(tip, size / 2.0, color);
            }
            ArrowStyle::Square => {
                painter.rect_filled(
                    Rect::from_center_size(tip, egui::Vec2::splat(size)),
                    0.0,
                    color,
                );
            }
            ArrowStyle::Diamond => {
                painter.add(Shape::convex_polygon(
                    vec![
                        tip + along / 2.0,
                        tip + across,
                        tip - along / 2.0,
                        tip - across,
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }
}

/// Replaces a circle with a path within a quarter of a physical pixel of it. The tessellator
/// gives a circle 32 sides from a radius of 5 points and 64 from 18, which made the anchor dots
/// and rings of a drawing most of its vertices. A small disc without an outline stays a circle,
/// which the tessellator draws as one textured quad.
fn fewer_circle_sides(shape: &mut Shape, pixels_per_point: f32, clip: Rect) {
    match shape {
        Shape::Vec(shapes) => {
            for shape in shapes {
                fewer_circle_sides(shape, pixels_per_point, clip);
            }
        }
        Shape::Circle(c) if !c.stroke.is_empty() || c.radius * pixels_per_point > 6.0 => {
            // Outside joins extend by width / cos(half_step), so wide rings need more sides.
            let radius = (f64::from(c.radius) * f64::from(pixels_per_point))
                .max(f64::from(c.stroke.width) * f64::from(pixels_per_point) + 0.25);
            let step = (1.0 - 0.25 / radius).clamp(-1.0, 1.0).acos();
            let sides = (std::f64::consts::PI / step).ceil().max(8.0) as usize;
            // Small marker rings keep their existing quarter-pixel polygons.
            let points = if sides <= 128 {
                (0..sides)
                    .map(|i| {
                        let angle = i as f32 * std::f32::consts::TAU / sides as f32;
                        c.center + c.radius * egui::vec2(angle.cos(), angle.sin())
                    })
                    .collect()
            } else {
                let bound = radius / f64::from(pixels_per_point);
                let mut points = smooth_path(
                    [0.0, std::f64::consts::TAU],
                    Acceleration {
                        x: bound,
                        y: bound,
                        length: bound,
                    },
                    clip.expand(c.stroke.width + 2.0),
                    pixels_per_point,
                    |angle| ellipse_point(c.center, egui::Vec2::splat(c.radius), angle),
                );
                points.pop();
                points
            };
            let mut path = egui::epaint::PathShape::convex_polygon(points, c.fill, c.stroke);
            path.stroke = path.stroke.outside();
            *shape = Shape::Path(path);
        }
        _ => {}
    }
}

/// Bounds on the second derivative in screen coordinates.
#[derive(Clone, Copy, Default)]
pub(super) struct Acceleration {
    /// Maximum absolute horizontal component.
    pub(super) x: f64,
    /// Maximum absolute vertical component.
    pub(super) y: f64,
    /// Maximum vector length, which may be tighter than the component bounds' hypotenuse.
    pub(super) length: f64,
}

impl Acceleration {
    /// Bounds for an ellipse parameterised by its angle in radians.
    pub(super) fn ellipse(radii: egui::Vec2) -> Self {
        Self {
            x: f64::from(radii.x.abs()),
            y: f64::from(radii.y.abs()),
            length: f64::from(radii.abs().max_elem()),
        }
    }
}

/// Samples whose chords stay within a quarter of a physical pixel of each curve in
/// `at`. Each `acceleration` bounds that curve's second derivative over `range`.
/// Off-screen spans stay coarse. A long enough chord certifies monotone projection
/// along it, so only the normal error matters; turns retain the full vector bound.
pub(super) fn sample_curve<const N: usize>(
    range: [f64; 2],
    acceleration: [Acceleration; N],
    clip: Rect,
    ppp: f32,
    at: impl Fn(f64) -> [Pos2; N],
) -> Vec<(f64, [Pos2; N])> {
    let [from, to] = range;
    let (a, b) = (at(from), at(to));
    if acceleration.iter().any(|m| !m.length.is_finite())
        || a.iter().chain(&b).any(|p| !p.is_finite())
    {
        return Vec::new();
    }
    // Leave room for the final conversion back to single-precision screen coordinates.
    let tolerance = 0.24 / f64::from(ppp.max(0.01));
    let max_acceleration = acceleration.iter().map(|m| m.length).fold(0.0, f64::max);
    let mut result = Vec::with_capacity(32);
    result.push((from, a));
    let mut pending = Vec::new();
    let mut current = (from, to, a, b);
    loop {
        let (from, to, a, b) = current;
        let factor = (to - from).powi(2) / 8.0;
        let mut refine = false;
        for i in 0..N {
            let m = acceleration[i];
            let mut error = m.length * factor;
            if error <= tolerance {
                continue;
            }
            if m.x < m.length || m.y < m.length {
                let dx = f64::from(b[i].x) - f64::from(a[i].x);
                let dy = f64::from(b[i].y) - f64::from(a[i].y);
                let chord = (dx * dx + dy * dy).sqrt();
                // The projected derivative differs from chord / dt by at most M * dt / 2.
                if chord > 0.0 && chord >= 4.0 * error {
                    error = error.min((m.x * dy.abs() + m.y * dx.abs()) * factor / chord);
                }
            }
            if error > tolerance
                && Rect::from_two_pos(a[i], b[i])
                    .expand2(egui::vec2((m.x * factor) as f32, (m.y * factor) as f32))
                    .intersects(clip)
            {
                refine = true;
                break;
            }
        }
        let mid = (from + to) / 2.0;
        if !refine || mid == from || mid == to {
            result.push((to, b));
        } else if max_acceleration * factor <= 1024.0 * tolerance {
            // Finish short visible spans directly, without rounding their density up to
            // a power of two or allocating more subdivision entries.
            let segments = (max_acceleration * factor / tolerance).sqrt().ceil() as usize;
            for i in 1..segments {
                let t = from + (to - from) * i as f64 / segments as f64;
                result.push((t, at(t)));
            }
            result.push((to, b));
        } else {
            let m = at(mid);
            pending.push((mid, to, m, b));
            current = (from, mid, a, m);
            continue;
        }
        let Some(next) = pending.pop() else { break };
        current = next;
    }
    result
}

/// A smooth sampled curve with the same tolerance and off-screen bounds as curved bands.
pub(super) fn smooth_path(
    range: [f64; 2],
    acceleration: Acceleration,
    clip: Rect,
    ppp: f32,
    at: impl Fn(f64) -> Pos2,
) -> Vec<Pos2> {
    sample_curve(range, [acceleration], clip, ppp, |t| [at(t)])
        .into_iter()
        .map(|(_, [point])| point)
        .collect()
}

/// A circular or elliptical arc, including both ends. Evaluate in double precision so a
/// large off-screen centre does not consume the precision of the visible coordinates.
pub(super) fn ellipse_arc(
    center: Pos2,
    radii: egui::Vec2,
    angles: [f64; 2],
    clip: Rect,
    ppp: f32,
) -> Vec<Pos2> {
    let mut points = smooth_path(angles, Acceleration::ellipse(radii), clip, ppp, |angle| {
        ellipse_point(center, radii, angle)
    });
    if angles[1] - angles[0] == std::f64::consts::TAU && !points.is_empty() {
        let first = points[0];
        *points.last_mut().unwrap() = first;
    }
    points
}

/// A point on the ellipse, retaining double precision until the final screen coordinates.
pub(super) fn ellipse_point(center: Pos2, radii: egui::Vec2, angle: f64) -> Pos2 {
    let (sin, cos) = angle.sin_cos();
    Pos2::new(
        (f64::from(center.x) + f64::from(radii.x) * cos) as f32,
        (f64::from(center.y) + f64::from(radii.y) * sin) as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::painting::point_to_segment_distance;
    use egui::epaint::StrokeKind;

    fn paint(drawing: &Drawing) -> Vec<Shape> {
        paint_on(&egui::Context::default(), drawing, false)
    }

    fn paint_on(ctx: &egui::Context, drawing: &Drawing, selected: bool) -> Vec<Shape> {
        let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(pane),
                ..Default::default()
            },
            |ui| {
                drawing.render_with_selection(
                    ui.painter(),
                    pane,
                    &crate::drawings::rendering::ChartView {
                        price_formatter: None,
                        thousands_sep: None,
                        time_formatter: None,
                        bar_seconds: 0,
                        instrument: None,
                        bars: &[],
                        revision: 0,
                        bar_to_x: &|x| x,
                        bar_spacing: 1.0,
                        price_to_y: &|p| p as f32,
                        y_to_price: &f64::from,
                        time_axis: true,
                        executions: None,
                        timeframe: None,
                        finer: &[],
                    },
                    selected,
                );
            },
        );
        fn collect(shape: &Shape, out: &mut Vec<Shape>) {
            match shape {
                Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                _ => out.push(shape.clone()),
            }
        }
        let mut shapes = Vec::new();
        for s in &output.shapes {
            collect(&s.shape, &mut shapes);
        }
        output.drop_without_applying_deltas();
        shapes
    }

    fn pattern(tool: DrawingToolType) -> Drawing {
        let mut drawing = Drawing::new(1, tool);
        drawing.points = [
            (100., 300.),
            (180., 100.),
            (260., 250.),
            (340., 150.),
            (420., 220.),
            (500., 180.),
            (580., 300.),
        ]
        .map(|(x, y)| Pos2::new(x, y))
        .to_vec();
        drawing.points.truncate(tool.required_points().unwrap_or(7));
        drawing.chart_points = drawing
            .points
            .iter()
            .map(|p| crate::drawings::ChartPoint::new(p.x, f64::from(p.y)))
            .collect();
        drawing
    }

    /// Empty annotations use one translated default; plain Text and Anchored text paint alike.
    #[test]
    fn text_annotations_paint_plainly_with_a_localised_default() {
        use DrawingToolType::*;
        let ctx = egui::Context::default();
        crate::localization::set_translator(&ctx, |s: &str| {
            (s == "Text").then(|| "Texte".to_owned())
        });
        for tool in [
            TextLabel,
            AnchoredText,
            Note,
            Callout,
            Comment,
            Signpost,
            AnchoredNote,
            FlagNote,
        ] {
            for text in [None, Some("")] {
                let mut drawing = pattern(tool);
                drawing.text = text.map(str::to_owned);
                let shapes = paint_on(&ctx, &drawing, tool == AnchoredNote);
                let texts: Vec<_> = shapes
                    .iter()
                    .filter_map(|s| match s {
                        Shape::Text(t) => Some(t.galley.text()),
                        _ => None,
                    })
                    .collect();
                let expected = if matches!(tool, AnchoredNote | FlagNote) {
                    vec![]
                } else {
                    vec!["Texte"]
                };
                assert_eq!(texts, expected, "{tool:?} {text:?}");
            }
        }
        for boxed in [false, true] {
            let mut text = pattern(TextLabel);
            text.text = Some("Hi".into());
            (text.background, text.border) = (boxed, boxed);
            let mut anchored = text.clone();
            anchored.tool_type = AnchoredText;
            assert_eq!(
                format!("{:?}", paint(&text)),
                format!("{:?}", paint(&anchored))
            );
        }
        let mut text = pattern(TextLabel);
        text.text = Some("Hi".into());
        let shapes = paint(&text);
        assert_eq!(shapes.len(), 1, "a plain label paints only its text");
        assert!(matches!(&shapes[0], Shape::Text(t) if t.pos == Pos2::new(100.0, 300.0)));
    }

    /// Text boxes take the theme's card, or a solid label in the drawing colour, unless their
    /// text, background and border settings say otherwise; a pin's popup leaves the pin's colour
    /// to the pin. Contrast of white on the red is 1.05 / 0.269 = 3.90 and on the yellow
    /// 1.05 / 0.860 = 1.22, so black reads on the yellow. A pin's popup follows its switch, the
    /// selection and the pointer on a neutral leader, and a signpost's disc follows its glyph
    /// switch and colour, taking the plate's place on the pole.
    #[test]
    fn text_boxes_follow_the_theme_and_their_settings() {
        use DrawingToolType::*;
        let red = Color32::from_rgb(242, 54, 69);
        let yellow = Color32::from_rgb(255, 235, 59);
        let blue = Color32::from_rgb(0, 0, 255);
        let green = Color32::from_rgb(0, 128, 0);
        for (theme, panel) in [
            (crate::theme::Theme::light(), Color32::WHITE),
            (crate::theme::Theme::dark(), Color32::BLACK),
        ] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            for tool in [
                TextLabel,
                AnchoredText,
                Note,
                AnchoredNote,
                FlagNote,
                Callout,
                Comment,
                PriceNote,
                Signpost,
            ] {
                let solid = matches!(tool, Comment | PriceNote | Signpost);
                let plain = matches!(tool, TextLabel | AnchoredText);
                let border =
                    |color| vec![Stroke::new(if tool == Callout { 2.0 } else { 1.0 }, color)];
                let pin = tool == AnchoredNote;
                let text = if pin { theme.ui.text } else { red };
                let outline = if pin { theme.ui.border } else { red };
                // Settings, text colour, box fills, box strokes and italics.
                type Case = (
                    &'static str,
                    fn(&mut Drawing),
                    Color32,
                    Vec<Color32>,
                    Vec<Stroke>,
                    bool,
                );
                let cases: [Case; 4] = [
                    (
                        "defaults",
                        |_| {},
                        if solid { Color32::WHITE } else { text },
                        if plain {
                            vec![]
                        } else {
                            vec![if solid { red } else { panel }]
                        },
                        if plain { vec![] } else { border(outline) },
                        false,
                    ),
                    (
                        "all set",
                        |d| {
                            d.text_color = Some([0, 0, 255, 255]);
                            d.fill_color = Some([255, 235, 59, 255]);
                            d.border_color = Some([0, 128, 0, 255]);
                            d.italic = true;
                            d.background = true;
                            d.border = true;
                        },
                        blue,
                        vec![yellow],
                        border(green),
                        true,
                    ),
                    (
                        "fill only",
                        |d| d.fill_color = Some([255, 235, 59, 255]),
                        if solid { Color32::BLACK } else { text },
                        if plain { vec![] } else { vec![yellow] },
                        if plain { vec![] } else { border(outline) },
                        false,
                    ),
                    (
                        "switched off",
                        |d| {
                            d.background = false;
                            d.border = false;
                        },
                        text,
                        vec![],
                        vec![],
                        false,
                    ),
                ];
                for (case, setup, text_color, fills, strokes, italics) in cases {
                    let mut drawing = pattern(tool);
                    // A red drawing with no text box settings of its own, whatever the tool's.
                    drawing.color = red.to_array();
                    (drawing.fill_color, drawing.text_color) = (None, None);
                    (drawing.background, drawing.border) = (!plain, !plain);
                    drawing.border_color = None;
                    drawing.points = vec![Pos2::new(100.0, 300.0), Pos2::new(200.0, 100.0)];
                    drawing.chart_points = (drawing.points.iter())
                        .map(|p| crate::drawings::ChartPoint::new(p.x, f64::from(p.y)))
                        .collect();
                    drawing.text = Some("Hi".into());
                    setup(&mut drawing);
                    let shapes = paint_on(&ctx, &drawing, true);
                    let texts: Vec<_> = (shapes.iter())
                        .filter_map(|s| match s {
                            Shape::Text(t) if t.galley.text().ends_with("Hi") => Some(t),
                            _ => None,
                        })
                        .collect();
                    let [text] = texts[..] else {
                        panic!("{tool:?} {case}: {} texts", texts.len());
                    };
                    let format = &text.galley.job.sections[0].format;
                    let label = text.visual_bounding_rect();
                    let (mut painted_fills, mut painted_strokes) = (vec![], vec![]);
                    for shape in &shapes {
                        let (bounds, fill, stroke) = match shape {
                            Shape::Rect(r) => (r.rect, r.fill, r.stroke),
                            Shape::Path(p) if p.closed => {
                                let ColorMode::Solid(color) = p.stroke.color else {
                                    continue;
                                };
                                (
                                    egui::Rect::from_points(&p.points),
                                    p.fill,
                                    Stroke::new(p.stroke.width, color),
                                )
                            }
                            _ => continue,
                        };
                        if bounds.expand(1.0).contains_rect(label) {
                            painted_fills.extend((fill.a() > 0).then_some(fill));
                            painted_strokes.extend((!stroke.is_empty()).then_some(stroke));
                        }
                    }
                    let name = format!("{tool:?} {case}, dark {}", theme.dark);
                    assert_eq!(format.color, text_color, "{name}");
                    assert_eq!(format.italics, italics, "{name}");
                    assert_eq!(painted_fills, fills, "{name}");
                    assert_eq!(painted_strokes, strokes, "{name}");
                }
            }

            let said = |shapes: &[Shape], text: &str| {
                shapes
                    .iter()
                    .any(|s| matches!(s, Shape::Text(t) if t.galley.text() == text))
            };
            let mut signpost = pattern(Signpost);
            signpost.color = red.to_array();
            signpost.text = Some("Hi".into());
            type Glyph = (fn(&mut Drawing), Option<Color32>);
            let glyphs: [Glyph; 3] = [
                (|_| {}, None),
                (|d| d.marker.show_glyph = true, Some(red)),
                (
                    |d| {
                        d.marker.show_glyph = true;
                        d.marker.glyph_color = Some([0, 0, 255, 255]);
                    },
                    Some(blue),
                ),
            ];
            for (setup, disc) in glyphs {
                let mut drawing = signpost.clone();
                setup(&mut drawing);
                let shapes = paint_on(&ctx, &drawing, false);
                // Circles reach these shapes as polygons.
                let discs: Vec<_> = (shapes.iter())
                    .filter_map(|s| match s {
                        Shape::Path(p) if p.closed => {
                            Some((egui::Rect::from_points(&p.points).center().round(), p.fill))
                        }
                        _ => None,
                    })
                    .collect();
                let name = format!("disc {disc:?}, dark {}", theme.dark);
                let anchor = Pos2::new(100.0, 100.0);
                // The bundled fonts lack the default face, so it is drawn in white on the disc:
                // its outline, its eyes and, below, its smile as a line.
                let face = |fill| {
                    let eye = |x| (Pos2::new(x, 98.0), Color32::WHITE);
                    [
                        (anchor, fill),
                        (anchor, Color32::TRANSPARENT),
                        eye(98.0),
                        eye(102.0),
                    ]
                };
                assert_eq!(
                    discs,
                    Vec::from_iter(disc.into_iter().flat_map(face)),
                    "{name}"
                );
                let smiles = (shapes.iter())
                    .filter(|s| matches!(s, Shape::Path(p) if !p.closed))
                    .count();
                assert_eq!(smiles, usize::from(disc.is_some()), "{name}");
                assert!(!said(&shapes, "🙂"), "{name}");
                let plates: Vec<_> = (shapes.iter())
                    .filter_map(|s| match s {
                        Shape::Rect(r) => Some(r.rect),
                        _ => None,
                    })
                    .collect();
                let [plate] = plates[..] else {
                    panic!("{name}: {} plates", plates.len());
                };
                // The plate is centred on the label's anchor, or ends 4 points above a disc of
                // 14 points radius, and the pole from the bar at (100, 300) stops at either.
                let (bottom, pole) = if disc.is_some() {
                    (82.0, 114.0)
                } else {
                    (100.0 + plate.height() / 2.0, plate.max.y)
                };
                assert!((plate.max.y - bottom).abs() < 0.01, "{name}: {plate:?}");
                let poles: Vec<_> = (shapes.iter())
                    .filter_map(|s| match s {
                        Shape::LineSegment { points, .. } => Some(*points),
                        _ => None,
                    })
                    .collect();
                let pole = [Pos2::new(100.0, 300.0), Pos2::new(100.0, pole)];
                assert_eq!(poles, [pole], "{name}");
            }

            let mut pin = pattern(AnchoredNote);
            pin.text = Some("Hi".into());
            pin.marker.show_text = false;
            assert!(!said(&paint_on(&ctx, &pin, true), "Hi"), "switched off");
            pin.marker.show_text = true;
            assert!(!said(&paint_on(&ctx, &pin, false), "Hi"), "pointer away");
            // The pin's tip is on its anchor, with its head 16 points above.
            let output = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::PointerMoved(Pos2::new(100.0, 284.0))],
                    ..Default::default()
                },
                |_| {},
            );
            output.drop_without_applying_deltas();
            let shapes = paint_on(&ctx, &pin, false);
            assert!(said(&shapes, "Hi"), "pointer on the pin");
            let leaders: Vec<_> = (shapes.iter())
                .filter_map(|s| match s {
                    Shape::LineSegment { stroke, .. } => Some(*stroke),
                    _ => None,
                })
                .collect();
            let neutral = Stroke::new(1.0, theme.chart.axis_text_secondary);
            assert_eq!(leaders, [neutral], "dark {}", theme.dark);
            pin.marker.show_text = false;
            assert!(!said(&paint_on(&ctx, &pin, false), "Hi"), "switched off");
        }
    }

    /// A tool's default border colour keeps 4:1 on the theme and a chosen one is drawn as chosen.
    /// Teal has 3.51:1 on white and (0, 140, 155), its first shade towards black to reach 4:1, has
    /// 4.02:1; on black it has 5.99:1, and the blue has 4.90:1 on white and 4.29:1 on black.
    #[test]
    fn default_box_borders_keep_their_contrast_on_the_theme() {
        let teal = Color32::from_rgb(0, 151, 167);
        let blue = Color32::from_rgb(41, 98, 255);
        for (theme, callout) in [
            (crate::theme::Theme::light(), Color32::from_rgb(0, 140, 155)),
            (crate::theme::Theme::dark(), teal),
            (crate::theme::Theme::high_contrast(), teal),
        ] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let border = |drawing: &Drawing| drawing.text_box_colors(&ctx).border.color;
            let mut drawing = Drawing::new(1, DrawingToolType::Callout);
            assert_eq!(border(&drawing), callout, "dark {}", theme.dark);
            drawing.color = [200, 200, 200, 255];
            assert_eq!(border(&drawing), Color32::from_gray(200));
            assert_eq!(border(&Drawing::new(1, DrawingToolType::Note)), blue);
        }
    }

    /// A circle, sine line, Elliott wave or regression trend saved in its tool's former default
    /// colour keeps the shade it was drawn in on the light theme: that of the tool that still
    /// starts in the colour.
    #[test]
    fn former_default_colours_keep_their_theme_shade() {
        use DrawingToolType as T;
        let ctx = egui::Context::default();
        crate::theme::apply_to_egui(&ctx, &crate::theme::Theme::light());
        for (tool, former, peer) in [
            (T::Circle, [242, 54, 69, 255], T::Ellipse),
            (T::RegressionTrend, [242, 54, 69, 255], T::Ellipse),
            (T::SineLine, [8, 153, 129, 255], T::Triangle),
            (T::ElliottTriangle, [61, 133, 198, 255], T::ElliottImpulse),
            (
                T::ElliottDoubleCombo,
                [61, 133, 198, 255],
                T::ElliottImpulse,
            ),
            (
                T::ElliottTripleCombo,
                [61, 133, 198, 255],
                T::ElliottImpulse,
            ),
        ] {
            let mut saved = Drawing::new(1, tool);
            saved.color = former;
            saved.text_color = saved.text_color.map(|_| former);
            let shade = Drawing::new(1, peer).theme_shaded(&ctx).unwrap().color;
            assert_ne!(shade, former, "{peer:?} is drawn in another shade on white");
            let shaded = saved.theme_shaded(&ctx).expect("a shaded drawing");
            assert_eq!(shaded.color, shade, "{tool:?}");
            if tool == T::Circle {
                assert_eq!(shaded.text_color, Some(shade), "{tool:?}");
            }
        }
    }

    /// A tool's default line and level colours keep 4:1 on the theme, while a drawing saved
    /// before the shade existed, a chosen colour, fills and the text on a note's fill are drawn
    /// as stored. By hand with the WCAG formula: orange (255, 152, 0) has 2.16:1 on white, and
    /// its 72nd step of 255 towards black, (183, 109, 0), has linear R 0.4734 and G 0.1530, so
    /// L = 0.2126 * 0.4734 + 0.7152 * 0.1530 = 0.2101 and 1.05 / 0.2601 = 4.04:1; step 71,
    /// (184, 110, 0), has 3.99:1. Purple (156, 39, 176) has 3.33:1 on black; towards the dark
    /// theme's text (219, 219, 219), step 35, (165, 64, 182), has L = 0.1504 and
    /// 0.2004 / 0.05 = 4.01:1 (step 34, (164, 63, 182), has 3.97:1), and towards white step 27,
    /// (166, 62, 184), has 4.00:1 (step 26, (166, 61, 184), has 3.98:1). Blue (41, 98, 255) has
    /// 4.90:1 on white and keeps its colour. A transparent chart, as an image exported without its
    /// background has, reads as the panel: purple keeps its 6.30:1 on white rather than going
    /// black, as no step towards black reaches 4:1 on both white and black.
    #[test]
    fn default_colours_keep_their_contrast_on_the_theme() {
        use crate::theme::Theme;
        use DrawingToolType::*;
        let rgb = Color32::from_rgb;
        let (orange, purple) = (rgb(255, 152, 0), rgb(156, 39, 176));
        let mut transparent = Theme::light();
        transparent.chart.bg = Color32::TRANSPARENT;
        // A flat top/bottom as saved before the shade existed, in its default style.
        let saved: Drawing = serde_json::from_str(
            r#"{
                "tool_type": "FlatTopBottom",
                "color": [255, 152, 0, 255],
                "stroke_width": 2.0,
                "line_style": "Solid",
                "fill_color": [255, 152, 0, 51],
                "locked": false,
                "visible": true,
                "font_size": 14.0,
                "font_weight": "Normal",
                "extend_left": false,
                "extend_right": false,
                "arrow_start_style": "None",
                "arrow_end_style": "None",
                "timeframe_visibility": {"show_on_all": true, "visible_timeframes": []},
                "z_order": 0
            }"#,
        )
        .unwrap();
        // Anchors 50 px apart in price keep every visible Fibonacci level in the pane.
        let anchored = |mut drawing: Drawing| {
            drawing.chart_points = [(100.0, 300.0), (180.0, 250.0), (260.0, 280.0)]
                .map(|(bar, price)| crate::drawings::ChartPoint::new(bar, price))
                .to_vec();
            drawing
                .chart_points
                .truncate(drawing.tool_type.required_points().unwrap_or(3));
            drawing.update_screen_coords(|x| x, |p| p as f32);
            drawing
        };
        let mut chosen = Drawing::new(1, FlatTopBottom);
        chosen.set_line_color([255, 200, 0, 255]);
        let flat = Some(Color32::from_rgba_unmultiplied(255, 152, 0, 51));
        let mut projection = Drawing::new(1, ProjectionTool);
        projection.theme_shade = true;
        let cases = [
            (
                Theme::light(),
                projection,
                vec![rgb(127, 127, 127)],
                Some(rgb(128, 128, 128)),
                Some(Color32::from_rgba_unmultiplied(41, 98, 255, 51)),
            ),
            (
                Theme::light(),
                Drawing::new(1, FlatTopBottom),
                vec![rgb(183, 109, 0)],
                Some(orange),
                flat,
            ),
            (
                Theme::light(),
                saved,
                vec![orange],
                Some(rgb(183, 109, 0)),
                flat,
            ),
            (Theme::light(), chosen, vec![rgb(255, 200, 0)], None, None),
            (
                Theme::dark(),
                Drawing::new(1, Rect),
                vec![rgb(165, 64, 182)],
                Some(purple),
                Some(Color32::from_rgba_unmultiplied(156, 39, 176, 51)),
            ),
            (
                Theme::high_contrast(),
                Drawing::new(1, Rect),
                vec![rgb(166, 62, 184)],
                None,
                None,
            ),
            (
                transparent,
                Drawing::new(1, Rect),
                vec![purple],
                Some(Color32::BLACK),
                None,
            ),
            (
                Theme::light(),
                Drawing::new(1, FibonacciRetracement),
                vec![rgb(183, 109, 0), rgb(41, 98, 255)],
                Some(orange),
                None,
            ),
        ];
        for (theme, drawing, present, absent, fill) in cases {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let name = format!("{:?}, dark {}", drawing.tool_type, theme.dark);
            let (mut strokes, mut fills) = (vec![], vec![]);
            for shape in paint_on(&ctx, &anchored(drawing), false) {
                let (stroke, fill) = match shape {
                    Shape::LineSegment { stroke, .. } => (stroke.color, Color32::TRANSPARENT),
                    Shape::Rect(r) => (r.stroke.color, r.fill),
                    Shape::Path(p) => match p.stroke.color {
                        ColorMode::Solid(color) => (color, p.fill),
                        _ => (Color32::TRANSPARENT, p.fill),
                    },
                    Shape::Mesh(mesh) => {
                        fills.extend(mesh.vertices.iter().map(|v| v.color));
                        continue;
                    }
                    _ => continue,
                };
                strokes.push(stroke);
                fills.push(fill);
            }
            for color in present {
                assert!(
                    strokes.contains(&color),
                    "{name}: no {color:?} in {strokes:?}"
                );
            }
            if let Some(color) = absent {
                assert!(!strokes.contains(&color), "{name}: {color:?} painted");
            }
            if let Some(color) = fill {
                assert!(
                    fills.contains(&color),
                    "{name}: no fill {color:?} in {fills:?}"
                );
            }
        }
        let ctx = egui::Context::default();
        crate::theme::apply_to_egui(&ctx, &Theme::light());
        // Prices set in the default orange take the line's shade, as the line does.
        let mut priced = Drawing::new(1, FlatTopBottom);
        priced.line_measurements.price_labels = true;
        priced.anchor_price_style = Some(crate::drawings::NoteTextStyle::new(
            [255, 152, 0, 255],
            12.0,
        ));
        let colours: Vec<_> = paint_on(&ctx, &anchored(priced), false)
            .into_iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) => Some(text.galley.job.sections[0].format.color),
                _ => None,
            })
            .collect();
        assert_eq!(colours, vec![rgb(183, 109, 0); 4]);
        let mut note = anchored(Drawing::new(1, Note));
        note.text = Some("Hi".into());
        let text = paint_on(&ctx, &note, false)
            .into_iter()
            .find_map(|s| match s {
                Shape::Text(t) if t.galley.text() == "Hi" => {
                    Some(t.galley.job.sections[0].format.color)
                }
                _ => None,
            });
        assert_eq!(text, Some(Color32::WHITE));
    }

    /// Finished drawings leave swing points clear; small circles are reserved for previews,
    /// selection handles and the flag's pole cap. Wave label backgrounds remain larger circles.
    #[test]
    fn finished_drawings_leave_anchors_clear() {
        use DrawingToolType::*;
        let marker = |shape: &Shape| match shape {
            Shape::Circle(c) if c.radius <= 8.0 => Some((c.center, c.radius)),
            Shape::Path(p) if p.closed && p.points.len() >= 8 => {
                let bounds = egui::Rect::from_points(&p.points);
                (bounds.width() <= 16.01 && bounds.height() <= 16.01)
                    .then_some((bounds.center(), bounds.width() / 2.0))
            }
            _ => None,
        };
        for tool in [
            Ray,
            ExtendedLine,
            HorizontalRay,
            CrossLine,
            TrendAngle,
            PriceRange,
            DateRange,
            InfoLine,
            RegressionTrend,
            FlatTopBottom,
            DisjointChannel,
            Arc,
            Polyline,
            Path,
            Curve,
            Pitchfork,
            SchiffPitchfork,
            ModifiedSchiffPitchfork,
            InsidePitchfork,
            Pitchfan,
            GannFan,
            GannSquare,
            GannBox,
            GannFixed,
            FibonacciTimeZones,
            FibonacciChannel,
            FibonacciCircles,
            FibonacciSpeedFan,
            XABCDPattern,
            CypherPattern,
            HeadAndShoulders,
            ABCDPattern,
            TrianglePattern,
            ThreeDrivesPattern,
            ElliottImpulse,
            ElliottCorrection,
            ElliottTriangle,
            ElliottDoubleCombo,
            ElliottTripleCombo,
            CyclicLines,
            TimeCycles,
            SineLine,
            AnchoredVWAP,
            FixedRangeVolumeProfile,
            AnchoredVolumeProfile,
            BarsPattern,
            GhostFeed,
            ProjectionTool,
            TextLabel,
            AnchoredText,
            Callout,
            FlagNote,
            Signpost,
        ] {
            let mut drawing = pattern(tool);
            drawing.completed = true;
            for shape in paint(&drawing) {
                if let Some((center, radius)) = marker(&shape) {
                    assert!(
                        tool == FlagNote
                            && center.distance(drawing.points[0]) < 0.01
                            && radius <= crate::tokens::DESIGN_TOKENS.rounding.sm,
                        "{tool:?} paints an anchor dot"
                    );
                }
            }
        }
        for (tool, count) in [
            (Ray, 1),
            (ExtendedLine, 1),
            (Arc, 1),
            (Arc, 2),
            (Polyline, 1),
            (Path, 1),
            (PriceRange, 1),
            (DateRange, 1),
            (Callout, 1),
            (Pitchfork, 1),
            (Pitchfork, 2),
            (SchiffPitchfork, 1),
            (ModifiedSchiffPitchfork, 1),
            (InsidePitchfork, 1),
            (XABCDPattern, 1),
            (ElliottImpulse, 1),
            (Curve, 2),
            (FibonacciChannel, 2),
            (BarsPattern, 2),
            (ProjectionTool, 2),
        ] {
            let mut drawing = pattern(tool);
            drawing.points.truncate(count);
            drawing.chart_points.truncate(count);
            assert!(
                paint(&drawing).iter().any(|s| marker(s).is_some()),
                "{tool:?} lost its {count}-anchor preview"
            );
        }
    }

    #[test]
    fn main_widths_are_exact_and_details_stay_thin() {
        use DrawingToolType::*;
        for tool in [
            XABCDPattern,
            CypherPattern,
            HeadAndShoulders,
            ABCDPattern,
            TrianglePattern,
            ThreeDrivesPattern,
            ElliottImpulse,
            ElliottCorrection,
            ElliottTriangle,
            ElliottDoubleCombo,
            ElliottTripleCombo,
            CyclicLines,
            TimeCycles,
            SineLine,
            GannFan,
            GannBox,
            GannSquare,
            GannFixed,
            Brush,
            Highlighter,
            Callout,
        ] {
            for width in [1.0, 2.0, 3.0] {
                let mut drawing = pattern(tool);
                drawing.stroke_width = width;
                if matches!(tool, Brush | Highlighter) {
                    drawing.color = [41, 98, 255, 60];
                }
                let shapes = paint(&drawing);
                let mut main = 0;
                for shape in &shapes {
                    let (is_main, stroke) = match shape {
                        Shape::LineSegment { points, stroke } => (
                            drawing.points.windows(2).any(|p| p == points)
                                || tool == CyclicLines
                                || matches!(tool, GannFan | GannBox | GannSquare | GannFixed),
                            *stroke,
                        ),
                        Shape::Path(p) => {
                            let ColorMode::Solid(color) = p.stroke.color else {
                                continue;
                            };
                            (
                                p.points
                                    .windows(2)
                                    .any(|p| drawing.points.windows(2).any(|leg| leg == p))
                                    || matches!(tool, Brush | Highlighter | SineLine | Callout)
                                    || matches!(tool, GannSquare | GannFixed)
                                    || (tool == TimeCycles
                                        && p.points.len() > 4
                                        && color == drawing.color32()),
                                Stroke::new(p.stroke.width, color),
                            )
                        }
                        Shape::Rect(r) => (tool == Callout, r.stroke),
                        _ => continue,
                    };
                    if stroke.is_empty() {
                        continue;
                    }
                    if is_main {
                        main += 1;
                        assert_eq!(stroke.width, width, "{tool:?} main at {width}");
                        if matches!(tool, Brush | Highlighter) {
                            assert_eq!(stroke.color, drawing.color32(), "{tool:?} colour");
                        }
                    } else if stroke.color != Color32::WHITE
                        && (stroke.color.a() < 255
                            || matches!(shape, Shape::Path(p) if p.closed && p.points.len() >= 8))
                    {
                        assert!(
                            stroke.width <= 2.0,
                            "{tool:?} detail widened to {}",
                            stroke.width
                        );
                    }
                }
                assert!(main > 0, "{tool:?} missing main line");
            }
        }
    }

    #[test]
    fn pattern_fills_toggle_without_recolouring_labels() {
        use DrawingToolType::*;
        for tool in [
            XABCDPattern,
            CypherPattern,
            HeadAndShoulders,
            TrianglePattern,
        ] {
            let mut drawing = pattern(tool);
            assert!(
                drawing.fill_color.is_some(),
                "{tool:?} lost its default tint"
            );
            let coloured = Drawing::with_color(1, tool, [67, 89, 123, 255]);
            assert_eq!(
                &coloured.fill_color.unwrap()[..3],
                &[67, 89, 123],
                "{tool:?} default fill must follow its line colour"
            );
            let configured = Color32::from_rgba_unmultiplied(12, 34, 56, 80);
            for fill in [None, Some([12, 34, 56, 80])] {
                drawing.fill_color = fill;
                let shapes = paint(&drawing);
                let mut bodies = 0;
                for shape in &shapes {
                    match shape {
                        Shape::Path(p) if p.closed && p.points.len() <= 7 => {
                            bodies += usize::from(p.fill.a() > 0);
                            assert_eq!(
                                p.fill,
                                fill.map_or(Color32::TRANSPARENT, |_| configured),
                                "{tool:?} body"
                            );
                        }
                        Shape::Rect(r) if r.fill.a() > 0 => {
                            assert_ne!(r.fill, configured, "{tool:?} label recoloured");
                        }
                        _ => {}
                    }
                }
                assert_eq!(bodies > 0, fill.is_some(), "{tool:?} fill toggle");
            }
        }
    }

    /// Guards the circles of drawings at 1x and 2x: a circle with an outline, or too large for a
    /// textured quad, becomes a closed polygon whose corners lie on the circle and whose sides
    /// stay within a quarter of a physical pixel of it, with the outline still outside the
    /// radius; a small disc without an outline stays a circle.
    #[test]
    fn circles_become_polygons_within_a_quarter_pixel() {
        let center = Pos2::new(50.0, 40.0);
        let ring = Stroke::new(1.0, Color32::WHITE);
        for (radius, ppp, stroke, polygon) in [
            (3.0, 1.0, Stroke::NONE, false),
            (3.0, 2.0, Stroke::NONE, false),
            (3.0, 1.0, ring, true),
            (8.0, 1.0, Stroke::NONE, true),
            (8.0, 2.0, ring, true),
            (200.0, 2.0, ring, true),
            (5000.0, 1.0, ring, true),
            (5000.0, 2.0, ring, true),
            (8.0, 1.0, Stroke::new(100.0, Color32::WHITE), true),
            (8.0, 2.0, Stroke::new(100.0, Color32::WHITE), true),
        ] {
            let circle = egui::epaint::CircleShape {
                center,
                radius,
                fill: Color32::RED,
                stroke,
            };
            let mut shape = Shape::Circle(circle);
            fewer_circle_sides(&mut shape, ppp, Rect::EVERYTHING);
            let case = format!("radius {radius} at {ppp}x");
            let Shape::Path(path) = &shape else {
                assert!(!polygon, "{case}: still a circle");
                continue;
            };
            assert!(polygon, "{case}: a quad disc became a polygon");
            assert!(path.closed, "{case}");
            assert_eq!(
                (path.fill, path.stroke.kind),
                (Color32::RED, StrokeKind::Outside)
            );
            if !stroke.is_empty() {
                let mut drawing = Drawing::new(1, DrawingToolType::Circle);
                drawing.points = vec![center, center + egui::vec2(radius, 0.0)];
                drawing.completed = true;
                assert!(paint(&drawing).iter().any(|shape| {
                    matches!(shape, Shape::Path(p) if p.stroke.width > 0.0 && p.stroke.kind == StrokeKind::Outside)
                }), "{case}: drawing circle lost its outward stroke");
            }
            let corners = &path.points;
            for (i, a) in corners.iter().enumerate() {
                let b = corners[(i + 1) % corners.len()];
                assert!(((*a - center).length() - radius).abs() < 1e-3, "{case}");
                let error = (radius - (center.distance(a.lerp(b, 0.5)))) * ppp;
                assert!((0.0..=0.25).contains(&error), "{case}: {error} px");
            }
            // Outside joins must stay on the outer circle even when the stroke exceeds the radius.
            let mut mesh = egui::Mesh::default();
            egui::epaint::Tessellator::new(
                ppp,
                egui::epaint::TessellationOptions {
                    feathering: false,
                    ..Default::default()
                },
                [1, 1],
                Vec::new(),
            )
            .tessellate_shape(shape, &mut mesh);
            for vertex in &mesh.vertices {
                let error = (vertex.pos.distance(center) - radius - stroke.width) * ppp;
                assert!(
                    error <= 0.25,
                    "{case}: outer stroke exceeds the circle by {error} px"
                );
            }
        }
    }

    #[test]
    fn zoomed_curves_and_brushes_follow_their_geometry() {
        use DrawingToolType as T;
        // Circle at pi/2 + 0.06, ellipse at pi/2 + 0.04; quadratic at t = 0.51; equal chord
        // Catmull-Rom knots at t = 1/4 of the middle span (tangents (50, 50)).
        for (tool, anchors, expected) in [
            (
                T::Circle,
                vec![(400., -4800.), (400., 200.)],
                (100.17997, 191.0027),
            ),
            (
                T::Circle,
                vec![(400., -999800.), (400., 200.)],
                (100., 199.955),
            ),
            (
                T::FibonacciCircles,
                vec![(-999600., -1999800.), (1000400., 200.)],
                (100., 199.955),
            ),
            (
                T::FibonacciSpeedResistanceArcs,
                vec![(400., -999800.), (400., 200.)],
                (100., 199.955),
            ),
            (
                T::Ellipse,
                vec![(-6600., -9800.), (7400., 200.)],
                (120.07466, 196.00053),
            ),
            (
                T::Arc,
                vec![(-4600., -4800.), (5400., -4800.), (400., 200.)],
                (100.17997, 191.0027),
            ),
            (
                T::Sector,
                vec![(400., -4800.), (5400., -4800.), (-4600., -4800.)],
                (100.17997, 191.0027),
            ),
            (
                T::DoubleCurve,
                vec![
                    (-5000., -5000.),
                    (-1400., 1933.3334),
                    (2200., 1933.3334),
                    (5800., -5000.),
                ],
                (508., 197.92),
            ),
            (
                T::Curve,
                vec![(-5000., -5000.), (400., 5400.), (5800., -5000.)],
                (508., 197.92),
            ),
            (
                T::DoubleCurve,
                vec![(100., 100.), (200., 500.), (300., -300.), (400., 100.)],
                (175., 212.5),
            ),
            (
                T::DoubleCurve,
                vec![(100., 100.), (600., 100.), (-400., 100.), (200., 100.)],
                (242.1875, 100.),
            ),
            (
                T::SineLine,
                vec![(100., 50.), (125., 550.)],
                (102.5, 62.23587),
            ),
            (
                T::Brush,
                vec![(100., 100.), (200., 100.), (200., 200.), (300., 200.)],
                (204.6875, 120.3125),
            ),
            // Chord lengths 100, 400, 400 give knot intervals 10, 20, 20 and
            // middle-span tangents (400/3,400/3) and (200,200).
            (
                T::Brush,
                vec![(0., 0.), (100., 0.), (100., 400.), (500., 400.)],
                (109.375, 71.875),
            ),
            (
                T::Highlighter,
                vec![
                    (100., 100.),
                    (200., 100.),
                    (200., 100.),
                    (200., 200.),
                    (300., 200.),
                ],
                (204.6875, 120.3125),
            ),
            (T::Brush, vec![(100., 100.), (300., 200.)], (200., 150.)),
        ] {
            for ppp in [1.0, 2.0] {
                let mut drawing = Drawing::new(1, tool);
                drawing.points = anchors.iter().copied().map(Pos2::from).collect();
                drawing.completed = true;
                drawing.fill_color = None;
                if matches!(tool, T::FibonacciCircles | T::FibonacciSpeedResistanceArcs) {
                    drawing.fib_config = Some(crate::drawings::FibonacciConfig {
                        levels: vec![crate::drawings::FibonacciLevel::new(1.0, "", [255; 4])],
                        show_background: false,
                        ..Default::default()
                    });
                }
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(ppp);
                let shapes = paint_on(&ctx, &drawing, false);
                let mut lines = Vec::new();
                for shape in shapes {
                    match shape {
                        Shape::Path(p) => {
                            lines.extend(p.points.windows(2).map(|p| [p[0], p[1]]));
                            if p.closed {
                                lines.push([*p.points.last().unwrap(), p.points[0]]);
                            }
                        }
                        Shape::LineSegment { points, .. } => lines.push(points),
                        _ => {}
                    }
                }
                assert!(lines.len() < 2048, "{tool:?}: {} segments", lines.len());
                if matches!(tool, T::Brush | T::Highlighter) {
                    for &sample in &drawing.points {
                        assert!(
                            lines.iter().flatten().any(|&p| p == sample),
                            "lost sample {sample:?}"
                        );
                    }
                }
                let point = Pos2::from(expected);
                let distance = lines
                    .iter()
                    .map(|&[a, b]| point_to_segment_distance(point, a, b))
                    .fold(f32::INFINITY, f32::min);
                assert!(distance * ppp <= 0.25, "{tool:?} at {ppp}x: {distance} pt");
                if matches!(tool, T::Circle | T::Ellipse) {
                    let (center, axes) = if tool == T::Circle {
                        (
                            drawing.points[0],
                            egui::Vec2::splat(drawing.points[0].distance(drawing.points[1])),
                        )
                    } else {
                        (
                            drawing.points[0].lerp(drawing.points[1], 0.5),
                            (drawing.points[1] - drawing.points[0]).abs() * 0.5,
                        )
                    };
                    let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800., 600.));
                    for &[a, b] in &lines {
                        let mid = a.lerp(b, 0.5);
                        if !pane.contains(mid) {
                            continue;
                        }
                        let dx = f64::from(mid.x) - f64::from(center.x);
                        let dy = f64::from(mid.y) - f64::from(center.y);
                        let length = (dx / f64::from(axes.x)).hypot(dy / f64::from(axes.y));
                        let error = dx.hypot(dy) * (1.0 / length - 1.0).abs() * f64::from(ppp);
                        assert!(error <= 0.25, "{tool:?} conic error {error} px");
                    }
                }
                if matches!(
                    tool,
                    T::FibonacciCircles | T::FibonacciSpeedResistanceArcs | T::SineLine
                ) {
                    continue;
                }
                let outline = drawing
                    .outline(Rect::from_min_size(Pos2::ZERO, egui::vec2(800., 600.)))
                    .unwrap();
                assert!(
                    outline
                        .lines
                        .iter()
                        .any(|&[a, b]| point_to_segment_distance(point, a, b) <= 0.25),
                    "{tool:?} hit outline"
                );
            }
        }
    }

    /// Guards the frame cost of dashed drawings: a rect, a circle and an ellipse reaching a
    /// million points beyond an 800 x 600 pane are dashed only inside it, rather than with one
    /// shape per dash along their whole outline.
    #[test]
    fn far_reaching_dashed_outlines_are_dashed_only_inside_the_pane() {
        use crate::drawings::rendering::ChartView;
        use egui::{Context, RawInput};
        fn leaves(shape: &Shape) -> usize {
            match shape {
                Shape::Vec(shapes) => shapes.iter().map(leaves).sum(),
                _ => 1,
            }
        }
        let pane = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        for (tool, a, b) in [
            (DrawingToolType::Rect, (-1e6, 100.0), (1e6, 200.0)),
            (
                DrawingToolType::Circle,
                (400.0, 1e6 + 300.0),
                (400.0, 300.0),
            ),
            (DrawingToolType::Ellipse, (-1e6, 300.0), (1e6, 2e6 + 300.0)),
        ] {
            let mut drawing = Drawing::new(1, tool);
            drawing.line_style = LineStyle::Dashed;
            drawing.points = vec![Pos2::new(a.0, a.1), Pos2::new(b.0, b.1)];
            let ctx = Context::default();
            let input = RawInput {
                screen_rect: Some(pane),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                let view = ChartView {
                    price_formatter: None,
                    thousands_sep: None,
                    time_formatter: None,
                    bar_seconds: 0,
                    instrument: None,
                    bars: &[],
                    revision: 0,
                    bar_to_x: &|b| b,
                    bar_spacing: 1.0,
                    price_to_y: &|p| p as f32,
                    y_to_price: &f64::from,
                    time_axis: true,
                    executions: None,
                    timeframe: None,
                    finer: &[],
                };
                drawing.render_original(ui.painter(), pane, &view);
            });
            let shapes: usize = output.shapes.iter().map(|s| leaves(&s.shape)).sum();
            output.drop_without_applying_deltas();
            assert!((1..1_000).contains(&shapes), "{tool:?}: {shapes} shapes");
        }
    }

    /// Bold text paints a second copy 0.6 pt along its own baseline, so text turned along a
    /// horizontal, a 45 degree and a vertical line thickens its stems alike.
    #[test]
    fn bold_text_thickens_along_its_baseline() {
        for end in [(300.0, 300.0), (300.0, 100.0), (100.0, 100.0)] {
            let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
            drawing.points = vec![Pos2::new(100.0, 300.0), Pos2::new(end.0, end.1)];
            drawing.text = Some("Weekly support".into());
            drawing.font_weight = FontWeight::Bold;
            let texts: Vec<_> = paint(&drawing)
                .into_iter()
                .filter_map(|s| match s {
                    Shape::Text(t) => Some(t),
                    _ => None,
                })
                .collect();
            let [text, copy] = texts.as_slice() else {
                panic!("{end:?}: {} texts", texts.len());
            };
            let offset = egui::emath::Rot2::from_angle(-text.angle) * (copy.pos - text.pos);
            assert!(
                (offset - egui::vec2(0.6, 0.0)).length() < 1e-4,
                "{end:?}: {offset:?}"
            );
        }
    }
}
