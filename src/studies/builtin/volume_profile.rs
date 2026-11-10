//! Volume profiles scoped to the viewport, saved times or exchange calendar periods.
//!
//! A profile distributes the volume of each chart bar over the price rows between its low and
//! high, which estimates where the volume traded. With a finer source interval the study asks
//! the host for that history, through [`Indicator::data_requests`], and distributes each finer
//! bar instead; a chart bar without finer bars keeps the estimate, so no volume is lost.
//! While the source interval follows the chart, the study reads the interval the chart
//! instrument suggests for the length of its profiles,
//! [`Instrument::profile_resolutions`](crate::model::Instrument::profile_resolutions).
//! [`ProfileInput`] tells how much of a profile the finer bars cover.
//! The chart notes each profile "Estimated from bars", also when finer bars cover all of it:
//! they narrow the estimate without making it exact.
//! Where the chart holds the executions of a profile's bars, through
//! [`Indicator::order_flow_inputs`], the profile sums them instead, in rows of whole ticks: the
//! Height input rounded to whole ticks, or the traded ticks over Row count, rounded up and widened
//! where rows counted from multiples of that height would exceed Row count.
//! A profile whose executions start after its first bar sums those held and reports itself
//! partial; one without them, or whose bars do not each hold their own executions, keeps the bars.
//!
//! Row fills stay translucent so price bars remain readable: the painter limits saved opaque colours
//! to the opacity of the default value area rows. The point of control is a 2 px line in the theme's
//! foreground colour until a line colour is chosen.

use super::anchor_period::{AnchorPeriod, ExchangeDays};
use super::{AutoAnchor, first_trade_in};
use crate::data::{
    FootprintBar, FootprintLevel, OrderFlowBars, OrderFlowInput, ProfileDisplay, ProfileLevel,
    ProfileLevelKind, ProfileSource, StudyProfile, VolumeProfile, VolumeProfileConfig,
};
use crate::model::{Bar, BarClock, Instrument, ProfileResolution, Timeframe};
use crate::studies::{
    Indicator, IndicatorValues, InputKind, InputValue, StudyData, StudyDataRequest, StudyInput,
};
use chrono::{DateTime, TimeDelta, Utc};
use egui::Color32;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::{Bound, Range};
use std::sync::Arc;

/// What a volume profile was built from.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileInput {
    /// The chart bars the profile covers.
    pub bars: Range<usize>,
    /// The share of those bars, from 0 to 1, whose volume comes from finer bars of the source
    /// interval. The volume of the other bars is an estimate spread over each bar's high to
    /// low range, so 0 marks a profile that is an estimate throughout, unless its
    /// [`source`](crate::data::VolumeProfile::source) is its executions, which take no finer bars.
    pub coverage: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Visible,
    Fixed,
    Session,
    Periodic,
    Anchored,
    Auto,
}

#[derive(Clone)]
struct ProfileStudy {
    kind: Kind,
    rows: usize,
    value_area: f64,
    row_size: f64,
    /// Share of a session's or period's width that its widest row takes.
    width: f64,
    extend: [bool; 3],
    display: ProfileDisplay,
    start_time: f64,
    end_time: f64,
    period: AnchorPeriod,
    anchor: AutoAnchor,
    length: usize,
    interval: Timeframe,
    /// Bar calendar, from `set_bar_clock`, by which each bar's executions are found.
    clock: BarClock,
    source: Option<Timeframe>,
    /// Read the interval the instrument suggests while `source` follows the chart.
    feed_source: bool,
    adaptive: bool,
    tick_size: Option<f64>,
    resolutions: Vec<ProfileResolution>,
    /// The finer interval and data start stated before the chart emptied the profile to wait
    /// for its full recompute, still stated until it has bars again.
    held: Option<(Option<Timeframe>, Option<DateTime<Utc>>)>,
    /// Chart bars in view, from `set_visible_range`.
    view: Range<usize>,
    days: ExchangeDays,
    data: Vec<Bar>,
    fine: Vec<Bar>,
    /// Each bar's executions by price, one level per tick. Clones share the chart-fed source.
    flow: OrderFlowInput,
    ranges: Vec<Range<usize>>,
    cache: RefCell<Vec<CachedProfile>>,
    /// Levels of finished profiles, kept while their bars and row layout hold.
    levels: RefCell<LevelCache>,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

#[derive(Clone)]
struct CachedProfile {
    plot: StudyProfile,
    opening: Option<VolumeProfile>,
    coverage: f64,
    dirty: bool,
}

/// The kinds of level the extend inputs carry, in their order.
const LEVELS: [ProfileLevelKind; 3] = [
    ProfileLevelKind::PointOfControl,
    ProfileLevelKind::ValueAreaHigh,
    ProfileLevelKind::ValueAreaLow,
];

/// Levels of finished profiles, kept while their bars and row layout hold.
#[derive(Clone, Default)]
struct LevelCache {
    /// The row layout the levels were found with.
    config: Option<VolumeProfileConfig>,
    /// Each finished profile's end and levels, by its first bar.
    profiles: BTreeMap<usize, (usize, Vec<Level>)>,
}

/// A level of a finished profile while it extends.
#[derive(Clone)]
struct Level {
    kind: ProfileLevelKind,
    price: f64,
    /// The profile row in which a trade ends the level.
    row: Range<f64>,
    /// The first bar after the profile that trades in `row`, once found.
    touch: Option<usize>,
    /// Bars from the profile's end up to this one trade outside `row`.
    searched: usize,
}

const PERIODS: [AnchorPeriod; 7] = [
    AnchorPeriod::Session,
    AnchorPeriod::Week,
    AnchorPeriod::Month,
    AnchorPeriod::Quarter,
    AnchorPeriod::Year,
    AnchorPeriod::Decade,
    AnchorPeriod::Century,
];
const ANCHORS: [AutoAnchor; 11] = [
    AutoAnchor::Auto,
    AutoAnchor::HighestHigh,
    AutoAnchor::LowestLow,
    AutoAnchor::HighestVolume,
    AutoAnchor::Session,
    AutoAnchor::Week,
    AutoAnchor::Month,
    AutoAnchor::Quarter,
    AutoAnchor::Year,
    AutoAnchor::Decade,
    AutoAnchor::Century,
];
const DISPLAYS: [ProfileDisplay; 3] = [
    ProfileDisplay::Total,
    ProfileDisplay::UpDown,
    ProfileDisplay::Delta,
];

/// UTC times in Inputs, with a fractional second only when one is present.
const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.f";

/// Saved milliseconds as text, empty for the unset endpoints.
pub(crate) fn time_text(ms: f64) -> String {
    if ms.abs() >= 8e15 {
        return String::new();
    }
    DateTime::from_timestamp_millis(ms as i64)
        .map_or_else(String::new, |time| time.format(TIME_FORMAT).to_string())
}

/// UTC date and time to the minute or finer, or `unset` for empty text.
fn parse_time(text: &str, unset: f64) -> Result<f64, &'static str> {
    if text.is_empty() {
        return Ok(unset);
    }
    chrono::NaiveDateTime::parse_from_str(text, TIME_FORMAT)
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M"))
        .map(|time| time.and_utc().timestamp_millis() as f64)
        .map_err(|_| "time must read YYYY-MM-DD HH:MM:SS")
}

impl ProfileStudy {
    fn new(kind: Kind, rows: usize) -> Self {
        Self {
            kind,
            rows: rows.max(1),
            value_area: 0.7,
            row_size: 0.0,
            width: 0.3,
            extend: [false; 3],
            display: ProfileDisplay::UpDown,
            start_time: -8e15,
            end_time: 8e15,
            period: AnchorPeriod::Week,
            anchor: AutoAnchor::Auto,
            length: 100,
            interval: Timeframe::Min1,
            clock: BarClock::new(Timeframe::Min1),
            source: None,
            feed_source: true,
            adaptive: false,
            tick_size: None,
            resolutions: Vec::new(),
            held: None,
            view: 0..0,
            days: ExchangeDays::default(),
            data: Vec::new(),
            fine: Vec::new(),
            flow: OrderFlowInput::footprint(),
            ranges: Vec::new(),
            cache: RefCell::new(Vec::new()),
            levels: RefCell::default(),
            values: IndicatorValues::new(),
            colors: vec![
                Color32::PLACEHOLDER,
                Color32::from_rgba_unmultiplied(0, 188, 212, 110),
                Color32::from_rgba_unmultiplied(233, 30, 99, 110),
                Color32::from_rgba_unmultiplied(0, 188, 212, 163),
                Color32::from_rgba_unmultiplied(233, 30, 99, 163),
            ],
            visible: true,
        }
    }

    fn inputs(&self) -> Vec<StudyInput> {
        let mut inputs = vec![
            StudyInput::new(
                "row_count",
                "Row count",
                InputValue::Length(self.rows),
                InputKind::Length(1, 1000),
            ),
            StudyInput::new(
                "value_area",
                "Value area (%)",
                InputValue::Number(self.value_area * 100.0),
                InputKind::Number(0.01, 100.0),
            ),
            StudyInput::new(
                "row_size",
                "Height (0 = equal)",
                InputValue::Number(self.row_size),
                InputKind::Number(0.0, f64::MAX),
            ),
            StudyInput::new(
                "display",
                "Volume",
                InputValue::Choice(DISPLAYS.iter().position(|d| *d == self.display).unwrap()),
                InputKind::Choice(vec!["Total".into(), "Up / down".into(), "Delta".into()]),
            ),
            StudyInput::new(
                "source_interval",
                "Source interval",
                InputValue::Timeframe(self.source),
                InputKind::Timeframe,
            ),
            StudyInput::new(
                "feed_source",
                "Feed suggested interval",
                InputValue::Toggle(self.feed_source),
                InputKind::Toggle,
            ),
        ];
        if matches!(self.kind, Kind::Fixed | Kind::Anchored) {
            inputs.push(StudyInput::new(
                "start_time",
                "Start (YYYY-MM-DD HH:MM:SS UTC)",
                InputValue::Text(time_text(self.start_time)),
                InputKind::Text,
            ));
        }
        if self.kind == Kind::Fixed {
            inputs.push(StudyInput::new(
                "end_time",
                "End (YYYY-MM-DD HH:MM:SS UTC)",
                InputValue::Text(time_text(self.end_time)),
                InputKind::Text,
            ));
        }
        if self.kind == Kind::Periodic {
            inputs.push(StudyInput::new(
                "period",
                "Period",
                InputValue::Choice(PERIODS.iter().position(|p| *p == self.period).unwrap()),
                InputKind::Choice(PERIODS.iter().map(|p| format!("{p:?}")).collect()),
            ));
        }
        if self.kind == Kind::Auto {
            inputs.push(StudyInput::new(
                "anchor",
                "Anchor",
                InputValue::Choice(ANCHORS.iter().position(|a| *a == self.anchor).unwrap()),
                InputKind::Choice(ANCHORS.iter().map(|a| format!("{a:?}")).collect()),
            ));
            inputs.push(StudyInput::new(
                "length",
                "Lookback bars",
                InputValue::Length(self.length),
                InputKind::Length(1, usize::MAX),
            ));
        }
        if self.kind == Kind::Session {
            inputs.push(StudyInput::new(
                "adaptive_rows",
                "Adapt rows to zoom",
                InputValue::Toggle(self.adaptive),
                InputKind::Toggle,
            ));
        }
        if matches!(self.kind, Kind::Session | Kind::Periodic) {
            inputs.push(StudyInput::new(
                "width",
                "Width (%)",
                InputValue::Number(self.width * 100.0),
                InputKind::Number(1.0, 100.0),
            ));
            for (i, (id, label)) in [
                ("extend_poc", "Extend POC right"),
                ("extend_vah", "Extend VAH right"),
                ("extend_val", "Extend VAL right"),
            ]
            .into_iter()
            .enumerate()
            {
                inputs.push(StudyInput::new(
                    id,
                    label,
                    InputValue::Toggle(self.extend[i]),
                    InputKind::Toggle,
                ));
            }
        }
        inputs
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), crate::studies::StudyError> {
        if !self
            .inputs()
            .iter()
            .any(|input| input.id == id && input.accepts(&value))
        {
            return Err(crate::studies::StudyError::rejected(&self.inputs(), id));
        }
        match (id, value) {
            ("row_count", InputValue::Length(v)) => self.rows = v,
            ("value_area", InputValue::Number(v)) => self.value_area = v / 100.0,
            ("row_size", InputValue::Number(v)) => self.row_size = v,
            ("display", InputValue::Choice(v)) => self.display = DISPLAYS[v],
            ("source_interval", InputValue::Timeframe(v)) => self.source = v,
            ("feed_source", InputValue::Toggle(v)) => self.feed_source = v,
            ("adaptive_rows", InputValue::Toggle(v)) => self.adaptive = v,
            ("start_time", InputValue::Text(text)) => {
                self.start_time = parse_time(&text, -8e15).map_err(|reason| {
                    crate::studies::StudyError::InvalidInput {
                        id: id.into(),
                        reason: reason.into(),
                    }
                })?
            }
            ("end_time", InputValue::Text(text)) => {
                self.end_time = parse_time(&text, 8e15).map_err(|reason| {
                    crate::studies::StudyError::InvalidInput {
                        id: id.into(),
                        reason: reason.into(),
                    }
                })?
            }
            ("period", InputValue::Choice(v)) => self.period = PERIODS[v],
            ("anchor", InputValue::Choice(v)) => self.anchor = ANCHORS[v],
            ("length", InputValue::Length(v)) => self.length = v,
            ("width", InputValue::Number(v)) => self.width = v / 100.0,
            ("extend_poc", InputValue::Toggle(v)) => self.extend[0] = v,
            ("extend_vah", InputValue::Toggle(v)) => self.extend[1] = v,
            ("extend_val", InputValue::Toggle(v)) => self.extend[2] = v,
            _ => return Err(crate::studies::StudyError::invalid_input(id)),
        }
        self.values.clear();
        self.data.clear();
        self.held = None;
        self.fine.clear();
        self.ranges.clear();
        self.cache.get_mut().clear();
        *self.levels.get_mut() = LevelCache::default();
        Ok(())
    }

