//! Host notifications from chart setters, history loads and rendered interaction.

use super::{Chart, ChartElementId, SeriesId, VisibleTimeRange};
use crate::chart::rendering::StudyValue;
use crate::model::Bar;
use chrono::{DateTime, Utc};
use egui::{Modifiers, PointerButton, Pos2, Ui};

/// Which study plots are included in cursor and click payloads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CursorDataPolicy {
    /// Only visible studies and visible plots, matching chart readouts.
    #[default]
    Visible,
    /// All computed study plots, including hidden studies and hidden plots.
    /// This does not change rendering, autoscale or crosshair index eligibility.
    IncludeHidden,
}

/// A chart cursor position with the data a host legend, tooltip or context action reads.
/// Identical whitespace slots compare equal, including their colours and host payloads.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ChartPointer {
    /// Stored timestamp of the nearest eligible main, added-series or custom point, including
    /// whitespace. `None` without eligible cursor data or when the selected position has no
    /// stored point.
    pub time: Option<DateTime<Utc>>,
    /// Price on the main pane, or the value on the scale of the hovered pane or volume.
    pub price: f64,
    /// `MAIN`, `VOLUME`, or the first series or study of another hovered pane. A primitive hit
    /// in an empty pane uses `MAIN`; `target` identifies the primitive independently.
    pub series: SeriesId,
    /// Nearest slot in the rendered series, counting elements on price-based chart types.
    /// Added points between main-series slots round to the nearest slot. Negative or past
    /// the last bar in whitespace.
    pub index: isize,
    /// Pointer position in UI points, the coordinates of the chart's `Response::rect`.
    pub pos: Pos2,
    /// Pane counted from the top among the panes drawn, the price pane with its volume band
    /// included: `0` unless panes sit above the price pane.
    pub pane: usize,
    /// Main series bar, or price-based element, at `time`, with its colours and payload: a
    /// [whitespace](Bar::whitespace) bar in an empty slot, `None` when the main series has no
    /// point at that time or has been removed.
    pub bar: Option<Bar>,
    /// Close of the last bar or element with a value before the selected main-series point,
    /// from which legends measure the change. `None` when no main-series point is selected.
    pub previous_close: Option<f64>,
    /// Every series added with `add_series` or `add_comparison` that has a bar at `time`,
    /// including hidden series and their host payloads.
    pub series_bars: Vec<(SeriesId, Bar)>,
    /// Points of custom data series at `time`, read with [`Self::custom_point`].
    custom_points: Vec<CustomPointAt>,
    /// Study plots at the selected main-series point, each with its stable study id. Defaults to the visible plots
    /// returned by [`Chart::study_values_at`]; [`Chart::set_cursor_data_policy`] can include
    /// hidden studies and plots. Missing values remain `None`; empty between main-series points.
    pub studies: Vec<(SeriesId, StudyValue)>,
    /// The annotation, drawing, primitive, series or study under the pointer. Custom hit priority is
    /// documented by [`PrimitiveHit`](super::PrimitiveHit).
    pub target: Option<PointerTarget>,
    /// The winning primitive or custom renderer's object, including its cursor and host data.
    /// `target` identifies its primitive or series; `None` for annotations, drawings and built-in plots.
    pub primitive_hit: Option<super::PrimitiveHit>,
    /// Modifier keys held during the event.
    pub modifiers: Modifiers,
    /// Device that produced the event.
    pub kind: PointerKind,
    /// Originating egui pointer or touch event, including button state or touch device, id,
    /// phase and pressure when supplied by the integration. A matching touch takes precedence
    /// over its emulated pointer event, and `pos` and `modifiers` are those of that event.
    /// `None` means the position was recomputed from chart state without originating input.
    pub origin: Option<egui::Event>,
}

/// A clone of the point of a custom data series under the cursor.
#[derive(Debug, Clone)]
struct CustomPointAt {
    series: SeriesId,
    /// Points have no equality of their own: the revision of the point under the cursor, renewed
    /// when the host supplies that point again.
    revision: u64,
    point: std::sync::Arc<dyn std::any::Any + Send + Sync>,
}

impl PartialEq for CustomPointAt {
    fn eq(&self, other: &Self) -> bool {
        self.series == other.series && self.revision == other.revision
    }
}

impl PartialEq for ChartPointer {
    fn eq(&self, other: &Self) -> bool {
        self.origin == other.origin && self.same_position(other)
    }
}

impl ChartPointer {
    /// The point of the custom data series `series` at [`time`](Self::time), including
    /// whitespace and hidden series, as [`series_bars`](Self::series_bars) holds added bars.
    /// `None` without a point at that key, for another series or for another point type.
    pub fn custom_point<P: super::CustomPoint>(&self, series: SeriesId) -> Option<&P> {
        self.custom_points
            .iter()
            .find(|item| item.series == series)?
            .point
            .downcast_ref()
    }

    fn same_position(&self, other: &Self) -> bool {
        self.time == other.time
            && self.price == other.price
            && self.series == other.series
            && self.index == other.index
            && self.pos == other.pos
            && self.pane == other.pane
            && self.previous_close == other.previous_close
            && self.custom_points == other.custom_points
            && self.studies == other.studies
            && self.target == other.target
            && self.primitive_hit == other.primitive_hit
            && self.modifiers == other.modifiers
            && self.kind == other.kind
            && match (&self.bar, &other.bar) {
                (Some(left), Some(right)) => same_bar(left, right),
                (None, None) => true,
                _ => false,
            }
            && self.series_bars.len() == other.series_bars.len()
            && self.series_bars.iter().zip(&other.series_bars).all(
                |((left_id, left), (right_id, right))| left_id == right_id && same_bar(left, right),
            )
    }
}

/// Whether two bars are the same, including whitespace bars, whose NaN values never compare equal.
pub(super) fn same_bar(left: &Bar, right: &Bar) -> bool {
    left == right
        || (left.is_whitespace()
            && right.is_whitespace()
            && left.time == right.time
            && left.colors == right.colors
            && left.payload == right.payload)
}

/// The object under the pointer of a [`ChartPointer`].
///
/// Annotations follow paint order: normal markers and added-series price lines sit with
/// their series; above-series markers precede main-series price lines and drawings; top
/// markers follow drawings. Above-pass primitives follow top markers.
/// Later annotations in the same layer win. Only the clipped parts painted this frame can be targeted.
/// Existing drawing and custom-hit precedence applies when no annotation wins.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointerTarget {
    /// A series or study line, as a click there would select it.
    Element(ChartElementId),
    /// A drawing, by its drawing id.
    Drawing(usize),
    /// A primitive whose [`hit_test_result`](super::Primitive::hit_test_result) reports the pointer.
    Primitive(super::PrimitiveId),
    /// A painted bar marker. The host ID remains stable when the marker list changes.
    Marker {
        /// The series that owns the marker.
        series: SeriesId,
        /// The identity supplied by the host, or `None` for an unnamed marker.
        id: Option<String>,
        /// Time of the marker, mapped to its displayed element on price-based charts.
        time: DateTime<Utc>,
    },
    /// A painted custom price line with its live handle.
    PriceLine {
        /// The series that owns the line.
        series: SeriesId,
        /// The handle returned by [`Chart::add_price_line`].
        id: super::PriceLineId,
    },
}

/// The device behind a [`ChartPointer`].
///
/// egui does not distinguish pens. Hosts with device information use
/// [`Chart::set_pointer_kind`] before showing the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointerKind {
    /// A mouse, trackpad or other hovering pointer.
    Mouse,
    /// A finger on a touch surface, or an unidentified pen from egui.
    Touch,
    /// A pen identified by the host input integration.
    Pen,
}

/// An accepted host data operation, reported by [`ChartEvent::DataOperation`].
/// Update times are the incoming timestamps, matched using the chart's bar slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataOperation {
    /// A full replacement through [`Chart::set_data`], including an empty or unchanged load.
    SetData,
    /// An [`update`](Chart::update) beyond the last stored slot, or into an empty series.
    Append(DateTime<Utc>),
    /// An [`update`](Chart::update) to an existing slot, including an unchanged bar.
    Update(DateTime<Utc>),
    /// An [`update`](Chart::update) inserted into a missing slot before the last stored slot.
    Insert(DateTime<Utc>),
}

/// A notification for the host. Existing selection and action polling APIs remain independent.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ChartEvent {
    /// Oldest notifications dropped because the host did not drain the 1024-entry queue.
    /// Emitted first on the next drain; read the current chart state to recover.
    EventsDropped(usize),
    /// The UTC plot edges changed; `None` means there is no data to map.
    VisibleRangeChanged(Option<VisibleTimeRange>),
    /// The visible logical range changed, as [`Chart::visible_logical_range`] returns it,
    /// including changes of bar spacing alone; `None` means there is no data.
    VisibleLogicalRangeChanged(Option<crate::model::LogicalRange>),
    /// The time axis changed size, as [`Chart::time_scale_size`] returns it, including its
    /// first non-zero size.
    TimeScaleResized(egui::Vec2),
    /// The local cursor moved or anything its pointer carries changed, such as the bar under a
    /// stationary cursor, held modifiers or touch details; `None` means it left the plots or
    /// loaded data has no eligible crosshair index in view. Empty plots and visible annotation
    /// targets without eligible cursor data report a position without a data time.
    /// Programmatically synchronized cursors do not emit this event.
    CrosshairMoved(Option<ChartPointer>),
    /// A primary or secondary click in a plot, including clicks selecting drawings or studies.
    Click {
        /// Click location in the pane's units.
        position: ChartPointer,
        /// The button released to complete the click.
        button: PointerButton,
    },
    /// A primary double click in a plot, after the [`Self::Click`] of each of its clicks.
    DoubleClick {
        /// Location of the second click.
        position: ChartPointer,
    },
    /// A primary click on a bar mark with a host ID. Of overlapping marks the one painted on
    /// top is reported, and the click selects nothing else.
    MarkClick {
        /// The [`Marker::id`](crate::model::Marker::id) the host gave the mark.
        id: String,
        /// The series the mark belongs to; [`SeriesId::MAIN`] for the main series.
        series: SeriesId,
        /// The time of the mark; on price-based chart types, of the element it sits on.
        time: DateTime<Utc>,
    },
    /// A primary click on the Open row of a post or idea card with a link. Open the post or
    /// idea it names; the link may be any text the user typed, so check it, such as its
    /// scheme, before opening it. The click also selects the card, as any click on it does.
    #[cfg(feature = "drawings")]
    #[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
    LinkClick {
        /// Id of the drawing.
        drawing: usize,
        /// Its [`PostLink::link`](crate::model::PostLink::link).
        link: String,
    },
    /// The user placed an image or dropped picture files on the price pane. Answer with
    /// [`Chart::set_drawing_image`]; until then the image shows its placeholder.
    #[cfg(feature = "drawings")]
    #[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
    ImageRequested(ImageRequest),
    /// A successful history page, `set_data` or `set_custom_data` completed, including an empty
    /// result. Live updates emit `DataUpdated` instead; failed or obsolete feed replies emit
    /// nothing.
    DataLoaded {
        /// Series that received data.
        series: SeriesId,
        /// Total accepted bars or points now held by that series, after cleaning and merging.
        bars: usize,
    },
    /// A live bar from `update` or the chart's feed changed the bars of a series, `pop_bars`
    /// removed some, or `update_custom_data` changed a custom data series. Changes of one series
    /// in a row are reported once, with the latest count.
    DataUpdated {
        /// Series whose bars or points changed.
        series: SeriesId,
        /// Total bars or points now held by that series.
        bars: usize,
    },
    /// One accepted `set_data`, `update`, `set_custom_data` or `update_custom_data` call, when
    /// enabled with [`Chart::set_data_operation_notifications`]. Never coalesced; additional to
    /// the compact `DataLoaded` and `DataUpdated` events. Feed replies, removals and constructor
    /// data do not emit this event. Overflow is reported by `EventsDropped` as for other
    /// notifications.
    DataOperation {
        /// Series that accepted the operation.
        series: SeriesId,
        /// The operation and, for an update, its incoming bar time.
        operation: DataOperation,
        /// Total bars now held in the series' current window, after cleaning and retention.
        bars: usize,
    },
    /// The symbol setter accepted a different symbol.
    SymbolChanged(String),
    /// The interval setter accepted a different interval.
    TimeframeChanged(crate::model::Timeframe),
    /// An added bar series joined the chart: through [`Chart::add_series`] or
    /// [`Chart::add_comparison`], a restored snapshot, or undo or redo bringing back a series.
    /// Undo and redo keep its id, options, visibility, pane, scale, markers, price lines,
    /// price formatter, bar limit and bars, under the current replay cutoff. Supply current
    /// bars with [`Chart::set_data`] and resume [`Chart::update`]. Snapshot restore assigns
    /// new ids and leaves supplying bars to the host.
    SeriesAdded(SeriesId),
    /// An added bar series left the chart: through [`Chart::remove_series`] or
    /// [`Chart::remove_pane`], the pane menu, a restored snapshot, or undo or redo of the edit
    /// that added it. Data sent for it is ignored from then on. Undo or redo can bring it back
    /// with the same id, so keep what the host needs to feed it again.
    SeriesRemoved(SeriesId),
}

