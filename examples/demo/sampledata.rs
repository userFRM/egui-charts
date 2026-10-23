//! Deterministic sample-data generation and the symbol catalog.
//!
//! Historical backfill gives every symbol a reproducible synthetic
//! price series seeded from its name. Switching symbols therefore yields a
//! stable, distinct chart rather than random noise that changes each frame.

use egui_charts::model::{Bar, Symbol, SymbolType};

/// Deterministic executions inside each fixture bar's UTC slot. These are synthetic aggressors,
/// independent of historical trades. Sizes sum to the fixture volume.
pub fn synthetic_trades(bars: &[Bar]) -> Vec<egui_charts::model::Tick> {
    use egui_charts::model::{Tick, TradeSide};
    bars.iter()
        .flat_map(|bar| {
            (0..64).map(move |i| {
                let fraction = ((i * 17) % 64) as f64 / 63.0;
                let price = bar.low + (bar.high - bar.low) * fraction;
                let side = if (i * 13 + bar.time.timestamp() as usize) % 11 < 6 {
                    TradeSide::Buy
                } else {
                    TradeSide::Sell
                };
                Tick::new(
                    bar.time + chrono::TimeDelta::microseconds(i as i64),
                    price,
                    bar.volume / 64.0,
                    Some(side),
                )
            })
        })
        .collect()
}

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

    /// The catalog as `Symbol` rows for the symbol-search dialog.
    pub fn symbols() -> Vec<Symbol> {
        Self::catalog()
            .iter()
            .map(|s| {
                let kind = match s.exchange {
                    "Crypto" => SymbolType::Crypto,
                    "Index" => SymbolType::Index,
                    "Forex" => SymbolType::Forex,
                    "Synthetic" => SymbolType::Fund,
                    _ => SymbolType::Stock,
                };
                Symbol::new(s.symbol, s.company_name).with_market(s.exchange, kind)
            })
            .collect()
    }
}

/// Map a top-toolbar chart-style label to a `ChartType`.
pub fn chart_type_from_name(name: &str) -> Option<egui_charts::model::ChartType> {
    use egui_charts::model::ChartType;
    match name.to_lowercase().as_str() {
        "bars" => Some(ChartType::Bars),
        "candles" | "candlestick" => Some(ChartType::Candles),
        "hollow candles" | "hollow" => Some(ChartType::HollowCandles),
        "line" => Some(ChartType::Line),
        "line with markers" => Some(ChartType::LineWithMarkers),
        "step line" => Some(ChartType::StepLine),
        "area" => Some(ChartType::Area),
        "hlc area" => Some(ChartType::HlcArea),
        "baseline" => Some(ChartType::Baseline),
        "columns" => Some(ChartType::Columns),
        "heikin ashi" | "heikin" => Some(ChartType::Heikin),
        "renko" => Some(ChartType::Renko),
        "kagi" => Some(ChartType::Kagi),
        "line break" => Some(ChartType::LineBreak),
        "point and figure" | "point & figure" => Some(ChartType::PointAndFigure),
        "range" => Some(ChartType::Range),
        "high-low" | "high low" => Some(ChartType::HighLow),
        "volume candles" => Some(ChartType::VolumeCandles),
        "time price opportunity" | "tpo" | "market profile" => {
            Some(ChartType::TimePriceOpportunity)
        }
        _ => None,
    }
}

/// Trading windows shared by the synthetic feed and its session controls.
pub fn sample_session() -> egui_charts::model::Session {
    use chrono::{
        NaiveTime,
        Weekday::{Fri, Mon, Thu, Tue, Wed},
    };
    use egui_charts::model::{Session, SessionWindow};
    let clock = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
    Session::new([SessionWindow::new(
        [Mon, Tue, Wed, Thu, Fri],
        clock(14, 30),
        clock(21, 0),
    )])
    .with_extended_windows([SessionWindow::new(
        [Mon, Tue, Wed, Thu, Fri],
        clock(8, 30),
        clock(14, 30),
    )])
}

/// Derive a displayed session and dividend basis from the unchanged source history.
///
/// SAMPLE supplies weekday extended hours from 08:30 to 21:00 UTC, with regular
/// hours from 14:30. Its synthetic dividend on January 26 reduces earlier OHLC
/// prices by 2% when adjusted; volumes and timestamps do not change.
pub fn session_bars(
    symbol: &str,
    source: &[Bar],
    session: egui_charts::ui::timeframe_toolbar::SessionType,
    adjusted: bool,
) -> Vec<Bar> {
    use chrono::{TimeZone, Utc};
    use egui_charts::ui::timeframe_toolbar::SessionType;

    let hours = egui_charts::model::ExchangeSessionProvider::new(
        egui_charts::model::TimezoneId::utc(),
        Some(sample_session()),
    );
    let dividend = Utc.with_ymd_and_hms(2026, 1, 26, 14, 30, 0).unwrap();
    source
        .iter()
        .filter(|bar| {
            symbol != "SAMPLE"
                || session == SessionType::Full24x7
                || hours.is_open(bar.time, session == SessionType::ETH)
        })
        .map(|bar| {
            let mut bar = bar.clone();
            if symbol == "SAMPLE" && adjusted && bar.time < dividend {
                bar.open *= 0.98;
                bar.high *= 0.98;
                bar.low *= 0.98;
                bar.close *= 0.98;
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
    use egui_charts::model::{BarClock, Timeframe};
    use egui_charts::ui::timeframe_toolbar::{DateRange, SessionType};

    #[test]
    fn synthetic_fixture_preserves_sessions_adjustments_and_ranges() {
        let times = [
            "2026-01-23T08:30:00Z",
            "2026-01-23T14:30:00Z",
            "2026-01-23T21:00:00Z",
            "2026-01-24T14:30:00Z",
            "2026-01-26T14:30:00Z",
        ];
        let source: Vec<_> = times
            .iter()
            .map(|time| Bar::new(time.parse().unwrap(), 100.0, 102.0, 99.0, 101.0, 640.0))
            .collect();
        for (session, indices) in [
            (SessionType::RTH, vec![1, 4]),
            (SessionType::ETH, vec![0, 1, 4]),
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
        for (range, count) in [
            (DateRange::Day1, 1),
            (DateRange::Day5, 5),
            (DateRange::Month1, 5),
            (DateRange::All, 5),
        ] {
            assert_eq!(visible_bars_for_range(range, &source), count);
        }
        assert_eq!(visible_bars_for_range(DateRange::All, &[]), 0);
        for timeframe in Timeframe::all() {
            let clock = BarClock::new(timeframe);
            let mut bar = source[0].clone();
            bar.time = clock.bar_open(bar.time);
            let trades = synthetic_trades(&[bar.clone()]);
            assert!(
                trades
                    .iter()
                    .all(|trade| clock.same_bar(trade.ts, bar.time))
            );
            assert_eq!(
                trades.iter().map(|trade| trade.size).sum::<f64>(),
                bar.volume
            );
            assert!(trades.windows(2).all(|pair| pair[0].ts <= pair[1].ts));
        }
    }
}
