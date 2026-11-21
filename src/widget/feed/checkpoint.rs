//! In-memory continuation of a drained, trade-only volume feed. The host owns the recording.

use super::{Chart, FeedLink, SeriesId, SubscriptionId, checkpoint_key::CheckpointKey};
use crate::data::{
    FootprintGuard, FootprintState, PackedFootprintInput, PackedFootprintState,
    inbox::{SinkCheckpoint, SinkCheckpointError, SinkCheckpointGuard},
    merge::Origin,
    trades::PackedTrades,
};
#[cfg(feature = "studies")]
use crate::data::{PackedTradeRate, RateGuard};
use crate::model::Bar;
use chrono::{DateTime, Utc};
use std::{fmt, mem::size_of_val};

/// Why a trade replay checkpoint was not planned, captured or restored.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TradeReplayCheckpointError {
    /// The chart contains state that cannot be resumed faithfully, or cannot be sized.
    Unsupported,
    /// A subscription, request, pending update or busy input has not settled.
    NotDrained,
    /// The plan belongs to another chart or the planned state has since changed.
    StalePlan,
    /// The destination and checkpoint have different replay configurations.
    Incompatible,
}
use TradeReplayCheckpointError as Error;
impl fmt::Display for TradeReplayCheckpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unsupported => "chart state does not support trade replay checkpoints",
            Self::NotDrained => "replay feed is not drained",
            Self::StalePlan => "trade replay checkpoint plan is stale",
            Self::Incompatible => "trade replay checkpoint is incompatible with this chart",
        })
    }
}
impl std::error::Error for TradeReplayCheckpointError {}
impl From<SinkCheckpointError> for Error {
    fn from(value: SinkCheckpointError) -> Self {
        match value {
            SinkCheckpointError::NotReady => Self::NotDrained,
            SinkCheckpointError::Unsupported => Self::Unsupported,
        }
    }
}

/// Single-use permission to capture one chart's unchanged execution position. Dropping it
/// does not affect the chart, its subscription or any other plan.
#[derive(Debug)]
pub struct TradeReplayCheckpointPlan {
    key: CheckpointKey,
    subscription: SubscriptionId,
    stamp: Stamp,
    bytes: usize,
    #[cfg(feature = "studies")]
    inputs: Vec<u64>,
}
impl TradeReplayCheckpointPlan {
    /// Upper bound in bytes for checkpoint storage and temporary heap requests during capture,
    /// including configuration transferred from this plan. Not allocator overhead, restore
    /// working memory, the chart's existing storage or process RSS.
    pub fn max_bytes(&self) -> usize {
        self.bytes
    }
}
#[derive(Debug, PartialEq)]
struct Stamp {
    inbox: u64,
    tape: u64,
    bars: u64,
    edits: u64,
    footprint: Option<(u64, u64)>,
    close_time: Option<DateTime<Utc>>,
    progress: FeedProgress,
}

// Replay diagnostics and tape paging decisions, not the destination subscription's health.
#[derive(Debug, PartialEq)]
struct FeedProgress {
    history_complete: bool,
    rejected_bars: u64,
    late_ticks: u64,
    repeated_ticks: u64,
    trades_complete: bool,
    trades_from_start: bool,
}
impl Stamp {
    fn new(chart: &Chart, link: &FeedLink, sink: &SinkCheckpointGuard<'_>) -> Self {
        Self {
            inbox: sink.revision(),
            tape: link.trades.revision(),
            bars: chart.state.revision(),
            edits: chart.readout_revision,
            footprint: link
                .footprint
                .as_ref()
                .zip(link.footprint_input.as_ref())
                .map(|(view, (_, seen))| (view.live.revision(), *seen)),
            close_time: chart.bar_close_time,
            progress: FeedProgress {
                history_complete: chart.feed.history_complete,
                rejected_bars: chart.feed.rejected_bars,
                late_ticks: chart.feed.late_ticks,
                repeated_ticks: chart.feed.repeated_ticks,
                trades_complete: link.trades_complete,
                trades_from_start: link.trades_from_start,
            },
        }
    }
}

/// Reusable, opaque chart/feed state at one execution boundary. Keep the recording identity,
/// range and exact next event position alongside it: neither a timestamp nor chart settings
/// identify the recording. Not a serialization format or a saved chart layout.
pub struct TradeReplayCheckpoint {
    key: CheckpointKey,
    sink: SinkCheckpoint,
    tape: PackedTrades,
    footprint: Option<Box<SavedFootprint>>,
    bars: Vec<Bar>,
    origins: Vec<(DateTime<Utc>, Origin)>,
    time: Option<DateTime<Utc>>,
    pushed_from: Option<DateTime<Utc>>,
    close_time: Option<DateTime<Utc>>,
    progress: FeedProgress,
    #[cfg(feature = "studies")]
    rates: Vec<PackedTradeRate>,
}
impl fmt::Debug for TradeReplayCheckpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TradeReplayCheckpoint")
            .field("bars", &self.bars.len())
            .field("time", &self.time)
            .finish_non_exhaustive()
    }
}

