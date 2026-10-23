//! Deterministic sample-data generation and the symbol catalog.
//!
//! The demo ships no live feed, so every symbol gets a reproducible synthetic
//! price series seeded from its name. Switching symbols therefore yields a
//! stable, distinct chart rather than random noise that changes each frame.

use egui_charts::model::{Bar, Symbol, Timeframe};

/// A tradable instrument the demo knows how to render, including the seed and
/// starting price used to synthesize its bars.
pub struct DemoSymbol {
    pub symbol: &'static str,
    pub company_name: &'static str,
    pub exchange: &'static str,
    pub start_price: f64,
    seed: usize,
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
            .map(|s| Symbol::new(s.symbol, s.company_name))
            .collect()
    }
}

/// Generate a deterministic OHLCV series for `symbol` at the given timeframe.
///
/// The same `(symbol, timeframe, count)` always produces identical bars, so a
/// symbol switch is repeatable and indicators/replay see stable input.
pub fn generate_sample_bars(symbol: &str, count: usize, timeframe: Timeframe) -> Vec<Bar> {
    use chrono::{Datelike, TimeZone, Timelike, Utc, Weekday};

    let spec = DemoSymbol::find(symbol);
    let mut bars = Vec::with_capacity(count);
    let mut price = spec.start_price;
    let session_fixture = spec.symbol == "SAMPLE";
    let mut time = if session_fixture {
        Utc.with_ymd_and_hms(
            2026,
            1,
            5,
            if (Timeframe::Min1.duration_ms()..Timeframe::Day1.duration_ms())
                .contains(&timeframe.duration_ms())
            {
                8
            } else {
                14
            },
            30,
            0,
        )
        .unwrap()
    } else {
        Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
    };
    let step_ms = timeframe.duration_ms();

    // Per-symbol salt so each instrument has its own price action.
    let salt = spec.seed.wrapping_mul(0x9E37_79B1);

    let mut trend = 0.0_f64;
    let mut volatility = 1.0_f64;

    for i in 0..count {
        let r1 = hash_f64(i, salt);
        let r2 = hash_f64(i, salt.wrapping_add(1000));
        let r3 = hash_f64(i, salt.wrapping_add(2000));
        let r4 = hash_f64(i, salt.wrapping_add(3000));
        let r5 = hash_f64(i, salt.wrapping_add(4000));

        // Slow-moving trend with mean reversion (momentum effect).
        trend = trend * 0.985 + (r5 - 0.5) * 0.003;

        // Return: trend + noise scaled by volatility.
        let return_pct = trend + (r1 - 0.5) * 0.025 * volatility;

        // Volatility clustering (big moves beget big moves).
        volatility = 0.93 * volatility + 0.07 * (1.0 + return_pct.abs() * 40.0);
        volatility = volatility.clamp(0.5, 3.0);

        let open = price;
        let close = if session_fixture {
            spec.start_price
                + (i as f64 / 19.0).sin() * 18.0
                + (i as f64 / 6.0).sin() * 9.0
                + (r1 - 0.5) * 3.0
        } else {
            open * (1.0 + return_pct)
        };

        let body_high = open.max(close);
        let body_low = open.min(close);
        let body_range = (body_high - body_low).max(open * 0.0005);

        let high = body_high + r2 * body_range * 2.0;
        let low = body_low - r3 * body_range * 2.0;

        let base_vol = if session_fixture {
            100_000.0 + r4 * 1_000_000.0
        } else {
            600.0 + r4 * 1400.0
        };
        let vol_spike = 1.0 + return_pct.abs() * 30.0;
        let volume = base_vol * vol_spike;

        bars.push(Bar {
            time,
            open,
            high,
            low,
            close,
            volume,
        });
        price = close;
        let date = time.date_naive();
        time += chrono::Duration::milliseconds(step_ms);
        if session_fixture {
            if step_ms < Timeframe::Day1.duration_ms()
                && (time.date_naive() != date || time.hour() >= 21)
            {
                time = (date + chrono::Duration::days(1))
                    .and_hms_opt(8, 30, 0)
                    .unwrap()
                    .and_utc();
            }
            while matches!(time.weekday(), Weekday::Sat | Weekday::Sun) {
                time += chrono::Duration::days(1);
            }
        }
    }

    bars
}

