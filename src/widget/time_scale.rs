//! Host control of the time scale: fitting, scrolling, logical ranges and coordinate conversions.

use super::Chart;
use crate::model::LogicalRange;
use chrono::{DateTime, Utc};
use egui::{Ui, Vec2};

/// A scroll moving the right offset linearly to `to` over `duration` seconds.
#[derive(Debug, Clone, Copy)]
pub(super) struct ScrollAnimation {
    to: f32,
    duration: f64,
    /// Start time and offset, taken on the first frame, and the offset the last frame set.
    progress: Option<(f64, f64, f64)>,
}

impl Chart {
    /// Fits every bar into the plot width, then restores the configured right offset.
    ///
    /// The right offset, in bars or pixels, is kept free of bars. The bar spacing limits still
    /// apply.
    pub fn fit_content(&mut self) {
        self.cancel_view_motion();
        self.state.time_scale_mut().fit_content();
    }

    /// Restores the configured bar spacing and right offset, leaving the price scale unchanged.
    pub fn reset_time_scale(&mut self) {
        self.cancel_view_motion();
        let scale = self.state.time_scale_mut();
        scale.set_bar_spacing(self.chart_options.time_scale.bar_spacing);
        scale.scroll_to_realtime();
    }

    /// Restores the configured bar spacing and right margin, scrolls to the latest bar and
    /// returns every price scale in every pane to auto-scaling. Scale modes and inversion stay.
    /// Pending scrolls, animations and requested visible-bar counts are cancelled.
    pub fn reset_chart_view(&mut self) {
        self.apply_visible_bars_once = false;
        self.desired_visible_bars = None;
        self.tracking_mode_active = false;
        self.reset_time_scale();
        self.state.price_scale_mut().reset();
        for scale in self.series_scales.values_mut() {
            scale.reset();
        }
        for pane in &mut self.pane_layout.panes {
            pane.scale.reset();
        }
    }

    /// Scrolls to the latest bar with the configured right offset, keeping the zoom.
    ///
    /// Unlike [`Chart::enable_tracking_mode`], the view does not keep returning to the latest
    /// bar afterwards. `animated` scrolls over 400 ms instead of jumping.
    pub fn scroll_to_real_time(&mut self, animated: bool) {
        self.cancel_view_motion();
        let target = self.state.time_scale().live_edge_offset();
        self.scroll_to(target, 0.4, animated);
    }

    /// Scrolls the latest bar `position` bars from the right edge, keeping the zoom.
    ///
    /// Positive positions leave empty space on the right and negative ones scroll into
    /// history; the edge limits of the time scale options still apply. `animated` scrolls
    /// over one second instead of jumping. Ignores non-finite positions.
    pub fn scroll_to_position(&mut self, position: f32, animated: bool) {
        if position.is_finite() {
            self.cancel_view_motion();
            self.tracking_mode_active = false;
            self.scroll_to(position, 1.0, animated);
        }
    }

    /// The visible range of logical indices, `None` without data.
    ///
    /// Index `0` is the oldest bar, or the oldest element on price-based chart types. Both
    /// bounds are fractional and may lie outside the data, in whitespace before the first bar
    /// or after the last. `right` is the index at the right edge less half a bar, so a range
    /// spans `right - left + 1` bar widths of plot.
    pub fn visible_logical_range(&self) -> Option<LogicalRange> {
        (!self.state.shown().is_empty()).then(|| self.state.time_scale().visible_logical_range())
    }

    /// Shows the logical range across the plot width of the last frame, setting both the bar
    /// spacing and the scroll position, so [`Chart::visible_logical_range`] returns it.
    ///
    /// Returns `false` without changing the view for empty data, non-finite bounds or a
    /// `right` below `left`. The bar spacing and edge limits still apply.
    pub fn set_visible_logical_range(&mut self, range: LogicalRange) -> bool {
        let bars = range.right - range.left + 1.0;
        if self.state.shown().is_empty() || !bars.is_finite() || range.right < range.left {
            return false;
        }
        self.cancel_view_motion();
        self.tracking_mode_active = false;
        let scale = self.state.time_scale_mut();
        scale.set_bar_spacing(scale.width() / bars);
        scale.set_right_offset(range.right - scale.base_idx() as f32);
        true
    }

