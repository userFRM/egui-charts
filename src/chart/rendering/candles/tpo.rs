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
//! The live entry point is [`render_live_tpo`], which builds the profiles of the
//! whole sessions of the visible bars and anchors each session's letter histogram
//! at the session it summarises, staying correct under pan and zoom.

use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc};

use crate::localization as l10n;
use crate::scales::PriceMap;
use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

use crate::model::{
    Bar, BarClock, ExchangeSessionProvider, SessionProvider, SpanCache, TPOColorMode, TPOConfig,
    TPODisplayMode, TPOProfile, derive_tick_size, to_tpo_profiles,
};
use crate::theme::{ChartColors, ThemeExt, readable_color};
use crate::tokens::DESIGN_TOKENS;

/// Target number of price rows when auto-deriving the TPO bin (tick) size.
///
/// The bin height is chosen so the visible price range spans roughly this many
/// rows, keeping the profile readable across instruments that trade at very
/// different price magnitudes.
const TARGET_ROWS: usize = 40;

/// Fixed pixel width of one TPO period column.
///
/// Letters/blocks for successive periods stack horizontally to the right of
/// each session's anchor; this is the widest per-period horizontal step.
const PERIOD_COLUMN_PX: f32 = 9.0;

/// Below this column width letters become unreadable, so we switch to blocks,
/// following the common convention of blocks when zoomed out and letters when
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
    /// Highlight behind rows touched by a single period (with alpha)
    pub single_print_color: Color32,
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
            poc_line_width: 1.0,
            value_area_color: tpo.value_area,
            initial_balance_color: tpo.initial_balance,
            single_print_color: tpo.single_print,
            default_letter_color: tpo.letter_default,
            period_colors: Self::default_period_colors(),
        }
    }
}

impl TpoRenderConfig {
    /// Default color palette for period-based coloring
    fn default_period_colors() -> Vec<Color32> {
        [
            (
                Color32::from_rgb(214, 56, 100),
                Color32::from_rgb(90, 197, 97),
            ),
            (
                Color32::from_rgb(84, 185, 209),
                Color32::from_rgb(93, 36, 245),
            ),
        ]
        .into_iter()
        .flat_map(|(start, end)| (0..26).map(move |i| start.lerp_to_gamma(end, i as f32 / 25.0)))
        .collect()
    }

    /// Get color for a specific period index
    pub fn color_for_period(&self, period_idx: usize) -> Color32 {
        self.period_colors[period_idx % self.period_colors.len()]
    }
}

/// A session's profiles and the inputs they were built from.
struct CachedProfile {
    revision: u64,
    first: usize,
    len: usize,
    last: Bar,
    config: TPOConfig,
    clock: BarClock,
    profiles: Rc<Vec<TPOProfile>>,
}

thread_local! {
    static GRID_CACHE: RefCell<SpanCache<f64, u64>> = const { RefCell::new(SpanCache::new(8)) };
    static PROFILE_CACHE: RefCell<Vec<CachedProfile>> = const { RefCell::new(Vec::new()) };
}

/// Whether profiles built with `old` equal those built with `new`.
fn same_profiles(old: &TPOConfig, new: &TPOConfig) -> bool {
    old.tick_size.to_bits() == new.tick_size.to_bits()
        && old.period_minutes == new.period_minutes
        && old.initial_balance_minutes == new.initial_balance_minutes
        && old.value_area_pct.to_bits() == new.value_area_pct.to_bits()
        && old.custom_letters == new.custom_letters
        && old.split_sessions == new.split_sessions
        && old.composites == new.composites
}

