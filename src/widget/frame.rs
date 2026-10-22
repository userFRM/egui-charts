//! Frame rendering: the `show` entry points and the per-frame paint and interaction pass.

use super::selection::hit_test_main_chart;
use super::{Chart, RightClickTarget};
use crate::chart::renderers::{self, ChartMapping, PriceScale, RenderContext, StyleColors};
use crate::chart::rendering;
use crate::config::{BackgroundStyle, WatermarkPos};
use crate::drawings::DrawingManager;
use crate::scales::TimeFormatterBuilder;
use crate::studies::IndicatorRegistry;
use crate::styles::{sizing, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Pos2, Rect, Response, Sense, Ui, Vec2};

impl Chart {
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

    /// Renders the chart with mouse interactions and optional drawing tools.
    ///
    /// This is the mid-level rendering method. Use this when you have drawing
    /// tools but no separate-pane indicators. For the simplest case, use
    /// [`Chart::show`]. For full functionality, use [`Chart::show_with_indicators`].
    pub fn show_with_drawings(
        &mut self,
        ui: &mut Ui,
        drawing_manager: Option<&mut DrawingManager>,
    ) -> Response {
        self.show_internal(ui, drawing_manager, None)
    }

    /// Renders the chart with standard mouse interactions.
    ///
    /// This is the simplest way to display a chart. It handles pan, zoom,
    /// crosshair, keyboard shortcuts, and all visual elements configured in
    /// [`ChartConfig`](crate::config::ChartConfig). No drawing tools or separate-pane indicators are rendered.
    ///
    /// Returns an [`egui::Response`] for additional interaction handling.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// egui::CentralPanel::default().show(ctx, |ui| {
    ///     let response = chart.show(ui);
    ///     if response.hovered() {
    ///         // Chart is being hovered
    ///     }
    /// });
    /// ```
    pub fn show(&mut self, ui: &mut Ui) -> Response {
        self.show_internal(ui, None, None)
    }

