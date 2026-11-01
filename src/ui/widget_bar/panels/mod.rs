//! Right toolbar panels
//!
//! Individual panels for alerts, object tree, watchlists, symbol details, time and sales, news,
//! the event calendars, the screener, hotlists, notifications, chats and the products launcher.

use crate::scales::{CurrencyFormatter, PriceFormatter};
use egui::text::TextFormat;
use egui::{Color32, FontId};

pub mod alerts_panel;
pub mod calendar;
pub mod chats;
mod controls;
pub mod hotlists;
pub mod market_details;
pub mod market_heatmap;
pub mod news;
pub mod notifications;
pub mod object_tree;
pub mod products;
pub mod screener;
pub mod symbol_details;
pub mod time_and_sales;
pub mod watchlist;

pub use alerts_panel::*;
pub use calendar::*;
pub use chats::*;
pub use hotlists::*;
pub use market_details::*;
pub use market_heatmap::*;
pub use news::*;
pub use notifications::*;
pub use object_tree::*;
pub use products::*;
pub use screener::*;
pub use symbol_details::*;
pub use time_and_sales::*;
pub use watchlist::*;

/// Height shared by market data rows.
const ROW_HEIGHT: f32 = 26.0;

/// Height of a panel header row, which fits its title and action buttons.
const HEADER_HEIGHT: f32 = 40.0;

/// Text in the bundled face at `weight`, such as 700 for bold.
fn weighted(size: f32, weight: f32, color: Color32) -> TextFormat {
    let mut format = TextFormat::simple(FontId::proportional(size), color);
    format.coords.push("wght", weight);
    format
}

/// `value` with an explicit sign, unsigned when it rounds to zero, or a dash when not finite.
fn signed(value: f64, decimals: usize) -> String {
    let text = grouped(value, decimals);
    if value > 0.0 && text.bytes().any(|b| matches!(b, b'1'..=b'9')) {
        format!("+{text}")
    } else {
        text
    }
}

/// A price with thousands separators and `decimals` decimals, or a dash when not finite.
fn grouped(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return "-".to_owned();
    }
    CurrencyFormatter::new("", decimals).format(value)
}

/// Format price with decimals chosen by its magnitude, so a negative price
/// keeps the decimals of the positive price of the same size
fn format_price(price: f64, decimals: Option<u8>) -> String {
    if !price.is_finite() {
        return "\u{2014}".to_owned();
    }
    let decimals = decimals.unwrap_or_else(|| price_decimals(price));
    format!("{:.*}", usize::from(decimals), price)
}

