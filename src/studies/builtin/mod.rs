//! Built-in technical indicators.
//!
//! This module contains the built-in technical indicators, grouped by
//! category below. All indicators implement the
//! [`Indicator`](super::Indicator) trait for a uniform API.
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
//! [`CCI`], [`Momentum`], [`RateOfChange`], [`Aroon`], [`AwesomeOscillator`],
//! [`UltimateOscillator`], [`ConnorsRSI`], [`CoppockCurve`], [`TRIX`],
//! [`ElderRay`], [`FisherTransform`], [`ForceIndex`], [`MassIndex`],
//! [`DetrendedPriceOscillator`], [`KDJ`], [`PrettyGoodOscillator`],
//! [`ChandeMomentumOscillator`], [`TrueStrengthIndex`],
//! [`StochasticMomentumIndex`], [`SchaffTrendCycle`], [`PriceOscillator`],
//! [`PercentageVolumeOscillator`]
//!
//! ## Trend
//! [`ADX`], [`SuperTrend`], [`IchimokuCloud`], [`ParabolicSAR`],
//! [`DirectionalMovement`], [`VortexIndicator`], [`SqueezeMomentum`]
//!
//! ## Volatility
//! [`BollingerBands`], [`ATR`], [`KeltnerChannels`], [`DonchianChannels`],
//! [`StandardDeviation`], [`HistoricalVolatility`], [`ChaikinVolatility`],
//! [`ChandelierExit`], [`AccelerationBands`], [`Envelopes`], [`ParkinsonVol`], [`ZeroTrendVol`]
//!
//! ## Volume
//! [`OnBalanceVolume`], [`VolumeWeightedAvgPrice`], [`MoneyFlowIndex`],
//! [`AccumulationDistribution`], [`ChaikinMoneyFlow`],
//! [`VolumeRateOfChange`], [`KlingerOscillator`], [`CVD`],
//! [`VolProfileVisible`], [`VolProfileFixed`]
//!
//! ## Support / resistance
//! [`PivotPoints`], [`ZigZag`], [`PriceChannel`]
//!
//! ## Statistics
//! [`LinearRegression`], [`StandardError`],
//! [`RankCorrelationIndex`], [`MajorityRule`]

use crate::model::Bar;
use crate::studies::IndicatorValue;

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
mod cmf;
mod mfi;
mod obv;
mod vwap;

// Trend indicators
mod adx;
mod ichimoku;
mod supertrend;

// Channel indicators
mod donchian;
mod keltner;

// Volatility indicators
mod std_dev;

// Support/Resistance indicators
mod pivot_points;
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
mod vol_profile_fixed;
mod vol_profile_visible;

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
pub use atr::ATR;
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
pub use cmf::ChaikinMoneyFlow;
pub use mfi::MoneyFlowIndex;
pub use obv::OnBalanceVolume;
pub use vwap::{VolumeWeightedAvgPrice, VwapAnchor};

// Re-exports - Trend indicators
pub use adx::ADX;
pub use ichimoku::IchimokuCloud;
pub use supertrend::SuperTrend;

// Re-exports - Channel indicators
pub use donchian::DonchianChannels;
pub use keltner::KeltnerChannels;

// Re-exports - Volatility indicators
pub use std_dev::StandardDeviation;

// Re-exports - Support/Resistance indicators
pub use pivot_points::{PivotMethod, PivotPoints};
pub use zigzag::{ZigZag, ZigZagMode};

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
pub use price_oscillator::PriceOscillator;
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
pub use vol_profile_fixed::VolProfileFixed;
pub use vol_profile_visible::VolProfileVisible;

// Re-exports - Eighth batch (Volatility indicators)
pub use close_to_close_vol::CloseToCloseVol;
pub use ohlc_volatility::OhlcVolatility;
pub use parkinson_vol::ParkinsonVol;
pub use zero_trend_vol::ZeroTrendVol;

// Re-exports - Ninth batch (Moving average variants and cross indicators)
pub use envelopes::Envelopes;
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
    compute: impl FnOnce(&[Bar]) -> Vec<IndicatorValue>,
) -> Vec<IndicatorValue> {
    if data.iter().all(&finite) {
        return compute(data);
    }
    let kept: Vec<_> = data.iter().filter(|b| finite(b)).cloned().collect();
    let mut values = compute(&kept).into_iter();
    data.iter()
        .map(|b| if finite(b) { values.next() } else { None })
        .map(|v| v.unwrap_or(IndicatorValue::None))
        .collect()
}

/// Most price rows a volume profile computes.
const MAX_PROFILE_ROWS: usize = 10_000;
