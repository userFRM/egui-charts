//! Volume tools: anchored VWAP and the fixed range and anchored volume profiles

use crate::data::{StudyDataRequest, VolumeProfile};
use crate::drawings::domain::Drawing;
use crate::drawings::domain::DrawingToolType;
use crate::drawings::rendering::{ChartView, MAX_CACHED_SPANS, bars_between};
use crate::model::{Bar, ProfileResolution, SpanCache, SpanFingerprint, Timeframe};
use crate::painting::volume_profile::ProfileStyle;
use crate::styles::stroke;
use chrono::{DateTime, Utc};
use egui::{Color32, Pos2, Shape, Stroke};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

/// Bars between the saved running sums of an anchored VWAP.
const VWAP_CHECKPOINT: usize = 1024;

/// Colour of the anchored VWAP's bands, its first band's fill included.
const VWAP_BAND_COLOR: Color32 = Color32::from_rgb(76, 175, 80);

type ProfileCache = SpanCache<Option<VolumeProfile>, (usize, Option<f64>)>;

/// Rows, row size, the finer span and its history's address.
type FinerCache = SpanCache<Option<VolumeProfile>, (usize, Option<f64>, SpanFingerprint, usize)>;

thread_local! {
    static VWAP_CHECKPOINTS: RefCell<SpanCache<Vec<VwapSums>>> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
    /// Profiles by their number of rows.
    static PROFILES: RefCell<ProfileCache> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
    /// Profiles built from finer bars.
    static FINER_PROFILES: RefCell<FinerCache> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
}

