//! Staple studies compared bar by bar with series computed independently from their published
//! definitions.
//!
//! `reference/expected.txt` holds one line per compared output line: the row name, the line index and one
//! token per fixture bar, `-` where the study has no value. The values were computed once in exact
//! rational arithmetic, with square roots to 40 digits, from the published definitions (Wilder 1978
//! for RSI, ATR, the directional indicators, the Commodity Selection Index and Parabolic SAR; Appel for MACD, PPO and
//! the Percentage Volume Oscillator; Bollinger; Lambert
//! 1980 for CCI; Quong and Soudack for MFI; Chande 1995 for Aroon; Hull; Chaikin; Lane; Williams;
//! Donchian; Hosoda; ordinary least squares; population standard deviation; Granville for OBV;
//! Chaikin for the Accumulation/Distribution line and volatility; Bill Williams; Larry Williams for the Ultimate
//! Oscillator; Elder for Force Index and Elder Ray; Botes and Siepman for the Vortex indicator; Dreiss for the
//! Choppiness Index; Hutson for TRIX; Dorsey for the Mass Index; Chande for the Chande Momentum Oscillator;
//! Coppock) on the same fixture, and rounded to 12 significant digits. Volume signs use exact integer comparisons; the
//! volatility rows use decimal logarithms at 50-digit precision. Calendar-window rows are
//! hand-calculated over an inclusive 364-day window.
//!
//! Where a definition leaves a convention open, the expected series follows the one the study
//! documents (EMA and MACD seed with the first close, PPO and the Percentage Volume Oscillator
//! seed their EMAs with the mean of the first values and their signal with the mean of the first
//! values of the oscillator, Force Index, Elder Ray, TRIX and the Mass Index seed their EMAs with the mean of
//! their first values, the Envelopes EMA and the Chaikin volatility EMA seed with their first input, an
//! Accumulation/Distribution bar with a high equal to its low adds nothing, and Parabolic SAR reverses when the price
//! crosses it, not on a touch), otherwise a published one (Wilder's first true range is high minus
//! low, Keltner's EMA seeds with the SMA of the first closes). Where no published rule settles it,
//! the series pins the study's current behaviour: Supertrend starts bullish, an Aroon tie counts
//! the most recent bar, and Parabolic SAR takes its first direction from the first two closes (up
//! on a tie), starts at the first bar's low or high and takes its first extreme point from the
//! first two bars. The Chaikin volatility starts once its average and its change both have their full
//! length; the windows of the Choppiness Index, Vortex indicator and Ultimate Oscillator start at the
//! second bar, the first with a previous close.

use crate::model::Bar;
use crate::studies::{
    ADX, ATR, AccumulationDistribution, Aroon, AwesomeOscillator, BalanceOfPower, BollingerBands,
    BollingerBandsWidth, CCI, ChaikinMoneyFlow, ChaikinVolatility, ChandeMomentumOscillator,
    ChoppinessIndex, CommoditySelectionIndex, CoppockCurve, DEMA, DirectionalMovement,
    DonchianChannels, EMA, ElderRay, Envelopes, ForceIndex, HL2, HLC3, HMA, IchimokuCloud,
    Indicator, IndicatorValue, KeltnerChannels, LinearRegression, LinearRegressionSlope, MACD,
    MassIndex, MedianPrice, MoneyFlowIndex, NetVolume, OHLC4, OnBalanceVolume, ParabolicSAR,
    ParkinsonVol, PercentB, PercentageVolumeOscillator, PriceOscillator, RSI, RateOfChange, SMA,
    StandardDeviation, Stochastic, SuperTrend, TEMA, TRIX, TrueRange, TypicalPrice,
    UltimateOscillator, VWMA, VortexIndicator, WMA, Week52HighLow, Week52HighLowLevels,
    WeightedClose, WilliamsR, ZeroTrendVol,
};
use crate::studies::{KnowSureThing, RankCorrelationIndex, RelativeVigorIndex, StochasticRSI};
use chrono::{TimeZone, Utc};

const EXPECTED: &str = include_str!("reference/expected.txt");

