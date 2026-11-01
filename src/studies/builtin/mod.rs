//! Built-in technical indicators.
//!
//! This module contains the built-in technical indicators, grouped by
//! category below. Bar-based indicators implement the
//! [`Indicator`](super::Indicator) trait for a uniform API. Execution-based studies
//! [`trade_delta`] and [`footprint_studies`] consume [`crate::data::Footprint`] explicitly.
//!
//! # Indicator categories
//!
//! ## Moving averages
//! [`SMA`], [`EMA`], [`WMA`], [`DEMA`], [`TEMA`], [`HMA`], [`ALMA`],
//! [`VWMA`], [`LSMA`], [`SWMA`], [`KAMA`], [`SmoothedMA`], [`GuppyMA`],
//! [`McGinleyDynamic`], [`VIDYA`], [`ZLEMA`], [`HammingMA`],
//! [`MultipleMA`], [`EMACross`], [`MACross`]
//!
//! ## Momentum / oscillators
//! [`RSI`], [`MACD`], [`Stochastic`], [`StochasticRSI`], [`WilliamsR`],
//! [`CCI`], [`Momentum`], [`RateOfChange`], [`Aroon`], [`AwesomeOscillator`], [`RsiDivergence`],
//! [`TechnicalRatings`], [`KnoxvilleDivergence`],
//! [`UltimateOscillator`], [`ConnorsRSI`], [`CoppockCurve`], [`TRIX`],
//! [`ElderRay`], [`FisherTransform`], [`ForceIndex`], [`MassIndex`],
//! [`DetrendedPriceOscillator`], [`KDJ`], [`PrettyGoodOscillator`],
//! [`ChandeMomentumOscillator`], [`TrueStrengthIndex`],
//! [`StochasticMomentumIndex`], [`SchaffTrendCycle`], [`PriceOscillator`],
//! [`PercentageVolumeOscillator`], [`AcceleratorOscillator`], [`AroonOscillator`],
//! [`BullBearPower`], [`AdvanceDeclineRatioBars`], [`PriceMomentumOscillator`], [`PringsSpecialK`],
//! [`SMIErgodicIndicator`], [`SMIErgodicOscillator`], [`WoodiesCCI`]
//!
//! ## Trend
//! [`ADX`], [`AdxBreakout`], [`SuperTrend`], [`IchimokuCloud`], [`ParabolicSAR`],
//! [`DirectionalMovement`], [`VortexIndicator`], [`SqueezeMomentum`],
//! [`WilliamsAlligator`], [`ChopZone`], [`VolatilityStop`], [`ChandeKrollStop`]
//!
//! ## Volatility
//! [`BollingerBands`], [`ATR`], [`KeltnerChannels`], [`DonchianChannels`],
//! [`StandardDeviation`], [`HistoricalVolatility`], [`ChaikinVolatility`],
//! [`ChandelierExit`], [`AccelerationBands`], [`Envelopes`], [`ParkinsonVol`], [`ZeroTrendVol`],
//! [`AverageDailyRange`], [`BBTrend`], [`UlcerIndex`], [`RelativeVolatilityIndex`],
//! [`Median`]
//!
//! ## Price
//! [`BollingerBars`]
//!
//! ## Volume
//! [`OnBalanceVolume`], [`VolumeWeightedAvgPrice`], [`MoneyFlowIndex`],
//! [`AccumulationDistribution`], [`ChaikinMoneyFlow`],
//! [`VolumeRateOfChange`], [`KlingerOscillator`], [`CVD`],
//! [`VolProfileVisible`], [`VolProfileFixed`], [`ChaikinOscillator`], [`TimeWeightedAvgPrice`],
//! [`VwapAutoAnchored`], [`Volume24h`], [`RelativeVolumeAtTime`], [`VolumeDelta`],
//! [`CumulativeVolumeDelta`]
//!
//! ## Support / resistance
//! [`PivotPoints`], [`IntradayPivotPoints`], [`ProjectedPivotPoints`], [`PivotPointsHighLow`],
//! [`MissedPivots`], [`KeyLevels`], [`ElliottWave`], [`Reversal`], [`RisingWedge`], [`FallingWedge`],
//! [`WilliamsFractals`], [`Gaps`], [`ZigZag`], [`PriceChannel`]
//!
//! ## Patterns
//! [`CandlestickPatterns`], [`ChartPatterns`]
//!
//! ## Statistics
//! [`LinearRegression`], [`TrendStrengthIndex`], [`StandardError`],
//! [`RankCorrelationIndex`], [`RciRibbon`], [`MajorityRule`], [`Seasonality`], [`Ratio`], [`LunarPhases`],
//! [`Spread`], [`Correlation`]
//!
//! ## Time
//! [`TradingSessions`]

