//! Frame rendering: the `show` entry points and the per-frame paint and interaction pass.

mod background;
mod sessions;

#[cfg(feature = "studies")]
pub(super) use sessions::session_ranges;

use super::selection::hit_test_main_chart;
use super::{Chart, PrimitivePass, RightClickTarget, primitives};
use crate::chart::overlays::FootprintConfig;
use crate::chart::rendering::{self, ChartMapping, RenderContext, StyleColors};
use crate::data::Footprint;
use crate::localization as l10n;
use crate::model::{AutoscaleMargins, ChartType, MarkerZOrder, PriceLine};
use crate::theme::ThemeExt;
use egui::{Pos2, Rect, Response, Sense, Ui, Vec2};

impl Chart {
    /// Prepares the main-series conflation groups for `bar_spacing` in logical points and
    /// `pixels_per_point` (the egui context's display scale), returning the bars per group.
    /// Call after supplying data and configuring
    /// [`conflation_threshold`](crate::config::ChartConfig::conflation_threshold), before showing
    /// a dense view at that spacing. Rendering also prepares lazily, using the same cache.
    ///
    /// Pan and hover reuse the groups. A changed grouping factor, price source or historical
    /// revision rebuilds them; appends and tail updates rebuild only the last existing group
    /// and any new groups. Only one factor is retained at a time. This reads the currently
    /// applied bars, including a replay cutoff; it does not poll a connected feed.
    /// Disabled conflation, numeric scales and price-based chart types return 1 and clear the groups.
    pub fn prepare_conflation(&mut self, bar_spacing: f32, pixels_per_point: f32) -> usize {
        let factor = if self.chart_type.is_time_independent()
            || self.state.time_scale().numeric_positions().is_some()
        {
            1
        } else {
            super::merge::factor(
                bar_spacing,
                pixels_per_point,
                self.config.conflation_threshold,
            )
        };
        self.merge_groups.update(
            &self.state.data().bars,
            self.state.revision(),
            factor,
            self.config.price_source,
        );
        factor
    }

    /// Renders the chart with its studies and drawings, and handles pan, zoom,
    /// crosshair, keyboard shortcuts, the drawing tools and all visual elements
    /// configured in [`ChartConfig`](crate::config::ChartConfig).
    ///
    /// Overlay studies (moving averages, Bollinger Bands) are drawn on the main
    /// pane, any other visible study (RSI, MACD) in a pane below it with its
    /// x-axis aligned to the main pane; [`Chart::get_rendered_indicator_panes`]
    /// returns their layout. A chart built with a feed first asks it for what it needs and
    /// applies what it sent. Studies are then brought up to date: an incremental study at once,
    /// and at most one study per frame in full, the one that has waited longest.
    ///
    /// On price-based types the time scale and reported indices count elements, one OHLC bar
    /// per brick, line, column or range bar. Each carries the last contributing bar's time.
    /// Studies use element OHLC on price-based types; time annotations follow element slots.
    /// Source-bar study results remain available through [`Self::indicators`].
    ///
    /// Reports the series summary and hovered bar values to accessibility clients. Native
    /// applications using `eframe` enable its `accesskit` feature to connect to screen readers.
    ///
    /// Returns an [`egui::Response`] for additional interaction handling.
    pub fn show(&mut self, ui: &mut Ui) -> Response {
        self.show_content(ui, None)
    }

