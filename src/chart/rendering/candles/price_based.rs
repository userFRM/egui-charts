//! Price-based chart types built once over the whole history.
//!
//! Renko, Kagi, Line Break, Point and Figure and range bars are path dependent: each element
//! depends on every bar before it. Transforming only the visible window made the elements change
//! with the window start. As with Heikin-Ashi, they are built over the full history, cached per
//! render thread, and the visible window is sliced out of the result. A tick or a new bar adds
//! one bar to the cached builder; a prepend or a correction (a new revision), or a change of
//! parameter, rebuilds.
//!
//! Builders keep only the newest elements (10,000 bricks or lines, 5,000 columns or range bars), and
//! Renko and range bars start late on a long history. A window that starts at or before the
//! oldest kept element is not covered by the build: it is drawn from the window's own bars, as
//! before the history was used.

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use std::thread::LocalKey;

use chrono::{DateTime, Utc};

use crate::model::kagi::{self, KagiBuilder};
use crate::model::line_break::{self, LineBreakBuilder};
use crate::model::point_figure::{self, PnfBuilder};
use crate::model::range_bar::{self, RangeBarBuilder};
use crate::model::renko::{self, RenkoBuilder};
use crate::model::{
    Bar, ChartTypeParams, LineBreakConfig, PointFigureConfig, PriceBuilder, SpanCache,
};

/// A builder and the elements it has built, newest limit included.
pub(super) struct Built<B: PriceBuilder> {
    builder: B,
    elements: Vec<B::Element>,
    /// The element limit of the type, and whether the build started after the first bar (the
    /// work bound): older elements may be missing.
    limit: usize,
    late_start: bool,
}

impl<B: PriceBuilder> Built<B> {
    fn new(builder: B, limit: usize, late_start: bool) -> Self {
        Self {
            elements: builder.elements(),
            builder,
            limit,
            late_start,
        }
    }

    /// The elements drawn for the bars from time `t0` to time `t1`, selected by element end
    /// time, which does not decrease. Element `i` is built over `(end[i - 1], end[i]]`, so the
    /// window runs from the first element that ends at or after `t0` (the last element when none
    /// does: the one in effect) to the first one that ends after `t1`. When older elements were
    /// dropped and the window starts at or before the oldest kept element, the elements that
    /// covered it are gone and `older` builds them from the window's bars, as the visible window
    /// was transformed before the history was.
    pub(super) fn window(
        &self,
        end: impl Fn(&B::Element) -> DateTime<Utc>,
        t0: DateTime<Utc>,
        t1: DateTime<Utc>,
        older: impl FnOnce() -> Vec<B::Element>,
    ) -> Cow<'_, [B::Element]> {
        let elements = &self.elements;
        let trimmed = self.late_start || elements.len() >= self.limit;
        if trimmed && elements.first().is_some_and(|first| t0 <= end(first)) {
            return Cow::Owned(older());
        }
        let lo = elements
            .partition_point(|e| end(e) < t0)
            .min(elements.len().saturating_sub(1));
        let hi = elements.partition_point(|e| end(e) <= t1);
        Cow::Borrowed(&elements[lo..(hi + 1).min(elements.len())])
    }
}

/// Each chart takes two entries (the result before the last bar and the result), so 16 keeps
/// eight charts of one type.
const CACHE_CAPACITY: usize = 16;

thread_local! {
    static RENKO: RefCell<SpanCache<Built<RenkoBuilder>, f64>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
    static KAGI: RefCell<SpanCache<Built<KagiBuilder>, f64>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
    static LINE_BREAK: RefCell<SpanCache<Built<LineBreakBuilder>, usize>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
    static POINT_AND_FIGURE: RefCell<SpanCache<Built<PnfBuilder>, (f64, u32)>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
    static RANGE: RefCell<SpanCache<Built<RangeBarBuilder>, f64>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
}

