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
use crate::widget::Chart;
use egui::{Painter, Rect, Response, Ui};

/// Actions emitted by cursor mode toolbar buttons.
///
/// These are returned to the caller so the UI layer can toggle the
/// corresponding cursor mode states.
#[derive(Clone, Debug, PartialEq)]
pub enum CursorModeAction {
    /// Toggle eraser mode active state
    ToggleEraser,
}

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
    /// - `last_close_price` - The most recent close price (for position drawings P&L)
    pub fn handle_drawings(
        &self,
        ui: &mut Ui,
        drawing_manager: &mut DrawingManager,
        cursor_modes: &mut CursorModeState,
        response: &Response,
        price_rect: Rect,
        adjusted_min: f64,
        adjusted_max: f64,
        painter: &Painter,
        last_close_price: Option<f64>,
        timescale: &crate::model::TimeScale,
    ) {
        let adjusted_range = (adjusted_max - adjusted_min).max(1e-12);
        let rect_min_x = price_rect.min.x;
        let rect_max_y = price_rect.max.y;
        let rect_height = price_rect.height();

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
        let price_to_y = |price: f64| -> f32 {
            let ratio = (price - adjusted_min) / adjusted_range;
            rect_max_y - (ratio as f32 * rect_height)
        };
        let y_to_price = |y: f32| -> f64 {
            let ratio = ((rect_max_y - y) / rect_height).clamp(0.0, 1.0) as f64;
            adjusted_min + ratio * adjusted_range
        };

        // Update all drawings' screen coords before rendering (handles pan/zoom)
        drawing_manager.update_all_screen_coords(bar_to_x, price_to_y);

        // Update real-time prices for position drawings
        if let Some(price) = last_close_price {
            drawing_manager.update_pos_prices(price);
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

        // Render existing drawings
        drawing_manager.render_all(painter, price_rect);
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
