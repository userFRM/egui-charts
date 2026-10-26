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

use crate::model::session::local_time;
use crate::model::{Bar, BarClock, TimezoneId};
use crate::studies::{Indicator, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use chrono::{NaiveTime, Timelike};
use egui::Color32;

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
}

impl TradingSessions {
    /// Tokyo 09:00 to 15:00, London 08:30 to 16:30 and New York 09:30 to 16:00, each in
    /// its exchange time zone. Without `timezones`, these use fixed standard-time offsets
    /// of +09:00, +00:00 and -05:00. Override them with [`with_session`](Self::with_session).
    pub fn new() -> Self {
        let zone = |name, hours| {
            TimezoneId::named(name)
                .unwrap_or_else(|| TimezoneId::fixed_offset(hours * 3600).unwrap())
        };
        let tint =
            |color: Color32| Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 38);
        Self {
            names: ["Tokyo".into(), "London".into(), "New York".into()],
            show: [true; 3],
            start: [9 * 60, 8 * 60 + 30, 9 * 60 + 30],
            end: [15 * 60, 16 * 60 + 30, 16 * 60],
            zones: [
                zone("Asia/Tokyo", 9),
                zone("Europe/London", 0),
                zone("America/New_York", -5),
            ],
            daily: false,
            values: IndicatorValues::new(),
            colors: [
                tint(DESIGN_TOKENS.semantic.extended.info),
                tint(DESIGN_TOKENS.semantic.extended.deep_orange),
                tint(DESIGN_TOKENS.semantic.extended.success),
            ],
            visible: true,
        }
    }

    /// Replaces session `slot` (0, 1 or 2; others are ignored) with `name`, open from `start`
    /// to `end` in `zone`. Seconds are dropped.
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
            self.names[slot] = name.into();
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
}

impl Default for TradingSessions {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for TradingSessions {
    crate::studies::inputs::study_inputs!(self;
        show[0]: toggle("Show first session"),
        start[0]: length("First session start, minutes after midnight", 0, 1439),
        end[0]: length("First session end, minutes after midnight", 0, 1439),
        show[1]: toggle("Show second session"),
        start[1]: length("Second session start, minutes after midnight", 0, 1439),
        end[1]: length("Second session end, minutes after midnight", 0, 1439),
        show[2]: toggle("Show third session"),
        start[2]: length("Third session start, minutes after midnight", 0, 1439),
        end[2]: length("Third session end, minutes after midnight", 0, 1439)
    ; {});

    fn name(&self) -> &str {
        "Trading Sessions"
    }

    fn desc(&self) -> &str {
        "Trading Sessions - Background highlight of the Tokyo, London and New York sessions"
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
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
        self.values = IndicatorValues::missing(data.len());
        if !data.is_empty() {
            // Every bar holds a value for each of the three plots, even a hidden one.
            self.values.set_value(2, 0, f64::NAN);
        }
        if self.daily {
            return;
        }
        for slot in (0..3).filter(|&slot| self.show[slot]) {
            let (start, end) = (self.start[slot] as u32 * 60, self.end[slot] as u32 * 60);
            let zone = &self.zones[slot];
            for (i, bar) in data.iter().enumerate() {
                let second = local_time(zone.clone(), bar.time).num_seconds_from_midnight();
                let inside = match start.cmp(&end) {
                    std::cmp::Ordering::Less => (start..end).contains(&second),
                    std::cmp::Ordering::Greater => second >= start || second < end,
                    std::cmp::Ordering::Equal => true,
                };
                if inside {
                    self.values.set_value(slot, i, 1.0);
                }
            }
        }
    }

    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let daily = clock.timeframe().is_some_and(|timeframe| {
            timeframe.duration_ms() >= crate::model::Timeframe::Day1.duration_ms()
        });
        let changed = daily != self.daily;
        self.daily = daily;
        changed
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
