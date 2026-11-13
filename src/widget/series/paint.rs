//! Hit testing, painting, legends and axes for secondary bar series.

use super::{PriceSeries, base_line};
use crate::chart::rendering;
use crate::chart::rendering::{RenderContext, StyleColors};
use crate::localization as l10n;
use crate::model::{
    Bar, ChartType, LastPriceAnimation, MarkerZOrder, PriceLine, PriceSource, SeriesScale,
};
use crate::scales::{PriceMap, PriceScaleMode};
use crate::theme::ThemeExt;
use crate::widget::{Chart, SeriesId};
use egui::{Pos2, Rect};

impl Chart {
    pub(in crate::widget) fn hit_price_series(
        &self,
        item: &PriceSeries,
        pos: Pos2,
        rect: Rect,
        map: PriceMap,
    ) -> Option<usize> {
        if !rect.contains(pos) {
            return None;
        }
        let main = self.state.shown();
        let x = |bar: &Bar| {
            crate::model::index_at(main, bar.time).map(|index| {
                self.state
                    .time_scale()
                    .idx_to_coord_precise(index, rect.left(), rect.width())
            })
        };
        let columns = item.options.chart_type == ChartType::Columns;
        let at = if columns {
            item.bars
                .partition_point(|bar| x(bar).is_some_and(|px| px < pos.x))
        } else {
            let logical =
                self.state
                    .time_scale()
                    .coord_to_idx_precise(pos.x, rect.left(), rect.width());
            let time = crate::model::time_at(main, logical)?;
            item.bars.partition_point(|bar| bar.time < time)
        };
        let tolerance = item.options.style.hit_tolerance;
        let slot = self.state.time_scale().bar_spacing() * self.series_slot_gap(&item.bars) as f32;
        let column_half_width = slot * self.config.candle_width / 2.0;
        let near = at.saturating_sub(1)..(at + 2).min(item.bars.len());
        let mut range = near.clone();
        if columns {
            // Padding can reach beyond the neighbouring timestamps.
            range.start = item.bars.partition_point(|bar| {
                x(bar).is_some_and(|px| px + column_half_width + tolerance < pos.x)
            });
            range.end = item.bars.partition_point(|bar| {
                x(bar).is_some_and(|px| px - column_half_width - tolerance <= pos.x)
            });
        }
        for index in near
            .clone()
            .chain((range.start..near.start).rev())
            .chain(near.end..range.end)
        {
            let bar = &item.bars[index];
            if bar.is_whitespace() {
                continue;
            }
            let px = x(bar)?;
            let close = Pos2::new(px, map.price_to_y(bar.close));
            let hit = if item.options.chart_type.uses_ohlc() {
                let half_width = slot * 0.35;
                let body = Rect::from_two_pos(
                    Pos2::new(px - half_width, map.price_to_y(bar.open)),
                    Pos2::new(px + half_width, close.y),
                );
                body.expand(1.0).contains(pos)
                    || ((pos.x - px).abs() <= tolerance
                        && (map.price_to_y(bar.low).min(map.price_to_y(bar.high))
                            ..=map.price_to_y(bar.low).max(map.price_to_y(bar.high)))
                            .contains(&pos.y))
            } else if columns {
                let base = item
                    .options
                    .columns_base
                    .map_or(rect.bottom(), |base| map.price_to_y(base));
                let (top, bottom) = if item.options.columns_base.is_none() || close.y <= base {
                    (close.y.min(base - 1.0), base)
                } else {
                    (base, close.y.max(base + 1.0))
                };
                Rect::from_min_max(
                    Pos2::new(px - column_half_width, top),
                    Pos2::new(px + column_half_width, bottom),
                )
                .expand(tolerance)
                .contains(pos)
            } else if pos.distance(close) <= tolerance {
                true
            } else if let Some(next) = item.bars.get(index + 1).filter(|bar| !bar.is_whitespace()) {
                let next = Pos2::new(x(next)?, map.price_to_y(next.close));
                if item.options.chart_type == ChartType::StepLine {
                    let corner = Pos2::new(next.x, close.y);
                    crate::chart::hit_test::point_to_segment_distance(pos, close, corner).min(
                        crate::chart::hit_test::point_to_segment_distance(pos, corner, next),
                    ) <= tolerance
                } else {
                    crate::chart::hit_test::point_to_segment_distance(pos, close, next) <= tolerance
                }
            } else {
                false
            };
            if hit {
                return crate::model::index_at(main, bar.time)
                    .map(|index| index.round().max(0.0) as usize);
            }
        }
        None
    }

