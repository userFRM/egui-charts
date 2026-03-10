//! Built-in technical indicators.
//!
//! This module contains 150+ technical indicators covering every major
//! category of technical analysis. All indicators implement the
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
//! [`CCI`], [`RateOfChange`], [`Aroon`], [`AwesomeOscillator`],
//! [`UltimateOscillator`], [`ConnorsRSI`], [`CoppockCurve`], [`TRIX`],
//! [`ElderRay`], [`FisherTransform`], [`ForceIndex`], [`MassIndex`],
//! [`DetrendedPriceOscillator`], [`KDJ`], [`PrettyGoodOscillator`],
//! [`ChandeMomentumOscillator`], [`TrueStrengthIndex`],
//! [`StochasticMomentumIndex`], [`SchaffTrendCycle`]
//!
//! ## Trend
//! [`ADX`], [`SuperTrend`], [`IchimokuCloud`], [`ParabolicSAR`],
//! [`DirectionalMovement`], [`VortexIndicator`], [`SqueezeMomentum`]
//!
//! ## Volatility
//! [`BollingerBands`], [`ATR`], [`KeltnerChannels`], [`DonchianChannels`],
//! [`StandardDeviation`], [`HistoricalVolatility`], [`ChaikinVolatility`],
//! [`ChandelierExit`], [`AccelerationBands`], [`Envelopes`]
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
//! [`CorrelationCoefficient`], [`LinearRegression`], [`StandardError`],
//! [`RankCorrelationIndex`], [`MajorityRule`]

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
mod correlation;
mod linear_regression;
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

// Seventh batch - DeepCharts/Volumetrica-style indicators
mod cvd;
mod iceberg;
mod liquidity_tracker;
mod stop_run;

// Eighth batch - Volume indicators
mod net_volume;
mod price_volume_trend_study;
mod vol_profile_fixed;
mod vol_profile_visible;

// Eighth batch - Volatility indicators
mod close_to_close_vol;
mod ohlc_volatility;
mod volatility_index;
mod zero_trend_vol;

// Ninth batch - Moving average variants and cross indicators
mod envelopes;
mod guppy_ma;
mod ma_channel;
mod ma_cross;
mod ma_ema_cross;
mod ma_hamming;
mod ma_multiple;
mod smoothed_ma;

// Tenth batch - Statistics indicators
mod correlation_log;
mod majority_rule;
mod rank_correlation;
mod standard_error;
mod standard_error_bands;

// Tenth batch - Breadth & Price indicators
mod advance_decline;
mod anchored_vwap_study;
mod week52_highlow;

// Tenth batch - Other indicators
mod bbw;
mod ratio_study;
mod spread_study;

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
pub use vwap::VolumeWeightedAvgPrice;

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
pub use correlation::CorrelationCoefficient;
pub use linear_regression::{LinearRegression, LinearRegressionSlope};
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

// Re-exports - Seventh batch (DeepCharts/Volumetrica indicators)
pub use cvd::{CVD, CVDMode};
pub use iceberg::{IcebergDetection, IcebergDetector};
pub use liquidity_tracker::{
    LiquidityLevel, LiquiditySweep, LiquidityTracker, LiquidityZone, LiquidityZoneType,
    SweepDirection,
};
pub use stop_run::{StopRunDetection, StopRunIndicator, StopRunType};

// Re-exports - Eighth batch (Volume indicators)
pub use net_volume::NetVolume;
pub use price_volume_trend_study::PriceVolumeTrendStudy;
pub use vol_profile_fixed::VolProfileFixed;
pub use vol_profile_visible::VolProfileVisible;

// Re-exports - Eighth batch (Volatility indicators)
pub use close_to_close_vol::CloseToCloseVol;
pub use ohlc_volatility::OhlcVolatility;
pub use volatility_index::VolatilityIndex;
pub use zero_trend_vol::ZeroTrendVol;

