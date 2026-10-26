use crate::{localization as l10n, theme::ThemeExt, ui_kit::EmptyState};
use chrono::{DateTime, Utc};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, vec2};

/// Plot timestamps on a shared time axis, with a separate scale for each chart.
pub(super) fn show(
    ui: &mut Ui,
    values: &[(DateTime<Utc>, f64)],
    comparison: &[(DateTime<Utc>, f64)],
    color: Color32,
    height: f32,
) {
    if values.is_empty() && comparison.is_empty() {
        EmptyState::new("No usable equity curve").show(ui);
        return;
    }
    let theme = ui.active_theme();
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width().max(1.0), height), Sense::hover());
    if rect.width() < 140.0 {
        return;
    }
    let plot = Rect::from_min_max(rect.min + vec2(70.0, 8.0), rect.max - vec2(8.0, 24.0));
    let all = || values.iter().chain(comparison);
    let first = all().map(|p| p.0).min().unwrap();
    let last = all().map(|p| p.0).max().unwrap();
    let low = all().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let high = all().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    // Divide before subtracting so opposite finite extremes cannot overflow the value range.
    let scale = low.abs().max(high.abs()).max(1.0);
    let lower = low / scale;
    let range = (high / scale - lower).max(f64::EPSILON);
    let flat = low == high;
    let span = (last - first).as_seconds_f64().max(f64::EPSILON);
    let point = |t: DateTime<Utc>, value: f64| {
        Pos2::new(
            plot.left() + (((t - first).as_seconds_f64() / span) as f32) * plot.width(),
            plot.bottom()
                - (if flat {
                    0.5
                } else {
                    ((value / scale - lower) / range) as f32
                }) * plot.height(),
        )
    };
    let painter = ui.painter_at(rect);
    let font = FontId::proportional(theme.text.small);
    for i in 0..=2 {
        if flat && i != 1 {
            continue;
        }
        let fraction = i as f64 / 2.0;
        let y = plot.bottom() - fraction as f32 * plot.height();
        painter.hline(
            plot.x_range(),
            y,
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
        painter.text(
            Pos2::new(plot.left() - 8.0, y),
            Align2::RIGHT_CENTER,
            format!(
                "{:.2}",
                if flat {
                    low
                } else {
                    (lower + fraction * range) * scale
                }
            ),
            font.clone(),
            theme.ui.text_muted,
        );
    }
    for (t, align) in [(first, Align2::LEFT_TOP), (last, Align2::RIGHT_TOP)] {
        painter.text(
            Pos2::new(
                if align == Align2::LEFT_TOP {
                    plot.left()
                } else {
                    plot.right()
                },
                plot.bottom() + 5.0,
            ),
            align,
            t.format("%Y-%m-%d %H:%M").to_string(),
            font.clone(),
            theme.ui.text_muted,
        );
    }
    for (curve, stroke) in [
        (values, Stroke::new(1.8, color)),
        (comparison, Stroke::new(1.5, theme.ui.warning)),
    ] {
        if curve.len() == 1 {
            painter.circle_filled(point(curve[0].0, curve[0].1), 3.0, stroke.color);
        } else if !curve.is_empty() {
            painter.add(Shape::line(
                curve.iter().map(|&(t, v)| point(t, v)).collect(),
                stroke,
            ));
        }
    }
    if let Some(pointer) = response.hover_pos().filter(|p| plot.contains(*p)) {
        let fraction = ((pointer.x - plot.left()) / plot.width()) as f64;
        let offset =
            chrono::TimeDelta::from_std(std::time::Duration::from_secs_f64(fraction * span))
                .expect("span between equity points");
        let at = first.checked_add_signed(offset).unwrap_or(last);
        let nearest = |curve: &[(DateTime<Utc>, f64)]| {
            let i = curve.partition_point(|p| p.0 < at);
            [i.checked_sub(1).and_then(|i| curve.get(i)), curve.get(i)]
                .into_iter()
                .flatten()
                .min_by_key(|p| (p.0 - at).abs())
                .copied()
        };
        painter.vline(
            pointer.x,
            plot.y_range(),
            Stroke::new(1.0, theme.ui.text_muted),
        );
        response.on_hover_ui(|ui| {
            for (label, curve) in [("Value", values), ("Buy-and-hold", comparison)] {
                if let Some((t, v)) = nearest(curve) {
                    ui.label(format!(
                        "{}: {v:.2} · {} UTC",
                        l10n::text(ui.ctx(), label),
                        t.format("%Y-%m-%d %H:%M:%S%.f")
                    ));
                }
            }
        });
    }
}
