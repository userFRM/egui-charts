//! The Magic cursor's look: the wand it shows over a plot and the emoji confetti a click throws
//! from its tip. Every constant of the effect is here.

use egui::{Color32, Pos2, Rect, Vec2};

/// Emoji in one burst.
const COUNT: usize = 40;
/// Most emoji in flight at once; a new burst takes the place of the oldest emoji beyond it.
const MAX_PARTICLES: usize = 200;
/// Seconds an emoji lives, unless it falls out through the bottom of its pane first.
const LIFETIME: f32 = 3.0;
/// Font size of every emoji, in points.
const SIZE: f32 = 20.0;
/// Launch speeds, in points per second.
const SPEED: std::ops::Range<f32> = 600.0..1400.0;
/// Largest launch angle either side of straight up, in radians (30 degrees).
const SPREAD: f32 = std::f32::consts::FRAC_PI_6;
/// Rate at which an emoji's velocity settles from its launch towards its drift and fall, per
/// second.
const DRAG: f32 = 3.0;
/// Downward pull, in points per second squared. With `DRAG` the fall settles at
/// `GRAVITY / DRAG`, 300 points per second.
const GRAVITY: f32 = 900.0;
/// Sideways drift at the largest tilt, in points per second. An emoji drifts the way it leans.
const DRIFT: f32 = 150.0;
/// Largest lean either way, in radians clockwise (30 degrees).
const TILT: f32 = std::f32::consts::FRAC_PI_6;
/// The emoji, all offered by the emoji picker and drawn by the bundled emoji font, each with
/// the colour its monochrome glyph takes. Every tint has at least 3:1 contrast on black and on
/// white.
const MIX: [(&str, Color32); 12] = [
    ("😀", Color32::from_rgb(230, 100, 0)),
    ("😂", Color32::from_rgb(196, 120, 0)),
    ("😎", Color32::from_rgb(160, 110, 0)),
    ("👍", Color32::from_rgb(41, 98, 255)),
    ("👏", Color32::from_rgb(126, 87, 194)),
    ("🔥", Color32::from_rgb(232, 60, 20)),
    ("🌈", Color32::from_rgb(200, 40, 170)),
    ("🐂", Color32::from_rgb(168, 92, 40)),
    ("🚀", Color32::from_rgb(20, 120, 220)),
    ("💰", Color32::from_rgb(30, 140, 60)),
    ("🎯", Color32::from_rgb(220, 40, 50)),
    ("🎂", Color32::from_rgb(225, 30, 100)),
];

/// One emoji in flight.
#[derive(Clone, Debug)]
struct Particle {
    /// The plot of the pane the burst was thrown on, which clips the emoji.
    plot: Rect,
    /// The wand's tip at the click.
    origin: Pos2,
    /// Input time of the click, in seconds.
    born: f64,
    /// Launch velocity in points per second, `y` pointing down.
    velocity: Vec2,
    /// Lean in radians clockwise, which also sets the drift.
    tilt: f32,
    /// Index into `MIX`.
    emoji: usize,
}

impl Particle {
    /// The centre `age` seconds after the click. The velocity settles at `DRAG` per second
    /// from its launch value towards the drift and the fall.
    fn at(&self, age: f32) -> Pos2 {
        let settled = Vec2::new(DRIFT * self.tilt / TILT, GRAVITY / DRAG);
        let decayed = (1.0 - (-DRAG * age).exp()) / DRAG;
        self.origin + settled * age + (self.velocity - settled) * decayed
    }
}

/// The emoji in flight and the generator their bursts draw from.
#[derive(Clone, Debug, Default)]
pub(super) struct Confetti {
    particles: Vec<Particle>,
    /// SplitMix64 state. The fixed start makes every chart's bursts repeatable.
    rng: u64,
}

impl Confetti {
    /// Throws a burst from `tip` on `plot` at `now` seconds.
    pub(super) fn throw(&mut self, plot: Rect, tip: Pos2, now: f64) {
        let excess = (self.particles.len() + COUNT).saturating_sub(MAX_PARTICLES);
        self.particles.drain(..excess);
        for _ in 0..COUNT {
            let speed = SPEED.start + (SPEED.end - SPEED.start) * self.unit();
            let angle = SPREAD * (2.0 * self.unit() - 1.0);
            let tilt = TILT * (2.0 * self.unit() - 1.0);
            let emoji = (self.next() % MIX.len() as u64) as usize;
            self.particles.push(Particle {
                plot,
                origin: tip,
                born: now,
                velocity: speed * Vec2::new(angle.sin(), -angle.cos()),
                tilt,
                emoji,
            });
        }
    }

    /// Drops every emoji in flight.
    pub(super) fn clear(&mut self) {
        self.particles.clear();
    }

