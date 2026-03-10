//! Symbol Header Widget
//!
//! Displays symbol info, OHLC values, bid/ask prices, and volume
//! in a horizontal bar at the top of the chart area.

use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// Symbol information for display
#[derive(Clone, Debug)]
pub struct SymbolInfo {
    /// Ticker symbol (e.g. "AAPL")
    pub symbol: String,
    /// Full company name
    pub company_name: String,
    /// Exchange name (e.g. "NASDAQ")
    pub exchange: String,
    /// Chart timeframe (e.g. "1D")
    pub timeframe: String,
}

impl Default for SymbolInfo {
    fn default() -> Self {
        Self {
            symbol: "AAPL".to_string(),
            company_name: "Apple Inc".to_string(),
            exchange: "NASDAQ".to_string(),
            timeframe: "1D".to_string(),
        }
    }
}

/// OHLC price data for the current bar
#[derive(Clone, Debug, Default)]
pub struct OhlcData {
    /// Opening price
    pub open: f64,
    /// Highest price
    pub high: f64,
    /// Lowest price
    pub low: f64,
    /// Closing price
    pub close: f64,
    /// Absolute price change (close - open)
    pub change: f64,
    /// Percentage price change
    pub change_percent: f64,
    /// Trading volume
    pub volume: f64,
}

impl OhlcData {
    /// Create OHLC data and compute change values automatically
    pub fn new(open: f64, high: f64, low: f64, close: f64, volume: f64) -> Self {
        let change = close - open;
        let change_percent = if open != 0.0 {
            (change / open) * 100.0
        } else {
            0.0
        };
        Self {
            open,
            high,
            low,
            close,
            change,
            change_percent,
            volume,
        }
    }

    /// Return true if the bar closed at or above its open
    pub fn is_bullish(&self) -> bool {
        self.close >= self.open
    }
}

/// Bid/Ask quote data
#[derive(Clone, Debug, Default)]
pub struct QuoteData {
    /// Best bid price
    pub bid: f64,
    /// Best ask price
    pub ask: f64,
}

/// Configuration for the symbol header
#[derive(Clone, Debug)]
pub struct SymbolHeaderConfig {
    pub height: f32,
    pub bg_color: Color32,
    pub text_color: Color32,
    pub label_color: Color32,
    pub bullish_color: Color32,
    pub bearish_color: Color32,
    pub separator_color: Color32,
    pub show_company_name: bool,
    pub show_bid_ask: bool,
    pub show_volume: bool,
}

impl Default for SymbolHeaderConfig {
    fn default() -> Self {
        // Note: These are placeholder colors - actual rendering uses ui.style().visuals
        Self {
            height: DESIGN_TOKENS.sizing.symbol_header.height,
            bg_color: Color32::TRANSPARENT, // Let parent Frame show through
            text_color: Color32::from_gray(200),
            label_color: Color32::from_gray(140),
            bullish_color: DESIGN_TOKENS.semantic.extended.bullish, // Canonical bullish teal
            bearish_color: DESIGN_TOKENS.semantic.extended.bearish, // Canonical bearish red
            separator_color: Color32::from_gray(80),
            show_company_name: true,
            show_bid_ask: true,
            show_volume: true,
        }
    }
}

impl SymbolHeaderConfig {
    /// Create config from theme - always uses light UI chrome
    pub fn from_theme(_theme: &crate::theme::Theme) -> Self {
        // Light UI chrome always
        Self::default()
    }
}

/// Symbol Header Widget
pub struct SymbolHeader {
    pub symbol_info: SymbolInfo,
    pub ohlc: OhlcData,
    pub quote: QuoteData,
    pub config: SymbolHeaderConfig,
}

impl SymbolHeader {
    /// Create a new symbol header for the given symbol
    pub fn new(symbol_info: SymbolInfo) -> Self {
        Self {
            symbol_info,
            ohlc: OhlcData::default(),
            quote: QuoteData::default(),
            config: SymbolHeaderConfig::default(),
        }
    }

    /// Set OHLC price data for the current bar
    pub fn with_ohlc(mut self, ohlc: OhlcData) -> Self {
        self.ohlc = ohlc;
        self
    }

