//! Volume tools: anchored VWAP and the fixed range and anchored volume profiles

use crate::data::{ProfileSource, VolumeProfile};
use crate::drawings::domain::Drawing;
use crate::drawings::domain::DrawingToolType;
use crate::drawings::rendering::{ChartView, MAX_CACHED_SPANS, bars_between};
use crate::localization as l10n;
use crate::model::{Bar, SpanCache};
use crate::styles::{stroke, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Shape, Stroke};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

/// Bars between the saved running sums of an anchored VWAP.
const VWAP_CHECKPOINT: usize = 1024;

type ProfileCache = SpanCache<Option<VolumeProfile>, (usize, Option<f64>)>;

thread_local! {
    static VWAP_CHECKPOINTS: RefCell<SpanCache<Vec<VwapSums>>> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
    /// Profiles by their number of rows.
    static PROFILES: RefCell<ProfileCache> =
        const { RefCell::new(SpanCache::new(MAX_CACHED_SPANS)) };
}

impl Drawing {
    /// Renders the anchored VWAP from the anchor bar to the last bar, with
    /// bands one and two volume-weighted standard deviations away.
    pub(crate) fn render_anchored_vwap(&self, painter: &egui::Painter, view: &ChartView) {
        let (Some(&anchor), Some(anchor_at)) = (self.points.first(), self.chart_points.first())
        else {
            return;
        };
        let color = self.color32();

        if let Some((first, bars)) = bars_between(view.bars, anchor_at.bar_idx, f32::INFINITY) {
            self.draw_vwap(painter, view, first, bars, color);
        }

        painter.circle_filled(anchor, DESIGN_TOKENS.rounding.lg, color);
        painter.circle_stroke(
            anchor,
            DESIGN_TOKENS.rounding.lg,
            Stroke::new(stroke::THICK, Color32::WHITE),
        );
    }

    fn draw_vwap(
        &self,
        painter: &egui::Painter,
        view: &ChartView,
        first: usize,
        bars: &[Bar],
        color: Color32,
    ) {
        // VWAP, then the bands, in standard deviations from it.
        const OFFSETS: [f64; 5] = [0.0, 1.0, -1.0, 2.0, -2.0];
        const NAMES: [&str; 4] = ["+1σ", "-1σ", "+2σ", "-2σ"];

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
        let mut lines: [Vec<Pos2>; 5] = Default::default();
        let mut last_vwap = None;
        for (i, value) in (from..to).zip(anchored_vwap(bars, &checkpoints, from..to)) {
            let Some((vwap, sigma)) = value else { continue };
            let x = (view.bar_to_x)((first + i) as f32);
            for (line, k) in lines.iter_mut().zip(OFFSETS) {
                line.push(Pos2::new(x, (view.price_to_y)(vwap + k * sigma)));
            }
            last_vwap = Some(vwap);
        }
        let (Some(vwap), Some(&end)) = (last_vwap, lines[0].last()) else {
            return;
        };

        // Bands (the 2σ ones fainter), then the VWAP line, each labelled at
        // the last drawn bar
        let font = egui::FontId::proportional(typography::XS);
        let band_color =
            Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 150);
        let [vwap_line, bands @ ..] = lines;
        for (i, (band, name)) in bands.into_iter().zip(NAMES).enumerate() {
            let label_at = band.last().map(|p| Pos2::new(p.x - 20.0, p.y));
            let alpha = if i < 2 { 1.0 } else { 0.5 };
            painter.add(Shape::line(
                band,
                Stroke::new(stroke::HAIRLINE, band_color.gamma_multiply(alpha)),
            ));
            if let Some(pos) = label_at {
                painter.text(
                    pos,
                    egui::Align2::CENTER_CENTER,
                    name,
                    font.clone(),
                    band_color,
                );
            }
        }
        painter.add(Shape::line(
            vwap_line,
            Stroke::new(self.stroke_width.max(2.0), color),
        ));