/// The bars of the sessions that `visible` (or with `config.latest_session_only`, the last bar)
/// belongs to, widened over the composites they are part of, so profiles cover whole sessions.
fn whole_sessions(
    full: &[Bar],
    visible: Range<usize>,
    sessions: &ExchangeSessionProvider,
    config: &TPOConfig,
) -> Range<usize> {
    // Bars are chronological, so the bars before `i` in another session come first.
    let start_of = |i: usize| {
        full[..i].partition_point(|bar| {
            sessions
                .session_break_between(&bar.time, &full[i].time)
                .is_some()
        })
    };
    let end_of = |i: usize| {
        i + 1
            + full[i + 1..].partition_point(|bar| {
                sessions
                    .session_break_between(&full[i].time, &bar.time)
                    .is_none()
            })
    };
    let (mut start, mut end) = if config.latest_session_only {
        (start_of(full.len() - 1), full.len())
    } else {
        (start_of(visible.start), end_of(visible.end - 1))
    };
    // A split profile can open inside a day, so intersect the whole day's span, not just its open.
    loop {
        let before = start..end;
        for range in &config.composites {
            if range.is_empty()
                || *range.end() < full[start].time
                || *range.start() > full[end - 1].time
            {
                continue;
            }
            let first = full
                .partition_point(|bar| bar.time < *range.start())
                .min(full.len() - 1);
            let last = full
                .partition_point(|bar| bar.time <= *range.end())
                .saturating_sub(1);
            start = start.min(start_of(first));
            end = end.max(end_of(last));
        }
        if before == (start..end) {
            break;
        }
    }
    start..end
}

/// Profiles of the sessions in `bars`, each with the index of its first bar and its bar count,
/// reusing those whose inputs are unchanged. Sessions in one composite are built together.
fn cached_profiles(
    mut bars: &[Bar],
    revision: u64,
    mut start_idx: usize,
    config: &TPOConfig,
    clock: &BarClock,
    sessions: &ExchangeSessionProvider,
) -> Vec<(usize, usize, Rc<Vec<TPOProfile>>)> {
    // Sessions must share the visible series' grid, including when only one session changes.
    let tick = config.tick_size;
    let grid = GRID_CACHE.with_borrow_mut(|cache| {
        cache.get(tick.to_bits(), start_idx, bars, revision, |b| {
            crate::model::bar::level_grid(b, tick)
        })
    });
    let config = TPOConfig {
        tick_size: *grid,
        ..config.clone()
    };
    // Session ends, leaving sessions that open in one composite together.
    let composite = |bar: &Bar| {
        config
            .composites
            .iter()
            .position(|range| range.contains(&bar.time))
    };
    let mut ends = Vec::new();
    let mut open = 0;
    let starts = crate::model::tpo::session_starts(bars, sessions, config.split_sessions);
    let mut profile_open = starts.first().copied().unwrap_or(0);
    let breaks = starts.into_iter().skip(1);
    for end in breaks.chain([bars.len()]) {
        if bars.get(end).is_some_and(|next| {
            composite(&bars[profile_open]).is_some_and(|group| composite(next) == Some(group))
        }) {
            continue;
        }
        ends.push(end - open);
        open = end;
        profile_open = end;
    }
    PROFILE_CACHE.with_borrow_mut(|cache| {
        let mut positions: HashMap<_, Vec<usize>> = HashMap::with_capacity(cache.len());
        let mut entries: Vec<_> = cache
            .drain(..)
            .enumerate()
            .map(|(i, entry)| {
                positions
                    .entry((entry.first, entry.len))
                    .or_default()
                    .push(i);
                Some(entry)
            })
            .collect();
        let mut used = Vec::with_capacity(ends.len());
        let mut profiles = Vec::new();
        for len in ends {
            let (session, rest) = bars.split_at(len);
            let last = session.last().unwrap();
            let entry = positions
                .get(&(start_idx, session.len()))
                .into_iter()
                .flatten()
                .find_map(|&i| {
                    let entry = entries[i].as_ref()?;
                    let bits = |bar: &Bar| {
                        [bar.open, bar.high, bar.low, bar.close, bar.volume].map(f64::to_bits)
                    };
                    (entry.revision == revision
                        && entry.last.time == last.time
                        && bits(&entry.last) == bits(last)
                        && same_profiles(&entry.config, &config)
                        && entry.clock == *clock)
                        .then_some(i)
                })
                .and_then(|i| entries[i].take())
                .unwrap_or_else(|| CachedProfile {
                    revision,
                    first: start_idx,
                    len: session.len(),
                    last: last.clone(),
                    config: config.clone(),
                    clock: clock.clone(),
                    profiles: Rc::new(to_tpo_profiles(session, &config, clock)),
                });
            profiles.push((start_idx, session.len(), Rc::clone(&entry.profiles)));
            used.push(entry);
            start_idx += session.len();
            bars = rest;
        }
        cache.extend(entries.into_iter().flatten());
        cache.extend(used);
        // Keep every visible session, with room for another view and small multi-chart layouts.
        let limit = 8.max(profiles.len().saturating_mul(2));
        if cache.len() > limit {
            cache.drain(..cache.len() - limit);
        }
        profiles
    })
}

