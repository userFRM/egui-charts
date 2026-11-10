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

impl Chart {
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
                time_formatter: None,
                bar_seconds: 0,
                instrument: price_member.and(self.instrument()),
                bars: self.state.shown(),
                revision: self.state.revision(),
                bar_to_x: &|bar| timescale.idx_to_coord_precise(bar, clip.min.x, clip.width()),
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

    /// The interval drawings measure on time-based charts with a time axis.
    pub(super) fn drawing_timeframe(&self) -> Option<crate::model::Timeframe> {
        let timeframe = self.timeframe.unwrap_or_default();
        (!timeframe.is_trade_based()
            && !self.chart_type.is_time_independent()
            && self.horizontal_scale.is_none())
        .then_some(timeframe)
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
        // x_to_bar_snapped: ROUNDED to nearest candle center (for line tools)
        let x_to_bar_snapped = |x: f32| -> f32 {
            timescale
                .coord_to_idx_precise(x, rect_min_x, rect_width)
                .round() as f32
        };
        // x_to_bar_precise: Full precision for freeform tools (Brush, Highlighter)
        // Prevents pixelation when zooming - preserves sub-pixel positioning
        let x_to_bar_precise =
            |x: f32| -> f32 { timescale.coord_to_idx(x, rect_min_x, rect_width) };
        let price_to_y = |price: f64| -> f32 { price_map.price_to_y(price) };
        // A strong snap keeps the actual OHLC even beyond a manual price range.
        let strong_magnet =
            drawing_manager.magnet_enabled() && drawing_manager.options.strong_magnet;
        let y_to_price = |y: f32| -> f64 {
            price_map.y_to_price(if strong_magnet {
                y
            } else {
                y.clamp(price_rect.min.y, price_rect.max.y)
            })
        };
        let translate_price = |price, from, to| price_map.translate_price(price, from, to);

        // Update all drawings' screen coords before rendering (handles pan/zoom)
        drawing_manager.update_pane_screen_coords(bar_to_x, price_to_y);
        drawing_manager.anchor_pins_to_bars(x_to_bar_precise, |y| price_map.y_to_price(y));

        // Only gather bars within the pointer's screen-space tolerance. Rebuild after pan/zoom.
        drawing_manager.snap_targets.clear();
        let press =
            if drawing_manager.active_tool.is_some() && drawing_manager.curr_drawing.is_none() {
                interaction::primary_press_pos(ui)
            } else {
                None
            };
        if drawing_manager.magnet_enabled()
            && let Some(pointer) = press
                .or_else(|| response.interact_pointer_pos())
                .or_else(|| response.hover_pos())
        {
            let distance = drawing_manager.options.magnet_distance;
            let bars = self.state.shown();
            let strong = drawing_manager.options.strong_magnet;
            let index = x_to_bar_snapped(pointer.x);
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
                        study.is_visible()
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
            }
        }

        // Handle drawing interaction
        // Select converter based on tool type: freeform tools (ContinuousDraw) use precise,
        // constrained tools retain their angles; other tools snap to candle centers
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
        super::drawing_api::begin_fibonacci_caption_edit(ui, drawing_manager, response, price_rect);
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
        if !owns_gesture || drawing_manager.text_editor.is_some() {
            // The editor owns this frame's pointer input, including its outside commit click.
        } else if let Some(active_tool) = drawing_manager.active_tool {
            let uses_precise = active_tool.interaction_mode()
                == DrawingInteractionMode::ContinuousDraw
                || (ui.input(|i| i.modifiers.shift)
                    && (DrawingManager::constrains_angle(active_tool)
                        || matches!(
                            active_tool,
                            crate::drawings::DrawingToolType::Rect
                                | crate::drawings::DrawingToolType::Ellipse
                        )));
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
        } else {
            selection::handle_selection(
                ui,
                drawing_manager,
                cursor_modes,
                response,
                price_rect,
                &x_to_bar_edit,
                &y_to_price,
                &translate_price,
            );
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

        drawing_manager.update_pane_screen_coords(bar_to_x, price_to_y);

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
            time_formatter: self.horizontal_scale.is_none().then_some(time_format.0),
            bar_seconds: time_format.1,
            instrument: price_member.and(self.instrument()),
            bars: self.state.shown(),
            revision: self.state.revision(),
            bar_to_x: &bar_to_x,
            price_to_y: &price_to_y,
            y_to_price: &|y| price_map.y_to_price(y),
            time_axis: self.horizontal_scale.is_none(),
            executions: self.feed_link.as_ref().and_then(|link| {
                if self.replay.is_some() {
                    link.replay_footprint.as_ref()?.1.as_ref()
                } else {
                    link.footprint.as_ref()
                }
            }),
            timeframe: self.drawing_timeframe(),
            finer: &finer,
        };
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

#[cfg(test)]
mod tests {
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType, TimeframeVisibility};
    use crate::model::{ChartType, Timeframe};
    use crate::widget::RightClickTarget;
    use crate::widget::test_frames::{chart, frame, shapes};
    use egui::{Color32, Context, Event, Key, Modifiers, PointerButton, Shape};

