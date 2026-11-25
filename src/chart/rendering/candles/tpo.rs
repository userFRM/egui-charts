//! # TPO (Time Price Opportunity) Chart Renderer
//!
//! Full Market Profile rendering with support for:
//! - TPO letters (A, B, C...) at each price level
//! - Point of Control (POC) highlighting
//! - Value Area guides and cell emphasis
//! - Initial Balance range and extensions
//! - Poor high and low markers
//! - Single prints highlighting
//! - Session separators
//! - Multiple color modes and display modes
//!
//! The live entry point is [`render_live_tpo`], which builds the profiles of the
//! whole sessions of the visible bars and anchors each session's letter histogram
//! at the session it summarises, staying correct under pan and zoom.

use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc};

use crate::localization as l10n;
use crate::painting::span_cache::{SpanCache, SpanFingerprint};
use crate::scales::PriceMap;
use egui::emath::GuiRounding as _;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2};

use crate::model::tpo::OUTSIDE_VALUE_AREA_OPACITY;
use crate::model::{
    Bar, BarClock, ExchangeSessionProvider, SessionProvider, TPOColorMode, TPOConfig,
    TPODisplayMode, TPOProfile, TPOProfilePeriod, TPOStatisticsRows, derive_tick_size,
    to_tpo_profiles,
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

/// Width kept free to the left of a profile for its level captions.
const LABEL_ROOM_PX: f32 = 40.0;

/// Width of the busiest row of the volume profile beside a TPO profile.
const VOLUME_PROFILE_PX: f32 = 40.0;

/// Estimated caption digit width, shared by painting, fitting and hit testing.
// shortcut: font-free room assumes digits no wider than 0.6 em; measure if host fonts clip.
const PRICE_CHAR_PX: f32 = 0.6 * typography::XS;

/// Caption order also decides which coinciding level keeps its own row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Level {
    Poc,
    ValueAreaHigh,
    ValueAreaLow,
    Midpoint,
    InitialBalanceHigh,
    InitialBalanceLow,
    Open,
    Close,
    PoorHigh,
    PoorLow,
    SinglePrints,
}

impl Level {
    /// Caption text and translation key.
    fn name(self) -> &'static str {
        match self {
            Self::Poc => "POC",
            Self::ValueAreaHigh => "VAH",
            Self::ValueAreaLow => "VAL",
            Self::Midpoint => "Mid",
            Self::InitialBalanceHigh => "IBH",
            Self::InitialBalanceLow => "IBL",
            Self::Open => "Open",
            Self::Close => "Close",
            Self::PoorHigh => "PH",
            Self::PoorLow => "PL",
            Self::SinglePrints => "SP",
        }
    }

    /// Row levels mark the row centre rather than the exact price.
    fn is_row(self) -> bool {
        matches!(
            self,
            Self::Poc
                | Self::ValueAreaHigh
                | Self::ValueAreaLow
                | Self::PoorHigh
                | Self::PoorLow
                | Self::SinglePrints
        )
    }
}

/// A single price, or the first and last row of a single-print run.
struct Caption {
    level: Level,
    low: f64,
    high: f64,
}

fn level_captions(profile: &TPOProfile, config: &TPOConfig) -> Vec<Caption> {
    let labels = &config.labels;
    let balance =
        config.show_initial_balance && profile.initial_balance_high >= profile.initial_balance_low;
    let mut captions: Vec<_> = [
        (Level::Poc, config.show_poc && labels.poc, profile.poc_price),
        (
            Level::ValueAreaHigh,
            config.show_value_area && labels.value_area_high,
            profile.value_area_high,
        ),
        (
            Level::ValueAreaLow,
            config.show_value_area && labels.value_area_low,
            profile.value_area_low,
        ),
        (
            Level::Midpoint,
            config.show_midpoint && labels.midpoint,
            (profile.profile_high + profile.profile_low) * 0.5,
        ),
        (
            Level::InitialBalanceHigh,
            balance && labels.initial_balance_high,
            profile.initial_balance_high,
        ),
        (
            Level::InitialBalanceLow,
            balance && labels.initial_balance_low,
            profile.initial_balance_low,
        ),
        (
            Level::Open,
            config.show_opening_range && labels.opening_price,
            profile.opening_price,
        ),
        (
            Level::Close,
            config.show_closing_price && labels.closing_price && profile.closing_price.is_finite(),
            profile.closing_price,
        ),
        (
            Level::PoorHigh,
            config.show_poor_high && profile.has_poor_high && labels.poor_high,
            profile.profile_high,
        ),
        (
            Level::PoorLow,
            config.show_poor_low && profile.has_poor_low && labels.poor_low,
            profile.profile_low,
        ),
    ]
    .into_iter()
    .filter(|(_, shown, _)| *shown)
    .map(|(level, _, price)| Caption {
        level,
        low: price,
        high: price,
    })
    .collect();
    if config.show_single_prints && labels.single_prints {
        captions.extend(
            profile
                .single_prints
                .chunk_by(|a, b| (b - a - profile.tick_size).abs() < profile.tick_size * 0.5)
                .map(|run| Caption {
                    level: Level::SinglePrints,
                    low: run[0],
                    high: run[run.len() - 1],
                }),
        );
    }
    captions
}

fn caption_value(caption: &Caption, profile: &TPOProfile) -> String {
    let tick = if caption.level == Level::Midpoint {
        profile.instrument_tick * 0.5
    } else {
        profile.instrument_tick
    };
    let decimals = crate::scales::decimals(tick);
    let Caption { low, high, .. } = *caption;
    if low == high {
        format!("{low:.decimals$}")
    } else {
        format!("{low:.decimals$} / {high:.decimals$}")
    }
}

fn caption_color(
    level: Level,
    profile: &TPOProfile,
    chart: &ChartColors,
    render_config: &TpoRenderConfig,
    bullish: Color32,
    bearish: Color32,
) -> Color32 {
    let color = match level {
        Level::Poc | Level::PoorHigh | Level::PoorLow => return chart.axis_text,
        Level::ValueAreaHigh | Level::ValueAreaLow => return chart.axis_text_secondary,
        Level::Midpoint => bearish,
        Level::InitialBalanceHigh | Level::InitialBalanceLow => render_config.initial_balance_color,
        Level::Open => {
            if profile.poc_price >= profile.opening_price {
                bullish
            } else {
                bearish
            }
        }
        Level::Close => {
            if profile.closing_price >= profile.opening_price {
                bullish
            } else {
                bearish
            }
        }
        Level::SinglePrints => render_config.single_print_color.to_opaque(),
    };
    readable_color(color, chart.axis_text, &[chart.bg], 4.5)
}

/// All placed profiles contribute, so vertical scrolling keeps the same gutter.
fn label_room<'a>(config: &TPOConfig, profiles: impl IntoIterator<Item = &'a TPOProfile>) -> f32 {
    if !config.labels.prices {
        return LABEL_ROOM_PX;
    }
    let chars = profiles
        .into_iter()
        .flat_map(|profile| {
            level_captions(profile, config)
                .into_iter()
                .map(move |caption| caption_value(&caption, profile).chars().count() + 1)
        })
        .max()
        .unwrap_or(0);
    LABEL_ROOM_PX + PRICE_CHAR_PX * chars as f32
}

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
    /// Highlight behind rows touched by a single period (with alpha); blocks take it opaque
    pub single_print_color: Color32,
    /// Default letter color
    pub default_letter_color: Color32,
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
        }
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

type ProfileUnits = Vec<(usize, usize, Rc<Vec<TPOProfile>>)>;

struct CachedProfiles {
    span: SpanFingerprint,
    last_time: Option<chrono::DateTime<chrono::Utc>>,
    config: TPOConfig,
    clock: BarClock,
    units: Rc<ProfileUnits>,
}

thread_local! {
    static GRID_CACHE: RefCell<SpanCache<f64, u64>> = const { RefCell::new(SpanCache::new(8)) };
    static PROFILE_CACHE: RefCell<Vec<CachedProfile>> = const { RefCell::new(Vec::new()) };
    static PROFILES_CACHE: RefCell<Vec<CachedProfiles>> = const { RefCell::new(Vec::new()) };
}

/// Whether profiles built with `old` equal those built with `new`.
fn same_profiles(old: &TPOConfig, new: &TPOConfig) -> bool {
    old.tick_size.to_bits() == new.tick_size.to_bits()
        && old.period_minutes == new.period_minutes
        && old.initial_balance_minutes == new.initial_balance_minutes
        && old.value_area_pct.to_bits() == new.value_area_pct.to_bits()
        && old.show_volume_profile == new.show_volume_profile
        && old.custom_letters == new.custom_letters
        && old.split_sessions == new.split_sessions
        && old.session == new.session
        && old.regular_hours == new.regular_hours
        && old.profile_period == new.profile_period
        && old.composites == new.composites
        && old.splits == new.splits
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
        // Adjacent bars of one period may share a profile, so keep their sessions together.
        if config.profile_period != TPOProfilePeriod::Session
            && !matches!(config.session, crate::model::ProfileSession::Custom(_))
        {
            let period = |i: usize| config.profile_period.key(sessions, full[i].time);
            while start > 0 && period(start - 1) == period(start) {
                start = start_of(start - 1);
            }
            while end < full.len() && period(end - 1) == period(end) {
                end = end_of(end);
            }
        }
        if before == (start..end) {
            break;
        }
    }
    if let crate::model::ProfileSession::Custom(custom) = &config.session {
        // A month and an overnight window fit in this padding; no unrelated history is scanned.
        let days = match config.profile_period {
            TPOProfilePeriod::Session => 2,
            TPOProfilePeriod::Week => 9,
            TPOProfilePeriod::Month => 33,
        };
        let padding = chrono::TimeDelta::days(days);
        let first = full[start]
            .time
            .checked_sub_signed(padding)
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC);
        let last = full[end - 1]
            .time
            .checked_add_signed(padding)
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MAX_UTC);
        let slice =
            full.partition_point(|b| b.time < first)..full.partition_point(|b| b.time <= last);
        let parts = custom.parts(
            full,
            slice,
            crate::model::Timeframe::Min1,
            config.regular_hours.as_ref(),
            true,
        );
        let key = |session, date| {
            let layer = if custom.overlap == crate::model::SessionOverlap::InBoth {
                session
            } else {
                0
            };
            (layer, config.profile_period.date_key(date))
        };
        let selected = start..end;
        let periods: Vec<_> = parts
            .iter()
            .filter(|p| p.bars.start < selected.end && p.bars.end > selected.start)
            .map(|p| key(p.session, p.date))
            .filter(|(_, period)| period.is_some())
            .collect();
        for part in parts {
            if (part.bars.start < selected.end && part.bars.end > selected.start)
                || periods.contains(&key(part.session, part.date))
            {
                start = start.min(part.bars.start);
                end = end.max(part.bars.end);
            }
        }
    }
    start..end
}

/// Profiles of the sessions in `bars`, each with the index of its first bar and its bar count,
/// reusing those whose inputs are unchanged. Sessions in one composite are built together.
fn cached_profiles(
    bars: &[Bar],
    revision: u64,
    start_idx: usize,
    config: &TPOConfig,
    clock: &BarClock,
    sessions: &ExchangeSessionProvider,
) -> Rc<ProfileUnits> {
    let span = SpanFingerprint::of(start_idx, bars, revision);
    let last_time = bars.last().map(|bar| bar.time);
    PROFILES_CACHE.with_borrow_mut(|cache| {
        if let Some(at) = cache.iter().position(|entry| {
            entry.span == span
                && entry.last_time == last_time
                && same_profiles(&entry.config, config)
                && entry.clock == *clock
        }) {
            let entry = cache.remove(at);
            let units = Rc::clone(&entry.units);
            cache.push(entry);
            return units;
        }
        // Fitting and painting share the same sessions without rebuilding their lookup tables.
        let units = Rc::new(build_profiles(
            bars, revision, start_idx, config, clock, sessions,
        ));
        if cache.len() >= 8 {
            cache.remove(0);
        }
        cache.push(CachedProfiles {
            span,
            last_time,
            config: config.clone(),
            clock: clock.clone(),
            units: Rc::clone(&units),
        });
        units
    })
}