impl Drawing {
    /// Renders the anchored VWAP from the anchor bar to the last bar at the drawing's width,
    /// with the enabled bands of its [`VwapConfig`](crate::drawings::VwapConfig), the first one
    /// filled at 5%.
    pub(crate) fn render_anchored_vwap(&self, painter: &egui::Painter, view: &ChartView) {
        let Some(anchor_at) = self.chart_points.first() else {
            return;
        };
        if let Some((first, bars)) = bars_between(view.bars, anchor_at.bar_idx, f32::INFINITY) {
            self.draw_vwap(painter, view, first, bars);
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

        // The checkpoints only cover whole blocks before the last bar, so the
        // forming bar's updates reuse them.
        let covered = (bars.len() - 1) / VWAP_CHECKPOINT * VWAP_CHECKPOINT;
        let checkpoints = VWAP_CHECKPOINTS.with_borrow_mut(|cache| {
            cache.get((), first, &bars[..covered], view.revision, vwap_checkpoints)
        });
        // The VWAP, then the upper and lower line of every enabled band, in standard deviations
        // from it.
        let config = self.vwap.unwrap_or_default();
        let mut offsets = vec![0.0];
        for band in config.bands.iter().filter(|b| b.enabled) {
            offsets.extend([band.multiplier(), -band.multiplier()]);
        }
        let mut lines = vec![Vec::with_capacity(to - from); offsets.len()];
        for (i, value) in (from..to).zip(anchored_vwap(bars, &checkpoints, from..to)) {
            let Some((vwap, sigma)) = value else { continue };
            let x = (view.bar_to_x)((first + i) as f32);
            for (line, k) in lines.iter_mut().zip(&offsets) {
                line.push(Pos2::new(x, (view.price_to_y)(vwap + k * sigma)));
            }
        }
        if lines[0].is_empty() {
            return;
        }

        if config.bands[0].enabled {
            super::super::fibonacci::paint_band(
                painter,
                &lines[1],
                &lines[2],
                VWAP_BAND_COLOR.gamma_multiply(0.05),
            );
        }
        let vwap_line = lines.remove(0);
        for line in lines {
            painter.add(Shape::line(
                line,
                Stroke::new(stroke::HAIRLINE, VWAP_BAND_COLOR),
            ));
        }
        painter.add(Shape::line(
            vwap_line,
            Stroke::new(self.stroke_width, self.color32()),
        ));
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
            DrawingToolType::FixedRangeVolumeProfile => bars_between(bars, anchor(0)?, anchor(1)?),
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

    /// Renders the volume profile of the bars between the two anchors, with
    /// the point of control and the value area.
    pub(crate) fn render_fixed_range_volume_profile(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
    ) {
        let (&[p1, p2, ..], &[a, b, ..]) = (self.points.as_slice(), self.chart_points.as_slice())
        else {
            return;
        };
        if let Some((first, bars)) = bars_between(view.bars, a.bar_idx, b.bar_idx) {
            let (left, right) = (p1.x.min(p2.x), p1.x.max(p2.x));
            self.draw_volume_profile(painter, view, first, bars, left, right);
        }
    }

    /// Renders the volume profile from the anchor bar to the last bar.
    pub(crate) fn render_anchored_volume_profile(&self, painter: &egui::Painter, view: &ChartView) {
        let (Some(&anchor), Some(anchor_at)) = (self.points.first(), self.chart_points.first())
        else {
            return;
        };
        if let Some((first, bars)) = bars_between(view.bars, anchor_at.bar_idx, f32::INFINITY) {
            let right = (view.bar_to_x)((first + bars.len() - 1) as f32);
            self.draw_volume_profile(painter, view, first, bars, anchor.x, right);
        }
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
            let complete = flow
                .complete_from()
                .is_some_and(|from| from <= executed.time);
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
    /// [`ChartView::executions`] holds complete bars in the selected span, those from
    /// [`Footprint::complete_from`](crate::data::Footprint::complete_from), else the finer bars
    /// of [`ChartView::finer`], else the bar estimate.
    /// `None` for other tools, a drawing without its anchors, or an empty selection.
    pub fn volume_profile_of(&self, view: &ChartView) -> Option<VolumeProfile> {
        let (first, bars) = self.profile_span(view.bars)?;
        self.span_profile(view, first, bars).as_ref().clone()
    }
}

/// Running sums of volume, typical price times volume, and squared typical
/// price times volume.
#[derive(Clone, Copy, Default)]
struct VwapSums {
    volume: f64,
    price_volume: f64,
    square_volume: f64,
}

impl VwapSums {
    fn add(&mut self, bar: &Bar) {
        let price = bar.typical_price();
        self.volume += bar.volume;
        self.price_volume += price * bar.volume;
        self.square_volume += price * price * bar.volume;
    }

    /// VWAP and the volume-weighted standard deviation around it; `None`
    /// until volume trades.
    fn value(&self) -> Option<(f64, f64)> {
        (self.volume > 0.0).then(|| {
            let vwap = self.price_volume / self.volume;
            let variance = self.square_volume / self.volume - vwap * vwap;
            (vwap, variance.max(0.0).sqrt())
        })
    }
}

/// The running sums at the start of the span and after every block of
/// `VWAP_CHECKPOINT` bars of `prefix`, the span's leading whole blocks, so the
/// VWAP at any bar takes fewer than `VWAP_CHECKPOINT` further additions.
fn vwap_checkpoints(prefix: &[Bar]) -> Vec<VwapSums> {
    let mut sums = VwapSums::default();
    let mut checkpoints = vec![sums];
    for block in prefix.chunks(VWAP_CHECKPOINT) {
        block.iter().for_each(|bar| sums.add(bar));
        checkpoints.push(sums);
    }
    checkpoints
}

/// VWAP from the first bar of the span, with its standard deviation, at each
/// bar of the non-empty `range`, resuming from the checkpoint before it.
fn anchored_vwap<'a>(
    bars: &'a [Bar],
    checkpoints: &[VwapSums],
    range: Range<usize>,
) -> impl Iterator<Item = Option<(f64, f64)>> + 'a {
    let checkpoint = range.start / VWAP_CHECKPOINT;
    let mut sums = checkpoints[checkpoint];
    bars[checkpoint * VWAP_CHECKPOINT..range.start]
        .iter()
        .for_each(|bar| sums.add(bar));
    bars[range].iter().map(move |bar| {
        sums.add(bar);
        sums.value()
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
    // A host replacement can keep the span's fingerprint while changing its middle bars.
    // shortcut: in-place interior corrections wait for a span change; use a history revision when feeds need those corrections immediately.
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

    /// The line keeps the drawing's width and only the enabled bands are drawn, band 1 filled.
    #[test]
    fn anchored_vwap_draws_its_line_and_enabled_bands_only() {
        use crate::drawings::{ChartPoint, VwapBand, VwapConfig};
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
        let mid = [100.0, 90.0, 90.0];
        let mut other_bands = VwapConfig::default();
        other_bands.bands[0].enabled = false;
        other_bands.bands[1].enabled = true;
        other_bands.bands[2] = VwapBand::new(true, 0.5).unwrap();
        let mut no_bands = VwapConfig::default();
        no_bands.bands[0].enabled = false;
        for (name, anchor, config, width, fill, expected) in [
            (
                "default",
                0.0,
                None,
                1.0,
                true,
                vec![
                    ([100.0, 80.0, 85.0], VWAP_BAND_COLOR, 1.0),
                    ([100.0, 100.0, 95.0], VWAP_BAND_COLOR, 1.0),
                    (mid, blue, 1.0),
                ],
            ),
            (
                "bands 2 and 3",
                0.0,
                Some(other_bands),
                3.0,
                false,
                vec![
                    ([100.0, 70.0, 80.0], VWAP_BAND_COLOR, 1.0),
                    ([100.0, 110.0, 100.0], VWAP_BAND_COLOR, 1.0),
                    ([100.0, 85.0, 87.5], VWAP_BAND_COLOR, 1.0),
                    ([100.0, 95.0, 92.5], VWAP_BAND_COLOR, 1.0),
                    (mid, blue, 3.0),
                ],
            ),
            (
                "no bands",
                0.0,
                Some(no_bands),
                1.0,
                false,
                vec![(mid, blue, 1.0)],
            ),
            ("past last bar", 4.0, None, 1.0, false, vec![]),
        ] {
            let mut drawing = Drawing::new(1, DrawingToolType::AnchoredVWAP);
            drawing.color = [41, 98, 255, 255];
            drawing.stroke_width = width;
            drawing.vwap = config;
            drawing.chart_points = vec![ChartPoint::new(anchor, 50.0)];
            let view = ChartView {
                instrument: None,
                price_formatter: None,
                bars: &bars,
                revision: 0,
                bar_to_x: &|b| 100.0 + 10.0 * b,
                price_to_y: &|p| ((20.0 - p) * 10.0) as f32,
                y_to_price: &|y| 20.0 - f64::from(y) / 10.0,
                time_axis: true,
                executions: None,
                timeframe: None,
                finer: &[],
            };
            let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                let painter = egui::Painter::new(
                    ui.ctx().clone(),
                    egui::LayerId::background(),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 600.0)),
                );
                drawing.render_anchored_vwap(&painter, &view);
            });
            let shapes = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            assert_eq!(shapes.len(), expected.len() + usize::from(fill), "{name}");
            if fill {
                let Shape::Mesh(mesh) = &shapes[0].shape else {
                    panic!("{name}: band fill")
                };
                assert!(
                    mesh.vertices
                        .iter()
                        .all(|v| v.color == Color32::from_rgba_premultiplied(4, 9, 4, 13)),
                    "{name}"
                );
            }
            for (shape, (ys, color, width)) in shapes[usize::from(fill)..].iter().zip(expected) {
                let Shape::Path(line) = &shape.shape else {
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
        }
    }

    /// Reference: VWAP of (h + l + c) / 3 from bar 1, and the standard
    /// deviation from its definition, sqrt(sum v (tp - vwap)^2 / sum v).
    #[test]
    fn anchored_vwap_matches_reference() {
        let want = [
            (11.666666666666664, 0.0),
            (11.595238095238095, 0.08247860988423196),
            (12.166666666666666, 0.6201736729460428),
            (12.675925925925924, 0.9757431833967023),
            (12.775641025641026, 0.8255305309085764),
            (12.78395061728395, 0.8112060000493586),
        ];
        let bars = &sample_bars()[1..];
        let got: Vec<_> = anchored_vwap(bars, &vwap_checkpoints(&[]), 0..bars.len()).collect();
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
        // blocks long, the forming bar in the third block.
        let long: Vec<Bar> = (0..3 * VWAP_CHECKPOINT)
            .map(|i| {
                let close = 100.0 + (i * 37 % 101) as f64 / 10.0;
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                let volume = (1 + i % 7) as f64;
                Bar::new(time, close, close + 1.0, close - 1.5, close, volume)
            })
            .collect();
        let checkpoints = vwap_checkpoints(&long[..2 * VWAP_CHECKPOINT]);
        for window in [
            VWAP_CHECKPOINT + 5..VWAP_CHECKPOINT + 9,
            2 * VWAP_CHECKPOINT..2 * VWAP_CHECKPOINT + 4,
        ] {
            let got = anchored_vwap(&long, &checkpoints, window.clone());
            for (i, got) in window.zip(got) {
                let prefix = &long[..=i];
                let volume: f64 = prefix.iter().map(|b| b.volume).sum();
                let vwap = prefix
                    .iter()
                    .map(|b| b.typical_price() * b.volume)
                    .sum::<f64>()
                    / volume;
                let variance = prefix
                    .iter()
                    .map(|b| b.volume * (b.typical_price() - vwap).powi(2))
                    .sum::<f64>()
                    / volume;
                let (got_vwap, got_sigma) = got.unwrap();
                assert!(
                    (got_vwap - vwap).abs() < 1e-9,
                    "bar {i}: vwap {got_vwap}, want {vwap}"
                );
                assert!(
                    (got_sigma - variance.sqrt()).abs() < 1e-6,
                    "bar {i}: sigma {got_sigma}, want {}",
                    variance.sqrt()
                );
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
                        instrument: None,
                        bars: &bars,
                        revision: 0,
                        bar_to_x: &|x| x,
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
                    let span_width = if anchored {
                        width.min(clip.width())
                    } else {
                        width
                    };
                    assert!(
                        (widest - span_width / 3.0).abs() < 1e-3,
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

    /// The profile of a span that ends on the forming bar follows its ticks
    /// and the next bar, extending the cached profile of the bars before it
    /// while they trade inside its range and rebuilding the rows otherwise.
    /// Either way it is the profile built from all the bars.
    #[test]
    fn span_profile_follows_the_forming_bar() {
        let check = |bars: &[Bar], case: &str| {
            let view = ChartView {
                price_formatter: None,
                instrument: None,
                bars,
                revision: 0,
                bar_to_x: &|x| x,
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
            instrument: None,
            bars: &bars,
            revision: 0,
            bar_to_x: &|x| x,
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
        let profile = drawing.volume_profile_of(&view(Some(&flow))).unwrap();
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
        let profile = drawing.volume_profile_of(&view(Some(&flow))).unwrap();
        assert_eq!((profile.low(), profile.volume()), (11.5, [3.0].as_slice()));

        // A span with no executions, and a view without any, fall back to the estimate.
        select(&mut drawing, 4.0, 5.0);
        for executions in [Some(&flow), None] {
            let profile = drawing.volume_profile_of(&view(executions)).unwrap();
            assert_eq!(profile.source(), ProfileSource::Estimated);
        }
        // Executions held from the span's first bar are complete.
        select(&mut drawing, 1.0, 2.0);
        let profile = drawing.volume_profile_of(&view(Some(&flow))).unwrap();
        assert_eq!(
            profile.source(),
            ProfileSource::Executions { complete: true }
        );
        // Once the store drops the sell of 4, bar 1 is cut: the same span counts bar 2 alone
        // and is partial.
        let mut cut = flow.clone();
        cut.drop_through(trades[0].ts, &trades[1..]);
        let profile = drawing.volume_profile_of(&view(Some(&cut))).unwrap();
        assert_eq!(
            profile.source(),
            ProfileSource::Executions { complete: false }
        );
        assert_eq!(profile.volume(), [3.0]);
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
                bars: &daily,
                revision: 0,
                bar_to_x: &|x| x,
                price_to_y: &|price| price as f32,
                y_to_price: &f64::from,
                time_axis: true,
                executions: executed.then_some(&flow),
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
