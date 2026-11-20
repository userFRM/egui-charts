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
//!
//! // Saved charts keep the factory key apart from the display name.
//! let study = factory.create("MySMA").unwrap();
//! assert_eq!((study.name(), study.registered_name()), ("SMA", "MySMA"));
//! ```

use crate::data::OrderFlowKind;
use crate::studies::*;
use std::collections::{HashMap, HashSet};

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
    (@help $factory:expr, $kind:literal, $($ty:ty),+ $(,)?) => {
        $($factory.help_urls.insert(
            std::any::type_name::<$ty>(),
            concat!("https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/", $kind, ".", stringify!($ty), ".html"),
        );)+
    };
    ($factory:expr, $($category:literal => [$($ty:ty),+ $(,)?]),+ $(,)?) => {
        $($(
            // Read the canonical name from a throwaway default instance so the
            // key is always exactly what the indicator reports at runtime.
            let key = <$ty as Default>::default().name().to_string();
            register_builtins!(@help $factory, "struct", $ty);
            $factory.constructors.insert(
                key,
                (Some($category), Box::new(|| Box::new(<$ty as Default>::default()))),
            );
        )+)+
    };
}

/// A built-in study's earlier factory key, its key now and its constructor.
type Renamed = (&'static str, &'static str, fn() -> Box<dyn Indicator>);

/// Earlier built-in keys kept for saved studies, stored drawing panes and browser favourites.
const RENAMED: [Renamed; 1] = [("VPT", "PVT", || Box::new(VolumePriceTrend::default()))];

