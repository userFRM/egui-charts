//! Bars merged when several fall within one pixel, so a zoomed-out frame draws about one bar
//! per pixel column however many bars are in view.

use crate::chart::renderers::ChartMapping;
use crate::model::{Bar, ChartType, PriceSource};
use std::ops::Range;

/// Consecutive bars drawn as one at `bar_spacing` points: the smallest power of two that spans
/// a physical pixel, or 1 while bars are at least a pixel apart. Groups start at multiples of
/// the factor, so panning keeps them and zooming splits or joins them in pairs.
pub(super) fn factor(bar_spacing: f32, pixels_per_point: f32) -> usize {
    let pixels = f64::from(bar_spacing * pixels_per_point);
    if pixels >= 1.0 || pixels.is_nan() || pixels <= 0.0 {
        return 1;
    }
    1 << (1.0 / pixels).log2().ceil().min(30.0) as u32
}

/// Whether `chart_type` merges as a line, `Some(true)`, or as bars, `Some(false)`; `None` for
/// chart types that draw unmerged.
pub(super) fn line_style(chart_type: ChartType) -> Option<bool> {
    match chart_type {
        ChartType::Candles
        | ChartType::Bars
        | ChartType::HollowCandles
        | ChartType::VolumeCandles
        | ChartType::Columns
        | ChartType::HlcArea
        | ChartType::HighLow => Some(false),
        ChartType::Line
        | ChartType::LineWithMarkers
        | ChartType::StepLine
        | ChartType::Area
        | ChartType::Baseline => Some(true),
        _ => None,
    }
}

/// The first, lowest, highest and last of `values`, in order and without repeats: the points
/// that draw the same line through one pixel column as all of the values would.
pub(super) fn column_points(values: impl Iterator<Item = (usize, f64)>) -> Vec<(usize, f64)> {
    let mut points: [Option<(usize, f64)>; 4] = [None; 4];
    for value in values {
        let [first, low, high, last] = &mut points;
        first.get_or_insert(value);
        if low.is_none_or(|low| value.1 < low.1) {
            *low = Some(value);
        }
        if high.is_none_or(|high| value.1 > high.1) {
            *high = Some(value);
        }
        *last = Some(value);
    }
    let mut points: Vec<_> = points.into_iter().flatten().collect();
    points.sort_by_key(|point| point.0);
    points.dedup_by_key(|point| point.0);
    points
}

/// `coords` with group `g` of `factor` bars at the position of its first bar, `g * factor`.
pub(super) fn group_mapping(coords: &ChartMapping, factor: usize, start: usize) -> ChartMapping {
    let base = coords.base_idx / factor;
    ChartMapping {
        bar_spacing: coords.bar_spacing * factor as f32,
        start_idx: start,
        base_idx: base,
        right_offset: (coords.base_idx as f32 + coords.right_offset + 0.5) / factor as f32
            - 0.5
            - base as f32,
        ..*coords
    }
}

/// A group of consecutive bars as merged drawing reads it.
#[derive(Clone)]
pub(super) struct Group {
    /// The first open, highest high, lowest low and last close at the first bar's time, with
    /// the highest volume.
    pub bar: Bar,
    /// The bar with the highest volume.
    pub peak: usize,
    /// The bars with the lowest and highest price-source value.
    pub extremes: [usize; 2],
}

/// The groups of the main bars at the frame's factor, rebuilt when the bars, the factor or the
/// price source change, so a frame that only pans or hovers does not read every bar in view.
#[derive(Default)]
pub(super) struct Groups {
    key: Option<(u64, usize, PriceSource)>,
    /// Group `g` holds bars `g * factor` up to the next multiple; none at a factor of 1.
    pub groups: Vec<Group>,
}

impl Groups {
    /// The groups of `factor` bars of `bars`, at `revision`, with extremes of `source`.
    pub fn update(&mut self, bars: &[Bar], revision: u64, factor: usize, source: PriceSource) {
        if self.key == Some((revision, factor, source)) {
            return;
        }
        self.key = Some((revision, factor, source));
        if factor == 1 {
            self.groups = Vec::new();
            return;
        }
        let value = |bar: &Bar| source.compute(bar.open, bar.high, bar.low, bar.close);
        self.groups = bars
            .chunks(factor)
            .enumerate()
            .map(|(index, chunk)| {
                let first = index * factor;
                let mut group = Group {
                    bar: chunk[0].clone(),
                    peak: first,
                    extremes: [first; 2],
                };
                let mut extremes = [value(&chunk[0]); 2];
                for (offset, bar) in chunk.iter().enumerate().skip(1) {
                    group.bar.high = group.bar.high.max(bar.high);
                    group.bar.low = group.bar.low.min(bar.low);
                    if bar.volume > group.bar.volume {
                        group.bar.volume = bar.volume;
                        group.peak = first + offset;
                    }
                    let value = value(bar);
                    if value < extremes[0] {
                        extremes[0] = value;
                        group.extremes[0] = first + offset;
                    }
                    if value > extremes[1] {
                        extremes[1] = value;
                        group.extremes[1] = first + offset;
                    }
                }
                group.bar.close = chunk[chunk.len() - 1].close;
                group
            })
            .collect();
    }

    /// Bars per group: 1 while bars are drawn one by one.
    pub fn factor(&self) -> usize {
        self.key.map_or(1, |(_, factor, _)| factor)
    }

    /// The groups covering bars `range`.
    pub fn covering(&self, range: Range<usize>) -> Range<usize> {
        let factor = self.factor();
        let end = range.end.div_ceil(factor).min(self.groups.len());
        (range.start / factor).min(end)..end
    }
}

/// Visible bars merged for drawing, from the group before the first visible one when there is
/// one, so a merged bar can be compared with its predecessor.
pub(super) struct Merged {
    /// The merged bars.
    pub bars: Vec<Bar>,
    /// Source index, possibly fractional, at which each merged bar is drawn.
    pub index: Vec<f32>,
    /// First merged bar in view.
    pub start: usize,
}

impl Merged {
    /// Merges `range` of `bars` with their `groups`. Bars draw one bar per group at
    /// its centre, so each pixel shows the extremes and volume height that all of its bars
    /// would. A `line` keeps the first, lowest, highest and last `source` value of each group
    /// at their own bars, which draws the same line through the column as every value would.
    pub fn new(
        bars: &[Bar],
        groups: &Groups,
        range: Range<usize>,
        line: bool,
        source: PriceSource,
    ) -> Self {
        let factor = groups.factor();
        let visible = groups.covering(range);
        let mut merged = Self {
            bars: Vec::new(),
            index: Vec::new(),
            start: 0,
        };
        if visible.is_empty() {
            return merged;
        }
        for g in visible.start.saturating_sub(1)..visible.end {
            if g == visible.start {
                merged.start = merged.bars.len();
            }
            let group = &groups.groups[g];
            let first = g * factor;
            let last = (first + factor).min(bars.len()) - 1;
            if line {
                let mut points = [first, group.extremes[0], group.extremes[1], last];
                points.sort_unstable();
                for (i, &index) in points.iter().enumerate() {
                    if i == 0 || points[i - 1] != index {
                        let bar = &bars[index];
                        let price = source.compute(bar.open, bar.high, bar.low, bar.close);
                        merged
                            .bars
                            .push(Bar::new(bar.time, price, price, price, price, bar.volume));
                        merged.index.push(index as f32);
                    }
                }
            } else {
                merged.bars.push(group.bar.clone());
                merged.index.push((first + last) as f32 / 2.0);
            }
        }
        merged
    }
}
