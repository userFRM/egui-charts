//! Frame rendering: the `show` entry points and the per-frame paint and interaction pass.

use super::selection::hit_test_main_chart;
use super::{Chart, RightClickTarget};
use crate::chart::overlays::FootprintConfig;
use crate::chart::renderers::{self, ChartMapping, RenderContext, StyleColors};
use crate::chart::rendering;
use crate::config::{BackgroundStyle, WatermarkPos};
use crate::data::Footprint;
use crate::model::PriceLine;
use crate::scales::TimeFormatterBuilder;
use crate::styles::typography;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect, Response, Sense, Ui, Vec2};

impl Chart {
    /// Formats a timestamp with the chart's zone, clock visibility and bar duration.
    /// Hosts can use this for data windows and other readouts beside the chart.
    pub fn format_time(&self, time: chrono::DateTime<chrono::Utc>) -> String {
        self.build_time_formatter()
            .format_crosshair(time, self.time_label_seconds())
    }

    fn current_bar_countdown(&self) -> Option<String> {
        if !self.config.show_countdown || !self.config.show_symbol_last_val {
            return None;
        }
        let last = self.state.data().bars.last()?;
        let timeframe = self.timeframe?;
        if matches!(timeframe, crate::model::Timeframe::Ticks(_)) {
            return None;
        }
        let now = self
            .server_time_offset
            .and_then(|offset| chrono::Utc::now().checked_add_signed(offset))
            .or_else(|| {
                self.feed_link
                    .as_ref()
                    .and_then(super::feed::FeedLink::server_time)
            })
            .or_else(|| self.quote.as_ref().map(|quote| quote.ts))?;
        let session = renderers::exchange_session(
            self.instrument(),
            self.config.session_config.as_ref(),
            &self.display_timezone(),
            timeframe,
        );
        // Calendar bars encode their trading date in the feed's selected zone.
        let session_open = if self.feed_link.is_some() && timeframe.duration_ms() >= 86_400_000 {
            let date = match self.display_timezone() {
                crate::scales::TimezoneMode::Local => {
                    last.time.with_timezone(&chrono::Local).date_naive()
                }
                zone => last
                    .time
                    .with_timezone(&zone.to_tz().unwrap_or(chrono_tz::UTC))
                    .date_naive(),
            };
            date.and_hms_opt(0, 0, 0)?.and_utc()
        } else {
            last.time
        };
        let session_close = session.bar_session_close(session_open, timeframe, self.extended_hours);
        let boundary = self
            .feed_link
            .as_ref()
            .and_then(|link| link.clock.next_open(link.clock.bar_open(last.time)));
        let close = match (session_close, boundary) {
            (Some(session), Some(boundary)) if timeframe.duration_ms() < 86_400_000 => {
                Some(session.min(boundary))
            }
            (session, boundary) => session.or(boundary),
        };
        let has_session = self
            .instrument()
            .and_then(|instrument| instrument.session.as_ref())
            .is_some_and(|session| {
                !session.windows.is_empty() || !session.extended_windows.is_empty()
            })
            || self.config.session_config.is_some();
        if self.bar_close_time.is_none() && session_close.is_none() && has_session {
            return None;
        }
        crate::scales::bar_countdown(last.time, timeframe, now, self.bar_close_time.or(close))
    }

    // Source-bar cadence sets precision even when many elements share one time.
    fn time_label_seconds(&self) -> i64 {
        let bars = &self.state.data().bars;
        let visible = self.state.visible_data();
        let Some((first, last)) = visible.first().zip(visible.last()) else {
            return 1;
        };
        let end = bars.partition_point(|bar| bar.time <= last.time);
        let at = bars
            .partition_point(|bar| bar.time < first.time)
            .min(end.saturating_sub(2));
        if end >= 2 {
            (bars[at + 1].time - bars[at].time).num_seconds().abs()
        } else {
            1
        }
    }

    pub(super) fn display_timezone(&self) -> crate::scales::TimezoneMode {
        match &self.chart_options.time_scale.timezone {
            crate::scales::TimezoneMode::Exchange(fallback) => {
                crate::scales::TimezoneMode::Exchange(
                    self.instrument()
                        .and_then(|instrument| instrument.time_zone.as_ref())
                        .map(crate::model::TimezoneId::tz)
                        .unwrap_or(*fallback),
                )
            }
            timezone => timezone.clone(),
        }
    }

    fn build_time_formatter(&self) -> Box<dyn crate::scales::TimeFormatter> {
        TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(self.chart_options.time_scale.seconds_visible)
            .with_time_visible(self.chart_options.time_scale.time_visible)
            .with_timezone(self.display_timezone())
            .build()
    }

    /// Overrides automatic session breaks with host logic. `None` restores metadata-based breaks.
    pub fn set_session_provider(
        &mut self,
        provider: Option<std::sync::Arc<dyn crate::model::SessionProvider>>,
    ) {
        self.session_provider = provider;
    }

    /// Draw the chart background (solid or gradient)
    fn draw_background(&self, painter: &egui::Painter, rect: Rect) {
        // Skip background when chart is inside a container that handles its own background
        if self.config.skip_background {
            return;
        }

        match self.config.background_style {
            BackgroundStyle::Solid => {
                painter.rect_filled(rect, 0.0, self.config.background_color);
            }
            BackgroundStyle::VerticalGradient {
                top_color,
                bottom_color,
            } => {
                // Draw vertical gradient using a mesh
                let mesh = egui::Mesh {
                    indices: vec![0, 1, 2, 2, 3, 0],
                    vertices: vec![
                        egui::epaint::Vertex {
                            pos: rect.left_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: top_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: top_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: bottom_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.left_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: bottom_color,
                        },
                    ],
                    texture_id: egui::TextureId::default(),
                };
                painter.add(egui::Shape::mesh(mesh));
            }
            BackgroundStyle::HorizontalGradient {
                left_color,
                right_color,
            } => {
                // Draw horizontal gradient using a mesh
                let mesh = egui::Mesh {
                    indices: vec![0, 1, 2, 2, 3, 0],
                    vertices: vec![
                        egui::epaint::Vertex {
                            pos: rect.left_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: left_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: right_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: right_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.left_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: left_color,
                        },
                    ],
                    texture_id: egui::TextureId::default(),
                };
                painter.add(egui::Shape::mesh(mesh));
            }
        }
    }

    /// Draw watermark overlay (large symbol name)
    fn draw_watermark(&self, painter: &egui::Painter, rect: Rect) {
        if !self.config.show_watermark {
            return;
        }

        let text = self
            .config
            .watermark_text
            .as_deref()
            .unwrap_or(&self.symbol);
        if text.is_empty() {
            return;
        }

        let font_id = egui::FontId::proportional(self.config.watermark_font_size);

        // Calculate position based on watermark_pos
        let pos = match self.config.watermark_pos {
            WatermarkPos::Center => rect.center(),
            WatermarkPos::TopLeft => Pos2::new(
                rect.min.x + 20.0,
                rect.min.y + self.config.watermark_font_size,
            ),
            WatermarkPos::TopRight => Pos2::new(
                rect.max.x - 20.0,
                rect.min.y + self.config.watermark_font_size,
            ),
            WatermarkPos::BottomLeft => Pos2::new(rect.min.x + 20.0, rect.max.y - 20.0),
            WatermarkPos::BottomRight => Pos2::new(rect.max.x - 20.0, rect.max.y - 20.0),
        };

        let anchor = match self.config.watermark_pos {
            WatermarkPos::Center => egui::Align2::CENTER_CENTER,
            WatermarkPos::TopLeft => egui::Align2::LEFT_TOP,
            WatermarkPos::TopRight => egui::Align2::RIGHT_TOP,
            WatermarkPos::BottomLeft => egui::Align2::LEFT_BOTTOM,
            WatermarkPos::BottomRight => egui::Align2::RIGHT_BOTTOM,
        };

        painter.text(pos, anchor, text, font_id, self.config.watermark_color);
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
    /// Price-based types keep studies, drawings and markers but do not draw, hit or edit them.
    /// Studies continue to update and all three appear again on time-based types.
    ///
    /// Returns an [`egui::Response`] for additional interaction handling.
    pub fn show(&mut self, ui: &mut Ui) -> Response {
        self.show_content(ui, None)
    }

    /// Draws a footprint in place of the price series, below studies, cursor and legend.
    /// Supply executions aggregated with the same clock as the chart's source bars.
    /// Price-based chart types keep their normal series because their x-axis is not time.
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
        self.pump(ui.ctx());
        if let Some(wait) = self.refresh_studies(ui.input(|i| i.time)) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
        self.show_elements();
        let previous_axis_width = self.measured_axis_width;
        self.last_rendered_indicator_panes.clear();
        // Pane studies share the available height and the lowest time axis.
        let has_panes = self
            .studies
            .indicators()
            .iter()
            .any(|study| !study.is_overlay() && study.is_visible());
        let response = if has_panes && !self.chart_type.is_time_independent() {
            self.show_with_panes(ui, footprint)
        } else {
            self.show_internal(ui, None, footprint)
        };
        if (self.measured_axis_width - previous_axis_width).abs() > 0.5 {
            let old_width = self.last_rendered_price_rect.width();
            let new_width = self
                .calculate_layout_rects(response.rect)
                .price_rect
                .width();
            // After the first layout, fitting labels keeps the visible bar range.
            if previous_axis_width > 0.0 && old_width > 0.0 && new_width > 0.0 {
                let scale = self.state.time_scale_mut();
                scale.set_bar_spacing(scale.bar_spacing() * new_width / old_width);
            }
            ui.ctx().request_discard("Chart measured its axis labels");
        }
        response
    }

    /// Resolve the timeframe used to pick a session-break granularity.
    ///
    /// The chart's timeframe is the primary source. Without one, the cadence
    /// is inferred from the visible bars by taking the median gap between
    /// consecutive timestamps and snapping it to the nearest preset, so session
    /// breaks work even when the host never set a timeframe. Falls back to the
    /// default (1-minute) when there are too few bars to measure.
    fn resolve_session_timeframe(
        &self,
        visible_data: &[crate::model::Bar],
    ) -> crate::model::Timeframe {
        use crate::model::Timeframe;

        if let Some(tf) = self.timeframe {
            return tf;
        }

        // Infer from the median inter-bar gap (robust to session-edge jumps).
        if visible_data.len() < 2 {
            return Timeframe::default();
        }
        let mut gaps_ms: Vec<i64> = visible_data
            .windows(2)
            .map(|w| (w[1].time - w[0].time).num_milliseconds())
            .filter(|&g| g > 0)
            .collect();
        if gaps_ms.is_empty() {
            return Timeframe::default();
        }
        gaps_ms.sort_unstable();
        let median = gaps_ms[gaps_ms.len() / 2];

        // Snap to the closest preset by duration.
        Timeframe::all()
            .into_iter()
            .min_by_key(|tf| (tf.duration_ms() - median).abs())
            .unwrap_or_default()
    }

