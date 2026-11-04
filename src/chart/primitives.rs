//! Custom drawing attached to a series: primitives, and the renderers of custom series.
//!
//! A [`Primitive`] is host code that the chart calls while it draws a frame. It is attached to a
//! series with [`Chart::attach_primitive`](crate::widget::Chart::attach_primitive) and draws in
//! the pane of that series, on its price scale. A primitive can draw behind the series, above
//! them, or above the drawings as well ([`PrimitivePass`]), draw inside the price and time axes
//! and put labels on them, widen the price range its scale fits, take part in hit testing with
//! its own pointer cursor, and follow the bars of its series through lifecycle hooks.
//!
//! A custom series is a study that supplies the values (legend, scale, crosshair and last-value
//! tag) together with a primitive that draws its points in place of the study's plots and hit
//! tests them: [`Chart::add_custom_series`](crate::widget::Chart::add_custom_series).
//!
//! Every hook has a default that does nothing, so a primitive implements only what it needs.
//! `examples/primitives.rs` shades the regular session behind the bars with labels on the time
//! axis, and adds a custom series that draws the range of each bar in a pane of its own.
//!
//! ```rust
//! use egui::{Color32, Painter};
//! use egui_charts::chart::primitives::{Primitive, PrimitivePass, PrimitiveView};
//! use egui_charts::widget::{Chart, SeriesId};
//!
//! /// Shades the bars whose close is above `level`.
//! struct AboveLevel {
//!     level: f64,
//! }
//!
//! impl Primitive for AboveLevel {
//!     fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
//!         if pass != PrimitivePass::Below {
//!             return;
//!         }
//!         let rect = view.mapping.rect;
//!         let half = view.mapping.bar_spacing / 2.0;
//!         for index in view.visible.clone() {
//!             if view.bars[index].close > self.level {
//!                 let x = view.index_to_x(index as f64);
//!                 painter.rect_filled(
//!                     egui::Rect::from_x_y_ranges(x - half..=x + half, rect.y_range()),
//!                     0.0,
//!                     Color32::from_rgba_unmultiplied(41, 98, 255, 30),
//!                 );
//!             }
//!         }
//!     }
//! }
//!
//! # fn example(chart: &mut Chart) {
//! let id = chart.attach_primitive(SeriesId::MAIN, AboveLevel { level: 100.0 });
//! # assert!(id.is_some());
//! # }
//! ```

use crate::chart::coords::ChartMapping;
use crate::chart::selection::SeriesId;
use crate::model::{AutoscaleInfo, Bar};
use crate::studies::Indicator;
use chrono::{DateTime, Utc};
use egui::{Color32, CursorIcon, Painter, Pos2, Rect};
use std::ops::Range;
use std::sync::Arc;

/// Identifies a primitive attached to a chart, for
/// [`Chart::detach_primitive`](crate::widget::Chart::detach_primitive) and pointer targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrimitiveId(pub(crate) u64);

/// When a primitive draws in its pane. Within a pass, every
/// [`draw_background`](Primitive::draw_background) runs in attachment order before any
/// [`draw`](Primitive::draw).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrimitivePass {
    /// Behind the series and studies of the pane, above its background, grid and sessions.
    Below,
    /// Above the series and studies of the pane, below drawings.
    Series,
    /// Above the series, studies and drawings of the pane. In the main pane it stays below the
    /// crosshair and legend.
    Above,
}

