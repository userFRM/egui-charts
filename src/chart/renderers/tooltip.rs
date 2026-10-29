//! Tooltip Renderers
//!
//! Provides three tooltip variants:
//! - Floating: Classic tooltip that follows cursor position
//! - Tracking: Fixed horizontal bar at top of chart showing OHLC data
//! - Magnifier: Circular zoom lens for detailed data inspection

use super::context::{ChartMapping, RenderContext, StyleColors};
use crate::config::{TooltipMode, TooltipOptions};
use crate::model::{Bar, Instrument, percent_change};
use crate::scales::price_decimals;
use crate::scales::{
    DefaultPriceFormatter, HorizontalScale, PriceFormatter, PriceMap, VolumeFormatter,
    instrument_price_precision,
};
use crate::styles::typography::font_size;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{FontId, Painter, Pos2, Rect, Stroke, Vec2, epaint::StrokeKind};

/// Host number formatters shared by legends and tooltips. Missing entries retain the
/// renderer's instrument precision and built-in formatting. Prices use instrument units;
/// percentages use percentage points, so `5.0` means 5%.
#[derive(Clone, Copy, Default)]
pub struct ValueFormatters<'a> {
    /// OHLC prices and absolute changes.
    pub price: Option<&'a dyn PriceFormatter>,
    /// Volume values.
    pub volume: Option<&'a dyn PriceFormatter>,
    /// Percentage changes.
    pub percentage: Option<&'a dyn PriceFormatter>,
}

/// One visible study plot's value at the selected bar. Missing or warming-up values stay absent.
#[derive(Clone, Debug, PartialEq)]
pub struct StudyValue {
    /// Host-formatted text, when supplied by the chart. `None` uses the units and precision.
    pub formatted: Option<String>,
    /// Study title.
    pub study: String,
    /// Plot title within the study.
    pub plot: String,
    /// Finite value at the selected index, or `None` for a gap.
    pub value: Option<f64>,
    /// Number of decimal places used by the study's legend.
    pub precision: usize,
    /// Value units used by this study.
    pub format: crate::scales::StudyFormat,
    /// Plot colour.
    pub color: egui::Color32,
}

impl StudyValue {
    /// Study and plot caption, omitting a repeated single-plot title.
    pub fn label(&self) -> String {
        if self.plot.is_empty() || self.plot == self.study {
            self.study.clone()
        } else {
            format!("{}: {}", self.study, self.plot)
        }
    }

    /// Value at the study's precision, or the missing-value marker.
    pub fn formatted_value(&self) -> String {
        if let Some(formatted) = &self.formatted {
            return formatted.clone();
        }
        self.value.filter(|value| value.is_finite()).map_or_else(
            || "∅".to_owned(),
            |value| self.format.format(value, Some(self.precision)),
        )
    }
}

/// Renders tooltip based on configured mode
pub fn render_tooltip_with_options(
    context: &RenderContext,
    hover_pos: Pos2,
    candle: &Bar,
    previous_close: Option<f64>,
    options: &TooltipOptions,
    colors: &StyleColors,
    price_scale: &PriceMap,
    coords: &ChartMapping,
    visible_data: &[Bar],
    scale: &dyn HorizontalScale,
    bar_seconds: i64,
    studies: &[StudyValue],
    instrument: Option<&Instrument>,
    formatters: ValueFormatters<'_>,
) {
    match options.mode {
        TooltipMode::Floating => {
            render_floating_tooltip(
                context.painter,
                hover_pos,
                context.rect,
                candle,
                previous_close,
                options,
                colors.bar_color(candle.close >= candle.open),
                scale,
                bar_seconds,
                studies,
                instrument,
                formatters,
            );
        }
        TooltipMode::Tracking => {
            render_tracking_tooltip(
                context.painter,
                context.rect,
                candle,
                options,
                scale,
                bar_seconds,
                studies,
                instrument,
                formatters,
            );
        }
        TooltipMode::Magnifier => {
            render_magnifier_tooltip(
                context,
                hover_pos,
                candle,
                options,
                price_scale,
                coords,
                visible_data,
            );
        }
    }
}

