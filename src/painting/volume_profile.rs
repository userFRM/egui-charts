//! Price-row profile painting shared by studies and drawing tools.

use crate::data::{ProfileDisplay, ProfileSource, VolumeProfile, VolumeProfileConfig};
use crate::styles::{stroke, typography};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};
use std::ops::Range;

/// Studies show their rows alone; drawings tint their span and leave the measured candles visible.
pub(crate) enum ProfileStyle {
    Study {
        right_aligned: bool,
    },
    #[cfg(feature = "drawings")]
    Drawing {
        anchored: bool,
    },
}

pub(crate) fn paint_profile(
    painter: &egui::Painter,
    profile: &VolumeProfile,
    config: VolumeProfileConfig,
    color: [u8; 4],
    span: Range<f32>,
    style: ProfileStyle,
    price_to_y: &dyn Fn(f64) -> f32,
) {
    let (left, right) = (span.start, span.end);
    if left >= right {
        return;
    }
    let Some((poc, (va_low, va_high))) = profile.poc_and_value_area(config.value_area()) else {
        return;
    };
    let y_at_row = |row: usize| (price_to_y)(profile.low + row as f64 * profile.row_height);
    let (drawing, right_aligned) = match style {
        ProfileStyle::Study { right_aligned } => (false, right_aligned),
        #[cfg(feature = "drawings")]
        ProfileStyle::Drawing { anchored } => (true, anchored),
    };
    let rect = Rect::from_two_pos(
        Pos2::new(left, y_at_row(profile.volume.len())),
        Pos2::new(right, y_at_row(0)),
    );
    let mut row_right = right;
    let mut width = right - left;
    if drawing {
        painter.rect_filled(
            rect,
            0.0,
            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 15),
        );
        if right_aligned {
            row_right = painter.clip_rect().right();
            // A distant anchor must not let the rows cover the entire pane.
            width = width.min(painter.clip_rect().width());
        }
    }
    let max_width = width * if drawing { 1.0 / 3.0 } else { 0.9 };
    let max_volume = profile.volume[poc];

    for (row, &volume) in profile.volume.iter().enumerate() {
        // Ordered on screen: the upper row edge is lower down on an inverted scale.
        let (upper, lower) = (y_at_row(row + 1), y_at_row(row));
        let (top, bottom) = (upper.min(lower), upper.max(lower));
        let gap = ((bottom - top) * 0.5).min(if drawing { 1.0 } else { 2.0 });
        let width = (volume / max_volume) as f32 * max_width;

        let in_value_area = (va_low..=va_high).contains(&row);
        // Color coding: POC is bright, Value Area is medium, outside is dim
        let bar_color = if row == poc {
            DESIGN_TOKENS.semantic.extended.favorite_gold
        } else if in_value_area {
            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 180)
        } else {
            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 80)
        };

        let paint = |start, width, color| {
            let start = if right_aligned {
                row_right - (start - left) - width
            } else {
                start
            };
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(start, top),
                    Pos2::new(start + width, bottom - gap),
                ),
                0.0,
                color,
            );
        };
        let rgba = |c: [u8; 4]| {
            let alpha = if drawing && !in_value_area {
                c[3] / 2
            } else {
                c[3]
            };
            Color32::from_rgba_unmultiplied(c[0], c[1], c[2], alpha)
        };
        let [up, down] = if in_value_area {
            config
                .value_area_colors
                .unwrap_or([config.up_color, config.down_color])
        } else {
            [config.up_color, config.down_color]
        };
        match config.display {
            ProfileDisplay::Total => paint(left, width, bar_color),
            ProfileDisplay::UpDown => {
                let up_width = (profile.up[row] / max_volume) as f32 * max_width;
                let down_width = (profile.down[row] / max_volume) as f32 * max_width;
                paint(left, up_width, rgba(up));
                paint(left + up_width, down_width, rgba(down));
                // Volume of unknown side, which only executions can leave over.
                let unknown = width - up_width - down_width;
                if unknown > 0.01 {
                    paint(
                        left + up_width + down_width,
                        unknown,
                        if drawing {
                            rgba([color[0], color[1], color[2], 180])
                        } else {
                            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 80)
                        },
                    );
                }
            }
            ProfileDisplay::Delta => {
                let delta = profile.up[row] - profile.down[row];
                let color = if delta >= 0.0 { up } else { down };
                paint(
                    left,
                    (delta.abs() / max_volume) as f32 * max_width,
                    rgba(color),
                );
            }
        }
    }

    let poc_y = (y_at_row(poc) + y_at_row(poc + 1)) / 2.0;
    let line_rect = Rect::from_min_max(rect.min, Pos2::new(row_right, rect.max.y));
    draw_volume_profile_lines(
        painter,
        &line_rect,
        color,
        poc_y,
        [y_at_row(va_high + 1), y_at_row(va_low)],
        config.show_value_area_lines,
        drawing,
    );
    // A study's span often starts at the pane's left edge, under the chart legend, so a study whose
    // rows start at the left notes its source at the right.
    let note_right = !drawing && !right_aligned;
    paint_source_note(painter, profile.source(), rect, note_right, color);
}