/// A primitive or custom renderer's object under the pointer.
///
/// Among custom hits, later draw passes win (`Above`, `Series`, `Below`), then the
/// shortest distance in UI points. Equal hits keep ordinary primitives ahead of series
/// renderers, the latest attached primitive first, and renderers in series visual order.
/// When series backgrounds group foregrounds in attachment order, equal custom hits also
/// follow reverse attachment order across primitives and renderers.
/// Drawings retain priority; built-in series still follow series visual order.
/// Within a renderer, return its best object using these same pass and distance rules,
/// breaking ties in reverse paint order. Object ids are local to the pointer's target.
#[derive(Clone, Debug, PartialEq)]
pub struct PrimitiveHit {
    /// Cursor to display while hovering the object.
    pub cursor: CursorIcon,
    /// Host identity of the individual object, if the renderer distinguishes objects.
    pub object_id: Option<String>,
    /// Host-defined information about the hit, shared with hover and click payloads.
    pub data: Option<Arc<serde_json::Value>>,
    /// Distance from the pointer to the object in UI points. Hits with negative or
    /// non-finite distances are ignored.
    pub distance: f32,
    /// Draw pass of the hit object, used before distance when ranking custom hits.
    pub pass: PrimitivePass,
}

impl PrimitiveHit {
    /// A cursor-only hit on the series pass, at distance zero.
    pub fn new(cursor: CursorIcon) -> Self {
        Self {
            cursor,
            object_id: None,
            data: None,
            distance: 0.0,
            pass: PrimitivePass::Series,
        }
    }
}

/// The pane and scale a primitive draws on in the current frame.
#[derive(Clone)]
pub struct PrimitiveView<'a> {
    /// The series the primitive is attached to.
    pub series: SeriesId,
    /// The study the primitive is attached to, whose values a custom series renderer draws.
    /// `None` for a bar series.
    pub study: Option<&'a dyn Indicator>,
    /// Bar index to x on the time axis the panes share, and price to y on the series' price
    /// scale. `mapping.rect` is the plot area of the pane.
    pub mapping: ChartMapping,
    /// The bars laid out on the time axis, oldest first: the main series, or the elements of a
    /// price-based chart type. Bar `i` is centred at `index_to_x(i as f64)`.
    pub bars: &'a [Bar],
    /// The indices of `bars` in view.
    pub visible: Range<usize>,
}

impl PrimitiveView<'_> {
    /// Aligned source ranges for a custom renderer's merge hook. Groups start at multiples
    /// of `factor`, include the visible range's edge groups and never exceed `bars`.
    /// A factor of zero is treated as one. Merge values in source order so open/close and
    /// extrema keep their meaning; host payload aggregation belongs to the renderer.
    pub fn point_groups(&self, factor: usize) -> impl Iterator<Item = Range<usize>> {
        let factor = factor.max(1);
        let end = self.visible.end.min(self.bars.len());
        let start = if self.visible.start >= end {
            end
        } else {
            self.visible.start / factor * factor
        };
        (start..end)
            .step_by(factor)
            .map(move |start| start..start.saturating_add(factor).min(self.bars.len()))
    }

    /// The x of a fractional bar index, the centre of bar `index` when it is whole.
    pub fn index_to_x(&self, index: f64) -> f32 {
        self.mapping.index_to_x(index)
    }

    /// The x of a UTC time, interpolated between bars. Without a timeframe, whitespace extends
    /// the most common positive spacing among the outermost 16 bars (shorter spacing breaks
    /// ties, one second if none). `None` without bars.
    pub fn time_to_x(&self, time: DateTime<Utc>) -> Option<f32> {
        crate::model::index_at(self.bars, time).map(|index| self.index_to_x(index))
    }
}

/// A label a primitive puts on the price axis of its pane, on a tag in `color`.
///
/// Automatic tags avoid earlier tags and stay inside the pane. Fixed tags keep their centre,
/// are clipped to the axis and paint above automatic tags without reserving an overlap slot.
/// Later fixed tags paint above earlier ones. Hidden series and hidden axes omit their tags.
#[derive(Clone, Debug, PartialEq)]
pub struct PriceAxisLabel {
    /// Price at the centre of the tag, on the scale of the primitive's series.
    pub price: f64,
    /// Text of the tag.
    pub text: String,
    /// Fill of the tag.
    pub color: Color32,
    /// Text and tick colour; `None` chooses readable white or black from the fill.
    pub text_color: Option<Color32>,
    /// Centre in UI points from the top of this pane's price axis. `None` maps `price`.
    /// A fixed tag ignores `price`, even when it is off scale. Non-finite offsets are omitted.
    pub fixed_coordinate: Option<f32>,
    /// Whether to draw the short tick at the plot edge, independently of the tag. Defaults to true.
    pub tick_visible: bool,
}