    /// Set bid/ask quote data
    pub fn with_quote(mut self, quote: QuoteData) -> Self {
        self.quote = quote;
        self
    }

    /// Set display configuration
    pub fn with_config(mut self, config: SymbolHeaderConfig) -> Self {
        self.config = config;
        self
    }

    /// Show the symbol header
    pub fn show(&self, ui: &mut Ui) -> Response {
        let (response, painter) = ui.allocate_painter(
            Vec2::new(ui.available_width(), self.config.height),
            Sense::hover(),
        );
        let rect = response.rect;

        // Background - REMOVED: Let parent Frame's fill show through
        // painter.rect_filled(rect, CornerRadius::ZERO, self.config.bg_color);

        let mut x = rect.min.x + DESIGN_TOKENS.spacing.sm;
        let center_y = rect.center().y;

        // Symbol name (bold, larger)
        let symbol_galley = painter.layout_no_wrap(
            self.symbol_info.symbol.clone(),
            egui::FontId::proportional(typography::LG),
            self.config.text_color,
        );
        painter.galley(
            Pos2::new(x, center_y - symbol_galley.size().y / 2.0),
            symbol_galley.clone(),
            Color32::TRANSPARENT,
        );
        x += symbol_galley.size().x + DESIGN_TOKENS.spacing.sm;

        // Company name • Exchange • Timeframe
        if self.config.show_company_name {
            let info_text = format!(
                "{} • {} • {}",
                self.symbol_info.company_name,
                self.symbol_info.exchange,
                self.symbol_info.timeframe
            );
            let info_galley = painter.layout_no_wrap(
                info_text,
                egui::FontId::proportional(typography::MD),
                self.config.label_color,
            );
            painter.galley(
                Pos2::new(x, center_y - info_galley.size().y / 2.0),
                info_galley.clone(),
                Color32::TRANSPARENT,
            );
            x += info_galley.size().x + DESIGN_TOKENS.spacing.md;
        }

        // Separator
        self.draw_separator(
            &painter,
            x,
            rect.min.y + DESIGN_TOKENS.spacing.xs,
            rect.max.y - DESIGN_TOKENS.spacing.xs,
        );
        x += DESIGN_TOKENS.spacing.md;

        // OHLC values
        let price_color = if self.ohlc.is_bullish() {
            self.config.bullish_color
        } else {
            self.config.bearish_color
        };

        // O: value
        x = self.draw_labeled_val(
            &painter,
            x,
            center_y,
            "O",
            self.ohlc.open,
            self.config.label_color,
        );
        x += DESIGN_TOKENS.spacing.sm;

        // H: value
        x = self.draw_labeled_val(
            &painter,
            x,
            center_y,
            "H",
            self.ohlc.high,
            self.config.label_color,
        );
        x += DESIGN_TOKENS.spacing.sm;

        // L: value
        x = self.draw_labeled_val(
            &painter,
            x,
            center_y,
            "L",
            self.ohlc.low,
            self.config.label_color,
        );
        x += DESIGN_TOKENS.spacing.sm;

        // C: value (colored)
        x = self.draw_labeled_val(&painter, x, center_y, "C", self.ohlc.close, price_color);
        x += DESIGN_TOKENS.spacing.sm;

        // Change (+/-value (percent%))
        let change_text = format!(
            "{}{:.2} ({}{:.2}%)",
            if self.ohlc.change >= 0.0 { "+" } else { "" },
            self.ohlc.change,
            if self.ohlc.change_percent >= 0.0 {
                "+"
            } else {
                ""
            },
            self.ohlc.change_percent
        );
        let change_galley = painter.layout_no_wrap(
            change_text,
            egui::FontId::proportional(typography::MD),
            price_color,
        );
        painter.galley(
            Pos2::new(x, center_y - change_galley.size().y / 2.0),
            change_galley.clone(),
            Color32::TRANSPARENT,
        );
        x += change_galley.size().x + DESIGN_TOKENS.spacing.md;

        // Separator
        self.draw_separator(
            &painter,
            x,
            rect.min.y + DESIGN_TOKENS.spacing.xs,
            rect.max.y - DESIGN_TOKENS.spacing.xs,
        );
        x += DESIGN_TOKENS.spacing.md;

        // Bid/Ask boxes
        if self.config.show_bid_ask && self.quote.bid > 0.0 {
            // SELL box (red background)
            x = self.draw_quote_box(
                &painter,
                x,
                center_y,
                self.quote.bid,
                "SELL",
                self.config.bearish_color,
            );
            x += DESIGN_TOKENS.spacing.xs;
            // BUY box (green background)
            x = self.draw_quote_box(
                &painter,
                x,
                center_y,
                self.quote.ask,
                "BUY",
                self.config.bullish_color,
            );
            x += DESIGN_TOKENS.spacing.md;

            // Separator
            self.draw_separator(
                &painter,
                x,
                rect.min.y + DESIGN_TOKENS.spacing.xs,
                rect.max.y - DESIGN_TOKENS.spacing.xs,
            );
            x += DESIGN_TOKENS.spacing.md;
        }

        // Volume
        if self.config.show_volume {
            let vol_text = format!("Vol {}", self.format_volume(self.ohlc.volume));
            let vol_galley = painter.layout_no_wrap(
                vol_text,
                egui::FontId::proportional(typography::MD),
                self.config.label_color,
            );
            painter.galley(
                Pos2::new(x, center_y - vol_galley.size().y / 2.0),
                vol_galley,
                Color32::TRANSPARENT,
            );
        }

        response
    }

