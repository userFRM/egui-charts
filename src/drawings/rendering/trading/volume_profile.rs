//! Volume tools: anchored VWAP and the fixed range and anchored volume profiles

use crate::data::{
    FootprintBarView, FootprintView, ProfileSource, StudyDataRequest, VolumeProfile,
    VolumeProfileConfig,
};
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
    /// Lowest low and highest high of the spans whose row count is laid on whole ticks.
    static PRICE_SPANS: RefCell<SpanCache<Option<(f64, f64)>>> =
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
        for (price, line) in self.anchored_vwap_tracked(view.bars, view.revision, view.executions) {
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
        for (i, value) in (from..to).zip(anchored_vwap(
            bars,
            &checkpoints,
            &config,
            view.executions,
            from..to,
        )) {
            let x = (view.bar_to_x)((first + i) as f32);
            for (line, k) in lines.iter_mut().zip(&offsets) {
                line.push(value.map_or(Pos2::NAN, |(vwap, unit)| {
                    Pos2::new(x, (view.price_to_y)(vwap + k * unit))
                }));
            }
        }
        if !lines[0].iter().any(|point| point.is_finite()) {
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
            let fill = |upper: &[Pos2], lower: &[Pos2]| {
                super::super::fibonacci::paint_band(
                    painter,
                    upper,
                    lower,
                    color.gamma_multiply(config.band_fill.opacity()),
                );
            };
            if config.bands[0].upper.plot_type.has_breaks()
                || config.bands[0].lower.plot_type.has_breaks()
            {
                for (upper, lower) in lines[1]
                    .split(|p| !p.is_finite())
                    .zip(lines[2].split(|p| !p.is_finite()))
                {
                    if !upper.is_empty() {
                        fill(upper, lower);
                    }
                }
            } else {
                lines[1].retain(|p| p.is_finite());
                lines[2].retain(|p| p.is_finite());
                fill(&lines[1], &lines[2]);
            }
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

    /// The values at the last of `bars`, the pane's bars at `revision`, with the chart's
    /// `executions`, of the anchored VWAP's tracked lines, each with its style: the VWAP while
    /// its line shows, in the drawing's colour, then the shown band lines of the enabled bands.
    /// Empty for other drawings, before volume trades and while the last bar has no value.
    pub(crate) fn anchored_vwap_tracked(
        &self,
        bars: &[Bar],
        revision: u64,
        executions: Option<FootprintView<'_>>,
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
            anchored_vwap(bars, &checkpoints, &config, executions, last..bars.len()).next()?
        };
        let Some((vwap, unit)) = latest() else {
            return Vec::new();
        };
        tracked
            .into_iter()
            .map(|(k, line)| (vwap + k * unit, line))
            .collect()
    }

    /// Bars from this anchored VWAP's anchor that trade but use their source price because
    /// their executions are not held in full, and all bars from the anchor. `None` for other
    /// drawings, one that does not count executions, or without bars from its anchor.
    #[cfg(any(feature = "ui", test))]
    pub(crate) fn anchored_vwap_estimated(
        &self,
        bars: &[Bar],
        executions: Option<FootprintView<'_>>,
    ) -> Option<(usize, usize)> {
        if self.tool_type != DrawingToolType::AnchoredVWAP
            || !self.vwap.unwrap_or_default().executions
        {
            return None;
        }
        let (_, span) = bars_between(bars, self.chart_points.first()?.bar_idx, f32::INFINITY)?;
        let estimated = span
            .iter()
            .filter(|bar| {
                bar.volume > 0.0
                    && executions
                        .and_then(|flow| executed_bar(flow, bar))
                        .is_none()
            })
            .count();
        Some((estimated, span.len()))
    }

    /// Whether a visible drawing reads executions: a profile, or an anchored VWAP set to count
    /// them.
    pub(crate) fn reads_executions(&self) -> bool {
        self.reads_profile()
            || (self.visible
                && self.tool_type == DrawingToolType::AnchoredVWAP
                && self.vwap.is_some_and(|config| config.executions))
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

    /// Shown bars whose executed rows this drawing reads.
    pub(crate) fn execution_span<'b>(&self, bars: &'b [Bar]) -> Option<&'b [Bar]> {
        if !self.reads_executions() {
            return None;
        }
        let (_, span) = if self.tool_type == DrawingToolType::AnchoredVWAP {
            bars_between(bars, self.chart_points.first()?.bar_idx, f32::INFINITY)?
        } else {
            self.profile_span(bars)?
        };
        Some(span)
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
    /// the last, with the point of control and the value area and, where its switches show them,
    /// the developing point of control and value area.
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
    /// last bar and, where its switches show them, the developing point of control and value area.
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
            let config = self.vwap.unwrap_or_default();
            let executions = view.executions.filter(|_| config.executions);
            let mut sums = VwapSums::default();
            let (i, (vwap, _)) = bars.iter().enumerate().find_map(|(i, bar)| {
                sums.add_bar(bar, config.source, executions)
                    .then(|| sums.value(VwapBandMode::Deviation))
                    .flatten()
                    .map(|value| (i, value))
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
    /// with a tint over the measured bars' price and time range and, where its switches show
    /// them, the developing point of control and value area over the bars.
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
                volume_decimals: view
                    .instrument
                    .and_then(|instrument| instrument.volume_decimals),
            },
            view.price_to_y,
        );
        if config.show_developing_poc || config.show_developing_value_area {
            let levels = self.developing_levels(view, first, bars, profile);
            paint_developing(painter, view, first, profile, &levels, config);
        }
    }

    /// The drawing's row layout on the view's instrument: rows of its ticks per row where the
    /// instrument has a tick size, else its row count or price height, the count as asked for:
    /// [`Self::span_profile`] lays it on whole ticks.
    fn profile_config(&self, view: &ChartView) -> VolumeProfileConfig {
        let config = self.volume_profile.unwrap_or_default();
        config
            .tick_height(view.instrument.and_then(|instrument| instrument.tick_size))
            .and_then(|height| config.with_row_size(Some(height)))
            .unwrap_or(config)
    }

    /// The profile of `bars`, the span that starts at bar `first`: the executed volume in the
    /// drawing's rows when the view holds complete executions in the span, else the finer bars
    /// where they cover chart bars, else estimated from the chart bars, a single one of which is
    /// up volume only when it closes above its open. A row count takes whole ticks of the view's
    /// instrument, as the volume profile studies lay theirs: from the price span of `bars` for
    /// bars and finer bars, and from the traded rows for executions.
    fn span_profile(
        &self,
        view: &ChartView,
        first: usize,
        bars: &[Bar],
    ) -> Rc<Option<VolumeProfile>> {
        let config = self.profile_config(view);
        // A row count is laid on whole ticks where the instrument has a tick size.
        let tick = view
            .instrument
            .and_then(|instrument| instrument.tick_size)
            .filter(|tick| config.row_size().is_none() && tick.is_finite() && *tick > 0.0);
        if let (Some(flow), Some(start), Some(end)) = (view.executions, bars.first(), bars.last()) {
            let executed = flow.profile(start.time, end.time);
            let complete = flow.span_is_complete(start.time, end.time);
            let levels = executed.levels.keys();
            let config = match (tick, levels.clone().next(), levels.clone().next_back()) {
                (Some(_), Some(&low), Some(&high)) => {
                    config.with_whole_rows(flow.price_step(), low as f64, high as f64 + 1.0)
                }
                _ => config,
            };
            if let Some(profile) = VolumeProfile::of_executions(
                &executed,
                &config,
                flow.price_step(),
                flow.tick_size(),
                complete,
            ) {
                return Rc::new(Some(profile));
            }
        }
        let span = tick.and_then(|tick| Some((tick, cached_price_span(view, first, bars)?)));
        let config = match span {
            Some((tick, (low, high))) => config.with_whole_rows(tick, low / tick, high / tick),
            None => config,
        };
        if let Some(profile) = finer_profile(view, config.rows(), config.row_size(), first, bars) {
            return profile;
        }
        let profile = span_profile(view, config.rows(), config.row_size(), first, bars);
        // One bar is up volume only when it closes above its open, several also at an unchanged
        // close. Applied here, not in the cache, whose profile of one bar later bars extend.
        if let [bar] = bars
            && bar.close == bar.open
            && let Some(estimate) = &*profile
        {
            let mut estimate = estimate.clone();
            estimate.count_single_bar(bar);
            return Rc::new(Some(estimate));
        }
        profile
    }

    /// The developing point of control and value area after each of `bars`, the span that starts
    /// at bar `first`, in the rows of `profile`, the span's profile, from the inputs it was built
    /// from: each bar's executions for an executed profile, else the finer bars inside each bar
    /// where the view holds them, else the bar.
    fn developing_levels(
        &self,
        view: &ChartView,
        first: usize,
        bars: &[Bar],
        profile: &VolumeProfile,
    ) -> Vec<Option<(usize, (usize, usize))>> {
        // shortcut: recomputed each frame, bars times rows; cache it by span as PROFILES does if
        // long spans with fine rows stall frames.
        let share = self.volume_profile.unwrap_or_default().value_area();
        if let ProfileSource::Executions { .. } = profile.source() {
            return view.executions.map_or_else(Vec::new, |flow| {
                profile.developing_executions(
                    share,
                    flow.price_step(),
                    bars.iter().map(|bar| flow.profile(bar.time, bar.time)),
                )
            });
        }
        match finer_inputs(view, first, bars) {
            Some((_, _, inside, end)) => profile.developing_estimate(
                share,
                VolumeProfile::finer_parts(bars, end, inside)
                    .zip(bars)
                    .map(|(part, bar)| part.unwrap_or(std::slice::from_ref(bar))),
            ),
            None => profile.developing_estimate(share, bars.iter().map(std::slice::from_ref)),
        }
    }

    /// The profile that a fixed range or anchored volume profile shows over the view's bars,
    /// with its [source](VolumeProfile::source): executed volume, in the drawing's rows as the
    /// volume profile studies group executions, when [`ChartView::executions`] holds complete
    /// bars in the selected span, including prepared bars before a missing join, else the finer
    /// bars of [`ChartView::finer`], else the bar estimate. An estimate from a single chart bar
    /// counts it as up volume only when it closes above its open, so a bar that closes at its open
    /// is down volume; over two or more bars an unchanged close is up. Rows set in ticks take the
    /// tick size of [`ChartView::instrument`], or the row count without one. A row count draws the
    /// whole number of those ticks per row whose row total lies closest to it, on multiples of
    /// that height, and equal rows without a tick size.
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
    /// Adds the bar at its source price; non-finite price or volume adds nothing and has no value.
    fn add(&mut self, bar: &Bar, source: PriceSource) -> bool {
        let price = source.compute(bar.open, bar.high, bar.low, bar.close);
        if !price.is_finite() || !bar.volume.is_finite() {
            return false;
        }
        self.volume += bar.volume;
        self.price_volume += price * bar.volume;
        self.square_volume += price * price * bar.volume;
        true
    }

    /// Adds a bar's executions at each row's middle tick price.
    fn add_executions(&mut self, rows: FootprintBarView<'_>, step: f64, tick_size: f64) {
        let offset = (step - tick_size) / 2.0;
        for (&level, row) in rows.levels.iter() {
            let (price, volume) = (level as f64 * step + offset, row.volume());
            self.volume += volume;
            self.price_volume += price * volume;
            self.square_volume += price * price * volume;
        }
    }

    /// Adds complete executions of the bar when held, else its source price and volume.
    /// Returns whether the bar has a value.
    fn add_bar(
        &mut self,
        bar: &Bar,
        source: PriceSource,
        executions: Option<FootprintView<'_>>,
    ) -> bool {
        match executions.and_then(|flow| Some((flow, executed_bar(flow, bar)?))) {
            Some((flow, rows)) => {
                self.add_executions(rows, flow.price_step(), flow.tick_size());
                true
            }
            None => self.add(bar, source),
        }
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

/// The executions of `bar` when all are held and finite positive volume traded there.
fn executed_bar<'a>(flow: FootprintView<'a>, bar: &Bar) -> Option<FootprintBarView<'a>> {
    if !flow.span_is_complete(bar.time, bar.time) {
        return None;
    }
    flow.complete_bar_at(bar.time)
        .filter(|rows| rows.volume() > 0.0 && rows.volume().is_finite())
}