/// Decimals of a price label without instrument precision, chosen by the price's magnitude
fn price_decimals(price: f64) -> u8 {
    match price.abs() {
        m if m >= 10000.0 => 0,
        m if m >= 1000.0 => 1,
        m if m >= 10.0 => 2,
        m if m >= 1.0 => 4,
        m if m >= 0.001 => 6,
        _ => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{
        Datafeed, FeedError, HistoryPage, HistoryRequest, HotlistEntry, HotlistQuery, Reply,
        ScreenerColumn, ScreenerColumnSet, ScreenerQuery, ScreenerRow, SymbolDetails,
    };
    use crate::model::{Instrument, OrderSide, Quote, Tick};
    use crate::theme::{Theme, apply_to_egui};

    struct Feed;
    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Err(FeedError::NotSupported));
        }
        fn hotlist(&self, _: HotlistQuery, reply: Reply<Vec<HotlistEntry>>) {
            reply.send(Ok(vec![
                HotlistEntry::new("AAA", 123.45, 12.34),
                HotlistEntry::new("B", 10.0, 0.0),
            ]));
        }
        fn screen(&self, _: ScreenerQuery, reply: Reply<Vec<ScreenerRow>>) {
            reply.send(Ok(vec![
                ScreenerRow::new("AAA", vec!["123.45".into()]),
                ScreenerRow::new("B", vec!["10.00".into()]),
            ]));
        }
        fn symbol_details(&self, _: &str, reply: Reply<SymbolDetails>) {
            reply.send(Ok(SymbolDetails::new(
                vec![("Market capitalization".into(), "2.95T USD".into())],
                vec![("Sector".into(), "Technology".into())],
            )
            .with_category("Software")));
        }
    }

    #[test]
    fn market_panel_cells_stay_separate_in_narrow_windows() {
        for theme in [Theme::light(), Theme::dark()] {
            for width in [220.0, 280.0] {
                for panel in [
                    "watchlist",
                    "details",
                    "tape",
                    "tape_combined",
                    "hotlist",
                    "screener",
                    "news",
                ] {
                    let tape = matches!(panel, "tape" | "tape_combined");
                    let ctx = egui::Context::default();
                    apply_to_egui(&ctx, &theme);
                    let symbol = "EXCHANGE:LONG_SYMBOL_NAME";
                    let mut watchlist = WatchlistPanel::new(vec![Watchlist::from_text(
                        "A long watchlist name that must leave room for its menu",
                        &format!("{symbol},A"),
                    )]);
                    let time = Default::default();
                    watchlist.set_quote(symbol, Quote::new(123456.789, 100000.0, time));
                    watchlist.set_price_decimals(symbol, 3);
                    watchlist.set_quote("A", Quote::new(10.0, 10.0, time));
                    watchlist.set_price_decimals("A", 2);
                    watchlist.set_mark(
                        "A",
                        egui::load::SizedTexture::new(
                            egui::TextureId::User(1),
                            egui::Vec2::splat(18.0),
                        )
                        .into(),
                    );
                    let mut details = SymbolDetailsPanel::new();
                    details.set_instrument(
                        &Instrument::new(symbol)
                            .with_description("A long description supplied by the feed")
                            .with_price_decimals(3),
                    );
                    details.set_quote(Quote::new(123456.789, 100000.0, time));
                    details.set_feed(std::sync::Arc::new(Feed));
                    details.poll(&ctx);
                    let mut trades = TimeAndSalesPanel::default();
                    trades.combine_fills = panel == "tape_combined";
                    trades.config.price_decimals = Some(2);
                    trades.push(Tick::new(
                        "2026-10-23T12:34:55Z".parse().unwrap(),
                        2.0,
                        3.0,
                        None,
                    ));
                    trades.push(Tick::new(
                        "2026-10-23T12:34:56Z".parse().unwrap(),
                        123456789.12,
                        123456.0,
                        Some(OrderSide::Buy),
                    ));
                    if trades.combine_fills {
                        trades.push(Tick::new(
                            "2026-10-23T12:34:56Z".parse().unwrap(),
                            123456789.12,
                            0.0,
                            Some(OrderSide::Buy),
                        ));
                    }
                    trades.filters = vec![
                        TradeSizeFilter {
                            min_size: 1.0,
                            sound: false
                        };
                        20
                    ];
                    let mut hotlist = HotlistsPanel::new(vec!["Gainers".into()]);
                    hotlist.set_feed(std::sync::Arc::new(Feed));
                    hotlist.poll(&ctx);
                    let mut screener = ScreenerPanel::new(vec![ScreenerColumnSet::new(
                        "Overview",
                        vec![ScreenerColumn::new("last", "Last")],
                    )]);
                    screener.set_feed(std::sync::Arc::new(Feed));
                    screener.poll(&ctx);
                    let mut news = NewsPanel::new();
                    news.set_symbol(symbol);
                    let mut used = egui::Rect::NOTHING;
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 300.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            if panel == "details" {
                                details.show(ui);
                            } else if panel == "news" {
                                news.show(ui);
                            } else if panel == "hotlist" {
                                hotlist.show(ui);
                            } else if panel == "screener" {
                                screener.show(ui);
                            } else if tape {
                                trades.show(ui);
                            } else {
                                watchlist.show(ui);
                            }
                            used = ui.min_rect();
                        },
                    );
                    let texts: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| {
                            let egui::Shape::Text(text) = &shape.shape else {
                                return None;
                            };
                            let rect = text.visual_bounding_rect().intersect(shape.clip_rect);
                            (rect.is_positive()).then(|| (text.galley.text().to_owned(), rect))
                        })
                        .collect();
                    output.drop_without_applying_deltas();
                    assert!(
                        used.right() <= width,
                        "{panel} expanded beyond {width}: {used:?}"
                    );
                    if panel == "details" {
                        for label in ["Software", "Key statistics", "2.95T USD", "Profile"] {
                            assert!(texts.iter().any(|(text, _)| text == label), "{texts:?}");
                        }
                        continue;
                    }
                    if panel == "news" {
                        assert!(texts.iter().any(|(text, _)| text == "Market"), "{texts:?}");
                        continue;
                    }
                    if matches!(panel, "hotlist" | "screener") {
                        let bounds = |label: &str| {
                            texts
                                .iter()
                                .rev()
                                .find(|(text, _)| text == label)
                                .unwrap_or_else(|| panic!("{panel}: missing {label}: {texts:?}"))
                                .1
                        };
                        for label in ["123.45", "10.00"] {
                            assert!(
                                (bounds(label).right() - bounds("Last").right()).abs() < 2.0,
                                "{panel}: {texts:?}"
                            );
                        }
                        assert!(
                            (bounds("B").center().y - bounds("AAA").center().y - 26.0).abs() < 1.0,
                            "{panel}: rows must have watchlist height: {texts:?}"
                        );
                        assert!(!bounds("AAA").intersects(bounds("123.45")), "{texts:?}");
                        continue;
                    }
                    let row: Vec<_> = texts
                        .iter()
                        .filter(|(text, _)| {
                            if tape {
                                matches!(text.as_str(), "12:34:56" | "123456789.12" | "123.46K")
                                    || (panel == "tape_combined" && text == "2")
                            } else {
                                [symbol, "123,456.789", "+23,456.789", "+23.46%"]
                                    .contains(&text.as_str())
                            }
                        })
                        .collect();
                    assert_eq!(row.len(), if panel == "tape" { 3 } else { 4 }, "{texts:?}");
                    // The last match, so a symbol's name wins over its mark's initial.
                    let bounds = |label: &str| {
                        texts
                            .iter()
                            .rev()
                            .find(|(text, _)| text == label)
                            .unwrap()
                            .1
                    };
                    if !tape {
                        // The name follows the 18 point mark at the symbol column's edge.
                        assert!((bounds("A").left() - 34.0).abs() < 2.0, "{texts:?}");
                        assert!(
                            (bounds("A").center().y - bounds(symbol).center().y - 30.0).abs() < 1.0,
                            "{texts:?}"
                        );
                    }
                    let (value, heading) = if tape {
                        ("2.00", "Price")
                    } else {
                        ("10.00", "Last")
                    };
                    assert!(
                        (bounds(value).right() - bounds(heading).right()).abs() < 2.0,
                        "{texts:?}"
                    );
                    for (i, (_, rect)) in row.iter().enumerate() {
                        assert!(rect.left() >= 0.0 && rect.right() <= width, "{row:?}");
                        for (_, other) in &row[i + 1..] {
                            assert!(!rect.intersects(*other), "{row:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn panel_numbers_group_digits_and_omit_signs_at_rounded_zero() {
        for (value, decimals, price, change) in [
            (43237.45, 2, "43,237.45", "+43,237.45"),
            (-1234567.0, 0, "-1,234,567", "-1,234,567"),
            (863.83, 2, "863.83", "+863.83"),
            (100000.5, 1, "100,000.5", "+100,000.5"),
            (0.000123, 6, "0.000123", "+0.000123"),
            (-0.0, 2, "0.00", "0.00"),
            (-0.001, 2, "0.00", "0.00"),
            (0.001, 2, "0.00", "0.00"),
            (f64::INFINITY, 2, "-", "-"),
            (f64::NAN, 2, "-", "-"),
        ] {
            assert_eq!(grouped(value, decimals), price);
            assert_eq!(signed(value, decimals), change);
        }
    }

    #[test]
    fn test_format_price() {
        assert_eq!(format_price(0.0, None), "0.00000000");
        assert_eq!(format_price(f64::NAN, Some(2)), "\u{2014}");
        assert_eq!(format_price(50000.0, None), "50000");
        assert_eq!(format_price(1234.5, None), "1234.5");
        assert_eq!(format_price(99.99, None), "99.99");
        assert_eq!(format_price(1.2345, None), "1.2345");
        assert_eq!(format_price(0.001234, None), "0.001234");
        assert_eq!(format_price(-37.63, None), "-37.63");
        for (price, decimals, expected) in [
            (50000.25, 2, "50000.25"),
            (-37.625, 3, "-37.625"),
            (0.0, 2, "0.00"),
            (1.2345, 4, "1.2345"),
        ] {
            assert_eq!(format_price(price, Some(decimals)), expected);
        }
    }
}
