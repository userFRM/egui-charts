//! Right toolbar panels
//!
//! Individual panels for alerts, object tree, watchlists, symbol details, time and sales, news,
//! the event calendars, the screener, hotlists, notifications, chats and the products launcher.

pub mod alerts_panel;
pub mod calendar;
pub mod chats;
mod controls;
pub mod hotlists;
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
pub use news::*;
pub use notifications::*;
pub use object_tree::*;
pub use products::*;
pub use screener::*;
pub use symbol_details::*;
pub use time_and_sales::*;
pub use watchlist::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{
        Datafeed, FeedError, HistoryPage, HistoryRequest, HotlistEntry, HotlistQuery, Reply,
        ScreenerColumn, ScreenerColumnSet, ScreenerQuery, ScreenerRow,
    };
    use crate::model::{Quote, Tick, TradeSide};
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
    }

    #[test]
    fn market_panel_cells_stay_separate_in_narrow_windows() {
        for theme in [Theme::light(), Theme::dark()] {
            for width in [220.0, 280.0] {
                for panel in ["watchlist", "tape", "hotlist", "screener", "news"] {
                    let tape = panel == "tape";
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
                    let mut trades = TimeAndSalesPanel::default();
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
                        Some(TradeSide::Buy),
                    ));
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
                            if panel == "news" {
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
                        "panel expanded beyond {width}: {used:?}"
                    );
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
                            } else {
                                [symbol, "123456.789", "+23456.789", "+23.46%"]
                                    .contains(&text.as_str())
                            }
                        })
                        .collect();
                    assert_eq!(row.len(), if tape { 3 } else { 4 }, "{texts:?}");
                    let bounds =
                        |label: &str| texts.iter().find(|(text, _)| text == label).unwrap().1;
                    if !tape {
                        assert!((bounds("A").left() - 10.0).abs() < 2.0, "{texts:?}");
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
}
