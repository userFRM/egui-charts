//! Host notifications from chart setters, history loads and rendered interaction.

use super::{Chart, ChartElementId, SeriesId, VisibleTimeRange};
use crate::chart::renderers::StudyValue;
use crate::model::Bar;
use chrono::{DateTime, Utc};
use egui::{Modifiers, PointerButton, Pos2, Ui};

/// A chart cursor position with the data a host legend, tooltip or context action reads.
/// Identical whitespace slots compare equal, including their colours and host payloads.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ChartPointer {
    /// Nearest slot's timestamp, extrapolated into whitespace as in `visible_time_range`.
    pub time: DateTime<Utc>,
    /// Price on the main pane, or the value on the scale of the hovered pane or volume.
    pub price: f64,
    /// `MAIN`, `VOLUME`, or the first series or study of another hovered pane. An empty pane
    /// reports no position.
    pub series: SeriesId,
    /// Nearest slot in the rendered series, counting elements on price-based chart types.
    /// Negative or past the last bar in whitespace.
    pub index: isize,
    /// Pointer position in UI points, the coordinates of the chart's `Response::rect`.
    pub pos: Pos2,
    /// Pane counted from the top among the panes drawn, the price pane with its volume band
    /// included: `0` unless panes sit above the price pane.
    pub pane: usize,
    /// Main series bar, or price-based element, at `index`, with its colours and payload: a
    /// [whitespace](Bar::whitespace) bar in an empty slot, `None` in the whitespace before the
    /// first bar or after the last.
    pub bar: Option<Bar>,
    /// Close of the last bar or element with a value before `index`, from which legends measure
    /// the change.
    pub previous_close: Option<f64>,
    /// Every series added with `add_series` or `add_comparison` that has a bar at `time`,
    /// including hidden series and their host payloads.
    pub series_bars: Vec<(SeriesId, Bar)>,
    /// Visible study plots at `index`, as [`Chart::study_values_at`] returns them, each with the
    /// id of its study.
    pub studies: Vec<(SeriesId, StudyValue)>,
    /// The drawing, series or study line under the pointer, in the priority a click selects.
    pub target: Option<PointerTarget>,
    /// Modifier keys held during the event.
    pub modifiers: Modifiers,
    /// Device that produced the event.
    pub kind: PointerKind,
}

impl PartialEq for ChartPointer {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
            && self.price == other.price
            && self.series == other.series
            && self.index == other.index
            && self.pos == other.pos
            && self.pane == other.pane
            && self.previous_close == other.previous_close
            && self.studies == other.studies
            && self.target == other.target
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

fn same_bar(left: &Bar, right: &Bar) -> bool {
    // Empty slots contain NaNs, but do not change while the pointer stays still.
    left == right
        || (left.is_whitespace()
            && right.is_whitespace()
            && left.time == right.time
            && left.colors == right.colors
            && left.payload == right.payload)
}

/// The object under the pointer of a [`ChartPointer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointerTarget {
    /// A series or study line, as a click there would select it.
    Element(ChartElementId),
    /// A drawing, by its drawing id.
    Drawing(usize),
    /// A primitive whose [`hit_test`](super::Primitive::hit_test) reports the pointer.
    Primitive(super::PrimitiveId),
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
    /// stationary cursor or a held modifier; `None` means it left the plots.
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
    /// A successful history page or `set_data` completed, including an empty result.
    /// Live updates emit `DataUpdated` instead; failed or obsolete feed replies emit nothing.
    DataLoaded {
        /// Bar series that received data.
        series: SeriesId,
        /// Total accepted bars now held by that series, after cleaning and merging.
        bars: usize,
    },
    /// A live bar from `update` or the chart's feed changed the bars of a series, or `pop_bars`
    /// removed some. Changes of one series in a row are reported once, with the latest count.
    DataUpdated {
        /// Bar series whose bars changed.
        series: SeriesId,
        /// Total bars now held by that series.
        bars: usize,
    },
    /// The symbol setter accepted a different symbol.
    SymbolChanged(String),
    /// The interval setter accepted a different interval.
    TimeframeChanged(crate::model::Timeframe),
}

