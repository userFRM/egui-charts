//! Host primitives attached to series or panes, and custom series drawn by them.

use super::series::{BarsInRange, MismatchDirection, index_of_key};
use super::{Chart, ChartElementId, DataOperation, PaneId, PointerTarget, SeriesId};
use crate::chart::coords::ChartMapping;
use crate::chart::primitives::{
    CustomPoint, CustomView, PanePrimitive, PanePrimitiveView, Primitive, PrimitiveHit,
    PrimitiveId, PrimitivePass, PrimitiveView,
};
use crate::data::FeedState;
use crate::model::{AutoscaleInfo, AutoscaleMargins, Bar, LogicalRange};
use crate::studies::{CustomIndicator, Indicator, IndicatorValue};
use chrono::{DateTime, Utc};
use egui::{Painter, Pos2};
use std::any::Any;
use std::ops::Range;
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard};

/// A primitive and its series or pane owner.
#[derive(Clone)]
pub(super) struct Attached {
    pub(super) id: PrimitiveId,
    pub(super) series: SeriesId,
    /// A pane owner takes precedence over `series`, which is unused for pane drawing.
    pub(super) pane: Option<PaneId>,
    /// Shared with image exports, which draw the same primitives.
    primitive: Arc<Mutex<dyn Primitive>>,
    /// Draws the study `series` in place of its plots.
    pub(super) replaces: bool,
    /// The series' bars last reported; `None` until `attached` ran.
    pub(super) seen: Option<Seen>,
    /// The points of a custom data series, which live and go with its renderer.
    pub(super) custom: Option<CustomData>,
}

/// What the chart reads from a custom point, without knowing its type.
#[derive(Clone, Copy)]
pub(super) struct CustomKey {
    pub(super) time: DateTime<Utc>,
    /// Lowest, highest and current price; `None` for whitespace.
    prices: Option<(f64, f64, f64)>,
    /// Renewed each time the host supplies the point, whose type has no equality of its own.
    revision: u64,
}

impl CustomKey {
    /// `None` for an invalid point: not whitespace, with no prices or a non-finite one.
    fn of<P: CustomPoint>(point: &P) -> Option<Self> {
        let time = point.time();
        let revision = crate::model::next_revision();
        if point.is_whitespace() {
            return Some(Self {
                time,
                prices: None,
                revision,
            });
        }
        let prices = point.prices();
        let current = *prices.last()?;
        prices.iter().all(|price| price.is_finite()).then(|| Self {
            time,
            prices: Some((
                prices.iter().copied().fold(f64::INFINITY, f64::min),
                prices.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                current,
            )),
            revision,
        })
    }
}

/// The host's points of a custom data series, in key order.
#[derive(Clone)]
pub(super) struct CustomData {
    /// A `Vec` of the point type, parallel to `keys`. Image exports share it.
    points: Arc<dyn Any + Send + Sync>,
    /// Shared with the calculation of the series' study, which outlives its removal until undo
    /// history lets it go, and with the clones of that study on price-based charts.
    keys: Arc<RwLock<Vec<CustomKey>>>,
    clone_point: fn(&(dyn Any + Send + Sync), usize) -> Arc<dyn Any + Send + Sync>,
    revision: u64,
    feed: FeedState,
    /// The exclusive cutoff of a replay, as of the last frame.
    cutoff: Option<DateTime<Utc>>,
}

impl CustomData {
    fn new<P: CustomPoint>(keys: Arc<RwLock<Vec<CustomKey>>>) -> Self {
        Self {
            points: Arc::new(Vec::<P>::new()),
            keys,
            clone_point: |points, at| {
                let points: &Vec<P> = points.downcast_ref().expect("the series' point type");
                Arc::new(points[at].clone())
            },
            revision: crate::model::next_revision(),
            feed: FeedState::default(),
            cutoff: None,
        }
    }

    pub(super) fn keys(&self) -> RwLockReadGuard<'_, Vec<CustomKey>> {
        self.keys
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The part of `keys` that is shown: all of them, or those a replay has reached.
    pub(super) fn shown<'k>(&self, keys: &'k [CustomKey]) -> &'k [CustomKey] {
        let end = self.cutoff.map_or(keys.len(), |cutoff| {
            keys.partition_point(|key| key.time < cutoff)
        });
        &keys[..end]
    }

    fn len(&self) -> usize {
        self.shown(&self.keys()).len()
    }

    /// Keeps the points before `cutoff`, for a copy of the chart that has no replay to hide the
    /// others.
    fn freeze(&mut self, cutoff: Option<DateTime<Utc>>) {
        self.cutoff = cutoff;
        let keys = self.shown(&self.keys()).to_vec();
        self.keys = Arc::new(RwLock::new(keys));
    }

    fn points_mut<P: CustomPoint>(&mut self) -> Option<&mut Vec<P>> {
        if Arc::get_mut(&mut self.points).is_none() {
            self.points = Arc::new(self.points.downcast_ref::<Vec<P>>()?.clone());
        }
        Arc::get_mut(&mut self.points)?.downcast_mut()
    }

    /// The points with logical positions from `left` to `right`, with edges rounded to whole
    /// indices as in [`Chart::bars_in_logical_range`].
    fn visible(&self, bars: &[Bar], left: f64, right: f64) -> Range<usize> {
        let keys = self.keys();
        let keys = self.shown(&keys);
        let index = |key: &CustomKey| crate::model::index_at(bars, key.time).unwrap_or(f64::NAN);
        let start = keys.partition_point(|key| index(key) < left.round());
        let end = keys
            .partition_point(|key| index(key) <= right.round())
            .max(start);
        start..end
    }

    /// The prices the points in view add to the fit of their scale.
    fn autoscale(&self, bars: &[Bar], left: f64, right: f64) -> Option<AutoscaleInfo> {
        let visible = self.visible(bars, left, right);
        self.keys()[visible]
            .iter()
            .filter_map(|key| key.prices)
            .map(|(low, high, _)| (low, high))
            .reduce(|(low, high), (lower, higher)| (low.min(lower), high.max(higher)))
            .map(Into::into)
    }

    fn at(&self, time: DateTime<Utc>) -> Option<usize> {
        index_of_key(
            self.shown(&self.keys()),
            |key| key.time,
            time,
            MismatchDirection::Exact,
        )
    }
}

/// The current value of the points at the times of `bars`, a value per bar as a study holds them.
/// A point between bars has no row; its renderer still places it by time.
fn custom_values(keys: &[CustomKey], bars: &[Bar]) -> Vec<IndicatorValue> {
    bars.iter()
        .map(|bar| {
            keys.binary_search_by_key(&bar.time, |key| key.time)
                .ok()
                .and_then(|at| keys[at].prices)
                .map_or(IndicatorValue::None, |(_, _, current)| {
                    IndicatorValue::Single(current)
                })
        })
        .collect()
}

/// Reuses pass ordering and hit ranking without exposing series or scale hooks to pane code.
struct PaneDrawing<P>(PaneId, P);

impl<P: PanePrimitive> Primitive for PaneDrawing<P> {
    fn draw_background(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
        self.1
            .draw_background(painter, &PanePrimitiveView::new(self.0, view), pass);
    }

    fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
        self.1
            .draw(painter, &PanePrimitiveView::new(self.0, view), pass);
    }

    fn hit_test_result(&self, pos: Pos2, view: &PrimitiveView<'_>) -> Option<PrimitiveHit> {
        self.1.hit_test(pos, &PanePrimitiveView::new(self.0, view))
    }

    fn attached(&mut self, ctx: &egui::Context, _: SeriesId) {
        self.1.attached(ctx, self.0);
    }

    fn detached(&mut self) {
        self.1.detached();
    }

    fn data_changed(&mut self, _: &[Bar]) {
        self.1.update();
    }
}

/// What identifies the bars of a series: a revision that changes with any bar but the last, the
/// number of bars and the last bar, bitwise.
pub(super) type Seen = (
    u64,
    usize,
    Option<(chrono::DateTime<chrono::Utc>, [u64; 5])>,
);

fn seen(revision: u64, bars: &[Bar]) -> Seen {
    let last = bars.last().map(|bar| {
        (
            bar.time,
            [bar.open, bar.high, bar.low, bar.close, bar.volume].map(f64::to_bits),
        )
    });
    (revision, bars.len(), last)
}

impl Attached {
    pub(super) fn draw_background(
        &self,
        painter: &Painter,
        view: &PrimitiveView<'_>,
        pass: PrimitivePass,
        factor: usize,
    ) {
        self.get()
            .draw_background_merged(painter, view, pass, factor);
    }

    pub(super) fn draw(
        &self,
        painter: &Painter,
        view: &PrimitiveView<'_>,
        pass: PrimitivePass,
        factor: usize,
    ) {
        self.get().draw_merged(painter, view, pass, factor);
    }

    pub(super) fn get(&self) -> MutexGuard<'_, dyn Primitive + 'static> {
        // A primitive that panicked in a hook keeps the state it had.
        self.primitive
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The points of a custom data series, with those in view on `mapping`.
    pub(super) fn custom_view<'a>(
        &'a self,
        bars: &[Bar],
        mapping: &ChartMapping,
    ) -> Option<CustomView<'a>> {
        let custom = self.custom.as_ref()?;
        Some(CustomView {
            points: custom.points.as_ref(),
            len: custom.len(),
            visible: custom.visible(
                bars,
                mapping.x_to_idx_f64(mapping.rect.left()),
                mapping.x_to_idx_f64(mapping.rect.right()),
            ),
        })
    }
}

/// A primitive under the pointer and the cursor it asks for.
#[derive(Clone)]
pub(super) struct Hit {
    /// The custom series a renderer draws, or the primitive itself.
    pub(super) target: PointerTarget,
    pub(super) result: PrimitiveHit,
}

impl Chart {
    /// Attaches `primitive` to `series`, the main series, an added bar series or a study, to
    /// draw in the pane of the series on its price scale from the next [`show`](Self::show); see
    /// [`Primitive`]. Several primitives draw in the order they were attached.
    ///
    /// Returns `None`, dropping `primitive`, when `series` is not present.
    pub fn attach_primitive(
        &mut self,
        series: SeriesId,
        primitive: impl Primitive + 'static,
    ) -> Option<PrimitiveId> {
        self.attach(series, Arc::new(Mutex::new(primitive)), false, None)
    }

    /// Attaches host drawing to an existing pane without selecting a series or price scale.
    /// It survives member changes while that pane exists, including an empty retained pane.
    /// Passes and backgrounds share attachment order with series primitives. See [`PanePrimitive`].
    ///
    /// Returns `None`, dropping `primitive` without calling hooks, for an unknown pane.
    pub fn attach_pane_primitive(
        &mut self,
        pane: PaneId,
        primitive: impl PanePrimitive + 'static,
    ) -> Option<PrimitiveId> {
        self.pane_index(pane)?;
        let id = PrimitiveId(self.next_primitive);
        self.next_primitive += 1;
        self.primitives.push(Attached {
            id,
            series: SeriesId::MAIN,
            pane: Some(pane),
            primitive: Arc::new(Mutex::new(PaneDrawing(pane, primitive))),
            replaces: false,
            seen: None,
            custom: None,
        });
        Some(id)
    }

    /// Detaches a primitive, calling its [`detached`](Primitive::detached) hook if it was
    /// attached in a frame. Returns `false` for an unknown or already detached id.
    pub fn detach_primitive(&mut self, id: PrimitiveId) -> bool {
        let Some(at) = self.primitives.iter().position(|item| item.id == id) else {
            return false;
        };
        let item = self.primitives.remove(at);
        if item.seen.is_some() {
            item.get().detached();
        }
        true
    }

    /// Adds a series type the host defines, and returns its id. `study` supplies its values,
    /// which the legend, the crosshair, the last-value tag and the autoscale of its pane read as
    /// for any study, in its own pane unless it is an overlay. `renderer` draws its points in
    /// place of the study's plots and hit tests them: a hit selects the series and is reported
    /// as [`ChartElementId::Indicator`]. The series is shown, hidden and removed like a study.
    pub fn add_custom_series(
        &mut self,
        study: Box<dyn Indicator>,
        renderer: impl Primitive + 'static,
    ) -> SeriesId {
        let id = self.add_indicator(study);
        self.attach(id, Arc::new(Mutex::new(renderer)), true, None);
        id
    }

    /// Adds a series type the host defines, fed with its own [`CustomPoint`]s instead of values
    /// computed from the bars. `P` is the point type of the series; `name` is its legend name.
    /// `configure` sets up the study behind it, such as
    /// [`with_overlay(false)`](CustomIndicator::with_overlay) for a pane of its own, or a colour.
    ///
    /// `renderer` draws the points through [`PrimitiveView::custom_points`] and hit tests them,
    /// as for [`add_custom_series`](Self::add_custom_series). The legend and crosshair read the
    /// [current price](CustomPoint::prices) of a point at the time of a main bar, and the
    /// last-value tag that of a point at the last main bar; a renderer can tag another point with
    /// [`price_axis_labels`](Primitive::price_axis_labels). All prices of the points in view fit
    /// the scale of the series. Points outside the bars are placed by extrapolation on the shared
    /// time scale, which only the main bars define, so a chart without bars draws no points. The
    /// series starts empty, and is shown, hidden and removed like a study. Undoing its removal
    /// restores the study with its last values, without its renderer or points.
    ///
    /// ```rust
    /// use chrono::{DateTime, Utc};
    /// use egui::{Painter, Pos2, Stroke};
    /// use egui_charts::model::BarData;
    /// use egui_charts::widget::{Chart, CustomPoint, Primitive, PrimitivePass, PrimitiveView};
    ///
    /// #[derive(Clone)]
    /// struct Range {
    ///     time: DateTime<Utc>,
    ///     low: f64,
    ///     high: f64,
    /// }
    ///
    /// impl CustomPoint for Range {
    ///     fn time(&self) -> DateTime<Utc> {
    ///         self.time
    ///     }
    ///     fn prices(&self) -> Vec<f64> {
    ///         vec![self.low, self.high]
    ///     }
    /// }
    ///
    /// struct Bars;
    /// impl Primitive for Bars {
    ///     fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
    ///         if pass == PrimitivePass::Series {
    ///             let points = view.custom_points::<Range>();
    ///             for point in &points[view.visible_custom_points()] {
    ///                 let Some(x) = view.time_to_x(point.time) else { continue };
    ///                 painter.line_segment(
    ///                     [point.low, point.high]
    ///                         .map(|price| Pos2::new(x, view.mapping.price_to_y(price))),
    ///                     Stroke::new(2.0, egui::Color32::GRAY),
    ///                 );
    ///             }
    ///         }
    ///     }
    /// }
    ///
    /// let mut chart = Chart::new(BarData::new());
    /// let id = chart.add_custom_data_series::<Range>("Range", |study| study.with_overlay(false), Bars);
    /// let time = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    /// assert!(chart.set_custom_data(id, vec![Range { time, low: 1.0, high: 2.0 }]));
    /// assert_eq!(chart.custom_points::<Range>(id)[0].high, 2.0);
    /// ```
    pub fn add_custom_data_series<P: CustomPoint>(
        &mut self,
        name: impl Into<String>,
        configure: impl FnOnce(CustomIndicator) -> CustomIndicator,
        renderer: impl Primitive + 'static,
    ) -> SeriesId {
        let keys = Arc::new(RwLock::new(Vec::new()));
        let source = Arc::clone(&keys);
        let study = configure(CustomIndicator::new(name, move |bars| {
            let keys = source.read().unwrap_or_else(|poison| poison.into_inner());
            custom_values(&keys, bars)
        }));
        let id = self.add_indicator(Box::new(study));
        self.attach(
            id,
            Arc::new(Mutex::new(renderer)),
            true,
            Some(CustomData::new::<P>(keys)),
        );
        id
    }

