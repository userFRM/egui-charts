//! Volume Weighted Average Price (VWAP) Indicator
//!
//! VWAP is the average price weighted by volume over an anchor period: a trading
//! session, a calendar period or a host-supplied event. It restarts at the first bar of each period and is
//! commonly used as an execution benchmark.
//!
//! # Interpretation
//! - Price above VWAP: Bullish bias
//! - Price below VWAP: Bearish bias
//! - VWAP can act as support/resistance
//! - Institutional traders often use VWAP as execution benchmark
//!
//! # Calculation
//! VWAP = Sum(Typical Price * Volume) / Sum(Volume) since the start of the period
//! Typical Price = (High + Low + Close) / 3
//! Band = VWAP +/- k * sigma, where sigma^2 = Sum(Volume * Typical Price^2) / Sum(Volume) - VWAP^2
//!
//! # Example
//! ```
//! # #[cfg(feature = "timezones")] {
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use chrono::{NaiveTime, Weekday};
//! use egui_charts::model::{Session, SessionWindow, TimezoneId};
//! use egui_charts::studies::{Indicator, VolumeWeightedAvgPrice};
//!
//! // Sessions from 17:00 to 16:00 Chicago time, each named by the day it closes.
//! let weekdays = [Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri];
//! let hour = |h| NaiveTime::from_hms_opt(h, 0, 0).unwrap();
//! let mut vwap = VolumeWeightedAvgPrice::new()
//!     .with_time_zone(TimezoneId::named("America/Chicago").unwrap())
//!     .with_session(Session::new([SessionWindow::new(weekdays, hour(17), hour(16))]))
//!     .with_std_dev_bands([1.0, 2.0]);
//! vwap.calculate(&bars);
//! assert_eq!(vwap.line_count(), 5);
//! # }
//! ```

use super::clamp_param;
use crate::model::{Bar, EventKind, Session, SessionWindow, TimeScaleEvent, Timeframe, TimezoneId};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use chrono::{
    DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone, Timelike, Utc,
};
use egui::Color32;

/// The period over which [`VolumeWeightedAvgPrice`] accumulates before it restarts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum VwapAnchor {
    /// Each trading day.
    #[default]
    Session,
    /// Each ISO week, Monday to Sunday, of trading days.
    Week,
    /// Each calendar month of trading days.
    Month,
    /// Each calendar quarter of trading days.
    Quarter,
    /// Each calendar year of trading days.
    Year,
    /// Restart at timestamps the host supplies with `with_event_anchors` or types into the inputs.
    Events,
    /// Each decade of trading days, 2020 to 2029 for example.
    Decade,
    /// Each century of trading days, 2000 to 2099 for example.
    Century,
    /// Restart at each earnings event the host gives the chart for its symbol.
    Earnings,
    /// Restart at each dividend event the host gives the chart for its symbol.
    Dividends,
    /// Restart at each split event the host gives the chart for its symbol.
    Splits,
}

const VWAP_ANCHORS: [VwapAnchor; 11] = [
    VwapAnchor::Session,
    VwapAnchor::Week,
    VwapAnchor::Month,
    VwapAnchor::Quarter,
    VwapAnchor::Year,
    VwapAnchor::Events,
    VwapAnchor::Decade,
    VwapAnchor::Century,
    VwapAnchor::Earnings,
    VwapAnchor::Dividends,
    VwapAnchor::Splits,
];

/// The most bands that the study inputs can hold.
const MAX_BANDS: usize = 10;
/// The most event anchors that the study inputs can hold.
const MAX_EVENTS: usize = 1000;

/// The local date and time of an event anchor in inputs, to the second.
const EVENT_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// The index in a slot input id such as `band_2`.
fn slot(id: &str, prefix: &str) -> Option<usize> {
    id.strip_prefix(prefix)?.parse().ok()
}

/// Minutes after midnight of the earliest open of a window in `windows` that spans midnight,
/// where [`trading_day`] starts the next trading day, or 0 for none: all it reads of them.
fn day_start(windows: &[SessionWindow]) -> usize {
    windows
        .iter()
        .filter(|w| w.close <= w.open && w.open > NaiveTime::MIN)
        .map(|w| (w.open.hour() * 60 + w.open.minute()) as usize)
        .min()
        .unwrap_or(0)
}

/// The one window that starts each trading day `minutes` after midnight; none at midnight.
fn day_start_windows(minutes: usize) -> Vec<SessionWindow> {
    NaiveTime::from_hms_opt((minutes / 60) as u32, (minutes % 60) as u32, 0)
        .filter(|open| *open > NaiveTime::MIN)
        .map(|open| {
            vec![SessionWindow::new(
                Vec::<chrono::Weekday>::new(),
                open,
                open,
            )]
        })
        .unwrap_or_default()
}

impl VwapAnchor {
    /// Identifies the period that holds trading day `day`.
    fn period(self, day: NaiveDate) -> (i32, u32) {
        match self {
            Self::Session => (day.year(), day.ordinal()),
            Self::Week => (day.iso_week().year(), day.iso_week().week()),
            Self::Month => (day.year(), day.month()),
            Self::Quarter => (day.year(), (day.month() - 1) / 3),
            Self::Year => (day.year(), 0),
            Self::Decade => (day.year().div_euclid(10), 0),
            Self::Century => (day.year().div_euclid(100), 0),
            Self::Events | Self::Earnings | Self::Dividends | Self::Splits => (0, 0),
        }
    }
}

/// The trading day of `time`: its date in `zone`, or the next date from the open of a
/// window that spans midnight, since a window is named by the day on which it closes.
pub(super) fn trading_day(
    time: DateTime<Utc>,
    zone: &TimezoneId,
    windows: &[SessionWindow],
) -> NaiveDate {
    let utc = time.naive_utc();
    // Near the ends of the representable range the local time may not exist; keep UTC there.
    let local = utc
        .checked_add_offset(zone.offset_from_utc_datetime(&utc).fix())
        .unwrap_or(utc);
    let date = local.date();
    // A window from 00:00 to 00:00 lies within its opening day.
    if windows
        .iter()
        .any(|w| w.close <= w.open && w.open > NaiveTime::MIN && local.time() >= w.open)
    {
        date.succ_opt().unwrap_or(date)
    } else {
        date
    }
}