    /// Paints the bars, price lines, change marks and markers of `item` in `rect` with `map`.
    /// Markers placed above the series are left to `paint_marks` on the price pane; in another
    /// pane they are painted here, after the pane's studies.
    pub(in crate::widget) fn paint_series(
        &self,
        plot: &egui::Painter,
        rect: Rect,
        item: &PriceSeries,
        map: PriceMap,
    ) {
        let main = self.state.shown();
        if main.is_empty() {
            return;
        }
        if self.host_events.annotation_target.borrow().is_some()
            && plot.ctx().pointer_interact_pos().is_some_and(|pos| {
                plot.clip_rect().contains(pos)
                    && self.hit_price_series(item, pos, rect, map).is_some()
            })
        {
            self.cover_annotation_hit();
        }
        let range = self.series_range(item);
        // One neighbour at each edge keeps clipped segments continuous during scrolling.
        let start = range.start.saturating_sub(1);
        let end = (range.end + 1).min(item.bars.len());
        let visible = &item.bars[start..end];
        let options = &item.options;
        let colors = StyleColors {
            bullish: options.up_color,
            bearish: options.down_color,
            text: self.config.text_color,
            bullish_border: None,
            bearish_border: None,
            bullish_wick: None,
            bearish_wick: None,
            candle_border_width: options.candle_border_width,
        };
        let context = RenderContext::new(plot, rect);
        let clock = self.bar_clock();
        let type_params = options.type_params();
        // The renderers place the series' bars by index; each index maps through its bar's
        // time onto the main time axis.
        let x = |index: usize, _| {
            item.bars
                .get(index)
                .and_then(|bar| crate::model::index_at(main, bar.time))
                .map_or(f32::NAN, |at| {
                    self.state
                        .time_scale()
                        .idx_to_coord_precise(at, rect.left(), rect.width())
                })
        };
        rendering::render_chart_type(
            options.chart_type,
            &rendering::CandleDataContext {
                price_ctx: &context,
                volume_ctx: &context,
                price_scale: &map,
                colors: &colors,
                visible_data: visible,
                full_data: &item.bars,
                revision: item.revision,
                shows_elements: false,
                heikin_ashi: &item.heikin_ashi,
                start_idx: start,
                clock: &clock,
                tick_size: None,
                price_formatter: None,
            },
            &rendering::ChartTypeParams::new(
                rendering::BarDimensions::new(
                    self.state.time_scale().bar_spacing()
                        * self.series_slot_gap(&item.bars) as f32
                        * self.config.candle_width,
                    self.config.wick_width,
                ),
                rendering::VolumeSettings::new(
                    false,
                    visible
                        .iter()
                        .map(|bar| bar.volume)
                        .filter(|volume| volume.is_finite())
                        .max_by(f64::total_cmp)
                        .unwrap_or(1.0),
                ),
                &type_params,
                rendering::TradingColors::new(options.up_color, options.down_color),
                rendering::CoordMapping::new(rect.left()),
                PriceSource::Close,
            ),
            x,
        );
        if let Some(last) = crate::model::last_with_value(&item.bars)
            && let Some(color) = rendering::candles::line_color_at(
                options.chart_type,
                &type_params.styles,
                type_params.baseline,
                &map,
                rect,
                last.close,
                last,
            )
            && let Some(index) = crate::model::index_at(main, last.time)
        {
            paint_last_price_pulse(
                plot,
                rect,
                Pos2::new(
                    self.state
                        .time_scale()
                        .idx_to_coord_precise(index, rect.left(), rect.width()),
                    map.price_to_y(last.close),
                ),
                color,
                options.last_price_animation,
                (last.time, last.close),
                &item.last_price_pulse,
            );
        }
        let style = &item.options.style;
        let price_line = self
            .series_last_bar(item)
            .filter(|_| self.config.show_price_line && style.price_line_visible)
            .map(|bar| {
                PriceLine::new(
                    bar.close,
                    style.price_line_color.unwrap_or(item.value_color(bar)),
                )
                .with_width(style.price_line_width)
                .with_style(style.price_line_style)
                .with_axis_label(false)
            });
        let base_line = base_line(&map, style);
        for line in base_line.iter().chain(&price_line) {
            crate::chart::rendering::render_price_line(
                plot,
                rect,
                &map,
                line,
                rect.left(),
                false,
                rect.right(),
            );
        }
        for (id, line) in &item.lines {
            self.paint_annotation_line(plot, rect, &map, item.id, *id, line);
        }
        if let Some(marks) = &item.change_marks {
            marks.paint(plot, |time, close| {
                let index = crate::model::index_at(main, time)?;
                let x =
                    self.state
                        .time_scale()
                        .idx_to_coord_precise(index, rect.left(), rect.width());
                Some(Pos2::new(x, map.price_to_y(close)))
            });
        }
        if item.marker_options.z_order == MarkerZOrder::Normal {
            self.paint_series_markers(plot, rect, item, map);
        }
    }