    /// An explicit interval shorter than the chart's, or the shortest suggested interval
    /// that serves the profile length while the source follows the chart; the held one while
    /// emptied for a recompute.
    fn finer(&self) -> Option<Timeframe> {
        let shorter = |source: &Timeframe| {
            !source.is_trade_based() && source.duration_ms() < self.interval.duration_ms()
        };
        // The chart interval may have changed since.
        if let Some((timeframe, _)) = self.held.filter(|_| self.data.is_empty()) {
            return timeframe.filter(shorter);
        }
        if self.source.is_some() || !self.feed_source || self.resolutions.is_empty() {
            return self.source.filter(shorter);
        }
        ProfileResolution::suggested(&self.resolutions, self.interval, self.span()?)
    }

    /// Profile length from first open to last close: visible bars for a visible profile,
    /// the longest range otherwise.
    fn span(&self) -> Option<TimeDelta> {
        let span = |range: &Range<usize>| {
            let end = range.end.min(self.data.len());
            let first = self.data.get(range.start).filter(|_| range.start < end)?;
            Some(self.close(self.data[end - 1].time) - first.time)
        };
        if self.kind == Kind::Visible {
            span(&self.view)
        } else {
            self.ranges.iter().filter_map(span).max()
        }
    }

    /// The close of the chart bar that opens at `open`, saturating past the calendar's end.
    fn close(&self, open: DateTime<Utc>) -> DateTime<Utc> {
        open.checked_add_signed(self.interval.duration())
            .unwrap_or(DateTime::<Utc>::MAX_UTC)
    }

    fn data_requests(&self) -> Vec<StudyDataRequest> {
        self.finer()
            .map(|timeframe| StudyDataRequest {
                symbol: String::new(),
                timeframe: Some(timeframe),
                metric: None,
            })
            .into_iter()
            .collect()
    }

