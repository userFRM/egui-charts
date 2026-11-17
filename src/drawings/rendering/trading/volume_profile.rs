//! Volume tools: anchored VWAP and the fixed range and anchored volume profiles

use crate::data::{StudyDataRequest, VolumeProfile};
use crate::drawings::domain::Drawing;
use crate::drawings::domain::DrawingToolType;
use crate::drawings::domain::{VwapBandLine, VwapBandMode, VwapConfig, VwapPlotType};
use crate::drawings::rendering::{ChartView, MAX_CACHED_SPANS, bars_between};
use crate::model::{Bar, LineStyle, PriceSource, ProfileResolution, Timeframe};
use crate::painting::span_cache::{SpanCache, SpanFingerprint};
use crate::painting::volume_profile::ProfileStyle;
use chrono::{DateTime, Utc};
use egui::{Color32, Pos2, Shape, Stroke};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

/// Bars between the saved running sums of an anchored VWAP.
const VWAP_CHECKPOINT: usize = 1024;

type ProfileCache = SpanCache<Option<VolumeProfile>, (usize, Option<f64>)>;

/// Rows, row size, the finer span and its history's address.
type FinerCache = SpanCache<Option<VolumeProfile>, (usize, Option<f64>, SpanFingerprint, usize)>;

thread_local! {
    /// Checkpoints by the price they average.
    static VWAP_CHECKPOINTS: RefCell<SpanCache<Vec<VwapSums>, PriceSource>> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
    /// Profiles by their number of rows.
    static PROFILES: RefCell<ProfileCache> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
    /// Profiles built from finer bars.
    static FINER_PROFILES: RefCell<FinerCache> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
}

impl Drawing {
    /// Renders the anchored VWAP from the anchor bar to the last bar as its
    /// [`VwapConfig`](crate::drawings::VwapConfig) sets: the background, the first band's fill,
    /// the shown lines of the enabled bands as each is styled, the line at the drawing's colour,
    /// width and line style, and the tracked latest values across the pane.
    pub(crate) fn render_anchored_vwap(&self, painter: &egui::Painter, view: &ChartView) {
        let Some(anchor_at) = self.chart_points.first() else {
            return;
        };
        if let Some((first, bars)) = bars_between(view.bars, anchor_at.bar_idx, f32::INFINITY) {
            self.draw_vwap(painter, view, first, bars);
        }
        // Drawn whatever bars are in view, at the values the price axis tags.
        for (price, line) in self.anchored_vwap_tracked(view.bars, view.revision) {
            let clip = painter.clip_rect();
            let y = (view.price_to_y)(price);
            if clip.y_range().contains(y) {
                crate::painting::dashed_line::draw_dashed_line(
                    painter,
                    Pos2::new(clip.left(), y),
                    Pos2::new(clip.right(), y),
                    Stroke::new(1.0, painted_color(&line)),
                    1.0,
                    3.0,
                );
            }
        }
    }

    fn draw_vwap(&self, painter: &egui::Painter, view: &ChartView, first: usize, bars: &[Bar]) {
        // Only bars inside the clip rect (plus one on each side) become
        // points; bar_to_x is linear, so they are found without a scan.
        let x0 = (view.bar_to_x)(first as f32);
        let step = (view.bar_to_x)(first as f32 + 1.0) - x0;
        if step.is_nan() || step <= 0.0 {
            return;
        }
        let clip = painter.clip_rect();
        let from = (((clip.left() - x0) / step).floor() - 1.0).max(0.0) as usize;
        let to = ((((clip.right() - x0) / step).ceil() + 2.0).max(0.0) as usize).min(bars.len());
        if from >= to {
            return;
        }

        let config = self.vwap.unwrap_or_default();
        let checkpoints = cached_checkpoints(config.source, first, bars, view.revision);
        // The VWAP, then the upper and lower line of every enabled band, in band units from it.
        let mut offsets = vec![0.0];
        let mut band_lines = Vec::new();
        for band in config.bands.iter().filter(|b| b.enabled) {
            offsets.extend([band.multiplier(), -band.multiplier()]);
            band_lines.extend([band.upper, band.lower]);
        }
        let mut lines = vec![Vec::with_capacity(to - from); offsets.len()];
        for (i, value) in (from..to).zip(anchored_vwap(bars, &checkpoints, &config, from..to)) {
            let Some((vwap, unit)) = value else { continue };
            let x = (view.bar_to_x)((first + i) as f32);
            for (line, k) in lines.iter_mut().zip(&offsets) {
                line.push(Pos2::new(x, (view.price_to_y)(vwap + k * unit)));
            }
        }
        if lines[0].is_empty() {
            return;
        }

        let line_color = self.color32();
        if config.background.visible {
            let color = config.background.color.map_or(line_color, rgba);
            let left = (view.bar_to_x)(first as f32 - 0.5);
            let right = (view.bar_to_x)((first + bars.len() - 1) as f32 + 0.5);
            painter.rect_filled(
                egui::Rect::from_x_y_ranges(left..=right, clip.y_range()),
                0.0,
                color.gamma_multiply(config.background.opacity()),
            );
        }
        if config.bands[0].enabled && config.band_fill.visible {
            let color = rgba(
                config
                    .band_fill
                    .color
                    .unwrap_or(config.bands[0].upper.color),
            );
            super::super::fibonacci::paint_band(
                painter,
                &lines[1],
                &lines[2],
                color.gamma_multiply(config.band_fill.opacity()),
            );
        }
        let vwap_line = lines.remove(0);
        let zero = (view.price_to_y)(0.0);
        for (points, line) in lines.into_iter().zip(band_lines) {
            if line.visible {
                let stroke = Stroke::new(line.width(), painted_color(&line));
                paint_vwap_line(
                    painter,
                    points,
                    line.plot_type,
                    line.line_style,
                    stroke,
                    zero,
                    step,
                );
            }
        }
        if !config.show_line {
            return;
        }
        paint_vwap_line(
            painter,
            vwap_line,
            config.plot_type,
            self.line_style,
            Stroke::new(self.stroke_width, line_color),
            zero,
            step,
        );
    }

    /// The values at the last of `bars`, the pane's bars at `revision`, of the anchored VWAP's
    /// tracked lines, each with its style: the VWAP while its line shows, in the drawing's
    /// colour, then the shown band lines of the enabled bands. Empty for other drawings or before
    /// volume trades.
    pub(crate) fn anchored_vwap_tracked(
        &self,
        bars: &[Bar],
        revision: u64,
    ) -> Vec<(f64, VwapBandLine)> {
        if self.tool_type != DrawingToolType::AnchoredVWAP {
            return Vec::new();
        }
        let config = self.vwap.unwrap_or_default();
        let vwap =
            (config.track_price && config.show_line).then(|| (0.0, VwapBandLine::new(self.color)));
        let tracked: Vec<_> = vwap
            .into_iter()
            .chain(
                config
                    .bands
                    .iter()
                    .filter(|band| band.enabled)
                    .flat_map(|band| {
                        [
                            (band.multiplier(), band.upper),
                            (-band.multiplier(), band.lower),
                        ]
                    })
                    .filter(|(_, line)| line.visible && line.track_price),
            )
            .collect();
        if tracked.is_empty() {
            return tracked;
        }
        let latest = || {
            let anchor = self.chart_points.first()?.bar_idx;
            let (first, bars) = bars_between(bars, anchor, f32::INFINITY)?;
            let checkpoints = cached_checkpoints(config.source, first, bars, revision);
            let last = bars.len() - 1;
            anchored_vwap(bars, &checkpoints, &config, last..bars.len()).next()?
        };
        let Some((vwap, unit)) = latest() else {
            return Vec::new();
        };
        tracked
            .into_iter()
            .map(|(k, line)| (vwap + k * unit, line))
            .collect()
    }

    /// Whether a visible profile drawing reads the chart's executions and finer bars.
    pub(crate) fn reads_profile(&self) -> bool {
        self.visible
            && matches!(
                self.tool_type,
                DrawingToolType::FixedRangeVolumeProfile | DrawingToolType::AnchoredVolumeProfile
            )
    }