    /// Paints `item`'s crosshair marker at the shown bar's time, using its own price map.
    pub(in crate::widget) fn paint_series_crosshair_marker(
        &self,
        plot: &egui::Painter,
        rect: Rect,
        item: &PriceSeries,
        map: PriceMap,
        index: usize,
        background: impl FnOnce(Pos2) -> egui::Color32,
    ) {
        let radius = item.options.crosshair_marker_radius;
        if !radius.is_finite() || radius <= 0.0 {
            return;
        }
        let Some(time) = self.state.shown().get(index).map(|bar| bar.time) else {
            return;
        };
        let Ok(at) = item.bars.binary_search_by_key(&time, |bar| bar.time) else {
            return;
        };
        let bar = &item.bars[at];
        let styles = item.options.type_styles();
        if let Some(color) = rendering::candles::line_color_at(
            item.options.chart_type,
            &styles,
            crate::model::BaselineLevel::PercentOfPane(50.0),
            &map,
            rect,
            bar.close,
            bar,
        ) {
            let x = self.state.time_scale().idx_to_coord_precise(
                index as f64,
                rect.left(),
                rect.width(),
            );
            let point = Pos2::new(x, map.price_to_y(bar.close));
            rendering::candles::render_crosshair_marker(
                &plot.with_clip_rect(plot.clip_rect().intersect(rect)),
                point,
                &styles,
                color,
                background(point),
            );
        }
    }

    /// Paints the markers of `item` with `map`, each at the time and price of its bar.
    pub(in crate::widget) fn paint_series_markers(
        &self,
        plot: &egui::Painter,
        rect: Rect,
        item: &PriceSeries,
        map: PriceMap,
    ) {
        let (from, to) =
            crate::widget::marks::window(self.state.shown(), &self.get_chart_mapping(), rect);
        for marker in item
            .markers
            .within(from, to)
            .into_iter()
            .map(|index| &item.markers[index])
        {
            let Some(index) = crate::model::index_at(self.state.shown(), marker.time) else {
                continue;
            };
            let at = item
                .bars
                .partition_point(|bar| bar.time <= marker.time)
                .saturating_sub(1);
            if let Some(bar) = item.bars.get(at) {
                let coords = crate::chart::coords::ChartMapping {
                    start_idx: 0,
                    numeric_positions: None,
                    right_offset: self.get_chart_mapping().right_offset
                        - self.state.time_scale().position(index),
                    price: map,
                    ..self.get_chart_mapping()
                };
                let bar = Bar {
                    time: marker.time,
                    ..bar.clone()
                };
                let hits = crate::chart::rendering::render_markers(
                    &RenderContext::new(plot, rect),
                    [(0, marker)],
                    &[bar],
                    &map,
                    &coords,
                );
                self.record_mark_hits(plot, item.id, hits);
            }
        }
    }

