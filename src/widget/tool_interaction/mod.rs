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

mod effects;
mod interaction;
mod selection;
mod text;

pub(super) use selection::render_eraser_highlight as render_pane_eraser_highlight;

use crate::chart::cursor_modes::CursorModeState;
use crate::drawings::DrawingInteractionMode;
use crate::drawings::DrawingManager;
use crate::drawings::rendering::ChartView;
use crate::scales::PriceMap;
use crate::widget::Chart;
use egui::{Painter, Pos2, Rect, Response, Ui};

impl Chart {
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
        pane: Option<super::PaneId>,
    ) {
        // New drawings belong to the pane's first member, or to an empty pane itself.
        let mut owner = None;
        let members: Vec<_> = if let Some(pane) = pane {
            let pane = self.pane_layout.panes.iter().find(|p| p.id == pane);
            owner = pane.map(|p| p.members.first().map_or(p.id.0, |id| id.0));
            pane.map(|p| {
                std::iter::once(Some(p.id.0))
                    .chain(p.members.iter().map(|id| Some(id.0)))
                    .collect()
            })
            .unwrap_or_default()
        } else {
            std::iter::once(None)
                .chain(
                    self.study_ids
                        .iter()
                        .zip(self.rendered_studies().indicators())
                        .filter(|(_, study)| study.is_overlay())
                        .map(|(id, _)| Some(id.0)),
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
                    first.saturating_add(1).min(bars.len())
                }
            } else {
                ((x_to_bar_precise(pointer.x + distance).ceil().max(0.0) as usize)
                    .saturating_add(1))
                .min(bars.len())
            };
            for (i, bar) in bars.iter().enumerate().take(end).skip(first) {
                let x = bar_to_x(i as f32);
                let mut add_price = |price: f64| {
                    if price.is_finite() {
                        let point = Pos2::new(x, price_to_y(price));
                        if point.is_finite() && (strong || price_rect.contains(point)) {
                            drawing_manager.snap_targets.push(point);
                        }
                    }
                };
                if pane.is_none() {
                    for price in [bar.open, bar.high, bar.low, bar.close] {
                        add_price(price);
                    }
                }
                for study in self
                    .study_ids
                    .iter()
                    .zip(self.rendered_studies().indicators())
                    .filter(|(id, s)| {
                        drawing_manager.options.snap_to_indicators
                            && s.is_visible()
                            && if pane.is_some() {
                                members.contains(&Some(id.0))
                            } else {
                                !strong
                                    && s.is_overlay()
                                    && s.scale() == crate::studies::StudyScale::Main
                            }
                    })
                    .map(|(_, study)| study)
                {
                    for line in (0..study.line_count()).filter(|line| {
                        let plot = study.plot_style(*line);
                        plot.visible
                            && !matches!(
                                plot.plot_type,
                                crate::studies::PlotType::BarColor
                                    | crate::studies::PlotType::BackgroundColor
                            )
                    }) {
                        if let Some(price) = study.plot_value(line, i) {
                            add_price(price);
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
        let owns_gesture = drawing_manager
            .curr_drawing
            .as_ref()
            .filter(|d| !d.chart_points.is_empty())
            .or_else(|| {
                drawing_manager
                    .dragging_handle
                    .and_then(|(id, _)| drawing_manager.drawings.iter().find(|d| d.id == id))
            })
            .is_none_or(|d| drawing_manager.owns_pane(d));
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
            drawing_manager.drawings.last().map(|d| d.id)
        } else if response.double_clicked() || response.triple_clicked() {
            response
                .interact_pointer_pos()
                .and_then(|pos| drawing_manager.hit_test(pos))
        } else {
            None
        };
        if let Some(id) = edit_id {
            text::begin_edit(drawing_manager, id);
        }

        drawing_manager.update_pane_screen_coords(bar_to_x, price_to_y);

        // Render existing drawings against the chart's bars
        let view = ChartView {
            instrument: self.instrument(),
            bars: self.state.shown(),
            revision: self.state.revision(),
            bar_to_x: &bar_to_x,
            price_to_y: &price_to_y,
            y_to_price: &|y| price_map.y_to_price(y),
            time_axis: self.horizontal_scale.is_none(),
            executions: self
                .feed_link
                .as_ref()
                .and_then(|link| link.footprint.as_ref()),
        };
        drawing_manager.render_all(painter, price_rect, &view);
        if drawing_manager.text_editor.as_ref().is_some_and(|editor| {
            drawing_manager
                .drawings
                .iter()
                .any(|d| d.id == editor.id && drawing_manager.owns_pane(d))
        }) {
            text::show_editor(ui, drawing_manager, response.id, price_rect);
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
            (true, true, 20.0, false, true, 40.0, false),
            (true, false, 20.0, false, false, 40.0, false),
            (true, true, 10.0, false, true, 40.0, false),
            (true, true, 20.0, false, true, 3.0, true),
        ]
        .into_iter()
        .map(|case| (case, "main", true))
        .chain([
            (
                (true, false, 15.0, true, false, 3.0, false),
                "independent",
                true,
            ),
            ((true, false, 15.0, true, false, 3.0, false), "none", true),
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
            .with_offsets(vec![if kind == "displaced" { 1 } else { 0 }; 2])
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
            for (visible, start) in [(100, 0), (40, 20)] {
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
                    chart.get_rendered_indicator_panes()[0].mapping
                } else {
                    chart.get_chart_mapping()
                };
                let (first, last) = chart.state.visible_range();
                let middle = (first + last) / 2;
                let bar = middle + (4 - middle % 3) % 3;
                let target = Pos2::new(map.idx_to_x(bar), map.price_to_y(price));
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
                let expected_price = if snaps { price } else { map.y_to_price(near.y) };
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
                        map.price_to_y(price) + if price == 20.0 { offset } else { -offset }
                    },
                ) + vec2(2.0, 0.0);
                step(&mut chart, vec![Event::PointerMoved(from)]);
                step(&mut chart, vec![button(from, true)]);
                step(&mut chart, vec![Event::PointerMoved(from + vec2(7.0, 0.0))]);
                step(&mut chart, vec![Event::PointerMoved(to)]);
                step(&mut chart, vec![button(to, false)]);
                let after = chart.drawings().drawings[0].chart_points[0];
                let want = if snaps { price } else { map.y_to_price(to.y) };
                assert!(
                    (after.price - want).abs() < 1e-5,
                    "drag: magnet {magnet}, price {price}, visible {visible}: {after:?}"
                );
                assert!(chart.drawings().dragging_handle.is_none());
                assert!(chart.undo());
                assert_eq!(chart.drawings().drawings[0].chart_points[0], before);
                assert!(chart.redo());
                assert_eq!(chart.drawings().drawings[0].chart_points[0], after);
            }
        }
    }

    /// With the eraser on, a click on a drawing deletes it, whether or not a drawing is
    /// selected, unless it is locked or Escape has turned the eraser off.
    #[test]
    fn an_eraser_click_deletes_the_drawing_unless_it_is_locked() {
        for (selected, locked, escaped, kept) in [
            (false, false, false, 0),
            (true, false, false, 0),
            (true, true, false, 1),
            (false, false, true, 1),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            let mut line = Drawing::new(1, DrawingToolType::HorizontalLine);
            line.chart_points = vec![ChartPoint::new(50.0, 15.0)];
            line.locked = locked;
            chart.drawings_mut().drawings.push(line);
            if selected {
                chart.drawings_mut().select(1);
            }
            chart.drawings_mut().set_eraser_mode(true);
            frame(&ctx, &mut chart, 0.0, vec![]);
            let pos = chart.drawings().drawings[0].points[0];
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
                "selected {selected}, locked {locked}, escaped {escaped}"
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
}
