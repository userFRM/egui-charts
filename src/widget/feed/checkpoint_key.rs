//! Exact configuration of a supported trade-only feed. No replay records or source handles
//! belong to this key: a second chart can match it with a different subscription.

use super::{Chart, FeedLink, Instrument, SeriesId, Timeframe};
use crate::model::{ChartType, TimezoneId};
use crate::scales::TimezoneMode;
use crate::widget::TradeReplayCheckpointError as Error;
use std::num::{NonZeroU64, NonZeroUsize};

#[derive(Debug, PartialEq)]
pub(super) struct CheckpointKey {
    symbol: Box<str>,
    instrument: Instrument,
    zone: Option<Box<str>>,
    conversion: Option<Box<str>>,
    grouping: Grouping,
    #[cfg(feature = "studies")]
    studies: Vec<StudyKey>,
    bytes: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct Grouping {
    lots: NonZeroU64,
    bar_limit: Option<NonZeroUsize>,
    footprint: Option<FootprintGrouping>,
    extended_hours: bool,
    overnight_hours: bool,
    adjusted: bool,
}

/// Configured row grouping, not the current viewport or the ATR of a particular execution.
#[derive(Debug, PartialEq, Eq)]
struct FootprintGrouping {
    ticks: u32,
    atr_length: Option<u32>,
    auto_height_bits: Option<u32>,
}

impl FeedLink {
    // The pump appends the chart's own footprint input after the studies' inputs.
    pub(super) fn checkpoint_studies(
        &self,
    ) -> Result<&[(crate::data::OrderFlowInput, u64)], Error> {
        let Some((footprint, seen)) = &self.footprint_input else {
            return Ok(&self.study_inputs);
        };
        let Some(((last, last_seen), studies)) = self.study_inputs.split_last() else {
            return Err(Error::NotDrained);
        };
        if !last.same_source(footprint) || last_seen != seen {
            return Err(Error::NotDrained);
        }
        Ok(studies)
    }
}

struct Binding<'a> {
    link: &'a FeedLink,
    instrument: &'a Instrument,
    zone: Option<&'a str>,
    grouping: Grouping,
}

impl<'a> Binding<'a> {
    fn read(chart: &'a Chart) -> Result<Self, Error> {
        let link = chart.feed_link.as_ref().ok_or(Error::Unsupported)?;
        let Some(Timeframe::Volume(lots)) = chart.timeframe else {
            return Err(Error::Unsupported);
        };
        let footprint = chart.chart_type == ChartType::VolumeFootprint;
        if !chart.main_series_present
            || !chart.price_series.is_empty()
            || chart.replay.is_some()
            || chart.market_replay.is_some()
            || chart.quote.is_some()
            || chart.session_quote.is_some()
            || chart.server_time_offset.is_some()
            || chart.session_provider.is_some()
            || chart.horizontal_scale.is_some()
            || !chart.primitives.is_empty()
            || chart.mark_source.is_some()
            || !chart.signal_sources.is_empty()
            || chart.show_price_targets
            || chart.study_data.has_feed_stream()
            || chart.change_marks.is_some()
            || chart.footprint_open_interest
            || chart.chart_type.transforms_data()
            || matches!(
                chart.chart_type,
                ChartType::TimePriceOpportunity | ChartType::SessionVolumeProfile
            )
            || link.paused
            || link.resumed_from.is_some()
            || link.non_trade_bars
            || link.paging_stopped
            || link.bar_cursor.is_some()
            || link.trade_cursor.is_some()
            || link.depth_cursor.is_some()
            || link.bubbles.is_some()
            || link.footprint.is_some() != footprint
            || link.footprint_input.is_some() != footprint
            || (footprint && link.footprints_stopped)
            || link.replay_footprint.is_some()
            || !link.footprint_unavailable.is_empty()
            || link.order_book.is_some()
            || link.order_queue.is_some()
            || link.book_visible == Some(true)
            || link.depth_history.is_some()
            || link.heatmap.is_some()
        {
            return Err(Error::Unsupported);
        }
        #[cfg(feature = "drawings")]
        if chart.post_source.is_some()
            || chart.drawings.drawings.iter().any(|d| d.reads_executions())
        {
            return Err(Error::Unsupported);
        }
        if link.reload_pending
            || link.resumed
            || !link.pending.is_empty()
            || !link.retries.is_empty()
            || chart.feed.loading_history
            || link.clock.timeframe() != chart.timeframe
            || link.extended_hours != chart.extended_hours
            || link.overnight_hours != chart.overnight_hours
            || link.conversion != chart.conversion
        {
            return Err(Error::NotDrained);
        }
        let instrument = link.instrument.as_ref().ok_or(Error::Unsupported)?;
        if link.subscription.is_none()
            || instrument.session.is_some()
            || instrument.tick_size.is_some_and(|v| !v.is_finite())
            || instrument.pip_size.is_some_and(|v| !v.is_finite())
        {
            return Err(Error::Unsupported);
        }
        let zone = link
            .clock
            .sessionless_zone()
            .ok_or(Error::Unsupported)?
            .map(TimezoneId::name);
        // On a feed, volume bars always use the selected display zone. Borrow its resolved
        // name rather than constructing a second clock (or cloning its fixed-offset name).
        let selected = match &chart.chart_options.time_scale.timezone {
            TimezoneMode::Local => None,
            TimezoneMode::Utc => Some("UTC"),
            TimezoneMode::Timezone(id) => Some(id.name()),
            TimezoneMode::Exchange(fallback) => {
                Some(instrument.time_zone.as_ref().unwrap_or(fallback).name())
            }
        };
        if zone != selected {
            return Err(Error::NotDrained);
        }
        #[cfg(not(feature = "studies"))]
        if !chart.studies.indicators().is_empty() || !link.checkpoint_studies()?.is_empty() {
            return Err(Error::Unsupported);
        }
        Ok(Self {
            link,
            instrument,
            zone,
            grouping: Grouping {
                lots,
                bar_limit: chart.bar_limit(SeriesId::MAIN),
                footprint: footprint.then(|| {
                    let p = &chart.chart_type_params;
                    FootprintGrouping {
                        ticks: p.footprint_row_ticks,
                        atr_length: p
                            .footprint_row_atr
                            .then(|| p.footprint_row_atr_length.clamp(1, 200)),
                        auto_height_bits: (p.footprint_row_ticks == 0)
                            .then(|| p.footprint_auto_row_height.to_bits()),
                    }
                }),
                extended_hours: link.extended_hours,
                overnight_hours: link.overnight_hours,
                adjusted: link.adjusted,
            },
        })
    }
}