/// First bar that can open inside the held executions; earlier bars always use their source.
fn first_held(bars: &[Bar], flow: FootprintView<'_>) -> usize {
    flow.covered_from().map_or(bars.len(), |held| {
        bars.partition_point(|bar| bar.time < held)
    })
}

/// VWAP of `config`'s source over all of `bars`, from the anchor bar, with its band unit, as
/// the painted line has it at the last of `bars`; `None` until volume trades and at a bar whose
/// source price or volume is not finite.
pub(crate) fn anchored_vwap_through(
    bars: &[Bar],
    config: &VwapConfig,
    executions: Option<FootprintView<'_>>,
) -> Option<(f64, f64)> {
    let executions = executions.filter(|_| config.executions);
    let mut sums = VwapSums::default();
    let mut counted = false;
    for bar in bars {
        counted = sums.add_bar(bar, config.source, executions);
    }
    if counted {
        sums.value(config.band_mode)
    } else {
        None
    }
}

/// The running sums at the start of the span and after every block of
/// `VWAP_CHECKPOINT` bars of `prefix`, the span's leading whole blocks, so the
/// VWAP at any bar takes fewer than `VWAP_CHECKPOINT` further additions.
fn vwap_checkpoints(prefix: &[Bar], source: PriceSource) -> Vec<VwapSums> {
    let mut sums = VwapSums::default();
    let mut checkpoints = vec![sums];
    for block in prefix.chunks(VWAP_CHECKPOINT) {
        block.iter().for_each(|bar| {
            sums.add(bar, source);
        });
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
    executions: Option<FootprintView<'a>>,
    range: Range<usize>,
) -> impl Iterator<Item = Option<(f64, f64)>> + 'a {
    let (source, mode) = (config.source, config.band_mode);
    let executions = executions.filter(|_| config.executions);
    // Bar checkpoints serve the prefix before held executions; later bars are summed each frame.
    // shortcut: one pass over held bars each frame; cache mixed checkpoints by footprint revision
    // if held histories grow past tens of thousands of bars.
    let split = range
        .start
        .min(executions.map_or(bars.len(), |flow| first_held(bars, flow)));
    let checkpoint = split / VWAP_CHECKPOINT;
    let mut sums = checkpoints[checkpoint];
    bars[checkpoint * VWAP_CHECKPOINT..split]
        .iter()
        .for_each(|bar| {
            sums.add(bar, source);
        });
    bars[split..range.start].iter().for_each(|bar| {
        sums.add_bar(bar, source, executions);
    });
    bars[range].iter().map(move |bar| {
        sums.add_bar(bar, source, executions)
            .then(|| sums.value(mode))
            .flatten()
    })
}