    /// The chart bar ranges of the profiles in `visible`: the clamped view for a visible
    /// profile, the ranges overlapping it otherwise.
    fn ranges_in(&self, visible: Range<usize>) -> impl Iterator<Item = Range<usize>> + '_ {
        let visible = visible.start.min(self.data.len())..visible.end.min(self.data.len());
        let own = (self.kind == Kind::Visible && !visible.is_empty()).then(|| visible.clone());
        // A visible profile has no ranges of its own.
        let overlapping = self.ranges.iter().filter(move |range| {
            !visible.is_empty() && range.start < visible.end && range.end > visible.start
        });
        own.into_iter().chain(overlapping.cloned())
    }

    /// The open of the first chart bar a profile in view reads, from which its finer bars are
    /// needed; the open of the newest bar with no profile in view, the held start while emptied
    /// for a recompute, and `None` without bars otherwise; the first bar while a level extends.
    fn data_start(&self) -> Option<DateTime<Utc>> {
        if self.data.is_empty() {
            return self.held.and_then(|(_, start)| start);
        }
        // A finished profile anywhere left of the view can carry a level into it.
        if self.extend.contains(&true) {
            return self.data.first().map(|bar| bar.time);
        }
        self.ranges_in(self.view.clone())
            .map(|range| range.start)
            .min()
            .map_or(self.data.last(), |at| self.data.get(at))
            .map(|bar| bar.time)
    }

    /// Takes the finer bars, replacing all of them from bar 0 and from the first changed chart
    /// bar on otherwise, so a reply for another interval is dropped.
    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
        let wanted = self.finer();
        match sources
            .iter()
            .find(|source| Some(source.timeframe) == wanted)
        {
            Some(source) if from > 0 && !data.is_empty() => {
                let cut = data[from.min(data.len() - 1)].time;
                self.fine
                    .truncate(self.fine.partition_point(|bar| bar.time < cut));
                let skip = source.bars.partition_point(|bar| bar.time < cut);
                self.fine.extend_from_slice(&source.bars[skip..]);
            }
            Some(source) => self.fine = source.bars.to_vec(),
            None => self.fine.clear(),
        }
        self.update(data, from);
    }

    /// What each profile in `visible` was built from.
    fn profile_inputs(&self, visible: Range<usize>) -> Vec<ProfileInput> {
        self.profiles(visible)
            .into_iter()
            .map(|profile| ProfileInput {
                coverage: self
                    .cache
                    .borrow()
                    .iter()
                    .find(|cached| cached.plot.bars == profile.bars)
                    .map_or(0.0, |cached| cached.coverage),
                bars: profile.bars,
            })
            .collect()
    }

    /// The executions the chart holds from the bar before chart bar `start` on, while the source
    /// opens its bars by this study's calendar; executions keyed by another calendar, as the
    /// chart's are while the study runs on another interval, cannot be assigned to these bars.
    fn held_executions(&self, start: usize) -> Option<OrderFlowBars<FootprintBar>> {
        if self.flow.footprint_clock().as_ref() != Some(&self.clock) {
            return None;
        }
        let from = start
            .checked_sub(1)
            .map_or(DateTime::<Utc>::MIN_UTC, |before| {
                self.clock.bar_open(self.data[before].time)
            });
        // shortcut: viewport queries copy up to 110,000 held rows; retain the copy by revision
        // if panning shows the cost.
        Some(self.flow.footprints_from(from))
    }

    /// The executions of chart bars `range` in `flow`, merged per tick, and whether `flow` holds
    /// all of them. `None` unless it holds some and every bar that can hold them is a bar of
    /// `flow`: stamped at its open, in a slot of its own and apart from the bars beside the
    /// range, with no executions held in a slot that no bar of the range opens. Bars of
    /// price-based chart types and bars the chart lacks so keep the estimate.
    fn executions_of(
        &self,
        range: &Range<usize>,
        flow: &OrderFlowBars<FootprintBar>,
    ) -> Option<(FootprintBar, bool)> {
        let open = |bar: &Bar| self.clock.bar_open(bar.time);
        let (start, end) = (
            open(&self.data[range.start]),
            open(&self.data[range.end - 1]),
        );
        let before = range.start.checked_sub(1).map(|i| open(&self.data[i]));
        // A bar beside the range in one of its slots shares that slot's executions.
        if start > end
            || before.is_some_and(|before| before >= start)
            || self
                .data
                .get(range.end)
                .is_some_and(|after| open(after) <= end)
        {
            return None;
        }
        let lower = before.map_or(Bound::Unbounded, Bound::Excluded);
        let held = flow.bars.range((lower, Bound::Included(end)));
        let first = *held.clone().next()?.0;
        // Bars stamped before the first held bar hold none of the executions.
        let from = range.start + self.data[range.clone()].partition_point(|bar| bar.time < first);
        let mut opens = Vec::with_capacity(range.end - from);
        for bar in &self.data[from..range.end] {
            let open = open(bar);
            if open != bar.time || opens.last().is_some_and(|&last| last >= open) {
                return None;
            }
            opens.push(open);
        }
        let mut opens = opens.into_iter().peekable();
        let mut merged = FootprintBar::empty(start);
        for (&time, bar) in held {
            while opens.next_if(|&open| open < time).is_some() {}
            // Executions in a slot that no bar of the range opens belong to bars it lacks.
            if opens.peek() != Some(&time) {
                return None;
            }
            merged.absorb(bar);
        }
        // During a replay the bar containing the cutoff is absent when its executions before the
        // cutoff are not known.
        let unknown = flow.cutoff.is_some_and(|cutoff| {
            end <= cutoff
                && self.clock.next_open(end).is_none_or(|next| cutoff < next)
                && !flow.bars.contains_key(&end)
        });
        let complete = flow.complete_from.is_some_and(|from| from <= start) && !unknown;
        Some((merged, complete))
    }

    /// The profile of chart bars `range` from the executions in `flow`, in rows of whole ticks
    /// (`ticks_per_row`). `None` without them ([`Self::executions_of`]), with a volume that is
    /// not finite, or past 10,000 rows.
    fn executed_profile(
        &self,
        range: &Range<usize>,
        config: &VolumeProfileConfig,
        flow: &OrderFlowBars<FootprintBar>,
    ) -> Option<VolumeProfile> {
        let tick = flow.tick_size?;
        let (bar, complete) = self.executions_of(range, flow)?;
        let (&first, &last) = (bar.levels.keys().next()?, bar.levels.keys().next_back()?);
        if !bar.volume().is_finite() {
            return None;
        }
        let ticks = ticks_per_row(config, tick, first, last)?;
        VolumeProfile::from_executions(&grouped(&bar, ticks), ticks as f64 * tick, tick, complete)
    }

    fn period_ranges(&self, period: AnchorPeriod) -> Vec<Range<usize>> {
        let mut ranges = Vec::new();
        let mut previous = None;
        let mut start = 0;
        for (i, day) in self.days.of(&self.data).into_iter().enumerate() {
            let key = period.key(day);
            if previous.is_some_and(|previous| previous != key) {
                ranges.push(start..i);
                start = i;
            }
            previous = Some(key);
        }
        if start < self.data.len() {
            ranges.push(start..self.data.len());
        }
        ranges
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.data = data.to_vec();
        self.values = IndicatorValues::missing(data.len());
        self.cache.get_mut().clear();
        *self.levels.get_mut() = LevelCache::default();
        self.refresh_ranges();
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if data.is_empty() && !self.data.is_empty() {
            // Keeps requesting what it read, so the chart keeps those bars for the recompute.
            self.held = Some((self.finer(), self.data_start()));
        }
        let from = from.min(data.len()).min(self.data.len());
        let times_changed = self.data.len() != data.len()
            || self.data[from..]
                .iter()
                .zip(&data[from..])
                .any(|(a, b)| a.time != b.time);
        self.data.truncate(from);
        self.data.extend_from_slice(&data[from..]);
        self.values = IndicatorValues::missing(data.len());
        self.cache.get_mut().retain_mut(|cached| {
            if cached.plot.bars.end.saturating_sub(1) > from {
                return false;
            }
            cached.dirty |= cached.plot.bars.end > from;
            true
        });
        // Profiles with a bar from `from` on are found again; the search for a level's first
        // trade resumes where a bar may have changed.
        let levels = self.levels.get_mut();
        levels.profiles.retain(|_, (end, _)| *end <= from);
        for level in levels.profiles.values_mut().flat_map(|(_, levels)| levels) {
            if level.touch.is_some_and(|touch| touch >= from) {
                level.touch = None;
            }
            level.searched = level.searched.min(from);
        }
        if times_changed
            || (self.kind == Kind::Auto
                && matches!(
                    self.anchor,
                    AutoAnchor::HighestHigh | AutoAnchor::LowestLow | AutoAnchor::HighestVolume
                ))
        {
            self.refresh_ranges();
        }
    }

    fn refresh_ranges(&mut self) {
        let data = &self.data;
        self.ranges = match self.kind {
            Kind::Visible => Vec::new(),
            Kind::Fixed | Kind::Anchored => {
                let start = data
                    .partition_point(|bar| (bar.time.timestamp_millis() as f64) < self.start_time);
                let end = if self.kind == Kind::Fixed {
                    data.partition_point(|bar| {
                        (bar.time.timestamp_millis() as f64) <= self.end_time
                    })
                } else {
                    data.len()
                };
                if start < end {
                    std::iter::once(start..end).collect()
                } else {
                    Vec::new()
                }
            }
            Kind::Session => self.period_ranges(AnchorPeriod::Session),
            Kind::Periodic => self.period_ranges(self.period),
            Kind::Auto => {
                let period = match self.anchor {
                    AutoAnchor::Auto => {
                        let day = Timeframe::Day1.duration_ms();
                        Some(match self.interval.duration_ms() {
                            ms if ms < day => AnchorPeriod::Session,
                            ms if ms <= day => AnchorPeriod::Month,
                            ms if ms <= 10 * day => AnchorPeriod::Quarter,
                            ms if ms <= 60 * day => AnchorPeriod::Year,
                            _ => AnchorPeriod::Decade,
                        })
                    }
                    AutoAnchor::HighestHigh | AutoAnchor::LowestLow | AutoAnchor::HighestVolume => {
                        None
                    }
                    _ => ANCHORS
                        .iter()
                        .position(|a| *a == self.anchor)
                        .map(|i| PERIODS[i - 4]),
                };
                if let Some(period) = period {
                    self.period_ranges(period).pop().into_iter().collect()
                } else {
                    let metric = |bar: &Bar| match self.anchor {
                        AutoAnchor::HighestHigh => bar.high,
                        AutoAnchor::LowestLow => -bar.low,
                        _ => bar.volume,
                    };
                    (data.len().saturating_sub(self.length)..data.len())
                        .filter(|&i| metric(&data[i]).is_finite())
                        .max_by(|&a, &b| metric(&data[a]).total_cmp(&metric(&data[b])))
                        .map(|start| start..data.len())
                        .into_iter()
                        .collect()
                }
            }
        };
        // A profile whose range changed holds no levels.
        let ranges = &self.ranges;
        self.levels.get_mut().profiles.retain(|start, (end, _)| {
            ranges
                .binary_search_by_key(start, |range| range.start)
                .is_ok_and(|i| ranges[i].end == *end)
        });
    }

    /// One row height for the chart-bar envelope of every session in view, rounded up
    /// to 1, 2 or 5 times a power of ten ticks, or of price without a tick size.
    fn adaptive_height(&self, ranges: &[Range<usize>]) -> Option<f64> {
        let (low, high) = ranges
            .iter()
            .filter_map(|range| self.data.get(range.clone()))
            .flatten()
            .filter(|bar| bar.low.is_finite() && bar.high.is_finite() && bar.high >= bar.low)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), bar| {
                (low.min(bar.low), high.max(bar.high))
            });
        let raw = (high - low) / self.rows as f64;
        if !(raw.is_finite() && raw > 0.0) {
            return None;
        }
        match self.tick_size {
            Some(tick) => nice_ceil((raw / tick).max(1.0)).map(|ticks| tick * ticks),
            None => nice_ceil(raw),
        }
    }

    fn profiles(&self, visible: Range<usize>) -> Vec<StudyProfile> {
        let ranges: Vec<_> = self.ranges_in(visible).collect();
        if ranges.is_empty() {
            return Vec::new();
        }
        self.config(&ranges)
            .map_or_else(Vec::new, |config| self.profiles_of(ranges, config))
    }

    /// The row layout, value area and colours of profiles built while `ranges` are in view;
    /// `None` for settings no profile takes.
    fn config(&self, ranges: &[Range<usize>]) -> Option<VolumeProfileConfig> {
        let mut config = VolumeProfileConfig::new(self.rows, self.value_area)
            .and_then(|c| c.with_row_size((self.row_size > 0.0).then_some(self.row_size)))?;
        if self.adaptive
            && let Some(height) = self.adaptive_height(ranges)
        {
            config = config.with_row_size(Some(height)).unwrap_or(config);
        }
        config.display = self.display;
        config.up_color = self.colors[1].to_srgba_unmultiplied();
        config.down_color = self.colors[2].to_srgba_unmultiplied();
        config.value_area_colors = Some([
            self.colors[3].to_srgba_unmultiplied(),
            self.colors[4].to_srgba_unmultiplied(),
        ]);
        Some(config)
    }

    /// The profiles of `ranges` under `config`, from the cache where it holds them.
    fn profiles_of(
        &self,
        ranges: Vec<Range<usize>>,
        config: VolumeProfileConfig,
    ) -> Vec<StudyProfile> {
        let width = match self.kind {
            Kind::Visible => 1.0,
            Kind::Session | Kind::Periodic => self.width as f32,
            Kind::Fixed | Kind::Anchored | Kind::Auto => 0.9,
        };
        let mut cache = self.cache.borrow_mut();
        let mut profiles = Vec::new();
        // Copied once, from the bar before the first profile built here on.
        let mut held: Option<Option<OrderFlowBars<FootprintBar>>> = None;
        for range in ranges {
            if let Some(plot) = cache.iter().find(|cached| {
                cached.plot.bars == range && cached.plot.config == config && !cached.dirty
            }) {
                profiles.push(plot.plot.clone());
            } else if let Some(profile) = held
                .get_or_insert_with(|| self.held_executions(range.start))
                .as_ref()
                .and_then(|flow| self.executed_profile(&range, &config, flow))
            {
                let plot = StudyProfile {
                    bars: range.clone(),
                    profile: Arc::new(profile),
                    config,
                    color: self.colors[0].to_srgba_unmultiplied(),
                    right_aligned: self.kind == Kind::Visible,
                    width,
                };
                profiles.push(plot.clone());
                cache.retain(|cached| cached.plot.bars != range);
                cache.push(CachedProfile {
                    plot,
                    opening: None,
                    coverage: 0.0,
                    dirty: false,
                });
            } else if self.finer().is_some() && !self.fine.is_empty() {
                let end = self.data.get(range.end).map_or_else(
                    || self.close(self.data[range.end - 1].time),
                    |next| next.time,
                );
                let (profile, covered) = VolumeProfile::of_finer(
                    &self.data[range.clone()],
                    end,
                    &self.fine,
                    config.rows(),
                    config.row_size(),
                );
                let coverage = covered as f64 / range.len() as f64;
                let Some(profile) = profile else {
                    continue;
                };
                let plot = StudyProfile {
                    bars: range.clone(),
                    profile: Arc::new(profile),
                    config,
                    color: self.colors[0].to_srgba_unmultiplied(),
                    right_aligned: self.kind == Kind::Visible,
                    width,
                };
                profiles.push(plot.clone());
                cache.retain(|cached| cached.plot.bars != range);
                cache.push(CachedProfile {
                    plot,
                    opening: None,
                    coverage,
                    dirty: false,
                });
            } else {
                let opening = cache
                    .iter()
                    .filter(|cached| {
                        cached.plot.bars.start == range.start
                            && cached.plot.bars.end <= range.end
                            && cached.plot.config == config
                    })
                    .max_by_key(|cached| cached.plot.bars.end)
                    .and_then(|cached| {
                        let mut opening = cached.opening.clone()?;
                        for bar in &self.data[cached.plot.bars.end - 1..range.end - 1] {
                            opening = opening.with_bar(bar)?;
                        }
                        Some(opening)
                    })
                    .or_else(|| {
                        VolumeProfile::from_bars(&self.data[range.start..range.end - 1], config)
                    });
                let profile = opening
                    .as_ref()
                    .and_then(|p| p.with_bar(&self.data[range.end - 1]))
                    .or_else(|| VolumeProfile::from_bars(&self.data[range.clone()], config));
                let Some(profile) = profile else {
                    continue;
                };
                let plot = StudyProfile {
                    bars: range.clone(),
                    profile: Arc::new(profile),
                    config,
                    color: self.colors[0].to_srgba_unmultiplied(),
                    right_aligned: self.kind == Kind::Visible,
                    width,
                };
                profiles.push(plot.clone());
                cache.retain(|cached| cached.plot.bars != range);
                cache.push(CachedProfile {
                    plot,
                    opening,
                    coverage: 0.0,
                    dirty: false,
                });
            }
        }
        // Keep neighbouring viewport queries from fitting, painting and hit testing cached.
        let keep = profiles.len().saturating_add(4);
        let excess = cache.len().saturating_sub(keep);
        cache.drain(..excess);
        profiles
    }

    /// The levels of finished profiles that reach into `visible`, each from the bar after its
    /// profile to the first later bar that trades in its row, or to the last bar.
    fn levels(&self, visible: Range<usize>) -> Vec<ProfileLevel> {
        let len = self.data.len();
        if !self.extend.contains(&true) {
            return Vec::new();
        }
        let in_view: Vec<_> = self.ranges_in(visible.clone()).collect();
        // Levels lie on the rows that the profiles in view are drawn with.
        let Some(config) = (!in_view.is_empty())
            .then(|| self.config(&in_view))
            .flatten()
        else {
            return Vec::new();
        };
        let mut cache = self.levels.borrow_mut();
        if cache.config != Some(config) {
            *cache = LevelCache {
                config: Some(config),
                profiles: BTreeMap::new(),
            };
        }
        // A finished profile anywhere left of the view's end can carry a level into it.
        // shortcut: a change of the adaptive row height rebuilds every finished profile once;
        // keep levels per height if zooming over many sessions shows the cost.
        let missing: Vec<_> = self
            .ranges
            .iter()
            .filter(|range| range.end < len && range.start < visible.end)
            .filter(|range| {
                cache
                    .profiles
                    .get(&range.start)
                    .is_none_or(|(end, _)| *end != range.end)
            })
            .cloned()
            .collect();
        if !missing.is_empty() {
            let plots = self.profiles_of(missing.clone(), config);
            for range in missing {
                let levels = plots
                    .iter()
                    .find(|plot| plot.bars == range)
                    .map_or_else(Vec::new, |plot| self.levels_of(plot));
                cache.profiles.insert(range.start, (range.end, levels));
            }
        }
        let color = self.colors[0].to_srgba_unmultiplied();
        let mut found = Vec::new();
        for (end, levels) in cache
            .profiles
            .range_mut(..visible.end)
            .map(|(_, entry)| entry)
        {
            let end = *end;
            if end >= len || end >= visible.end {
                continue;
            }
            for level in levels {
                if level.touch.is_none() && level.searched < len {
                    let from = level.searched.max(end);
                    level.touch = first_trade_in(&self.data[from..], &level.row).map(|i| from + i);
                    level.searched = len;
                }
                let last = level.touch.unwrap_or(len - 1);
                if last >= visible.start {
                    found.push(ProfileLevel {
                        bars: end..last + 1,
                        price: level.price,
                        kind: level.kind,
                        color,
                    });
                }
            }
        }
        found
    }

    /// The extended levels of the finished profile `plot`. There are none for a profile without
    /// volume, or for one from executions that miss its first bars, whose levels are not the
    /// session's.
    fn levels_of(&self, plot: &StudyProfile) -> Vec<Level> {
        let profile = &plot.profile;
        if profile.source() == (ProfileSource::Executions { complete: false }) {
            return Vec::new();
        }
        let Some((poc, (va_low, va_high))) = profile.poc_and_value_area(plot.config.value_area())
        else {
            return Vec::new();
        };
        let (low, height) = (profile.low(), profile.row_height());
        let row = |row: usize| low + row as f64 * height..low + (row + 1) as f64 * height;
        LEVELS
            .into_iter()
            .zip(self.extend)
            .filter(|(_, on)| *on)
            .map(|(kind, _)| {
                let (price, row) = match kind {
                    ProfileLevelKind::PointOfControl => {
                        (low + (poc as f64 + 0.5) * height, row(poc))
                    }
                    ProfileLevelKind::ValueAreaHigh => {
                        (low + (va_high + 1) as f64 * height, row(va_high))
                    }
                    ProfileLevelKind::ValueAreaLow => (low + va_low as f64 * height, row(va_low)),
                };
                Level {
                    kind,
                    price,
                    row,
                    touch: None,
                    searched: plot.bars.end,
                }
            })
            .collect()
    }
}