    /// Paints a legend row for each of `items`, the first at `top`. The values follow a pointer
    /// over `rect`. Returns the bottom of the last row, or `top` without rows, and the
    /// handle of the series whose remove control was activated.
    pub(in crate::widget) fn paint_secondary_legend<'a>(
        &self,
        ui: &egui::Ui,
        id: egui::Id,
        painter: &egui::Painter,
        rect: Rect,
        top: f32,
        items: impl Iterator<Item = &'a PriceSeries>,
    ) -> (f32, Option<SeriesId>) {
        let plot = painter.with_clip_rect(painter.clip_rect().intersect(rect));
        let font = egui::FontId::proportional(ui.active_theme().text.ohlc);
        let row_height = painter
            .layout_no_wrap("Ag".into(), font.clone(), self.config.text_color)
            .size()
            .y
            .max(20.0)
            + ui.active_theme().spacing.sm;
        let main = self.state.shown();
        let mut bottom = top;
        let mut remove = None;
        for (row, item) in items.enumerate() {
            if self.config.show_indicator_labels || self.config.show_indicator_last_val {
                let selected_time = ui
                    .input(|input| input.pointer.hover_pos())
                    .filter(|point| rect.contains(*point))
                    .and_then(|point| {
                        main.get(
                            self.state
                                .time_scale()
                                .coord_to_idx_precise(point.x, rect.left(), rect.width())
                                .round()
                                .max(0.0) as usize,
                        )
                    })
                    .map(|bar| bar.time);
                let bar = match selected_time {
                    Some(time) => item
                        .bars
                        .binary_search_by_key(&time, |bar| bar.time)
                        .ok()
                        .map(|at| &item.bars[at])
                        .filter(|bar| !bar.is_whitespace()),
                    None => crate::model::last_with_value(&item.bars),
                };
                let formatter = self.series_label_formatter(item.id, PriceScaleMode::Normal);
                let value = bar.map_or_else(
                    || "∅".into(),
                    |bar| {
                        formatter.as_ref().map_or_else(
                            || {
                                let map = self.series_map(item, rect);
                                crate::scales::group_thousands(
                                    format!("{:.*}", map.price_precision().unwrap_or(2), bar.close),
                                    map.thousands_sep(),
                                )
                            },
                            |formatter| formatter.format(bar.close),
                        )
                    },
                );
                let mut job = egui::text::LayoutJob::simple_singleline(
                    format!(
                        "{}  {}",
                        if self.config.show_indicator_labels {
                            &item.options.name
                        } else {
                            ""
                        },
                        if self.config.show_indicator_last_val {
                            &value
                        } else {
                            ""
                        }
                    ),
                    font.clone(),
                    item.options.color,
                );
                job.wrap.max_width = (rect.width() - 36.0).max(0.0);
                job.wrap.max_rows = 1;
                let galley = plot.layout_job(job);
                let label = Rect::from_min_size(
                    Pos2::new(rect.left() + 8.0, top + row as f32 * row_height),
                    galley.size(),
                );
                plot.galley(label.min, galley, item.options.color);
                let control = Rect::from_min_size(
                    Pos2::new(label.right() + 8.0, label.center().y - 10.0),
                    egui::Vec2::splat(20.0),
                )
                .intersect(rect);
                bottom = label.bottom().max(control.bottom());
                if !control.is_positive() {
                    continue;
                }
                let outline = label.union(control).intersect(rect);
                let control_id = id.with(("remove_series", item.id));
                let revealed = (ui
                    .input(|input| input.pointer.hover_pos().is_some() && !input.any_touches())
                    && ui.rect_contains_pointer(outline))
                    || ui.memory(|memory| memory.has_focus(control_id));
                let name = l10n::text(ui.ctx(), "Remove series");
                let response = ui
                    .interact(
                        control,
                        control_id,
                        if revealed {
                            egui::Sense::click()
                        } else {
                            egui::Sense::focusable_noninteractive()
                        },
                    )
                    .on_hover_text(&name);
                response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
                let revealed = revealed || response.has_focus();
                if revealed {
                    let theme = ui.active_theme();
                    plot.rect_filled(
                        control,
                        theme.radius.sm,
                        if response.hovered() {
                            theme.ui.btn_bg_hover
                        } else {
                            theme.chart.bg
                        },
                    );
                    plot.rect_stroke(
                        outline,
                        theme.radius.sm,
                        egui::Stroke::new(1.0, theme.ui.accent),
                        egui::StrokeKind::Inside,
                    );
                    #[cfg(feature = "icons")]
                    crate::icons::icons::TRASH
                        .as_image_tinted(egui::Vec2::splat(14.0), theme.ui.text)
                        .paint_at(ui, control.shrink(3.0));
                    #[cfg(not(feature = "icons"))]
                    plot.text(
                        control.center(),
                        egui::Align2::CENTER_CENTER,
                        "×",
                        egui::FontId::proportional(14.0),
                        theme.ui.text,
                    );
                }
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                // A tap that never revealed the button must not delete a series unseen.
                if revealed && response.clicked() {
                    remove = Some(item.id);
                }
            }
        }
        (bottom, remove)
    }

    pub(in crate::widget) fn secondary_axes(
        &self,
        widget: Rect,
        rect: Rect,
    ) -> Vec<(SeriesScale, Rect)> {
        let keys = self.series_axis_keys();
        if keys.is_empty() {
            return Vec::new();
        }
        self.pane_scale_axes(self.main_series_pane(), widget, rect)
            .filter_map(|(key, axis)| match key {
                crate::widget::PaneScale::Named(key) if keys.contains(&key) => Some((key, axis)),
                _ => None,
            })
            .collect()
    }

    pub(in crate::widget) fn handle_secondary_axes(
        &mut self,
        ui: &egui::Ui,
        widget: Rect,
        rect: Rect,
    ) {
        let interaction = self.chart_options.handle_scale;
        for (key, axis) in self.secondary_axes(widget, rect) {
            let response = ui.interact(
                axis,
                ui.scope_id().with(("series_axis", key)),
                egui::Sense::drag(),
            );
            let scale = self.pane_price_scale_mut(self.main_series_pane(), key);
            if response.double_clicked()
                && interaction.enabled
                && interaction.axis_double_click_reset.price
            {
                scale.reset();
            } else if response.dragged()
                && interaction.enabled
                && interaction.axis_pressed_mouse_move.price
            {
                let delta = ui.input(|input| input.pointer.delta().y);
                let range = scale.range();
                let anchor = (range.min + range.max) * 0.5;
                scale.zoom(
                    (f64::from(delta) / f64::from(rect.height().max(1.0))).exp(),
                    anchor,
                );
            }
        }
    }

    pub(in crate::widget) fn paint_series_axes(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        widget: Rect,
        rect: Rect,
        colors: &StyleColors,
    ) -> f32 {
        let study_axis = crate::config::PriceAxisOptions {
            show_study_values: self.config.price_axis.show_study_values,
            ..self.config.price_axis
        };
        let mut measured = 0.0_f32;
        for (key, axis) in self.secondary_axes(widget, rect) {
            let position = if self.pane_scale_side(
                self.main_series_pane(),
                crate::widget::PaneScale::Named(key),
            ) == Some(crate::model::AxisSide::Right)
            {
                crate::scales::PriceScaleId::Right
            } else {
                crate::scales::PriceScaleId::Left
            };
            let clipped = painter.with_clip_rect(painter.clip_rect().intersect(axis));
            let assigned: Vec<_> = self
                .rendered_studies()
                .indicators()
                .iter()
                .zip(&self.study_ids)
                .filter(|(study, id)| {
                    study.is_visible()
                        && self.series_pane(**id) == Some(self.main_series_pane())
                        && study.scale() == crate::studies::StudyScale::Shared(key)
                })
                .map(|(study, _)| study.as_ref())
                .collect();
            let Some(item) = self
                .main_pane_series()
                .find(|item| item.options.scale == key)
            else {
                if let Some(study) = assigned.first() {
                    let coords = crate::chart::rendering::ChartMapping {
                        rect,
                        price: study.scale_mapping().unwrap_or(self.last_price_map),
                        ..self.get_chart_mapping()
                    };
                    measured = measured.max(crate::chart::rendering::render_study_axis(
                        painter,
                        &assigned,
                        &coords,
                        axis,
                        &study_axis,
                        self.percentage_formatter.as_deref(),
                    ));
                }
                continue;
            };
            let map = self.series_map(item, rect);
            let label_rect = super::axis_label_rect(rect, axis);
            let ctx = RenderContext::new(&clipped, label_rect);
            let slot = clipped.add(egui::Shape::Noop);
            let mut tags = Vec::new();
            for (value, colors, mapping, label) in self.secondary_axis_values(key, rect) {
                tags.push(crate::chart::rendering::axes::render_axis_value(
                    &clipped,
                    label_rect,
                    axis.right(),
                    mapping.price_to_y(value),
                    label,
                    colors,
                    &tags,
                    self.config.price_axis.allow_label_overlap,
                ));
            }
            if study_axis.show_study_values {
                for study in &assigned {
                    let mapping = study
                        .scale_mapping()
                        .unwrap_or(map)
                        .with_bounds(rect.top(), rect.bottom())
                        .with_thousands_sep(map.thousands_sep());
                    crate::chart::rendering::axes::render_study_tags(
                        &clipped,
                        label_rect,
                        &mapping,
                        *study,
                        axis.right(),
                        &mut tags,
                        None,
                        &study_axis,
                    );
                }
            }
            let formatter = self.series_label_formatter(item.id, map.transform().mode);
            crate::chart::rendering::render_price_labels_at_pos(
                &ctx,
                &map,
                colors,
                &self.config.price_axis,
                position,
                &tags,
                slot,
                formatter.as_deref(),
            );
            measured = measured.max(crate::chart::rendering::axes::price_axis_width_with_edges(
                &clipped,
                &map,
                &[],
                formatter.as_deref(),
                self.config
                    .price_axis
                    .edge_ticks_visible
                    .then_some(label_rect),
            ));
            ui.interact(
                axis,
                ui.scope_id().with(("series_axis", key)),
                egui::Sense::drag(),
            )
            .on_hover_text(l10n::text(ui.ctx(), &item.options.name));
        }
        measured
    }
}

