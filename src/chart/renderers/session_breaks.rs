//! Session-break rendering: day/session boundary dividers and optional
//! alternating session background shading.
//!
//! Intraday charts (1-minute, hourly, …) show a thin vertical divider wherever
//! the calendar day — or the configured trading session — changes between two
//! consecutive bars, mirroring the day separators on TradingView intraday
//! charts. On daily-and-higher charts the same machinery draws the next coarser
//! boundary (week, then month) so the dividers stay meaningful rather than
//! landing on every bar.
//!
//! Boundaries are detected with [`find_session_breaks`] over the *visible* bar
//! slice and placed with the chart's canonical [`ChartMapping::idx_to_x`], so
//! they track the bars exactly under pan and zoom and never duplicate the
//! coordinate math used by the candles.

use super::context::{ChartMapping, RenderContext};
use crate::config::SessionBreakStyle;
use crate::model::{Bar, SessionBreakType, SessionProvider, Timeframe, find_session_breaks};
use egui::{Color32, Pos2, Stroke};

/// Pick the session provider whose boundaries are meaningful at `timeframe`.
///
/// Boundary granularity is chosen one tier coarser than the bar interval so a
/// divider marks a genuine break in the series rather than appearing between
/// every pair of bars:
///
/// | Timeframe tier            | Boundary drawn            |
/// |---------------------------|---------------------------|
/// | Intraday (`< 1 day`)      | Day change (midnight UTC) |
/// | Daily (`1D`)              | ISO-week change           |
/// | Weekly / monthly (`≥ 1W`) | Calendar-month change     |
///
/// Intraday charts use a midnight-UTC day boundary by default; callers that
/// know the exchange session can substitute a session-aware provider.
pub fn provider_for_timeframe(timeframe: Timeframe) -> Box<dyn SessionProvider> {
    use crate::model::{DailySessionProvider, MonthlySessionProvider, WeeklySessionProvider};

    // Day1 has duration 86_400_000 ms; anything below that is intraday.
    let day_ms = Timeframe::Day1.duration_ms();
    let ms = timeframe.duration_ms();

    if ms < day_ms {
        Box::new(DailySessionProvider::continuous())
    } else if ms == day_ms {
        Box::new(WeeklySessionProvider::default())
    } else {
        Box::new(MonthlySessionProvider)
    }
}

/// Convert a [`SessionBreakStyle`] into the dash pattern egui needs.
///
/// `Solid` yields a single segment so the caller can take a plain
/// `line_segment` fast path; `Dashed`/`Dotted` return on/off lengths scaled so
/// the pattern reads at the typical divider width.
fn dash_pattern(style: SessionBreakStyle) -> Option<(f32, f32)> {
    match style {
        SessionBreakStyle::Solid => None,
        SessionBreakStyle::Dashed => Some((6.0, 4.0)),
        SessionBreakStyle::Dotted => Some((2.0, 3.0)),
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
    pub style: SessionBreakStyle,
}

impl Default for SessionBreakRenderConfig {
    fn default() -> Self {
        Self {
            line_color: Color32::from_gray(80),
            line_width: 1.0,
            style: SessionBreakStyle::Dashed,
        }
    }
}

/// Renderer for session-break divider lines.
///
/// Draws a thin vertical line at the first bar of each new session/day/week/
/// month within the visible range. The emphasis (width and opacity) scales with
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
                    ctx.painter.add(egui::Shape::dashed_line(
                        &[from, to],
                        Stroke::new(width, color),
                        dash,
                        gap,
                    ));
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
        }
    }

    #[test]
    fn provider_tier_is_one_step_coarser_than_bars() {
        // Intraday -> day boundary.
        let p = provider_for_timeframe(Timeframe::Min1);
        assert_eq!(p.name(), "Daily Sessions");
        let p = provider_for_timeframe(Timeframe::Hour4);
        assert_eq!(p.name(), "Daily Sessions");

        // Daily -> week boundary.
        let p = provider_for_timeframe(Timeframe::Day1);
        assert_eq!(p.name(), "Weekly Sessions");

        // Weekly / monthly -> month boundary.
        let p = provider_for_timeframe(Timeframe::Week1);
        assert_eq!(p.name(), "Monthly Sessions");
        let p = provider_for_timeframe(Timeframe::Month1);
        assert_eq!(p.name(), "Monthly Sessions");
    }

    #[test]
    fn two_days_yield_one_break_at_first_bar_of_day_two() {
        // Three hourly bars on day one, three on day two: exactly one day
        // boundary, at the first bar of day two (local index 3).
        let bars = vec![
            bar_at(2024, 1, 1, 9),
            bar_at(2024, 1, 1, 12),
            bar_at(2024, 1, 1, 15),
            bar_at(2024, 1, 2, 9),
            bar_at(2024, 1, 2, 12),
            bar_at(2024, 1, 2, 15),
        ];
        let provider = DailySessionProvider::continuous();
        let breaks = find_session_breaks(&bars, &provider);

        assert_eq!(breaks.len(), 1, "exactly one day boundary");
        assert_eq!(breaks[0].0, 3, "break at first bar of day two");
        assert_eq!(breaks[0].1.break_type, SessionBreakType::Daily);
    }

    #[test]
    fn single_session_in_view_has_no_breaks() {
        let bars = vec![
            bar_at(2024, 1, 1, 9),
            bar_at(2024, 1, 1, 12),
            bar_at(2024, 1, 1, 15),
        ];
        let provider = DailySessionProvider::continuous();
        assert!(find_session_breaks(&bars, &provider).is_empty());
    }

    #[test]
    fn solid_style_has_no_dash_pattern() {
        assert!(dash_pattern(SessionBreakStyle::Solid).is_none());
        assert!(dash_pattern(SessionBreakStyle::Dashed).is_some());
        assert!(dash_pattern(SessionBreakStyle::Dotted).is_some());
    }
}
