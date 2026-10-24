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
use crate::model::Bar;
use crate::studies::Indicator;
use chrono::{DateTime, Utc};
use egui::{Color32, CursorIcon, Painter, Pos2, Rect};
use std::ops::Range;

/// Identifies a primitive attached to a chart, for
/// [`Chart::detach_primitive`](crate::widget::Chart::detach_primitive) and pointer targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrimitiveId(pub(crate) u64);

/// When a primitive draws in its pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrimitivePass {
    /// Behind the series and studies of the pane, above its background, grid and sessions.
    Below,
    /// Above the series and studies of the pane, below drawings.
    Series,
    /// Above the series, studies and drawings of the pane. In the main pane it stays below the
    /// crosshair and legend.
    Above,
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
    /// The x of a fractional bar index, the centre of bar `index` when it is whole.
    pub fn index_to_x(&self, index: f64) -> f32 {
        self.mapping.idx_to_x(0) + index as f32 * self.mapping.bar_spacing
    }

    /// The x of a UTC time, interpolated between bars and extrapolated into whitespace as
    /// [`Chart::time_to_index`](crate::widget::Chart::time_to_index) does. `None` without bars.
    pub fn time_to_x(&self, time: DateTime<Utc>) -> Option<f32> {
        crate::widget::sync::index_at(self.bars, time).map(|index| self.index_to_x(index))
    }
}

/// A label a primitive puts on the price axis of its pane, on a tag in `color`.
#[derive(Clone, Debug, PartialEq)]
pub struct PriceAxisLabel {
    /// Price at the centre of the tag, on the scale of the primitive's series.
    pub price: f64,
    /// Text of the tag.
    pub text: String,
    /// Fill of the tag; the text is white or black, whichever reads better on it.
    pub color: Color32,
}

/// A label a primitive puts on the time axis, on a tag in `color`.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeAxisLabel {
    /// Time at the centre of the tag.
    pub time: DateTime<Utc>,
    /// Text of the tag.
    pub text: String,
    /// Fill of the tag; the text is white or black, whichever reads better on it.
    pub color: Color32,
}

/// Host drawing attached to a series, called by the chart while it draws each frame.
///
/// The chart calls the drawing hooks only while the series is visible. A primitive attached to
/// the main series, an added bar series or an overlay study draws in the main pane; one attached
/// to a study in a pane of its own draws in that pane. See the [module](self) documentation.
pub trait Primitive: Send {
    /// Draws in `pass` with `painter`, clipped to the plot area of the pane.
    fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
        let _ = (painter, view, pass);
    }

    /// Labels for the price axis of the pane, placed at their prices next to the chart's own
    /// tags. A label off the scale or without room is left out.
    fn price_axis_labels(&self, view: &PrimitiveView<'_>) -> Vec<PriceAxisLabel> {
        let _ = view;
        Vec::new()
    }

    /// Labels for the time axis, centred at their times. Axis labels they cover are left out.
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

    /// Whether the pointer at `pos` is over the primitive, and the cursor to show there.
    ///
    /// A hit is the pointer target of chart events. Drawings come first, then primitives,
    /// the latest attached first, then the series and studies.
    fn hit_test(&self, pos: Pos2, view: &PrimitiveView<'_>) -> Option<CursorIcon> {
        let _ = (pos, view);
        None
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