// Re-exports - Ninth batch (Moving average variants and cross indicators)
pub use envelopes::Envelopes;
pub use guppy_ma::GuppyMA;
pub use ma_channel::MAChannel;
pub use ma_cross::MACross;
pub use ma_ema_cross::EMACross;
pub use ma_hamming::HammingMA;
pub use ma_multiple::MultipleMA;
pub use smoothed_ma::SmoothedMA;

// Re-exports - Tenth batch (Statistics)
pub use correlation_log::CorrelationLogReturns;
pub use majority_rule::MajorityRule;
pub use rank_correlation::RankCorrelationIndex;
pub use standard_error::StandardError;
pub use standard_error_bands::StandardErrorBands;

// Re-exports - Tenth batch (Breadth & Price)
pub use advance_decline::AdvanceDeclineLine;
pub use anchored_vwap_study::AnchoredVWAP;
pub use week52_highlow::Week52HighLow;

// Re-exports - Tenth batch (Other)
pub use bbw::BollingerBandsWidth;
pub use ratio_study::RatioStudy;
pub use spread_study::SpreadStudy;

/// Return a list of `(short_name, description)` pairs for every built-in indicator.
///
/// This is useful for building indicator-picker UIs or auto-complete
/// lists. The short name matches the factory registration key used by
/// [`IndicatorFactory`](super::IndicatorFactory).
pub fn list_builtin_indicators() -> Vec<(&'static str, &'static str)> {
    vec![
        // Moving Avgs
        ("SMA", "Simple Moving Avg"),
        ("EMA", "Exponential Moving Avg"),
        ("WMA", "Weighted Moving Avg"),
        ("DEMA", "Double Exponential Moving Avg"),
        ("TEMA", "Triple Exponential Moving Avg"),
        ("HMA", "Hull Moving Avg"),
        ("ALMA", "Arnaud Legoux Moving Avg"),
        ("VWMA", "Volume Weighted Moving Avg"),
        ("LSMA", "Least Squares Moving Avg"),
        ("SWMA", "Symmetrically Weighted Moving Avg"),
        ("KAMA", "Kaufman Adaptive Moving Avg"),
        // Volatility
        ("Bollinger Bands", "Bollinger Bands"),
        ("ATR", "Avg True Range"),
        ("Keltner", "Keltner Channels"),
        ("Donchian", "Donchian Channels"),
        ("StdDev", "Standard Deviation"),
        // Momentum
        ("RSI", "Relative Strength Index"),
        ("MACD", "Moving Avg Convergence Divergence"),
        ("VW-MACD", "Volume Weighted MACD"),
        ("Stochastic", "Stochastic Oscillator"),
        ("Stochastic RSI", "Stochastic RSI"),
        ("Williams %R", "Williams Percent Range"),
        ("CCI", "Commodity Channel Index"),
        ("ROC", "Rate of Change"),
        ("Aroon", "Aroon Indicator"),
        ("Awesome Oscillator", "Awesome Oscillator"),
        ("Ultimate Oscillator", "Ultimate Oscillator"),
        // Trend
        ("ADX", "Avg Directional Index"),
        ("SuperTrend", "SuperTrend"),
        ("Ichimoku", "Ichimoku Cloud"),
        ("Parabolic SAR", "Parabolic Stop and Reverse"),
        // Volume
        ("OBV", "On Balance Volume"),
        ("VWAP", "Volume Weighted Avg Price"),
        ("MFI", "Money Flow Index"),
        ("A/D", "Accumulation/Distribution Line"),
        ("CMF", "Chaikin Money Flow"),
        // Support/Resistance
        (
            "Pivot Points",
            "Pivot Points (Standard, Fibonacci, Woodie, Camarilla, DeMark)",
        ),
        ("ZigZag", "ZigZag - Identifies significant swings"),
        // Additional Oscillators/Momentum
        ("TRIX", "Triple Exponential Avg"),
        ("Coppock", "Coppock Curve - Long-term momentum"),
        ("Force Index", "Force Index - Price movement force"),
        ("Elder Ray", "Elder Ray - Bull/Bear Power"),
        (
            "Fisher Transform",
            "Fisher Transform - Gaussian distribution",
        ),
        ("Connors RSI", "Connors RSI - Composite momentum"),
        ("DPO", "Detrended Price Oscillator"),
        ("Mass Index", "Mass Index - Range expansion reversal"),
        // Volume/Price Oscillators
        ("VROC", "Volume Rate of Change"),
        ("BoP", "Balance of Power - Buyer/seller strength"),
        ("EoM", "Ease of Movement - Price/volume relationship"),
        (
            "Volume Oscillator",
            "Volume Oscillator - Volume EMA difference",
        ),
        ("PPO", "Percentage Price Oscillator"),
        // Trend/Volatility
        ("Vortex", "Vortex Indicator - Trend direction"),
        ("Klinger", "Klinger Volume Oscillator - Money flow trend"),
        ("CHOP", "Choppiness Index - Market trendiness"),
        ("HV", "Historical Volatility - Annualized volatility"),
        ("STC", "Schaff Trend Cycle - Fast trend oscillator"),
        // Advanced Indicators
        ("KDJ", "KDJ - Enhanced Stochastic with K, D, J lines"),
        ("MFI BW", "Market Facilitation Index (Bill Williams)"),
        ("Squeeze", "Squeeze Momentum - Volatility breakout"),
        (
            "Ehlers Fisher",
            "Ehlers Fisher Transform - Price distribution",
        ),
        ("COG", "Center of Gravity - Reversal detection"),
        ("Chandelier", "Chandelier Exit - ATR-based trailing stop"),
        ("KST", "Know Sure Thing - Multi-timeframe momentum"),
        ("Rainbow", "Rainbow Oscillator - Multi-MA distance"),
        ("DMI", "Directional Movement Index - +DI/-DI lines"),
        ("PGO", "Pretty Good Oscillator - Price in ATR units"),
        // Fourth batch - Statistics and Price variants
        ("ASI", "Accumulation Swing Index - Cumulative swing"),
        ("Correlation", "Correlation Coefficient - Price vs time"),
        ("LinReg", "Linear Regression - Trend line projection"),
        ("LinReg Slope", "Linear Regression Slope - Rate of change"),
        ("%B", "Percent B - Pos within Bollinger Bands"),
        ("Price Channel", "Price Channel - N-bar high/low bands"),
        ("RVI", "Relative Vigor Index - Momentum oscillator"),
        ("SMI", "Stochastic Momentum Index - Smoothed stochastic"),
        ("TP", "Typical Price - (H+L+C)/3"),
        ("WC", "Weighted Close - (H+L+2C)/4"),
        ("MP", "Median Price - (H+L)/2"),
        (
            "Ultimate Momentum",
            "Ultimate Momentum - Multi-period momentum",
        ),
        // Fifth batch - Additional momentum and volume
        ("AccBands", "Acceleration Bands - Volatility-based envelope"),
        ("CV", "Chaikin Volatility - Range rate of change"),
        ("CMO", "Chande Momentum Oscillator - Up/down momentum"),
        ("CSI", "Commodity Selection Index - Pos sizing"),
        ("TDI", "Trend Detection Index - Net vs absolute movement"),
        ("DS", "Double Stochastic - Double-smoothed oscillator"),
        ("Impulse", "Elder Impulse System - Trend with momentum"),
        ("MD", "McGinley Dynamic - Self-adjusting MA"),
        ("TII", "Trend Intensity Index - Trend strength"),
        ("Qstick", "Qstick - Candlestick body avg"),
        ("TSI", "True Strength Index - Double-smoothed momentum"),
        ("VIDYA", "Variable Index Dynamic Avg - Volatility MA"),
        ("ZLEMA", "Zero Lag EMA - Reduced lag moving avg"),
        ("VPT", "Volume Price Trend - Price-weighted volume"),
        ("NVI", "Negative Volume Index - Low volume days"),
        ("PVI", "Positive Volume Index - High volume days"),
        ("WAD", "Williams AD - Buying/selling pressure"),
        // Sixth batch - Price calculations
        ("OHLC4", "OHLC4 - (Open + High + Low + Close) / 4"),
        ("HLC3", "HLC3 - (High + Low + Close) / 3"),
        ("HL2", "HL2 - (High + Low) / 2 - Midpoint"),
        ("TR", "True Range - Max price movement"),
        // Seventh batch - DeepCharts/Volumetrica indicators
        ("CVD", "Cumulative Volume Delta - Order flow analysis"),
        ("Iceberg", "Iceberg Detector - Hidden large orders"),
        ("Stop Run", "Stop Run Indicator - Stop hunt patterns"),
        ("Liquidity", "Liquidity Tracker - Volume accumulation zones"),
        // Eighth batch - Volume indicators
        (
            "Net Volume",
            "Net Volume - Positive on up bars, negative on down bars",
        ),
        (
            "Vol Profile VR",
            "Volume Profile Visible Range - Volume distribution across price levels",
        ),
        (
            "Vol Profile FR",
            "Volume Profile Fixed Range - Volume distribution with fixed boundaries",
        ),
        (
            "PVT Enhanced",
            "Price Volume Trend Enhanced - PVT with EMA signal line",
        ),
        // Eighth batch - Volatility indicators
        (
            "C2C Vol",
            "Close-to-Close Volatility - Annualized std dev of log returns",
        ),
        (
            "GK Vol",
            "Garman-Klass OHLC Volatility - Efficient OHLC estimator",
        ),
        (
            "Parkinson Vol",
            "Zero-Trend Volatility (Parkinson) - High-low range estimator",
        ),
        (
            "Vol Index",
            "Volatility Index - ATR-based annualized volatility percentage",
        ),
        // Ninth batch - Moving average variants and cross indicators
        (
            "SMMA",
            "Smoothed Moving Average - Wilder's smoothing method",
        ),
        (
            "GMMA",
            "Guppy Multiple Moving Average - 12 EMAs in short/long groups",
        ),
        (
            "MA Cross",
            "Moving Average Cross - Two SMA lines for crossover signals",
        ),
        (
            "EMA Cross",
            "EMA Cross - Two EMA lines for crossover signals",
        ),
        (
            "MA Channel",
            "Moving Average Channel - MA with upper/lower offset bands",
        ),
        (
            "Multi MA",
            "Multiple Moving Averages - SMA 20, 50, 100, 200",
        ),
        (
            "Envelopes",
            "Moving Average Envelopes - Percentage bands around MA",
        ),
        (
            "Hamming MA",
            "Hamming Weighted Moving Average - Hamming window weighting",
        ),
        // Tenth batch - Statistics
        (
            "CorrLogRet",
            "Correlation Log Returns - Pearson correlation of log returns vs time",
        ),
        (
            "MajRule",
            "Majority Rule - Up/down close majority over period",
        ),
        (
            "RankCorr",
            "Rank Correlation Index (Spearman) - Price vs time rank correlation",
        ),
        (
            "StdErr",
            "Standard Error - Standard error of linear regression",
        ),
        (
            "StdErrBands",
            "Standard Error Bands - LinReg with standard error envelope",
        ),
        // Tenth batch - Breadth & Price
        (
            "A/D Line",
            "Advance/Decline Line - Cumulative up/down close count",
        ),
        (
            "52W HL%",
            "52-Week High/Low Percentage - Position within period range",
        ),
        (
            "AVWAP",
            "Anchored VWAP - Volume weighted average price from anchor bar",
        ),
        // Tenth batch - Other
        ("Spread", "Spread Study - High minus Low for each bar"),
        (
            "Ratio",
            "Ratio Study - Close/Open percentage change per bar",
        ),
        (
            "BBW",
            "Bollinger Bands Width - Band width as percentage of middle band",
        ),
    ]
}