    /// Internal rendering method that orchestrates all modules
    pub(crate) fn show_internal(
        &mut self,
        ui: &mut Ui,
        shared_time_axis: Option<Rect>,
        footprint: Option<(&Footprint, &FootprintConfig)>,
    ) -> Response {
        // Register the SVG image loader once per context so the embedded
        // icons used by the toolbars and panels decode without the host app
        // having to wire egui_extras itself.
        #[cfg(feature = "icons")]
        {
            let ctx = ui.ctx().clone();
            let loaders_installed = egui::Id::unique("egui_charts::image_loaders_installed");
            if !ctx.data(|d| d.get_temp::<bool>(loaders_installed).unwrap_or(false)) {
                egui_extras::install_image_loaders(&ctx);
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
            painter.set_clip_rect(ui.clip_rect().intersect(rect.union(axis)));
            response = response.union(ui.interact(
                axis,
                response.id.with("shared_time_axis"),
                Sense::click_and_drag(),
            ));
        }
        let layout = self.pane_layout_rects(
            rect,
            self.config.show_time_labels && shared_time_axis.is_none(),
        );
        let chart_rect = layout.price_rect;
        let time_axis_top = shared_time_axis.map_or(chart_rect.bottom(), |axis| axis.top());

        let chart_width = chart_rect.width();

        // CRITICAL: Apply TimeScale width configuration BEFORE any zoom operations
        // This ensures apply_constraints() inside zoom() uses the correct self.width
        // to calculate constraint bounds. Without this, drawings drift during zoom
        // because constraints are calculated with stale width values.
        self.apply_timescale_config(chart_width);

        let navigation_rect = self.navigation_rect(ui, chart_rect);
        let navigation = navigation_rect
            .filter(|_| self.navigation_visible(ui, chart_rect))
            .map(|nav| {
                (
                    nav,
                    self.interact_navigation(ui, nav, chart_rect, response.id),
                )
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
        let shown = response.clone();
        let axis_corner = Rect::from_min_max(
            Pos2::new(
                if self.config.show_right_axis {
                    chart_rect.right()
                } else {
                    rect.left()
                },
                time_axis_top,
            ),
            Pos2::new(
                if self.config.show_right_axis {
                    rect.right()
                } else {
                    chart_rect.left()
                },
                time_axis_top + 28.0,
            ),
        );
        let axis_interaction = cfg!(feature = "icons")
            && (self.config.show_right_axis || self.config.show_left_axis)
            && ui.input(|input| {
                (input.pointer.any_down() || input.pointer.any_released())
                    && input
                        .pointer
                        .press_origin()
                        .or(input.pointer.hover_pos())
                        .is_some_and(|pos| {
                            axis_corner.contains(pos)
                                || self.axis_hover.is_some_and(|(_, tag)| tag.contains(pos))
                        })
            });
        if navigation_interaction || axis_interaction {
            response = ui.interact(
                Rect::NOTHING,
                response.id.with("navigation_input"),
                Sense::hover(),
            );
            response.rect = rect;
        }

        // Handle tracking mode
        self.handle_tracking_mode(ui, &response);

        // Request focus on hover for keyboard shortcuts
        self.request_focus_if_needed(&mut response);

        // Handle keyboard shortcuts
        self.handle_keyboard_shortcuts(ui, &response, chart_width, chart_rect.min.x);

        // Calculate visible bars
        let logical_range = self.state.time_scale().visible_logical_range();
        let visible_bars = logical_range.length().ceil() as usize;
        self.last_visible_bars = visible_bars;

        // Show grabbing cursor during panning
        self.set_panning_cursor(ui, &response);

        // Define axis rects
        let price_axis_rect = Rect::from_min_max(
            Pos2::new(axis_corner.left(), chart_rect.top()),
            Pos2::new(axis_corner.right(), chart_rect.bottom()),
        );
        let time_axis_rect = shared_time_axis.unwrap_or_else(|| {
            Rect::from_min_max(
                Pos2::new(chart_rect.min.x, chart_rect.max.y),
                Pos2::new(chart_rect.max.x, rect.max.y),
            )
        });

        // Handle double-click to reset axes
        self.handle_double_click(&response, price_axis_rect, time_axis_rect);

        // Handle mouse wheel for zoom/scroll
        // Block pan/zoom when drawing tool is active OR when manipulating a drawing
        let time_based = !self.chart_type.is_time_independent();
        let show_volume = self.main_series_visible
            && self.config.show_volume
            && self.chart_type.supports_volume();
        let is_drawing_interaction = time_based
            && (self.drawings.active_tool.is_some()
                || self.drawings.dragging_handle.is_some()
                || self.drawings.curr_drawing.is_some()
                || self.drawings.sel_drawing().is_some_and(|id| {
                    ui.input(|i| i.pointer.press_origin())
                        .is_some_and(|origin| {
                            self.drawings.hit_test_handle_by_id(origin, id).is_some()
                                || self.drawings.hit_test(origin) == Some(id)
                        })
                }));
        let pending_price_zoom = self.handle_mouse_wheel(
            ui,
            &response,
            chart_width,
            chart_rect.min.x,
            price_axis_rect,
        );

        // Handle pinch-to-zoom for touch/trackpad gestures
        self.handle_pinch_zoom(ui, &response, chart_width, chart_rect.min.x);

        // Handle drag to pan (blocked when interacting with drawings)
        self.handle_drag_pan(
            ui,
            &response,
            price_axis_rect,
            time_axis_rect,
            chart_rect.min.x,
            is_drawing_interaction,
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
                CrosshairStyle::Arrow | CrosshairStyle::Demonstration => {
                    // Arrow mode - default pointer
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
            }
        }

        // Draw background (solid or gradient)
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

        // Draw watermark overlay (if enabled)
        self.draw_watermark(&painter, chart_rect);

        if self.state.data().is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No data available",
                egui::FontId::proportional(typography::LG),
                self.config.text_color,
            );
            return response;
        }

        let price_rect = layout.price_rect;
        let volume_rect = layout.volume_rect;

        // Get visible range
        let (start_idx, _end_idx) = self.state.visible_range();
        self.start_idx = start_idx;

        // Capture near_live status for button
        let near_live_edge = self.state.time_scale().is_near_live_edge();

        // Handle "Jump to Latest" button interaction
        if !near_live_edge {
            let btn_size = Vec2::new(
                DESIGN_TOKENS.sizing.charts_ext.realtime_button_width,
                DESIGN_TOKENS.sizing.button_md,
            );
            let btn_pos = Pos2::new(
                price_rect.center().x - btn_size.x / 2.0,
                price_rect.min.y + DESIGN_TOKENS.spacing.lg + DESIGN_TOKENS.spacing.xs,
            );
            let btn_rect = Rect::from_min_size(btn_pos, btn_size);
            let btn_id = ui.scope_id().with("jump_to_latest");
            let btn_res = ui.interact(btn_rect, btn_id, egui::Sense::click());

            if btn_res.clicked() {
                self.state.time_scale_mut().scroll_to_realtime();
            }
        }

        // Fit the price scale, then apply this frame's price zoom
        let transform = self.state.fit_price_scale(
            self.studies
                .indicators()
                .iter()
                .zip(&self.study_autoscale)
                .filter(|(_, autoscale)| time_based && **autoscale)
                .map(|(study, _)| study.as_ref()),
        );
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
        let price_map = self
            .state
            .price_scale()
            .map(transform, price_rect.min.y, price_rect.max.y);

        // Store the final price mapping and rects for external use (selection dots, hit testing)
        self.last_price_map = price_map;
        self.last_rendered_price_rect = price_rect;
        self.last_rendered_volume_rect = volume_rect;

        // Get visible data
        let visible_data = self.state.visible_data();

        if visible_data.is_empty() {
            if let Some((nav, buttons)) = navigation {
                self.paint_navigation(ui, nav, buttons);
            }
            return shown;
        }

        // Largest visible volume: the volume pane and volume candles scale by it
        let max_volume = visible_data
            .iter()
            .map(|c| c.volume)
            .filter(|v| v.is_finite())
            .max_by(f64::total_cmp)
            .unwrap_or(1.0);

        // Draw grid
        if self.config.show_grid && self.config.show_horizontal_grid {
            rendering::render_grid(&painter, price_rect, &price_map, self.config.grid_color);
        }

        // Create rendering contexts
        let bar_spacing = self.state.time_scale().bar_spacing();
        let bar_width = bar_spacing * self.config.candle_width;
        let price_ctx = RenderContext::new(&painter, price_rect);
        let coords = ChartMapping::new(
            price_rect,
            bar_spacing,
            start_idx,
            self.state.time_scale().base_idx(),
            self.state.time_scale().right_offset(),
            price_map,
        );
        let colors = StyleColors {
            bullish: self.config.bullish_color,
            bearish: self.config.bearish_color,
            text: self.config.text_color,
            bullish_border: self.config.bullish_border_color,
            bearish_border: self.config.bearish_border_color,
            bullish_wick: self.config.bullish_wick_color,
            bearish_wick: self.config.bearish_wick_color,
            candle_border_width: self.config.candle_border_width,
        };

        let timezone = self.display_timezone();
        let formatter = self.build_time_formatter();

        if self.config.show_grid && self.config.show_vertical_grid {
            if time_based {
                renderers::render_time_grid(
                    &RenderContext::new(&painter, chart_rect),
                    visible_data,
                    &coords,
                    formatter.as_ref(),
                    self.config
                        .vertical_grid_color
                        .unwrap_or(self.config.grid_color),
                );
            } else {
                rendering::render_vertical_grid(
                    &painter,
                    chart_rect,
                    &coords,
                    self.config
                        .vertical_grid_color
                        .unwrap_or(self.config.grid_color),
                );
            }
        }

        let session_tf = self.resolve_session_timeframe(visible_data);
        let exchange_session = renderers::exchange_session(
            self.instrument(),
            self.config.session_config.as_ref(),
            &timezone,
            session_tf,
        );
        let session_painter = painter.with_clip_rect(price_rect);
        let session_ctx = RenderContext::new(&session_painter, price_rect);
        if self.config.show_session_breaks {
            let provider = self
                .session_provider
                .as_deref()
                .unwrap_or(&exchange_session);
            let background =
                renderers::SessionBackgroundRenderer::from_background(self.config.background_color);
            background.render(&session_ctx, visible_data, provider, &coords, start_idx);
            renderers::SessionBreakRenderer::new(renderers::SessionBreakRenderConfig {
                line_color: self.config.session_break_color,
                line_width: 1.0,
                style: self.config.session_break_style,
            })
            .render(&session_ctx, visible_data, provider, &coords, start_idx);
        }
        if time_based
            && session_tf.duration_ms() < crate::model::Timeframe::Day1.duration_ms()
            && self.config.show_extended_hours
        {
            let color = if self.config.extended_hours_color == egui::Color32::PLACEHOLDER {
                ui.active_theme().ui.accent.linear_multiply(0.08)
            } else {
                self.config.extended_hours_color
            };
            renderers::render_extended_hours(
                &session_ctx,
                visible_data,
                &exchange_session,
                &coords,
                start_idx,
                color,
            );
        }

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

        let render_ctx = rendering::CandleDataContext {
            price_ctx: &clipped_price_ctx,
            volume_ctx: &clipped_volume_ctx,
            price_scale: &price_map,
            colors: &colors,
            visible_data,
            // Full dataset so window-independent transforms (Heikin-Ashi) can be
            // computed over the complete series, not just the visible slice.
            full_data: &self.state.data().bars,
            revision: self.state.revision(),
            start_idx,
        };

        let render_params = rendering::ChartTypeParams::new(
            rendering::BarDimensions::new(bar_width, self.config.wick_width),
            rendering::VolumeSettings::new(self.config.show_volume, max_volume),
            &self.chart_type_params,
            rendering::TradingColors::new(self.config.bullish_color, self.config.bearish_color),
            rendering::CoordMapping::new(chart_rect.min.x),
            self.config.price_source,
        );

        if self.main_series_visible {
            if let Some((flow, config)) = footprint.filter(|_| time_based) {
                if show_volume {
                    rendering::candles::volume::render_volume_bars(
                        &clipped_volume_ctx,
                        visible_data,
                        start_idx,
                        max_volume,
                        &colors,
                        chart_rect.min.x,
                        idx_to_coord,
                    );
                }
                crate::chart::overlays::render_footprint(
                    &clipped_painter,
                    &self.get_chart_mapping(),
                    self.bars(super::SeriesId::MAIN),
                    flow,
                    config,
                );
            } else {
                rendering::render_chart_type(
                    self.chart_type,
                    &render_ctx,
                    &render_params,
                    idx_to_coord,
                );
            }
        }

        if let Some(bubbles) = self
            .feed_link
            .as_ref()
            .and_then(|link| link.bubbles.as_ref())
            .filter(|_| time_based && self.main_series_visible)
        {
            let x = |i| idx_to_coord(start_idx + i, chart_rect.min.x);
            bubbles.render(&clipped_painter, visible_data, x, |p| {
                price_map.price_to_y(p)
            });
        }

        // Bar-indexed studies have no position on an element time scale.
        let hidden_studies = crate::studies::IndicatorRegistry::new();
        let studies = if time_based {
            &self.studies
        } else {
            &hidden_studies
        };
        // Draw indicators
        renderers::IndicatorRenderer::render(
            &price_ctx,
            studies.indicators(),
            visible_data,
            &price_map,
            &coords,
        );

        // Resolve a click on the main chart area into a selection. Overlay
        // indicators take priority over the series beneath them; an empty-area
        // click clears the selection. Clicks in a study's pane are resolved
        // where the panes are rendered, outside this rect.
        let main_visible_range = start_idx..(start_idx + visible_data.len());
        if response.clicked()
            && let Some(click_pos) = response.interact_pointer_pos()
        {
            let volume_coords = if show_volume {
                Some(coords.with_rect(volume_rect))
            } else {
                None
            };
            let hit = hit_test_main_chart(
                click_pos,
                studies,
                &self.study_ids,
                &coords,
                volume_coords.as_ref(),
                self.state.shown(),
                max_volume,
                main_visible_range.clone(),
            )
            .filter(|(id, _)| {
                self.main_series_visible
                    || matches!(id, crate::widget::ChartElementId::Indicator(_))
            });
            match hit {
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
            let drawing_hit = self.drawings.hit_test(click_pos).filter(|_| time_based);

            self.right_click = Some(if let Some(drawing_id) = drawing_hit {
                // Right-clicking a drawing selects it and
                // clears any series/indicator selection so handles don't show
                // on two objects at once.
                self.selection.deselect();
                self.drawings.select(drawing_id);
                RightClickTarget::Drawing {
                    drawing_id,
                    pos: click_pos,
                }
            } else {
                let volume_coords = if show_volume {
                    Some(coords.with_rect(volume_rect))
                } else {
                    None
                };
                let element_hit = hit_test_main_chart(
                    click_pos,
                    studies,
                    &self.study_ids,
                    &coords,
                    volume_coords.as_ref(),
                    self.state.shown(),
                    max_volume,
                    main_visible_range.clone(),
                )
                .filter(|(id, _)| {
                    self.main_series_visible
                        || matches!(id, crate::widget::ChartElementId::Indicator(_))
                });
                match element_hit {
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

        // Draw selection handles for whichever element is currently selected on
        // the main chart (overlay indicator line or series data points).
        self.render_main_chart_selection(&painter, &coords, visible_data, main_visible_range);

        // Render bar marks (Widget API annotations)
        if time_based && self.main_series_visible && !self.marks.is_empty() {
            renderers::render_markers(
                &clipped_price_ctx,
                &self.marks,
                visible_data,
                &price_map,
                &coords,
            );
        }

        // The axis labels go at these depths, painted once the crosshair's tags are placed
        let price_labels = painter.add(egui::Shape::Noop);

        let axis_edge = if self.config.show_right_axis {
            rect.max.x
        } else {
            price_rect.left()
        };
        let show_price_axis = self.config.show_right_axis || self.config.show_left_axis;
        let has_currency = self
            .instrument()
            .and_then(|instrument| instrument.currency.as_deref())
            .is_some_and(|code| !code.is_empty());
        let mut tags = Vec::new();
        if has_currency {
            tags.push(Some(Rect::from_min_size(
                price_rect.min,
                egui::vec2(price_rect.width(), 24.0),
            )));
        }
        let mut pending_values = Vec::new();
        for line in &self.price_lines {
            rendering::render_price_line(
                &painter,
                price_rect,
                &price_map,
                line,
                price_rect.left(),
                false,
                axis_edge,
            );
            if line.axis_label {
                pending_values.push((line.price, line.color));
            }
        }

        if self.main_series_visible
            && (self.config.show_symbol_prev_close || self.config.show_prev_close_line)
            && let Some(last) = self.state.data().bars.last()
            && let Some(index) = self
                .state
                .data()
                .bars
                .partition_point(|bar| {
                    exchange_session.trading_date(bar.time)
                        < exchange_session.trading_date(last.time)
                })
                .checked_sub(1)
            && let Some(previous) = self.state.data().bars.get(index)
        {
            let line = PriceLine::new(previous.close, ui.active_theme().ui.text_secondary)
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
                pending_values.push((previous.close, line.color));
            }
        }

        // Last price line & label, from the newest bar even when it is scrolled out of view
        let last_price_tag = if let Some(last) = self
            .state
            .data()
            .bars
            .last()
            .filter(|_| self.main_series_visible)
        {
            let color = if last.close >= last.open {
                self.config.bullish_color
            } else {
                self.config.bearish_color
            };
            let line = PriceLine::new(last.close, color)
                .with_width(if self.config.show_price_line {
                    DESIGN_TOKENS.stroke.hairline
                } else {
                    0.0
                })
                .with_style(crate::model::LineStyle::Dotted)
                .with_axis_label(show_price_axis && self.config.show_symbol_last_val);
            let countdown = self.current_bar_countdown();
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
                    .filter(|_| show_price_axis)
                    .map(|quote| (quote, egui::Color32::from_rgb(251, 142, 1))),
                (show_volume && show_price_axis)
                    .then_some((last.volume, egui::Color32::from_rgb(233, 30, 99))),
                axis_edge,
                countdown.as_deref(),
                self.instrument()
                    .and_then(|instrument| instrument.volume_decimals),
                if has_currency { 24.0 } else { 0.0 },
            );
            tags.extend([quote_tags[1], quote_tags[2]]);
            quote_tags[0]
        } else {
            None
        };

        tags.push(last_price_tag);
        if show_price_axis {
            for (value, color) in &pending_values {
                if price_map
                    .range()
                    .contains(price_map.transform().to_scale(*value))
                {
                    let label = price_map.transform().label(
                        price_map.transform().to_scale(*value),
                        price_map.price_precision().unwrap_or(2),
                    );
                    tags.push(rendering::axes::render_axis_value(
                        &painter,
                        price_rect,
                        axis_edge,
                        price_map.price_to_y(*value),
                        label,
                        (rendering::axes::tag_text_color(*color), *color),
                        &tags,
                    ));
                }
            }
        }
        if show_price_axis && self.config.show_indicator_last_val {
            for study in studies
                .indicators()
                .iter()
                .filter(|study| study.is_overlay())
            {
                rendering::axes::render_study_tags(
                    &painter,
                    price_rect,
                    &price_map,
                    study.as_ref(),
                    axis_edge,
                    &mut tags,
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
                values.extend([
                    (high, "High", self.config.bullish_color),
                    (low, "Low", self.config.bearish_color),
                ]);
            }
            if self.config.show_bid_ask
                && let Some(quote) = self.quote.as_ref().filter(|quote| {
                    quote.bid.is_finite()
                        && quote.ask.is_finite()
                        && quote.bid <= quote.ask
                        && quote.spread().is_finite()
                })
            {
                values.extend([
                    (quote.bid, "Bid", self.config.bearish_color),
                    (quote.ask, "Ask", self.config.bullish_color),
                ]);
            }
            for (value, name, color) in values {
                let line = PriceLine::new(value, color)
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
                    let label = format!(
                        "{name} {}",
                        price_map.transform().label(
                            price_map.transform().to_scale(value),
                            price_map.price_precision().unwrap_or(2)
                        )
                    );
                    tags.push(rendering::axes::render_axis_value(
                        &painter,
                        price_rect,
                        axis_edge,
                        price_map.price_to_y(value),
                        label,
                        (egui::Color32::WHITE, color),
                        &tags,
                    ));
                }
            }
        }

        let time_labels = painter.add(egui::Shape::Noop);

        // Handle drawing tools
        if time_based {
            // Clone/extract values before mutable borrow of self
            let timescale = self.state.time_scale().clone();

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
            );

            // Render eraser highlight if in eraser mode
            self.render_eraser_highlight(&painter, &drawings, &cursor_modes);

            // Put them back
            self.drawings = drawings;
            self.cursor_modes = cursor_modes;
        }

        if !time_based
            && (!self.study_ids.is_empty()
                || !self.drawings.drawings.is_empty()
                || !self.marks.is_empty())
        {
            painter.text(
                price_rect.left_bottom() + Vec2::new(8.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                "Studies, drawings and markers are not shown on this chart type",
                egui::FontId::proportional(typography::SM),
                self.config.text_color,
            );
        }

        // Render "Jump to Latest" button
        if self.config.show_realtime_btn {
            let btn_id = ui.scope_id().with("jump_to_latest");
            let btn_size = Vec2::new(
                DESIGN_TOKENS.sizing.charts_ext.realtime_button_width,
                DESIGN_TOKENS.sizing.button_md,
            );
            let btn_pos = Pos2::new(
                price_rect.center().x - btn_size.x / 2.0,
                price_rect.min.y + DESIGN_TOKENS.spacing.lg + DESIGN_TOKENS.spacing.xs,
            );
            let btn_rect = Rect::from_min_size(btn_pos, btn_size);
            let btn_res = ui.interact(btn_rect, btn_id, egui::Sense::click());

            rendering::render_realtime_btn(
                &painter,
                price_rect,
                near_live_edge,
                self.config.show_realtime_btn,
                self.config.realtime_button_size,
                self.config.realtime_button_pos,
                self.config.realtime_button_color,
                self.config.realtime_button_hover_color,
                self.config.realtime_button_text_color,
                self.config.realtime_button_text.as_deref(),
                btn_res.hovered(),
            );
        }

        let bar_seconds = self.time_label_seconds();

        // Draw crosshair with options from chart_options. The pointer still hovers the chart
        // while a context menu is open, so the crosshair, its labels and the tooltip wait until
        // the menu closes.
        let menu_open = std::mem::take(&mut self.menu_open);
        let mut crosshair_tags = [None; 2];
        self.cursor_modes.update(ui.input(|input| input.stable_dt));
        if self.chart_options.crosshair.style == crate::config::CrosshairStyle::Demonstration
            && !self.drawings.eraser_mode
        {
            self.cursor_modes
                .move_pointer(response.hover_pos().filter(|pos| price_rect.contains(*pos)));
            self.cursor_modes.render(ui, price_rect);
        } else {
            self.cursor_modes.move_pointer(None);
        }
        let hover_rect =
            Rect::from_min_max(price_rect.min, Pos2::new(price_rect.right(), time_axis_top));
        let hover_pos = self
            .axis_hover
            .filter(|(_, tag)| {
                ui.input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pos| tag.contains(pos))
                })
            })
            .map(|(pos, _)| pos)
            .or_else(|| {
                ui.input(|input| input.pointer.hover_pos())
                    .filter(|pos| hover_rect.contains(*pos) && ui.rect_contains_pointer(hover_rect))
            });
        let mut study_action = None;
        {
            let visible_data = self.state.visible_data();
            let selected = hover_pos
                .and_then(|pos| coords.local_idx_at_x(pos.x, visible_data.len()))
                .map(|index| start_idx + index)
                .unwrap_or_else(|| self.state.shown().len().saturating_sub(1));
            if let Some(bar) = self.state.shown().get(selected) {
                let timeframe = self.timeframe.map(|tf| tf.as_str()).unwrap_or_default();
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
                legend.instrument = self.instrument();
                legend.badge = self.legend_badge.clone();
                legend.statuses = &statuses;
                legend.quote = self
                    .quote
                    .as_ref()
                    .filter(|_| self.main_series_visible && self.config.show_quote_boxes);
                legend.show_title = self.main_series_visible && self.config.show_symbol_labels;
                legend.title_as_description = self.config.title_as_description;
                legend.show_ohlc = self.main_series_visible && self.config.show_ohlc_info;
                legend.show_change = self.main_series_visible && self.config.show_ohlc_change;
                legend.show_change_percent =
                    self.main_series_visible && self.config.show_ohlc_change_percent;
                legend.show_indicator_titles = self.config.show_indicator_labels;
                legend.show_indicator_inputs = self.config.show_indicator_inputs;
                legend.show_indicator_values = self.config.show_indicator_last_val;
                let inputs: Vec<_> = self.study_inputs.iter().map(String::as_str).collect();
                legend.study_inputs = &inputs;
                legend.previous_close = selected
                    .checked_sub(1)
                    .and_then(|index| self.state.shown().get(index))
                    .map(|bar| bar.close);
                legend.bar_index = selected;
                legend.studies = if time_based {
                    self.studies.indicators()
                } else {
                    &[]
                };
                legend.show_volume = self.main_series_visible
                    && self.chart_type.supports_volume()
                    && self.config.show_legend_volume;
                let (quote_action, study_settings) = ui
                    .push_id(shown.id, |ui| {
                        legend.show_with_colors(ui, price_rect, &colors)
                    })
                    .inner;
                if let Some(action) = quote_action {
                    self.quote_action = Some(action);
                }
                study_action = study_settings;
            }
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
                let held = ui.input(|input| {
                    input.pointer.primary_down()
                        && (!input.any_touches()
                            || input
                                .pointer
                                .press_start_time()
                                .is_some_and(|start| input.time - start >= 0.5))
                });
                let held_values = tooltip_options.enabled
                    && tooltip_options.on_long_press
                    && held
                    && !self.drawings.eraser_mode
                    && self.drawings.active_tool.is_none();
                let mut crosshair_options = self.chart_options.crosshair;
                if held_values || self.drawings.eraser_mode {
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
                crosshair_options.mode = crate::config::CrosshairMode::Normal;
                let mut main_options = crosshair_options;
                if volume_rect.contains(hover_pos) {
                    main_options.horz_line_visible = false;
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
                    formatter.as_ref(),
                    bar_seconds,
                );

                if volume_rect.contains(hover_pos) {
                    let map = crate::scales::PriceMap::new(
                        crate::scales::PriceTransform::default(),
                        crate::scales::PriceRange::new(0.0, max_volume.max(1.0)),
                        volume_rect.top(),
                        volume_rect.bottom(),
                        false,
                    )
                    .with_price_precision(Some(
                        self.instrument()
                            .and_then(|instrument| instrument.volume_decimals)
                            .unwrap_or(0),
                    ));
                    let mut pos = hover_pos;
                    if let Some(index) = index {
                        pos.x = coords.idx_to_x(index);
                        if self.chart_options.crosshair.mode == crate::config::CrosshairMode::Magnet
                        {
                            pos.y = map.price_to_y(self.state.shown()[index].volume);
                        }
                    }
                    let mut options = crosshair_options.resolved(ui.ctx());
                    options.vert_line_visible = false;
                    let context = RenderContext::new(&painter, volume_rect);
                    renderers::render_crosshair_lines(
                        &context,
                        pos,
                        &options,
                        volume_rect.bottom(),
                    );
                    crosshair_tags[0] = renderers::render_crosshair_price_tag(
                        &context, pos, &map, &options, axis_edge, None,
                    );
                }

                if tooltip_options.on_long_press
                    && ui.input(|input| input.any_touches() && input.pointer.primary_down())
                {
                    ui.ctx().request_repaint();
                }
                if tooltip_options.enabled
                    && (!tooltip_options.on_long_press || held)
                    && !self.drawings.eraser_mode
                    && self.drawings.active_tool.is_none()
                    && let Some(local_idx) = coords.local_idx_at_x(hover_pos.x, visible_data.len())
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
                        for (index, bar) in visible_data.iter().enumerate().step_by(25) {
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
                    rendering::render_tooltip_with_options(
                        &price_ctx,
                        hover_pos,
                        candle,
                        (start_idx + local_idx)
                            .checked_sub(1)
                            .and_then(|index| self.state.shown().get(index))
                            .map(|bar| bar.close),
                        &tooltip_options,
                        &colors,
                        &price_map,
                        &coords,
                        visible_data,
                        formatter.as_ref(),
                        bar_seconds,
                        &self.study_values_at(start_idx + local_idx),
                        self.instrument(),
                    );
                }
            }
        } else {
            // Clear hover bar index when not hovering locally
            self.last_hover_bar_idx = None;

            // Render synced crosshair from other charts (if available)
            {
                if !menu_open && let Some(bar_idx) = self.synced_crosshair_bar_idx {
                    // Convert bar index to screen x coordinate
                    let x = coords.idx_to_x(bar_idx.round() as usize);
                    if coords.is_x_visible(x) {
                        // Create a synthetic hover position at the center of the price range
                        let center_y = price_rect.center().y;
                        let synced_pos = Pos2::new(x, center_y);

                        crosshair_tags = rendering::render_crosshair_with_options(
                            &price_ctx,
                            synced_pos,
                            visible_data,
                            &price_map,
                            &coords,
                            &self.chart_options.crosshair,
                            time_axis_top,
                            axis_edge,
                            last_price_tag,
                            formatter.as_ref(),
                            bar_seconds,
                        );
                    }
                }
            }
        }

        self.axis_hover = hover_pos.zip(crosshair_tags[0]);
        #[cfg(feature = "icons")]
        if show_price_axis {
            if let Some((pos, tag)) = self
                .axis_hover
                .filter(|(pos, _)| price_rect.contains(*pos) && !volume_rect.contains(*pos))
            {
                let price = if self.chart_options.crosshair.mode
                    == crate::config::CrosshairMode::Magnet
                    && let Some(index) = coords.local_idx_at_x(pos.x, visible_data.len())
                {
                    price_map.y_to_price(self.snap_crosshair(pos, &price_map, start_idx + index).y)
                } else {
                    price_map.y_to_price(pos.y)
                };
                let foreground = self
                    .chart_options
                    .crosshair
                    .resolved(ui.ctx())
                    .label_text_color;
                if let Some(price) =
                    rendering::axes::crosshair_price_action(ui, tag, price, foreground)
                {
                    self.right_click = Some(RightClickTarget::Background {
                        pos: tag.left_bottom(),
                        price,
                    });
                }
            }
            if rendering::axes::axis_settings_button(ui, axis_corner).clicked() {
                self.scale_settings_requested = true;
            }
        }
        if time_based
            && let Some(id) = renderers::render_event_band(
                ui,
                &self.events,
                visible_data,
                &coords,
                price_rect.bottom(),
            )
        {
            self.event_click = Some(id);
        }

        let current_price = self
            .state
            .data()
            .bars
            .last()
            .filter(|_| self.main_series_visible && self.config.show_symbol_last_val)
            .map(|bar| {
                price_map.transform().label(
                    price_map.transform().to_scale(bar.close),
                    price_map.price_precision().unwrap_or(2),
                )
            });
        self.measured_axis_width = rendering::axes::price_axis_width(
            &painter,
            &price_map,
            &current_price.as_deref().into_iter().collect::<Vec<_>>(),
        );
        for tag in tags.iter().skip(usize::from(has_currency)).flatten() {
            self.measured_axis_width = self.measured_axis_width.max(tag.width() + 5.0 + 8.0);
        }

        if show_price_axis
            && let Some(currency) = self
                .instrument()
                .and_then(|instrument| instrument.currency.as_deref())
                .filter(|code| !code.is_empty())
        {
            let currency_width = painter
                .layout_no_wrap(
                    currency.to_owned(),
                    rendering::axes::axis_font(ui.ctx()),
                    colors.text,
                )
                .size()
                .x
                + 26.0;
            let axis = Rect::from_min_max(
                Pos2::new(
                    if self.config.show_right_axis {
                        price_rect.right()
                    } else {
                        rect.left()
                    },
                    price_rect.top(),
                ),
                Pos2::new(axis_edge, price_rect.bottom()),
            );
            let (button, choice) =
                renderers::render_axis_currency(ui, axis, currency, &self.currency_choices);
            self.measured_axis_width = self.measured_axis_width.max(currency_width);
            if choice.is_some() {
                self.currency_choice = choice;
            }
            tags.push(Some(button.rect));
        }

        // Draw the axis labels, leaving out those the tags overlap
        tags.extend(crosshair_tags);
        if self.config.show_right_axis {
            rendering::render_price_labels(&price_ctx, &price_map, &colors, &tags, price_labels);
        } else if self.config.show_left_axis {
            renderers::render_price_labels_at_pos(
                &price_ctx,
                &price_map,
                &colors,
                crate::scales::PriceScaleId::Left,
                &tags,
                price_labels,
            );
        }
        if self.config.show_time_labels {
            let axis_context_rect =
                Rect::from_min_max(chart_rect.min, Pos2::new(chart_rect.right(), time_axis_top));
            let chart_ctx = RenderContext::new(&painter, axis_context_rect);
            rendering::render_time_labels(
                &chart_ctx,
                visible_data,
                &coords,
                &colors,
                formatter.as_ref(),
                &tags,
                self.chart_options.time_scale.ticks_visible,
                time_labels,
            );
        }

        // Draw box zoom rect
        rendering::render_box_zoom(&painter, &self.box_zoom);

        if let Some((nav, buttons)) = navigation {
            self.paint_navigation(ui, nav, buttons);
        }

        // Focus ring for keyboard accessibility
        crate::styles::focus::draw_focus_ring(ui, &shown, 0.0);

        if let Some(action) = study_action {
            self.apply_study_action(action);
        }
        shown
    }
}

#[cfg(test)]
mod tests {
    use crate::chart::series::SeriesSettings;
    use crate::scales::PriceScaleMode;
    use crate::widget::test_frames::{chart, frame, shapes};
    use egui::{Event, Pos2, Rect, Shape};