    /// First index and bars between the anchors, or from the anchor to the last bar.
    fn profile_span<'b>(&self, bars: &'b [Bar]) -> Option<(usize, &'b [Bar])> {
        let anchor = |i: usize| self.chart_points.get(i).map(|p| p.bar_idx);
        match self.tool_type {
            DrawingToolType::FixedRangeVolumeProfile => {
                let extended = self.volume_profile.is_some_and(|c| c.extend_right);
                let start = anchor(0)?;
                let start = if extended {
                    start.min(anchor(1).unwrap_or(start))
                } else {
                    start
                };
                bars_between(
                    bars,
                    start,
                    if extended { f32::INFINITY } else { anchor(1)? },
                )
            }
            DrawingToolType::AnchoredVolumeProfile => bars_between(bars, anchor(0)?, f32::INFINITY),
            _ => None,
        }
    }

    /// The suggested history this visible profile reads, with the first open of its span.
    pub(crate) fn profile_history(
        &self,
        bars: &[Bar],
        timeframe: Timeframe,
        resolutions: &[ProfileResolution],
    ) -> Option<(StudyDataRequest, DateTime<Utc>)> {
        if !self.reads_profile() {
            return None;
        }
        let (_, span) = self.profile_span(bars)?;
        let interval = finer_interval(span, timeframe, resolutions)?;
        Some((
            StudyDataRequest {
                symbol: String::new(),
                timeframe: Some(interval),
                metric: None,
            },
            span.first()?.time,
        ))
    }

    /// Renders the volume profile of the bars between the two anchors, or from the earlier one to
    /// the last bar when extended right, from half a bar before the first bar to half a bar after
    /// the last, with the point of control and the value area.
    pub(crate) fn render_fixed_range_volume_profile(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
    ) {
        let &[a, b, ..] = self.chart_points.as_slice() else {
            return;
        };
        if let Some((first, bars)) = self.profile_span(view.bars) {
            let end = a.bar_idx.max(b.bar_idx).round();
            let end = if self.volume_profile.is_some_and(|c| c.extend_right) {
                end.max((first + bars.len() - 1) as f32)
            } else {
                end
            };
            let left = (view.bar_to_x)(a.bar_idx.min(b.bar_idx).round() - 0.5);
            let right = (view.bar_to_x)(end + 0.5);
            self.draw_volume_profile(painter, view, first, bars, left, right);
        }
    }

    /// Renders the volume profile from half a bar before the anchor bar to half a bar after the
    /// last bar.
    pub(crate) fn render_anchored_volume_profile(&self, painter: &egui::Painter, view: &ChartView) {
        let Some(anchor) = self.chart_points.first() else {
            return;
        };
        if let Some((first, bars)) = self.profile_span(view.bars) {
            let left = (view.bar_to_x)(anchor.bar_idx.round() - 0.5);
            let right = (view.bar_to_x)((first + bars.len() - 1) as f32 + 0.5);
            self.draw_volume_profile(painter, view, first, bars, left, right);
        }
    }

    /// Where the handles of an anchored VWAP or a profile drawing sit, since their anchors only
    /// choose bars: on those bars, at the start of the VWAP, the top of the fixed range or the
    /// anchored profile's point of control. `None` when nothing is drawn.
    pub(crate) fn volume_tool_handles(&self, view: &ChartView) -> Option<Vec<Pos2>> {
        if self.tool_type == DrawingToolType::AnchoredVWAP {
            let anchor = self.chart_points.first()?.bar_idx;
            let (first, bars) = bars_between(view.bars, anchor, f32::INFINITY)?;
            let source = self.vwap.unwrap_or_default().source;
            let mut sums = VwapSums::default();
            let (i, (vwap, _)) = bars.iter().enumerate().find_map(|(i, bar)| {
                sums.add(bar, source);
                sums.value(VwapBandMode::Deviation).map(|value| (i, value))
            })?;
            let x = (view.bar_to_x)((first + i) as f32);
            return Some(vec![Pos2::new(x, (view.price_to_y)(vwap))]);
        }
        let (first, bars) = self.profile_span(view.bars)?;
        let profile = self.span_profile(view, first, bars);
        let profile = profile.as_ref().as_ref()?;
        let row = if self.tool_type == DrawingToolType::AnchoredVolumeProfile {
            let share = self.volume_profile.unwrap_or_default().value_area();
            profile.poc_and_value_area(share)?.0 as f64 + 0.5
        } else {
            profile.volume.len() as f64
        };
        let y = (view.price_to_y)(profile.low + row * profile.row_height);
        Some(
            self.chart_points
                .iter()
                .map(|p| Pos2::new((view.bar_to_x)(p.bar_idx.round()), y))
                .collect(),
        )
    }

    /// Draws the profile of `bars`, the span that starts at bar `first`, as
    /// rows beside the first anchor, or at the pane's right edge when anchored,
    /// with a tint over the measured bars' price and time range.
    fn draw_volume_profile(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
        first: usize,
        bars: &[Bar],
        left: f32,
        right: f32,
    ) {
        let config = self.volume_profile.unwrap_or_default();
        let profile = self.span_profile(view, first, bars);
        let Some(profile) = profile.as_ref() else {
            return;
        };
        crate::painting::volume_profile::paint_profile(
            painter,
            profile,
            config,
            self.color,
            left..right,
            ProfileStyle::Drawing {
                anchored: self.tool_type == DrawingToolType::AnchoredVolumeProfile,
                poc_width: self.stroke_width,
            },
            view.price_to_y,
        );
    }

    /// The profile of `bars`, the span that starts at bar `first`: the executed volume at each
    /// price when the view holds complete executions in the span, else the finer bars where they
    /// cover chart bars, else estimated from the chart bars.
    fn span_profile(
        &self,
        view: &ChartView,
        first: usize,
        bars: &[Bar],
    ) -> Rc<Option<VolumeProfile>> {
        if let (Some(flow), Some(start), Some(end)) = (view.executions, bars.first(), bars.last()) {
            let executed = flow.profile(start.time, end.time);
            let complete = flow.span_is_complete(start.time, end.time);
            if let Some(profile) = VolumeProfile::from_executions(
                &executed,
                flow.price_step(),
                flow.tick_size(),
                complete,
            ) {
                return Rc::new(Some(profile));
            }
        }
        let config = self.volume_profile.unwrap_or_default();
        if let Some(profile) = finer_profile(view, config.rows(), config.row_size(), first, bars) {
            return profile;
        }
        span_profile(view, config.rows(), config.row_size(), first, bars)
    }

    /// The profile that a fixed range or anchored volume profile shows over the view's bars,
    /// with its [source](VolumeProfile::source): executed volume when
    /// [`ChartView::executions`] holds complete bars in the selected span, including prepared
    /// bars before a missing join, else the finer bars
    /// of [`ChartView::finer`], else the bar estimate.
    /// `None` for other tools, a drawing without its anchors, or an empty selection.
    pub fn volume_profile_of(&self, view: &ChartView) -> Option<VolumeProfile> {
        let (first, bars) = self.profile_span(view.bars)?;
        self.span_profile(view, first, bars).as_ref().clone()
    }
}

/// Running sums of volume, source price times volume, and squared source
/// price times volume.
#[derive(Clone, Copy, Default)]
struct VwapSums {
    volume: f64,
    price_volume: f64,
    square_volume: f64,
}

impl VwapSums {
    fn add(&mut self, bar: &Bar, source: PriceSource) {
        let price = source.compute(bar.open, bar.high, bar.low, bar.close);
        self.volume += bar.volume;
        self.price_volume += price * bar.volume;
        self.square_volume += price * price * bar.volume;
    }

    /// VWAP and the unit of its bands in `mode`; `None` until volume trades.
    fn value(&self, mode: VwapBandMode) -> Option<(f64, f64)> {
        (self.volume > 0.0).then(|| {
            let vwap = self.price_volume / self.volume;
            let variance = self.square_volume / self.volume - vwap * vwap;
            match mode {
                VwapBandMode::Deviation => (vwap, variance.max(0.0).sqrt()),
                VwapBandMode::Percentage => (vwap, vwap.abs() / 100.0),
            }
        })
    }
}

/// VWAP of `config`'s source over all of `bars`, from the anchor bar, with its band unit, as
/// the painted line has it at the last of `bars`; `None` until volume trades.
pub(crate) fn anchored_vwap_through(bars: &[Bar], config: &VwapConfig) -> Option<(f64, f64)> {
    let mut sums = VwapSums::default();
    bars.iter().for_each(|bar| sums.add(bar, config.source));
    sums.value(config.band_mode)
}

/// The running sums at the start of the span and after every block of
/// `VWAP_CHECKPOINT` bars of `prefix`, the span's leading whole blocks, so the
/// VWAP at any bar takes fewer than `VWAP_CHECKPOINT` further additions.
fn vwap_checkpoints(prefix: &[Bar], source: PriceSource) -> Vec<VwapSums> {
    let mut sums = VwapSums::default();
    let mut checkpoints = vec![sums];
    for block in prefix.chunks(VWAP_CHECKPOINT) {
        block.iter().for_each(|bar| sums.add(bar, source));
        checkpoints.push(sums);
    }
    checkpoints
}

/// The checkpoints of the span of `bars` that starts at bar `first`, averaging `source`. They
/// only cover whole blocks before the last bar, so the forming bar's updates reuse them.
fn cached_checkpoints(
    source: PriceSource,
    first: usize,
    bars: &[Bar],
    revision: u64,
) -> Rc<Vec<VwapSums>> {
    let covered = (bars.len() - 1) / VWAP_CHECKPOINT * VWAP_CHECKPOINT;
    VWAP_CHECKPOINTS.with_borrow_mut(|cache| {
        cache.get(source, first, &bars[..covered], revision, |prefix| {
            vwap_checkpoints(prefix, source)
        })
    })
}