/// The build for `full` at `revision`: `fresh` makes an empty builder, `start` picks the first
/// bar to push (the work bound of Renko and range bars, 0 for the others).
fn cached<B: PriceBuilder + Clone, P: Copy + PartialEq>(
    cache: &'static LocalKey<RefCell<SpanCache<Built<B>, P>>>,
    param: P,
    full: &[Bar],
    revision: u64,
    limit: usize,
    fresh: impl Fn() -> B,
    start: impl Fn(&[Bar]) -> usize,
) -> Rc<Built<B>> {
    cache.with_borrow_mut(|cache| {
        cache.get_extended(
            param,
            0,
            full,
            revision,
            |bars| {
                let mut builder = fresh();
                let from = start(bars);
                for bar in &bars[from..] {
                    builder.push(bar);
                }
                Built::new(builder, limit, from > 0)
            },
            |built, bar| {
                let mut builder = built.builder.clone();
                builder.push(bar);
                Some(Built::new(builder, limit, built.late_start))
            },
        )
    })
}

/// Renko bricks of `full`; `params.renko_brick_size` must be valid.
pub(super) fn renko(
    full: &[Bar],
    revision: u64,
    params: &ChartTypeParams,
) -> Rc<Built<RenkoBuilder>> {
    let size = params.renko_brick_size;
    cached(
        &RENKO,
        size,
        full,
        revision,
        renko::MAX_BRICKS,
        || RenkoBuilder::new(size),
        |bars| renko::work_start(bars, size),
    )
}

/// Kagi lines of `full`; `params.kagi_reversal` must be valid.
pub(super) fn kagi(
    full: &[Bar],
    revision: u64,
    params: &ChartTypeParams,
) -> Rc<Built<KagiBuilder>> {
    let reversal = params.kagi_reversal;
    cached(
        &KAGI,
        reversal,
        full,
        revision,
        kagi::MAX_LINES,
        || KagiBuilder::new(reversal),
        |_| 0,
    )
}

/// Line Break lines of `full`; `params.line_break_count` must be valid.
pub(super) fn line_break(
    full: &[Bar],
    revision: u64,
    params: &ChartTypeParams,
) -> Rc<Built<LineBreakBuilder>> {
    let count = params.line_break_count;
    cached(
        &LINE_BREAK,
        count,
        full,
        revision,
        line_break::MAX_LINES,
        || LineBreakBuilder::new(&LineBreakConfig::new(count)),
        |_| 0,
    )
}

/// Point and Figure columns of `full`; `params.pnf_box_size` and `pnf_reversal` must be valid.
pub(super) fn point_and_figure(
    full: &[Bar],
    revision: u64,
    params: &ChartTypeParams,
) -> Rc<Built<PnfBuilder>> {
    let (box_size, reversal) = (params.pnf_box_size, params.pnf_reversal);
    let config = PointFigureConfig::new(box_size, reversal as usize);
    cached(
        &POINT_AND_FIGURE,
        (box_size, reversal),
        full,
        revision,
        point_figure::MAX_COLUMNS,
        || PnfBuilder::new(box_size, &config),
        |_| 0,
    )
}

