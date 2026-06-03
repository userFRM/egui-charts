//! # TPO (Time Price Opportunity) Chart Renderer
//!
//! Full Market Profile rendering with support for:
//! - TPO letters (A, B, C...) at each price level
//! - Point of Control (POC) highlighting
//! - Value Area shading (70% of volume)
//! - Initial Balance range
//! - Single prints highlighting
//! - Session separators
//! - Multiple color modes and display modes
//!
//! The live entry point is [`render_live_tpo`], which builds the profiles from
//! the currently visible bars and anchors each session's letter histogram under
//! the session it summarises, staying correct under pan and zoom.

use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

use super::helpers::PriceCoords;
use crate::model::{
    Bar, TPOColorMode, TPOConfig, TPODisplayMode, TPOProfile, derive_tick_size, to_tpo_profiles,
};
use crate::tokens::DESIGN_TOKENS;

/// Target number of price rows when auto-deriving the TPO bin (tick) size.
///
/// The bin height is chosen so the visible price range spans roughly this many
/// rows, keeping the profile readable across instruments that trade at very
/// different price magnitudes.
const TARGET_ROWS: usize = 40;

/// Fixed pixel width of one TPO period column.
///
/// Letters/blocks for successive 30-minute periods stack horizontally to the
/// right of each session's anchor; this is the per-period horizontal step.
const PERIOD_COLUMN_PX: f32 = 9.0;

/// Below this column width letters become unreadable, so we switch to blocks —
/// mirroring how TradingView shows blocks when zoomed out and letters when
/// zoomed in.
const LETTER_MIN_COLUMN_PX: f32 = 7.0;

/// TPO rendering configuration (visual settings)
#[derive(Debug, Clone)]
pub struct TpoRenderConfig {
    /// Font size for TPO letters
    pub letter_font_size: f32,
    /// Height of each price row in pixels
    pub row_height: f32,
    /// POC line color
    pub poc_color: Color32,
    /// POC line width
    pub poc_line_width: f32,
    /// Value Area background color (with alpha)
    pub value_area_color: Color32,
    /// Initial Balance bracket color
    pub initial_balance_color: Color32,
    /// Default letter color
    pub default_letter_color: Color32,
    /// Color palette for period-based coloring
    pub period_colors: Vec<Color32>,
}

impl Default for TpoRenderConfig {
    fn default() -> Self {
        let tpo = &DESIGN_TOKENS.semantic.tpo;
        Self {
            letter_font_size: 10.0,
            row_height: 14.0,
            poc_color: tpo.poc,
            poc_line_width: 2.0,
            value_area_color: tpo.value_area,
            initial_balance_color: tpo.initial_balance,
            default_letter_color: tpo.letter_default,
            period_colors: Self::default_period_colors(),
        }
    }
}

impl TpoRenderConfig {
    /// Default color palette for period-based coloring
    fn default_period_colors() -> Vec<Color32> {
        let tpo = &DESIGN_TOKENS.semantic.tpo;
        vec![
            tpo.period_1,  // Red
            tpo.period_2,  // Orange
            tpo.period_3,  // Yellow
            tpo.period_4,  // Green
            tpo.period_5,  // Blue
            tpo.period_6,  // Deep Purple
            tpo.period_7,  // Pink
            tpo.period_8,  // Cyan
            tpo.period_9,  // Light Green
            tpo.period_10, // Brown
            tpo.period_11, // Blue Grey
            tpo.period_12, // Deep Orange
        ]
    }

    /// Get color for a specific period index
    pub fn color_for_period(&self, period_idx: usize) -> Color32 {
        self.period_colors[period_idx % self.period_colors.len()]
    }
}