/// Renders the floating tooltip: a label and value grid in the top corner of `price_rect`
/// on the side away from the pointer, so it never covers the hovered bar.
/// The `instrument`'s symbol heads the grid.
/// Held values use time, OHLC and total volume instead of the hover change row;
/// optional study rows follow these values.
pub fn render_floating_tooltip(
    painter: &Painter,
    hover_pos: Pos2,
    price_rect: Rect,
    candle: &Bar,
    previous_close: Option<f64>,
    options: &TooltipOptions,
    value_color: egui::Color32,
    scale: &dyn HorizontalScale,
    bar_seconds: i64,
    studies: &[StudyValue],
    instrument: Option<&Instrument>,
    formatters: ValueFormatters<'_>,
) {
    let options = &options.resolved(painter.ctx());
    let price = |price: f64| {
        let decimals = instrument.and_then(instrument_price_precision).map_or_else(
            || price_decimals(price).min(options.price_precision),
            usize::from,
        );
        formatters.price.map_or_else(
            || DefaultPriceFormatter::with_precision(decimals).format(price),
            |formatter| formatter.format(price),
        )
    };
    let mut rows = Vec::new();

    if options.show_time || options.on_long_press {
        rows.push(("Time", scale.format_crosshair(candle.time, bar_seconds)));
    }

    if options.show_ohlc {
        rows.push(("Open", price(candle.open)));
        rows.push(("High", price(candle.high)));
        rows.push(("Low", price(candle.low)));
        rows.push(("Close", price(candle.close)));
    }

    if options.show_change && !options.on_long_press {
        let reference = previous_close.unwrap_or(candle.open);
        let change = candle.close - reference;
        let change_pct = percent_change(reference, candle.close);
        let absolute = formatters.price.map_or_else(
            || {
                format!(
                    "{change:+.*}",
                    instrument
                        .and_then(instrument_price_precision)
                        .map_or(2, usize::from)
                )
            },
            |formatter| formatter.format(change),
        );
        let percent = formatters.percentage.map_or_else(
            || format!("{change_pct:+.2}%"),
            |formatter| formatter.format(change_pct),
        );
        rows.push(("Change", format!("{absolute} ({percent})")));
    }
    if options.show_volume {
        let volume = VolumeFormatter {
            precision: instrument
                .and_then(|instrument| instrument.volume_decimals)
                .map_or(2, usize::from),
        }
        .format(candle.volume);
        rows.push((
            "Vol",
            formatters
                .volume
                .map_or(volume, |formatter| formatter.format(candle.volume)),
        ));
    }

    let mut rows: Vec<_> = rows
        .into_iter()
        .map(|(label, value)| (label.to_owned(), value, value_color))
        .collect();
    rows.extend(
        studies
            .iter()
            .map(|study| (study.label(), study.formatted_value(), study.color)),
    );

    if rows.is_empty() {
        return;
    }
    if let Some(instrument) = instrument {
        rows.insert(0, (instrument.symbol.clone(), String::new(), value_color));
    }

    let theme = painter.ctx().active_theme();
    let font_id = FontId::proportional(font_size(options.font_size));
    let rows: Vec<_> = rows
        .into_iter()
        .map(|(label, value, color)| {
            (
                painter.layout_no_wrap(label, font_id.clone(), options.text_color),
                painter.layout_no_wrap(value, font_id.clone(), color),
            )
        })
        .collect();
    let label_width = rows
        .iter()
        .map(|(label, _)| label.size().x)
        .fold(0.0, f32::max);
    let value_width = rows
        .iter()
        .map(|(_, value)| value.size().x)
        .fold(0.0, f32::max);
    let row_gap = if options.on_long_press {
        6.0
    } else {
        theme.spacing.xs
    };
    let line_height = rows[0].0.size().y + row_gap;
    let padding = if options.on_long_press {
        14.0
    } else {
        theme.spacing.lg
    };
    let inset = theme.spacing.lg;
    let mut bounds = price_rect.shrink(inset);
    if hover_pos.x > price_rect.center().x {
        bounds.max.x = hover_pos.x - inset;
    } else {
        bounds.min.x = hover_pos.x + inset;
    }
    if !bounds.is_positive() {
        return;
    }
    let padding = padding.min(bounds.width() / 8.0).min(bounds.height() / 8.0);
    let gap = theme.spacing.lg.min(bounds.width() / 8.0);
    let content_width = (bounds.width() - 2.0 * padding - gap).max(0.0);
    let value_width = value_width.min(content_width * 0.65);
    let label_width = label_width.min(content_width - value_width);
    let fit = |galley: std::sync::Arc<egui::Galley>, width| {
        let mut job = (*galley.job).clone();
        job.wrap = egui::text::TextWrapping::truncate_at_width(width);
        painter.layout_job(job)
    };
    let rows: Vec<_> = rows
        .into_iter()
        .map(|(label, value)| (fit(label, label_width), fit(value, value_width)))
        .collect();
    let size = Vec2::new(
        label_width + gap + value_width + 2.0 * padding,
        (rows.len() as f32 * line_height - row_gap + 2.0 * padding).min(bounds.height()),
    );
    let x = if hover_pos.x > price_rect.center().x {
        bounds.left()
    } else {
        bounds.right() - size.x
    };
    let y = if options.on_long_press {
        (hover_pos.y - size.y / 2.0).clamp(bounds.top(), bounds.bottom() - size.y)
    } else {
        bounds.top()
    };
    let tooltip_rect = Rect::from_min_size(Pos2::new(x, y), size);
    if options.on_long_press {
        painter.add(
            egui::Shadow {
                offset: [0, 2],
                blur: 8,
                spread: 0,
                color: egui::Color32::from_black_alpha(40),
            }
            .as_shape(tooltip_rect, theme.radius.sm),
        );
    }

    let border_color = if candle.is_bullish() {
        options.border_color_bullish
    } else {
        options.border_color_bearish
    };
    painter.rect_filled(tooltip_rect, theme.radius.sm, options.background_color);
    painter.rect_stroke(
        tooltip_rect,
        theme.radius.sm,
        Stroke::new(1.0, border_color),
        StrokeKind::Inside,
    );

    // Labels left-aligned, values right-aligned. Overflow stays within the readout.
    let painter =
        painter.with_clip_rect(tooltip_rect.shrink(padding).intersect(painter.clip_rect()));
    let values_right = tooltip_rect.max.x - padding;
    for (i, (label, value)) in rows.into_iter().enumerate() {
        let y = tooltip_rect.min.y + padding + i as f32 * line_height;
        if y + label.size().y.max(value.size().y) > tooltip_rect.bottom() - padding {
            break;
        }
        painter.galley(
            Pos2::new(tooltip_rect.min.x + padding, y),
            label,
            options.text_color,
        );
        painter.galley(
            Pos2::new(values_right - value.size().x, y),
            value,
            options.text_color,
        );
    }
}