#[derive(Default)]
pub(super) struct HostEvents {
    queue: std::collections::VecDeque<ChartEvent>,
    dropped: usize,
    range: Option<VisibleTimeRange>,
    logical_range: Option<crate::model::LogicalRange>,
    time_scale_size: egui::Vec2,
    pointer: Option<ChartPointer>,
    kind: Option<PointerKind>,
    click_frames: [Option<u64>; 3],
    /// What the pointer is over this frame, resolved while the plots are drawn.
    pub target: Option<PointerTarget>,
    pub volume_map: Option<crate::scales::PriceMap>,
}

impl HostEvents {
    pub(super) fn push(&mut self, event: ChartEvent) {
        if self.queue.len() == 1024 {
            self.queue.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.queue.push_back(event);
    }
}

impl Chart {
    /// Overrides the input device reported by pointer events. Set this before `show` when
    /// the host knows the active device, including pens that egui cannot identify. `None`
    /// restores automatic mouse/touch detection. Update it when the active device changes.
    pub fn set_pointer_kind(&mut self, kind: Option<PointerKind>) {
        self.host_events.kind = kind;
    }

    /// Drains host notifications in occurrence order. Call after `show` each frame.
    ///
    /// Symbol, interval and data setters enqueue immediately; view and pointer changes are
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
            .chain(self.host_events.queue.drain(..))
    }

    pub(super) fn data_loaded(&mut self, series: SeriesId) {
        self.host_events.push(ChartEvent::DataLoaded {
            series,
            bars: self.bars(series).len(),
        });
    }

    pub(super) fn data_updated(&mut self, series: SeriesId) {
        let bars = self.bars(series).len();
        if let Some(ChartEvent::DataUpdated {
            series: last,
            bars: count,
        }) = self.host_events.queue.back_mut()
            && *last == series
        {
            *count = bars;
        } else {
            self.host_events
                .push(ChartEvent::DataUpdated { series, bars });
        }
    }

    fn pointer_at(&self, ui: &Ui, pos: Pos2) -> Option<ChartPointer> {
        let scale = self.state.time_scale();
        let plot = self.last_rendered_price_rect;
        let shown = self.state.shown();
        let index = scale
            .coord_to_idx_precise(pos.x, plot.left(), plot.width())
            .round();
        let time = crate::model::time_at(shown, index)?;
        let bar_at = (index >= 0.0 && index < shown.len() as f64).then_some(index as usize);
        let magnet = |map: crate::scales::PriceMap, values: Vec<f64>| {
            if self.chart_options.crosshair.mode.snaps() {
                crate::chart::renderers::snap_to_values(pos.y, &map, values)
                    .unwrap_or_else(|| map.y_to_price(pos.y))
            } else {
                map.y_to_price(pos.y)
            }
        };
        // Panes are counted as drawn, from the top; the price pane may sit below others.
        let pane_index = |id: super::PaneId| {
            (self.pane_layout.panes.iter())
                .filter(|pane| pane.id == super::PaneId::MAIN || self.pane_shown(pane))
                .position(|pane| pane.id == id)
                .unwrap_or_default()
        };
        let (series, price, pane) = if let Some(group) =
            self.pane_layout.panes.iter().find(|group| {
                group.id != super::PaneId::MAIN
                    && group.rect.contains(pos)
                    && plot.x_range().contains(pos.x)
            }) {
            let owner = *self.pane_series(group.id).first()?;
            let map = group.price_map();
            let closes = group.members.iter().filter_map(|id| {
                let item =
                    (self.price_series.iter()).find(|item| item.id == *id && item.visible)?;
                bar_at.and_then(|index| self.series_close_at(item, index))
            });
            let values = group
                .members
                .iter()
                .filter_map(|id| self.study_ids.iter().position(|study| study == id))
                .map(|index| &self.rendered_studies().indicators()[index])
                .filter(|study| study.is_visible())
                .flat_map(|study| {
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
                            bar_at.and_then(|index| study.plot_value(line, index))
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
                pane_index(super::PaneId::MAIN),
            )
        } else if plot.contains(pos) {
            let snapped = bar_at.map_or(pos, |index| {
                self.snap_crosshair(pos, &self.last_price_map, index)
            });
            let price = self.last_price_map.y_to_price(snapped.y);
            (SeriesId::MAIN, price, pane_index(super::PaneId::MAIN))
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
            bar: bar_at.map(|index| shown[index].clone()),
            previous_close: bar_at
                .and_then(|index| crate::model::last_with_value(&shown[..index]))
                .map(|bar| bar.close),
            series_bars: self
                .price_series
                .iter()
                .filter_map(|item| {
                    let at = item.bars.binary_search_by_key(&time, |bar| bar.time).ok()?;
                    Some((item.id, item.bars[at].clone()))
                })
                .collect(),
            studies: bar_at.map_or_else(Vec::new, |index| self.study_rows_at(index)),
            target: self.host_events.target,
            modifiers: ui.input(|input| input.modifiers),
            kind: self.host_events.kind.unwrap_or_else(|| {
                if ui.input(|input| {
                    input.any_touches()
                        || input
                            .events
                            .iter()
                            .any(|event| matches!(event, egui::Event::Touch { .. }))
                }) {
                    PointerKind::Touch
                } else {
                    PointerKind::Mouse
                }
            }),
        })
    }