use crate::model::Bar;
mod footprint_studies;
pub use footprint_studies::{
    FootprintStudy, FootprintStudyConfig, FootprintStudyKind, FootprintStudyState,
    footprint_studies,
};
mod options_volume_profile;
pub use options_volume_profile::OptionsVolumeProfile;
mod trade_delta;
pub use trade_delta::{TradeDeltaValue, trade_delta};

mod candlestick_patterns;
pub use candlestick_patterns::{CandlestickPatterns, PatternTrend, PatternType};

mod chart_patterns;
pub use chart_patterns::{
    ChartPatterns, Formation, FormationKind, FormationStatus, PatternDirection, PatternFamily,
};

mod higher_interval_candles;
pub use higher_interval_candles::HigherIntervalCandles;

// Price overlays and their pane companion
mod bollinger_bars;
mod chande_kroll_stop;
mod chop_zone;
mod median;
mod volatility_stop;
mod williams_alligator;
pub use bollinger_bars::BollingerBars;
pub use chande_kroll_stop::ChandeKrollStop;
pub use chop_zone::ChopZone;
pub use median::Median;
pub use volatility_stop::VolatilityStop;
pub use williams_alligator::WilliamsAlligator;

mod time_price_opportunity;
pub use time_price_opportunity::TimePriceOpportunity;

mod bollinger_bands;
mod ema;
mod macd;
mod rsi;
mod sma;
mod vw_macd;

// Additional moving avgs
mod alma;
mod dema;
mod hma;
mod kama;
mod lsma;
mod swma;
mod tema;
mod vwma;
mod wma;

// Momentum indicators
mod aroon;
mod atr;
mod awesome_oscillator;
mod cci;
mod momentum;
mod parabolic_sar;
mod roc;
mod stochastic;
mod stochastic_rsi;
mod ultimate_oscillator;
mod williams_r;

// Volume indicators
mod acc_dist;
mod anchor_period;
mod cmf;
mod mfi;
mod obv;
mod twap;
mod volume;
mod volume_profile;
mod vwap;
mod vwap_auto_anchored;

// Trend indicators
mod adx;
mod adx_breakout;
mod ichimoku;
mod supertrend;

// Channel indicators
mod donchian;
mod keltner;

// Volatility indicators
mod std_dev;

// Support/Resistance indicators
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
mod auto_drawing;
mod elliott_wave;
mod gaps;
mod intraday_pivot_points;
mod key_levels;
mod knoxville_divergence;
mod lunar_phases;
mod missed_pivots;
mod pivot_points;
mod pivot_points_high_low;
mod projected_pivot_points;
mod reversal;
mod rsi_divergence;
mod swings;
mod wedge;
mod williams_fractals;
mod zigzag;

// Oscillator/Momentum indicators (additional)
mod connors_rsi;
mod coppock;
mod dpo;
mod elder_ray;
mod fisher_transform;
mod force_index;
mod mass_index;
mod trix;

// Volume/Price oscillators
mod balance_of_power;
mod ease_of_movement;
mod percentage_volume_oscillator;
mod price_oscillator;
mod volume_oscillator;
mod vroc;