    /// Replaces the points of a custom data series. Points are sorted by key, and of several
    /// with one key, or in one slot of the timeframe for a chart built with a feed, the last is
    /// kept. A [whitespace](CustomPoint::is_whitespace) point keeps its key without values. A
    /// point that is not whitespace, without prices or with a non-finite one, is invalid: it is
    /// dropped before duplicates are resolved and counted in [`feed_state`](Self::feed_state).
    /// The main bars are not touched, and study values follow on the next [`show`](Self::show).
    /// Reported by [`ChartEvent::DataLoaded`](super::ChartEvent::DataLoaded) and, when enabled,
    /// [`DataOperation`].
    ///
    /// Returns `false`, changing nothing, for an id that is not a custom data series or a point
    /// type other than the one the series was added with.
    pub fn set_custom_data<P: CustomPoint>(&mut self, series: SeriesId, points: Vec<P>) -> bool {
        let clock = self.clock();
        let Some(custom) = self.custom_data_mut::<P>(series, "set_custom_data") else {
            return false;
        };
        let total = points.len();
        let mut rows: Vec<_> = points
            .into_iter()
            .filter_map(|point| Some((CustomKey::of(&point)?, point)))
            .collect();
        custom.feed.rejected_bars += (total - rows.len()) as u64;
        // Keys in order keep a slot together with its latest point last, and a stable sort leaves
        // points with one key in the order they arrived, so the last one wins.
        rows.sort_by_key(|(key, _)| key.time);
        let mut kept: Vec<(CustomKey, P)> = Vec::with_capacity(rows.len());
        for row in rows {
            match kept.last_mut() {
                Some(last) if clock.bar_open(last.0.time) == clock.bar_open(row.0.time) => {
                    *last = row;
                }
                _ => kept.push(row),
            }
        }
        let (keys, points): (Vec<_>, Vec<_>) = kept.into_iter().unzip();
        custom.points = Arc::new(points);
        *custom
            .keys
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = keys;
        custom.revision = crate::model::next_revision();
        self.custom_data_changed(series, 0);
        self.data_set(series);
        true
    }

    /// Merges `point` into a custom data series by the rules of
    /// [`set_custom_data`](Self::set_custom_data): a point at a stored key or slot replaces it,
    /// including with whitespace, a newer one is appended and an older one is inserted in order.
    /// An invalid point is dropped and counted, leaving the stored point at its key in place.
    /// Reported by [`ChartEvent::DataUpdated`](super::ChartEvent::DataUpdated) and, when
    /// enabled, [`DataOperation`].
    ///
    /// Returns `false`, changing nothing, for an id that is not a custom data series or a point
    /// type other than the one the series was added with.
    pub fn update_custom_data<P: CustomPoint>(&mut self, series: SeriesId, point: P) -> bool {
        let clock = self.clock();
        let Some(custom) = self.custom_data_mut::<P>(series, "update_custom_data") else {
            return false;
        };
        let Some(key) = CustomKey::of(&point) else {
            custom.feed.rejected_bars += 1;
            return true;
        };
        let slot = clock.bar_open(key.time);
        let (found, len) = {
            let keys = custom.keys();
            let found = keys.binary_search_by_key(&slot, |key| clock.bar_open(key.time));
            (found, keys.len())
        };
        let Some(points) = custom.points_mut::<P>() else {
            return false;
        };
        match found {
            Ok(at) => points[at] = point,
            Err(at) => points.insert(at, point),
        }
        let mut keys = custom
            .keys
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let operation = match found {
            Ok(at) => {
                keys[at] = key;
                DataOperation::Update(key.time)
            }
            Err(at) => {
                keys.insert(at, key);
                if at == len {
                    DataOperation::Append(key.time)
                } else {
                    DataOperation::Insert(key.time)
                }
            }
        };
        drop(keys);
        custom.revision = crate::model::next_revision();
        // A change at the first bar would blank the study until its turn to recompute.
        let last = self.state.shown().len().saturating_sub(1);
        self.custom_data_changed(series, last.max(1));
        self.data_updated(series);
        self.data_operation(series, operation);
        true
    }

    /// The points of a custom data series in key order, including whitespace; during a replay,
    /// those the replay reached by the last frame. Empty for a removed or other series, or
    /// another point type. `custom_points(id).last()` is the last point.
    pub fn custom_points<P: CustomPoint>(&self, series: SeriesId) -> &[P] {
        self.custom_of(series)
            .and_then(|custom| Some(&custom.points.downcast_ref::<Vec<P>>()?[..custom.len()]))
            .unwrap_or_default()
    }

    /// The point of a custom data series at key `time`, or else its nearest neighbour in
    /// `direction`, including whitespace. `None` when there is no such point.
    pub fn custom_point_at<P: CustomPoint>(
        &self,
        series: SeriesId,
        time: DateTime<Utc>,
        direction: MismatchDirection,
    ) -> Option<&P> {
        let points = self.custom_points::<P>(series);
        points.get(index_of_key(points, P::time, time, direction)?)
    }

    /// The point of a custom data series at a logical position on the shared horizontal scale,
    /// or its nearest neighbour in `direction`, as [`bar_at_index`](Self::bar_at_index) selects
    /// bars. `None` for a non-finite index, an empty time scale or no point in that direction.
    pub fn custom_point_at_index<P: CustomPoint>(
        &self,
        series: SeriesId,
        index: f64,
        direction: MismatchDirection,
    ) -> Option<&P> {
        let points = self.custom_points::<P>(series);
        points.get(self.key_at_index(points, P::time, index, direction)?)
    }

    /// The points of a custom data series inside `range`, with the distances to the ends of the
    /// series, as [`bars_in_logical_range`](Self::bars_in_logical_range) selects bars. The
    /// result indexes [`custom_points`](Self::custom_points). `None` for a series without
    /// points or a chart without a time scale.
    pub fn custom_points_in_logical_range(
        &self,
        series: SeriesId,
        range: LogicalRange,
    ) -> Option<BarsInRange> {
        let custom = self.custom_of(series)?;
        self.keys_in_logical_range(custom.shown(&custom.keys()), |key| key.time, range)
    }

    /// The custom data of a present series; hooks of a removed one still wait for the next frame.
    fn custom_of(&self, series: SeriesId) -> Option<&CustomData> {
        if !self.study_ids.contains(&series) {
            return None;
        }
        self.primitives_of(series)
            .find_map(|item| item.custom.as_ref())
    }

    fn custom_data_mut<P: CustomPoint>(
        &mut self,
        series: SeriesId,
        call: &str,
    ) -> Option<&mut CustomData> {
        let present = self.study_ids.contains(&series);
        let custom = self
            .primitives
            .iter_mut()
            .filter(|item| present && item.series == series)
            .find_map(|item| item.custom.as_mut())
            .filter(|custom| custom.points.is::<Vec<P>>());
        if custom.is_none() {
            log::warn!("Chart::{call} ignored: {series:?} holds no such custom points");
        }
        custom
    }

    fn custom_data_changed(&mut self, series: SeriesId, from: usize) {
        if let Some(at) = self.study_ids.iter().position(|id| *id == series) {
            self.pending_from[at] =
                Some(self.pending_from[at].map_or(from, |pending| pending.min(from)));
            // Its copy on the elements reads the same points and keeps its values until it
            // recomputes; a rebuild would blank every other study's copy.
            if self.chart_type.is_time_independent()
                && let Some(stale) = self.element_studies.stale.get_mut(at)
            {
                *stale = true;
            } else {
                self.element_studies.dirty = true;
            }
            self.readout_revision = crate::model::next_revision();
        }
    }

    /// The bars or points a series holds, as data events report them.
    pub(super) fn series_len(&self, series: SeriesId) -> usize {
        self.custom_of(series)
            .map_or_else(|| self.bars(series).len(), CustomData::len)
    }

    /// Whether `series` is a custom data series, whose prices reach its scale through its points.
    pub(super) fn is_custom_data(&self, series: SeriesId) -> bool {
        self.custom_of(series).is_some()
    }

    pub(super) fn custom_feed_state(&self, series: SeriesId) -> Option<&FeedState> {
        self.custom_of(series).map(|custom| &custom.feed)
    }

    /// The custom data series, for the pointer's nearest-point search.
    pub(super) fn custom_series(&self) -> impl Iterator<Item = &CustomData> {
        self.primitives
            .iter()
            .filter_map(|item| item.custom.as_ref())
    }