/// Render a real Market Profile for the visible bars, anchored in chart time.
///
/// This is the live entry point used by `render_chart_type`. It builds the
/// profiles from the currently visible bars and anchors each session's letter
/// histogram at the x-coordinate of that session's first visible bar.
/// Coordinates flow through the chart's shared price mapping ([`PriceCoords`])
/// and the supplied `idx_to_coord`, so the profile stays correct under pan and
/// zoom.
///
/// The price-bin (tick) size auto-derives from the visible price range toward a
/// readable row count unless the caller pins it via `config.tick_size`. Letters
/// are drawn when the period column is wide enough to read; otherwise blocks are
/// drawn, matching the zoomed-out behaviour of professional terminals.
#[allow(clippy::too_many_arguments)]
pub fn render_live_tpo(
    painter: &Painter,
    price_rect: Rect,
    visible_data: &[Bar],
    start_idx: usize,
    min_price: f64,
    max_price: f64,
    base_config: &TPOConfig,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    chart_rect_min_x: f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    if visible_data.is_empty() {
        return;
    }

    let price_range = max_price - min_price;
    if !price_range.is_finite() || price_range <= 0.0 {
        return;
    }

    // Derive a readable bin size from the visible range unless one was pinned.
    let mut config = base_config.clone();
    if config.tick_size <= 0.0 || !config.tick_size.is_finite() {
        config.tick_size = derive_tick_size(price_range, TARGET_ROWS);
    }

    let profiles = to_tpo_profiles(visible_data, &config);
    if profiles.is_empty() {
        return;
    }

    let coords = PriceCoords::new(min_price, max_price, price_rect);
    let render_config = TpoRenderConfig::default();

    // Map each profile back to the index of its first visible bar so it anchors
    // under the session it summarises. Sessions split on calendar day, mirroring
    // `to_tpo_profiles`; a profile with no matching visible bar is skipped.
    let mut profile_iter = profiles.iter();
    let mut current = profile_iter.next();
    let mut anchor_idx: Option<usize> = None;
    let mut last_day = None;

    for (offset, bar) in visible_data.iter().enumerate() {
        let day = bar.time.date_naive();
        let new_session = last_day.map(|d| d != day).unwrap_or(true);
        last_day = Some(day);

        if new_session {
            // Flush the profile for the previous session before starting this one.
            if let (Some(profile), Some(idx)) = (current, anchor_idx) {
                draw_session_profile(
                    painter,
                    price_rect,
                    &coords,
                    profile,
                    &config,
                    &render_config,
                    idx_to_coord(idx, chart_rect_min_x),
                    bullish_color,
                    bearish_color,
                );
                current = profile_iter.next();
            }
            anchor_idx = Some(start_idx + offset);
        }
    }

    // Flush the final (or only) session.
    if let (Some(profile), Some(idx)) = (current, anchor_idx) {
        draw_session_profile(
            painter,
            price_rect,
            &coords,
            profile,
            &config,
            &render_config,
            idx_to_coord(idx, chart_rect_min_x),
            bullish_color,
            bearish_color,
        );
    }
}

/// Draw one session's profile anchored at pixel x `anchor_x`, growing rightward.
#[allow(clippy::too_many_arguments)]
fn draw_session_profile(
    painter: &Painter,
    price_rect: Rect,
    coords: &PriceCoords,
    profile: &TPOProfile,
    config: &TPOConfig,
    render_config: &TpoRenderConfig,
    anchor_x: f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    if profile.letters.is_empty() {
        return;
    }

    let tick_size = config.tick_size;
    let column_width = PERIOD_COLUMN_PX;
    let use_letters = column_width >= LETTER_MIN_COLUMN_PX
        && matches!(
            config.display_mode,
            TPODisplayMode::Letters | TPODisplayMode::Both
        );

    // Row height in pixels for one tick — used to size blocks/letters so they
    // fill their price bin without overlapping the neighbour above/below.
    let row_height = {
        let h = (coords.price_to_y(0.0) - coords.price_to_y(tick_size)).abs();
        h.clamp(1.0, render_config.row_height * 1.5)
    };

    let levels = profile.price_levels(tick_size);
    let mut max_right = anchor_x;

    // Value Area shading first, so letters/blocks paint on top.
    if config.show_value_area {
        let va_top = coords.price_to_y(profile.value_area_high + tick_size * 0.5);
        let va_bottom = coords.price_to_y(profile.value_area_low - tick_size * 0.5);
        // Width spans the widest row drawn below; approximate with profile width.
        let va_right = anchor_x + profile.width() as f32 * column_width;
        let va_rect = Rect::from_min_max(
            Pos2::new(anchor_x, va_top.min(va_bottom)),
            Pos2::new(va_right, va_top.max(va_bottom)),
        );
        painter.rect_filled(va_rect, 0.0, render_config.value_area_color);
    }

    for price in levels {
        let y = coords.price_to_y(price);
        if y < price_rect.top() - row_height || y > price_rect.bottom() + row_height {
            continue;
        }

        // Letters at this level, one per period, left-justified by period order.
        let mut letters_at_price: Vec<_> = profile.letters_at(price, tick_size);
        letters_at_price.sort_by_key(|l| l.period_idx);
        letters_at_price.dedup_by_key(|l| l.period_idx);

        for (col, letter) in letters_at_price.iter().enumerate() {
            let x = anchor_x + col as f32 * column_width + column_width * 0.5;
            max_right = max_right.max(x + column_width * 0.5);

            let color = letter_color(render_config, config, letter.period_idx, price, profile);

            if use_letters {
                painter.text(
                    Pos2::new(x, y),
                    egui::Align2::CENTER_CENTER,
                    letter.letter.to_string(),
                    FontId::monospace(render_config.letter_font_size),
                    color,
                );
            } else {
                let block = Rect::from_center_size(
                    Pos2::new(x, y),
                    egui::vec2(column_width * 0.9, row_height * 0.9),
                );
                painter.rect_filled(block, 0.0, color.gamma_multiply(0.85));
            }
        }
    }

    // POC line across the drawn width.
    if config.show_poc {
        let poc_y = coords.price_to_y(profile.poc_price);
        if poc_y >= price_rect.top() && poc_y <= price_rect.bottom() {
            painter.line_segment(
                [Pos2::new(anchor_x, poc_y), Pos2::new(max_right, poc_y)],
                Stroke::new(render_config.poc_line_width, render_config.poc_color),
            );
        }
    }

    // Value Area edges as thin guides at VAH / VAL.
    if config.show_value_area {
        for edge in [profile.value_area_high, profile.value_area_low] {
            let ey = coords.price_to_y(edge);
            if ey >= price_rect.top() && ey <= price_rect.bottom() {
                painter.line_segment(
                    [Pos2::new(anchor_x, ey), Pos2::new(max_right, ey)],
                    Stroke::new(
                        DESIGN_TOKENS.stroke.hairline,
                        render_config.value_area_color.gamma_multiply(2.0),
                    ),
                );
            }
        }
    }

    // Opening marker: a small triangle at the session open, tinted by direction.
    if config.show_opening_range {
        let open_y = coords.price_to_y(profile.opening_price);
        if open_y >= price_rect.top() && open_y <= price_rect.bottom() {
            let dir_color = if profile.poc_price >= profile.opening_price {
                bullish_color
            } else {
                bearish_color
            };
            let s = 5.0;
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(anchor_x - s, open_y),
                    Pos2::new(anchor_x, open_y - s * 0.5),
                    Pos2::new(anchor_x, open_y + s * 0.5),
                ],
                dir_color,
                Stroke::NONE,
            ));
        }
    }
}

