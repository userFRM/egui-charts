use crate::{localization as l10n, theme::ThemeExt, ui_kit::EmptyState};
use chrono::{DateTime, Utc};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, vec2};

/// Plot timestamps on a shared time axis. Multiply values by `scale` before plotting and labelling.
pub(super) fn show(
    ui: &mut Ui,
    values: &[(DateTime<Utc>, f64)],
    comparison: &[(DateTime<Utc>, f64)],
    color: Color32,
    height: f32,
    scale: f64,
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
    let low = all().map(|p| p.1 * scale).fold(f64::INFINITY, f64::min);
    let high = all().map(|p| p.1 * scale).fold(f64::NEG_INFINITY, f64::max);
    if !low.is_finite() || !high.is_finite() {
        EmptyState::new("No usable equity curve").show(ui);
        return;
    }
    // Divide before subtracting so opposite finite extremes cannot overflow the value range.
    let magnitude = low.abs().max(high.abs()).max(1.0);
    let lower = low / magnitude;
    let range = (high / magnitude - lower).max(f64::EPSILON);
    let flat = low == high;
    let span = (last - first).as_seconds_f64().max(f64::EPSILON);
    let point = |t: DateTime<Utc>, value: f64| {
        Pos2::new(
            plot.left() + (((t - first).as_seconds_f64() / span) as f32) * plot.width(),
            plot.bottom()
                - (if flat {
                    0.5
                } else {
                    ((value / magnitude - lower) / range) as f32
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
                    (lower + fraction * range) * magnitude
                }
            ),
            font.clone(),
            theme.ui.text_muted,
        );
    }
    let count = ((plot.width() / 140.0) as usize).clamp(2, 6);
    // A single instant gets one label with its date and time.
    let (count, format) = if first == last {
        (1, "%Y-%m-%d %H:%M")
    } else if span >= (count - 1) as f64 * 86_400.0 {
        (count, "%Y-%m-%d")
    } else if span >= (count - 1) as f64 * 60.0 {
        (count, "%Y-%m-%d %H:%M")
    } else {
        (count, "%H:%M:%S%.f")
    };
    for k in 0..count {
        let fraction = k as f64 / (count - 1).max(1) as f64;
        let offset =
            chrono::TimeDelta::from_std(std::time::Duration::from_secs_f64(fraction * span))
                .expect("span between equity points");
        let t = first.checked_add_signed(offset).unwrap_or(last);
        let align = if k == 0 {
            Align2::LEFT_TOP
        } else if k == count - 1 {
            Align2::RIGHT_TOP
        } else {
            Align2::CENTER_TOP
        };
        painter.text(
            Pos2::new(
                plot.left() + fraction as f32 * plot.width(),
                plot.bottom() + 5.0,
            ),
            align,
            t.format(format).to_string(),
            font.clone(),
            theme.ui.text_muted,
        );
    }
    for (curve, stroke) in [
        (values, Stroke::new(1.8, color)),
        (comparison, Stroke::new(1.5, theme.ui.warning)),
    ] {
        if curve.len() == 1 {
            painter.circle_filled(point(curve[0].0, curve[0].1 * scale), 3.0, stroke.color);
        } else if !curve.is_empty() {
            painter.add(Shape::line(
                curve.iter().map(|&(t, v)| point(t, v * scale)).collect(),
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
                    let v = v * scale;
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
