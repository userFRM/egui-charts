//! Price-row profile painting shared by studies and drawing tools.

use crate::data::{
    ProfileDisplay, ProfileLevel, ProfileLevelKind, ProfileSource, VolumeProfile,
    VolumeProfileConfig,
};
use crate::styles::{stroke, typography};
use crate::theme::ThemeExt;
use egui::{Color32, Pos2, Rect, Stroke};
use std::ops::Range;

/// Strongest opacity of a study's rows, that of its default value area colours, so that saved
/// opaque colours also leave the price bars beneath them readable.
const STUDY_ROW_ALPHA: u8 = 163;

/// Whether a study's profile measures the bars in view, not a span of its own, so that its point
/// of control crosses the plot. An options profile measures strikes.
pub(crate) fn measures_view(profile: &VolumeProfile, strip: bool) -> bool {
    strip && marks_levels(profile)
}

/// Whether a profile marks its point of control and value area. An options profile's rows
/// are strikes, where neither has a meaning.
pub(crate) fn marks_levels(profile: &VolumeProfile) -> bool {
    profile.source() != ProfileSource::Options
}

/// Study rows leave candles visible; drawings also tint their span.
pub(crate) enum ProfileStyle {
    Study {
        right_aligned: bool,
        strip: bool,
        /// Share of the span that the row with the most volume takes.
        width: f32,
        /// Visibility of the option profile's call and put wall plots.
        walls_visible: [bool; 2],
        /// Decimals of the row values, two without.
        volume_decimals: Option<u8>,
    },
    #[cfg(feature = "drawings")]
    Drawing {
        anchored: bool,
        /// Width of the point of control line, the drawing's line width.
        poc_width: f32,
    },
}

/// `color`, or the active theme's foreground for `Color32::PLACEHOLDER`, which follows the
/// theme until a colour is chosen.
fn shown_color(painter: &egui::Painter, color: [u8; 4]) -> [u8; 4] {
    if color == Color32::PLACEHOLDER.to_srgba_unmultiplied() {
        painter.ctx().active_theme().ui.text.to_srgba_unmultiplied()
    } else {
        color
    }
}

/// A strike's contract count with at most two decimals and no trailing zeros, abbreviated
/// with K, M or B from a thousand: 20, 1.5K, 1.23M.
fn contracts_text(count: f64) -> String {
    use crate::scales::PriceFormatter;
    let text = crate::scales::VolumeFormatter::default().format(count);
    let digits = text.trim_end_matches(char::is_alphabetic);
    // Two decimals always write a point, so trimming zeros never eats an integer digit.
    let number = digits.trim_end_matches('0').trim_end_matches('.');
    format!("{number}{}", &text[digits.len()..])
}

