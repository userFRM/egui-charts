//! Hit testing, painting, legends and axes for secondary bar series.

use super::{PriceSeries, base_line};
use crate::chart::renderers::{RenderContext, StyleColors};
use crate::chart::rendering;
use crate::localization as l10n;
use crate::model::{Bar, ChartType, MarkerZOrder, PriceLine, PriceSource, SeriesScale};
use crate::scales::{PriceMap, PriceScaleMode};
use crate::theme::ThemeExt;
use crate::widget::Chart;
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
        let logical =
            self.state
                .time_scale()
                .coord_to_idx_precise(pos.x, rect.left(), rect.width());
        let time = crate::model::time_at(main, logical)?;
        let at = item.bars.partition_point(|bar| bar.time < time);
        let x = |bar: &Bar| {
            crate::model::index_at(main, bar.time).map(|index| {
                self.state
                    .time_scale()
                    .idx_to_coord_precise(index, rect.left(), rect.width())
            })
        };
        let tolerance = item.options.style.hit_tolerance;
        for index in at.saturating_sub(1)..(at + 2).min(item.bars.len()) {
            let bar = &item.bars[index];
            if bar.is_whitespace() {
                continue;
            }
            let px = x(bar)?;
            let close = Pos2::new(px, map.price_to_y(bar.close));
            let hit = if item.options.chart_type.uses_ohlc() {
                let half_width = self.state.time_scale().bar_spacing() * 0.35;
                let body = Rect::from_two_pos(
                    Pos2::new(px - half_width, map.price_to_y(bar.open)),
                    Pos2::new(px + half_width, close.y),
                );
                body.expand(1.0).contains(pos)
                    || ((pos.x - px).abs() <= tolerance
                        && (map.price_to_y(bar.low).min(map.price_to_y(bar.high))
                            ..=map.price_to_y(bar.low).max(map.price_to_y(bar.high)))
                            .contains(&pos.y))
            } else if item.options.chart_type == ChartType::Columns {
                let base = item
                    .options
                    .columns_base
                    .map_or(rect.bottom(), |base| map.price_to_y(base));
                Rect::from_two_pos(
                    Pos2::new(px - self.state.time_scale().bar_spacing() * 0.4, base),
                    Pos2::new(px + self.state.time_scale().bar_spacing() * 0.4, close.y),
                )
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
            candle_border_width: 0.0,
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
            },
            &rendering::ChartTypeParams::new(
                rendering::BarDimensions::new(
                    self.state.time_scale().bar_spacing() * self.config.candle_width,
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
        let base_line = base_line(&map).filter(|_| style.base_line_visible);
        let lines = item.lines.iter().map(|(_, line)| line);
        for line in base_line.iter().chain(&price_line).chain(lines) {
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
        if item.marker_options.z_order == MarkerZOrder::Normal || self.in_pane(item.id) {
            self.paint_series_markers(plot, rect, item, map);
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
                    right_offset: self.get_chart_mapping().right_offset - index,
                    price: map,
                    ..self.get_chart_mapping()
                };
                let bar = Bar {
                    time: marker.time,
                    ..bar.clone()
                };
                let hits = crate::chart::renderers::render_markers(
                    &RenderContext::new(plot, rect),
                    [(0, marker)],
                    &[bar],
                    &map,
                    &coords,
                );
                self.record_mark_hits(item.id, hits);
            }
        }
    }

    /// Paints a legend row for each of `items`, the first at `top`. The values follow a pointer
    /// over `rect`. Returns the bottom of the last row, or `top` without rows.
    pub(in crate::widget) fn paint_secondary_legend<'a>(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        top: f32,
        items: impl Iterator<Item = &'a PriceSeries>,
    ) -> f32 {
        let plot = painter.with_clip_rect(painter.clip_rect().intersect(rect));
        let font = egui::FontId::proportional(ui.active_theme().text.ohlc);
        let row_height = painter
            .layout_no_wrap("Ag".into(), font.clone(), self.config.text_color)
            .size()
            .y
            + ui.active_theme().spacing.sm;
        let main = self.state.shown();
        let mut bottom = top;
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
                let label = plot.text(
                    Pos2::new(rect.left() + 8.0, top + row as f32 * row_height),
                    egui::Align2::LEFT_TOP,
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
                bottom = label.bottom();
            }
        }
        bottom
    }

    pub(in crate::widget) fn secondary_axes(
        &self,
        widget: Rect,
        rect: Rect,
    ) -> Vec<(SeriesScale, Rect)> {
        let keys = self.series_axis_keys();
        let left_count = keys
            .iter()
            .filter(|key| **key != SeriesScale::Right)
            .count();
        let primary_left = if self.config.show_left_axis {
            self.config.left_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        };
        let primary_right = if self.config.show_right_axis {
            self.config.right_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        };
        let study_count = self
            .studies
            .indicators()
            .iter()
            .filter(|study| {
                (self.config.show_left_axis || self.config.show_right_axis)
                    && study.is_visible()
                    && study.is_overlay()
                    && !study.is_volume()
                    && study.scale() == crate::studies::StudyScale::Independent
            })
            .count();
        let padding = self
            .config
            .padding
            .max(0.0)
            .min(widget.width().min(widget.height()).max(0.0) / 2.0);
        let mut left = (rect.left() - primary_left).max(widget.left() + padding);
        let slot_width =
            (left - widget.left() - padding).max(0.0) / (left_count + study_count).max(1) as f32;
        keys.into_iter()
            .map(|key| {
                let axis = if key == SeriesScale::Right {
                    Rect::from_min_max(
                        Pos2::new(
                            (rect.right() + primary_right).min(widget.right()),
                            rect.top(),
                        ),
                        Pos2::new(widget.right(), rect.bottom()),
                    )
                } else {
                    let axis = Rect::from_min_max(
                        Pos2::new(left - slot_width, rect.top()),
                        Pos2::new(left, rect.bottom()),
                    );
                    left -= slot_width;
                    axis
                };
                (key, axis)
            })
            .collect()
    }

    pub(in crate::widget) fn handle_secondary_axes(
        &mut self,
        ui: &egui::Ui,
        widget: Rect,
        rect: Rect,
    ) {
        for (key, axis) in self.secondary_axes(widget, rect) {
            let response = ui.interact(
                axis,
                ui.scope_id().with(("series_axis", key)),
                egui::Sense::drag(),
            );
            let Some(scale) = self.series_scales.get_mut(&key) else {
                continue;
            };
            if response.double_clicked()
                && self
                    .chart_options
                    .handle_scale
                    .axis_double_click_reset
                    .price
            {
                scale.reset();
            } else if response.dragged()
                && self
                    .chart_options
                    .handle_scale
                    .axis_pressed_mouse_move
                    .price
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
            show_study_values: self.config.show_indicator_last_val
                && self.config.price_axis.show_study_values,
            ..self.config.price_axis
        };
        let mut measured = 0.0_f32;
        for (key, axis) in self.secondary_axes(widget, rect) {
            let position = if key == SeriesScale::Right {
                crate::scales::PriceScaleId::Right
            } else {
                crate::scales::PriceScaleId::Left
            };
            let clipped = painter.with_clip_rect(painter.clip_rect().intersect(axis));
            let assigned: Vec<_> = self
                .rendered_studies()
                .indicators()
                .iter()
                .filter(|study| {
                    study.is_visible()
                        && study.is_overlay()
                        && study.scale() == crate::studies::StudyScale::Shared(key)
                })
                .map(|study| study.as_ref())
                .collect();
            let Some(item) = self
                .main_pane_series()
                .find(|item| item.options.scale == key)
            else {
                if let Some(study) = assigned.first() {
                    let coords = crate::chart::renderers::ChartMapping {
                        rect,
                        price: study.scale_mapping().unwrap_or(self.last_price_map),
                        ..self.get_chart_mapping()
                    };
                    measured = measured.max(crate::chart::renderers::render_study_axis(
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
            let label_rect = if key == SeriesScale::Right {
                Rect::from_min_max(rect.min, axis.left_bottom())
            } else {
                Rect::from_min_max(axis.right_top(), rect.max)
            };
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
            crate::chart::renderers::render_price_labels_at_pos(
                &ctx,
                &map,
                colors,
                &self.config.price_axis,
                position,
                &tags,
                slot,
                formatter.as_deref(),
            );
            measured = measured.max(crate::chart::rendering::axes::price_axis_width(
                &clipped,
                &map,
                &[],
                formatter.as_deref(),
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