/// The least of 1, 2 and 5 times a power of ten that reaches a positive `value`.
fn nice_ceil(value: f64) -> Option<f64> {
    let power = 10_f64.powi(value.log10().floor() as i32);
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|step| step * power)
        .find(|step| *step >= value * (1.0 - 1e-9))
}

/// Ticks per row of an executed profile over the traded levels `first..=last`: the row height in
/// whole ticks, at least one. With equal rows, the levels over the row count, rounded up, when
/// rows of that many ticks, counted from its multiples as a footprint groups them, stay within
/// the row count; else the levels over one row fewer, which always do. Asked for one row, levels
/// that straddle a multiple keep two.
fn ticks_per_row(config: &VolumeProfileConfig, tick: f64, first: i64, last: i64) -> Option<i64> {
    if let Some(height) = config.row_size() {
        let ticks = (height / tick).round();
        return (ticks < i64::MAX as f64).then(|| (ticks as i64).max(1));
    }
    let (first, last, rows) = (i128::from(first), i128::from(last), config.rows() as i128);
    let count = |ticks: i128| last.div_euclid(ticks) - first.div_euclid(ticks) + 1;
    let span = last - first + 1;
    let even = (span + rows - 1) / rows;
    let ticks = if rows == 1 || count(even) <= rows {
        even
    } else {
        (span + rows - 2) / (rows - 1)
    };
    i64::try_from(ticks).ok()
}

/// `bar` with its levels summed into rows of `ticks` levels, level `n` into row
/// `n.div_euclid(ticks)`, as a footprint with that many ticks per row groups them. Totals and the
/// delta path are unchanged.
fn grouped(bar: &FootprintBar, ticks: i64) -> FootprintBar {
    let mut levels = BTreeMap::new();
    for (&level, row) in &bar.levels {
        let key = level.div_euclid(ticks);
        let target = levels.entry(key).or_insert_with(|| FootprintLevel {
            level: key,
            ..Default::default()
        });
        target.absorb(row);
    }
    FootprintBar {
        time: bar.time,
        levels,
        bid: bar.bid,
        ask: bar.ask,
        unknown: bar.unknown,
        min_delta: bar.min_delta,
        max_delta: bar.max_delta,
    }
}

macro_rules! profile_study {
    ($ty:ident, $kind:ident, $name:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone)]
        pub struct $ty {
            profile: ProfileStudy,
        }
        impl $ty {
            /// Create a profile with this many equal price rows, at least one.
            /// Counts above 1,000 produce no profile; settings accept 1 to 1,000.
            pub fn new(row_count: usize) -> Self {
                Self {
                    profile: ProfileStudy::new(Kind::$kind, row_count),
                }
            }

            /// What each profile in the `visible` chart bars was built from, with the share of
            /// its bars that the finer source interval covers.
            pub fn profile_inputs(&self, visible: Range<usize>) -> Vec<ProfileInput> {
                self.profile.profile_inputs(visible)
            }
        }
        impl Default for $ty {
            fn default() -> Self {
                Self::new(24)
            }
        }
        impl Indicator for $ty {
            fn name(&self) -> &str {
                $name
            }
            fn desc(&self) -> &str {
                $doc
            }
            fn inputs(&self) -> Vec<StudyInput> {
                self.profile
                    .inputs()
                    .into_iter()
                    .zip(Self::default().profile.inputs())
                    .map(|(input, default)| input.with_default(default.value))
                    .collect()
            }
            fn set_input(
                &mut self,
                id: &str,
                value: InputValue,
            ) -> Result<(), crate::studies::StudyError> {
                self.profile.set_input(id, value)
            }
            fn calculate(&mut self, data: &[Bar]) {
                self.profile.calculate(data);
            }
            fn update(&mut self, data: &[Bar], from: usize) {
                self.profile.update(data, from);
            }
            fn data_requests(&self) -> Vec<StudyDataRequest> {
                self.profile.data_requests()
            }
            fn data_start(&self, _: &StudyDataRequest) -> Option<DateTime<Utc>> {
                self.profile.data_start()
            }
            fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
                self.profile.update_data(data, from, sources);
            }
            fn order_flow_inputs(&self) -> &[OrderFlowInput] {
                std::slice::from_ref(&self.profile.flow)
            }
            fn needs_order_flow(&self) -> bool {
                false
            }
            fn volume_profiles(&self, visible: Range<usize>) -> Vec<StudyProfile> {
                self.profile.profiles(visible)
            }
            fn profile_levels(&self, visible: Range<usize>) -> Vec<ProfileLevel> {
                self.profile.levels(visible)
            }
            fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
                self.profile.resolutions =
                    instrument.map_or_else(Vec::new, |i| i.profile_resolutions.clone());
                self.profile.tick_size = instrument
                    .and_then(|i| i.tick_size)
                    .filter(|t| t.is_finite() && *t > 0.0);
                self.profile.days.set(instrument)
            }
            fn set_visible_range(&mut self, range: Range<usize>) -> bool {
                self.profile.view = range;
                false
            }
            fn set_timeframe(&mut self, timeframe: Timeframe) {
                self.profile.interval = timeframe;
                self.profile.clock = self.profile.clock.for_timeframe(timeframe);
            }
            fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
                let changed = self.profile.clock != *clock;
                self.profile.clock = clock.clone();
                changed
            }
            fn values(&self) -> &IndicatorValues {
                &self.profile.values
            }
            fn colors(&self) -> Vec<Color32> {
                self.profile.colors.clone()
            }
            fn set_colors(&mut self, colors: Vec<Color32>) {
                if colors.len() == 3 {
                    self.profile.colors[3] = colors[1];
                    self.profile.colors[4] = colors[2];
                }
                for (stored, color) in self.profile.colors.iter_mut().zip(colors) {
                    *stored = color;
                }
                self.profile.cache.get_mut().clear();
            }
            fn line_names(&self) -> Vec<String> {
                Vec::new()
            }
            fn is_visible(&self) -> bool {
                self.profile.visible
            }
            fn set_visible(&mut self, visible: bool) {
                self.profile.visible = visible;
            }
            fn clone_box(&self) -> Box<dyn Indicator> {
                Box::new(self.clone())
            }
        }
    };
}
profile_study!(
    VolProfileVisible,
    Visible,
    "Vol Profile VR",
    "Volume distributed over the bars currently visible in the chart."
);
profile_study!(
    VolProfileFixed,
    Fixed,
    "Vol Profile FR",
    "Volume between inclusive saved start and end times; all loaded bars until times are chosen."
);
profile_study!(
    VolProfileSession,
    Session,
    "Vol Profile Session",
    "One volume profile per exchange trading day, including the whole session of visible bars."
);
profile_study!(
    VolProfilePeriodic,
    Periodic,
    "Vol Profile Periodic",
    "One volume profile per chosen exchange calendar period, weekly by default."
);
profile_study!(
    VolProfileAnchored,
    Anchored,
    "Vol Profile Anchored",
    "Volume from a saved start time to the latest bar; all loaded bars until an anchor is chosen."
);
profile_study!(
    VolProfileAutoAnchored,
    Auto,
    "Vol Profile Auto Anchored",
    "Volume from the latest calendar anchor or an extreme in the chosen lookback."
);

