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
//! The rows of later studies come from these definitions, in the same arithmetic:
//!
//! - Bill Williams for the Accelerator Oscillator and the Alligator; Chande for the Aroon Oscillator;
//!   Bollinger for BBTrend; Chaikin for the Chaikin Oscillator; Elder for Bull Bear Power; Martin 1989
//!   for the Ulcer Index; the simple average of the bar range for the Average Daily Range; up and down
//!   bars against their open for the Advance/Decline Ratio of bars; the detrended price oscillator of
//!   the conventional definition, aligned and centred.
//! - Swenlin for the Price Momentum Oscillator; Pring for the Special K, whose 725-bar warm-up needs a
//!   1000-bar series of its own; Spearman for the RCI Ribbon; Dorsey for the Relative Volatility Index;
//!   Blau for the SMI Ergodic lines; Lambert's CCI of the close for Woodies CCI.
//! - Chande and Kroll 1994 for the Chande Kroll Stop; Wilder's volatility system for the Volatility Stop;
//!   the median of the bar midpoints with ATR bands for the Median; the angle of the 34-bar EMA for the
//!   Chop Zone.
//! - The conventional floor-trader pivot points with Tom DeMark's and Nick Stott's Camarilla levels,
//!   from the previous period of UTC dates; the time-weighted mean of ohlc4 and Berkowitz, Logue and
//!   Noser's volume-weighted mean of hlc3 since an anchor bar.
//! - Time-study rows run on hand-built series in calendar time: 24-hour volume over the bars opening
//!   in the last 24 hours; relative volume at time as the cumulative or single-bar volume over its
//!   mean at the same offset into the previous periods (the last earlier bar of a period at that
//!   offset, zero before its first bar, trading days from the exchange zone and session); seasonality
//!   as the mean and population standard deviation of the same calendar month's percentage changes in
//!   earlier years, applied to the previous month's close; and trading sessions as bar opens in
//!   `[start, end)` exchange time, overnight when the end is not after the start, across daylight
//!   saving changes.
//! - Event-study rows run on the fixture and on hand-built series: Williams fractals by Bill Williams'
//!   five-bar rule with up to four equal highs (lows) before the fractal bar, pivot highs and lows by
//!   the conventional left/right window with the newest of equal extremes as the pivot, gaps by the
//!   conventional zone rule (a gap of at least a percentage of the 14-bar mean range, closed or
//!   shrunk by the first bar to enter it), and RSI divergences by comparing consecutive Wilder RSI
//!   pivots with the price at the same bars.
//!
//! Where a definition leaves a convention open, the expected series follows the one the study
//! documents (EMA and MACD seed with the first close, PPO and the Percentage Volume Oscillator
//! seed their EMAs with the mean of the first values and their signal with the mean of the first
//! values of the oscillator, Force Index, Elder Ray, Bull Bear Power, TRIX and the Mass Index seed their EMAs with the mean of
//! their first values, the Envelopes EMA and the Chaikin volatility and Chaikin Oscillator EMAs seed with their first input, an
//! Accumulation/Distribution bar with a high equal to its low adds nothing, the Price Momentum Oscillator's smoothings
//! and signal seed with the mean of their first inputs, the Relative Volatility Index and SMI Ergodic EMAs seed with
//! their first input and an unchanged close counts as a falling bar in the Relative Volatility Index, and Parabolic
//! SAR reverses when the price crosses it, not on a touch), otherwise a published one (Wilder's first true range is high minus
//! low, Keltner's EMA seeds with the SMA of the first closes). Where no published rule settles it,
//! the series pins the study's current behaviour: Supertrend starts bullish, an Aroon tie counts
//! the most recent bar, and Parabolic SAR takes its first direction from the first two closes (up
//! on a tie), starts at the first bar's low or high and takes its first extreme point from the
//! first two bars. The Chaikin volatility starts once its average and its change both have their full
//! length; the windows of the Choppiness Index, Vortex indicator and Ultimate Oscillator start at the
//! second bar, the first with a previous close. A window of the Advance/Decline Ratio of bars without a
//! down bar has no value, and a bar with its close equal to its open counts as neither up nor down. The
//! Median takes the nearest-rank median (the lower middle value for an even length) and seeds its EMA
//! with the mean of the first medians, the Chop Zone seeds its EMA with the mean of the first 34 closes,
//! and the Volatility Stop starts at the first close in an uptrend and uses the bar's true range as its
//! band until its ATR has a full length.

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
use crate::studies::{
    AcceleratorOscillator, AdvanceDeclineRatioBars, AroonOscillator, AverageDailyRange, BBTrend,
    BullBearPower, ChaikinOscillator, DetrendedPriceOscillator, KnowSureThing,
    RankCorrelationIndex, RelativeVigorIndex, StochasticRSI, UlcerIndex,
};
use crate::studies::{
    BollingerBars, ChandeKrollStop, ChopZone, Median, VolatilityStop, WilliamsAlligator,
};
use crate::studies::{Gaps, PivotPointsHighLow, RsiDivergence, WilliamsFractals};
use crate::studies::{
    PriceMomentumOscillator, PringsSpecialK, RciRibbon, RelativeVolatilityIndex,
    SMIErgodicIndicator, SMIErgodicOscillator, WoodiesCCI,
};
use chrono::{TimeZone, Utc};