/// A factory that creates [`Indicator`] instances by name.
///
/// On construction, catalogue studies are registered under their canonical
/// [`Indicator::name`] (e.g. `"SMA"`, `"RSI"`, `"MACD"`, `"SuperTrend"`) using
/// its conventional default parameters. Additional indicators can be registered at runtime
/// with [`register`](Self::register).
///
/// [`create`](Self::create) and [`has`](Self::has) also resolve the names of five exported
/// studies the catalogue leaves out, so that a template, a copied study or a saved layout of
/// one is recreated: `CVD`, `HL2`, `HLC3`, `Ehlers Fisher` and `MA Combination`, which creates
/// [`MACombination::ribbon`] for its inputs to reshape. They also resolve `VPT`, the earlier
/// name of Price Volume Trend (`PVT`), which a saved study restores under. They have no category
/// and are not in [`list`](Self::list) or [`count`](Self::count).
pub struct IndicatorFactory {
    /// The category of each name, `None` for a built-in outside the catalogue, and its constructor.
    constructors: HashMap<String, (Option<&'static str>, IndicatorConstructor)>,
    help_urls: HashMap<&'static str, &'static str>,
    defaults: HashMap<String, StudyTemplate>,
    /// The names the host registered, a built-in's name included once the host replaces it.
    hosts: HashSet<String>,
}

impl std::fmt::Debug for IndicatorFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndicatorFactory")
            .field("constructors", &self.constructors.keys())
            .finish_non_exhaustive()
    }
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
    /// The category of studies computed from trades, quotes or the order book.
    pub const ORDER_FLOW: &'static str = "Order flow";

    /// Create a factory with the built-in catalogue enabled by the current features.
    pub fn new() -> Self {
        let mut factory = Self {
            constructors: HashMap::new(),
            help_urls: HashMap::new(),
            defaults: HashMap::new(),
            hosts: HashSet::new(),
        };

        factory.register_builtin();
        factory
    }

    /// Register every built-in indicator under its canonical name.
    ///
    /// Types use their defaults; automatic drawings select a construction kind and
    /// stream studies select aggregation parameters, and each candlestick pattern is also
    /// registered alone under its label. Names come from each study.
    /// Exported studies outside the catalogue are registered first, without a category, so a
    /// listed study of the same name would replace them.
    fn register_builtin(&mut self) {
        // Exported studies outside the catalogue, resolved by name so that saved copies load.
        register_builtins!(@help self, "struct", CVD, HL2, HLC3, EhlersFisher, MACombination, OrderFlowStudy, ChartPatterns, OptionsVolumeProfile);
        #[cfg(feature = "drawings")]
        register_builtins!(@help self, "struct", AutoDrawing, AutoTrendLines);
        let unlisted: [fn() -> Box<dyn Indicator>; 5] = [
            || Box::new(CVD::default()),
            || Box::new(HL2::default()),
            || Box::new(HLC3::default()),
            || Box::new(EhlersFisher::default()),
            || Box::new(MACombination::ribbon()),
        ];
        for constructor in unlisted {
            self.constructors.insert(
                constructor().name().to_owned(),
                (None, Box::new(constructor)),
            );
        }
        // Earlier names resolve without being listed, so that saves made under them load.
        for (earlier, _, constructor) in RENAMED {
            self.constructors
                .insert(earlier.to_owned(), (None, Box::new(constructor)));
        }
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
                AdvanceDecline,
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
                PriceChannel,
                StandardDeviation,
                HistoricalVolatility,
                CloseToCloseVol,
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
                VolatilityIndex,
            ],
            "Volume" => [
                Volume,
                UpDownVolume,
                VolumeDelta,
                CumulativeVolumeDelta,
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
                VolProfileSessionHd,
                VolProfilePeriodic,
                VolProfileAnchored,
                VolProfileAutoAnchored,
                TimePriceOpportunity,
            ],
            "Support and resistance" => [
                PivotPoints,
                PivotPointsHighLow,
                Reversal,
                BookerReversal,
                ElliottWave,
                RisingWedge,
                FallingWedge,
                MissedPivots,
                KeyLevels,
                AutoKeyLevels,
                IntradayPivotPoints,
                ProjectedPivotPoints,
                ZivGhostPivots,
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
                CorrelationLog,
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
                AnalystPriceForecast,
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
        // Individual patterns are found and saved under the same labels as their toggles.
        for index in 0..CandlestickPatterns::PATTERN_COUNT {
            self.constructors.insert(
                CandlestickPatterns::only(index).name().to_owned(),
                (
                    Some(Self::PATTERNS),
                    Box::new(move || Box::new(CandlestickPatterns::only(index))),
                ),
            );
        }
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
            OrderFlowKind::BookBalance {
                near_levels: 5,
                far_levels: 10,
                near_distance: 0.0,
                far_distance: 0.0,
            },
            OrderFlowKind::PriceRange { seconds: 60 },
        ] {
            let name = OrderFlowStudy::new(kind).name().to_owned();
            self.constructors.insert(
                name,
                (
                    Some(Self::ORDER_FLOW),
                    Box::new(move || Box::new(OrderFlowStudy::new(kind))),
                ),
            );
        }
        register_builtins!(@help self, "type", SMIErgodicIndicator, SMIErgodicOscillator, RisingWedge, FallingWedge, Ratio, Spread, Correlation, CorrelationLog, AdvanceDeclineRatio, AdvanceDeclineLine, CumulativeVolumeIndex);
        for cumulative in [false, true] {
            let name = OrderFlowStudy::execution_delta(cumulative)
                .name()
                .to_owned();
            self.constructors.insert(
                name,
                (
                    Some(Self::ORDER_FLOW),
                    Box::new(move || Box::new(OrderFlowStudy::execution_delta(cumulative))),
                ),
            );
        }
        for family in PatternFamily::ALL {
            let name = ChartPatterns::new(family).name().to_owned();
            self.constructors.insert(
                name,
                (
                    Some(Self::PATTERNS),
                    Box::new(move || Box::new(ChartPatterns::new(family))),
                ),
            );
        }
        self.constructors.insert(
            ChartPatterns::all().name().to_owned(),
            (
                Some(Self::PATTERNS),
                Box::new(|| Box::new(ChartPatterns::all())),
            ),
        );
        for &kind in PatternFamily::ALL.iter().flat_map(|family| family.kinds()) {
            self.constructors.insert(
                ChartPatterns::only(kind).name().to_owned(),
                (
                    Some(Self::PATTERNS),
                    Box::new(move || Box::new(ChartPatterns::only(kind))),
                ),
            );
        }
        #[cfg(feature = "drawings")]
        for kind in AutoDrawingKind::ALL {
            let name = AutoDrawing::new(kind, 5.0, 10).name().to_owned();
            self.constructors.insert(
                name,
                (
                    Some("Support and resistance"),
                    Box::new(move || Box::new(AutoDrawing::catalogue(kind))),
                ),
            );
        }
        #[cfg(feature = "drawings")]
        self.constructors.insert(
            AutoTrendLines::new().name().to_owned(),
            (
                Some("Support and resistance"),
                Box::new(|| Box::new(AutoTrendLines::new())),
            ),
        );
    }

    /// The name a built-in study saved as `saved` has now, or `None` when it was not renamed.
    pub(crate) fn renamed(saved: &str) -> Option<&'static str> {
        RENAMED
            .iter()
            .find(|(earlier, ..)| *earlier == saved)
            .map(|(_, now, _)| *now)
    }

    /// Register a named indicator constructor in the [`CUSTOM`](Self::CUSTOM) category.
    ///
    /// A constructor already registered under `name`, a built-in included, is replaced. Returns
    /// `false`, registering nothing, when `name` is empty or only whitespace. Created studies
    /// are saved under `name`, as [`create`](Self::create) describes.
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
    /// [`PROFILES`](Self::PROFILES) or [`ORDER_FLOW`](Self::ORDER_FLOW).
    ///
    /// A constructor already registered under `name`, a built-in included, is replaced. Returns
    /// `false`, registering nothing, when `name` is empty or only whitespace. Created studies
    /// are saved under `name`, as [`create`](Self::create) describes.
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
        self.defaults.remove(&name);
        self.hosts.insert(name.clone());
        self.constructors
            .insert(name, (Some(category), Box::new(constructor)));
        true
    }

    /// Whether the host registered `name`, so that it resolves to a study whose saved templates
    /// never went through the changes of a built-in of that name.
    pub(crate) fn is_host(&self, name: &str) -> bool {
        self.hosts.contains(name)
    }

    /// Set the inputs and presentation used by subsequent [`create`](Self::create) calls.
    /// Each template replaces the defaults for its registered name; other names retain theirs.
    /// An empty list changes nothing, and the last template for a repeated name wins.
    /// Defaults saved under earlier input ids are upgraded like saved templates.
    /// Invalid names, inputs or settings reject the entire batch without changing any defaults.
    /// Existing studies, saved templates, layouts, clipboard copies and alert studies keep their
    /// own settings.
    /// Replacing or removing a constructor discards its defaults.
    ///
    /// Capture a study with [`StudyTemplate::from_study`], edit its typed inputs or settings,
    /// then pass it here. A chooser supplied this factory through `IndicatorDialog::with_factory`
    /// uses the same defaults. Explicit edits to a created study take precedence.
    /// A study this factory creates, also from a saved template with
    /// [`StudyTemplate::create_with`], reports the value a new study starts with as the
    /// [`default`](StudyInput::default) of each input that these defaults or that template name,
    /// which resetting the inputs restores.
    ///
    /// ```
    /// use egui_charts::studies::{IndicatorFactory, InputValue, StudyTemplate};
    /// let mut factory = IndicatorFactory::new();
    /// let mut defaults = StudyTemplate::from_study(factory.create("SMA").unwrap().as_ref());
    /// defaults.inputs = vec![("period".into(), InputValue::Length(5))];
    /// defaults.settings.plots[0].width = 3.0;
    /// factory.set_defaults(&[defaults])?;
    /// assert_eq!(factory.create("SMA").unwrap().plot_style(0).width, 3.0);
    /// # Ok::<(), egui_charts::studies::StudyError>(())
    /// ```
    pub fn set_defaults(&mut self, defaults: &[StudyTemplate]) -> Result<(), StudyError> {
        for template in defaults {
            template.create_with(self)?;
        }
        for template in defaults {
            self.defaults
                .insert(template.name.clone(), template.clone());
        }
        Ok(())
    }

    /// Discard the host defaults for `name`. Returns whether defaults were removed.
    /// Subsequent creations use the registered constructor's values again.
    pub fn clear_defaults(&mut self, name: &str) -> bool {
        self.defaults.remove(name).is_some()
    }

    /// Create an indicator by its registered name with the host's defaults, or `None` if not
    /// found or if the constructor no longer accepts those defaults. Without host defaults,
    /// returns the constructor's study.
    ///
    /// The study's [`Indicator::registered_name`] is `name`. Saved charts, study templates,
    /// clipboard copies and alerts use it to recreate the study through this factory, even
    /// when its display [`Indicator::name`] differs. A study that cannot retain the key itself
    /// is returned in a [`ConfiguredStudy`], as a chart holds it.
    pub fn create(&self, name: &str) -> Option<Box<dyn Indicator>> {
        if let Some(template) = self.defaults.get(name) {
            return template.create_with(self).ok();
        }
        let mut study = self.create_registered(name)?;
        if study.registered_name() != name && !study.set_registered_name(name) {
            study = Box::new(ConfiguredStudy::new(study));
            study.set_registered_name(name);
        }
        Some(study)
    }

    /// The inputs a study created under `name` starts with, or `None` without host defaults for
    /// `name` or when its constructor no longer accepts them.
    pub(crate) fn input_defaults(&self, name: &str) -> Option<Vec<(String, InputValue)>> {
        let study = self.defaults.get(name)?.build(self, false).ok()?;
        Some(
            study
                .inputs()
                .into_iter()
                .map(|input| (input.id, input.value))
                .collect(),
        )
    }

    /// Reconstruct saved state without applying defaults intended for new studies.
    pub(crate) fn create_registered(&self, name: &str) -> Option<Box<dyn Indicator>> {
        self.constructors.get(name).map(|(_, f)| f())
    }

    /// The category of the indicator registered under `name`, such as `"Momentum"` or
    /// `"Volume"` for a built-in and [`CUSTOM`](Self::CUSTOM) for one added with
    /// [`register`](Self::register), or `None` if not found or left out of the catalogue.
    pub fn category(&self, name: &str) -> Option<&'static str> {
        self.constructors
            .get(name)
            .and_then(|(category, _)| *category)
    }

    /// The names a study picker offers, unordered: every registered name except the exported
    /// studies outside the catalogue.
    pub fn list(&self) -> Vec<String> {
        self.constructors
            .iter()
            .filter(|(_, (category, _))| category.is_some())
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Whether [`create`](Self::create) resolves `name`, also one that [`list`](Self::list)
    /// leaves out.
    pub fn has(&self, name: &str) -> bool {
        self.constructors.contains_key(name)
    }

    /// Remove the indicator registered under `name`.
    ///
    /// Returns `true` if it was present and removed, `false` otherwise.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.defaults.remove(name);
        self.hosts.remove(name);
        self.constructors.remove(name).is_some()
    }

    /// Remove all registered indicators, including built-ins.
    pub fn clear(&mut self) {
        self.defaults.clear();
        self.hosts.clear();
        self.constructors.clear();
    }

    /// The number of names in [`list`](Self::list).
    pub fn count(&self) -> usize {
        self.constructors
            .values()
            .filter(|(category, _)| category.is_some())
            .count()
    }
}

impl Default for IndicatorFactory {
    fn default() -> Self {
        Self::new()
    }
}

// Resolve the built-in articles once so direct constructors retain the same help as the chooser.
pub(crate) fn builtin_help_url(study_type: &str) -> Option<&'static str> {
    static CATALOGUE: std::sync::OnceLock<HashMap<&'static str, &'static str>> =
        std::sync::OnceLock::new();
    CATALOGUE
        .get_or_init(|| IndicatorFactory::new().help_urls)
        .get(study_type)
        .copied()
}

#[cfg(test)]
mod builtin_properties;
#[cfg(test)]
mod tests;
