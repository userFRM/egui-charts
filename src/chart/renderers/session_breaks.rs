//! Session-break rendering: day/session boundary dividers and optional
//! alternating session background shading.
//!
//! Intraday charts (1-minute, hourly, …) show a thin vertical divider wherever
//! the calendar day, or the configured trading session, changes between two
//! consecutive bars, following the common day-separator convention on intraday
//! charts. On daily-and-higher charts the same machinery draws the next coarser
//! boundary (week, month, then year) so the dividers stay meaningful rather than
//! landing on every bar.
//!
//! Boundaries are detected with [`find_session_breaks`] over the *visible* bar
//! slice and placed with the chart's canonical [`ChartMapping::idx_to_x`], so
//! they track the bars exactly under pan and zoom and never duplicate the
//! coordinate math used by the candles.

use super::context::{ChartMapping, RenderContext};
use crate::model::LineStyle;
use crate::model::{Bar, SessionBreakType, SessionProvider, Timeframe, find_session_breaks};
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

/// Shade complete bar slots only where the supplied session identifies extended hours.
pub(crate) fn render_extended_hours(
    ctx: &RenderContext,
    bars: &[Bar],
    provider: &crate::model::ExchangeSessionProvider,
    mapping: &ChartMapping,
    start_idx: usize,
    color: Color32,
) {
    let mut start = None;
    for i in 0..=bars.len() {
        let extended = bars
            .get(i)
            .is_some_and(|bar| provider.is_extended(bar.time));
        if extended && start.is_none() {
            start = Some(i);
        }
        if !extended && let Some(first) = start.take() {
            let x0 = mapping.idx_to_x(start_idx + first) - mapping.bar_spacing / 2.0;
            let x1 = mapping.idx_to_x(start_idx + i - 1) + mapping.bar_spacing / 2.0;
            let rect = egui::Rect::from_min_max(
                Pos2::new(x0, ctx.rect.top()),
                Pos2::new(x1, ctx.rect.bottom()),
            )
            .intersect(ctx.rect);
            if rect.is_positive() {
                ctx.painter.rect_filled(rect, 0.0, color);
            }
        }
    }
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
    /// Base color for the divider lines.
    pub line_color: Color32,
    /// Base line width in points.
    pub line_width: f32,
    /// Line style (solid / dashed / dotted).
    pub style: LineStyle,
}

impl Default for SessionBreakRenderConfig {
    fn default() -> Self {
        Self {
            line_color: Color32::from_gray(80),
            line_width: 1.0,
            style: LineStyle::Dashed,
        }
    }
}

/// Renderer for session-break divider lines.
///
/// Draws a thin vertical line at the first bar of each new session/day/week/
/// month/year within the visible range. The emphasis (width and opacity) scales with
/// the break type so monthly boundaries read stronger than daily ones, matching
/// the visual hierarchy on professional terminals.
pub struct SessionBreakRenderer {
    config: SessionBreakRenderConfig,
}

impl SessionBreakRenderer {
    /// Create a renderer with the given divider styling.
    pub fn new(config: SessionBreakRenderConfig) -> Self {
        Self { config }
    }

    /// Draw dividers for `visible_data`.
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
    ) {
        if visible_data.len() < 2 {
            return;
        }

        for (local_idx, session_break) in find_session_breaks(visible_data, provider) {
            let x = mapping.idx_to_x(start_idx + local_idx);
            if !mapping.is_x_visible(x) {
                continue;
            }

            let (color, width) = self.style_for(session_break.break_type);
            let from = Pos2::new(x, ctx.rect.min.y);
            let to = Pos2::new(x, ctx.rect.max.y);

            match dash_pattern(self.config.style) {
                None => {
                    ctx.painter
                        .line_segment([from, to], Stroke::new(width, color));
                }
                Some((dash, gap)) => {
                    draw_dashed_line(ctx.painter, from, to, Stroke::new(width, color), dash, gap);
                }
            }
        }
    }

    /// Color and width for a break type. Coarser boundaries read stronger.
    fn style_for(&self, break_type: SessionBreakType) -> (Color32, f32) {
        let base = self.config.line_color;
        match break_type {
            SessionBreakType::Daily => (base.gamma_multiply(0.6), self.config.line_width),
            SessionBreakType::Weekly => (base.gamma_multiply(0.8), self.config.line_width * 1.5),
            SessionBreakType::Monthly => (base, self.config.line_width * 2.0),
            SessionBreakType::Yearly => (base, self.config.line_width * 2.5),
            SessionBreakType::Custom => (base.gamma_multiply(0.7), self.config.line_width * 1.2),
        }
    }
}

/// Renderer for alternating session background shading.
///
/// Fills each session span (the region between two consecutive boundaries) with
/// one of two near-identical tints so adjacent trading days are subtly
/// distinguishable without competing with the candles drawn on top. Intended to
/// be drawn *behind* the candles, alongside the grid.
pub struct SessionBackgroundRenderer {
    /// The two alternating fill colors.
    colors: (Color32, Color32),
}

impl SessionBackgroundRenderer {
    /// Build a shading renderer from a base background color.
    ///
    /// The second tint is a faint darkening of the base so the alternation is
    /// perceptible but unobtrusive.
    pub fn from_background(background: Color32) -> Self {
        Self {
            colors: (Color32::TRANSPARENT, background.gamma_multiply(0.94)),
        }
    }

    /// Fill alternating session spans across the visible range.
    ///
    /// Spans are delimited by the same boundaries the divider renderer uses, so
    /// shading and dividers always agree. The leading partial span (before the
    /// first visible boundary) and the trailing partial span (after the last)
    /// are both filled so the alternation covers the whole chart rect.
    pub fn render(
        &self,
        ctx: &RenderContext,
        visible_data: &[Bar],
        provider: &dyn SessionProvider,
        mapping: &ChartMapping,
        start_idx: usize,
    ) {
        let breaks = find_session_breaks(visible_data, provider);

        let mut even = true;
        let mut prev_x = ctx.rect.min.x;

        for (local_idx, _) in breaks {
            let x = mapping
                .idx_to_x(start_idx + local_idx)
                .clamp(ctx.rect.min.x, ctx.rect.max.x);
            self.fill_span(ctx, prev_x, x, even);
            even = !even;
            prev_x = x;
        }

        // Trailing span up to the right edge.
        self.fill_span(ctx, prev_x, ctx.rect.max.x, even);
    }

    fn fill_span(&self, ctx: &RenderContext, x0: f32, x1: f32, even: bool) {
        if x1 <= x0 {
            return;
        }
        let color = if even { self.colors.0 } else { self.colors.1 };
        if color == Color32::TRANSPARENT {
            return;
        }
        let rect =
            egui::Rect::from_min_max(Pos2::new(x0, ctx.rect.min.y), Pos2::new(x1, ctx.rect.max.y));
        ctx.painter.rect_filled(rect, 0.0, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DailySessionProvider;
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
