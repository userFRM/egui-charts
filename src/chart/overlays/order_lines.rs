//! Order Line Visualization
//!
//! Renders order lines on the chart showing entry prices, stop losses,
//! take profits, and pending orders with labels and cancel buttons.

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::ext::HasDesignTokens;
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;

/// Unique string identifier for an order, used to reference specific order lines.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OrderId(pub String);

impl OrderId {
    /// Create a new order ID from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for OrderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Side of the order: buy or sell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    /// Long / buy order.
    Buy,
    /// Short / sell order.
    Sell,
}

impl OrderSide {
    /// Get display text
    pub fn label(&self) -> &'static str {
        match self {
            OrderSide::Buy => "BUY",
            OrderSide::Sell => "SELL",
        }
    }
}

/// Categorization of an order line, determining its visual style and label.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderLineType {
    /// Entry order (limit or market)
    Entry,
    /// Stop loss order
    StopLoss,
    /// Take profit order (with optional partial percentage)
    TakeProfit { partial_pct: Option<f64> },
    /// Pending limit order
    PendingLimit,
    /// Pending stop order
    PendingStop,
}

impl OrderLineType {
    /// Get short label for the line type
    pub fn short_label(&self) -> &'static str {
        match self {
            OrderLineType::Entry => "ENT",
            OrderLineType::StopLoss => "SL",
            OrderLineType::TakeProfit { .. } => "TP",
            OrderLineType::PendingLimit => "LMT",
            OrderLineType::PendingStop => "STP",
        }
    }
}

/// A single order line rendered as a dashed horizontal line across the chart
/// with a label, type indicator, and optional cancel button.
#[derive(Debug, Clone)]
pub struct OrderLine {
    /// Unique order identifier
    pub order_id: OrderId,
    /// Price level for the line
    pub price: f64,
    /// Type of order line
    pub line_type: OrderLineType,
    /// Display label (e.g., "Submx | -4 LMT | 100.00 $")
    pub label: String,
    /// Order side (buy/sell)
    pub side: OrderSide,
    /// Quantity
    pub quantity: f64,
    /// Whether this order is currently active (can be cancelled)
    pub is_active: bool,
    /// Whether this order is selected/highlighted
    pub is_selected: bool,
}

impl OrderLine {
    /// Create a new entry order line
    pub fn entry(
        order_id: impl Into<String>,
        price: f64,
        side: OrderSide,
        quantity: f64,
        label: impl Into<String>,
    ) -> Self {
        Self {
            order_id: OrderId::new(order_id),
            price,
            line_type: OrderLineType::Entry,
            label: label.into(),
            side,
            quantity,
            is_active: true,
            is_selected: false,
        }
    }

    /// Create a stop loss order line
    pub fn stop_loss(
        order_id: impl Into<String>,
        price: f64,
        side: OrderSide,
        quantity: f64,
        label: impl Into<String>,
    ) -> Self {
        Self {
            order_id: OrderId::new(order_id),
            price,
            line_type: OrderLineType::StopLoss,
            label: label.into(),
            side,
            quantity,
            is_active: true,
            is_selected: false,
        }
    }

    /// Create a take profit order line
    pub fn take_profit(
        order_id: impl Into<String>,
        price: f64,
        side: OrderSide,
        quantity: f64,
        label: impl Into<String>,
        partial_pct: Option<f64>,
    ) -> Self {
        Self {
            order_id: OrderId::new(order_id),
            price,
            line_type: OrderLineType::TakeProfit { partial_pct },
            label: label.into(),
            side,
            quantity,
            is_active: true,
            is_selected: false,
        }
    }

    /// Create a pending limit order line
    pub fn pending_limit(
        order_id: impl Into<String>,
        price: f64,
        side: OrderSide,
        quantity: f64,
        label: impl Into<String>,
    ) -> Self {
        Self {
            order_id: OrderId::new(order_id),
            price,
            line_type: OrderLineType::PendingLimit,
            label: label.into(),
            side,
            quantity,
            is_active: true,
            is_selected: false,
        }
    }

    /// Set selected state
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.is_selected = selected;
        self
    }

    /// Set active state
    pub fn with_active(mut self, active: bool) -> Self {
        self.is_active = active;
        self
    }
}

/// Action returned from [`OrderLineOverlay::render`] when the user interacts
/// with an order line (click, drag, cancel).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum OrderLineAction {
    /// No action
    #[default]
    None,
    /// Order line was clicked/selected
    Selected(OrderId),
    /// Cancel button was clicked
    CancelRequested(OrderId),
    /// Order line is being dragged to new price
    DragStarted(OrderId),
    /// Order line drag in progress
    Dragging(OrderId, f64),
    /// Order line drag completed
    DragCompleted(OrderId, f64),
}