const EXPECTED: &str = include_str!("reference/expected.txt");

#[test]
fn catalogue_input_schemas_round_trip_the_current_calculation() {
    let bars = fixture();
    let factory = crate::studies::IndicatorFactory::new();
    for name in factory.list() {
        let mut study = factory.create(&name).unwrap();
        study.calculate(&bars);
        let before = format!("{:?}", study.values());
        for input in study.inputs() {
            assert!(input.accepts(&input.value), "{name}: {}", input.id);
            study
                .set_input(&input.id, input.value)
                .unwrap_or_else(|error| panic!("{name}: {}: {error}", input.id));
        }
        study.calculate(&bars);
        assert_eq!(format!("{:?}", study.values()), before, "{name}");
    }
}

#[test]
fn editing_an_input_invalidates_values_and_preserves_other_inputs() {
    use crate::studies::InputValue;
    let bars: Vec<_> = [2.0, 4.0, 8.0]
        .into_iter()
        .enumerate()
        .map(|(i, p)| Bar::new(Utc.timestamp_opt(i as i64, 0).unwrap(), p, p, p, p, 1.0))
        .collect();
    let mut study = BollingerBands::new(3, 2.0);
    study.calculate(&bars);
    study.set_input("period", InputValue::Length(2)).unwrap();
    assert!(study.values().is_empty());
    study.calculate(&bars);
    assert_eq!(study.values().value(0, 2), Some(10.0));
    assert!(
        study
            .set_input("std_dev", InputValue::Number(f64::NAN))
            .is_err()
    );
    assert_eq!(study.values().value(0, 2), Some(10.0));
    assert!(study.set_input("period", InputValue::Length(0)).is_err());
    assert_eq!(
        study
            .inputs()
            .iter()
            .find(|input| input.id == "std_dev")
            .unwrap()
            .value,
        InputValue::Number(2.0)
    );
}

#[test]
fn bands_store_aligned_columns_through_warmup_and_corrections() {
    let mut bars: Vec<_> = [2.0, 4.0, 8.0]
        .into_iter()
        .enumerate()
        .map(|(i, p)| Bar::new(Utc.timestamp_opt(i as i64, 0).unwrap(), p, p, p, p, 1.0))
        .collect();
    let mut study = BollingerBands::new(2, 2.0);
    study.calculate(&bars);
    // Means 3 and 6, population deviations 1 and 2, and two-deviation bands.
    let expected = [[5.0, 10.0], [3.0, 6.0], [1.0, 2.0]];
    assert_eq!(study.values().columns().len(), 3);
    for (column, expected) in study.values().columns().iter().zip(expected) {
        assert_eq!(column.len(), bars.len());
        assert!(column[0].is_nan());
        assert_eq!(column[1..], expected);
    }
    bars[2].close = 10.0;
    study.update(&bars, 2);
    // The corrected window [4, 10] has mean 7 and population deviation 3.
    assert_eq!(
        (0..3)
            .map(|line| study.values().value(line, 2).unwrap())
            .collect::<Vec<_>>(),
        [13.0, 7.0, 1.0]
    );
}

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

