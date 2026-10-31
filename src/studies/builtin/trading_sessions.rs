//! Trading Sessions
//!
//! Highlights the bars that open within each of up to three trading sessions, by default the
//! regular hours of the Tokyo, London and New York stock exchanges, the Asian, European and
//! North American sessions, each in its own exchange time zone so daylight saving shifts the
//! highlight with the session.
//!
//! # Calculation
//! A bar belongs to a session when its open time, read in the session's time zone, lies in
//! `[start, end)`. A session whose end is at or before its start runs overnight, from the
//! start to the end on the next day; one whose end equals its start lasts the whole day.
//! Each session is a background plot, 1 on the bars in the session and without a value
//! elsewhere. Sessions apply on every day; markets without weekend bars have no weekend
//! highlight. Like any session highlight it needs intraday bars: on intervals of a day or
//! longer the study has no value.
//!
//! Each session can be drawn as a box from its high to its low, labelled with its name, range and
//! average price (the mean of its closes), and crossed by lines at its open, close and average
//! price. An overnight session is one session across midnight, and one lasting the whole day
//! ends at its start time. Bars with a price that is not finite do not count.
//!
//! # Example
//! ```
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, TradingSessions};
//!
//! // 00:00 UTC is 09:00 in Tokyo, in its session; 09:00 UTC is 09:00 in London in January.
//! let bars: Vec<Bar> = [0, 9]
//!     .into_iter()
//!     .map(|h| Bar::new(Utc.with_ymd_and_hms(2024, 1, 15, h, 0, 0).unwrap(), 1.0, 1.0, 1.0, 1.0, 1.0))
//!     .collect();
//! let mut sessions = TradingSessions::new();
//! sessions.calculate(&bars);
//! assert_eq!(sessions.values().value(0, 0), Some(1.0));
//! assert_eq!(sessions.values().value(1, 0), None);
//! assert_eq!(sessions.values().value(1, 1), Some(1.0));
//! ```

use crate::model::local_time;
use crate::model::{Bar, BarClock, TimezoneId};
use crate::studies::palette;
use crate::studies::stepper::update_window;
use crate::studies::{Indicator, IndicatorValues, LabelPlacement, StudyLabel, StudyZone};
use chrono::{NaiveDate, NaiveTime, TimeDelta, Timelike};
use egui::Color32;
use std::ops::Range;

/// Background highlight of up to three trading sessions on intraday charts.
///
/// The chart supplies its interval through [`Indicator::set_bar_clock`]; without it every
/// bar is treated as intraday.
#[derive(Clone)]
pub struct TradingSessions {
    names: [String; 3],
    show: [bool; 3],
    /// Session start, in minutes after midnight in the session's time zone.
    start: [usize; 3],
    /// Session end, in minutes after midnight in the session's time zone.
    end: [usize; 3],
    zones: [TimezoneId; 3],
    /// Whether the chart interval is a day or longer.
    daily: bool,
    values: IndicatorValues,
    colors: [Color32; 3],
    visible: bool,
    boxes: bool,
    name_labels: bool,
    range_labels: bool,
    average_labels: bool,
    open_lines: bool,
    close_lines: bool,
    average_lines: bool,
    /// The instrument's decimals and those of the study's presentation settings.
    precision: Option<usize>,
    label_precision: Option<usize>,
    /// Sorted by first bar, then slot, as `study_zones` and `labels` are.
    sessions: Vec<SessionSummary>,
    study_zones: Vec<StudyZone>,
    labels: Vec<StudyLabel>,
    source_len: usize,
}

/// One session of one slot on one day.
#[derive(Clone)]
struct SessionSummary {
    slot: usize,
    day: NaiveDate,
    bars: Range<usize>,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    close_sum: f64,
    closes: usize,
}

/// The exchange time zones of the default sessions, each with the standard-time offset in hours
/// that stands in for it without `timezones`.
const ZONES: [(&str, i32); 3] = [
    ("Asia/Tokyo", 9),
    ("Europe/London", 0),
    ("America/New_York", -5),
];