// Trend/Volatility indicators (additional)
mod choppiness;
mod historical_volatility;
mod klinger;
mod schaff_trend;
mod vortex;

// Advanced indicators (third batch)
mod center_of_gravity;
mod chandelier_exit;
mod directional_movement;
mod ehlers_fisher;
mod kdj;
mod know_sure_thing;
mod market_facilitation;
mod pretty_good_oscillator;
mod rainbow_oscillator;
mod squeeze_momentum;

// Fourth batch - Statistics and Price variants
mod accumulation_swing;
mod linear_regression;
mod linear_regression_channel;
mod percent_b;
mod price_channel;
mod relative_vigor;
mod smi;
mod typical_price;
mod ultimate_momentum;

// Fifth batch - Additional momentum and volume indicators
mod acceleration_bands;
mod chaikin_volatility;
mod chande_momentum;
mod commodity_selection;
mod double_stochastic;
mod elder_impulse;
mod mcginley_dynamic;
mod trend_intensity;
mod trend_strength_index;
mod true_strength;
mod vidya;
mod volume_price_trend;
mod williams_ad;

// Sixth batch - Price calculations
mod average_price;

// Seventh batch - order-flow indicators
mod cvd;

// Eighth batch - Volume indicators
mod net_volume;
mod price_volume_trend_study;

// Eighth batch - Volatility indicators
mod close_to_close_vol;
mod ohlc_volatility;
mod parkinson_vol;
mod zero_trend_vol;

// Ninth batch - Moving average variants and cross indicators
mod envelopes;
mod guppy_ma;
mod ma_channel;
mod ma_combination;
mod ma_cross;
mod ma_ema_cross;
mod ma_hamming;
mod ma_multiple;
mod ma_presets;
mod smoothed_ma;

// Tenth batch - Statistics indicators
mod majority_rule;
mod rank_correlation;
mod standard_error;
mod standard_error_bands;

// Tenth batch - Market structure indicators
mod anchored_vwap_study;
mod week52_highlow;

// Tenth batch - Other indicators
mod bbw;

// Eleventh batch - Oscillators
mod accelerator_oscillator;
mod advance_decline_ratio_bars;
mod aroon_oscillator;
mod average_daily_range;
mod bbtrend;
mod bull_bear_power;
mod chaikin_oscillator;
mod ulcer_index;
// Standard oscillators
mod market_breadth;
mod price_momentum_oscillator;
mod rci_ribbon;
mod relative_volatility_index;
mod smi_ergodic;
mod symbol_pair;
mod technical_ratings;
pub use market_breadth::{
    AdvanceDeclineLine, AdvanceDeclineRatio, CumulativeVolumeIndex, MarketBreadth,
};
mod special_k;
mod woodies_cci;
pub use symbol_pair::{Correlation, Ratio, Spread, SymbolPair};
pub use technical_ratings::TechnicalRatings;
// Time studies
mod lower_interval;
mod performance;
mod relative_volume_at_time;
mod seasonality;
mod trading_sessions;
mod up_down_volume;
mod visible_average_price;
mod volume_24h;
mod volume_delta;

// Re-exports - Original indicators
pub use bollinger_bands::BollingerBands;
pub use ema::EMA;
pub use macd::MACD;
pub use rsi::RSI;
pub use sma::SMA;
pub use vw_macd::VolumeWeightedMACD;

// Re-exports - Additional moving avgs
pub use alma::ALMA;
pub use dema::DEMA;
pub use hma::HMA;
pub use kama::KAMA;
pub use lsma::LSMA;
pub use swma::SWMA;
pub use tema::TEMA;
pub use vwma::VWMA;
pub use wma::WMA;

