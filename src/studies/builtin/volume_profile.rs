//! Volume profiles scoped to the viewport, saved times or exchange calendar periods.
//!
//! A profile distributes the volume of each chart bar over the price rows between its low and
//! high, which estimates where the volume traded. With a finer source interval the study asks
//! the host for that history, through [`Indicator::data_requests`], and distributes each finer
//! bar instead; a chart bar without finer bars keeps the estimate, so no volume is lost.
//! [`ProfileInput`] tells how much of a profile the finer bars cover.

use super::AutoAnchor;
use super::anchor_period::{AnchorPeriod, ExchangeDays};
use crate::data::{ProfileDisplay, StudyProfile, VolumeProfile, VolumeProfileConfig};
use crate::model::{Bar, Instrument, Timeframe};
use crate::studies::{
    Indicator, IndicatorValues, InputKind, InputValue, StudyData, StudyDataRequest, StudyInput,
};
use egui::Color32;
use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;

/// What a volume profile was built from.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileInput {
    /// The chart bars the profile covers.
    pub bars: Range<usize>,
    /// The share of those bars, from 0 to 1, whose volume comes from finer bars of the source
    /// interval. The volume of the other bars is an estimate spread over each bar's high to
    /// low range, so 0 marks a profile that is an estimate throughout.
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
    display: ProfileDisplay,
    start_time: f64,
    end_time: f64,
    period: AnchorPeriod,
    anchor: AutoAnchor,
    length: usize,
    interval: Timeframe,
    source: Option<Timeframe>,
    days: ExchangeDays,
    data: Vec<Bar>,
    fine: Vec<Bar>,
    ranges: Vec<Range<usize>>,
    cache: RefCell<Vec<CachedProfile>>,
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