/// The time zone named `name` or, without `timezones`, the offset that stands in for the zone
/// of a default session, so that a template saved with `timezones` loads without it.
fn zone(name: &str) -> Option<TimezoneId> {
    TimezoneId::named(name).or_else(|| {
        let (_, hours) = ZONES.into_iter().find(|(zone, _)| *zone == name)?;
        TimezoneId::fixed_offset(hours * 3600)
    })
}

impl TradingSessions {
    /// Tokyo 09:00 to 15:00, London 08:30 to 16:30 and New York 09:30 to 16:00, each in
    /// its exchange time zone. Without `timezones`, these use fixed standard-time offsets
    /// of +09:00, +00:00 and -05:00. Override them with [`with_session`](Self::with_session).
    pub fn new() -> Self {
        let tint =
            |color: Color32| Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 38);
        Self {
            names: ["Tokyo".into(), "London".into(), "New York".into()],
            show: [true; 3],
            start: [9 * 60, 8 * 60 + 30, 9 * 60 + 30],
            end: [15 * 60, 16 * 60 + 30, 16 * 60],
            zones: ZONES.map(|(name, _)| zone(name).unwrap()),
            daily: false,
            values: IndicatorValues::new(),
            colors: [
                tint(palette::BLUE),
                tint(palette::WARM_ORANGE),
                tint(palette::TEAL),
            ],
            visible: true,
            boxes: true,
            name_labels: true,
            range_labels: false,
            average_labels: false,
            open_lines: false,
            close_lines: false,
            average_lines: false,
            precision: None,
            label_precision: None,
            sessions: Vec::new(),
            study_zones: Vec::new(),
            labels: Vec::new(),
            source_len: 0,
        }
    }

    /// Replaces session `slot` (0, 1 or 2; others are ignored) with `name`, open from `start`
    /// to `end` in `zone`. Seconds are dropped, and the name is kept on one line, without control
    /// characters or surrounding spaces, as the study inputs hold it.
    pub fn with_session(
        mut self,
        slot: usize,
        name: impl Into<String>,
        start: NaiveTime,
        end: NaiveTime,
        zone: TimezoneId,
    ) -> Self {
        if slot < 3 {
            let minutes = |time: NaiveTime| (time.hour() * 60 + time.minute()) as usize;
            let name: String = name.into();
            self.names[slot] = name.replace(char::is_control, " ").trim().to_owned();
            self.start[slot] = minutes(start);
            self.end[slot] = minutes(end);
            self.zones[slot] = zone;
        }
        self
    }

    /// Shows or hides session `slot`.
    pub fn with_shown(mut self, slot: usize, show: bool) -> Self {
        if let Some(shown) = self.show.get_mut(slot) {
            *shown = show;
        }
        self
    }

    /// Whether each session is drawn as a box from its high to its low over its bars. On by
    /// default.
    pub fn with_boxes(mut self, boxes: bool) -> Self {
        self.boxes = boxes;
        self
    }

    /// Which labels each session carries above its box: its name (on by default), its range
    /// (high minus low) and its average price, the mean of its closes.
    pub fn with_labels(mut self, names: bool, ranges: bool, averages: bool) -> Self {
        self.name_labels = names;
        self.range_labels = ranges;
        self.average_labels = averages;
        self
    }

    /// Which lines cross each session: at its open, its close and its average price. Off by
    /// default.
    pub fn with_lines(mut self, opens: bool, closes: bool, averages: bool) -> Self {
        self.open_lines = opens;
        self.close_lines = closes;
        self.average_lines = averages;
        self
    }
}

impl TradingSessions {
    /// Whether a time of day, in seconds after midnight, lies in session `slot`.
    fn inside(&self, slot: usize, second: u32) -> bool {
        let (start, end) = (self.start[slot] as u32 * 60, self.end[slot] as u32 * 60);
        match start.cmp(&end) {
            std::cmp::Ordering::Less => (start..end).contains(&second),
            std::cmp::Ordering::Greater => second >= start || second < end,
            std::cmp::Ordering::Equal => true,
        }
    }

