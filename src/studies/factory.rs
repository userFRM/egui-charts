//! Indicator factory for dynamic indicator creation by name.
//!
//! The [`IndicatorFactory`] provides a registry of constructor functions
//! keyed by indicator name strings. Every built-in indicator is
//! pre-registered under its canonical [`Indicator::name`] key, and users can
//! add their own at runtime.
//!
//! # Example
//!
//! ```rust
//! use egui_charts::studies::IndicatorFactory;
//!
//! let factory = IndicatorFactory::new(); // includes all built-ins
//! let rsi = factory.create("RSI").unwrap();
//! assert_eq!(rsi.name(), "RSI");
//!
//! // Register a custom entry
//! let mut factory = factory;
//! factory.register("MySMA", || Box::new(egui_charts::studies::SMA::new(42)));
//! ```

use crate::studies::*;
use std::collections::HashMap;

/// Type alias for indicator constructor closures stored in the factory.
type IndicatorConstructor = Box<dyn Fn() -> Box<dyn Indicator> + Send + Sync>;

/// Register every listed built-in indicator type with the factory.
///
/// Each indicator is constructed through its [`Default`] implementation --
/// the single canonical source of its conventional parameters -- and keyed by
/// the [`Indicator::name`] it reports. Keying off `name()` keeps the registry
/// in lock-step with the indicators themselves: a name can never drift from
/// the value the rendering layer and legends display, and a new indicator is
/// reachable through the factory the moment it is added to this list.
macro_rules! register_builtins {
    ($factory:expr, $($ty:ty),+ $(,)?) => {
        $(
            // Read the canonical name from a throwaway default instance so the
            // key is always exactly what the indicator reports at runtime.
            let key = <$ty as Default>::default().name().to_string();
            $factory.register(key, || Box::new(<$ty as Default>::default()));
        )+
    };
}

/// A factory that creates [`Indicator`] instances by name.
///
/// On construction, every built-in indicator is registered under its canonical
/// [`Indicator::name`] (e.g. `"SMA"`, `"RSI"`, `"MACD"`, `"SuperTrend"`) using
/// its conventional default parameters. Additional indicators can be registered
/// at runtime with [`register`](Self::register).
pub struct IndicatorFactory {
    constructors: HashMap<String, IndicatorConstructor>,
}

impl IndicatorFactory {
    /// Create a new factory pre-loaded with every built-in indicator.
    pub fn new() -> Self {
        let mut factory = Self {
            constructors: HashMap::new(),
        };

        factory.register_builtin();
        factory
    }

    /// Register every built-in indicator under its canonical name.
    ///
    /// The list below is the single registration site for built-ins: adding a
    /// new indicator type here makes it constructible by name with zero further
    /// wiring. Each entry is instantiated via its [`Default`] implementation.
    fn register_builtin(&mut self) {
        register_builtins!(
            self,
            // Moving averages
            SMA,
            EMA,
            WMA,
            DEMA,
            TEMA,
            HMA,
            ALMA,
            VWMA,
            LSMA,
            SWMA,
            KAMA,
            SmoothedMA,
            GuppyMA,
            McGinleyDynamic,
            VIDYA,
            ZLEMA,
            HammingMA,
            MultipleMA,
            MAChannel,
            EMACross,
            MACross,
            // Momentum / oscillators
            RSI,
            MACD,
            VolumeWeightedMACD,
            Stochastic,
            StochasticRSI,
            WilliamsR,
            CCI,
            Momentum,
            RateOfChange,
            Aroon,
            AwesomeOscillator,
            UltimateOscillator,
            ConnorsRSI,
            CoppockCurve,
            TRIX,
            ElderRay,
            FisherTransform,
            EhlersFisher,
            ForceIndex,
            MassIndex,
            DetrendedPriceOscillator,
            KDJ,
            PrettyGoodOscillator,
            ChandeMomentumOscillator,
            TrueStrengthIndex,
            StochasticMomentumIndex,
            SchaffTrendCycle,
            RelativeVigorIndex,
            DoubleStochastic,
            CenterOfGravity,
            KnowSureThing,
            RainbowOscillator,
            UltimateMomentum,
            PriceOscillator,
            VolumeOscillator,
            BalanceOfPower,
            EaseOfMovement,
            // Trend
            ADX,
            SuperTrend,
            IchimokuCloud,
            ParabolicSAR,
            DirectionalMovement,
            VortexIndicator,
            SqueezeMomentum,
            TrendIntensityIndex,
            TrendDetectionIndex,
            Qstick,
            ChoppinessIndex,
            ElderImpulseSystem,
            // Volatility
            BollingerBands,
            BollingerBandsWidth,
            ATR,
            TrueRange,
            KeltnerChannels,
            DonchianChannels,
            StandardDeviation,
            HistoricalVolatility,
            ChaikinVolatility,
            ChandelierExit,
            AccelerationBands,
            Envelopes,
            PercentB,
            CloseToCloseVol,
            OhlcVolatility,
            VolatilityIndex,
            ZeroTrendVol,
            CommoditySelectionIndex,
            // Volume
            OnBalanceVolume,
            VolumeWeightedAvgPrice,
            AnchoredVWAP,
            MoneyFlowIndex,
            AccumulationDistribution,
            ChaikinMoneyFlow,
            VolumeRateOfChange,
            KlingerOscillator,
            CVD,
            VolProfileVisible,
            VolProfileFixed,
            NetVolume,
            VolumePriceTrend,
            PriceVolumeTrendStudy,
            NegativeVolumeIndex,
            PositiveVolumeIndex,
            WilliamsAD,
            MarketFacilitationIndex,
            // Support / resistance
            PivotPoints,
            ZigZag,
            PriceChannel,
            // Statistics
            CorrelationCoefficient,
            CorrelationLogReturns,
            LinearRegression,
            LinearRegressionSlope,
            StandardError,
            StandardErrorBands,
            RankCorrelationIndex,
            MajorityRule,
            AccumulationSwingIndex,
            // Price calculations
            TypicalPrice,
            MedianPrice,
            WeightedClose,
            HL2,
            HLC3,
            OHLC4,
            // Breadth & spread
            AdvanceDeclineLine,
            Week52HighLow,
            SpreadStudy,
            RatioStudy,
        );
    }