    /// Resolve the timeframe used to pick a session-break granularity.
    ///
    /// The legend `timeframe` label is the primary source (it parses forms like
    /// `"1H"`, `"15min"`, `"1D"`). When it is empty or unrecognized, the cadence
    /// is inferred from the visible bars by taking the median gap between
    /// consecutive timestamps and snapping it to the nearest preset, so session
    /// breaks work even when the host never set a label. Falls back to the
    /// default (1-minute) when there are too few bars to measure.
    fn resolve_session_timeframe(
        &self,
        visible_data: &[crate::model::Bar],
    ) -> crate::model::Timeframe {
        use crate::model::Timeframe;
        use std::str::FromStr;

        if let Ok(tf) = Timeframe::from_str(self.timeframe.trim()) {
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
        mut drawing_manager: Option<&mut DrawingManager>,
        indicators: Option<&IndicatorRegistry>,
    ) -> Response {
        // Register egui's image loaders once per context so the embedded SVG
        // icons used by the toolbars and panels decode without the host app
        // having to wire egui_extras itself.
        let ctx = ui.ctx().clone();
        let loaders_installed = egui::Id::new("egui_charts::image_loaders_installed");
        if !ctx.data(|d| d.get_temp::<bool>(loaders_installed).unwrap_or(false)) {
            egui_extras::install_image_loaders(&ctx);
            ctx.data_mut(|d| d.insert_temp(loaders_installed, true));
        }

        // Reset zoom_just_applied flag at the start of each frame
        self.zoom_just_applied = false;

        // A right-click result lives for exactly one frame: clear any stale
        // capture so a target that is never drained does not reappear later.
        self.right_click = None;

        let available_size = ui.available_size();
        let (mut response, painter) = ui.allocate_painter(available_size, Sense::click_and_drag());
        let rect = response.rect;

        // Establish chart_rect FIRST before any operations
        let top_padding = if self.config.show_ohlc_info {
            sizing::chart::TOP_PADDING_WITH_OHLC
        } else {
            sizing::chart::TOP_PADDING_NO_OHLC
        };
        let bottom_padding = if self.config.show_time_labels {
            sizing::chart::BOTTOM_PADDING_WITH_TIME
        } else {
            sizing::chart::BOTTOM_PADDING_NO_TIME
        };
        let right_axis_width = sizing::chart::RIGHT_AXIS_WIDTH;

        let left_margin = sizing::chart::PADDING;
        let right_margin = sizing::chart::PADDING + right_axis_width;

        let chart_rect = Rect::from_min_size(
            rect.min + Vec2::new(left_margin, top_padding),
            Vec2::new(
                (rect.width() - left_margin - right_margin).max(sizing::chart::MIN_CHART_WIDTH),
                (rect.height() - top_padding - bottom_padding).max(sizing::chart::MIN_CHART_HEIGHT),
            ),
        );

        let chart_width = chart_rect.width();

        // CRITICAL: Apply TimeScale width configuration BEFORE any zoom operations
        // This ensures apply_constraints() inside zoom() uses the correct self.width
        // to calculate constraint bounds. Without this, drawings drift during zoom
        // because constraints are calculated with stale width values.
        self.apply_timescale_config(chart_width);

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
            Pos2::new(chart_rect.max.x, chart_rect.min.y),
            Pos2::new(rect.max.x, chart_rect.max.y),
        );
        let time_axis_rect = Rect::from_min_max(
            Pos2::new(chart_rect.min.x, chart_rect.max.y),
            Pos2::new(chart_rect.max.x, rect.max.y),
        );

        // Handle double-click to reset axes
        self.handle_double_click(&response, price_axis_rect, time_axis_rect);

        // Handle mouse wheel for zoom/scroll
        // Block pan/zoom when drawing tool is active OR when manipulating a drawing
        let is_drawing_interaction = drawing_manager.as_ref().is_some_and(|dm| {
            dm.active_tool.is_some() || dm.dragging_handle.is_some() || dm.curr_drawing.is_some()
        });
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
                CrosshairStyle::Arrow => {
                    // Arrow mode - default pointer
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
            }
        }

        // Draw background (solid or gradient)
        self.draw_background(&painter, rect);

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

        // Split chart area into price and volume sections
        let (price_rect, volume_rect) = if self.config.show_volume {
            let split_y =
                chart_rect.min.y + chart_rect.height() * (1.0 - self.config.volume_height_fraction);
            (
                Rect::from_min_max(chart_rect.min, Pos2::new(chart_rect.max.x, split_y)),
                Rect::from_min_max(Pos2::new(chart_rect.min.x, split_y), chart_rect.max),
            )
        } else {
            (chart_rect, Rect::ZERO)
        };

        // Get visible range
        let (start_idx, _end_idx) = self.state.visible_range();
        self.start_idx = start_idx;

        // Capture near_live status for button
        let near_live_edge = self.state.time_scale().right_offset() >= -1.5;

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
            let btn_id = ui.id().with("jump_to_latest");
            let btn_res = ui.interact(btn_rect, btn_id, egui::Sense::click());