/// Render a real Market Profile for the sessions of the visible bars, anchored in chart time.
///
/// This is the live entry point of the TPO chart type and the TPO study. It builds the profiles
/// of the whole sessions that the bars `visible` of `full_data` belong to, in the sessions of
/// `clock`, reuses unchanged sessions, and anchors each profile at its session's first bar, or
/// at the plot's left edge while that bar is out of view, fitting it within the session's width.
/// Coordinates flow through the chart's shared price mapping ([`PriceMap`]) and the supplied
/// `idx_to_coord`, so the profile stays correct under pan and zoom.
///
/// The price-bin (tick) size auto-derives from the visible price range toward a
/// readable row count unless the caller pins it via `config.tick_size`. Letters
/// are drawn when the period column is wide enough to read; otherwise blocks are
/// drawn, matching the zoomed-out behaviour of professional terminals.
#[allow(clippy::too_many_arguments)]
pub fn render_live_tpo(
    painter: &Painter,
    price_rect: Rect,
    full_data: &[Bar],
    visible: Range<usize>,
    revision: u64,
    price_map: &PriceMap,
    base_config: &TPOConfig,
    clock: &BarClock,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    chart_rect_min_x: f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    if visible.is_empty() || visible.end > full_data.len() {
        return;
    }

    let Some(config) = row_config(base_config, price_map) else {
        return;
    };

    let sessions = clock.sessions();
    let bars = whole_sessions(full_data, visible, &sessions, &config);
    let start_idx = bars.start;
    let units = cached_profiles(
        &full_data[bars],
        revision,
        start_idx,
        &config,
        clock,
        &sessions,
    );
    let render_config = TpoRenderConfig {
        poc_color: painter.ctx().active_theme().ui.text,
        ..Default::default()
    };
    place_profiles(
        &units,
        full_data,
        price_rect,
        &config,
        &sessions,
        |i| idx_to_coord(i, chart_rect_min_x),
        |profile, anchor_x, column_width| {
            draw_session_profile(
                painter,
                price_rect,
                price_map,
                profile,
                &config,
                &render_config,
                anchor_x,
                column_width,
                bullish_color,
                bearish_color,
            );
        },
    );
}

fn row_config(base_config: &TPOConfig, price_map: &PriceMap) -> Option<TPOConfig> {
    let (range, transform) = (price_map.range(), price_map.transform());
    let price_range = (transform.to_price(range.max) - transform.to_price(range.min)).abs();
    if !price_range.is_finite() || price_range <= 0.0 {
        return None;
    }
    let mut config = base_config.clone();
    if config.tick_size <= 0.0 || !config.tick_size.is_finite() {
        config.tick_size = derive_tick_size(price_range, TARGET_ROWS);
    }
    Some(config)
}

