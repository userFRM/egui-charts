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

use crate::chart::cursor_modes::CursorModeState;
use crate::drawings::DrawingInteractionMode;
use crate::drawings::DrawingManager;
use crate::drawings::rendering::ChartView;
use crate::scales::PriceMap;
use crate::studies::IndicatorValue;
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
    pub fn handle_drawings(
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
        let rect_min_x = price_rect.min.x;

        let rect_width = price_rect.width();

        // Coordinate conversion closures
        // Use idx_to_coord_precise to preserve fractional bar indices for accurate drawing positions
        // CRITICAL: Must pass the actual rect width, not timescale.width (which may differ)
        let bar_to_x = |bar_idx: f32| -> f32 {
            timescale.idx_to_coord_precise(bar_idx, rect_min_x, rect_width)
        };
        // x_to_bar_snapped: ROUNDED to nearest candle center (for line tools)
        let x_to_bar_snapped =
            |x: f32| -> f32 { timescale.coord_to_idx(x, rect_min_x, rect_width).round() };
        // x_to_bar_precise: Full precision for freeform tools (Brush, Highlighter)
        // Prevents pixelation when zooming - preserves sub-pixel positioning
        let x_to_bar_precise =
            |x: f32| -> f32 { timescale.coord_to_idx(x, rect_min_x, rect_width) };
        let price_to_y = |price: f64| -> f32 { price_map.price_to_y(price) };
        // Points placed with the pointer stay on the pane.
        let y_to_price =
            |y: f32| -> f64 { price_map.y_to_price(y.clamp(price_rect.min.y, price_rect.max.y)) };

        // Update all drawings' screen coords before rendering (handles pan/zoom)
        drawing_manager.update_all_screen_coords(bar_to_x, price_to_y);

        // Only gather bars within the pointer's screen-space tolerance. Rebuild after pan/zoom.
        drawing_manager.snap_targets.clear();
        if drawing_manager.options.magnet_mode
            && let Some(pointer) = response
                .interact_pointer_pos()
                .or_else(|| response.hover_pos())
        {
            let distance = drawing_manager.options.magnet_distance;
            let bars = &self.state.data().bars;
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
                for price in [bar.open, bar.high, bar.low, bar.close] {
                    add_price(price);
                }
                for study in self
                    .studies
                    .indicators()
                    .iter()
                    .filter(|s| !strong && s.is_visible() && s.is_overlay())
                {
                    match study.values().get(i) {
                        Some(IndicatorValue::Single(price)) => add_price(*price),
                        Some(IndicatorValue::Multiple(prices)) => {
                            for price in prices.iter().take(study.line_cnt()) {
                                add_price(*price);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Handle drawing interaction
        // Select converter based on tool type: freeform tools (ContinuousDraw) use precise,
        // all other tools snap to candle centers
        if let Some(active_tool) = drawing_manager.active_tool {
            let uses_precise =
                active_tool.interaction_mode() == DrawingInteractionMode::ContinuousDraw;
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
                &x_to_bar_snapped,
                &y_to_price,
            );
        }

        // Handle keyboard shortcuts for drawing manipulation
        selection::handle_keyboard_shortcuts(ui, drawing_manager);

        drawing_manager.update_all_screen_coords(bar_to_x, price_to_y);

        // Render existing drawings against the chart's bars
        let view = ChartView {
            instrument: self.instrument(),
            bars: &self.state.data().bars,
            revision: self.state.revision(),
            bar_to_x: &bar_to_x,
            price_to_y: &price_to_y,
        };
        drawing_manager.render_all(painter, price_rect, &view);
    }

    /// Render eraser highlight for hovered drawing
    pub fn render_eraser_highlight(
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

    /// Drawing anchors use nearby bar and visible overlay values only while the magnet is on,
    /// including after a view change and a handle drag that can be undone and redone.
    #[test]
    fn drawing_magnet_places_and_drags_anchors_in_data_coordinates() {
        use crate::studies::{CustomIndicator, IndicatorValue};
        use egui::{Pos2, vec2};

        for (magnet, strong, price, overlay_visible, snaps, offset) in [
            (true, false, 10.0, true, true, 3.0),
            (true, false, 20.0, true, true, 3.0),
            (true, false, 12.0, true, true, 3.0),
            (true, false, 15.0, true, true, 3.0),
            (true, false, 17.0, true, true, 3.0),
            (false, false, 15.0, true, false, 3.0),
            (true, false, 15.0, false, false, 3.0),
            (true, true, 20.0, false, true, 40.0),
            (true, false, 20.0, false, false, 40.0),
            (true, true, 10.0, false, true, 40.0),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[0.0, 10.0, 10.0], &[30.0, 20.0, 20.0]);
            let overlay = CustomIndicator::new("Two levels", |bars| {
                bars.iter()
                    .map(|_| IndicatorValue::Multiple(vec![15.0, 17.0]))
                    .collect()
            })
            .with_line_cnt(2);
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
                chart.drawings_mut().update_snap_options();
                time += 0.1;
                frame(&ctx, &mut chart, time, vec![]);
                let map = chart.get_chart_mapping();
                let (first, last) = chart.state.visible_range();
                let middle = (first + last) / 2;
                let bar = middle + (4 - middle % 3) % 3;
                let target = Pos2::new(map.idx_to_x(bar), map.price_to_y(price));
                let near = target + vec2(2.0, if price == 10.0 { -offset } else { offset });
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
                for pos in [near, Pos2::new(map.idx_to_x(bar + 8), map.price_to_y(13.0))] {
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
                let from = drawing.points[0];
                let before = drawing.chart_points[0];
                chart.drawings_mut().select(id);
                let to = Pos2::new(map.idx_to_x(bar + 3), map.price_to_y(price))
                    + vec2(2.0, if price == 20.0 { offset } else { -offset });
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
                assert!(chart.drawings_mut().undo());
                assert_eq!(chart.drawings().drawings[0].chart_points[0], before);
                assert!(chart.drawings_mut().redo());
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
    /// set to another interval or a price-based chart type.
    #[test]
    fn a_hidden_drawing_is_neither_drawn_nor_hit() {
        let color = Color32::from_rgb(1, 2, 3);
        for (interval, kind, shown) in [
            (Timeframe::Day1, ChartType::Candles, true),
            (Timeframe::Hour1, ChartType::Candles, false),
            (Timeframe::Day1, ChartType::Renko, false),
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
            chart.set_chart_type(kind);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]).iter().any(|s| {
                matches!(s, Shape::LineSegment { stroke, .. } if stroke.color == color)
                    || matches!(s, Shape::Circle(c) if c.fill == color)
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