/// Volume Weighted Average Price, restarted at each anchor period, with optional
/// standard deviation bands.
///
/// [`new`](Self::new) restarts at each UTC day until a time zone is set with
/// [`with_time_zone`](Self::with_time_zone). Pass the instrument's
/// [`time_zone`](crate::model::Instrument::time_zone) and
/// [`session`](crate::model::Instrument::session) so that sessions which cross
/// midnight in the exchange zone, or UTC midnight, stay one period.
#[derive(Clone)]
pub struct VolumeWeightedAvgPrice {
    values: IndicatorValues,
    stepper: Stepper<State>,
    /// First bar sharing the preceding bar's UTC date, if any.
    shared: Option<usize>,
    /// Whether no two consecutive bars shared a UTC date at the last update.
    daily: bool,
    timeframe: Option<Timeframe>,
    /// One per line: the VWAP, then the upper and lower band of each multiplier.
    colors: Vec<Color32>,
    visible: bool,
    anchor: VwapAnchor,
    time_zone: TimezoneId,
    windows: Vec<SessionWindow>,
    instrument_session: Option<(TimezoneId, Vec<SessionWindow>)>,
    bands: Vec<f64>,
    percentage_bands: bool,
    event_anchors: Vec<DateTime<Utc>>,
    /// The zone in which the event anchor inputs are written.
    event_zone: TimezoneId,
    /// The earnings, dividends and splits the chart holds for its symbol.
    chart_events: Vec<(EventKind, DateTime<Utc>)>,
}

#[derive(Clone, Default)]
struct State {
    period: Option<(i32, u32)>,
    events: usize,
    volume: f64,
    tp_volume: f64,
    tp2_volume: f64,
}

impl VolumeWeightedAvgPrice {
    /// A VWAP restarted at each session, in UTC with no session windows, without bands.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            shared: None,
            daily: true,
            timeframe: None,
            colors: vec![palette::BLUE],
            visible: true,
            anchor: VwapAnchor::Session,
            time_zone: TimezoneId::utc(),
            windows: Vec::new(),
            instrument_session: None,
            bands: Vec::new(),
            percentage_bands: false,
            event_anchors: Vec::new(),
            event_zone: TimezoneId::utc(),
            chart_events: Vec::new(),
        }
    }

    /// Restarts at each selected calendar period or host event.
    pub fn with_anchor(mut self, anchor: VwapAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// The exchange time zone, in which trading days are counted. A bar shorter than a
    /// day counts on the exchange date of its timestamp. A bar of a day or longer, which
    /// a feed stamps inside its UTC period ([`HistoryPage::bars`](crate::data::HistoryPage::bars)),
    /// counts on its UTC date. Without an explicit timeframe, bars are taken to be a
    /// day or longer when no two consecutive bars share a UTC date.
    pub fn with_time_zone(mut self, time_zone: TimezoneId) -> Self {
        self.time_zone = time_zone;
        self
    }

    /// The trading hours, in exchange time. A window that spans midnight starts the next
    /// trading day at its open. The study keeps the earliest such open, to the minute, as its
    /// trading day start input, which is all the calculation reads of the session.
    pub fn with_session(mut self, session: Session) -> Self {
        self.windows = day_start_windows(day_start(&session.windows));
        self
    }

    /// Adds an upper and a lower band at each multiple of the volume-weighted
    /// standard deviation of the typical price over the period. Multipliers clamp
    /// to 0 or more, NaN to 0, and at most ten are kept. The bands are drawn in the VWAP
    /// colour at half intensity.
    pub fn with_std_dev_bands(mut self, multipliers: impl IntoIterator<Item = f64>) -> Self {
        self.percentage_bands = false;
        self.bands = multipliers
            .into_iter()
            .take(MAX_BANDS)
            .map(|k| clamp_param(k, 0.0, f64::MAX))
            .collect();
        self.colors.truncate(1);
        self.fit_colors();
        self
    }

    /// One colour per line, with green, olive and dark teal for successive band pairs.
    fn fit_colors(&mut self) {
        let colors = [
            palette::GREEN,
            Color32::from_rgb(128, 128, 0),
            Color32::from_rgb(0, 137, 123),
        ];
        while self.colors.len() < self.line_count() {
            let band = (self.colors.len() - 1) / 2;
            self.colors.push(colors[band.min(2)]);
        }
    }

    /// Adds symmetric bands at percentages of the absolute VWAP. Percentages
    /// clamp to 0 or more, NaN to 0, and replace any standard deviation bands.
    pub fn with_percentage_bands(self, percentages: impl IntoIterator<Item = f64>) -> Self {
        let mut study = self.with_std_dev_bands(percentages);
        study.percentage_bands = true;
        study
    }

    /// Restart on the first bar at or after each event timestamp. The host supplies
    /// the chosen earnings, dividend or split events; no event feed is requested.
    /// Timestamps are sorted and deduplicated. Selects `VwapAnchor::Events`. The study
    /// inputs write them in the zone of the "Event time zone" input, UTC by default.
    pub fn with_event_anchors(mut self, times: impl IntoIterator<Item = DateTime<Utc>>) -> Self {
        self.event_anchors = times.into_iter().collect();
        self.event_anchors.sort_unstable();
        self.event_anchors.dedup();
        self.anchor = VwapAnchor::Events;
        self
    }

    /// Set the VWAP line color; the bands take it at half intensity.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.colors.fill(color.gamma_multiply(0.5));
        self.colors[0] = color;
        self
    }
}

