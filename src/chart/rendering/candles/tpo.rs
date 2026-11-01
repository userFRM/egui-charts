//! # TPO (Time Price Opportunity) Chart Renderer
//!
//! Full Market Profile rendering with support for:
//! - TPO letters (A, B, C...) at each price level
//! - Point of Control (POC) highlighting
//! - Value Area guides and cell emphasis
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
use crate::painting::span_cache::SpanCache;
use crate::scales::PriceMap;
use egui::emath::GuiRounding as _;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

use crate::model::{
    Bar, BarClock, ExchangeSessionProvider, SessionProvider, TPOColorMode, TPOConfig,
    TPODisplayMode, TPOProfile, derive_tick_size, to_tpo_profiles,
};
use crate::styles::typography;
use crate::theme::{ChartColors, ThemeExt, contrast, readable_color};
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
const PERIOD_COLUMN_PX: f32 = 24.0;

/// Below this column width letters become unreadable, so we switch to blocks,
/// following the common convention of blocks when zoomed out and letters when
/// zoomed in.
const LETTER_MIN_COLUMN_PX: f32 = 7.0;

/// Width kept free before each session for the level labels of the profile after it.
const LABEL_ROOM_PX: f32 = 40.0;

/// TPO rendering configuration (visual settings)
#[derive(Debug, Clone)]
pub struct TpoRenderConfig {
    /// Font size for TPO letters
    pub letter_font_size: f32,
    /// Height of each price row in pixels
    pub row_height: f32,
    /// POC row and underline color
    pub poc_color: Color32,
    /// Width of the POC underline drawn with letters
    pub poc_line_width: f32,
    /// Value Area tone for value-area coloring (with alpha)
    pub value_area_color: Color32,
    /// Initial Balance bar color
    pub initial_balance_color: Color32,
    /// Highlight behind rows touched by a single period (with alpha)
    pub single_print_color: Color32,
    /// Default letter color
    pub default_letter_color: Color32,
    /// Gradient stops for period-based coloring
    pub period_colors: Vec<Color32>,
}

impl Default for TpoRenderConfig {
    fn default() -> Self {
        let tpo = &DESIGN_TOKENS.semantic.tpo;
        Self {
            letter_font_size: typography::XS,
            row_height: 16.0,
            poc_color: tpo.poc,
            poc_line_width: DESIGN_TOKENS.stroke.hairline,
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
        && old.instrument_tick.map(f64::to_bits) == new.instrument_tick.map(f64::to_bits)
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
/// Unless `base_config.tick_size` pins the row height, rows fit about 40 to the visible price
/// range, rounded up to a whole number of `instrument_tick` when one is given. Letters
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
    instrument_tick: Option<f64>,
    clock: &BarClock,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    chart_rect_min_x: f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    if visible.is_empty() || visible.end > full_data.len() {
        return;
    }

    let Some(config) = row_config(base_config, instrument_tick, price_map) else {
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
        |profile, anchor_x, column_width, labels| {
            draw_session_profile(
                painter,
                price_rect,
                price_map,
                profile,
                &config,
                &render_config,
                anchor_x,
                column_width,
                labels,
                bullish_color,
                bearish_color,
            );
        },
    );
}

/// `base_config` with a row height: its own, or about `TARGET_ROWS` rows over the visible
/// range, in whole multiples of `instrument_tick` when one is given, and records
/// `instrument_tick` so the profiles count whole ticks.
fn row_config(
    base_config: &TPOConfig,
    instrument_tick: Option<f64>,
    price_map: &PriceMap,
) -> Option<TPOConfig> {
    let (range, transform) = (price_map.range(), price_map.transform());
    let price_range = (transform.to_price(range.max) - transform.to_price(range.min)).abs();
    if !price_range.is_finite() || price_range <= 0.0 {
        return None;
    }
    let mut config = base_config.clone();
    config.instrument_tick = instrument_tick;
    if config.tick_size <= 0.0 || !config.tick_size.is_finite() {
        let step = derive_tick_size(price_range, TARGET_ROWS);
        config.tick_size = instrument_tick
            .filter(|tick| *tick > 0.0 && tick.is_finite())
            .map(|tick| crate::model::tpo::whole_ticks(step, tick) * tick)
            // A subnormal tick overflows the tick count
            .filter(|row| row.is_finite())
            .unwrap_or(step);
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
    mut visit: impl FnMut(&TPOProfile, f32, f32, bool),
) {
    // Each profile with its left edge, its session's width, its widest row and whether it starts
    // out of view.
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
                placed.push((
                    profile,
                    anchor_x,
                    end_x - anchor_x,
                    widest,
                    open_x < anchor_x,
                ));
            }
        }
    }
    // Level labels sit in the room left before each next session, so they show only while every
    // session in full view keeps at least as much width for its own profile.
    let labels = placed
        .iter()
        .filter(|(.., cut)| !cut)
        .all(|&(_, _, width, ..)| width >= 2.0 * LABEL_ROOM_PX);
    let room = if labels { LABEL_ROOM_PX } else { 0.0 };
    let fit = |width: f32, widest: usize| (width - room) / widest as f32;
    // One column width for every profile that fits its whole session, so all show letters or
    // all show blocks; a profile cut by the plot's edge narrows further on its own.
    let column = placed
        .iter()
        .filter(|(.., cut)| !cut)
        .map(|&(_, _, width, widest, _)| fit(width, widest))
        .fold(PERIOD_COLUMN_PX, f32::min);
    for (profile, anchor_x, width, widest, _) in placed {
        visit(
            profile,
            anchor_x,
            column.min(fit(width, widest)).max(1.0),
            labels,
        );
    }
}