impl PriceAxisLabel {
    /// An automatically positioned tag with contrasting text and a visible tick.
    pub fn new(price: f64, text: impl Into<String>, color: Color32) -> Self {
        Self {
            price,
            text: text.into(),
            color,
            text_color: None,
            fixed_coordinate: None,
            tick_visible: true,
        }
    }
}

/// A label a primitive puts on the time axis, on a tag in `color`.
///
/// Automatic tags move horizontally to fit inside the axis. Fixed tags retain their centre
/// and are clipped to the axis instead. Both cover ordinary time marks; fixed tags paint
/// above automatic primitive tags, with later fixed tags on top. Hidden series omit their tags.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeAxisLabel {
    /// Time at the centre of the tag.
    pub time: DateTime<Utc>,
    /// Text of the tag.
    pub text: String,
    /// Fill of the tag.
    pub color: Color32,
    /// Text and tick colour; `None` chooses readable white or black from the fill.
    pub text_color: Option<Color32>,
    /// Centre in UI points from the left of the shared time axis. `None` maps `time`.
    /// A fixed tag ignores `time`. Non-finite offsets are omitted.
    pub fixed_coordinate: Option<f32>,
    /// Whether to draw the short tick at the top of the axis, independently of the tag.
    /// Defaults to true.
    pub tick_visible: bool,
}

impl TimeAxisLabel {
    /// An automatically positioned tag with contrasting text and a visible tick.
    pub fn new(time: DateTime<Utc>, text: impl Into<String>, color: Color32) -> Self {
        Self {
            time,
            text: text.into(),
            color,
            text_color: None,
            fixed_coordinate: None,
            tick_visible: true,
        }
    }
}

/// Host drawing attached to a series, called by the chart while it draws each frame.
///
/// The chart calls the drawing hooks only while the series is visible. A primitive attached to
/// the main series, an added bar series or an overlay study draws in the main pane; one attached
/// to a study in a pane of its own draws in that pane. See the [module](self) documentation.
pub trait Primitive: Send {
    /// Draws the background of `pass`, clipped to the plot area of the pane. All participating
    /// backgrounds draw in attachment order before any foreground in that pass.
    /// The chart may defer these shapes; painter indices must stay within this hook.
    fn draw_background(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
        let _ = (painter, view, pass);
    }