/// 150 bars of a seeded 64-bit LCG walk in whole ticks of 0.25, so every price is exact in binary:
/// rising to bar 74, falling after, a random opening offset on bars ending in 5 (so the true range
/// exceeds the bar range on some of them), flat on bars 110 to 114.
fn fixture() -> Vec<Bar> {
    let mut state: u64 = 0x5EED;
    let mut rnd = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as i64
    };
    let mut prev = 400_i64;
    (0..150)
        .map(|i| {
            let (open, high, low, close, volume) = if (110..115).contains(&i) {
                (prev, prev, prev, prev, 5_000)
            } else {
                let drift = if i < 75 { 1 } else { -1 };
                let open = prev + if i % 10 == 5 { rnd() % 17 - 8 } else { 0 };
                let close = open + rnd() % 9 - 4 + drift;
                let high = open.max(close) + rnd() % 5;
                let low = open.min(close) - rnd() % 5;
                (open, high, low, close, 1_000 + rnd() % 9_000)
            };
            prev = close;
            Bar {
                time: Utc.timestamp_opt(1_700_000_000 + i * 60, 0).unwrap(),
                open: open as f64 / 4.0,
                high: high as f64 / 4.0,
                low: low as f64 / 4.0,
                close: close as f64 / 4.0,
                volume: volume as f64,
            }
        })
        .collect()
}

/// Line `line` of a study value, `None` when the bar has no value or the line is NaN.
fn line_value(value: &IndicatorValue, line: usize) -> Option<f64> {
    match value {
        IndicatorValue::Single(v) if line == 0 => Some(*v),
        IndicatorValue::Multiple(vs) => vs.get(line).copied(),
        _ => None,
    }
    .filter(|v| !v.is_nan())
}

