//! Original vector pointer painted over the demo after it handles the frame.

use super::DemoApp;
use egui::{Color32, CursorIcon, Order, Shape, Stroke, pos2, vec2};
use std::ops::{Deref, DerefMut};

pub(super) struct CaptureApp {
    pub demo: DemoApp,
    pub show_pointer: bool,
    pub pointing_at_row: bool,
}

impl Deref for CaptureApp {
    type Target = DemoApp;
    fn deref(&self) -> &DemoApp {
        &self.demo
    }
}

impl DerefMut for CaptureApp {
    fn deref_mut(&mut self) -> &mut DemoApp {
        &mut self.demo
    }
}

impl eframe::App for CaptureApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.demo.ui(ui, frame);
        if !self.show_pointer {
            return;
        }
        let ctx = ui.ctx();
        let Some(p) = ctx.pointer_hover_pos() else {
            return;
        };
        let painter = ctx.layer_painter(egui::LayerId::new(
            Order::Tooltip,
            egui::Id::unique("capture-pointer"),
        ));
        let icon = ctx.output(|o| o.cursor_icon);
        let icon = if self.pointing_at_row
            && !matches!(icon, CursorIcon::Text | CursorIcon::VerticalText)
        {
            CursorIcon::PointingHand
        } else {
            icon
        };
        let points = match icon {
            CursorIcon::Text | CursorIcon::VerticalText => vec![
                pos2(-3.0, -7.0),
                pos2(3.0, -7.0),
                pos2(0.0, -7.0),
                pos2(0.0, 7.0),
                pos2(-3.0, 7.0),
                pos2(3.0, 7.0),
            ],
            CursorIcon::PointingHand => vec![
                pos2(0.0, 12.0),
                pos2(-4.0, 6.0),
                pos2(-4.0, 3.0),
                pos2(-1.0, 3.0),
                pos2(0.0, 5.0),
                pos2(0.0, -6.0),
                pos2(3.0, -6.0),
                pos2(3.0, 0.0),
                pos2(10.0, 1.0),
                pos2(11.0, 5.0),
                pos2(8.0, 12.0),
            ],
            _ => vec![
                pos2(0.0, 0.0),
                pos2(0.0, 17.0),
                pos2(4.0, 13.0),
                pos2(8.0, 20.0),
                pos2(11.0, 18.0),
                pos2(7.0, 11.0),
                pos2(13.0, 11.0),
            ],
        };
        let text = matches!(icon, CursorIcon::Text | CursorIcon::VerticalText);
        // The painter fills convex polygons, so split the silhouettes along their inner edges.
        let pieces: &[&[usize]] = match icon {
            CursorIcon::Text | CursorIcon::VerticalText => &[],
            CursorIcon::PointingHand => &[&[0, 1, 2, 3, 4], &[4, 5, 6, 7], &[0, 4, 7, 8, 9, 10]],
            _ => &[&[0, 1, 2], &[0, 2, 5], &[0, 5, 6], &[2, 3, 4], &[2, 4, 5]],
        };
        for (offset, width, color) in [
            (vec2(1.5, 2.0), 4.0, Color32::from_black_alpha(25)),
            (vec2(1.0, 1.5), 2.5, Color32::from_black_alpha(55)),
            (egui::Vec2::ZERO, 1.0, Color32::from_rgb(22, 26, 33)),
        ] {
            let shifted: Vec<_> = points.iter().map(|v| p + v.to_vec2() + offset).collect();
            let fill = if offset == egui::Vec2::ZERO {
                Color32::WHITE
            } else {
                color
            };
            for piece in pieces {
                painter.add(Shape::convex_polygon(
                    piece.iter().map(|&i| shifted[i]).collect(),
                    fill,
                    Stroke::NONE,
                ));
            }
            painter.add(Shape::Path(egui::epaint::PathShape {
                points: shifted,
                closed: !text,
                fill: Color32::TRANSPARENT,
                stroke: Stroke::new(
                    if text && offset == egui::Vec2::ZERO {
                        3.0
                    } else {
                        width
                    },
                    color,
                )
                .into(),
            }));
        }
        if text {
            painter.add(Shape::line(
                points.into_iter().map(|v| p + v.to_vec2()).collect(),
                Stroke::new(1.2, Color32::WHITE),
            ));
        }
    }
}