/// How many bars to synthesize for a given timeframe.
pub fn bar_count_for_timeframe(tf: Timeframe) -> usize {
    match tf {
        Timeframe::Min1 | Timeframe::Min5 => 1000,
        Timeframe::Min15 | Timeframe::Min30 => 800,
        Timeframe::Hour1 | Timeframe::Hour4 => 600,
        Timeframe::Day1 | Timeframe::Week1 => 500,
        Timeframe::Month1 => 120,
        _ => 500,
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
    use chrono::{NaiveTime, TimeZone, Utc};
    use egui_charts::ui::timeframe_toolbar::SessionType;

    let dividend = Utc.with_ymd_and_hms(2026, 1, 26, 14, 30, 0).unwrap();
    source
        .iter()
        .filter(|bar| {
            symbol != "SAMPLE"
                || session != SessionType::RTH
                || bar.time.time() >= NaiveTime::from_hms_opt(14, 30, 0).unwrap()
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

/// Deterministic pseudo-random f64 in `[0, 1)` from two seeds, without the `rand` crate.
fn hash_f64(index: usize, salt: usize) -> f64 {
    let mut x = (index.wrapping_mul(2654435761) ^ salt) as u32;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x = x.wrapping_mul(1597334677);
    (x % 100_000) as f64 / 100_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike, Weekday};

    #[test]
    fn synthetic_fixture_preserves_sessions_adjustments_and_ranges() {
        for timeframe in Timeframe::all() {
            let bars = generate_sample_bars("SAMPLE", 600, timeframe);
            let repeated = generate_sample_bars("SAMPLE", 600, timeframe);
            assert_eq!(bars.len(), 600);
            assert_eq!(bars, repeated);
            assert!(bars.iter().all(|b| (680.0..790.0).contains(&b.close)));
            assert!(
                bars.iter()
                    .all(|b| !matches!(b.time.weekday(), Weekday::Sat | Weekday::Sun))
            );
            assert!(bars.windows(2).all(|b| b[0].time < b[1].time));
            assert!(bars.iter().all(|b| (8..21).contains(&b.time.hour())));
            assert!(
                !session_bars(
                    "SAMPLE",
                    &bars,
                    egui_charts::ui::timeframe_toolbar::SessionType::RTH,
                    false,
                )
                .is_empty(),
                "{timeframe}"
            );
            if timeframe == Timeframe::Hour1 {
                assert!(bars.iter().all(|b| b.time.minute() == 30));
                assert!(
                    bars.windows(2)
                        .any(|b| b[1].time - b[0].time > chrono::Duration::hours(24))
                );
            }
        }
        assert_eq!(DemoSymbol::find("SAMPLE").exchange, "Synthetic");

        use egui_charts::ui::timeframe_toolbar::{DateRange, SessionType};
        let source = generate_sample_bars("SAMPLE", 600, Timeframe::Hour1);
        let regular = session_bars("SAMPLE", &source, SessionType::RTH, false);
        assert!(regular.len() < source.len());
        assert!(regular.iter().all(|b| b.time.hour() >= 14));
        for (session, count) in [
            (SessionType::RTH, regular.len()),
            (SessionType::ETH, source.len()),
        ] {
            let raw = session_bars("SAMPLE", &source, session, false);
            let adjusted = session_bars("SAMPLE", &source, session, true);
            assert_eq!(raw.len(), count);
            assert_eq!(adjusted.len(), count);
            assert_eq!(raw[0].close * 0.98, adjusted[0].close);
            assert_eq!(raw.last().unwrap().close, adjusted.last().unwrap().close);
            for (raw, adjusted) in raw.iter().zip(adjusted.iter()) {
                assert_eq!((raw.time, raw.volume), (adjusted.time, adjusted.volume));
                assert!(adjusted.low <= adjusted.open.min(adjusted.close));
                assert!(adjusted.high >= adjusted.open.max(adjusted.close));
            }
        }
        for (range, count) in [
            (DateRange::Day1, 7),
            (DateRange::Day5, 22),
            (DateRange::Month1, 141),
            (DateRange::Month3, regular.len()),
            (DateRange::Month6, regular.len()),
            (DateRange::YTD, regular.len()),
            (DateRange::Year1, regular.len()),
            (DateRange::Year5, regular.len()),
            (DateRange::All, regular.len()),
        ] {
            assert_eq!(visible_bars_for_range(range, &regular), count, "{range:?}");
        }
        assert_eq!(visible_bars_for_range(DateRange::All, &[]), 0);
        let daily = generate_sample_bars("SAMPLE", 600, Timeframe::Day1);
        assert_eq!(
            session_bars("SAMPLE", &daily, SessionType::RTH, false).len(),
            600
        );
    }
}