    #[test]
    fn countdown_uses_feed_calendar_and_session_trading_dates() {
        use crate::data::{Datafeed, HistoryPage, HistoryRequest, Reply};
        use crate::model::{
            Bar, Instrument, Level1Quote, Session, SessionWindow, Timeframe, TimezoneId,
        };
        use chrono::{
            NaiveTime,
            Weekday::{Fri, Mon, Thu, Tue, Wed},
        };
        struct Feed;
        impl Datafeed for Feed {
            fn history(&self, _: HistoryRequest, _: Reply<HistoryPage>) {
                unreachable!("this test only reads the attached calendar");
            }
        }
        for (interval, open, now, session, expected) in [
            (
                "7min",
                "2024-03-31T00:59:00Z",
                "2024-03-31T00:59:30Z",
                false,
                Some("00:30"),
            ),
            (
                "1D",
                "2024-03-10T23:00:00Z",
                "2024-03-11T14:59:00Z",
                true,
                Some("01:00"),
            ),
            (
                "1W",
                "2024-03-10T23:00:00Z",
                "2024-03-15T14:59:00Z",
                true,
                Some("01:00"),
            ),
            (
                "1M",
                "2024-02-29T23:00:00Z",
                "2024-03-29T14:59:00Z",
                true,
                Some("01:00"),
            ),
            (
                "10t",
                "2024-03-11T14:59:00Z",
                "2024-03-11T14:59:30Z",
                false,
                None,
            ),
        ] {
            let timeframe: Timeframe = interval.parse().unwrap();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.timeframe = Some(timeframe);
            chart.chart_options.time_scale.timezone =
                crate::scales::TimezoneMode::Exchange(chrono_tz::Europe::Zurich);
            chart.attach_feed(std::sync::Arc::new(Feed), 1);
            let mut instrument =
                Instrument::new("TEST").with_time_zone(TimezoneId::named("Europe/Zurich").unwrap());
            if session {
                instrument.session = Some(Session::new([SessionWindow::new(
                    [Mon, Tue, Wed, Thu, Fri],
                    NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                    NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
                )]));
            }
            chart.set_instrument(Some(instrument));
            // Metadata resolves Exchange timezone before the bar is assigned to its slot.
            chart.feed_link.as_mut().unwrap().clock = chart.bar_clock();
            chart.set_data(
                crate::widget::SeriesId::MAIN,
                vec![Bar::new(
                    open.parse().unwrap(),
                    100.0,
                    110.0,
                    100.0,
                    110.0,
                    1.0,
                )],
            );
            chart.set_quote(Some(Level1Quote {
                symbol: "TEST".into(),
                ts: now.parse().unwrap(),
                bid: 109.0,
                ask: 111.0,
                bid_size: 1.0,
                ask_size: 1.0,
            }));
            assert_eq!(
                chart.current_bar_countdown().as_deref(),
                expected,
                "{interval}"
            );
        }
    }