#[test]
fn staple_studies_match_published_definitions() {
    let bars = fixture();
    let studies: Vec<(&str, Box<dyn Indicator>)> = vec![
        ("ZeroTrend(1)", Box::new(ZeroTrendVol::new(1))),
        ("ZeroTrend(20)", Box::new(ZeroTrendVol::default())),
        ("Parkinson(20)", Box::new(ParkinsonVol::default())),
        ("NetVolume", Box::new(NetVolume::default())),
        ("SMA(20)", Box::new(SMA::default())),
        ("EMA(12)", Box::new(EMA::default())),
        ("WMA(20)", Box::new(WMA::default())),
        ("RSI(14)", Box::new(RSI::default())),
        ("StochRSI(14,14,3,3)", Box::new(StochasticRSI::default())),
        ("MACD(12,26,9)", Box::new(MACD::default())),
        ("BB(20,2)", Box::new(BollingerBands::default())),
        ("ATR(14)", Box::new(ATR::default())),
        ("ADX(14)", Box::new(ADX::default())),
        ("DMI(14)", Box::new(DirectionalMovement::default())),
        (
            "CSI(14,50,3000,25)",
            Box::new(
                CommoditySelectionIndex::new(14)
                    .with_point_value(50.0)
                    .with_margin(3000.0)
                    .with_commission(25.0),
            ),
        ),
        ("CCI(20)", Box::new(CCI::default())),
        ("MFI(14)", Box::new(MoneyFlowIndex::default())),
        ("Keltner(20,10,2)", Box::new(KeltnerChannels::default())),
        ("LinReg(14)", Box::new(LinearRegression::default())),
        (
            "LinRegSlope(14)",
            Box::new(LinearRegressionSlope::default()),
        ),
        ("Aroon(14)", Box::new(Aroon::default())),
        ("HMA(20)", Box::new(HMA::default())),
        ("HMA(21)", Box::new(HMA::new(21))),
        ("VWMA(20)", Box::new(VWMA::default())),
        ("CMF(20)", Box::new(ChaikinMoneyFlow::default())),
        ("SuperTrend(10,3)", Box::new(SuperTrend::default())),
        ("SuperTrend(10,1)", Box::new(SuperTrend::new(10, 1.0))),
        ("PSAR(0.02,0.02,0.2)", Box::new(ParabolicSAR::default())),
        (
            "PSAR(0.01,0.03,0.12)",
            Box::new(ParabolicSAR::new(0.01, 0.03, 0.12)),
        ),
        ("ROC(14)", Box::new(RateOfChange::default())),
        ("StdDev(20)", Box::new(StandardDeviation::default())),
        ("Donchian(20)", Box::new(DonchianChannels::default())),
        ("Stoch(14,3,3)", Box::new(Stochastic::default())),
        ("Stoch(14,1,3)", Box::new(Stochastic::new(14, 1, 3))),
        ("WilliamsR(14)", Box::new(WilliamsR::default())),
        ("BoP(14)", Box::new(BalanceOfPower::default())),
        ("PPO(12,26,9)", Box::new(PriceOscillator::default())),
        (
            "PVO(12,26,9)",
            Box::new(PercentageVolumeOscillator::default()),
        ),
        ("OBV", Box::new(OnBalanceVolume::default())),
        ("A/D", Box::new(AccumulationDistribution::default())),
        ("AO(5,34)", Box::new(AwesomeOscillator::default())),
        ("UO(7,14,28)", Box::new(UltimateOscillator::default())),
        ("TP", Box::new(TypicalPrice::default())),
        ("WC", Box::new(WeightedClose::default())),
        ("MP", Box::new(MedianPrice::default())),
        ("OHLC4", Box::new(OHLC4::default())),
        ("HLC3", Box::new(HLC3::default())),
        ("HL2", Box::new(HL2::default())),
        ("TR", Box::new(TrueRange::default())),
        ("Force(13)", Box::new(ForceIndex::default())),
        ("ElderRay(13)", Box::new(ElderRay::default())),
        ("Vortex(14)", Box::new(VortexIndicator::default())),
        ("Chop(14)", Box::new(ChoppinessIndex::default())),
        ("TRIX(15,9)", Box::new(TRIX::default())),
        ("Mass(9,25)", Box::new(MassIndex::default())),
        ("%B(20,2)", Box::new(PercentB::default())),
        ("BBW(20,2)", Box::new(BollingerBandsWidth::default())),
        ("Env(20,2.5,SMA)", Box::new(Envelopes::default())),
        ("Env(20,2.5,EMA)", Box::new(Envelopes::new(20, 2.5, "EMA"))),
        ("CV(10,10)", Box::new(ChaikinVolatility::default())),
        ("CMO(14)", Box::new(ChandeMomentumOscillator::default())),
        ("Coppock(14,11,10)", Box::new(CoppockCurve::default())),
        ("Ichimoku(9,26,52)", Box::new(IchimokuCloud::default())),
        (
            "Ichimoku(9,26,52,22)",
            Box::new(IchimokuCloud::new(9, 26, 52).with_displacement(22)),
        ),
    ];
    let mut studies: Vec<_> = studies
        .into_iter()
        .map(|(name, study)| (name, study, bars.clone()))
        .collect();
    // The boundary is inclusive; extrema expire one second later, even across gaps.
    let calendar: Vec<_> = [
        (0, 100.0, 0.0, 50.0),
        (86_400, 80.0, 10.0, 40.0),
        (363 * 86_400, 60.0, 20.0, 40.0),
        (364 * 86_400 - 1, 55.0, 25.0, 40.0),
        (364 * 86_400, 50.0, 30.0, 40.0),
        (364 * 86_400 + 1, 45.0, 35.0, 40.0),
        (365 * 86_400 + 1, 40.0, 32.0, 36.0),
        (800 * 86_400, 30.0, 26.0, 28.0),
    ]
    .into_iter()
    .map(|(t, high, low, close)| {
        Bar::new(
            Utc.timestamp_opt(t, 0).unwrap(),
            close,
            high,
            low,
            close,
            1.0,
        )
    })
    .collect();
    let minutes: Vec<_> = (0..300)
        .map(|i| {
            Bar::new(
                Utc.timestamp_opt(i * 60, 0).unwrap(),
                2.0,
                10.0,
                0.0,
                2.0,
                1.0,
            )
        })
        .collect();
    let weekly: Vec<_> = [0, 7, 357, 364, 371, 735]
        .into_iter()
        .enumerate()
        .map(|(i, day)| {
            let high = 100.0 - i as f64 * 10.0;
            Bar::new(
                Utc.timestamp_opt(day * 86_400, 0).unwrap(),
                20.0,
                high,
                0.0,
                20.0,
                1.0,
            )
        })
        .collect();
    let flat: Vec<_> = calendar
        .iter()
        .map(|b| Bar::new(b.time, 10.0, 10.0, 10.0, 10.0, 1.0))
        .collect();
    // With period 2, each EMA seeds with two existing inputs, alpha = 2/3.
    let short: Vec<_> = [2.0, 4.0, 8.0, 6.0, 10.0, 5.0]
        .into_iter()
        .enumerate()
        .map(|(i, c)| Bar::new(bars[i].time, c, c, c, c, 1.0))
        .collect();
    let constant = vec![Bar::new(bars[0].time, 100.0, 100.0, 100.0, 100.0, 1.0); 60];
    // RSI(2) starts at bar 2; stochastic(2) at 3, K(2) at 4, D(2) at 5.
    let rsi_short: Vec<_> = [1.0, 2.0, 1.0, 2.0, 1.0, 3.0, 2.0, 4.0]
        .into_iter()
        .enumerate()
        .map(|(i, c)| Bar::new(bars[i].time, c, c, c, c, 1.0))
        .collect();
    let doubling: Vec<_> = [1.0, 2.0, 4.0, 2.0, 4.0, 8.0]
        .into_iter()
        .enumerate()
        .map(|(i, c)| Bar::new(bars[i].time, c, c, c, c, 1.0))
        .collect();
    let vigor: Vec<_> = [1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0]
        .into_iter()
        .enumerate()
        .map(|(i, c)| Bar::new(bars[i].time, 0.0, 10.0, 0.0, c, 1.0))
        .collect();
    let ties: Vec<_> = [1.0, 1.0, 2.0, 2.0]
        .into_iter()
        .enumerate()
        .map(|(i, c)| Bar::new(bars[i].time, c, c, c, c, 1.0))
        .collect();
    use crate::studies::{AverageLine, AverageType, MACombination};
    let combination_bars: Vec<_> = [2.0, 4.0, 8.0, 6.0, 10.0, 5.0]
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            Bar::new(
                bars[i].time,
                c - 1.0,
                c + 1.0,
                c - 1.0,
                c,
                if i % 2 == 0 { 1.0 } else { 2.0 },
            )
        })
        .collect();
    let combo = |types: &[(AverageType, usize, crate::model::PriceSource)]| -> Box<dyn Indicator> {
        Box::new(
            MACombination::new(
                types
                    .iter()
                    .map(|&(kind, period, source)| {
                        AverageLine::new(kind, period, egui::Color32::BLUE).with_source(source)
                    })
                    .collect(),
            )
            .unwrap(),
        )
    };
    studies.extend([
        (
            "MA-combination-a",
            combo(&[
                (AverageType::Simple, 2, crate::model::PriceSource::Close),
                (
                    AverageType::Exponential,
                    2,
                    crate::model::PriceSource::Close,
                ),
                (AverageType::Weighted, 2, crate::model::PriceSource::Close),
            ]),
            combination_bars.clone(),
        ),
        (
            "MA-combination-b",
            combo(&[
                (AverageType::Smoothed, 2, crate::model::PriceSource::Close),
                (
                    AverageType::VolumeWeighted,
                    2,
                    crate::model::PriceSource::Close,
                ),
                (AverageType::Hull, 4, crate::model::PriceSource::Open),
            ]),
            combination_bars,
        ),
        (
            "LinRegChannel-hand",
            Box::new(crate::studies::LinearRegressionChannel::new(3, 2.0)) as Box<dyn Indicator>,
            {
                let mut series = ties.clone();
                for (bar, c) in series.iter_mut().zip([1.0, 4.0, 2.0, 4.0]) {
                    bar.close = c;
                }
                series
            },
        ),
        (
            "KST-hand",
            Box::new(KnowSureThing::new().with_params([1; 4], [2; 4], 2)) as Box<dyn Indicator>,
            doubling,
        ),
        ("RVI-hand", Box::new(RelativeVigorIndex::new(2)), vigor),
        (
            "Rank-ties-hand",
            Box::new(RankCorrelationIndex::new(4)),
            ties,
        ),
        (
            "StochRSI-hand",
            Box::new(StochasticRSI::new(2, 2, 2, 2)) as Box<dyn Indicator>,
            rsi_short[..6].to_vec(),
        ),
        (
            "StochRSI@31",
            Box::new(StochasticRSI::default()),
            bars[..31].to_vec(),
        ),
        (
            "StochRSI@32",
            Box::new(StochasticRSI::default()),
            bars[..32].to_vec(),
        ),
        (
            "StochRSI@33",
            Box::new(StochasticRSI::default()),
            bars[..33].to_vec(),
        ),
        (
            "DEMA(2)-hand",
            Box::new(DEMA::new(2)) as Box<dyn Indicator>,
            short.clone(),
        ),
        ("TEMA(2)-hand", Box::new(TEMA::new(2)), short),
        (
            "DEMA(20)-constant",
            Box::new(DEMA::new(20)),
            constant.clone(),
        ),
        ("TEMA(20)-constant", Box::new(TEMA::new(20)), constant),
        (
            "52W-levels-calendar",
            Box::new(Week52HighLowLevels::default()) as Box<dyn Indicator>,
            calendar.clone(),
        ),
        (
            "52W-levels-weekly",
            Box::new(Week52HighLowLevels::default()),
            weekly.clone(),
        ),
        ("52W%-flat", Box::new(Week52HighLow::default()), flat),
        (
            "52W%-calendar",
            Box::new(Week52HighLow::default()),
            calendar,
        ),
        ("52W%-minutes", Box::new(Week52HighLow::default()), minutes),
        ("52W%-weekly", Box::new(Week52HighLow::default()), weekly),
    ]);
    for (_, study, data) in &mut studies {
        study.calculate(data);
    }

    let mut failures = Vec::new();
    let mut checked_lines = vec![0; studies.len()];
    for row in EXPECTED.lines() {
        let mut tokens = row.split_whitespace();
        let name = tokens.next().unwrap();
        let line: usize = tokens.next().unwrap().parse().unwrap();
        let idx = studies.iter().position(|(n, _, _)| *n == name);
        let idx = idx.unwrap_or_else(|| panic!("{name} has expected values but is not built"));
        checked_lines[idx] += 1;
        let values = studies[idx].1.values();
        assert_eq!(
            values.len(),
            studies[idx].2.len(),
            "{name}: one value per bar"
        );
        let expected: Vec<&str> = tokens.collect();
        assert_eq!(
            expected.len(),
            values.len(),
            "{name} line {line}: one token per bar"
        );
        let mismatches: Vec<String> = expected
            .iter()
            .zip(values)
            .enumerate()
            .filter_map(|(bar, (token, value))| {
                let actual = line_value(value, line);
                let want = (*token != "-").then(|| token.parse::<f64>().unwrap());
                let ok = match (want, actual) {
                    (None, None) => true,
                    (Some(w), Some(a)) => (a - w).abs() <= 1e-9 * w.abs() + 1e-12,
                    _ => false,
                };
                (!ok).then(|| format!("bar {bar}: expected {want:?}, got {actual:?}"))
            })
            .collect();
        if let Some(first) = mismatches.first() {
            failures.push(format!(
                "{name} line {line}: {} bars differ, first {first}",
                mismatches.len()
            ));
        }
    }
    for ((name, _, _), lines) in studies.iter().zip(&checked_lines) {
        assert!(*lines > 0, "{name} is built but has no expected values");
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A refit drops the expired bar and replaces all three lines on an append or correction.
#[test]
fn regression_channel_refits_the_last_n_bars_on_update() {
    use crate::studies::LinearRegressionChannel;
    let mut bars: Vec<_> = [1.0, 2.0, 4.0, 2.0]
        .into_iter()
        .enumerate()
        .map(|(i, c)| Bar::new(Utc.timestamp_opt(i as i64, 0).unwrap(), c, c, c, c, 1.0))
        .collect();
    let mut study = LinearRegressionChannel::new(3, 2.0);
    assert!(
        study.is_incremental(),
        "the trailing channel must refit on every live update"
    );
    study.calculate(&bars[..2]);
    assert!(
        study
            .values()
            .iter()
            .all(|v| matches!(v, IndicatorValue::None))
    );
    // OLS of [1,2,4]: intercept 5/6, slope 3/2, SSE 1/6, offset 2*sqrt(SSE/2).
    // OLS of [2,4,2]: intercept 8/3, slope 0, SSE 8/3.
    for (len, edit, intercept, slope, offset) in [
        (3, None, 5.0 / 6.0, 1.5, 1.0 / 3.0_f64.sqrt()),
        (4, None, 8.0 / 3.0, 0.0, 4.0 / 3.0_f64.sqrt()),
        (4, Some(6.0), 2.0, 2.0, 0.0),
    ] {
        if let Some(close) = edit {
            bars[3].close = close;
        }
        study.update(&bars[..len], len - 1);
        assert_eq!(study.values().len(), len);
        for (i, value) in study.values().iter().enumerate() {
            if i < len - 3 {
                assert!(matches!(value, IndicatorValue::None));
                continue;
            }
            let IndicatorValue::Multiple(lines) = value else {
                panic!("{value:?}")
            };
            let middle = intercept + slope * (i - (len - 3)) as f64;
            for (got, want) in lines.iter().zip([middle + offset, middle, middle - offset]) {
                assert!((got - want).abs() < 1e-12, "{value:?}: {want}");
            }
        }
    }
    assert_eq!(study.correlation(), Some(1.0));
    assert_eq!(study.r_squared(), Some(1.0));
    let mut open =
        LinearRegressionChannel::new(3, 2.0).with_source(crate::model::PriceSource::Open);
    open.calculate(&bars);
    // Opens remain [2,4,2] in the trailing window after the close was corrected to 6.
    let IndicatorValue::Multiple(lines) = &open.values()[3] else {
        panic!("missing open fit")
    };
    assert!((lines[1] - 8.0 / 3.0).abs() < 1e-12);
    assert_eq!(open.correlation(), Some(0.0));
}

/// The configurable cross, triple and ribbon presets are reachable by their catalogue names.
#[test]
fn moving_average_combinations_mark_only_actual_crossings() {
    let factory = super::IndicatorFactory::new();
    for name in ["SMA/EMA Cross", "Triple MA", "MA Ribbon"] {
        assert!(factory.create(name).is_some(), "missing {name}");
    }
    use crate::studies::{AverageLine, AverageType, CrossDirection, MACombination};
    let mut bars: Vec<_> = [2.0, 1.0, 1.0, 0.0, 2.0, 2.0, 3.0, 1.0, 1.0, 0.0, 2.0]
        .into_iter()
        .enumerate()
        .map(|(i, c)| Bar::new(Utc.timestamp_opt(i as i64, 0).unwrap(), c, c, c, c, 1.0))
        .collect();
    let lines = vec![
        AverageLine::new(AverageType::Simple, 1, egui::Color32::BLUE),
        AverageLine::new(AverageType::Simple, 2, egui::Color32::RED),
    ];
    let mut study = MACombination::new(lines.clone()).unwrap();
    study.calculate(&bars);
    let crossings = |s: &MACombination| {
        s.markers()
            .iter()
            .map(|m| (m.bar, m.direction))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        crossings(&study),
        vec![
            (4, CrossDirection::Up),
            (7, CrossDirection::Down),
            (10, CrossDirection::Up)
        ]
    );
    // Revising the last bar removes the now-unconfirmed crossing.
    bars[10].close = 0.0;
    study.update(&bars, 10);
    assert_eq!(
        crossings(&study),
        vec![(4, CrossDirection::Up), (7, CrossDirection::Down)]
    );
    bars[6].close = f64::NAN;
    study.update(&bars, 6);
    assert_eq!(crossings(&study), vec![(4, CrossDirection::Up)]);
    for count in [0, 1, 15] {
        assert!(MACombination::new(vec![lines[0].clone(); count]).is_err());
    }
    assert!(
        MACombination::new(lines)
            .unwrap()
            .with_crossing_pair(0, 2)
            .is_err()
    );
}
