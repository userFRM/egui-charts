//! Deterministic sample-data generation and the symbol catalog.
//!
//! Historical backfill gives every symbol a reproducible synthetic
//! price series seeded from its name. Switching symbols therefore yields a
//! stable, distinct chart rather than random noise that changes each frame.

use egui_charts::model::{Bar, Instrument, SymbolType};

/// A tradable instrument the demo knows how to render, including the seed and
/// starting price used to synthesize its bars.
pub struct DemoSymbol {
    pub symbol: &'static str,
    pub company_name: &'static str,
    pub exchange: &'static str,
    pub start_price: f64,
    pub seed: u64,
}

impl DemoSymbol {
    /// The instrument catalog backing both the symbol-search dialog and the
    /// deterministic series generator.
    pub fn catalog() -> &'static [DemoSymbol] {
        &[
            DemoSymbol {
                symbol: "BTCUSD",
                company_name: "Bitcoin / US Dollar",
                exchange: "Crypto",
                start_price: 42_500.0,
                seed: 1,
            },
            DemoSymbol {
                symbol: "ETHUSD",
                company_name: "Ethereum / US Dollar",
                exchange: "Crypto",
                start_price: 2_300.0,
                seed: 7,
            },
            DemoSymbol {
                symbol: "AAPL",
                company_name: "Apple Inc.",
                exchange: "NASDAQ",
                start_price: 189.0,
                seed: 13,
            },
            DemoSymbol {
                symbol: "MSFT",
                company_name: "Microsoft Corporation",
                exchange: "NASDAQ",
                start_price: 415.0,
                seed: 23,
            },
            DemoSymbol {
                symbol: "NVDA",
                company_name: "NVIDIA Corporation",
                exchange: "NASDAQ",
                start_price: 880.0,
                seed: 29,
            },
            DemoSymbol {
                symbol: "TSLA",
                company_name: "Tesla, Inc.",
                exchange: "NASDAQ",
                start_price: 175.0,
                seed: 37,
            },
            DemoSymbol {
                symbol: "SPX",
                company_name: "S&P 500 Index",
                exchange: "Index",
                start_price: 5_100.0,
                seed: 41,
            },
            DemoSymbol {
                symbol: "EURUSD",
                company_name: "Euro / US Dollar",
                exchange: "Forex",
                start_price: 1.08,
                seed: 53,
            },
            DemoSymbol {
                symbol: "ESZ2026",
                company_name: "S&P 500 futures, December 2026",
                exchange: "CME",
                start_price: 5_100.0,
                seed: 59,
            },
            DemoSymbol {
                symbol: "ESH2027",
                company_name: "S&P 500 futures, March 2027",
                exchange: "CME",
                start_price: 5_120.0,
                seed: 61,
            },
            // Neutral name for published screenshots: no real ticker with made-up prices.
            DemoSymbol {
                symbol: "SAMPLE",
                company_name: "Synthetic ETF sample",
                exchange: "Synthetic",
                start_price: 735.0,
                seed: 1,
            },
        ]
    }

    /// Look up an instrument by ticker, falling back to the first entry so the
    /// demo always has something to draw.
    pub fn find(symbol: &str) -> &'static DemoSymbol {
        let cat = Self::catalog();
        cat.iter()
            .find(|s| s.symbol.eq_ignore_ascii_case(symbol))
            .unwrap_or(&cat[0])
    }

    fn kind(&self) -> SymbolType {
        match self.exchange {
            "Crypto" => SymbolType::Crypto,
            "Index" => SymbolType::Index,
            "Forex" => SymbolType::Forex,
            "CME" => SymbolType::Futures,
            "Synthetic" => SymbolType::Fund,
            _ => SymbolType::Stock,
        }
    }

    /// Whether the symbol trades in US equity hours and pays a dividend: the stocks and the fund.
    pub fn has_hours(symbol: &str) -> bool {
        matches!(
            Self::find(symbol).kind(),
            SymbolType::Stock | SymbolType::Fund
        )
    }

    /// The catalog as `Instrument` rows for the symbol-search dialog.
    pub fn symbols() -> Vec<Instrument> {
        Self::catalog()
            .iter()
            .map(|s| {
                let kind = s.kind();
                let mut instrument = Instrument::new(s.symbol)
                    .with_description(s.company_name)
                    .with_exchange(s.exchange)
                    .with_symbol_type(kind);
                if kind == SymbolType::Futures {
                    instrument = instrument.with_root("ES");
                }
                instrument
            })
            .collect()
    }
}

/// US equity hours: weekdays, regular 09:30 to 16:00 and extended 04:00 to 09:30 and 16:00 to
/// 20:00, in New York time.
pub fn us_equity_session() -> egui_charts::model::Session {
    use chrono::{
        NaiveTime,
        Weekday::{Fri, Mon, Thu, Tue, Wed},
    };
    use egui_charts::model::{Session, SessionWindow};
    let window = |open: (u32, u32), close: (u32, u32)| {
        SessionWindow::new(
            [Mon, Tue, Wed, Thu, Fri],
            NaiveTime::from_hms_opt(open.0, open.1, 0).unwrap(),
            NaiveTime::from_hms_opt(close.0, close.1, 0).unwrap(),
        )
    };
    Session::new([window((9, 30), (16, 0))])
        .with_extended_windows([window((4, 0), (9, 30)), window((16, 0), (20, 0))])
}

/// The exchange zone of [`us_equity_session`], which follows daylight saving.
pub fn us_equity_zone() -> egui_charts::model::TimezoneId {
    egui_charts::model::TimezoneId::named("America/New_York").expect("America/New_York is known")
}