        let vwap_bg =
            Rect::from_center_size(Pos2::new(end.x - 60.0, end.y), egui::vec2(80.0, 16.0));
        painter.rect_filled(vwap_bg, DESIGN_TOKENS.rounding.sm, color);
        painter.text(
            vwap_bg.center(),
            egui::Align2::CENTER_CENTER,
            format!("VWAP {vwap:.2}"),
            font,
            Color32::WHITE,
        );
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
        let color = self.color32();
        if let Some((first, bars)) = bars_between(view.bars, a.bar_idx, b.bar_idx) {
            let (left, right) = (p1.x.min(p2.x), p1.x.max(p2.x));
            self.draw_volume_profile(painter, view, first, bars, left, right);
        }
        self.draw_volume_profile_anchors(painter, &[p1, p2], color);
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
        self.draw_volume_profile_anchors(painter, &[anchor], self.color32());
    }

    /// Draws the profile of `bars`, the span that starts at bar `first`, as
    /// rows growing right from `left`, boxed from `left` to `right` across the
    /// bars' price range.
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
            false,
            view.price_to_y,
        );
        let note = match profile.source() {
            ProfileSource::Estimated => "Estimated from bars",
            ProfileSource::Executions { complete: false } => "Partial executions",
            ProfileSource::Executions { complete: true } => return,
        };
        if right - left >= 60.0 {
            painter.text(
                Pos2::new(left + 3.0, (view.price_to_y)(profile.high())),
                egui::Align2::LEFT_TOP,
                l10n::text(painter.ctx(), note),
                egui::FontId::proportional(typography::XS),
                Color32::from_rgba_unmultiplied(self.color[0], self.color[1], self.color[2], 200),
            );
        }
    }

    /// The profile of `bars`, the span that starts at bar `first`: the executed volume at each
    /// price when the view holds executions in the span, else estimated from the bars.
    fn span_profile(
        &self,
        view: &ChartView,
        first: usize,
        bars: &[Bar],
    ) -> Rc<Option<VolumeProfile>> {
        if let (Some(flow), Some(start), Some(end)) = (view.executions, bars.first(), bars.last()) {
            let executed = flow.profile(start.time, end.time);
            let complete = flow
                .bars()
                .keys()
                .next()
                .is_some_and(|&t| t <= executed.time);
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
        span_profile(view, config.rows(), config.row_size(), first, bars)
    }

    /// The profile that a fixed range or anchored volume profile shows over the view's bars,
    /// with its [source](VolumeProfile::source): executed volume when
    /// [`ChartView::executions`] holds any in the selected span, else the bar estimate.
    /// `None` for other tools, a drawing without its anchors, or an empty selection.
    pub fn volume_profile_of(&self, view: &ChartView) -> Option<VolumeProfile> {
        let anchor = |i: usize| self.chart_points.get(i).map(|p| p.bar_idx);
        let (first, bars) = match self.tool_type {
            DrawingToolType::FixedRangeVolumeProfile => {
                bars_between(view.bars, anchor(0)?, anchor(1)?)?
            }
            DrawingToolType::AnchoredVolumeProfile => {
                bars_between(view.bars, anchor(0)?, f32::INFINITY)?
            }
            _ => return None,
        };
        self.span_profile(view, first, bars).as_ref().clone()
    }

    fn draw_volume_profile_anchors(
        &self,
        painter: &egui::Painter,
        anchors: &[Pos2],
        color: Color32,
    ) {
        for &anchor in anchors {
            painter.circle_filled(anchor, DESIGN_TOKENS.rounding.lg, color);
            painter.circle_stroke(
                anchor,
                DESIGN_TOKENS.rounding.lg,
                Stroke::new(stroke::MEDIUM, Color32::WHITE),
            );
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::domain::DrawingToolType;
    use crate::drawings::rendering::sample_bars;

    fn assert_close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
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

    /// The 24 default rows of the sample bars (9 to 15) are 0.25 apart, 12.5 px
    /// at 50 px per unit, so each row is drawn 11.5 px tall with its 1 px gap,
    /// inside a box 300 px tall, whichever way up the scale is. The VAH and VAL
    /// labels sit 8 px outside the value area: beyond the VAH line on the side
    /// away from the VAL line, and the other way round. At 1,000 rows the 0.3 px
    /// rows retain a 0.15 px body instead of disappearing under a fixed 1 px gap.
    #[test]
    fn volume_profile_rows_are_drawn_on_either_scale_orientation() {
        let bars = sample_bars();
        let mut drawing = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
        let upright = |price: f64| (16.0 - price) as f32 * 50.0;
        let inverted = |price: f64| (price - 8.0) as f32 * 50.0;
        for (name, price_to_y, width, row_count, row_height) in [
            ("upright", &upright as &dyn Fn(f64) -> f32, 400.0, 24, 11.5),
            ("inverted", &inverted, 400.0, 24, 11.5),
            ("narrow upright", &upright, 24.0, 24, 11.5),
            ("narrow inverted", &inverted, 24.0, 24, 11.5),
            ("dense upright", &upright, 400.0, 1000, 0.15),
            ("dense inverted", &inverted, 400.0, 1000, 0.15),
        ] {
            drawing.volume_profile = crate::data::VolumeProfileConfig::new(row_count, 0.7);
            let view = ChartView {
                instrument: None,
                bars: &bars,
                revision: 0,
                bar_to_x: &|x| x,
                price_to_y,
                time_axis: true,
                executions: None,
            };
            let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                let painter = egui::Painter::new(
                    ui.ctx().clone(),
                    egui::LayerId::background(),
                    Rect::EVERYTHING,
                );
                drawing.draw_volume_profile(&painter, &view, 0, &bars, 100.0, 100.0 + width);
            });
            let rects: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Rect(r) if r.rect.min.x == 100.0 => Some((r.rect, r.fill)),
                    _ => None,
                })
                .collect();
            // The two full-width lines are the value area edges.
            let mut edges: Vec<f32> = output
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::LineSegment { points, .. }
                        if points[0].x == 100.0 && points[1].x == 100.0 + width =>
                    {
                        Some(points[0].y)
                    }
                    _ => None,
                })
                .collect();
            let label_y = |label: &str| {
                output.shapes.iter().find_map(|c| match &c.shape {
                    egui::Shape::Text(t) if t.galley.job.text == label => {
                        Some(t.pos.y + t.galley.size().y / 2.0)
                    }
                    _ => None,
                })
            };
            let (vah_label, val_label) = (label_y("VAH"), label_y("VAL"));
            let poc_label = label_y("POC");
            output.drop_without_applying_deltas();
            if width < 50.0 {
                assert!(poc_label.is_none(), "{name}: POC exceeds span");
                assert!(
                    vah_label.is_none() && val_label.is_none(),
                    "{name}: labels exceed span"
                );
            } else {
                assert!(
                    poc_label.is_some() && vah_label.is_some() && val_label.is_some(),
                    "{name}: missing labels"
                );
            }
            edges.sort_by(f32::total_cmp);
            let [top, bottom] = edges[..] else {
                panic!("{name}: value area edges {edges:?}")
            };
            // The higher price, VAH, is the top edge upright and the bottom one inverted.
            let (want_vah, want_val) = if name.ends_with("upright") {
                (top - 8.0, bottom + 8.0)
            } else {
                (bottom + 8.0, top - 8.0)
            };
            if let (Some(vah_label), Some(val_label)) = (vah_label, val_label) {
                assert!(
                    (vah_label - want_vah).abs() < 1.0,
                    "{name}: VAH {vah_label}, want {want_vah}"
                );
                assert!(
                    (val_label - want_val).abs() < 1.0,
                    "{name}: VAL {val_label}, want {want_val}"
                );
            }
            let (boxes, rows): (Vec<_>, Vec<_>) = rects
                .into_iter()
                .partition(|&(_, fill)| fill == Color32::TRANSPARENT);
            assert_eq!(rows.len(), row_count, "{name}");
            for (row, _) in rows {
                assert!((row.height() - row_height).abs() < 1e-3, "{name}: {row:?}");
            }
            assert_eq!(boxes.len(), 1, "{name}");
            let outline = boxes[0].0;
            assert!(
                (outline.height() - 300.0).abs() < 1e-3,
                "{name}: {outline:?}"
            );
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
                instrument: None,
                bars,
                revision: 0,
                bar_to_x: &|x| x,
                price_to_y: &|price| price as f32,
                time_axis: true,
                executions: None,
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
        for (bar, price, size, side) in [
            (1, 11.0, 4.0, Some(OrderSide::Sell)),
            (1, 12.0, 6.0, Some(OrderSide::Buy)),
            (1, 12.0, 2.0, None),
            (2, 12.0, 3.0, Some(OrderSide::Buy)),
        ] {
            flow.push(&Tick::new(bars[bar].time, price, size, side))
                .unwrap();
        }
        let view = |executions| ChartView {
            instrument: None,
            bars: &bars,
            revision: 0,
            bar_to_x: &|x| x,
            price_to_y: &|price| price as f32,
            time_axis: true,
            executions,
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
    }
}