    /// Register a named indicator constructor.
    ///
    /// If a constructor with the same name already exists it is replaced.
    ///
    /// # Arguments
    /// * `name` -- The lookup key (e.g. `"MySMA"`).
    /// * `constructor` -- A closure that returns a new boxed indicator.
    pub fn register<F>(&mut self, name: impl Into<String>, constructor: F)
    where
        F: Fn() -> Box<dyn Indicator> + Send + Sync + 'static,
    {
        self.constructors.insert(name.into(), Box::new(constructor));
    }

    /// Create an indicator by its registered name, or `None` if not found.
    pub fn create(&self, name: &str) -> Option<Box<dyn Indicator>> {
        self.constructors.get(name).map(|f| f())
    }

    /// Return a list of all registered indicator names (unordered).
    pub fn list(&self) -> Vec<String> {
        self.constructors.keys().cloned().collect()
    }

    /// Check whether an indicator with `name` is registered.
    pub fn has(&self, name: &str) -> bool {
        self.constructors.contains_key(name)
    }

    /// Remove the indicator registered under `name`.
    ///
    /// Returns `true` if it was present and removed, `false` otherwise.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.constructors.remove(name).is_some()
    }

    /// Remove all registered indicators, including built-ins.
    pub fn clear(&mut self) {
        self.constructors.clear();
    }

    /// Return the number of currently registered indicators.
    pub fn count(&self) -> usize {
        self.constructors.len()
    }
}

impl Default for IndicatorFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory() {
        let factory = IndicatorFactory::new();

        // Canonical built-in keys are present.
        assert!(factory.has("RSI"));
        assert!(factory.has("MACD"));

        // Creation yields the matching indicator.
        let rsi = factory.create("RSI");
        assert!(rsi.is_some());
        assert_eq!(rsi.unwrap().name(), "RSI");