/// Paint a visible pulse and request another frame only while its period is active.
pub(in crate::widget) fn paint_last_price_pulse(
    painter: &egui::Painter,
    rect: Rect,
    point: Pos2,
    color: egui::Color32,
    animation: LastPriceAnimation,
    last: (chrono::DateTime<chrono::Utc>, f64),
    state: &std::cell::Cell<Option<(chrono::DateTime<chrono::Utc>, f64, f64)>>,
) {
    if animation == LastPriceAnimation::Disabled {
        return;
    }
    let now = painter.ctx().input(|input| input.time);
    let started = match state.get() {
        Some((time, price, started)) if time == last.0 && price.total_cmp(&last.1).is_eq() => {
            started
        }
        previous => {
            // The first price seen is loaded history, not an update.
            let started = previous.map_or(f64::NEG_INFINITY, |_| now);
            state.set(Some((last.0, last.1, started)));
            started
        }
    };
    let period = rendering::candles::LAST_PRICE_PULSE_SECONDS;
    let phase = match animation {
        LastPriceAnimation::Continuous => (now / period).fract(),
        _ => (now - started) / period,
    };
    if (0.0..1.0).contains(&phase) && rect.contains(point) {
        rendering::candles::render_last_price_pulse(painter, point, color, phase as f32);
        painter.ctx().request_repaint();
    }
}

