use super::*;
use crate::data::OrderFlowKind;
use crate::model::{Bar, TimezoneId};
use chrono::{Duration, TimeZone, Utc};

/// Every built-in, with its `k`-th length parameter set to `l(k)` and each
/// float parameter of conventional value `v` set to `x(v)`.
fn builtins_with(l: impl Fn(usize) -> usize, x: impl Fn(f64) -> f64) -> Vec<Box<dyn Indicator>> {
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
        Box::new(MAChannel::new(l(0), l(1)).with_offsets(3, -2)),
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
        Box::new(
            KeltnerChannels::new(l(0), l(1), x(2.0))
                .with_average(OscillatorAverage::Simple)
                .with_band(KeltnerBand::Range)
                .with_source(crate::model::PriceSource::HL2),
        ),
        Box::new(DonchianChannels::new(l(0))),
        Box::new(StandardDeviation::new(l(0))),
        Box::new(HistoricalVolatility::new(l(0)).with_trading_days(x(252.0))),
        Box::new(ChaikinVolatility::new(l(0), l(1))),
        Box::new(ChandelierExit::new(l(0), x(3.0))),
        Box::new(AccelerationBands::new(l(0), x(4.0))),
        Box::new(Envelopes::new(l(0), x(10.0), EnvelopeAverage::Simple)),
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
        Box::new(UpDownVolume::new()),
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
        Box::new(VolProfileSession::new(l(0))),
        Box::new(VolProfilePeriodic::new(l(0))),
        Box::new(VolProfileAnchored::new(l(0))),
        Box::new(VolProfileAutoAnchored::new(l(0))),
        Box::new(TimePriceOpportunity::default()),
        Box::new(OrderFlowStudy::new(OrderFlowKind::Spread)),
        Box::new(OrderFlowStudy::new(OrderFlowKind::BookSize {
            levels: l(0),
            distance: x(0.0),
        })),
        Box::new(OrderFlowStudy::new(OrderFlowKind::TradeRate {
            volume: false,
            seconds: l(0),
            instant: l(1),
        })),
        Box::new(OrderFlowStudy::new(OrderFlowKind::AggressorRate {
            seconds: l(0),
            instant: l(1),
        })),
        Box::new(OrderFlowStudy::new(OrderFlowKind::LevelsExecuted {
            seconds: l(0),
            reversal: true,
        })),
        Box::new(OrderFlowStudy::new(OrderFlowKind::SizeDelta {
            ranges: [[x(0.0), x(9.0)], [x(10.0), x(99.0)], [x(100.0), f64::MAX]],
        })),
        Box::new(OrderFlowStudy::new(OrderFlowKind::FlowSum { bars: l(0) })),
        Box::new(OrderFlowStudy::new(OrderFlowKind::FilteredCvd {
            min: x(100.0),
            max: f64::MAX,
        })),
        Box::new(OrderFlowStudy::new(OrderFlowKind::LevelsTradedThrough {
            seconds: l(0),
        })),
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
        #[cfg(feature = "drawings")]
        Box::new(AutoDrawing::new(AutoDrawingKind::TrendLine, x(5.0), l(0))),
        #[cfg(feature = "drawings")]
        Box::new(AutoDrawing::new(
            AutoDrawingKind::ParallelChannel,
            x(5.0),
            l(0),
        )),
        #[cfg(feature = "drawings")]
        Box::new(AutoDrawing::new(
            AutoDrawingKind::FibonacciRetracement,
            x(5.0),
            l(0),
        )),
        #[cfg(feature = "drawings")]
        Box::new(AutoDrawing::new(
            AutoDrawingKind::FibonacciExtension,
            x(5.0),
            l(0),
        )),
        #[cfg(feature = "drawings")]
        Box::new(AutoDrawing::new(AutoDrawingKind::Pitchfork, x(5.0), l(0))),
        Box::new(PivotPointsHighLow::new(l(0), l(1), l(2), l(3))),
        Box::new(MissedPivots::new(l(0), l(1), x(0.1), l(2))),
        Box::new(KeyLevels::new(l(0), l(1), l(2), x(0.25))),
        Box::new(IntradayPivotPoints::new(l(0))),
        Box::new(ProjectedPivotPoints::new().with_pivots_back(l(1))),
        Box::new(WilliamsFractals::new(l(0))),
        Box::new(LunarPhases::new()),
        Box::new(Gaps::new(l(0), x(30.0)).with_trail_limit(Some(l(1)))),
        Box::new(Gaps::new(l(0), x(30.0)).with_partial_closing(true)),
        Box::new(RsiDivergence::new(l(0), l(1), l(2), l(3), 60)),
        Box::new(TechnicalRatings::new(l(0), l(1), l(2))),
        Box::new(Reversal::new(l(0))),
        Box::new(ElliottWave::new(l(0), l(1), l(2))),
        Box::new(RisingWedge::new(l(0), l(1), l(2), l(3), l(4), l(5))),
        Box::new(FallingWedge::new(l(0), l(1), l(2), l(3), l(4), l(5))),
        Box::new(KnoxvilleDivergence::new(l(0), l(1), l(2), l(3), l(4))),
        Box::new(AdxBreakout::new(l(0), l(1), l(2))),
        Box::new(PriceChannel::new(l(0))),
        Box::new(LinearRegression::new(l(0))),
        Box::new(LinearRegressionChannel::new(l(0), x(2.0)).with_multipliers(x(2.0), x(1.0))),
        Box::new(LinearRegressionSlope::new(l(0))),
        Box::new(TrendStrengthIndex::new(l(0))),
        Box::new(StandardError::new(l(0))),
        Box::new(StandardErrorBands::new(l(0), x(2.0))),
        Box::new(RankCorrelationIndex::new(l(0))),
        Box::new(Ratio::default()),
        Box::new(Spread::default()),
        Box::new(Correlation::new(l(0))),
        Box::new(Correlation::new(l(0)).with_log_returns(true)),
        Box::new(RciRibbon::new(l(0), l(1), l(2))),
        Box::new(MajorityRule::new(l(0))),
        Box::new(AccumulationSwingIndex::new(x(0.5))),
        Box::new(Seasonality::new()),
        Box::new(Performance::new()),
        Box::new(HigherIntervalCandles::default()),
        Box::new(BollingerBars::new()),
        Box::new(TypicalPrice::new()),
        Box::new(MedianPrice::new()),
        Box::new(WeightedClose::new()),
        Box::new(HL2::new()),
        Box::new(HLC3::new()),
        Box::new(OHLC4::new()),
        Box::new(VisibleAveragePrice::new()),
        Box::new(Week52HighLow::new()),
        Box::new(Week52HighLowLevels::new()),
        Box::new(AcceleratorOscillator::new()),
        Box::new(AroonOscillator::new(l(0))),
        Box::new(AverageDailyRange::new(l(0))),
        Box::new(BBTrend::new(l(0), l(1), x(2.0))),
        Box::new(ChaikinOscillator::new(l(0), l(1))),
        Box::new(BullBearPower::new(l(0))),
        Box::new(AdvanceDeclineRatioBars::new(l(0))),
        Box::new(AdvanceDeclineRatio::default()),
        Box::new(AdvanceDeclineLine::default()),
        Box::new(CumulativeVolumeIndex::default()),
        Box::new(UlcerIndex::new(l(0))),
        Box::new(DetrendedPriceOscillator::new(l(0)).with_centered(true)),
        Box::new(TradingSessions::new()),
        Box::new(CandlestickPatterns::default()),
        Box::new(ChartPatterns::new(PatternFamily::Continuation)),
        Box::new(ChartPatterns::new(PatternFamily::Reversal)),
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
/// turning intraday, unchanged data, and bars with a non-finite price or volume. Unchanged
/// updates must retain the checkpoint for withdrawing and restoring the developing bar,
/// including its markers, zones, tables and projections.
#[test]
fn every_update_equals_a_fresh_calculate() {
    // In UTC no VWAP value depends on its daily rule; in Tokyo with weekly periods the
    // daily and intraday trading days group the bars differently.
    let tokyo_week = || -> Box<dyn Indicator> {
        Box::new(
            VolumeWeightedAvgPrice::new()
                .with_anchor(VwapAnchor::Week)
                .with_time_zone(TimezoneId::fixed_offset(9 * 3600).unwrap()),
        )
    };
    let pivot_labels = || -> Box<dyn Indicator> {
        Box::new(
            PivotPoints::new()
                .with_pivots_back(3)
                .with_labels(true, true)
                .with_label_side(PivotLabelSide::Right),
        )
    };
    let zigzag_labels = || -> Box<dyn Indicator> {
        Box::new(ZigZag::new(1.0, 2).with_labels(true, Some(ZigZagChange::Price), true))
    };
    let session_labels = || -> Box<dyn Indicator> {
        Box::new(
            TradingSessions::new()
                .with_labels(true, true, true)
                .with_lines(true, true, true),
        )
    };
    let mut studies = builtins_with(|k| 3 + 2 * k, |v| v);
    studies.push(tokyo_week());
    studies.push(pivot_labels());
    studies.push(zigzag_labels());
    studies.push(session_labels());
    for study in &studies {
        if matches!(
            study.name(),
            "C2C Vol" | "Price Channel" | "HLC3" | "HL2" | "CVD" | "Ehlers Fisher"
        ) {
            assert!(
                study.is_incremental(),
                "{} must update on every tick",
                study.name()
            );
        }
    }
    let mut twins = builtins_with(|k| 3 + 2 * k, |v| v);
    twins.push(tokyo_week());
    twins.push(pivot_labels());
    twins.push(zigzag_labels());
    twins.push(session_labels());
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
                                Duration::weeks(1)
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
            let last = data.last().cloned();
            for revision in 0..3 {
                let from = if revision == 0 {
                    from
                } else if let Some(last) = &last {
                    let at = data.len() - 1;
                    data[at] = if revision == 1 {
                        Bar::whitespace(last.time)
                    } else {
                        last.clone()
                    };
                    at
                } else {
                    continue;
                };
                study.update(&data, from);
                study.update(&data, data.len());
                let mut fresh = twin.clone_box();
                fresh.calculate(&data);
                assert_eq!(
                    bits(&study.values().to_rows()),
                    bits(&fresh.values().to_rows()),
                    "{} event {event}, revision {revision}",
                    study.name()
                );
                assert_eq!(
                    format!(
                        "{:?}",
                        (
                            study.markers(),
                            study.zones(),
                            study.table(),
                            study.projections(),
                            study.labels(),
                            (0..study.line_count())
                                .map(|line| study.provisional_after(line))
                                .collect::<Vec<_>>()
                        )
                    ),
                    format!(
                        "{:?}",
                        (
                            fresh.markers(),
                            fresh.zones(),
                            fresh.table(),
                            fresh.projections(),
                            fresh.labels(),
                            (0..fresh.line_count())
                                .map(|line| fresh.provisional_after(line))
                                .collect::<Vec<_>>()
                        )
                    ),
                    "{} metadata, event {event}, revision {revision}",
                    study.name()
                );
            }
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
                (b.open, b.high, b.low, b.close) = (b.open * s, b.high * s, b.low * s, b.close * s);
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
    // several lines never emits `Single`.
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
                    Err(_) => failures.push(format!("{}{n:?} on {label}: panicked", study.name())),
                    Ok(len) if len != bars.len() => failures.push(format!(
                        "{}{n:?} on {label}: {len} values for {} bars",
                        study.name(),
                        bars.len()
                    )),
                    Ok(_) => {
                        let lines = study.line_count();
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
            && format!("{:?}", zero.values().to_rows()) != format!("{:?}", one.values().to_rows())
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
    for (study, conventional) in bottom.iter().zip(&conventional).filter(|(study, _)| {
        !matches!(
            study.name(),
            "ZigZag"
                | "Auto Trend Line"
                | "Auto Parallel Channel"
                | "Auto Fibonacci Retracement"
                | "Auto Fibonacci Extension"
                | "Auto Pitchfork"
        )
    }) {
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
                } else if skipped.is_none() && seen.is_some() && !hostile.get(at).is_some_and(blank)
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