    /// Drawings use raw prices from their pane's series, including changes and level labels.
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
        let mut chart = chart(&[90.0], &[110.0]);
        use DrawingToolType::*;
        for (id, (tool, anchors)) in [
            (Measure, vec![(30.0, 100.0), (40.0, 95.0)]),
            (Forecast, vec![(30.0, 100.0), (40.0, 105.0)]),
            (
                ProjectionTool,
                vec![(30.0, 100.0), (40.0, 104.0), (50.0, 101.0)],
            ),
            (LongPos, vec![(30.0, 100.0), (40.0, 106.0)]),
            (FibonacciRetracement, vec![(30.0, 90.0), (40.0, 110.0)]),
            (Rect, vec![(30.0, 92.0), (40.0, 108.0)]),
            (PriceLabel, vec![(30.0, 102.0)]),
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
        chart.drawings_mut().select(5);
        let texts = |chart: &mut crate::Chart, time| {
            shapes(&ctx, chart, time, vec![])
                .into_iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let mut failures = Vec::new();
        if !texts(&mut chart, 0.0).iter().any(|text| text == "102.00") {
            failures.push("built-in price label".to_owned());
        }
        chart.set_price_formatter(marked(" pts"));
        let formatted = texts(&mut chart, 0.1);
        // Hand values: 95-100=-5, 105-100=5, |104-100|=4, 101+4=105,
        // target 106 and mirrored stop 94, midpoint 100, height 16 at 16/92*100 percent.
        for expected in [
            "-5.0 pts (-5.00%)",
            "+5.0 pts (+5.00%)",
            "4.0 pts (4.0%)",
            "Target: 105.0 pts",
            "Target: 6.0 pts (6.000%)",
            "Stop: 6.0 pts (6.000%)",
            "(100.0 pts)",
            "16.0 pts (17.39%)",
            "102.0 pts",
        ] {
            if !formatted.iter().any(|text| text.contains(expected)) {
                failures.push(format!("missing {expected}"));
            }
        }
        assert!(chart.set_series_formatter(SeriesId::MAIN, marked(" main")));
        let formatted = texts(&mut chart, 0.2);
        if !formatted.iter().any(|text| text == "102.0 main")
            || formatted.iter().any(|text| text == "102.0 pts")
        {
            failures.push("main series formatter overrides chart formatter".to_owned());
        }
        let series = chart.add_series(Default::default());
        chart.set_data(series, chart.bars(SeriesId::MAIN).to_vec());
        let pane = chart.add_pane();
        assert!(chart.move_series_to_pane(series, pane));
        assert!(chart.set_series_formatter(series, marked(" alt")));
        let mut drawing = Drawing::new(7, PriceLabel);
        drawing.chart_points = vec![ChartPoint::new(30.0, 102.0)];
        drawing.completed = true;
        drawing.pane = Some(series.0);
        chart.drawings_mut().drawings.push(drawing);
        let formatted = texts(&mut chart, 0.3);
        for expected in ["102.0 main", "102.0 alt"] {
            if !formatted.iter().any(|text| text == expected) {
                failures.push(format!("missing {expected}"));
            }
        }
        // A host may supply its own sign or put a currency before a negative number.
        chart.set_series_formatter(SeriesId::MAIN, None);
        for (prefix, change, expected) in [
            (false, 5.0, "+5.0 pts (+5.00%)"),
            (false, -5.0, "-5.0 pts (-5.00%)"),
            (true, 5.0, "+USD 5.0 (+5.00%)"),
            (true, -5.0, "USD -5.0 (-5.00%)"),
        ] {
            chart.set_price_formatter(Some(Arc::new(CustomPriceFormatter::new(move |price| {
                if prefix {
                    format!("USD {price:.1}")
                } else {
                    format!("{price:+.1} pts")
                }
            }))));
            for drawing in &mut chart.drawings_mut().drawings[..2] {
                drawing.chart_points[1].price = 100.0 + change;
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
            assert_eq!(
                clones.load(Ordering::Relaxed),
                shows,
                "forecast: {forecast}"
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
            for (cancel, batched, secondary_drag) in [
                (false, false, false),
                (true, false, false),
                (false, true, false),
                (false, false, true),
            ] {
                let ctx = Context::default();
                let mut chart = chart(&[10.0], &[20.0]);
                frame(&ctx, &mut chart, 0.0, vec![]);
                chart.drawings_mut().set_active_tool(Some(tool));
                let map = chart.get_chart_mapping();
                let a = pos2(map.idx_to_x(30), map.price_to_y(16.0));
                let b = pos2(map.idx_to_x(50), map.price_to_y(14.0));
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
                assert_eq!(preview.chart_points[0].bar_idx, 30.0, "{tool:?}");
                assert_eq!(preview.chart_points[1].bar_idx, 50.0, "{tool:?}");
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
                    assert_eq!(drawing.chart_points[0].bar_idx, 30.0, "{tool:?}");
                    assert_eq!(drawing.chart_points[1].bar_idx, 50.0, "{tool:?}");
                    if tool == Measure {
                        // The anchors span 50 - 30 = 20 bars, including after completion.
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

    /// Closing uses placed anchors, Escape keeps paths with at least two, and two clicks place
    /// curves with editable controls between their endpoints.
    #[test]
    fn multi_point_shapes_close_finish_on_escape_and_curves_take_two_clicks() {
        use DrawingToolType::{Curve, DoubleCurve, Path, Polyline};
        let a = (30, 16.0);
        let b = (50, 14.0);
        let c = (40, 12.0);
        for (tool, clicks, escape, kept) in [
            (Polyline, vec![a, b, c, a], false, Some((3, true, 40.0))),
            (Polyline, vec![a, b, a], true, Some((3, false, 30.0))),
            (Polyline, vec![a], true, None),
            (Path, vec![a, b], true, Some((2, false, 50.0))),
            (Path, vec![a], true, None),
            (Curve, vec![a, b], false, Some((3, false, 50.0))),
            (DoubleCurve, vec![a, b], false, Some((4, false, 50.0))),
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
            for (i, (bar, price)) in clicks.into_iter().enumerate() {
                let pos = egui::pos2(map.idx_to_x(bar), map.price_to_y(price));
                let time = 1.0 + i as f64;
                frame(&ctx, &mut chart, time, vec![Event::PointerMoved(pos)]);
                frame(&ctx, &mut chart, time + 0.1, vec![button(pos, true)]);
                frame(&ctx, &mut chart, time + 0.2, vec![button(pos, false)]);
            }
            if escape {
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
        }
    }

    /// Shift-click on empty space uses the ruler without persisting it or bypassing availability.
    #[test]
    fn shift_click_on_empty_space_measures_until_the_next_click() {
        use crate::drawings::ToolAvailability;
        use egui::pos2;
        for enabled in [true, false] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.drawings_mut().registry.set_availability(
                DrawingToolType::Measure,
                ToolAvailability {
                    visible: enabled,
                    enabled,
                },
            );
            let map = chart.get_chart_mapping();
            let a = pos2(map.idx_to_x(30), map.price_to_y(16.0));
            let b = pos2(map.idx_to_x(50), map.price_to_y(14.0));
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
                assert_eq!(
                    chart
                        .drawings()
                        .measurement
                        .as_ref()
                        .unwrap()
                        .chart_points
                        .iter()
                        .map(|p| p.bar_idx)
                        .collect::<Vec<_>>(),
                    [30.0, 50.0]
                );
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
    fn shift_rectangle_creation_is_square_in_preview_and_after_release() {
        use egui::{pos2, vec2};
        for (shift, delta) in [
            (true, vec2(240.0, 60.0)),
            (false, vec2(240.0, 60.0)),
            (true, vec2(-240.0, 60.0)),
            (true, vec2(240.0, -60.0)),
            (true, vec2(-240.0, -60.0)),
            (true, vec2(1000.0, 60.0)),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart
                .drawings_mut()
                .set_active_tool(Some(DrawingToolType::Rect));
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
                    "{delta:?}: {points:?}"
                );
                assert!(
                    (d.x.abs() - d.y.abs()).abs() < 0.01,
                    "{delta:?}: {points:?}"
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
        ]) {
            let ctx = Context::default();
            let mut chart = chart(&[0.0, 10.0, 10.0], &[30.0, 20.0, 20.0]);
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
    /// magnet targets while it is hidden.
    #[test]
    fn magnet_ignores_hidden_main_series_bars_and_volume() {
        use egui::Pos2;

        let mut observed = Vec::new();
        for (visible, strong, target) in [
            (true, false, "volume"),
            (true, true, "volume"),
            (true, false, "high"),
            (true, true, "high"),
            (false, false, "volume"),
            (false, true, "volume"),
            (false, false, "high"),
            (false, true, "high"),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            chart.config_mut().show_volume = true;
            chart.drawings_mut().options.magnet_mode = true;
            chart.drawings_mut().options.strong_magnet = strong;
            chart.drawings_mut().update_snap_options();
            frame(&ctx, &mut chart, 0.1, vec![]);
            chart.set_main_series_visible(visible);
            frame(&ctx, &mut chart, 0.2, vec![]);
            let map = chart.get_chart_mapping();
            // Every bar has volume 1, the visible maximum, so each column reaches the top of the
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
                ((anchor - y) * 100.0).round() / 100.0,
            ));
        }
        // Shown, the anchor lands on the value; hidden, it stays 1 point from where it was clicked.
        let expected = [
            (true, false, "volume", 0.0),
            (true, true, "volume", 0.0),
            (true, false, "high", 0.0),
            (true, true, "high", 0.0),
            (false, false, "volume", -1.0),
            (false, true, "volume", -1.0),
            (false, false, "high", 1.0),
            (false, true, "high", 1.0),
        ];
        assert!(
            observed == expected,
            "(main series visible, strong magnet, target, first anchor below the target) \
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
        // 100..200 with 10% margins spans 87.5..212.5, so the middle of the pane is price 150.
        assert!(
            observed
                .iter()
                .all(|(_, _, price, below)| *price == 150.0 && *below == 0.0),
            "(main series moved into the study's pane, strong magnet, first anchor price, \
             anchor below the painted level) observed {observed:?} expected price 150 and 0 \
             below in every row"
        );
    }

    /// With the eraser on, a click on a drawing deletes it, whether or not a drawing is
    /// selected, unless it is locked or Escape has turned the eraser off.
    #[test]
    fn an_eraser_click_deletes_the_drawing_unless_it_is_locked() {
        for (tool, selected, locked, escaped, kept) in [
            (DrawingToolType::HorizontalLine, false, false, false, 0),
            (DrawingToolType::HorizontalLine, true, false, false, 0),
            (DrawingToolType::HorizontalLine, true, true, false, 1),
            (DrawingToolType::HorizontalLine, false, false, true, 1),
            (DrawingToolType::FibonacciExtension, false, false, false, 0),
            (DrawingToolType::FibonacciExtension, true, false, false, 0),
            (DrawingToolType::FibonacciExtension, true, true, false, 1),
            (DrawingToolType::FibonacciExtension, false, false, true, 1),
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
                    config.levels[0].caption = "Target".into();
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
                "{tool:?}: selected {selected}, locked {locked}, escaped {escaped}"
            );
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
    /// paints and hits only in its own pane.
    #[cfg(feature = "ui")]
    #[test]
    fn vertical_line_reaches_other_panes_without_leaving_its_own_scale() {
        use crate::drawings::{ChartPoint, DrawingToolType};
        use crate::studies::RSI;
        use egui::Pos2;

        const COLOR: [u8; 4] = [23, 57, 181, 255];
        for in_study in [false, true] {
            let ctx = Context::default();
            let mut chart = chart(&[90.0], &[110.0]);
            let study = chart.add_indicator(Box::new(RSI::new(14))).0;
            let price = if in_study { 50.0 } else { 100.0 };
            let mut line = Drawing::new(1, DrawingToolType::VerticalLine);
            line.chart_points = vec![ChartPoint::new(60.0, price)];
            line.completed = true;
            line.color = COLOR;
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
            let color = Color32::from_rgba_unmultiplied(23, 57, 181, 255);
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
                (texts, lines)
            };
            let (texts, lines) = paint(&mut chart, 0.1);
            let x = chart.drawings().drawings[0].points[0].x;
            let row = format!("in study {in_study}");
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
            assert_eq!(texts.len(), 1, "{row}");
            assert!(own.y_range().contains(texts[0].y), "{row}: {texts:?}");
            // The panes share one time axis, which tags the line whichever pane it belongs to.
            assert!(
                chart
                    .drawings()
                    .axis_tags(main, Color32::WHITE)
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
            let (_, lines) = paint(&mut chart, 1.0);
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