    /// Draws the foreground of `pass` with `painter`, clipped to the plot area of the pane.
    fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
        let _ = (painter, view, pass);
    }

    /// Draws with `factor` consecutive source slots per physical pixel group, one while
    /// zoomed in. Override this for custom point aggregation; use
    /// [`PrimitiveView::point_groups`] to visit the aligned ranges. Cache aggregates in
    /// [`data_changed`](Self::data_changed) when visiting all source values would be costly.
    /// The default preserves [`draw`](Self::draw) for existing renderers.
    fn draw_merged(
        &self,
        painter: &Painter,
        view: &PrimitiveView<'_>,
        pass: PrimitivePass,
        factor: usize,
    ) {
        let _ = factor;
        self.draw(painter, view, pass);
    }

    /// Draws the background with the same grouping factor as [`draw_merged`](Self::draw_merged).
    /// The default calls [`draw_background`](Self::draw_background).
    fn draw_background_merged(
        &self,
        painter: &Painter,
        view: &PrimitiveView<'_>,
        pass: PrimitivePass,
        factor: usize,
    ) {
        let _ = factor;
        self.draw_background(painter, view, pass);
    }

    /// Labels for the price axis of the pane, placed at their prices next to the chart's own
    /// tags. An automatic label off the scale or without room is left out; fixed labels follow
    /// [`PriceAxisLabel`]'s clipping and overlap rules.
    fn price_axis_labels(&self, view: &PrimitiveView<'_>) -> Vec<PriceAxisLabel> {
        let _ = view;
        Vec::new()
    }

    /// Labels for the time axis, positioned according to [`TimeAxisLabel`]. Axis marks they
    /// cover are left out.
    ///
    /// The time axis is shared by every pane, so the view of a primitive in a study pane maps
    /// prices on the main pane here.
    fn time_axis_labels(&self, view: &PrimitiveView<'_>) -> Vec<TimeAxisLabel> {
        let _ = view;
        Vec::new()
    }

    /// Draws inside `axis`, the price axis of the pane, behind its labels.
    fn draw_price_axis(&self, painter: &Painter, axis: Rect, view: &PrimitiveView<'_>) {
        let _ = (painter, axis, view);
    }

    /// Draws inside `axis`, the time axis, behind its labels. The view is as in
    /// [`time_axis_labels`](Self::time_axis_labels).
    fn draw_time_axis(&self, painter: &Painter, axis: Rect, view: &PrimitiveView<'_>) {
        let _ = (painter, axis, view);
    }

    /// The lowest and highest price the scale of the series must show while it fits itself to
    /// the bars in view, `visible` indexing `bars` as in [`PrimitiveView`]. `None` adds nothing.
    /// An overlay study on an independent scale fits that scale to its own values only.
    fn autoscale(&self, bars: &[Bar], visible: Range<usize>) -> Option<(f64, f64)> {
        let _ = (bars, visible);
        None
    }

    /// Prices and extra top/bottom logical pixels to fit on the attached series' pane and
    /// scale. Shared scales take the largest request at each screen edge. Manual ranges
    /// ignore this contribution. Independent overlay studies retain their own value-only fit.
    /// The default preserves [`autoscale`](Self::autoscale) with zero extra margins.
    fn autoscale_info(&self, bars: &[Bar], visible: Range<usize>) -> Option<AutoscaleInfo> {
        self.autoscale(bars, visible).map(Into::into)
    }

    /// Whether the pointer at `pos` is over the primitive, and the cursor to show there.
    ///
    /// A hit is the pointer target of chart events. Drawings come first, then primitives,
    /// the latest attached first, then the series and studies.
    fn hit_test(&self, pos: Pos2, view: &PrimitiveView<'_>) -> Option<CursorIcon> {
        let _ = (pos, view);
        None
    }

    /// Identifies the hit object and supplies its cursor, distance, draw pass and host data.
    /// The chart reports the winning result as
    /// [`ChartPointer::primitive_hit`](crate::widget::ChartPointer::primitive_hit).
    ///
    /// The default wraps [`hit_test`](Self::hit_test) in [`PrimitiveHit::new`], preserving
    /// cursor-only implementations. Override this hook to distinguish or rank objects.
    fn hit_test_result(&self, pos: Pos2, view: &PrimitiveView<'_>) -> Option<PrimitiveHit> {
        self.hit_test(pos, view).map(PrimitiveHit::new)
    }

    /// Called once, at the first frame after the primitive was attached to `series`. `ctx` is
    /// the chart's egui context; keep a clone to request repaints, from any thread.
    fn attached(&mut self, ctx: &egui::Context, series: SeriesId) {
        let _ = (ctx, series);
    }

    /// Called once when the primitive is detached, directly or by removing its series. Undoing
    /// the removal of a study restores the study without its primitives.
    fn detached(&mut self) {}

    /// Called after [`attached`](Self::attached) and at every frame after the bars of the series
    /// changed, before anything is drawn. `bars` are the series' own bars, oldest first; for a
    /// study they are the main series bars the study is computed from.
    fn data_changed(&mut self, bars: &[Bar]) {
        let _ = bars;
    }
}