/// Heap of a cloned instrument in the admitted subset. Match every field so a new nested
/// allocation cannot silently evade the bound. Calendar rules are deliberately not admitted.
fn instrument_bytes(instrument: &Instrument) -> Option<usize> {
    let Instrument {
        symbol,
        description,
        exchange,
        logo,
        exchange_logo,
        country,
        sector,
        source,
        root,
        identifiers,
        symbol_type: _,
        tick_size: _,
        tick_tiers,
        pip_size: _,
        price_decimals: _,
        price_fraction: _,
        volume_decimals: _,
        currency,
        unit,
        time_zone,
        session,
        native_timeframes,
        supported_timeframes,
        services,
        data_status: _,
        dividend_adjustment: _,
        profile_resolutions,
    } = instrument;
    if session.is_some() {
        return None;
    }
    let text = [
        description,
        exchange,
        logo,
        exchange_logo,
        country,
        sector,
        source,
        root,
        currency,
        unit,
    ]
    .into_iter()
    .flatten()
    .try_fold(symbol.len(), |sum, s| sum.checked_add(s.len()))?;
    let text = identifiers.iter().try_fold(text, |sum, (kind, value)| {
        use crate::model::IdentifierKind;
        let label = match kind {
            IdentifierKind::Other(label) => label.len(),
            IdentifierKind::Isin
            | IdentifierKind::Cusip
            | IdentifierKind::Sedol
            | IdentifierKind::Figi => 0,
        };
        sum.checked_add(label)?.checked_add(value.len())
    })?;
    [
        std::mem::size_of_val(identifiers.as_slice()),
        std::mem::size_of_val(tick_tiers.as_slice()),
        time_zone.as_ref().map_or(0, |z| z.name().len()),
        std::mem::size_of_val(native_timeframes.as_deref().unwrap_or_default()),
        std::mem::size_of_val(supported_timeframes.as_deref().unwrap_or_default()),
        std::mem::size_of_val(services.as_deref().unwrap_or_default()),
        std::mem::size_of_val(profile_resolutions.as_slice()),
    ]
    .into_iter()
    .try_fold(text, usize::checked_add)
}

#[cfg(feature = "studies")]
#[derive(Debug, Eq, PartialEq)]
enum StudyKey {
    Ema {
        period: usize,
        source: crate::model::PriceSource,
        offset: i32,
        wait_for_close: bool,
    },
    TradeRate {
        volume: bool,
        seconds: usize,
        instant: usize,
    },
}