/// Notes at the top of `rect`, in the colour of the value area lines, a profile that does not hold
/// every execution of its span, when the note fits inside the part of `rect` the painter shows. It
/// takes the left corner of that part, or with `right` the room left of the value area labels.
fn paint_source_note(
    painter: &egui::Painter,
    source: ProfileSource,
    rect: Rect,
    right: bool,
    color: [u8; 4],
) {
    let note = match source {
        ProfileSource::Estimated => "Estimated from bars",
        ProfileSource::Executions { complete: false } => "Partial executions",
        ProfileSource::Executions { complete: true } | ProfileSource::Options => return,
    };
    if !rect.is_finite() {
        return;
    }
    // The rows of an anchored drawing stay at the pane's edge once its anchor is scrolled away.
    let rect = rect.intersect(painter.clip_rect());
    // No narrower profile has room for a note, so its text is never laid out.
    if rect.width() < 60.0 {
        return;
    }
    let color = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 200);
    let galley = painter.layout_no_wrap(
        crate::localization::text(painter.ctx(), note),
        egui::FontId::proportional(typography::XS),
        color,
    );
    let size = galley.size();
    if size.x + 6.0 <= rect.width() && size.y <= rect.height() {
        let left = rect.left() + 3.0;
        let x = if right {
            (rect.right() - 50.0 - size.x).max(left)
        } else {
            left
        };
        painter.galley(Pos2::new(x, rect.top()), galley, color);
    }
}

fn draw_volume_profile_lines(
    painter: &egui::Painter,
    rect: &Rect,
    color: [u8; 4],
    poc_y: f32,
    [vah_y, val_y]: [f32; 2],
    show_value_area_lines: bool,
    drawing: bool,
) {
    let theme = painter.ctx().active_theme();
    let font = egui::FontId::proportional(theme.text.tiny);

    let poc_color = if drawing {
        theme.ui.text
    } else {
        Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3])
    };
    painter.add(egui::Shape::line(
        vec![Pos2::new(rect.min.x, poc_y), Pos2::new(rect.max.x, poc_y)],
        Stroke::new(
            if drawing {
                stroke::THICK
            } else {
                stroke::HAIRLINE
            },
            poc_color,
        ),
    ));

    let va_color = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 200);
    for y in [vah_y, val_y].into_iter().filter(|_| show_value_area_lines) {
        painter.hline(
            rect.min.x..=rect.max.x,
            y,
            Stroke::new(stroke::HAIRLINE, va_color),
        );
    }
    if rect.width() < 50.0 || !show_value_area_lines {
        return;
    }

    // Each label sits outside the value area, away from the other line;
    // the VAH line is the lower one on an inverted scale.
    let away = if vah_y <= val_y { 8.0 } else { -8.0 };
    for (label, y) in [("VAH", vah_y - away), ("VAL", val_y + away)] {
        painter.text(
            Pos2::new(rect.max.x - 25.0, y),
            egui::Align2::CENTER_CENTER,
            label,
            font.clone(),
            va_color,
        );
    }
}

#[cfg(all(test, feature = "drawings"))]
mod tests {
    use super::{ProfileStyle, paint_profile};
    use crate::data::{ProfileDisplay, VolumeProfile, VolumeProfileConfig};
    use crate::drawings::domain::ChartPoint;
    use crate::drawings::{Drawing, DrawingToolType};
    use crate::model::{Bar, BarData};
    use crate::theme::ThemeExt;
    use chrono::{TimeZone, Utc};
    use egui::{Color32, Context, Pos2, RawInput, Rect, Shape, Stroke};

