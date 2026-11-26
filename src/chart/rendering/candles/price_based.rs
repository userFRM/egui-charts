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
//! Renko and range bars start late on a long history. The time scale starts at the oldest kept element.

mod studies;
pub(crate) use studies::ElementStudies;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::LocalKey;

use crate::model::kagi::{self, KagiBuilder};
use crate::model::line_break::{self, LineBreakBuilder};
use crate::model::point_figure::{self, PnfBuilder};
use crate::model::range_bar::{self, RangeBarBuilder};
use crate::model::renko::{self, RenkoBuilder};
use crate::model::{
    Bar, ChartType, ChartTypeParams, LineBreakConfig, PointFigureConfig, PriceBasedSource,
    PriceBuilder,
};
use crate::painting::span_cache::SpanCache;

/// A builder and the elements it has built, newest limit included.
pub(super) struct Built<B: PriceBuilder> {
    builder: B,
    last_source: Option<Bar>,
    elements: Vec<B::Element>,
    bars: Arc<[Bar]>,
    /// The element limit of the type, and whether the build started after the first bar (the
    /// work bound): older elements may be missing.
    limit: usize,
    late_start: bool,
}

impl<B: PriceBuilder> Built<B> {
    fn new(
        builder: B,
        limit: usize,
        late_start: bool,
        last_source: Option<Bar>,
        bar: fn(&B::Element) -> Bar,
    ) -> Self {
        let elements = builder.elements();
        let bars = elements.iter().map(bar).collect();
        Self {
            elements,
            bars,
            builder,
            last_source,
            limit,
            late_start,
        }
    }

    /// The elements in the visible slots, or none when the build has no such range.
    pub(super) fn shown(&self, start: usize, len: usize) -> &[B::Element] {
        self.elements.get(start..start + len).unwrap_or_default()
    }

    /// The bars of the elements in the visible slots, as [`Self::shown`].
    pub(super) fn shown_bars(&self, start: usize, len: usize) -> &[Bar] {
        self.bars.get(start..start + len).unwrap_or_default()
    }

    fn series(&self) -> (Arc<[Bar]>, bool) {
        (
            self.bars.clone(),
            self.late_start || self.elements.len() >= self.limit,
        )
    }
}

/// Each chart takes two entries (the result before the last bar and the result), so 16 keeps
/// eight charts of one type.
const CACHE_CAPACITY: usize = 16;

/// Box size, reversal, source and one step back of a Point and Figure chart.
type PnfKey = (f64, u32, PriceBasedSource, bool);

thread_local! {
    static RENKO: RefCell<SpanCache<Built<RenkoBuilder>, (f64, PriceBasedSource, bool)>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
    static KAGI: RefCell<SpanCache<Built<KagiBuilder>, (f64, PriceBasedSource)>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
    static LINE_BREAK: RefCell<SpanCache<Built<LineBreakBuilder>, (usize, PriceBasedSource)>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
    static POINT_AND_FIGURE: RefCell<SpanCache<Built<PnfBuilder>, PnfKey>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
    static RANGE: RefCell<SpanCache<Built<RangeBarBuilder>, f64>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
}

/// The build for `full` at `revision`: `fresh` makes an empty builder, `start` picks the first
/// bar to push (the work bound of Renko and range bars, 0 for the others). Whitespace bars are
/// not pushed.
fn cached<B: PriceBuilder + Clone, P: Copy + PartialEq>(
    cache: &'static LocalKey<RefCell<SpanCache<Built<B>, P>>>,
    param: P,
    full: &[Bar],
    revision: u64,
    limit: usize,
    fresh: impl Fn() -> B,
    start: impl Fn(&[Bar]) -> usize,
    bar: fn(&B::Element) -> Bar,
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
                for bar in bars[from..].iter().filter(|bar| !bar.is_whitespace()) {
                    builder.push(bar);
                }
                Built::new(
                    builder,
                    limit,
                    from > 0,
                    bars.iter().rfind(|b| !b.is_whitespace()).cloned(),
                    bar,
                )
            },
            |built, next| {
                let mut builder = built.builder.clone();
                let mut last_source = built.last_source.clone();
                if !next.is_whitespace() {
                    // Reject an extension that a fresh build would skip, including across whitespace.
                    if let Some(previous) = &last_source
                        && start(&[previous.clone(), next.clone()]) > 0
                    {
                        return None;
                    }
                    builder.push(next);
                    last_source = Some(next.clone());
                }
                Some(Built::new(
                    builder,
                    limit,
                    built.late_start,
                    last_source,
                    bar,
                ))
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
        (size, params.renko_source, params.renko_wicks),
        full,
        revision,
        renko::MAX_BRICKS,
        || RenkoBuilder::new(size, params.renko_source),
        |bars| renko::work_start(bars, size, params.renko_source),
        if params.renko_wicks {
            renko::RenkoBrick::to_bar
        } else {
            |brick| {
                let mut bar = brick.to_bar();
                bar.high = bar.open.max(bar.close);
                bar.low = bar.open.min(bar.close);
                bar
            }
        },
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
        (reversal, params.kagi_source),
        full,
        revision,
        kagi::MAX_LINES,
        || KagiBuilder::new(reversal, params.kagi_source),
        |_| 0,
        kagi::KagiLine::to_bar,
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
        (count, params.line_break_source),
        full,
        revision,
        line_break::MAX_LINES,
        || {
            LineBreakBuilder::new(&LineBreakConfig {
                line_count: count,
                source: params.line_break_source,
            })
        },
        |_| 0,
        |line| Bar {
            time: line.ts,
            open: line.open,
            high: line.high(),
            low: line.low(),
            close: line.close,
            volume: 0.0,
            colors: None,
            payload: None,
        },
    )
}

/// Point and Figure columns of `full`; `params.pnf_box_size` and `pnf_reversal` must be valid.
pub(super) fn point_and_figure(
    full: &[Bar],
    revision: u64,
    params: &ChartTypeParams,
) -> Rc<Built<PnfBuilder>> {
    let (box_size, reversal) = (params.pnf_box_size, params.pnf_reversal);
    let config = PointFigureConfig {
        use_close: params.pnf_source == PriceBasedSource::Close,
        one_step_back: params.pnf_one_step_back,
        ..PointFigureConfig::new(box_size, reversal as usize)
    };
    cached(
        &POINT_AND_FIGURE,
        (
            box_size,
            reversal,
            params.pnf_source,
            params.pnf_one_step_back,
        ),
        full,
        revision,
        point_figure::MAX_COLUMNS,
        || PnfBuilder::new(box_size, &config),
        |_| 0,
        |column| Bar {
            time: column.end_time,
            open: column.start_price,
            high: column.high(),
            low: column.low(),
            close: column.end_price,
            volume: 0.0,
            colors: None,
            payload: None,
        },
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
        range_bar::RangeBar::to_bar,
    )
}

/// Range bars built from executions, after the range bars the chart's bars approximate before
/// the first held trade. The executions are built again when they, the range size or the replay
/// cutoff change, and joined again when the approximated part changes.
#[derive(Default)]
pub(crate) struct ExecutionRanges {
    key: Option<(f64, u64, Option<chrono::DateTime<chrono::Utc>>)>,
    /// The range bars of the held trades, whether each closed without a trade in its range,
    /// and the time of the first trade.
    executions: Option<(Vec<Bar>, Vec<bool>, chrono::DateTime<chrono::Utc>)>,
    /// The approximated range bars before the bar holding the first held trade, as the cache built
    /// them: a new build gives a new `Arc`. Held instead of the build, whose `Rc` is not `Send`.
    approximated: Option<Arc<[Bar]>>,
    built: Option<(Arc<[Bar]>, bool)>,
    /// Whether `built` starts with approximated range bars.
    mixed: bool,
    /// Whether each bar of `built` closed without a trade in its range.
    phantoms: Vec<bool>,
    /// Whether the last range bar of the held trades is the bar in progress.
    open: bool,
}