impl ProfileStudy {
    fn new(kind: Kind, rows: usize) -> Self {
        Self {
            kind,
            rows: rows.max(1),
            value_area: 0.7,
            row_size: 0.0,
            display: ProfileDisplay::UpDown,
            start_time: -8e15,
            end_time: 8e15,
            period: AnchorPeriod::Week,
            anchor: AutoAnchor::Auto,
            length: 100,
            interval: Timeframe::Min1,
            source: None,
            days: ExchangeDays::default(),
            data: Vec::new(),
            fine: Vec::new(),
            ranges: Vec::new(),
            cache: RefCell::new(Vec::new()),
            values: IndicatorValues::new(),
            colors: vec![
                Color32::from_rgb(41, 98, 255),
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
        ];
        if matches!(self.kind, Kind::Fixed | Kind::Anchored) {
            inputs.push(StudyInput::new(
                "start_time",
                "Start (Unix ms)",
                InputValue::Number(self.start_time),
                InputKind::Number(-8e15, 8e15),
            ));
        }
        if self.kind == Kind::Fixed {
            inputs.push(StudyInput::new(
                "end_time",
                "End (Unix ms)",
                InputValue::Number(self.end_time),
                InputKind::Number(-8e15, 8e15),
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
        inputs
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        if !self
            .inputs()
            .iter()
            .any(|input| input.id == id && input.accepts(&value))
        {
            return Err("invalid volume profile input");
        }
        match (id, value) {
            ("row_count", InputValue::Length(v)) => self.rows = v,
            ("value_area", InputValue::Number(v)) => self.value_area = v / 100.0,
            ("row_size", InputValue::Number(v)) => self.row_size = v,
            ("display", InputValue::Choice(v)) => self.display = DISPLAYS[v],
            ("source_interval", InputValue::Timeframe(v)) => self.source = v,
            ("start_time", InputValue::Number(v)) => self.start_time = v,
            ("end_time", InputValue::Number(v)) => self.end_time = v,
            ("period", InputValue::Choice(v)) => self.period = PERIODS[v],
            ("anchor", InputValue::Choice(v)) => self.anchor = ANCHORS[v],
            ("length", InputValue::Length(v)) => self.length = v,
            _ => return Err("invalid volume profile input"),
        }
        self.values.clear();
        self.data.clear();
        self.fine.clear();
        self.ranges.clear();
        self.cache.get_mut().clear();
        Ok(())
    }

    /// The finer interval the profiles read: a source interval shorter than the chart's.
    fn finer(&self) -> Option<Timeframe> {
        self.source.filter(|source| {
            !matches!(source, Timeframe::Ticks(_))
                && source.duration_ms() < self.interval.duration_ms()
        })
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

    /// The bars that feed the profile of chart bars `range`: the finer bars inside each chart
    /// bar, or the chart bar itself where there are none, and the share of chart bars with
    /// finer bars.
    fn inputs_of(&self, range: &Range<usize>) -> (Vec<Bar>, f64) {
        let mut bars = Vec::new();
        let mut covered = 0;
        for i in range.clone() {
            let start = self.data[i].time;
            let end = self.data.get(i + 1).map_or_else(
                || start + chrono::Duration::milliseconds(self.interval.duration_ms()),
                |next| next.time,
            );
            let inside = &self.fine[self.fine.partition_point(|bar| bar.time < start)
                ..self.fine.partition_point(|bar| bar.time < end)];
            if inside.is_empty() {
                bars.push(self.data[i].clone());
            } else {
                covered += 1;
                bars.extend_from_slice(inside);
            }
        }
        (bars, covered as f64 / range.len() as f64)
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
        self.refresh_ranges();
    }

    fn update(&mut self, data: &[Bar], from: usize) {
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
    }

    fn profiles(&self, visible: Range<usize>) -> Vec<StudyProfile> {
        let visible = visible.start.min(self.data.len())..visible.end.min(self.data.len());
        if visible.is_empty() {
            return Vec::new();
        }
        let ranges = if self.kind == Kind::Visible {
            vec![visible]
        } else {
            self.ranges
                .iter()
                .filter(|range| range.start < visible.end && range.end > visible.start)
                .cloned()
                .collect()
        };
        let Some(mut config) = VolumeProfileConfig::new(self.rows, self.value_area)
            .and_then(|c| c.with_row_size((self.row_size > 0.0).then_some(self.row_size)))
        else {
            return Vec::new();
        };
        config.display = self.display;
        config.up_color = self.colors[1].to_srgba_unmultiplied();
        config.down_color = self.colors[2].to_srgba_unmultiplied();
        config.value_area_colors = Some([
            self.colors[3].to_srgba_unmultiplied(),
            self.colors[4].to_srgba_unmultiplied(),
        ]);
        let mut cache = self.cache.borrow_mut();
        let mut profiles = Vec::new();
        for range in ranges {
            if let Some(plot) = cache
                .iter()
                .find(|cached| cached.plot.bars == range && !cached.dirty)
            {
                profiles.push(plot.plot.clone());
            } else if self.finer().is_some() && !self.fine.is_empty() {
                let (bars, coverage) = self.inputs_of(&range);
                let Some(profile) = VolumeProfile::from_bars(&bars, config) else {
                    continue;
                };
                let plot = StudyProfile {
                    bars: range.clone(),
                    profile: Arc::new(profile),
                    config,
                    color: self.colors[0].to_srgba_unmultiplied(),
                    right_aligned: self.kind == Kind::Visible,
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
                        cached.plot.bars.start == range.start && cached.plot.bars.end <= range.end
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
            fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
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
            fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
                self.profile.update_data(data, from, sources);
            }
            fn volume_profiles(&self, visible: Range<usize>) -> Vec<StudyProfile> {
                self.profile.profiles(visible)
            }
            fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
                self.profile.days.set(instrument)
            }
            fn set_timeframe(&mut self, timeframe: Timeframe) {
                self.profile.interval = timeframe;
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
            study
                .set_input("value_area", InputValue::Number(70.0))
                .unwrap();
            study.calculate(&bars);
            let got = study.volume_profiles(1..4);
            assert_eq!(got.len(), expected.len(), "{}", study.name());
            for (got, (range, volume)) in got.iter().zip(expected) {
                assert_eq!(got.bars, range, "{}", study.name());
                assert_eq!(got.profile.volume(), [volume / 2.0, volume / 2.0]);
                let down = if range.contains(&2) { 15.0 } else { 0.0 };
                assert_eq!(got.profile.down(), [down, down]);
                assert_eq!(got.profile.up(), [volume / 2.0 - down, volume / 2.0 - down]);
            }
            let cached = study.volume_profiles(1..4);
            assert!(Arc::ptr_eq(&got[0].profile, &cached[0].profile));
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
    use crate::chart::renderers::{ChartMapping, IndicatorRenderer, RenderContext};
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::{model::Bar, studies::Indicator};

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
        let poc = output.shapes.iter().any(|shape| {
            matches!(&shape.shape,
            egui::Shape::Text(t) if t.galley.text() == "POC")
        });
        let rows = output
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape,
            egui::Shape::Rect(r) if (r.rect.height() - 49.0).abs() < 0.01)
            })
            .count();
        output.drop_without_applying_deltas();
        assert!(poc && rows >= 2, "profile rows {rows}, POC {poc}");
        for (x, expected) in [(395.0, true), (285.0, false)] {
            let hit = crate::chart::indicators::hit_test_indicator(
                egui::pos2(x, 125.0),
                indicators[0].as_ref(),
                0,
                0..1,
                &coords,
                |value| price.price_to_y(value),
            );
            assert_eq!(hit.is_some(), expected, "profile selection at x={x}");
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
        let mapping = crate::chart::renderers::study_mapping(&independent, &coords);
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
        let source = |bars: &[Bar], timeframe| {
            let mut study = VolProfileFixed::new(10);
            study.set_timeframe(Timeframe::Hour1);
            study
                .set_input("source_interval", InputValue::Timeframe(Some(timeframe)))
                .unwrap();
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
        let study = source(&fine, Timeframe::Min30);
        assert_eq!(study.data_requests()[0].timeframe, Some(Timeframe::Min30));
        let volume = study.volume_profiles(0..2)[0].profile.volume().to_vec();
        let expected = [80.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 40.0];
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
        let mut other = source(&fine, Timeframe::Min30);
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