    pub(super) fn record_host_events(&mut self, ui: &Ui) {
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
                self.last_rendered_price_rect.min,
                egui::pos2(
                    self.last_rendered_price_rect.right(),
                    ui.max_rect().bottom(),
                ),
            ))
            .then(|| self.pointer_at(ui, pos))
            .flatten()
        });
        if position != self.host_events.pointer {
            self.host_events.pointer = position.clone();
            self.host_events.push(ChartEvent::CrosshairMoved(position));
        }
    }

    pub(super) fn record_host_click(&mut self, ui: &Ui, response: &egui::Response) {
        let Some(position) = response
            .interact_pointer_pos()
            .and_then(|pos| self.pointer_at(ui, pos))
        else {
            return;
        };
        let frame = ui.ctx().cumulative_frame_nr();
        for (index, button) in [PointerButton::Primary, PointerButton::Secondary]
            .into_iter()
            .enumerate()
        {
            if response.clicked_by(button) && self.host_events.click_frames[index] != Some(frame) {
                self.host_events.click_frames[index] = Some(frame);
                self.host_events.push(ChartEvent::Click {
                    position: position.clone(),
                    button,
                });
            }
        }
        if response.double_clicked() && self.host_events.click_frames[2] != Some(frame) {
            self.host_events.click_frames[2] = Some(frame);
            self.host_events.push(ChartEvent::DoubleClick { position });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::test_frames::{chart, frame};
    use egui::{Context, Event};

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
            assert_eq!(pointer.time, bars[70].time);
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
            frame(&ctx, &mut chart, 1.5, vec![]);
            chart.drain_events().for_each(drop);
            let pane = &chart.last_rendered_indicator_panes[0];
            let pos = Pos2::new(pane.mapping.idx_to_x(70), pane.chart_rect.center().y);
            frame(&ctx, &mut chart, 1.6, vec![Event::PointerMoved(pos)]);
            let events = chart.drain_events().collect::<Vec<_>>();
            let Some(ChartEvent::CrosshairMoved(Some(pointer))) = events.last() else {
                panic!("{events:?}")
            };
            assert_eq!(pointer.series, id);
            assert_eq!(pointer.time, bars[70].time);
            assert!((pointer.price - 50.0).abs() < 0.001);
            assert_eq!(pointer.pane, 1);
            assert_eq!(
                pointer.studies,
                chart
                    .study_values_at(70)
                    .into_iter()
                    .map(|row| (id, row))
                    .collect::<Vec<_>>()
            );
            let expected = pointer.clone();
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
            assert!(chart.drain_events().any(|event| event
                == ChartEvent::Click {
                    position: expected.clone(),
                    button: PointerButton::Primary
                }));
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