/// A collection of [`OrderLine`]s to render on the chart.
///
/// Manages adding, removing, and rendering order lines. The [`render`](Self::render)
/// method returns an [`OrderLineAction`] describing any user interaction.
#[derive(Debug, Clone, Default)]
pub struct OrderLineOverlay {
    /// All order lines to display
    pub lines: Vec<OrderLine>,
}

impl OrderLineOverlay {
    /// Create a new empty overlay
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Add an order line
    pub fn add_line(&mut self, line: OrderLine) {
        self.lines.push(line);
    }

    /// Remove an order line by ID
    pub fn remove_line(&mut self, order_id: &OrderId) {
        self.lines.retain(|l| &l.order_id != order_id);
    }

    /// Clear all lines
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Get a line by ID
    pub fn get_line(&self, order_id: &OrderId) -> Option<&OrderLine> {
        self.lines.iter().find(|l| &l.order_id == order_id)
    }

    /// Get mutable reference to a line by ID
    pub fn get_line_mut(&mut self, order_id: &OrderId) -> Option<&mut OrderLine> {
        self.lines.iter_mut().find(|l| &l.order_id == order_id)
    }

    /// Render all order lines
    pub fn render(
        &self,
        ui: &mut Ui,
        chart_rect: Rect,
        price_to_y: impl Fn(f64) -> f32,
    ) -> OrderLineAction {
        let mut action = OrderLineAction::None;

        for line in &self.lines {
            let y = price_to_y(line.price);

            // Skip if outside visible area
            if y < chart_rect.top() - 20.0 || y > chart_rect.bottom() + 20.0 {
                continue;
            }

            let line_action = render_order_line(ui, chart_rect, y, line);
            if !matches!(line_action, OrderLineAction::None) {
                action = line_action;
            }
        }

        action
    }
}

/// Render a single order line
fn render_order_line(ui: &mut Ui, chart_rect: Rect, y: f32, line: &OrderLine) -> OrderLineAction {
    let mut action = OrderLineAction::None;

    // Get colors based on order type and side
    let (line_color, label_bg, label_text_color) = get_order_colors(ui, line);
    let bearish = ui.bearish_color();

    // Line thickness based on selection
    let line_width = if line.is_selected {
        DESIGN_TOKENS.spacing.xs
    } else {
        DESIGN_TOKENS.spacing.hairline
    };

    // Calculate rects first
    let label_width = DESIGN_TOKENS.sizing.charts_ext.order_line_label_width;
    let label_height = DESIGN_TOKENS.sizing.charts_ext.order_line_label_height;
    let label_rect = Rect::from_min_size(
        Pos2::new(
            chart_rect.right() - label_width - DESIGN_TOKENS.spacing.lg,
            y - label_height / 2.0,
        ),
        Vec2::new(label_width, label_height),
    );

    let cancel_size = DESIGN_TOKENS.sizing.icon_sm;
    let cancel_rect = Rect::from_min_size(
        Pos2::new(
            label_rect.right() + DESIGN_TOKENS.spacing.xs,
            y - cancel_size / 2.0,
        ),
        Vec2::splat(cancel_size),
    );

    let type_rect = Rect::from_min_size(
        Pos2::new(
            chart_rect.left() + DESIGN_TOKENS.spacing.sm,
            y - DESIGN_TOKENS.spacing.lg,
        ),
        Vec2::new(DESIGN_TOKENS.sizing.button_md, DESIGN_TOKENS.sizing.icon_sm),
    );

    // Allocate interactive areas first (requires mutable borrow)
    let label_response = ui.allocate_rect(label_rect, Sense::click());
    if label_response.clicked() {
        action = OrderLineAction::Selected(line.order_id.clone());
    }

    let cancel_response = if line.is_active {
        let resp = ui.allocate_rect(cancel_rect, Sense::click());
        if resp.clicked() {
            action = OrderLineAction::CancelRequested(line.order_id.clone());
        }
        Some(resp)
    } else {
        None
    };

    // Now do all the painting (only immutable borrow needed)
    let painter = ui.painter();

    // Draw dashed line across chart
    let dash_length = 6.0;
    let gap_length = 4.0;
    let mut x = chart_rect.left();

    while x < chart_rect.right() - 100.0 {
        let segment_end = (x + dash_length).min(chart_rect.right() - 100.0);
        painter.line_segment(
            [Pos2::new(x, y), Pos2::new(segment_end, y)],
            Stroke::new(line_width, line_color),
        );
        x += dash_length + gap_length;
    }

    // Label background
    painter.rect_filled(label_rect, CornerRadius::same(3), label_bg);

    // Label border if selected
    if line.is_selected {
        painter.rect_stroke(
            label_rect,
            CornerRadius::same(3),
            Stroke::new(DESIGN_TOKENS.stroke.hairline, line_color),
            StrokeKind::Inside,
        );
    }

    // Label text
    let label_text_display = if line.label.len() > 18 {
        format!("{}...", &line.label[..15])
    } else {
        line.label.clone()
    };

    painter.text(
        label_rect.center(),
        Align2::CENTER_CENTER,
        &label_text_display,
        FontId::proportional(typography::SM),
        label_text_color,
    );

    // Draw cancel button if active
    if let Some(cancel_resp) = cancel_response {
        let cancel_bg = if cancel_resp.hovered() {
            bearish
        } else {
            Color32::from_rgba_unmultiplied(bearish.r(), bearish.g(), bearish.b(), 180)
        };

        painter.rect_filled(cancel_rect, CornerRadius::same(2), cancel_bg);

        // X icon
        let x_icon_color = DESIGN_TOKENS.semantic.chart.crosshair_label_text;
        let x_margin = 4.0;
        painter.line_segment(
            [
                cancel_rect.left_top() + Vec2::splat(x_margin),
                cancel_rect.right_bottom() - Vec2::splat(x_margin),
            ],
            Stroke::new(DESIGN_TOKENS.stroke.medium, x_icon_color),
        );
        painter.line_segment(
            [
                cancel_rect.right_top() + Vec2::new(-x_margin, x_margin),
                cancel_rect.left_bottom() + Vec2::new(x_margin, -x_margin),
            ],
            Stroke::new(DESIGN_TOKENS.stroke.medium, x_icon_color),
        );
    }

    // Type indicator on the left side of line
    let type_label = line.line_type.short_label();
    painter.rect_filled(type_rect, CornerRadius::same(2), label_bg);
    painter.text(
        type_rect.center(),
        Align2::CENTER_CENTER,
        type_label,
        FontId::proportional(typography::XS),
        label_text_color,
    );

    action
}