impl VolProfileFixed {
    /// Include bars from `start` through `end`. Reversed times select no bars.
    pub fn with_range(
        mut self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        self.profile.start_time = start.timestamp_millis() as f64;
        self.profile.end_time = end.timestamp_millis() as f64;
        self.profile.data.clear();
        self.profile.values.clear();
        self.profile.cache.get_mut().clear();
        self
    }
}
impl VolProfileAnchored {
    /// Include bars at or after `start`, retaining the time when history is prepended.
    pub fn with_anchor(mut self, start: chrono::DateTime<chrono::Utc>) -> Self {
        self.profile.start_time = start.timestamp_millis() as f64;
        self.profile.data.clear();
        self.profile.values.clear();
        self.profile.cache.get_mut().clear();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hourly bars in three UTC sessions. With height 2, day one holds 2, 4, 2 in
    /// [98,100), [100,102), [102,104), so its POC is 101 and its 70% area is [98,104).
    pub(super) fn level_bars() -> Vec<Bar> {
        let time: DateTime<Utc> = "2024-01-02T10:00:00Z".parse().unwrap();
        [
            (0, 99.0, 99.0, 99.0, 99.0, 2.0),
            (1, 101.0, 101.0, 101.0, 101.0, 4.0),
            (2, 103.0, 103.0, 103.0, 103.0, 2.0),
            (24, 103.0, 104.0, 103.0, 104.0, 1.0),
            (25, 104.0, 104.0, 103.0, 103.0, 1.0),
            (48, 101.0, 101.0, 101.0, 101.0, 1.0),
            (49, 99.0, 99.0, 99.0, 99.0, 1.0),
        ]
        .into_iter()
        .map(|(hour, open, high, low, close, volume)| {
            Bar::new(
                time + TimeDelta::hours(hour),
                open,
                high,
                low,
                close,
                volume,
            )
        })
        .collect()
    }

    /// Finished session and periodic levels survive untouched sessions, end at independent
    /// row revisits and stay consistent through corrections, appends, recomputes and eviction.
    #[test]
    fn finished_profiles_extend_their_levels_to_the_first_bar_trading_in_their_row() {
        use crate::data::ProfileLevelKind::{
            PointOfControl as P, ValueAreaHigh as H, ValueAreaLow as V,
        };
        use crate::studies::ConfiguredStudy;
        let bars = level_bars();
        let levels = |study: &dyn Indicator, view| -> Vec<_> {
            study
                .profile_levels(view)
                .into_iter()
                .map(|l| (l.kind, l.price, l.bars))
                .collect()
        };
        let expected = vec![
            (P, 101.0, 3..6),
            (H, 104.0, 3..4),
            (V, 98.0, 3..7),
            (P, 103.0, 5..7),
            (H, 104.0, 5..7),
            (V, 102.0, 5..7),
        ];
        let mut periodic = VolProfilePeriodic::new(24);
        periodic.set_input("period", InputValue::Choice(0)).unwrap();
        for mut study in [
            Box::new(VolProfileSession::new(24)) as Box<dyn Indicator>,
            Box::new(periodic),
        ] {
            study.calculate(&bars);
            assert!(study.profile_levels(0..7).is_empty());
            study
                .set_input("row_size", InputValue::Number(2.0))
                .unwrap();
            for id in ["extend_poc", "extend_vah", "extend_val"] {
                study.set_input(id, InputValue::Toggle(true)).unwrap();
            }
            for (len, from, wanted) in [
                (
                    5,
                    0,
                    vec![(P, 101.0, 3..5), (H, 104.0, 3..4), (V, 98.0, 3..5)],
                ),
                (
                    6,
                    5,
                    vec![
                        (P, 101.0, 3..6),
                        (H, 104.0, 3..4),
                        (V, 98.0, 3..6),
                        (P, 103.0, 5..6),
                        (H, 104.0, 5..6),
                        (V, 102.0, 5..6),
                    ],
                ),
                (7, 6, expected.clone()),
            ] {
                if from == 0 {
                    study.calculate(&bars[..len]);
                } else {
                    study.update(&bars[..len], from);
                }
                assert_eq!(
                    levels(study.as_ref(), 0..len),
                    wanted,
                    "{} length {len}",
                    study.name()
                );
                let mut fresh = study.clone_box();
                fresh.calculate(&bars[..len]);
                assert_eq!(levels(fresh.as_ref(), 0..len), wanted);
            }
            let mut corrected = bars.clone();
            corrected[5] = Bar::new(bars[5].time, 105.0, 105.0, 105.0, 105.0, 1.0);
            study.update(&corrected, 5);
            let mut wanted = expected.clone();
            wanted[0].2 = 3..7;
            assert_eq!(levels(study.as_ref(), 0..7), wanted);
            let mut fresh = study.clone_box();
            fresh.calculate(&corrected);
            assert_eq!(levels(fresh.as_ref(), 0..7), wanted);
            // Correcting a finished profile changes its own levels; moving a boundary bar
            // into the earlier session changes both profiles' spans.
            for (from, replacement, first) in [
                (
                    1,
                    Bar::new(bars[1].time, 99.0, 99.0, 99.0, 99.0, 4.0),
                    vec![(P, 99.0, 3..7), (H, 100.0, 3..7), (V, 98.0, 3..7)],
                ),
                (
                    3,
                    Bar {
                        time: bars[2].time + TimeDelta::hours(1),
                        ..bars[3].clone()
                    },
                    vec![(P, 101.0, 4..6), (H, 104.0, 4..5), (V, 100.0, 4..6)],
                ),
            ] {
                study.calculate(&bars);
                assert_eq!(levels(study.as_ref(), 0..7), expected);
                let mut corrected = bars.clone();
                corrected[from] = replacement;
                study.update(&corrected, from);
                let wanted: Vec<_> = first
                    .into_iter()
                    .chain(expected[3..].iter().cloned())
                    .collect();
                assert_eq!(levels(study.as_ref(), 0..7), wanted);
                let mut fresh = study.clone_box();
                fresh.calculate(&corrected);
                assert_eq!(levels(fresh.as_ref(), 0..7), wanted);
            }
            study.calculate(&bars);
            let visible: Vec<_> = expected
                .iter()
                .filter(|(_, _, r)| r.end > 5)
                .cloned()
                .collect();
            assert_eq!(levels(study.as_ref(), 5..7), visible);
            assert!(levels(study.as_ref(), 7..9).is_empty());
            study.set_visible_range(5..7);
            let request = StudyDataRequest {
                symbol: String::new(),
                timeframe: None,
                metric: None,
            };
            assert_eq!(study.data_start(&request), Some(bars[0].time));
            for id in ["extend_poc", "extend_vah"] {
                study.set_input(id, InputValue::Toggle(false)).unwrap();
            }
            study.calculate(&bars);
            assert_eq!(
                levels(study.as_ref(), 0..7),
                vec![(V, 98.0, 3..7), (V, 102.0, 5..7)]
            );
            study
                .set_input("extend_val", InputValue::Toggle(false))
                .unwrap();
            study.calculate(&bars);
            assert_eq!(study.data_start(&request), Some(bars[5].time));
            for id in ["extend_poc", "extend_vah", "extend_val"] {
                study.set_input(id, InputValue::Toggle(true)).unwrap();
            }
            let mut wrapped = ConfiguredStudy::new(study);
            wrapped.calculate(&bars);
            assert_eq!(levels(&wrapped, 0..7), expected);
            assert!(wrapped.remove_oldest_bars(1));
            assert!(wrapped.profile_levels(0..6).is_empty());
            assert!(wrapped.volume_profiles(0..6).is_empty());
            wrapped.calculate(&bars[1..]);
            assert_eq!(
                levels(&wrapped, 0..6),
                vec![
                    (P, 101.0, 2..5),
                    (H, 104.0, 2..3),
                    (V, 100.0, 2..5),
                    (P, 103.0, 4..6),
                    (H, 104.0, 4..6),
                    (V, 102.0, 4..6),
                ]
            );
        }
        // The first finished session has no levels if retained executions miss its first bar.
        // With all executions, its two-tick rows put the POC at 100.5 and end it at bar 5.
        for skip in [0, 1] {
            let mut study = VolProfileSession::new(24);
            study.set_timeframe(Timeframe::Hour1);
            study
                .set_input("row_size", InputValue::Number(2.0))
                .unwrap();
            study
                .set_input("extend_poc", InputValue::Toggle(true))
                .unwrap();
            let flow = &study.order_flow_inputs()[0];
            flow.set_tick_size(Some(1.0));
            let clock = BarClock::new(Timeframe::Hour1);
            for bar in &bars[skip..] {
                flow.push_trade(
                    &clock,
                    &crate::model::Tick::new(bar.time, bar.close, bar.volume, None),
                );
            }
            study.calculate(&bars);
            let profiles = study.volume_profiles(0..7);
            assert_eq!(
                profiles[0].profile.source(),
                ProfileSource::Executions {
                    complete: skip == 0
                }
            );
            let got = levels(&study, 0..7);
            if skip == 0 {
                assert_eq!(got, vec![(P, 100.5, 3..6), (P, 102.5, 5..7)]);
            } else {
                assert_eq!(got, vec![(P, 102.5, 5..7)]);
            }
        }
    }

    /// Profiles sum the executions held for their bars, in rows of whole ticks within the row
    /// count or of the row height, as a footprint with that many ticks per row sums them. One
    /// whose executions start inside it is partial; one without them, or whose bars they cannot
    /// be assigned to exactly, keeps the estimate.
    #[test]
    fn profiles_read_the_executions_of_their_bars() {
        use crate::data::{Footprint, ProfileSource};
        use crate::model::{
            BarClock,
            OrderSide::{Buy, Sell},
            Tick,
        };
        let time = |at: &str| at.parse::<DateTime<Utc>>().unwrap();
        let bar = |at: &str| Bar::new(time(at), 100.0, 105.0, 99.0, 102.0, 10.0);
        let trade = |at: &str, price, size, side| Tick::new(time(at), price, size, side);
        let bars: Vec<Bar> = [
            "2026-03-06T23:58:00Z",
            "2026-03-06T23:59:00Z",
            "2026-03-07T00:00:00Z",
            "2026-03-07T00:01:00Z",
        ]
        .map(bar)
        .into();
        // Bar 2 trades 2 bought at 100 and 1 sold at 101; bar 3 trades 1 bought at 101 and 3 of
        // unknown side at 104.
        let trades = vec![
            trade("2026-03-07T00:00:10Z", 100.0, 2.0, Some(Buy)),
            trade("2026-03-07T00:00:20Z", 101.0, 1.0, Some(Sell)),
            trade("2026-03-07T00:01:10Z", 101.0, 1.0, Some(Buy)),
            trade("2026-03-07T00:01:20Z", 104.0, 3.0, None),
        ];
        let negative = vec![
            trade("2026-03-07T00:00:10Z", -1.0, 1.0, Some(Buy)),
            trade("2026-03-07T00:00:20Z", -3.0, 2.0, Some(Sell)),
        ];
        // Range bars of these executions end at 00:00:20 and 00:01:30.
        let ranged = vec![bar("2026-03-07T00:00:20Z"), bar("2026-03-07T00:01:30Z")];
        let range_trades = vec![
            trade("2026-03-07T00:00:10Z", 100.0, 1.0, Some(Buy)),
            trade("2026-03-07T00:00:40Z", 101.0, 1.0, Some(Buy)),
            trade("2026-03-07T00:01:20Z", 102.0, 1.0, Some(Buy)),
        ];
        let gap = vec![bars[0].clone(), bars[1].clone(), bars[3].clone()];
        let mut shared = bars.clone();
        shared.insert(3, bars[2].clone());
        let fixed = |rows, first: &Bar, last: &Bar| -> Box<dyn Indicator> {
            Box::new(VolProfileFixed::new(rows).with_range(first.time, last.time))
        };
        let mut height = fixed(24, &bars[2], &bars[3]);
        height
            .set_input("row_size", InputValue::Number(1.6))
            .unwrap();
        let minute = BarClock::new(Timeframe::Min1);
        let (complete, partial, estimated) = (
            ProfileSource::Executions { complete: true },
            ProfileSource::Executions { complete: false },
            ProfileSource::Estimated,
        );
        // Ticks per row, low, and volume, buys and sells per row.
        type Rows = (u32, f64, Vec<f64>, Vec<f64>, Vec<f64>);
        let per_tick: Rows = (
            1,
            99.5,
            vec![2.0, 2.0, 0.0, 0.0, 3.0],
            vec![2.0, 1.0, 0.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0, 0.0, 0.0],
        );
        type Expected = Vec<(Range<usize>, ProfileSource, Option<Rows>)>;
        type Case = (
            &'static str,
            Box<dyn Indicator>,
            Vec<Bar>,
            Vec<Tick>,
            BarClock,
            Range<usize>,
            Expected,
        );
        let rows: Vec<Case> = vec![
            // Levels 100..=104 take 5 rows of 24.
            (
                "single ticks within the row count",
                fixed(24, &bars[2], &bars[3]),
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![(2..4, complete, Some(per_tick.clone()))],
            ),
            // ceil(5 / 2) = 3 ticks: rows 33 (99..=101) and 34 (102..=104).
            (
                "the traded ticks over the row count",
                fixed(2, &bars[2], &bars[3]),
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![(
                    2..4,
                    complete,
                    Some((3, 98.5, vec![4.0, 3.0], vec![3.0, 0.0], vec![1.0, 0.0])),
                )],
            ),
            // Levels 101..=104: 2 ticks give rows 50, 51, 52, too many; 4 ticks give 25 and 26.
            (
                "levels across a multiple take wider rows",
                fixed(2, &bars[3], &bars[3]),
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![(
                    3..4,
                    complete,
                    Some((4, 99.5, vec![1.0, 3.0], vec![1.0, 0.0], vec![0.0, 0.0])),
                )],
            ),
            // round(1.6) = 2 ticks: rows 50, 51, 52.
            (
                "the row height in whole ticks",
                height,
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![(
                    2..4,
                    complete,
                    Some((
                        2,
                        99.5,
                        vec![4.0, 0.0, 3.0],
                        vec![3.0, 0.0, 0.0],
                        vec![1.0, 0.0, 0.0],
                    )),
                )],
            ),
            // Held from 00:00, after the profile's first bar at 23:59.
            (
                "executions from inside the profile",
                fixed(24, &bars[1], &bars[3]),
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![(1..4, partial, Some(per_tick.clone()))],
            ),
            (
                "a session before the executions",
                Box::new(VolProfileSession::new(24)),
                bars.clone(),
                trades.clone(),
                minute.clone(),
                0..4,
                vec![
                    (0..2, estimated, None),
                    (2..4, complete, Some(per_tick.clone())),
                ],
            ),
            // Levels -3 and -1 in 2-tick rows: rows -2 (-4..=-3) and -1 (-2..=-1).
            (
                "negative prices group downwards",
                fixed(2, &bars[2], &bars[3]),
                bars.clone(),
                negative,
                minute.clone(),
                0..4,
                vec![(
                    2..4,
                    complete,
                    Some((2, -4.5, vec![2.0, 1.0], vec![0.0, 1.0], vec![2.0, 0.0])),
                )],
            ),
            (
                "executions of another calendar",
                fixed(24, &bars[2], &bars[3]),
                bars.clone(),
                trades.clone(),
                BarClock::new(Timeframe::Min5),
                0..4,
                vec![(2..4, estimated, None)],
            ),
            (
                "executions of a bar the chart lacks",
                fixed(24, &bars[1], &bars[3]),
                gap,
                trades.clone(),
                minute.clone(),
                0..3,
                vec![(1..3, estimated, None)],
            ),
            (
                "a bar before sharing the first bar's slot",
                Box::new(VolProfileVisible::new(24)),
                shared.clone(),
                trades.clone(),
                minute.clone(),
                3..5,
                vec![(3..5, estimated, None)],
            ),
            (
                "a bar after sharing the last bar's slot",
                Box::new(VolProfileVisible::new(24)),
                shared,
                trades.clone(),
                minute.clone(),
                2..3,
                vec![(2..3, estimated, None)],
            ),
            (
                "a bar stamped inside its slot",
                fixed(24, &ranged[1], &ranged[1]),
                ranged,
                range_trades,
                minute.clone(),
                0..2,
                vec![(1..2, estimated, None)],
            ),
            (
                "unordered bar opens keep the estimate",
                Box::new(VolProfileVisible::new(24)),
                vec![
                    bars[3].clone(),
                    bar("2026-03-07T00:02:00Z"),
                    bars[2].clone(),
                ],
                trades.clone(),
                minute.clone(),
                1..3,
                vec![(1..3, estimated, None)],
            ),
        ];
        for (label, mut study, bars, trades, clock, view, expected) in rows {
            let flow = study.order_flow_inputs()[0].clone();
            flow.set_tick_size(Some(1.0));
            for trade in &trades {
                flow.push_trade(&clock, trade);
            }
            study.calculate(&bars);
            let got = study.volume_profiles(view);
            assert_eq!(got.len(), expected.len(), "{label}");
            for (got, (range, source, rows)) in got.iter().zip(expected) {
                assert_eq!(got.bars, range, "{label}");
                assert_eq!(got.profile.source(), source, "{label}");
                let Some((ticks, low, volume, up, down)) = rows else {
                    continue;
                };
                let profile = &*got.profile;
                assert_eq!(
                    (profile.low(), profile.row_height()),
                    (low, f64::from(ticks)),
                    "{label}"
                );
                assert_eq!(profile.volume(), volume, "{label}");
                assert_eq!(profile.up(), up, "{label}");
                assert_eq!(profile.down(), down, "{label}");
                // The same executions, grouped by a footprint as they arrive.
                let mut footprint = Footprint::new(clock.clone(), 1.0, ticks).unwrap();
                for trade in &trades {
                    footprint.push(trade).unwrap();
                }
                let span = footprint.profile(bars[range.start].time, bars[range.end - 1].time);
                let independent = VolumeProfile::from_executions(
                    &span,
                    footprint.price_step(),
                    footprint.tick_size(),
                    source == complete,
                );
                assert_eq!(Some(profile), independent.as_ref(), "{label}");
            }
        }
        // A later execution in the last bar shows at the next update, as a fresh calculation has.
        let mut study = fixed(24, &bars[2], &bars[3]);
        let flow = study.order_flow_inputs()[0].clone();
        flow.set_tick_size(Some(1.0));
        for trade in &trades {
            flow.push_trade(&minute, trade);
        }
        study.calculate(&bars);
        assert_eq!(
            study.volume_profiles(0..4)[0].profile.volume(),
            [2.0, 2.0, 0.0, 0.0, 3.0]
        );
        flow.push_trade(&minute, &trade("2026-03-07T00:01:30Z", 104.0, 2.0, None));
        study.update(&bars, 3);
        let got = study.volume_profiles(0..4);
        assert_eq!(got[0].profile.volume(), [2.0, 2.0, 0.0, 0.0, 5.0]);
        let mut fresh = study.clone_box();
        fresh.calculate(&bars);
        assert_eq!(
            format!("{got:?}"),
            format!("{:?}", fresh.volume_profiles(0..4))
        );
    }

    /// Zoom changes the shared tick grid without reusing cached rows or adding volume.
    #[test]
    fn adaptive_session_rows_follow_the_sessions_in_view() {
        let at = |time: &str| time.parse::<DateTime<Utc>>().unwrap();
        let bars = [
            Bar::new(
                at("2026-03-02T09:00:00Z"),
                100.0,
                110.0,
                100.0,
                110.0,
                100.0,
            ),
            Bar::new(at("2026-03-03T09:00:00Z"), 100.0, 101.0, 100.0, 101.0, 40.0),
            Bar::new(at("2026-03-03T10:00:00Z"), 101.0, 102.0, 101.0, 102.0, 40.0),
        ];
        let mut study = VolProfileSession::new(4);
        study
            .set_input("adaptive_rows", InputValue::Toggle(true))
            .unwrap();
        study.set_instrument(Some(&Instrument::new("SYNTH").with_tick_size(0.25)));
        study.calculate(&bars);
        for (view, height, expected) in [
            (
                0..3,
                2.5,
                vec![vec![25.0, 25.0, 25.0, 25.0, 0.0], vec![80.0]],
            ),
            (1..3, 0.5, vec![vec![20.0, 20.0, 20.0, 20.0, 0.0]]),
            (
                0..3,
                2.5,
                vec![vec![25.0, 25.0, 25.0, 25.0, 0.0], vec![80.0]],
            ),
        ] {
            let profiles = study.volume_profiles(view);
            assert_eq!(profiles.len(), expected.len());
            for (plot, volume) in profiles.iter().zip(expected) {
                assert_eq!(plot.profile.row_height(), height);
                assert_eq!(plot.profile.volume(), volume);
            }
        }
        let mut live = bars.to_vec();
        live.push(Bar::new(
            at("2026-03-03T11:00:00Z"),
            101.0,
            102.0,
            100.0,
            102.0,
            20.0,
        ));
        study.update(&live, 3);
        let profiles = study.volume_profiles(1..4);
        assert_eq!(profiles[0].profile.row_height(), 0.5);
        assert_eq!(profiles[0].profile.volume(), [25.0, 25.0, 25.0, 25.0, 0.0]);
        let mut fresh = study.clone_box();
        fresh.calculate(&live);
        assert_eq!(
            format!("{profiles:?}"),
            format!("{:?}", fresh.volume_profiles(1..4))
        );

        study.set_input("row_count", InputValue::Length(3)).unwrap();
        for (instrument, height, volume) in [
            (
                Some(Instrument::new("SYNTH").with_tick_size(0.25)),
                1.25,
                vec![50.0, 30.0],
            ),
            (None, 1.0, vec![40.0, 40.0, 0.0]),
        ] {
            study.set_instrument(instrument.as_ref());
            study.calculate(&bars);
            let profiles = study.volume_profiles(1..3);
            assert_eq!(profiles[0].profile.row_height(), height);
            assert_eq!(profiles[0].profile.volume(), volume);
        }
        let mut flat = VolProfileSession::new(4);
        flat.set_input("adaptive_rows", InputValue::Toggle(true))
            .unwrap();
        flat.calculate(&[Bar::new(bars[0].time, 105.0, 105.0, 105.0, 105.0, 10.0)]);
        let profiles = flat.volume_profiles(0..1);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].config.row_size(), None);
        assert_eq!(profiles[0].profile.volume(), [10.0]);
    }

    /// Suggestions serve an inclusive profile length; explicit choices and the opt-out win. A
    /// profile emptied to wait for its recompute drops a held interval the chart has reached.
    #[test]
    fn feed_suggested_interval_follows_the_profile_length() {
        let start: DateTime<Utc> = "2026-03-02T09:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..4)
            .map(|hour| {
                Bar::new(
                    start + TimeDelta::hours(hour),
                    100.0,
                    101.0,
                    99.0,
                    100.0,
                    10.0,
                )
            })
            .collect();
        let instrument = Instrument::new("SYNTH").with_profile_resolutions([
            ProfileResolution::new(
                Timeframe::Volume(std::num::NonZeroU64::new(1).unwrap()),
                None,
            ),
            ProfileResolution::new(Timeframe::Min1, Some(TimeDelta::hours(2))),
            ProfileResolution::new(Timeframe::Min5, Some(TimeDelta::hours(4))),
            ProfileResolution::new(Timeframe::Min15, None),
            ProfileResolution::new(Timeframe::Hour1, None),
            ProfileResolution::new(
                Timeframe::Ticks(std::num::NonZeroU64::new(1).unwrap()),
                None,
            ),
        ]);
        for case in 0..11 {
            let mut study: Box<dyn Indicator> = match case {
                1 => Box::new(VolProfileFixed::new(4).with_range(bars[0].time, bars[1].time)),
                2..=4 => Box::new(VolProfileVisible::new(4)),
                _ => Box::new(VolProfileFixed::new(4)),
            };
            study.set_timeframe(if case == 8 {
                Timeframe::Min5
            } else {
                Timeframe::Hour1
            });
            if case == 5 {
                study
                    .set_input(
                        "source_interval",
                        InputValue::Timeframe(Some(Timeframe::Min15)),
                    )
                    .unwrap();
            }
            if case == 6 {
                study
                    .set_input("feed_source", InputValue::Toggle(false))
                    .unwrap();
            }
            study.set_instrument((case != 7).then_some(&instrument));
            if case != 9 {
                study.calculate(&bars);
            }
            study.set_visible_range(match case {
                2 => 0..1,
                3 => 0..9,
                4 => 4..9,
                _ => 0..4,
            });
            if case == 10 {
                // Emptied to wait for a recompute, then the chart moves to the held interval.
                study.update(&[], 0);
                study.set_timeframe(Timeframe::Min5);
            }
            let expected = match case {
                0 | 3 => Some(Timeframe::Min5),
                1 | 2 => Some(Timeframe::Min1),
                5 => Some(Timeframe::Min15),
                _ => None,
            };
            assert_eq!(
                study.data_requests().first().and_then(|r| r.timeframe),
                expected,
                "case {case}"
            );
        }
    }

    /// Range selection uses bar opens and inclusive saved times; calendar profiles retain
    /// whole exchange periods while a visible profile includes only the requested bars.
    #[test]
    fn profiles_use_only_their_selected_bars() {
        let times = [
            "2026-03-06T22:00:00Z",
            "2026-03-06T23:00:00Z",
            "2026-03-07T00:00:00Z",
            "2026-03-09T01:00:00Z",
        ];
        let bars: Vec<_> = times
            .iter()
            .enumerate()
            .map(|(i, t)| {
                Bar::new(
                    t.parse().unwrap(),
                    1.0,
                    2.0,
                    0.0,
                    if i == 2 { 0.5 } else { 1.5 },
                    (i + 1) as f64 * 10.0,
                )
            })
            .collect();
        type Case = (Box<dyn Indicator>, Vec<(Range<usize>, f64)>);
        let cases: Vec<Case> = vec![
            (Box::new(VolProfileVisible::new(2)), vec![(1..4, 90.0)]),
            (
                Box::new(VolProfileFixed::new(2).with_range(bars[1].time, bars[2].time)),
                vec![(1..3, 50.0)],
            ),
            (
                Box::new(VolProfileAnchored::new(2).with_anchor(bars[2].time)),
                vec![(2..4, 70.0)],
            ),
            (
                Box::new(VolProfileSession::new(2)),
                vec![(0..2, 30.0), (2..3, 30.0), (3..4, 40.0)],
            ),
            (
                Box::new(VolProfilePeriodic::new(2)),
                vec![(0..3, 60.0), (3..4, 40.0)],
            ),
            (Box::new(VolProfileAutoAnchored::new(2)), vec![(3..4, 40.0)]),
        ];
        let request = StudyDataRequest {
            symbol: String::new(),
            timeframe: None,
            metric: None,
        };
        for (mut study, expected) in cases {
            let defaults = crate::studies::IndicatorFactory::new()
                .create(study.name())
                .unwrap()
                .inputs();
            study
                .set_input("value_area", InputValue::Number(90.0))
                .unwrap();
            for (input, default) in study.inputs().iter().zip(&defaults) {
                assert_eq!(
                    input.default,
                    default.value,
                    "{}: {}",
                    study.name(),
                    input.id
                );
            }
            // Saved UTC times are displayed and accepted back without changing the range.
            let times: Vec<_> = study
                .inputs()
                .into_iter()
                .filter_map(|input| match input.value {
                    InputValue::Text(text) => Some((input.id, text)),
                    _ => None,
                })
                .collect();
            let expected_times: &[(&str, &str)] = match study.name() {
                "Vol Profile FR" => &[
                    ("start_time", "2026-03-06 23:00:00"),
                    ("end_time", "2026-03-07 00:00:00"),
                ],
                "Vol Profile Anchored" => &[("start_time", "2026-03-07 00:00:00")],
                _ => &[],
            };
            assert_eq!(
                times
                    .iter()
                    .map(|(id, text)| (id.as_str(), text.as_str()))
                    .collect::<Vec<_>>(),
                expected_times,
                "{}",
                study.name()
            );
            for (id, text) in times {
                study.set_input(&id, InputValue::Text(text)).unwrap();
            }
            study
                .set_input("value_area", InputValue::Number(70.0))
                .unwrap();
            study.calculate(&bars);
            let got = study.volume_profiles(1..4);
            assert_eq!(got.len(), expected.len(), "{}", study.name());
            let first = expected[0].0.start;
            for (got, (range, volume)) in got.iter().zip(expected) {
                assert_eq!(got.bars, range, "{}", study.name());
                assert_eq!(got.profile.volume(), [volume / 2.0, volume / 2.0]);
                let down = if range.contains(&2) { 15.0 } else { 0.0 };
                assert_eq!(got.profile.down(), [down, down]);
                assert_eq!(got.profile.up(), [volume / 2.0 - down, volume / 2.0 - down]);
            }
            let cached = study.volume_profiles(1..4);
            assert!(Arc::ptr_eq(&got[0].profile, &cached[0].profile));
            // Finer bars are needed from the first bar of the first profile in view, and with
            // no profile in view from the newest bar on.
            for (view, at) in [(1..4, first), (4..4, 3)] {
                study.set_visible_range(view);
                assert_eq!(
                    study.data_start(&request),
                    Some(bars[at].time),
                    "{}",
                    study.name()
                );
            }
            let mut live = bars.clone();
            for step in 0..3 {
                let from = match step {
                    0 => {
                        live[3].close = 0.5;
                        live[3].volume += 5.0;
                        3
                    }
                    1 => {
                        live.push(Bar::new(
                            live[3].time + chrono::Duration::hours(1),
                            1.0,
                            3.0,
                            -1.0,
                            1.5,
                            8.0,
                        ));
                        3
                    }
                    _ => {
                        live[0].volume += 9.0;
                        0
                    }
                };
                study.update(&live, from);
                let mut fresh = study.clone_box();
                fresh.calculate(&live);
                assert_eq!(
                    format!("{:?}", study.volume_profiles(1..live.len())),
                    format!("{:?}", fresh.volume_profiles(1..live.len())),
                    "{} step {step}",
                    study.name()
                );
            }
        }
        // Fractions survive the editor, junk changes nothing, and empty fields select all bars.
        let mut fixed = VolProfileFixed::new(2).with_range(
            bars[1].time + chrono::Duration::milliseconds(500),
            bars[2].time,
        );
        let start = fixed
            .inputs()
            .into_iter()
            .find(|input| input.id == "start_time")
            .unwrap();
        assert_eq!(
            start.value,
            InputValue::Text("2026-03-06 23:00:00.500".into())
        );
        fixed.set_input("start_time", start.value).unwrap();
        fixed.calculate(&bars);
        assert_eq!(fixed.volume_profiles(0..4)[0].bars, 2..3);
        assert!(
            fixed
                .set_input("start_time", InputValue::Text("23:00".into()))
                .is_err()
        );
        assert_eq!(fixed.volume_profiles(0..4)[0].bars, 2..3);
        fixed
            .set_input("start_time", InputValue::Text("2026-03-06 23:00".into()))
            .unwrap();
        fixed.calculate(&bars);
        assert_eq!(fixed.volume_profiles(0..4)[0].bars, 1..3);
        fixed
            .set_input("start_time", InputValue::Text(String::new()))
            .unwrap();
        fixed
            .set_input("end_time", InputValue::Text(String::new()))
            .unwrap();
        fixed.calculate(&bars);
        assert_eq!(fixed.volume_profiles(0..4)[0].bars, 0..4);
        let mut visible = VolProfileVisible::new(2);
        visible.calculate(&bars);
        assert_eq!(
            visible.volume_profiles(1..3)[0].profile.volume(),
            [25.0, 25.0]
        );
        assert_eq!(
            visible.volume_profiles(0..1)[0].profile.volume(),
            [5.0, 5.0]
        );
        assert!(visible.volume_profiles(4..5).is_empty());
        let mut tick = bars.clone();
        tick[2].close = 1.5;
        tick[2].volume = 50.0;
        visible.update(&tick, 2);
        let got = visible.volume_profiles(1..3);
        assert_eq!(got[0].profile.volume(), [35.0, 35.0]);
        assert_eq!(got[0].profile.down(), [0.0, 0.0]);
        assert!(visible.values().columns().is_empty());

        let mut session = VolProfileSession::new(2);
        session
            .set_instrument(Some(&Instrument::new("SYNTH").with_time_zone(
                crate::model::TimezoneId::fixed_offset(-5 * 3600).unwrap(),
            )));
        session.calculate(&bars);
        let got = session.volume_profiles(1..3);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].bars, 0..3);
        assert_eq!(got[0].profile.volume(), [30.0, 30.0]);
    }
}