/// The hours of [`us_equity_session`] in [`us_equity_zone`].
pub fn us_equity_hours() -> egui_charts::model::ExchangeSessionProvider {
    egui_charts::model::ExchangeSessionProvider::new(us_equity_zone(), Some(us_equity_session()))
}

/// Derive a displayed session and dividend basis from the unchanged source history.
///
/// Symbols with [`DemoSymbol::has_hours`] show only the bars inside the selected session of
/// [`us_equity_hours`]. Their synthetic dividend, a Monday in winter at 09:30 New York time with a
/// rate of 2% to 4% taken from the symbol's seed, reduces earlier OHLC prices when adjusted;
/// volumes and timestamps do not change.
pub fn session_bars(
    symbol: &str,
    source: &[Bar],
    session: egui_charts::ui::timeframe_toolbar::SessionType,
    adjusted: bool,
) -> Vec<Bar> {
    use chrono::{Duration, TimeZone, Utc};
    use egui_charts::ui::timeframe_toolbar::SessionType;

    if !DemoSymbol::has_hours(symbol) {
        return source.to_vec();
    }
    let hours = us_equity_hours();
    let seed = DemoSymbol::find(symbol).seed;
    let dividend = Utc.with_ymd_and_hms(2026, 1, 26, 14, 30, 0).unwrap()
        + Duration::weeks(((seed - 1) % 4) as i64);
    let factor = 1.0 - 0.01 * (1 + seed % 3) as f64;
    source
        .iter()
        .filter(|bar| {
            session == SessionType::Full24x7 || hours.is_open(bar.time, session == SessionType::ETH)
        })
        .map(|bar| {
            let mut bar = bar.clone();
            if adjusted && bar.time < dividend {
                bar.open *= factor;
                bar.high *= factor;
                bar.low *= factor;
                bar.close *= factor;
            }
            bar
        })
        .collect()
}

/// Count the supplied bars in a lookback ending at the latest fixture timestamp.
/// Session gaps and the selected interval determine the count, including YTD.
pub fn visible_bars_for_range(
    range: egui_charts::ui::timeframe_toolbar::DateRange,
    bars: &[Bar],
) -> usize {
    use chrono::{Datelike, Duration, Months, TimeZone, Utc};
    use egui_charts::ui::timeframe_toolbar::DateRange;
    let Some(last) = bars.last() else {
        return 0;
    };
    let end = last.time;
    let start = match range {
        DateRange::Day1 => Some(end - Duration::days(1)),
        DateRange::Day5 => Some(end - Duration::days(5)),
        DateRange::Month1 => end.checked_sub_months(Months::new(1)),
        DateRange::Month3 => end.checked_sub_months(Months::new(3)),
        DateRange::Month6 => end.checked_sub_months(Months::new(6)),
        DateRange::YTD => Utc.with_ymd_and_hms(end.year(), 1, 1, 0, 0, 0).single(),
        DateRange::Year1 => end.checked_sub_months(Months::new(12)),
        DateRange::Year5 => end.checked_sub_months(Months::new(60)),
        DateRange::All => None,
    };
    bars.len() - start.map_or(0, |start| bars.partition_point(|bar| bar.time < start))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_charts::ui::timeframe_toolbar::{DateRange, SessionType};

    #[test]
    fn synthetic_fixture_preserves_sessions_adjustments_and_ranges() {
        let times = [
            "2026-01-23T09:00:00Z",
            "2026-01-23T14:30:00Z",
            "2026-01-23T21:30:00Z",
            "2026-01-24T14:30:00Z",
            "2026-01-26T14:30:00Z",
        ];
        let source: Vec<_> = times
            .iter()
            .map(|time| Bar::new(time.parse().unwrap(), 100.0, 102.0, 99.0, 101.0, 640.0))
            .collect();
        for (session, indices) in [
            (SessionType::RTH, vec![1, 4]),
            (SessionType::ETH, vec![0, 1, 2, 4]),
            (SessionType::Full24x7, vec![0, 1, 2, 3, 4]),
        ] {
            let raw = session_bars("SAMPLE", &source, session, false);
            let adjusted = session_bars("SAMPLE", &source, session, true);
            assert_eq!(
                raw,
                indices
                    .iter()
                    .map(|i| source[*i].clone())
                    .collect::<Vec<_>>()
            );
            assert_eq!(raw[0].close * 0.98, adjusted[0].close);
            assert_eq!(raw.last().unwrap().close, adjusted.last().unwrap().close);
            for (raw, adjusted) in raw.iter().zip(&adjusted) {
                assert_eq!((raw.time, raw.volume), (adjusted.time, adjusted.volume));
            }
        }
        // Stocks keep the same hours with a dividend of their own; continuous markets are untouched.
        let msft = session_bars("MSFT", &source, SessionType::RTH, true);
        assert_eq!(msft.len(), 2);
        assert!((msft[0].close - 101.0 * 0.97).abs() < 1e-9);
        assert_eq!(
            session_bars("BTCUSD", &source, SessionType::RTH, true),
            source
        );
        for (range, count) in [
            (DateRange::Day1, 1),
            (DateRange::Day5, 5),
            (DateRange::Month1, 5),
            (DateRange::All, 5),
        ] {
            assert_eq!(visible_bars_for_range(range, &source), count);
        }
        assert_eq!(visible_bars_for_range(DateRange::All, &[]), 0);
    }
}