/// Renders tracking tooltip as horizontal bar at top of chart
/// Shows: SYMBOL  O: xxx  H: xxx  L: xxx  C: xxx  Vol: xxx  (+x.xx%)
pub fn render_tracking_tooltip(
    painter: &Painter,
    chart_rect: Rect,
    candle: &Bar,
    options: &TooltipOptions,
    scale: &dyn HorizontalScale,
    bar_seconds: i64,
    studies: &[StudyValue],
    instrument: Option<&Instrument>,
    formatters: ValueFormatters<'_>,
) {
    let options = &options.resolved(painter.ctx());
    let bar_height = options.tracking_bar_height;
    let bar_rect = Rect::from_min_size(chart_rect.min, Vec2::new(chart_rect.width(), bar_height));

    let painter = painter.with_clip_rect(
        bar_rect
            .intersect(chart_rect)
            .intersect(painter.clip_rect()),
    );
    // Draw bar background
    painter.rect_filled(bar_rect, 0.0, options.tracking_bar_background);

    // Build tracking text
    let precision = instrument
        .and_then(instrument_price_precision)
        .map_or(options.price_precision.min(4), usize::from);
    let mut parts: Vec<String> = instrument
        .map(|instrument| instrument.symbol.clone())
        .into_iter()
        .collect();

    if options.show_time {
        parts.push(scale.format_crosshair(candle.time, bar_seconds));
    }

    if options.show_ohlc {
        for (label, value) in [
            ("O", candle.open),
            ("H", candle.high),
            ("L", candle.low),
            ("C", candle.close),
        ] {
            let value = formatters.price.map_or_else(
                || format!("{value:.precision$}"),
                |formatter| formatter.format(value),
            );
            parts.push(format!("{label}: {value}"));
        }
    }

    if options.show_volume {
        let volume = formatters.volume.map_or_else(
            || {
                VolumeFormatter {
                    precision: instrument
                        .and_then(|instrument| instrument.volume_decimals)
                        .map_or(2, usize::from),
                }
                .format(candle.volume)
            },
            |formatter| formatter.format(candle.volume),
        );
        parts.push(format!("Vol: {volume}"));
    }

    parts.extend(
        studies
            .iter()
            .map(|study| format!("{}: {}", study.label(), study.formatted_value())),
    );

    if options.show_change {
        let change_pct = percent_change(candle.open, candle.close);
        let sign = if change_pct >= 0.0 { "+" } else { "" };
        let chart = painter.ctx().active_theme().chart;
        let change_color = if change_pct >= 0.0 {
            chart.candle_up
        } else {
            chart.candle_down
        };

        // Draw change separately with color
        let main_text = parts.join("   ");
        let change_text = formatters.percentage.map_or_else(
            || format!("({sign}{change_pct:.2}%)"),
            |formatter| format!("({})", formatter.format(change_pct)),
        );

        let font_id = FontId::proportional(font_size(options.font_size));
        let text_y = bar_rect.center().y;

        // Draw main text
        let main_rect = painter.text(
            Pos2::new(bar_rect.min.x + 10.0, text_y),
            egui::Align2::LEFT_CENTER,
            &main_text,
            font_id.clone(),
            options.text_color,
        );

        // Draw change text with color
        painter.text(
            Pos2::new(main_rect.max.x + 15.0, text_y),
            egui::Align2::LEFT_CENTER,
            &change_text,
            font_id,
            change_color,
        );

        return;
    }

    // If no change display, just show combined text
    let text = parts.join("   ");
    let font_id = FontId::proportional(font_size(options.font_size));

    painter.text(
        Pos2::new(bar_rect.min.x + 10.0, bar_rect.center().y),
        egui::Align2::LEFT_CENTER,
        &text,
        font_id,
        options.text_color,
    );
}

