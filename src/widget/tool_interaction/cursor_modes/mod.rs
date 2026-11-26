//! Cursor Modes Module
//!
//! Cursor modes for chart interaction.
//! Each cursor type provides different visual feedback and behavior.
//!
//! ## Cursor Types
//! - **Cross**: Full crosshair with vertical + horizontal lines
//! - **Dot**: crosshair lines with a dot at the cursor position
//! - **Arrow**: Standard pointer, no crosshair overlay
//! - **Demonstration** and **Magic**: fading marks of a press
//! - **Eraser**: Click to delete drawings

pub mod eraser;

use crate::config::CrosshairStyle;

pub use eraser::EraserMode;

/// Seconds a released Demonstration or Alt (Option) mark takes to fade out.
const MARK_FADE: f32 = 3.0;
/// Seconds each point of a Magic trail takes to fade out behind the pointer.
const TRAIL_FADE: f32 = 0.5;

/// Aggregate state for all cursor modes.
///
/// Holds the state for each cursor mode variant (eraser).
/// The chart widget owns a single `CursorModeState` and delegates input events
/// and rendering to the individual mode states.
#[derive(Clone, Debug, Default)]
pub struct CursorModeState {
    /// Eraser mode: click on drawings to delete them.
    pub eraser: EraserMode,
    /// Marked points with their age in seconds and whether they continue the stroke of the
    /// point before.
    trail: Vec<(egui::Pos2, f32, bool)>,
    last_pointer: Option<egui::Pos2>,
    /// The cursor the trail was drawn under, `None` for the eraser.
    cursor: Option<CrosshairStyle>,
    /// The newest stroke is held and waits for its release to fade.
    hold: bool,
    /// The primary gesture in progress marks the chart.
    marking: bool,
    /// The plot column of every pane, where the last press was checked and the trail is
    /// painted.
    plot: Option<egui::Rect>,
}

impl CursorModeState {
    /// Create a new state with all cursor modes inactive.
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance all cursor mode animations by `dt` seconds (call once per frame).
    pub fn update(&mut self, dt: f32) {
        // A held stroke starts at the last point that does not continue another.
        let fading = if self.hold {
            self.trail
                .iter()
                .rposition(|(_, _, joined)| !joined)
                .unwrap_or(0)
        } else {
            self.trail.len()
        };
        for (_, age, _) in &mut self.trail[..fading] {
            *age += dt.max(0.0);
        }
        let fade = self.fade();
        self.trail.retain(|(_, age, _)| *age < fade);
    }

    /// Seconds a point takes to fade out once it ages: a Magic trail's, or a released mark's.
    fn fade(&self) -> f32 {
        if self.cursor == Some(CrosshairStyle::Magic) {
            TRAIL_FADE
        } else {
            MARK_FADE
        }
    }

    /// Paint the trail over the plot column of every pane, above their content, and keep
    /// repainting while it fades.
    pub fn render(&self, ui: &mut egui::Ui) {
        use crate::theme::ThemeExt;
        let Some(plot) = self.plot else {
            return;
        };
        let painter = ui.painter().with_clip_rect(plot);
        let fade = self.fade();
        for pair in self.trail.windows(2).filter(|pair| pair[1].2) {
            painter.line_segment(
                [pair[0].0, pair[1].0],
                egui::Stroke::new(
                    6.0,
                    ui.active_theme()
                        .ui
                        .accent
                        .gamma_multiply((1.0 - pair[0].1 / fade) * 0.65),
                ),
            );
        }
        for (pos, age, _) in &self.trail {
            painter.circle_filled(
                *pos,
                5.0,
                ui.active_theme()
                    .ui
                    .accent
                    .gamma_multiply((1.0 - age / fade) * 0.65),
            );
        }
        if !self.trail.is_empty() {
            ui.ctx().request_repaint();
        }
    }

