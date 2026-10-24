//! Indicator factory for dynamic indicator creation by name.
//!
//! The [`IndicatorFactory`] provides a registry of constructor functions
//! keyed by indicator name strings. Every built-in indicator is
//! pre-registered under its canonical [`Indicator::name`] key and a category,
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
/// On construction, every built-in indicator is registered under its canonical
/// [`Indicator::name`] (e.g. `"SMA"`, `"RSI"`, `"MACD"`, `"SuperTrend"`) using
/// its conventional default parameters. Each name is a distinct study: names
/// that computed the same values as another under their defaults are not
/// registered. Additional indicators can be registered at runtime with
/// [`register`](Self::register).
pub struct IndicatorFactory {
    constructors: HashMap<String, (&'static str, IndicatorConstructor)>,
}

impl IndicatorFactory {
    /// The category of every indicator added with [`register`](Self::register).
    pub const CUSTOM: &'static str = "Custom";

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
                VolProfileVisible,
                VolProfileFixed,
                TimePriceOpportunity,
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
            "Support and resistance" => [
                PivotPoints,
                PivotPointsHighLow,
                WilliamsFractals,
                ZigZag,
            ],
            "Statistics" => [
                LinearRegression,
                LinearRegressionChannel,
                LinearRegressionSlope,
                StandardError,
                StandardErrorBands,
                RankCorrelationIndex,
                RciRibbon,
                MajorityRule,
                AccumulationSwingIndex,
                Seasonality,
            ],
            "Price" => [
                HigherIntervalCandles,
                BollingerBars,
                TypicalPrice,
                MedianPrice,
                WeightedClose,
                OHLC4,
            ],
            "Market structure" => [
                Week52HighLow,
                Week52HighLowLevels,
                TradingSessions,
                Gaps,
            ],
            "Patterns" => [
                CandlestickPatterns,
            ],
        );
    }

    /// Register a named indicator constructor in the [`CUSTOM`](Self::CUSTOM) category.
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
        self.constructors
            .insert(name.into(), (Self::CUSTOM, Box::new(constructor)));
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
mod tests {
    use super::*;
    use crate::model::{Bar, TimezoneId};
    use chrono::{Duration, TimeZone, Utc};

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

    /// Confirming the settings dialog without an edit hands each plot's colour back through
    /// `set_colors`, which must leave every drawn colour as it was.
    #[test]
    fn unchanged_settings_keep_every_colour() {
        use crate::studies::ConfiguredStudy;

        let bars: Vec<Bar> = (0..300)
            .map(|i| {
                let c = 100.0 + 10.0 * (i as f64 / 9.0).sin();
                let time = Utc.timestamp_opt(i * 3600, 0).unwrap();
                Bar::new(time, c - 0.5, c + 1.0, c - 1.0, c, 1000.0 + i as f64)
            })
            .collect();
        let colours = |study: &ConfiguredStudy| -> Vec<_> {
            (0..study.line_cnt())
                .flat_map(|line| {
                    let style = study.plot_style(line).color;
                    (0..bars.len()).map(move |bar| study.plot_color(line, bar).unwrap_or(style))
                })
                .collect()
        };
        for name in IndicatorFactory::new().list() {
            let mut study = ConfiguredStudy::new(IndicatorFactory::new().create(&name).unwrap());
            study.calculate(&bars);
            let before = colours(&study);
            study.apply_settings(&[], study.settings()).unwrap();
            study.calculate(&bars);
            assert_eq!(colours(&study), before, "{name}");
        }
    }

