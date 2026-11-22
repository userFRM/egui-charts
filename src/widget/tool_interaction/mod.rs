//! Tool Interaction Module
//!
//! Handles drawing tool interactions on the chart pane:
//! - Coordinate transformations between screen and chart space
//! - Active drawing tool interactions (click, drag, multi-point)
//! - Drawing selection and manipulation
//! - Cursor mode effects (Eraser)
//!
//! ## Module Structure
//! - `interaction` - Active drawing tool interaction handling
//! - `selection` - Drawing selection, manipulation, eraser

pub(super) mod cursor_modes;
mod interaction;
mod selection;
mod text;

pub(super) use selection::render_eraser_highlight as render_pane_eraser_highlight;

use crate::drawings::DrawingInteractionMode;
use crate::drawings::DrawingManager;
use crate::drawings::rendering::ChartView;
use crate::scales::PriceMap;
use crate::widget::Chart;
use crate::widget::tool_interaction::cursor_modes::CursorModeState;
use egui::{Painter, Pos2, Rect, Response, Ui};

pub(super) type LayeredDrawing = (
    egui::LayerId,
    egui::layers::ShapeIdx,
    std::ops::Range<usize>,
);

impl Chart {
    /// Queues the paint of each drawing of the pane last handled whose `series_layer` puts it
    /// under a series or study to move before the first shape of those items: `marks[k]` is
    /// where item `k` of the pane began, `None` for an item that painted nothing.
    pub(super) fn queue_layered_drawings(
        drawings: &DrawingManager,
        queued: &mut Vec<LayeredDrawing>,
        layer: egui::LayerId,
        marks: &[Option<egui::layers::ShapeIdx>],
    ) {
        if marks.is_empty() {
            return;
        }
        for drawing in drawings.drawings.iter().filter(|d| drawings.owns_pane(d)) {
            if let Some(s) = drawing.series_layer.filter(|s| *s < marks.len())
                && let Some(target) = marks[s..].iter().flatten().min_by_key(|mark| mark.0)
                && let Some(range) = drawings.painted_range(drawing.id)
            {
                queued.push((layer, *target, range));
            }
        }
    }

    /// The rectangle of `drawing`'s readout card at its last painted anchors, with the card's text
    /// and pane clip as painted, or `None` without a card or before its pane has been drawn.
    #[cfg(feature = "ui")]
    pub(crate) fn line_measurement_card_rect(
        &self,
        ui: &Ui,
        drawing: &crate::drawings::Drawing,
    ) -> Option<Rect> {
        if !drawing.tool_type.supports_line_measurements() {
            return None;
        }
        let clip = *self.drawings.pane_clips.get(&drawing.pane)?;
        let timescale = self.state.time_scale();
        // The pane holding the main series is painted without an id.
        let pane = self
            .owner_pane(drawing)
            .filter(|pane| *pane != self.main_series_pane());
        let price_member = self.pane_price_member(pane);
        drawing.line_measurement_card_rect(
            &ui.painter().with_clip_rect(clip),
            &ChartView {
                price_formatter: price_member
                    .and_then(|id| self.series_price_formatter(id))
                    .as_deref(),
                thousands_sep: self.drawing_thousands_sep(price_member),
                time_formatter: None,
                bar_seconds: 0,
                instrument: price_member.and(self.instrument()),
                bars: self.state.shown(),
                revision: self.state.revision(),
                bar_to_x: &|bar| timescale.idx_to_coord_precise(bar, clip.min.x, clip.width()),
                bar_spacing: timescale.bar_spacing(),
                // The card's text and place read the anchors' screen points, not these.
                price_to_y: &|_| 0.0,
                y_to_price: &|_| 0.0,
                time_axis: self.horizontal_scale.is_none(),
                executions: None,
                timeframe: None,
                finer: &[],
            },
        )
    }

    /// The price series whose axis `pane`'s drawings share: the main series in the main pane, else
    /// the pane's first visible price series. `None` in a pane of studies only, whose drawings keep
    /// their own units, without a series' formatter or the instrument's pips, ticks and precision.
    fn pane_price_member(&self, pane: Option<super::PaneId>) -> Option<super::SeriesId> {
        let Some(pane) = pane else {
            return Some(super::SeriesId::MAIN);
        };
        // The original price pane does not list its series as members.
        self.pane_series_iter(pane).find(|id| {
            self.price_series
                .iter()
                .any(|item| item.id == *id && item.visible)
        })
    }

    /// Groups built-in drawing prices beside a price series, as its axis does. Study-only
    /// panes keep their own units; a decimal point or digit cannot separate built-in prices.
    fn drawing_thousands_sep(&self, price_member: Option<super::SeriesId>) -> Option<char> {
        price_member
            .and(self.config.thousands_sep)
            .filter(|separator| *separator != '.' && !separator.is_ascii_digit())
    }

    /// The interval drawings measure on time-based charts with a time axis.
    pub(super) fn drawing_timeframe(&self) -> Option<crate::model::Timeframe> {
        let timeframe = self.timeframe.unwrap_or_default();
        (!timeframe.is_trade_based()
            && !self.chart_type.is_time_independent()
            && self.horizontal_scale.is_none())
        .then_some(timeframe)
    }