impl ExecutionRanges {
    /// One bar per range bar, and whether older range bars were left out by the bar limit. The
    /// range bars come from the trades before `cutoff` (all of them without one), after the
    /// closed range bars approximated from the bars of `history` that open before the bar of
    /// `clock` that the first such trade joins, on a daily or longer clock with a session the bar
    /// of its trading date; `trades_revision` identifies `trades` and
    /// `history_revision` `history`. `None` for an invalid range size, one that waits for
    /// history or without a trade, when the range bars come from the chart's OHLC bars alone.
    pub(crate) fn bars<'a>(
        &mut self,
        params: &ChartTypeParams,
        history: &[Bar],
        history_revision: u64,
        clock: &crate::model::BarClock,
        trades_revision: u64,
        trades: impl Iterator<Item = &'a crate::model::Tick>,
        cutoff: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Option<(Arc<[Bar]>, bool)> {
        if params.invalid(ChartType::Range).is_some()
            || params.bars_needed(ChartType::Range).is_some()
        {
            *self = Self::default();
            return None;
        }
        let key = (params.range_size, trades_revision, cutoff);
        if self.key != Some(key) {
            let mut builder = RangeBarBuilder::new(params.range_size);
            let mut first = None;
            for trade in trades.filter(|t| cutoff.is_none_or(|c| t.ts < c)) {
                first.get_or_insert(trade.ts);
                builder.push_tick(trade);
            }
            self.open = builder.has_open_bar();
            let elements = builder.elements();
            let bars = elements.iter().map(range_bar::RangeBar::to_bar).collect();
            let phantoms = elements.iter().map(|bar| bar.tick_cnt == 0).collect();
            self.executions = first.map(|first| (bars, phantoms, first));
            self.key = Some(key);
            self.built = None;
        }
        let Some((executions, execution_phantoms, first)) = &self.executions else {
            (self.built, self.approximated, self.mixed) = (None, None, false);
            self.phantoms.clear();
            return None;
        };
        // The bar holding the first trade is left out: the trades from it on replace its OHLC. A
        // trade-based bar opens at its first trade, so the last one open by then holds it.
        let before = if clock.timeframe().is_some_and(|tf| tf.is_trade_based()) {
            history
                .partition_point(|bar| bar.time <= *first)
                .saturating_sub(1)
        } else {
            let open = clock.instant_open(*first);
            history.partition_point(|bar| clock.bar_open(bar.time) < open)
        };
        let approximated =
            (before > 0).then(|| range_bars(&history[..before], history_revision, params));
        let unchanged = match (&approximated, &self.approximated) {
            (Some(new), Some(old)) => Arc::ptr_eq(&new.bars, old),
            (new, old) => new.is_none() && old.is_none(),
        };
        if self.built.is_none() || !unchanged {
            // The range in progress at the first trade is dropped: the trades open the next one.
            let (closed, closed_elements, late_start) =
                approximated
                    .as_deref()
                    .map_or((&[][..], &[][..], false), |built| {
                        let len = built.bars.len() - usize::from(built.builder.has_open_bar());
                        (&built.bars[..len], &built.elements[..len], built.late_start)
                    });
            let all = closed.len() + executions.len();
            let skipped = all.saturating_sub(range_bar::MAX_BARS);
            let bars: Arc<[Bar]> = closed
                .iter()
                .chain(executions)
                .skip(skipped)
                .cloned()
                .collect();
            self.phantoms = (closed_elements.iter())
                .map(|bar| bar.tick_cnt == 0)
                .chain(execution_phantoms.iter().copied())
                .skip(skipped)
                .collect();
            self.built = Some((bars, late_start || all >= range_bar::MAX_BARS));
            self.mixed = closed.len() > skipped;
            self.approximated = approximated.map(|built| built.bars.clone());
        }
        self.built.clone()
    }

    /// What the last bars were built from.
    pub(crate) fn source(&self) -> crate::model::RangeBarSource {
        use crate::model::RangeBarSource::{Executions, OhlcApproximation, OhlcThenExecutions};
        match (&self.built, self.mixed) {
            (None, _) => OhlcApproximation,
            (Some(_), false) => Executions,
            (Some(_), true) => OhlcThenExecutions,
        }
    }
}

/// Whether each of the `len` range bars the chart shows from `start` on closed without a trade
/// in its range: those built from `executions` while it holds them, else those approximated from
/// `full`, for which `params.range_size` must be valid.
pub(crate) fn range_phantoms(
    executions: &ExecutionRanges,
    full: &[Bar],
    revision: u64,
    params: &ChartTypeParams,
    start: usize,
    len: usize,
) -> Vec<bool> {
    if executions.built.is_some() {
        let phantoms = executions.phantoms.get(start..start + len);
        return phantoms.unwrap_or_default().to_vec();
    }
    let built = range_bars(full, revision, params);
    let shown = built.shown(start, len);
    shown.iter().map(|bar| bar.tick_cnt == 0).collect()
}

/// Whether the newest range bar the chart shows is the bar in progress: the last one built from
/// `executions` while it holds them, else the last one approximated from `full`, for which
/// `params.range_size` must be valid.
pub(crate) fn range_forming(
    executions: &ExecutionRanges,
    full: &[Bar],
    revision: u64,
    params: &ChartTypeParams,
) -> bool {
    if executions.built.is_some() {
        return executions.open;
    }
    range_bars(full, revision, params).builder.has_open_bar()
}

/// The element containing a loaded time. Columns include quiet bars up to the next column's
/// start; other elements cover the interval up to their completion. Repeated completions
/// choose the first slot. Times in unloaded history or outside the source series have no slot.
pub(crate) fn element_index_at(
    state: &crate::studies::ChartState,
    chart_type: ChartType,
    params: &ChartTypeParams,
    older_bars_drawn: bool,
    time: chrono::DateTime<chrono::Utc>,
) -> Option<usize> {
    let source = &state.data().bars;
    let (first, last) = source.first().zip(source.last())?;
    if !state.shows_elements() || time < first.time || time > last.time {
        return None;
    }
    let elements = state.shown();
    if chart_type == ChartType::PointAndFigure {
        let built = point_and_figure(source, state.revision(), params);
        if time < built.elements.first()?.start_time {
            return None;
        }
        return Some(
            built
                .elements
                .partition_point(|column| column.start_time <= time)
                .saturating_sub(1),
        );
    }
    if !older_bars_drawn && time < elements.first()?.time {
        return None;
    }
    Some(
        elements
            .partition_point(|bar| bar.time < time)
            .min(elements.len() - 1),
    )
}

/// Maps host marker times to their element slots, excluding unloaded history.
pub(crate) fn markers<'a>(
    markers: impl IntoIterator<Item = (usize, &'a crate::model::Marker)>,
    state: &crate::studies::ChartState,
    chart_type: ChartType,
    params: &ChartTypeParams,
    older_bars_drawn: bool,
) -> Vec<(usize, crate::model::Marker)> {
    markers
        .into_iter()
        .filter_map(|(key, marker)| {
            let index = element_index_at(state, chart_type, params, older_bars_drawn, marker.time)?;
            let mut marker = marker.clone();
            marker.time = state.shown()[index].time;
            Some((key, marker))
        })
        .collect()
}