/// Range bars of `full`; `params.range_size` must be valid.
pub(super) fn range_bars(
    full: &[Bar],
    revision: u64,
    params: &ChartTypeParams,
) -> Rc<Built<RangeBarBuilder>> {
    let size = params.range_size;
    cached(
        &RANGE,
        size,
        full,
        revision,
        range_bar::MAX_BARS,
        || RangeBarBuilder::new(size),
        |bars| range_bar::work_start(bars, size),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ChartType, KagiConfig, RangeBarConfig, RenkoConfig, to_kagi_lines, to_line_break_lines,
        to_pnf_columns, to_range_bars_from_ohlc, to_renko_bricks,
    };
    use chrono::{Duration, TimeZone};

    /// One-minute flat bars (open, high, low and close equal) from 2024-01-01; `from` is the
    /// minute of the first bar. The first bar's open can differ from its close.
    fn bars(from: i64, closes: &[f64], first_open: Option<f64>) -> Vec<Bar> {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let o = if i == 0 { first_open.unwrap_or(c) } else { c };
                Bar {
                    time: start + Duration::minutes(from + i as i64),
                    open: o,
                    high: o.max(c),
                    low: o.min(c),
                    close: c,
                    volume: 0.0,
                }
            })
            .collect()
    }

    /// The (open, close) or (start, end) pairs drawn for bars `window.0..=window.1` of `full`.
    fn drawn(
        chart_type: ChartType,
        full: &[Bar],
        revision: u64,
        params: &ChartTypeParams,
        window: (usize, usize),
    ) -> Vec<(f64, f64)> {
        let (t0, t1) = (full[window.0].time, full[window.1].time);
        // What drawing transforms from the window's bars alone, for a window the build lost.
        let visible = &full[window.0..=window.1];
        match chart_type {
            ChartType::Renko => {
                let built = renko(full, revision, params);
                let config = RenkoConfig::new(params.renko_brick_size);
                let w = built.window(|e| e.ts, t0, t1, || to_renko_bricks(visible, &config));
                w.iter().map(|e| (e.open, e.close)).collect()
            }
            ChartType::Kagi => {
                let built = kagi(full, revision, params);
                let config = KagiConfig::new(params.kagi_reversal);
                let w = built.window(|e| e.ts, t0, t1, || to_kagi_lines(visible, &config));
                w.iter().map(|e| (e.start_price, e.end_price)).collect()
            }
            ChartType::LineBreak => {
                let built = line_break(full, revision, params);
                let config = LineBreakConfig::new(params.line_break_count);
                let w = built.window(|e| e.ts, t0, t1, || to_line_break_lines(visible, &config));
                w.iter().map(|e| (e.open, e.close)).collect()
            }
            ChartType::PointAndFigure => {
                let built = point_and_figure(full, revision, params);
                let config =
                    PointFigureConfig::new(params.pnf_box_size, params.pnf_reversal as usize);
                let w = built.window(|e| e.end_time, t0, t1, || to_pnf_columns(visible, &config));
                w.iter().map(|e| (e.start_price, e.end_price)).collect()
            }
            ChartType::Range => {
                let built = range_bars(full, revision, params);
                let config = RangeBarConfig::new(params.range_size);
                let w = built.window(
                    |e| e.end_time,
                    t0,
                    t1,
                    || to_range_bars_from_ohlc(visible, &config),
                );
                w.iter().map(|e| (e.open, e.close)).collect()
            }
            other => unreachable!("{other:?} is not price-based"),
        }
    }

    #[test]
    fn price_based_charts_are_built_over_the_whole_history() {
        // (name, chart type, params, calls as (bars, revision), window of the last call, expected)
        type Row = (
            &'static str,
            ChartType,
            ChartTypeParams,
            Vec<(Vec<Bar>, u64)>,
            (usize, usize),
            (usize, (f64, f64), (f64, f64)),
        );
        let u = 1.0 / 256.0;
        let z = [
            100.0,
            110.0,
            110.0 - 5.0 * u,
            110.0,
            110.0 - 13.0 * u,
            110.0,
            100.0,
        ];
        let with_last = |s: &[f64], last: f64| {
            let mut v = s.to_vec();
            *v.last_mut().unwrap() = last;
            v
        };
        let w = [100.0, 103.0, 100.5, 104.0];
        let w_then_105: Vec<f64> = w.iter().copied().chain([105.0]).collect();
        let w_after_prepend: Vec<f64> = [104.0].into_iter().chain(w).collect();
        let lb = [11.0, 12.0, 13.0, 10.5];
        let w_corrected = [100.0, 102.0, 100.5, 104.0];
        // A history far past the element limit, alternating 100 and 110.
        let alternating: Vec<f64> = (0..3000).map(|i| [100.0, 110.0][i % 2]).collect();
        // Closes 100 to 110, then 30 bars that move nothing.
        let quiet: Vec<f64> = (100..=110).map(f64::from).chain([109.5; 30]).collect();
        // Every non-Renko row sets a Renko size that would give one element if it were read.
        let params = |f: &dyn Fn(&mut ChartTypeParams)| {
            let mut p = ChartTypeParams {
                renko_brick_size: 1000.0,
                ..Default::default()
            };
            f(&mut p);
            p
        };
        let tick = |s: &[f64], open: Option<f64>| {
            vec![
                (bars(0, &with_last(s, 105.0), open), 0),
                (bars(0, s, open), 0),
            ]
        };
        let rows: Vec<Row> = vec![
            (
                "renko window",
                ChartType::Renko,
                ChartTypeParams::default(),
                vec![
                    (bars(0, &with_last(&w, 103.5), None), 0),
                    (bars(0, &w, None), 1),
                ],
                (2, 3),
                (3, (102.0, 101.0), (103.0, 104.0)),
            ),
            (
                "renko append",
                ChartType::Renko,
                ChartTypeParams::default(),
                vec![(bars(0, &w, None), 0), (bars(0, &w_then_105, None), 0)],
                (3, 4),
                (3, (102.0, 103.0), (104.0, 105.0)),
            ),
            (
                "renko prepend",
                ChartType::Renko,
                ChartTypeParams::default(),
                vec![(bars(1, &w, None), 0), (bars(0, &w_after_prepend, None), 2)],
                (2, 4),
                (5, (101.0, 102.0), (103.0, 104.0)),
            ),
            (
                "renko after a mid-history correction",
                ChartType::Renko,
                ChartTypeParams::default(),
                vec![(bars(0, &w, None), 0), (bars(0, &w_corrected, None), 5)],
                (0, 3),
                (4, (100.0, 101.0), (103.0, 104.0)),
            ),
            (
                "renko window older than the kept bricks",
                ChartType::Renko,
                ChartTypeParams::default(),
                vec![(bars(0, &alternating, None), 0)],
                (1880, 1900),
                (181, (100.0, 101.0), (101.0, 100.0)),
            ),
            (
                "point and figure at a quiet live edge",
                ChartType::PointAndFigure,
                params(&|_| {}),
                vec![(bars(0, &quiet, None), 0)],
                (31, 40),
                (1, (100.0, 110.0), (100.0, 110.0)),
            ),
            (
                "line break at a quiet live edge",
                ChartType::LineBreak,
                params(&|_| {}),
                vec![(bars(0, &quiet, None), 0)],
                (31, 40),
                (1, (109.0, 110.0), (109.0, 110.0)),
            ),
            (
                "kagi",
                ChartType::Kagi,
                params(&|p| p.kagi_reversal = 1.0 / 64.0),
                tick(&z, None),
                (0, 6),
                (6, (100.0, 110.0), (110.0, 100.0)),
            ),
            (
                "point and figure",
                ChartType::PointAndFigure,
                params(&|p| {
                    p.pnf_box_size = 1.0 / 64.0;
                    p.pnf_reversal = 2;
                }),
                tick(&z, None),
                (0, 6),
                (6, (100.0, 110.0), (109.984375, 100.0)),
            ),
            (
                "range bars",
                ChartType::Range,
                params(&|p| p.range_size = 1.0 / 32.0),
                tick(&z, None),
                (0, 6),
                (643, (100.0, 100.03125), (100.0, 100.0)),
            ),
            (
                "line break",
                ChartType::LineBreak,
                params(&|p| p.line_break_count = 2),
                tick(&lb, Some(10.0)),
                (0, 3),
                (4, (10.0, 11.0), (12.0, 10.5)),
            ),
        ];
        let failures: Vec<String> = rows
            .iter()
            .filter_map(|(name, chart_type, params, calls, window, want)| {
                let mut got = Vec::new();
                for (i, (full, revision)) in calls.iter().enumerate() {
                    // Only the last call's window is asserted.
                    let window = if i + 1 == calls.len() {
                        *window
                    } else {
                        (0, 0)
                    };
                    got = drawn(*chart_type, full, *revision, params, window);
                }
                let at = |i: Option<&(f64, f64)>| i.copied().unwrap_or_default();
                let got = (got.len(), at(got.first()), at(got.last()));
                (got != *want).then(|| format!("{name}: {got:?}, want {want:?}"))
            })
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