    fn draw_separator(&self, painter: &egui::Painter, x: f32, y1: f32, y2: f32) {
        painter.line_segment(
            [Pos2::new(x, y1), Pos2::new(x, y2)],
            Stroke::new(DESIGN_TOKENS.stroke.hairline, self.config.separator_color),
        );
    }

    fn draw_labeled_val(
        &self,
        painter: &egui::Painter,
        x: f32,
        center_y: f32,
        label: &str,
        value: f64,
        val_color: Color32,
    ) -> f32 {
        // Label
        let label_galley = painter.layout_no_wrap(
            format!("{label}:"),
            egui::FontId::proportional(typography::MD),
            self.config.label_color,
        );
        painter.galley(
            Pos2::new(x, center_y - label_galley.size().y / 2.0),
            label_galley.clone(),
            Color32::TRANSPARENT,
        );
        let label_width = label_galley.size().x;

        // Value
        let val_galley = painter.layout_no_wrap(
            format!("{value:.2}"),
            egui::FontId::proportional(typography::MD),
            val_color,
        );
        painter.galley(
            Pos2::new(
                x + label_width + DESIGN_TOKENS.spacing.xs,
                center_y - val_galley.size().y / 2.0,
            ),
            val_galley.clone(),
            Color32::TRANSPARENT,
        );

        x + label_width + DESIGN_TOKENS.spacing.xs + val_galley.size().x
    }

    fn draw_quote_box(
        &self,
        painter: &egui::Painter,
        x: f32,
        center_y: f32,
        price: f64,
        label: &str,
        bg_color: Color32,
    ) -> f32 {
        let text = format!("{price:.2} {label}");
        let galley = painter.layout_no_wrap(
            text,
            egui::FontId::proportional(typography::SM),
            DESIGN_TOKENS.semantic.ui.text_light,
        );
        let padding = DESIGN_TOKENS.spacing.xs;
        let box_width = galley.size().x + padding * 2.0;
        let box_height = DESIGN_TOKENS.sizing.symbol_header.quote_box_height;

        let box_rect = Rect::from_min_size(
            Pos2::new(x, center_y - box_height / 2.0),
            Vec2::new(box_width, box_height),
        );

        painter.rect_filled(box_rect, DESIGN_TOKENS.rounding.sm, bg_color);
        painter.galley(
            Pos2::new(x + padding, center_y - galley.size().y / 2.0),
            galley.clone(),
            Color32::TRANSPARENT,
        );

        x + box_width
    }

    fn format_volume(&self, vol: f64) -> String {
        if vol >= 1_000_000_000.0 {
            format!("{:.2}B", vol / 1_000_000_000.0)
        } else if vol >= 1_000_000.0 {
            format!("{:.2}M", vol / 1_000_000.0)
        } else if vol >= 1_000.0 {
            format!("{:.2}K", vol / 1_000.0)
        } else {
            format!("{vol:.0}")
        }
    }
}