/// A level carried past its profile, a hairline over `span`: the point of control in the
/// profile's colour, the value area high and low at the opacity of its value area lines.
pub(crate) fn paint_level(
    painter: &egui::Painter,
    level: &ProfileLevel,
    span: Range<f32>,
    price_to_y: &dyn Fn(f64) -> f32,
) {
    let y = price_to_y(level.price);
    if span.start >= span.end || !y.is_finite() {
        return;
    }
    let [r, g, b, a] = shown_color(painter, level.color);
    let alpha = if level.kind == ProfileLevelKind::PointOfControl {
        a
    } else {
        200
    };
    painter.hline(
        span.start..=span.end,
        y,
        Stroke::new(
            stroke::HAIRLINE,
            Color32::from_rgba_unmultiplied(r, g, b, alpha),
        ),
    );
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
    let options = profile.source() == ProfileSource::Options;
    let (drawing, right_aligned, strip, share, poc_width, walls_visible, volume_decimals) =
        match style {
            ProfileStyle::Study {
                right_aligned,
                strip,
                width,
                walls_visible,
                volume_decimals,
            } => (
                false,
                right_aligned,
                strip,
                width,
                stroke::THICK,
                walls_visible,
                volume_decimals,
            ),
            #[cfg(feature = "drawings")]
            ProfileStyle::Drawing {
                anchored,
                poc_width,
            } => (true, anchored, false, 0.3, poc_width, [true; 2], None),
        };
    if profile.exposure.is_some() {
        paint_exposure(painter, profile, config, span.clone(), share, price_to_y);
        paint_option_walls(
            painter,
            profile,
            span,
            right_aligned,
            price_to_y,
            walls_visible,
            None,
            Vec::new(),
        );
        return;
    }
    let Some((poc, (va_low, va_high))) = profile.poc_and_value_area(config.value_area()) else {
        if !drawing {
            paint_option_walls(
                painter,
                profile,
                span,
                right_aligned,
                price_to_y,
                walls_visible,
                Some(&[]),
                Vec::new(),
            );
        }
        return;
    };
    let y_at_row = |row: usize| (price_to_y)(profile.low + row as f64 * profile.row_height);
    let color = shown_color(painter, color);
    let spans_view = !drawing && measures_view(profile, strip);
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
    // How far from the base a row's bar reaches; its figures and a wall follow it.
    let bar_width = |row: usize| {
        let amount = if config.display == ProfileDisplay::Delta {
            (profile.up[row] - profile.down[row]).abs()
        } else {
            profile.volume[row]
        };
        if config.show_rows {
            (amount / max_volume) as f32 * max_width
        } else {
            0.0
        }
    };
    let mut reach: Vec<f32> = if profile.walls.is_some() {
        (0..profile.volume.len()).map(bar_width).collect()
    } else {
        Vec::new()
    };
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
            poc_width,
            [y_at_row(va_high + 1), y_at_row(va_low)],
            config,
            drawing,
        );
    };
    // A drawing's rows lie over its lines, as they do over the bars.
    if drawing {
        draw_lines();
    }

    for (row, &volume) in profile
        .volume
        .iter()
        .enumerate()
        .filter(|_| config.show_rows)
    {
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

        // An options profile's rows are strikes, all painted alike.
        let in_value_area = !options && (va_low..=va_high).contains(&row);

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
                if let Some(parts) = &profile.expiry_parts {
                    // Each expiry's part of the row, from its base in date order.
                    let mut start = left;
                    for (part, (_, rgb)) in parts.rows.iter().zip(&parts.labels) {
                        let width = (part[row] / max_volume) as f32 * max_width;
                        paint(
                            start,
                            width,
                            // The opacity of the study's default colours.
                            Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], 180),
                        );
                        start += width;
                    }
                } else {
                    paint(
                        left,
                        width,
                        if options {
                            rgba(color)
                        } else if drawing {
                            rgba([color[0], color[1], color[2], 191])
                        } else {
                            rgba(up)
                        },
                    );
                }
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

    if config.show_values && !drawing {
        let formatter = crate::scales::VolumeFormatter {
            precision: volume_decimals.map_or(2, usize::from),
        };
        let font = egui::FontId::proportional(typography::XS);
        let text_color = painter.ctx().active_theme().ui.text;
        for (row, &volume) in profile.volume.iter().enumerate() {
            let delta = profile.up[row] - profile.down[row];
            let amount = if config.display == ProfileDisplay::Delta {
                delta.abs()
            } else {
                volume
            };
            // A strike's figures show while it has any, also where its calls and puts net out.
            if (if options { volume } else { amount }) <= 0.0 {
                continue;
            }
            let text = if options && profile.ratio_rows {
                format!("{:.2}", profile.up[row])
            } else if options {
                format!(
                    "{} × {}",
                    contracts_text(profile.up[row]),
                    contracts_text(profile.down[row])
                )
            } else {
                match config.display {
                    ProfileDisplay::Total => formatter.format_with_min_unit_decimals(volume, 2),
                    ProfileDisplay::UpDown => format!(
                        "{} / {}",
                        formatter.format_with_min_unit_decimals(profile.up[row], 2),
                        formatter.format_with_min_unit_decimals(profile.down[row], 2)
                    ),
                    ProfileDisplay::Delta => format!(
                        "{}{}",
                        if delta > 0.0 { "+" } else { "" },
                        formatter.format_with_min_unit_decimals(delta, 2)
                    ),
                }
            };
            let galley = painter.layout_no_wrap(text, font.clone(), text_color);
            let (a, b) = (y_at_row(row), y_at_row(row + 1));
            if (a - b).abs() < galley.size().y {
                continue;
            }
            let width = bar_width(row);
            let x = if right_aligned {
                row_right - width - 2.0 - galley.size().x
            } else {
                left + width + 2.0
            };
            if let Some(reach) = reach.get_mut(row) {
                *reach = width + 2.0 + galley.size().x;
            }
            painter.galley(
                Pos2::new(x, (a + b - galley.size().y) * 0.5),
                galley,
                text_color,
            );
        }
    }

    let key = match &profile.expiry_parts {
        Some(parts) => paint_expiry_key(painter, &parts.labels, rect, !right_aligned),
        None => Vec::new(),
    };
    if !drawing {
        if marks_levels(profile) {
            draw_lines();
        }
        paint_option_walls(
            painter,
            profile,
            span,
            right_aligned,
            price_to_y,
            walls_visible,
            Some(&reach),
            key,
        );
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

/// Marks the call and put walls of a profile drawn at its expiration in their colours.
/// Without `reach`, for exposure bars that grow both ways from the span's centre, a wall is a
/// line across the span with its label above the line's start. Otherwise the label alone names
/// it beside its row: centred on the strike, on the side away from the rows' base, past what
/// `reach` gives for every row its height meets (how far from the base that row's bar and
/// figures reach) and past each of `taken` at that height, so that it covers no figures, no key
/// line and not the other wall's label.
fn paint_option_walls(
    painter: &egui::Painter,
    profile: &VolumeProfile,
    span: Range<f32>,
    right_aligned: bool,
    price_to_y: &dyn Fn(f64) -> f32,
    visible: [bool; 2],
    reach: Option<&[f32]>,
    mut taken: Vec<Rect>,
) {
    let Some(walls) = &profile.walls else {
        return;
    };
    let font = egui::FontId::proportional(typography::XS);
    let y_at_row = |row: usize| price_to_y(profile.low + row as f64 * profile.row_height);
    for (side, label) in ["Call wall", "Put wall"].into_iter().enumerate() {
        let Some(strike) = walls.strikes[side].filter(|_| visible[side]) else {
            continue;
        };
        let [r, g, b, a] = walls.colors[side];
        let color = Color32::from_rgba_unmultiplied(r, g, b, a);
        let y = price_to_y(strike);
        if !y.is_finite() {
            continue;
        }
        let text = crate::localization::text(painter.ctx(), label);
        let Some(reach) = reach else {
            painter.hline(span.start..=span.end, y, Stroke::new(1.0, color));
            let (x, align) = if right_aligned {
                (span.end - 2.0, egui::Align2::RIGHT_BOTTOM)
            } else {
                (span.start + 2.0, egui::Align2::LEFT_BOTTOM)
            };
            painter.text(Pos2::new(x, y - 2.0), align, text, font.clone(), color);
            continue;
        };
        let galley = painter.layout_no_wrap(text, font.clone(), color);
        let size = galley.size();
        let band = egui::Rangef::new(y - size.y * 0.5, y + size.y * 0.5);
        let beyond = |rect: &Rect| {
            if right_aligned {
                span.end - rect.left()
            } else {
                rect.right() - span.start
            }
        };
        let far = reach
            .iter()
            .enumerate()
            .filter(|&(row, _)| {
                let (a, b) = (y_at_row(row), y_at_row(row + 1));
                band.intersects(egui::Rangef::new(a.min(b), a.max(b)))
            })
            .map(|(_, &reach)| reach)
            .chain(
                taken
                    .iter()
                    .filter(|rect| band.intersects(rect.y_range()))
                    .map(beyond),
            )
            .fold(0.0, f32::max);
        let x = if right_aligned {
            span.end - far - 4.0 - size.x
        } else {
            span.start + far + 4.0
        };
        let min = Pos2::new(x, band.min);
        taken.push(Rect::from_min_size(min, size));
        painter.galley(min, galley, color);
    }
}

// Both painting and selection use these signed rectangles, including inverted price scales.
pub(crate) fn exposure_rects(
    profile: &VolumeProfile,
    display: ProfileDisplay,
    span: Range<f32>,
    share: f32,
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
    // The largest bar takes half the share on each side of the centre: 0.45 at the default 0.9.
    let width = (span.end - span.start) * share * 0.5;
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
    share: f32,
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
    for (index, rect, down) in exposure_rects(profile, config.display, span, share, price_to_y) {
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

/// Names each expiry of a profile drawn by expiry beside a swatch of its colour, one line each
/// from the top of the part of `rect` the painter shows, while lines fit. It takes the far end
/// of the rows: the left of a right-aligned profile and, for one at the left, the room left of
/// the value area labels, clear of the chart legend. Returns the rectangles of the lines it paints.
fn paint_expiry_key(
    painter: &egui::Painter,
    labels: &[(String, [u8; 4])],
    rect: Rect,
    rows_from_left: bool,
) -> Vec<Rect> {
    if !rect.is_finite() {
        return Vec::new();
    }
    let rect = rect.intersect(painter.clip_rect());
    // As for the source note, a narrower profile has no room for text.
    if rect.width() < 60.0 {
        return Vec::new();
    }
    let text = painter.ctx().active_theme().ui.text;
    let mut top = rect.top() + 3.0;
    let mut taken = Vec::new();
    for (label, rgb) in labels {
        let galley = painter.layout_no_wrap(
            label.clone(),
            egui::FontId::proportional(typography::XS),
            text,
        );
        let size = galley.size();
        let width = size.y + 4.0 + size.x;
        if top + size.y > rect.bottom() || width + 6.0 > rect.width() {
            break;
        }
        let x = if rows_from_left {
            (rect.right() - 50.0 - width).max(rect.left() + 3.0)
        } else {
            rect.left() + 3.0
        };
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(x, top), egui::vec2(size.y, size.y)).shrink(2.0),
            0.0,
            Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
        );
        painter.galley(Pos2::new(x + size.y + 4.0, top), galley, text);
        taken.push(Rect::from_min_size(
            Pos2::new(x, top),
            egui::vec2(width, size.y),
        ));
        top += size.y + 2.0;
    }
    taken
}

fn draw_volume_profile_lines(
    painter: &egui::Painter,
    rect: &Rect,
    color: [u8; 4],
    poc_y: f32,
    poc_span: egui::Rangef,
    poc_width: f32,
    [vah_y, val_y]: [f32; 2],
    config: VolumeProfileConfig,
    drawing: bool,
) {
    let theme = painter.ctx().active_theme();
    let font = egui::FontId::proportional(theme.text.tiny);

    let poc_color = if drawing {
        theme.ui.text
    } else {
        Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3])
    };
    if config.show_poc {
        painter.add(egui::Shape::line(
            vec![
                Pos2::new(poc_span.min, poc_y),
                Pos2::new(poc_span.max, poc_y),
            ],
            Stroke::new(poc_width, poc_color),
        ));
    }

    let va_color = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 200);
    let away = if vah_y <= val_y { 8.0 } else { -8.0 };
    for (i, (label, y, label_y, shown)) in [
        (
            "VAH",
            vah_y,
            vah_y - away,
            config.show_value_area_lines || config.show_value_area_high,
        ),
        (
            "VAL",
            val_y,
            val_y + away,
            config.show_value_area_lines || config.show_value_area_low,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if !shown {
            continue;
        }
        let rgba = config.value_area_line_colors[i];
        let line_color = if rgba == Color32::PLACEHOLDER.to_srgba_unmultiplied() {
            va_color
        } else {
            Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
        };
        painter.hline(
            rect.min.x..=rect.max.x,
            y,
            Stroke::new(stroke::HAIRLINE, line_color),
        );
        if rect.width() >= 50.0 {
            painter.text(
                Pos2::new(rect.max.x - 25.0, label_y),
                egui::Align2::CENTER_CENTER,
                crate::localization::text(painter.ctx(), label),
                font.clone(),
                line_color,
            );
        }
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
                        ProfileStyle::Drawing {
                            anchored: false,
                            poc_width: crate::styles::stroke::THICK,
                        }
                    } else {
                        ProfileStyle::Study {
                            right_aligned: false,
                            strip: false,
                            walls_visible: [true; 2],
                            width: 0.9,
                            volume_decimals: None,
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
                        strip: right_aligned,
                        walls_visible: [true; 2],
                        width: if right_aligned { 1.0 } else { 0.9 },
                        volume_decimals: None,
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