fn build_profiles(
    mut bars: &[Bar],
    revision: u64,
    mut start_idx: usize,
    config: &TPOConfig,
    clock: &BarClock,
    sessions: &ExchangeSessionProvider,
) -> ProfileUnits {
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
    // Keep whole sessions in each unit so split parts can count periods from their session's open.
    let starts = crate::model::tpo::session_starts(bars, sessions, config.split_hours(), &[]);
    let parts = if matches!(config.session, crate::model::ProfileSession::Custom(_)) {
        // A custom profile may open after the chart's day starts. Composites refer to its own
        // opening bar, including a split, rather than a bar outside the selected hours.
        Some(
            crate::model::tpo::profile_parts(bars, sessions, &config, clock.timeframe())
                .into_iter()
                .map(|part| part.bars.start)
                .collect::<Vec<_>>(),
        )
    } else {
        (!config.splits.is_empty()).then(|| {
            crate::model::tpo::session_starts(bars, sessions, config.split_hours(), &config.splits)
        })
    };
    let mut profile_open = starts.first().copied().unwrap_or(0);
    let custom_parts = match &config.session {
        crate::model::ProfileSession::Custom(custom) => custom.parts(
            bars,
            0..bars.len(),
            clock.timeframe().unwrap_or_default(),
            config.regular_hours.as_ref(),
            true,
        ),
        _ => Vec::new(),
    };
    let breaks = starts.into_iter().skip(1);
    for end in breaks.chain([bars.len()]) {
        if end < bars.len()
            && (matches!(config.session, crate::model::ProfileSession::Custom(_))
                && config.profile_period != TPOProfilePeriod::Session
                || custom_parts
                    .iter()
                    .any(|p| p.bars.start < end && end < p.bars.end))
        {
            continue;
        }
        // A session's last split part can join the next session while its first part stays apart.
        let last_open = parts
            .as_ref()
            .and_then(|parts| {
                parts
                    .partition_point(|&i| i < end)
                    .checked_sub(1)
                    .map(|i| parts[i])
            })
            .unwrap_or(profile_open);
        let joins_period = config.profile_period != TPOProfilePeriod::Session
            && bars.get(end).is_some_and(|next| {
                config.profile_period.key(sessions, bars[end - 1].time)
                    == config.profile_period.key(sessions, next.time)
            });
        if joins_period
            || bars.get(end).is_some_and(|next| {
                composite(&bars[last_open]).is_some_and(|group| composite(next) == Some(group))
            })
        {
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
    let render_config = TpoRenderConfig {
        poc_color: painter.ctx().active_theme().ui.text,
        ..Default::default()
    };
    let x = |i| idx_to_coord(i, chart_rect_min_x);
    let extend = |profile: &TPOProfile, ends: [Option<Pos2>; 2]| {
        let from = profile.letters.last().map_or(full_data.len(), |last| {
            full_data.partition_point(|bar| bar.time < last.ts)
        });
        for (start, above) in ends.into_iter().zip([true, false]) {
            let Some(start) = start else { continue };
            let price = if above {
                profile.profile_high
            } else {
                profile.profile_low
            };
            let mut end = price_rect.right();
            for (i, bar) in full_data.iter().enumerate().skip(from) {
                let bar_x = x(i);
                if bar_x > price_rect.right() {
                    break;
                }
                if (above && bar.high > price) || (!above && bar.low < price) {
                    end = bar_x;
                    break;
                }
            }
            if end > start.x {
                painter.line_segment(
                    [start, Pos2::new(end, start.y)],
                    Stroke::new(
                        DESIGN_TOKENS.stroke.light,
                        painter.ctx().active_theme().chart.axis_text,
                    ),
                );
            }
        }
    };
    visit_profiles(
        full_data,
        visible.clone(),
        revision,
        (base_config, instrument_tick),
        clock,
        (price_map, price_rect, &x),
        |profile, config, anchor_x, column_width, room, label_room| {
            let ends = draw_session_profile(
                painter,
                price_rect,
                price_map,
                profile,
                config,
                &render_config,
                anchor_x,
                column_width,
                room,
                label_room,
                bullish_color,
                bearish_color,
            );
            if config.extend_poor_extremes && room.is_finite() {
                extend(profile, ends);
            }
        },
    );
    if !base_config.extend_poor_extremes
        || (!base_config.show_poor_high && !base_config.show_poor_low)
        || base_config.latest_session_only
        || visible.start == 0
        || visible.is_empty()
        || visible.end > full_data.len()
    {
        return;
    }
    let Some(config) = row_config(base_config, instrument_tick, price_map) else {
        return;
    };
    let sessions = clock.sessions();
    let history = whole_sessions(full_data, 0..visible.start, &sessions, &config);
    let units = cached_profiles(
        &full_data[history.clone()],
        revision,
        history.start,
        &config,
        clock,
        &sessions,
    );
    // A completed profile can leave the plot while its unrepaired extremes still cross it.
    // Keep these lines separate from placement and fitting, which use visible profiles only.
    for (first, len, profiles) in units.iter() {
        for (i, profile) in profiles.iter().enumerate() {
            let end = profiles.get(i + 1).map_or(first + len, |next| {
                full_data.partition_point(|bar| bar.time < next.session_date)
            });
            if end == full_data.len() || x(end) > price_rect.left() {
                continue;
            }
            let mark = |shown: bool, poor: bool, price| {
                let y = row_center_y(price_map, profile, price);
                (shown
                    && poor
                    && y.is_finite()
                    && (price_rect.top()..=price_rect.bottom()).contains(&y))
                .then(|| Pos2::new(price_rect.left(), y))
            };
            extend(
                profile,
                [
                    mark(
                        config.show_poor_high,
                        profile.has_poor_high,
                        profile.profile_high,
                    ),
                    mark(
                        config.show_poor_low,
                        profile.has_poor_low,
                        profile.profile_low,
                    ),
                ],
            );
        }
    }
}

/// Visits the renderer's placed distributions, for painting and price fitting.
pub(crate) fn visit_profiles(
    full_data: &[Bar],
    visible: Range<usize>,
    revision: u64,
    (base_config, instrument_tick): (&TPOConfig, Option<f64>),
    clock: &BarClock,
    (price_map, price_rect, idx_to_coord): (&PriceMap, Rect, impl Fn(usize) -> f32),
    mut visit: impl FnMut(&TPOProfile, &TPOConfig, f32, f32, f32, f32),
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
    place_profiles(
        &units,
        full_data,
        price_rect,
        &config,
        &sessions,
        idx_to_coord,
        |profile, anchor, column, room, label_room| {
            visit(profile, &config, anchor, column, room, label_room)
        },
    );
}

/// Captioned prices and colours of the profile holding the last bar, while it is placed.
/// Single-print runs span prices and take no scale tag.
pub(crate) fn latest_level_tags(
    full_data: &[Bar],
    visible: Range<usize>,
    revision: u64,
    config: (&TPOConfig, Option<f64>),
    clock: &BarClock,
    placement: (&PriceMap, Rect, impl Fn(usize) -> f32),
    (chart, bullish, bearish): (&ChartColors, Color32, Color32),
) -> Vec<(f64, Color32)> {
    let mut tags = Vec::new();
    let right = placement.1.right();
    visit_profiles(
        full_data,
        visible,
        revision,
        config,
        clock,
        placement,
        |profile, config, anchor, _, room, _| {
            if room.is_infinite() && anchor < right {
                tags.extend(
                    level_captions(profile, config)
                        .into_iter()
                        .filter(|caption| caption.level != Level::SinglePrints)
                        .map(|caption| {
                            (
                                caption.low,
                                caption_color(
                                    caption.level,
                                    profile,
                                    chart,
                                    &TpoRenderConfig::default(),
                                    bullish,
                                    bearish,
                                ),
                            )
                        }),
                );
            }
        },
    );
    tags
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

/// Visit profiles at the same anchors and column widths for painting and hit testing, with the
/// room each has from its anchor to the next session, infinite for the forming one.
fn place_profiles(
    units: &[(usize, usize, Rc<Vec<TPOProfile>>)],
    full_data: &[Bar],
    price_rect: Rect,
    config: &TPOConfig,
    sessions: &ExchangeSessionProvider,
    idx_to_coord: impl Fn(usize) -> f32,
    mut visit: impl FnMut(&TPOProfile, f32, f32, f32, f32),
) {
    let balance_room = if config.show_initial_balance_extensions {
        10.0
    } else {
        0.0
    };
    let poor_room = if config.show_poor_high || config.show_poor_low {
        12.0
    } else {
        0.0
    };
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
            // Shared custom windows can start on the same bar; each still needs drawing room.
            let end = if matches!(&config.session, crate::model::ProfileSession::Custom(custom)
                if custom.overlap == crate::model::SessionOverlap::InBoth)
            {
                anchors
                    .iter()
                    .copied()
                    .find(|&i| i > span[0])
                    .unwrap_or(span[1])
            } else {
                span[1]
            };
            let end_x = if end == full_data.len() {
                f32::INFINITY
            } else {
                idx_to_coord(end)
            };
            // A session that opened left of the plot keeps its profile at the plot's edge.
            let open_x = idx_to_coord(span[0]);
            // Leave the extension strokes inside the plot when the session opens out of view.
            let anchor_x = open_x.max(price_rect.left() + balance_room);
            let widest = profile_columns(profile, config);
            if end_x > anchor_x && widest > 0 {
                placed.push((
                    profile,
                    anchor_x,
                    end_x - anchor_x,
                    widest,
                    open_x < price_rect.left(),
                ));
            }
        }
    }
    // Level labels sit in the room left before each next session, so they show only while every
    // session in full view keeps at least as much width for its own profile.
    let label_room = label_room(config, placed.iter().map(|p| p.0));
    // The forming session has infinite room; keep visible room for both its priced gutter and profile.
    let labels = (!config.labels.prices || price_rect.width() >= 2.0 * label_room)
        && placed
            .iter()
            .filter(|(.., cut)| !cut)
            .all(|&(_, _, width, ..)| width >= 2.0 * label_room);
    let volume_room = if config.show_volume_profile {
        14.0 + VOLUME_PROFILE_PX
    } else {
        0.0
    };
    let room = (if labels { label_room } else { 0.0 }).max(balance_room + poor_room) + volume_room;
    if labels {
        for (_, anchor, width, widest, _) in &mut placed {
            let gutter = (price_rect.left() + label_room - *anchor).max(0.0);
            if *width >= gutter + room + *widest as f32 {
                *anchor += gutter;
                *width -= gutter;
            }
        }
    }
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
            width,
            if labels { label_room } else { 0.0 },
        );
    }
}

/// Columns a profile spans: one per period in split display, otherwise its widest row.
pub(crate) fn profile_columns(profile: &TPOProfile, config: &TPOConfig) -> usize {
    if config.split_periods {
        profile.letters.chunk_by(|a, b| a.ts == b.ts).count()
    } else {
        profile.price_tpo_count.values().max().copied().unwrap_or(0)
    }
}

/// First bar time of each period, in the order of the split display's columns.
fn period_starts(profile: &TPOProfile) -> Vec<chrono::DateTime<chrono::Utc>> {
    profile
        .letters
        .chunk_by(|a, b| a.ts == b.ts)
        .map(|period| period[0].ts)
        .collect()
}

/// The displayed profile at a menu click, including one anchored at the plot's edge: its first
/// bar time and, when a letter or block is clicked, the first bar time of that period.
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
) -> Option<(
    chrono::DateTime<chrono::Utc>,
    Option<chrono::DateTime<chrono::Utc>>,
)> {
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
        |profile, anchor, column, _, label_room| {
            let column = column.min(profile_row_height(profile, &mapping.price));
            let price = mapping.y_to_price(pos.y);
            let row = profile.row_price(price);
            let mut letters = profile.letters_at(row);
            letters.sort_by_key(|letter| letter.ts);
            let starts = config.split_periods.then(|| period_starts(profile));
            let reach = if config.show_value_area && profile.is_in_value_area(row) {
                profile_columns(profile, &config)
            } else if let Some(starts) = &starts {
                letters
                    .iter()
                    .map(|letter| starts.partition_point(|ts| *ts < letter.ts) + 1)
                    .max()
                    .unwrap_or(0)
            } else {
                letters.len()
            };
            // Keep the gutter beside occupied rows in the profile's context-menu target.
            let left = anchor - label_room;
            if reach > 0 && pos.x >= left && pos.x < anchor + reach as f32 * column {
                let period = (pos.x >= anchor)
                    .then(|| ((pos.x - anchor) / column) as usize)
                    .and_then(|col| {
                        if let Some(starts) = &starts {
                            starts
                                .get(col)
                                .copied()
                                .filter(|ts| letters.iter().any(|letter| letter.ts == *ts))
                        } else {
                            letters.get(col).map(|letter| letter.ts)
                        }
                    });
                hit = Some((profile.session_date, period));
            }
        },
    );
    hit
}

/// Columns from the anchor to the right end of the row holding `price`, in packed or split display.
fn row_reach(
    profile: &TPOProfile,
    starts: Option<&[chrono::DateTime<chrono::Utc>]>,
    price: f64,
) -> usize {
    starts.map_or_else(
        || profile.tpo_count_at(price),
        |starts| {
            profile
                .letters_at(price)
                .iter()
                .map(|letter| starts.partition_point(|ts| *ts < letter.ts) + 1)
                .max()
                .unwrap_or(0)
        },
    )
}

/// Pixel y of the centre of the row holding `price`, where the row's marks and lines sit.
/// A row of several ticks spans from half a tick below its price, so its centre is not its price.
fn row_center_y(coords: &PriceMap, profile: &TPOProfile, price: f64) -> f32 {
    let (low, high) = profile.row_span(price);
    coords.price_to_y((low + high) * 0.5)
}