/// Draws a line of an anchored VWAP at `points` as `plot` in `stroke`, the line, step line and
/// area line in `style`, the plots that start from price zero from `zero`, at bars `pitch` apart.
/// An area is its line over a fill of its colour at 20% of its opacity, as a study's area is.
fn paint_vwap_line(
    painter: &egui::Painter,
    mut points: Vec<Pos2>,
    plot: VwapPlotType,
    style: LineStyle,
    stroke: Stroke,
    zero: f32,
    pitch: f32,
) {
    if plot.has_breaks() {
        let joined = match plot {
            VwapPlotType::LineWithBreaks => VwapPlotType::Line,
            VwapPlotType::StepLineWithBreaks => VwapPlotType::StepLine,
            VwapPlotType::AreaWithBreaks => VwapPlotType::Area,
            _ => plot,
        };
        for run in points
            .split(|point| !point.is_finite())
            .filter(|run| !run.is_empty())
        {
            paint_vwap_line(painter, run.to_vec(), joined, style, stroke, zero, pitch);
        }
        return;
    }
    points.retain(|point| point.is_finite());
    if points.is_empty() {
        return;
    }
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
        VwapPlotType::Line | VwapPlotType::LineWithBreaks => path(points),
        VwapPlotType::StepLine
        | VwapPlotType::StepLineWithBreaks
        | VwapPlotType::StepLineWithDiamonds => {
            let mut steps = Vec::with_capacity(points.len() * 2);
            for (i, &point) in points.iter().enumerate() {
                if i > 0 {
                    steps.push(Pos2::new(point.x, points[i - 1].y));
                }
                steps.push(point);
            }
            path(steps);
            if plot == VwapPlotType::StepLineWithDiamonds {
                for (i, &point) in points.iter().enumerate() {
                    if i == 0 || point.y != points[i - 1].y {
                        let half = stroke.width + 2.0;
                        painter.add(Shape::convex_polygon(
                            vec![
                                point - egui::vec2(0.0, half),
                                point + egui::vec2(half, 0.0),
                                point + egui::vec2(0.0, half),
                                point - egui::vec2(half, 0.0),
                            ],
                            color,
                            Stroke::NONE,
                        ));
                    }
                }
            }
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
        VwapPlotType::Area | VwapPlotType::AreaWithBreaks => {
            let base: Vec<_> = points.iter().map(|p| Pos2::new(p.x, zero)).collect();
            let fill = color.gamma_multiply(0.2);
            super::super::fibonacci::paint_band(painter, &points, &base, fill);
            path(points);
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

/// Step lines through the centres of the bars from `first` at the developing `levels` of
/// `profile` that `config` shows: the point of control at its row's middle, as the profile draws
/// it, and the value area's high and low edges. A bar before any volume, or a level off the
/// scale, breaks a line.
fn paint_developing(
    painter: &egui::Painter,
    view: &ChartView,
    first: usize,
    profile: &VolumeProfile,
    levels: &[Option<(usize, (usize, usize))>],
    config: VolumeProfileConfig,
) {
    use crate::data::ProfileLevelKind as Kind;
    let y_at_row = |row: usize| (view.price_to_y)(profile.low + row as f64 * profile.row_height);
    let [high_color, low_color] = config.developing_value_area_colors;
    for (shown, color, kind) in [
        (
            config.show_developing_poc,
            config.developing_poc_color,
            Kind::PointOfControl,
        ),
        (
            config.show_developing_value_area,
            high_color,
            Kind::ValueAreaHigh,
        ),
        (
            config.show_developing_value_area,
            low_color,
            Kind::ValueAreaLow,
        ),
    ] {
        if !shown {
            continue;
        }
        let y = |(poc, (low, high)): (usize, (usize, usize))| match kind {
            Kind::PointOfControl => (y_at_row(poc) + y_at_row(poc + 1)) / 2.0,
            Kind::ValueAreaHigh => y_at_row(high + 1),
            Kind::ValueAreaLow => y_at_row(low),
        };
        let stroke = Stroke::new(crate::styles::stroke::HAIRLINE, rgba(color));
        let mut run: Vec<Pos2> = Vec::new();
        // The break after the last bar paints the last run.
        for (i, level) in levels.iter().copied().chain([None]).enumerate() {
            let point = level
                .map(|level| Pos2::new((view.bar_to_x)((first + i) as f32), y(level)))
                .filter(|point| point.is_finite());
            match point {
                Some(point) => {
                    // Level until the bar, then a step to its new level.
                    if let Some(last) = run.last().copied()
                        && last.y != point.y
                    {
                        run.push(Pos2::new(point.x, last.y));
                    }
                    run.push(point);
                }
                None if run.len() > 1 => {
                    painter.add(Shape::line(std::mem::take(&mut run), stroke));
                }
                None => run.clear(),
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

/// Lowest low and highest high of the valid bars of `bars`, the span that starts at bar `first`,
/// kept as the span's profile is, so a tick or a new bar costs one bar.
fn cached_price_span(view: &ChartView, first: usize, bars: &[Bar]) -> Option<(f64, f64)> {
    use crate::data::price_span;
    PRICE_SPANS.with_borrow_mut(|cache| {
        *view.cached(cache, (), first, bars, price_span, |span, bar| {
            Some(match (*span, price_span(std::slice::from_ref(bar))) {
                (Some((low, high)), Some((lo, hi))) => Some((low.min(lo), high.max(hi))),
                (span, bar) => span.or(bar),
            })
        })
    })
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

/// Suggested interval for a span from its first open through its last close, by length and dates.
fn finer_interval(
    span: &[Bar],
    timeframe: Timeframe,
    resolutions: &[ProfileResolution],
) -> Option<Timeframe> {
    let (first, last) = (span.first()?, span.last()?);
    let end = close(last.time, timeframe);
    ProfileResolution::suggested(resolutions, timeframe, end - first.time, (first.time, end))
}

/// A finer history, the index of its first bar opening in a span, the bars opening in the span,
/// and the span's end.
type FinerInputs<'a> = (&'a [Bar], usize, &'a [Bar], DateTime<Utc>);

/// The suggested finer history for `bars`, the span that starts at bar `first`, the index of its
/// first bar opening in the span, the bars opening in the span, and the span's end: the next chart
/// bar's open or the last bar's close. `None` without finer bars inside the span.
fn finer_inputs<'a>(view: &ChartView<'a>, first: usize, bars: &[Bar]) -> Option<FinerInputs<'a>> {
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
    Some((finer, at, inside, end))
}

/// Replaces chart bars by the suggested finer bars inside them, when any are held in the span.
fn finer_profile(
    view: &ChartView,
    rows: usize,
    row_size: Option<f64>,
    first: usize,
    bars: &[Bar],
) -> Option<Rc<Option<VolumeProfile>>> {
    let (finer, at, inside, end) = finer_inputs(view, first, bars)?;
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

    /// Joined plots span a bar without volume; plots with breaks restart each line after it.
    #[test]
    fn anchored_vwap_plot_types_keep_or_break_missing_values() {
        let bars: Vec<_> = [1.0, 2.0, f64::NAN, 1.0, 1.0]
            .into_iter()
            .enumerate()
            .map(|(i, volume)| {
                let price = 10.0 + i as f64;
                Bar::new(
                    DateTime::from_timestamp(i as i64 * 60, 0).unwrap(),
                    price,
                    price,
                    price,
                    price,
                    volume,
                )
            })
            .collect();
        let mut failures = Vec::new();
        for (name, label, paths, diamonds) in [
            ("Line", "Line", 1, 0),
            ("StepLine", "Step line", 1, 0),
            ("Area", "Area", 1, 0),
            ("LineWithBreaks", "Line with breaks", 2, 0),
            ("StepLineWithBreaks", "Step line with breaks", 2, 0),
            ("AreaWithBreaks", "Area with breaks", 2, 0),
            ("StepLineWithDiamonds", "Step line with diamonds", 1, 4),
        ] {
            let Ok(plot) = serde_json::from_value::<VwapPlotType>(name.into()) else {
                failures.push(format!("{name}: missing plot type"));
                continue;
            };
            assert!(VwapPlotType::ALL.contains(&plot));
            assert_eq!(plot.name(), label);
            assert_eq!(serde_json::to_value(plot).unwrap(), serde_json::json!(name));
            let mut config = VwapConfig {
                plot_type: plot,
                ..Default::default()
            };
            config.bands[0].upper.plot_type = plot;
            config.bands[0].lower.plot_type = plot;
            let shapes = render_vwap(&bars, 0.0, 1.0, LineStyle::Solid, Some(config));
            let lines: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Path(line) if line.stroke.width > 0.0 => Some(line),
                    _ => None,
                })
                .collect();
            if lines.len() != paths * 3 {
                failures.push(format!(
                    "{name}: {} paths, expected {}",
                    lines.len(),
                    paths * 3
                ));
            }
            for line in lines {
                assert!(line.points.iter().all(|p| p.is_finite()), "{name}");
                if paths == 2 {
                    assert!(
                        line.points.iter().all(|p| p.x <= 110.0)
                            || line.points.iter().all(|p| p.x >= 130.0),
                        "{name}: bridged gap"
                    );
                }
            }
            let fills: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Mesh(mesh) => Some(mesh),
                    _ => None,
                })
                .collect();
            assert_eq!(
                fills.len(),
                paths * if name.starts_with("Area") { 4 } else { 1 },
                "{name}: fills"
            );
            for fill in fills {
                assert!(
                    fill.vertices.iter().all(|v| v.pos.is_finite()),
                    "{name}: fill"
                );
                if paths == 2 {
                    assert!(
                        fill.vertices.iter().all(|v| v.pos.x <= 110.0)
                            || fill.vertices.iter().all(|v| v.pos.x >= 130.0),
                        "{name}: fill bridged gap"
                    );
                }
            }
            assert_eq!(shapes.iter().filter(|shape| matches!(shape,
                Shape::Path(line) if line.closed && line.points.len() == 4 && line.stroke.width == 0.0
            )).count(), diamonds * 3, "{name}: diamonds");
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
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
        // Area: the VWAP's line over a fill of its colour at 20% of its opacity down to zero.
        let shapes = edited(|c| c.plot_type = crate::drawings::VwapPlotType::Area);
        assert_eq!(strokes(&shapes), [green, green, blue]);
        let Shape::Mesh(area) = &shapes[3] else {
            panic!("area fill")
        };
        assert!(
            area.vertices
                .iter()
                .all(|v| v.color == Color32::from_rgba_premultiplied(8, 20, 51, 51))
        );
        assert_eq!(
            area.vertices
                .iter()
                .step_by(2)
                .map(|v| v.pos)
                .collect::<Vec<_>>(),
            [(110.0, 100.0), (120.0, 90.0), (130.0, 90.0)].map(|(x, y)| Pos2::new(x, y))
        );
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
        let tracked = drawing.anchored_vwap_tracked(&bars, 0, None);
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
            thousands_sep: None,
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
            None,
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
                    let got = anchored_vwap(&long, &checkpoints, &config, None, window.clone());
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
        // Non-finite inputs have no value and do not poison later bars.
        let flat = |minute: i64, price: f64, volume: f64| {
            let time = chrono::DateTime::from_timestamp(minute * 60, 0).unwrap();
            Bar::new(time, price, price, price, price, volume)
        };
        for gap in [
            flat(1, f64::NAN, 1.0),
            flat(1, 11.0, f64::NAN),
            flat(1, f64::INFINITY, 1.0),
        ] {
            let bars = [flat(0, 10.0, 1.0), gap, flat(2, 12.0, 1.0)];
            let checkpoints = vwap_checkpoints(&[], typical.source);
            let got: Vec<_> = anchored_vwap(&bars, &checkpoints, &typical, None, 0..3).collect();
            assert_eq!(
                got,
                [Some((10.0, 0.0)), None, Some((11.0, 1.0))],
                "{:?}",
                bars[1]
            );
            assert_eq!(anchored_vwap_through(&bars[..2], &typical, None), None);
            assert_eq!(
                anchored_vwap_through(&bars, &typical, None),
                Some((11.0, 1.0))
            );
        }
    }

    /// Complete bars count executions at their prices, gaps count only that bar's source,
    /// and tracking, alerts and the estimated count agree, with the setting off by default.
    #[test]
    fn anchored_vwap_counts_held_executions() {
        use crate::data::Footprint;
        use crate::drawings::ChartPoint;
        use crate::model::{
            BarClock,
            OrderSide::{Buy, Sell},
            Tick,
        };
        use crate::scales::PriceScaleMode;
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let bars = [
            Bar::new(start, 100.0, 104.0, 100.0, 104.0, 1000.0),
            Bar::new(
                start + chrono::TimeDelta::minutes(1),
                104.0,
                106.0,
                103.0,
                105.0,
                500.0,
            ),
            Bar::new(
                start + chrono::TimeDelta::minutes(2),
                105.0,
                105.0,
                101.0,
                102.0,
                400.0,
            ),
        ];
        let trades = [
            (10, 100.0, 900.0, Sell),
            (20, 104.0, 100.0, Buy),
            (70, 104.0, 200.0, Buy),
            (80, 105.0, 300.0, Buy),
            (130, 102.0, 400.0, Sell),
        ]
        .map(|(second, price, volume, side)| {
            Tick::new(
                start + chrono::TimeDelta::seconds(second),
                price,
                volume,
                Some(side),
            )
        });
        let footprint = |trades: &[Tick], ticks| {
            let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.01, ticks).unwrap();
            for trade in trades {
                flow.push(trade).unwrap();
            }
            flow
        };
        let full = footprint(&trades, 1);
        let mut gap = full.clone();
        gap.add_missing(bars[1].time, bars[2].time);
        let late = footprint(&trades[2..], 1);
        let grouped = footprint(&trades, 5);
        let mut invalid = bars[0].clone();
        invalid.close = f64::NAN;
        let mut absent = bars[0].clone();
        absent.time = start + chrono::TimeDelta::minutes(3);
        type Row<'a> = (
            &'a str,
            &'a [Bar],
            Option<FootprintView<'a>>,
            bool,
            f64,
            f64,
            Option<(usize, usize)>,
        );
        let rows: [Row<'_>; 8] = [
            (
                "issue example",
                &bars[..1],
                Some((&full).into()),
                true,
                502.0 / 5.0,
                1.2,
                Some((0, 1)),
            ),
            (
                "middle gap",
                &bars,
                Some((&gap).into()),
                true,
                5806.0 / 57.0,
                (12856.0_f64 / 3249.0).sqrt(),
                Some((1, 3)),
            ),
            (
                "before coverage",
                &bars,
                Some((&late).into()),
                true,
                5873.0 / 57.0,
                (1088.0_f64 / 1083.0).sqrt(),
                Some((1, 3)),
            ),
            (
                "off",
                &bars[..1],
                Some((&full).into()),
                false,
                308.0 / 3.0,
                0.0,
                None,
            ),
            (
                "no executions",
                &bars[..1],
                None,
                true,
                308.0 / 3.0,
                0.0,
                Some((1, 1)),
            ),
            (
                "grouped row midpoint",
                &bars[..1],
                Some((&grouped).into()),
                true,
                100.42,
                1.2,
                Some((0, 1)),
            ),
            (
                "non-finite bar with executions",
                std::slice::from_ref(&invalid),
                Some((&full).into()),
                true,
                100.4,
                1.2,
                Some((0, 1)),
            ),
            (
                "empty complete slot",
                std::slice::from_ref(&absent),
                Some((&full).into()),
                true,
                308.0 / 3.0,
                0.0,
                Some((1, 1)),
            ),
        ];
        for (name, bars, flow, executions, mean, sigma, estimated) in rows {
            let mut drawing = Drawing::new(1, DrawingToolType::AnchoredVWAP);
            drawing.chart_points = vec![ChartPoint::new(0.0, 100.0)];
            let mut config = VwapConfig {
                executions,
                track_price: true,
                ..VwapConfig::default()
            };
            config.bands[0].upper.track_price = true;
            config.bands[0].lower.track_price = true;
            drawing.vwap = Some(config);
            let expected = [mean, mean + sigma, mean - sigma];
            let tracked = drawing.anchored_vwap_tracked(bars, 0, flow);
            let alerts = drawing.alert_levels_with_executions(
                (bars.len() - 1) as f64,
                PriceScaleMode::Normal,
                bars,
                flow,
            );
            assert_eq!((tracked.len(), alerts.len()), (3, 3), "{name}");
            for (i, ((tracked, _), (level, _, alert))) in tracked.iter().zip(&alerts).enumerate() {
                assert_eq!(*level, i, "{name}");
                assert!(
                    (tracked - expected[i]).abs() < 1e-6,
                    "{name}: {tracked}, want {}",
                    expected[i]
                );
                assert!(
                    (alert - expected[i]).abs() < 1e-6,
                    "{name}: {alert}, want {}",
                    expected[i]
                );
            }
            assert_eq!(
                drawing.anchored_vwap_estimated(bars, flow),
                estimated,
                "{name}"
            );
        }
    }

    /// The point of control, the value area high and low lines, the row values and the box of
    /// both profile drawings paint in the colours set on them, or as before while none is set in
    /// either theme, and not at all when switched off. Anchored values measure their room from
    /// the rows at the pane's edge, and values follow the instrument's volume decimals.
    #[test]
    fn volume_profile_drawing_styles_paint_in_their_colours() {
        use crate::data::VolumeProfileConfig;
        use crate::model::Instrument;
        use crate::theme::ThemeExt;
        let bars = sample_bars();
        let price_to_y = |price: f64| (16.0 - price) as f32 * 50.0;
        let clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 600.0));
        let rgba = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
        let one_decimal = Instrument::new("TEST").with_volume_decimals(1);
        let mut on = VolumeProfileConfig::new(4, 0.7).unwrap();
        on.show_value_area_high = true;
        on.show_value_area_low = true;
        on.show_values = true;
        let mut set = on;
        set.poc_color = Some([200, 0, 0, 255]);
        set.value_area_line_colors = [[0, 200, 0, 255], [0, 0, 200, 255]];
        set.values_color = Some([250, 200, 0, 255]);
        set.box_color = Some([10, 20, 30, 60]);
        let mut inside = set;
        inside.value_area_values_color = Some([0, 200, 200, 255]);
        let mut off = set;
        off.show_poc = false;
        off.show_value_area_high = false;
        off.show_value_area_low = false;
        off.show_values = false;
        off.show_box = false;
        // Four 1.5-price rows have totals 83 1/3, 350, 619 1/6 and 397.5.
        let two = ["83.33", "350.00", "619.17", "397.50"];
        let one = ["83.3", "350.0", "619.2", "397.5"];
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for tool in [
                DrawingToolType::FixedRangeVolumeProfile,
                DrawingToolType::AnchoredVolumeProfile,
            ] {
                let anchored = tool == DrawingToolType::AnchoredVolumeProfile;
                for (case, config, width, instrument, values) in [
                    (
                        "defaults",
                        VolumeProfileConfig::new(4, 0.7).unwrap(),
                        400.0,
                        None,
                        None,
                    ),
                    ("on", on, 400.0, None, Some(two)),
                    ("set", set, 400.0, None, Some(two)),
                    ("set, value area values", inside, 400.0, None, Some(two)),
                    ("off", off, 400.0, None, None),
                    (
                        "set, one decimal",
                        set,
                        400.0,
                        Some(&one_decimal),
                        Some(one),
                    ),
                    // Anchored rows sit at the pane's edge, where the values fit.
                    ("set, narrow", set, 24.0, None, anchored.then_some(two)),
                ] {
                    let mut drawing = Drawing::new(1, tool);
                    drawing.color = [40, 90, 200, 255];
                    drawing.volume_profile = Some(config);
                    let view = ChartView {
                        price_formatter: None,
                        thousands_sep: None,
                        time_formatter: None,
                        bar_seconds: 0,
                        instrument,
                        bars: &bars,
                        revision: 0,
                        bar_to_x: &|x| x,
                        bar_spacing: 1.0,
                        price_to_y: &price_to_y,
                        y_to_price: &|y| 16.0 - f64::from(y) / 50.0,
                        time_axis: true,
                        executions: None,
                        timeframe: None,
                        finer: &[],
                    };
                    let ctx = egui::Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let mut text = Color32::PLACEHOLDER;
                    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                        text = ui.ctx().active_theme().ui.text;
                        let painter =
                            egui::Painter::new(ui.ctx().clone(), egui::LayerId::background(), clip);
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
                    let name = format!("{case}, {tool:?}, dark {}", theme.dark);
                    // Placeholder bytes take the drawing's colour at alpha 200.
                    let line = |set: [u8; 4]| {
                        if set == Color32::PLACEHOLDER.to_srgba_unmultiplied() {
                            rgba([40, 90, 200, 200])
                        } else {
                            rgba(set)
                        }
                    };
                    let box_rect =
                        Rect::from_min_max(Pos2::new(100.0, 50.0), Pos2::new(100.0 + width, 350.0));
                    let mut boxes = Vec::new();
                    let mut pocs = Vec::new();
                    let mut edges = Vec::new();
                    let mut labels = Vec::new();
                    let mut figures = Vec::new();
                    for shape in &shapes {
                        match &shape.shape {
                            egui::Shape::Rect(r) if r.rect == box_rect => boxes.push(r.fill),
                            egui::Shape::Path(path) => {
                                let egui::epaint::ColorMode::Solid(color) = path.stroke.color
                                else {
                                    panic!("{name}: POC is one colour");
                                };
                                pocs.push((path.points[0], path.points[1].x, color));
                            }
                            egui::Shape::LineSegment { points, stroke } => {
                                edges.push((points[0].y, stroke.color));
                            }
                            egui::Shape::Text(t) => {
                                let label = t.galley.job.text.clone();
                                let color = t.galley.job.sections[0].format.color;
                                if ["VAH", "VAL"].contains(&label.as_str()) {
                                    labels.push((label, color));
                                } else if label != "Estimated from bars" {
                                    figures.push((label, color, t.visual_bounding_rect()));
                                }
                            }
                            _ => {}
                        }
                    }
                    let shown = config
                        .show_box
                        .then(|| rgba(config.box_color.unwrap_or([40, 90, 200, 15])));
                    assert_eq!(boxes.first().copied(), shown, "{name}: box");
                    let reach = if anchored {
                        [0.0, 800.0]
                    } else {
                        [100.0, 100.0 + width]
                    };
                    let poc = config.poc_color.map_or(text, rgba);
                    assert_eq!(
                        pocs,
                        if config.show_poc {
                            vec![(Pos2::new(reach[0], 162.5), reach[1], poc)]
                        } else {
                            Vec::new()
                        },
                        "{name}: POC"
                    );
                    let [high, low] = config.value_area_line_colors;
                    let want = if config.show_value_area_high {
                        vec![(125.0, line(high)), (350.0, line(low))]
                    } else {
                        Vec::new()
                    };
                    assert_eq!(edges, want, "{name}: value area lines");
                    if width >= 50.0 {
                        assert_eq!(
                            labels,
                            want.iter()
                                .zip(["VAH", "VAL"])
                                .map(|(&(_, color), label)| (label.to_owned(), color))
                                .collect::<Vec<_>>(),
                            "{name}: value area labels"
                        );
                    }
                    let bounds = if anchored {
                        clip.x_range()
                    } else {
                        (102.0..=100.0 + width).into()
                    };
                    assert_eq!(
                        figures
                            .iter()
                            .map(|(label, ..)| label.as_str())
                            .collect::<Vec<_>>(),
                        values.map_or(Vec::new(), Vec::from),
                        "{name}: values"
                    );
                    for (label, color, rect) in &figures {
                        // The top row, 13.5 to 15 at y 50 to 125, lies above the VAH at y 125.
                        let inside = rect.center().y > 125.0;
                        let want = if inside {
                            config.value_area_values_color.or(config.values_color)
                        } else {
                            config.values_color
                        }
                        .map_or(text, rgba);
                        assert_eq!(*color, want, "{name}: {label}");
                        assert!(
                            bounds.contains(rect.min.x) && bounds.contains(rect.max.x),
                            "{name}: {label} at {rect:?}"
                        );
                    }
                }
            }
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
                        thousands_sep: None,
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
            thousands_sep: None,
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
            thousands_sep: None,
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
                        position + egui::vec2(3.0, 10.0),
                        |x| (x - 100.0) / 10.0,
                        view.y_to_price,
                        |p, from, to| p - f64::from(to - from) / 10.0,
                    );
                    assert_eq!(
                        manager.drawings[0].chart_points, before,
                        "{name}: drag within its bar"
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
                thousands_sep: None,
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
            thousands_sep: None,
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

    /// Executed rows follow the drawing's row layout as the fixed range volume profile study
    /// groups the same executions: Rows 2 gives two rows of six ticks, since rows of three
    /// counted from multiples of three would need three, and Price height 5 gives rows of five
    /// ticks. The point of control and value area come from those rows, and every layout keeps
    /// the 21 executed, split by side. A host footprint of two ticks to a row keeps whole rows of
    /// it.
    #[test]
    fn executed_rows_follow_the_row_layout() {
        use crate::data::{Footprint, ProfileSource, VolumeProfileConfig};
        use crate::drawings::ChartPoint;
        use crate::model::{BarClock, OrderSide, Tick};
        let bars = sample_bars();
        // Bar 1 trades 10 (1 sold), 11 (2 bought) and 12 (3 bought); bar 2 trades 13 (4 sold),
        // 14 (5 unknown) and 15 (6 bought).
        let trades: Vec<_> = [
            (1, 10.0, 1.0, Some(OrderSide::Sell)),
            (1, 11.0, 2.0, Some(OrderSide::Buy)),
            (1, 12.0, 3.0, Some(OrderSide::Buy)),
            (2, 13.0, 4.0, Some(OrderSide::Sell)),
            (2, 14.0, 5.0, None),
            (2, 15.0, 6.0, Some(OrderSide::Buy)),
        ]
        .into_iter()
        .map(|(bar, price, size, side)| Tick::new(bars[bar].time, price, size, side))
        .collect();
        let height = |price| {
            VolumeProfileConfig::default()
                .with_row_size(Some(price))
                .unwrap()
        };
        // Ticks per footprint row, the layout, then the rows: bottom edge, height, volume, buys,
        // sells, and the row of the point of control, which alone holds the 70% value area (14.7
        // of 21).
        for (label, ticks, config, low, row_height, volume, up, down, poc) in [
            (
                "rows 2",
                1,
                VolumeProfileConfig::new(2, 0.7).unwrap(),
                5.5,
                6.0,
                [3.0, 18.0],
                [2.0, 9.0],
                [1.0, 4.0],
                1,
            ),
            (
                "price height 5",
                1,
                height(5.0),
                9.5,
                5.0,
                [15.0, 6.0],
                [5.0, 6.0],
                [5.0, 0.0],
                0,
            ),
            (
                "price height 4, two ticks a row",
                2,
                height(4.0),
                7.5,
                4.0,
                [3.0, 18.0],
                [2.0, 9.0],
                [1.0, 4.0],
                1,
            ),
        ] {
            let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, ticks).unwrap();
            for trade in &trades {
                flow.push(trade).unwrap();
            }
            let view = ChartView {
                price_formatter: None,
                thousands_sep: None,
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
                executions: Some((&flow).into()),
                timeframe: None,
                finer: &[],
            };
            let mut drawing = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
            drawing.chart_points = vec![ChartPoint::new(1.0, 10.0), ChartPoint::new(2.0, 10.0)];
            drawing.volume_profile = Some(config);
            let profile = drawing.volume_profile_of(&view).unwrap();
            assert_eq!(
                profile.source(),
                ProfileSource::Executions { complete: true },
                "{label}"
            );
            assert_eq!(
                (profile.low(), profile.row_height()),
                (low, row_height),
                "{label}"
            );
            assert_eq!(profile.volume(), volume, "{label}");
            assert_eq!(profile.up(), up, "{label}");
            assert_eq!(profile.down(), down, "{label}");
            assert_eq!(
                profile.poc_and_value_area(0.7),
                Some((poc, (poc, poc))),
                "{label}"
            );
        }
    }

    /// Both profile drawings draw rows set in ticks that many ticks of the instrument high, on
    /// multiples of that height, from executions, finer bars and chart bars, and the row count on an
    /// instrument without a tick size. A row count draws the whole ticks per row whose row total
    /// lies closest to it where the instrument has a tick size, as the volume profile studies do,
    /// and equal rows without one. A price height saved before rows could be set in ticks draws the
    /// rows it drew then.
    #[test]
    fn profile_rows_follow_ticks_of_the_instrument() {
        use crate::data::{Footprint, ProfileSource};
        use crate::drawings::ChartPoint;
        use crate::model::{BarClock, Instrument, OrderSide, Tick};
        use crate::studies::StudyData;
        // A rising bar over 10.25..11.25 and a falling one over 11.5..12.5.
        let bars: Vec<_> = [
            (10.25, 11.25, 10.25, 11.0, 40.0),
            (12.0, 12.5, 11.5, 11.75, 20.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (open, high, low, close, volume))| {
            let time = DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
            Bar::new(time, open, high, low, close, volume)
        })
        .collect();
        // Bar 0 trades 10.25 (1 sold), 10.75 (2 bought) and 11 (3 bought); bar 1 trades 11.5
        // (4 sold), 12 (5 unknown) and 12.25 (6 bought), in a footprint of 0.25 ticks.
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        for (bar, price, size, side) in [
            (0, 10.25, 1.0, Some(OrderSide::Sell)),
            (0, 10.75, 2.0, Some(OrderSide::Buy)),
            (0, 11.0, 3.0, Some(OrderSide::Buy)),
            (1, 11.5, 4.0, Some(OrderSide::Sell)),
            (1, 12.0, 5.0, None),
            (1, 12.25, 6.0, Some(OrderSide::Buy)),
        ] {
            flow.push(&Tick::new(bars[bar].time, price, size, side))
                .unwrap();
        }
        // Finer bars have the same ranges and twice the volume, so using chart bars would fail.
        let finer_bars: Vec<_> = bars
            .iter()
            .cloned()
            .map(|mut bar| {
                bar.volume *= 2.0;
                bar
            })
            .collect();
        let history = [StudyData {
            request: StudyDataRequest {
                symbol: "TEST".into(),
                timeframe: Some(Timeframe::Sec1),
                metric: None,
            },
            bars: &finer_bars,
            clock: BarClock::new(Timeframe::Sec1),
            instrument: None,
            timeframe: Timeframe::Sec1,
            chart: None,
        }];
        let four_ticks = VolumeProfileConfig::default()
            .with_row_ticks(Some(4))
            .unwrap();
        let saved = |json| serde_json::from_str::<VolumeProfileConfig>(json).unwrap();
        // The layout and the instrument's tick size, then the rows from chart bars and from
        // executions: bottom edge, height, volume, buys and sells. Executed rows start half a
        // tick below their first tick.
        for (label, config, tick, from_bars, executed) in [
            (
                "4 ticks of 0.25",
                four_ticks,
                Some(0.25),
                (
                    10.0,
                    1.0,
                    vec![30.0, 20.0, 10.0],
                    vec![30.0, 10.0, 0.0],
                    vec![0.0, 10.0, 10.0],
                ),
                (
                    9.875,
                    1.0,
                    vec![3.0, 7.0, 11.0],
                    vec![2.0, 3.0, 6.0],
                    vec![1.0, 4.0, 0.0],
                ),
            ),
            (
                "4 ticks of 0.5",
                four_ticks,
                Some(0.5),
                (
                    10.0,
                    2.0,
                    vec![50.0, 10.0],
                    vec![40.0, 0.0],
                    vec![10.0, 10.0],
                ),
                (9.875, 2.0, vec![10.0, 11.0], vec![5.0, 6.0], vec![5.0, 0.0]),
            ),
            (
                "4 ticks without a tick size",
                VolumeProfileConfig::new(3, 0.7)
                    .and_then(|c| c.with_row_ticks(Some(4)))
                    .unwrap(),
                None,
                (
                    10.25,
                    0.75,
                    vec![30.0, 15.0, 15.0],
                    vec![30.0, 10.0, 0.0],
                    vec![0.0, 5.0, 15.0],
                ),
                (9.875, 1.25, vec![6.0, 15.0], vec![5.0, 6.0], vec![1.0, 4.0]),
            ),
            // 9 ticks over 3 rows are 3 ticks a row, which takes 4 rows from 9.75; the
            // executed rows hold the ticks 39 to 41, 42 to 44, 45 to 47 and 48 to 50.
            (
                "saved row count",
                saved(r#"{"rows":3,"value_area":0.7}"#),
                Some(0.25),
                (
                    9.75,
                    0.75,
                    vec![10.0, 30.0, 10.0, 10.0],
                    vec![10.0, 30.0, 0.0, 0.0],
                    vec![0.0, 0.0, 10.0, 10.0],
                ),
                (
                    9.625,
                    0.75,
                    vec![1.0, 5.0, 4.0, 11.0],
                    vec![0.0, 5.0, 0.0, 6.0],
                    vec![1.0, 0.0, 4.0, 0.0],
                ),
            ),
            (
                "saved row count without a tick size",
                saved(r#"{"rows":3,"value_area":0.7}"#),
                None,
                (
                    10.25,
                    0.75,
                    vec![30.0, 15.0, 15.0],
                    vec![30.0, 10.0, 0.0],
                    vec![0.0, 5.0, 15.0],
                ),
                (9.875, 1.25, vec![6.0, 15.0], vec![5.0, 6.0], vec![1.0, 4.0]),
            ),
            (
                "saved price height",
                saved(r#"{"row_size":0.5,"rows":24,"value_area":0.7}"#),
                Some(0.25),
                (
                    10.0,
                    0.5,
                    vec![10.0, 20.0, 10.0, 10.0, 10.0, 0.0],
                    vec![10.0, 20.0, 10.0, 0.0, 0.0, 0.0],
                    vec![0.0, 0.0, 0.0, 10.0, 10.0, 0.0],
                ),
                (
                    9.875,
                    0.5,
                    vec![1.0, 2.0, 3.0, 4.0, 11.0],
                    vec![0.0, 2.0, 3.0, 0.0, 6.0],
                    vec![1.0, 0.0, 0.0, 4.0, 0.0],
                ),
            ),
        ] {
            let mut instrument = Instrument::new("TEST")
                .with_profile_resolutions([ProfileResolution::new(Timeframe::Sec1, None)]);
            if let Some(tick) = tick {
                instrument = instrument.with_tick_size(tick);
            }
            for tool in [
                DrawingToolType::FixedRangeVolumeProfile,
                DrawingToolType::AnchoredVolumeProfile,
            ] {
                let mut drawing = Drawing::new(1, tool);
                drawing.chart_points = vec![ChartPoint::new(0.0, 10.0), ChartPoint::new(1.0, 10.0)];
                drawing.volume_profile = Some(config);
                for (executions, finer, source, (low, height, volume, up, down)) in [
                    (None, false, ProfileSource::Estimated, &from_bars),
                    (None, true, ProfileSource::Estimated, &from_bars),
                    (
                        Some((&flow).into()),
                        false,
                        ProfileSource::Executions { complete: true },
                        &executed,
                    ),
                ] {
                    let view = ChartView {
                        price_formatter: None,
                        thousands_sep: None,
                        time_formatter: None,
                        bar_seconds: 0,
                        instrument: Some(&instrument),
                        bars: &bars,
                        revision: 0,
                        bar_to_x: &|x| x,
                        bar_spacing: 1.0,
                        price_to_y: &|price| price as f32,
                        y_to_price: &f64::from,
                        time_axis: true,
                        executions,
                        timeframe: Some(Timeframe::Min1),
                        finer: if finer { &history } else { &[] },
                    };
                    let profile = drawing.volume_profile_of(&view).unwrap();
                    let at = format!("{label}, {tool:?}, {source:?}, finer={finer}");
                    assert_eq!(profile.source(), source, "{at}");
                    assert_eq!(
                        (profile.low(), profile.row_height()),
                        (*low, *height),
                        "{at}"
                    );
                    let expected = |values: &[f64]| {
                        values
                            .iter()
                            .map(|value| value * if finer { 2.0 } else { 1.0 })
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(profile.volume(), expected(volume), "{at}");
                    assert_eq!(profile.up(), expected(up), "{at}");
                    assert_eq!(profile.down(), expected(down), "{at}");
                }
            }
        }
        // Cent ticks from 10.00 to 11.00: a rising bar of 60 over 10.00..10.60 and a falling one
        // of 40 over 10.60..11.00, 100 a point each, and executions at 10.00 (1 sold), 10.30
        // (2 bought) and 10.99 (3 bought) in a footprint of 0.01 ticks.
        let bars: Vec<_> = [
            (10.0, 10.6, 10.0, 10.6, 60.0),
            (11.0, 11.0, 10.6, 10.6, 40.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (open, high, low, close, volume))| {
            let time = DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
            Bar::new(time, open, high, low, close, volume)
        })
        .collect();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.01, 1).unwrap();
        for (bar, price, size, side) in [
            (0, 10.0, 1.0, OrderSide::Sell),
            (0, 10.3, 2.0, OrderSide::Buy),
            (1, 10.99, 3.0, OrderSide::Buy),
        ] {
            flow.push(&Tick::new(bars[bar].time, price, size, Some(side)))
                .unwrap();
        }
        let finer_bars: Vec<_> = bars
            .iter()
            .cloned()
            .map(|mut bar| {
                bar.volume *= 2.0;
                bar
            })
            .collect();
        let history = [StudyData {
            request: StudyDataRequest {
                symbol: "TEST".into(),
                timeframe: Some(Timeframe::Sec1),
                metric: None,
            },
            bars: &finer_bars,
            clock: BarClock::new(Timeframe::Sec1),
            instrument: None,
            timeframe: Timeframe::Sec1,
            chart: None,
        }];
        let instrument = Instrument::new("TEST")
            .with_tick_size(0.01)
            .with_profile_resolutions([ProfileResolution::new(Timeframe::Sec1, None)]);
        let sparse = |len: usize, rows: &[(usize, f64)]| {
            let mut volume = vec![0.0; len];
            for &(row, value) in rows {
                volume[row] = value;
            }
            volume
        };
        // Rows asked for, then the rows from chart bars and from executions.
        for (rows, from_bars, executed) in [
            // 100 ticks over 30 rows are 3.33 a row: 3 ticks take 34 rows and 4 take 25, so 3,
            // from 9.99. The first bar ends 0.01 into row 20 and the second starts there.
            (
                30,
                (
                    9.99,
                    0.03,
                    [vec![2.0], vec![3.0; 32], vec![2.0]].concat(),
                    [vec![2.0], vec![3.0; 19], vec![1.0], vec![0.0; 13]].concat(),
                    [vec![0.0; 20], vec![2.0], vec![3.0; 12], vec![2.0]].concat(),
                ),
                (
                    9.985,
                    0.03,
                    sparse(34, &[(0, 1.0), (10, 2.0), (33, 3.0)]),
                    sparse(34, &[(10, 2.0), (33, 3.0)]),
                    sparse(34, &[(0, 1.0)]),
                ),
            ),
            // Over 24 rows they are 4.17 a row: 4 ticks take 25 rows and 5 take 20, so 4, from
            // 10.00. The high of 11.00 lies on a row edge, which opens an empty row above it.
            (
                24,
                (
                    10.0,
                    0.04,
                    [vec![4.0; 25], vec![0.0]].concat(),
                    [vec![4.0; 15], vec![0.0; 11]].concat(),
                    [vec![0.0; 15], vec![4.0; 10], vec![0.0]].concat(),
                ),
                (
                    9.995,
                    0.04,
                    sparse(25, &[(0, 1.0), (7, 2.0), (24, 3.0)]),
                    sparse(25, &[(7, 2.0), (24, 3.0)]),
                    sparse(25, &[(0, 1.0)]),
                ),
            ),
        ] {
            for tool in [
                DrawingToolType::FixedRangeVolumeProfile,
                DrawingToolType::AnchoredVolumeProfile,
            ] {
                let mut drawing = Drawing::new(1, tool);
                drawing.chart_points = vec![ChartPoint::new(0.0, 10.0), ChartPoint::new(1.0, 10.0)];
                drawing.volume_profile = VolumeProfileConfig::new(rows, 0.7);
                for (executions, finer, (low, height, volume, up, down)) in [
                    (None, false, &from_bars),
                    (None, true, &from_bars),
                    (Some((&flow).into()), false, &executed),
                ] {
                    let view = ChartView {
                        price_formatter: None,
                        thousands_sep: None,
                        time_formatter: None,
                        bar_seconds: 0,
                        instrument: Some(&instrument),
                        bars: &bars,
                        revision: 0,
                        bar_to_x: &|x| x,
                        bar_spacing: 1.0,
                        price_to_y: &|price| price as f32,
                        y_to_price: &f64::from,
                        time_axis: true,
                        executions,
                        timeframe: Some(Timeframe::Min1),
                        finer: if finer { &history } else { &[] },
                    };
                    let profile = drawing.volume_profile_of(&view).unwrap();
                    let at = format!(
                        "{rows} rows, {tool:?}, executed={}, finer={finer}",
                        executions.is_some()
                    );
                    // Cent prices are not exact in binary, so rows are compared within 1e-9.
                    let close = |got: &[f64], want: &[f64], scale: f64| {
                        got.len() == want.len()
                            && got
                                .iter()
                                .zip(want)
                                .all(|(got, want)| (got - want * scale).abs() < 1e-9)
                    };
                    let scale = if finer { 2.0 } else { 1.0 };
                    assert!(
                        close(
                            &[profile.low(), profile.row_height()],
                            &[*low, *height],
                            1.0
                        ),
                        "{at}: from {}, {} high",
                        profile.low(),
                        profile.row_height()
                    );
                    assert!(
                        close(profile.volume(), volume, scale),
                        "{at}: {:?}",
                        profile.volume()
                    );
                    assert!(close(profile.up(), up, scale), "{at}: {:?}", profile.up());
                    assert!(
                        close(profile.down(), down, scale),
                        "{at}: {:?}",
                        profile.down()
                    );
                }
            }
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
        let days =
            |days| ProfileResolution::new(Timeframe::Min1, Some(chrono::TimeDelta::days(days)));
        // One-minute bars held from the second day on only.
        let later = days(10).with_history(Some(daily[1].time), None);
        // Whole daily bars have volume 100 each. The first day has 60 + 40 finer volume,
        // the second 100, and the new bar of day 3 contributes 50 in place of its daily 100.
        for (name, anchored, last, count, hint, executed, inputs, total) in [
            (
                "fixed",
                false,
                1.0,
                3,
                days(10),
                false,
                fine[..3].to_vec(),
                200.0,
            ),
            (
                "partial",
                false,
                2.0,
                3,
                days(10),
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
                days(10),
                false,
                vec![fine[2].clone(), daily[2].clone()],
                200.0,
            ),
            (
                "anchored after",
                true,
                2.0,
                4,
                days(10),
                false,
                fine[2..].to_vec(),
                150.0,
            ),
            (
                "anchored in history",
                true,
                2.0,
                4,
                later,
                false,
                fine[2..].to_vec(),
                150.0,
            ),
            (
                "span too long",
                false,
                1.0,
                3,
                days(1),
                false,
                daily[..2].to_vec(),
                200.0,
            ),
            (
                "history starts later",
                false,
                1.0,
                3,
                later,
                false,
                daily[..2].to_vec(),
                200.0,
            ),
            ("executions", false, 1.0, 3, days(10), true, Vec::new(), 5.0),
        ] {
            let instrument = Instrument::new("A").with_profile_resolutions([hint]);
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
                chart: None,
            }];
            let view = ChartView {
                instrument: Some(&instrument),
                price_formatter: None,
                thousands_sep: None,
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

    /// A profile estimated from one chart bar counts its volume as up only when the bar closes
    /// above its open, so a bar that closes at its open is down volume. Over two or more bars, and
    /// over the forming bar once the next bar opens, an unchanged close is up, and executions keep
    /// their aggressor side.
    #[test]
    fn single_bar_profile_counts_an_unchanged_close_as_down() {
        use crate::data::Footprint;
        use crate::drawings::ChartPoint;
        use crate::model::{BarClock, OrderSide, Tick};
        // Bars that open and close at 10.50 between 10 and 11, with 100 traded each.
        let bars: Vec<_> = (0..3)
            .map(|i| {
                let time = DateTime::from_timestamp(i * 60, 0).unwrap();
                Bar::new(time, 10.5, 11.0, 10.0, 10.5, 100.0)
            })
            .collect();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        flow.push(&Tick::new(bars[0].time, 10.0, 5.0, Some(OrderSide::Buy)))
            .unwrap();
        let mut failures = Vec::new();
        // In order: "two bars" and "next bar" extend the cached profiles of "one bar" and
        // "forming bar", which start at the same bar.
        for (case, held, anchors, executed, up, down) in [
            ("one bar", 2, &[0.0, 0.0][..], false, 0.0, 100.0),
            ("two bars", 2, &[0.0, 1.0][..], false, 200.0, 0.0),
            ("forming bar", 2, &[1.0][..], false, 0.0, 100.0),
            ("next bar", 3, &[1.0][..], false, 200.0, 0.0),
            ("executions", 2, &[0.0, 0.0][..], true, 5.0, 0.0),
        ] {
            let view = ChartView {
                price_formatter: None,
                thousands_sep: None,
                time_formatter: None,
                bar_seconds: 0,
                instrument: None,
                bars: &bars[..held],
                revision: 0,
                bar_to_x: &|x| x,
                bar_spacing: 1.0,
                price_to_y: &|price| price as f32,
                y_to_price: &f64::from,
                time_axis: true,
                executions: executed.then_some((&flow).into()),
                timeframe: None,
                finer: &[],
            };
            let mut drawing = Drawing::new(
                1,
                if anchors.len() == 1 {
                    DrawingToolType::AnchoredVolumeProfile
                } else {
                    DrawingToolType::FixedRangeVolumeProfile
                },
            );
            drawing.chart_points = anchors
                .iter()
                .map(|&bar| ChartPoint::new(bar, 10.5))
                .collect();
            let profile = drawing.volume_profile_of(&view).unwrap();
            let sides = (
                profile.up().iter().sum::<f64>(),
                profile.down().iter().sum::<f64>(),
            );
            if (sides.0 - up).abs() > 1e-9 || (sides.1 - down).abs() > 1e-9 {
                failures.push(format!(
                    "{case}: up and down {sides:?}, expected ({up}, {down})"
                ));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// The developing point of control and value area step through the drawing's bars at the
    /// levels of the bars up to each one, in the rows the drawing draws, from the executions or
    /// finer bars its profile reads, through the last bar while the range extends right, each
    /// switch drawing its own lines in their own colours. Over three bars the point of control
    /// moves up one row at the third, and the value area reaches the top row at the second.
    #[test]
    fn developing_levels_step_after_each_bar() {
        use crate::data::{Footprint, StudyData, VolumeProfileConfig};
        use crate::drawings::ChartPoint;
        use crate::model::{BarClock, Instrument, OrderSide, Tick};
        #[derive(Clone, Copy, PartialEq)]
        enum Source {
            Bars,
            Executions,
            Finer,
        }
        type Points<'a> = &'a [(f32, f32)];
        type Row<'a> = (
            &'a str,
            &'a [f32],
            bool,
            Option<f64>,
            Source,
            [bool; 2],
            [Points<'a>; 3],
        );
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let at = |seconds| start + chrono::TimeDelta::seconds(seconds);
        // 10 traded in 100..101, then 6 in 102..103 and 12 in 101..102.
        let bars = [
            Bar::new(at(0), 100.25, 100.75, 100.25, 100.75, 10.0),
            Bar::new(at(60), 102.25, 102.75, 102.25, 102.75, 6.0),
            Bar::new(at(120), 101.25, 101.75, 101.25, 101.75, 12.0),
        ];
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (bar, price, size) in [(0, 100.0, 10.0), (1, 102.0, 6.0), (2, 101.0, 12.0)] {
            flow.push(&Tick::new(
                bars[bar].time,
                price,
                size,
                Some(OrderSide::Buy),
            ))
            .unwrap();
        }
        // Finer bars put 30 in the second bar in place of its 6.
        let finer_bars = [Bar::new(at(60), 102.25, 102.75, 102.25, 102.75, 30.0)];
        let finer = [StudyData {
            request: StudyDataRequest {
                symbol: "A".into(),
                timeframe: Some(Timeframe::Sec30),
                metric: None,
            },
            bars: &finer_bars,
            clock: BarClock::new(Timeframe::Sec30),
            instrument: None,
            timeframe: Timeframe::Sec30,
            chart: None,
        }];
        let instrument = Instrument::new("A")
            .with_profile_resolutions([ProfileResolution::new(Timeframe::Sec30, None)]);
        // Rows of 1 from 100: bar centres at x 100, 110, 120 and row edges at y 200 - 20 * row.
        // 70% of the volume after each bar, [10, 0, 0], [10, 0, 6], [10, 12, 6]: POC rows 0, 0,
        // 1 and value areas 0..=0, 0..=2 (only rows above are left), 0..=1 (10 below beats 6).
        let fixed: [Points; 3] = [
            &[
                (100.0, 190.0),
                (110.0, 190.0),
                (120.0, 190.0),
                (120.0, 170.0),
            ],
            &[
                (100.0, 180.0),
                (110.0, 180.0),
                (110.0, 140.0),
                (120.0, 140.0),
                (120.0, 160.0),
            ],
            &[(100.0, 200.0), (110.0, 200.0), (120.0, 200.0)],
        ];
        // The same rows of volume in three equal rows over 100.25..102.75, edges 16 2/3 apart
        // from y 195: each prefix is binned in them, not in a layout of its own.
        let equal: [Points; 3] = [
            &[
                (100.0, 186.667),
                (110.0, 186.667),
                (120.0, 186.667),
                (120.0, 170.0),
            ],
            &[
                (100.0, 178.333),
                (110.0, 178.333),
                (110.0, 145.0),
                (120.0, 145.0),
                (120.0, 161.667),
            ],
            &[(100.0, 195.0), (110.0, 195.0), (120.0, 195.0)],
        ];
        // Executed rows of 1 centred on 100, 101 and 102, so edges at y 210 - 20 * row; the
        // second bar leaves 101 untraded, which the value area still crosses.
        let executed: [Points; 3] = [
            &[
                (100.0, 200.0),
                (110.0, 200.0),
                (120.0, 200.0),
                (120.0, 180.0),
            ],
            &[
                (100.0, 190.0),
                (110.0, 190.0),
                (110.0, 150.0),
                (120.0, 150.0),
                (120.0, 170.0),
            ],
            &[(100.0, 210.0), (110.0, 210.0), (120.0, 210.0)],
        ];
        // [10, 0, 0], [10, 0, 30], [10, 12, 30]: POC rows 0, 2, 2 and value areas 0..=0, 2..=2
        // and 1..=2.
        let finer_levels: [Points; 3] = [
            &[
                (100.0, 190.0),
                (110.0, 190.0),
                (110.0, 150.0),
                (120.0, 150.0),
            ],
            &[
                (100.0, 180.0),
                (110.0, 180.0),
                (110.0, 140.0),
                (120.0, 140.0),
            ],
            &[
                (100.0, 200.0),
                (110.0, 200.0),
                (110.0, 160.0),
                (120.0, 160.0),
                (120.0, 180.0),
            ],
        ];
        let rows: [Row; 8] = [
            (
                "fixed",
                &[0.0, 2.0],
                false,
                Some(1.0),
                Source::Bars,
                [true; 2],
                fixed,
            ),
            (
                "extended",
                &[0.0, 1.0],
                true,
                Some(1.0),
                Source::Bars,
                [true; 2],
                fixed,
            ),
            (
                "anchored",
                &[0.0],
                false,
                Some(1.0),
                Source::Bars,
                [true; 2],
                fixed,
            ),
            (
                "equal rows",
                &[0.0, 2.0],
                false,
                None,
                Source::Bars,
                [true; 2],
                equal,
            ),
            (
                "executions",
                &[0.0, 2.0],
                false,
                Some(1.0),
                Source::Executions,
                [true; 2],
                executed,
            ),
            (
                "finer bars",
                &[0.0, 2.0],
                false,
                Some(1.0),
                Source::Finer,
                [true; 2],
                finer_levels,
            ),
            (
                "POC only",
                &[0.0, 2.0],
                false,
                Some(1.0),
                Source::Bars,
                [true, false],
                [fixed[0], &[], &[]],
            ),
            (
                "VA only",
                &[0.0, 2.0],
                false,
                Some(1.0),
                Source::Bars,
                [false, true],
                [&[], fixed[1], fixed[2]],
            ),
        ];
        let colors = [[200, 0, 0, 255], [0, 0, 200, 255], [0, 150, 0, 255]];
        let mut failures = Vec::new();
        // One anchor is an anchored profile, two a fixed range.
        for (name, anchors, extend_right, row_size, source, [poc, va], want) in rows {
            let view = ChartView {
                price_formatter: None,
                thousands_sep: None,
                time_formatter: None,
                bar_seconds: 60,
                instrument: (source == Source::Finer).then_some(&instrument),
                bars: &bars,
                revision: 0,
                bar_to_x: &|i| 100.0 + 10.0 * i,
                bar_spacing: 10.0,
                price_to_y: &|price| ((110.0 - price) * 20.0) as f32,
                y_to_price: &|y| 110.0 - f64::from(y) / 20.0,
                time_axis: true,
                executions: (source == Source::Executions).then_some((&flow).into()),
                timeframe: Some(Timeframe::Min1),
                finer: if source == Source::Finer { &finer } else { &[] },
            };
            let mut config = VolumeProfileConfig::new(3, 0.7)
                .and_then(|config| config.with_row_size(row_size))
                .unwrap();
            config.extend_right = extend_right;
            config.show_developing_poc = poc;
            config.show_developing_value_area = va;
            config.developing_poc_color = colors[0];
            config.developing_value_area_colors = [colors[1], colors[2]];
            let anchored = anchors.len() == 1;
            let mut drawing = Drawing::new(
                1,
                if anchored {
                    DrawingToolType::AnchoredVolumeProfile
                } else {
                    DrawingToolType::FixedRangeVolumeProfile
                },
            );
            drawing.volume_profile = Some(config);
            drawing.chart_points = anchors
                .iter()
                .map(|&bar| ChartPoint::new(bar, 100.0))
                .collect();
            let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                let clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 600.0));
                let painter =
                    egui::Painter::new(ui.ctx().clone(), egui::LayerId::background(), clip);
                if anchored {
                    drawing.render_anchored_volume_profile(&painter, &view);
                } else {
                    drawing.render_fixed_range_volume_profile(&painter, &view);
                }
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            for ((line, [r, g, b, a]), want) in ["POC", "VA high", "VA low"]
                .into_iter()
                .zip(colors)
                .zip(want)
            {
                let color =
                    egui::epaint::ColorMode::Solid(Color32::from_rgba_unmultiplied(r, g, b, a));
                let paths: Vec<_> = shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Path(path) if path.stroke.color == color => Some(&path.points),
                        _ => None,
                    })
                    .collect();
                let matches = match paths.as_slice() {
                    [] => want.is_empty(),
                    [points] => {
                        points.len() == want.len()
                            && points.iter().zip(want).all(|(point, &(x, y))| {
                                (point.x - x).abs() < 1e-3 && (point.y - y).abs() < 1e-3
                            })
                    }
                    _ => false,
                };
                if !matches {
                    failures.push(format!(
                        "{name}: developing {line} {paths:?}, expected {want:?}"
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
