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
use crate::widget::Chart;
use egui::{Painter, Rect, Response, Ui};

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

        // Render existing drawings against the chart's bars
        let view = ChartView {
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
        selection::render_eraser_highlight(painter, drawing_manager, cursor_modes);
    }
}

#[cfg(test)]
mod tests {
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType, TimeframeVisibility};
    use crate::model::{ChartType, Timeframe};
    use crate::widget::RightClickTarget;
    use crate::widget::test_frames::{chart, frame, shapes};
    use egui::{Color32, Context, Event, Key, Modifiers, PointerButton, Shape};

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

            frame(&ctx, &mut chart, 0.0, vec![]);
            let on_line = chart.drawings().drawings[0].points[0];
            chart.set_chart_type(kind);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![])
                .iter()
                .any(|s| matches!(s, Shape::LineSegment { stroke, .. } if stroke.color == color));
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