    /// The sessions of `slot` in `data` from bar `first` on, each day's from its start time.
    fn scan(&self, slot: usize, data: &[Bar], first: usize, out: &mut Vec<SessionSummary>) {
        let mut current: Option<SessionSummary> = None;
        let start = TimeDelta::minutes(self.start[slot] as i64);
        for (at, bar) in data.iter().enumerate().skip(first) {
            if ![bar.open, bar.high, bar.low, bar.close]
                .iter()
                .all(|price| price.is_finite())
            {
                continue;
            }
            let local = local_time(self.zones[slot].clone(), bar.time);
            if !self.inside(slot, local.num_seconds_from_midnight()) {
                continue;
            }
            let day = local
                .checked_sub_signed(start)
                .map_or(local.date(), |time| time.date());
            match &mut current {
                Some(session) if session.day == day => {
                    session.bars.end = at + 1;
                    session.high = session.high.max(bar.high);
                    session.low = session.low.min(bar.low);
                    session.close = bar.close;
                    session.close_sum += bar.close;
                    session.closes += 1;
                }
                _ => {
                    out.extend(current.take());
                    current = Some(SessionSummary {
                        slot,
                        day,
                        bars: at..at + 1,
                        open: bar.open,
                        high: bar.high,
                        low: bar.low,
                        close: bar.close,
                        close_sum: bar.close,
                        closes: 1,
                    });
                }
            }
        }
        out.extend(current);
    }

    /// Writes the zones and label of `session`.
    fn emit(&mut self, session: &SessionSummary) {
        let average = session.close_sum / session.closes as f64;
        let plot = 3 + session.slot;
        let line = |price: f64, plot: usize| StudyZone {
            bars: session.bars.clone(),
            top: price,
            bottom: price,
            plot,
        };
        if self.boxes {
            self.study_zones.push(StudyZone {
                bars: session.bars.clone(),
                top: session.high,
                bottom: session.low,
                plot,
            });
        }
        if self.open_lines {
            self.study_zones.push(line(session.open, plot));
        }
        if self.close_lines {
            self.study_zones.push(line(session.close, plot));
        }
        if self.average_lines {
            self.study_zones.push(line(average, 6 + session.slot));
        }
        let precision = self.label_precision.or(self.precision);
        let price = |value: f64| crate::scales::StudyFormat::Price.format(value, precision);
        let mut lines = Vec::new();
        if self.name_labels && !self.names[session.slot].is_empty() {
            lines.push(self.names[session.slot].clone());
        }
        if self.range_labels {
            lines.push(format!("Range: {}", price(session.high - session.low)));
        }
        if self.average_labels {
            lines.push(format!("Avg: {}", price(average)));
        }
        if !lines.is_empty() {
            self.labels.push(StudyLabel {
                bar: session.bars.start,
                price: session.high,
                text: lines.join("\n"),
                placement: LabelPlacement::AboveRight,
                plot: session.slot,
            });
        }
    }

    /// Brings the sessions up to date. A session stays only if all its bars come before `from`
    /// and its slot has a later session, because the next bar can extend the last one even past
    /// a bar that does not count.
    fn update_sessions(&mut self, data: &[Bar], from: usize) {
        let from = from.min(self.source_len).min(data.len());
        self.source_len = data.len();
        if self.daily {
            self.sessions.clear();
            self.study_zones.clear();
            self.labels.clear();
            return;
        }
        let mut last = [usize::MAX; 3];
        for (at, session) in self.sessions.iter().enumerate() {
            last[session.slot] = at;
        }
        let keep = self
            .sessions
            .iter()
            .enumerate()
            .position(|(at, session)| session.bars.end >= from || last[session.slot] == at)
            .unwrap_or(self.sessions.len());
        let mut resume = [0; 3];
        for session in &self.sessions[..keep] {
            resume[session.slot] = session.bars.end;
        }
        let first = self.sessions.get(keep).map_or(from, |session| {
            let (bar, slot) = (session.bars.start, session.slot);
            let kept = self
                .labels
                .partition_point(|label| (label.bar, label.plot) < (bar, slot));
            self.labels.truncate(kept);
            bar
        });
        self.sessions.truncate(keep);
        let per_session = [
            self.boxes,
            self.open_lines,
            self.close_lines,
            self.average_lines,
        ]
        .iter()
        .filter(|shown| **shown)
        .count();
        self.study_zones.truncate(keep * per_session);
        let mut tail = Vec::new();
        for slot in (0..3).filter(|&slot| self.show[slot]) {
            self.scan(slot, data, first.max(resume[slot]), &mut tail);
        }
        tail.sort_by_key(|session| (session.bars.start, session.slot));
        for session in &tail {
            self.emit(session);
        }
        self.sessions.append(&mut tail);
    }

    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::missing(data.len());
        if !data.is_empty() {
            // Every bar holds a value for each of the three plots, even a hidden one.
            values.set_value(2, 0, f64::NAN);
        }
        if self.daily {
            return values;
        }
        for slot in (0..3).filter(|&slot| self.show[slot]) {
            let zone = &self.zones[slot];
            for (i, bar) in data.iter().enumerate() {
                let second = local_time(zone.clone(), bar.time).num_seconds_from_midnight();
                if self.inside(slot, second) {
                    values.set_value(slot, i, 1.0);
                }
            }
        }
        values
    }
}