    #[test]
    fn footprint_stays_below_legend_at_wide_and_narrow_sizes() {
        use crate::chart::overlays::{FootprintColor, FootprintConfig};
        use crate::data::Footprint;
        use crate::model::{BarClock, Tick, Timeframe, TradeSide};
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (width, pane) in [(1440.0, false), (600.0, false), (600.0, true)] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = chart(&[100.0], &[110.0]);
                chart.set_symbol("FLOW");
                #[cfg(feature = "studies")]
                if pane {
                    chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
                }
                let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
                flow.push(&Tick::new(
                    chart
                        .bars(crate::widget::SeriesId::MAIN)
                        .last()
                        .unwrap()
                        .time,
                    100.0,
                    2.0,
                    Some(TradeSide::Buy),
                ))
                .unwrap();
                let config = FootprintConfig {
                    color_by: FootprintColor::Volume,
                    neutral_color: egui::Color32::from_rgb(21, 45, 78),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(width, 500.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        chart.show_footprint(ui, &flow, &config);
                    },
                );
                output.textures_delta.clear();
                assert!(
                    output.shapes.iter().any(|shape| match &shape.shape {
                        Shape::Rect(rect) => rect.fill == theme.chart.volume_up(),
                        _ => false,
                    }),
                    "footprint keeps the volume histogram"
                );
                let cells = output
                    .shapes
                    .iter()
                    .position(|shape| match &shape.shape {
                        Shape::Mesh(mesh) => mesh
                            .vertices
                            .iter()
                            .any(|v| v.color == config.neutral_color.gamma_multiply(0.5)),
                        _ => false,
                    })
                    .expect("footprint cells");
                let legend = output
                    .shapes
                    .iter()
                    .position(|shape| match &shape.shape {
                        Shape::Text(text) => text.galley.text().contains("FLOW"),
                        _ => false,
                    })
                    .expect("chart legend");
                assert!(
                    cells < legend,
                    "width {width}, pane {pane}: cells cover legend"
                );
            }
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn status_visibility_reaches_the_chart_legend() {
        use crate::model::{Instrument, Level1Quote};
        for (description, hidden) in [
            (true, 0),
            (false, 0),
            (true, 1),
            (true, 2),
            (true, 3),
            (true, 4),
            (true, 5),
            (true, 6),
            (true, 7),
            (true, 8),
            (true, 9),
            (true, 10),
            (true, 11),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.set_instrument(Some(
                Instrument::new("TICKER").with_description("Instrument description"),
            ));
            let bar = chart.state.data().bars.last().unwrap();
            chart.set_quote(Some(Level1Quote {
                symbol: "TICKER".into(),
                ts: bar.time,
                bid: 109.0,
                ask: 111.0,
                bid_size: 1.0,
                ask_size: 1.0,
            }));
            let study = chart.add_indicator(Box::new(crate::studies::SMA::new(1)));
            chart.set_indicator_inputs(study, "1 close");
            let oscillator = chart.add_indicator(Box::new(crate::studies::RSI::new(2)));
            chart.set_indicator_inputs(oscillator, "2 close");
            let config = chart.config_mut();
            config.title_as_description = description;
            config.show_volume = hidden != 10;
            config.show_symbol_labels = hidden != 1;
            config.show_quote_boxes = hidden != 2;
            config.show_ohlc_info = hidden != 3;
            config.show_ohlc_change = hidden != 4;
            config.show_ohlc_change_percent = hidden != 9;
            config.show_legend_volume = hidden != 5;
            config.show_indicator_labels = hidden != 6;
            config.show_indicator_inputs = hidden != 7;
            config.show_indicator_last_val = hidden != 8;
            chart.set_main_series_visible(hidden != 11);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            assert_eq!(painted.iter().any(|shape| matches!(shape, Shape::Rect(rect)
                if rect.fill == chart.config.bullish_color && chart.last_rendered_price_rect.contains(rect.rect.center()))), hidden != 11, "main series body: hidden={hidden}");
            assert_eq!(chart.bars(crate::widget::SeriesId::MAIN).len(), 100);
            let texts: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) if text.pos.x < chart.last_rendered_price_rect.right() => {
                        Some(text.galley.text())
                    }
                    _ => None,
                })
                .collect();
            for (text, flag) in [
                ("SELL", 2),
                ("BUY", 2),
                ("SMA", 6),
                ("1 close", 7),
                ("110.00", 8),
                ("O 100.00", 3),
                ("Vol 1", 5),
            ] {
                assert_eq!(
                    texts.contains(&text),
                    hidden != flag && (hidden != 11 || matches!(flag, 6..=8)),
                    "{text}: hidden={hidden} {texts:?}"
                );
            }
            assert_eq!(
                texts.iter().any(|text| text.contains("RSI")),
                hidden != 6,
                "{texts:?}"
            );
            assert_eq!(
                texts.iter().any(|text| text.contains("2 close")),
                hidden != 7,
                "{texts:?}"
            );
            let title = if description {
                "Instrument description"
            } else {
                "TICKER"
            };
            assert_eq!(
                texts.iter().any(|text| text.starts_with(title)),
                hidden != 1 && hidden != 11,
                "{texts:?}"
            );
            assert_eq!(
                chart.last_rendered_volume_rect.is_positive(),
                hidden != 10 && hidden != 11,
                "legend visibility and histogram visibility are independent"
            );
        }
    }

    #[test]
    fn price_labels_and_lines_are_independent_on_either_axis() {
        use crate::model::{Level1Quote, Timeframe};
        for left in [false, true] {
            for (label, line, countdown) in [
                (true, true, true),
                (true, false, false),
                (false, true, false),
                (false, false, false),
            ] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[100.0], &[110.0, 120.0]);
                chart.set_timeframe(Timeframe::Min1);
                chart.config.show_right_axis = !left;
                chart.config.show_left_axis = left;
                chart.config.show_volume = false;
                chart.config.show_symbol_labels = false;
                chart.config.show_ohlc_info = false;
                chart.config.show_ohlc_change = false;
                chart.config.show_ohlc_change_percent = false;
                chart.config.show_symbol_last_val = label;
                chart.config.show_price_line = line;
                chart.config.show_symbol_prev_close = label;
                chart.config.show_prev_close_line = line;
                chart.config.show_countdown = countdown;
                let bar = chart.state.data().bars.last().unwrap();
                chart.set_quote(Some(Level1Quote {
                    symbol: "TEST".into(),
                    ts: bar.time + chrono::Duration::seconds(30),
                    bid: 119.0,
                    ask: 121.0,
                    bid_size: 1.0,
                    ask_size: 1.0,
                }));
                chart.config.show_quote_boxes = false;
                chart.set_session_quote(Some(crate::model::quote::SessionQuote::new(
                    115.0, "EXTENDED",
                )));
                chart.set_price_lines(
                    crate::chart::series::SeriesId::MAIN,
                    vec![crate::model::PriceLine::new(115.0, egui::Color32::GOLD)],
                );
                frame(&ctx, &mut chart, 0.0, vec![]);
                let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                let plot = chart.last_rendered_price_rect;
                let custom_tag = painted
                    .iter()
                    .find_map(|shape| match shape {
                        Shape::Rect(tag) if tag.fill == egui::Color32::GOLD => Some(tag.rect),
                        _ => None,
                    })
                    .expect("custom price line retains its axis label");
                assert!(if left {
                    custom_tag.right() <= plot.left()
                } else {
                    custom_tag.left() >= plot.right()
                });
                let session_text = painted
                    .iter()
                    .find_map(|shape| match shape {
                        Shape::Text(text) if text.galley.text().starts_with("EXTENDED") => {
                            Some(text)
                        }
                        _ => None,
                    })
                    .expect("session tag");
                assert!(
                    session_text.pos.x
                        >= plot.left()
                            - chart.config.left_axis_width.max(chart.measured_axis_width),
                    "session label is clipped on the left"
                );
                let close_y = chart.last_price_map.price_to_y(120.0);
                let prev_y = chart.last_price_map.price_to_y(110.0);
                let price_tags: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Rect(rect)
                            if rect.fill == chart.config.bullish_color
                                && (rect.rect.center().y - close_y).abs() < 1.0
                                && rect.rect.width() > 20.0 =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(!price_tags.is_empty(), label, "{left} {label} {line}");
                for tag in price_tags {
                    assert!(if left {
                        tag.right() <= plot.left()
                    } else {
                        tag.left() >= plot.right()
                    });
                }
                for (y, color) in [
                    (close_y, chart.config.bullish_color),
                    (
                        prev_y,
                        crate::theme::ThemeExt::active_theme(&ctx).ui.text_secondary,
                    ),
                ] {
                    let has_line = painted.iter().any(|shape| match shape {
                        Shape::LineSegment { points, stroke } => {
                            stroke.color == color
                                && (points[0].y - y).abs() < 0.01
                                && (points[1].y - y).abs() < 0.01
                        }
                        Shape::Mesh(mesh) => {
                            mesh.vertices.len() > 8
                                && mesh.vertices.iter().all(|vertex| {
                                    vertex.color == color && (vertex.pos.y - y).abs() <= 1.5
                                })
                        }
                        _ => false,
                    });
                    assert_eq!(
                        has_line, line,
                        "line at {y}: left={left} label={label} line={line}"
                    );
                }
                assert_eq!(painted.iter().any(|shape| matches!(shape, Shape::Text(text) if text.galley.text().contains("00:30"))), countdown);
            }
        }
    }

    /// On a logarithmic scale 100 lies halfway between 10 and 1,000 (ln 100 is the mean of ln 10 and
    /// ln 1,000); a linear mapping puts it a tenth of the way up. A range zoomed by hand before the
    /// switch, current or in the zoom history, is in the old mode's units and must not carry over.
    #[test]
    fn log_mode_reaches_the_rendered_mapping() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[1000.0]);
        frame(&ctx, &mut chart, 0.0, Vec::new());
        chart.state.price_scale_mut().zoom(0.5, 500.0);
        chart.state.push_zoom_state();
        chart.apply_series_settings(&SeriesSettings {
            price_scale_mode: PriceScaleMode::Logarithmic,
            ..SeriesSettings::default()
        });
        chart.state.pop_zoom_state();
        frame(&ctx, &mut chart, 0.1, Vec::new());
        let map = chart.get_chart_mapping();
        let (y10, y100, y1000) = (
            map.price_to_y(10.0),
            map.price_to_y(100.0),
            map.price_to_y(1000.0),
        );
        assert!(
            (y100 - (y10 + y1000) / 2.0).abs() < 0.01,
            "{y10} {y100} {y1000}"
        );
        let (top, bottom) = map.price.extent();
        assert!(
            top < y1000 && y10 < bottom,
            "{y10}..{y1000} outside {top}..{bottom}"
        );
    }

    /// Axis buttons route their clicks without changing the price range or viewport.
    #[cfg(feature = "icons")]
    #[test]
    fn axis_buttons_route_without_scaling() {
        for (settings, magnet) in [(false, false), (true, false), (false, true)] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[110.0]);
            if magnet {
                chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Magnet;
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            let plot = chart.last_rendered_price_rect;
            frame(
                &ctx,
                &mut chart,
                0.1,
                vec![Event::PointerMoved(plot.center() + egui::vec2(0.0, 10.0))],
            );
            let map = chart.get_chart_mapping();
            let range = map.price.range();
            let viewport = chart.get_time_scale_state();
            let pos = if settings {
                Pos2::new(
                    plot.right() + chart.config.right_axis_width / 2.0,
                    plot.bottom() + 14.0,
                )
            } else {
                let (_, tag) = chart.axis_hover.expect("hover tag");
                tag.left_center() + egui::Vec2::new(10.0, if magnet { 5.0 } else { 0.0 })
            };
            for (step, events) in [
                vec![Event::PointerMoved(pos)],
                vec![navigation_press(pos, true)],
                vec![navigation_press(pos, false)],
            ]
            .into_iter()
            .enumerate()
            {
                frame(&ctx, &mut chart, 0.2 + step as f64 * 0.1, events);
            }
            let expected_price = if magnet {
                100.0
            } else {
                map.y_to_price(plot.center().y + 10.0)
            };
            if settings {
                assert!(chart.take_scale_settings_request());
                assert!(!chart.take_scale_settings_request());
            } else {
                assert!(
                    matches!(chart.take_right_click(), Some(super::RightClickTarget::Background { price, .. }) if (price - expected_price).abs() < 0.001)
                );
            }
            assert_eq!(chart.get_chart_mapping().price.range().min, range.min);
            assert_eq!(chart.get_chart_mapping().price.range().max, range.max);
            assert_eq!(chart.get_time_scale_state(), viewport);
        }
    }

    /// Held values appear only for a pressed pointer and disappear on release or disable.
    #[test]
    fn held_values_follow_press_release_and_switch() {
        for (_, theme) in crate::theme::presets() {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0], &[110.0, 120.0]);
            chart.config.bullish_color = egui::Color32::BLUE;
            chart.config.show_ohlc_info = true;
            chart.chart_options.tooltip.on_long_press = true;
            chart.chart_options.tooltip.show_time = false;
            frame(&ctx, &mut chart, 0.0, vec![]);
            let map = chart.get_chart_mapping();
            let pos = Pos2::new(map.idx_to_x(50), map.rect.center().y);
            for (step, enabled, pressed, expected) in [
                (1, true, false, false),
                (2, true, true, true),
                (3, true, false, false),
                (4, false, true, false),
                (5, false, false, false),
            ] {
                chart.set_tooltip_enabled(enabled);
                let painted = shapes(
                    &ctx,
                    &mut chart,
                    step as f64 * 0.1,
                    vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                );
                let labels: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Text(text) => Some(text.galley.text()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    labels.contains(&"Open"),
                    expected,
                    "step {step}: {labels:?}"
                );
                assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(text)
                    if text.galley.text() == "O 100.00" && text.galley.job.sections[1].format.color == egui::Color32::BLUE)));
                if expected {
                    assert!(labels.contains(&"-10.00 (-8.33%)"));
                    assert!(labels.contains(&"Vol"));
                    let legend = labels
                        .iter()
                        .position(|label| *label == "O 100.00")
                        .unwrap();
                    let tooltip = labels.iter().position(|label| *label == "Open").unwrap();
                    assert!(
                        tooltip > legend,
                        "the floating readout must paint above the legend"
                    );
                    assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(text)
                        if text.galley.text() == "110.00" && text.galley.job.sections[0].format.color == egui::Color32::BLUE)));
                }
            }
        }
    }

    #[test]
    fn configured_sessions_reach_breaks_and_extended_shading() {
        use crate::model::{Bar, BarData, Timeframe};
        let times = [
            "2024-03-08T23:00:00Z",
            "2024-03-11T07:59:00Z",
            "2024-03-11T08:00:00Z",
            "2024-03-11T13:00:00Z",
            "2024-03-11T13:30:00Z",
            "2024-03-11T20:00:00Z",
            "2024-03-11T23:59:00Z",
            "2024-03-12T00:00:00Z",
        ];
        for (shade, custom, metadata, expected) in [
            (true, false, false, vec![2]),
            (false, false, false, vec![2]),
            (true, true, false, vec![1, 6]),
            (true, false, true, vec![3]),
        ] {
            let ctx = egui::Context::default();
            let bars = times
                .iter()
                .map(|time| Bar::new(time.parse().unwrap(), 100.0, 102.0, 99.0, 101.0, 1.0))
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Hour1);
            if metadata {
                use crate::model::{Instrument, Session, SessionWindow, TimezoneId};
                use chrono::{NaiveTime, Weekday::*};
                let weekdays = [Mon, Tue, Wed, Thu, Fri];
                let clock = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
                // The instrument opens pre-market at 05:00, independent of the display zone.
                chart.set_instrument(Some(
                    Instrument::new("SESSION")
                        .with_time_zone(TimezoneId::named("America/New_York").unwrap())
                        .with_session(
                            Session::new([SessionWindow::new(
                                weekdays,
                                clock(9, 30),
                                clock(16, 0),
                            )])
                            .with_extended_windows([
                                SessionWindow::new(weekdays, clock(5, 0), clock(9, 30)),
                                SessionWindow::new(weekdays, clock(16, 0), clock(20, 0)),
                            ]),
                        ),
                ));
                chart.chart_options.time_scale.timezone =
                    crate::scales::TimezoneMode::Timezone("Europe/Zurich".parse().unwrap());
            } else {
                chart.config.session_config = Some(crate::config::SessionConfig {
                    show_premarket: true,
                    show_postmarket: true,
                    ..crate::config::SessionConfig::nyse()
                });
            }
            chart.config.show_session_breaks = true;
            chart.config.session_break_style = crate::config::SessionBreakStyle::Solid;
            chart.config.session_break_color = egui::Color32::from_rgb(201, 37, 180);
            chart.config.extended_hours_color =
                egui::Color32::from_rgba_unmultiplied(90, 40, 170, 30);
            chart.config.show_extended_hours = shade;
            if custom {
                chart.set_session_provider(Some(std::sync::Arc::new(
                    crate::model::DailySessionProvider::new(21, 0),
                )));
            }
            let painted = shapes(&ctx, &mut chart, 0.0, vec![]);
            let map = chart.get_chart_mapping();
            let lines: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::LineSegment { points, stroke }
                        if stroke.color == chart.config.session_break_color.gamma_multiply(0.6) =>
                    {
                        Some(points[0].x)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                lines,
                expected
                    .iter()
                    .map(|index| map.idx_to_x(*index))
                    .collect::<Vec<_>>()
            );
            let fills: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Rect(rect) if rect.fill == chart.config.extended_hours_color => {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            let expected: Vec<_> = if shade {
                [(0, 1), (if metadata { 3 } else { 2 }, 4), (5, 7)]
                    .into_iter()
                    .map(|(first, end)| {
                        Rect::from_min_max(
                            Pos2::new(map.idx_to_x(first) - map.bar_spacing / 2.0, map.rect.top()),
                            Pos2::new(
                                map.idx_to_x(end - 1) + map.bar_spacing / 2.0,
                                map.rect.bottom(),
                            ),
                        )
                        .intersect(map.rect)
                    })
                    .collect()
            } else {
                vec![]
            };
            assert_eq!(fills, expected);
        }
    }

    #[test]
    fn time_visibility_flags_reach_axis_and_crosshair() {
        use crate::model::{Bar, BarData, Timeframe};
        use chrono::{Duration, TimeZone, Utc};
        let time = Utc.with_ymd_and_hms(2024, 3, 10, 6, 59, 30).unwrap();
        for (seconds, time_visible, seconds_visible, ticks_visible, expected) in [
            (1, true, true, false, "Sun 10 Mar '24 01:59:30"),
            (1, true, false, false, "Sun 10 Mar '24 01:59"),
            (1, false, true, false, "Mar 10, 2024"),
            (1, false, false, false, "Mar 10, 2024"),
            (1, true, true, true, "Sun 10 Mar '24 01:59:30"),
            (60, true, true, true, "Sun 10 Mar '24 01:59"),
            (86_400, true, true, true, "Mar 10, 2024"),
        ] {
            let ctx = egui::Context::default();
            let bars = (0..100)
                .map(|i| {
                    Bar::new(
                        time + Duration::seconds((i - 50) * seconds),
                        100.0,
                        102.0,
                        99.0,
                        101.0,
                        1.0,
                    )
                })
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(if seconds == 1 {
                Timeframe::Sec1
            } else if seconds == 60 {
                Timeframe::Min1
            } else {
                Timeframe::Day1
            });
            let options = &mut chart.chart_options.time_scale;
            options.timezone = crate::scales::TimezoneMode::nyse();
            options.time_visible = time_visible;
            options.seconds_visible = seconds_visible;
            options.ticks_visible = ticks_visible;
            chart.chart_options.tooltip.enabled = false;
            frame(&ctx, &mut chart, 0.0, vec![]);
            let map = chart.get_chart_mapping();
            let pos = Pos2::new(map.idx_to_x(50), map.rect.center().y);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pos)]);
            assert!(
                painted.iter().any(
                    |shape| matches!(shape, Shape::Text(text) if text.galley.text() == expected)
                ),
                "{seconds} {time_visible} {seconds_visible}"
            );
            let ticks = painted.iter().filter(|shape| matches!(shape, Shape::LineSegment { points, .. } if (points[0].y - map.rect.bottom()).abs() < 0.1 && (points[1].y - points[0].y - 3.0).abs() < 0.1)).count();
            assert_eq!(ticks > 0, ticks_visible, "tick visibility: {seconds}");
        }
    }

    #[test]
    fn hover_times_share_the_chart_zone_through_dst() {
        use crate::model::{Bar, BarData, Timeframe};
        use crate::scales::TimezoneMode;
        use chrono::{Duration, TimeZone, Utc};
        // US: UTC-5 to -4 at 07:00Z in March, -4 to -5 at 06:00Z in November.
        // Zurich: UTC+1 to +2 at 01:00Z in March, +2 to +1 at 01:00Z in October.
        for (zone, month, day, hour, before, after) in [
            (
                "America/New_York",
                3,
                10,
                7,
                "Sun 10 Mar '24 01:59:59",
                "Sun 10 Mar '24 03:00:00",
            ),
            (
                "America/New_York",
                11,
                3,
                6,
                "Sun 03 Nov '24 01:59:59",
                "Sun 03 Nov '24 01:00:00",
            ),
            (
                "Europe/Zurich",
                3,
                31,
                1,
                "Sun 31 Mar '24 01:59:59",
                "Sun 31 Mar '24 03:00:00",
            ),
            (
                "Europe/Zurich",
                10,
                27,
                1,
                "Sun 27 Oct '24 02:59:59",
                "Sun 27 Oct '24 02:00:00",
            ),
        ] {
            let transition = Utc.with_ymd_and_hms(2024, month, day, hour, 0, 0).unwrap();
            for (time, expected) in [
                (transition - Duration::seconds(1), before),
                (transition, after),
            ] {
                for mode in [
                    crate::config::TooltipMode::Floating,
                    crate::config::TooltipMode::Tracking,
                ] {
                    let ctx = egui::Context::default();
                    let bars = (0..100)
                        .map(|i| {
                            Bar::new(
                                time + Duration::seconds(i - 50),
                                100.0,
                                102.0,
                                99.0,
                                101.0,
                                1.0,
                            )
                        })
                        .collect();
                    let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
                    let study = chart.add_indicator(Box::new(
                        crate::studies::CustomIndicator::new("Fixture", |bars| {
                            vec![crate::studies::IndicatorValue::Single(0.12345); bars.len()]
                        }),
                    ));
                    let study = chart.indicator_mut(study).unwrap();
                    let mut settings = study.settings();
                    settings.precision = Some(3);
                    study.apply_settings(&[], settings).unwrap();
                    chart.set_timeframe(Timeframe::Sec1);
                    chart.chart_options.time_scale.timezone = TimezoneMode::nyse();
                    chart.set_instrument(Some(
                        crate::model::Instrument::new("DST")
                            .with_time_zone(crate::model::TimezoneId::named(zone).unwrap()),
                    ));
                    assert_eq!(chart.format_time(time), expected);
                    chart.chart_options.tooltip.mode = mode;
                    chart.chart_options.tooltip.show_time = true;
                    chart.chart_options.tooltip.on_long_press = false;
                    frame(&ctx, &mut chart, 0.0, vec![]);
                    let map = chart.get_chart_mapping();
                    let pos = Pos2::new(map.idx_to_x(50), map.rect.center().y);
                    let painted = shapes(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pos)]);
                    let times: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Text(text) if text.galley.text().contains(expected) => {
                                Some(text.galley.text())
                            }
                            _ => None,
                        })
                        .collect();
                    let values: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Text(text) => Some(text.galley.text()),
                            _ => None,
                        })
                        .collect();
                    match mode {
                        crate::config::TooltipMode::Floating => {
                            assert_eq!(
                                values.iter().filter(|text| **text == "Fixture").count(),
                                2,
                                "{values:?}"
                            );
                            assert_eq!(
                                values
                                    .windows(2)
                                    .filter(|pair| *pair == ["Fixture", "0.123"])
                                    .count(),
                                2,
                                "{values:?}"
                            );
                        }
                        _ => assert!(
                            values.iter().any(|text| text.contains("Fixture: 0.123")),
                            "{values:?}"
                        ),
                    }
                    assert_eq!(
                        times.len(),
                        2,
                        "{zone} {time} {mode:?}: expected matching tooltip and crosshair, {times:?}"
                    );
                }
            }
        }
    }

    /// The crosshair's time label names the bar nearest the pointer, and a chart that follows
    /// another one's index names the same bar. The bars are daily from 2023-11-14, so bar 50 is
    /// 3 Jan 2024 and bar 51 is 4 Jan 2024; 0.7 of a bar past the centre of bar 50 is nearer to 51.
    #[test]
    fn crosshair_time_label_names_the_nearest_bar() {
        use crate::model::{ChartType, ChartTypeParams};
        let mut highs = vec![120.0; 100];
        highs[99] = 3120.0;
        // (type, highs, element index, pointer over chart, synced index, expected label)
        for case in [
            (
                ChartType::Candles,
                &[200.0][..],
                50,
                true,
                None,
                "Jan 04, 2024",
                false,
            ),
            (
                ChartType::Candles,
                &[200.0][..],
                50,
                false,
                Some(50.7),
                "Jan 04, 2024",
                false,
            ),
            #[cfg(feature = "studies")]
            (
                ChartType::Candles,
                &[200.0][..],
                50,
                true,
                None,
                "Jan 04, 2024",
                true,
            ),
            // Bar 99 alone completes 300 bricks, all dated 21 February 2024.
            (
                ChartType::Renko,
                highs.as_slice(),
                250,
                true,
                None,
                "Feb 21, 2024",
                false,
            ),
        ] {
            let (kind, highs, index, pointer, synced, expected, panes) = case;
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], highs);
            chart.chart_options.tooltip.enabled = false;
            #[cfg(feature = "studies")]
            if panes {
                chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
                chart.add_indicator(Box::new(crate::studies::MACD::new(12, 26, 9)));
            }
            chart.set_chart_type(kind);
            chart.set_chart_type_params(ChartTypeParams {
                renko_brick_size: 10.0,
                ..Default::default()
            });
            chart.set_synced_crosshair_bar_idx(synced);
            frame(&ctx, &mut chart, 0.0, Vec::new());
            let map = chart.get_chart_mapping();
            let hover = Pos2::new(
                map.idx_to_x(index) + 0.7 * map.bar_spacing,
                map.rect.center().y,
            );
            let events = || {
                pointer
                    .then_some(Event::PointerMoved(hover))
                    .into_iter()
                    .collect()
            };
            frame(&ctx, &mut chart, 0.1, events());
            let painted = shapes(&ctx, &mut chart, 0.2, events());
            let labels: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) if text.galley.text().ends_with(", 2024") => {
                        if panes {
                            let bottom = chart
                                .get_rendered_indicator_panes()
                                .last()
                                .unwrap()
                                .panel_rect
                                .bottom();
                            assert!(text.pos.y >= bottom, "time tag above lowest pane");
                        }
                        Some(text.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(labels, [expected], "{case:?}");
            let time = chart.state.shown()[index + 1].time;
            assert_eq!(chart.format_time(time), expected, "host readout: {case:?}");
        }
    }

    #[test]
    fn plot_uses_the_interior_and_configured_axis_widths() {
        let mut chart = chart(&[100.0], &[110.0]);
        for (width, height, padding, left, right) in [
            (800.0, 600.0, 0.0, 0.0, 70.0),
            (640.0, 450.0, 2.0, 50.0, 80.0),
            (45.0, 20.0, 0.0, 0.0, 70.0),
            (800.0, 600.0, 8.0, 0.0, 0.0),
        ] {
            chart.config.padding = padding;
            chart.config.show_left_axis = left > 0.0;
            chart.config.left_axis_width = left;
            chart.config.show_right_axis = right > 0.0;
            chart.config.right_axis_width = right;
            let widget = Rect::from_min_size(Pos2::new(7.0, 11.0), egui::vec2(width, height));
            let layout = chart.calculate_layout_rects(widget);
            assert!(widget.contains_rect(layout.price_rect));
            assert_eq!(layout.price_rect.top(), widget.top() + padding);
            assert_eq!(
                layout.price_rect.left(),
                widget.left() + (padding + left).min(width)
            );
            assert_eq!(
                layout.price_rect.width(),
                (width - 2.0 * padding - left - right).max(0.0)
            );
            assert_eq!(
                layout.price_rect.height(),
                (height - 2.0 * padding - 28.0).max(0.0)
            );
        }
    }

    #[test]
    fn volume_uses_themes_alpha_and_pitch_above_a_configured_baseline() {
        use crate::chart::selection::{ChartElementId, SeriesId};
        use crate::model::ChartType;
        use crate::theme::{Theme, apply_to_egui};
        use egui::Color32;

        // Volume uses muted teal and salmon independently of candle colours.
        // A second opacity proves that painting reads the applied theme on every frame.
        for (mut theme, alpha, candle_width, kind, height_fraction, inset, expected) in [
            (
                Theme::dark(),
                153,
                0.6,
                ChartType::Candles,
                0.12,
                12.0,
                [[23, 100, 92, 153], [143, 50, 48, 153]],
            ),
            (
                Theme::dark(),
                51,
                0.2,
                ChartType::Heikin,
                0.12,
                12.0,
                [[8, 33, 31, 51], [48, 17, 16, 51]],
            ),
            (
                Theme::light(),
                128,
                0.6,
                ChartType::Line,
                0.25,
                0.0,
                [[19, 83, 77, 128], [120, 42, 40, 128]],
            ),
            (
                Theme::high_contrast(),
                128,
                0.6,
                ChartType::Area,
                0.4,
                6.0,
                [[19, 83, 77, 128], [120, 42, 40, 128]],
            ),
        ] {
            let ctx = egui::Context::default();
            theme.chart.volume_alpha = alpha;
            apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0], &[110.0]);
            // The last visible bar falls, and has half the other bars' volume.
            let mut bars = chart.state.data().bars.clone();
            let last = bars.last_mut().unwrap();
            last.open = 110.0;
            last.close = 100.0;
            last.volume = 0.5;
            chart.set_data(crate::chart::selection::SeriesId::MAIN, bars);
            chart.config.candle_width = candle_width;
            chart.config.volume_height_fraction = height_fraction;
            chart.config.volume_baseline_inset = inset;
            chart.set_chart_type(kind);
            chart
                .selection
                .select(ChartElementId::Series(SeriesId::VOLUME), None);
            let painted = shapes(&ctx, &mut chart, 0.0, Vec::new());
            let price = chart.last_rendered_price_rect;
            let volume = chart.last_rendered_volume_rect;
            let layout = chart.calculate_layout_rects(egui::Rect::from_min_max(
                Pos2::new(8.0, 8.0),
                Pos2::new(792.0, 592.0),
            ));
            assert_eq!(layout.price_rect, price);
            assert_eq!(layout.volume_rect, volume);
            assert!(
                (volume.height() - (price.height() - inset) * height_fraction).abs() < 0.001,
                "{volume:?} in {price:?}"
            );
            assert_eq!(volume.bottom(), price.bottom() - inset);
            let dots: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Circle(dot) if dot.radius == 4.5 => Some(dot.center),
                    _ => None,
                })
                .collect();
            assert!(!dots.is_empty(), "missing volume selection handles");
            assert!(dots.iter().all(|dot| (dot.y - volume.top()).abs() < 0.001));
            let pitch = chart.state.time_scale().bar_spacing();
            for (colour, fraction) in expected.into_iter().zip([1.0, 0.5]) {
                let colour =
                    Color32::from_rgba_premultiplied(colour[0], colour[1], colour[2], colour[3]);
                let bars: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Rect(rect) if rect.fill == colour => Some(rect.rect),
                        _ => None,
                    })
                    .collect();
                assert!(!bars.is_empty(), "alpha {alpha}: missing volume {colour:?}");
                for bar in bars {
                    assert!(
                        (bar.height() - volume.height() * fraction).abs() < 0.001,
                        "{bar:?}"
                    );
                    assert_eq!(bar.bottom(), volume.bottom());
                    assert!(
                        (bar.width() - pitch * 0.8).abs() < 0.001,
                        "{bar:?} at pitch {pitch}"
                    );
                }
            }
        }
    }

    fn navigation_pos(chart: &crate::widget::Chart, button: usize) -> Pos2 {
        let rect = chart
            .last_rendered_price_rect
            .union(chart.last_rendered_volume_rect);
        Pos2::new(
            rect.center().x - 112.0 + 32.0 * button as f32,
            rect.bottom() - 26.0,
        )
    }

    fn navigation_click(ctx: &egui::Context, chart: &mut crate::widget::Chart, button: usize) {
        let pos = navigation_pos(chart, button);
        for (i, events) in [
            vec![Event::PointerMoved(pos)],
            vec![navigation_press(pos, true)],
            vec![navigation_press(pos, false)],
            Vec::new(),
        ]
        .into_iter()
        .enumerate()
        {
            frame(ctx, chart, 1.0 + i as f64 * 0.02, events);
        }
    }

    fn navigation_press(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// On the 714 px time scale, spacing 20 shows ceil(714/20 - 1) = 35 bars.
    /// A zoom step of 0.2 adds +/-0.2 * 20/10 to spacing and keeps the right edge
    /// (offset -20) fixed; reset restores spacing 6 and the configured 60 px / 6 = 10 bar offset.
    #[test]
    fn navigation_buttons_apply_each_action() {
        use crate::scales::PriceScaleMode::{Logarithmic, Normal, Percentage};
        // (button, initial manual/mode, expected spacing/offset/manual/mode)
        for (button, initial_manual, initial_mode, spacing, offset, manual, mode) in [
            (0, false, Normal, 19.6, -20.0, false, Normal),
            (1, false, Normal, 20.4, -20.0, false, Normal),
            (2, false, Normal, 20.0, -55.0, false, Normal),
            (3, false, Normal, 20.0, 15.0, false, Normal),
            (4, true, Normal, 6.0, 10.0, false, Normal),
            (5, false, Normal, 20.0, -20.0, true, Normal),
            (5, true, Normal, 20.0, -20.0, false, Normal),
            (6, false, Normal, 20.0, -20.0, false, Logarithmic),
            (6, true, Logarithmic, 20.0, -20.0, false, Normal),
            (7, false, Normal, 20.0, -20.0, false, Percentage),
            (7, true, Percentage, 20.0, -20.0, false, Normal),
        ] {
            let ctx = egui::Context::default();
            let mut chart =
                chart(&[100.0], &[200.0]).with_chart_options(crate::config::ChartOptions {
                    time_scale: crate::config::TimeScaleOptions {
                        right_offset_pixels: Some(60.0),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            chart.config.navigation_buttons = crate::config::NavigationButtons::OnHover;
            frame(&ctx, &mut chart, 0.0, Vec::new());
            assert_eq!(chart.state.time_scale().width(), 714.0);
            chart.state.time_scale_mut().set_bar_spacing(20.0);
            chart.state.time_scale_mut().set_right_offset(-20.0);
            chart.apply_series_settings(&SeriesSettings {
                price_scale_mode: initial_mode,
                ..Default::default()
            });
            if initial_manual {
                chart.state.price_scale_mut().set_range(90.0, 210.0);
            }
            frame(&ctx, &mut chart, 0.1, Vec::new());
            navigation_click(&ctx, &mut chart, button);
            let ts = chart.state.time_scale();
            assert!(
                (ts.bar_spacing() - spacing).abs() < 1e-4,
                "button {button}: spacing {}",
                ts.bar_spacing()
            );
            assert!(
                (f64::from(ts.right_offset()) - offset).abs() < 1e-4,
                "button {button}: offset {}",
                ts.right_offset()
            );
            assert_eq!(
                chart.state.price_scale().is_manual(),
                manual,
                "button {button}"
            );
            assert_eq!(
                chart.state.price_scale().options().mode,
                mode,
                "button {button}"
            );
        }
    }

    /// The selected fill follows price-axis scaling/reset and series settings on the next frame,
    /// and the row shows while hovering the main pane (volume included) per the visibility setting.
    #[test]
    fn navigation_buttons_show_visibility_and_scale_state() {
        use crate::config::NavigationButtons::{Always, Never, OnHover};
        use crate::scales::PriceScaleMode::{Logarithmic, Normal, Percentage};
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[200.0]);
        assert_eq!(chart.config.navigation_buttons, Never);
        #[cfg(feature = "ui")]
        assert_eq!(
            crate::ui::top_toolbar::components::settings_menu::ButtonsSettings::default()
                .navigation,
            crate::ui::top_toolbar::components::settings_menu::ButtonVisibility::Hidden,
        );
        frame(&ctx, &mut chart, 0.0, Vec::new());
        for (i, (manual, mode, visibility, hover, selected)) in [
            (false, Normal, OnHover, true, [true, false, false]),
            (true, Normal, OnHover, true, [false, false, false]),
            (false, Logarithmic, OnHover, true, [true, true, false]),
            (false, Percentage, OnHover, true, [true, false, true]),
            (false, Normal, OnHover, false, [false, false, false]),
            (false, Normal, Always, false, [true, false, false]),
            (false, Normal, Never, true, [false, false, false]),
        ]
        .into_iter()
        .enumerate()
        {
            chart.apply_series_settings(&SeriesSettings {
                price_scale_mode: mode,
                ..Default::default()
            });
            if manual {
                chart.state.price_scale_mut().set_range(90.0, 210.0);
            } else {
                chart.state.price_scale_mut().reset();
            }
            chart.config.navigation_buttons = visibility;
            let pointer = if hover {
                chart.last_rendered_volume_rect.center()
            } else {
                chart.last_rendered_price_rect.right_top() + egui::vec2(20.0, 20.0)
            };
            let painted = shapes(
                &ctx,
                &mut chart,
                0.1 + i as f64,
                vec![Event::PointerMoved(pointer)],
            );
            for (toggle, expected) in selected.into_iter().enumerate() {
                let center = navigation_pos(&chart, toggle + 5);
                let active = painted.iter().any(|shape| matches!(shape, Shape::Rect(r)
                    if r.rect.center().distance(center) < 0.1 && r.fill == ctx.global_style().visuals.selection.bg_fill));
                assert_eq!(
                    active, expected,
                    "manual {manual}, mode {mode:?}, toggle {toggle}"
                );
            }
        }
    }

    /// A drag starting on a button belongs to that button even after leaving the row.
    /// Clicking it also preserves a series selection while a drawing tool is active.
    #[test]
    fn navigation_buttons_do_not_start_chart_gestures() {
        use crate::drawings::DrawingToolType;
        use crate::widget::{ChartElementId, SeriesId};
        for (tool, box_zoom) in [
            (None, false),
            (Some(DrawingToolType::TrendLine), false),
            (Some(DrawingToolType::Rect), false),
            (None, true),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[200.0]);
            chart.config.navigation_buttons = crate::config::NavigationButtons::OnHover;
            frame(&ctx, &mut chart, 0.0, Vec::new());
            chart.state.time_scale_mut().set_bar_spacing(20.0);
            chart.state.time_scale_mut().set_right_offset(-20.0);
            chart.drawings.active_tool = tool;
            chart.zoom_mode_active = box_zoom;
            chart
                .selection
                .select(ChartElementId::Series(SeriesId::MAIN), Some(50));
            let start = navigation_pos(&chart, 5);
            let end = start + egui::vec2(-100.0, -60.0);
            for (i, events) in [
                vec![Event::PointerMoved(start)],
                vec![navigation_press(start, true)],
                vec![Event::PointerMoved(end)],
                vec![navigation_press(end, false)],
            ]
            .into_iter()
            .enumerate()
            {
                frame(&ctx, &mut chart, 0.1 + i as f64 * 0.02, events);
                assert_eq!(
                    chart.state.time_scale().right_offset(),
                    -20.0,
                    "{tool:?}, box zoom {box_zoom}"
                );
                assert!(chart.scroll_start_pos.is_none());
                assert!(!chart.box_zoom.active);
                assert!(chart.drawings.curr_drawing.is_none());
                assert!(chart.drawings.drawings.is_empty());
            }
            navigation_click(&ctx, &mut chart, 5);
            assert_eq!(chart.state.time_scale().right_offset(), -20.0);
            assert!(chart.scroll_start_pos.is_none());
            assert_eq!(
                chart.selected_element(),
                Some(ChartElementId::Series(SeriesId::MAIN))
            );
            assert!(chart.drawings.curr_drawing.is_none());
        }
    }
}
