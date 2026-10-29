pub use crate::model::{Watchlist, WatchlistFlag, WatchlistItem};

#[cfg(test)]
mod tests {
    use super::super::tests::symbol;
    use super::*;

    #[test]
    fn text_keeps_sections_and_skips_blank_and_repeated_entries() {
        let list =
            Watchlist::from_text("Mine", " AAPL ,\n###Tech, MSFT\r\n,AAPL,,###Crypto,BTCUSD");
        assert_eq!(
            list.items,
            [
                symbol("AAPL"),
                WatchlistItem::Section("Tech".into()),
                symbol("MSFT"),
                WatchlistItem::Section("Crypto".into()),
                symbol("BTCUSD"),
            ]
        );
        assert_eq!(list.to_text(), "AAPL,###Tech,MSFT,###Crypto,BTCUSD");
        let mut list = list;
        for invalid in ["", " ", "AAPL", "A,B", "A\nB", "A\rB", "###Section"] {
            assert!(!list.add_symbol(invalid), "{invalid:?}");
        }
        assert!(list.add_symbol(" EURUSD "));
        assert_eq!(Watchlist::from_text("Mine", &list.to_text()), list);
        let mut titled = Watchlist::new("Titles");
        titled
            .items
            .push(WatchlistItem::Section("Rates, FX\nand more".into()));
        assert_eq!(
            Watchlist::from_text("Again", &titled.to_text()).items,
            [WatchlistItem::Section("Rates  FX and more".into())]
        );
    }
}