    /// Drops the emoji that are done and paints the rest, each clipped to its pane within
    /// `column`, the plot column of every pane; keeps repainting while any is in flight.
    pub(super) fn paint(&mut self, ui: &egui::Ui, column: Rect) {
        let now = ui.input(|input| input.time);
        self.particles.retain(|particle| {
            let age = (now - particle.born) as f32;
            (0.0..LIFETIME).contains(&age) && particle.at(age).y <= particle.plot.bottom() + SIZE
        });
        for particle in &self.particles {
            // shortcut: a pane that moves or closes mid-flight keeps its old rectangle for the
            // rest of the burst (at most `LIFETIME`); look the pane up by id if that shows.
            let painter = ui.painter().with_clip_rect(particle.plot.intersect(column));
            let center = particle.at((now - particle.born) as f32);
            paint_emoji(&painter, particle.emoji, center, particle.tilt);
        }
        if !self.particles.is_empty() {
            ui.ctx().request_repaint();
        }
    }

    /// SplitMix64.
    fn next(&mut self) -> u64 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0.0..1.0`.
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Paints emoji `index` of `MIX` centred on `center`, turned `tilt` radians clockwise. Glyphs
/// of a colour font keep their own colours.
fn paint_emoji(painter: &egui::Painter, index: usize, center: Pos2, tilt: f32) {
    let (emoji, tint) = MIX[index];
    let galley = painter.layout_no_wrap(
        emoji.to_owned(),
        crate::drawings::glyphs::font_id(painter.ctx(), emoji, SIZE),
        tint,
    );
    let min = center - galley.rect.center().to_vec2();
    painter.add(
        egui::epaint::TextShape::new(min, galley, tint)
            .with_angle_and_anchor(tilt, egui::Align2::CENTER_CENTER),
    );
}

/// Paints the wand with its tip at `tip` and its handle running down to the right: a rod in
/// the theme's text colour with an accent tip, outlined in the chart background.
pub(super) fn paint_wand(ui: &egui::Ui, clip: Rect, tip: Pos2) {
    use crate::theme::ThemeExt;
    let theme = ui.active_theme();
    let mut mesh = egui::Mesh::default();
    // Laid out along +x from the tip, then turned 45 degrees clockwise about it.
    for (x, half_width, color) in [
        (-1.0..=23.0, 2.5, theme.chart.bg),
        (5.0..=22.0, 1.5, theme.ui.text),
        (0.0..=5.0, 1.5, theme.ui.accent),
    ] {
        mesh.add_colored_rect(
            Rect::from_x_y_ranges(
                tip.x + x.start()..=tip.x + x.end(),
                tip.y - half_width..=tip.y + half_width,
            ),
            color,
        );
    }
    mesh.rotate(
        egui::emath::Rot2::from_angle(std::f32::consts::FRAC_PI_4),
        tip,
    );
    ui.painter().with_clip_rect(clip).add(mesh);
}

#[cfg(test)]
mod tests {
    use super::MIX;
    use crate::config::CrosshairStyle;
    use crate::widget::Chart;
    use crate::widget::test_frames::chart;
    use egui::{
        Context, CursorIcon, Event, PointerButton, Pos2, RawInput, Rect, Shape, ViewportId, vec2,
    };
    use std::time::Duration;

    /// What a frame painted and asked for: the centres of the emoji, the vertex bounds of each
    /// mesh, the cursor, the repaint delay and how many emoji were drawn in colour.
    type Painted = (Vec<Pos2>, Vec<Rect>, CursorIcon, Duration, usize);

    /// Runs one frame of `chart` at `time` seconds with `events` on an 800x600 screen.
    fn step(ctx: &Context, chart: &mut Chart, time: f64, events: Vec<Event>) -> Painted {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            time: Some(time),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                chart.show(ui);
            });
        });
        fn collect(
            shape: &Shape,
            emoji: &mut Vec<Pos2>,
            meshes: &mut Vec<Rect>,
            color: &mut usize,
        ) {
            match shape {
                Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, emoji, meshes, color)),
                Shape::Text(text) if MIX.iter().any(|(e, _)| text.galley.text() == *e) => {
                    emoji.push(
                        text.pos
                            + egui::emath::Rot2::from_angle(text.angle)
                                * text.galley.rect.center().to_vec2(),
                    );
                    let rows = text.galley.rows.iter();
                    *color += usize::from(rows.flat_map(|r| &r.glyphs).any(|g| g.is_color));
                }
                Shape::Mesh(mesh) => meshes.push(Rect::from_points(
                    &mesh.vertices.iter().map(|v| v.pos).collect::<Vec<_>>(),
                )),
                _ => {}
            }
        }
        let (mut emoji, mut meshes, mut color) = (Vec::new(), Vec::new(), 0);
        for clipped in std::mem::take(&mut output.shapes) {
            collect(&clipped.shape, &mut emoji, &mut meshes, &mut color);
        }
        let painted = (
            emoji,
            meshes,
            output.platform_output.cursor_icon,
            output.viewport_output[&ViewportId::ROOT].repaint_delay,
            color,
        );
        output.drop_without_applying_deltas();
        painted
    }

    /// Through `Chart::show` under the Magic cursor: the wand shows at the pointer, a long press
    /// throws nothing, a click throws 40 emoji from the tip that rise and spread, then fall and
    /// spread further, and are all gone three seconds later, with repaints only while they fly;
    /// rapid clicks never keep more than 200 in flight and each new burst is whole; and changing
    /// the cursor drops them.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn magic_bursts_fly_for_three_seconds_and_stay_bounded() {
        let ctx = Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        chart.chart_options.crosshair.style = CrosshairStyle::Magic;
        step(&ctx, &mut chart, 0.0, vec![]);
        let plot = chart.last_rendered_price_rect;
        let tip = plot.center_top() + vec2(0.0, 120.0);
        // No emoji leaves through the bottom before 2 s, when the lowest is 327.47 below the tip.
        assert!(tip.y + 327.5 <= plot.bottom() + 20.0, "{plot:?}");
        let button = |pressed| Event::PointerButton {
            pos: tip,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let near = |a: f32, b: f32| (a - b).abs() <= 0.01;
        let at_tip = |emoji: &[Pos2]| {
            emoji
                .iter()
                .filter(|c| near(c.x, tip.x) && near(c.y, tip.y))
                .count()
        };
        let span = |emoji: &[Pos2]| Rect::from_points(emoji).size();

        // Hover is hit-tested against the widgets of the previous frame.
        step(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(tip)]);
        let (emoji, meshes, cursor, _, _) =
            step(&ctx, &mut chart, 0.2, vec![Event::PointerMoved(tip)]);
        assert_eq!((emoji.len(), cursor), (0, CursorIcon::None));
        // The wand's outline, 24 by 5 points from the tip, turned 45 degrees about it.
        assert!(
            meshes.iter().any(|m| {
                near(m.min.x, tip.x - 2.47)
                    && near(m.min.y, tip.y - 2.47)
                    && near(m.max.x, tip.x + 18.03)
                    && near(m.max.y, tip.y + 18.03)
            }),
            "no wand at {tip:?}"
        );
        // egui repaints one frame past the last request, under every cursor.
        step(&ctx, &mut chart, 0.8, vec![]);
        assert_eq!(step(&ctx, &mut chart, 0.9, vec![]).3, Duration::MAX);

        // Held 1.1 s, longer than a click.
        for (time, events) in [
            (1.0, vec![button(true)]),
            (2.0, vec![]),
            (2.1, vec![button(false)]),
        ] {
            assert_eq!(step(&ctx, &mut chart, time, events).0.len(), 0, "at {time}");
        }

        step(&ctx, &mut chart, 3.0, vec![button(true)]);
        let emoji = step(&ctx, &mut chart, 3.05, vec![button(false)]).0;
        assert_eq!((emoji.len(), at_tip(&emoji)), (40, 40));
        // Hand-computed bounds of every emoji 0.5 s and 2 s after the click.
        let mut widths = Vec::new();
        for (time, dy, dx) in [
            (3.55, -290.23..=-62.24, 217.43),
            (5.05, 34.74..=327.47, 482.88),
        ] {
            let (emoji, _, _, delay, color) = step(&ctx, &mut chart, time, vec![]);
            assert_eq!((emoji.len(), delay), (40, Duration::ZERO), "at {time}");
            // The colour emoji font, installed by the first burst, draws them in colour.
            let colored = if cfg!(feature = "color-emoji") { 40 } else { 0 };
            assert_eq!(color, colored, "at {time}");
            for c in &emoji {
                let (x, y) = (c.x - tip.x, c.y - tip.y);
                assert!(
                    y >= dy.start() - 0.01 && y <= dy.end() + 0.01 && x.abs() <= dx + 0.01,
                    "at {time}: {c:?} from {tip:?}"
                );
            }
            let size = span(&emoji);
            assert!(size.x > 50.0 && size.y > 50.0, "at {time}: {size:?}");
            widths.push(size.x);
        }
        assert!(widths[1] > widths[0], "{widths:?}");
        assert_eq!(step(&ctx, &mut chart, 6.05, vec![]).0.len(), 0);
        assert_eq!(step(&ctx, &mut chart, 6.1, vec![]).3, Duration::MAX);

        for i in 0..6 {
            let t = 7.0 + 0.1 * f64::from(i);
            step(&ctx, &mut chart, t, vec![button(true)]);
            let emoji = step(&ctx, &mut chart, t + 0.05, vec![button(false)]).0;
            assert_eq!(emoji.len(), (40 * (i + 1)).min(200) as usize, "click {i}");
            assert_eq!(at_tip(&emoji), 40, "click {i}");
        }
        chart.chart_options.crosshair.style = CrosshairStyle::Full;
        assert_eq!(step(&ctx, &mut chart, 7.6, vec![]).0.len(), 0);
    }
}
