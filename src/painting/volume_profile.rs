//! Price-row profile painting shared by studies and drawing tools.

use crate::data::{ProfileDisplay, ProfileSource, VolumeProfile, VolumeProfileConfig};
use crate::styles::{stroke, typography};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Stroke};
use std::ops::Range;

/// Strongest opacity of a study's rows, that of its default value area colours, so that saved
/// opaque colours also leave the price bars beneath them readable.
const STUDY_ROW_ALPHA: u8 = 163;

/// Whether a study's profile measures the bars in view, not a span of its own, so that its rows fill
/// their strip and its point of control crosses the plot. An options profile measures strikes.
pub(crate) fn measures_view(profile: &VolumeProfile, right_aligned: bool) -> bool {
    right_aligned && profile.source() != ProfileSource::Options
}

/// Study rows leave candles visible; drawings also tint their span.
pub(crate) enum ProfileStyle {
    Study {
        right_aligned: bool,
        /// Share of the span that the row with the most volume takes.
        width: f32,
    },
    #[cfg(feature = "drawings")]
    Drawing { anchored: bool },
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
    if profile.exposure.is_some() {
        paint_exposure(painter, profile, config, span, price_to_y);
        return;
    }
    let Some((poc, (va_low, va_high))) = profile.poc_and_value_area(config.value_area()) else {
        return;
    };
    let y_at_row = |row: usize| (price_to_y)(profile.low + row as f64 * profile.row_height);
    // `Color32::PLACEHOLDER` follows the active theme until a colour is chosen.
    let color = if color == Color32::PLACEHOLDER.to_srgba_unmultiplied() {
        painter.ctx().active_theme().ui.text.to_srgba_unmultiplied()
    } else {
        color
    };
    let (drawing, right_aligned, share) = match style {
        ProfileStyle::Study {
            right_aligned,
            width,
        } => (false, right_aligned, width),
        #[cfg(feature = "drawings")]
        ProfileStyle::Drawing { anchored } => (true, anchored, 0.3),
    };
    let options = profile.source() == ProfileSource::Options;
    let spans_view = !drawing && measures_view(profile, right_aligned);
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
            // Rows at the pane's edge scale with the pane, wherever the anchor is.
            width = painter.clip_rect().width();
        }
    }
    let max_width = width * share;
    let max_volume = profile.volume[poc];
    let poc_y = (y_at_row(poc) + y_at_row(poc + 1)) / 2.0;
    let line_rect = Rect::from_min_max(rect.min, Pos2::new(row_right, rect.max.y));
    let draw_lines = || {
        draw_volume_profile_lines(
            painter,
            &line_rect,
            color,
            poc_y,
            // An anchored drawing marks the price for the whole pane, its rows being far from
            // the bars they measure.
            if spans_view || (drawing && right_aligned) {
                painter.clip_rect().x_range()
            } else {
                line_rect.x_range()
            },
            if options {
                stroke::HAIRLINE
            } else {
                stroke::THICK
            },
            [y_at_row(va_high + 1), y_at_row(va_low)],
            config.show_value_area_lines,
            drawing,
        );
    };
    // A drawing's rows lie over its lines, as they do over the bars.
    if drawing {
        draw_lines();
    }

    for (row, &volume) in profile.volume.iter().enumerate() {
        // Ordered on screen: the upper row edge is lower down on an inverted scale.
        let (upper, lower) = (y_at_row(row + 1), y_at_row(row));
        let (top, bottom) = (upper.min(lower), upper.max(lower));
        // A drawing's rows are parted by one physical pixel.
        let gap = ((bottom - top) * 0.5).min(if drawing {
            1.0 / painter.pixels_per_point()
        } else {
            1.0
        });
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

        let paint = |start, width, color: Color32| {
            let color = if options {
                color.gamma_multiply(0.35)
            } else {
                color
            };
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
                (u16::from(c[3]) * 2 / 3) as u8
            } else if drawing || options {
                c[3]
            } else {
                // Studies can sit above price bars, also with opaque colours from saved styles.
                c[3].min(STUDY_ROW_ALPHA)
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
            ProfileDisplay::Total => {
                paint(
                    left,
                    width,
                    if options {
                        bar_color
                    } else if drawing {
                        rgba([color[0], color[1], color[2], 191])
                    } else {
                        rgba(up)
                    },
                );
            }
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
                            // A neutral grey, read as neither side.
                            rgba([128, 128, 128, 191])
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

    if !drawing {
        draw_lines();
    }
    // A study's span often starts at the pane's left edge, under the chart legend, so a study whose
    // rows start at the left notes its source at the right.
    let note_right = !drawing && !right_aligned;
    // A drawing's colour tints its range: too faint for text.
    let note_color = if drawing {
        painter.ctx().active_theme().ui.text.to_srgba_unmultiplied()
    } else {
        color
    };
    paint_source_note(painter, profile.source(), rect, note_right, note_color);
}

// Both painting and selection use these signed rectangles, including inverted price scales.
pub(crate) fn exposure_rects(
    profile: &VolumeProfile,
    display: ProfileDisplay,
    span: Range<f32>,
    price_to_y: &dyn Fn(f64) -> f32,
) -> Vec<(usize, Rect, bool)> {
    let Some(snapshot) = &profile.exposure else {
        return Vec::new();
    };
    let split = display == ProfileDisplay::UpDown;
    let max = snapshot
        .rows
        .iter()
        .flat_map(|row| {
            if split {
                [row.calls, row.puts]
            } else {
                [row.net, None]
            }
        })
        .flatten()
        .map(f64::abs)
        .fold(0.0, f64::max);
    let zero = (span.start + span.end) * 0.5;
    let width = (span.end - span.start) * 0.45;
    let mut rects = Vec::new();
    for (index, row) in snapshot.rows.iter().enumerate() {
        let y = price_to_y(row.strike);
        let height = (price_to_y(row.strike - profile.row_height * 0.5)
            - price_to_y(row.strike + profile.row_height * 0.5))
        .abs()
        .clamp(2.0, 24.0);
        for (side, value) in (if split {
            [row.calls, row.puts]
        } else {
            [row.net, None]
        })
        .into_iter()
        .enumerate()
        {
            let Some(value) = value else {
                continue;
            };
            let end = zero
                + if max > 0.0 {
                    (value / max) as f32 * width
                } else {
                    0.0
                };
            let (top, bottom) = if split {
                let middle = y;
                let gap = (height * 0.25).min(1.0);
                if side == 0 {
                    (middle - height * 0.5, middle - gap)
                } else {
                    (middle + gap, middle + height * 0.5)
                }
            } else {
                (y - height * 0.5, y + height * 0.5)
            };
            rects.push((
                index,
                Rect::from_two_pos(Pos2::new(zero, top), Pos2::new(end, bottom)),
                if split { side == 1 } else { value < 0.0 },
            ));
        }
    }
    rects
}

fn paint_exposure(
    painter: &egui::Painter,
    profile: &VolumeProfile,
    config: VolumeProfileConfig,
    span: Range<f32>,
    price_to_y: &dyn Fn(f64) -> f32,
) {
    let snapshot = profile.exposure.as_ref().expect("exposure profile");
    let theme = painter.ctx().active_theme();
    let zero = (span.start + span.end) * 0.5;
    let (a, b) = (price_to_y(profile.low()), price_to_y(profile.high()));
    painter.vline(
        zero,
        a.min(b)..=a.max(b),
        Stroke::new(1.0, theme.ui.text_muted),
    );
    let point = painter.ctx().pointer_hover_pos().filter(|point| {
        painter.clip_rect().contains(*point)
            && painter.ctx().layer_id_at(*point) == Some(painter.layer_id())
    });
    let mut hovered = None;
    for (index, rect, down) in exposure_rects(profile, config.display, span, price_to_y) {
        let rgba = if down {
            config.down_color
        } else {
            config.up_color
        };
        let color = Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]);
        painter.rect_filled(rect, 0.0, color.gamma_multiply(0.35));
        if point.is_some_and(|point| rect.expand(3.0).contains(point)) {
            hovered = Some(index);
        }
    }
    if let Some(index) = hovered {
        let row = &snapshot.rows[index];
        egui::Tooltip::always_open(
            painter.ctx().clone(),
            painter.layer_id(),
            egui::Id::unique((
                "options_exposure",
                painter.layer_id(),
                painter.clip_rect().min.x.to_bits(),
                painter.clip_rect().min.y.to_bits(),
                row.strike.to_bits(),
            )),
            point.unwrap(),
        )
        .show(|ui| {
            let ctx = ui.ctx().clone();
            ui.label(crate::localization::format_text(
                &ctx,
                "Strike {0}",
                &[("0", row.strike.to_string())],
            ));
            for (label, value) in [("Calls", row.calls), ("Puts", row.puts), ("Net", row.net)] {
                ui.label(format!(
                    "{}: {}",
                    crate::localization::text(&ctx, label),
                    value.map_or_else(
                        || crate::localization::text(&ctx, "Unavailable"),
                        |value| format!("{value:+.2}")
                    )
                ));
            }
            ui.label(&snapshot.note);
        });
    }
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
    poc_span: egui::Rangef,
    poc_width: f32,
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
        vec![
            Pos2::new(poc_span.min, poc_y),
            Pos2::new(poc_span.max, poc_y),
        ],
        Stroke::new(poc_width, poc_color),
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

    /// A study's rows alone show its extent, 1 px apart, with its point of control a 2 px line in
    /// the profile's colour and no label. Study fills stay translucent in every display mode, also
    /// with saved opaque colours; a drawing keeps its colours and a line in the theme's foreground.
    #[test]
    fn studies_mark_the_point_of_control_without_a_box_or_label() {
        let bars = [
            Bar::new(
                Utc.timestamp_opt(0, 0).unwrap(),
                100.0,
                110.0,
                100.0,
                110.0,
                30.0,
            ),
            Bar::new(
                Utc.timestamp_opt(60, 0).unwrap(),
                110.0,
                110.0,
                100.0,
                100.0,
                10.0,
            ),
        ];
        let mut config = VolumeProfileConfig::new(10, 0.7).unwrap();
        config.up_color = [51, 74, 212, 89];
        config.down_color = [214, 140, 35, 89];
        config.value_area_colors = Some([[51, 74, 212, 255], [214, 140, 35, 255]]);
        let profile = VolumeProfile::from_bars(&bars, config).unwrap();
        let color = [51, 74, 212, 255];
        let tone = Color32::from_rgb(51, 74, 212);
        // Drawing, row gap, point of control line width and volume display.
        for (drawing, row_gap, poc_width, display) in [
            (false, 1.0, 2.0, ProfileDisplay::Total),
            (false, 1.0, 2.0, ProfileDisplay::UpDown),
            (false, 1.0, 2.0, ProfileDisplay::Delta),
            (true, 1.0, 2.0, ProfileDisplay::Total),
            (true, 1.0, 2.0, ProfileDisplay::UpDown),
            (true, 1.0, 2.0, ProfileDisplay::Delta),
        ] {
            config.display = display;
            let ctx = Context::default();
            let text = ctx.active_theme().ui.text;
            let mut output = ctx.run_ui(RawInput::default(), |ui| {
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
                            width: 0.9,
                        }
                    },
                    &|price| (price - 100.0) as f32 * 10.0,
                );
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let rows: Vec<_> = shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Rect(row) if (row.rect.height() - (10.0 - row_gap)).abs() < 0.01 => {
                        Some(row)
                    }
                    _ => None,
                })
                .collect();
            assert!(!rows.is_empty());
            if !drawing {
                assert!(rows.iter().all(|row| row.fill.a() <= 163), "{display:?}");
            } else if display != ProfileDisplay::Total {
                assert!(rows.iter().any(|row| row.fill.a() == 255), "{display:?}");
            }
            if display != ProfileDisplay::Total {
                // Studies limit the opaque value area; drawings only thin outside rows.
                for alpha in if drawing { [59, 255] } else { [89, 163] } {
                    assert!(
                        rows.iter().any(|row| row.fill.a() == alpha),
                        "{display:?}: {alpha}"
                    );
                }
            }
            // Each row holds 3 up and 1 down: 3/4 and 1/4 widths, or a delta of 2/4.
            let max_width = if drawing { 90.0 } else { 270.0 };
            let widths = match display {
                ProfileDisplay::Total => vec![max_width],
                ProfileDisplay::UpDown => vec![max_width * 0.75, max_width * 0.25],
                ProfileDisplay::Delta => vec![max_width * 0.5],
            };
            for (row, width) in rows.iter().zip(widths.iter().cycle()) {
                assert!((row.rect.width() - width).abs() < 0.01);
            }
            assert!(shapes.iter().all(|s| match &s.shape {
                Shape::Rect(rect) => rect.stroke == Stroke::NONE,
                Shape::Text(label) => label.galley.text() != "POC",
                _ => true,
            }));
            let poc: Vec<_> = shapes
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
        }
    }

    /// A profile of the bars in view runs its point of control across the plot, at the middle of
    /// its row, with its rows filling the strip at the right. A profile of its own span keeps the
    /// line on that span.
    #[test]
    fn point_of_control_crosses_the_plot_only_for_a_profile_of_the_bars_in_view() {
        let bars = [Bar::new(
            Utc.timestamp_opt(0, 0).unwrap(),
            100.0,
            110.0,
            100.0,
            110.0,
            10.0,
        )];
        let config = VolumeProfileConfig::new(10, 0.7).unwrap();
        let profile = VolumeProfile::from_bars(&bars, config).unwrap();
        let plot = Rect::from_min_max(Pos2::new(40.0, 0.0), Pos2::new(1040.0, 100.0));
        // Right aligned, span, the ends of the line and the longest row.
        for (right_aligned, span, line, row) in [
            (true, 740.0..1040.0, [40.0, 1040.0], 300.0),
            (false, 200.0..500.0, [200.0, 500.0], 270.0),
        ] {
            let ctx = Context::default();
            let mut output = ctx.run_ui(RawInput::default(), |ui| {
                paint_profile(
                    &ui.painter().with_clip_rect(plot),
                    &profile,
                    config,
                    [51, 74, 212, 255],
                    span.clone(),
                    ProfileStyle::Study {
                        right_aligned,
                        width: if right_aligned { 1.0 } else { 0.9 },
                    },
                    &|price| (price - 100.0) as f32 * 10.0,
                );
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let poc = shapes
                .iter()
                .find_map(|s| match &s.shape {
                    Shape::Path(line) => Some(line.points.clone()),
                    _ => None,
                })
                .expect("point of control");
            // The tallest rows are level, so the first of the middle rows takes the line.
            assert_eq!(poc.len(), 2);
            assert_eq!([poc[0].x, poc[1].x], line, "right aligned: {right_aligned}");
            assert_eq!(poc[0].y, 5.0);
            let widest = shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Rect(row) => Some(row.rect),
                    _ => None,
                })
                .max_by(|a, b| a.width().total_cmp(&b.width()))
                .expect("rows");
            assert!((widest.width() - row).abs() < 0.01, "{widest:?}");
            assert_eq!(widest.right(), if right_aligned { 1040.0 } else { 470.0 });
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
        assert!(
            anchor < pane.left(),
            "anchor bar at {anchor}, pane {pane:?}"
        );
        assert!(pane.contains_rect(note), "note at {note:?}, pane {pane:?}");
    }
}