// Re-exports - Momentum indicators
pub use aroon::Aroon;
pub use atr::{ATR, AtrSmoothing};
pub use awesome_oscillator::AwesomeOscillator;
pub use cci::CCI;
pub use momentum::Momentum;
pub use parabolic_sar::ParabolicSAR;
pub use roc::RateOfChange;
pub use stochastic::Stochastic;
pub use stochastic_rsi::StochasticRSI;
pub use ultimate_oscillator::UltimateOscillator;
pub use williams_r::WilliamsR;

// Re-exports - Volume indicators
pub use acc_dist::AccumulationDistribution;
pub use anchor_period::AnchorPeriod;
pub use cmf::ChaikinMoneyFlow;
pub use mfi::MoneyFlowIndex;
pub use obv::OnBalanceVolume;
pub use twap::TimeWeightedAvgPrice;
pub use volume::{Volume, VolumeAverage, VolumeColorRule};
pub(crate) use volume_profile::time_text;
pub use volume_profile::{
    ProfileInput, VolProfileAnchored, VolProfileAutoAnchored, VolProfileFixed, VolProfilePeriodic,
    VolProfileSession, VolProfileVisible,
};
pub use vwap::{VolumeWeightedAvgPrice, VwapAnchor};
pub use vwap_auto_anchored::{AutoAnchor, VwapAutoAnchored};

// Re-exports - Trend indicators
pub use adx::ADX;
pub use adx_breakout::AdxBreakout;
pub use ichimoku::IchimokuCloud;
pub use supertrend::SuperTrend;

// Re-exports - Channel indicators
pub use donchian::DonchianChannels;
pub use keltner::{KeltnerBand, KeltnerChannels};

// Re-exports - Volatility indicators
pub use std_dev::StandardDeviation;

// Re-exports - Support/Resistance indicators
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
pub use auto_drawing::{AutoDrawing, AutoDrawingKind};
pub use elliott_wave::ElliottWave;
pub use gaps::Gaps;
pub use intraday_pivot_points::IntradayPivotPoints;
pub use key_levels::KeyLevels;
pub use knoxville_divergence::KnoxvilleDivergence;
pub use lunar_phases::{LunarPhase, LunarPhases, MoonPhase, lunar_phases};
pub use missed_pivots::MissedPivots;
pub use pivot_points::{PivotLabelSide, PivotMethod, PivotPoints, PivotTimeframe};
pub use pivot_points_high_low::PivotPointsHighLow;
pub use projected_pivot_points::ProjectedPivotPoints;
pub use reversal::Reversal;
pub use rsi_divergence::RsiDivergence;
pub use wedge::{FallingWedge, RisingWedge, Wedge};
pub use williams_fractals::WilliamsFractals;
pub use zigzag::{ZigZag, ZigZagChange, ZigZagMode, ZigZagPivot};

// Re-exports - Additional Oscillator/Momentum indicators
pub use connors_rsi::ConnorsRSI;
pub use coppock::CoppockCurve;
pub use dpo::DetrendedPriceOscillator;
pub use elder_ray::ElderRay;
pub use fisher_transform::FisherTransform;
pub use force_index::ForceIndex;
pub use mass_index::MassIndex;
pub use trix::TRIX;

// Re-exports - Volume/Price oscillators
pub use balance_of_power::BalanceOfPower;
pub use ease_of_movement::EaseOfMovement;
pub use percentage_volume_oscillator::PercentageVolumeOscillator;
pub use price_oscillator::{OscillatorAverage, PriceOscillator};
pub use volume_oscillator::VolumeOscillator;
pub use vroc::VolumeRateOfChange;

// Re-exports - Trend/Volatility indicators (additional)
pub use choppiness::ChoppinessIndex;
pub use historical_volatility::HistoricalVolatility;
pub use klinger::KlingerOscillator;
pub use schaff_trend::SchaffTrendCycle;
pub use vortex::VortexIndicator;