    /// Each registered name is a study of its own: on one series, no two names give the same
    /// values on every bar, even with their lines in another order.
    #[test]
    fn no_two_names_compute_the_same_study() {
        use crate::model::Bar;
        use chrono::{TimeZone, Utc};

        // Two bars per session over more than 52 weeks distinguish session and calendar studies;
        // 800 bars warm up the longest study, and a step every 160 bars opens a price gap.
        let bars: Vec<Bar> = (0..800)
            .map(|i: i64| {
                let x = i as f64;
                let close = 100.0
                    + (x * 0.13).sin() * 8.0
                    + (x * 0.029).cos() * 5.0
                    + (i / 160) as f64 * 6.0;
                let open = close - (x * 0.7).sin() * 1.5;
                Bar {
                    time: Utc.timestamp_opt((i / 2 * 48 + i % 2) * 3_600, 0).unwrap(),
                    open,
                    high: close.max(open) + 0.5 + (x * 0.3).sin().abs(),
                    low: close.min(open) - 0.5 - (x * 0.5).cos().abs(),
                    close,
                    volume: 1_000.0 + ((i * 37) % 211) as f64 * 9.0,
                    colors: None,
                    payload: None,
                }
            })
            .collect();
        let factory = IndicatorFactory::new();
        let mut names = factory.list();
        names.sort();
        let mut seen = HashMap::new();
        let mut duplicates = Vec::new();
        for name in names {
            let mut study = factory.create(&name).unwrap();
            study.calculate(&bars);
            let lines: Vec<Vec<String>> = study
                .values()
                .to_rows()
                .iter()
                .map(|v| {
                    let mut xs = match v {
                        IndicatorValue::Single(x) => vec![*x],
                        IndicatorValue::Multiple(xs) => xs.clone(),
                        IndicatorValue::None => Vec::new(),
                    };
                    xs.sort_by(f64::total_cmp);
                    xs.iter().map(|x| format!("{x:.9e}")).collect()
                })
                .collect();
            if let Some(first) = seen.insert(lines, name.clone()) {
                duplicates.push(format!("{first} and {name}"));
            }
        }
        assert!(duplicates.is_empty(), "same values: {duplicates:?}");
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
            Box::new(MixedMACross::new(l(0), l(1))),
            Box::new(TripleMA::new(l(0), l(1), l(2))),
            Box::new(MARibbon::new()),
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
            Box::new(PercentageVolumeOscillator::new(l(0), l(1), l(2))),
            Box::new(VolumeOscillator::new(l(0), l(1))),
            Box::new(BalanceOfPower::new(l(0))),
            Box::new(EaseOfMovement::new(l(0)).with_divisor(x(1e4))),
            Box::new(PriceMomentumOscillator::new(l(0), l(1), l(2))),
            Box::new(PringsSpecialK::new(l(0), l(1))),
            Box::new(SMIErgodicIndicator::new(l(0), l(1), l(2))),
            Box::new(SMIErgodicOscillator::new(l(0), l(1), l(2))),
            Box::new(WoodiesCCI::new(l(0), l(1))),
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
            Box::new(WilliamsAlligator::new(l(0), l(1), l(2))),
            Box::new(ChopZone::new()),
            Box::new(VolatilityStop::new(l(0), x(2.0))),
            Box::new(ChandeKrollStop::new(l(0), x(1.0), l(1))),
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
            Box::new(ParkinsonVol::new(l(0))),
            Box::new(ZeroTrendVol::new(l(0))),
            Box::new(RelativeVolatilityIndex::new(l(0))),
            Box::new(
                CommoditySelectionIndex::new(l(0))
                    .with_point_value(x(1.0))
                    .with_margin(x(1.0))
                    .with_commission(x(1.0)),
            ),
            Box::new(Median::new(l(0), l(1), x(2.0))),
            Box::new(Volume::new(l(0))),
            Box::new(OnBalanceVolume::new()),
            Box::new(VolumeWeightedAvgPrice::new().with_std_dev_bands([x(1.0), x(2.0)])),
            Box::new(AnchoredVWAP::new(0)),
            Box::new(VwapAutoAnchored::new().with_length(l(0))),
            Box::new(TimeWeightedAvgPrice::new()),
            Box::new(MoneyFlowIndex::new(l(0))),
            Box::new(AccumulationDistribution::new()),
            Box::new(ChaikinMoneyFlow::new(l(0))),
            Box::new(VolumeRateOfChange::new(l(0))),
            Box::new(KlingerOscillator::new().with_periods(l(0), l(1), l(2))),
            Box::new(CVD::new()),
            Box::new(VolProfileVisible::new(l(0))),
            Box::new(VolProfileFixed::new(l(0))),
            Box::new(TimePriceOpportunity::default()),
            Box::new(NetVolume::new()),
            Box::new(VolumePriceTrend::new()),
            Box::new(PriceVolumeTrendStudy::new(l(0))),
            Box::new(NegativeVolumeIndex::new()),
            Box::new(PositiveVolumeIndex::new()),
            Box::new(WilliamsAD::new()),
            Box::new(MarketFacilitationIndex::new().with_divisor(x(1.0))),
            Box::new(PivotPoints::new().with_pivots_back(l(0))),
            Box::new(Volume24h::new()),
            Box::new(RelativeVolumeAtTime::new(l(0))),
            Box::new(ZigZag::new(x(5.0), l(0))),
            Box::new(PivotPointsHighLow::new(l(0), l(1), l(2), l(3))),
            Box::new(WilliamsFractals::new(l(0))),
            Box::new(Gaps::new(l(0), x(30.0)).with_trail_limit(Some(l(1)))),
            Box::new(RsiDivergence::new(l(0), l(1), l(2), l(3), 60)),
            Box::new(PriceChannel::new(l(0))),
            Box::new(LinearRegression::new(l(0))),
            Box::new(LinearRegressionChannel::new(l(0), x(2.0))),
            Box::new(LinearRegressionSlope::new(l(0))),
            Box::new(StandardError::new(l(0))),
            Box::new(StandardErrorBands::new(l(0), x(2.0))),
            Box::new(RankCorrelationIndex::new(l(0))),
            Box::new(RciRibbon::new(l(0), l(1), l(2))),
            Box::new(MajorityRule::new(l(0))),
            Box::new(AccumulationSwingIndex::new(x(0.5))),
            Box::new(Seasonality::new()),
            Box::new(HigherIntervalCandles::default()),
            Box::new(BollingerBars::new()),
            Box::new(TypicalPrice::new()),
            Box::new(MedianPrice::new()),
            Box::new(WeightedClose::new()),
            Box::new(HL2::new()),
            Box::new(HLC3::new()),
            Box::new(OHLC4::new()),
            Box::new(Week52HighLow::new()),
            Box::new(Week52HighLowLevels::new()),
            Box::new(AcceleratorOscillator::new()),
            Box::new(AroonOscillator::new(l(0))),
            Box::new(AverageDailyRange::new(l(0))),
            Box::new(BBTrend::new(l(0), l(1), x(2.0))),
            Box::new(ChaikinOscillator::new(l(0), l(1))),
            Box::new(BullBearPower::new(l(0))),
            Box::new(AdvanceDeclineRatioBars::new(l(0))),
            Box::new(UlcerIndex::new(l(0))),
            Box::new(DetrendedPriceOscillator::new(l(0)).with_centered(true)),
            Box::new(TradingSessions::new()),
            Box::new(CandlestickPatterns::default()),
        ]
    }

    /// xorshift64*, so the event sequence is the same on every platform and run.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        /// Uniform in [0, 1).
        fn unit(&mut self) -> f64 {
            (self.next() >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn bar(rng: &mut Rng, time: chrono::DateTime<Utc>, prev_close: f64) -> Bar {
        let close = prev_close + rng.unit() - 0.5;
        let mut bar = Bar {
            time,
            open: prev_close,
            high: prev_close.max(close) + rng.unit() * 0.2,
            low: prev_close.min(close) - rng.unit() * 0.2,
            close,
            volume: 100.0 + rng.unit() * 900.0,
            colors: None,
            payload: None,
        };
        if rng.unit() < 0.01 {
            bar.close = f64::NAN;
        }
        if rng.unit() < 0.01 {
            bar.high = f64::INFINITY;
        }
        if rng.unit() < 0.01 {
            bar.volume = f64::NAN;
        }
        bar
    }

    /// Each value's variant and bits, so -0.0 differs from 0.0 and NaN equals itself.
    fn bits(values: &[IndicatorValue]) -> Vec<(u8, Vec<u64>)> {
        values
            .iter()
            .map(|value| match value {
                IndicatorValue::None => (0, Vec::new()),
                IndicatorValue::Single(x) => (1, vec![x.to_bits()]),
                IndicatorValue::Multiple(xs) => (2, xs.iter().map(|x| x.to_bits()).collect()),
            })
            .collect()
    }

    /// `update(data, from)` leaves the values of a fresh `calculate(data)`, bit for bit, for every
    /// built-in after each change a chart passes on: a new bar, a new close of the last bar, a
    /// correction up to five bars back, a removed last bar, bars loaded in front, a daily series
    /// turning intraday, and bars with a non-finite price or volume. A checkpoint not restored on
    /// a last-bar update, a correction resumed instead of restarted, a window read one bar short
    /// or a warm-up threshold or VWAP's daily rule not rechecked fails it.
    #[test]
    fn every_update_equals_a_fresh_calculate() {
        // In UTC no VWAP value depends on its daily rule; in Tokyo with weekly periods the
        // daily and intraday trading days group the bars differently.
        let tokyo_week = || -> Box<dyn Indicator> {
            Box::new(
                VolumeWeightedAvgPrice::new()
                    .with_anchor(VwapAnchor::Week)
                    .with_time_zone(TimezoneId::named("Asia/Tokyo").unwrap()),
            )
        };
        let mut studies = builtins_with(|k| 3 + 2 * k, |v| v);
        studies.push(tokyo_week());
        let mut twins = builtins_with(|k| 3 + 2 * k, |v| v);
        twins.push(tokyo_week());
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut data = Vec::<Bar>::new();
        for event in 0..200 {
            let u = if event < 80 { 0.7 } else { rng.unit() };
            let len = data.len();
            let from = if u < 0.65 && len > 0 {
                data[len - 1].close = data[len - 1].open + rng.unit() - 0.5;
                len - 1
            } else if (0.85..0.93).contains(&u) && len >= 7 {
                let at = len - 2 - (rng.next() % 5) as usize;
                data[at] = bar(&mut rng, data[at].time, data[at].open);
                at
            } else if (0.93..0.965).contains(&u) && len > 0 {
                data.pop();
                len - 1
            } else if u >= 0.965 && len > 0 {
                for _ in 0..1 + rng.next() % 3 {
                    let b = bar(&mut rng, data[0].time - Duration::days(1), data[0].open);
                    data.insert(0, b);
                }
                0
            } else {
                let time =
                    data.last()
                        .map_or(Utc.with_ymd_and_hms(2023, 9, 15, 23, 0, 0).unwrap(), |b| {
                            b.time
                                + if len < 60 {
                                    Duration::days(1)
                                } else {
                                    Duration::minutes(1)
                                }
                        });
                let close =
                    data.last().map_or(
                        100.0,
                        |b| if b.close.is_finite() { b.close } else { b.open },
                    );
                data.push(bar(&mut rng, time, close));
                len
            };
            for (study, twin) in studies.iter_mut().zip(&twins) {
                study.update(&data, from);
                let mut fresh = twin.clone_box();
                fresh.calculate(&data);
                assert_eq!(
                    bits(&study.values().to_rows()),
                    bits(&fresh.values().to_rows()),
                    "{} event {event}",
                    study.name()
                );
            }
        }
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
                colors: None,
                payload: None,
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
        let covered: Vec<String> = builtins_with(|_| 1, |v| v)
            .iter()
            .map(|s| s.name().to_string())
            .collect();
        let missing: Vec<String> = factory
            .list()
            .into_iter()
            .filter(|name| !covered.contains(name))
            .collect();
        assert!(
            missing.is_empty(),
            "the table must cover every built-in: {missing:?}"
        );

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

        // Every value has one entry per declared line and every line a name; a study of
        // several lines never emits `Single`. The volume profiles emit a histogram per bar
        // instead of lines (#167).
        let fits = |v: &IndicatorValue, lines: usize| match v {
            IndicatorValue::Single(_) => lines == 1,
            IndicatorValue::Multiple(xs) => xs.len() == lines,
            IndicatorValue::None => true,
        };
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
                        Ok(_) if study.name().starts_with("Vol Profile") => {}
                        Ok(_) => {
                            let lines = study.line_cnt();
                            if study.line_names().len() != lines {
                                failures.push(format!(
                                    "{}: {lines} lines with names {:?}",
                                    study.name(),
                                    study.line_names()
                                ));
                            } else if let Some(v) =
                                study.values().to_rows().iter().find(|v| !fits(v, lines))
                            {
                                failures.push(format!(
                                    "{}{n:?} on {label}: {v:?} for {lines} lines",
                                    study.name()
                                ));
                            }
                        }
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
                && format!("{:?}", zero.values().to_rows())
                    != format!("{:?}", one.values().to_rows())
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
                if format!("{:?}", study.values().to_rows())
                    != format!("{:?}", clamped.values().to_rows())
                {
                    failures.push(format!(
                        "{} with float parameters {label} not clamped",
                        study.name()
                    ));
                }
            }
        }
        // Counted per bar: SuperTrend and PSAR split one series over two lines by trend,
        // and the trend may differ at 0.0.
        let finite = |v: &IndicatorValue| match v {
            IndicatorValue::Single(x) => usize::from(x.is_finite()),
            IndicatorValue::Multiple(xs) => xs.iter().filter(|x| x.is_finite()).count(),
            IndicatorValue::None => 0,
        };
        // A Zig Zag deviation of 0 filters nothing, so it has no values by design.
        for (study, conventional) in bottom
            .iter()
            .zip(&conventional)
            .filter(|(study, _)| study.name() != "ZigZag")
        {
            let lost = study
                .values()
                .to_rows()
                .iter()
                .zip(conventional.values().to_rows())
                .any(|(b, c)| finite(b) < finite(&c));
            if lost {
                failures.push(format!(
                    "{} with float parameters at 0.0 loses values",
                    study.name()
                ));
            }
        }

        // The `Indicator` policy: after one non-finite bar every later value equals the value
        // for the series without that bar, within 1e-9 relative, whichever field is bad and
        // wherever the bar sits (inside a warm-up window or after it). The series has the close
        // off the middle of the range, gaps between bars and a varying volume, which the range-midpoint series
        // above hides from studies that weigh the close position or the volume. Lengths differ
        // per slot so that fast, slow and signal lengths are not equal. 600 bars keep every
        // window at the last bar clear of the bad bar.
        let real = |i: i64| {
            let x = i as f64;
            let c = 100.0
                + (x * 0.17).sin() * 6.0
                + (x * 0.031).cos() * 9.0
                + x * 0.02
                + ((i * i * 31 + 7 * i) % 17) as f64 * 0.8;
            let o = c - (x * 0.7).sin() * 1.3;
            Bar {
                time: Utc.timestamp_opt(i * 60, 0).unwrap(),
                open: o,
                high: c.max(o) + 1.0 + (x * 0.3).sin().abs(),
                low: c.min(o) - 1.0 - (x * 0.5).cos().abs(),
                close: c,
                volume: 1000.0 + ((i * 37) % 211) as f64 * 9.0,
                colors: None,
                payload: None,
            }
        };
        let long: Vec<Bar> = (0..600).map(real).collect();
        type Edit = fn(&mut Bar);
        let edits: [(&str, Edit); 10] = [
            ("NaN prices", |b| {
                (b.high, b.low, b.close) = (f64::NAN, f64::NAN, f64::NAN)
            }),
            ("NaN high", |b| b.high = f64::NAN),
            ("NaN low", |b| b.low = f64::NAN),
            ("NaN close", |b| b.close = f64::NAN),
            ("infinite high", |b| b.high = f64::INFINITY),
            ("negative infinite low", |b| b.low = f64::NEG_INFINITY),
            ("infinite close", |b| b.close = f64::INFINITY),
            ("NaN volume", |b| b.volume = f64::NAN),
            ("infinite volume", |b| b.volume = f64::INFINITY),
            ("NaN open", |b| b.open = f64::NAN),
        ];
        let values = |study: &mut Box<dyn Indicator>, bars: &[Bar]| {
            catch_unwind(AssertUnwindSafe(|| study.calculate(bars))).ok()?;
            Some(study.values().to_rows())
        };
        let parts = |v: &IndicatorValue| match v {
            IndicatorValue::Single(x) => vec![*x],
            IndicatorValue::Multiple(xs) => xs.clone(),
            IndicatorValue::None => Vec::new(),
        };
        let blank = |v: &IndicatorValue| parts(v).iter().all(|x| !x.is_finite());
        // A finite component must match the reference. A window study may leave the windows
        // holding the bad bar without a value (`near`, no study looks back further than 250
        // bars), but later every component the reference has must be there.
        let agrees = |hostile: &IndicatorValue, reference: &IndicatorValue, near: bool| {
            let (hostile, reference) = (parts(hostile), parts(reference));
            let right = |h: f64, r: f64| h.is_finite() && (h - r).abs() <= 1e-9 * r.abs().max(1.0);
            hostile
                .iter()
                .enumerate()
                .all(|(k, &h)| !h.is_finite() || reference.get(k).is_some_and(|&r| right(h, r)))
                && reference.iter().enumerate().all(|(k, &r)| {
                    !r.is_finite() || near || hostile.get(k).is_some_and(|&h| right(h, r))
                })
        };
        for mut study in builtins_with(|k| 12 + 3 * k, |v| v) {
            let Some(full) = values(&mut study, &long) else {
                continue;
            };
            for at in [5, 100] {
                let mut without = long.clone();
                without.remove(at);
                let Some(clean) = values(&mut study, &without) else {
                    continue;
                };
                for (label, edit) in edits {
                    let mut bars = long.clone();
                    edit(&mut bars[at]);
                    let hostile = values(&mut study, &bars).unwrap_or_default();
                    // A study that reads the bad field skips the bar and matches the series
                    // without it; one that does not read it sees an ordinary bar and matches
                    // the full series.
                    let first_wrong = |reference: &[IndicatorValue], shift: usize| {
                        (at + 1..bars.len()).find(|&i| {
                            !hostile
                                .get(i)
                                .is_some_and(|h| agrees(h, &reference[i - shift], i <= at + 250))
                        })
                    };
                    let (skipped, seen) = (first_wrong(&clean, 1), first_wrong(&full, 0));
                    if let (Some(i), Some(_)) = (skipped, seen) {
                        failures.push(format!(
                            "{} differs from the series without a bar with {label} at {at} \
                             at bar {i}: {:?} against {:?}",
                            study.name(),
                            hostile.get(i),
                            clean[i - 1]
                        ));
                    } else if skipped.is_none()
                        && seen.is_some()
                        && !hostile.get(at).is_some_and(blank)
                    {
                        failures.push(format!(
                            "{} gives {:?} on its bad bar with {label} at {at}",
                            study.name(),
                            hostile.get(at)
                        ));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