#[cfg(all(test, feature = "ui"))]
mod tests {
    use super::*;
    use crate::widget::test_frames::{chart, frame, shapes};
    use crate::widget::{ChartElementId, ChartEvent, SeriesId};
    use egui::{Event, Key, PointerButton, Shape};

    #[test]
    fn tap_on_hidden_remove_control_reaches_the_chart() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (touch_events, width) in
                [(false, 800.0), (true, 800.0), (false, 320.0), (true, 320.0)]
            {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = chart(&[100.0], &[110.0]);
                chart.set_size(Some(egui::vec2(width, 580.0)));
                let peer = chart.add_comparison("Peer", egui::Color32::GOLD);
                chart.set_data(peer, chart.bars(SeriesId::MAIN).to_vec());
                frame(&ctx, &mut chart, 0.0, vec![]);
                let label = shapes(&ctx, &mut chart, 0.1, vec![])
                    .into_iter()
                    .find_map(|shape| match shape {
                        Shape::Text(text) if text.galley.text().starts_with("Peer  ") => {
                            Some(Rect::from_min_size(text.pos, text.galley.size()))
                        }
                        _ => None,
                    })
                    .unwrap();
                for (index, pos) in [
                    chart.get_chart_mapping().rect.center(),
                    Pos2::new(label.right() + 18.0, label.center().y),
                ]
                .into_iter()
                .enumerate()
                {
                    chart.drain_events().for_each(drop);
                    for (step, pressed) in [(0.0, true), (0.01, false)] {
                        let mut events = vec![Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        }];
                        if touch_events {
                            events.push(Event::Touch {
                                device_id: egui::TouchDeviceId(1),
                                id: egui::TouchId(1),
                                phase: if pressed {
                                    egui::TouchPhase::Start
                                } else {
                                    egui::TouchPhase::End
                                },
                                pos,
                                force: None,
                            });
                        }
                        events.push(if pressed {
                            Event::PointerMoved(pos)
                        } else {
                            Event::PointerGone
                        });
                        frame(&ctx, &mut chart, 0.2 + index as f64 + step, events);
                    }
                    assert!(
                        chart.drain_events().any(|event| matches!(event,
                            ChartEvent::Click { position, button: PointerButton::Primary }
                                if position.pos == pos
                        )),
                        "tap at {pos:?} did not reach the chart (touch events: {touch_events})"
                    );
                    assert!(chart.series().any(|(id, _)| id == peer));
                }
            }
        }
    }

    /// A row removes its own handle, including after restore remaps ids; undo restores its
    /// data and legend position without disturbing the main series, studies or neighbours.
    #[test]
    fn overlay_legend_removal_keeps_other_series_and_undo_order() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (restored, keyboard) in [(false, false), (true, false), (false, true), (true, true)]
            {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = chart(&[100.0], &[110.0]);
                let data = chart.bars(SeriesId::MAIN).to_vec();
                chart.add_indicator(Box::new(crate::studies::SMA::new(2)));
                let mut ids = [0; 3].map(|_| chart.add_comparison("Peer", egui::Color32::GOLD));
                if restored {
                    let saved = serde_json::from_str(
                        &serde_json::to_string(&chart.snapshot().unwrap()).unwrap(),
                    )
                    .unwrap();
                    chart.restore(&saved).unwrap();
                    ids = ids.map(|id| chart.restored_series_id(id.0).unwrap());
                }
                for id in ids {
                    chart.set_data(id, data.clone());
                }
                frame(&ctx, &mut chart, 0.0, vec![]);
                let before = chart.snapshot().unwrap();
                let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                let labels = |painted: Vec<Shape>| {
                    painted
                        .into_iter()
                        .filter_map(|shape| match shape {
                            Shape::Text(text) if text.galley.text().starts_with("Peer  ") => {
                                Some(Rect::from_min_size(text.pos, text.galley.size()))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                };
                let rows = labels(painted);
                assert_eq!(rows.len(), 3);
                let pos = Pos2::new(rows[1].right() + 18.0, rows[1].center().y);
                chart
                    .selection
                    .select(ChartElementId::Series(ids[1]), Some(0));
                if keyboard {
                    let mut focused = false;
                    for stop in 0..30 {
                        let painted = shapes(
                            &ctx,
                            &mut chart,
                            0.2 + stop as f64 * 0.001,
                            vec![Event::Key {
                                key: Key::Tab,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: Default::default(),
                            }],
                        );
                        focused = ctx
                            .memory(|memory| memory.focused())
                            .and_then(|id| ctx.read_response(id))
                            .is_some_and(|response| response.rect.center() == pos);
                        if focused {
                            assert!(
                                painted.iter().any(|shape| matches!(shape,
                                    Shape::Rect(outline) if outline.rect.contains(pos)
                                        && outline.stroke.color == theme.ui.accent
                                )),
                                "Tab must reveal the focused control immediately"
                            );
                            break;
                        }
                    }
                    assert!(focused, "Tab must reach a hidden remove control");
                } else {
                    frame(&ctx, &mut chart, 0.2, vec![Event::PointerMoved(pos)]);
                }
                for (time, pressed) in [(0.3, true), (0.4, false)] {
                    let event = if keyboard {
                        Event::Key {
                            key: Key::Enter,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers: Default::default(),
                        }
                    } else {
                        Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        }
                    };
                    frame(&ctx, &mut chart, time, vec![event]);
                }
                assert!(
                    !chart.series().any(|(id, _)| id == ids[1]),
                    "clicked overlay remains"
                );
                assert!(chart.is_main_series_visible());
                assert_eq!(chart.bars(SeriesId::MAIN), data);
                assert_eq!(chart.selected_element(), None);
                let after = chart.snapshot().unwrap();
                let mut expected = before.series.clone().unwrap();
                expected.remove(1);
                assert_eq!(
                    serde_json::to_value(&after.series).unwrap(),
                    serde_json::to_value(&expected).unwrap()
                );
                assert_eq!(
                    serde_json::to_value(&after.studies).unwrap(),
                    serde_json::to_value(&before.studies).unwrap()
                );
                assert!(chart.undo());
                assert_eq!(chart.bars(ids[1]), data);
                assert_eq!(
                    serde_json::to_value(chart.snapshot().unwrap().series).unwrap(),
                    serde_json::to_value(&before.series).unwrap()
                );
                assert_eq!(
                    labels(shapes(&ctx, &mut chart, 0.5, vec![Event::PointerGone])),
                    rows
                );
                assert!(chart.redo());
                assert!(!chart.series().any(|(id, _)| id == ids[1]));
                assert_eq!(chart.bars(ids[0]), data);
                assert_eq!(chart.bars(ids[2]), data);
            }
        }
    }
}
