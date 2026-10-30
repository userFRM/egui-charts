//! Indicator factory for dynamic indicator creation by name.
//!
//! The [`IndicatorFactory`] provides a registry of constructor functions
//! keyed by indicator name strings. The built-in catalogue is
//! registered under its canonical [`Indicator::name`] key and a category,
//! and users can add their own at runtime. It is the catalogue of studies: a
//! picker lists [`list`](IndicatorFactory::list), describes each entry with
//! [`Indicator::desc`] and [`category`](IndicatorFactory::category), and
//! creates it with [`create`](IndicatorFactory::create).
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

use crate::data::OrderFlowKind;
use crate::studies::*;
use std::collections::HashMap;

/// Type alias for indicator constructor closures stored in the factory.
type IndicatorConstructor = Box<dyn Fn() -> Box<dyn Indicator> + Send + Sync>;

/// Register every listed built-in indicator type with the factory, under its category.
///
/// Each indicator is constructed through its [`Default`] implementation --
/// the single canonical source of its conventional parameters -- and keyed by
/// the [`Indicator::name`] it reports. Keying off `name()` keeps the registry
/// in lock-step with the indicators themselves: a name can never drift from
/// the value the rendering layer and legends display, and a new indicator is
/// reachable through the factory the moment it is added to this list.
macro_rules! register_builtins {
    ($factory:expr, $($category:literal => [$($ty:ty),+ $(,)?]),+ $(,)?) => {
        $($(
            // Read the canonical name from a throwaway default instance so the
            // key is always exactly what the indicator reports at runtime.
            let key = <$ty as Default>::default().name().to_string();
            $factory.constructors.insert(
                key,
                ($category, Box::new(|| Box::new(<$ty as Default>::default()))),
            );
        )+)+
    };
}

/// A factory that creates [`Indicator`] instances by name.
///
/// On construction, catalogue studies are registered under their canonical
/// [`Indicator::name`] (e.g. `"SMA"`, `"RSI"`, `"MACD"`, `"SuperTrend"`) using
/// its conventional default parameters. Each name selects a distinct calculation
/// or source range. Additional indicators can be registered at runtime with
/// [`register`](Self::register).
pub struct IndicatorFactory {
    constructors: HashMap<String, (&'static str, IndicatorConstructor)>,
}

impl IndicatorFactory {
    /// The category of every indicator added with [`register`](Self::register).
    pub const CUSTOM: &'static str = "Custom";
    /// The category of the user's own studies, such as scripts the host compiles or imports;
    /// a study browser lists them as the user's scripts.
    pub const PERSONAL: &'static str = "Personal";
    /// The category of studies the host obtains from other users.
    pub const COMMUNITY: &'static str = "Community";
    /// The category of the built-in volume and time price profiles.
    pub const PROFILES: &'static str = "Profiles";
    /// The category of the built-in pattern detectors.
    pub const PATTERNS: &'static str = "Patterns";

    /// Create a factory with the built-in catalogue enabled by the current features.
    pub fn new() -> Self {
        let mut factory = Self {
            constructors: HashMap::new(),
        };

        factory.register_builtin();
        factory
    }