/// The producer owns the next trade allocation. Its chart mirror can also contain a
/// different retained prefix or coverage boundary. Neither alone is a faithful rewind.
struct SavedFootprint {
    input: PackedFootprintInput,
    display: PackedFootprintState,
}

struct LockedFootprint<'a> {
    input: FootprintGuard<'a>,
    display: &'a FootprintState,
}

impl LockedFootprint<'_> {
    fn heap_bytes(&self) -> Option<usize> {
        size_of::<SavedFootprint>()
            .checked_add(self.input.heap_bytes()?)?
            .checked_add(self.display.checkpoint_bytes()?)
    }

    fn capture(&self) -> Box<SavedFootprint> {
        Box::new(SavedFootprint {
            input: self.input.snapshot(),
            display: self.display.pack_checkpoint(),
        })
    }
}

fn validate_tape(chart: &Chart, link: &FeedLink) -> Result<(), Error> {
    if !link.trades.checkpoint_eligible()
        || chart
            .bars(SeriesId::MAIN)
            .iter()
            .any(|b| b.colors.is_some() || b.payload.is_some())
    {
        return Err(Error::Unsupported);
    }
    Ok(())
}

fn footprint_guard<'a>(
    link: &'a FeedLink,
    sink: &SinkCheckpointGuard<'_>,
) -> Result<Option<LockedFootprint<'a>>, Error> {
    // A host can hold an input independently of the sink. Verify the complete list before
    // separating the built-in footprint from the optional study inputs.
    if !link
        .study_inputs
        .iter()
        .map(|(i, _)| i)
        .zip(sink.inputs())
        .all(|(a, b)| a.same_source(b))
        || link.study_inputs.len() != sink.inputs().len()
    {
        return Err(Error::NotDrained);
    }
    let Some((input, seen)) = &link.footprint_input else {
        return Ok(None);
    };
    let display = link.footprint.as_ref().ok_or(Error::Unsupported)?;
    let input = input.try_checkpoint_footprint().ok_or(Error::NotDrained)?;
    if input.revision() != *seen {
        return Err(Error::NotDrained);
    }
    if !input.eligible(&link.clock)
        || display.view().clock() != &link.clock
        || !display.checkpoint_eligible()
    {
        return Err(Error::Unsupported);
    }
    Ok(Some(LockedFootprint { input, display }))
}

#[cfg(feature = "studies")]
fn rate_guards(link: &FeedLink) -> Result<Vec<RateGuard<'_>>, Error> {
    let studies = link.checkpoint_studies()?;
    let mut guards = Vec::with_capacity(studies.len());
    for (input, seen) in studies {
        let guard = input.try_checkpoint_rate().ok_or(Error::NotDrained)?;
        if guard.revision() != *seen {
            return Err(Error::NotDrained);
        }
        if !guard.eligible() {
            return Err(Error::Unsupported);
        }
        guards.push(guard);
    }
    Ok(guards)
}