        let list = factory.list();
        assert!(!list.is_empty());
    }

    #[test]
    fn test_custom_indicator() {
        let mut factory = IndicatorFactory::new();

        factory.register("Custom", || Box::new(SMA::new(10)));

        assert!(factory.has("Custom"));
        assert!(factory.create("Custom").is_some());
    }

    /// Every registered name must construct, and the produced indicator's
    /// `name()` must round-trip back to the key it was registered under. This
    /// guards the factory's core invariant: a name returned by `list()` is a
    /// name that `create()` honors, and the key matches the indicator identity.
    #[test]
    fn every_registered_name_round_trips() {
        let factory = IndicatorFactory::new();

        for name in factory.list() {
            let indicator = factory
                .create(&name)
                .unwrap_or_else(|| panic!("registered name {name:?} failed to construct"));
            assert_eq!(
                indicator.name(),
                name,
                "indicator constructed for key {name:?} reports a different name()",
            );
        }
    }

    /// Every built-in, with its `k`-th length parameter set to `l(k)` and each
    /// float parameter of conventional value `v` set to `x(v)`.
    fn builtins_with(
        l: impl Fn(usize) -> usize,
        x: impl Fn(f64) -> f64,
    ) -> Vec<Box<dyn Indicator>> {
        vec![
            Box::new(SMA::new(l(0))),
            Box::new(EMA::new(l(0))),
            Box::new(WMA::new(l(0))),
            Box::new(DEMA::new(l(0))),
            Box::new(TEMA::new(l(0))),
            Box::new(HMA::new(l(0))),
            Box::new(ALMA::new(l(0), x(0.85), x(6.0))),
            Box::new(VWMA::new(l(0))),
            Box::new(LSMA::new(l(0))),
            Box::new(SWMA::new()),
            Box::new(KAMA::new(l(0)).with_periods(l(1), l(2))),
            Box::new(SmoothedMA::new(l(0))),
            Box::new(GuppyMA::new()),
            Box::new(McGinleyDynamic::new(l(0))),
            Box::new(VIDYA::new(l(0))),
            Box::new(ZLEMA::new(l(0))),
            Box::new(HammingMA::new(l(0))),
            Box::new(MultipleMA::new()),
            Box::new(MAChannel::new(l(0), x(1.0))),
            Box::new(EMACross::new(l(0), l(1))),
            Box::new(MACross::new(l(0), l(1))),
            Box::new(RSI::new(l(0))),
            Box::new(MACD::new(l(0), l(1), l(2))),
            Box::new(VolumeWeightedMACD::new(l(0), l(1), l(2))),
            Box::new(Stochastic::new(l(0), l(1), l(2))),
            Box::new(StochasticRSI::new(l(0), l(1), l(2), l(3))),
            Box::new(WilliamsR::new(l(0))),
            Box::new(CCI::new(l(0))),
            Box::new(RateOfChange::new(l(0))),
            Box::new(Momentum::new(l(0))),
            Box::new(Aroon::new(l(0))),
            Box::new(AwesomeOscillator::new(l(0), l(1))),
            Box::new(UltimateOscillator::new(l(0), l(1), l(2))),
            Box::new(ConnorsRSI::new().with_periods(l(0), l(1), l(2))),
            Box::new(CoppockCurve::new().with_periods(l(0), l(1), l(2))),
            Box::new(TRIX::new(l(0)).with_signal(l(1))),
            Box::new(ElderRay::new(l(0))),
            Box::new(FisherTransform::new(l(0))),
            Box::new(EhlersFisher::new(l(0))),
            Box::new(ForceIndex::new(l(0))),
            Box::new(MassIndex::new().with_periods(l(0), l(1))),
            Box::new(DetrendedPriceOscillator::new(l(0))),
            Box::new(KDJ::new(l(0), l(1), l(2))),
            Box::new(PrettyGoodOscillator::new(l(0))),
            Box::new(ChandeMomentumOscillator::new(l(0))),
            Box::new(TrueStrengthIndex::new(l(0), l(1), l(2))),
            Box::new(StochasticMomentumIndex::new(l(0), l(1), l(2), l(3))),
            Box::new(SchaffTrendCycle::new().with_periods(l(0), l(1), l(2))),
            Box::new(RelativeVigorIndex::new(l(0))),
            Box::new(DoubleStochastic::new(l(0), l(1))),
            Box::new(CenterOfGravity::new(l(0))),
            Box::new(KnowSureThing::new().with_params(
                [l(0), l(1), l(2), l(3)],
                [l(4), l(5), l(6), l(7)],
                l(8),
            )),
            Box::new(RainbowOscillator::new(l(0), l(1))),
            Box::new(UltimateMomentum::new(l(0), l(1), l(2))),
            Box::new(PriceOscillator::new(l(0), l(1), l(2))),
            Box::new(VolumeOscillator::new(l(0), l(1))),
            Box::new(BalanceOfPower::new(l(0))),
            Box::new(EaseOfMovement::new(l(0)).with_divisor(x(1e4))),
            Box::new(ADX::new(l(0))),
            Box::new(SuperTrend::new(l(0), x(3.0))),
            Box::new(IchimokuCloud::new(l(0), l(1), l(2))),
            Box::new(ParabolicSAR::new(x(0.02), x(0.02), x(0.2))),
            Box::new(DirectionalMovement::new(l(0))),
            Box::new(VortexIndicator::new(l(0))),
            Box::new(
                SqueezeMomentum::new()
                    .with_bb_params(l(0), x(2.0))
                    .with_kc_params(l(1), x(1.5)),
            ),
            Box::new(TrendIntensityIndex::new(l(0))),
            Box::new(TrendDetectionIndex::new(l(0))),
            Box::new(Qstick::new(l(0))),
            Box::new(ChoppinessIndex::new(l(0))),
            Box::new(ElderImpulseSystem::new(l(0), l(1), l(2), l(3))),
            Box::new(BollingerBands::new(l(0), x(2.0))),
            Box::new(BollingerBandsWidth::new(l(0), x(2.0))),
            Box::new(ATR::new(l(0))),
            Box::new(TrueRange::new()),
            Box::new(KeltnerChannels::new(l(0), l(1), x(2.0))),
            Box::new(DonchianChannels::new(l(0))),
            Box::new(StandardDeviation::new(l(0))),
            Box::new(HistoricalVolatility::new(l(0)).with_trading_days(x(252.0))),
            Box::new(ChaikinVolatility::new(l(0), l(1))),
            Box::new(ChandelierExit::new(l(0), x(3.0))),
            Box::new(AccelerationBands::new(l(0), x(4.0))),
            Box::new(Envelopes::new(l(0), x(2.5), "SMA")),
            Box::new(PercentB::new(l(0), x(2.0))),
            Box::new(CloseToCloseVol::new(l(0))),
            Box::new(OhlcVolatility::new(l(0))),
            Box::new(VolatilityIndex::new(l(0))),
            Box::new(ZeroTrendVol::new(l(0))),
            Box::new(
                CommoditySelectionIndex::new(l(0))
                    .with_point_value(x(1.0))
                    .with_margin(x(1.0))
                    .with_commission(x(1.0)),
            ),
            Box::new(OnBalanceVolume::new()),
            Box::new(VolumeWeightedAvgPrice::new().with_std_dev_bands([x(1.0), x(2.0)])),
            Box::new(AnchoredVWAP::new(0)),
            Box::new(MoneyFlowIndex::new(l(0))),
            Box::new(AccumulationDistribution::new()),
            Box::new(ChaikinMoneyFlow::new(l(0))),
            Box::new(VolumeRateOfChange::new(l(0))),
            Box::new(KlingerOscillator::new().with_periods(l(0), l(1), l(2))),
            Box::new(CVD::new()),
            Box::new(VolProfileVisible::new(l(0))),
            Box::new(VolProfileFixed::new(l(0))),
            Box::new(NetVolume::new()),
            Box::new(VolumePriceTrend::new()),
            Box::new(PriceVolumeTrendStudy::new(l(0))),
            Box::new(NegativeVolumeIndex::new()),
            Box::new(PositiveVolumeIndex::new()),
            Box::new(WilliamsAD::new()),
            Box::new(MarketFacilitationIndex::new().with_divisor(x(1.0))),
            Box::new(PivotPoints::new(l(0))),
            Box::new(ZigZag::new(x(5.0), l(0))),
            Box::new(PriceChannel::new(l(0))),
            Box::new(CorrelationCoefficient::new(l(0))),
            Box::new(CorrelationLogReturns::new(l(0))),
            Box::new(LinearRegression::new(l(0))),
            Box::new(LinearRegressionSlope::new(l(0))),
            Box::new(StandardError::new(l(0))),
            Box::new(StandardErrorBands::new(l(0), x(2.0))),
            Box::new(RankCorrelationIndex::new(l(0))),
            Box::new(MajorityRule::new(l(0))),
            Box::new(AccumulationSwingIndex::new(x(0.5))),
            Box::new(TypicalPrice::new()),
            Box::new(MedianPrice::new()),
            Box::new(WeightedClose::new()),
            Box::new(HL2::new()),
            Box::new(HLC3::new()),
            Box::new(OHLC4::new()),
            Box::new(AdvanceDeclineLine::new()),
            Box::new(Week52HighLow::new(l(0))),
            Box::new(SpreadStudy::new()),
            Box::new(RatioStudy::new()),
        ]
    }

    /// No built-in may panic, hang or allocate without bound, whatever its
    /// lengths and bars. Lengths below the minimum clamp at construction, so
    /// a zero length must give exactly the output of the smallest valid one,
    /// and float parameters clamp into their documented range.
    #[test]
    fn degenerate_lengths_and_hostile_bars_are_survivable() {
        use crate::model::Bar;
        use chrono::{TimeZone, Utc};
        use std::panic::{AssertUnwindSafe, catch_unwind};

        let bar = |i: i64| {
            let c = 100.0 + 10.0 * (i as f64 / 7.0).sin();
            Bar {
                time: Utc.timestamp_opt(i * 60, 0).unwrap(),
                open: c - 0.5,
                high: c + 1.0,
                low: c - 1.0,
                close: c,
                volume: 1000.0 + i as f64,
            }
        };
        let normal: Vec<Bar> = (0..200).map(bar).collect();
        let edit = |f: fn(usize, &mut Bar)| -> Vec<Bar> {
            let mut bars = normal.clone();
            bars.iter_mut().enumerate().for_each(|(i, b)| f(i, b));
            bars
        };
        let datasets = [
            ("empty", Vec::new()),
            ("one bar", normal[..1].to_vec()),
            ("normal", normal.clone()),
            (
                "NaN",
                edit(|i, b| {
                    if i % 10 == 3 {
                        b.close = f64::NAN;
                        b.high = f64::NAN;
                    }
                }),
            ),
            (
                "infinite",
                edit(|i, b| {
                    if i % 10 == 3 {
                        b.high = f64::INFINITY;
                        b.low = f64::NEG_INFINITY;
                    }
                }),
            ),
            (
                "±1e300",
                edit(|i, b| {
                    if i % 10 == 3 {
                        b.high = 1e300;
                        b.low = -1e300;
                    }
                }),
            ),
            (
                "1e12 scale",
                edit(|i, b| {
                    let s = if i % 2 == 0 { 1e12 } else { 1e-9 };
                    (b.open, b.high, b.low, b.close) =
                        (b.open * s, b.high * s, b.low * s, b.close * s);
                }),
            ),
            ("zero volume", edit(|_, b| b.volume = 0.0)),
            (
                "flat",
                edit(|_, b| (b.open, b.high, b.low, b.close) = (100.0, 100.0, 100.0, 100.0)),
            ),
        ];

        let factory = IndicatorFactory::new();
        let mut covered: Vec<String> = builtins_with(|_| 1, |v| v)
            .iter()
            .map(|s| s.name().to_string())
            .collect();
        let mut registered = factory.list();
        covered.sort();
        registered.sort();
        assert_eq!(covered, registered, "the table must cover every built-in");

        // Every length equal, then each length alone huge, then each alone longer than the rest
        // (a swapped fast/slow pair, for example).
        let mut lengths: Vec<[usize; 9]> = vec![[0; 9], [1; 9], [usize::MAX; 9]];
        for k in 0..9 {
            let mut huge = [1; 9];
            huge[k] = usize::MAX;
            let mut longer = [20; 9];
            longer[k] = 50;
            lengths.extend([huge, longer]);
        }

        let mut failures = Vec::new();
        for n in lengths {
            for mut study in builtins_with(|k| n[k], |v| v) {
                for (label, bars) in &datasets {
                    let len = catch_unwind(AssertUnwindSafe(|| {
                        study.calculate(bars);
                        study.values().len()
                    }));
                    match len {
                        Err(_) => {
                            failures.push(format!("{}{n:?} on {label}: panicked", study.name()))
                        }
                        Ok(len) if len != bars.len() => failures.push(format!(
                            "{}{n:?} on {label}: {len} values for {} bars",
                            study.name(),
                            bars.len()
                        )),
                        Ok(_) => {}
                    }
                }
            }
        }
        for (mut zero, mut one) in builtins_with(|_| 0, |v| v)
            .into_iter()
            .zip(builtins_with(|_| 1, |v| v))
        {
            if catch_unwind(AssertUnwindSafe(|| {
                zero.calculate(&normal);
                one.calculate(&normal);
            }))
            .is_ok()
                && format!("{:?}", zero.values()) != format!("{:?}", one.values())
            {
                failures.push(format!("{}(0) differs from {}(1)", zero.name(), one.name()));
            }
        }

        // Float parameters clamp into their documented range at construction, NaN to its
        // bottom: NaN, -inf and -1 must give the output of 0.0, and +inf that of f64::MAX.
        // The bottom of every range must still draw wherever the conventional value does.
        let run = |x: fn(f64) -> f64| {
            let mut studies = builtins_with(|_| 14, x);
            for study in &mut studies {
                // A study that panics is reported by the table above
                let _ = catch_unwind(AssertUnwindSafe(|| study.calculate(&normal)));
            }
            studies
        };
        let (conventional, bottom) = (run(|v| v), run(|_| 0.0));
        for (label, hostile, clamped) in [
            ("NaN", run(|_| f64::NAN), &bottom),
            ("-inf", run(|_| f64::NEG_INFINITY), &bottom),
            ("-1", run(|_| -1.0), &bottom),
            ("+inf", run(|_| f64::INFINITY), &run(|_| f64::MAX)),
        ] {
            for (study, clamped) in hostile.iter().zip(clamped) {
                if format!("{:?}", study.values()) != format!("{:?}", clamped.values()) {
                    failures.push(format!(
                        "{} with float parameters {label} not clamped",
                        study.name()
                    ));
                }
            }
        }
        let finite = |v: &IndicatorValue| match v {
            IndicatorValue::Single(x) => vec![x.is_finite()],
            IndicatorValue::Multiple(xs) => xs.iter().map(|x| x.is_finite()).collect(),
            IndicatorValue::None => Vec::new(),
        };
        // A Zig Zag deviation of 0 filters nothing, so it has no values by design.
        for (study, conventional) in bottom
            .iter()
            .zip(&conventional)
            .filter(|(study, _)| study.name() != "ZigZag")
        {
            let lost = study
                .values()
                .iter()
                .zip(conventional.values())
                .any(|(b, c)| {
                    let (b, c) = (finite(b), finite(c));
                    c.iter()
                        .enumerate()
                        .any(|(k, c)| *c && !b.get(k).copied().unwrap_or(false))
                });
            if lost {
                failures.push(format!(
                    "{} with float parameters at 0.0 loses values",
                    study.name()
                ));
            }
        }

        // One non-finite bar must not blank the rest of the series: wherever the series without
        // it has a finite last value, the series with it has one too, whether the bar has NaN
        // prices, an infinite high, a NaN volume or a NaN open. 600 bars keep every window at
        // the last bar clear of bar 100. Each study listed still carries the bar to the end,
        // under the issue that tracks its fix; the list may only shrink.
        const CARRY_NON_FINITE: &[(&str, u32)] = &[
            ("VWAP", 647),
            ("AVWAP", 647),
            ("A/D", 647),
            ("CVD", 647),
            ("VPT", 647),
            ("PVT Enhanced", 647),
            ("PVI", 647),
            ("WAD", 647),
            ("ASI", 647),
            ("Volume Osc", 647),
            ("Vol Profile VR", 647),
            ("Vol Profile FR", 647),
        ];
        let long: Vec<Bar> = (0..600).map(bar).collect();
        let mut without = long.clone();
        without.remove(100);
        let mut nan = long.clone();
        (nan[100].high, nan[100].low, nan[100].close) = (f64::NAN, f64::NAN, f64::NAN);
        let mut infinite = long.clone();
        infinite[100].high = f64::INFINITY;
        let mut no_volume = long.clone();
        no_volume[100].volume = f64::NAN;
        let mut no_open = long.clone();
        no_open[100].open = f64::NAN;
        let last = |study: &mut Box<dyn Indicator>, bars: &[Bar]| {
            catch_unwind(AssertUnwindSafe(|| study.calculate(bars))).ok()?;
            study.values().last().map(finite)
        };
        for mut study in builtins_with(|_| 14, |v| v) {
            let Some(clean) = last(&mut study, &without) else {
                continue;
            };
            let carries = [&nan, &infinite, &no_volume, &no_open].iter().any(|bars| {
                let hostile = last(&mut study, bars).unwrap_or_default();
                clean
                    .iter()
                    .enumerate()
                    .any(|(k, c)| *c && !hostile.get(k).copied().unwrap_or(false))
            });
            let listed = CARRY_NON_FINITE
                .iter()
                .find(|(name, _)| *name == study.name());
            match (carries, listed) {
                (true, None) => failures.push(format!(
                    "{} carries one non-finite bar to the end of the series",
                    study.name()
                )),
                (false, Some((name, issue))) => failures.push(format!(
                    "{name} no longer carries a non-finite bar; remove it from CARRY_NON_FINITE (#{issue})"
                )),
                _ => {}
            }
        }
        for (name, issue) in CARRY_NON_FINITE {
            if !registered.iter().any(|r| r == name) {
                failures.push(format!(
                    "{name} in CARRY_NON_FINITE is not a study (#{issue})"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