/// An image drawing waiting for its picture, from [`ChartEvent::ImageRequested`].
///
/// The chart reads no file and opens no dialog. For a placed image, `file` is `None` and the
/// host may offer a file chooser; for a dropped file it holds egui's handle, which gives the
/// path on native platforms and the bytes on the web. Treat the file as untrusted: check its
/// size, format and pixel dimensions before loading it, then call [`Chart::set_drawing_image`]
/// with an address the host's image loaders resolve.
#[cfg(feature = "drawings")]
#[cfg_attr(docsrs, doc(cfg(feature = "drawings")))]
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ImageRequest {
    /// Id of the image drawing to answer.
    pub drawing: usize,
    /// The dropped file, or `None` when the user placed the image with the tool.
    pub file: Option<egui::DroppedFileHandle>,
}

#[cfg(feature = "drawings")]
impl PartialEq for ImageRequest {
    fn eq(&self, other: &Self) -> bool {
        self.drawing == other.drawing
            && match (&self.file, &other.file) {
                (None, None) => true,
                (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}

#[derive(Default)]
pub(super) struct HostEvents {
    // Queue growth should move handles, not large pointer snapshots.
    queue: std::collections::VecDeque<Box<ChartEvent>>,
    dropped: usize,
    pub(super) data_operations: bool,
    range: Option<VisibleTimeRange>,
    logical_range: Option<crate::model::LogicalRange>,
    time_scale_size: egui::Vec2,
    pointer: Option<ChartPointer>,
    kind: Option<PointerKind>,
    cursor_data_policy: CursorDataPolicy,
    click_frames: [Option<u64>; 3],
    modifiers: Modifiers,
    input_frame: Option<(u64, Modifiers)>,
    /// What the pointer is over this frame, resolved while the plots are drawn.
    pub target: Option<PointerTarget>,
    pub annotation_target: std::cell::RefCell<Option<PointerTarget>>,
    pub primitive_hit: Option<super::PrimitiveHit>,
    pub volume_map: Option<crate::scales::PriceMap>,
    #[cfg(feature = "drawings")]
    dropped_images: Vec<egui::DroppedFileHandle>,
}

impl HostEvents {
    pub(super) fn push(&mut self, event: ChartEvent) {
        if self.queue.len() == 1024 {
            self.queue.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.queue.push_back(Box::new(event));
    }
}

impl Chart {
    /// Selects the study data included in [`ChartEvent::CrosshairMoved`], [`ChartEvent::Click`]
    /// and [`ChartEvent::DoubleClick`]. The default is [`CursorDataPolicy::Visible`].
    ///
    /// Takes effect on the next `show`, including for a stationary cursor. This changes only
    /// host payloads; legends, tooltips, rendering and autoscale retain their visibility rules.
    pub fn set_cursor_data_policy(&mut self, policy: CursorDataPolicy) {
        self.host_events.cursor_data_policy = policy;
    }

    /// Enables an additional [`ChartEvent::DataOperation`] for each accepted
    /// [`set_data`](Self::set_data) or [`update`](Self::update) call, in call order, including
    /// unchanged data. Disabled by default; changing this setting affects future calls and keeps
    /// already queued events.
    /// Compact state-change notifications remain available in either mode.
    ///
    /// Invalid updates emit no operation event and increment
    /// [`FeedState::rejected_bars`](crate::data::FeedState::rejected_bars).
    /// A full replacement still emits one after dropping invalid bars, even if none remain.
    /// Unknown, removed, volume and study series accept no data and emit no operation event.
    /// During replay, updates are classified against the retained history, including future bars.
    /// Feed notifications keep their existing batch semantics; this setting observes direct calls.
    pub fn set_data_operation_notifications(&mut self, enabled: bool) {
        self.host_events.data_operations = enabled;
    }

    /// Overrides the input device reported by pointer events. Set this before `show` when
    /// the host knows the active device, including pens that egui cannot identify. `None`
    /// restores automatic mouse/touch detection. Update it when the active device changes.
    pub fn set_pointer_kind(&mut self, kind: Option<PointerKind>) {
        self.host_events.kind = kind;
    }

    /// Drains host notifications in occurrence order. Call after `show` each frame.
    ///
    /// Symbol, interval, data and series setters enqueue immediately; view and pointer changes are
    /// measured during `show`. Unchanged views and pointers emit nothing. Drain
    /// regularly: the newest 1024 notifications remain queued until taken; overflow is
    /// reported as `EventsDropped` before the retained events. A new chart's constructor data
    /// emits no load event. This queue does not consume context, quote or selection actions.
    ///
    /// ```
    /// # use egui_charts::{model::BarData, widget::{Chart, ChartEvent}};
    /// # let mut chart = Chart::new(BarData::new());
    /// # egui::__run_test_ui(|ui| {
    /// chart.show(ui);
    /// for event in chart.drain_events() {
    ///     if let ChartEvent::Click { position, .. } = event {
    ///         // Route the time and price to host-owned application state.
    ///         let _ = (position.time, position.price);
    ///     }
    /// }
    /// # });
    /// ```
    pub fn drain_events(&mut self) -> impl Iterator<Item = ChartEvent> + '_ {
        let dropped = std::mem::take(&mut self.host_events.dropped);
        (dropped > 0)
            .then_some(ChartEvent::EventsDropped(dropped))
            .into_iter()
            .chain(self.host_events.queue.drain(..).map(|event| *event))
    }

    pub(super) fn data_loaded(&mut self, series: SeriesId) {
        self.host_events.push(ChartEvent::DataLoaded {
            series,
            bars: self.series_len(series),
        });
    }

    pub(super) fn data_set(&mut self, series: SeriesId) {
        self.data_loaded(series);
        self.data_operation(series, DataOperation::SetData);
    }

    pub(super) fn data_operation(&mut self, series: SeriesId, operation: DataOperation) {
        if self.host_events.data_operations {
            self.host_events.push(ChartEvent::DataOperation {
                series,
                operation,
                bars: self.series_len(series),
            });
        }
    }

    pub(super) fn data_updated(&mut self, series: SeriesId) {
        let bars = self.series_len(series);
        if let Some(ChartEvent::DataUpdated {
            series: last,
            bars: count,
        }) = self
            .host_events
            .queue
            .iter_mut()
            .rev()
            .map(Box::as_mut)
            .find(|event| !matches!(event, ChartEvent::DataOperation { .. }))
            && *last == series
        {
            *count = bars;
        } else {
            self.host_events
                .push(ChartEvent::DataUpdated { series, bars });
        }
    }

    /// Selects from the same fractional positions used to paint added series.
    fn pointer_time(&self, pos: Pos2) -> Option<(Option<DateTime<Utc>>, f64)> {
        let shown = self.state.shown();
        let coords = self.get_chart_mapping();
        let index = coords.x_to_idx_f64(pos.x);
        if shown.is_empty() {
            return Some((None, index.round()));
        }
        let pointer_time = crate::model::time_at(shown, index)?;
        let mut selected = self.crosshair_position(pos, &coords).map(|pos| {
            let index = coords.x_to_idx_f64(pos.x).round();
            let time =
                (index >= 0.0 && index < shown.len() as f64).then(|| shown[index as usize].time);
            (time, index)
        });
        let x_at = |index| {
            self.state.time_scale().idx_to_coord_precise(
                index,
                coords.rect.left(),
                coords.rect.width(),
            )
        };
        let mut distance = selected.map_or(f32::INFINITY, |(_, index)| (x_at(index) - pos.x).abs());
        let precision = f32::EPSILON
            * coords
                .rect
                .left()
                .abs()
                .max(coords.rect.right().abs())
                .max(1.0);
        let mut nearest = |times: &mut dyn Iterator<Item = DateTime<Utc>>| {
            for time in times {
                let Some(index) = crate::model::index_at(shown, time) else {
                    continue;
                };
                let x = x_at(index);
                let candidate_distance = (x - pos.x).abs();
                if candidate_distance > distance + precision {
                    break;
                }
                if !coords.is_x_visible(x) {
                    continue;
                }
                let main = shown.binary_search_by_key(&time, |bar| bar.time).ok();
                if self.crosshair_time_eligible(time, main)
                    && (candidate_distance + precision < distance
                        || selected.is_none_or(|(selected, _)| {
                            selected.is_none_or(|selected| time > selected)
                        }))
                {
                    selected = Some((Some(time), index));
                    distance = candidate_distance;
                }
            }
        };
        for item in &self.price_series {
            let at = item.bars.partition_point(|bar| bar.time < pointer_time);
            nearest(&mut item.bars[..at].iter().rev().map(|bar| bar.time));
            nearest(&mut item.bars[at..].iter().map(|bar| bar.time));
        }
        for custom in self.custom_series() {
            let keys = custom.keys();
            let keys = custom.shown(&keys);
            let at = keys.partition_point(|key| key.time < pointer_time);
            nearest(&mut keys[..at].iter().rev().map(|key| key.time));
            nearest(&mut keys[at..].iter().map(|key| key.time));
        }
        selected
    }

    fn prepare_pointer_input(&mut self, ui: &Ui) {
        let frame = ui.ctx().cumulative_frame_nr();
        if self.host_events.input_frame.map(|(at, _)| at) != Some(frame) {
            self.host_events.input_frame = Some((frame, self.host_events.modifiers));
            self.host_events.modifiers = ui.input(|input| input.modifiers);
        }
    }

    fn pointer_origin(
        &self,
        ui: &Ui,
        pos: Pos2,
        button: Option<PointerButton>,
    ) -> Option<(egui::Event, Modifiers)> {
        use egui::{Event, TouchPhase};

        ui.input(|input| {
            let events = &input.raw.events;
            let initial = if events
                .iter()
                .any(|event| matches!(event, Event::ModifiersChanged(_)))
            {
                self.host_events.input_frame?.1
            } else {
                input.modifiers
            };
            let inputs = || {
                events
                    .iter()
                    .enumerate()
                    .scan(initial, |modifiers, (at, event)| {
                        match event {
                            Event::ModifiersChanged(keys)
                            | Event::PointerButton {
                                modifiers: keys, ..
                            }
                            | Event::Key {
                                modifiers: keys, ..
                            }
                            | Event::MouseWheel {
                                modifiers: keys, ..
                            } => *modifiers = *keys,
                            Event::WindowFocused(false) => *modifiers = Modifiers::NONE,
                            _ => {}
                        }
                        Some((at, event, *modifiers))
                    })
            };
            let (at, event, modifiers) = inputs()
                .filter(|(_, event, _)| match event {
                    Event::PointerMoved(point) | Event::Touch { pos: point, .. } => {
                        button.is_none() && *point == pos
                    }
                    Event::PointerButton {
                        pos: point,
                        button: released,
                        pressed,
                        ..
                    } => *point == pos && button.is_none_or(|b| b == *released && !pressed),
                    _ => false,
                })
                .last()?;
            // Integrations emit both touch and emulated pointer events, in either order.
            let phase_matches = |phase: TouchPhase| match event {
                Event::PointerMoved(_) => matches!(phase, TouchPhase::Start | TouchPhase::Move),
                Event::PointerButton {
                    button: PointerButton::Primary,
                    pressed,
                    ..
                } => {
                    if *pressed {
                        phase == TouchPhase::Start
                    } else {
                        matches!(phase, TouchPhase::End | TouchPhase::Cancel)
                    }
                }
                _ => false,
            };
            let touch = inputs()
                .filter(|(_, candidate, _)| {
                    matches!(candidate, Event::Touch { pos: point, phase, .. }
                        if *point == pos && phase_matches(*phase))
                })
                .min_by_key(|(index, _, _)| index.abs_diff(at));
            let (_, event, modifiers) = touch.unwrap_or((at, event, modifiers));
            Some((event.clone(), modifiers))
        })
    }

    fn pointer_at(
        &self,
        ui: &Ui,
        pos: Pos2,
        origin: Option<(egui::Event, Modifiers)>,
    ) -> Option<ChartPointer> {
        let plot = self.last_rendered_price_rect;
        let shown = self.state.shown();
        let pane_primitive = matches!(self.host_events.target.as_ref(),
            Some(PointerTarget::Primitive(id))
                if self.primitives.iter().any(|item| item.id == *id && item.pane.is_some())
        );
        let (time, index) = self.pointer_time(pos).or_else(|| {
            (self.host_events.annotation_target.borrow().is_some() || pane_primitive)
                .then(|| (None, self.get_chart_mapping().x_to_idx_f64(pos.x)))
        })?;
        let index = index.round();
        let bar_at = (index >= 0.0 && index < shown.len() as f64)
            .then_some(index as usize)
            .filter(|at| Some(shown[*at].time) == time);
        let magnet = |map: crate::scales::PriceMap, values: Vec<f64>| {
            if self.chart_options.crosshair.mode.snaps() {
                crate::chart::rendering::snap_to_values(pos.y, &map, values)
                    .unwrap_or_else(|| map.y_to_price(pos.y))
            } else {
                map.y_to_price(pos.y)
            }
        };
        // Panes are counted as drawn, from the top; the price pane may sit below others.
        let pane_index = |id: super::PaneId| {
            (self.pane_layout.panes.iter())
                .filter(|pane| {
                    pane.id == super::PaneId::MAIN
                        || pane.id == self.main_series_pane()
                        || self.pane_shown(pane)
                })
                .position(|pane| pane.id == id)
                .unwrap_or_default()
        };
        let (series, price, pane) = if let Some(group) =
            self.pane_layout.panes.iter().find(|group| {
                group.id != self.main_series_pane()
                    && group.rect.contains(pos)
                    && plot.x_range().contains(pos.x)
            }) {
            let members = self.pane_series(group.id);
            let owner = members
                .first()
                .copied()
                .or(pane_primitive.then_some(SeriesId::MAIN))?;
            let map = members
                .first()
                .and_then(|id| self.series_price_map(*id))
                .unwrap_or_else(|| group.price_map());
            let closes = members.iter().filter_map(|id| {
                let item =
                    (self.price_series.iter()).find(|item| item.id == *id && item.visible)?;
                let at = item
                    .bars
                    .binary_search_by_key(&time?, |bar| bar.time)
                    .ok()?;
                let own = self.series_price_map(*id).unwrap_or(map);
                (!item.bars[at].is_whitespace()).then(|| {
                    let value = item.bars[at].close;
                    if own == map {
                        value
                    } else {
                        map.y_to_price(own.price_to_y(value))
                    }
                })
            });
            let values = members
                .iter()
                .filter_map(|id| self.study_ids.iter().position(|study| study == id))
                .map(|index| {
                    (
                        self.study_ids[index],
                        &self.rendered_studies().indicators()[index],
                    )
                })
                .filter(|(_, study)| study.is_visible())
                .flat_map(|(id, study)| {
                    let own = self.series_price_map(id).unwrap_or(map);
                    (0..study.line_count())
                        .filter(|line| {
                            let plot = study.plot_style(*line);
                            plot.visible
                                && !matches!(
                                    plot.plot_type,
                                    crate::studies::PlotType::BarColor
                                        | crate::studies::PlotType::BackgroundColor
                                )
                        })
                        .filter_map(move |line| {
                            bar_at
                                .and_then(|index| study.plot_value(line, index))
                                .map(|value| {
                                    if own == map {
                                        value
                                    } else {
                                        map.y_to_price(own.price_to_y(value))
                                    }
                                })
                        })
                })
                .chain(closes)
                .collect::<Vec<_>>();
            (owner, magnet(map, values), pane_index(group.id))
        } else if self.last_rendered_volume_rect.contains(pos)
            && let Some(map) = self.host_events.volume_map
        {
            let values = bar_at
                .map(|index| shown[index].volume)
                .into_iter()
                .collect();
            (
                SeriesId::VOLUME,
                magnet(map, values),
                pane_index(self.main_series_pane()),
            )
        } else if plot.contains(pos) {
            let snapped = bar_at.map_or(pos, |index| {
                self.snap_crosshair(pos, &self.last_price_map, index)
            });
            let price = self.last_price_map.y_to_price(snapped.y);
            (SeriesId::MAIN, price, pane_index(self.main_series_pane()))
        } else {
            return None;
        };
        price.is_finite().then(|| ChartPointer {
            time,
            price,
            series,
            index: index as isize,
            pos,
            pane,
            bar: bar_at
                .filter(|_| self.main_series_present)
                .map(|index| shown[index].clone()),
            previous_close: bar_at
                .and_then(|index| crate::model::last_with_value(&shown[..index]))
                .map(|bar| bar.close),
            series_bars: self
                .price_series
                .iter()
                .filter_map(|item| {
                    let at = item
                        .bars
                        .binary_search_by_key(&time?, |bar| bar.time)
                        .ok()?;
                    Some((item.id, item.bars[at].clone()))
                })
                .collect(),
            custom_points: time.map_or_else(Vec::new, |time| {
                let previous = self.host_events.pointer.as_ref();
                let previous = previous.map_or(&[][..], |pointer| &pointer.custom_points);
                self.custom_points_at(time)
                    .map(|(series, revision, copy)| {
                        let kept = previous
                            .iter()
                            .find(|point| point.series == series && point.revision == revision);
                        CustomPointAt {
                            series,
                            revision,
                            point: kept
                                .map_or_else(copy, |kept| std::sync::Arc::clone(&kept.point)),
                        }
                    })
                    .collect()
            }),
            studies: bar_at.map_or_else(Vec::new, |index| {
                self.study_rows_at_with_policy(index, self.host_events.cursor_data_policy)
            }),
            target: self
                .host_events
                .annotation_target
                .borrow()
                .clone()
                .or_else(|| self.host_events.target.clone()),
            primitive_hit: self
                .host_events
                .annotation_target
                .borrow()
                .is_none()
                .then(|| self.host_events.primitive_hit.clone())
                .flatten(),
            modifiers: origin
                .as_ref()
                .map_or_else(|| ui.input(|input| input.modifiers), |(_, keys)| *keys),
            kind: self.host_events.kind.unwrap_or_else(|| match &origin {
                Some((egui::Event::Touch { .. }, _)) => PointerKind::Touch,
                Some(_) => PointerKind::Mouse,
                None => self
                    .host_events
                    .pointer
                    .as_ref()
                    .map_or(PointerKind::Mouse, |pointer| pointer.kind),
            }),
            origin: origin.map(|(event, _)| event),
        })
    }

    pub(super) fn record_host_events(&mut self, ui: &Ui) {
        self.prepare_pointer_input(ui);
        #[cfg(feature = "drawings")]
        self.record_image_requests(ui);
        let range = self.visible_time_range();
        if range != self.host_events.range {
            self.host_events.range = range;
            self.host_events
                .push(ChartEvent::VisibleRangeChanged(range));
        }
        let logical_range = self.visible_logical_range();
        if logical_range != self.host_events.logical_range {
            self.host_events.logical_range = logical_range;
            self.host_events
                .push(ChartEvent::VisibleLogicalRangeChanged(logical_range));
        }
        let size = self.time_scale_size();
        if size != self.host_events.time_scale_size {
            self.host_events.time_scale_size = size;
            self.host_events.push(ChartEvent::TimeScaleResized(size));
        }
        let position = ui.input(|input| input.pointer.hover_pos()).and_then(|pos| {
            ui.rect_contains_pointer(egui::Rect::from_min_max(
                egui::pos2(self.last_rendered_price_rect.left(), ui.max_rect().top()),
                egui::pos2(
                    self.last_rendered_price_rect.right(),
                    ui.max_rect().bottom(),
                ),
            ))
            .then(|| self.pointer_at(ui, pos, self.pointer_origin(ui, pos, None)))
            .flatten()
        });
        let unchanged = match (&position, &self.host_events.pointer) {
            (Some(position), Some(previous)) => {
                // Button transitions alone do not move the cursor; changed touch details do.
                position.same_position(previous)
                    && (!matches!(position.origin, Some(egui::Event::Touch { .. }))
                        || position.origin == previous.origin)
            }
            _ => position == self.host_events.pointer,
        };
        if !unchanged {
            self.host_events.pointer = position.clone();
            self.host_events.push(ChartEvent::CrosshairMoved(position));
        }
    }

    #[cfg(feature = "drawings")]
    fn record_image_requests(&mut self, ui: &Ui) {
        use crate::drawings::DrawingToolType;

        if let Some(drawing) = self.drawings.image_request.take() {
            self.host_events
                .push(ChartEvent::ImageRequested(ImageRequest {
                    drawing,
                    file: None,
                }));
        }
        let pos = ui.input(|input| {
            for file in &input.raw.dropped_files {
                let supported = file
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| {
                        ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"]
                            .iter()
                            .any(|image| ext.eq_ignore_ascii_case(image))
                    });
                if supported && self.host_events.dropped_images.len() < 16 {
                    self.host_events.dropped_images.push(file.clone());
                }
            }
            input.pointer.hover_pos()
        });
        // shortcut: egui has no drop position; hidden charts retain files until shown with a
        // pointer. Use the drop position instead when egui supplies it.
        let Some(pos) = pos.filter(|_| !self.host_events.dropped_images.is_empty()) else {
            return;
        };
        let files = std::mem::take(&mut self.host_events.dropped_images);
        let plot = self.last_rendered_price_rect;
        if !ui.rect_contains_pointer(plot) {
            return;
        }
        let index = self
            .state
            .time_scale()
            .coord_to_idx_precise(pos.x, plot.left(), plot.width())
            .round();
        let Some(time) = crate::model::time_at(self.state.shown(), index) else {
            return;
        };
        let price = self.last_price_map.y_to_price(pos.y);
        if !price.is_finite() {
            return;
        }
        let requests = self.user_action(|chart| {
            files
                .into_iter()
                .filter_map(|file| {
                    let mut drawing = chart.drawings.new_drawing(0, DrawingToolType::Image);
                    drawing.z_order = chart.drawings.top_z_order();
                    let drawing = chart.create_drawing(drawing, &[(time, price)]).ok()?;
                    Some(ImageRequest {
                        drawing,
                        file: Some(file),
                    })
                })
                .collect::<Vec<_>>()
        });
        if let Some(request) = requests.last() {
            self.drawings.select(request.drawing);
        }
        for request in requests {
            self.host_events.push(ChartEvent::ImageRequested(request));
        }
    }

    pub(super) fn record_host_click(&mut self, ui: &Ui, response: &egui::Response) {
        self.prepare_pointer_input(ui);
        let frame = ui.ctx().cumulative_frame_nr();
        for (index, button) in [PointerButton::Primary, PointerButton::Secondary]
            .into_iter()
            .enumerate()
        {
            if response.clicked_by(button) && self.host_events.click_frames[index] != Some(frame) {
                let pos = ui.input(|input| {
                    input.raw.events.iter().rev().find_map(|event| match event {
                        egui::Event::PointerButton {
                            pos,
                            button: released,
                            pressed: false,
                            ..
                        } if *released == button => Some(*pos),
                        _ => None,
                    })
                });
                let Some(position) =
                    pos.or_else(|| response.interact_pointer_pos())
                        .and_then(|pos| {
                            self.pointer_at(ui, pos, self.pointer_origin(ui, pos, Some(button)))
                        })
                else {
                    continue;
                };
                self.host_events.click_frames[index] = Some(frame);
                self.host_events.push(ChartEvent::Click {
                    position: position.clone(),
                    button,
                });
                if button == PointerButton::Primary {
                    self.click_mark(position.pos);
                    // The second click of a double click edits the card; the first opened it.
                    #[cfg(feature = "drawings")]
                    if !response.double_clicked() && !response.triple_clicked() {
                        self.click_link(ui.ctx(), position.pos);
                    }
                    if response.double_clicked() && self.host_events.click_frames[2] != Some(frame)
                    {
                        self.host_events.click_frames[2] = Some(frame);
                        self.host_events.push(ChartEvent::DoubleClick { position });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// Placement, delayed drops and host answers retain their ids, coordinates and undo steps.
    #[cfg(feature = "drawings")]
    #[test]
    fn image_requests_reach_the_host() {
        use crate::drawings::{Drawing, DrawingToolType};
        use crate::widget::test_frames::{chart, frame};
        use egui::{Context, Event, RawInput, Rect, Vec2, pos2, vec2};
        use std::sync::Arc;

        #[derive(Debug)]
        struct File(&'static str);
        impl egui::DroppedFile for File {
            fn path(&self) -> &std::path::Path {
                self.0.as_ref()
            }
            #[cfg(not(target_arch = "wasm32"))]
            fn bytes(&self) -> Result<Vec<u8>, String> {
                Ok(Vec::new())
            }
            #[cfg(target_arch = "wasm32")]
            fn bytes_async(
                &self,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + '_>>
            {
                Box::pin(async { Ok(Vec::new()) })
            }
        }
        let requests = |chart: &mut Chart| {
            chart
                .drain_events()
                .filter_map(|event| {
                    if let ChartEvent::ImageRequested(request) = event {
                        Some(request)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        let ctx = Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        frame(&ctx, &mut chart, 0.0, vec![]);
        let map = chart.get_chart_mapping();
        let p = pos2(map.idx_to_x(30), map.price_to_y(16.0));
        chart
            .drawings_mut()
            .set_active_tool(Some(DrawingToolType::Image));
        frame(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(p)]);
        for (time, pressed) in [(0.2, true), (0.3, false)] {
            frame(
                &ctx,
                &mut chart,
                time,
                vec![Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
        let id = chart.drawings().drawings[0].id;
        assert_eq!(
            requests(&mut chart),
            [ImageRequest {
                drawing: id,
                file: None
            }]
        );
        assert_eq!(chart.drawings().text_editor.as_ref().unwrap().id, id);
        assert_eq!(
            chart.set_drawing_image(id, "test://a", vec2(1920.0, 1080.0)),
            Ok(())
        );
        let picture = &chart.drawings().drawings[0];
        assert_eq!(picture.text.as_deref(), Some("test://a"));
        assert_eq!(picture.image.unwrap().size, Some([240.0, 135.0]));
        assert!(chart.drawings().text_editor.is_none());
        assert!(chart.undo());
        assert_eq!(chart.drawings().drawings[0].text, None);
        assert_eq!(chart.drawings().drawings[0].image, None);
        for (size, expected) in [
            (vec2(100.0, 50.0), [100.0, 50.0]),
            (vec2(400.0, 200.0), [240.0, 120.0]),
            (vec2(480.0, 720.0), [120.0, 180.0]),
            (vec2(10_000.0, 1.0), [240.0, 1.0]),
        ] {
            chart.set_drawing_image(id, "test://a", size).unwrap();
            let actual = chart.drawings().drawings[0].image.unwrap().size.unwrap();
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert!((actual - expected).abs() < 1e-3);
            }
            assert!(actual[0] <= 240.0 && actual[1] <= 180.0, "{actual:?}");
            assert!(chart.undo());
        }
        let line = chart
            .create_drawing(
                Drawing::new(0, DrawingToolType::TrendLine),
                &[
                    (chart.bars(SeriesId::MAIN)[0].time, 15.0),
                    (chart.bars(SeriesId::MAIN)[1].time, 16.0),
                ],
            )
            .unwrap();
        for (target, locked, uri, size, error) in [
            (usize::MAX, false, "a", vec2(10.0, 10.0), "Unknown drawing"),
            (id, true, "a", vec2(10.0, 10.0), "Drawing is locked"),
            (
                line,
                false,
                "a",
                vec2(10.0, 10.0),
                "Drawing is not an image",
            ),
            (id, false, "  ", vec2(10.0, 10.0), "Image address is empty"),
            (
                id,
                false,
                "a",
                vec2(0.0, 10.0),
                "Image size must be positive and finite",
            ),
            (
                id,
                false,
                "a",
                vec2(f32::NAN, 1.0),
                "Image size must be positive and finite",
            ),
        ] {
            chart.drawings_mut().drawings[0].locked = locked;
            let before = chart.drawings().drawings.clone();
            assert_eq!(chart.set_drawing_image(target, uri, size), Err(error));
            assert_eq!(
                serde_json::to_value(&chart.drawings().drawings).unwrap(),
                serde_json::to_value(&before).unwrap()
            );
        }
        chart.drawings_mut().drawings.clear();
        frame(&ctx, &mut chart, 0.4, vec![Event::PointerGone]);
        let files: Vec<egui::DroppedFileHandle> = vec![
            Arc::new(File("a.PNG")),
            Arc::new(File("b.jpg")),
            Arc::new(File("notes.txt")),
        ];
        let drop_files = |chart: &mut Chart, time, dropped_files| {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                    time: Some(time),
                    dropped_files,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        chart.show(ui);
                    });
                },
            );
            output.drop_without_applying_deltas();
        };
        drop_files(&mut chart, 0.5, files.clone());
        assert!(chart.drawings().drawings.is_empty());
        assert!(requests(&mut chart).is_empty());
        frame(&ctx, &mut chart, 0.6, vec![Event::PointerMoved(p)]);
        let dropped = requests(&mut chart);
        assert_eq!(dropped.len(), 2);
        assert_eq!(chart.drawings().drawings.len(), 2);
        for (request, file) in dropped.iter().zip(&files) {
            assert!(Arc::ptr_eq(request.file.as_ref().unwrap(), file));
            let anchors = chart.drawing_points(request.drawing).unwrap();
            assert_eq!(anchors.len(), 1);
            assert_eq!(anchors[0].0.timestamp(), 1_702_592_000);
            assert!((anchors[0].1 - 16.0).abs() < 1e-6, "{anchors:?}");
        }
        assert_eq!(chart.drawings().sel_drawing(), Some(dropped[1].drawing));
        assert!(chart.undo());
        assert!(chart.drawings().drawings.is_empty());
        frame(&ctx, &mut chart, 0.7, vec![Event::PointerGone]);
        drop_files(&mut chart, 0.8, vec![files[0].clone()]);
        let rect = chart.get_rendered_price_rect();
        frame(
            &ctx,
            &mut chart,
            0.9,
            vec![Event::PointerMoved(pos2(
                rect.center().x,
                rect.bottom() + 5.0,
            ))],
        );
        assert!(chart.host_events.dropped_images.is_empty());
        frame(&ctx, &mut chart, 1.0, vec![Event::PointerMoved(p)]);
        assert!(chart.drawings().drawings.is_empty());
        assert!(requests(&mut chart).is_empty());
    }

    use super::*;
    use crate::widget::test_frames::{chart, frame};
    use egui::{Context, Event};

    /// Interleaved points occupy fractional main-series slots: minute 1 is at 0.5 between
    /// minutes 0 and 2, and minute 3 is at 1.125 between minutes 2 and 10.
    #[test]
    fn cursor_events_resolve_interleaved_series_points() {
        let start = "2024-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let bar = |minute, value| {
            let mut bar = Bar::from_value(start + chrono::TimeDelta::minutes(minute), value);
            bar.payload = Some(std::sync::Arc::new(
                serde_json::json!({ "minute": minute, "value": value }),
            ));
            bar
        };
        for pane in [false, true] {
            for main in ["valued", "whitespace", "removed"] {
                let ctx = Context::default();
                let mut chart = Chart::new(crate::model::BarData::from_bars(
                    [0, 2, 10].map(|minute| bar(minute, 100.0)).to_vec(),
                ));
                chart.config.show_volume = false;
                chart.config.show_ohlc_info = false;
                #[cfg(feature = "studies")]
                chart.add_indicator(Box::new(crate::studies::SMA::new(1)));
                let series = chart.add_series(Default::default());
                let hidden = chart.add_series(Default::default());
                chart.set_series_visible(hidden, false);
                chart.set_data(series, vec![bar(1, 120.0), bar(3, 130.0)]);
                chart.set_data(hidden, vec![bar(1, 140.0), bar(4, 125.0)]);
                if main == "whitespace" {
                    chart.set_data(
                        SeriesId::MAIN,
                        [0, 2, 10]
                            .map(|minute| Bar::whitespace(bar(minute, 100.0).time))
                            .to_vec(),
                    );
                } else if main == "removed" {
                    assert!(chart.remove_series(SeriesId::MAIN));
                }
                if pane {
                    let id = chart.add_pane();
                    assert!(chart.move_series_to_pane(series, id));
                }
                chart.state.time_scale_mut().set_bar_spacing(160.0);
                chart.state.time_scale_mut().set_right_offset(0.0);
                for time in [0.0, 0.1, 0.2] {
                    frame(&ctx, &mut chart, time, vec![]);
                }
                for (step, (whitespace, hidden_indices, minute, index)) in [
                    (false, false, 1, 0.5),
                    (true, false, 3, 1.125),
                    (false, true, 1, 0.5),
                    (true, true, 1, 0.5),
                    (false, false, 4, 1.25),
                    (true, false, 4, 1.25),
                    (true, true, 3, 1.25),
                ]
                .into_iter()
                .enumerate()
                {
                    let mut options = chart.chart_options().clone();
                    options.time_scale.ignore_whitespace_indices = whitespace;
                    options.time_scale.ignore_hidden_indices = hidden_indices;
                    chart.set_chart_options(options);
                    let time = 1.0 + step as f64;
                    frame(&ctx, &mut chart, time, vec![Event::PointerGone]);
                    chart.drain_events().for_each(drop);
                    let map = chart.get_chart_mapping();
                    let value = match minute {
                        1 => 120.0,
                        3 => 130.0,
                        _ => 125.0,
                    };
                    let pos = Pos2::new(
                        map.idx_to_x(0) + index * map.bar_spacing,
                        chart.series_price_map(series).unwrap().price_to_y(125.0),
                    );
                    frame(&ctx, &mut chart, time + 0.1, vec![Event::PointerMoved(pos)]);
                    let pointer = chart
                        .drain_events()
                        .find_map(|event| match event {
                            ChartEvent::CrosshairMoved(Some(pointer)) => Some(pointer),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("{pane} {main} {step}: missing pointer"));
                    assert_eq!(
                        pointer.time,
                        Some(bar(minute, value).time),
                        "{pane} {main} {step}"
                    );
                    assert!(pointer.bar.is_none() && pointer.studies.is_empty());
                    let selected_series = if minute == 4 { hidden } else { series };
                    let mut expected = vec![(selected_series, bar(minute, value))];
                    if minute == 1 {
                        expected.push((hidden, bar(1, 140.0)));
                    }
                    assert_eq!(pointer.series_bars, expected);
                    assert_eq!(pointer.pos, pos);
                    assert_eq!(pointer.pane, usize::from(pane));
                    frame(&ctx, &mut chart, time + 0.2, vec![]);
                    assert!(
                        !chart
                            .drain_events()
                            .any(|event| matches!(event, ChartEvent::CrosshairMoved(_)))
                    );
                    chart.update(selected_series, bar(minute, value + 1.0));
                    frame(&ctx, &mut chart, time + 0.3, vec![]);
                    assert!(chart.drain_events().any(|event| matches!(
                        event,
                        ChartEvent::CrosshairMoved(Some(updated))
                            if updated.time == pointer.time
                                && updated.series_bars[0] == (selected_series, bar(minute, value + 1.0))
                    )));
                    chart.update(selected_series, bar(minute, value));
                }
            }
        }
    }

    /// Separate inputs in one frame retain their coordinates and keys; state updates have no
    /// originating input, even with unrelated touches in that frame.
    #[test]
    fn pointer_origins_match_each_notification() {
        for (button, touch_first) in [
            (PointerButton::Primary, true),
            (PointerButton::Secondary, false),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[100.0], &[200.0]);
            chart.config.show_volume = false;
            chart.config.show_ohlc_info = false;
            chart.config.show_symbol_labels = false;
            for t in [0.0, 0.1, 0.2] {
                frame(&ctx, &mut chart, t, vec![]);
            }
            let map = chart.get_chart_mapping();
            let click_pos = Pos2::new(map.idx_to_x(60), map.price_to_y(150.0));
            let touch_pos = Pos2::new(map.idx_to_x(70), map.price_to_y(160.0));
            let unrelated_pos = Pos2::new(map.idx_to_x(80), map.price_to_y(170.0));
            // Modifiers can change while this chart is hidden.
            ctx.run_ui(
                egui::RawInput {
                    time: Some(0.25),
                    events: vec![Event::ModifiersChanged(Modifiers::SHIFT)],
                    ..Default::default()
                },
                |_| {},
            )
            .drop_without_applying_deltas();
            frame(&ctx, &mut chart, 0.3, vec![Event::PointerMoved(click_pos)]);
            assert!(chart.drain_events().any(|event| matches!(event,
                ChartEvent::CrosshairMoved(Some(pointer)) if pointer.modifiers == Modifiers::SHIFT
                    && pointer.origin == Some(Event::PointerMoved(click_pos))
            )));
            frame(
                &ctx,
                &mut chart,
                0.4,
                vec![Event::PointerButton {
                    pos: click_pos,
                    button,
                    pressed: true,
                    modifiers: Modifiers::SHIFT,
                }],
            );
            chart.drain_events().for_each(drop);
            let release = Event::PointerButton {
                pos: click_pos,
                button,
                pressed: false,
                modifiers: Modifiers::SHIFT,
            };
            let touch = Event::Touch {
                device_id: egui::TouchDeviceId(42),
                id: egui::TouchId(7),
                phase: egui::TouchPhase::Start,
                pos: touch_pos,
                force: Some(0.75),
            };
            let touch_inputs = vec![Event::ModifiersChanged(Modifiers::ALT), touch.clone()];
            let pointer_inputs = vec![
                release.clone(),
                Event::ModifiersChanged(Modifiers::CTRL),
                Event::PointerMoved(touch_pos),
            ];
            let mut inputs = if touch_first {
                [touch_inputs, pointer_inputs].concat()
            } else {
                [pointer_inputs, touch_inputs].concat()
            };
            inputs.push(Event::ModifiersChanged(Modifiers::NONE));
            frame(&ctx, &mut chart, 0.5, inputs);
            let events = chart.drain_events().collect::<Vec<_>>();
            let click = events
                .iter()
                .find_map(|event| match event {
                    ChartEvent::Click {
                        position,
                        button: b,
                    } if *b == button => Some(position),
                    _ => None,
                })
                .unwrap();
            assert_eq!((click.pos, click.index), (click_pos, 60));
            assert_eq!(
                (click.modifiers, click.kind),
                (Modifiers::SHIFT, PointerKind::Mouse)
            );
            assert_eq!(click.origin, Some(release));
            let pointer = events
                .iter()
                .find_map(|event| match event {
                    ChartEvent::CrosshairMoved(Some(pointer)) => Some(pointer),
                    _ => None,
                })
                .unwrap();
            assert_eq!((pointer.pos, pointer.index), (touch_pos, 70));
            assert_eq!(
                (pointer.modifiers, pointer.kind),
                (Modifiers::ALT, PointerKind::Touch)
            );
            assert_eq!(pointer.origin, Some(touch));

            // Pressure and phase changes at a stationary position still carry their input.
            for (step, phase) in [
                egui::TouchPhase::Move,
                egui::TouchPhase::End,
                egui::TouchPhase::Cancel,
            ]
            .into_iter()
            .enumerate()
            {
                let touch = Event::Touch {
                    device_id: egui::TouchDeviceId(42),
                    id: egui::TouchId(7),
                    phase,
                    pos: touch_pos,
                    force: Some(0.25),
                };
                frame(
                    &ctx,
                    &mut chart,
                    0.6 + step as f64 * 0.1,
                    vec![touch.clone()],
                );
                assert!(chart.drain_events().any(|event| matches!(event,
                    ChartEvent::CrosshairMoved(Some(pointer)) if pointer.origin == Some(touch.clone())
                        && pointer.pos == touch_pos && pointer.modifiers == Modifiers::NONE
                )));
            }
            frame(
                &ctx,
                &mut chart,
                0.82,
                vec![
                    Event::Touch {
                        device_id: egui::TouchDeviceId(42),
                        id: egui::TouchId(7),
                        phase: egui::TouchPhase::Start,
                        pos: touch_pos,
                        force: Some(0.5),
                    },
                    Event::PointerButton {
                        pos: touch_pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
            chart.drain_events().for_each(drop);
            let lift = Event::Touch {
                device_id: egui::TouchDeviceId(42),
                id: egui::TouchId(7),
                phase: egui::TouchPhase::End,
                pos: touch_pos,
                force: Some(0.125),
            };
            frame(
                &ctx,
                &mut chart,
                0.84,
                vec![
                    Event::PointerButton {
                        pos: touch_pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    },
                    lift.clone(),
                    Event::Touch {
                        device_id: egui::TouchDeviceId(99),
                        id: egui::TouchId(99),
                        phase: egui::TouchPhase::End,
                        pos: unrelated_pos,
                        force: None,
                    },
                ],
            );
            assert!(chart.drain_events().any(|event| matches!(event,
                ChartEvent::Click { position, button: PointerButton::Primary }
                    if position.origin == Some(lift.clone()) && position.pos == touch_pos
                        && position.kind == PointerKind::Touch
            )));
            let mut bars = chart.bars(SeriesId::MAIN).to_vec();
            bars[70].payload = Some(std::sync::Arc::new(serde_json::json!({ "revision": 1 })));
            chart.set_data(SeriesId::MAIN, bars);
            chart.set_crosshair_position(Some((chart.bars(SeriesId::MAIN)[50].time, 125.0)));
            frame(
                &ctx,
                &mut chart,
                0.9,
                vec![Event::Touch {
                    device_id: egui::TouchDeviceId(99),
                    id: egui::TouchId(99),
                    phase: egui::TouchPhase::Start,
                    pos: unrelated_pos,
                    force: Some(1.0),
                }],
            );
            assert!(chart.drain_events().any(|event| matches!(event,
                ChartEvent::CrosshairMoved(Some(pointer)) if pointer.origin.is_none()
                    && pointer.pos == touch_pos && pointer.index == 70
            )));
            frame(&ctx, &mut chart, 1.0, vec![]);
            assert!(
                !chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::CrosshairMoved(_)))
            );
            frame(&ctx, &mut chart, 1.1, vec![Event::PointerGone]);
            chart.drain_events().for_each(drop);
            chart.set_crosshair_position(Some((chart.bars(SeriesId::MAIN)[55].time, 130.0)));
            frame(&ctx, &mut chart, 1.2, vec![]);
            assert!(
                !chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::CrosshairMoved(_)))
            );
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn cursor_data_policy_includes_hidden_values_without_changing_the_chart() {
        use crate::studies::{CustomIndicator, IndicatorValue};
        use crate::widget::test_frames::shapes;

        let ctx = Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        chart.config.show_volume = false;
        let mut ids = Vec::new();
        for _ in 0..2 {
            // Equal names still identify two different studies. The second plot's value
            // is 100 * 20 = 2000, far outside the visible bars' 10..20 price range.
            let id = chart.add_indicator(Box::new(
                CustomIndicator::new("Pair", |bars| {
                    bars.iter()
                        .enumerate()
                        .map(|(index, bar)| match index {
                            72 => IndicatorValue::None,
                            71 => IndicatorValue::Multiple(vec![bar.close, f64::NAN]),
                            _ => IndicatorValue::Multiple(vec![bar.close, bar.close * 100.0]),
                        })
                        .collect()
                })
                .with_line_count(2)
                .with_line_names(vec!["close".into(), "scaled".into()]),
            ));
            let study = chart.indicator_mut(id).unwrap();
            let mut settings = study.settings();
            settings.plots[1].visible = false;
            study.apply_settings(&[], settings).unwrap();
            ids.push(id);
        }
        chart.set_indicator_visible(ids[1], false);
        for time in [0.0, 0.1, 0.2] {
            frame(&ctx, &mut chart, time, vec![]);
        }
        chart.set_visible_logical_range(crate::model::LogicalRange::new(60.0, 115.0));
        frame(&ctx, &mut chart, 0.3, vec![]);
        for (step, index) in [70, 71, 72, 110].into_iter().enumerate() {
            let time = 1.0 + step as f64;
            let pos = Pos2::new(
                chart.get_chart_mapping().idx_to_x(index),
                chart.last_rendered_price_rect.center().y,
            );
            frame(&ctx, &mut chart, time, vec![Event::PointerMoved(pos)]);
            let painted = shapes(&ctx, &mut chart, time + 0.1, vec![]);
            let readout = chart.study_values_at(index);
            let range = chart.state.price_scale().range();
            let visible = readout
                .iter()
                .cloned()
                .map(|row| (ids[0], row))
                .collect::<Vec<_>>();
            assert_eq!(chart.host_events.pointer.as_ref().unwrap().studies, visible);
            chart.drain_events().for_each(drop);

            chart.set_cursor_data_policy(CursorDataPolicy::IncludeHidden);
            let included = shapes(&ctx, &mut chart, time + 0.2, vec![]);
            assert_eq!(included, painted, "painted shapes at {index}");
            assert_eq!(chart.state.price_scale().range(), range);
            assert_eq!(chart.study_values_at(index), readout);
            let pointer = chart.host_events.pointer.clone().unwrap();
            let actual = pointer
                .studies
                .iter()
                .map(|(id, row)| (*id, row.plot.as_str(), row.value))
                .collect::<Vec<_>>();
            let expected = if index == 110 {
                Vec::new()
            } else {
                ids.iter()
                    .flat_map(|&id| {
                        [
                            (id, "close", (index != 72).then_some(20.0)),
                            (id, "scaled", (index == 70).then_some(2000.0)),
                        ]
                    })
                    .collect()
            };
            assert_eq!(actual, expected, "cursor values at {index}");
            let events = chart.drain_events().collect::<Vec<_>>();
            assert_eq!(
                events,
                if index == 110 {
                    vec![]
                } else {
                    vec![ChartEvent::CrosshairMoved(Some(pointer.clone()))]
                }
            );
            frame(&ctx, &mut chart, time + 0.3, vec![]);
            assert_eq!(chart.drain_events().count(), 0);
            for pressed in [true, false] {
                frame(
                    &ctx,
                    &mut chart,
                    time + 0.4,
                    vec![Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    }],
                );
            }
            assert!(chart.drain_events().any(|event| matches!(
                event, ChartEvent::Click { position, .. } if position.studies == pointer.studies
            )));
            chart.set_cursor_data_policy(CursorDataPolicy::Visible);
            frame(&ctx, &mut chart, time + 0.5, vec![]);
            assert_eq!(chart.host_events.pointer.as_ref().unwrap().studies, visible);
        }
    }

    /// Plot positions survive missing history; only stored slots supply a data time.
    #[test]
    fn pointer_events_keep_positions_without_data_times() {
        for (empty, filtered) in [(false, false), (true, false), (true, true)] {
            for secondary in [false, true] {
                let ctx = Context::default();
                let mut chart = chart(&[100.0], &[200.0]);
                let mut bars = chart.bars(SeriesId::MAIN)[..3].to_vec();
                bars[1] = Bar::whitespace(bars[1].time);
                bars[2].time += chrono::Duration::days(3);
                if empty {
                    bars.clear();
                }
                chart.set_data(SeriesId::MAIN, bars.clone());
                chart.config.show_volume = false;
                chart.config.show_ohlc_info = false;
                chart.config.show_symbol_labels = false;
                chart.chart_options.time_scale.ignore_whitespace_indices = filtered;
                chart.chart_options.time_scale.ignore_hidden_indices = filtered;
                let owner = if secondary {
                    let series = chart.add_series(Default::default());
                    chart.set_data(series, bars.clone());
                    let pane = chart.add_pane();
                    assert!(chart.move_series_to_pane(series, pane));
                    series
                } else {
                    SeriesId::MAIN
                };
                for time in [0.0, 0.1, 0.2] {
                    frame(&ctx, &mut chart, time, vec![]);
                }
                if !empty {
                    assert!(
                        chart
                            .set_visible_logical_range(crate::model::LogicalRange::new(-2.0, 5.0,))
                    );
                    frame(&ctx, &mut chart, 0.3, vec![]);
                }
                chart.drain_events().for_each(drop);
                for (step, index) in [-1, 0, 1, 2, 3].into_iter().enumerate() {
                    let map = chart.get_chart_mapping();
                    let y = if secondary {
                        chart.pane_layout.panes[1].rect.center().y
                    } else {
                        200.0
                    };
                    let x = if empty {
                        300.0 + step as f32 * 10.0
                    } else {
                        map.idx_to_x(0) + index as f32 * map.bar_spacing
                    };
                    let pos = egui::pos2(x, y);
                    let time = 1.0 + step as f64;
                    frame(&ctx, &mut chart, time, vec![Event::PointerMoved(pos)]);
                    let pointer = chart
                        .drain_events()
                        .find_map(|event| match event {
                            ChartEvent::CrosshairMoved(pointer) => pointer,
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("missing position: {empty} {secondary} {index}"));
                    let bar = usize::try_from(index).ok().and_then(|i| bars.get(i));
                    assert_eq!(pointer.time, bar.map(|bar| bar.time));
                    assert_eq!(
                        (pointer.pos, pointer.pane, pointer.series),
                        (pos, usize::from(secondary), owner)
                    );
                    assert!(pointer.price.is_finite());
                    assert_eq!(
                        pointer.bar.as_ref().map(|bar| bar.time),
                        bar.map(|bar| bar.time)
                    );
                    assert_eq!(
                        pointer.bar.as_ref().map(Bar::is_whitespace),
                        bar.map(Bar::is_whitespace)
                    );
                    assert_eq!(
                        pointer.series_bars.len(),
                        usize::from(secondary && bar.is_some())
                    );
                    if !empty {
                        assert_eq!(pointer.index, index);
                    }
                    for button in [PointerButton::Primary, PointerButton::Secondary] {
                        let mut expected = pointer.clone();
                        expected.origin = Some(Event::PointerButton {
                            pos,
                            button,
                            pressed: false,
                            modifiers: Modifiers::NONE,
                        });
                        for pressed in [true, false] {
                            frame(
                                &ctx,
                                &mut chart,
                                time + 0.1,
                                vec![Event::PointerButton {
                                    pos,
                                    button,
                                    pressed,
                                    modifiers: Modifiers::NONE,
                                }],
                            );
                        }
                        assert!(
                            chart.drain_events().any(|event| event
                                == ChartEvent::Click {
                                    position: expected.clone(),
                                    button,
                                }),
                            "missing click: {empty} {secondary} {index}"
                        );
                    }
                    frame(&ctx, &mut chart, time + 0.2, vec![]);
                    assert!(
                        !chart
                            .drain_events()
                            .any(|event| matches!(event, ChartEvent::CrosshairMoved(_)))
                    );
                }
                frame(&ctx, &mut chart, 7.0, vec![Event::PointerGone]);
                assert!(
                    chart
                        .drain_events()
                        .any(|event| event == ChartEvent::CrosshairMoved(None))
                );
            }
        }
    }

    /// Direct operations keep their kind and order, including unchanged updates, while compact
    /// events still coalesce. Rejections stay silent and overflow reports every lost entry.
    #[test]
    fn data_operation_notifications_keep_accepted_setter_order() {
        let start = "2024-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let bar = |minute, price| {
            Bar::new(
                start + chrono::TimeDelta::minutes(minute),
                price,
                price,
                price,
                price,
                1.0,
            )
        };
        for secondary in [false, true] {
            for replay in [false, true] {
                for enabled in [false, true] {
                    let mut chart = Chart::new(crate::model::BarData::new());
                    let series = if secondary {
                        chart.add_series(Default::default())
                    } else {
                        SeriesId::MAIN
                    };
                    if replay {
                        chart.set_replay_time(Some(bar(2, 1.0).time));
                    }
                    chart.drain_events().for_each(drop);
                    if enabled {
                        chart.set_data_operation_notifications(true);
                    }
                    chart.set_data(series, vec![bar(0, 10.0), bar(2, 20.0)]);
                    chart.update(series, bar(3, 30.0));
                    chart.update(series, bar(3, 30.0));
                    chart.update(series, bar(0, 11.0));
                    chart.update(series, bar(1, 15.0));
                    chart.update(series, bar(4, f64::INFINITY));
                    chart.update(SeriesId(usize::MAX), bar(4, 40.0));
                    chart.set_data(SeriesId(usize::MAX), vec![bar(4, 40.0)]);
                    assert_eq!(chart.feed_state(series).unwrap().rejected_bars, 1);
                    let events = chart.drain_events().collect::<Vec<_>>();
                    let operations = events
                        .iter()
                        .filter(|event| matches!(event, ChartEvent::DataOperation { .. }))
                        .cloned()
                        .collect::<Vec<_>>();
                    let operation = |operation, bars| ChartEvent::DataOperation {
                        series,
                        operation,
                        bars,
                    };
                    let expected = if enabled {
                        vec![
                            operation(DataOperation::SetData, if replay { 1 } else { 2 }),
                            operation(
                                DataOperation::Append(bar(3, 30.0).time),
                                if replay { 1 } else { 3 },
                            ),
                            operation(
                                DataOperation::Update(bar(3, 30.0).time),
                                if replay { 1 } else { 3 },
                            ),
                            operation(DataOperation::Update(start), if replay { 1 } else { 3 }),
                            operation(
                                DataOperation::Insert(bar(1, 15.0).time),
                                if replay { 2 } else { 4 },
                            ),
                        ]
                    } else {
                        Vec::new()
                    };
                    assert_eq!(
                        operations, expected,
                        "secondary={secondary}, replay={replay}, enabled={enabled}"
                    );
                    let compact = events
                        .into_iter()
                        .filter(|event| !matches!(event, ChartEvent::DataOperation { .. }))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        compact,
                        [
                            ChartEvent::DataLoaded {
                                series,
                                bars: if replay { 1 } else { 2 }
                            },
                            ChartEvent::DataUpdated {
                                series,
                                bars: if replay { 2 } else { 4 }
                            },
                        ]
                    );
                    chart.set_data(series, Vec::new());
                    chart.update(series, bar(0, 10.0));
                    chart.set_data_operation_notifications(false);
                    let events = chart.drain_events().collect::<Vec<_>>();
                    assert_eq!(events.len(), if enabled { 4 } else { 2 });
                    if enabled {
                        assert_eq!(events[1], operation(DataOperation::SetData, 0));
                        assert_eq!(events[3], operation(DataOperation::Append(start), 1));
                    }
                    chart.update(series, bar(0, 10.0));
                    assert_eq!(chart.drain_events().count(), 0);
                    chart.set_replay_time(None);
                    chart.set_data_operation_notifications(true);
                    for _ in 0..1100 {
                        chart.update(series, bar(0, 10.0));
                    }
                    let events = chart.drain_events().collect::<Vec<_>>();
                    assert_eq!(events.len(), 1025);
                    assert_eq!(events[0], ChartEvent::EventsDropped(76));
                    assert!(
                        events[1..]
                            .iter()
                            .all(|event| *event == operation(DataOperation::Update(start), 1))
                    );
                }
            }
        }
    }

    /// Hosts receive changes once, retain setter order, and get UTC and raw prices rather
    /// than scale coordinates. Polling selection or draining twice cannot consume a click.
    #[test]
    fn host_events_report_changes_and_plot_coordinates_once() {
        let ctx = Context::default();
        let mut chart = chart(&[100.0], &[200.0]);
        assert_eq!(chart.drain_events().count(), 0);
        for i in 0..1100 {
            chart.set_symbol(i.to_string());
        }
        let events = chart.drain_events().collect::<Vec<_>>();
        assert_eq!(events.len(), 1025);
        assert_eq!(events[0], ChartEvent::EventsDropped(76));
        assert_eq!(
            events.last(),
            Some(&ChartEvent::SymbolChanged("1099".into()))
        );
        chart.set_symbol("A");
        chart.set_symbol("A");
        chart.set_timeframe(crate::model::Timeframe::Day1);
        chart.set_timeframe(crate::model::Timeframe::Day1);
        let mut bars = chart.bars(SeriesId::MAIN).to_vec();
        // A payload comes back with its bar, and a whitespace bar keeps its slot and is skipped
        // for the previous close.
        bars[70].payload = Some(std::sync::Arc::new(serde_json::json!({ "id": 70 })));
        bars[69] = Bar::whitespace(bars[69].time);
        chart.set_data(SeriesId::MAIN, bars.clone());
        chart.update(SeriesId::MAIN, bars[99].clone());
        assert_eq!(
            chart.drain_events().collect::<Vec<_>>(),
            vec![
                ChartEvent::SymbolChanged("A".into()),
                ChartEvent::TimeframeChanged(crate::model::Timeframe::Day1),
                ChartEvent::DataLoaded {
                    series: SeriesId::MAIN,
                    bars: 100
                },
            ]
        );
        chart.config.show_volume = false;
        chart.config.show_ohlc_info = false;
        chart.config.show_symbol_labels = false;
        for t in [0.0, 0.1, 0.2] {
            frame(&ctx, &mut chart, t, vec![]);
        }
        assert!(
            chart
                .drain_events()
                .any(|event| matches!(event, ChartEvent::VisibleRangeChanged(Some(_))))
        );
        frame(&ctx, &mut chart, 0.3, vec![]);
        assert_eq!(chart.drain_events().count(), 0);
        // With a manual linear 100..200 range, a quarter of the way from bottom is 125.
        // The same fraction on an inverted map is 175; on a logarithmic map it is 100*2^0.25.
        // Bars open at their low of 100 and close at their high of 200, so the magnet takes
        // the nearer 100 and the close magnet 200.
        use crate::config::CrosshairMode;
        for (mode, inverted, crosshair, price) in [
            (
                crate::scales::PriceScaleMode::Normal,
                false,
                CrosshairMode::Normal,
                125.0,
            ),
            (
                crate::scales::PriceScaleMode::Normal,
                true,
                CrosshairMode::Normal,
                175.0,
            ),
            (
                crate::scales::PriceScaleMode::Logarithmic,
                false,
                CrosshairMode::Normal,
                100.0 * 2.0_f64.powf(0.25),
            ),
            (
                crate::scales::PriceScaleMode::Normal,
                false,
                CrosshairMode::Magnet,
                100.0,
            ),
            (
                crate::scales::PriceScaleMode::Normal,
                false,
                CrosshairMode::MagnetClose,
                200.0,
            ),
        ] {
            chart.set_crosshair_mode(crosshair);
            let scale = chart.state.price_scale_mut();
            scale.options_mut().mode = mode;
            scale.options_mut().invert_scale = inverted;

            let transform = crate::scales::PriceTransform { mode, base: 0.0 };
            scale.rebase(transform);
            scale.set_range(transform.to_scale(100.0), transform.to_scale(200.0));
            frame(&ctx, &mut chart, 1.0, vec![]);
            chart.drain_events().for_each(drop);
            let rect = chart.last_rendered_price_rect;
            let pos = Pos2::new(
                chart.get_chart_mapping().idx_to_x(70),
                rect.bottom() - rect.height() * 0.25,
            );
            frame(&ctx, &mut chart, 1.1, vec![Event::PointerMoved(pos)]);
            let events = chart.drain_events().collect::<Vec<_>>();
            let Some(ChartEvent::CrosshairMoved(Some(pointer))) = events.last() else {
                panic!("{events:?}")
            };
            assert_eq!(pointer.time, Some(bars[70].time));
            assert_eq!(pointer.series, SeriesId::MAIN);
            assert!(
                (pointer.price - price).abs() < 0.001,
                "{} != {price}",
                pointer.price
            );
            // Each candle spans 100..200, so the pointer is over the main series.
            assert_eq!((pointer.index, pointer.pos, pointer.pane), (70, pos, 0));
            assert_eq!(pointer.bar.as_ref(), Some(&bars[70]));
            assert_eq!(pointer.previous_close, Some(bars[68].close));
            assert!(pointer.series_bars.is_empty() && pointer.studies.is_empty());
            assert_eq!(
                pointer.target,
                Some(PointerTarget::Element(ChartElementId::Series(
                    SeriesId::MAIN
                )))
            );
            assert_eq!(
                (pointer.modifiers, pointer.kind),
                (Modifiers::NONE, PointerKind::Mouse)
            );
            let expected = pointer.clone();
            frame(&ctx, &mut chart, 1.2, vec![]);
            assert_eq!(chart.drain_events().count(), 0);
            for button in [PointerButton::Primary, PointerButton::Secondary] {
                let mut expected = expected.clone();
                expected.origin = Some(Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                });
                for pressed in [true, false] {
                    frame(
                        &ctx,
                        &mut chart,
                        1.3,
                        vec![Event::PointerButton {
                            pos,
                            button,
                            pressed,
                            modifiers: Default::default(),
                        }],
                    );
                }
                assert!(chart.drain_events().any(|event| event
                    == ChartEvent::Click {
                        position: expected.clone(),
                        button
                    }));
                assert_eq!(chart.drain_events().count(), 0);
            }
            frame(&ctx, &mut chart, 1.4, vec![Event::PointerGone]);
            assert!(
                chart
                    .drain_events()
                    .any(|event| event == ChartEvent::CrosshairMoved(None))
            );
        }
        chart.set_crosshair_mode(CrosshairMode::Normal);
        #[cfg(feature = "studies")]
        {
            let id = chart.add_indicator(Box::new(crate::studies::RSI::default()));
            chart.sync_panes();
            let pane_id = chart.series_pane(id).unwrap();
            for (above, maximized) in [(false, false), (false, true), (true, true), (true, false)] {
                chart.move_pane(pane_id, if above { 0 } else { 1 });
                chart.pane_layout.maximized = maximized.then_some(pane_id);
                frame(&ctx, &mut chart, 1.5, vec![]);
                chart.drain_events().for_each(drop);
                let pane = &chart.last_rendered_indicator_panes[0];
                let pos = Pos2::new(pane.mapping.idx_to_x(70), pane.chart_rect.center().y);
                frame(&ctx, &mut chart, 1.6, vec![Event::PointerMoved(pos)]);
                let events = chart.drain_events().collect::<Vec<_>>();
                let Some(ChartEvent::CrosshairMoved(Some(pointer))) = events.last() else {
                    panic!("above={above}, maximized={maximized}: {events:?}")
                };
                assert_eq!(pointer.series, id);
                assert_eq!(pointer.time, Some(bars[70].time));
                assert!((pointer.price - 50.0).abs() < 0.001);
                assert_eq!(pointer.pane, usize::from(!above));
                assert_eq!(
                    pointer.studies,
                    chart
                        .study_values_at(70)
                        .into_iter()
                        .map(|row| (id, row))
                        .collect::<Vec<_>>()
                );
                let mut expected = pointer.clone();
                expected.origin = Some(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                });
                for pressed in [true, false] {
                    frame(
                        &ctx,
                        &mut chart,
                        1.7,
                        vec![Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        }],
                    );
                }
                let events = chart.drain_events().collect::<Vec<_>>();
                assert!(
                    events.iter().any(|event| *event
                        == ChartEvent::Click {
                            position: expected.clone(),
                            button: PointerButton::Primary
                        }),
                    "above={above}, maximized={maximized}: {events:?}, expected {expected:?}"
                );
                frame(&ctx, &mut chart, 1.75, vec![Event::PointerGone]);
            }
            chart.move_pane(pane_id, 1);
            let other = chart.add_indicator(Box::new(
                crate::studies::CustomIndicator::new("Other", |bars| {
                    bars.iter()
                        .map(|_| crate::studies::IndicatorValue::Single(75.0))
                        .collect()
                })
                .with_overlay(false),
            ));
            assert!(chart.move_indicator_to_pane(other, id));
            chart.set_crosshair_mode(crate::config::CrosshairMode::Magnet);
            frame(&ctx, &mut chart, 1.8, vec![Event::PointerGone]);
            chart.drain_events().for_each(drop);
            let pane = &chart.last_rendered_indicator_panes[0];
            let pos = Pos2::new(pane.mapping.idx_to_x(70), pane.mapping.price_to_y(74.0));
            frame(&ctx, &mut chart, 1.9, vec![Event::PointerMoved(pos)]);
            let events = chart.drain_events().collect::<Vec<_>>();
            assert!(events.iter().any(|event| matches!(event, ChartEvent::CrosshairMoved(Some(point)) if point.series == id && (point.price - 75.0).abs() < 0.001)), "{events:?}");
            let pane = &chart.last_rendered_indicator_panes[0];
            let pos = Pos2::new(pane.mapping.idx_to_x(70), pane.mapping.price_to_y(75.0));
            frame(&ctx, &mut chart, 1.95, vec![Event::PointerMoved(pos)]);
            let events = chart.drain_events().collect::<Vec<_>>();
            assert!(events.iter().any(|event| matches!(event, ChartEvent::CrosshairMoved(Some(point)) if point.target == Some(PointerTarget::Element(ChartElementId::Indicator(other))))), "{events:?}");
            // The pane below the merged pair is the second pane, not the third study, and a pane
            // of bar series counts as one, its pointer snapping to their closes.
            let series = chart.add_series(crate::widget::SeriesOptions::default());
            chart.set_data(series, bars.clone());
            let series_pane = chart.add_pane();
            assert!(chart.move_series_to_pane(series, series_pane));
            let below = chart.add_indicator(Box::new(crate::studies::RSI::default()));
            frame(&ctx, &mut chart, 1.96, vec![Event::PointerGone]);
            chart.drain_events().for_each(drop);
            let map = chart.series_price_map(series).unwrap();
            let pos = Pos2::new(
                chart.get_chart_mapping().idx_to_x(70),
                map.price_to_y(bars[70].close) + 2.0,
            );
            assert!(chart.pane_layout.panes[2].rect.contains(pos));
            frame(&ctx, &mut chart, 1.965, vec![Event::PointerMoved(pos)]);
            let events = chart.drain_events().collect::<Vec<_>>();
            assert!(events.iter().any(|event| matches!(event, ChartEvent::CrosshairMoved(Some(point)) if point.series == series && point.pane == 2 && point.price == bars[70].close)), "{events:?}");
            let pane = chart.last_rendered_indicator_panes.last().unwrap();
            let pos = Pos2::new(pane.mapping.idx_to_x(70), pane.chart_rect.center().y);
            frame(&ctx, &mut chart, 1.97, vec![Event::PointerMoved(pos)]);
            let events = chart.drain_events().collect::<Vec<_>>();
            assert!(events.iter().any(|event| matches!(event, ChartEvent::CrosshairMoved(Some(point)) if point.series == below && point.pane == 3)), "{events:?}");
            // Panes count from the top as drawn, so the price pane moved to the bottom is fourth.
            assert!(chart.move_pane(super::super::PaneId::MAIN, usize::MAX));
            frame(&ctx, &mut chart, 1.98, vec![Event::PointerGone]);
            chart.drain_events().for_each(drop);
            let pos = chart.last_rendered_price_rect.center();
            frame(&ctx, &mut chart, 1.99, vec![Event::PointerMoved(pos)]);
            let events = chart.drain_events().collect::<Vec<_>>();
            assert!(events.iter().any(|event| matches!(event, ChartEvent::CrosshairMoved(Some(point)) if point.series == SeriesId::MAIN && point.pane == 3)), "{events:?}");
            chart.remove_series(series);
            chart.remove_series(below);
            chart.remove_series(other);
            chart.remove_series(id);
            chart.set_crosshair_mode(crate::config::CrosshairMode::Normal);
            frame(&ctx, &mut chart, 2.0, vec![Event::PointerGone]);
            chart.drain_events().for_each(drop);
        }
        chart.config.show_volume = true;
        frame(&ctx, &mut chart, 2.1, vec![]);
        chart.drain_events().for_each(drop);
        let volume = chart.last_rendered_volume_rect;
        let pos = Pos2::new(chart.get_chart_mapping().idx_to_x(70), volume.center().y);
        for (step, (mode, price)) in [
            (crate::config::CrosshairMode::Normal, 0.5),
            (crate::config::CrosshairMode::Magnet, 1.0),
        ]
        .into_iter()
        .enumerate()
        {
            chart.set_crosshair_mode(mode);
            frame(
                &ctx,
                &mut chart,
                2.2 + step as f64,
                vec![Event::PointerMoved(pos)],
            );
            chart.drain_events().for_each(drop);
            for pressed in [true, false] {
                frame(
                    &ctx,
                    &mut chart,
                    2.3 + step as f64,
                    vec![Event::PointerButton {
                        pos,
                        button: PointerButton::Secondary,
                        pressed,
                        modifiers: Default::default(),
                    }],
                );
            }
            assert!(chart.drain_events().any(|event| matches!(event, ChartEvent::Click { position, button: PointerButton::Secondary } if position.series == SeriesId::VOLUME && (position.price - price).abs() < 0.001)));
        }
        frame(&ctx, &mut chart, 3.5, vec![Event::PointerGone]);
        chart.drain_events().for_each(drop);
        // Added series, drawings, touch input and held modifiers reach the payload.
        let series = chart.add_series(Default::default());
        chart.set_data(series, bars.clone());
        assert!(chart.set_series_visible(series, false));
        #[cfg(feature = "drawings")]
        {
            let mut line = crate::drawings::Drawing::with_color(
                1,
                crate::drawings::DrawingToolType::HorizontalLine,
                [255, 0, 0, 255],
            );
            line.chart_points = vec![crate::drawings::ChartPoint::new(50.0, 150.0)];
            chart.drawings_mut().drawings.push(line);
        }
        frame(&ctx, &mut chart, 3.6, vec![]);
        chart.drain_events().for_each(drop);
        let map = chart.get_chart_mapping();
        let pos = Pos2::new(map.idx_to_x(70), map.price_to_y(150.0));
        frame(
            &ctx,
            &mut chart,
            3.7,
            vec![
                Event::ModifiersChanged(Modifiers::SHIFT),
                Event::Touch {
                    device_id: egui::TouchDeviceId(0),
                    id: egui::TouchId(0),
                    phase: egui::TouchPhase::Start,
                    pos,
                    force: None,
                },
                Event::PointerMoved(pos),
            ],
        );
        let events = chart.drain_events().collect::<Vec<_>>();
        let Some(ChartEvent::CrosshairMoved(Some(pointer))) = events.last() else {
            panic!("{events:?}")
        };
        assert_eq!(pointer.series_bars, vec![(series, bars[70].clone())]);
        #[cfg(feature = "drawings")]
        assert_eq!(pointer.target, Some(PointerTarget::Drawing(1)));
        assert_eq!(
            (pointer.modifiers, pointer.kind),
            (Modifiers::SHIFT, PointerKind::Touch)
        );
        for kind in [PointerKind::Pen, PointerKind::Mouse] {
            chart.set_pointer_kind(Some(kind));
            frame(&ctx, &mut chart, 3.75, vec![Event::PointerMoved(pos)]);
            assert!(chart.drain_events().any(|event| matches!(event,
                ChartEvent::CrosshairMoved(Some(pointer)) if pointer.kind == kind
                    && pointer.series_bars == vec![(series, bars[70].clone())]
            )));
        }
        chart.set_pointer_kind(None);
        let mut empty = bars.clone();
        empty[70] = Bar::whitespace(bars[70].time);
        empty[70].payload = Some(std::sync::Arc::new(serde_json::json!({ "id": "empty" })));
        chart.set_data(SeriesId::MAIN, empty.clone());
        chart.set_data(series, empty.clone());
        frame(&ctx, &mut chart, 3.76, vec![Event::PointerMoved(pos)]);
        let pointer = chart
            .drain_events()
            .find_map(|event| match event {
                ChartEvent::CrosshairMoved(Some(pointer)) => Some(pointer),
                _ => None,
            })
            .unwrap();
        assert!(pointer.bar.as_ref().unwrap().is_whitespace());
        assert!(pointer.series_bars[0].1.is_whitespace());
        assert_eq!(pointer.series_bars[0].1.payload, empty[70].payload);
        assert_eq!(pointer, pointer.clone());
        frame(&ctx, &mut chart, 3.77, vec![]);
        assert!(
            !chart
                .drain_events()
                .any(|event| matches!(event, ChartEvent::CrosshairMoved(_)))
        );
        empty[70].payload = Some(std::sync::Arc::new(serde_json::json!({ "id": "changed" })));
        chart.set_data(series, empty);
        frame(&ctx, &mut chart, 3.78, vec![]);
        assert!(chart.drain_events().any(|event| matches!(event,
            ChartEvent::CrosshairMoved(Some(updated)) if updated != pointer
        )));
        chart.set_data(SeriesId::MAIN, bars.clone());
        chart.remove_series(series);
        #[cfg(feature = "drawings")]
        chart.drawings_mut().drawings.clear();
        frame(
            &ctx,
            &mut chart,
            3.8,
            vec![Event::ModifiersChanged(Modifiers::NONE), Event::PointerGone],
        );
        chart.drain_events().for_each(drop);
        // The second of two quick clicks follows its click with a double click at the same point.
        for (step, pressed) in [true, false, true, false].into_iter().enumerate() {
            frame(
                &ctx,
                &mut chart,
                3.95 + step as f64 * 0.01,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        let events = chart.drain_events().collect::<Vec<_>>();
        let [
            ..,
            ChartEvent::Click { position, .. },
            ChartEvent::DoubleClick { position: second },
        ] = events.as_slice()
        else {
            panic!("{events:?}")
        };
        assert_eq!((position.index, second), (70, position));
        let doubles = events
            .iter()
            .filter(|event| matches!(event, ChartEvent::DoubleClick { .. }));
        assert_eq!(doubles.count(), 1);
        frame(&ctx, &mut chart, 3.99, vec![Event::PointerGone]);
        chart.drain_events().for_each(drop);
        let range = chart.visible_time_range().unwrap();
        chart.apply_synced_time_range(VisibleTimeRange {
            from: range.from + chrono::Duration::days(1),
            to: range.to + chrono::Duration::days(1),
        });
        frame(&ctx, &mut chart, 4.0, vec![]);
        assert!(
            chart
                .drain_events()
                .any(|event| matches!(event, ChartEvent::VisibleRangeChanged(Some(_))))
        );
        chart.set_data(SeriesId::MAIN, vec![]);
        frame(&ctx, &mut chart, 4.1, vec![]);
        assert_eq!(
            chart.drain_events().collect::<Vec<_>>(),
            vec![
                ChartEvent::DataLoaded {
                    series: SeriesId::MAIN,
                    bars: 0
                },
                ChartEvent::VisibleRangeChanged(None),
                ChartEvent::VisibleLogicalRangeChanged(None),
            ]
        );
    }
}