    /// The UTC time at a logical index, `None` without data or for a non-finite index.
    ///
    /// Between bars the time is interpolated, and in whitespace it is extrapolated, as in
    /// [`Chart::visible_time_range`], at the smallest positive spacing among the outermost 16
    /// bars, or by calendar months for a known month or year timeframe.
    pub fn index_to_time(&self, index: f64) -> Option<DateTime<Utc>> {
        crate::model::time_at_with_timeframe(self.state.shown(), index, self.timeframe())
    }

    /// The fractional logical index of a UTC time, `None` without data.
    ///
    /// The inverse of [`Chart::index_to_time`]: a time between two bars lies between their
    /// indices, and a time outside the data is extrapolated into whitespace.
    pub fn time_to_index(&self, time: DateTime<Utc>) -> Option<f64> {
        crate::model::index_at_with_timeframe(self.state.shown(), time, self.timeframe())
    }

    /// Gives the horizontal axis a meaning other than time, such as option strikes or bond
    /// maturities. The [`time`](crate::model::Bar::time) of each bar, marker and drawing then
    /// holds a key of `scale`, which labels the axis and the crosshair; everything placed by
    /// time is placed by key. `None` restores the time scale.
    ///
    /// Session breaks and extended-hours shading are periods of time, drawn on the time scale
    /// only.
    pub fn set_horizontal_scale(
        &mut self,
        scale: Option<std::sync::Arc<dyn crate::scales::HorizontalScale>>,
    ) {
        self.horizontal_scale = scale;
    }

    /// The screen x of a logical index in the last frame, at the centre of its bar.
    ///
    /// `None` before the chart is shown or for a non-finite index.
    pub fn index_to_x(&self, index: f64) -> Option<f32> {
        let plot = self.last_rendered_price_rect;
        (index.is_finite() && plot.width() > 0.0).then(|| {
            self.state
                .time_scale()
                .idx_to_coord_precise(index, plot.left(), plot.width())
        })
    }

    /// The fractional logical index at a screen x in the last frame, the inverse of
    /// [`Chart::index_to_x`]. Round it for the nearest bar.
    ///
    /// `None` before the chart is shown or for a non-finite x.
    pub fn x_to_index(&self, x: f32) -> Option<f64> {
        let plot = self.last_rendered_price_rect;
        (x.is_finite() && plot.width() > 0.0).then(|| {
            self.state
                .time_scale()
                .coord_to_idx_precise(x, plot.left(), plot.width())
        })
    }

    /// The size of the time axis below the plots in the last frame, in points.
    ///
    /// The width is the plot width; the height is zero while time labels are hidden.
    pub fn time_scale_size(&self) -> Vec2 {
        Vec2::new(
            self.last_rendered_price_rect.width().max(0.0),
            self.time_axis_rect.height().max(0.0),
        )
    }

    /// The time axis height: the label row, or the configured minimum when larger, and zero
    /// while time labels are hidden.
    pub(super) fn time_axis_height(&self) -> f32 {
        if self.config.show_time_labels {
            crate::styles::sizing::chart::BOTTOM_PADDING_WITH_TIME
                .max(self.chart_options.time_scale.minimum_height)
        } else {
            0.0
        }
    }

    /// The x, relative to the plot, that a zoom keeps in place: `x`, or the last pixel, which
    /// keeps the right offset, while the right bar stays on scroll.
    pub(super) fn zoom_anchor(&self, x: f32, width: f32) -> f32 {
        if self.chart_options.time_scale.right_bar_stays_on_scroll {
            width - 1.0
        } else {
            x
        }
    }

    /// Moves the right offset this frame of a scroll animation; any other change of the
    /// offset since the previous frame, by the user, the host or new data, ends it.
    pub(super) fn advance_scroll_animation(&mut self, ui: &Ui) {
        let Some(mut animation) = self.scroll_animation.take() else {
            return;
        };
        let now = ui.input(|input| input.time);
        let scale = self.state.time_scale_mut();
        let (start, from, last) = *animation.progress.get_or_insert((
            now,
            scale.right_offset_precise(),
            scale.right_offset_precise(),
        ));
        if scale.right_offset_precise() != last {
            return;
        }
        let elapsed = ((now - start) / animation.duration) as f32;
        if elapsed >= 1.0 {
            scale.set_right_offset(animation.to);
            return;
        }
        scale.set_right_offset(from + (f64::from(animation.to) - from) * f64::from(elapsed));
        animation.progress = Some((start, from, scale.right_offset_precise()));
        self.scroll_animation = Some(animation);
        ui.ctx().request_repaint();
    }

    fn scroll_to(&mut self, offset: f32, duration: f64, animated: bool) {
        if animated {
            self.scroll_animation = Some(ScrollAnimation {
                to: offset,
                duration,
                progress: None,
            });
        } else {
            self.state.time_scale_mut().set_right_offset(offset);
        }
    }

