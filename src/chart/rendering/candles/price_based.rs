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
    PriceBuilder, SpanCache,
};

/// A builder and the elements it has built, newest limit included.
pub(super) struct Built<B: PriceBuilder> {
    builder: B,
    elements: Vec<B::Element>,
    bars: Arc<[Bar]>,
    /// The element limit of the type, and whether the build started after the first bar (the
    /// work bound): older elements may be missing.
    limit: usize,
    late_start: bool,
}

impl<B: PriceBuilder> Built<B> {
    fn new(builder: B, limit: usize, late_start: bool, bar: fn(&B::Element) -> Bar) -> Self {
        let elements = builder.elements();
        let bars = elements.iter().map(bar).collect();
        Self {
            elements,
            bars,
            builder,
            limit,
            late_start,
        }
    }

    /// The elements in the visible slots, or none when the build has no such range.
    pub(super) fn shown(&self, start: usize, len: usize) -> &[B::Element] {
        self.elements.get(start..start + len).unwrap_or_default()
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

thread_local! {
    static RENKO: RefCell<SpanCache<Built<RenkoBuilder>, (f64, PriceBasedSource, bool)>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
    static KAGI: RefCell<SpanCache<Built<KagiBuilder>, (f64, PriceBasedSource)>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
    static LINE_BREAK: RefCell<SpanCache<Built<LineBreakBuilder>, (usize, PriceBasedSource)>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
    static POINT_AND_FIGURE: RefCell<SpanCache<Built<PnfBuilder>, (f64, u32)>> = const { RefCell::new(SpanCache::new(CACHE_CAPACITY)) };
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
                Built::new(builder, limit, from > 0, bar)
            },
            |built, next| {
                let mut builder = built.builder.clone();
                if !next.is_whitespace() {
                    builder.push(next);
                }
                Some(Built::new(builder, limit, built.late_start, bar))
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
                line_cnt: count,
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
    let config = PointFigureConfig::new(box_size, reversal as usize);
    cached(
        &POINT_AND_FIGURE,
        (box_size, reversal),
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

/// The element containing a loaded time. Columns include quiet bars up to the next column's
/// start; other elements cover the interval up to their completion. Repeated completions
/// choose the first slot. Times in unloaded history or outside the source series have no slot.
pub(crate) fn element_index_at(
    state: &crate::model::ChartState,
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
pub(crate) fn markers(
    markers: &[crate::model::Marker],
    state: &crate::model::ChartState,
    chart_type: ChartType,
    params: &ChartTypeParams,
    older_bars_drawn: bool,
) -> Vec<crate::model::Marker> {
    markers
        .iter()
        .filter_map(|marker| {
            let index = element_index_at(state, chart_type, params, older_bars_drawn, marker.time)?;
            let mut marker = marker.clone();
            marker.time = state.shown()[index].time;
            Some(marker)
        })
        .collect()
}

/// One bar per kept element, and whether older elements were left out by the element or work limit.
/// Time-based types, invalid parameters and empty builds keep the source bars' axes.
pub(crate) fn elements(
    chart_type: ChartType,
    params: &ChartTypeParams,
    bars: &[Bar],
    revision: u64,
) -> Option<(Arc<[Bar]>, bool)> {
    if params.invalid(chart_type).is_some() {
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
}