#[cfg(all(test, feature = "widget"))]
mod rendering_tests {
    use super::*;
    use crate::chart::rendering::{ChartMapping, IndicatorRenderer, RenderContext};
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::{model::Bar, studies::Indicator};

    /// Hidden value area boundaries leave clicks to the plot beneath, while rows and the point
    /// of control still select the study.
    #[test]
    fn hidden_value_area_lines_do_not_select() {
        let time = |minute| chrono::DateTime::UNIX_EPOCH + TimeDelta::minutes(minute);
        let bars = [
            Bar::new(time(0), 10.0, 12.0, 10.0, 12.0, 100.0),
            Bar::new(time(1), 20.0, 20.0, 10.0, 10.0, 100.0),
            Bar::new(time(2), 12.0, 20.0, 12.0, 20.0, 100.0),
        ];
        let mut study = VolProfileSession::new(10);
        study.calculate(&bars);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(9.0, 21.0),
            0.0,
            200.0,
            false,
        );
        let coords = ChartMapping::new(rect, 200.0, 0, 1, 0.0, price);
        #[derive(Clone)]
        struct Guides(VolProfileSession, bool);
        impl Indicator for Guides {
            fn name(&self) -> &str {
                self.0.name()
            }
            fn calculate(&mut self, bars: &[Bar]) {
                self.0.calculate(bars);
            }
            fn values(&self) -> &IndicatorValues {
                self.0.values()
            }
            fn colors(&self) -> Vec<Color32> {
                self.0.colors()
            }
            fn set_colors(&mut self, colors: Vec<Color32>) {
                self.0.set_colors(colors);
            }
            fn set_visible(&mut self, visible: bool) {
                self.0.set_visible(visible);
            }
            fn clone_box(&self) -> Box<dyn Indicator> {
                Box::new(self.clone())
            }
            fn volume_profiles(&self, visible: Range<usize>) -> Vec<StudyProfile> {
                let mut plots = self.0.volume_profiles(visible);
                for plot in &mut plots {
                    plot.config.show_value_area_lines = self.1;
                }
                plots
            }
        }
        // Rows 0 and 1 hold 60 each, the rest 22.5 each. The 70% area is rows 0 to 5,
        // prices 10 to 16; POC is 10.5. Every row ends at or before x=120.
        for shown in [false, true] {
            let study = Guides(study.clone(), shown);
            for (x, value, expected) in [
                (390.0, 10.0, shown),
                (390.0, 16.0, shown),
                (390.0, 10.5, true),
                (20.0, 12.5, true),
            ] {
                let hit = crate::chart::indicators::hit_test_indicator(
                    egui::pos2(x, price.price_to_y(value)),
                    &study,
                    0,
                    0..3,
                    &coords,
                    |value| price.price_to_y(value),
                );
                assert_eq!(
                    hit.is_some(),
                    expected,
                    "guides {shown}, selection at ({x}, {value})"
                );
            }
        }
    }

    /// Extensions use the profile's colour in either theme and stop at the touching bar's
    /// centre, starting at the first later bar's left edge.
    #[test]
    fn extended_levels_paint_hairlines_in_the_profile_colour_of_either_theme() {
        let bars = super::tests::level_bars();
        let light = crate::theme::Theme::light();
        let dark = crate::theme::Theme::dark();
        let saved = Color32::from_rgb(41, 98, 255);
        for (theme, color, plot_width) in [
            (&light, None, 400.0),
            (&dark, None, 400.0),
            (&light, Some(saved), 400.0),
            (&dark, Some(saved), 400.0),
            (&light, None, 120.0),
            (&dark, None, 120.0),
        ] {
            let mut study = VolProfileSession::new(24);
            study
                .set_input("row_size", InputValue::Number(2.0))
                .unwrap();
            for id in ["extend_poc", "extend_vah", "extend_val"] {
                study.set_input(id, InputValue::Toggle(true)).unwrap();
            }
            if let Some(color) = color {
                study.set_colors(vec![color]);
            }
            study.calculate(&bars);
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(plot_width, 200.0));
            let price = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(96.0, 106.0),
                0.0,
                200.0,
                false,
            );
            let coords = ChartMapping::new(rect, 50.0, 0, 6, 0.0, price);
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, theme);
            let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                IndicatorRenderer::render(
                    &RenderContext::new(ui.painter(), rect),
                    &indicators,
                    &bars,
                    &price,
                    &coords,
                );
            });
            let lines: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::LineSegment { points, stroke } => {
                        assert_eq!(shape.clip_rect, rect);
                        Some((
                            points[0].x,
                            points[1].x,
                            points[0].y,
                            stroke.width,
                            stroke.color,
                        ))
                    }
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            let color = color.unwrap_or(theme.ui.text);
            let [r, g, b, _] = color.to_srgba_unmultiplied();
            let va = Color32::from_rgba_unmultiplied(r, g, b, 200);
            let expected: Vec<_> = [
                (199.0, 324.0, 100.0, 1.0, color),
                (199.0, 224.0, 40.0, 1.0, va),
                (199.0, 374.0, 160.0, 1.0, va),
                (299.0, 374.0, 60.0, 1.0, color),
                (299.0, 374.0, 40.0, 1.0, va),
                (299.0, 374.0, 80.0, 1.0, va),
            ]
            .into_iter()
            .filter(|(_, right, _, _, _)| right + plot_width - 400.0 >= 0.0)
            .collect();
            assert_eq!(lines.len(), expected.len());
            for ((x0, x1, y, width, color), (left, right, price_y, stroke, ink)) in
                lines.into_iter().zip(expected)
            {
                assert!(
                    (x0 - (left + plot_width - 400.0)).abs() < 1e-3
                        && (x1 - (right + plot_width - 400.0)).abs() < 1e-3
                        && (y - price_y).abs() < 1e-3
                );
                assert_eq!((width, color), (stroke, ink));
            }
        }
    }

    /// Painted rows determine the profile's selection and independent price bounds.
    #[test]
    fn visible_profile_paints_price_rows() {
        let bars = [Bar::new(
            chrono::DateTime::UNIX_EPOCH,
            10.0,
            12.0,
            10.0,
            11.0,
            20.0,
        )];
        let mut study = VolProfileVisible::new(2);
        study.calculate(&bars);
        let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 200.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(9.0, 13.0),
            0.0,
            200.0,
            false,
        );
        let coords = ChartMapping::new(rect, 40.0, 0, 0, 0.0, price);
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            IndicatorRenderer::render(
                &RenderContext::new(ui.painter(), rect),
                &indicators,
                &bars,
                &price,
                &coords,
            );
        });
        let rows = output
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape,
            egui::Shape::Rect(r) if (r.rect.height() - 49.0).abs() < 0.01)
            })
            .count();
        // The rows are estimated from the bar, as noted at the top left of the
        // right-aligned box: x from 280 (400 less 30%) plus 3, y of 12 = 50.
        let notes: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == "Estimated from bars" => Some(t.pos),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        assert_eq!(notes, [egui::pos2(283.0, 50.0)]);
        assert!(rows >= 2, "profile rows {rows}");
        // Equal row volumes put the POC at y=125 and fill the strip from x=280; the line crosses
        // the plot, past the rows.
        for (x, y, expected) in [
            (395.0, 125.0, true),
            (285.0, 75.0, true),
            (5.0, 125.0, true),
            (5.0, 75.0, false),
        ] {
            let hit = crate::chart::indicators::hit_test_indicator(
                egui::pos2(x, y),
                indicators[0].as_ref(),
                0,
                0..1,
                &coords,
                |value| price.price_to_y(value),
            );
            assert_eq!(hit.is_some(), expected, "profile selection at ({x}, {y})");
        }

        // The first bar is outside the painted viewport and must not stretch its scale.
        let mut independent =
            crate::studies::ConfiguredStudy::new(Box::new(VolProfileVisible::new(2)));
        independent.set_scale(crate::studies::StudyScale::Independent);
        independent.calculate(&[
            Bar {
                high: 1000.0,
                ..bars[0].clone()
            },
            bars[0].clone(),
            bars[0].clone(),
        ]);
        let coords = ChartMapping {
            base_idx: 10,
            ..coords
        };
        let mapping = crate::chart::rendering::study_mapping(&independent, &coords);
        assert!((mapping.price_to_y(12.0) - 200.0 / 12.0).abs() < 0.01);
        assert!((mapping.price_to_y(10.0) - 200.0 * 11.0 / 12.0).abs() < 0.01);
        assert!(
            crate::chart::indicators::hit_test_indicator(
                egui::pos2(395.0, mapping.price_to_y(10.5)),
                &independent,
                0,
                0..3,
                &mapping,
                |value| mapping.price_to_y(value),
            )
            .is_some()
        );
    }

    /// A new session profile paints cyan and pink rows, stronger in the value area, under a 2 px
    /// point of control in the foreground colour of either theme. Saved colours keep theirs. Its
    /// widest row takes 30% of a session that fills the plot, or the share its width input sets.
    #[test]
    fn session_profile_paints_its_default_and_saved_colours() {
        let time = |minute| chrono::DateTime::UNIX_EPOCH + TimeDelta::minutes(minute);
        // Rows 10 and 11 hold most of the volume, up and down; rows 12 to 19 hold the rest of each.
        let bars = [
            Bar::new(time(0), 10.0, 12.0, 10.0, 12.0, 100.0),
            Bar::new(time(1), 20.0, 20.0, 10.0, 10.0, 100.0),
            Bar::new(time(2), 12.0, 20.0, 12.0, 20.0, 100.0),
        ];
        let saved = vec![
            Color32::from_rgb(41, 98, 255),
            Color32::from_rgba_unmultiplied(51, 74, 212, 89),
            Color32::from_rgba_unmultiplied(214, 140, 35, 89),
            Color32::from_rgb(51, 74, 212),
            Color32::from_rgb(214, 140, 35),
        ];
        let defaults = [
            [0, 188, 212, 110],
            [233, 30, 99, 110],
            [0, 188, 212, 163],
            [233, 30, 99, 163],
        ];
        let stored = [
            [51, 74, 212, 89],
            [214, 140, 35, 89],
            [51, 74, 212, 163],
            [214, 140, 35, 163],
        ];
        let light = crate::theme::Theme::light();
        let dark = crate::theme::Theme::dark();
        // Theme, saved colours, the rows outside and inside the value area, the line colour, the
        // width input and where the widest row ends: the session spans the plot, 0 to 400.
        for (theme, colors, rows, poc, width, right_edge) in [
            (&light, None, defaults, light.ui.text, None, 120.0),
            (&dark, None, defaults, dark.ui.text, None, 120.0),
            (&light, Some(saved.clone()), stored, saved[0], None, 120.0),
            (&light, None, defaults, light.ui.text, Some(90.0), 360.0),
        ] {
            let mut study = VolProfileSession::new(10);
            if let Some(colors) = colors {
                study.set_colors(colors);
            }
            if let Some(width) = width {
                study
                    .set_input("width", InputValue::Number(width))
                    .expect("width");
            }
            study.calculate(&bars);
            let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 200.0));
            let price = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(9.0, 21.0),
                0.0,
                200.0,
                false,
            );
            // A view inside the session: its first bar starts at the plot's left edge and its last
            // ends past the right.
            let coords = ChartMapping::new(rect, 200.0, 0, 1, 0.0, price);
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, theme);
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                IndicatorRenderer::render(
                    &RenderContext::new(ui.painter(), rect),
                    &indicators,
                    &bars,
                    &price,
                    &coords,
                );
            });
            let fills: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(row) => Some(row.fill),
                    _ => None,
                })
                .collect();
            let widest = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(row) => Some(row.rect.right()),
                    _ => None,
                })
                .fold(f32::MIN, f32::max);
            let lines: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Path(line) => Some(line.stroke.clone()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            assert!(
                (widest - right_edge).abs() < 1e-3,
                "{widest} to {right_edge}"
            );
            for [r, g, b, a] in rows {
                // Colours pass through the study's stored and painted forms as Color32 does.
                let [r, g, b, a] =
                    Color32::from_rgba_unmultiplied(r, g, b, a).to_srgba_unmultiplied();
                let row = Color32::from_rgba_unmultiplied(r, g, b, a);
                assert!(fills.contains(&row), "{row:?} among {fills:?}");
            }
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0].width, 2.0);
            assert_eq!(lines[0].color, egui::epaint::ColorMode::Solid(poc));
        }
    }

    /// A box that starts at the pane's left edge, under the chart legend, keeps its source note
    /// at the right, short of the value area labels.
    #[test]
    fn profile_from_the_left_edge_notes_its_source_clear_of_the_legend() {
        let bars: Vec<_> = (0..10)
            .map(|i| {
                let time = chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(i);
                Bar::new(time, 10.0, 12.0, 10.0, 11.0, 20.0)
            })
            .collect();
        let mut study = VolProfileFixed::new(2);
        study.calculate(&bars);
        let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(study)];
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 200.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(9.0, 13.0),
            0.0,
            200.0,
            false,
        );
        let coords = ChartMapping {
            base_idx: 9,
            ..ChartMapping::new(rect, 40.0, 0, 0, 0.0, price)
        };
        let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            IndicatorRenderer::render(
                &RenderContext::new(ui.painter(), rect),
                &indicators,
                &bars,
                &price,
                &coords,
            );
        });
        let note = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == "Estimated from bars" => {
                Some((t.pos.x + t.galley.size().x, t.pos.y))
            }
            _ => None,
        });
        output.drop_without_applying_deltas();
        // Last bar's right edge at 399 (400 less half a 40-point bar, 1, plus half a bar), first
        // bar's left edge clamped to 0; the note ends 50 short of 399, at the y of 12 = 50.
        let (right, top) = note.expect("source note");
        assert!((right - 349.0).abs() < 0.01, "note ends at {right}");
        assert_eq!(top, 50.0);
    }

    /// A finer source interval is requested only when it is shorter than the chart's; its bars
    /// replace the estimate for the chart bars they fall in, which keep the estimate where there
    /// are none, so the total volume stays and the coverage is the share of bars with finer
    /// bars. A reply for another interval is dropped.
    #[test]
    fn finer_bars_replace_the_estimate_where_they_exist() {
        use crate::model::BarClock;
        let at = |minutes: i64| {
            "2026-03-02T09:00:00Z"
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap()
                + chrono::Duration::minutes(minutes)
        };
        // Two hourly bars over 100 to 110, and half-hour bars for the first only: 70 at the
        // bottom row and 30 at the top row.
        let chart = [
            Bar::new(at(0), 100.0, 110.0, 100.0, 105.0, 100.0),
            Bar::new(at(60), 100.0, 110.0, 100.0, 105.0, 100.0),
        ];
        let fine = [
            Bar::new(at(0), 100.0, 100.9, 100.0, 100.5, 70.0),
            Bar::new(at(30), 109.5, 110.0, 109.1, 109.5, 30.0),
        ];
        let source = |bars: &[Bar], timeframe, hinted| {
            let mut study = VolProfileFixed::new(10);
            study.set_timeframe(Timeframe::Hour1);
            if hinted {
                study.set_instrument(Some(
                    &Instrument::new("SYNTH")
                        .with_profile_resolutions([ProfileResolution::new(timeframe, None)]),
                ));
            } else {
                study
                    .set_input("source_interval", InputValue::Timeframe(Some(timeframe)))
                    .unwrap();
            }
            study.calculate(&chart);
            study.update_data(
                &chart,
                0,
                &[StudyData {
                    request: StudyDataRequest {
                        symbol: String::new(),
                        timeframe: Some(timeframe),
                        metric: None,
                    },
                    bars,
                    clock: BarClock::new(timeframe),
                    instrument: None,
                    timeframe,
                }],
            );
            study
        };
        let expected = [80.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 40.0];
        for (interval, hinted) in [
            (Timeframe::Hour1, false),
            (Timeframe::Hour1, true),
            (Timeframe::Custom(std::num::NonZeroU64::MAX), false),
        ] {
            let mut study = source(&fine, Timeframe::Min30, hinted);
            study.set_timeframe(interval);
            assert_eq!(study.data_requests()[0].timeframe, Some(Timeframe::Min30));
            let volume = study.volume_profiles(0..2)[0].profile.volume().to_vec();
            assert!(
                volume
                    .iter()
                    .zip(expected)
                    .all(|(v, e)| (v - e).abs() < 1e-9),
                "{volume:?}"
            );
            assert_eq!(
                study.profile_inputs(0..2),
                [ProfileInput {
                    bars: 0..2,
                    coverage: 0.5
                }]
            );
        }

        // A longer source interval is not requested, and another interval's reply is dropped.
        let mut longer = VolProfileFixed::new(10);
        longer.set_timeframe(Timeframe::Hour1);
        longer
            .set_input(
                "source_interval",
                InputValue::Timeframe(Some(Timeframe::Day1)),
            )
            .unwrap();
        assert!(longer.data_requests().is_empty());
        let mut other = source(&fine, Timeframe::Min30, false);
        other
            .set_input(
                "source_interval",
                InputValue::Timeframe(Some(Timeframe::Min15)),
            )
            .unwrap();
        other.calculate(&chart);
        other.update_data(&chart, 0, &[]);
        assert_eq!(other.profile_inputs(0..2)[0].coverage, 0.0);
    }
}