/// Visit profiles at the same anchors and column widths for painting and hit testing.
fn place_profiles(
    units: &[(usize, usize, Rc<Vec<TPOProfile>>)],
    full_data: &[Bar],
    price_rect: Rect,
    config: &TPOConfig,
    sessions: &ExchangeSessionProvider,
    idx_to_coord: impl Fn(usize) -> f32,
    mut visit: impl FnMut(&TPOProfile, f32, f32),
) {
    // Each profile with its left edge, its session's width and whether it starts out of view.
    let mut placed = Vec::new();
    for (first, len, profiles) in units {
        let session = &full_data[*first..first + len];
        // Each profile, split off within the session or not, starts at its first bar.
        let anchors: Vec<_> = profiles
            .iter()
            .map(|profile| first + session.partition_point(|bar| bar.time < profile.session_date))
            .chain([first + len])
            .collect();
        for (profile, span) in profiles.iter().zip(anchors.windows(2)) {
            // A composite can start on an older day but still cover the latest one.
            if config.latest_session_only
                && profile.letters.last().is_some_and(|letter| {
                    sessions
                        .session_break_between(&letter.ts, &full_data.last().unwrap().time)
                        .is_some()
                })
            {
                continue;
            }
            // The forming session has the empty space after the last bar to grow into.
            let end_x = if span[1] == full_data.len() {
                f32::INFINITY
            } else {
                idx_to_coord(span[1])
            };
            // A session that opened left of the plot keeps its profile at the plot's edge.
            let open_x = idx_to_coord(span[0]);
            let anchor_x = open_x.max(price_rect.left());
            let widest = profile.price_tpo_count.values().max().copied().unwrap_or(0);
            if end_x > anchor_x && widest > 0 {
                let fit = (end_x - anchor_x) / widest as f32;
                placed.push((profile, anchor_x, fit, open_x < anchor_x));
            }
        }
    }
    // One column width for every profile that fits its whole session, so all show letters or
    // all show blocks; a profile cut by the plot's edge narrows further on its own.
    let column = placed
        .iter()
        .filter(|(.., cut)| !cut)
        .map(|&(_, _, fit, _)| fit)
        .fold(PERIOD_COLUMN_PX, f32::min);
    for (profile, anchor_x, fit, _) in placed {
        visit(profile, anchor_x, column.min(fit).max(1.0));
    }
}

/// The displayed profile at a menu click, including a profile anchored at the plot's edge.
#[cfg(feature = "ui")]
pub(crate) fn profile_at(
    full_data: &[Bar],
    visible: Range<usize>,
    revision: u64,
    base_config: &TPOConfig,
    clock: &BarClock,
    mapping: &crate::chart::coords::ChartMapping,
    pos: Pos2,
) -> Option<chrono::DateTime<chrono::Utc>> {
    if !mapping.rect.contains(pos) || visible.is_empty() || visible.end > full_data.len() {
        return None;
    }
    let config = row_config(base_config, &mapping.price)?;
    let sessions = clock.sessions();
    let bars = whole_sessions(full_data, visible, &sessions, &config);
    let units = cached_profiles(
        &full_data[bars.clone()],
        revision,
        bars.start,
        &config,
        clock,
        &sessions,
    );
    let mut hit = None;
    place_profiles(
        &units,
        full_data,
        mapping.rect,
        &config,
        &sessions,
        |i| mapping.idx_to_x(i),
        |profile, anchor, column| {
            let price = mapping.y_to_price(pos.y);
            let row = (price / profile.tick_size).round() * profile.tick_size;
            let count = if config.show_value_area && profile.is_in_value_area(row) {
                profile.price_tpo_count.values().max().copied().unwrap_or(0)
            } else {
                profile.tpo_count_at(row, profile.tick_size)
            };
            if pos.x >= anchor && pos.x < anchor + count as f32 * column {
                hit = Some(profile.session_date);
            }
        },
    );
    hit
}

