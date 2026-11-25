//! Session-break rendering: day and session boundary dividers, and extended-hours shading.
//!
//! Intraday charts (1-minute, hourly, …) show a thin vertical divider wherever
//! the calendar day, or the configured trading session, changes between two
//! consecutive bars, following the common day-separator convention on intraday
//! charts. Daily and longer charts draw none, since each of their bars spans a
//! session or more.
//!
//! Boundaries are detected with [`find_session_breaks`] over the *visible* bar
//! slice and placed with the chart's canonical [`ChartMapping::idx_to_x`], so
//! they track the bars exactly under pan and zoom and never duplicate the
//! coordinate math used by the candles.

use super::{ChartMapping, RenderContext};
use crate::model::LineStyle;
use crate::model::{Bar, SessionProvider, Timeframe, find_session_breaks};
use crate::painting::dashed_line::draw_dashed_line;
use egui::{Color32, Pos2, Stroke};

/// Resolve explicit legacy hours, instrument metadata, then the displayed zone.
pub(crate) fn exchange_session(
    instrument: Option<&crate::model::Instrument>,
    config: Option<&crate::config::SessionConfig>,
    timezone: &crate::scales::TimezoneMode,
    timeframe: Timeframe,
) -> crate::model::ExchangeSessionProvider {
    use crate::model::{ExchangeSessionProvider, Session, SessionWindow, TimezoneId};
    use chrono::{NaiveTime, Weekday::*};
    let supplied_zone = instrument.and_then(|instrument| instrument.time_zone.clone());
    let mut local_zone = supplied_zone.is_none() && *timezone == crate::scales::TimezoneMode::Local;
    let mut zone = supplied_zone.unwrap_or_else(|| timezone.to_tz().unwrap_or_default());
    let mut session = instrument.and_then(|instrument| instrument.session.clone());
    if let Some(config) = config
        && let (Some(time_zone), Ok(open), Ok(close)) = (
            TimezoneId::named(&config.timezone),
            NaiveTime::parse_from_str(&config.session_start, "%H:%M"),
            NaiveTime::parse_from_str(&config.session_end, "%H:%M"),
        )
    {
        zone = time_zone;
        local_zone = false;
        let days = if config.session_start == "00:00" && config.session_end == "23:59" {
            vec![Mon, Tue, Wed, Thu, Fri, Sat, Sun]
        } else {
            vec![Mon, Tue, Wed, Thu, Fri]
        };
        let close = if days.len() == 7 {
            NaiveTime::MIN
        } else {
            close
        };
        let mut hours = Session::new([SessionWindow::new(days.clone(), open, close)]);
        if let Some(session) = &session {
            hours.holidays.clone_from(&session.holidays);
            hours.overrides.clone_from(&session.overrides);
        }
        if config.show_premarket
            && let Some(start) = config
                .premarket_start
                .as_ref()
                .and_then(|start| NaiveTime::parse_from_str(start, "%H:%M").ok())
        {
            hours
                .extended_windows
                .push(SessionWindow::new(days.clone(), start, open));
        }
        if config.show_postmarket
            && let Some(end) = config
                .postmarket_end
                .as_ref()
                .and_then(|end| NaiveTime::parse_from_str(end, "%H:%M").ok())
        {
            hours
                .extended_windows
                .push(SessionWindow::new(days, close, end));
        }
        session = Some(hours);
    }
    if local_zone {
        ExchangeSessionProvider::local(session)
    } else {
        ExchangeSessionProvider::new(zone, session)
    }
    .with_calendar_zone(match timezone {
        crate::scales::TimezoneMode::Local => None,
        _ => Some(timezone.to_tz().unwrap_or_default()),
    })
    .with_timeframe(timeframe)
}

/// Shades complete bar slots in the pre-market, post-market and overnight colours, returning
/// disjoint rectangles and their colours from left to right for the legend backdrops. Extended
/// bars that regular hours of the same trading date follow are the pre-market, other extended
/// bars the post-market; a phase without a colour stays unshaded.
pub(crate) fn render_extended_hours(
    ctx: &RenderContext,
    bars: &[Bar],
    provider: &crate::model::ExchangeSessionProvider,
    mapping: &ChartMapping,
    start_idx: usize,
    (premarket, postmarket, overnight): (Option<Color32>, Option<Color32>, Option<Color32>),
) -> Vec<(egui::Rect, Color32)> {
    use crate::model::session::{MarketSession, WindowKind};
    let mut shaded = Vec::new();
    let mut run: Option<(usize, Color32)> = None;
    for i in 0..=bars.len() {
        let color = bars
            .get(i)
            .and_then(|bar| match provider.window_kind(bar.time)? {
                WindowKind::Regular => None,
                WindowKind::Extended => match provider.market_session(bar.time) {
                    Some(MarketSession::PreMarket) => premarket,
                    _ => postmarket,
                },
                WindowKind::Overnight => overnight,
            });
        if let Some((first, shade)) = run.filter(|&(_, shade)| Some(shade) != color) {
            run = None;
            let x0 = mapping.idx_to_x(start_idx + first) - mapping.bar_spacing / 2.0;
            let x1 = mapping.idx_to_x(start_idx + i - 1) + mapping.bar_spacing / 2.0;
            let rect = egui::Rect::from_min_max(
                Pos2::new(x0, ctx.rect.top()),
                Pos2::new(x1, ctx.rect.bottom()),
            )
            .intersect(ctx.rect);
            if rect.is_positive() {
                ctx.painter.rect_filled(rect, 0.0, shade);
                shaded.push((rect, shade));
            }
        }
        if run.is_none()
            && let Some(color) = color
        {
            run = Some((i, color));
        }
    }
    shaded
}