    /// For each custom data series with a point at `time`, whether the series is visible and
    /// the point has a value.
    pub(super) fn custom_values_at(
        &self,
        time: DateTime<Utc>,
    ) -> impl Iterator<Item = (bool, bool)> + '_ {
        self.primitives.iter().filter_map(move |item| {
            let custom = item.custom.as_ref()?;
            let at = custom.at(time)?;
            let valued = custom.keys()[at].prices.is_some();
            Some((self.is_series_visible(item.series), valued))
        })
    }

    /// Each custom data series' point at `time`, with its id and revision and a way to copy it.
    pub(super) fn custom_points_at(
        &self,
        time: DateTime<Utc>,
    ) -> impl Iterator<
        Item = (
            SeriesId,
            u64,
            impl FnOnce() -> Arc<dyn Any + Send + Sync> + '_,
        ),
    > + '_ {
        self.primitives.iter().filter_map(move |item| {
            let custom = item.custom.as_ref()?;
            let at = custom.at(time)?;
            Some((item.series, custom.keys()[at].revision, move || {
                (custom.clone_point)(custom.points.as_ref(), at)
            }))
        })
    }

    fn attach(
        &mut self,
        series: SeriesId,
        primitive: Arc<Mutex<dyn Primitive>>,
        replaces: bool,
        custom: Option<CustomData>,
    ) -> Option<PrimitiveId> {
        self.primitive_source(series)?;
        let id = PrimitiveId(self.next_primitive);
        self.next_primitive += 1;
        self.primitives.push(Attached {
            id,
            series,
            pane: None,
            primitive,
            replaces,
            seen: None,
            custom,
        });
        Some(id)
    }

    /// The bars reported to the primitives of `series` and what identifies them, `None` when the
    /// series is not present. Studies follow the main bars they are computed from.
    fn primitive_source(&self, series: SeriesId) -> Option<(Seen, &[Bar])> {
        let bars = self.state.data().bars.as_slice();
        let main = (seen(self.state.revision(), bars), bars);
        if series == SeriesId::MAIN {
            self.main_series_present.then_some(main)
        } else if let Some(item) = self.price_series.iter().find(|item| item.id == series) {
            Some((seen(item.revision, &item.bars), &item.bars))
        } else {
            self.study_ids.contains(&series).then_some(main)
        }
    }

    fn attachment_source(&self, item: &Attached) -> Option<(Seen, &[Bar])> {
        if let Some(pane) = item.pane {
            let bars = self.state.data().bars.as_slice();
            self.pane_index(pane)
                .is_some()
                .then(|| (seen(self.state.revision(), bars), bars))
        } else {
            let (mut current, bars) = self.primitive_source(item.series)?;
            // Both counters only grow, so their sum changes whenever either does.
            if let Some(custom) = &item.custom {
                current.0 = current.0.wrapping_add(custom.revision);
            }
            Some((current, bars))
        }
    }

    /// Runs the lifecycle hooks before a frame is drawn: `detached` for the primitives of
    /// removed series, `attached` for new ones and `data_changed` when their bars changed.
    pub(super) fn sync_primitives(&mut self, ctx: &egui::Context) {
        let cutoff = self.replay_time();
        let mut primitives = std::mem::take(&mut self.primitives);
        primitives.retain_mut(|item| {
            if let Some(custom) = &mut item.custom {
                custom.cutoff = cutoff;
            }
            let Some((current, bars)) = self.attachment_source(item) else {
                if item.seen.is_some() {
                    item.get().detached();
                }
                return false;
            };
            if item.pane.is_some() || item.seen != Some(current) {
                let mut primitive = item.get();
                if item.seen.is_none() {
                    primitive.attached(ctx, item.series);
                }
                primitive.data_changed(bars);
                drop(primitive);
                item.seen = Some(current);
            }
            true
        });
        // The hooks have no access to the chart, so no primitive was attached meanwhile.
        self.primitives = primitives;
    }

    /// Limits the custom points of a frozen copy of a chart to those shown at `cutoff`.
    pub(super) fn freeze_custom_points(&mut self, cutoff: Option<DateTime<Utc>>) {
        for item in &mut self.primitives {
            if let Some(custom) = &mut item.custom {
                custom.freeze(cutoff);
            }
        }
    }

    /// Marks primitives as current for an image export, avoiding repeated attachment and
    /// series data hooks. Pane update hooks still refresh their drawing state for the frame, so
    /// a pane primitive whose `attached` hook has not run is left out of the export.
    pub(super) fn settle_primitives(&mut self) {
        self.primitives
            .retain(|item| item.pane.is_none() || item.seen.is_some());
        for at in 0..self.primitives.len() {
            let current = self
                .attachment_source(&self.primitives[at])
                .map(|(current, _)| current);
            self.primitives[at].seen = current;
        }
    }

    pub(super) fn primitive_view(
        &self,
        series: SeriesId,
        mapping: ChartMapping,
    ) -> PrimitiveView<'_> {
        let (start, end) = self.state.visible_range();
        let custom = self
            .primitives_of(series)
            .find_map(|item| item.custom_view(self.state.shown(), &mapping));
        PrimitiveView {
            series,
            study: self
                .study_ids
                .iter()
                .position(|id| *id == series)
                .and_then(|idx| self.rendered_studies().indicators().get(idx))
                .map(|study| study.as_ref()),
            mapping,
            bars: self.state.shown(),
            visible: start..end,
            custom,
        }
    }

    /// The view of a primitive drawn in the main pane, from the main pane's mapping. `None`
    /// when its series is hidden or a study in a pane of its own.
    fn main_view(&self, series: SeriesId, coords: &ChartMapping) -> Option<PrimitiveView<'_>> {
        if self.series_pane(series) != Some(self.main_series_pane()) {
            return None;
        }
        let mapping = if series == SeriesId::MAIN {
            self.main_series_visible.then_some(coords.clone())?
        } else if let Some(item) = self.price_series.iter().find(|item| item.id == series) {
            item.visible.then(|| ChartMapping {
                price: self.series_map(item, coords.rect),
                ..coords.clone()
            })?
        } else {
            let idx = self.study_ids.iter().position(|id| *id == series)?;
            let study = self.rendered_studies().indicators().get(idx)?;
            (study.is_visible()
                && (!study.is_volume()
                    || matches!(study.scale(), crate::studies::StudyScale::Shared(_))))
            .then(|| crate::chart::rendering::study_mapping(study.as_ref(), coords))?
        };
        Some(self.primitive_view(series, mapping))
    }

    fn main_attachment_view(
        &self,
        item: &Attached,
        coords: &ChartMapping,
    ) -> Option<PrimitiveView<'_>> {
        if let Some(pane) = item.pane {
            return (pane == self.main_series_pane()
                && self.pane_index(pane).is_some()
                && coords.rect.is_positive())
            .then(|| self.primitive_view(SeriesId::MAIN, coords.clone()));
        }
        self.main_view(item.series, coords)
    }

    /// The primitives drawn in the main pane, with their views.
    pub(super) fn main_primitives<'a>(
        &'a self,
        coords: &'a ChartMapping,
    ) -> impl DoubleEndedIterator<Item = (&'a Attached, PrimitiveView<'a>)> + Clone {
        self.primitives
            .iter()
            .filter_map(move |item| Some((item, self.main_attachment_view(item, coords)?)))
    }

    /// The primitives of every visible series, for the time axis the panes share. Those in a
    /// study pane get the main pane's mapping.
    pub(super) fn time_axis_primitives<'a>(
        &'a self,
        coords: &'a ChartMapping,
    ) -> impl Iterator<Item = (&'a Attached, PrimitiveView<'a>)> {
        self.primitives.iter().filter_map(move |item| {
            if item.pane.is_some() {
                return None;
            }
            let view = self.main_view(item.series, coords).or_else(|| {
                self.is_series_visible(item.series)
                    .then(|| self.primitive_view(item.series, coords.clone()))
            })?;
            Some((item, view))
        })
    }

    /// The primitives attached to `series`.
    pub(super) fn primitives_of(
        &self,
        series: SeriesId,
    ) -> impl DoubleEndedIterator<Item = &Attached> {
        self.primitives
            .iter()
            .filter(move |item| item.pane.is_none() && item.series == series)
    }

    /// Prices and screen space the primitives of `series` add to its scale's fit.
    pub(super) fn primitive_autoscale(
        &self,
        series: SeriesId,
    ) -> impl Iterator<Item = AutoscaleInfo> + '_ {
        let (start, end) = self.state.visible_range();
        let scale = self.state.time_scale();
        let left = scale.coord_to_idx_precise(0.0, 0.0, scale.width());
        let right = scale.coord_to_idx_precise(scale.width(), 0.0, scale.width());
        self.primitives_of(series).flat_map(move |item| {
            let shown = self.state.shown();
            let points =
                (item.custom.as_ref()).and_then(|custom| custom.autoscale(shown, left, right));
            let renderer = item.get().autoscale_info(shown, start..end);
            renderer.into_iter().chain(points)
        })
    }

    /// The prices the primitives of the main series and of the overlay studies on the main
    /// scale add to its fit.
    pub(super) fn main_primitive_prices(&self) -> (Vec<f64>, AutoscaleMargins) {
        let studies = self.rendered_studies().indicators();
        let mut prices = Vec::new();
        let mut margins = AutoscaleMargins::default();
        let mut include = |info: AutoscaleInfo| {
            prices.extend([info.price_range.0, info.price_range.1]);
            margins.merge(info.margins);
        };
        if self.main_series_visible {
            self.primitive_autoscale(SeriesId::MAIN)
                .for_each(&mut include);
        }
        for (idx, id) in self.study_ids.iter().enumerate() {
            if self.series_pane(*id) == Some(self.main_series_pane())
                && studies.get(idx).is_some_and(|study| {
                    study.is_visible() && study.scale() == crate::studies::StudyScale::Main
                })
                && self.study_autoscale[idx]
                && !self.config.scale_price_chart_only
            {
                self.primitive_autoscale(*id).for_each(&mut include);
            }
        }
        (prices, margins)
    }

    /// Whether a renderer draws the study `series` in place of its plots.
    pub(super) fn has_renderer(&self, series: SeriesId) -> bool {
        self.primitives_of(series).any(|item| item.replaces)
    }

    /// Draws the main pane's primitives in `pass`, clipped to the plot.
    pub(super) fn paint_primitives(
        &self,
        painter: &Painter,
        coords: &ChartMapping,
        pass: PrimitivePass,
    ) {
        let painter = painter.with_clip_rect(painter.clip_rect().intersect(coords.rect));
        let shapes = backgrounds(
            &painter,
            self.main_pass_primitives(coords),
            pass,
            self.merge_groups.factor(),
        );
        let grouped = !shapes.is_empty();
        paint_backgrounds(&painter, shapes);
        if grouped {
            for (item, view) in self.main_pass_primitives(coords) {
                item.draw(&painter, &view, pass, self.merge_groups.factor());
            }
        } else {
            for (item, view) in self.main_primitives(coords) {
                item.draw(&painter, &view, pass, self.merge_groups.factor());
            }
        }
    }

    /// Pane decorations do not require loaded bars, unlike series rendering.
    pub(super) fn paint_empty_pane_primitives(&mut self, ui: &egui::Ui, painter: &Painter) {
        let coords = self.get_chart_mapping();
        let painter = painter.with_clip_rect(painter.clip_rect().intersect(coords.rect));
        for pass in [
            PrimitivePass::Below,
            PrimitivePass::Series,
            PrimitivePass::Above,
        ] {
            if pass != PrimitivePass::Above || !self.hide_foreground {
                paint_pass(
                    &painter,
                    self.main_primitives(&coords)
                        .filter(|(item, _)| item.pane.is_some()),
                    pass,
                    1,
                );
            }
        }
        if let Some(hit) = ui
            .input(|input| input.pointer.interact_pos())
            .filter(|pos| coords.rect.contains(*pos))
            .and_then(|pos| {
                primitive_at(
                    self.main_primitives(&coords)
                        .filter(|(item, _)| item.pane.is_some()),
                    pos,
                )
            })
        {
            ui.ctx().set_cursor_icon(hit.result.cursor);
            self.host_events.target = Some(hit.target);
            self.host_events.primitive_hit = Some(hit.result);
        }
    }

    /// Includes renderers of volume overlays, whose foregrounds normally draw with the series.
    pub(super) fn main_pass_primitives<'a>(
        &'a self,
        coords: &'a ChartMapping,
    ) -> impl DoubleEndedIterator<Item = (&'a Attached, PrimitiveView<'a>)> + Clone {
        self.primitives.iter().filter_map(move |item| {
            let view = self.main_attachment_view(item, coords).or_else(|| {
                if !item.replaces || self.series_pane(item.series) != Some(self.main_series_pane())
                {
                    return None;
                }
                let mut view = self.primitive_view(item.series, coords.clone());
                let study = view.study?;
                if !study.is_visible() {
                    return None;
                }
                view.mapping = crate::chart::rendering::study_mapping(study, coords);
                Some(view)
            })?;
            Some((item, view))
        })
    }

    /// The visible primitives of a secondary pane, in attachment order, on their own scales.
    pub(super) fn pane_primitives<'a>(
        &'a self,
        pane: PaneId,
        members: &'a [SeriesId],
        coords: &'a ChartMapping,
    ) -> impl DoubleEndedIterator<Item = (&'a Attached, PrimitiveView<'a>)> + Clone {
        self.primitives.iter().filter_map(move |item| {
            if let Some(owner) = item.pane {
                return (owner == pane)
                    .then(|| (item, self.primitive_view(SeriesId::MAIN, coords.clone())));
            }
            if !members.contains(&item.series) || !self.is_series_visible(item.series) {
                return None;
            }
            let mut view = self.primitive_view(item.series, coords.clone());
            let pane = self.series_pane(item.series)?;
            if let Some(study) = view.study {
                view.mapping = self.pane_study_mapping(pane, study, coords);
            } else if let Some(series) = self
                .price_series
                .iter()
                .find(|series| series.id == item.series)
            {
                view.mapping.price = self.series_map(series, coords.rect);
            }
            Some((item, view))
        })
    }
}

/// Defers background shapes until the series have drawn, retaining their clip rectangles.
/// An empty result keeps the existing series foreground order.
pub(super) fn backgrounds<'a>(
    painter: &Painter,
    primitives: impl Iterator<Item = (&'a Attached, PrimitiveView<'a>)>,
    pass: PrimitivePass,
    factor: usize,
) -> Vec<egui::epaint::ClippedShape> {
    let start = painter
        .ctx()
        .graphics_mut(|graphics| graphics.entry(painter.layer_id()).next_idx());
    for (item, view) in primitives {
        item.draw_background(painter, &view, pass, factor);
    }
    painter.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(painter.layer_id());
        let end = list.next_idx();
        let mut shapes = Vec::new();
        for index in start.0..end.0 {
            list.mutate_shape(egui::layers::ShapeIdx(index), |shape| {
                let empty = egui::epaint::ClippedShape::new(shape.clip_rect, egui::Shape::Noop);
                shapes.push(std::mem::replace(shape, empty));
            });
        }
        shapes
    })
}

/// Inserts deferred backgrounds without applying the painter's opacity a second time.
pub(super) fn paint_backgrounds(painter: &Painter, shapes: Vec<egui::epaint::ClippedShape>) {
    painter.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(painter.layer_id());
        for shape in shapes {
            let index = list.add(shape.clip_rect, egui::Shape::Noop);
            list.mutate_shape(index, |entry| *entry = shape);
        }
    });
}

/// Draws one complete pane pass, with every background before any foreground.
pub(super) fn paint_pass<'a>(
    painter: &Painter,
    primitives: impl Iterator<Item = (&'a Attached, PrimitiveView<'a>)> + Clone,
    pass: PrimitivePass,
    factor: usize,
) {
    for (item, view) in primitives.clone() {
        item.draw_background(painter, &view, pass, factor);
    }
    for (item, view) in primitives {
        item.draw(painter, &view, pass, factor);
    }
}

/// Chooses by pass, then distance, keeping `first` on a tie.
pub(super) fn best_hit(first: Option<Hit>, second: Option<Hit>) -> Option<Hit> {
    match (first, second) {
        (Some(first), Some(second)) => {
            if second.result.pass > first.result.pass
                || (second.result.pass == first.result.pass
                    && second.result.distance < first.result.distance)
            {
                Some(second)
            } else {
                Some(first)
            }
        }
        (first, second) => first.or(second),
    }
}