/// Draw one session's profile anchored at pixel x `anchor_x`, growing rightward in columns of
/// `column_width` pixels.
#[allow(clippy::too_many_arguments)]
fn draw_session_profile(
    painter: &Painter,
    price_rect: Rect,
    coords: &PriceMap,
    profile: &TPOProfile,
    config: &TPOConfig,
    render_config: &TpoRenderConfig,
    anchor_x: f32,
    column_width: f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let Some(&widest) = profile.price_tpo_count.values().max() else {
        return;
    };

    let chart = painter.ctx().active_theme().chart;
    let tick_size = profile.tick_size;
    let use_letters = column_width >= LETTER_MIN_COLUMN_PX
        && matches!(
            config.display_mode,
            TPODisplayMode::Letters | TPODisplayMode::Both
        );

    // Row height in pixels for one tick, used to size blocks/letters so they
    // fill their price bin without overlapping the neighbour above/below.
    let row_height = {
        // Measured at the POC: a row is not the same height everywhere on a log scale.
        let poc = profile.poc_price;
        let h = (coords.price_to_y(poc) - coords.price_to_y(poc + tick_size)).abs();
        h.clamp(1.0, render_config.row_height * 1.5)
    };

    let levels = profile.price_levels(tick_size);
    let mut max_right = anchor_x;

    // Value Area shading first, so letters/blocks paint on top.
    if config.show_value_area {
        let va_top = coords.price_to_y(profile.value_area_high + tick_size * 0.5);
        let va_bottom = coords.price_to_y(profile.value_area_low - tick_size * 0.5);
        let va_right = anchor_x + widest as f32 * column_width;
        let va_rect = Rect::from_min_max(
            Pos2::new(anchor_x, va_top.min(va_bottom)),
            Pos2::new(va_right, va_top.max(va_bottom)),
        );
        painter.rect_filled(va_rect, 0.0, render_config.value_area_color);
    }

    // Single-print rows are highlighted behind their letter; hovering one names its price.
    if config.show_single_prints {
        let decimals = (-tick_size.log10()).ceil().clamp(0.0, 8.0) as usize;
        for &price in &profile.single_prints {
            let top = coords.price_to_y(price + tick_size * 0.5);
            let bottom = coords.price_to_y(price - tick_size * 0.5);
            let row = Rect::from_x_y_ranges(
                anchor_x..=anchor_x + column_width,
                top.min(bottom)..=top.max(bottom),
            )
            .intersect(price_rect);
            if row.height() <= 0.0 {
                continue;
            }
            painter.rect_filled(row, 0.0, render_config.single_print_color);
            let ctx = painter.ctx();
            if ctx
                .pointer_hover_pos()
                .is_some_and(|p| row.contains(p) && ctx.layer_id_at(p) == Some(painter.layer_id()))
            {
                egui::Tooltip::always_open(
                    ctx.clone(),
                    painter.layer_id(),
                    egui::Id::unique(("tpo-single-print", anchor_x.to_bits(), price.to_bits())),
                    row,
                )
                .show(|ui| {
                    ui.label(l10n::format_text(
                        ui.ctx(),
                        "Single print {0}",
                        &[("0", format!("{price:.decimals$}"))],
                    ));
                });
            }
        }
    }

    for price in levels {
        let y = coords.price_to_y(price);
        if y < price_rect.top() - row_height || y > price_rect.bottom() + row_height {
            continue;
        }

        // Letters at this level, one per period of each session, left-justified in time order.
        let mut letters_at_price: Vec<_> = profile.letters_at(price, tick_size);
        letters_at_price.sort_by_key(|l| l.ts);

        for (col, letter) in letters_at_price.iter().enumerate() {
            let x = anchor_x + col as f32 * column_width + column_width * 0.5;
            max_right = max_right.max(x + column_width * 0.5);

            let color = letter_color(
                render_config,
                config,
                letter.period_idx,
                price,
                profile,
                &chart,
            );

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
                        painter.ctx().active_theme().ui.text,
                    ),
                );
            }
        }
    }

    // Initial balance: a bar beside the profile from its low to its high.
    if config.show_initial_balance && config.initial_balance_minutes > 0 {
        let top = coords.price_to_y(profile.initial_balance_high);
        let bottom = coords.price_to_y(profile.initial_balance_low);
        painter.rect_filled(
            Rect::from_x_y_ranges(anchor_x - 2.0..=anchor_x, top.min(bottom)..=top.max(bottom)),
            0.0,
            render_config.initial_balance_color,
        );
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
    chart: &ChartColors,
) -> Color32 {
    let color = match config.color_mode {
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
    };
    readable_color(
        color,
        chart.axis_text,
        &[chart.bg, chart.bg.blend(render_config.value_area_color)],
        4.5,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, Timeframe, to_tpo_profiles};
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
                colors: None,
                payload: None,
            },
            Bar {
                time: start + Duration::minutes(30),
                open: 103.0,
                high: 107.0,
                low: 101.0,
                close: 106.0,
                volume: 1200.0,
                colors: None,
                payload: None,
            },
            Bar {
                time: start + Duration::minutes(60),
                open: 106.0,
                high: 108.0,
                low: 104.0,
                close: 105.0,
                volume: 800.0,
                colors: None,
                payload: None,
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
        let model = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let profile =
            to_tpo_profiles(&create_test_bars(), &model, &Timeframe::Min1.into()).remove(0);
        for (name, theme) in crate::theme::presets() {
            for mode in [
                TPOColorMode::ByPeriod,
                TPOColorMode::Solid,
                TPOColorMode::ByValueArea,
                TPOColorMode::ByInitialBalance,
            ] {
                let model = TPOConfig {
                    color_mode: mode,
                    ..model.clone()
                };
                for period in 0..num_colors {
                    for price in [98.0, 103.0, 108.0] {
                        let color =
                            letter_color(&config, &model, period, price, &profile, &theme.chart);
                        for bg in [
                            theme.chart.bg,
                            theme.chart.bg.blend(config.value_area_color),
                        ] {
                            assert!(
                                crate::theme::contrast(color, bg) >= 4.5,
                                "{name}: {mode:?}, period {period}, price {price}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_tpo_profile_generation() {
        let bars = create_test_bars();
        let config = TPOConfig {
            tick_size: 1.0,
            period_minutes: 30,
            ..Default::default()
        };

        let profiles = to_tpo_profiles(&bars, &config, &Timeframe::Min1.into());
        assert!(!profiles.is_empty());

        let profile = &profiles[0];
        assert!(profile.poc_price > 0.0);
        assert!(profile.value_area_high >= profile.value_area_low);
    }

    #[test]
    fn single_print_rows_are_highlighted_only_when_enabled() {
        let model = TPOConfig {
            tick_size: 1.0,
            period_minutes: 30,
            ..Default::default()
        };
        let profile =
            to_tpo_profiles(&create_test_bars(), &model, &Timeframe::Min1.into()).remove(0);
        assert!(!profile.single_prints.is_empty());
        let render_config = TpoRenderConfig::default();
        let rect = Rect::from_x_y_ranges(0.0..=400.0, 0.0..=400.0);
        let coords = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(90.0, 115.0),
            rect.min.y,
            rect.max.y,
            false,
        );
        let highlighted = |show_single_prints| {
            let ctx = egui::Context::default();
            ctx.begin_pass(egui::RawInput::default());
            let painter = ctx.layer_painter(egui::LayerId::background());
            let config = TPOConfig {
                show_single_prints,
                ..model.clone()
            };
            draw_session_profile(
                &painter,
                rect,
                &coords,
                &profile,
                &config,
                &render_config,
                10.0,
                20.0,
                Color32::GREEN,
                Color32::RED,
            );
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            output
                .shapes
                .iter()
                .filter(|s| {
                    matches!(&s.shape, Shape::Rect(r) if r.fill == render_config.single_print_color)
                })
                .count()
        };
        assert_eq!(highlighted(true), profile.single_prints.len());
        assert_eq!(highlighted(false), 0);
    }

    #[test]
    fn test_render_config_defaults() {
        let config = TpoRenderConfig::default();

        assert!(config.poc_line_width > 0.0);
        assert!(config.letter_font_size > 0.0);
        assert!(config.row_height > 0.0);
    }

    #[test]
    fn profiles_cover_the_whole_sessions_of_the_visible_bars() {
        // Three UTC days of four bars each: indices 0..4, 4..8 and 8..12
        let bars: Vec<_> = (0..12)
            .map(|i| {
                let minute = i / 4 * 1440 + i % 4 * 30;
                let time = chrono::DateTime::UNIX_EPOCH + Duration::minutes(minute);
                Bar::new(time, 100.0, 100.0, 100.0, 100.0, 1.0)
            })
            .collect();
        let sessions = BarClock::new(Timeframe::Min30).sessions();
        let day = |d: i64| chrono::DateTime::UNIX_EPOCH + Duration::days(d);
        // (visible bars, latest session only, composites, bars to build)
        let cases = [
            (5..7, false, vec![], 4..8),
            (3..5, false, vec![], 0..8),
            (11..12, false, vec![], 8..12),
            (1..2, true, vec![], 8..12),
            (5..6, false, vec![day(0)..=day(1)], 0..8),
            (1..2, false, vec![day(0)..=day(2)], 0..12),
            (1..2, true, vec![day(1)..=day(2)], 4..12),
            (
                1..2,
                false,
                vec![day(0) + Duration::minutes(90)..=day(1)],
                0..8,
            ),
            (
                5..6,
                false,
                vec![day(0) + Duration::minutes(90)..=day(1)],
                0..8,
            ),
            (5..6, false, vec![day(-1)..=day(3)], 0..12),
        ];
        for (visible, latest_session_only, composites, expected) in cases {
            let config = TPOConfig {
                latest_session_only,
                composites,
                ..Default::default()
            };
            assert_eq!(
                whole_sessions(&bars, visible.clone(), &sessions, &config),
                expected,
                "{visible:?}, {config:?}"
            );
        }
    }

    #[test]
    fn cached_profiles_draw_intraday_splits_and_cross_day_composites() {
        use crate::model::{Session, SessionWindow, TimezoneId};
        use chrono::{NaiveTime, TimeZone, Weekday::*};
        let at = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        let days = [Mon, Tue, Wed, Thu, Fri];
        let clock = BarClock::new(Timeframe::Min30).with_session(
            TimezoneId::fixed_offset(-5 * 3600).unwrap(),
            Session::new([SessionWindow::new(days, at(9, 30), at(16, 0))]).with_extended_windows([
                SessionWindow::new(days, at(4, 0), at(9, 30)),
                SessionWindow::new(days, at(16, 0), at(20, 0)),
            ]),
            true,
        );
        let start = Utc.with_ymd_and_hms(2024, 3, 5, 13, 0, 0).unwrap();
        let bars: Vec<_> = (0..38)
            .map(|i| {
                Bar::new(
                    start + Duration::minutes(i / 19 * 1440 + i % 19 * 30),
                    100.0,
                    100.0,
                    100.0,
                    100.0,
                    1.0,
                )
            })
            .collect();
        // Each day has 3 pre-market, 13 regular and 3 post-market half-hour periods.
        for (range, latest_session_only, expected) in [
            (Some((480, 1440)), false, vec![3, 13, 6, 13, 3]),
            (Some((90, 1530)), false, vec![3, 32, 3]),
            (None, true, vec![3, 13, 3]),
            (Some((480, 1440)), true, vec![6, 13, 3]),
            (Some((90, 1530)), true, vec![32, 3]),
        ] {
            let config = TPOConfig {
                split_sessions: true,
                latest_session_only,
                composites: range
                    .into_iter()
                    .map(|(first, last)| {
                        start + Duration::minutes(first)..=start + Duration::minutes(last)
                    })
                    .collect(),
                ..Default::default()
            };
            let span = whole_sessions(&bars, 24..25, &clock.sessions(), &config);
            assert_eq!(span, if range.is_none() { 19..38 } else { 0..38 });
            let cached = cached_profiles(
                &bars[span.clone()],
                1,
                span.start,
                &config,
                &clock,
                &clock.sessions(),
            );
            let mut counts = Vec::new();
            place_profiles(
                &cached,
                &bars,
                Rect::from_x_y_ranges(-1.0..=1000.0, 0.0..=1000.0),
                &config,
                &clock.sessions(),
                |i| i as f32 * 10.0,
                |profile, _, _| counts.push(profile.tpo_count_at(100.0, profile.tick_size)),
            );
            assert_eq!(counts, expected, "{range:?}, latest={latest_session_only}");
        }
    }

    #[test]
    fn unchanged_sessions_reuse_profiles_and_changed_inputs_rebuild() {
        let mut bars: Vec<_> = [0, 30, 1440]
            .into_iter()
            .map(|minute| Bar {
                time: chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(minute),
                open: 101.0,
                high: 102.0,
                low: 100.0,
                close: 101.0,
                volume: 1.0,
                colors: None,
                payload: None,
            })
            .collect();
        let mut config = TPOConfig::default();
        let mut revision = 1;
        let mut start_idx = 0;
        let mut clock = BarClock::new(Timeframe::Min1);
        let sessions = |clock: &BarClock| clock.sessions();
        let mut previous = cached_profiles(
            &bars,
            revision,
            start_idx,
            &config,
            &clock,
            &sessions(&clock),
        );
        // (input change, reused sessions, letters per session, first letter, row height)
        let cases = [
            ("steady", [true, true], [6, 3], 'A', 1.0),
            ("forming bar", [true, false], [6, 4], 'A', 1.0),
            ("revision", [false, false], [6, 4], 'A', 1.0),
            ("older correction", [false, false], [7, 4], 'A', 1.0),
            ("start index", [false, false], [7, 4], 'A', 1.0),
            ("append", [true, false], [7, 4], 'A', 1.0),
            ("last bits", [true, false], [7, 4], 'A', 1.0),
            ("last time", [true, false], [7, 8], 'A', 1.0),
            ("steady NaN", [true, true], [7, 8], 'A', 1.0),
            ("period", [false, false], [4, 4], 'A', 1.0),
            ("letters", [false, false], [4, 4], 'x', 1.0),
            ("grid", [false, false], [3, 3], 'x', 2.0),
            ("global span", [false, false], [2, 502], 'x', 4.0),
            ("zone", [false, false], [2, 502], 'x', 4.0),
        ];
        for (change, reused, counts, first_letter, tick) in cases {
            match change {
                "steady" | "steady NaN" => {}
                "forming bar" => bars[2].high = 103.0,
                "revision" => revision += 1,
                "older correction" => {
                    bars[0].high = 103.0;
                    revision += 1;
                }
                "start index" => start_idx += 1,
                "append" => bars.push(bars.last().unwrap().clone()),
                "last time" => bars.last_mut().unwrap().time += chrono::Duration::minutes(30),
                "last bits" => bars.last_mut().unwrap().volume = f64::NAN,
                "period" => config.period_minutes = 60,
                "letters" => config.custom_letters = Some("xy".into()),
                "grid" => config.tick_size = 2.0,
                "global span" => bars.last_mut().unwrap().high = 2103.0,
                "zone" => clock = clock.with_timezone("+09:00".parse().unwrap()),
                _ => unreachable!(),
            }
            let current = cached_profiles(
                &bars,
                revision,
                start_idx,
                &config,
                &clock,
                &sessions(&clock),
            );
            assert_eq!(current.len(), 2, "{change}");
            for (i, profile) in current.iter().enumerate() {
                assert_eq!(
                    Rc::ptr_eq(&profile.2, &previous[i].2),
                    reused[i],
                    "{change}, session {i}"
                );
                let profile = &profile.2[0];
                assert_eq!(profile.letters.len(), counts[i], "{change}, session {i}");
                assert_eq!(profile.letters[0].letter, first_letter, "{change}");
                assert_eq!(profile.tick_size, tick, "{change}");
            }
            previous = current;
        }

        let mut bars: Vec<_> = (0..16)
            .map(|day| {
                let price = 100.0 + (day % 5) as f64;
                Bar::new(
                    chrono::DateTime::UNIX_EPOCH + Duration::days(day),
                    price,
                    price,
                    price,
                    price,
                    1.0,
                )
            })
            .collect();
        let clock = BarClock::new(Timeframe::Min1);
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let mut seen = std::collections::HashMap::new();
        for (visible, append) in [
            (0..8, false),
            (4..12, false),
            (0..8, false),
            (8..16, false),
            (9..17, true),
        ] {
            if append {
                let mut bar = bars.last().unwrap().clone();
                bar.time += Duration::days(1);
                bars.push(bar);
            }
            let cached = cached_profiles(
                &bars[visible.clone()],
                99,
                visible.start,
                &config,
                &clock,
                &clock.sessions(),
            );
            let expected = to_tpo_profiles(&bars[visible.clone()], &config, &clock);
            assert_eq!(cached.len(), expected.len(), "{visible:?}");
            for ((first, len, profiles), expected) in cached.iter().zip(&expected) {
                assert_eq!(*len, 1);
                if let Some(previous) = seen.insert(*first, Rc::clone(profiles)) {
                    assert!(Rc::ptr_eq(&previous, profiles), "{visible:?}, {first}");
                }
                assert_eq!(profiles.len(), 1);
                assert_eq!(
                    format!("{:?}", profiles[0]),
                    format!("{expected:?}"),
                    "{visible:?}, {first}"
                );
            }
        }
    }
}