            if btn_res.clicked() {
                self.state.time_scale_mut().scroll_to_realtime();
            }
        }

        // Determine price bounds
        let (mut adjusted_min, mut adjusted_max) = self.state.price_range();

        // Apply price zoom
        let (new_min, new_max) = self.apply_price_zoom(
            pending_price_zoom,
            &response,
            chart_rect,
            adjusted_min,
            adjusted_max,
        );
        adjusted_min = new_min;
        adjusted_max = new_max;

        // Store the final rendered price range and rects for external use (selection dots, hit testing)
        self.last_rendered_price_range = (adjusted_min, adjusted_max);
        self.last_rendered_price_rect = price_rect;
        self.last_rendered_volume_rect = volume_rect;

        // Get visible data
        let visible_data = self.state.visible_data();

        if visible_data.is_empty() {
            return response;
        }

        // Calculate volume range
        let max_volume = if self.config.show_volume {
            visible_data
                .iter()
                .map(|c| c.volume)
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(1.0)
        } else {
            1.0
        };

        // Draw grid
        if self.config.show_horizontal_grid {
            rendering::render_grid(
                &painter,
                price_rect,
                adjusted_min,
                adjusted_max,
                self.config.grid_color,
            );
        }

        // Create rendering contexts
        let bar_spacing = self.state.time_scale().bar_spacing();
        let bar_width = bar_spacing * self.config.candle_width;
        let price_ctx = RenderContext::new(&painter, price_rect);
        let price_scale = PriceScale::new(adjusted_min, adjusted_max);
        let coords = ChartMapping::new(
            price_rect,
            bar_spacing,
            start_idx,
            self.state.time_scale().base_idx(),
            self.state.time_scale().right_offset(),
            adjusted_min,
            adjusted_max,
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

        let formatter = if self.config.show_time_labels || self.config.show_vertical_grid {
            Some(
                TimeFormatterBuilder::new()
                    .with_24_hour(true)
                    .with_seconds(true)
                    .with_timezone(self.chart_options.time_scale.timezone.clone())
                    .build(),
            )
        } else {
            None
        };

        if self.config.show_vertical_grid {
            // Simple bar-index-based vertical grid - moves 1:1 with chart
            rendering::render_vertical_grid(&painter, chart_rect, &coords, self.config.grid_color);
        }

        // Session-break layer: day/session dividers and optional alternating
        // session shading. Drawn here, after the grid and before the candles, so
        // the shading sits behind the bars and the dividers stay subtle. The
        // boundary granularity follows the timeframe: day changes intraday, week
        // changes on daily bars, month changes on weekly/monthly bars. Painters
        // are clipped to the price rect so neither bleeds onto the axes.
        if self.config.show_session_breaks {
            let session_tf = self.resolve_session_timeframe(visible_data);
            let provider = renderers::provider_for_timeframe(session_tf);
            let session_painter = painter.with_clip_rect(price_rect);
            let session_ctx = RenderContext::new(&session_painter, price_rect);

            // Subtle alternating shading behind the candles (still gated by the
            // same flag; the even span is transparent so the effect reads as a
            // faint banding rather than two competing fills).
            let background =
                renderers::SessionBackgroundRenderer::from_background(self.config.background_color);
            background.render(
                &session_ctx,
                visible_data,
                provider.as_ref(),
                &coords,
                start_idx,
            );

            let break_renderer =
                renderers::SessionBreakRenderer::new(renderers::SessionBreakRenderConfig {
                    line_color: self.config.session_break_color,
                    line_width: 1.0,
                    style: self.config.session_break_style,
                });
            break_renderer.render(
                &session_ctx,
                visible_data,
                provider.as_ref(),
                &coords,
                start_idx,
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
            price_scale: &price_scale,
            colors: &colors,
            visible_data,
            // Full dataset so window-independent transforms (Heikin-Ashi) can be
            // computed over the complete series, not just the visible slice.
            full_data: &self.state.data().bars,
            start_idx,
        };

        let render_params = rendering::ChartTypeParams::new(
            rendering::BarDimensions::new(bar_width, self.config.wick_width),
            rendering::VolumeSettings::new(self.config.show_volume, max_volume),
            rendering::JapaneseChartSettings::new(self.renko_brick_size, self.kagi_reversal_amount),
            rendering::TradingColors::new(self.config.bullish_color, self.config.bearish_color),
            rendering::CoordMapping::new(chart_rect.min.x),
            self.config.price_source,
        );

        rendering::render_chart_type(self.chart_type, &render_ctx, &render_params, idx_to_coord);

        // Draw indicators
        if let Some(indicator_registry) = indicators {
            renderers::IndicatorRenderer::render(
                &price_ctx,
                indicator_registry.indicators(),
                visible_data,
                &price_scale,
                &coords,
            );
        }

        // Resolve a click on the main chart area into a selection. Overlay
        // indicators take priority over the series beneath them; an empty-area
        // click clears the selection. Pane-indicator clicks are resolved by
        // show_with_indicators, which renders the panes outside this rect.
        let main_visible_range = start_idx..(start_idx + visible_data.len());
        if response.clicked()
            && let Some(click_pos) = response.interact_pointer_pos()
        {
            let volume_coords = if self.config.show_volume {
                Some(coords.with_rect(volume_rect))
            } else {
                None
            };
            let hit = hit_test_main_chart(
                click_pos,
                indicators,
                &coords,
                volume_coords.as_ref(),
                &self.state.data().bars,
                max_volume,
                main_visible_range.clone(),
            );
            match hit {
                Some((id, bar_idx)) => self.selection.select(id, Some(bar_idx)),
                None => self.selection.deselect(),
            }
        }

        // Resolve a right-click into a context-menu target. Mirrors the
        // left-click priority (drawing > overlay indicator > series), and
        // right-clicking an object also selects it so the menu and the
        // selection handles agree. The captured target is
        // drained by the host via `take_right_click`, or consumed by the
        // turnkey menu in `TradingChart`.
        if response.secondary_clicked()
            && let Some(click_pos) = response.interact_pointer_pos()
            && chart_rect.contains(click_pos)
        {
            let price = coords.y_to_price(click_pos.y);

            // Drawings sit on top of the series, so they take priority. The
            // drawing manager is reborrowed here; it is moved into
            // `handle_drawings` later in the frame.
            let drawing_hit = drawing_manager
                .as_deref()
                .and_then(|dm| dm.hit_test(click_pos));

            self.right_click = Some(if let Some(drawing_id) = drawing_hit {
                // Right-clicking a drawing selects it and
                // clears any series/indicator selection so handles don't show
                // on two objects at once.
                self.selection.deselect();
                if let Some(dm) = drawing_manager.as_deref_mut() {
                    dm.select(drawing_id);
                }
                RightClickTarget::Drawing {
                    drawing_id,
                    pos: click_pos,
                }
            } else {
                let volume_coords = if self.config.show_volume {
                    Some(coords.with_rect(volume_rect))
                } else {
                    None
                };
                let element_hit = hit_test_main_chart(
                    click_pos,
                    indicators,
                    &coords,
                    volume_coords.as_ref(),
                    &self.state.data().bars,
                    max_volume,
                    main_visible_range.clone(),
                );
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
        self.render_main_chart_selection(
            &painter,
            indicators,
            &coords,
            visible_data,
            main_visible_range,
        );

        // Render bar marks (Widget API annotations)
        if !self.marks.is_empty() {
            renderers::render_markers(
                &clipped_price_ctx,
                &self.marks,
                visible_data,
                &price_scale,
                &coords,
            );
        }

        // Draw price labels
        if self.config.show_right_axis {
            rendering::render_price_labels(
                &price_ctx,
                &price_scale,
                &colors,
                crate::scales::PriceScaleMode::Normal,
            );
        }

        // Last price line & label
        if self.config.show_symbol_last_val
            && let Some(last) = visible_data.last()
        {
            rendering::render_last_price_line(
                &painter,
                price_rect,
                last.close,
                last.open,
                adjusted_min,
                adjusted_max,
                self.config.bullish_color,
                self.config.bearish_color,
                self.config.show_right_axis,
            );
        }

        // Draw time labels
        if self.config.show_time_labels {
            let chart_ctx = RenderContext::new(&painter, chart_rect);
            rendering::render_time_labels(
                &chart_ctx,
                visible_data,
                &coords,
                &colors,
                formatter.as_deref(),
            );
        }

        // Draw OHLC info header (legend if symbol is set)
        if self.config.show_ohlc_info {
            if !self.symbol.is_empty() {
                // Calculate prev_close from second-to-last bar for change calculation
                let prev_close = if visible_data.len() >= 2 {
                    Some(visible_data[visible_data.len() - 2].close)
                } else {
                    None
                };
                rendering::render_legend(
                    &painter,
                    rect,
                    &self.symbol,
                    &self.timeframe,
                    visible_data,
                    prev_close,
                    &colors,
                    sizing::chart::PADDING,
                );
            } else {
                // Fallback to basic OHLC info
                rendering::render_ohlc_info(
                    &painter,
                    rect,
                    visible_data,
                    sizing::chart::PADDING,
                    self.config.text_color,
                );
            }
        }

        // Handle drawing tools
        if let Some(dm) = drawing_manager {
            // Clone/extract values before mutable borrow of self
            let timescale = self.state.time_scale().clone();

            // Temporarily take cursor_modes to avoid borrow conflict
            let mut cursor_modes = std::mem::take(&mut self.cursor_modes);

            self.handle_drawings(
                ui,
                dm,
                &mut cursor_modes,
                &response,
                price_rect,
                adjusted_min,
                adjusted_max,
                &painter,
                &timescale,
            );

            // Render eraser highlight if in eraser mode
            self.render_eraser_highlight(&painter, dm, &cursor_modes);

            // Put cursor_modes back
            self.cursor_modes = cursor_modes;
        }

        // Render "Jump to Latest" button
        if self.config.show_realtime_btn {
            let btn_id = ui.id().with("jump_to_latest");
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

        // Draw crosshair with options from chart_options
        if let Some(hover_pos) = response.hover_pos()
            && price_rect.contains(hover_pos)
        {
            // Cache the hover bar index for multi-chart sync
            self.last_hover_bar_idx = Some(coords.x_to_idx_f32(hover_pos.x) as f64);

            rendering::render_crosshair_with_options(
                &price_ctx,
                hover_pos,
                visible_data,
                &price_scale,
                &coords,
                &self.chart_options.crosshair,
            );

            // Draw the OHLC/value readout for the bar under the cursor, matching
            // the bar the crosshair snaps to. Resolve via the shared coordinate
            // mapping so the readout and the crosshair never disagree, and skip
            // it when the cursor is in the empty scroll margin past the data.
            let tooltip_options = &self.chart_options.tooltip;
            if tooltip_options.enabled
                && let Some(local_idx) = coords.local_idx_at_x(hover_pos.x, visible_data.len())
            {
                let candle = &visible_data[local_idx];
                rendering::render_tooltip_with_options(
                    &price_ctx,
                    hover_pos,
                    candle,
                    tooltip_options,
                    &price_scale,
                    &coords,
                    visible_data,
                );
            }
        } else {
            // Clear hover bar index when not hovering locally
            self.last_hover_bar_idx = None;

            // Render synced crosshair from other charts (if available)
            {
                if let Some(bar_idx) = self.synced_crosshair_bar_idx {
                    // Convert bar index to screen x coordinate
                    let x = coords.idx_to_x(bar_idx as usize);
                    if coords.is_x_visible(x) {
                        // Create a synthetic hover position at the center of the price range
                        let center_y = price_rect.center().y;
                        let synced_pos = Pos2::new(x, center_y);

                        rendering::render_crosshair_with_options(
                            &price_ctx,
                            synced_pos,
                            visible_data,
                            &price_scale,
                            &coords,
                            &self.chart_options.crosshair,
                        );
                    }
                }
            }
        }

        // Draw box zoom rect
        rendering::render_box_zoom(&painter, &self.box_zoom);

        // Focus ring for keyboard accessibility
        crate::styles::focus::draw_focus_ring(ui, &response);

        response
    }
}