// Re-exports - Advanced indicators (third batch)
pub use center_of_gravity::CenterOfGravity;
pub use chandelier_exit::ChandelierExit;
pub use directional_movement::DirectionalMovement;
pub use ehlers_fisher::EhlersFisher;
pub use kdj::KDJ;
pub use know_sure_thing::KnowSureThing;
pub use market_facilitation::MarketFacilitationIndex;
pub use pretty_good_oscillator::PrettyGoodOscillator;
pub use rainbow_oscillator::RainbowOscillator;
pub use squeeze_momentum::SqueezeMomentum;

// Re-exports - Fourth batch (Statistics and Price variants)
pub use accumulation_swing::AccumulationSwingIndex;
pub use linear_regression::{LinearRegression, LinearRegressionSlope};
pub use linear_regression_channel::LinearRegressionChannel;
pub use percent_b::PercentB;
pub use price_channel::PriceChannel;
pub use relative_vigor::RelativeVigorIndex;
pub use smi::StochasticMomentumIndex;
pub use trend_strength_index::TrendStrengthIndex;
pub use typical_price::{MedianPrice, TypicalPrice, WeightedClose};
pub use ultimate_momentum::UltimateMomentum;

// Re-exports - Fifth batch (Additional momentum and volume)
pub use acceleration_bands::AccelerationBands;
pub use chaikin_volatility::ChaikinVolatility;
pub use chande_momentum::ChandeMomentumOscillator;
pub use commodity_selection::{CommoditySelectionIndex, TrendDetectionIndex};
pub use double_stochastic::DoubleStochastic;
pub use elder_impulse::{ElderImpulseSystem, ImpulseSignal};
pub use mcginley_dynamic::McGinleyDynamic;
pub use trend_intensity::{Qstick, TrendIntensityIndex};
pub use true_strength::TrueStrengthIndex;
pub use vidya::{VIDYA, ZLEMA};
pub use volume_price_trend::{NegativeVolumeIndex, PositiveVolumeIndex, VolumePriceTrend};
pub use williams_ad::WilliamsAD;

// Re-exports - Sixth batch (Price calculations)
pub use average_price::{HL2, HLC3, OHLC4, TrueRange};

// Re-exports - Seventh batch (order-flow indicators)
pub use cvd::{CVD, CVDMode};

// Re-exports - Eighth batch (Volume indicators)
pub use net_volume::NetVolume;
pub use price_volume_trend_study::PriceVolumeTrendStudy;

// Re-exports - Eighth batch (Volatility indicators)
pub use close_to_close_vol::CloseToCloseVol;
pub use ohlc_volatility::OhlcVolatility;
pub use parkinson_vol::ParkinsonVol;
pub use zero_trend_vol::ZeroTrendVol;

// Re-exports - Ninth batch (Moving average variants and cross indicators)
pub use envelopes::{EnvelopeAverage, Envelopes};
pub use guppy_ma::GuppyMA;
pub use ma_channel::MAChannel;
pub use ma_combination::{AverageLine, AverageType, MACombination};
pub use ma_cross::MACross;
pub use ma_ema_cross::EMACross;
pub use ma_hamming::HammingMA;
pub use ma_multiple::MultipleMA;
pub use ma_presets::{MARibbon, MixedMACross, TripleMA};
pub use smoothed_ma::SmoothedMA;

// Re-exports - Tenth batch (Statistics)
pub use majority_rule::MajorityRule;
pub use rank_correlation::RankCorrelationIndex;
pub use standard_error::StandardError;
pub use standard_error_bands::StandardErrorBands;

// Re-exports - Tenth batch (Market structure)
pub use anchored_vwap_study::AnchoredVWAP;
pub use week52_highlow::{Week52HighLow, Week52HighLowLevels};

// Re-exports - Tenth batch (Other)
pub use bbw::BollingerBandsWidth;