impl Default for TradingSessions {
    fn default() -> Self {
        Self::new()
    }
}

/// The ordinal of each session in input labels, in lower and upper case.
const SLOTS: [(&str, &str); 3] = [("first", "First"), ("second", "Second"), ("third", "Third")];

impl Indicator for TradingSessions {
    /// Each session shows or not, and has a name, a start and an end in minutes after midnight,
    /// and a time zone that [`TimezoneId::named`] reads. The toggles after them apply to every
    /// session; its average price is the mean of its closes.
    fn inputs(&self) -> Vec<crate::studies::StudyInput> {
        use crate::studies::{InputKind, InputValue, StudyInput};
        let defaults = Self::default();
        let mut inputs = Vec::with_capacity(22);
        for (slot, (lower, upper)) in SLOTS.into_iter().enumerate() {
            let input = |field: &str, label: String, value, default, kind| {
                StudyInput::new(format!("{field} [{slot}]"), label, value, kind)
                    .with_default(default)
            };
            let minutes = InputKind::Length(0, 1439);
            inputs.extend([
                input(
                    "show",
                    format!("Show {lower} session"),
                    InputValue::Toggle(self.show[slot]),
                    InputValue::Toggle(defaults.show[slot]),
                    InputKind::Toggle,
                ),
                input(
                    "name",
                    format!("{upper} session name"),
                    InputValue::Text(self.names[slot].clone()),
                    InputValue::Text(defaults.names[slot].clone()),
                    InputKind::Text,
                ),
                input(
                    "start",
                    format!("{upper} session start, minutes after midnight"),
                    InputValue::Length(self.start[slot]),
                    InputValue::Length(defaults.start[slot]),
                    minutes.clone(),
                ),
                input(
                    "end",
                    format!("{upper} session end, minutes after midnight"),
                    InputValue::Length(self.end[slot]),
                    InputValue::Length(defaults.end[slot]),
                    minutes,
                ),
                input(
                    "zone",
                    format!("{upper} session time zone"),
                    InputValue::Text(self.zones[slot].name().to_owned()),
                    InputValue::Text(defaults.zones[slot].name().to_owned()),
                    InputKind::Text,
                ),
            ]);
        }
        for (id, label, value, default) in [
            ("boxes", "Show session boxes", self.boxes, defaults.boxes),
            (
                "name_labels",
                "Show session names",
                self.name_labels,
                defaults.name_labels,
            ),
            (
                "range_labels",
                "Show session ranges",
                self.range_labels,
                defaults.range_labels,
            ),
            (
                "average_labels",
                "Show session average prices",
                self.average_labels,
                defaults.average_labels,
            ),
            (
                "open_lines",
                "Show session open lines",
                self.open_lines,
                defaults.open_lines,
            ),
            (
                "close_lines",
                "Show session close lines",
                self.close_lines,
                defaults.close_lines,
            ),
            (
                "average_lines",
                "Show session average lines",
                self.average_lines,
                defaults.average_lines,
            ),
        ] {
            inputs.push(
                StudyInput::new(id, label, InputValue::Toggle(value), InputKind::Toggle)
                    .with_default(InputValue::Toggle(default)),
            );
        }
        inputs
    }