/// Pick the color for one TPO entry, honouring the configured color mode.
fn letter_color(
    render_config: &TpoRenderConfig,
    config: &TPOConfig,
    period_idx: usize,
    price: f64,
    profile: &TPOProfile,
) -> Color32 {
    match config.color_mode {
        TPOColorMode::ByPeriod => render_config.color_for_period(period_idx),
        TPOColorMode::Solid => render_config.default_letter_color,
        TPOColorMode::ByValueArea => {
            if profile.is_in_value_area(price) {
                let va = render_config.value_area_color;
                Color32::from_rgb(va.r(), va.g(), va.b())
            } else {
                render_config.default_letter_color.gamma_multiply(0.5)
            }
        }
        TPOColorMode::ByInitialBalance => {
            if profile.is_in_initial_balance(price) {
                render_config.initial_balance_color
            } else {
                render_config.default_letter_color
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, to_tpo_profiles};
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        vec![
            Bar {
                time: start,
                open: 100.0,
                high: 105.0,
                low: 98.0,
                close: 103.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(30),
                open: 103.0,
                high: 107.0,
                low: 101.0,
                close: 106.0,
                volume: 1200.0,
            },
            Bar {
                time: start + Duration::minutes(60),
                open: 106.0,
                high: 108.0,
                low: 104.0,
                close: 105.0,
                volume: 800.0,
            },
        ]
    }

    #[test]
    fn test_period_color_cycling() {
        let config = TpoRenderConfig::default();
        let num_colors = config.period_colors.len();

        // Should cycle through colors
        let color_0 = config.color_for_period(0);
        let color_wrap = config.color_for_period(num_colors);

        assert_eq!(color_0, color_wrap);
    }

    #[test]
    fn test_tpo_profile_generation() {
        let bars = create_test_bars();
        let config = TPOConfig {
            tick_size: 1.0,
            period_minutes: 30,
            ..Default::default()
        };

        let profiles = to_tpo_profiles(&bars, &config);
        assert!(!profiles.is_empty());

        let profile = &profiles[0];
        assert!(profile.poc_price > 0.0);
        assert!(profile.value_area_high >= profile.value_area_low);
    }

    #[test]
    fn test_render_config_defaults() {
        let config = TpoRenderConfig::default();

        assert!(config.poc_line_width > 0.0);
        assert!(config.letter_font_size > 0.0);
        assert!(config.row_height > 0.0);
    }
}