    /// Ends pending scrolls, animations and momentum, so a programmatic view replaces them.
    pub(super) fn cancel_view_motion(&mut self) {
        self.pending_scroll_time = None;
        self.pending_start_idx = None;
        self.scroll_animation = None;
        self.kinetic_scroll = super::state::KineticScrollState::new();
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Bar, BarData, LogicalRange};
    use crate::scales::{PriceHorizontalScale, key_time};
    use crate::widget::test_frames::{chart, frame, shapes};
    use crate::widget::{Chart, ChartEvent, SeriesId};
    use chrono::Duration;

    fn assert_close(actual: f64, expected: f64, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what}: {actual} != {expected}"
        );
    }

    fn assert_range(chart: &Chart, left: f32, right: f32, what: &str) {
        let range = chart.visible_logical_range().unwrap();
        assert_close(range.left.into(), left.into(), what);
        assert_close(range.right.into(), right.into(), what);
    }

    #[test]
    fn zero_margin_keeps_the_latest_bar_and_its_live_updates_visible() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[95.0], &[105.0]);
        let first = chart.bars(SeriesId::MAIN)[0].time;
        chart.set_data(
            SeriesId::MAIN,
            (0..1000)
                .map(|index| {
                    Bar::new(
                        first + Duration::minutes(index),
                        95.0,
                        105.0,
                        95.0,
                        105.0,
                        100.0,
                    )
                })
                .collect(),
        );
        let mut options = chart.chart_options().clone();
        options.time_scale.right_offset = 0.0;
        options.time_scale.fix_right_edge = true;
        chart.set_chart_options(options);
        for time in [0.0, 0.1, 0.2] {
            frame(&ctx, &mut chart, time, vec![]);
        }
        let last = chart.bars(SeriesId::MAIN).len() - 1;
        let plot = chart.get_rendered_price_rect();
        assert!((plot.left()..plot.right()).contains(&chart.index_to_x(last as f64).unwrap()));
        assert_eq!(chart.state().visible_range().1, last + 1);
        let mut update = chart.bars(SeriesId::MAIN)[last].clone();
        update.close = 110.0;
        update.high = 110.0;
        chart.update(SeriesId::MAIN, update);
        frame(&ctx, &mut chart, 0.3, vec![]);
        assert_eq!(chart.state().visible_data().last().unwrap().close, 110.0);
        assert!(
            chart
                .series_price_map(SeriesId::MAIN)
                .unwrap()
                .range()
                .contains(110.0)
        );
    }

    #[test]
    fn host_coordinates_retain_fractions_on_large_histories() {
        let bars = (0..1_000_000)
            .map(|i| {
                Bar::from_value(
                    chrono::DateTime::from_timestamp(1_700_000_000 + i * 60, 0).unwrap(),
                    10.0,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let ctx = egui::Context::default();
        frame(&ctx, &mut chart, 0.0, vec![]);
        chart.state.time_scale_mut().set_bar_spacing(200.0);
        chart.state.time_scale_mut().set_right_offset(0.0);
        let x = chart.index_to_x(999_999.0).unwrap();
        let fractional = chart.index_to_x(999_999.005).unwrap();
        assert!((fractional - x - 1.0).abs() < 1e-4);
        assert!((chart.x_to_index(fractional).unwrap() - 999_999.005).abs() < 1e-6);
        let scale = chart.state.time_scale();
        let distance = (f64::from(scale.width()) - 1.0) / 200.0 - 0.5;
        let expected = chart.bars(SeriesId::MAIN).last().unwrap().time
            - Duration::nanoseconds((distance * 60e9).round() as i64);
        assert!(
            (chart.visible_time_range().unwrap().from - expected)
                .num_nanoseconds()
                .unwrap()
                .abs()
                < 1_000
        );
        let plot = chart.get_rendered_price_rect();
        for (step, (index, nearest)) in [(999_997.49, 999_997), (999_997.51, 999_998)]
            .into_iter()
            .enumerate()
        {
            let pos = egui::pos2(chart.index_to_x(index).unwrap(), plot.center().y);
            frame(
                &ctx,
                &mut chart,
                0.1 + step as f64 * 0.1,
                vec![egui::Event::PointerMoved(pos)],
            );
            assert!(
                chart.drain_events().any(|event| matches!(
                    event,
                    ChartEvent::CrosshairMoved(Some(pointer)) if pointer.index == nearest
                )),
                "index {index} must select bar {nearest}"
            );
        }
        let time = chart.bars(SeriesId::MAIN)[999_997].time + Duration::milliseconds(300);
        let added = chart.add_series(crate::widget::SeriesOptions {
            chart_type: crate::model::ChartType::Candles,
            up_color: egui::Color32::BLUE,
            ..Default::default()
        });
        chart.set_data(added, vec![Bar::new(time, 9.0, 11.0, 9.0, 11.0, 100.0)]);
        let painted = shapes(&ctx, &mut chart, 0.3, vec![]);
        let x = chart.index_to_x(999_997.005).unwrap();
        assert!(
            painted.iter().any(|shape| matches!(shape,
                egui::Shape::Rect(body)
                    if body.fill == egui::Color32::BLUE && (body.rect.center().x - x).abs() < 1e-3
            )),
            "an added series must retain its fractional time position"
        );
        let plot = chart.get_rendered_price_rect();
        let center = 999_997.137;
        let x = chart.index_to_x(center).unwrap();
        assert!(chart.execute_box_zoom(
            egui::pos2(x - 125.0, plot.top() + 100.0),
            egui::pos2(x + 125.0, plot.top() + 200.0),
            plot,
            plot.width(),
        ));
        assert!((chart.index_to_x(center).unwrap() - plot.center().x).abs() < 1e-3);
        chart.state.time_scale_mut().set_bar_spacing(200.0);
        chart.state.time_scale_mut().set_right_offset(-500_000.005);
        let x = chart.index_to_x(499_999.0).unwrap();
        assert_eq!(chart.get_chart_mapping().idx_to_x(499_999), x);
        chart.state.time_scale_mut().scroll_pixels(1.0);
        assert!((chart.index_to_x(499_999.0).unwrap() - x - 1.0).abs() < 1e-4);
    }

    /// Expected values follow from the time scale's definition on 100 daily bars: a logical
    /// range spans `right - left + 1` bar widths, bar `i` is centred `right + 0.5 - i` bar
    /// widths left of the plot's last pixel, and the defaults are a spacing of 6 and a right
    /// offset of 2.5 bars.
    #[test]
    fn time_scale_methods_set_read_and_convert_the_view() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[95.0], &[105.0]);
        for t in [0.0, 0.1] {
            frame(&ctx, &mut chart, t, vec![]);
        }
        let plot = chart.get_rendered_price_rect();
        let width = plot.width();
        let events: Vec<_> = chart.drain_events().collect();
        assert_eq!(chart.time_scale_size(), egui::vec2(width, 28.0));
        assert!(events.contains(&ChartEvent::TimeScaleResized(chart.time_scale_size())));
        assert!(events.contains(&ChartEvent::VisibleLogicalRangeChanged(
            chart.visible_logical_range()
        )));

        // 40 bar widths across the plot, with bar 49 the last whole bar.
        assert!(chart.set_visible_logical_range(LogicalRange::new(10.0, 49.0)));
        let spacing = chart.get_time_scale_state().0;
        assert_close(spacing.into(), (width / 40.0).into(), "spacing");
        assert_close(chart.get_time_scale_state().1.into(), -50.0, "offset");
        assert_range(&chart, 10.0, 49.0, "set range");
        // Bar 10 is centred half a bar in from the left edge: the last pixel is width - 1.
        let x = chart.index_to_x(10.0).unwrap();
        assert_close(x.into(), (plot.left() + spacing / 2.0 - 1.0).into(), "x");
        assert_close(chart.x_to_index(x).unwrap(), 10.0, "index");
        frame(&ctx, &mut chart, 0.2, vec![]);
        let range = chart.visible_logical_range();
        assert!(
            chart
                .drain_events()
                .any(|event| event == ChartEvent::VisibleLogicalRangeChanged(range))
        );
        for invalid in [
            LogicalRange::new(5.0, 4.0),
            LogicalRange::new(f32::NAN, 4.0),
        ] {
            assert!(!chart.set_visible_logical_range(invalid));
        }
        assert_range(&chart, 10.0, 49.0, "invalid ranges");

        // Daily bars: 10.5 is noon of day 10, and 105 extrapolates past the last bar.
        let first = chart.bars(SeriesId::MAIN)[0].time;
        let noon = first + Duration::hours(10 * 24 + 12);
        assert_eq!(chart.index_to_time(10.5), Some(noon));
        assert_close(chart.time_to_index(noon).unwrap(), 10.5, "time");
        assert_eq!(
            chart.index_to_time(105.0),
            Some(first + Duration::days(105))
        );

        // 100 bars and the 2.5-bar right offset share the width.
        chart.fit_content();
        assert_close(
            chart.get_time_scale_state().0.into(),
            (width / 102.5).into(),
            "fit",
        );
        assert_range(&chart, 0.0, 101.5, "fit");
        chart.reset_time_scale();
        assert_eq!(chart.get_time_scale_state(), (6.0, 2.5));

        chart.scroll_to_position(-20.0, false);
        assert_eq!(chart.get_time_scale_state().1, -20.0);
        // Animations start on the next frame and move linearly: one second to a position,
        // 400 ms back to the live edge.
        chart.scroll_to_position(10.0, true);
        for (t, offset) in [(1.0, -20.0), (1.5, -5.0), (2.0, 10.0), (2.1, 10.0)] {
            frame(&ctx, &mut chart, t, vec![]);
            assert_close(chart.get_time_scale_state().1.into(), offset, "position");
        }
        chart.scroll_to_real_time(true);
        for (t, offset) in [(3.0, 10.0), (3.2, 6.25), (3.4, 2.5)] {
            frame(&ctx, &mut chart, t, vec![]);
            assert_close(chart.get_time_scale_state().1.into(), offset, "real time");
        }
        // Any other change of the view ends an animation.
        chart.scroll_to_position(-20.0, true);
        frame(&ctx, &mut chart, 4.0, vec![]);
        chart.apply_synced_time_scale(6.0, 0.0);
        frame(&ctx, &mut chart, 4.5, vec![]);
        assert_eq!(chart.get_time_scale_state().1, 0.0);

        let mut options = chart.chart_options().clone();
        options.time_scale.minimum_height = 40.0;
        options.time_scale.right_offset_pixels = Some(60.0);
        let mut chart = chart.with_chart_options(options);
        frame(&ctx, &mut chart, 5.0, vec![]);
        assert_eq!(chart.time_scale_size(), egui::vec2(width, 40.0));
        assert!(
            chart
                .drain_events()
                .any(|event| event == ChartEvent::TimeScaleResized(egui::vec2(width, 40.0)))
        );
        // A pixel offset is kept free of bars, so all 100 fit the remaining width.
        chart.fit_content();
        assert_close(
            chart.get_time_scale_state().0.into(),
            ((width - 60.0) / 100.0).into(),
            "fit with pixels",
        );
        assert_range(
            &chart,
            0.0,
            99.0 + 60.0 / chart.get_time_scale_state().0,
            "fit with pixels",
        );
    }

    /// Strikes 4400 to 4600 in steps of 25 across the plot are about 80 points apart, so the
    /// axis labels every other one, the multiples of 50, by the scale's weights; the crosshair
    /// on 4475 reads it, the CSV export writes strikes, and no time is formatted anywhere.
    #[test]
    fn a_horizontal_scale_labels_the_axis_and_the_crosshair_by_key() {
        let bars = (0..=8)
            .map(|i| Bar::from_value(key_time(4400.0 + 25.0 * f64::from(i)).unwrap(), 20.0))
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_horizontal_scale(Some(std::sync::Arc::new(PriceHorizontalScale {
            precision: 0,
        })));
        let ctx = egui::Context::default();
        frame(&ctx, &mut chart, 0.0, vec![]);
        assert!(chart.set_visible_logical_range(LogicalRange::new(0.0, 8.0)));
        frame(&ctx, &mut chart, 0.1, vec![]);
        let plot = chart.get_rendered_price_rect();
        let hover = egui::pos2(chart.index_to_x(3.0).unwrap(), plot.center().y);
        let texts = |painted: Vec<egui::Shape>| -> Vec<String> {
            painted
                .into_iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Text(text) if text.pos.y >= plot.bottom() => {
                        Some(text.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect()
        };
        let axis = texts(shapes(&ctx, &mut chart, 0.2, vec![]));
        assert_eq!(axis, ["4400", "4450", "4500", "4550", "4600"]);
        let hovered = texts(shapes(
            &ctx,
            &mut chart,
            0.3,
            vec![egui::Event::PointerMoved(hover)],
        ));
        assert!(hovered.contains(&"4475".to_owned()), "{hovered:?}");
        let csv = chart.export_csv().unwrap();
        assert_eq!(
            csv.lines()
                .nth(1)
                .and_then(|row| row.split_once(','))
                .map(|(key, _)| key),
            Some("4400")
        );
    }
}