/// Convert a [`LineStyle`] into the dash pattern egui needs.
///
/// `Solid` yields a single segment so the caller can take a plain
/// `line_segment` fast path; `Dashed`/`Dotted` return on/off lengths scaled so
/// the pattern reads at the typical divider width.
fn dash_pattern(style: LineStyle) -> Option<(f32, f32)> {
    match style {
        LineStyle::Solid => None,
        LineStyle::Dashed => Some((6.0, 4.0)),
        LineStyle::Dotted => Some((2.0, 3.0)),
        LineStyle::LargeDashed => Some((12.0, 8.0)),
        LineStyle::SparseDotted => Some((2.0, 8.0)),
    }
}

/// Configuration for session-break divider rendering.
#[derive(Debug, Clone)]
pub struct SessionBreakRenderConfig {
    /// Colour of the divider lines.
    pub line_color: Color32,
    /// Line width in points.
    pub line_width: f32,
    /// Line style (solid / dashed / dotted).
    pub style: LineStyle,
}

/// Renderer for session-break divider lines: a vertical line in the configured colour,
/// width and pattern at the first bar of each new session, day, week, month or year within the
/// visible range.
pub struct SessionBreakRenderer {
    config: SessionBreakRenderConfig,
}

impl SessionBreakRenderer {
    /// Create a renderer with the given divider styling.
    pub fn new(config: SessionBreakRenderConfig) -> Self {
        Self { config }
    }

    /// Draws the dividers of `visible_data` over the height of `ctx.rect` and returns their x
    /// positions from left to right, which the other panes of the chart repeat with
    /// [`paint`](Self::paint). A width that draws nothing returns none.
    ///
    /// `start_idx` is the global index of `visible_data[0]`; it is added to the
    /// local break index so the divider is placed at the bar's true x via the
    /// shared [`ChartMapping`]. Dividers outside the chart rect are skipped.
    pub fn render(
        &self,
        ctx: &RenderContext,
        visible_data: &[Bar],
        provider: &dyn SessionProvider,
        mapping: &ChartMapping,
        start_idx: usize,
    ) -> Vec<f32> {
        let width = self.config.line_width;
        if visible_data.len() < 2 || !(width.is_finite() && width > 0.0) {
            return Vec::new();
        }
        let xs: Vec<_> = find_session_breaks(visible_data, provider)
            .into_iter()
            .map(|(local_idx, _)| mapping.idx_to_x(start_idx + local_idx))
            .filter(|x| mapping.is_x_visible(*x))
            .collect();
        self.paint(ctx.painter, &xs, ctx.rect.y_range());
        xs
    }

    /// Draws a divider at each of `xs` over the height `y`.
    pub(crate) fn paint(&self, painter: &egui::Painter, xs: &[f32], y: egui::Rangef) {
        let stroke = Stroke::new(self.config.line_width, self.config.line_color);
        for &x in xs {
            let from = Pos2::new(x, y.min);
            let to = Pos2::new(x, y.max);
            match dash_pattern(self.config.style) {
                None => {
                    painter.line_segment([from, to], stroke);
                }
                Some((dash, gap)) => {
                    draw_dashed_line(painter, from, to, stroke, dash, gap);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DailySessionProvider, SessionBreakType};
    use chrono::{TimeZone, Utc};

    fn bar_at(year: i32, month: u32, day: u32, hour: u32) -> Bar {
        Bar {
            time: Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap(),
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.5,
            volume: 1000.0,
            colors: None,
            payload: None,
        }
    }

    #[test]
    fn day_breaks_mark_only_the_first_bar_of_a_new_day() {
        for (times, expected) in [
            (vec![(1, 9), (1, 12), (1, 15)], vec![]),
            (
                vec![(1, 9), (1, 12), (1, 15), (2, 9), (2, 12), (2, 15)],
                vec![(3, SessionBreakType::Daily)],
            ),
        ] {
            let bars: Vec<_> = times
                .iter()
                .map(|&(day, hour)| bar_at(2024, 1, day, hour))
                .collect();
            let breaks = find_session_breaks(&bars, &DailySessionProvider::continuous());
            let actual: Vec<_> = breaks
                .iter()
                .map(|(index, session)| (*index, session.break_type))
                .collect();
            assert_eq!(actual, expected, "{times:?}");
        }
    }
}