/// The best hit under `pos`, with the latest attached primitive winning ties.
pub(super) fn primitive_at<'a>(
    primitives: impl DoubleEndedIterator<Item = (&'a Attached, PrimitiveView<'a>)>,
    pos: Pos2,
) -> Option<Hit> {
    primitives.rev().fold(None, |best, (item, view)| {
        let Some(result) = item.get().hit_test_result(pos, &view) else {
            return best;
        };
        if !result.distance.is_finite() || result.distance < 0.0 {
            return best;
        }
        best_hit(
            best,
            Some(Hit {
                target: if item.replaces {
                    PointerTarget::Element(ChartElementId::Indicator(item.series))
                } else {
                    PointerTarget::Primitive(item.id)
                },
                result,
            }),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::primitives::{PriceAxisLabel, TimeAxisLabel};
    use crate::model::Bar;
    use crate::studies::{CustomIndicator, IndicatorValue};
    use crate::widget::test_frames::{chart, frame, shapes};
    use crate::widget::{ChartEvent, SeriesOptions};
    use egui::{Color32, CursorIcon, Event, Shape};

    type Log = Arc<Mutex<Vec<String>>>;

    struct Probe(&'static str, Log);

    impl Primitive for Probe {
        fn attached(&mut self, _: &egui::Context, _: SeriesId) {
            self.1.lock().unwrap().push(format!("{} attached", self.0));
        }
        fn data_changed(&mut self, bars: &[Bar]) {
            let line = format!("{} data {}", self.0, bars.len());
            self.1.lock().unwrap().push(line);
        }
        fn detached(&mut self) {
            self.1.lock().unwrap().push(format!("{} detached", self.0));
        }
    }

    /// `axis` is the main price axis of the pane: its column beside the plot and its plot's
    /// rows, without the corner beside the time axis.
    #[test]
    fn main_axis_primitives_draw_in_their_column_and_rows() {
        struct AxisRect(Arc<Mutex<Option<egui::Rect>>>);
        impl Primitive for AxisRect {
            fn draw_price_axis(&self, _: &egui::Painter, axis: egui::Rect, _: &PrimitiveView<'_>) {
                *self.0.lock().unwrap() = Some(axis);
            }
        }
        for side in [crate::model::AxisSide::Right, crate::model::AxisSide::Left] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            chart.place_pane_scales(crate::widget::PaneId::MAIN, side);
            let seen = Arc::default();
            chart.attach_primitive(SeriesId::MAIN, AxisRect(Arc::clone(&seen)));
            frame(&ctx, &mut chart, 0.0, vec![]);
            let axis = seen.lock().unwrap().unwrap();
            let plot = chart.get_rendered_price_rect();
            assert_eq!(axis.y_range(), plot.y_range(), "{side:?}");
            let touching = match side {
                crate::model::AxisSide::Right => axis.left() == plot.right(),
                crate::model::AxisSide::Left => axis.right() == plot.left(),
            };
            assert!(touching, "{side:?}: {axis:?} beside {plot:?}");
        }
    }

    #[test]
    fn hooks_follow_attachment_bar_changes_and_removal() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let bars = chart.bars(SeriesId::MAIN).to_vec();
        let line = chart.add_series(SeriesOptions::default());
        chart.set_data(line, bars[..50].to_vec());
        let log = Log::default();
        let main = chart.attach_primitive(SeriesId::MAIN, Probe("main", log.clone()));
        chart.attach_primitive(line, Probe("line", log.clone()));
        assert!(
            chart
                .attach_primitive(SeriesId(99), Probe("absent", log.clone()))
                .is_none()
        );
        let take = || std::mem::take(&mut *log.lock().unwrap());
        frame(&ctx, &mut chart, 0.0, vec![]);
        frame(&ctx, &mut chart, 0.1, vec![]);
        assert_eq!(
            take(),
            [
                "main attached",
                "main data 100",
                "line attached",
                "line data 50"
            ]
        );
        chart.update(line, bars[50].clone());
        frame(&ctx, &mut chart, 0.2, vec![]);
        assert_eq!(take(), ["line data 51"]);
        let mut corrected = bars[10].clone();
        corrected.close = 15.0;
        chart.update(line, corrected);
        frame(&ctx, &mut chart, 0.25, vec![]);
        assert_eq!(take(), ["line data 51"], "a correction before the last bar");
        let mut last = bars[99].clone();
        last.close = 15.0;
        chart.update(SeriesId::MAIN, last);
        frame(&ctx, &mut chart, 0.3, vec![]);
        assert_eq!(take(), ["main data 100"]);
        chart.remove_series(line);
        frame(&ctx, &mut chart, 0.4, vec![]);
        assert_eq!(take(), ["line detached"]);
        assert!(chart.detach_primitive(main.unwrap()));
        assert!(!chart.detach_primitive(main.unwrap()));
        assert_eq!(take(), ["main detached"]);
    }

    const BELOW: Color32 = Color32::from_rgb(1, 2, 3);
    const SERIES: Color32 = Color32::from_rgb(4, 5, 6);
    const ABOVE: Color32 = Color32::from_rgb(7, 8, 9);

    #[test]
    fn pane_attachments_follow_the_pane_through_empty_hidden_and_removed_members() {
        #[derive(Default)]
        struct Observed {
            attached: usize,
            detached: usize,
            updates: usize,
            passes: Vec<(bool, PrimitivePass)>,
            rect: Option<egui::Rect>,
        }
        struct Decoration(PaneId, Arc<Mutex<Observed>>, bool);
        impl Decoration {
            fn paint(
                &self,
                painter: &Painter,
                view: &PanePrimitiveView<'_>,
                pass: PrimitivePass,
                background: bool,
            ) {
                assert_eq!(view.pane, self.0);
                assert!(view.rect.is_positive());
                assert_eq!(painter.clip_rect(), view.rect);
                if let Some(bar) = view.bars.get(70) {
                    assert!(
                        (view.time_to_x(bar.time).unwrap() - view.index_to_x(70.0)).abs() < 0.001
                    );
                    let half_day = bar.time + chrono::Duration::hours(12);
                    let midpoint = (view.index_to_x(70.0) + view.index_to_x(71.0)) * 0.5;
                    assert!((view.time_to_x(half_day).unwrap() - midpoint).abs() < 0.001);
                }
                let mut observed = self.1.lock().unwrap();
                observed.rect = Some(view.rect);
                observed.passes.push((background, pass));
                if !background || self.2 {
                    let color = match pass {
                        PrimitivePass::Below => BELOW,
                        PrimitivePass::Series => SERIES,
                        PrimitivePass::Above => ABOVE,
                    };
                    painter.rect_filled(view.rect.expand(100.0), 0.0, color);
                }
            }
        }
        impl PanePrimitive for Decoration {
            fn attached(&mut self, ctx: &egui::Context, pane: PaneId) {
                assert_eq!(pane, self.0);
                ctx.request_repaint();
                self.1.lock().unwrap().attached += 1;
            }
            fn detached(&mut self) {
                self.1.lock().unwrap().detached += 1;
            }
            fn update(&mut self) {
                self.1.lock().unwrap().updates += 1;
            }
            fn draw_background(
                &self,
                painter: &Painter,
                view: &PanePrimitiveView<'_>,
                pass: PrimitivePass,
            ) {
                self.paint(painter, view, pass, true);
            }
            fn draw(&self, painter: &Painter, view: &PanePrimitiveView<'_>, pass: PrimitivePass) {
                self.paint(painter, view, pass, false);
            }
            fn hit_test(&self, _: Pos2, _: &PanePrimitiveView<'_>) -> Option<PrimitiveHit> {
                Some(PrimitiveHit {
                    object_id: Some("pane decoration".into()),
                    pass: PrimitivePass::Above,
                    ..PrimitiveHit::new(CursorIcon::PointingHand)
                })
            }
        }
        for original in [false, true] {
            for backgrounds in [false, true] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[10.0], &[20.0]);
                let added = chart.add_pane();
                let (pane, other) = if original {
                    chart.move_series_to_pane(SeriesId::MAIN, added);
                    (PaneId::MAIN, added)
                } else {
                    (added, PaneId::MAIN)
                };
                for id in [pane, other] {
                    if id != PaneId::MAIN {
                        assert!(chart.set_preserve_empty_pane(id, true));
                    }
                }
                let observed = Arc::new(Mutex::new(Observed::default()));
                let id = chart
                    .attach_pane_primitive(pane, Decoration(pane, observed.clone(), backgrounds))
                    .unwrap();
                let invalid = PaneId(usize::MAX);
                assert!(
                    chart
                        .attach_pane_primitive(
                            invalid,
                            Decoration(invalid, observed.clone(), backgrounds)
                        )
                        .is_none()
                );
                // Attach while empty, then add, hide and remove members, and move the main
                // series through the pane. Neither visibility nor membership owns the drawing.
                let mut member = None;
                for step in 0..9 {
                    match step {
                        1 => {
                            let series = chart.add_series(SeriesOptions::default());
                            chart.set_data(series, chart.bars(SeriesId::MAIN).to_vec());
                            chart.move_series_to_pane(series, pane);
                            member = Some(series);
                        }
                        2 => {
                            chart.set_series_visible(member.unwrap(), false);
                        }
                        3 => {
                            chart.remove_series(member.unwrap());
                        }
                        4 => {
                            chart.move_series_to_pane(SeriesId::MAIN, pane);
                        }
                        5 => {
                            chart.move_series_to_pane(SeriesId::MAIN, other);
                        }
                        6 => {
                            chart.remove_series(SeriesId::MAIN);
                        }
                        7 => {
                            chart.move_pane(pane, 0);
                        }
                        8 => {
                            chart.set_pane_height(pane, 180.0);
                        }
                        _ => {}
                    }
                    frame(&ctx, &mut chart, step as f64, vec![]);
                    let before = observed.lock().unwrap().updates;
                    observed.lock().unwrap().passes.clear();
                    let painted = shapes(&ctx, &mut chart, step as f64 + 0.1, vec![]);
                    let state = observed.lock().unwrap();
                    let panel = chart
                        .pane_layout
                        .panes
                        .iter()
                        .find(|item| item.id == pane)
                        .unwrap()
                        .rect;
                    assert!(
                        panel.contains_rect(state.rect.unwrap()),
                        "drawing stays in its owning pane"
                    );
                    assert_eq!((state.attached, state.detached), (1, 0));
                    assert!(state.updates > before, "each paint refreshes cached state");
                    let expected: Vec<_> = [
                        PrimitivePass::Below,
                        PrimitivePass::Series,
                        PrimitivePass::Above,
                    ]
                    .into_iter()
                    .flat_map(|pass| [(true, pass), (false, pass)])
                    .collect();
                    assert_eq!(
                        state.passes, expected,
                        "original={original}, backgrounds={backgrounds}, step={step}"
                    );
                    let colors: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Rect(rect) if [BELOW, SERIES, ABOVE].contains(&rect.fill) => {
                                Some(rect.fill)
                            }
                            _ => None,
                        })
                        .collect();
                    let expected_colors: Vec<_> = [BELOW, SERIES, ABOVE]
                        .into_iter()
                        .flat_map(|color| {
                            std::iter::repeat_n(color, if backgrounds { 2 } else { 1 })
                        })
                        .collect();
                    assert_eq!(colors, expected_colors);
                }
                let pos = observed.lock().unwrap().rect.unwrap().center();
                frame(&ctx, &mut chart, 9.0, vec![Event::PointerMoved(pos)]);
                assert!(chart.drain_events().any(|event| matches!(event,
                    ChartEvent::CrosshairMoved(Some(pointer))
                        if pointer.target == Some(PointerTarget::Primitive(id))
                        && pointer.primitive_hit.as_ref().unwrap().object_id.as_deref() == Some("pane decoration")
                )));
                let clicks = [true, false].map(|pressed| Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: Default::default(),
                });
                frame(&ctx, &mut chart, 10.0, clicks.to_vec());
                assert!(chart.drain_events().any(|event| matches!(event,
                    ChartEvent::Click { position, .. } if position.target == Some(PointerTarget::Primitive(id))
                )));
                chart.pane_layout.maximized = Some(other);
                observed.lock().unwrap().passes.clear();
                frame(&ctx, &mut chart, 11.0, vec![]);
                assert!(observed.lock().unwrap().passes.is_empty());
                chart.pane_layout.maximized = None;
                if pane != chart.main_series_pane() {
                    chart
                        .pane_layout
                        .panes
                        .iter_mut()
                        .find(|item| item.id == pane)
                        .unwrap()
                        .collapsed = Some(true);
                    observed.lock().unwrap().passes.clear();
                    frame(&ctx, &mut chart, 12.0, vec![]);
                    assert!(observed.lock().unwrap().passes.is_empty());
                }
                assert!(chart.remove_pane(pane));
                frame(&ctx, &mut chart, 13.0, vec![]);
                frame(&ctx, &mut chart, 14.0, vec![]);
                assert_eq!(observed.lock().unwrap().detached, 1);
                assert!(!chart.detach_primitive(id));
                assert!(
                    chart
                        .attach_pane_primitive(pane, Decoration(pane, observed.clone(), false))
                        .is_none()
                );
                // Explicit removal before the first frame runs neither lifecycle hook.
                let pending = chart
                    .attach_pane_primitive(other, Decoration(other, observed.clone(), false))
                    .unwrap();
                assert!(chart.detach_primitive(pending));
                assert_eq!(observed.lock().unwrap().attached, 1);
                let explicit = chart
                    .attach_pane_primitive(other, Decoration(other, observed.clone(), false))
                    .unwrap();
                frame(&ctx, &mut chart, 15.0, vec![]);
                assert!(chart.detach_primitive(explicit));
                assert!(!chart.detach_primitive(explicit));
                assert_eq!(observed.lock().unwrap().detached, 2);
            }
        }
        let ctx = egui::Context::default();
        let mut empty = Chart::new(Default::default());
        let observed = Arc::new(Mutex::new(Observed::default()));
        let id = empty
            .attach_pane_primitive(
                PaneId::MAIN,
                Decoration(PaneId::MAIN, observed.clone(), false),
            )
            .unwrap();
        frame(&ctx, &mut empty, 0.0, vec![]);
        observed.lock().unwrap().passes.clear();
        frame(&ctx, &mut empty, 0.1, vec![]);
        assert_eq!(
            observed.lock().unwrap().passes.len(),
            6,
            "a pane needs no history"
        );
        let pos = observed.lock().unwrap().rect.unwrap().center();
        frame(&ctx, &mut empty, 1.0, vec![Event::PointerMoved(pos)]);
        assert!(empty.drain_events().any(|event| matches!(event,
            ChartEvent::CrosshairMoved(Some(pointer))
                if pointer.time.is_none() && pointer.target == Some(PointerTarget::Primitive(id))
        )));
    }

    #[test]
    fn backgrounds_precede_foregrounds_in_every_pass_and_pane() {
        struct Ordered {
            id: u8,
            merged: bool,
            log: Log,
        }
        impl Ordered {
            fn record(
                &self,
                painter: &Painter,
                rect: egui::Rect,
                pass: PrimitivePass,
                background: bool,
            ) {
                self.log
                    .lock()
                    .unwrap()
                    .push(format!("{pass:?} {background} {}", self.id));
                painter.rect_filled(
                    egui::Rect::from_min_size(rect.min, egui::vec2(8.0, 8.0)),
                    0.0,
                    Color32::from_rgb(self.id, background as u8, pass as u8),
                );
            }
        }
        impl Primitive for Ordered {
            fn draw_background(
                &self,
                painter: &Painter,
                view: &PrimitiveView<'_>,
                pass: PrimitivePass,
            ) {
                self.record(painter, view.mapping.rect, pass, true);
            }
            fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
                self.record(painter, view.mapping.rect, pass, false);
            }
            fn draw_background_merged(
                &self,
                painter: &Painter,
                view: &PrimitiveView<'_>,
                pass: PrimitivePass,
                factor: usize,
            ) {
                if self.merged {
                    assert_eq!(factor, 4);
                    self.record(painter, view.mapping.rect, pass, true);
                } else {
                    Primitive::draw_background(self, painter, view, pass);
                }
            }
            fn draw_merged(
                &self,
                painter: &Painter,
                view: &PrimitiveView<'_>,
                pass: PrimitivePass,
                factor: usize,
            ) {
                if self.merged {
                    assert_eq!(factor, 4);
                    self.record(painter, view.mapping.rect, pass, false);
                } else {
                    Primitive::draw(self, painter, view, pass);
                }
            }
        }
        impl PanePrimitive for Ordered {
            fn draw_background(
                &self,
                painter: &Painter,
                view: &PanePrimitiveView<'_>,
                pass: PrimitivePass,
            ) {
                self.record(painter, view.rect, pass, true);
            }
            fn draw(&self, painter: &Painter, view: &PanePrimitiveView<'_>, pass: PrimitivePass) {
                self.record(painter, view.rect, pass, false);
            }
        }
        for (pane, move_main) in [(false, false), (true, false), (false, true), (true, true)] {
            for kind in ["main", "bars", "study", "custom", "custom_volume"] {
                for merged in [false, true] {
                    if (kind == "custom_volume" && !cfg!(feature = "studies"))
                        || (move_main && kind == "main")
                    {
                        continue;
                    }
                    let ctx = egui::Context::default();
                    let mut chart = chart(&[10.0], &[20.0]);
                    let other = chart.add_indicator(Box::new(
                        CustomIndicator::new("Other", |bars| {
                            vec![IndicatorValue::Single(15.0); bars.len()]
                        })
                        .with_overlay(!pane),
                    ));
                    let log = Log::default();
                    let probe = |id| Ordered {
                        id,
                        merged,
                        log: log.clone(),
                    };
                    let study = || -> Box<dyn Indicator> {
                        #[cfg(feature = "studies")]
                        if kind == "custom_volume" {
                            let mut volume = crate::studies::Volume::default();
                            volume.set_overlay(!pane);
                            return Box::new(volume);
                        }
                        Box::new(
                            CustomIndicator::new("Ordered", |bars| {
                                bars.iter()
                                    .map(|bar| IndicatorValue::Single(bar.close))
                                    .collect()
                            })
                            .with_overlay(!pane),
                        )
                    };
                    let id = match kind {
                        "main" => {
                            if pane {
                                let destination = chart.add_pane();
                                assert!(chart.move_series_to_pane(SeriesId::MAIN, destination));
                            }
                            SeriesId::MAIN
                        }
                        "bars" => {
                            let id = chart.add_series(SeriesOptions::default());
                            chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
                            if pane {
                                let destination = chart.add_pane();
                                chart.move_series_to_pane(id, destination);
                            }
                            id
                        }
                        "study" => chart.add_indicator(study()),
                        "custom" | "custom_volume" => chart.add_custom_series(study(), probe(1)),
                        _ => unreachable!(),
                    };
                    if !kind.starts_with("custom") {
                        chart.attach_primitive(id, probe(1));
                    }
                    if pane {
                        let destination = chart.series_pane(id).unwrap();
                        assert!(chart.move_series_to_pane(other, destination));
                    }
                    chart.attach_primitive(other, probe(2));
                    if move_main {
                        let destination = if pane {
                            chart.series_pane(id).unwrap()
                        } else {
                            chart.add_pane()
                        };
                        assert!(chart.move_series_to_pane(SeriesId::MAIN, destination));
                    }
                    let owner = chart.series_pane(id).unwrap();
                    let decoration = chart.attach_pane_primitive(owner, probe(3)).unwrap();
                    if merged {
                        let mut options = chart.chart_options().clone();
                        options.time_scale.min_bar_spacing = 0.0;
                        chart.set_chart_options(options);
                        chart.apply_synced_time_scale(0.25, 0.0);
                    }
                    frame(&ctx, &mut chart, 0.0, vec![]);
                    log.lock().unwrap().clear();
                    let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                    let mut expected = Vec::new();
                    let mut colors = Vec::new();
                    for pass in [
                        PrimitivePass::Below,
                        PrimitivePass::Series,
                        PrimitivePass::Above,
                    ] {
                        for background in [true, false] {
                            for id in [1, 2, 3] {
                                expected.push(format!("{pass:?} {background} {id}"));
                                colors.push(Color32::from_rgb(id, background as u8, pass as u8));
                            }
                        }
                    }
                    assert_eq!(
                        *log.lock().unwrap(),
                        expected,
                        "{kind}, pane={pane}, move_main={move_main}, merged={merged}"
                    );
                    let actual: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Rect(rect) if colors.contains(&rect.fill) => Some(rect.fill),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        actual, colors,
                        "paint order: {kind}, pane={pane}, move_main={move_main}"
                    );
                    assert!(chart.detach_primitive(decoration));
                    chart.set_series_visible(id, false);
                    chart.set_series_visible(other, false);
                    log.lock().unwrap().clear();
                    frame(&ctx, &mut chart, 0.2, vec![]);
                    assert!(log.lock().unwrap().is_empty(), "hidden {kind}");
                }
            }
        }
    }

    /// Fills a corner of the plot in each pass and asks for prices 0 to 500.
    struct Layers;

    impl Primitive for Layers {
        fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
            let fill = match pass {
                PrimitivePass::Below => BELOW,
                PrimitivePass::Series => SERIES,
                PrimitivePass::Above => ABOVE,
            };
            let corner = egui::Rect::from_min_size(view.mapping.rect.min, egui::vec2(9.0, 9.0));
            painter.rect_filled(corner, 0.0, fill);
        }
        fn autoscale(&self, _: &[Bar], _: std::ops::Range<usize>) -> Option<(f64, f64)> {
            Some((0.0, 500.0))
        }
        fn price_axis_labels(&self, _: &PrimitiveView<'_>) -> Vec<PriceAxisLabel> {
            vec![PriceAxisLabel::new(250.0, "Level", SERIES)]
        }
        fn time_axis_labels(&self, view: &PrimitiveView<'_>) -> Vec<TimeAxisLabel> {
            vec![TimeAxisLabel::new(
                view.bars[90].time,
                "A session label much wider than any price on the axis",
                ABOVE,
            )]
        }
    }

    #[test]
    fn passes_layer_around_series_and_studies_and_widen_the_scale() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let study = Color32::from_rgb(10, 11, 12);
        let study_id = chart.add_indicator(Box::new(
            CustomIndicator::new("Mid", |bars| {
                bars.iter()
                    .map(|bar| IndicatorValue::Single((bar.high + bar.low) / 2.0))
                    .collect()
            })
            .with_color(study),
        ));
        let primitive = chart.attach_primitive(SeriesId::MAIN, Layers).unwrap();
        frame(&ctx, &mut chart, 0.0, vec![]);
        let shapes = shapes(&ctx, &mut chart, 0.1, vec![]);
        let first = |found: &dyn Fn(&Shape) -> bool| shapes.iter().position(found).unwrap();
        let fill =
            |color| move |shape: &Shape| matches!(shape, Shape::Rect(rect) if rect.fill == color);
        let candle = first(&fill(chart.config.bullish_color));
        let line = first(&|shape| match shape {
            Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(study),
            Shape::LineSegment { stroke, .. } => stroke.color == study,
            _ => false,
        });
        let (below, series, above) = (
            first(&fill(BELOW)),
            first(&fill(SERIES)),
            first(&fill(ABOVE)),
        );
        assert!(below < candle && candle < line && line < series && series < above);
        let range = chart.state().price_scale().range();
        assert!(range.min <= 0.0 && range.max >= 500.0, "{range:?}");
        let text = |label: &str| {
            shapes
                .iter()
                .any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == label))
        };
        assert!(text("Level") && text("A session label much wider than any price on the axis"));
        // A time label is on the time axis and does not size the price axis.
        assert!(
            chart.measured_axis_width < 120.0,
            "{}",
            chart.measured_axis_width
        );
        chart.detach_primitive(primitive);
        for (owner, price_only, expected) in [
            (SeriesId::MAIN, true, (-62.5, 562.5)),
            (study_id, true, (8.75, 21.25)),
            (study_id, false, (-62.5, 562.5)),
        ] {
            let primitive = chart.attach_primitive(owner, Layers).unwrap();
            chart.config_mut().scale_price_chart_only = price_only;
            let painted = super::super::test_frames::shapes(&ctx, &mut chart, 0.2, vec![]);
            let range = chart.state().price_scale().range();
            assert_eq!((range.min, range.max), expected);
            assert!(
                painted.iter().any(fill(ABOVE)),
                "the primitive still paints"
            );
            chart.detach_primitive(primitive);
        }
    }

    #[test]
    fn axis_tags_keep_fixed_positions_ink_ticks_and_clips_after_scroll_and_resize() {
        struct Tags {
            tick: bool,
            offset: f32,
        }
        const INK: Color32 = Color32::from_rgb(211, 113, 17);
        const FILL: Color32 = Color32::from_rgb(27, 43, 67);
        impl Primitive for Tags {
            fn price_axis_labels(&self, _: &PrimitiveView<'_>) -> Vec<PriceAxisLabel> {
                let mut fixed = PriceAxisLabel::new(f64::NAN, "Fixed price", FILL);
                fixed.fixed_coordinate = Some(self.offset);
                fixed.text_color = Some(INK);
                fixed.tick_visible = self.tick;
                let automatic = PriceAxisLabel::new(15.0, "Auto price", Color32::WHITE);
                let mut top = fixed.clone();
                top.text = "Top price".into();
                top.tick_visible = false;
                // Fixed tags may overlap each other, and automatic tags stay underneath.
                vec![fixed, automatic, top]
            }
            fn time_axis_labels(&self, view: &PrimitiveView<'_>) -> Vec<TimeAxisLabel> {
                let mut fixed = TimeAxisLabel::new(view.bars[0].time, "Fixed time", FILL);
                fixed.fixed_coordinate = Some(self.offset);
                fixed.text_color = Some(INK);
                fixed.tick_visible = self.tick;
                let mut top = fixed.clone();
                top.text = "Top time".into();
                top.tick_visible = false;
                vec![
                    fixed,
                    TimeAxisLabel::new(view.bars[90].time, "Auto time", Color32::WHITE),
                    top,
                ]
            }
        }
        fn flatten(clip: egui::Rect, shape: Shape, out: &mut Vec<(egui::Rect, Shape)>) {
            match shape {
                Shape::Vec(shapes) => shapes
                    .into_iter()
                    .for_each(|shape| flatten(clip, shape, out)),
                shape => out.push((clip, shape)),
            }
        }
        for owner in ["main", "bars", "study"] {
            for left in [false, true] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[10.0], &[20.0]);
                chart.config.show_volume = false;
                chart.config.show_left_axis = left;
                chart.config.show_right_axis = !left;
                let id = match owner {
                    "bars" => {
                        let id = chart.add_series(SeriesOptions::default());
                        chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
                        let pane = chart.add_pane();
                        assert!(chart.move_series_to_pane(id, pane));
                        id
                    }
                    "study" => chart.add_indicator(Box::new(
                        CustomIndicator::new("Pane", |bars| {
                            vec![IndicatorValue::Single(15.0); bars.len()]
                        })
                        .with_overlay(false),
                    )),
                    _ => SeriesId::MAIN,
                };
                for (step, (tick, offset, size, scroll)) in [
                    (true, 50.0, egui::vec2(760.0, 540.0), 0.0),
                    (false, 50.0, egui::vec2(680.0, 420.0), -10.0),
                    (true, 0.0, egui::vec2(760.0, 540.0), 0.0),
                    (false, f32::NAN, egui::vec2(760.0, 540.0), 0.0),
                    (true, f32::INFINITY, egui::vec2(760.0, 540.0), 0.0),
                    (true, -1000.0, egui::vec2(760.0, 540.0), 0.0),
                ]
                .into_iter()
                .enumerate()
                {
                    chart.set_size(Some(size));
                    chart.scroll_to_position(scroll, false);
                    let primitive = chart.attach_primitive(id, Tags { tick, offset }).unwrap();
                    for at in 0..2 {
                        frame(&ctx, &mut chart, (step * 3 + at) as f64, vec![]);
                    }
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                Pos2::ZERO,
                                egui::vec2(800.0, 600.0),
                            )),
                            time: Some((step * 3 + 2) as f64),
                            ..Default::default()
                        },
                        |ui| {
                            chart.show(ui);
                        },
                    );
                    let mut painted = Vec::new();
                    for shape in std::mem::take(&mut output.shapes) {
                        flatten(shape.clip_rect, shape.shape, &mut painted);
                    }
                    output.drop_without_applying_deltas();
                    let plot = match owner {
                        "bars" => {
                            chart
                                .pane_layout
                                .panes
                                .iter()
                                .find(|pane| Some(pane.id) == chart.series_pane(id))
                                .unwrap()
                                .rect
                        }
                        "study" => chart.last_rendered_indicator_panes[0].chart_rect,
                        _ => chart.last_rendered_price_rect,
                    };
                    let axis = chart.time_axis_rect;
                    let find = |label: &str| {
                        painted
                            .iter()
                            .enumerate()
                            .find_map(|(at, (clip, shape))| match shape {
                                Shape::Text(text)
                                    if text.galley.text() == label
                                        && clip.intersects(
                                            text.galley.rect.translate(text.pos.to_vec2()),
                                        ) =>
                                {
                                    Some((at, *clip, text))
                                }
                                _ => None,
                            })
                    };
                    for (fixed, automatic, origin, price) in [
                        ("Fixed price", "Auto price", plot.top(), true),
                        ("Fixed time", "Auto time", axis.left(), false),
                    ] {
                        let visible = offset.is_finite() && offset >= 0.0;
                        assert_eq!(
                            find(fixed).is_some(),
                            visible,
                            "{owner}, left={left}, step={step}, {fixed}"
                        );
                        if let Some((at, clip, text)) = find(fixed) {
                            let centre = text.pos + text.galley.size() / 2.0;
                            let coordinate = if price { centre.y } else { centre.x };
                            assert!(
                                (coordinate - origin - offset).abs() < 0.01,
                                "{coordinate} != {origin} + {offset}"
                            );
                            let (top_at, _, top) =
                                find(if price { "Top price" } else { "Top time" }).unwrap();
                            assert!(at < top_at);
                            let top_centre = top.pos + top.galley.size() / 2.0;
                            let top_coordinate = if price { top_centre.y } else { top_centre.x };
                            assert!((top_coordinate - coordinate).abs() < 0.01);
                            assert_eq!(text.fallback_color, INK);
                            assert!(
                                text.galley
                                    .job
                                    .sections
                                    .iter()
                                    .all(|s| s.format.color == INK)
                            );
                            if let Some((auto_at, _, auto)) = find(automatic) {
                                assert!(auto_at < at);
                                assert_eq!(auto.fallback_color, Color32::BLACK);
                                let auto_y = auto.pos.y + auto.galley.size().y / 2.0;
                                assert!(painted.iter().any(|(_, shape)| matches!(shape,
                                    Shape::LineSegment { points, stroke }
                                        if stroke.color == Color32::BLACK
                                            && (points[0].x == points[1].x) != price
                                            && (points[0].y - if price { auto_y } else { axis.top() }).abs() < 0.01
                                )));
                            }
                            if price {
                                assert!(clip.top() >= plot.top() && clip.bottom() <= plot.bottom());
                            } else {
                                assert!(axis.contains_rect(clip));
                            }
                        }
                        let ticks = painted
                            .iter()
                            .filter(|(clip, shape)| match shape {
                                Shape::LineSegment { points, stroke }
                                    if stroke.color == INK
                                        && (points[0].x == points[1].x) != price =>
                                {
                                    if price {
                                        assert!(if left {
                                            clip.right() <= points[0].x
                                        } else {
                                            clip.left() >= points[0].x
                                        });
                                        assert!(
                                            clip.top() >= plot.top()
                                                && clip.bottom() <= plot.bottom()
                                        );
                                    } else {
                                        assert!(axis.contains_rect(*clip));
                                    }
                                    true
                                }
                                _ => false,
                            })
                            .count();
                        assert_eq!(
                            ticks,
                            usize::from(tick && visible),
                            "{fixed}, {owner}, step={step}"
                        );
                    }
                    chart.detach_primitive(primitive);
                }
                chart.set_series_visible(id, false);
                chart.attach_primitive(
                    id,
                    Tags {
                        tick: true,
                        offset: 50.0,
                    },
                );
                assert!(!shapes(&ctx, &mut chart, 30.0, vec![]).iter().any(|shape|
                    matches!(shape, Shape::Text(text) if text.galley.text().starts_with("Fixed "))));
            }
        }
    }

    #[test]
    fn bar_series_primitives_follow_pane_moves_and_layer_around_the_bars() {
        use crate::model::ChartType;
        use crate::widget::PaneId;

        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let color = Color32::from_rgb(10, 11, 12);
        let id = chart.add_series(SeriesOptions {
            chart_type: ChartType::Candles,
            up_color: color,
            ..Default::default()
        });
        chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
        chart.attach_primitive(id, Layers);
        let pane = chart.add_pane();
        assert!(chart.set_preserve_empty_pane(pane, true));
        for destination in [pane, PaneId::MAIN, pane] {
            assert!(chart.move_series_to_pane(id, destination));
            let painted = shapes(&ctx, &mut chart, 0.0, vec![]);
            let map = chart.series_price_map(id).unwrap();
            assert!(map.range().contains(map.transform().to_scale(500.0)));
            if destination != PaneId::MAIN {
                assert!(chart.last_price_map.range().max < 100.0);
            }
            let rects = |fill| {
                painted
                    .iter()
                    .enumerate()
                    .filter_map(move |(at, shape)| match shape {
                        Shape::Rect(rect)
                            if rect.fill == fill
                                && (fill == color || rect.rect.size() == egui::vec2(9.0, 9.0)) =>
                        {
                            Some((at, rect.rect))
                        }
                        _ => None,
                    })
            };
            let (below, rect) = rects(BELOW).next().unwrap();
            assert_eq!(rects(BELOW).count(), 1, "drawn only in its own pane");
            let candle = rects(color).next().unwrap().0;
            let series = rects(SERIES).next().unwrap().0;
            let above = rects(ABOVE).next().unwrap().0;
            assert!(
                below < candle && candle < series && series < above,
                "{destination:?}: {below} < {candle} < {series} < {above}"
            );
            let pane_rect = chart
                .pane_layout
                .panes
                .iter()
                .find(|item| item.id == destination)
                .unwrap()
                .rect;
            assert!(pane_rect.contains_rect(rect));
        }
        chart.set_series_visible(id, false);
        assert!(
            !shapes(&ctx, &mut chart, 0.2, vec![])
                .iter()
                .any(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == BELOW))
        );
    }

    /// Draws a dot at each value of its study, fills a corner of the plot above everything, and
    /// claims the whole plot for the pointer.
    struct Dots;

    impl Primitive for Dots {
        fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
            if pass == PrimitivePass::Above {
                let corner = egui::Rect::from_min_size(view.mapping.rect.min, egui::vec2(9.0, 9.0));
                painter.rect_filled(corner, 0.0, ABOVE);
            }
            for index in view
                .visible
                .clone()
                .filter(|_| pass == PrimitivePass::Series)
            {
                if let Some(value) = view.study.and_then(|study| study.plot_value(0, index)) {
                    let centre = Pos2::new(
                        view.index_to_x(index as f64),
                        view.mapping.price_to_y(value),
                    );
                    painter.circle_filled(centre, 2.0, SERIES);
                }
            }
        }
        fn hit_test(&self, pos: Pos2, view: &PrimitiveView<'_>) -> Option<CursorIcon> {
            view.mapping
                .rect
                .contains(pos)
                .then_some(CursorIcon::PointingHand)
        }
    }

    #[test]
    fn object_hits_rank_by_pass_distance_and_paint_order_in_every_pane() {
        struct Objects(Arc<Mutex<Vec<(egui::Vec2, PrimitiveHit)>>>, bool);
        impl Primitive for Objects {
            fn draw_background(
                &self,
                painter: &Painter,
                view: &PrimitiveView<'_>,
                pass: PrimitivePass,
            ) {
                if self.1 && pass == PrimitivePass::Series {
                    painter.rect_filled(view.mapping.rect, 0.0, BELOW);
                }
            }
            fn hit_test_result(&self, pos: Pos2, view: &PrimitiveView<'_>) -> Option<PrimitiveHit> {
                self.0
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(offset, hit)| {
                        let mut hit = hit.clone();
                        hit.distance = pos.distance(view.mapping.rect.center() + *offset);
                        hit
                    })
                    .filter(|hit| hit.distance <= 20.0)
                    .max_by(|left, right| {
                        left.pass
                            .cmp(&right.pass)
                            .then_with(|| right.distance.total_cmp(&left.distance))
                    })
            }
        }
        struct Invalid(f32);
        impl Primitive for Invalid {
            fn hit_test_result(&self, _: Pos2, _: &PrimitiveView<'_>) -> Option<PrimitiveHit> {
                Some(PrimitiveHit {
                    distance: self.0,
                    pass: PrimitivePass::Above,
                    ..PrimitiveHit::new(CursorIcon::Wait)
                })
            }
        }
        let object = |id: &str, cursor, offset| {
            (
                offset,
                PrimitiveHit {
                    object_id: Some(id.into()),
                    data: Some(Arc::new(serde_json::json!({ "object": id }))),
                    ..PrimitiveHit::new(cursor)
                },
            )
        };
        for (secondary, move_main, backgrounds) in
            [(false, false), (true, false), (false, true), (true, true)]
                .into_iter()
                .flat_map(|(secondary, move_main)| {
                    [false, true].map(|backgrounds| (secondary, move_main, backgrounds))
                })
        {
            let ctx = egui::Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            chart.config.show_volume = false;
            let objects = Arc::new(Mutex::new(vec![
                object("upper", CursorIcon::ResizeVertical, egui::vec2(0.0, -8.0)),
                object("lower", CursorIcon::Grab, egui::vec2(0.0, 8.0)),
            ]));
            let series = chart.add_custom_series(
                Box::new(
                    CustomIndicator::new("Objects", |bars| {
                        vec![IndicatorValue::Single(15.0); bars.len()]
                    })
                    .with_overlay(!secondary),
                ),
                Objects(objects.clone(), backgrounds),
            );
            let rival = chart.add_custom_series(
                Box::new(CustomIndicator::new("Rival", |bars| {
                    vec![IndicatorValue::Single(15.0); bars.len()]
                })),
                Objects(
                    Arc::new(Mutex::new(vec![object(
                        "rival",
                        CursorIcon::Wait,
                        egui::vec2(0.0, -18.0),
                    )])),
                    backgrounds,
                ),
            );
            assert!(chart.move_series_to_pane(rival, chart.series_pane(series).unwrap()));
            if move_main {
                let destination = if secondary {
                    chart.series_pane(series).unwrap()
                } else {
                    chart.add_pane()
                };
                assert!(chart.move_series_to_pane(SeriesId::MAIN, destination));
                // Keep the main series behind the renderers, as in the original layout.
                for id in [series, rival] {
                    assert!(chart.reorder_indicator(id, super::super::VisualOrder::BringToFront));
                }
            }
            let first = Arc::new(Mutex::new(vec![object(
                "first",
                CursorIcon::Crosshair,
                egui::vec2(3.0, 4.0),
            )]));
            let second = Arc::new(Mutex::new(vec![object(
                "second",
                CursorIcon::PointingHand,
                egui::vec2(0.0, 6.0),
            )]));
            let first_id = chart
                .attach_primitive(series, Objects(first.clone(), backgrounds))
                .unwrap();
            let second_id = chart
                .attach_primitive(series, Objects(second.clone(), backgrounds))
                .unwrap();
            for time in [0.0, 0.1, 0.2] {
                frame(&ctx, &mut chart, time, vec![]);
            }
            let plot = if chart.series_pane(series) == Some(chart.main_series_pane()) {
                chart.last_rendered_price_rect
            } else {
                chart.last_rendered_indicator_panes[0].chart_rect
            };
            let renderer = PointerTarget::Element(ChartElementId::Indicator(series));
            let mut pos = plot.center();
            // At the centre, sqrt(3^2 + 4^2) = 5 beats 6; both renderer objects are 8 away.
            for (step, (target, id, distance, cursor)) in [
                (
                    PointerTarget::Primitive(first_id),
                    "first",
                    5.0,
                    CursorIcon::Crosshair,
                ),
                (
                    PointerTarget::Primitive(second_id),
                    "second",
                    5.0,
                    CursorIcon::PointingHand,
                ),
                (
                    PointerTarget::Primitive(first_id),
                    "first",
                    5.0,
                    CursorIcon::Crosshair,
                ),
                (renderer.clone(), "lower", 8.0, CursorIcon::Grab),
                (
                    PointerTarget::Primitive(second_id),
                    "second",
                    5.0,
                    CursorIcon::PointingHand,
                ),
                (renderer.clone(), "upper", 8.0, CursorIcon::ResizeVertical),
                (renderer, "lower", 0.0, CursorIcon::Grab),
            ]
            .into_iter()
            .enumerate()
            {
                match step {
                    1 => second.lock().unwrap()[0].0.y = 5.0,
                    2 => second.lock().unwrap()[0].1.pass = PrimitivePass::Below,
                    3 => first.lock().unwrap()[0].1.pass = PrimitivePass::Below,
                    4 => {
                        for (_, hit) in objects.lock().unwrap().iter_mut() {
                            hit.pass = PrimitivePass::Above;
                        }
                        second.lock().unwrap()[0].1.pass = PrimitivePass::Above;
                    }
                    5 => {
                        second.lock().unwrap()[0].1.pass = PrimitivePass::Below;
                        objects.lock().unwrap()[1].1.pass = PrimitivePass::Below;
                    }
                    6 => {
                        pos.y += 8.0;
                        objects.lock().unwrap()[1].1.pass = PrimitivePass::Above;
                    }
                    _ => {}
                }
                chart.drain_events().for_each(drop);
                let time = 1.0 + step as f64;
                let actual_cursor = frame(&ctx, &mut chart, time, vec![Event::PointerMoved(pos)]);
                let pointer = chart
                    .drain_events()
                    .find_map(|event| match event {
                        ChartEvent::CrosshairMoved(Some(pointer)) => Some(pointer),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(
                    pointer.target,
                    Some(target),
                    "secondary={secondary}, move_main={move_main}, backgrounds={backgrounds}, step={step}"
                );
                let hit = pointer.primitive_hit.as_ref().unwrap();
                assert_eq!(hit.object_id.as_deref(), Some(id));
                assert_eq!(
                    hit.data.as_deref(),
                    Some(&serde_json::json!({ "object": id }))
                );
                assert_eq!((hit.distance, hit.cursor), (distance, cursor));
                assert_eq!(actual_cursor, cursor);
                frame(&ctx, &mut chart, time + 0.1, vec![]);
                assert!(
                    !chart
                        .drain_events()
                        .any(|event| matches!(event, ChartEvent::CrosshairMoved(_)))
                );
                for pressed in [true, false] {
                    frame(
                        &ctx,
                        &mut chart,
                        time + 0.2,
                        vec![Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        }],
                    );
                }
                assert!(chart.drain_events().any(|event| matches!(event,
                    ChartEvent::Click { position, .. }
                        if position.target == pointer.target && position.primitive_hit == pointer.primitive_hit
                )));
            }
            // Payload changes under a stationary pointer still reach the host.
            objects.lock().unwrap()[1].1.data =
                Some(Arc::new(serde_json::json!({ "updated": true })));
            frame(&ctx, &mut chart, 9.0, vec![]);
            assert!(chart.drain_events().any(|event| matches!(event,
                ChartEvent::CrosshairMoved(Some(pointer))
                    if pointer.primitive_hit.as_ref().unwrap().data.as_deref() == Some(&serde_json::json!({ "updated": true }))
            )));
            chart.set_series_visible(rival, false);
            // Legacy hits retain their cursor and the latest attachment wins a tie.
            chart.attach_primitive(series, Dots);
            let latest = chart.attach_primitive(series, Dots).unwrap();
            objects.lock().unwrap().clear();
            for distance in [f32::NAN, f32::INFINITY, -1.0] {
                let invalid = chart.attach_primitive(series, Invalid(distance)).unwrap();
                frame(&ctx, &mut chart, 9.05, vec![]);
                assert_eq!(
                    chart.host_events.target,
                    Some(PointerTarget::Primitive(latest))
                );
                assert_eq!(
                    chart.host_events.primitive_hit,
                    Some(PrimitiveHit::new(CursorIcon::PointingHand))
                );
                chart.detach_primitive(invalid);
            }
            chart.set_series_visible(series, false);
            frame(&ctx, &mut chart, 9.1, vec![]);
            assert!(chart.host_events.primitive_hit.is_none());
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct Band {
        time: DateTime<Utc>,
        /// Low and high, the current value being the high. `None` is whitespace.
        values: Option<(f64, f64)>,
        note: &'static str,
    }

    impl CustomPoint for Band {
        fn time(&self) -> DateTime<Utc> {
            self.time
        }
        fn prices(&self) -> Vec<f64> {
            self.values
                .map_or_else(Vec::new, |(low, high)| vec![low, high])
        }
    }

    /// A point type other than [`Band`].
    #[derive(Clone, Debug, PartialEq)]
    struct OtherPoint(DateTime<Utc>);

    impl CustomPoint for OtherPoint {
        fn time(&self) -> DateTime<Utc> {
            self.0
        }
        fn prices(&self) -> Vec<f64> {
            vec![1.0]
        }
    }

    /// Records the notes of the points it is given to draw.
    struct BandNotes(Arc<Mutex<Vec<&'static str>>>);

    impl Primitive for BandNotes {
        fn draw(&self, _: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
            if pass == PrimitivePass::Series {
                let points = view.custom_points::<Band>();
                *self.0.lock().unwrap() = points[view.visible_custom_points()]
                    .iter()
                    .map(|point| point.note)
                    .collect();
            }
        }
    }

    fn band_series(chart: &mut Chart) -> (SeriesId, Arc<Mutex<Vec<&'static str>>>) {
        let drawn = Arc::<Mutex<Vec<&'static str>>>::default();
        let id = chart.add_custom_data_series::<Band>(
            "Band",
            |study| study.with_overlay(false),
            BandNotes(Arc::clone(&drawn)),
        );
        (id, drawn)
    }

    #[test]
    fn custom_points_reach_renderer_scale_cursor_and_queries_apart_from_the_bars() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let main = chart.bars(SeriesId::MAIN).to_vec();
        let (id, drawn) = band_series(&mut chart);
        let time = |bar: usize| main[bar].time;
        let band = |at, low, high, note| Band {
            time: at,
            values: Some((low, high)),
            note,
        };
        let half_past_90 = time(90) + chrono::TimeDelta::hours(12);

        assert!(chart.set_custom_data(
            id,
            vec![
                band(time(10), -1e6, 1e6, "out of view"),
                band(time(92), 1000.0, 1100.0, "c"),
                band(time(90), 1000.0, 1100.0, "replaced by the later duplicate"),
                band(time(90), 1200.0, 1300.0, "a"),
                band(half_past_90, 1400.0, 1500.0, "b"),
                band(
                    time(92),
                    f64::NAN,
                    1100.0,
                    "invalid, so it does not replace c"
                ),
                Band {
                    time: time(94),
                    values: None,
                    note: "whitespace"
                },
            ],
        ));
        let notes = |chart: &Chart| -> Vec<&str> {
            chart
                .custom_points::<Band>(id)
                .iter()
                .map(|point| point.note)
                .collect()
        };
        assert_eq!(notes(&chart), ["out of view", "a", "b", "c", "whitespace"]);
        assert_eq!(chart.feed_state(id).unwrap().rejected_bars, 1);
        // A series keeps one point type, and bars are not its data.
        assert!(!chart.set_custom_data(SeriesId::MAIN, vec![band(time(1), 1.0, 2.0, "")]));
        assert!(!chart.set_custom_data::<OtherPoint>(id, vec![OtherPoint(time(1))]));
        chart.set_visible_bars(12);

        frame(&ctx, &mut chart, 0.0, vec![]);
        frame(&ctx, &mut chart, 0.1, vec![]);
        assert_eq!(*drawn.lock().unwrap(), ["a", "b", "c", "whitespace"]);
        let range = |chart: &Chart| chart.series_price_map(id).unwrap().range();
        // The lowest price is no point's current value, and no bar comes close.
        assert!(range(&chart).min <= 1000.0 && range(&chart).max >= 1500.0);
        assert!(range(&chart).min > -1e6 && range(&chart).max < 1e6);
        let readout =
            |chart: &Chart, bar| chart.indicator_value_at(id, bar).map(|(_, value)| value);
        assert_eq!(readout(&chart, 90), Some(1300.0));
        assert_eq!(readout(&chart, 94), None);
        assert_eq!(readout(&chart, 80), None);

        // An update at a stored key, a new key, an older key and invalid input.
        chart.drain_events().for_each(drop);
        chart.set_data_operation_notifications(true);
        assert!(chart.update_custom_data(id, band(time(92), 2000.0, 2100.0, "c2")));
        assert!(chart.update_custom_data(id, band(time(96), 3000.0, 3100.0, "d")));
        assert!(chart.update_custom_data(id, band(time(91), 1000.0, 1100.0, "older")));
        assert!(chart.update_custom_data(
            id,
            Band {
                time: time(90),
                values: Some((f64::INFINITY, 1.0)),
                note: "bad"
            }
        ));
        assert_eq!(
            notes(&chart),
            ["out of view", "a", "b", "older", "c2", "whitespace", "d"]
        );
        assert_eq!(chart.feed_state(id).unwrap().rejected_bars, 2);
        let operations: Vec<_> = chart
            .drain_events()
            .filter_map(|event| match event {
                ChartEvent::DataOperation {
                    operation, bars, ..
                } => Some((operation, bars)),
                _ => None,
            })
            .collect();
        assert_eq!(
            operations,
            [
                (DataOperation::Update(time(92)), 5),
                (DataOperation::Append(time(96)), 6),
                (DataOperation::Insert(time(91)), 7),
            ]
        );
        frame(&ctx, &mut chart, 1.0, vec![]);
        frame(&ctx, &mut chart, 1.1, vec![]);
        assert_eq!(readout(&chart, 92), Some(2100.0));
        assert!(range(&chart).max >= 3100.0, "{:?}", range(&chart));

        // The cursor selects stored keys of the series, on a main bar and between two.
        let pane = chart
            .pane_layout
            .panes
            .iter()
            .find(|pane| pane.members.contains(&id))
            .unwrap()
            .rect;
        let coords = chart.get_chart_mapping();
        let mut hover = |at: f64, time_at: f64| {
            let pos = Pos2::new(coords.index_to_x(at), pane.center().y);
            frame(&ctx, &mut chart, time_at, vec![Event::PointerMoved(pos)]);
            frame(&ctx, &mut chart, time_at + 0.01, vec![]);
            chart
                .drain_events()
                .filter_map(|event| match event {
                    ChartEvent::CrosshairMoved(Some(pointer)) => Some(pointer),
                    _ => None,
                })
                .last()
                .expect("the pointer moved inside the pane")
        };
        let on_bar = hover(92.0, 2.0);
        assert_eq!(on_bar.time, Some(time(92)));
        assert_eq!(
            on_bar.custom_point::<Band>(id).map(|point| point.note),
            Some("c2")
        );
        assert_eq!(on_bar.custom_point::<OtherPoint>(id), None);
        let between = hover(90.5, 3.0);
        assert_eq!(between.time, Some(half_past_90));
        assert_eq!(
            between.custom_point::<Band>(id).map(|point| point.note),
            Some("b")
        );

        for (at, direction, note) in [
            (time(91), MismatchDirection::Exact, Some("older")),
            (time(93), MismatchDirection::Exact, None),
            (time(93), MismatchDirection::NearestLeft, Some("c2")),
            (
                time(93),
                MismatchDirection::NearestRight,
                Some("whitespace"),
            ),
        ] {
            let point = chart.custom_point_at::<Band>(id, at, direction);
            assert_eq!(point.map(|point| point.note), note, "{at} {direction:?}");
        }
        assert_eq!(
            chart
                .custom_point_at_index::<Band>(id, 91.0, MismatchDirection::Exact)
                .map(|point| point.note),
            Some("older"),
        );
        let inside = chart
            .custom_points_in_logical_range(id, LogicalRange::new(91.0, 94.0))
            .unwrap();
        assert_eq!(inside.bars, 3..6);

        // Points between bars have no study value, whose empty range must not reach the fit.
        chart.set_custom_data(id, vec![band(half_past_90, 1400.0, 1500.0, "between")]);
        frame(&ctx, &mut chart, 4.0, vec![]);
        frame(&ctx, &mut chart, 4.1, vec![]);
        assert!(range(&chart).min > 1000.0, "{:?}", range(&chart));

        // Removal takes the points with it and leaves the bars alone.
        assert!(chart.remove_series(id));
        assert!(chart.custom_points::<Band>(id).is_empty());
        assert!(!chart.update_custom_data(id, band(time(98), 1.0, 2.0, "gone")));
        frame(&ctx, &mut chart, 5.0, vec![]);
        assert_eq!(chart.bars(SeriesId::MAIN), main);
    }

    /// A replay must not show points ahead of its position, even in the whitespace past the bars.
    #[test]
    fn replay_withholds_custom_points_it_has_not_reached() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let main = chart.bars(SeriesId::MAIN).to_vec();
        let (id, drawn) = band_series(&mut chart);
        let point = |bar: usize, high, note| Band {
            time: main[bar].time,
            values: Some((high, high)),
            note,
        };
        chart.set_custom_data(
            id,
            vec![
                point(50, 1000.0, "reached"),
                Band {
                    time: main[54].time + chrono::TimeDelta::hours(12),
                    ..point(54, 1000.0, "between the last bar and the cursor")
                },
                point(55, 9000.0, "ahead"),
            ],
        );
        chart.set_replay_time(Some(main[55].time));
        chart.set_visible_bars(40);
        frame(&ctx, &mut chart, 0.0, vec![]);
        frame(&ctx, &mut chart, 0.1, vec![]);
        let reached = ["reached", "between the last bar and the cursor"];
        assert_eq!(*drawn.lock().unwrap(), reached);
        assert_eq!(chart.custom_points::<Band>(id).len(), 2);
        assert!(chart.series_price_map(id).unwrap().range().max < 9000.0);
        // An image export draws a copy of the chart that has no replay of its own.
        drawn.lock().unwrap().clear();
        let mut image = chart
            .export_image(&ctx, &crate::widget::ImageExportOptions::default())
            .unwrap();
        image.textures_delta.clear();
        assert_eq!(*drawn.lock().unwrap(), reached);
        chart.set_replay_time(None);
        frame(&ctx, &mut chart, 0.2, vec![]);
        frame(&ctx, &mut chart, 0.3, vec![]);
        assert_eq!(chart.custom_points::<Band>(id).len(), 3);
    }

    #[test]
    fn custom_series_draws_and_hit_tests_in_place_of_its_plots() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let color = Color32::from_rgb(10, 11, 12);
        let id = chart.add_custom_series(
            Box::new(
                CustomIndicator::new("Range", |bars| {
                    bars.iter()
                        .map(|bar| IndicatorValue::Single(bar.high - bar.low))
                        .collect()
                })
                .with_overlay(false)
                .with_color(color),
            ),
            Dots,
        );
        #[cfg(feature = "drawings")]
        {
            let mut drawing = crate::drawings::Drawing::with_color(
                1,
                crate::drawings::DrawingToolType::HorizontalLine,
                [255, 0, 0, 255],
            );
            drawing.chart_points = vec![crate::drawings::ChartPoint::new(50.0, 10.0)];
            drawing.pane = chart.series_pane(id).map(|pane| pane.0);
            chart.drawings_mut().drawings.push(drawing);
        }
        frame(&ctx, &mut chart, 0.0, vec![]);
        let shapes = shapes(&ctx, &mut chart, 0.1, vec![]);
        #[cfg(feature = "drawings")]
        {
            // The last pass of a study pane is above the pane's drawings.
            let red = |shape: &Shape| match shape {
                Shape::Path(path) => {
                    path.stroke.color == egui::epaint::ColorMode::Solid(Color32::RED)
                }
                Shape::LineSegment { stroke, .. } => stroke.color == Color32::RED,
                // Unchanged drawings paint their cached triangles in the same place in the list.
                Shape::Mesh(mesh) => mesh.triangles().any(|triangle| {
                    triangle
                        .into_iter()
                        .all(|index| mesh.vertices[index as usize].color == Color32::RED)
                }),
                _ => false,
            };
            let above = |shape: &Shape| matches!(shape, Shape::Rect(rect) if rect.fill == ABOVE);
            let drawing = shapes
                .iter()
                .position(red)
                .expect("the red drawing is painted");
            let last_pass = shapes
                .iter()
                .position(above)
                .expect("the last pass is painted");
            assert!(
                drawing < last_pass,
                "the last pass must paint above the drawing"
            );
        }
        assert!(
            shapes
                .iter()
                .any(|shape| matches!(shape, Shape::Circle(dot) if dot.fill == SERIES))
        );
        assert!(!shapes.iter().any(|shape| match shape {
            Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(color),
            Shape::LineSegment { stroke, .. } => stroke.color == color,
            _ => false,
        }));
        let pane = chart.last_rendered_indicator_panes[0].chart_rect;
        // Away from the values, where the study's own line hit test would miss.
        let pos = Pos2::new(pane.center().x, pane.bottom() - 2.0);
        chart.drain_events().for_each(drop);
        frame(&ctx, &mut chart, 0.2, vec![Event::PointerMoved(pos)]);
        let events: Vec<_> = chart.drain_events().collect();
        let Some(ChartEvent::CrosshairMoved(Some(pointer))) = events.last() else {
            panic!("{events:?}")
        };
        assert_eq!(
            pointer.target,
            Some(PointerTarget::Element(ChartElementId::Indicator(id)))
        );
        assert_eq!(
            pointer.primitive_hit,
            Some(PrimitiveHit::new(CursorIcon::PointingHand))
        );
        let click = |pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&ctx, &mut chart, 0.3, vec![click(true), click(false)]);
        assert_eq!(
            chart.selection.selected_id(),
            Some(ChartElementId::Indicator(id))
        );
        let pane = chart.series_pane(id).unwrap();
        chart.set_preserve_empty_pane(pane, true);
        #[cfg(feature = "drawings")]
        chart.drawings_mut().drawings.clear();
        let cover = chart.add_indicator(Box::new(CustomIndicator::new("Cover", |bars| {
            vec![IndicatorValue::Single(15.0); bars.len()]
        })));
        for (step, destination) in [super::super::PaneId::MAIN, pane].into_iter().enumerate() {
            chart.move_series_to_pane(id, destination);
            chart.move_series_to_pane(cover, destination);
            for (layer, (order, expected)) in [
                (super::super::VisualOrder::SendToBack, cover),
                (super::super::VisualOrder::BringToFront, id),
            ]
            .into_iter()
            .enumerate()
            {
                chart.drain_events().for_each(drop);
                chart.reorder_indicator(id, order);
                let time = 1.0 + step as f64 + layer as f64 * 0.2;
                frame(&ctx, &mut chart, time, vec![]);
                let map = if destination == super::super::PaneId::MAIN {
                    chart.get_chart_mapping().price
                } else {
                    chart.series_price_map(cover).unwrap()
                };
                let pos = Pos2::new(chart.get_chart_mapping().idx_to_x(70), map.price_to_y(15.0));
                frame(&ctx, &mut chart, time + 0.1, vec![Event::PointerMoved(pos)]);
                assert!(chart.drain_events().any(|event| matches!(event,
                    ChartEvent::CrosshairMoved(Some(pointer))
                        if pointer.target == Some(PointerTarget::Element(ChartElementId::Indicator(expected)))
                )), "{destination:?} {order:?}");
            }
        }
    }

    const PINNED: Color32 = Color32::from_rgb(27, 43, 67);

    fn text_rect(text: &egui::epaint::TextShape) -> egui::Rect {
        text.galley.rect.translate(text.pos.to_vec2())
    }

    /// The series a primitive goes on for the tests below: the main series, a study whose values
    /// fill its own pane, or the second of two such studies sharing a pane.
    fn pane_owner(chart: &mut Chart, owner: &str) -> SeriesId {
        chart.config.show_volume = false;
        let study = || {
            Box::new(
                CustomIndicator::new("Pane", |bars| {
                    (0..bars.len())
                        .map(|at| IndicatorValue::Single(at as f64))
                        .collect()
                })
                .with_overlay(false),
            )
        };
        match owner {
            "main" => SeriesId::MAIN,
            "study" => chart.add_indicator(study()),
            _ => {
                let first = chart.add_indicator(study());
                let second = chart.add_indicator(study());
                assert!(chart.move_indicator_to_pane(second, first));
                second
            }
        }
    }

    fn pane_plot(chart: &Chart, owner: &str) -> egui::Rect {
        match owner {
            "main" => chart.last_rendered_price_rect,
            _ => chart.last_rendered_indicator_panes[0].chart_rect,
        }
    }

    #[test]
    fn crosshair_labels_stay_above_pinned_axis_tags() {
        struct Pinned;
        impl Primitive for Pinned {
            fn price_axis_labels(&self, _: &PrimitiveView<'_>) -> Vec<PriceAxisLabel> {
                let mut label = PriceAxisLabel::new(f64::NAN, "Pinned price", PINNED);
                label.fixed_coordinate = Some(60.0);
                vec![label]
            }
            fn time_axis_labels(&self, view: &PrimitiveView<'_>) -> Vec<TimeAxisLabel> {
                let mut label = TimeAxisLabel::new(view.bars[0].time, "Pinned time", PINNED);
                label.fixed_coordinate = Some(200.0);
                vec![label]
            }
        }
        let mut observed = Vec::new();
        for owner in ["main", "study", "second member"] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            let id = pane_owner(&mut chart, owner);
            chart.attach_primitive(id, Pinned).unwrap();
            frame(&ctx, &mut chart, 0.0, vec![]);
            let plot = pane_plot(&chart, owner);
            // The pointer sits where both tags are pinned: 60 points below the top of the
            // pane's price axis and 200 points right of the left edge of the time axis.
            let pos = Pos2::new(chart.time_axis_rect.left() + 200.0, plot.top() + 60.0);
            frame(&ctx, &mut chart, 1.0, vec![Event::PointerMoved(pos)]);
            let painted = shapes(&ctx, &mut chart, 2.0, vec![Event::PointerMoved(pos)]);
            let axes: &[(&str, bool)] = if owner == "main" {
                &[("Pinned price", true), ("Pinned time", false)]
            } else {
                &[("Pinned price", true)]
            };
            for &(pinned, price) in axes {
                // The pinned tag's fill, then the other text it overlaps nearest the pointer:
                // the crosshair's label on that axis.
                let (fill_at, fill) = painted
                    .iter()
                    .enumerate()
                    .find_map(|(at, shape)| match shape {
                        Shape::Rect(rect)
                            if rect.fill == PINNED
                                && (rect.rect.center().y > plot.bottom()) != price =>
                        {
                            Some((at, rect.rect))
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{owner}: {pinned} is painted"));
                let crosshair = painted
                    .iter()
                    .enumerate()
                    .filter_map(|(at, shape)| match shape {
                        Shape::Text(text)
                            if text.galley.text() != pinned && text_rect(text).intersects(fill) =>
                        {
                            Some((at, text))
                        }
                        _ => None,
                    })
                    .min_by(|(_, a), (_, b)| {
                        let distance = |text: &egui::epaint::TextShape| {
                            let centre = text_rect(text).center();
                            if price {
                                (centre.y - pos.y).abs()
                            } else {
                                (centre.x - pos.x).abs()
                            }
                        };
                        distance(a).total_cmp(&distance(b))
                    });
                observed.push(match crosshair {
                    Some((at, text)) => format!(
                        "{owner} {pinned}: crosshair label {:?} {}",
                        text.galley.text(),
                        if at > fill_at { "above" } else { "below" }
                    ),
                    None => format!("{owner} {pinned}: no crosshair label"),
                });
            }
        }
        // The crosshair labels read the pointer and stay above every other tag on their axis,
        // as they do over the last price, drawings and automatic primitive tags.
        assert!(
            observed.iter().all(|line| line.ends_with("above")),
            "{observed:?}"
        );
    }

    #[test]
    fn primitive_time_axis_stays_below_footprint_statistics() {
        struct Tags(Arc<Mutex<Option<f32>>>);
        impl Primitive for Tags {
            fn time_axis_labels(&self, view: &PrimitiveView<'_>) -> Vec<TimeAxisLabel> {
                let mut fixed = TimeAxisLabel::new(view.bars[0].time, "Fixed", Color32::WHITE);
                fixed.fixed_coordinate = Some(100.0);
                vec![
                    TimeAxisLabel::new(view.bars[90].time, "Automatic", Color32::WHITE),
                    fixed,
                ]
            }
            fn draw_time_axis(&self, _: &Painter, axis: egui::Rect, _: &PrimitiveView<'_>) {
                *self.0.lock().unwrap() = Some(axis.top());
            }
        }
        let tops = [false, true].map(|statistics| {
            let ctx = egui::Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            chart.set_chart_type(crate::model::ChartType::VolumeFootprint);
            chart.set_chart_type_params(crate::model::ChartTypeParams {
                footprint_statistics: statistics,
                footprint_statistics_rows: vec![
                    crate::model::FootprintStatistic::Volume,
                    crate::model::FootprintStatistic::Delta,
                ],
                ..Default::default()
            });
            let axis = Arc::new(Mutex::new(None));
            chart
                .attach_primitive(SeriesId::MAIN, Tags(axis.clone()))
                .unwrap();
            frame(&ctx, &mut chart, 0.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 1.0, vec![]);
            let tags = ["Automatic", "Fixed"].map(|label| {
                painted.iter().find_map(|shape| match shape {
                    Shape::Text(text) if text.galley.text() == label => Some(text_rect(text).top()),
                    _ => None,
                })
            });
            (tags, *axis.lock().unwrap())
        });
        // Statistics rows are taken from the bottom of the price plot; the time axis below them
        // keeps its place, and so do the tags on it and the axis handed to the primitive.
        assert!(
            tops[1] == tops[0],
            "tag tops and axis top without statistics {:?}, with statistics {:?}",
            tops[0],
            tops[1]
        );
    }

    #[test]
    fn pinned_price_tags_hide_the_axis_labels_they_cover_in_every_pane() {
        struct Pinned(Arc<Mutex<f32>>);
        impl Primitive for Pinned {
            fn price_axis_labels(&self, _: &PrimitiveView<'_>) -> Vec<PriceAxisLabel> {
                let mut label = PriceAxisLabel::new(f64::NAN, "X", PINNED);
                label.fixed_coordinate = Some(*self.0.lock().unwrap());
                vec![label]
            }
        }
        let mut covered = Vec::new();
        for owner in ["main", "study", "second member"] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            let id = pane_owner(&mut chart, owner);
            // A non-finite offset omits the tag until a scale label is chosen.
            let offset = Arc::new(Mutex::new(f32::NAN));
            chart.attach_primitive(id, Pinned(offset.clone())).unwrap();
            frame(&ctx, &mut chart, 0.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 1.0, vec![]);
            let plot = pane_plot(&chart, owner);
            // The scale label of the pane nearest its middle.
            let mark = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text)
                        if text_rect(text).left() > plot.right()
                            && plot.y_range().contains(text_rect(text).center().y)
                            && text.galley.text().replace(',', "").parse::<f64>().is_ok() =>
                    {
                        Some(text_rect(text))
                    }
                    _ => None,
                })
                .min_by(|a, b| {
                    (a.center().y - plot.center().y)
                        .abs()
                        .total_cmp(&(b.center().y - plot.center().y).abs())
                })
                .unwrap_or_else(|| panic!("{owner}: a scale label"));
            *offset.lock().unwrap() = mark.center().y - plot.top();
            frame(&ctx, &mut chart, 2.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 3.0, vec![]);
            let tag = painted
                .iter()
                .find_map(|shape| match shape {
                    Shape::Rect(rect) if rect.fill == PINNED => Some(rect.rect),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{owner}: the pinned tag"));
            covered.push((
                owner,
                painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Text(text)
                            if text.galley.text() != "X" && text_rect(text).intersects(tag) =>
                        {
                            Some(text.galley.text().to_owned())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            ));
        }
        // Scale labels under a tag are left out in the main pane; every pane follows that rule.
        assert!(
            covered.iter().all(|(_, labels)| labels.is_empty()),
            "{covered:?}"
        );
    }

    #[test]
    fn image_export_refreshes_no_pane_primitive_before_it_is_attached() {
        struct Stamp(Log);
        impl PanePrimitive for Stamp {
            fn attached(&mut self, _: &egui::Context, _: PaneId) {
                self.0.lock().unwrap().push("attached".into());
            }
            fn update(&mut self) {
                self.0.lock().unwrap().push("update".into());
            }
        }
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        frame(&ctx, &mut chart, 0.0, vec![]);
        let log = Log::default();
        // Attached between two frames, so its `attached` hook has not run.
        let id = chart
            .attach_pane_primitive(PaneId::MAIN, Stamp(log.clone()))
            .unwrap();
        let mut image = chart
            .export_image(&ctx, &crate::widget::ImageExportOptions::default())
            .unwrap();
        image.textures_delta.clear();
        assert_eq!(*log.lock().unwrap(), Vec::<String>::new());
        // The next frame attaches it, and only then refreshes it.
        frame(&ctx, &mut chart, 1.0, vec![]);
        assert_eq!(*log.lock().unwrap(), ["attached", "update"]);
        assert!(chart.detach_primitive(id));
    }

    /// Of several points in one slot of a fed chart's interval, `set_custom_data` keeps the one
    /// with the latest key, as `set_data` keeps the latest bar of a slot, whatever their order.
    #[test]
    fn feed_slot_keeps_the_latest_point_as_bars_do() {
        struct Silent;
        impl crate::data::Datafeed for Silent {
            fn history(
                &self,
                _: crate::data::HistoryRequest,
                _: crate::data::Reply<crate::data::HistoryPage>,
            ) {
            }
        }
        let mut chart = crate::widget::ChartBuilder::new()
            .with_feed(Arc::new(Silent))
            .with_timeframe(crate::model::Timeframe::Hour1)
            .build();
        let (id, _) = band_series(&mut chart);
        let at = |minutes| DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(minutes);
        chart.set_data(
            SeriesId::MAIN,
            vec![
                Bar::new(at(45), 1.0, 2.0, 1.0, 2.0, 1.0),
                Bar::new(at(15), 1.0, 2.0, 1.0, 2.0, 1.0),
            ],
        );
        let point = |minutes, note| Band {
            time: at(minutes),
            values: Some((1.0, 2.0)),
            note,
        };
        assert!(chart.set_custom_data(id, vec![point(45, "00:45"), point(15, "00:15")]));
        let bars: Vec<_> = (chart.bars(SeriesId::MAIN).iter())
            .map(|bar| bar.time)
            .collect();
        let points: Vec<_> = chart
            .custom_points::<Band>(id)
            .iter()
            .map(|point| point.note)
            .collect();
        let kept = (bars, points);
        assert_eq!(kept, (vec![at(45)], vec!["00:45"]), "{kept:?}");
    }

    /// A point that counts its clones.
    #[derive(Debug)]
    struct Counted {
        time: DateTime<Utc>,
        clones: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl Clone for Counted {
        fn clone(&self) -> Self {
            self.clones
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Self {
                time: self.time,
                clones: Arc::clone(&self.clones),
            }
        }
    }

    impl CustomPoint for Counted {
        fn time(&self) -> DateTime<Utc> {
            self.time
        }
        fn prices(&self) -> Vec<f64> {
            vec![15.0]
        }
    }

    /// While the pointer rests and nothing changes, the chart reports nothing and copies no
    /// point; another key of the series changing leaves the point under the pointer the same,
    /// and supplying that point again reports it.
    #[test]
    fn stationary_pointer_over_custom_points() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let main = chart.bars(SeriesId::MAIN).to_vec();
        let clones = Arc::<std::sync::atomic::AtomicUsize>::default();
        let id = chart.add_custom_data_series::<Counted>(
            "Counted",
            |study| study.with_overlay(false),
            Probe("counted", Log::default()),
        );
        let point = |bar: usize| Counted {
            time: main[bar].time,
            clones: Arc::clone(&clones),
        };
        assert!(chart.set_custom_data(id, (0..100).map(point).collect()));
        chart.set_visible_bars(12);
        frame(&ctx, &mut chart, 0.0, vec![]);
        frame(&ctx, &mut chart, 0.1, vec![]);
        let pane = chart
            .pane_layout
            .panes
            .iter()
            .find(|pane| pane.members.contains(&id))
            .unwrap()
            .rect;
        let pos = Pos2::new(chart.get_chart_mapping().index_to_x(92.0), pane.center().y);
        frame(&ctx, &mut chart, 1.0, vec![Event::PointerMoved(pos)]);
        frame(&ctx, &mut chart, 1.1, vec![]);
        let hovered = chart
            .drain_events()
            .filter_map(|event| match event {
                ChartEvent::CrosshairMoved(Some(pointer)) => Some(pointer),
                _ => None,
            })
            .last()
            .expect("the pointer moved inside the pane");
        let at = hovered.custom_point::<Counted>(id).map(|point| point.time);
        assert_eq!(at, Some(main[92].time));
        let before = clones.load(std::sync::atomic::Ordering::Relaxed);
        for step in 0..10 {
            frame(&ctx, &mut chart, 2.0 + f64::from(step) * 0.1, vec![]);
        }
        let copied = clones.load(std::sync::atomic::Ordering::Relaxed) - before;
        let moved_after = |chart: &mut Chart, time: f64| {
            frame(&ctx, chart, time, vec![]);
            frame(&ctx, chart, time + 0.1, vec![]);
            chart
                .drain_events()
                .filter(|event| matches!(event, ChartEvent::CrosshairMoved(_)))
                .count()
        };
        // A point out of view changes; the one under the pointer stays as it was.
        assert!(chart.update_custom_data(id, point(10)));
        let moved = moved_after(&mut chart, 4.0);
        assert!(chart.update_custom_data(id, point(92)));
        let replaced = moved_after(&mut chart, 5.0);
        let seen = (copied, moved, replaced);
        assert_eq!(seen, (0, 0, 1), "{seen:?}");
    }

    /// On a price-based chart type, feeding the custom data series leaves the other studies
    /// drawn, whose values do not depend on its points, and its own value follows its points.
    #[test]
    fn custom_updates_keep_other_studies_on_price_based_charts() {
        use crate::model::{BarData, ChartType, ChartTypeParams};
        let ctx = egui::Context::default();
        let bars: Vec<Bar> = [100.0, 130.0, 160.0]
            .into_iter()
            .enumerate()
            .map(|(index, close)| {
                Bar::from_value(
                    DateTime::UNIX_EPOCH + chrono::Duration::minutes(index as i64),
                    close,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        let (id, _) = band_series(&mut chart);
        let closes = chart.add_indicator(Box::new(CustomIndicator::new("Close", |bars| {
            bars.iter()
                .map(|bar| IndicatorValue::Single(bar.close))
                .collect()
        })));
        chart.set_chart_type(ChartType::Renko);
        chart.set_chart_type_params(ChartTypeParams {
            renko_brick_size: 10.0,
            ..Default::default()
        });
        for at in 0..4 {
            frame(&ctx, &mut chart, f64::from(at), vec![]);
        }
        // Bricks of 10 from 100 close at 110, 120, ..., 160, so the third closes at 130.
        assert_eq!(chart.indicator_value_at(closes, 2), Some((0, 130.0)));
        let mut seen = Vec::new();
        for at in 0..3 {
            let point = Band {
                time: bars[2].time,
                values: Some((1.0, 2.0 + f64::from(at))),
                note: "",
            };
            assert!(chart.update_custom_data(id, point));
            frame(&ctx, &mut chart, 10.0 + f64::from(at), vec![]);
            seen.push(chart.indicator_value_at(closes, 2));
        }
        assert_eq!(seen, [Some((0, 130.0)); 3], "{seen:?}");
        // Bricks 3 to 5 are formed by the third bar and take its time, where the last point holds
        // (1, 4), whose current price is its high.
        assert_eq!(chart.indicator_value_at(id, 5), Some((0, 4.0)));
    }

    /// Every price of the points in view fits the scale a custom data series is drawn on in the
    /// main pane: after the study menu moves it there from its own pane, pinned to no scale, and
    /// with every point in view between two bars.
    #[test]
    fn custom_points_fit_their_scale_in_the_main_pane() {
        let mut ranges = Vec::new();
        for (pin, on_bar) in [
            (None, true),
            (Some(crate::studies::StudyScale::None), true),
            (None, false),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            let main = chart.bars(SeriesId::MAIN).to_vec();
            let (id, _) = band_series(&mut chart);
            let mut points = vec![Band {
                time: main[92].time + chrono::TimeDelta::hours(12),
                values: Some((1400.0, 1500.0)),
                note: "between two bars",
            }];
            if on_bar {
                points.push(Band {
                    time: main[90].time,
                    values: Some((1000.0, 1100.0)),
                    note: "on a bar",
                });
            }
            assert!(chart.set_custom_data(id, points));
            chart.set_visible_bars(12);
            assert!(chart.move_series_to_pane(id, crate::widget::PaneId::MAIN));
            if let Some(scale) = pin {
                assert!(chart.set_indicator_scale(id, scale));
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            frame(&ctx, &mut chart, 0.1, vec![]);
            let range = chart.series_price_map(id).unwrap().range();
            ranges.push((range.min, range.max));
        }
        let lowest = [1000.0, 1000.0, 1400.0];
        assert!(
            (ranges.iter().zip(lowest)).all(|(&(min, max), low)| min <= low && max >= 1500.0),
            "{ranges:?}"
        );
    }
}