/// VWAP of `config`'s source from the first bar of the span, with its band unit, at each bar of
/// the non-empty `range`, resuming from the checkpoint before it.
fn anchored_vwap<'a>(
    bars: &'a [Bar],
    checkpoints: &[VwapSums],
    config: &VwapConfig,
    range: Range<usize>,
) -> impl Iterator<Item = Option<(f64, f64)>> + 'a {
    let (source, mode) = (config.source, config.band_mode);
    let checkpoint = range.start / VWAP_CHECKPOINT;
    let mut sums = checkpoints[checkpoint];
    bars[checkpoint * VWAP_CHECKPOINT..range.start]
        .iter()
        .for_each(|bar| sums.add(bar, source));
    bars[range].iter().map(move |bar| {
        sums.add(bar, source);
        sums.value(mode)
    })
}

/// Draws a line of an anchored VWAP at `points` as `plot` in `stroke`, the line and step line
/// in `style`, the plots that start from price zero from `zero`, at bars `pitch` apart. The area
/// fills in the stroke's colour, as a study's area does.
fn paint_vwap_line(
    painter: &egui::Painter,
    points: Vec<Pos2>,
    plot: VwapPlotType,
    style: LineStyle,
    stroke: Stroke,
    zero: f32,
    pitch: f32,
) {
    let clip = painter.clip_rect();
    // Zero is usually far off the pane: plots from it start at the pane's edge.
    let zero = if zero.is_finite() {
        zero.clamp(clip.top(), clip.bottom())
    } else {
        clip.bottom()
    };
    let color = stroke.color;
    let path = |points: Vec<Pos2>| match style.dash_pattern() {
        Some((dash, gap)) => {
            crate::painting::dashed_line::draw_dashed_polyline(painter, &points, stroke, dash, gap);
        }
        None => {
            painter.add(Shape::line(points, stroke));
        }
    };
    match plot {
        VwapPlotType::Line => path(points),
        VwapPlotType::StepLine => {
            let mut steps = Vec::with_capacity(points.len() * 2);
            for (i, &point) in points.iter().enumerate() {
                if i > 0 {
                    steps.push(Pos2::new(point.x, points[i - 1].y));
                }
                steps.push(point);
            }
            path(steps);
        }
        VwapPlotType::Histogram | VwapPlotType::Columns => {
            let half = if plot == VwapPlotType::Columns {
                pitch * 0.4
            } else {
                stroke.width * 0.5
            };
            for point in points {
                painter.rect_filled(
                    egui::Rect::from_two_pos(
                        Pos2::new(point.x - half, zero),
                        Pos2::new(point.x + half, point.y),
                    ),
                    0.0,
                    color,
                );
            }
        }
        VwapPlotType::Area => {
            let base: Vec<_> = points.iter().map(|p| Pos2::new(p.x, zero)).collect();
            super::super::fibonacci::paint_band(painter, &points, &base, color);
        }
        VwapPlotType::Circles => {
            for point in points {
                painter.circle_filled(point, stroke.width, color);
            }
        }
        VwapPlotType::Crosses => {
            let arm = stroke.width + 1.0;
            for point in points {
                for delta in [egui::vec2(arm, 0.0), egui::vec2(0.0, arm)] {
                    painter.line_segment([point - delta, point + delta], Stroke::new(1.0, color));
                }
            }
        }
    }
}