/// The displayed profile at a menu click, including a profile anchored at the plot's edge.
#[cfg(feature = "ui")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn profile_at(
    full_data: &[Bar],
    visible: Range<usize>,
    revision: u64,
    base_config: &TPOConfig,
    instrument_tick: Option<f64>,
    clock: &BarClock,
    mapping: &crate::chart::coords::ChartMapping,
    pos: Pos2,
) -> Option<chrono::DateTime<chrono::Utc>> {
    if !mapping.rect.contains(pos) || visible.is_empty() || visible.end > full_data.len() {
        return None;
    }
    let config = row_config(base_config, instrument_tick, &mapping.price)?;
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
        |profile, anchor, column, _| {
            let column = column.min(profile_row_height(profile, &mapping.price));
            let price = mapping.y_to_price(pos.y);
            let row = profile.row_price(price);
            let count = if config.show_value_area && profile.is_in_value_area(row) {
                profile.price_tpo_count.values().max().copied().unwrap_or(0)
            } else {
                profile.tpo_count_at(row)
            };
            if pos.x >= anchor && pos.x < anchor + count as f32 * column {
                hit = Some(profile.session_date);
            }
        },
    );
    hit
}

/// Draw one session's profile anchored at pixel x `anchor_x`, growing rightward in columns of
/// `column_width` pixels, with its level labels to the left when `labels` is set.
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
    labels: bool,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let Some(&widest) = profile.price_tpo_count.values().max() else {
        return;
    };

    let chart = painter.ctx().active_theme().chart;
    let tick_size = profile.tick_size;
    let measured_height = profile_row_height(profile, coords);
    // A row of several ticks spans from half a tick below its price, so its centre is not its price.
    let row_y = |price: f64| {
        let (low, high) = profile.row_span(price);
        coords.price_to_y((low + high) * 0.5)
    };
    let readable = column_width.min(measured_height) >= LETTER_MIN_COLUMN_PX;
    // `Both` paints the cells of `Blocks` and adds the letters while they stay readable.
    let use_letters = readable && config.display_mode == TPODisplayMode::Letters;
    let letters_in_cells = readable && config.display_mode == TPODisplayMode::Both;

    let row_height = measured_height.min(render_config.row_height * 1.5);
    let column_width = column_width.min(row_height);
    let pixels_per_point = painter.ctx().pixels_per_point();
    let gap = 1.0 / pixels_per_point;
    let levels = profile.price_levels();
    let max_right = anchor_x + widest as f32 * column_width;
    let block_width = (column_width - gap).max(gap);
    let block_height = (row_height - gap).max(gap);
    // About 9 px of cap height in a 20 px cell, shrinking with the cell to the bare letters' size.
    let cell_font_size =
        (column_width * 0.65).clamp(render_config.letter_font_size, typography::MD);
    let poc_color = readable_color(
        render_config.poc_color,
        chart.axis_text,
        &[chart.bg],
        if use_letters { 4.5 } else { 3.0 },
    );

    // Single-print rows are highlighted behind their letter; hovering one names its price.
    if config.show_single_prints {
        let decimals = (-tick_size.log10()).ceil().clamp(0.0, 8.0) as usize;
        for &price in &profile.single_prints {
            let (low, high) = profile.row_span(price);
            let top = coords.price_to_y(high);
            let bottom = coords.price_to_y(low);
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

    // The tone of an entry follows its period, or its row for the other color modes, so it is
    // resolved once per period (and once per row) rather than for every cell.
    let by_period = matches!(config.color_mode, TPOColorMode::ByPeriod);
    let dims_outside_value_area = !use_letters && config.show_value_area;
    let tone = |period_idx: usize, price: f64| {
        if use_letters {
            letter_color(render_config, config, period_idx, price, profile, &chart)
        } else {
            block_color(render_config, config, period_idx, price, profile, &chart)
        }
    };
    let dim = |color: Color32| chart.bg.lerp_to_gamma(color, 0.45);
    let tones: Vec<Color32> = if by_period {
        (0..profile.width())
            .map(|period| tone(period, 0.0))
            .collect()
    } else {
        Vec::new()
    };
    let dimmed_tones: Vec<Color32> = if dims_outside_value_area {
        tones.iter().map(|&color| dim(color)).collect()
    } else {
        Vec::new()
    };

    for price in levels {
        let y = row_y(price);
        if y < price_rect.top() - row_height || y > price_rect.bottom() + row_height {
            continue;
        }
        let is_poc = config.show_poc && (price - profile.poc_price).abs() < tick_size * 0.5;
        let dimmed = !is_poc && dims_outside_value_area && !profile.is_in_value_area(price);
        let row_tone = if is_poc {
            Some(poc_color)
        } else if by_period {
            None
        } else {
            let color = tone(0, price);
            Some(if dimmed { dim(color) } else { color })
        };
        let period_tones = if dimmed { &dimmed_tones } else { &tones };
        // Rounded once for the row; only the horizontal edges differ between its cells.
        let top = (y - block_height * 0.5).round_to_pixels(pixels_per_point);
        let bottom = (y + block_height * 0.5).round_to_pixels(pixels_per_point);

        // Letters at this level, one per period of each session, left-justified in time order.
        let mut letters_at_price: Vec<_> = profile.letters_at(price);
        letters_at_price.sort_by_key(|l| l.ts);

        for (col, letter) in letters_at_price.iter().enumerate() {
            let x = anchor_x + col as f32 * column_width + column_width * 0.5;

            let color = row_tone.unwrap_or_else(|| period_tones[letter.period_idx]);

            if use_letters {
                painter.text(
                    Pos2::new(x, y),
                    egui::Align2::CENTER_CENTER,
                    letter.letter.to_string(),
                    FontId::monospace(render_config.letter_font_size),
                    color,
                );
            } else {
                let left = (x - block_width * 0.5).round_to_pixels(pixels_per_point);
                let right = (x + block_width * 0.5).round_to_pixels(pixels_per_point);
                let cell = Rect::from_min_max(Pos2::new(left, top), Pos2::new(right, bottom));
                painter.rect_filled(cell, 0.0, color);
                if letters_in_cells {
                    let letter_tone = cell_letter_color(color, &chart);
                    let mut format =
                        TextFormat::simple(FontId::proportional(cell_font_size), letter_tone);
                    format.coords.push("wght", 700.0);
                    let galley = painter
                        .layout_job(LayoutJob::single_section(letter.letter.to_string(), format));
                    let origin = egui::Align2::CENTER_CENTER
                        .anchor_size(cell.center(), galley.size())
                        .min;
                    painter.galley(origin, galley, letter_tone);
                }
            }
        }
    }

    // Letters retain a fine POC underline; blocks carry the POC in the row itself.
    if config.show_poc && use_letters {
        let y = row_y(profile.poc_price) + row_height * 0.5;
        painter.line_segment(
            [Pos2::new(anchor_x, y), Pos2::new(max_right, y)],
            Stroke::new(render_config.poc_line_width, poc_color),
        );
    }

    let guide_color = chart.axis_text_secondary;
    if config.show_value_area {
        let stroke = Stroke::new(
            DESIGN_TOKENS.stroke.light,
            painter.ctx().active_theme().ui.text,
        );
        for edge in [profile.value_area_high, profile.value_area_low] {
            let y = row_y(edge);
            if price_rect.y_range().contains(y) {
                // Dashes of 2 px every 5 px, as single segments rather than a path each.
                let end = max_right + 6.0;
                let mut x = anchor_x;
                while x < end {
                    painter
                        .line_segment([Pos2::new(x, y), Pos2::new((x + 2.0).min(end), y)], stroke);
                    x += 5.0;
                }
            }
        }
    }

    if labels {
        let mut label_rows = Vec::new();
        for (show, y, text, color) in [
            (
                config.show_poc,
                row_y(profile.poc_price),
                "POC",
                chart.axis_text,
            ),
            (
                config.show_value_area,
                row_y(profile.value_area_high),
                "VAH",
                guide_color,
            ),
            (
                config.show_value_area,
                row_y(profile.value_area_low),
                "VAL",
                guide_color,
            ),
            (
                true,
                coords.price_to_y((profile.profile_high + profile.profile_low) * 0.5),
                "Mid",
                readable_color(bearish_color, chart.axis_text, &[chart.bg], 4.5),
            ),
        ] {
            if !labels
                || !show
                || !price_rect.y_range().contains(y)
                || anchor_x - price_rect.left() < 32.0
            {
                continue;
            }
            // Displace only the caption when levels coincide; the marker keeps its price.
            let Some(label_y) = [0.0, -12.0, 12.0, -24.0, 24.0, -36.0, 36.0]
                .into_iter()
                .map(|offset| y + offset)
                .find(|candidate| {
                    *candidate >= price_rect.top() + typography::XS
                        && *candidate <= price_rect.bottom() - typography::XS
                        && label_rows.iter().all(|previous: &f32| {
                            (candidate - previous).abs() >= typography::XS + 1.0
                        })
                })
            else {
                continue;
            };
            label_rows.push(label_y);
            if label_y != y {
                painter.line_segment(
                    [
                        Pos2::new(anchor_x - 8.0, label_y),
                        Pos2::new(anchor_x - 4.0, y),
                    ],
                    Stroke::new(DESIGN_TOKENS.stroke.extra_thin, color),
                );
            }
            painter.text(
                Pos2::new(anchor_x - 8.0, label_y),
                egui::Align2::RIGHT_CENTER,
                text,
                FontId::proportional(typography::XS),
                color,
            );
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(anchor_x - 6.0, y - 2.5),
                    Pos2::new(anchor_x - 3.0, y),
                    Pos2::new(anchor_x - 6.0, y + 2.5),
                ],
                color,
                Stroke::NONE,
            ));
        }
    }

    // Initial balance: a bar beside the profile from its low to its high.
    if config.show_initial_balance && profile.initial_balance_high >= profile.initial_balance_low {
        let top = coords.price_to_y(profile.initial_balance_high);
        let bottom = coords.price_to_y(profile.initial_balance_low);
        painter.rect_filled(
            Rect::from_x_y_ranges(anchor_x - 2.0..=anchor_x, top.min(bottom)..=top.max(bottom)),
            0.0,
            readable_color(
                render_config.initial_balance_color,
                chart.axis_text_secondary,
                &[chart.bg],
                3.0,
            ),
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
            let s = 3.0;
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

/// Measure at the POC so logarithmic and inverted scales keep the same geometry.
fn profile_row_height(profile: &TPOProfile, coords: &PriceMap) -> f32 {
    let (low, high) = profile.row_span(profile.poc_price);
    (coords.price_to_y(low) - coords.price_to_y(high))
        .abs()
        .max(1.0)
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
    readable_color(
        entry_color(render_config, config, period_idx, price, profile),
        chart.axis_text,
        &[chart.bg, chart.bg.blend(render_config.value_area_color)],
        4.5,
    )
}

/// Pick the fill of one TPO cell, which reads on the chart at 3:1 as other graphics do.
fn block_color(
    render_config: &TpoRenderConfig,
    config: &TPOConfig,
    period_idx: usize,
    price: f64,
    profile: &TPOProfile,
    chart: &ChartColors,
) -> Color32 {
    readable_color(
        entry_color(render_config, config, period_idx, price, profile),
        chart.axis_text,
        &[chart.bg],
        3.0,
    )
}

/// White where it reaches 4.5:1 on the `cell`, else black, which always does there.
fn cell_letter_color(cell: Color32, chart: &ChartColors) -> Color32 {
    if contrast(Color32::WHITE, chart.bg.blend(cell)) >= 4.5 {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

fn entry_color(
    render_config: &TpoRenderConfig,
    config: &TPOConfig,
    period_idx: usize,
    price: f64,
    profile: &TPOProfile,
) -> Color32 {
    match config.color_mode {
        TPOColorMode::ByPeriod => {
            let last = profile
                .letters
                .last()
                .map_or(1, |letter| letter.period_idx)
                .max(1);
            let position = (period_idx as f32 / last as f32).min(1.0)
                * (render_config.period_colors.len() - 1) as f32;
            let index = position as usize;
            render_config.color_for_period(index).lerp_to_gamma(
                render_config
                    .color_for_period((index + 1).min(render_config.period_colors.len() - 1)),
                position.fract(),
            )
        }
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
    fn letters_have_at_least_4_5_contrast_in_every_mode_period_price_and_theme() {
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
                                contrast(color, bg) >= 4.5,
                                "{name}: {mode:?}, period {period}, price {price}"
                            );
                        }
                        // TPODisplayMode::Both: a letter on its cell, dimmed or not.
                        let cell =
                            block_color(&config, &model, period, price, &profile, &theme.chart);
                        for fill in [cell, theme.chart.bg.lerp_to_gamma(cell, 0.45)] {
                            let letter = cell_letter_color(fill, &theme.chart);
                            assert!(
                                contrast(letter, theme.chart.bg.blend(fill)) >= 4.5,
                                "{name}: {mode:?}, period {period}, price {price} in a cell"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn profile_cells_preserve_period_colors_and_pixel_alignment() {
        let bars: Vec<_> = [(0, 101.0), (60, 102.0), (150, 101.0)]
            .into_iter()
            .map(|(minute, high)| {
                Bar::new(
                    chrono::DateTime::UNIX_EPOCH + Duration::minutes(minute),
                    100.0,
                    high,
                    100.0,
                    100.0,
                    1.0,
                )
            })
            .collect();
        let model = TPOConfig {
            tick_size: 1.0,
            show_single_prints: false,
            show_initial_balance: false,
            show_opening_range: false,
            ..Default::default()
        };
        let mut profile = to_tpo_profiles(&bars, &model, &Timeframe::Min30.into()).remove(0);
        profile.poc_price = 101.0;
        profile.value_area_low = 101.0;
        profile.value_area_high = 101.0;
        let cells = [
            (102.0, 2, 0),
            (101.0, 0, 0),
            (101.0, 2, 1),
            (101.0, 5, 2),
            (100.0, 0, 0),
            (100.0, 2, 1),
            (100.0, 5, 2),
        ];
        let render = TpoRenderConfig::default();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
        let coords = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(90.0, 115.0),
            rect.top(),
            rect.bottom(),
            false,
        );
        for theme in [crate::theme::Theme::dark(), crate::theme::Theme::light()] {
            for mode in [
                TPOColorMode::ByPeriod,
                TPOColorMode::Solid,
                TPOColorMode::ByValueArea,
                TPOColorMode::ByInitialBalance,
            ] {
                for (display_mode, column) in [
                    (TPODisplayMode::Letters, 12.0),
                    (TPODisplayMode::Letters, 3.5),
                    (TPODisplayMode::Blocks, 12.0),
                    (TPODisplayMode::Both, 12.0),
                    (TPODisplayMode::Both, 3.5),
                ] {
                    for (show_poc, show_value_area) in [(false, false), (false, true), (true, true)]
                    {
                        for density in [1.0, 1.5] {
                            let config = TPOConfig {
                                color_mode: mode,
                                display_mode,
                                show_poc,
                                show_value_area,
                                ..model.clone()
                            };
                            let ctx = egui::Context::default();
                            crate::theme::apply_to_egui(&ctx, &theme);
                            ctx.set_pixels_per_point(density);
                            ctx.begin_pass(egui::RawInput::default());
                            draw_session_profile(
                                &ctx.layer_painter(egui::LayerId::background()),
                                rect,
                                &coords,
                                &profile,
                                &config,
                                &render,
                                10.25,
                                column,
                                false,
                                Color32::GREEN,
                                Color32::RED,
                            );
                            let mut output = ctx.end_pass();
                            output.textures_delta.clear();
                            let painted: Vec<_> = output
                                .shapes
                                .iter()
                                .filter(|s| matches!(s.shape, Shape::Rect(_) | Shape::Text(_)))
                                .collect();
                            let readable = column >= 7.0;
                            let letters = display_mode == TPODisplayMode::Letters && readable;
                            let in_cells = display_mode == TPODisplayMode::Both && readable;
                            let shapes_per_cell = if in_cells { 2 } else { 1 };
                            assert_eq!(painted.len(), cells.len() * shapes_per_cell);
                            for (shapes, (price, period, col)) in
                                painted.chunks(shapes_per_cell).zip(cells)
                            {
                                let shape = shapes[0];
                                let mut color = if letters {
                                    letter_color(
                                        &render,
                                        &config,
                                        period,
                                        price,
                                        &profile,
                                        &theme.chart,
                                    )
                                } else {
                                    block_color(
                                        &render,
                                        &config,
                                        period,
                                        price,
                                        &profile,
                                        &theme.chart,
                                    )
                                };
                                if show_poc && price == 101.0 {
                                    color = readable_color(
                                        render.poc_color,
                                        theme.chart.axis_text,
                                        &[theme.chart.bg],
                                        if letters { 4.5 } else { 3.0 },
                                    );
                                } else if !letters && show_value_area && price != 101.0 {
                                    color = theme.chart.bg.lerp_to_gamma(color, 0.45);
                                }
                                match &shape.shape {
                                    Shape::Rect(block) if !letters => {
                                        assert_eq!(block.fill, color);
                                        let gap = 1.0 / density;
                                        let expected = Rect::from_center_size(
                                            Pos2::new(
                                                10.25 + (col as f32 + 0.5) * column,
                                                coords.price_to_y(price),
                                            ),
                                            egui::vec2((column - gap).max(gap), 16.0 - gap),
                                        )
                                        .round_to_pixels(density);
                                        assert_eq!(block.rect, expected);
                                        if in_cells {
                                            let Shape::Text(text) = &shapes[1].shape else {
                                                panic!("no letter: {:?}", shapes[1].shape);
                                            };
                                            assert_eq!(
                                                text.fallback_color,
                                                cell_letter_color(color, &theme.chart)
                                            );
                                            let center = text.pos + text.galley.size() / 2.0;
                                            let error = center - block.rect.center();
                                            // The text position is snapped to physical pixels.
                                            assert!(error.length() <= 1.0, "{error:?}");
                                        }
                                    }
                                    Shape::Text(text) if letters => {
                                        assert_eq!(text.fallback_color, color)
                                    }
                                    _ => panic!("unexpected cell: {:?}", shape.shape),
                                }
                            }
                        }
                    }
                }
            }
        }
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
                true,
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

        // A row of four 0.25 ticks at 110 spans 109.875 to 110.875: y 66 to 82 at 16 px a point.
        let config = TPOConfig {
            tick_size: 1.0,
            instrument_tick: Some(0.25),
            show_single_prints: true,
            ..model.clone()
        };
        let bar = Bar::new(
            chrono::DateTime::UNIX_EPOCH,
            110.0,
            110.5,
            110.0,
            110.5,
            1.0,
        );
        let profile = to_tpo_profiles(&[bar], &config, &Timeframe::Min1.into()).remove(0);
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput::default());
        let painter = ctx.layer_painter(egui::LayerId::background());
        draw_session_profile(
            &painter,
            rect,
            &coords,
            &profile,
            &config,
            &render_config,
            10.0,
            20.0,
            false,
            Color32::GREEN,
            Color32::RED,
        );
        let mut output = ctx.end_pass();
        output.textures_delta.clear();
        let print = output.shapes.iter().find_map(|s| match &s.shape {
            Shape::Rect(r) if r.fill == render_config.single_print_color => Some(r.rect),
            _ => None,
        });
        assert_eq!(print.map(|r| (r.top(), r.bottom())), Some((66.0, 82.0)));
        let letter = output.shapes.iter().find_map(|s| match &s.shape {
            Shape::Text(text) => Some(text.pos.y + text.galley.size().y / 2.0),
            _ => None,
        });
        assert!(letter.is_some_and(|y| (y - 74.0).abs() < 1.0), "{letter:?}");
    }

    #[test]
    fn level_labels_show_only_when_sessions_leave_room_for_them() {
        // Three UTC days of four half-hour bars each
        let bars: Vec<_> = (0..12)
            .map(|i| {
                let minute = i / 4 * 1440 + i % 4 * 30;
                let time = chrono::DateTime::UNIX_EPOCH + Duration::minutes(minute);
                Bar::new(time, 100.0, 101.0, 99.0, 100.0, 1.0)
            })
            .collect();
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let rect = Rect::from_x_y_ranges(0.0..=400.0, 0.0..=400.0);
        let coords = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(90.0, 115.0),
            rect.min.y,
            rect.max.y,
            false,
        );
        // (pixels per bar, sessions labelled): the first session has no room left of the plot.
        for (spacing, expected) in [(30.0, 2), (10.0, 0)] {
            let ctx = egui::Context::default();
            ctx.begin_pass(egui::RawInput::default());
            render_live_tpo(
                &ctx.layer_painter(egui::LayerId::background()),
                rect,
                &bars,
                0..12,
                1,
                &coords,
                &config,
                None,
                &BarClock::new(Timeframe::Min30),
                |i, _| i as f32 * spacing,
                0.0,
                Color32::GREEN,
                Color32::RED,
            );
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            let labelled = output
                .shapes
                .iter()
                .filter(|s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == "VAH"))
                .count();
            assert_eq!(labelled, expected, "{spacing} px per bar");
        }
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
                |profile, _, _, _| counts.push(profile.tpo_count_at(100.0)),
            );
            assert_eq!(counts, expected, "{range:?}, latest={latest_session_only}");
        }
    }

    #[test]
    fn automatic_rows_are_whole_instrument_ticks() {
        use crate::model::ChartTypeParams;
        use crate::scales::{PriceRange, PriceTransform};
        use chrono::TimeZone;
        let start = Utc.with_ymd_and_hms(2024, 1, 2, 14, 30, 0).unwrap();
        let tick_64 = 1.0 / 64.0;
        // (tick, visible prices, half-hour periods as (low, high), letters as (period, row price),
        // point of control with its TPOs)
        type Case = (
            f64,
            (f64, f64),
            Vec<(f64, f64)>,
            Vec<(usize, f64)>,
            (f64, usize),
        );
        let cases: [Case; 2] = [
            // Six points fit rows of 0.2, which round up to one 0.25 tick.
            (
                0.25,
                (5000.0, 5006.0),
                vec![(5000.0, 5000.25), (5000.25, 5000.5)],
                vec![(0, 5000.0), (0, 5000.25), (1, 5000.25), (1, 5000.5)],
                (5000.25, 2),
            ),
            // 109.5 to 111 fits rows of 0.05, 3.2 ticks of 1/64, which round up to 4 (0.0625).
            // Rows start at a multiple of 4 ticks, so the row of 110 holds ticks 0 to 3 above it
            // and every letter is on it.
            (
                tick_64,
                (109.5, 111.0),
                vec![
                    (110.0, 110.0 + 2.0 * tick_64),
                    (110.0 + tick_64, 110.0 + 3.0 * tick_64),
                    (110.0 + tick_64, 110.0 + tick_64),
                ],
                vec![(0, 110.0), (1, 110.0), (2, 110.0)],
                (110.0, 3),
            ),
        ];
        for (tick, (low, high), periods, letters, poc) in cases {
            let map = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(low, high),
                0.0,
                400.0,
                false,
            );
            let base = ChartTypeParams::default().tpo_config(Some(tick));
            let config = row_config(&base, Some(tick), &map).unwrap();
            let bars: Vec<_> = periods
                .iter()
                .enumerate()
                .map(|(i, &(low, high))| {
                    Bar::new(
                        start + Duration::minutes(30 * i as i64),
                        low,
                        high,
                        low,
                        high,
                        1.0,
                    )
                })
                .collect();
            let profile = &to_tpo_profiles(&bars, &config, &Timeframe::Min30.into())[0];
            let mut placed: Vec<_> = profile
                .letters
                .iter()
                .map(|letter| (letter.period_idx, letter.price))
                .collect();
            placed.sort_by(|a, b| a.partial_cmp(b).unwrap());
            assert_eq!(placed, letters, "tick {tick}");
            assert_eq!(
                (profile.poc_price, profile.tpo_count_at(profile.poc_price)),
                poc,
                "tick {tick}"
            );
        }
        // 0.0006 fits rows of 2e-5, exactly 20 ticks of 1e-6, although the quotient computes as
        // 20.000000000000004
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(0.001, 0.0016),
            0.0,
            400.0,
            false,
        );
        let base = ChartTypeParams::default().tpo_config(Some(1e-6));
        assert_eq!(
            row_config(&base, Some(1e-6), &map).unwrap().tick_size,
            20.0 * 1e-6
        );
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
