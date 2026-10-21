//! Indicator factory for dynamic indicator creation by name.
//!
//! The [`IndicatorFactory`] provides a registry of constructor functions
//! keyed by indicator name strings. Every built-in indicator is
//! pre-registered under its canonical [`Indicator::name`] key, and users can
//! add their own at runtime.
//!
//! # Example
//!
//! ```rust,ignore
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
            // Order-flow / liquidity
            IcebergDetector,
            StopRunIndicator,
            LiquidityTracker,
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

    /// The factory exposes the full built-in catalogue. This pins the count so
    /// a dropped registration (or a name collision silently overwriting an
    /// entry) is caught immediately.
    /// Every built-in, with each of its length parameters set to `n`.
    fn builtins_with_length(n: usize) -> Vec<Box<dyn Indicator>> {
        vec![
            Box::new(SMA::new(n)),
            Box::new(EMA::new(n)),
            Box::new(WMA::new(n)),
            Box::new(DEMA::new(n)),
            Box::new(TEMA::new(n)),
            Box::new(HMA::new(n)),
            Box::new(ALMA::new(n, 0.85, 6.0)),
            Box::new(VWMA::new(n)),
            Box::new(LSMA::new(n)),
            Box::new(SWMA::new()),
            Box::new(KAMA::new(n).with_periods(n, n)),
            Box::new(SmoothedMA::new(n)),
            Box::new(GuppyMA::new()),
            Box::new(McGinleyDynamic::new(n)),
            Box::new(VIDYA::new(n)),
            Box::new(ZLEMA::new(n)),
            Box::new(HammingMA::new(n)),
            Box::new(MultipleMA::new()),
            Box::new(MAChannel::new(n, 1.0)),
            Box::new(EMACross::new(n, n)),
            Box::new(MACross::new(n, n)),
            Box::new(RSI::new(n)),
            Box::new(MACD::new(n, n, n)),
            Box::new(VolumeWeightedMACD::new(n, n, n)),
            Box::new(Stochastic::new(n, n, n)),
            Box::new(StochasticRSI::new(n, n, n, n)),
            Box::new(WilliamsR::new(n)),
            Box::new(CCI::new(n)),
            Box::new(RateOfChange::new(n)),
            Box::new(Aroon::new(n)),
            Box::new(AwesomeOscillator::new(n, n)),
            Box::new(UltimateOscillator::new(n, n, n)),
            Box::new(ConnorsRSI::new().with_periods(n, n, n)),
            Box::new(CoppockCurve::new().with_periods(n, n, n)),
            Box::new(TRIX::new(n).with_signal(n)),
            Box::new(ElderRay::new(n)),
            Box::new(FisherTransform::new(n)),
            Box::new(EhlersFisher::new(n)),
            Box::new(ForceIndex::new(n)),
            Box::new(MassIndex::new().with_periods(n, n)),
            Box::new(DetrendedPriceOscillator::new(n)),
            Box::new(KDJ::new(n, n, n)),
            Box::new(PrettyGoodOscillator::new(n)),
            Box::new(ChandeMomentumOscillator::new(n)),
            Box::new(TrueStrengthIndex::new(n, n, n)),
            Box::new(StochasticMomentumIndex::new(n, n, n, n)),
            Box::new(SchaffTrendCycle::new().with_periods(n, n, n)),
            Box::new(RelativeVigorIndex::new(n)),
            Box::new(DoubleStochastic::new(n, n)),
            Box::new(CenterOfGravity::new(n)),
            Box::new(KnowSureThing::new().with_params([n; 4], [n; 4], n)),
            Box::new(RainbowOscillator::new(n, n)),
            Box::new(UltimateMomentum::new(n, n, n)),
            Box::new(PriceOscillator::new(n, n, n)),
            Box::new(VolumeOscillator::new(n, n)),
            Box::new(BalanceOfPower::new(n)),
            Box::new(EaseOfMovement::new(n)),
            Box::new(ADX::new(n)),
            Box::new(SuperTrend::new(n, 3.0)),
            Box::new(IchimokuCloud::new(n, n, n)),
            Box::new(ParabolicSAR::new(0.02, 0.02, 0.2)),
            Box::new(DirectionalMovement::new(n)),
            Box::new(VortexIndicator::new(n)),
            Box::new(
                SqueezeMomentum::new()
                    .with_bb_params(n, 2.0)
                    .with_kc_params(n, 1.5),
            ),
            Box::new(TrendIntensityIndex::new(n)),
            Box::new(TrendDetectionIndex::new(n)),
            Box::new(Qstick::new(n)),
            Box::new(ChoppinessIndex::new(n)),
            Box::new(ElderImpulseSystem::new(n, n, n, n)),
            Box::new(BollingerBands::new(n, 2.0)),
            Box::new(BollingerBandsWidth::new(n, 2.0)),
            Box::new(ATR::new(n)),
            Box::new(TrueRange::new()),
            Box::new(KeltnerChannels::new(n, n, 2.0)),
            Box::new(DonchianChannels::new(n)),
            Box::new(StandardDeviation::new(n)),
            Box::new(HistoricalVolatility::new(n)),
            Box::new(ChaikinVolatility::new(n, n)),
            Box::new(ChandelierExit::new(n, 3.0)),
            Box::new(AccelerationBands::new(n, 4.0)),
            Box::new(Envelopes::new(n, 2.5, "SMA")),
            Box::new(PercentB::new(n, 2.0)),
            Box::new(CloseToCloseVol::new(n)),
            Box::new(OhlcVolatility::new(n)),
            Box::new(VolatilityIndex::new(n)),
            Box::new(ZeroTrendVol::new(n)),
            Box::new(CommoditySelectionIndex::new(n, 1.0)),
            Box::new(OnBalanceVolume::new()),
            Box::new(VolumeWeightedAvgPrice::new()),
            Box::new(AnchoredVWAP::new(0)),
            Box::new(MoneyFlowIndex::new(n)),
            Box::new(AccumulationDistribution::new()),
            Box::new(ChaikinMoneyFlow::new(n)),
            Box::new(VolumeRateOfChange::new(n)),
            Box::new(KlingerOscillator::new().with_periods(n, n, n)),
            Box::new(CVD::new()),
            Box::new(VolProfileVisible::new(n)),
            Box::new(VolProfileFixed::new(n)),
            Box::new(NetVolume::new()),
            Box::new(VolumePriceTrend::new()),
            Box::new(PriceVolumeTrendStudy::new(n)),
            Box::new(NegativeVolumeIndex::new()),
            Box::new(PositiveVolumeIndex::new()),
            Box::new(WilliamsAD::new()),
            Box::new(MarketFacilitationIndex::new()),
            Box::new(IcebergDetector::new(n)),
            Box::new(StopRunIndicator::new(n).with_reversal_window(n)),
            Box::new(LiquidityTracker::new(n)),
            Box::new(PivotPoints::new(n)),
            Box::new(ZigZag::new(5.0, n)),
            Box::new(PriceChannel::new(n)),
            Box::new(CorrelationCoefficient::new(n)),
            Box::new(CorrelationLogReturns::new(n)),
            Box::new(LinearRegression::new(n)),
            Box::new(LinearRegressionSlope::new(n)),
            Box::new(StandardError::new(n)),
            Box::new(StandardErrorBands::new(n, 2.0)),
            Box::new(RankCorrelationIndex::new(n)),
            Box::new(MajorityRule::new(n)),
            Box::new(AccumulationSwingIndex::new(0.5)),
            Box::new(TypicalPrice::new()),
            Box::new(MedianPrice::new()),
            Box::new(WeightedClose::new()),
            Box::new(HL2::new()),
            Box::new(HLC3::new()),
            Box::new(OHLC4::new()),
            Box::new(AdvanceDeclineLine::new()),
            Box::new(Week52HighLow::new(n)),
            Box::new(SpreadStudy::new()),
            Box::new(RatioStudy::new()),
        ]
    }

    /// No built-in may panic, hang or allocate without bound, whatever its
    /// lengths and bars. Lengths below the minimum clamp at construction, so
    /// a zero length must give exactly the output of the smallest valid one.
    #[test]
    fn degenerate_lengths_and_hostile_bars_are_survivable() {
        use crate::model::Bar;
        use chrono::{TimeZone, Utc};
        use std::panic::{AssertUnwindSafe, catch_unwind};

        let normal: Vec<Bar> = (0..200)
            .map(|i| {
                let c = 100.0 + 10.0 * (i as f64 / 7.0).sin();
                Bar {
                    time: Utc.timestamp_opt(i * 60, 0).unwrap(),
                    open: c - 0.5,
                    high: c + 1.0,
                    low: c - 1.0,
                    close: c,
                    volume: 1000.0 + i as f64,
                }
            })
            .collect();
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
        let mut covered: Vec<String> = builtins_with_length(1)
            .iter()
            .map(|s| s.name().to_string())
            .collect();
        let mut registered = factory.list();
        covered.sort();
        registered.sort();
        assert_eq!(covered, registered, "the table must cover every built-in");

        let mut failures = Vec::new();
        for n in [0, 1, usize::MAX] {
            for mut study in builtins_with_length(n) {
                for (label, bars) in &datasets {
                    let len = catch_unwind(AssertUnwindSafe(|| {
                        study.calculate(bars);
                        study.values().len()
                    }));
                    match len {
                        Err(_) => {
                            failures.push(format!("{}({n}) on {label}: panicked", study.name()))
                        }
                        Ok(len) if len != bars.len() => failures.push(format!(
                            "{}({n}) on {label}: {len} values for {} bars",
                            study.name(),
                            bars.len()
                        )),
                        Ok(_) => {}
                    }
                }
            }
        }
        for (mut zero, mut one) in builtins_with_length(0)
            .into_iter()
            .zip(builtins_with_length(1))
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
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn factory_exposes_full_builtin_catalogue() {
        let factory = IndicatorFactory::new();
        assert_eq!(
            factory.count(),
            131,
            "expected every built-in indicator to be registered exactly once",
        );
    }
}