impl Chart {
    /// Plan a checkpoint for this chart after its replay feed has drained. No bars, tape,
    /// footprint rows or study data are copied. Built-in volume footprints must have bounded,
    /// trade-derived rows. The plan is single use and tied to this chart and subscription;
    /// new input or changed replay state invalidates it. On refusal chart and sink are untouched.
    ///
    /// A host can capture after pumping its feed, retain the checkpoint while playback
    /// continues, then rewind to it. Here the host drives a volume chart with executions only.
    /// ```
    /// use egui_charts::{ChartBuilder, data::{Datafeed, HistoryPage, HistoryRequest, Reply, Sink, Subscription},
    ///     model::{Instrument, Tick, Timeframe}, widget::TradeReplayCheckpointError};
    /// use std::{num::NonZeroU64, sync::{Arc, Mutex}};
    /// # #[derive(Default)] struct Feed(Mutex<Option<Sink>>);
    /// # impl Datafeed for Feed {
    /// #     fn resolve(&self,s:&str,r:Reply<Instrument>){r.send(Ok(Instrument::new(s)));}
    /// #     fn history(&self,_:HistoryRequest,r:Reply<HistoryPage>){r.send(Ok(HistoryPage::new(vec![],true)));}
    /// #     fn subscribe(&self,s:Subscription){*self.0.lock().unwrap()=Some(s.sink);}
    /// # }
    /// # fn main()->Result<(), Box<dyn std::error::Error>> {
    /// let feed = Arc::new(Feed::default());
    /// let mut chart = ChartBuilder::new().with_symbol("A").with_feed(feed.clone())
    ///     .with_timeframe(Timeframe::Volume(NonZeroU64::new(2048).unwrap())).build();
    /// # let ctx=egui::Context::default();
    /// # let frame=|c:&mut egui_charts::Chart| {ctx.run_ui(Default::default(),|ui|{c.show(ui);}).drop_without_applying_deltas();};
    /// # for _ in 0..6 {frame(&mut chart);}
    /// # let sink=feed.0.lock().unwrap().clone().unwrap();
    /// # let time=chrono::DateTime::from_timestamp(1_700_000_000,0).unwrap();
    /// # sink.push_tick(Tick::new(time,10.,2047.,None).with_id(1)); frame(&mut chart);
    /// let plan = chart.plan_trade_replay_checkpoint()?;
    /// assert!(plan.max_bytes() <= 8 * 1024 * 1024);
    /// let mark = chart.capture_trade_replay_checkpoint(plan)?;
    /// let next = chart.plan_trade_replay_checkpoint()?;
    /// sink.push_tick(Tick::new(time, 11., 7., None).with_id(2));
    /// frame(&mut chart);
    /// assert!(matches!(chart.capture_trade_replay_checkpoint(next),
    ///     Err(TradeReplayCheckpointError::StalePlan)));
    /// chart.restore_trade_replay_checkpoint(&mark)?;
    /// // Resume the matching recording at the next event, not the next timestamp.
    /// # Ok(()) }
    /// ```
    pub fn plan_trade_replay_checkpoint(&self) -> Result<TradeReplayCheckpointPlan, Error> {
        let key = CheckpointKey::new(self)?;
        let link = self.feed_link.as_ref().ok_or(Error::Unsupported)?;
        validate_tape(self, link)?;
        let subscription = link.subscription.ok_or(Error::NotDrained)?;
        let sink = link.inbox.checkpoint_guard(subscription)?;
        let footprint = footprint_guard(link, &sink)?;
        let footprint_bytes = match &footprint {
            Some(f) => f.heap_bytes().ok_or(Error::Unsupported)?,
            None => 0,
        };
        #[cfg(feature = "studies")]
        let guards = rate_guards(link)?;
        let bytes = size_of::<TradeReplayCheckpoint>()
            .checked_add(key.heap_bytes())
            .and_then(|n| n.checked_add(size_of_val(self.bars(SeriesId::MAIN))))
            .and_then(|n| {
                n.checked_add(
                    self.from_ticks
                        .len()
                        .checked_mul(size_of::<(DateTime<Utc>, Origin)>())?,
                )
            })
            .and_then(|n| n.checked_add(link.trades.checkpoint_bytes()?))
            .and_then(|n| n.checked_add(footprint_bytes))
            .and_then(|n| n.checked_add(sink.heap_bytes()))
            .ok_or(Error::Unsupported)?;
        #[cfg(feature = "studies")]
        let bytes = guards
            .iter()
            .try_fold(bytes, |n, g| {
                n.checked_add(
                    size_of::<RateGuard<'_>>() + size_of::<u64>() + size_of::<PackedTradeRate>(),
                )?
                .checked_add(g.heap_bytes()?)
            })
            .ok_or(Error::Unsupported)?;
        Ok(TradeReplayCheckpointPlan {
            key,
            subscription,
            stamp: Stamp::new(self, link, &sink),
            bytes,
            #[cfg(feature = "studies")]
            inputs: guards.iter().map(|g| g.revision()).collect(),
        })
    }

    /// Consume this chart's single-use plan. The feed and connected inputs are checked and
    /// held stable while copying: a changed chart, subscription or execution position refuses
    /// the plan. On refusal no checkpoint is produced and chart and sink are untouched.
    pub fn capture_trade_replay_checkpoint(
        &self,
        plan: TradeReplayCheckpointPlan,
    ) -> Result<TradeReplayCheckpoint, Error> {
        let link = self.feed_link.as_ref().ok_or(Error::StalePlan)?;
        if link.subscription != Some(plan.subscription) || !plan.key.matches(self).unwrap_or(false)
        {
            return Err(Error::StalePlan);
        }
        validate_tape(self, link).map_err(|_| Error::StalePlan)?;
        let sink = link
            .inbox
            .checkpoint_guard(plan.subscription)
            .map_err(|_| Error::StalePlan)?;
        if Stamp::new(self, link, &sink) != plan.stamp {
            return Err(Error::StalePlan);
        }
        let footprint = footprint_guard(link, &sink).map_err(|_| Error::StalePlan)?;
        #[cfg(feature = "studies")]
        let rates = {
            let guards = rate_guards(link).map_err(|_| Error::StalePlan)?;
            if guards.len() != plan.inputs.len() {
                return Err(Error::StalePlan);
            }
            if !guards
                .iter()
                .map(|g| g.revision())
                .eq(plan.inputs.iter().copied())
            {
                return Err(Error::StalePlan);
            }
            guards.iter().map(|g| g.snapshot()).collect()
        };
        let mut origins = Vec::with_capacity(self.from_ticks.len());
        origins.extend(self.from_ticks.iter().map(|(&t, &o)| (t, o)));
        Ok(TradeReplayCheckpoint {
            key: plan.key,
            sink: sink.capture(),
            tape: link.trades.pack_checkpoint(),
            footprint: footprint.as_ref().map(LockedFootprint::capture),
            bars: self.bars(SeriesId::MAIN).to_vec(),
            origins,
            time: link.time,
            pushed_from: link.pushed_from,
            close_time: self.bar_close_time,
            progress: plan.stamp.progress,
            #[cfg(feature = "studies")]
            rates,
        })
    }