    /// Register every built-in indicator under its canonical name.
    ///
    /// Types use their defaults; automatic drawings select a construction kind and
    /// stream studies select aggregation parameters. Names come from each study.
    fn register_builtin(&mut self) {
        register_builtins!(
            self,
            "Moving averages" => [
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
                MixedMACross,
                TripleMA,
                MARibbon,
            ],
            "Momentum" => [
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
                RsiDivergence,
                KnoxvilleDivergence,
                TechnicalRatings,
                UltimateOscillator,
                ConnorsRSI,
                CoppockCurve,
                TRIX,
                ElderRay,
                FisherTransform,
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
                PercentageVolumeOscillator,
                VolumeOscillator,
                BalanceOfPower,
                EaseOfMovement,
                AcceleratorOscillator,
                AroonOscillator,
                BullBearPower,
                AdvanceDeclineRatioBars,
                PriceMomentumOscillator,
                PringsSpecialK,
                SMIErgodicIndicator,
                SMIErgodicOscillator,
                WoodiesCCI,
            ],
            "Trend" => [
                ADX,
                AdxBreakout,
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
                WilliamsAlligator,
                ChopZone,
                VolatilityStop,
                ChandeKrollStop,
            ],
            "Volatility" => [
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
                OhlcVolatility,
                ParkinsonVol,
                ZeroTrendVol,
                CommoditySelectionIndex,
                AverageDailyRange,
                BBTrend,
                UlcerIndex,
                RelativeVolatilityIndex,
                Median,
            ],
            "Volume" => [
                Volume,
                UpDownVolume,
                OnBalanceVolume,
                VolumeWeightedAvgPrice,
                AnchoredVWAP,
                VwapAutoAnchored,
                TimeWeightedAvgPrice,
                MoneyFlowIndex,
                AccumulationDistribution,
                ChaikinMoneyFlow,
                VolumeRateOfChange,
                KlingerOscillator,
                NetVolume,
                VolumePriceTrend,
                PriceVolumeTrendStudy,
                NegativeVolumeIndex,
                PositiveVolumeIndex,
                WilliamsAD,
                MarketFacilitationIndex,
                ChaikinOscillator,
                Volume24h,
                RelativeVolumeAtTime,
            ],
            "Profiles" => [
                VolProfileVisible,
                VolProfileFixed,
                VolProfileSession,
                VolProfilePeriodic,
                VolProfileAnchored,
                VolProfileAutoAnchored,
                TimePriceOpportunity,
            ],
            "Support and resistance" => [
                PivotPoints,
                PivotPointsHighLow,
                Reversal,
                ElliottWave,
                RisingWedge,
                FallingWedge,
                MissedPivots,
                KeyLevels,
                IntradayPivotPoints,
                ProjectedPivotPoints,
                WilliamsFractals,
                ZigZag,
            ],
            "Statistics" => [
                LinearRegression,
                LinearRegressionChannel,
                LinearRegressionSlope,
                TrendStrengthIndex,
                StandardError,
                StandardErrorBands,
                RankCorrelationIndex,
                Ratio,
                Spread,
                Correlation,
                RciRibbon,
                MajorityRule,
                AccumulationSwingIndex,
                Seasonality,
                LunarPhases,
                Performance,
            ],
            "Price" => [
                HigherIntervalCandles,
                BollingerBars,
                TypicalPrice,
                MedianPrice,
                WeightedClose,
                OHLC4,
                VisibleAveragePrice,
            ],
            "Market structure" => [
                Week52HighLow,
                Week52HighLowLevels,
                TradingSessions,
                Gaps,
            ],
            "Market breadth" => [
                AdvanceDeclineRatio,
                AdvanceDeclineLine,
                CumulativeVolumeIndex,
            ],
            "Patterns" => [
                CandlestickPatterns,
            ],
        );
        for kind in [
            OrderFlowKind::Spread,
            OrderFlowKind::BookSize {
                levels: 10,
                distance: 0.0,
            },
            OrderFlowKind::TradeRate {
                volume: false,
                seconds: 60,
                instant: 3,
            },
            OrderFlowKind::AggressorRate {
                seconds: 60,
                instant: 3,
            },
            OrderFlowKind::LevelsExecuted {
                seconds: 0,
                reversal: false,
            },
            OrderFlowKind::SizeDelta {
                ranges: [[0.0, 9.0], [10.0, 99.0], [100.0, f64::MAX]],
            },
            OrderFlowKind::FlowSum { bars: 10 },
            OrderFlowKind::FilteredCvd {
                min: 100.0,
                max: f64::MAX,
            },
            OrderFlowKind::LevelsTradedThrough { seconds: 0 },
        ] {
            let name = OrderFlowStudy::new(kind).name().to_owned();
            self.constructors.insert(
                name,
                (
                    "Order flow",
                    Box::new(move || Box::new(OrderFlowStudy::new(kind))),
                ),
            );
        }
        for family in PatternFamily::ALL {
            let name = ChartPatterns::new(family).name().to_owned();
            self.constructors.insert(
                name,
                (
                    Self::PATTERNS,
                    Box::new(move || Box::new(ChartPatterns::new(family))),
                ),
            );
        }
        #[cfg(feature = "drawings")]
        for kind in AutoDrawingKind::ALL {
            let name = AutoDrawing::new(kind, 5.0, 10).name().to_owned();
            self.constructors.insert(
                name,
                (
                    "Support and resistance",
                    Box::new(move || Box::new(AutoDrawing::new(kind, 5.0, 10))),
                ),
            );
        }
    }

    /// Register a named indicator constructor in the [`CUSTOM`](Self::CUSTOM) category.
    ///
    /// A constructor already registered under `name`, a built-in included, is replaced. Returns
    /// `false`, registering nothing, when `name` is empty or only whitespace.
    ///
    /// # Arguments
    /// * `name` -- The lookup key (e.g. `"MySMA"`).
    /// * `constructor` -- A closure that returns a new boxed indicator.
    pub fn register<F>(&mut self, name: impl Into<String>, constructor: F) -> bool
    where
        F: Fn() -> Box<dyn Indicator> + Send + Sync + 'static,
    {
        self.register_in(Self::CUSTOM, name, constructor)
    }

    /// Register a named indicator constructor in `category`, such as [`PERSONAL`](Self::PERSONAL)
    /// for a script the user imported, [`COMMUNITY`](Self::COMMUNITY) or
    /// [`PROFILES`](Self::PROFILES).
    ///
    /// A constructor already registered under `name`, a built-in included, is replaced. Returns
    /// `false`, registering nothing, when `name` is empty or only whitespace.
    pub fn register_in<F>(
        &mut self,
        category: &'static str,
        name: impl Into<String>,
        constructor: F,
    ) -> bool
    where
        F: Fn() -> Box<dyn Indicator> + Send + Sync + 'static,
    {
        let name = name.into();
        if name.trim().is_empty() {
            return false;
        }
        self.constructors
            .insert(name, (category, Box::new(constructor)));
        true
    }

    /// Create an indicator by its registered name, or `None` if not found.
    pub fn create(&self, name: &str) -> Option<Box<dyn Indicator>> {
        self.constructors.get(name).map(|(_, f)| f())
    }

    /// The category of the indicator registered under `name`, such as `"Momentum"` or
    /// `"Volume"` for a built-in and [`CUSTOM`](Self::CUSTOM) for one added with
    /// [`register`](Self::register), or `None` if not found.
    pub fn category(&self, name: &str) -> Option<&'static str> {
        self.constructors.get(name).map(|(category, _)| *category)
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
mod builtin_properties;
#[cfg(test)]
mod tests;