    /// Whether the primary gesture marks the chart instead of panning, selecting or erasing. The
    /// press decides for the whole gesture: one that `response`, the plots of every pane, receives
    /// on `plot`, the plot column of every pane, marks when `free` and either `trail` (a
    /// Demonstration or Magic cursor) or Alt (Option) is held without Ctrl or Command. `plot` is
    /// kept for painting the trail.
    pub(crate) fn marks(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        plot: egui::Rect,
        trail: bool,
        free: bool,
    ) -> bool {
        self.plot = Some(plot);
        let (pressed, held, alt, origin) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down() || i.pointer.primary_released(),
                i.modifiers.alt && !i.modifiers.ctrl && !i.modifiers.command,
                i.pointer.press_origin().or(i.pointer.interact_pos()),
            )
        });
        if pressed {
            self.marking = free
                && (trail || alt)
                && (response.is_pointer_button_down_on() || response.clicked())
                && origin.is_some_and(|pos| plot.contains(pos));
        } else if !held {
            self.marking = false;
        }
        self.marking
    }

    /// Whether the primary gesture that [`Self::marks`] last decided on marks the chart.
    pub(crate) fn marking(&self) -> bool {
        self.marking
    }

    /// `response` as the drawings see it: while the primary gesture marks the chart, the hover
    /// without the press, click or drag that the mark owns.
    pub(crate) fn drawing_response(&self, response: &egui::Response) -> egui::Response {
        let mut response = response.clone();
        if self.marking {
            use egui::response::Flags;
            response.flags.remove(
                Flags::CLICKED
                    | Flags::FAKE_PRIMARY_CLICKED
                    | Flags::DRAG_STARTED
                    | Flags::DRAGGED
                    | Flags::DRAG_STOPPED
                    | Flags::IS_POINTER_BUTTON_DOWN_ON,
            );
        }
        response
    }

    /// Extend the trail with `mark`, the pointer of a marking gesture over the plot this frame.
    /// `cursor` is the cursor in force, `None` for the eraser; changing it drops the trail. While
    /// `held`, the newest stroke stays whole, except under the Magic cursor, whose trail fades
    /// behind the pointer.
    pub(crate) fn follow(
        &mut self,
        cursor: Option<CrosshairStyle>,
        mark: Option<egui::Pos2>,
        held: bool,
    ) {
        if self.cursor != cursor {
            self.cursor = cursor;
            self.move_pointer(None);
        }
        match mark {
            Some(pos) => self.move_pointer(Some(pos)),
            None => self.last_pointer = None,
        }
        self.hold = held && self.last_pointer.is_some() && cursor != Some(CrosshairStyle::Magic);
        if !held {
            self.last_pointer = None;
        }
    }

    pub(crate) fn move_pointer(&mut self, pos: Option<egui::Pos2>) {
        if let Some(pos) = pos {
            if self
                .last_pointer
                .is_none_or(|last| last.distance(pos) > 1.0)
            {
                // shortcut: a held mark longer than 512 points loses its start; raise it if
                // presenters need longer strokes.
                if self.trail.len() == 512 {
                    self.trail.remove(0);
                }
                self.trail.push((pos, 0.0, self.last_pointer.is_some()));
                self.last_pointer = Some(pos);
            }
        } else {
            self.trail.clear();
            self.last_pointer = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stationary pointer must not restart a trail after its last point expires.
    #[test]
    fn held_trail_expires_without_restarting() {
        let mut modes = CursorModeState::new();
        modes.follow(Some(CrosshairStyle::Magic), None, false);
        for (dt, x, expected) in [
            (0.0, 10.0, 1),
            (0.1, 20.0, 2),
            (0.4, 20.0, 1),
            (0.2, 20.0, 0),
            (0.1, 20.0, 0),
            (0.1, 30.0, 1),
        ] {
            modes.update(dt);
            modes.move_pointer(Some(egui::pos2(x, 10.0)));
            assert_eq!(modes.trail.len(), expected, "at {x} after {dt}");
        }
    }

    /// A held Demonstration or Alt stroke stays whole until its release and a Magic trail fades
    /// behind the pointer; strokes are never joined, hover marks nothing, and changing the cursor
    /// drops the trail.
    #[test]
    fn trails_follow_their_cursor_and_clear_on_switching() {
        use CrosshairStyle::{Demonstration, Full, Magic};
        type Row = (
            f32,
            Option<CrosshairStyle>,
            Option<f32>,
            bool,
            &'static [bool],
        );
        const T: bool = true;
        const F: bool = false;
        let rows: [Row; 13] = [
            (0.0, Some(Demonstration), None, F, &[]),
            (0.1, Some(Demonstration), Some(10.0), T, &[F]),
            (0.4, Some(Demonstration), Some(20.0), T, &[F, T]),
            (0.4, Some(Demonstration), Some(30.0), F, &[F, T, T]),
            (0.1, Some(Demonstration), Some(40.0), T, &[F, T, T, F]),
            (0.1, Some(Demonstration), Some(40.0), F, &[F, T, T, F]),
            (2.85, Some(Demonstration), None, F, &[F]),
            (0.2, Some(Demonstration), None, F, &[]),
            (0.0, Some(Magic), Some(10.0), T, &[F]),
            (0.4, Some(Magic), Some(20.0), T, &[F, T]),
            (0.2, Some(Magic), Some(30.0), T, &[T, T]),
            (0.0, Some(Full), Some(40.0), T, &[F]),
            (0.0, None, None, F, &[]),
        ];
        let mut modes = CursorModeState::new();
        for (row, (dt, cursor, x, held, expected)) in rows.into_iter().enumerate() {
            modes.update(dt);
            modes.follow(cursor, x.map(|x| egui::pos2(x, 10.0)), held);
            let joined: Vec<_> = modes.trail.iter().map(|point| point.2).collect();
            assert_eq!(joined, expected, "row {}", row + 1);
        }
    }

    /// A released Demonstration or Alt mark is painted for three seconds and fades linearly: a
    /// third of its opacity is gone after one second and nothing is left after three.
    #[test]
    fn released_marks_fade_over_three_seconds() {
        use CrosshairStyle::{Demonstration, Full};
        let at = egui::pos2(100.0, 100.0);
        for style in [Demonstration, Full] {
            let ctx = egui::Context::default();
            let mut modes = CursorModeState::new();
            modes.plot = Some(egui::Rect::from_center_size(at, egui::vec2(200.0, 200.0)));
            modes.follow(Some(style), Some(at), true);
            modes.follow(Some(style), Some(at), false);
            let mut alphas = Vec::new();
            for dt in [0.0, 1.0, 2.0] {
                modes.update(dt);
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| modes.render(ui));
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                alphas.push(shapes.into_iter().find_map(|clipped| match clipped.shape {
                    egui::Shape::Circle(circle) if circle.center == at => Some(circle.fill.a()),
                    _ => None,
                }));
            }
            let &[Some(released), Some(one), None] = alphas.as_slice() else {
                panic!("{style:?}: {alphas:?}");
            };
            assert!(
                released > 0 && (f32::from(one) - f32::from(released) * 2.0 / 3.0).abs() <= 1.0,
                "{style:?}: {alphas:?}"
            );
        }
    }

    /// Through `Chart::show`, a click or a drag marks under the Demonstration cursor, or under
    /// the cross with Alt held, on the price pane and on a study pane, without panning or
    /// selecting, and the mark is painted above that pane; hovering marks nothing, and the same
    /// gestures without Alt pan or select as before.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn marks_follow_presses_through_show() {
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
        use crate::widget::test_frames::{chart, frame, run};
        use CrosshairStyle::{Demonstration, Full, Magic};
        use egui::{Context, Event, Modifiers, PointerButton, Shape, pos2, vec2};
        // Style, modifiers, gesture, then the trail length, whether the view moved and the
        // number of selected drawings and whether a series was selected.
        for (row, (style, modifiers, gesture, marks, pans, selected, series_selected)) in [
            (Demonstration, Modifiers::NONE, "hover", 0, false, 0, false),
            (Demonstration, Modifiers::NONE, "click", 1, false, 0, false),
            (Demonstration, Modifiers::NONE, "drag", 3, false, 0, false),
            (Full, Modifiers::ALT, "hover", 0, false, 0, false),
            (Full, Modifiers::ALT, "click", 1, false, 0, false),
            (Full, Modifiers::ALT, "drag", 3, false, 0, false),
            (Full, Modifiers::NONE, "drag", 0, true, 0, false),
            (Full, Modifiers::ALT, "click on line", 1, false, 0, false),
            (Full, Modifiers::NONE, "click on line", 0, false, 1, false),
            (
                Demonstration,
                Modifiers::NONE,
                "click on series",
                1,
                false,
                0,
                false,
            ),
            (
                Magic,
                Modifiers::NONE,
                "click on series",
                1,
                false,
                0,
                false,
            ),
            (Full, Modifiers::ALT, "click on series", 1, false, 0, false),
            (Full, Modifiers::NONE, "click on series", 0, false, 0, true),
            (
                Demonstration,
                Modifiers::NONE,
                "click on pane",
                1,
                false,
                0,
                false,
            ),
            (Full, Modifiers::ALT, "click on pane", 1, false, 0, false),
            (Full, Modifiers::NONE, "click on pane", 0, false, 0, true),
            (
                Full,
                Modifiers::ALT,
                "click on pane line",
                1,
                false,
                0,
                false,
            ),
            (
                Full,
                Modifiers::NONE,
                "click on pane line",
                0,
                false,
                1,
                false,
            ),
            (
                Demonstration,
                Modifiers::NONE,
                "drag across panes",
                3,
                false,
                0,
                false,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            chart.chart_options.crosshair.style = style;
            let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
            drawing.completed = true;
            drawing.chart_points = vec![ChartPoint::new(30.0, 18.0), ChartPoint::new(70.0, 18.0)];
            // Pane rows press on a study pane whose line runs at 50, under its own drawing.
            if gesture.contains("pane") {
                let study = chart.add_indicator(Box::new(
                    crate::studies::CustomIndicator::new("Pane", |bars| {
                        vec![crate::studies::IndicatorValue::Single(50.0); bars.len()]
                    })
                    .with_overlay(false),
                ));
                drawing.pane = Some(study.0);
                drawing.chart_points =
                    vec![ChartPoint::new(30.0, 50.0), ChartPoint::new(70.0, 50.0)];
            }
            chart.drawings_mut().drawings.push(drawing);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let plot = chart.last_rendered_price_rect;
            let (pressed_plot, a) = if gesture.contains("pane") {
                let pane = &chart.last_rendered_indicator_panes[0];
                let beside_line = pos2(
                    chart.index_to_x(85.0).unwrap(),
                    pane.mapping.price_to_y(50.0),
                );
                (pane.chart_rect, beside_line)
            } else {
                (plot, pos2(200.0, 350.0))
            };
            let (b, c) = if gesture == "drag across panes" {
                (plot.center(), plot.center() + vec2(100.0, 0.0))
            } else {
                (pos2(300.0, 360.0), pos2(400.0, 370.0))
            };
            let points = &chart.drawings().drawings[0].points;
            let at = match gesture {
                "click on line" | "click on pane line" => points[0].lerp(points[1], 0.5),
                "click on series" => pos2(
                    chart.index_to_x(50.0).unwrap(),
                    chart.last_price_map.price_to_y(15.0),
                ),
                _ => a,
            };
            assert!(
                pressed_plot.contains(at) && plot.contains(b) && plot.contains(c),
                "row {}: gesture outside {pressed_plot:?} or {plot:?}",
                row + 1
            );
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers,
            };
            let mut steps = vec![vec![
                Event::ModifiersChanged(modifiers),
                Event::PointerMoved(at),
            ]];
            steps.extend(match gesture {
                "hover" => vec![vec![Event::PointerMoved(b)]],
                "drag" | "drag across panes" => vec![
                    vec![button(a, true)],
                    vec![Event::PointerMoved(b)],
                    vec![Event::PointerMoved(c)],
                    vec![button(c, false)],
                ],
                _ => vec![vec![button(at, true)], vec![button(at, false)]],
            });
            let view = chart.get_time_scale_state();
            let mut painted = Vec::new();
            for (step, events) in steps.into_iter().enumerate() {
                painted = run(&ctx, &mut chart, (step + 1) as f64 / 60.0, events).0;
            }
            assert_eq!(
                (
                    chart.cursor_modes.trail.len(),
                    chart.get_time_scale_state() != view,
                    chart.drawings().selected_ids().count(),
                    chart.selected_element().is_some(),
                ),
                (marks, pans, selected, series_selected),
                "row {}: {style:?} {modifiers:?} {gesture}",
                row + 1
            );
            // The mark at the press is painted, and no pane background covers it afterwards.
            if marks > 0 {
                let background = chart.config().background_color;
                let dot = painted.iter().position(|(clip, shape)| match shape {
                    Shape::Circle(circle) => {
                        clip.contains(at) && circle.center == at && circle.radius == 5.0
                    }
                    _ => false,
                });
                let covered = dot.is_some_and(|dot| {
                    painted[dot + 1..].iter().any(|(clip, shape)| match shape {
                        Shape::Rect(rect) => {
                            clip.contains(at) && rect.fill == background && rect.rect.contains(at)
                        }
                        _ => false,
                    })
                });
                assert!(
                    dot.is_some() && !covered,
                    "row {}: no visible mark at {at:?}",
                    row + 1
                );
            }
        }
    }
}
