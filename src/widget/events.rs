//! Host notifications from chart setters, history loads and rendered interaction.

use super::{Chart, SeriesId, VisibleTimeRange};
use chrono::{DateTime, Utc};
use egui::{PointerButton, Pos2, Ui};

/// A chart cursor position in UTC and the owning pane's value units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartPointer {
    /// Nearest slot's timestamp, extrapolated into whitespace as in `visible_time_range`.
    pub time: DateTime<Utc>,
    /// Price on the main pane, or the study/volume value on another pane.
    pub price: f64,
    /// `MAIN`, `VOLUME`, or the study owning the hovered pane.
    pub series: SeriesId,
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
    /// The local cursor moved or its mapped value changed; `None` means it left the plots.
    /// Programmatically synchronized cursors do not emit this event.
    CrosshairMoved(Option<ChartPointer>),
    /// A primary or secondary click in a plot, including clicks selecting drawings or studies.
    Click {
        /// Click location in the pane's units.
        position: ChartPointer,
        /// The button released to complete the click.
        button: PointerButton,
    },
    /// A successful history page or `set_data` completed, including an empty result.
    /// Live updates and failed or obsolete feed replies do not emit this event.
    DataLoaded {
        /// Bar series that received data.
        series: SeriesId,
        /// Total accepted bars now held by that series, after cleaning and merging.
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
    click_frames: [Option<u64>; 2],
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
    /// Drains host notifications in occurrence order. Call after `show` each frame.
    ///
    /// Symbol, interval and data setters enqueue immediately; view and pointer changes are
    /// measured during `show`. Unchanged views and stationary cursors emit nothing. Drain
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

    fn pointer_at(&self, pos: Pos2) -> Option<ChartPointer> {
        let scale = self.state.time_scale();
        let plot = self.last_rendered_price_rect;
        let index = scale.coord_to_idx(pos.x, plot.left(), plot.width()).round() as f64;
        let time = super::sync::time_at(self.state.shown(), index)?;
        let (series, map, values) = if let Some(pane) = self
            .last_rendered_indicator_panes
            .iter()
            .find(|pane| pane.chart_rect.contains(pos))
        {
            let id = *self.study_ids.get(pane.indicator_idx)?;
            let group = self
                .pane_layout
                .panes
                .iter()
                .find(|group| group.studies.contains(&id))?;
            let values = group
                .studies
                .iter()
                .filter_map(|id| self.study_ids.iter().position(|study| study == id))
                .map(|index| &self.rendered_studies().indicators()[index])
                .filter(|study| study.is_visible())
                .flat_map(|study| {
                    (0..study.line_cnt())
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
                            (index >= 0.0 && index < self.state.shown().len() as f64)
                                .then(|| study.plot_value(line, index as usize))
                                .flatten()
                        })
                })
                .collect::<Vec<_>>();
            (group.id, pane.mapping.price, values)
        } else if self.last_rendered_volume_rect.contains(pos)
            && let Some(map) = self.host_events.volume_map
        {
            let values = if index >= 0.0 && index < self.state.shown().len() as f64 {
                vec![self.state.shown()[index as usize].volume]
            } else {
                Vec::new()
            };
            (SeriesId::VOLUME, map, values)
        } else if plot.contains(pos) {
            let snapped = if index >= 0.0 && index < self.state.shown().len() as f64 {
                self.snap_crosshair(pos, &self.last_price_map, index as usize)
            } else {
                pos
            };
            let price = self.last_price_map.y_to_price(snapped.y);
            return price.is_finite().then_some(ChartPointer {
                time,
                price,
                series: SeriesId::MAIN,
            });
        } else {
            return None;
        };
        let price = if self.chart_options.crosshair.mode == crate::config::CrosshairMode::Magnet {
            crate::chart::renderers::snap_to_values(pos.y, &map, values)
                .unwrap_or_else(|| map.y_to_price(pos.y))
        } else {
            map.y_to_price(pos.y)
        };
        price.is_finite().then_some(ChartPointer {
            time,
            price,
            series,
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
            .then(|| self.pointer_at(pos))
            .flatten()
        });
        if position != self.host_events.pointer {
            self.host_events.pointer = position;
            self.host_events.push(ChartEvent::CrosshairMoved(position));
        }
    }

    pub(super) fn record_host_click(&mut self, ui: &Ui, response: &egui::Response) {
        let Some(position) = response
            .interact_pointer_pos()
            .and_then(|pos| self.pointer_at(pos))
        else {
            return;
        };
        for (index, button) in [PointerButton::Primary, PointerButton::Secondary]
            .into_iter()
            .enumerate()
        {
            let frame = ui.ctx().cumulative_frame_nr();
            if response.clicked_by(button) && self.host_events.click_frames[index] != Some(frame) {
                self.host_events.click_frames[index] = Some(frame);
                self.host_events
                    .push(ChartEvent::Click { position, button });
            }
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
        let bars = chart.bars(SeriesId::MAIN).to_vec();
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
        for (mode, inverted, price) in [
            (crate::scales::PriceScaleMode::Normal, false, 125.0),
            (crate::scales::PriceScaleMode::Normal, true, 175.0),
            (
                crate::scales::PriceScaleMode::Logarithmic,
                false,
                100.0 * 2.0_f64.powf(0.25),
            ),
        ] {
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
            let expected = *pointer;
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
                        position: expected,
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
            let expected = *pointer;
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
                    position: expected,
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