/// Draw one session's profile anchored at pixel x `anchor_x`, growing rightward in columns of
/// `column_width` pixels, with its level labels in the left `label_room` and its
/// statistics below when they fit the `room` of its session.
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
    room: f32,
    label_room: f32,
    bullish_color: Color32,
    bearish_color: Color32,
) -> [Option<Pos2>; 2] {
    if profile.price_tpo_count.is_empty() {
        return [None; 2];
    }

    let chart = painter.ctx().active_theme().chart;
    let tick_size = profile.tick_size;
    let measured_height = profile_row_height(profile, coords);
    let row_y = |price: f64| row_center_y(coords, profile, price);
    let readable = column_width.min(measured_height) >= LETTER_MIN_COLUMN_PX;
    // `Both` paints the cells of `Blocks` and adds the letters while they stay readable.
    let use_letters = readable && config.display_mode == TPODisplayMode::Letters;
    let letters_in_cells = readable && config.display_mode == TPODisplayMode::Both;

    let row_height = measured_height.min(render_config.row_height * 1.5);
    let column_width = column_width.min(row_height);
    let pixels_per_point = painter.ctx().pixels_per_point();
    let gap = 1.0 / pixels_per_point;
    let levels = profile.price_levels();
    let columns = profile_columns(profile, config);
    let max_right = anchor_x + columns as f32 * column_width;
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

    let starts = config.split_periods.then(|| period_starts(profile));

    // Single-print rows are highlighted behind their letter; hovering one names its price.
    if config.show_single_prints {
        let decimals = crate::scales::decimals(profile.instrument_tick);
        for &price in &profile.single_prints {
            let (low, high) = profile.row_span(price);
            let top = coords.price_to_y(high);
            let bottom = coords.price_to_y(low);
            let left = anchor_x
                + starts
                    .as_ref()
                    .and_then(|starts| {
                        profile
                            .letters_at(price)
                            .first()
                            .map(|letter| starts.partition_point(|ts| *ts < letter.ts))
                    })
                    .unwrap_or(0) as f32
                    * column_width;
            let row = Rect::from_x_y_ranges(
                left..=left + column_width,
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
    let opacity = config.outside_value_area_opacity;
    let opacity = if opacity.is_nan() {
        OUTSIDE_VALUE_AREA_OPACITY
    } else {
        opacity.clamp(0.0, 1.0)
    };
    let dim = |color: Color32| chart.bg.lerp_to_gamma(color, opacity);
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

    for &price in &levels {
        let y = row_y(price);
        if y < price_rect.top() - row_height || y > price_rect.bottom() + row_height {
            continue;
        }
        let is_poc = config.show_poc && (price - profile.poc_price).abs() < tick_size * 0.5;
        let single_print = config.show_single_prints && profile.tpo_count_at(price) == 1;
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
            let col = starts
                .as_ref()
                .map_or(col, |starts| starts.partition_point(|ts| *ts < letter.ts));
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
                // An opaque cell hides the highlight behind it, so a single print takes the colour
                // itself: a tint barely shows on a cell dimmed toward a light background. The POC
                // row keeps its colour, as every row of a one-period profile is a single print.
                let color = if single_print && !is_poc {
                    render_config.single_print_color.to_opaque()
                } else {
                    color
                };
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

    let mut poor_ends = [None; 2];
    for (end, (poor, price)) in poor_ends.iter_mut().zip([
        (
            config.show_poor_high && profile.has_poor_high,
            profile.profile_high,
        ),
        (
            config.show_poor_low && profile.has_poor_low,
            profile.profile_low,
        ),
    ]) {
        let y = row_y(price);
        if poor && price_rect.y_range().contains(y) {
            let right =
                anchor_x + row_reach(profile, starts.as_deref(), price) as f32 * column_width;
            painter.line_segment(
                [Pos2::new(right + 4.0, y), Pos2::new(right + 12.0, y)],
                Stroke::new(DESIGN_TOKENS.stroke.medium, chart.axis_text),
            );
            *end = Some(Pos2::new(right + 12.0, y));
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

    if config.show_volume_profile {
        let max = levels
            .iter()
            .map(|&price| profile.volume_at(price))
            .fold(0.0_f64, f64::max);
        if max > 0.0 {
            let left = max_right + 14.0;
            for &price in &levels {
                let volume = profile.volume_at(price);
                let y = row_y(price);
                if volume <= 0.0
                    || y + row_height < price_rect.top()
                    || y - row_height > price_rect.bottom()
                {
                    continue;
                }
                let top = (y - block_height * 0.5).round_to_pixels(pixels_per_point);
                let bottom = (y + block_height * 0.5).round_to_pixels(pixels_per_point);
                let color = if dims_outside_value_area && !profile.is_in_value_area(price) {
                    dim(chart.axis_text_secondary)
                } else {
                    chart.axis_text_secondary
                };
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(left, top),
                        Pos2::new(left + VOLUME_PROFILE_PX * (volume / max) as f32, bottom),
                    ),
                    0.0,
                    color,
                );
            }
        }
    }

    if label_room > 0.0 {
        let mut label_rows = Vec::new();
        let captions = level_captions(profile, config);
        let candidates = 2 * captions.len();
        for caption in captions {
            let level = caption.level;
            let y_at = |price| {
                if level.is_row() {
                    row_y(price)
                } else {
                    coords.price_to_y(price)
                }
            };
            let y = (y_at(caption.low) + y_at(caption.high)) * 0.5;
            if !price_rect.y_range().contains(y) || anchor_x - price_rect.left() < label_room - 8.0
            {
                continue;
            }
            let name = l10n::text(painter.ctx(), level.name());
            let text = if config.labels.prices {
                format!("{name} {}", caption_value(&caption, profile))
            } else {
                name.clone()
            };
            let color = caption_color(
                level,
                profile,
                &chart,
                render_config,
                bullish_color,
                bearish_color,
            );
            let galley = painter.layout_no_wrap(text, FontId::proportional(typography::XS), color);
            let half_height = galley.size().y * 0.5;
            if price_rect.height() < galley.size().y {
                continue;
            }
            let spacing = galley.size().y + 2.0;
            let center = y.clamp(
                price_rect.top() + half_height,
                price_rect.bottom() - half_height,
            );
            // Displace only the caption when levels coincide; the marker keeps its price.
            // Each earlier caption can block two neighbouring candidates.
            let Some(label_y) = std::iter::once(0.0)
                .chain((1..=candidates).flat_map(|i| [-(i as f32), i as f32]))
                .map(|offset| center + offset * spacing)
                .find(|candidate| {
                    *candidate >= price_rect.top() + half_height
                        && *candidate <= price_rect.bottom() - half_height
                        && label_rows.iter().all(|previous: &f32| {
                            (candidate - previous).abs() >= galley.size().y + 1.0
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
            let caption_rect = egui::Align2::RIGHT_CENTER
                .anchor_size(Pos2::new(anchor_x - 8.0, label_y), galley.size());
            painter.galley(caption_rect.min, galley, color);
            let ctx = painter.ctx();
            if ctx.pointer_hover_pos().is_some_and(|p| {
                caption_rect.contains(p) && ctx.layer_id_at(p) == Some(painter.layer_id())
            }) {
                egui::Tooltip::always_open(
                    ctx.clone(),
                    painter.layer_id(),
                    egui::Id::unique((
                        "tpo-level",
                        profile.session_date,
                        level,
                        caption.low.to_bits(),
                    )),
                    caption_rect,
                )
                .show(|ui| {
                    ui.label(format!("{name} {}", caption_value(&caption, profile)));
                });
            }
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
    if (config.show_initial_balance || config.show_initial_balance_extensions)
        && profile.initial_balance_high >= profile.initial_balance_low
    {
        let top = coords.price_to_y(profile.initial_balance_high);
        let bottom = coords.price_to_y(profile.initial_balance_low);
        let color = readable_color(
            render_config.initial_balance_color,
            chart.axis_text_secondary,
            &[chart.bg],
            3.0,
        );
        if config.show_initial_balance {
            painter.rect_filled(
                Rect::from_x_y_ranges(anchor_x - 2.0..=anchor_x, top.min(bottom)..=top.max(bottom)),
                0.0,
                color,
            );
        }
        let range = profile.initial_balance_high - profile.initial_balance_low;
        if config.show_initial_balance_extensions && range > 0.0 && range.is_finite() {
            for multiple in [1.0, 2.0] {
                for price in [
                    profile.initial_balance_low - multiple * range,
                    profile.initial_balance_high + multiple * range,
                ] {
                    let y = coords.price_to_y(price);
                    if price.is_finite() && y.is_finite() && price_rect.y_range().contains(y) {
                        painter.line_segment(
                            [Pos2::new(anchor_x - 10.0, y), Pos2::new(anchor_x, y)],
                            Stroke::new(DESIGN_TOKENS.stroke.light, color),
                        );
                    }
                }
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

    if config.show_closing_price && profile.closing_price.is_finite() {
        let y = coords.price_to_y(profile.closing_price);
        if price_rect.y_range().contains(y) {
            let right = anchor_x
                + row_reach(profile, starts.as_deref(), profile.closing_price) as f32
                    * column_width;
            let color = if profile.closing_price >= profile.opening_price {
                bullish_color
            } else {
                bearish_color
            };
            let s = 3.0;
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(right + 1.0, y),
                    Pos2::new(right + 1.0 + s, y - s * 0.5),
                    Pos2::new(right + 1.0 + s, y + s * 0.5),
                ],
                color,
                Stroke::NONE,
            ));
        }
    }

    if config.show_statistics {
        // The next session's captions sit in the strip that ends this session's room.
        let captions = label_room > 0.0 && config.shows_captions();
        let room = if captions { room - label_room } else { room };
        draw_statistics(
            painter,
            price_rect,
            coords,
            profile,
            anchor_x,
            room,
            &config.statistics_rows,
        );
    }
    poor_ends
}

/// Paint the session statistics table with its left edge at `anchor_x`, below the profile's lowest
/// row or up against the bottom of the plot, unless it is wider than the `room` of its session.
fn draw_statistics(
    painter: &Painter,
    price_rect: Rect,
    coords: &PriceMap,
    profile: &TPOProfile,
    anchor_x: f32,
    room: f32,
    shown: &TPOStatisticsRows,
) {
    let chart = painter.ctx().active_theme().chart;
    let decimals = crate::scales::decimals(profile.instrument_tick);
    let price = |value: f64| format!("{value:.decimals$}");
    let (below, above) = profile.tpo_counts_around_poc();
    let rotation = match profile.rotation_factor() {
        0 => "0".to_owned(),
        rotation => format!("{rotation:+}"),
    };
    let balance = profile.initial_balance_range();
    let values: [Option<String>; 12] = [
        Some(price(profile.profile_high - profile.profile_low)),
        Some(price(profile.poc_price)),
        Some(format!(
            "{} / {}",
            price(profile.value_area_low),
            price(profile.value_area_high)
        )),
        Some(price(profile.value_area_high - profile.value_area_low)),
        balance.map(|_| price(profile.initial_balance_high)),
        balance.map(|_| price(profile.initial_balance_low)),
        balance.map(price),
        Some(format!("{below} / {above}")),
        Some(profile.price_tpo_count.values().sum::<usize>().to_string()),
        // shortcut: the total prints in the default volume format; pass the instrument's volume
        // decimals to draw_statistics if a host needs fractional totals.
        Some(crate::scales::StudyFormat::Volume.format(profile.total_volume, None)),
        Some(rotation),
        profile
            .range_extension()
            .map(|(below, above)| format!("{} / {}", price(below), price(above))),
    ];
    let mut shown = *shown;
    let rows = shown
        .switches()
        .into_iter()
        .zip(values)
        .filter_map(|((_, label, shown), value)| Some((label, value.filter(|_| *shown)?)));

    let font = FontId::proportional(typography::XS);
    let galleys: Vec<_> = rows
        .into_iter()
        .map(|(label, value)| {
            let label = l10n::text(painter.ctx(), label);
            (
                painter.layout_no_wrap(label, font.clone(), chart.axis_text),
                painter.layout_no_wrap(value, font.clone(), chart.axis_text),
            )
        })
        .collect();
    if galleys.is_empty() {
        return;
    }
    let (label_width, value_width, line) = galleys.iter().fold(
        (0.0_f32, 0.0_f32, 0.0_f32),
        |(labels, values, line), (label, value)| {
            (
                labels.max(label.size().x),
                values.max(value.size().x),
                line.max(label.size().y),
            )
        },
    );
    let (padding, gap) = (Vec2::new(6.0, 4.0), 12.0);
    let size =
        Vec2::new(label_width + gap + value_width, line * galleys.len() as f32) + padding * 2.0;
    if size.x > room {
        return;
    }

    let lowest_row = coords
        .price_to_y(profile.row_span(profile.profile_low).0)
        .max(coords.price_to_y(profile.row_span(profile.profile_high).1));
    let top = (lowest_row + 4.0).min(price_rect.bottom() - size.y);
    let bounds = Rect::from_min_size(Pos2::new(anchor_x, top), size);
    let painter = painter.with_clip_rect(price_rect);
    painter.rect_filled(bounds, 4, chart.bg);
    let inner = bounds.shrink2(padding);
    for (row, (label, value)) in galleys.into_iter().enumerate() {
        let y = inner.top() + row as f32 * line;
        let value_x = inner.right() - value.size().x;
        painter.galley(Pos2::new(inner.left(), y), label, chart.axis_text);
        painter.galley(Pos2::new(value_x, y), value, chart.axis_text);
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

/// Steps in each half of the period palette.
const PERIOD_STEPS: usize = 26;

/// One step of the first or second gradient, preserving the default palette's rounding.
fn period_step(colors: &[Color32; 4], step: usize) -> Color32 {
    let (start, end, step) = if step < PERIOD_STEPS {
        (colors[0], colors[1], step)
    } else {
        (colors[2], colors[3], step - PERIOD_STEPS)
    };
    start.lerp_to_gamma(end, step as f32 / (PERIOD_STEPS - 1) as f32)
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
            let last = profile.width().saturating_sub(1).max(1);
            let steps = 2 * PERIOD_STEPS - 1;
            let position = (period_idx as f32 / last as f32).min(1.0) * steps as f32;
            let index = position as usize;
            period_step(&config.period_colors, index).lerp_to_gamma(
                period_step(&config.period_colors, (index + 1).min(steps)),
                position.fract(),
            )
        }
        TPOColorMode::Solid => config.solid_color,
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

    #[test]
    fn split_display_draws_each_period_in_its_own_column() {
        let bars: Vec<_> = [
            (100.0, 102.0),
            (101.0, 103.0),
            (100.0, 100.0),
            (102.0, 104.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (low, high))| {
            Bar::new(
                chrono::DateTime::UNIX_EPOCH + Duration::minutes(i as i64 * 30),
                low,
                high,
                low,
                high,
                1.0,
            )
        })
        .collect();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
        let coords = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(90.0, 115.0),
            0.0,
            400.0,
            false,
        );
        let mut previous = None;
        for split in [false, true] {
            let params: crate::model::ChartTypeParams = serde_json::from_value(
                serde_json::json!({"tpo_row_ticks": 1, "tpo_split_periods": split}),
            )
            .unwrap();
            let config = TPOConfig {
                display_mode: TPODisplayMode::Blocks,
                value_area_pct: 0.5,
                show_statistics: true,
                show_initial_balance: false,
                show_opening_range: false,
                ..params.tpo_config(Some(1.0))
            };
            let profile = to_tpo_profiles(&bars, &config, &Timeframe::Min30.into()).remove(0);
            // Counts 2,2,3,2,1: POC 102 and its lower neighbour reach five TPOs.
            assert_eq!(
                (
                    profile.poc_price,
                    profile.value_area_low,
                    profile.value_area_high
                ),
                (102.0, 101.0, 102.0)
            );
            let ctx = egui::Context::default();
            ctx.begin_pass(egui::RawInput::default());
            draw_session_profile(
                &ctx.layer_painter(egui::LayerId::background()),
                rect,
                &coords,
                &profile,
                &config,
                &TpoRenderConfig::default(),
                10.25,
                12.0,
                f32::INFINITY,
                0.0,
                Color32::GREEN,
                Color32::RED,
            );
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            let cells: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Rect(r) if r.rect.width() < 12.0 => {
                        let price = (100..=104)
                            .min_by(|a, b| {
                                (coords.price_to_y(*a as f64) - r.rect.center().y)
                                    .abs()
                                    .total_cmp(
                                        &(coords.price_to_y(*b as f64) - r.rect.center().y).abs(),
                                    )
                            })
                            .unwrap();
                        Some((
                            price,
                            ((r.rect.center().x - 10.25) / 12.0 - 0.5).round() as i32,
                            r.fill,
                        ))
                    }
                    _ => None,
                })
                .collect();
            let expected = if split {
                vec![
                    (104, 3),
                    (103, 1),
                    (103, 3),
                    (102, 0),
                    (102, 1),
                    (102, 3),
                    (101, 0),
                    (101, 1),
                    (100, 0),
                    (100, 2),
                ]
            } else {
                vec![
                    (104, 0),
                    (103, 0),
                    (103, 1),
                    (102, 0),
                    (102, 1),
                    (102, 2),
                    (101, 0),
                    (101, 1),
                    (100, 0),
                    (100, 1),
                ]
            };
            assert_eq!(
                cells
                    .iter()
                    .map(|&(price, col, _)| (price, col))
                    .collect::<Vec<_>>(),
                expected
            );
            let fills: Vec<_> = cells
                .iter()
                .map(|&(price, _, fill)| (price, fill))
                .collect();
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Text(t) => Some(t.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            if let Some((old_fills, old_texts)) = previous {
                assert_eq!(fills, old_fills);
                assert_eq!(texts, old_texts);
            }
            previous = Some((fills, texts));
        }
    }

    #[test]
    fn block_colours_follow_the_colour_settings_in_both_themes() {
        use crate::model::ChartTypeParams;
        let epoch = chrono::DateTime::UNIX_EPOCH;
        let bars: Vec<_> = [(0, 103.0), (30, 101.0), (60, 100.0), (1440, 100.0)]
            .into_iter()
            .map(|(minute, high)| {
                Bar::new(
                    epoch + Duration::minutes(minute),
                    100.0,
                    high,
                    100.0,
                    high,
                    1.0,
                )
            })
            .collect();
        let params = ChartTypeParams {
            tpo_row_ticks: 1,
            tpo_composites: vec![epoch..=epoch + Duration::days(1)],
            ..Default::default()
        };
        let rgb = Color32::from_rgb;
        type Row = (&'static str, ChartTypeParams, [[Color32; 8]; 2]);
        let rows: [Row; 5] = [
            (
                "default",
                params.clone(),
                [
                    [
                        rgb(96, 25, 45),
                        rgb(96, 25, 45),
                        rgb(214, 56, 100),
                        rgb(87, 191, 153),
                        rgb(214, 56, 100),
                        rgb(87, 191, 153),
                        rgb(93, 36, 245),
                        rgb(214, 56, 100),
                    ],
                    [
                        rgb(237, 165, 185),
                        rgb(237, 165, 185),
                        rgb(214, 56, 100),
                        rgb(75, 164, 131),
                        rgb(214, 56, 100),
                        rgb(75, 164, 131),
                        rgb(93, 36, 245),
                        rgb(214, 56, 100),
                    ],
                ],
            ),
            (
                "colours",
                ChartTypeParams {
                    tpo_block_colors: [
                        rgb(255, 0, 0),
                        rgb(0, 160, 0),
                        rgb(0, 160, 0),
                        rgb(128, 128, 128),
                    ],
                    ..params.clone()
                },
                [
                    [
                        rgb(115, 0, 0),
                        rgb(115, 0, 0),
                        rgb(255, 0, 0),
                        rgb(0, 160, 0),
                        rgb(255, 0, 0),
                        rgb(0, 160, 0),
                        rgb(128, 128, 128),
                        rgb(255, 0, 0),
                    ],
                    [
                        rgb(255, 140, 140),
                        rgb(255, 140, 140),
                        rgb(255, 0, 0),
                        rgb(0, 160, 0),
                        rgb(255, 0, 0),
                        rgb(0, 160, 0),
                        rgb(128, 128, 128),
                        rgb(255, 0, 0),
                    ],
                ],
            ),
            (
                "single colour",
                ChartTypeParams {
                    tpo_colored_blocks: false,
                    tpo_block_color: rgb(128, 128, 128),
                    ..params.clone()
                },
                [
                    [
                        rgb(58, 58, 58),
                        rgb(58, 58, 58),
                        rgb(128, 128, 128),
                        rgb(128, 128, 128),
                        rgb(128, 128, 128),
                        rgb(128, 128, 128),
                        rgb(128, 128, 128),
                        rgb(128, 128, 128),
                    ],
                    [
                        rgb(198, 198, 198),
                        rgb(198, 198, 198),
                        rgb(128, 128, 128),
                        rgb(128, 128, 128),
                        rgb(128, 128, 128),
                        rgb(128, 128, 128),
                        rgb(128, 128, 128),
                        rgb(128, 128, 128),
                    ],
                ],
            ),
            (
                "opaque outside",
                ChartTypeParams {
                    tpo_block_colors: [
                        rgb(255, 0, 0),
                        rgb(0, 160, 0),
                        rgb(0, 160, 0),
                        rgb(128, 128, 128),
                    ],
                    tpo_outside_value_area_opacity: 100,
                    ..params.clone()
                },
                [[
                    rgb(255, 0, 0),
                    rgb(255, 0, 0),
                    rgb(255, 0, 0),
                    rgb(0, 160, 0),
                    rgb(255, 0, 0),
                    rgb(0, 160, 0),
                    rgb(128, 128, 128),
                    rgb(255, 0, 0),
                ]; 2],
            ),
            (
                "hidden outside",
                ChartTypeParams {
                    tpo_block_colors: [
                        rgb(255, 0, 0),
                        rgb(0, 160, 0),
                        rgb(0, 160, 0),
                        rgb(128, 128, 128),
                    ],
                    tpo_outside_value_area_opacity: 0,
                    ..params
                },
                [
                    [
                        rgb(0, 0, 0),
                        rgb(0, 0, 0),
                        rgb(255, 0, 0),
                        rgb(0, 160, 0),
                        rgb(255, 0, 0),
                        rgb(0, 160, 0),
                        rgb(128, 128, 128),
                        rgb(255, 0, 0),
                    ],
                    [
                        rgb(255, 255, 255),
                        rgb(255, 255, 255),
                        rgb(255, 0, 0),
                        rgb(0, 160, 0),
                        rgb(255, 0, 0),
                        rgb(0, 160, 0),
                        rgb(128, 128, 128),
                        rgb(255, 0, 0),
                    ],
                ],
            ),
        ];
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
        let coords = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(90.0, 115.0),
            0.0,
            400.0,
            false,
        );
        for (name, params, expected) in rows {
            let config = TPOConfig {
                show_poc: false,
                show_initial_balance: false,
                show_opening_range: false,
                display_mode: TPODisplayMode::Blocks,
                ..params.tpo_config(Some(1.0))
            };
            #[cfg(feature = "studies")]
            let config = {
                use crate::studies::{Indicator, InputValue, TimePriceOpportunity};
                let mut study = TimePriceOpportunity::default();
                study.set_instrument(Some(
                    &crate::model::Instrument::new("X").with_tick_size(1.0),
                ));
                for (id, value) in [
                    ("row_ticks", InputValue::Length(1)),
                    ("display", InputValue::Choice(1)),
                    ("poc", InputValue::Toggle(false)),
                    ("initial_balance", InputValue::Toggle(false)),
                    ("open_price", InputValue::Toggle(false)),
                    (
                        "coloring",
                        InputValue::Choice(usize::from(!params.tpo_colored_blocks)),
                    ),
                    ("block_color", InputValue::Color(params.tpo_block_color)),
                    (
                        "outside_value_area_opacity",
                        InputValue::Length(params.tpo_outside_value_area_opacity as usize),
                    ),
                ] {
                    study.set_input(id, value).unwrap();
                }
                for (i, color) in params.tpo_block_colors.iter().enumerate() {
                    study
                        .set_input(
                            &format!("period_color_{}", i + 1),
                            InputValue::Color(*color),
                        )
                        .unwrap();
                }
                let mut study_config = study.tpo().unwrap().clone();
                study_config.composites = params.tpo_composites.clone();
                assert_eq!(format!("{:?}", study_config), format!("{:?}", config));
                study_config
            };
            let profile = to_tpo_profiles(&bars, &config, &Timeframe::Min30.into()).remove(0);
            // Counts 4,2,1,1 give POC 100 and a six-TPO value area covering 100 and 101.
            assert_eq!(
                (
                    profile.poc_price,
                    profile.value_area_low,
                    profile.value_area_high
                ),
                (100.0, 100.0, 101.0)
            );
            for (theme, expected) in [crate::theme::Theme::dark(), crate::theme::Theme::light()]
                .into_iter()
                .zip(expected)
            {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                ctx.begin_pass(egui::RawInput::default());
                draw_session_profile(
                    &ctx.layer_painter(egui::LayerId::background()),
                    rect,
                    &coords,
                    &profile,
                    &config,
                    &TpoRenderConfig::default(),
                    10.25,
                    12.0,
                    f32::INFINITY,
                    0.0,
                    Color32::GREEN,
                    Color32::RED,
                );
                let mut output = ctx.end_pass();
                output.textures_delta.clear();
                let fills: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Rect(r) => Some(r.fill),
                        _ => None,
                    })
                    .collect();
                assert_eq!(fills, expected, "{name}, background {:?}", theme.chart.bg);
            }
        }
    }

    #[test]
    fn poor_extremes_mark_only_rows_with_multiple_periods() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
        let map = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(90.0, 115.0),
            0.0,
            400.0,
            false,
        );
        // The two half-hour periods share both extremes, just the high, just the low, or neither.
        for (first, second, tick, switches, expected) in [
            (
                (100.0, 104.0),
                (100.0, 104.0),
                1.0,
                r#"{"tpo_poor_highs_lows":true}"#,
                vec![104.0, 100.0],
            ),
            (
                (100.0, 104.0),
                (101.0, 104.0),
                1.0,
                r#"{"tpo_poor_highs_lows":true}"#,
                vec![104.0],
            ),
            (
                (100.0, 104.0),
                (100.0, 103.0),
                1.0,
                r#"{"tpo_poor_highs_lows":true}"#,
                vec![100.0],
            ),
            (
                (100.0, 104.0),
                (101.0, 103.0),
                1.0,
                r#"{"tpo_poor_highs_lows":true}"#,
                vec![],
            ),
            ((100.0, 104.0), (100.0, 104.0), 1.0, "{}", vec![]),
            // A row of four 0.25 ticks starts at 104 and is centred at 104.375.
            (
                (100.0, 104.25),
                (101.0, 104.75),
                0.25,
                r#"{"tpo_poor_highs_lows":true}"#,
                vec![104.375],
            ),
            (
                (100.0, 104.0),
                (100.0, 104.0),
                1.0,
                r#"{"tpo_poor_high":true}"#,
                vec![104.0],
            ),
            (
                (100.0, 104.0),
                (100.0, 104.0),
                1.0,
                r#"{"tpo_poor_low":true}"#,
                vec![100.0],
            ),
        ] {
            let mut params: crate::model::ChartTypeParams = serde_json::from_str(switches).unwrap();
            params.tpo_row_ticks = (1.0 / tick) as u32;
            let config = TPOConfig {
                show_poc: false,
                show_value_area: false,
                show_opening_range: false,
                ..params.tpo_config(Some(tick))
            };
            let bars = [first, second].map(|(low, high)| {
                Bar::new(chrono::DateTime::UNIX_EPOCH, low, high, low, high, 1.0)
            });
            let mut bars = bars;
            bars[1].time += Duration::minutes(30);
            let profile = to_tpo_profiles(&bars, &config, &Timeframe::Min30.into()).remove(0);
            let ctx = egui::Context::default();
            ctx.begin_pass(egui::RawInput::default());
            draw_session_profile(
                &ctx.layer_painter(egui::LayerId::background()),
                rect,
                &map,
                &profile,
                &config,
                &TpoRenderConfig::default(),
                40.0,
                20.0,
                f32::INFINITY,
                0.0,
                Color32::GREEN,
                Color32::RED,
            );
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            let marks: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::LineSegment { points, .. } => Some(points[0].y),
                    _ => None,
                })
                .collect();
            let expected: Vec<_> = expected
                .into_iter()
                .map(|price| map.price_to_y(price))
                .collect();
            assert_eq!(marks, expected, "{second:?}, switches {switches}");
        }
    }

    #[test]
    fn poor_extremes_extend_until_a_later_bar_trades_through() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
        let map = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(90.0, 115.0),
            0.0,
            400.0,
            false,
        );
        let bars: Vec<_> = (0..12)
            .map(|i| {
                let (low, high) = match i {
                    0..=3 => (100.0, 104.0),
                    9 => (102.0, 104.5),
                    _ => (101.0, 103.0),
                };
                Bar::new(
                    chrono::DateTime::UNIX_EPOCH
                        + Duration::days(i / 4)
                        + Duration::minutes(i % 4 * 30),
                    low,
                    high,
                    low,
                    high,
                    1.0,
                )
            })
            .collect();
        for (ticks, extend, high, low, start, latest, expected) in [
            (1, false, true, true, 0, false, vec![]),
            (
                1,
                true,
                true,
                true,
                0,
                false,
                vec![
                    (104.0, 270.0),
                    (100.0, 400.0),
                    (103.0, 270.0),
                    (101.0, 400.0),
                ],
            ),
            (
                1,
                true,
                true,
                false,
                0,
                false,
                vec![(104.0, 270.0), (103.0, 270.0)],
            ),
            // The first profile is offscreen; its high still ends at bar 9 and its low at the edge.
            (
                1,
                true,
                true,
                true,
                4,
                false,
                vec![
                    (103.0, 150.0),
                    (101.0, 400.0),
                    (104.0, 150.0),
                    (100.0, 400.0),
                ],
            ),
            (
                1,
                true,
                true,
                true,
                8,
                false,
                vec![(104.0, 30.0), (100.0, 400.0), (103.0, 30.0), (101.0, 400.0)],
            ),
            // Both highs were repaired before this crop; the lows remain unrepaired.
            (
                1,
                true,
                true,
                true,
                10,
                false,
                vec![(100.0, 400.0), (101.0, 400.0)],
            ),
            (1, true, true, true, 8, true, vec![]),
            // Rows of two ticks: each line keeps its row's centre once its profile is offscreen.
            (
                2,
                true,
                true,
                true,
                0,
                false,
                vec![
                    (104.5, 270.0),
                    (100.5, 400.0),
                    (102.5, 270.0),
                    (100.5, 400.0),
                ],
            ),
            (
                2,
                true,
                true,
                false,
                0,
                false,
                vec![(104.5, 270.0), (102.5, 270.0)],
            ),
            (
                2,
                true,
                true,
                true,
                4,
                false,
                vec![
                    (102.5, 150.0),
                    (100.5, 400.0),
                    (104.5, 150.0),
                    (100.5, 400.0),
                ],
            ),
            (
                2,
                true,
                true,
                true,
                8,
                false,
                vec![(104.5, 30.0), (100.5, 400.0), (102.5, 30.0), (100.5, 400.0)],
            ),
            (
                2,
                true,
                true,
                true,
                10,
                false,
                vec![(100.5, 400.0), (100.5, 400.0)],
            ),
        ] {
            let params: crate::model::ChartTypeParams = serde_json::from_value(serde_json::json!({"tpo_row_ticks":ticks,"tpo_extend_poor":extend,"tpo_poor_high":high,"tpo_poor_low":low})).unwrap();
            let mut config = params.tpo_config(Some(1.0));
            config.latest_session_only = latest;
            let ctx = egui::Context::default();
            ctx.begin_pass(egui::RawInput::default());
            render_live_tpo(
                &ctx.layer_painter(egui::LayerId::background()),
                rect,
                &bars,
                start..bars.len(),
                0,
                &map,
                &config,
                Some(1.0),
                &Timeframe::Min30.into(),
                |i, _| (i as f32 - start as f32) * 30.0,
                0.0,
                Color32::GREEN,
                Color32::RED,
            );
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            let lines: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::LineSegment { points, stroke }
                        if stroke.width == DESIGN_TOKENS.stroke.light
                            && points[0].y == points[1].y
                            && points[1].x - points[0].x > 12.0 =>
                    {
                        Some((points[0].y, points[1].x))
                    }
                    _ => None,
                })
                .collect();
            let expected: Vec<_> = expected
                .into_iter()
                .map(|(price, end)| (map.price_to_y(price), end))
                .collect();
            assert_eq!(
                lines, expected,
                "ticks {ticks}, extend {extend}, high {high}, low {low}, start {start}, latest {latest}"
            );
        }
    }
    #[test]
    fn initial_balance_extensions_use_one_and_two_ranges_and_require_balance_bars() {
        use crate::model::{Session, SessionWindow, TimezoneId};
        use chrono::{NaiveTime, Weekday};
        let clock = BarClock::new(Timeframe::Min30).with_session(
            TimezoneId::fixed_offset(0).unwrap(),
            Session::new([SessionWindow::new(
                [Weekday::Thu],
                NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
            )]),
            false,
        );
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
        for (enabled, minutes, high, bar_minute, marker, expected) in [
            (true, 60, 104.0, 0, true, vec![96.0, 108.0, 92.0, 112.0]),
            (false, 60, 104.0, 0, true, vec![]),
            (true, 0, 104.0, 0, true, vec![]),
            // History starts 90 minutes after the session opens, outside the 60-minute balance.
            (true, 60, 104.0, 90, true, vec![]),
            (true, 60, 100.0, 0, true, vec![]),
            (true, 60, 104.0, 0, false, vec![96.0, 108.0, 92.0, 112.0]),
        ] {
            let params: crate::model::ChartTypeParams = serde_json::from_value(serde_json::json!({
                "tpo_initial_balance_minutes": minutes,
                "tpo_initial_balance": marker,
                "tpo_initial_balance_extensions": enabled,
                "tpo_row_ticks": 1
            }))
            .unwrap();
            let config = TPOConfig {
                show_poc: false,
                show_value_area: false,
                show_opening_range: false,
                ..params.tpo_config(Some(1.0))
            };
            let bar = Bar::new(
                chrono::DateTime::UNIX_EPOCH + Duration::minutes(bar_minute),
                100.0,
                high,
                100.0,
                high,
                1.0,
            );
            let profile = to_tpo_profiles(&[bar], &config, &clock).remove(0);
            for inverted in [false, true] {
                let map = PriceMap::new(
                    crate::scales::PriceTransform::default(),
                    crate::scales::PriceRange::new(90.0, 115.0),
                    0.0,
                    400.0,
                    inverted,
                );
                let ctx = egui::Context::default();
                ctx.begin_pass(egui::RawInput::default());
                draw_session_profile(
                    &ctx.layer_painter(egui::LayerId::background()),
                    rect,
                    &map,
                    &profile,
                    &config,
                    &TpoRenderConfig::default(),
                    40.0,
                    20.0,
                    f32::INFINITY,
                    0.0,
                    Color32::GREEN,
                    Color32::RED,
                );
                let mut output = ctx.end_pass();
                output.textures_delta.clear();
                let balance_color = output.shapes.iter().find_map(|shape| match &shape.shape {
                    Shape::Rect(r) if r.rect.width() == 2.0 => Some(r.fill),
                    _ => None,
                });
                if !marker {
                    assert!(balance_color.is_none());
                }
                let lines: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::LineSegment { points, stroke } => {
                            if let Some(color) = balance_color {
                                assert_eq!(stroke.color, color);
                            }
                            assert!(points[0].x < points[1].x && points[1].x <= 40.0);
                            assert_eq!(points[0].y, points[1].y);
                            Some(points[0].y)
                        }
                        _ => None,
                    })
                    .collect();
                let expected: Vec<_> = expected
                    .iter()
                    .map(|&price| map.price_to_y(price))
                    .collect();
                assert_eq!(
                    lines, expected,
                    "enabled {enabled}, minutes {minutes}, high {high}"
                );
            }
        }
    }

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
                for period in 0..52 {
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
    fn volume_profiles_share_bar_volume_across_rows_and_follow_opacity() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
        let map = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(90.0, 115.0),
            0.0,
            400.0,
            false,
        );
        let epoch = chrono::DateTime::UNIX_EPOCH;
        let bars = [
            Bar::new(epoch, 100.0, 102.0, 100.0, 101.0, 30.0),
            Bar::new(
                epoch + Duration::minutes(30),
                101.0,
                102.0,
                101.0,
                102.0,
                10.0,
            ),
        ];
        for (revision, (enabled, volume, composite, opacity, expected)) in [
            (false, 30.0, false, 45, vec![]),
            (
                true,
                30.0,
                false,
                45,
                vec![(102.0, 40.0), (101.0, 40.0), (100.0, 80.0 / 3.0)],
            ),
            (
                true,
                f64::NAN,
                false,
                45,
                vec![(102.0, 40.0), (101.0, 40.0)],
            ),
            (true, 0.0, false, 45, vec![(102.0, 40.0), (101.0, 40.0)]),
            (
                true,
                f64::INFINITY,
                false,
                45,
                vec![(102.0, 40.0), (101.0, 40.0)],
            ),
            (
                true,
                30.0,
                true,
                45,
                vec![(102.0, 40.0), (101.0, 40.0), (100.0, 80.0 / 3.0)],
            ),
            (true, -1.0, false, 45, vec![(102.0, 40.0), (101.0, 40.0)]),
            (
                true,
                30.0,
                false,
                100,
                vec![(102.0, 40.0), (101.0, 40.0), (100.0, 80.0 / 3.0)],
            ),
            (
                true,
                30.0,
                false,
                0,
                vec![(102.0, 40.0), (101.0, 40.0), (100.0, 80.0 / 3.0)],
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let mut bars = bars.clone();
            bars[0].volume = volume;
            if composite {
                bars[1].time = epoch + Duration::days(1);
            }
            let params: crate::model::ChartTypeParams = serde_json::from_value(serde_json::json!({
                "tpo_volume_profile": enabled,
                "tpo_row_ticks": 1,
                "tpo_outside_value_area_opacity": opacity,
            }))
            .unwrap();
            let config = TPOConfig {
                show_poc: false,
                show_initial_balance: false,
                show_opening_range: false,
                display_mode: TPODisplayMode::Blocks,
                composites: if composite {
                    vec![epoch..=epoch + Duration::days(1)]
                } else {
                    vec![]
                },
                ..params.tpo_config(Some(1.0))
            };
            let ctx = egui::Context::default();
            ctx.begin_pass(egui::RawInput::default());
            // Reuse the cache under one revision while the volume switch changes.
            let clock: BarClock = Timeframe::Min30.into();
            let units = cached_profiles(
                &bars,
                revision.saturating_sub(1) as u64,
                0,
                &config,
                &clock,
                &clock.sessions(),
            );
            let profile = &units[0].2[0];
            let volumes = if !enabled {
                [0.0; 3]
            } else if volume == 30.0 {
                [10.0, 15.0, 15.0]
            } else {
                [0.0, 5.0, 5.0]
            };
            for (price, expected) in [100.0, 101.0, 102.0].into_iter().zip(volumes) {
                assert_eq!(profile.volume_at(price), expected);
            }
            draw_session_profile(
                &ctx.layer_painter(egui::LayerId::background()),
                rect,
                &map,
                profile,
                &config,
                &TpoRenderConfig::default(),
                40.0,
                20.0,
                f32::INFINITY,
                0.0,
                Color32::GREEN,
                Color32::RED,
            );
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            let rows: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Rect(r) if r.rect.left() == 86.0 => Some(r),
                    _ => None,
                })
                .collect();
            assert_eq!(
                rows.len(),
                expected.len(),
                "enabled {enabled}, volume {volume}"
            );
            for (r, (price, width)) in rows.iter().zip(expected) {
                assert!((r.rect.width() - width).abs() < 0.001);
                // A 16 px row leaves one gap pixel; rounding its edges gives y-7 through y+8.
                assert_eq!(
                    (r.rect.top(), r.rect.bottom()),
                    (map.price_to_y(price) - 7.0, map.price_to_y(price) + 8.0)
                );
                let chart = ctx.active_theme().chart;
                let color = if price == 100.0 {
                    chart
                        .bg
                        .lerp_to_gamma(chart.axis_text_secondary, opacity as f32 / 100.0)
                } else {
                    chart.axis_text_secondary
                };
                assert_eq!(r.fill, color);
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
                                f32::INFINITY,
                                0.0,
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
                f32::INFINITY,
                LABEL_ROOM_PX,
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
            f32::INFINITY,
            0.0,
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

    /// Each caption follows its own switch and visible level, including coinciding levels and
    /// instrument-precision prices. Counts in F1 are 2,2,2,2,1,1,3,3,3,3,2 (24 TPOs):
    /// POC 106; the 17-TPO target grows up to 110, then down to 103 (18 TPOs).
    #[test]
    fn level_captions_show_exactly_their_switches_while_levels_are_drawn() {
        use crate::model::TPOLabels;
        let fixture = |prices: &[(f64, f64, f64, f64)]| {
            prices
                .iter()
                .enumerate()
                .map(|(i, &(o, h, l, c))| {
                    Bar::new(
                        chrono::DateTime::UNIX_EPOCH + Duration::minutes(30 * i as i64),
                        o,
                        h,
                        l,
                        c,
                        1.0,
                    )
                })
                .collect::<Vec<_>>()
        };
        let f1 = fixture(&[
            (107.0, 110.0, 106.0, 108.0),
            (108.0, 110.0, 105.0, 106.0),
            (104.0, 104.0, 100.0, 101.0),
            (101.0, 103.0, 100.0, 102.0),
            (107.0, 109.0, 106.0, 108.0),
        ]);
        let f2 = fixture(&[(100.5, 101.25, 100.0, 101.0), (101.0, 101.0, 100.25, 100.5)]);
        let names = [
            "POC", "VAH", "VAL", "Mid", "IBH", "IBL", "Open", "Close", "PH", "PL", "SP",
        ];
        let labels = |on: &[&str], prices| TPOLabels {
            poc: on.contains(&"POC"),
            value_area_high: on.contains(&"VAH"),
            value_area_low: on.contains(&"VAL"),
            midpoint: on.contains(&"Mid"),
            initial_balance_high: on.contains(&"IBH"),
            initial_balance_low: on.contains(&"IBL"),
            opening_price: on.contains(&"Open"),
            closing_price: on.contains(&"Close"),
            poor_high: on.contains(&"PH"),
            poor_low: on.contains(&"PL"),
            single_prints: on.contains(&"SP"),
            prices,
        };
        let config = TPOConfig {
            show_midpoint: true,
            show_closing_price: true,
            show_poor_high: true,
            show_poor_low: true,
            show_single_prints: true,
            ..Default::default()
        };
        let with_labels = |on: &[&str], prices| TPOConfig {
            labels: labels(on, prices),
            ..config.clone()
        };
        let mut rows = vec![(
            "defaults",
            &f1,
            config.clone(),
            vec!["POC", "VAH", "VAL", "Mid"],
        )];
        for name in names {
            rows.push((name, &f1, with_labels(&[name], false), vec![name]));
        }
        rows.extend([
            ("all", &f1, with_labels(&names, false), names.to_vec()),
            (
                "all prices",
                &f1,
                with_labels(&names, true),
                vec![
                    "POC 106",
                    "VAH 110",
                    "VAL 103",
                    "Mid 105.0",
                    "IBH 110",
                    "IBL 105",
                    "Open 107",
                    "Close 108",
                    "PH 110",
                    "PL 100",
                    "SP 104 / 105",
                ],
            ),
            (
                "balance hidden",
                &f1,
                TPOConfig {
                    show_initial_balance: false,
                    ..with_labels(&["IBH", "IBL"], false)
                },
                vec![],
            ),
            (
                "poor high hidden",
                &f1,
                TPOConfig {
                    show_poor_high: false,
                    ..with_labels(&["PH", "PL"], false)
                },
                vec!["PL"],
            ),
            (
                "single prints hidden",
                &f1,
                TPOConfig {
                    show_single_prints: false,
                    ..with_labels(&["SP"], false)
                },
                vec![],
            ),
            (
                "opening hidden",
                &f1,
                TPOConfig {
                    show_opening_range: false,
                    ..with_labels(&["Open"], false)
                },
                vec![],
            ),
            (
                "closing hidden",
                &f1,
                TPOConfig {
                    show_closing_price: false,
                    ..with_labels(&["Close"], false)
                },
                vec![],
            ),
            (
                "coinciding VAH and IBH",
                &f1,
                with_labels(&["VAH", "IBH"], false),
                vec!["VAH", "IBH"],
            ),
            (
                "quarter tick IBH",
                &f2,
                TPOConfig {
                    tick_size: 0.25,
                    ..with_labels(&["IBH"], true)
                },
                vec!["IBH 101.25"],
            ),
            (
                "quarter tick IBH on four-tick rows",
                &f2,
                TPOConfig {
                    tick_size: 1.0,
                    instrument_tick: Some(0.25),
                    ..with_labels(&["IBH"], true)
                },
                vec!["IBH 101.25"],
            ),
            (
                "half tick midpoint",
                &f2,
                TPOConfig {
                    tick_size: 0.25,
                    ..with_labels(&["Mid"], true)
                },
                vec!["Mid 100.625"],
            ),
            (
                "balance absent",
                &f1,
                TPOConfig {
                    initial_balance_minutes: 0,
                    ..with_labels(&["IBH", "IBL"], false)
                },
                vec![],
            ),
            (
                "POC hidden",
                &f1,
                TPOConfig {
                    show_poc: false,
                    ..with_labels(&["POC"], false)
                },
                vec![],
            ),
            (
                "value area hidden",
                &f1,
                TPOConfig {
                    show_value_area: false,
                    ..with_labels(&["VAH", "VAL"], false)
                },
                vec![],
            ),
            (
                "midpoint hidden",
                &f1,
                TPOConfig {
                    show_midpoint: false,
                    ..with_labels(&["Mid"], false)
                },
                vec![],
            ),
        ]);
        let coords = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(95.0, 115.0),
            0.0,
            400.0,
            false,
        );
        let mut failures = Vec::new();
        for (width, priced_captions_fit) in [(400.0, true), (80.0, false), (101.0, false)] {
            let rect = Rect::from_x_y_ranges(0.0..=width, 0.0..=400.0);
            for (case, bars, config, expected) in &rows {
                // Priced gutters need 100 px for F1 and at least 82 px for F2, plus profile room.
                let mut expected = if !priced_captions_fit && config.labels.prices {
                    vec![]
                } else {
                    expected.clone()
                };
                let ctx = egui::Context::default();
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(rect),
                        ..Default::default()
                    },
                    |ui| {
                        render_live_tpo(
                            ui.painter(),
                            rect,
                            bars,
                            0..bars.len(),
                            1,
                            &coords,
                            config,
                            config.instrument_tick,
                            &BarClock::new(Timeframe::Min30),
                            |i, _| i as f32 * 30.0,
                            0.0,
                            Color32::GREEN,
                            Color32::RED,
                        );
                    },
                );
                output.textures_delta.clear();
                let captions: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(t)
                            if names.contains(&t.galley.text().split(' ').next().unwrap()) =>
                        {
                            Some((
                                t.galley.text().to_owned(),
                                Rect::from_min_size(t.pos, t.galley.size()),
                            ))
                        }
                        _ => None,
                    })
                    .collect();
                let mut texts: Vec<_> = captions.iter().map(|(text, _)| text.as_str()).collect();
                texts.sort_unstable();
                expected.sort_unstable();
                let contained = captions.iter().all(|(_, r)| rect.contains_rect(*r));
                let separate = captions
                    .iter()
                    .enumerate()
                    .all(|(i, (_, r))| captions[..i].iter().all(|(_, prev)| !r.intersects(*prev)));
                let profile_in_view = output.shapes.iter().any(|shape| match &shape.shape {
                    Shape::Text(t)
                        if t.galley.text().len() == 1
                            && t.galley.text().chars().all(|c| c.is_ascii_uppercase()) =>
                    {
                        rect.contains(Rect::from_min_size(t.pos, t.galley.size()).center())
                    }
                    Shape::Rect(r) if r.rect.width() > 2.0 => rect.contains(r.rect.center()),
                    _ => false,
                });
                if texts != expected || !contained || !separate || !profile_in_view {
                    failures.push(format!("{case}, {width}px: expected {expected:?}, got {texts:?}, contained={contained}, separate={separate}, profile_in_view={profile_in_view}"));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn level_labels_and_extremes_fit_between_sessions() {
        for (high, tick, prices, readouts) in [
            (
                101.0,
                1.0,
                [99.0, 101.0, 99.0, 100.0],
                ["POC 99", "VAH 101", "VAL 99", "Mid 100.0"],
            ),
            (
                101.25,
                0.25,
                [99.0, 100.5, 99.0, 100.125],
                ["POC 99.00", "VAH 100.50", "VAL 99.00", "Mid 100.125"],
            ),
        ] {
            // Three UTC days of four half-hour bars each
            let bars: Vec<_> = (0..12)
                .map(|i| {
                    let minute = i / 4 * 1440 + i % 4 * 30;
                    let time = chrono::DateTime::UNIX_EPOCH + Duration::minutes(minute);
                    Bar::new(time, 100.0, high, 99.0, 100.0, 1.0)
                })
                .collect();
            let config = TPOConfig {
                tick_size: tick,
                show_midpoint: true,
                ..Default::default()
            };
            let rect = Rect::from_x_y_ranges(0.0..=400.0, 0.0..=400.0);
            // Labels include the first session, whether its opening bar is visible or offscreen.
            for (spacing, expected, extremes, offset, end, range, inverted) in [
                (30.0, 3, false, 0.0, 12, (90.0, 115.0), false),
                (10.0, 0, false, 0.0, 12, (90.0, 115.0), false),
                (30.0, 3, true, 0.0, 12, (90.0, 115.0), false),
                (10.0, 0, true, 0.0, 12, (90.0, 115.0), false),
                (10.0, 0, true, -4.0, 12, (90.0, 115.0), false),
                (30.0, 3, true, -30.0, 12, (90.0, 115.0), false),
                (30.0, 1, false, 0.0, 4, (90.0, 115.0), false),
                (30.0, 1, false, -90.0, 4, (90.0, 115.0), false),
                // Nearby levels at either vertical edge must all keep a caption.
                (30.0, 1, false, 0.0, 4, (99.0, 129.0), false),
                (30.0, 1, false, 0.0, 4, (99.0, 129.0), true),
            ] {
                let coords = PriceMap::new(
                    crate::scales::PriceTransform::default(),
                    crate::scales::PriceRange::new(range.0, range.1),
                    rect.min.y,
                    rect.max.y,
                    inverted,
                );
                let config = TPOConfig {
                    show_poor_high: extremes,
                    show_poor_low: extremes,
                    show_initial_balance_extensions: extremes,
                    display_mode: TPODisplayMode::Blocks,
                    ..config.clone()
                };
                let ctx = egui::Context::default();
                let draw = |events| {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(rect),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            ui.allocate_rect(rect, egui::Sense::hover());
                            render_live_tpo(
                                ui.painter(),
                                rect,
                                &bars[..end],
                                0..end,
                                1,
                                &coords,
                                &config,
                                None,
                                &BarClock::new(Timeframe::Min30),
                                |i, _| i as f32 * spacing + offset,
                                0.0,
                                Color32::GREEN,
                                Color32::RED,
                            )
                        },
                    );
                    output.textures_delta.clear();
                    output
                };
                let output = draw(vec![]);
                let labelled = output
                    .shapes
                    .iter()
                    .filter(|s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == "VAH"))
                    .count();
                assert_eq!(labelled, expected, "{spacing} px per bar");
                let captions: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::Text(t)
                            if ["POC", "VAH", "VAL", "Mid"].contains(&t.galley.text()) =>
                        {
                            Some(Rect::from_min_size(t.pos, t.galley.size()))
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(captions.len(), expected * 4);
                for session in captions.chunks(4) {
                    for (i, caption) in session.iter().enumerate() {
                        assert!(rect.contains_rect(*caption), "{caption:?}");
                        assert!(
                            session[..i].iter().all(|other| !other.intersects(*caption)),
                            "{session:?}"
                        );
                    }
                }
                // Equal row counts put POC = VAL at 99. The 70% area needs all three
                // unit rows or seven of ten quarter-point rows. Mid is (99 + high) / 2.
                let markers: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::Path(p)
                            if p.points.len() == 3
                                && p.points[0].x == p.points[2].x
                                && p.points[1].x > p.points[0].x =>
                        {
                            Some(p.points[1].y)
                        }
                        _ => None,
                    })
                    .collect();
                let expected_y: Vec<_> = (0..expected)
                    .flat_map(|_| prices)
                    .map(|price| coords.price_to_y(price))
                    .collect();
                assert_eq!(markers, expected_y);
                let mut extensions = 0;
                let mut poor = 0;
                for shape in &output.shapes {
                    let Shape::LineSegment { points, .. } = &shape.shape else {
                        continue;
                    };
                    let width = points[1].x - points[0].x;
                    if width == 10.0 {
                        assert!(points.iter().all(|p| rect.contains(*p)), "{points:?}");
                        extensions += 1;
                    } else if width == 8.0 {
                        let session = poor / 2;
                        if session < end / 4 - 1 {
                            let next_balance = (session + 1) as f32 * 4.0 * spacing + offset - 10.0;
                            assert!(points[1].x <= next_balance, "{points:?}");
                        }
                        poor += 1;
                    }
                }
                assert_eq!(extensions, if extremes { end } else { 0 });
                assert_eq!(poor, if extremes { end / 2 } else { 0 });
                for (caption, readout) in captions.iter().take(4).zip(readouts) {
                    draw(vec![egui::Event::PointerMoved(caption.center())]);
                    let hovered = draw(vec![egui::Event::PointerMoved(caption.center())]);
                    assert!(
                        hovered.shapes.iter().any(|s| matches!(&s.shape,
                    Shape::Text(t) if t.galley.text() == readout)),
                        "missing {readout}"
                    );
                }
            }
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
        for (range, latest_session_only, profile_period, expected) in [
            (
                Some((480, 1440)),
                false,
                TPOProfilePeriod::Session,
                vec![3, 13, 6, 13, 3],
            ),
            (
                Some((90, 1530)),
                false,
                TPOProfilePeriod::Session,
                vec![3, 32, 3],
            ),
            (None, true, TPOProfilePeriod::Session, vec![3, 13, 3]),
            (
                Some((480, 1440)),
                true,
                TPOProfilePeriod::Session,
                vec![6, 13, 3],
            ),
            (
                Some((90, 1530)),
                true,
                TPOProfilePeriod::Session,
                vec![32, 3],
            ),
            (None, false, TPOProfilePeriod::Week, vec![38]),
            (None, true, TPOProfilePeriod::Week, vec![38]),
        ] {
            let config = TPOConfig {
                split_sessions: true,
                profile_period,
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
            assert_eq!(
                span,
                if range.is_none() && profile_period == TPOProfilePeriod::Session {
                    19..38
                } else {
                    0..38
                }
            );
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
                |profile, _, _, _, _| counts.push(profile.tpo_count_at(100.0)),
            );
            assert_eq!(counts, expected, "{range:?}, latest={latest_session_only}");
        }
    }

    /// Cache units retain overnight windows across calendar boundaries, including windows
    /// sharing an opening bar; neither a midnight cut nor a zero-width placement loses a part.
    #[test]
    fn cached_custom_profiles_keep_overnight_and_shared_opening_bars() {
        use crate::model::{
            CustomProfileSession, CustomProfileSessions, ProfileSession, SessionOverlap,
        };
        let at = |hour| chrono::NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
        let bars: Vec<_> = (0..72)
            .map(|i| Bar::from_value(chrono::DateTime::UNIX_EPOCH + Duration::hours(i), 100.0))
            .collect();
        let clock = BarClock::new(Timeframe::Hour1);
        for (second_start, overlap, profile_period, expected) in [
            (
                23,
                SessionOverlap::InBoth,
                TPOProfilePeriod::Session,
                vec![4, 2, 4, 2],
            ),
            (
                23,
                SessionOverlap::OwnProfile,
                TPOProfilePeriod::Session,
                vec![2, 1, 1, 2],
            ),
            (
                22,
                SessionOverlap::InBoth,
                TPOProfilePeriod::Session,
                vec![4, 3, 4, 3],
            ),
            (
                23,
                SessionOverlap::InBoth,
                TPOProfilePeriod::Week,
                vec![10, 5],
            ),
        ] {
            // Sunday through Tuesday: Sunday night's window belongs to Monday's ISO week.
            let bars: Vec<_> = bars
                .iter()
                .cloned()
                .map(|mut bar| {
                    if profile_period == TPOProfilePeriod::Week {
                        bar.time += Duration::days(3);
                    }
                    bar
                })
                .collect();
            let mut custom = CustomProfileSessions::new([
                CustomProfileSession::new(at(22), at(2)),
                CustomProfileSession::new(at(second_start), at(1)),
            ]);
            custom.overlap = overlap;
            let config = TPOConfig {
                session: ProfileSession::Custom(custom),
                profile_period,
                period_minutes: 60,
                ..Default::default()
            };
            let span = whole_sessions(&bars, 27..28, &clock.sessions(), &config);
            assert_eq!(
                span,
                if overlap == SessionOverlap::OwnProfile {
                    23..49
                } else if profile_period == TPOProfilePeriod::Session {
                    22..50
                } else {
                    22..72
                }
            );
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
                |profile, _, _, _, _| counts.push(profile.tpo_count_at(100.0)),
            );
            assert_eq!(counts, expected, "{overlap:?} second start {second_start}");
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
            ("display", [true, true], [6, 3], 'A', 1.0),
            ("labels", [true, true], [6, 3], 'A', 1.0),
            ("forming bar", [true, false], [6, 4], 'A', 1.0),
            ("revision", [false, false], [6, 4], 'A', 1.0),
            ("older correction", [false, false], [7, 4], 'A', 1.0),
            ("start index", [false, false], [7, 4], 'A', 1.0),
            ("append", [true, false], [7, 4], 'A', 1.0),
            ("last bits", [true, false], [7, 4], 'A', 1.0),
            ("last time", [true, false], [7, 8], 'A', 1.0),
            ("last fraction", [true, false], [7, 8], 'A', 1.0),
            ("steady NaN", [true, true], [7, 8], 'A', 1.0),
            ("period", [false, false], [4, 4], 'A', 1.0),
            ("letters", [false, false], [4, 4], 'x', 1.0),
            ("grid", [false, false], [3, 3], 'x', 2.0),
            ("global span", [false, false], [2, 502], 'x', 4.0),
            ("zone", [false, false], [2, 502], 'x', 4.0),
            // Without trading hours, regular hours keep each whole day.
            ("hours", [false, false], [2, 502], 'x', 4.0),
        ];
        for (change, reused, counts, first_letter, tick) in cases {
            match change {
                "steady" | "steady NaN" => {}
                "display" => config.display_mode = TPODisplayMode::Blocks,
                "labels" => {
                    config.labels.poc = false;
                    config.labels.initial_balance_high = true;
                    config.labels.prices = true;
                }
                "forming bar" => bars[2].high = 103.0,
                "revision" => revision += 1,
                "older correction" => {
                    bars[0].high = 103.0;
                    revision += 1;
                }
                "start index" => start_idx += 1,
                "append" => bars.push(bars.last().unwrap().clone()),
                "last time" => bars.last_mut().unwrap().time += chrono::Duration::minutes(30),
                "last fraction" => {
                    bars.last_mut().unwrap().time += chrono::Duration::nanoseconds(1);
                }
                "last bits" => bars.last_mut().unwrap().volume = f64::NAN,
                "period" => config.period_minutes = 60,
                "letters" => config.custom_letters = Some("xy".into()),
                "grid" => config.tick_size = 2.0,
                "global span" => bars.last_mut().unwrap().high = 2103.0,
                "zone" => clock = clock.with_timezone("+09:00".parse().unwrap()),
                "hours" => config.session = crate::model::ProfileSession::Regular,
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
            assert_eq!(
                Rc::ptr_eq(&current, &previous),
                matches!(change, "steady" | "display" | "labels" | "steady NaN"),
                "session lookup reuse: {change}"
            );
            assert!(Rc::ptr_eq(
                &current,
                &cached_profiles(
                    &bars,
                    revision,
                    start_idx,
                    &config,
                    &clock,
                    &sessions(&clock),
                ),
            ));
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

    /// CIE76 difference of two opaque sRGB colours (D65 white), from the published definitions.
    fn delta_e76(a: Color32, b: Color32) -> f64 {
        fn lab(c: Color32) -> [f64; 3] {
            let [r, g, b] = [c.r(), c.g(), c.b()].map(|v| {
                let v = f64::from(v) / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            let f = |t: f64| {
                if t > 216.0 / 24389.0 {
                    t.cbrt()
                } else {
                    (24389.0 / 27.0 * t + 16.0) / 116.0
                }
            };
            let x = f((0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047);
            let y = f(0.2126729 * r + 0.7151522 * g + 0.0721750 * b);
            let z = f((0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883);
            [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
        }
        let (a, b) = (lab(a), lab(b));
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    /// Three half-hour periods on rows of 1: A 98 to 105, B 101 to 107, C 104 to 108, so rows
    /// 98, 99, 100 and 108 hold one TPO each.
    fn three_overlapping_periods() -> Vec<Bar> {
        let start = chrono::DateTime::parse_from_rfc3339("2024-01-02T14:30:00Z")
            .unwrap()
            .to_utc();
        [(98.0, 105.0), (101.0, 107.0), (104.0, 108.0)]
            .into_iter()
            .enumerate()
            .map(|(i, (low, high))| {
                Bar::new(
                    start + Duration::minutes(30 * i as i64),
                    low,
                    high,
                    low,
                    high,
                    1.0,
                )
            })
            .collect()
    }

    #[test]
    fn single_print_highlights_show_on_blocks() {
        let model = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let profile = to_tpo_profiles(
            &three_overlapping_periods(),
            &model,
            &Timeframe::Min30.into(),
        )
        .remove(0);
        assert_eq!(profile.single_prints, vec![98.0, 99.0, 100.0, 108.0]);
        let render_config = TpoRenderConfig::default();
        let rect = Rect::from_x_y_ranges(0.0..=400.0, 0.0..=400.0);
        // 16 px a point: rows of 16 px, below the 24 px up to which a block fills its row.
        let coords = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(90.0, 115.0),
            rect.min.y,
            rect.max.y,
            false,
        );
        let fills = |theme: &crate::theme::Theme,
                     profile: &TPOProfile,
                     display_mode,
                     show_single_prints| {
            let ctx = egui::Context::default();
            let _theme = crate::theme::ThemeScope::new(&ctx, theme);
            ctx.begin_pass(egui::RawInput::default());
            let painter = ctx.layer_painter(egui::LayerId::background());
            let config = TPOConfig {
                display_mode,
                show_single_prints,
                ..model.clone()
            };
            draw_session_profile(
                &painter,
                rect,
                &coords,
                profile,
                &config,
                &render_config,
                10.0,
                20.0,
                f32::INFINITY,
                LABEL_ROOM_PX,
                Color32::GREEN,
                Color32::RED,
            );
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Rect(r) => Some((r.rect, r.fill)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        // The colour a point shows: the last opaque fill there and those painted over it, on the
        // chart background.
        let shown = |fills: &[(Rect, Color32)], p: Pos2, bg: Color32| {
            fills
                .iter()
                .filter(|(r, _)| r.contains(p))
                .fold(bg, |under, (_, fill)| under.blend(*fill))
        };
        // Each single print's cell is one 16 px column of one 16 px row.
        let half_cells = 4 * 16 * 16 / 2;
        let one_period = to_tpo_profiles(
            &three_overlapping_periods()[..1],
            &model,
            &Timeframe::Min30.into(),
        )
        .remove(0);
        assert!(one_period.single_prints.contains(&one_period.poc_price));
        let poc = Pos2::new(14.0, coords.price_to_y(one_period.poc_price));
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let bg = theme.chart.bg;
            for display_mode in [TPODisplayMode::Blocks, TPODisplayMode::Both] {
                let on = fills(&theme, &profile, display_mode, true);
                let off = fills(&theme, &profile, display_mode, false);
                let mut changed = 0;
                for p in (0..400)
                    .flat_map(|y| (0..120).map(move |x| Pos2::new(x as f32 + 0.5, y as f32 + 0.5)))
                {
                    let (marked, plain) = (shown(&on, p, bg), shown(&off, p, bg));
                    if marked != plain {
                        changed += 1;
                        let difference = delta_e76(marked, plain);
                        assert!(
                            difference >= 5.0,
                            "{display_mode:?} on {bg:?} at {p:?}: {plain:?} to {marked:?} is only \
                             {difference:.1} dE76"
                        );
                    }
                }
                assert!(
                    changed >= half_cells,
                    "{display_mode:?}: 4 single prints changed {changed} px, expected {half_cells}"
                );

                // One period makes the POC row a single print too; it must still show the POC.
                assert_eq!(
                    shown(&fills(&theme, &one_period, display_mode, true), poc, bg),
                    shown(&fills(&theme, &one_period, display_mode, false), poc, bg),
                    "{display_mode:?} on {bg:?}: the POC row of a one-period profile"
                );
            }
        }
    }

    #[test]
    fn single_print_tooltips_name_the_row_price() {
        // (instrument tick, row height, two half-hour bars as (low, high), price range, single
        // prints by hand)
        type Case<'a> = (f64, f64, [(f64, f64); 2], (f64, f64), &'a [&'a str]);
        let cases: [Case; 3] = [
            // Rows of one 0.25 tick: only A reaches 110.75.
            (
                0.25,
                0.25,
                [(110.0, 110.75), (110.0, 110.5)],
                (109.0, 112.0),
                &["110.75"],
            ),
            // Rows of ten 0.25 ticks from multiples of 2.5: A spans rows 100, 102.5 and 105,
            // B only row 100.
            (
                0.25,
                2.5,
                [(100.0, 105.0), (100.0, 102.0)],
                (95.0, 110.0),
                &["102.50", "105.00"],
            ),
            // Rows of one 1/64 tick: only A reaches 110.03125.
            (
                0.015625,
                0.015625,
                [(110.0, 110.03125), (110.0, 110.015625)],
                (109.9, 110.1),
                &["110.031250"],
            ),
        ];
        let rect = Rect::from_x_y_ranges(0.0..=400.0, 0.0..=400.0);
        let highlight = TpoRenderConfig::default().single_print_color;
        for (tick, rows, periods, range, prints) in cases {
            let bars: Vec<_> = periods
                .iter()
                .enumerate()
                .map(|(i, &(low, high))| {
                    Bar::new(
                        chrono::DateTime::UNIX_EPOCH + Duration::minutes(30 * i as i64),
                        low,
                        high,
                        low,
                        high,
                        1.0,
                    )
                })
                .collect();
            let config = TPOConfig {
                tick_size: rows,
                show_single_prints: true,
                ..Default::default()
            };
            let coords = PriceMap::new(
                crate::scales::PriceTransform::default(),
                crate::scales::PriceRange::new(range.0, range.1),
                rect.min.y,
                rect.max.y,
                false,
            );
            let ctx = egui::Context::default();
            let draw = |events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(rect),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        ui.allocate_rect(rect, egui::Sense::hover());
                        render_live_tpo(
                            ui.painter(),
                            rect,
                            &bars,
                            0..bars.len(),
                            1,
                            &coords,
                            &config,
                            Some(tick),
                            &BarClock::new(Timeframe::Min30),
                            |i, _| i as f32 * 100.0 + 50.0,
                            0.0,
                            Color32::GREEN,
                            Color32::RED,
                        )
                    },
                );
                output.textures_delta.clear();
                output
            };
            let rows_lit: Vec<_> = draw(vec![])
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Rect(r) if r.fill == highlight => Some(r.rect),
                    _ => None,
                })
                .collect();
            let mut named = Vec::new();
            for row in rows_lit {
                // A new tooltip shows from its second pass.
                draw(vec![egui::Event::PointerMoved(row.center())]);
                let hovered = draw(vec![egui::Event::PointerMoved(row.center())]);
                named.extend(hovered.shapes.iter().filter_map(|s| {
                    match &s.shape {
                        Shape::Text(t) => t
                            .galley
                            .text()
                            .strip_prefix("Single print ")
                            .map(str::to_owned),
                        _ => None,
                    }
                }));
            }
            assert_eq!(named, prints, "tick {tick}, rows {rows}");
        }
    }

    #[test]
    fn statistics_draw_only_where_they_fit() {
        let model = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let profile = to_tpo_profiles(
            &three_overlapping_periods(),
            &model,
            &Timeframe::Min30.into(),
        )
        .remove(0);
        let rect = Rect::from_x_y_ranges(0.0..=400.0, 0.0..=400.0);
        let coords = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(90.0, 115.0),
            rect.min.y,
            rect.max.y,
            false,
        );
        // (left edge, room of its session, drawn)
        for (anchor, room, drawn) in [
            (10.0, f32::INFINITY, true),
            // It would reach the next session's table.
            (10.0, 20.0, false),
        ] {
            let ctx = egui::Context::default();
            ctx.begin_pass(egui::RawInput::default());
            let painter = ctx.layer_painter(egui::LayerId::background());
            let rows = TPOStatisticsRows::default();
            draw_statistics(&painter, rect, &coords, &profile, anchor, room, &rows);
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            let painted = output
                .shapes
                .iter()
                .any(|s| matches!(&s.shape, Shape::Text(t) if t.galley.text() == "Rotation"));
            assert_eq!(painted, drawn, "anchor {anchor}, room {room}");
        }
    }

    #[test]
    fn statistics_rows_follow_their_switches() {
        // The hand-counted session of the model's statistics test, with volumes 400 to 800.
        let start = chrono::DateTime::parse_from_rfc3339("2024-01-02T14:30:00Z")
            .unwrap()
            .to_utc();
        let bars: Vec<_> = [
            (101.0, 103.0, 100.0, 102.0, 400.0),
            (102.0, 104.0, 101.0, 103.0, 600.0),
            (104.0, 106.0, 103.0, 105.0, 700.0),
            (104.0, 105.0, 99.0, 100.0, 800.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (o, h, l, c, v))| {
            Bar::new(start + Duration::minutes(30 * i as i64), o, h, l, c, v)
        })
        .collect();
        let clock = Timeframe::Min30.into();
        let config = TPOConfig {
            tick_size: 1.0,
            ..Default::default()
        };
        let whole = to_tpo_profiles(&bars, &config, &clock).remove(0);
        let first_two = to_tpo_profiles(&bars[..2], &config, &clock).remove(0);
        let split = TPOConfig {
            splits: vec![start + Duration::minutes(60)],
            ..config.clone()
        };
        let second_part = to_tpo_profiles(&bars, &split, &clock).remove(1);
        let mut all = TPOStatisticsRows::default();
        for (_, _, shown) in all.switches() {
            *shown = true;
        }
        let only_volume = {
            let mut rows = all;
            for (id, _, shown) in rows.switches() {
                *shown = id == "total_volume";
            }
            rows
        };
        let none = {
            let mut rows = all;
            for (_, _, shown) in rows.switches() {
                *shown = false;
            }
            rows
        };
        let rect = Rect::from_x_y_ranges(0.0..=400.0, 0.0..=400.0);
        let coords = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(90.0, 115.0),
            rect.min.y,
            rect.max.y,
            false,
        );
        let whole_default = [
            ("POC", "103"),
            ("VAL / VAH", "101 / 105"),
            ("IB range", "4"),
            ("TPO below / above", "9 / 6"),
            ("Rotation", "+2"),
            ("IB ext. below / above", "1 / 2"),
        ];
        // (profile, rows shown, expected (label, value) pairs)
        type Case<'a> = (&'a TPOProfile, TPOStatisticsRows, &'a [(&'a str, &'a str)]);
        let cases: [Case; 6] = [
            (
                &whole,
                all,
                &[
                    ("High-low range", "7"),
                    ("POC", "103"),
                    ("VAL / VAH", "101 / 105"),
                    ("VA range", "4"),
                    ("IB high", "104"),
                    ("IB low", "100"),
                    ("IB range", "4"),
                    ("TPO below / above", "9 / 6"),
                    ("Total TPOs", "19"),
                    ("Total volume", "2.50K"),
                    ("Rotation", "+2"),
                    ("IB ext. below / above", "1 / 2"),
                ],
            ),
            (
                &first_two,
                all,
                &[
                    ("High-low range", "4"),
                    ("POC", "101"),
                    ("VAL / VAH", "101 / 103"),
                    ("VA range", "2"),
                    ("IB high", "104"),
                    ("IB low", "100"),
                    ("IB range", "4"),
                    ("TPO below / above", "1 / 5"),
                    ("Total TPOs", "8"),
                    ("Total volume", "1.00K"),
                    ("Rotation", "+2"),
                    ("IB ext. below / above", "0 / 0"),
                ],
            ),
            // After the balance window: no initial balance rows.
            (
                &second_part,
                all,
                &[
                    ("High-low range", "7"),
                    ("POC", "103"),
                    ("VAL / VAH", "101 / 105"),
                    ("VA range", "4"),
                    ("TPO below / above", "4 / 5"),
                    ("Total TPOs", "11"),
                    ("Total volume", "1.50K"),
                    ("Rotation", "-2"),
                ],
            ),
            (&whole, TPOStatisticsRows::default(), &whole_default),
            (&whole, only_volume, &[("Total volume", "2.50K")]),
            (&whole, none, &[]),
        ];
        for (profile, rows, expected) in cases {
            let ctx = egui::Context::default();
            ctx.begin_pass(egui::RawInput::default());
            let painter = ctx.layer_painter(egui::LayerId::background());
            draw_statistics(&painter, rect, &coords, profile, 10.0, f32::INFINITY, &rows);
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Text(t) => Some(t.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            let pairs: Vec<_> = texts
                .chunks(2)
                .map(|pair| (pair[0].as_str(), pair[1].as_str()))
                .collect();
            assert_eq!(pairs, expected, "rows {rows:?}");
            // An empty table paints no background either.
            assert_eq!(
                output.shapes.is_empty(),
                expected.is_empty(),
                "rows {rows:?}"
            );
        }
    }

    #[test]
    fn statistics_stay_clear_of_the_next_session_captions() {
        // Two UTC days of eight half-hour bars, the second trading 11 lower, so its VAH caption
        // falls at the height of the first day's table.
        let bars: Vec<_> = [0.0, -11.0]
            .into_iter()
            .enumerate()
            .flat_map(|(day, shift)| {
                [
                    (104.0, 110.0),
                    (102.0, 108.0),
                    (103.0, 109.0),
                    (101.0, 107.0),
                    (104.0, 108.0),
                    (102.0, 106.0),
                    (103.0, 107.0),
                    (101.0, 105.0),
                ]
                .into_iter()
                .enumerate()
                .map(move |(i, (low, high))| {
                    let minute = day as i64 * 1440 + i as i64 * 30;
                    let time = chrono::DateTime::UNIX_EPOCH + Duration::minutes(minute);
                    Bar::new(
                        time,
                        low + shift,
                        high + shift,
                        low + shift,
                        high + shift,
                        1.0,
                    )
                })
            })
            .collect();
        let rect = Rect::from_x_y_ranges(0.0..=800.0, 0.0..=400.0);
        let coords = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(85.0, 115.0),
            rect.min.y,
            rect.max.y,
            false,
        );
        // (px per bar, level captions shown, table drawn if known)
        let rows = (18..=25)
            .map(|spacing| (spacing as f32, true, None))
            .chain([(40.0, true, Some(true)), (23.0, false, Some(true))]);
        for (spacing, captions, drawn) in rows {
            let config = TPOConfig {
                tick_size: 1.0,
                show_statistics: true,
                show_poc: captions,
                show_value_area: captions,
                ..Default::default()
            };
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| {
                    ui.allocate_rect(rect, egui::Sense::hover());
                    render_live_tpo(
                        ui.painter(),
                        rect,
                        &bars,
                        0..bars.len(),
                        1,
                        &coords,
                        &config,
                        None,
                        &BarClock::new(Timeframe::Min30),
                        |i, _| 50.0 + i as f32 * spacing,
                        0.0,
                        Color32::GREEN,
                        Color32::RED,
                    )
                },
            );
            output.textures_delta.clear();
            let background = ctx.active_theme().chart.bg;
            let tables: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Rect(r) if r.fill == background => Some(r.rect),
                    _ => None,
                })
                .collect();
            // The second day's captions, right-aligned 8 px left of its first bar.
            let caption_right = 50.0 + 8.0 * spacing - 8.0;
            let captions_drawn: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Text(t)
                        if ["POC", "VAH", "VAL", "Mid"].contains(&t.galley.text())
                            && (t.pos.x + t.galley.size().x - caption_right).abs() < 0.5 =>
                    {
                        Some(Rect::from_min_size(t.pos, t.galley.size()))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                captions_drawn.len(),
                if captions { 3 } else { 0 },
                "{spacing} px per bar"
            );
            for table in &tables {
                for caption in &captions_drawn {
                    assert!(
                        !table.intersects(*caption),
                        "{spacing} px per bar: table {table:?}, caption {caption:?}"
                    );
                }
            }
            if let Some(drawn) = drawn {
                // The first day's table starts at its first bar.
                let first = tables.iter().any(|table| table.left() == 50.0);
                assert_eq!(first, drawn, "{spacing} px per bar, captions {captions}");
            }
        }
    }

    #[test]
    fn statistics_name_widened_row_prices_exactly() {
        // One session whose automatic rows (0.025) are widened to 0.075, which is not exact in f64.
        let start = chrono::DateTime::parse_from_rfc3339("2024-01-02T14:30:00Z")
            .unwrap()
            .to_utc();
        let bars: Vec<_> = [
            (100.0, 120.0),
            (120.0, 140.0),
            (140.0, 160.0),
            (110.0, 150.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (low, high))| {
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
        let rect = Rect::from_x_y_ranges(0.0..=400.0, 0.0..=400.0);
        let coords = PriceMap::new(
            crate::scales::PriceTransform::default(),
            crate::scales::PriceRange::new(100.0, 101.0),
            rect.min.y,
            rect.max.y,
            false,
        );
        let statistics_rows = TPOStatisticsRows {
            value_area_range: true,
            high_low_range: true,
            ..Default::default()
        };
        let config = TPOConfig {
            tick_size: 0.0,
            show_statistics: true,
            statistics_rows,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| {
                ui.allocate_rect(rect, egui::Sense::hover());
                render_live_tpo(
                    ui.painter(),
                    rect,
                    &bars,
                    0..bars.len(),
                    1,
                    &coords,
                    &config,
                    None,
                    &BarClock::new(Timeframe::Min30),
                    |i, _| i as f32 * 100.0 + 50.0,
                    0.0,
                    Color32::GREEN,
                    Color32::RED,
                )
            },
        );
        output.textures_delta.clear();
        let texts: Vec<String> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| t == "120.000")
                && texts.iter().any(|t| t == "110.100 / 145.050")
                && texts.iter().any(|t| t == "34.950")
                && texts.iter().any(|t| t == "60.000"),
            "statistics texts {texts:?}"
        );
    }
}