    fn set_input(
        &mut self,
        id: &str,
        value: crate::studies::InputValue,
    ) -> Result<(), &'static str> {
        use crate::studies::InputValue;
        let schema = self
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .ok_or("unknown study input")?;
        if !schema.accepts(&value) {
            return Err("study input is outside its accepted values");
        }
        let option = match id {
            "boxes" => Some(&mut self.boxes),
            "name_labels" => Some(&mut self.name_labels),
            "range_labels" => Some(&mut self.range_labels),
            "average_labels" => Some(&mut self.average_labels),
            "open_lines" => Some(&mut self.open_lines),
            "close_lines" => Some(&mut self.close_lines),
            "average_lines" => Some(&mut self.average_lines),
            _ => None,
        };
        if let (Some(option), InputValue::Toggle(on)) = (option, &value) {
            *option = *on;
            self.calculate(&[]);
            return Ok(());
        }
        // Every other id is `field [slot]`, as the schema lookup confirmed.
        let (field, slot) = id.split_once(" [").ok_or("unknown study input")?;
        let slot: usize = slot
            .trim_end_matches(']')
            .parse()
            .map_err(|_| "unknown study input")?;
        match (field, value) {
            ("show", InputValue::Toggle(show)) => self.show[slot] = show,
            ("name", InputValue::Text(name)) => self.names[slot] = name,
            ("start", InputValue::Length(minutes)) => self.start[slot] = minutes,
            ("end", InputValue::Length(minutes)) => self.end[slot] = minutes,
            ("zone", InputValue::Text(name)) => {
                self.zones[slot] = zone(&name).ok_or("unknown time zone")?;
            }
            _ => return Err("unknown study input"),
        }
        self.calculate(&[]);
        Ok(())
    }

    fn name(&self) -> &str {
        "Trading Sessions"
    }

    fn desc(&self) -> &str {
        "Trading Sessions - Background highlight of the Tokyo, London and New York sessions"
    }

    /// Boxes and session lines use plots 3 to 8, past the three outputs and in their colours at
    /// full opacity: the highlight's tint would leave a box edge at 15% and its fill near 2%.
    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        if (3..9).contains(&line) {
            return crate::studies::PlotStyle {
                color: self.colors[line % 3].to_opaque(),
                width: if line < 6 { 1.0 } else { 2.0 },
                pattern: crate::model::LineStyle::Solid,
                plot_type: crate::studies::PlotType::Line,
                visible: true,
            };
        }
        crate::studies::PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::BackgroundColor,
            visible: true,
        }
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Fixed(0)
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            |_, from| from,
            |data| self.compute(data),
        );
        self.values = values;
        self.update_sessions(data, from);
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let daily = clock.timeframe().is_some_and(|timeframe| {
            timeframe.duration_ms() >= crate::model::Timeframe::Day1.duration_ms()
        });
        let changed = daily != self.daily;
        self.daily = daily;
        changed
    }

    fn set_instrument(&mut self, instrument: Option<&crate::model::Instrument>) -> bool {
        let precision = instrument
            .and_then(crate::scales::instrument_price_precision)
            .map(usize::from);
        let changed = precision != self.precision;
        self.precision = precision;
        changed
    }

    fn zones(&self) -> &[StudyZone] {
        &self.study_zones
    }

    fn labels(&self) -> &[StudyLabel] {
        &self.labels
    }

    /// The study's own values are whole numbers, so its presentation settings hold 0 decimals by
    /// default, which says nothing about prices. Any other setting applies.
    fn set_label_precision(&mut self, precision: Option<usize>) {
        self.label_precision = precision.filter(|&decimals| decimals != 0);
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
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

    fn line_names(&self) -> Vec<String> {
        self.names.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    type Row = (u32, u32, f64, f64, f64, f64);
    type ZoneRow = (Range<usize>, f64, f64, usize);
    // (name, study, zones, text of the label)
    type Case<'a> = (&'a str, Box<dyn Indicator>, Vec<ZoneRow>, Option<&'a str>);

    /// The one session "Name" from `start` to `end` hours, UTC, over bars of (day, hour, open,
    /// high, low, close) of January 2024.
    fn study(name: &str, start: u32, end: u32) -> TradingSessions {
        let time = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
        TradingSessions::new()
            .with_session(0, name, time(start), time(end), TimezoneId::utc())
            .with_shown(1, false)
            .with_shown(2, false)
    }

    fn bars(rows: &[Row]) -> Vec<Bar> {
        rows.iter()
            .map(|&(day, hour, open, high, low, close)| {
                let time = Utc.with_ymd_and_hms(2024, 1, day, hour, 0, 0).unwrap();
                Bar::new(time, open, high, low, close, 1.0)
            })
            .collect()
    }

    fn zones(study: &dyn Indicator) -> Vec<ZoneRow> {
        let zones = study.zones().iter();
        zones
            .map(|zone| (zone.bars.clone(), zone.top, zone.bottom, zone.plot))
            .collect()
    }

    /// Session bars 1, 2 and 3, 12:00 being past its end: high 13, low 8.5, range 4.5, open 10,
    /// close 9 and average (11 + 12.5 + 9) / 3 = 10.8333.
    const DAY: &[Row] = &[
        (15, 8, 10.0, 11.0, 9.0, 10.0),
        (15, 9, 10.0, 12.0, 9.5, 11.0),
        (15, 10, 11.0, 13.0, 10.0, 12.5),
        (15, 11, 12.5, 12.8, 8.5, 9.0),
        (15, 12, 9.0, 10.0, 8.0, 9.5),
    ];

    #[test]
    fn a_session_draws_its_box_labels_and_lines_each_with_its_own_toggle() {
        let average = 32.5 / 3.0;
        let square = (1..4, 13.0, 8.5, 3);
        let open = (1..4, 10.0, 10.0, 3);
        let close = (1..4, 9.0, 9.0, 3);
        let mean = (1..4, average, average, 6);
        let all = || {
            study("Test", 9, 12)
                .with_labels(true, true, true)
                .with_lines(true, true, true)
        };
        let mut tick = all();
        let instrument = crate::model::Instrument::new("X").with_tick_size(0.001);
        tick.set_instrument(Some(&instrument));
        // The style tab's default of 0 decimals for the study's own values keeps the prices.
        let mut styled = crate::studies::ConfiguredStudy::new(Box::new(all()));
        styled.apply_settings(&[], styled.settings()).unwrap();
        let mut set = crate::studies::ConfiguredStudy::new(Box::new(all()));
        let mut settings = set.settings();
        settings.precision = Some(3);
        set.apply_settings(&[], settings).unwrap();
        let mut unboxed = study("Test", 9, 12);
        unboxed
            .set_input("boxes", crate::studies::InputValue::Toggle(false))
            .unwrap();
        let all_text = "Test\nRange: 4.50\nAvg: 10.83";
        let all_zones = vec![square.clone(), open.clone(), close.clone(), mean.clone()];
        let rows: Vec<Case> = vec![
            (
                "defaults",
                Box::new(study("Test", 9, 12)),
                vec![square.clone()],
                Some("Test"),
            ),
            ("all", Box::new(all()), all_zones.clone(), Some(all_text)),
            (
                "no box",
                Box::new(all().with_boxes(false)),
                vec![open.clone(), close.clone(), mean.clone()],
                Some(all_text),
            ),
            (
                "no box, through the input",
                Box::new(unboxed),
                Vec::new(),
                Some("Test"),
            ),
            (
                "no labels",
                Box::new(all().with_labels(false, false, false)),
                all_zones.clone(),
                None,
            ),
            (
                "range only",
                Box::new(all().with_labels(false, true, false)),
                all_zones.clone(),
                Some("Range: 4.50"),
            ),
            (
                "no lines",
                Box::new(all().with_lines(false, false, false)),
                vec![square.clone()],
                Some(all_text),
            ),
            (
                "open line only",
                Box::new(all().with_boxes(false).with_lines(true, false, false)),
                vec![open.clone()],
                Some(all_text),
            ),
            (
                "instrument decimals",
                Box::new(tick),
                all_zones.clone(),
                Some("Test\nRange: 4.500\nAvg: 10.833"),
            ),
            (
                "default style precision",
                Box::new(styled),
                all_zones.clone(),
                Some(all_text),
            ),
            (
                "style precision",
                Box::new(set),
                all_zones.clone(),
                Some("Test\nRange: 4.500\nAvg: 10.833"),
            ),
        ];
        let data = bars(DAY);
        for (name, mut study, want_zones, want_text) in rows {
            study.calculate(&data);
            assert_eq!(zones(study.as_ref()), want_zones, "{name}");
            let labels = study.labels();
            assert_eq!(labels.first().map(|l| l.text.as_str()), want_text, "{name}");
            if let Some(label) = labels.first() {
                assert_eq!(labels.len(), 1, "{name}");
                assert_eq!(
                    (label.bar, label.price, label.placement, label.plot),
                    (1, 13.0, LabelPlacement::AboveRight, 0),
                    "{name}"
                );
            }
        }

        let mut daily = all();
        daily.set_bar_clock(&BarClock::new(crate::model::Timeframe::Day1));
        daily.calculate(&data);
        assert!(daily.zones().is_empty() && daily.labels().is_empty());
    }

    /// Session bars 1 to 4 across midnight: high 13, low 9 and average 45.6 / 4 = 11.4. The two
    /// bars after 02:00 are outside it, and the next night's bar opens another session.
    #[test]
    fn an_overnight_session_is_one_session_across_midnight() {
        let data = bars(&[
            (15, 21, 1.0, 50.0, 1.0, 1.0),
            (15, 22, 10.0, 11.0, 9.0, 10.5),
            (15, 23, 10.5, 12.0, 10.0, 11.5),
            (16, 0, 11.5, 11.8, 10.2, 11.0),
            (16, 1, 11.0, 13.0, 10.8, 12.6),
            (16, 2, 1.0, 50.0, 1.0, 1.0),
            (16, 3, 1.0, 50.0, 1.0, 1.0),
            (16, 22, 20.0, 21.0, 19.0, 20.0),
        ]);
        let mut night = study("Night", 22, 2).with_labels(true, true, true);
        night.calculate(&data);
        assert_eq!(zones(&night), [(1..5, 13.0, 9.0, 3), (7..8, 21.0, 19.0, 3)]);
        assert_eq!(night.labels()[0].text, "Night\nRange: 4.00\nAvg: 11.40");
    }

    /// Hourly bars from 08:00 on the 15th to 10:00 on the 16th, sessions 08:00 to 16:00 and 14:00
    /// to 21:00. The latest second session, bars 6 to 12, overlaps the finished first session,
    /// bars 0 to 7, which a new bar must not start again from bar 6.
    #[test]
    fn a_new_bar_leaves_a_finished_session_that_overlaps_another() {
        let time = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
        let data: Vec<Bar> = (0..27)
            .map(|hour| {
                let time =
                    Utc.with_ymd_and_hms(2024, 1, 15, 8, 0, 0).unwrap() + TimeDelta::hours(hour);
                Bar::new(time, 1.5, 2.0, 1.0, 1.5, 1.0)
            })
            .collect();
        let mut sessions = study("First", 8, 16)
            .with_session(1, "Second", time(14), time(21), TimezoneId::utc())
            .with_shown(1, true);
        sessions.calculate(&data[..26]);
        sessions.update(&data, 26);
        assert_eq!(
            zones(&sessions),
            [
                (0..8, 2.0, 1.0, 3),
                (6..13, 2.0, 1.0, 4),
                (24..27, 2.0, 1.0, 3)
            ]
        );
    }

    /// A bar that does not count does not split a session, even when it is the newest when the
    /// next bar arrives.
    #[test]
    fn a_session_continues_past_a_bar_without_prices() {
        let mut data = bars(DAY);
        let next = data.pop().unwrap();
        let mut sessions = study("Test", 9, 13);
        data[3] = Bar::whitespace(data[3].time);
        sessions.calculate(&data);
        assert_eq!(zones(&sessions), [(1..3, 13.0, 9.5, 3)]);
        data.push(next);
        sessions.update(&data, 4);
        assert_eq!(zones(&sessions), [(1..5, 13.0, 8.0, 3)]);
    }
}