fn rgba([r, g, b, a]: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// The colour of a band line at its opacity.
fn painted_color(line: &VwapBandLine) -> Color32 {
    rgba(line.color).gamma_multiply(line.opacity())
}

/// Profile of `bars`, the span that starts at bar `first`, with `rows` rows.
/// While the last bar, which may be forming, trades inside the range of the
/// bars before it, their profile is reused, so a tick or a new bar costs one
/// bar instead of the span.
fn span_profile(
    view: &ChartView,
    rows: usize,
    row_size: Option<f64>,
    first: usize,
    bars: &[Bar],
) -> Rc<Option<VolumeProfile>> {
    PROFILES.with_borrow_mut(|cache| {
        view.cached(
            cache,
            (rows, row_size),
            first,
            bars,
            |bars| VolumeProfile::of(bars, rows, row_size),
            |profile, bar| profile.as_ref()?.with_bar(bar).map(Some),
        )
    })
}

/// Close of a bar of `timeframe`, saturating at the end of the calendar.
fn close(open: DateTime<Utc>, timeframe: Timeframe) -> DateTime<Utc> {
    open.checked_add_signed(timeframe.duration())
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

/// Suggested interval for a span measured from its first open through its last close.
fn finer_interval(
    span: &[Bar],
    timeframe: Timeframe,
    resolutions: &[ProfileResolution],
) -> Option<Timeframe> {
    let (first, last) = (span.first()?, span.last()?);
    ProfileResolution::suggested(
        resolutions,
        timeframe,
        close(last.time, timeframe) - first.time,
    )
}

/// Replaces chart bars by the suggested finer bars inside them, when any are held in the span.
fn finer_profile(
    view: &ChartView,
    rows: usize,
    row_size: Option<f64>,
    first: usize,
    bars: &[Bar],
) -> Option<Rc<Option<VolumeProfile>>> {
    if view.finer.is_empty() {
        return None;
    }
    let timeframe = view.timeframe?;
    let interval = finer_interval(bars, timeframe, &view.instrument?.profile_resolutions)?;
    let finer = view
        .finer
        .iter()
        .find(|data| data.timeframe == interval)?
        .bars;
    let (start, last) = (bars.first()?.time, bars.last()?.time);
    let end = view
        .bars
        .get(first + bars.len())
        .map_or_else(|| close(last, timeframe), |next| next.time);
    let at = finer.partition_point(|bar| bar.time < start);
    let inside = &finer[at..at + finer[at..].partition_point(|bar| bar.time < end)];
    if inside.is_empty() {
        return None;
    }
    // shortcut: a host correction of a finer bar inside the span that keeps its fingerprint
    // shows at the next span change; key on a history revision if feeds need it at once.
    let key = (
        rows,
        row_size,
        SpanFingerprint::of(at, inside, 0),
        finer.as_ptr() as usize,
    );
    Some(FINER_PROFILES.with_borrow_mut(|cache| {
        cache.get(key, first, bars, view.revision, |bars| {
            VolumeProfile::of_finer(bars, end, inside, rows, row_size).0
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::DrawingToolType;
    use crate::drawings::rendering::sample_bars;
    use egui::Rect;

    fn assert_close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
    }

    /// The line keeps the drawing's width and line style and only the enabled bands are drawn,
    /// band 1 filled. The line, its plot, the fill, the background, the latest-value line and each
    /// band line follow their own settings, so styling one band line leaves the other and the VWAP
    /// alone, and a dashed drawing leaves its solid band lines solid.
    #[test]
    fn anchored_vwap_draws_its_line_and_enabled_bands_only() {
        use crate::drawings::{VwapBand, VwapBandLine, VwapConfig, VwapFill};
        // From bar 1: V = 1, 2, 8; PV = 10, 22, 88; P²V = 100, 244, 970.
        // VWAP = 10, 11, 11; deviation = 0, 1, 0.5.
        let bars: Vec<_> = [(50.0, 0.0), (10.0, 1.0), (12.0, 1.0), (11.0, 6.0)]
            .into_iter()
            .enumerate()
            .map(|(i, (price, volume))| {
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                Bar::new(time, price, price, price, price, volume)
            })
            .collect();
        let blue = Color32::from_rgb(41, 98, 255);
        let green = Color32::from_rgb(76, 175, 80);
        let olive = Color32::from_rgb(128, 128, 0);
        let teal = Color32::from_rgb(0, 137, 123);
        let mid = [100.0, 90.0, 90.0];
        let mut other_bands = VwapConfig::default();
        other_bands.bands[0].enabled = false;
        other_bands.bands[1].enabled = true;
        let teal_band = other_bands.bands[2];
        other_bands.bands[2] = VwapBand::new(true, 0.5).unwrap();
        other_bands.bands[2].upper = teal_band.upper;
        other_bands.bands[2].lower = teal_band.lower;
        let mut styled = VwapConfig::default();
        styled.bands[0].upper = VwapBandLine::new([200, 0, 0, 255])
            .with_width(2.0)
            .and_then(|line| line.with_opacity(0.5))
            .unwrap();
        let mut no_bands = VwapConfig::default();
        no_bands.bands[0].enabled = false;
        let render =
            |anchor, width, style, config| render_vwap(&bars, anchor, width, style, config);
        for (name, anchor, style, config, width, fill, expected) in [
            (
                "default",
                0.0,
                LineStyle::Solid,
                None,
                1.0,
                true,
                vec![
                    ([100.0, 80.0, 85.0], green, 1.0),
                    ([100.0, 100.0, 95.0], green, 1.0),
                    (mid, blue, 1.0),
                ],
            ),
            (
                "bands 2 and 3",
                0.0,
                LineStyle::Solid,
                Some(other_bands),
                3.0,
                false,
                vec![
                    ([100.0, 70.0, 80.0], olive, 1.0),
                    ([100.0, 110.0, 100.0], olive, 1.0),
                    ([100.0, 85.0, 87.5], teal, 1.0),
                    ([100.0, 95.0, 92.5], teal, 1.0),
                    (mid, blue, 3.0),
                ],
            ),
            (
                "styled upper line",
                0.0,
                LineStyle::Solid,
                Some(styled),
                1.0,
                true,
                vec![
                    (
                        [100.0, 80.0, 85.0],
                        Color32::from_rgba_premultiplied(100, 0, 0, 128),
                        2.0,
                    ),
                    ([100.0, 100.0, 95.0], green, 1.0),
                    (mid, blue, 1.0),
                ],
            ),
            (
                "no bands",
                0.0,
                LineStyle::Solid,
                Some(no_bands),
                1.0,
                false,
                vec![(mid, blue, 1.0)],
            ),
            // The default band's solid lines at the values above, then the VWAP line in dashes.
            (
                "dashed line, solid bands",
                0.0,
                LineStyle::Dashed,
                None,
                1.0,
                true,
                vec![
                    ([100.0, 80.0, 85.0], green, 1.0),
                    ([100.0, 100.0, 95.0], green, 1.0),
                ],
            ),
            (
                "past last bar",
                4.0,
                LineStyle::Solid,
                None,
                1.0,
                false,
                vec![],
            ),
        ] {
            let shapes = render(anchor, width, style, config);
            if fill {
                let Shape::Mesh(mesh) = &shapes[0] else {
                    panic!("{name}: band fill")
                };
                assert!(
                    mesh.vertices
                        .iter()
                        .all(|v| v.color == Color32::from_rgba_premultiplied(4, 9, 4, 13)),
                    "{name}"
                );
            }
            let (paths, dashes) = shapes[usize::from(fill)..].split_at(expected.len());
            for (shape, (ys, color, width)) in paths.iter().zip(expected) {
                let Shape::Path(line) = shape else {
                    panic!("{name}: unexpected shape")
                };
                assert_eq!(line.stroke, Stroke::new(width, color).into(), "{name}");
                let points: Vec<_> = ys
                    .into_iter()
                    .enumerate()
                    .map(|(i, y)| Pos2::new(110.0 + 10.0 * i as f32, y))
                    .collect();
                assert_eq!(line.points, points, "{name}");
            }
            // The VWAP line runs 14.14 px and then 10 px from (110, 100) through (120, 90) to
            // (130, 90) in its colour and width; dashed 8 px on and 4 px off from its start, it
            // paints pieces of 8, 8 and 0.14 px.
            let pieces: Vec<f32> = dashes
                .iter()
                .filter_map(|shape| {
                    let points = match shape {
                        Shape::Noop => return None,
                        Shape::LineSegment { points, stroke } => {
                            assert_eq!(*stroke, Stroke::new(width, blue), "{name}");
                            points.to_vec()
                        }
                        Shape::Path(line) => {
                            assert_eq!(line.stroke, Stroke::new(width, blue).into(), "{name}");
                            line.points.clone()
                        }
                        other => panic!("{name}: {other:?}"),
                    };
                    Some(points.windows(2).map(|p| p[0].distance(p[1])).sum())
                })
                .collect();
            if style == LineStyle::Solid {
                assert!(pieces.is_empty(), "{name}: {pieces:?}");
            } else {
                assert!(
                    pieces.iter().all(|&piece| piece < 8.001),
                    "{name}: {pieces:?}"
                );
                assert!(
                    (pieces.iter().sum::<f32>() - 16.14).abs() < 0.01,
                    "{name}: {pieces:?}"
                );
            }
        }

        let edited = |edit: fn(&mut VwapConfig)| {
            let mut config = VwapConfig::default();
            edit(&mut config);
            render(0.0, 1.0, LineStyle::Solid, Some(config))
        };
        let strokes = |shapes: &[Shape]| -> Vec<Color32> {
            shapes
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Path(line) => match line.stroke.color {
                        egui::epaint::ColorMode::Solid(color) => Some(color),
                        _ => None,
                    },
                    _ => None,
                })
                .collect()
        };
        // Hidden line: the fill and band 1 stay.
        let shapes = edited(|c| c.show_line = false);
        assert!(matches!(shapes[0], Shape::Mesh(_)));
        assert_eq!(strokes(&shapes), [green, green]);
        assert_eq!(shapes.len(), 3);
        // A hidden band line leaves the other; a dashed one is not a solid path.
        assert_eq!(
            strokes(&edited(|c| c.bands[0].lower.visible = false)),
            [green, blue]
        );
        let shapes = edited(|c| c.bands[0].upper.line_style = crate::model::LineStyle::Dashed);
        assert_eq!(strokes(&shapes), [green, blue]);
        // Band line plotted as circles of its width at its values.
        let shapes =
            edited(|c| c.bands[0].upper.plot_type = crate::drawings::VwapPlotType::Circles);
        let circles: Vec<_> = shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Circle(circle) if circle.fill == green => {
                    Some((circle.center, circle.radius))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            circles,
            [(110.0, 100.0), (120.0, 80.0), (130.0, 85.0)].map(|(x, y)| (Pos2::new(x, y), 1.0))
        );
        assert_eq!(strokes(&shapes), [green, blue]);
        // Columns: one column per value, from price zero up to the VWAP.
        let shapes = edited(|c| c.plot_type = crate::drawings::VwapPlotType::Columns);
        assert_eq!(strokes(&shapes), [green, green]);
        let columns: Vec<_> = shapes[3..]
            .iter()
            .map(|shape| match shape {
                Shape::Rect(rect) if rect.fill == blue => (rect.rect.center().x, rect.rect.top()),
                other => panic!("column: {other:?}"),
            })
            .collect();
        assert_eq!(columns, [(110.0, 100.0), (120.0, 90.0), (130.0, 90.0)]);
        // Fill off: no mesh under the band.
        let shapes = edited(|c| c.band_fill.visible = false);
        assert_eq!(strokes(&shapes), [green, green, blue]);
        assert_eq!(shapes.len(), 3);
        // Fill in its own colour and opacity.
        let shapes =
            edited(|c| c.band_fill = VwapFill::new(true, Some([200, 0, 0, 255]), 0.5).unwrap());
        let Shape::Mesh(mesh) = &shapes[0] else {
            panic!("band fill")
        };
        let red = Color32::from_rgb(200, 0, 0).gamma_multiply(0.5);
        assert!(mesh.vertices.iter().all(|v| v.color == red));
        // Background: green at 5% from half a bar before the anchor bar to half a bar after the
        // last bar, full height, under the fill.
        let shapes = edited(|c| c.background.visible = true);
        let Shape::Rect(wash) = &shapes[0] else {
            panic!("background")
        };
        assert_eq!(
            (wash.rect, wash.fill),
            (
                Rect::from_min_max(Pos2::new(95.0, 0.0), Pos2::new(135.0, 600.0)),
                green.gamma_multiply(0.05)
            )
        );
        assert!(matches!(shapes[1], Shape::Mesh(_)));
        assert_eq!(shapes.len(), 5);
        // Anchored on the last bar: one bar wide, centred on it.
        let mut config = VwapConfig::default();
        config.background.visible = true;
        let Shape::Rect(wash) = &render(3.0, 1.0, LineStyle::Solid, Some(config))[0] else {
            panic!("background on the last bar")
        };
        assert_eq!(
            wash.rect,
            Rect::from_min_max(Pos2::new(125.0, 0.0), Pos2::new(135.0, 600.0))
        );
        // Latest value: a dotted line in the line colour across the pane at VWAP 11, y = 90.
        let shapes = edited(|c| c.track_price = true);
        assert_eq!(shapes.len(), 5);
        let Shape::Mesh(dashes) = &shapes[4] else {
            panic!("price line")
        };
        assert!(
            dashes
                .vertices
                .iter()
                .all(|v| (v.pos.y - 90.0).abs() <= 1.0)
        );
        let xs = dashes.vertices.iter().map(|v| v.pos.x);
        assert!(xs.clone().fold(f32::MAX, f32::min) < 5.0);
        assert!(xs.fold(f32::MIN, f32::max) > 795.0);
        // A tracked band line: its latest value, 11 - 0.5 = 10.5 at y = 95, in its colour; the
        // VWAP's own latest value stays untracked.
        let mut config = VwapConfig::default();
        config.bands[0].lower.track_price = true;
        let shapes = render(0.0, 1.0, LineStyle::Solid, Some(config));
        let Shape::Mesh(dashes) = &shapes[4] else {
            panic!("band price line")
        };
        assert!(
            dashes
                .vertices
                .iter()
                .all(|v| (v.pos.y - 95.0).abs() <= 1.0 && v.color == green)
        );
        assert_eq!(shapes.len(), 5);
    }

    /// A tracked VWAP paints its latest-value line whenever its axis tag shows it, also while the
    /// view lies before the anchor: 100 bars at 10 with volume 1, anchored at bar 80, which sits
    /// at x = 900, right of the 800 px pane. The VWAP from bar 80 is 200 / 20 = 10, at y = 100.
    #[test]
    fn tracked_vwap_draws_its_latest_value_line_with_the_anchor_out_of_view() {
        let bars: Vec<_> = (0..100)
            .map(|i| {
                let time = chrono::DateTime::from_timestamp(i * 60, 0).unwrap();
                Bar::new(time, 10.0, 10.0, 10.0, 10.0, 1.0)
            })
            .collect();
        let config = crate::drawings::VwapConfig {
            track_price: true,
            ..Default::default()
        };
        let mut drawing = Drawing::new(1, DrawingToolType::AnchoredVWAP);
        drawing.vwap = Some(config);
        drawing.chart_points = vec![crate::drawings::ChartPoint::new(80.0, 50.0)];
        // The price axis tags this value.
        let tracked = drawing.anchored_vwap_tracked(&bars, 0);
        assert_eq!(
            tracked.iter().map(|(price, _)| *price).collect::<Vec<_>>(),
            [10.0]
        );
        let shapes = render_vwap(&bars, 80.0, 1.0, LineStyle::Solid, Some(config));
        let line = shapes.iter().any(|shape| {
            matches!(shape, Shape::Mesh(mesh) if !mesh.vertices.is_empty()
                && mesh.vertices.iter().all(|v| (v.pos.y - 100.0).abs() <= 1.0)
                && mesh.vertices.iter().any(|v| v.pos.x < 5.0)
                && mesh.vertices.iter().any(|v| v.pos.x > 795.0))
        });
        assert!(line, "no latest-value line among {} shapes", shapes.len());
    }

    /// Paints an anchored VWAP of a blue drawing in `style` over `bars` from `anchor`, as the
    /// chart paints a drawing, 10 px a bar from x = 100 and 10 px a price from 20 at the top of an
    /// 800 by 600 pane.
    fn render_vwap(
        bars: &[Bar],
        anchor: f32,
        width: f32,
        style: LineStyle,
        config: Option<crate::drawings::VwapConfig>,
    ) -> Vec<Shape> {
        let mut drawing = Drawing::new(1, DrawingToolType::AnchoredVWAP);
        drawing.color = [41, 98, 255, 255];
        drawing.stroke_width = width;
        drawing.line_style = style;
        drawing.vwap = config;
        drawing.chart_points = vec![crate::drawings::ChartPoint::new(anchor, 50.0)];
        let view = ChartView {
            instrument: None,
            price_formatter: None,
            time_formatter: None,
            bar_seconds: 0,
            bars,
            revision: 0,
            bar_to_x: &|b| 100.0 + 10.0 * b,
            bar_spacing: 10.0,
            price_to_y: &|p| ((20.0 - p) * 10.0) as f32,
            y_to_price: &|y| 20.0 - f64::from(y) / 10.0,
            time_axis: true,
            executions: None,
            timeframe: None,
            finer: &[],
        };
        drawing.update_screen_coords(view.bar_to_x, view.price_to_y);
        let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            let pane = Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 600.0));
            let painter = egui::Painter::new(ui.ctx().clone(), egui::LayerId::background(), pane);
            drawing.render(&painter, pane, &view);
        });
        let shapes = std::mem::take(&mut output.shapes);
        output.drop_without_applying_deltas();
        shapes.into_iter().map(|shape| shape.shape).collect()
    }

    /// Reference: VWAP of (h + l + c) / 3 from bar 1, and the standard
    /// deviation from its definition, sqrt(sum v (tp - vwap)^2 / sum v). Every source and band
    /// unit follows the same definition from the bar's own prices, and each source gives its own
    /// VWAP on bars whose open, high, low and close differ.
    #[test]
    fn anchored_vwap_matches_reference() {
        use crate::drawings::{VwapBandMode, VwapConfig};
        let want = [
            (11.666666666666664, 0.0),
            (11.595238095238095, 0.08247860988423196),
            (12.166666666666666, 0.6201736729460428),
            (12.675925925925924, 0.9757431833967023),
            (12.775641025641026, 0.8255305309085764),
            (12.78395061728395, 0.8112060000493586),
        ];
        let bars = &sample_bars()[1..];
        let typical = VwapConfig::default();
        let got: Vec<_> = anchored_vwap(
            bars,
            &vwap_checkpoints(&[], typical.source),
            &typical,
            0..bars.len(),
        )
        .collect();
        assert_eq!(got.len(), want.len());
        for (got, (vwap, sigma)) in got.into_iter().zip(want) {
            let (got_vwap, got_sigma) = got.unwrap();
            assert_close(got_vwap, vwap);
            // One-pass variance cancels to about 1e-14, so its root is near 1e-7.
            assert!(
                (got_sigma - sigma).abs() < 1e-6,
                "sigma {got_sigma}, want {sigma}"
            );
        }

        // Windows that start inside a block or on a block boundary resume
        // from a checkpoint: compare with the definition over a series three
        // blocks long, the forming bar in the third block, for every source
        // and unit. The checkpoints of one source are never reused for another.
        let long: Vec<Bar> = (0..3 * VWAP_CHECKPOINT)
            .map(|i| {
                let close = 100.0 + (i * 37 % 101) as f64 / 10.0;
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                let volume = (1 + i % 7) as f64;
                Bar::new(time, close - 0.4, close + 1.0, close - 1.5, close, volume)
            })
            .collect();
        type Source = (PriceSource, fn(&Bar) -> f64);
        let sources: [Source; 7] = [
            (PriceSource::Open, |b| b.open),
            (PriceSource::High, |b| b.high),
            (PriceSource::Low, |b| b.low),
            (PriceSource::Close, |b| b.close),
            (PriceSource::HL2, |b| (b.high + b.low) / 2.0),
            (PriceSource::HLC3, |b| (b.high + b.low + b.close) / 3.0),
            (PriceSource::OHLC4, |b| {
                (b.open + b.high + b.low + b.close) / 4.0
            }),
        ];
        let mut finals = Vec::new();
        for (source, price) in sources {
            for mode in [VwapBandMode::Deviation, VwapBandMode::Percentage] {
                let config = VwapConfig {
                    source,
                    band_mode: mode,
                    ..VwapConfig::default()
                };
                let checkpoints = cached_checkpoints(source, 0, &long, 0);
                for window in [
                    0..4,
                    VWAP_CHECKPOINT + 5..VWAP_CHECKPOINT + 9,
                    2 * VWAP_CHECKPOINT..2 * VWAP_CHECKPOINT + 4,
                    long.len() - 1..long.len(),
                ] {
                    let got = anchored_vwap(&long, &checkpoints, &config, window.clone());
                    for (i, got) in window.zip(got) {
                        let prefix = &long[..=i];
                        let volume: f64 = prefix.iter().map(|b| b.volume).sum();
                        let vwap = prefix.iter().map(|b| price(b) * b.volume).sum::<f64>() / volume;
                        let unit = match mode {
                            VwapBandMode::Deviation => (prefix
                                .iter()
                                .map(|b| b.volume * (price(b) - vwap).powi(2))
                                .sum::<f64>()
                                / volume)
                                .sqrt(),
                            VwapBandMode::Percentage => vwap / 100.0,
                        };
                        let (got_vwap, got_unit) = got.unwrap();
                        let row = format!("{source:?} {mode:?} bar {i}");
                        assert!((got_vwap - vwap).abs() < 1e-9, "{row}: {got_vwap} {vwap}");
                        assert!((got_unit - unit).abs() < 1e-6, "{row}: {got_unit} {unit}");
                        if i == long.len() - 1 && mode == VwapBandMode::Deviation {
                            finals.push(got_vwap);
                        }
                    }
                }
            }
        }
        for (i, a) in finals.iter().enumerate() {
            assert!(
                finals[i + 1..].iter().all(|b| (a - b).abs() > 1e-3),
                "{finals:?}"
            );
        }
    }

    /// Rows retain their gaps on either scale, leave two thirds of the span clear,
    /// and anchored rows grow left from the pane edge independently of the range tint.
    #[test]
    fn volume_profile_rows_are_drawn_on_either_scale_orientation() {
        use crate::data::{ProfileDisplay, VolumeProfileConfig};
        let bars = sample_bars();
        let upright = |price: f64| (16.0 - price) as f32 * 50.0;
        let inverted = |price: f64| (price - 8.0) as f32 * 50.0;
        let clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 600.0));
        for tool in [
            DrawingToolType::FixedRangeVolumeProfile,
            DrawingToolType::AnchoredVolumeProfile,
        ] {
            for price_to_y in [&upright as &dyn Fn(f64) -> f32, &inverted] {
                for (width, row_count, row_height, display, va_lines) in [
                    (400.0, 24, 11.5, ProfileDisplay::Total, false),
                    (24.0, 24, 11.5, ProfileDisplay::Total, false),
                    (24.0, 4, 74.0, ProfileDisplay::UpDown, true),
                    (1200.0, 24, 11.5, ProfileDisplay::Total, false),
                    (400.0, 1000, 0.15, ProfileDisplay::Total, false),
                    (400.0, 4, 74.0, ProfileDisplay::UpDown, false),
                    (400.0, 4, 74.0, ProfileDisplay::UpDown, true),
                ] {
                    let mut drawing = Drawing::new(1, tool);
                    let mut config = VolumeProfileConfig::new(row_count, 0.7).unwrap();
                    config.display = display;
                    config.show_value_area_lines = va_lines;
                    drawing.volume_profile = Some(config);
                    let view = ChartView {
                        price_formatter: None,
                        time_formatter: None,
                        bar_seconds: 0,
                        instrument: None,
                        bars: &bars,
                        revision: 0,
                        bar_to_x: &|x| x,
                        bar_spacing: 1.0,
                        price_to_y,
                        y_to_price: &|y| {
                            8.0 + f64::from(y - price_to_y(8.0))
                                / f64::from(price_to_y(9.0) - price_to_y(8.0))
                        },
                        time_axis: true,
                        executions: None,
                        timeframe: None,
                        finer: &[],
                    };
                    let mut output =
                        egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                            let painter = egui::Painter::new(
                                ui.ctx().clone(),
                                egui::LayerId::background(),
                                clip,
                            );
                            drawing.draw_volume_profile(
                                &painter,
                                &view,
                                0,
                                &bars,
                                100.0,
                                100.0 + width,
                            );
                        });
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    let rects: Vec<_> = shapes
                        .iter()
                        .filter_map(|c| match &c.shape {
                            egui::Shape::Rect(r) => Some(r),
                            _ => None,
                        })
                        .collect();
                    let tint = rects[0];
                    assert_eq!(
                        tint.rect,
                        Rect::from_min_max(Pos2::new(100.0, 50.0), Pos2::new(100.0 + width, 350.0))
                    );
                    assert!(tint.stroke.is_empty(), "range has no outline");
                    assert!(
                        tint.fill.a() > 0 && tint.fill.a() < 40,
                        "range has a faint tint"
                    );
                    let rows = &rects[1..];
                    let split = display == ProfileDisplay::UpDown;
                    assert_eq!(rows.len(), row_count * if split { 2 } else { 1 });
                    let anchored = tool == DrawingToolType::AnchoredVolumeProfile;
                    let edge = if anchored { 800.0 } else { 100.0 };
                    let mut widest = 0.0_f32;
                    for row in rows.chunks(if split { 2 } else { 1 }) {
                        let mut row_width = 0.0;
                        for segment in row {
                            assert!(segment.stroke.is_empty());
                            assert!((segment.rect.height() - row_height).abs() < 1e-3);
                            row_width += segment.rect.width();
                        }
                        assert!(
                            ((if anchored {
                                row[0].rect.right()
                            } else {
                                row[0].rect.left()
                            }) - edge)
                                .abs()
                                < 1e-4
                        );
                        widest = widest.max(row_width);
                    }
                    let span_width = if anchored { clip.width() } else { width };
                    assert!(
                        (widest - span_width * 0.3).abs() < 1e-3,
                        "widest row {widest}"
                    );
                    if split {
                        // Four 1.5-price rows have totals 83 1/3, 350, 619 1/6, 397.5.
                        // 70% of 1450 needs rows 0..=2; row 3 is outside the value area.
                        for side in 0..2 {
                            for row in 0..3 {
                                assert!(rows[row * 2 + side].fill.a() > rows[6 + side].fill.a());
                            }
                        }
                    }
                    let mut lines = Vec::new();
                    let mut labels = 0;
                    let mut notes = 0;
                    for shape in &shapes {
                        match &shape.shape {
                            egui::Shape::LineSegment { points, .. } => lines.push(points.to_vec()),
                            egui::Shape::Path(path) => {
                                assert_eq!(path.stroke.width, 2.0, "POC remains clear");
                                lines.push(path.points.clone());
                            }
                            egui::Shape::Text(text) => {
                                let label = text.galley.job.text.as_str();
                                assert_ne!(label, "POC");
                                if label == "Estimated from bars" {
                                    notes += 1;
                                    // The tint's top left corner on either scale, inset 3 points.
                                    assert_eq!(text.pos, Pos2::new(103.0, 50.0));
                                }
                                if ["VAH", "VAL"].contains(&label) {
                                    assert!(va_lines);
                                    labels += 1;
                                    let away = if price_to_y(15.0) < price_to_y(9.0) {
                                        8.0
                                    } else {
                                        -8.0
                                    };
                                    let want = if label == "VAH" {
                                        price_to_y(13.5) - away
                                    } else {
                                        price_to_y(9.0) + away
                                    };
                                    assert!(
                                        (text.pos.y + text.galley.size().y / 2.0 - want).abs()
                                            < 1.0
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                    assert_eq!(lines.len(), if va_lines { 3 } else { 1 });
                    // The point of control, painted first, crosses the pane when anchored and
                    // stays on the range otherwise.
                    let reach_of_line = if anchored {
                        [clip.left(), clip.right()]
                    } else {
                        [100.0, 100.0 + width]
                    };
                    assert_eq!([lines[0][0].x, lines[0][1].x], reach_of_line);
                    // Labels need 50 px between the first anchor and the rows' far edge.
                    let reach = if anchored { 800.0 - 100.0 } else { width };
                    assert_eq!(labels, if va_lines && reach >= 50.0 { 2 } else { 0 });
                    // The note fits in every span but the 24-point one.
                    assert_eq!(notes, usize::from(width > 24.0));
                    assert!(
                        lines
                            .iter()
                            .all(|points| points.len() == 2 && points[0].y == points[1].y)
                    );
                }
            }
        }
    }

    /// A profile covers its bars whole, half a bar past each end, and the anchors of the volume
    /// profiles only choose bars: their handles sit on those bars, at the range's top or the
    /// anchored point of control. An extended fixed range reads from its earlier anchor.
    #[test]
    fn extended_fixed_range_reads_from_its_earlier_anchor() {
        use crate::data::VolumeProfileConfig;
        use crate::drawings::ChartPoint;
        let bars: Vec<_> = [(10.0, 1.0), (12.0, 1.0), (20.0, 1.0), (30.0, 9.0)]
            .into_iter()
            .enumerate()
            .map(|(i, (price, volume))| {
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                Bar::new(time, price, price + 1.0, price, price, volume)
            })
            .collect();
        let bar_to_x = |b: f32| 100.0 + 10.0 * b;
        let view = ChartView {
            price_formatter: None,
            time_formatter: None,
            bar_seconds: 0,
            instrument: None,
            bars: &bars,
            revision: 0,
            bar_to_x: &bar_to_x,
            bar_spacing: 10.0,
            price_to_y: &|p| 500.0 - p as f32 * 10.0,
            y_to_price: &|y| f64::from(500.0 - y) / 10.0,
            time_axis: true,
            executions: None,
            timeframe: None,
            finer: &[],
        };
        let config = |extend_right| {
            let mut config = VolumeProfileConfig::new(4, 0.7).unwrap();
            config.extend_right = extend_right;
            Some(config)
        };
        // Anchors, whether extended, the tint's ends, the handles' bars and their price: the top of
        // the bars read, or the middle of the anchored profile's last row, which holds bar 3's
        // volume of 9.
        for (tool, anchors, extended, ends, handles, price, volume) in [
            (
                DrawingToolType::FixedRangeVolumeProfile,
                vec![0.2, 2.0],
                false,
                [95.0, 125.0],
                vec![0.0, 2.0],
                21.0,
                3.0,
            ),
            (
                DrawingToolType::FixedRangeVolumeProfile,
                vec![2.0, 0.2],
                false,
                [95.0, 125.0],
                vec![2.0, 0.0],
                21.0,
                3.0,
            ),
            (
                DrawingToolType::FixedRangeVolumeProfile,
                vec![0.2, 2.0],
                true,
                [95.0, 135.0],
                vec![0.0, 2.0],
                31.0,
                12.0,
            ),
            // An extended fixed range reads from its earlier anchor in either placement order.
            (
                DrawingToolType::FixedRangeVolumeProfile,
                vec![3.0, 0.2],
                true,
                [95.0, 135.0],
                vec![3.0, 0.0],
                31.0,
                12.0,
            ),
            (
                DrawingToolType::FixedRangeVolumeProfile,
                vec![0.2, 9.0],
                false,
                [95.0, 195.0],
                vec![0.0, 9.0],
                31.0,
                12.0,
            ),
            (
                DrawingToolType::AnchoredVolumeProfile,
                vec![0.2],
                false,
                [95.0, 135.0],
                vec![0.0],
                10.0 + 21.0 * 3.5 / 4.0,
                12.0,
            ),
        ] {
            let mut drawing = Drawing::new(1, tool);
            drawing.volume_profile = config(extended);
            drawing.chart_points = anchors.iter().map(|&b| ChartPoint::new(b, 5.0)).collect();
            let profile = drawing.volume_profile_of(&view).expect("profile");
            // The selected bars contribute 1 + 1 + 1, or 1 + 1 + 1 + 9 when bar 3 is read.
            assert_close(profile.volume().iter().sum(), volume);
            assert_close(profile.low(), 10.0);
            let (_, start) = drawing
                .profile_history(
                    &bars,
                    Timeframe::Min1,
                    &[ProfileResolution::new(Timeframe::Sec1, None)],
                )
                .expect("finer history");
            assert_eq!(start, bars[0].time, "{tool:?} {anchors:?}");
            let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                if tool == DrawingToolType::AnchoredVolumeProfile {
                    drawing.render_anchored_volume_profile(ui.painter(), &view);
                } else {
                    drawing.render_fixed_range_volume_profile(ui.painter(), &view);
                }
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let tint = shapes
                .iter()
                .find_map(|c| match &c.shape {
                    egui::Shape::Rect(r) => Some(r.rect),
                    _ => None,
                })
                .expect("tint");
            assert_eq!([tint.left(), tint.right()], ends, "{tool:?} {anchors:?}");
            let got = drawing.volume_tool_handles(&view).expect("handles");
            let want: Vec<_> = handles
                .iter()
                .map(|&b| Pos2::new(bar_to_x(b), 500.0 - price as f32 * 10.0))
                .collect();
            assert_eq!(got, want, "{tool:?} {anchors:?}");
        }
        // While placing an extended range, its sole anchor is still the start.
        for (anchors, extended, span) in [
            (vec![2.0], true, Some((2, 2))),
            (vec![2.0], false, None),
            (Vec::new(), true, None),
        ] {
            let mut drawing = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
            drawing.volume_profile = config(extended);
            drawing.chart_points = anchors.iter().map(|&b| ChartPoint::new(b, 5.0)).collect();
            assert_eq!(
                drawing
                    .profile_span(&bars)
                    .map(|(first, bars)| (first, bars.len())),
                span,
            );
        }
    }

    /// The handle follows the first plotted VWAP value across zero volume and history bounds.
    #[test]
    fn vwap_handle_sits_on_the_first_point_of_its_line() {
        use crate::drawings::ChartPoint;
        let bars: Vec<_> = [(10.0, 1.0), (12.0, 1.0), (20.0, 1.0), (30.0, 9.0)]
            .into_iter()
            .enumerate()
            .map(|(i, (price, volume))| {
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                Bar::new(time, price, price + 1.0, price, price, volume)
            })
            .collect();
        let bar_to_x = |b: f32| 100.0 + 10.0 * b;
        let view = ChartView {
            price_formatter: None,
            time_formatter: None,
            bar_seconds: 0,
            instrument: None,
            bars: &bars,
            revision: 0,
            bar_to_x: &bar_to_x,
            bar_spacing: 10.0,
            price_to_y: &|p| 500.0 - p as f32 * 10.0,
            y_to_price: &|y| f64::from(500.0 - y) / 10.0,
            time_axis: true,
            executions: None,
            timeframe: None,
            finer: &[],
        };
        // A VWAP starts at the first bar with positive cumulative volume, even if the
        // anchor is outside loaded history. The expected prices are (high + low + close) / 3.
        for (name, volumes, anchor, first) in [
            (
                "zero anchor volume",
                [0.0, 1.0, 1.0, 9.0],
                0.0,
                Some((1, 37.0 / 3.0)),
            ),
            (
                "rounded anchor",
                [1.0, 1.0, 1.0, 9.0],
                1.2,
                Some((1, 37.0 / 3.0)),
            ),
            (
                "before history",
                [1.0, 1.0, 1.0, 9.0],
                -2.0,
                Some((0, 31.0 / 3.0)),
            ),
            (
                "before zero volume",
                [0.0, 0.0, 1.0, 9.0],
                -2.0,
                Some((2, 61.0 / 3.0)),
            ),
            (
                "later zero volume",
                [1.0, 1.0, 0.0, 9.0],
                2.0,
                Some((3, 91.0 / 3.0)),
            ),
            ("no volume", [0.0; 4], 0.0, None),
            ("after history", [1.0, 1.0, 1.0, 9.0], 9.0, None),
        ] {
            let mut bars = bars.clone();
            for (bar, volume) in bars.iter_mut().zip(volumes) {
                bar.volume = volume;
            }
            let view = ChartView {
                bars: &bars,
                ..view
            };
            let mut vwap = Drawing::new(1, DrawingToolType::AnchoredVWAP);
            vwap.chart_points = vec![ChartPoint::new(anchor, 5.0)];
            let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                let painter = egui::Painter::new(
                    ui.ctx().clone(),
                    egui::LayerId::background(),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 600.0)),
                );
                vwap.render_anchored_vwap(&painter, &view);
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            let point = shapes.last().map(|shape| {
                let Shape::Path(line) = &shape.shape else {
                    panic!("VWAP line")
                };
                line.points[0]
            });
            let expected =
                first.map(|(bar, price)| Pos2::new(bar_to_x(bar as f32), (view.price_to_y)(price)));
            assert_eq!(point, expected, "{name}: line start");
            assert_eq!(
                vwap.volume_tool_handles(&view),
                point.map(|p| vec![p]),
                "{name}: handle"
            );
            for (theme, width) in [
                (crate::theme::Theme::light(), 800.0),
                (crate::theme::Theme::dark(), 800.0),
                (crate::theme::Theme::light(), 320.0),
                (crate::theme::Theme::dark(), 320.0),
            ] {
                let mut manager = crate::drawings::DrawingManager::new();
                manager.drawings.push(vwap.clone());
                manager.update_all_screen_coords(bar_to_x, view.price_to_y, 10.0);
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(width, 600.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        manager.render_all(ui.painter(), ui.clip_rect(), &view);
                    },
                )
                .drop_without_applying_deltas();
                let drawing = &manager.drawings[0];
                let (handle, position) = manager.get_handles(drawing)[0];
                assert_eq!(position, point.unwrap_or(drawing.points[0]), "{name}");
                assert_eq!(manager.hit_test_handle(drawing, position), Some(handle));
                if point.is_some() {
                    let before = drawing.chart_points.clone();
                    manager.start_drag_handle(1, handle);
                    manager.update_drag_handle(
                        position + egui::vec2(0.0, 10.0),
                        |x| (x - 100.0) / 10.0,
                        view.y_to_price,
                        |p, from, to| p - f64::from(to - from) / 10.0,
                    );
                    assert_eq!(
                        manager.drawings[0].chart_points, before,
                        "{name}: vertical drag"
                    );
                    manager.update_drag_handle(
                        position + egui::vec2(10.0, 0.0),
                        |x| (x - 100.0) / 10.0,
                        view.y_to_price,
                        |p, from, to| p - f64::from(to - from) / 10.0,
                    );
                    manager.end_drag_handle();
                    assert_eq!(
                        manager.drawings[0].chart_points[0].bar_idx,
                        (position.x - 100.0) / 10.0 + 1.0,
                        "{name}: horizontal drag"
                    );
                    assert!(manager.undo());
                    assert_eq!(manager.drawings[0].chart_points, before, "{name}: undo");
                }
            }
        }
    }

    /// The profile of a span that ends on the forming bar follows its ticks
    /// and the next bar, extending the cached profile of the bars before it
    /// while they trade inside its range and rebuilding the rows otherwise.
    /// Either way it is the profile built from all the bars.
    #[test]
    fn span_profile_follows_the_forming_bar() {
        let check = |bars: &[Bar], case: &str| {
            let view = ChartView {
                price_formatter: None,
                time_formatter: None,
                bar_seconds: 0,
                instrument: None,
                bars,
                revision: 0,
                bar_to_x: &|x| x,
                bar_spacing: 1.0,
                price_to_y: &|price| price as f32,
                y_to_price: &f64::from,
                time_axis: true,
                executions: None,
                timeframe: None,
                finer: &[],
            };
            let want = VolumeProfile::of(&bars[1..], 24, None);
            assert_eq!(
                *span_profile(&view, 24, None, 1, &bars[1..]),
                want,
                "{case}"
            );
        };
        let mut bars = sample_bars();
        check(&bars, "first frame");
        for (high, low, close, volume) in [
            (16.0, 12.0, 14.0, 150.0),
            (14.0, 12.5, 13.5, 90.0),
            (14.5, 12.0, 12.5, 130.0),
        ] {
            let forming = bars.last_mut().unwrap();
            (forming.high, forming.low, forming.close, forming.volume) = (high, low, close, volume);
            check(&bars, &format!("tick {low}..{high}"));
        }
        // The bar closes on a trade that was not drawn, and the next one opens.
        bars.last_mut().unwrap().volume += 10.0;
        let time = bars.last().unwrap().time + chrono::Duration::minutes(1);
        bars.push(Bar::new(time, 13.0, 13.5, 12.5, 13.2, 20.0));
        check(&bars, "new bar");
    }

    /// A selection over held executions shows their exact profile, as the public footprint
    /// aggregation gives it: totals, aggressor sides, POC and value area. Moving an anchor
    /// changes the selected span, and without executions the bar estimate is labelled as such.
    #[test]
    fn volume_profile_selects_executions_by_anchor() {
        use crate::data::{Footprint, ProfileSource};
        use crate::model::{BarClock, OrderSide, Tick, Timeframe};
        let bars = sample_bars();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        // Bar 1 trades 11 (4 sold), 12 (6 bought, 2 unknown); bar 2 trades 12 (3 bought).
        let trades: Vec<_> = [
            (1, 11.0, 4.0, Some(OrderSide::Sell)),
            (1, 12.0, 6.0, Some(OrderSide::Buy)),
            (1, 12.0, 2.0, None),
            (2, 12.0, 3.0, Some(OrderSide::Buy)),
        ]
        .into_iter()
        .map(|(bar, price, size, side)| Tick::new(bars[bar].time, price, size, side))
        .collect();
        for trade in &trades {
            flow.push(trade).unwrap();
        }
        let view = |executions| ChartView {
            price_formatter: None,
            time_formatter: None,
            bar_seconds: 0,
            instrument: None,
            bars: &bars,
            revision: 0,
            bar_to_x: &|x| x,
            bar_spacing: 1.0,
            price_to_y: &|price| price as f32,
            y_to_price: &f64::from,
            time_axis: true,
            executions,
            timeframe: None,
            finer: &[],
        };
        let mut drawing = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
        let select = |drawing: &mut Drawing, first, last| {
            drawing.chart_points = vec![
                crate::drawings::domain::ChartPoint::new(first, 10.0),
                crate::drawings::domain::ChartPoint::new(last, 10.0),
            ];
        };

        select(&mut drawing, 0.0, 2.0);
        let profile = drawing
            .volume_profile_of(&view(Some((&flow).into())))
            .unwrap();
        let exact = flow.profile(bars[0].time, bars[2].time);
        assert_eq!(
            profile.source(),
            ProfileSource::Executions { complete: false }
        );
        assert_eq!((profile.low(), profile.high()), (10.5, 12.5));
        assert_eq!(profile.volume(), [4.0, 11.0]);
        assert_eq!(
            (profile.up(), profile.down()),
            ([0.0, 9.0].as_slice(), [4.0, 0.0].as_slice())
        );
        assert_eq!(profile.volume().iter().sum::<f64>(), exact.volume());
        // 90% of 15 is 13.5, so the POC's 11 needs the 4 below it.
        let (poc, (low, high)) = profile.poc_and_value_area(0.9).unwrap();
        assert_eq!(exact.poc(), Some(12));
        assert_eq!(exact.value_area(0.9), Some((11, 12)));
        assert_eq!((poc, (low, high)), (1, (0, 1)));

        // Moving the first anchor past bar 1 drops its executions and the lower row.
        select(&mut drawing, 2.0, 2.0);
        let profile = drawing
            .volume_profile_of(&view(Some((&flow).into())))
            .unwrap();
        assert_eq!((profile.low(), profile.volume()), (11.5, [3.0].as_slice()));

        // A span with no executions, and a view without any, fall back to the estimate.
        select(&mut drawing, 4.0, 5.0);
        for executions in [Some((&flow).into()), None] {
            let profile = drawing.volume_profile_of(&view(executions)).unwrap();
            assert_eq!(profile.source(), ProfileSource::Estimated);
        }
        // Executions held from the span's first bar are complete.
        select(&mut drawing, 1.0, 2.0);
        let profile = drawing
            .volume_profile_of(&view(Some((&flow).into())))
            .unwrap();
        assert_eq!(
            profile.source(),
            ProfileSource::Executions { complete: true }
        );
        // Once the store drops the sell of 4, bar 1 is cut: the same span counts bar 2 alone
        // and is partial.
        let mut cut = flow.clone();
        cut.drop_through(trades[0].ts, &trades[1..]);
        let profile = drawing
            .volume_profile_of(&view(Some((&cut).into())))
            .unwrap();
        assert_eq!(
            profile.source(),
            ProfileSource::Executions { complete: false }
        );
        assert_eq!(profile.volume(), [3.0]);
        let mut gap = flow.clone();
        gap.add_missing(bars[2].time, bars[3].time);
        for (first, last, complete) in [(0.0, 1.0, false), (1.0, 1.0, true), (1.0, 2.0, false)] {
            select(&mut drawing, first, last);
            let profile = drawing
                .volume_profile_of(&view(Some((&gap).into())))
                .unwrap();
            assert_eq!(profile.source(), ProfileSource::Executions { complete });
        }
    }

    /// Finer bars replace only the chart bars they cover; executions still take precedence.
    /// An anchored span includes a new intrabar even inside the last chart bar.
    #[test]
    fn volume_profile_spreads_finer_bars() {
        use crate::data::{Footprint, ProfileSource, VolumeProfileConfig};
        use crate::drawings::ChartPoint;
        use crate::model::{BarClock, Instrument, OrderSide, Tick};
        use crate::studies::StudyData;
        let start: DateTime<Utc> = "2026-03-02T00:00:00Z".parse().unwrap();
        let daily: Vec<_> = (0..3)
            .map(|day| {
                Bar::new(
                    start + chrono::TimeDelta::days(day),
                    100.0,
                    110.0,
                    100.0,
                    105.0,
                    100.0,
                )
            })
            .collect();
        let minute = |day, minutes, low, high, volume| {
            Bar::new(
                start + chrono::TimeDelta::days(day) + chrono::TimeDelta::minutes(minutes),
                low,
                high,
                low,
                (low + high) / 2.0,
                volume,
            )
        };
        let fine = [
            minute(0, 870, 100.0, 101.0, 60.0),
            minute(0, 871, 109.0, 110.0, 40.0),
            minute(1, 870, 104.0, 106.0, 100.0),
            minute(2, 870, 102.0, 103.0, 50.0),
        ];
        let mut flow = Footprint::new(BarClock::new(Timeframe::Day1), 1.0, 1).unwrap();
        flow.push(&Tick::new(fine[0].time, 100.0, 5.0, Some(OrderSide::Buy)))
            .unwrap();
        let rows = |profile: &VolumeProfile| {
            (
                profile.low(),
                profile.high(),
                profile.volume().to_vec(),
                profile.up().to_vec(),
                profile.down().to_vec(),
                profile.poc_and_value_area(0.7),
            )
        };
        // Whole daily bars have volume 100 each. The first day has 60 + 40 finer volume,
        // the second 100, and the new bar of day 3 contributes 50 in place of its daily 100.
        for (name, anchored, last, count, max_days, executed, inputs, total) in [
            ("fixed", false, 1.0, 3, 10, false, fine[..3].to_vec(), 200.0),
            (
                "partial",
                false,
                2.0,
                3,
                10,
                false,
                vec![
                    fine[0].clone(),
                    fine[1].clone(),
                    fine[2].clone(),
                    daily[2].clone(),
                ],
                300.0,
            ),
            (
                "anchored before",
                true,
                2.0,
                3,
                10,
                false,
                vec![fine[2].clone(), daily[2].clone()],
                200.0,
            ),
            (
                "anchored after",
                true,
                2.0,
                4,
                10,
                false,
                fine[2..].to_vec(),
                150.0,
            ),
            (
                "span too long",
                false,
                1.0,
                3,
                1,
                false,
                daily[..2].to_vec(),
                200.0,
            ),
            ("executions", false, 1.0, 3, 10, true, Vec::new(), 5.0),
        ] {
            let instrument =
                Instrument::new("A").with_profile_resolutions([ProfileResolution::new(
                    Timeframe::Min1,
                    Some(chrono::TimeDelta::days(max_days)),
                )]);
            let history = [StudyData {
                request: StudyDataRequest {
                    symbol: "A".into(),
                    timeframe: Some(Timeframe::Min1),
                    metric: None,
                },
                bars: &fine[..count],
                clock: BarClock::new(Timeframe::Min1),
                instrument: None,
                timeframe: Timeframe::Min1,
            }];
            let view = ChartView {
                instrument: Some(&instrument),
                price_formatter: None,
                time_formatter: None,
                bar_seconds: 0,
                bars: &daily,
                revision: 0,
                bar_to_x: &|x| x,
                bar_spacing: 1.0,
                price_to_y: &|price| price as f32,
                y_to_price: &f64::from,
                time_axis: true,
                executions: executed.then_some((&flow).into()),
                timeframe: Some(Timeframe::Day1),
                finer: &history,
            };
            let mut drawing = Drawing::new(
                1,
                if anchored {
                    DrawingToolType::AnchoredVolumeProfile
                } else {
                    DrawingToolType::FixedRangeVolumeProfile
                },
            );
            drawing.chart_points = if anchored {
                vec![ChartPoint::new(1.0, 100.0)]
            } else {
                vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(last, 100.0)]
            };
            let profile = drawing.volume_profile_of(&view).unwrap();
            if executed {
                assert_eq!(
                    profile.source(),
                    ProfileSource::Executions { complete: true },
                    "{name}"
                );
                assert_eq!((profile.low(), profile.high()), (99.5, 100.5));
                assert_eq!(profile.volume(), [5.0]);
            } else {
                let expected =
                    VolumeProfile::of(&inputs, VolumeProfileConfig::default().rows(), None)
                        .unwrap();
                assert_eq!(rows(&profile), rows(&expected), "{name}");
                assert_eq!(profile.source(), ProfileSource::Estimated, "{name}");
            }
            assert!(
                (profile.volume().iter().sum::<f64>() - total).abs() < 1e-9,
                "{name}"
            );
        }
    }
}
