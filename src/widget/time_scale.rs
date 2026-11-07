//! Host control of the time scale: fitting, scrolling, logical ranges and coordinate conversions.

use super::{Chart, MismatchDirection};
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
    /// Fits all loaded bar series across every pane, including hidden series and whitespace.
    ///
    /// The configured right offset, in bars or pixels, follows the latest participating point.
    /// Spacing and fixed-edge limits still apply. Without a main horizontal domain, the union
    /// of added-series timestamps supplies whitespace slots for the shared mapping;
    /// values remain in their added series. With no loaded points the view is unchanged.
    pub fn fit_content(&mut self) {
        self.cancel_view_motion();
        self.refresh_time_scale_content();
        self.state.time_scale_mut().fit_content();
    }

    /// Keep fitting and edge limits on the same mapping used to paint added series.
    pub(super) fn refresh_time_scale_content(&mut self) {
        let tail = self
            .state
            .shows_fallback()
            .then(|| self.state.shown().last().map(|bar| bar.time))
            .flatten()
            .map(|tail| (tail, self.right_edge()));
        self.state.set_fallback_series(
            self.price_series
                .iter()
                .map(|item| (item.bars.as_slice(), item.revision)),
        );
        let shown = self.state.shown();
        let scale = self.state.time_scale();
        let range = shown.first().map(|_| {
            self.price_series
                .iter()
                .flat_map(|item| item.bars.first().into_iter().chain(item.bars.last()))
                .filter_map(|bar| crate::model::index_at(shown, bar.time))
                .map(|index| scale.position(index))
                .fold(
                    (0.0_f64, scale.base_idx() as f64),
                    |(left, right), index| (left.min(index), right.max(index)),
                )
        });
        self.state.time_scale_mut().set_content_range(range);
        if let Some((tail, right_edge)) = tail {
            self.added_slots_arrived(tail, right_edge);
        }
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
            for (_, scale) in &mut pane.scales {
                scale.reset();
            }
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
    /// spans `right - left + 1` bar widths of plot on an ordinal axis. Numeric axes apply
    /// those margins in mean key gaps before converting the bounds to fractional row indices.
    pub fn visible_logical_range(&self) -> Option<LogicalRange> {
        (!self.state.shown().is_empty()).then(|| self.state.time_scale().visible_logical_range())
    }

    /// Shows the logical range across the plot width of the last frame, setting both the bar
    /// spacing and the scroll position, so [`Chart::visible_logical_range`] returns it.
    ///
    /// Returns `false` without changing the view for empty data, non-finite bounds or a
    /// `right` below `left`. The bar spacing and edge limits still apply.
    pub fn set_visible_logical_range(&mut self, range: LogicalRange) -> bool {
        let ts = self.state.time_scale();
        let left = ts.position(f64::from(range.left));
        let right = ts.position(f64::from(range.right));
        let bars = (right - left + 1.0) as f32;
        if self.state.shown().is_empty() || !bars.is_finite() || range.right < range.left {
            return false;
        }
        self.cancel_view_motion();
        self.tracking_mode_active = false;
        let scale = self.state.time_scale_mut();
        scale.set_bar_spacing(scale.width() / bars);
        scale.set_right_offset(right - scale.base_idx() as f64);
        true
    }

    /// Calendar zone behind the axis; `None` selects the system's local zone.
    pub(super) fn calendar_timezone(&self) -> Option<crate::model::TimezoneId> {
        let timezone = self.display_timezone();
        match timezone {
            crate::scales::TimezoneMode::Local => None,
            _ => Some(timezone.to_tz().unwrap_or_default()),
        }
    }

    /// The UTC time at a logical index, `None` without data or for a non-finite index.
    ///
    /// Between bars the time is interpolated, and in whitespace it is extrapolated, as in
    /// [`Chart::visible_time_range`], by the known timeframe duration or the most common positive
    /// spacing among the outermost 16 bars (shorter spacing breaks ties, one second if none).
    /// A step that divides an hour keeps the slots on the grid of the regular bars nearest the
    /// edge, so a partial edge bar does not shift them. Known month and year timeframes step by
    /// calendar months in the display time zone when the edge bars fall at its midnight, and in
    /// UTC otherwise.
    pub fn index_to_time(&self, index: f64) -> Option<DateTime<Utc>> {
        if self.state.time_scale().numeric_positions().is_some() {
            return crate::model::time_at(self.state.shown(), index);
        }
        crate::model::time_at_with_timeframe(
            self.state.shown(),
            index,
            self.timeframe(),
            self.calendar_timezone().as_ref(),
        )
    }

    /// The fractional logical index of a UTC time, `None` without data.
    ///
    /// The inverse of [`Chart::index_to_time`]: a time between two bars lies between their
    /// indices, and a time outside the data is extrapolated into whitespace.
    pub fn time_to_index(&self, time: DateTime<Utc>) -> Option<f64> {
        if self.state.time_scale().numeric_positions().is_some() {
            return crate::model::index_at(self.state.shown(), time);
        }
        crate::model::index_at_with_timeframe(
            self.state.shown(),
            time,
            self.timeframe(),
            self.calendar_timezone().as_ref(),
        )
    }

    /// The stored UTC time at a whole logical index of the horizontal scale, including
    /// whitespace points, or `None` outside the loaded scale. On price-based charts the index
    /// addresses a displayed element. Unlike [`index_to_time`](Self::index_to_time), this never
    /// interpolates or extrapolates.
    pub fn time_at_index(&self, index: usize) -> Option<DateTime<Utc>> {
        self.state.shown().get(index).map(|bar| bar.time)
    }

    /// The whole logical index of a stored horizontal-scale point at `time`, or of its nearest
    /// neighbour in `direction`. An exact match returns its first index when displayed elements
    /// share a timestamp. Includes whitespace points and returns `None` for an empty scale or
    /// when no point lies in the requested direction.
    ///
    /// Unlike [`time_to_index`](Self::time_to_index), this never interpolates or extrapolates;
    /// unlike [`bar_index_at`](Self::bar_index_at), it addresses the shared scale, not a series'
    /// stored rows.
    pub fn time_index_at(
        &self,
        time: DateTime<Utc>,
        direction: MismatchDirection,
    ) -> Option<usize> {
        super::series::index_at_time(self.state.shown(), time, direction)
    }

    /// Gives the horizontal axis a meaning other than time, such as option strikes or bond
    /// maturities. The [`time`](crate::model::Bar::time) of each bar, marker and drawing then
    /// holds a key of `scale`, which labels the axis and the crosshair; everything placed by
    /// time is placed by key. `None` restores the time scale.
    ///
    /// Session breaks and extended-hours shading are periods of time, drawn on the time scale
    /// only. Numeric scales space actual samples proportionally, retaining row indices for
    /// data access; spacing and scroll offsets use the mean numeric gap as one unit.
    /// [`CustomHorizontalScale`](crate::scales::CustomHorizontalScale) accepts categorical and
    /// structured keys directly, using their ordered ranks for positions.
    pub fn set_horizontal_scale(
        &mut self,
        scale: Option<std::sync::Arc<dyn crate::scales::HorizontalScale>>,
    ) {
        let numeric = scale.as_ref().is_some_and(|scale| scale.is_numeric());
        let positions = numeric.then(|| crate::model::numeric_positions(self.state.shown()));
        self.state.time_scale_mut().set_numeric_positions(positions);
        self.horizontal_scale = scale;
    }

    /// Finds a bar by its host key in the shared custom horizontal domain. Returns `None` for
    /// an unknown key, absent bar or removed series. Use the same `scale` installed on this chart.
    pub fn bar_at_key<K: Ord>(
        &self,
        series: super::SeriesId,
        scale: &crate::scales::CustomHorizontalScale<K>,
        key: &K,
    ) -> Option<&crate::model::Bar> {
        let index = self.bar_index_at(series, scale.time(key)?, super::MismatchDirection::Exact)?;
        self.bars(series).get(index)
    }

    /// The screen x of a host key in the shared custom horizontal domain, using the last frame.
    /// Missing categories interpolate between loaded neighbours; known keys outside the data
    /// extrapolate. Returns `None` for an unknown key, empty chart or before the first frame.
    /// Use the same `scale` installed on this chart for bars and annotations.
    pub fn key_to_x<K: Ord>(
        &self,
        scale: &crate::scales::CustomHorizontalScale<K>,
        key: &K,
    ) -> Option<f32> {
        self.index_to_x(self.time_to_index(scale.time(key)?)?)
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

    /// The screen x of the first horizontal-scale point stored at `time` in the last frame.
    /// Includes whitespace points. `None` for an absent timestamp or before the chart is shown;
    /// loaded points outside the visible range still have coordinates.
    pub fn time_to_x(&self, time: DateTime<Utc>) -> Option<f32> {
        self.index_to_x(self.time_index_at(time, MismatchDirection::Exact)? as f64)
    }

    /// The stored UTC time of the horizontal-scale point nearest to a screen x in the last
    /// frame, including whitespace points. Halfway between points selects the one on the right.
    /// `None` before the chart is shown, for a non-finite x or when the rounded logical index
    /// lies outside the loaded scale. Use [`x_to_index`](Self::x_to_index) and
    /// [`index_to_time`](Self::index_to_time) for continuous interpolation and extrapolation.
    pub fn x_to_time(&self, x: f32) -> Option<DateTime<Utc>> {
        let index = (self.x_to_index(x)? + 0.5).floor();
        if !(0.0..self.state.shown().len() as f64).contains(&index) {
            return None;
        }
        self.time_at_index(index as usize)
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
    use crate::widget::{Chart, ChartEvent, MismatchDirection, SeriesId};
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

    /// Category order comes from the key, not its label or arrival order. Inserting the middle
    /// category moves its annotation to the new slot without changing its identity.
    #[test]
    fn custom_keys_align_insertion_lookup_axis_crosshair_and_annotations() {
        use crate::model::MarkerPos;
        use crate::scales::{CustomHorizontalScale, HorizontalScale};
        use crate::theme::{Theme, apply_to_egui};
        use std::sync::Arc;

        // Structured keys with labels whose alphabetic order differs from their key order.
        let keys = [(1, "Zulu"), (2, "Alpha"), (3, "Mike")];
        let scale = Arc::new(CustomHorizontalScale::new(
            [keys[2], keys[0], keys[1], keys[1]],
            |key| key.1.to_owned(),
        ));
        assert_eq!(scale.time(&(4, "Unknown")), None);
        assert!(scale.value(&(4, "Unknown"), 12.0).is_none());
        assert!(scale.marker(&(4, "Unknown")).is_none());
        for theme in [Theme::light(), Theme::dark()] {
            let ctx = egui::Context::default();
            apply_to_egui(&ctx, &theme);
            let mut chart = Chart::new(BarData::new());
            chart.set_horizontal_scale(Some(scale.clone()));
            chart.set_chart_type(crate::ChartType::LineWithMarkers);
            // Calendar intervals must not change category spacing or extrapolation.
            chart.set_timeframe(crate::model::Timeframe::Month1);
            let marker_color = egui::Color32::from_rgb(177, 52, 210);
            chart.set_markers(
                SeriesId::MAIN,
                vec![
                    scale
                        .marker(&keys[1])
                        .unwrap()
                        .with_pos(MarkerPos::InBar)
                        .with_color(marker_color),
                ],
            );
            // Last duplicate wins, and streaming inserts an older missing key in order.
            chart.set_data(
                SeriesId::MAIN,
                vec![
                    scale.value(&keys[2], 13.0).unwrap(),
                    scale.value(&keys[0], 9.0).unwrap(),
                    scale.value(&keys[0], 10.0).unwrap(),
                ],
            );
            assert!(chart.bar_at_key(SeriesId::MAIN, &scale, &keys[1]).is_none());
            assert!(chart.key_to_x(&scale, &keys[1]).is_none());
            frame(&ctx, &mut chart, 0.0, vec![]);
            // Ranks 0 and 2 leave rank 1 exactly halfway, even before it has a bar.
            let x = |key| chart.key_to_x(&scale, key).unwrap();
            assert_close(
                x(&keys[1]).into(),
                ((x(&keys[0]) + x(&keys[2])) / 2.0).into(),
                "missing category",
            );
            chart.update(SeriesId::MAIN, scale.value(&keys[1], 11.0).unwrap());
            chart.update(SeriesId::MAIN, scale.value(&keys[1], 12.0).unwrap());
            assert_eq!(chart.bars(SeriesId::MAIN).len(), 3);
            assert_eq!(
                chart.index_to_time(-1.0),
                Some(scale.time(&keys[0]).unwrap() - Duration::seconds(1))
            );
            for (key, value) in keys.iter().zip([10.0, 12.0, 13.0]) {
                let bar = chart.bar_at_key(SeriesId::MAIN, &scale, key).unwrap();
                assert_eq!(bar.close, value);
                assert_eq!(scale.key(bar.time), Some(key));
                assert_eq!(scale.format_crosshair(bar.time, 60), key.1);
            }
            assert!(
                chart
                    .bar_at_key(SeriesId::MAIN, &scale, &(4, "Unknown"))
                    .is_none()
            );
            assert!(chart.set_visible_logical_range(LogicalRange::new(-1.0, 3.0)));
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let plot = chart.get_rendered_price_rect();
            let xs: Vec<_> = keys
                .iter()
                .map(|key| chart.key_to_x(&scale, key).unwrap())
                .collect();
            assert_close(
                (xs[1] - xs[0]).into(),
                (xs[2] - xs[1]).into(),
                "category spacing",
            );
            assert!(chart.key_to_x(&scale, &(4, "Unknown")).is_none());
            for (key, x) in keys.iter().zip(&xs) {
                assert!(
                    painted.iter().any(|shape| matches!(shape,
                        egui::Shape::Text(text) if text.galley.text() == key.1
                            && text.pos.y >= plot.bottom()
                            && (text.pos.x + text.galley.size().x / 2.0 - x).abs() < 0.01
                    )),
                    "axis {}",
                    key.1
                );
            }
            assert!(
                painted.iter().any(|shape| matches!(shape,
                    egui::Shape::Circle(circle) if circle.fill == marker_color
                        && (circle.center.x - xs[1]).abs() < 0.01
                )),
                "marker at the inserted category"
            );
            let axis_y = painted
                .iter()
                .find_map(|shape| match shape {
                    egui::Shape::Text(text)
                        if text.galley.text() == "Alpha" && text.pos.y >= plot.bottom() =>
                    {
                        Some(text.pos.y)
                    }
                    _ => None,
                })
                .unwrap();
            let hovered = shapes(
                &ctx,
                &mut chart,
                0.2,
                vec![egui::Event::PointerMoved(egui::pos2(
                    xs[1],
                    plot.center().y,
                ))],
            );
            assert!(hovered.iter().any(|shape| matches!(shape,
                egui::Shape::Text(text) if text.galley.text() == "Alpha"
                    && text.pos.y >= plot.bottom() && text.pos.y < axis_y
            )));
            assert_eq!(chart.last_hover_bar_idx, Some(1.0));
            for unknown in [
                scale.time(&keys[0]).unwrap() - Duration::seconds(1),
                scale.time(&keys[2]).unwrap() + Duration::seconds(1),
                scale.time(&keys[0]).unwrap() + Duration::milliseconds(500),
            ] {
                assert_eq!(scale.key(unknown), None);
                assert_eq!(scale.format_crosshair(unknown, 60), "");
            }
        }
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

        // 1 ns / 60 s is below the precision of a floating-point index near one million.
        let time = chart.time_at_index(999_997).unwrap() + Duration::nanoseconds(1);
        let near = Bar::from_value(time, 10.0);
        chart.set_data(added, vec![near.clone()]);
        for (direction, expected) in [
            (MismatchDirection::Exact, None),
            (MismatchDirection::NearestLeft, None),
            (MismatchDirection::NearestRight, Some(&near)),
        ] {
            assert_eq!(chart.bar_at_index(added, 999_997.0, direction), expected);
        }
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
        assert_eq!(chart.time_scale_size(), egui::vec2(width, 35.0));
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

    /// An affine axis preserves differences: strikes have gaps 10:30 and maturities 9:24.
    /// Secondary samples, edge keys, markers, panes and the cursor share that same line.
    #[test]
    fn numeric_curves_use_key_distances_for_rendering_and_coordinates() {
        use crate::model::{Marker, MarkerPos, MarkerShape};
        use crate::scales::{HorizontalScale, MaturityScale};
        use crate::studies::{CustomIndicator, IndicatorValue};
        use crate::theme::{Theme, apply_to_egui};
        use crate::widget::SeriesOptions;
        use std::sync::Arc;

        let cases: [(Arc<dyn HorizontalScale>, &[f64]); 7] = [
            (
                Arc::new(PriceHorizontalScale { precision: 1 }),
                &[100.0, 110.0, 140.0],
            ),
            (Arc::new(MaturityScale::default()), &[3.0, 12.0, 36.0]),
            (
                Arc::new(PriceHorizontalScale::default()),
                &[-2.0, -1.75, 0.5],
            ),
            // The first whitespace step differs from subsequent steps at the right edge.
            (Arc::new(PriceHorizontalScale::default()), &[0.0, 0.25, 1.1]),
            (Arc::new(MaturityScale::default()), &[3.0]),
            (Arc::new(PriceHorizontalScale::default()), &[100.0]),
            (Arc::new(PriceHorizontalScale::default()), &[]),
        ];
        for theme in [Theme::light(), Theme::dark()] {
            for (scale, keys) in &cases {
                let bars = keys
                    .iter()
                    .enumerate()
                    .map(|(i, &key)| Bar::from_value(key_time(key).unwrap(), 10.0 + i as f64))
                    .collect();
                let mut chart = Chart::new(BarData::from_bars(bars));
                chart.set_chart_type(crate::ChartType::LineWithMarkers);
                chart.set_horizontal_scale(Some(scale.clone()));
                chart.config_mut().conflation_threshold = Some(4.0);
                assert_eq!(chart.prepare_conflation(0.01, 1.0), 1);
                assert!(chart.merge_groups.groups.is_empty());
                chart
                    .chart_options
                    .time_scale
                    .tick_mark_max_character_length = 4;
                chart.chart_options.time_scale.ticks_visible = true;
                chart.add_indicator(Box::new(
                    CustomIndicator::new("Numeric pane", |bars| {
                        bars.iter()
                            .map(|bar| IndicatorValue::Single(bar.close))
                            .collect()
                    })
                    .with_overlay(false),
                ));
                let color = egui::Color32::from_rgb(133, 77, 211);
                let secondary = chart.add_series(SeriesOptions {
                    color,
                    ..Default::default()
                });
                let ctx = egui::Context::default();
                apply_to_egui(&ctx, &theme);
                frame(&ctx, &mut chart, 0.0, vec![]);
                chart.fit_content();
                frame(&ctx, &mut chart, 0.1, vec![]);
                if keys.is_empty() {
                    assert_eq!(chart.visible_logical_range(), None);
                    assert_eq!(chart.index_to_time(0.0), None);
                    // A scale installed before data must pick up the first streamed samples.
                    for key in [100.0, 110.0, 140.0] {
                        chart.update(
                            SeriesId::MAIN,
                            Bar::from_value(key_time(key).unwrap(), 10.0),
                        );
                    }
                    let x = |i| chart.index_to_x(i).unwrap();
                    assert_close(
                        f64::from((x(2.0) - x(1.0)) / (x(1.0) - x(0.0))),
                        3.0,
                        "streamed gaps",
                    );
                    continue;
                }
                let xs: Vec<_> = (0..keys.len())
                    .map(|i| chart.index_to_x(i as f64).unwrap())
                    .collect();
                let plot = chart.get_rendered_price_rect();
                for (i, &x) in xs.iter().enumerate() {
                    assert!(plot.x_range().contains(x), "{keys:?}: {x}");
                    assert_close(chart.x_to_index(x).unwrap(), i as f64, "inverse x");
                    assert_close(
                        chart.get_chart_mapping().idx_to_x(i).into(),
                        x.into(),
                        "rendering mapping",
                    );
                    let pane = &chart.get_rendered_indicator_panes()[0];
                    assert_close(pane.mapping.idx_to_x(i).into(), x.into(), "pane mapping");
                    assert_eq!(chart.index_to_time(i as f64), key_time(keys[i]));
                    assert_eq!(chart.time_at_index(i), key_time(keys[i]));
                    assert_eq!(
                        chart.time_index_at(key_time(keys[i]).unwrap(), MismatchDirection::Exact),
                        Some(i)
                    );
                    assert_eq!(chart.time_to_x(key_time(keys[i]).unwrap()), Some(x));
                    assert_eq!(chart.x_to_time(x), key_time(keys[i]));
                }
                if keys.len() >= 3 {
                    let expected = (keys[2] - keys[1]) / (keys[1] - keys[0]);
                    assert_close(
                        f64::from((xs[2] - xs[1]) / (xs[1] - xs[0])),
                        expected,
                        "numeric gap ratio",
                    );
                }
                // A single point uses one key unit per spacing unit.
                let unit = if keys.len() == 1 {
                    1.0
                } else {
                    (keys[keys.len() - 1] - keys[0]) / (keys.len() - 1) as f64
                };
                let samples = [
                    keys[0] - 2.0 * unit,
                    keys[0] + unit / 2.0,
                    keys[keys.len() - 1] + 2.0 * unit,
                ];
                chart.set_data(
                    secondary,
                    samples
                        .iter()
                        .map(|&key| Bar::from_value(key_time(key).unwrap(), 15.0))
                        .collect(),
                );
                let range = LogicalRange::new(
                    chart.time_to_index(key_time(samples[0]).unwrap()).unwrap() as f32 - 1.0,
                    chart.time_to_index(key_time(samples[2]).unwrap()).unwrap() as f32 + 1.0,
                );
                assert!(chart.set_visible_logical_range(range));
                assert_range(&chart, range.left, range.right, "numeric bounds");
                let marker_key = keys[keys.len() / 2];
                for (id, marker_color) in [
                    (SeriesId::MAIN, egui::Color32::GREEN),
                    (secondary, egui::Color32::RED),
                ] {
                    chart.set_markers(
                        id,
                        vec![
                            Marker::new(key_time(marker_key).unwrap())
                                .with_shape(MarkerShape::Circle)
                                .with_pos(MarkerPos::InBar)
                                .with_color(marker_color),
                        ],
                    );
                }
                let painted = shapes(&ctx, &mut chart, 0.2, vec![]);
                let first = chart.index_to_x(0.0).unwrap();
                let spacing = chart.get_time_scale_state().0;
                let expected_x = |key: f64| first + ((key - keys[0]) / unit) as f32 * spacing;
                let tick_x = |key: f64| {
                    let label = scale.format_crosshair(key_time(key).unwrap(), 0);
                    let x = painted.iter().find_map(|shape| match shape {
                        egui::Shape::Text(text)
                            if text.galley.text() == label && text.pos.y >= plot.bottom() =>
                        {
                            Some(text.pos.x + text.galley.size().x / 2.0)
                        }
                        _ => None,
                    });
                    (label, x)
                };
                for &key in *keys {
                    let (label, tick) = tick_x(key);
                    if let Some(x) = tick {
                        assert_close(x.into(), expected_x(key).into(), "numeric tick position");
                    } else if keys[0] == 100.0 && keys.len() == 3 {
                        panic!("missing numeric tick {label}");
                    }
                }
                if keys[0] == 100.0 && keys.len() == 3 {
                    for key in [90.0, 150.0] {
                        let (label, x) = tick_x(key);
                        let x = x.unwrap_or_else(|| panic!("numeric whitespace tick {label}"));
                        assert_close(
                            x.into(),
                            expected_x(key).into(),
                            "numeric whitespace position",
                        );
                    }
                }
                for key in samples {
                    let index = chart.time_to_index(key_time(key).unwrap()).unwrap();
                    let actual = chart.index_to_x(index).unwrap();
                    assert_close(
                        actual.into(),
                        expected_x(key).into(),
                        "numeric edge and fractional key",
                    );
                    assert_close(
                        crate::scales::time_key(
                            chart
                                .index_to_time(chart.x_to_index(actual).unwrap())
                                .unwrap(),
                        ),
                        key,
                        "inverse numeric key",
                    );
                }
                let paths: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        egui::Shape::Path(path)
                            if path.stroke.color == egui::epaint::ColorMode::Solid(color) =>
                        {
                            Some(path)
                        }
                        _ => None,
                    })
                    .collect();
                assert!(
                    paths.iter().any(|path| samples.iter().all(|&key| path
                        .points
                        .iter()
                        .any(|p| (p.x - expected_x(key)).abs() < 0.01))),
                    "secondary curve: {paths:?}"
                );
                for color in [egui::Color32::GREEN, egui::Color32::RED] {
                    assert!(painted.iter().any(|shape| matches!(shape,
                        egui::Shape::Circle(circle) if circle.fill == color && (circle.center.x - expected_x(marker_key)).abs() < 0.01)), "marker {color:?}");
                }
                let hover = egui::pos2(expected_x(marker_key), plot.center().y);
                let hovered = shapes(
                    &ctx,
                    &mut chart,
                    0.3,
                    vec![egui::Event::PointerMoved(hover)],
                );
                let label = scale.format_crosshair(key_time(marker_key).unwrap(), 0);
                assert!(hovered.iter().any(|shape| matches!(shape, egui::Shape::Text(text) if text.galley.text() == label && text.pos.y >= plot.bottom())), "numeric crosshair {label}");
                assert_close(
                    chart.last_hover_bar_idx.unwrap(),
                    (keys.len() / 2) as f64,
                    "crosshair index",
                );
                chart.set_horizontal_scale(None);
                frame(&ctx, &mut chart, 0.4, vec![]);
                if keys.len() >= 3 {
                    let x = |i| chart.index_to_x(i).unwrap();
                    assert_close(
                        f64::from(x(2.0) - x(1.0)),
                        f64::from(x(1.0) - x(0.0)),
                        "ordinal time spacing",
                    );
                }
            }
        }
        // Both keys fit key_time, but their difference exceeds an i64 count of nanoseconds.
        // -9e9 + 0.75 * (8e9 - -9e9) = 3.75e9.
        let mut wide = Chart::new(BarData::from_bars(
            [-9e9, 8e9, 9e9]
                .into_iter()
                .map(|key| Bar::from_value(key_time(key).unwrap(), 10.0))
                .collect(),
        ));
        wide.set_horizontal_scale(Some(Arc::new(PriceHorizontalScale::default())));
        assert_eq!(wide.index_to_time(0.75), key_time(3.75e9));
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
        #[cfg(feature = "csv")]
        {
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

    /// Bars loaded between frames keep a view at the live edge there: a history page or older
    /// bars in front, newer bars it follows, and a chart type switch to many more bricks.
    #[test]
    fn bars_loaded_between_frames_keep_the_live_edge() {
        use crate::data::merge::Origin;
        use crate::model::{ChartType, ChartTypeParams};
        let flat = |time, close| Bar::new(time, close, close, close, close, 1.0);
        for (fix_right_edge, offset) in [(false, 2.5), (true, 0.0)] {
            for case in ["history page", "older bars", "newer bars", "both", "renko"] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[1.0], &[2.0]);
                let bars = chart.bars(SeriesId::MAIN).to_vec();
                let (first, last) = (bars[0].time, bars[99].time);
                let older = |count| -> Vec<Bar> {
                    (1..=count)
                        .rev()
                        .map(|day| flat(first - Duration::days(day), 1.5))
                        .collect()
                };
                let newer = |count| (1..=count).map(|day| flat(last + Duration::days(day), 1.5));
                if case == "renko" {
                    // Each bar after the first rises by 1000, which makes 100 bricks of 10 per step.
                    chart.set_data(
                        SeriesId::MAIN,
                        bars.iter()
                            .enumerate()
                            .map(|(i, bar)| flat(bar.time, 100.0 + 1000.0 * i as f64))
                            .collect(),
                    );
                }
                let mut options = chart.chart_options().clone();
                options.time_scale.fix_right_edge = fix_right_edge;
                chart.set_chart_options(options);
                frame(&ctx, &mut chart, 0.0, vec![]);
                match case {
                    "history page" => {
                        let page = older(1500).into_iter().map(|bar| (bar, Origin::History));
                        chart.apply(page.collect());
                    }
                    "older bars" => {
                        let all = older(5000).into_iter().chain(bars).collect();
                        chart.set_data(SeriesId::MAIN, all);
                    }
                    "newer bars" => {
                        let all = bars.into_iter().chain(newer(3000)).collect();
                        chart.set_data(SeriesId::MAIN, all);
                    }
                    "both" => {
                        let all = older(5000).into_iter().chain(bars).chain(newer(3000));
                        chart.set_data(SeriesId::MAIN, all.collect());
                    }
                    _ => {
                        chart.set_chart_type(ChartType::Renko);
                        chart.set_chart_type_params(ChartTypeParams {
                            renko_brick_size: 10.0,
                            ..Default::default()
                        });
                    }
                }
                frame(&ctx, &mut chart, 0.1, vec![]);
                frame(&ctx, &mut chart, 0.2, vec![]);
                assert_eq!(
                    chart.get_time_scale_state().1,
                    offset,
                    "{case}, fix_right_edge {fix_right_edge}"
                );
            }
        }
    }

    /// With a fixed right edge, every frame ends at the newest bar, one or two per frame.
    #[test]
    fn fixed_right_edge_keeps_streamed_bars_at_the_edge() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[1.0], &[2.0]);
        let first = chart.bars(SeriesId::MAIN)[0].time;
        let day = |day: i64| Bar::new(first + Duration::days(day), 1.0, 2.0, 1.0, 2.0, 1.0);
        let mut options = chart.chart_options().clone();
        options.time_scale.fix_right_edge = true;
        chart.set_chart_options(options);
        frame(&ctx, &mut chart, 0.0, vec![]);
        let mut observed = vec![chart.get_time_scale_state().1];
        for (time, days) in [(0.1, 100..101), (0.2, 101..103), (0.3, 103..104)] {
            for index in days {
                chart.update(SeriesId::MAIN, day(index));
            }
            frame(&ctx, &mut chart, time, vec![]);
            observed.push(chart.get_time_scale_state().1);
        }
        assert_eq!(observed, [0.0; 4]);
    }

    /// A view 30 bars back keeps its times when older, newer or both kinds of bars arrive in
    /// one replacement, with a fixed right edge or without one.
    #[test]
    fn bars_loaded_between_frames_keep_a_scrolled_back_view() {
        for fix_right_edge in [false, true] {
            for (older, newer) in [(5000, 0), (0, 3000), (5000, 3000)] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[1.0], &[2.0]);
                let bars = chart.bars(SeriesId::MAIN).to_vec();
                let (first, last) = (bars[0].time, bars[99].time);
                let flat = |time| Bar::new(time, 1.5, 1.5, 1.5, 1.5, 1.0);
                let mut options = chart.chart_options().clone();
                options.time_scale.fix_right_edge = fix_right_edge;
                options.time_scale.bar_spacing = 20.0;
                chart.set_chart_options(options);
                frame(&ctx, &mut chart, 0.0, vec![]);
                chart.scroll_to_position(-30.0, false);
                frame(&ctx, &mut chart, 0.1, vec![]);
                let before = chart.visible_time_range().unwrap();
                let all = (1..=older)
                    .rev()
                    .map(|day| flat(first - Duration::days(day)))
                    .chain(bars)
                    .chain((1..=newer).map(|day| flat(last + Duration::days(day))));
                chart.set_data(SeriesId::MAIN, all.collect());
                frame(&ctx, &mut chart, 0.2, vec![]);
                // The logical range is narrowed to f32, which rounds below a second here.
                let after = chart.visible_time_range().unwrap();
                assert!(
                    (after.from - before.from).abs() < Duration::seconds(1)
                        && (after.to - before.to).abs() < Duration::seconds(1),
                    "{older} older, {newer} newer, fix_right_edge {fix_right_edge}: \
                     {before:?} became {after:?}"
                );
            }
        }
    }
}
