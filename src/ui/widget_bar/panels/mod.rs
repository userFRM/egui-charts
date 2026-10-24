//! Right toolbar panels
//!
//! Individual panels for alerts, object tree, watchlists and time and sales.

pub mod alerts_panel;
pub mod object_tree;
pub mod time_and_sales;
pub mod watchlist;

pub use alerts_panel::*;
pub use object_tree::*;
pub use time_and_sales::*;
pub use watchlist::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Tick, TradeSide};
    use crate::theme::{Theme, apply_to_egui};

    #[test]
    fn market_panel_cells_stay_separate_in_narrow_windows() {
        for theme in [Theme::light(), Theme::dark()] {
            for width in [220.0, 280.0] {
                for tape in [false, true] {
                    let ctx = egui::Context::default();
                    apply_to_egui(&ctx, &theme);
                    let symbol = "EXCHANGE:LONG_SYMBOL_NAME";
                    let mut watchlist = WatchlistPanel::new(vec![Watchlist::from_text(
                        "A long watchlist name that must leave room for its menu",
                        symbol,
                    )]);
                    watchlist.set_quote(
                        symbol,
                        WatchlistQuote {
                            last: 123456.789,
                            prev_close: 100000.0,
                            price_decimals: Some(3),
                        },
                    );
                    let mut trades = TimeAndSalesPanel::default();
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
                            if tape {
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
                    let row: Vec<_> = texts
                        .iter()
                        .filter(|(text, _)| {
                            if tape {
                                matches!(text.as_str(), "12:34:56" | "123456789" | "123.46K")
                            } else {
                                [symbol, "123456.789", "+23456.789", "+23.46%"]
                                    .contains(&text.as_str())
                            }
                        })
                        .collect();
                    assert_eq!(row.len(), if tape { 3 } else { 4 }, "{texts:?}");
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