/// Renders magnifier tooltip as circular zoom lens
/// Shows enlarged view of candles around cursor position
pub fn render_magnifier_tooltip(
    context: &RenderContext,
    hover_pos: Pos2,
    _candle: &Bar,
    options: &TooltipOptions,
    price_scale: &PriceMap,
    coords: &ChartMapping,
    visible_data: &[Bar],
) {
    let chart = context.painter.ctx().active_theme().chart;
    // A zoom or size of 0, NaN or infinity would make the lens span unboundedly many bars or none.
    let zoom = if options.magnifier_zoom.is_nan() {
        0.1
    } else {
        options.magnifier_zoom.clamp(0.1, 100.0)
    };
    let size = options
        .magnifier_size
        .max(0.0)
        .min(context.rect.width().min(context.rect.height()));
    let radius = size / 2.0;

    // Pos magnifier (centered on cursor, but kept in bounds)
    let mut center = hover_pos;
    if center.x - radius < context.rect.min.x {
        center.x = context.rect.min.x + radius;
    }
    if center.x + radius > context.rect.max.x {
        center.x = context.rect.max.x - radius;
    }
    if center.y - radius < context.rect.min.y {
        center.y = context.rect.min.y + radius;
    }
    if center.y + radius > context.rect.max.y {
        center.y = context.rect.max.y - radius;
    }

    // Draw magnifier background (dark circle)
    context
        .painter
        .circle_filled(center, radius, chart.bg_tooltip);

    // Draw magnifier border
    context.painter.circle_stroke(
        center,
        radius,
        Stroke::new(DESIGN_TOKENS.stroke.thick, chart.axis_text_secondary),
    );

    // Calculate visible range in the magnifier
    // The magnifier shows a zoomed view centered on the hover position
    let src_width = size / zoom;

    // Enlarges the chart about the cursor, so prices land where the chart's own scale puts them.
    let price_to_mag_y =
        |price: f64| center.y + (price_scale.price_to_y(price) - hover_pos.y) * zoom;

    // Calculate bar indices visible in magnifier. Degenerate bar spacing maps
    // every pixel onto the same bar rather than dividing by zero.
    let bars_per_pixel = if coords.bar_spacing.abs() < f32::EPSILON {
        0.0
    } else {
        1.0 / coords.bar_spacing
    };
    let center_bars_from_right =
        f64::from((context.rect.max.x - hover_pos.x) * bars_per_pixel) - 0.5 - coords.right_offset;
    let src_bars = (src_width * bars_per_pixel) as isize;
    let half_bars = src_bars / 2;

    // Render candles within the magnifier clip region
    // Use clip rect to constrain drawing to the circle
    let clip_rect = Rect::from_center_size(center, Vec2::splat(size));

    // Draw zoomed candles
    let zoomed_bar_spacing = coords.bar_spacing * zoom;
    let candle_width = zoomed_bar_spacing * 0.7;

    // Only bars of `visible_data` can be drawn, so the loop never visits more than that many.
    let last = (coords.base_idx as f64 - center_bars_from_right - coords.start_idx as f64).floor()
        as isize;
    let first = last
        .saturating_add(1)
        .saturating_sub(visible_data.len() as isize);
    for i in (-half_bars).max(first)..=half_bars.min(last) {
        let bar_offset = center_bars_from_right + i as f64;
        let global_idx = (coords.base_idx as f64 - bar_offset).floor() as isize;

        if global_idx < coords.start_idx as isize {
            continue;
        }
        let local_idx = (global_idx as usize).saturating_sub(coords.start_idx);
        if local_idx >= visible_data.len() {
            continue;
        }

        let bar = &visible_data[local_idx];

        // Calculate position within magnifier
        let bar_x = center.x - (i as f32 * zoomed_bar_spacing);

        // Skip if outside magnifier bounds
        if bar_x < clip_rect.min.x - candle_width || bar_x > clip_rect.max.x + candle_width {
            continue;
        }

        let open_y = price_to_mag_y(bar.open);
        let close_y = price_to_mag_y(bar.close);
        let high_y = price_to_mag_y(bar.high);
        let low_y = price_to_mag_y(bar.low);

        let is_bullish = bar.close >= bar.open;
        let candle_color = if is_bullish {
            chart.candle_up
        } else {
            chart.candle_down
        };

        // Draw wick
        context.painter.line_segment(
            [Pos2::new(bar_x, high_y), Pos2::new(bar_x, low_y)],
            Stroke::new(DESIGN_TOKENS.stroke.hairline, candle_color),
        );

        // Draw body
        let body_top = open_y.min(close_y);
        let body_bottom = open_y.max(close_y);
        let body_height = (body_bottom - body_top).max(1.0);

        let body_rect = Rect::from_min_size(
            Pos2::new(bar_x - candle_width / 2.0, body_top),
            Vec2::new(candle_width, body_height),
        );

        if is_bullish {
            context.painter.rect_stroke(
                body_rect,
                0.0,
                Stroke::new(DESIGN_TOKENS.stroke.hairline, candle_color),
                StrokeKind::Inside,
            );
        } else {
            context.painter.rect_filled(body_rect, 0.0, candle_color);
        }
    }

    // Draw crosshair in center of magnifier
    let crosshair_color = chart.crosshair_line;
    context.painter.line_segment(
        [
            Pos2::new(center.x, center.y - radius * 0.3),
            Pos2::new(center.x, center.y + radius * 0.3),
        ],
        Stroke::new(DESIGN_TOKENS.stroke.hairline, crosshair_color),
    );
    context.painter.line_segment(
        [
            Pos2::new(center.x - radius * 0.3, center.y),
            Pos2::new(center.x + radius * 0.3, center.y),
        ],
        Stroke::new(DESIGN_TOKENS.stroke.hairline, crosshair_color),
    );

    // Draw zoom indicator
    let zoom_text = format!("{zoom:.1}x");
    context.painter.text(
        Pos2::new(center.x, center.y + radius - 12.0),
        egui::Align2::CENTER_BOTTOM,
        &zoom_text,
        FontId::proportional(context.painter.ctx().active_theme().text.tiny),
        chart.axis_text_secondary,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::{PriceRange, PriceScaleMode, PriceTransform};
    use chrono::{TimeZone, Utc};

    const HOVER: Pos2 = Pos2::new(400.0, 250.0);

    /// The floating, held and tracking readouts lead with the instrument's symbol, and the
    /// symbol follows a change of instrument.
    #[test]
    fn readouts_lead_with_the_instrument_symbol() {
        let plot = Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(800.0, 300.0));
        let bar = Bar::new(chrono::DateTime::UNIX_EPOCH, 10.0, 11.0, 9.0, 10.5, 100.0);
        let scale = crate::scales::TimeFormatterBuilder::new().build();
        for (instrument, held, tracking, expected) in [
            (Some("SAMPLE"), false, false, Some("SAMPLE")),
            (Some("OTHER"), false, false, Some("OTHER")),
            (Some("SAMPLE"), true, false, Some("SAMPLE")),
            (Some("SAMPLE"), false, true, Some("SAMPLE")),
            (None, false, false, None),
        ] {
            let instrument = instrument.map(Instrument::new);
            let ctx = egui::Context::default();
            let output = ctx.run_ui(Default::default(), |ui| {
                if tracking {
                    render_tracking_tooltip(
                        ui.painter(),
                        plot,
                        &bar,
                        &TooltipOptions::default(),
                        &scale,
                        3600,
                        &[],
                        instrument.as_ref(),
                        ValueFormatters::default(),
                    );
                } else {
                    render_floating_tooltip(
                        ui.painter(),
                        Pos2::new(plot.left() + 100.0, plot.center().y),
                        plot,
                        &bar,
                        None,
                        &TooltipOptions {
                            on_long_press: held,
                            ..Default::default()
                        },
                        egui::Color32::GREEN,
                        &scale,
                        3600,
                        &[],
                        instrument.as_ref(),
                        ValueFormatters::default(),
                    );
                }
            });
            let first = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            });
            output.drop_without_applying_deltas();
            let first = first.unwrap_or_default();
            match expected {
                Some(symbol) => assert!(first.starts_with(symbol), "{first:?}"),
                None => assert!(!first.contains("SAMPLE"), "{first:?}"),
            }
        }
    }

    #[test]
    fn study_readouts_fit_narrow_plots() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for width in [160.0, 320.0, 800.0] {
                for (fraction, held, tracking) in [
                    (0.25, false, false),
                    (0.75, false, false),
                    (0.25, true, false),
                    (0.75, true, false),
                    (0.5, false, true),
                ] {
                    let ctx = egui::Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let plot = Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(width, 300.0));
                    let pointer = Pos2::new(plot.left() + width * fraction, plot.center().y);
                    let bar = Bar::new(chrono::DateTime::UNIX_EPOCH, 10.0, 11.0, 9.0, 10.5, 100.0);
                    let studies = vec![
                        StudyValue {
                            formatted: None,
                            format: crate::scales::StudyFormat::Fixed(2),
                            study: "A very long study caption with several input parameters".into(),
                            plot: "Signal".into(),
                            value: Some(1234.567),
                            precision: 3,
                            color: egui::Color32::RED,
                        };
                        12
                    ];
                    let mut output = ctx.run_ui(Default::default(), |ui| {
                        if tracking {
                            render_tracking_tooltip(
                                ui.painter(),
                                plot,
                                &bar,
                                &TooltipOptions::default(),
                                &crate::scales::TimeFormatterBuilder::new().build(),
                                3600,
                                &studies,
                                None,
                                ValueFormatters::default(),
                            );
                            return;
                        }
                        render_floating_tooltip(
                            ui.painter(),
                            pointer,
                            plot,
                            &bar,
                            None,
                            &TooltipOptions {
                                on_long_press: held,
                                show_time: !held,
                                ..Default::default()
                            },
                            egui::Color32::GREEN,
                            &crate::scales::TimeFormatterBuilder::new().build(),
                            3600,
                            &studies,
                            None,
                            ValueFormatters::default(),
                        );
                    });
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    if held {
                        let labels: Vec<_> = shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Text(text) => Some(text.galley.text()),
                                _ => None,
                            })
                            .step_by(2)
                            .take(6)
                            .collect();
                        assert_eq!(labels, ["Time", "Open", "High", "Low", "Close", "Vol"]);
                    }
                    let mut labels = 0;
                    for shape in shapes {
                        match shape.shape {
                            egui::Shape::Text(text) => {
                                let visible = Rect::from_min_size(text.pos, text.galley.size())
                                    .intersect(shape.clip_rect);
                                if visible.is_positive() {
                                    assert!(
                                        plot.contains_rect(visible),
                                        "text escapes plot at {width}: {visible:?}"
                                    );
                                    labels += 1;
                                }
                            }
                            egui::Shape::Rect(panel) if panel.stroke.width == 1.0 => {
                                assert!(
                                    plot.contains_rect(panel.rect),
                                    "tooltip escapes plot at {width}: {:?}",
                                    panel.rect
                                );
                                assert!(
                                    !panel.rect.x_range().contains(pointer.x),
                                    "tooltip covers hovered bar"
                                );
                            }
                            _ => {}
                        }
                    }
                    assert!(labels > 0);
                }
            }
        }
    }

    /// The shapes the lens paints over 80 bars on a logarithmic scale, cursor at `HOVER`.
    fn lens_shapes(options: &TooltipOptions) -> Vec<egui::epaint::ClippedShape> {
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 500.0));
        let transform = PriceTransform {
            mode: PriceScaleMode::Logarithmic,
            base: 0.0,
        };
        let range = PriceRange::new(10f64.ln(), 10_000f64.ln());
        let map = PriceMap::new(transform, range, 0.0, 500.0, false);
        let p = map.y_to_price(HOVER.y);
        let t = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let bars = vec![Bar::new(t, p, p * 2.0, p / 2.0, p, 1.0); 80];
        let coords = ChartMapping::new(rect, 10.0, 0, 79, 0.0, map);
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            let painter = Painter::new(ui.ctx().clone(), egui::LayerId::background(), rect);
            render_magnifier_tooltip(
                &RenderContext::new(&painter, rect),
                HOVER,
                &bars[0],
                options,
                &map,
                &coords,
                &bars,
            );
        });
        let shapes = output.shapes.clone();
        output.drop_without_applying_deltas();
        shapes
    }

    /// The lens magnifies the chart about the cursor: on a logarithmic scale a wick
    /// from half to twice the price under the cursor is centred on the lens.
    #[test]
    fn magnifier_centres_the_cursor_price_on_a_log_scale() {
        let shapes = lens_shapes(&TooltipOptions::default());
        // Longer than the lens crosshair, so only a candle wick matches.
        let wick = shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::Shape::LineSegment { points, .. }
                    if points[0].x == points[1].x && (points[0].y - points[1].y).abs() > 50.0 =>
                {
                    Some(*points)
                }
                _ => None,
            })
            .expect("a wick in the lens");
        let mid = (wick[0].y + wick[1].y) / 2.0;
        assert!((mid - HOVER.y).abs() < 0.5, "wick {wick:?}");
    }

    /// A zoom of 0 or NaN neither hangs the frame nor paints non-finite geometry.
    #[test]
    fn degenerate_magnifier_zoom_draws_finite_lens() {
        for zoom in [f32::NAN, 0.0] {
            for c in lens_shapes(&TooltipOptions::magnifier_with_zoom(zoom)) {
                if let egui::Shape::LineSegment { points, .. } = &c.shape {
                    assert!(
                        points.iter().all(|p| p.is_finite()),
                        "zoom {zoom}: {points:?}"
                    );
                }
            }
        }
    }
}