    /// The executions drawings read: the copy cut at candle replay's time, else the feed's
    /// held executions; `None` without a feed or while the chart holds none.
    pub(super) fn drawing_executions(&self) -> Option<crate::data::FootprintView<'_>> {
        self.feed_link.as_ref().and_then(|link| {
            if self.replay.is_some() {
                link.replay_footprint.as_ref()?.1.as_ref().map(Into::into)
            } else {
                link.footprint.as_ref().map(|flow| flow.view())
            }
        })
    }

    /// Estimated and total bars of drawing `id` over the shown bars and executions drawings read.
    #[cfg(feature = "ui")]
    pub(crate) fn vwap_estimated_bars(&self, id: usize) -> Option<(usize, usize)> {
        self.drawings
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)?
            .anchored_vwap_estimated(self.state.shown(), self.drawing_executions())
    }

    /// Suggested finer histories and first opens of the visible profile drawings' spans.
    pub(super) fn profile_histories(
        &self,
        drawings: &DrawingManager,
    ) -> Vec<(
        crate::studies::StudyDataRequest,
        chrono::DateTime<chrono::Utc>,
    )> {
        let (Some(timeframe), Some(instrument)) = (self.drawing_timeframe(), self.instrument())
        else {
            return Vec::new();
        };
        let bars = self.state.shown();
        drawings
            .drawings
            .iter()
            .filter_map(|drawing| {
                drawing.profile_history(bars, timeframe, &instrument.profile_resolutions)
            })
            .collect()
    }

    /// Handles drawing tool interaction and rendering
    ///
    /// This is the main entry point for drawing handling, coordinating:
    /// - Coordinate transformations between screen and chart space
    /// - Active drawing tool interactions (click, drag, multi-point)
    /// - Drawing selection and manipulation
    /// - Cursor mode effects (Eraser)
    ///
    /// # Arguments
    /// - `cursor_modes` - Mutable reference to cursor mode state (passed separately to avoid borrow conflicts)
    pub(crate) fn handle_drawings(
        &self,
        ui: &mut Ui,
        drawing_manager: &mut DrawingManager,
        cursor_modes: &mut CursorModeState,
        response: &Response,
        price_rect: Rect,
        price_map: &PriceMap,
        painter: &Painter,
        timescale: &crate::model::TimeScale,
        time_format: (&dyn crate::scales::TimeFormatter, i64),
    ) {
        self.handle_pane_drawings(
            ui,
            drawing_manager,
            cursor_modes,
            response,
            price_rect,
            price_map,
            painter,
            timescale,
            time_format,
            None,
        );
    }

    pub(super) fn handle_pane_drawings(
        &self,
        ui: &mut Ui,
        drawing_manager: &mut DrawingManager,
        cursor_modes: &mut CursorModeState,
        response: &Response,
        price_rect: Rect,
        price_map: &PriceMap,
        painter: &Painter,
        timescale: &crate::model::TimeScale,
        time_format: (&dyn crate::scales::TimeFormatter, i64),
        pane: Option<super::PaneId>,
    ) {
        // New drawings belong to the pane's first member, or to an empty pane itself.
        let mut owner = None;
        let members: Vec<_> = if let Some(pane) = pane {
            let members = self.pane_series(pane);
            owner = Some(members.first().map_or(pane.0, |id| id.0));
            std::iter::once(Some(pane.0))
                .chain(members.iter().map(|id| Some(id.0)))
                .collect()
        } else {
            std::iter::once(None)
                .chain(
                    (self.main_series_pane() != super::PaneId::MAIN)
                        .then_some(Some(self.main_series_pane().0)),
                )
                .chain(
                    self.pane_series(self.main_series_pane())
                        .iter()
                        // Series 0 names the original pane while that pane exists.
                        .filter(|id| {
                            **id != super::SeriesId::MAIN
                                || self.pane_index(super::PaneId::MAIN).is_none()
                        })
                        .map(|id| Some(id.0)),
                )
                .collect()
        };
        drawing_manager.set_pane(owner, members.clone(), price_rect);
        drawing_manager.interaction_modifiers = ui.input(|i| i.modifiers);
        let rect_min_x = price_rect.min.x;

        let rect_width = price_rect.width();

        // Coordinate conversion closures
        // Use idx_to_coord_precise to preserve fractional bar indices for accurate drawing positions
        // CRITICAL: Must pass the actual rect width, not timescale.width (which may differ)
        let bar_to_x = |bar_idx: f32| -> f32 {
            timescale.idx_to_coord_precise(bar_idx, rect_min_x, rect_width)
        };
        // Full precision for freeform tools and Shift edits, exact on a painted bar centre.
        let x_to_bar_precise = |x: f32| -> f32 { bar_at(timescale, x, rect_min_x, rect_width) };
        // The bar whose slot holds x.
        let x_to_nearest_bar = |x: f32| -> f32 {
            timescale
                .coord_to_idx_precise(x, rect_min_x, rect_width)
                .round() as f32
        };
        // Other tools place and move anchors on whole bars unless `snap_to_time` is off.
        let snap_to_time = drawing_manager.options.snap_to_time;
        let x_to_bar_snapped = |x: f32| -> f32 {
            if snap_to_time {
                x_to_nearest_bar(x)
            } else {
                x_to_bar_precise(x)
            }
        };
        let price_to_y = |price: f64| -> f32 { price_map.price_to_y(price) };
        // A strong snap keeps the actual OHLC even beyond a manual price range.
        let strong_magnet =
            drawing_manager.magnet_enabled() && drawing_manager.options.strong_magnet;
        let anchor_y = |y: f32| {
            if strong_magnet {
                y
            } else {
                y.clamp(price_rect.min.y, price_rect.max.y)
            }
        };
        let y_to_price = |y: f32| -> f64 { price_map.y_to_price(anchor_y(y)) };
        let translate_price = |price, from, to| price_map.translate_price(price, from, to);

        // Update all drawings' screen coords before rendering (handles pan/zoom)
        drawing_manager.update_pane_screen_coords(bar_to_x, price_to_y, timescale.bar_spacing());
        drawing_manager.anchor_pins_to_bars(x_to_bar_precise, |y| price_map.y_to_price(y));
        drawing_manager.place_anchored_text(bar_to_x, x_to_bar_precise, price_to_y, |y| {
            price_map.y_to_price(y)
        });

        // Only gather bars within the pointer's screen-space tolerance. Rebuild after pan/zoom.
        drawing_manager.snap_targets.clear();
        let press =
            if drawing_manager.active_tool.is_some() && drawing_manager.curr_drawing.is_none() {
                interaction::primary_press_pos(ui)
            } else {
                None
            };
        let snap_pointer = press
            .or_else(|| response.interact_pointer_pos())
            .or_else(|| response.hover_pos());
        if drawing_manager.magnet_enabled()
            && let Some(pointer) = snap_pointer
        {
            let distance = drawing_manager.options.magnet_distance;
            let bars = self.state.shown();
            let strong = drawing_manager.options.strong_magnet;
            let index = x_to_nearest_bar(pointer.x);
            let first = if strong {
                index.max(0.0) as usize
            } else {
                x_to_bar_precise(pointer.x - distance).floor().max(0.0) as usize
            };
            let end = if strong {
                if index < 0.0 {
                    0
                } else {
                    first.saturating_add(1)
                }
            } else {
                (x_to_bar_precise(pointer.x + distance).ceil().max(0.0) as usize).saturating_add(1)
            };
            for (i, bar) in bars.iter().enumerate().take(end).skip(first) {
                if pane.is_none() && self.main_series_visible {
                    for price in [bar.open, bar.high, bar.low, bar.close] {
                        let point = Pos2::new(bar_to_x(i as f32), price_to_y(price));
                        if price.is_finite()
                            && point.is_finite()
                            && (strong || price_rect.contains(point))
                        {
                            drawing_manager.snap_targets.push(point);
                        }
                    }
                }
            }
            if drawing_manager.options.snap_to_indicators {
                let mapping = crate::chart::rendering::ChartMapping {
                    rect: price_rect,
                    price: *price_map,
                    ..self.get_chart_mapping()
                };
                for (_, study) in self
                    .study_ids
                    .iter()
                    .zip(self.rendered_studies().indicators())
                    .filter(|(id, study)| {
                        self.shows_study(study.as_ref())
                            && members.contains(&Some(id.0))
                            // The price pane paints a volume overlay only with the main series.
                            && (pane.is_some()
                                || self.main_series_visible
                                || !study.is_volume()
                                || matches!(study.scale(), crate::studies::StudyScale::Shared(_)))
                    })
                {
                    // Use the renderer's map so independent, shared and volume scales snap
                    // at the painted point rather than interpreting study units as prices.
                    let study_map = self.pane_study_mapping(
                        pane.unwrap_or(super::PaneId::MAIN),
                        study.as_ref(),
                        &mapping,
                    );
                    for line in (0..study.line_count()).filter(|line| {
                        let plot = study.plot_style(*line);
                        plot.visible
                            && !matches!(
                                plot.plot_type,
                                crate::studies::PlotType::BarColor
                                    | crate::studies::PlotType::BackgroundColor
                            )
                    }) {
                        for i in first..end {
                            if let Some(value) = study.plot_value(line, i) {
                                let point =
                                    Pos2::new(bar_to_x(i as f32), study_map.price_to_y(value));
                                if value.is_finite()
                                    && point.is_finite()
                                    && (strong || price_rect.contains(point))
                                {
                                    drawing_manager.snap_targets.push(point);
                                }
                            }
                        }
                    }
                }
                // Without studies the chart paints the volume itself, on the map the frame keeps.
                #[cfg(not(feature = "studies"))]
                if pane.is_none()
                    && self.main_series_visible
                    && let Some(map) = self.host_events.volume_map
                {
                    for (i, bar) in bars.iter().enumerate().take(end).skip(first) {
                        let point = Pos2::new(bar_to_x(i as f32), map.price_to_y(bar.volume));
                        if bar.volume.is_finite()
                            && point.is_finite()
                            && (strong || price_rect.contains(point))
                        {
                            drawing_manager.snap_targets.push(point);
                        }
                    }
                }
            }
        }
        // The price pane's crosshair shows the height a click there gives the next anchor, or a
        // drag below gives the dragged point, whatever the crosshair mode.
        if pane.is_none() {
            let tool = drawing_manager.active_tool.filter(|tool| !tool.is_cursor());
            drawing_manager.anchor_crosshair = tool
                .zip(snap_pointer.filter(|pointer| price_rect.contains(*pointer)))
                .map(|(tool, pointer)| {
                    let y = drawing_manager.snap_placement(Some(tool), pointer).y;
                    (ui.ctx().cumulative_frame_nr(), pointer.y, anchor_y(y))
                });
        }
        drawing_manager.drag_crosshair = None;

        // Handle drawing interaction
        // Select converter based on tool type: freeform tools (ContinuousDraw) use precise,
        // constrained tools retain their angles; other tools follow `snap_to_time`.
        let x_to_bar_edit = |x| {
            if ui.input(|i| i.modifiers.shift) {
                x_to_bar_precise(x)
            } else {
                x_to_bar_snapped(x)
            }
        };
        // A measurement lasts until the next click on the chart.
        if response.clicked() {
            drawing_manager.measurement = None;
        }
        let count_before = drawing_manager.drawings.len();
        // A line shortcut places its line at the pointer, as a click there would.
        if drawing_manager.place_at_pointer
            && let Some(tool) = drawing_manager.active_tool
            && let Some(pos) = response.hover_pos().filter(|pos| price_rect.contains(*pos))
        {
            drawing_manager.place_at_pointer = false;
            drawing_manager.start_drawing_with_coords(tool, pos, x_to_bar_snapped, y_to_price);
        }
        let owns_gesture = drawing_manager
            .curr_drawing
            .as_ref()
            .filter(|d| !d.chart_points.is_empty())
            .or_else(|| {
                drawing_manager
                    .dragging_handle
                    .and_then(|(id, _)| drawing_manager.drawings.iter().find(|d| d.id == id))
            })
            .is_none_or(|d| drawing_manager.owns_pane(d) || d.reaches_other_panes());
        // Only an area gesture in this pane repaints for its input; drags and other panes do not.
        let area_input = drawing_manager.active_tool.is_none()
            && drawing_manager.dragging_handle.is_none()
            && drawing_manager.box_selection_input(ui)
            && (drawing_manager.selection_area())
                .is_none_or(|area| area.pane == pane.map(|id| id.0));
        if owns_gesture {
            text::begin_fibonacci_edit(ui, drawing_manager, response, price_rect);
            text::begin_shape_edit(ui, drawing_manager, response);
        }
        if !owns_gesture || drawing_manager.text_editor.is_some() {
            // The editor owns this frame's pointer input, including its outside commit click.
        } else if let Some(active_tool) = drawing_manager.active_tool {
            let uses_precise = active_tool.interaction_mode()
                == DrawingInteractionMode::ContinuousDraw
                || (ui.input(|i| i.modifiers.shift)
                    && (DrawingManager::constrains_angle(active_tool)
                        || DrawingManager::squares_box(active_tool)));
            if uses_precise {
                interaction::handle_active_tool(
                    ui,
                    drawing_manager,
                    response,
                    price_rect,
                    &x_to_bar_precise,
                    &y_to_price,
                    active_tool,
                );
            } else {
                interaction::handle_active_tool(
                    ui,
                    drawing_manager,
                    response,
                    price_rect,
                    &x_to_bar_snapped,
                    &y_to_price,
                    active_tool,
                );
            }
        } else if !area_input {
            selection::handle_selection(
                ui,
                drawing_manager,
                cursor_modes,
                response,
                price_rect,
                pane.map(|id| id.0),
                &x_to_bar_edit,
                &y_to_price,
                &translate_price,
            );
        }

        // A card placed this frame stands at the time its content was published.
        if drawing_manager.drawings.len() > count_before
            && drawing_manager.dragging_handle.is_none()
            && let Some(id) = drawing_manager.drawings.last().map(|d| d.id)
        {
            self.place_new_post(drawing_manager, id);
        }
        let edit_id = if drawing_manager.text_editor.is_some() {
            None
        } else if drawing_manager.drawings.len() > count_before
            && drawing_manager.dragging_handle.is_none()
        {
            // A card placed with its post has its content; there is nothing to type.
            drawing_manager
                .drawings
                .last()
                .filter(|d| d.post.is_none())
                .map(|d| (d.id, true))
        } else if response.double_clicked() || response.triple_clicked() {
            response
                .interact_pointer_pos()
                .and_then(|pos| drawing_manager.hit_test(pos))
                .map(|id| (id, false))
        } else {
            None
        };
        if let Some((id, placed)) = edit_id {
            if drawing_manager.drawings.len() > count_before
                && drawing_manager
                    .drawings
                    .iter()
                    .any(|d| d.id == id && d.tool_type == crate::drawings::DrawingToolType::Image)
            {
                drawing_manager.image_request = Some(id);
            }
            text::begin_edit(drawing_manager, id, placed);
        }

        drawing_manager.update_pane_screen_coords(bar_to_x, price_to_y, timescale.bar_spacing());

        let finer = self.study_data.data(
            self.profile_histories(drawing_manager)
                .into_iter()
                .map(|(request, _)| request)
                .collect(),
            &self.symbol,
            self.timeframe,
            self.replay_time(),
        );
        let price_member = self.pane_price_member(pane);
        let price_formatter = price_member.and_then(|id| self.series_price_formatter(id));
        // Drawings write dates as the crosshair labels the same bar; a host horizontal scale has
        // no dates.
        // Render existing drawings against the chart's bars
        let view = ChartView {
            price_formatter: price_formatter.as_deref(),
            thousands_sep: self.drawing_thousands_sep(price_member),
            time_formatter: self.horizontal_scale.is_none().then_some(time_format.0),
            bar_seconds: time_format.1,
            instrument: price_member.and(self.instrument()),
            bars: self.state.shown(),
            revision: self.state.revision(),
            bar_to_x: &bar_to_x,
            bar_spacing: timescale.bar_spacing(),
            price_to_y: &price_to_y,
            y_to_price: &|y| price_map.y_to_price(y),
            time_axis: self.horizontal_scale.is_none(),
            executions: self.drawing_executions(),
            timeframe: self.drawing_timeframe(),
            finer: &finer,
        };
        if area_input && owns_gesture && drawing_manager.text_editor.is_none() {
            // Area input needs this frame's geometry, before the selected handles are painted.
            let previous = ui
                .ctx()
                .graphics_mut(|g| std::mem::take(g.entry(painter.layer_id())));
            drawing_manager.render_all(painter, price_rect, &view);
            ui.ctx()
                .graphics_mut(|g| *g.entry(painter.layer_id()) = previous);
            selection::handle_selection(
                ui,
                drawing_manager,
                cursor_modes,
                response,
                price_rect,
                pane.map(|id| id.0),
                &x_to_bar_edit,
                &y_to_price,
                &translate_price,
            );
        }
        // A drag shows the height it puts its handle or grabbed point at, as a tool shows the
        // next anchor's.
        if pane.is_none()
            && let Some(y) = drawing_manager.drag_crosshair
            && let Some(pointer) = snap_pointer.filter(|pointer| price_rect.contains(*pointer))
        {
            drawing_manager.anchor_crosshair =
                Some((ui.ctx().cumulative_frame_nr(), pointer.y, anchor_y(y)));
        }
        drawing_manager.render_all(painter, price_rect, &view);
        if drawing_manager.text_editor.as_ref().is_some_and(|editor| {
            drawing_manager
                .drawings
                .iter()
                .any(|d| d.id == editor.id && drawing_manager.owns_pane(d))
        }) {
            text::show_editor(
                ui,
                drawing_manager,
                response.id,
                price_rect,
                self.post_source.is_some(),
            );
        }
    }

    /// Render eraser highlight for hovered drawing
    pub(crate) fn render_eraser_highlight(
        &self,
        painter: &Painter,
        drawing_manager: &DrawingManager,
        cursor_modes: &CursorModeState,
    ) {
        selection::render_eraser_highlight(
            &painter.with_clip_rect(self.get_rendered_price_rect()),
            drawing_manager,
            cursor_modes,
        );
    }
}