/// Get colors for order line based on type and side
fn get_order_colors(ui: &Ui, line: &OrderLine) -> (Color32, Color32, Color32) {
    let bullish = ui.bullish_color();
    let bearish = ui.bearish_color();

    match line.line_type {
        OrderLineType::Entry | OrderLineType::PendingLimit | OrderLineType::PendingStop => {
            // Entry orders use side color
            let base = match line.side {
                OrderSide::Buy => bullish,
                OrderSide::Sell => bearish,
            };
            let [r, g, b, _] = base.to_array();
            (
                base,
                Color32::from_rgba_unmultiplied(r, g, b, 220),
                DESIGN_TOKENS.semantic.chart.crosshair_label_text,
            )
        }
        OrderLineType::StopLoss => {
            // Stop loss is always red/bearish
            let [r, g, b, _] = bearish.to_array();
            (
                bearish,
                Color32::from_rgba_unmultiplied(r, g, b, 220),
                DESIGN_TOKENS.semantic.chart.crosshair_label_text,
            )
        }
        OrderLineType::TakeProfit { .. } => {
            // Take profit is always green/bullish
            let [r, g, b, _] = bullish.to_array();
            (
                bullish,
                Color32::from_rgba_unmultiplied(r, g, b, 220),
                DESIGN_TOKENS.semantic.chart.crosshair_label_text,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_line_creation() {
        let line = OrderLine::entry("order1", 100.0, OrderSide::Buy, 1.0, "Test Order");
        assert_eq!(line.order_id.0, "order1");
        assert_eq!(line.price, 100.0);
        assert!(line.is_active);
    }

    #[test]
    fn test_overlay_management() {
        let mut overlay = OrderLineOverlay::new();

        overlay.add_line(OrderLine::entry(
            "o1",
            100.0,
            OrderSide::Buy,
            1.0,
            "Order 1",
        ));
        overlay.add_line(OrderLine::stop_loss("o2", 95.0, OrderSide::Buy, 1.0, "SL"));

        assert_eq!(overlay.lines.len(), 2);

        overlay.remove_line(&OrderId::new("o1"));
        assert_eq!(overlay.lines.len(), 1);

        overlay.clear();
        assert!(overlay.lines.is_empty());
    }
}
