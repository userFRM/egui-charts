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
    use chrono::{TimeZone, Utc};

    let spec = DemoSymbol::find(symbol);
    let mut bars = Vec::with_capacity(count);
    let mut price = spec.start_price;
    let base_time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let step_secs = timeframe.as_seconds().max(1);

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
        let close = open * (1.0 + return_pct);

        let body_high = open.max(close);
        let body_low = open.min(close);
        let body_range = (body_high - body_low).max(open * 0.0005);

        let high = body_high + r2 * body_range * 2.0;
        let low = body_low - r3 * body_range * 2.0;

        let base_vol = 600.0 + r4 * 1400.0;
        let vol_spike = 1.0 + return_pct.abs() * 30.0;
        let volume = base_vol * vol_spike;

        let time = base_time + chrono::Duration::seconds(step_secs * i as i64);

        bars.push(Bar {
            time,
            open,
            high,
            low,
            close,
            volume,
        });
        price = close;
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
        "volume footprint" => Some(ChartType::VolumeFootprint),
        "session volume" => Some(ChartType::SessionVolume),
        _ => None,
    }
}

/// Map a top-toolbar interval display string to a `Timeframe`.
pub fn timeframe_from_display(display: &str) -> Option<Timeframe> {
    Timeframe::from_resolution(display)
}

/// Map a bottom-toolbar `DateRange` preset to a visible-bar count, so the
/// preset actually re-zooms the chart's visible window.
pub fn visible_bars_for_range(
    range: egui_charts::ui::timeframe_toolbar::DateRange,
    total: usize,
) -> usize {
    use egui_charts::ui::timeframe_toolbar::DateRange;
    let want = match range {
        DateRange::Day1 => 24,
        DateRange::Day5 => 60,
        DateRange::Month1 => 120,
        DateRange::Month3 => 240,
        DateRange::Month6 => 320,
        DateRange::YTD => 400,
        DateRange::Year1 => 440,
        DateRange::Year5 => 480,
        DateRange::All => total,
    };
    want.clamp(10, total.max(10))
}

/// Deterministic pseudo-random f64 in `[0, 1)` with two seeds — no `rand` dep.
fn hash_f64(index: usize, salt: usize) -> f64 {
    let mut x = (index.wrapping_mul(2654435761) ^ salt) as u32;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x = x.wrapping_mul(1597334677);
    (x % 100_000) as f64 / 100_000.0
}