/// The fractional bar at `x`, or exactly the bar that `idx_to_coord_precise` paints at `x`, so
/// that a point snapped to a bar centre keeps that bar through the `f32` round trip.
fn bar_at(timescale: &crate::model::TimeScale, x: f32, rect_min_x: f32, rect_width: f32) -> f32 {
    let index = timescale.coord_to_idx(x, rect_min_x, rect_width);
    let bar = index.round();
    if timescale.idx_to_coord_precise(bar, rect_min_x, rect_width) == x {
        bar
    } else {
        index
    }
}

#[cfg(test)]
mod tests {
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType, TimeframeVisibility};
    use crate::model::{ChartType, Timeframe};
    use crate::widget::RightClickTarget;
    use crate::widget::test_frames::{chart, frame, shapes};
    use egui::{Color32, Context, Event, Key, Modifiers, PointerButton, Shape};

    /// A painted bar centre converts back to exactly its bar, where the plain conversion lands
    /// a few millionths of a bar to either side.
    #[test]
    fn painted_bar_centres_convert_back_to_whole_bars() {
        for (spacing, offset) in [(7.3, 0.0), (7.3, 0.37), (21.7, 3.0), (6.0, 0.37)] {
            let mut scale = crate::model::TimeScale::new();
            scale.set_bar_cnt(100);
            scale.set_width(789.0);
            scale.set_bar_spacing(spacing);
            scale.set_right_offset(offset);
            for bar in 0..100 {
                let x = scale.idx_to_coord(bar, 13.0, 789.0);
                assert_eq!(
                    super::bar_at(&scale, x, 13.0, 789.0),
                    bar as f32,
                    "spacing {spacing}, offset {offset}, bar {bar}"
                );
            }
        }
    }

    /// Drawings use raw prices from their pane's series, including changes and level labels;
    /// without a formatter they group thousands as the axis does, and a formatter's text is never
    /// grouped.
    #[test]
    fn drawings_write_prices_with_the_formatter_of_their_series() {
        use crate::drawings::FibonacciConfig;
        use crate::scales::CustomPriceFormatter;
        use crate::widget::SeriesId;
        use std::sync::Arc;

        let marked = |suffix: &'static str| {
            Some(Arc::new(CustomPriceFormatter::new(move |price| {
                format!("{price:.1}{suffix}")
            })) as Arc<dyn crate::scales::PriceFormatter>)
        };
        let ctx = Context::default();
        let mut chart = chart(&[90_000.0], &[110_000.0]);
        use DrawingToolType::*;
        for (id, (tool, anchors)) in [
            (Measure, vec![(30.0, 100_000.0), (40.0, 95_000.0)]),
            (Forecast, vec![(30.0, 100_000.0), (40.0, 105_000.0)]),
            (LongPos, vec![(30.0, 100_000.0), (40.0, 106_000.0)]),
            (
                FibonacciRetracement,
                vec![(30.0, 90_000.0), (40.0, 110_000.0)],
            ),
            (Rect, vec![(30.0, 92_000.0), (40.0, 108_000.0)]),
            (PriceLabel, vec![(30.0, 102_000.0)]),
            (GannSquare, vec![(30.0, 90_000.0), (40.0, 110_000.0)]),
        ]
        .into_iter()
        .enumerate()
        {
            let mut drawing = Drawing::new(id, tool);
            drawing.chart_points = anchors
                .into_iter()
                .map(|(bar, price)| ChartPoint::new(bar, price))
                .collect();
            drawing.completed = true;
            drawing.position_readout.persistent = tool == LongPos;
            if tool == FibonacciRetracement {
                drawing.fib_config = Some(FibonacciConfig {
                    show_prices: Some(true),
                    ..Default::default()
                });
            }
            chart.drawings_mut().drawings.push(drawing);
        }
        chart.drawings_mut().select(4);
        // Only texts left of the price axes, whose tick labels can read as a drawing's price.
        let texts = |chart: &mut crate::Chart, time| {
            let painted = shapes(&ctx, chart, time, vec![]);
            let right = chart.get_rendered_price_rect().right();
            painted
                .into_iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) if text.pos.x < right => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let mut failures = Vec::new();
        let built_in = texts(&mut chart, 0.0);
        // Hand values in thousands: 95-100=-5, 105-100=5,
        // target 106 and mirrored stop 94, midpoint 100, height 16 at 16/92*100 percent.
        for expected in [
            "-5,000.00 (-5.00%)",
            "+5,000.00 (+5.00%)",
            "Target: 6,000.00 (6.000%)",
            "Stop: 6,000.00 (6.000%)",
            "(100,000.00)",
            "16,000.00 (17.39%)",
            "102,000.00",
            "20,000.00\n10 bars\n2000.0000/bar",
        ] {
            if !built_in.iter().any(|text| text.contains(expected)) {
                failures.push(format!("missing built-in {expected}"));
            }
        }
        chart.set_price_formatter(marked(" pts"));
        let formatted = texts(&mut chart, 0.1);
        for expected in [
            "-5000.0 pts (-5.00%)",
            "+5000.0 pts (+5.00%)",
            "Target: 6000.0 pts (6.000%)",
            "Stop: 6000.0 pts (6.000%)",
            "(100000.0 pts)",
            "16000.0 pts (17.39%)",
            "102000.0 pts",
            "20000.0 pts\n10 bars\n2000.0000/bar",
        ] {
            if !formatted.iter().any(|text| text.contains(expected)) {
                failures.push(format!("missing {expected}"));
            }
        }
        assert!(chart.set_series_formatter(SeriesId::MAIN, marked(" main")));
        let formatted = texts(&mut chart, 0.2);
        if !formatted.iter().any(|text| text == "102000.0 main")
            || formatted.iter().any(|text| text == "102000.0 pts")
        {
            failures.push("main series formatter overrides chart formatter".to_owned());
        }
        let series = chart.add_series(Default::default());
        chart.set_data(series, chart.bars(SeriesId::MAIN).to_vec());
        let pane = chart.add_pane();
        assert!(chart.move_series_to_pane(series, pane));
        assert!(chart.set_series_formatter(series, marked(" alt")));
        let mut drawing = Drawing::new(8, PriceLabel);
        drawing.chart_points = vec![ChartPoint::new(30.0, 102_000.0)];
        drawing.completed = true;
        drawing.pane = Some(series.0);
        chart.drawings_mut().drawings.push(drawing);
        let formatted = texts(&mut chart, 0.3);
        for expected in ["102000.0 main", "102000.0 alt"] {
            if !formatted.iter().any(|text| text == expected) {
                failures.push(format!("missing {expected}"));
            }
        }
        // A host may supply its own sign or put a currency before a negative number.
        chart.set_series_formatter(SeriesId::MAIN, None);
        for (prefix, change, expected) in [
            (false, 5_000.0, "+5000.0 pts (+5.00%)"),
            (false, -5_000.0, "-5000.0 pts (-5.00%)"),
            (true, 5_000.0, "+USD 5000.0 (+5.00%)"),
            (true, -5_000.0, "USD -5000.0 (-5.00%)"),
        ] {
            chart.set_price_formatter(Some(Arc::new(CustomPriceFormatter::new(move |price| {
                if prefix {
                    format!("USD {price:.1}")
                } else {
                    format!("{price:+.1} pts")
                }
            }))));
            for drawing in &mut chart.drawings_mut().drawings[..2] {
                drawing.chart_points[1].price = 100_000.0 + change;
            }
            let formatted = texts(&mut chart, 0.4);
            assert_eq!(
                formatted
                    .iter()
                    .filter(|text| text.starts_with(expected))
                    .count(),
                2,
                "measurement and forecast must each show {expected}"
            );
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Forecast anchors use the crosshair's date format, display zone and bar duration.
    #[test]
    fn forecast_dates_follow_the_chart_time_zone_and_time_formatter() {
        use crate::model::{Bar, BarData, TimezoneId};
        use crate::scales::{TickMarkType, TimeFormatter, TimezoneMode};
        use chrono::{DateTime, Duration, TimeZone, Utc};
        use std::sync::Arc;

        #[derive(Clone)]
        struct Host;
        impl TimeFormatter for Host {
            fn format(&self, _: DateTime<Utc>, _: TickMarkType) -> String {
                String::new()
            }
            fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
                format!("host {} {bar_seconds}", time.format("%H:%M"))
            }
            fn clone_box(&self) -> Box<dyn TimeFormatter> {
                Box::new(self.clone())
            }
        }

        let start = Utc.with_ymd_and_hms(2024, 1, 10, 14, 0, 0).unwrap();
        let bars = (0..100)
            .map(|i| Bar::new(start + Duration::minutes(i), 95.0, 110.0, 90.0, 105.0, 1.0))
            .collect();
        let mut chart = crate::Chart::new(BarData::from_bars(bars));
        let mut forecast = Drawing::new(1, DrawingToolType::Forecast);
        forecast.chart_points = vec![ChartPoint::new(30.0, 100.0), ChartPoint::new(40.0, 105.0)];
        forecast.completed = true;
        chart.drawings_mut().drawings.push(forecast);
        let ctx = Context::default();
        let mut failures = Vec::new();
        // Wednesday 10 January 2024: 14:00 + 30/40 one-minute bars, then minus five hours.
        for (step, expected) in [
            ["Wed 10 Jan '24 14:30", "Wed 10 Jan '24 14:40"],
            ["Wed 10 Jan '24 09:30", "Wed 10 Jan '24 09:40"],
            ["host 09:30 60", "host 09:40 60"],
        ]
        .into_iter()
        .enumerate()
        {
            if step == 1 {
                chart.chart_options.time_scale.timezone =
                    TimezoneMode::Exchange(TimezoneId::fixed_offset(-5 * 3600).unwrap());
                assert_eq!(
                    chart.format_time(start + Duration::minutes(30)),
                    expected[0]
                );
            } else if step == 2 {
                chart.set_time_formatter(Some(Arc::new(Host)));
            }
            let texts: Vec<_> = shapes(&ctx, &mut chart, step as f64 * 0.1, vec![])
                .into_iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            for date in expected {
                if !texts
                    .iter()
                    .any(|text| text.ends_with(&format!("\n{date}")))
                {
                    failures.push(format!(
                        "step {step}: missing forecast date {date}: {texts:?}"
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn drawings_reuse_the_frames_time_formatter() {
        use crate::scales::{TickMarkType, TimeFormatter};
        use chrono::{DateTime, Utc};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        #[derive(Clone)]
        struct Host(Arc<AtomicUsize>);
        impl TimeFormatter for Host {
            fn format(&self, _: DateTime<Utc>, _: TickMarkType) -> String {
                "tick".into()
            }
            fn clone_box(&self) -> Box<dyn TimeFormatter> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Box::new(self.clone())
            }
        }

        let clones = Arc::new(AtomicUsize::new(0));
        let mut chart = crate::widget::test_frames::chart(&[95.0; 100], &[105.0; 100]);
        chart.config.show_volume = false;
        // Exclude the axis mark generator's own formatter clone.
        chart.config.show_time_labels = false;
        chart.config.show_grid = false;
        chart.set_time_formatter(Some(Arc::new(Host(clones.clone()))));
        let ctx = Context::default();
        for forecast in [false, true] {
            if forecast {
                let mut drawing = Drawing::new(1, DrawingToolType::Forecast);
                drawing.chart_points =
                    vec![ChartPoint::new(30.0, 100.0), ChartPoint::new(40.0, 105.0)];
                drawing.completed = true;
                chart.drawings_mut().drawings.push(drawing);
            }
            clones.store(0, Ordering::Relaxed);
            let mut shows = 0;
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                shows += 1;
                chart.show(ui);
            });
            output.textures_delta.clear();
            assert!(shows > 0);
            assert!(
                clones.load(Ordering::Relaxed) <= shows,
                "drawings add no formatter clones to the frame's own, forecast: {forecast}"
            );
        }
    }

    /// Primary clicks place two anchors; secondary drags leave the preview and Escape cancels it.
    #[test]
    fn two_point_tools_accept_clicks_and_cancel_unfinished_previews() {
        use crate::drawings::DrawingToolType::*;
        use egui::pos2;
        for tool in [
            Rect,
            Circle,
            Ellipse,
            Measure,
            PriceRange,
            DateRange,
            DateAndPriceRange,
            GannBox,
            GannSquare,
            GannFixed,
            Forecast,
            FixedRangeVolumeProfile,
            FibonacciRetracement,
            FibonacciSpeedResistanceArcs,
            FibonacciTimeZones,
            FibonacciCircles,
            FibonacciSpeedFan,
            FibonacciSpiral,
            GannFan,
            RegressionTrend,
            CyclicLines,
            TimeCycles,
            SineLine,
            Callout,
            Arrow,
            TrendLine,
        ] {
            for (cancel, batched, secondary_drag, snap_to_time) in [
                (false, false, false, true),
                (true, false, false, true),
                (false, true, false, true),
                (false, false, true, true),
                (false, false, false, false),
            ] {
                let ctx = Context::default();
                let mut chart = chart(&[10.0], &[20.0]);
                frame(&ctx, &mut chart, 0.0, vec![]);
                chart.drawings_mut().options.snap_to_time = snap_to_time;
                chart.drawings_mut().set_active_tool(Some(tool));
                let map = chart.get_chart_mapping();
                let a = pos2(map.index_to_x(30.4), map.price_to_y(16.0));
                let b = pos2(map.index_to_x(49.6), map.price_to_y(14.0));
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                };
                let frames = if batched {
                    vec![
                        vec![Event::PointerMoved(a), button(a, true), button(a, false)],
                        vec![Event::PointerMoved(b)],
                    ]
                } else {
                    vec![
                        vec![Event::PointerMoved(a)],
                        vec![button(a, true)],
                        vec![button(a, false)],
                        vec![Event::PointerMoved(b)],
                    ]
                };
                for (i, events) in frames.into_iter().enumerate() {
                    frame(&ctx, &mut chart, (i + 1) as f64 * 0.1, events);
                }
                let preview = chart
                    .drawings()
                    .curr_drawing
                    .as_ref()
                    .expect("live preview");
                assert_eq!(preview.chart_points.len(), 2, "{tool:?}");
                let (first, last) = if snap_to_time {
                    (30.0, 50.0)
                } else {
                    (30.4, 49.6)
                };
                assert!(
                    (preview.chart_points[0].bar_idx - first).abs() < 1e-3,
                    "{tool:?}"
                );
                assert!(
                    (preview.chart_points[1].bar_idx - last).abs() < 1e-3,
                    "{tool:?}"
                );
                assert!(chart.drawings().drawings.is_empty());
                if secondary_drag {
                    for (i, events) in [
                        vec![Event::PointerButton {
                            pos: b,
                            button: PointerButton::Secondary,
                            pressed: true,
                            modifiers: Modifiers::NONE,
                        }],
                        vec![Event::PointerMoved(b + egui::vec2(20.0, 0.0))],
                        vec![Event::PointerButton {
                            pos: b,
                            button: PointerButton::Secondary,
                            pressed: false,
                            modifiers: Modifiers::NONE,
                        }],
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        frame(&ctx, &mut chart, 0.41 + i as f64 * 0.01, events);
                    }
                    assert!(
                        chart.drawings().drawings.is_empty(),
                        "{tool:?}: secondary drag"
                    );
                    assert!(chart.drawings().curr_drawing.is_some());
                }
                if cancel {
                    frame(
                        &ctx,
                        &mut chart,
                        0.5,
                        vec![Event::Key {
                            key: Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Modifiers::NONE,
                        }],
                    );
                    assert!(chart.drawings().drawings.is_empty());
                } else {
                    if batched {
                        frame(
                            &ctx,
                            &mut chart,
                            0.5,
                            vec![button(b, true), button(b, false)],
                        );
                    } else {
                        frame(&ctx, &mut chart, 0.5, vec![button(b, true)]);
                        frame(&ctx, &mut chart, 0.6, vec![button(b, false)]);
                    }
                    let drawing = if tool == Measure {
                        assert!(chart.drawings().drawings.is_empty());
                        assert!(chart.drawings().pending_sync.lock().unwrap().is_empty());
                        assert!(!chart.undo());
                        assert_eq!(chart.drawings().active_tool, None);
                        chart.drawings().measurement.as_ref().unwrap()
                    } else {
                        &chart.drawings().drawings[0]
                    };
                    assert_eq!(drawing.chart_points.len(), 2, "{tool:?}");
                    assert!(
                        (drawing.chart_points[0].bar_idx - first).abs() < 1e-3,
                        "{tool:?}"
                    );
                    assert!(
                        (drawing.chart_points[1].bar_idx - last).abs() < 1e-3,
                        "{tool:?}"
                    );
                    if tool == Measure {
                        // The readout rounds the anchors to bars 30 and 50, including after completion.
                        assert!(shapes(&ctx, &mut chart, 0.7, vec![]).iter().any(|shape| {
                            matches!(shape, Shape::Text(text) if text.galley.text().contains("20 bars"))
                        }));
                        frame(
                            &ctx,
                            &mut chart,
                            0.8,
                            vec![button(b, true), button(b, false)],
                        );
                        assert!(chart.drawings().measurement.is_none());
                    } else {
                        assert!(chart.undo());
                        assert!(chart.drawings().drawings.is_empty());
                        assert!(chart.redo());
                        assert_eq!(chart.drawings().drawings.len(), 1);
                    }
                }
                assert!(chart.drawings().curr_drawing.is_none(), "{tool:?}");
            }
        }
    }

    /// Closing uses placed anchors, Escape keeps paths with at least two, a double click
    /// finishes without reporting itself to the host while a later one does, and two clicks
    /// place curves with editable controls between their endpoints.
    #[test]
    fn multi_point_shapes_close_finish_on_escape_and_curves_take_two_clicks() {
        use crate::widget::{Chart, ChartEvent, PointerTarget};
        use DrawingToolType::{Curve, DoubleCurve, Path, Polyline};
        #[derive(PartialEq)]
        enum End {
            Click,
            Escape,
            DoubleClick,
        }
        let a = (30, 16.0);
        let b = (50, 14.0);
        let c = (40, 12.0);
        let doubles = |chart: &mut Chart| {
            chart
                .drain_events()
                .filter_map(|event| match event {
                    ChartEvent::DoubleClick { position } => Some(position.target),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        for (tool, clicks, end, kept) in [
            (
                Polyline,
                vec![a, b, c, a],
                End::Click,
                Some((3, true, 40.0)),
            ),
            (Polyline, vec![a, b, a], End::Escape, Some((3, false, 30.0))),
            (Polyline, vec![a], End::Escape, None),
            (Path, vec![a, b], End::Escape, Some((2, false, 50.0))),
            (Path, vec![a], End::Escape, None),
            (
                Path,
                vec![a, b, c],
                End::DoubleClick,
                Some((3, false, 40.0)),
            ),
            (
                Polyline,
                vec![a, b, c, a],
                End::DoubleClick,
                Some((3, true, 40.0)),
            ),
            (Curve, vec![a, b], End::Click, Some((3, false, 50.0))),
            (DoubleCurve, vec![a, b], End::Click, Some((4, false, 50.0))),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.drawings_mut().set_active_tool(Some(tool));
            let map = chart.get_chart_mapping();
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            let at =
                |(bar, price): (usize, f64)| egui::pos2(map.idx_to_x(bar), map.price_to_y(price));
            let mut last = (egui::Pos2::ZERO, 0.0);
            for (i, &point) in clicks.iter().enumerate() {
                let pos = at(point);
                let time = 1.0 + i as f64;
                frame(&ctx, &mut chart, time, vec![Event::PointerMoved(pos)]);
                frame(&ctx, &mut chart, time + 0.1, vec![button(pos, true)]);
                frame(&ctx, &mut chart, time + 0.2, vec![button(pos, false)]);
                last = (pos, time);
            }
            if end == End::DoubleClick {
                let (pos, time) = last;
                frame(&ctx, &mut chart, time + 0.3, vec![button(pos, true)]);
                frame(&ctx, &mut chart, time + 0.4, vec![button(pos, false)]);
            }
            if end == End::Escape {
                frame(
                    &ctx,
                    &mut chart,
                    6.0,
                    vec![Event::Key {
                        key: Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    }],
                );
            }
            assert!(chart.drawings().curr_drawing.is_none(), "{tool:?}");
            assert_eq!(
                doubles(&mut chart),
                [],
                "{tool:?}: placement reports no double click"
            );
            let Some((count, closed, last_bar)) = kept else {
                assert!(chart.drawings().drawings.is_empty(), "{tool:?}");
                continue;
            };
            assert_eq!(chart.drawings().drawings.len(), 1, "{tool:?}");
            let drawing = &chart.drawings().drawings[0];
            assert_eq!(drawing.chart_points.len(), count, "{tool:?}");
            assert_eq!(drawing.closed, closed, "{tool:?}");
            assert!((drawing.chart_points[0].bar_idx - 30.0).abs() < 1e-3);
            assert!((drawing.chart_points.last().unwrap().bar_idx - last_bar).abs() < 1e-3);
            let id = drawing.id;
            assert!(
                chart
                    .set_drawing_points(id, &chart.drawing_points(id).unwrap())
                    .is_ok()
            );
            if end == End::DoubleClick {
                let mid = at(a).lerp(at(b), 0.5);
                for (i, pressed) in [true, false, true, false].into_iter().enumerate() {
                    let events = vec![Event::PointerMoved(mid), button(mid, pressed)];
                    frame(&ctx, &mut chart, 7.0 + 0.1 * i as f64, events);
                }
                assert_eq!(
                    doubles(&mut chart),
                    [Some(PointerTarget::Drawing(id))],
                    "{tool:?}"
                );
            }
        }
    }

    /// Shift-click on empty space uses the ruler without persisting it or bypassing availability.
    #[test]
    fn shift_click_on_empty_space_measures_until_the_next_click() {
        use crate::drawings::ToolAvailability;
        use egui::pos2;
        for (enabled, snap_to_time) in [(true, true), (false, true), (true, false)] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.drawings_mut().options.snap_to_time = snap_to_time;
            chart.drawings_mut().registry.set_availability(
                DrawingToolType::Measure,
                ToolAvailability {
                    visible: enabled,
                    enabled,
                },
            );
            let map = chart.get_chart_mapping();
            let a = pos2(map.index_to_x(30.4), map.price_to_y(16.0));
            let b = pos2(map.index_to_x(49.6), map.price_to_y(14.0));
            let button = |pos, pressed, modifiers| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers,
            };
            for (i, events) in [
                vec![
                    Event::ModifiersChanged(Modifiers::SHIFT),
                    Event::PointerMoved(a),
                ],
                vec![button(a, true, Modifiers::SHIFT)],
                vec![button(a, false, Modifiers::SHIFT)],
                vec![
                    Event::ModifiersChanged(Modifiers::NONE),
                    Event::PointerMoved(b),
                ],
                vec![button(b, true, Modifiers::NONE)],
                vec![button(b, false, Modifiers::NONE)],
            ]
            .into_iter()
            .enumerate()
            {
                frame(&ctx, &mut chart, (i + 1) as f64 * 0.1, events);
            }
            assert!(chart.drawings().drawings.is_empty());
            assert!(chart.drawings().curr_drawing.is_none());
            assert_eq!(chart.drawings().active_tool, None);
            if enabled {
                let expected = if snap_to_time {
                    [30.0, 50.0]
                } else {
                    [30.4, 49.6]
                };
                for (point, bar) in chart
                    .drawings()
                    .measurement
                    .as_ref()
                    .unwrap()
                    .chart_points
                    .iter()
                    .zip(expected)
                {
                    assert!((point.bar_idx - bar).abs() < 1e-3, "{point:?}, {bar}");
                }
                frame(
                    &ctx,
                    &mut chart,
                    0.7,
                    vec![
                        button(b, true, Modifiers::NONE),
                        button(b, false, Modifiers::NONE),
                    ],
                );
            }
            assert!(chart.drawings().measurement.is_none());
        }
    }

    /// A square has equal screen sides in any quadrant, including against a pane edge.
    #[test]
    fn shift_box_creation_is_square_in_preview_and_after_release() {
        use DrawingToolType as T;
        use egui::{pos2, vec2};
        for (tool, shift, delta) in [
            (T::Rect, true, vec2(240.0, 60.0)),
            (T::Rect, false, vec2(240.0, 60.0)),
            (T::Rect, true, vec2(-240.0, 60.0)),
            (T::Rect, true, vec2(240.0, -60.0)),
            (T::Rect, true, vec2(-240.0, -60.0)),
            (T::Rect, true, vec2(1000.0, 60.0)),
            (T::FibonacciCircles, true, vec2(1000.0, 60.0)),
            (T::GannBox, true, vec2(1000.0, 60.0)),
            (T::GannSquare, true, vec2(1000.0, 60.0)),
            (T::GannFixed, true, vec2(1000.0, 60.0)),
            (T::GannFan, true, vec2(1000.0, 60.0)),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.drawings_mut().set_active_tool(Some(tool));
            let map = chart.get_chart_mapping();
            let center = chart.get_rendered_price_rect().center();
            let a = pos2(map.idx_to_x(map.x_to_idx(center.x)), center.y);
            let b = a + delta;
            let modifiers = Modifiers {
                shift,
                ..Modifiers::NONE
            };
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers,
            };
            for (i, events) in [
                vec![Event::PointerMoved(a)],
                vec![button(a, true)],
                vec![Event::PointerMoved(a + vec2(60.0, 15.0))],
                vec![Event::PointerMoved(b)],
            ]
            .into_iter()
            .enumerate()
            {
                let mut events = events;
                events.insert(0, Event::ModifiersChanged(modifiers));
                frame(&ctx, &mut chart, (i + 1) as f64 * 0.1, events);
            }
            let pane = chart.get_rendered_price_rect();
            let expected_side = if delta.x > 500.0 {
                pane.height().min(pane.width()) / 2.0
            } else {
                240.0
            };
            let square = |points: &[egui::Pos2]| {
                let d = points[1] - points[0];
                assert!(
                    (d.x.abs() - expected_side).abs() < 0.01,
                    "{tool:?} {delta:?}: {points:?}"
                );
                assert!(
                    (d.x.abs() - d.y.abs()).abs() < 0.01,
                    "{tool:?} {delta:?}: {points:?}"
                );
            };
            if shift {
                square(&chart.drawings().curr_drawing.as_ref().unwrap().points);
            }
            frame(
                &ctx,
                &mut chart,
                0.5,
                vec![Event::ModifiersChanged(modifiers), button(b, false)],
            );
            let drawing = &chart.drawings().drawings[0];
            if shift {
                square(&drawing.points);
            } else {
                let d = drawing.points[1] - drawing.points[0];
                assert!(d.x.abs() > d.y.abs() * 3.0);
            }
            // Check the stored coordinates map back to the constrained screen geometry.
            let map = chart.get_chart_mapping();
            for (screen, point) in drawing.points.iter().zip(&drawing.chart_points) {
                assert!((map.x_to_idx_f32(screen.x) - point.bar_idx).abs() < 1e-4);
                assert!((screen.y - map.price_to_y(point.price)).abs() < 0.01);
            }
        }
    }

    /// Drawing anchors use nearby bar and visible overlay values only while the magnet is on,
    /// including after a view change and a handle drag that can be undone and redone.
    #[test]
    fn drawing_magnet_places_and_drags_anchors_in_data_coordinates() {
        use crate::studies::{CustomIndicator, IndicatorValue};
        use egui::{Pos2, vec2};

        for (
            (magnet, strong, price, overlay_visible, snaps, offset, off_scale),
            kind,
            snap_to_indicators,
        ) in [
            (true, false, 10.0, true, true, 3.0, false),
            (true, false, 20.0, true, true, 3.0, false),
            (true, false, 12.0, true, true, 3.0, false),
            (true, false, 15.0, true, true, 3.0, false),
            (true, false, 17.0, true, true, 3.0, false),
            (false, false, 15.0, true, false, 3.0, false),
            (true, false, 15.0, false, false, 3.0, false),
            (true, true, 15.0, true, true, 3.0, false),
            (true, true, 17.0, true, true, 3.0, false),
            (true, true, 20.0, false, true, 40.0, false),
            (true, false, 20.0, false, false, 40.0, false),
            (true, true, 10.0, false, true, 40.0, false),
            (true, true, 20.0, false, true, 3.0, true),
        ]
        .into_iter()
        .map(|case| (case, "main", true))
        .chain([
            (
                (true, false, 15.0, true, true, 3.0, false),
                "independent",
                true,
            ),
            ((true, false, 15.0, true, true, 3.0, false), "none", true),
            (
                (true, false, 15.0, true, false, 3.0, false),
                "background",
                true,
            ),
            (
                (true, false, 15.0, true, true, 3.0, false),
                "displaced",
                true,
            ),
            ((true, false, 15.0, true, true, 3.0, false), "pane", true),
            ((true, true, 15.0, true, true, 30.0, false), "pane", true),
            ((true, false, 15.0, true, true, 3.0, false), "future", true),
            ((true, true, 15.0, true, true, 3.0, false), "future", true),
            ((true, false, 15.0, true, false, 3.0, false), "main", false),
            ((true, false, 17.0, true, false, 3.0, false), "main", false),
            ((true, false, 20.0, true, true, 3.0, false), "main", false),
            ((true, false, 15.0, true, false, 3.0, false), "pane", false),
            ((false, false, 15.0, true, false, 3.0, false), "free", true),
        ]) {
            let ctx = Context::default();
            let mut chart = chart(&[0.0, 10.0, 10.0], &[30.0, 20.0, 20.0]);
            chart.config_mut().navigation_buttons = crate::config::NavigationButtons::Never;
            let overlay = CustomIndicator::new("Two levels", move |bars| {
                bars.iter()
                    .enumerate()
                    .map(|(index, _)| {
                        if kind == "displaced" && index % 3 != 0 {
                            IndicatorValue::None
                        } else {
                            IndicatorValue::Multiple(vec![15.0, 17.0])
                        }
                    })
                    .collect()
            })
            .with_line_count(2)
            .with_overlay(kind != "pane")
            .with_offsets(vec![
                match kind {
                    "displaced" => 1,
                    "future" => 100,
                    _ => 0,
                };
                2
            ])
            .with_plot_types(vec![
                if kind == "background" {
                    crate::studies::PlotType::BackgroundColor
                } else {
                    crate::studies::PlotType::Line
                };
                2
            ])
            .with_scale(match kind {
                "independent" => crate::studies::StudyScale::Independent,
                "none" => crate::studies::StudyScale::None,
                _ => crate::studies::StudyScale::Main,
            });
            let id = chart.add_indicator(Box::new(overlay));
            chart.set_indicator_visible(id, overlay_visible);
            let single = chart.add_indicator(Box::new(CustomIndicator::new("One level", |bars| {
                bars.iter().map(|_| IndicatorValue::Single(12.0)).collect()
            })));
            chart.set_indicator_visible(single, overlay_visible);
            let mut time = 0.0;
            for (visible, start) in if kind == "future" {
                [(100, 50), (40, 90)]
            } else {
                [(100, 0), (40, 20)]
            } {
                chart.drawings_mut().clear_all();
                chart.drawings_mut().deselect();
                chart.set_visible_bars(visible);
                chart.set_start_idx(start);
                chart.drawings_mut().options.snap_to_time = kind != "free";
                chart.drawings_mut().options.magnet_mode = magnet;
                chart.drawings_mut().options.strong_magnet = strong;
                chart.drawings_mut().options.snap_to_indicators = snap_to_indicators;
                chart.drawings_mut().update_snap_options();
                if off_scale {
                    chart.price_scale_mut().set_range(12.0, 18.0);
                }
                time += 0.1;
                frame(&ctx, &mut chart, time, vec![]);
                let map = if kind == "pane" {
                    chart.get_rendered_indicator_panes()[0].mapping.clone()
                } else {
                    chart.get_chart_mapping()
                };
                let (first, last) = chart.state.visible_range();
                let middle = (first + last) / 2;
                let bar = if kind == "future" {
                    110
                } else {
                    middle + (4 - middle % 3) % 3
                };
                let study_map = chart.pane_study_mapping(
                    if kind == "pane" {
                        chart.series_pane(id).unwrap()
                    } else {
                        chart.main_series_pane()
                    },
                    chart
                        .rendered_studies()
                        .indicators()
                        .iter()
                        .find(|study| study.name() == "Two levels")
                        .unwrap()
                        .as_ref(),
                    &map,
                );
                let target_y = if kind == "independent" || kind == "none" {
                    study_map.price_to_y(price)
                } else {
                    map.price_to_y(price)
                };
                if kind == "independent" || kind == "none" {
                    // Values 15 and 17 fit 14.8..17.2 after 10% padding: 15 is 11/12
                    // of the way down, independently of the main price range.
                    let fraction = if price == 15.0 {
                        11.0 / 12.0
                    } else {
                        1.0 / 12.0
                    };
                    assert!(
                        (target_y - (map.rect.top() + map.rect.height() * fraction)).abs() < 0.001
                    );
                }
                let target = Pos2::new(map.idx_to_x(bar), target_y);
                let near = if off_scale {
                    Pos2::new(target.x + 2.0, map.price_to_y(17.0))
                } else {
                    target
                        + vec2(
                            2.0,
                            if price == 10.0 || kind == "pane" {
                                -offset
                            } else {
                                offset
                            },
                        )
                };
                let snapped_price = map.y_to_price(target.y);
                let expected_price = if snaps {
                    snapped_price
                } else {
                    map.y_to_price(near.y)
                };
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                };
                let mut step = |chart: &mut crate::widget::Chart, events| {
                    time += 0.1;
                    frame(&ctx, chart, time, events);
                };
                chart
                    .drawings_mut()
                    .set_active_tool(Some(DrawingToolType::TrendLine));
                for pos in [
                    near,
                    Pos2::new(
                        map.idx_to_x(bar + 8),
                        map.price_to_y(if kind == "pane" { 15.0 } else { 13.0 }),
                    ),
                ] {
                    step(&mut chart, vec![Event::PointerMoved(pos)]);
                    step(&mut chart, vec![button(pos, true)]);
                    step(&mut chart, vec![button(pos, false)]);
                }
                assert_eq!(
                    chart.drawings().drawings.len(),
                    1,
                    "magnet {magnet}, price {price}, visible {visible}, near {near:?}, rect {:?}, current {:?}",
                    map.rect,
                    chart
                        .drawings()
                        .curr_drawing
                        .as_ref()
                        .map(|d| &d.chart_points)
                );
                let drawing = &chart.drawings().drawings[0];
                assert!(
                    (drawing.chart_points[0].price - expected_price).abs() < 1e-5,
                    "placement: magnet {magnet}, price {price}, visible {visible}: {:?}",
                    drawing.chart_points
                );
                if snaps {
                    assert_eq!(drawing.chart_points[0].bar_idx, bar as f32);
                }
                let id = drawing.id;
                if off_scale {
                    // Bring the first handle into view, then drag it back to an off-scale OHLC.
                    chart.drawings_mut().drawings[0].chart_points[0].price = 15.0;
                    step(&mut chart, vec![]);
                }
                let drawing = &chart.drawings().drawings[0];
                let from = drawing.points[0];
                let before = drawing.chart_points[0];
                chart.drawings_mut().select(id);
                let to = Pos2::new(
                    map.idx_to_x(bar + 3),
                    if off_scale {
                        map.price_to_y(17.0)
                    } else {
                        target_y + if price == 20.0 { offset } else { -offset }
                    },
                ) + vec2(2.0, 0.0);
                step(&mut chart, vec![Event::PointerMoved(from)]);
                step(&mut chart, vec![button(from, true)]);
                step(&mut chart, vec![Event::PointerMoved(from + vec2(7.0, 0.0))]);
                step(&mut chart, vec![Event::PointerMoved(to)]);
                step(&mut chart, vec![button(to, false)]);
                let after = chart.drawings().drawings[0].chart_points[0];
                let want = if snaps {
                    snapped_price
                } else {
                    map.y_to_price(to.y)
                };
                assert!(
                    (after.price - want).abs() < 1e-5,
                    "drag: {kind}, strong {strong}, magnet {magnet}, price {price}, visible {visible}: {after:?}"
                );
                if kind == "free" {
                    // Without time snapping the anchor keeps the 2 px past the bar's centre.
                    let want = (bar + 3) as f32 + 2.0 / map.bar_spacing;
                    assert!(
                        (after.bar_idx - want).abs() < 1e-3,
                        "free drag: {after:?}, {want}"
                    );
                }
                assert!(chart.drawings().dragging_handle.is_none());
                assert!(chart.undo());
                assert_eq!(chart.drawings().drawings[0].chart_points[0], before);
                assert!(chart.redo());
                assert_eq!(chart.drawings().drawings[0].chart_points[0], after);
            }
        }
    }

    /// Places a trend line from `first` and returns the price and screen height of its first
    /// anchor.
    fn first_anchor(
        ctx: &Context,
        chart: &mut crate::widget::Chart,
        first: egui::Pos2,
        second: egui::Pos2,
    ) -> (f64, f32) {
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        chart
            .drawings_mut()
            .set_active_tool(Some(DrawingToolType::TrendLine));
        let mut time = 1.0;
        for pos in [first, second] {
            for events in [
                vec![Event::PointerMoved(pos)],
                vec![button(pos, true)],
                vec![button(pos, false)],
            ] {
                time += 0.1;
                frame(ctx, chart, time, events);
            }
        }
        let price = chart.drawings().drawings[0].chart_points[0].price;
        (price, chart.get_chart_mapping().price_to_y(price))
    }

    /// Hiding the main series hides its volume, so neither its bars nor its volume columns are
    /// magnet targets while it is hidden. Shown, the magnet snaps to the painted column tops, also
    /// when every visible volume is below 1.
    #[test]
    fn magnet_ignores_hidden_main_series_bars_and_volume() {
        use crate::model::Bar;
        use crate::widget::SeriesId;
        use egui::Pos2;

        let mut observed = Vec::new();
        for (visible, strong, target, volume) in [
            (true, false, "volume", 1.0),
            (true, true, "volume", 1.0),
            (true, false, "volume", 0.25),
            (true, false, "high", 1.0),
            (true, true, "high", 1.0),
            (false, false, "volume", 1.0),
            (false, true, "volume", 1.0),
            (false, false, "high", 1.0),
            (false, true, "high", 1.0),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            let bars = chart.bars(SeriesId::MAIN).iter().map(|bar| Bar {
                volume,
                ..bar.clone()
            });
            assert!(chart.set_data(SeriesId::MAIN, bars.collect()));
            chart.config_mut().show_volume = true;
            chart.drawings_mut().options.magnet_mode = true;
            chart.drawings_mut().options.strong_magnet = strong;
            chart.drawings_mut().update_snap_options();
            frame(&ctx, &mut chart, 0.1, vec![]);
            chart.set_main_series_visible(visible);
            frame(&ctx, &mut chart, 0.2, vec![]);
            let map = chart.get_chart_mapping();
            // Every bar has the visible maximum volume, so each column reaches the top of the
            // overlay: 12% of the plot above its 12 point baseline inset.
            let bottom = map.rect.bottom() - 12.0;
            let y = if target == "volume" {
                bottom - (bottom - map.rect.top()) * 0.12
            } else {
                map.price_to_y(20.0)
            };
            let (first, last) = chart.state.visible_range();
            let bar = (first + last) / 2;
            let (_, anchor) = first_anchor(
                &ctx,
                &mut chart,
                Pos2::new(
                    map.idx_to_x(bar) + 2.0,
                    if target == "volume" { y - 1.0 } else { y + 1.0 },
                ),
                Pos2::new(map.idx_to_x(bar + 8), map.rect.center().y),
            );
            observed.push((
                visible,
                strong,
                target,
                volume,
                ((anchor - y) * 100.0).round() / 100.0,
            ));
        }
        // Shown, the anchor lands on the value; hidden, it stays 1 point from where it was clicked.
        let expected = [
            (true, false, "volume", 1.0, 0.0),
            (true, true, "volume", 1.0, 0.0),
            (true, false, "volume", 0.25, 0.0),
            (true, false, "high", 1.0, 0.0),
            (true, true, "high", 1.0, 0.0),
            (false, false, "volume", 1.0, -1.0),
            (false, true, "volume", 1.0, -1.0),
            (false, false, "high", 1.0, 1.0),
            (false, true, "high", 1.0, 1.0),
        ];
        assert!(
            observed == expected,
            "(main series visible, strong magnet, target, volume, first anchor below the target) \
             observed {observed:?} expected {expected:?}"
        );
    }

    /// The price pane's studies snap where the price pane paints them, also after the main
    /// series moves into the pane of a study.
    #[test]
    fn magnet_snaps_price_pane_studies_where_painted_after_main_series_moves() {
        use crate::studies::{CustomIndicator, IndicatorValue, StudyScale};
        use crate::widget::SeriesId;
        use egui::Pos2;

        let mut observed = Vec::new();
        for (moved, strong) in [(false, false), (false, true), (true, false), (true, true)] {
            let ctx = Context::default();
            let mut chart = chart(&[100.0], &[200.0]);
            chart.config_mut().show_volume = false;
            let levels = CustomIndicator::new("Three levels", |bars| {
                bars.iter()
                    .map(|_| IndicatorValue::Multiple(vec![15.0, 16.0, 17.0]))
                    .collect()
            })
            .with_line_count(3)
            .with_overlay(!moved)
            .with_scale(StudyScale::Independent);
            let id = chart.add_indicator(Box::new(levels));
            chart.drawings_mut().options.magnet_mode = true;
            chart.drawings_mut().options.strong_magnet = strong;
            chart.drawings_mut().update_snap_options();
            frame(&ctx, &mut chart, 0.1, vec![]);
            if moved {
                let pane = chart.series_pane(id).expect("the study's pane");
                assert!(chart.move_series_to_pane(SeriesId::MAIN, pane));
            }
            frame(&ctx, &mut chart, 0.2, vec![]);
            frame(&ctx, &mut chart, 0.3, vec![]);
            let map = chart.get_chart_mapping();
            // The study fits its own scale: 15 and 17 span 14.8..17.2 after 10% padding, so 16
            // is painted halfway down the price pane.
            let y = map.rect.center().y;
            let (first, last) = chart.state.visible_range();
            let bar = (first + last) / 2;
            let (price, anchor) = first_anchor(
                &ctx,
                &mut chart,
                Pos2::new(map.idx_to_x(bar) + 2.0, y - 3.0),
                Pos2::new(map.idx_to_x(bar + 8), y - 60.0),
            );
            observed.push((
                moved,
                strong,
                (price * 100.0).round() / 100.0,
                ((anchor - y) * 100.0).round() / 100.0,
            ));
        }
        // The main 100..200 span is 100 / 0.82. Unequal margins put its midpoint at
        // 150 + (0.1 - 0.08) * (100 / 0.82) / 2 = 151.219512..., rounded here to 151.22.
        assert!(
            observed
                .iter()
                .all(|(_, _, price, below)| *price == 151.22 && *below == 0.0),
            "(main series moved into the study's pane, strong magnet, first anchor price, \
             anchor below the painted level) observed {observed:?} expected price 151.22 and 0 \
             below in every row"
        );
    }

    /// The price pane's crosshair, as drawn and as reported to the host, reads the price a click
    /// there gives the next anchor while a drawing tool is active, and the price a dragged anchor
    /// takes: the weak magnet snaps only within its distance, Ctrl inverts the magnet, and study
    /// values count only while Snap to indicators is on, whatever the crosshair mode. A size
    /// handle follows the pointer, and so does the crosshair while it is dragged.
    #[test]
    fn crosshair_reads_the_price_the_next_or_dragged_anchor_takes() {
        use crate::config::CrosshairMode;
        use crate::drawings::HandlePos;
        use crate::studies::{CustomIndicator, IndicatorValue, StudyScale};
        use crate::widget::{ChartEvent, SeriesId};
        use egui::{Pos2, vec2};

        let round = |price: f64| (price * 100.0).round() / 100.0;
        let mut observed = Vec::new();
        for gesture in ["place", "drag", "resize"] {
            for (case, magnet, strong, ctrl, snap_to_indicators, price, below) in [
                ("weak, far", true, false, false, true, 101.0, 0.0),
                ("weak, near", true, false, false, true, 105.0, 5.0),
                ("strong", true, true, false, true, 101.0, 0.0),
                ("strong, Ctrl", true, true, true, true, 101.0, 0.0),
                ("off, Ctrl", false, true, true, true, 101.0, 0.0),
                ("indicators off", true, true, false, false, 101.0, 0.0),
            ] {
                if gesture == "resize" && case != "strong" {
                    continue;
                }
                let ctx = Context::default();
                let mut chart = chart(&[90.0], &[110.0]);
                chart.config_mut().show_volume = false;
                chart.add_indicator(Box::new(
                    CustomIndicator::new("Level", |bars| {
                        bars.iter().map(|_| IndicatorValue::Single(105.0)).collect()
                    })
                    .with_overlay(true)
                    .with_scale(StudyScale::Main),
                ));
                let options = &mut chart.drawings_mut().options;
                options.magnet_mode = magnet;
                options.strong_magnet = strong;
                options.snap_to_indicators = snap_to_indicators;
                chart.drawings_mut().update_snap_options();
                // As the demo sets it for its magnet buttons.
                chart.set_crosshair_mode(if magnet {
                    CrosshairMode::Magnet
                } else {
                    CrosshairMode::Normal
                });
                frame(&ctx, &mut chart, 0.1, vec![]);
                let map = chart.get_chart_mapping();
                let (first, last) = chart.state.visible_range();
                let middle = (first + last) / 2;
                let pointer = Pos2::new(map.idx_to_x(middle) + 2.0, map.price_to_y(price) + below);
                let modifiers = if ctrl {
                    Modifiers::CTRL
                } else {
                    Modifiers::NONE
                };
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                };
                let mut time = 1.0;
                let mut step = |chart: &mut crate::widget::Chart, event| {
                    time += 0.1;
                    frame(
                        &ctx,
                        chart,
                        time,
                        vec![Event::ModifiersChanged(modifiers), event],
                    );
                };
                if gesture == "place" {
                    chart
                        .drawings_mut()
                        .set_active_tool(Some(DrawingToolType::HorizontalLine));
                } else {
                    // A line at 95 has its anchor dragged; an icon at 103 its size corner.
                    let (tool, at, handle) = if gesture == "drag" {
                        (DrawingToolType::HorizontalLine, 95.0, HandlePos::Start)
                    } else {
                        (DrawingToolType::FontIcon, 103.0, HandlePos::BottomRight)
                    };
                    let mut drawing = Drawing::new(1, tool);
                    drawing.chart_points = vec![ChartPoint::new(middle as f32, at)];
                    chart.drawings_mut().drawings.push(drawing);
                    chart.drawings_mut().select(1);
                    frame(&ctx, &mut chart, 0.5, vec![]);
                    let from = chart
                        .drawings()
                        .get_handles(&chart.drawings().drawings[0])
                        .into_iter()
                        .find(|(at, _)| *at == handle)
                        .expect("the handle is shown")
                        .1;
                    step(&mut chart, Event::PointerMoved(from));
                    step(&mut chart, button(from, true));
                    step(&mut chart, Event::PointerMoved(from + vec2(7.0, 0.0)));
                }
                chart.drain_events().for_each(drop);
                step(&mut chart, Event::PointerMoved(pointer));
                let crosshair = chart
                    .drain_events()
                    .filter_map(|event| match event {
                        ChartEvent::CrosshairMoved(Some(pointer)) => Some(pointer),
                        _ => None,
                    })
                    .last()
                    .expect("the pointer moves the crosshair");
                assert_eq!(crosshair.series, SeriesId::MAIN, "{gesture} {case}");
                let painted = chart.last_crosshair.and_then(|(_, price)| price);
                if gesture == "place" {
                    step(&mut chart, button(pointer, true));
                }
                step(&mut chart, button(pointer, false));
                assert!(
                    chart.drawings().dragging_handle.is_none(),
                    "{gesture} {case}"
                );
                let anchor = chart.drawings().drawings[0].chart_points[0].price;
                observed.push((
                    gesture,
                    case,
                    round(crosshair.price),
                    painted.map(round),
                    round(anchor),
                ));
            }
        }
        // The level at 105 is 5.4 px from the near pointer, within the weak magnet's 20 px, and
        // four price units, far more than 20 px, from a pointer at 101. The strong magnet takes
        // the nearest of 90, 105 and 110. A dragged anchor lands where a click places one; the
        // icon keeps its anchor at 103 and the crosshair stays on the pointer at 101.
        let lands = [
            ("weak, far", 101.0),
            ("weak, near", 105.0),
            ("strong", 105.0),
            ("strong, Ctrl", 101.0),
            ("off, Ctrl", 105.0),
            ("indicators off", 110.0),
        ];
        let expected: Vec<_> = ["place", "drag"]
            .into_iter()
            .flat_map(|gesture| {
                lands.map(|(case, price)| (gesture, case, price, Some(price), price))
            })
            .chain([("resize", "strong", 101.0, Some(101.0), 103.0)])
            .collect();
        assert!(
            observed == expected,
            "(gesture, case, crosshair event price, painted crosshair price, anchor price) \
             observed {observed:?} expected {expected:?}"
        );
    }

    /// With the eraser on, a click on a drawing deletes it, whether or not a drawing is
    /// selected, unless it is locked, protected or Escape has turned the eraser off.
    #[test]
    fn an_eraser_click_deletes_the_drawing_unless_it_is_locked_or_protected() {
        for (tool, selected, locked, escaped, user_edit, kept) in [
            (
                DrawingToolType::HorizontalLine,
                false,
                false,
                false,
                true,
                0,
            ),
            (DrawingToolType::HorizontalLine, true, false, false, true, 0),
            (DrawingToolType::HorizontalLine, true, true, false, true, 1),
            (DrawingToolType::HorizontalLine, false, false, true, true, 1),
            (
                DrawingToolType::FibonacciExtension,
                false,
                false,
                false,
                true,
                0,
            ),
            (
                DrawingToolType::FibonacciExtension,
                true,
                false,
                false,
                true,
                0,
            ),
            (
                DrawingToolType::FibonacciExtension,
                true,
                true,
                false,
                true,
                1,
            ),
            (
                DrawingToolType::FibonacciExtension,
                false,
                false,
                true,
                true,
                1,
            ),
            (
                DrawingToolType::HorizontalLine,
                false,
                false,
                false,
                false,
                1,
            ),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            let mut line = Drawing::new(1, tool);
            line.chart_points = if tool == DrawingToolType::FibonacciExtension {
                let config = line.fib_config.as_mut().unwrap();
                for (index, level) in config.levels.iter_mut().enumerate() {
                    level.visible = index == 0;
                }
                if !selected {
                    config.levels[0].text = "Target".into();
                }
                vec![
                    ChartPoint::new(20.0, 12.0),
                    ChartPoint::new(50.0, 18.0),
                    ChartPoint::new(80.0, 15.0),
                ]
            } else {
                vec![ChartPoint::new(50.0, 15.0)]
            };
            line.locked = locked;
            line.user_edit_enabled = user_edit;
            chart.drawings_mut().drawings.push(line);
            if selected {
                chart.drawings_mut().select(1);
            }
            chart.drawings_mut().set_eraser_mode(true);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let points = &chart.drawings().drawings[0].points;
            let pos = if tool == DrawingToolType::FibonacciExtension {
                egui::pos2((points[1].x + points[2].x) * 0.5, points[2].y)
            } else {
                points[0]
            };
            let button = |pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            let escape = Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            };
            let frames = [
                if escaped { vec![escape] } else { vec![] },
                vec![Event::PointerMoved(pos)],
                vec![button(true)],
                vec![button(false)],
            ];
            for (i, events) in frames.into_iter().enumerate() {
                frame(&ctx, &mut chart, 0.1 * (i + 1) as f64, events);
            }
            let left = chart.drawings().drawings.len();
            assert_eq!(
                left, kept,
                "{tool:?}: selected {selected}, locked {locked}, escaped {escaped}, user edits {user_edit}"
            );
        }
    }

    /// A modified eraser sweep cuts only the crossed span, keeps style and sample anchors,
    /// saves on release, and restores the whole stroke with one undo step without panning.
    #[test]
    fn a_modified_eraser_drag_erases_only_the_part_of_a_stroke_it_crosses() {
        use crate::drawings::manager::DrawingSyncAction;
        use egui::pos2;

        for (tool, modifiers, locked, user_edit, crossed, pieces) in [
            (DrawingToolType::Brush, Modifiers::CTRL, false, true, 15, 2),
            (
                DrawingToolType::Highlighter,
                Modifiers::COMMAND,
                false,
                true,
                15,
                2,
            ),
            (DrawingToolType::Brush, Modifiers::CTRL, false, true, 0, 1),
            (DrawingToolType::Brush, Modifiers::CTRL, true, true, 15, 0),
            (DrawingToolType::Brush, Modifiers::NONE, false, true, 15, 0),
            (DrawingToolType::Brush, Modifiers::CTRL, false, false, 15, 0),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            let mut stroke = Drawing::new(1, tool);
            stroke.chart_points = (20..=80)
                .step_by(2)
                .map(|bar| {
                    let mut point = ChartPoint::new(bar as f32, 15.0);
                    point.time_anchor = Some((
                        chrono::DateTime::from_timestamp(1_700_000_000 + bar * 86400, 0).unwrap(),
                        bar as f32,
                    ));
                    point
                })
                .collect();
            stroke.locked = locked;
            stroke.user_edit_enabled = user_edit;
            stroke.stored_id = Some("stroke".into());
            stroke.created_at = Some(123);
            chart.drawings_mut().drawings.push(stroke.clone());
            chart.drawings_mut().set_eraser_mode(true);
            chart.drawings_mut().set_sync_enabled(true);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let samples = &chart.drawings().drawings[0].points;
            let px_per_bar = (samples[1].x - samples[0].x) / 2.0;
            let (x, y) = (samples[crossed].x, samples[0].y);
            let (p0, p1) = (pos2(x - 10.0, y - 40.0), pos2(x + 10.0, y + 40.0));
            // At height y, distance to this diagonal is |dx| * 80 / sqrt(20² + 80²).
            let reach = 5.0_f32.max(stroke.stroke_width / 2.0);
            let delta = reach * 20.0_f32.hypot(80.0) / 80.0 / px_per_bar;
            let center = stroke.chart_points[crossed].bar_idx;
            let button = |pos, pressed, modifiers| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers,
            };
            chart.drawings().pending_sync.lock().unwrap().clear();
            let view = chart.get_time_scale_state();
            let frames = [
                vec![Event::ModifiersChanged(modifiers), Event::PointerMoved(p0)],
                vec![button(p0, true, modifiers)],
                // The modifier belongs to the press; releasing it does not end the sweep.
                vec![
                    Event::ModifiersChanged(Modifiers::NONE),
                    Event::PointerMoved(p1),
                ],
                vec![button(p1, false, Modifiers::NONE)],
            ];
            for (i, events) in frames.into_iter().enumerate() {
                frame(&ctx, &mut chart, 0.1 * (i + 1) as f64, events);
                if modifiers != Modifiers::NONE {
                    assert_eq!(
                        chart.get_time_scale_state(),
                        view,
                        "{tool:?}, frame {i}: no pan"
                    );
                }
                if i < 3 {
                    assert!(chart.drawings().pending_sync.lock().unwrap().is_empty());
                }
                if i == 2 && pieces > 0 {
                    assert!(!chart.undo(), "history waits for the gesture to end");
                }
            }
            let drawings = &chart.drawings().drawings;
            if pieces == 0 {
                assert_eq!(
                    drawings.len(),
                    1,
                    "{tool:?}, {modifiers:?}, locked {locked}"
                );
                assert_eq!(drawings[0].chart_points, stroke.chart_points);
                assert!(!chart.drawings_mut().undo());
                continue;
            }
            assert_eq!(drawings.len(), pieces, "{tool:?}, sample {crossed}");
            assert_eq!(drawings[0].id, 1);
            assert_eq!(drawings[0].stored_id, stroke.stored_id);
            assert_eq!(drawings[0].created_at, stroke.created_at);
            for (i, piece) in drawings.iter().enumerate() {
                assert_eq!(piece.tool_type, stroke.tool_type);
                assert_eq!(piece.color, stroke.color);
                assert_eq!(piece.stroke_width, stroke.stroke_width);
                assert_eq!(piece.line_style, stroke.line_style);
                let kept: Vec<_> = stroke
                    .chart_points
                    .iter()
                    .filter(|p| {
                        if pieces == 2 && i == 0 {
                            p.bar_idx < center - delta
                        } else {
                            p.bar_idx > center + delta
                        }
                    })
                    .copied()
                    .collect();
                let (originals, cut) = if pieces == 2 && i == 0 {
                    (
                        &piece.chart_points[..piece.chart_points.len() - 1],
                        piece.chart_points.last().unwrap(),
                    )
                } else {
                    (&piece.chart_points[1..], &piece.chart_points[0])
                };
                assert_eq!(originals, kept);
                assert_eq!(
                    originals.iter().map(|p| p.time_anchor).collect::<Vec<_>>(),
                    kept.iter().map(|p| p.time_anchor).collect::<Vec<_>>()
                );
                let expected = if pieces == 2 && i == 0 {
                    center - delta
                } else {
                    center + delta
                };
                assert!(
                    (cut.bar_idx - expected).abs() < 1e-3,
                    "{tool:?}: {} vs {expected}",
                    cut.bar_idx
                );
                assert_eq!(cut.price, 15.0);
                if i > 0 {
                    assert_ne!(piece.id, 1);
                    assert!(piece.stored_id.is_none());
                    assert!(piece.created_at.is_none());
                }
            }
            let sync = chart.drawings().pending_sync.lock().unwrap();
            assert_eq!(sync.len(), pieces);
            assert!(matches!(sync[0], DrawingSyncAction::UpdateDrawing(1)));
            if pieces == 2 {
                assert!(
                    matches!(sync[1], DrawingSyncAction::SaveDrawing(id) if id == drawings[1].id)
                );
            }
            drop(sync);
            let after = drawings.clone();
            assert!(chart.undo());
            assert_eq!(chart.drawings().drawings.len(), 1);
            assert_eq!(chart.drawings().drawings[0].id, 1);
            assert_eq!(
                chart.drawings().drawings[0].chart_points,
                stroke.chart_points
            );
            assert!(!chart.undo(), "one undo step for the sweep");
            assert!(chart.redo());
            assert_eq!(
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| (d.id, &d.chart_points))
                    .collect::<Vec<_>>(),
                after
                    .iter()
                    .map(|d| (d.id, &d.chart_points))
                    .collect::<Vec<_>>()
            );
            let saved = chart
                .save_drawings()
                .expect("pieces save like ordinary strokes");
            assert!(saved.to_json().is_ok());
        }
    }

    /// A drawing shown only on the daily interval is neither drawn nor hit once the chart is
    /// set to another interval; price-based types retain interval visibility.
    #[test]
    fn a_hidden_drawing_is_neither_drawn_nor_hit() {
        let color = Color32::from_rgb(1, 2, 3);
        for (interval, kind, shown) in [
            (Timeframe::Day1, ChartType::Candles, true),
            (Timeframe::Hour1, ChartType::Candles, false),
            (Timeframe::Day1, ChartType::Renko, true),
        ] {
            let (ctx, mut chart) = (Context::default(), chart(&[10.0], &[20.0]));
            chart.set_timeframe(interval);
            let mut line =
                Drawing::with_color(1, DrawingToolType::HorizontalLine, color.to_array());
            line.chart_points = vec![ChartPoint::new(50.0, 15.0)];
            line.timeframe_visibility = TimeframeVisibility::specific(&["1D"]);
            chart.drawings_mut().drawings.push(line);
            chart.drawings_mut().select(1);
            chart.drawings_mut().options.handle_config.color = color.to_array();

            frame(&ctx, &mut chart, 0.0, vec![]);
            let on_line = chart.drawings().drawings[0].points[0];
            assert_eq!(
                chart.drawings().hit_test_handle_by_id(on_line, 1).is_some(),
                interval == Timeframe::Day1,
                "handle hit on {interval:?}"
            );
            chart.set_chart_type(kind);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]).iter().any(|s| {
                matches!(s, Shape::LineSegment { stroke, .. } if stroke.color == color)
                    || matches!(s, Shape::Circle(c) if c.fill == color)
                    || matches!(s, Shape::Mesh(m) if m.vertices.iter().any(|v| v.color == color))
            });
            frame(&ctx, &mut chart, 0.2, vec![Event::PointerMoved(on_line)]);
            for (time, pressed) in [(0.3, true), (0.4, false)] {
                frame(
                    &ctx,
                    &mut chart,
                    time,
                    vec![Event::PointerButton {
                        pos: on_line,
                        button: PointerButton::Secondary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    }],
                );
            }
            let hit = matches!(
                chart.take_right_click(),
                Some(RightClickTarget::Drawing { drawing_id: 1, .. })
            );
            assert_eq!((painted, hit), (shown, shown), "{interval:?} {kind:?}");
        }
    }

    /// A vertical line runs through the panes it does not belong to at its anchor's bar, without
    /// its text, and is hit and moved there. A move from such a pane is a move in time: the price
    /// stays on the scale of the line's own pane, whichever pane that is. Switched off, the line
    /// paints and hits only in its own pane. Every pane draws it in one colour: the tool's
    /// default in the shade the theme needs for contrast, a chosen colour as chosen.
    #[cfg(feature = "ui")]
    #[test]
    fn vertical_line_reaches_other_panes_without_leaving_its_own_scale() {
        use crate::drawings::{ChartPoint, DrawingToolType};
        use crate::studies::RSI;
        use egui::Pos2;

        const COLOR: [u8; 4] = [23, 57, 181, 255];
        // The tool's blue (41, 98, 255) has 3.65:1 on (19, 23, 34). Step 21 of 255 towards the
        // dark theme's text (219, 219, 219), (56, 108, 252), is the first with 4:1: 4.02:1, while
        // step 20, (55, 107, 252), has 3.98:1.
        let mut theme = crate::theme::Theme::dark();
        theme.chart.bg = Color32::from_rgb(19, 23, 34);
        theme.ui.panel_bg = theme.chart.bg;
        for (in_study, chosen) in [(false, true), (true, true), (false, false), (true, false)] {
            let ctx = Context::default();
            let mut chart = chart(&[90.0], &[110.0]);
            chart.set_theme(&ctx, &theme);
            let study = chart.add_indicator(Box::new(RSI::new(14))).0;
            let price = if in_study { 50.0 } else { 100.0 };
            let mut line = Drawing::new(1, DrawingToolType::VerticalLine);
            line.chart_points = vec![ChartPoint::new(60.0, price)];
            line.completed = true;
            if chosen {
                line.color = COLOR;
            }
            line.pane = in_study.then_some(study);
            line.text = Some("Weekly".into());
            chart.drawings_mut().drawings.push(line);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let main = chart.get_rendered_price_rect();
            let other = chart.get_rendered_indicator_panes()[0].chart_rect;
            let (own, foreign) = if in_study {
                (other, main)
            } else {
                (main, other)
            };
            let [r, g, b, _] = if chosen { COLOR } else { [56, 108, 252, 255] };
            let color = Color32::from_rgb(r, g, b);
            let paint = |chart: &mut crate::widget::Chart, time| {
                let shapes = shapes(&ctx, chart, time, vec![]);
                let texts: Vec<_> = shapes
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Text(t) if t.galley.text() == "Weekly" => Some(t.pos),
                        _ => None,
                    })
                    .collect();
                let lines: Vec<_> = shapes
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::LineSegment { points, stroke } if stroke.color == color => {
                            Some(*points)
                        }
                        _ => None,
                    })
                    .collect();
                // The own pane keeps last frame's tessellation, a mesh in the line's colour.
                let ys = crate::widget::test_frames::candle_ys(&shapes, color);
                (texts, lines, ys)
            };
            let (texts, lines, ys) = paint(&mut chart, 0.1);
            let x = chart.drawings().drawings[0].points[0].x;
            let row = format!("in study {in_study}, chosen {chosen}");
            assert!(
                lines.iter().all(|[a, b]| a.x == x && b.x == x),
                "{row}: {lines:?}"
            );
            assert!(
                lines
                    .iter()
                    .any(|[a, b]| a.y == foreign.top() && b.y == foreign.bottom()),
                "{row}: {lines:?}"
            );
            assert!(
                ys.iter().any(|y| own.y_range().contains(*y)),
                "{row}: {ys:?}"
            );
            assert_eq!(texts.len(), 1, "{row}");
            assert!(own.y_range().contains(texts[0].y), "{row}: {texts:?}");
            // The panes share one time axis, which tags the line whichever pane it belongs to.
            assert!(
                chart
                    .drawings()
                    .axis_tags(main, Color32::WHITE, &[], 0, None)
                    .any(|tag| matches!(
                        tag,
                        crate::drawings::manager::DrawingAxisTag::Time { x: tag_x, .. } if tag_x == x
                    )),
                "{row}"
            );
            let grab = Pos2::new(x, foreign.center().y);
            assert_eq!(chart.drawings().hit_test(grab), Some(1), "{row}");

            let view = chart.get_time_scale_state();
            let own_y = chart.drawings().drawings[0].points[0].y;
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            let to = grab + egui::vec2(40.0, 25.0);
            for (step, events) in [
                vec![Event::PointerMoved(grab)],
                vec![button(grab, true)],
                vec![Event::PointerMoved(grab + egui::vec2(20.0, 10.0))],
                vec![Event::PointerMoved(to)],
                vec![button(to, false)],
                vec![],
            ]
            .into_iter()
            .enumerate()
            {
                frame(&ctx, &mut chart, 0.2 + step as f64 * 0.1, events);
            }
            assert_eq!(chart.get_time_scale_state(), view, "{row}");
            let moved = chart.drawings().drawings[0].clone();
            assert!(moved.chart_points[0].bar_idx > 60.0, "{row}: {moved:?}");
            assert_eq!(moved.chart_points[0].price, price, "{row}");
            assert_eq!(moved.points[0].y, own_y, "{row}");
            assert!(chart.undo(), "{row}");
            assert_eq!(
                chart.drawings().drawings[0].chart_points[0].bar_idx,
                60.0,
                "{row}"
            );

            chart.drawings_mut().drawings[0]
                .line_measurements
                .across_panes = false;
            let (_, lines, _) = paint(&mut chart, 1.0);
            assert!(
                lines.iter().all(|[a, _]| own.y_range().contains(a.y)),
                "{row}: {lines:?}"
            );
            assert_eq!(chart.drawings().hit_test(grab), None, "{row}");
        }
    }

    /// An RSI pane is in RSI points, not in the instrument's quarter-point ticks: an info line
    /// from 40 to 50 on it rises 10 points, which its readouts must not count as 40 ticks. The
    /// original price pane the main series left empty has no price series either; it paints the
    /// drawings that name it by its id, 0, which is also the main series' id. With a host series
    /// left in it, that pane is a price pane, where a line from 100 to 110 rises 40 ticks.
    #[cfg(feature = "ui")]
    #[test]
    fn study_pane_readouts_leave_out_the_instrument_ticks() {
        use crate::studies::RSI;
        use crate::widget::{PaneId, SeriesId, SeriesOptions};
        for (moved, kept) in [(false, false), (true, false), (true, true)] {
            let row = format!("main series moved {moved}, host series kept {kept}");
            let ctx = Context::default();
            let data = chart(&[90.0], &[95.0, 115.0]).bars(SeriesId::MAIN).to_vec();
            let mut chart = chart(&[90.0], &[110.0]);
            chart.set_instrument(Some(
                crate::model::Instrument::new("TEST").with_tick_size(0.25),
            ));
            let owner = if moved {
                chart.config_mut().show_volume = false;
                let pane = chart.add_pane();
                assert!(chart.move_series_to_pane(SeriesId::MAIN, pane));
                if kept {
                    let series = chart.add_series(SeriesOptions::default());
                    chart.set_data(series, data);
                    series.0
                } else {
                    // The emptied pane keeps its scale's default range, 0 to 100.
                    PaneId::MAIN.0
                }
            } else {
                chart.add_indicator(Box::new(RSI::new(14))).0
            };
            let from = if kept { 100.0 } else { 40.0 };
            let mut line = Drawing::new(1, DrawingToolType::InfoLine);
            line.chart_points = vec![
                ChartPoint::new(60.0, from),
                ChartPoint::new(90.0, from + 10.0),
            ];
            line.completed = true;
            line.pane = Some(owner);
            chart.drawings_mut().drawings.push(line);
            let readouts: Vec<String> = [0.0, 0.1]
                .into_iter()
                .flat_map(|time| shapes(&ctx, &mut chart, time, vec![]))
                .filter_map(|shape| match shape {
                    Shape::Text(text) if text.galley.text().contains("bars") => {
                        Some(text.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect();
            assert!(!readouts.is_empty(), "{row}: no readouts painted");
            assert!(
                readouts.iter().all(|text| if kept {
                    text.contains("+40 ticks")
                } else {
                    !text.contains("tick")
                }),
                "{row}: {readouts:?}"
            );
            if kept {
                continue;
            }
            // The card the selection toolbar keeps clear of is the painted one, without ticks too.
            let card = |chart: &crate::widget::Chart| {
                let mut card = None;
                ctx.run_ui(Default::default(), |ui| {
                    card = chart.line_measurement_card_rect(ui, &chart.drawings().drawings[0]);
                })
                .drop_without_applying_deltas();
                card
            };
            let with_instrument = card(&chart);
            chart.set_instrument(None);
            assert!(with_instrument.is_some(), "{row}");
            assert_eq!(with_instrument, card(&chart), "{row}");
        }
    }
}