    /// A study's rows alone show its extent, 2 px apart, with its point of control a 1 px line in
    /// the profile's colour and no label. A drawing keeps 1 px gaps and its stronger line.
    #[test]
    fn studies_mark_the_point_of_control_without_a_box_or_label() {
        let bars = [Bar::new(
            Utc.timestamp_opt(0, 0).unwrap(),
            100.0,
            110.0,
            100.0,
            110.0,
            1.0,
        )];
        let mut config = VolumeProfileConfig::new(10, 0.7).unwrap();
        config.display = ProfileDisplay::UpDown;
        let profile = VolumeProfile::from_bars(&bars, config).unwrap();
        let color = [51, 74, 212, 255];
        let tone = Color32::from_rgb(51, 74, 212);
        // (a drawing, gap between rows, width of the point of control line)
        for (drawing, row_gap, poc_width) in [(false, 2.0, 1.0), (true, 1.0, 2.0)] {
            let ctx = Context::default();
            let text = ctx.active_theme().ui.text;
            let output = ctx.run_ui(RawInput::default(), |ui| {
                paint_profile(
                    ui.painter(),
                    &profile,
                    config,
                    color,
                    0.0..300.0,
                    if drawing {
                        ProfileStyle::Drawing { anchored: false }
                    } else {
                        ProfileStyle::Study {
                            right_aligned: false,
                        }
                    },
                    &|price| (price - 100.0) as f32 * 10.0,
                );
            });
            let rows = output.shapes.iter().find_map(|s| match &s.shape {
                Shape::Rect(row)
                    if row.fill
                        == Color32::from_rgba_unmultiplied(
                            config.up_color[0],
                            config.up_color[1],
                            config.up_color[2],
                            config.up_color[3],
                        ) =>
                {
                    Some(row.rect.height())
                }
                _ => None,
            });
            assert_eq!(rows, Some(10.0 - row_gap));
            assert!(output.shapes.iter().all(|s| match &s.shape {
                Shape::Rect(rect) => rect.stroke == Stroke::NONE,
                Shape::Text(label) => label.galley.text() != "POC",
                _ => true,
            }));
            let poc: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Path(line) => Some(line.stroke.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(poc.len(), 1);
            assert_eq!(poc[0].width, poc_width);
            assert_eq!(
                poc[0].color,
                egui::epaint::ColorMode::Solid(if drawing { text } else { tone })
            );
            output.drop_without_applying_deltas();
        }
    }

    /// Guards the frame cost of an anchored volume profile restored from saved drawings with its
    /// anchor 200,000 bars before the view: it paints a bounded number of shapes, not one per
    /// bar or dash from the anchor.
    #[test]
    fn far_anchored_profile_paints_a_bounded_number_of_shapes() {
        fn leaves(shape: &Shape) -> usize {
            match shape {
                Shape::Vec(shapes) => shapes.iter().map(leaves).sum(),
                _ => 1,
            }
        }
        let bars = (0..200)
            .map(|i| {
                let time = Utc.timestamp_opt(i * 60, 0).unwrap();
                Bar::new(time, 100.0, 101.0, 99.0, 100.5, 1.0)
            })
            .collect();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
        chart.set_symbol("A");
        let mut profile = Drawing::new(1, DrawingToolType::AnchoredVolumeProfile);
        profile.chart_points = vec![ChartPoint::new(-200_000.0, 100.0)];
        profile.completed = true;
        chart.drawings_mut().drawings.push(profile);
        let saved = chart.save_drawings().unwrap().to_json().unwrap();
        chart.load_drawings(&saved).unwrap();
        let ctx = Context::default();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            chart.show(ui);
        });
        let shapes: usize = output.shapes.iter().map(|s| leaves(&s.shape)).sum();
        output.drop_without_applying_deltas();
        assert!(shapes < 5_000, "{shapes} shapes in an 800 x 600 frame");
    }

    /// An anchored volume profile whose anchor bar is scrolled off the left of the pane keeps its
    /// rows at the pane's right edge, so it keeps its source note inside the pane too.
    #[test]
    fn anchored_profile_notes_its_source_in_view_with_its_anchor_off_screen() {
        let bars = (0..200)
            .map(|i| {
                let time = Utc.timestamp_opt(i * 60, 0).unwrap();
                Bar::new(time, 100.0, 101.0, 99.0, 100.5, 1.0)
            })
            .collect();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
        chart.set_symbol("A");
        let mut profile = Drawing::new(1, DrawingToolType::AnchoredVolumeProfile);
        profile.chart_points = vec![ChartPoint::new(0.0, 100.0)];
        profile.completed = true;
        chart.drawings_mut().drawings.push(profile);
        let saved = chart.save_drawings().unwrap().to_json().unwrap();
        chart.load_drawings(&saved).unwrap();
        let ctx = Context::default();
        let mut note = None;
        for _ in 0..2 {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                chart.show(ui);
            });
            note = output.shapes.iter().find_map(|s| match &s.shape {
                Shape::Text(text) if text.galley.text() == "Estimated from bars" => Some((
                    s.clip_rect,
                    Rect::from_min_size(text.pos, text.galley.size()),
                )),
                _ => None,
            });
            output.drop_without_applying_deltas();
        }
        let anchor = chart.index_to_x(0.0).unwrap();
        let (pane, note) = note.expect("source note");
        assert!(anchor < pane.left(), "anchor bar at {anchor}, pane {pane:?}");
        assert!(pane.contains_rect(note), "note at {note:?}, pane {pane:?}");
    }
}