/// One bar per kept element, and whether older elements were left out by the element or work limit.
/// Time-based types, invalid parameters, sizes waiting for history and empty builds keep the
/// source bars' axes.
pub(crate) fn elements(
    chart_type: ChartType,
    params: &ChartTypeParams,
    bars: &[Bar],
    revision: u64,
) -> Option<(Arc<[Bar]>, bool)> {
    if params.invalid(chart_type).is_some() || params.bars_needed(chart_type).is_some() {
        return None;
    }
    let built = match chart_type {
        ChartType::Renko => renko(bars, revision, params).series(),
        ChartType::Kagi => kagi(bars, revision, params).series(),
        ChartType::LineBreak => line_break(bars, revision, params).series(),
        ChartType::PointAndFigure => point_and_figure(bars, revision, params).series(),
        ChartType::Range => range_bars(bars, revision, params).series(),
        _ => return None,
    };
    (!built.0.is_empty()).then_some(built)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};

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
                    colors: None,
                    payload: None,
                }
            })
            .collect()
    }

    /// Studies consume element OHLC, and time annotations land on completion slots.
    #[cfg(feature = "studies")]
    #[test]
    #[cfg(feature = "drawings")]
    fn studies_and_annotations_follow_price_based_elements() {
        use crate::{
            drawings::DrawingToolType,
            model::{BarData, Marker, MarkerPos},
            studies::{CustomIndicator, IndicatorValue, RSI, SMA},
            widget::{
                Chart, SeriesId,
                test_frames::{frame, shapes},
            },
        };
        use egui::{Color32, Context, Event, Modifiers, PointerButton, Shape};
        for kind in [
            ChartType::Renko,
            ChartType::Kagi,
            ChartType::LineBreak,
            ChartType::PointAndFigure,
            ChartType::Range,
        ] {
            let ctx = Context::default();
            let closes: Vec<_> = (0..8)
                .flat_map(|_| [100.0, 103.0, 101.0, 104.0, 100.0, 105.0])
                .collect();
            let source = bars(0, &closes, None);
            let mut chart = Chart::new(BarData::from_bars(source.clone()));
            let sma = chart.add_indicator(Box::new(SMA::new(3)));
            let study_color = Color32::from_rgb(17, 83, 149);
            chart
                .indicator_mut(sma)
                .unwrap()
                .set_colors(vec![study_color]);
            chart.add_indicator(Box::new(RSI::new(2)));
            chart.add_indicator(Box::new(CustomIndicator::new("Fit", |bars| {
                vec![IndicatorValue::Single(120.0); bars.len()]
            })));
            assert_eq!(
                chart
                    .indicators()
                    .find(|(id, _)| *id == sma)
                    .unwrap()
                    .1
                    .values()
                    .value(0, 2),
                Some(304.0 / 3.0)
            );
            chart.set_chart_type_params(ChartTypeParams {
                kagi_reversal: 1.0,
                ..Default::default()
            });
            chart.set_chart_type(kind);
            chart.set_visible_bars(300);
            let marker = Marker::new(source[16].time)
                .with_pos(MarkerPos::InBar)
                .with_text("element marker");
            chart.set_markers(SeriesId::MAIN, vec![marker.clone()]);
            frame(&ctx, &mut chart, 0.0, vec![]);
            frame(&ctx, &mut chart, 0.1, vec![]);
            frame(&ctx, &mut chart, 0.15, vec![]);
            assert!(
                chart.state.price_scale().range().max >= 120.0,
                "overlay fit on {kind:?}"
            );
            assert_eq!(
                chart.get_rendered_indicator_panes().len(),
                1,
                "RSI pane on {kind:?}"
            );
            assert_eq!(
                chart
                    .indicators()
                    .find(|(id, _)| *id == sma)
                    .unwrap()
                    .1
                    .values()
                    .value(0, 2),
                Some(304.0 / 3.0)
            );
            let elements = chart.state.shown().to_vec();
            let value_at = |at| {
                chart
                    .study_values_at(at)
                    .into_iter()
                    .find(|row| row.study == "SMA")
                    .unwrap()
                    .value
            };
            for at in 2..elements.len() {
                let want = elements[at - 2..=at]
                    .iter()
                    .map(|bar| bar.close)
                    .sum::<f64>()
                    / 3.0;
                assert!(
                    (value_at(at).unwrap() - want).abs() < 1e-9,
                    "{kind:?} SMA at {at}"
                );
            }
            if kind == ChartType::Renko {
                // The first three one-unit closes are 101, 102, 103: SMA(3) = 102.
                assert_eq!(value_at(2), Some(102.0));
            }
            assert!(!chart.study_values_at(2).is_empty());
            let at = elements
                .partition_point(|bar| bar.time < marker.time)
                .min(elements.len() - 1);
            let x = chart.get_chart_mapping().idx_to_x(at);
            let painted = shapes(&ctx, &mut chart, 0.2, vec![]);
            let map = chart.get_chart_mapping();
            let first_value = chart
                .study_values_at(2)
                .into_iter()
                .find(|row| row.study == "SMA")
                .unwrap()
                .value
                .unwrap();
            let first = egui::pos2(map.idx_to_x(2), map.price_to_y(first_value));
            assert!(
                painted.iter().any(|shape| match shape {
                    Shape::LineSegment { points, stroke } =>
                        stroke.color == study_color
                            && points.iter().any(|p| p.distance(first) < 0.1),
                    Shape::Path(path) =>
                        path.stroke.color == egui::epaint::ColorMode::Solid(study_color)
                            && path.points.iter().any(|p| p.distance(first) < 0.1),
                    _ => false,
                }),
                "SMA geometry on {kind:?}"
            );
            assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == "element marker" && (text.pos.x + text.galley.size().x * 0.5 - x).abs() < 0.1)), "marker slot on {kind:?}");
            chart
                .drawings_mut()
                .set_active_tool(Some(DrawingToolType::TrendLine));
            let mut now = 0.2;
            for index in [2, 5] {
                let pos = egui::pos2(map.idx_to_x(index), map.price_to_y(102.0));
                for events in [
                    vec![Event::PointerMoved(pos)],
                    vec![Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    }],
                    vec![Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    }],
                ] {
                    now += 0.05;
                    frame(&ctx, &mut chart, now, events);
                }
            }
            assert_eq!(chart.drawings().drawings.len(), 1, "draw on {kind:?}");
            let saved = chart.save_drawings().unwrap();
            let times: Vec<_> = saved.drawings[0]
                .points
                .iter()
                .map(|point| point.ts_ms)
                .collect();
            chart.set_chart_type(ChartType::Candles);
            frame(&ctx, &mut chart, 0.6, vec![]);
            assert_eq!(
                chart.save_drawings().unwrap().drawings[0]
                    .points
                    .iter()
                    .map(|p| p.ts_ms)
                    .collect::<Vec<_>>(),
                times
            );
            chart.set_chart_type(kind);
            frame(&ctx, &mut chart, 0.7, vec![]);
            assert_eq!(
                chart.save_drawings().unwrap().drawings[0]
                    .points
                    .iter()
                    .map(|p| p.ts_ms)
                    .collect::<Vec<_>>(),
                times
            );
            // A live correction must update every affected element output, without changing bar results.
            let mut live = source.last().unwrap().clone();
            live.close = 108.0;
            live.high = 108.0;
            chart.update(SeriesId::MAIN, live);
            frame(&ctx, &mut chart, 1.0, vec![]);
            let elements = chart.state.shown();
            let output = chart
                .study_values_at(elements.len() - 1)
                .into_iter()
                .find(|row| row.study == "SMA")
                .unwrap()
                .value;
            let last = elements.len() - 1;
            let want = elements[last - 2..]
                .iter()
                .map(|bar| bar.close)
                .sum::<f64>()
                / 3.0;
            assert!((output.unwrap() - want).abs() < 1e-9, "live {kind:?}");
            chart
                .indicator_mut(sma)
                .unwrap()
                .set_input("period", crate::studies::InputValue::Length(2))
                .unwrap();
            frame(&ctx, &mut chart, 1.5, vec![]);
            let elements = chart.state.shown();
            let output = chart
                .study_values_at(1)
                .into_iter()
                .find(|row| row.study == "SMA")
                .unwrap()
                .value;
            assert_eq!(
                output,
                Some((elements[0].close + elements[1].close) / 2.0),
                "edited input on {kind:?}"
            );
            if kind == ChartType::PointAndFigure {
                // Quiet bars at 109 and 108 still belong to the up column, before the 107 reversal.
                let quiet = bars(0, &[100.0, 110.0, 109.0, 108.0, 107.0], None);
                chart.set_markers(
                    SeriesId::MAIN,
                    vec![
                        Marker::new(quiet[2].time)
                            .with_pos(MarkerPos::InBar)
                            .with_text("quiet marker"),
                    ],
                );
                chart.set_data(SeriesId::MAIN, quiet);
                frame(&ctx, &mut chart, 2.0, vec![]);
                let x = chart.get_chart_mapping().idx_to_x(0);
                let painted = shapes(&ctx, &mut chart, 2.1, vec![]);
                assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == "quiet marker" && (text.pos.x + text.galley.size().x * 0.5 - x).abs() < 0.1)), "quiet time belongs to the existing column");
            }
        }
    }

    /// A range bar is drawn over its whole range, wherever in it the bar opened.
    #[test]
    fn range_bars_are_painted_over_their_whole_range() {
        use crate::model::BarData;
        use crate::widget::{
            Chart,
            test_frames::{frame, shapes},
        };
        // With a range of 2, the fall from 100 to 99 and the rise to 101 complete a bar that
        // opened at 100, in the middle of its range from 99 to 101.
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let source = [
            (100.0, 100.0, 100.0, 100.0),
            (100.0, 100.0, 99.0, 99.0),
            (99.0, 101.0, 99.0, 101.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (open, high, low, close))| {
            let time = start + Duration::minutes(i as i64);
            Bar::new(time, open, high, low, close, 1.0)
        })
        .collect();
        let mut chart = Chart::new(BarData::from_bars(source));
        chart.set_chart_type_params(ChartTypeParams {
            range_size: 2.0,
            ..Default::default()
        });
        chart.set_chart_type(ChartType::Range);
        let ctx = egui::Context::default();
        frame(&ctx, &mut chart, 0.0, vec![]);
        frame(&ctx, &mut chart, 0.1, vec![]);
        let painted = shapes(&ctx, &mut chart, 0.2, vec![]);
        let map = chart.get_chart_mapping();
        let colors = [chart.config().bullish_color, chart.config().bearish_color];
        let first = painted
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::Rect(rect)
                    if colors.contains(&rect.fill)
                        && (rect.rect.center().x - map.idx_to_x(0)).abs() < 0.5 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("the first range bar");
        let (top, bottom) = (map.price_to_y(101.0), map.price_to_y(99.0));
        assert!(
            (first.top() - top).abs() < 0.01 && (first.bottom() - bottom).abs() < 0.01,
            "painted {} to {}, want {top} to {bottom}",
            first.top(),
            first.bottom()
        );
    }

    /// The Range, Renko and Line Break styles reach the painter. With a range of 2, the closes 100, 101 and
    /// 99 complete a bar from 100 up to 101 and down to 99, which thin bars draw as a stroke from
    /// 99 to 101 at the wick width with ticks at 100 and 99. With a range of 1, a bar that opens
    /// at 103.5 after a close at 100 completes the bars 100 to 101, 101 to 102 and 102 to 103 in
    /// one jump: only the first held a price, so phantom bars outline the second and third. A bar
    /// that trades from 100 to 103.5 completes them along its path, and none is a gap. With a
    /// range of 2, the closes 100 and 102 complete a bar from 100 to 102, and a close at 103 or
    /// 101 reaches neither 104 nor 100: the bar from 102 is in progress, rising or falling, and
    /// takes the forming colour of its direction, filled or thin, while it is in view. Bricks of
    /// 1 from 100 to 102 take the border and wick colours set for rising bricks, a translucent
    /// border as set. Shown Line Break borders take the colour set for rising lines, or else the
    /// rising body colour at full opacity.
    #[test]
    fn range_and_renko_styles_reach_the_painter() {
        use crate::model::BarData;
        use crate::widget::{
            Chart,
            test_frames::{frame, shapes},
        };
        use egui::{Color32, Shape};
        let paint = |kind: ChartType, params: ChartTypeParams, source: Vec<Bar>| {
            let mut chart = Chart::new(BarData::from_bars(source));
            chart.set_chart_type_params(params);
            chart.set_chart_type(kind);
            chart
                .series_style_mut(crate::widget::SeriesId::MAIN)
                .unwrap()
                .price_line_visible = false;
            let ctx = egui::Context::default();
            frame(&ctx, &mut chart, 0.0, vec![]);
            frame(&ctx, &mut chart, 0.1, vec![]);
            let painted = shapes(&ctx, &mut chart, 0.2, vec![]);
            (painted, chart)
        };

        let mut params = ChartTypeParams {
            range_size: 2.0,
            ..Default::default()
        };
        params.styles.range.thin_bars = true;
        let (painted, chart) = paint(
            ChartType::Range,
            params,
            bars(0, &[100.0, 101.0, 99.0], None),
        );
        let map = chart.get_chart_mapping();
        let (x, down) = (map.idx_to_x(0), chart.config().bearish_color);
        // A slot holds what lies within half the spacing of its centre.
        let half = (map.idx_to_x(1) - x) / 2.0;
        let near = |px: f32| (px - x).abs() < half;
        let wick = chart.config().wick_width;
        let parts: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::LineSegment {
                    points: [a, b],
                    stroke,
                } if stroke.color == down && near(a.x) && near(b.x) => {
                    let part = if a.x == b.x {
                        "stroke"
                    } else if a.x < x && b.x == x {
                        "open tick"
                    } else if a.x == x && b.x > x {
                        "close tick"
                    } else {
                        "other"
                    };
                    Some((part, a.y, b.y, stroke.width == wick))
                }
                _ => None,
            })
            .collect();
        let y = |price| map.price_to_y(price);
        assert_eq!(
            parts,
            [
                ("stroke", y(101.0), y(99.0), true),
                ("open tick", y(100.0), y(100.0), true),
                ("close tick", y(99.0), y(99.0), true),
            ],
            "thin range bar"
        );
        assert!(
            !painted
                .iter()
                .any(|shape| matches!(shape, Shape::Rect(r) if r.fill == down)),
            "a thin range bar has no block"
        );

        // The same three bars, completed by one bar that trades from 100 up to 103.5.
        let mut traded = bars(0, &[100.0, 103.5], None);
        (traded[1].open, traded[1].low) = (100.0, 100.0);
        for (source, phantom_bars, gap) in [
            (bars(0, &[100.0, 103.5], None), false, "filled"),
            (bars(0, &[100.0, 103.5], None), true, "outlined"),
            (traded, true, "filled"),
        ] {
            let mut params = ChartTypeParams::default();
            params.styles.range.phantom_bars = phantom_bars;
            let (painted, chart) = paint(ChartType::Range, params, source);
            let map = chart.get_chart_mapping();
            let up = chart.config().bullish_color;
            let drawn: Vec<_> = (0..3)
                .map(|i| {
                    let x = map.idx_to_x(i);
                    painted.iter().find_map(|shape| match shape {
                        Shape::Rect(r) if (r.rect.center().x - x).abs() < 0.5 => {
                            Some(if r.fill == up {
                                "filled"
                            } else if r.fill == Color32::TRANSPARENT && r.stroke.color == up {
                                "outlined"
                            } else {
                                "other"
                            })
                        }
                        _ => None,
                    })
                })
                .collect();
            assert_eq!(
                drawn,
                [Some("filled"), Some(gap), Some(gap)],
                "phantom bars {phantom_bars}, gaps {gap}"
            );
        }

        let forming = [Color32::from_rgb(7, 8, 9), Color32::from_rgb(10, 11, 12)];
        // The candle and forming colours painted in each of the first two slots, without repeats.
        let slots = |painted: &[Shape], chart: &Chart| {
            let map = chart.get_chart_mapping();
            let config = chart.config();
            let known = [
                config.bullish_color,
                config.bearish_color,
                forming[0],
                forming[1],
            ];
            let half = (map.idx_to_x(1) - map.idx_to_x(0)) / 2.0;
            (0..2)
                .map(|i| {
                    let x = map.idx_to_x(i);
                    let near = |px: f32| (px - x).abs() < half;
                    let mut found = Vec::new();
                    for shape in painted {
                        let color = match shape {
                            Shape::Rect(r) if (r.rect.center().x - x).abs() < 0.5 => r.fill,
                            Shape::LineSegment {
                                points: [a, b],
                                stroke,
                            } if near(a.x) && near(b.x) => stroke.color,
                            _ => continue,
                        };
                        if known.contains(&color) && !found.contains(&color) {
                            found.push(color);
                        }
                    }
                    found
                })
                .collect::<Vec<_>>()
        };
        for (thin_bars, closes, side) in [
            (false, [100.0, 102.0, 103.0], 0),
            (false, [100.0, 102.0, 101.0], 1),
            (true, [100.0, 102.0, 103.0], 0),
            (true, [100.0, 102.0, 101.0], 1),
        ] {
            let mut params = ChartTypeParams {
                range_size: 2.0,
                ..Default::default()
            };
            let style = &mut params.styles.range;
            style.thin_bars = thin_bars;
            [style.forming_up, style.forming_down] = forming;
            let (painted, mut chart) = paint(ChartType::Range, params, bars(0, &closes, None));
            let up = chart.config().bullish_color;
            assert_eq!(
                slots(&painted, &chart),
                [vec![up], vec![forming[side]]],
                "thin bars {thin_bars}, closes {closes:?}"
            );
            if thin_bars || side != 0 {
                continue;
            }
            // Scrolled back until the newest bar is out of view, no bar is drawn as in progress.
            let (spacing, _) = chart.get_time_scale_state();
            let ctx = egui::Context::default();
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.apply_synced_time_scale(spacing, -1.6);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            assert_eq!(slots(&painted, &chart), [vec![up], vec![]], "scrolled back");
        }

        let (border, wick) = (
            Color32::from_rgba_unmultiplied(1, 2, 3, 128),
            Color32::from_rgb(4, 5, 6),
        );
        let mut params = ChartTypeParams {
            renko_wicks: true,
            ..Default::default()
        };
        params.styles.renko.border_up = Some(border);
        params.styles.renko.wick_up = Some(wick);
        let (painted, chart) = paint(
            ChartType::Renko,
            params,
            bars(0, &[100.0, 101.0, 102.0], None),
        );
        let plot = chart.get_rendered_price_rect();
        let borders = painted
            .iter()
            .filter(|shape| {
                matches!(shape, Shape::Rect(r)
                    if r.stroke.color == border && plot.contains(r.rect.center()))
            })
            .count();
        let wicks = painted
            .iter()
            .filter(|shape| {
                matches!(shape, Shape::LineSegment { points, stroke }
                    if stroke.color == wick && points[0].x == points[1].x)
            })
            .count();
        assert_eq!((borders, wicks), (2, 2), "renko borders and wicks");

        for (visible, own, count) in [
            (true, Some(border), 3),
            (true, None, 3),
            (false, Some(border), 0),
            (false, None, 0),
        ] {
            let mut params = ChartTypeParams::default();
            let style = &mut params.styles.line_break;
            style.border_visible = visible;
            style.border_up = own;
            let (painted, chart) = paint(
                ChartType::LineBreak,
                params,
                bars(0, &[100.0, 101.0, 102.0], None),
            );
            let plot = chart.get_rendered_price_rect();
            let expected = own.unwrap_or(chart.config().bullish_color);
            let borders = painted
                .iter()
                .filter(|shape| {
                    matches!(shape, Shape::Rect(r)
                    if r.stroke.width > 0.0 && r.stroke.color == expected
                        && plot.contains(r.rect.center()))
                })
                .count();
            assert_eq!(borders, count, "line break borders {visible}, {own:?}");
        }
    }

    #[test]
    fn box_sizes_freeze_through_streaming_and_wait_for_history() {
        use crate::model::{BarData, BoxSizeMethod};
        use crate::widget::{Chart, SeriesId};
        let mut data = bars(0, &[100.0; 4], None);
        for (i, bar) in data.iter_mut().enumerate() {
            bar.high += i as f64 + 1.0;
            bar.low -= i as f64 + 1.0;
        }
        let paint = |chart: &mut Chart| {
            let ctx = egui::Context::default();
            ctx.run_ui(egui::RawInput::default(), |ui| {
                chart.show(ui);
            })
            .drop_without_applying_deltas();
        };
        let sizes = |chart: &Chart| {
            let p = chart.chart_type_params();
            [
                p.renko_brick_size,
                p.kagi_reversal,
                p.range_size,
                p.pnf_box_size,
            ]
        };
        for (method, initial, recalculated) in [
            (BoxSizeMethod::Fixed, 2.0, 2.0),
            (
                BoxSizeMethod::Atr {
                    period: 3,
                    multiplier: 1.5,
                },
                8.0,
                169.0 / 3.0,
            ),
            (BoxSizeMethod::Percentage(1.0), 1.0, 2.0),
        ] {
            let mut chart = Chart::new(BarData::from_bars(data.clone()));
            chart.set_chart_type_params(ChartTypeParams {
                renko_brick_size: 2.0,
                kagi_reversal: 2.0,
                range_size: 2.0,
                pnf_box_size: 2.0,
                renko_size_method: method,
                kagi_size_method: method,
                range_size_method: method,
                pnf_size_method: method,
                ..Default::default()
            });
            assert_eq!(sizes(&chart), [initial; 4]);
            chart.update(
                SeriesId::MAIN,
                Bar::new(
                    data[3].time + Duration::minutes(1),
                    200.0,
                    202.0,
                    198.0,
                    200.0,
                    1.0,
                ),
            );
            for kind in [
                ChartType::Renko,
                ChartType::Kagi,
                ChartType::Range,
                ChartType::PointAndFigure,
            ] {
                chart.set_chart_type(kind);
                paint(&mut chart);
                assert_eq!(
                    sizes(&chart),
                    [initial; 4],
                    "{kind:?} changed its size on append"
                );
            }
            let saved = serde_json::to_string(chart.chart_type_params()).unwrap();
            chart.set_chart_type_params(serde_json::from_str(&saved).unwrap());
            for size in sizes(&chart) {
                assert!((size - recalculated).abs() < 1e-12);
            }
        }
        let mut chart = Chart::new(BarData::new());
        chart.set_chart_type_params(ChartTypeParams {
            renko_size_method: BoxSizeMethod::Atr {
                period: 3,
                multiplier: 1.0,
            },
            ..Default::default()
        });
        for (i, bar) in data.into_iter().enumerate() {
            chart.update(SeriesId::MAIN, bar);
            paint(&mut chart);
            assert_eq!(
                chart.chart_type_params().renko_brick_size,
                if i < 2 { 0.0 } else { 4.0 }
            );
        }
        // Fourteen flat bars give an ATR(14) of 0, so the size waits for a bar with a range,
        // streamed in or reached by a replay step: a 15th true range of max(2, 1, 1) = 2 smooths
        // the seed of 0 to 2 / 14.
        let mut flat = Chart::new(BarData::new());
        flat.set_chart_type_params(ChartTypeParams {
            renko_size_method: BoxSizeMethod::Atr {
                period: 14,
                multiplier: 1.0,
            },
            ..Default::default()
        });
        let mut stream = bars(0, &[100.0; 15], None);
        (stream[14].high, stream[14].low) = (101.0, 99.0);
        let last = stream[14].time;
        for (i, bar) in stream.into_iter().enumerate() {
            flat.update(SeriesId::MAIN, bar);
            paint(&mut flat);
            let size = if i < 14 { 0.0 } else { 2.0 / 14.0 };
            assert!((flat.chart_type_params().renko_brick_size - size).abs() < 1e-12);
        }
        for (cursor, size) in [(last, 0.0), (last + Duration::minutes(1), 2.0 / 14.0)] {
            flat.set_replay_time(Some(cursor));
            paint(&mut flat);
            assert!((flat.chart_type_params().renko_brick_size - size).abs() < 1e-12);
        }
    }

    #[test]
    fn sources_and_renko_wicks_reach_the_cached_series() {
        let mut data = bars(0, &[100.0, 100.0, 100.0], None);
        data[1].high = 103.0;
        data[2].low = 97.0;
        // High/low: Renko 100->103, then a reversal from 102 down to 97;
        // Kagi 100->103->97; Line Break 100->100, 100->103, 100->97.
        for (kind, count, first, last) in [
            (ChartType::Renko, 8, (100.0, 101.0), (98.0, 97.0)),
            (ChartType::Kagi, 2, (100.0, 103.0), (103.0, 97.0)),
            (ChartType::LineBreak, 3, (100.0, 100.0), (100.0, 97.0)),
        ] {
            let close = ChartTypeParams::default();
            let before = elements(kind, &close, &data, 0);
            assert!(before.is_none_or(|(bars, _)| bars.len() == 1));
            let params = ChartTypeParams {
                renko_source: PriceBasedSource::HighLow,
                kagi_source: PriceBasedSource::HighLow,
                line_break_source: PriceBasedSource::HighLow,
                ..Default::default()
            };
            let (built, _) = elements(kind, &params, &data, 0).unwrap();
            assert_eq!(built.len(), count, "{kind:?}");
            assert_eq!((built[0].open, built[0].close), first);
            assert_eq!((built[count - 1].open, built[count - 1].close), last);
            let restored = elements(kind, &close, &data, 0);
            assert!(restored.is_none_or(|(bars, _)| bars.len() == 1));
        }
        // Point and Figure with 1-point boxes and 3-box reversals: the high of 104 lifts the
        // first column to 104 and the same bar's low of 100 reverses it to a column from 103
        // down to 100; close-only input sees neither.
        let mut data = bars(0, &[100.0, 100.0], None);
        data[1].high = 104.0;
        for (source, count, first, last) in [
            (PriceBasedSource::HighLow, 2, (100.0, 104.0), (103.0, 100.0)),
            (PriceBasedSource::Close, 1, (100.0, 100.0), (100.0, 100.0)),
        ] {
            let params = ChartTypeParams {
                pnf_source: source,
                ..Default::default()
            };
            let (built, _) = elements(ChartType::PointAndFigure, &params, &data, 0).unwrap();
            assert_eq!(built.len(), count, "{source:?}");
            assert_eq!((built[0].open, built[0].close), first, "{source:?}");
            let end = &built[count - 1];
            assert_eq!((end.open, end.close), last, "{source:?}");
        }
        let data = bars(10, &[100.0, 103.0, 101.5, 104.0], None);
        let mut params = ChartTypeParams::default();
        let (hidden, _) = elements(ChartType::Renko, &params, &data, 0).unwrap();
        assert_eq!((hidden[3].high, hidden[3].low), (104.0, 103.0));
        params.renko_wicks = true;
        let (shown, _) = elements(ChartType::Renko, &params, &data, 0).unwrap();
        assert_eq!(
            (shown[3].open, shown[3].close, shown[3].high, shown[3].low),
            (103.0, 104.0, 104.0, 101.5)
        );
        // A correction to the latest source bar extends the same unclosed wick history.
        let mut tick = data.clone();
        tick[3].close = 105.0;
        tick[3].open = 105.0;
        tick[3].high = 105.0;
        tick[3].low = 105.0;
        let (updated, _) = elements(ChartType::Renko, &params, &tick, 0).unwrap();
        assert_eq!((updated[3].high, updated[3].low), (104.0, 101.5));
        assert_eq!((updated[4].high, updated[4].low), (105.0, 104.0));
        // A partial move beyond the last completed brick belongs to the next wick.
        for (closes, high, low) in [
            ([100.0, 103.8, 101.0], 103.8, 101.0),
            ([100.0, 96.2, 99.0], 99.0, 96.2),
        ] {
            let data = bars(20, &closes, None);
            let (built, _) = elements(ChartType::Renko, &params, &data, 0).unwrap();
            let last = built.last().unwrap();
            assert_eq!((last.high, last.low), (high, low));
        }
    }

    #[test]
    fn cached_price_moves_obey_the_fresh_build_work_bound() {
        // The gap exceeds two million sizes; reseeding leaves only the final three-size move.
        for (kind, source) in [
            (ChartType::Renko, PriceBasedSource::Close),
            (ChartType::Renko, PriceBasedSource::HighLow),
            (ChartType::Range, PriceBasedSource::Close),
        ] {
            for append in [false, true] {
                for whitespace in [false, true] {
                    let params = ChartTypeParams {
                        renko_source: source,
                        range_size: 1.0,
                        ..Default::default()
                    };
                    let build = |data: &[Bar]| match kind {
                        ChartType::Renko => renko(data, 0, &params).series(),
                        _ => range_bars(data, 0, &params).series(),
                    };
                    let mut data = bars(0, &[100.0, 101.0], None);
                    if whitespace {
                        data.push(Bar::whitespace(
                            data.last().unwrap().time + Duration::minutes(1),
                        ));
                    }
                    build(&data);
                    if append {
                        data.extend(bars(data.len() as i64, &[2_000_102.0], None));
                    } else {
                        let last = data.len() - 1;
                        data[last] = bars(last as i64, &[2_000_102.0], None).remove(0);
                    }
                    if source == PriceBasedSource::HighLow || kind == ChartType::Range {
                        // A large wick must be bounded even when the close is unchanged.
                        let last = data.last_mut().unwrap();
                        last.open = 101.0;
                        last.low = 101.0;
                        last.close = 101.0;
                    }
                    let cached = build(&data);
                    assert_eq!(
                        cached.0.len(),
                        if kind == ChartType::Renko { 0 } else { 1 },
                        "{kind:?} {source:?} {append} {whitespace}"
                    );
                    assert!(cached.1);
                    // Settle at the new price after the wide bar has only seeded the state.
                    data.extend(bars(data.len() as i64, &[2_000_102.0], None));
                    build(&data);
                    data.extend(bars(data.len() as i64, &[2_000_105.0], None));
                    let resumed = build(&data);
                    assert_eq!(
                        resumed.0.len(),
                        if kind == ChartType::Renko { 3 } else { 4 }
                    );
                    assert_eq!(resumed.0.last().unwrap().close, 2_000_105.0);
                }
            }
        }
    }

    /// The bar that the first held trade joins is left out of the approximation, so its moves
    /// after that trade are drawn once, from the trades. A bar of a trade-based interval opens at
    /// its first trade, so the held trades can start inside a bar that opens before them, as after
    /// the oldest held trades are dropped. On a daily chart with a session the first trade joins
    /// the bar of its trading date, which an evening trade opens before the date's calendar day.
    #[test]
    fn range_bars_leave_the_bar_of_the_first_trade_to_the_trades() {
        use crate::model::{BarClock, Tick};
        // (clock, history bars as (time, open, high, low, close), trades as (time, price),
        // range bar closes)
        type Row = (BarClock, Vec<Bar>, Vec<Tick>, Vec<f64>);
        let bars = |rows: &[(chrono::DateTime<Utc>, f64, f64, f64, f64)]| {
            Vec::from_iter((rows.iter()).map(|&(at, o, h, l, c)| Bar::new(at, o, h, l, c, 3.0)))
        };
        let ticks = |rows: &[(chrono::DateTime<Utc>, f64)]| {
            Vec::from_iter((rows.iter()).map(|&(at, price)| Tick::new(at, price, 1.0, None)))
        };
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let at = |second| start + Duration::seconds(second);
        #[cfg_attr(not(feature = "timezones"), allow(unused_mut))]
        let mut rows: Vec<Row> = vec![(
            BarClock::new("3t".parse().unwrap()),
            // Bars of three trades: 99, 99, 100; 100, 100, 101; 101, 101, 104; then 104, 105.
            bars(&[
                (at(0), 99.0, 100.0, 99.0, 100.0),
                (at(3), 100.0, 101.0, 100.0, 101.0),
                (at(6), 101.0, 104.0, 101.0, 104.0),
                (at(9), 104.0, 105.0, 104.0, 105.0),
            ]),
            // The trades from the second one of the third bar on.
            ticks(&[
                (at(7), 101.0),
                (at(8), 104.0),
                (at(9), 104.0),
                (at(10), 105.0),
            ]),
            // The first two bars close range bars at 100 and 101, and the trades at 102 to 105.
            vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0],
        )];
        // Chicago futures trade 17:00 to 16:00 local, CST being UTC-6, their daily bars opening
        // at 06:00 UTC: 17:30 on Monday 8 January trades Tuesday, so Monday's rise to 103 closes
        // range bars at 102 and 103 before the trades take over.
        #[cfg(feature = "timezones")]
        {
            use crate::model::{Session, SessionWindow, Timeframe, TimezoneId};
            use chrono::{NaiveTime, Weekday::*};
            let chicago = TimezoneId::named("America/Chicago").unwrap();
            let hm = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
            let day = |date: u32, hour, minute| {
                Utc.with_ymd_and_hms(2024, 1, date, hour, minute, 0)
                    .unwrap()
            };
            rows.push((
                BarClock::new(Timeframe::Day1)
                    .with_timezone(chicago.clone())
                    .with_session(
                        chicago,
                        Session::new([SessionWindow::new(
                            [Mon, Tue, Wed, Thu, Fri],
                            hm(17),
                            hm(16),
                        )]),
                        false,
                    ),
                bars(&[
                    (day(4, 6, 0), 99.0, 100.0, 99.0, 100.0),
                    (day(5, 6, 0), 100.0, 101.0, 100.0, 101.0),
                    (day(8, 6, 0), 101.0, 103.0, 101.0, 103.0),
                    (day(9, 6, 0), 103.0, 105.0, 103.0, 105.0),
                ]),
                ticks(&[
                    (day(8, 23, 30), 103.0),
                    (day(9, 15, 0), 104.0),
                    (day(9, 16, 0), 105.0),
                ]),
                vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0],
            ));
        }
        let params = ChartTypeParams {
            range_size: 1.0,
            ..Default::default()
        };
        for (clock, history, trades, expected) in rows {
            let (bars, _) = ExecutionRanges::default()
                .bars(&params, &history, 0, &clock, 0, trades.iter(), None)
                .unwrap();
            let closes: Vec<_> = bars.iter().map(|bar| bar.close).collect();
            assert_eq!(closes, expected, "{:?}", clock.timeframe());
        }
    }

    /// Phantom marks follow the same history/trade join and retention as the displayed bars.
    #[test]
    fn phantom_marks_follow_approximated_bars_before_the_trades() {
        use crate::model::{BarClock, Tick, Timeframe};
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let at = |minute| start + Duration::minutes(minute);
        let flat = |minute, price| Bar::new(at(minute), price, price, price, price, 0.0);
        // (history, two trade prices, closes of the two phantom bars, retained bar count)
        type Row = (Vec<Bar>, [f64; 2], [f64; 2], usize);
        let rows: [Row; 3] = [
            (vec![], [100.0, 103.5], [102.0, 103.0], 4),
            (
                vec![flat(0, 100.0), flat(1, 103.5)],
                [104.0, 105.5],
                [102.0, 103.0],
                5,
            ),
            (
                vec![
                    flat(0, 0.0),
                    Bar::new(at(1), 0.0, 5000.0, 0.0, 5000.0, 0.0),
                    flat(2, 5003.5),
                ],
                [5004.0, 5005.5],
                [5002.0, 5003.0],
                5000,
            ),
        ];
        let params = ChartTypeParams {
            range_size: 1.0,
            ..Default::default()
        };
        for (history, prices, expected, len) in rows {
            let first = at(history.len() as i64);
            let trades = prices.map(|price| Tick::new(first, price, 1.0, None));
            let mut ranges = ExecutionRanges::default();
            let clock = BarClock::new(Timeframe::Min1);
            let (bars, _) = ranges
                .bars(&params, &history, 0, &clock, 0, trades.iter(), None)
                .unwrap();
            let marks = range_phantoms(&ranges, &history, 0, &params, 0, bars.len());
            let phantom_closes: Vec<_> = bars
                .iter()
                .zip(&marks)
                .filter(|(_, phantom)| **phantom)
                .map(|(bar, _)| bar.close)
                .collect();
            assert_eq!(
                (bars.len(), marks.len(), phantom_closes),
                (len, len, expected.to_vec())
            );
        }
    }

    /// With a range of 1, a trade at 103.5 after one at 100 closes the bars to 101, 102 and 103
    /// and leaves the bar from 103 in progress at 103.5. A trade at 104 closes the bars to 101,
    /// 102, 103 and 104 and ends on that boundary, so the last bar built is complete.
    #[test]
    fn newest_range_bar_is_in_progress_until_a_trade_closes_it() {
        use crate::model::{BarClock, Tick, Timeframe};
        let time = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let params = ChartTypeParams {
            range_size: 1.0,
            ..Default::default()
        };
        for (prices, forming) in [([100.0, 103.5], true), ([100.0, 104.0], false)] {
            let trades = prices.map(|price| Tick::new(time, price, 1.0, None));
            let mut ranges = ExecutionRanges::default();
            let clock = BarClock::new(Timeframe::Min1);
            let (bars, _) = ranges
                .bars(&params, &[], 0, &clock, 0, trades.iter(), None)
                .unwrap();
            assert_eq!(
                (bars.len(), range_forming(&ranges, &[], 0, &params)),
                (4, forming),
                "{prices:?}"
            );
        }
    }

    #[test]
    fn price_based_charts_are_built_over_the_whole_history() {
        // (name, chart type, params, calls as (bars, revision), expected OHLC ends and minutes)
        type Row = (
            &'static str,
            ChartType,
            ChartTypeParams,
            Vec<(Vec<Bar>, u64)>,
            (usize, (f64, f64), (f64, f64), (i64, i64)),
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
                "renko append",
                ChartType::Renko,
                ChartTypeParams::default(),
                vec![(bars(0, &w, None), 0), (bars(0, &w_then_105, None), 0)],
                (7, (100.0, 101.0), (104.0, 105.0), (1, 4)),
            ),
            (
                "renko prepend",
                ChartType::Renko,
                ChartTypeParams::default(),
                vec![(bars(1, &w, None), 0), (bars(0, &w_after_prepend, None), 2)],
                (9, (104.0, 103.0), (103.0, 104.0), (1, 4)),
            ),
            (
                "renko after a mid-history correction",
                ChartType::Renko,
                ChartTypeParams::default(),
                vec![(bars(0, &w, None), 0), (bars(0, &w_corrected, None), 5)],
                (4, (100.0, 101.0), (103.0, 104.0), (1, 3)),
            ),
            (
                "kagi",
                ChartType::Kagi,
                params(&|p| p.kagi_reversal = 1.0 / 64.0),
                tick(&z, None),
                (6, (100.0, 110.0), (110.0, 100.0), (2, 6)),
            ),
            (
                "point and figure",
                ChartType::PointAndFigure,
                params(&|p| {
                    p.pnf_box_size = 1.0 / 64.0;
                    p.pnf_reversal = 2;
                }),
                tick(&z, None),
                (6, (100.0, 110.0), (109.984375, 100.0), (1, 6)),
            ),
            (
                "range bars",
                ChartType::Range,
                params(&|p| p.range_size = 1.0 / 32.0),
                tick(&z, None),
                (643, (100.0, 100.03125), (100.0, 100.0), (1, 6)),
            ),
            (
                "line break",
                ChartType::LineBreak,
                params(&|p| p.line_break_count = 2),
                tick(&lb, Some(10.0)),
                (4, (10.0, 11.0), (12.0, 10.5), (0, 3)),
            ),
        ];
        let failures: Vec<String> = rows
            .iter()
            .filter_map(|(name, chart_type, params, calls, want)| {
                let mut got = None;
                for (full, revision) in calls {
                    got = elements(*chart_type, params, full, *revision);
                }
                let (bars, _) = got.expect("a nonempty build");
                let first = bars.first().unwrap();
                let last = bars.last().unwrap();
                let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
                let got = (
                    bars.len(),
                    (first.open, first.close),
                    (last.open, last.close),
                    (
                        (first.time - start).num_minutes(),
                        (last.time - start).num_minutes(),
                    ),
                );
                (got != *want).then(|| format!("{name}: {got:?}, want {want:?}"))
            })
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// The newest close adds no element and lies beyond the elements in view, further than the
    /// scale margins of a tenth of the pane reach: the fit holds the last price line and its
    /// label all the same. A type that labels its newest element instead shows that element's
    /// close: closes that rise to 139 and end at 138.6, short of a new element, leave the newest
    /// brick, line and column at 139.
    #[test]
    fn last_price_line_stays_in_view_on_price_based_charts() {
        use crate::model::{BarData, LineStyle};
        use crate::widget::test_frames::{frame, shapes};
        use crate::widget::{Chart, SeriesId};
        use egui::{Color32, Shape};
        // (chart type, parameters, first open, closes, label of the price line)
        type Row<'a> = (ChartType, ChartTypeParams, Option<f64>, &'a [f64], &'a str);
        // Lines of 10 from 100 to 340, then 241: above 240, the low of the last ten lines. Any
        // close that adds no line lies within those, so this chart is read with fewer in view.
        let stairs = Vec::from_iter((1..=24).map(|i| 100.0 + 10.0 * f64::from(i)).chain([241.0]));
        let rise = [100.0, 139.0, 138.6];
        let mut choices: Vec<Row> = Vec::new();
        for kind in [
            ChartType::Renko,
            ChartType::Kagi,
            ChartType::LineBreak,
            ChartType::PointAndFigure,
        ] {
            for real in [true, false] {
                let mut params = ChartTypeParams::default();
                *match kind {
                    ChartType::Renko => &mut params.renko_real_price,
                    ChartType::Kagi => &mut params.kagi_real_price,
                    ChartType::LineBreak => &mut params.line_break_real_price,
                    _ => &mut params.pnf_real_price,
                } = real;
                let label = if real { "138.60" } else { "139.00" };
                choices.push((kind, params, None, &rise, label));
            }
        }
        let rows: [Row; 4] = [
            // Bricks of 1 from 100 to 103, fitted up to 103.375.
            (
                ChartType::Renko,
                ChartTypeParams::default(),
                None,
                &[100.0, 101.2, 102.4, 103.1, 103.4],
                "103.40",
            ),
            // One line from 100 to 102, fitted down to 99.75; 97.5 is short of a reversal of 5.
            (
                ChartType::Kagi,
                ChartTypeParams {
                    kagi_reversal: 5.0,
                    ..Default::default()
                },
                None,
                &[100.0, 102.0, 97.5],
                "97.50",
            ),
            (
                ChartType::LineBreak,
                ChartTypeParams {
                    line_break_count: 10,
                    ..Default::default()
                },
                Some(100.0),
                &stairs,
                "241.00",
            ),
            // One column of boxes of 1 from 100 to 102, fitted up to 102.25.
            (
                ChartType::PointAndFigure,
                ChartTypeParams::default(),
                None,
                &[100.0, 102.0, 102.9],
                "102.90",
            ),
        ];
        let mut failures = Vec::new();
        for (kind, params, first_open, closes, label) in rows.into_iter().chain(choices) {
            let ctx = egui::Context::default();
            let mut chart = Chart::new(BarData::from_bars(bars(0, closes, first_open)));
            chart.set_chart_type_params(params);
            chart.set_chart_type(kind);
            let style = chart.series_style_mut(SeriesId::MAIN).unwrap();
            style.price_line_color = Some(Color32::GOLD);
            style.price_line_style = LineStyle::Solid;
            frame(&ctx, &mut chart, 0.0, vec![]);
            // About five slots across the plot, the newest element one slot from its right edge.
            chart.apply_synced_time_scale(150.0, 1.0);
            frame(&ctx, &mut chart, 0.1, vec![]);
            let painted = shapes(&ctx, &mut chart, 0.2, vec![]);
            let plot = chart.get_rendered_price_rect();
            let y = chart.get_chart_mapping().price_to_y(label.parse().unwrap());
            if !painted.iter().any(|shape| {
                matches!(shape,
                    Shape::LineSegment { points, stroke }
                        if stroke.color == Color32::GOLD
                            && (points[0].y - y).abs() < 0.1
                            && plot.y_range().contains(y)
                )
            }) {
                let view = chart.state.visible_range();
                failures.push(format!(
                    "{kind:?}: no last price line at y {y}, {view:?} in view"
                ));
            }
            let tag = painted.iter().find_map(|shape| match shape {
                Shape::Rect(rect)
                    if rect.fill == Color32::GOLD && rect.rect.center().x > plot.right() =>
                {
                    Some(rect.rect)
                }
                _ => None,
            });
            if !tag.is_some_and(|tag| {
                painted.iter().any(|shape| {
                    matches!(shape,
                        Shape::Text(text) if text.galley.text() == label && tag.contains(text.pos)
                    )
                })
            }) {
                failures.push(format!("{kind:?}: no last price label {label}"));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