/// Inspect the actual configured built-ins, not user-overridable trait metadata. Match each
/// rate's input to the feed in order; a just-edited or shared source must not silently zip
/// with a different saved window. Recomputing a simple volume display needs only the bars.
#[cfg(feature = "studies")]
fn visit_studies(
    chart: &Chart,
    link: &FeedLink,
    mut visit: impl FnMut(StudyKey),
) -> Result<(), Error> {
    use crate::studies::{ConfiguredStudy, EMA, Indicator, OrderFlowStudy, StudySource, Volume};
    let mut inputs = link.checkpoint_studies()?.iter();
    for (&id, study) in chart.study_ids.iter().zip(chart.studies.indicators()) {
        let configured = study
            .as_ref()
            .concrete::<ConfiguredStudy>()
            .ok_or(Error::Unsupported)?;
        let (inner, offset, wait_for_close) =
            configured.replay_calculation().ok_or(Error::Unsupported)?;
        if Some(id) == chart.volume_study
            && inner
                .concrete::<Volume>()
                .is_some_and(Volume::default_replay_display)
            && configured.source().is_none()
            && offset == 0
            && wait_for_close
        {
            continue;
        }
        let key = if let Some(ema) = inner.concrete::<EMA>() {
            let Some(StudySource::Price(source)) = configured.source() else {
                // A chained output may carry a retained or throttled calculation of its parent.
                return Err(Error::Unsupported);
            };
            StudyKey::Ema {
                period: ema.replay_period(),
                source,
                offset,
                wait_for_close,
            }
        } else if let Some(rate) = inner.concrete::<OrderFlowStudy>() {
            if configured.source().is_some() || offset != 0 || !wait_for_close {
                return Err(Error::Unsupported);
            }
            let (volume, seconds, instant) = rate.replay_rate().ok_or(Error::Unsupported)?;
            let [source] = rate.order_flow_inputs() else {
                return Err(Error::Unsupported);
            };
            if !inputs
                .next()
                .is_some_and(|(input, _)| input.same_source(source))
            {
                return Err(Error::Unsupported);
            }
            StudyKey::TradeRate {
                volume,
                seconds,
                instant,
            }
        } else {
            return Err(Error::Unsupported);
        };
        visit(key);
    }
    if inputs.next().is_some() {
        return Err(Error::Unsupported);
    }
    Ok(())
}

impl CheckpointKey {
    pub(super) fn new(chart: &Chart) -> Result<Self, Error> {
        let b = Binding::read(chart)?;
        #[cfg(feature = "studies")]
        let studies = {
            let count = chart
                .study_ids
                .len()
                .saturating_sub(usize::from(chart.volume_study.is_some()));
            let mut keys = Vec::with_capacity(count);
            visit_studies(chart, b.link, |key| keys.push(key))?;
            keys
        };
        #[cfg(feature = "studies")]
        let study_bytes = studies
            .capacity()
            .checked_mul(size_of::<StudyKey>())
            .ok_or(Error::Unsupported)?;
        #[cfg(not(feature = "studies"))]
        let study_bytes = 0;
        let bytes = [
            chart.symbol.len(),
            b.zone.map_or(0, str::len),
            b.link.conversion.as_ref().map_or(0, String::len),
            study_bytes,
        ]
        .into_iter()
        .try_fold(
            instrument_bytes(b.instrument).ok_or(Error::Unsupported)?,
            usize::checked_add,
        )
        .ok_or(Error::Unsupported)?;
        Ok(Self {
            symbol: chart.symbol.as_str().into(),
            instrument: b.instrument.clone(),
            zone: b.zone.map(Into::into),
            conversion: b.link.conversion.as_deref().map(Into::into),
            grouping: b.grouping,
            #[cfg(feature = "studies")]
            studies,
            bytes,
        })
    }

    pub(super) fn heap_bytes(&self) -> usize {
        self.bytes
    }

    pub(super) fn matches(&self, chart: &Chart) -> Result<bool, Error> {
        let b = Binding::read(chart)?;
        #[cfg(feature = "studies")]
        let studies_match = {
            let mut planned = self.studies.iter();
            let mut same = true;
            visit_studies(chart, b.link, |key| same &= planned.next() == Some(&key))?;
            same && planned.next().is_none()
        };
        #[cfg(not(feature = "studies"))]
        let studies_match = true;
        Ok(studies_match
            && self.symbol.as_ref() == chart.symbol
            && &self.instrument == b.instrument
            && self.zone.as_deref() == b.zone
            && self.conversion.as_deref() == b.link.conversion.as_deref()
            && self.grouping == b.grouping)
    }
}