    /// Keeps a cropped distribution's session and central levels readable at its visible edge.
    fn paint_profile_context(
        &self,
        ui: &Ui,
        mapping: &ChartMapping,
        span: std::ops::Range<f32>,
        session: chrono::DateTime<chrono::Utc>,
        levels: &[(&str, f64)],
        top: f32,
    ) {
        let left = span.start.max(mapping.rect.left());
        let right = span.end.min(mapping.rect.right());
        if right <= left {
            return;
        }
        let theme = ui.active_theme();
        let mut text = self.format_time(session);
        for &(name, price) in levels {
            let y = mapping.price_to_y(price);
            let direction = if y < mapping.rect.top() {
                "above"
            } else if y > mapping.rect.bottom() {
                "below"
            } else {
                ""
            };
            let formatter =
                self.series_label_formatter(super::SeriesId::MAIN, mapping.price.transform().mode);
            let value = mapping.price.label(
                mapping.price.transform().to_scale(price),
                rendering::axes::tag_decimals(&mapping.price, y),
                formatter.as_deref(),
            );
            text.push_str(&format!("\n{name} {value}"));
            if !direction.is_empty() {
                text.push_str(&format!(" ({})", l10n::text(ui.ctx(), direction)));
            }
        }
        let target = Rect::from_x_y_ranges(left..=right, top..=mapping.rect.bottom())
            .intersect(mapping.rect);
        // Painted rows can own another tooltip, so use the same hit test as the rows.
        if ui.ctx().pointer_hover_pos().is_some_and(|pos| {
            target.contains(pos) && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
        }) {
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                ui.scope_id().with(("profile_context", session)),
                target,
            )
            .show(|ui| {
                ui.label(&text);
            });
        }
        let painter = ui.painter().with_clip_rect(mapping.rect);
        if right - left < 40.0 {
            painter.with_clip_rect(target).text(
                Pos2::new((left + right) * 0.5, top),
                egui::Align2::CENTER_TOP,
                "i",
                egui::FontId::proportional(theme.text.label),
                theme.chart.axis_text,
            );
            return;
        }
        let galley = painter.layout(
            text,
            egui::FontId::proportional(theme.text.label),
            theme.chart.axis_text,
            (right - left - 12.0).max(1.0),
        );
        let pos = Pos2::new(right - galley.size().x - 6.0, top);
        painter.rect_filled(
            Rect::from_min_size(pos, galley.size()).expand(3.0),
            2.0,
            theme.chart.bg,
        );
        painter.galley(pos, galley, theme.chart.axis_text);
    }

    /// Draws a footprint in place of the price series, below studies, cursor and legend.
    /// Supply executions aggregated with the same clock as the chart's source bars.
    /// Bars without complete executions are drawn as candles. While cells would be too narrow to
    /// read, every bar is, merged per pixel column when zoomed out.
    /// Price-based chart types keep their normal series because their x-axis is not time.
    /// `config.studies` enables execution annotations with the `studies` feature;
    /// `config.statistics` reserves a per-bar table below the price pane.
    pub fn show_footprint(
        &mut self,
        ui: &mut Ui,
        flow: &Footprint,
        config: &FootprintConfig,
    ) -> Response {
        self.show_content(ui, Some((flow, config)))
    }

    fn show_content(
        &mut self,
        ui: &mut Ui,
        footprint: Option<(&Footprint, &FootprintConfig)>,
    ) -> Response {
        let footprint = footprint.filter(|_| self.replay.is_none());
        let theme = self.theme_override.clone();
        // Axis labels take their size from the theme's label text.
        let scaled = self.config.axis_font_size.map(|size| {
            let mut scaled = theme.clone().unwrap_or_else(|| ui.ctx().active_theme());
            scaled.text.label = crate::styles::typography::font_size(size);
            scaled
        });
        let _theme_scope = scaled
            .as_ref()
            .or(theme.as_ref())
            .map(|theme| crate::theme::ThemeScope::new(ui.ctx(), theme));
        let _font_scope =
            rendering::axes::AxisFontScope::new(ui.ctx(), &self.config.axis_font_family);
        let size = self.size;
        let show = |ui: &mut Ui| {
            if let Some(theme) = &theme {
                crate::theme::apply_to_style(ui.style_mut(), theme);
            }
            self.show_themed_content(ui, footprint)
        };
        // Keep the same UI scope when an override is cleared, preserving focus and widget state.
        match size {
            Some(size) => ui.allocate_ui(size, show).inner,
            None => ui.scope(show).inner,
        }
    }

    fn show_themed_content(
        &mut self,
        ui: &mut Ui,
        footprint: Option<(&Footprint, &FootprintConfig)>,
    ) -> Response {
        self.follow_chart_theme(&ui.active_theme());
        self.host_events.volume_map = None;
        self.host_events.target = None;
        self.host_events.primitive_hit = None;
        self.cover_annotation_hit();
        self.apply_config_scale_mode();
        #[cfg(feature = "studies")]
        self.sync_volume_study(ui.ctx());
        #[cfg(feature = "drawings")]
        self.sync_stored_drawings(ui.ctx());
        self.pump(ui.ctx());
        if let Some(wait) = self.refresh_studies(ui.input(|i| i.time)) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
        self.show_elements();
        // A crosshair set by time follows its bar when bars are evicted or loaded in front.
        if let Some(period) = self.synced_crosshair_period {
            self.set_synced_crosshair_for_series(
                self.synced_crosshair_series,
                Some(period),
                self.synced_crosshair_price,
            );
            if self.synced_crosshair_bar_idx.is_none() {
                self.last_crosshair = None;
            }
        }
        if let Some(wait) = self.refresh_element_studies(ui.input(|i| i.time)) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
        self.sync_primitives(ui.ctx());
        let frame = ui.ctx().cumulative_frame_nr();
        let width = ui.available_width();
        let resized = self
            .last_layout
            .is_none_or(|(previous, changed)| previous != width || changed == frame);
        let previous_axis_width = self.measured_axis_width;
        self.last_rendered_indicator_panes.clear();
        self.sync_panes();
        for pane in &mut self.pane_layout.panes {
            pane.rect = Rect::NOTHING;
        }
        // Panes share the available height and the lowest time axis.
        let has_panes = self.has_panes();
        let response = if has_panes {
            self.show_with_panes(ui, footprint)
        } else {
            let response = self.show_internal(ui, None, None, footprint);
            if let Some(main) = self
                .pane_layout
                .panes
                .iter_mut()
                .find(|pane| pane.id == super::PaneId::MAIN)
            {
                main.rect = response.rect;
            }
            response
        };
        self.menu_open = false;
        if !self.chart_type.is_time_independent() {
            let source_marks = self.mark_source.as_ref().map(|source| &source.page);
            let mut coords = self.get_chart_mapping();
            coords.start_idx = 0;
            let baseline = if has_panes {
                response.rect.bottom() - self.time_axis_height().min(response.rect.height())
            } else {
                coords.rect.bottom()
            };
            coords.rect.max.y = baseline;
            // Source pages cover the view; host lists are windowed before cloning or formatting.
            // Keys keep each badge's interaction stable as the view moves.
            let (from, to) = super::marks::window(self.state.shown(), &coords, coords.rect);
            let mut events: Vec<_> = self
                .events
                .within(from, to)
                .into_iter()
                .map(|index| (index, self.events[index].clone()))
                .collect();
            let mut key = self.events.len();
            if let Some(source) = source_marks {
                events.extend(
                    source
                        .time_scale_marks
                        .iter()
                        .cloned()
                        .enumerate()
                        .map(|(index, event)| (key + index, event)),
                );
                key += source.time_scale_marks.len();
            }
            let typed_count = self.time_scale_events.len();
            let typed = self
                .time_scale_events
                .within(from, to)
                .into_iter()
                .map(|index| (key + index, &self.time_scale_events[index]))
                .chain(source_marks.into_iter().flat_map(|page| {
                    page.events
                        .iter()
                        .enumerate()
                        .map(move |(index, event)| (key + typed_count + index, event))
                }));
            let mut breaks = Vec::new();
            for (key, event) in typed {
                if event.kind == crate::model::EventKind::Earnings
                    && self.event_visibility.show_earnings_breaks
                {
                    breaks.push(event.time);
                }
                if self.event_visibility.shows(event.kind) {
                    events.push((
                        key,
                        crate::model::Marker::event(event.time, event.kind.label())
                            .with_id(&event.id)
                            .with_tooltip(format!("{}\n{}", event.title, event.details)),
                    ));
                }
            }
            rendering::render_event_breaks(ui, &breaks, self.state.shown(), &coords);
            if let Some(id) = rendering::render_event_badges(
                ui,
                events.iter().map(|(key, event)| (*key, event)),
                self.state.shown(),
                &coords,
                baseline,
            ) {
                self.event_click = Some(id);
            }
        }
        if self.last_rendered_price_rect.width() > 1.0
            && self
                .last_layout
                .is_none_or(|(previous, _)| previous != width)
        {
            self.last_layout = Some((width, frame));
        }
        if (self.measured_axis_width - previous_axis_width).abs() > 0.5 {
            let old_width = self.last_rendered_price_rect.width();
            let new_width = self
                .calculate_layout_rects(response.rect)
                .price_rect
                .width();
            // Sizing passes follow the same zoom policy as the viewport resize.
            if (!resized
                || self.desired_visible_bars.is_some_and(|count| count > 0)
                || self
                    .chart_options
                    .time_scale
                    .lock_visible_time_range_on_resize)
                && previous_axis_width > 0.0
                && old_width > 0.0
                && new_width > 0.0
            {
                let scale = self.state.time_scale_mut();
                scale.set_bar_spacing(scale.bar_spacing() * new_width / old_width);
            }
            ui.ctx().request_discard("Chart measured its axis labels");
        }
        self.record_host_events(ui);
        let needs_info = response.clicked()
            || response.double_clicked()
            || response.triple_clicked()
            || response.gained_focus()
            || response.changed()
            || ui
                .ctx()
                .accesskit_node_builder(response.id, |_| ())
                .is_some();
        #[cfg(debug_assertions)]
        let needs_info = needs_info || ui.ctx().global_style().debug.show_interactive_widgets;
        if needs_info {
            // Translation callbacks must run outside the context locks held by widget_info.
            let info = self.accessibility_info(ui.ctx(), response.enabled());
            response.widget_info(|| info.clone());
        }
        // Within the frame of the edit, in case it is the last.
        #[cfg(feature = "drawings")]
        self.save_stored_drawings();
        response
    }

    /// Pulses at the last price of a line-based chart as its `last_price_animation` asks, and
    /// keeps repainting while a visible pulse runs.
    fn paint_last_price_pulse(
        &self,
        painter: &egui::Painter,
        price_rect: Rect,
        coords: &ChartMapping,
    ) {
        use crate::model::LastPriceAnimation;
        let styles = &self.chart_type_params.styles;
        let animation = styles.last_price_animation;
        let Some(index) = self
            .state
            .shown()
            .iter()
            .rposition(|bar| !bar.is_whitespace())
        else {
            return;
        };
        let last = &self.state.shown()[index];
        let price = self
            .config
            .price_source
            .compute(last.open, last.high, last.low, last.close);
        let Some(color) = rendering::candles::line_color_at(
            self.chart_type,
            styles,
            self.chart_type_params.baseline,
            &coords.price,
            price_rect,
            price,
            last,
        )
        .filter(|_| animation != LastPriceAnimation::Disabled) else {
            return;
        };
        super::series::paint_last_price_pulse(
            painter,
            price_rect,
            Pos2::new(coords.idx_to_x(index), coords.price.price_to_y(price)),
            color,
            animation,
            (last.time, price),
            &self.last_price_pulse,
        );
    }

    /// Internal rendering method that orchestrates all modules
    pub(crate) fn show_internal(
        &mut self,
        ui: &mut Ui,
        shared_time_axis: Option<Rect>,
        shared_plot: Option<&Response>,
        footprint: Option<(&Footprint, &FootprintConfig)>,
    ) -> Response {
        let primary_side = self.primary_axis_side();
        let show_left_axis = primary_side == Some(crate::model::AxisSide::Left);
        let show_right_axis = primary_side == Some(crate::model::AxisSide::Right);
        let main_pane = self.main_series_pane();
        let order = self.main_series_order().to_vec();
        // Register the icon loader once per context, and the general SVG loader for UI images.
        #[cfg(feature = "icons")]
        {
            let ctx = ui.ctx().clone();
            let loaders_installed = egui::Id::unique("egui_charts::image_loaders_installed");
            if !ctx.data(|d| d.get_temp::<bool>(loaders_installed).unwrap_or(false)) {
                #[cfg(feature = "ui")]
                egui_extras::install_image_loaders(&ctx);
                crate::icons::install_loader(&ctx);
                ctx.data_mut(|d| d.insert_temp(loaders_installed, true));
            }
        }

        // Reset zoom_just_applied flag at the start of each frame
        self.zoom_just_applied = false;

        // A right-click result lives for exactly one frame: clear any stale
        // capture so a target that is never drained does not reappear later.
        self.right_click = None;

        let available_size = ui.available_size();
        let (mut response, mut painter) =
            ui.allocate_painter(available_size, Sense::click_and_drag());
        let rect = response.rect;

        if let Some(axis) = shared_time_axis {
            painter.set_clip_rect(ui.clip_rect().intersect(if rect.height() == 0.0 {
                axis
            } else {
                rect.union(axis)
            }));
            response = response.union(ui.interact(
                axis,
                response.id.with("shared_time_axis"),
                Sense::click_and_drag(),
            ));
        }
        let mut layout = self.pane_layout_rects(
            rect,
            self.config.show_time_labels && shared_time_axis.is_none(),
        );
        if rect.height() == 0.0
            && let Some(axis) = shared_time_axis
        {
            layout.price_rect = Rect::from_x_y_ranges(axis.x_range(), rect.y_range());
            layout.volume_rect = Rect::NOTHING;
            layout.legend_rect = Rect::NOTHING;
        }
        let time_axis_top = shared_time_axis.map_or(layout.price_rect.bottom(), |axis| axis.top());
        let statistics_rows = if let Some((_, config)) = footprint {
            config.statistics.then(|| config.statistics_rows.clone())
        } else {
            (self.chart_type == ChartType::VolumeFootprint
                && self.chart_type_params.footprint_statistics)
                .then(|| self.chart_type_params.footprint_statistics_rows.clone())
        }
        .filter(|rows| !rows.is_empty());
        let statistics_rect = statistics_rows
            .as_deref()
            .filter(|_| !self.chart_type.is_time_independent() && self.main_series_visible)
            .map(|rows| {
                let bottom = layout.price_rect.bottom();
                let height =
                    (rows.len() as f32 * 12.0 + 4.0).min(layout.price_rect.height().max(0.0) * 0.4);
                layout.price_rect.max.y -= height;
                if self.shows_volume() {
                    layout.volume_rect =
                        super::volume::overlay_rect(layout.price_rect, &self.config);
                }
                Rect::from_min_max(
                    Pos2::new(layout.price_rect.left(), layout.price_rect.bottom()),
                    Pos2::new(layout.price_rect.right(), bottom),
                )
            });
        let chart_rect = layout.price_rect;

        self.handle_secondary_axes(ui, rect, chart_rect);
        let chart_width = chart_rect.width();

        // CRITICAL: Apply TimeScale width configuration BEFORE any zoom operations
        // This ensures apply_constraints() inside zoom() uses the correct self.width
        // to calculate constraint bounds. Without this, drawings drift during zoom
        // because constraints are calculated with stale width values.
        self.apply_timescale_config(chart_width);
        self.advance_scroll_animation(ui);

        let navigation_rect = self.navigation_rect(ui, chart_rect);
        let navigation = navigation_rect
            .filter(|_| self.navigation_visible(ui, chart_rect))
            .map(|nav| {
                (
                    nav,
                    self.interact_navigation(ui, nav, chart_rect, response.id),
                )
            });
        let scale = self.state.time_scale();
        let mut live_scale = scale.clone();
        live_scale.scroll_to_realtime();
        let realtime_rect = (self.config.show_realtime_btn
            && !self.state.data().is_empty()
            && (scale.right_offset_precise() - live_scale.right_offset_precise()).abs() > 1e-6)
            .then(|| {
                rendering::realtime_button_rect(
                    chart_rect,
                    self.config.realtime_button_size,
                    self.config.realtime_button_pos,
                    navigation_rect.filter(|_| {
                        self.config.navigation_buttons != crate::config::NavigationButtons::Never
                    }),
                )
            })
            .flatten();
        let realtime_interaction = realtime_rect.is_some_and(|button| {
            ui.input(|input| {
                (input.pointer.any_down() || input.pointer.any_released())
                    && input
                        .pointer
                        .press_origin()
                        .or_else(|| input.pointer.hover_pos())
                        .is_some_and(|pos| button.contains(pos))
            })
        });
        let navigation_interaction = navigation_rect.is_some_and(|nav| {
            self.config.navigation_buttons != crate::config::NavigationButtons::Never
                && !self.state.data().is_empty()
                && ui.input(|i| {
                    (i.pointer.any_down() || i.pointer.any_released())
                        && i.pointer
                            .press_origin()
                            .or_else(|| i.pointer.hover_pos())
                            .is_some_and(|p| nav.contains(p))
                })
        });
        // A button owns the whole gesture, including a drag that leaves the row. The chart's
        // own response goes back to the caller; the gesture handlers see a hover-only one.
        let mut shown =
            shared_plot.map_or_else(|| response.clone(), |plot| response.union(plot.clone()));
        if self.pane_collapse_click(ui.ctx()) {
            response.flags.remove(egui::response::Flags::CLICKED);
            shown.flags.remove(egui::response::Flags::CLICKED);
        }
        let axis_corner = Rect::from_min_max(
            Pos2::new(
                if show_right_axis {
                    chart_rect.right()
                } else {
                    rect.left()
                },
                time_axis_top,
            ),
            Pos2::new(
                if show_right_axis {
                    rect.right()
                } else {
                    chart_rect.left()
                },
                time_axis_top + crate::styles::sizing::chart::BOTTOM_PADDING_WITH_TIME,
            ),
        );
        #[cfg(feature = "drawings")]
        let eraser_mode = self.drawings.eraser_mode;
        #[cfg(not(feature = "drawings"))]
        let eraser_mode = false;
        #[cfg(feature = "drawings")]
        let drawing_tool_active = self.drawings.active_tool.is_some();
        #[cfg(not(feature = "drawings"))]
        let drawing_tool_active = false;
        #[cfg(not(feature = "drawings"))]
        let is_drawing_interaction = false;
        // The Magic cursor marks while the button is held, so a drag paints rather than pans.
        #[cfg(feature = "drawings")]
        let is_drawing_interaction = self.drawings.box_selection_input(ui)
            || self.drawings.active_tool.is_some()
            || self.chart_options.crosshair.style == crate::config::CrosshairStyle::Magic
            || self.drawings.dragging_handle.is_some()
            || self.drawings.text_editor.is_some()
            || self.drawings.curr_drawing.is_some()
            || ui
                .input(|i| i.pointer.press_origin().or_else(|| i.pointer.hover_pos()))
                .is_some_and(|origin| {
                    self.drawings
                        .selected_ids()
                        .any(|id| self.drawings.hit_test_handle_by_id(origin, id).is_some())
                        || self.drawings.hit_test(origin).is_some_and(|id| {
                            self.drawings
                                .drawings
                                .iter()
                                .any(|d| d.id == id && !d.locked)
                        })
                });
        let show_price_button =
            self.config.show_plus_button && !eraser_mode && !is_drawing_interaction;
        let show_axis_settings = !self.layout_inactive || ui.rect_contains_pointer(rect);
        let axis_interaction = cfg!(feature = "icons")
            && (show_right_axis || show_left_axis)
            && ui.input(|input| {
                (input.pointer.any_down() || input.pointer.any_released())
                    && input
                        .pointer
                        .press_origin()
                        .or(input.pointer.hover_pos())
                        .is_some_and(|pos| {
                            (show_axis_settings && axis_corner.contains(pos))
                                || (show_price_button
                                    && self.axis_hover.is_some_and(|(_, tag)| {
                                        rendering::axes::crosshair_price_button_rect(tag)
                                            .contains(pos)
                                    }))
                        })
            });
        let hover_rect = shared_plot.map_or_else(
            || {
                Rect::from_min_max(
                    Pos2::new(
                        chart_rect.left(),
                        self.pane_layout
                            .panes
                            .iter()
                            .map(|pane| pane.rect.top())
                            .fold(chart_rect.top(), f32::min),
                    ),
                    Pos2::new(chart_rect.right(), time_axis_top),
                )
            },
            |plot| plot.rect,
        );
        let touch_readout_interaction = self.handle_touch_crosshair(
            ui,
            shown.id,
            hover_rect,
            is_drawing_interaction
                || eraser_mode
                || navigation_interaction
                || realtime_interaction
                || axis_interaction
                || self.menu_open
                || egui::Popup::is_any_open(ui.ctx())
                || self.state.data().is_empty(),
        );
        let touch_readout = super::interaction::touch_crosshair(ui, shown.id);
        if self.chart_options.tooltip.on_long_press
            && self.chart_options.tooltip.enabled
            && touch_readout.touch_input
            && !is_drawing_interaction
            && !eraser_mode
        {
            // The readout owns a touch hold, including when its delay differs from egui's.
            response.flags.remove(egui::response::Flags::LONG_TOUCHED);
            shown.flags.remove(egui::response::Flags::LONG_TOUCHED);
        }
        if navigation_interaction || realtime_interaction || axis_interaction {
            response = ui.interact(
                Rect::NOTHING,
                response.id.with("navigation_input"),
                Sense::hover(),
            );
            response.rect = rect;
        }

        // The visible pane drives time navigation even while the price pane has no height.
        let mut time_response =
            shared_plot.map_or_else(|| response.clone(), |plot| response.union(plot.clone()));
        // Handle tracking mode
        self.handle_tracking_mode(ui, &time_response);

        // Request focus on hover for keyboard shortcuts
        self.request_focus_if_needed(&mut time_response);

        // Handle keyboard shortcuts
        self.handle_keyboard_shortcuts(ui, &time_response, chart_width, chart_rect.min.x);

        // Taken after the shortcuts: a key can remove a study and shift the ones after it.
        let study_indices: Vec<_> = self
            .study_ids
            .iter()
            .enumerate()
            .filter(|(_, id)| self.series_pane(**id) == Some(main_pane))
            .map(|(idx, _)| idx)
            .collect();

        // Calculate visible bars
        let logical_range = self.state.time_scale().visible_logical_range();
        let visible_bars = logical_range.length().ceil() as usize;
        self.last_visible_bars = visible_bars;

        // Show grabbing cursor during panning
        self.set_panning_cursor(ui, &time_response);

        // Define axis rects
        let price_axis_rect = self
            .pane_scale_axes(main_pane, rect, chart_rect)
            .find(|(key, _)| *key == super::PaneScale::Named(self.main_series_scale))
            .map_or(Rect::NOTHING, |(_, axis)| {
                Rect::from_x_y_ranges(axis.x_range(), chart_rect.y_range())
            });
        let time_axis_rect = shared_time_axis.unwrap_or_else(|| {
            Rect::from_min_max(
                Pos2::new(chart_rect.min.x, chart_rect.max.y),
                Pos2::new(chart_rect.max.x, rect.max.y),
            )
        });
        // Below any footprint statistics rows, which the interaction rect above includes.
        self.time_axis_rect = Rect::from_min_size(
            Pos2::new(time_axis_rect.left(), time_axis_top),
            Vec2::new(
                time_axis_rect.width(),
                self.time_axis_height()
                    .min(time_axis_rect.bottom() - time_axis_top),
            ),
        );

        // Handle double-click to reset axes
        self.handle_double_click(&time_response, price_axis_rect, time_axis_rect);

        // Handle mouse wheel for zoom/scroll
        // Block pan/zoom when drawing tool is active OR when manipulating a drawing
        let time_based = !self.chart_type.is_time_independent();
        let show_volume = self.shows_volume();
        let pending_price_zoom = self.handle_mouse_wheel(
            ui,
            &time_response,
            chart_width,
            chart_rect.min.x,
            price_axis_rect,
        );

        // Handle pinch-to-zoom for touch/trackpad gestures
        self.handle_pinch_zoom(ui, &time_response, chart_width, chart_rect.min.x);

        // Handle drag to pan (blocked when interacting with drawings)
        self.handle_drag_pan(
            ui,
            &time_response,
            price_axis_rect,
            time_axis_rect,
            chart_rect.min.x,
            is_drawing_interaction || touch_readout_interaction,
        );

        // Apply kinetic scrolling
        self.apply_kinetic_scroll(ui);

        // Handle box zoom (only when zoom mode is active from toolbar)
        // Right-click is reserved for context menu, zoom uses left-click when mode is active
        self.zoom_just_applied = self.handle_box_zoom(
            ui,
            &response,
            chart_rect,
            chart_width,
            self.zoom_mode_active,
        );
        if self.zoom_just_applied {
            log::info!("Zoom applied - chart will auto-deactivate zoom mode");
        }

        // Set zoom-in cursor when zoom mode is active
        if self.zoom_mode_active && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomIn);
        }
        // Set cursor icon based on crosshair style (cursor type)
        // This is only for default case - drawing manager may override for eraser mode
        else if response.hovered() {
            use crate::config::CrosshairStyle;
            match self.chart_options.crosshair.style {
                CrosshairStyle::Full => {
                    // Cross cursor mode - show crosshair cursor
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                CrosshairStyle::Dot => {
                    // Dot mode - default pointer (dot is rendered on chart)
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
                CrosshairStyle::Arrow
                | CrosshairStyle::Demonstration
                | CrosshairStyle::Magic
                | CrosshairStyle::Hidden => {
                    // Arrow mode - default pointer
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
            }
        }

        // Draw background (solid or gradient)
        if rect.height() == 0.0
            && let Some(axis) = shared_time_axis
        {
            self.draw_background(
                ui.painter(),
                Rect::from_min_max(rect.min, Pos2::new(rect.right(), axis.bottom())),
            );
        }
        self.draw_background(&painter, rect);
        if let Some(axis) = shared_time_axis {
            self.draw_background(
                &painter,
                Rect::from_min_max(
                    Pos2::new(rect.left(), axis.top()),
                    Pos2::new(rect.right(), axis.bottom()),
                ),
            );
        }

        self.pump_signals(ui.ctx());
        self.pump_targets(ui.ctx());
        #[cfg(feature = "drawings")]
        self.pump_posts(ui.ctx());

        // Draw watermark overlay (if enabled)
        self.draw_watermark(ui, &painter, chart_rect);
        if let Some(watermark) = self
            .pane_layout
            .panes
            .iter()
            .find(|pane| pane.id == main_pane)
            .and_then(|pane| pane.watermark.as_ref())
        {
            rendering::render_watermark(ui, &painter, chart_rect, watermark);
        }

        if self.state.data().is_empty() && self.state.shown().is_empty() {
            self.last_hover_bar_idx = None;
            self.last_crosshair = None;
            self.last_rendered_price_rect = layout.price_rect;
            self.last_rendered_volume_rect = Rect::NOTHING;
            let scale = self.state.price_scale();
            self.last_price_map = scale
                .map(scale.transform(), chart_rect.top(), chart_rect.bottom())
                .with_thousands_sep(self.config.thousands_sep);
            self.pump_marks(ui.ctx());
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                l10n::text(ui.ctx(), "No data available"),
                egui::FontId::proportional(ui.active_theme().text.title),
                self.config.text_color,
            );
            self.paint_empty_pane_primitives(ui, &painter);
            // This frame paints no marks, and those of the last frame went with its bars.
            self.mark_hits.get_mut().clear();
            self.clickable_marks.clear();
            self.record_host_click(ui, &response);
            return response;
        }

        let price_rect = layout.price_rect;
        let volume_rect = layout.volume_rect;

        // Get visible range
        let (start_idx, end_idx) = self.state.visible_range();
        let cutoff = self.replay_time();
        let (registry, bars) = if self.chart_type.is_time_independent() {
            (
                &mut self.element_studies.studies,
                &self.element_studies.bars[..],
            )
        } else {
            (&mut self.studies, self.state.data().bars.as_slice())
        };
        let mut changed: Vec<_> = registry
            .indicators_mut()
            .iter_mut()
            .enumerate()
            .filter_map(|(at, study)| study.set_visible_range(start_idx..end_idx).then_some(at))
            .collect();
        if !changed.is_empty() {
            let requests = registry
                .indicators()
                .iter()
                .flat_map(|study| study.data_requests())
                .collect();
            let sources = self
                .study_data
                .data(requests, &self.symbol, self.timeframe, cutoff);
            for at in super::studies::order(registry.indicators(), &self.study_ids) {
                if let Some(crate::studies::StudySource::Study { id, .. }) =
                    registry.indicators()[at].source()
                    && let Some(parent) = self
                        .study_ids
                        .iter()
                        .position(|candidate| candidate.0 == id)
                    && changed.contains(&parent)
                    && !changed.contains(&at)
                {
                    changed.push(at);
                }
                if changed.contains(&at) {
                    super::studies::update(
                        registry.indicators_mut(),
                        &self.study_ids,
                        at,
                        bars,
                        0,
                        &sources,
                    );
                }
            }
            // The data table lists the studies of the source bars, not their copies on elements.
            if !self.chart_type.is_time_independent() {
                self.readout_revision = crate::model::next_revision();
            }
        }
        self.start_idx = start_idx;

        let factor = self.prepare_conflation(
            self.state.time_scale().bar_spacing(),
            ui.ctx().pixels_per_point(),
        );

        #[cfg(feature = "studies")]
        let execution_profiles = self
            .feed_link
            .as_ref()
            .and_then(|link| link.footprint.as_ref())
            .filter(|_| {
                self.main_series_visible
                    && footprint.is_none()
                    && self.replay.is_none()
                    && self.chart_type == ChartType::SessionVolumeProfile
            })
            .and_then(|flow| {
                let first = flow.complete_from()?;
                let candles = self.state.shown()[..end_idx].partition_point(|bar| bar.time < first);
                let settings = self.trade_profile_settings();
                let extend = settings.extend_poc || settings.extend_vah || settings.extend_val;
                // Sessions left of the view still extend, but only from the first complete bar on.
                let profile_start = if extend {
                    candles
                } else {
                    candles.max(start_idx)
                };
                let ranges = self.execution_profile_ranges(profile_start..end_idx)?;
                let mapping = self.get_chart_mapping().with_rect(price_rect);
                let bars = &self.state.data().bars;
                let mut profiles =
                    crate::studies::trade_profiles_with_rows(flow, &ranges, settings.rows);
                profiles.retain(|profile| {
                    let first = bars.partition_point(|b| b.time < profile.range.start);
                    let end = bars.partition_point(|b| b.time < profile.range.end);
                    first < end
                        && mapping.idx_to_x(first) - mapping.bar_spacing * 0.45
                            <= price_rect.right()
                });
                // Sessions left of the view only draw their extensions.
                let shown = profiles.partition_point(|profile| {
                    let end = bars.partition_point(|b| b.time < profile.range.end);
                    mapping.idx_to_x(end - 1) < price_rect.left()
                });
                Some((flow.price_step(), flow.tick_size(), profiles, shown))
            });
        let fixed_footprint = self
            .feed_link
            .as_ref()
            .and_then(|link| link.footprint.as_ref())
            .filter(|_| {
                self.main_series_visible
                    && footprint.is_none()
                    && self.replay.is_none()
                    && self.chart_type == ChartType::VolumeFootprint
                    && self.chart_type_params.footprint_row_ticks > 0
            });
        let footprint_prices = fixed_footprint
            .into_iter()
            .flat_map(|flow| {
                let group = crate::chart::overlays::fixed_row_group(
                    flow,
                    self.chart_type_params.footprint_row_ticks,
                );
                let step = flow.price_step() * group as f64;
                self.state.shown()[start_idx..end_idx]
                    .iter()
                    .filter(|bar| flow.complete_from().is_some_and(|from| bar.time >= from))
                    .filter_map(|bar| flow.bar_at(bar.time))
                    .filter_map(|bar| {
                        bar.levels
                            .first_key_value()
                            .zip(bar.levels.last_key_value())
                    })
                    .flat_map(move |((&low, _), (&high, _))| {
                        [
                            low.div_euclid(group) as f64 * step - flow.tick_size() * 0.5,
                            (high.div_euclid(group) as f64 + 1.0) * step - flow.tick_size() * 0.5,
                        ]
                    })
            })
            .filter(|price| price.is_finite())
            .map(|price| [price, price])
            .reduce(|a, b| [a[0].min(b[0]), a[1].max(b[1])]);
        #[cfg(feature = "studies")]
        let profile_prices = execution_profiles
            .iter()
            .flat_map(|(step, tick, profiles, shown)| {
                profiles[*shown..].iter().flat_map(move |profile| {
                    let step = step * profile.group as f64;
                    profile
                        .volume
                        .levels
                        .first_key_value()
                        .zip(profile.volume.levels.last_key_value())
                        .into_iter()
                        .flat_map(move |((&low, _), (&high, _))| {
                            [
                                low as f64 * step - tick * 0.5,
                                (high as f64 + 1.0) * step - tick * 0.5,
                            ]
                        })
                })
            });
        #[cfg(not(feature = "studies"))]
        let profile_prices = std::iter::empty();
        // Fit the price scale, then apply this frame's price zoom
        let (series_values, mut margins) = self.fit_secondary_series();
        let (mut primitive_prices, mut primitive_margins) = self.main_primitive_prices();
        for idx in &study_indices {
            let study = &self.rendered_studies().indicators()[*idx];
            if study.is_visible()
                && !study.is_overlay()
                && self.study_autoscale[*idx]
                && !self.config.scale_price_chart_only
                && study.scale() == crate::studies::StudyScale::Main
                && !self.is_custom_data(self.study_ids[*idx])
            {
                let range = super::IndicatorPane::with_config(
                    super::IndicatorPaneConfig::from_study(study.as_ref()),
                )
                .calculate_y_range(study.as_ref(), &(start_idx..end_idx));
                primitive_prices.extend([range.0, range.1]);
            }
        }
        let target_prices = self.target_prices();
        let source_marks = self
            .mark_source
            .as_ref()
            .map(|source| &source.page.bar_marks);
        let marker_prices = self
            .marker_prices(
                &self.marks,
                source_marks.map_or(&[], |marks| &marks[..]),
                self.marker_options,
            )
            .filter(|_| self.main_series_visible && time_based)
            .collect::<Vec<_>>();
        let heikin_ashi = (self.chart_type == crate::model::ChartType::Heikin
            && !self.state.data().is_empty())
        .then(|| {
            self.heikin_ashi
                .bars(&self.state.data().bars, self.state.revision())
        });
        let (transform, fit) = self.state.fit_price_scale_with_series(
            (if self.chart_type.is_time_independent() {
                &self.element_studies.studies
            } else {
                &self.studies
            })
            .indicators()
            .iter()
            .zip(&self.study_autoscale)
            .enumerate()
            .filter(|(idx, (study, autoscale))| {
                study_indices.contains(idx)
                    && !self.config.scale_price_chart_only
                    && **autoscale
                    && !study.is_volume()
            })
            .map(|(_, (study, _))| study.as_ref()),
            heikin_ashi.as_ref().map(|bars| &bars[start_idx..end_idx]),
            self.main_series_visible,
            self.main_series_style.autoscale.as_ref(),
            (factor > 1
                && heikin_ashi.is_none()
                && self.state.price_scale().options().mode
                    != crate::scales::PriceScaleMode::Logarithmic)
                .then(|| {
                    self.merge_groups
                        .price_range(self.state.shown(), start_idx..end_idx)
                })
                .flatten(),
            marker_prices
                .into_iter()
                .chain(primitive_prices)
                .chain(target_prices)
                .chain(footprint_prices.into_iter().flatten())
                .chain(profile_prices),
            series_values,
        );
        drop(heikin_ashi);
        let data_range = fit.price_range;
        primitive_margins.merge(fit.margins);
        if self.main_series_visible && time_based {
            // The summary below the lowest ladder bar is drawn whole or not at all; bars left to
            // candles keep the range candles fit.
            let room = |flow: &Footprint, config: &FootprintConfig| {
                let spacing = self.state.time_scale().bar_spacing();
                if flow.complete_from().is_some()
                    && crate::chart::overlays::footprint_column_width(spacing, config).is_some()
                {
                    crate::chart::overlays::summary_room(config)
                } else {
                    0.0
                }
            };
            let bottom = match footprint {
                Some((flow, config)) => room(flow, config),
                None if self.replay.is_none() && self.chart_type == ChartType::VolumeFootprint => {
                    self.feed_link
                        .as_ref()
                        .and_then(|link| link.footprint.as_ref())
                        .map_or(0.0, |flow| {
                            // The room does not depend on the colours.
                            let colors = [egui::Color32::TRANSPARENT; 4];
                            let params = &self.chart_type_params;
                            room(flow, &FootprintConfig::from_chart_params(params, colors))
                        })
                }
                None => 0.0,
            };
            primitive_margins.merge(AutoscaleMargins { top: 0.0, bottom });
        }
        if primitive_margins != AutoscaleMargins::default() {
            margins
                .entry(self.main_series_scale)
                .or_default()
                .merge(primitive_margins);
        }
        // With nothing to fit the scale keeps its range, which already holds its margins.
        if !data_range.0.is_finite() {
            margins.remove(&self.main_series_scale);
        }
        if self.main_series_visible
            && footprint.is_none()
            && self.chart_type == ChartType::TimePriceOpportunity
            && !self.state.price_scale().is_manual()
        {
            let mut clock = self.bar_clock();
            if let Some(zone) = self
                .instrument()
                .filter(|i| i.session.is_none())
                .and_then(|i| i.time_zone.as_ref())
            {
                clock = clock.with_timezone(zone.clone());
            }
            let tick = self
                .instrument()
                .and_then(|instrument| instrument.tick_size);
            let config = self.chart_type_params.tpo_config(tick);
            // Automatic rows depend on the scale span. Revisit after fitting until the rows
            // actually drawn, including their outer half-tick edges, fit in the scale. Retain
            // fitted edges when the grid changes, so non-nested grids cannot alternate ranges.
            let initial = self.state.price_scale().range();
            let mut fitted = data_range;
            loop {
                let map =
                    self.state
                        .price_scale()
                        .map(transform, price_rect.top(), price_rect.bottom());
                let range = map.range();
                let (mut low, mut high) = fitted;
                let (mut traded_low, mut traded_high) = data_range;
                let mapping = self.get_chart_mapping().with_rect(price_rect);
                rendering::candles::tpo::visit_profiles(
                    &self.state.data().bars,
                    start_idx..end_idx,
                    self.state.revision(),
                    (&config, tick),
                    &clock,
                    (&map, price_rect, |i| mapping.idx_to_x(i)),
                    |profile, _, anchor, _, _, _| {
                        if anchor >= price_rect.right() {
                            return;
                        }
                        for value in [profile.profile_low, profile.profile_high]
                            .map(|p| transform.to_scale(p))
                        {
                            if value.is_finite() {
                                traded_low = traded_low.min(value);
                                traded_high = traded_high.max(value);
                            }
                        }
                        for price in [
                            profile.row_span(profile.profile_low).0,
                            profile.row_span(profile.profile_high).1,
                        ] {
                            let value = transform.to_scale(price);
                            if value.is_finite() {
                                low = low.min(value);
                                high = high.max(value);
                            }
                        }
                    },
                );
                if (low, high) == fitted {
                    break;
                }
                self.state.price_scale_mut().autoscale(low, high);
                fitted = (low, high);
                let next = self.state.price_scale().range();
                // A flat session without a tick must keep the scale's minimum span, rather than
                // deriving ever smaller rows from its own fitted row height.
                if traded_high - traded_low <= 1e-9
                    && next.max - next.min < initial.max - initial.min
                    && initial.contains(low)
                    && initial.contains(high)
                {
                    self.state
                        .price_scale_mut()
                        .autoscale(data_range.0, data_range.1);
                    break;
                }
                if next == range {
                    break;
                }
            }
        }
        self.fit_marker_scales(&painter, price_rect, &margins);
        self.apply_price_zoom(pending_price_zoom, &response, price_rect, transform);
        let bar_spacing = self.state.time_scale().bar_spacing();
        if self.config.lock_price_to_bar_ratio != self.last_ratio_lock {
            self.state.price_scale_mut().set_bar_ratio_locked(
                self.config.lock_price_to_bar_ratio,
                bar_spacing,
                price_rect.height(),
            );
        }
        self.config.lock_price_to_bar_ratio = self.state.price_scale().bar_ratio_locked();
        self.last_ratio_lock = self.config.lock_price_to_bar_ratio;
        self.state
            .price_scale_mut()
            .update_bar_ratio(bar_spacing, price_rect.height());
        self.update_secondary_bar_ratios(bar_spacing, price_rect.height());
        let price_map = self
            .series_display_map(
                super::SeriesId::MAIN,
                self.state
                    .price_scale()
                    .map(transform, price_rect.min.y, price_rect.max.y),
            )
            .with_thousands_sep(self.config.thousands_sep);

        self.update_shared_study_maps(price_rect);
        let price_formatter = self.series_label_formatter(
            self.series_axis_owner(super::SeriesId::MAIN),
            price_map.transform().mode,
        );

        // Store the final price mapping and rects for external use (selection dots, hit testing)
        self.last_price_map = price_map;
        self.last_rendered_price_rect = price_rect;
        self.last_rendered_volume_rect = volume_rect;

        self.begin_mark_hits();
        self.pump_marks(ui.ctx());

        // Get visible data
        let visible_data = self.state.visible_data();

        let groups = (factor > 1).then(|| {
            let covering = self
                .merge_groups
                .covering(start_idx..start_idx + visible_data.len());
            (covering.start, &self.merge_groups.groups[covering])
        });
        // The first bar of each group, at the group's position, for the time axis and sessions.
        let time_bars: Option<Vec<_>> =
            groups.map(|(_, groups)| groups.iter().map(|group| group.bar.clone()).collect());

        // Largest visible volume: the volume pane and volume candles scale by it
        let max_volume = time_bars
            .as_deref()
            .unwrap_or(visible_data)
            .iter()
            .map(|c| c.volume)
            .filter(|v| v.is_finite())
            .max_by(f64::total_cmp)
            .unwrap_or(1.0);

        // Time and price grid lines sharing a style are painted together.
        let horizontal_grid = self.config.show_grid && self.config.show_horizontal_grid;
        let time_grid = time_based && self.config.show_grid && self.config.show_vertical_grid;
        if horizontal_grid && !time_grid {
            rendering::render_grid(
                &painter,
                price_rect,
                &price_map,
                self.config.grid_color,
                self.config.grid_line_style,
            );
        }

        // Create rendering contexts
        let bar_spacing = self.state.time_scale().bar_spacing();
        let bar_width = bar_spacing * self.config.candle_width;
        let price_ctx = RenderContext::new(&painter, price_rect);
        let mut coords = ChartMapping::new(
            price_rect,
            bar_spacing,
            start_idx,
            self.state.time_scale().base_idx(),
            self.state.time_scale().right_offset_precise(),
            price_map,
        );
        coords.numeric_positions = self.state.time_scale().numeric_positions().cloned();
        let (time_data, time_coords) = match (&time_bars, groups) {
            (Some(bars), Some((start, _))) => (
                bars.as_slice(),
                super::merge::group_mapping(&coords, factor, start),
            ),
            _ => (visible_data, coords.clone()),
        };
        let (bullish, bearish) = self.main_body_colors();
        let colors = StyleColors {
            bullish,
            bearish,
            text: self.config.text_color,
            bullish_border: self.config.bullish_border_color,
            bearish_border: self.config.bearish_border_color,
            bullish_wick: self.config.bullish_wick_color,
            bearish_wick: self.config.bearish_wick_color,
            candle_border_width: self.config.candle_border_width,
        };

        let timezone = self.display_timezone();
        let time_scale = self.build_time_formatter();
        let host_scale = self.horizontal_scale.clone();
        let scale: &dyn crate::scales::HorizontalScale =
            host_scale.as_deref().unwrap_or(&time_scale);
        // The grid and the labels share one set of marks and the merged bars' mapping.
        let (time_marks, time_points, axis_coords) = if time_grid || self.config.show_time_labels {
            // Resolved once: a fixed-offset zone allocates on every clone.
            let calendar = self.calendar_timezone();
            let numeric = time_coords.numeric_positions.is_some();
            rendering::time_axis_marks(
                &RenderContext::new(&painter, chart_rect),
                time_data,
                &time_coords,
                scale,
                &self.chart_options.time_scale,
                calendar.as_ref(),
                |index| {
                    let index = index * factor as f64;
                    if numeric {
                        crate::model::time_at(self.state.shown(), index)
                    } else {
                        crate::model::time_at_with_timeframe(
                            self.state.shown(),
                            index,
                            self.timeframe(),
                            calendar.as_ref(),
                        )
                    }
                },
            )
        } else {
            (Vec::new(), Vec::new(), time_coords.clone())
        };

        if self.config.show_grid && self.config.show_vertical_grid {
            let style = self
                .config
                .vertical_grid_line_style
                .unwrap_or(self.config.grid_line_style);
            if time_based {
                rendering::render_time_grid(
                    &RenderContext::new(&painter, chart_rect),
                    &time_marks,
                    &time_points,
                    &axis_coords,
                    self.config
                        .vertical_grid_color
                        .unwrap_or(self.config.grid_color),
                    style,
                    horizontal_grid
                        .then_some((self.config.grid_color, self.config.grid_line_style)),
                );
            } else {
                rendering::render_vertical_grid(
                    &painter,
                    chart_rect,
                    &coords,
                    self.config
                        .vertical_grid_color
                        .unwrap_or(self.config.grid_color),
                    style,
                );
            }
        }

        let session_tf = self.resolve_session_timeframe(visible_data);
        let exchange_session = rendering::exchange_session(
            self.instrument(),
            self.config.session_config.as_ref(),
            &timezone,
            session_tf,
        );
        let session_painter = painter.with_clip_rect(price_rect);
        let session_ctx = RenderContext::new(&session_painter, price_rect);
        // Sessions and extended hours are periods of time, absent from other horizontal scales.
        if self.config.show_session_breaks && host_scale.is_none() {
            let provider = self
                .session_provider
                .as_deref()
                .unwrap_or(&exchange_session);
            let background =
                rendering::SessionBackgroundRenderer::from_background(self.config.background_color);
            background.render(
                &session_ctx,
                time_data,
                provider,
                &time_coords,
                time_coords.start_idx,
            );
            rendering::SessionBreakRenderer::new(rendering::SessionBreakRenderConfig {
                line_color: self.config.session_break_color,
                line_width: 1.0,
                style: self.config.session_break_style,
            })
            .render(
                &session_ctx,
                time_data,
                provider,
                &time_coords,
                time_coords.start_idx,
            );
        }
        let extended_hours = if time_based
            && host_scale.is_none()
            && session_tf.duration_ms() < crate::model::Timeframe::Day1.duration_ms()
            && self.config.show_extended_hours
        {
            let color = if self.config.extended_hours_color == egui::Color32::PLACEHOLDER {
                ui.active_theme().ui.accent.linear_multiply(0.08)
            } else {
                self.config.extended_hours_color
            };
            let shaded = rendering::render_extended_hours(
                &session_ctx,
                time_data,
                &exchange_session,
                &time_coords,
                time_coords.start_idx,
                color,
            );
            (shaded, color)
        } else {
            (Vec::new(), egui::Color32::TRANSPARENT)
        };

        // Render chart type with clipping to prevent bars from overlapping axes
        // CRITICAL: Use chart_rect.width() for consistency with drawing coordinate system
        let chart_rect_width = chart_rect.width();
        let idx_to_coord = |idx: usize, min_x: f32| -> f32 {
            self.state
                .time_scale()
                .idx_to_coord(idx, min_x, chart_rect_width)
        };

        // Create clipped painter and contexts to prevent bars from rendering on axes
        let clipped_painter = painter.with_clip_rect(chart_rect);
        let clipped_price_ctx = RenderContext::new(&clipped_painter, price_rect);
        let clipped_volume_ctx = RenderContext::new(&clipped_painter, volume_rect);

        let mut bar_clock = self.bar_clock();
        // TPO days follow the instrument's zone, as the session breaks do, also without its hours.
        if let Some(zone) = self
            .instrument()
            .filter(|instrument| instrument.session.is_none())
            .and_then(|instrument| instrument.time_zone.as_ref())
        {
            bar_clock = bar_clock.with_timezone(zone.clone());
        }
        let render_ctx = rendering::CandleDataContext {
            price_ctx: &clipped_price_ctx,
            volume_ctx: &clipped_volume_ctx,
            price_scale: &price_map,
            colors: &colors,
            visible_data,
            // Full dataset so window-independent transforms (Heikin-Ashi) can be
            // computed over the complete series, not just the visible slice.
            full_data: &self.state.data().bars,
            shows_elements: self.state.shows_elements(),
            revision: self.state.revision(),
            heikin_ashi: &self.heikin_ashi,
            start_idx,
            clock: &bar_clock,
            tick_size: self
                .instrument()
                .and_then(|instrument| instrument.tick_size),
            price_formatter: price_formatter.as_deref(),
        };

        let render_params = rendering::ChartTypeParams::new(
            rendering::BarDimensions::new(bar_width, self.config.wick_width),
            rendering::VolumeSettings::new(
                self.config.show_volume && !cfg!(feature = "studies"),
                max_volume,
            ),
            &self.chart_type_params,
            rendering::TradingColors::new(colors.bullish, colors.bearish),
            rendering::CoordMapping::new(chart_rect.min.x),
            self.config.price_source,
        );
        // Draw one merged bar, or a column's line points, for each group.
        let merged = footprint
            .map(|_| false)
            .or_else(|| super::merge::line_style(self.chart_type))
            .filter(|_| factor > 1)
            .map(|line| {
                let merge = |line| {
                    super::merge::Merged::new(
                        self.state.shown(),
                        &self.merge_groups,
                        start_idx..start_idx + visible_data.len(),
                        line,
                        self.config.price_source,
                    )
                };
                // Volume columns merge as bars beneath a merged line.
                let volume = (line && render_params.show_volume()).then(|| merge(false));
                (merge(line), volume)
            });
        let merged_x = |merged: &super::merge::Merged, index: usize, min_x: f32| {
            // Past the last merged bar, as a renderer measuring its pitch asks for.
            let past = |last: f32| last + (index + 1 - merged.index.len()) as f32 * factor as f32;
            let index = merged
                .index
                .get(index)
                .copied()
                .unwrap_or_else(|| merged.index.last().map_or(0.0, |&last| past(last)));
            self.state
                .time_scale()
                .idx_to_coord_precise(index, min_x, chart_rect_width)
        };
        let merged_params = rendering::ChartTypeParams::new(
            rendering::BarDimensions::new(bar_width * factor as f32, self.config.wick_width),
            rendering::VolumeSettings::new(
                render_params.show_volume() && merged.as_ref().is_some_and(|m| m.1.is_none()),
                max_volume,
            ),
            &self.chart_type_params,
            rendering::TradingColors::new(colors.bullish, colors.bearish),
            rendering::CoordMapping::new(chart_rect.min.x),
            self.config.price_source,
        );

        // Both paint paths and the summary box mix their shades with what the chart paints.
        let plot_background = self.plot_background();
        let host_config = footprint
            .filter(|_| time_based)
            .map(|(_, config)| FootprintConfig {
                background: config.background.or(plot_background),
                ..config.clone()
            });
        let host = footprint.map(|(flow, _)| flow).zip(host_config.as_ref());
        let feed = self
            .feed_link
            .as_ref()
            .and_then(|link| link.footprint.as_ref())
            .filter(|_| {
                time_based
                    && self.replay.is_none()
                    && (self.chart_type == ChartType::VolumeFootprint
                        || (self.chart_type == ChartType::SessionVolumeProfile
                            && cfg!(feature = "studies")))
            });
        let feed_config = feed.map(|_| FootprintConfig {
            background: plot_background,
            ..FootprintConfig::from_chart_params(
                &self.chart_type_params,
                [colors.bullish, colors.bearish, colors.text, colors.text],
            )
        });
        let session_profiles = host.is_none() && self.chart_type == ChartType::SessionVolumeProfile;
        // Bars from the first execution on are drawn from executions, those before as
        // candles, and all of them as candles while their cells would not be readable.
        let executions = host
            .or_else(|| feed.zip(feed_config.as_ref()))
            .filter(|(_, config)| {
                session_profiles
                    || crate::chart::overlays::footprint_column_width(bar_spacing, config).is_some()
            })
            .map(|(flow, config)| {
                let first = flow.complete_from();
                let candles = first.map_or(visible_data.len(), |first| {
                    visible_data.partition_point(|bar| bar.time < first)
                });
                (flow, config, candles)
            });

        let style = &self.main_series_style;
        // Last price line & label, from the newest bar even when it is scrolled out of view, or
        // from the newest bar in view
        let (last_bars, last_from) = match style.price_line_source {
            crate::model::PriceLineSource::LastBar => (self.state.data().bars.as_slice(), 0),
            crate::model::PriceLineSource::LastVisible => {
                let (from, to) = self.state.visible_range();
                (&self.state.shown()[..to], from)
            }
        };
        let last_at = last_bars[last_from..]
            .iter()
            .rposition(|bar| !bar.is_whitespace())
            .map(|at| last_from + at);
        let last = last_at.map(|at| &last_bars[at]);
        let last_color = last_at.map(|at| self.main_value_color(last_bars, at));
        let heikin_style = self.chart_type_params.styles.heikin.clone();
        let heikin_last = (self.chart_type == ChartType::Heikin && !self.state.data().is_empty())
            .then(|| {
                let bars = self
                    .heikin_ashi
                    .bars(&self.state.data().bars, self.state.revision());
                let (from, to) = match style.price_line_source {
                    crate::model::PriceLineSource::LastBar => (0, bars.len()),
                    crate::model::PriceLineSource::LastVisible => (start_idx, end_idx),
                };
                let index = from
                    + bars[from..to]
                        .iter()
                        .rposition(|bar| !bar.is_whitespace())?;
                let by_previous_close = heikin_style.color_by_previous_close;
                Some((
                    bars[index].clone(),
                    rendering::candles::heikin_rising(&bars, index, by_previous_close),
                ))
            })
            .flatten();
        let original_last = last;
        let last = if heikin_style.real_price_only {
            last
        } else {
            heikin_last.as_ref().map(|(bar, _)| bar).or(last)
        };
        let last_price_line = last
            .zip(last_color)
            .filter(|_| self.main_series_visible)
            .map(|(last, color)| {
                PriceLine::new(
                    last.close,
                    if self.chart_type == ChartType::Heikin && !heikin_style.real_price_only {
                        let rising = heikin_last.as_ref().is_none_or(|(_, rising)| *rising);
                        style.price_line_color.unwrap_or(if rising {
                            heikin_style.body_up
                        } else {
                            heikin_style.body_down
                        })
                    } else {
                        color
                    },
                )
                .with_width(if self.config.show_price_line && style.price_line_visible {
                    style.price_line_width
                } else {
                    0.0
                })
                .with_style(style.price_line_style)
                .with_axis_label(
                    (show_right_axis || show_left_axis)
                        && self.config.show_symbol_last_val
                        && style.last_value_visible,
                )
            });
        // Footprint fills and glyphs must cover the current-price stroke.
        let price_line_below = !session_profiles
            && executions.is_some_and(|(flow, _, candles)| {
                visible_data[candles..]
                    .iter()
                    .any(|bar| flow.bar_at(bar.time).is_some())
            });

        self.paint_primitives(&painter, &coords, PrimitivePass::Below);
        let series_backgrounds = primitives::backgrounds(
            &clipped_painter,
            self.main_pass_primitives(&coords),
            PrimitivePass::Series,
            factor,
        );
        let grouped_primitives = !series_backgrounds.is_empty();
        let studies = if self.chart_type.is_time_independent() {
            &self.element_studies.studies
        } else {
            &self.studies
        };
        let volume_map = volume_rect.is_positive().then(|| {
            crate::scales::PriceMap::new(
                crate::scales::PriceTransform::default(),
                crate::scales::PriceRange::new(
                    0.0,
                    studies
                        .indicators()
                        .iter()
                        .find(|study| {
                            study.is_volume()
                                && study.is_overlay()
                                && study.is_visible()
                                && !matches!(study.scale(), crate::studies::StudyScale::Shared(_))
                        })
                        .map_or(max_volume.max(1.0), |study| {
                            super::volume::range(
                                study.as_ref(),
                                &(start_idx..end_idx),
                                Some(&self.merge_groups),
                            )
                            .1
                        }),
                ),
                volume_rect.top(),
                volume_rect.bottom(),
                false,
            )
            .with_price_precision(Some(
                self.instrument()
                    .and_then(|instrument| instrument.volume_decimals)
                    .unwrap_or(0),
            ))
        });
        self.host_events.volume_map = volume_map;
        // The main pane's visual order: overlays listed before the series are drawn under it.
        let series_at = order
            .iter()
            .position(|id| *id == super::SeriesId::MAIN)
            .unwrap_or(0);
        let (behind, front) = order.split_at(series_at);
        let main_visible_range = start_idx..(start_idx + visible_data.len());
        let volume_coords = show_volume.then(|| coords.with_rect(volume_rect));
        // Primitives are hit tested at the pointer, before the chart state is borrowed mutably.
        let pointer = ui
            .input(|input| input.pointer.interact_pos())
            .filter(|pos| chart_rect.contains(*pos));
        let primitive_hit = pointer.and_then(|pos| {
            if grouped_primitives {
                primitives::primitive_at(self.main_pass_primitives(&coords), pos)
            } else {
                primitives::primitive_at(
                    self.main_primitives(&coords)
                        .filter(|(item, _)| !item.replaces),
                    pos,
                )
            }
        });
        let renderer_hit = pointer.and_then(|pos| {
            if grouped_primitives {
                primitives::primitive_at(
                    self.main_pass_primitives(&coords)
                        .filter(|(item, _)| item.replaces),
                    pos,
                )
            } else {
                order.iter().rev().fold(None, |best, id| {
                    primitives::best_hit(
                        best,
                        primitives::primitive_at(
                            self.main_primitives(&coords)
                                .filter(|(item, _)| item.replaces && item.series == *id),
                            pos,
                        ),
                    )
                })
            }
        });
        let rendered: Vec<_> = self
            .study_ids
            .iter()
            .copied()
            .filter(|id| self.has_renderer(*id))
            .collect();
        let hit_order: Vec<_> = order
            .iter()
            .copied()
            .filter(|id| {
                !rendered.contains(id) && (*id != super::SeriesId::MAIN || self.main_series_visible)
            })
            .collect();
        let last_bar = self.state.shown().len().saturating_sub(1);
        let secondary_hits = [pointer, response.interact_pointer_pos()].map(|pos| {
            pos.map(|pos| {
                (
                    pos,
                    hit_order.iter().rev().find_map(|id| {
                        let item = self.main_pane_series().find(|item| item.id == *id)?;
                        self.hit_price_series(
                            item,
                            pos,
                            price_rect,
                            self.series_map(item, price_rect),
                        )
                        .map(|bar| (super::ChartElementId::Series(item.id), bar))
                    }),
                )
            })
        });
        let element_at = |pos: Pos2| {
            let renderer = renderer_hit
                .as_ref()
                .filter(|_| Some(pos) == pointer)
                .and_then(|hit| {
                    if let super::PointerTarget::Element(id) = hit.target {
                        Some((id, coords.x_to_idx(pos.x).min(last_bar)))
                    } else {
                        None
                    }
                });
            let hit = hit_test_main_chart(
                pos,
                studies,
                &self.study_ids,
                &hit_order,
                &coords,
                volume_coords.as_ref(),
                self.state.shown(),
                max_volume,
                main_visible_range.clone(),
                self.main_series_style.hit_tolerance,
                Some(&self.merge_groups),
            )
            .filter(|(id, _)| match id {
                // A renderer hit tests its custom series in place of the study's plots.
                crate::widget::ChartElementId::Indicator(study) => !rendered.contains(study),
                _ => self.main_series_visible,
            });
            let secondary = secondary_hits
                .iter()
                .flatten()
                .find_map(|(at, hit)| (*at == pos).then_some(*hit).flatten());
            if grouped_primitives && renderer.is_some() {
                return renderer;
            }
            hit.into_iter()
                .chain(secondary)
                .chain(renderer)
                .max_by_key(|(element, _)| {
                    let id = match element {
                        super::ChartElementId::Indicator(id)
                        | super::ChartElementId::Series(id) => id,
                    };
                    order.iter().position(|member| member == id)
                })
        };
        // The element under the pointer goes last when hovered series are drawn on top.
        let on_top = ui
            .input(|input| input.pointer.hover_pos())
            .filter(|pos| self.chart_options.hovered_series_on_top && chart_rect.contains(*pos))
            .and_then(element_at)
            .map(|(id, _)| id);
        let main_on_top = on_top == Some(super::ChartElementId::Series(super::SeriesId::MAIN));
        let paint_overlay = |id: &super::SeriesId| {
            if let Some(item) = self.main_pane_series().find(|item| item.id == *id) {
                self.paint_series(
                    &clipped_painter,
                    price_rect,
                    item,
                    self.series_map(item, price_rect),
                );
                return;
            }
            let Some(study) = self
                .study_ids
                .iter()
                .position(|study| study == id)
                .map(|idx| &studies.indicators()[idx])
                .filter(|study| study.is_visible())
            else {
                return;
            };
            if self.host_events.annotation_target.borrow().is_some()
                && pointer
                    .and_then(element_at)
                    .is_some_and(|(element, _)| element == super::ChartElementId::Indicator(*id))
            {
                self.cover_annotation_hit();
            }
            if study.is_volume()
                && !matches!(study.scale(), crate::studies::StudyScale::Shared(_))
                && self.main_series_visible
                && !self.state.data().is_empty()
            {
                let rect = super::volume::overlay_rect(price_rect, &self.config);
                let (min, max) = super::volume::range(
                    study.as_ref(),
                    &(start_idx..end_idx),
                    Some(&self.merge_groups),
                );
                if let Some(color) = study.background() {
                    painter.rect_filled(rect, 0.0, color);
                }
                super::volume::paint(
                    &painter,
                    rect,
                    study.as_ref(),
                    &(start_idx..end_idx),
                    &coords,
                    min,
                    max,
                    Some(&self.merge_groups),
                );
            }
            // TPO studies draw the chart type's profiles beside the bars.
            if let Some(config) = study
                .tpo()
                .filter(|_| !self.chart_type.is_time_independent())
            {
                rendering::candles::tpo::render_live_tpo(
                    &clipped_painter,
                    price_rect,
                    &self.state.data().bars,
                    start_idx..start_idx + visible_data.len(),
                    self.state.revision(),
                    &price_map,
                    config,
                    render_ctx.tick_size,
                    &bar_clock,
                    idx_to_coord,
                    chart_rect.min.x,
                    colors.bullish,
                    colors.bearish,
                );
            }
            if self.has_renderer(*id) {
                if !grouped_primitives {
                    let view = self.primitive_view(
                        *id,
                        crate::chart::rendering::study_mapping(study.as_ref(), &coords),
                    );
                    for item in self.primitives_of(*id).filter(|item| item.replaces) {
                        item.draw(&clipped_painter, &view, PrimitivePass::Series, factor);
                    }
                }
            } else if study.is_overlay() {
                rendering::IndicatorRenderer::render(
                    &price_ctx,
                    std::slice::from_ref(study),
                    visible_data,
                    &price_map,
                    &coords,
                );
            } else {
                let mapping = rendering::study_mapping(study.as_ref(), &coords);
                if let Some(color) = study.background() {
                    painter
                        .with_clip_rect(price_rect)
                        .rect_filled(price_rect, 0.0, color);
                }
                rendering::IndicatorRenderer::render_study(
                    &price_ctx,
                    study.as_ref(),
                    visible_data,
                    start_idx,
                    &mapping.price,
                    &mapping,
                );
            }
        };
        let paint_main_series = || {
            if self.main_series_visible {
                if self.host_events.annotation_target.borrow().is_some()
                    && pointer.and_then(element_at).is_some_and(|(element, _)| {
                        element == super::ChartElementId::Series(super::SeriesId::MAIN)
                    })
                {
                    self.cover_annotation_hit();
                }
                if time_based {
                    self.paint_liquidity(&clipped_painter, &self.get_chart_mapping());
                }
                // Share the renderer instantiation between ordinary and merged bars.
                let (mut data, params) = match &merged {
                    Some((merged, _)) => (
                        rendering::CandleDataContext {
                            visible_data: &merged.bars[merged.start..],
                            full_data: &merged.bars,
                            start_idx: merged.start,
                            ..render_ctx
                        },
                        &merged_params,
                    ),
                    None => (
                        rendering::CandleDataContext { ..render_ctx },
                        &render_params,
                    ),
                };
                if let Some((_, _, candles)) = executions {
                    data.visible_data = match &merged {
                        // Merged candles end before the group of the first execution.
                        Some((merged, _)) if candles < visible_data.len() => {
                            let end = merged.count_before(start_idx + candles, factor);
                            &merged.bars[merged.start..end.max(merged.start)]
                        }
                        Some(_) => data.visible_data,
                        None => &visible_data[..candles],
                    };
                }
                // A host's footprint replaces any chart type, so what it leaves is always candles.
                let series_type = if host.is_some() {
                    ChartType::Candles
                } else {
                    self.chart_type
                };
                // Main volume candles have a Borders switch and the painter's stroke; the
                // configured border width is for plain candles.
                let without_width = StyleColors {
                    candle_border_width: 0.0,
                    ..colors
                };
                if series_type == ChartType::VolumeCandles {
                    data.colors = &without_width;
                }
                let paint_series =
                    |data: &rendering::CandleDataContext,
                     params: &rendering::ChartTypeParams,
                     merged: Option<&super::merge::Merged>| {
                        rendering::render_chart_type(series_type, data, params, |index, min_x| {
                            match merged {
                                Some(merged) => merged_x(merged, index, min_x),
                                None => idx_to_coord(index, min_x),
                            }
                        });
                    };
                paint_series(&data, params, merged.as_ref().map(|(merged, _)| merged));
                if let Some((flow, config, candles)) = executions {
                    let has_cells = |bar: &crate::model::Bar| {
                        session_profiles || flow.bar_at(bar.time).is_some()
                    };
                    let mut first = start_idx + candles;
                    // A bar nothing traded in shows its candle among the cells.
                    for run in visible_data[candles..]
                        .chunk_by(|bar, next| has_cells(bar) == has_cells(next))
                    {
                        if !has_cells(&run[0]) {
                            let data = rendering::CandleDataContext {
                                visible_data: run,
                                start_idx: first,
                                ..render_ctx
                            };
                            paint_series(&data, &render_params, None);
                        } else if params.show_volume() {
                            rendering::candles::volume::render_volume_bars(
                                &clipped_volume_ctx,
                                run,
                                first,
                                max_volume,
                                &colors,
                                chart_rect.min.x,
                                idx_to_coord,
                            );
                        }
                        first += run.len();
                    }
                    let mapping = self.get_chart_mapping();
                    let bars = self.bars(super::SeriesId::MAIN);
                    if !session_profiles {
                        if price_line_below && let Some(line) = &last_price_line {
                            rendering::render_price_line(
                                &painter,
                                price_rect,
                                &price_map,
                                line,
                                price_rect.left(),
                                false,
                                price_rect.right(),
                            );
                        }
                        crate::chart::overlays::render_footprint(
                            &clipped_painter,
                            &mapping,
                            bars,
                            flow,
                            config,
                        );
                        #[cfg(feature = "studies")]
                        crate::chart::overlays::render_footprint_annotations(
                            &clipped_painter,
                            &mapping,
                            bars,
                            flow,
                            config,
                            response.id,
                        );
                    }
                    #[cfg(feature = "studies")]
                    if session_profiles
                        && let (Some((_, _, profiles, _)), Some(flow)) = (&execution_profiles, feed)
                    {
                        crate::chart::overlays::render_session_volume_profiles(
                            &clipped_painter,
                            &mapping,
                            bars,
                            profiles,
                            flow,
                            self.trade_profile_settings(),
                        );
                    }
                }
                if let Some(table) = statistics_rect
                    && let Some((flow, config)) = host.or_else(|| feed.zip(feed_config.as_ref()))
                {
                    crate::chart::overlays::render_footprint_statistics_rows(
                        &painter,
                        &self.get_chart_mapping().with_rect(table),
                        self.bars(super::SeriesId::MAIN),
                        flow,
                        config.text_color,
                        statistics_rows.as_deref().unwrap(),
                    );
                }
                if let Some((_, Some(volume))) = &merged {
                    rendering::candles::volume::render_volume_bars(
                        &clipped_volume_ctx,
                        &volume.bars[volume.start..],
                        volume.start,
                        max_volume,
                        &colors,
                        chart_rect.min.x,
                        |index, min_x| merged_x(volume, index, min_x),
                    );
                }
                self.paint_last_price_pulse(&clipped_painter, price_rect, &coords);
            }
        };
        let paint_unhovered = |id: &super::SeriesId| {
            let element = if self.price_series.iter().any(|item| item.id == *id) {
                super::ChartElementId::Series(*id)
            } else {
                super::ChartElementId::Indicator(*id)
            };
            if on_top != Some(element) {
                paint_overlay(id);
            }
        };
        behind.iter().for_each(paint_unhovered);
        if !main_on_top {
            paint_main_series();
        }

        self.paint_marks(&clipped_price_ctx, &coords, MarkerZOrder::Normal);

        if let Some(bubbles) = self
            .feed_link
            .as_ref()
            .and_then(|link| link.bubbles.as_ref())
            .filter(|_| time_based && self.main_series_visible && self.replay.is_none())
        {
            let x = |i| idx_to_coord(start_idx + i, chart_rect.min.x);
            bubbles.render(&clipped_painter, visible_data, coords.bar_spacing, x, |p| {
                price_map.price_to_y(p)
            });
        }

        front.iter().for_each(paint_unhovered);
        match on_top {
            Some(super::ChartElementId::Indicator(id)) => paint_overlay(&id),
            _ if main_on_top => paint_main_series(),
            Some(super::ChartElementId::Series(id)) => {
                if let Some(item) = self.main_pane_series().find(|item| item.id == id) {
                    self.paint_series(
                        &clipped_painter,
                        price_rect,
                        item,
                        self.series_map(item, price_rect),
                    );
                }
            }
            _ => {}
        }
        for study in studies
            .indicators()
            .iter()
            .filter(|study| study.is_visible() && study.labels_on_price())
        {
            crate::chart::indicators::plots::paint_labels(
                &clipped_painter,
                study.as_ref(),
                &price_map,
                &coords,
            );
        }
        self.paint_signals(ui, &coords);
        let target_lines = self
            .show_price_targets
            .then(|| painter.add(egui::Shape::Noop));

        primitives::paint_backgrounds(&clipped_painter, series_backgrounds);
        if grouped_primitives {
            for (item, view) in self.main_pass_primitives(&coords) {
                item.draw(&clipped_painter, &view, PrimitivePass::Series, factor);
            }
        } else {
            for (item, view) in self
                .main_primitives(&coords)
                .filter(|(item, _)| !item.replaces)
            {
                item.draw(&clipped_painter, &view, PrimitivePass::Series, factor);
            }
        }

        // Resolve a click on the main chart area into a selection. Overlay
        // indicators take priority over the series beneath them; an empty-area
        // click clears the selection. Clicks in a study's pane are resolved
        // where the panes are rendered, outside this rect.
        if response.clicked()
            && let Some(click_pos) = response.interact_pointer_pos()
            && self.mark_at(click_pos).is_none()
        {
            match element_at(click_pos) {
                Some((id, bar_idx)) => self.selection.select(id, Some(bar_idx)),
                None => self.selection.deselect(),
            }
        }

        // Resolve a right-click into a context-menu target. Mirrors the
        // left-click priority (drawing > overlay indicator > series), and
        // right-clicking an object also selects it so the menu and the
        // selection handles agree. The captured target is
        // drained by the host via `take_right_click`, or consumed by
        // `ChartMenus`.
        if response.secondary_clicked()
            && let Some(click_pos) = response.interact_pointer_pos()
            && chart_rect.contains(click_pos)
        {
            let price = coords.y_to_price(click_pos.y);

            // Drawings sit on top of the series, so they take priority.
            #[cfg(feature = "drawings")]
            let drawing_hit = self.drawings.hit_test(click_pos);
            #[cfg(not(feature = "drawings"))]
            let drawing_hit: Option<usize> = None;

            self.right_click = Some(if let Some(drawing_id) = drawing_hit {
                // Right-clicking a drawing selects it and
                // clears any series/indicator selection so handles don't show
                // on two objects at once.
                self.selection.deselect();
                #[cfg(feature = "drawings")]
                {
                    if self.drawings.is_selected(drawing_id) {
                        let ids: Vec<_> = std::iter::once(drawing_id)
                            .chain(self.drawings.selected_ids())
                            .collect();
                        self.drawings.select_drawings(ids);
                    } else {
                        self.drawings.select(drawing_id);
                    }
                }
                RightClickTarget::Drawing {
                    drawing_id,
                    pos: click_pos,
                }
            } else {
                match element_at(click_pos) {
                    Some((id, bar_idx)) => {
                        self.selection.select(id, Some(bar_idx));
                        RightClickTarget::Element {
                            id,
                            bar_idx,
                            pos: click_pos,
                            price,
                        }
                    }
                    None => {
                        self.selection.deselect();
                        RightClickTarget::Background {
                            pos: click_pos,
                            price,
                        }
                    }
                }
            });
        }

        // What the pointer is over, for host pointer events, in the right-click priority.
        if let Some(pos) = pointer {
            #[cfg(feature = "drawings")]
            let drawing = self.drawings.hit_test(pos);
            #[cfg(not(feature = "drawings"))]
            let drawing: Option<usize> = None;
            // These primitives paint after the series layer. A renderer painted in its place in
            // the order has already covered what lies under it there.
            let covers_annotations = primitive_hit
                .as_ref()
                .is_some_and(|hit| hit.result.pass == PrimitivePass::Series);
            let hit = primitives::best_hit(
                primitive_hit,
                renderer_hit
                    .as_ref()
                    .filter(|hit| {
                        element_at(pos).map(|(id, _)| super::PointerTarget::Element(id))
                            == Some(hit.target.clone())
                    })
                    .cloned(),
            );
            if let Some(hit) = hit.as_ref().filter(|_| {
                drawing.is_none()
                    && response.hovered()
                    && !response.dragged()
                    && !self.zoom_mode_active
                    && !eraser_mode
                    && !drawing_tool_active
            }) {
                ui.ctx().set_cursor_icon(hit.result.cursor);
            }
            self.host_events.target = drawing
                .map(super::PointerTarget::Drawing)
                .or(hit.as_ref().map(|hit| hit.target.clone()))
                .or_else(|| element_at(pos).map(|(id, _)| super::PointerTarget::Element(id)));
            if covers_annotations {
                self.cover_annotation_hit();
            }
            self.host_events.primitive_hit =
                hit.filter(|_| drawing.is_none()).map(|hit| hit.result);
        }

        // Draw selection handles for whichever element is currently selected on
        // the main chart (overlay indicator line or series data points).
        self.render_main_chart_selection(&painter, &coords, visible_data, main_visible_range);

        self.paint_marks(&clipped_price_ctx, &coords, MarkerZOrder::AboveSeries);

        if show_right_axis || show_left_axis {
            let axis_painter =
                painter.with_clip_rect(painter.clip_rect().intersect(price_axis_rect));
            for (item, view) in self.main_primitives(&coords) {
                item.get()
                    .draw_price_axis(&axis_painter, price_axis_rect, &view);
            }
        }

        // The axis labels go at these depths, painted once the crosshair's tags are placed
        let price_labels = painter.add(egui::Shape::Noop);

        let axis_edge = if show_right_axis {
            price_axis_rect.right()
        } else {
            price_rect.left()
        };
        let show_price_axis = show_right_axis || show_left_axis;
        let study_axis = crate::config::PriceAxisOptions {
            show_study_values: self.config.price_axis.show_study_values,
            ..self.config.price_axis
        };
        let currency = self
            .instrument()
            .and_then(|instrument| instrument.currency.as_deref())
            .filter(|code| self.config.show_currency && !code.is_empty());
        let unit = self
            .instrument()
            .and_then(|instrument| instrument.unit.as_deref())
            .filter(|unit| self.config.show_unit && !unit.is_empty());
        let has_currency = currency.is_some() || unit.is_some();
        let mut tags = Vec::new();
        if has_currency {
            tags.push(Some(Rect::from_min_size(
                price_rect.min,
                egui::vec2(price_rect.width(), 24.0),
            )));
        }
        let mut pending_values = Vec::new();
        for (id, line) in self.price_lines.iter().filter(|_| self.main_series_visible) {
            self.paint_annotation_line(
                &painter,
                price_rect,
                &price_map,
                super::SeriesId::MAIN,
                *id,
                line,
            );
            if line.axis_label {
                pending_values.push((line.price, rendering::axes::axis_label_colors(line), false));
            }
        }
        #[cfg(feature = "ui")]
        let trading_outlined = std::mem::take(&mut self.trading_outlined);
        #[cfg(feature = "ui")]
        pending_values.extend(
            std::mem::take(&mut self.trading_tags)
                .into_iter()
                .enumerate()
                .map(|(index, (value, fill))| {
                    let outlined = trading_outlined.contains(&index);
                    let colors = if outlined {
                        (fill, ui.active_theme().chart.bg)
                    } else {
                        (rendering::axes::tag_text_color(fill), fill)
                    };
                    (value, colors, outlined)
                }),
        );

        // The previous session's close and the last close, for the previous close line and the
        // legend's day change.
        let session_closes = (self.config.show_symbol_prev_close
            || self.config.show_prev_close_line
            || self.config.show_last_day_change)
            .then(|| {
                let bars = &self.state.data().bars;
                let last = crate::model::last_with_value(bars)?;
                let index = bars
                    .partition_point(|bar| {
                        exchange_session.trading_date(bar.time)
                            < exchange_session.trading_date(last.time)
                    })
                    .checked_sub(1)?;
                crate::model::last_with_value(&bars[..=index])
                    .map(|previous| (previous.close, last.close))
            })
            .flatten();
        if self.main_series_visible
            && (self.config.show_symbol_prev_close || self.config.show_prev_close_line)
            && let Some((previous_close, _)) = session_closes
        {
            let line = PriceLine::new(previous_close, ui.active_theme().ui.text_secondary)
                .with_style(crate::model::LineStyle::Dotted)
                .with_axis_label(self.config.show_symbol_prev_close)
                .with_width(if self.config.show_prev_close_line {
                    1.0
                } else {
                    0.0
                });
            rendering::render_price_line(
                &painter,
                price_rect,
                &price_map,
                &line,
                price_rect.left(),
                false,
                axis_edge,
            );
            if self.config.show_symbol_prev_close {
                pending_values.push((
                    previous_close,
                    rendering::axes::axis_label_colors(&line),
                    false,
                ));
            }
        }

        let style = &self.main_series_style;
        if self.main_series_visible
            && let Some(line) = super::series::base_line(&price_map, style)
        {
            rendering::render_price_line(
                &painter,
                price_rect,
                &price_map,
                &line,
                price_rect.left(),
                false,
                axis_edge,
            );
        }

        if self.main_series_visible
            && self.chart_type == ChartType::Heikin
            && !heikin_style.real_price_only
            && let Some((raw, color)) = original_last.zip(last_color)
        {
            let line = PriceLine::new(raw.close, color)
                .with_width(
                    if self.config.show_price_line && self.main_series_style.price_line_visible {
                        self.main_series_style.price_line_width
                    } else {
                        0.0
                    },
                )
                .with_style(self.main_series_style.price_line_style);
            rendering::render_price_line(
                &painter,
                price_rect,
                &price_map,
                &line,
                price_rect.left(),
                false,
                axis_edge,
            );
            if self.config.show_symbol_last_val && self.main_series_style.last_value_visible {
                pending_values.push((raw.close, rendering::axes::axis_label_colors(&line), false));
            }
        }

        let last_price_tag = if let Some((last, line)) = last.zip(last_price_line) {
            let line = if price_line_below {
                line.with_width(0.0)
            } else {
                line
            };
            // Only a countdown drawn on the axis needs the once-a-second repaint.
            let countdown = self.current_bar_countdown().filter(|_| {
                line.axis_label
                    && price_map
                        .range()
                        .contains(price_map.transform().to_scale(last.close))
            });
            if countdown.is_some()
                && (self.server_time_offset.is_some() || self.feed_link.is_some())
            {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs(1));
            }
            let quote_tags = rendering::axes::render_quote_tags_with_countdown(
                &painter,
                price_rect,
                &price_map,
                &line,
                self.session_quote
                    .as_ref()
                    .filter(|_| show_price_axis && self.config.show_extended_session_price)
                    .map(|quote| (quote, self.config.extended_session_price_color)),
                (show_volume && show_price_axis && !cfg!(feature = "studies")).then_some((
                    original_last.unwrap_or(last).volume,
                    // The tag takes the colour of the bar's direction in the applied theme, as
                    // its volume bar does.
                    if original_last.unwrap_or(last).is_bullish() {
                        ui.active_theme().chart.candle_up
                    } else {
                        ui.active_theme().chart.candle_down
                    },
                )),
                axis_edge,
                countdown.as_deref(),
                self.instrument()
                    .and_then(|instrument| instrument.volume_decimals),
                if has_currency { 24.0 } else { 0.0 },
                price_formatter.as_deref(),
                self.volume_formatter.as_deref(),
                self.config.price_axis.allow_label_overlap,
                if self.config.price_axis.show_symbol_name {
                    &self.symbol
                } else {
                    ""
                },
            );
            tags.extend([quote_tags[1], quote_tags[2]]);
            quote_tags[0]
        } else {
            None
        };

        tags.push(last_price_tag);
        if show_price_axis {
            // Labels that sit closer than a tag is high stack in the order of their prices.
            let mut shown: Vec<_> = pending_values
                .iter()
                .filter(|(value, ..)| {
                    price_map
                        .range()
                        .contains(price_map.transform().to_scale(*value))
                })
                .map(|(value, colors, outlined)| {
                    (price_map.price_to_y(*value), *value, *colors, *outlined)
                })
                .collect();
            shown.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut ys: Vec<_> = shown.iter().map(|shown| shown.0).collect();
            if !self.config.price_axis.allow_label_overlap {
                ys = rendering::axes::spread_axis_ys(&painter, &ys);
            }
            for ((_, value, colors, outlined), y) in shown.into_iter().zip(ys) {
                let label = price_map.label(
                    price_map.transform().to_scale(value),
                    rendering::axes::tag_decimals(&price_map, price_map.price_to_y(value)),
                    price_formatter.as_deref(),
                );
                let tag = rendering::axes::render_axis_value(
                    &painter,
                    price_rect,
                    axis_edge,
                    y,
                    label,
                    colors,
                    &tags,
                    self.config.price_axis.allow_label_overlap,
                );
                if outlined && let Some(rect) = tag {
                    painter.rect_stroke(
                        rect,
                        2.0,
                        egui::Stroke::new(1.0, colors.0),
                        egui::StrokeKind::Inside,
                    );
                }
                tags.push(tag);
            }
        }
        if show_price_axis {
            for (value, colors, map, label) in
                self.secondary_axis_values(self.main_series_scale, price_rect)
            {
                tags.push(rendering::axes::render_axis_value(
                    &painter,
                    price_rect,
                    axis_edge,
                    map.price_to_y(value),
                    label,
                    colors,
                    &tags,
                    self.config.price_axis.allow_label_overlap,
                ));
            }
        }
        let mut fixed_price_labels = Vec::new();
        if show_price_axis {
            let (fixed, automatic): (Vec<_>, Vec<_>) = self
                .main_primitives(&coords)
                .flat_map(|(item, view)| {
                    let map = view.mapping.price;
                    item.get()
                        .price_axis_labels(&view)
                        .into_iter()
                        .map(move |label| (label, map))
                })
                .partition(|(label, _)| label.fixed_coordinate.is_some());
            fixed_price_labels = fixed;
            rendering::axes::render_primitive_price_labels(
                &painter,
                price_rect,
                axis_edge,
                automatic,
                &mut tags,
                self.config.price_axis.allow_label_overlap,
            );
        }
        let mut independent_axis_width =
            self.paint_series_axes(ui, &painter, rect, price_rect, &colors);
        for (key, axis) in self.pane_scale_axes(main_pane, rect, price_rect) {
            let super::PaneScale::Study(id) = key else {
                continue;
            };
            let Some(idx) = self.study_ids.iter().position(|study| *study == id) else {
                continue;
            };
            let study = studies.indicators()[idx].as_ref();
            let mapping = crate::chart::rendering::study_mapping(study, &coords);
            independent_axis_width =
                independent_axis_width.max(crate::chart::rendering::render_study_axis(
                    &painter,
                    &[study],
                    &mapping,
                    axis,
                    &study_axis,
                    None,
                ));
        }
        if show_price_axis && study_axis.show_study_values {
            for study in studies
                .indicators()
                .iter()
                .enumerate()
                .filter(|(idx, _)| study_indices.contains(idx))
                .map(|(_, study)| study)
            {
                if study.is_volume()
                    && !matches!(study.scale(), crate::studies::StudyScale::Shared(_))
                {
                    if self.main_series_visible {
                        let rect = super::volume::overlay_rect(price_rect, &self.config);
                        let mapping = super::volume::mapping(
                            &coords,
                            rect,
                            study.as_ref(),
                            &(start_idx..end_idx),
                            Some(&self.merge_groups),
                        );
                        super::volume::tags(
                            &painter,
                            price_rect,
                            &mapping.price,
                            study.as_ref(),
                            axis_edge,
                            &mut tags,
                            &study_axis,
                        );
                    }
                    continue;
                }
                if study.scale() != crate::studies::StudyScale::Main
                    && study.scale() != crate::studies::StudyScale::Shared(self.main_series_scale)
                {
                    continue;
                }
                rendering::axes::render_study_tags(
                    &painter,
                    price_rect,
                    &price_map,
                    study.as_ref(),
                    axis_edge,
                    &mut tags,
                    price_formatter.as_deref(),
                    &study_axis,
                );
            }
        }

        if show_price_axis && self.main_series_visible {
            let mut values = Vec::new();
            if self.config.show_high_low {
                let high = visible_data
                    .iter()
                    .map(|bar| bar.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let low = visible_data
                    .iter()
                    .map(|bar| bar.low)
                    .fold(f64::INFINITY, f64::min);
                values.extend([(high, "High", colors.bullish), (low, "Low", colors.bearish)]);
            }
            let trading_bid_ask = {
                #[cfg(feature = "ui")]
                {
                    std::mem::take(&mut self.trading_bid_ask)
                }
                #[cfg(not(feature = "ui"))]
                {
                    false
                }
            };
            if (self.config.show_bid_ask || trading_bid_ask)
                && let Some(quote) = self.quote.as_ref().filter(|quote| {
                    quote.bid.is_finite()
                        && quote.ask.is_finite()
                        && quote.bid <= quote.ask
                        && quote.spread().is_finite()
                })
            {
                values.extend([
                    (quote.bid, "Bid", ui.active_theme().ui.accent),
                    (quote.ask, "Ask", ui.active_theme().chart.candle_down),
                ]);
            }
            for (value, name, color) in values {
                let line = PriceLine::new(value, color)
                    .with_width(
                        if (name == "Bid" || name == "Ask") && !self.config.show_bid_ask {
                            0.0
                        } else {
                            1.0
                        },
                    )
                    .with_style(crate::model::LineStyle::Dotted)
                    .with_axis_label(false);
                rendering::render_price_line(
                    &painter,
                    price_rect,
                    &price_map,
                    &line,
                    price_rect.left(),
                    false,
                    axis_edge,
                );
                if price_map
                    .range()
                    .contains(price_map.transform().to_scale(value))
                {
                    let price = price_map.label(
                        price_map.transform().to_scale(value),
                        rendering::axes::tag_decimals(&price_map, price_map.price_to_y(value)),
                        price_formatter.as_deref(),
                    );
                    let quote = name == "Bid" || name == "Ask";
                    let label = if quote {
                        price
                    } else {
                        format!("{name} {price}")
                    };
                    let text_color = rendering::axes::tag_text_color(color);
                    let tag = rendering::axes::render_axis_value(
                        &painter,
                        price_rect,
                        axis_edge,
                        price_map.price_to_y(value),
                        label,
                        (text_color, color),
                        &tags,
                        self.config.price_axis.allow_label_overlap,
                    );
                    if quote && let Some(rect) = tag {
                        let badge = painter.layout_no_wrap(
                            l10n::text(ui.ctx(), name),
                            rendering::axes::axis_font(ui.ctx()),
                            text_color,
                        );
                        let box_rect = Rect::from_min_size(
                            egui::pos2(
                                if axis_edge <= price_rect.left() {
                                    rect.right() + 1.0
                                } else {
                                    rect.left() - badge.size().x - 8.0
                                },
                                rect.top(),
                            ),
                            egui::vec2(badge.size().x + 7.0, rect.height()),
                        );
                        painter.rect_filled(box_rect, 2.0, color);
                        painter.galley(box_rect.center() - badge.size() / 2.0, badge, text_color);
                    }
                    tags.push(tag);
                }
            }
        }

        if self.config.show_time_labels {
            let axis = self.time_axis_rect;
            let axis_painter = painter.with_clip_rect(painter.clip_rect().intersect(axis));
            for (item, view) in self.time_axis_primitives(&coords) {
                item.get().draw_time_axis(&axis_painter, axis, &view);
            }
        }
        let time_labels = painter.add(egui::Shape::Noop);
        // Kept apart from the price tags, which size the price axis.
        let mut time_tags = Vec::new();
        let mut fixed_time_labels = Vec::new();
        if self.config.show_time_labels {
            let axis = self.time_axis_rect;
            let labels: Vec<_> = self
                .time_axis_primitives(&coords)
                .flat_map(|(item, view)| {
                    item.get()
                        .time_axis_labels(&view)
                        .into_iter()
                        .filter_map(move |label| {
                            let x = label
                                .fixed_coordinate
                                .map(|offset| axis.left() + offset)
                                .or_else(|| view.time_to_x(label.time))?;
                            Some((x, label))
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            for (x, label) in labels {
                if label.fixed_coordinate.is_some() {
                    fixed_time_labels.push((x, label));
                } else {
                    time_tags.push(rendering::axes::render_primitive_time_label(
                        &painter, axis, x, label,
                    ));
                }
            }
        }

        let bar_seconds = self.time_label_seconds();

        // Handle drawing tools
        #[cfg(feature = "drawings")]
        {
            // Clone/extract values before mutable borrow of self
            let timescale = self.state.time_scale().clone();

            let drawing_history = self.history.start_drawing_action(&mut self.drawings);
            // Temporarily take cursor_modes and the drawings to avoid borrow conflict
            let mut cursor_modes = std::mem::take(&mut self.cursor_modes);
            let mut drawings = std::mem::take(&mut self.drawings);

            self.handle_drawings(
                ui,
                &mut drawings,
                &mut cursor_modes,
                &response,
                price_rect,
                &price_map,
                &painter,
                &timescale,
                (time_scale.as_ref(), bar_seconds),
            );

            // Render eraser highlight if in eraser mode
            self.render_eraser_highlight(&painter, &drawings, &cursor_modes);

            // Put them back
            self.drawings = drawings;
            self.cursor_modes = cursor_modes;
            self.history
                .finish_drawing_action(&mut self.drawings, drawing_history);
        }
        #[cfg(feature = "drawings")]
        if let Some(id) = pointer.and_then(|pos| self.drawings.hit_test(pos)) {
            self.cover_annotation_hit();
            self.host_events.target = Some(super::PointerTarget::Drawing(id));
            self.host_events.primitive_hit = None;
        }
        self.paint_marks(&clipped_price_ctx, &coords, MarkerZOrder::Top);
        if !self.hide_foreground {
            self.paint_primitives(&painter, &coords, PrimitivePass::Above);
            if self.host_events.annotation_target.borrow().is_some()
                && pointer
                    .and_then(|pos| {
                        primitives::primitive_at(self.main_pass_primitives(&coords), pos)
                    })
                    .is_some_and(|hit| hit.result.pass == PrimitivePass::Above)
            {
                self.cover_annotation_hit();
            }
        }

        // Placed after the drawings handled this frame's input, so a dragged line's tags move
        // with it in the same frame.
        #[cfg(feature = "drawings")]
        {
            let mut prices = Vec::new();
            for tag in self
                .drawings
                .axis_tags(price_rect, ui.active_theme().chart.axis_text_secondary)
            {
                match tag {
                    crate::drawings::manager::DrawingAxisTag::Price { price, color } => {
                        if show_price_axis
                            && price.is_finite()
                            && price_map
                                .range()
                                .contains(price_map.transform().to_scale(price))
                        {
                            prices.push((price_map.price_to_y(price), price, color));
                        }
                    }
                    crate::drawings::manager::DrawingAxisTag::Time { x, anchor, color } => {
                        if self.config.show_time_labels
                            && let Some(time) = self.drawing_point_time(&anchor)
                        {
                            time_tags.push(Some(rendering::axes::paint_crosshair_tag(
                                &painter,
                                Pos2::new(x, time_axis_top + 1.5),
                                egui::Align2::CENTER_TOP,
                                chart_rect.left()..=chart_rect.right(),
                                scale.format_crosshair(time, bar_seconds),
                                (rendering::axes::tag_text_color(color), color),
                                rendering::axes::axis_font(ui.ctx()),
                                25.0,
                                None,
                            )));
                        }
                    }
                }
            }
            if !prices.is_empty() {
                // As the price lines' tags: tags closer than a tag is high stack in price order.
                prices.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut ys: Vec<_> = prices.iter().map(|price| price.0).collect();
                if !self.config.price_axis.allow_label_overlap {
                    ys = rendering::axes::spread_axis_ys(&painter, &ys);
                }
                for ((_, price, color), y) in prices.into_iter().zip(ys) {
                    let label = price_map.label(
                        price_map.transform().to_scale(price),
                        rendering::axes::tag_decimals(&price_map, price_map.price_to_y(price)),
                        price_formatter.as_deref(),
                    );
                    tags.push(rendering::axes::render_axis_value(
                        &painter,
                        price_rect,
                        axis_edge,
                        y,
                        label,
                        (rendering::axes::tag_text_color(color), color),
                        &tags,
                        self.config.price_axis.allow_label_overlap,
                    ));
                }
            }
        }

        // Pinned tags go above every other tag and below the crosshair's, which stay readable.
        for (x, label) in fixed_time_labels {
            time_tags.push(rendering::axes::render_primitive_time_label(
                &painter,
                self.time_axis_rect,
                x,
                label,
            ));
        }

        rendering::axes::render_primitive_price_labels(
            &painter,
            price_rect,
            axis_edge,
            fixed_price_labels,
            &mut tags,
            self.config.price_axis.allow_label_overlap,
        );

        // Draw crosshair with options from chart_options. The pointer still hovers the chart
        // while a context menu is open, so the crosshair, its labels and the tooltip wait until
        // the menu closes.
        #[cfg(feature = "drawings")]
        let eraser_mode = self.drawings.eraser_mode;
        // A handle or body drag runs without an active tool and keeps the readout off the drawing.
        #[cfg(feature = "drawings")]
        let drawing_interaction_active =
            self.drawings.active_tool.is_some() || self.drawings.dragging_handle.is_some();
        #[cfg(not(feature = "drawings"))]
        let drawing_interaction_active = false;
        let menu_open = self.menu_open || response.secondary_clicked();
        self.menu_open = menu_open;
        let mut crosshair_tags = [None; 2];
        #[cfg(feature = "drawings")]
        {
            self.cursor_modes.update(ui.input(|input| input.stable_dt));
            let trail = Some(self.chart_options.crosshair.style)
                .filter(|style| {
                    matches!(
                        style,
                        crate::config::CrosshairStyle::Demonstration
                            | crate::config::CrosshairStyle::Magic
                    )
                })
                .filter(|_| !eraser_mode);
            self.cursor_modes.follow(
                trail,
                response.hover_pos().filter(|pos| price_rect.contains(*pos)),
                ui.input(|input| input.pointer.primary_down()),
            );
            if trail.is_some() {
                self.cursor_modes.render(ui, price_rect);
            }
        }
        let mut table_rect = price_rect;
        for (id, study) in self
            .study_ids
            .iter()
            .zip(self.rendered_studies().indicators())
            .filter(|(id, study)| study.is_visible() && self.series_pane(**id) == Some(main_pane))
        {
            if let Some(table) = study.table() {
                table_rect.max.y =
                    crate::studies::table::paint(ui, table_rect, table, ui.scope_id().with(id))
                        .top()
                        - 8.0;
            }
        }
        let hover_pos = if touch_readout.touch_input
            && self.chart_options.tooltip.enabled
            && self.chart_options.tooltip.on_long_press
        {
            touch_readout
                .position
                .filter(|pos| hover_rect.contains(*pos))
        } else {
            self.axis_hover
                .filter(|(_, tag)| {
                    ui.input(|input| {
                        input.pointer.hover_pos().is_some_and(|pos| {
                            cfg!(feature = "icons")
                                && show_price_button
                                && rendering::axes::crosshair_price_button_rect(*tag).contains(pos)
                        })
                    })
                })
                .map(|(pos, _)| pos)
                .or_else(|| {
                    ui.input(|input| input.pointer.hover_pos()).filter(|pos| {
                        hover_rect.contains(*pos) && ui.rect_contains_pointer(hover_rect)
                    })
                })
        }
        .and_then(|pos| self.crosshair_position(pos, &coords));
        let mut study_action = None;
        let mut series_action = None;
        let mut legend_bottom = price_rect.top() + 8.0;
        {
            let visible_data = self.state.visible_data();
            let selected = hover_pos
                .and_then(|pos| coords.local_idx_at_x(pos.x, visible_data.len()))
                .map(|index| start_idx + index)
                .unwrap_or_else(|| {
                    // The last bar with a value, before any whitespace bars at the end.
                    let shown = self.state.shown();
                    shown
                        .iter()
                        .rposition(|bar| !bar.is_whitespace())
                        .unwrap_or(shown.len().saturating_sub(1))
                });
            if let Some(bar) = self.state.shown().get(selected) {
                let timeframe = self.timeframe.map(|tf| tf.as_str()).unwrap_or_default();
                #[cfg(feature = "ui")]
                let order_quantity = self.order_quantity.take();
                let mut legend = super::Legend::new(&self.symbol, &timeframe, bar);
                let statuses: Vec<_> = self
                    .legend_statuses
                    .iter()
                    .map(|(text, color, background)| super::LegendStatus {
                        text,
                        color: *color,
                        background: *background,
                    })
                    .collect();
                legend.formatters = self.value_formatters();
                legend.thousands_sep = price_map.thousands_sep();
                legend.instrument = self.instrument();
                legend.badge = self.legend_badge.clone();
                legend.show_logo = self.config.show_legend_badge;
                legend.statuses = &statuses;
                legend.quote = self
                    .quote
                    .as_ref()
                    .filter(|_| self.main_series_visible && self.config.show_quote_boxes);
                legend.show_title = self.main_series_present && self.config.show_symbol_labels;
                legend.show_market_status = self.config.show_market_status;
                legend.replaying = self.replay_time().is_some();
                if legend.show_title && legend.show_market_status {
                    // The quote's time is not a clock: it stops when quotes stop.
                    let now = self.chart_time().unwrap_or_else(chrono::Utc::now);
                    legend.session = rendering::exchange_session(
                        self.instrument(),
                        self.config.session_config.as_ref(),
                        &self.display_timezone(),
                        crate::model::Timeframe::Min1,
                    )
                    .market_session(now);
                    if legend.session.is_some() && self.replay_time().is_none() {
                        // Sessions change state on whole minutes.
                        let seconds = now.timestamp().rem_euclid(60) as u64;
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_secs(60 - seconds));
                    }
                }
                legend.series_visible = self.main_series_visible;
                legend.title_as_description = self.config.title_as_description;
                // A whitespace bar has no values to show.
                let values = self.main_series_visible && !bar.is_whitespace();
                legend.show_ohlc = values && self.config.show_ohlc_info;
                legend.show_change = values && self.config.show_ohlc_change;
                legend.show_change_percent = values && self.config.show_ohlc_change_percent;
                legend.show_indicator_titles = self.config.show_indicator_labels;
                legend.show_indicator_inputs = self.config.show_indicator_inputs;
                legend.show_indicator_values = self.config.show_indicator_last_val;
                legend.day_change =
                    session_closes.filter(|_| values && self.config.show_last_day_change);
                legend.study_backdrop = self.config.indicator_backdrop_opacity;
                legend.extended_hours = (&extended_hours.0, extended_hours.1);
                let inputs: Vec<_> = self.study_inputs.iter().map(String::as_str).collect();
                legend.study_inputs = &inputs;
                legend.previous_close =
                    crate::model::last_with_value(&self.state.shown()[..selected])
                        .map(|bar| bar.close);
                legend.bar_index = selected;
                legend.studies = self.rendered_studies().indicators();
                legend.show_volume =
                    values && self.chart_type.supports_volume() && self.config.show_legend_volume;
                #[cfg(feature = "ui")]
                {
                    legend.quantity = order_quantity;
                }
                let (quote_action, study_settings, bottom, quantity, main_action) = ui
                    .push_id(shown.id, |ui| {
                        legend.show_with_colors(
                            ui,
                            Rect::from_min_max(
                                price_rect.min,
                                egui::pos2(
                                    (price_rect.right() - 48.0).min(rect.right() - 100.0),
                                    price_rect.bottom(),
                                ),
                            ),
                            &colors,
                            Some(&study_indices),
                            if self.layout_link_controls {
                                rect.right() - super::layout::LINK_CONTROLS_INSET
                            } else {
                                f32::INFINITY
                            },
                        )
                    })
                    .inner;
                if let Some(action) = quote_action {
                    self.quote_action = Some(action);
                }
                #[cfg(feature = "ui")]
                if quantity.is_some() {
                    self.quantity_clicked = quantity;
                }
                #[cfg(not(feature = "ui"))]
                let _ = quantity;
                study_action = study_settings;
                series_action = main_action;
                legend_bottom = bottom;
            }
        }
        let (legend_bottom, overlay_remove) = self.paint_secondary_legend(
            ui,
            shown.id,
            &painter,
            price_rect,
            legend_bottom + 4.0,
            self.main_pane_series(),
        );
        #[cfg(feature = "studies")]
        if let Some((step, tick, profiles, shown)) = &execution_profiles {
            for profile in &profiles[*shown..] {
                let step = step * profile.group as f64;
                let levels = &profile.volume.levels;
                let Some(((&low, _), (&high, _))) =
                    levels.first_key_value().zip(levels.last_key_value())
                else {
                    continue;
                };
                if [
                    low as f64 * step - tick * 0.5,
                    (high as f64 + 1.0) * step - tick * 0.5,
                ]
                .into_iter()
                .all(|p| price_map.range().contains(transform.to_scale(p)))
                {
                    continue;
                }
                let mut labels = Vec::new();
                if let Some(poc) = profile.volume.poc() {
                    labels.push(("POC", poc as f64 * step));
                }
                if let Some((low, high)) = self
                    .trade_profile_settings()
                    .value_area
                    .and_then(|s| profile.volume.value_area(s))
                {
                    labels.extend([("VAH", high as f64 * step), ("VAL", low as f64 * step)]);
                }
                let bars = &self.state.data().bars;
                let first = bars.partition_point(|b| b.time < profile.range.start);
                let end = bars.partition_point(|b| b.time < profile.range.end);
                self.paint_profile_context(
                    ui,
                    &coords,
                    (coords.idx_to_x(first) - coords.bar_spacing * 0.45)
                        ..(coords.idx_to_x(end.saturating_sub(1)) + coords.bar_spacing * 0.5),
                    profile.range.start,
                    &labels,
                    legend_bottom + 6.0,
                );
            }
        }
        if self.main_series_visible
            && footprint.is_none()
            && self.chart_type == ChartType::TimePriceOpportunity
            && self.state.price_scale().is_manual()
        {
            rendering::candles::tpo::visit_profiles(
                &self.state.data().bars,
                start_idx..end_idx,
                self.state.revision(),
                (
                    &self.chart_type_params.tpo_config(render_ctx.tick_size),
                    render_ctx.tick_size,
                ),
                &bar_clock,
                (&price_map, price_rect, |i| coords.idx_to_x(i)),
                |profile, config, anchor, column, _, _| {
                    let cropped = [
                        profile.row_span(profile.profile_low).0,
                        profile.row_span(profile.profile_high).1,
                    ]
                    .into_iter()
                    .any(|p| !price_map.range().contains(transform.to_scale(p)));
                    if !cropped {
                        return;
                    }
                    let mut labels = Vec::new();
                    if config.show_poc {
                        labels.push(("POC", profile.poc_price));
                    }
                    if config.show_value_area {
                        labels.extend([
                            ("VAH", profile.value_area_high),
                            ("VAL", profile.value_area_low),
                        ]);
                    }
                    let widest = profile.price_tpo_count.values().max().copied().unwrap_or(0);
                    self.paint_profile_context(
                        ui,
                        &coords,
                        anchor..anchor + column * widest as f32,
                        profile.session_date,
                        &labels,
                        legend_bottom + 6.0,
                    );
                },
            );
        }
        if let Some(lines) = target_lines {
            self.paint_targets(ui, &coords, legend_bottom + 4.0, lines);
        }

        if self.chart_options.time_scale.ignore_whitespace_indices
            || self.chart_options.time_scale.ignore_hidden_indices
        {
            self.last_crosshair = None;
        }
        if let Some(hover_pos) = hover_pos {
            // Cache the hover bar index for multi-chart sync
            self.last_hover_bar_idx = coords
                .local_idx_at_x(hover_pos.x, visible_data.len())
                .map(|local| (start_idx + local) as f64);

            if !menu_open {
                let mut tooltip_options = self.chart_options.tooltip.clone();
                tooltip_options.enabled &= self.main_series_visible;
                tooltip_options.show_volume &= self.chart_type.supports_volume();
                let held = touch_readout.position.is_some()
                    || (!touch_readout.touch_input
                        && ui.input(|input| input.pointer.primary_down()));
                let held_values = tooltip_options.enabled
                    && tooltip_options.on_long_press
                    && held
                    && !eraser_mode
                    && !drawing_interaction_active;
                let mut crosshair_options = self.chart_options.crosshair;
                // Arrow keeps the axis labels, which a hidden crosshair never shows.
                if (held_values || eraser_mode)
                    && crosshair_options.style != crate::config::CrosshairStyle::Hidden
                {
                    crosshair_options.style = crate::config::CrosshairStyle::Arrow;
                }
                let index = coords
                    .local_idx_at_x(hover_pos.x, visible_data.len())
                    .map(|local| start_idx + local);
                let snapped = if price_rect.contains(hover_pos) {
                    index.map_or(hover_pos, |index| {
                        self.snap_crosshair(hover_pos, &price_map, index)
                    })
                } else {
                    hover_pos
                };
                self.last_crosshair = index.map(|index| {
                    let main = price_rect.contains(hover_pos) && !volume_rect.contains(hover_pos);
                    (
                        self.state.shown()[index].time,
                        main.then(|| price_map.y_to_price(snapped.y)),
                    )
                });
                self.last_crosshair_series = None;
                crosshair_options.mode = crate::config::CrosshairMode::Normal;
                let mut main_options = crosshair_options;
                if volume_rect.contains(hover_pos) {
                    main_options.horz_line_visible = false;
                    main_options.horz_label_visible = false;
                    if main_options.style == crate::config::CrosshairStyle::Dot {
                        main_options.style = crate::config::CrosshairStyle::Arrow;
                    }
                }
                crosshair_tags = rendering::render_crosshair_with_options(
                    &price_ctx,
                    snapped,
                    visible_data,
                    &price_map,
                    &coords,
                    &main_options,
                    time_axis_top,
                    axis_edge,
                    last_price_tag,
                    scale,
                    bar_seconds,
                    price_formatter.as_deref(),
                );
                if let Some(index) = index
                    .filter(|_| crosshair_options.style != crate::config::CrosshairStyle::Hidden)
                {
                    if self.main_series_visible {
                        let bar = &self.state.shown()[index];
                        let value = self
                            .config
                            .price_source
                            .compute(bar.open, bar.high, bar.low, bar.close);
                        if let Some(color) = rendering::candles::line_color_at(
                            self.chart_type,
                            &self.chart_type_params.styles,
                            self.chart_type_params.baseline,
                            &price_map,
                            price_rect,
                            value,
                            bar,
                        ) {
                            rendering::candles::render_crosshair_marker(
                                &painter.with_clip_rect(price_rect),
                                Pos2::new(coords.idx_to_x(index), price_map.price_to_y(value)),
                                &self.chart_type_params.styles,
                                color,
                                self.config.background_color,
                            );
                        }
                    }
                    for item in self.main_pane_series() {
                        self.paint_series_crosshair_marker(
                            &painter,
                            price_rect,
                            item,
                            self.series_map(item, price_rect),
                            index,
                        );
                    }
                }

                if volume_rect.contains(hover_pos)
                    && let Some(map) = volume_map
                {
                    let mut pos = hover_pos;
                    if let Some(index) = index {
                        pos.x = coords.idx_to_x(index);
                        if self.chart_options.crosshair.mode.snaps() {
                            pos.y = map.price_to_y(self.state.shown()[index].volume);
                        }
                    }
                    let mut options = crosshair_options.resolved(ui.ctx());
                    options.vert_line_visible = false;
                    let context = RenderContext::new(&painter, volume_rect);
                    rendering::render_crosshair_lines(
                        &context,
                        pos,
                        &options,
                        volume_rect.bottom(),
                    );
                    crosshair_tags[0] = rendering::render_crosshair_price_tag(
                        &context,
                        pos,
                        &map,
                        &options,
                        axis_edge,
                        None,
                        Some(
                            crate::scales::StudyFormat::Volume
                                .format(map.y_to_price(pos.y), map.price_precision()),
                        ),
                    );
                }

                if tooltip_options.enabled
                    && (!tooltip_options.on_long_press || held)
                    && !eraser_mode
                    && !drawing_interaction_active
                    && let Some(local_idx) = coords.local_idx_at_x(hover_pos.x, visible_data.len())
                    && !visible_data[local_idx].is_whitespace()
                {
                    let candle = &visible_data[local_idx];
                    if tooltip_options.on_long_press {
                        let x = coords.idx_to_x(start_idx + local_idx);
                        rendering::draw_dashed_line(
                            &clipped_painter,
                            Pos2::new(x, price_rect.top()),
                            Pos2::new(x, price_rect.bottom()),
                            egui::Stroke::new(
                                1.0,
                                self.chart_options
                                    .crosshair
                                    .resolved(ui.ctx())
                                    .vert_line_color,
                            ),
                            5.0,
                            5.0,
                        );
                        for (index, bar) in visible_data
                            .iter()
                            .enumerate()
                            .step_by(25)
                            .filter(|(_, bar)| !bar.is_whitespace())
                        {
                            let pos = Pos2::new(
                                coords.idx_to_x(start_idx + index),
                                price_map.price_to_y(bar.close),
                            );
                            clipped_painter.circle(
                                pos,
                                3.5,
                                ui.active_theme().chart.bg,
                                egui::Stroke::new(1.5, ui.active_theme().ui.accent),
                            );
                        }
                    }
                    // A symbol without metadata still heads the readout.
                    let heading = (self.instrument().is_none() && !self.symbol.is_empty())
                        .then(|| crate::model::Instrument::new(self.symbol.as_str()));
                    rendering::render_tooltip_with_options(
                        &price_ctx,
                        hover_pos,
                        candle,
                        crate::model::last_with_value(&self.state.shown()[..start_idx + local_idx])
                            .map(|bar| bar.close),
                        &tooltip_options,
                        &colors,
                        &price_map,
                        &coords,
                        visible_data,
                        scale,
                        bar_seconds,
                        &self
                            .study_rows_at(start_idx + local_idx)
                            .into_iter()
                            .filter(|(id, _)| {
                                !tooltip_options.on_long_press
                                    || tooltip_options.show_volume_details
                                    || self
                                        .study_ids
                                        .iter()
                                        .zip(self.rendered_studies().indicators())
                                        .any(|(study_id, study)| {
                                            study_id == id && !study.is_volume()
                                        })
                            })
                            .map(|(_, row)| row)
                            .collect::<Vec<_>>(),
                        self.instrument().or(heading.as_ref()),
                        self.value_formatters(),
                    );
                }
            }
        } else {
            // Clear hover bar index when not hovering locally
            self.last_hover_bar_idx = None;
            if touch_readout.touch_input {
                self.last_crosshair = None;
            }

            // Render synced crosshair from other charts (if available)
            {
                if !menu_open
                    && let Some(bar_idx) = self.synced_crosshair_bar_idx.and_then(|index| {
                        let options = &self.chart_options.time_scale;
                        if !options.ignore_whitespace_indices && !options.ignore_hidden_indices {
                            return Some(index);
                        }
                        self.crosshair_position(
                            Pos2::new(coords.index_to_x(index), price_rect.center().y),
                            &coords,
                        )
                        .map(|pos| coords.x_to_idx_f64(pos.x))
                    })
                {
                    // Convert bar index to screen x coordinate
                    let x = coords.idx_to_x(bar_idx.round() as usize);
                    if coords.is_x_visible(x) {
                        // At the host's price, or a synthetic position at the centre of the
                        // price range for a time alone
                        let target = self.synced_crosshair_series;
                        let main_target = target.is_none_or(|id| {
                            self.series_pane(id) == Some(main_pane) && self.is_series_visible(id)
                        });
                        let map = target
                            .filter(|_| main_target)
                            .and_then(|id| self.series_price_map(id))
                            .unwrap_or(price_map);
                        let mut options = self.chart_options.crosshair;
                        if !main_target {
                            options.horz_line_visible = false;
                            options.horz_label_visible = false;
                            if options.style == crate::config::CrosshairStyle::Dot {
                                options.style = crate::config::CrosshairStyle::Arrow;
                            }
                        }
                        let y = match self.synced_crosshair_price {
                            Some(price) => {
                                options.mode = crate::config::CrosshairMode::Normal;
                                map.price_to_y(price)
                            }
                            None => price_rect.center().y,
                        };
                        if let Some(bar) = self.state.shown().get(bar_idx.round() as usize) {
                            self.last_crosshair = Some((bar.time, self.synced_crosshair_price));
                            self.last_crosshair_series = target;
                        }
                        let target_axis = target.and_then(|id| {
                            let key = self.series_scale(id)?;
                            if key == self.main_series_scale {
                                show_price_axis.then_some(axis_edge)
                            } else {
                                self.secondary_axes(rect, price_rect)
                                    .into_iter()
                                    .find(|(scale, _)| *scale == key)
                                    .map(|(_, axis)| axis.right())
                            }
                        });
                        let formatter = target
                            .and_then(|id| self.series_label_formatter(id, map.transform().mode));
                        let mut main_options = options;
                        // A scale without a visible axis still gets its line and time label.
                        if target.is_some() && target_axis.is_none() {
                            main_options.horz_label_visible = false;
                        }
                        crosshair_tags = rendering::render_crosshair_with_options(
                            &price_ctx,
                            Pos2::new(x, y),
                            visible_data,
                            &map,
                            &coords,
                            &main_options,
                            time_axis_top,
                            target_axis.unwrap_or(axis_edge),
                            last_price_tag,
                            scale,
                            bar_seconds,
                            if target.is_some() {
                                formatter.as_deref()
                            } else {
                                price_formatter.as_deref()
                            },
                        );
                    }
                }
            }
        }

        self.axis_hover = hover_pos.zip(crosshair_tags[0]);
        #[cfg(feature = "icons")]
        if show_price_axis {
            if let Some((pos, tag)) = self.axis_hover.filter(|(pos, _)| {
                show_price_button && price_rect.contains(*pos) && !volume_rect.contains(*pos)
            }) {
                let price = if self.chart_options.crosshair.mode.snaps()
                    && let Some(index) = coords.local_idx_at_x(pos.x, visible_data.len())
                {
                    price_map.y_to_price(self.snap_crosshair(pos, &price_map, start_idx + index).y)
                } else {
                    price_map.y_to_price(pos.y)
                };
                let options = self.chart_options.crosshair.resolved(ui.ctx());
                if let Some(price) = rendering::axes::crosshair_price_action_with_colors(
                    ui,
                    tag,
                    price,
                    (
                        options.label_text_color,
                        options.horz_label_background_color,
                    ),
                ) {
                    self.right_click = Some(RightClickTarget::Background {
                        pos: Pos2::new(tag.left(), tag.bottom().min(price_rect.bottom())),
                        price,
                    });
                }
            }
            if show_axis_settings
                && rendering::axes::axis_settings_button(ui, axis_corner).clicked()
            {
                self.scale_settings_requested = true;
            }
        }

        let current_price = crate::model::last_with_value(&self.state.data().bars)
            .filter(|_| self.main_series_visible && self.config.show_symbol_last_val)
            .map(|bar| {
                price_map.label(
                    price_map.transform().to_scale(bar.close),
                    rendering::axes::tag_decimals(&price_map, price_map.price_to_y(bar.close)),
                    price_formatter.as_deref(),
                )
            });
        self.measured_axis_width = rendering::axes::price_axis_width(
            &painter,
            &price_map,
            &current_price.as_deref().into_iter().collect::<Vec<_>>(),
            price_formatter.as_deref(),
        );
        if cfg!(feature = "icons")
            && show_left_axis
            && !show_right_axis
            && self.config.show_plus_button
        {
            // Reserve the button before hover so entering the plot does not resize its panes.
            self.measured_axis_width += 26.0;
        }
        self.measured_axis_width = self.measured_axis_width.max(independent_axis_width);
        for tag in tags.iter().skip(usize::from(has_currency)).flatten() {
            self.measured_axis_width = self.measured_axis_width.max(tag.width() + 5.0 + 6.0);
        }

        if show_price_axis && has_currency {
            let currency = self
                .instrument()
                .and_then(|instrument| instrument.currency.as_deref())
                .filter(|_| self.config.show_currency)
                .unwrap_or("");
            let unit = self
                .instrument()
                .and_then(|instrument| instrument.unit.as_deref())
                .filter(|_| self.config.show_unit);

            let header = match (currency.is_empty(), unit.filter(|unit| !unit.is_empty())) {
                (false, Some(unit)) => format!("{currency} · {unit}"),
                (true, Some(unit)) => unit.to_owned(),
                _ => currency.to_owned(),
            };
            let header_width = painter
                .layout_no_wrap(header, rendering::axes::axis_font(ui.ctx()), colors.text)
                .size()
                .x
                + if currency.is_empty() { 16.0 } else { 26.0 };
            let axis = if show_left_axis
                && self.pane_scale_count(main_pane, crate::model::AxisSide::Left) == 1
            {
                Rect::from_min_max(
                    Pos2::new(rect.left(), price_rect.top()),
                    price_axis_rect.max,
                )
            } else {
                price_axis_rect
            };
            let (button, choice) =
                rendering::render_axis_currency(ui, axis, currency, unit, &self.currency_choices);
            self.measured_axis_width = self.measured_axis_width.max(header_width);
            if choice.is_some() {
                self.currency_choice = choice;
            }
            tags.push(Some(button.rect));
        }

        // Draw the axis labels, leaving out those the tags overlap
        tags.extend(crosshair_tags);
        let time_tags = if time_tags.is_empty() {
            &tags
        } else {
            time_tags.extend_from_slice(&tags);
            &time_tags
        };
        if show_right_axis {
            rendering::render_price_labels(
                &price_ctx,
                &price_map,
                &colors,
                &self.config.price_axis,
                &tags,
                price_labels,
                price_formatter.as_deref(),
            );
        } else if show_left_axis {
            rendering::render_price_labels_at_pos(
                &price_ctx,
                &price_map,
                &colors,
                &self.config.price_axis,
                crate::scales::PriceScaleId::Left,
                &tags,
                price_labels,
                price_formatter.as_deref(),
            );
        }
        if self.config.show_time_labels {
            let axis_context_rect =
                Rect::from_min_max(chart_rect.min, Pos2::new(chart_rect.right(), time_axis_top));
            let chart_ctx = RenderContext::new(&painter, axis_context_rect);
            rendering::render_time_labels(
                &chart_ctx,
                time_marks,
                &time_points,
                &axis_coords,
                &colors,
                time_tags,
                &self.chart_options.time_scale,
                time_labels,
            );
        }

        // Draw box zoom rect
        crate::widget::pan_zoom::render_box_zoom(&painter, &self.box_zoom);

        if let Some((nav, buttons)) = navigation {
            self.paint_navigation(ui, nav, buttons);
        }

        self.show_realtime_button(ui, realtime_rect, shown.id);

        // Focus ring for keyboard accessibility
        crate::styles::focus::draw_focus_ring(ui, &shown, 0.0);

        self.show_signal_menu(ui, price_rect);
        if let Some(action) = study_action {
            self.apply_study_action(action);
        }
        if let Some(action) = series_action {
            self.apply_main_series_action(action);
        }
        if let Some(id) = overlay_remove {
            self.user_action(|chart| chart.remove_series(id));
        }
        self.main_pane_actions(ui, rect, chart_rect);
        self.record_host_click(ui, &response);
        shown
    }
}

#[cfg(test)]
mod tests;
