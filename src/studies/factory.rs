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
    #[test]
    fn factory_exposes_full_builtin_catalogue() {
        let factory = IndicatorFactory::new();
        assert_eq!(
            factory.count(),
            130,
            "expected every built-in indicator to be registered exactly once",
        );
    }
}