// Re-exports - Eleventh batch (Oscillators)
pub use accelerator_oscillator::AcceleratorOscillator;
pub use advance_decline_ratio_bars::AdvanceDeclineRatioBars;
pub use aroon_oscillator::AroonOscillator;
pub use average_daily_range::AverageDailyRange;
pub use bbtrend::BBTrend;
pub use bull_bear_power::BullBearPower;
pub use chaikin_oscillator::ChaikinOscillator;
pub use ulcer_index::UlcerIndex;
// Re-exports - Standard oscillators
pub use price_momentum_oscillator::PriceMomentumOscillator;
pub use rci_ribbon::RciRibbon;
pub use relative_volatility_index::RelativeVolatilityIndex;
pub use smi_ergodic::{SMIErgodic, SMIErgodicIndicator, SMIErgodicOscillator};
pub use special_k::PringsSpecialK;
pub use woodies_cci::WoodiesCCI;
// Re-exports - Time studies
pub use performance::Performance;
pub use relative_volume_at_time::{RelativeVolumeAnchor, RelativeVolumeAtTime, RelativeVolumeMode};
pub use seasonality::Seasonality;
pub use trading_sessions::TradingSessions;
pub use up_down_volume::UpDownVolume;
pub use visible_average_price::VisibleAveragePrice;
pub use volume_24h::Volume24h;
pub use volume_delta::{CumulativeVolumeDelta, VolumeDelta};

/// A float parameter clamped into `min..=max`, NaN to `min`, the way constructors
/// raise a length of 0 to 1.
fn clamp_param(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

/// `compute` run over the bars whose inputs `finite` accepts, the other bars given no
/// value, so an unusable bar leaves running state unchanged instead of spoiling every
/// later bar. Clean data is not copied.
fn over_finite_bars(
    data: &[Bar],
    finite: impl Fn(&Bar) -> bool,
    compute: impl FnOnce(&[Bar]) -> crate::studies::IndicatorValues,
) -> crate::studies::IndicatorValues {
    if data.iter().all(&finite) {
        return compute(data);
    }
    let kept: Vec<_> = data.iter().filter(|b| finite(b)).cloned().collect();
    let computed = compute(&kept);
    let mut values = crate::studies::IndicatorValues::missing(data.len());
    for (line, column) in computed.columns().iter().enumerate() {
        let mut source = column.iter();
        for (bar, input) in data.iter().enumerate() {
            if finite(input) {
                values.set_value(line, bar, *source.next().expect("one value per finite bar"));
            }
        }
    }
    values
}

mod trade_profile;
pub use trade_profile::{TradeProfile, TradeProfileRanges, trade_profiles};

mod order_flow;
pub use order_flow::OrderFlowStudy;

mod smoothing;

/// Asserts that a causal study gives, on every prefix of `bars`, the values a batch calculation
/// of all bars has on those bars: appending bars one at a time and calculating each prefix afresh
/// both equal the batch, so no value depends on a later bar.
#[cfg(test)]
pub(super) fn assert_replay_matches_batch(study: &dyn crate::studies::Indicator, bars: &[Bar]) {
    let mut batch = study.clone_box();
    batch.calculate(bars);
    let mut stepped = study.clone_box();
    for end in 1..=bars.len() {
        let mut prefix = study.clone_box();
        prefix.calculate(&bars[..end]);
        stepped.update(&bars[..end], end - 1);
        for (label, candidate) in [("stepped", &stepped), ("prefix", &prefix)] {
            for line in 0..batch.line_count() {
                for bar in 0..end {
                    let (got, want) = (
                        candidate.values().value(line, bar),
                        batch.values().value(line, bar),
                    );
                    assert_eq!(
                        got,
                        want,
                        "{label} {} line {line} bar {bar} of {end}",
                        batch.name()
                    );
                }
            }
        }
    }
}

/// Daily bars from (high, low, close) triples, opening at the close.
#[cfg(test)]
pub(super) fn hlc_bars(hlc: &[(f64, f64, f64)]) -> Vec<Bar> {
    hlc.iter()
        .enumerate()
        .map(|(at, &(high, low, close))| {
            let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::days(at as i64);
            Bar::new(time, close, high, low, close, 1.0)
        })
        .collect()
}