/// The colour bands of the Chop Zone (0 steepest rise to 8 steepest fall) and the trend of the
/// Volatility Stop on the fixture, computed independently from the same definitions as the
/// expected rows, one character per bar, `-` where the bar has no colour.
#[test]
fn chop_zone_and_volatility_stop_colour_bars_by_their_definitions() {
    let bars = fixture();
    let cases: [(Box<dyn Indicator>, &str, &str); 3] = [
        (
            Box::new(ChopZone::default()),
            "012345678",
            "----------------------------------00000000000000000000000000000000000000000000000003088888888682036888888754888888888888888888888888888888888888888888",
        ),
        (
            Box::new(VolatilityStop::default()),
            "ud",
            "uuuuuddddduuuuuuuuuudddddddddddddduuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuudddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        ),
        (
            Box::new(VolatilityStop::new(3, 1.0)),
            "ud",
            "uuuuuddddduuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuddduuuuuuuuuuuuuuuuuudddddddddddddddduuuuuuuddddduuudddddduuddddddddddddddddddddddddduuuuddddd",
        ),
    ];
    for (mut study, symbols, expected) in cases {
        study.calculate(&bars);
        let colors = study.colors();
        let actual: String = (0..bars.len())
            .map(|bar| {
                study.plot_color(0, bar).map_or('-', |color| {
                    let band = colors.iter().position(|c| *c == color).unwrap();
                    symbols.as_bytes()[band] as char
                })
            })
            .collect();
        assert_eq!(actual, expected, "{}", study.name());
    }
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
        ("Alligator(13,8,5)", Box::new(WilliamsAlligator::default())),
        ("ChandeKroll(10,1,9)", Box::new(ChandeKrollStop::default())),
        ("Median(3,14,2)", Box::new(Median::default())),
        ("Median(4,14,2)", Box::new(Median::new(4, 14, 2.0))),
        ("VStop(20,2)", Box::new(VolatilityStop::default())),
        ("VStop(3,1)", Box::new(VolatilityStop::new(3, 1.0))),
        ("ChopZone", Box::new(ChopZone::default())),
        ("BollingerBars", Box::new(BollingerBars::default())),
        (
            "Ichimoku(9,26,52,22)",
            Box::new(IchimokuCloud::new(9, 26, 52).with_displacement(22)),
        ),
        ("AC(5,34,5)", Box::new(AcceleratorOscillator::default())),
        ("AroonOsc(14)", Box::new(AroonOscillator::default())),
        ("ADR(14)", Box::new(AverageDailyRange::default())),
        ("BBTrend(20,50,2)", Box::new(BBTrend::default())),
        ("ChaikinOsc(3,10)", Box::new(ChaikinOscillator::default())),
        ("BBP(13)", Box::new(BullBearPower::default())),
        (
            "ADRatioBars(9)",
            Box::new(AdvanceDeclineRatioBars::default()),
        ),
        ("Ulcer(14)", Box::new(UlcerIndex::default())),
        ("DPO(21)", Box::new(DetrendedPriceOscillator::default())),
        (
            "DPO(21,centered)",
            Box::new(DetrendedPriceOscillator::new(21).with_centered(true)),
        ),
        (
            "PMO(35,20,10)",
            Box::new(PriceMomentumOscillator::default()),
        ),
        ("RCIRibbon(10,30,50)", Box::new(RciRibbon::default())),
        ("RVI(10)", Box::new(RelativeVolatilityIndex::default())),
        ("SMIEI(20,5,5)", Box::new(SMIErgodicIndicator::default())),
        ("SMIEO(20,5,5)", Box::new(SMIErgodicOscillator::default())),
        ("Woodies(6,14)", Box::new(WoodiesCCI::default())),
        ("Fractals(2)", Box::new(WilliamsFractals::default())),
        ("Fractals(3)", Box::new(WilliamsFractals::new(3))),
        (
            "PivotsHL(10,10,10,10)",
            Box::new(PivotPointsHighLow::default()),
        ),
        (
            "PivotsHL(3,2,4,1)",
            Box::new(PivotPointsHighLow::new(3, 2, 4, 1)),
        ),
        ("Gaps(15,30)", Box::new(Gaps::default())),
        (
            "Gaps(3,10,partial,12)",
            Box::new(
                Gaps::new(3, 10.0)
                    .with_partial_closing(true)
                    .with_trail_limit(Some(12)),
            ),
        ),
        ("RSIDiv(14,5,5,5,60)", Box::new(RsiDivergence::default())),
        (
            "RSIDiv(14,3,2,2,40,hidden)",
            Box::new(RsiDivergence::new(14, 3, 2, 2, 40).with_divergences(true, true, true, true)),
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
    // 1000 closes in eighths of a tick: a 600-bar triangle wave with a ripple, long enough for every
    // Special K component and both signal smoothings.
    let long: Vec<_> = (0..1000_i64)
        .map(|i| {
            let c = (1600 + 2 * (i % 600 - 300).abs() + (i * 37) % 29) as f64 / 4.0;
            Bar::new(
                Utc.timestamp_opt(1_700_000_000 + i * 60, 0).unwrap(),
                c,
                c,
                c,
                c,
                1.0,
            )
        })
        .collect();
    // Bars of the given highs, lows and closes, each opening at its close.
    let hand = |rows: Vec<(f64, f64, f64)>| -> Vec<Bar> {
        rows.into_iter()
            .enumerate()
            .map(|(i, (h, l, c))| {
                Bar::new(
                    bars[0].time + chrono::Duration::minutes(i as i64),
                    c,
                    h,
                    l,
                    c,
                    1.0,
                )
            })
            .collect()
    };
    // Range-2 bars: an up gap filled in two steps, a gap too small, an up gap closed by a down
    // gap's bar, a down gap entered part way, and two later down gaps.
    let gap_bars = hand(
        [100.0; 14]
            .into_iter()
            .chain([
                103.0, 104.0, 104.0, 103.5, 102.75, 103.0, 102.5, 105.0, 108.0, 108.0, 104.0,
                104.0, 105.5, 104.0, 100.5, 100.0, 96.0,
            ])
            .chain([95.0; 9])
            .map(|m| (m + 1.0, m - 1.0, m))
            .collect(),
    );
    // Flat tops of two, five and six equal highs, and the same bottoms one lower.
    let tie_bars = hand(
        [
            1, 2, 5, 5, 3, 2, 1, 4, 4, 4, 4, 4, 4, 3, 2, 1, 4, 4, 4, 4, 4, 3, 2, 3, 3, 4, 5,
        ]
        .into_iter()
        .map(|h| (h as f64, h as f64 - 1.0, h as f64 - 0.5))
        .collect(),
    );
    // Closes in legs of (step, bars): a regular bullish and a regular bearish divergence.
    let legs = [
        (1.0, 20),
        (-3.0, 8),
        (2.0, 8),
        (-1.0, 18),
        (3.0, 10),
        (1.0, 8),
        (-2.0, 6),
        (1.0, 6),
        (0.75, 12),
        (-1.0, 6),
        (-3.0, 6),
        (2.0, 6),
        (-1.0, 13),
        (2.0, 8),
    ];
    let mut closes = vec![100.0];
    for (step, count) in legs {
        for _ in 0..count {
            closes.push(closes[closes.len() - 1] + step);
        }
    }
    let divergence_bars = hand(closes.into_iter().map(|c| (c + 1.0, c - 1.0, c)).collect());
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
            "Rank-flat-hand",
            Box::new(RankCorrelationIndex::new(4)),
            flat[..4].to_vec(),
        ),
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
        (
            "SpecialK-long",
            Box::new(PringsSpecialK::default()) as Box<dyn Indicator>,
            long,
        ),
        ("Gaps-hand", Box::new(Gaps::default()), gap_bars.clone()),
        (
            "Gaps-hand-partial",
            Box::new(Gaps::default().with_partial_closing(true)),
            gap_bars.clone(),
        ),
        (
            "Gaps-hand(3,partial,6)",
            Box::new(
                Gaps::new(3, 30.0)
                    .with_partial_closing(true)
                    .with_trail_limit(Some(6)),
            ),
            gap_bars,
        ),
        (
            "Fractals-ties",
            Box::new(WilliamsFractals::default()),
            tie_bars.clone(),
        ),
        (
            "PivotsHL-ties",
            Box::new(PivotPointsHighLow::new(2, 2, 2, 2)),
            tie_bars,
        ),
        (
            "RSIDiv-hand",
            Box::new(RsiDivergence::default()),
            divergence_bars.clone(),
        ),
        // 18 bars lie between the bullish pivots and 23 between the bearish ones.
        (
            "RSIDiv(14,5,5,5,18)-hand",
            Box::new(RsiDivergence::new(14, 5, 5, 5, 18)),
            divergence_bars,
        ),
    ]);
    use crate::studies::{
        AutoAnchor, PivotMethod, PivotPoints, PivotTimeframe, TimeWeightedAvgPrice,
        VwapAutoAnchored,
    };
    // The fixture's minute bars cross UTC midnight once, before bar 107.
    for (name, method) in [
        ("Pivots-Trad", PivotMethod::Traditional),
        ("Pivots-Fib", PivotMethod::Fibonacci),
        ("Pivots-Woodie", PivotMethod::Woodie),
        ("Pivots-Classic", PivotMethod::Classic),
        ("Pivots-DM", PivotMethod::DM),
        ("Pivots-Cam", PivotMethod::Camarilla),
    ] {
        studies.push((
            name,
            Box::new(PivotPoints::new().with_method(method)),
            bars.clone(),
        ));
    }
    // The fixture as daily bars, 14 November 2023 to 11 April 2024.
    let daily: Vec<_> = bars
        .iter()
        .enumerate()
        .map(|(i, b)| Bar {
            time: Utc
                .timestamp_opt(1_700_000_000 + i as i64 * 86_400, 0)
                .unwrap(),
            ..b.clone()
        })
        .collect();
    let mut monthly = PivotPoints::new().with_method(PivotMethod::Woodie);
    monthly.set_timeframe(crate::model::Timeframe::Day1);
    let auto = |anchor| Box::new(VwapAutoAnchored::new().with_anchor(anchor));
    studies.extend([
        (
            "Pivots-Woodie-auto-daily",
            Box::new(monthly) as Box<dyn Indicator>,
            daily.clone(),
        ),
        (
            "Pivots-Trad-weekly-daily",
            Box::new(PivotPoints::new().with_timeframe(PivotTimeframe::Weekly)),
            daily.clone(),
        ),
        (
            "Pivots-DM-monthly-back2",
            Box::new(
                PivotPoints::new()
                    .with_method(PivotMethod::DM)
                    .with_timeframe(PivotTimeframe::Monthly)
                    .with_pivots_back(2),
            ),
            daily,
        ),
        (
            "TWAP",
            Box::new(TimeWeightedAvgPrice::default()),
            bars.clone(),
        ),
        (
            "AutoVWAP",
            Box::new(VwapAutoAnchored::default()),
            bars.clone(),
        ),
        ("AutoVWAP-HH", auto(AutoAnchor::HighestHigh), bars.clone()),
        ("AutoVWAP-LL", auto(AutoAnchor::LowestLow), bars.clone()),
        ("AutoVWAP-HV", auto(AutoAnchor::HighestVolume), bars.clone()),
    ]);
    studies.extend(time_studies(&bars));
    for (_, study, data) in &mut studies {
        study.calculate(data);
    }

    let mut failures = Vec::new();
    let mut checked_lines = vec![0; studies.len()];
    let mut gap_rows = std::collections::BTreeMap::<usize, Vec<(usize, Vec<&str>)>>::new();
    for row in EXPECTED.lines() {
        let mut tokens = row.split_whitespace();
        let name = tokens.next().unwrap();
        let line: usize = tokens.next().unwrap().parse().unwrap();
        let idx = studies.iter().position(|(n, _, _)| *n == name);
        let idx = idx.unwrap_or_else(|| panic!("{name} has expected values but is not built"));
        checked_lines[idx] += 1;
        if name.starts_with("Gaps") {
            gap_rows
                .entry(idx)
                .or_default()
                .push((line, tokens.collect()));
            continue;
        }
        let values = studies[idx].1.values().to_rows();
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
                let actual = line_value(&value, line);
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
    // Gap rows hold gap `j`'s near and far edge on each bar it covers in lines `2 * (j % max)` and
    // `2 * (j % max) + 1`. Each run of bars with one extent is a zone; a zone after a partial fill
    // starts on the last bar of the one before.
    for (idx, mut rows) in gap_rows {
        rows.sort_by_key(|(line, _)| *line);
        let mut expected = Vec::new();
        for pair in rows.chunks(2) {
            let (near, far) = (&pair[0].1, &pair[1].1);
            let (mut previous, mut start) = (None, 0);
            for bar in 0..=near.len() {
                let current = near.get(bar).filter(|token| **token != "-").map(|token| {
                    (
                        token.parse::<f64>().unwrap(),
                        far[bar].parse::<f64>().unwrap(),
                    )
                });
                if current == previous {
                    continue;
                }
                if let Some((near, far)) = previous {
                    expected.push(crate::studies::StudyZone {
                        bars: start..bar,
                        top: near.max(far),
                        bottom: near.min(far),
                        plot: usize::from(near < far),
                    });
                }
                start = if previous.is_some() { bar - 1 } else { bar };
                previous = current;
            }
        }
        let mut actual = studies[idx].1.zones().to_vec();
        for zones in [&mut expected, &mut actual] {
            zones.sort_by_key(|zone| (zone.bars.start, zone.bars.end));
        }
        if actual != expected {
            failures.push(format!(
                "{}: expected zones {expected:?}, got {actual:?}",
                studies[idx].0
            ));
        }
    }
    for ((name, _, _), lines) in studies.iter().zip(&checked_lines) {
        assert!(*lines > 0, "{name} is built but has no expected values");
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The calendar-time studies, each with the series its rows were computed on.
fn time_studies(bars: &[Bar]) -> Vec<(&'static str, Box<dyn Indicator>, Vec<Bar>)> {
    use crate::model::{BarClock, Instrument, Session, SessionWindow, Timeframe, TimezoneId};
    use crate::studies::{
        RelativeVolumeAnchor, RelativeVolumeAtTime, RelativeVolumeMode, Seasonality,
        TradingSessions, Volume24h,
    };
    use chrono::{NaiveTime, Weekday};
    let series = |points: &[(i64, f64, f64)]| -> Vec<Bar> {
        points
            .iter()
            .map(|&(t, c, v)| Bar::new(Utc.timestamp_opt(t, 0).unwrap(), c, c, c, c, v))
            .collect()
    };
    let volumes = |points: &[(i64, f64)]| -> Vec<Bar> {
        series(&points.iter().map(|&(t, v)| (t, 1.0, v)).collect::<Vec<_>>())
    };
    let clock = |mut study: Box<dyn Indicator>, timeframe| {
        study.set_bar_clock(&BarClock::new(timeframe));
        study
    };
    let hour = |h| NaiveTime::from_hms_opt(h, 0, 0).unwrap();
    // 2024-03-04 is a Monday; the second day lacks 01:00 and the third starts at 01:00.
    let utc_days = volumes(
        &[
            (0, 100.0),
            (1, 50.0),
            (2, 150.0),
            (24, 120.0),
            (26, 60.0),
            (49, 80.0),
            (50, 40.0),
            (72, 300.0),
            (73, 100.0),
            (74, 200.0),
            (75, 100.0),
        ]
        .map(|(h, v)| (1_709_510_400 + h * 3_600, v)),
    );
    // Chicago 17:00 on 7, 8 and 10 March 2024 and the next mornings; clocks moved on the 10th.
    let chicago = volumes(&[
        (1_709_852_400, 100.0),
        (1_709_856_000, 100.0),
        (1_709_910_000, 200.0),
        (1_710_108_000, 300.0),
        (1_710_111_600, 100.0),
        (1_710_162_000, 400.0),
    ]);
    let futures = Instrument::new("FUTURES")
        .with_time_zone(TimezoneId::named("America/Chicago").unwrap())
        .with_session(Session::new([SessionWindow::new(
            [
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ],
            hour(17),
            hour(16),
        )]));
    let mut overnight = RelativeVolumeAtTime::new(1);
    overnight.set_instrument(Some(&futures));
    // Daily bars from Monday 1 January 2024; Monday the 8th and Thursday the 11th are missing.
    let daily = volumes(
        &[
            (1, 100.0),
            (2, 100.0),
            (3, 100.0),
            (4, 100.0),
            (5, 100.0),
            (9, 150.0),
            (10, 150.0),
            (12, 300.0),
        ]
        .map(|(d, v)| (1_704_067_200 + (d - 1) * 86_400, v)),
    );
    let seasons = series(
        &[
            ((2021, 12, 15), 100.0),
            ((2022, 1, 10), 104.0),
            ((2022, 1, 25), 110.0),
            ((2022, 2, 14), 99.0),
            ((2022, 12, 20), 120.0),
            ((2023, 1, 5), 118.0),
            ((2023, 1, 30), 126.0),
            ((2023, 2, 10), 130.0),
            ((2023, 2, 27), 132.0),
            ((2024, 1, 15), 140.0),
            ((2024, 2, 15), 150.0),
        ]
        .map(|((y, m, d), c)| {
            (
                Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap().timestamp(),
                c,
                1.0,
            )
        }),
    );
    // Session edges in January, after the United States moved its clocks in March but before
    // the United Kingdom did, and in British summer time.
    let sessions = volumes(
        &[
            1_705_269_540,
            1_705_269_600,
            1_705_276_800,
            1_705_298_340,
            1_705_298_400,
            1_705_307_400,
            1_705_327_200,
            1_705_329_000,
            1_705_336_140,
            1_705_336_200,
            1_710_163_800,
            1_710_187_140,
            1_710_187_200,
            1_719_819_000,
        ]
        .map(|t| (t, 1.0)),
    );
    let hourly = volumes(
        &[
            (0, 10.0),
            (1, 20.0),
            (5, 30.0),
            (23, 40.0),
            (24, 50.0),
            (25, 60.0),
            (49, 70.0),
            (72, 80.0),
            (96, 90.0),
        ]
        .map(|(h, v)| (h * 3_600, v)),
    );
    vec![
        ("Volume24h", Box::new(Volume24h::new()), bars.to_vec()),
        ("Volume24h-hand", Box::new(Volume24h::new()), hourly.clone()),
        (
            "Volume24h-weekly",
            clock(Box::<Volume24h>::default(), Timeframe::Week1),
            hourly[..4].to_vec(),
        ),
        (
            "RVOL-cumulative",
            Box::new(RelativeVolumeAtTime::new(2)),
            utc_days.clone(),
        ),
        (
            "RVOL-regular",
            Box::new(RelativeVolumeAtTime::new(2).with_mode(RelativeVolumeMode::Regular)),
            utc_days,
        ),
        ("RVOL-chicago", Box::new(overnight), chicago),
        (
            "RVOL-week-daily",
            clock(
                Box::new(RelativeVolumeAtTime::new(1).with_anchor(RelativeVolumeAnchor::Week)),
                Timeframe::Day1,
            ),
            daily,
        ),
        (
            "Seasonality-hand",
            Box::new(Seasonality::new()),
            seasons.clone(),
        ),
        (
            "Seasonality-2023",
            Box::new(Seasonality::new().with_start_year(2023)),
            seasons,
        ),
        (
            "Sessions-default",
            Box::new(TradingSessions::new()),
            sessions.clone(),
        ),
        (
            "Sessions-overnight",
            Box::new(TradingSessions::new().with_session(
                0,
                "Overnight",
                hour(22),
                hour(6),
                TimezoneId::utc(),
            )),
            sessions,
        ),
    ]
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
            .to_rows()
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
        for (i, value) in study.values().to_rows().iter().enumerate() {
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
    let IndicatorValue::Multiple(lines) = &open.values().to_rows()[3] else {
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