    /// Resume the saved execution boundary on this or a second compatible drained chart.
    /// The host verifies the recording and exact event position. Unlike the capture plan,
    /// the checkpoint is reusable. A pending update, unsupported state or incompatible
    /// instrument, grouping, retention or studies refuses before changing the chart or sink.
    /// This restores replay data and trade counters, not the destination's connection status,
    /// symbol catalog revision, last feed error, viewport or drawing settings.
    pub fn restore_trade_replay_checkpoint(
        &mut self,
        checkpoint: &TradeReplayCheckpoint,
    ) -> Result<(), Error> {
        if !checkpoint.key.matches(self)? {
            return Err(Error::Incompatible);
        }
        let link = self.feed_link.as_ref().ok_or(Error::Unsupported)?;
        validate_tape(self, link)?;
        let subscription = link.subscription.ok_or(Error::NotDrained)?;
        // Own the Arc independently of &self so that this guard spans the complete commit.
        let inbox = link.inbox.clone();
        let mut sink = inbox.checkpoint_guard(subscription)?;
        if !sink.compatible(&checkpoint.sink) || !checkpoint.tape.compatible(&link.trades) {
            return Err(Error::Incompatible);
        }
        let footprint = footprint_guard(link, &sink)?;
        match (&footprint, &checkpoint.footprint) {
            (None, None) => {}
            (Some(f), Some(saved))
                if f.input.compatible(&saved.input) && saved.display.compatible(f.display) => {}
            _ => return Err(Error::Incompatible),
        }
        #[cfg(feature = "studies")]
        let revisions = {
            let mut guards = rate_guards(link)?;
            if guards.len() != checkpoint.rates.len()
                || !guards
                    .iter()
                    .zip(&checkpoint.rates)
                    .all(|(g, s)| g.compatible(s))
            {
                return Err(Error::Incompatible);
            }
            guards
                .iter_mut()
                .zip(&checkpoint.rates)
                .map(|(g, s)| {
                    g.restore(s);
                    g.revision()
                })
                .collect::<Vec<_>>()
        };
        let restored = footprint
            .zip(checkpoint.footprint.as_deref())
            .map(|(mut f, saved)| {
                f.input.restore(&saved.input, &link.clock);
                (f.input.revision(), saved.display.restore(&link.clock))
            });
        sink.restore(&checkpoint.sink);
        let link = self.feed_link.as_mut().unwrap();
        checkpoint.tape.restore(&mut link.trades);
        link.time = checkpoint.time;
        link.pushed_from = checkpoint.pushed_from;
        link.trades_complete = checkpoint.progress.trades_complete;
        link.trades_from_start = checkpoint.progress.trades_from_start;
        #[cfg(feature = "studies")]
        for ((_, seen), revision) in link.study_inputs.iter_mut().zip(revisions) {
            *seen = revision;
        }
        if let Some((revision, display)) = restored {
            // The pump registered this same input last; both readers have observed the mark.
            link.study_inputs.last_mut().unwrap().1 = revision;
            link.footprint_input.as_mut().unwrap().1 = revision;
            link.footprint = Some(display);
        }
        self.replace_main_data(checkpoint.bars.clone());
        self.from_ticks.extend(checkpoint.origins.iter().copied());
        self.bar_close_time = checkpoint.close_time;
        self.feed.history_complete = checkpoint.progress.history_complete;
        self.feed.rejected_bars = checkpoint.progress.rejected_bars;
        self.feed.late_ticks = checkpoint.progress.late_ticks;
        self.feed.repeated_ticks = checkpoint.progress.repeated_ticks;
        self.recalculate_studies();
        Ok(())
    }
}
