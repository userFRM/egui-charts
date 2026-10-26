//! Symbol Header Widget
//!
//! Displays symbol info, the last close and its change, bid/ask prices, and volume
//! in a horizontal bar at the top of the chart area.

use crate::localization as l10n;
use crate::model::{Quote, percent_change};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::widget::QuoteSide;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// Symbol information for display
#[derive(Clone, Debug, Default)]
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
        let change_percent = percent_change(open, close);
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
        // `Color32::PLACEHOLDER` colours resolve from the active egui style while painting.
        Self {
            height: DESIGN_TOKENS.sizing.symbol_header.height,
            bg_color: Color32::TRANSPARENT, // Let parent Frame show through
            text_color: Color32::PLACEHOLDER,
            label_color: Color32::PLACEHOLDER,
            bullish_color: Color32::PLACEHOLDER,
            bearish_color: Color32::PLACEHOLDER,
            separator_color: Color32::PLACEHOLDER,
            show_company_name: true,
            show_bid_ask: true,
            show_volume: true,
        }
    }
}

impl SymbolHeaderConfig {
    /// Create config with colours fixed to `theme`.
    pub fn from_theme(theme: &crate::theme::Theme) -> Self {
        let ui = &theme.ui;
        Self {
            text_color: ui.text,
            label_color: ui.text_secondary,
            bullish_color: ui.success,
            bearish_color: ui.error,
            separator_color: ui.border,
            ..Self::default()
        }
    }

    /// This config with `Color32::PLACEHOLDER` colours taken from the style and theme of `ui`.
    fn resolved(&self, ui: &Ui) -> Self {
        let visuals = ui.visuals();
        // Text-grade status colours: the candle colours are too light for text on white.
        let colors = ui.active_theme().ui;
        let or = |c: Color32, style: Color32| if c == Color32::PLACEHOLDER { style } else { c };
        Self {
            bullish_color: or(self.bullish_color, colors.success),
            bearish_color: or(self.bearish_color, colors.error),
            text_color: or(self.text_color, visuals.strong_text_color()),
            label_color: or(
                self.label_color,
                visuals.widgets.noninteractive.fg_stroke.color,
            ),
            separator_color: or(
                self.separator_color,
                visuals.widgets.noninteractive.bg_stroke.color,
            ),
            ..self.clone()
        }
    }
}

/// Symbol Header Widget
pub struct SymbolHeader {
    pub symbol_info: SymbolInfo,
    pub ohlc: OhlcData,
    /// The symbol's quote, such as the one `WatchlistPanel::quote` holds. Its bid and ask show
    /// when both are known.
    pub quote: Option<Quote>,
    pub config: SymbolHeaderConfig,
}

impl SymbolHeader {
    /// Create a new symbol header for the given symbol
    pub fn new(symbol_info: SymbolInfo) -> Self {
        Self {
            symbol_info,
            ohlc: OhlcData::default(),
            quote: None,
            config: SymbolHeaderConfig::default(),
        }
    }

    /// Set OHLC price data for the current bar
    pub fn with_ohlc(mut self, ohlc: OhlcData) -> Self {
        self.ohlc = ohlc;
        self
    }

    /// Set the quote whose bid and ask are shown
    pub fn with_quote(mut self, quote: Quote) -> Self {
        self.quote = Some(quote);
        self
    }

    /// Set display configuration
    pub fn with_config(mut self, config: SymbolHeaderConfig) -> Self {
        self.config = config;
        self
    }

    /// Show the symbol header
    pub fn show(&self, ui: &mut Ui) -> Response {
        self.show_with_quote_action(ui).0
    }