impl Default for VolumeWeightedAvgPrice {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for VolumeWeightedAvgPrice {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::HLC3,
        ))
    }

    fn inputs(&self) -> Vec<crate::studies::StudyInput> {
        use crate::studies::{InputKind, InputValue, StudyInput};
        let mut inputs = vec![
            StudyInput::new(
                "anchor",
                "Anchor",
                InputValue::Choice(self.anchor as usize),
                InputKind::Choice(
                    VWAP_ANCHORS
                        .iter()
                        .map(|anchor| format!("{anchor:?}"))
                        .collect(),
                ),
            )
            .with_default(InputValue::Choice(0)),
            StudyInput::new(
                "time_zone",
                "Time zone",
                InputValue::Text(self.time_zone.name().to_owned()),
                InputKind::Text,
            )
            .with_default(InputValue::Text("UTC".to_owned())),
            StudyInput::new(
                "day_start",
                "Trading day start, minutes after midnight",
                InputValue::Length(day_start(&self.windows)),
                InputKind::Length(0, 1439),
            )
            .with_default(InputValue::Length(0)),
            StudyInput::new(
                "percentage_bands",
                "Percentage bands",
                InputValue::Toggle(self.percentage_bands),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(false)),
            StudyInput::new(
                "band_count",
                "Bands",
                InputValue::Length(self.bands.len()),
                InputKind::Length(0, MAX_BANDS),
            )
            .with_default(InputValue::Length(0)),
        ];
        inputs.extend(self.bands.iter().enumerate().map(|(i, k)| {
            StudyInput::new(
                format!("band_{i}"),
                format!("Band {} multiplier", i + 1),
                InputValue::Number(*k),
                InputKind::Number(0.0, f64::MAX),
            )
            .with_default(InputValue::Number((i + 1) as f64))
        }));
        inputs.extend([
            StudyInput::new(
                "event_time_zone",
                "Event time zone",
                InputValue::Text(self.event_zone.name().to_owned()),
                InputKind::Text,
            )
            .with_default(InputValue::Text("UTC".to_owned())),
            StudyInput::new(
                "event_count",
                "Events",
                InputValue::Length(self.event_anchors.len()),
                InputKind::Length(0, MAX_EVENTS),
            )
            .with_default(InputValue::Length(0)),
        ]);
        inputs.extend(self.event_anchors.iter().enumerate().map(|(i, time)| {
            let local = time.with_timezone(&self.event_zone).naive_local();
            StudyInput::new(
                format!("event_{i}"),
                format!("Event {} (YYYY-MM-DD HH:MM:SS)", i + 1),
                InputValue::Text(local.format(EVENT_FORMAT).to_string()),
                InputKind::Text,
            )
            .with_default(InputValue::Text(String::new()))
        }));
        inputs
    }

    /// `band_count` and `event_count` resize their lists, so they come first among the slots
    /// of an applied set of inputs; a slot past the end of its list has nothing to edit and is
    /// accepted unchanged. Times are read in the current "Event time zone", and a repeated
    /// local time takes its first occurrence. Events are applied sorted and deduplicated.
    /// `time_zone` and `day_start` count trading days while the chart passes no instrument; an
    /// instrument's time zone and session, given with
    /// [`set_instrument`](Indicator::set_instrument), take their place.
    fn set_input(
        &mut self,
        id: &str,
        value: crate::studies::InputValue,
    ) -> Result<(), &'static str> {
        use crate::studies::InputValue;
        // Older templates stored this inferred flag; it never controlled the calculation.
        if id == "daily" && matches!(value, InputValue::Toggle(_)) {
            return Ok(());
        }
        if slot(id, "band_").is_some_and(|i| (self.bands.len()..MAX_BANDS).contains(&i))
            || slot(id, "event_")
                .is_some_and(|i| (self.event_anchors.len()..MAX_EVENTS).contains(&i))
        {
            return Ok(());
        }
        let schema = self
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .ok_or("unknown study input")?;
        if !schema.accepts(&value) {
            return Err("study input is outside its accepted values");
        }
        match (id, value) {
            ("anchor", InputValue::Choice(index)) => self.anchor = VWAP_ANCHORS[index],
            ("time_zone", InputValue::Text(name)) => {
                self.time_zone = TimezoneId::named(&name).ok_or("unknown time zone")?;
            }
            ("day_start", InputValue::Length(minutes)) => self.windows = day_start_windows(minutes),
            ("percentage_bands", InputValue::Toggle(value)) => self.percentage_bands = value,
            ("band_count", InputValue::Length(count)) => {
                let mut next = self.bands.len();
                self.bands.resize_with(count, || {
                    next += 1;
                    next as f64
                });
                self.fit_colors();
            }
            ("event_time_zone", InputValue::Text(name)) => {
                self.event_zone = TimezoneId::named(&name).ok_or("unknown time zone")?;
            }
            ("event_count", InputValue::Length(count)) => {
                self.event_anchors.resize(count, DateTime::UNIX_EPOCH);
            }
            (id, InputValue::Number(value)) if slot(id, "band_").is_some() => {
                self.bands[slot(id, "band_").unwrap()] = value;
            }
            (id, InputValue::Text(text)) if slot(id, "event_").is_some() => {
                let local = NaiveDateTime::parse_from_str(&text, EVENT_FORMAT)
                    .or_else(|_| NaiveDateTime::parse_from_str(&text, "%Y-%m-%d %H:%M"))
                    .map_err(|_| "event time must read YYYY-MM-DD HH:MM:SS")?;
                self.event_anchors[slot(id, "event_").unwrap()] = self
                    .event_zone
                    .from_local_datetime(&local)
                    .earliest()
                    .ok_or("event time does not exist in the time zone")?
                    .with_timezone(&Utc);
            }
            _ => return Err("invalid VWAP input"),
        }
        self.calculate(&[]);
        Ok(())
    }

    fn name(&self) -> &str {
        "VWAP"
    }

    fn desc(&self) -> &str {
        "Volume Weighted Avg Price - Trading benchmark indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let from = from.min(self.values.len()).min(data.len());
        // The daily rule reads all bars, including those with unusable prices or volume.
        self.shared = self.shared.filter(|&at| at < from).or_else(|| {
            (from.max(1)..data.len())
                .find(|&at| data[at - 1].time.date_naive() == data[at].time.date_naive())
        });
        let daily = self.timeframe.map_or(self.shared.is_none(), |tf| {
            tf.duration_ms() >= Timeframe::Day1.duration_ms()
        });
        let from = if daily != self.daily { 0 } else { from };
        self.daily = daily;
        let (zone, windows) = self.instrument_session.as_ref().map_or(
            (&self.time_zone, self.windows.as_slice()),
            |(zone, windows)| (zone, windows.as_slice()),
        );
        let chart_kind = match self.anchor {
            VwapAnchor::Earnings => Some(EventKind::Earnings),
            VwapAnchor::Dividends => Some(EventKind::Dividend),
            VwapAnchor::Splits => Some(EventKind::Split),
            _ => None,
        };
        // Inputs keep the events in the order they were written.
        let mut events: Vec<_> = if self.anchor == VwapAnchor::Events {
            self.event_anchors.clone()
        } else {
            self.chart_events
                .iter()
                .filter(|(kind, _)| Some(*kind) == chart_kind)
                .map(|(_, time)| *time)
                .collect()
        };
        events.sort_unstable();
        events.dedup();
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                // A bar with a non-finite price or volume has no value and adds nothing.
                if ![bar.high, bar.low, bar.close, bar.volume]
                    .iter()
                    .all(|x| x.is_finite())
                {
                    return IndicatorValue::None;
                }
                let day = if daily {
                    bar.time.date_naive()
                } else {
                    trading_day(bar.time, zone, windows)
                };
                let current = Some(self.anchor.period(day));
                let events = events.partition_point(|time| *time <= bar.time);
                if current != state.period || events != state.events {
                    state.events = events;
                    state.period = current;
                    (state.volume, state.tp_volume, state.tp2_volume) = (0.0, 0.0, 0.0);
                }

                let tp = bar.typical_price();
                state.volume += bar.volume;
                state.tp_volume += tp * bar.volume;
                state.tp2_volume += tp * tp * bar.volume;

                let (vwap, sigma) = if state.volume == 0.0 {
                    (tp, 0.0)
                } else {
                    let vwap = state.tp_volume / state.volume;
                    // Clamped: cancellation can leave a tiny negative variance.
                    (
                        vwap,
                        (state.tp2_volume / state.volume - vwap * vwap)
                            .max(0.0)
                            .sqrt(),
                    )
                };
                let band_unit = if self.percentage_bands {
                    vwap.abs() / 100.0
                } else {
                    sigma
                };
                if self.bands.is_empty() {
                    IndicatorValue::Single(vwap)
                } else {
                    IndicatorValue::Multiple(
                        std::iter::once(vwap)
                            .chain(
                                self.bands
                                    .iter()
                                    .flat_map(|k| [vwap + k * band_unit, vwap - k * band_unit]),
                            )
                            .collect(),
                    )
                }
            });
    }

    fn set_timeframe(&mut self, timeframe: Timeframe) {
        self.timeframe = Some(timeframe);
    }

    fn set_instrument(&mut self, instrument: Option<&crate::model::Instrument>) -> bool {
        let session = instrument.map(|instrument| {
            (
                instrument.time_zone.clone().unwrap_or_else(TimezoneId::utc),
                instrument
                    .session
                    .as_ref()
                    .map_or(&[][..], |s| s.windows.as_slice()),
            )
        });
        if self
            .instrument_session
            .as_ref()
            .map(|(zone, windows)| (zone, windows.as_slice()))
            == session.as_ref().map(|(zone, windows)| (zone, *windows))
        {
            return false;
        }
        self.instrument_session = session.map(|(zone, windows)| (zone, windows.to_vec()));
        true
    }

    fn set_time_scale_events(&mut self, events: &[TimeScaleEvent]) -> bool {
        let corporate = |event: &&TimeScaleEvent| {
            matches!(
                event.kind,
                EventKind::Earnings | EventKind::Dividend | EventKind::Split
            )
        };
        let pairs = || {
            events
                .iter()
                .filter(corporate)
                .map(|event| (event.kind, event.time))
        };
        // Called every frame: no allocation while the events are unchanged.
        if self.chart_events.iter().copied().eq(pairs()) {
            return false;
        }
        self.chart_events = pairs().collect();
        matches!(
            self.anchor,
            VwapAnchor::Earnings | VwapAnchor::Dividends | VwapAnchor::Splits
        )
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    /// Sets the colors line by line; lines without a color keep theirs.
    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (line, color) in self.colors.iter_mut().zip(colors) {
            *line = color;
        }
    }

    fn is_overlay(&self) -> bool {
        true // VWAP is drawn on the main chart
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }

    fn line_count(&self) -> usize {
        1 + 2 * self.bands.len()
    }

    fn line_names(&self) -> Vec<String> {
        std::iter::once("VWAP".to_string())
            .chain(self.bands.iter().flat_map(|k| {
                let unit = if self.percentage_bands { "%" } else { "σ" };
                [format!("Upper {k}{unit}"), format!("Lower {k}{unit}")]
            }))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::InputValue;
    use chrono::{NaiveDateTime, NaiveTime, TimeZone, Weekday};

    struct Case {
        name: &'static str,
        /// `None` keeps the UTC default of `new()`.
        zone: Option<&'static str>,
        /// Open and close hour of one window, Monday to Friday.
        window: Option<(u32, u32)>,
        anchor: VwapAnchor,
        bands: &'static [f64],
        /// Exchange-local time, then high, low, close and volume.
        bars: &'static [(&'static str, [f64; 4])],
        /// VWAP, then the upper and lower band of each multiplier.
        want: &'static [&'static [f64]],
    }

    /// Expected values come from the anchored VWAP definition, sum(tp * v) / sum(v) with
    /// tp = (h + l + c) / 3 restarted at each period (Berkowitz, Logue and Noser, "The Total
    /// Cost of Transactions on the NYSE", Journal of Finance 43(1), 1988), with bands at k
    /// volume-weighted population standard deviations of tp, and trading days named by the
    /// day on which their session window closes in the exchange zone. They are computed
    /// independently with exact fractions.
    #[test]
    fn calculate_matches_reference() {
        let cases = [
            Case {
                name: "new() restarts at UTC midnight",
                zone: None,
                window: None,
                anchor: VwapAnchor::Session,
                bands: &[],
                bars: &[
                    ("2026-03-02 22:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-03-02 23:00", [103.0, 100.0, 102.0, 2000.0]),
                    ("2026-03-03 00:00", [104.0, 101.0, 103.0, 1500.0]),
                    ("2026-03-03 01:00", [102.0, 100.0, 101.0, 500.0]),
                ],
                want: &[
                    &[100.0],
                    &[101.11111111111111],
                    &[102.66666666666667],
                    &[102.25],
                ],
            },
            Case {
                // The 10:00 open is 23:00 UTC on the previous day.
                name: "Sydney session across UTC midnight",
                zone: Some("Australia/Sydney"),
                window: Some((10, 16)),
                anchor: VwapAnchor::Session,
                bands: &[],
                bars: &[
                    ("2026-03-02 15:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-03-03 10:00", [103.0, 101.0, 102.0, 2000.0]),
                    ("2026-03-03 11:00", [104.0, 102.0, 103.0, 1500.0]),
                    ("2026-03-03 12:00", [102.0, 100.0, 101.0, 800.0]),
                ],
                want: &[
                    &[100.0],
                    &[102.0],
                    &[102.42857142857143],
                    &[102.16279069767442],
                ],
            },
            Case {
                // The 17:00 open starts the next trading day; UTC midnight is 18:00.
                name: "Chicago session across local midnight",
                zone: Some("America/Chicago"),
                window: Some((17, 16)),
                anchor: VwapAnchor::Session,
                bands: &[],
                bars: &[
                    ("2026-03-02 14:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-03-02 15:00", [102.0, 100.0, 101.0, 1200.0]),
                    ("2026-03-02 17:00", [103.0, 101.0, 102.0, 900.0]),
                    ("2026-03-02 18:00", [104.0, 102.0, 103.0, 1100.0]),
                    ("2026-03-03 09:00", [105.0, 103.0, 104.0, 700.0]),
                ],
                want: &[
                    &[100.0],
                    &[100.54545454545455],
                    &[102.0],
                    &[102.55],
                    &[102.92592592592592],
                ],
            },
            Case {
                // Intraday bars: Sunday 17:00, the day daylight saving starts, opens Monday's session.
                name: "Chicago week",
                zone: Some("America/Chicago"),
                window: Some((17, 16)),
                anchor: VwapAnchor::Week,
                bands: &[],
                bars: &[
                    ("2026-03-05 15:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-03-06 15:00", [102.0, 100.0, 101.0, 1200.0]),
                    ("2026-03-08 17:00", [99.0, 97.0, 98.0, 900.0]),
                    ("2026-03-08 18:00", [100.0, 98.0, 99.0, 600.0]),
                    ("2026-03-09 17:00", [100.0, 98.0, 99.0, 1100.0]),
                ],
                want: &[
                    &[100.0],
                    &[100.54545454545455],
                    &[98.0],
                    &[98.4],
                    &[98.65384615384616],
                ],
            },
            Case {
                // Daily bars at 00:00 UTC on Thu 4, Fri 5, Mon 8 and Tue 9 January, 19:00 the
                // evening before in New York: the week restarts on the Monday bar.
                name: "New York daily bars at UTC midnight, week",
                zone: Some("America/New_York"),
                window: None,
                anchor: VwapAnchor::Week,
                bands: &[],
                bars: &[
                    ("2024-01-03 19:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2024-01-04 19:00", [102.0, 100.0, 101.0, 1200.0]),
                    ("2024-01-07 19:00", [99.0, 97.0, 98.0, 900.0]),
                    ("2024-01-08 19:00", [100.0, 98.0, 99.0, 1100.0]),
                ],
                want: &[&[100.0], &[100.54545454545455], &[98.0], &[98.55]],
            },
            Case {
                // The forming daily bar pushed with its trade time: Tuesday's last push at
                // 20:59 UTC, then Wednesday's first at 14:30 UTC, after daily bars at 00:00 UTC.
                name: "New York daily bars with live stamps, week",
                zone: Some("America/New_York"),
                window: None,
                anchor: VwapAnchor::Week,
                bands: &[],
                bars: &[
                    ("2024-01-03 19:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2024-01-04 19:00", [102.0, 100.0, 101.0, 1200.0]),
                    ("2024-01-07 19:00", [99.0, 97.0, 98.0, 900.0]),
                    ("2024-01-09 15:59", [100.0, 98.0, 99.0, 1100.0]),
                    ("2024-01-10 09:30", [101.0, 99.0, 100.0, 800.0]),
                ],
                want: &[
                    &[100.0],
                    &[100.54545454545455],
                    &[98.0],
                    &[98.55],
                    &[98.96428571428571],
                ],
            },
            Case {
                // Daily bars at 23:00 UTC on 30 and 31 January and 1 and 2 February, 08:00 the
                // next morning in Tokyo: the month restarts on the 1 February bar.
                name: "Tokyo daily bars before UTC midnight, month",
                zone: Some("Asia/Tokyo"),
                window: None,
                anchor: VwapAnchor::Month,
                bands: &[],
                bars: &[
                    ("2024-01-31 08:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2024-02-01 08:00", [102.0, 100.0, 101.0, 1200.0]),
                    ("2024-02-02 08:00", [99.0, 97.0, 98.0, 900.0]),
                    ("2024-02-03 08:00", [100.0, 98.0, 99.0, 1100.0]),
                ],
                want: &[&[100.0], &[100.54545454545455], &[98.0], &[98.55]],
            },
            Case {
                name: "UTC month",
                zone: None,
                window: None,
                anchor: VwapAnchor::Month,
                bands: &[],
                bars: &[
                    ("2026-01-30 12:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-01-31 12:00", [102.0, 100.0, 101.0, 1200.0]),
                    ("2026-02-01 12:00", [99.0, 97.0, 98.0, 900.0]),
                    ("2026-02-02 12:00", [100.0, 98.0, 99.0, 1100.0]),
                ],
                want: &[&[100.0], &[100.54545454545455], &[98.0], &[98.55]],
            },
            Case {
                // Intraday bars in a 24-hour session from 00:00 to 00:00: each keeps its own date.
                name: "UTC month with a 24-hour session",
                zone: None,
                window: Some((0, 0)),
                anchor: VwapAnchor::Month,
                bands: &[],
                bars: &[
                    ("2026-01-30 12:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-01-31 12:00", [102.0, 100.0, 101.0, 1200.0]),
                    ("2026-02-01 12:00", [99.0, 97.0, 98.0, 900.0]),
                    ("2026-02-01 13:00", [100.0, 98.0, 99.0, 600.0]),
                    ("2026-02-02 12:00", [100.0, 98.0, 99.0, 1100.0]),
                ],
                want: &[
                    &[100.0],
                    &[100.54545454545455],
                    &[98.0],
                    &[98.4],
                    &[98.65384615384616],
                ],
            },
            Case {
                // No restart at the year, a restart at the decade.
                name: "UTC decade",
                zone: None,
                window: None,
                anchor: VwapAnchor::Decade,
                bands: &[],
                bars: &[
                    ("2028-12-29 00:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2029-01-02 00:00", [103.0, 101.0, 102.0, 1000.0]),
                    ("2030-01-02 00:00", [111.0, 109.0, 110.0, 1000.0]),
                ],
                want: &[&[100.0], &[101.0], &[110.0]],
            },
            Case {
                // No restart at the decade, a restart at the century.
                name: "UTC century",
                zone: None,
                window: None,
                anchor: VwapAnchor::Century,
                bands: &[],
                bars: &[
                    ("2089-12-30 00:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2090-01-03 00:00", [103.0, 101.0, 102.0, 1000.0]),
                    ("2100-01-04 00:00", [111.0, 109.0, 110.0, 1000.0]),
                ],
                want: &[&[100.0], &[101.0], &[110.0]],
            },
            Case {
                // The second day opens on a bar without volume: its typical price.
                name: "bands",
                zone: None,
                window: None,
                anchor: VwapAnchor::Session,
                bands: &[1.0, 2.5],
                bars: &[
                    ("2026-03-02 10:00", [101.0, 99.0, 100.0, 1000.0]),
                    ("2026-03-02 11:00", [104.0, 100.0, 102.0, 2000.0]),
                    ("2026-03-02 12:00", [103.0, 97.0, 99.0, 1500.0]),
                    ("2026-03-03 10:00", [102.0, 100.0, 101.0, 0.0]),
                    ("2026-03-03 11:00", [104.0, 102.0, 103.0, 500.0]),
                ],
                want: &[
                    &[100.0, 100.0, 100.0, 100.0, 100.0],
                    &[
                        101.33333333333333,
                        102.2761423749154,
                        100.39052429175128,
                        103.69035593728849,
                        98.97631072937817,
                    ],
                    &[
                        100.77777777777777,
                        101.87772165962352,
                        99.67783389593204,
                        103.52763748239212,
                        98.02791807316342,
                    ],
                    &[101.0, 101.0, 101.0, 101.0, 101.0],
                    &[103.0, 103.0, 103.0, 103.0, 103.0],
                ],
            },
        ];

        let weekdays = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
        ];
        let hour = |h| NaiveTime::from_hms_opt(h, 0, 0).unwrap();
        let mut failures = Vec::new();
        for case in cases {
            let Some(zone) = TimezoneId::named(case.zone.unwrap_or("UTC")) else {
                continue;
            };
            let bars: Vec<Bar> = case
                .bars
                .iter()
                .map(|(local, [h, l, c, v])| {
                    let local = NaiveDateTime::parse_from_str(local, "%Y-%m-%d %H:%M").unwrap();
                    let time = zone.from_local_datetime(&local).single().unwrap();
                    Bar::new(time.with_timezone(&Utc), *c, *h, *l, *c, *v)
                })
                .collect();
            let mut study = VolumeWeightedAvgPrice::new();
            if let Some(name) = case.zone {
                study = study.with_time_zone(TimezoneId::named(name).unwrap());
            }
            if let Some((open, close)) = case.window {
                let window = SessionWindow::new(weekdays, hour(open), hour(close));
                study = study.with_session(Session::new([window]));
            }
            if case.anchor != VwapAnchor::default() {
                study = study.with_anchor(case.anchor);
            }
            study = study.with_std_dev_bands(case.bands.iter().copied());
            study.calculate(&bars);

            let lines = 1 + 2 * case.bands.len();
            assert_eq!(study.line_count(), lines, "{}", case.name);
            let names = study.line_names();
            assert_eq!(names.len(), lines, "{}", case.name);
            assert_eq!(names[0], "VWAP");
            for (band, multiplier) in case.bands.iter().enumerate() {
                assert_eq!(names[1 + band * 2], format!("Upper {multiplier}σ"));
                assert_eq!(names[2 + band * 2], format!("Lower {multiplier}σ"));
            }
            assert_eq!(study.colors().len(), lines, "{}", case.name);
            assert_eq!(study.values().len(), case.want.len(), "{}", case.name);
            for (i, (got, want)) in study.values().to_rows().iter().zip(case.want).enumerate() {
                let got = match got {
                    IndicatorValue::Single(v) if case.bands.is_empty() => vec![*v],
                    IndicatorValue::Multiple(vs) if !case.bands.is_empty() => vs.clone(),
                    other => panic!("{} bar {i}: {other:?}", case.name),
                };
                let close = got.len() == want.len()
                    && got
                        .iter()
                        .zip(*want)
                        .all(|(g, w)| (g - w).abs() <= 1e-12 * w.abs());
                if !close {
                    failures.push(format!("{} bar {i}: got {got:?}, want {want:?}", case.name));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Equal volumes at 10 and 14 give 12; a reset then starts at 18.
    /// One-percent bands are 0.10, 0.12 and 0.18 from the VWAP.
    #[test]
    fn calendar_and_host_event_anchors_with_percentage_bands() {
        let times = [
            Utc.with_ymd_and_hms(2025, 12, 30, 12, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2025, 12, 31, 12, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 1, 2, 12, 0, 0).unwrap(),
        ];
        for anchor in [VwapAnchor::Quarter, VwapAnchor::Year, VwapAnchor::Events] {
            let times = if anchor == VwapAnchor::Quarter {
                [
                    Utc.with_ymd_and_hms(2026, 3, 30, 12, 0, 0).unwrap(),
                    Utc.with_ymd_and_hms(2026, 3, 31, 12, 0, 0).unwrap(),
                    Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap(),
                ]
            } else {
                times
            };
            let mut study = VolumeWeightedAvgPrice::new().with_anchor(anchor);
            if anchor == VwapAnchor::Events {
                // Duplicate events in a closed-market gap still cause exactly one reset.
                let event = times[2] - chrono::Duration::hours(12);
                study = study.with_event_anchors([event, event, times[0]]);
            }
            study = study.with_percentage_bands([1.0]);
            assert_eq!(study.line_names(), ["VWAP", "Upper 1%", "Lower 1%"]);
            let bars: Vec<_> = times
                .into_iter()
                .zip([10.0, 14.0, 18.0])
                .map(|(t, c)| Bar::new(t, c, c, c, c, 1.0))
                .collect();
            study.calculate(&bars);
            for (got, want) in study.values().to_rows().iter().zip([10.0, 12.0, 18.0]) {
                let IndicatorValue::Multiple(lines) = got else {
                    panic!("{got:?}")
                };
                for (actual, expected) in lines.iter().zip([want, want * 1.01, want * 0.99]) {
                    assert!((actual - expected).abs() < 1e-12, "{anchor:?}: {got:?}");
                }
            }
        }
    }

    /// A dialog-created VWAP inherits metadata both when added and when metadata arrives later.
    #[cfg(feature = "widget")]
    #[cfg(feature = "timezones")]
    #[test]
    fn chart_supplies_the_exchange_session_to_vwap() {
        use crate::model::{BarData, Instrument};
        use crate::widget::{Chart, test_frames::shapes};
        let bars: Vec<_> = [23, 24, 25]
            .into_iter()
            .zip([10.0, 14.0, 18.0])
            .map(|(hour, c)| {
                Bar::new(
                    Utc.with_ymd_and_hms(2026, 3, 2, 0, 0, 0).unwrap()
                        + chrono::Duration::hours(hour),
                    c,
                    c,
                    c,
                    c,
                    1.0,
                )
            })
            .collect();
        let instrument = Instrument::new("SESSION")
            .with_time_zone(TimezoneId::named("America/New_York").unwrap());
        for late in [false, true] {
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            if !late {
                chart.set_instrument(Some(instrument.clone()));
            }
            chart.add_indicator(Box::new(VolumeWeightedAvgPrice::new()));
            if late {
                chart.set_instrument(Some(instrument.clone()));
                shapes(&egui::Context::default(), &mut chart, 0.0, Vec::new());
            }
            let study = chart.indicators().next().unwrap().1;
            assert!(
                matches!(study.values().to_rows()[2], IndicatorValue::Single(v) if v == 14.0),
                "late={late}: {:?}",
                study.values().to_rows()
            );
            chart.set_instrument(None);
            shapes(&egui::Context::default(), &mut chart, 1.0, Vec::new());
            let study = chart.indicators().next().unwrap().1;
            assert!(
                matches!(study.values().to_rows()[2], IndicatorValue::Single(v) if v == 16.0),
                "clearing metadata must restore UTC: {:?}",
                study.values().to_rows()
            );
        }
        // These two intraday bars straddle UTC midnight but share a New York date.
        let sparse = bars[..2].to_vec();
        for interval_first in [false, true] {
            let mut chart = Chart::new(BarData::from_bars(sparse.clone()));
            chart.set_instrument(Some(instrument.clone()));
            if interval_first {
                chart.set_timeframe(crate::model::Timeframe::Hour1);
            }
            chart.add_indicator(
                crate::studies::IndicatorFactory::new()
                    .create("VWAP")
                    .unwrap(),
            );
            chart.set_timeframe(crate::model::Timeframe::Hour1);
            shapes(&egui::Context::default(), &mut chart, 0.0, Vec::new());
            assert_eq!(
                chart.indicators().next().unwrap().1.values().value(0, 1),
                Some(12.0)
            );
            chart.set_timeframe(crate::model::Timeframe::Day1);
            shapes(&egui::Context::default(), &mut chart, 1.0, Vec::new());
            assert_eq!(
                chart.indicators().next().unwrap().1.values().value(0, 1),
                Some(14.0)
            );
            chart.set_timeframe(crate::model::Timeframe::Hour1);
            chart.set_symbol("UTC");
            chart.set_instrument(Some(Instrument::new("UTC")));
            shapes(&egui::Context::default(), &mut chart, 2.0, Vec::new());
            assert_eq!(
                chart.indicators().next().unwrap().1.values().value(0, 1),
                Some(14.0)
            );
        }
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.add_indicator(Box::new(
            VolumeWeightedAvgPrice::new()
                .with_time_zone(TimezoneId::named("America/New_York").unwrap()),
        ));
        shapes(&egui::Context::default(), &mut chart, 0.0, Vec::new());
        assert!(matches!(
            chart.indicators().next().unwrap().1.values().to_rows()[2],
            IndicatorValue::Single(v) if v == 14.0
        ));
    }

    /// Daily bars from 2 March with typical prices 100, 102, 104, 106 and volumes 1000, 1000,
    /// 2000, 1000; earnings on the third bar's time and a dividend inside the first bar's day, so
    /// restarting on the second bar, the first at or after it.
    #[cfg(feature = "widget")]
    #[test]
    fn vwap_restarts_at_the_chart_events_of_its_anchor() {
        use crate::model::BarData;
        use crate::widget::{Chart, test_frames::shapes};
        let bars: Vec<_> = [
            (100.0, 1000.0),
            (102.0, 1000.0),
            (104.0, 2000.0),
            (106.0, 1000.0),
        ]
        .into_iter()
        .zip(2..)
        .map(|((price, volume), day)| {
            let time = Utc.with_ymd_and_hms(2026, 3, day, 0, 0, 0).unwrap();
            Bar::new(time, price, price, price, price, volume)
        })
        .collect();
        let event = |kind, time| TimeScaleEvent {
            id: format!("{kind:?}"),
            time,
            kind,
            title: String::new(),
            details: String::new(),
        };
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_time_scale_events(vec![
            event(
                EventKind::Earnings,
                Utc.with_ymd_and_hms(2026, 3, 4, 0, 0, 0).unwrap(),
            ),
            event(
                EventKind::Dividend,
                Utc.with_ymd_and_hms(2026, 3, 2, 12, 0, 0).unwrap(),
            ),
        ]);
        let id = chart.add_indicator(
            crate::studies::IndicatorFactory::new()
                .create("VWAP")
                .unwrap(),
        );
        let ctx = egui::Context::default();
        let values = |chart: &Chart| -> Vec<f64> {
            let study = chart.indicators().next().unwrap().1;
            (0..4)
                .map(|bar| study.values().value(0, bar).unwrap())
                .collect()
        };
        let assert_close = |got: Vec<f64>, want: [f64; 4]| {
            for (got, want) in got.iter().zip(want) {
                assert!((got - want).abs() < 1e-9, "{got} != {want}");
            }
        };
        let earnings = [100.0, 101.0, 104.0, 314000.0 / 3000.0];
        let dividends = [100.0, 102.0, 310000.0 / 3000.0, 104.0];
        let none = [100.0, 101.0, 102.5, 103.2];
        for (anchor, want, now) in [(9, dividends, 1.0), (10, none, 2.0), (8, earnings, 3.0)] {
            chart
                .indicator_mut(id)
                .unwrap()
                .set_input("anchor", InputValue::Choice(anchor))
                .unwrap();
            shapes(&ctx, &mut chart, now, Vec::new());
            assert_close(values(&chart), want);
        }
        // Earnings are the anchor from the last round; a change of events recalculates.
        chart.set_time_scale_events(Vec::new());
        shapes(&ctx, &mut chart, 4.0, Vec::new());
        assert_close(values(&chart), none);
    }

    /// The local time of a bar at either end of the representable range is out of range
    /// in a zone east or west of UTC; `calculate` must still not panic.
    #[test]
    fn extreme_bar_times_do_not_panic_in_a_zone() {
        for (zone, time) in [
            ("+11:00", DateTime::<Utc>::MAX_UTC),
            ("-06:00", DateTime::<Utc>::MIN_UTC),
        ] {
            let mut study =
                VolumeWeightedAvgPrice::new().with_time_zone(TimezoneId::named(zone).unwrap());
            study.calculate(&[Bar::new(time, 1.0, 1.0, 1.0, 1.0, 1.0)]);
            assert_eq!(study.values().len(), 1, "{zone}");
        }
    }

    /// Hourly bars at 13:00, 14:00 and 15:00 UTC with prices 10, 20, 40 and volumes 1, 1, 2.
    /// Events typed in New York, five hours behind UTC before 8 March, as 09:00 twice and 08:00
    /// are the instants 14:00 and 13:00 UTC once each: the average restarts at 14:00, so the last
    /// bar is (20 + 2 * 40) / 3, where an event read as UTC would leave (10 + 20 + 80) / 4.
    #[cfg(feature = "timezones")]
    #[test]
    fn event_anchors_are_edited_through_inputs_in_their_time_zone() {
        use crate::studies::InputValue::{Length, Text};
        let bars: Vec<_> = [(13, 10.0, 1.0), (14, 20.0, 1.0), (15, 40.0, 2.0)]
            .into_iter()
            .map(|(hour, price, volume)| {
                let time = Utc.with_ymd_and_hms(2026, 3, 2, hour, 0, 0).unwrap();
                Bar::new(time, price, price, price, price, volume)
            })
            .collect();
        let mut study = VolumeWeightedAvgPrice::new();
        study.set_input("anchor", InputValue::Choice(5)).unwrap();
        study
            .set_input("event_time_zone", Text("America/New_York".into()))
            .unwrap();
        study.set_input("event_count", Length(3)).unwrap();
        for (i, time) in [
            "2026-03-02 09:00:00",
            "2026-03-02 08:00",
            "2026-03-02 09:00:00",
        ]
        .into_iter()
        .enumerate()
        {
            study
                .set_input(&format!("event_{i}"), Text(time.into()))
                .unwrap();
        }
        assert!(study.set_input("event_0", Text("tomorrow".into())).is_err());
        study.calculate(&bars);
        let last = study.values().value(0, 2).unwrap();
        assert!((last - 100.0 / 3.0).abs() < 1e-12, "{last}");
    }

    /// Bands and events are inputs that a template restores, with one style per resulting line,
    /// and editing the number of bands through a chart's settings resizes the styles.
    #[test]
    fn bands_and_events_survive_a_template_and_resize_styles() {
        use crate::studies::{ConfiguredStudy, InputValue::Length, StudyTemplate};
        let event = Utc.with_ymd_and_hms(2026, 3, 2, 14, 30, 0).unwrap();
        let study = VolumeWeightedAvgPrice::new()
            .with_percentage_bands([0.5, 2.5])
            .with_event_anchors([event]);
        let study = ConfiguredStudy::new(Box::new(study));
        let restored = StudyTemplate::from_study(&study).create().unwrap();
        let inputs = |s: &dyn Indicator| {
            s.inputs()
                .into_iter()
                .map(|i| (i.id, i.value))
                .collect::<Vec<_>>()
        };
        assert_eq!(inputs(&study), inputs(&*restored));
        assert_eq!(restored.line_count(), 5);

        let mut edited = study.clone();
        let mut settings = edited.settings();
        settings.plots[0].width = 3.0;
        edited
            .apply_settings(&[("band_count".into(), Length(3))], settings)
            .unwrap();
        assert_eq!(edited.line_count(), 7);
        let plots = edited.settings().plots;
        assert_eq!(plots.len(), 7);
        assert_eq!(plots[0].width, 3.0);
        let bands: Vec<_> = edited
            .inputs()
            .into_iter()
            .filter(|i| i.id.starts_with("band_") && i.id != "band_count")
            .map(|i| i.value)
            .collect();
        assert_eq!(bands, [0.5, 2.5, 3.0].map(InputValue::Number));
    }
}