    /// Show the symbol header and return the Sell or Buy box the user clicked, for
    /// [`Trading::trade`](crate::ui::trading::Trading::trade).
    pub fn show_with_quote_action(&self, ui: &mut Ui) -> (Response, Option<QuoteSide>) {
        let width = ui.available_width();
        egui::ScrollArea::horizontal()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let config = &self.config.resolved(ui);
                let response =
                    ui.allocate_response(Vec2::new(width, config.height), Sense::hover());
                let painter = ui.painter().clone();
                let rect = response.rect;

                let mut x = rect.min.x + ui.active_theme().spacing.sm;
                let center_y = rect.center().y;

                // Symbol name (bold, larger)
                let symbol_galley = painter.layout_no_wrap(
                    self.symbol_info.symbol.clone(),
                    egui::FontId::proportional(ui.active_theme().text.title),
                    config.text_color,
                );
                painter.galley(
                    Pos2::new(x, center_y - symbol_galley.size().y / 2.0),
                    symbol_galley.clone(),
                    Color32::TRANSPARENT,
                );
                x += symbol_galley.size().x + ui.active_theme().spacing.sm;

                // Company name • Exchange • Timeframe, skipping empty fields
                let info = &self.symbol_info;
                let info_text = [&info.company_name, &info.exchange, &info.timeframe]
                    .into_iter()
                    .filter(|field| !field.is_empty())
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(" • ");
                if config.show_company_name && !info_text.is_empty() {
                    let info_galley = painter.layout_no_wrap(
                        info_text,
                        egui::FontId::proportional(ui.active_theme().text.body),
                        config.label_color,
                    );
                    painter.galley(
                        Pos2::new(x, center_y - info_galley.size().y / 2.0),
                        info_galley.clone(),
                        Color32::TRANSPARENT,
                    );
                    x += info_galley.size().x + ui.active_theme().spacing.md;
                }

                // Separator
                self.draw_separator(
                    &painter,
                    config,
                    x,
                    rect.min.y + ui.active_theme().spacing.xs,
                    rect.max.y - ui.active_theme().spacing.xs,
                );
                x += ui.active_theme().spacing.md;

                // Last close and change; the chart legend shows the bar's OHLC.
                let price_color = if self.ohlc.is_bullish() {
                    config.bullish_color
                } else {
                    config.bearish_color
                };

                let change_text = format!(
                    "{:.2} {}{:.2} ({}{:.2}%)",
                    self.ohlc.close,
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
                    egui::FontId::proportional(ui.active_theme().text.body),
                    price_color,
                );
                painter.galley(
                    Pos2::new(x, center_y - change_galley.size().y / 2.0),
                    change_galley.clone(),
                    Color32::TRANSPARENT,
                );
                x += change_galley.size().x + ui.active_theme().spacing.md;

                // Separator
                self.draw_separator(
                    &painter,
                    config,
                    x,
                    rect.min.y + ui.active_theme().spacing.xs,
                    rect.max.y - ui.active_theme().spacing.xs,
                );
                x += ui.active_theme().spacing.md;

                // Bid/Ask boxes
                let mut quote_action = None;
                let bid_ask = (self.quote.as_ref())
                    .and_then(|quote| quote.bid.zip(quote.ask))
                    .filter(|(bid, ask)| bid.is_finite() && ask.is_finite());
                if config.show_bid_ask
                    && let Some((bid, ask)) = bid_ask
                {
                    // SELL box (red background)
                    let sell = self.draw_quote_box(
                        &painter,
                        x,
                        center_y,
                        bid,
                        "SELL",
                        config.bearish_color,
                    );
                    x = sell.right() + ui.active_theme().spacing.xs;
                    // BUY box (green background)
                    let buy = self.draw_quote_box(
                        &painter,
                        x,
                        center_y,
                        ask,
                        "BUY",
                        config.bullish_color,
                    );
                    x = buy.right() + ui.active_theme().spacing.md;
                    for (rect, side) in [(sell, QuoteSide::Sell), (buy, QuoteSide::Buy)] {
                        let id = response.id.with(side == QuoteSide::Buy);
                        let quote_response = ui.interact(rect, id, Sense::click());
                        let translated_label = l10n::text(
                            ui.ctx(),
                            if side == QuoteSide::Buy {
                                "Buy"
                            } else {
                                "Sell"
                            },
                        );
                        quote_response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                quote_response.enabled(),
                                &translated_label,
                            )
                        });
                        if quote_response.clicked() {
                            quote_action = Some(side);
                        }
                    }

                    // Separator
                    self.draw_separator(
                        &painter,
                        config,
                        x,
                        rect.min.y + ui.active_theme().spacing.xs,
                        rect.max.y - ui.active_theme().spacing.xs,
                    );
                    x += ui.active_theme().spacing.md;
                }

                // Volume
                if config.show_volume {
                    let vol_text = l10n::format_text(
                        ui.ctx(),
                        "Vol {volume}",
                        &[("volume", self.format_volume(self.ohlc.volume))],
                    );
                    let vol_galley = painter.layout_no_wrap(
                        vol_text,
                        egui::FontId::proportional(ui.active_theme().text.body),
                        config.label_color,
                    );
                    let width = vol_galley.size().x;
                    painter.galley(
                        Pos2::new(x, center_y - vol_galley.size().y / 2.0),
                        vol_galley,
                        Color32::TRANSPARENT,
                    );
                    x += width;
                }
                ui.set_min_width(x - rect.left() + ui.active_theme().spacing.sm);
                (response, quote_action)
            })
            .inner
    }

    fn draw_separator(
        &self,
        painter: &egui::Painter,
        config: &SymbolHeaderConfig,
        x: f32,
        y1: f32,
        y2: f32,
    ) {
        painter.line_segment(
            [Pos2::new(x, y1), Pos2::new(x, y2)],
            Stroke::new(DESIGN_TOKENS.stroke.hairline, config.separator_color),
        );
    }

    fn draw_quote_box(
        &self,
        painter: &egui::Painter,
        x: f32,
        center_y: f32,
        price: f64,
        label: &str,
        bg_color: Color32,
    ) -> Rect {
        let text = format!("{price:.2} {}", l10n::text(painter.ctx(), label));
        let galley = painter.layout_no_wrap(
            text,
            egui::FontId::proportional(painter.ctx().active_theme().text.small),
            if crate::theme::contrast(Color32::WHITE, bg_color)
                >= crate::theme::contrast(Color32::BLACK, bg_color)
            {
                Color32::WHITE
            } else {
                Color32::BLACK
            },
        );
        let padding = painter.ctx().active_theme().spacing.xs;
        let box_width = galley.size().x + padding * 2.0;
        let box_height = DESIGN_TOKENS.sizing.symbol_header.quote_box_height;

        let box_rect = Rect::from_min_size(
            Pos2::new(x, center_y - box_height / 2.0),
            Vec2::new(box_width, box_height),
        );

        painter.rect_filled(box_rect, painter.ctx().active_theme().radius.sm, bg_color);
        painter.galley(
            Pos2::new(x + padding, center_y - galley.size().y / 2.0),
            galley.clone(),
            Color32::TRANSPARENT,
        );

        box_rect
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

#[cfg(test)]
mod tests {
    use super::OhlcData;

    #[test]
    fn quote_buttons_remain_clickable_in_narrow_headers() {
        use super::*;
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for width in [800.0, 320.0] {
                for (translated, label, expected) in [
                    (false, "SELL", QuoteSide::Sell),
                    (false, "BUY", QuoteSide::Buy),
                    (true, "VERKAUFEN", QuoteSide::Sell),
                    (true, "KAUFEN", QuoteSide::Buy),
                ] {
                    let ctx = egui::Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    if translated {
                        l10n::set_translator(&ctx, |english| match english {
                            "SELL" => Some("VERKAUFEN".into()),
                            "BUY" => Some("KAUFEN".into()),
                            _ => None,
                        });
                    }
                    let mut header = SymbolHeader::new(SymbolInfo {
                        symbol: "SYM".into(),
                        company_name: "A long company name".into(),
                        ..Default::default()
                    });
                    let quote = Quote::new(100.5, 100.0, Default::default());
                    header.quote = Some(quote.with_bid_ask(100.0, 101.0));
                    let mut selected = None;
                    let mut target = None;
                    let mut pressed = false;
                    for pass in 0..30 {
                        let mut events = vec![];
                        if let Some(pos) = target {
                            events.push(egui::Event::PointerMoved(pos));
                            events.push(egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: !pressed,
                                modifiers: Default::default(),
                            });
                            pressed = !pressed;
                        } else if pass > 2 {
                            events.push(egui::Event::PointerMoved(egui::pos2(width / 2.0, 20.0)));
                            events.push(egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                source: egui::MouseWheelSource::Wheel,
                                phase: egui::TouchPhase::Move,
                                delta: egui::vec2(-60.0, 0.0),
                                modifiers: Default::default(),
                            });
                        }
                        let output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    Vec2::new(width, 200.0),
                                )),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                selected = header.show_with_quote_action(ui).1;
                            },
                        );
                        if pass > 14 && target.is_none() {
                            target = output.shapes.iter().find_map(|shape| match &shape.shape {
                                egui::Shape::Text(text)
                                    if text.galley.text().ends_with(&format!(" {label}")) =>
                                {
                                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                                    shape.clip_rect.contains_rect(rect).then_some(rect.center())
                                }
                                _ => None,
                            });
                        }
                        output.drop_without_applying_deltas();
                        if selected.is_some() {
                            break;
                        }
                    }
                    assert_eq!(selected, Some(expected), "{label} at {width}");
                }
            }
        }
    }

    #[test]
    fn ohlc_change_percent_follows_the_sign_of_the_change() {
        // (open, close, expected percent)
        for (open, close, expected) in [(100.0, 105.0, 5.0), (-10.0, -5.0, 50.0), (0.0, 5.0, 0.0)] {
            let data = OhlcData::new(open, close, open, close, 0.0);
            assert_eq!(data.change_percent, expected, "{open} -> {close}");
        }
    }
}
